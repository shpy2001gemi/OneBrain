#!/usr/bin/env python3
"""Publish a verified Base v1 manifest and tag with one final ref CAS."""

from __future__ import annotations

import argparse
import base64
import json
import os
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Callable

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

import blake3

from scripts.base.qualify_base import (
    BaseQualificationError,
    QualificationInputs,
    canonical_json as qualifier_canonical_json,
    qualify_base,
    read_evidence_root,
    read_qualification_bundle,
    verify_manifest_ready,
)
from scripts.release.create_base_release_request import (
    ReleaseRequestCreationError,
    verify_task28_release_request,
)
from scripts.release.verify_base_release_request import canonical_json, verify_release_request
from scripts.release.prepare_clean_candidate import (
    CleanCandidateError,
    finalize_prepared_candidate_receipt,
)


TAG_NAME = "base-v1.0.0"
TAG_REF = f"refs/tags/{TAG_NAME}"
MANIFEST_USAGE = "base-evidence-manifest"
TAG_USAGE = "base-release-tag"
POLICY_CONTEXT = "onebrain:base-v1:release-signer-policy:1"
RELEASE_SIGNER_FINGERPRINT = "F9DDAFB46FB6603E14B21B4DB0D9DBF23DBE8ED2"
RELEASE_SIGNER_POLICY_DIGEST = (
    "443534ac4f583368cc5e07b1c4dbddf1ac66c63eba32bcf9e565b07f07a80d88"
)
RELEASE_SIGNER_PUBLIC_PACKET_BLAKE3 = (
    "d28acd703d6bb7addad30de1e9cd0c05d1ca84dfd1e41ad5963458913382d0db"
)
FailureHook = Callable[[str], object]
Signer = Callable[[bytes, str, str], bytes]
Verifier = Callable[[bytes, bytes, str, str], bool | datetime]


class BaseReleasePublicationError(RuntimeError):
    """The release cannot be published without weakening an immutable gate."""


@dataclass(frozen=True)
class PublishedBaseRelease:
    status: str
    manifest_digest: str
    envelope_digest: str
    tag_object: str
    ready_pointer: Path
    receipt_path: Path


def _instant(value: object, label: str) -> datetime:
    if not isinstance(value, str) or not value.endswith("Z"):
        raise BaseReleasePublicationError(f"{label} is not a UTC instant")
    try:
        parsed = datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError as error:
        raise BaseReleasePublicationError(f"{label} is invalid") from error
    if parsed.microsecond:
        raise BaseReleasePublicationError(f"{label} is not whole-second precision")
    return parsed


def _signature_verified(
    verify: Verifier,
    payload: bytes,
    signature: bytes,
    usage: str,
    fingerprint: str,
    interval: tuple[datetime, datetime] | None,
) -> bool:
    result = verify(payload, signature, usage, fingerprint)
    if result is False:
        return False
    if interval is None:
        return result is True or isinstance(result, datetime)
    if not isinstance(result, datetime):
        return False
    created = result.astimezone(timezone.utc)
    return interval[0] <= created < interval[1]


def _canonical_path(path: Path, label: str) -> tuple[dict[str, object], bytes]:
    try:
        payload = path.read_bytes()
        value = json.loads(payload)
    except (OSError, json.JSONDecodeError) as error:
        raise BaseReleasePublicationError(f"{label} is unreadable or invalid JSON") from error
    if not isinstance(value, dict) or payload != canonical_json(value):
        raise BaseReleasePublicationError(f"{label} bytes are not canonical JSON")
    return value, payload


def _git(repository: Path, *arguments: str, stdin: bytes | None = None, check: bool = True):
    completed = subprocess.run(
        ["git", "-C", str(repository), *arguments],
        input=stdin,
        capture_output=True,
        check=False,
    )
    if check and completed.returncode != 0:
        raise BaseReleasePublicationError(f"Git {' '.join(arguments)} failed")
    return completed


def _policy(profile_path: Path, fingerprint: str) -> tuple[dict[str, object], str]:
    if fingerprint != RELEASE_SIGNER_FINGERPRINT:
        raise BaseReleasePublicationError("base-release fingerprint is not allowlisted")
    try:
        profile = json.loads(profile_path.read_bytes())
        if profile.get("format") != "onebrain/base-v1-release-signers/1":
            raise ValueError("format")
        rows = [row for row in profile["policies"] if row["policy"].get("role") == "base-release"]
        if len(rows) != 1:
            raise ValueError("role")
        row = rows[0]
        policy = row["policy"]
        if set(policy["allowed_usages"]) != {MANIFEST_USAGE, TAG_USAGE}:
            raise ValueError("usages")
        signers = policy["signers"]
        if len(signers) != 1 or signers[0]["fingerprint"] != fingerprint:
            raise ValueError("fingerprint")
        if signers[0].get("public_key_packet_blake3") != RELEASE_SIGNER_PUBLIC_PACKET_BLAKE3:
            raise ValueError("public packet")
        digest = blake3.blake3(
            canonical_json(policy), derive_key_context=POLICY_CONTEXT
        ).hexdigest()
        if row["digest"] != {
            "algorithm": "BLAKE3 derive-key",
            "context": POLICY_CONTEXT,
            "expected_hex": digest,
        }:
            raise ValueError("digest")
        if digest != RELEASE_SIGNER_POLICY_DIGEST:
            raise ValueError("frozen policy digest")
        return policy, digest
    except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
        raise BaseReleasePublicationError("base-release signer policy or role is invalid") from error


