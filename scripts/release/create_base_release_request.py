#!/usr/bin/env python3
"""Create an immutable, qualification-approver-signed Base v1 request."""

from __future__ import annotations

import argparse
import json
import os
import secrets
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Callable, Mapping

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

import blake3

from scripts.release.verify_base_release_request import (
    FROZEN_APPROVER_POLICY,
    REQUEST_FIELDS,
    TASK28_REQUEST_FIELDS,
    TOOLING_FIELDS,
    _policy,
    _validate_request,
    canonical_json,
    verify_task28_release_request as verify_task28_release_request_context,
)


TARGETS = {
    "x86_64-unknown-linux-gnu",
    "x86_64-pc-windows-msvc",
    "aarch64-apple-darwin",
}
REQUEST_VALIDITY = timedelta(hours=168)
CANONICAL_PROFILE = Path("src/test-vectors/vnext/base-v1-freeze-v1.json")
CANONICAL_VECTOR = Path("src/test-vectors/vnext/base-v1-release-signers-v1.json")
CANONICAL_HISTORY = Path("src/test-vectors/vnext/base-v1-runtime-interface-history-v1.json")
CANONICAL_TOOLING = {
    "qualifier": Path("scripts/base/qualify_base.py"),
    "request": Path("scripts/release/create_base_release_request.py"),
    "clean_worktree": Path("scripts/release/prepare_clean_candidate.py"),
    "release_wrapper": Path("scripts/release/create_verified_base_release.py"),
    "verifier": Path("scripts/release/verify_base_release_request.py"),
    "signer_policy": CANONICAL_VECTOR,
}


class ReleaseRequestCreationError(RuntimeError):
    """The immutable request could not be safely created."""


@dataclass(frozen=True)
class CreatedReleaseRequest:
    request_digest: str
    request_path: Path
    signature_path: Path


def _run_git(root: Path, *arguments: str) -> str:
    completed = subprocess.run(
        ["git", "-C", str(root), *arguments],
        capture_output=True,
        text=True,
        check=False,
    )
    if completed.returncode != 0:
        raise ReleaseRequestCreationError(f"Git {' '.join(arguments)} failed")
    return completed.stdout.strip()


def _json_file(path: Path, label: str) -> tuple[object, bytes]:
    try:
        payload = path.read_bytes()
        value = json.loads(payload)
    except (OSError, json.JSONDecodeError) as error:
        raise ReleaseRequestCreationError(f"{label} is not JSON") from error
    return value, payload


def _digest_file(path: Path, label: str) -> str:
    try:
        return blake3.blake3(path.read_bytes()).hexdigest()
    except OSError as error:
        raise ReleaseRequestCreationError(f"{label} cannot be read") from error


def _candidate_path(root: Path, supplied: Path, relative: Path, label: str) -> Path:
    expected = (root / relative).resolve(strict=True)
    if supplied.resolve(strict=True) != expected:
        raise ReleaseRequestCreationError(f"{label} is not the canonical candidate path")
    relative_text = relative.as_posix()
    tracked = subprocess.run(
        ["git", "-C", str(root), "ls-files", "--error-unmatch", "--", relative_text],
        capture_output=True,
        check=False,
    )
    mode = _run_git(root, "ls-tree", "HEAD", "--", relative_text).split()[0]
    if tracked.returncode != 0 or mode not in {"100644", "100755"}:
        raise ReleaseRequestCreationError(f"{label} is not a tracked regular candidate file")
    return expected


def _require_external_output_root(candidate_root: Path, output_root: Path) -> Path:
    root = candidate_root.resolve(strict=True)
    output = output_root.resolve()
    try:
        output.relative_to(root)
    except ValueError:
        return output
    raise ReleaseRequestCreationError(
        "Task 28 request output root must remain outside the candidate"
    )


