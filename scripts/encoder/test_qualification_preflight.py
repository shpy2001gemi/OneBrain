"""Inventory integrity tests; these contain no model or qualification evidence."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from scripts.encoder.qualification_preflight import local_model, metadata, strict_json, verify_layer


class PreflightTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.home = Path(self.tmp.name)
        (self.home / "blobs").mkdir()
        data = b"test bytes, not real model weights"
        self.sha = hashlib.sha256(data).hexdigest()
        self.blob = self.home / "blobs" / ("sha256-" + self.sha)
        self.blob.write_bytes(data)
        self.layer = {"digest": "sha256:" + self.sha, "size": len(data),
                      "mediaType": "application/vnd.ollama.image.model"}

    def test_verified_content_and_size(self):
        self.assertEqual(verify_layer(self.home, self.layer)["sha256"], self.sha)

    def test_same_size_tampering_rejected(self):
        self.blob.write_bytes(b"x" * self.layer["size"])
        with self.assertRaisesRegex(ValueError, "blob_digest_mismatch"):
            verify_layer(self.home, self.layer)

    def test_truncated_blob_rejected(self):
        self.blob.write_bytes(b"short")
        with self.assertRaisesRegex(ValueError, "blob_size_mismatch"):
            verify_layer(self.home, self.layer)

    def test_digest_path_escape_rejected(self):
        with self.assertRaisesRegex(ValueError, "invalid_blob_digest"):
            verify_layer(self.home, {**self.layer, "digest": "../../secret"})

    def test_duplicate_or_nonfinite_json_rejected(self):
        for raw in [b'{"a":1,"a":2}', b'{"a":NaN}']:
            with self.subTest(raw=raw), self.assertRaises(ValueError):
                strict_json(raw)

    def test_tag_only_cannot_replace_artifact_verification(self):
        with self.assertRaises(FileNotFoundError):
            local_model(self.home, "test:1")

    def test_url_or_implicit_tag_rejected(self):
        for tag in ["latest", "https://cloud/model:1", "../../secret:1"]:
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                local_model(self.home, tag)

    def test_manifest_without_model_rejected(self):
        folder = self.home / "manifests/registry.ollama.ai/library/test"
        folder.mkdir(parents=True)
        config = {**self.layer, "mediaType": "config"}
        (folder / "1").write_text(json.dumps({"config": config, "layers": []}))
        with self.assertRaisesRegex(ValueError, "model_layer_missing"):
            local_model(self.home, "test:1")

    def test_inference_routes_rejected_before_network(self):
        with patch("urllib.request.build_opener") as network:
            for route in ["chat", "generate", "pull", "embed", "delete"]:
                with self.subTest(route=route), self.assertRaisesRegex(ValueError, "metadata_only"):
                    metadata(route, "test:1")
            network.assert_not_called()


if __name__ == "__main__":
    unittest.main()
