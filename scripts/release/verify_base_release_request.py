#!/usr/bin/env python3
"""Verify a canonical, detached-signed Base v1 release request.

The verifier owns the transition from untrusted JSON/files to a closed
VerifiedQualificationContextV1.  Callers receive derived context and Registry
bindings; they cannot supply or override either value.
"""

from __future__ import annotations

import argparse
import json
import struct
import sys
import subprocess
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

import blake3
from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey


APPROVER_POLICY_DIGEST_CONTEXT = "onebrain:base-v1:qualification-approver-policy:1"
APPROVER_POLICY_DIGEST = "2e7cc2dacafad658ab5fe4e1536a4b92590f788c9c9e5a450d123930d65cfbd6"
FROZEN_APPROVER_POLICY: dict[str, object] = {
    "algorithm": "OpenPGP-Ed25519",
    "allowed_usages": ["base-release-request"],
    "format": "onebrain/base-v1-qualification-approver-policy/1",
    "role": "qualification-approver",
    "signers": [{
        "created_utc": "2026-08-09T13:27:27Z",
        "expires_utc": "2028-08-08T13:27:27Z",
        "fingerprint": "CB3FF16A1A2C8B017B5D83DF59DC9C079E00928B",
        "key_id": "59DC9C079E00928B",
        "public_key_packet_blake3": "ecee4527ed22908e0afc3a859492f7e0be7d4f4ccef087dd2781673364f39108",
    }],
    "valid_unlisted_signature": "reject",
    "verification": {
        "fingerprint_source": "gpg-status-fd-VALIDSIG-full-primary-fingerprint",
        "trust_model": "explicit-allowlist",
    },
}

REQUEST_FIELDS = {
    "format", "usage", "qualification_session_id", "candidate",
    "qualification_approver_fingerprint", "trust_policy_digest",
    "required_targets", "production_profile_blake3", "production_vector_blake3",
    "append_only_idl_history_root", "created_utc", "expires_utc",
    "evidence_root_uri", "candidate_tooling_blake3", "registry_candidate",
    "reference_environment",
}
TOOLING_FIELDS = {
    "qualifier", "request", "clean_worktree", "release_wrapper", "verifier",
    "signer_policy",
}
ARTIFACT_FIELDS = {
    "OBR:concepts.obr", "LABEL_INDEX:concepts.obr.labels.idx",
    "CCID_INDEX:concepts.obr.ccids.idx", "MANIFEST:concepts.obr.manifest.json",
    "SPDX_SBOM:sbom.spdx.json",
}


class ReleaseRequestError(RuntimeError):
    """The external request cannot establish a verified qualification context."""


@dataclass(frozen=True)
class VerifiedQualificationContextV1:
    request_digest: str
    signer_fingerprint: str
    trust_policy_digest: str
    run_context: dict[str, object]
    bindings: dict[str, object]
    tooling_blake3: dict[str, str]
    production: bool

    def as_dict(self) -> dict[str, object]:
        return {
            "format": "onebrain/verified-qualification-context/1",
            "production": self.production,
            "request_digest": self.request_digest,
            "signer_fingerprint": self.signer_fingerprint,
            "trust_policy_digest": self.trust_policy_digest,
            "run_context": self.run_context,
            "bindings": self.bindings,
            "tooling_blake3": self.tooling_blake3,
        }


def blake3_file(path: Path) -> str:
    digest = blake3.blake3()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def python_executable_path() -> Path:
    """Return the byte-bearing interpreter image (Windows aliases are metadata)."""
    executable = Path(sys.executable)
    try:
        if executable.is_file() and executable.stat().st_size:
            return executable
    except OSError:
        pass
    packaged = Path(sys.prefix) / ("python.exe" if sys.platform == "win32" else "bin/python3")
    if not packaged.is_file():
        raise ReleaseRequestError("running Python executable image is unavailable")
    return packaged


def _verify_authenticated_tooling(
    request: dict[str, Any],
    policy_bytes: bytes,
    *,
    python_path: Path,
    gpg_path: Path,
    verifier_path: Path,
) -> None:
    expected = request["reference_environment"]
    tooling = request["candidate_tooling_blake3"]
    measured = {
        "Python executable": (blake3_file(python_path), expected["python_executable_blake3"]),
        "GPG executable": (blake3_file(gpg_path), expected["gpg_executable_blake3"]),
        "verifier tooling": (blake3_file(verifier_path), tooling["verifier"]),
        "signer policy tooling": (blake3.blake3(policy_bytes).hexdigest(), tooling["signer_policy"]),
    }
    for name, (actual, signed) in measured.items():
        if actual != signed:
            raise ReleaseRequestError(f"signed {name} digest mismatch")


