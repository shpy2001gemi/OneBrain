#!/usr/bin/env python3
"""Fail-closed verifier for Base v1 three-OS candidate provenance."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

import blake3


class ProvenanceError(RuntimeError):
    """Candidate provenance is incomplete, mixed, mutable, or untriaged."""


_HEX_32 = re.compile(r"^[0-9a-f]{64}$")
_GIT_ID = re.compile(r"^(?:[0-9a-f]{40}|[0-9a-f]{64})$")
_ACTION = re.compile(r"^\s*(?:-\s+)?uses:\s*([^\s#]+)\s*(?:#.*)?$", re.MULTILINE)
_FULL_SHA = re.compile(r"^[0-9a-f]{40}$")
_OS_SET = {"linux", "windows", "macos"}
_OS_TARGETS = {
    "linux": "x86_64-unknown-linux-gnu",
    "windows": "x86_64-pc-windows-msvc",
    "macos": "aarch64-apple-darwin",
}
_REVIEWED_ACTIONS = {
    "actions/checkout": "fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09",
    "actions/setup-python": "ece7cb06caefa5fff74198d8649806c4678c61a1",
    "actions/setup-node": "a0853c24544627f65ddf259abe73b1d18a591444",
    "dart-lang/setup-dart": "65eb853c7ba17dde3be364c3d2858773e7144260",
    "actions/upload-artifact": "ea165f8d65b6e75b540449e92b4886f43607fa02",
    "actions/download-artifact": "634f93cb2916e3fdff6788551b99b062d0335ce0",
}
REVIEWED_AUDIT_ITEMS = {
    advisory: {"id": advisory, "severity": "P3", "triage": "documented-non-base"}
    for advisory in (
        "RUSTSEC-2024-0370",
        "RUSTSEC-2024-0411",
        "RUSTSEC-2024-0412",
        "RUSTSEC-2024-0413",
        "RUSTSEC-2024-0414",
        "RUSTSEC-2024-0415",
        "RUSTSEC-2024-0416",
        "RUSTSEC-2024-0417",
        "RUSTSEC-2024-0418",
        "RUSTSEC-2024-0419",
        "RUSTSEC-2024-0420",
        "RUSTSEC-2025-0075",
        "RUSTSEC-2025-0080",
        "RUSTSEC-2025-0081",
        "RUSTSEC-2025-0098",
        "RUSTSEC-2025-0100",
    )
}
REVIEWED_AUDIT_ITEMS.update({
    advisory: {"id": advisory, "severity": "P2", "triage": "documented-non-base"}
    for advisory in ("RUSTSEC-2024-0429", "RUSTSEC-2026-0221")
})
_IDENTITY_FIELDS = (
    "release_request_digest",
    "qualification_session_id",
    "candidate_commit",
    "candidate_tree",
    "candidate_semantic_digest",
)
_BUNDLE_FIELDS = {
    "format", "qualification_mode", "workflow_path", "action_allowlist", "lanes",
    *_IDENTITY_FIELDS,
}
_LANE_FIELDS = {
    "qualification_mode", "os", "target_triple", "toolchain_digest", "runner_image",
    "artifact_tuple_digest", "executable_path", "executable_blake3", "compiler_path",
    "compiler_blake3", "sbom_path", "sbom_blake3", "workflow_sha256", "raw_audits",
    "audit_items", *_IDENTITY_FIELDS,
}


def _object(value: object, label: str) -> dict[str, object]:
    if not isinstance(value, dict):
        raise ProvenanceError(f"{label} must be an object")
    return value


def _b3(path: Path, label: str) -> str:
    try:
        return blake3.blake3(path.read_bytes()).hexdigest()
    except OSError as error:
        raise ProvenanceError(f"cannot read {label}: {error}") from error


def _sha256(path: Path, label: str) -> str:
    try:
        return hashlib.sha256(path.read_bytes()).hexdigest()
    except OSError as error:
        raise ProvenanceError(f"cannot read {label}: {error}") from error


def _git(repo_root: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", *args], cwd=repo_root, text=True, capture_output=True, check=False
    )
    if result.returncode != 0:
        raise ProvenanceError(f"git {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout.strip()


def _validate_actions(workflow: Path, allowlist: object) -> str:
    policy = _object(allowlist, "action allowlist")
    if policy != _REVIEWED_ACTIONS:
        raise ProvenanceError("action allowlist differs from the frozen reviewed policy")
    payload = workflow.read_bytes()
    text = payload.decode("utf-8")
    references = _ACTION.findall(text)
    if not references:
        raise ProvenanceError("workflow has no reviewed action reference")
    for reference in references:
        if reference.startswith("./"):
            continue
        if "@" not in reference:
            raise ProvenanceError(f"action reference has no immutable revision: {reference}")
        action, revision = reference.rsplit("@", 1)
        if not _FULL_SHA.fullmatch(revision):
            raise ProvenanceError(f"action reference is mutable: {reference}")
        if policy.get(action) != revision:
            raise ProvenanceError(f"action reference is unknown or not allowlisted: {reference}")
    return hashlib.sha256(payload).hexdigest()


def _validate_source(repo_root: Path, commit: str, tree: str) -> None:
    if _git(repo_root, "rev-parse", "HEAD") != commit:
        raise ProvenanceError("candidate_commit does not match repository HEAD")
    if _git(repo_root, "rev-parse", "HEAD^{tree}") != tree:
        raise ProvenanceError("candidate_tree does not match repository tree")
    status = _git(
        repo_root,
        "status",
        "--porcelain=v1",
        "--untracked-files=all",
        "--ignored=matching",
    )
    if status:
        raise ProvenanceError(f"source tree is not byte-clean (tracked/untracked/ignored): {status}")


def _expect_hex(value: object, label: str, pattern: re.Pattern[str] = _HEX_32) -> str:
    if not isinstance(value, str) or not pattern.fullmatch(value):
        raise ProvenanceError(f"{label} must be lowercase hex")
    return value


def _validate_audits(lane: dict[str, object]) -> dict[str, dict[str, str]]:
    raw = _object(lane.get("raw_audits"), "raw audits")
    if set(raw) != {"cargo", "npm"}:
        raise ProvenanceError("raw audits must preserve cargo and npm reports")
    preserved: dict[str, dict[str, str]] = {}
    detected: set[str] = set()
    for ecosystem in sorted(raw):
        item = _object(raw[ecosystem], f"{ecosystem} audit")
        if set(item) != {"path", "blake3"}:
            raise ProvenanceError(f"{ecosystem} raw audit fields are not closed")
        path = Path(str(item.get("path", ""))).resolve()
        expected = _expect_hex(item.get("blake3"), f"{ecosystem} audit digest")
        if _b3(path, f"{ecosystem} audit") != expected:
            raise ProvenanceError(f"{ecosystem} raw audit digest mismatch")
        try:
            report = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            raise ProvenanceError(f"{ecosystem} raw audit is not valid JSON: {error}") from error
        if ecosystem == "cargo":
            vulnerabilities = _object(report.get("vulnerabilities", {}), "cargo vulnerabilities")
            for finding in vulnerabilities.get("list", []):
                advisory = _object(_object(finding, "cargo finding").get("advisory"), "cargo advisory")
                if isinstance(advisory.get("id"), str):
                    detected.add(advisory["id"])
            warnings = _object(report.get("warnings", {}), "cargo warnings")
            for warning_list in warnings.values():
                if not isinstance(warning_list, list):
                    raise ProvenanceError("cargo warning group must be an array")
                for warning in warning_list:
                    warning_value = _object(warning, "cargo warning")
                    advisory = warning_value.get("advisory", warning_value)
                    if isinstance(advisory, dict) and isinstance(advisory.get("id"), str):
                        detected.add(advisory["id"])
        else:
            vulnerabilities = _object(report.get("vulnerabilities", {}), "npm vulnerabilities")
            detected.update(str(identifier) for identifier in vulnerabilities)
        preserved[ecosystem] = {"path": str(path), "blake3": expected}
    items = lane.get("audit_items")
    if not isinstance(items, list):
        raise ProvenanceError("audit_items must be an array")
    item_ids: set[str] = set()
    for raw_item in items:
        item = _object(raw_item, "audit item")
        identifier = item.get("id")
        if not isinstance(identifier, str) or not identifier or identifier in item_ids:
            raise ProvenanceError("audit item id is missing or duplicated")
        item_ids.add(identifier)
        if identifier in detected and item != REVIEWED_AUDIT_ITEMS.get(identifier):
            raise ProvenanceError(
                f"raw dependency advisory is missing triage from the reviewed policy: {identifier}"
            )
        if item.get("severity") in {"P0", "P1"} and item.get("triage") not in {
            "resolved", "accepted-risk"
        }:
            raise ProvenanceError(
                f"untriaged {item.get('severity')} dependency finding: {item.get('id')}"
            )
    missing = sorted(detected - item_ids)
    if missing:
        raise ProvenanceError(f"raw dependency advisory is missing triage: {missing}")
    return preserved


def verify_provenance(bundle_value: object, repo_root: Path) -> dict[str, object]:
    """Recompute every local byte binding and derive a verified aggregate receipt."""
    bundle = _object(bundle_value, "provenance bundle")
    if set(bundle) != _BUNDLE_FIELDS:
        raise ProvenanceError("provenance bundle fields are not closed")
    if bundle.get("format") != "onebrain/base-v1-provenance/1":
        raise ProvenanceError("provenance format is unsupported")
    if bundle.get("qualification_mode") not in {"prequalification", "release"}:
        raise ProvenanceError("qualification_mode is invalid")
    for field in _IDENTITY_FIELDS:
        pattern = _GIT_ID if field in {"candidate_commit", "candidate_tree"} else _HEX_32
        _expect_hex(bundle.get(field), field, pattern)
    lanes = bundle.get("lanes")
    if not isinstance(lanes, list):
        raise ProvenanceError("lanes must be an array")
    lane_by_os: dict[str, dict[str, object]] = {}
    for raw_lane in lanes:
        lane = _object(raw_lane, "OS lane")
        if set(lane) != _LANE_FIELDS:
            raise ProvenanceError("OS lane fields are not closed")
        os_name = lane.get("os")
        if not isinstance(os_name, str) or os_name in lane_by_os:
            raise ProvenanceError(f"duplicate or invalid OS lane: {os_name}")
        lane_by_os[os_name] = lane
    if set(lane_by_os) != _OS_SET:
        raise ProvenanceError(f"required OS lanes are {_OS_SET}, got {set(lane_by_os)}")
    workflow = Path(str(bundle.get("workflow_path", ""))).resolve()
    expected_workflow = (repo_root.resolve() / ".github/workflows/base-v1-candidate.yml").resolve()
    if workflow != expected_workflow:
        raise ProvenanceError("workflow path is not the candidate-owned Base workflow")
    try:
        workflow_sha = _validate_actions(workflow, bundle.get("action_allowlist"))
    except OSError as error:
        raise ProvenanceError(f"cannot read workflow: {error}") from error
    artifact_tuples: set[str] = set()
    target_triples: set[str] = set()
    lane_receipts: dict[str, dict[str, object]] = {}
    for os_name in sorted(lane_by_os):
        lane = lane_by_os[os_name]
        if lane.get("qualification_mode") != bundle["qualification_mode"]:
            raise ProvenanceError(f"mixed or wrong qualification_mode in {os_name} lane")
        for field in _IDENTITY_FIELDS:
            if lane.get(field) != bundle[field]:
                raise ProvenanceError(f"mixed or wrong {field} in {os_name} lane")
        if lane.get("workflow_sha256") != workflow_sha:
            raise ProvenanceError(f"workflow digest mismatch in {os_name} lane")
        executable = Path(str(lane.get("executable_path", ""))).resolve()
        executable_digest = _expect_hex(lane.get("executable_blake3"), "executable digest")
        if _b3(executable, "executable") != executable_digest:
            raise ProvenanceError(f"executable digest mismatch in {os_name} lane")
        sbom = Path(str(lane.get("sbom_path", ""))).resolve()
        sbom_digest = _expect_hex(lane.get("sbom_blake3"), "SBOM digest")
        if _b3(sbom, "SBOM") != sbom_digest:
            raise ProvenanceError(f"SBOM digest mismatch in {os_name} lane")
        try:
            sbom_data = json.loads(sbom.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            raise ProvenanceError(f"SBOM is not valid JSON in {os_name} lane: {error}") from error
        if sbom_data.get("spdxVersion") != "SPDX-2.3":
            raise ProvenanceError(f"SBOM format mismatch in {os_name} lane")
        target = lane.get("target_triple")
        runner_image = lane.get("runner_image")
        if target != _OS_TARGETS[os_name]:
            raise ProvenanceError(f"target triple mismatch in {os_name} lane")
        if not isinstance(runner_image, str) or "@" not in runner_image:
            raise ProvenanceError(f"target/compiler runner provenance missing in {os_name} lane")
        if target in target_triples:
            raise ProvenanceError("OS lanes must have distinct target triples")
        target_triples.add(target)
        toolchain = _expect_hex(lane.get("toolchain_digest"), "toolchain digest")
        binding = _object(sbom_data.get("onebrainCandidateBinding"), "SBOM candidate binding")
        expected_binding = {
            "format": "onebrain/base-v1-candidate-binding/1",
            **{field: bundle[field] for field in _IDENTITY_FIELDS},
            "target_triple": target,
            "toolchain_digest": toolchain,
        }
        if set(binding) != set(expected_binding) | {"created_utc"} or any(
            binding.get(field) != value for field, value in expected_binding.items()
        ):
            raise ProvenanceError(f"SBOM candidate binding mismatch in {os_name} lane")
        compiler = Path(str(lane.get("compiler_path", ""))).resolve()
        compiler_digest = _expect_hex(lane.get("compiler_blake3"), "compiler digest")
        if _b3(compiler, "compiler evidence") != compiler_digest:
            raise ProvenanceError(f"compiler evidence digest mismatch in {os_name} lane")
        artifact_tuple = _expect_hex(lane.get("artifact_tuple_digest"), "artifact tuple digest")
        if artifact_tuple in artifact_tuples:
            raise ProvenanceError("an OS artifact copied another target's artifact tuple")
        artifact_tuples.add(artifact_tuple)
        audits = _validate_audits(lane)
        lane_receipts[os_name] = {
            "target_triple": target,
            "toolchain_digest": toolchain,
            "runner_image": runner_image,
            "compiler_blake3": compiler_digest,
            "workflow_sha256": workflow_sha,
            "artifact_tuple_digest": artifact_tuple,
            "executable_blake3": executable_digest,
            "sbom_blake3": sbom_digest,
            "raw_audits": audits,
        }
    _validate_source(repo_root.resolve(), str(bundle["candidate_commit"]), str(bundle["candidate_tree"]))
    return {
        "format": "onebrain/base-v1-provenance-receipt/1",
        "verified": True,
        "qualification_mode": bundle["qualification_mode"],
        **{field: bundle[field] for field in _IDENTITY_FIELDS},
        "workflow_sha256": workflow_sha,
        "lane_receipts": lane_receipts,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", type=Path, required=True)
    parser.add_argument("--repo-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        bundle = json.loads(args.bundle.read_text(encoding="utf-8"))
        receipt = verify_provenance(bundle, args.repo_root)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        with args.output.open("x", encoding="utf-8", newline="\n") as stream:
            json.dump(receipt, stream, sort_keys=True, separators=(",", ":"))
            stream.write("\n")
    except (OSError, json.JSONDecodeError, ProvenanceError) as error:
        print(f"Base provenance verification failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
