#!/usr/bin/env python3
"""Pure verifier and aggregator for exact-candidate Registry qualification receipts.

This program never launches a probe.  It verifies already-produced signed
receipts, requires one fresh release context and exact cross-report bindings,
then emits a separately signed Registry-only aggregate receipt.
"""

from __future__ import annotations

import argparse
import json
import os
import tempfile
from pathlib import Path
from typing import Any

import blake3
from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives.asymmetric.ed25519 import (
    Ed25519PrivateKey,
    Ed25519PublicKey,
)


RECEIPT_FORMAT = "onebrain/concept-registry-qualification-receipt/1"
RECEIPT_USAGE = "registry-qualification-receipt"
RUN_CONTEXT_FORMAT = "onebrain/qualification-run-context/1"
RECEIPT_DOMAIN = b"onebrain:concept-registry-qualification-receipt:1\0"
FINGERPRINT_CONTEXT = "onebrain:concept-registry:signer-fingerprint:1"
TRUST_POLICY_CONTEXT = "onebrain:concept-registry:trust-policy:1"
FROZEN_PROFILE_BLAKE3 = "8919f487d1e05e826dbac381f7d0a78c5c7b524da16aa6cbc2410f23723cd071"
FROZEN_TRUST_POLICY_DIGEST = "e0a2551a39823c3f2cb088defe60484c8a33ffe0f3aab9df9493b52557ab55fe"
FROZEN_SIGNER_PUBLIC_KEY = "bef8e2b9d8ae7a38b3753a7d756a39c20948f128a66ca71ed04799e7a5d5177c"
ENVELOPE_FIELDS = {
    "format",
    "receipt_kind",
    "usage",
    "payload",
    "signer_public_key",
    "signer_fingerprint",
    "trust_policy_digest",
    "signature",
}
COMMON_BINDINGS = (
    "release_aggregate_root",
    "registry_generation",
    "production_profile_blake3",
    "trust_policy_digest",
    "signer_fingerprint",
    "probe_blake3",
    "executable_blake3",
    "candidate_payload_artifacts_blake3",
    "release_stamp_blake3",
)
RELEASE_CANDIDATE_BINDINGS = ("candidate_semantic_digest", "artifact_tuple_digest")
PRODUCTION_EQUALITY_BINDINGS = (*COMMON_BINDINGS, *RELEASE_CANDIDATE_BINDINGS)
RELEASE_CONTEXT_FIELDS = (
    "release_request_digest",
    "qualification_session_id",
    "candidate_commit",
    "candidate_tree",
)
REQUIRED_PAYLOAD_FIELDS = {
    "qualification_context_variant",
    *COMMON_BINDINGS,
    *RELEASE_CONTEXT_FIELDS,
    *RELEASE_CANDIDATE_BINDINGS,
    "base_candidate_bound",
    "command",
    "result",
    "exit_oracles",
    "limitations",
}
EXPECTED_RESOURCE_PROFILES = {"cold-cache", "low-ram", "ssd", "hdd"}
EXPECTED_SINGLE_KINDS = {
    "failure-qualification",
    "generation-swap",
    "ccid-stability",
    "signed-release-cycle",
}
PAYLOAD_ARTIFACT_KEYS = {
    "OBR:concepts.obr",
    "LABEL_INDEX:concepts.obr.labels.idx",
    "CCID_INDEX:concepts.obr.ccids.idx",
    "MANIFEST:concepts.obr.manifest.json",
    "SPDX_SBOM:sbom.spdx.json",
}


class AggregationError(RuntimeError):
    """Receipts cannot derive the frozen Registry production subgate."""


def canonical_json(value: object) -> bytes:
    return json.dumps(
        value, ensure_ascii=False, sort_keys=True, separators=(",", ":")
    ).encode("utf-8")


def signer_fingerprint(public_key: bytes) -> str:
    if len(public_key) != 32:
        raise AggregationError("signer public key must be 32 bytes")
    return blake3.blake3(
        public_key, derive_key_context=FINGERPRINT_CONTEXT
    ).hexdigest()


def trust_policy_digest(policy: dict[str, object]) -> str:
    return blake3.blake3(
        canonical_json(policy), derive_key_context=TRUST_POLICY_CONTEXT
    ).hexdigest()


def _hex(value: object, field: str, lengths: tuple[int, ...] = (64,)) -> str:
    if (
        not isinstance(value, str)
        or len(value) not in lengths
        or any(character not in "0123456789abcdef" for character in value)
    ):
        raise AggregationError(f"{field} must be lowercase hexadecimal")
    return value


