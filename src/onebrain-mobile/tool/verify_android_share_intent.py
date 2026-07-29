#!/usr/bin/env python3
"""Exercise Android cold-start share spool, process death, and Rust import."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import time
from datetime import datetime, timezone
from pathlib import Path


PACKAGE = "org.onebrain.onebrain_mobile"
ACTIVITY = f"{PACKAGE}/.MainActivity"
SHARED_TEXT = "OneBrain emulator private shared idea"


def run(command: list[str], cwd: Path | None = None) -> str:
    completed = subprocess.run(
        command,
        cwd=cwd,
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    return completed.stdout + completed.stderr


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
    return run([adb_path, "-s", device, *arguments])


def wait_for_log(
    adb_path: str,
    device: str,
    marker: str,
    timeout_seconds: float,
) -> str:
    deadline = time.monotonic() + timeout_seconds
    while time.monotonic() < deadline:
        output = adb(
            adb_path,
            device,
            "logcat",
            "-d",
            "-s",
            "OneBrainMobileRuntime:I",
            "*:S",
        )
        if marker in output:
            return output
        time.sleep(0.5)
    raise RuntimeError(f"log marker not observed: {marker}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("apk", type=Path)
    parser.add_argument("--device", default="emulator-5554")
    parser.add_argument("--adb")
    parser.add_argument("--flutter", default=shutil.which("flutter"))
    parser.add_argument("--report", type=Path)
    parser.add_argument("--timeout-seconds", type=float, default=30)
    arguments = parser.parse_args()
    adb_path = resolve_adb(arguments.adb)
    if not arguments.flutter:
        raise SystemExit("flutter is required")
    apk = arguments.apk.resolve()
    if not apk.is_file():
        raise SystemExit(f"APK does not exist: {apk}")
    mobile_root = Path(__file__).resolve().parents[1]

    adb(adb_path, arguments.device, "install", "-r", "-t", str(apk))
    adb(adb_path, arguments.device, "shell", "pm", "clear", PACKAGE)
    adb(adb_path, arguments.device, "logcat", "-c")
    adb(
        adb_path,
        arguments.device,
        "shell",
        "am",
        "start",
        "-W",
        "-n",
        ACTIVITY,
        "-a",
        "android.intent.action.SEND",
        "-t",
        "text/plain",
        "--es",
        "android.intent.extra.TEXT",
        SHARED_TEXT,
    )
    landed_log = wait_for_log(
        adb_path,
        arguments.device,
        "share_spool_landed",
        arguments.timeout_seconds,
    )
    if SHARED_TEXT in landed_log:
        raise RuntimeError("share plaintext leaked into runtime log")
    adb(
        adb_path,
        arguments.device,
        "shell",
        "am",
        "start",
        "-W",
        "-n",
        ACTIVITY,
        "-a",
        "android.intent.action.SEND",
        "-t",
        "text/html",
        "--es",
        "android.intent.extra.TEXT",
        "unsupported-html-payload",
    )
    wait_for_log(
        adb_path,
        arguments.device,
        "share_spool_rejected",
        arguments.timeout_seconds,
    )

    adb(adb_path, arguments.device, "shell", "am", "force-stop", PACKAGE)
    adb(adb_path, arguments.device, "logcat", "-c")
    adb(adb_path, arguments.device, "shell", "am", "start", "-W", "-n", ACTIVITY)
    recovered_log = wait_for_log(
        adb_path,
        arguments.device,
        "shareSpools=1",
        arguments.timeout_seconds,
    )
    if "recovered=true" not in recovered_log:
        raise RuntimeError("share spool restart did not recover an unclean generation")

    integration_output = run(
        [
            arguments.flutter,
            "test",
            "integration_test/share_intent_bridge_test.dart",
            "-d",
            arguments.device,
        ],
        cwd=mobile_root,
    )
    if "All tests passed!" not in integration_output:
        raise RuntimeError("share-intent integration test did not pass")

    report = {
        "format": "onebrain.mobile.android-share-intent/1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "device": arguments.device,
        "package": PACKAGE,
        "mime_type": "text/plain",
        "cold_start_spool_landed": True,
        "plaintext_absent_from_runtime_log": True,
        "unsupported_mime_rejected": True,
        "survived_force_stop": True,
        "rust_import_idempotent": True,
        "physical_device_claimed": False,
        "result": "passed",
    }
    rendered = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if arguments.report:
        arguments.report.parent.mkdir(parents=True, exist_ok=True)
        arguments.report.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
