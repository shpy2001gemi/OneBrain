#!/usr/bin/env python3
"""Execute and independently inspect the first-party nine-step release cycle."""

from __future__ import annotations

import json
import struct
import subprocess
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


class CycleError(RuntimeError):
    """The independently measured release cycle did not complete exactly."""


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
    if not isinstance(value, dict) or value.get("generation") != len(states):
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
        test_git_executable=None, **inputs,
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
        test_git_executable=git_executable, **inputs,
    )
