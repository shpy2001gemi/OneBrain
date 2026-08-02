#!/usr/bin/env python3
"""Exercise the MOB-05B native Registry chunk stream across process death."""

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


def resolve_adb(explicit: str | None) -> str:
    if explicit:
        return explicit
    discovered = shutil.which("adb")
    if discovered:
        return discovered
    local_app_data = os.environ.get("LOCALAPPDATA")
    if local_app_data:
        candidate = Path(local_app_data) / "Android" / "Sdk" / "platform-tools" / "adb.exe"
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
    return completed.stdout.strip()


def send_probe(adb_path: str, device: str, mode: str) -> None:
    adb(adb_path, device, "logcat", "-c")
    adb(
        adb_path,
        device,
        "shell",
        "am",
        "broadcast",
        "-f",
        "0x20",
        "-n",
        PROBE,
        "-a",
        ACTION,
        "--es",
        "mode",
        mode,
    )


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
            raise RuntimeError(f"Registry chunk probe failed: {value}")
        time.sleep(0.25)
    raise TimeoutError(f"Registry chunk probe did not reach {expected_status}")


def wait_for_process_exit(
    adb_path: str,
    device: str,
    timeout_seconds: float,
) -> None:
    deadline = time.monotonic() + timeout_seconds
    while time.monotonic() < deadline:
        if not process_id(adb_path, device):
            return
        time.sleep(0.25)
    raise TimeoutError("OneBrain process remained alive after force-stop")


def process_id(adb_path: str, device: str) -> str:
    completed = subprocess.run(
        [adb_path, "-s", device, "shell", "pidof", PACKAGE],
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=90,
    )
    return completed.stdout.strip()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("apk", type=Path)
    parser.add_argument("--device", default="emulator-5554")
    parser.add_argument("--adb")
    parser.add_argument("--timeout-seconds", type=float, default=30.0)
    parser.add_argument("--report", type=Path)
    arguments = parser.parse_args()
    adb_path = resolve_adb(arguments.adb)
    apk = arguments.apk.resolve()
    if not apk.is_file():
        raise SystemExit(f"APK not found: {apk}")

    subprocess.run(
        [adb_path, "-s", arguments.device, "uninstall", PACKAGE],
        check=False,
        capture_output=True,
        text=True,
        timeout=90,
    )
    adb(adb_path, arguments.device, "install", "-t", str(apk))
    adb(adb_path, arguments.device, "shell", "dumpsys", "battery", "unplug")
    adb(adb_path, arguments.device, "shell", "dumpsys", "battery", "set", "status", "3")
    try:
        adb(adb_path, arguments.device, "shell", "am", "start", "-W", "-n", ACTIVITY)
        send_probe(adb_path, arguments.device, "land_partial")
        partial = wait_for_probe(
            adb_path,
            arguments.device,
            "CHUNK_PARTIAL_DURABLE",
            arguments.timeout_seconds,
        )
        if int(partial["written_bytes"]) != 300 or int(partial["durable_bytes"]) != 300:
            raise AssertionError("The native partial was not durably checkpointed at 300 bytes")
        if int(partial["chunk_state"]) != 1:
            raise AssertionError("The partial chunk is not in Receiving state")
        transfer_nonce = str(partial["transfer_nonce"])

        pid_before = process_id(adb_path, arguments.device)
        if not pid_before:
            raise AssertionError("The OneBrain process was not alive before the kill")
        adb(adb_path, arguments.device, "shell", "input", "keyevent", "KEYCODE_HOME")
        adb(adb_path, arguments.device, "shell", "am", "force-stop", PACKAGE)
        wait_for_process_exit(adb_path, arguments.device, arguments.timeout_seconds)

        send_probe(adb_path, arguments.device, "land_resume")
        complete = wait_for_probe(
            adb_path,
            arguments.device,
            "CHUNKS_VERIFIED",
            arguments.timeout_seconds,
        )
        pid_after = process_id(adb_path, arguments.device)
        if not pid_after or pid_after == pid_before:
            raise AssertionError("The Registry resume did not start a fresh process")
        if str(complete["transfer_nonce"]) != transfer_nonce:
            raise AssertionError("Resume rebound to a different transfer nonce")
        if int(complete["verified_chunks"]) != 3:
            raise AssertionError("Not every manifest-derived chunk was verified")
        if int(complete["expected_bytes"]) != 6_144:
            raise AssertionError("The signed fixture byte total changed")
        if int(complete["verified_bytes"]) != 6_144 or complete["bytes_complete"] is not True:
            raise AssertionError("Rust did not close the BytesComplete barrier")

        reported_partial = dict(partial)
        reported_partial.pop("transfer_nonce", None)
        reported_partial["transfer_nonce_prefix"] = transfer_nonce[:26]
        reported_complete = dict(complete)
        reported_complete.pop("transfer_nonce", None)
        reported_complete["transfer_nonce_prefix"] = transfer_nonce[:26]
        report = {
            "format": "onebrain.mobile.android-registry-native-chunk-stream/1",
            "device": arguments.device,
            "result": "passed",
            "abi_revision": 11,
            "process_before": pid_before,
            "process_after": pid_after,
            "partial": reported_partial,
            "complete": reported_complete,
            "limitations": [
                "the 6 KiB public debug fixture is not production Registry data",
                "bytes are generated by a debug-only probe and do not prove HTTPS or URLSession transport",
                "the debug probe cancels the OS job after checkpoint without changing the adopted Rust ledger, then force-stops the package and uses an explicit include-stopped native broadcast to isolate process-death ABI recovery",
                "this isolated callback gate does not replace the separate UIDT task-inventory reconciliation evidence and is not Task Manager Stop evidence",
                "this does not qualify full-size target-filesystem, power-loss, Doze, thermal, reboot or physical-device behavior",
                "the release build still embeds no trust profile, descriptor, Registry bytes or INTERNET permission",
            ],
        }
        rendered = json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True)
        if arguments.report:
            arguments.report.parent.mkdir(parents=True, exist_ok=True)
            arguments.report.write_text(rendered + "\n", encoding="utf-8")
        print(rendered)
    finally:
        adb(adb_path, arguments.device, "shell", "dumpsys", "battery", "reset")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
