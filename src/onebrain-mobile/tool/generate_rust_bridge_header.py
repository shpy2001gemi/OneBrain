#!/usr/bin/env python3
"""Generate the checked-in iOS C header from the Rust mobile bridge."""

from __future__ import annotations

import shutil
import subprocess
from pathlib import Path


MOBILE_ROOT = Path(__file__).resolve().parents[1]
RUST_CRATE = MOBILE_ROOT.parent / "onebrain-mobile-bridge"
OUTPUT = RUST_CRATE / "include" / "onebrain_mobile_bridge.h"


def main() -> int:
    executable = shutil.which("cbindgen")
    if executable is None:
        raise SystemExit(
            "cbindgen 0.29.4 is required; run "
            "`cargo install cbindgen --version 0.29.4 --locked`"
        )
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            executable,
            "--config",
            str(RUST_CRATE / "cbindgen.toml"),
            "--crate",
            "onebrain-mobile-bridge",
            "--output",
            str(OUTPUT),
        ],
        cwd=RUST_CRATE.parent,
        check=True,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
