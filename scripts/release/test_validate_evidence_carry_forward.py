from __future__ import annotations

import copy
import json
import unittest
import tempfile
from pathlib import Path

from scripts.release.validate_evidence_carry_forward import (
    EvidenceCarryForwardError,
    _raw_evidence_manifest,
    _verify_p5_aggregate_v1,
    analyze_evidence_carry_forward,
)


ROOT = Path(__file__).resolve().parents[2]
OLD_M5_07 = (
    ROOT
    / "docs"
    / "research"
    / "evidence"
    / "M5_07_MACOS_ARM64_PRE_RELEASE_72H_C336C5BA_RUN_30704999238.json"
)


def _candidate() -> dict[str, object]:
    return {
        "format": "onebrain/base-v1-candidate-identity/1",
        "object_format": "sha1",
        "candidate_commit": "11" * 20,
        "candidate_tree": "22" * 20,
        "candidate_semantic_digest": "33" * 32,
        "frozen_target_artifact_digest": "44" * 32,
        "registry_root": "55" * 32,
        "p5_aggregate_root": "66" * 32,
        "executable_blake3": "77" * 32,
        "sbom_blake3": "88" * 32,
        "provenance_blake3": "99" * 32,
        "runner_image_digest": "aa" * 32,
        "toolchain_digest": "bb" * 32,
        "lockfile_digest": "cc" * 32,
        "release_request_digest": "dd" * 32,
        "qualification_session_id": "ee" * 32,
    }


def _evidence() -> dict[str, object]:
    candidate = _candidate()
    return {
        "format": "onebrain/base-v1-soak-evidence/1",
        "source_binding": copy.deepcopy(candidate),
        "runner_identity": "self-hosted:onebrain-soak:runner-a",
        "evidence_filename": "base-v1-soak-" + str(candidate["candidate_commit"]) + ".json",
        "closure_manifest": {
            "src/onebrain-archive/src/lib.rs": "01" * 32,
            "src/onebrain-node/src/base_services.rs": "02" * 32,
            "scripts/concept_registry/production_qualification.py": "03" * 32,
            "src/Cargo.lock": str(candidate["lockfile_digest"]),
            "rust-toolchain.toml": str(candidate["toolchain_digest"]),
        },
    }


class EvidenceCarryForwardTests(unittest.TestCase):
    def test_v1_verifier_remains_explicit_and_raw_v2_manifest_is_content_addressed(self) -> None:
        self.assertTrue(callable(_verify_p5_aggregate_v1))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "receipt.json").write_bytes(b"{}")
            first, count = _raw_evidence_manifest(root)
            self.assertEqual(count, 1)
            (root / "receipt.json").write_bytes(b'{"changed":true}')
            second, _ = _raw_evidence_manifest(root)
            self.assertNotEqual(first, second)

    def test_filename_only_identity_is_rejected(self) -> None:
        evidence = _evidence()
        del evidence["source_binding"]
        result = analyze_evidence_carry_forward(evidence, _candidate(), [])
        self.assertFalse(result["analytically_reusable"])
        self.assertIn("authenticated source binding is missing", result["rejection_reasons"])

    def test_short_commit_is_rejected(self) -> None:
        candidate = _candidate()
        candidate["candidate_commit"] = "11" * 6
        with self.assertRaisesRegex(EvidenceCarryForwardError, "full candidate_commit"):
            analyze_evidence_carry_forward(_evidence(), candidate, [])

    def test_changed_archive_facade_registry_lockfile_or_toolchain_is_stale(self) -> None:
        for changed in (
            "src/onebrain-archive/src/lib.rs",
            "src/onebrain-node/src/base_services.rs",
            "scripts/concept_registry/production_qualification.py",
            "src/Cargo.lock",
            "rust-toolchain.toml",
        ):
            with self.subTest(changed=changed):
                result = analyze_evidence_carry_forward(
                    _evidence(), _candidate(), [changed]
                )
                self.assertFalse(result["analytically_reusable"])
                self.assertIn(changed, result["changed_closure_paths"])

    def test_missing_runner_identity_is_rejected(self) -> None:
        evidence = _evidence()
        evidence["runner_identity"] = ""
        result = analyze_evidence_carry_forward(evidence, _candidate(), [])
        self.assertFalse(result["analytically_reusable"])
        self.assertIn("runner identity is missing", result["rejection_reasons"])

    def test_unchanged_closure_is_analytical_only_and_never_qualifies_base_v1(self) -> None:
        result = analyze_evidence_carry_forward(_evidence(), _candidate(), [])
        self.assertTrue(result["analytically_reusable"])
        self.assertFalse(result["base_v1_reusable"])
        self.assertTrue(result["fresh_soak_required"])
        self.assertFalse(result["production_qualified"])
        self.assertIn("fresh 72-hour soak", result["base_v1_rejection_reason"])

    def test_synthetically_unchanged_digest_cannot_hide_a_changed_critical_path(self) -> None:
        evidence = _evidence()
        evidence["synthetic_closure_digest"] = "ff" * 32
        result = analyze_evidence_carry_forward(
            evidence,
            _candidate(),
            ["src/onebrain-node/src/base_services.rs"],
        )
        self.assertFalse(result["analytically_reusable"])
        self.assertTrue(result["fresh_soak_required"])

    def test_existing_m5_07_report_is_recorded_as_rejected(self) -> None:
        evidence = json.loads(OLD_M5_07.read_text(encoding="utf-8"))
        result = analyze_evidence_carry_forward(evidence, _candidate(), [])
        self.assertEqual(result["evidence_path_class"], "legacy-m5-07")
        self.assertFalse(result["analytically_reusable"])
        self.assertFalse(result["base_v1_reusable"])
        self.assertTrue(result["fresh_soak_required"])


if __name__ == "__main__":
    unittest.main()
