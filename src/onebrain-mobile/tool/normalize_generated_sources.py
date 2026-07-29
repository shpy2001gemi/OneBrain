#!/usr/bin/env python3
"""Normalize generator whitespace without changing generated semantics."""

from __future__ import annotations

from pathlib import Path


GENERATED_FILES = (
    Path("lib/design/generated/mobile_design_tokens.g.dart"),
    Path("lib/platform/generated/mobile_host_api.g.dart"),
    Path(
        "android/app/src/main/kotlin/org/onebrain/onebrain_mobile/"
        "generated/MobileHostApi.g.kt"
    ),
    Path("ios/Runner/Generated/MobileHostApi.g.swift"),
)


def main() -> int:
    for path in GENERATED_FILES:
        text = path.read_text(encoding="utf-8")
        lines = [line.rstrip() for line in text.splitlines()]
        while lines and not lines[-1]:
            lines.pop()
        normalized = "\n".join(lines)
        path.write_text(normalized + "\n", encoding="utf-8", newline="\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
