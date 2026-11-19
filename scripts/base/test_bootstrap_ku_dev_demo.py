"""Synthetic unit tests for bootstrap_ku_dev_demo; no cargo, host or real Registry is used."""

from __future__ import annotations

import contextlib
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from scripts.base import bootstrap_ku_dev_demo as boot

TOKEN_CANARY = "token-canary-0123456789abcdef0123456789abcdef"
PUBLIC_KEY = "ab" * 32


class FakeRunner:
    """Simulates the composed tools by creating their documented outputs."""

    def __init__(self, fail_step: str | None = None, cargo: bool = True):
        self.calls: list[list[str]] = []
        self.fail_step = fail_step
        self.cargo = cargo

    def __call__(self, argv):
        argv = list(argv)
        self.calls.append(argv)
        ok = subprocess.CompletedProcess(argv, 0, stdout="", stderr="")
        if argv[-1] == "--version":
            if "cargo" in Path(argv[0]).name and not self.cargo:
                return subprocess.CompletedProcess(argv, 1, stdout="", stderr="broken")
            return subprocess.CompletedProcess(argv, 0, stdout="tool 1.0.0\n", stderr="")
        if "concept_registry_dev_demo" in argv:
            if self.fail_step == "registry":
                return subprocess.CompletedProcess(argv, 1, stdout="", stderr="boom: disk\n")
            registry, key_file = Path(argv[-2]), Path(argv[-1])
            (registry / "releases").mkdir(parents=True)
            (registry / "state").mkdir()
            key_file.write_text(PUBLIC_KEY + "\n", encoding="utf-8")
            ok.stdout = ("ku_dev_demo_registry_ready release_id=dev-demo-synthetic-abababababab\n"
                         f"public_key={PUBLIC_KEY}\n")
            return ok
        if "concept_registry_release" in argv:
            if self.fail_step == "status":
                return subprocess.CompletedProcess(argv, 1, stdout="", stderr="no active\n")
            return ok
        if str(boot.SECRETS_HELPER) in argv:
            out = Path(argv[argv.index("--output-dir") + 1])
            out.mkdir()
            (out / "vault.key").write_bytes(b"\x01" * 32)
            (out / "api-token.txt").write_text(TOKEN_CANARY, encoding="ascii")
            ok.stdout = "ku_local_secrets_ready\n"
            return ok
        if "ku_manual_source" in argv:
            if self.fail_step == "source":
                return subprocess.CompletedProcess(argv, 1, stdout="", stderr="bad request\n")
            request = json.loads(Path(argv[-1]).read_text(encoding="utf-8"))
            out = Path(request["output_dir"])
            out.mkdir()
            (out / "governance.json").write_text("{}", encoding="utf-8")
            (out / "source.canonical").write_bytes(b"canonical")
            return ok
        raise AssertionError(f"unexpected tool call: {argv}")


def run_main(argv, runner):
    stdout, stderr = io.StringIO(), io.StringIO()
    with patch.object(boot.shutil, "which", side_effect=lambda tool: f"/usr/bin/{tool}"), \
            contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
        code = boot.main(argv, runner)
    return code, stdout.getvalue(), stderr.getvalue()


