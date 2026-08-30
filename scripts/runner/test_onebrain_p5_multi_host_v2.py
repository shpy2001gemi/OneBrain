from __future__ import annotations

import importlib.util
import io
import json
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.asymmetric.x25519 import X25519PrivateKey


MODULE_PATH = Path(__file__).with_name("onebrain-p5-multi-host-v2.py")
SPEC = importlib.util.spec_from_file_location("onebrain_p5_multi_host_v2", MODULE_PATH)
assert SPEC and SPEC.loader
runner = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = runner
SPEC.loader.exec_module(runner)


def _authority() -> dict[str, object]:
    return {
        "inventory_blake3": "11" * 32,
        "public_probe_set_blake3": "22" * 32,
        "topology_attestation_blake3": "33" * 32,
        "provider_evidence_blake3": "44" * 32,
        "provider_evidence_status": "owner-telephone-verified-provider-document-pending",
        "qualification_tier": "production-reference",
    }


def _route(source: str, target: str, path: str, *, failed: bool = False) -> dict[str, object]:
    row: dict[str, object] = {
        "from": source,
        "to": target,
        "authenticated_peer": target,
        "path_kind": path,
        "checkpoint": {"sequence": 7, "intent": "55" * 32, "roots": "66" * 32},
        "faults": list(runner.REQUIRED_FAULTS),
    }
    if failed:
        row["failover"] = {
            "selected_relay": "relay-a",
            "alternate_relay": "relay-c",
            "selected_reservation_issued_at": 10,
            "alternate_reservation_issued_at": 11,
            "failure_observed_at": 20,
            "prior_binding": "77" * 32,
            "resumed_binding": "88" * 32,
            "prior_session": "99" * 32,
            "resumed_session": "aa" * 32,
            "resumed_checkpoint": {"sequence": 8, "intent": "55" * 32, "roots": "66" * 32},
        }
    return row


def _aggregate() -> dict[str, object]:
    routes = [
        _route("host-a", "host-b", "relay-tcp-443"),
        _route("host-b", "host-c", "relay-tcp-443", failed=True),
        _route("host-c", "host-a", "relay-tcp-443"),
    ]
    return {
        "format": 2,
        "request_digest": "ab" * 32,
        "session_id": "bc" * 32,
        "evidence_authority": _authority(),
        "routes": routes,
        "cleanup_complete": True,
        "resource_bounds": True,
        "preflight_only": False,
        "transport": "real-obp",
    }


def _probe_receipts() -> list[dict[str, object]]:
    rows = []
    issued_at = int(time.time()) - 10
    for descriptor, relay, sources in (
        ("01", "host-a", ("host-b", "host-c")),
        ("02", "host-c", ("host-a", "host-b")),
    ):
        for source in sources:
            rows.append({
                "descriptor_expires_at": issued_at + 600,
                "descriptor_issued_at": issued_at,
                "probes": [{"success": True}],
                "relay_descriptor_hex": descriptor,
                "relay_host_id": relay,
                "relay_node_id": ("a1" if relay == "host-a" else "c1") * 32,
                "source_host_id": source,
            })
    return rows


class FakeAgent:
    def __init__(self, host_id: str, response: bytes | BaseException) -> None:
        self.host_id = host_id
        self.response = response
        self.terminated = False
        self.killed = False
        self.closed = False

    def execute(self, command, deadline_monotonic_ns):
        if isinstance(self.response, BaseException):
            raise self.response
        return self.response

    def terminate(self): self.terminated = True
    def wait(self, timeout): return 0
    def kill(self): self.killed = True
    def close(self): self.closed = True


