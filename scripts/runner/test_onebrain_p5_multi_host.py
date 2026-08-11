from __future__ import annotations

import copy
import importlib.util
import sys
import unittest
from pathlib import Path

import blake3
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey


RUNNER = Path(__file__).with_name("onebrain-p5-multi-host.py")
SPEC = importlib.util.spec_from_file_location("onebrain_p5_multi_host", RUNNER)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("P5 multi-host runner module cannot be loaded")
runner = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = runner
SPEC.loader.exec_module(runner)


class FakeSshExecutor:
    def __init__(self, receipts: dict[str, list[dict[str, object]]]) -> None:
        self.receipts = receipts
        self.calls: list[tuple[str, list[dict[str, object]], float]] = []
        self.timeout_host: str | None = None

    def run(
        self,
        host: dict[str, object],
        commands: list[dict[str, object]],
        timeout_seconds: float,
    ) -> list[dict[str, object]]:
        host_id = str(host["physical_host_id"])
        self.calls.append((host_id, commands, timeout_seconds))
        if host_id == self.timeout_host:
            raise TimeoutError(host_id)
        return copy.deepcopy(self.receipts.get(host_id, []))


class P5MultiHostOrchestratorTests(unittest.TestCase):
    def setUp(self) -> None:
        self.host_keys = {
            host: Ed25519PrivateKey.generate() for host in ("host-a", "host-b", "host-c")
        }
        self.orchestrator_key = Ed25519PrivateKey.generate()
        self.profile = runner.profile_for_test_nonproduction(
            self.host_keys, self.orchestrator_key
        )
        self.binding = {
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
            "trust_policy_digest": self.profile["trust_policy"]["digest_hex"],
        }
        self.inventory = runner.inventory_for_test_nonproduction(
            self.profile, self.binding
        )
        self.receipts = self._complete_receipts()

    def _complete_receipts(self) -> dict[str, list[dict[str, object]]]:
        receipts: dict[str, list[dict[str, object]]] = {}
        for host in ("host-a", "host-b", "host-c"):
            rows = []
            for sequence, fault in enumerate(self.profile["fault_matrix"], start=1):
                rows.append(
                    runner.sign_child_receipt_for_test_nonproduction(
                        profile=self.profile,
                        binding=self.binding,
                        host_id=host,
                        sequence=sequence,
                        fault_id=fault,
                        signing_key=self.host_keys[host],
                    )
                )
            receipts[host] = rows
        return receipts

    def _run(
        self,
        *,
        inventory: dict[str, object] | None = None,
        receipts: dict[str, list[dict[str, object]]] | None = None,
        claimed_root: str | None = None,
    ) -> dict[str, object]:
        executor = FakeSshExecutor(receipts or self.receipts)
        return runner.run_multi_host_qualification_for_test_nonproduction(
            profile=self.profile,
            inventory=inventory or self.inventory,
            binding=self.binding,
            executor=executor,
            timeout_seconds=1.0,
            claimed_aggregate_root=claimed_root,
        )

    def test_complete_fake_ssh_matrix_is_verified_but_never_production(self) -> None:
        report = self._run()
        self.assertEqual(report["distinct_physical_hosts"], 3)
        self.assertEqual(report["verified_child_receipts"], 39)
        self.assertFalse(report["multi_host_qualified"])
        self.assertEqual(report["evidence_tier"], "nonproduction-test")
        unsigned = {
            key: value
            for key, value in report.items()
            if key
            not in {
                "aggregate_signer_public_key",
                "aggregate_signer_fingerprint",
                "aggregate_signature",
            }
        }
        message = b"onebrain:p5:multi-host-production-aggregate:1\0" + blake3.blake3(
            runner._canonical_json(unsigned)
        ).digest()
        self.orchestrator_key.public_key().verify(
            bytes.fromhex(report["aggregate_signature"]), message
        )

    def test_every_ssh_control_is_bounded_and_signed_by_the_orchestrator(self) -> None:
        executor = FakeSshExecutor(self.receipts)
        runner.run_multi_host_qualification_for_test_nonproduction(
            profile=self.profile,
            inventory=self.inventory,
            binding=self.binding,
            executor=executor,
            timeout_seconds=1.0,
        )
        self.assertEqual(len(executor.calls), 3)
        for _, commands, _ in executor.calls:
            self.assertEqual(len(commands), 13)
            self.assertLessEqual(
                len(runner._canonical_json(commands)),
                self.profile["resource_bounds"]["max_control_message_bytes"],
            )
            for envelope in commands:
                payload = envelope["payload"]
                message = b"onebrain:p5:multi-host-control:1\0" + blake3.blake3(
                    runner._canonical_json(payload)
                ).digest()
                self.orchestrator_key.public_key().verify(
                    bytes.fromhex(envelope["signature"]), message
                )

    def test_pinned_ssh_host_key_mismatch_is_rejected(self) -> None:
        inventory = copy.deepcopy(self.inventory)
        inventory["hosts"][0]["observed_ssh_host_key_fingerprint"] = "ff" * 32
        with self.assertRaisesRegex(runner.P5OrchestrationError, "SSH host key"):
            self._run(inventory=inventory)

    def test_wrong_agent_executable_hash_is_rejected(self) -> None:
        receipts = copy.deepcopy(self.receipts)
        receipts["host-b"][0]["payload"]["agent_binary_digest"] = "fe" * 32
        with self.assertRaisesRegex(runner.P5OrchestrationError, "agent_binary_digest"):
            self._run(receipts=receipts)

    def test_timeout_and_partial_host_result_are_rejected(self) -> None:
        executor = FakeSshExecutor(self.receipts)
        executor.timeout_host = "host-b"
        with self.assertRaisesRegex(runner.P5OrchestrationError, "timed out"):
            runner.run_multi_host_qualification_for_test_nonproduction(
                profile=self.profile,
                inventory=self.inventory,
                binding=self.binding,
                executor=executor,
                timeout_seconds=0.01,
            )

        receipts = copy.deepcopy(self.receipts)
        receipts["host-c"] = []
        with self.assertRaisesRegex(runner.P5OrchestrationError, "partial host"):
            self._run(receipts=receipts)

    def test_real_process_capture_is_timeout_and_output_bounded(self) -> None:
        with self.assertRaisesRegex(runner.P5OrchestrationError, "response exceeds"):
            runner._run_bounded_process(
                [sys.executable, "-c", "import sys;sys.stdout.write('x'*17)"],
                b"",
                1.0,
                stdout_limit=16,
                stderr_limit=16,
            )
        with self.assertRaises(TimeoutError):
            runner._run_bounded_process(
                [sys.executable, "-c", "import time;time.sleep(1)"],
                b"",
                0.01,
                stdout_limit=16,
                stderr_limit=16,
            )

    def test_reordered_receipts_have_the_same_canonical_root(self) -> None:
        first = self._run()
        reordered = {host: list(reversed(rows)) for host, rows in self.receipts.items()}
        second = self._run(receipts=reordered)
        self.assertEqual(first["aggregate_root"], second["aggregate_root"])

    def test_duplicate_host_principal_root_or_receipt_key_is_rejected(self) -> None:
        for field in (
            "runner_identity",
            "durable_root_locator",
            "expected_principal",
            "receipt_signer_fingerprint",
        ):
            with self.subTest(field=field):
                inventory = copy.deepcopy(self.inventory)
                inventory["hosts"][1][field] = inventory["hosts"][0][field]
                with self.assertRaisesRegex(runner.P5OrchestrationError, "duplicate"):
                    self._run(inventory=inventory)

    def test_fault_omission_is_rejected(self) -> None:
        receipts = copy.deepcopy(self.receipts)
        receipts["host-c"] = receipts["host-c"][:-1]
        with self.assertRaisesRegex(runner.P5OrchestrationError, "fault matrix"):
            self._run(receipts=receipts)

    def test_resource_overflow_is_rejected(self) -> None:
        receipts = copy.deepcopy(self.receipts)
        receipts["host-a"][0]["payload"]["resource_observation"][
            "peak_rss_bytes"
        ] = self.profile["resource_bounds"]["max_peak_rss_bytes_per_host"] + 1
        self._resign(receipts["host-a"][0], self.host_keys["host-a"])
        with self.assertRaisesRegex(runner.P5OrchestrationError, "resource bound"):
            self._run(receipts=receipts)

    def test_claimed_aggregate_root_mismatch_is_rejected(self) -> None:
        with self.assertRaisesRegex(runner.P5OrchestrationError, "aggregate root"):
            self._run(claimed_root="00" * 32)

    def test_release_request_session_registry_and_compatibility_are_exact(self) -> None:
        for field in (
            "release_request_digest",
            "qualification_session_id",
            "registry_root",
            "linux_artifact_tuple_digest",
        ):
            with self.subTest(field=field):
                receipts = copy.deepcopy(self.receipts)
                receipts["host-a"][0]["payload"][field] = "ee" * 32
                self._resign(receipts["host-a"][0], self.host_keys["host-a"])
                with self.assertRaisesRegex(runner.P5OrchestrationError, field):
                    self._run(receipts=receipts)

    def test_valid_signature_from_unlisted_key_is_rejected(self) -> None:
        receipts = copy.deepcopy(self.receipts)
        unlisted = Ed25519PrivateKey.generate()
        public = unlisted.public_key().public_bytes_raw()
        receipt = receipts["host-a"][0]
        receipt["signer_public_key"] = public.hex()
        receipt["signer_fingerprint"] = blake3.blake3(
            public,
            derive_key_context=self.profile["trust_policy"]["fingerprint_context"],
        ).hexdigest()
        self._resign(receipt, unlisted)
        with self.assertRaisesRegex(runner.P5OrchestrationError, "allowlisted"):
            self._run(receipts=receipts)

    def _resign(
        self, receipt: dict[str, object], signing_key: Ed25519PrivateKey
    ) -> None:
        receipt["signature"] = signing_key.sign(
            runner.child_receipt_signature_message(receipt["payload"])
        ).hex()


if __name__ == "__main__":
    unittest.main()
