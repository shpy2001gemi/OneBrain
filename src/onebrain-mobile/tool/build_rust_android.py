#!/usr/bin/env python3
"""Cross-compile the Rust bridge into Android's standard jniLibs layout."""

from __future__ import annotations

import argparse
import shutil
import subprocess
from pathlib import Path


MOBILE_ROOT = Path(__file__).resolve().parents[1]
RUST_WORKSPACE = MOBILE_ROOT.parent
OUTPUT = MOBILE_ROOT / "android" / "app" / "src" / "main" / "jniLibs"
TARGETS = ("arm64-v8a", "x86_64")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--release", action="store_true")
    parser.add_argument(
        "--target",
        action="append",
        choices=TARGETS,
        help="Android ABI to build; defaults to arm64-v8a and x86_64.",
    )
    arguments = parser.parse_args()

    if shutil.which("cargo-ndk") is None:
        raise SystemExit(
            "cargo-ndk 4.1.2 is required; run "
            "`cargo install cargo-ndk --version 4.1.2 --locked`"
        )

    selected_targets = arguments.target or list(TARGETS)
    command = ["cargo", "ndk", "--platform", "24"]
    for target in selected_targets:
        command.extend(("-t", target))
    command.extend(
        (
            "-o",
            str(OUTPUT),
            "build",
            "--manifest-path",
            str(RUST_WORKSPACE / "Cargo.toml"),
            "-p",
            "onebrain-mobile-bridge",
        )
    )
    if arguments.release:
        command.append("--release")
    subprocess.run(command, cwd=RUST_WORKSPACE, check=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