def _create_or_exact(path: Path, payload: bytes) -> None:
    _ensure_directory(path.parent)
    temporary: Path | None = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="wb", prefix=f".{path.name}.", suffix=".tmp", dir=path.parent, delete=False
        ) as handle:
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
            temporary = Path(handle.name)
        try:
            os.link(temporary, path)
        except FileExistsError:
            if path.read_bytes() != payload:
                raise BaseReleasePublicationError(f"immutable publication collision at {path.name}")
        _fsync_directory(path.parent)
    except OSError as error:
        raise BaseReleasePublicationError("atomic immutable publication failed") from error
    finally:
        if temporary is not None:
            try:
                temporary.unlink(missing_ok=True)
            except OSError:
                pass


def _flush_windows_directory(path: Path) -> None:
    """Flush a directory handle on Windows or fail closed.

    Python cannot open directory descriptors with ``os.open`` on Windows.  A
    handle created with ``FILE_FLAG_BACKUP_SEMANTICS`` is the corresponding
    durability primitive; publication must not silently downgrade it to a
    no-op.
    """
    import ctypes
    from ctypes import wintypes

    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    create_file = kernel32.CreateFileW
    create_file.argtypes = (
        wintypes.LPCWSTR,
        wintypes.DWORD,
        wintypes.DWORD,
        wintypes.LPVOID,
        wintypes.DWORD,
        wintypes.DWORD,
        wintypes.HANDLE,
    )
    create_file.restype = wintypes.HANDLE
    flush_file_buffers = kernel32.FlushFileBuffers
    flush_file_buffers.argtypes = (wintypes.HANDLE,)
    flush_file_buffers.restype = wintypes.BOOL
    close_handle = kernel32.CloseHandle
    close_handle.argtypes = (wintypes.HANDLE,)
    close_handle.restype = wintypes.BOOL

    handle = create_file(
        str(path),
        0x80000000 | 0x40000000,  # GENERIC_READ | GENERIC_WRITE
        0x00000001 | 0x00000002 | 0x00000004,
        None,
        3,  # OPEN_EXISTING
        0x02000000,  # FILE_FLAG_BACKUP_SEMANTICS
        None,
    )
    invalid_handle = ctypes.c_void_p(-1).value
    if handle == invalid_handle:
        raise OSError(ctypes.get_last_error(), "opening directory for durable flush failed")
    flush_error: OSError | None = None
    try:
        if not flush_file_buffers(handle):
            flush_error = OSError(
                ctypes.get_last_error(), "flushing Windows directory failed"
            )
    finally:
        if not close_handle(handle) and flush_error is None:
            flush_error = OSError(
                ctypes.get_last_error(), "closing flushed Windows directory failed"
            )
    if flush_error is not None:
        raise flush_error


