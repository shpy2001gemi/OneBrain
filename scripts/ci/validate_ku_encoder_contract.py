#!/usr/bin/env python3
"""Offline schema, generated-artifact and KU extraction corpus conformance."""
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from scripts.encoder.contract import BUNDLE, DEFS, evaluate, assemble, typed
from scripts.encoder.generate_bundle import run
from scripts.encoder.schema import ContractError, require, strict_loads
from scripts.encoder.generate_selection_v2_schema import DEST as SELECTION_V2, schemas as selection_v2_schemas
import json


def validate_contract():
    for name, value in selection_v2_schemas().items():
        expected = json.dumps(value, ensure_ascii=False, indent=2) + '\n'
        require((SELECTION_V2 / name).read_bytes() == expected.encode('utf-8'),
                'selection_v2_schema_drift: ' + name)
    generated = run(check=True)
    profile = strict_loads((BUNDLE / 'profile.json').read_bytes())
    require(profile['profile'] == DEFS['Context']['properties']['profile']['const'] ==
            DEFS['Candidate']['properties']['profile']['const'], 'profile_version_drift')
    require(profile['production_enabled'] is False, 'contract_only')
    require(profile['roots'] == {'context': 'Context', 'candidate': 'Candidate', 'resolution': 'Resolution',
                                'attempt': 'Attempt', 'provider': 'ProviderManifest',
                                'input': 'ProviderInput'}, 'schema_roots')
    for language in ['vi', 'en']:
        require(profile['profile'] in (BUNDLE / f'prompt.{language}.txt').read_text(encoding='utf-8'),
                'prompt_version_drift')
    for example in strict_loads((BUNDLE / 'examples.json').read_bytes())['examples']:
        typed(example['candidate'], 'Candidate')
        typed(example['context'], 'ProviderInput')
    require(profile['max_context_calls'] == 2 and profile['max_parallel_calls'] == 1 and
            profile['max_chunks_per_job'] == 16 and profile['max_job_statements'] == 256,
            'oracle_profile_bounds')
    limits = DEFS['Attempt']['properties']
    standard = profile['resource_profiles']['standard']
    for field, setting in [('calls_reserved', 'job_calls'), ('input_tokens_reserved', 'job_input_tokens'),
                           ('output_tokens_reserved', 'job_output_tokens')]:
        require(limits[field]['maximum'] == standard[setting], 'attempt_budget_drift')
    require(profile['experimental_host_overrides'] == {'ollama_cpu_deadline_ms': 600000},
            'experimental_deadline_policy')
    require(standard['deadline_ms'] == 120000 and
            profile['resource_profiles']['constrained']['deadline_ms'] == 30000,
            'qualification_deadline_drift')
    require(limits['remaining_deadline_ms']['maximum'] ==
            profile['experimental_host_overrides']['ollama_cpu_deadline_ms'], 'attempt_budget_drift')
    require(set(profile['phase_base_states']) == set(limits['phase']['enum']), 'phase_map_drift')
    for bounds in profile['resource_profiles'].values():
        require(bounds['job_input_tokens'] <= bounds['job_calls'] * bounds['call_input_tokens'] and
                bounds['job_output_tokens'] <= bounds['job_calls'] * bounds['call_output_tokens'] and
                bounds['deadline_ms'] <= standard['deadline_ms'], 'aggregate_profile_bounds')
    corpus = strict_loads((BUNDLE / 'corpus.json').read_bytes())
    cases = {row['id']: row for row in corpus['cases']}
    require(len(cases) == len(corpus['cases']), 'duplicate_case')
    require({r['language'] for r in cases.values()} == {'vi', 'en'}, 'corpus_languages')
    require({'positive', 'negative', 'ambiguous', 'abstention', 'multi-chunk'} <=
            {tag for r in cases.values() for tag in r['tags']}, 'corpus_categories')
    for name, row in cases.items():
        try:
            actual = evaluate(row['context'], row['candidate'], row['resolution'])
        except ContractError as error:
            actual = {'error': str(error)}
        require(actual == row['expected'], 'corpus_oracle: ' + name)
    for job in corpus['jobs']:
        actual = assemble([cases[key] for key in job['cases']])
        require(actual['status'] == job['expected_status'], 'job_oracle: ' + job['id'])
        if 'expected_statement_count' in job:
            require(len(actual['sem']['statements']) == job['expected_statement_count'], 'job_statement_count')
    return len(cases), len(corpus['jobs']), generated


if __name__ == '__main__':
    try:
        cases, jobs, generated = validate_contract()
    except (ContractError, OSError, ValueError, KeyError, TypeError) as error:
        print(f'KU encoder contract failed: {error}', file=sys.stderr)
        raise SystemExit(1)
    print(f'KU encoder contract OK: {cases} cases, {jobs} jobs, {generated} generated artifacts')
