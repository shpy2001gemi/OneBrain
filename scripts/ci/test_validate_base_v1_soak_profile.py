from __future__ import annotations

import copy
import importlib.util
import json
import subprocess
import sys
import unittest
from pathlib import Path

import blake3
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

from scripts.ci.validate_vnext_contracts import (
    ContractError,
    validate_base_v1_exact_candidate_soak,
)
from scripts.release.validate_evidence_carry_forward import (
    SoakEvidenceError,
    aggregate_soak_receipts_for_test_nonproduction,
    profile_for_test_nonproduction,
    sign_soak_child_for_test_nonproduction,
)


ROOT = Path(__file__).resolve().parents[2]
PROFILE_PATH = ROOT / "src/test-vectors/vnext/base-v1-exact-candidate-soak-v1.json"
WORKFLOW_PATH = ROOT / ".github/workflows/vnext-p5-production-canary.yml"
P5_RUNNER_PATH = ROOT / "scripts/runner/onebrain-p5-multi-host.py"


class ExactCandidateSoakProfileTests(unittest.TestCase):
    def setUp(self) -> None:
        self.runner_keys = {
            runner_id: Ed25519PrivateKey.generate()
            for runner_id in ("runner-a", "runner-b", "runner-c")
        }
        self.aggregator_key = Ed25519PrivateKey.generate()
        self.profile = profile_for_test_nonproduction(
            self.runner_keys, self.aggregator_key
        )
        self.binding = {
            "release_request_digest": "10" * 32,
            "qualification_session_id": "20" * 32,
            "candidate_commit": "30" * 20,
            "candidate_tree": "40" * 20,
            "candidate_semantic_digest": "50" * 32,
            "frozen_target_artifact_digest": "60" * 32,
            "registry_root": "70" * 32,
            "p5_aggregate_root": "80" * 32,
            "executable_blake3": "90" * 32,
            "sbom_blake3": "a0" * 32,
            "provenance_blake3": "b0" * 32,
            "runner_image_digest": "c0" * 32,
            "trust_policy_digest": self.profile["trust_policy"]["digest_hex"],
        }
        self.receipts = [
            sign_soak_child_for_test_nonproduction(
                profile=self.profile,
                binding=self.binding,
                runner_id=runner_id,
                interval_sequence=index,
                receipt_kind="fault" if index == 2 else "interval",
                signing_key=self.runner_keys[runner_id],
            )
            for index, runner_id in enumerate(
                ("runner-a", "runner-b", "runner-c"), start=1
            )
        ]

    def _aggregate(
        self,
        receipts: list[dict[str, object]] | None = None,
        claimed_root: str | None = None,
    ) -> dict[str, object]:
        return aggregate_soak_receipts_for_test_nonproduction(
            profile=self.profile,
            binding=self.binding,
            receipts=receipts or self.receipts,
            aggregator_key=self.aggregator_key,
            claimed_root=claimed_root,
        )

    def test_frozen_profile_and_workflow_are_closed(self) -> None:
        counts = validate_base_v1_exact_candidate_soak()
        self.assertEqual(counts, (3, 4, 2, 13))

    def test_valid_but_unlisted_signer_is_rejected(self) -> None:
        receipts = copy.deepcopy(self.receipts)
        key = Ed25519PrivateKey.generate()
        public = key.public_key().public_bytes_raw()
        receipt = receipts[0]
        receipt["signer_public_key"] = public.hex()
        receipt["signer_fingerprint"] = blake3.blake3(
            public,
            derive_key_context=self.profile["trust_policy"]["fingerprint_context"],
        ).hexdigest()
        receipt["signature"] = key.sign(
            self._child_message(receipt["payload"])
        ).hex()
        with self.assertRaisesRegex(SoakEvidenceError, "allowlisted"):
            self._aggregate(receipts)

    def test_wrong_or_cross_runner_role_is_rejected(self) -> None:
        receipts = copy.deepcopy(self.receipts)
        receipt = receipts[0]
        receipt["payload"]["role"] = "soak-runner:runner-b"
        receipt["signature"] = self.runner_keys["runner-a"].sign(
            self._child_message(receipt["payload"])
        ).hex()
        with self.assertRaisesRegex(SoakEvidenceError, "role"):
            self._aggregate(receipts)

    def test_changed_trust_policy_is_rejected(self) -> None:
        profile = copy.deepcopy(self.profile)
        profile["trust_policy"]["digest_hex"] = "ff" * 32
        with self.assertRaisesRegex(SoakEvidenceError, "trust-policy digest"):
            aggregate_soak_receipts_for_test_nonproduction(
                profile=profile,
                binding=self.binding,
                receipts=self.receipts,
                aggregator_key=self.aggregator_key,
            )

    def test_mixed_qualification_session_is_rejected(self) -> None:
        receipts = copy.deepcopy(self.receipts)
        receipt = receipts[0]
        receipt["payload"]["qualification_session_id"] = "ff" * 32
        receipt["signature"] = self.runner_keys["runner-a"].sign(
            self._child_message(receipt["payload"])
        ).hex()
        with self.assertRaisesRegex(SoakEvidenceError, "qualification_session_id"):
            self._aggregate(receipts)

    def test_self_including_aggregate_root_is_rejected(self) -> None:
        with self.assertRaisesRegex(SoakEvidenceError, "aggregate root"):
            self._aggregate(claimed_root="ff" * 32)

    def test_nonproduction_soak_aggregate_never_claims_qualification(self) -> None:
        report = self._aggregate()
        self.assertEqual(report["verified_child_receipts"], 3)
        self.assertFalse(report["soak_qualified"])
        self.assertFalse(report["production_qualified"])

    def test_three_local_process_p5_dry_run_cannot_claim_multi_host(self) -> None:
        spec = importlib.util.spec_from_file_location("p5_dry_run", P5_RUNNER_PATH)
        assert spec is not None and spec.loader is not None
        module = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = module
        spec.loader.exec_module(module)
        host_keys = {
            host: Ed25519PrivateKey.generate()
            for host in ("host-a", "host-b", "host-c")
        }
        orchestrator = Ed25519PrivateKey.generate()
        profile = module.profile_for_test_nonproduction(host_keys, orchestrator)
        binding = {
            "release_request_digest": "11" * 32,
            "qualification_session_id": "22" * 32,
            "candidate_commit": "33" * 20,
            "candidate_tree": "44" * 20,
            "candidate_semantic_digest": "55" * 32,
            "linux_artifact_tuple_digest": "66" * 32,
            "agent_binary_digest": "77" * 32,
            "agent_signature_digest": "88" * 32,
            "registry_root": "99" * 32,
            "profile_digest": "aa" * 32,
            "trust_policy_digest": profile["trust_policy"]["digest_hex"],
        }
        inventory = module.inventory_for_test_nonproduction(profile, binding)

        class LocalProcessExecutor:
            def __init__(self) -> None:
                self.processes_started = 0

            def run(self, host, commands, timeout_seconds):
                host_id = host["physical_host_id"]
                receipts = [
                    module.sign_child_receipt_for_test_nonproduction(
                        profile=profile,
                        binding=binding,
                        host_id=host_id,
                        sequence=index,
                        fault_id=fault,
                        signing_key=host_keys[host_id],
                    )
                    for index, fault in enumerate(profile["fault_matrix"], start=1)
                ]
                child = subprocess.run(
                    [
                        sys.executable,
                        "-c",
                        "import json,sys; value=json.load(sys.stdin); json.dump(value,sys.stdout)",
                    ],
                    input=json.dumps(receipts),
                    text=True,
                    capture_output=True,
                    check=True,
                    timeout=timeout_seconds,
                )
                self.processes_started += 1
                return json.loads(child.stdout)

        executor = LocalProcessExecutor()
        report = module.run_multi_host_qualification_for_test_nonproduction(
            profile=profile,
            inventory=inventory,
            binding=binding,
            executor=executor,
            timeout_seconds=5.0,
        )
        self.assertEqual(executor.processes_started, 3)
        self.assertEqual(report["verified_child_receipts"], 39)
        self.assertFalse(report["multi_host_qualified"])
        self.assertNotIn("production_qualified", report)

    def test_workflow_has_immutable_identity_and_separate_outputs(self) -> None:
        workflow = WORKFLOW_PATH.read_text(encoding="utf-8")
        for marker in (
            "workflow_dispatch:",
            "verify_base_release_request.py",
            "candidate_commit",
            "candidate_tree",
            "release_request_digest",
            "qualification_session_id",
            "compare-release-executable-hashes",
            "retain-signed-raw-receipts",
            "p5-multi-host-aggregate",
            "base-v1-exact-candidate-soak-aggregate",
        ):
            self.assertIn(marker, workflow)
        for forbidden in (
            "pull_request:",
            "candidate_commit:\n        description:",
            "candidate_tree:\n        description:",
        ):
            self.assertNotIn(forbidden, workflow)

    def test_production_cli_measures_p5_executable_sbom_and_provenance(self) -> None:
        source = (
            ROOT / "scripts/release/validate_evidence_carry_forward.py"
        ).read_text(encoding="utf-8")
        for marker in (
            "def _verify_p5_aggregate(",
            'parser.add_argument("--p5-aggregate", type=Path, required=True)',
            'parser.add_argument("--executable", type=Path, required=True)',
            '"SPDX_SBOM:sbom.spdx.json"',
            '_canonical_json(verified["tooling_blake3"])',
        ):
            self.assertIn(marker, source)
        for forbidden in (
            'parser.add_argument("--p5-aggregate-root"',
            'parser.add_argument("--executable-blake3"',
            'parser.add_argument("--sbom-blake3"',
            'parser.add_argument("--provenance-blake3"',
        ):
            self.assertNotIn(forbidden, source)

    def _child_message(self, payload: object) -> bytes:
        domain = self.profile["child_receipt"]["signature_domain"].replace(
            "\\0", "\0"
        ).encode("ascii")
        canonical = json.dumps(
            payload, sort_keys=True, separators=(",", ":"), ensure_ascii=True
        ).encode()
        return domain + blake3.blake3(canonical).digest()


if __name__ == "__main__":
    unittest.main()
