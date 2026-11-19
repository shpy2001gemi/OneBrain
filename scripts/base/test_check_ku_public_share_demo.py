"""Synthetic unit tests for check_ku_public_share_demo; no real dataset or host is opened."""

from __future__ import annotations

import argparse
import base64
import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import MagicMock, patch

from scripts.base import check_ku_public_share_demo as demo


class CheckKuPublicShareDemoTests(unittest.TestCase):
    def _write_host_configs(self, root: Path) -> tuple[Path, Path, str]:
        sender_token = root / "sender-token.txt"
        receiver_token = root / "receiver-token.txt"
        secret_value = "receiver-secret-canary-token-0123456789abcdef"
        sender_token.write_text("sender-secret-canary-token-0123456789abcdef\n", encoding="utf-8")
        receiver_token.write_text(f"{secret_value}\n", encoding="utf-8")
        base_cfg = {
            "registry_root": str(root / "registry"),
            "registry_public_key": "11" * 32,
            "vault_key_file": str(root / "vault.key"),
            "web_dir": str(root / "web"),
            "network": {"network_opt_in": True, "bind_addr": "127.0.0.1:0"},
        }
        sender_cfg = {
            **base_cfg,
            "data_dir": str(root / "sender-data"),
            "api_token_file": str(sender_token),
            "port": 4511,
            "sources": [{"label": "note", "canonical_file": str(root / "note.canonical")}],
            "ollama": {
                "executable": str(root / "ollama"),
                "models_dir": str(root / "models"),
                "models": ["qwen3:8b"],
                "memory_limit_bytes": 12884901888,
            },
        }
        receiver_cfg = {
            **base_cfg,
            "data_dir": str(root / "receiver-data"),
            "api_token_file": str(receiver_token),
            "port": 4512,
        }
        sender_path = root / "sender.json"
        receiver_path = root / "receiver.json"
        sender_path.write_text(json.dumps(sender_cfg), encoding="utf-8")
        receiver_path.write_text(json.dumps(receiver_cfg), encoding="utf-8")
        return sender_path, receiver_path, secret_value

    def _make_args(self, sender_path: Path, receiver_path: Path, **overrides) -> argparse.Namespace:
        defaults = {
            "sender_config": sender_path,
            "receiver_config": receiver_path,
            "sender_kind": "web",
            "receiver_kind": "web",
            "receiver_exe": None,
            "cli_exe": sender_path.parent / "missing-onebrain",
            "web_exe": sender_path.parent / "ku_local_web",
            "peer_exe": sender_path.parent / "ku_peer_demo",
            "require_cli_client": False,
            "skip_cli_client": False,
            "ack_timeout": 5,
            "result": None,
        }
        defaults.update(overrides)
        return argparse.Namespace(**defaults)

    def test_build_cli_host_args_switches_network_and_never_embeds_raw_token(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            sender_path, _, _ = self._write_host_configs(root)
            cfg = demo.load_host_config(sender_path, "sender")
            exe = root / "onebrain"
            net_argv = demo.build_cli_host_args(exe, cfg, with_network=True)
            local_argv = demo.build_cli_host_args(exe, cfg, with_network=False)

            self.assertIn("--api-token-file", net_argv)
            self.assertNotIn("--api-token", net_argv)
            self.assertIn("--ku-network-bind", net_argv)
            self.assertNotIn("--ku-network-bind", local_argv)
            self.assertIn("--ku-ollama-model", local_argv)
            self.assertFalse(any("sender-secret-canary" in item for item in net_argv))
            self.assertFalse(any("sender-secret-canary" in item for item in local_argv))

    def test_cli_client_resolution_and_port_collision_guards(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            sender_path, receiver_path, _ = self._write_host_configs(root)

            # Optional on web receiver when --cli-exe is absent.
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                check_opt = demo.Check(self._make_args(sender_path, receiver_path))
            self.assertFalse(check_opt.cli_client_enabled)
            self.assertIn("skipping optional onebrain ku CLI client checks", buf.getvalue())

            # Required on --receiver-kind cli or --require-cli-client when missing.
            for kwargs in ({"receiver_kind": "cli"}, {"require_cli_client": True}):
                with self.assertRaisesRegex(SystemExit, "CLI binary not found"):
                    demo.Check(self._make_args(sender_path, receiver_path, **kwargs))

            cli_exe = root / "onebrain-fake"
            cli_exe.write_text("placeholder", encoding="utf-8")
            check_cli = demo.Check(self._make_args(
                sender_path, receiver_path, receiver_kind="cli", cli_exe=cli_exe))
            self.assertTrue(check_cli.cli_client_enabled)
            self.assertEqual(check_cli.receiver.kind, "onebrain start --api")

            check_skip = demo.Check(self._make_args(
                sender_path, receiver_path, receiver_kind="cli",
                cli_exe=cli_exe, skip_cli_client=True))
            self.assertFalse(check_skip.cli_client_enabled)

            same_port = json.loads(receiver_path.read_text(encoding="utf-8"))
            same_port["port"] = 4511
            receiver_path.write_text(json.dumps(same_port), encoding="utf-8")
            with self.assertRaisesRegex(SystemExit, "different port"):
                demo.Check(self._make_args(sender_path, receiver_path, cli_exe=cli_exe))

    def test_cli_client_checks_verify_get_export_env_override_and_token_redaction(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            sender_path, receiver_path, receiver_secret = self._write_host_configs(root)
            cli_exe = root / "onebrain-fake"
            cli_exe.write_text("placeholder", encoding="utf-8")
            check = demo.Check(self._make_args(
                sender_path, receiver_path, receiver_kind="cli", cli_exe=cli_exe))

            pub_cid = "aa" * 32
            sem_cid = "bb" * 32
            raw_object = b"canonical-public-ku-bytes"
            canonical_b64 = base64.b64encode(raw_object).decode("ascii")
            exchange_stream = (
                b"OBXV1\n"
                + json.dumps({
                    "version": 1,
                    "class": "vnext-public",
                    "kind": 1,
                    "cid": pub_cid,
                    "canonical_hex": raw_object.hex(),
                }).encode("utf-8") + b"\n"
                + json.dumps({
                    "footer": {"version": 1, "count": 1, "byte_length": 1, "root": "dd" * 32},
                }).encode("utf-8") + b"\n"
            )
            exchange_b64 = base64.b64encode(exchange_stream).decode("ascii")
            prepared = {
                "public_object_cid": pub_cid,
                "semantic_content_cid": sem_cid,
                "canonical_base64": canonical_b64,
                "semantic": {"statements": [{"predicate_ccid": "cc" * 32, "text_arguments": ["water"]}]},
            }

            calls: list[tuple[list[str], dict]] = []

            def fake_run(argv, **kwargs):
                calls.append((argv, kwargs["env"]))
                if "get" in argv:
                    body = {
                        "ok": True,
                        "data": {"payload": {
                            "object_cid": pub_cid,
                            "disclosure_class": "PUBLIC",
                            "artifact_validity": "accepted_known",
                            "semantic_content_cid": sem_cid,
                            "canonical_bytes": canonical_b64,
                            "executable": False,
                            "limitations": ["fidelity_unassessed"],
                        }},
                    }
                else:
                    idx = argv.index("--payload-file")
                    req = json.loads(Path(argv[idx + 1]).read_text(encoding="utf-8"))
                    self.assertEqual(req, {"mode": "canonical_public_exchange", "object_cids": [pub_cid]})
                    body = {
                        "ok": True,
                        "data": {"payload": {
                            "mode": "canonical_public_exchange",
                            "object_cids": [pub_cid],
                            "requires_base_management": False,
                            "public_records": exchange_b64,
                        }},
                    }
                res = MagicMock()
                res.returncode = 0
                res.stdout = json.dumps(body)
                res.stderr = "Local authorized scope only."
                return res

            with patch.object(demo.subprocess, "run", side_effect=fake_run), \
                    contextlib.redirect_stdout(io.StringIO()):
                check.check_receiver_cli_client(prepared, "")

            self.assertEqual(len(calls), 2)
            for argv, env in calls:
                self.assertIn("--api-token-file", argv)
                self.assertNotIn("--api-token", argv)
                self.assertEqual(env["ONEBRAIN_API_TOKEN"], demo.IGNORED_ENV_TOKEN)
            self.assertTrue(all(r["pass"] for r in check.results), check.results)

            # A leaked token in stderr must fail the check immediately.
            check_leak = demo.Check(self._make_args(
                sender_path, receiver_path, receiver_kind="cli", cli_exe=cli_exe))

            def leak_run(argv, **kwargs):
                res = fake_run(argv, **kwargs)
                res.stderr = f"leaked {receiver_secret}"
                return res

            with patch.object(demo.subprocess, "run", side_effect=leak_run), \
                    contextlib.redirect_stdout(io.StringIO()):
                check_leak.check_receiver_cli_client(prepared, "")
            self.assertTrue(all(not r["pass"] for r in check_leak.results))

    def test_local_only_offline_restart_strips_network_and_cleans_up_temp_config(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            sender_path, receiver_path, _ = self._write_host_configs(root)
            check = demo.Check(self._make_args(
                sender_path, receiver_path, receiver_kind="web", skip_cli_client=True))

            captured_cfg = {}

            def fake_start_web(exe, cfg_path, api, name, *, with_network=True):
                captured_cfg["with_network"] = with_network
                captured_cfg["path"] = Path(cfg_path)
                captured_cfg["json"] = json.loads(Path(cfg_path).read_text(encoding="utf-8"))
                proc = MagicMock()
                proc.p.returncode = 0
                return proc, "", ""

            pub_cid = "aa" * 32
            sem_cid = "bb" * 32
            stmts = [{"predicate_ccid": "cc" * 32, "text_arguments": ["water"]}]
            prepared = {
                "public_object_cid": pub_cid,
                "semantic_content_cid": sem_cid,
                "canonical_base64": base64.b64encode(b"obj").decode("ascii"),
                "semantic": {"statements": stmts},
            }

            def fake_editor(action, req_payload):
                if action == "workspace_summary":
                    return 200, {"ok": True, "data": {"payload": {
                        "private_saved": 0, "network": {"enabled": False}}}}
                if action == "share_list":
                    return 200, {"ok": True, "data": {"payload": {"items": [{
                        "object_cid": pub_cid,
                        "semantic": {"semantic_content_cid": sem_cid},
                        "authorizes_reward": False,
                    }]}}}
                if action == "saved_semantic":
                    return 200, {"ok": True, "data": {"payload": {"items": [{
                        "object_cid": pub_cid,
                        "semantic": {"statements": stmts},
                    }]}}}
                if action == "share_inspect":
                    return 503, {"ok": False, "error": {
                        "code": "capability_disabled",
                        "failure": {"limitations": ["ku_public_share_network_unavailable"]},
                    }}
                raise AssertionError(f"unexpected editor action {action}")

            with patch.object(demo, "start_web_host", side_effect=fake_start_web), \
                    patch.object(check.receiver.api, "editor", side_effect=fake_editor), \
                    contextlib.redirect_stdout(io.StringIO()):
                check.check_receiver_local_only_offline(prepared)
                self.assertFalse(captured_cfg["with_network"])
                self.assertNotIn("network", captured_cfg["json"])
                self.assertTrue(captured_cfg["path"].exists())
                check.receiver.stop()
                self.assertFalse(captured_cfg["path"].exists())

            self.assertEqual(len(check.results), 4)
            self.assertTrue(all(r["pass"] for r in check.results), check.results)


if __name__ == "__main__":
    unittest.main()
