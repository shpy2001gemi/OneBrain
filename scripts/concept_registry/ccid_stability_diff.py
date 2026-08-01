#!/usr/bin/env python3
"""Generate a bounded, machine-readable CCID stability report.

The report joins stable source identities from the exact old/new builder input
JSONL files with the CCIDs actually stored in each OBR. SQLite keeps memory
bounded for production-scale registries while preserving exact comparisons.
"""

from __future__ import annotations

import argparse
import json
import os
import sqlite3
import struct
import sys
import tempfile
from pathlib import Path
from typing import Any, BinaryIO

import blake3

from config import OBR_MAGIC, OBR_VERSION


PROFILE = "onebrain/ccid-stability-diff/1"
ALGORITHM = "actual-obr-ccid-by-stable-source-identity-v1"
HEADER = struct.Struct("<4sIQQ8s")
ENTRY_PREFIX = struct.Struct("<16sIBBH")
U16 = struct.Struct("<H")
SOURCE_NAMES = {
    0: "wikidata",
    1: "geonames",
    2: "ncbi",
    3: "chebi",
    4: "wordnet",
}
MAX_MANIFEST_BYTES = 1024 * 1024
MAX_INPUT_LINE_BYTES = 1024 * 1024
MAX_STRING_ID_BYTES = 4096
MAX_SAMPLE_LIMIT = 1000


class StabilityError(RuntimeError):
    """An input artifact cannot participate in a trustworthy comparison."""


def _blake3_file(path: Path) -> str:
    digest = blake3.blake3()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def _read_exact(handle: BinaryIO, length: int, label: str) -> bytes:
    value = handle.read(length)
    if len(value) != length:
        raise StabilityError(f"truncated OBR while reading {label}")
    return value


def _skip_exact(handle: BinaryIO, length: int, label: str) -> None:
    remaining = length
    while remaining:
        chunk = handle.read(min(remaining, 64 * 1024))
        if not chunk:
            raise StabilityError(f"truncated OBR while reading {label}")
        remaining -= len(chunk)


def _stable_identity(ext_id: object) -> tuple[str, int]:
    if isinstance(ext_id, bool) or not isinstance(ext_id, (int, str)):
        raise StabilityError("ext_id must be an integer or string")
    if isinstance(ext_id, int):
        if not 0 <= ext_id <= 0xFFFFFFFF:
            raise StabilityError("integer ext_id is outside the OBR u32 range")
        stored = ext_id
        kind = "int"
    else:
        try:
            encoded_id = ext_id.encode("utf-8")
        except UnicodeEncodeError as error:
            raise StabilityError("string ext_id is not valid UTF-8") from error
        if len(encoded_id) > MAX_STRING_ID_BYTES:
            raise StabilityError(
                f"string ext_id exceeds {MAX_STRING_ID_BYTES} UTF-8 bytes"
            )
        stored = int.from_bytes(
            blake3.blake3(encoded_id).digest(length=4), "little"
        )
        kind = "str"
    identity = json.dumps(
        {"type": kind, "value": ext_id},
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    )
    return identity, stored


def _read_manifest(path: Path, obr_path: Path) -> dict[str, Any]:
    if path.stat().st_size > MAX_MANIFEST_BYTES:
        raise StabilityError(f"manifest is too large: {path}")
    try:
        manifest = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise StabilityError(f"invalid manifest {path}: {error}") from error
    if not isinstance(manifest, dict):
        raise StabilityError(f"manifest is not an object: {path}")
    actual_obr_hash = _blake3_file(obr_path)
    if manifest.get("obr_blake3") != actual_obr_hash:
        raise StabilityError(f"manifest/OBR checksum mismatch: {obr_path}")
    entry_count = manifest.get("entry_count")
    if (
        isinstance(entry_count, bool)
        or not isinstance(entry_count, int)
        or entry_count < 0
    ):
        raise StabilityError(f"manifest entry_count is invalid: {path}")
    return manifest