def _verify_registry_candidate_measurements(
    verified: VerifiedQualificationContextV1,
    *,
    git_executable: Path,
    candidate_root: Path,
    registry_root: Path,
    release_id: str,
    candidate_semantic_evidence: Path,
    production_profile: Path,
    production_vector: Path,
    append_only_idl_history: Path,
    candidate_tooling: dict[str, Path],
    payload_artifacts: dict[str, Path],
    release_stamp: Path,
    probe: Path,
    probe_signature: Path,
    executable: Path,
    rust_toolchain_evidence: Path,
    runner_image_evidence: Path,
    target_triple: str,
) -> dict[str, object]:
    """Measure every candidate-owned Registry byte named by the request."""
    if not isinstance(verified, VerifiedQualificationContextV1):
        raise ReleaseRequestError("closed verified release context is required")
    root = candidate_root.resolve(strict=True)
    git_values: dict[str, str] = {}
    for field, revision in (
        ("object_format", "--show-object-format"),
        ("commit", "HEAD"),
        ("tree", "HEAD^{tree}"),
    ):
        command = [str(git_executable), "-C", str(root), "rev-parse", revision]
        completed = subprocess.run(command, capture_output=True, text=True, check=False)
        if completed.returncode != 0:
            raise ReleaseRequestError(f"candidate Git {field} could not be measured")
        git_values[field] = completed.stdout.strip()
    expected_git = {
        "object_format": "sha1" if len(verified.run_context["candidate_commit"]) == 40 else "sha256",
        "commit": verified.run_context["candidate_commit"],
        "tree": verified.run_context["candidate_tree"],
    }
    if git_values != expected_git:
        raise ReleaseRequestError("measured candidate commit/tree/object format differs from request")
    try:
        semantic_bytes = candidate_semantic_evidence.read_bytes()
        compatibility_tuple = json.loads(semantic_bytes)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ReleaseRequestError("candidate semantic tuple evidence is invalid") from error
    if semantic_bytes != canonical_json(compatibility_tuple):
        raise ReleaseRequestError("candidate semantic tuple evidence is not canonical JSON")
    commit = compatibility_tuple.get("base_commit") if isinstance(compatibility_tuple, dict) else None
    if not isinstance(commit, dict) or commit.get("hex") != verified.run_context["candidate_commit"]:
        raise ReleaseRequestError("candidate semantic tuple commit differs from signed request")
    semantic_tuple_bytes = canonical_compatibility_tuple_bytes(
        compatibility_tuple, include_artifact_fields=False
    )
    semantic = blake3.blake3(
        semantic_tuple_bytes,
        derive_key_context="onebrain:base:candidate-semantic:1\0",
    ).hexdigest()
    if semantic != verified.bindings["candidate_semantic_digest"]:
        raise ReleaseRequestError("measured candidate semantic digest differs from request")
    measured_toolchain_digest = blake3_file(rust_toolchain_evidence)
    if (
        compatibility_tuple.get("target_triple") != target_triple
        or compatibility_tuple.get("toolchain")
        != {"kind": "known", "hex": measured_toolchain_digest}
    ):
        raise ReleaseRequestError("artifact tuple target/toolchain differs from measured evidence")
    artifact_tuple_bytes = canonical_compatibility_tuple_bytes(
        compatibility_tuple, include_artifact_fields=True
    )
    artifact_tuple = blake3.blake3(
        artifact_tuple_bytes,
        derive_key_context="onebrain:base:artifact-tuple:1\0",
    ).hexdigest()
    targets = verified.bindings["required_targets"]
    if (
        target_triple not in targets
        or artifact_tuple != verified.bindings["artifact_tuple_digest"]
        or artifact_tuple != targets[target_triple]
    ):
        raise ReleaseRequestError("derived target artifact tuple differs from signed request")
    try:
        profile_value = json.loads(production_profile.read_bytes())
        vector_value = json.loads(production_vector.read_bytes())
    except (OSError, json.JSONDecodeError) as error:
        raise ReleaseRequestError("production profile/vector evidence is invalid") from error
    if blake3.blake3(canonical_json(profile_value)).hexdigest() != verified.bindings["production_profile_blake3"]:
        raise ReleaseRequestError("measured production profile differs from request")
    if blake3.blake3(canonical_json(vector_value)).hexdigest() != verified.bindings["production_vector_blake3"]:
        raise ReleaseRequestError("measured production vector differs from request")
    if append_only_idl_history.read_text(encoding="ascii").strip() != verified.bindings["append_only_idl_history_root"]:
        raise ReleaseRequestError("measured append-only IDL history root differs from request")
    if set(candidate_tooling) != TOOLING_FIELDS:
        raise ReleaseRequestError("measured candidate tooling map is not exact")
    measured_tooling = {name: blake3_file(path) for name, path in candidate_tooling.items()}
    if measured_tooling != verified.tooling_blake3:
        raise ReleaseRequestError("measured candidate tooling differs from request")
    if set(payload_artifacts) != ARTIFACT_FIELDS:
        raise ReleaseRequestError("measured Registry payload tuple is not exact")
    release_dir = registry_root.resolve(strict=True) / "releases" / release_id
    if release_stamp.resolve(strict=True) != (release_dir / "release.stamp.json").resolve(strict=True):
        raise ReleaseRequestError("release stamp path is not the measured installed release")
    expected_names = {name.split(":", 1)[1] for name in ARTIFACT_FIELDS}
    if {
        path.resolve(strict=True).name for path in payload_artifacts.values()
    } != expected_names or any(path.resolve(strict=True).parent != release_dir for path in payload_artifacts.values()):
        raise ReleaseRequestError("payload paths are not the measured installed release")
    measured_artifacts = {
        name: blake3_file(path) for name, path in payload_artifacts.items()
    }
    if measured_artifacts != verified.bindings["candidate_payload_artifacts_blake3"]:
        raise ReleaseRequestError("measured Registry payload bytes differ from request")
    try:
        stamp = json.loads(release_stamp.read_bytes())
    except (OSError, json.JSONDecodeError) as error:
        raise ReleaseRequestError("installed release stamp is invalid") from error
    stamp_artifacts = {
        f"{artifact['role']}:{artifact['relative_path']}": artifact["blake3"]
        for artifact in stamp.get("artifacts", [])
        if isinstance(artifact, dict)
    }
    if stamp.get("release_id") != release_id or stamp.get("artifact_root") != verified.bindings["release_aggregate_root"]:
        raise ReleaseRequestError("installed release root differs from request")
    if stamp_artifacts != measured_artifacts:
        raise ReleaseRequestError("installed release stamp artifact tuple differs from measured bytes")
    states = sorted((registry_root / "state").glob("state-*.json"))
    if not states:
        raise ReleaseRequestError("installed Registry has no active state generation")
    try:
        state = json.loads(states[-1].read_bytes())
    except (OSError, json.JSONDecodeError) as error:
        raise ReleaseRequestError("active Registry state generation is invalid") from error
    if state.get("active_release") != release_id or state.get("generation") != verified.bindings["registry_generation"]:
        raise ReleaseRequestError("active Registry root/generation differs from request")
    measured = {
        "candidate_payload_artifacts_blake3": measured_artifacts,
        "release_stamp_blake3": blake3_file(release_stamp),
        "probe_blake3": blake3_file(probe),
        "probe_signature": blake3_file(probe_signature),
        "executable_blake3": blake3_file(executable),
        "rust_toolchain_digest": measured_toolchain_digest,
        "runner_image_digest": blake3_file(runner_image_evidence),
        "target_triple": target_triple,
    }
    for field, value in measured.items():
        if value != verified.bindings[field]:
            raise ReleaseRequestError(f"measured {field} differs from request")
    public_hex = verified.bindings["probe_signer_public_key"]
    expected_fingerprint = verified.bindings["probe_signer_fingerprint"]
    fingerprint_hasher = blake3.blake3(
        derive_key_context="onebrain:concept-registry:signer-fingerprint:1"
    )
    fingerprint_hasher.update(bytes.fromhex(public_hex))
    if fingerprint_hasher.hexdigest() != expected_fingerprint:
        raise ReleaseRequestError("probe signer public key/fingerprint mismatch")
    try:
        signature_hex = probe_signature.read_text(encoding="ascii").strip()
        signature = bytes.fromhex(signature_hex)
        message = b"onebrain:concept-registry-probe:1\0" + blake3.blake3(probe.read_bytes()).digest()
        Ed25519PublicKey.from_public_bytes(bytes.fromhex(public_hex)).verify(signature, message)
    except (OSError, UnicodeError, ValueError, InvalidSignature) as error:
        raise ReleaseRequestError("probe detached signature identity verification failed") from error
    return measured


