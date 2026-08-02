#!/usr/bin/env python3
"""Generate the public MOB-05A signed fixture's three exact role artifacts."""

from __future__ import annotations

import argparse
from pathlib import Path


ARTIFACTS = {
    "concepts.obr": (0x41, 1024),
    "concepts.obr.labels.idx": (0x42, 2048),
    "concepts.obr.ccids.idx": (0x43, 3072),
}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    for name, (value, length) in ARTIFACTS.items():
        (args.output / name).write_bytes(bytes([value]) * length)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
