from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

import blake3

from scripts.release.preflight_task28_registry_sources import (
    Task28RegistrySourceError,
    inspect_registry_sources,
)


class Task28RegistrySourcePreflightTests(unittest.TestCase):
    def _fixture(self, root: Path) -> tuple[Path, Path, Path]:
        processed = root / "onebrain_data"
        checkpoints = root / "scripts/concept_registry/checkpoints"
        merged = root / "scripts/concept_registry/merged/concepts_deduped.jsonl"
        processed.mkdir(parents=True)
        checkpoints.mkdir(parents=True)
        merged.parent.mkdir(parents=True)
        obr = b"production-obR"
        labels = b"labels"
        ccids = b"ccids"
        merged.write_bytes(b'{"concept":"x"}\n')
        (processed / "concepts.obr").write_bytes(obr)
        (processed / "concepts.obr.labels.idx").write_bytes(labels)
        (processed / "concepts.obr.ccids.idx").write_bytes(ccids)
        manifest = {
            "manifest_version": 1,
            "builder_version": "onebrain-concept-registry-builder/1",
            "entry_count": 1,
            "sources": {name: {} for name in ("chebi", "geonames", "ncbi", "wikidata", "wordnet")},
            "obr_blake3": blake3.blake3(obr).hexdigest(),
            "label_index": {"blake3": blake3.blake3(labels).hexdigest(), "file_size": len(labels)},
            "ccid_index": {"blake3": blake3.blake3(ccids).hexdigest(), "file_size": len(ccids)},
        }
        verification = {
            "file_size": len(obr),
            "obr_blake3": manifest["obr_blake3"],
            "label_index": manifest["label_index"],
            "ccid_index": manifest["ccid_index"],
        }
        (processed / "concepts.obr.manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
        (processed / "concepts.obr.verification.json").write_text(json.dumps(verification), encoding="utf-8")
        for name in (
            "allCountries.zip",
            "compounds.sql.zip",
            "names.sql.zip",
            "taxdump.tar.gz",
            "wikidata-20260713-all.json.gz",
        ):
            (checkpoints / name).write_bytes(name.encode())
        return processed, checkpoints, merged

    def test_source_and_processed_output_roles_are_bound_separately(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            processed, checkpoints, merged = self._fixture(root)
            report = inspect_registry_sources(
                processed_root=processed,
                checkpoint_root=checkpoints,
                canonical_input=merged,
                min_obr_bytes=1,
                max_obr_bytes=100,
            )
            self.assertTrue(report["production_ready"])
            self.assertEqual(
                {row["source_kind"] for row in report["source_checkpoints"]},
                {"checkpoint-source"},
            )
            self.assertEqual(
                {row["source_kind"] for row in report["processed_outputs"]},
                {"processed-output"},
            )
            self.assertEqual(len(report["source_set_blake3"]), 64)

    def test_current_size_class_is_rejected_before_qualification(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            processed, checkpoints, merged = self._fixture(root)
            with self.assertRaisesRegex(Task28RegistrySourceError, "frozen production interval"):
                inspect_registry_sources(
                    processed_root=processed,
                    checkpoint_root=checkpoints,
                    canonical_input=merged,
                )

    def test_processed_output_inside_candidate_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            processed, checkpoints, merged = self._fixture(root)
            with self.assertRaisesRegex(Task28RegistrySourceError, "outside the candidate"):
                inspect_registry_sources(
                    processed_root=processed,
                    checkpoint_root=checkpoints,
                    canonical_input=merged,
                    candidate_root=root,
                    min_obr_bytes=1,
                    max_obr_bytes=100,
                )


if __name__ == "__main__":
    unittest.main()