def _policy_signers(policy: dict[str, object]) -> dict[str, str]:
    if (
        policy.get("format") != "onebrain/concept-registry-trust-policy/1"
        or policy.get("algorithm") != "Ed25519"
        or RECEIPT_USAGE not in policy.get("allowed_usages", [])
    ):
        raise AggregationError("trust policy does not allow Registry qualification receipts")
    signers = policy.get("signers")
    if not isinstance(signers, list) or not signers:
        raise AggregationError("trust policy signer allowlist is empty")
    result: dict[str, str] = {}
    for signer in signers:
        if not isinstance(signer, dict):
            raise AggregationError("trust policy signer is invalid")
        public = _hex(signer.get("public_key_hex"), "policy public key")
        fingerprint = _hex(signer.get("fingerprint_hex"), "policy fingerprint")
        if signer_fingerprint(bytes.fromhex(public)) != fingerprint:
            raise AggregationError("trust policy signer fingerprint is invalid")
        result[public] = fingerprint
    return result


def create_signed_receipt(
    receipt_kind: str,
    payload: dict[str, object],
    signing_key: Ed25519PrivateKey,
    policy: dict[str, object],
) -> dict[str, object]:
    """Create the frozen receipt envelope from explicit inputs.

    Tests and qualification runners pass ephemeral keys.  This function never
    discovers a key or reads a key path.
    """
    signers = _policy_signers(policy)
    public = signing_key.public_key().public_bytes_raw()
    public_hex = public.hex()
    fingerprint = signer_fingerprint(public)
    if signers.get(public_hex) != fingerprint:
        raise AggregationError("aggregate signer is not allowlisted")
    policy_digest = trust_policy_digest(policy)
    envelope: dict[str, object] = {
        "format": RECEIPT_FORMAT,
        "receipt_kind": receipt_kind,
        "usage": RECEIPT_USAGE,
        "payload": payload,
        "signer_public_key": public_hex,
        "signer_fingerprint": fingerprint,
        "trust_policy_digest": policy_digest,
        "signature": "",
    }
    message = RECEIPT_DOMAIN + blake3.blake3(canonical_json(envelope)).digest()
    envelope["signature"] = signing_key.sign(message).hex()
    return envelope


def _verify_receipt(
    receipt: object,
    profile: dict[str, object],
    policy: dict[str, object],
) -> tuple[str, dict[str, object]]:
    if not isinstance(receipt, dict) or set(receipt) != ENVELOPE_FIELDS:
        raise AggregationError("receipt envelope fields are not closed")
    envelope = profile.get("qualification_receipt_envelope")
    if not isinstance(envelope, dict):
        raise AggregationError("production profile receipt envelope is missing")
    if receipt.get("format") != RECEIPT_FORMAT or receipt.get("usage") != RECEIPT_USAGE:
        raise AggregationError("receipt format or usage is invalid")
    kind = receipt.get("receipt_kind")
    if not isinstance(kind, str) or kind not in envelope.get("closed_receipt_kinds", []):
        raise AggregationError("receipt kind is not closed or allowlisted")
    if kind == "production-aggregate":
        raise AggregationError("production aggregate cannot be used as a component report")
    public_hex = _hex(receipt.get("signer_public_key"), "receipt signer public key")
    fingerprint = _hex(receipt.get("signer_fingerprint"), "receipt signer fingerprint")
    expected_policy_digest = trust_policy_digest(policy)
    if receipt.get("trust_policy_digest") != expected_policy_digest:
        raise AggregationError("receipt trust_policy_digest mismatch")
    signers = _policy_signers(policy)
    if signers.get(public_hex) != fingerprint:
        raise AggregationError("receipt signer is not allowlisted")
    if signer_fingerprint(bytes.fromhex(public_hex)) != fingerprint:
        raise AggregationError("receipt signer fingerprint mismatch")
    signature = bytes.fromhex(_hex(receipt.get("signature"), "receipt signature", (128,)))
    unsigned = dict(receipt)
    unsigned["signature"] = ""
    message = RECEIPT_DOMAIN + blake3.blake3(canonical_json(unsigned)).digest()
    try:
        Ed25519PublicKey.from_public_bytes(bytes.fromhex(public_hex)).verify(signature, message)
    except (InvalidSignature, ValueError) as error:
        raise AggregationError("receipt signature verification failed") from error
    payload = receipt.get("payload")
    if not isinstance(payload, dict):
        raise AggregationError("receipt payload is not an object")
    return kind, payload


