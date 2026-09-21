"""Private development probe for the staged review-draft profile.

No KU authority or runtime storage. Reports retain approved development source,
requests, responses and draft revisions. Use an existing local Ollama server.
"""
import argparse
import copy
import hashlib
import json
import re
import time
import urllib.request
from pathlib import Path

import jsonschema

from .probe_review_proposal import NoRedirect, anchors, check_proposal, parse_proposal

ASSETS = Path(__file__).resolve().parents[2] / 'docs/specs/vnext/ku-review-draft-v1'
ROLES = ('frequency', 'negation', 'condition', 'time', 'location', 'modality', 'approximation')
SCHEMA = json.loads((ASSETS / 'draft.schema.json').read_text(encoding='utf-8'))
REVIEW_SCHEMA = json.loads((ASSETS / 'review.schema.json').read_text(encoding='utf-8'))
NUMBER_SCHEMA = json.loads((ASSETS / 'numbers.schema.json').read_text(encoding='utf-8'))
LEGACY_SCHEMA = json.loads((Path(__file__).parent / 'review_proposal.schema.json').read_text(encoding='utf-8'))
# The adapter flattens seven individually bounded role arrays.
LEGACY_SCHEMA['properties']['statements']['items']['properties']['qualifiers']['maxItems'] = 112


def constrained_schema(base, data):
    """Only quote strings are constrained, not semantic role choices or reasons."""
    source = data['SOURCE']
    # Explicit bound: longer windows keep the closed schema and host validation.
    tokens = list(re.finditer(r'\w+|[^\w\s]', source, re.UNICODE))
    if len(tokens) > 48:
        return base
    quotes = {source[a.start():b.end()] for i,a in enumerate(tokens) for b in tokens[i:]}
    quotes |= {m.group() for m in re.finditer(r'-?[0-9]+(?:\.[0-9]+|/[1-9][0-9]*)?|[^\W\d_]+', source)}
    quotes = sorted(quotes)
    numeric = sorted({m.group() for m in re.finditer(r'-?[0-9]+(?:\.[0-9]+|/[1-9][0-9]*)?', source)})
    if sum(len(q.encode('utf-8')) for q in quotes) > 16384:
        return base
    before = {''}
    for claim in data.get('DRAFT', {}).get('statements', []):
        before |= {claim['subject'], claim['predicate'], *claim['arguments']}
        before |= {q for role in ROLES for q in claim[role]}
    result = copy.deepcopy(base)
    quote_fields = {'subject','predicate','evidence','quote','source_quote','after',
                    'value_quote','unit_quote','counted_entity_quote'}
    def visit(schema, name=''):
        if name == 'numbers' and schema.get('type') == 'array' and not numeric:
            schema['maxItems'] = 0
        if name == 'statement' and schema.get('type') == 'integer' and data.get('DRAFT', {}).get('statements'):
            schema['maximum'] = len(data['DRAFT']['statements'])-1
        if schema.get('type') == 'string' and (name in quote_fields or name in ROLES or name in ('arguments','missing','before')):
            values = sorted(before) if name == 'before' else quotes
            if not schema.get('minLength') and '' not in values:
                values = ['', *values]
            schema['enum'] = [v for v in values if len(v) <= schema.get('maxLength', 8192)]
            if name == 'value_quote' and numeric:
                schema['enum'] = numeric
        for key, child in schema.get('properties', {}).items():
            visit(child, key)
        if 'items' in schema:
            visit(schema['items'], name)
    visit(result)
    return result


def validate(source, draft):
    errors = [f'schema: {e.json_path}: {e.validator}' for e in
              jsonschema.Draft202012Validator(SCHEMA).iter_errors(draft)][:8]
    if errors:
        return errors, []
    legacy = copy.deepcopy(draft)
    for claim in legacy['statements']:
        claim['qualifiers'] = [{'kind': role, 'quote': quote}
                               for role in ROLES for quote in claim.pop(role)]
    errors, _ = check_proposal(source, legacy, LEGACY_SCHEMA)
    # Evidence provides scope; only assigned roles count toward role coverage.
    covered = bytearray(len(source.encode('utf-8')))
    for claim in draft['statements']:
        evidence = claim['evidence']
        mentions = anchors(source, evidence)
        if len(mentions) != 1:
            errors.append('grounding: ambiguous proposition evidence needs review')
            continue
        quotes = [claim['subject'], claim['predicate'], *claim['arguments']]
        quotes += [q for role in ROLES for q in claim[role]]
        quotes += [q for n in claim['numbers'] for q in n.values()]
        for quote in filter(None, quotes):
            for match in anchors(evidence, quote):
                start = mentions[0]['start'] + match['start']
                end = mentions[0]['start'] + match['end']
                covered[start:end] = b'\x01' * (end-start)
        for rel in claim['relations']:
            matches = anchors(source, rel['quote'])
            if len(matches) == 1:
                start, end = matches[0]['start'], matches[0]['end']
                covered[start:end] = b'\x01' * (end-start)
    missing, current, byte = [], '', 0
    for char in source:
        size = len(char.encode('utf-8'))
        assigned = all(covered[byte:byte+size])
        if assigned:
            if any(c.isalnum() for c in current):
                missing.append(current.strip())
            current = ''
        else:
            current += char
        byte += size
    if any(c.isalnum() for c in current):
        missing.append(current.strip())
    return errors[:8], missing[:16]


