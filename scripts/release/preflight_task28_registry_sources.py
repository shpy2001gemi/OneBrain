#!/usr/bin/env python3
"""Fail closed before staging the external Task 28 Registry source set.

The checkpoint directory is source material.  ``onebrain_data`` is processed
output, and the merged JSONL is the canonical builder input.  This tool binds
those three classes without copying them into the read-only candidate.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

import blake3


MIN_PRODUCTION_OBR_BYTES = 2_200_000_000
MAX_PRODUCTION_OBR_BYTES = 2_500_000_000
PROCESSED_FILES = (
    "concepts.obr",
    "concepts.obr.labels.idx",
    "concepts.obr.ccids.idx",
    "concepts.obr.manifest.json",
    "concepts.obr.verification.json",
)
SOURCE_NAMES = {"chebi", "geonames", "ncbi", "wikidata", "wordnet"}


class Task28RegistrySourceError(RuntimeError):
    """The external Registry source/output set is incomplete or inconsistent."""


def _canonical(value: object) -> bytes:
    return json.dumps(
        value, ensure_ascii=True, sort_keys=True, separators=(",", ":")
    ).encode("utf-8")


def _regular(path: Path, label: str) -> Path:
    try:
        resolved = path.resolve(strict=True)
    except OSError as error:
        raise Task28RegistrySourceError(f"{label} is unavailable") from error
    if path.is_symlink() or not resolved.is_file():
        raise Task28RegistrySourceError(f"{label} must be a regular no-follow file")
    return resolved


def _directory(path: Path, label: str) -> Path:
    try:
        resolved = path.resolve(strict=True)
    except OSError as error:
        raise Task28RegistrySourceError(f"{label} is unavailable") from error
    if path.is_symlink() or not resolved.is_dir():
        raise Task28RegistrySourceError(f"{label} must be a real directory")
    return resolved


def _digest(path: Path) -> str:
    value = blake3.blake3()
    with path.open("rb") as stream:
        while block := stream.read(8 * 1024 * 1024):
            value.update(block)
    return value.hexdigest()


def _json(path: Path, label: str) -> dict[str, object]:
    try:
        value = json.loads(path.read_bytes())
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise Task28RegistrySourceError(f"{label} is invalid JSON") from error
    if not isinstance(value, dict):
        raise Task28RegistrySourceError(f"{label} must be an object")
    return value


def _outside_candidate(path: Path, candidate_root: Path | None, label: str) -> None:
    if candidate_root is None:
        return
    candidate = _directory(candidate_root, "candidate root")
    if path == candidate or path.is_relative_to(candidate):
        raise Task28RegistrySourceError(f"{label} must remain outside the candidate")


def inspect_registry_sources(
    *,
    processed_root: Path,
    checkpoint_root: Path,
    canonical_input: Path,
    candidate_root: Path | None = None,
    min_obr_bytes: int = MIN_PRODUCTION_OBR_BYTES,
    max_obr_bytes: int = MAX_PRODUCTION_OBR_BYTES,
) -> dict[str, object]:
    processed = _directory(processed_root, "processed Registry root")
    checkpoints = _directory(checkpoint_root, "Registry checkpoint root")
    input_path = _regular(canonical_input, "canonical merged Registry input")
    _outside_candidate(processed, candidate_root, "processed Registry root")
    _outside_candidate(checkpoints, candidate_root, "Registry checkpoint root")
    _outside_candidate(input_path, candidate_root, "canonical merged Registry input")

    processed_paths = {
        name: _regular(processed / name, f"processed Registry artifact {name}")
        for name in PROCESSED_FILES
    }
    obr_size = processed_paths["concepts.obr"].stat().st_size
    if not min_obr_bytes <= obr_size <= max_obr_bytes:
        raise Task28RegistrySourceError(
            "processed concepts.obr is outside the frozen production interval: "
            f"{obr_size} bytes (required {min_obr_bytes}..{max_obr_bytes})"
        )

    manifest = _json(processed_paths["concepts.obr.manifest.json"], "OBR manifest")
    verification = _json(
        processed_paths["concepts.obr.verification.json"], "OBR verification receipt"
    )
    if (
        manifest.get("manifest_version") != 1
        or manifest.get("builder_version") != "onebrain-concept-registry-builder/1"
        or not isinstance(manifest.get("entry_count"), int)
        or manifest["entry_count"] <= 0
        or set(manifest.get("sources", {})) != SOURCE_NAMES
    ):
        raise Task28RegistrySourceError("OBR manifest identity/source set is not frozen")

    measured = {
        name: {
            "blake3": _digest(path),
            "size": path.stat().st_size,
        }
        for name, path in processed_paths.items()
    }
    if (
        manifest.get("obr_blake3") != measured["concepts.obr"]["blake3"]
        or verification.get("obr_blake3") != measured["concepts.obr"]["blake3"]
        or verification.get("file_size") != obr_size
    ):
        raise Task28RegistrySourceError("OBR bytes differ from manifest/verification")
    for filename, field in (
        ("concepts.obr.labels.idx", "label_index"),
        ("concepts.obr.ccids.idx", "ccid_index"),
    ):
        manifest_row = manifest.get(field)
        verification_row = verification.get(field)
        if (
            not isinstance(manifest_row, dict)
            or not isinstance(verification_row, dict)
            or manifest_row.get("blake3") != measured[filename]["blake3"]
            or verification_row.get("blake3") != measured[filename]["blake3"]
            or manifest_row.get("file_size") != measured[filename]["size"]
            or verification_row.get("file_size") != measured[filename]["size"]
        ):
            raise Task28RegistrySourceError(f"{filename} differs from manifest/verification")

    checkpoint_paths = sorted(
        (path for path in checkpoints.iterdir() if path.is_file() and not path.is_symlink()),
        key=lambda path: path.name.encode("utf-8"),
    )
    names = {path.name for path in checkpoint_paths}
    required = {"allCountries.zip", "compounds.sql.zip", "names.sql.zip", "taxdump.tar.gz"}
    if not required <= names or not any(
        name.startswith("wikidata-") and name.endswith("-all.json.gz") for name in names
    ):
        raise Task28RegistrySourceError("checkpoint source archive set is incomplete")
    checkpoint_rows = [
        {"name": path.name, "size": path.stat().st_size, "blake3": _digest(path)}
        for path in checkpoint_paths
    ]
    input_row = {
        "path": str(input_path),
        "size": input_path.stat().st_size,
        "blake3": _digest(input_path),
        "source_kind": "canonical-processed-input",
    }
    report = {
        "format": "onebrain/task28-registry-source-preflight/1",
        "production_ready": True,
        "candidate_root": str(candidate_root.resolve()) if candidate_root else None,
        "source_checkpoint_root": str(checkpoints),
        "processed_output_root": str(processed),
        "canonical_input": input_row,
        "limits": {
            "minimum_obr_bytes": min_obr_bytes,
            "maximum_obr_bytes": max_obr_bytes,
        },
        "processed_outputs": [
            {"name": name, **measured[name], "source_kind": "processed-output"}
            for name in PROCESSED_FILES
        ],
        "source_checkpoints": [
            {**row, "source_kind": "checkpoint-source"} for row in checkpoint_rows
        ],
        "manifest_entry_count": manifest["entry_count"],
    }
    report["source_set_blake3"] = blake3.blake3(
        _canonical(
            {
                "canonical_input": input_row,
                "processed_outputs": report["processed_outputs"],
                "source_checkpoints": report["source_checkpoints"],
            }
        )
    ).hexdigest()
    return report


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--processed-root", type=Path, required=True)
    parser.add_argument("--checkpoint-root", type=Path, required=True)
    parser.add_argument("--canonical-input", type=Path, required=True)
    parser.add_argument("--candidate-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        report = inspect_registry_sources(
            processed_root=args.processed_root,
            checkpoint_root=args.checkpoint_root,
            canonical_input=args.canonical_input,
            candidate_root=args.candidate_root,
        )
        args.output.parent.mkdir(parents=True, exist_ok=True)
        with args.output.open("xb") as stream:
            stream.write(_canonical(report) + b"\n")
            stream.flush()
            os.fsync(stream.fileno())
    except (OSError, Task28RegistrySourceError) as error:
        print(f"Task 28 Registry source preflight failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(report, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
