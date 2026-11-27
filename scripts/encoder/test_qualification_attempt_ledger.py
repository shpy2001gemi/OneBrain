"""Synthetic scheduling tests, never qualification or provider evidence."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from scripts.encoder import qualification_attempt_ledger as ledger


def digest(value):
    return ledger.sha256(value.encode())


class AttemptLedgerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name).resolve()
        self.path = self.base / "matrix.sqlite"
        self.plan = {
            "kind": "ku-enc-003-attempt-plan/1", "package_sha256": digest("synthetic-package"),
            "profile_sha256": ledger.sha256(ledger.PROFILE_BYTES), "repeats": 3,
            "sources": [{"source_id_sha256": digest(s), "utf8_sha256": digest(s + "bytes"), "language": s}
                        for s in ("vi", "en")],
            "tuples": [{"tuple_sha256": digest("synthetic-tuple"), "resource_profile": "constrained"}],
        }
        self.db = ledger.AttemptLedger.create(self.path, self.plan)
        self.addCleanup(self.db.close)
        self.now = 1000000
        self.db.clock = lambda: self.now
        self.ids = [key for key, _ in ledger.plan_rows(self.plan)]
        self.generation, self.context = digest("generation"), digest("context")

    def start(self, index=0):
        self.db.start(self.ids[index], self.generation)

    def reserve(self, index=0, kind="extract"):
        return self.db.reserve_call(self.ids[index], self.generation, self.context, 4096, 2048, kind)

    def finish(self, outcome="failed"):
        return self.db.finish(self.ids[0], self.generation, outcome, digest("evidence"))

    def test_all_rows_durable_before_any_dispatch(self):
        snapshot = self.db.snapshot()
        self.assertEqual(snapshot["planned_rows"], 6)
        self.assertEqual({r["state"] for r in snapshot["rows"]}, {"pending"})
        self.assertFalse(snapshot["execution_admitted"])
        self.assertFalse(snapshot["model_qualified"])

    def test_failure_keeps_charged_reservations(self):
        self.start()
        self.reserve()
        self.db.charge_work(self.ids[0], self.generation, 41)
        self.finish("oom")
        row = self.db.row(self.ids[0])
        self.assertEqual((row["calls"], row["input_tokens"], row["output_tokens"], row["work"]), (1, 4096, 2048, 41))
        self.assertEqual(self.db.snapshot()["planned_rows"], 6)

    def test_no_resampling_after_terminal_failure(self):
        self.start()
        self.finish("canceled")
        with self.assertRaisesRegex(ValueError, "already_started"):
            self.start()
        with self.assertRaisesRegex(ValueError, "stale_or_terminal"):
            self.reserve()

    def test_late_completion_records_timeout(self):
        self.start()
        self.now += 30_000_000_000
        self.assertEqual(self.finish("reported_complete"), "timeout")

    def test_deadline_blocks_reservation(self):
        self.start()
        self.now += 30_000_000_000
        with self.assertRaisesRegex(ValueError, "deadline_exhausted"):
            self.reserve()
        self.assertEqual(self.db.row(self.ids[0])["calls"], 0)

    def test_context_repair_shares_two_call_cap(self):
        self.start()
        for seq in (1, 2):
            self.assertEqual(self.reserve(kind="repair_or_disambiguate"), seq)
            self.db.call_returned(self.ids[0], self.generation, seq)
        with self.assertRaisesRegex(ValueError, "call_budget"):
            self.reserve()

    def test_one_outstanding_call(self):
        self.start()
        self.reserve()
        with self.assertRaisesRegex(ValueError, "call_already_active"):
            self.reserve()

    def test_completion_before_call_return_rejected(self):
        self.start()
        self.reserve()
        with self.assertRaisesRegex(ValueError, "completion_before_call_return"):
            self.finish("reported_complete")
        self.db.call_returned(self.ids[0], self.generation, 1)
        self.assertEqual(self.finish("reported_complete"), "reported_complete")

    def test_terminal_requires_stop_receipt_before_next_row(self):
        self.start()
        self.finish("process_death")
        with self.assertRaisesRegex(ValueError, "stop_confirmation"):
            self.start(1)
        self.db.confirm_worker_stopped(self.ids[0], digest("collector-receipt"))
        self.start(1)

    def test_conflicting_stop_receipt_rejected(self):
        self.start()
        self.finish()
        self.db.confirm_worker_stopped(self.ids[0], digest("receipt"))
        with self.assertRaisesRegex(ValueError, "receipt_conflict"):
            self.db.confirm_worker_stopped(self.ids[0], digest("different"))

    def test_restart_interrupts_and_never_refunds(self):
        self.start()
        self.reserve()
        other = ledger.AttemptLedger(self.path, self.db.plan_digest)
        self.addCleanup(other.close)
        self.assertEqual(other.recover(digest("new-process")), 1)
        row = other.row(self.ids[0])
        self.assertEqual((row["state"], row["calls"], row["elapsed_ms"]), ("interrupted", 1, None))
        with self.assertRaisesRegex(ValueError, "stale_or_terminal"):
            self.finish("reported_complete")

    def test_same_process_cannot_claim_restart(self):
        self.start()
        with self.assertRaisesRegex(ValueError, "fresh_generation"):
            self.db.recover(self.generation)

    def test_concurrent_connection_cannot_start_second_worker(self):
        other = ledger.AttemptLedger(self.path, self.db.plan_digest)
        self.addCleanup(other.close)
        self.start()
        with self.assertRaisesRegex(ValueError, "stop_confirmation"):
            other.start(self.ids[1], digest("second"))

    def test_work_overrun_rolls_back(self):
        self.start()
        self.db.charge_work(self.ids[0], self.generation, 999999)
        with self.assertRaisesRegex(ValueError, "work_budget"):
            self.db.charge_work(self.ids[0], self.generation, 2)
        self.assertEqual(self.db.row(self.ids[0])["work"], 999999)

    def test_negative_and_boolean_counters_rejected(self):
        self.start()
        for bad in (-1, True, 1.5):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                self.db.reserve_call(self.ids[0], self.generation, self.context, bad, 1)

    def test_no_llm_cannot_reserve_model_call(self):
        plan = copy.deepcopy(self.plan)
        plan["tuples"][0]["resource_profile"] = "no_llm"
        db = ledger.AttemptLedger.create(self.base / "no-llm.sqlite", plan)
        self.addCleanup(db.close)
        row = ledger.plan_rows(plan)[0][0]
        db.start(row, self.generation)
        with self.assertRaisesRegex(ValueError, "call_tokens"):
            db.reserve_call(row, self.generation, self.context, 1, 1)

    def test_wrong_profile_and_experimental_lane_rejected(self):
        for key, value in (("profile_sha256", digest("wrong")), ("repeats", 1)):
            plan = copy.deepcopy(self.plan)
            plan[key] = value
            with self.assertRaises(ValueError):
                ledger.plan_rows(plan)
        plan = copy.deepcopy(self.plan)
        plan["tuples"][0]["resource_profile"] = "experimental_600s"
        with self.assertRaises(ValueError):
            ledger.plan_rows(plan)

    def test_source_deletion_detected(self):
        self.db.db.execute("DELETE FROM rows WHERE id=?", (self.ids[0],))
        with self.assertRaisesRegex(ValueError, "matrix_mismatch"):
            self.db.snapshot()

    def test_duplicate_source_and_label_fields_rejected(self):
        plan = copy.deepcopy(self.plan)
        plan["sources"].append(plan["sources"][0])
        with self.assertRaises(ValueError):
            ledger.plan_rows(plan)
        plan = copy.deepcopy(self.plan)
        plan["sources"][0]["label"] = "PRIVATE_LABEL_SENTINEL"
        with self.assertRaisesRegex(ValueError, "source_fields"):
            ledger.plan_rows(plan)

    def test_existing_ledger_not_overwritten(self):
        with self.assertRaises(FileExistsError):
            ledger.AttemptLedger.create(self.path, self.plan)
        self.assertEqual(self.db.snapshot()["planned_rows"], 6)

    def test_wrong_plan_digest_fails_before_mutation(self):
        other = ledger.AttemptLedger(self.path, digest("wrong-plan"))
        self.addCleanup(other.close)
        with self.assertRaisesRegex(ValueError, "plan_mismatch"):
            other.start(self.ids[0], self.generation)

    def test_real_process_exit_keeps_committed_reservation(self):
        code = """import os,sys
from scripts.encoder.qualification_attempt_ledger import AttemptLedger
db=AttemptLedger(sys.argv[1],sys.argv[2])
db.start(sys.argv[3],sys.argv[4])
db.reserve_call(sys.argv[3],sys.argv[4],sys.argv[5],4096,2048)
os._exit(73)
"""
        result = subprocess.run([sys.executable, "-c", code, str(self.path), self.db.plan_digest,
                                 self.ids[0], self.generation, self.context], capture_output=True)
        self.assertEqual(result.returncode, 73)
        self.assertEqual(self.db.recover(digest("restart-after-exit")), 1)
        row = self.db.row(self.ids[0])
        self.assertEqual((row["state"], row["calls"], row["input_tokens"]), ("interrupted", 1, 4096))


if __name__ == "__main__":
    unittest.main()
