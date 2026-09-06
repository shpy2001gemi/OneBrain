"""Executable KU-ENC-001 corpus oracle, never a production KU compiler.

Registry authentication, custody and reviewer authority are external preconditions.
Logical SEM projections here are not canonical encodings or object CIDs.
"""
from fractions import Fraction
import hashlib
from pathlib import Path
import re
import unicodedata

from .schema import ContractError, require, strict_loads, stable_bytes, validate

ROOT = Path(__file__).resolve().parents[2]
BUNDLE = ROOT / 'docs/specs/vnext/ku-encoder-v1'
DEFS = strict_loads((BUNDLE / 'schema.json').read_bytes())['$defs']


def digest(value):
    return hashlib.sha256(stable_bytes(value)).hexdigest()


def typed(value, name):
    validate(value, DEFS[name], DEFS)


def index(rows, field='key'):
    result = {row[field]: row for row in rows}
    require(len(result) == len(rows), 'duplicate_id')
    return result


def utf8(value):
    try:
        return value.encode('utf-8')
    except UnicodeError as error:
        raise ContractError('invalid_unicode') from error


def ratio(value):
    number = Fraction(int(value['numerator']), int(value['denominator']))
    require(str(number.numerator) == value['numerator'] and
            str(number.denominator) == value['denominator'], 'noncanonical_ratio')
    return checked(number)


def checked(value):
    require(-(2**63) <= value.numerator < 2**63 and 0 < value.denominator < 2**64,
            'exact_number_overflow')
    return {'numerator': str(value.numerator), 'denominator': str(value.denominator)}


def number(text):
    require(len(text) <= 64 and re.fullmatch(r'-?(?:0|[1-9][0-9]*)(?:\.[0-9]+|/[1-9][0-9]*)?', text),
            'unsupported_number')
    return checked(Fraction(text))


def inside(span, container):
    return container['start'] <= span['start'] < span['end'] <= container['end']


def context_check(context):
    typed(context, 'Context')
    raw = utf8(context['source_text'])
    require(len(raw) <= 786432, 'source_bytes')
    require(len(stable_bytes(context)) <= 1048576, 'payload_bytes')
    windows = index(context['windows'])
    require(any(w['role'] == 'focus' for w in windows.values()), 'missing_focus')
    for window in windows.values():
        require(0 <= window['start'] < window['end'] <= len(raw), 'window_bounds')
        try:
            raw[window['start']:window['end']].decode('utf-8')
        except UnicodeError as error:
            raise ContractError('window_boundary') from error

    def span(value):
        typed(value, 'Span')
        require(0 <= value['start'] < value['end'] <= len(raw), 'span_bounds')
        require(raw[value['start']:value['end']] == utf8(value['quote']), 'span_quote')
        require(any(inside(value, window) for window in windows.values()), 'outside_context')

    units = index(context['required_units'])
    for unit in units.values():
        span(unit['span'])
        require(any(w['role'] == 'focus' and inside(unit['span'], w) for w in windows.values()),
                'unit_outside_focus')
    options = index(context['options'])
    for option in options.values():
        span(option['mention'])
        require(option['lookup_label'] == option['mention']['quote'], 'lookup_label')
        if 'unit' in option:
            ratio(option['unit']['scale'])
            ratio(option['unit']['offset'])
            require(int(option['unit']['scale']['numerator']) > 0, 'unit_scale')
    return span, units, options


def provider_view(context):
    """Only admitted windows go to a provider; whole source stays in the host."""
    context_check(context)
    raw = utf8(context['source_text'])
    view = {
        'profile': context['profile'], 'attempt_id': context['attempt_id'],
        'context_sha256': digest(context),
        'windows': [dict(w, text=raw[w['start']:w['end']].decode('utf-8'))
                    for w in context['windows']],
        'required_units': context['required_units'],
        # CCIDs and unit arithmetic are host-only; model refers to local keys.
        'options': [{key: o[key] for key in ('key', 'lookup_label', 'mention', 'description') if key in o}
                    for o in context['options']],
    }
    typed(view, 'ProviderInput')
    require(len(stable_bytes(view)) <= 1048576, 'payload_bytes')
    return view