class P5MultiHostV2Tests(unittest.TestCase):
    @mock.patch("scripts.release.create_base_release_request.verify_task28_release_request")
    def test_base_authority_uses_task28_v2_verifier(self, verify_task28) -> None:
        args = SimpleNamespace(
            release_request=Path("release-request.json"),
            release_signature=Path("release-request.json.asc"),
            base_policy=Path("base-v1-release-signers-v1.json"),
            base_gpg_home=Path("qualification-approver-gnupg"),
        )

        runner.verify_base_authority(args)

        verify_task28.assert_called_once_with(
            args.release_request,
            args.release_signature,
            args.base_policy,
            gpg_home=args.base_gpg_home,
        )

    def test_inventory_known_hosts_uses_plain_default_port_and_bracketed_nondefault_port(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            inventory = {
                "hosts": [
                    {
                        "host_id": host_id,
                        "runner_id": f"runner-{suffix}",
                        "ssh_destination": f"controller@192.0.2.{index}",
                        "ssh_port": port,
                        "ssh_host_public_key": "ssh-ed25519 AAAA",
                        "receipt_public_key": "11" * 32,
                    }
                    for index, (host_id, suffix, port) in enumerate(
                        (("host-a", "a", 10041), ("host-b", "b", 22), ("host-c", "c", 22)),
                        start=1,
                    )
                ],
            }

            _, _, known_hosts = runner._inventory_host_configs(inventory, root)

            self.assertEqual(known_hosts["host-a"].read_text(), "[192.0.2.1]:10041 ssh-ed25519 AAAA\n")
            self.assertEqual(known_hosts["host-b"].read_text(), "192.0.2.2 ssh-ed25519 AAAA\n")
            self.assertEqual(known_hosts["host-c"].read_text(), "192.0.2.3 ssh-ed25519 AAAA\n")

    def test_bootstrap_frame_is_inventory_bound_signed_and_effect_free(self) -> None:
        controller = Ed25519PrivateKey.generate()
        public = controller.public_key().public_bytes_raw().hex()
        host = runner.HostConfigV2("host-a", "runner-a", "example.test", 22, "runner", "admin", "ssh-ed25519 AAAA", "x", b"a" * 32, b"b" * 32, "/e")
        inventory = {
            "controller_application_public_key": public,
            "hosts": [{"host_id": "host-a", "identity_public_key": "11" * 32, "receipt_public_key": "22" * 32, "previous_generation": "/opt/onebrain/base-v1/" + "aa" * 32}],
            "provider_evidence": [{"host_id": "host-a"}],
            "provider_evidence_status": "owner-telephone-verified-provider-document-pending",
            "public_probe_sets": [{"host_id": "host-a"}],
            "qualification_tier": "production-reference",
            "topology_attestation": {"format": 2},
        }
        request = {
            "expires_at": int(time.time()) + 600,
            "profile_blake3": "33" * 32,
            "session_id": "44" * 32,
            "vector_blake3": "55" * 32,
        }
        manifest = runner.canonical_json({"candidate": {"id": "66" * 20, "version": "77" * 20}})
        p5_request = runner.canonical_json(request)
        p5_policy = runner.canonical_json({"format": 2, "public_key": "88" * 32, "role": "p5-run-approver", "signing_domain": "onebrain/p5/run-request/v2", "valid_from": 0, "valid_until": request["expires_at"]})
        command = runner._signed_bootstrap_command(
            host, inventory, request, controller,
            release_request=b"release", release_signature=b"release-signature",
            base_policy=b"base-policy", base_keyring=b"base-keyring",
            p5_request=p5_request, p5_signature=b"p5-signature",
            p5_approval_policy=p5_policy, bundle_manifest=manifest,
        )
        frame = json.loads(command.canonical_bytes)
        signature = bytes.fromhex(frame.pop("signature"))
        controller.public_key().verify(signature, runner.BOOTSTRAP_DOMAIN + runner.canonical_json(frame))
        self.assertEqual(frame["kind"], "bootstrap")
        self.assertEqual(
            frame["expires_at"] - frame["issued_at"],
            runner.BOOTSTRAP_REMOTE_FUTURE_LIMIT_SECONDS - runner.BOOTSTRAP_MAX_CLOCK_SKEW_SECONDS,
        )
        self.assertEqual(frame["session_config"]["controller_application_public_key"], public)
        self.assertEqual(frame["session_config"]["candidate_commit"], "66" * 20)
        self.assertEqual(frame["session_config"]["evidence_authority"]["provider_evidence_status"], "owner-telephone-verified-provider-document-pending")
        self.assertEqual(frame["session_config"]["inventory_blake3"], frame["session_config"]["evidence_authority"]["inventory_blake3"])
        digest = runner.blake3.blake3(runner.canonical_json(frame["session_config"])).hexdigest()
        response = runner.canonical_json({
            "format": 2, "host_id": "host-a", "installed_config_blake3": digest,
            "network_changed": False, "operation_id": frame["operation_id"], "units_changed": False,
        }) + b"\n"
        self.assertEqual(runner.verify_bootstrap_response("host-a", response, digest)["operation_id"], frame["operation_id"])
        forged = json.loads(response); forged["units_changed"] = True
        with self.assertRaises(runner.P5ExecutionError):
            runner.verify_bootstrap_response("host-a", runner.canonical_json(forged), digest)
        cleanup_digest = "99" * 32
        finalization = runner.canonical_json({
            "cleanup_receipt_blake3": cleanup_digest, "format": 2, "host_id": "host-a",
            "operation": {"command_count": 2}, "session_config_removed": True, "signer_stopped": True,
        }) + b"\n"
        self.assertTrue(runner.verify_finalization_response("host-a", finalization, cleanup_digest)["signer_stopped"])

    def test_production_preflight_drives_relay_only_ring_and_bidirectional_markers(self) -> None:
        class FakeWaveExecutor:
            waves: list[list[str]] = []
            wave_sequences: list[list[int]] = []
            ring_roles: list[list[str]] = []

            def __init__(self, evidence_root, *, verify_child_receipt):
                self.evidence_root = evidence_root

            def start_agents(self, hosts, credentials, deadline):
                return tuple(FakeAgent(host.host_id, b"") for host in hosts)

            def execute_bootstrap_wave(self, hosts, commands, credentials, deadline):
                return tuple({"format": 2, "host_id": host.host_id} for host in hosts)

            def execute_admin_wave(self, hosts, commands, credentials, keys, deadline):
                return tuple({"receipt": {"host_id": host.host_id}} for host in hosts)

            def execute_finalization_wave(self, hosts, commands, cleanup_digests, credentials, deadline):
                return tuple({"format": 2, "host_id": host.host_id} for host in hosts)

            def execute_wave(self, agents, commands, deadline):
                rows = []
                names = []
                sequences = []
                roles = []
                for command in commands:
                    frame = json.loads(command.canonical_bytes)
                    name = frame["command"]
                    names.append(name)
                    sequences.append(frame["sequence"])
                    host_id = frame["host_id"]
                    parameters = frame["parameters"]
                    if name in ("connect-ring-edge", "reconnect-ring-edge"):
                        roles.append(parameters["ring_role"])
                    node = {"host-a": "aa" * 32, "host-b": "bb" * 32, "host-c": "cc" * 32}[host_id]
                    result = {"accepted": True, "command": name}
                    if name == "status": result["network_started"] = True
                    elif name == "start-reachability": result.update(bind="0.0.0.0:41010", local_node_id=node)
                    elif name == "diagnose-relay-matrix": result.update(
                        success=True,
                        probes=[
                            {
                                "descriptor_blake3": runner.blake3.blake3(bytes.fromhex(descriptor)).hexdigest(),
                                "relay_node_id": "dd" * 32,
                                "success": True,
                            }
                            for descriptor in parameters["relay_descriptors"]
                        ],
                    )
                    elif name == "ensure-reservations": result["grant_digests"] = ["11" * 32, "22" * 32]
                    elif name == "publish-advertisement": result.update(
                        advertisement_hex={"host-a": "01", "host-b": "02", "host-c": "03"}[host_id],
                        peer_node_id=node,
                        peer_public_key={"host-a": "1a" * 32, "host-b": "1b" * 32, "host-c": "1c" * 32}[host_id],
                        reflexive_observations=["aa"],
                        reservation_count=2,
                    )
                    elif name == "arm-direct-inbound": result.update(
                        expected_peer=parameters["expected_peer"],
                        observation_blake3="21" * 32,
                    )
                    elif name == "connect-ring": result.update(
                        outgoing={"expected_peer": parameters["outgoing_expected_peer"], "path_kind": "RelayTcp443", "route_receipt_blake3": "31" * 32, "session_id": "41" * 32},
                        incoming={"expected_peer": parameters["incoming_expected_peer"], "path_kind": "RelayTcp443", "route_receipt_blake3": "32" * 32, "session_id": "42" * 32},
                    )
                    elif name in ("connect-ring-edge", "reconnect-ring-edge"):
                        role = parameters["ring_role"]
                        result["role"] = role
                        if role != "idle":
                            result["route"] = {
                                "expected_peer": parameters["expected_peer"],
                                "path_kind": "RelayTcp443",
                                "route_receipt_blake3": "31" * 32,
                                "session_id": "41" * 32,
                            }
                    elif name in ("deliver-marker", "receive-marker"):
                        result.update(marker_blake3=parameters.get("expected_blake3", "51" * 32), marker_bytes=parameters.get("expected_bytes", 8))
                    receipt = {"format": 2, "host_id": host_id, "sequence": frame["sequence"], "result": result, "signature": "11" * 64}
                    rows.append(runner.SignedChildReceiptV2(host_id, frame["sequence"], runner.canonical_json(receipt)))
                self.waves.append(names)
                self.wave_sequences.append(sequences)
                if roles:
                    self.ring_roles.append(roles)
                return tuple(rows)

            @staticmethod
            def close_agents(agents):
                for agent in agents:
                    agent.close()

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            inventory_path = root / "inventory.json"
            inventory_path.write_text(json.dumps({
                "format": 2,
                "provider_evidence_status": "owner-telephone-verified-provider-document-pending",
                "provider_evidence": [{"host_id": host} for host in runner.REQUIRED_HOSTS],
                "public_probe_sets": _probe_receipts(),
                "qualification_tier": "production-reference",
                "topology_attestation": {"format": 2},
            }), encoding="utf-8")
            controller_key = root / "controller.key"
            controller_key.write_bytes(Ed25519PrivateKey.generate().private_bytes_raw())
            hosts = tuple(
                runner.HostConfigV2(host, "runner-" + host[-1], "example.test", 22, "runner", "admin", "ssh-ed25519 AAAA", "x", b"a" * 32, b"b" * 32, "/e")
                for host in runner.REQUIRED_HOSTS
            )
            args = SimpleNamespace(
                controller_signing_key=controller_key,
                evidence_root=root / "evidence",
                inventory=inventory_path,
                ssh_identity_key=root / "ssh.key",
            )
            request = {"expires_at": int(time.time()) + 600, "session_id": "61" * 32}
            material = {name: b"x" for name in ("release_request", "release_signature", "base_policy", "base_keyring", "p5_request", "p5_signature", "p5_approval_policy", "bundle_manifest")}
            bootstrap_command = lambda host, *args, **kwargs: runner.CanonicalCommandV2.create(1, {"host_id": host.host_id})
            with mock.patch.object(runner, "_inventory_host_configs", return_value=(hosts, {host.host_id: b"r" * 32 for host in hosts}, {host.host_id: root / host.host_id for host in hosts})), mock.patch.object(runner, "_bootstrap_material", return_value=material), mock.patch.object(runner, "_signed_bootstrap_command", side_effect=bootstrap_command), mock.patch.object(runner, "OpenSshWaveExecutor", FakeWaveExecutor):
                runner.run_production_preflight(args, request)
            output = json.loads((args.evidence_root / "p5" / "production-preflight.json").read_text(encoding="utf-8"))
            self.assertFalse(output["preflight_only"])
            self.assertEqual(set(output["ring"]), set(runner.REQUIRED_HOSTS))
            self.assertEqual(
                FakeWaveExecutor.waves,
                [["status"] * 3, ["start-reachability"] * 3, ["diagnose-relay-matrix"] * 3,
                 ["ensure-reservations"] * 3,
                 ["publish-advertisement"] * 3,
                 ["connect-ring-edge"] * 3, ["connect-ring-edge"] * 3,
                 ["connect-ring-edge"] * 3,
                 ["deliver-marker"] * 3, ["receive-marker"] * 3, ["status"] * 3,
                 ["shutdown"] * 3],
            )
            self.assertEqual(
                FakeWaveExecutor.wave_sequences[5:8],
                [[6] * 3, [7] * 3, [8] * 3],
            )
            self.assertEqual(
                FakeWaveExecutor.ring_roles,
                [
                    ["outbound", "inbound", "idle"],
                    ["idle", "outbound", "inbound"],
                    ["inbound", "idle", "outbound"],
                ],
            )

    def test_relay_matrix_reports_exact_host_relay_failure(self) -> None:
        descriptors = ("01", "02")
        good_probes = [
            {
                "descriptor_blake3": runner.blake3.blake3(bytes.fromhex(value)).hexdigest(),
                "relay_node_id": "aa" * 32,
                "success": True,
            }
            for value in descriptors
        ]
        matrix = {
            host: {"success": True, "probes": [dict(probe) for probe in good_probes]}
            for host in runner.REQUIRED_HOSTS
        }
        runner._require_relay_matrix(matrix, descriptors)
        matrix["host-b"]["success"] = False
        matrix["host-b"]["probes"][1].update(
            success=False,
            error="Discovery(PossessionFailed { endpoint_index: 1, transport: Tcp443, reason: Handshake })",
        )
        with self.assertRaisesRegex(runner.P5ExecutionError, "host-b.*Handshake"):
            runner._require_relay_matrix(matrix, descriptors)

    def test_outbound_first_ring_accepts_all_relay_and_rejects_direct(self) -> None:
        relay = {
            host: {"outgoing": {"path_kind": "RelayUdp"}}
            for host in runner.REQUIRED_HOSTS
        }
        direct = {
            host: {"outgoing": {"path_kind": "Direct"}}
            for host in runner.REQUIRED_HOSTS
        }
        runner._require_relay_ring(relay)
        with self.assertRaises(runner.P5ExecutionError):
            runner._require_relay_ring(direct)

    def test_relay_descriptor_inventory_rejects_missing_duplicate_and_noncanonical_hex(self) -> None:
        with self.assertRaises(runner.P5ExecutionError):
            runner._inventory_relay_descriptors({"public_probe_sets": []})
        with self.assertRaises(runner.P5ExecutionError):
            runner._inventory_relay_descriptors({"public_probe_sets": [{"relay_descriptor_hex": "aa"}, {"relay_descriptor_hex": "aa"}]})
        with self.assertRaises(runner.P5ExecutionError):
            runner._inventory_relay_descriptors({"public_probe_sets": [{"relay_descriptor_hex": "AA"}, {"relay_descriptor_hex": "bb"}]})
        self.assertEqual(runner._inventory_relay_descriptors({"public_probe_sets": _probe_receipts()}), ("01", "02"))

    def test_relay_descriptor_freshness_fails_before_remote_waves(self) -> None:
        inventory = {"public_probe_sets": _probe_receipts()}
        expires_at = int(inventory["public_probe_sets"][0]["descriptor_expires_at"])
        runner._require_relay_descriptor_freshness(inventory, expires_at - 180, 180)
        with self.assertRaisesRegex(runner.P5ExecutionError, "freshness window"):
            runner._require_relay_descriptor_freshness(inventory, expires_at - 179, 180)

    def test_relay_descriptor_validity_must_match_across_probe_receipts(self) -> None:
        receipts = _probe_receipts()
        receipts[1]["descriptor_expires_at"] = int(receipts[1]["descriptor_expires_at"]) - 1
        with self.assertRaisesRegex(runner.P5ExecutionError, "validity metadata"):
            runner._inventory_relay_descriptors({"public_probe_sets": receipts})

    def test_relay_descriptor_validity_accepts_1800_seconds_and_rejects_more(self) -> None:
        receipts = _probe_receipts()
        for receipt in receipts:
            receipt["descriptor_expires_at"] = int(receipt["descriptor_issued_at"]) + 1_800
        self.assertEqual(
            runner._inventory_relay_descriptors({"public_probe_sets": receipts}),
            ("01", "02"),
        )
        for receipt in receipts:
            receipt["descriptor_expires_at"] = int(receipt["descriptor_issued_at"]) + 1_801
        with self.assertRaisesRegex(runner.P5ExecutionError, "bounded descriptor validity"):
            runner._inventory_relay_descriptors({"public_probe_sets": receipts})

    def test_relay_probe_without_descriptor_validity_is_rejected(self) -> None:
        receipts = _probe_receipts()
        for receipt in receipts:
            receipt.pop("descriptor_issued_at")
            receipt.pop("descriptor_expires_at")
        with self.assertRaisesRegex(runner.P5ExecutionError, "bounded descriptor validity"):
            runner._inventory_relay_descriptors({"public_probe_sets": receipts})

    def test_admin_response_requires_inventory_receipt_key_and_exact_signature(self) -> None:
        key = Ed25519PrivateKey.generate()
        public = key.public_key().public_bytes_raw()
        authority = _authority()
        receipt = {
            "accepted": True,
            "action": "observe",
            "evidence_authority": authority,
            "fault": "network-partition",
            "format": 2,
            "frame_blake3": "31" * 32,
            "host_id": "host-a",
            "inventory_blake3": authority["inventory_blake3"],
            "observation": {"phase": "Before"},
            "request_digest": "32" * 32,
            "sequence": 1,
            "session_id": "33" * 32,
        }
        signature = key.sign(b"onebrain/p5/admin-operation-receipt/v2" + runner.canonical_json(receipt))
        encoded = runner.canonical_json({"receipt": receipt, "signature": signature.hex(), "signer_public_key": public.hex()}) + b"\n"
        self.assertEqual(runner.verify_admin_response("host-a", encoded, public)["receipt"], receipt)
        with self.assertRaises(runner.P5ExecutionError):
            runner.verify_admin_response("host-b", encoded, public)
        forged = json.loads(encoded)
        forged["receipt"]["sequence"] = 2
        with self.assertRaises(runner.P5ExecutionError):
            runner.verify_admin_response("host-a", runner.canonical_json(forged), public)

    def test_child_receipt_repeats_request_inventory_and_provider_authority(self) -> None:
        key = Ed25519PrivateKey.generate()
        public = key.public_key().public_bytes_raw()
        authority = _authority()
        unsigned = {
            "command_blake3": "10" * 32,
            "evidence_authority": authority,
            "format": 2,
            "host_id": "host-a",
            "inventory_blake3": authority["inventory_blake3"],
            "issued_at": int(time.time()),
            "request_digest": "ab" * 32,
            "result": {"accepted": True, "command": "status"},
            "sequence": 1,
            "session_id": "bc" * 32,
            "signer_public_key": public.hex(),
        }
        signature = key.sign(b"onebrain/p5/child-receipt/v2" + runner.canonical_json({k: v for k, v in unsigned.items() if k != "signer_public_key"}))
        # The signer public key and signature are appended after the signer has
        # authenticated the exact unsigned receipt emitted by the Rust agent.
        encoded = runner.canonical_json({**unsigned, "signature": signature.hex()})
        verifier = runner._production_receipt_verifier(
            {"host-a": public}, authority, "ab" * 32,
        )
        self.assertEqual(verifier("host-a", encoded).host_id, "host-a")
        forged = json.loads(encoded)
        forged["evidence_authority"]["provider_evidence_status"] = "unknown"
        with self.assertRaises(runner.P5ExecutionError):
            verifier("host-a", runner.canonical_json(forged))

    def test_fault_cycle_uses_agent_measured_targets_and_binds_all_three_admin_phases(self) -> None:
        class FaultExecutor:
            admin_frames: list[dict[str, object]] = []
            agent_frames: list[dict[str, object]] = []

            def execute_wave(self, agents, commands, deadline):
                receipts = []
                for command in commands:
                    frame = json.loads(command.canonical_bytes)
                    self.agent_frames.append(frame)
                    if frame["command"] == "prepare-fault-target":
                        result = {
                            "command": frame["command"],
                            "target": {
                                "peer_endpoints": ["203.0.113.9:41000"],
                                "selected_relay": "51" * 32,
                            },
                        }
                    else:
                        result = {
                            "command": frame["command"],
                            "fault": frame["parameters"]["fault"],
                            "phase": frame["parameters"]["phase"],
                            "roots": {
                                "canonical_root": "11" * 32,
                                "journal_root": "22" * 32,
                                "outbox_root": "33" * 32,
                                "operational_root": "44" * 32,
                            },
                        }
                    receipt = {"host_id": frame["host_id"], "result": result, "sequence": frame["sequence"]}
                    receipts.append(runner.SignedChildReceiptV2(frame["host_id"], frame["sequence"], runner.canonical_json(receipt)))
                return tuple(receipts)

            def execute_admin_wave(self, hosts, commands, credentials, keys, deadline):
                responses = []
                for command in commands:
                    frame = json.loads(command.canonical_bytes)
                    self.admin_frames.append(frame)
                    responses.append({
                        "receipt": {
                            "action": frame["action"],
                            "fault": frame["fault"],
                            "host_id": frame["host_id"],
                            "sequence": frame["sequence"],
                        },
                        "signature": "55" * 64,
                        "signer_public_key": "66" * 32,
                    })
                return tuple(responses)

        hosts = tuple(
            runner.HostConfigV2(host, "runner-" + host[-1], "example.test", 22, "runner", "admin", "ssh-ed25519 AAAA", "x", b"a" * 32, b"b" * 32, "/e")
            for host in runner.REQUIRED_HOSTS
        )
        agents = tuple(FakeAgent(host.host_id, b"") for host in hosts)
        key = Ed25519PrivateKey.generate()
        request = {"expires_at": int(time.time()) + 600, "session_id": "71" * 32}
        executor = FaultExecutor()
        _, next_agent, next_admin, evidence = runner.execute_fault_cycle(
            executor,
            hosts,
            agents,
            runner.ControllerCredentialsV2(Path("app"), Path("ssh"), {}),
            {host.host_id: b"r" * 32 for host in hosts},
            request,
            key,
            "partition",
            {"host-a": "aa" * 32, "host-b": "bb" * 32, "host-c": "cc" * 32},
            agent_sequence=9,
            admin_sequence=2,
            deadline_monotonic_ns=time.monotonic_ns() + 1_000_000_000,
        )
        self.assertEqual(next_agent, 13)
        self.assertEqual(next_admin, 5)
        self.assertEqual([frame["action"] for frame in executor.admin_frames], ["observe"] * 3 + ["apply"] * 3 + ["clear"] * 3)
        self.assertEqual([frame["parameters"]["peer_endpoints"] for frame in executor.admin_frames], [["203.0.113.9:41000"]] * 9)
        self.assertEqual(set(evidence["phases"]), {"before", "during", "after"})
        self.assertEqual(evidence["fault"], "partition")

    def test_production_matrix_schedules_every_frozen_fault_then_real_failover(self) -> None:
        hosts = tuple(
            runner.HostConfigV2(host, "runner-" + host[-1], "example.test", 22, "runner", "admin", "ssh-ed25519 AAAA", "x", b"a" * 32, b"b" * 32, "/e")
            for host in runner.REQUIRED_HOSTS
        )
        agents = tuple(FakeAgent(host.host_id, b"") for host in hosts)
        advertisements = {
            host: {"peer_node_id": (index + 1).to_bytes(1, "big").hex() * 32}
            for index, host in enumerate(runner.REQUIRED_HOSTS)
        }
        ring = {
            "host-a": {"outgoing": {"path_kind": "Direct"}},
            "host-b": {"outgoing": {"path_kind": "RelayUdp", "selected_relay": "91" * 32}},
            "host-c": {"outgoing": {"path_kind": "HolePunched"}},
        }
        seen: list[tuple[str, str]] = []

        def fault_cycle(*args, **kwargs):
            fault = args[7]
            seen.append((fault, kwargs["selected_host_id"]))
            return args[2], kwargs["agent_sequence"] + 4, kwargs["admin_sequence"] + 3, {"fault": fault}

        checkpoints = {host: {"sequence": 1, "intent": "11" * 32, "roots": "22" * 32} for host in runner.REQUIRED_HOSTS}
        failover = {"source": "host-b"}
        with mock.patch.object(runner, "_record_checkpoints", return_value=(10, checkpoints)), mock.patch.object(runner, "execute_fault_cycle", side_effect=fault_cycle), mock.patch.object(runner, "_exercise_ring_markers", side_effect=lambda *a, **kw: kw["agent_sequence"] + 2), mock.patch.object(runner, "_selected_relay_failover", return_value=(100, 50, ring, failover)):
            result = runner.execute_production_matrix(
                object(), hosts, agents,
                runner.ControllerCredentialsV2(Path("app"), Path("ssh"), {}),
                {host.host_id: b"r" * 32 for host in hosts},
                {"session_id": "44" * 32}, Ed25519PrivateKey.generate(),
                {}, ("01", "02"), advertisements, ring,
                agent_sequence=9, admin_sequence=2,
                deadline_monotonic_ns=time.monotonic_ns() + 1_000_000_000,
            )
        self.assertEqual([fault for fault, _ in seen], list(runner.REQUIRED_FAULTS))
        self.assertEqual([host for _, host in seen], [runner.REQUIRED_HOSTS[index % 3] for index in range(len(runner.REQUIRED_FAULTS))])
        self.assertEqual(result[4], failover)
        self.assertTrue(all(row["recovery_marker_verified"] for row in result[5]))

    def test_production_entrypoints_do_not_ship_placeholder_backends(self) -> None:
        repository = MODULE_PATH.parents[2]
        sources = {
            "controller": MODULE_PATH.read_text(encoding="utf-8"),
            "agent": (repository / "src/onebrain-node/examples/p5_multi_host_agent_v2.rs").read_text(encoding="utf-8"),
            "admin": (repository / "src/onebrain-node/examples/p5_admin_ctl_v2.rs").read_text(encoding="utf-8"),
            "relay-runtime": (repository / "src/onebrain-relay/src/runtime.rs").read_text(encoding="utf-8"),
            "relay-preflight": (repository / "src/onebrain-relay/src/bin/relay_preflight_probe.rs").read_text(encoding="utf-8"),
        }
        forbidden = (
            "verified run inputs require the Task 15 installed host inventory",
            "P5_V2_COMMAND_EXECUTOR_NOT_ATTACHED",
            "P5_V2_ADMIN_BACKEND_NOT_ATTACHED",
            "admin-response-stub",
            "accept_echo_once",
            "transport adapter unavailable",
        )
        for label, source in sources.items():
            for marker in forbidden:
                self.assertNotIn(marker, source, f"{label} still contains placeholder backend {marker}")

    def test_relay_only_real_ring_and_selected_relay_failover_qualifies(self) -> None:
        result = runner.derive_qualification(_aggregate())
        self.assertTrue(result["multi_host_qualified"])
        self.assertFalse(result["mixed_path_classes"])
        self.assertTrue(result["relay_only_path_classes"])

    def test_signed_aggregate_is_derived_from_raw_receipts_and_real_route_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            raw = Path(temporary)
            for index, host in enumerate(runner.REQUIRED_HOSTS, 1):
                (raw / f"child-{index:06d}-{host}.json").write_bytes(
                    runner.canonical_json({"format": 2, "host_id": host, "sequence": index}) + b"\n"
                )
            inventory = {
                "provider_evidence": [{"host_id": host} for host in runner.REQUIRED_HOSTS],
                "provider_evidence_status": "owner-telephone-verified-provider-document-pending",
                "public_probe_sets": _probe_receipts(),
                "qualification_tier": "production-reference",
                "topology_attestation": {"format": 2},
            }
            ring = {
                "host-a": {"outgoing": {"path_kind": "RelayTcp443", "route_receipt_blake3": "10" * 32}},
                "host-b": {"outgoing": {"path_kind": "RelayTcp443", "route_receipt_blake3": "11" * 32}},
                "host-c": {"outgoing": {"path_kind": "RelayTcp443", "route_receipt_blake3": "12" * 32}},
            }
            checkpoints = {
                host: {"sequence": 7, "intent": "55" * 32, "roots": "66" * 32}
                for host in runner.REQUIRED_HOSTS
            }
            aggregate = runner.build_signed_production_aggregate(
                request={"session_id": "bc" * 32},
                inventory=inventory,
                initial_ring=ring,
                checkpoints=checkpoints,
                failover_source="host-b",
                failover=_route("host-b", "host-c", "relay-udp", failed=True)["failover"],
                raw_root=raw,
                controller=Ed25519PrivateKey.generate(),
                cleanup_complete=True,
            )
            self.assertTrue(aggregate["qualification"]["multi_host_qualified"])
            self.assertEqual(len(aggregate["child_receipts"]), 3)
            self.assertEqual(aggregate["evidence_authority"]["provider_evidence_status"], "owner-telephone-verified-provider-document-pending")

    def test_all_direct_missing_edge_and_wrong_peer_reject(self) -> None:
        value = _aggregate()
        for route in value["routes"]:
            route["path_kind"] = "direct"
        self.assertFalse(runner.derive_qualification(value)["multi_host_qualified"])
        missing = _aggregate(); missing["routes"].pop()
        self.assertFalse(runner.derive_qualification(missing)["multi_host_qualified"])
        wrong = _aggregate(); wrong["routes"][0]["authenticated_peer"] = "host-c"
        self.assertFalse(runner.derive_qualification(wrong)["multi_host_qualified"])

    def test_simulation_socat_wireguard_and_observe_only_never_qualify(self) -> None:
        for transport in ("simulation", "socat", "wireguard", "observe-only"):
            value = _aggregate(); value["transport"] = transport
            self.assertFalse(runner.derive_qualification(value)["multi_host_qualified"])
        value = _aggregate(); value["preflight_only"] = True
        self.assertFalse(runner.derive_qualification(value)["multi_host_qualified"])

    def test_missing_fault_alternate_or_fresh_session_and_wrong_checkpoint_reject(self) -> None:
        mutations = []
        value = _aggregate(); value["routes"][0]["faults"].pop(); mutations.append(value)
        value = _aggregate(); value["routes"][1]["failover"].pop("alternate_relay"); mutations.append(value)
        value = _aggregate(); value["routes"][1]["failover"]["alternate_relay"] = "relay-a"; mutations.append(value)
        value = _aggregate(); value["routes"][1]["failover"]["alternate_reservation_issued_at"] = 21; mutations.append(value)
        value = _aggregate(); value["routes"][1]["failover"]["resumed_session"] = value["routes"][1]["failover"]["prior_session"]; mutations.append(value)
        value = _aggregate(); value["routes"][1]["failover"]["resumed_binding"] = value["routes"][1]["failover"]["prior_binding"]; mutations.append(value)
        value = _aggregate(); value["routes"][1]["failover"]["resumed_checkpoint"]["sequence"] = 9; mutations.append(value)
        for mutation in mutations:
            self.assertFalse(runner.derive_qualification(mutation)["multi_host_qualified"])

    def test_exact_ssh_argv_is_config_and_agent_independent(self) -> None:
        host = runner.HostConfigV2("host-a", "runner-a", "example.test", 10041, "p5-runner", "p5-admin", "ssh-ed25519 AAAA", "x", b"a"*32, b"b"*32, "/var/lib/onebrain")
        credentials = runner.ControllerCredentialsV2(Path("app.key"), Path("ssh.key"), {"host-a": Path("known-hosts")})
        argv = runner.build_ssh_argv(host, credentials, admin=False, ssh_binary="ssh")
        joined = " ".join(argv)
        for token in ("-F /dev/null", "GlobalKnownHostsFile=none", "UpdateHostKeys=no", "VerifyHostKeyDNS=no", "PreferredAuthentications=publickey", "PasswordAuthentication=no", "KbdInteractiveAuthentication=no", "GSSAPIAuthentication=no", "HostbasedAuthentication=no", "IdentityAgent=none", "CertificateFile=none", "IdentitiesOnly=yes", "TCPKeepAlive=yes", "ServerAliveInterval=2", "ServerAliveCountMax=15"):
            self.assertIn(token, joined)
        self.assertEqual(argv[-1], "p5-runner@example.test")
        self.assertNotIn("app.key", argv)

    def test_exited_bridge_reports_bounded_remote_stderr(self) -> None:
        process = SimpleNamespace(
            stdin=io.BytesIO(),
            stdout=io.BytesIO(),
            stderr=io.BytesIO(b"P5 V2 agent failed: File exists (os error 17)\n"),
            poll=lambda: 1,
        )
        agent = runner.OpenSshRunningAgent("host-a", process)
        command = runner.CanonicalCommandV2.create(12, {"command": "record-checkpoint"})

        with self.assertRaisesRegex(
            runner.P5ExecutionError,
            r"truncated frame.*File exists \(os error 17\)",
        ):
            agent.execute(command, time.monotonic_ns() + 5_000_000_000)

    def test_bridge_waits_briefly_for_exit_before_reading_remote_stderr(self) -> None:
        waits: list[float] = []

        def wait(*, timeout: float) -> int:
            waits.append(timeout)
            return 1

        process = SimpleNamespace(
            stdin=io.BytesIO(),
            stdout=io.BytesIO(),
            stderr=io.BytesIO(b"P5 V2 agent failed: OBP_REACHABILITY_ADMISSION: SequenceRollback\n"),
            poll=lambda: None,
            wait=wait,
        )
        agent = runner.OpenSshRunningAgent("host-a", process)
        command = runner.CanonicalCommandV2.create(44, {"command": "connect-ring-edge"})

        with self.assertRaisesRegex(
            runner.P5ExecutionError,
            r"truncated frame.*SequenceRollback",
        ):
            agent.execute(command, time.monotonic_ns() + 5_000_000_000)
        self.assertEqual(waits, [0.25])

    def test_partial_receipt_is_durable_before_other_failure_is_reported(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            receipt = json.dumps({"format": 2, "host_id": "host-a", "sequence": 1, "signature": "11"*64}, sort_keys=True, separators=(",", ":")).encode()
            executor = runner.OpenSshWaveExecutor(root, verify_child_receipt=runner.verify_minimal_child_receipt)
            agents = (FakeAgent("host-a", receipt), FakeAgent("host-b", RuntimeError("boom")))
            commands = tuple(runner.CanonicalCommandV2.create(i + 1, {"host": agent.host_id}) for i, agent in enumerate(agents))
            with self.assertRaisesRegex(
                runner.P5ExecutionError,
                "child failure: host-b sequence 2: boom",
            ):
                executor.execute_wave(agents, commands, time.monotonic_ns() + 5_000_000_000)
            persisted = list((root / "p5" / "raw").glob("*.json"))
            self.assertEqual(len(persisted), 1)
            self.assertTrue(all(agent.terminated for agent in agents))

    def test_agent_start_is_concurrent_and_partial_start_is_cleaned(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            executor = runner.OpenSshWaveExecutor(Path(temporary), verify_child_receipt=runner.verify_minimal_child_receipt)
            started: list[FakeAgent] = []
            barrier = threading.Barrier(3)
            hosts = tuple(runner.HostConfigV2(host, "runner-" + host[-1], "example.test", 22, "runner", "admin", "ssh-ed25519 AAAA", "x", b"a"*32, b"b"*32, "/e") for host in runner.REQUIRED_HOSTS)
            credentials = runner.ControllerCredentialsV2(Path("app"), Path("ssh"), {host.host_id: Path(host.host_id) for host in hosts})
            def start(host, credentials, deadline):
                barrier.wait(timeout=1)
                if host.host_id == "host-c": raise RuntimeError("start failed")
                agent = FakeAgent(host.host_id, b""); started.append(agent); return agent
            executor._start_bridge = start
            with self.assertRaisesRegex(runner.P5ExecutionError, "bridge start failure"):
                executor.start_agents(hosts, credentials, time.monotonic_ns() + 5_000_000_000)
            self.assertEqual({agent.host_id for agent in started}, {"host-a", "host-b"})
            self.assertTrue(all(agent.terminated for agent in started))

    def test_controller_exposes_exactly_eight_closed_modes(self) -> None:
        source = MODULE_PATH.read_text(encoding="utf-8")
        expected = {
            "generate-controller-key", "generate-run-approver-key",
            "generate-raw-evidence-recipient", "prepare-inventory",
            "prepare-request", "sign-request", "verify-request", "run",
        }
        for mode in expected: self.assertIn(f'add_parser("{mode}")', source)
        self.assertNotIn('add_parser("derive-qualification")', source)

    def test_agent_bridge_response_budget_covers_reservation_window(self) -> None:
        source = (
            MODULE_PATH.parents[2]
            / "src/onebrain-node/examples/p5_agent_ctl_v2.rs"
        ).read_text(encoding="utf-8")
        self.assertIn(
            "RESPONSE_READ_TIMEOUT: std::time::Duration = "
            "std::time::Duration::from_secs(30)",
            source,
        )
        self.assertIn("stream.set_read_timeout(Some(RESPONSE_READ_TIMEOUT))", source)
        self.assertIn("stream.set_write_timeout(Some(FRAME_WRITE_TIMEOUT))", source)

    def test_raw_archive_hpke_is_randomized_authenticated_and_round_trips(self) -> None:
        recipient = X25519PrivateKey.generate()
        private = recipient.private_bytes(serialization.Encoding.Raw, serialization.PrivateFormat.Raw, serialization.NoEncryption())
        public = recipient.public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw)
        aad = {"request_digest": "11" * 32, "aggregate_blake3": "22" * 32}
        first = runner.encrypt_raw_archive(b"private evidence", public, aad)
        second = runner.encrypt_raw_archive(b"private evidence", public, aad)
        self.assertNotEqual(first["encapsulated_key"], second["encapsulated_key"])
        self.assertNotEqual(first["ciphertext"], second["ciphertext"])
        self.assertEqual(runner.decrypt_raw_archive(first, private, aad), b"private evidence")
        with self.assertRaises(runner.P5ExecutionError):
            runner.decrypt_raw_archive(first, private, {**aad, "aggregate_blake3": "33" * 32})

    def test_distinct_run_approver_signs_only_the_p5_domain(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); private = root / "approver.key"; policy_path = root / "policy.json"
            runner.generate_run_approver(private, policy_path, 10, 100)
            policy = json.loads(policy_path.read_text(encoding="utf-8"))
            inventory = {"format": 2}; inventory_path = root / "inventory.json"; inventory_path.write_text(json.dumps(inventory), encoding="utf-8")
            request = {"format": 2, "inventory_blake3": runner.blake3.blake3(runner.canonical_json(inventory)).hexdigest(), "issued_at": 20, "expires_at": 30, "qualification_tier": "production-reference"}
            request_path = root / "request.json"; request_path.write_text(json.dumps(request), encoding="utf-8")
            signature_path = root / "request.sig"; signature_path.write_bytes(runner.sign_request(request_path, policy_path, private))
            self.assertEqual(runner.verify_p5_request(request_path, signature_path, policy_path, inventory_path), request)
            signature_path.write_bytes(b"\0" * 64)
            with self.assertRaises(runner.P5ExecutionError): runner.verify_p5_request(request_path, signature_path, policy_path, inventory_path)

    def test_handcrafted_or_mismatched_child_receipt_rejects(self) -> None:
        with self.assertRaises(runner.P5ExecutionError):
            runner.verify_minimal_child_receipt("host-a", b"{}")
        forged = json.dumps({"format": 2, "host_id": "host-b", "sequence": 1, "signature": "11"*64}).encode()
        with self.assertRaisesRegex(runner.P5ExecutionError, "host"):
            runner.verify_minimal_child_receipt("host-a", forged)


if __name__ == "__main__":
    unittest.main()
