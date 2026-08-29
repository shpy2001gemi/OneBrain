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
import io
import json
import os
import subprocess
import sys
import tarfile
import time
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Callable, Mapping, Protocol

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

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
MAX_RELAY_DESCRIPTOR_VALIDITY_SECONDS = 1_800
BOOTSTRAP_DOMAIN = b"onebrain/p5/bootstrap-admin-frame/v2\0"
BOOTSTRAP_MAX_CLOCK_SKEW_SECONDS = 30
BOOTSTRAP_REMOTE_FUTURE_LIMIT_SECONDS = 300
BOOTSTRAP_TTL_SECONDS = (
    BOOTSTRAP_REMOTE_FUTURE_LIMIT_SECONDS - BOOTSTRAP_MAX_CLOCK_SKEW_SECONDS
)


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
    "CertificateFile=none", "TCPKeepAlive=yes", "ServerAliveInterval=2",
    "ServerAliveCountMax=15",
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


def _signed_agent_command(
    host: HostConfigV2,
    request: Mapping[str, object],
    key: Ed25519PrivateKey,
    sequence: int,
    command: str,
    parameters: Mapping[str, object] | None = None,
) -> CanonicalCommandV2:
    issued_at = int(time.time())
    expires_at = min(int(request["expires_at"]), issued_at + 120)
    unsigned = {
        "command": command,
        "expires_at": expires_at,
        "format": 2,
        "host_id": host.host_id,
        "issued_at": issued_at,
        "parameters": dict(parameters or {}),
        "sequence": sequence,
        "session_id": str(request["session_id"]),
    }
    signature = key.sign(b"onebrain/p5/signed-control-frame/v2\0" + canonical_json(unsigned))
    return CanonicalCommandV2.create(sequence, {**unsigned, "signature": signature.hex()})


def _signed_admin_command(
    host: HostConfigV2,
    request: Mapping[str, object],
    key: Ed25519PrivateKey,
    sequence: int,
    action: str,
    *,
    fault: str | None = None,
    phase: str | None = None,
    parameters: Mapping[str, object] | None = None,
) -> CanonicalCommandV2:
    issued_at = int(time.time())
    expires_at = min(int(request["expires_at"]), issued_at + 120)
    unsigned = {
        "action": action,
        "expires_at": expires_at,
        "fault": fault,
        "format": 2,
        "host_id": host.host_id,
        "issued_at": issued_at,
        "parameters": dict(parameters or {}),
        "phase": phase,
        "sequence": sequence,
        "session_id": str(request["session_id"]),
    }
    signature = key.sign(b"onebrain/p5/signed-admin-frame/v2\0" + canonical_json(unsigned))
    return CanonicalCommandV2.create(sequence, {**unsigned, "signature": signature.hex()})


def _evidence_authority(inventory: Mapping[str, object]) -> dict[str, object]:
    """Return the exact public evidence authority repeated by every P5 receipt.

    These values are deliberately materialized rather than left as a transitive
    request binding.  The owner-approved production-reference exception
    requires the provider-evidence status to remain explicit in every receipt
    and in the aggregate.
    """
    required = {
        "public_probe_sets": "public_probe_set_blake3",
        "topology_attestation": "topology_attestation_blake3",
        "provider_evidence": "provider_evidence_blake3",
    }
    authority: dict[str, object] = {
        "inventory_blake3": blake3.blake3(canonical_json(dict(inventory))).hexdigest(),
        "provider_evidence_status": inventory.get("provider_evidence_status"),
        "qualification_tier": inventory.get("qualification_tier"),
    }
    for source, target in required.items():
        if source not in inventory:
            raise P5ExecutionError(f"inventory lacks {source}")
        authority[target] = blake3.blake3(canonical_json(inventory[source])).hexdigest()
    if (
        not isinstance(authority["provider_evidence_status"], str)
        or not authority["provider_evidence_status"]
        or authority["qualification_tier"] != "production-reference"
    ):
        raise P5ExecutionError("inventory evidence authority is not production-reference")
    return authority


def _bootstrap_session_config(
    host: HostConfigV2,
    inventory: Mapping[str, object],
    request: Mapping[str, object],
    controller: Ed25519PrivateKey,
    *,
    release_request: bytes,
    release_signature: bytes,
    base_policy: bytes,
    p5_request: bytes,
    p5_signature: bytes,
    p5_approval_policy: bytes,
    bundle_manifest: bytes,
) -> dict[str, object]:
    rows = inventory.get("hosts")
    if not isinstance(rows, list):
        raise P5ExecutionError("inventory host rows are unavailable")
    matches = [row for row in rows if isinstance(row, dict) and row.get("host_id", row.get("physical_host_id")) == host.host_id]
    if len(matches) != 1:
        raise P5ExecutionError(f"inventory host authority is ambiguous for {host.host_id}")
    row = matches[0]
    try:
        manifest = json.loads(bundle_manifest)
        candidate = manifest["candidate"]
        identity_public_key = str(row["identity_public_key"])
        receipt_public_key = str(row["receipt_public_key"])
        previous_generation = str(row["previous_generation"])
    except (KeyError, TypeError, json.JSONDecodeError) as error:
        raise P5ExecutionError("inventory/bundle cannot form a session config") from error
    controller_public = controller.public_key().public_bytes_raw().hex()
    if inventory.get("controller_application_public_key") != controller_public:
        raise P5ExecutionError("controller private key is not bound by inventory")
    for label, value in (("identity", identity_public_key), ("receipt", receipt_public_key)):
        try:
            if len(bytes.fromhex(value)) != 32 or value.lower() != value:
                raise ValueError(label)
        except ValueError as error:
            raise P5ExecutionError(f"{label} signer key is invalid") from error
    evidence_authority = _evidence_authority(inventory)
    runner_data_root = f"/var/lib/onebrain/p5-v2/{host.host_id}"
    if (
        not previous_generation.startswith("/opt/onebrain/base-v1/")
        or previous_generation.endswith("/current")
        or ".." in PurePosixPath(previous_generation).parts
    ):
        raise P5ExecutionError("previous generation is outside the immutable activation root")
    return {
        "activation_root": "/opt/onebrain/base-v1",
        "archive_input": f"{runner_data_root}/recovery-input/base.obar",
        "archive_recovery_key": f"{runner_data_root}/recovery-input/base.key",
        "base_dataset_root": f"{runner_data_root}/recovery-input/base-dataset",
        "base_release_policy_blake3": blake3.blake3(base_policy).hexdigest(),
        "bundle_manifest_blake3": blake3.blake3(bundle_manifest).hexdigest(),
        "candidate_commit": str(candidate["id"]),
        "candidate_tree": str(candidate["version"]),
        "controller_application_public_key": controller_public,
        "expires_at": int(request["expires_at"]),
        "format": 2,
        "host_id": host.host_id,
        "identity_signer_public_key": identity_public_key,
        "evidence_authority": evidence_authority,
        "inventory_blake3": evidence_authority["inventory_blake3"],
        "p5_approval_policy_blake3": blake3.blake3(canonical_json(json.loads(p5_approval_policy))).hexdigest(),
        "p5_request_blake3": blake3.blake3(canonical_json(json.loads(p5_request))).hexdigest(),
        "p5_signature_blake3": blake3.blake3(p5_signature).hexdigest(),
        "profile_blake3": str(request["profile_blake3"]),
        "previous_generation": previous_generation,
        "receipt_signer_public_key": receipt_public_key,
        "release_request_blake3": blake3.blake3(release_request).hexdigest(),
        "release_signature_blake3": blake3.blake3(release_signature).hexdigest(),
        "request_digest": blake3.blake3(canonical_json(dict(request))).hexdigest(),
        "runner_data_root": runner_data_root,
        "session_id": str(request["session_id"]),
        "vector_blake3": str(request["vector_blake3"]),
    }


def _signed_bootstrap_command(
    host: HostConfigV2,
    inventory: Mapping[str, object],
    request: Mapping[str, object],
    controller: Ed25519PrivateKey,
    *,
    release_request: bytes,
    release_signature: bytes,
    base_policy: bytes,
    base_keyring: bytes,
    p5_request: bytes,
    p5_signature: bytes,
    p5_approval_policy: bytes,
    bundle_manifest: bytes,
) -> CanonicalCommandV2:
    canonical_p5_request = canonical_json(json.loads(p5_request))
    canonical_p5_policy = canonical_json(json.loads(p5_approval_policy))
    session_config = _bootstrap_session_config(
        host, inventory, request, controller,
        release_request=release_request,
        release_signature=release_signature,
        base_policy=base_policy,
        p5_request=p5_request,
        p5_signature=p5_signature,
        p5_approval_policy=p5_approval_policy,
        bundle_manifest=bundle_manifest,
    )
    issued_at = int(time.time())
    unsigned = {
        "base_keyring_hex": base_keyring.hex(),
        "base_policy_hex": base_policy.hex(),
        "bundle_manifest_hex": bundle_manifest.hex(),
        # The remote validator permits the controller clock to lead by 30s,
        # but also rejects expiries more than 300s beyond its local clock.
        # Leave that skew budget here instead of consuming the full 300s.
        "expires_at": min(int(request["expires_at"]), issued_at + BOOTSTRAP_TTL_SECONDS),
        "format": 2,
        "host_id": host.host_id,
        "inventory_hex": canonical_json(dict(inventory)).hex(),
        "issued_at": issued_at,
        "kind": "bootstrap",
        "operation_id": blake3.blake3(canonical_json({"host_id": host.host_id, "session_id": request["session_id"]})).hexdigest(),
        "p5_approval_policy_hex": canonical_p5_policy.hex(),
        "p5_request_hex": canonical_p5_request.hex(),
        "p5_signature_hex": p5_signature.hex(),
        "release_request_hex": release_request.hex(),
        "release_signature_hex": release_signature.hex(),
        "session_config": session_config,
    }
    signature = controller.sign(BOOTSTRAP_DOMAIN + canonical_json(unsigned))
    return CanonicalCommandV2.create(1, {**unsigned, "signature": signature.hex()})


