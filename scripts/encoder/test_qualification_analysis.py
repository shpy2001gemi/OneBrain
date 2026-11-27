"""Operational reducer regressions on synthetic rows only."""
import copy
import unittest

from scripts.encoder import qualification_attempt_ledger as ledger
from scripts.encoder.qualification_analysis import nearest_rank, reduce_operations


class AnalysisTests(unittest.TestCase):
    def setUp(self):
        h = ledger.sha256
        self.plan = {"kind": "ku-enc-003-attempt-plan/1", "package_sha256": h(b"package"),
                     "profile_sha256": h(ledger.PROFILE_BYTES), "repeats": 3,
                     "sources": [{"source_id_sha256": h(b"source"), "utf8_sha256": h(b"text"), "language": "vi"}],
                     "tuples": [{"tuple_sha256": h(b"tuple"), "resource_profile": "standard"}]}
        rows = [{"id": key, "binding": ledger.encode(binding), "state": "reported_complete",
                 "calls": 1, "input_tokens": 10, "output_tokens": 20, "work": 5, "elapsed_ms": 100}
                for key, binding in ledger.plan_rows(self.plan)]
        self.snapshot = {"kind": "ku-enc-003-attempt-ledger-snapshot/1",
                         "plan_sha256": h(ledger.encode(self.plan).encode()), "planned_rows": 3, "rows": rows}

    def test_failure_remains_one_source_three_repeats(self):
        self.snapshot["rows"][2].update(state="timeout", elapsed_ms=120000)
        group = reduce_operations(self.plan, self.snapshot)["groups"][0]
        self.assertEqual((group["source_rows"], group["planned_repeat_rows"]), (1, 3))
        self.assertEqual(group["sources_all_repeats_reported_complete"], 0)
        self.assertEqual(group["sources_with_failure_or_unfinished_repeat"], 1)
        self.assertEqual(group["charged"]["calls"], 3)
        self.assertEqual(group["all_terminal_latency_ms"]["p95"], 120000)

    def test_omitted_repeat_rejected_not_silent_filter(self):
        self.snapshot["rows"].pop()
        with self.assertRaisesRegex(ValueError, "rows_missing"):
            reduce_operations(self.plan, self.snapshot)

    def test_duplicate_repeat_rejected(self):
        self.snapshot["rows"][2] = copy.deepcopy(self.snapshot["rows"][0])
        with self.assertRaisesRegex(ValueError, "duplicate_or_unknown"):
            reduce_operations(self.plan, self.snapshot)

    def test_no_quality_inferred_from_all_complete(self):
        result = reduce_operations(self.plan, self.snapshot)
        group = result["groups"][0]
        self.assertIsNone(group["semantic_recall"])
        self.assertIsNone(group["independent_source_count"])
        self.assertFalse(group["latency_gate_evaluated"])
        self.assertFalse(result["model_qualified"])

    def test_pending_rows_preserved_without_zero_latency(self):
        for row in self.snapshot["rows"]:
            row.update(state="pending", elapsed_ms=None, calls=0)
        group = reduce_operations(self.plan, self.snapshot)["groups"][0]
        self.assertEqual(group["outcomes"], {"pending": 3})
        self.assertIsNone(group["all_terminal_latency_ms"]["p95"])

    def test_restart_unknown_elapsed_not_zero_or_success(self):
        self.snapshot["rows"][0].update(state="interrupted", elapsed_ms=None)
        group = reduce_operations(self.plan, self.snapshot)["groups"][0]
        self.assertEqual(group["missing_terminal_latency_rows"], 1)
        self.assertEqual(group["planned_repeat_rows"], 3)

    def test_altered_source_or_package_rejected(self):
        self.snapshot["rows"][0]["binding"] = "{}"
        with self.assertRaisesRegex(ValueError, "binding_mismatch"):
            reduce_operations(self.plan, self.snapshot)

    def test_nearest_rank_is_declared_and_deterministic(self):
        self.assertIsNone(nearest_rank([], 95))
        self.assertEqual(nearest_rank(list(range(1, 101)), 95), 95)
        self.assertEqual(nearest_rank([30, 10, 20], 50), 20)


if __name__ == "__main__":
    unittest.main()
