"""Tests for cold-cache and low-RAM Concept Registry qualification."""

from __future__ import annotations

import json
import os
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path
from unittest.mock import patch

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from build_obr import build
from config import SOURCE_WIKIDATA
from resource_qualification import (
    PROFILE,
    PROBE_PROFILE,
    QualificationError,
    evaluate_oracles,
    execute_probe,
    main,
    resolve_budget,
    run_qualification,
)


def _record(ext_id: int, name: str) -> dict[str, object]:
    return {
        "source": SOURCE_WIKIDATA,
        "ext_id": ext_id,
        "category": 7,
        "name": name,
        "canonical_form": f"wd:{ext_id}",
        "labels": {"en": name},
    }


def _valid_execution() -> dict[str, object]:
    return {
        "exit_code": 0,
        "timed_out": False,
        "peak_rss_bytes": 32 * 1024 * 1024,
        "probe": {
            "profile": PROBE_PROFILE,
            "verification_mode": "uncached",
            "cache_capacity": 0,
            "labels_source": "external-file",
            "sampled_from_obr": False,
            "lookups": 2,
            "found": 1,
            "ambiguous": 0,
            "missing": 1,
            "ready_ms": 50,
            "p95_us": 100,
        },
    }


class ResourceQualificationTests(unittest.TestCase):
    def _build_fixture(self, root: Path) -> tuple[Path, Path]:
        merged = root / "merged"
        merged.mkdir(parents=True)
        input_path = merged / "concepts_deduped.jsonl"
        input_path.write_text(
            "".join(
                json.dumps(record) + "\n"
                for record in [_record(42, "Douglas Adams"), _record(283, "Water")]
            ),
            encoding="utf-8",
        )
        obr_path = root / "concepts.obr"
        build(input_path, obr_path)
        labels_path = root / "probe-labels.txt"
        labels_path.write_text("Water\nmissing-label\n", encoding="utf-8")
        return obr_path, labels_path

    def test_cold_cache_oracles_require_targeted_eviction(self) -> None:
        oracles = evaluate_oracles(
            "cold-cache",
            _valid_execution(),
            {"request_completed": False},
            None,
            1000,
            1000,
            64 * 1024 * 1024,
        )
        self.assertFalse(oracles["targeted_cache_eviction_request_completed"])
        self.assertFalse(all(oracles.values()))

    def test_low_ram_oracles_require_hard_address_space_limit(self) -> None:
        oracles = evaluate_oracles(
            "low-ram",
            _valid_execution(),
            {"request_completed": False},
            None,
            1000,
            1000,
            64 * 1024 * 1024,
        )
        self.assertFalse(oracles["hard_address_space_limit_applied"])
        self.assertFalse(all(oracles.values()))

    def test_budget_cannot_be_used_for_the_wrong_profile(self) -> None:
        with self.assertRaisesRegex(QualificationError, "does not allow"):
            resolve_budget("low-ram", "cold-cache-production-v1")

    def test_shared_ci_budget_does_not_apply_low_ram_limit_to_cold_cache(self) -> None:
        execution = _valid_execution()
        with patch(
            "resource_qualification._artifact_evidence", return_value={}
        ), patch(
            "resource_qualification.prepare_cold_cache",
            return_value={"request_completed": True},
        ), patch("resource_qualification.execute_probe", return_value=execution) as probe:
            report = run_qualification(
                "cold-cache",
                Path("probe"),
                Path("fixture.obr"),
                Path("labels.txt"),
                "auto",
                "ci-small-fixture-v1",
                30,
            )
        self.assertTrue(report["qualified"])
        self.assertIsNone(report["memory_enforcement"]["address_space_limit_bytes"])
        self.assertIsNone(probe.call_args.args[-1])

    @unittest.skipUnless(
        os.environ.get("ONEBRAIN_REGISTRY_PROBE"),
        "set ONEBRAIN_REGISTRY_PROBE to run the compiled-probe integration",
    )
    def test_compiled_probe_emits_frozen_uncached_json(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            obr_path, labels_path = self._build_fixture(Path(directory))
            execution = execute_probe(
                Path(os.environ["ONEBRAIN_REGISTRY_PROBE"]),
                obr_path,
                labels_path,
                30,
                None,
            )
            self.assertEqual(execution["exit_code"], 0, execution["stderr_tail"])
            probe = execution["probe"]
            self.assertEqual(probe["profile"], PROBE_PROFILE)
            self.assertEqual(probe["verification_mode"], "uncached")
            self.assertEqual(probe["cache_capacity"], 0)
            self.assertEqual(probe["labels_source"], "external-file")
            self.assertFalse(probe["sampled_from_obr"])
            self.assertEqual(probe["lookups"], 2)
            self.assertIsInstance(execution["peak_rss_bytes"], int)

    @unittest.skipUnless(
        sys.platform.startswith("linux")
        and os.environ.get("ONEBRAIN_REGISTRY_PROBE"),
        "Linux compiled probe is required for resource-limit integration",
    )
    def test_linux_cold_cache_and_low_ram_fixtures_qualify(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            obr_path, labels_path = self._build_fixture(Path(directory))
            common = (
                Path(os.environ["ONEBRAIN_REGISTRY_PROBE"]),
                obr_path,
                labels_path,
                "auto",
                "ci-small-fixture-v1",
            )
            cold = run_qualification("cold-cache", *common, 30)
            self.assertTrue(cold["qualified"], cold)
            low_ram = run_qualification("low-ram", *common, 30)
            self.assertTrue(low_ram["qualified"], low_ram)

    def test_cli_writes_report_and_returns_gate_status(self) -> None:
        report = {
            "profile": PROFILE,
            "qualified": True,
            "exit_oracles": {"fixture": True},
        }
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "evidence" / "resource.json"
            arguments = [
                "--profile",
                "cold-cache",
                "--probe",
                "probe",
                "--obr",
                "fixture.obr",
                "--labels-file",
                "labels.txt",
                "--output",
                str(output),
                "--budget-profile",
                "ci-small-fixture-v1",
            ]
            with patch(
                "resource_qualification.run_qualification", return_value=report
            ), redirect_stdout(StringIO()):
                self.assertEqual(main(arguments), 0)
            self.assertEqual(json.loads(output.read_text(encoding="utf-8")), report)


if __name__ == "__main__":
    unittest.main()
