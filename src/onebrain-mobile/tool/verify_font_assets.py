#!/usr/bin/env python3
"""Verify bundled font and license bytes against the pinned asset manifest."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "assets" / "font_asset_manifest_v1.json"


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def main() -> None:
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    failures: list[str] = []
    checked: list[dict[str, object]] = []

    for asset in manifest["assets"]:
        for path_key, hash_key, size_key in (
            ("asset", "sha256", "bytes"),
            ("license_asset", "license_sha256", None),
        ):
            relative = asset[path_key]
            path = ROOT / relative
            if not path.is_file():
                failures.append(f"{relative}: file is missing")
                continue
            actual_hash = _sha256(path)
            if actual_hash != asset[hash_key]:
                failures.append(
                    f"{relative}: sha256 {actual_hash} != {asset[hash_key]}"
                )
            if size_key is not None and path.stat().st_size != asset[size_key]:
                failures.append(
                    f"{relative}: bytes {path.stat().st_size} != {asset[size_key]}"
                )
        checked.append(
            {
                "family": asset["family"],
                "asset": asset["asset"],
                "source_commit": asset["source_commit"],
            }
        )

    report = {
        "format": manifest["format"],
        "manifest": str(MANIFEST.relative_to(ROOT)).replace("\\", "/"),
        "checked": checked,
        "failures": failures,
    }
    print(json.dumps(report, indent=2))
    if failures:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