def evaluate(context, candidate, resolution):
    span, units, options = context_check(context)
    typed(candidate, 'Candidate')
    typed(resolution, 'Resolution')
    require(len(stable_bytes(candidate)) <= 1048576, 'payload_bytes')
    for value in (candidate, resolution):
        require(value['attempt_id'] == context['attempt_id'] and
                value['context_sha256'] == digest(context), 'context_binding')
    concepts = index(candidate['concepts'])
    statements = index(candidate['statements'])
    coverage = index(candidate['coverage'], 'unit')
    bindings = index(resolution['bindings'], 'concept')
    require(set(coverage) == set(units), 'coverage_set')
    require(set(bindings) <= set(concepts), 'extraneous_binding')
    ready = True
    resolved = {}
    for key, concept in concepts.items():
        span(concept['evidence'])
        require(concept['label'] == concept['evidence']['quote'], 'concept_label')
        if 'option_proposal' in concept:
            proposal = options.get(concept['option_proposal'])
            require(proposal is not None, 'unknown_option_proposal')
            require(proposal['mention'] == concept['evidence'], 'option_mention')
        binding = bindings.get(key)
        if binding is None:
            ready = False
            continue
        require(binding['option'] in options, 'unknown_option')
        option = options[binding['option']]
        require(option['mention'] == concept['evidence'], 'option_mention')
        matches = [o for o in options.values() if o['mention'] == concept['evidence']]
        if binding['selection'] == 'exact_label':
            require(len(matches) == 1 and option['lookup_label'] == concept['label'], 'ambiguous_exact_label')
        elif binding['selection'] == 'model_proposal':
            ready = False
        resolved[key] = option

    used = set()
    edges = {key: set() for key in statements}
    normalized_ids = {key: position for position, key in enumerate(statements)}

    def concept_ref(key, evidence=None):
        require(key in concepts, 'unknown_concept')
        used.add(key)
        if evidence is not None:
            require(evidence == concepts[key]['evidence'], 'concept_mention')
        return resolved[key]['ccid'] if key in resolved else None

    def statement_ref(key, parent):
        require(key in statements, 'unknown_statement')
        edges[parent].add(key)
        return normalized_ids[key]

    def term(value, parent):
        kind = value['kind']
        mentions = [value['number'], value['unit_evidence']] if kind == 'quantity' else [value['evidence']]
        for mention in mentions:
            require(any(inside(mention, e) for e in statements[parent]['evidence']), 'term_scope')
        if kind == 'quantity':
            span(value['number'])
            span(value['unit_evidence'])
            identity = concept_ref(value['unit'], value['unit_evidence'])
            exact = number(value['number']['quote'])
            option = resolved.get(value['unit'])
            if option:
                require('unit' in option, 'not_unit')
                unit = dict(option['unit'], ccid=identity)
                n = Fraction(int(exact['numerator']), int(exact['denominator']))
                scale = unit['scale']; offset = unit['offset']
                checked(n * Fraction(int(scale['numerator']), int(scale['denominator'])) +
                        Fraction(int(offset['numerator']), int(offset['denominator'])))
            else:
                unit = None
            return {'kind': 'quantity', 'value': exact, 'unit': unit}
        span(value['evidence'])
        if kind == 'concept':
            return {'kind': kind, 'ccid': concept_ref(value['concept'], value['evidence'])}
        if kind == 'statement':
            return {'kind': kind, 'id': statement_ref(value['statement'], parent)}
        if kind == 'text':
            require(value['value'] == value['evidence']['quote'], 'literal_quote')
            require(unicodedata.normalize('NFC', value['value']) == value['value'], 'non_nfc')
        elif kind == 'boolean':
            accepted = {'true': True, 'false': False, 'đúng': True, 'sai': False}
            require(value['evidence']['quote'] in accepted and
                    accepted[value['evidence']['quote']] is value['value'], 'boolean_lexeme')
        return {'kind': kind, 'value': value['value']}

    output = []
    for key, statement in statements.items():
        require(len({(e['start'], e['end']) for e in statement['evidence']}) == len(statement['evidence']),
                'duplicate_source_span')
        for evidence in statement['evidence']:
            span(evidence)
        predicate = concept_ref(statement['predicate'])
        require(any(inside(concepts[statement['predicate']]['evidence'], e)
                    for e in statement['evidence']), 'predicate_scope')
        for name in ('negation', 'modality'):
            qualifier = statement[name]
            if (name == 'negation' and qualifier['value']) or (name == 'modality' and qualifier['value'] != 'asserted'):
                require(qualifier['evidence'], 'missing_qualifier_evidence')
            for evidence in qualifier['evidence']:
                span(evidence)
                require(any(inside(evidence, e) for e in statement['evidence']), 'qualifier_scope')
        qualifiers = {'negated': statement['negation']['value'], 'modality': statement['modality']['value'],
                      'source_spans': [{'source': context['source_ref'], 'start': e['start'], 'end': e['end']}
                                       for e in statement['evidence']]}
        if 'condition' in statement:
            span(statement['condition']['evidence'])
            require(any(inside(statement['condition']['evidence'], e) for e in statement['evidence']), 'qualifier_scope')
            qualifiers['condition'] = statement_ref(statement['condition']['statement'], key)
        for name in ('time', 'location', 'perspective', 'tolerance'):
            if name in statement:
                qualifiers[name] = term(statement[name], key)
        output.append({'id': normalized_ids[key], 'predicate': predicate,
                       'arguments': [term(t, key) for t in statement['arguments']],
                       'constraints': [], 'qualifiers': qualifiers})
    require(used == set(concepts), 'unused_concept')
    roots = set()
    for key, item in coverage.items():
        require(len(set(item['statements'])) == len(item['statements']), 'duplicate_coverage_statement')
        if item['status'] == 'represented':
            require(item['reason'] == 'none' and item['statements'], 'represented_coverage')
        else:
            require(item['reason'] != 'none', 'unresolved_reason')
            ready = False
        for ref in item['statements']:
            require(ref in statements, 'unknown_statement')
            require(any(inside(e, units[key]['span']) for e in statements[ref]['evidence']), 'coverage_scope')
            roots.add(ref)
    visited = set()

    def walk(key, active):
        require(key not in active, 'cyclic_statement')
        if key in visited:
            return
        for child in edges[key]:
            walk(child, active | {key})
        visited.add(key)
    for key in roots:
        walk(key, set())
    require(visited == set(statements), 'orphan_statement')
    if not ready or not output:
        return {'status': 'needs_resolution'}
    return {'status': 'compilable', 'sem': {'major': 1, 'minor': 0, 'statements': output}}


