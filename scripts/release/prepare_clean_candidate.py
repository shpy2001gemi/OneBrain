#!/usr/bin/env python3
"""Prepare an exact, detached, read-only Base qualification worktree."""

from __future__ import annotations

import argparse
import json
import os
import stat
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Mapping

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

import blake3

from scripts.release.verify_base_release_request import (
    VerifiedQualificationContextV1,
    VerifiedQualificationContextV2,
    verify_release_request,
    verify_task28_release_request,
)


class CleanCandidateError(RuntimeError):
    """The requested candidate cannot be isolated without source contamination."""


@dataclass(frozen=True)
class PreparedCandidate:
    worktree: Path
    commit: str
    tree: str
    object_format: str
    request_digest: str
    qualification_session_id: str
    environment: Mapping[str, str]
    tracked_blake3: Mapping[str, str]


@dataclass(frozen=True)
class FinalizedCandidate:
    format: str
    status: str
    commit: str
    tree: str
    release_request_digest: str
    qualification_session_id: str
    tracked_files_blake3: str


def _git(root: Path, *arguments: str, text: bool = True) -> subprocess.CompletedProcess:
    completed = subprocess.run(
        ["git", "-C", str(root), *arguments],
        capture_output=True,
        text=text,
        check=False,
    )
    if completed.returncode != 0:
        raise CleanCandidateError(f"Git {' '.join(arguments)} failed with {completed.returncode}")
    return completed


def _status(root: Path) -> str:
    return _git(
        root,
        "status",
        "--porcelain=v1",
        "--untracked-files=all",
        "--ignored=matching",
    ).stdout.strip()


def _source_status_with_only(root: Path, allowed_ignored: set[Path]) -> str:
    tracked_or_untracked = _git(
        root, "status", "--porcelain=v1", "--untracked-files=all"
    ).stdout.strip()
    if tracked_or_untracked:
        return tracked_or_untracked
    actual_raw = _git(
        root,
        "ls-files",
        "--others",
        "--ignored",
        "--exclude-standard",
        "-z",
        text=False,
    ).stdout
    actual = {
        (root / item.decode("utf-8")).resolve()
        for item in actual_raw.split(b"\0")
        if item
    }
    return "" if actual == allowed_ignored else "ignored artifact set differs"


def _tracked_paths(root: Path, revision: str | None = None) -> set[str]:
    arguments = (
        ("ls-tree", "-r", "--name-only", "-z", revision)
        if revision is not None
        else ("ls-files", "-z")
    )
    raw = _git(root, *arguments, text=False).stdout
    return {item.decode("utf-8") for item in raw.split(b"\0") if item}


def _filesystem_files(root: Path) -> set[str]:
    files: set[str] = set()
    for directory, names, filenames in os.walk(root):
        relative_directory = Path(directory).relative_to(root)
        if relative_directory == Path(".") and ".git" in names:
            names.remove(".git")
        for filename in filenames:
            relative = (relative_directory / filename).as_posix()
            if relative != ".git":
                files.add(relative)
    return files


def _outside(path: Path, source: Path) -> bool:
    try:
        return os.path.commonpath([str(path.resolve()), str(source.resolve())]) != str(source.resolve())
    except ValueError:
        return True