def _approver_policy(vector: object) -> tuple[dict[str, object], str]:
    try:
        if not isinstance(vector, dict) or vector.get("format") != "onebrain/base-v1-release-signers/1":
            raise ValueError("format")
        rows = [row for row in vector["policies"] if row["policy"].get("role") == "qualification-approver"]
        if len(rows) != 1:
            raise ValueError("role")
        policy, digest = _policy(rows[0]["policy"], production=True)
        if rows[0]["digest"] != {
            "algorithm": "BLAKE3 derive-key",
            "context": "onebrain:base-v1:qualification-approver-policy:1",
            "expected_hex": digest,
        }:
            raise ValueError("digest")
        return policy, digest
    except (KeyError, TypeError, ValueError) as error:
        raise ReleaseRequestCreationError("qualification approver signer vector is invalid") from error


def create_release_request(
    *,
    candidate_root: Path,
    output_root: Path,
    approver_policy_path: Path,
    signer_fingerprint: str,
    evidence_root_uri: str,
    required_targets: Mapping[str, str],
    production_profile_path: Path,
    production_vector_path: Path,
    append_only_idl_history_path: Path,
    candidate_tooling: Mapping[str, Path],
    registry_candidate: Mapping[str, object],
    reference_environment: Mapping[str, object],
    created_utc: datetime,
    expires_utc: datetime,
    sign_detached: Callable[[bytes, str], bytes],
    verify_detached: Callable[[bytes, bytes, str], bool],
    resume: bool = False,
    resume_request_path: Path | None = None,
    session_id: str | None = None,
) -> CreatedReleaseRequest:
    root = candidate_root.resolve(strict=True)
    if _run_git(root, "status", "--porcelain", "--untracked-files=all", "--ignored=matching"):
        raise ReleaseRequestCreationError("candidate worktree is dirty, untracked or ignored")
    object_format = _run_git(root, "rev-parse", "--show-object-format")
    commit = _run_git(root, "rev-parse", "HEAD")
    tree = _run_git(root, "rev-parse", "HEAD^{tree}")
    if object_format not in {"sha1", "sha256"}:
        raise ReleaseRequestCreationError("candidate Git object format is unsupported")
    if set(required_targets) != TARGETS:
        raise ReleaseRequestCreationError("release request requires the exact three target map")
    if set(candidate_tooling) != TOOLING_FIELDS:
        raise ReleaseRequestCreationError("candidate tooling map is not exact")
    if created_utc.tzinfo is None or expires_utc.tzinfo is None or created_utc >= expires_utc:
        raise ReleaseRequestCreationError("release request validity interval is invalid")
    if expires_utc - created_utc != REQUEST_VALIDITY:
        raise ReleaseRequestCreationError("release request validity must be exactly 168 hours")

    canonical_profile = _candidate_path(root, production_profile_path, CANONICAL_PROFILE, "production profile")
    canonical_vector = _candidate_path(root, production_vector_path, CANONICAL_VECTOR, "production vector")
    canonical_history = _candidate_path(root, append_only_idl_history_path, CANONICAL_HISTORY, "append-only IDL history")
    canonical_tools = {
        name: _candidate_path(root, candidate_tooling[name], relative, f"candidate tooling {name}")
        for name, relative in CANONICAL_TOOLING.items()
    }

    if approver_policy_path.resolve(strict=True) != canonical_vector:
        raise ReleaseRequestCreationError("approver policy is not the candidate signer vector")
    policy_vector, policy_bytes = _json_file(canonical_vector, "approver policy vector")
    try:
        policy, policy_digest = _approver_policy(policy_vector)
    except Exception as error:
        raise ReleaseRequestCreationError("qualification approver policy is not frozen") from error
    signer = policy["signers"][0]
    if signer_fingerprint != signer["fingerprint"]:
        raise ReleaseRequestCreationError("qualification approver fingerprint is not allowlisted")
    if canonical_tools["signer_policy"] != canonical_vector:
        raise ReleaseRequestCreationError("candidate signer-policy tool is not the signed policy bytes")

    _, profile_bytes = _json_file(canonical_profile, "production profile")
    _, vector_bytes = _json_file(canonical_vector, "production vector")
    history_value, _ = _json_file(canonical_history, "append-only IDL history")
    try:
        history_root = history_value["history_chain"]["root_sha256"]
    except (KeyError, TypeError) as error:
        raise ReleaseRequestCreationError("append-only IDL history root is missing") from error
    tooling_digests = {
        name: _digest_file(path, f"candidate tooling {name}")
        for name, path in canonical_tools.items()
    }
    prior_request: dict[str, object] | None = None
    if resume:
        if resume_request_path is None:
            raise ReleaseRequestCreationError("resume requires an explicit prior request identity")
        try:
            prior_bytes = resume_request_path.resolve(strict=True).read_bytes()
            prior_request = json.loads(prior_bytes)
        except (OSError, json.JSONDecodeError) as error:
            raise ReleaseRequestCreationError("resume prior request is invalid") from error
        if prior_bytes != canonical_json(prior_request):
            raise ReleaseRequestCreationError("resume prior request is not canonical")
        created_utc = datetime.fromisoformat(str(prior_request["created_utc"]).replace("Z", "+00:00"))
        expires_utc = datetime.fromisoformat(str(prior_request["expires_utc"]).replace("Z", "+00:00"))
        if expires_utc - created_utc != REQUEST_VALIDITY:
            raise ReleaseRequestCreationError("resume prior request validity is not frozen")
        qualification_session_id = str(prior_request["qualification_session_id"])
        if session_id is not None and session_id != qualification_session_id:
            raise ReleaseRequestCreationError("resume session identity differs")
    else:
        if resume_request_path is not None:
            raise ReleaseRequestCreationError("prior request identity is only valid with resume")
        qualification_session_id = session_id or secrets.token_hex(32)
    request = {
        "format": "onebrain/base-v1-release-request/1",
        "usage": "base-release-request",
        "qualification_session_id": qualification_session_id,
        "candidate": {"commit": commit, "tree": tree, "object_format": object_format},
        "qualification_approver_fingerprint": signer_fingerprint,
        "trust_policy_digest": policy_digest,
        "required_targets": dict(sorted(required_targets.items())),
        "production_profile_blake3": blake3.blake3(profile_bytes).hexdigest(),
        "production_vector_blake3": blake3.blake3(vector_bytes).hexdigest(),
        "append_only_idl_history_root": history_root,
        "created_utc": created_utc.astimezone(timezone.utc).isoformat().replace("+00:00", "Z"),
        "expires_utc": expires_utc.astimezone(timezone.utc).isoformat().replace("+00:00", "Z"),
        "evidence_root_uri": evidence_root_uri,
        "candidate_tooling_blake3": tooling_digests,
        "registry_candidate": dict(registry_candidate),
        "reference_environment": dict(reference_environment),
    }
    if set(request) != REQUEST_FIELDS:
        raise ReleaseRequestCreationError("release request fields are not closed")
    try:
        _validate_request(request, policy_digest, signer, created_utc)
    except Exception as error:
        raise ReleaseRequestCreationError("release request binding is invalid") from error
    request_bytes = canonical_json(request)
    digest = blake3.blake3(request_bytes).hexdigest()
    request_dir = output_root.resolve() / digest
    request_path = request_dir / "request.json"
    signature_path = request_dir / "request.json.asc"
    result = CreatedReleaseRequest(digest, request_path, signature_path)

    if resume:
        assert prior_request is not None and resume_request_path is not None
        expected_prior_path = request_path.resolve()
        if resume_request_path.resolve() != expected_prior_path:
            raise ReleaseRequestCreationError("resume attempt path is not its content address")
        if canonical_json(prior_request) != request_bytes:
            raise ReleaseRequestCreationError("resume request bytes are not identical")
    if request_dir.exists():
        if not resume:
            raise ReleaseRequestCreationError("immutable release-request attempt already exists")
        try:
            existing_request = request_path.read_bytes()
            existing_signature = signature_path.read_bytes()
        except OSError as error:
            raise ReleaseRequestCreationError("resume attempt is incomplete") from error
        if existing_request != request_bytes:
            raise ReleaseRequestCreationError("resume request bytes are not identical")
        if not verify_detached(existing_request, existing_signature, signer_fingerprint):
            raise ReleaseRequestCreationError("resume signature is not byte-valid for the request")
        return result

    output_root.mkdir(parents=True, exist_ok=True)
    try:
        request_dir.mkdir()
        with request_path.open("xb") as handle:
            handle.write(request_bytes)
            handle.flush()
            os.fsync(handle.fileno())
        try:
            signature = sign_detached(request_bytes, signer_fingerprint)
        except Exception as error:
            raise ReleaseRequestCreationError("release request signing failed") from error
        if not isinstance(signature, bytes) or not signature:
            raise ReleaseRequestCreationError("release request signer returned no signature")
        if not verify_detached(request_bytes, signature, signer_fingerprint):
            raise ReleaseRequestCreationError("release request signature verification failed")
        with signature_path.open("xb") as handle:
            handle.write(signature)
            handle.flush()
            os.fsync(handle.fileno())
        return result
    except FileExistsError as error:
        raise ReleaseRequestCreationError("release request publication raced another writer") from error


