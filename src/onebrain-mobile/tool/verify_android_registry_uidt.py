#!/usr/bin/env python3
"""Exercise the MOB-05B Android UIDT durable submit/adopt barrier."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import time
from pathlib import Path


PACKAGE = "org.onebrain.onebrain_mobile"
ACTIVITY = f"{PACKAGE}/.MainActivity"
PROBE = f"{PACKAGE}/.RegistryUidtProbeReceiver"
ACTION = f"{PACKAGE}.debug.REGISTRY_UIDT_PROBE"
LOG_TAG = "OneBrainRegistryUidtProbe"
SERVICE = f"{PACKAGE}.RegistryTransferJobService"


def resolve_adb(explicit: str | None) -> str:
    if explicit:
        return explicit
    discovered = shutil.which("adb")
    if discovered:
        return discovered
    local_app_data = os.environ.get("LOCALAPPDATA")
    if local_app_data:
        candidate = (
            Path(local_app_data) / "Android" / "Sdk" / "platform-tools" / "adb.exe"
        )
        if candidate.is_file():
            return str(candidate)
    raise SystemExit("adb is required")


def adb(adb_path: str, device: str, *arguments: str) -> str:
    completed = subprocess.run(
        [adb_path, "-s", device, *arguments],
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=90,
    )
    return completed.stdout


def read_probe(adb_path: str, device: str) -> dict[str, object] | None:
    output = adb(adb_path, device, "logcat", "-d", "-s", f"{LOG_TAG}:V", "*:S")
    for line in reversed(output.splitlines()):
        start = line.find("{")
        if start < 0:
            continue
        try:
            value = json.loads(line[start:])
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict):
            return value
    return None


def wait_for_probe(
    adb_path: str,
    device: str,
    expected_status: str,
    timeout_seconds: float,
) -> dict[str, object]:
    deadline = time.monotonic() + timeout_seconds
    while time.monotonic() < deadline:
        value = read_probe(adb_path, device)
        if value and value.get("status") == expected_status:
            return value
        if value and value.get("status") == "FAILED":
            raise RuntimeError(f"UIDT probe failed: {value}")
        time.sleep(0.25)
    raise TimeoutError(f"UIDT probe did not reach {expected_status}")


def send_probe(adb_path: str, device: str, mode: str) -> None:
    adb(adb_path, device, "logcat", "-c")
    adb(
        adb_path,
        device,
        "shell",
        "am",
        "broadcast",
        "-n",
        PROBE,
        "-a",
        ACTION,
        "--es",
        "mode",
        mode,
    )


def job_inventory(adb_path: str, device: str) -> str:
    return adb(adb_path, device, "shell", "dumpsys", "jobscheduler")


def assert_job_present(inventory: str, job_id: int) -> None:
    if SERVICE not in inventory or str(job_id) not in inventory:
        raise AssertionError("The exact OneBrain UIDT job is absent from JobScheduler inventory")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("apk", type=Path)
    parser.add_argument("--device", default="emulator-5554")
    parser.add_argument("--adb")
    parser.add_argument("--timeout-seconds", type=float, default=30.0)
    parser.add_argument("--report", type=Path)
    arguments = parser.parse_args()
    arguments.adb = resolve_adb(arguments.adb)
    apk = arguments.apk.resolve()
    if not apk.is_file():
        raise SystemExit(f"APK not found: {apk}")

    adb(arguments.adb, arguments.device, "install", "-r", "-t", str(apk))
    adb(arguments.adb, arguments.device, "shell", "pm", "clear", PACKAGE)
    adb(arguments.adb, arguments.device, "shell", "dumpsys", "battery", "unplug")
    adb(arguments.adb, arguments.device, "shell", "dumpsys", "battery", "set", "status", "3")
    try:
        adb(arguments.adb, arguments.device, "shell", "am", "start", "-W", "-n", ACTIVITY)
        send_probe(arguments.adb, arguments.device, "schedule_only")
        scheduled = wait_for_probe(
            arguments.adb,
            arguments.device,
            "SCHEDULED_ONLY",
            arguments.timeout_seconds,
        )
        job_id = int(scheduled["job_id"])
        transfer_nonce = str(scheduled["transfer_nonce"])
        if int(scheduled["rust_state"]) != 0:
            raise AssertionError("Rust was not left at SchedulePrepared")
        if int(scheduled["expected_total_bytes"]) <= 0:
            raise AssertionError("UIDT byte estimate is not bound to exact manifest bytes")
        before_kill = job_inventory(arguments.adb, arguments.device)
        assert_job_present(before_kill, job_id)

        adb(arguments.adb, arguments.device, "shell", "input", "keyevent", "KEYCODE_HOME")
        adb(arguments.adb, arguments.device, "shell", "am", "kill", PACKAGE)
        after_kill = job_inventory(arguments.adb, arguments.device)
        assert_job_present(after_kill, job_id)

        adb(arguments.adb, arguments.device, "shell", "am", "start", "-W", "-n", ACTIVITY)
        send_probe(arguments.adb, arguments.device, "reconcile")
        adopted = wait_for_probe(
            arguments.adb,
            arguments.device,
            "ADOPTED",
            arguments.timeout_seconds,
        )
        if int(adopted["matching_job_count"]) != 1:
            raise AssertionError("Recovery did not find exactly one UIDT job")
        if int(adopted["rust_state"]) != 2:
            raise AssertionError("Rust did not durably adopt the scheduled UIDT job")
        if int(adopted["job_id"]) != job_id or adopted["transfer_nonce"] != transfer_nonce:
            raise AssertionError("Recovery adopted a different UIDT request")

        send_probe(arguments.adb, arguments.device, "stop")
        stopped = wait_for_probe(
            arguments.adb,
            arguments.device,
            "USER_STOPPED",
            arguments.timeout_seconds,
        )
        if int(stopped["rust_state"]) != 4:
            raise AssertionError("Positive user Stop did not remain distinct in Rust")

        report = {
            "format": "onebrain.mobile.android-registry-uidt-emulator/1",
            "device": arguments.device,
            "result": "passed",
            "job_id": job_id,
            "transfer_nonce_prefix": transfer_nonce[:26],
            "expected_total_bytes": int(scheduled["expected_total_bytes"]),
            "crash_window": "scheduled_before_rust_submit_bind",
            "recovery": adopted,
            "stop": stopped,
            "limitations": [
                "charging was forced false so the barrier probe remained pending and transferred no bytes",
                "the signed debug fixture is 6 KiB; the real 2.2 GB-class estimate and transfer remain an explicit qualification gate",
                "the debug-only descriptor is a digest fixture, not a URL or production transport authority",
                "this does not qualify HTTPS range landing, full-size transfer, Task Manager Stop, Doze, thermal pressure, reboot, or a physical device",
            ],
        }
        rendered = json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True)
        if arguments.report:
            arguments.report.parent.mkdir(parents=True, exist_ok=True)
            arguments.report.write_text(rendered + "\n", encoding="utf-8")
        print(rendered)
    finally:
        adb(arguments.adb, arguments.device, "shell", "dumpsys", "battery", "reset")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
