#!/usr/bin/env python3
"""Execute and independently inspect the first-party nine-step release cycle."""

from __future__ import annotations

import json
import struct
import subprocess
from dataclasses import dataclass
from pathlib import Path

import blake3
from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey

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
from verify_base_release_request import (  # noqa: E402
    VerifiedQualificationContextV1,
    verify_release_request,
    verify_release_request_for_test_nonproduction,
    verify_registry_candidate_measurements,
    verify_registry_candidate_measurements_for_test_nonproduction,
)

REQUIRED_STEPS = (
    "package", "verify", "activate", "query",
    "build-new-signed-generation", "ccid-diff", "activate-new",
    "rollback", "reactivate-new",
)
HEADER = struct.Struct("<4sIQQ8s")
ENTRY_PREFIX = struct.Struct("<16sIBBH")
U16 = struct.Struct("<H")
STAMP_FIELDS = (
    "profile", "release_id", "builder_version", "dedup_policy_version",
    "artifacts", "artifact_root", "sources", "source_root", "distribution",
    "signer_public_key", "signature",
)
STAMP_ARTIFACT_FIELDS = ("role", "relative_path", "length", "blake3")
STAMP_SOURCE_FIELDS = (
    "name", "snapshot_id", "source_uri", "license", "snapshot_blake3",
    "download_blake3",
)
STATE_FIELDS = ("profile", "generation", "active_release", "previous_release", "state_root")
STAMP_SIGNATURE_DOMAIN = b"onebrain:concept-registry-release-stamp:1\0"
STAMP_SOURCE_ROOT_DOMAIN = b"onebrain:concept-registry-sources:1\0"
STAMP_DISTRIBUTION = "MIRROR_OR_OFFLINE_ONLY_NO_OBP_GOSSIP"
STAMP_ARTIFACTS = {
    ("OBR", "concepts.obr"),
    ("LABEL_INDEX", "concepts.obr.labels.idx"),
    ("CCID_INDEX", "concepts.obr.ccids.idx"),
    ("MANIFEST", "concepts.obr.manifest.json"),
    ("SPDX_SBOM", "sbom.spdx.json"),
}
STAMP_SOURCES = {"chebi", "geonames", "ncbi", "wikidata", "wordnet"}


class CycleError(RuntimeError):
    """The independently measured release cycle did not complete exactly."""


@dataclass(frozen=True)
class MeasuredCandidateContext:
    registry_root: Path
    release_stamp_identity: str
    state_identity: str


def _digest(path: Path) -> str:
    return blake3.blake3(path.read_bytes()).hexdigest()


def _execute_bridge(bridge: Path, operation: str, arguments: list[str]) -> list[str]:
    command = [str(bridge), operation, *arguments]
    child = subprocess.Popen(
        command, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        text=True, encoding="utf-8", errors="replace",
    )
    stdout, stderr = child.communicate(timeout=120)
    if child.returncode != 0:
        raise CycleError(f"first-party {operation} failed: {stderr.strip()[-2000:]}")
    if stdout.strip():
        raise CycleError(f"first-party {operation} emitted untrusted stdout")
    return command


def _stamp(registry_root: Path, release_id: str) -> dict[str, object]:
    path = registry_root / "releases" / release_id / "release.stamp.json"
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise CycleError(f"installed stamp is invalid: {release_id}") from error
    if not isinstance(value, dict) or value.get("release_id") != release_id:
        raise CycleError("installed stamp release identity mismatch")
    expected = {
        "OBR:concepts.obr", "LABEL_INDEX:concepts.obr.labels.idx",
        "CCID_INDEX:concepts.obr.ccids.idx", "MANIFEST:concepts.obr.manifest.json",
        "SPDX_SBOM:sbom.spdx.json",
    }
    artifacts = value.get("artifacts")
    if not isinstance(artifacts, list) or {
        f"{item.get('role')}:{item.get('relative_path')}"
        for item in artifacts if isinstance(item, dict)
    } != expected:
        raise CycleError("installed stamp artifact tuple is not exact")
    release_dir = path.parent
    measured = []
    for item in artifacts:
        artifact = release_dir / item["relative_path"]
        length = artifact.stat().st_size
        digest = _digest(artifact)
        if item.get("length") != length or item.get("blake3") != digest:
            raise CycleError("installed payload bytes differ from stamp")
        measured.append((item["role"], item["relative_path"], length, digest))
    hasher = blake3.blake3()
    hasher.update(b"onebrain:concept-registry-artifacts:1\0")
    for role, relative, length, digest in sorted(measured):
        for raw in (role.encode(), relative.encode()):
            hasher.update(len(raw).to_bytes(8, "big"))
            hasher.update(raw)
        hasher.update(length.to_bytes(8, "big"))
        raw_digest = digest.encode("ascii")
        hasher.update(len(raw_digest).to_bytes(8, "big"))
        hasher.update(raw_digest)
    if value.get("artifact_root") != hasher.hexdigest():
        raise CycleError("installed release aggregate root does not match measured payloads")
    value["measured_artifacts_blake3"] = {
        f"{role}:{relative}": digest for role, relative, _length, digest in measured
    }
    value["measured_stamp_blake3"] = _digest(path)
    return value


