#!/usr/bin/env python3
"""Fail-closed Base v1 soak carry-forward and signed-receipt validator.

Carry-forward is deliberately analytical only.  Base v1 always requires a
fresh uninterrupted 72-hour run on the exact Task 27 candidate in Task 28.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import stat
import sys
import time
from datetime import datetime
from pathlib import Path
from typing import Iterable

import blake3
from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives.asymmetric.ed25519 import (
    Ed25519PrivateKey,
    Ed25519PublicKey,
)


ROOT = Path(__file__).resolve().parents[2]
# This file is invoked both as a package module and as a repository-relative
# script by production qualification workflows.  In the latter form Python
# places ``scripts/release`` (not the repository root) on sys.path, so the
# fail-closed verifier's lazy ``scripts.release`` imports would otherwise fail
# after all inputs have already been staged.
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))
PROFILE_PATH = ROOT / "src/test-vectors/vnext/base-v1-exact-candidate-soak-v1.json"
HEX_32_FIELDS = {
    "candidate_semantic_digest",
    "frozen_target_artifact_digest",
    "registry_root",
    "p5_aggregate_root",
    "executable_blake3",
    "sbom_blake3",
    "provenance_blake3",
    "runner_image_digest",
    "toolchain_digest",
    "lockfile_digest",
    "release_request_digest",
    "qualification_session_id",
}


class EvidenceCarryForwardError(RuntimeError):
    """Invalid carry-forward input."""


class SoakEvidenceError(RuntimeError):
    """Invalid exact-candidate soak evidence."""


def _canonical_json(value: object) -> bytes:
    return json.dumps(
        value, sort_keys=True, separators=(",", ":"), ensure_ascii=True
    ).encode("utf-8")


def _domain(value: str) -> bytes:
    return value.replace("\\0", "\0").encode("ascii")


def _hex(value: object, byte_length: int, field: str, error_type=SoakEvidenceError) -> str:
    if not isinstance(value, str) or len(value) != byte_length * 2:
        raise error_type(f"{field} must be {byte_length}-byte lowercase hex")
    try:
        decoded = bytes.fromhex(value)
    except ValueError as error:
        raise error_type(f"{field} must be hexadecimal") from error
    if decoded.hex() != value:
        raise error_type(f"{field} must be lowercase canonical hex")
    return value


def _validate_candidate(candidate: dict[str, object]) -> None:
    if candidate.get("format") != "onebrain/base-v1-candidate-identity/1":
        raise EvidenceCarryForwardError("candidate identity format mismatch")
    object_format = candidate.get("object_format")
    expected = 20 if object_format == "sha1" else 32 if object_format == "sha256" else 0
    if not expected:
        raise EvidenceCarryForwardError("candidate object_format must be sha1 or sha256")
    for field in ("candidate_commit", "candidate_tree"):
        try:
            _hex(candidate.get(field), expected, field, EvidenceCarryForwardError)
        except EvidenceCarryForwardError as error:
            raise EvidenceCarryForwardError(f"full {field} is required") from error
    for field in sorted(HEX_32_FIELDS):
        _hex(candidate.get(field), 32, field, EvidenceCarryForwardError)


def _critical_changed_paths(changed_paths: Iterable[str]) -> list[str]:
    roots = (
        "src/onebrain-archive/",
        "src/onebrain-node/",
        "src/onebrain-api/",
        "src/onebrain-cli/",
        "src/onebrain-base-abi/",
        "scripts/concept_registry/",
        "src/ku-core/",
        "src/ku-net/",
    )
    exact = {"src/Cargo.lock", "rust-toolchain.toml", "src/rust-toolchain.toml"}
    normalized = sorted({str(value).replace("\\", "/") for value in changed_paths})
    return [path for path in normalized if path in exact or path.startswith(roots)]


def analyze_evidence_carry_forward(
    evidence: dict[str, object],
    candidate: dict[str, object],
    changed_paths: Iterable[str],
) -> dict[str, object]:
    """Return an auditable analytical result that can never qualify Base v1."""
    _validate_candidate(candidate)
    reasons: list[str] = []
    source = evidence.get("source_binding")
    legacy = evidence.get("profile") == "onebrain/dr-m5-soak-release/1"
    if not isinstance(source, dict):
        reasons.append("authenticated source binding is missing")
    else:
        for field, expected in candidate.items():
            if source.get(field) != expected:
                reasons.append(f"source binding {field} differs from candidate")
    runner_identity = evidence.get("runner_identity")
    if not isinstance(runner_identity, str) or not runner_identity.strip():
        reasons.append("runner identity is missing")
    critical = _critical_changed_paths(changed_paths)
    if critical:
        reasons.append("candidate closure contains changed authority or tooling paths")
    analytical = not reasons
    evidence_bytes = _canonical_json(evidence)
    candidate_bytes = _canonical_json(candidate)
    return {
        "format": "onebrain/base-v1-evidence-carry-forward-analysis/1",
        "evidence_path_class": "legacy-m5-07" if legacy else "base-v1-bound",
        "evidence_blake3": blake3.blake3(evidence_bytes).hexdigest(),
        "candidate_identity_blake3": blake3.blake3(candidate_bytes).hexdigest(),
        "changed_closure_paths": critical,
        "analytically_reusable": analytical,
        "rejection_reasons": reasons,
        "base_v1_reusable": False,
        "fresh_soak_required": True,
        "production_qualified": False,
        "base_v1_rejection_reason": (
            "Base v1 requires a fresh 72-hour soak on the exact Task 27 commit "
            "under the Task 28 signed release request"
        ),
    }


def _profile() -> dict[str, object]:
    value = json.loads(PROFILE_PATH.read_text(encoding="utf-8"))
    if value.get("format") != "onebrain/base-v1-exact-candidate-soak/1":
        raise SoakEvidenceError("exact-candidate soak profile format mismatch")
    return value


def _fingerprint(public_key: bytes, profile: dict[str, object]) -> str:
    return blake3.blake3(
        public_key,
        derive_key_context=str(profile["trust_policy"]["fingerprint_context"]),
    ).hexdigest()


def _policy_digest(policy: dict[str, object], profile: dict[str, object]) -> str:
    return blake3.blake3(
        _canonical_json(policy),
        derive_key_context=str(profile["trust_policy"]["digest_context"]),
    ).hexdigest()


def profile_for_test_nonproduction(
    runner_keys: dict[str, Ed25519PrivateKey],
    aggregator_key: Ed25519PrivateKey,
) -> dict[str, object]:
    profile = _profile()
    expected = [str(row["runner_id"]) for row in profile["runners"]]
    if sorted(runner_keys) != sorted(expected):
        raise SoakEvidenceError("test runner key set does not match the frozen runner set")
    rows = []
    for runner_id in expected:
        public = runner_keys[runner_id].public_key().public_bytes_raw()
        rows.append(
            {
                "role": f"soak-runner:{runner_id}",
                "public_key_hex": public.hex(),
                "fingerprint_hex": _fingerprint(public, profile),
            }
        )
    public = aggregator_key.public_key().public_bytes_raw()
    rows.append(
        {
            "role": "soak-aggregator",
            "public_key_hex": public.hex(),
            "fingerprint_hex": _fingerprint(public, profile),
        }
    )
    profile["trust_policy"]["policy"]["role_bindings"] = rows
    profile["trust_policy"]["digest_hex"] = _policy_digest(
        profile["trust_policy"]["policy"], profile
    )
    profile["trust_policy"]["owner_approval"] = {
        "status": "nonproduction-test",
        "approved_utc": None,
    }
    profile["qualification_state"] = {
        "contract_frozen": False,
        "measured_evidence_committed": False,
        "soak_qualified": False,
        "production_qualified": False,
    }
    profile["_test_only_nonproduction"] = True
    return profile


def _roles(profile: dict[str, object]) -> dict[str, dict[str, str]]:
    return {
        str(row["role"]): row
        for row in profile["trust_policy"]["policy"]["role_bindings"]
    }


def _validate_profile(profile: dict[str, object], production: bool) -> None:
    frozen = _profile()
    if production:
        if profile != frozen:
            raise SoakEvidenceError("production requires the byte-frozen soak profile")
        if profile["trust_policy"]["owner_approval"]["status"] != "owner-approved":
            raise SoakEvidenceError("soak signer policy lacks owner approval")
    elif not profile.get("_test_only_nonproduction"):
        raise SoakEvidenceError("nonproduction helper requires an explicit test profile")
    if _policy_digest(profile["trust_policy"]["policy"], profile) != profile["trust_policy"]["digest_hex"]:
        raise SoakEvidenceError("soak trust-policy digest mismatch")
    seen: set[str] = set()
    for role in profile["trust_policy"]["policy"]["role_bindings"]:
        public = bytes.fromhex(str(role["public_key_hex"]))
        if role["fingerprint_hex"] != _fingerprint(public, profile):
            raise SoakEvidenceError("soak signer fingerprint mismatch")
        if role["fingerprint_hex"] in seen:
            raise SoakEvidenceError("soak signer cannot be reused across roles")
        seen.add(str(role["fingerprint_hex"]))


def _child_signature_message(profile: dict[str, object], payload: object) -> bytes:
    return _domain(str(profile["child_receipt"]["signature_domain"])) + blake3.blake3(
        _canonical_json(payload)
    ).digest()


def sign_soak_child_for_test_nonproduction(
    *,
    profile: dict[str, object],
    binding: dict[str, str],
    runner_id: str,
    interval_sequence: int,
    receipt_kind: str,
    signing_key: Ed25519PrivateKey,
) -> dict[str, object]:
    if not profile.get("_test_only_nonproduction"):
        raise SoakEvidenceError("test signer requires a nonproduction profile")
    payload = {
        **binding,
        "role": f"soak-runner:{runner_id}",
        "runner_id": runner_id,
        "runner_identity": f"local-process:{runner_id}",
        "interval_sequence": interval_sequence,
        "receipt_kind": receipt_kind,
        "monotonic_start_ns": interval_sequence * 1_000_000,
        "monotonic_end_ns": interval_sequence * 1_000_000 + 1_000,
        "command": "dry-run-exact-candidate-soak-protocol",
        "result": {
            "status": "pass",
            "raw_report_blake3": blake3.blake3(runner_id.encode()).hexdigest(),
            "elapsed_seconds": 0,
            "pre_release_qualified": False,
            "fault_cycle_counts": {
                "slow-peer": 1,
                "bounded-session-flood": 1,
                "partition-reunion": 1,
            },
            "rollback_recommended": False,
            "active_sessions_after_shutdown": 0,
            "semantic_amplification": False,
        },
        "limitations": ["nonproduction-test-key", "local-process-dry-run"],
    }
    public = signing_key.public_key().public_bytes_raw()
    return {
        "format": profile["child_receipt"]["format"],
        "evidence_tier": "nonproduction-test",
        "payload": payload,
        "signer_public_key": public.hex(),
        "signer_fingerprint": _fingerprint(public, profile),
        "signature": signing_key.sign(_child_signature_message(profile, payload)).hex(),
    }


def _validate_binding(profile: dict[str, object], binding: dict[str, str]) -> None:
    required = set(profile["child_receipt"]["required_bindings"])
    if set(binding) != required:
        raise SoakEvidenceError("soak binding has unknown or missing fields")
    for field in required:
        length = 20 if field in {"candidate_commit", "candidate_tree"} else 32
        _hex(binding.get(field), length, field)
    if binding["trust_policy_digest"] != profile["trust_policy"]["digest_hex"]:
        raise SoakEvidenceError("soak trust-policy digest mismatch")


def _validate_child(
    receipt: dict[str, object],
    profile: dict[str, object],
    binding: dict[str, str],
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
        raise SoakEvidenceError("soak child receipt has unknown or missing fields")
    if receipt["format"] != profile["child_receipt"]["format"]:
        raise SoakEvidenceError("soak child receipt format mismatch")
    if receipt["evidence_tier"] != expected_tier:
        raise SoakEvidenceError("soak child evidence tier mismatch")
    payload = receipt.get("payload")
    payload_fields = set(profile["child_receipt"]["required_payload_fields"])
    binding_fields = set(profile["child_receipt"]["required_bindings"])
    if not isinstance(payload, dict) or set(payload) != payload_fields | binding_fields:
        raise SoakEvidenceError("soak child payload has unknown or missing fields")
    for field in binding_fields:
        if payload[field] != binding[field]:
            raise SoakEvidenceError(f"soak child {field} mismatch")
    runner_id = payload["runner_id"]
    expected_role = f"soak-runner:{runner_id}"
    if payload["role"] != expected_role:
        raise SoakEvidenceError("soak child role does not match runner")
    configured = {row["runner_id"] for row in profile["runners"]}
    if runner_id not in configured:
        raise SoakEvidenceError("soak child runner is not configured")
    if not isinstance(payload["runner_identity"], str) or not payload["runner_identity"]:
        raise SoakEvidenceError("soak child runner identity is missing")
    if payload["receipt_kind"] not in profile["child_receipt"]["receipt_kinds"]:
        raise SoakEvidenceError("soak child receipt kind mismatch")
    for field in ("interval_sequence", "monotonic_start_ns", "monotonic_end_ns"):
        value = payload[field]
        if isinstance(value, bool) or not isinstance(value, int) or value < 0:
            raise SoakEvidenceError(f"soak child {field} is invalid")
    if payload["interval_sequence"] <= 0 or payload["monotonic_end_ns"] <= payload["monotonic_start_ns"]:
        raise SoakEvidenceError("soak child monotonic interval is invalid")
    if not isinstance(payload["command"], str) or not payload["command"]:
        raise SoakEvidenceError("soak child command is missing")
    result = payload["result"]
    result_fields = {
        "status",
        "raw_report_blake3",
        "elapsed_seconds",
        "pre_release_qualified",
        "fault_cycle_counts",
        "rollback_recommended",
        "active_sessions_after_shutdown",
        "semantic_amplification",
    }
    if not isinstance(result, dict) or set(result) != result_fields or result.get("status") != "pass":
        raise SoakEvidenceError("soak child result is not passing")
    _hex(result["raw_report_blake3"], 32, "raw_report_blake3")
    if (
        isinstance(result["elapsed_seconds"], bool)
        or not isinstance(result["elapsed_seconds"], int)
        or result["elapsed_seconds"] < 0
    ):
        raise SoakEvidenceError("soak child elapsed_seconds is invalid")
    counts = result["fault_cycle_counts"]
    if not isinstance(counts, dict) or set(counts) != set(profile["fault_cycle"]):
        raise SoakEvidenceError("soak child fault cycle counts are incomplete")
    if any(isinstance(value, bool) or not isinstance(value, int) or value < 0 for value in counts.values()):
        raise SoakEvidenceError("soak child fault cycle count is invalid")
    for field in ("pre_release_qualified", "rollback_recommended", "semantic_amplification"):
        if not isinstance(result[field], bool):
            raise SoakEvidenceError(f"soak child {field} is invalid")
    if (
        isinstance(result["active_sessions_after_shutdown"], bool)
        or not isinstance(result["active_sessions_after_shutdown"], int)
        or result["active_sessions_after_shutdown"] < 0
    ):
        raise SoakEvidenceError("soak child active session result is invalid")
    if not isinstance(payload["limitations"], list) or not all(
        isinstance(value, str) and value for value in payload["limitations"]
    ):
        raise SoakEvidenceError("soak child limitations are invalid")
    role = _roles(profile).get(expected_role)
    if role is None:
        raise SoakEvidenceError("soak child role is not allowlisted")
    if receipt["signer_public_key"] != role["public_key_hex"] or receipt["signer_fingerprint"] != role["fingerprint_hex"]:
        raise SoakEvidenceError("soak child signer is not allowlisted for its role")
    try:
        Ed25519PublicKey.from_public_bytes(bytes.fromhex(str(receipt["signer_public_key"]))).verify(
            bytes.fromhex(str(receipt["signature"])),
            _child_signature_message(profile, payload),
        )
    except (ValueError, InvalidSignature) as error:
        raise SoakEvidenceError("soak child signature is invalid") from error
    return payload


def _aggregate_root(profile: dict[str, object], receipts: list[dict[str, object]]) -> str:
    ordered = sorted(
        receipts,
        key=lambda row: (
            row["payload"]["runner_id"],
            row["payload"]["monotonic_start_ns"],
            row["payload"]["interval_sequence"],
            row["payload"]["receipt_kind"],
        ),
    )
    digest = blake3.blake3()
    digest.update(_domain(str(profile["aggregate"]["root_domain"])))
    for receipt in ordered:
        digest.update(_canonical_json(receipt))
    return digest.hexdigest()


def _aggregate_soak_receipts(
    *,
    profile: dict[str, object],
    binding: dict[str, str],
    receipts: list[dict[str, object]],
    aggregator_key: Ed25519PrivateKey,
    production: bool,
    claimed_root: str | None = None,
) -> dict[str, object]:
    _validate_profile(profile, production)
    _validate_binding(profile, binding)
    if not receipts:
        raise SoakEvidenceError("soak aggregate has no child receipts")
    expected_tier = "production-reference" if production else "nonproduction-test"
    payloads = [
        _validate_child(receipt, profile, binding, expected_tier)
        for receipt in receipts
    ]
    root = _aggregate_root(profile, receipts)
    if claimed_root is not None and claimed_root != root:
        raise SoakEvidenceError("claimed aggregate root includes or differs from child root")
    distinct = len({str(payload["runner_id"]) for payload in payloads})
    elapsed_by_runner: dict[str, int] = {}
    for payload in payloads:
        if payload["receipt_kind"] == "interval":
            runner_id = str(payload["runner_id"])
            elapsed_by_runner[runner_id] = elapsed_by_runner.get(runner_id, 0) + (
                int(payload["monotonic_end_ns"]) - int(payload["monotonic_start_ns"])
            )
    minimum_ns = int(profile["scope"]["minimum_uninterrupted_elapsed_seconds"]) * 1_000_000_000
    complete_results = all(
        payload["result"]["pre_release_qualified"]
        and not payload["result"]["rollback_recommended"]
        and payload["result"]["active_sessions_after_shutdown"] == 0
        and not payload["result"]["semantic_amplification"]
        and all(payload["result"]["fault_cycle_counts"][fault] > 0 for fault in profile["fault_cycle"])
        for payload in payloads
        if payload["receipt_kind"] == "interval"
    )
    expected_runners = {str(row["runner_id"]) for row in profile["runners"]}
    observed_runners = {str(payload["runner_id"]) for payload in payloads}
    interval_payloads = [
        payload for payload in payloads if payload["receipt_kind"] == "interval"
    ]
    runner_identities = [
        str(payload["runner_identity"])
        for payload in interval_payloads
    ]
    production_evidence = bool(
        production
        and distinct >= profile["scope"]["minimum_distinct_physical_runners"]
        and observed_runners == expected_runners
        and len(interval_payloads) == len(expected_runners)
        and {str(payload["runner_id"]) for payload in interval_payloads}
        == expected_runners
        and all(payload["interval_sequence"] == 1 for payload in interval_payloads)
        and len(set(runner_identities)) == len(expected_runners)
        and all(value >= minimum_ns for value in elapsed_by_runner.values())
        and len(elapsed_by_runner) >= profile["scope"]["minimum_distinct_physical_runners"]
        and complete_results
    )
    if production and not production_evidence:
        raise SoakEvidenceError("fresh exact-candidate soak evidence is incomplete")
    unsigned = {
        "format": profile["aggregate"]["format"],
        "evidence_tier": expected_tier,
        "binding": binding,
        "distinct_physical_runners": distinct,
        "verified_child_receipts": len(receipts),
        "aggregate_root": root,
        "soak_qualified": production_evidence,
        "production_qualified": production_evidence,
        "child_receipts": sorted(
            receipts,
            key=lambda row: (
                row["payload"]["runner_id"],
                row["payload"]["monotonic_start_ns"],
                row["payload"]["interval_sequence"],
                row["payload"]["receipt_kind"],
            ),
        ),
    }
    role = _roles(profile)["soak-aggregator"]
    public = aggregator_key.public_key().public_bytes_raw()
    fingerprint = _fingerprint(public, profile)
    if public.hex() != role["public_key_hex"] or fingerprint != role["fingerprint_hex"]:
        raise SoakEvidenceError("aggregate signer is not allowlisted for soak-aggregator")
    message = _domain(str(profile["aggregate"]["signature_domain"])) + blake3.blake3(
        _canonical_json(unsigned)
    ).digest()
    return {
        **unsigned,
        "aggregate_signer_public_key": public.hex(),
        "aggregate_signer_fingerprint": fingerprint,
        "aggregate_signature": aggregator_key.sign(message).hex(),
    }


def aggregate_soak_receipts_for_test_nonproduction(
    *,
    profile: dict[str, object],
    binding: dict[str, str],
    receipts: list[dict[str, object]],
    aggregator_key: Ed25519PrivateKey,
    claimed_root: str | None = None,
) -> dict[str, object]:
    return _aggregate_soak_receipts(
        profile=profile,
        binding=binding,
        receipts=receipts,
        aggregator_key=aggregator_key,
        production=False,
        claimed_root=claimed_root,
    )


def _read_private_key(path: Path) -> Ed25519PrivateKey:
    encoded = path.read_bytes()
    stripped = encoded.strip()
    if len(stripped) == 64:
        try:
            key_bytes = bytes.fromhex(stripped.decode("ascii"))
        except (UnicodeDecodeError, ValueError) as error:
            raise SoakEvidenceError("signing key is not canonical hex") from error
        if key_bytes.hex().encode("ascii") != stripped:
            raise SoakEvidenceError("signing key hex is not canonical")
    elif len(encoded) == 32:
        key_bytes = encoded
    else:
        raise SoakEvidenceError("signing key must be 32 raw bytes or 64 lowercase hex characters")
    return Ed25519PrivateKey.from_private_bytes(key_bytes)


def _verify_p5_aggregate_v1(
    aggregate_path: Path,
    verified: dict[str, object],
) -> tuple[str, dict[str, str]]:
    encoded = aggregate_path.read_bytes()
    if len(encoded) > 16 * 1024 * 1024:
        raise SoakEvidenceError("P5 aggregate exceeds its bounded input size")
    try:
        report = json.loads(encoded)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise SoakEvidenceError("P5 aggregate is not valid JSON") from error
    expected_report_fields = {
        "format",
        "evidence_tier",
        "binding",
        "distinct_physical_hosts",
        "verified_child_receipts",
        "aggregate_root",
        "multi_host_qualified",
        "child_receipts",
        "aggregate_signer_public_key",
        "aggregate_signer_fingerprint",
        "aggregate_signature",
    }
    if not isinstance(report, dict) or set(report) != expected_report_fields:
        raise SoakEvidenceError("P5 aggregate has unknown or missing fields")
    p5_profile = json.loads(
        (
            ROOT
            / "src/test-vectors/vnext/p5-multi-host-production-qualification-v1.json"
        ).read_text(encoding="utf-8")
    )
    if (
        report["format"] != p5_profile["aggregate"]["format"]
        or report["evidence_tier"] != "production-reference"
        or report["multi_host_qualified"] is not True
        or report["distinct_physical_hosts"] != 3
        or report["verified_child_receipts"] != 39
    ):
        raise SoakEvidenceError("P5 aggregate is not production-qualified evidence")
    run = verified["run_context"]
    binding = report.get("binding")
    if not isinstance(binding, dict) or set(binding) != {
        "release_request_digest", "qualification_session_id", "candidate_commit",
        "candidate_tree", "candidate_semantic_digest", "linux_artifact_tuple_digest",
        "toolchain_digest", "runner_bundle_manifest_digest", "agent_binary_digest",
        "agent_signature_digest", "registry_root", "profile_digest",
        "trust_policy_digest",
    }:
        raise SoakEvidenceError("P5 aggregate binding is missing")
    expected_p5 = {
        "release_request_digest": run["release_request_digest"],
        "qualification_session_id": run["qualification_session_id"],
        "candidate_commit": run["candidate_commit"],
        "candidate_tree": run["candidate_tree"],
        "profile_digest": blake3.blake3(_canonical_json(p5_profile)).hexdigest(),
        "trust_policy_digest": p5_profile["trust_policy"]["digest_hex"],
    }
    for field, expected in expected_p5.items():
        if binding.get(field) != expected:
            raise SoakEvidenceError(f"P5 aggregate {field} mismatch")
    child_receipts = report["child_receipts"]
    if not isinstance(child_receipts, list) or len(child_receipts) != 39:
        raise SoakEvidenceError("P5 aggregate child set is incomplete")
    p5_roles = {
        row["role"]: row
        for row in p5_profile["trust_policy"]["policy"]["role_bindings"]
    }
    p5_faults = p5_profile["fault_matrix"]
    seen_faults: dict[str, set[str]] = {}
    for receipt in child_receipts:
        if not isinstance(receipt, dict) or set(receipt) != {
            "format",
            "evidence_tier",
            "payload",
            "signer_public_key",
            "signer_fingerprint",
            "signature",
        }:
            raise SoakEvidenceError("P5 child receipt has unknown or missing fields")
        if (
            receipt["format"] != p5_profile["child_receipt"]["format"]
            or receipt["evidence_tier"] != "production-reference"
        ):
            raise SoakEvidenceError("P5 child receipt identity mismatch")
        payload = receipt.get("payload") if isinstance(receipt, dict) else None
        if not isinstance(payload, dict) or set(payload) != set(
            p5_profile["child_receipt"]["required_bindings"]
        ):
            raise SoakEvidenceError("P5 child payload has unknown or missing fields")
        for field, expected in binding.items():
            if payload.get(field) != expected:
                raise SoakEvidenceError(f"P5 child {field} mismatch")
        role = p5_roles.get(payload.get("role"))
        if role is None or receipt.get("signer_public_key") != role["public_key_hex"] or receipt.get("signer_fingerprint") != role["fingerprint_hex"]:
            raise SoakEvidenceError("P5 child signer is not role-bound")
        host_id = payload.get("physical_host_id")
        if payload.get("role") != f"p5-host:{host_id}":
            raise SoakEvidenceError("P5 child role/host mismatch")
        fault_id = payload.get("fault_id")
        if fault_id not in p5_faults:
            raise SoakEvidenceError("P5 child fault is outside the frozen matrix")
        if (
            payload.get("command") != "observe-host-fault"
            or payload.get("command_sequence") != p5_faults.index(fault_id) + 1
            or payload.get("result") != "pass"
        ):
            raise SoakEvidenceError("P5 child command, sequence, or result mismatch")
        roots_required = set(p5_profile["child_receipt"]["required_root_fields"])
        for root_set_name in ("before_roots", "after_roots"):
            root_set = payload.get(root_set_name)
            if not isinstance(root_set, dict) or set(root_set) != roots_required:
                raise SoakEvidenceError("P5 child root set is incomplete")
            for root_name, root_value in root_set.items():
                _hex(root_value, 32, f"P5 {root_name}")
        observation = payload.get("resource_observation")
        resource_fields = {
            "peak_rss_bytes": "max_peak_rss_bytes_per_host",
            "durable_growth_bytes": "max_durable_growth_bytes_per_host",
            "task_count": "max_task_count_per_host",
            "active_sessions": "max_active_sessions_per_host",
            "fault_duration_ms": "max_fault_duration_ms",
            "reunion_ms": "max_reunion_ms",
            "quiescence_ms": "max_quiescence_ms",
        }
        if not isinstance(observation, dict) or set(observation) != set(resource_fields):
            raise SoakEvidenceError("P5 child resource observation is incomplete")
        for field, bound in resource_fields.items():
            value = observation[field]
            if (
                isinstance(value, bool)
                or not isinstance(value, int)
                or value < 0
                or value > p5_profile["resource_bounds"][bound]
            ):
                raise SoakEvidenceError(f"P5 child resource bound failed: {field}")
        if not isinstance(payload.get("limitations"), list) or not all(
            isinstance(value, str) and value for value in payload["limitations"]
        ):
            raise SoakEvidenceError("P5 child limitations are invalid")
        seen_faults.setdefault(str(host_id), set()).add(str(fault_id))
        child_message = _domain(
            str(p5_profile["child_receipt"]["signature_domain"])
        ) + blake3.blake3(_canonical_json(payload)).digest()
        try:
            Ed25519PublicKey.from_public_bytes(
                bytes.fromhex(str(receipt["signer_public_key"]))
            ).verify(bytes.fromhex(str(receipt["signature"])), child_message)
        except (KeyError, ValueError, InvalidSignature) as error:
            raise SoakEvidenceError("P5 child signature is invalid") from error
    if set(seen_faults) != {"host-a", "host-b", "host-c"} or any(
        faults != set(p5_faults) for faults in seen_faults.values()
    ):
        raise SoakEvidenceError("P5 child fault coverage is incomplete")
    ordered = sorted(
        child_receipts,
        key=lambda row: (
            row["payload"]["physical_host_id"],
            p5_faults.index(row["payload"]["fault_id"]),
            row["payload"]["command_sequence"],
        ),
    )
    root_digest = blake3.blake3()
    root_digest.update(_domain(str(p5_profile["aggregate"]["root_domain"])))
    for receipt in ordered:
        root_digest.update(_canonical_json(receipt))
    p5_root = root_digest.hexdigest()
    if report["aggregate_root"] != p5_root:
        raise SoakEvidenceError("P5 aggregate child root mismatch")
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
    orchestrator = p5_roles["p5-orchestrator"]
    if (
        report["aggregate_signer_public_key"] != orchestrator["public_key_hex"]
        or report["aggregate_signer_fingerprint"] != orchestrator["fingerprint_hex"]
    ):
        raise SoakEvidenceError("P5 aggregate signer is not allowlisted")
    message = _domain(str(p5_profile["aggregate"]["signature_domain"])) + blake3.blake3(
        _canonical_json(unsigned)
    ).digest()
    try:
        Ed25519PublicKey.from_public_bytes(
            bytes.fromhex(orchestrator["public_key_hex"])
        ).verify(bytes.fromhex(str(report["aggregate_signature"])), message)
    except (ValueError, InvalidSignature) as error:
        raise SoakEvidenceError("P5 aggregate signature is invalid") from error
    for field in (
        "candidate_semantic_digest", "linux_artifact_tuple_digest", "toolchain_digest",
        "runner_bundle_manifest_digest", "agent_binary_digest", "agent_signature_digest",
        "registry_root",
    ):
        _hex(binding[field], 32, f"P5 aggregate {field}")
    return p5_root, {str(key): str(value) for key, value in binding.items()}


def _load_p5_v2_controller():
    path = ROOT / "scripts/runner/onebrain-p5-multi-host-v2.py"
    spec = importlib.util.spec_from_file_location("onebrain_p5_multi_host_v2_verify", path)
    if spec is None or spec.loader is None:
        raise SoakEvidenceError("P5 V2 controller implementation is unavailable")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def _load_p5_v1_bundle_verifier():
    path = ROOT / "scripts/runner/onebrain-p5-multi-host.py"
    spec = importlib.util.spec_from_file_location("onebrain_p5_multi_host_v1_bundle_verify", path)
    if spec is None or spec.loader is None:
        raise SoakEvidenceError("P5 native bundle verifier is unavailable")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def _raw_evidence_manifest(root: Path) -> tuple[str, int]:
    if not root.is_dir() or root.is_symlink():
        raise SoakEvidenceError("P5 raw evidence root must be a real directory")
    rows: list[dict[str, object]] = []
    for path in sorted(root.rglob("*")):
        if path.is_dir():
            continue
        metadata = path.lstat()
        if path.is_symlink() or not stat.S_ISREG(metadata.st_mode):
            raise SoakEvidenceError("P5 raw evidence contains a non-regular entry")
        encoded = path.read_bytes()
        if len(encoded) > 1_048_576 or len(rows) >= 4096:
            raise SoakEvidenceError("P5 raw evidence exceeds its fixed bounds")
        rows.append({
            "path": path.relative_to(root).as_posix(),
            "bytes": len(encoded),
            "blake3": blake3.blake3(encoded).hexdigest(),
        })
    return blake3.blake3(_canonical_json(rows)).hexdigest(), len(rows)


def _load_canonical_p5_v2_aggregate(
    aggregate_path: Path,
    controller: object,
) -> dict[str, object]:
    encoded = aggregate_path.read_bytes()
    if not encoded or len(encoded) > 4_194_304:
        raise SoakEvidenceError("P5 V2 aggregate is empty or exceeds its bound")
    try:
        aggregate = json.loads(encoded)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise SoakEvidenceError("P5 V2 aggregate is invalid JSON") from error
    if not isinstance(aggregate, dict) or aggregate.get("format") != 2:
        raise SoakEvidenceError("P5 V2 aggregate format mismatch")
    canonical_json = getattr(controller, "canonical_json", None)
    if not callable(canonical_json):
        raise SoakEvidenceError("P5 V2 canonicalizer is unavailable")
    if encoded not in {canonical_json(aggregate), canonical_json(aggregate) + b"\n"}:
        raise SoakEvidenceError("P5 V2 aggregate is not canonical")
    return aggregate


def _verify_p5_v2_raw_binding(
    aggregate: dict[str, object],
    raw_evidence_root: Path,
) -> tuple[str, int]:
    raw_manifest, raw_count = _raw_evidence_manifest(raw_evidence_root)
    if (
        aggregate.get("raw_manifest_blake3") != raw_manifest
        or aggregate.get("raw_object_count") != raw_count
    ):
        raise SoakEvidenceError("P5 V2 raw evidence manifest mismatch")
    return raw_manifest, raw_count


def _verify_p5_aggregate_v2(
    *,
    release_request: Path,
    release_signature: Path,
    base_policy: Path,
    base_gpg_home: Path,
    p5_request: Path,
    p5_signature: Path,
    p5_approval_policy: Path,
    inventory: Path,
    raw_evidence_root: Path,
    aggregate_path: Path,
    executable: Path,
    bundle_root: Path,
    registry_candidate_root: Path,
) -> dict[str, object]:
    from scripts.release.verify_base_release_request import (
        ReleaseRequestError,
        verify_task28_release_request,
    )
    try:
        verified_base = verify_task28_release_request(
            release_request,
            release_signature,
            base_policy,
            gpg_home=base_gpg_home,
            gpg_executable=Path("/usr/bin/gpg"),
            candidate_root=ROOT,
        )
    except ReleaseRequestError as error:
        raise SoakEvidenceError(f"Base release request is invalid: {error}") from error
    controller = _load_p5_v2_controller()
    try:
        request = controller.verify_p5_request(p5_request, p5_signature, p5_approval_policy, inventory)
    except controller.P5ExecutionError as error:
        raise SoakEvidenceError(str(error)) from error
    base_request = verified_base.request
    release_bytes = release_request.read_bytes()
    inventory_bytes = inventory.read_bytes()
    try:
        inv = json.loads(inventory_bytes)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise SoakEvidenceError("P5 V2 inventory is invalid JSON") from error
    if inventory_bytes not in {
        controller.canonical_json(inv),
        controller.canonical_json(inv) + b"\n",
    }:
        raise SoakEvidenceError("P5 V2 inventory is not canonical")
    if request.get("release_request_blake3") != blake3.blake3(release_bytes).hexdigest():
        raise SoakEvidenceError("P5 V2 request differs from the Base request bytes")
    try:
        policy_value = json.loads(p5_approval_policy.read_bytes())
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise SoakEvidenceError("P5 V2 approval policy is invalid JSON") from error
    if request.get("p5_approval_policy_blake3") != blake3.blake3(
        controller.canonical_json(policy_value)
    ).hexdigest():
        raise SoakEvidenceError("P5 V2 request approval policy binding mismatch")
    profile_path = ROOT / "docs/specs/vnext/P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V2.md"
    vector_path = ROOT / "src/test-vectors/vnext/p5-multi-host-production-qualification-v2.json"
    if (
        request.get("profile_blake3") != blake3.blake3(profile_path.read_bytes()).hexdigest()
        or request.get("vector_blake3") != blake3.blake3(vector_path.read_bytes()).hexdigest()
    ):
        raise SoakEvidenceError("P5 V2 request profile/vector differs from candidate bytes")
    now = int(time.time())
    if (
        isinstance(request.get("issued_at"), bool)
        or not isinstance(request.get("issued_at"), int)
        or isinstance(request.get("expires_at"), bool)
        or not isinstance(request.get("expires_at"), int)
    ):
        raise SoakEvidenceError("P5 V2 request validity interval is invalid")
    if not request["issued_at"] <= now < request["expires_at"]:
        raise SoakEvidenceError("P5 V2 request is outside its validity interval")
    try:
        base_created = int(
            datetime.fromisoformat(str(base_request["created_utc"]).replace("Z", "+00:00")).timestamp()
        )
        base_expires = int(
            datetime.fromisoformat(str(base_request["expires_utc"]).replace("Z", "+00:00")).timestamp()
        )
    except (KeyError, TypeError, ValueError, OverflowError) as error:
        raise SoakEvidenceError("Base request validity interval is invalid") from error
    if not base_created <= request["issued_at"] < request["expires_at"] <= base_expires:
        raise SoakEvidenceError("P5 V2 request is not nested in the Base request interval")

    bundle = bundle_root.resolve(strict=True)
    manifest_path = bundle / "metadata/bundle.manifest.json"
    manifest_bytes = manifest_path.read_bytes()
    try:
        manifest = json.loads(manifest_bytes)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise SoakEvidenceError("P5 V2 bundle manifest is invalid JSON") from error
    if manifest_bytes != controller.canonical_json(manifest):
        raise SoakEvidenceError("P5 V2 bundle manifest is not canonical")
    candidate = base_request["candidate"]
    if manifest.get("candidate") != {
        "id": candidate["commit"],
        "source_digest": manifest.get("candidate", {}).get("source_digest")
        if isinstance(manifest.get("candidate"), dict)
        else None,
        "version": candidate["tree"],
    }:
        raise SoakEvidenceError("P5 V2 bundle candidate differs from the Base request")
    bundle_verifier = _load_p5_v1_bundle_verifier()
    try:
        bundle_digest, _ = bundle_verifier._bundle_manifest_binding(
            bundle,
            bundle / "bin/p5_multi_host_agent",
            candidate_commit=str(candidate["commit"]),
            candidate_tree=str(candidate["tree"]),
        )
    except bundle_verifier.P5OrchestrationError as error:
        raise SoakEvidenceError(f"P5 V2 native bundle is invalid: {error}") from error
    if inv.get("bundle_manifest_blake3") != bundle_digest:
        raise SoakEvidenceError("P5 V2 inventory bundle binding mismatch")
    registry_manifest = registry_candidate_root.resolve(strict=True) / "concepts.obr.manifest.json"
    if inv.get("registry_candidate_manifest_blake3") != blake3.blake3(
        registry_manifest.read_bytes()
    ).hexdigest():
        raise SoakEvidenceError("P5 V2 inventory Registry binding mismatch")
    expected_executable = (bundle / "bin/p5_multi_host_agent_v2").resolve(strict=True)
    if executable.resolve(strict=True) != expected_executable or executable.is_symlink():
        raise SoakEvidenceError("P5 V2 executable is not the bundle-owned agent")

    aggregate = _load_canonical_p5_v2_aggregate(aggregate_path, controller)
    if aggregate.get("request_digest") != blake3.blake3(controller.canonical_json(request)).hexdigest():
        raise SoakEvidenceError("P5 V2 aggregate request binding mismatch")
    authority = aggregate.get("evidence_authority")
    if not isinstance(authority, dict) or authority.get("inventory_blake3") != blake3.blake3(controller.canonical_json(inv)).hexdigest():
        raise SoakEvidenceError("P5 V2 evidence authority inventory mismatch")
    if authority.get("provider_evidence_status") != inv.get("provider_evidence_status"):
        raise SoakEvidenceError("P5 V2 provider evidence status mismatch")
    if (
        aggregate.get("session_id") != request.get("session_id")
        or aggregate.get("controller_public_key")
        != inv.get("controller_application_public_key")
    ):
        raise SoakEvidenceError("P5 V2 aggregate session/controller binding mismatch")
    child_receipts = aggregate.get("child_receipts")
    if not isinstance(child_receipts, list) or len(child_receipts) < 3:
        raise SoakEvidenceError("P5 V2 signed child receipt set is incomplete")
    child_hosts: set[str] = set()
    inventory_signers = {
        str(row.get("host_id", row.get("physical_host_id", ""))): str(row.get("receipt_public_key", ""))
        for row in inv.get("hosts", []) if isinstance(row, dict)
    }
    try:
        receipt_verifier = controller._production_receipt_verifier(
            {
                host_id: bytes.fromhex(public_key)
                for host_id, public_key in inventory_signers.items()
            },
            authority,
            str(aggregate["request_digest"]),
        )
    except (KeyError, ValueError) as error:
        raise SoakEvidenceError("P5 V2 inventory child signer set is invalid") from error
    for receipt in child_receipts:
        if not isinstance(receipt, dict) or receipt.get("format") != 2:
            raise SoakEvidenceError("P5 V2 child receipt format mismatch")
        if receipt.get("evidence_authority") != authority:
            raise SoakEvidenceError("P5 V2 child evidence authority mismatch")
        host_id = receipt.get("host_id")
        if host_id not in {"host-a", "host-b", "host-c"}:
            raise SoakEvidenceError("P5 V2 child host is invalid")
        if receipt.get("request_digest") != aggregate.get("request_digest"):
            raise SoakEvidenceError("P5 V2 child request binding mismatch")
        if receipt.get("inventory_blake3") != authority.get("inventory_blake3"):
            raise SoakEvidenceError("P5 V2 child inventory binding mismatch")
        issued_at = receipt.get("issued_at")
        if (
            isinstance(issued_at, bool)
            or not isinstance(issued_at, int)
            or not request["issued_at"] <= issued_at < request["expires_at"]
        ):
            raise SoakEvidenceError("P5 V2 child receipt lies outside its request interval")
        try:
            receipt_verifier(str(host_id), controller.canonical_json(receipt))
        except controller.P5ExecutionError as error:
            raise SoakEvidenceError(f"P5 V2 child receipt is invalid: {error}") from error
        child_hosts.add(str(host_id))
    if child_hosts != {"host-a", "host-b", "host-c"}:
        raise SoakEvidenceError("P5 V2 child host coverage is incomplete")
    qualification = controller.derive_qualification(aggregate)
    if aggregate.get("qualification") != qualification or not qualification["multi_host_qualified"]:
        raise SoakEvidenceError("P5 V2 qualification is not derived production evidence")
    raw_manifest, raw_count = _verify_p5_v2_raw_binding(
        aggregate, raw_evidence_root
    )
    unsigned_aggregate = {
        key: value for key, value in aggregate.items()
        if key not in {"controller_signature", "aggregate_blake3"}
    }
    aggregate_blake3 = blake3.blake3(controller.canonical_json(unsigned_aggregate)).hexdigest()
    if aggregate.get("aggregate_blake3") != aggregate_blake3:
        raise SoakEvidenceError("P5 V2 aggregate digest mismatch")
    try:
        Ed25519PublicKey.from_public_bytes(bytes.fromhex(str(aggregate["controller_public_key"]))).verify(
            bytes.fromhex(str(aggregate["controller_signature"])),
            b"onebrain/p5/multi-host-aggregate/v2\0" + bytes.fromhex(aggregate_blake3),
        )
    except (KeyError, ValueError, InvalidSignature) as error:
        raise SoakEvidenceError("P5 V2 aggregate signature is invalid") from error
    return {
        "format": 2,
        "request_digest": aggregate["request_digest"],
        "evidence_authority": authority,
        "session_id": request["session_id"],
        "aggregate_blake3": aggregate_blake3,
        "raw_manifest_blake3": raw_manifest,
        "verified_child_receipts": len(child_receipts),
        "verified_raw_objects": raw_count,
        "multi_host_qualified": True,
        "limitations": aggregate.get("limitations", []),
        "verifier_implementation_blake3": blake3.blake3(Path(__file__).read_bytes()).hexdigest(),
        "verified_at": int(time.time()),
    }


def _verified_binding(
    *,
    request: Path,
    signature: Path,
    policy: Path,
    gpg_home: Path,
    registry_aggregate: Path,
    registry_binding: Path,
    p5_request: Path,
    p5_signature: Path,
    p5_approval_policy: Path,
    p5_inventory: Path,
    p5_raw_evidence_root: Path,
    p5_aggregate: Path,
    p5_executable: Path,
    p5_bundle_root: Path,
    p5_registry_candidate_root: Path,
    executable: Path,
    sbom: Path,
    provenance: Path,
    runner_image_evidence: Path,
) -> dict[str, str]:
    from scripts.release.verify_base_release_request import (
        ReleaseRequestError,
        load_task28_registry_measurement_context,
        verify_task28_release_request,
    )

    try:
        verified_context = verify_task28_release_request(
            request,
            signature,
            policy,
            gpg_home=gpg_home,
            gpg_executable=Path("/usr/bin/gpg"),
            candidate_root=ROOT,
        )
        registry_context = load_task28_registry_measurement_context(
            verified_context, registry_binding
        )
    except ReleaseRequestError as error:
        raise SoakEvidenceError(f"Base release request is invalid: {error}") from error
    verified = verified_context.as_dict()
    run = verified["run_context"]
    try:
        from scripts.concept_registry.production_qualification import (
            AggregationError,
            PRODUCTION_EQUALITY_BINDINGS,
            _verify_receipt,
        )
    except ImportError as error:
        raise SoakEvidenceError("Registry production verifier is unavailable") from error
    try:
        profile = json.loads(
            (
                ROOT
                / "src/test-vectors/vnext/concept-registry-production-qualification-v1.json"
            ).read_text(encoding="utf-8")
        )
        registry_receipt = json.loads(registry_aggregate.read_bytes())
        policy_value = profile.get("trust_policy", {}).get("policy")
        if not isinstance(policy_value, dict):
            raise SoakEvidenceError("Registry production trust policy is missing")
        kind, registry_payload = _verify_receipt(
            registry_receipt, profile, policy_value
        )
    except (OSError, UnicodeDecodeError, json.JSONDecodeError, AggregationError) as error:
        raise SoakEvidenceError(f"Registry production aggregate is invalid: {error}") from error
    if (
        kind != "production-aggregate"
        or registry_payload.get("registry_production_qualified") is not True
        or registry_payload.get("base_candidate_bound") is not True
        or registry_payload.get("result") is not True
        or registry_payload.get("evidence_tier") != "production-reference"
    ):
        raise SoakEvidenceError("Registry production aggregate does not qualify the candidate")
    for field in ("release_request_digest", "qualification_session_id", "candidate_commit", "candidate_tree"):
        if registry_payload.get(field) != run[field]:
            raise SoakEvidenceError(f"Registry production aggregate {field} mismatch")
    for field in PRODUCTION_EQUALITY_BINDINGS:
        if registry_payload.get(field) != registry_context.bindings.get(field):
            raise SoakEvidenceError(f"Registry production aggregate {field} mismatch")
    p5_verified = _verify_p5_aggregate_v2(
        release_request=request,
        release_signature=signature,
        base_policy=policy,
        base_gpg_home=gpg_home,
        p5_request=p5_request,
        p5_signature=p5_signature,
        p5_approval_policy=p5_approval_policy,
        inventory=p5_inventory,
        raw_evidence_root=p5_raw_evidence_root,
        aggregate_path=p5_aggregate,
        executable=p5_executable,
        bundle_root=p5_bundle_root,
        registry_candidate_root=p5_registry_candidate_root,
    )
    p5_registry_manifest = (
        p5_registry_candidate_root.resolve(strict=True) / "concepts.obr.manifest.json"
    )
    if blake3.blake3(p5_registry_manifest.read_bytes()).hexdigest() != registry_context.bindings[
        "candidate_payload_artifacts_blake3"
    ]["MANIFEST:concepts.obr.manifest.json"]:
        raise SoakEvidenceError("P5 V2 and Registry aggregate candidate manifests differ")
    executable_blake3 = blake3.blake3(executable.read_bytes()).hexdigest()
    sbom_blake3 = blake3.blake3(sbom.read_bytes()).hexdigest()
    provenance_blake3 = blake3.blake3(provenance.read_bytes()).hexdigest()
    runner_image_digest = blake3.blake3(runner_image_evidence.read_bytes()).hexdigest()
    return {
        "release_request_digest": run["release_request_digest"],
        "qualification_session_id": run["qualification_session_id"],
        "candidate_commit": run["candidate_commit"],
        "candidate_tree": run["candidate_tree"],
        "candidate_semantic_digest": registry_context.bindings["candidate_semantic_digest"],
        "frozen_target_artifact_digest": registry_context.bindings["artifact_tuple_digest"],
        "registry_root": registry_payload["release_aggregate_root"],
        "p5_aggregate_root": p5_verified["aggregate_blake3"],
        "executable_blake3": executable_blake3,
        "sbom_blake3": sbom_blake3,
        "provenance_blake3": provenance_blake3,
        "runner_image_digest": runner_image_digest,
        "trust_policy_digest": _profile()["trust_policy"]["digest_hex"],
    }


def _verified_prebuilt_binding(
    *,
    request: Path,
    signature: Path,
    policy: Path,
    gpg_home: Path,
    registry_binding: Path,
    registry_prebuilt_root: Path,
    candidate_semantic_evidence: Path,
    p5_request: Path,
    p5_signature: Path,
    p5_approval_policy: Path,
    p5_inventory: Path,
    p5_raw_evidence_root: Path,
    p5_aggregate: Path,
    p5_executable: Path,
    p5_bundle_root: Path,
    executable: Path,
    sbom: Path,
    provenance: Path,
    runner_image_evidence: Path,
) -> dict[str, str]:
    """Bind fresh P5/soak evidence to owner-produced final Registry bytes."""

    from scripts.release.task28_prebuilt_registry import (
        PrebuiltRegistryError,
        verify_prebuilt_registry_binding,
    )
    from scripts.release.verify_base_release_request import (
        ReleaseRequestError,
        verify_task28_release_request,
    )

    try:
        verified_context = verify_task28_release_request(
            request,
            signature,
            policy,
            gpg_home=gpg_home,
            gpg_executable=Path("/usr/bin/gpg"),
            candidate_root=ROOT,
        )
        registry_bytes = registry_binding.read_bytes()
        registry_receipt = json.loads(registry_bytes)
        if registry_bytes not in {
            _canonical_json(registry_receipt),
            _canonical_json(registry_receipt) + b"\n",
        }:
            raise SoakEvidenceError("prebuilt Registry binding is not canonical")
        registry_payload = verify_prebuilt_registry_binding(
            verified_context,
            registry_prebuilt_root,
            candidate_semantic_evidence,
            registry_receipt,
        )
    except (ReleaseRequestError, PrebuiltRegistryError) as error:
        raise SoakEvidenceError(f"prebuilt Registry binding is invalid: {error}") from error

    p5_verified = _verify_p5_aggregate_v2(
        release_request=request,
        release_signature=signature,
        base_policy=policy,
        base_gpg_home=gpg_home,
        p5_request=p5_request,
        p5_signature=p5_signature,
        p5_approval_policy=p5_approval_policy,
        inventory=p5_inventory,
        raw_evidence_root=p5_raw_evidence_root,
        aggregate_path=p5_aggregate,
        executable=p5_executable,
        bundle_root=p5_bundle_root,
        registry_candidate_root=registry_prebuilt_root,
    )
    manifest = registry_prebuilt_root.resolve(strict=True) / "concepts.obr.manifest.json"
    if blake3.blake3(manifest.read_bytes()).hexdigest() != registry_payload[
        "candidate_payload_artifacts_blake3"
    ]["MANIFEST:concepts.obr.manifest.json"]:
        raise SoakEvidenceError("P5 V2 and prebuilt Registry manifests differ")
    return {
        "release_request_digest": str(registry_payload["release_request_digest"]),
        "qualification_session_id": str(registry_payload["qualification_session_id"]),
        "candidate_commit": str(registry_payload["candidate_commit"]),
        "candidate_tree": str(registry_payload["candidate_tree"]),
        "candidate_semantic_digest": str(registry_payload["candidate_semantic_digest"]),
        "frozen_target_artifact_digest": str(registry_payload["artifact_tuple_digest"]),
        "registry_root": str(registry_payload["release_aggregate_root"]),
        "p5_aggregate_root": str(p5_verified["aggregate_blake3"]),
        "executable_blake3": blake3.blake3(executable.read_bytes()).hexdigest(),
        "sbom_blake3": blake3.blake3(sbom.read_bytes()).hexdigest(),
        "provenance_blake3": blake3.blake3(provenance.read_bytes()).hexdigest(),
        "runner_image_digest": blake3.blake3(runner_image_evidence.read_bytes()).hexdigest(),
        "trust_policy_digest": _profile()["trust_policy"]["digest_hex"],
    }


def sign_soak_child_receipt(
    *,
    binding: dict[str, str],
    runner_id: str,
    runner_identity: str,
    monotonic_start_ns: int,
    monotonic_end_ns: int,
    raw_report_bytes: bytes,
    signing_key: Ed25519PrivateKey,
) -> dict[str, object]:
    profile = _profile()
    _validate_profile(profile, production=True)
    _validate_binding(profile, binding)
    configured = {str(row["runner_id"]) for row in profile["runners"]}
    if runner_id not in configured:
        raise SoakEvidenceError("runner_id is outside the frozen soak profile")
    if not runner_identity:
        raise SoakEvidenceError("runner identity is missing")
    try:
        report = json.loads(raw_report_bytes)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise SoakEvidenceError("raw soak report is not valid JSON") from error
    if report.get("profile") != "onebrain/dr-m5-soak-release/1":
        raise SoakEvidenceError("raw soak report profile mismatch")
    if report.get("run_profile") != "pre-release72h":
        raise SoakEvidenceError("raw soak report is not the pre-release 72-hour profile")
    if report.get("host_os") != "linux" or report.get("host_arch") != "x86_64":
        raise SoakEvidenceError("raw soak report target host differs from the frozen target")
    elapsed = report.get("elapsed_seconds")
    if isinstance(elapsed, bool) or not isinstance(elapsed, int):
        raise SoakEvidenceError("raw soak elapsed_seconds is invalid")
    counts = {
        "slow-peer": report.get("slow_peer_cycles"),
        "bounded-session-flood": report.get("bounded_flood_cycles"),
        "partition-reunion": report.get("partition_reunion_cycles"),
    }
    semantic = any(
        bool(report.get(field))
        for field in (
            "changes_wallet_state",
            "changes_obt_state",
            "grants_authority",
            "claims_truth",
            "claims_benefit",
            "claims_network_completion",
        )
    )
    if (
        report.get("qualification_met") is not True
        or report.get("fair_redelivery_oracle_matches") is not True
        or report.get("task_leak_detected") is not False
        or report.get("rollback_recommended") is not False
        or report.get("rollback_reasons") != []
    ):
        raise SoakEvidenceError("raw soak report fails a duration, fault, leak, or rollback gate")
    result = {
        "status": "pass" if report.get("pre_release_qualified") is True else "fail",
        "raw_report_blake3": blake3.blake3(raw_report_bytes).hexdigest(),
        "elapsed_seconds": elapsed,
        "pre_release_qualified": report.get("pre_release_qualified") is True,
        "fault_cycle_counts": counts,
        "rollback_recommended": report.get("rollback_recommended"),
        "active_sessions_after_shutdown": report.get("active_sessions_after_shutdown"),
        "semantic_amplification": semantic,
    }
    payload = {
        **binding,
        "role": f"soak-runner:{runner_id}",
        "runner_id": runner_id,
        "runner_identity": runner_identity,
        "interval_sequence": 1,
        "receipt_kind": "interval",
        "monotonic_start_ns": monotonic_start_ns,
        "monotonic_end_ns": monotonic_end_ns,
        "command": "dr_m5_soak_release --profile pre-release-72h",
        "result": result,
        "limitations": [],
    }
    public = signing_key.public_key().public_bytes_raw()
    role = _roles(profile)[f"soak-runner:{runner_id}"]
    if public.hex() != role["public_key_hex"] or _fingerprint(public, profile) != role["fingerprint_hex"]:
        raise SoakEvidenceError("runner signing key is not allowlisted for its role")
    receipt = {
        "format": profile["child_receipt"]["format"],
        "evidence_tier": "production-reference",
        "payload": payload,
        "signer_public_key": public.hex(),
        "signer_fingerprint": role["fingerprint_hex"],
        "signature": signing_key.sign(_child_signature_message(profile, payload)).hex(),
    }
    _validate_child(receipt, profile, binding, "production-reference")
    minimum_ns = profile["scope"]["minimum_uninterrupted_elapsed_seconds"] * 1_000_000_000
    if monotonic_end_ns - monotonic_start_ns < minimum_ns or elapsed < profile["scope"]["minimum_uninterrupted_elapsed_seconds"]:
        raise SoakEvidenceError("raw soak does not contain 72 uninterrupted hours")
    if result["status"] != "pass" or semantic:
        raise SoakEvidenceError("raw soak report fails the release or semantic gate")
    return receipt


def _write_atomic(path: Path, value: object) -> None:
    encoded = _canonical_json(value) + b"\n"
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".new")
    try:
        with temporary.open("xb") as stream:
            stream.write(encoded)
            stream.flush()
            os.fsync(stream.fileno())
        try:
            os.link(temporary, path)
        except FileExistsError as error:
            raise SoakEvidenceError("evidence output already exists") from error
    finally:
        temporary.unlink(missing_ok=True)


def _add_verified_binding_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--request", type=Path, required=True)
    parser.add_argument("--signature", type=Path, required=True)
    parser.add_argument("--policy", type=Path, required=True)
    parser.add_argument("--gpg-home", type=Path, required=True)
    parser.add_argument("--registry-aggregate", type=Path)
    parser.add_argument("--registry-binding", type=Path, required=True)
    parser.add_argument("--registry-prebuilt-root", type=Path)
    parser.add_argument("--candidate-semantic-evidence", type=Path)
    parser.add_argument("--p5-request", type=Path, required=True)
    parser.add_argument("--p5-signature", type=Path, required=True)
    parser.add_argument("--p5-approval-policy", type=Path, required=True)
    parser.add_argument("--p5-inventory", type=Path, required=True)
    parser.add_argument("--p5-raw-evidence-root", type=Path, required=True)
    parser.add_argument("--p5-aggregate", type=Path, required=True)
    parser.add_argument("--p5-executable", type=Path, required=True)
    parser.add_argument("--p5-bundle-root", type=Path, required=True)
    parser.add_argument("--p5-registry-candidate-root", type=Path, required=True)
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--sbom", type=Path, required=True)
    parser.add_argument("--provenance", type=Path, required=True)
    parser.add_argument("--runner-image-evidence", type=Path, required=True)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    carry = subparsers.add_parser("carry-forward")
    carry.add_argument("--evidence", type=Path, required=True)
    carry.add_argument("--candidate-identity", type=Path, required=True)
    carry.add_argument("--changed-path", action="append", default=[])
    carry.add_argument("--output", type=Path, required=True)
    child = subparsers.add_parser("sign-child")
    _add_verified_binding_arguments(child)
    child.add_argument("--runner-id", required=True)
    child.add_argument("--runner-identity", required=True)
    child.add_argument("--monotonic-start-file", type=Path, required=True)
    child.add_argument("--monotonic-end-file", type=Path, required=True)
    child.add_argument("--raw-report", type=Path, required=True)
    child.add_argument("--signing-key", type=Path, required=True)
    child.add_argument("--output", type=Path, required=True)
    aggregate = subparsers.add_parser("aggregate-soak")
    _add_verified_binding_arguments(aggregate)
    aggregate.add_argument("--receipts-root", type=Path, required=True)
    aggregate.add_argument("--signing-key", type=Path, required=True)
    aggregate.add_argument("--output", type=Path, required=True)
    verify_p5 = subparsers.add_parser("verify-p5")
    for name in (
        "release-request", "release-signature", "base-policy", "base-gpg-home",
        "p5-request", "p5-signature", "p5-approval-policy", "inventory",
        "raw-evidence-root", "p5-aggregate", "executable", "bundle-root",
        "registry-candidate-root", "output",
    ):
        verify_p5.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        if args.command == "carry-forward":
            result = analyze_evidence_carry_forward(
                json.loads(args.evidence.read_text(encoding="utf-8")),
                json.loads(args.candidate_identity.read_text(encoding="utf-8")),
                args.changed_path,
            )
        elif args.command == "verify-p5":
            result = _verify_p5_aggregate_v2(
                release_request=args.release_request,
                release_signature=args.release_signature,
                base_policy=args.base_policy,
                base_gpg_home=args.base_gpg_home,
                p5_request=args.p5_request,
                p5_signature=args.p5_signature,
                p5_approval_policy=args.p5_approval_policy,
                inventory=args.inventory,
                raw_evidence_root=args.raw_evidence_root,
                aggregate_path=args.p5_aggregate,
                executable=args.executable,
                bundle_root=args.bundle_root,
                registry_candidate_root=args.registry_candidate_root,
            )
        else:
            if args.registry_prebuilt_root is not None:
                if args.registry_aggregate is not None:
                    raise SoakEvidenceError(
                        "prebuilt Registry mode rejects a fresh Registry aggregate"
                    )
                if (
                    args.p5_registry_candidate_root.resolve(strict=True)
                    != args.registry_prebuilt_root.resolve(strict=True)
                ):
                    raise SoakEvidenceError(
                        "P5 and soak must use the same prebuilt Registry root"
                    )
                if args.candidate_semantic_evidence is None:
                    raise SoakEvidenceError(
                        "prebuilt Registry mode requires --candidate-semantic-evidence"
                    )
                binding = _verified_prebuilt_binding(
                    request=args.request,
                    signature=args.signature,
                    policy=args.policy,
                    gpg_home=args.gpg_home,
                    registry_binding=args.registry_binding,
                    registry_prebuilt_root=args.registry_prebuilt_root,
                    candidate_semantic_evidence=args.candidate_semantic_evidence,
                    p5_request=args.p5_request,
                    p5_signature=args.p5_signature,
                    p5_approval_policy=args.p5_approval_policy,
                    p5_inventory=args.p5_inventory,
                    p5_raw_evidence_root=args.p5_raw_evidence_root,
                    p5_aggregate=args.p5_aggregate,
                    p5_executable=args.p5_executable,
                    p5_bundle_root=args.p5_bundle_root,
                    executable=args.executable,
                    sbom=args.sbom,
                    provenance=args.provenance,
                    runner_image_evidence=args.runner_image_evidence,
                )
            else:
                if args.candidate_semantic_evidence is not None:
                    raise SoakEvidenceError(
                        "fresh Registry mode rejects --candidate-semantic-evidence"
                    )
                if args.registry_aggregate is None:
                    raise SoakEvidenceError(
                        "fresh Registry mode requires --registry-aggregate"
                    )
                binding = _verified_binding(
                    request=args.request,
                    signature=args.signature,
                    policy=args.policy,
                    gpg_home=args.gpg_home,
                    registry_aggregate=args.registry_aggregate,
                    registry_binding=args.registry_binding,
                    p5_request=args.p5_request,
                    p5_signature=args.p5_signature,
                    p5_approval_policy=args.p5_approval_policy,
                    p5_inventory=args.p5_inventory,
                    p5_raw_evidence_root=args.p5_raw_evidence_root,
                    p5_aggregate=args.p5_aggregate,
                    p5_executable=args.p5_executable,
                    p5_bundle_root=args.p5_bundle_root,
                    p5_registry_candidate_root=args.p5_registry_candidate_root,
                    executable=args.executable,
                    sbom=args.sbom,
                    provenance=args.provenance,
                    runner_image_evidence=args.runner_image_evidence,
                )
            if args.command == "sign-child":
                result = sign_soak_child_receipt(
                    binding=binding,
                    runner_id=args.runner_id,
                    runner_identity=args.runner_identity,
                    monotonic_start_ns=int(
                        args.monotonic_start_file.read_text(encoding="ascii").strip()
                    ),
                    monotonic_end_ns=int(
                        args.monotonic_end_file.read_text(encoding="ascii").strip()
                    ),
                    raw_report_bytes=args.raw_report.read_bytes(),
                    signing_key=_read_private_key(args.signing_key),
                )
            else:
                receipt_paths = sorted(
                    args.receipts_root.rglob("base-v1-soak-child-receipt.json")
                )
                receipts = [
                    json.loads(path.read_text(encoding="utf-8"))
                    for path in receipt_paths
                ]
                result = _aggregate_soak_receipts(
                    profile=_profile(),
                    binding=binding,
                    receipts=receipts,
                    aggregator_key=_read_private_key(args.signing_key),
                    production=True,
                )
        _write_atomic(args.output, result)
    except (OSError, ValueError, EvidenceCarryForwardError, SoakEvidenceError) as error:
        print(f"Base v1 evidence validation failed: {error}", file=__import__("sys").stderr)
        return 1
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
