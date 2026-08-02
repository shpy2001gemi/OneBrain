#!/usr/bin/env python3
"""Audit the Android UIDT/native-stream boundary before network transport."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


FORBIDDEN_BOOTSTRAP_PERMISSIONS = {
    "android.permission.ACCESS_WIFI_STATE",
    "android.permission.BLUETOOTH_ADVERTISE",
    "android.permission.BLUETOOTH_CONNECT",
    "android.permission.BLUETOOTH_SCAN",
    "android.permission.CHANGE_NETWORK_STATE",
    "android.permission.CHANGE_WIFI_MULTICAST_STATE",
    "android.permission.CHANGE_WIFI_STATE",
    "android.permission.INTERNET",
    "android.permission.NEARBY_WIFI_DEVICES",
}
REQUIRED_UIDT_PERMISSIONS = {
    "android.permission.ACCESS_NETWORK_STATE",
    "android.permission.RECEIVE_BOOT_COMPLETED",
    "android.permission.RUN_USER_INITIATED_JOBS",
}
UIDT_SERVICE = "org.onebrain.onebrain_mobile.RegistryTransferJobService"
UIDT_CONTROL_RECEIVER = (
    "org.onebrain.onebrain_mobile.RegistryTransferControlReceiver"
)
DEBUG_PROBE_RECEIVER = "org.onebrain.onebrain_mobile.RegistryUidtProbeReceiver"


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _find_aapt() -> Path:
    sdk_value = os.environ.get("ANDROID_HOME") or os.environ.get(
        "ANDROID_SDK_ROOT"
    )
    if not sdk_value and os.name == "nt":
        local_app_data = os.environ.get("LOCALAPPDATA")
        if local_app_data:
            default_sdk = Path(local_app_data) / "Android" / "Sdk"
            if default_sdk.is_dir():
                sdk_value = str(default_sdk)
    if not sdk_value:
        raise RuntimeError(
            "Android SDK not found via ANDROID_HOME, ANDROID_SDK_ROOT, "
            "or the Windows user SDK location"
        )
    build_tools = Path(sdk_value) / "build-tools"
    executable = "aapt.exe" if os.name == "nt" else "aapt"
    candidates = sorted(
        build_tools.glob(f"*/{executable}"),
        key=lambda path: path.parent.name,
        reverse=True,
    )
    if not candidates:
        raise RuntimeError(f"{executable} was not found under {build_tools}")
    return candidates[0]


def inspect(apk: Path) -> dict[str, object]:
    aapt = _find_aapt()
    result = subprocess.run(
        [str(aapt), "dump", "permissions", str(apk)],
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    permissions = sorted(
        set(
            re.findall(
                r"uses-permission(?:-sdk-\d+)?: name='([^']+)'",
                result.stdout,
            )
        )
    )
    forbidden = sorted(
        set(permissions).intersection(FORBIDDEN_BOOTSTRAP_PERMISSIONS)
    )
    manifest = subprocess.run(
        [str(aapt), "dump", "xmltree", str(apk), "AndroidManifest.xml"],
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    ).stdout
    missing_uidt = sorted(REQUIRED_UIDT_PERMISSIONS.difference(permissions))
    return {
        "format": "onebrain.mobile.android-permissions/1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "scope": "MOB-05B Android UIDT and native verified-stream boundary before network response execution",
        "package": apk.name,
        "package_bytes": apk.stat().st_size,
        "package_sha256": _sha256(apk),
        "aapt": aapt.name,
        "android_build_tools": aapt.parent.name,
        "permissions": permissions,
        "forbidden_bootstrap_network_permissions": forbidden,
        "required_uidt_permissions": sorted(REQUIRED_UIDT_PERMISSIONS),
        "missing_uidt_permissions": missing_uidt,
        "internet_permission_present": "android.permission.INTERNET" in permissions,
        "network_capability_present": "android.permission.INTERNET" in permissions,
        "uidt_service_declared": UIDT_SERVICE in manifest,
        "uidt_control_receiver_declared": UIDT_CONTROL_RECEIVER in manifest,
        "debug_probe_receiver_declared": DEBUG_PROBE_RECEIVER in manifest,
        "limitations": (
            "ACCESS_NETWORK_STATE permits JobScheduler constraint evaluation; "
            "it does not open a socket. This proves the release declares the "
            "UIDT scheduler permissions but still lacks INTERNET. It does not "
            "replace HTTPS landing, packet, full-size, or physical-device tests."
        ),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("apk", type=Path)
    parser.add_argument("--report", type=Path)
    arguments = parser.parse_args()
    report = inspect(arguments.apk.resolve())
    rendered = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if arguments.report:
        arguments.report.parent.mkdir(parents=True, exist_ok=True)
        arguments.report.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
    if (
        report["network_capability_present"]
        or report["missing_uidt_permissions"]
        or not report["uidt_service_declared"]
        or not report["uidt_control_receiver_declared"]
        or report["debug_probe_receiver_declared"]
    ):
        print("Android UIDT permission boundary: FAIL", file=sys.stderr)
        return 1
    print("Android UIDT permission boundary: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