def _create_schema(connection: sqlite3.Connection) -> None:
    for side in ("old", "new"):
        connection.execute(
            f"""
            CREATE TABLE {side}_records (
                source INTEGER NOT NULL,
                identity_json TEXT NOT NULL,
                ccid BLOB NOT NULL CHECK(length(ccid) = 16),
                PRIMARY KEY(source, identity_json)
            ) WITHOUT ROWID
            """
        )
        connection.execute(
            f"CREATE INDEX {side}_ccid ON {side}_records(ccid)"
        )


def _ingest_pair(
    connection: sqlite3.Connection,
    side: str,
    input_path: Path,
    obr_path: Path,
    expected_entries: int,
) -> int:
    inserted = 0
    with input_path.open("rb") as source, obr_path.open("rb") as obr:
        magic, version, entry_count, _label_count, reserved = HEADER.unpack(
            _read_exact(obr, HEADER.size, "header")
        )
        if magic != OBR_MAGIC or version != OBR_VERSION or reserved != b"\x00" * 8:
            raise StabilityError(f"unsupported or corrupt OBR header: {obr_path}")
        if entry_count != expected_entries:
            raise StabilityError(
                f"OBR/manifest entry count mismatch for {side}: "
                f"{entry_count} != {expected_entries}"
            )

        line_number = 0
        while raw_line := source.readline(MAX_INPUT_LINE_BYTES + 1):
            line_number += 1
            if len(raw_line) > MAX_INPUT_LINE_BYTES:
                raise StabilityError(
                    f"{side} input line {line_number} exceeds "
                    f"{MAX_INPUT_LINE_BYTES} bytes"
                )
            try:
                line = raw_line.decode("utf-8")
            except UnicodeDecodeError as error:
                raise StabilityError(
                    f"invalid UTF-8 in {side} input at line {line_number}"
                ) from error
            if not line.strip():
                continue
            try:
                record = json.loads(line)
            except json.JSONDecodeError as error:
                raise StabilityError(
                    f"invalid {side} input JSON at line {line_number}: {error}"
                ) from error
            if not isinstance(record, dict):
                raise StabilityError(
                    f"invalid {side} input record at line {line_number}"
                )
            source_id = record.get("source")
            if (
                isinstance(source_id, bool)
                or not isinstance(source_id, int)
                or source_id not in SOURCE_NAMES
            ):
                raise StabilityError(
                    f"unknown {side} source at line {line_number}: {source_id!r}"
                )
            identity_json, expected_stored_id = _stable_identity(record.get("ext_id"))
            ccid, stored_id, obr_source, _category, name_length = ENTRY_PREFIX.unpack(
                _read_exact(obr, ENTRY_PREFIX.size, f"entry {inserted} prefix")
            )
            if obr_source != source_id or stored_id != expected_stored_id:
                raise StabilityError(
                    f"{side} input/OBR identity mismatch at line {line_number}"
                )
            _skip_exact(obr, name_length, f"entry {inserted} name")
            label_count = U16.unpack(_read_exact(obr, U16.size, "label count"))[0]
            for label_index in range(label_count):
                length = U16.unpack(
                    _read_exact(obr, U16.size, f"label {label_index} length")
                )[0]
                _skip_exact(obr, length, f"label {label_index}")
            try:
                connection.execute(
                    f"INSERT INTO {side}_records(source, identity_json, ccid) "
                    "VALUES (?, ?, ?)",
                    (source_id, identity_json, ccid),
                )
            except sqlite3.IntegrityError as error:
                raise StabilityError(
                    f"duplicate stable identity in {side} input at line {line_number}"
                ) from error
            inserted += 1

        if inserted != entry_count:
            raise StabilityError(
                f"{side} input/OBR entry count mismatch: {inserted} != {entry_count}"
            )
        if obr.read(1):
            raise StabilityError(f"OBR has trailing bytes: {obr_path}")
    connection.commit()
    return inserted


def _scalar(connection: sqlite3.Connection, statement: str) -> int:
    row = connection.execute(statement).fetchone()
    return int(row[0])


def _identity_sample(
    connection: sqlite3.Connection, statement: str, limit: int
) -> list[dict[str, object]]:
    rows = connection.execute(statement, (limit,)).fetchall()
    return [
        {
            "source": SOURCE_NAMES[int(source)],
            "identity": json.loads(identity_json),
            "old_ccid": old_ccid.lower() if old_ccid is not None else None,
            "new_ccid": new_ccid.lower() if new_ccid is not None else None,
        }
        for source, identity_json, old_ccid, new_ccid in rows
    ]


