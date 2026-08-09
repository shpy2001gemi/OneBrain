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
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from build_obr import build
from config import SOURCE_WIKIDATA
from production_qualification import canonical_json, signer_fingerprint


PROFILE = "onebrain/concept-registry-failure-qualification/1"
REQUIRED_SOURCES = ("chebi", "geonames", "ncbi", "wikidata", "wordnet")


@unittest.skipUnless(
    os.environ.get("ONEBRAIN_REGISTRY_FAILURE_QUALIFICATION"),
    "set ONEBRAIN_REGISTRY_FAILURE_QUALIFICATION to run the compiled drill",
)
class FailureQualificationIntegrationTests(unittest.TestCase):
    def test_raw_release_context_binding_cli_is_rejected_before_inputs_are_read(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            raw_context = root / "raw-release-context.json"
            raw_binding = root / "raw-release-binding.json"
            raw_context.write_text(json.dumps({"variant": "Release"}), encoding="utf-8")
            raw_binding.write_text(json.dumps({"release_aggregate_root": "00" * 32}), encoding="utf-8")
            result = subprocess.run(
                [
                    os.environ["ONEBRAIN_REGISTRY_FAILURE_QUALIFICATION"],
                    str(root / "work"), str(root / "candidate.obr"), str(root / "sbom.json"),
                    str(root / "sources.json"), str(root / "private.key"),
                    str(raw_context), str(raw_binding), str(root / "policy.json"),
                    str(root / "output.json"),
                ],
                capture_output=True, text=True, encoding="utf-8", errors="replace",
                timeout=30, check=False,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(
                "explicit --prequalification or --release mode is required",
                result.stderr,
            )

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
            public = (
                Ed25519PrivateKey.from_private_bytes(bytes([19] * 32)
                ).public_key().public_bytes_raw()
            )
            policy = {
                "algorithm": "Ed25519",
                "allowed_usages": [
                    "registry-release-stamp",
                    "registry-qualification-receipt",
                ],
                "format": "onebrain/concept-registry-trust-policy/1",
                "signers": [{
                    "fingerprint_algorithm": "blake3-derive-key-v1",
                    "fingerprint_context": "onebrain:concept-registry:signer-fingerprint:1",
                    "fingerprint_hex": signer_fingerprint(public),
                    "public_key_hex": public.hex(),
                }],
            }
            policy_path = root / "trust-policy.json"
            policy_path.write_text(json.dumps(policy), encoding="utf-8")
            context = {
                "format": "onebrain/qualification-run-context/1",
                "variant": "Prequalification",
                "closure_digest": "ab" * 32,
            }
            context_path = root / "run-context.json"
            context_path.write_text(json.dumps(context), encoding="utf-8")
            executable_path = Path(
                os.environ["ONEBRAIN_REGISTRY_FAILURE_QUALIFICATION"]
            )
            executable_digest = blake3.blake3(executable_path.read_bytes()).hexdigest()
            binding_path = root / "binding.json"
            binding_path.write_text(
                json.dumps(
                    {
                        "production_profile_blake3": "cd" * 32,
                        "probe_blake3": executable_digest,
                        "executable_blake3": executable_digest,
                    }
                ),
                encoding="utf-8",
            )
            output_path = root / "evidence" / "failure-qualification.json"

            result = subprocess.run(
                [
                    os.environ["ONEBRAIN_REGISTRY_FAILURE_QUALIFICATION"],
                    "--prequalification",
                    str(root / "work"),
                    str(obr_path),
                    str(sbom_path),
                    str(sources_path),
                    str(private_key_path),
                    str(context_path),
                    str(binding_path),
                    str(policy_path),
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
            self.assertEqual(
                report["format"],
                "onebrain/concept-registry-qualification-receipt/1",
            )
            self.assertEqual(report["receipt_kind"], "failure-qualification")
            payload = report["payload"]
            self.assertEqual(payload["profile"], PROFILE)
            self.assertTrue(payload["result"])
            self.assertFalse(payload["production_qualified"])
            self.assertFalse(payload["base_candidate_bound"])
            self.assertEqual(payload["qualification_context_variant"], "Prequalification")
            self.assertEqual(payload["closure_digest"], "ab" * 32)
            for forbidden in (
                "release_request_digest",
                "qualification_session_id",
                "candidate_commit",
                "candidate_tree",
            ):
                self.assertNotIn(forbidden, payload)
            self.assertTrue(all(payload["exit_oracles"].values()))
            self.assertTrue(
                payload["drills"]["truncated_label_index"]["activation_rejected"]
            )
            self.assertTrue(
                payload["drills"]["truncated_ccid_index"]["activation_rejected"]
            )
            self.assertTrue(
                payload["drills"]["disk_shortage"]["staging_directory_absent"]
            )
            kills = payload["drills"]["process_kills"]
            self.assertEqual(len(kills), 6)
            self.assertTrue(all(drill["old_or_new_complete"] for drill in kills))
            self.assertTrue(payload["exit_oracles"]["all_process_kills_reopened_complete_state"])
            command = payload["command"]
            self.assertEqual(
                payload["command_blake3"],
                blake3.blake3(canonical_json(command)).hexdigest(),
            )
            serialized_command = json.dumps(command, sort_keys=True)
            self.assertNotIn(str(private_key_path), serialized_command)
            self.assertIn("<external-private-key-redacted>", serialized_command)
            for path in (executable_path, obr_path, sbom_path, sources_path, policy_path):
                with self.subTest(path=path):
                    self.assertIn(blake3.blake3(path.read_bytes()).hexdigest(), serialized_command)
            mutated = [*command, "--omitted-option"]
            self.assertNotEqual(
                payload["command_blake3"],
                blake3.blake3(canonical_json(mutated)).hexdigest(),
            )
            self.assertEqual(
                json.loads(result.stdout)["payload"]["input"], payload["input"]
            )


if __name__ == "__main__":
    unittest.main()