def apply_edits(source, draft, review):
    """Atomic, revision-local draft edits; no arbitrary paths or KU promotion."""
    if list(jsonschema.Draft202012Validator(REVIEW_SCHEMA).iter_errors(review)):
        raise ValueError('review_schema')
    result = copy.deepcopy(draft)
    used = set()
    for edit in review['edits']:
        index, field = edit['statement'], edit['field']
        if index >= len(draft['statements']):
            raise ValueError('edit_statement')
        old, new = edit['before'], edit['after']
        key = (index, field, old)
        claim = result['statements'][index]
        if not old and not new and field not in ('subject', 'predicate'):
            continue
        if old == new:
            present = claim[field] == old if field in ('subject','predicate') else claim[field].count(old) == 1
            if not present:
                raise ValueError('edit_precondition')
            continue
        if key in used:
            raise ValueError('edit_conflict')
        used.add(key)
        if new and new not in claim['evidence']:
            raise ValueError('edit_quote')
        if new and not anchors(source, new):
            raise ValueError('edit_quote')
        if field in ('subject', 'predicate'):
            if claim[field] != old or not new:
                raise ValueError('edit_precondition')
            claim[field] = new
        else:
            values = claim[field]
            if old:
                if values.count(old) != 1:
                    raise ValueError('edit_precondition')
                position = values.index(old)
                if new:
                    values[position] = new
                else:
                    values.pop(position)
            elif new and new not in values:
                values.append(new)
            else:
                raise ValueError('edit_precondition')
    _, before_missing = validate(source, draft)
    errors, missing = validate(source, result)
    def positions(quotes):
        return {byte for q in quotes for m in anchors(source, q)
                for byte in range(m['start'], m['end'])
                if source.encode('utf-8')[byte:byte+1] not in b' \t\r\n.,;:!?'}
    if errors or not positions(missing).issubset(positions(before_missing)):
        raise ValueError('edit_revalidation')
    for entry in review['unresolved']:
        if not anchors(source, entry['quote']):
            raise ValueError('review_quote')
    if any(not anchors(source, quote) for quote in review['missing']):
        raise ValueError('review_quote')
    return result


class LocalClient:
    def __init__(self, port, model, template):
        self.url = f'http://127.0.0.1:{port}'
        self.model, self.template = model, template
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
        with self.opener.open(self.url+'/api/tags', timeout=5) as r:
            models = json.loads(r.read(1048576))['models']
        self.record = next((m for m in models if m['name'] == model), None)
        if self.record is None:
            raise ValueError('model_not_installed')

    def call(self, stage, data, remaining):
        system = (ASSETS / f'{stage}.vi.txt').read_text(encoding='utf-8')
        schema = constrained_schema({'draft': SCHEMA, 'review': REVIEW_SCHEMA, 'numbers': NUMBER_SCHEMA}[stage], data)
        content = json.dumps(data, ensure_ascii=False, separators=(',', ':'))
        body = {'model': self.model, 'stream': False, 'format': schema, 'keep_alive': 0,
                'options': {'num_ctx': 8192, 'num_predict': 2048,
                            'temperature': 0, 'seed': 1, 'num_gpu': 0}}
        if self.template == 'qwen3-raw':
            path = '/api/generate'
            body.update(raw=True, prompt=f'<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{content}<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n')
        else:
            path = '/api/chat'
            body['messages'] = [{'role': 'system', 'content': system}, {'role': 'user', 'content': content}]
        started = time.monotonic()
        req = urllib.request.Request(self.url+path, data=json.dumps(body, ensure_ascii=False).encode(),
                                     headers={'Content-Type': 'application/json'})
        with self.opener.open(req, timeout=min(600, remaining)) as r:
            raw = r.read(1048577)
        if len(raw) > 1048576:
            raise ValueError('response_size')
        response = json.loads(raw)
        output = response.get('response', response.get('message', {}).get('content', ''))
        trace = {'stage': stage, 'request': body, 'raw_response': output,
                 'seconds': round(time.monotonic()-started, 2),
                 'prompt_tokens': response.get('prompt_eval_count'),
                 'output_tokens': response.get('eval_count'), 'done_reason': response.get('done_reason')}
        if response.get('done') is not True or response.get('done_reason') != 'stop':
            trace['error'] = 'incomplete_generation'
        return trace


