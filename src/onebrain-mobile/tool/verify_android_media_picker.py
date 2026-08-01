#!/usr/bin/env python3
"""Drive Android's picker and verify durable Rust OwnedOriginal import."""

from __future__ import annotations

import argparse
import base64
import json
import os
import re
import shutil
import subprocess
import tempfile
import time
import xml.etree.ElementTree as ET
from datetime import datetime, timezone
from pathlib import Path


PACKAGE = "org.onebrain.onebrain_mobile"
PICKER_FILE = "onebrain-picker-private-test.png"
PNG_BASE64 = (
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR4nGNg"
    "YAAAAAMAASsJTYQAAAAASUVORK5CYII="
)


def run(
    command: list[str],
    *,
    cwd: Path | None = None,
    check: bool = True,
    timeout: float | None = None,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=cwd,
        check=check,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=timeout,
    )


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
    completed = run([adb_path, "-s", device, *arguments])
    return completed.stdout + completed.stderr


def picker_node(adb_path: str, device: str) -> tuple[int, int] | None:
    adb(adb_path, device, "shell", "uiautomator", "dump", "/sdcard/window.xml")
    xml = adb(adb_path, device, "exec-out", "cat", "/sdcard/window.xml")
    try:
        root = ET.fromstring(xml)
    except ET.ParseError:
        return None
    for node in root.iter("node"):
        text = node.attrib.get("text", "")
        description = node.attrib.get("content-desc", "")
        if text != PICKER_FILE and not description.startswith(f"{PICKER_FILE},"):
            continue
        bounds = node.attrib.get("bounds", "")
        match = re.fullmatch(r"\[(\d+),(\d+)\]\[(\d+),(\d+)\]", bounds)
        if match:
            left, top, right, bottom = (int(value) for value in match.groups())
            return ((left + right) // 2, (top + bottom) // 2)
    return None


def wait_and_select_picker(
    adb_path: str,
    device: str,
    timeout_seconds: float,
) -> None:
    deadline = time.monotonic() + timeout_seconds
    while time.monotonic() < deadline:
        location = picker_node(adb_path, device)
        if location is not None:
            adb(
                adb_path,
                device,
                "shell",
                "input",
                "tap",
                str(location[0]),
                str(location[1]),
            )
            return
        time.sleep(0.5)
    raise RuntimeError("system picker did not expose the prepared PNG")


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
    parser.add_argument("--timeout-seconds", type=float, default=60)
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
    with tempfile.TemporaryDirectory(prefix="onebrain-media-picker-") as temp:
        local_png = Path(temp) / PICKER_FILE
        local_png.write_bytes(base64.b64decode(PNG_BASE64))
        adb(
            adb_path,
            arguments.device,
            "push",
            str(local_png),
            f"/sdcard/Download/{PICKER_FILE}",
        )
    adb(
        adb_path,
        arguments.device,
        "shell",
        "am",
        "broadcast",
        "-a",
        "android.intent.action.MEDIA_SCANNER_SCAN_FILE",
        "-d",
        f"file:///sdcard/Download/{PICKER_FILE}",
    )
    adb(adb_path, arguments.device, "logcat", "-c")

    process = subprocess.Popen(
        [
            arguments.flutter,
            "drive",
            "--driver=test_driver/integration_test.dart",
            "--target=integration_test/media_picker_bridge_test.dart",
            "-d",
            arguments.device,
            "--keep-app-running",
        ],
        cwd=mobile_root,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    try:
        wait_and_select_picker(
            adb_path,
            arguments.device,
            arguments.timeout_seconds,
        )
        output, _ = process.communicate(timeout=arguments.timeout_seconds * 3)
    except BaseException:
        process.kill()
        output, _ = process.communicate()
        raise RuntimeError(f"media picker integration failed:\n{output}")
    if process.returncode != 0:
        raise RuntimeError(f"media picker integration failed:\n{output}")

    log = wait_for_log(
        adb_path,
        arguments.device,
        "owned_original_media_imported class=image",
        arguments.timeout_seconds,
    )
    if PICKER_FILE in log or "content://" in log or "file://" in log:
        raise RuntimeError("picker filename or URI leaked into runtime logs")

    adb(adb_path, arguments.device, "shell", "am", "force-stop", PACKAGE)
    recovery = run(
        [
            arguments.flutter,
            "drive",
            "--driver=test_driver/integration_test.dart",
            "--target=integration_test/owned_media_recovery_test.dart",
            "-d",
            arguments.device,
        ],
        cwd=mobile_root,
        check=False,
        timeout=arguments.timeout_seconds * 4,
    )
    recovery_output = recovery.stdout + recovery.stderr
    if recovery.returncode != 0:
        raise RuntimeError(f"OwnedOriginal recovery integration failed:\n{recovery_output}")

    report = {
        "format": "onebrain.mobile.android-owned-media/1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "device": arguments.device,
        "package": PACKAGE,
        "system_picker_exercised": True,
        "native_streamed_without_dart_path": True,
        "rust_chunk_encryption_and_full_length_verified": True,
        "magic_byte_mime_verified": True,
        "runtime_log_redacted": True,
        "files_activated_before_reference_commit": True,
        "owned_hold_verified": True,
        "typed_catalog_recovery_after_force_stop": True,
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
