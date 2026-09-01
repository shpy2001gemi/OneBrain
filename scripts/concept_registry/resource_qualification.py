#!/usr/bin/env python3
"""Run bounded cold-cache or low-RAM Concept Registry qualification.

The harness launches the Rust registry probe in a fresh process with its
application lookup cache disabled and artifact verification forced uncached.
On Linux, cold-cache preparation issues POSIX_FADV_DONTNEED for the exact
artifacts; low-RAM qualification applies a hard RLIMIT_AS to the child.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import plistlib
import shutil
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable

import blake3
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey


PROFILE = "onebrain/concept-registry-resource-qualification/1"
PROBE_PROFILE = "onebrain/concept-registry-probe/1"
QUALIFICATION_PROFILES = ("cold-cache", "low-ram", "ssd", "hdd")
MIN_PRODUCTION_REGISTRY_DATA_BYTES = 2_200_000_000
MAX_PRODUCTION_REGISTRY_DATA_BYTES = 2_500_000_000
BUDGETS: dict[str, dict[str, object]] = {
    "ci-small-fixture-v1": {
        "qualification_profiles": ["cold-cache", "low-ram"],
        "max_ready_ms": 60_000,
        "max_p95_us": 1_000_000,
        "max_peak_rss_bytes": 256 * 1024 * 1024,
        "address_space_limit_bytes": 512 * 1024 * 1024,
    },
    "cold-cache-production-v1": {
        "qualification_profiles": ["cold-cache"],
        "max_ready_ms": 180_000,
        "max_p95_us": 250_000,
        "max_peak_rss_bytes": 512 * 1024 * 1024,
        "address_space_limit_bytes": None,
    },
    "low-ram-production-v1": {
        "qualification_profiles": ["low-ram"],
        "max_ready_ms": 300_000,
        "max_p95_us": 500_000,
        "max_peak_rss_bytes": 256 * 1024 * 1024,
        "address_space_limit_bytes": 3 * 1024 * 1024 * 1024,
    },
    "ssd-production-v1": {
        "qualification_profiles": ["ssd"],
        "max_ready_ms": 120_000,
        "max_p95_us": 100_000,
        "max_peak_rss_bytes": 512 * 1024 * 1024,
        "address_space_limit_bytes": None,
    },
    "hdd-production-v1": {
        "qualification_profiles": ["hdd"],
        "max_ready_ms": 300_000,
        "max_p95_us": 750_000,
        "max_peak_rss_bytes": 512 * 1024 * 1024,
        "address_space_limit_bytes": None,
    },
}
MAX_EVIDENCE_BYTES = 1024 * 1024
MAX_STDERR_BYTES = 64 * 1024
POLL_SECONDS = 0.01


class QualificationError(RuntimeError):
    """Qualification could not produce trustworthy evidence."""


def create_resource_receipt(
    report: dict[str, object],
    run_context: dict[str, object],
    binding: dict[str, object],
    signing_key: Ed25519PrivateKey,
    policy: dict[str, object],
) -> dict[str, object]:
    """Bind a resource report to one closed context and sign its receipt."""
    from production_qualification import (
        AggregationError,
        COMMON_BINDINGS,
        create_signed_receipt,
        parse_qualification_run_context,
        signer_fingerprint,
        trust_policy_digest,
    )

    try:
        context = parse_qualification_run_context(run_context)
        if context["variant"] == "Release":
            raise QualificationError(
                "Release receipts require a verified signed release request, not caller context/binding JSON"
            )
        missing = [field for field in COMMON_BINDINGS if field not in binding]
        if missing:
            raise QualificationError(f"release binding missing {missing[0]}")
        public = signing_key.public_key().public_bytes_raw()
        if binding.get("trust_policy_digest") != trust_policy_digest(policy):
            raise QualificationError("release binding trust_policy_digest mismatch")
        if binding.get("signer_fingerprint") != signer_fingerprint(public):
            raise QualificationError("release binding signer_fingerprint mismatch")
        payload: dict[str, object] = {
            **{field: binding[field] for field in COMMON_BINDINGS},
            "qualification_profile": report.get("qualification_profile"),
            "command": [
                "resource_qualification.py",
                "--profile",
                str(report.get("qualification_profile")),
            ],
            "result": report.get("qualified") is True,
            "exit_oracles": report.get("exit_oracles"),
            "limitations": [
                "Registry-only resource evidence; never BASE-GATE-V1",
                "Prequalification evidence cannot derive registry_production_qualified",
            ],
            "resource_report": report,
        }
        if context["variant"] == "Prequalification":
            payload.update(
                {
                    "qualification_context_variant": "Prequalification",
                    "closure_digest": context["closure_digest"],
                    "base_candidate_bound": False,
                    "evidence_tier": "prequalification",
                }
            )
        return create_signed_receipt(
            "resource-qualification", payload, signing_key, policy
        )
    except AggregationError as error:
        raise QualificationError(str(error)) from error


def _create_verified_resource_receipt(
    report: dict[str, object],
    verified: object,
    *,
    test_git_executable: Path | None,
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
    labels_file: Path,
    cache_strategy: str,
    budget_profile: str,
    timeout_seconds: int,
    signing_key: Ed25519PrivateKey,
    policy: dict[str, object],
) -> dict[str, object]:
    """Measure a request-bound candidate and sign one resource receipt."""
    release_dir = Path(__file__).resolve().parents[1] / "release"
    if str(release_dir) not in sys.path:
        sys.path.insert(0, str(release_dir))
    from verify_base_release_request import (
        ReleaseRequestError,
        VerifiedQualificationContextV1,
        VerifiedQualificationContextV2,
        verify_registry_candidate_measurements,
        verify_registry_candidate_measurements_for_test_nonproduction,
    )
    from production_qualification import (
        AggregationError,
        canonical_json,
        create_signed_receipt,
        signer_fingerprint,
        trust_policy_digest,
    )

    if not isinstance(
        verified, (VerifiedQualificationContextV1, VerifiedQualificationContextV2)
    ):
        raise QualificationError("closed verified release context is required")
    if trust_policy_digest(policy) != verified.bindings["trust_policy_digest"]:
        raise QualificationError("Registry trust policy differs from verified request")
    if signer_fingerprint(signing_key.public_key().public_bytes_raw()) != verified.bindings["signer_fingerprint"]:
        raise QualificationError("Registry signer differs from verified request")
    if (
        report.get("budget_profile") != budget_profile
        or report.get("cache_strategy_requested") != cache_strategy
        or not isinstance(report.get("limits"), dict)
        or report["limits"].get("timeout_seconds") != timeout_seconds
    ):
        raise QualificationError("resource report options differ from receipt invocation")
    try:
        measurement_inputs = dict(
            candidate_root=candidate_root,
            registry_root=registry_root,
            release_id=release_id,
            candidate_semantic_evidence=candidate_semantic_evidence,
            production_profile=production_profile,
            production_vector=production_vector,
            append_only_idl_history=append_only_idl_history,
            candidate_tooling=candidate_tooling,
            payload_artifacts=payload_artifacts,
            release_stamp=release_stamp,
            probe=probe,
            probe_signature=probe_signature,
            executable=executable,
            rust_toolchain_evidence=rust_toolchain_evidence,
            runner_image_evidence=runner_image_evidence,
            target_triple=target_triple,
        )
        measured = (
            verify_registry_candidate_measurements(verified, **measurement_inputs)
            if test_git_executable is None
            else verify_registry_candidate_measurements_for_test_nonproduction(
                verified, git_executable=test_git_executable, **measurement_inputs
            )
        )
        context = verified.run_context
        invocation = [
            "resource_qualification.py",
            f"--profile={report.get('qualification_profile')}",
            f"--labels-file={labels_file.name}@blake3:{_blake3_file(labels_file)}",
            f"--cache-strategy={cache_strategy}",
            f"--budget-profile={budget_profile}",
            f"--timeout-seconds={timeout_seconds}",
            f"--release-request-digest={verified.request_digest}",
            f"--candidate-tree={context['candidate_tree']}",
            f"--release-id={release_id}",
            *[
                f"--payload-{name}={path.name}@blake3:{_blake3_file(path)}"
                for name, path in sorted(payload_artifacts.items())
            ],
            f"--release-stamp={release_stamp.name}@blake3:{_blake3_file(release_stamp)}",
            f"--probe={probe.name}@blake3:{_blake3_file(probe)}",
            f"--probe-signature={probe_signature.name}@blake3:{_blake3_file(probe_signature)}",
            f"--executable={executable.name}@blake3:{_blake3_file(executable)}",
            f"--production-profile={production_profile.name}@blake3:{_blake3_file(production_profile)}",
            f"--production-vector={production_vector.name}@blake3:{_blake3_file(production_vector)}",
            f"--idl-history={append_only_idl_history.name}@blake3:{_blake3_file(append_only_idl_history)}",
            f"--rust-toolchain={rust_toolchain_evidence.name}@blake3:{_blake3_file(rust_toolchain_evidence)}",
            f"--runner-image={runner_image_evidence.name}@blake3:{_blake3_file(runner_image_evidence)}",
            *[
                f"--candidate-tool-{name}={path.name}@blake3:{_blake3_file(path)}"
                for name, path in sorted(candidate_tooling.items())
            ],
            f"--target-triple={target_triple}",
            "--gpg-home=<redacted>",
            "--receipt-signer=<external-redacted>",
        ]
        payload: dict[str, object] = {
            **verified.bindings,
            **measured,
            "qualification_context_variant": "Release",
            "release_request_digest": context["release_request_digest"],
            "qualification_session_id": context["qualification_session_id"],
            "candidate_commit": context["candidate_commit"],
            "candidate_tree": context["candidate_tree"],
            "base_candidate_bound": True,
            "evidence_tier": (
                "production-reference" if verified.production else "nonproduction-test"
            ),
            "qualification_profile": report.get("qualification_profile"),
            "command": invocation,
            "command_blake3": blake3.blake3(canonical_json(invocation)).hexdigest(),
            "result": report.get("qualified") is True,
            "exit_oracles": report.get("exit_oracles"),
            "limitations": ["Registry-only resource evidence; never BASE-GATE-V1"],
            "resource_report": report,
        }
        return create_signed_receipt("resource-qualification", payload, signing_key, policy)
    except (ReleaseRequestError, AggregationError) as error:
        raise QualificationError(str(error)) from error


def create_verified_resource_receipt(
    report: dict[str, object],
    verified: object,
    *,
    labels_file: Path,
    cache_strategy: str,
    budget_profile: str,
    timeout_seconds: int,
    **measurements: object,
) -> dict[str, object]:
    """Production receipt producer with fixed production measurement tools."""
    return _create_verified_resource_receipt(
        report, verified, test_git_executable=None,
        labels_file=labels_file, cache_strategy=cache_strategy,
        budget_profile=budget_profile, timeout_seconds=timeout_seconds,
        **measurements
    )


def create_verified_resource_receipt_for_test_nonproduction(
    report: dict[str, object],
    verified: object,
    *,
    git_executable: Path,
    labels_file: Path,
    cache_strategy: str,
    budget_profile: str,
    timeout_seconds: int,
    **measurements: object,
) -> dict[str, object]:
    """Explicit test producer; it cannot accept a production verified context."""
    if getattr(verified, "production", None) is not False:
        raise QualificationError("test-only resource producer rejects production contexts")
    return _create_verified_resource_receipt(
        report,
        verified,
        test_git_executable=git_executable,
        labels_file=labels_file,
        cache_strategy=cache_strategy,
        budget_profile=budget_profile,
        timeout_seconds=timeout_seconds,
        **measurements,
    )


def _utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def _blake3_file(path: Path) -> str:
    digest = blake3.blake3()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def _artifact_paths(obr_path: Path) -> list[Path]:
    return [
        obr_path,
        Path(f"{obr_path}.labels.idx"),
        Path(f"{obr_path}.ccids.idx"),
        Path(f"{obr_path}.manifest.json"),
    ]


def _registry_data_bytes(obr_path: Path) -> int:
    return sum(path.stat().st_size for path in _artifact_paths(obr_path))


def _artifact_evidence(
    probe_path: Path, obr_path: Path, labels_path: Path
) -> dict[str, object]:
    artifacts: dict[str, object] = {}
    for path in [probe_path, *_artifact_paths(obr_path), labels_path]:
        if not path.is_file():
            raise QualificationError(f"required qualification input is missing: {path}")
        artifacts[str(path)] = {
            "bytes": path.stat().st_size,
            "blake3": _blake3_file(path),
        }
    return artifacts


def _prepare_linux_fadvise(paths: list[Path]) -> dict[str, object]:
    if not hasattr(os, "posix_fadvise") or not hasattr(os, "POSIX_FADV_DONTNEED"):
        raise QualificationError("POSIX_FADV_DONTNEED is unavailable on this Linux host")
    advised_bytes = 0
    for path in paths:
        descriptor = os.open(path, os.O_RDONLY)
        try:
            os.posix_fadvise(descriptor, 0, 0, os.POSIX_FADV_DONTNEED)
            advised_bytes += path.stat().st_size
        finally:
            os.close(descriptor)
    return {
        "strategy": "linux-posix-fadvise-dontneed",
        "targeted_artifact_count": len(paths),
        "targeted_bytes": advised_bytes,
        "request_completed": True,
    }


def _prepare_vmtouch(paths: list[Path]) -> dict[str, object]:
    executable = shutil.which("vmtouch")
    if executable is None:
        raise QualificationError("vmtouch is not installed or not on PATH")
    result = subprocess.run(
        [executable, "-e", *map(str, paths)],
        capture_output=True,
        text=True,
        timeout=600,
        check=False,
    )
    if result.returncode != 0:
        detail = (result.stderr or result.stdout).strip()[-2000:]
        raise QualificationError(f"vmtouch cache eviction failed: {detail}")
    return {
        "strategy": "vmtouch-evict",
        "targeted_artifact_count": len(paths),
        "targeted_bytes": sum(path.stat().st_size for path in paths),
        "request_completed": True,
        "tool_output_tail": result.stdout.strip()[-2000:],
    }


def prepare_cold_cache(paths: list[Path], strategy: str) -> dict[str, object]:
    if strategy == "auto":
        if sys.platform.startswith("linux"):
            return _prepare_linux_fadvise(paths)
        if shutil.which("vmtouch") is not None:
            return _prepare_vmtouch(paths)
        raise QualificationError(
            "automatic targeted cache eviction requires Linux POSIX_FADV_DONTNEED "
            "or vmtouch"
        )
    if strategy == "vmtouch":
        return _prepare_vmtouch(paths)
    raise QualificationError(f"unsupported cache preparation strategy: {strategy}")


def _run_collector(command: list[str], label: str) -> subprocess.CompletedProcess[bytes]:
    try:
        result = subprocess.run(
            command,
            capture_output=True,
            timeout=30,
            check=False,
        )
    except (OSError, subprocess.SubprocessError) as error:
        raise QualificationError(f"{label} collector failed: {error}") from error
    if result.returncode != 0:
        detail = result.stderr.decode("utf-8", errors="replace").strip()[-2000:]
        raise QualificationError(f"{label} collector failed: {detail}")
    return result


def _linux_volume_evidence(candidate_path: Path) -> dict[str, object]:
    result = _run_collector(
        ["findmnt", "--noheadings", "--output", "SOURCE,FSTYPE", "--target", str(candidate_path)],
        "Linux findmnt",
    )
    fields = result.stdout.decode("utf-8", errors="strict").strip().split(None, 1)
    if len(fields) != 2 or not fields[0].startswith("/dev/"):
        raise QualificationError("Linux volume source or filesystem type is unknown")
    source, filesystem_type = fields
    device = Path(source).name
    sysfs = Path("/sys/class/block") / device
    try:
        resolved = sysfs.resolve(strict=True)
        if (resolved / "partition").is_file():
            resolved = resolved.parent
        rotational_text = (resolved / "queue" / "rotational").read_text(
            encoding="ascii"
        ).strip()
    except (OSError, UnicodeError) as error:
        raise QualificationError(
            "Linux sysfs block device rotational evidence is unavailable"
        ) from error
    if rotational_text not in {"0", "1"}:
        raise QualificationError("Linux sysfs rotational value is unknown")
    rotational = int(rotational_text)
    return {
        "collector": "linux-sysfs",
        "source": source,
        "filesystem_type": filesystem_type,
        "block_device": resolved.name,
        "rotational": rotational,
        "storage_class": "hdd" if rotational == 1 else "ssd",
    }


def _windows_volume_evidence(candidate_path: Path) -> dict[str, object]:
    script = (
        "& { param($candidate) $p=(Resolve-Path -LiteralPath $candidate).Path;"
        "$v=Get-Volume -FilePath $p;"
        "$part=Get-Partition -DriveLetter $v.DriveLetter;"
        "$disk=Get-PhysicalDisk | Where-Object DeviceId -eq $part.DiskNumber;"
        "[pscustomobject]@{DriveLetter=$v.DriveLetter;FileSystem=$v.FileSystem;"
        "DiskNumber=$part.DiskNumber;MediaType=[string]$disk.MediaType} | ConvertTo-Json -Compress }"
    )
    result = _run_collector(
        ["powershell", "-NoProfile", "-NonInteractive", "-Command", script, str(candidate_path)],
        "Windows physical-disk",
    )
    try:
        value = json.loads(result.stdout.decode("utf-8", errors="strict"))
    except (UnicodeError, json.JSONDecodeError) as error:
        raise QualificationError("Windows physical-disk evidence is invalid") from error
    media_type = str(value.get("MediaType", "")).strip().lower()
    storage_class = {"ssd": "ssd", "hdd": "hdd"}.get(media_type)
    if storage_class is None:
        raise QualificationError("Windows physical-disk media type is unknown")
    return {
        "collector": "windows-physical-disk",
        "source": f"disk-{value.get('DiskNumber')}",
        "filesystem_type": value.get("FileSystem"),
        "media_type": media_type,
        "storage_class": storage_class,
    }


def _macos_volume_evidence(candidate_path: Path) -> dict[str, object]:
    result = _run_collector(["diskutil", "info", "-plist", str(candidate_path)], "macOS diskutil")
    try:
        value = plistlib.loads(result.stdout)
    except Exception as error:
        raise QualificationError("macOS storage evidence is invalid") from error
    solid_state = value.get("SolidState")
    protocol = str(value.get("BusProtocol", "")).strip()
    if not isinstance(solid_state, bool) or not protocol:
        raise QualificationError("macOS solid-state or storage protocol evidence is unknown")
    return {
        "collector": "macos-diskutil",
        "source": value.get("DeviceNode"),
        "filesystem_type": value.get("FilesystemType"),
        "storage_protocol": protocol,
        "solid_state": solid_state,
        "storage_class": "ssd" if solid_state else "hdd",
    }


def collect_volume_evidence(candidate_path: Path) -> dict[str, object]:
    """Capture OS-owned storage class evidence for the candidate filesystem."""
    if sys.platform.startswith("linux"):
        return _linux_volume_evidence(candidate_path)
    if sys.platform == "win32":
        return _windows_volume_evidence(candidate_path)
    if sys.platform == "darwin":
        return _macos_volume_evidence(candidate_path)
    raise QualificationError(f"unsupported platform for volume evidence: {sys.platform}")


def _linux_rss_bytes(process_id: int) -> int | None:
    try:
        lines = Path(f"/proc/{process_id}/status").read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeError):
        return None
    values: dict[str, int] = {}
    for line in lines:
        if line.startswith(("VmRSS:", "VmHWM:")):
            key, value, unit = line.split()
            if unit != "kB":
                return None
            values[key.rstrip(":")] = int(value) * 1024
    return max(values.values(), default=0) or None


def _macos_rss_bytes(process_id: int) -> int | None:
    result = subprocess.run(
        ["ps", "-o", "rss=", "-p", str(process_id)],
        capture_output=True,
        text=True,
        timeout=2,
        check=False,
    )
    if result.returncode != 0 or not result.stdout.strip():
        return None
    try:
        return int(result.stdout.strip()) * 1024
    except ValueError:
        return None


def _windows_rss_bytes(process_id: int) -> int | None:
    try:
        import ctypes
        from ctypes import wintypes

        class ProcessMemoryCounters(ctypes.Structure):
            _fields_ = [
                ("cb", wintypes.DWORD),
                ("PageFaultCount", wintypes.DWORD),
                ("PeakWorkingSetSize", ctypes.c_size_t),
                ("WorkingSetSize", ctypes.c_size_t),
                ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
                ("QuotaPagedPoolUsage", ctypes.c_size_t),
                ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
                ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
                ("PagefileUsage", ctypes.c_size_t),
                ("PeakPagefileUsage", ctypes.c_size_t),
            ]

        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        psapi = ctypes.WinDLL("psapi", use_last_error=True)
        kernel32.OpenProcess.argtypes = [
            wintypes.DWORD,
            wintypes.BOOL,
            wintypes.DWORD,
        ]
        kernel32.OpenProcess.restype = wintypes.HANDLE
        kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
        kernel32.CloseHandle.restype = wintypes.BOOL
        psapi.GetProcessMemoryInfo.argtypes = [
            wintypes.HANDLE,
            ctypes.POINTER(ProcessMemoryCounters),
            wintypes.DWORD,
        ]
        psapi.GetProcessMemoryInfo.restype = wintypes.BOOL
        handle = kernel32.OpenProcess(0x0410, False, process_id)
        if not handle:
            return None
        try:
            counters = ProcessMemoryCounters()
            counters.cb = ctypes.sizeof(counters)
            if not psapi.GetProcessMemoryInfo(
                handle, ctypes.byref(counters), counters.cb
            ):
                return None
            return int(counters.PeakWorkingSetSize)
        finally:
            kernel32.CloseHandle(handle)
    except (AttributeError, OSError, ValueError):
        return None


def current_peak_rss_bytes(process_id: int) -> int | None:
    if sys.platform.startswith("linux"):
        return _linux_rss_bytes(process_id)
    if sys.platform == "darwin":
        return _macos_rss_bytes(process_id)
    if os.name == "nt":
        return _windows_rss_bytes(process_id)
    return None


def _memory_preexec(address_space_limit_bytes: int | None) -> Callable[[], None] | None:
    if address_space_limit_bytes is None:
        return None
    if not sys.platform.startswith("linux"):
        raise QualificationError("hard low-RAM enforcement currently requires Linux")
    import resource

    def apply_limit() -> None:
        resource.setrlimit(
            resource.RLIMIT_AS,
            (address_space_limit_bytes, address_space_limit_bytes),
        )

    return apply_limit


def execute_probe(
    probe_path: Path,
    obr_path: Path,
    labels_path: Path,
    timeout_seconds: int,
    address_space_limit_bytes: int | None,
) -> dict[str, object]:
    if not probe_path.is_file():
        raise QualificationError(f"registry probe executable is missing: {probe_path}")
    command = [
        str(probe_path),
        str(obr_path),
        "--labels-file",
        str(labels_path),
        "--cache-capacity",
        "0",
        "--verification-cache",
        "uncached",
        "--json",
    ]
    started = time.monotonic()
    process = subprocess.Popen(
        command,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        errors="replace",
        preexec_fn=_memory_preexec(address_space_limit_bytes),
    )
    peak_rss = 0
    rss_samples = 0
    timed_out = False
    while True:
        observed = current_peak_rss_bytes(process.pid)
        if observed is not None:
            peak_rss = max(peak_rss, observed)
            rss_samples += 1
        if process.poll() is not None:
            break
        if time.monotonic() - started > timeout_seconds:
            timed_out = True
            process.kill()
            break
        time.sleep(POLL_SECONDS)
    stdout, stderr = process.communicate()
    elapsed_ms = round((time.monotonic() - started) * 1000)
    if len(stdout.encode("utf-8")) > MAX_EVIDENCE_BYTES:
        raise QualificationError("registry probe stdout exceeds evidence limit")
    stderr_bytes = stderr.encode("utf-8")
    if len(stderr_bytes) > MAX_STDERR_BYTES:
        stderr = stderr_bytes[-MAX_STDERR_BYTES:].decode("utf-8", errors="replace")

    probe: dict[str, Any] | None = None
    if process.returncode == 0 and not timed_out:
        try:
            parsed = json.loads(stdout)
        except json.JSONDecodeError as error:
            raise QualificationError(f"registry probe returned invalid JSON: {error}") from error
        if not isinstance(parsed, dict):
            raise QualificationError("registry probe JSON is not an object")
        probe = parsed
    return {
        "exit_code": process.returncode,
        "timed_out": timed_out,
        "elapsed_ms": elapsed_ms,
        "peak_rss_bytes": peak_rss or None,
        "rss_sample_count": rss_samples,
        "stderr_tail": stderr.strip()[-4000:],
        "probe": probe,
    }


def evaluate_oracles(
    qualification_profile: str,
    execution: dict[str, object],
    cache_preparation: dict[str, object],
    address_space_limit_bytes: int | None,
    max_ready_ms: int,
    max_p95_us: int,
    max_peak_rss_bytes: int,
    *,
    volume_evidence: dict[str, object] | None = None,
    registry_data_bytes: int | None = None,
    production_candidate: bool = False,
) -> dict[str, bool]:
    probe = execution.get("probe")
    probe = probe if isinstance(probe, dict) else {}
    peak_rss = execution.get("peak_rss_bytes")
    oracles = {
        "probe_completed": execution.get("exit_code") == 0
        and execution.get("timed_out") is False,
        "probe_profile_is_frozen": probe.get("profile") == PROBE_PROFILE,
        "artifact_verification_is_uncached": probe.get("verification_mode")
        == "uncached",
        "application_lookup_cache_is_disabled": probe.get("cache_capacity") == 0,
        "labels_are_external_to_obr": probe.get("labels_source") == "external-file"
        and probe.get("sampled_from_obr") is False,
        "lookups_were_exercised": isinstance(probe.get("lookups"), int)
        and probe.get("lookups", 0) > 0,
        "representative_hit_was_observed": (
            isinstance(probe.get("found"), int)
            and isinstance(probe.get("ambiguous"), int)
            and probe.get("found", 0) + probe.get("ambiguous", 0) > 0
        ),
        "negative_lookup_was_observed": isinstance(probe.get("missing"), int)
        and probe.get("missing", 0) > 0,
        "ready_time_within_budget": isinstance(probe.get("ready_ms"), int)
        and probe.get("ready_ms", max_ready_ms + 1) <= max_ready_ms,
        "p95_lookup_within_budget": isinstance(probe.get("p95_us"), int)
        and probe.get("p95_us", max_p95_us + 1) <= max_p95_us,
        "peak_rss_was_observed": isinstance(peak_rss, int) and peak_rss > 0,
        "peak_rss_within_budget": isinstance(peak_rss, int)
        and peak_rss <= max_peak_rss_bytes,
    }
    if qualification_profile == "cold-cache":
        oracles["targeted_cache_eviction_request_completed"] = (
            cache_preparation.get("request_completed") is True
        )
    elif qualification_profile == "low-ram":
        oracles["hard_address_space_limit_applied"] = (
            isinstance(address_space_limit_bytes, int)
            and address_space_limit_bytes > 0
        )
    elif qualification_profile == "ssd":
        oracles["storage_is_ssd"] = (
            isinstance(volume_evidence, dict)
            and volume_evidence.get("storage_class") == "ssd"
            and volume_evidence.get("collector")
            in {"linux-sysfs", "windows-physical-disk", "macos-diskutil"}
            and _storage_details_match("ssd", volume_evidence)
        )
    elif qualification_profile == "hdd":
        oracles["storage_is_rotational_hdd"] = (
            isinstance(volume_evidence, dict)
            and volume_evidence.get("storage_class") == "hdd"
            and volume_evidence.get("collector")
            in {"linux-sysfs", "windows-physical-disk", "macos-diskutil"}
            and _storage_details_match("hdd", volume_evidence)
        )
    else:
        raise QualificationError(
            f"unsupported qualification profile: {qualification_profile}"
        )
    if production_candidate:
        oracles["production_registry_data_size_is_inclusive"] = (
            isinstance(registry_data_bytes, int)
            and not isinstance(registry_data_bytes, bool)
            and MIN_PRODUCTION_REGISTRY_DATA_BYTES
            <= registry_data_bytes
            <= MAX_PRODUCTION_REGISTRY_DATA_BYTES
        )
        oracles["production_reference_host_is_linux"] = sys.platform.startswith(
            "linux"
        )
    return oracles


def _storage_details_match(
    expected_class: str, evidence: dict[str, object]
) -> bool:
    collector = evidence.get("collector")
    if collector == "linux-sysfs":
        return evidence.get("rotational") == (0 if expected_class == "ssd" else 1)
    if collector == "windows-physical-disk":
        return evidence.get("media_type") == expected_class
    if collector == "macos-diskutil":
        return evidence.get("solid_state") is (expected_class == "ssd")
    return False


def resolve_budget(
    qualification_profile: str, budget_profile: str
) -> dict[str, object]:
    budget = BUDGETS.get(budget_profile)
    if budget is None:
        raise QualificationError(f"unknown resource budget profile: {budget_profile}")
    allowed = budget["qualification_profiles"]
    if qualification_profile not in allowed:
        raise QualificationError(
            f"budget {budget_profile} does not allow {qualification_profile} qualification"
        )
    return budget


def run_qualification(
    qualification_profile: str,
    probe_path: Path,
    obr_path: Path,
    labels_path: Path,
    cache_strategy: str,
    budget_profile: str,
    timeout_seconds: int,
) -> dict[str, object]:
    if qualification_profile not in QUALIFICATION_PROFILES:
        raise QualificationError(
            f"unsupported qualification profile: {qualification_profile}"
        )
    budget = resolve_budget(qualification_profile, budget_profile)
    max_ready_ms = int(budget["max_ready_ms"])
    max_p95_us = int(budget["max_p95_us"])
    max_peak_rss_bytes = int(budget["max_peak_rss_bytes"])
    address_space_limit_bytes = (
        budget["address_space_limit_bytes"]
        if qualification_profile == "low-ram"
        else None
    )
    for name, value in {"timeout_seconds": timeout_seconds}.items():
        if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
            raise QualificationError(f"{name} must be a positive integer")
    if qualification_profile == "low-ram":
        if address_space_limit_bytes is None or address_space_limit_bytes <= 0:
            raise QualificationError("low-ram profile requires a hard address-space limit")
        if address_space_limit_bytes < max_peak_rss_bytes:
            raise QualificationError(
                "address-space limit cannot be smaller than the peak RSS budget"
            )

    production_candidate = budget_profile != "ci-small-fixture-v1"
    artifacts = _artifact_evidence(probe_path, obr_path, labels_path)
    registry_data_bytes = (
        _registry_data_bytes(obr_path) if production_candidate else None
    )
    if qualification_profile == "cold-cache":
        cache_preparation = prepare_cold_cache(_artifact_paths(obr_path), cache_strategy)
    else:
        cache_preparation = {
            "strategy": f"not-required-for-{qualification_profile}",
            "request_completed": False,
        }
    volume_evidence = (
        collect_volume_evidence(obr_path)
        if qualification_profile in {"ssd", "hdd"}
        else None
    )
    execution = execute_probe(
        probe_path,
        obr_path,
        labels_path,
        timeout_seconds,
        address_space_limit_bytes,
    )
    oracles = evaluate_oracles(
        qualification_profile,
        execution,
        cache_preparation,
        address_space_limit_bytes,
        max_ready_ms,
        max_p95_us,
        max_peak_rss_bytes,
        volume_evidence=volume_evidence,
        registry_data_bytes=registry_data_bytes,
        production_candidate=production_candidate,
    )
    return {
        "profile": PROFILE,
        "qualification_profile": qualification_profile,
        "budget_profile": budget_profile,
        "cache_strategy_requested": cache_strategy,
        "generated_at_utc": _utc_now(),
        "host": {
            "system": platform.system(),
            "release": platform.release(),
            "machine": platform.machine(),
            "python": platform.python_version(),
        },
        "filesystem": {
            "candidate_path": str(obr_path.resolve()),
            "volume_evidence_captured": volume_evidence is not None,
        },
        "volume_evidence": volume_evidence,
        "candidate": {
            "obr_path": str(obr_path.resolve()),
            "obr_bytes": obr_path.stat().st_size if obr_path.is_file() else None,
            "registry_data_bytes": registry_data_bytes,
        },
        "artifacts": artifacts,
        "cache_preparation": cache_preparation,
        "memory_enforcement": {
            "strategy": (
                "linux-rlimit-as"
                if address_space_limit_bytes is not None
                else "none"
            ),
            "address_space_limit_bytes": address_space_limit_bytes,
        },
        "limits": {
            "max_ready_ms": max_ready_ms,
            "max_p95_us": max_p95_us,
            "max_peak_rss_bytes": max_peak_rss_bytes,
            "timeout_seconds": timeout_seconds,
        },
        "execution": execution,
        "exit_oracles": oracles,
        "qualified": all(oracles.values()),
        "base_candidate_bound": False,
        "evidence_tier": "prequalification",
        "production_qualified": False,
    }


def _write_report(path: Path, report: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary_name = tempfile.mkstemp(
        prefix=f".{path.name}.", suffix=".tmp", dir=path.parent
    )
    temporary_path = Path(temporary_name)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as handle:
            json.dump(report, handle, indent=2, sort_keys=True)
            handle.write("\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary_path, path)
        if os.name != "nt":
            directory_fd = os.open(path.parent, os.O_RDONLY)
            try:
                os.fsync(directory_fd)
            finally:
                os.close(directory_fd)
    finally:
        temporary_path.unlink(missing_ok=True)


def _positive_int(value: str) -> int:
    parsed = int(value)
    if parsed <= 0:
        raise argparse.ArgumentTypeError("value must be positive")
    return parsed


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=QUALIFICATION_PROFILES, required=True)
    parser.add_argument("--probe", type=Path, required=True)
    parser.add_argument("--obr", type=Path, required=True)
    parser.add_argument("--labels-file", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--cache-strategy", choices=("auto", "vmtouch"), default="auto")
    parser.add_argument("--budget-profile", choices=tuple(BUDGETS), required=True)
    parser.add_argument("--timeout-seconds", type=_positive_int, default=600)
    parser.add_argument("--run-context", type=Path)
    parser.add_argument("--release-binding", type=Path)
    parser.add_argument("--trust-policy", type=Path)
    parser.add_argument("--private-key", type=Path)
    parser.add_argument("--release-request", type=Path)
    parser.add_argument("--release-request-signature", type=Path)
    parser.add_argument("--qualification-approver-policy", type=Path)
    parser.add_argument("--task28-registry-binding", type=Path)
    parser.add_argument("--gpg-home", type=Path)
    parser.add_argument("--candidate-root", type=Path)
    parser.add_argument("--registry-root", type=Path)
    parser.add_argument("--release-id")
    parser.add_argument("--candidate-semantic-evidence", type=Path)
    parser.add_argument("--production-profile", type=Path)
    parser.add_argument("--production-vector", type=Path)
    parser.add_argument("--append-only-idl-history", type=Path)
    for tooling_name in ("qualifier", "request", "clean-worktree", "release-wrapper", "verifier", "signer-policy"):
        parser.add_argument(f"--candidate-tool-{tooling_name}", type=Path)
    parser.add_argument("--label-index", type=Path)
    parser.add_argument("--ccid-index", type=Path)
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--sbom", type=Path)
    parser.add_argument("--release-stamp", type=Path)
    parser.add_argument("--probe-signature", type=Path)
    parser.add_argument("--executable", type=Path)
    parser.add_argument("--rust-toolchain-evidence", type=Path)
    parser.add_argument("--runner-image-evidence", type=Path)
    parser.add_argument("--target-triple")
    return parser


def _read_json_object(path: Path) -> dict[str, object]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise QualificationError(f"JSON input is not an object: {path}")
    return value


def _read_private_key(path: Path) -> Ed25519PrivateKey:
    try:
        value = path.read_text(encoding="ascii").strip()
    except OSError as error:
        raise QualificationError("private signing key could not be read") from error
    if len(value) != 64 or any(character not in "0123456789abcdef" for character in value):
        raise QualificationError("private signing key must be exactly 64 lowercase hex digits")
    return Ed25519PrivateKey.from_private_bytes(bytes.fromhex(value))


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    try:
        report = run_qualification(
            args.profile,
            args.probe,
            args.obr,
            args.labels_file,
            args.cache_strategy,
            args.budget_profile,
            args.timeout_seconds,
        )
        qualified = report.get("qualified") is True
        verified_inputs = (
            args.release_request,
            args.release_request_signature,
            args.qualification_approver_policy,
            args.gpg_home,
            args.candidate_root,
            args.registry_root,
            args.release_id,
            args.candidate_semantic_evidence,
            args.production_profile,
            args.production_vector,
            args.append_only_idl_history,
            args.candidate_tool_qualifier,
            args.candidate_tool_request,
            args.candidate_tool_clean_worktree,
            args.candidate_tool_release_wrapper,
            args.candidate_tool_verifier,
            args.candidate_tool_signer_policy,
            args.label_index,
            args.ccid_index,
            args.manifest,
            args.sbom,
            args.release_stamp,
            args.probe_signature,
            args.executable,
            args.rust_toolchain_evidence,
            args.runner_image_evidence,
            args.target_triple,
            args.trust_policy,
            args.private_key,
        )
        signing_inputs = (
            args.run_context,
            args.release_binding,
            args.trust_policy,
            args.private_key,
        )
        if any(value is not None for value in verified_inputs[:-2]):
            if not all(value is not None for value in verified_inputs):
                raise QualificationError("all verified release-request and measured candidate inputs are required together")
            if args.run_context is not None or args.release_binding is not None:
                raise QualificationError("caller run context/binding overrides are forbidden in Release mode")
            release_dir = Path(__file__).resolve().parents[1] / "release"
            if str(release_dir) not in sys.path:
                sys.path.insert(0, str(release_dir))
            from verify_base_release_request import (
                load_task28_registry_measurement_context,
                verify_release_request,
                verify_task28_release_request,
            )
            try:
                if args.task28_registry_binding is None:
                    verified = verify_release_request(
                        args.release_request,
                        args.release_request_signature,
                        args.qualification_approver_policy,
                        args.gpg_home,
                    )
                else:
                    verified = verify_task28_release_request(
                        args.release_request,
                        args.release_request_signature,
                        args.qualification_approver_policy,
                        gpg_home=args.gpg_home,
                        gpg_executable=Path("/usr/bin/gpg"),
                    )
                    verified = load_task28_registry_measurement_context(
                        verified, args.task28_registry_binding
                    )
            except RuntimeError as error:
                raise QualificationError(str(error)) from error
            report = create_verified_resource_receipt(
                report,
                verified,
                candidate_root=args.candidate_root,
                registry_root=args.registry_root,
                release_id=args.release_id,
                candidate_semantic_evidence=args.candidate_semantic_evidence,
                production_profile=args.production_profile,
                production_vector=args.production_vector,
                append_only_idl_history=args.append_only_idl_history,
                candidate_tooling={
                    "qualifier": args.candidate_tool_qualifier,
                    "request": args.candidate_tool_request,
                    "clean_worktree": args.candidate_tool_clean_worktree,
                    "release_wrapper": args.candidate_tool_release_wrapper,
                    "verifier": args.candidate_tool_verifier,
                    "signer_policy": args.candidate_tool_signer_policy,
                },
                payload_artifacts={
                    "OBR:concepts.obr": args.obr,
                    "LABEL_INDEX:concepts.obr.labels.idx": args.label_index,
                    "CCID_INDEX:concepts.obr.ccids.idx": args.ccid_index,
                    "MANIFEST:concepts.obr.manifest.json": args.manifest,
                    "SPDX_SBOM:sbom.spdx.json": args.sbom,
                },
                release_stamp=args.release_stamp,
                probe=args.probe,
                probe_signature=args.probe_signature,
                executable=args.executable,
                rust_toolchain_evidence=args.rust_toolchain_evidence,
                runner_image_evidence=args.runner_image_evidence,
                target_triple=args.target_triple,
                labels_file=args.labels_file,
                cache_strategy=args.cache_strategy,
                budget_profile=args.budget_profile,
                timeout_seconds=args.timeout_seconds,
                signing_key=_read_private_key(args.private_key),
                policy=_read_json_object(args.trust_policy),
            )
        elif any(value is not None for value in signing_inputs):
            if not all(value is not None for value in signing_inputs):
                raise QualificationError(
                    "run context, release binding, trust policy, and private key are required together"
                )
            report = create_resource_receipt(
                report,
                _read_json_object(args.run_context),
                _read_json_object(args.release_binding),
                _read_private_key(args.private_key),
                _read_json_object(args.trust_policy),
            )
        elif args.budget_profile != "ci-small-fixture-v1":
            raise QualificationError(
                "production resource qualification requires signed run context and binding"
            )
        _write_report(args.output, report)
    except (OSError, subprocess.SubprocessError, QualificationError, ValueError) as error:
        print(f"Concept Registry resource qualification failed: {error}", file=sys.stderr)
        return 2
    print(json.dumps(report, sort_keys=True))
    return 0 if qualified else 1


if __name__ == "__main__":
    raise SystemExit(main())
