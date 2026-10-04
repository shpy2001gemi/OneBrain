"""Temporary synthetic custody only; no real dataset, Registry or host is opened."""

import contextlib
import io
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from scripts.base import prepare_ku_local_secrets as setup


class LocalSecretsTests(unittest.TestCase):
    def test_new_pair_matches_host_format_without_creating_dataset_or_printing_secrets(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            data = root / "dataset"
            output = root / "private-path-canary"
            stdout = io.StringIO()
            with contextlib.redirect_stdout(stdout):
                self.assertEqual(setup.main([
                    "--data-dir", str(data), "--output-dir", str(output)
                ]), 0)
            key = (output / "vault.key").read_bytes()
            token = (output / "api-token.txt").read_text(encoding="utf-8")
            self.assertEqual(len(key), 32)
            self.assertEqual(len(token), 64)
            self.assertTrue(all(c in "0123456789abcdef" for c in token))
            self.assertEqual(len(list(output.iterdir())), 2)
            self.assertFalse(data.exists())
            self.assertIn("ku_local_secrets_ready", stdout.getvalue())
            self.assertNotIn(token, stdout.getvalue())
            self.assertNotIn("canary", stdout.getvalue())
            if os.name != "nt":
                self.assertEqual(output.stat().st_mode & 0o777, 0o700)
                self.assertEqual((output / "vault.key").stat().st_mode & 0o777, 0o600)

    def test_existing_dataset_and_custody_are_never_replaced_even_when_empty(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            data = root / "dataset"
            output = root / "custody"
            data.mkdir()
            with self.assertRaisesRegex(setup.SetupError, "dataset_exists"):
                setup.prepare(data, output)
            saved = data / "saved.bin"
            saved.write_bytes(b"synthetic saved bytes")
            with self.assertRaisesRegex(setup.SetupError, "dataset_exists"):
                setup.prepare(data, output)
            self.assertEqual(saved.read_bytes(), b"synthetic saved bytes")
            self.assertFalse(output.exists())
            fresh_data = root / "new-dataset"
            setup.prepare(fresh_data, output)
            before = {p.name: p.read_bytes() for p in output.iterdir()}
            with self.assertRaisesRegex(setup.SetupError, "output_exists"):
                setup.prepare(fresh_data, output)
            self.assertEqual(before, {p.name: p.read_bytes() for p in output.iterdir()})
            with self.assertRaises(FileExistsError):
                setup._write_secret(output / "vault.key", b"replacement")
            self.assertEqual((output / "vault.key").read_bytes(), before["vault.key"])

    def test_repository_overlap_and_missing_parent_refuse_before_writes(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            cases = [
                (setup.REPOSITORY_ROOT / "unused-secret-test-dataset", root / "custody", "repository_path"),
                (root / "dataset", setup.REPOSITORY_ROOT / "unused-secret-test-custody", "repository_path"),
                (root / "same", root / "same", "path_overlap"),
                (root / "dataset", root / "dataset" / "custody", "path_overlap"),
                (root / "custody" / "dataset", root / "custody", "path_overlap"),
                (root / "dataset", root / "missing" / "custody", "parent_unavailable"),
            ]
            for data, output, code in cases:
                with self.subTest(code=code), self.assertRaisesRegex(setup.SetupError, code):
                    setup.prepare(data, output)
                self.assertFalse(data.exists())
                self.assertFalse(output.exists())

    def test_later_write_failure_retains_partial_key_and_redacts_diagnostics(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            data = root / "dataset"
            output = root / "private-path-canary"
            write = setup._write_secret

            def fail_token(path, content):
                if path.name == "api-token.txt":
                    raise OSError("private-path-and-secret-canary")
                write(path, content)

            stderr = io.StringIO()
            with patch.object(setup, "_write_secret", side_effect=fail_token), contextlib.redirect_stderr(stderr):
                self.assertEqual(setup.main([
                    "--data-dir", str(data), "--output-dir", str(output)
                ]), 1)
            retained = (output / "vault.key").read_bytes()
            self.assertEqual(len(retained), 32)
            self.assertFalse((output / "api-token.txt").exists())
            self.assertFalse(data.exists())
            self.assertIn("ku_local_secrets_write_failed", stderr.getvalue())
            self.assertNotIn("canary", stderr.getvalue())
            with self.assertRaisesRegex(setup.SetupError, "output_exists"):
                setup.prepare(data, output)
            setup.prepare(data, root / "retry-custody")
            self.assertEqual((output / "vault.key").read_bytes(), retained)


if __name__ == "__main__":
    unittest.main()