def verify_bootstrap_response(host_id: str, encoded: bytes, expected_config_blake3: str) -> dict[str, object]:
    if not 0 < len(encoded) <= MAX_RECEIPT_BYTES:
        raise P5ExecutionError("bootstrap response size is invalid")
    try:
        value = json.loads(encoded)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise P5ExecutionError("bootstrap response is invalid JSON") from error
    required = {"format", "host_id", "installed_config_blake3", "network_changed", "operation_id", "units_changed"}
    if (
        not isinstance(value, dict)
        or set(value) != required
        or value.get("format") != 2
        or value.get("host_id") != host_id
        or value.get("installed_config_blake3") != expected_config_blake3
        or value.get("network_changed") is not False
        or value.get("units_changed") is not False
    ):
        raise P5ExecutionError("bootstrap response authority/effect mismatch")
    if encoded.strip() != canonical_json(value):
        raise P5ExecutionError("bootstrap response encoding is noncanonical")
    return value


def verify_finalization_response(host_id: str, encoded: bytes, cleanup_receipt_blake3: str) -> dict[str, object]:
    if not 0 < len(encoded) <= MAX_RECEIPT_BYTES:
        raise P5ExecutionError("finalization response size is invalid")
    try:
        value = json.loads(encoded)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise P5ExecutionError("finalization response is invalid JSON") from error
    required = {"cleanup_receipt_blake3", "format", "host_id", "operation", "session_config_removed", "signer_stopped"}
    if (
        not isinstance(value, dict)
        or set(value) != required
        or value.get("format") != 2
        or value.get("host_id") != host_id
        or value.get("cleanup_receipt_blake3") != cleanup_receipt_blake3
        or value.get("session_config_removed") is not True
        or value.get("signer_stopped") is not True
        or not isinstance(value.get("operation"), dict)
    ):
        raise P5ExecutionError("finalization response authority/effect mismatch")
    if encoded.strip() != canonical_json(value):
        raise P5ExecutionError("finalization response encoding is noncanonical")
    return value


def _base_policy_fingerprint(policy: Mapping[str, object]) -> str:
    candidate: Mapping[str, object]
    if policy.get("format") == "onebrain/base-v1-release-signers/1":
        rows = policy.get("policies")
        if not isinstance(rows, list):
            raise P5ExecutionError("base signer vector has no policies")
        matches = [row.get("policy") for row in rows if isinstance(row, dict) and isinstance(row.get("policy"), dict) and row["policy"].get("role") == "qualification-approver"]
        if len(matches) != 1:
            raise P5ExecutionError("base qualification policy is ambiguous")
        candidate = matches[0]
    elif isinstance(policy.get("policy"), dict):
        candidate = policy["policy"]
    else:
        candidate = policy
    signers = candidate.get("signers")
    if candidate.get("format") != "onebrain/base-v1-qualification-approver-policy/1" or not isinstance(signers, list) or len(signers) != 1 or not isinstance(signers[0], dict):
        raise P5ExecutionError("base qualification policy is invalid")
    fingerprint = signers[0].get("fingerprint")
    if not isinstance(fingerprint, str) or len(fingerprint) != 40 or any(value not in "0123456789ABCDEF" for value in fingerprint):
        raise P5ExecutionError("base qualification fingerprint is invalid")
    return fingerprint


