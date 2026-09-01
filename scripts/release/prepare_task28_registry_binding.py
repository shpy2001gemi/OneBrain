#!/usr/bin/env python3
"""Derive the fresh Registry measurement expectation for one Task 28 request.

This output is not an authority statement.  It is a closed expectation file
whose paths and digests are independently remeasured by every Registry receipt
producer before the Registry signer is called.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

import blake3

from scripts.release.verify_base_release_request import (
    ARTIFACT_FIELDS,
    VerifiedQualificationContextV2,
    bind_task28_registry_measurements,
    blake3_file,
    canonical_compatibility_tuple_bytes,
    canonical_json,
)


class RegistryBindingError(RuntimeError):
    """The staged Registry candidate cannot form a closed v2 binding."""


def _json(path: Path, label: str) -> dict[str, object]:
    try:
        encoded = path.read_bytes()
        value = json.loads(encoded)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise RegistryBindingError(f"{label} is invalid JSON") from error
    if not isinstance(value, dict):
        raise RegistryBindingError(f"{label} must be a JSON object")
    return value


def _verified_context(path: Path) -> VerifiedQualificationContextV2:
    value = _json(path, "verified Task 28 context")
    if value.get("format") != "onebrain/verified-qualification-context/2" or value.get("production") is not True:
        raise RegistryBindingError("production verified Task 28 context v2 is required")
    required = {
        "format", "production", "request_digest", "signer_fingerprint",
        "trust_policy_digest", "run_context", "bindings", "tooling_blake3",
    }
    if set(value) != required:
        raise RegistryBindingError("verified Task 28 context fields are not closed")
    bindings = value["bindings"]
    if not isinstance(bindings, dict):
        raise RegistryBindingError("verified Task 28 bindings are invalid")
    request = {
        "required_targets": bindings["required_targets"],
    }
    return VerifiedQualificationContextV2(
        request_digest=str(value["request_digest"]),
        signer_fingerprint=str(value["signer_fingerprint"]),
        trust_policy_digest=str(value["trust_policy_digest"]),
        run_context=dict(value["run_context"]),
        bindings=dict(bindings),
        tooling_blake3=dict(value["tooling_blake3"]),
        request=request,
        production=True,
    )


def derive_binding(args: argparse.Namespace) -> dict[str, object]:
    verified = _verified_context(args.verified_context)
    stamp = _json(args.release_stamp, "Registry release stamp")
    state = _json(args.registry_state, "Registry state")
    semantic_value = _json(args.candidate_semantic_evidence, "candidate semantic evidence")
    policy = _json(args.registry_trust_policy, "Registry trust policy")
    if args.target_triple != verified.request["required_targets"]["linux"]:
        raise RegistryBindingError("Registry target differs from the signed Linux target")
    payloads = {
        name: args.installed_release / name.split(":", 1)[1]
        for name in ARTIFACT_FIELDS
    }
    measured_payloads = {name: blake3_file(path) for name, path in payloads.items()}
    stamp_payloads = {
        f"{row['role']}:{row['relative_path']}": row["blake3"]
        for row in stamp.get("artifacts", [])
        if isinstance(row, dict)
    }
    if stamp_payloads != measured_payloads:
        raise RegistryBindingError("Registry stamp payload tuple differs from installed bytes")
    if state.get("active_release") != stamp.get("release_id"):
        raise RegistryBindingError("Registry state does not activate the staged release")
    semantic = blake3.blake3(
        canonical_compatibility_tuple_bytes(semantic_value, include_artifact_fields=False),
        derive_key_context="onebrain:base:candidate-semantic:1\0",
    ).hexdigest()
    artifact = blake3.blake3(
        canonical_compatibility_tuple_bytes(semantic_value, include_artifact_fields=True),
        derive_key_context="onebrain:base:artifact-tuple:1\0",
    ).hexdigest()
    signers = policy.get("signers")
    if not isinstance(signers, list) or len(signers) != 1 or not isinstance(signers[0], dict):
        raise RegistryBindingError("Registry trust policy signer set is not closed")
    signer = signers[0]
    ccid_paths = {
        "old_input": args.previous_root / "input.jsonl",
        "old_obr": args.previous_root / "concepts.obr",
        "old_manifest": args.previous_root / "concepts.obr.manifest.json",
        "candidate_input": args.candidate_root / "input.jsonl",
        "candidate_obr": args.candidate_root / "concepts.obr",
        "candidate_manifest": args.candidate_root / "concepts.obr.manifest.json",
    }
    binding = {
        "candidate_semantic_digest": semantic,
        "artifact_tuple_digest": artifact,
        "release_aggregate_root": stamp["artifact_root"],
        "registry_generation": state["generation"],
        "candidate_payload_artifacts_blake3": measured_payloads,
        "release_stamp_blake3": blake3_file(args.release_stamp),
        "trust_policy_digest": blake3.blake3(
            canonical_json(policy),
            derive_key_context="onebrain:concept-registry:trust-policy:1",
        ).hexdigest(),
        "signer_fingerprint": signer["fingerprint_hex"],
        "ccid_inputs_blake3": {name: blake3_file(path) for name, path in ccid_paths.items()},
        "probe_blake3": blake3_file(args.probe),
        "probe_signature": blake3_file(args.probe_signature),
        "probe_signer_fingerprint": signer["fingerprint_hex"],
        "probe_signer_public_key": signer["public_key_hex"],
        "executable_blake3": blake3_file(args.probe),
        "rust_toolchain_digest": blake3_file(args.rust_toolchain_evidence),
        "runner_image_digest": blake3_file(args.runner_image_evidence),
        "target_triple": args.target_triple,
    }
    bind_task28_registry_measurements(verified, binding)
    return binding


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    for name in (
        "verified-context", "candidate-root", "previous-root", "installed-release",
        "release-stamp", "registry-state", "candidate-semantic-evidence",
        "registry-trust-policy", "probe", "probe-signature",
        "rust-toolchain-evidence", "runner-image-evidence", "output",
    ):
        parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument("--target-triple", required=True)
    args = parser.parse_args(argv)
    try:
        binding = derive_binding(args)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_bytes(canonical_json(binding) + b"\n")
    except (OSError, KeyError, TypeError, ValueError, RegistryBindingError, RuntimeError) as error:
        parser.error(str(error))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