def attempt_check(attempt):
    typed(attempt, 'Attempt')
    phase = attempt['phase']
    if phase in ('candidate_recorded', 'resolving', 'validated', 'prepared'):
        require('candidate_sha256' in attempt, 'missing_candidate_digest')
    if phase in ('validated', 'prepared'):
        require('resolution_sha256' in attempt, 'missing_resolution_digest')
    if phase in ('failed', 'canceled', 'interrupted'):
        require(attempt['reason'] != 'none', 'terminal_reason')
    else:
        require(attempt['reason'] == 'none', 'active_reason')
    if phase in ('canceled', 'interrupted'):
        require(attempt['reason'] == phase, 'terminal_reason')


def assemble(rows):
    """Manifest-ordered, independent closed chunks; no inferred cross-chunk edges."""
    require(0 < len(rows) <= 16, 'job_chunks')
    contexts = [row['context'] for row in rows]
    require(len({c['attempt_id'] for c in contexts}) == len(contexts), 'duplicate_attempt')
    require(len({(c['source_ref'], c['source_text'], c['registry_root'], c['resource_profile'])
                 for c in contexts}) == 1, 'job_binding')
    spans = [u['span'] for c in contexts for u in c['required_units']]
    require(all(a['end'] <= b['start'] for a, b in zip(spans, spans[1:])), 'chunk_order_or_overlap')
    outputs = [evaluate(row['context'], row['candidate'], row['resolution']) for row in rows]
    if any(out['status'] != 'compilable' for out in outputs):
        return {'status': 'needs_resolution'}
    frames = []
    for output in outputs:
        offset = len(frames)
        for statement in output['sem']['statements']:
            statement['id'] += offset
            qualifiers = statement['qualifiers']
            if 'condition' in qualifiers:
                qualifiers['condition'] += offset
            for value in statement['arguments'] + [qualifiers[k] for k in ('time', 'location', 'perspective') if k in qualifiers]:
                if value['kind'] == 'statement':
                    value['id'] += offset
            frames.append(statement)
    require(len(frames) <= 256, 'job_statements')
    return {'status': 'compilable', 'sem': {'major': 1, 'minor': 0, 'statements': frames}}


