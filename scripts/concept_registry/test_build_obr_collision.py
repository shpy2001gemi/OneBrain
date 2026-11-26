"""Unit tests for P1 §2.2 128-bit ConceptCcid collision quarantine in build_obr.py."""

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build_obr  # noqa: E402
from config import CATEGORY_ENTITY, CATEGORY_SUBSTANCE, SOURCE_CHEBI, SOURCE_WIKIDATA  # noqa: E402


class TestBuildObrCollisionQuarantine(unittest.TestCase):
    def _write_jsonl(self, path: Path, rows: list[dict]) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        with open(path, "w", encoding="utf-8") as fh:
            for row in rows:
                fh.write(json.dumps(row, ensure_ascii=False) + "\n")

    def test_distinct_origin_ccid_collision_writes_quarantine_and_exits_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            input_jsonl = root / "merged" / "concepts_deduped.jsonl"
            release_dir = root / "release"
            obr_path = release_dir / "concepts.obr"
            diag_dir = root / "diagnostics"

            self._write_jsonl(
                input_jsonl,
                [
                    {
                        "source": SOURCE_WIKIDATA,
                        "ext_id": 283,
                        "category": CATEGORY_SUBSTANCE,
                        "name": "water",
                        "labels": {"en": "water", "vi": "nước"},
                        "canonical_form": "wd:Q283",
                    },
                    {
                        "source": SOURCE_CHEBI,
                        "ext_id": 15377,
                        "category": CATEGORY_SUBSTANCE,
                        "name": "oxidane",
                        "labels": {"en": "oxidane"},
                        "canonical_form": "chebi:15377",
                    },
                ],
            )

            forced_ccid = bytes.fromhex("0123456789abcdef0123456789abcdef")

            with self.assertRaises(SystemExit) as ctx:
                build_obr.build(
                    input_jsonl,
                    obr_path,
                    diagnostics_dir=diag_dir,
                    ccid_fn=lambda _src, _ext: forced_ccid,
                )

            self.assertEqual(ctx.exception.code, 1)

            quarantine_file = diag_dir / "collision_quarantine.json"
            self.assertTrue(quarantine_file.is_file())
            report = json.loads(quarantine_file.read_text(encoding="utf-8"))
            self.assertEqual(report["profile"], build_obr.COLLISION_QUARANTINE_PROFILE)
            self.assertEqual(report["collision_count"], 1)
            collision = report["collisions"][0]
            self.assertEqual(collision["ccid_hex"], forced_ccid.hex())
            self.assertEqual(collision["existing"]["source_namespace"], "wikidata")
            self.assertEqual(collision["existing"]["source_identifier"], "283")
            self.assertEqual(collision["quarantined"]["source_namespace"], "chebi")
            self.assertEqual(collision["quarantined"]["source_identifier"], "15377")

            # Release directory must remain untouched (no partial .obr or quarantine file inside it).
            self.assertFalse(obr_path.exists())
            self.assertFalse((release_dir / "collision_quarantine.json").exists())

    def test_diagnostics_dir_inside_release_dir_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            input_jsonl = root / "merged" / "concepts_deduped.jsonl"
            release_dir = root / "release"
            obr_path = release_dir / "concepts.obr"
            self._write_jsonl(
                input_jsonl,
                [
                    {
                        "source": SOURCE_WIKIDATA,
                        "ext_id": 42,
                        "category": CATEGORY_ENTITY,
                        "name": "copper",
                        "labels": {"en": "copper"},
                    }
                ],
            )
            with self.assertRaises(ValueError):
                build_obr.build(input_jsonl, obr_path, diagnostics_dir=release_dir)
            with self.assertRaises(ValueError):
                build_obr.build(
                    input_jsonl, obr_path, diagnostics_dir=release_dir / "nested"
                )

    def test_same_origin_duplicate_and_distinct_non_colliding_build_succeeds(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            input_jsonl = root / "merged" / "concepts_deduped.jsonl"
            release_dir = root / "release"
            obr_path = release_dir / "concepts.obr"
            diag_dir = root / "diagnostics"

            self._write_jsonl(
                input_jsonl,
                [
                    {
                        "source": SOURCE_WIKIDATA,
                        "ext_id": 283,
                        "category": CATEGORY_SUBSTANCE,
                        "name": "water",
                        "labels": {"en": "water"},
                    },
                    {
                        "source": SOURCE_WIKIDATA,
                        "ext_id": 283,
                        "category": CATEGORY_SUBSTANCE,
                        "name": "water",
                        "labels": {"vi": "nước"},
                    },
                    {
                        "source": SOURCE_WIKIDATA,
                        "ext_id": 753,
                        "category": CATEGORY_SUBSTANCE,
                        "name": "copper",
                        "labels": {"en": "copper"},
                    },
                ],
            )

            stats = build_obr.build(input_jsonl, obr_path, diagnostics_dir=diag_dir)
            self.assertEqual(stats["entries"], 3)
            self.assertEqual(stats["collisions"], 0)
            self.assertTrue(obr_path.is_file())
            self.assertTrue(Path(f"{obr_path}.labels.idx").is_file())
            self.assertTrue(Path(f"{obr_path}.ccids.idx").is_file())
            self.assertTrue(Path(f"{obr_path}.manifest.json").is_file())
            self.assertFalse((diag_dir / "collision_quarantine.json").exists())
            self.assertFalse((release_dir / "collision_quarantine.json").exists())


if __name__ == "__main__":
    unittest.main()
