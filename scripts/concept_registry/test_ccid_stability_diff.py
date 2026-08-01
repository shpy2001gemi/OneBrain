"""Tests for the production-scale CCID stability report."""

from __future__ import annotations

import json
import io
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path

import blake3

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from build_obr import build
from ccid_stability_diff import (
    ENTRY_PREFIX,
    HEADER,
    PROFILE,
    U16,
    StabilityError,
    generate_report,
    main,
)
from config import SOURCE_NCBI, SOURCE_WIKIDATA


def _record(source: int, ext_id: int | str, name: str) -> dict[str, object]:
    return {
        "source": source,
        "ext_id": ext_id,
        "category": 7,
        "name": name,
        "canonical_form": f"{source}:{ext_id}",
        "labels": {"en": name},
    }


class CcidStabilityDiffTests(unittest.TestCase):
    def _build(
        self, root: Path, name: str, records: list[dict[str, object]]
    ) -> tuple[Path, Path, Path]:
        directory = root / name
        merged = directory / "merged"
        merged.mkdir(parents=True)
        input_path = merged / "concepts_deduped.jsonl"
        input_path.write_text(
            "".join(json.dumps(record) + "\n" for record in records),
            encoding="utf-8",
        )
        obr_path = directory / "concepts.obr"
        build(input_path, obr_path)
        return input_path, obr_path, Path(f"{obr_path}.manifest.json")

    def _rehash_manifest(self, obr_path: Path, manifest_path: Path) -> None:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        manifest["obr_blake3"] = blake3.blake3(obr_path.read_bytes()).hexdigest()
        manifest_path.write_text(
            json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )

    def _ccid_offsets(self, obr_path: Path) -> list[int]:
        value = obr_path.read_bytes()
        _magic, _version, entry_count, _label_count, _reserved = HEADER.unpack_from(
            value
        )
        offsets: list[int] = []
        offset = HEADER.size
        for _ in range(entry_count):
            offsets.append(offset)
            _ccid, _stored_id, _source, _category, name_length = (
                ENTRY_PREFIX.unpack_from(value, offset)
            )
            offset += ENTRY_PREFIX.size + name_length
            label_count = U16.unpack_from(value, offset)[0]
            offset += U16.size
            for _ in range(label_count):
                length = U16.unpack_from(value, offset)[0]
                offset += U16.size + length
        self.assertEqual(offset, len(value))
        return offsets

    def test_stable_numeric_and_string_identities_keep_actual_obr_ccids(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            old = self._build(
                root,
                "old",
                [
                    _record(SOURCE_WIKIDATA, 42, "Douglas Adams"),
                    _record(SOURCE_NCBI, "taxon-alpha", "Taxon alpha"),
                ],
            )
            new = self._build(
                root,
                "new",
                [
                    _record(SOURCE_NCBI, "taxon-alpha", "Renamed taxon alpha"),
                    _record(SOURCE_WIKIDATA, 42, "Douglas Adams"),
                    _record(SOURCE_WIKIDATA, 283, "Water"),
                ],
            )

            report = generate_report(*old, *new, sample_limit=10)
            comparison = report["comparison"]
            self.assertEqual(report["profile"], PROFILE)
            self.assertTrue(report["qualified"])
            self.assertEqual(comparison["stable_identity_count"], 2)
            self.assertEqual(comparison["stable_identity_changed_ccid_count"], 0)
            self.assertEqual(comparison["old_only_identity_count"], 0)
            self.assertEqual(comparison["new_only_identity_count"], 1)

    def test_changed_actual_obr_ccid_is_a_failing_diff(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            records = [_record(SOURCE_WIKIDATA, 42, "Douglas Adams")]
            old = self._build(root, "old", records)
            new = self._build(root, "new", records)
            bytes_value = bytearray(new[1].read_bytes())
            bytes_value[32] ^= 0x01
            new[1].write_bytes(bytes_value)
            self._rehash_manifest(new[1], new[2])

            report = generate_report(*old, *new)
            self.assertFalse(report["qualified"])
            self.assertEqual(
                report["comparison"]["stable_identity_changed_ccid_count"], 1
            )
            self.assertEqual(len(report["comparison"]["changed_sample"]), 1)

    def test_no_stable_identity_overlap_is_not_qualified(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            old = self._build(
                root, "old", [_record(SOURCE_WIKIDATA, 42, "Douglas Adams")]
            )
            new = self._build(
                root, "new", [_record(SOURCE_WIKIDATA, 283, "Water")]
            )

            report = generate_report(*old, *new)
            self.assertFalse(report["qualified"])
            self.assertFalse(
                report["exit_oracles"]["has_stable_source_identity_overlap"]
            )

    def test_actual_obr_ccid_collision_is_a_failing_diff(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            records = [
                _record(SOURCE_WIKIDATA, 42, "Douglas Adams"),
                _record(SOURCE_WIKIDATA, 283, "Water"),
            ]
            old = self._build(root, "old", records)
            new = self._build(root, "new", records)
            value = bytearray(new[1].read_bytes())
            first, second = self._ccid_offsets(new[1])
            value[second : second + 16] = value[first : first + 16]
            new[1].write_bytes(value)
            self._rehash_manifest(new[1], new[2])

            report = generate_report(*old, *new)
            self.assertFalse(report["qualified"])
            self.assertEqual(report["comparison"]["new_ccid_collision_count"], 1)
            self.assertEqual(len(report["comparison"]["new_collision_sample"]), 1)

    def test_input_obr_identity_mismatch_fails_explicitly(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            old = self._build(
                root, "old", [_record(SOURCE_WIKIDATA, 42, "Douglas Adams")]
            )
            new = self._build(
                root, "new", [_record(SOURCE_WIKIDATA, 42, "Douglas Adams")]
            )
            new[0].write_text(
                json.dumps(_record(SOURCE_WIKIDATA, 43, "Douglas Adams")) + "\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(StabilityError, "identity mismatch"):
                generate_report(*old, *new)

    def test_incomplete_manifest_source_snapshots_fail_explicitly(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            records = [_record(SOURCE_WIKIDATA, 42, "Douglas Adams")]
            old = self._build(root, "old", records)
            new = self._build(root, "new", records)
            manifest = json.loads(new[2].read_text(encoding="utf-8"))
            manifest["sources"].pop("wordnet")
            new[2].write_text(
                json.dumps(manifest, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )

            with self.assertRaisesRegex(StabilityError, "sources are invalid"):
                generate_report(*old, *new)

    def test_truncated_obr_fails_before_report_qualification(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            old = self._build(
                root, "old", [_record(SOURCE_WIKIDATA, 42, "Douglas Adams")]
            )
            new = self._build(
                root, "new", [_record(SOURCE_WIKIDATA, 42, "Douglas Adams")]
            )
            new[1].write_bytes(new[1].read_bytes()[:-2])
            self._rehash_manifest(new[1], new[2])
            with self.assertRaisesRegex(StabilityError, "truncated OBR"):
                generate_report(*old, *new)

    def test_cli_writes_report_and_returns_gate_status(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            records = [_record(SOURCE_WIKIDATA, 42, "Douglas Adams")]
            old = self._build(root, "old", records)
            new = self._build(root, "new", records)
            output = root / "evidence" / "ccid-diff.json"
            work_dir = root / "work"
            arguments = [
                "--old-input",
                str(old[0]),
                "--old-obr",
                str(old[1]),
                "--old-manifest",
                str(old[2]),
                "--new-input",
                str(new[0]),
                "--new-obr",
                str(new[1]),
                "--new-manifest",
                str(new[2]),
                "--output",
                str(output),
                "--work-dir",
                str(work_dir),
            ]
            with redirect_stdout(io.StringIO()):
                self.assertEqual(main(arguments), 0)
            self.assertTrue(json.loads(output.read_text(encoding="utf-8"))["qualified"])
            self.assertEqual(list(work_dir.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
