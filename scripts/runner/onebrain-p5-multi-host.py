#!/usr/bin/env python3
"""Signed three-host P5 orchestrator.

Production mode authenticates the Base release request, pins the exact agent
bytes and SSH host keys, and accepts only role-bound Ed25519 child receipts.
The explicit test helpers always emit non-production evidence.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import copy
import hashlib
import json
import os
import stat
import subprocess
import sys
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Protocol

import blake3
from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives.asymmetric.ed25519 import (
    Ed25519PrivateKey,
    Ed25519PublicKey,
)


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
if str(REPOSITORY_ROOT) not in sys.path:
    sys.path.insert(0, str(REPOSITORY_ROOT))
PROFILE_PATH = (
    REPOSITORY_ROOT
    / "src"
    / "test-vectors"
    / "vnext"
    / "p5-multi-host-production-qualification-v1.json"
)
CHILD_FIELDS = {
    "role",
    "physical_host_id",
    "release_request_digest",
    "qualification_session_id",
    "candidate_commit",
    "candidate_tree",
    "candidate_semantic_digest",
    "linux_artifact_tuple_digest",
    "toolchain_digest",
    "runner_bundle_manifest_digest",
    "agent_binary_digest",
    "agent_signature_digest",
    "registry_root",
    "profile_digest",
    "trust_policy_digest",
    "runner_identity",
    "ssh_host_key_fingerprint",
    "physical_machine_fingerprint",
    "host_evidence_sha256",
    "placement_evidence_sha256",
    "command_sequence",
    "command",
    "fault_id",
    "before_roots",
    "after_roots",
    "resource_observation",
    "result",
    "limitations",
}
BINDING_FIELDS = {
    "release_request_digest",
    "qualification_session_id",
    "candidate_commit",
    "candidate_tree",
    "candidate_semantic_digest",
    "linux_artifact_tuple_digest",
    "toolchain_digest",
    "runner_bundle_manifest_digest",
    "agent_binary_digest",
    "agent_signature_digest",
    "registry_root",
    "profile_digest",
    "trust_policy_digest",
}
INVENTORY_UNSIGNED_FIELDS = {
    "format",
    "evidence_tier",
    "binding",
    "limitations",
    "registry_candidate",
    "hosts",
}
INVENTORY_FIELDS = INVENTORY_UNSIGNED_FIELDS | {
    "signer_public_key",
    "signer_fingerprint",
    "signature",
}
ROOT_FIELDS = {"canonical_root", "journal_root", "outbox_root", "operational_root"}
RESOURCE_FIELDS = {
    "peak_rss_bytes": "max_peak_rss_bytes_per_host",
    "durable_growth_bytes": "max_durable_growth_bytes_per_host",
    "task_count": "max_task_count_per_host",
    "active_sessions": "max_active_sessions_per_host",
    "fault_duration_ms": "max_fault_duration_ms",
    "reunion_ms": "max_reunion_ms",
    "quiescence_ms": "max_quiescence_ms",
}
REGISTRY_CANDIDATE_FILES = (
    "concepts.obr",
    "concepts.obr.ccids.idx",
    "concepts.obr.labels.idx",
    "concepts.obr.manifest.json",
    "concepts.obr.verification.json",
)
REQUIRED_PRODUCTION_LIMITATIONS = [
    "aggregate-qualification-is-orchestrator-owned",
    "base-gate-v1-not-claimed",
    "receipt-is-evidence-not-authority",
    "real-quic-ring-and-fault-injection-pending",
    "registry-candidate-bytes-bound-without-full-profile-qualification",
    "registry-production-qualification-not-claimed",
    "registry-production-resource-profiles-pending",
]
AGENT_SIGNATURE_DOMAIN = b"onebrain:p5:release-agent-signature:1\0"
INVENTORY_SIGNATURE_DOMAIN = b"onebrain:p5:multi-host-inventory:1\0"
REGISTRY_ROOT_DOMAIN = b"onebrain:p5:registry-candidate-binding:1\0"
PHYSICAL_MACHINE_FINGERPRINT_CONTEXT = "onebrain:p5:physical-machine-fingerprint:1"
HOST_SPEC_FIELDS = {
    "physical_host_id",
    "runner_identity",
    "ssh_host_key_algorithm",
    "ssh_host_key_fingerprint",
    "observed_ssh_host_key_fingerprint",
    "receipt_role",
    "receipt_signer_fingerprint",
    "durable_root_locator",
    "expected_principal",
    "ssh_destination",
    "ssh_port",
    "known_hosts_file",
    "agent_command",
    "host_evidence_path",
    "placement_evidence_path",
    "remote_agent_signature_path",
}


class P5OrchestrationError(RuntimeError):
    """A fail-closed P5 orchestration or evidence validation error."""


def _regular_file_bytes(path: Path, label: str) -> bytes:
    _regular_file_metadata(path, label)
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(path, flags)
    try:
        before = os.fstat(descriptor)
        if not stat.S_ISREG(before.st_mode):
            raise P5OrchestrationError(f"{label} must be a regular non-symlink file")
        with os.fdopen(descriptor, "rb", closefd=False) as stream:
            bytes_value = stream.read()
        after = os.fstat(descriptor)
    finally:
        os.close(descriptor)
    if _file_identity(before) != _file_identity(after):
        raise P5OrchestrationError(f"{label} changed during read")
    return bytes_value


def _regular_file_metadata(path: Path, label: str) -> os.stat_result:
    try:
        metadata = path.lstat()
    except OSError as error:
        raise P5OrchestrationError(f"{label} is unavailable") from error
    if not stat.S_ISREG(metadata.st_mode) or path.is_symlink():
        raise P5OrchestrationError(f"{label} must be a regular non-symlink file")
    return metadata


def _measure_regular_file(path: Path, label: str) -> dict[str, object]:
    _regular_file_metadata(path, label)
    digest = blake3.blake3()
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(path, flags)
    try:
        before = os.fstat(descriptor)
        if not stat.S_ISREG(before.st_mode):
            raise P5OrchestrationError(f"{label} must be a regular non-symlink file")
        with os.fdopen(descriptor, "rb", closefd=False) as stream:
            for chunk in iter(lambda: stream.read(8 * 1024 * 1024), b""):
                digest.update(chunk)
        after = os.fstat(descriptor)
    finally:
        os.close(descriptor)
    if _file_identity(before) != _file_identity(after):
        raise P5OrchestrationError(f"{label} changed during measurement")
    return {"path": path.name, "size": before.st_size, "blake3": digest.hexdigest()}


def _file_identity(value: os.stat_result) -> tuple[int, int, int, int, int, int]:
    return (
        value.st_dev,
        value.st_ino,
        value.st_mode,
        value.st_size,
        value.st_mtime_ns,
        value.st_ctime_ns,
    )


def _measured_json(
    path: Path, label: str, measurement: dict[str, object]
) -> dict[str, object]:
    bytes_value = _regular_file_bytes(path, label)
    if (
        len(bytes_value) != measurement["size"]
        or blake3.blake3(bytes_value).hexdigest() != measurement["blake3"]
    ):
        raise P5OrchestrationError(f"{label} changed after measurement")
    return _json_object(bytes_value, label)


def agent_signature_message(agent_bytes: bytes, bundle_manifest_digest: str) -> bytes:
    manifest_digest = bytes.fromhex(
        _hex(bundle_manifest_digest, 32, "runner bundle manifest digest")
    )
    return AGENT_SIGNATURE_DOMAIN + blake3.blake3(agent_bytes).digest() + manifest_digest


def _verify_agent_signature(
    agent_bytes: bytes,
    bundle_manifest_digest: str,
    signature_path: Path,
    profile: dict[str, object],
) -> str:
    signature = _regular_file_bytes(signature_path, "P5 release agent signature")
    if len(signature) != 64:
        raise P5OrchestrationError("P5 release agent signature must be 64 raw bytes")
    role = _role_bindings(profile)["p5-orchestrator"]
    try:
        Ed25519PublicKey.from_public_bytes(bytes.fromhex(role["public_key_hex"])).verify(
            signature, agent_signature_message(agent_bytes, bundle_manifest_digest)
        )
    except (ValueError, InvalidSignature) as error:
        raise P5OrchestrationError("P5 release agent signature is invalid") from error
    return blake3.blake3(signature).hexdigest()


def inventory_signature_message(unsigned: object) -> bytes:
    return INVENTORY_SIGNATURE_DOMAIN + blake3.blake3(_canonical_json(unsigned)).digest()


def _json_object(bytes_value: bytes, label: str) -> dict[str, object]:
    try:
        value = json.loads(bytes_value)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise P5OrchestrationError(f"{label} is invalid JSON") from error
    if not isinstance(value, dict):
        raise P5OrchestrationError(f"{label} must be a JSON object")
    return value


def _nonnegative_int(value: object, label: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise P5OrchestrationError(f"{label} must be a nonnegative integer")
    return value


def _registry_candidate_binding(root: Path) -> dict[str, object]:
    try:
        candidate_root = root.resolve(strict=True)
    except OSError as error:
        raise P5OrchestrationError("Registry candidate root is unavailable") from error
    if not candidate_root.is_dir() or root.is_symlink():
        raise P5OrchestrationError("Registry candidate root must be a real directory")

    measured = {
        name: _measure_regular_file(candidate_root / name, f"Registry candidate {name}")
        for name in REGISTRY_CANDIDATE_FILES
    }
    manifest = _measured_json(
        candidate_root / "concepts.obr.manifest.json",
        "Registry candidate manifest",
        measured["concepts.obr.manifest.json"],
    )
    verification = _measured_json(
        candidate_root / "concepts.obr.verification.json",
        "Registry candidate verification",
        measured["concepts.obr.verification.json"],
    )
    if manifest.get("manifest_version") != 1:
        raise P5OrchestrationError("Registry candidate manifest version is unsupported")

    if (
        manifest.get("obr_blake3") != measured["concepts.obr"]["blake3"]
        or verification.get("obr_blake3") != measured["concepts.obr"]["blake3"]
        or _nonnegative_int(verification.get("file_size"), "Registry OBR size")
        != measured["concepts.obr"]["size"]
    ):
        raise P5OrchestrationError("Registry OBR bytes differ from manifest/verification")
    for name, field, label in (
        ("concepts.obr.labels.idx", "label_index", "label index"),
        ("concepts.obr.ccids.idx", "ccid_index", "CCID index"),
    ):
        manifest_row = manifest.get(field)
        verification_row = verification.get(field)
        if not isinstance(manifest_row, dict) or not isinstance(verification_row, dict):
            raise P5OrchestrationError(f"Registry {label} evidence is missing")
        if (
            manifest_row.get("blake3") != measured[name]["blake3"]
            or verification_row.get("blake3") != measured[name]["blake3"]
            or _nonnegative_int(manifest_row.get("file_size"), f"Registry {label} size")
            != measured[name]["size"]
            or _nonnegative_int(
                verification_row.get("file_size"), f"Registry verification {label} size"
            )
            != measured[name]["size"]
        ):
            raise P5OrchestrationError(f"Registry {label} bytes differ from evidence")

    digest = blake3.blake3()
    digest.update(REGISTRY_ROOT_DOMAIN)
    files = []
    for name in sorted(measured):
        row = measured[name]
        digest.update(name.encode("utf-8"))
        digest.update(b"\0")
        digest.update(int(row["size"]).to_bytes(8, "big"))
        digest.update(bytes.fromhex(str(row["blake3"])))
        files.append(row)
    return {
        "format": "onebrain/p5-registry-candidate-binding/1",
        "root": digest.hexdigest(),
        "registry_production_qualified": False,
        "files": files,
    }


class SshExecutor(Protocol):
    def run(
        self,
        host: dict[str, object],
        commands: list[dict[str, object]],
        timeout_seconds: float,
    ) -> list[dict[str, object]]: ...


def _canonical_json(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode()


def _domain(value: str) -> bytes:
    return value.replace("\\0", "\0").encode("ascii")


def _hex(value: object, byte_length: int, field: str) -> str:
    if not isinstance(value, str) or len(value) != byte_length * 2:
        raise P5OrchestrationError(f"{field} must be {byte_length}-byte lowercase hex")
    try:
        decoded = bytes.fromhex(value)
    except ValueError as error:
        raise P5OrchestrationError(f"{field} must be hexadecimal") from error
    if decoded.hex() != value:
        raise P5OrchestrationError(f"{field} must be lowercase canonical hex")
    return value


def _profile() -> dict[str, object]:
    value = json.loads(PROFILE_PATH.read_text(encoding="utf-8"))
    if value.get("format") != "onebrain/p5-multi-host-production-qualification/1":
        raise P5OrchestrationError("the frozen P5 profile has an invalid format")
    return value


def _fingerprint(public_key: bytes, profile: dict[str, object]) -> str:
    trust = profile["trust_policy"]
    return blake3.blake3(
        public_key, derive_key_context=str(trust["fingerprint_context"])
    ).hexdigest()


def _policy_digest(policy: dict[str, object], profile: dict[str, object]) -> str:
    trust = profile["trust_policy"]
    return blake3.blake3(
        _canonical_json(policy), derive_key_context=str(trust["digest_context"])
    ).hexdigest()


def child_receipt_signature_message(evidence_tier: str, payload: object) -> bytes:
    profile = _profile()
    return _domain(str(profile["child_receipt"]["signature_domain"])) + blake3.blake3(
        _canonical_json(
            {
                "format": profile["child_receipt"]["format"],
                "evidence_tier": evidence_tier,
                "payload": payload,
            }
        )
    ).digest()


def profile_for_test_nonproduction(
    host_keys: dict[str, Ed25519PrivateKey], orchestrator_key: Ed25519PrivateKey
) -> dict[str, object]:
    """Return an ephemeral profile that cannot carry production identity."""
    profile = _profile()
    roles: list[dict[str, str]] = []
    for host in ("host-a", "host-b", "host-c"):
        public = host_keys[host].public_key().public_bytes_raw()
        roles.append(
            {
                "role": f"p5-host:{host}",
                "public_key_hex": public.hex(),
                "fingerprint_hex": _fingerprint(public, profile),
            }
        )
    public = orchestrator_key.public_key().public_bytes_raw()
    roles.append(
        {
            "role": "p5-orchestrator",
            "public_key_hex": public.hex(),
            "fingerprint_hex": _fingerprint(public, profile),
        }
    )
    profile["trust_policy"]["policy"]["role_bindings"] = roles
    profile["trust_policy"]["digest_hex"] = _policy_digest(
        profile["trust_policy"]["policy"], profile
    )
    profile["qualification_state"]["multi_host_qualified"] = False
    profile["qualification_state"]["measured_evidence_committed"] = False
    profile["_test_only_nonproduction"] = True
    profile["_test_orchestrator_private_key"] = orchestrator_key
    return profile


def inventory_for_test_nonproduction(
    profile: dict[str, object], binding: dict[str, str]
) -> dict[str, object]:
    roles = {
        row["role"]: row for row in profile["trust_policy"]["policy"]["role_bindings"]
    }
    hosts: list[dict[str, str]] = []
    for index, host in enumerate(("host-a", "host-b", "host-c"), start=1):
        ssh_fingerprint = blake3.blake3(f"ssh:{host}".encode()).hexdigest()
        hosts.append(
            {
                "physical_host_id": host,
                "runner_identity": f"test-runner-{host}",
                "ssh_host_key_algorithm": "ssh-ed25519",
                "ssh_host_key_fingerprint": ssh_fingerprint,
                "observed_ssh_host_key_fingerprint": ssh_fingerprint,
                "receipt_role": f"p5-host:{host}",
                "receipt_signer_fingerprint": roles[f"p5-host:{host}"][
                    "fingerprint_hex"
                ],
                "durable_root_locator": f"test-only://root-{index}",
                "expected_principal": blake3.blake3(f"principal:{host}".encode()).hexdigest(),
                "ssh_port": 22,
                "physical_machine_fingerprint": blake3.blake3(
                    f"machine:{host}".encode()
                ).hexdigest(),
                "host_evidence_sha256": blake3.blake3(
                    f"host-evidence:{host}".encode()
                ).hexdigest(),
                "placement_evidence_sha256": blake3.blake3(
                    f"placement-evidence:{host}".encode()
                ).hexdigest(),
            }
        )
    unsigned = {
        "format": "onebrain/p5-multi-host-inventory/1",
        "evidence_tier": "nonproduction-test",
        "binding": copy.deepcopy(binding),
        "limitations": list(REQUIRED_PRODUCTION_LIMITATIONS),
        "registry_candidate": {
            "format": "onebrain/p5-registry-candidate-binding/1",
            "root": binding["registry_root"],
            "registry_production_qualified": False,
            "files": [],
        },
        "hosts": hosts,
    }
    signing_key = profile["_test_orchestrator_private_key"]
    public = signing_key.public_key().public_bytes_raw()
    return {
        **unsigned,
        "signer_public_key": public.hex(),
        "signer_fingerprint": _fingerprint(public, profile),
        "signature": signing_key.sign(inventory_signature_message(unsigned)).hex(),
    }


def _roots(host_id: str, fault_id: str, suffix: str) -> dict[str, str]:
    return {
        field: blake3.blake3(f"{host_id}:{fault_id}:{suffix}:{field}".encode()).hexdigest()
        for field in sorted(ROOT_FIELDS)
    }


def sign_child_receipt_for_test_nonproduction(
    *,
    profile: dict[str, object],
    binding: dict[str, str],
    host_id: str,
    sequence: int,
    fault_id: str,
    signing_key: Ed25519PrivateKey,
) -> dict[str, object]:
    inventory = inventory_for_test_nonproduction(profile, binding)
    host = next(row for row in inventory["hosts"] if row["physical_host_id"] == host_id)
    payload: dict[str, object] = {
        "role": f"p5-host:{host_id}",
        "physical_host_id": host_id,
        **binding,
        "runner_identity": host["runner_identity"],
        "ssh_host_key_fingerprint": host["ssh_host_key_fingerprint"],
        "physical_machine_fingerprint": host["physical_machine_fingerprint"],
        "host_evidence_sha256": host["host_evidence_sha256"],
        "placement_evidence_sha256": host["placement_evidence_sha256"],
        "command_sequence": sequence,
        "command": "observe-host-fault",
        "fault_id": fault_id,
        "before_roots": _roots(host_id, fault_id, "before"),
        "after_roots": _roots(host_id, fault_id, "after"),
        "resource_observation": {
            "peak_rss_bytes": 64 * 1024 * 1024,
            "durable_growth_bytes": 1024,
            "task_count": 8,
            "active_sessions": 0,
            "fault_duration_ms": 10,
            "reunion_ms": 10,
            "quiescence_ms": 10,
        },
        "result": "pass",
        "limitations": list(REQUIRED_PRODUCTION_LIMITATIONS),
    }
    public = signing_key.public_key().public_bytes_raw()
    return {
        "format": profile["child_receipt"]["format"],
        "evidence_tier": "nonproduction-test",
        "payload": payload,
        "signer_public_key": public.hex(),
        "signer_fingerprint": _fingerprint(public, profile),
        "signature": signing_key.sign(
            child_receipt_signature_message("nonproduction-test", payload)
        ).hex(),
    }


def _role_bindings(profile: dict[str, object]) -> dict[str, dict[str, str]]:
    return {
        row["role"]: row for row in profile["trust_policy"]["policy"]["role_bindings"]
    }


def _validate_profile(profile: dict[str, object], *, production: bool) -> None:
    frozen = _profile()
    if production:
        if profile != frozen:
            raise P5OrchestrationError("production requires the byte-frozen P5 profile")
    elif not profile.get("_test_only_nonproduction"):
        raise P5OrchestrationError("nonproduction helper requires an explicit test profile")
    trust = profile["trust_policy"]
    if _policy_digest(trust["policy"], profile) != trust["digest_hex"]:
        raise P5OrchestrationError("P5 trust-policy digest mismatch")
    seen: set[str] = set()
    for role in trust["policy"]["role_bindings"]:
        public = bytes.fromhex(role["public_key_hex"])
        expected = _fingerprint(public, profile)
        if expected != role["fingerprint_hex"]:
            raise P5OrchestrationError("P5 signer fingerprint mismatch")
        if role["fingerprint_hex"] in seen:
            raise P5OrchestrationError("duplicate trust-policy signer")
        seen.add(role["fingerprint_hex"])


def _validate_inventory(
    profile: dict[str, object],
    inventory: dict[str, object],
    binding: dict[str, str],
    expected_tier: str,
) -> dict[str, dict[str, object]]:
    if set(inventory) != INVENTORY_FIELDS:
        raise P5OrchestrationError("inventory has unknown or missing fields")
    if inventory.get("format") != "onebrain/p5-multi-host-inventory/1":
        raise P5OrchestrationError("inventory format mismatch")
    if inventory.get("evidence_tier") != expected_tier:
        raise P5OrchestrationError("inventory evidence_tier mismatch")
    if inventory.get("binding") != binding:
        raise P5OrchestrationError("inventory candidate binding mismatch")
    if inventory.get("limitations") != REQUIRED_PRODUCTION_LIMITATIONS:
        raise P5OrchestrationError("inventory limitations are not the frozen P5 limitations")
    registry_candidate = inventory.get("registry_candidate")
    if (
        not isinstance(registry_candidate, dict)
        or registry_candidate.get("format")
        != "onebrain/p5-registry-candidate-binding/1"
        or registry_candidate.get("root") != binding["registry_root"]
        or registry_candidate.get("registry_production_qualified") is not False
    ):
        raise P5OrchestrationError("inventory Registry candidate binding mismatch")
    unsigned = {field: inventory[field] for field in INVENTORY_UNSIGNED_FIELDS}
    role = _role_bindings(profile)["p5-orchestrator"]
    if (
        inventory.get("signer_public_key") != role["public_key_hex"]
        or inventory.get("signer_fingerprint") != role["fingerprint_hex"]
    ):
        raise P5OrchestrationError("inventory signer is not the frozen orchestrator")
    try:
        Ed25519PublicKey.from_public_bytes(bytes.fromhex(role["public_key_hex"])).verify(
            bytes.fromhex(str(inventory["signature"])),
            inventory_signature_message(unsigned),
        )
    except (ValueError, InvalidSignature) as error:
        raise P5OrchestrationError("inventory signature is invalid") from error
    hosts = inventory.get("hosts")
    if not isinstance(hosts, list) or len(hosts) != 3:
        raise P5OrchestrationError("inventory must contain exactly three physical hosts")
    expected = {row["physical_host_id"]: row for row in profile["topology"]["hosts"]}
    required = set(profile["inventory"]["required_host_fields"])
    unique_fields = (
        "physical_host_id",
        "runner_identity",
        "durable_root_locator",
        "expected_principal",
        "receipt_signer_fingerprint",
        "physical_machine_fingerprint",
        "host_evidence_sha256",
        "placement_evidence_sha256",
    )
    for field in unique_fields:
        values = [row.get(field) for row in hosts]
        if len(set(values)) != len(values):
            raise P5OrchestrationError(f"duplicate inventory {field}")
    by_id: dict[str, dict[str, object]] = {}
    roles = _role_bindings(profile)
    for row in hosts:
        missing = required - set(row)
        if missing:
            raise P5OrchestrationError(f"inventory host is missing {sorted(missing)}")
        host_id = str(row["physical_host_id"])
        if host_id not in expected or row["receipt_role"] != expected[host_id]["receipt_role"]:
            raise P5OrchestrationError("inventory host role mismatch")
        if row.get("observed_ssh_host_key_fingerprint") != row["ssh_host_key_fingerprint"]:
            raise P5OrchestrationError("SSH host key does not match its pinned fingerprint")
        ssh_port = row.get("ssh_port")
        if (
            isinstance(ssh_port, bool)
            or not isinstance(ssh_port, int)
            or not 1 <= ssh_port <= 65535
        ):
            raise P5OrchestrationError("inventory SSH port is invalid")
        for field in (
            "physical_machine_fingerprint",
            "host_evidence_sha256",
            "placement_evidence_sha256",
        ):
            _hex(row.get(field), 32, f"inventory {field}")
        role = str(row["receipt_role"])
        if row["receipt_signer_fingerprint"] != roles[role]["fingerprint_hex"]:
            raise P5OrchestrationError("inventory receipt signer is not role-bound")
        by_id[host_id] = row
    if set(by_id) != set(expected):
        raise P5OrchestrationError("inventory physical-host set mismatch")
    return by_id


def _validate_receipt(
    receipt: dict[str, object],
    *,
    profile: dict[str, object],
    binding: dict[str, str],
    host: dict[str, object],
    expected_tier: str,
) -> dict[str, object]:
    if set(receipt) != {
        "format",
        "evidence_tier",
        "payload",
        "signer_public_key",
        "signer_fingerprint",
        "signature",
    }:
        raise P5OrchestrationError("child receipt has unknown or missing fields")
    payload = receipt.get("payload")
    if receipt["format"] != profile["child_receipt"]["format"]:
        raise P5OrchestrationError("child receipt format mismatch")
    if receipt["evidence_tier"] != expected_tier:
        raise P5OrchestrationError("child receipt evidence_tier mismatch")
    if not isinstance(payload, dict) or set(payload) != CHILD_FIELDS:
        raise P5OrchestrationError("child receipt payload has unknown or missing fields")
    for field in BINDING_FIELDS:
        if payload.get(field) != binding.get(field):
            raise P5OrchestrationError(f"child receipt {field} mismatch")
    if payload["physical_host_id"] != host["physical_host_id"]:
        raise P5OrchestrationError("child receipt physical_host_id mismatch")
    if payload["role"] != host["receipt_role"]:
        raise P5OrchestrationError("child receipt role mismatch")
    if payload["runner_identity"] != host["runner_identity"]:
        raise P5OrchestrationError("child receipt runner_identity mismatch")
    if payload["ssh_host_key_fingerprint"] != host["ssh_host_key_fingerprint"]:
        raise P5OrchestrationError("child receipt SSH host key mismatch")
    for field in (
        "physical_machine_fingerprint",
        "host_evidence_sha256",
        "placement_evidence_sha256",
    ):
        if payload[field] != host[field]:
            raise P5OrchestrationError(f"child receipt {field} mismatch")
    if payload["fault_id"] not in profile["fault_matrix"]:
        raise P5OrchestrationError("child receipt fault_id is outside the matrix")
    expected_sequence = profile["fault_matrix"].index(payload["fault_id"]) + 1
    if (
        payload["command_sequence"] != expected_sequence
        or payload["command"] != "observe-host-fault"
    ):
        raise P5OrchestrationError("child receipt command/sequence mismatch")
    for root_set in (payload["before_roots"], payload["after_roots"]):
        if not isinstance(root_set, dict) or set(root_set) != ROOT_FIELDS:
            raise P5OrchestrationError("child receipt root set is incomplete")
        for field, value in root_set.items():
            _hex(value, 32, field)
    observation = payload["resource_observation"]
    if not isinstance(observation, dict) or set(observation) != set(RESOURCE_FIELDS):
        raise P5OrchestrationError("child receipt resource observation is incomplete")
    bounds = profile["resource_bounds"]
    for observed, maximum in RESOURCE_FIELDS.items():
        value = observation[observed]
        if isinstance(value, bool) or not isinstance(value, int) or value < 0:
            raise P5OrchestrationError(f"resource bound value {observed} is invalid")
        if value > bounds[maximum]:
            raise P5OrchestrationError(f"resource bound exceeded: {observed}")
    role = _role_bindings(profile)[str(payload["role"])]
    if receipt["signer_public_key"] != role["public_key_hex"]:
        raise P5OrchestrationError("child receipt signer is not allowlisted for its role")
    if receipt["signer_fingerprint"] != role["fingerprint_hex"]:
        raise P5OrchestrationError("child receipt signer fingerprint is not allowlisted")
    if receipt["signer_fingerprint"] != host["receipt_signer_fingerprint"]:
        raise P5OrchestrationError("child receipt signer differs from inventory")
    try:
        Ed25519PublicKey.from_public_bytes(bytes.fromhex(str(receipt["signer_public_key"]))).verify(
            bytes.fromhex(str(receipt["signature"])),
            child_receipt_signature_message(str(receipt["evidence_tier"]), payload),
        )
    except (ValueError, InvalidSignature) as error:
        raise P5OrchestrationError("child receipt signature is invalid") from error
    if payload["result"] != "pass":
        raise P5OrchestrationError("child receipt does not report a passing result")
    if payload["limitations"] != REQUIRED_PRODUCTION_LIMITATIONS:
        raise P5OrchestrationError("child receipt limitations are not the frozen P5 limitations")
    return payload


def _control_commands(
    profile: dict[str, object],
    binding: dict[str, str],
    host_id: str,
    signing_key: Ed25519PrivateKey,
) -> list[dict[str, object]]:
    issued = int(time.time() * 1000)
    public = signing_key.public_key().public_bytes_raw()
    commands = []
    for index, fault in enumerate(profile["fault_matrix"], start=1):
        payload = {
            "format": "onebrain/p5-multi-host-control/1",
            "physical_host_id": host_id,
            "command_sequence": index,
            "issued_unix_ms": issued,
            "expires_unix_ms": issued
            + profile["resource_bounds"]["max_fault_duration_ms"],
            "binding": binding,
            "command": {"kind": "observe-host-fault", "fault": fault},
        }
        message = b"onebrain:p5:multi-host-control:1\0" + blake3.blake3(
            _canonical_json(payload)
        ).digest()
        commands.append(
            {
                "payload": payload,
                "signer_public_key": public.hex(),
                "signer_fingerprint": _fingerprint(public, profile),
                "signature": signing_key.sign(message).hex(),
            }
        )
    return commands


def _aggregate_root(profile: dict[str, object], receipts: list[dict[str, object]]) -> str:
    faults = {fault: index for index, fault in enumerate(profile["fault_matrix"])}
    ordered = sorted(
        receipts,
        key=lambda row: (
            row["payload"]["physical_host_id"],
            faults[row["payload"]["fault_id"]],
            row["payload"]["command_sequence"],
        ),
    )
    digest = blake3.blake3()
    digest.update(_domain(str(profile["aggregate"]["root_domain"])))
    for row in ordered:
        digest.update(_canonical_json(row))
    return digest.hexdigest()


def _signed_aggregate(
    profile: dict[str, object],
    unsigned: dict[str, object],
    signing_key: Ed25519PrivateKey,
) -> dict[str, object]:
    public = signing_key.public_key().public_bytes_raw()
    role = _role_bindings(profile)["p5-orchestrator"]
    fingerprint = _fingerprint(public, profile)
    if public.hex() != role["public_key_hex"] or fingerprint != role["fingerprint_hex"]:
        raise P5OrchestrationError("aggregate signer is not allowlisted for p5-orchestrator")
    message = _domain(str(profile["aggregate"]["signature_domain"])) + blake3.blake3(
        _canonical_json(unsigned)
    ).digest()
    return {
        **unsigned,
        "aggregate_signer_public_key": public.hex(),
        "aggregate_signer_fingerprint": fingerprint,
        "aggregate_signature": signing_key.sign(message).hex(),
    }


def _run_multi_host_qualification(
    *,
    profile: dict[str, object],
    inventory: dict[str, object],
    binding: dict[str, str],
    executor: SshExecutor,
    timeout_seconds: float,
    production: bool,
    control_signer: Ed25519PrivateKey,
    claimed_aggregate_root: str | None = None,
) -> dict[str, object]:
    _validate_profile(profile, production=production)
    if timeout_seconds <= 0 or timeout_seconds > 330:
        raise P5OrchestrationError("SSH timeout is outside the bounded profile")
    for field in BINDING_FIELDS:
        length = 20 if field in {"candidate_commit", "candidate_tree"} else 32
        _hex(binding.get(field), length, field)
    if binding["trust_policy_digest"] != profile["trust_policy"]["digest_hex"]:
        raise P5OrchestrationError("trust_policy_digest mismatch")
    expected_tier = "production-reference" if production else "nonproduction-test"
    hosts = _validate_inventory(profile, inventory, binding, expected_tier)
    all_receipts: list[dict[str, object]] = []
    fault_set = set(profile["fault_matrix"])
    for host_id in sorted(hosts):
        commands = _control_commands(profile, binding, host_id, control_signer)
        if len(_canonical_json(commands)) > profile["resource_bounds"]["max_control_message_bytes"]:
            raise P5OrchestrationError("control message exceeds its resource bound")
        try:
            receipts = executor.run(hosts[host_id], commands, timeout_seconds)
        except TimeoutError as error:
            raise P5OrchestrationError(f"P5 host {host_id} timed out") from error
        if not receipts:
            raise P5OrchestrationError(f"partial host result from {host_id}")
        payloads = [
            _validate_receipt(
                row,
                profile=profile,
                binding=binding,
                host=hosts[host_id],
                expected_tier=expected_tier,
            )
            for row in receipts
        ]
        observed = [str(row["fault_id"]) for row in payloads]
        if len(observed) != len(fault_set) or set(observed) != fault_set:
            raise P5OrchestrationError(f"host {host_id} did not cover the complete fault matrix")
        sequences = [row["command_sequence"] for row in payloads]
        if any(isinstance(value, bool) or not isinstance(value, int) or value <= 0 for value in sequences):
            raise P5OrchestrationError("child receipt command sequence is invalid")
        if len(set(sequences)) != len(sequences):
            raise P5OrchestrationError("child receipt command sequence is duplicated")
        all_receipts.extend(receipts)
    aggregate_root = _aggregate_root(profile, all_receipts)
    if claimed_aggregate_root is not None and claimed_aggregate_root != aggregate_root:
        raise P5OrchestrationError("claimed aggregate root does not match verified receipts")
    distinct_hosts = len({row["payload"]["physical_host_id"] for row in all_receipts})
    unsigned = {
        "format": profile["aggregate"]["format"],
        "evidence_tier": "production-reference" if production else "nonproduction-test",
        "binding": binding,
        "distinct_physical_hosts": distinct_hosts,
        "verified_child_receipts": len(all_receipts),
        "aggregate_root": aggregate_root,
        "limitations": list(REQUIRED_PRODUCTION_LIMITATIONS),
        "registry_production_qualified": False,
        "base_gate_v1_qualified": False,
        # This controller currently collects exact-host production-reference
        # evidence using observe-host-fault commands. Until the A→B→C→A QUIC
        # ring and injected fault outcomes are measured, qualification is false.
        "multi_host_qualified": False,
        "child_receipts": sorted(
            all_receipts,
            key=lambda row: (
                row["payload"]["physical_host_id"],
                profile["fault_matrix"].index(row["payload"]["fault_id"]),
                row["payload"]["command_sequence"],
            ),
        ),
    }
    return _signed_aggregate(profile, unsigned, control_signer)


def run_multi_host_qualification_for_test_nonproduction(
    *,
    profile: dict[str, object],
    inventory: dict[str, object],
    binding: dict[str, str],
    executor: SshExecutor,
    timeout_seconds: float,
    claimed_aggregate_root: str | None = None,
) -> dict[str, object]:
    return _run_multi_host_qualification(
        profile=profile,
        inventory=inventory,
        binding=binding,
        executor=executor,
        timeout_seconds=timeout_seconds,
        production=False,
        control_signer=profile["_test_orchestrator_private_key"],
        claimed_aggregate_root=claimed_aggregate_root,
    )


@dataclass(frozen=True)
class OpenSshExecutor:
    """Bounded stdio control transport; application traffic remains QUIC-only."""

    def run(
        self,
        host: dict[str, object],
        commands: list[dict[str, object]],
        timeout_seconds: float,
    ) -> list[dict[str, object]]:
        required = ("ssh_destination", "known_hosts_file", "agent_command")
        if any(not isinstance(host.get(field), str) or not host[field] for field in required):
            raise P5OrchestrationError("production inventory lacks SSH execution fields")
        ssh_port = host.get("ssh_port")
        if (
            isinstance(ssh_port, bool)
            or not isinstance(ssh_port, int)
            or not 1 <= ssh_port <= 65535
        ):
            raise P5OrchestrationError("production inventory SSH port is invalid")
        command = [
            "/usr/bin/ssh",
            "-o",
            "BatchMode=yes",
            "-o",
            "StrictHostKeyChecking=yes",
            "-o",
            f"UserKnownHostsFile={host['known_hosts_file']}",
            "-p",
            str(ssh_port),
            str(host["ssh_destination"]),
            str(host["agent_command"]),
        ]
        limit = int(_profile()["resource_bounds"]["max_control_message_bytes"])
        return_code, stdout, stderr = _run_bounded_process(
            command,
            _canonical_json(commands) + b"\n",
            timeout_seconds,
            stdout_limit=limit,
            stderr_limit=4096,
        )
        if return_code != 0:
            raise P5OrchestrationError(
                f"SSH agent failed for {host['physical_host_id']}: "
                + stderr.decode("utf-8", errors="replace")
            )
        try:
            value = json.loads(stdout)
        except json.JSONDecodeError as error:
            raise P5OrchestrationError("SSH agent response is not valid JSON") from error
        if not isinstance(value, list):
            raise P5OrchestrationError("SSH agent response must be a receipt list")
        return value


def _run_bounded_process(
    command: list[str],
    input_bytes: bytes,
    timeout_seconds: float,
    *,
    stdout_limit: int,
    stderr_limit: int,
) -> tuple[int, bytes, bytes]:
    process = subprocess.Popen(
        command,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert process.stdin is not None
    assert process.stdout is not None
    assert process.stderr is not None
    try:
        process.stdin.write(input_bytes)
        process.stdin.close()
        deadline = time.monotonic() + timeout_seconds
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
            stdout_future = pool.submit(process.stdout.read, stdout_limit + 1)
            stderr_future = pool.submit(process.stderr.read, stderr_limit + 1)
            while process.poll() is None:
                if stdout_future.done() and len(stdout_future.result()) > stdout_limit:
                    process.kill()
                    raise P5OrchestrationError(
                        "SSH agent response exceeds the control-message bound"
                    )
                if stderr_future.done() and len(stderr_future.result()) > stderr_limit:
                    process.kill()
                    raise P5OrchestrationError("SSH agent stderr exceeds its fixed bound")
                if time.monotonic() >= deadline:
                    process.kill()
                    raise TimeoutError("SSH agent timed out")
                time.sleep(0.01)
            stdout = stdout_future.result(timeout=1)
            stderr = stderr_future.result(timeout=1)
        if len(stdout) > stdout_limit:
            raise P5OrchestrationError(
                "SSH agent response exceeds the control-message bound"
            )
        if len(stderr) > stderr_limit:
            raise P5OrchestrationError("SSH agent stderr exceeds its fixed bound")
        return process.returncode, stdout, stderr
    finally:
        if process.poll() is None:
            process.kill()
        process.wait(timeout=1)
        for stream in (process.stdin, process.stdout, process.stderr):
            if stream is not None and not stream.closed:
                stream.close()


def _git(candidate_root: Path, *arguments: str) -> str:
    git = Path("/usr/bin/git")
    if not git.is_file():
        raise P5OrchestrationError("fixed production Git executable is unavailable")
    completed = subprocess.run(
        [str(git), "-C", str(candidate_root), *arguments],
        capture_output=True,
        text=True,
        check=False,
    )
    if completed.returncode != 0:
        raise P5OrchestrationError("candidate Git measurement failed")
    return completed.stdout.strip()


def _verified_task28_request(args: argparse.Namespace) -> tuple[dict[str, object], bytes]:
    from scripts.release.create_base_release_request import (
        CANONICAL_HISTORY,
        CANONICAL_PROFILE,
        CANONICAL_TOOLING,
        CANONICAL_VECTOR,
        ReleaseRequestCreationError,
        verify_task28_release_request,
    )

    if args.policy.resolve(strict=True) != (REPOSITORY_ROOT / CANONICAL_VECTOR).resolve(
        strict=True
    ):
        raise P5OrchestrationError("production signer policy is not candidate-owned")
    if args.gpg_home.resolve(strict=True).is_relative_to(REPOSITORY_ROOT.resolve()):
        raise P5OrchestrationError("production GPG home must remain outside the repository")
    try:
        request = verify_task28_release_request(
            args.request,
            args.signature,
            args.policy,
            gpg_home=args.gpg_home,
            gpg_executable=Path("/usr/bin/gpg"),
        )
    except (OSError, ReleaseRequestCreationError) as error:
        raise P5OrchestrationError(f"Base release request is invalid: {error}") from error
    payload = _regular_file_bytes(args.request, "Base release request")
    if payload != _canonical_json(request):
        raise P5OrchestrationError("Base release request changed after signature verification")
    created = datetime.fromisoformat(str(request["created_utc"]).replace("Z", "+00:00"))
    expires = datetime.fromisoformat(str(request["expires_utc"]).replace("Z", "+00:00"))
    now = datetime.now(timezone.utc)
    if not created <= now < expires:
        raise P5OrchestrationError("Base release request is outside its validity interval")
    candidate = request["candidate"]
    if not isinstance(candidate, dict):
        raise P5OrchestrationError("Base release request candidate is invalid")
    if (
        _git(REPOSITORY_ROOT, "rev-parse", "HEAD") != candidate.get("commit")
        or _git(REPOSITORY_ROOT, "show", "-s", "--format=%T", "HEAD")
        != candidate.get("tree")
        or _git(REPOSITORY_ROOT, "rev-parse", "--show-object-format")
        != candidate.get("object_format")
        or _git(REPOSITORY_ROOT, "status", "--porcelain", "--untracked-files=all")
    ):
        raise P5OrchestrationError("production controller is not the exact pristine candidate")
    profile_path = REPOSITORY_ROOT / CANONICAL_PROFILE
    vector_path = REPOSITORY_ROOT / CANONICAL_VECTOR
    history_path = REPOSITORY_ROOT / CANONICAL_HISTORY
    if blake3.blake3(profile_path.read_bytes()).hexdigest() != request["production_profile_blake3"]:
        raise P5OrchestrationError("signed production profile differs from candidate bytes")
    if blake3.blake3(vector_path.read_bytes()).hexdigest() != request["production_vector_blake3"]:
        raise P5OrchestrationError("signed production vector differs from candidate bytes")
    history = _json_object(history_path.read_bytes(), "append-only IDL history")
    if history.get("history_chain", {}).get("root_sha256") != request["append_only_idl_history_root"]:
        raise P5OrchestrationError("signed append-only IDL history differs")
    tooling = request.get("candidate_tooling_blake3")
    if not isinstance(tooling, dict) or set(tooling) != set(CANONICAL_TOOLING):
        raise P5OrchestrationError("signed candidate tooling map is not closed")
    for name, relative in CANONICAL_TOOLING.items():
        if blake3.blake3((REPOSITORY_ROOT / relative).read_bytes()).hexdigest() != tooling[name]:
            raise P5OrchestrationError(f"signed candidate tooling differs: {name}")
    if request.get("required_targets", {}).get("linux") != "x86_64-unknown-linux-gnu":
        raise P5OrchestrationError("signed Linux target differs from P5 reference target")
    return request, payload


def _compiled_binding(agent: Path) -> dict[str, str]:
    return_code, stdout, stderr = _run_bounded_process(
        [str(agent), "--print-compiled-binding"],
        b"",
        30.0,
        stdout_limit=4096,
        stderr_limit=4096,
    )
    if return_code != 0:
        raise P5OrchestrationError(
            "compiled P5 binding probe failed: "
            + stderr.decode("utf-8", errors="replace")
        )
    try:
        value = json.loads(stdout)
    except json.JSONDecodeError as error:
        raise P5OrchestrationError("compiled P5 binding is not JSON") from error
    fields = {
        "format",
        "candidate_commit",
        "candidate_semantic_digest",
        "linux_artifact_tuple_digest",
        "target_triple",
        "toolchain_digest",
    }
    if not isinstance(value, dict) or set(value) != fields:
        raise P5OrchestrationError("compiled P5 binding fields are not closed")
    if value["format"] != "onebrain/p5-compiled-binding/1":
        raise P5OrchestrationError("compiled P5 binding format differs")
    _hex(value["candidate_commit"], 20, "compiled candidate_commit")
    for field in (
        "candidate_semantic_digest",
        "linux_artifact_tuple_digest",
        "toolchain_digest",
    ):
        _hex(value[field], 32, f"compiled {field}")
    if value["target_triple"] != "x86_64-unknown-linux-gnu":
        raise P5OrchestrationError("compiled P5 target is not the Linux reference target")
    return {field: str(value[field]) for field in fields if field != "format"}


def _bundle_manifest_binding(
    bundle_root: Path,
    agent: Path,
    *,
    candidate_commit: str,
    candidate_tree: str,
) -> tuple[str, bytes]:
    root = bundle_root.resolve(strict=True)
    expected_agent = (root / "bin" / "p5_multi_host_agent").resolve(strict=True)
    if agent.resolve(strict=True) != expected_agent:
        raise P5OrchestrationError("P5 agent is not the exact bundle-owned executable")
    manifest_bytes = _regular_file_bytes(
        root / "metadata" / "bundle.manifest.json", "native bundle manifest"
    )
    manifest = _json_object(manifest_bytes, "native bundle manifest")
    if manifest_bytes != _canonical_json(manifest):
        raise P5OrchestrationError("native bundle manifest bytes are not canonical")
    if set(manifest) != {
        "format",
        "qualification_tier",
        "private_material_included",
        "candidate",
        "build",
        "required_runtime",
        "files",
    }:
        raise P5OrchestrationError("native bundle manifest fields are not closed")
    if (
        manifest.get("format") != "onebrain/base-v1-native-runner-bundle/1"
        or manifest.get("qualification_tier") != "prepared-not-production-qualified"
        or manifest.get("private_material_included") is not False
        or manifest.get("candidate")
        != {
            "id": candidate_commit,
            "version": candidate_tree,
            "source_digest": manifest.get("candidate", {}).get("source_digest")
            if isinstance(manifest.get("candidate"), dict)
            else None,
        }
        or manifest.get("required_runtime")
        != {"architecture": "x64", "minimum_glibc": "2.39", "os": "linux"}
    ):
        raise P5OrchestrationError("native bundle manifest candidate/runtime differs")
    candidate_row = manifest["candidate"]
    _hex(candidate_row["source_digest"], 32, "native bundle source digest")
    build = manifest.get("build")
    if (
        not isinstance(build, dict)
        or set(build) != {"digest", "platform", "source_date_epoch"}
        or build.get("platform") != "linux/x64"
        or isinstance(build.get("source_date_epoch"), bool)
        or not isinstance(build.get("source_date_epoch"), int)
        or build["source_date_epoch"] < 0
    ):
        raise P5OrchestrationError("native bundle build identity is invalid")
    _hex(build["digest"], 32, "native bundle build digest")
    rows = manifest.get("files")
    if not isinstance(rows, list) or not 1 <= len(rows) <= 64:
        raise P5OrchestrationError("native bundle manifest file inventory is invalid")
    expected_paths: list[str] = []
    measured: dict[str, bytes] = {}
    for row in rows:
        if not isinstance(row, dict) or set(row) != {
            "path",
            "size",
            "mode",
            "sha256",
            "blake3",
        }:
            raise P5OrchestrationError("native bundle file record fields are not closed")
        relative = row.get("path")
        if (
            not isinstance(relative, str)
            or not relative
            or relative.startswith("/")
            or "\\" in relative
            or any(part in ("", ".", "..") for part in relative.split("/"))
        ):
            raise P5OrchestrationError("native bundle file path is not canonical")
        expected_paths.append(relative)
        file_path = root.joinpath(*relative.split("/"))
        bytes_value = _regular_file_bytes(file_path, f"native bundle file {relative}")
        measured[relative] = bytes_value
        expected_mode = "0555" if relative.startswith(("bin/", "scripts/")) else "0444"
        if (
            row.get("size") != len(bytes_value)
            or row.get("mode") != expected_mode
            or row.get("sha256") != hashlib.sha256(bytes_value).hexdigest()
            or row.get("blake3") != blake3.blake3(bytes_value).hexdigest()
        ):
            raise P5OrchestrationError(f"native bundle file differs: {relative}")
        if stat.S_IMODE(file_path.stat().st_mode) != int(expected_mode, 8):
            raise P5OrchestrationError(f"native bundle file mode differs: {relative}")
    if expected_paths != sorted(expected_paths) or len(set(expected_paths)) != len(expected_paths):
        raise P5OrchestrationError("native bundle file inventory is not sorted and unique")
    actual_paths: list[str] = []
    for current, directories, files in os.walk(root, followlinks=False):
        current_path = Path(current)
        if any((current_path / name).is_symlink() for name in directories):
            raise P5OrchestrationError("native bundle contains a symlink directory")
        for name in files:
            path = current_path / name
            relative = path.relative_to(root).as_posix()
            if relative != "metadata/bundle.manifest.json":
                _regular_file_metadata(path, f"native bundle file {relative}")
                actual_paths.append(relative)
    if sorted(actual_paths) != expected_paths:
        raise P5OrchestrationError("native bundle filesystem differs from manifest inventory")
    sums = _canonical_native_bundle_sha256sums(measured, expected_paths)
    if measured.get("metadata/SHA256SUMS") != sums:
        raise P5OrchestrationError("native bundle SHA256SUMS differs from manifest records")
    if hashlib.sha256(measured.get("metadata/BUILD-PROVENANCE.json", b"")).hexdigest() != build["digest"]:
        raise P5OrchestrationError("native bundle build provenance digest differs")
    if measured.get("metadata/candidate-commit.txt") != (candidate_commit + "\n").encode():
        raise P5OrchestrationError("native bundle candidate commit file differs")
    if measured.get("metadata/candidate-tree.txt") != (candidate_tree + "\n").encode():
        raise P5OrchestrationError("native bundle candidate tree file differs")
    if "bin/p5_multi_host_agent" not in measured:
        raise P5OrchestrationError("native bundle lacks the P5 release agent")
    return blake3.blake3(manifest_bytes).hexdigest(), measured["bin/p5_multi_host_agent"]


def _canonical_native_bundle_sha256sums(
    measured: dict[str, bytes], expected_paths: list[str]
) -> bytes:
    return b"".join(
        f"{hashlib.sha256(measured[path]).hexdigest()} *{path}\n".encode("ascii")
        for path in expected_paths
        if path != "metadata/SHA256SUMS"
    )


def _derive_verified_binding(
    args: argparse.Namespace,
) -> tuple[dict[str, str], dict[str, object]]:
    if not sys.platform.startswith("linux"):
        raise P5OrchestrationError("production P5 orchestration requires Linux")
    request, request_bytes = _verified_task28_request(args)
    candidate = request["candidate"]
    runner_bundle_manifest_digest, agent_bytes = _bundle_manifest_binding(
        args.bundle_root,
        args.agent,
        candidate_commit=str(candidate["commit"]),
        candidate_tree=str(candidate["tree"]),
    )
    agent_signature_digest = _verify_agent_signature(
        agent_bytes,
        runner_bundle_manifest_digest,
        args.agent_signature,
        _profile(),
    )
    compiled = _compiled_binding(args.agent)
    if compiled["candidate_commit"] != candidate["commit"]:
        raise P5OrchestrationError("compiled P5 agent commit differs from signed request")
    registry_candidate = _registry_candidate_binding(args.registry_candidate_root)
    profile = _profile()
    derived = {
        "release_request_digest": blake3.blake3(request_bytes).hexdigest(),
        "qualification_session_id": str(request["qualification_session_id"]),
        "candidate_commit": str(candidate["commit"]),
        "candidate_tree": str(candidate["tree"]),
        "candidate_semantic_digest": compiled["candidate_semantic_digest"],
        "linux_artifact_tuple_digest": compiled["linux_artifact_tuple_digest"],
        "toolchain_digest": compiled["toolchain_digest"],
        "runner_bundle_manifest_digest": runner_bundle_manifest_digest,
        "agent_binary_digest": blake3.blake3(agent_bytes).hexdigest(),
        "agent_signature_digest": agent_signature_digest,
        "registry_root": str(registry_candidate["root"]),
        "profile_digest": blake3.blake3(_canonical_json(profile)).hexdigest(),
        "trust_policy_digest": profile["trust_policy"]["digest_hex"],
    }
    return derived, registry_candidate


def _verified_binding(args: argparse.Namespace, inventory: dict[str, object]) -> dict[str, str]:
    derived, registry_candidate = _derive_verified_binding(args)
    supplied = inventory.get("binding")
    if not isinstance(supplied, dict):
        raise P5OrchestrationError("production inventory lacks its candidate binding")
    if supplied != derived:
        raise P5OrchestrationError("inventory does not match the verified exact candidate")
    if inventory.get("registry_candidate") != registry_candidate:
        raise P5OrchestrationError("inventory does not match measured Registry candidate")
    return derived


def _read_private_key(path: Path) -> Ed25519PrivateKey:
    encoded = path.read_bytes()
    stripped = encoded.strip()
    if len(stripped) == 64:
        try:
            key_bytes = bytes.fromhex(stripped.decode("ascii"))
        except (UnicodeDecodeError, ValueError) as error:
            raise P5OrchestrationError(
                "orchestrator signing key is neither raw nor canonical hex"
            ) from error
        if key_bytes.hex().encode("ascii") != stripped:
            raise P5OrchestrationError("orchestrator signing key hex is not canonical")
    elif len(encoded) == 32:
        key_bytes = encoded
    else:
        raise P5OrchestrationError(
            "orchestrator signing key must be 32 raw bytes or 64 lowercase hex characters"
        )
    return Ed25519PrivateKey.from_private_bytes(key_bytes)


def _physical_machine_fingerprint(
    host_evidence: dict[str, object], placement: dict[str, object]
) -> str:
    machine_id = host_evidence.get("machine_id_sha256")
    virtualization = host_evidence.get("virtualization")
    if (
        host_evidence.get("format") != "onebrain/host-evidence/1"
        or not isinstance(machine_id, dict)
        or machine_id.get("status") != "ok"
        or not isinstance(machine_id.get("value"), str)
        or not isinstance(virtualization, dict)
        or virtualization.get("status") != "ok"
        or not isinstance(placement.get("physical_host_id"), str)
    ):
        raise P5OrchestrationError("host evidence lacks stable machine identity fields")
    _hex(machine_id["value"], 32, "host evidence machine_id_sha256")
    value = {
        "machine_id_sha256": machine_id["value"],
        "physical_host_id": placement["physical_host_id"],
        "virtualization": virtualization.get("value"),
    }
    return blake3.blake3(
        _canonical_json(value),
        derive_key_context=PHYSICAL_MACHINE_FINGERPRINT_CONTEXT,
    ).hexdigest()


def _prepare_signed_inventory(
    *,
    args: argparse.Namespace,
    host_spec_path: Path,
    signing_key: Ed25519PrivateKey,
    config_output_root: Path,
) -> dict[str, object]:
    binding, registry_candidate = _derive_verified_binding(args)
    spec = _json_object(
        _regular_file_bytes(host_spec_path, "P5 host specification"),
        "P5 host specification",
    )
    if set(spec) != {"format", "hosts"} or spec.get("format") != "onebrain/p5-production-host-spec/1":
        raise P5OrchestrationError("P5 host specification fields/format are not closed")
    rows = spec.get("hosts")
    if not isinstance(rows, list) or len(rows) != 3:
        raise P5OrchestrationError("P5 host specification requires exactly three hosts")
    profile = _profile()
    roles = _role_bindings(profile)
    expected_hosts = {row["physical_host_id"]: row for row in profile["topology"]["hosts"]}
    hosts: list[dict[str, object]] = []
    configs: dict[str, dict[str, object]] = {}
    for source in rows:
        if not isinstance(source, dict) or set(source) != HOST_SPEC_FIELDS:
            raise P5OrchestrationError("P5 host specification row fields are not closed")
        host_id = str(source["physical_host_id"])
        if host_id not in expected_hosts:
            raise P5OrchestrationError("P5 host specification physical host is not frozen")
        role = str(source["receipt_role"])
        if (
            role != expected_hosts[host_id]["receipt_role"]
            or role not in roles
            or source["receipt_signer_fingerprint"] != roles[role]["fingerprint_hex"]
        ):
            raise P5OrchestrationError("P5 host specification receipt role/key differs")
        evidence_path = Path(str(source["host_evidence_path"])).resolve(strict=True)
        placement_path = Path(str(source["placement_evidence_path"])).resolve(strict=True)
        host_evidence_bytes = _regular_file_bytes(evidence_path, f"{host_id} host evidence")
        placement_bytes = _regular_file_bytes(placement_path, f"{host_id} placement evidence")
        host_evidence = _json_object(host_evidence_bytes, f"{host_id} host evidence")
        placement = _json_object(placement_bytes, f"{host_id} placement evidence")
        if (
            host_evidence.get("runner_id") != source["runner_identity"]
            or host_evidence.get("placement") != placement
            or placement.get("receipt_verified") is not True
        ):
            raise P5OrchestrationError(f"{host_id} host/placement evidence differs")
        machine_fingerprint = _physical_machine_fingerprint(host_evidence, placement)
        public_row = {
            field: source[field]
            for field in (
                "physical_host_id",
                "runner_identity",
                "ssh_host_key_algorithm",
                "ssh_host_key_fingerprint",
                "observed_ssh_host_key_fingerprint",
                "receipt_role",
                "receipt_signer_fingerprint",
                "durable_root_locator",
                "expected_principal",
                "ssh_destination",
                "ssh_port",
                "known_hosts_file",
                "agent_command",
            )
        }
        public_row.update(
            {
                "physical_machine_fingerprint": machine_fingerprint,
                "host_evidence_sha256": hashlib.sha256(host_evidence_bytes).hexdigest(),
                "placement_evidence_sha256": hashlib.sha256(placement_bytes).hexdigest(),
            }
        )
        hosts.append(public_row)
        configs[host_id] = {
            "physical_host_id": host_id,
            "runner_identity": source["runner_identity"],
            "ssh_host_key_fingerprint": source["ssh_host_key_fingerprint"],
            "physical_machine_fingerprint": machine_fingerprint,
            "host_evidence_sha256": public_row["host_evidence_sha256"],
            "placement_evidence_sha256": public_row["placement_evidence_sha256"],
            "durable_root_locator": source["durable_root_locator"],
            "expected_principal": source["expected_principal"],
            "agent_signature_path": source["remote_agent_signature_path"],
            "binding": binding,
            "evidence_tier": "production-reference",
            "limitations": list(REQUIRED_PRODUCTION_LIMITATIONS),
        }
    if set(row["physical_host_id"] for row in hosts) != set(expected_hosts):
        raise P5OrchestrationError("P5 host specification host set differs")
    unsigned = {
        "format": "onebrain/p5-multi-host-inventory/1",
        "evidence_tier": "production-reference",
        "binding": binding,
        "limitations": list(REQUIRED_PRODUCTION_LIMITATIONS),
        "registry_candidate": registry_candidate,
        "hosts": sorted(hosts, key=lambda row: str(row["physical_host_id"])),
    }
    public = signing_key.public_key().public_bytes_raw()
    role = roles["p5-orchestrator"]
    if public.hex() != role["public_key_hex"]:
        raise P5OrchestrationError("orchestrator private key does not match the frozen role")
    inventory = {
        **unsigned,
        "signer_public_key": public.hex(),
        "signer_fingerprint": _fingerprint(public, profile),
        "signature": signing_key.sign(inventory_signature_message(unsigned)).hex(),
    }
    config_output_root.mkdir(parents=True, exist_ok=True)
    for host_id, config in configs.items():
        _write_atomic(
            config_output_root / f"{host_id}-agent-config.json",
            _canonical_json(config) + b"\n",
        )
    return inventory


def _write_atomic(path: Path, bytes_value: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".p5-new")
    try:
        with temporary.open("xb") as stream:
            stream.write(bytes_value)
            stream.flush()
            os.fsync(stream.fileno())
        os.link(temporary, path)
        if os.name != "nt":
            descriptor = os.open(path.parent, os.O_RDONLY)
            try:
                os.fsync(descriptor)
            finally:
                os.close(descriptor)
    finally:
        temporary.unlink(missing_ok=True)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--inventory", type=Path)
    mode.add_argument("--prepare-inventory-host-spec", type=Path)
    parser.add_argument("--host-config-output-root", type=Path)
    parser.add_argument("--request", type=Path, required=True)
    parser.add_argument("--signature", type=Path, required=True)
    parser.add_argument("--policy", type=Path, required=True)
    parser.add_argument("--gpg-home", type=Path, required=True)
    parser.add_argument("--bundle-root", type=Path, required=True)
    parser.add_argument("--agent", type=Path, required=True)
    parser.add_argument("--agent-signature", type=Path, required=True)
    parser.add_argument("--registry-candidate-root", type=Path, required=True)
    parser.add_argument("--orchestrator-signing-key", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--timeout-seconds", type=float, default=330.0)
    args = parser.parse_args(argv)
    try:
        control_signer = _read_private_key(args.orchestrator_signing_key)
        role = _role_bindings(_profile())["p5-orchestrator"]
        if control_signer.public_key().public_bytes_raw().hex() != role["public_key_hex"]:
            raise P5OrchestrationError("orchestrator private key does not match the frozen role")
        if args.prepare_inventory_host_spec is not None:
            if args.host_config_output_root is None:
                raise P5OrchestrationError(
                    "inventory preparation requires --host-config-output-root"
                )
            inventory = _prepare_signed_inventory(
                args=args,
                host_spec_path=args.prepare_inventory_host_spec,
                signing_key=control_signer,
                config_output_root=args.host_config_output_root,
            )
            _write_atomic(args.output, _canonical_json(inventory) + b"\n")
            return 0
        if args.host_config_output_root is not None:
            raise P5OrchestrationError(
                "--host-config-output-root is valid only during inventory preparation"
            )
        assert args.inventory is not None
        inventory = _json_object(
            _regular_file_bytes(args.inventory, "P5 production inventory"),
            "P5 production inventory",
        )
        binding = _verified_binding(args, inventory)
        report = _run_multi_host_qualification(
            profile=_profile(),
            inventory=inventory,
            binding=binding,
            executor=OpenSshExecutor(),
            timeout_seconds=args.timeout_seconds,
            production=True,
            control_signer=control_signer,
        )
        _write_atomic(args.output, _canonical_json(report) + b"\n")
    except (OSError, ValueError, P5OrchestrationError) as error:
        print(f"P5 multi-host qualification failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