def _collision_sample(
    connection: sqlite3.Connection, side: str, limit: int
) -> list[dict[str, object]]:
    rows = connection.execute(
        f"""
        SELECT lower(hex(ccid)), count(*)
        FROM {side}_records
        GROUP BY ccid
        HAVING count(*) > 1
        ORDER BY ccid
        LIMIT ?
        """,
        (limit,),
    ).fetchall()
    return [{"ccid": ccid, "identity_count": int(count)} for ccid, count in rows]


def generate_report(
    old_input: Path,
    old_obr: Path,
    old_manifest_path: Path,
    new_input: Path,
    new_obr: Path,
    new_manifest_path: Path,
    sample_limit: int = 20,
    work_dir: Path | None = None,
) -> dict[str, object]:
    if not 0 <= sample_limit <= MAX_SAMPLE_LIMIT:
        raise StabilityError(f"sample_limit must be between 0 and {MAX_SAMPLE_LIMIT}")
    old_manifest = _read_manifest(old_manifest_path, old_obr)
    new_manifest = _read_manifest(new_manifest_path, new_obr)

    if work_dir is not None:
        work_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(
        prefix="onebrain-ccid-diff-", dir=work_dir
    ) as directory:
        database_path = Path(directory) / "ccid-diff.sqlite3"
        connection = sqlite3.connect(database_path)
        try:
            connection.execute("PRAGMA journal_mode=OFF")
            connection.execute("PRAGMA synchronous=OFF")
            connection.execute("PRAGMA temp_store=FILE")
            _create_schema(connection)
            old_count = _ingest_pair(
                connection,
                "old",
                old_input,
                old_obr,
                int(old_manifest["entry_count"]),
            )
            new_count = _ingest_pair(
                connection,
                "new",
                new_input,
                new_obr,
                int(new_manifest["entry_count"]),
            )
            stable_count = _scalar(
                connection,
                """
                SELECT count(*) FROM old_records old
                JOIN new_records new USING(source, identity_json)
                """,
            )
            changed_count = _scalar(
                connection,
                """
                SELECT count(*) FROM old_records old
                JOIN new_records new USING(source, identity_json)
                WHERE old.ccid != new.ccid
                """,
            )
            old_only_count = _scalar(
                connection,
                """
                SELECT count(*) FROM old_records old
                LEFT JOIN new_records new USING(source, identity_json)
                WHERE new.identity_json IS NULL
                """,
            )
            new_only_count = _scalar(
                connection,
                """
                SELECT count(*) FROM new_records new
                LEFT JOIN old_records old USING(source, identity_json)
                WHERE old.identity_json IS NULL
                """,
            )
            old_collision_count = _scalar(
                connection,
                "SELECT count(*) FROM (SELECT ccid FROM old_records GROUP BY ccid HAVING count(*) > 1)",
            )
            new_collision_count = _scalar(
                connection,
                "SELECT count(*) FROM (SELECT ccid FROM new_records GROUP BY ccid HAVING count(*) > 1)",
            )
            changed_sample = _identity_sample(
                connection,
                """
                SELECT old.source, old.identity_json,
                       lower(hex(old.ccid)), lower(hex(new.ccid))
                FROM old_records old
                JOIN new_records new USING(source, identity_json)
                WHERE old.ccid != new.ccid
                ORDER BY old.source, old.identity_json
                LIMIT ?
                """,
                sample_limit,
            )
            old_only_sample = _identity_sample(
                connection,
                """
                SELECT old.source, old.identity_json, lower(hex(old.ccid)), NULL
                FROM old_records old
                LEFT JOIN new_records new USING(source, identity_json)
                WHERE new.identity_json IS NULL
                ORDER BY old.source, old.identity_json
                LIMIT ?
                """,
                sample_limit,
            )
            new_only_sample = _identity_sample(
                connection,
                """
                SELECT new.source, new.identity_json, NULL, lower(hex(new.ccid))
                FROM new_records new
                LEFT JOIN old_records old USING(source, identity_json)
                WHERE old.identity_json IS NULL
                ORDER BY new.source, new.identity_json
                LIMIT ?
                """,
                sample_limit,
            )
            old_collision_sample = _collision_sample(
                connection, "old", sample_limit
            )
            new_collision_sample = _collision_sample(
                connection, "new", sample_limit
            )
        finally:
            connection.close()

    qualified = (
        stable_count > 0
        and changed_count == 0
        and old_collision_count == 0
        and new_collision_count == 0
    )

    def artifact(
        input_path: Path,
        obr_path: Path,
        manifest_path: Path,
        manifest: dict[str, Any],
        count: int,
    ) -> dict[str, object]:
        sources = manifest.get("sources")
        if not isinstance(sources, dict):
            raise StabilityError(f"manifest sources are invalid: {manifest_path}")
        return {
            "input_path": str(input_path),
            "input_blake3": _blake3_file(input_path),
            "obr_path": str(obr_path),
            "obr_blake3": manifest["obr_blake3"],
            "manifest_path": str(manifest_path),
            "manifest_blake3": _blake3_file(manifest_path),
            "entry_count": count,
            "builder_version": manifest.get("builder_version"),
            "dedup_policy_version": manifest.get("dedup_policy_version"),
            "source_snapshots": {
                name: source.get("snapshot_id")
                for name, source in sorted(sources.items())
                if isinstance(source, dict)
            },
        }
    return {
        "profile": PROFILE,
        "algorithm": ALGORITHM,
        "old": artifact(
            old_input, old_obr, old_manifest_path, old_manifest, old_count
        ),
        "new": artifact(
            new_input, new_obr, new_manifest_path, new_manifest, new_count
        ),
        "comparison": {
            "stable_identity_count": stable_count,
            "stable_identity_changed_ccid_count": changed_count,
            "old_only_identity_count": old_only_count,
            "new_only_identity_count": new_only_count,
            "old_ccid_collision_count": old_collision_count,
            "new_ccid_collision_count": new_collision_count,
            "changed_sample": changed_sample,
            "old_only_sample": old_only_sample,
            "new_only_sample": new_only_sample,
            "old_collision_sample": old_collision_sample,
            "new_collision_sample": new_collision_sample,
        },
        "exit_oracles": {
            "has_stable_source_identity_overlap": stable_count > 0,
            "all_stable_source_identities_keep_ccid": changed_count == 0,
            "old_release_has_no_ccid_collision": old_collision_count == 0,
            "new_release_has_no_ccid_collision": new_collision_count == 0,
        },
        "qualified": qualified,
    }


