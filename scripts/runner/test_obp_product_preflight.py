import copy
import tempfile
import unittest
from pathlib import Path

from scripts.runner.obp_product_preflight import CLAIMS, FORMAT, SUITES, collect, digest, verify


class LocalEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.report = {"format": FORMAT, "scope": "single-machine-local-preflight",
                       "claims": CLAIMS.copy(), "preflight_passed": True, "suites": []}
        for name, argv in SUITES.items():
            log = self.root / (name + ".log")
            log.write_text("fixture only\n", encoding="utf-8")
            self.report["suites"].append({"id": name, "argv": argv, "exit_code": 0,
                                          "log": log.name, "sha256": digest(log)})

    def test_integrity_does_not_qualify(self):
        verify(self.report, self.root)
        self.assertTrue(all(value is False for value in self.report["claims"].values()))

    def test_each_qualification_promotion_is_rejected(self):
        for name in CLAIMS:
            changed = copy.deepcopy(self.report)
            changed["claims"][name] = True
            with self.subTest(name=name), self.assertRaises(ValueError):
                verify(changed, self.root)

    def test_missing_duplicate_and_tampered_logs_reject(self):
        for rows in [self.report["suites"][:-1], self.report["suites"] + [self.report["suites"][0]]]:
            changed = dict(self.report, suites=rows)
            with self.assertRaises(ValueError):
                verify(changed, self.root)
        (self.root / "node.log").write_text("changed", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "digest"):
            verify(self.report, self.root)

    def test_failure_cannot_be_reported_as_pass(self):
        self.report["suites"][0]["exit_code"] = 1
        with self.assertRaises(ValueError):
            verify(self.report, self.root)
        self.report["preflight_passed"] = False
        verify(self.report, self.root)

    def test_command_and_path_substitution_reject(self):
        for field, value in [("argv", ["ssh", "host"]), ("log", "../outside")]:
            changed = copy.deepcopy(self.report)
            changed["suites"][0][field] = value
            with self.assertRaises(ValueError):
                verify(changed, self.root)

    def test_existing_attempt_is_not_overwritten_or_executed(self):
        before = {path.name: path.read_bytes() for path in self.root.iterdir()}
        with self.assertRaises(FileExistsError):
            collect(self.root)
        self.assertEqual(before, {path.name: path.read_bytes() for path in self.root.iterdir()})

    def test_missing_claim_or_numeric_false_is_not_accepted(self):
        for claims in [{}, dict(CLAIMS, multi_host_qualified=0)]:
            with self.assertRaises(ValueError):
                verify(dict(self.report, claims=claims), self.root)


if __name__ == "__main__":
    unittest.main()
