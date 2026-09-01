import copy
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import blake3
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

from scripts.concept_registry.production_qualification import signer_fingerprint
from scripts.release.task28_prebuilt_registry import (
    BINDING_ENVELOPE_FORMAT,
    PrebuiltRegistryError,
    create_prebuilt_registry_binding,
    inspect_prebuilt_registry,
    verify_prebuilt_registry_binding,
)
from scripts.release.verify_base_release_request import VerifiedQualificationContextV2


class Task28PrebuiltRegistryTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.key = Ed25519PrivateKey.generate()
        public = self.key.public_key().public_bytes_raw()
        self.policy = {
            "format": "onebrain/concept-registry-trust-policy/1",
            "algorithm": "Ed25519",
            "allowed_usages": ["registry-qualification-receipt"],
            "signers": [
                {
                    "public_key_hex": public.hex(),
                    "fingerprint_hex": signer_fingerprint(public),
                }
            ],
        }
        self.context = VerifiedQualificationContextV2(
            request_digest="11" * 32,
            signer_fingerprint="22" * 32,
            trust_policy_digest="33" * 32,
            run_context={
                "release_request_digest": "11" * 32,
                "qualification_session_id": "44" * 32,
                "candidate_commit": "55" * 20,
                "candidate_tree": "66" * 20,
            },
            bindings={},
            tooling_blake3={},
            request={"required_targets": {"linux": "x86_64-unknown-linux-gnu"}},
            production=True,
        )
        self._write_registry()
        vector = json.loads(
            (
                Path(__file__).resolve().parents[2]
                / "src/test-vectors/vnext/base-v1-compatibility-v1.json"
            ).read_text(encoding="utf-8")
        )
        semantic = copy.deepcopy(vector["baseline"])
        semantic["base_commit"] = {"kind": "sha1", "hex": "55" * 20}
        semantic["target_triple"] = "x86_64-unknown-linux-gnu"
        semantic["toolchain"] = {"kind": "known", "hex": "88" * 32}
        self.semantic = self.root / "candidate-semantic-evidence.json"
        self.semantic.write_text(
            json.dumps(semantic, sort_keys=True, separators=(",", ":")),
            encoding="utf-8",
        )

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def _write_registry(self) -> None:
        obr = b"owner-local-prebuilt-obr"
        labels = b"owner-local-label-index"
        ccids = b"owner-local-ccid-index"
        (self.root / "concepts.obr").write_bytes(obr)
        (self.root / "concepts.obr.labels.idx").write_bytes(labels)
        (self.root / "concepts.obr.ccids.idx").write_bytes(ccids)
        manifest = {
            "manifest_version": 1,
            "builder_version": "onebrain-concept-registry-builder/1",
            "obr_schema_version": 1,
            "dedup_policy_version": "fixture-dedup-v1",
            "entry_count": 7,
            "label_count": 9,
            "obr_blake3": blake3.blake3(obr).hexdigest(),
            "label_index": {
                "blake3": blake3.blake3(labels).hexdigest(),
                "file_size": len(labels),
            },
            "ccid_index": {
                "blake3": blake3.blake3(ccids).hexdigest(),
                "file_size": len(ccids),
            },
            "sources": {"fixture": {"snapshot_id": "already-built"}},
        }
        verification = {
            "obr_blake3": blake3.blake3(obr).hexdigest(),
            "file_size": len(obr),
            "label_index": {
                "blake3": blake3.blake3(labels).hexdigest(),
                "file_size": len(labels),
            },
            "ccid_index": {
                "blake3": blake3.blake3(ccids).hexdigest(),
                "file_size": len(ccids),
            },
        }
        (self.root / "concepts.obr.manifest.json").write_text(
            json.dumps(manifest), encoding="utf-8"
        )
        (self.root / "concepts.obr.verification.json").write_text(
            json.dumps(verification), encoding="utf-8"
        )

    def _create(self) -> dict[str, object]:
        with patch(
            "scripts.release.task28_prebuilt_registry._profile_policy",
            return_value=self.policy,
        ):
            return create_prebuilt_registry_binding(
                self.context,
                self.root,
                self.semantic,
                self.key,
                minimum_data_bytes=1,
                maximum_data_bytes=10_000,
            )

    def _verify(self, binding: object, context: VerifiedQualificationContextV2 | None = None) -> dict[str, object]:
        with patch(
            "scripts.release.task28_prebuilt_registry._profile_policy",
            return_value=self.policy,
        ):
            return verify_prebuilt_registry_binding(
                context or self.context,
                self.root,
                self.semantic,
                binding,
                minimum_data_bytes=1,
                maximum_data_bytes=10_000,
            )

    def test_signs_and_rehashes_only_final_prebuilt_output(self) -> None:
        measured = inspect_prebuilt_registry(
            self.root, minimum_data_bytes=1, maximum_data_bytes=10_000
        )
        binding = self._create()
        verified = self._verify(binding)
        self.assertEqual(binding["format"], BINDING_ENVELOPE_FORMAT)
        self.assertEqual(verified["registry_origin"], "owner-local-prebuilt-output")
        self.assertFalse(verified["source_archives_reprocessed"])
        self.assertEqual(verified["release_aggregate_root"], measured["release_aggregate_root"])
        self.assertEqual(
            verified["registry_semantic_digest"], measured["registry_semantic_digest"]
        )
        self.assertNotEqual(
            verified["candidate_semantic_digest"], verified["registry_semantic_digest"]
        )
        self.assertEqual(
            [row["name"] for row in measured["rows"]],
            [
                "concepts.obr",
                "concepts.obr.labels.idx",
                "concepts.obr.ccids.idx",
                "concepts.obr.manifest.json",
                "concepts.obr.verification.json",
            ],
        )

    def test_changed_registry_byte_fails_closed(self) -> None:
        binding = self._create()
        (self.root / "concepts.obr").write_bytes(b"changed")
        with self.assertRaisesRegex(PrebuiltRegistryError, "manifest OBR digest"):
            self._verify(binding)

    def test_binding_cannot_cross_request_or_candidate(self) -> None:
        binding = self._create()
        changed = copy.deepcopy(self.context)
        changed = VerifiedQualificationContextV2(
            **{**changed.__dict__, "run_context": {**changed.run_context, "candidate_tree": "77" * 20}}
        )
        with self.assertRaisesRegex(PrebuiltRegistryError, "candidate_tree mismatch"):
            self._verify(binding, changed)

    def test_changed_base_semantic_evidence_fails_closed(self) -> None:
        binding = self._create()
        semantic = json.loads(self.semantic.read_text(encoding="utf-8"))
        semantic["feature_set_digest"] = "99" * 32
        self.semantic.write_text(
            json.dumps(semantic, sort_keys=True, separators=(",", ":")),
            encoding="utf-8",
        )
        with self.assertRaisesRegex(PrebuiltRegistryError, "Base candidate_semantic_digest"):
            self._verify(binding)

    def test_unknown_binding_field_fails_closed(self) -> None:
        binding = self._create()
        binding["unexpected"] = True
        with self.assertRaisesRegex(PrebuiltRegistryError, "envelope fields are not closed"):
            self._verify(binding)


if __name__ == "__main__":
    unittest.main()
