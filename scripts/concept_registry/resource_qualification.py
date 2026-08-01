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
import shutil
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable

import blake3


PROFILE = "onebrain/concept-registry-resource-qualification/1"
PROBE_PROFILE = "onebrain/concept-registry-probe/1"
QUALIFICATION_PROFILES = ("cold-cache", "low-ram")
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
}
MAX_EVIDENCE_BYTES = 1024 * 1024
MAX_STDERR_BYTES = 64 * 1024
POLL_SECONDS = 0.01


class QualificationError(RuntimeError):
    """Qualification could not produce trustworthy evidence."""


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
    else:
        raise QualificationError(
            f"unsupported qualification profile: {qualification_profile}"
        )
    return oracles


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
    address_space_limit_bytes = budget["address_space_limit_bytes"]
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
    elif address_space_limit_bytes is not None:
        raise QualificationError(
            "address-space limit is only valid for the low-ram profile"
        )

    artifacts = _artifact_evidence(probe_path, obr_path, labels_path)
    if qualification_profile == "cold-cache":
        cache_preparation = prepare_cold_cache(_artifact_paths(obr_path), cache_strategy)
    else:
        cache_preparation = {
            "strategy": "not-required-for-low-ram",
            "request_completed": False,
        }
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
    )
    return {
        "profile": PROFILE,
        "qualification_profile": qualification_profile,
        "budget_profile": budget_profile,
        "generated_at_utc": _utc_now(),
        "host": {
            "system": platform.system(),
            "release": platform.release(),
            "machine": platform.machine(),
            "python": platform.python_version(),
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
    return parser


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
        _write_report(args.output, report)
    except (OSError, subprocess.SubprocessError, QualificationError) as error:
        print(f"Concept Registry resource qualification failed: {error}", file=sys.stderr)
        return 2
    print(json.dumps(report, sort_keys=True))
    return 0 if report["qualified"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