def _latest_state(registry_root: Path) -> dict[str, object]:
    states = sorted((registry_root / "state").glob("state-*.json"))
    if not states:
        raise CycleError("active state generation is missing")
    try:
        value = json.loads(states[-1].read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise CycleError("active state generation is invalid") from error
    generation = value.get("generation") if isinstance(value, dict) else None
    if (
        not isinstance(generation, int)
        or isinstance(generation, bool)
        or generation != len(states)
    ):
        raise CycleError("active state generation is not append-only")
    state_view = {
        "profile": value.get("profile"), "generation": value.get("generation"),
        "active_release": value.get("active_release"),
        "previous_release": value.get("previous_release"),
    }
    state_bytes = json.dumps(
        state_view, ensure_ascii=False, separators=(",", ":")
    ).encode("utf-8")
    expected_root = blake3.blake3(
        b"onebrain:concept-registry-state:1\0" + state_bytes
    ).hexdigest()
    if value.get("state_root") != expected_root:
        raise CycleError("active state root does not match authoritative state bytes")
    active_stamp = _stamp(registry_root, str(value.get("active_release")))
    value["active_release_root"] = active_stamp["artifact_root"]
    return value


def _ordered_json(value: dict[str, object], fields: tuple[str, ...]) -> bytes:
    return json.dumps(
        {field: value[field] for field in fields},
        ensure_ascii=False,
        separators=(",", ":"),
    ).encode("utf-8")


def _pretty_ordered_json(value: dict[str, object], fields: tuple[str, ...]) -> bytes:
    return (
        json.dumps(
            {field: value[field] for field in fields},
            ensure_ascii=False,
            indent=2,
        )
        + "\n"
    ).encode("utf-8")


def _is_lower_hex(value: object, length: int) -> bool:
    return (
        isinstance(value, str)
        and len(value) == length
        and all(character in "0123456789abcdef" for character in value)
    )


def _is_release_id(value: object) -> bool:
    return (
        isinstance(value, str)
        and 0 < len(value) <= 96
        and value.isascii()
        and all(character.isalnum() or character in "._-" for character in value)
    )


def _source_root(sources: list[dict[str, object]]) -> str:
    hasher = blake3.blake3()
    hasher.update(STAMP_SOURCE_ROOT_DOMAIN)
    for source in sorted(sources, key=lambda item: str(item["name"])):
        for field in STAMP_SOURCE_FIELDS:
            raw = str(source[field]).encode("utf-8")
            hasher.update(len(raw).to_bytes(8, "big"))
            hasher.update(raw)
    return hasher.hexdigest()


def _verify_candidate_stamp_and_state(
    verified: VerifiedQualificationContextV1,
    *,
    candidate_release_stamp: Path,
    candidate_state: Path,
    old_release_id: str,
    candidate_release_id: str,
    sources: Path,
    release_private_key: Path,
    release_public_key: str,
) -> MeasuredCandidateContext:
    """Authenticate the pre-existing candidate release/state without trusting paths."""
    if not _is_release_id(old_release_id) or not _is_release_id(candidate_release_id):
        raise CycleError("pre-operation release identity is invalid")
    if old_release_id == candidate_release_id:
        raise CycleError("pre-operation old and candidate release identities must be distinct")
    if (
        candidate_release_stamp.is_symlink()
        or not candidate_release_stamp.is_file()
        or candidate_state.is_symlink()
        or not candidate_state.is_file()
    ):
        raise CycleError("pre-operation candidate stamp/state must be regular files")
    try:
        stamp_bytes = candidate_release_stamp.read_bytes()
        stamp = json.loads(stamp_bytes)
        state_bytes = candidate_state.read_bytes()
        state = json.loads(state_bytes)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise CycleError("pre-operation candidate stamp/state is invalid") from error
    stamp_blake3 = blake3.blake3(stamp_bytes).hexdigest()
    state_blake3 = blake3.blake3(state_bytes).hexdigest()
    if not isinstance(stamp, dict) or set(stamp) != set(STAMP_FIELDS):
        raise CycleError("pre-operation candidate stamp fields are not closed")
    artifacts = stamp.get("artifacts")
    stamp_sources = stamp.get("sources")
    if (
        not isinstance(artifacts, list)
        or len(artifacts) != 5
        or any(not isinstance(item, dict) or set(item) != set(STAMP_ARTIFACT_FIELDS) for item in artifacts)
        or not isinstance(stamp_sources, list)
        or any(not isinstance(item, dict) or set(item) != set(STAMP_SOURCE_FIELDS) for item in stamp_sources)
    ):
        raise CycleError("pre-operation candidate stamp nested fields are not closed")
    artifact_tuples = {
        (item.get("role"), item.get("relative_path")) for item in artifacts
    }
    if artifact_tuples != STAMP_ARTIFACTS or any(
        not isinstance(item.get("role"), str)
        or not isinstance(item.get("relative_path"), str)
        or not isinstance(item.get("length"), int)
        or isinstance(item.get("length"), bool)
        or not 0 < item["length"] <= (1 << 64) - 1
        or not _is_lower_hex(item.get("blake3"), 64)
        for item in artifacts
    ):
        raise CycleError("pre-operation candidate stamp artifact schema is invalid")
    source_names = {item.get("name") for item in stamp_sources}
    if len(stamp_sources) != 5 or source_names != STAMP_SOURCES or any(
        not all(isinstance(item.get(field), str) for field in STAMP_SOURCE_FIELDS)
        or not str(item.get("snapshot_id")).strip()
        or not str(item.get("source_uri")).strip()
        or not str(item.get("license")).strip()
        or not _is_lower_hex(item.get("snapshot_blake3"), 64)
        or not _is_lower_hex(item.get("download_blake3"), 64)
        for item in stamp_sources
    ):
        raise CycleError("pre-operation candidate stamp source schema is invalid")
    canonical_stamp = {
        field: (
            [{nested: item[nested] for nested in STAMP_ARTIFACT_FIELDS} for item in artifacts]
            if field == "artifacts"
            else [{nested: item[nested] for nested in STAMP_SOURCE_FIELDS} for item in stamp_sources]
            if field == "sources"
            else stamp[field]
        )
        for field in STAMP_FIELDS
    }
    if stamp_bytes != _pretty_ordered_json(canonical_stamp, STAMP_FIELDS):
        raise CycleError("pre-operation candidate stamp is not canonical JSON")
    if (
        stamp.get("profile") != "onebrain/concept-registry-release/1"
        or stamp.get("release_id") != candidate_release_id
        or not isinstance(stamp.get("builder_version"), str)
        or not stamp["builder_version"].strip()
        or not isinstance(stamp.get("dedup_policy_version"), str)
        or not stamp["dedup_policy_version"].strip()
        or stamp.get("distribution") != STAMP_DISTRIBUTION
        or not _is_lower_hex(stamp.get("artifact_root"), 64)
        or stamp.get("artifact_root") != verified.bindings["release_aggregate_root"]
        or not _is_lower_hex(stamp.get("source_root"), 64)
        or stamp.get("source_root") != _source_root(stamp_sources)
        or not _is_lower_hex(stamp.get("signer_public_key"), 64)
        or not _is_lower_hex(stamp.get("signature"), 128)
        or stamp_blake3 != verified.bindings["release_stamp_blake3"]
        or stamp.get("signer_public_key") != release_public_key
    ):
        if stamp.get("source_root") != _source_root(stamp_sources):
            raise CycleError("pre-operation candidate source root is invalid")
        raise CycleError("pre-operation candidate stamp differs from signed request or Rust schema")
    try:
        source_value = json.loads(sources.read_bytes())
    except (OSError, json.JSONDecodeError) as error:
        raise CycleError("candidate release sources are invalid") from error
    if source_value != stamp_sources:
        raise CycleError("candidate release sources differ from signed stamp")
    release_dir = candidate_release_stamp.resolve(strict=True).parent
    if release_dir.name != candidate_release_id or release_dir.parent.name != "releases":
        raise CycleError("pre-operation candidate stamp path is not the staged release")
    try:
        manifest = json.loads((release_dir / "concepts.obr.manifest.json").read_bytes())
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise CycleError("pre-operation candidate manifest is invalid") from error
    manifest_sources = manifest.get("sources") if isinstance(manifest, dict) else None
    if (
        not isinstance(manifest_sources, dict)
        or stamp["builder_version"] != manifest.get("builder_version")
        or stamp["dedup_policy_version"] != manifest.get("dedup_policy_version")
        or len(manifest_sources) != len(stamp_sources)
        or any(
            not isinstance(manifest_sources.get(source["name"]), dict)
            or any(
                source[field] != manifest_sources[source["name"]].get(field)
                for field in ("snapshot_id", "source_uri", "license")
            )
            for source in stamp_sources
        )
    ):
        raise CycleError("pre-operation candidate stamp differs from packaged manifest")
    try:
        private_hex = release_private_key.read_text(encoding="ascii").strip()
        if not _is_lower_hex(private_hex, 64) or not _is_lower_hex(release_public_key, 64):
            raise ValueError("release key encoding is invalid")
        public_bytes = bytes.fromhex(release_public_key)
        private_key = Ed25519PrivateKey.from_private_bytes(
            bytes.fromhex(private_hex)
        )
        if private_key.public_key().public_bytes_raw() != public_bytes:
            raise ValueError("release key pair mismatch")
        unsigned = dict(canonical_stamp)
        unsigned["signature"] = ""
        message = STAMP_SIGNATURE_DOMAIN + blake3.blake3(
            _ordered_json(unsigned, STAMP_FIELDS)
        ).digest()
        Ed25519PublicKey.from_public_bytes(public_bytes).verify(
            bytes.fromhex(str(stamp["signature"])), message
        )
    except (OSError, UnicodeError, ValueError, InvalidSignature) as error:
        raise CycleError("pre-operation candidate stamp signature is invalid") from error
    if signer_fingerprint(public_bytes) != verified.bindings["signer_fingerprint"]:
        raise CycleError("pre-operation release signer differs from signed request")

    if not isinstance(state, dict) or set(state) != set(STATE_FIELDS):
        raise CycleError("pre-operation candidate state fields are not closed")
    if state_bytes != _pretty_ordered_json(state, STATE_FIELDS):
        raise CycleError("pre-operation candidate state is not canonical JSON")
    if (
        not isinstance(state.get("generation"), int)
        or isinstance(state.get("generation"), bool)
        or not 0 < state["generation"] <= (1 << 64) - 1
        or not _is_release_id(state.get("active_release"))
        or not _is_release_id(state.get("previous_release"))
        or not _is_lower_hex(state.get("state_root"), 64)
    ):
        raise CycleError("pre-operation candidate state schema is invalid")
    state_view = {field: state[field] for field in STATE_FIELDS[:-1]}
    state_root = blake3.blake3(
        b"onebrain:concept-registry-state:1\0" + _ordered_json(state_view, STATE_FIELDS[:-1])
    ).hexdigest()
    if (
        state.get("profile") != "onebrain/concept-registry-release-state/1"
        or state.get("active_release") != candidate_release_id
        or state.get("previous_release") != old_release_id
        or state.get("generation") != verified.bindings["registry_generation"]
        or state.get("state_root") != state_root
    ):
        raise CycleError("pre-operation candidate state differs from signed request")

    candidate_registry_root = release_dir.parent.parent
    expected_state = candidate_registry_root / "state" / f"state-{int(state['generation']):020}.json"
    if candidate_state.resolve(strict=True) != expected_state.resolve(strict=True):
        raise CycleError("pre-operation candidate state is not the active staged generation")
    measured_stamp = _stamp(candidate_registry_root, candidate_release_id)
    measured_state = _latest_state(candidate_registry_root)
    if (
        measured_stamp["artifact_root"] != verified.bindings["release_aggregate_root"]
        or measured_state["active_release_root"] != verified.bindings["release_aggregate_root"]
        or measured_state["generation"] != verified.bindings["registry_generation"]
    ):
        raise CycleError("pre-operation candidate Registry context differs from signed request")
    try:
        if (
            candidate_release_stamp.read_bytes() != stamp_bytes
            or candidate_state.read_bytes() != state_bytes
        ):
            raise CycleError("pre-operation candidate stamp/state changed while measured")
    except OSError as error:
        raise CycleError("pre-operation candidate stamp/state changed while measured") from error
    return MeasuredCandidateContext(
        registry_root=candidate_registry_root,
        release_stamp_identity=(
            f"{candidate_release_stamp.name}@blake3:{stamp_blake3}"
        ),
        state_identity=f"{candidate_state.name}@blake3:{state_blake3}",
    )


def _verify_cycle_candidate(
    verified: VerifiedQualificationContextV1,
    candidate_root: Path,
    bridge: Path,
    git_executable: Path,
    production: bool,
) -> None:
    root = candidate_root.resolve(strict=True)
    executable_name = "concept_registry_release_ops.exe" if __import__("sys").platform == "win32" else "concept_registry_release_ops"
    allowed_profiles = ("release",) if production else ("debug", "release")
    expected_bridges = {
        (root / "src/target" / profile / "examples" / executable_name).resolve(strict=True)
        for profile in allowed_profiles
        if (root / "src/target" / profile / "examples" / executable_name).is_file()
    }
    if bridge.resolve(strict=True) not in expected_bridges:
        raise CycleError("release bridge is not the fixed candidate-owned operation")
    for revision, expected in (
        ("HEAD", verified.run_context["candidate_commit"]),
        ("HEAD^{tree}", verified.run_context["candidate_tree"]),
    ):
        result = subprocess.run(
            [str(git_executable), "-C", str(root), "rev-parse", revision],
            capture_output=True, text=True, check=False,
        )
        if result.returncode != 0 or result.stdout.strip() != expected:
            raise CycleError("release-cycle candidate Git identity differs from signed request")
    if _digest(bridge) != verified.tooling_blake3["release_wrapper"]:
        raise CycleError("release-cycle bridge tooling differs from signed request")


def _query_obr(path: Path, query: str) -> dict[str, object]:
    with path.open("rb") as handle:
        _magic, _version, entries, _labels, _reserved = HEADER.unpack(handle.read(HEADER.size))
        for _ in range(entries):
            ccid, _stored, _source, _category, name_length = ENTRY_PREFIX.unpack(handle.read(ENTRY_PREFIX.size))
            name = handle.read(name_length).decode("utf-8")
            labels = []
            for _ in range(U16.unpack(handle.read(U16.size))[0]):
                labels.append(handle.read(U16.unpack(handle.read(U16.size))[0]).decode("utf-8"))
            if any(value.casefold() == query.casefold() for value in (name, *labels)):
                return {"found": True, "ccid": ccid.hex(), "query_blake3": blake3.blake3(query.encode()).hexdigest()}
    return {"found": False, "ccid": None, "query_blake3": blake3.blake3(query.encode()).hexdigest()}


def _sanitized_step(name: str, command: list[str], private_key: Path) -> tuple[list[str], str]:
    sanitized = [name]
    for value in command:
        if value == str(private_key):
            sanitized.append("<external-private-key-redacted>")
        else:
            candidate = Path(value)
            sanitized.append(
                f"{candidate.name}@blake3:{_digest(candidate)}"
                if candidate.is_file() else value
            )
    return sanitized, blake3.blake3(canonical_json(sanitized)).hexdigest()


def _run_release_cycle(
    verified: VerifiedQualificationContextV1,
    *,
    bridge: Path,
    candidate_root: Path,
    test_git_executable: Path | None,
    candidate_semantic_evidence: Path,
    production_profile: Path,
    production_vector: Path,
    append_only_idl_history: Path,
    candidate_tooling: dict[str, Path],
    probe: Path,
    probe_signature: Path,
    executable: Path,
    rust_toolchain_evidence: Path,
    runner_image_evidence: Path,
    target_triple: str,
    candidate_release_stamp: Path,
    candidate_state: Path,
    registry_root: Path,
    old_input: Path,
    old_obr: Path,
    old_manifest: Path,
    old_sbom: Path,
    candidate_input: Path,
    candidate_obr: Path,
    candidate_manifest: Path,
    candidate_sbom: Path,
    sources: Path,
    old_release_id: str,
    candidate_release_id: str,
    query_label: str,
    release_private_key: Path,
    release_public_key: str,
    signing_key: Ed25519PrivateKey,
    receipt_policy: dict[str, object],
) -> dict[str, object]:
    if not isinstance(verified, VerifiedQualificationContextV1):
        raise CycleError("closed verified release context is required")
    if trust_policy_digest(receipt_policy) != verified.bindings["trust_policy_digest"]:
        raise CycleError("Registry receipt policy differs from signed request")
    if signer_fingerprint(signing_key.public_key().public_bytes_raw()) != verified.bindings["signer_fingerprint"]:
        raise CycleError("Registry receipt signer differs from signed request")
    if registry_root.exists() and any(registry_root.iterdir()):
        raise CycleError("release-cycle Registry root must start empty")
    ccid_inputs = verified.bindings["ccid_inputs_blake3"]
    exact_ccid_paths = {
        "old_input": old_input,
        "old_obr": old_obr,
        "old_manifest": old_manifest,
        "candidate_input": candidate_input,
        "candidate_obr": candidate_obr,
        "candidate_manifest": candidate_manifest,
    }
    for name, path in exact_ccid_paths.items():
        if ccid_inputs.get(name) != _digest(path):
            raise CycleError(f"{name} differs from signed request")
    measured_candidate = _verify_candidate_stamp_and_state(
        verified,
        candidate_release_stamp=candidate_release_stamp,
        candidate_state=candidate_state,
        old_release_id=old_release_id,
        candidate_release_id=candidate_release_id,
        sources=sources,
        release_private_key=release_private_key,
        release_public_key=release_public_key,
    )
    candidate_registry_root = measured_candidate.registry_root
    source_payloads = {
        "OBR:concepts.obr": candidate_obr,
        "LABEL_INDEX:concepts.obr.labels.idx": Path(f"{candidate_obr}.labels.idx"),
        "CCID_INDEX:concepts.obr.ccids.idx": Path(f"{candidate_obr}.ccids.idx"),
        "MANIFEST:concepts.obr.manifest.json": Path(f"{candidate_obr}.manifest.json"),
        "SPDX_SBOM:sbom.spdx.json": candidate_sbom,
    }
    expected_payloads = verified.bindings["candidate_payload_artifacts_blake3"]
    if (
        {name: _digest(path) for name, path in source_payloads.items()} != expected_payloads
        or _digest(candidate_manifest) != expected_payloads["MANIFEST:concepts.obr.manifest.json"]
    ):
        raise CycleError("pre-operation candidate payload bytes differ from signed request")
    candidate_release_dir = candidate_release_stamp.parent
    measurement_inputs = dict(
        candidate_root=candidate_root,
        registry_root=candidate_registry_root,
        release_id=candidate_release_id,
        candidate_semantic_evidence=candidate_semantic_evidence,
        production_profile=production_profile,
        production_vector=production_vector,
        append_only_idl_history=append_only_idl_history,
        candidate_tooling=candidate_tooling,
        payload_artifacts={
            name: candidate_release_dir / name.split(":", 1)[1]
            for name in expected_payloads
        },
        release_stamp=candidate_release_stamp,
        probe=probe,
        probe_signature=probe_signature,
        executable=executable,
        rust_toolchain_evidence=rust_toolchain_evidence,
        runner_image_evidence=runner_image_evidence,
        target_triple=target_triple,
    )
    try:
        if test_git_executable is None:
            verify_registry_candidate_measurements(verified, **measurement_inputs)
        else:
            verify_registry_candidate_measurements_for_test_nonproduction(
                verified, git_executable=test_git_executable, **measurement_inputs
            )
    except Exception as error:
        raise CycleError(f"release-cycle pre-operation candidate binding failed: {error}") from error
    registry_root.mkdir(parents=True, exist_ok=True)
    observed: list[dict[str, object]] = []
    step_digests: dict[str, str] = {}
    step_commands: dict[str, list[str]] = {}

    command = _execute_bridge(bridge, "package", [str(registry_root), str(old_obr), str(old_sbom), str(sources), old_release_id, str(release_private_key)])
    old_stamp = _stamp(registry_root, old_release_id)
    observed.append({"step": "package", "result": True, "observed_release_root": old_stamp["artifact_root"], "stamp_blake3": old_stamp["measured_stamp_blake3"]})
    step_commands["package"], step_digests["package"] = _sanitized_step("package", command, release_private_key)

    command = _execute_bridge(bridge, "verify", [str(registry_root), old_release_id, release_public_key])
    verified_old = _stamp(registry_root, old_release_id)
    if verified_old["artifact_root"] != old_stamp["artifact_root"]:
        raise CycleError("old root changed after verification")
    observed.append({"step": "verify", "result": True, "observed_release_root": verified_old["artifact_root"]})
    step_commands["verify"], step_digests["verify"] = _sanitized_step("verify", command, release_private_key)

    command = _execute_bridge(bridge, "activate", [str(registry_root), old_release_id, release_public_key])
    state = _latest_state(registry_root)
    observed.append({"step": "activate", "result": state.get("active_release") == old_release_id and state.get("active_release_root") == old_stamp["artifact_root"], "observed_release_root": old_stamp["artifact_root"], "registry_generation": state["generation"], "state_root": state["state_root"]})
    step_commands["activate"], step_digests["activate"] = _sanitized_step("activate", command, release_private_key)

    query = _query_obr(registry_root / "releases" / old_release_id / "concepts.obr", query_label)
    state = _latest_state(registry_root)
    _stamp(registry_root, old_release_id)
    if not query["found"]:
        raise CycleError("old release query did not resolve")
    observed.append({"step": "query", "result": True, "observed_release_root": old_stamp["artifact_root"], "registry_generation": state["generation"], "query": query})
    step_commands["query"] = [
        "query", "internal-obr-query",
        f"--obr={old_obr.name}@blake3:{_digest(old_obr)}",
        f"--label={query_label}",
    ]
    step_digests["query"] = blake3.blake3(canonical_json(step_commands["query"])).hexdigest()

    command = _execute_bridge(bridge, "package", [str(registry_root), str(candidate_obr), str(candidate_sbom), str(sources), candidate_release_id, str(release_private_key)])
    candidate_stamp = _stamp(registry_root, candidate_release_id)
    state = _latest_state(registry_root)
    if candidate_stamp["artifact_root"] != verified.bindings["release_aggregate_root"]:
        raise CycleError("built candidate release root differs from signed request")
    expected_artifacts = verified.bindings["candidate_payload_artifacts_blake3"]
    actual_artifacts = {
        f"{item['role']}:{item['relative_path']}": item["blake3"]
        for item in candidate_stamp.get("artifacts", [])
    }
    if actual_artifacts != expected_artifacts:
        raise CycleError("built candidate payloads differ from signed request")
    if candidate_stamp["measured_stamp_blake3"] != verified.bindings["release_stamp_blake3"]:
        raise CycleError("built candidate stamp differs from signed request")
    observed.append({"step": "build-new-signed-generation", "result": True, "observed_release_root": candidate_stamp["artifact_root"], "registry_generation": state["generation"], "stamp_blake3": candidate_stamp["measured_stamp_blake3"]})
    step_commands["build-new-signed-generation"], step_digests["build-new-signed-generation"] = _sanitized_step("build-new-signed-generation", command, release_private_key)

    ccid = generate_report(old_input, old_obr, old_manifest, candidate_input, candidate_obr, candidate_manifest)
    _stamp(registry_root, old_release_id)
    _stamp(registry_root, candidate_release_id)
    state = _latest_state(registry_root)
    if ccid.get("qualified") is not True:
        raise CycleError("real CCID stability diff failed")
    observed.append({"step": "ccid-diff", "result": True, "observed_release_root": candidate_stamp["artifact_root"], "registry_generation": state["generation"], "ccid_report_blake3": blake3.blake3(canonical_json(ccid)).hexdigest()})
    ccid_command = [
        "ccid_stability_diff.py",
        *[f"--{name.replace('_', '-')}={path.name}@blake3:{_digest(path)}" for name, path in exact_ccid_paths.items()],
    ]
    step_commands["ccid-diff"] = ccid_command
    step_digests["ccid-diff"] = blake3.blake3(canonical_json(ccid_command)).hexdigest()

    command = _execute_bridge(bridge, "activate", [str(registry_root), candidate_release_id, release_public_key])
    state = _latest_state(registry_root)
    observed.append({"step": "activate-new", "result": state.get("active_release") == candidate_release_id and state.get("active_release_root") == candidate_stamp["artifact_root"], "observed_release_root": candidate_stamp["artifact_root"], "registry_generation": state["generation"], "state_root": state["state_root"]})
    step_commands["activate-new"], step_digests["activate-new"] = _sanitized_step("activate-new", command, release_private_key)

    command = _execute_bridge(bridge, "rollback", [str(registry_root), release_public_key])
    state = _latest_state(registry_root)
    observed.append({"step": "rollback", "result": state.get("active_release") == old_release_id and state.get("active_release_root") == old_stamp["artifact_root"], "observed_release_root": old_stamp["artifact_root"], "registry_generation": state["generation"], "state_root": state["state_root"]})
    step_commands["rollback"], step_digests["rollback"] = _sanitized_step("rollback", command, release_private_key)

    command = _execute_bridge(bridge, "activate", [str(registry_root), candidate_release_id, release_public_key])
    state = _latest_state(registry_root)
    final_stamp = _stamp(registry_root, candidate_release_id)
    final_query = _query_obr(registry_root / "releases" / candidate_release_id / "concepts.obr", query_label)
    final_ok = (
        state.get("active_release") == candidate_release_id
        and state.get("active_release_root") == final_stamp["artifact_root"]
        and state.get("generation") == verified.bindings["registry_generation"]
        and final_stamp["artifact_root"] == verified.bindings["release_aggregate_root"]
        and final_query["found"] is True
    )
    observed.append({"step": "reactivate-new", "result": final_ok, "observed_release_root": final_stamp["artifact_root"], "registry_generation": state["generation"], "state_root": state["state_root"], "query": final_query})
    step_commands["reactivate-new"], step_digests["reactivate-new"] = _sanitized_step("reactivate-new", command, release_private_key)
    if [step["step"] for step in observed] != list(REQUIRED_STEPS) or not all(step["result"] for step in observed):
        raise CycleError("one or more independently inspected cycle steps failed")
    installed_release = registry_root / "releases" / candidate_release_id
    measurement_inputs = dict(
        candidate_root=candidate_root, registry_root=registry_root,
        release_id=candidate_release_id,
        candidate_semantic_evidence=candidate_semantic_evidence,
        production_profile=production_profile, production_vector=production_vector,
        append_only_idl_history=append_only_idl_history,
        candidate_tooling=candidate_tooling,
        payload_artifacts={
            name: installed_release / name.split(":", 1)[1]
            for name in verified.bindings["candidate_payload_artifacts_blake3"]
        },
        release_stamp=installed_release / "release.stamp.json",
        probe=probe, probe_signature=probe_signature, executable=executable,
        rust_toolchain_evidence=rust_toolchain_evidence,
        runner_image_evidence=runner_image_evidence, target_triple=target_triple,
    )
    try:
        if test_git_executable is None:
            verify_registry_candidate_measurements(verified, **measurement_inputs)
        else:
            verify_registry_candidate_measurements_for_test_nonproduction(
                verified, git_executable=test_git_executable, **measurement_inputs
            )
    except Exception as error:
        raise CycleError(f"release-cycle measured candidate binding failed: {error}") from error

    command = [
        "release_cycle_qualification.py",
        f"--release-request-digest={verified.request_digest}",
        f"--bridge-blake3={_digest(bridge)}",
        f"--old-input-blake3={_digest(old_input)}",
        f"--candidate-input-blake3={_digest(candidate_input)}",
        f"--old-obr={old_obr.name}@blake3:{_digest(old_obr)}",
        f"--old-manifest={old_manifest.name}@blake3:{_digest(old_manifest)}",
        f"--old-sbom={old_sbom.name}@blake3:{_digest(old_sbom)}",
        f"--candidate-obr={candidate_obr.name}@blake3:{_digest(candidate_obr)}",
        f"--candidate-manifest={candidate_manifest.name}@blake3:{_digest(candidate_manifest)}",
        f"--candidate-sbom={candidate_sbom.name}@blake3:{_digest(candidate_sbom)}",
        f"--sources={sources.name}@blake3:{_digest(sources)}",
        f"--old-release-id={old_release_id}",
        f"--candidate-release-id={candidate_release_id}",
        f"--query-label={query_label}",
        f"--target-triple={verified.bindings['target_triple']}",
        f"--semantic-tuple={candidate_semantic_evidence.name}@blake3:{_digest(candidate_semantic_evidence)}",
        f"--production-profile={production_profile.name}@blake3:{_digest(production_profile)}",
        f"--production-vector={production_vector.name}@blake3:{_digest(production_vector)}",
        f"--idl-history={append_only_idl_history.name}@blake3:{_digest(append_only_idl_history)}",
        f"--probe={probe.name}@blake3:{_digest(probe)}",
        f"--probe-signature={probe_signature.name}@blake3:{_digest(probe_signature)}",
        f"--executable={executable.name}@blake3:{_digest(executable)}",
        f"--rust-toolchain={rust_toolchain_evidence.name}@blake3:{_digest(rust_toolchain_evidence)}",
        f"--runner-image={runner_image_evidence.name}@blake3:{_digest(runner_image_evidence)}",
        f"--candidate-release-stamp={measured_candidate.release_stamp_identity}",
        f"--candidate-state={measured_candidate.state_identity}",
        *[
            f"--candidate-tool-{name}={path.name}@blake3:{_digest(path)}"
            for name, path in sorted(candidate_tooling.items())
        ],
        *[f"--step-{name}-blake3={step_digests[name]}" for name in REQUIRED_STEPS],
        "--release-private-key=<external-redacted>",
        "--gpg-home=<redacted>",
        "--receipt-signer=<external-redacted>",
    ]
    payload: dict[str, object] = {
        **verified.bindings,
        "qualification_context_variant": "Release",
        **{field: verified.run_context[field] for field in ("release_request_digest", "qualification_session_id", "candidate_commit", "candidate_tree")},
        "base_candidate_bound": True,
        "evidence_tier": (
            "production-reference" if verified.production else "nonproduction-test"
        ),
        "command": command,
        "command_blake3": blake3.blake3(canonical_json(command)).hexdigest(),
        "step_command_blake3": step_digests,
        "step_commands": step_commands,
        "result": True,
        "exit_oracles": {
            "all_required_steps_executed_once_in_order": True,
            "every_step_independently_inspected": True,
            "old_and_candidate_stamps_exact": True,
            "activation_generations_are_monotonic": True,
            "queries_resolved_from_actual_obr": True,
            "real_ccid_diff_qualified": True,
            "rollback_restored_exact_old_root": True,
            "reactivation_restored_exact_candidate_root": True,
        },
        "limitations": ["Registry-only release-cycle evidence; never BASE-GATE-V1"],
        "previous_release_aggregate_root": old_stamp["artifact_root"],
        "steps": observed,
    }
    try:
        return create_signed_receipt("signed-release-cycle", payload, signing_key, receipt_policy)
    except AggregationError as error:
        raise CycleError(str(error)) from error


def run_release_cycle(
    request_path: Path,
    signature_path: Path,
    approver_policy_path: Path,
    gpg_home: Path,
    *,
    candidate_release_stamp: Path,
    candidate_state: Path,
    **inputs: object,
) -> dict[str, object]:
    """Production entry with fixed candidate bridge and fixed request verifier."""
    verified = verify_release_request(request_path, signature_path, approver_policy_path, gpg_home)
    candidate_root = Path(__file__).resolve().parents[2]
    bridge = candidate_root / "src/target/release/examples/concept_registry_release_ops"
    if not bridge.is_file():
        raise CycleError("fixed first-party release operation bridge is unavailable")
    _verify_cycle_candidate(verified, candidate_root, bridge, Path("/usr/bin/git"), True)
    return _run_release_cycle(
        verified, bridge=bridge, candidate_root=candidate_root,
        test_git_executable=None, candidate_release_stamp=candidate_release_stamp,
        candidate_state=candidate_state, **inputs,
    )


def run_release_cycle_for_test_nonproduction(
    request_path: Path,
    signature_path: Path,
    approver_policy_path: Path,
    gpg_home: Path,
    *,
    gpg_executable: Path,
    bridge: Path,
    candidate_root: Path,
    git_executable: Path,
    candidate_release_stamp: Path,
    candidate_state: Path,
    **inputs: object,
) -> dict[str, object]:
    """Explicit test path with real signature verification and nonproduction identity."""
    verified = verify_release_request_for_test_nonproduction(
        request_path, signature_path, approver_policy_path, gpg_home,
        gpg_executable=gpg_executable,
    )
    _verify_cycle_candidate(verified, candidate_root, bridge, git_executable, False)
    return _run_release_cycle(
        verified, bridge=bridge, candidate_root=candidate_root,
        test_git_executable=git_executable,
        candidate_release_stamp=candidate_release_stamp,
        candidate_state=candidate_state, **inputs,
    )