def parse_qualification_run_context(value: object) -> dict[str, object]:
    if not isinstance(value, dict) or value.get("format") != RUN_CONTEXT_FORMAT:
        raise AggregationError("QualificationRunContextV1 format is invalid")
    variant = value.get("variant")
    if variant == "Prequalification":
        expected = {"format", "variant", "closure_digest"}
        if set(value) != expected:
            raise AggregationError("Prequalification context fields are not closed")
        _hex(value.get("closure_digest"), "closure_digest")
    elif variant == "Release":
        expected = {"format", "variant", *RELEASE_CONTEXT_FIELDS}
        if set(value) != expected:
            raise AggregationError("Release context fields are not closed")
        _hex(value.get("release_request_digest"), "release_request_digest")
        session = value.get("qualification_session_id")
        if not isinstance(session, str) or not session or len(session) > 128:
            raise AggregationError("qualification_session_id is invalid")
        _hex(value.get("candidate_commit"), "candidate_commit", (40, 64))
        _hex(value.get("candidate_tree"), "candidate_tree", (40, 64))
    else:
        raise AggregationError("QualificationRunContextV1 variant is invalid")
    return dict(value)


def _validate_component_payload(
    kind: str,
    payload: dict[str, object],
    context: dict[str, object],
    profile_digest: str,
    policy_digest: str,
    envelope_fingerprint: str,
) -> None:
    missing = REQUIRED_PAYLOAD_FIELDS.difference(payload)
    if missing:
        field = sorted(missing)[0]
        raise AggregationError(f"component payload missing {field}")
    if any(field in payload for field in ("carry_forward", "carry_forward_receipt", "carried_forward_from")):
        raise AggregationError("carry-forward Registry evidence is forbidden for Base v1")
    if payload.get("qualification_context_variant") != "Release":
        raise AggregationError("component qualification context must be Release")
    if payload.get("base_candidate_bound") is not True:
        raise AggregationError("component is not bound to the Base candidate")
    for field in RELEASE_CONTEXT_FIELDS:
        if payload.get(field) != context[field]:
            raise AggregationError(f"component {field} mismatch")
    if payload.get("production_profile_blake3") != profile_digest:
        raise AggregationError("component production profile digest mismatch")
    if payload.get("trust_policy_digest") != policy_digest:
        raise AggregationError("component trust_policy_digest mismatch")
    if payload.get("signer_fingerprint") != envelope_fingerprint:
        raise AggregationError("component signer_fingerprint mismatch")
    if payload.get("result") is not True:
        raise AggregationError("component result is false")
    oracles = payload.get("exit_oracles")
    if not isinstance(oracles, dict) or not oracles or any(value is not True for value in oracles.values()):
        raise AggregationError("component exit_oracles contain a false result")
    command = payload.get("command")
    limitations = payload.get("limitations")
    if not isinstance(command, list) or not command or not all(isinstance(value, str) for value in command):
        raise AggregationError("component command is invalid")
    if not isinstance(limitations, list) or not all(isinstance(value, str) for value in limitations):
        raise AggregationError("component limitations are invalid")
    for field in (
        "release_aggregate_root",
        "production_profile_blake3",
        "trust_policy_digest",
        "signer_fingerprint",
        "probe_blake3",
        "executable_blake3",
        "release_stamp_blake3",
        "candidate_semantic_digest",
        "artifact_tuple_digest",
    ):
        _hex(payload.get(field), field)
    generation = payload.get("registry_generation")
    if isinstance(generation, bool) or not isinstance(generation, int) or generation <= 0:
        raise AggregationError("registry_generation must be a positive integer")
    artifacts = payload.get("candidate_payload_artifacts_blake3")
    if not isinstance(artifacts, dict) or set(artifacts) != PAYLOAD_ARTIFACT_KEYS:
        raise AggregationError("candidate payload artifact tuple is not exact")
    for path, digest in artifacts.items():
        _hex(digest, f"candidate artifact {path}")
    if kind == "resource-qualification" and payload.get("qualification_profile") not in EXPECTED_RESOURCE_PROFILES:
        raise AggregationError("resource qualification profile is invalid")


