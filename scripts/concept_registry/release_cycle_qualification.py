#!/usr/bin/env python3
"""Run and sign the complete Concept Registry release-cycle qualification."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

from production_qualification import (
    AggregationError,
    create_signed_receipt,
    parse_qualification_run_context,
    trust_policy_digest,
)


REQUIRED_STEPS = (
    "package",
    "verify",
    "activate",
    "query",
    "build-new-signed-generation",
    "ccid-diff",
    "activate-new",
    "rollback",
    "reactivate-new",
)
MAX_STEP_OUTPUT_BYTES = 4 * 1024 * 1024


class CycleError(RuntimeError):
    """The signed release cycle did not complete exactly."""


def _run_step(name: str, command: list[str]) -> dict[str, object]:
    if not command or not all(isinstance(value, str) and value for value in command):
        raise CycleError(f"{name} command is invalid")
    if any("quarterly_update.py" in Path(value).name.lower() for value in command):
        raise CycleError("quarterly_update.py is not a signed release-cycle harness")
    try:
        result = subprocess.run(
            command,
            capture_output=True,
            timeout=3600,
            check=False,
        )
    except (OSError, subprocess.SubprocessError) as error:
        raise CycleError(f"{name} command failed: {error}") from error
    if len(result.stdout) > MAX_STEP_OUTPUT_BYTES:
        raise CycleError(f"{name} output exceeds the evidence limit")
    if result.returncode != 0:
        detail = result.stderr.decode("utf-8", errors="replace").strip()[-2000:]
        raise CycleError(f"{name} command exited {result.returncode}: {detail}")
    try:
        value = json.loads(result.stdout.decode("utf-8", errors="strict"))
    except (UnicodeError, json.JSONDecodeError) as error:
        raise CycleError(f"{name} command did not emit one JSON result") from error
    if not isinstance(value, dict) or value.get("step") != name:
        raise CycleError(f"{name} command returned the wrong step identity")
    if value.get("result") is not True:
        raise CycleError(f"{name} result is false")
    return value


def run_release_cycle(
    plan: dict[str, object],
    run_context: dict[str, object],
    binding: dict[str, object],
    signing_key: Ed25519PrivateKey,
    policy: dict[str, object],
) -> dict[str, object]:
    try:
        context = parse_qualification_run_context(run_context)
    except AggregationError as error:
        raise CycleError(str(error)) from error
    if not isinstance(plan, dict) or set(plan) != {
        "previous_release_aggregate_root",
        "steps",
    }:
        raise CycleError("release-cycle plan fields are not closed")
    old_root = plan.get("previous_release_aggregate_root")
    new_root = binding.get("release_aggregate_root")
    if not isinstance(old_root, str) or not isinstance(new_root, str):
        raise CycleError("release-cycle roots are missing")
    steps = plan.get("steps")
    if not isinstance(steps, list):
        raise CycleError("release-cycle steps are missing")
    names = [value.get("name") if isinstance(value, dict) else None for value in steps]
    if names != list(REQUIRED_STEPS) or len(set(names)) != len(REQUIRED_STEPS):
        raise CycleError("every required release-cycle step must appear exactly once in order")
    final_generation = binding.get("registry_generation")
    if (
        isinstance(final_generation, bool)
        or not isinstance(final_generation, int)
        or final_generation < 4
    ):
        raise CycleError("final registry generation must allow the activation cycle")

    observed = []
    for value in steps:
        assert isinstance(value, dict)
        if set(value) != {"name", "command"} or not isinstance(value.get("command"), list):
            raise CycleError(f"{value.get('name')} step fields are not closed")
        observed.append(_run_step(str(value["name"]), value["command"]))

    expected_roots = {
        "package": old_root,
        "verify": old_root,
        "activate": old_root,
        "query": old_root,
        "build-new-signed-generation": new_root,
        "ccid-diff": new_root,
        "activate-new": new_root,
        "rollback": old_root,
        "reactivate-new": new_root,
    }
    expected_generations = {
        "activate": final_generation - 3,
        "query": final_generation - 3,
        "build-new-signed-generation": final_generation - 3,
        "ccid-diff": final_generation - 3,
        "activate-new": final_generation - 2,
        "rollback": final_generation - 1,
        "reactivate-new": final_generation,
    }
    for result in observed:
        name = str(result["step"])
        if result.get("observed_release_root") != expected_roots[name]:
            raise CycleError(f"{name} observed the wrong release aggregate root")
        if name in expected_generations and result.get("registry_generation") != expected_generations[name]:
            raise CycleError(f"{name} observed the wrong registry generation")

    if binding.get("trust_policy_digest") != trust_policy_digest(policy):
        raise CycleError("release-cycle trust_policy_digest mismatch")
    payload: dict[str, object] = {
        **binding,
        "command": ["release_cycle_qualification.py", *REQUIRED_STEPS],
        "result": True,
        "exit_oracles": {
            "all_required_steps_executed_once_in_order": True,
            "all_step_results_true": True,
            "old_and_candidate_roots_exact": True,
            "activation_generations_are_monotonic": True,
            "rollback_restored_exact_old_root": True,
            "reactivation_restored_exact_candidate_root": True,
            "quarterly_update_not_used": True,
        },
        "limitations": [
            "Registry-only release-cycle evidence; never BASE-GATE-V1",
            "The harness consumes previously verified release context and does not define the Base release-request signer",
        ],
        "previous_release_aggregate_root": old_root,
        "steps": observed,
    }
    if context["variant"] == "Prequalification":
        for field in ("candidate_semantic_digest", "artifact_tuple_digest"):
            payload.pop(field, None)
        payload.update(
            {
                "qualification_context_variant": "Prequalification",
                "closure_digest": context["closure_digest"],
                "base_candidate_bound": False,
            }
        )
    else:
        payload.update(
            {
                "qualification_context_variant": "Release",
                "release_request_digest": context["release_request_digest"],
                "qualification_session_id": context["qualification_session_id"],
                "candidate_commit": context["candidate_commit"],
                "candidate_tree": context["candidate_tree"],
                "base_candidate_bound": True,
            }
        )
    try:
        return create_signed_receipt(
            "signed-release-cycle", payload, signing_key, policy
        )
    except AggregationError as error:
        raise CycleError(str(error)) from error


def _read_json(path: Path) -> dict[str, object]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise CycleError(f"JSON input is not an object: {path}")
    return value


def _read_key(path: Path) -> Ed25519PrivateKey:
    try:
        value = path.read_text(encoding="ascii").strip()
    except OSError as error:
        raise CycleError("private signing key could not be read") from error
    if len(value) != 64 or any(character not in "0123456789abcdef" for character in value):
        raise CycleError("private signing key must be exactly 64 lowercase hex digits")
    return Ed25519PrivateKey.from_private_bytes(bytes.fromhex(value))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--binding", type=Path, required=True)
    parser.add_argument("--run-context", type=Path, required=True)
    parser.add_argument("--trust-policy", type=Path, required=True)
    parser.add_argument("--private-key", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        receipt = run_release_cycle(
            _read_json(args.plan),
            _read_json(args.run_context),
            _read_json(args.binding),
            _read_key(args.private_key),
            _read_json(args.trust_policy),
        )
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(
            json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
    except (OSError, ValueError, json.JSONDecodeError, CycleError) as error:
        print(f"Concept Registry release-cycle qualification failed: {error}", file=sys.stderr)
        return 2
    print(json.dumps(receipt, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
