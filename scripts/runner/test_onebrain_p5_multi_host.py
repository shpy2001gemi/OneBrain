from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

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
            "toolchain_digest": "67" * 32,
            "runner_bundle_manifest_digest": "68" * 32,
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
        self.assertFalse(report["registry_production_qualified"])
        self.assertFalse(report["base_gate_v1_qualified"])
        self.assertEqual(report["limitations"], runner.REQUIRED_PRODUCTION_LIMITATIONS)
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

    def test_nonproduction_receipts_cannot_be_relabelled_as_production(self) -> None:
        receipts = copy.deepcopy(self.receipts)
        for rows in receipts.values():
            for receipt in rows:
                receipt["evidence_tier"] = "production-reference"
        executor = FakeSshExecutor(receipts)
        with (
            mock.patch.object(runner, "_validate_profile"),
            self.assertRaisesRegex(
                runner.P5OrchestrationError,
                "inventory evidence_tier|child receipt signature",
            ),
        ):
            runner._run_multi_host_qualification(
                profile=self.profile,
                inventory=self.inventory,
                binding=self.binding,
                executor=executor,
                timeout_seconds=1.0,
                production=True,
                control_signer=self.orchestrator_key,
            )

    def test_observe_only_matrix_cannot_claim_real_quic_qualification(self) -> None:
        inventory = copy.deepcopy(self.inventory)
        inventory["evidence_tier"] = "production-reference"
        self._resign_inventory(inventory)
        receipts = copy.deepcopy(self.receipts)
        for host_id, rows in receipts.items():
            for receipt in rows:
                receipt["evidence_tier"] = "production-reference"
                self._resign(receipt, self.host_keys[host_id])
        with mock.patch.object(runner, "_validate_profile"):
            report = runner._run_multi_host_qualification(
                profile=self.profile,
                inventory=inventory,
                binding=self.binding,
                executor=FakeSshExecutor(receipts),
                timeout_seconds=1.0,
                production=True,
                control_signer=self.orchestrator_key,
            )
        self.assertFalse(report["multi_host_qualified"])
        self.assertIn(
            "real-quic-ring-and-fault-injection-pending", report["limitations"]
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
        self._resign_inventory(inventory)
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
                self._resign_inventory(inventory)
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

    def test_registry_candidate_binding_hashes_real_bytes_and_rejects_mutation(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            obr = b"obr-candidate-bytes"
            labels = b"label-index"
            ccids = b"ccid-index"
            (root / "concepts.obr").write_bytes(obr)
            (root / "concepts.obr.labels.idx").write_bytes(labels)
            (root / "concepts.obr.ccids.idx").write_bytes(ccids)
            manifest = {
                "manifest_version": 1,
                "obr_blake3": blake3.blake3(obr).hexdigest(),
                "label_index": {
                    "blake3": blake3.blake3(labels).hexdigest(),
                    "file_size": len(labels),
                },
                "ccid_index": {
                    "blake3": blake3.blake3(ccids).hexdigest(),
                    "file_size": len(ccids),
                },
            }
            verification = {
                "file_size": len(obr),
                "obr_blake3": manifest["obr_blake3"],
                "label_index": manifest["label_index"],
                "ccid_index": manifest["ccid_index"],
            }
            (root / "concepts.obr.manifest.json").write_text(
                json.dumps(manifest), encoding="utf-8"
            )
            (root / "concepts.obr.verification.json").write_text(
                json.dumps(verification), encoding="utf-8"
            )

            measured = runner._registry_candidate_binding(root)
            self.assertEqual(measured["format"], "onebrain/p5-registry-candidate-binding/1")
            self.assertFalse(measured["registry_production_qualified"])
            self.assertEqual(len(measured["files"]), 5)
            self.assertEqual(len(measured["root"]), 64)

            (root / "concepts.obr.labels.idx").write_bytes(labels + b"tamper")
            with self.assertRaisesRegex(runner.P5OrchestrationError, "label index"):
                runner._registry_candidate_binding(root)

    def test_agent_signature_is_verified_not_only_hashed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            agent = root / "agent"
            signature = root / "agent.sig"
            agent.write_bytes(b"exact-agent")
            manifest_digest = "ab" * 32
            signature.write_bytes(
                self.orchestrator_key.sign(
                    runner.agent_signature_message(agent.read_bytes(), manifest_digest)
                )
            )
            digest = runner._verify_agent_signature(
                agent.read_bytes(), manifest_digest, signature, self.profile
            )
            self.assertEqual(digest, blake3.blake3(signature.read_bytes()).hexdigest())

            agent.write_bytes(b"mutated-agent")
            with self.assertRaisesRegex(runner.P5OrchestrationError, "agent signature"):
                runner._verify_agent_signature(
                    agent.read_bytes(), manifest_digest, signature, self.profile
                )
            with self.assertRaisesRegex(runner.P5OrchestrationError, "agent signature"):
                runner._verify_agent_signature(
                    b"exact-agent", "ac" * 32, signature, self.profile
                )

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux bundle-mode contract")
    def test_bundle_manifest_is_verified_without_executing_bundle_scripts(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            commit = "12" * 20
            tree = "34" * 20
            files = {
                "bin/p5_multi_host_agent": b"signed-agent-bytes",
                "metadata/BUILD-PROVENANCE.json": b"{\"build\":true}\n",
                "metadata/candidate-commit.txt": (commit + "\n").encode(),
                "metadata/candidate-tree.txt": (tree + "\n").encode(),
                "scripts/verify.sh": b"#!/bin/sh\ntouch verifier-was-executed\n",
            }
            for relative, bytes_value in files.items():
                path = root.joinpath(*relative.split("/"))
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(bytes_value)
                path.chmod(0o555 if relative.startswith(("bin/", "scripts/")) else 0o444)
            sums = b"".join(
                f"{hashlib.sha256(files[path]).hexdigest()}  {path}\n".encode()
                for path in sorted(files)
            )
            sums_path = root / "metadata" / "SHA256SUMS"
            sums_path.write_bytes(sums)
            sums_path.chmod(0o444)
            files["metadata/SHA256SUMS"] = sums
            rows = []
            for relative in sorted(files):
                bytes_value = files[relative]
                rows.append(
                    {
                        "blake3": blake3.blake3(bytes_value).hexdigest(),
                        "mode": "0555" if relative.startswith(("bin/", "scripts/")) else "0444",
                        "path": relative,
                        "sha256": hashlib.sha256(bytes_value).hexdigest(),
                        "size": len(bytes_value),
                    }
                )
            provenance = files["metadata/BUILD-PROVENANCE.json"]
            manifest = {
                "build": {
                    "digest": hashlib.sha256(provenance).hexdigest(),
                    "platform": "linux/x64",
                    "source_date_epoch": 1,
                },
                "candidate": {
                    "id": commit,
                    "source_digest": "56" * 32,
                    "version": tree,
                },
                "files": rows,
                "format": "onebrain/base-v1-native-runner-bundle/1",
                "private_material_included": False,
                "qualification_tier": "prepared-not-production-qualified",
                "required_runtime": {
                    "architecture": "x64",
                    "minimum_glibc": "2.39",
                    "os": "linux",
                },
            }
            manifest_path = root / "metadata" / "bundle.manifest.json"
            manifest_path.write_bytes(runner._canonical_json(manifest))
            manifest_path.chmod(0o444)
            digest, agent_bytes = runner._bundle_manifest_binding(
                root,
                root / "bin" / "p5_multi_host_agent",
                candidate_commit=commit,
                candidate_tree=tree,
            )
            self.assertEqual(digest, blake3.blake3(manifest_path.read_bytes()).hexdigest())
            self.assertEqual(agent_bytes, b"signed-agent-bytes")
            self.assertFalse((root / "verifier-was-executed").exists())
            (root / "scripts" / "verify.sh").chmod(0o755)
            (root / "scripts" / "verify.sh").write_bytes(b"tampered")
            with self.assertRaisesRegex(runner.P5OrchestrationError, "file differs"):
                runner._bundle_manifest_binding(
                    root,
                    root / "bin" / "p5_multi_host_agent",
                    candidate_commit=commit,
                    candidate_tree=tree,
                )

    def test_inventory_and_every_receipt_bind_topology_evidence_and_limitations(self) -> None:
        self.assertEqual(
            self.inventory["limitations"], runner.REQUIRED_PRODUCTION_LIMITATIONS
        )
        self.assertIn("signature", self.inventory)
        for host in self.inventory["hosts"]:
            for field in (
                "physical_machine_fingerprint",
                "host_evidence_sha256",
                "placement_evidence_sha256",
            ):
                self.assertEqual(len(host[field]), 64)
        for receipts in self.receipts.values():
            for receipt in receipts:
                payload = receipt["payload"]
                self.assertEqual(payload["limitations"], runner.REQUIRED_PRODUCTION_LIMITATIONS)
                self.assertIn("physical_machine_fingerprint", payload)
                self.assertIn("host_evidence_sha256", payload)
                self.assertIn("placement_evidence_sha256", payload)

        tampered = copy.deepcopy(self.inventory)
        tampered["hosts"][0]["placement_evidence_sha256"] = "ff" * 32
        with self.assertRaisesRegex(runner.P5OrchestrationError, "inventory signature"):
            self._run(inventory=tampered)

    def test_production_ssh_executor_uses_inventory_bound_port(self) -> None:
        host = {
            "physical_host_id": "host-a",
            "ssh_destination": "runner-a@host-a.example",
            "ssh_port": 10041,
            "known_hosts_file": "/controller/known_hosts",
            "agent_command": "/opt/onebrain/run-p5-agent",
        }
        with mock.patch.object(
            runner,
            "_run_bounded_process",
            return_value=(0, b"[]", b""),
        ) as process:
            self.assertEqual(runner.OpenSshExecutor().run(host, [], 3.0), [])
        command = process.call_args.args[0]
        self.assertEqual(command[command.index("-p") + 1], "10041")
        self.assertLess(command.index("-p"), command.index(host["ssh_destination"]))

        for invalid in (True, 0, 65536, "10041"):
            with self.subTest(invalid=invalid):
                rejected = dict(host, ssh_port=invalid)
                with self.assertRaisesRegex(
                    runner.P5OrchestrationError, "SSH port"
                ):
                    runner.OpenSshExecutor().run(rejected, [], 3.0)

    def test_inventory_preparation_measures_topology_and_writes_closed_configs(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            rows = []
            expected_machine_fingerprints = set()
            roles = runner._role_bindings(self.profile)
            for index, host_id in enumerate(("host-a", "host-b", "host-c"), start=1):
                runner_id = f"runner-{host_id[-1]}"
                placement = {
                    "account_scope_digest": f"{index:064x}",
                    "collected_at": "2026-08-14T00:00:00Z",
                    "collector_identity": "onebrain-owner-telephone-verifier-v1",
                    "instance_id": f"instance-{host_id}",
                    "physical_host_id": f"owner-phone-attested:{index:064x}",
                    "placement_group": f"region-{index}",
                    "provider": f"provider-{index}",
                    "receipt_sha256": f"{index + 10:064x}",
                    "receipt_verified": True,
                }
                host_evidence = {
                    "format": "onebrain/host-evidence/1",
                    "runner_id": runner_id,
                    "machine_id_sha256": {"status": "ok", "value": f"{index + 20:064x}"},
                    "virtualization": {"status": "ok", "value": "kvm"},
                    "placement": placement,
                }
                evidence_path = root / f"{host_id}-evidence.json"
                placement_path = root / f"{host_id}-placement.json"
                evidence_path.write_bytes(runner._canonical_json(host_evidence))
                placement_path.write_bytes(runner._canonical_json(placement))
                expected_machine_fingerprints.add(
                    runner._physical_machine_fingerprint(host_evidence, placement)
                )
                receipt_role = f"p5-host:{host_id}"
                rows.append(
                    {
                        "physical_host_id": host_id,
                        "runner_identity": runner_id,
                        "ssh_host_key_algorithm": "ssh-ed25519",
                        "ssh_host_key_fingerprint": f"{index + 30:064x}",
                        "observed_ssh_host_key_fingerprint": f"{index + 30:064x}",
                        "receipt_role": receipt_role,
                        "receipt_signer_fingerprint": roles[receipt_role]["fingerprint_hex"],
                        "durable_root_locator": f"/var/lib/onebrain/{runner_id}",
                        "expected_principal": f"{index + 40:064x}",
                        "ssh_destination": f"{runner_id}@host-{index}.example",
                        "ssh_port": 10000 + index,
                        "known_hosts_file": f"/controller/known-hosts-{host_id}",
                        "agent_command": f"/controller/run-{host_id}",
                        "host_evidence_path": str(evidence_path),
                        "placement_evidence_path": str(placement_path),
                        "remote_agent_signature_path": "/etc/onebrain/p5-agent.sig",
                    }
                )
            host_spec = root / "hosts.json"
            host_spec.write_bytes(
                runner._canonical_json(
                    {"format": "onebrain/p5-production-host-spec/1", "hosts": rows}
                )
            )
            registry = {
                "format": "onebrain/p5-registry-candidate-binding/1",
                "registry_production_qualified": False,
                "root": self.binding["registry_root"],
                "files": [],
            }
            output_root = root / "configs"
            with (
                mock.patch.object(runner, "_profile", return_value=self.profile),
                mock.patch.object(
                    runner,
                    "_derive_verified_binding",
                    return_value=(self.binding, registry),
                ),
            ):
                inventory = runner._prepare_signed_inventory(
                    args=mock.Mock(),
                    host_spec_path=host_spec,
                    signing_key=self.orchestrator_key,
                    config_output_root=output_root,
                )
            self.assertEqual(inventory["limitations"], runner.REQUIRED_PRODUCTION_LIMITATIONS)
            self.assertFalse(inventory["registry_candidate"]["registry_production_qualified"])
            self.assertEqual(
                {row["physical_machine_fingerprint"] for row in inventory["hosts"]},
                expected_machine_fingerprints,
            )
            unsigned = {
                field: inventory[field] for field in runner.INVENTORY_UNSIGNED_FIELDS
            }
            self.orchestrator_key.public_key().verify(
                bytes.fromhex(inventory["signature"]),
                runner.inventory_signature_message(unsigned),
            )
            for host_id in ("host-a", "host-b", "host-c"):
                config_path = output_root / f"{host_id}-agent-config.json"
                config = json.loads(config_path.read_bytes())
                self.assertEqual(config["binding"], self.binding)
                self.assertEqual(config["evidence_tier"], "production-reference")
                self.assertEqual(config["limitations"], runner.REQUIRED_PRODUCTION_LIMITATIONS)
            with self.assertRaises(FileExistsError):
                runner._write_atomic(
                    output_root / "host-a-agent-config.json", b"must-not-clobber\n"
                )

    def _resign(
        self, receipt: dict[str, object], signing_key: Ed25519PrivateKey
    ) -> None:
        receipt["signature"] = signing_key.sign(
            runner.child_receipt_signature_message(
                str(receipt["evidence_tier"]), receipt["payload"]
            )
        ).hex()

    def _resign_inventory(self, inventory: dict[str, object]) -> None:
        unsigned = {
            field: inventory[field] for field in runner.INVENTORY_UNSIGNED_FIELDS
        }
        inventory["signature"] = self.orchestrator_key.sign(
            runner.inventory_signature_message(unsigned)
        ).hex()


if __name__ == "__main__":
    unittest.main()