class BootstrapKuDevDemoTests(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = Path(self._tmp.name)

    def tearDown(self):
        self._tmp.cleanup()

    def test_dev_demo_registry_composes_host_config_without_secrets(self):
        root = self.tmp / "demo"
        runner = FakeRunner()
        code, out, err = run_main(["--demo-root", str(root), "--dev-demo-registry"], runner)
        self.assertEqual(code, 0, err)
        raw = (root / "host.json").read_bytes()
        self.assertFalse(raw.startswith(b"\xef\xbb\xbf"))
        cfg = json.loads(raw)
        self.assertEqual(cfg["registry_public_key"], PUBLIC_KEY)
        self.assertEqual(Path(cfg["registry_root"]), root.resolve() / "registry")
        self.assertEqual(Path(cfg["data_dir"]), root.resolve() / "sender" / "dataset")
        self.assertFalse(Path(cfg["data_dir"]).exists())  # host creates it on first start
        self.assertEqual(cfg["port"], 4280)
        self.assertNotIn("network", cfg)
        self.assertEqual(len(cfg["sources"]), 1)
        self.assertTrue(Path(cfg["sources"][0]["canonical_file"]).is_file())
        self.assertFalse((root / "receiver.json").exists())
        self.assertIn("ku_dev_demo_bootstrap_ready", out)
        self.assertIn("DEV-ONLY", out)
        self.assertNotIn(TOKEN_CANARY, out + err)
        request = json.loads((root / "sample" / "manual-source-request.json").read_text("utf-8"))
        self.assertIs(request["consent_local_private"], True)
        self.assertEqual((root / "sample" / "water.txt").read_text("utf-8"), boot.SAMPLE_TEXT)
        # Every cargo invocation is locked and uses the repository manifest.
        for call in runner.calls:
            if call[0] == "cargo":
                self.assertIn("--locked", call)
                self.assertIn(str(boot.MANIFEST), call)

    def test_with_receiver_writes_two_isolated_network_configs(self):
        root = self.tmp / "demo"
        code, out, err = run_main(["--demo-root", str(root), "--dev-demo-registry",
                                   "--with-receiver", "--port", "4300"], FakeRunner())
        self.assertEqual(code, 0, err)
        sender = json.loads((root / "host.json").read_text("utf-8"))
        receiver = json.loads((root / "receiver.json").read_text("utf-8"))
        self.assertEqual(sender["network"], boot.NETWORK_BLOCK)
        self.assertEqual(receiver["network"], boot.NETWORK_BLOCK)
        self.assertEqual((sender["port"], receiver["port"]), (4300, 4301))
        self.assertEqual(receiver["sources"], [])
        self.assertEqual(sender["registry_root"], receiver["registry_root"])
        for key in ("data_dir", "vault_key_file", "api_token_file"):
            self.assertNotEqual(sender[key], receiver[key])
        self.assertIn("--receiver-kind web", out)
        self.assertIn("vnext-network-runtime", out)

    def test_registry_mode_is_explicit(self):
        root = self.tmp / "demo"
        cases = [
            ([], "ku_dev_demo_registry_required"),
            (["--dev-demo-registry", "--registry-root", str(self.tmp)],
             "ku_dev_demo_mode_conflict"),
            (["--registry-root", str(self.tmp)], "ku_dev_demo_registry_incomplete"),
        ]
        for extra, code_text in cases:
            code, _, err = run_main(["--demo-root", str(root), *extra], FakeRunner())
            self.assertEqual(code, 1)
            self.assertIn(code_text, err)
            self.assertFalse(root.exists())

    def test_refuses_existing_repository_and_parentless_roots(self):
        existing = self.tmp / "existing"
        existing.mkdir()
        (existing / "keep.txt").write_text("keep", encoding="utf-8")
        code, _, err = run_main(["--demo-root", str(existing), "--dev-demo-registry"],
                                FakeRunner())
        self.assertEqual(code, 1)
        self.assertIn("ku_dev_demo_root_exists", err)
        self.assertEqual(sorted(p.name for p in existing.iterdir()), ["keep.txt"])

        inside = boot.REPO / "src" / "never-created-demo-root"
        code, _, err = run_main(["--demo-root", str(inside), "--dev-demo-registry"],
                                FakeRunner())
        self.assertEqual(code, 1)
        self.assertIn("ku_dev_demo_repository_path", err)
        self.assertFalse(inside.exists())

        orphan = self.tmp / "missing" / "demo"
        code, _, err = run_main(["--demo-root", str(orphan), "--dev-demo-registry"],
                                FakeRunner())
        self.assertEqual(code, 1)
        self.assertIn("ku_dev_demo_parent_missing", err)
        self.assertFalse(orphan.parent.exists())

    def test_step_failure_retains_partial_root_and_writes_no_config(self):
        for step in ("registry", "source"):
            root = self.tmp / f"demo-{step}"
            code, _, err = run_main(["--demo-root", str(root), "--dev-demo-registry"],
                                    FakeRunner(fail_step=step))
            self.assertEqual(code, 1)
            self.assertIn("ku_dev_demo_step_failed", err)
            self.assertIn("Partial demo root retained", err)
            self.assertTrue(root.is_dir())
            self.assertFalse((root / "host.json").exists())
        self.assertIn("boom: disk", run_main(
            ["--demo-root", str(self.tmp / "demo-x"), "--dev-demo-registry"],
            FakeRunner(fail_step="registry"))[2])

    def test_check_only_writes_nothing_and_missing_cargo_fails(self):
        before = sorted(self.tmp.iterdir())
        code, out, _ = run_main(["--check-only", "--demo-root", str(self.tmp / "d")],
                                FakeRunner())
        self.assertEqual(code, 0)
        self.assertIn("ku_dev_demo_prerequisites_ok", out)
        self.assertEqual(sorted(self.tmp.iterdir()), before)
        code, out, err = run_main(["--check-only"], FakeRunner(cargo=False))
        self.assertEqual(code, 1)
        self.assertIn("[FAIL] cargo", out)
        self.assertIn("ku_dev_demo_prerequisites_missing", err)

    def test_real_registry_mode_validates_key_and_runs_read_only_status(self):
        registry = self.tmp / "registry"
        registry.mkdir()
        key_file = self.tmp / "registry-public-key.txt"
        key_file.write_text("NOT-HEX", encoding="utf-8")
        root = self.tmp / "demo"
        args = ["--demo-root", str(root), "--registry-root", str(registry),
                "--registry-public-key-file", str(key_file)]
        code, _, err = run_main(args, FakeRunner())
        self.assertEqual(code, 1)
        self.assertIn("ku_dev_demo_public_key_invalid", err)
        self.assertNotIn("NOT-HEX", err)

        key_file.write_text(f" {PUBLIC_KEY}\r\n", encoding="utf-8")
        root2 = self.tmp / "demo2"
        runner = FakeRunner()
        code, out, err = run_main(["--demo-root", str(root2), *args[2:]], runner)
        self.assertEqual(code, 0, err)
        self.assertTrue(any("concept_registry_release" in c and "status" in c
                            for c in runner.calls))
        self.assertFalse(any("concept_registry_dev_demo" in c for c in runner.calls))
        cfg = json.loads((root2 / "host.json").read_text("utf-8"))
        self.assertEqual(Path(cfg["registry_root"]), registry.resolve())
        self.assertEqual(cfg["registry_public_key"], PUBLIC_KEY)
        self.assertNotIn("DEV-ONLY", out)
        self.assertEqual(sorted(p.name for p in registry.iterdir()), [])

        code, _, err = run_main(["--demo-root", str(self.tmp / "demo3"), *args[2:]],
                                FakeRunner(fail_step="status"))
        self.assertEqual(code, 1)
        self.assertIn("verify active real Registry", err)


if __name__ == "__main__":
    unittest.main()
