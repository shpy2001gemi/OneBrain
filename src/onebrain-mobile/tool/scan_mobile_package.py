#!/usr/bin/env python3
"""Fail closed when a mobile archive contains prohibited data payloads."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import zipfile
from datetime import datetime, timezone
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[3]
CONTRACT_PATH = (
    REPOSITORY_ROOT / "docs/design/mobile/mobile_build_contract_v1.json"
)
KNOWN_FLUTTER_CODE_ASSETS = {
    "assets/flutter_assets/kernel_blob.bin",
    "assets/flutter_assets/isolate_snapshot_data",
    "assets/flutter_assets/vm_snapshot_data",
}


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def scan_package(package: Path) -> dict[str, object]:
    contract = json.loads(CONTRACT_PATH.read_text(encoding="utf-8"))
    guards = contract["source_guards"]
    forbidden_names = {
        str(value).lower() for value in guards["forbidden_packaged_names"]
    }
    forbidden_suffixes = tuple(
        str(value).lower() for value in guards["forbidden_packaged_suffixes"]
    )
    maximum_asset_bytes = int(guards["maximum_unlisted_asset_bytes"])
    violations: list[str] = []
    entries: list[dict[str, object]] = []
    android_runtime_abis: set[str] = set()
    android_bridge_abis: set[str] = set()

    with zipfile.ZipFile(package) as archive:
        for info in archive.infolist():
            if info.is_dir():
                continue
            normalized = info.filename.replace("\\", "/").lower()
            basename = normalized.rsplit("/", 1)[-1]
            entries.append(
                {
                    "path": info.filename,
                    "uncompressed_bytes": info.file_size,
                    "compressed_bytes": info.compress_size,
                }
            )
            path_parts = normalized.split("/")
            if (
                len(path_parts) == 3
                and path_parts[0] == "lib"
                and path_parts[2] in {"libflutter.so", "libapp.so"}
            ):
                android_runtime_abis.add(path_parts[1])
            if (
                len(path_parts) == 3
                and path_parts[0] == "lib"
                and path_parts[2] == "libonebrain_mobile_bridge.so"
            ):
                android_bridge_abis.add(path_parts[1])
            if basename in forbidden_names or normalized.endswith(
                forbidden_suffixes
            ):
                violations.append(f"FORBIDDEN_PAYLOAD:{info.filename}")
            if (
                "concepts.obr" in normalized
                or "/registry/chunks/" in normalized
                or "/registry/releases/" in normalized
            ):
                violations.append(f"REGISTRY_PAYLOAD:{info.filename}")
            if (
                normalized.startswith("assets/")
                and normalized not in KNOWN_FLUTTER_CODE_ASSETS
                and info.file_size > maximum_asset_bytes
            ):
                violations.append(
                    f"UNLISTED_LARGE_ASSET:{info.filename}:{info.file_size}"
                )

    for abi in sorted(android_runtime_abis - android_bridge_abis):
        violations.append(f"MISSING_RUST_BRIDGE_FOR_ANDROID_ABI:{abi}")
    for abi in sorted(android_bridge_abis - android_runtime_abis):
        violations.append(f"UNBOUND_RUST_BRIDGE_ANDROID_ABI:{abi}")

    largest = sorted(
        entries,
        key=lambda entry: int(entry["uncompressed_bytes"]),
        reverse=True,
    )[:10]
    return {
        "format": "onebrain.mobile.package-inventory/1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "package": package.name,
        "package_bytes": package.stat().st_size,
        "package_sha256": _sha256(package),
        "entry_count": len(entries),
        "android_runtime_abis": sorted(android_runtime_abis),
        "android_rust_bridge_abis": sorted(android_bridge_abis),
        "forbidden_payload_count": len(violations),
        "violations": sorted(set(violations)),
        "largest_entries": largest,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("package", type=Path)
    parser.add_argument("--report", type=Path)
    arguments = parser.parse_args()
    package = arguments.package.resolve()
    report = scan_package(package)
    rendered = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if arguments.report:
        arguments.report.parent.mkdir(parents=True, exist_ok=True)
        arguments.report.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
    if report["violations"]:
        print("mobile package inventory: FAIL", file=sys.stderr)
        return 1
    print("mobile package inventory: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