def verify_registry_candidate_measurements(
    verified: VerifiedQualificationContextV1,
    *,
    candidate_root: Path,
    registry_root: Path,
    release_id: str,
    candidate_semantic_evidence: Path,
    production_profile: Path,
    production_vector: Path,
    append_only_idl_history: Path,
    candidate_tooling: dict[str, Path],
    payload_artifacts: dict[str, Path],
    release_stamp: Path,
    probe: Path,
    probe_signature: Path,
    executable: Path,
    rust_toolchain_evidence: Path,
    runner_image_evidence: Path,
    target_triple: str,
) -> dict[str, object]:
    """Production candidate measurement with fixed Linux Git executable."""
    if not verified.production:
        raise ReleaseRequestError("production candidate measurement requires production verified context")
    if not sys.platform.startswith("linux") or not Path("/usr/bin/git").is_file():
        raise ReleaseRequestError("production candidate measurement requires fixed Linux Git")
    return _verify_registry_candidate_measurements(
        verified,
        git_executable=Path("/usr/bin/git"), candidate_root=candidate_root,
        registry_root=registry_root, release_id=release_id,
        candidate_semantic_evidence=candidate_semantic_evidence,
        production_profile=production_profile, production_vector=production_vector,
        append_only_idl_history=append_only_idl_history,
        candidate_tooling=candidate_tooling, payload_artifacts=payload_artifacts,
        release_stamp=release_stamp, probe=probe, probe_signature=probe_signature,
        executable=executable, rust_toolchain_evidence=rust_toolchain_evidence,
        runner_image_evidence=runner_image_evidence, target_triple=target_triple,
    )


