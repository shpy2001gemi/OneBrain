#!/usr/bin/env python3
"""Record an adb-provided debug cold-start baseline without custom timers."""

from __future__ import annotations

import argparse
import json
import re
import statistics
import subprocess
from datetime import datetime, timezone
from pathlib import Path


FIELD = re.compile(r"^(Status|Activity|TotalTime|WaitTime):\s*(.+)$")


def adb(*arguments: str) -> str:
    result = subprocess.run(
        ("adb", *arguments),
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    return result.stdout


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--component",
        default="org.onebrain.onebrain_mobile/.MainActivity",
    )
    parser.add_argument("--samples", type=int, default=5)
    parser.add_argument("--output", required=True, type=Path)
    arguments = parser.parse_args()
    if arguments.samples not in range(1, 21):
        raise SystemExit("--samples must be between 1 and 20")

    package = arguments.component.split("/", maxsplit=1)[0]
    samples: list[dict[str, object]] = []
    for index in range(arguments.samples):
        adb("shell", "am", "force-stop", package)
        output = adb(
            "shell",
            "am",
            "start",
            "-W",
            "-S",
            "-n",
            arguments.component,
        )
        values = {
            match.group(1): match.group(2)
            for line in output.splitlines()
            if (match := FIELD.match(line.strip()))
        }
        if values.get("Status") != "ok":
            raise SystemExit(f"cold-start sample {index + 1} failed: {output}")
        samples.append(
            {
                "sample": index + 1,
                "activity": values.get("Activity"),
                "total_time_ms": int(values["TotalTime"]),
                "wait_time_ms": int(values["WaitTime"]),
            }
        )

    total_times = [int(sample["total_time_ms"]) for sample in samples]
    report = {
        "format": "onebrain.mobile.android-startup-baseline/1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "build": "debug",
        "device": adb("shell", "getprop", "ro.product.model").strip(),
        "android_release": adb(
            "shell", "getprop", "ro.build.version.release"
        ).strip(),
        "component": arguments.component,
        "sample_count": len(samples),
        "total_time_ms": {
            "minimum": min(total_times),
            "median": statistics.median(total_times),
            "maximum": max(total_times),
        },
        "samples": samples,
        "limits": (
            "Debug emulator feasibility baseline only; not a release SLO or "
            "physical-device launch claim."
        ),
    }
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
