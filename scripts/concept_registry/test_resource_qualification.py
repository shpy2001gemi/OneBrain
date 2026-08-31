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

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from build_obr import build
from config import SOURCE_WIKIDATA
from resource_qualification import (
    MAX_PRODUCTION_OBR_BYTES,
    MIN_PRODUCTION_OBR_BYTES,
    PROFILE,
    PROBE_PROFILE,
    QualificationError,
    collect_volume_evidence,
    create_resource_receipt,
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

    def test_ssd_and_hdd_require_matching_captured_volume_evidence(self) -> None:
        cases = (
            (
                "ssd",
                {"storage_class": "ssd", "collector": "linux-sysfs", "rotational": 0},
                "storage_is_ssd",
            ),
            (
                "hdd",
                {"storage_class": "hdd", "collector": "linux-sysfs", "rotational": 1},
                "storage_is_rotational_hdd",
            ),
        )
        for profile, evidence, oracle in cases:
            with self.subTest(profile=profile):
                oracles = evaluate_oracles(
                    profile,
                    _valid_execution(),
                    {"request_completed": False},
                    None,
                    1_000,
                    1_000,
                    64 * 1024 * 1024,
                    volume_evidence=evidence,
                    obr_bytes=MIN_PRODUCTION_OBR_BYTES,
                    production_candidate=True,
                )
                self.assertTrue(oracles[oracle])
                self.assertTrue(oracles["production_obr_size_is_inclusive"])

    def test_unknown_or_missing_production_volume_evidence_fails_closed(self) -> None:
        for evidence in (None, {}, {"storage_class": "unknown"}):
            with self.subTest(evidence=evidence):
                oracles = evaluate_oracles(
                    "ssd",
                    _valid_execution(),
                    {"request_completed": False},
                    None,
                    1_000,
                    1_000,
                    64 * 1024 * 1024,
                    volume_evidence=evidence,
                    obr_bytes=MIN_PRODUCTION_OBR_BYTES,
                    production_candidate=True,
                )
                self.assertFalse(oracles["storage_is_ssd"])
                self.assertFalse(all(oracles.values()))

        mismatched = evaluate_oracles(
            "ssd",
            _valid_execution(),
            {"request_completed": False},
            None,
            1_000,
            1_000,
            64 * 1024 * 1024,
            volume_evidence={
                "storage_class": "ssd",
                "collector": "linux-sysfs",
                "rotational": 1,
            },
            obr_bytes=MIN_PRODUCTION_OBR_BYTES,
            production_candidate=True,
        )
        self.assertFalse(mismatched["storage_is_ssd"])

    def test_portability_storage_collector_cannot_claim_production_reference(self) -> None:
        with patch("resource_qualification.sys.platform", "win32"):
            oracles = evaluate_oracles(
                "ssd",
                _valid_execution(),
                {"request_completed": False},
                None,
                1_000,
                1_000,
                64 * 1024 * 1024,
                volume_evidence={
                    "storage_class": "ssd",
                    "collector": "windows-physical-disk",
                },
                obr_bytes=MIN_PRODUCTION_OBR_BYTES,
                production_candidate=True,
            )
        self.assertFalse(oracles["production_reference_host_is_linux"])
        self.assertFalse(all(oracles.values()))

    def test_production_candidate_size_bounds_are_inclusive(self) -> None:
        cases = (
            (MIN_PRODUCTION_OBR_BYTES - 1, False),
            (MIN_PRODUCTION_OBR_BYTES, True),
            (MAX_PRODUCTION_OBR_BYTES, True),
            (MAX_PRODUCTION_OBR_BYTES + 1, False),
        )
        for size, expected in cases:
            with self.subTest(size=size):
                oracles = evaluate_oracles(
                    "cold-cache",
                    _valid_execution(),
                    {"request_completed": True},
                    None,
                    1_000,
                    1_000,
                    64 * 1024 * 1024,
                    obr_bytes=size,
                    production_candidate=True,
                )
                self.assertEqual(
                    oracles["production_obr_size_is_inclusive"], expected
                )

    def test_platform_volume_collector_rejects_unknown_platform(self) -> None:
        with patch("resource_qualification.sys.platform", "plan9"):
            with self.assertRaisesRegex(QualificationError, "unsupported platform"):
                collect_volume_evidence(Path("candidate"))

    def test_prequalification_receipt_is_signed_without_release_only_fields(self) -> None:
        from production_qualification import signer_fingerprint, trust_policy_digest

        key = Ed25519PrivateKey.from_private_bytes(bytes([43]) * 32)
        public = key.public_key().public_bytes_raw()
        policy = {
            "algorithm": "Ed25519",
            "allowed_usages": ["registry-qualification-receipt"],
            "format": "onebrain/concept-registry-trust-policy/1",
            "signers": [{
                "fingerprint_algorithm": "blake3-derive-key-v1",
                "fingerprint_context": "onebrain:concept-registry:signer-fingerprint:1",
                "fingerprint_hex": signer_fingerprint(public),
                "public_key_hex": public.hex(),
            }],
        }
        binding = {
            "release_aggregate_root": "11" * 32,
            "registry_generation": 1,
            "production_profile_blake3": "22" * 32,
            "trust_policy_digest": trust_policy_digest(policy),
            "signer_fingerprint": signer_fingerprint(public),
            "probe_blake3": "33" * 32,
            "executable_blake3": "33" * 32,
            "candidate_payload_artifacts_blake3": {
                "OBR:concepts.obr": "41" * 32,
                "LABEL_INDEX:concepts.obr.labels.idx": "42" * 32,
                "CCID_INDEX:concepts.obr.ccids.idx": "43" * 32,
                "MANIFEST:concepts.obr.manifest.json": "44" * 32,
                "SPDX_SBOM:sbom.spdx.json": "45" * 32,
            },
            "release_stamp_blake3": "55" * 32,
        }
        receipt = create_resource_receipt(
            {"qualified": True, "qualification_profile": "ssd", "exit_oracles": {"ok": True}},
            {
                "format": "onebrain/qualification-run-context/1",
                "variant": "Prequalification",
                "closure_digest": "66" * 32,
            },
            binding,
            key,
            policy,
        )
        payload = receipt["payload"]
        self.assertFalse(payload["base_candidate_bound"])
        self.assertEqual(payload["qualification_context_variant"], "Prequalification")
        for field in ("release_request_digest", "qualification_session_id", "candidate_commit", "candidate_tree"):
            self.assertNotIn(field, payload)

    def test_raw_release_context_and_binding_cannot_create_a_receipt(self) -> None:
        from production_qualification import signer_fingerprint, trust_policy_digest

        key = Ed25519PrivateKey.from_private_bytes(bytes([45]) * 32)
        public = key.public_key().public_bytes_raw()
        policy = {
            "algorithm": "Ed25519",
            "allowed_usages": ["registry-qualification-receipt"],
            "format": "onebrain/concept-registry-trust-policy/1",
            "signers": [{
                "fingerprint_algorithm": "blake3-derive-key-v1",
                "fingerprint_context": "onebrain:concept-registry:signer-fingerprint:1",
                "fingerprint_hex": signer_fingerprint(public),
                "public_key_hex": public.hex(),
            }],
        }
        binding = {
            "release_aggregate_root": "11" * 32,
            "registry_generation": 1,
            "production_profile_blake3": "22" * 32,
            "trust_policy_digest": trust_policy_digest(policy),
            "signer_fingerprint": signer_fingerprint(public),
            "probe_blake3": "33" * 32,
            "executable_blake3": "34" * 32,
            "candidate_payload_artifacts_blake3": {name: "44" * 32 for name in (
                "OBR:concepts.obr", "LABEL_INDEX:concepts.obr.labels.idx",
                "CCID_INDEX:concepts.obr.ccids.idx", "MANIFEST:concepts.obr.manifest.json",
                "SPDX_SBOM:sbom.spdx.json",
            )},
            "release_stamp_blake3": "55" * 32,
            "candidate_semantic_digest": "66" * 32,
            "artifact_tuple_digest": "77" * 32,
        }
        context = {
            "format": "onebrain/qualification-run-context/1",
            "variant": "Release",
            "release_request_digest": "88" * 32,
            "qualification_session_id": "99" * 32,
            "candidate_commit": "aa" * 20,
            "candidate_tree": "bb" * 20,
        }
        with self.assertRaisesRegex(QualificationError, "verified signed release request"):
            create_resource_receipt(
                {"qualified": True, "qualification_profile": "ssd", "exit_oracles": {"ok": True}},
                context, binding, key, policy,
            )

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

    def test_signed_cli_returns_payload_gate_status_without_key_error(self) -> None:
        from production_qualification import signer_fingerprint, trust_policy_digest

        report = {
            "profile": PROFILE,
            "qualified": True,
            "qualification_profile": "ssd",
            "exit_oracles": {"fixture": True},
        }
        key = Ed25519PrivateKey.from_private_bytes(bytes([44]) * 32)
        public = key.public_key().public_bytes_raw()
        policy = {
            "algorithm": "Ed25519",
            "allowed_usages": ["registry-qualification-receipt"],
            "format": "onebrain/concept-registry-trust-policy/1",
            "signers": [{
                "fingerprint_algorithm": "blake3-derive-key-v1",
                "fingerprint_context": "onebrain:concept-registry:signer-fingerprint:1",
                "fingerprint_hex": signer_fingerprint(public),
                "public_key_hex": public.hex(),
            }],
        }
        context = {
            "format": "onebrain/qualification-run-context/1",
            "variant": "Prequalification",
            "closure_digest": "66" * 32,
        }
        binding = {
            "release_aggregate_root": "11" * 32,
            "registry_generation": 1,
            "production_profile_blake3": "22" * 32,
            "trust_policy_digest": trust_policy_digest(policy),
            "signer_fingerprint": signer_fingerprint(public),
            "probe_blake3": "33" * 32,
            "executable_blake3": "34" * 32,
            "candidate_payload_artifacts_blake3": {
                "OBR:concepts.obr": "41" * 32,
                "LABEL_INDEX:concepts.obr.labels.idx": "42" * 32,
                "CCID_INDEX:concepts.obr.ccids.idx": "43" * 32,
                "MANIFEST:concepts.obr.manifest.json": "44" * 32,
                "SPDX_SBOM:sbom.spdx.json": "45" * 32,
            },
            "release_stamp_blake3": "55" * 32,
        }
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            paths = {
                "context": root / "context.json",
                "binding": root / "binding.json",
                "policy": root / "policy.json",
                "key": root / "key.hex",
                "output": root / "receipt.json",
            }
            paths["context"].write_text(json.dumps(context), encoding="utf-8")
            paths["binding"].write_text(json.dumps(binding), encoding="utf-8")
            paths["policy"].write_text(json.dumps(policy), encoding="utf-8")
            paths["key"].write_text((bytes([44]) * 32).hex(), encoding="ascii")
            arguments = [
                "--profile", "ssd", "--probe", "probe", "--obr", "fixture.obr",
                "--labels-file", "labels.txt", "--output", str(paths["output"]),
                "--budget-profile", "ci-small-fixture-v1", "--run-context", str(paths["context"]),
                "--release-binding", str(paths["binding"]), "--trust-policy", str(paths["policy"]),
                "--private-key", str(paths["key"]),
            ]
            with patch("resource_qualification.run_qualification", return_value=report), redirect_stdout(StringIO()):
                self.assertEqual(main(arguments), 0)
            receipt = json.loads(paths["output"].read_text(encoding="utf-8"))
            self.assertTrue(receipt["payload"]["result"])


if __name__ == "__main__":
    unittest.main()