def _blake3_file(path: Path) -> str:
    digest = blake3.blake3()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def finalize_prepared_candidate(prepared: PreparedCandidate) -> FinalizedCandidate:
    """Perform the mandatory post-run integrity transition."""
    worktree = prepared.worktree.resolve(strict=True)
    if _git(worktree, "rev-parse", "HEAD").stdout.strip() != prepared.commit:
        raise CleanCandidateError("post-run candidate commit integrity failed")
    if _git(worktree, "rev-parse", "HEAD^{tree}").stdout.strip() != prepared.tree:
        raise CleanCandidateError("post-run candidate tree integrity failed")
    if _git(worktree, "rev-parse", "--show-object-format").stdout.strip() != prepared.object_format:
        raise CleanCandidateError("post-run candidate object format integrity failed")
    tree_files = _tracked_paths(worktree, prepared.commit)
    if tree_files != _tracked_paths(worktree) or tree_files != _filesystem_files(worktree):
        raise CleanCandidateError("post-run candidate filesystem integrity failed")
    if _status(worktree):
        raise CleanCandidateError("post-run candidate status is not pristine")
    diff = subprocess.run(
        ["git", "-C", str(worktree), "diff", "--no-ext-diff", "--quiet", "HEAD", "--"],
        capture_output=True,
        check=False,
    )
    if diff.returncode != 0:
        raise CleanCandidateError("post-run candidate tracked-byte integrity failed")
    measured = {relative: _blake3_file(worktree / relative) for relative in sorted(tree_files)}
    if measured != dict(prepared.tracked_blake3):
        raise CleanCandidateError("post-run candidate tracked hashes differ")
    for location in prepared.environment.values():
        output = Path(location).resolve(strict=True)
        if not _outside(output, worktree):
            raise CleanCandidateError("post-run qualification output entered candidate source")
    digest = blake3.blake3(
        json.dumps(measured, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()
    return FinalizedCandidate(
        format="onebrain/base-v1-candidate-finalization/1",
        status="post-run-integrity-verified",
        commit=prepared.commit,
        tree=prepared.tree,
        release_request_digest=prepared.request_digest,
        qualification_session_id=prepared.qualification_session_id,
        tracked_files_blake3=digest,
    )


def prepared_candidate_receipt(prepared: PreparedCandidate) -> dict[str, object]:
    return {
        "format": "onebrain/base-v1-prepared-candidate/1",
        "lifecycle_state": "qualification-running",
        "worktree": str(prepared.worktree.resolve(strict=True)),
        "commit": prepared.commit,
        "tree": prepared.tree,
        "object_format": prepared.object_format,
        "release_request_digest": prepared.request_digest,
        "qualification_session_id": prepared.qualification_session_id,
        "environment": dict(prepared.environment),
        "tracked_blake3": dict(prepared.tracked_blake3),
    }


def _load_prepared_candidate_receipt(
    receipt_path: Path,
) -> tuple[PreparedCandidate, str]:
    try:
        payload = receipt_path.read_bytes()
        receipt = json.loads(payload)
    except (OSError, json.JSONDecodeError) as error:
        raise CleanCandidateError("prepared-candidate receipt is unreadable") from error
    fields = {
        "format", "lifecycle_state", "worktree", "commit", "tree", "object_format",
        "release_request_digest", "qualification_session_id", "environment", "tracked_blake3",
    }
    if (
        not isinstance(receipt, dict)
        or set(receipt) != fields
        or receipt["format"] != "onebrain/base-v1-prepared-candidate/1"
        or receipt["lifecycle_state"] != "qualification-running"
        or payload != json.dumps(receipt, sort_keys=True, separators=(",", ":")).encode("utf-8")
        or not isinstance(receipt["environment"], dict)
        or not isinstance(receipt["tracked_blake3"], dict)
    ):
        raise CleanCandidateError("prepared-candidate receipt fields are not closed canonical bytes")
    worktree_text = str(receipt["worktree"])
    worktree = Path(worktree_text)
    if not worktree.is_absolute():
        raise CleanCandidateError("prepared-candidate worktree is not canonical absolute")
    try:
        canonical_worktree = worktree.resolve(strict=True)
    except OSError as error:
        raise CleanCandidateError("prepared-candidate worktree is unavailable") from error
    if worktree_text != str(canonical_worktree):
        raise CleanCandidateError("prepared-candidate worktree is not canonical absolute")
    prepared = PreparedCandidate(
        worktree=canonical_worktree,
        commit=str(receipt["commit"]),
        tree=str(receipt["tree"]),
        object_format=str(receipt["object_format"]),
        request_digest=str(receipt["release_request_digest"]),
        qualification_session_id=str(receipt["qualification_session_id"]),
        environment={str(key): str(value) for key, value in receipt["environment"].items()},
        tracked_blake3={str(key): str(value) for key, value in receipt["tracked_blake3"].items()},
    )
    return prepared, blake3.blake3(payload).hexdigest()


def finalize_prepared_candidate_receipt(
    receipt_path: Path,
    *,
    expected_worktree: Path | None = None,
    expected_receipt_blake3: str | None = None,
) -> FinalizedCandidate:
    prepared, measured_digest = _load_prepared_candidate_receipt(receipt_path)
    if expected_receipt_blake3 is not None and measured_digest != expected_receipt_blake3:
        raise CleanCandidateError("prepared-candidate receipt digest differs")
    if (
        expected_worktree is not None
        and prepared.worktree != expected_worktree.resolve(strict=True)
    ):
        raise CleanCandidateError("prepared-candidate receipt worktree differs from candidate root")
    return finalize_prepared_candidate(prepared)


def prepare_clean_candidate(
    *,
    source_root: Path,
    external_root: Path,
    request_path: Path,
    signature_path: Path,
    policy_path: Path,
    gpg_home: Path,
    verify_request: Callable[..., object] = verify_release_request,
    after_checkout: Callable[[Path], object] | None = None,
    allowed_source_artifacts: tuple[Path, ...] = (),
) -> PreparedCandidate:
    source = source_root.resolve(strict=True)
    external = external_root.resolve(strict=True)
    if not _outside(external, source):
        raise CleanCandidateError("qualification output root must be outside the source worktree")
    try:
        verified = verify_request(request_path, signature_path, policy_path, gpg_home)
    except Exception as error:
        raise CleanCandidateError("signed release request verification failed") from error
    if not isinstance(
        verified, (VerifiedQualificationContextV1, VerifiedQualificationContextV2)
    ) or not verified.production:
        raise CleanCandidateError("production verified release context is required")
    allowed = {path.resolve(strict=True) for path in allowed_source_artifacts}
    if any(not path.is_file() or path.is_symlink() for path in allowed):
        raise CleanCandidateError("allowed request artifact is not a regular file")
    allowed_inside_source: set[Path] = set()
    for path in allowed:
        try:
            path.relative_to(source)
        except ValueError:
            continue
        allowed_inside_source.add(path)
    if allowed_inside_source:
        source_status = _source_status_with_only(source, allowed_inside_source)
    else:
        source_status = _status(source)
    if source_status:
        raise CleanCandidateError("source worktree is dirty, untracked, ignored or generated")
    commit = str(verified.run_context.get("candidate_commit", ""))
    tree = str(verified.run_context.get("candidate_tree", ""))
    measured_commit = _git(source, "rev-parse", commit).stdout.strip()
    measured_tree = _git(source, "rev-parse", f"{commit}^{{tree}}").stdout.strip()
    object_format = _git(source, "rev-parse", "--show-object-format").stdout.strip()
    if measured_commit != commit or measured_tree != tree:
        raise CleanCandidateError("signed candidate commit or tree is not exact")

    qualification_root = Path(
        tempfile.mkdtemp(prefix="onebrain-base-v1-candidate-", dir=external)
    ).resolve()
    worktree = qualification_root / "candidate"
    completed = subprocess.run(
        ["git", "-C", str(source), "worktree", "add", "--detach", str(worktree), commit],
        capture_output=True,
        text=True,
        check=False,
    )
    if completed.returncode != 0:
        raise CleanCandidateError(f"detached worktree creation failed with {completed.returncode}")
    if after_checkout is not None:
        after_checkout(worktree)

    if _git(worktree, "rev-parse", "HEAD").stdout.strip() != commit:
        raise CleanCandidateError("detached worktree HEAD differs from signed commit")
    if _git(worktree, "rev-parse", "HEAD^{tree}").stdout.strip() != tree:
        raise CleanCandidateError("detached worktree tree differs from signed tree")
    tree_files = _tracked_paths(worktree, commit)
    index_files = _tracked_paths(worktree)
    filesystem_files = _filesystem_files(worktree)
    if tree_files != index_files or tree_files != filesystem_files:
        raise CleanCandidateError("candidate filesystem differs from Git tree/files")
    if _status(worktree):
        raise CleanCandidateError("candidate contains dirty, untracked, ignored or generated files")
    diff = subprocess.run(
        ["git", "-C", str(worktree), "diff", "--no-ext-diff", "--quiet", "HEAD", "--"],
        capture_output=True,
        check=False,
    )
    if diff.returncode != 0:
        raise CleanCandidateError("candidate tracked bytes differ from Git HEAD")

    outputs = qualification_root / "outputs"
    environment = {
        "CARGO_TARGET_DIR": str(outputs / "cargo-target"),
        "CARGO_HOME": str(outputs / "cargo-home"),
        "PYTHONPYCACHEPREFIX": str(outputs / "python-cache"),
        "TMPDIR": str(outputs / "tmp"),
        "TEMP": str(outputs / "tmp"),
        "TMP": str(outputs / "tmp"),
        "BASE_V1_EVIDENCE_ROOT": str(outputs / "evidence"),
    }
    for value in set(environment.values()):
        Path(value).mkdir(parents=True, exist_ok=True)
    for relative in sorted(tree_files):
        path = worktree / Path(relative)
        mode = path.stat(follow_symlinks=False).st_mode
        if stat.S_ISREG(mode):
            path.chmod(mode & ~(stat.S_IWUSR | stat.S_IWGRP | stat.S_IWOTH))
    if _status(worktree):
        raise CleanCandidateError("read-only fencing altered candidate source bytes")
    return PreparedCandidate(
        worktree=worktree,
        commit=commit,
        tree=tree,
        object_format=object_format,
        request_digest=verified.request_digest,
        qualification_session_id=str(verified.run_context["qualification_session_id"]),
        environment=environment,
        tracked_blake3={relative: _blake3_file(worktree / relative) for relative in sorted(tree_files)},
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path)
    parser.add_argument("--release-request", type=Path)
    parser.add_argument("--signature", type=Path)
    parser.add_argument("--signer-policy", type=Path)
    parser.add_argument("--read-only", action="store_true")
    parser.add_argument("--verify-only", action="store_true")
    parser.add_argument("--candidate-root", type=Path)
    args = parser.parse_args()
    try:
        if args.release_request is None or args.signature is None or args.signer_policy is None:
            raise CleanCandidateError("release request verification arguments are incomplete")
        context = verify_task28_release_request(
            args.release_request,
            args.signature,
            args.signer_policy,
            candidate_root=args.source_root if not args.verify_only else None,
        )
        request_digest = context.request_digest
        if args.verify_only:
            if args.candidate_root is None or args.source_root is not None or args.read_only:
                raise CleanCandidateError("verify-only arguments are not closed")
            receipt_path = args.candidate_root.resolve(strict=True).parent / "prepared-candidate.json"
            finalized = finalize_prepared_candidate_receipt(receipt_path)
            if finalized.release_request_digest != request_digest:
                raise CleanCandidateError("finalized candidate release request differs")
            return 0
        if args.source_root is None or not args.read_only or args.candidate_root is not None:
            raise CleanCandidateError("preparation arguments are not closed")
        external_root = Path(tempfile.gettempdir()).resolve(strict=True)
        prepared = prepare_clean_candidate(
            source_root=args.source_root,
            external_root=external_root,
            request_path=args.release_request,
            signature_path=args.signature,
            policy_path=args.signer_policy,
            gpg_home=external_root,
            verify_request=lambda *_args: context,
            allowed_source_artifacts=(args.release_request, args.signature),
        )
        receipt = prepared_candidate_receipt(prepared)
        receipt_path = prepared.worktree.parent / "prepared-candidate.json"
        with receipt_path.open("x", encoding="utf-8", newline="") as handle:
            json.dump(receipt, handle, sort_keys=True, separators=(",", ":"))
            handle.flush()
            os.fsync(handle.fileno())
        print(prepared.worktree)
        return 0
    except (CleanCandidateError, OSError) as error:
        parser.error(str(error))
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
