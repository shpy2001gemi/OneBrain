#!/usr/bin/env python3
"""Create or verify a Task 28 binding for owner-produced Registry output.

The large source checkpoints are intentionally outside this gate.  The owner
builds the Registry once on the local workstation.  Task 28 signs the exact
five final output artifacts and every qualification host independently hashes
those same bytes before P5 or soak evidence can be accepted.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

import blake3
from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives.asymmetric.ed25519 import (
    Ed25519PrivateKey,
    Ed25519PublicKey,
)

from scripts.concept_registry.production_qualification import (
    FROZEN_SIGNER_PUBLIC_KEY,
    FROZEN_TRUST_POLICY_DIGEST,
    _policy_signers,
    canonical_json,
    signer_fingerprint,
    trust_policy_digest,
)
from scripts.release.verify_base_release_request import (
    VerifiedQualificationContextV2,
    blake3_file,
    canonical_compatibility_tuple_bytes,
    verify_task28_release_request,
)


ROOT = Path(__file__).resolve().parents[2]
PROFILE_PATH = ROOT / "src/test-vectors/vnext/concept-registry-production-qualification-v1.json"
BINDING_KIND = "prebuilt-artifact-binding"
BINDING_ENVELOPE_FORMAT = "onebrain/task28-prebuilt-registry-envelope/1"
BINDING_USAGE = "registry-qualification-receipt"
BINDING_DOMAIN = b"onebrain:task28:prebuilt-registry-binding:1\0"
PAYLOAD_FORMAT = "onebrain/task28-prebuilt-registry-binding/1"
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
ARTIFACT_NAMES = (
    "concepts.obr",
    "concepts.obr.labels.idx",
    "concepts.obr.ccids.idx",
    "concepts.obr.manifest.json",
    "concepts.obr.verification.json",
)
DATA_ARTIFACT_NAMES = ARTIFACT_NAMES[:4]
MIN_REGISTRY_DATA_BYTES = 2_200_000_000
MAX_REGISTRY_DATA_BYTES = 2_500_000_000
PAYLOAD_FIELDS = {
    "format",
    "qualification_context_variant",
    "release_request_digest",
    "qualification_session_id",
    "candidate_commit",
    "candidate_tree",
    "base_candidate_bound",
    "evidence_tier",
    "command",
    "result",
    "registry_origin",
    "source_archives_reprocessed",
    "registry_data_bytes",
    "registry_entry_count",
    "registry_label_count",
    "candidate_semantic_digest",
    "artifact_tuple_digest",
    "registry_semantic_digest",
    "registry_artifact_tuple_digest",
    "release_aggregate_root",
    "candidate_payload_artifacts_blake3",
    "verification_blake3",
    "trust_policy_digest",
    "signer_fingerprint",
    "limitations",
}


class PrebuiltRegistryError(RuntimeError):
    """The final Registry output or its signed binding is invalid."""


def _json(path: Path, label: str) -> dict[str, object]:
    try:
        value = json.loads(path.read_bytes())
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise PrebuiltRegistryError(f"{label} is invalid JSON") from error
    if not isinstance(value, dict):
        raise PrebuiltRegistryError(f"{label} must be a JSON object")
    return value


def _canonical_binding(path: Path) -> dict[str, object]:
    value = _json(path, "prebuilt Registry binding")
    encoded = path.read_bytes()
    if encoded not in {canonical_json(value), canonical_json(value) + b"\n"}:
        raise PrebuiltRegistryError("prebuilt Registry binding is not canonical")
    return value


def _regular(root: Path, name: str) -> Path:
    path = root / name
    try:
        resolved_root = root.resolve(strict=True)
        resolved = path.resolve(strict=True)
    except OSError as error:
        raise PrebuiltRegistryError(f"Registry artifact is missing: {name}") from error
    if path.is_symlink() or not resolved.is_file() or resolved.parent != resolved_root:
        raise PrebuiltRegistryError(f"Registry artifact is not a direct regular file: {name}")
    return resolved


def _positive_integer(value: object, label: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
        raise PrebuiltRegistryError(f"{label} must be a positive integer")
    return value


def _hex(value: object, label: str, *, length: int = 64) -> str:
    if (
        not isinstance(value, str)
        or len(value) != length
        or any(character not in "0123456789abcdef" for character in value)
    ):
        raise PrebuiltRegistryError(f"{label} must be 32-byte lowercase hexadecimal")
    return value


def _profile_policy() -> dict[str, object]:
    profile = _json(PROFILE_PATH, "Registry production profile")
    policy = profile.get("trust_policy", {}).get("policy")
    if not isinstance(policy, dict):
        raise PrebuiltRegistryError("Registry trust policy is missing")
    signers = _policy_signers(policy)
    if (
        trust_policy_digest(policy) != FROZEN_TRUST_POLICY_DIGEST
        or set(signers) != {FROZEN_SIGNER_PUBLIC_KEY}
    ):
        raise PrebuiltRegistryError("Registry trust policy or signer is not frozen")
    return policy


def inspect_prebuilt_registry(
    registry_root: Path,
    *,
    minimum_data_bytes: int = MIN_REGISTRY_DATA_BYTES,
    maximum_data_bytes: int = MAX_REGISTRY_DATA_BYTES,
) -> dict[str, object]:
    root = registry_root.resolve(strict=True)
    if not root.is_dir() or registry_root.is_symlink():
        raise PrebuiltRegistryError("prebuilt Registry root must be a real directory")
    paths = {name: _regular(root, name) for name in ARTIFACT_NAMES}
    rows = [
        {"name": name, "size": paths[name].stat().st_size, "blake3": blake3_file(paths[name])}
        for name in ARTIFACT_NAMES
    ]
    measured = {str(row["name"]): row for row in rows}
    data_bytes = sum(int(measured[name]["size"]) for name in DATA_ARTIFACT_NAMES)
    if not minimum_data_bytes <= data_bytes <= maximum_data_bytes:
        raise PrebuiltRegistryError("prebuilt Registry data is outside the frozen 2.2--2.5 GB interval")

    manifest = _json(paths["concepts.obr.manifest.json"], "Registry manifest")
    verification = _json(paths["concepts.obr.verification.json"], "Registry verification")
    if manifest.get("manifest_version") != 1 or manifest.get("builder_version") != "onebrain-concept-registry-builder/1":
        raise PrebuiltRegistryError("Registry manifest identity is unsupported")
    entry_count = _positive_integer(manifest.get("entry_count"), "Registry entry_count")
    label_count = _positive_integer(manifest.get("label_count"), "Registry label_count")
    if manifest.get("obr_blake3") != measured["concepts.obr"]["blake3"]:
        raise PrebuiltRegistryError("Registry manifest OBR digest differs from final bytes")
    for section, name in (
        ("label_index", "concepts.obr.labels.idx"),
        ("ccid_index", "concepts.obr.ccids.idx"),
    ):
        row = manifest.get(section)
        if not isinstance(row, dict) or row.get("blake3") != measured[name]["blake3"] or row.get("file_size") != measured[name]["size"]:
            raise PrebuiltRegistryError(f"Registry manifest {section} differs from final bytes")
    if verification.get("obr_blake3") != measured["concepts.obr"]["blake3"] or verification.get("file_size") != measured["concepts.obr"]["size"]:
        raise PrebuiltRegistryError("Registry verification OBR tuple differs from final bytes")
    for section, name in (
        ("label_index", "concepts.obr.labels.idx"),
        ("ccid_index", "concepts.obr.ccids.idx"),
    ):
        row = verification.get(section)
        if not isinstance(row, dict) or row.get("blake3") != measured[name]["blake3"] or row.get("file_size") != measured[name]["size"]:
            raise PrebuiltRegistryError(f"Registry verification {section} differs from final bytes")

    semantic_projection = {
        "builder_version": manifest["builder_version"],
        "dedup_policy_version": manifest.get("dedup_policy_version"),
        "entry_count": entry_count,
        "label_count": label_count,
        "obr_schema_version": manifest.get("obr_schema_version"),
        "sources": manifest.get("sources"),
        "payload_blake3": {
            name: measured[name]["blake3"]
            for name in ARTIFACT_NAMES[:3]
        },
    }
    registry_semantic_digest = blake3.blake3(
        canonical_json(semantic_projection),
        derive_key_context="onebrain:task28:prebuilt-registry-semantic:1",
    ).hexdigest()
    registry_artifact_tuple_digest = blake3.blake3(
        canonical_json(rows),
        derive_key_context="onebrain:task28:prebuilt-registry-artifacts:1",
    ).hexdigest()
    release_aggregate_root = blake3.blake3(
        canonical_json(
            {
                "registry_artifact_tuple_digest": registry_artifact_tuple_digest,
                "registry_semantic_digest": registry_semantic_digest,
                "registry_data_bytes": data_bytes,
            }
        ),
        derive_key_context="onebrain:task28:prebuilt-registry-root:1",
    ).hexdigest()
    return {
        "rows": rows,
        "registry_data_bytes": data_bytes,
        "registry_entry_count": entry_count,
        "registry_label_count": label_count,
        "registry_semantic_digest": registry_semantic_digest,
        "registry_artifact_tuple_digest": registry_artifact_tuple_digest,
        "release_aggregate_root": release_aggregate_root,
        "candidate_payload_artifacts_blake3": {
            "OBR:concepts.obr": measured["concepts.obr"]["blake3"],
            "LABEL_INDEX:concepts.obr.labels.idx": measured["concepts.obr.labels.idx"]["blake3"],
            "CCID_INDEX:concepts.obr.ccids.idx": measured["concepts.obr.ccids.idx"]["blake3"],
            "MANIFEST:concepts.obr.manifest.json": measured["concepts.obr.manifest.json"]["blake3"],
        },
        "verification_blake3": measured["concepts.obr.verification.json"]["blake3"],
}


def inspect_base_candidate_identity(
    verified: VerifiedQualificationContextV2,
    candidate_semantic_evidence: Path,
) -> dict[str, str]:
    """Derive Base semantic/artifact identity independently of Registry bytes."""

    try:
        encoded = candidate_semantic_evidence.read_bytes()
        value = json.loads(encoded)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise PrebuiltRegistryError("Base candidate semantic evidence is invalid JSON") from error
    if encoded != canonical_json(value):
        raise PrebuiltRegistryError("Base candidate semantic evidence is not canonical JSON")
    commit = value.get("base_commit") if isinstance(value, dict) else None
    if not isinstance(commit, dict) or commit.get("hex") != verified.run_context["candidate_commit"]:
        raise PrebuiltRegistryError("Base candidate semantic evidence commit mismatch")
    required_targets = verified.request.get("required_targets")
    if (
        not isinstance(required_targets, dict)
        or value.get("target_triple") != required_targets.get("linux")
    ):
        raise PrebuiltRegistryError("Base candidate semantic evidence is not the signed Linux target")
    try:
        semantic_bytes = canonical_compatibility_tuple_bytes(
            value, include_artifact_fields=False
        )
        artifact_bytes = canonical_compatibility_tuple_bytes(
            value, include_artifact_fields=True
        )
    except RuntimeError as error:
        raise PrebuiltRegistryError(f"Base candidate semantic evidence is invalid: {error}") from error
    return {
        "candidate_semantic_digest": blake3.blake3(
            semantic_bytes,
            derive_key_context="onebrain:base:candidate-semantic:1\0",
        ).hexdigest(),
        "artifact_tuple_digest": blake3.blake3(
            artifact_bytes,
            derive_key_context="onebrain:base:artifact-tuple:1\0",
        ).hexdigest(),
    }


def create_prebuilt_registry_binding(
    verified: VerifiedQualificationContextV2,
    registry_root: Path,
    candidate_semantic_evidence: Path,
    signing_key: Ed25519PrivateKey,
    *,
    minimum_data_bytes: int = MIN_REGISTRY_DATA_BYTES,
    maximum_data_bytes: int = MAX_REGISTRY_DATA_BYTES,
) -> dict[str, object]:
    if not isinstance(verified, VerifiedQualificationContextV2) or not verified.production:
        raise PrebuiltRegistryError("production verified Task 28 context v2 is required")
    measured = inspect_prebuilt_registry(
        registry_root,
        minimum_data_bytes=minimum_data_bytes,
        maximum_data_bytes=maximum_data_bytes,
    )
    candidate_identity = inspect_base_candidate_identity(
        verified, candidate_semantic_evidence
    )
    policy = _profile_policy()
    public = signing_key.public_key().public_bytes_raw()
    fingerprint = signer_fingerprint(public)
    payload = {
        "format": PAYLOAD_FORMAT,
        "qualification_context_variant": "Release",
        "release_request_digest": verified.run_context["release_request_digest"],
        "qualification_session_id": verified.run_context["qualification_session_id"],
        "candidate_commit": verified.run_context["candidate_commit"],
        "candidate_tree": verified.run_context["candidate_tree"],
        "base_candidate_bound": True,
        "evidence_tier": "production-reference",
        "command": "verify-owner-produced-prebuilt-registry-output",
        "result": True,
        "registry_origin": "owner-local-prebuilt-output",
        "source_archives_reprocessed": False,
        "registry_data_bytes": measured["registry_data_bytes"],
        "registry_entry_count": measured["registry_entry_count"],
        "registry_label_count": measured["registry_label_count"],
        "candidate_semantic_digest": candidate_identity["candidate_semantic_digest"],
        "artifact_tuple_digest": candidate_identity["artifact_tuple_digest"],
        "registry_semantic_digest": measured["registry_semantic_digest"],
        "registry_artifact_tuple_digest": measured["registry_artifact_tuple_digest"],
        "release_aggregate_root": measured["release_aggregate_root"],
        "candidate_payload_artifacts_blake3": measured["candidate_payload_artifacts_blake3"],
        "verification_blake3": measured["verification_blake3"],
        "trust_policy_digest": trust_policy_digest(policy),
        "signer_fingerprint": fingerprint,
        "limitations": [
            "source-checkpoint-archives-not-reprocessed-on-qualification-hosts",
            "registry-resource-profile-rerun-not-part-of-three-vps-network-gate",
        ],
    }
    signers = _policy_signers(policy)
    public_hex = public.hex()
    if signers.get(public_hex) != fingerprint:
        raise PrebuiltRegistryError("prebuilt Registry signing key is not allowlisted")
    envelope: dict[str, object] = {
        "format": BINDING_ENVELOPE_FORMAT,
        "receipt_kind": BINDING_KIND,
        "usage": BINDING_USAGE,
        "payload": payload,
        "signer_public_key": public_hex,
        "signer_fingerprint": fingerprint,
        "trust_policy_digest": trust_policy_digest(policy),
        "signature": "",
    }
    envelope["signature"] = signing_key.sign(
        BINDING_DOMAIN + blake3.blake3(canonical_json(envelope)).digest()
    ).hex()
    return envelope


def verify_prebuilt_registry_binding(
    verified: VerifiedQualificationContextV2,
    registry_root: Path,
    candidate_semantic_evidence: Path,
    binding: object,
    *,
    minimum_data_bytes: int = MIN_REGISTRY_DATA_BYTES,
    maximum_data_bytes: int = MAX_REGISTRY_DATA_BYTES,
) -> dict[str, object]:
    if not isinstance(verified, VerifiedQualificationContextV2) or not verified.production:
        raise PrebuiltRegistryError("production verified Task 28 context v2 is required")
    if not isinstance(binding, dict) or set(binding) != ENVELOPE_FIELDS:
        raise PrebuiltRegistryError("prebuilt Registry binding envelope fields are not closed")
    if binding.get("format") != BINDING_ENVELOPE_FORMAT or binding.get("usage") != BINDING_USAGE or binding.get("receipt_kind") != BINDING_KIND:
        raise PrebuiltRegistryError("prebuilt Registry binding envelope identity mismatch")
    payload = binding.get("payload")
    if not isinstance(payload, dict) or set(payload) != PAYLOAD_FIELDS:
        raise PrebuiltRegistryError("prebuilt Registry binding payload fields are not closed")
    policy = _profile_policy()
    public_hex = _hex(binding.get("signer_public_key"), "Registry signer public key")
    fingerprint = _hex(binding.get("signer_fingerprint"), "Registry signer fingerprint")
    signers = _policy_signers(policy)
    if signers.get(public_hex) != fingerprint or signer_fingerprint(bytes.fromhex(public_hex)) != fingerprint:
        raise PrebuiltRegistryError("prebuilt Registry signer is not allowlisted")
    policy_digest = trust_policy_digest(policy)
    if binding.get("trust_policy_digest") != policy_digest:
        raise PrebuiltRegistryError("prebuilt Registry trust-policy digest mismatch")
    unsigned = dict(binding)
    signature = _hex(unsigned.get("signature"), "Registry binding signature", length=128)
    unsigned["signature"] = ""
    try:
        Ed25519PublicKey.from_public_bytes(bytes.fromhex(public_hex)).verify(
            bytes.fromhex(signature),
            BINDING_DOMAIN + blake3.blake3(canonical_json(unsigned)).digest(),
        )
    except (ValueError, InvalidSignature) as error:
        raise PrebuiltRegistryError("prebuilt Registry binding signature is invalid") from error
    expected_context = {
        "release_request_digest": verified.run_context["release_request_digest"],
        "qualification_session_id": verified.run_context["qualification_session_id"],
        "candidate_commit": verified.run_context["candidate_commit"],
        "candidate_tree": verified.run_context["candidate_tree"],
    }
    for field, expected in expected_context.items():
        if payload.get(field) != expected:
            raise PrebuiltRegistryError(f"prebuilt Registry {field} mismatch")
    if (
        payload.get("format") != PAYLOAD_FORMAT
        or payload.get("qualification_context_variant") != "Release"
        or payload.get("base_candidate_bound") is not True
        or payload.get("evidence_tier") != "production-reference"
        or payload.get("command") != "verify-owner-produced-prebuilt-registry-output"
        or payload.get("result") is not True
        or payload.get("registry_origin") != "owner-local-prebuilt-output"
        or payload.get("source_archives_reprocessed") is not False
        or payload.get("trust_policy_digest") != policy_digest
        or payload.get("signer_fingerprint") != fingerprint
    ):
        raise PrebuiltRegistryError("prebuilt Registry binding claims are invalid")
    measured = inspect_prebuilt_registry(
        registry_root,
        minimum_data_bytes=minimum_data_bytes,
        maximum_data_bytes=maximum_data_bytes,
    )
    candidate_identity = inspect_base_candidate_identity(
        verified, candidate_semantic_evidence
    )
    for field, expected in candidate_identity.items():
        if payload.get(field) != expected:
            raise PrebuiltRegistryError(f"prebuilt Registry Base {field} mismatch")
    for field in (
        "registry_data_bytes",
        "registry_entry_count",
        "registry_label_count",
        "registry_semantic_digest",
        "registry_artifact_tuple_digest",
        "release_aggregate_root",
        "candidate_payload_artifacts_blake3",
        "verification_blake3",
    ):
        if payload.get(field) != measured[field]:
            raise PrebuiltRegistryError(f"prebuilt Registry measured {field} mismatch")
    expected_limitations = [
        "source-checkpoint-archives-not-reprocessed-on-qualification-hosts",
        "registry-resource-profile-rerun-not-part-of-three-vps-network-gate",
    ]
    if payload.get("limitations") != expected_limitations:
        raise PrebuiltRegistryError("prebuilt Registry limitations are not closed")
    return dict(payload)


def _read_private_key(path: Path) -> Ed25519PrivateKey:
    encoded = path.read_bytes()
    stripped = encoded.strip()
    if len(stripped) == 64:
        try:
            raw = bytes.fromhex(stripped.decode("ascii"))
        except (UnicodeDecodeError, ValueError) as error:
            raise PrebuiltRegistryError("Registry signing key is not canonical hex") from error
        if raw.hex().encode("ascii") != stripped:
            raise PrebuiltRegistryError("Registry signing key hex is not canonical")
    elif len(encoded) == 32:
        raw = encoded
    else:
        raise PrebuiltRegistryError("Registry signing key must be 32 raw bytes or 64 lowercase hex characters")
    return Ed25519PrivateKey.from_private_bytes(raw)


def _write_new(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("xb") as stream:
        stream.write(canonical_json(value) + b"\n")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    for command in ("prepare", "verify"):
        child = subparsers.add_parser(command)
        for name in (
            "release-request",
            "release-signature",
            "base-policy",
            "base-gpg-home",
            "registry-root",
            "candidate-semantic-evidence",
            "output",
        ):
            child.add_argument(f"--{name}", type=Path, required=True)
        child.add_argument("--candidate-root", type=Path, default=ROOT)
        child.add_argument("--gpg-executable", type=Path, default=Path("/usr/bin/gpg"))
        if command == "prepare":
            child.add_argument("--signing-key", type=Path, required=True)
        else:
            child.add_argument("--binding", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        verified = verify_task28_release_request(
            args.release_request,
            args.release_signature,
            args.base_policy,
            gpg_home=args.base_gpg_home,
            gpg_executable=args.gpg_executable,
            candidate_root=args.candidate_root,
        )
        if args.command == "prepare":
            result = create_prebuilt_registry_binding(
                verified,
                args.registry_root,
                args.candidate_semantic_evidence,
                _read_private_key(args.signing_key),
            )
        else:
            result = verify_prebuilt_registry_binding(
                verified,
                args.registry_root,
                args.candidate_semantic_evidence,
                _canonical_binding(args.binding),
            )
        _write_new(args.output, result)
    except (OSError, KeyError, TypeError, ValueError, RuntimeError, PrebuiltRegistryError) as error:
        print(f"Task 28 prebuilt Registry failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