def _gpg_callbacks(gpg: Path, gpg_home: Path):
    def sign(payload: bytes, fingerprint: str) -> bytes:
        with tempfile.TemporaryDirectory(prefix="onebrain-base-request-") as directory:
            root = Path(directory)
            source = root / "request.json"
            signature = root / "request.json.asc"
            source.write_bytes(payload)
            completed = subprocess.run(
                [str(gpg), "--homedir", str(gpg_home), "--batch", "--no-tty", "--local-user", fingerprint,
                 "--detach-sign", "--output", str(signature), str(source)],
                capture_output=True, check=False,
            )
            if completed.returncode != 0:
                raise ReleaseRequestCreationError("GPG detached signing failed")
            return signature.read_bytes()

    def verify(payload: bytes, signature_bytes: bytes, fingerprint: str) -> bool:
        with tempfile.TemporaryDirectory(prefix="onebrain-base-request-verify-") as directory:
            root = Path(directory)
            source = root / "request.json"
            signature = root / "request.json.asc"
            source.write_bytes(payload)
            signature.write_bytes(signature_bytes)
            completed = subprocess.run(
                [str(gpg), "--homedir", str(gpg_home), "--batch", "--no-tty", "--status-fd", "1",
                 "--verify", str(signature), str(source)],
                capture_output=True, text=True, check=False,
            )
            valid = [line.split() for line in completed.stdout.splitlines() if line.startswith("[GNUPG:] VALIDSIG ")]
            if completed.returncode != 0 or len(valid) != 1 or len(valid[0]) < 12:
                return False
            return valid[0][8] == "22" and valid[0][-1] == fingerprint

    return sign, verify