def provider_check(manifest):
    typed(manifest, 'ProviderManifest')
    if manifest['mode'] == 'rules':
        require(manifest['max_context_tokens'] == 0 and 'model_artifact_sha256' not in manifest
                and 'tokenizer_sha256' not in manifest, 'rules_provider')
    else:
        require('model_artifact_sha256' in manifest and 'tokenizer_sha256' in manifest
                and manifest['max_context_tokens'] > 0, 'missing_model_identity')
    if manifest['mode'] == 'grammar':
        require('grammar_sha256' in manifest, 'missing_grammar_digest')
    else:
        require('grammar_sha256' not in manifest, 'unexpected_grammar')
    require(len(set(manifest['supported_schema_keywords'])) == len(manifest['supported_schema_keywords']),
            'duplicate_schema_capability')


class Budget:
    """Reservation arithmetic oracle. Runtime must persist/fence this in KU-ENC-002."""
    def __init__(self, profile, provider, admitted_memory_bytes):
        provider_check(provider)
        settings = strict_loads((BUNDLE / 'profile.json').read_bytes())
        require(profile in settings['resource_profiles'], 'resource_profile')
        self.limits = settings['resource_profiles'][profile]
        require(provider['peak_bytes_reservation'] <= admitted_memory_bytes, 'memory_admission')
        if profile == 'no_llm':
            require(provider['mode'] == 'rules', 'no_llm_provider')
        self.provider = provider
        self.calls = self.input_tokens = self.output_tokens = self.work = 0
        self.context_calls = {}
        self.elapsed_ms = 0
        self.closed = False
        self.pending = False

    def reserve(self, context_id, input_tokens, output_tokens, elapsed_ms, work_units=1):
        require(not self.closed and self.provider['mode'] != 'rules', 'provider_closed')
        require(not self.pending, 'provider_busy')
        require(all(type(n) is int and n >= 0 for n in (input_tokens, output_tokens, elapsed_ms, work_units)),
                'invalid_budget_amount')
        require(elapsed_ms >= self.elapsed_ms and elapsed_ms < self.limits['deadline_ms'], 'deadline')
        require(input_tokens <= self.limits['call_input_tokens'] and output_tokens <= self.limits['call_output_tokens']
                and input_tokens + output_tokens <= self.provider['max_context_tokens'], 'call_tokens')
        require(self.calls < self.limits['job_calls'] and self.context_calls.get(context_id, 0) < 2, 'call_budget')
        require(self.input_tokens + input_tokens <= self.limits['job_input_tokens']
                and self.output_tokens + output_tokens <= self.limits['job_output_tokens']
                and self.work + work_units <= 1000000, 'aggregate_budget')
        self.calls += 1
        self.context_calls[context_id] = self.context_calls.get(context_id, 0) + 1
        self.input_tokens += input_tokens
        self.output_tokens += output_tokens
        self.work += work_units
        self.elapsed_ms = elapsed_ms
        self.pending = True

    def finish_call(self, elapsed_ms, success=False):
        require(self.pending, 'no_pending_call')
        require(type(elapsed_ms) is int and elapsed_ms >= self.elapsed_ms, 'deadline')
        if success:
            self.accept_callback(elapsed_ms)
        self.pending = False
        self.elapsed_ms = elapsed_ms

    def cancel(self):
        self.closed = True

    def accept_callback(self, elapsed_ms):
        require(not self.closed and self.pending and type(elapsed_ms) is int and
                self.elapsed_ms <= elapsed_ms < self.limits['deadline_ms'], 'late_callback')
