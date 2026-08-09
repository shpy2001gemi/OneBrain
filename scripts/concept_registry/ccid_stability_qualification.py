#!/usr/bin/env python3
"""Run the real CCID diff over request-bound bytes and sign its receipt."""

from __future__ import annotations

from pathlib import Path

import blake3
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

from ccid_stability_diff import generate_report
from production_qualification import (
    AggregationError,
    canonical_json,
    create_signed_receipt,
    signer_fingerprint,
    trust_policy_digest,
)

RELEASE_DIR = Path(__file__).resolve().parents[1] / "release"
if str(RELEASE_DIR) not in __import__("sys").path:
    __import__("sys").path.insert(0, str(RELEASE_DIR))
from verify_base_release_request import VerifiedQualificationContextV1  # noqa: E402


class CcidQualificationError(RuntimeError):
    """The measured CCID inputs do not match the verified release request."""


def _digest(path: Path) -> str:
    value = blake3.blake3()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            value.update(chunk)
    return value.hexdigest()


def qualify_ccid_stability(
    verified: VerifiedQualificationContextV1,
    old_input: Path,
    old_obr: Path,
    old_manifest: Path,
    candidate_input: Path,
    candidate_obr: Path,
    candidate_manifest: Path,
    *,
    sample_limit: int,
    work_dir: Path | None,
    invocation: list[str],
    signing_key: Ed25519PrivateKey,
    receipt_policy: dict[str, object],
) -> dict[str, object]:
    if not isinstance(verified, VerifiedQualificationContextV1):
        raise CcidQualificationError("closed verified release context is required")
    paths = {
        "old_input": old_input,
        "old_obr": old_obr,
        "old_manifest": old_manifest,
        "candidate_input": candidate_input,
        "candidate_obr": candidate_obr,
        "candidate_manifest": candidate_manifest,
    }
    expected = verified.bindings.get("ccid_inputs_blake3")
    if not isinstance(expected, dict) or set(expected) != set(paths):
        raise CcidQualificationError("verified request has no exact CCID input tuple")
    measured = {name: _digest(path) for name, path in paths.items()}
    if measured != expected:
        raise CcidQualificationError("CCID input bytes do not match the verified request")
    payload_artifacts = verified.bindings["candidate_payload_artifacts_blake3"]
    if (
        measured["candidate_obr"] != payload_artifacts["OBR:concepts.obr"]
        or measured["candidate_manifest"] != payload_artifacts["MANIFEST:concepts.obr.manifest.json"]
    ):
        raise CcidQualificationError("candidate CCID artifacts do not match the five-file payload tuple")
    if trust_policy_digest(receipt_policy) != verified.bindings["trust_policy_digest"]:
        raise CcidQualificationError("Registry receipt trust policy differs from the request")
    public = signing_key.public_key().public_bytes_raw()
    if signer_fingerprint(public) != verified.bindings["signer_fingerprint"]:
        raise CcidQualificationError("Registry receipt signer differs from the request")
    if (
        not invocation
        or not all(isinstance(value, str) and value for value in invocation)
        or any("private-key" in value.lower() or "secret" in value.lower() for value in invocation)
    ):
        raise CcidQualificationError("canonical sanitized invocation is invalid")
    report = generate_report(
        old_input, old_obr, old_manifest,
        candidate_input, candidate_obr, candidate_manifest,
        sample_limit=sample_limit, work_dir=work_dir,
    )
    context = verified.run_context
    payload: dict[str, object] = {
        **verified.bindings,
        "qualification_context_variant": "Release",
        "release_request_digest": context["release_request_digest"],
        "qualification_session_id": context["qualification_session_id"],
        "candidate_commit": context["candidate_commit"],
        "candidate_tree": context["candidate_tree"],
        "base_candidate_bound": verified.production,
        "command": invocation,
        "command_blake3": blake3.blake3(canonical_json(invocation)).hexdigest(),
        "result": report.get("qualified") is True,
        "exit_oracles": report["exit_oracles"],
        "limitations": ["Registry-only CCID evidence; never BASE-GATE-V1"],
        "ccid_report": report,
        "ccid_inputs_blake3": measured,
    }
    try:
        return create_signed_receipt("ccid-stability", payload, signing_key, receipt_policy)
    except AggregationError as error:
        raise CcidQualificationError(str(error)) from error
