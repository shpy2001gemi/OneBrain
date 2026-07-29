#!/usr/bin/env python3
"""Fail closed when a bootstrap-only Android release can open a network."""

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
    "android.permission.ACCESS_NETWORK_STATE",
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
    if not sdk_value:
        raise RuntimeError("ANDROID_HOME or ANDROID_SDK_ROOT is required")
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
    return {
        "format": "onebrain.mobile.android-permissions/1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "scope": "MOB-01 BootstrapOnly Android release",
        "package": apk.name,
        "package_bytes": apk.stat().st_size,
        "package_sha256": _sha256(apk),
        "aapt": aapt.name,
        "android_build_tools": aapt.parent.name,
        "permissions": permissions,
        "forbidden_bootstrap_network_permissions": forbidden,
        "network_capability_present": bool(forbidden),
        "limitations": (
            "This proves the packaged Android release lacks OS network "
            "permissions. It does not replace later explicit-Init transport "
            "tests or physical-device packet capture."
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
    if report["network_capability_present"]:
        print("bootstrap Android network capability: FAIL", file=sys.stderr)
        return 1
    print("bootstrap Android network capability: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