def verify_registry_candidate_measurements_for_test_nonproduction(
    verified: VerifiedQualificationContextV1,
    *,
    git_executable: Path,
    **measurements: Any,
) -> dict[str, object]:
    """Test-only exact measurement; never upgrades the verified production flag."""
    if verified.production:
        raise ReleaseRequestError("test measurement helper rejects production contexts")
    return _verify_registry_candidate_measurements(
        verified, git_executable=git_executable, **measurements
    )


def canonical_json(value: object) -> bytes:
    return json.dumps(
        value, ensure_ascii=False, sort_keys=True, separators=(",", ":")
    ).encode("utf-8")


COMPATIBILITY_TUPLE_FIELDS = (
    "base_version", "base_commit", "canonical_schema_digest",
    "domain_registry_digest", "resource_registry_digest", "storage_schema",
    "archive_profile", "migration_profile", "registry_profile",
    "registry_profile_digest", "wire_session", "product_api", "c_abi",
    "feature_set_digest", "target_triple", "toolchain",
)


def canonical_compatibility_tuple_bytes(
    value: object, *, include_artifact_fields: bool
) -> bytes:
    """Encode the frozen BaseCompatibilityTuple field framing byte-for-byte."""
    tuple_value = _closed(value, set(COMPATIBILITY_TUPLE_FIELDS), "Base compatibility tuple")

    def unsigned(number: object, width: int, field: str) -> bytes:
        if isinstance(number, bool) or not isinstance(number, int) or number < 0:
            raise ReleaseRequestError(f"{field} must be an unsigned integer")
        try:
            return number.to_bytes(width, "little")
        except OverflowError as error:
            raise ReleaseRequestError(f"{field} is out of range") from error

    def release(raw: object) -> bytes:
        item = _closed(raw, {"major", "minor", "patch", "prerelease"}, "base_version")
        encoded = b"".join(unsigned(item[name], 2, f"base_version.{name}") for name in ("major", "minor", "patch"))
        prerelease = item["prerelease"]
        if prerelease is None:
            return encoded + b"\0"
        if not isinstance(prerelease, str) or not prerelease or not prerelease.isascii():
            raise ReleaseRequestError("base_version.prerelease is invalid")
        raw_prerelease = prerelease.encode("ascii")
        return encoded + b"\1" + len(raw_prerelease).to_bytes(4, "little") + raw_prerelease

    def commit(raw: object) -> bytes:
        item = _closed(raw, {"kind", "hex"}, "base_commit")
        sizes = {"sha1": (1, 20), "sha256": (2, 32)}
        if item["kind"] not in sizes:
            raise ReleaseRequestError("base_commit kind is invalid")
        discriminator, size = sizes[item["kind"]]
        digest = bytes.fromhex(_hex(item["hex"], "base_commit.hex", (size * 2,)))
        return bytes((1, discriminator)) + size.to_bytes(4, "little") + digest

    def profile(raw: object, field: str) -> bytes:
        item = _closed(raw, {"major", "minor"}, field)
        return unsigned(item["major"], 2, f"{field}.major") + unsigned(item["minor"], 2, f"{field}.minor")

    def toolchain(raw: object) -> bytes:
        item = _closed(raw, {"kind", "hex"}, "toolchain")
        if item["kind"] != "known":
            raise ReleaseRequestError("qualification artifact toolchain must be known")
        digest = bytes.fromhex(_hex(item["hex"], "toolchain.hex"))
        return b"\1" + len(digest).to_bytes(4, "little") + digest

    values = (
        release(tuple_value["base_version"]), commit(tuple_value["base_commit"]),
        bytes.fromhex(_hex(tuple_value["canonical_schema_digest"], "canonical_schema_digest")),
        bytes.fromhex(_hex(tuple_value["domain_registry_digest"], "domain_registry_digest")),
        bytes.fromhex(_hex(tuple_value["resource_registry_digest"], "resource_registry_digest")),
        unsigned(tuple_value["storage_schema"], 4, "storage_schema"),
        profile(tuple_value["archive_profile"], "archive_profile"),
        profile(tuple_value["migration_profile"], "migration_profile"),
        profile(tuple_value["registry_profile"], "registry_profile"),
        bytes.fromhex(_hex(tuple_value["registry_profile_digest"], "registry_profile_digest")),
        profile(tuple_value["wire_session"], "wire_session"),
        profile(tuple_value["product_api"], "product_api"),
        profile(tuple_value["c_abi"], "c_abi"),
        bytes.fromhex(_hex(tuple_value["feature_set_digest"], "feature_set_digest")),
        str(tuple_value["target_triple"]).encode("ascii"), toolchain(tuple_value["toolchain"]),
    )
    count = 16 if include_artifact_fields else 14
    output = bytearray()
    for identifier, raw in enumerate(values[:count], start=1):
        output.extend(struct.pack("<HI", identifier, len(raw)))
        output.extend(raw)
    return bytes(output)


