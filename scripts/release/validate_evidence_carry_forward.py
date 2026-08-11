#!/usr/bin/env python3
"""Fail-closed Base v1 soak carry-forward and signed-receipt validator.

Carry-forward is deliberately analytical only.  Base v1 always requires a
fresh uninterrupted 72-hour run on the exact Task 27 candidate in Task 28.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
from typing import Iterable

import blake3
from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives.asymmetric.ed25519 import (
    Ed25519PrivateKey,
    Ed25519PublicKey,
)


ROOT = Path(__file__).resolve().parents[2]
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


def _verify_p5_aggregate(
    aggregate_path: Path,
    verified: dict[str, object],
) -> str:
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
    release = verified["bindings"]
    binding = report.get("binding")
    if not isinstance(binding, dict):
        raise SoakEvidenceError("P5 aggregate binding is missing")
    expected_p5 = {
        "release_request_digest": run["release_request_digest"],
        "qualification_session_id": run["qualification_session_id"],
        "candidate_commit": run["candidate_commit"],
        "candidate_tree": run["candidate_tree"],
        "candidate_semantic_digest": release["candidate_semantic_digest"],
        "linux_artifact_tuple_digest": release["artifact_tuple_digest"],
        "registry_root": release["release_aggregate_root"],
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
        for field, expected in expected_p5.items():
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
    return p5_root


def _verified_binding(
    *,
    request: Path,
    signature: Path,
    policy: Path,
    gpg_home: Path,
    p5_aggregate: Path,
    executable: Path,
) -> dict[str, str]:
    from scripts.release.verify_base_release_request import (
        ReleaseRequestError,
        verify_release_request,
    )

    try:
        verified = verify_release_request(request, signature, policy, gpg_home).as_dict()
    except ReleaseRequestError as error:
        raise SoakEvidenceError(f"Base release request is invalid: {error}") from error
    run = verified["run_context"]
    release = verified["bindings"]
    p5_aggregate_root = _verify_p5_aggregate(p5_aggregate, verified)
    executable_blake3 = blake3.blake3(executable.read_bytes()).hexdigest()
    sbom_blake3 = release["candidate_payload_artifacts_blake3"][
        "SPDX_SBOM:sbom.spdx.json"
    ]
    provenance_blake3 = blake3.blake3(
        _canonical_json(verified["tooling_blake3"])
    ).hexdigest()
    return {
        "release_request_digest": run["release_request_digest"],
        "qualification_session_id": run["qualification_session_id"],
        "candidate_commit": run["candidate_commit"],
        "candidate_tree": run["candidate_tree"],
        "candidate_semantic_digest": release["candidate_semantic_digest"],
        "frozen_target_artifact_digest": release["artifact_tuple_digest"],
        "registry_root": release["release_aggregate_root"],
        "p5_aggregate_root": p5_aggregate_root,
        "executable_blake3": executable_blake3,
        "sbom_blake3": sbom_blake3,
        "provenance_blake3": provenance_blake3,
        "runner_image_digest": release["runner_image_digest"],
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
    parser.add_argument("--p5-aggregate", type=Path, required=True)
    parser.add_argument("--executable", type=Path, required=True)


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
    args = parser.parse_args(argv)
    try:
        if args.command == "carry-forward":
            result = analyze_evidence_carry_forward(
                json.loads(args.evidence.read_text(encoding="utf-8")),
                json.loads(args.candidate_identity.read_text(encoding="utf-8")),
                args.changed_path,
            )
        else:
            binding = _verified_binding(
                request=args.request,
                signature=args.signature,
                policy=args.policy,
                gpg_home=args.gpg_home,
                p5_aggregate=args.p5_aggregate,
                executable=args.executable,
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
