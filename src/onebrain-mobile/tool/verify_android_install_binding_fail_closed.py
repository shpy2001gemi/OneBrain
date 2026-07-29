#!/usr/bin/env python3
"""Inject unbound Android authority residue and require fail-closed startup."""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import time
from datetime import datetime, timezone
from pathlib import Path


PACKAGE = "org.onebrain.onebrain_mobile"
ACTIVITY = f"{PACKAGE}/.MainActivity"
LOG_TAG = "OneBrainMobileRuntime"
MARKER = "no_backup/security/install-marker.v1"
REJECTION = "secure runtime open rejected"


def run(adb: str, device: str, *arguments: str) -> str:
    completed = subprocess.run(
        [adb, "-s", device, *arguments],
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    return completed.stdout


def wait_for_log(adb: str, device: str, needle: str, timeout_seconds: float) -> str:
    deadline = time.monotonic() + timeout_seconds
    while time.monotonic() < deadline:
        output = run(adb, device, "logcat", "-d", "-s", f"{LOG_TAG}:V", "*:S")
        if needle in output:
            return output
        time.sleep(0.5)
    raise RuntimeError(f"log did not contain {needle!r} within {timeout_seconds:g} seconds")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("apk", type=Path)
    parser.add_argument("--device", default="emulator-5554")
    parser.add_argument("--adb", default=shutil.which("adb"))
    parser.add_argument("--report", type=Path)
    parser.add_argument("--timeout-seconds", type=float, default=30)
    arguments = parser.parse_args()
    if not arguments.adb:
        raise SystemExit("adb is required")
    apk = arguments.apk.resolve()
    if not apk.is_file():
        raise SystemExit(f"APK does not exist: {apk}")

    run(arguments.adb, arguments.device, "install", "-r", "-t", str(apk))
    run(arguments.adb, arguments.device, "shell", "pm", "clear", PACKAGE)
    run(arguments.adb, arguments.device, "logcat", "-c")
    run(arguments.adb, arguments.device, "shell", "am", "start", "-W", "-n", ACTIVITY)
    wait_for_log(
        arguments.adb,
        arguments.device,
        "profile=MOB-03/1",
        arguments.timeout_seconds,
    )

    run(arguments.adb, arguments.device, "shell", "am", "force-stop", PACKAGE)
    run(arguments.adb, arguments.device, "shell", "run-as", PACKAGE, "rm", MARKER)
    run(arguments.adb, arguments.device, "logcat", "-c")
    run(arguments.adb, arguments.device, "shell", "am", "start", "-W", "-n", ACTIVITY)
    output = wait_for_log(
        arguments.adb,
        arguments.device,
        REJECTION,
        arguments.timeout_seconds,
    )
    if "profile=MOB-03/1" in output:
        raise RuntimeError("unbound authority unexpectedly produced a successful runtime snapshot")

    report = {
        "format": "onebrain.mobile.install-binding-fail-closed/1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "device": arguments.device,
        "package": PACKAGE,
        "injection": "removed exact no-backup install marker while authority remained",
        "runtime_snapshot_after_injection": False,
        "redacted_rejection_observed": True,
        "physical_device_claimed": False,
        "result": "passed",
    }
    rendered = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if arguments.report:
        arguments.report.parent.mkdir(parents=True, exist_ok=True)
        arguments.report.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
    run(arguments.adb, arguments.device, "shell", "pm", "clear", PACKAGE)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