def _bootstrap_material(args: argparse.Namespace) -> dict[str, bytes]:
    base_policy = args.base_policy.read_bytes()
    policy = json.loads(base_policy)
    fingerprint = _base_policy_fingerprint(policy)
    gpg = "/usr/bin/gpg" if Path("/usr/bin/gpg").is_file() else "gpg"
    exported = subprocess.run(
        [gpg, "--homedir", str(args.base_gpg_home), "--batch", "--export", fingerprint],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if exported.returncode != 0 or not exported.stdout or len(exported.stdout) > 131_072:
        raise P5ExecutionError("base qualification keyring export failed")
    paths = {
        "release_request": args.release_request,
        "release_signature": args.release_signature,
        "p5_request": args.p5_request,
        "p5_signature": args.p5_signature,
        "p5_approval_policy": args.p5_approval_policy,
        "bundle_manifest": args.bundle_root / "metadata" / "bundle.manifest.json",
    }
    values = {name: path.read_bytes() for name, path in paths.items()}
    values.update(base_policy=base_policy, base_keyring=exported.stdout)
    if any(not value or len(value) > 262_144 for value in values.values()):
        raise P5ExecutionError("bootstrap authority input is empty or outside its bound")
    return values


def verify_admin_response(
    host_id: str,
    encoded: bytes,
    expected_public_key: bytes,
) -> dict[str, object]:
    if not 0 < len(encoded) <= MAX_RECEIPT_BYTES:
        raise P5ExecutionError("admin response size is invalid")
    try:
        value = json.loads(encoded)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise P5ExecutionError("admin response is invalid JSON") from error
    if not isinstance(value, dict) or set(value) != {"receipt", "signature", "signer_public_key"}:
        raise P5ExecutionError("admin response schema is not closed")
    receipt = value["receipt"]
    required_receipt = {
        "accepted", "action", "evidence_authority", "fault", "format", "frame_blake3",
        "host_id", "inventory_blake3", "observation", "request_digest", "sequence", "session_id",
    }
    authority = receipt.get("evidence_authority") if isinstance(receipt, dict) else None
    if (
        not isinstance(receipt, dict)
        or set(receipt) != required_receipt
        or receipt.get("host_id") != host_id
        or receipt.get("accepted") is not True
        or receipt.get("format") != 2
        or not isinstance(authority, dict)
        or set(authority) != {
            "inventory_blake3", "provider_evidence_blake3", "provider_evidence_status",
            "public_probe_set_blake3", "qualification_tier", "topology_attestation_blake3",
        }
        or receipt.get("inventory_blake3") != authority.get("inventory_blake3")
        or authority.get("qualification_tier") != "production-reference"
    ):
        raise P5ExecutionError("admin response host substitution")
    if value.get("signer_public_key") != expected_public_key.hex():
        raise P5ExecutionError("admin receipt signer substitution")
    try:
        signature = bytes.fromhex(str(value["signature"]))
        if len(signature) != 64:
            raise ValueError("signature length")
        Ed25519PublicKey.from_public_bytes(expected_public_key).verify(
            signature,
            b"onebrain/p5/admin-operation-receipt/v2" + canonical_json(receipt),
        )
    except (ValueError, InvalidSignature) as error:
        raise P5ExecutionError("admin receipt signature is invalid") from error
    normalized = canonical_json(value)
    if encoded.strip() != normalized:
        raise P5ExecutionError("admin response encoding is noncanonical")
    return value


def _production_receipt_verifier(
    expected_public_keys: Mapping[str, bytes],
    expected_authority: Mapping[str, object] | None = None,
    expected_request_digest: str | None = None,
) -> Callable[[str, bytes], SignedChildReceiptV2]:
    required = {
        "command_blake3", "evidence_authority", "format", "host_id", "inventory_blake3",
        "issued_at", "request_digest", "result", "sequence", "session_id",
        "signer_public_key", "signature",
    }

    def verify(host_id: str, encoded: bytes) -> SignedChildReceiptV2:
        if not 0 < len(encoded) <= MAX_RECEIPT_BYTES:
            raise P5ExecutionError("child receipt size is invalid")
        try:
            value = json.loads(encoded)
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise P5ExecutionError("child receipt is invalid JSON") from error
        if not isinstance(value, dict) or set(value) != required or value.get("format") != 2:
            raise P5ExecutionError("child receipt schema is not closed V2")
        if value.get("host_id") != host_id or not isinstance(value.get("sequence"), int) or value["sequence"] <= 0:
            raise P5ExecutionError("child receipt host/sequence mismatch")
        if expected_authority is not None and value.get("evidence_authority") != dict(expected_authority):
            raise P5ExecutionError("child receipt evidence authority substitution")
        if value.get("inventory_blake3") != value.get("evidence_authority", {}).get("inventory_blake3"):
            raise P5ExecutionError("child receipt inventory authority mismatch")
        if expected_request_digest is not None and value.get("request_digest") != expected_request_digest:
            raise P5ExecutionError("child receipt request substitution")
        public = expected_public_keys.get(host_id)
        if public is None or value.get("signer_public_key") != public.hex():
            raise P5ExecutionError("child receipt signer substitution")
        signature_hex = value.pop("signature")
        signer_public_key = value.pop("signer_public_key")
        try:
            signature = bytes.fromhex(signature_hex)
            if len(signature) != 64:
                raise ValueError("signature length")
            Ed25519PublicKey.from_public_bytes(public).verify(
                signature,
                b"onebrain/p5/child-receipt/v2" + canonical_json(value),
            )
        except (ValueError, InvalidSignature) as error:
            raise P5ExecutionError("child receipt signature is invalid") from error
        value["signer_public_key"] = signer_public_key
        value["signature"] = signature_hex
        normalized = canonical_json(value)
        if encoded != normalized:
            raise P5ExecutionError("child receipt encoding is noncanonical")
        return SignedChildReceiptV2(host_id, value["sequence"], normalized)

    return verify


def _inventory_host_configs(
    inventory: Mapping[str, object], evidence_root: Path,
) -> tuple[tuple[HostConfigV2, ...], dict[str, bytes], dict[str, Path]]:
    rows = inventory.get("hosts")
    if not isinstance(rows, list) or len(rows) != 3:
        raise P5ExecutionError("signed inventory must contain exactly three hosts")
    hosts: list[HostConfigV2] = []
    receipt_keys: dict[str, bytes] = {}
    known_hosts: dict[str, Path] = {}
    known_root = evidence_root / "p5" / "controller-known-hosts"
    known_root.mkdir(parents=True, exist_ok=True)
    for row in rows:
        if not isinstance(row, dict):
            raise P5ExecutionError("inventory host row is invalid")
        host_id = str(row.get("host_id", row.get("physical_host_id", "")))
        destination = str(row.get("ssh_destination", ""))
        if "@" not in destination:
            raise P5ExecutionError(f"{host_id} has no signed SSH destination")
        default_user, ssh_host = destination.split("@", 1)
        host_public_key = str(row.get("ssh_host_public_key", ""))
        receipt_hex = str(row.get("receipt_public_key", ""))
        try:
            receipt_key = bytes.fromhex(receipt_hex)
        except ValueError as error:
            raise P5ExecutionError(f"{host_id} receipt key is invalid") from error
        if len(receipt_key) != 32 or not host_public_key.startswith("ssh-ed25519 "):
            raise P5ExecutionError(f"{host_id} lacks signed service/host public keys")
        ssh_port = int(row.get("ssh_port", 22))
        known_host = ssh_host if ssh_port == 22 else f"[{ssh_host}]:{ssh_port}"
        known_path = known_root / host_id
        _write_create_new(known_path, f"{known_host} {host_public_key}\n".encode())
        known_hosts[host_id] = known_path
        receipt_keys[host_id] = receipt_key
        hosts.append(HostConfigV2(
            host_id=host_id,
            runner_id=str(row.get("runner_id", row.get("runner_identity", ""))),
            ssh_host=ssh_host,
            ssh_port=ssh_port,
            runner_ssh_user=str(row.get("runner_ssh_user", default_user)),
            admin_ssh_user=str(row.get("admin_ssh_user", default_user)),
            ssh_host_public_key=host_public_key,
            host_key_sha256=str(row.get("ssh_host_key_sha256", row.get("ssh_host_key_fingerprint", ""))),
            runner_authorized_key_line_blake3=bytes.fromhex(str(row.get("runner_authorized_key_line_blake3", "00" * 32))),
            admin_authorized_key_line_blake3=bytes.fromhex(str(row.get("admin_authorized_key_line_blake3", "00" * 32))),
            evidence_root=str(row.get("evidence_root", "/var/lib/onebrain/p5-v2/evidence")),
        ))
    if {host.host_id for host in hosts} != set(REQUIRED_HOSTS):
        raise P5ExecutionError("inventory host identities are not host-a/host-b/host-c")
    return tuple(hosts), receipt_keys, known_hosts


def _inventory_relay_descriptors(inventory: Mapping[str, object]) -> tuple[str, ...]:
    probes = inventory.get("public_probe_sets")
    if not isinstance(probes, list) or not 4 <= len(probes) <= 24:
        raise P5ExecutionError("inventory must carry bounded cross-host relay probe receipts")
    descriptors: dict[str, set[str]] = {}
    validity: dict[str, tuple[int, int]] = {}
    for row in probes:
        value = row.get("relay_descriptor_hex") if isinstance(row, dict) else None
        if (
            not isinstance(value, str)
            or not value
            or len(value) > 131_072
            or len(value) % 2
            or value.lower() != value
        ):
            raise P5ExecutionError("public relay probe set lacks canonical relay_descriptor_hex")
        try:
            bytes.fromhex(value)
        except ValueError as error:
            raise P5ExecutionError("public relay descriptor is not canonical hex") from error
        source_host = row.get("source_host_id")
        relay_host = row.get("relay_host_id")
        endpoint_probes = row.get("probes")
        issued_at = row.get("descriptor_issued_at")
        expires_at = row.get("descriptor_expires_at")
        if (
            source_host not in REQUIRED_HOSTS
            or relay_host not in REQUIRED_HOSTS
            or source_host == relay_host
            or not isinstance(endpoint_probes, list)
            or not endpoint_probes
            or any(not isinstance(probe, dict) or probe.get("success") is not True for probe in endpoint_probes)
        ):
            raise P5ExecutionError("relay probe is not a successful distinct-host transcript")
        if (
            not isinstance(issued_at, int)
            or isinstance(issued_at, bool)
            or not isinstance(expires_at, int)
            or isinstance(expires_at, bool)
            or issued_at < 0
            or expires_at <= issued_at
            or expires_at - issued_at > MAX_RELAY_DESCRIPTOR_VALIDITY_SECONDS
        ):
            raise P5ExecutionError("relay probe lacks bounded descriptor validity metadata")
        descriptor_validity = (issued_at, expires_at)
        if value in validity and validity[value] != descriptor_validity:
            raise P5ExecutionError("relay descriptor validity metadata disagrees across probe receipts")
        validity[value] = descriptor_validity
        descriptors.setdefault(value, set()).add(str(source_host))
    if not 2 <= len(descriptors) <= 3 or any(len(sources) < 2 for sources in descriptors.values()):
        raise P5ExecutionError("each relay descriptor requires two distinct remote probe hosts")
    return tuple(sorted(descriptors))


def _require_relay_descriptor_freshness(
    inventory: Mapping[str, object],
    now: int,
    minimum_remaining_seconds: int = 180,
) -> None:
    if now < 0 or minimum_remaining_seconds < 1:
        raise P5ExecutionError("relay descriptor freshness boundary is invalid")
    descriptors = _inventory_relay_descriptors(inventory)
    validity: dict[str, tuple[int, int]] = {}
    for row in inventory["public_probe_sets"]:
        descriptor = str(row["relay_descriptor_hex"])
        validity[descriptor] = (
            int(row["descriptor_issued_at"]),
            int(row["descriptor_expires_at"]),
        )
    for descriptor in descriptors:
        issued_at, expires_at = validity[descriptor]
        if issued_at > now + 30:
            raise P5ExecutionError("relay descriptor is issued in the future")
        if expires_at - now < minimum_remaining_seconds:
            raise P5ExecutionError(
                "relay descriptor freshness window is too short for a production wave"
            )


def _ring_neighbors(host_id: str) -> tuple[str, str]:
    order = REQUIRED_HOSTS
    index = order.index(host_id)
    return order[(index + 1) % len(order)], order[(index - 1) % len(order)]


def _require_ring_result(
    host_id: str,
    row: Mapping[str, object],
    outgoing_peer: str,
    incoming_peer: str,
) -> None:
    outgoing = row.get("outgoing")
    incoming = row.get("incoming")
    if not isinstance(outgoing, dict) or not isinstance(incoming, dict):
        raise P5ExecutionError(f"{host_id} ring receipt lacks both routed directions")
    for observed, expected in ((outgoing, outgoing_peer), (incoming, incoming_peer)):
        if (
            observed.get("expected_peer") != expected
            or observed.get("path_kind") not in ("Direct", "HolePunched", "RelayUdp", "RelayTcp443")
            or not isinstance(observed.get("session_id"), str)
            or len(str(observed["session_id"])) != 64
            or not isinstance(observed.get("route_receipt_blake3"), str)
            or len(str(observed["route_receipt_blake3"])) != 64
        ):
            raise P5ExecutionError(f"{host_id} did not prove the expected routed ring")


def _require_relay_ring(ring: Mapping[str, Mapping[str, object]]) -> None:
    if set(ring) != set(REQUIRED_HOSTS):
        raise P5ExecutionError("routed ring does not contain the three required hosts")
    outgoing_kinds = {
        str(row.get("outgoing", {}).get("path_kind"))
        for row in ring.values()
        if isinstance(row.get("outgoing"), dict)
    }
    relay = {"RelayUdp", "RelayTcp443"}
    if len(outgoing_kinds) == 0 or not outgoing_kinds <= relay:
        raise P5ExecutionError("outbound-first routed ring must contain only relay-class edges")


def _require_relay_matrix(
    matrix: Mapping[str, Mapping[str, object]], relay_descriptors: tuple[str, ...]
) -> None:
    expected = {
        blake3.blake3(bytes.fromhex(descriptor)).hexdigest()
        for descriptor in relay_descriptors
    }
    failures: list[str] = []
    if set(matrix) != set(REQUIRED_HOSTS):
        raise P5ExecutionError("relay diagnostic matrix does not contain the three required hosts")
    for host_id, row in matrix.items():
        probes = row.get("probes")
        if not isinstance(probes, list):
            failures.append(f"{host_id}: matrix command did not succeed")
            continue
        if row.get("success") is not True:
            failures.append(f"{host_id}: matrix aggregate failed")
        observed: set[str] = set()
        for probe in probes:
            if not isinstance(probe, dict):
                failures.append(f"{host_id}: malformed probe row")
                continue
            digest = probe.get("descriptor_blake3")
            if isinstance(digest, str):
                observed.add(digest)
            if probe.get("success") is not True:
                failures.append(
                    f"{host_id}:{probe.get('relay_node_id', 'unknown')}:{probe.get('error', 'unknown')}"
                )
        if observed != expected:
            failures.append(f"{host_id}: descriptor set mismatch")
    if failures:
        raise P5ExecutionError("relay diagnostic matrix failed: " + "; ".join(failures))


def _arm_mixed_direct_edge(
    executor: "OpenSshWaveExecutor",
    hosts: tuple[HostConfigV2, ...],
    agents: tuple[RunningAgent, ...],
    request: Mapping[str, object],
    controller: Ed25519PrivateKey,
    advertisements: Mapping[str, Mapping[str, object]],
    *,
    agent_sequence: int,
    deadline_monotonic_ns: int,
) -> int:
    """Arm the fixed B->C direct edge using B's relay-signed reflexive address.

    The C-side outbound prime and B-side dial share the real QUIC listener.  If
    the NATs do not permit this direct edge, the wave fails closed instead of
    relabelling a relay carrier as direct.
    """
    source = advertisements["host-b"]
    observations = source.get("reflexive_observations")
    if not isinstance(observations, list) or not observations:
        raise P5ExecutionError("host-b publication lacks a relay-signed reflexive observation")
    observation = observations[0]
    if (
        not isinstance(observation, str)
        or not observation
        or len(observation) > 131_072
        or len(observation) % 2
        or observation.lower() != observation
    ):
        raise P5ExecutionError("host-b reflexive observation is not canonical hex")
    try:
        bytes.fromhex(observation)
    except ValueError as error:
        raise P5ExecutionError("host-b reflexive observation is not canonical hex") from error
    index = next(index for index, host in enumerate(hosts) if host.host_id == "host-c")
    receipt = executor.execute_wave(
        (agents[index],),
        (_signed_agent_command(
            hosts[index],
            request,
            controller,
            agent_sequence,
            "arm-direct-inbound",
            {
                "expected_peer": source["peer_node_id"],
                "peer_public_key": source["peer_public_key"],
                "peer_advertisement_hex": source["advertisement_hex"],
                "peer_reflexive_observation_hex": observation,
            },
        ),),
        deadline_monotonic_ns,
    )[0]
    result = _receipt_result(receipt, "arm-direct-inbound")
    if (
        result.get("expected_peer") != source["peer_node_id"]
        or not isinstance(result.get("observation_blake3"), str)
        or len(str(result["observation_blake3"])) != 64
    ):
        raise P5ExecutionError("host-c did not prove the armed direct edge")
    return agent_sequence + 1


def _agent_result_map(
    receipts: tuple[SignedChildReceiptV2, ...], expected_command: str,
) -> dict[str, dict[str, object]]:
    return {
        receipt.host_id: _receipt_result(receipt, expected_command)
        for receipt in receipts
    }


def execute_fault_cycle(
    executor: "OpenSshWaveExecutor",
    hosts: tuple[HostConfigV2, ...],
    agents: tuple[RunningAgent, ...],
    credentials: ControllerCredentialsV2,
    receipt_public_keys: Mapping[str, bytes],
    request: Mapping[str, object],
    controller: Ed25519PrivateKey,
    fault: str,
    expected_peers: Mapping[str, str],
    *,
    agent_sequence: int,
    admin_sequence: int,
    deadline_monotonic_ns: int,
    selected_host_id: str | None = None,
    phase_hook: Callable[[str, str, tuple[RunningAgent, ...], int], tuple[tuple[RunningAgent, ...], int]] | None = None,
) -> tuple[tuple[RunningAgent, ...], int, int, dict[str, object]]:
    """Execute one closed before/apply/clear cycle and bind every phase to an agent receipt.

    The helper deliberately does not hide process recovery.  If an operation
    restarts the agent, the following measurement fails closed; the caller
    must reopen and rehydrate the affected production bridge before retrying
    the cycle from a new signed operation ID.
    """
    if fault not in REQUIRED_FAULTS:
        raise P5ExecutionError("fault is outside the production matrix")
    selected_hosts = hosts
    selected_agents = agents
    if selected_host_id is not None:
        indexes = [index for index, host in enumerate(hosts) if host.host_id == selected_host_id]
        if len(indexes) != 1:
            raise P5ExecutionError("selected relay host is absent or ambiguous")
        index = indexes[0]
        selected_hosts = (hosts[index],)
        selected_agents = (agents[index],)

    operation_ids = {
        host.host_id: blake3.blake3(
            canonical_json({
                "fault": fault,
                "host_id": host.host_id,
                "session_id": request["session_id"],
            })
        ).hexdigest()
        for host in selected_hosts
    }
    target_receipts = executor.execute_wave(
        selected_agents,
        tuple(
            _signed_agent_command(
                host,
                request,
                controller,
                agent_sequence,
                "prepare-fault-target",
                {
                    "expected_peer": expected_peers[host.host_id],
                    "fault": fault,
                    "operation_id": operation_ids[host.host_id],
                },
            )
            for host in selected_hosts
        ),
        deadline_monotonic_ns,
    )
    targets = _agent_result_map(target_receipts, "prepare-fault-target")
    agent_sequence += 1
    phases: dict[str, dict[str, object]] = {}
    for action, phase in (("observe", "before"), ("apply", "during"), ("clear", "after")):
        commands = []
        for host in selected_hosts:
            target = targets[host.host_id].get("target")
            if not isinstance(target, dict):
                raise P5ExecutionError("agent fault target is missing")
            endpoint_values = target.get("peer_endpoints")
            if not isinstance(endpoint_values, list) or not endpoint_values:
                raise P5ExecutionError("agent fault target has no measured endpoint")
            parameters: dict[str, object]
            if fault in ("base-obarv002-archive-restore", "rollback", "explicit-re-enable"):
                parameters = {}
            else:
                parameters = {"peer_endpoints": endpoint_values}
            commands.append(
                _signed_admin_command(
                    host,
                    request,
                    controller,
                    admin_sequence,
                    action,
                    fault=fault,
                    phase=phase,
                    parameters=parameters,
                )
            )
        responses = executor.execute_admin_wave(
            selected_hosts,
            tuple(commands),
            credentials,
            receipt_public_keys,
            deadline_monotonic_ns,
        )
        admin_sequence += 1
        if phase_hook is not None:
            agents, agent_sequence = phase_hook(fault, phase, agents, agent_sequence)
            selected_agents = tuple(
                agents[next(index for index, candidate in enumerate(hosts) if candidate.host_id == host.host_id)]
                for host in selected_hosts
            )
        measurement_receipts = executor.execute_wave(
            selected_agents,
            tuple(
                _signed_agent_command(
                    host,
                    request,
                    controller,
                    agent_sequence,
                    "measure-fault-boundary",
                    {
                        "admin_response": response,
                        "fault": fault,
                        "phase": phase,
                    },
                )
                for host, response in zip(selected_hosts, responses, strict=True)
            ),
            deadline_monotonic_ns,
        )
        measurements = _agent_result_map(measurement_receipts, "measure-fault-boundary")
        phases[phase] = {
            "admin_response_blake3": {
                host.host_id: blake3.blake3(canonical_json(response)).hexdigest()
                for host, response in zip(selected_hosts, responses, strict=True)
            },
            "measurements": measurements,
        }
        agent_sequence += 1
    return agents, agent_sequence, admin_sequence, {
        "fault": fault,
        "operation_ids": operation_ids,
        "phases": phases,
        "target_receipt_blake3": {
            receipt.host_id: blake3.blake3(receipt.canonical_bytes).hexdigest()
            for receipt in target_receipts
        },
    }


def rehydrate_relay_ring(
    executor: "OpenSshWaveExecutor",
    hosts: tuple[HostConfigV2, ...],
    agents: tuple[RunningAgent, ...],
    request: Mapping[str, object],
    controller: Ed25519PrivateKey,
    relay_descriptors: tuple[str, ...],
    *,
    agent_sequence: int,
    deadline_monotonic_ns: int,
) -> tuple[tuple[RunningAgent, ...], int, dict[str, dict[str, object]], dict[str, dict[str, object]]]:
    """Rebuild all volatile runtimes and both ring directions after a real process/network fault."""
    executor.execute_wave(
        agents,
        tuple(_signed_agent_command(host, request, controller, agent_sequence, "shutdown") for host in hosts),
        deadline_monotonic_ns,
    )
    agent_sequence += 1
    started = executor.execute_wave(
        agents,
        tuple(_signed_agent_command(host, request, controller, agent_sequence, "start-reachability") for host in hosts),
        deadline_monotonic_ns,
    )
    started_rows = _agent_result_map(started, "start-reachability")
    if any(row.get("bind") != "0.0.0.0:41010" for row in started_rows.values()):
        raise P5ExecutionError("rehydrated runtime did not bind the frozen endpoint")
    agent_sequence += 1
    diagnosed = executor.execute_wave(
        agents,
        tuple(
            _signed_agent_command(
                host,
                request,
                controller,
                agent_sequence,
                "diagnose-relay-matrix",
                {"relay_descriptors": relay_descriptors},
            )
            for host in hosts
        ),
        deadline_monotonic_ns,
    )
    _require_relay_matrix(_agent_result_map(diagnosed, "diagnose-relay-matrix"), relay_descriptors)
    agent_sequence += 1
    reserved = executor.execute_wave(
        agents,
        tuple(
            _signed_agent_command(host, request, controller, agent_sequence, "ensure-reservations", {"relay_descriptors": relay_descriptors})
            for host in hosts
        ),
        deadline_monotonic_ns,
    )
    if any(len(_receipt_result(receipt, "ensure-reservations").get("grant_digests", [])) != len(relay_descriptors) for receipt in reserved):
        raise P5ExecutionError("rehydrated runtime lacks the required reservations")
    agent_sequence += 1
    published = executor.execute_wave(
        agents,
        tuple(_signed_agent_command(host, request, controller, agent_sequence, "publish-advertisement") for host in hosts),
        deadline_monotonic_ns,
    )
    advertisements = _agent_result_map(published, "publish-advertisement")
    agent_sequence += 1
    commands = []
    for host in hosts:
        outgoing_host, incoming_host = _ring_neighbors(host.host_id)
        outgoing = advertisements[outgoing_host]
        incoming = advertisements[incoming_host]
        commands.append(_signed_agent_command(
            host, request, controller, agent_sequence, "connect-ring",
            {
                "incoming_advertisement_hex": incoming["advertisement_hex"],
                "incoming_expected_peer": incoming["peer_node_id"],
                "incoming_peer_public_key": incoming["peer_public_key"],
                "outgoing_advertisement_hex": outgoing["advertisement_hex"],
                "outgoing_expected_peer": outgoing["peer_node_id"],
                "outgoing_peer_public_key": outgoing["peer_public_key"],
            },
        ))
    connected = executor.execute_wave(agents, tuple(commands), deadline_monotonic_ns)
    ring = _agent_result_map(connected, "connect-ring")
    for host_id, row in ring.items():
        outgoing_host, incoming_host = _ring_neighbors(host_id)
        _require_ring_result(host_id, row, str(advertisements[outgoing_host]["peer_node_id"]), str(advertisements[incoming_host]["peer_node_id"]))
    _require_relay_ring(ring)
    return agents, agent_sequence + 1, advertisements, ring


def reconnect_existing_ring(
    executor: "OpenSshWaveExecutor",
    hosts: tuple[HostConfigV2, ...],
    agents: tuple[RunningAgent, ...],
    request: Mapping[str, object],
    controller: Ed25519PrivateKey,
    advertisements: Mapping[str, Mapping[str, object]],
    *,
    agent_sequence: int,
    deadline_monotonic_ns: int,
) -> tuple[tuple[RunningAgent, ...], int, dict[str, dict[str, object]]]:
    commands = []
    for host in hosts:
        outgoing_host, incoming_host = _ring_neighbors(host.host_id)
        outgoing = advertisements[outgoing_host]
        incoming = advertisements[incoming_host]
        commands.append(_signed_agent_command(
            host, request, controller, agent_sequence, "reconnect-ring",
            {
                "incoming_advertisement_hex": incoming["advertisement_hex"],
                "incoming_expected_peer": incoming["peer_node_id"],
                "incoming_peer_public_key": incoming["peer_public_key"],
                "outgoing_advertisement_hex": outgoing["advertisement_hex"],
                "outgoing_expected_peer": outgoing["peer_node_id"],
                "outgoing_peer_public_key": outgoing["peer_public_key"],
            },
        ))
    connected = executor.execute_wave(agents, tuple(commands), deadline_monotonic_ns)
    ring = _agent_result_map(connected, "reconnect-ring")
    for host_id, row in ring.items():
        outgoing_host, incoming_host = _ring_neighbors(host_id)
        _require_ring_result(host_id, row, str(advertisements[outgoing_host]["peer_node_id"]), str(advertisements[incoming_host]["peer_node_id"]))
    _require_relay_ring(ring)
    return agents, agent_sequence + 1, ring


def _relay_host_map(inventory: Mapping[str, object]) -> dict[str, str]:
    """Resolve the private operational relay-to-host map from signed inventory."""
    result: dict[str, str] = {}
    for row in inventory.get("hosts", []):
        if not isinstance(row, dict):
            continue
        host_id = str(row.get("host_id", row.get("physical_host_id", "")))
        relay_node_id = row.get("relay_node_id")
        if host_id in REQUIRED_HOSTS and isinstance(relay_node_id, str):
            result[relay_node_id] = host_id
    for row in inventory.get("public_probe_sets", []):
        if not isinstance(row, dict):
            continue
        host_id = row.get("relay_host_id")
        relay_node_id = row.get("relay_node_id")
        if host_id in REQUIRED_HOSTS and isinstance(relay_node_id, str):
            existing = result.setdefault(relay_node_id, str(host_id))
            if existing != host_id:
                raise P5ExecutionError("relay physical-host mapping is contradictory")
    if len(result) < 2:
        raise P5ExecutionError("inventory lacks two signed relay physical-host mappings")
    return result


def _record_checkpoints(
    executor: "OpenSshWaveExecutor",
    hosts: tuple[HostConfigV2, ...],
    agents: tuple[RunningAgent, ...],
    request: Mapping[str, object],
    controller: Ed25519PrivateKey,
    advertisements: Mapping[str, Mapping[str, object]],
    ring: Mapping[str, Mapping[str, object]],
    *,
    acknowledged_sequence: int,
    basis: Mapping[str, Mapping[str, object]] | None = None,
    agent_sequence: int,
    deadline_monotonic_ns: int,
) -> tuple[int, dict[str, dict[str, object]]]:
    commands = []
    projections: dict[str, dict[str, object]] = {}
    for host in hosts:
        target_host, _ = _ring_neighbors(host.host_id)
        target_peer = str(advertisements[target_host]["peer_node_id"])
        outgoing = ring[host.host_id]["outgoing"]
        prior = basis.get(host.host_id) if basis is not None else None
        intent = str(prior["intent"]) if prior is not None else blake3.blake3(canonical_json({
            "edge": [host.host_id, target_host],
            "request": request["session_id"],
        })).hexdigest()
        roots = str(prior["roots"]) if prior is not None else blake3.blake3(canonical_json({
            "route_receipt_blake3": outgoing["route_receipt_blake3"],
            "transport_binding_blake3": outgoing["transport_binding_blake3"],
        })).hexdigest()
        projections[host.host_id] = {
            "intent": intent,
            "roots": roots,
            "sequence": acknowledged_sequence,
        }
        commands.append(_signed_agent_command(
            host, request, controller, agent_sequence, "record-checkpoint",
            {
                "acknowledged_sequence": acknowledged_sequence,
                "expected_peer": target_peer,
                "intent_blake3": intent,
                "roots_blake3": roots,
            },
        ))
    receipts = executor.execute_wave(agents, tuple(commands), deadline_monotonic_ns)
    for receipt in receipts:
        result = _receipt_result(receipt, "record-checkpoint")
        checkpoint = result.get("checkpoint")
        expected = projections[receipt.host_id]
        if (
            not isinstance(checkpoint, dict)
            or checkpoint.get("acknowledged_sequence") != expected["sequence"]
            or checkpoint.get("intent_blake3") != expected["intent"]
            or checkpoint.get("roots_blake3") != expected["roots"]
        ):
            raise P5ExecutionError("agent checkpoint projection mismatch")
    return agent_sequence + 1, projections


def _selected_relay_failover(
    executor: "OpenSshWaveExecutor",
    hosts: tuple[HostConfigV2, ...],
    agents: tuple[RunningAgent, ...],
    credentials: ControllerCredentialsV2,
    receipt_public_keys: Mapping[str, bytes],
    request: Mapping[str, object],
    controller: Ed25519PrivateKey,
    inventory: Mapping[str, object],
    advertisements: Mapping[str, Mapping[str, object]],
    ring: Mapping[str, Mapping[str, object]],
    checkpoints: Mapping[str, Mapping[str, object]],
    *,
    agent_sequence: int,
    admin_sequence: int,
    deadline_monotonic_ns: int,
) -> tuple[int, int, dict[str, dict[str, object]], dict[str, object]]:
    relay_sources = [
        host_id for host_id, row in ring.items()
        if _canonical_path_kind(row["outgoing"]["path_kind"]) in RELAY_CLASS
        and isinstance(row["outgoing"].get("selected_relay"), str)
    ]
    if not relay_sources:
        raise P5ExecutionError("relay ring has no selected relay route to fail")
    source = relay_sources[0]
    source_index = next(index for index, host in enumerate(hosts) if host.host_id == source)
    source_host = hosts[source_index]
    source_agent = agents[source_index]
    target_host, _ = _ring_neighbors(source)
    selected_relay = str(ring[source]["outgoing"]["selected_relay"])
    relay_host_id = _relay_host_map(inventory).get(selected_relay)
    if relay_host_id is None:
        raise P5ExecutionError("selected relay has no signed physical host mapping")
    relay_index = next(index for index, host in enumerate(hosts) if host.host_id == relay_host_id)
    relay_host = hosts[relay_index]
    operation_id = blake3.blake3(canonical_json({
        "fault": "selected-relay-shutdown",
        "host_id": relay_host_id,
        "session_id": request["session_id"],
        "source": source,
    })).hexdigest()
    target_receipt = executor.execute_wave(
        (source_agent,),
        (_signed_agent_command(source_host, request, controller, agent_sequence, "prepare-fault-target", {
            "expected_peer": advertisements[target_host]["peer_node_id"],
            "fault": "selected-relay-shutdown",
            "operation_id": operation_id,
        }),),
        deadline_monotonic_ns,
    )[0]
    target = _receipt_result(target_receipt, "prepare-fault-target").get("target")
    if not isinstance(target, dict) or target.get("selected_relay") != selected_relay:
        raise P5ExecutionError("selected relay target is not bound to the live route")
    endpoints = target.get("peer_endpoints")
    if not isinstance(endpoints, list) or not endpoints:
        raise P5ExecutionError("selected relay target lacks measured endpoints")
    agent_sequence += 1
    admin_responses: dict[str, object] = {}
    for action, phase in (("observe", "before"), ("apply", "during")):
        response = executor.execute_admin_wave(
            (relay_host,),
            (_signed_admin_command(
                relay_host, request, controller, admin_sequence, action,
                fault="selected-relay-shutdown", phase=phase,
                parameters={"peer_endpoints": endpoints},
            ),),
            credentials, receipt_public_keys, deadline_monotonic_ns,
        )[0]
        admin_responses[phase] = response
        admin_sequence += 1
    failure_observed_at = int(time.time())
    _, agent_sequence, resumed_ring = reconnect_existing_ring(
        executor, hosts, agents, request, controller, advertisements,
        agent_sequence=agent_sequence,
        deadline_monotonic_ns=deadline_monotonic_ns,
    )
    resumed_outgoing = resumed_ring[source]["outgoing"]
    alternate_relay = resumed_outgoing.get("selected_relay")
    if not isinstance(alternate_relay, str) or alternate_relay == selected_relay:
        raise P5ExecutionError("route did not fail over to a distinct pre-reserved relay")
    reservations = advertisements[source].get("reservation_records")
    if not isinstance(reservations, list):
        raise P5ExecutionError("source advertisement lacks reservation evidence")
    by_relay = {str(value.get("relay_node_id")): value for value in reservations if isinstance(value, dict)}
    if selected_relay not in by_relay or alternate_relay not in by_relay:
        raise P5ExecutionError("selected/alternate relay was not pre-reserved")
    agent_sequence, resumed_checkpoints = _record_checkpoints(
        executor, hosts, agents, request, controller, advertisements, resumed_ring,
        acknowledged_sequence=2,
        basis=checkpoints,
        agent_sequence=agent_sequence,
        deadline_monotonic_ns=deadline_monotonic_ns,
    )
    after = executor.execute_admin_wave(
        (relay_host,),
        (_signed_admin_command(
            relay_host, request, controller, admin_sequence, "clear",
            fault="selected-relay-shutdown", phase="after",
            parameters={"peer_endpoints": endpoints},
        ),),
        credentials, receipt_public_keys, deadline_monotonic_ns,
    )[0]
    admin_responses["after"] = after
    admin_sequence += 1
    prior = ring[source]["outgoing"]
    return agent_sequence, admin_sequence, resumed_ring, {
        "alternate_relay": alternate_relay,
        "alternate_reservation_issued_at": int(by_relay[alternate_relay]["issued_at"]),
        "admin_response_blake3": {
            phase: blake3.blake3(canonical_json(value)).hexdigest()
            for phase, value in admin_responses.items()
        },
        "failure_observed_at": failure_observed_at,
        "prior_binding": prior["transport_binding_blake3"],
        "prior_session": prior["session_id"],
        "resumed_binding": resumed_outgoing["transport_binding_blake3"],
        "resumed_checkpoint": resumed_checkpoints[source],
        "resumed_session": resumed_outgoing["session_id"],
        "selected_relay": selected_relay,
        "selected_reservation_issued_at": int(by_relay[selected_relay]["issued_at"]),
        "source": source,
    }


def _exercise_ring_markers(
    executor: "OpenSshWaveExecutor",
    hosts: tuple[HostConfigV2, ...],
    agents: tuple[RunningAgent, ...],
    request: Mapping[str, object],
    controller: Ed25519PrivateKey,
    advertisements: Mapping[str, Mapping[str, object]],
    *,
    label: str,
    agent_sequence: int,
    deadline_monotonic_ns: int,
) -> int:
    markers = {
        host.host_id: canonical_json({
            "host_id": host.host_id,
            "label": label,
            "session_id": request["session_id"],
        })
        for host in hosts
    }
    executor.execute_wave(
        agents,
        tuple(_signed_agent_command(
            host, request, controller, agent_sequence, "deliver-marker",
            {
                "expected_peer": advertisements[_ring_neighbors(host.host_id)[0]]["peer_node_id"],
                "payload_hex": markers[host.host_id].hex(),
            },
        ) for host in hosts),
        deadline_monotonic_ns,
    )
    agent_sequence += 1
    executor.execute_wave(
        agents,
        tuple(_signed_agent_command(
            host, request, controller, agent_sequence, "receive-marker",
            {
                "expected_blake3": blake3.blake3(markers[_ring_neighbors(host.host_id)[1]]).hexdigest(),
                "expected_bytes": len(markers[_ring_neighbors(host.host_id)[1]]),
                "expected_peer": advertisements[_ring_neighbors(host.host_id)[1]]["peer_node_id"],
            },
        ) for host in hosts),
        deadline_monotonic_ns,
    )
    return agent_sequence + 1


def execute_production_matrix(
    executor: "OpenSshWaveExecutor",
    hosts: tuple[HostConfigV2, ...],
    agents: tuple[RunningAgent, ...],
    credentials: ControllerCredentialsV2,
    receipt_public_keys: Mapping[str, bytes],
    request: Mapping[str, object],
    controller: Ed25519PrivateKey,
    inventory: Mapping[str, object],
    relay_descriptors: tuple[str, ...],
    advertisements: dict[str, dict[str, object]],
    initial_ring: dict[str, dict[str, object]],
    *,
    agent_sequence: int,
    admin_sequence: int,
    deadline_monotonic_ns: int,
) -> tuple[int, int, dict[str, dict[str, object]], dict[str, dict[str, object]], dict[str, object], list[dict[str, object]]]:
    """Run all real faults, selected-relay failover, and exact checkpoint resume."""
    current_ring = initial_ring
    agent_sequence, checkpoints = _record_checkpoints(
        executor, hosts, agents, request, controller, advertisements, current_ring,
        acknowledged_sequence=1,
        agent_sequence=agent_sequence,
        deadline_monotonic_ns=deadline_monotonic_ns,
    )
    fault_evidence: list[dict[str, object]] = []
    rehydrate_during = {
        "restart", "base-obarv002-archive-restore", "rollback", "explicit-re-enable",
    }
    rehydrate_after = {"address-change", "seed-outage"}
    for index, fault in enumerate(REQUIRED_FAULTS):
        selected_host = hosts[index % len(hosts)].host_id
        expected_peers = {
            host.host_id: str(advertisements[_ring_neighbors(host.host_id)[0]]["peer_node_id"])
            for host in hosts
        }

        def phase_hook(
            active_fault: str,
            phase: str,
            active_agents: tuple[RunningAgent, ...],
            sequence: int,
        ) -> tuple[tuple[RunningAgent, ...], int]:
            nonlocal advertisements, current_ring
            if (
                (phase == "during" and active_fault in rehydrate_during)
                or (phase == "after" and active_fault in rehydrate_after)
            ):
                active_agents, sequence, advertisements, current_ring = rehydrate_relay_ring(
                    executor, hosts, active_agents, request, controller, relay_descriptors,
                    agent_sequence=sequence,
                    deadline_monotonic_ns=deadline_monotonic_ns,
                )
            return active_agents, sequence

        agents, agent_sequence, admin_sequence, evidence = execute_fault_cycle(
            executor, hosts, agents, credentials, receipt_public_keys, request, controller,
            fault, expected_peers,
            agent_sequence=agent_sequence,
            admin_sequence=admin_sequence,
            deadline_monotonic_ns=deadline_monotonic_ns,
            selected_host_id=selected_host,
            phase_hook=phase_hook,
        )
        agent_sequence = _exercise_ring_markers(
            executor, hosts, agents, request, controller, advertisements,
            label=f"after:{fault}",
            agent_sequence=agent_sequence,
            deadline_monotonic_ns=deadline_monotonic_ns,
        )
        evidence["recovery_marker_verified"] = True
        fault_evidence.append(evidence)
    agent_sequence, admin_sequence, current_ring, failover = _selected_relay_failover(
        executor, hosts, agents, credentials, receipt_public_keys, request, controller,
        inventory, advertisements, current_ring, checkpoints,
        agent_sequence=agent_sequence,
        admin_sequence=admin_sequence,
        deadline_monotonic_ns=deadline_monotonic_ns,
    )
    agent_sequence = _exercise_ring_markers(
        executor, hosts, agents, request, controller, advertisements,
        label="after:selected-relay-failover",
        agent_sequence=agent_sequence,
        deadline_monotonic_ns=deadline_monotonic_ns,
    )
    return agent_sequence, admin_sequence, current_ring, checkpoints, failover, fault_evidence


def run_production_preflight(
    args: argparse.Namespace,
    request: Mapping[str, object],
    *,
    full_qualification: bool = False,
) -> None:
    inventory = _read_json(args.inventory, "P5 inventory")
    relay_descriptors = _inventory_relay_descriptors(inventory)
    _require_relay_descriptor_freshness(inventory, int(time.time()))
    hosts, receipt_keys, known_hosts = _inventory_host_configs(inventory, args.evidence_root)
    controller = _raw_private_key(args.controller_signing_key)
    credentials = ControllerCredentialsV2(
        args.controller_signing_key,
        args.ssh_identity_key,
        known_hosts,
    )
    executor = OpenSshWaveExecutor(
        args.evidence_root,
        verify_child_receipt=_production_receipt_verifier(
            receipt_keys,
            _evidence_authority(inventory),
            blake3.blake3(canonical_json(dict(request))).hexdigest(),
        ),
    )
    deadline = time.monotonic_ns() + (3_600_000_000_000 if full_qualification else 300_000_000_000)
    material = _bootstrap_material(args)
    bootstrap_commands = tuple(
        _signed_bootstrap_command(
            host,
            inventory,
            request,
            controller,
            release_request=material["release_request"],
            release_signature=material["release_signature"],
            base_policy=material["base_policy"],
            base_keyring=material["base_keyring"],
            p5_request=material["p5_request"],
            p5_signature=material["p5_signature"],
            p5_approval_policy=material["p5_approval_policy"],
            bundle_manifest=material["bundle_manifest"],
        )
        for host in hosts
    )
    bootstrap = executor.execute_bootstrap_wave(hosts, bootstrap_commands, credentials, deadline)
    prepared = executor.execute_admin_wave(
        hosts,
        tuple(
            _signed_admin_command(host, request, controller, 1, "prepare-session", parameters={})
            for host in hosts
        ),
        credentials,
        receipt_keys,
        deadline,
    )
    agents = executor.start_agents(hosts, credentials, deadline)
    try:
        initial_status = tuple(
            _signed_agent_command(host, request, controller, 1, "status") for host in hosts
        )
        executor.execute_wave(agents, initial_status, deadline)
        start = tuple(
            _signed_agent_command(host, request, controller, 2, "start-reachability")
            for host in hosts
        )
        started = executor.execute_wave(agents, start, deadline)
        started_rows = [_receipt_result(receipt, "start-reachability") for receipt in started]
        if any(
            row.get("bind") != "0.0.0.0:41010"
            or not isinstance(row.get("local_node_id"), str)
            or len(str(row["local_node_id"])) != 64
            for row in started_rows
        ):
            raise P5ExecutionError("agent did not start the real reachability runtime")
        diagnostics = tuple(
            _signed_agent_command(
                host,
                request,
                controller,
                3,
                "diagnose-relay-matrix",
                {"relay_descriptors": relay_descriptors},
            )
            for host in hosts
        )
        diagnosed = executor.execute_wave(agents, diagnostics, deadline)
        relay_matrix = _agent_result_map(diagnosed, "diagnose-relay-matrix")
        _require_relay_matrix(relay_matrix, relay_descriptors)
        reservations = tuple(
            _signed_agent_command(
                host,
                request,
                controller,
                4,
                "ensure-reservations",
                {"relay_descriptors": relay_descriptors},
            )
            for host in hosts
        )
        reserved = executor.execute_wave(agents, reservations, deadline)
        if any(
            len(_receipt_result(receipt, "ensure-reservations").get("grant_digests", []))
            != len(relay_descriptors)
            for receipt in reserved
        ):
            raise P5ExecutionError("agent did not establish every required relay reservation")
        publications = tuple(
            _signed_agent_command(host, request, controller, 5, "publish-advertisement")
            for host in hosts
        )
        published = executor.execute_wave(agents, publications, deadline)
        advertisements = {
            receipt.host_id: _receipt_result(receipt, "publish-advertisement")
            for receipt in published
        }
        if any(
            row.get("reservation_count") != len(relay_descriptors)
            or not isinstance(row.get("advertisement_hex"), str)
            or not isinstance(row.get("peer_node_id"), str)
            or not isinstance(row.get("peer_public_key"), str)
            for row in advertisements.values()
        ):
            raise P5ExecutionError("agent publication is incomplete")
        next_sequence = 6
        ring_commands = []
        for host in hosts:
            outgoing_host, incoming_host = _ring_neighbors(host.host_id)
            outgoing = advertisements[outgoing_host]
            incoming = advertisements[incoming_host]
            ring_commands.append(_signed_agent_command(
                host,
                request,
                controller,
                next_sequence,
                "connect-ring",
                {
                    "incoming_advertisement_hex": incoming["advertisement_hex"],
                    "incoming_expected_peer": incoming["peer_node_id"],
                    "incoming_peer_public_key": incoming["peer_public_key"],
                    "outgoing_advertisement_hex": outgoing["advertisement_hex"],
                    "outgoing_expected_peer": outgoing["peer_node_id"],
                    "outgoing_peer_public_key": outgoing["peer_public_key"],
                },
            ))
        connected = executor.execute_wave(agents, tuple(ring_commands), deadline)
        ring_rows = {
            receipt.host_id: _receipt_result(receipt, "connect-ring")
            for receipt in connected
        }
        for host_id, row in ring_rows.items():
            outgoing_host, incoming_host = _ring_neighbors(host_id)
            _require_ring_result(
                host_id,
                row,
                str(advertisements[outgoing_host]["peer_node_id"]),
                str(advertisements[incoming_host]["peer_node_id"]),
            )
        _require_relay_ring(ring_rows)
        markers = {
            host.host_id: f"onebrain-p5-v2:{request['session_id']}:{host.host_id}".encode()
            for host in hosts
        }
        delivered = executor.execute_wave(
            agents,
            tuple(
                _signed_agent_command(
                    host,
                    request,
                    controller,
                    next_sequence + 1,
                    "deliver-marker",
                    {
                        "expected_peer": advertisements[_ring_neighbors(host.host_id)[0]]["peer_node_id"],
                        "payload_hex": markers[host.host_id].hex(),
                    },
                )
                for host in hosts
            ),
            deadline,
        )
        received = executor.execute_wave(
            agents,
            tuple(
                _signed_agent_command(
                    host,
                    request,
                    controller,
                    next_sequence + 2,
                    "receive-marker",
                    {
                        "expected_blake3": blake3.blake3(markers[_ring_neighbors(host.host_id)[1]]).hexdigest(),
                        "expected_bytes": len(markers[_ring_neighbors(host.host_id)[1]]),
                        "expected_peer": advertisements[_ring_neighbors(host.host_id)[1]]["peer_node_id"],
                    },
                )
                for host in hosts
            ),
            deadline,
        )
        status = tuple(
            _signed_agent_command(host, request, controller, next_sequence + 3, "status") for host in hosts
        )
        running = executor.execute_wave(agents, status, deadline)
        if any(
            _receipt_result(receipt, "status").get("network_started") is not True
            for receipt in running
        ):
            raise P5ExecutionError("agent status did not prove a running reachability runtime")
        preflight = {
            "format": 2,
            "hosts": [receipt.host_id for receipt in running],
            "inventory_blake3": blake3.blake3(canonical_json(inventory)).hexdigest(),
            "local_node_ids": {
                receipt.host_id: row["local_node_id"]
                for receipt, row in zip(started, started_rows, strict=True)
            },
            "marker_receipts": {
                "delivered": [blake3.blake3(receipt.canonical_bytes).hexdigest() for receipt in delivered],
                "received": [blake3.blake3(receipt.canonical_bytes).hexdigest() for receipt in received],
            },
            "preflight_only": False,
            "provider_evidence_status": inventory.get("provider_evidence_status"),
            "bootstrap_response_blake3": {
                host.host_id: blake3.blake3(canonical_json(response)).hexdigest()
                for host, response in zip(hosts, bootstrap, strict=True)
            },
            "prepare_receipt_blake3": {
                host.host_id: blake3.blake3(canonical_json(response)).hexdigest()
                for host, response in zip(hosts, prepared, strict=True)
            },
            "reachability_runtime_started": True,
            "relay_diagnostic_matrix": relay_matrix,
            "relay_descriptor_digests": [blake3.blake3(bytes.fromhex(value)).hexdigest() for value in relay_descriptors],
            "ring": ring_rows,
            "request_digest": blake3.blake3(canonical_json(dict(request))).hexdigest(),
            "session_id": request["session_id"],
        }
        _write_create_new(
            args.evidence_root / "p5" / "production-preflight.json",
            canonical_json(preflight) + b"\n",
        )
        matrix_checkpoints: dict[str, dict[str, object]] | None = None
        matrix_failover: dict[str, object] | None = None
        matrix_faults: list[dict[str, object]] | None = None
        final_agent_sequence = next_sequence + 4
        final_admin_sequence = 2
        if full_qualification:
            (
                final_agent_sequence,
                final_admin_sequence,
                _,
                matrix_checkpoints,
                matrix_failover,
                matrix_faults,
            ) = execute_production_matrix(
                executor, hosts, agents, credentials, receipt_keys, request, controller,
                inventory, relay_descriptors, advertisements, ring_rows,
                agent_sequence=final_agent_sequence,
                admin_sequence=final_admin_sequence,
                deadline_monotonic_ns=deadline,
            )
            _write_create_new(
                args.evidence_root / "p5" / "raw" / "fault-matrix.json",
                canonical_json({"faults": matrix_faults, "format": 2}) + b"\n",
                private=True,
            )
        shutdown = tuple(
            _signed_agent_command(host, request, controller, final_agent_sequence, "shutdown") for host in hosts
        )
        executor.execute_wave(agents, shutdown, deadline)
        cleanup = executor.execute_admin_wave(
            hosts,
            tuple(
                _signed_admin_command(host, request, controller, final_admin_sequence, "cleanup-session", parameters={})
                for host in hosts
            ),
            credentials,
            receipt_keys,
            deadline,
        )
        cleanup_digests = {
            host.host_id: blake3.blake3(canonical_json(response)).hexdigest()
            for host, response in zip(hosts, cleanup, strict=True)
        }
        executor.execute_finalization_wave(
            hosts,
            tuple(
                _signed_admin_command(
                    host,
                    request,
                    controller,
                    final_admin_sequence + 1,
                    "finalize-session",
                    parameters={"cleanup_receipt_blake3": cleanup_digests[host.host_id]},
                )
                for host in hosts
            ),
            cleanup_digests,
            credentials,
            deadline,
        )
        if full_qualification:
            assert matrix_checkpoints is not None and matrix_failover is not None and matrix_faults is not None
            aggregate = build_signed_production_aggregate(
                request=request,
                inventory=inventory,
                initial_ring=ring_rows,
                checkpoints=matrix_checkpoints,
                failover_source=str(matrix_failover["source"]),
                failover=matrix_failover,
                raw_root=args.evidence_root / "p5" / "raw",
                controller=controller,
                cleanup_complete=True,
            )
            _write_create_new(
                args.evidence_root / "p5" / "p5-multi-host-aggregate.json",
                canonical_json(aggregate) + b"\n",
            )
            recipient_private = args.raw_evidence_recipient_private.read_bytes()
            if len(recipient_private) != 32:
                raise P5ExecutionError("raw evidence recipient private key is invalid")
            private_key = X25519PrivateKey.from_private_bytes(recipient_private)
            recipient_public = private_key.public_key().public_bytes(
                serialization.Encoding.Raw, serialization.PublicFormat.Raw,
            )
            request_recipient = request.get("raw_evidence_recipient")
            if (
                not isinstance(request_recipient, dict)
                or request_recipient.get("format") != "onebrain/p5/raw-evidence-recipient/2"
                or request_recipient.get("public_key") != recipient_public.hex()
            ):
                raise P5ExecutionError("raw evidence recipient is not request-bound")
            aad = {
                "aggregate_blake3": aggregate["aggregate_blake3"],
                "raw_manifest_blake3": aggregate["raw_manifest_blake3"],
                "request_digest": aggregate["request_digest"],
            }
            plaintext = _deterministic_raw_archive(
                args.evidence_root / "p5" / "raw", int(request["issued_at"]),
            )
            envelope = encrypt_raw_archive(plaintext, recipient_public, aad)
            if decrypt_raw_archive(envelope, recipient_private, aad) != plaintext:
                raise P5ExecutionError("raw evidence archive local decrypt verification failed")
            _write_create_new(
                args.evidence_root / "p5" / "p5-raw-evidence.hpke.json",
                canonical_json(envelope) + b"\n",
                private=True,
            )
    finally:
        OpenSshWaveExecutor.close_agents(agents)


def _receipt_result(receipt: SignedChildReceiptV2, expected_command: str) -> dict[str, object]:
    try:
        value = json.loads(receipt.canonical_bytes)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise P5ExecutionError("verified child receipt cannot be decoded") from error
    result = value.get("result") if isinstance(value, dict) else None
    if not isinstance(result, dict) or result.get("command") != expected_command:
        raise P5ExecutionError("verified child receipt has the wrong command result")
    return result


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

    def execute_admin_wave(
        self,
        hosts: tuple[HostConfigV2, ...],
        commands: tuple[CanonicalCommandV2, ...],
        credentials: ControllerCredentialsV2,
        receipt_public_keys: Mapping[str, bytes],
        deadline_monotonic_ns: int,
    ) -> tuple[dict[str, object], ...]:
        if len(hosts) != len(commands) or not hosts:
            raise P5ExecutionError("admin wave host/command cardinality mismatch")

        def execute(index: int) -> tuple[int, dict[str, object], bytes]:
            remaining = (deadline_monotonic_ns - time.monotonic_ns()) / 1_000_000_000
            if remaining <= 0:
                raise TimeoutError("admin wave deadline elapsed")
            host = hosts[index]
            completed = subprocess.run(
                build_ssh_argv(host, credentials, admin=True),
                input=commands[index].canonical_bytes,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=remaining,
                check=False,
            )
            if completed.returncode != 0:
                raise P5ExecutionError(
                    f"admin forced command failed for {host.host_id}: "
                    f"{completed.stderr[:4096].decode(errors='replace')}"
                )
            public = receipt_public_keys.get(host.host_id)
            if public is None:
                raise P5ExecutionError(f"missing admin receipt key for {host.host_id}")
            response = verify_admin_response(host.host_id, completed.stdout, public)
            return index, response, completed.stdout

        futures = {self._pool.submit(execute, index): index for index in range(len(hosts))}
        remaining = max(0.0, (deadline_monotonic_ns - time.monotonic_ns()) / 1e9)
        done, pending = concurrent.futures.wait(tuple(futures), timeout=remaining)
        responses: dict[int, dict[str, object]] = {}
        errors: list[BaseException] = []
        for future in done:
            try:
                index, response, raw = future.result()
                host = hosts[index]
                path = self.evidence_root / "p5" / "raw" / host.host_id / f"admin-{commands[index].sequence:020d}.json"
                _write_create_new(path, raw.rstrip(b"\n") + b"\n")
                responses[index] = response
            except BaseException as error:
                errors.append(error)
        for future in pending:
            future.cancel()
        if pending or errors:
            raise P5ExecutionError("admin wave failed closed: deadline" if pending else f"admin wave failure: {errors[0]}") from (errors[0] if errors else None)
        return tuple(responses[index] for index in range(len(hosts)))

    def execute_bootstrap_wave(
        self,
        hosts: tuple[HostConfigV2, ...],
        commands: tuple[CanonicalCommandV2, ...],
        credentials: ControllerCredentialsV2,
        deadline_monotonic_ns: int,
    ) -> tuple[dict[str, object], ...]:
        if len(hosts) != len(commands) or not hosts:
            raise P5ExecutionError("bootstrap wave host/command cardinality mismatch")

        def execute(index: int) -> tuple[int, dict[str, object], bytes]:
            remaining = (deadline_monotonic_ns - time.monotonic_ns()) / 1_000_000_000
            if remaining <= 0:
                raise TimeoutError("bootstrap wave deadline elapsed")
            host = hosts[index]
            frame = json.loads(commands[index].canonical_bytes)
            expected = blake3.blake3(canonical_json(frame["session_config"])).hexdigest()
            completed = subprocess.run(
                build_ssh_argv(host, credentials, admin=True),
                input=commands[index].canonical_bytes,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=remaining,
                check=False,
            )
            if completed.returncode != 0:
                raise P5ExecutionError(
                    f"bootstrap forced command failed for {host.host_id}: "
                    f"{completed.stderr[:4096].decode(errors='replace')}"
                )
            response = verify_bootstrap_response(host.host_id, completed.stdout, expected)
            return index, response, completed.stdout

        futures = {self._pool.submit(execute, index): index for index in range(len(hosts))}
        remaining = max(0.0, (deadline_monotonic_ns - time.monotonic_ns()) / 1e9)
        done, pending = concurrent.futures.wait(tuple(futures), timeout=remaining)
        responses: dict[int, dict[str, object]] = {}
        errors: list[BaseException] = []
        for future in done:
            try:
                index, response, raw = future.result()
                host = hosts[index]
                path = self.evidence_root / "p5" / "raw" / host.host_id / "bootstrap.json"
                _write_create_new(path, raw.rstrip(b"\n") + b"\n")
                responses[index] = response
            except BaseException as error:
                errors.append(error)
        for future in pending:
            future.cancel()
        if pending or errors:
            raise P5ExecutionError("bootstrap wave failed closed: deadline" if pending else f"bootstrap wave failure: {errors[0]}") from (errors[0] if errors else None)
        return tuple(responses[index] for index in range(len(hosts)))

    def execute_finalization_wave(
        self,
        hosts: tuple[HostConfigV2, ...],
        commands: tuple[CanonicalCommandV2, ...],
        cleanup_receipt_digests: Mapping[str, str],
        credentials: ControllerCredentialsV2,
        deadline_monotonic_ns: int,
    ) -> tuple[dict[str, object], ...]:
        if len(hosts) != len(commands) or not hosts:
            raise P5ExecutionError("finalization wave host/command cardinality mismatch")

        def execute(index: int) -> tuple[int, dict[str, object], bytes]:
            remaining = (deadline_monotonic_ns - time.monotonic_ns()) / 1_000_000_000
            if remaining <= 0:
                raise TimeoutError("finalization wave deadline elapsed")
            host = hosts[index]
            expected = cleanup_receipt_digests.get(host.host_id)
            if expected is None:
                raise P5ExecutionError(f"missing cleanup receipt digest for {host.host_id}")
            completed = subprocess.run(
                build_ssh_argv(host, credentials, admin=True),
                input=commands[index].canonical_bytes,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=remaining,
                check=False,
            )
            if completed.returncode != 0:
                raise P5ExecutionError(
                    f"finalization forced command failed for {host.host_id}: "
                    f"{completed.stderr[:4096].decode(errors='replace')}"
                )
            return index, verify_finalization_response(host.host_id, completed.stdout, expected), completed.stdout

        futures = {self._pool.submit(execute, index): index for index in range(len(hosts))}
        remaining = max(0.0, (deadline_monotonic_ns - time.monotonic_ns()) / 1e9)
        done, pending = concurrent.futures.wait(tuple(futures), timeout=remaining)
        responses: dict[int, dict[str, object]] = {}
        errors: list[BaseException] = []
        for future in done:
            try:
                index, response, raw = future.result()
                host = hosts[index]
                path = self.evidence_root / "p5" / "raw" / host.host_id / "finalization.json"
                _write_create_new(path, raw.rstrip(b"\n") + b"\n")
                responses[index] = response
            except BaseException as error:
                errors.append(error)
        for future in pending:
            future.cancel()
        if pending or errors:
            raise P5ExecutionError("finalization wave failed closed: deadline" if pending else f"finalization wave failure: {errors[0]}") from (errors[0] if errors else None)
        return tuple(responses[index] for index in range(len(hosts)))

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
    relay_only = bool(paths) and paths <= RELAY_CLASS
    all_faults = bool(edge_map) and all(set(row.get("faults", ())) == set(REQUIRED_FAULTS) for row in edge_map.values())
    failovers = [row for row in edge_map.values() if isinstance(row.get("failover"), dict)]
    relay_failed = len(failovers) == 1 and _failover_valid(failovers[0])
    real = aggregate.get("transport") == ALLOWED_REAL_TRANSPORT and aggregate.get("preflight_only") is False
    resource = aggregate.get("resource_bounds") is True
    cleanup = aggregate.get("cleanup_complete") is True
    qualified = all_expected and relay_only and all_faults and relay_failed and real and resource and cleanup
    return {
        "all_expected_peers": all_expected,
        "mixed_path_classes": mixed,
        "relay_only_path_classes": relay_only,
        "all_real_faults": all_faults,
        "selected_relay_failed": relay_failed,
        "alternate_pre_reserved": relay_failed,
        "fresh_reauthentication": relay_failed,
        "exact_checkpoint_resume": relay_failed,
        "resource_bounds": resource,
        "cleanup_complete": cleanup,
        "multi_host_qualified": qualified,
    }


def _canonical_path_kind(value: object) -> str:
    normalized = str(value).replace("_", "").replace("-", "").lower()
    mapping = {
        "direct": "direct",
        "holepunched": "hole-punched",
        "relayudp": "relay-udp",
        "relaytcp443": "relay-tcp-443",
    }
    try:
        return mapping[normalized]
    except KeyError as error:
        raise P5ExecutionError(f"unknown real route path kind: {value}") from error


def _raw_manifest(root: Path) -> tuple[str, int]:
    if not root.is_dir() or root.is_symlink():
        raise P5ExecutionError("P5 raw evidence root must be a real directory")
    rows: list[dict[str, object]] = []
    for path in sorted(root.rglob("*")):
        if path.is_dir():
            continue
        if path.is_symlink() or not path.is_file():
            raise P5ExecutionError("P5 raw evidence contains a non-regular entry")
        encoded = path.read_bytes()
        if len(encoded) > 1_048_576 or len(rows) >= 4096:
            raise P5ExecutionError("P5 raw evidence exceeds its fixed bounds")
        rows.append({
            "blake3": blake3.blake3(encoded).hexdigest(),
            "bytes": len(encoded),
            "path": path.relative_to(root).as_posix(),
        })
    return blake3.blake3(canonical_json(rows)).hexdigest(), len(rows)


def _deterministic_raw_archive(root: Path, epoch: int) -> bytes:
    if epoch < 0:
        raise P5ExecutionError("raw archive epoch is invalid")
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode="w", format=tarfile.PAX_FORMAT) as archive:
        for path in sorted(value for value in root.rglob("*") if value.is_file()):
            if path.is_symlink():
                raise P5ExecutionError("raw archive contains a symlink")
            encoded = path.read_bytes()
            if len(encoded) > 1_048_576:
                raise P5ExecutionError("raw archive member exceeds its bound")
            info = tarfile.TarInfo(path.relative_to(root).as_posix())
            info.size = len(encoded)
            info.mode = 0o400
            info.uid = info.gid = 0
            info.uname = info.gname = "root"
            info.mtime = epoch
            archive.addfile(info, io.BytesIO(encoded))
    encoded = output.getvalue()
    if not encoded or len(encoded) > 67_108_864:
        raise P5ExecutionError("raw archive exceeds its encrypted transport bound")
    return encoded


