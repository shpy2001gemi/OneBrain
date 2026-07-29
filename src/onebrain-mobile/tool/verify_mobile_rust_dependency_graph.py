#!/usr/bin/env python3
"""Reject desktop, transport and concrete LLM crates from the mobile core."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path


MOBILE_ROOT = Path(__file__).resolve().parents[1]
RUST_WORKSPACE = MOBILE_ROOT.parent
FORBIDDEN = {
    "ku-ai",
    "ku-net",
    "ollama",
    "onebrain-node",
    "onebrain-protocol",
    "reqwest",
    "tokio",
}
PACKAGE_LINE = re.compile(r"^(?P<name>[A-Za-z0-9_-]+) v[0-9]")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--report", type=Path)
    arguments = parser.parse_args()
    completed = subprocess.run(
        [
            "cargo",
            "tree",
            "-p",
            "onebrain-mobile-core",
            "--edges",
            "normal",
            "--prefix",
            "none",
            "--format",
            "{p}",
        ],
        cwd=RUST_WORKSPACE,
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    packages = sorted(
        {
            match.group("name")
            for line in completed.stdout.splitlines()
            if (match := PACKAGE_LINE.match(line))
        }
    )
    violations = sorted(FORBIDDEN.intersection(packages))
    report = {
        "format": "onebrain.mobile.rust-dependency-isolation/1",
        "root_package": "onebrain-mobile-core",
        "package_count": len(packages),
        "forbidden_packages": sorted(FORBIDDEN),
        "violations": violations,
        "result": "failed" if violations else "passed",
    }
    rendered = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if arguments.report:
        arguments.report.parent.mkdir(parents=True, exist_ok=True)
        arguments.report.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
    return 1 if violations else 0


if __name__ == "__main__":
    raise SystemExit(main())