def _write_report(path: Path, report: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary_name = tempfile.mkstemp(
        prefix=f".{path.name}.", suffix=".tmp", dir=path.parent
    )
    temporary_path = Path(temporary_name)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as handle:
            json.dump(report, handle, ensure_ascii=False, indent=2, sort_keys=True)
            handle.write("\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary_path, path)
        if os.name != "nt":
            directory_fd = os.open(path.parent, os.O_RDONLY)
            try:
                os.fsync(directory_fd)
            finally:
                os.close(directory_fd)
    finally:
        temporary_path.unlink(missing_ok=True)


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--old-input", type=Path, required=True)
    parser.add_argument("--old-obr", type=Path, required=True)
    parser.add_argument("--old-manifest", type=Path, required=True)
    parser.add_argument("--new-input", type=Path, required=True)
    parser.add_argument("--new-obr", type=Path, required=True)
    parser.add_argument("--new-manifest", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--sample-limit", type=int, default=20)
    parser.add_argument(
        "--work-dir",
        type=Path,
        help="Directory for the bounded SQLite join (defaults to system temp)",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    try:
        report = generate_report(
            args.old_input,
            args.old_obr,
            args.old_manifest,
            args.new_input,
            args.new_obr,
            args.new_manifest,
            args.sample_limit,
            args.work_dir,
        )
        _write_report(args.output, report)
    except (OSError, sqlite3.Error, StabilityError) as error:
        print(f"CCID stability report failed: {error}", file=sys.stderr)
        return 2
    print(json.dumps(report, ensure_ascii=False, sort_keys=True))
    return 0 if report["qualified"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
