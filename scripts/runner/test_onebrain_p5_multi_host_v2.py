from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path

from cryptography.hazmat.primitives import serialization
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
        _route("host-a", "host-b", "direct"),
        _route("host-b", "host-c", "relay-udp", failed=True),
        _route("host-c", "host-a", "hole-punched"),
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
    def test_mixed_real_ring_and_selected_relay_failover_qualifies(self) -> None:
        result = runner.derive_qualification(_aggregate())
        self.assertTrue(result["multi_host_qualified"])
        self.assertTrue(result["mixed_path_classes"])

    def test_all_direct_all_relay_missing_edge_and_wrong_peer_reject(self) -> None:
        for paths in (("direct",) * 3, ("relay-udp",) * 3):
            value = _aggregate()
            for route, path in zip(value["routes"], paths, strict=True):
                route["path_kind"] = path
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
        for token in ("-F /dev/null", "GlobalKnownHostsFile=none", "UpdateHostKeys=no", "VerifyHostKeyDNS=no", "PreferredAuthentications=publickey", "PasswordAuthentication=no", "KbdInteractiveAuthentication=no", "GSSAPIAuthentication=no", "HostbasedAuthentication=no", "IdentityAgent=none", "CertificateFile=none", "IdentitiesOnly=yes"):
            self.assertIn(token, joined)
        self.assertEqual(argv[-1], "p5-runner@example.test")
        self.assertNotIn("app.key", argv)

    def test_partial_receipt_is_durable_before_other_failure_is_reported(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            receipt = json.dumps({"format": 2, "host_id": "host-a", "sequence": 1, "signature": "11"*64}, sort_keys=True, separators=(",", ":")).encode()
            executor = runner.OpenSshWaveExecutor(root, verify_child_receipt=runner.verify_minimal_child_receipt)
            agents = (FakeAgent("host-a", receipt), FakeAgent("host-b", RuntimeError("boom")))
            commands = tuple(runner.CanonicalCommandV2.create(i + 1, {"host": agent.host_id}) for i, agent in enumerate(agents))
            with self.assertRaisesRegex(runner.P5ExecutionError, "child failure"):
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