def _fsync_directory(
    path: Path,
    *,
    platform_name: str | None = None,
    windows_flusher: Callable[[Path], None] | None = None,
) -> None:
    active_platform = os.name if platform_name is None else platform_name
    if active_platform == "nt":
        (windows_flusher or _flush_windows_directory)(path)
        return
    descriptor = os.open(path, os.O_RDONLY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def _ensure_directory(path: Path) -> None:
    existed = path.is_dir()
    path.mkdir(parents=True, exist_ok=True)
    if not existed:
        _fsync_directory(path.parent)


def _hook(callback: FailureHook | None, point: str) -> None:
    if callback is None:
        return
    try:
        callback(point)
    except Exception as error:
        raise BaseReleasePublicationError(f"injected failure at {point}") from error


def _tag_unsigned(
    commit: str,
    tree: str,
    session: str,
    commit_epoch: int,
    request_digest: str,
    manifest_digest: str,
    envelope_digest: str,
) -> bytes:
    message = (
        f"object {commit}\n"
        "type commit\n"
        f"tag {TAG_NAME}\n"
        f"tagger OneBrain Base Release <release@onebrain.invalid> {commit_epoch} +0000\n\n"
        "OneBrain Base v1.0.0\n\n"
        f"Release-request-BLAKE3: {request_digest}\n"
        f"Qualification-session: {session}\n"
        f"Candidate-tree: {tree}\n"
        f"Evidence-manifest-BLAKE3: {manifest_digest}\n"
        f"Release-envelope-BLAKE3: {envelope_digest}\n"
    )
    return message.encode("utf-8")


def _existing_ref(repository: Path) -> str | None:
    completed = _git(repository, "rev-parse", "--verify", TAG_REF, check=False)
    return completed.stdout.decode().strip() if completed.returncode == 0 else None


def _verify_ready(
    *,
    repository: Path,
    pointer_path: Path,
    current_ref: str | None,
    manifest_digest: str,
    request_digest: str,
    commit: str,
    tree: str,
    session: str,
    fingerprint: str,
    policy_digest: str,
    verify: Verifier,
    qualification_tier: str,
    signature_interval: tuple[datetime, datetime] | None,
) -> tuple[dict[str, object], bytes, bytes]:
    pointer, _ = _canonical_path(pointer_path, "release-ready pointer")
    if set(pointer) != {"format", "release_ready_digest", "release_ready"} or pointer["format"] != "onebrain/base-v1-release-ready-pointer/1":
        raise BaseReleasePublicationError("existing release-ready pointer is foreign")
    ready = pointer["release_ready"]
    if not isinstance(ready, dict) or pointer["release_ready_digest"] != blake3.blake3(canonical_json(ready)).hexdigest():
        raise BaseReleasePublicationError("release-ready pointer checksum is stale")
    if set(ready) != {
        "format",
        "manifest_digest",
        "release_request_digest",
        "envelope_digest",
        "target_commit",
        "target_tree",
        "tag_name",
        "tag_object",
        "tag_unsigned_base64",
        "tag_signature_base64",
        "signer_fingerprint",
        "trust_policy_digest",
        "qualification_tier",
    } or ready.get("format") != "onebrain/base-v1-release-ready/1" or ready.get(
        "tag_name"
    ) != TAG_NAME:
        raise BaseReleasePublicationError("release-ready fields are not closed")
    expected = {
        "manifest_digest": manifest_digest,
        "release_request_digest": request_digest,
        "target_commit": commit,
        "signer_fingerprint": fingerprint,
        "trust_policy_digest": policy_digest,
        "qualification_tier": qualification_tier,
    }
    for field, value in expected.items():
        if ready.get(field) != value:
            raise BaseReleasePublicationError("existing release-ready pointer has foreign bindings")
    tag_object = ready.get("tag_object")
    if not isinstance(tag_object, str) or (current_ref is not None and current_ref != tag_object):
        raise BaseReleasePublicationError("existing tag ref is foreign")
    try:
        unsigned = base64.b64decode(ready["tag_unsigned_base64"], validate=True)
        signature = base64.b64decode(ready["tag_signature_base64"], validate=True)
    except (KeyError, TypeError, ValueError) as error:
        raise BaseReleasePublicationError("release-ready tag envelope is invalid") from error
    commit_epoch_raw = _git(repository, "show", "-s", "--format=%ct", commit).stdout.decode().strip()
    try:
        expected_unsigned = _tag_unsigned(
            commit,
            tree,
            session,
            int(commit_epoch_raw),
            request_digest,
            manifest_digest,
            str(ready["envelope_digest"]),
        )
    except ValueError as error:
        raise BaseReleasePublicationError("candidate commit timestamp is invalid") from error
    if unsigned != expected_unsigned:
        raise BaseReleasePublicationError("stored tag unsigned bytes are stale or foreign")
    tag_bytes = unsigned + signature
    measured_object = _git(
        repository, "hash-object", "-t", "tag", "--stdin", stdin=tag_bytes
    ).stdout.decode().strip()
    if measured_object != tag_object:
        raise BaseReleasePublicationError("release-ready tag object ID is stale or foreign")
    object_exists = _git(
        repository, "cat-file", "-e", f"{tag_object}^{{tag}}", check=False
    ).returncode == 0
    if current_ref is not None and not object_exists:
        raise BaseReleasePublicationError("existing tag ref does not resolve to its tag object")
    if object_exists and _git(repository, "cat-file", "tag", tag_object).stdout != tag_bytes:
        raise BaseReleasePublicationError("stored tag object bytes are stale or foreign")
    if not _signature_verified(
        verify, unsigned, signature, TAG_USAGE, fingerprint, signature_interval
    ):
        raise BaseReleasePublicationError("stored tag object or signature is not verified")
    return ready, unsigned, signature


def _verify_manifest_envelope(
    *,
    output_root: Path,
    ready: dict[str, object],
    manifest_bytes: bytes,
    manifest_digest: str,
    request_digest: str,
    fingerprint: str,
    policy_digest: str,
    verify: Verifier,
    qualification_tier: str,
    signature_interval: tuple[datetime, datetime] | None,
    release_envelope_root: Path | None = None,
) -> None:
    envelope_digest = ready.get("envelope_digest")
    if not isinstance(envelope_digest, str):
        raise BaseReleasePublicationError("release-ready envelope digest is invalid")
    if release_envelope_root is not None:
        root = release_envelope_root.resolve() / manifest_digest / envelope_digest
        try:
            names = {item.name for item in root.resolve(strict=True).iterdir()}
            stored_signature = (root / "manifest.json.asc").read_bytes()
        except OSError as error:
            raise BaseReleasePublicationError("release detached signature is missing") from error
        if names != {"manifest.json.asc"} or blake3.blake3(stored_signature).hexdigest() != envelope_digest:
            raise BaseReleasePublicationError("release detached-signature generation differs")
        if not _signature_verified(
            verify, manifest_bytes, stored_signature, MANIFEST_USAGE, fingerprint, signature_interval
        ):
            raise BaseReleasePublicationError("stored manifest detached signature is invalid")
        return
    root = output_root.resolve() / "envelopes" / envelope_digest
    envelope, envelope_bytes = _canonical_path(root / "envelope.json", "release envelope")
    if {item.name for item in root.iterdir()} != {"envelope.json", "manifest.json.asc"}:
        raise BaseReleasePublicationError("release envelope generation file set differs")
    if set(envelope) != {
        "format",
        "usage",
        "manifest_digest",
        "release_request_digest",
        "signer_fingerprint",
        "trust_policy_digest",
        "manifest_signature_base64",
        "qualification_tier",
    } or envelope != {
        "format": "onebrain/base-v1-release-envelope/1",
        "usage": MANIFEST_USAGE,
        "manifest_digest": manifest_digest,
        "release_request_digest": request_digest,
        "signer_fingerprint": fingerprint,
        "trust_policy_digest": policy_digest,
        "qualification_tier": qualification_tier,
        "manifest_signature_base64": envelope.get("manifest_signature_base64"),
    }:
        raise BaseReleasePublicationError("release envelope bindings are foreign")
    if blake3.blake3(envelope_bytes).hexdigest() != envelope_digest:
        raise BaseReleasePublicationError("release envelope content address is stale")
    try:
        signature = base64.b64decode(envelope["manifest_signature_base64"], validate=True)
    except (TypeError, ValueError) as error:
        raise BaseReleasePublicationError("manifest envelope signature is invalid") from error
    try:
        stored_signature = (root / "manifest.json.asc").read_bytes()
    except OSError as error:
        raise BaseReleasePublicationError("manifest detached signature is missing") from error
    if stored_signature != signature or not _signature_verified(
        verify,
        manifest_bytes,
        signature,
        MANIFEST_USAGE,
        fingerprint,
        signature_interval,
    ):
        raise BaseReleasePublicationError("stored manifest detached signature is invalid")


def _publication_receipt(
    *,
    tag_object: str,
    commit: str,
    manifest_digest: str,
    request_digest: str,
    envelope_digest: str,
    pointer_path: Path,
    fingerprint: str,
    policy_digest: str,
) -> dict[str, object]:
    return {
        "format": "onebrain/base-v1-release-publication-receipt/1",
        "status": "Published",
        "tag_ref": TAG_REF,
        "tag_object": tag_object,
        "target_commit": commit,
        "manifest_digest": manifest_digest,
        "release_request_digest": request_digest,
        "envelope_digest": envelope_digest,
        "ready_pointer_blake3": blake3.blake3(pointer_path.read_bytes()).hexdigest(),
        "signer_fingerprint": fingerprint,
        "trust_policy_digest": policy_digest,
    }


def _publish_verified_base_release(
    *,
    repository: Path,
    manifest_path: Path,
    request_path: Path,
    output_root: Path,
    signer_profile_path: Path,
    signer_fingerprint: str,
    sign: Signer,
    verify: Verifier,
    expected_manifest_digest: str,
    verified_request_digest: str,
    qualification_tier: str,
    require_signature_time: bool,
    failure_hook: FailureHook | None = None,
    release_envelope_root: Path | None = None,
    release_ready_output: Path | None = None,
) -> PublishedBaseRelease:
    repo = repository.resolve(strict=True)
    manifest, manifest_bytes = _canonical_path(manifest_path, "Base evidence manifest")
    request, request_bytes = _canonical_path(request_path, "Base release request")
    manifest_digest = blake3.blake3(manifest_bytes).hexdigest()
    request_digest = blake3.blake3(request_bytes).hexdigest()
    if manifest_digest != expected_manifest_digest:
        raise BaseReleasePublicationError("expected manifest digest is stale")
    if request_digest != verified_request_digest:
        raise BaseReleasePublicationError("verified release request digest is stale")
    if (
        manifest.get("format") != "onebrain/base-v1-evidence-manifest/1"
        or manifest.get("qualified") is not True
        or manifest.get("qualification_tier") != qualification_tier
    ):
        raise BaseReleasePublicationError("manifest is not a derived qualified Base v1 manifest")
    candidate = manifest.get("candidate")
    request_candidate = request.get("candidate")
    if (
        not isinstance(candidate, dict)
        or not isinstance(request_candidate, dict)
        or manifest.get("release_request_digest") != request_digest
        or manifest.get("qualification_session_id") != request.get("qualification_session_id")
        or candidate.get("commit") != request_candidate.get("commit")
        or candidate.get("tree") != request_candidate.get("tree")
        or candidate.get("object_format") != request_candidate.get("object_format")
    ):
        raise BaseReleasePublicationError("manifest candidate/request bindings are mixed")
    commit = str(candidate["commit"])
    tree = str(candidate["tree"])
    if _git(repo, "rev-parse", commit).stdout.decode().strip() != commit:
        raise BaseReleasePublicationError("candidate commit is not exact in repository")
    if _git(repo, "rev-parse", f"{commit}^{{tree}}").stdout.decode().strip() != tree:
        raise BaseReleasePublicationError("candidate tree differs from repository")
    policy, policy_digest = _policy(signer_profile_path, signer_fingerprint)
    if policy.get("role") != "base-release":
        raise BaseReleasePublicationError("wrong signer role")
    signature_interval: tuple[datetime, datetime] | None = None
    if require_signature_time:
        signer = policy["signers"][0]
        request_created = _instant(request.get("created_utc"), "release request created_utc")
        request_expires = _instant(request.get("expires_utc"), "release request expires_utc")
        signer_created = _instant(signer.get("created_utc"), "release signer created_utc")
        signer_expires = _instant(signer.get("expires_utc"), "release signer expires_utc")
        signature_interval = (max(request_created, signer_created), min(request_expires, signer_expires))
        if signature_interval[0] >= signature_interval[1]:
            raise BaseReleasePublicationError("request and release signer validity do not overlap")

    pointer_path = (
        release_ready_output.resolve()
        if release_ready_output is not None
        else output_root.resolve() / "ready" / f"{TAG_NAME}.json"
    )
    current_ref = _existing_ref(repo)
    if current_ref is not None:
        if not pointer_path.is_file():
            raise BaseReleasePublicationError("foreign existing tag ref has no verified ready pointer")
        ready, unsigned_tag, tag_signature = _verify_ready(
            repository=repo,
            pointer_path=pointer_path,
            current_ref=current_ref,
            manifest_digest=manifest_digest,
            request_digest=request_digest,
            commit=commit,
            tree=tree,
            session=str(manifest["qualification_session_id"]),
            fingerprint=signer_fingerprint,
            policy_digest=policy_digest,
            verify=verify,
            qualification_tier=qualification_tier,
            signature_interval=signature_interval,
        )
        _verify_manifest_envelope(
            output_root=output_root,
            ready=ready,
            manifest_bytes=manifest_bytes,
            manifest_digest=manifest_digest,
            request_digest=request_digest,
            fingerprint=signer_fingerprint,
            policy_digest=policy_digest,
            verify=verify,
            qualification_tier=qualification_tier,
            signature_interval=signature_interval,
            release_envelope_root=release_envelope_root,
        )
        receipt_path = output_root.resolve() / "receipts" / f"{ready['tag_object']}.json"
        receipt = _publication_receipt(
            tag_object=str(ready["tag_object"]),
            commit=commit,
            manifest_digest=manifest_digest,
            request_digest=request_digest,
            envelope_digest=str(ready["envelope_digest"]),
            pointer_path=pointer_path,
            fingerprint=signer_fingerprint,
            policy_digest=policy_digest,
        )
        _create_or_exact(receipt_path, canonical_json(receipt))
        return PublishedBaseRelease(
            "AlreadyPublished", manifest_digest, str(ready["envelope_digest"]),
            str(ready["tag_object"]), pointer_path, receipt_path,
        )

    # A retry after an object/pointer crash reuses the already verified bytes.
    if pointer_path.is_file():
        ready, unsigned_tag, tag_signature = _verify_ready(
            repository=repo,
            pointer_path=pointer_path,
            current_ref=None,
            manifest_digest=manifest_digest,
            request_digest=request_digest,
            commit=commit,
            tree=tree,
            session=str(manifest["qualification_session_id"]),
            fingerprint=signer_fingerprint,
            policy_digest=policy_digest,
            verify=verify,
            qualification_tier=qualification_tier,
            signature_interval=signature_interval,
        )
        _verify_manifest_envelope(
            output_root=output_root,
            ready=ready,
            manifest_bytes=manifest_bytes,
            manifest_digest=manifest_digest,
            request_digest=request_digest,
            fingerprint=signer_fingerprint,
            policy_digest=policy_digest,
            verify=verify,
            qualification_tier=qualification_tier,
            signature_interval=signature_interval,
            release_envelope_root=release_envelope_root,
        )
        tag_object = str(ready["tag_object"])
        envelope_digest = str(ready["envelope_digest"])
    else:
        try:
            manifest_signature = sign(manifest_bytes, MANIFEST_USAGE, signer_fingerprint)
        except Exception as error:
            raise BaseReleasePublicationError("manifest signing failed") from error
        if not manifest_signature or not _signature_verified(
            verify,
            manifest_bytes,
            manifest_signature,
            MANIFEST_USAGE,
            signer_fingerprint,
            signature_interval,
        ):
            raise BaseReleasePublicationError("manifest signature is not from the allowlisted role")
        envelope = {
            "format": "onebrain/base-v1-release-envelope/1",
            "usage": MANIFEST_USAGE,
            "manifest_digest": manifest_digest,
            "release_request_digest": request_digest,
            "signer_fingerprint": signer_fingerprint,
            "trust_policy_digest": policy_digest,
            "qualification_tier": qualification_tier,
            "manifest_signature_base64": base64.b64encode(manifest_signature).decode("ascii"),
        }
        envelope_bytes = canonical_json(envelope)
        envelope_digest = (
            blake3.blake3(manifest_signature).hexdigest()
            if release_envelope_root is not None
            else blake3.blake3(envelope_bytes).hexdigest()
        )
        _hook(failure_hook, "before-envelope-readiness")
        if release_envelope_root is not None:
            durable_envelope_root = release_envelope_root.resolve()
            _ensure_directory(durable_envelope_root)
            manifest_envelope_root = durable_envelope_root / manifest_digest
            _ensure_directory(manifest_envelope_root)
            envelope_root = manifest_envelope_root / envelope_digest
            _ensure_directory(envelope_root)
        else:
            envelope_root = output_root.resolve() / "envelopes" / envelope_digest
            _ensure_directory(envelope_root)
        expected_envelope_files = (
            {"manifest.json.asc"}
            if release_envelope_root is not None
            else {"envelope.json", "manifest.json.asc"}
        )
        existing_envelope_files = {item.name for item in envelope_root.iterdir()}
        if existing_envelope_files and existing_envelope_files != expected_envelope_files:
            raise BaseReleasePublicationError(
                "release envelope generation contains missing or extra files"
            )
        if release_envelope_root is None:
            _create_or_exact(envelope_root / "envelope.json", envelope_bytes)
        _create_or_exact(envelope_root / "manifest.json.asc", manifest_signature)
        if {item.name for item in envelope_root.iterdir()} != expected_envelope_files:
            raise BaseReleasePublicationError(
                "release envelope generation contains missing or extra files"
            )
        _fsync_directory(envelope_root)

        commit_epoch_raw = _git(repo, "show", "-s", "--format=%ct", commit).stdout.decode().strip()
        try:
            commit_epoch = int(commit_epoch_raw)
        except ValueError as error:
            raise BaseReleasePublicationError("candidate commit timestamp is invalid") from error
        unsigned_tag = _tag_unsigned(
            commit,
            tree,
            str(manifest["qualification_session_id"]),
            commit_epoch,
            request_digest,
            manifest_digest,
            envelope_digest,
        )
        try:
            tag_signature = sign(unsigned_tag, TAG_USAGE, signer_fingerprint)
        except Exception as error:
            raise BaseReleasePublicationError("tag signing failed") from error
        if not tag_signature or not _signature_verified(
            verify, unsigned_tag, tag_signature, TAG_USAGE, signer_fingerprint, signature_interval
        ):
            raise BaseReleasePublicationError("tag signature is not from the allowlisted role")
        tag_bytes = unsigned_tag + tag_signature
        tag_object = _git(
            repo, "hash-object", "-t", "tag", "--stdin", stdin=tag_bytes
        ).stdout.decode().strip()
        ready = {
            "format": "onebrain/base-v1-release-ready/1",
            "manifest_digest": manifest_digest,
            "release_request_digest": request_digest,
            "envelope_digest": envelope_digest,
            "target_commit": commit,
            "target_tree": tree,
            "tag_name": TAG_NAME,
            "tag_object": tag_object,
            "tag_unsigned_base64": base64.b64encode(unsigned_tag).decode("ascii"),
            "tag_signature_base64": base64.b64encode(tag_signature).decode("ascii"),
            "signer_fingerprint": signer_fingerprint,
            "trust_policy_digest": policy_digest,
            "qualification_tier": qualification_tier,
        }
        pointer = {
            "format": "onebrain/base-v1-release-ready-pointer/1",
            "release_ready_digest": blake3.blake3(canonical_json(ready)).hexdigest(),
            "release_ready": ready,
        }
        _create_or_exact(pointer_path, canonical_json(pointer))
        _hook(failure_hook, "after-ready-before-object")

    tag_bytes = unsigned_tag + tag_signature
    _hook(failure_hook, "before-object-write")
    written_tag_object = _git(
        repo, "hash-object", "-t", "tag", "-w", "--stdin", stdin=tag_bytes
    ).stdout.decode().strip()
    if written_tag_object != tag_object or _git(repo, "cat-file", "tag", tag_object).stdout != tag_bytes:
        raise BaseReleasePublicationError("written tag object bytes are not exact")
    if not _signature_verified(
        verify, unsigned_tag, tag_signature, TAG_USAGE, signer_fingerprint, signature_interval
    ):
        raise BaseReleasePublicationError("written tag object signature cannot be reverified")

    _hook(failure_hook, "before-cas")
    zero = "0" * len(commit)
    cas = _git(repo, "update-ref", TAG_REF, tag_object, zero, check=False)
    if cas.returncode != 0:
        raced = _existing_ref(repo)
        if raced != tag_object:
            raise BaseReleasePublicationError("final tag compare-and-swap failed or found a foreign ref")
    receipt = _publication_receipt(
        tag_object=tag_object,
        commit=commit,
        manifest_digest=manifest_digest,
        request_digest=request_digest,
        envelope_digest=envelope_digest,
        pointer_path=pointer_path,
        fingerprint=signer_fingerprint,
        policy_digest=policy_digest,
    )
    receipt_path = output_root.resolve() / "receipts" / f"{tag_object}.json"
    _create_or_exact(receipt_path, canonical_json(receipt))
    _hook(failure_hook, "after-cas-receipt-fsync")
    return PublishedBaseRelease(
        "Published", manifest_digest, envelope_digest, tag_object, pointer_path, receipt_path
    )


def publish_verified_base_release(
    *,
    qualification_inputs: QualificationInputs,
    verified_request_digest: str,
    prepared_candidate_receipt: Path | None = None,
    prepared_candidate_receipt_digest: str | None = None,
    candidate_root: Path | None = None,
    **publication: object,
) -> PublishedBaseRelease:
    """Production entrypoint: re-derive and byte-compare before any signing."""
    if not isinstance(qualification_inputs, QualificationInputs):
        raise BaseReleasePublicationError("production publication requires qualification inputs")
    manifest_path = publication.get("manifest_path")
    if not isinstance(manifest_path, Path):
        raise BaseReleasePublicationError("production publication manifest path is invalid")
    try:
        derived = qualify_base(qualification_inputs)
    except BaseQualificationError as error:
        raise BaseReleasePublicationError("production qualifier rejected release evidence") from error
    if derived.get("qualification_tier") != "production":
        raise BaseReleasePublicationError("nonproduction manifest cannot be published")
    try:
        supplied = manifest_path.read_bytes()
    except OSError as error:
        raise BaseReleasePublicationError("production manifest cannot be read") from error
    if supplied != qualifier_canonical_json(derived):
        raise BaseReleasePublicationError("manifest bytes differ from production qualifier output")
    if (
        prepared_candidate_receipt is None
        or prepared_candidate_receipt_digest is None
        or candidate_root is None
    ):
        raise BaseReleasePublicationError(
            "production publication requires persisted candidate finalizer inputs"
        )
    repository = publication.get("repository")
    if not isinstance(repository, Path) or repository.resolve(strict=True) != candidate_root.resolve(strict=True):
        raise BaseReleasePublicationError("production candidate root differs from release repository")
    try:
        finalized = finalize_prepared_candidate_receipt(
            prepared_candidate_receipt.resolve(strict=True),
            expected_worktree=candidate_root.resolve(strict=True),
            expected_receipt_blake3=prepared_candidate_receipt_digest,
        )
    except (CleanCandidateError, OSError) as error:
        raise BaseReleasePublicationError(
            f"production candidate final filesystem/tooling verification failed: {error}"
        ) from error
    candidate = derived.get("candidate")
    if (
        not isinstance(candidate, dict)
        or finalized.commit != candidate.get("commit")
        or finalized.tree != candidate.get("tree")
        or finalized.release_request_digest != verified_request_digest
        or finalized.qualification_session_id != derived.get("qualification_session_id")
    ):
        raise BaseReleasePublicationError("production candidate finalizer bindings differ")
    return _publish_verified_base_release(
        **publication,
        verified_request_digest=verified_request_digest,
        qualification_tier="production",
        require_signature_time=True,
    )


def publish_verified_base_release_for_test_nonproduction(
    *,
    verified_request_digest: str,
    **publication: object,
) -> PublishedBaseRelease:
    """Explicit nonproduction publication helper; never accepts production output."""
    manifest_path = publication.get("manifest_path")
    if not isinstance(manifest_path, Path):
        raise BaseReleasePublicationError("nonproduction manifest path is invalid")
    manifest, _ = _canonical_path(manifest_path, "nonproduction Base evidence manifest")
    if manifest.get("qualification_tier") != "nonproduction-test":
        raise BaseReleasePublicationError("nonproduction helper requires nonproduction manifest")
    return _publish_verified_base_release(
        **publication,
        verified_request_digest=verified_request_digest,
        qualification_tier="nonproduction-test",
        require_signature_time=False,
    )


def _gpg_callbacks(gpg: Path, gpg_home: Path) -> tuple[Signer, Verifier]:
    exported = subprocess.run(
        [
            str(gpg),
            "--homedir",
            str(gpg_home),
            "--batch",
            "--no-tty",
            "--export",
            RELEASE_SIGNER_FINGERPRINT,
        ],
        capture_output=True,
        check=False,
    )
    if (
        exported.returncode != 0
        or not exported.stdout
        or blake3.blake3(exported.stdout).hexdigest()
        != RELEASE_SIGNER_PUBLIC_PACKET_BLAKE3
    ):
        raise BaseReleasePublicationError(
            "base-release GPG home does not contain the approved public key packet"
        )

    def sign(payload: bytes, usage: str, fingerprint: str) -> bytes:
        with tempfile.TemporaryDirectory(prefix="onebrain-base-release-") as directory:
            root = Path(directory)
            source = root / usage
            signature = root / f"{usage}.asc"
            source.write_bytes(payload)
            completed = subprocess.run(
                [str(gpg), "--homedir", str(gpg_home), "--batch", "--no-tty", "--armor",
                 "--local-user", fingerprint, "--detach-sign", "--output", str(signature), str(source)],
                capture_output=True, check=False,
            )
            if completed.returncode != 0:
                raise BaseReleasePublicationError(f"GPG signing failed for {usage}")
            return signature.read_bytes()

    def verify(payload: bytes, signature_bytes: bytes, _usage: str, fingerprint: str) -> bool | datetime:
        with tempfile.TemporaryDirectory(prefix="onebrain-base-release-verify-") as directory:
            root = Path(directory)
            source = root / "payload"
            signature = root / "payload.asc"
            source.write_bytes(payload)
            signature.write_bytes(signature_bytes)
            completed = subprocess.run(
                [str(gpg), "--homedir", str(gpg_home), "--batch", "--no-tty", "--status-fd", "1",
                 "--verify", str(signature), str(source)], capture_output=True, text=True, check=False,
            )
            valid = [line.split() for line in completed.stdout.splitlines() if line.startswith("[GNUPG:] VALIDSIG ")]
            if not (
                completed.returncode == 0
                and len(valid) == 1
                and len(valid[0]) >= 12
                and valid[0][8] == "22"
                and valid[0][-1] == fingerprint
            ):
                return False
            try:
                return datetime.fromtimestamp(int(valid[0][4]), timezone.utc)
            except (ValueError, OverflowError):
                return False

    return sign, verify


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release-request", type=Path, required=True)
    parser.add_argument("--release-request-signature", type=Path, required=True)
    parser.add_argument("--manifest-ready", type=Path, required=True)
    parser.add_argument("--release-envelope-root", type=Path, required=True)
    parser.add_argument("--release-ready-output", type=Path, required=True)
    parser.add_argument("--signer-policy", type=Path, required=True)
    parser.add_argument("--signer-role", required=True)
    parser.add_argument("--tag", required=True)
    args = parser.parse_args()
    try:
        if args.signer_role != "base-release" or args.tag != TAG_NAME:
            raise BaseReleasePublicationError("Task 28 signer role or tag differs from freeze")
        request = verify_task28_release_request(
            args.release_request,
            args.release_request_signature,
            args.signer_policy,
        )
        ready, manifest_path = verify_manifest_ready(args.manifest_ready, args.release_request)
        evidence_root = args.manifest_ready.resolve(strict=True).parent
        inputs, bundle = read_evidence_root(evidence_root)
        if (
            ready.get("candidate_root") != bundle.get("candidate_root")
            or ready.get("prepared_candidate_receipt") != bundle.get("prepared_candidate_receipt")
            or ready.get("qualification_session_id") != request.get("qualification_session_id")
        ):
            raise BaseReleasePublicationError("Task 28 ready/evidence/request bindings differ")
        gpg_location = shutil.which("gpg") or shutil.which("gpg.exe")
        gpg_home_value = os.environ.get("GNUPGHOME")
        if not gpg_location or not gpg_home_value:
            raise BaseReleasePublicationError(
                "production GPG executable and explicit GNUPGHOME are required"
            )
        sign, verify = _gpg_callbacks(
            Path(gpg_location).resolve(strict=True), Path(gpg_home_value).resolve(strict=True)
        )
        request_digest = blake3.blake3(args.release_request.read_bytes()).hexdigest()
        result = publish_verified_base_release(
            qualification_inputs=inputs,
            prepared_candidate_receipt=Path(str(ready["prepared_candidate_receipt"])),
            prepared_candidate_receipt_digest=str(
                ready["prepared_candidate_receipt_blake3"]
            ),
            candidate_root=Path(str(ready["candidate_root"])),
            repository=Path(str(ready["candidate_root"])),
            manifest_path=manifest_path,
            request_path=args.release_request,
            output_root=evidence_root,
            signer_profile_path=args.signer_policy,
            signer_fingerprint=RELEASE_SIGNER_FINGERPRINT,
            sign=sign,
            verify=verify,
            expected_manifest_digest=str(ready["manifest_digest"]),
            verified_request_digest=request_digest,
            release_envelope_root=args.release_envelope_root,
            release_ready_output=args.release_ready_output,
        )
    except (
        BaseQualificationError,
        BaseReleasePublicationError,
        KeyError,
        OSError,
        ReleaseRequestCreationError,
    ) as error:
        parser.error(str(error))
    print(json.dumps({
        "status": result.status,
        "manifest_digest": result.manifest_digest,
        "tag_object": result.tag_object,
        "ready_pointer": str(result.ready_pointer),
        "receipt": str(result.receipt_path),
    }, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
