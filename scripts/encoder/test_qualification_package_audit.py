"""Synthetic offline preparation regressions, not model/holdout evidence."""
import copy
import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from scripts.encoder.qualification_package_audit import (
    BUNDLE, MAX_JSON, ROOT, audit, main, sha256, strict_json, verify_pin, write_report,
)


class PreparationAuditTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name).resolve()
        self.root = self.base / "repo"
        self.private = self.base / "private"
        self.private.mkdir()
        bundle = self.root / BUNDLE
        bundle.mkdir(parents=True)
        profile = (ROOT / BUNDLE / "profile.json").read_bytes()
        (bundle / "profile.json").write_bytes(profile)
        self.dump(bundle / "bundle.manifest.json", {
            "hash_algorithm": "sha256", "artifacts": {
                (BUNDLE / "profile.json").as_posix(): sha256(profile),
            },
        })
        books = []
        for language in ("vi", "en"):
            path = self.private / (language + ".xlsx")
            path.write_bytes(("synthetic opaque bytes " + language).encode())
            ids = [sha256(f"{language}-{i}".encode()) for i in range(100)]
            books.append({**self.pin(path), "source_count": 100, "label_count": 100,
                          "source_commitments": [{"source_id_sha256": v,
                              "source_text_sha256": sha256((v + "text").encode())} for v in ids],
                          "label_commitments": [{"source_id_sha256": v,
                              "row_sha256": sha256((v + "label").encode())} for v in ids],
                          "duplicate_source_ids": 0, "duplicate_label_ids": 0,
                          "source_label_ids_match": True, "declared_groups": 100,
                          "nfc_exact_duplicate_sources": 0, "nfc_exact_development_overlap": 0,
                          "language_counts": {language: 100}, "split_counts": {"holdout": 100},
                          "reviewer_fields_missing": 100})
        self.intake = {"kind": "private_intake_inventory_not_run_lock",
                       "model_qualified": False, "inference_calls": 0, "workbooks": books,
                       "development_exclusion_inventory": [self.artifact("dev", "development") ]}
        self.intake_path = self.private / "intake.json"
        self.draft = {"kind": "qualification_run_preparation_draft", "locked": False,
                      "model_qualified": False, "existing_host_reused_or_modified": False,
                      "candidate_git_head": "a" * 40,
                      "contract_inputs": [self.pin(bundle / n) for n in
                                          ("profile.json", "bundle.manifest.json")],
                      "existing_host_config": self.artifact("host", "DO_NOT_PRINT_TOKEN"),
                      "registry_metadata_candidates": [self.artifact("registry", "metadata")],
                      "reviewer_acknowledgement": None, "source_grants": None,
                      "harness_build": None, "model_tuples": None, "analysis_lock": None}
        self.draft_path = self.private / "draft.json"
        self.save()

    @staticmethod
    def dump(path, value):
        path.write_text(json.dumps(value), encoding="utf-8")

    @staticmethod
    def pin(path):
        raw = path.read_bytes()
        return {"path": str(path), "bytes": len(raw), "sha256": sha256(raw)}

    def artifact(self, name, value):
        path = self.private / name
        path.write_text(value, encoding="utf-8")
        return self.pin(path)

    def save(self):
        self.dump(self.intake_path, self.intake)
        self.draft["intake"] = self.pin(self.intake_path)
        self.dump(self.draft_path, self.draft)

    def run_audit(self):
        return audit(self.draft_path, [self.private], self.root)

    def test_valid_inventory_is_still_blocked_without_network_or_process(self):
        with patch("socket.socket", side_effect=AssertionError("network forbidden")), \
                patch("subprocess.Popen", side_effect=AssertionError("process forbidden")):
            report = self.run_audit()
        self.assertFalse(report["execution_admitted"])
        self.assertFalse(report["model_qualified"])
        self.assertEqual(report["inference_calls"], 0)
        self.assertEqual(report["missing_reviewer_fields"], 200)
        self.assertEqual(report["planned_rows_per_tuple_before_additional_sets"], 600)
        self.assertEqual(report["attempt_rows_created"], 0)
        rendered = json.dumps(report)
        self.assertNotIn("DO_NOT_PRINT_TOKEN", rendered)
        self.assertNotIn(str(self.private), rendered)
        self.assertEqual(report["thresholds"], json.loads(
            (ROOT / BUNDLE / "profile.json").read_text())["qualification"])

    def test_fabricated_lock_is_rejected(self):
        for field in ("locked", "model_qualified"):
            with self.subTest(field=field):
                self.draft[field] = True
                self.save()
                with self.assertRaisesRegex(ValueError, "draft_cannot_assert"):
                    self.run_audit()
                self.draft[field] = False

    def test_populated_review_fields_cannot_self_authorize(self):
        for field in ("reviewer_acknowledgement", "source_grants", "harness_build",
                      "model_tuples", "analysis_lock"):
            self.draft[field] = {"verified": True}
        self.save()
        report = self.run_audit()
        self.assertFalse(report["execution_admitted"])
        self.assertIn("reviewer_acknowledgement_unverified", report["blockers"])

    def test_same_size_workbook_change_rejected(self):
        path = Path(self.intake["workbooks"][0]["path"])
        path.write_bytes(b"X" * path.stat().st_size)
        with self.assertRaisesRegex(ValueError, "artifact_digest_mismatch"):
            self.run_audit()

    def test_truncated_artifact_rejected(self):
        Path(self.intake["workbooks"][0]["path"]).write_bytes(b"X")
        with self.assertRaisesRegex(ValueError, "artifact_size_mismatch"):
            self.run_audit()

    def test_stale_intake_commitment_rejected(self):
        self.intake_path.write_text("{}")
        with self.assertRaises(ValueError):
            self.run_audit()

    def test_contract_drift_rejected(self):
        (self.root / BUNDLE / "profile.json").write_text("{}")
        with self.assertRaises(ValueError):
            self.run_audit()

    def test_source_label_mismatch_rejected_even_with_matching_counts(self):
        self.intake["workbooks"][0]["label_commitments"][0]["source_id_sha256"] = "0" * 64
        self.save()
        with self.assertRaisesRegex(ValueError, "source_label_commitments_mismatch"):
            self.run_audit()

    def test_duplicate_source_id_rejected(self):
        rows = self.intake["workbooks"][0]["source_commitments"]
        rows[1] = copy.deepcopy(rows[0])
        self.save()
        with self.assertRaisesRegex(ValueError, "duplicate_row_id"):
            self.run_audit()

    def test_overlap_and_non_holdout_cannot_be_ignored(self):
        book = self.intake["workbooks"][0]
        book["nfc_exact_development_overlap"] = 1
        book["split_counts"] = {"development": 100}
        self.save()
        blockers = self.run_audit()["blockers"]
        self.assertIn("development_overlap_reported", blockers)
        self.assertIn("non_holdout_split", blockers)

    def test_boolean_size_is_not_a_byte_count(self):
        pin = self.artifact("one", "x")
        pin["bytes"] = True
        with self.assertRaisesRegex(ValueError, "invalid_pin_size"):
            verify_pin(pin, [self.private])

    def test_private_input_escape_rejected(self):
        other = self.base / "outside"
        other.write_text("private")
        self.draft["existing_host_config"] = self.pin(other)
        self.save()
        with self.assertRaisesRegex(ValueError, "input_outside_allowed_roots"):
            self.run_audit()

    def test_unc_and_relative_inputs_rejected(self):
        for value in ("relative", "//server/share/secret", "\\\\server\\share\\secret"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                verify_pin({"path": value, "bytes": 1, "sha256": "a" * 64}, [self.private])

    def test_duplicate_nonfinite_float_and_oversize_json_rejected(self):
        for raw in (b'{"a":1,"a":2}', b'{"a":NaN}', b'{"a":1.5}', b"x" * (MAX_JSON + 1)):
            with self.assertRaises(ValueError):
                strict_json(raw)

    def test_output_is_private_and_never_overwritten(self):
        target = self.private / "audit.json"
        report = self.run_audit()
        write_report(target, report, self.root)
        before = target.read_bytes()
        with self.assertRaises(FileExistsError):
            write_report(target, {}, self.root)
        self.assertEqual(before, target.read_bytes())
        with self.assertRaisesRegex(ValueError, "private_output_required"):
            write_report(self.root / "public.json", report, self.root)

    def test_cli_blocked_exit_is_two_and_errors_are_redacted(self):
        output = self.private / "cli-report.json"
        args = ["audit", "--draft", str(self.draft_path), "--input-root",
                str(self.private), "--output", str(output)]
        with patch("sys.argv", args), patch(
                "scripts.encoder.qualification_package_audit.audit", return_value=self.run_audit()), \
                contextlib.redirect_stdout(io.StringIO()) as printed:
            self.assertEqual(main(), 2)
        self.assertIn("BLOCKED", printed.getvalue())
        with patch("sys.argv", args), patch(
                "scripts.encoder.qualification_package_audit.audit",
                side_effect=ValueError("PRIVATE_SOURCE_MUST_NOT_LEAK")), \
                contextlib.redirect_stdout(io.StringIO()) as printed:
            self.assertEqual(main(), 1)
        self.assertNotIn("PRIVATE_SOURCE_MUST_NOT_LEAK", printed.getvalue())


if __name__ == "__main__":
    unittest.main()