def run_case(client, case, emit):
    started = time.monotonic()
    calls, revisions, issues, missing = [], [], [], []
    draft = None
    def call(stage, data):
        remaining = 1800-(time.monotonic()-started)
        if remaining <= 0 or len(calls) >= 5:
            raise ValueError('budget')
        emit({'stage': stage, 'call': len(calls)+1})
        trace = client.call(stage, data, remaining)
        calls.append(trace)
        emit({'stage': stage, 'completed': True, 'seconds': trace['seconds']})
        if trace.get('error'):
            raise ValueError(trace['error'])
        return parse_proposal(trace['raw_response'])
    state = 'needs_review'
    try:
        draft = call('draft', {'SOURCE': case['text']})
        issues, missing = validate(case['text'], draft)
        revisions.append(copy.deepcopy(draft))
        if any(e.startswith('schema:') for e in issues):
            raise ValueError('draft_schema')
        required = sorted(m.group() for m in re.finditer(r'-?[0-9]+(?:\.[0-9]+|/[1-9][0-9]*)?', case['text']))
        extracted = [n for c in draft['statements'] for n in c['numbers']]
        number_incomplete = sorted(n['value_quote'] for n in extracted) != required or any(bool(n['unit_quote']) == bool(n['counted_entity_quote']) for n in extracted)
        if required and number_incomplete:
            numbers = call('numbers', {'SOURCE': case['text'], 'DRAFT': draft})
            jsonschema.validate(numbers, NUMBER_SCHEMA)
            revised = copy.deepcopy(draft)
            for claim in revised['statements']:
                claim['numbers'] = []
            for number in numbers['numbers']:
                index = number['statement']
                if index >= len(revised['statements']):
                    raise ValueError('number_statement')
                revised['statements'][index]['numbers'].append({k:v for k,v in number.items() if k != 'statement'})
            next_errors, next_missing = validate(case['text'], revised)
            if any(e not in issues for e in next_errors) or any(q not in missing for q in next_missing):
                raise ValueError('number_revalidation')
            # This task may refine types but cannot silently drop an extracted number.
            for old, new in zip(draft['statements'], revised['statements']):
                if any(n['value_quote'] not in [m['value_quote'] for m in new['numbers']] for n in old['numbers']):
                    raise ValueError('number_omission')
            draft = revised
            revisions.append(copy.deepcopy(draft))
            issues, missing = next_errors, next_missing
        for _ in range(2):
            review = call('review', {'SOURCE': case['text'], 'DRAFT': draft,
                                     'HOST_ISSUES': issues, 'UNCOVERED_ROLE_TEXT': missing})
            changed = apply_edits(case['text'], draft, review)
            if changed != draft:
                draft = changed
                revisions.append(copy.deepcopy(draft))
                issues, missing = validate(case['text'], draft)
                continue
            if not issues and not missing and not draft['unresolved'] and not review['missing'] and not review['unresolved']:
                state = 'draft_ready'
            else:
                issues += ['review_requires_attention']
            break
    except (ValueError, OSError, jsonschema.ValidationError) as error:
        issues.append(str(error) if isinstance(error, ValueError) else type(error).__name__)
    return {'case': case, 'model': client.model, 'model_record': client.record,
            'template': client.template, 'seconds': round(time.monotonic()-started, 2),
            'status': state, 'issues': issues, 'uncovered_role_text': missing,
            'draft': draft, 'revisions': revisions, 'calls': calls,
            'canonical_ku': False, 'factual_verification': 'unassessed'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--development-inputs', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--port', type=int, required=True)
    parser.add_argument('--model', required=True)
    parser.add_argument('--template', choices=['qwen3-raw', 'ollama-chat'], required=True)
    args = parser.parse_args()
    if not 1024 <= args.port <= 65535:
        parser.error('invalid port')
    cases = json.loads(args.development_inputs.read_text(encoding='utf-8'))
    if not 1 <= len(cases) <= 16 or any(not 0 < len(c['text'].encode()) <= 8192 for c in cases):
        parser.error('bounded development cases required')
    client = LocalClient(args.port, args.model, args.template)
    commitment = hashlib.sha256(b''.join(p.read_bytes() for p in sorted(ASSETS.glob('*')))).hexdigest()
    with args.report.open('x', encoding='utf-8') as report:
        for case in cases:
            def emit(event):
                print(json.dumps({'case': case['id'], **event}), flush=True)
            row = run_case(client, case, emit)
            row['assets_sha256'] = commitment
            report.write(json.dumps(row, ensure_ascii=False)+'\n')
            report.flush()
            emit({'status': row['status'], 'seconds': row['seconds'], 'issues': row['issues']})


if __name__ == '__main__':
    main()