def _closed(value: object, fields: set[str], name: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != fields:
        raise ReleaseRequestError(f"{name} fields are not closed")
    return value


def _hex(value: object, field: str, lengths: tuple[int, ...] = (64,)) -> str:
    if (
        not isinstance(value, str)
        or len(value) not in lengths
        or any(c not in "0123456789abcdef" for c in value)
    ):
        raise ReleaseRequestError(f"{field} must be lowercase hexadecimal")
    return value


def _fingerprint(value: object, field: str) -> str:
    if (
        not isinstance(value, str)
        or len(value) != 40
        or any(c not in "0123456789ABCDEF" for c in value)
    ):
        raise ReleaseRequestError(f"{field} must be a full uppercase fingerprint")
    return value


def _instant(value: object, field: str) -> datetime:
    if not isinstance(value, str) or not value.endswith("Z"):
        raise ReleaseRequestError(f"{field} must be UTC")
    try:
        parsed = datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError as error:
        raise ReleaseRequestError(f"{field} is invalid") from error
    if parsed.microsecond:
        raise ReleaseRequestError(f"{field} must have whole-second precision")
    return parsed


def _policy(value: object, production: bool) -> tuple[dict[str, Any], str]:
    policy = _closed(
        value,
        {"algorithm", "allowed_usages", "format", "role", "signers",
         "valid_unlisted_signature", "verification"},
        "qualification approver policy",
    )
    if (
        policy["format"] != "onebrain/base-v1-qualification-approver-policy/1"
        or policy["algorithm"] != "OpenPGP-Ed25519"
        or policy["role"] != "qualification-approver"
        or policy["allowed_usages"] != ["base-release-request"]
        or policy["valid_unlisted_signature"] != "reject"
        or policy["verification"] != {
            "fingerprint_source": "gpg-status-fd-VALIDSIG-full-primary-fingerprint",
            "trust_model": "explicit-allowlist",
        }
    ):
        raise ReleaseRequestError("qualification approver policy contract is invalid")
    signers = policy["signers"]
    if not isinstance(signers, list) or len(signers) != 1:
        raise ReleaseRequestError("qualification approver allowlist must contain one signer")
    signer = _closed(
        signers[0],
        {"created_utc", "expires_utc", "fingerprint", "key_id", "public_key_packet_blake3"},
        "qualification approver signer",
    )
    fingerprint = _fingerprint(signer["fingerprint"], "policy fingerprint")
    if signer["key_id"] != fingerprint[-16:]:
        raise ReleaseRequestError("policy key id does not match full fingerprint")
    _hex(signer["public_key_packet_blake3"], "public key packet BLAKE3")
    if _instant(signer["created_utc"], "policy created_utc") >= _instant(signer["expires_utc"], "policy expires_utc"):
        raise ReleaseRequestError("policy validity interval is empty")
    digest = blake3.blake3(
        canonical_json(policy), derive_key_context=APPROVER_POLICY_DIGEST_CONTEXT
    ).hexdigest()
    if production and (policy != FROZEN_APPROVER_POLICY or digest != APPROVER_POLICY_DIGEST):
        raise ReleaseRequestError("production qualification approver policy is not frozen")
    return policy, digest


def _validate_request(value: object, policy_digest: str, signer: dict[str, Any], now: datetime) -> dict[str, Any]:
    request = _closed(value, REQUEST_FIELDS, "release request")
    if request["format"] != "onebrain/base-v1-release-request/1" or request["usage"] != "base-release-request":
        raise ReleaseRequestError("release request format or usage is invalid")
    _hex(request["qualification_session_id"], "qualification_session_id")
    candidate = _closed(request["candidate"], {"commit", "tree", "object_format"}, "candidate")
    object_format = candidate["object_format"]
    if object_format not in ("sha1", "sha256"):
        raise ReleaseRequestError("candidate object format is invalid")
    length = 40 if object_format == "sha1" else 64
    _hex(candidate["commit"], "candidate commit", (length,))
    _hex(candidate["tree"], "candidate tree", (length,))
    fingerprint = _fingerprint(request["qualification_approver_fingerprint"], "request approver fingerprint")
    if fingerprint != signer["fingerprint"]:
        raise ReleaseRequestError("request approver fingerprint is not allowlisted")
    if request["trust_policy_digest"] != policy_digest:
        raise ReleaseRequestError("request trust policy digest mismatch")
    targets = request["required_targets"]
    if not isinstance(targets, dict) or not targets or any(not isinstance(k, str) or not k for k in targets):
        raise ReleaseRequestError("required target map is invalid")
    for target, digest in targets.items():
        _hex(digest, f"required target {target}")
    for field in ("production_profile_blake3", "production_vector_blake3", "append_only_idl_history_root"):
        _hex(request[field], field)
    created = _instant(request["created_utc"], "request created_utc")
    expires = _instant(request["expires_utc"], "request expires_utc")
    signer_created = _instant(signer["created_utc"], "signer created_utc")
    signer_expires = _instant(signer["expires_utc"], "signer expires_utc")
    if created >= expires or created < signer_created or expires > signer_expires:
        raise ReleaseRequestError("request validity is outside signer policy")
    if now < created or now >= expires:
        raise ReleaseRequestError("release request is expired or not yet valid")
    uri = request["evidence_root_uri"]
    if not isinstance(uri, str) or not urlparse(uri).scheme:
        raise ReleaseRequestError("evidence_root_uri must be absolute")
    tooling = _closed(request["candidate_tooling_blake3"], TOOLING_FIELDS, "candidate tooling")
    for field, digest in tooling.items():
        _hex(digest, f"candidate tooling {field}")
    registry = _closed(
        request["registry_candidate"],
        {"candidate_semantic_digest", "artifact_tuple_digest", "release_aggregate_root",
         "registry_generation", "payload_artifacts_blake3", "release_stamp_blake3",
         "registry_trust_policy_digest", "registry_signer_fingerprint",
         "ccid_inputs_blake3"},
        "registry candidate",
    )
    for field in ("candidate_semantic_digest", "artifact_tuple_digest", "release_aggregate_root", "release_stamp_blake3"):
        _hex(registry[field], f"registry {field}")
    _hex(registry["registry_trust_policy_digest"], "registry trust policy digest")
    _hex(registry["registry_signer_fingerprint"], "registry signer fingerprint")
    if isinstance(registry["registry_generation"], bool) or not isinstance(registry["registry_generation"], int) or registry["registry_generation"] <= 0:
        raise ReleaseRequestError("registry generation must be positive")
    artifacts = _closed(registry["payload_artifacts_blake3"], ARTIFACT_FIELDS, "registry payload artifacts")
    for name, digest in artifacts.items():
        _hex(digest, f"registry artifact {name}")
    ccid_inputs = _closed(
        registry["ccid_inputs_blake3"],
        {"old_input", "old_obr", "old_manifest", "candidate_input", "candidate_obr", "candidate_manifest"},
        "CCID stability inputs",
    )
    for name, digest in ccid_inputs.items():
        _hex(digest, f"CCID stability input {name}")
    environment = _closed(
        request["reference_environment"],
        {"target_triple", "rust_toolchain_digest", "runner_image_digest", "probe_blake3",
         "probe_signature", "probe_signer_fingerprint", "probe_signer_public_key",
         "executable_blake3", "python_executable_blake3", "gpg_executable_blake3"},
        "reference environment",
    )
    if not isinstance(environment["target_triple"], str) or not environment["target_triple"]:
        raise ReleaseRequestError("reference target triple is invalid")
    for field in set(environment) - {"target_triple"}:
        _hex(environment[field], f"reference environment {field}")
    return request


def _verify_release_request(
    request_path: Path,
    signature_path: Path,
    policy_path: Path,
    gpg_home: Path,
    *,
    gpg_executable: Path,
    production: bool,
    now: datetime | None = None,
) -> VerifiedQualificationContextV1:
    try:
        request_bytes = request_path.read_bytes()
        policy_bytes = policy_path.read_bytes()
        request_value = json.loads(request_bytes)
        policy_value = json.loads(policy_bytes)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ReleaseRequestError("release request or policy could not be read") from error
    if request_bytes != canonical_json(request_value):
        raise ReleaseRequestError("release request bytes are not canonical")
    if policy_bytes != canonical_json(policy_value):
        raise ReleaseRequestError("qualification approver policy bytes are not canonical")
    policy, policy_digest = _policy(policy_value, production)
    signer = policy["signers"][0]
    request = _validate_request(
        request_value, policy_digest, signer, now or datetime.now(timezone.utc)
    )
    command = [
        str(gpg_executable), "--homedir", str(gpg_home), "--batch",
        "--no-tty", "--status-fd", "1", "--verify", str(signature_path),
        str(request_path),
    ]
    completed = subprocess.run(command, capture_output=True, text=True, check=False)
    status = [line.split() for line in completed.stdout.splitlines() if line.startswith("[GNUPG:] VALIDSIG ")]
    if completed.returncode != 0 or len(status) != 1:
        raise ReleaseRequestError("detached signature verification failed")
    tokens = status[0]
    signing_fingerprint = tokens[2]
    primary_fingerprint = tokens[-1] if len(tokens) >= 12 else signing_fingerprint
    allowed = signer["fingerprint"]
    if len(tokens) < 12 or tokens[8] != "22":
        raise ReleaseRequestError("VALIDSIG is not an OpenPGP Ed25519 signature")
    try:
        signature_created = datetime.fromtimestamp(int(tokens[4]), timezone.utc)
    except (ValueError, OverflowError) as error:
        raise ReleaseRequestError("VALIDSIG creation time is invalid") from error
    request_created = _instant(request["created_utc"], "request created_utc")
    request_expires = _instant(request["expires_utc"], "request expires_utc")
    if not request_created <= signature_created < request_expires:
        raise ReleaseRequestError("VALIDSIG creation time is outside request validity")
    if primary_fingerprint != allowed or request["qualification_approver_fingerprint"] != allowed:
        raise ReleaseRequestError("VALIDSIG full primary fingerprint is not in the explicit allowlist")
    exported = subprocess.run(
        [str(gpg_executable), "--homedir", str(gpg_home), "--batch", "--export", allowed],
        capture_output=True, check=False,
    )
    if exported.returncode != 0 or not exported.stdout:
        raise ReleaseRequestError("allowlisted public key packet could not be exported")
    if blake3.blake3(exported.stdout).hexdigest() != signer["public_key_packet_blake3"]:
        raise ReleaseRequestError("public key packet BLAKE3 mismatch")
    _verify_authenticated_tooling(
        request, policy_bytes, python_path=python_executable_path(),
        gpg_path=gpg_executable, verifier_path=Path(__file__).resolve(),
    )
    registry = request["registry_candidate"]
    environment = request["reference_environment"]
    request_digest = blake3.blake3(request_bytes).hexdigest()
    run_context = {
        "format": "onebrain/qualification-run-context/1",
        "variant": "Release",
        "release_request_digest": request_digest,
        "qualification_session_id": request["qualification_session_id"],
        "candidate_commit": request["candidate"]["commit"],
        "candidate_tree": request["candidate"]["tree"],
    }
    bindings = {
        "evidence_tier": "production-reference" if production else "nonproduction-test",
        "release_request_digest": request_digest,
        "qualification_session_id": request["qualification_session_id"],
        "candidate_commit": request["candidate"]["commit"],
        "candidate_tree": request["candidate"]["tree"],
        "candidate_semantic_digest": registry["candidate_semantic_digest"],
        "artifact_tuple_digest": registry["artifact_tuple_digest"],
        "release_aggregate_root": registry["release_aggregate_root"],
        "registry_generation": registry["registry_generation"],
        "production_profile_blake3": request["production_profile_blake3"],
        "production_vector_blake3": request["production_vector_blake3"],
        "append_only_idl_history_root": request["append_only_idl_history_root"],
        "required_targets": request["required_targets"],
        "candidate_payload_artifacts_blake3": registry["payload_artifacts_blake3"],
        "release_stamp_blake3": registry["release_stamp_blake3"],
        "probe_blake3": environment["probe_blake3"],
        "probe_signature": environment["probe_signature"],
        "probe_signer_fingerprint": environment["probe_signer_fingerprint"],
        "probe_signer_public_key": environment["probe_signer_public_key"],
        "executable_blake3": environment["executable_blake3"],
        "rust_toolchain_digest": environment["rust_toolchain_digest"],
        "runner_image_digest": environment["runner_image_digest"],
        "target_triple": environment["target_triple"],
        "python_executable_blake3": environment["python_executable_blake3"],
        "gpg_executable_blake3": environment["gpg_executable_blake3"],
        "trust_policy_digest": registry["registry_trust_policy_digest"],
        "signer_fingerprint": registry["registry_signer_fingerprint"],
        "ccid_inputs_blake3": registry["ccid_inputs_blake3"],
    }
    return VerifiedQualificationContextV1(
        request_digest=request_digest,
        signer_fingerprint=primary_fingerprint,
        trust_policy_digest=policy_digest,
        run_context=run_context,
        bindings=bindings,
        tooling_blake3=dict(request["candidate_tooling_blake3"]),
        production=production,
    )


def verify_release_request(
    request_path: Path,
    signature_path: Path,
    policy_path: Path,
    gpg_home: Path,
    *,
    now: datetime | None = None,
) -> VerifiedQualificationContextV1:
    """Production Linux verifier with no executable or policy-mode injection."""
    if not sys.platform.startswith("linux"):
        raise ReleaseRequestError("production release-request verification requires Linux")
    gpg = Path("/usr/bin/gpg")
    if not gpg.is_file():
        raise ReleaseRequestError("fixed production GPG executable is unavailable")
    return _verify_release_request(
        request_path, signature_path, policy_path, gpg_home,
        gpg_executable=gpg, production=True, now=now,
    )


def verify_release_request_for_test_nonproduction(
    request_path: Path,
    signature_path: Path,
    policy_path: Path,
    gpg_home: Path,
    *,
    gpg_executable: Path,
    now: datetime | None = None,
) -> VerifiedQualificationContextV1:
    """Explicit test helper; its result can never carry a production identity."""
    return _verify_release_request(
        request_path, signature_path, policy_path, gpg_home,
        gpg_executable=gpg_executable, production=False, now=now,
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--request", type=Path, required=True)
    parser.add_argument("--signature", type=Path, required=True)
    parser.add_argument("--policy", type=Path, required=True)
    parser.add_argument("--gpg-home", type=Path, required=True)
    parser.add_argument(
        "--test-nonproduction-gpg",
        type=Path,
        help=argparse.SUPPRESS,
    )
    args = parser.parse_args(argv)
    try:
        if args.test_nonproduction_gpg is None:
            verified = verify_release_request(
                args.request, args.signature, args.policy, args.gpg_home,
            )
        else:
            verified = verify_release_request_for_test_nonproduction(
                args.request,
                args.signature,
                args.policy,
                args.gpg_home,
                gpg_executable=args.test_nonproduction_gpg,
            )
    except ReleaseRequestError as error:
        print(f"Base release request verification failed: {error}", file=__import__("sys").stderr)
        return 1
    print(json.dumps(verified.as_dict(), sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