def aggregate_reports(
    reports: list[dict[str, object]],
    run_context: dict[str, object],
    profile: dict[str, object],
    aggregate_signing_key: Ed25519PrivateKey,
) -> dict[str, object]:
    """Verify component receipts and return one signed Registry-only aggregate."""
    context = parse_qualification_run_context(run_context)
    if context["variant"] != "Release":
        raise AggregationError("only Release context may derive a production aggregate")
    if not isinstance(reports, list) or not reports:
        raise AggregationError("component reports are missing")
    policy_entry = profile.get("trust_policy")
    if not isinstance(policy_entry, dict) or not isinstance(policy_entry.get("policy"), dict):
        raise AggregationError("production profile trust policy is missing")
    policy = policy_entry["policy"]
    policy_digest = trust_policy_digest(policy)
    if policy_entry.get("digest_hex") != policy_digest:
        raise AggregationError("production profile trust policy digest mismatch")
    profile_digest = blake3.blake3(canonical_json(profile)).hexdigest()

    seen: set[tuple[str, str | None]] = set()
    verified: list[tuple[str, dict[str, object]]] = []
    identity: dict[str, object] | None = None
    for receipt in reports:
        kind, payload = _verify_receipt(receipt, profile, policy)
        fingerprint = str(receipt["signer_fingerprint"])
        _validate_component_payload(
            kind, payload, context, profile_digest, policy_digest, fingerprint
        )
        discriminator = (
            str(payload.get("qualification_profile"))
            if kind == "resource-qualification"
            else None
        )
        report_key = (kind, discriminator)
        if report_key in seen:
            raise AggregationError(f"duplicate component report: {report_key}")
        seen.add(report_key)
        current_identity = {field: payload[field] for field in PRODUCTION_EQUALITY_BINDINGS}
        if identity is None:
            identity = current_identity
        else:
            for field in PRODUCTION_EQUALITY_BINDINGS:
                if current_identity[field] != identity[field]:
                    raise AggregationError(f"component {field} mismatch")
        verified.append((kind, payload))

    resource_profiles = {profile for kind, profile in seen if kind == "resource-qualification"}
    if resource_profiles != EXPECTED_RESOURCE_PROFILES:
        raise AggregationError("resource qualification report set is incomplete")
    single_kinds = {kind for kind, profile in seen if profile is None}
    if single_kinds != EXPECTED_SINGLE_KINDS:
        raise AggregationError("required Registry component report set is incomplete")
    assert identity is not None
    component_digests = sorted(
        blake3.blake3(canonical_json(receipt)).hexdigest() for receipt in reports
    )
    payload: dict[str, object] = {
        "qualification_context_variant": "Release",
        **{field: context[field] for field in RELEASE_CONTEXT_FIELDS},
        "base_candidate_bound": True,
        **identity,
        "command": ["production_qualification.py", "--pure-aggregate"],
        "result": True,
        "exit_oracles": {
            "all_component_signatures_valid": True,
            "all_component_results_true": True,
            "all_exact_candidate_bindings_identical": True,
            "all_required_fresh_reports_present_once": True,
            "release_context_is_closed": True,
        },
        "limitations": [
            "Registry-only subgate; never BASE-GATE-V1",
            "No carry-forward Registry evidence is accepted for Base v1",
        ],
        "component_receipt_blake3": component_digests,
        "registry_production_qualified": True,
        "base_gate_v1": False,
    }
    return create_signed_receipt(
        "production-aggregate", payload, aggregate_signing_key, policy
    )


def _read_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def _read_private_key(path: Path) -> Ed25519PrivateKey:
    try:
        value = path.read_text(encoding="ascii").strip()
    except OSError as error:
        raise AggregationError("aggregate private key could not be read") from error
    _hex(value, "aggregate private key")
    return Ed25519PrivateKey.from_private_bytes(bytes.fromhex(value))


def _write_json_atomic(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, name = tempfile.mkstemp(prefix=f".{path.name}.", suffix=".tmp", dir=path.parent)
    temporary = Path(name)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as handle:
            json.dump(value, handle, indent=2, sort_keys=True)
            handle.write("\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", type=Path, required=True)
    parser.add_argument("--run-context", type=Path, required=True)
    parser.add_argument("--receipt", type=Path, action="append", required=True)
    parser.add_argument("--aggregate-private-key", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        profile = _read_json(args.profile)
        if blake3.blake3(canonical_json(profile)).hexdigest() != FROZEN_PROFILE_BLAKE3:
            raise AggregationError("production profile does not match the frozen Task 19 contract")
        if profile.get("trust_policy", {}).get("digest_hex") != FROZEN_TRUST_POLICY_DIGEST:
            raise AggregationError("production profile trust policy is not frozen")
        signers = profile.get("trust_policy", {}).get("policy", {}).get("signers", [])
        if len(signers) != 1 or signers[0].get("public_key_hex") != FROZEN_SIGNER_PUBLIC_KEY:
            raise AggregationError("production profile Registry signer allowlist is not frozen")
        aggregate = aggregate_reports(
            [_read_json(path) for path in args.receipt],
            _read_json(args.run_context),
            profile,
            _read_private_key(args.aggregate_private_key),
        )
        _write_json_atomic(args.output, aggregate)
    except (OSError, ValueError, json.JSONDecodeError, AggregationError) as error:
        print(f"Concept Registry production aggregation failed: {error}", file=__import__("sys").stderr)
        return 2
    print(json.dumps(aggregate, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