def _task28_request_value(path: Path) -> tuple[dict[str, object], bytes]:
    try:
        payload = path.read_bytes()
        value = json.loads(payload)
    except (OSError, json.JSONDecodeError) as error:
        raise ReleaseRequestCreationError("Task 28 release request is invalid") from error
    if not isinstance(value, dict) or set(value) != TASK28_REQUEST_FIELDS or payload != canonical_json(value):
        raise ReleaseRequestCreationError("Task 28 release request fields/bytes are not canonical")
    if value["format"] != "onebrain/base-v1-release-request/2" or value["usage"] != "base-release-request":
        raise ReleaseRequestCreationError("Task 28 release request format is unsupported")
    return value, payload


def _default_gpg() -> Path:
    located = shutil.which("gpg") or shutil.which("gpg.exe")
    if not located:
        raise ReleaseRequestCreationError("GPG executable is unavailable")
    return Path(located).resolve(strict=True)


def _fsync_directory(path: Path) -> None:
    if os.name == "nt":
        return
    descriptor = os.open(path, os.O_RDONLY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def verify_task28_release_request(
    request_path: Path,
    signature_path: Path,
    signer_policy_path: Path,
    *,
    gpg_home: Path | None = None,
    gpg_executable: Path | None = None,
) -> dict[str, object]:
    _task28_request_value(request_path)
    gpg = gpg_executable.resolve(strict=True) if gpg_executable is not None else _default_gpg()
    try:
        verified = verify_task28_release_request_context(
            request_path,
            signature_path,
            signer_policy_path,
            gpg_home=gpg_home,
            gpg_executable=gpg,
            candidate_root=Path(__file__).resolve().parents[2],
        )
    except RuntimeError as error:
        raise ReleaseRequestCreationError(str(error)) from error
    return dict(verified.request)


def create_task28_release_request(
    *, candidate_root: Path, candidate_commit: str, output_root: Path, signer_policy_path: Path
) -> Path:
    root = candidate_root.resolve(strict=True)
    if _run_git(root, "status", "--porcelain", "--untracked-files=all", "--ignored=matching"):
        raise ReleaseRequestCreationError("Task 28 bootstrap candidate is not pristine")
    durable_output_root = _require_external_output_root(root, output_root)
    actual_commit = _run_git(root, "rev-parse", "HEAD")
    tree = _run_git(root, "rev-parse", "HEAD^{tree}")
    object_format = _run_git(root, "rev-parse", "--show-object-format")
    if candidate_commit != actual_commit:
        raise ReleaseRequestCreationError("Task 28 candidate commit differs from bootstrap HEAD")
    profile = _candidate_path(root, root / CANONICAL_PROFILE, CANONICAL_PROFILE, "freeze profile")
    vector = _candidate_path(root, signer_policy_path, CANONICAL_VECTOR, "signer vector")
    history = _candidate_path(root, root / CANONICAL_HISTORY, CANONICAL_HISTORY, "IDL history")
    tools = {
        name: _candidate_path(root, root / relative, relative, f"candidate tooling {name}")
        for name, relative in CANONICAL_TOOLING.items()
    }
    vector_value, _ = _json_file(vector, "signer vector")
    policy, policy_digest = _approver_policy(vector_value)
    signer = policy["signers"][0]
    freeze_value, freeze_bytes = _json_file(profile, "freeze profile")
    history_value, _ = _json_file(history, "IDL history")
    targets = freeze_value.get("targets")
    if targets != [
        "x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc", "aarch64-apple-darwin"
    ]:
        raise ReleaseRequestCreationError("Task 28 target contract differs")
    now = datetime.now(timezone.utc).replace(microsecond=0)
    session = secrets.token_hex(32)
    request = {
        "format": "onebrain/base-v1-release-request/2",
        "usage": "base-release-request",
        "qualification_session_id": session,
        "candidate": {"commit": actual_commit, "tree": tree, "object_format": object_format},
        "qualification_approver_fingerprint": signer["fingerprint"],
        "trust_policy_digest": policy_digest,
        "required_targets": {"linux": targets[0], "windows": targets[1], "macos": targets[2]},
        "production_profile_blake3": blake3.blake3(freeze_bytes).hexdigest(),
        "production_vector_blake3": blake3.blake3(vector.read_bytes()).hexdigest(),
        "append_only_idl_history_root": history_value["history_chain"]["root_sha256"],
        "created_utc": now.isoformat().replace("+00:00", "Z"),
        "expires_utc": (now + REQUEST_VALIDITY).isoformat().replace("+00:00", "Z"),
        "evidence_root_uri": (output_root.resolve().parent / "evidence" / "sessions" / session).as_uri(),
        "candidate_tooling_blake3": {name: _digest_file(path, name) for name, path in tools.items()},
    }
    payload = canonical_json(request)
    digest = blake3.blake3(payload).hexdigest()
    output_existed = durable_output_root.is_dir()
    durable_output_root.mkdir(parents=True, exist_ok=True)
    if not output_existed:
        _fsync_directory(durable_output_root.parent)
    directory = durable_output_root / digest
    request_path = directory / "release-request.json"
    signature_path = Path(f"{request_path}.asc")
    directory.mkdir(parents=True, exist_ok=False)
    _fsync_directory(durable_output_root)
    with request_path.open("xb") as handle:
        handle.write(payload)
        handle.flush()
        os.fsync(handle.fileno())
    gpg = _default_gpg()
    signed = subprocess.run(
        [str(gpg), "--batch", "--no-tty", "--local-user", str(signer["fingerprint"]), "--detach-sign", "--output", str(signature_path), str(request_path)],
        capture_output=True,
        check=False,
    )
    if signed.returncode != 0:
        raise ReleaseRequestCreationError("Task 28 approver signing failed")
    with signature_path.open("rb") as handle:
        os.fsync(handle.fileno())
    _fsync_directory(directory)
    verify_task28_release_request(request_path, signature_path, vector)
    return request_path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument("--new-attempt", action="store_true")
    modes.add_argument("--verify", type=Path, metavar="RELEASE_REQUEST")
    modes.add_argument("--print", choices=("candidate_commit", "candidate_tree", "qualification_session_id"))
    modes.add_argument("--resume", type=Path, metavar="RELEASE_REQUEST")
    parser.add_argument("--candidate-commit")
    parser.add_argument("--output-root", type=Path)
    parser.add_argument("--signer-policy", type=Path)
    parser.add_argument("--signature", type=Path)
    parser.add_argument("--request", type=Path)
    args = parser.parse_args()
    try:
        if args.new_attempt:
            if (
                args.candidate_commit is None
                or args.output_root is None
                or args.signer_policy is None
                or args.signature is not None
                or args.request is not None
            ):
                raise ReleaseRequestCreationError("new attempt arguments are incomplete")
            path = create_task28_release_request(
                candidate_root=Path(__file__).resolve().parents[2],
                candidate_commit=args.candidate_commit,
                output_root=args.output_root,
                signer_policy_path=args.signer_policy,
            )
            print(path)
            return 0
        if args.verify is not None:
            if (
                args.signature is None
                or args.signer_policy is None
                or args.candidate_commit is not None
                or args.output_root is not None
                or args.request is not None
            ):
                raise ReleaseRequestCreationError("verify arguments are incomplete")
            verify_task28_release_request(args.verify, args.signature, args.signer_policy)
            return 0
        if args.print is not None:
            if (
                args.request is None
                or args.candidate_commit is not None
                or args.output_root is not None
                or args.signer_policy is not None
                or args.signature is not None
            ):
                raise ReleaseRequestCreationError("print requires --request")
            request, _ = _task28_request_value(args.request)
            values = {
                "candidate_commit": request["candidate"]["commit"],
                "candidate_tree": request["candidate"]["tree"],
                "qualification_session_id": request["qualification_session_id"],
            }
            print(values[args.print])
            return 0
        if args.resume is not None:
            if (
                args.signature is None
                or args.signer_policy is None
                or args.candidate_commit is not None
                or args.output_root is not None
                or args.request is not None
            ):
                raise ReleaseRequestCreationError("resume arguments are incomplete")
            verify_task28_release_request(args.resume, args.signature, args.signer_policy)
            print(args.resume.resolve(strict=True))
            return 0
        raise ReleaseRequestCreationError("Task 28 request mode is missing")
    except (OSError, json.JSONDecodeError, KeyError, ReleaseRequestCreationError) as error:
        parser.error(str(error))
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
