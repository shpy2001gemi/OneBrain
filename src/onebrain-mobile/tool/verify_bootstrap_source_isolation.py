#!/usr/bin/env python3
"""Prove the MOB-01 bootstrap slice has no mobile transport implementation."""

from __future__ import annotations

import argparse
import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BRIDGE_ROOT = ROOT.parent / "onebrain-mobile-bridge"
CORE_ROOT = ROOT.parent / "onebrain-mobile-core"
RULES = {
    "dart": (
        r"""import\s+['"]dart:io['"]""",
        r"""import\s+['"]package:(?:http|dio|web_socket_channel|grpc)/""",
        r"\b(?:HttpClient|WebSocket|RawSocket|Socket)\s*\(",
    ),
    "kotlin": (
        r"\bjava\.net\.",
        r"\bokhttp3\.",
        r"\bio\.ktor\.client\.",
        r"\b(?:HttpURLConnection|Socket|ServerSocket)\s*\(",
    ),
    "swift": (
        r"\b(?:URLSession|URLRequest|URLProtocol|NWConnection|NWListener)\b",
        r"^\s*import\s+Network\s*$",
    ),
    "rust": (
        r"\bstd::net::",
        r"\btokio::net::",
        r"\b(?:reqwest|quinn|libp2p)::",
    ),
}


def _sources() -> list[tuple[str, Path]]:
    sources: list[tuple[str, Path]] = []
    sources.extend(("dart", path) for path in (ROOT / "lib").rglob("*.dart"))
    sources.extend(
        ("kotlin", path)
        for path in (ROOT / "android" / "app" / "src" / "main").rglob("*.kt")
    )
    sources.extend(
        ("swift", path) for path in (ROOT / "ios" / "Runner").rglob("*.swift")
    )
    sources.extend(("rust", path) for path in (BRIDGE_ROOT / "src").rglob("*.rs"))
    sources.extend(("rust", path) for path in (CORE_ROOT / "src").rglob("*.rs"))
    return sorted(sources, key=lambda item: item[1].as_posix())


def verify() -> dict[str, object]:
    violations: list[dict[str, object]] = []
    counts = {language: 0 for language in RULES}
    for language, path in _sources():
        counts[language] += 1
        text = path.read_text(encoding="utf-8")
        for pattern in RULES[language]:
            match = re.search(pattern, text, flags=re.MULTILINE)
            if match:
                line = text.count("\n", 0, match.start()) + 1
                violations.append(
                    {
                        "language": language,
                        "path": path.relative_to(ROOT.parent.parent).as_posix(),
                        "line": line,
                        "pattern": pattern,
                    }
                )
    return {
        "format": "onebrain.mobile.bootstrap-source-isolation/1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "scope": "MOB-04 BootstrapOnly app, Rust bridge and private Limited shell",
        "files_scanned": counts,
        "forbidden_transport_reference_count": len(violations),
        "violations": violations,
        "limitations": (
            "Static transport isolation complements the Android release "
            "permission proof. Later explicit Registry Init must replace this "
            "foundation gate with consent-bound transport and packet tests."
        ),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--report", type=Path)
    arguments = parser.parse_args()
    report = verify()
    rendered = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if arguments.report:
        arguments.report.parent.mkdir(parents=True, exist_ok=True)
        arguments.report.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
    if report["violations"]:
        print("bootstrap source isolation: FAIL", file=sys.stderr)
        return 1
    print("bootstrap source isolation: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
