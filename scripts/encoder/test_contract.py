"""Meaningful rejection, identity, aggregation and provider reservation probes."""
from copy import deepcopy
import json
import unittest
from unittest.mock import patch

from scripts.ci.validate_ku_encoder_contract import validate_contract
from .contract import (BUNDLE, DEFS, Budget, assemble, attempt_check, digest,
                       evaluate, number, provider_check, provider_view, typed)
from .schema import ContractError, check_schema, strict_loads, stable_bytes
from .generate_bundle import run as check_generated


class ContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.cases = {r['id']: r for r in strict_loads((BUNDLE / 'corpus.json').read_bytes())['cases']}

    def row(self, name='en-assertion'):
        return deepcopy(self.cases[name])

    def evaluate(self, row):
        return evaluate(row['context'], row['candidate'], row['resolution'])

    def provider(self, mode='json_schema'):
        return dict(profile='ku-extraction-provider/1.0', provider_id='fixture',
                    backend_build_sha256='aa'*32, mode=mode, tools_enabled=False,
                    max_context_tokens=8192, peak_bytes_reservation=1024,
                    schema_bundle_sha256='bb'*32, model_artifact_sha256='cc'*32,
                    tokenizer_sha256='dd'*32, supported_schema_keywords=['type'],
                    temperature_milli=0)

    def test_bundle_and_all_corpus_oracles(self):
        validate_contract()

    def test_generated_drift_is_rejected_without_rewriting_files(self):
        with patch('scripts.encoder.generate_bundle.generated', return_value={'candidate.schema.json': b'{}'}):
            with self.assertRaisesRegex(ContractError, 'generated_bundle_drift'):
                check_generated(check=True)

    def test_strict_json_rejects_duplicate_float_nonfinite_depth_and_utf8(self):
        for value in [b'{"a":1,"a":2}', b'1.5', b'NaN', b'Infinity', b'\xff',
                      b'['*33+b'0'+b']'*33, b'{} trailing']:
            with self.subTest(value=value), self.assertRaises(ContractError):
                strict_loads(value)
        self.assertEqual(strict_loads(b'{"a":"[\\\""}'), {'a': '["'})
        with self.assertRaisesRegex(ContractError, 'payload_bytes'):
            strict_loads(b' '*101, limit=100)

    def test_schema_unknown_keyword_remote_ref_and_boolean_offset_fail(self):
        for schema in [{'type': 'string', 'format': 'date'}, {'$ref': 'https://invalid.example/schema'}]:
            with self.assertRaises(ContractError):
                check_schema(schema, DEFS)
        row = self.row(); row['candidate']['concepts'][0]['evidence']['start'] = True
        with self.assertRaises(ContractError):
            self.evaluate(row)

    def test_context_and_attempt_replay_binding(self):
        for field in ['context_sha256', 'attempt_id']:
            row = self.row(); row['candidate'][field] = 'ff'*32
            with self.subTest(field=field), self.assertRaisesRegex(ContractError, 'context_binding'):
                self.evaluate(row)
        row = self.row(); row['context']['registry_root'] = 'ff'*32
        with self.assertRaisesRegex(ContractError, 'context_binding'):
            self.evaluate(row)

    def test_alpha_names_do_not_change_semantics(self):
        row = self.row('en-context-condition'); expected = self.evaluate(row)
        row['candidate']['statements'][0]['key'] = 'renamed'
        row['candidate']['statements'][1]['condition']['statement'] = 'renamed'
        row['candidate']['concepts'][0]['key'] = 'renamed_concept'
        row['candidate']['statements'][0]['predicate'] = 'renamed_concept'
        row['resolution']['bindings'][0]['concept'] = 'renamed_concept'
        self.assertEqual(self.evaluate(row), expected)

    def test_statement_order_and_source_provenance_remain_semantic(self):
        row = self.row('en-context-condition'); expected = self.evaluate(row)
        row['candidate']['statements'].reverse()
        changed = self.evaluate(row)
        self.assertNotEqual(changed, expected)
        self.assertEqual(changed['sem']['statements'][0]['qualifiers']['condition'], 1)
        row = self.row(); expected = self.evaluate(row)
        row['context']['source_ref'] = 'ff'*32
        for name in ['candidate', 'resolution']:
            row[name]['context_sha256'] = digest(row['context'])
        self.assertNotEqual(self.evaluate(row), expected)

    def test_provider_view_hides_unadmitted_source_and_ccids(self):
        row = self.row('chunk-0'); view = provider_view(row['context'])
        self.assertNotIn('source_text', view)
        self.assertNotIn('cold', json.dumps(view))
        self.assertNotIn('ccid', json.dumps(view))
        self.assertEqual(view['context_sha256'], digest(row['context']))

    def test_exact_numbers_and_overflow(self):
        self.assertEqual(number('273.15'), dict(numerator='5463', denominator='20'))
        self.assertEqual(number('-2/4'), dict(numerator='-1', denominator='2'))
        self.assertEqual(number('-9223372036854775808')['numerator'], '-9223372036854775808')
        for value in ['1e3', '1,5', '1/0', '01', '9223372036854775808', '1/18446744073709551616']:
            with self.subTest(value=value), self.assertRaises(ContractError):
                number(value)

    def test_grounded_quote_is_not_semantic_truth_proof(self):
        row = self.row('en-assertion')
        # Both words are grounded and legal. Swapping semantic roles remains
        # structurally legal: independent fidelity review must catch this.
        row['candidate']['statements'][0]['arguments'].reverse()
        result = self.evaluate(row)
        self.assertEqual(result['status'], 'compilable')
        self.assertNotEqual(result, row['expected'])

    def test_terms_cannot_borrow_unrelated_statement_evidence(self):
        row = self.row('en-context-condition')
        row['candidate']['statements'][1]['arguments'][0] = deepcopy(row['candidate']['statements'][0]['arguments'][0])
        with self.assertRaisesRegex(ContractError, 'term_scope'):
            self.evaluate(row)

    def test_job_complete_partial_order_and_binding(self):
        rows = [self.row('chunk-0'), self.row('chunk-1')]
        out = assemble(rows)
        self.assertEqual([s['id'] for s in out['sem']['statements']], [0, 1])
        with self.assertRaisesRegex(ContractError, 'chunk_order_or_overlap'):
            assemble(rows[::-1])
        with self.assertRaisesRegex(ContractError, 'duplicate_attempt'):
            assemble([rows[0], rows[0]])
        rows[1]['candidate']['coverage'][0].update(status='unresolved', reason='budget_exhausted')
        self.assertEqual(assemble(rows), {'status': 'needs_resolution'})
        rows[1]['context']['registry_root'] = 'ff'*32
        with self.assertRaisesRegex(ContractError, 'job_binding'):
            assemble(rows)

    def test_provider_manifest_rejects_tools_missing_identity_and_missing_grammar(self):
        for mutation in [lambda p:p.update(tools_enabled=True), lambda p:p.pop('tokenizer_sha256'),
                         lambda p:p.update(mode='grammar')]:
            provider = self.provider(); mutation(provider)
            with self.assertRaises(ContractError):
                provider_check(provider)

    def test_budget_counts_failed_calls_and_repair_in_one_total(self):
        budget = Budget('constrained', self.provider(), 1024)
        budget.reserve('a', 100, 100, 0)
        # Failure does not undo this reservation. A repair consumes slot two.
        budget.finish_call(1, success=False)
        budget.reserve('a', 100, 100, 10)
        budget.finish_call(11, success=False)
        with self.assertRaisesRegex(ContractError, 'call_budget'):
            budget.reserve('a', 100, 100, 20)
        self.assertEqual((budget.calls, budget.input_tokens), (2, 200))
        for i in range(6):
            budget.reserve(str(i), 100, 100, 20)
            budget.finish_call(20)
        with self.assertRaisesRegex(ContractError, 'call_budget'):
            budget.reserve('next', 100, 100, 30)

    def test_memory_tokens_work_and_monotonic_deadline_fail_before_charge(self):
        with self.assertRaisesRegex(ContractError, 'memory_admission'):
            Budget('constrained', self.provider(), 1023)
        budget = Budget('constrained', self.provider(), 1024)
        for args in [('x', 4097, 1, 0), ('x', 10, 10, 30000), ('x', 10, 10, 0, 1000001),
                     ('x', -1, 1, 0), ('x', True, 1, 0)]:
            with self.subTest(args=args), self.assertRaises(ContractError):
                budget.reserve(*args)
        self.assertEqual(budget.calls, 0)
        budget.reserve('x', 1, 1, 10)
        budget.finish_call(10)
        with self.assertRaisesRegex(ContractError, 'deadline'):
            budget.reserve('x', 1, 1, 9)

    def test_no_llm_cancel_and_late_callbacks(self):
        with self.assertRaisesRegex(ContractError, 'no_llm_provider'):
            Budget('no_llm', self.provider(), 1024)
        provider = self.provider('rules'); provider['max_context_tokens'] = 0
        del provider['model_artifact_sha256']; del provider['tokenizer_sha256']
        rules = Budget('no_llm', provider, 1024)
        with self.assertRaisesRegex(ContractError, 'provider_closed'):
            rules.reserve('x', 0, 0, 0)
        budget = Budget('constrained', self.provider(), 1024)
        budget.reserve('x', 1, 1, 0)
        with self.assertRaisesRegex(ContractError, 'late_callback'):
            budget.accept_callback(30000)
        budget.cancel()
        with self.assertRaisesRegex(ContractError, 'late_callback'):
            budget.accept_callback(1)
        with self.assertRaisesRegex(ContractError, 'provider_closed'):
            budget.reserve('x', 1, 1, 1)
        self.assertEqual(budget.calls, 1)

    def test_only_one_live_call_and_no_unreserved_callback(self):
        budget = Budget('constrained', self.provider(), 1024)
        with self.assertRaisesRegex(ContractError, 'late_callback'):
            budget.accept_callback(0)
        budget.reserve('a', 1, 1, 0)
        with self.assertRaisesRegex(ContractError, 'provider_busy'):
            budget.reserve('b', 1, 1, 0)
        budget.finish_call(1, success=True)
        with self.assertRaisesRegex(ContractError, 'late_callback'):
            budget.accept_callback(2)

    def test_attempt_phase_requires_durable_dependencies(self):
        fields = DEFS['Attempt']['required']
        attempt = {key:'aa'*32 for key in fields}
        attempt.update(profile='ku-extraction-attempt/1.0', phase='prepared', reason='none',
                       calls_reserved=1, input_tokens_reserved=100, output_tokens_reserved=100,
                       work_units_charged=1000, remaining_deadline_ms=100)
        with self.assertRaisesRegex(ContractError, 'missing_candidate_digest'):
            attempt_check(attempt)
        attempt['candidate_sha256'] = 'bb'*32
        with self.assertRaisesRegex(ContractError, 'missing_resolution_digest'):
            attempt_check(attempt)
        attempt['resolution_sha256'] = 'cc'*32
        attempt_check(attempt)
        attempt.update(phase='canceled')
        with self.assertRaisesRegex(ContractError, 'terminal_reason'):
            attempt_check(attempt)


if __name__ == '__main__':
    unittest.main()
