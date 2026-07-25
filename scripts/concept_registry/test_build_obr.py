"""Focused tests for the OBR artifact and provenance manifest builder."""

import json
import sys
import tempfile
import unittest
from pathlib import Path

import blake3

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from build_obr import build
from config import SOURCE_WIKIDATA
from index_existing_obr import build_indexes


class BuildObrManifestTests(unittest.TestCase):
    def test_build_emits_checksum_counts_and_all_required_sources(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            merged = root / "merged"
            merged.mkdir()
            input_path = merged / "concepts_deduped.jsonl"
            output_path = root / "concepts.obr"
            record = {
                "source": SOURCE_WIKIDATA,
                "ext_id": 42,
                "category": 5,
                "name": "Douglas Adams",
                "canonical_form": "wd:Q42",
                "labels": {"en": "Douglas Adams"},
            }
            input_path.write_text(json.dumps(record) + "\n", encoding="utf-8")

            stats = build(input_path, output_path)
            manifest_path = Path(f"{output_path}.manifest.json")
            label_index_path = Path(f"{output_path}.labels.idx")
            ccid_index_path = Path(f"{output_path}.ccids.idx")
            verification_path = Path(f"{output_path}.verification.json")
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))

            self.assertEqual(stats["entries"], 1)
            self.assertEqual(manifest["entry_count"], 1)
            self.assertEqual(manifest["label_count"], 1)
            self.assertEqual(
                manifest["obr_blake3"],
                blake3.blake3(output_path.read_bytes()).hexdigest(),
            )
            self.assertEqual(
                set(manifest["sources"]),
                {"wikidata", "wordnet", "geonames", "ncbi", "chebi"},
            )
            self.assertEqual(manifest["label_index"]["record_count"], 1)
            self.assertEqual(manifest["ccid_index"]["record_count"], 1)
            self.assertEqual(
                manifest["label_index"]["file_size"], label_index_path.stat().st_size
            )
            self.assertEqual(
                manifest["ccid_index"]["file_size"], ccid_index_path.stat().st_size
            )
            verification = json.loads(verification_path.read_text(encoding="utf-8"))
            self.assertEqual(verification["obr_blake3"], manifest["obr_blake3"])
            self.assertEqual(
                verification["label_index"]["blake3"],
                manifest["label_index"]["blake3"],
            )
            self.assertEqual(
                verification["ccid_index"]["blake3"],
                manifest["ccid_index"]["blake3"],
            )

    def test_existing_obr_indexer_is_non_destructive(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            merged = root / "merged"
            merged.mkdir()
            input_path = merged / "concepts_deduped.jsonl"
            output_path = root / "concepts.obr"
            input_path.write_text(
                json.dumps(
                    {
                        "source": SOURCE_WIKIDATA,
                        "ext_id": 283,
                        "category": 7,
                        "name": "water",
                        "canonical_form": "wd:Q283",
                        "labels": {"vi": "nước"},
                    }
                )
                + "\n",
                encoding="utf-8",
            )
            build(input_path, output_path)
            original = output_path.read_bytes()
            for suffix in (
                ".labels.idx",
                ".ccids.idx",
                ".manifest.json",
                ".verification.json",
            ):
                Path(f"{output_path}{suffix}").unlink()

            stats = build_indexes(output_path, input_path)

            self.assertEqual(output_path.read_bytes(), original)
            self.assertEqual(stats["entries"], 1)
            self.assertEqual(stats["label_index"]["record_count"], 2)
            self.assertEqual(stats["ccid_index"]["record_count"], 1)


if __name__ == "__main__":
    unittest.main()
