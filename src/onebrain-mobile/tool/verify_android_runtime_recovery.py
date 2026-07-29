#!/usr/bin/env python3
"""Exercise MOB-04 secure process-death recovery on an Android emulator."""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import time
from datetime import datetime, timezone
from pathlib import Path


PACKAGE = "org.onebrain.onebrain_mobile"
ACTIVITY = f"{PACKAGE}/.MainActivity"
LOG_TAG = "OneBrainMobileRuntime"
RUNTIME_PATTERN = re.compile(
    r"profile=(?P<profile>\S+) "
    r"generation=(?P<generation>\d+) "
    r"phase=(?P<phase>\S+) "
    r"grants=(?P<grants>\d+) "
    r"recovered=(?P<recovered>true|false) "
    r"bootstrap=(?P<bootstrap>true|false) "
    r"registry=(?P<registry>\S+) "
    r"kql=(?P<kql>true|false) "
    r"planner=(?P<planner>true|false) "
    r"noLlm=(?P<no_llm>true|false) "
    r"staleFence=(?P<stale_fence>true|false) "
    r"secure=(?P<secure>true|false) "
    r"binding=(?P<binding>true|false) "
    r"unlocked=(?P<unlocked>true|false) "
    r"vault=(?P<vault>true|false) "
    r"domains=(?P<domains>true|false) "
    r"privacy=(?P<privacy>true|false) "
    r"history=(?P<history>true|false) "
    r"drafts=(?P<drafts>\d+) "
    r"onboarding=(?P<onboarding>\d+)"
)


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


def read_runtime_log(adb: str, device: str) -> dict[str, object] | None:
    output = run(adb, device, "logcat", "-d", "-s", f"{LOG_TAG}:I", "*:S")
    matches = list(RUNTIME_PATTERN.finditer(output))
    if not matches:
        return None
    values = matches[-1].groupdict()
    return {
        "profile": values["profile"],
        "generation": int(values["generation"]),
        "phase": values["phase"],
        "active_grants": int(values["grants"]),
        "recovered_unclean_start": values["recovered"] == "true",
        "bootstrap_store_opened": values["bootstrap"] == "true",
        "registry_state": values["registry"],
        "local_kql_fixture_verified": values["kql"] == "true",
        "private_planner_verified": values["planner"] == "true",
        "no_llm_provider": values["no_llm"] == "true",
        "stale_callback_rejected": values["stale_fence"] == "true",
        "secure_profile_active": values["secure"] == "true",
        "installation_binding_verified": values["binding"] == "true",
        "security_session_unlocked": values["unlocked"] == "true",
        "private_vault_ready": values["vault"] == "true",
        "identity_domains_separated": values["domains"] == "true",
        "privacy_defaults_fail_safe": values["privacy"] == "true",
        "redacted_history_ready": values["history"] == "true",
        "encrypted_raw_draft_count": int(values["drafts"]),
        "onboarding_cursor": int(values["onboarding"]),
    }


def wait_for_runtime(adb: str, device: str, timeout_seconds: float) -> dict[str, object]:
    deadline = time.monotonic() + timeout_seconds
    while time.monotonic() < deadline:
        snapshot = read_runtime_log(adb, device)
        if snapshot is not None:
            return snapshot
        time.sleep(0.5)
    raise RuntimeError(f"no {LOG_TAG} snapshot within {timeout_seconds:g} seconds")


def assert_common(snapshot: dict[str, object]) -> None:
    expected = {
        "profile": "MOB-04/1",
        "phase": "Active",
        "active_grants": 1,
        "bootstrap_store_opened": True,
        "registry_state": "BootstrapOnly",
        "local_kql_fixture_verified": True,
        "private_planner_verified": True,
        "no_llm_provider": True,
        "stale_callback_rejected": True,
        "secure_profile_active": True,
        "installation_binding_verified": True,
        "security_session_unlocked": True,
        "private_vault_ready": True,
        "identity_domains_separated": True,
        "privacy_defaults_fail_safe": True,
        "redacted_history_ready": True,
        "onboarding_cursor": 0,
    }
    mismatches = {
        key: {"expected": value, "actual": snapshot.get(key)}
        for key, value in expected.items()
        if snapshot.get(key) != value
    }
    if mismatches:
        raise RuntimeError(f"runtime snapshot mismatch: {mismatches}")


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
    first = wait_for_runtime(
        arguments.adb, arguments.device, arguments.timeout_seconds
    )
    assert_common(first)
    if first["generation"] != 1 or first["recovered_unclean_start"]:
        raise RuntimeError(f"fresh launch was not generation 1: {first}")

    run(arguments.adb, arguments.device, "shell", "am", "force-stop", PACKAGE)
    run(arguments.adb, arguments.device, "logcat", "-c")
    run(arguments.adb, arguments.device, "shell", "am", "start", "-W", "-n", ACTIVITY)
    recovered = wait_for_runtime(
        arguments.adb, arguments.device, arguments.timeout_seconds
    )
    assert_common(recovered)
    if recovered["generation"] != 2 or not recovered["recovered_unclean_start"]:
        raise RuntimeError(f"force-stop recovery was not generation 2: {recovered}")

    report = {
        "format": "onebrain.mobile.android-runtime-recovery/1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "device": arguments.device,
        "package": PACKAGE,
        "first_launch": first,
        "after_force_stop": recovered,
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
