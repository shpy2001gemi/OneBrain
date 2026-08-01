"""Integration test for Concept Registry failure qualification evidence."""

from __future__ import annotations

import json
import os
import subprocess
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


PROFILE = "onebrain/concept-registry-failure-qualification/1"
REQUIRED_SOURCES = ("chebi", "geonames", "ncbi", "wikidata", "wordnet")


@unittest.skipUnless(
    os.environ.get("ONEBRAIN_REGISTRY_FAILURE_QUALIFICATION"),
    "set ONEBRAIN_REGISTRY_FAILURE_QUALIFICATION to run the compiled drill",
)
class FailureQualificationIntegrationTests(unittest.TestCase):
    def test_truncated_indexes_and_disk_shortage_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            merged = root / "merged"
            merged.mkdir()
            input_path = merged / "concepts_deduped.jsonl"
            input_path.write_text(
                json.dumps(
                    {
                        "source": SOURCE_WIKIDATA,
                        "ext_id": 283,
                        "category": 7,
                        "name": "water",
                        "canonical_form": "wd:Q283",
                        "labels": {"en": "water", "vi": "nước"},
                    }
                )
                + "\n",
                encoding="utf-8",
            )
            obr_path = root / "concepts.obr"
            build(input_path, obr_path)
            manifest = json.loads(
                Path(f"{obr_path}.manifest.json").read_text(encoding="utf-8")
            )

            sbom_path = root / "sbom.spdx.json"
            sbom_path.write_text(
                json.dumps(
                    {
                        "spdxVersion": "SPDX-2.3",
                        "dataLicense": "CC0-1.0",
                        "packages": [],
                    }
                ),
                encoding="utf-8",
            )
            sources_path = root / "sources.json"
            sources = []
            for index, name in enumerate(REQUIRED_SOURCES, start=1):
                source = manifest["sources"][name]
                sources.append(
                    {
                        "name": name,
                        "snapshot_id": source["snapshot_id"],
                        "source_uri": source["source_uri"],
                        "license": source["license"],
                        "snapshot_blake3": blake3.blake3(
                            f"snapshot-{index}".encode()
                        ).hexdigest(),
                        "download_blake3": blake3.blake3(
                            f"download-{index}".encode()
                        ).hexdigest(),
                    }
                )
            sources_path.write_text(
                json.dumps(sources, indent=2), encoding="utf-8"
            )
            private_key_path = root / "private.key"
            private_key_path.write_text(bytes([19] * 32).hex() + "\n", encoding="utf-8")
            output_path = root / "evidence" / "failure-qualification.json"

            result = subprocess.run(
                [
                    os.environ["ONEBRAIN_REGISTRY_FAILURE_QUALIFICATION"],
                    str(root / "work"),
                    str(obr_path),
                    str(sbom_path),
                    str(sources_path),
                    str(private_key_path),
                    str(output_path),
                ],
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
                timeout=60,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(output_path.read_text(encoding="utf-8"))
            self.assertEqual(report["profile"], PROFILE)
            self.assertTrue(report["qualified"])
            self.assertFalse(report["production_qualified"])
            self.assertTrue(report["full_registry_evidence_required"])
            self.assertTrue(all(report["exit_oracles"].values()))
            self.assertTrue(
                report["drills"]["truncated_label_index"]["activation_rejected"]
            )
            self.assertTrue(
                report["drills"]["truncated_ccid_index"]["activation_rejected"]
            )
            self.assertTrue(
                report["drills"]["disk_shortage"]["staging_directory_absent"]
            )
            self.assertEqual(
                json.loads(result.stdout)["input"], report["input"]
            )


if __name__ == "__main__":
    unittest.main()
