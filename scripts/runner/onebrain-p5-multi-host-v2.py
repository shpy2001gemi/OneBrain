#!/usr/bin/env python3
"""Concurrent, fail-closed controller for the P5 V2 three-host reference run.

The module intentionally owns only controller concerns.  It cannot mutate a
host directly: runner and admin operations cross separately forced, pinned
OpenSSH channels and qualification is derived from verified receipt bytes.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import hmac
import json
import os
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Callable, Mapping, Protocol

import blake3
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305
from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey, Ed25519PublicKey
from cryptography.hazmat.primitives.asymmetric.x25519 import X25519PrivateKey, X25519PublicKey


REQUIRED_HOSTS = ("host-a", "host-b", "host-c")
REQUIRED_EDGES = (("host-a", "host-b"), ("host-b", "host-c"), ("host-c", "host-a"))
DIRECT_CLASS = frozenset(("direct", "hole-punched"))
RELAY_CLASS = frozenset(("relay-udp", "relay-tcp-443"))
REQUIRED_FAULTS = (
    "partition", "drop", "reorder", "duplicate", "restart", "address-change",
    "seed-outage", "signer-outage", "disk-pressure", "slow-peer",
    "base-obarv002-archive-restore", "rollback", "explicit-re-enable",
)
ALLOWED_REAL_TRANSPORT = "real-obp"
MAX_CONTROL_BYTES = 131_072
MAX_RECEIPT_BYTES = 262_144


class P5ExecutionError(RuntimeError):
    """A controller boundary, receipt, or qualification failed closed."""


def canonical_json(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode("utf-8")


@dataclass(frozen=True)
class HostConfigV2:
    host_id: str
    runner_id: str
    ssh_host: str
    ssh_port: int
    runner_ssh_user: str
    admin_ssh_user: str
    ssh_host_public_key: str
    host_key_sha256: str
    runner_authorized_key_line_blake3: bytes
    admin_authorized_key_line_blake3: bytes
    evidence_root: PurePosixPath | str

    def __post_init__(self) -> None:
        if self.host_id not in REQUIRED_HOSTS or not 1 <= self.ssh_port <= 65535:
            raise P5ExecutionError("invalid signed host configuration")
        if not self.ssh_host or not self.runner_ssh_user or not self.admin_ssh_user:
            raise P5ExecutionError("incomplete signed host configuration")


@dataclass(frozen=True)
class ControllerCredentialsV2:
    application_signing_key: Path
    ssh_identity_file: Path
    known_hosts_by_host: Mapping[str, Path]


@dataclass(frozen=True)
class CanonicalCommandV2:
    sequence: int
    canonical_bytes: bytes
    digest: bytes

    @classmethod
    def create(cls, sequence: int, payload: object) -> "CanonicalCommandV2":
        if sequence <= 0:
            raise P5ExecutionError("command sequence must be positive")
        encoded = canonical_json(payload)
        if len(encoded) > MAX_CONTROL_BYTES:
            raise P5ExecutionError("command exceeds its bounded size")
        return cls(sequence, encoded, blake3.blake3(encoded).digest())


@dataclass(frozen=True)
class SignedChildReceiptV2:
    host_id: str
    sequence: int
    canonical_bytes: bytes


class RunningAgent(Protocol):
    host_id: str
    def execute(self, command: CanonicalCommandV2, deadline_monotonic_ns: int) -> bytes: ...
    def terminate(self) -> None: ...
    def wait(self, timeout: float) -> int: ...
    def kill(self) -> None: ...
    def close(self) -> None: ...


SSH_OPTIONS = (
    "BatchMode=yes", "StrictHostKeyChecking=yes", "GlobalKnownHostsFile=none",
    "UpdateHostKeys=no", "VerifyHostKeyDNS=no", "HostKeyAlgorithms=ssh-ed25519",
    "IdentitiesOnly=yes", "PreferredAuthentications=publickey",
    "PasswordAuthentication=no", "KbdInteractiveAuthentication=no",
    "GSSAPIAuthentication=no", "HostbasedAuthentication=no", "IdentityAgent=none",
    "CertificateFile=none",
)


def build_ssh_argv(
    host: HostConfigV2,
    credentials: ControllerCredentialsV2,
    *,
    admin: bool,
    ssh_binary: str = "ssh",
) -> list[str]:
    known_hosts = credentials.known_hosts_by_host.get(host.host_id)
    if known_hosts is None:
        raise P5ExecutionError(f"missing pinned known-hosts file for {host.host_id}")
    user = host.admin_ssh_user if admin else host.runner_ssh_user
    argv = [ssh_binary, "-F", "/dev/null", "-T"]
    for option in SSH_OPTIONS:
        argv.extend(("-o", option))
    argv.extend(("-o", f"UserKnownHostsFile={known_hosts}", "-i", str(credentials.ssh_identity_file)))
    argv.extend(("-p", str(host.ssh_port), f"{user}@{host.ssh_host}"))
    return argv


class OpenSshRunningAgent:
    def __init__(self, host_id: str, process: subprocess.Popen[bytes]) -> None:
        self.host_id = host_id
        self._process = process

    def execute(self, command: CanonicalCommandV2, deadline_monotonic_ns: int) -> bytes:
        if time.monotonic_ns() >= deadline_monotonic_ns:
            raise TimeoutError("command deadline elapsed")
        if self._process.stdin is None or self._process.stdout is None:
            raise P5ExecutionError("SSH bridge has no framed stdio")
        frame = len(command.canonical_bytes).to_bytes(4, "big") + command.canonical_bytes
        self._process.stdin.write(frame); self._process.stdin.flush()
        header = self._process.stdout.read(4)
        if len(header) != 4:
            raise P5ExecutionError("SSH bridge returned a truncated frame")
        size = int.from_bytes(header, "big")
        if not 0 < size <= MAX_RECEIPT_BYTES:
            raise P5ExecutionError("SSH bridge receipt size is invalid")
        body = self._process.stdout.read(size)
        if len(body) != size:
            raise P5ExecutionError("SSH bridge receipt is truncated")
        return body

    def terminate(self) -> None: self._process.terminate()
    def wait(self, timeout: float) -> int: return self._process.wait(timeout=timeout)
    def kill(self) -> None: self._process.kill()
    def close(self) -> None:
        if self._process.stdin is not None: self._process.stdin.close()
        if self._process.stdout is not None: self._process.stdout.close()
        if self._process.stderr is not None: self._process.stderr.close()


def verify_minimal_child_receipt(host_id: str, encoded: bytes) -> SignedChildReceiptV2:
    if not 0 < len(encoded) <= MAX_RECEIPT_BYTES:
        raise P5ExecutionError("child receipt size is invalid")
    try:
        value = json.loads(encoded)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise P5ExecutionError("child receipt is not canonical JSON") from error
    required = {"format", "host_id", "sequence", "signature"}
    if not isinstance(value, dict) or set(value) != required or value["format"] != 2:
        raise P5ExecutionError("handcrafted or non-V2 child receipt")
    if value["host_id"] != host_id:
        raise P5ExecutionError("child receipt host substitution")
    if isinstance(value["sequence"], bool) or not isinstance(value["sequence"], int) or value["sequence"] <= 0:
        raise P5ExecutionError("child receipt sequence is invalid")
    signature = value["signature"]
    if not isinstance(signature, str) or len(signature) != 128:
        raise P5ExecutionError("child receipt signature is missing")
    normalized = canonical_json(value)
    if encoded != normalized:
        raise P5ExecutionError("child receipt encoding is noncanonical")
    return SignedChildReceiptV2(host_id, value["sequence"], normalized)


class OpenSshWaveExecutor:
    def __init__(
        self,
        evidence_root: Path,
        *,
        verify_child_receipt: Callable[[str, bytes], SignedChildReceiptV2],
        popen_factory: Callable[..., subprocess.Popen[bytes]] = subprocess.Popen,
        max_workers: int = 3,
    ) -> None:
        self.evidence_root = evidence_root
        self._verify = verify_child_receipt
        self._popen = popen_factory
        self._pool = concurrent.futures.ThreadPoolExecutor(max_workers=max_workers)

    def _start_bridge(self, host: HostConfigV2, credentials: ControllerCredentialsV2, deadline: int) -> RunningAgent:
        if time.monotonic_ns() >= deadline:
            raise TimeoutError("bridge deadline elapsed")
        process = self._popen(build_ssh_argv(host, credentials, admin=False), stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        return OpenSshRunningAgent(host.host_id, process)

    def start_agents(self, hosts: tuple[HostConfigV2, ...], credentials: ControllerCredentialsV2, deadline_monotonic_ns: int) -> tuple[RunningAgent, ...]:
        indexed = {self._pool.submit(self._start_bridge, host, credentials, deadline_monotonic_ns): index for index, host in enumerate(hosts)}
        remaining = max(0.0, (deadline_monotonic_ns - time.monotonic_ns()) / 1e9)
        done, pending = concurrent.futures.wait(tuple(indexed), timeout=remaining)
        started: dict[int, RunningAgent] = {}; errors: list[BaseException] = []
        for future in done:
            try: started[indexed[future]] = future.result()
            except BaseException as error: errors.append(error)
        for future in pending:
            future.cancel(); future.add_done_callback(self._terminate_late_started_bridge)
        if pending or errors or len(started) != len(hosts):
            self._terminate_wait_kill(tuple(started.values()))
            raise P5ExecutionError("P5 start failed closed: deadline" if pending else "P5 start failed closed: bridge start failure") from (errors[0] if errors else None)
        return tuple(started[index] for index in range(len(hosts)))

    def _terminate_late_started_bridge(self, future: concurrent.futures.Future[RunningAgent]) -> None:
        if future.cancelled(): return
        try: agent = future.result()
        except BaseException: return
        self._terminate_wait_kill((agent,))

    def execute_wave(self, agents: tuple[RunningAgent, ...], commands: tuple[CanonicalCommandV2, ...], deadline_monotonic_ns: int) -> tuple[SignedChildReceiptV2, ...]:
        if len(agents) != len(commands):
            raise P5ExecutionError("wave cardinality mismatch")
        indexed = {self._pool.submit(agent.execute, command, deadline_monotonic_ns): index for index, (agent, command) in enumerate(zip(agents, commands, strict=True))}
        remaining = max(0.0, (deadline_monotonic_ns - time.monotonic_ns()) / 1e9)
        done, pending = concurrent.futures.wait(tuple(indexed), timeout=remaining)
        verified: dict[int, SignedChildReceiptV2] = {}; errors: list[BaseException] = []
        for future in done:
            index = indexed[future]
            try:
                receipt = self._verify(agents[index].host_id, future.result())
                if receipt.sequence != commands[index].sequence:
                    raise P5ExecutionError("receipt command sequence mismatch")
                self._persist_verified_partial_receipt(receipt)
                verified[index] = receipt
            except BaseException as error:
                errors.append(error)
        if pending or errors:
            self._terminate_wait_kill(agents)
            raise P5ExecutionError("P5 wave failed closed: deadline" if pending else "P5 wave failed closed: child failure") from (errors[0] if errors else None)
        receipts = tuple(verified[index] for index in range(len(agents)))
        if len({receipt.host_id for receipt in receipts}) != len(receipts):
            raise P5ExecutionError("duplicate host receipt")
        return receipts

    def _persist_verified_partial_receipt(self, receipt: SignedChildReceiptV2) -> None:
        root = self.evidence_root / "p5" / "raw"
        root.mkdir(parents=True, exist_ok=True)
        if os.name != "nt": root.chmod(0o700)
        path = root / f"child-{receipt.sequence:06d}-{receipt.host_id}.json"
        try:
            with path.open("xb") as stream:
                stream.write(receipt.canonical_bytes); stream.write(b"\n"); stream.flush(); os.fsync(stream.fileno())
            if os.name != "nt": path.chmod(0o400)
            if os.name != "nt":
                descriptor = os.open(root, os.O_RDONLY)
                try: os.fsync(descriptor)
                finally: os.close(descriptor)
        except FileExistsError as error:
            raise P5ExecutionError("partial receipt overwrite rejected") from error

    @staticmethod
    def _terminate_wait_kill(agents: tuple[RunningAgent, ...], terminate_seconds: float = 5, kill_seconds: float = 5) -> None:
        for agent in agents:
            try: agent.terminate()
            except BaseException: pass
        for agent in agents:
            try: agent.wait(terminate_seconds)
            except BaseException:
                try: agent.kill(); agent.wait(kill_seconds)
                except BaseException: pass

    @staticmethod
    def close_agents(agents: tuple[RunningAgent, ...]) -> None:
        for agent in agents: agent.close()


def _failover_valid(route: dict[str, object]) -> bool:
    failure = route.get("failover")
    if not isinstance(failure, dict): return False
    selected, alternate = failure.get("selected_relay"), failure.get("alternate_relay")
    before = failure.get("selected_reservation_issued_at")
    alt_before = failure.get("alternate_reservation_issued_at")
    failed = failure.get("failure_observed_at")
    checkpoint = route.get("checkpoint")
    resumed = failure.get("resumed_checkpoint")
    return bool(
        selected and alternate and selected != alternate
        and isinstance(before, int) and isinstance(alt_before, int) and isinstance(failed, int)
        and before < failed and alt_before < failed
        and failure.get("prior_binding") != failure.get("resumed_binding")
        and failure.get("prior_session") != failure.get("resumed_session")
        and isinstance(checkpoint, dict) and isinstance(resumed, dict)
        and resumed.get("sequence") == checkpoint.get("sequence", -2) + 1
        and resumed.get("intent") == checkpoint.get("intent")
        and resumed.get("roots") == checkpoint.get("roots")
    )


def derive_qualification(aggregate: dict[str, object]) -> dict[str, bool]:
    routes = aggregate.get("routes")
    valid_routes = isinstance(routes, list) and len(routes) == 3
    edge_map: dict[tuple[object, object], dict[str, object]] = {}
    if valid_routes:
        edge_map = {(row.get("from"), row.get("to")): row for row in routes if isinstance(row, dict)}
    all_expected = set(edge_map) == set(REQUIRED_EDGES) and all(row.get("authenticated_peer") == target for (_, target), row in edge_map.items())
    paths = {str(row.get("path_kind")) for row in edge_map.values()}
    mixed = bool(paths & DIRECT_CLASS and paths & RELAY_CLASS and paths <= DIRECT_CLASS | RELAY_CLASS)
    all_faults = bool(edge_map) and all(set(row.get("faults", ())) == set(REQUIRED_FAULTS) for row in edge_map.values())
    failovers = [row for row in edge_map.values() if isinstance(row.get("failover"), dict)]
    relay_failed = len(failovers) == 1 and _failover_valid(failovers[0])
    real = aggregate.get("transport") == ALLOWED_REAL_TRANSPORT and aggregate.get("preflight_only") is False
    resource = aggregate.get("resource_bounds") is True
    cleanup = aggregate.get("cleanup_complete") is True
    qualified = all_expected and mixed and all_faults and relay_failed and real and resource and cleanup
    return {
        "all_expected_peers": all_expected,
        "mixed_path_classes": mixed,
        "all_real_faults": all_faults,
        "selected_relay_failed": relay_failed,
        "alternate_pre_reserved": relay_failed,
        "fresh_reauthentication": relay_failed,
        "exact_checkpoint_resume": relay_failed,
        "resource_bounds": resource,
        "cleanup_complete": cleanup,
        "multi_host_qualified": qualified,
    }


def _write_create_new(path: Path, data: bytes, *, private: bool = False) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("xb") as stream:
        stream.write(data); stream.flush(); os.fsync(stream.fileno())
    if os.name != "nt": path.chmod(0o400 if private else 0o444)


def _generate_ed25519(private_path: Path, public_path: Path) -> None:
    key = Ed25519PrivateKey.generate()
    _write_create_new(private_path, key.private_bytes_raw(), private=True)
    _write_create_new(public_path, key.public_key().public_bytes_raw())


def _generate_x25519(private_path: Path, public_path: Path) -> None:
    key = X25519PrivateKey.generate()
    _write_create_new(private_path, key.private_bytes(serialization.Encoding.Raw, serialization.PrivateFormat.Raw, serialization.NoEncryption()), private=True)
    public = {"format": "onebrain/p5/raw-evidence-recipient/2", "kem": "x25519-hkdf-sha256", "public_key": key.public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw).hex()}
    _write_create_new(public_path, canonical_json(public) + b"\n")


def _hkdf_extract(salt: bytes, ikm: bytes) -> bytes:
    return hmac.new(salt or b"\0" * 32, ikm, hashlib.sha256).digest()


def _hkdf_expand(prk: bytes, info: bytes, length: int) -> bytes:
    output = b""; block = b""
    for counter in range(1, (length + 31) // 32 + 1):
        block = hmac.new(prk, block + info + bytes((counter,)), hashlib.sha256).digest()
        output += block
    return output[:length]


def _labeled_extract(suite: bytes, salt: bytes, label: bytes, ikm: bytes) -> bytes:
    return _hkdf_extract(salt, b"HPKE-v1" + suite + label + ikm)


def _labeled_expand(suite: bytes, prk: bytes, label: bytes, info: bytes, length: int) -> bytes:
    return _hkdf_expand(prk, length.to_bytes(2, "big") + b"HPKE-v1" + suite + label + info, length)


def _hpke_context(shared_secret: bytes, info: bytes) -> tuple[bytes, bytes]:
    suite = b"HPKE" + (0x20).to_bytes(2, "big") + (1).to_bytes(2, "big") + (3).to_bytes(2, "big")
    psk_id_hash = _labeled_extract(suite, b"", b"psk_id_hash", b"")
    info_hash = _labeled_extract(suite, b"", b"info_hash", info)
    context = b"\0" + psk_id_hash + info_hash
    secret = _labeled_extract(suite, shared_secret, b"secret", b"")
    return _labeled_expand(suite, secret, b"key", context, 32), _labeled_expand(suite, secret, b"base_nonce", context, 12)


def encrypt_raw_archive(plaintext: bytes, recipient_public_key: bytes, aad_object: object) -> dict[str, object]:
    """RFC 9180 base-mode X25519/HKDF-SHA256/ChaCha20Poly1305 envelope."""
    if len(recipient_public_key) != 32 or len(plaintext) > 67_108_864:
        raise P5ExecutionError("raw archive recipient or plaintext bound is invalid")
    ephemeral = X25519PrivateKey.generate(); enc = ephemeral.public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw)
    try: recipient = X25519PublicKey.from_public_bytes(recipient_public_key)
    except ValueError as error: raise P5ExecutionError("raw archive recipient key is invalid") from error
    dh = ephemeral.exchange(recipient); kem_suite = b"KEM" + (0x20).to_bytes(2, "big"); kem_context = enc + recipient_public_key
    eae_prk = _labeled_extract(kem_suite, b"", b"eae_prk", dh)
    shared = _labeled_expand(kem_suite, eae_prk, b"shared_secret", kem_context, 32)
    info = b"onebrain/p5/raw-evidence-archive/v2"; key, nonce = _hpke_context(shared, info)
    aad = canonical_json(aad_object); ciphertext = ChaCha20Poly1305(key).encrypt(nonce, plaintext, aad)
    return {"format": 2, "suite": "hpke-x25519-hkdf-sha256-chacha20poly1305", "encapsulated_key": enc.hex(), "aad_blake3": blake3.blake3(aad).hexdigest(), "plaintext_blake3": blake3.blake3(plaintext).hexdigest(), "ciphertext": ciphertext.hex()}


def decrypt_raw_archive(envelope: dict[str, object], recipient_private_key: bytes, aad_object: object) -> bytes:
    if envelope.get("format") != 2 or envelope.get("suite") != "hpke-x25519-hkdf-sha256-chacha20poly1305" or len(recipient_private_key) != 32:
        raise P5ExecutionError("raw archive envelope identity is invalid")
    try:
        private = X25519PrivateKey.from_private_bytes(recipient_private_key); enc = bytes.fromhex(str(envelope["encapsulated_key"])); ciphertext = bytes.fromhex(str(envelope["ciphertext"]))
        peer = X25519PublicKey.from_public_bytes(enc)
    except (KeyError, ValueError) as error: raise P5ExecutionError("raw archive envelope encoding is invalid") from error
    public = private.public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw)
    dh = private.exchange(peer); kem_suite = b"KEM" + (0x20).to_bytes(2, "big"); eae_prk = _labeled_extract(kem_suite, b"", b"eae_prk", dh)
    shared = _labeled_expand(kem_suite, eae_prk, b"shared_secret", enc + public, 32)
    aad = canonical_json(aad_object)
    if envelope.get("aad_blake3") != blake3.blake3(aad).hexdigest(): raise P5ExecutionError("raw archive AAD mismatch")
    key, nonce = _hpke_context(shared, b"onebrain/p5/raw-evidence-archive/v2")
    try: plaintext = ChaCha20Poly1305(key).decrypt(nonce, ciphertext, aad)
    except Exception as error: raise P5ExecutionError("raw archive authentication failed") from error
    if envelope.get("plaintext_blake3") != blake3.blake3(plaintext).hexdigest(): raise P5ExecutionError("raw archive plaintext digest mismatch")
    return plaintext


def _read_json(path: Path, label: str, maximum: int = 262_144) -> dict[str, object]:
    encoded = path.read_bytes()
    if not encoded or len(encoded) > maximum:
        raise P5ExecutionError(f"{label} is empty or exceeds its bound")
    try: value = json.loads(encoded)
    except (UnicodeDecodeError, json.JSONDecodeError) as error: raise P5ExecutionError(f"{label} is invalid JSON") from error
    if not isinstance(value, dict): raise P5ExecutionError(f"{label} must be an object")
    return value


def _raw_private_key(path: Path) -> Ed25519PrivateKey:
    encoded = path.read_bytes()
    if len(encoded) != 32: raise P5ExecutionError("Ed25519 private key must be 32 raw bytes")
    return Ed25519PrivateKey.from_private_bytes(encoded)


def generate_run_approver(private_path: Path, policy_path: Path, valid_from: int, valid_until: int) -> dict[str, str]:
    if valid_from < 0 or valid_until <= valid_from: raise P5ExecutionError("approval validity interval is invalid")
    key = Ed25519PrivateKey.generate(); public = key.public_key().public_bytes_raw()
    policy = {"format": 2, "role": "p5-run-approver", "signing_domain": "onebrain/p5/run-request/v2", "public_key": public.hex(), "public_key_blake3": blake3.blake3(public).hexdigest(), "valid_from": valid_from, "valid_until": valid_until}
    _write_create_new(private_path, key.private_bytes_raw(), private=True)
    encoded = canonical_json(policy) + b"\n"; _write_create_new(policy_path, encoded)
    return {"public_key_fingerprint": blake3.blake3(public).hexdigest(), "policy_blake3": blake3.blake3(canonical_json(policy)).hexdigest()}


def prepare_inventory(args: argparse.Namespace) -> dict[str, object]:
    expected_hosts = {f"{host}.json" for host in REQUIRED_HOSTS}
    host_files = {path.name for path in args.host_public_root.glob("*.json") if path.is_file() and not path.is_symlink()}
    if host_files != expected_hosts: raise P5ExecutionError("inventory requires exactly host-a/host-b/host-c public exports")
    provider_files = {path.name for path in args.provider_evidence_root.glob("*.json") if path.is_file() and not path.is_symlink()}
    if provider_files != expected_hosts: raise P5ExecutionError("provider evidence requires exactly one typed entry per host")
    probe_files = sorted(path for path in args.relay_evidence_root.glob("*.json") if path.is_file() and not path.is_symlink())
    if len(probe_files) < 2: raise P5ExecutionError("at least two descriptor-key-bound public probe sets are required")
    hosts = [_read_json(args.host_public_root / f"{host}.json", f"{host} public export") for host in REQUIRED_HOSTS]
    providers = [_read_json(args.provider_evidence_root / f"{host}.json", f"{host} provider evidence", 131_072) for host in REQUIRED_HOSTS]
    probes = [_read_json(path, path.name, 131_072) for path in probe_files]
    topology = _read_json(args.topology_attestation, "topology attestation", 131_072)
    pending = any(row.get("status") != "provider-document-verified" for row in providers)
    provider_status = "owner-telephone-verified-provider-document-pending" if pending else "provider-document-verified"
    public = args.controller_public.read_bytes(); ssh_public = args.ssh_public.read_bytes()
    if len(public) != 32 or not ssh_public.strip(): raise P5ExecutionError("controller public identities are invalid")
    bundle_manifest = args.bundle_root / "metadata" / "bundle.manifest.json"
    if not bundle_manifest.is_file(): raise P5ExecutionError("bundle manifest is missing")
    registry_manifest = args.registry_candidate_root / "concepts.obr.manifest.json"
    if not registry_manifest.is_file(): raise P5ExecutionError("Registry candidate manifest is missing")
    inventory = {"format": 2, "qualification_tier": "production-reference", "hosts": hosts, "public_probe_sets": probes, "topology_attestation": topology, "provider_evidence": providers, "provider_evidence_status": provider_status, "controller_application_public_key": public.hex(), "controller_ssh_key_sha256": hashlib.sha256(ssh_public).hexdigest(), "bundle_manifest_blake3": blake3.blake3(bundle_manifest.read_bytes()).hexdigest(), "registry_candidate_manifest_blake3": blake3.blake3(registry_manifest.read_bytes()).hexdigest()}
    inventory["inventory_blake3"] = blake3.blake3(canonical_json(inventory)).hexdigest()
    return inventory


def prepare_request(args: argparse.Namespace) -> dict[str, object]:
    nonce = bytes.fromhex(args.run_nonce)
    if len(nonce) != 32 or nonce.hex() != args.run_nonce: raise P5ExecutionError("run nonce must be 32-byte lowercase hex")
    if args.issued_at < 0 or args.expires_at <= args.issued_at: raise P5ExecutionError("request validity interval is invalid")
    inventory = _read_json(args.inventory, "P5 inventory")
    policy = _read_json(args.approval_policy, "P5 approval policy", 65_536)
    if policy.get("format") != 2 or policy.get("role") != "p5-run-approver" or policy.get("signing_domain") != "onebrain/p5/run-request/v2": raise P5ExecutionError("P5 approval policy is not authorized")
    release_bytes = args.release_request.read_bytes(); inventory_bytes = canonical_json(inventory)
    seed = blake3.blake3(b"onebrain/p5/run-session-id/v2\0" + blake3.blake3(release_bytes).digest() + blake3.blake3(inventory_bytes).digest() + nonce).hexdigest()
    recipient = _read_json(args.raw_evidence_recipient_public, "raw evidence recipient")
    return {"format": 2, "release_request_blake3": blake3.blake3(release_bytes).hexdigest(), "inventory_blake3": blake3.blake3(inventory_bytes).hexdigest(), "p5_approval_policy_blake3": blake3.blake3(canonical_json(policy)).hexdigest(), "raw_evidence_recipient": recipient, "profile_blake3": blake3.blake3(args.profile.read_bytes()).hexdigest(), "vector_blake3": blake3.blake3(args.vector.read_bytes()).hexdigest(), "run_nonce": args.run_nonce, "session_id": seed, "issued_at": args.issued_at, "expires_at": args.expires_at, "qualification_tier": "production-reference"}


def sign_request(request_path: Path, policy_path: Path, key_path: Path) -> bytes:
    request = _read_json(request_path, "P5 request"); policy = _read_json(policy_path, "P5 approval policy", 65_536); key = _raw_private_key(key_path)
    public = key.public_key().public_bytes_raw()
    if policy.get("public_key") != public.hex() or policy.get("signing_domain") != "onebrain/p5/run-request/v2": raise P5ExecutionError("P5 approver key/policy mismatch")
    if not policy["valid_from"] <= request.get("issued_at", -1) <= request.get("expires_at", -1) <= policy["valid_until"]: raise P5ExecutionError("P5 request lies outside approved validity")
    return key.sign(b"onebrain/p5/run-request/v2\0" + blake3.blake3(canonical_json(request)).digest())


def verify_p5_request(request_path: Path, signature_path: Path, policy_path: Path, inventory_path: Path) -> dict[str, object]:
    request = _read_json(request_path, "P5 request"); policy = _read_json(policy_path, "P5 approval policy", 65_536); inventory = _read_json(inventory_path, "P5 inventory")
    signature = signature_path.read_bytes()
    if len(signature) != 64: raise P5ExecutionError("P5 detached signature must be 64 raw bytes")
    try: Ed25519PublicKey.from_public_bytes(bytes.fromhex(str(policy["public_key"]))).verify(signature, b"onebrain/p5/run-request/v2\0" + blake3.blake3(canonical_json(request)).digest())
    except (KeyError, ValueError, InvalidSignature) as error: raise P5ExecutionError("P5 request signature is invalid") from error
    if request.get("inventory_blake3") != blake3.blake3(canonical_json(inventory)).hexdigest(): raise P5ExecutionError("P5 request inventory binding mismatch")
    if request.get("qualification_tier") != "production-reference": raise P5ExecutionError("P5 request tier is invalid")
    return request


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="mode", required=True)
    controller = commands.add_parser("generate-controller-key")
    controller.add_argument("--output-private", type=Path, required=True); controller.add_argument("--output-public", type=Path, required=True)
    recipient = commands.add_parser("generate-raw-evidence-recipient")
    recipient.add_argument("--output-private", type=Path, required=True); recipient.add_argument("--output-public", type=Path, required=True)
    approver = commands.add_parser("generate-run-approver-key")
    approver.add_argument("--output-private", type=Path, required=True); approver.add_argument("--output-policy", type=Path, required=True)
    approver.add_argument("--valid-from", type=int, required=True); approver.add_argument("--valid-until", type=int, required=True)
    inventory = commands.add_parser("prepare-inventory")
    for name in ("host-public-root", "relay-evidence-root", "provider-evidence-root", "bundle-root", "registry-candidate-root"):
        inventory.add_argument(f"--{name}", type=Path, required=True)
    for name in ("topology-attestation", "controller-public", "ssh-public", "output"):
        inventory.add_argument(f"--{name}", type=Path, required=True)
    request = commands.add_parser("prepare-request")
    for name in ("release-request", "inventory", "approval-policy", "raw-evidence-recipient-public", "profile", "vector", "output"):
        request.add_argument(f"--{name}", type=Path, required=True)
    request.add_argument("--run-nonce", required=True); request.add_argument("--issued-at", type=int, required=True); request.add_argument("--expires-at", type=int, required=True)
    sign = commands.add_parser("sign-request")
    sign.add_argument("--p5-request", type=Path, required=True); sign.add_argument("--approval-policy", type=Path, required=True); sign.add_argument("--signing-key", type=Path, required=True); sign.add_argument("--output", type=Path, required=True)
    verify = commands.add_parser("verify-request")
    for name in ("release-request", "release-signature", "base-policy", "base-gpg-home", "p5-request", "p5-signature", "p5-approval-policy", "inventory", "bundle-root", "registry-candidate-root"):
        verify.add_argument(f"--{name}", type=Path, required=True)
    run = commands.add_parser("run")
    for name in ("release-request", "release-signature", "base-policy", "base-gpg-home", "p5-request", "p5-signature", "p5-approval-policy", "inventory", "controller-signing-key", "ssh-identity-key", "raw-evidence-recipient-private", "bundle-root", "registry-candidate-root", "evidence-root"):
        run.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        if args.mode == "generate-controller-key": _generate_ed25519(args.output_private, args.output_public)
        elif args.mode == "generate-raw-evidence-recipient": _generate_x25519(args.output_private, args.output_public)
        elif args.mode == "generate-run-approver-key":
            print(json.dumps(generate_run_approver(args.output_private, args.output_policy, args.valid_from, args.valid_until), sort_keys=True))
        elif args.mode == "prepare-inventory": _write_create_new(args.output, canonical_json(prepare_inventory(args)) + b"\n")
        elif args.mode == "prepare-request": _write_create_new(args.output, canonical_json(prepare_request(args)) + b"\n")
        elif args.mode == "sign-request": _write_create_new(args.output, sign_request(args.p5_request, args.approval_policy, args.signing_key))
        elif args.mode in {"verify-request", "run"}:
            verify_p5_request(args.p5_request, args.p5_signature, args.p5_approval_policy, args.inventory)
            # Base OpenPGP verification remains a separate unchanged authority path.
            from scripts.release.verify_base_release_request import verify_release_request
            verify_release_request(args.release_request, args.release_signature, args.base_policy, args.base_gpg_home)
            if args.mode == "run":
                raise P5ExecutionError("verified run inputs require the Task 15 installed host inventory")
    except (OSError, ValueError, P5ExecutionError) as error:
        print(f"P5 V2 controller failed: {error}", file=__import__("sys").stderr); return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
