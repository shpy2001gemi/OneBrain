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
    return value


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

    command = _execute_bridge(bridge, "package", [str(registry_root), str(old_obr), str(old_sbom), str(sources), old_release_id, str(release_private_key)])
    old_stamp = _stamp(registry_root, old_release_id)
    observed.append({"step": "package", "result": True, "observed_release_root": old_stamp["artifact_root"], "stamp_blake3": old_stamp["measured_stamp_blake3"]})
    _, step_digests["package"] = _sanitized_step("package", command, release_private_key)

    command = _execute_bridge(bridge, "verify", [str(registry_root), old_release_id, release_public_key])
    verified_old = _stamp(registry_root, old_release_id)
    if verified_old["artifact_root"] != old_stamp["artifact_root"]:
        raise CycleError("old root changed after verification")
    observed.append({"step": "verify", "result": True, "observed_release_root": verified_old["artifact_root"]})
    _, step_digests["verify"] = _sanitized_step("verify", command, release_private_key)

    command = _execute_bridge(bridge, "activate", [str(registry_root), old_release_id, release_public_key])
    state = _latest_state(registry_root)
    observed.append({"step": "activate", "result": state.get("active_release") == old_release_id, "observed_release_root": old_stamp["artifact_root"], "registry_generation": state["generation"], "state_root": state["state_root"]})
    _, step_digests["activate"] = _sanitized_step("activate", command, release_private_key)

    query = _query_obr(registry_root / "releases" / old_release_id / "concepts.obr", query_label)
    if not query["found"]:
        raise CycleError("old release query did not resolve")
    observed.append({"step": "query", "result": True, "observed_release_root": old_stamp["artifact_root"], "registry_generation": state["generation"], "query": query})
    step_digests["query"] = blake3.blake3(canonical_json(query)).hexdigest()

    command = _execute_bridge(bridge, "package", [str(registry_root), str(candidate_obr), str(candidate_sbom), str(sources), candidate_release_id, str(release_private_key)])
    candidate_stamp = _stamp(registry_root, candidate_release_id)
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
    _, step_digests["build-new-signed-generation"] = _sanitized_step("build-new-signed-generation", command, release_private_key)

    ccid = generate_report(old_input, old_obr, old_manifest, candidate_input, candidate_obr, candidate_manifest)
    if ccid.get("qualified") is not True:
        raise CycleError("real CCID stability diff failed")
    observed.append({"step": "ccid-diff", "result": True, "observed_release_root": candidate_stamp["artifact_root"], "registry_generation": state["generation"], "ccid_report_blake3": blake3.blake3(canonical_json(ccid)).hexdigest()})
    ccid_command = [
        "ccid_stability_diff.py",
        *[f"--{name.replace('_', '-')}={path.name}@blake3:{_digest(path)}" for name, path in exact_ccid_paths.items()],
    ]
    step_digests["ccid-diff"] = blake3.blake3(canonical_json(ccid_command)).hexdigest()

    command = _execute_bridge(bridge, "activate", [str(registry_root), candidate_release_id, release_public_key])
    state = _latest_state(registry_root)
    observed.append({"step": "activate-new", "result": state.get("active_release") == candidate_release_id, "observed_release_root": candidate_stamp["artifact_root"], "registry_generation": state["generation"], "state_root": state["state_root"]})
    _, step_digests["activate-new"] = _sanitized_step("activate-new", command, release_private_key)

    command = _execute_bridge(bridge, "rollback", [str(registry_root), release_public_key])
    state = _latest_state(registry_root)
    observed.append({"step": "rollback", "result": state.get("active_release") == old_release_id, "observed_release_root": old_stamp["artifact_root"], "registry_generation": state["generation"], "state_root": state["state_root"]})
    _, step_digests["rollback"] = _sanitized_step("rollback", command, release_private_key)

    command = _execute_bridge(bridge, "activate", [str(registry_root), candidate_release_id, release_public_key])
    state = _latest_state(registry_root)
    final_stamp = _stamp(registry_root, candidate_release_id)
    final_query = _query_obr(registry_root / "releases" / candidate_release_id / "concepts.obr", query_label)
    final_ok = (
        state.get("active_release") == candidate_release_id
        and state.get("generation") == verified.bindings["registry_generation"]
        and final_stamp["artifact_root"] == verified.bindings["release_aggregate_root"]
        and final_query["found"] is True
    )
    observed.append({"step": "reactivate-new", "result": final_ok, "observed_release_root": final_stamp["artifact_root"], "registry_generation": state["generation"], "state_root": state["state_root"], "query": final_query})
    _, step_digests["reactivate-new"] = _sanitized_step("reactivate-new", command, release_private_key)
    if [step["step"] for step in observed] != list(REQUIRED_STEPS) or not all(step["result"] for step in observed):
        raise CycleError("one or more independently inspected cycle steps failed")

    command = [
        "release_cycle_qualification.py",
        f"--release-request-digest={verified.request_digest}",
        f"--bridge-blake3={_digest(bridge)}",
        f"--old-input-blake3={_digest(old_input)}",
        f"--candidate-input-blake3={_digest(candidate_input)}",
        f"--target-triple={verified.bindings['target_triple']}",
        "--release-private-key=<external-redacted>",
    ]
    payload: dict[str, object] = {
        **verified.bindings,
        "qualification_context_variant": "Release",
        **{field: verified.run_context[field] for field in ("release_request_digest", "qualification_session_id", "candidate_commit", "candidate_tree")},
        "base_candidate_bound": True,
        "command": command,
        "command_blake3": blake3.blake3(canonical_json(command)).hexdigest(),
        "step_command_blake3": step_digests,
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
    candidate_root: Path,
    **inputs: object,
) -> dict[str, object]:
    """Production entry with fixed candidate bridge and fixed request verifier."""
    verified = verify_release_request(request_path, signature_path, approver_policy_path, gpg_home)
    bridge = candidate_root / "src/target/release/examples/concept_registry_release_ops"
    if not bridge.is_file():
        raise CycleError("fixed first-party release operation bridge is unavailable")
    return _run_release_cycle(verified, bridge=bridge, **inputs)


def run_release_cycle_for_test_nonproduction(
    request_path: Path,
    signature_path: Path,
    approver_policy_path: Path,
    gpg_home: Path,
    *,
    gpg_executable: Path,
    bridge: Path,
    **inputs: object,
) -> dict[str, object]:
    """Explicit test path with real signature verification and nonproduction identity."""
    verified = verify_release_request_for_test_nonproduction(
        request_path, signature_path, approver_policy_path, gpg_home,
        gpg_executable=gpg_executable,
    )
    return _run_release_cycle(verified, bridge=bridge, **inputs)
