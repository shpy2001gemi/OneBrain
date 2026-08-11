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
import json
import os
import subprocess
import sys
import time
from dataclasses import dataclass
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
    "agent_binary_digest",
    "agent_signature_digest",
    "registry_root",
    "profile_digest",
    "trust_policy_digest",
    "runner_identity",
    "ssh_host_key_fingerprint",
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
    "agent_binary_digest",
    "agent_signature_digest",
    "registry_root",
    "profile_digest",
    "trust_policy_digest",
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


class P5OrchestrationError(RuntimeError):
    """A fail-closed P5 orchestration or evidence validation error."""


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


def child_receipt_signature_message(payload: object) -> bytes:
    profile = _profile()
    return _domain(str(profile["child_receipt"]["signature_domain"])) + blake3.blake3(
        _canonical_json(payload)
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
            }
        )
    return {
        "format": "onebrain/p5-multi-host-inventory/1",
        "evidence_tier": "nonproduction-test",
        "binding": copy.deepcopy(binding),
        "hosts": hosts,
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
        "limitations": ["nonproduction-test-key", "single-process-fixture"],
    }
    public = signing_key.public_key().public_bytes_raw()
    return {
        "format": profile["child_receipt"]["format"],
        "evidence_tier": "nonproduction-test",
        "payload": payload,
        "signer_public_key": public.hex(),
        "signer_fingerprint": _fingerprint(public, profile),
        "signature": signing_key.sign(child_receipt_signature_message(payload)).hex(),
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
    profile: dict[str, object], inventory: dict[str, object], binding: dict[str, str]
) -> dict[str, dict[str, object]]:
    if inventory.get("format") != "onebrain/p5-multi-host-inventory/1":
        raise P5OrchestrationError("inventory format mismatch")
    if inventory.get("binding") != binding:
        raise P5OrchestrationError("inventory candidate binding mismatch")
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
            child_receipt_signature_message(payload),
        )
    except (ValueError, InvalidSignature) as error:
        raise P5OrchestrationError("child receipt signature is invalid") from error
    if payload["result"] != "pass":
        raise P5OrchestrationError("child receipt does not report a passing result")
    if not isinstance(payload["limitations"], list) or not all(
        isinstance(value, str) and value for value in payload["limitations"]
    ):
        raise P5OrchestrationError("child receipt limitations are invalid")
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
    hosts = _validate_inventory(profile, inventory, binding)
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
                expected_tier=(
                    "production-reference" if production else "nonproduction-test"
                ),
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
    verified_matrix = len(all_receipts) == len(profile["fault_matrix"]) * 3
    production_evidence = production and all(
        row["evidence_tier"] == "production-reference" for row in all_receipts
    )
    unsigned = {
        "format": profile["aggregate"]["format"],
        "evidence_tier": "production-reference" if production else "nonproduction-test",
        "binding": binding,
        "distinct_physical_hosts": distinct_hosts,
        "verified_child_receipts": len(all_receipts),
        "aggregate_root": aggregate_root,
        "multi_host_qualified": bool(
            production_evidence
            and verified_matrix
            and distinct_hosts >= profile["aggregate"]["minimum_distinct_physical_hosts"]
        ),
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
        command = [
            "/usr/bin/ssh",
            "-o",
            "BatchMode=yes",
            "-o",
            "StrictHostKeyChecking=yes",
            "-o",
            f"UserKnownHostsFile={host['known_hosts_file']}",
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


def _verified_binding(args: argparse.Namespace, inventory: dict[str, object]) -> dict[str, str]:
    if not sys.platform.startswith("linux"):
        raise P5OrchestrationError("production P5 orchestration requires Linux")
    from scripts.release.verify_base_release_request import (
        ReleaseRequestError,
        verify_release_request,
    )

    try:
        verified = verify_release_request(
            args.request, args.signature, args.policy, args.gpg_home
        ).as_dict()
    except ReleaseRequestError as error:
        raise P5OrchestrationError(f"Base release request is invalid: {error}") from error
    supplied = inventory.get("binding")
    if not isinstance(supplied, dict):
        raise P5OrchestrationError("production inventory lacks its candidate binding")
    run = verified["run_context"]
    release = verified["bindings"]
    profile = _profile()
    derived = {
        "release_request_digest": run["release_request_digest"],
        "qualification_session_id": run["qualification_session_id"],
        "candidate_commit": run["candidate_commit"],
        "candidate_tree": run["candidate_tree"],
        "candidate_semantic_digest": release["candidate_semantic_digest"],
        "linux_artifact_tuple_digest": release["artifact_tuple_digest"],
        "agent_binary_digest": blake3.blake3(args.agent.read_bytes()).hexdigest(),
        "agent_signature_digest": blake3.blake3(args.agent_signature.read_bytes()).hexdigest(),
        "registry_root": release["release_aggregate_root"],
        "profile_digest": blake3.blake3(_canonical_json(profile)).hexdigest(),
        "trust_policy_digest": profile["trust_policy"]["digest_hex"],
    }
    if supplied != derived:
        raise P5OrchestrationError("inventory does not match the verified exact candidate")
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


def _write_atomic(path: Path, bytes_value: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".p5-new")
    try:
        with temporary.open("xb") as stream:
            stream.write(bytes_value)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inventory", type=Path, required=True)
    parser.add_argument("--request", type=Path, required=True)
    parser.add_argument("--signature", type=Path, required=True)
    parser.add_argument("--policy", type=Path, required=True)
    parser.add_argument("--gpg-home", type=Path, required=True)
    parser.add_argument("--agent", type=Path, required=True)
    parser.add_argument("--agent-signature", type=Path, required=True)
    parser.add_argument("--orchestrator-signing-key", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--timeout-seconds", type=float, default=330.0)
    args = parser.parse_args(argv)
    try:
        inventory = json.loads(args.inventory.read_text(encoding="utf-8"))
        binding = _verified_binding(args, inventory)
        control_signer = _read_private_key(args.orchestrator_signing_key)
        role = _role_bindings(_profile())["p5-orchestrator"]
        if control_signer.public_key().public_bytes_raw().hex() != role["public_key_hex"]:
            raise P5OrchestrationError("orchestrator private key does not match the frozen role")
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
