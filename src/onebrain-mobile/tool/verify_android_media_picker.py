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
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path


PACKAGE = "org.onebrain.onebrain_mobile"
PICKER_FILE = "onebrain-picker-private-test.png"
PNG_BASE64 = (
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR4nGNg"
    "YAAAAAMAASsJTYQAAAAASUVORK5CYII="
)


@dataclass(frozen=True)
class PickerControls:
    file_location: tuple[int, int] | None
    confirmation_location: tuple[int, int] | None


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


def node_center(node: ET.Element) -> tuple[int, int] | None:
    bounds = node.attrib.get("bounds", "")
    match = re.fullmatch(r"\[(\d+),(\d+)\]\[(\d+),(\d+)\]", bounds)
    if not match:
        return None
    left, top, right, bottom = (int(value) for value in match.groups())
    return ((left + right) // 2, (top + bottom) // 2)


def picker_controls(adb_path: str, device: str) -> PickerControls:
    try:
        adb(adb_path, device, "shell", "uiautomator", "dump", "/sdcard/window.xml")
        xml = adb(adb_path, device, "exec-out", "cat", "/sdcard/window.xml")
    except subprocess.CalledProcessError:
        return PickerControls(None, None)
    try:
        root = ET.fromstring(xml)
    except ET.ParseError:
        return PickerControls(None, None)
    file_location = None
    confirmation_location = None
    for node in root.iter("node"):
        text = node.attrib.get("text", "")
        description = node.attrib.get("content-desc", "")
        if text == PICKER_FILE or description.startswith(f"{PICKER_FILE},"):
            bounds = node.attrib.get("bounds", "")
            match = re.fullmatch(r"\[(\d+),(\d+)\]\[(\d+),(\d+)\]", bounds)
            if not match:
                continue
            left, top, right, bottom = (int(value) for value in match.groups())
            # DocumentsUI overlays a preview affordance over the upper-right of
            # image tiles. Use the lower-left quadrant so the harness selects
            # the document instead of opening that preview on narrow runners.
            file_location = (
                left + (right - left) // 4,
                top + ((bottom - top) * 3) // 4,
            )
            continue
        label = (text or description).strip().casefold()
        resource_id = node.attrib.get("resource-id", "").casefold()
        is_confirmation = label in {
            "add",
            "choose",
            "done",
            "open",
            "select",
            "use this photo",
        } or resource_id.endswith(
            (
                ":id/action_menu_select",
                ":id/button_add",
                ":id/picker_action_button",
            )
        )
        if (
            is_confirmation
            and node.attrib.get("clickable") == "true"
            and node.attrib.get("enabled") == "true"
        ):
            confirmation_location = node_center(node)
    return PickerControls(file_location, confirmation_location)


def diagnostic_tail(output: str) -> str:
    redacted = output.replace(PICKER_FILE, "[REDACTED_FILE]")
    redacted = re.sub(r"(?:content|file)://\S+", "[REDACTED_URI]", redacted)
    redacted = re.sub(r"\x1b\[[0-9;]*[A-Za-z]", "", redacted)
    lines = [line.strip() for line in redacted.splitlines() if line.strip()]
    if not lines:
        return "no diagnostic output"
    return " | ".join(lines[-8:])[-2000:]


def app_is_resumed(adb_path: str, device: str) -> bool:
    try:
        activities = adb(
            adb_path,
            device,
            "shell",
            "dumpsys",
            "activity",
            "activities",
        )
    except subprocess.CalledProcessError:
        return False
    return any(
        "topResumedActivity=" in line and PACKAGE in line
        for line in activities.splitlines()
    )


def wait_and_select_picker(
    adb_path: str,
    device: str,
    timeout_seconds: float,
) -> None:
    deadline = time.monotonic() + timeout_seconds
    last_tap = 0.0
    selection_attempted = False
    confirmation_attempted = False
    while time.monotonic() < deadline:
        if (selection_attempted or confirmation_attempted) and app_is_resumed(
            adb_path, device
        ):
            return
        controls = picker_controls(adb_path, device)
        now = time.monotonic()
        location = None
        if not selection_attempted and controls.file_location is not None:
            location = controls.file_location
            selection_attempted = True
        elif controls.confirmation_location is not None:
            location = controls.confirmation_location
            confirmation_attempted = True
        if location is not None and now - last_tap >= 1.0:
            adb(
                adb_path,
                device,
                "shell",
                "input",
                "tap",
                str(location[0]),
                str(location[1]),
            )
            last_tap = now
        time.sleep(0.5)
    if confirmation_attempted:
        raise RuntimeError("system picker confirmation did not return to the app")
    if selection_attempted:
        raise RuntimeError("system picker did not return the selected PNG to the app")
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
    parser.add_argument("--timeout-seconds", type=float, default=120)
    arguments = parser.parse_args()
    adb_path = resolve_adb(arguments.adb)
    if not arguments.flutter:
        raise SystemExit("flutter is required")
    apk = arguments.apk.resolve()
    if not apk.is_file():
        raise SystemExit(f"APK does not exist: {apk}")
    mobile_root = Path(__file__).resolve().parents[1]

    print("harness-stage|install-and-clear", flush=True)
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

    print("harness-stage|picker-import", flush=True)
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
    except BaseException as error:
        process.kill()
        output, _ = process.communicate()
        detail = str(error).splitlines()[0] if str(error) else type(error).__name__
        raise RuntimeError(
            f"media picker coordination failed: {detail}; {diagnostic_tail(output)}"
        ) from error
    if process.returncode != 0:
        raise RuntimeError(
            "media picker Flutter drive failed "
            f"with exit code {process.returncode}: {diagnostic_tail(output)}"
        )

    print("harness-stage|verify-redacted-import-log", flush=True)
    log = wait_for_log(
        adb_path,
        arguments.device,
        "owned_original_media_imported class=image",
        arguments.timeout_seconds,
    )
    if PICKER_FILE in log or "content://" in log or "file://" in log:
        raise RuntimeError("picker filename or URI leaked into runtime logs")

    print("harness-stage|force-stop", flush=True)
    adb(adb_path, arguments.device, "shell", "am", "force-stop", PACKAGE)
    print("harness-stage|typed-catalog-recovery", flush=True)
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
    try:
        raise SystemExit(main())
    except Exception as error:
        first_line = str(error).splitlines()[0] if str(error) else type(error).__name__
        annotation = (
            first_line.replace("%", "%25")
            .replace("\r", "%0D")
            .replace("\n", "%0A")
        )
        print(
            f"::error title=MOB-07 Android OwnedOriginal media harness::{annotation}",
            flush=True,
        )
        raise
