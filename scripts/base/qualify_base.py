#!/usr/bin/env python3
"""Pure, fail-closed Base v1 evidence qualifier.

The core routine consumes bytes already supplied by its caller.  It never runs
tests, invokes a shell, accepts a qualification claim, or signs its output.
The detached manifest signature is deliberately a later, outer envelope.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import re
import os
import tempfile
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Mapping
from urllib.parse import urlsplit

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

import blake3
from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey

from scripts.release.verify_base_release_request import (
    ReleaseRequestError,
    canonical_compatibility_tuple_bytes,
)


PROFILE_ID = "BASE_V1_FREEZE_AND_EVIDENCE_PROFILE_V1"
FROZEN_PROFILE_BLAKE3 = "b137ae7f74c35259455c27bcb191338b3a95eda270cd693fc0e342da2f6312aa"
INPUT_FORMAT = "onebrain/base-v1-qualification-input/1"
MANIFEST_FORMAT = "onebrain/base-v1-evidence-manifest/1"
CHILD_FORMAT = "onebrain/base-v1-child-evidence-reference/1"
CHILD_SIGNATURE_DOMAIN = b"onebrain:base-v1:child-evidence-reference:1\0"
EVIDENCE_APPROVAL_FORMAT = "onebrain/base-v1-evidence-receipt-approval/1"
EVIDENCE_APPROVAL_DOMAIN = b"onebrain:base-v1:evidence-receipt-approval:1\0"
GATE_RECEIPT_FORMAT = "onebrain/base-v1-gate-receipt/1"
TARGET_RECEIPT_FORMAT = "onebrain/base-v1-target-receipt/1"
SBOM_FORMAT = "SPDX-2.3"
PROVENANCE_FORMAT = "onebrain/base-v1-target-provenance/1"
TARGETS = {
    "linux": "x86_64-unknown-linux-gnu",
    "windows": "x86_64-pc-windows-msvc",
    "macos": "aarch64-apple-darwin",
}
SIGNED_GATES = {
    "signed-prebuilt-registry",
    "fresh-multi-host-p5",
    "fresh-exact-candidate-72h-soak",
}
NO_CARRY_FORWARD = {
    "signed-prebuilt-registry",
    "fresh-exact-candidate-72h-soak",
}
SECURITY_LANES = {"cargo-audit", "cargo-deny", "npm-audit"}
HEX_32 = re.compile(r"^[0-9a-f]{64}$")
HEX_40 = re.compile(r"^[0-9a-f]{40}$")
HEX_64 = re.compile(r"^[0-9a-f]{128}$")

INPUT_FIELDS = {
    "format",
    "release_request_digest",
    "release_request_created_utc",
    "release_request_expires_utc",
    "qualification_session_id",
    "candidate",
    "compatibility",
    "feature_matrix",
    "target_receipts",
    "gate_evidence",
    "child_signatures",
    "evidence_signatures",
    "roots",
    "documents",
    "limitations",
}
CANDIDATE_FIELDS = {"commit", "tree", "object_format", "semantic_digest"}
COMPATIBILITY_FIELDS = {
    "per_target_tuples",
    "per_target_artifact_digests",
    "schema_digest",
    "domain_registry_digest",
    "resource_registry_digest",
    "storage_schema_version",
    "archive_profile_version",
    "migration_profile_version",
    "registry_profile_version",
    "registry_profile_digest",
    "wire_session_version",
    "product_api_version",
    "c_abi_version",
    "feature_set_digest",
}
FEATURE_FIELDS = {
    "base_default",
    "legacy_default",
    "network_default",
    "network_kill_switch_verified",
}
TARGET_RECEIPT_FIELDS = {
    "os", "target_triple", "receipt_evidence_id", "receipt_blake3",
}
GATE_FIELDS = {
    "gate", "receipt_evidence_id", "receipt_blake3",
}
RECEIPT_BINDING_FIELDS = {
    "release_request_digest", "qualification_session_id", "candidate_commit",
    "candidate_tree", "candidate_semantic_digest", "artifact_tuple_digests",
    "registry_root", "p5_root", "soak_root",
}
CHECK_FIELDS = {
    "name", "command", "command_blake3", "exit_code",
    "stdout_evidence_id", "stdout_blake3", "stderr_evidence_id", "stderr_blake3", "runner",
}
RUNNER_FIELDS = {
    "format", "kind", "identity", "candidate_commit", "candidate_tree",
    "command_blake3", "invocation_id",
}
CHECK_RESULT_FIELDS = {
    "format", "check", "status", "bindings", "assertions", "assertion_root",
}
ASSERTION_FIELDS = {"id", "passed", "evidence_id", "evidence_blake3"}
GATE_CHECK_CONTRACT_FIELDS = {
    "name", "command", "runner_kind", "runner_identity", "required_assertion_ids",
}
TARGET_CHECK_CONTRACT_FIELDS = GATE_CHECK_CONTRACT_FIELDS | {"builder_id"}
GATE_RECEIPT_FIELDS = {
    "format", "gate", "bindings", "fresh", "carry_forward", "checks", "details", "derived_root",
}
TARGET_MACHINE_RECEIPT_FIELDS = {
    "format", "os", "target_triple", "bindings", "artifact_tuple_digest",
    "binary", "sbom", "provenance", "checks",
}
EVIDENCE_REFERENCE_FIELDS = {"evidence_id", "blake3"}
CHILD_FIELDS = {
    "format",
    "gate",
    "evidence_id",
    "evidence_blake3",
    "release_request_digest",
    "qualification_session_id",
    "candidate_commit",
    "candidate_tree",
    "candidate_semantic_digest",
    "artifact_tuple_digests",
    "registry_root",
    "p5_root",
    "soak_root",
    "role",
    "signer_fingerprint",
    "trust_policy_digest",
    "fresh",
    "carry_forward",
    "checks_blake3",
    "derived_root",
    "signature",
}
EVIDENCE_APPROVAL_FIELDS = {
    "format", "kind", "identity", "receipt_evidence_id", "receipt_blake3",
    "role", "signer_fingerprint", "trust_policy_digest", "signature",
}
EVIDENCE_APPROVER_POLICY_RECORD_FIELDS = {
    "status", "trust_policy_context", "trust_policy_digest", "policy",
}
EVIDENCE_APPROVER_POLICY_FIELDS = {
    "algorithm", "allowed_usages", "format", "role", "signature_domain",
    "signers", "valid_unlisted_signature",
}
EVIDENCE_APPROVER_SIGNER_FIELDS = {
    "created_utc", "expires_utc", "fingerprint_context", "fingerprint_hex",
    "public_key_hex",
}
EVIDENCE_APPROVER_POLICY_CONTEXT = "onebrain:base-v1:evidence-approver-policy:1"
EVIDENCE_APPROVER_FINGERPRINT_CONTEXT = (
    "onebrain:base-v1:evidence-approver-fingerprint:1"
)


class BaseQualificationError(RuntimeError):
    """Qualification input cannot establish every frozen Base v1 gate."""


@dataclass(frozen=True)
class QualificationInputs:
    document: Mapping[str, object]
    evidence_bytes: Mapping[str, bytes]
    freeze_profile: Mapping[str, object]


def canonical_json(value: object) -> bytes:
    return json.dumps(
        value, ensure_ascii=False, sort_keys=True, separators=(",", ":")
    ).encode("utf-8")


def _closed(value: object, fields: set[str], label: str) -> dict[str, object]:
    if not isinstance(value, dict) or set(value) != fields:
        raise BaseQualificationError(f"{label} fields are not closed")
    return value


def _hex(value: object, pattern: re.Pattern[str], label: str) -> str:
    if not isinstance(value, str) or not pattern.fullmatch(value):
        raise BaseQualificationError(f"{label} is not canonical lowercase hex")
    return value


def _digest(value: object, label: str) -> str:
    return _hex(value, HEX_32, label)


def _instant(value: object, label: str) -> datetime:
    if not isinstance(value, str) or not value.endswith("Z"):
        raise BaseQualificationError(f"{label} is not a UTC instant")
    try:
        parsed = datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError as error:
        raise BaseQualificationError(f"{label} is invalid") from error
    if parsed.microsecond or parsed.tzinfo != timezone.utc:
        raise BaseQualificationError(f"{label} is not whole-second UTC")
    return parsed


def _absolute_type_uri(value: object, label: str) -> str:
    if (
        not isinstance(value, str)
        or not value
        or not re.match(r"^[A-Za-z][A-Za-z0-9+.-]*:", value)
        or any(character.isspace() for character in value)
    ):
        raise BaseQualificationError(f"{label} is not an absolute TypeURI")
    try:
        parsed = urlsplit(value)
        if not parsed.scheme or (parsed.scheme in {"http", "https"} and not parsed.netloc):
            raise ValueError("hierarchical URI has no authority")
        parsed.port
    except ValueError as error:
        raise BaseQualificationError(f"{label} is not an absolute TypeURI") from error
    return value


def _spdx_package_verification_code(file_sha1_digests: list[str]) -> str:
    if not file_sha1_digests:
        raise BaseQualificationError("SPDX package verification code has no analyzed files")
    normalized = [
        _hex(digest, HEX_40, "SPDX analyzed-file SHA1")
        for digest in file_sha1_digests
    ]
    return hashlib.sha1("".join(sorted(normalized)).encode("ascii")).hexdigest()


def _hash_evidence(
    evidence: Mapping[str, bytes], evidence_id: object, expected: object, label: str
) -> str:
    if not isinstance(evidence_id, str) or not evidence_id:
        raise BaseQualificationError(f"{label} evidence ID is invalid")
    content = evidence.get(evidence_id)
    if not isinstance(content, bytes):
        raise BaseQualificationError(f"{label} evidence bytes are missing")
    measured = blake3.blake3(content).hexdigest()
    if measured != _digest(expected, f"{label} evidence digest"):
        raise BaseQualificationError(f"{label} evidence bytes differ from their digest")
    return measured


def _canonical_evidence_json(
    evidence: Mapping[str, bytes], evidence_id: object, expected: object, label: str
) -> dict[str, object]:
    """Hash, decode and require byte-exact canonical JSON evidence."""
    _hash_evidence(evidence, evidence_id, expected, label)
    try:
        content = evidence[str(evidence_id)]
        value = json.loads(content)
    except (UnicodeDecodeError, json.JSONDecodeError, TypeError) as error:
        raise BaseQualificationError(f"{label} machine receipt is not JSON") from error
    if not isinstance(value, dict) or content != canonical_json(value):
        raise BaseQualificationError(f"{label} machine receipt is not canonical JSON")
    return value


def _verify_checks(
    checks: object,
    evidence: Mapping[str, bytes],
    label: str,
    contracts: object,
    bindings: dict[str, object],
    *,
    require_builder_id: bool = False,
) -> tuple[list[dict[str, object]], set[str]]:
    if not isinstance(contracts, list) or not contracts:
        raise BaseQualificationError(f"{label} frozen check contract is missing")
    frozen: dict[str, dict[str, object]] = {}
    contract_fields = (
        TARGET_CHECK_CONTRACT_FIELDS
        if require_builder_id
        else GATE_CHECK_CONTRACT_FIELDS
    )
    for item in contracts:
        contract = _closed(item, contract_fields, f"{label} check contract")
        name = contract["name"]
        if not isinstance(name, str) or not name or name in frozen:
            raise BaseQualificationError(f"{label} frozen check name is invalid")
        command = contract["command"]
        assertions = contract["required_assertion_ids"]
        if (
            not isinstance(command, list)
            or not command
            or not all(isinstance(argument, str) and argument for argument in command)
            or not isinstance(assertions, list)
            or not assertions
            or len(assertions) != len(set(assertions))
            or not all(isinstance(assertion, str) and assertion for assertion in assertions)
            or contract["runner_kind"] != "candidate-bound-runner"
            or not isinstance(contract["runner_identity"], str)
            or not contract["runner_identity"]
        ):
            raise BaseQualificationError(f"{label} frozen check contract is invalid")
        if require_builder_id:
            _absolute_type_uri(contract["builder_id"], f"{label} SLSA builder ID")
        frozen[name] = contract
    if not isinstance(checks, list) or len(checks) != len(frozen):
        raise BaseQualificationError(f"{label} machine receipt has no checks")
    normalized: list[dict[str, object]] = []
    referenced: set[str] = set()
    seen: set[str] = set()
    for item in checks:
        check = _closed(item, CHECK_FIELDS, f"{label} check")
        name = check["name"]
        command = check["command"]
        if (
            not isinstance(name, str)
            or not name
            or name in seen
            or name not in frozen
            or not isinstance(command, list)
            or not command
            or not all(isinstance(argument, str) and argument for argument in command)
        ):
            raise BaseQualificationError(f"{label} machine check identity is invalid")
        seen.add(name)
        contract = frozen[name]
        if command != contract["command"]:
            raise BaseQualificationError(f"{label} command is not the frozen command")
        measured_command = blake3.blake3(canonical_json(command)).hexdigest()
        if measured_command != _digest(check["command_blake3"], f"{label} command"):
            raise BaseQualificationError(f"{label} machine command digest differs")
        exit_code = check["exit_code"]
        if isinstance(exit_code, bool) or not isinstance(exit_code, int):
            raise BaseQualificationError(f"{label} machine exit code is invalid")
        if exit_code != 0:
            raise BaseQualificationError(f"{label} machine check failed")
        runner = _closed(check["runner"], RUNNER_FIELDS, f"{label} runner provenance")
        if runner != {
            "format": "onebrain/base-v1-runner-provenance/1",
            "kind": contract["runner_kind"],
            "identity": contract["runner_identity"],
            "candidate_commit": bindings["candidate_commit"],
            "candidate_tree": bindings["candidate_tree"],
            "command_blake3": measured_command,
            "invocation_id": runner.get("invocation_id"),
        }:
            raise BaseQualificationError(f"{label} runner provenance is not candidate-bound")
        _digest(runner["invocation_id"], f"{label} runner invocation")
        stdout = _closed(
            _canonical_evidence_json(
                evidence,
                check["stdout_evidence_id"],
                check["stdout_blake3"],
                f"{label} {name} stdout",
            ),
            CHECK_RESULT_FIELDS,
            f"{label} substantive check result",
        )
        referenced.add(str(check["stdout_evidence_id"]))
        assertions = stdout["assertions"]
        if not isinstance(assertions, list) or len(assertions) != len(contract["required_assertion_ids"]):
            raise BaseQualificationError(f"{label} output oracle assertion set is incomplete")
        assertion_rows: list[dict[str, object]] = []
        assertion_ids: set[str] = set()
        for assertion_item in assertions:
            assertion = _closed(assertion_item, ASSERTION_FIELDS, f"{label} assertion")
            assertion_id = assertion["id"]
            if not isinstance(assertion_id, str) or assertion_id in assertion_ids or assertion["passed"] is not True:
                raise BaseQualificationError(f"{label} output oracle assertion failed")
            assertion_ids.add(assertion_id)
            evidence_id = assertion["evidence_id"]
            content_digest = _hash_evidence(
                evidence, evidence_id, assertion["evidence_blake3"], f"{label} assertion {assertion_id}"
            )
            if not evidence[str(evidence_id)]:
                raise BaseQualificationError(f"{label} assertion evidence output is empty")
            referenced.add(str(evidence_id))
            assertion_rows.append({**dict(assertion), "evidence_blake3": content_digest})
        if assertion_ids != set(contract["required_assertion_ids"]):
            raise BaseQualificationError(f"{label} output oracle assertion identities differ")
        assertion_rows.sort(key=lambda row: str(row["id"]))
        expected_stdout = {
            "format": "onebrain/base-v1-check-result/1",
            "check": name,
            "status": "passed",
            "bindings": bindings,
            "assertions": assertion_rows,
            "assertion_root": blake3.blake3(canonical_json(assertion_rows)).hexdigest(),
        }
        if stdout != expected_stdout:
            raise BaseQualificationError(f"{label} substantive output oracle differs")
        _hash_evidence(
            evidence,
            check["stderr_evidence_id"],
            check["stderr_blake3"],
            f"{label} {name} stderr",
        )
        referenced.add(str(check["stderr_evidence_id"]))
        normalized.append(dict(check))
    if seen != set(frozen):
        raise BaseQualificationError(f"{label} frozen check set is incomplete")
    return sorted(normalized, key=lambda row: str(row["name"])), referenced


def _receipt_bindings(document: dict[str, object], artifacts: dict[str, str]) -> dict[str, object]:
    return {
        "release_request_digest": document["release_request_digest"],
        "qualification_session_id": document["qualification_session_id"],
        "candidate_commit": document["candidate"]["commit"],
        "candidate_tree": document["candidate"]["tree"],
        "candidate_semantic_digest": document["candidate"]["semantic_digest"],
        "artifact_tuple_digests": artifacts,
        "registry_root": document["roots"]["registry"],
        "p5_root": document["roots"]["p5"],
        "soak_root": document["roots"]["soak"],
    }


def _derive_tuple_digests(value: dict[str, object]) -> tuple[str, str]:
    try:
        semantic_bytes = canonical_compatibility_tuple_bytes(
            value, include_artifact_fields=False
        )
        artifact_bytes = canonical_compatibility_tuple_bytes(
            value, include_artifact_fields=True
        )
    except (ReleaseRequestError, KeyError, TypeError, ValueError) as error:
        raise BaseQualificationError("compatibility tuple is invalid") from error
    semantic = blake3.blake3(
        semantic_bytes, derive_key_context="onebrain:base:candidate-semantic:1\0"
    ).hexdigest()
    artifact = blake3.blake3(
        artifact_bytes, derive_key_context="onebrain:base:artifact-tuple:1\0"
    ).hexdigest()
    return semantic, artifact


def _verify_profile(
    inputs: QualificationInputs, *, enforce_frozen_profile: bool
) -> tuple[
    list[str], dict[str, object], dict[str, object], dict[str, object], dict[str, object]
]:
    profile = inputs.freeze_profile
    if profile.get("format") != "onebrain/base-v1-freeze/1" or profile.get("profile_id") != PROFILE_ID:
        raise BaseQualificationError("unsupported Base freeze profile")
    candidate = profile.get("candidate")
    if not isinstance(candidate, dict) or candidate.get("version") != "1.0.0":
        raise BaseQualificationError("freeze profile does not name Base 1.0.0")
    gates = profile.get("base_gate_v1")
    if not isinstance(gates, list) or len(gates) != len(set(gates)) or not all(isinstance(item, str) for item in gates):
        raise BaseQualificationError("freeze profile gate set is invalid")
    child_policies = profile.get("child_evidence_policies")
    if not isinstance(child_policies, dict) or set(child_policies) != SIGNED_GATES:
        raise BaseQualificationError("freeze profile child signer policies are incomplete")
    gate_contracts = profile.get("gate_check_contracts")
    target_contracts = profile.get("target_check_contracts")
    if not isinstance(gate_contracts, dict) or set(gate_contracts) != set(gates):
        raise BaseQualificationError("freeze profile gate check contracts are incomplete")
    if not isinstance(target_contracts, dict) or set(target_contracts) != set(TARGETS.values()):
        raise BaseQualificationError("freeze profile target check contracts are incomplete")
    evidence_policy_record = profile.get("base_evidence_approver_policy")
    if (
        not isinstance(evidence_policy_record, dict)
        or set(evidence_policy_record) != EVIDENCE_APPROVER_POLICY_RECORD_FIELDS
    ):
        raise BaseQualificationError("base-evidence-approver signer policy is missing")
    if evidence_policy_record.get("status") == "pending-owner-approval":
        raise BaseQualificationError(
            "base-evidence-approver production signer policy is pending owner approval"
        )
    if enforce_frozen_profile and evidence_policy_record.get("status") != "owner-approved":
        raise BaseQualificationError("base-evidence-approver signer policy is not production-approved")
    if not enforce_frozen_profile and evidence_policy_record.get("status") not in {
        "owner-approved", "test-only-ephemeral-approved",
    }:
        raise BaseQualificationError("base-evidence-approver signer policy is invalid")
    if evidence_policy_record.get("trust_policy_context") != EVIDENCE_APPROVER_POLICY_CONTEXT:
        raise BaseQualificationError("base-evidence-approver trust policy context differs")
    evidence_policy = _closed(
        evidence_policy_record.get("policy"),
        EVIDENCE_APPROVER_POLICY_FIELDS,
        "base-evidence-approver public policy",
    )
    if (
        evidence_policy.get("algorithm") != "Ed25519"
        or evidence_policy.get("allowed_usages")
        != ["gate-receipt-approval", "target-receipt-approval"]
        or evidence_policy.get("format")
        != "onebrain/base-v1-evidence-approver-policy/1"
        or evidence_policy.get("role") != "base-evidence-approver"
        or evidence_policy.get("signature_domain")
        != "onebrain:base-v1:evidence-receipt-approval:1"
        or evidence_policy.get("valid_unlisted_signature") != "reject"
    ):
        raise BaseQualificationError("base-evidence-approver public policy contract differs")
    signers = evidence_policy.get("signers")
    if not isinstance(signers, list) or len(signers) != 1:
        raise BaseQualificationError("base-evidence-approver signer allowlist is not exact")
    signer = _closed(
        signers[0], EVIDENCE_APPROVER_SIGNER_FIELDS,
        "base-evidence-approver signer",
    )
    if signer.get("fingerprint_context") != EVIDENCE_APPROVER_FINGERPRINT_CONTEXT:
        raise BaseQualificationError("base-evidence-approver fingerprint context differs")
    public_key_hex = _digest(
        signer.get("public_key_hex"), "base evidence approver public key"
    )
    fingerprint = _digest(
        signer.get("fingerprint_hex"), "base evidence approver fingerprint"
    )
    try:
        public_key_bytes = bytes.fromhex(public_key_hex)
        Ed25519PublicKey.from_public_bytes(public_key_bytes)
    except (TypeError, ValueError) as error:
        raise BaseQualificationError(
            "base evidence approver public key is invalid"
        ) from error
    measured_fingerprint = blake3.blake3(
        public_key_bytes,
        derive_key_context=EVIDENCE_APPROVER_FINGERPRINT_CONTEXT,
    ).hexdigest()
    if measured_fingerprint != fingerprint:
        raise BaseQualificationError("base evidence approver fingerprint does not derive")
    created = _instant(signer.get("created_utc"), "base evidence approver created_utc")
    expires = _instant(signer.get("expires_utc"), "base evidence approver expires_utc")
    if created >= expires:
        raise BaseQualificationError("base evidence approver validity interval is empty")
    trust_digest = _digest(
        evidence_policy_record.get("trust_policy_digest"),
        "base evidence approver trust policy digest",
    )
    measured_trust = blake3.blake3(
        canonical_json(evidence_policy),
        derive_key_context=EVIDENCE_APPROVER_POLICY_CONTEXT,
    ).hexdigest()
    if measured_trust != trust_digest:
        raise BaseQualificationError("base evidence approver trust policy does not derive")
    if enforce_frozen_profile:
        measured = blake3.blake3(canonical_json(profile)).hexdigest()
        if measured != FROZEN_PROFILE_BLAKE3:
            raise BaseQualificationError("Base freeze profile digest is not candidate-frozen")
    verified_evidence_policy = {
        "role": evidence_policy["role"],
        "signature_domain": evidence_policy["signature_domain"],
        "public_key_hex": public_key_hex,
        "fingerprint_hex": fingerprint,
        "fingerprint_context": signer["fingerprint_context"],
        "trust_policy_digest": trust_digest,
        "created_utc": signer["created_utc"],
        "expires_utc": signer["expires_utc"],
    }
    return (
        gates, child_policies, gate_contracts, target_contracts,
        verified_evidence_policy,
    )


def _verify_compatibility(
    compatibility: object, candidate: dict[str, object]
) -> tuple[dict[str, dict[str, object]], dict[str, str]]:
    value = _closed(compatibility, COMPATIBILITY_FIELDS, "compatibility")
    tuples = value["per_target_tuples"]
    artifacts = value["per_target_artifact_digests"]
    required_targets = set(TARGETS.values())
    if not isinstance(tuples, dict) or set(tuples) != required_targets:
        raise BaseQualificationError("compatibility target tuple set is not exact")
    if not isinstance(artifacts, dict) or set(artifacts) != required_targets:
        raise BaseQualificationError("compatibility artifact target set is not exact")
    semantic_values: set[str] = set()
    for target in required_targets:
        tuple_value = tuples[target]
        if not isinstance(tuple_value, dict):
            raise BaseQualificationError("compatibility target tuple is invalid")
        version = tuple_value.get("base_version")
        if version != {"major": 1, "minor": 0, "patch": 0, "prerelease": None}:
            raise BaseQualificationError("candidate tuple is not Base 1.0.0 final")
        commit = tuple_value.get("base_commit")
        if commit != {"kind": candidate["object_format"], "hex": candidate["commit"]}:
            raise BaseQualificationError("compatibility tuple candidate commit is mixed")
        if tuple_value.get("target_triple") != target:
            raise BaseQualificationError("compatibility target tuple is cross-target")
        semantic, artifact = _derive_tuple_digests(tuple_value)
        semantic_values.add(semantic)
        if artifact != _digest(artifacts[target], f"{target} artifact digest"):
            raise BaseQualificationError("compatibility artifact tuple digest mismatch")
    if semantic_values != {candidate["semantic_digest"]}:
        raise BaseQualificationError("candidate semantic digest differs across target tuples")

    first = tuples[TARGETS["linux"]]
    expected_globals = {
        "schema_digest": first["canonical_schema_digest"],
        "domain_registry_digest": first["domain_registry_digest"],
        "resource_registry_digest": first["resource_registry_digest"],
        "storage_schema_version": first["storage_schema"],
        "archive_profile_version": first["archive_profile"],
        "migration_profile_version": first["migration_profile"],
        "registry_profile_version": first["registry_profile"],
        "registry_profile_digest": first["registry_profile_digest"],
        "wire_session_version": first["wire_session"],
        "product_api_version": first["product_api"],
        "c_abi_version": first["c_abi"],
        "feature_set_digest": first["feature_set_digest"],
    }
    for field, expected in expected_globals.items():
        if value[field] != expected:
            raise BaseQualificationError(f"compatibility {field} differs from target tuples")
    for tuple_value in tuples.values():
        for field, expected in (
            ("canonical_schema_digest", value["schema_digest"]),
            ("domain_registry_digest", value["domain_registry_digest"]),
            ("resource_registry_digest", value["resource_registry_digest"]),
            ("storage_schema", value["storage_schema_version"]),
            ("archive_profile", value["archive_profile_version"]),
            ("migration_profile", value["migration_profile_version"]),
            ("registry_profile", value["registry_profile_version"]),
            ("registry_profile_digest", value["registry_profile_digest"]),
            ("wire_session", value["wire_session_version"]),
            ("product_api", value["product_api_version"]),
            ("c_abi", value["c_abi_version"]),
            ("feature_set_digest", value["feature_set_digest"]),
        ):
            if tuple_value[field] != expected:
                raise BaseQualificationError(f"compatibility {field} is mixed across targets")
    return tuples, {target: str(artifacts[target]) for target in required_targets}


def _verify_target_receipts(
    receipts: object,
    artifacts: dict[str, str],
    evidence: Mapping[str, bytes],
    document: dict[str, object],
    target_contracts: dict[str, object],
    request_interval: tuple[datetime, datetime],
) -> tuple[list[dict[str, object]], set[str]]:
    if not isinstance(receipts, list) or len(receipts) != 3:
        raise BaseQualificationError("exactly three OS target receipts are required")
    normalized: list[dict[str, object]] = []
    seen_os: set[str] = set()
    seen_targets: set[str] = set()
    referenced: set[str] = set()
    expected_bindings = _receipt_bindings(document, artifacts)
    for item in receipts:
        reference = _closed(item, TARGET_RECEIPT_FIELDS, "target receipt reference")
        os_name = reference["os"]
        target = reference["target_triple"]
        if os_name not in TARGETS or TARGETS[os_name] != target:
            raise BaseQualificationError("target receipt OS/target binding is invalid")
        if os_name in seen_os or target in seen_targets:
            raise BaseQualificationError("duplicate target receipt")
        seen_os.add(str(os_name))
        seen_targets.add(str(target))
        receipt = _closed(
            _canonical_evidence_json(
                evidence,
                reference["receipt_evidence_id"],
                reference["receipt_blake3"],
                f"{os_name} target receipt",
            ),
            TARGET_MACHINE_RECEIPT_FIELDS,
            "target machine receipt",
        )
        referenced.add(str(reference["receipt_evidence_id"]))
        if (
            receipt["format"] != TARGET_RECEIPT_FORMAT
            or receipt["os"] != os_name
            or receipt["target_triple"] != target
            or receipt["artifact_tuple_digest"] != artifacts[target]
        ):
            raise BaseQualificationError("target machine receipt identity or artifact is mixed")
        bindings = _closed(receipt["bindings"], RECEIPT_BINDING_FIELDS, "target bindings")
        if bindings != expected_bindings:
            raise BaseQualificationError("target receipt has mixed candidate/request binding")
        contracts = target_contracts[target]
        if not isinstance(contracts, list) or not contracts:
            raise BaseQualificationError("target frozen check contract is missing")
        builder_ids = {
            _absolute_type_uri(
                _closed(
                    contract,
                    TARGET_CHECK_CONTRACT_FIELDS,
                    "target check contract",
                )["builder_id"],
                "target SLSA builder ID",
            )
            for contract in contracts
        }
        if len(builder_ids) != 1:
            raise BaseQualificationError("target SLSA builder ID is not exact")
        expected_builder_id = next(iter(builder_ids))
        refs: dict[str, dict[str, object]] = {}
        for kind in ("binary", "sbom", "provenance"):
            row = _closed(receipt[kind], EVIDENCE_REFERENCE_FIELDS, f"target {kind}")
            _hash_evidence(
                evidence,
                row["evidence_id"],
                row["blake3"],
                f"{os_name} {kind}",
            )
            referenced.add(str(row["evidence_id"]))
            refs[kind] = row
        binary_digest = str(refs["binary"]["blake3"])
        binary_sha1 = hashlib.sha1(
            evidence[str(refs["binary"]["evidence_id"])]
        ).hexdigest()
        binary_sha256 = hashlib.sha256(
            evidence[str(refs["binary"]["evidence_id"])]
        ).hexdigest()
        sbom = _closed(
            _canonical_evidence_json(
                evidence, refs["sbom"]["evidence_id"], refs["sbom"]["blake3"], f"{os_name} SBOM"
            ),
            {
                "spdxVersion", "SPDXID", "dataLicense", "name", "documentNamespace",
                "creationInfo", "documentDescribes", "packages", "files",
                "relationships", "annotations",
            },
            "target SBOM",
        )
        creation = _closed(
            sbom["creationInfo"], {"created", "creators", "licenseListVersion"},
            "SPDX creation info",
        )
        files = sbom["files"]
        annotations = sbom["annotations"]
        packages = sbom["packages"]
        if not isinstance(packages, list) or len(packages) != 1:
            raise BaseQualificationError("SPDX package verification code contract is not exact")
        package = _closed(
            packages[0],
            {
                "SPDXID", "name", "versionInfo", "downloadLocation",
                "filesAnalyzed", "packageVerificationCode", "licenseConcluded",
                "licenseDeclared", "copyrightText",
            },
            "SPDX package verification code contract",
        )
        verification_code = _closed(
            package["packageVerificationCode"],
            {"packageVerificationCodeValue"},
            "SPDX package verification code",
        )
        measured_package_verification_code = _spdx_package_verification_code(
            [binary_sha1]
        )
        if (
            _hex(
                verification_code["packageVerificationCodeValue"],
                HEX_40,
                "SPDX package verification code",
            )
            != measured_package_verification_code
        ):
            raise BaseQualificationError("SPDX package verification code differs")
        expected_comments = {
            f"onebrain:target-triple:{target}",
            f"onebrain:artifact-tuple-blake3:{artifacts[target]}",
        }
        if (
            sbom["spdxVersion"] != SBOM_FORMAT
            or sbom["SPDXID"] != "SPDXRef-DOCUMENT"
            or sbom["dataLicense"] != "CC0-1.0"
            or not isinstance(sbom["name"], str)
            or not sbom["name"]
            or not isinstance(sbom["documentNamespace"], str)
            or not sbom["documentNamespace"].startswith("https://onebrain.dev/spdx/base-v1/")
            or creation["creators"] != ["Tool: onebrain-base-v1-sbom-generator"]
            or not isinstance(creation["licenseListVersion"], str)
            or not creation["licenseListVersion"]
            or not request_interval[0] <= _instant(
                creation["created"], "SPDX creation timestamp"
            ) < request_interval[1]
            or sbom["documentDescribes"] != ["SPDXRef-Package-OneBrainBase"]
            or package != {
                "SPDXID": "SPDXRef-Package-OneBrainBase",
                "name": "onebrain-base",
                "versionInfo": "1.0.0",
                "downloadLocation": "NOASSERTION",
                "filesAnalyzed": True,
                "packageVerificationCode": {
                    "packageVerificationCodeValue": measured_package_verification_code,
                },
                "licenseConcluded": "NOASSERTION",
                "licenseDeclared": "NOASSERTION",
                "copyrightText": "NOASSERTION",
            }
            or not isinstance(files, list)
            or len(files) != 1
            or _closed(
                files[0],
                {"SPDXID", "fileName", "checksums", "licenseConcluded", "copyrightText"},
                "SPDX binary file",
            ) != {
                "SPDXID": "SPDXRef-BaseBinary",
                "fileName": f"onebrain-base-{target}",
                "checksums": [
                    {"algorithm": "SHA1", "checksumValue": binary_sha1},
                    {"algorithm": "SHA256", "checksumValue": binary_sha256},
                ],
                "licenseConcluded": "NOASSERTION",
                "copyrightText": "NOASSERTION",
            }
            or sbom["relationships"] != [
                {"spdxElementId": "SPDXRef-DOCUMENT", "relationshipType": "DESCRIBES", "relatedSpdxElement": "SPDXRef-Package-OneBrainBase"},
                {"spdxElementId": "SPDXRef-Package-OneBrainBase", "relationshipType": "CONTAINS", "relatedSpdxElement": "SPDXRef-BaseBinary"},
            ]
            or not isinstance(annotations, list)
            or len(annotations) != 2
            or any(
                _closed(annotation, {"annotationType", "annotator", "annotationDate", "comment"}, "SPDX annotation").get("annotationType") != "OTHER"
                or annotation.get("annotator") != "Tool: onebrain-base-v1-qualifier"
                or not request_interval[0] <= _instant(
                    annotation.get("annotationDate"), "SPDX annotation timestamp"
                ) < request_interval[1]
                for annotation in annotations
            )
            or {annotation["comment"] for annotation in annotations} != expected_comments
        ):
            raise BaseQualificationError("target SBOM does not bind the binary/artifact tuple")
        provenance = _closed(
            _canonical_evidence_json(
                evidence,
                refs["provenance"]["evidence_id"],
                refs["provenance"]["blake3"],
                f"{os_name} provenance",
            ),
            {"_type", "subject", "predicateType", "predicate"},
            "target provenance",
        )
        predicate = _closed(
            provenance["predicate"], {"buildDefinition", "runDetails"}, "SLSA predicate"
        )
        build = _closed(
            predicate["buildDefinition"],
            {"buildType", "externalParameters", "internalParameters", "resolvedDependencies"},
            "SLSA build definition",
        )
        external = _closed(
            build["externalParameters"],
            {"target_triple", "artifact_tuple_blake3", "sbom_blake3"},
            "SLSA external parameters",
        )
        run = _closed(predicate["runDetails"], {"builder", "metadata"}, "SLSA run details")
        builder = _closed(run["builder"], {"id"}, "SLSA builder")
        measured_builder_id = _absolute_type_uri(builder["id"], "SLSA builder ID")
        if measured_builder_id != expected_builder_id:
            raise BaseQualificationError("SLSA builder ID differs from frozen target")
        if (
            provenance["_type"] != "https://in-toto.io/Statement/v1"
            or provenance["predicateType"] != "https://slsa.dev/provenance/v1"
            or provenance["subject"] != [{
                "name": f"onebrain-base-{target}", "digest": {"sha256": binary_sha256}
            }]
            or build["buildType"] != "https://onebrain.dev/base-v1/build/v1"
            or external != {
                "target_triple": target,
                "artifact_tuple_blake3": artifacts[target],
                "sbom_blake3": refs["sbom"]["blake3"],
            }
            or build["internalParameters"] != {
                "candidate_commit": document["candidate"]["commit"],
                "candidate_tree": document["candidate"]["tree"],
            }
            or build["resolvedDependencies"] != [{
                "uri": "git+https://onebrain.invalid/OneBrain",
                "digest": {
                    "gitCommit": document["candidate"]["commit"],
                    "gitTree": document["candidate"]["tree"],
                },
            }]
            or not isinstance(run["metadata"], dict)
            or set(run["metadata"]) != {"invocationId", "startedOn", "finishedOn"}
            or run["metadata"]["invocationId"] != document["qualification_session_id"]
            or not request_interval[0] <= _instant(
                run["metadata"]["startedOn"], "SLSA startedOn"
            ) <= _instant(
                run["metadata"]["finishedOn"], "SLSA finishedOn"
            ) < request_interval[1]
        ):
            raise BaseQualificationError("target provenance does not bind binary/SBOM/artifact")
        checks, check_ids = _verify_checks(
            receipt["checks"], evidence, f"{os_name} target", contracts, bindings,
            require_builder_id=True,
        )
        referenced.update(check_ids)
        normalized.append(
            {
                "os": os_name,
                "target_triple": target,
                **dict(bindings),
                "artifact_tuple_digest": artifacts[target],
                "binary_evidence_id": refs["binary"]["evidence_id"],
                "binary_blake3": refs["binary"]["blake3"],
                "sbom_evidence_id": refs["sbom"]["evidence_id"],
                "sbom_blake3": refs["sbom"]["blake3"],
                "provenance_evidence_id": refs["provenance"]["evidence_id"],
                "provenance_blake3": refs["provenance"]["blake3"],
                "receipt_evidence_id": reference["receipt_evidence_id"],
                "receipt_blake3": reference["receipt_blake3"],
                "checks_blake3": blake3.blake3(canonical_json(checks)).hexdigest(),
                "result": "pass",
            }
        )
    if seen_os != set(TARGETS) or seen_targets != set(TARGETS.values()):
        raise BaseQualificationError("Linux, Windows and macOS target receipts are mandatory")
    return sorted(normalized, key=lambda row: str(row["os"])), referenced


def _verify_gates(
    items: object,
    required_gates: list[str],
    document: dict[str, object],
    artifacts: dict[str, str],
    evidence: Mapping[str, bytes],
    gate_contracts: dict[str, object],
) -> tuple[dict[str, dict[str, object]], set[str], dict[str, str]]:
    if not isinstance(items, list):
        raise BaseQualificationError("gate evidence must be a list")
    rows: dict[str, dict[str, object]] = {}
    expected_bindings = _receipt_bindings(document, artifacts)
    referenced: set[str] = set()
    derived_roots: dict[str, str] = {}
    for item in items:
        reference = _closed(item, GATE_FIELDS, "gate evidence reference")
        gate = reference["gate"]
        if not isinstance(gate, str) or gate in rows:
            raise BaseQualificationError("duplicate or invalid gate evidence")
        if gate not in required_gates:
            raise BaseQualificationError("unknown gate evidence")
        receipt = _closed(
            _canonical_evidence_json(
                evidence,
                reference["receipt_evidence_id"],
                reference["receipt_blake3"],
                f"{gate} gate receipt",
            ),
            GATE_RECEIPT_FIELDS,
            "gate machine receipt",
        )
        referenced.add(str(reference["receipt_evidence_id"]))
        if receipt["format"] != GATE_RECEIPT_FORMAT or receipt["gate"] != gate:
            raise BaseQualificationError("gate machine receipt identity is mixed")
        bindings = _closed(receipt["bindings"], RECEIPT_BINDING_FIELDS, "gate bindings")
        if bindings != expected_bindings:
            raise BaseQualificationError("gate evidence has mixed receipt bindings")
        row = receipt
        if not isinstance(row["fresh"], bool) or not isinstance(row["carry_forward"], bool):
            raise BaseQualificationError("gate freshness fields are not booleans")
        if gate in SIGNED_GATES and not row["fresh"]:
            raise BaseQualificationError(f"Base gate {gate} is not fresh")
        if gate in NO_CARRY_FORWARD and row["carry_forward"]:
            raise BaseQualificationError(f"Base gate {gate} attempted carry-forward")
        if not isinstance(row["details"], dict):
            raise BaseQualificationError("gate evidence details are invalid")
        if gate == "dependency-security-and-sbom":
            if set(row["details"]) != {"security_lanes"} or set(
                row["details"].get("security_lanes", [])
            ) != SECURITY_LANES:
                raise BaseQualificationError("required security lane is missing")
        elif row["details"]:
            raise BaseQualificationError("gate evidence details contain unknown fields")
        root_name = {
            "signed-prebuilt-registry": "registry",
            "fresh-multi-host-p5": "p5",
            "fresh-exact-candidate-72h-soak": "soak",
        }.get(gate)
        if root_name is None:
            if row["derived_root"] is not None:
                raise BaseQualificationError("unsigned local gate cannot supply an aggregate root")
        else:
            derived_root = _digest(row["derived_root"], f"{gate} derived root")
            if derived_root != document["roots"][root_name]:
                raise BaseQualificationError(f"{gate} signed derived root differs")
            derived_roots[root_name] = derived_root
        checks, check_ids = _verify_checks(
            row["checks"], evidence, gate, gate_contracts[gate], bindings
        )
        referenced.update(check_ids)
        rows[gate] = {
            "gate": gate,
            "evidence_id": reference["receipt_evidence_id"],
            "evidence_blake3": reference["receipt_blake3"],
            **dict(bindings),
            "fresh": row["fresh"],
            "carry_forward": row["carry_forward"],
            "result": "pass",
            "details": dict(row["details"]),
            "checks_blake3": blake3.blake3(canonical_json(checks)).hexdigest(),
            "derived_root": row["derived_root"],
        }
    if set(rows) != set(required_gates):
        raise BaseQualificationError("Base gate evidence set is incomplete")
    if set(derived_roots) != {"registry", "p5", "soak"}:
        raise BaseQualificationError("signed aggregate roots are incomplete")
    return rows, referenced, derived_roots


def _verify_child_signatures(
    items: object,
    gates: dict[str, dict[str, object]],
    child_policies: dict[str, object],
) -> list[dict[str, object]]:
    if not isinstance(items, list) or len(items) != len(SIGNED_GATES):
        raise BaseQualificationError("signed child evidence set is incomplete")
    verified: list[dict[str, object]] = []
    seen: set[str] = set()
    for item in items:
        envelope = _closed(item, CHILD_FIELDS, "child evidence signature")
        gate = envelope["gate"]
        if gate not in SIGNED_GATES or gate in seen:
            raise BaseQualificationError("duplicate or unexpected child evidence signature")
        seen.add(str(gate))
        policy = child_policies[gate]
        if not isinstance(policy, dict) or set(policy) != {
            "role",
            "public_key_hex",
            "fingerprint_context",
            "fingerprint_hex",
            "trust_policy_digest",
        }:
            raise BaseQualificationError("child signer policy is invalid")
        role = envelope["role"]
        try:
            public_key_bytes = bytes.fromhex(str(policy["public_key_hex"]))
            derived_fingerprint = blake3.blake3(
                public_key_bytes,
                derive_key_context=str(policy["fingerprint_context"]),
            ).hexdigest()
        except ValueError as error:
            raise BaseQualificationError("child signer public key is invalid") from error
        if len(public_key_bytes) != 32 or derived_fingerprint != policy["fingerprint_hex"]:
            raise BaseQualificationError(
                "child signer fingerprint does not derive from its public key"
            )
        if (
            role != policy["role"]
            or envelope["signer_fingerprint"] != policy["fingerprint_hex"]
            or envelope["trust_policy_digest"] != policy["trust_policy_digest"]
        ):
            raise BaseQualificationError("child signature role or policy binding differs")
        gate_row = gates[str(gate)]
        for field in (
            "evidence_id",
            "evidence_blake3",
            "release_request_digest",
            "qualification_session_id",
            "candidate_commit",
            "candidate_tree",
            "candidate_semantic_digest",
            "artifact_tuple_digests",
            "registry_root",
            "p5_root",
            "soak_root",
            "fresh",
            "carry_forward",
            "checks_blake3",
            "derived_root",
        ):
            if envelope[field] != gate_row[field]:
                raise BaseQualificationError("child signature does not bind its gate evidence")
        if envelope["format"] != CHILD_FORMAT:
            raise BaseQualificationError("unsupported child evidence signature format")
        unsigned = {key: value for key, value in envelope.items() if key != "signature"}
        try:
            signature = base64.b64decode(str(envelope["signature"]), validate=True)
            if not HEX_64.fullmatch(signature.hex()):
                raise ValueError("wrong Ed25519 signature length")
            Ed25519PublicKey.from_public_bytes(public_key_bytes).verify(
                signature,
                CHILD_SIGNATURE_DOMAIN + blake3.blake3(canonical_json(unsigned)).digest(),
            )
        except (ValueError, InvalidSignature) as error:
            raise BaseQualificationError("child evidence signature is invalid") from error
        verified.append(
            {
                "gate": gate,
                "role": role,
                "signer_fingerprint": envelope["signer_fingerprint"],
                "trust_policy_digest": envelope["trust_policy_digest"],
                "evidence_blake3": envelope["evidence_blake3"],
                "signature": envelope["signature"],
            }
        )
    return sorted(verified, key=lambda row: str(row["gate"]))


def _verify_evidence_approvals(
    signatures: object,
    target_receipts: list[dict[str, object]],
    gates: dict[str, dict[str, object]],
    policy: dict[str, object],
) -> list[dict[str, object]]:
    required: dict[tuple[str, str], dict[str, object]] = {
        ("target", str(receipt["target_triple"])): receipt
        for receipt in target_receipts
    }
    required.update({
        ("gate", gate): receipt
        for gate, receipt in gates.items()
        if gate not in SIGNED_GATES
    })
    if not isinstance(signatures, list) or len(signatures) != len(required):
        raise BaseQualificationError("base evidence approver signature set is incomplete")
    try:
        public_bytes = bytes.fromhex(str(policy["public_key_hex"]))
        public_key = Ed25519PublicKey.from_public_bytes(public_bytes)
    except (ValueError, TypeError) as error:
        raise BaseQualificationError("base evidence approver public key is invalid") from error
    measured_fingerprint = blake3.blake3(
        public_bytes, derive_key_context=str(policy["fingerprint_context"])
    ).hexdigest()
    if measured_fingerprint != policy["fingerprint_hex"]:
        raise BaseQualificationError("base evidence approver fingerprint does not derive")
    normalized: list[dict[str, object]] = []
    seen: set[tuple[str, str]] = set()
    for item in signatures:
        envelope = _closed(item, EVIDENCE_APPROVAL_FIELDS, "base evidence approver signature")
        key = (str(envelope["kind"]), str(envelope["identity"]))
        if key in seen or key not in required:
            raise BaseQualificationError("base evidence approver signature identity differs")
        seen.add(key)
        reference = required[key]
        receipt_evidence_id = reference.get(
            "receipt_evidence_id", reference.get("evidence_id")
        )
        receipt_blake3 = reference.get("receipt_blake3", reference.get("evidence_blake3"))
        if (
            envelope["format"] != EVIDENCE_APPROVAL_FORMAT
            or envelope["receipt_evidence_id"] != receipt_evidence_id
            or envelope["receipt_blake3"] != receipt_blake3
            or envelope["role"] != policy["role"]
            or envelope["signer_fingerprint"] != policy["fingerprint_hex"]
            or envelope["trust_policy_digest"] != policy["trust_policy_digest"]
        ):
            raise BaseQualificationError("base evidence approver signature bindings differ")
        try:
            signature = base64.b64decode(str(envelope["signature"]), validate=True)
            if len(signature) != 64:
                raise ValueError("wrong Ed25519 signature length")
            unsigned = {field: value for field, value in envelope.items() if field != "signature"}
            public_key.verify(
                signature,
                EVIDENCE_APPROVAL_DOMAIN
                + blake3.blake3(canonical_json(unsigned)).digest(),
            )
        except (ValueError, InvalidSignature) as error:
            raise BaseQualificationError("base evidence approver signature is invalid") from error
        normalized.append(dict(envelope))
    if seen != set(required):
        raise BaseQualificationError("base evidence approver signature set is incomplete")
    return sorted(normalized, key=lambda row: (str(row["kind"]), str(row["identity"])))


def _qualify_base(
    inputs: QualificationInputs, *, enforce_frozen_profile: bool
) -> dict[str, object]:
    (
        required_gates, child_policies, gate_contracts, target_contracts,
        evidence_approver_policy,
    ) = _verify_profile(
        inputs, enforce_frozen_profile=enforce_frozen_profile
    )
    document = _closed(inputs.document, INPUT_FIELDS, "qualification input")
    if document["format"] != INPUT_FORMAT:
        raise BaseQualificationError("unsupported qualification input format")
    request = _digest(document["release_request_digest"], "release request digest")
    request_created = _instant(
        document["release_request_created_utc"], "release request created_utc"
    )
    request_expires = _instant(
        document["release_request_expires_utc"], "release request expires_utc"
    )
    if request_created >= request_expires:
        raise BaseQualificationError("release request validity interval is empty")
    evidence_signer_created = _instant(
        evidence_approver_policy["created_utc"],
        "base evidence approver created_utc",
    )
    evidence_signer_expires = _instant(
        evidence_approver_policy["expires_utc"],
        "base evidence approver expires_utc",
    )
    if (
        request_created < evidence_signer_created
        or request_expires > evidence_signer_expires
    ):
        raise BaseQualificationError(
            "candidate request is outside base evidence approver validity"
        )
    session = _digest(document["qualification_session_id"], "qualification session ID")
    candidate = _closed(document["candidate"], CANDIDATE_FIELDS, "candidate")
    object_format = candidate["object_format"]
    if object_format not in {"sha1", "sha256"}:
        raise BaseQualificationError("candidate object format is unsupported")
    commit_pattern = HEX_40 if object_format == "sha1" else HEX_32
    _hex(candidate["commit"], commit_pattern, "candidate commit")
    _hex(candidate["tree"], commit_pattern, "candidate tree")
    _digest(candidate["semantic_digest"], "candidate semantic digest")

    roots = _closed(document["roots"], {"registry", "p5", "soak"}, "evidence roots")
    for name, value in roots.items():
        _digest(value, f"{name} root")
    _, artifacts = _verify_compatibility(document["compatibility"], candidate)
    features = _closed(document["feature_matrix"], FEATURE_FIELDS, "feature matrix")
    if features != {
        "base_default": True,
        "legacy_default": False,
        "network_default": False,
        "network_kill_switch_verified": True,
    }:
        raise BaseQualificationError("Base default, legacy or network policy is unsafe")
    receipts, target_evidence_ids = _verify_target_receipts(
        document["target_receipts"], artifacts, inputs.evidence_bytes, document,
        target_contracts, (request_created, request_expires),
    )
    gates, gate_evidence_ids, derived_roots = _verify_gates(
        document["gate_evidence"], required_gates, document, artifacts, inputs.evidence_bytes,
        gate_contracts,
    )
    signatures = _verify_child_signatures(
        document["child_signatures"], gates, child_policies
    )
    evidence_signatures = _verify_evidence_approvals(
        document["evidence_signatures"], receipts, gates, evidence_approver_policy
    )
    if derived_roots != dict(roots):
        raise BaseQualificationError("manifest roots do not derive from signed gate receipts")

    documents = _closed(document["documents"], {"migration", "rollback", "changelog"}, "release documents")
    normalized_documents: dict[str, dict[str, str]] = {}
    for name, item in documents.items():
        row = _closed(item, {"evidence_id", "blake3"}, f"{name} document")
        digest = _hash_evidence(
            inputs.evidence_bytes, row["evidence_id"], row["blake3"], f"{name} document"
        )
        normalized_documents[name] = {"evidence_id": str(row["evidence_id"]), "blake3": digest}
    limitations = document["limitations"]
    if not isinstance(limitations, list) or not all(isinstance(item, str) and item for item in limitations):
        raise BaseQualificationError("limitations must be an explicit string list")

    referenced_ids = gate_evidence_ids | target_evidence_ids | {
        str(row["evidence_id"]) for row in documents.values()
    }
    if set(inputs.evidence_bytes) != referenced_ids:
        raise BaseQualificationError("raw evidence set has missing or unreferenced bytes")
    raw_evidence = {
        evidence_id: blake3.blake3(inputs.evidence_bytes[evidence_id]).hexdigest()
        for evidence_id in sorted(referenced_ids)
    }
    manifest = {
        "format": MANIFEST_FORMAT,
        "qualification_tier": "production" if enforce_frozen_profile else "nonproduction-test",
        "freeze_profile_blake3": blake3.blake3(canonical_json(inputs.freeze_profile)).hexdigest(),
        "release_request_digest": request,
        "release_request_created_utc": document["release_request_created_utc"],
        "release_request_expires_utc": document["release_request_expires_utc"],
        "qualification_session_id": session,
        "candidate": dict(candidate),
        "compatibility": dict(document["compatibility"]),
        "feature_matrix": dict(features),
        "target_receipts": receipts,
        "gate_evidence": [gates[gate] for gate in required_gates],
        "child_signatures": signatures,
        "evidence_signatures": evidence_signatures,
        "raw_evidence": raw_evidence,
        "documents": normalized_documents,
        "limitations": list(limitations),
        "qualified": True,
    }
    return manifest


def qualify_base(inputs: QualificationInputs) -> dict[str, object]:
    """Recompute production evidence and derive ``qualified`` without side effects."""
    return _qualify_base(inputs, enforce_frozen_profile=True)


def qualify_base_for_test_nonproduction(inputs: QualificationInputs) -> dict[str, object]:
    """Exercise mutation logic with ephemeral signers; never used by the CLI."""
    return _qualify_base(inputs, enforce_frozen_profile=False)


def read_qualification_bundle(path: Path) -> QualificationInputs:
    try:
        bundle = json.loads(path.read_text(encoding="utf-8"))
        if set(bundle) != {"document", "evidence", "freeze_profile"}:
            raise BaseQualificationError("qualification bundle fields are not closed")
        evidence = {
            evidence_id: Path(locator).resolve(strict=True).read_bytes()
            for evidence_id, locator in bundle["evidence"].items()
        }
        return QualificationInputs(
            document=bundle["document"],
            evidence_bytes=evidence,
            freeze_profile=bundle["freeze_profile"],
        )
    except (OSError, json.JSONDecodeError, TypeError) as error:
        raise BaseQualificationError("qualification bundle is invalid") from error


def read_evidence_root(root: Path) -> tuple[QualificationInputs, dict[str, object]]:
    evidence_root = root.resolve(strict=True)
    bundle_path = evidence_root / "qualification-bundle.json"
    try:
        payload = bundle_path.read_bytes()
        bundle = json.loads(payload)
    except (OSError, json.JSONDecodeError) as error:
        raise BaseQualificationError("evidence-root qualification bundle is invalid") from error
    fields = {
        "format", "document", "evidence", "freeze_profile",
        "prepared_candidate_receipt", "candidate_root",
    }
    if (
        not isinstance(bundle, dict)
        or set(bundle) != fields
        or bundle["format"] != "onebrain/base-v1-evidence-root/1"
        or payload != canonical_json(bundle)
        or not isinstance(bundle["evidence"], dict)
    ):
        raise BaseQualificationError("evidence-root bundle fields/bytes are not closed canonical JSON")
    evidence: dict[str, bytes] = {}
    for evidence_id, locator in bundle["evidence"].items():
        if not isinstance(evidence_id, str) or not isinstance(locator, str):
            raise BaseQualificationError("evidence-root locator is invalid")
        relative = Path(locator)
        if relative.is_absolute() or ".." in relative.parts:
            raise BaseQualificationError("evidence-root locator escapes its root")
        path = (evidence_root / relative).resolve(strict=True)
        try:
            path.relative_to(evidence_root)
        except ValueError as error:
            raise BaseQualificationError("evidence-root locator escapes its root") from error
        evidence[evidence_id] = path.read_bytes()
    return (
        QualificationInputs(
            document=bundle["document"],
            evidence_bytes=evidence,
            freeze_profile=bundle["freeze_profile"],
        ),
        bundle,
    )


def _fsync_directory(path: Path) -> None:
    if os.name == "nt":
        return
    descriptor = os.open(path, os.O_RDONLY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def _ensure_directory(path: Path) -> None:
    existed = path.is_dir()
    path.mkdir(parents=True, exist_ok=True)
    if not existed:
        _fsync_directory(path.parent)


def _create_exact_file(path: Path, payload: bytes) -> None:
    _ensure_directory(path.parent)
    try:
        with path.open("xb") as handle:
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
    except FileExistsError:
        if path.read_bytes() != payload:
            raise BaseQualificationError(f"immutable qualification collision at {path.name}")
    _fsync_directory(path.parent)


def _atomic_create_or_exact(path: Path, payload: bytes) -> None:
    _ensure_directory(path.parent)
    temporary: Path | None = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="wb", prefix=f".{path.name}.", suffix=".tmp", dir=path.parent, delete=False
        ) as handle:
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
            temporary = Path(handle.name)
        try:
            os.link(temporary, path)
        except FileExistsError:
            if path.read_bytes() != payload:
                raise BaseQualificationError(
                    f"immutable qualification collision at {path.name}"
                )
        _fsync_directory(path.parent)
    except OSError as error:
        raise BaseQualificationError("atomic qualification publication failed") from error
    finally:
        if temporary is not None:
            try:
                temporary.unlink(missing_ok=True)
            except OSError:
                pass


def publish_manifest_generation(
    *,
    inputs: QualificationInputs,
    bundle: dict[str, object],
    output_generation_root: Path,
    ready_output: Path,
) -> Path:
    try:
        candidate_root = Path(str(bundle["candidate_root"])).resolve(strict=True)
        prepared_receipt = Path(str(bundle["prepared_candidate_receipt"])).resolve(strict=True)
        prepared_receipt_digest = blake3.blake3(prepared_receipt.read_bytes()).hexdigest()
    except (KeyError, OSError) as error:
        raise BaseQualificationError("prepared candidate identity is unavailable") from error
    if str(candidate_root) != str(bundle["candidate_root"]) or str(prepared_receipt) != str(
        bundle["prepared_candidate_receipt"]
    ):
        raise BaseQualificationError("prepared candidate paths are not canonical absolute")
    manifest = qualify_base(inputs)
    manifest_bytes = canonical_json(manifest)
    manifest_digest = blake3.blake3(manifest_bytes).hexdigest()
    generation_root = output_generation_root.resolve()
    _ensure_directory(generation_root)
    generation = generation_root / manifest_digest
    _ensure_directory(generation)
    _create_exact_file(generation / "manifest.json", manifest_bytes)
    _create_exact_file(generation / "manifest.blake3", (manifest_digest + "\n").encode("ascii"))
    if {item.name for item in generation.iterdir()} != {"manifest.json", "manifest.blake3"}:
        raise BaseQualificationError("manifest generation contains missing or extra files")
    _fsync_directory(generation)
    ready = {
        "format": "onebrain/base-v1-manifest-ready/1",
        "manifest_digest": manifest_digest,
        "generation": str(generation),
        "release_request_digest": manifest["release_request_digest"],
        "qualification_session_id": manifest["qualification_session_id"],
        "candidate": manifest["candidate"],
        "candidate_root": str(candidate_root),
        "prepared_candidate_receipt": str(prepared_receipt),
        "prepared_candidate_receipt_blake3": prepared_receipt_digest,
    }
    pointer = {
        "format": "onebrain/base-v1-manifest-ready-pointer/1",
        "ready_blake3": blake3.blake3(canonical_json(ready)).hexdigest(),
        "ready": ready,
    }
    _atomic_create_or_exact(ready_output.resolve(), canonical_json(pointer))
    return ready_output.resolve()


def verify_manifest_ready(ready_path: Path, release_request_path: Path) -> tuple[dict[str, object], Path]:
    try:
        payload = ready_path.resolve(strict=True).read_bytes()
        pointer = json.loads(payload)
        request_bytes = release_request_path.resolve(strict=True).read_bytes()
    except (OSError, json.JSONDecodeError) as error:
        raise BaseQualificationError("manifest ready pointer is unreadable") from error
    if (
        payload != canonical_json(pointer)
        or not isinstance(pointer, dict)
        or set(pointer) != {"format", "ready_blake3", "ready"}
        or pointer["format"] != "onebrain/base-v1-manifest-ready-pointer/1"
        or pointer["ready_blake3"] != blake3.blake3(canonical_json(pointer["ready"])).hexdigest()
    ):
        raise BaseQualificationError("manifest ready pointer checksum/fields differ")
    ready = _closed(
        pointer["ready"],
        {
            "format", "manifest_digest", "generation", "release_request_digest",
            "qualification_session_id", "candidate", "candidate_root", "prepared_candidate_receipt",
            "prepared_candidate_receipt_blake3",
        },
        "manifest ready",
    )
    if ready["format"] != "onebrain/base-v1-manifest-ready/1":
        raise BaseQualificationError("manifest ready format is unsupported")
    generation = Path(str(ready["generation"])).resolve(strict=True)
    if {item.name for item in generation.iterdir()} != {"manifest.json", "manifest.blake3"}:
        raise BaseQualificationError("manifest ready generation file set differs")
    manifest_path = generation / "manifest.json"
    manifest_bytes = manifest_path.read_bytes()
    digest = blake3.blake3(manifest_bytes).hexdigest()
    if (
        digest != ready["manifest_digest"]
        or (generation / "manifest.blake3").read_text(encoding="ascii") != digest + "\n"
        or generation.name != digest
        or ready["release_request_digest"] != blake3.blake3(request_bytes).hexdigest()
    ):
        raise BaseQualificationError("manifest ready digest/request binding differs")
    manifest = json.loads(manifest_bytes)
    if manifest_bytes != canonical_json(manifest) or manifest.get("candidate") != ready["candidate"]:
        raise BaseQualificationError("manifest ready candidate/canonical bytes differ")
    try:
        candidate_root = Path(str(ready["candidate_root"])).resolve(strict=True)
        prepared_receipt = Path(str(ready["prepared_candidate_receipt"])).resolve(strict=True)
        prepared_digest = blake3.blake3(prepared_receipt.read_bytes()).hexdigest()
    except OSError as error:
        raise BaseQualificationError("manifest ready candidate receipt is unavailable") from error
    if (
        str(candidate_root) != ready["candidate_root"]
        or str(prepared_receipt) != ready["prepared_candidate_receipt"]
        or prepared_digest != ready["prepared_candidate_receipt_blake3"]
    ):
        raise BaseQualificationError("manifest ready candidate receipt binding differs")
    return ready, manifest_path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release-request", type=Path)
    parser.add_argument("--release-request-signature", type=Path)
    parser.add_argument("--evidence-root", type=Path)
    parser.add_argument("--output-generation-root", type=Path)
    parser.add_argument("--ready-output", type=Path)
    parser.add_argument("--verify-ready", type=Path)
    args = parser.parse_args()
    try:
        if args.verify_ready is not None:
            if args.release_request is None or any(
                value is not None
                for value in (
                    args.release_request_signature, args.evidence_root,
                    args.output_generation_root, args.ready_output,
                )
            ):
                raise BaseQualificationError("verify-ready arguments are not closed")
            verify_manifest_ready(args.verify_ready, args.release_request)
            return 0
        if any(
            value is None
            for value in (
                args.release_request, args.release_request_signature, args.evidence_root,
                args.output_generation_root, args.ready_output,
            )
        ):
            raise BaseQualificationError("production qualification arguments are incomplete")
        inputs, bundle = read_evidence_root(args.evidence_root)
        candidate_root = Path(str(bundle["candidate_root"])).resolve(strict=True)
        from scripts.release.create_base_release_request import verify_task28_release_request

        request = verify_task28_release_request(
            args.release_request,
            args.release_request_signature,
            candidate_root / "src/test-vectors/vnext/base-v1-release-signers-v1.json",
        )
        request_digest = blake3.blake3(args.release_request.read_bytes()).hexdigest()
        document = inputs.document
        if (
            document.get("release_request_digest") != request_digest
            or document.get("qualification_session_id") != request["qualification_session_id"]
            or document.get("release_request_created_utc") != request["created_utc"]
            or document.get("release_request_expires_utc") != request["expires_utc"]
            or not isinstance(document.get("candidate"), dict)
            or any(
                document["candidate"].get(field) != request["candidate"].get(field)
                for field in ("commit", "tree", "object_format")
            )
        ):
            raise BaseQualificationError("qualification bundle differs from signed release request")
        publish_manifest_generation(
            inputs=inputs,
            bundle=bundle,
            output_generation_root=args.output_generation_root,
            ready_output=args.ready_output,
        )
        print("BASE-GATE-V1 PASS qualified=true")
        return 0
    except (BaseQualificationError, OSError) as error:
        parser.error(str(error))
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