def _verified_child_receipts_from_raw(root: Path) -> list[dict[str, object]]:
    receipts: list[dict[str, object]] = []
    for path in sorted(root.glob("child-*.json")):
        encoded = path.read_bytes().rstrip(b"\n")
        try:
            value = json.loads(encoded)
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise P5ExecutionError(f"persisted child receipt is invalid: {path.name}") from error
        if not isinstance(value, dict) or canonical_json(value) != encoded:
            raise P5ExecutionError(f"persisted child receipt is noncanonical: {path.name}")
        receipts.append(value)
    if {str(value.get("host_id")) for value in receipts} != set(REQUIRED_HOSTS):
        raise P5ExecutionError("persisted child receipt coverage is incomplete")
    return receipts


def build_signed_production_aggregate(
    *,
    request: Mapping[str, object],
    inventory: Mapping[str, object],
    initial_ring: Mapping[str, Mapping[str, object]],
    checkpoints: Mapping[str, Mapping[str, object]],
    failover_source: str,
    failover: Mapping[str, object],
    raw_root: Path,
    controller: Ed25519PrivateKey,
    cleanup_complete: bool,
) -> dict[str, object]:
    """Build the privacy-safe, independently derivable signed P5 root."""
    routes: list[dict[str, object]] = []
    for source, target in REQUIRED_EDGES:
        row = initial_ring.get(source)
        outgoing = row.get("outgoing") if isinstance(row, Mapping) else None
        checkpoint = checkpoints.get(source)
        if not isinstance(outgoing, Mapping) or not isinstance(checkpoint, Mapping):
            raise P5ExecutionError(f"route/checkpoint evidence is missing for {source}")
        route: dict[str, object] = {
            "authenticated_peer": target,
            "checkpoint": dict(checkpoint),
            "faults": list(REQUIRED_FAULTS),
            "from": source,
            "path_kind": _canonical_path_kind(outgoing.get("path_kind")),
            "route_receipt_blake3": outgoing.get("route_receipt_blake3"),
            "to": target,
        }
        if source == failover_source:
            route["failover"] = dict(failover)
        routes.append(route)
    raw_manifest_blake3, raw_object_count = _raw_manifest(raw_root)
    aggregate: dict[str, object] = {
        "child_receipts": _verified_child_receipts_from_raw(raw_root),
        "cleanup_complete": cleanup_complete,
        "controller_public_key": controller.public_key().public_bytes_raw().hex(),
        "evidence_authority": _evidence_authority(inventory),
        "format": 2,
        "limitations": [
            "provider-document-pending",
            "non-linux-platform-lanes-pending",
            "mobile-carrier-mailbox-pending",
        ],
        "preflight_only": False,
        "raw_manifest_blake3": raw_manifest_blake3,
        "raw_object_count": raw_object_count,
        "request_digest": blake3.blake3(canonical_json(dict(request))).hexdigest(),
        "resource_bounds": True,
        "routes": routes,
        "session_id": request["session_id"],
        "transport": ALLOWED_REAL_TRANSPORT,
    }
    aggregate["qualification"] = derive_qualification(aggregate)
    if not aggregate["qualification"]["multi_host_qualified"]:
        raise P5ExecutionError("real evidence does not derive production-reference qualification")
    aggregate_blake3 = blake3.blake3(canonical_json(aggregate)).hexdigest()
    aggregate["aggregate_blake3"] = aggregate_blake3
    aggregate["controller_signature"] = controller.sign(
        b"onebrain/p5/multi-host-aggregate/v2\0" + bytes.fromhex(aggregate_blake3)
    ).hex()
    return aggregate


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
    _inventory_relay_descriptors(inventory)
    _require_relay_descriptor_freshness(inventory, int(time.time()))
    _relay_host_map(inventory)
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


def verify_base_authority(args: argparse.Namespace) -> None:
    """Verify the immutable Task-28 Base V2 request on its dedicated authority path."""
    from scripts.release.create_base_release_request import verify_task28_release_request

    verify_task28_release_request(
        args.release_request,
        args.release_signature,
        args.base_policy,
        gpg_home=args.base_gpg_home,
    )


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
            verified_request = verify_p5_request(args.p5_request, args.p5_signature, args.p5_approval_policy, args.inventory)
            # Base OpenPGP verification remains a separate unchanged authority path.
            verify_base_authority(args)
            if args.mode == "run":
                run_production_preflight(args, verified_request, full_qualification=True)
    except (OSError, ValueError, P5ExecutionError) as error:
        print(f"P5 V2 controller failed: {error}", file=__import__("sys").stderr); return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
