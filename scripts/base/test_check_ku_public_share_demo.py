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
            "cli_share": False,
            "ack_timeout": 5,
            "seed_sample_ku": False,
            "seed_predicate_label": "water",
            "seed_text": "Water is a liquid.",
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

    def test_seed_sample_ku_uses_button_api_sequence_and_stops_on_ambiguity(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            sender_path, receiver_path, _ = self._write_host_configs(root)
            op = "ee" * 32
            source_ref = {"kind": "source", "id": "ff" * 32}
            calls: list[tuple] = []

            def make_editor(candidates):
                def fake_editor(action, req):
                    calls.append(("editor", action, req))
                    if action == "catalog":
                        return 200, {"ok": True, "data": {"payload": {
                            "sources": [{"source_ref": source_ref, "label": "note"}]}}}
                    if action == "resolve":
                        return 200, {"ok": True, "data": {"payload": {"candidates": candidates}}}
                    if action == "draft":
                        return 200, {"ok": True, "data": {"payload": {"draft": True}}}
                    raise AssertionError(f"unexpected editor action {action}")
                return fake_editor

            def fake_operation(name, req):
                calls.append(("operation", name, req))
                if name == "prepare":
                    self.assertEqual(req, {"draft": True})
                    return 200, {"ok": True, "data": {"payload": {
                        "validity": "ready", "object_cids": ["11" * 32]}}}
                if name == "save":
                    return 200, {"ok": True, "data": {"payload": {
                        "state": "committed", "published": False, "authorizes_reward": False}}}
                raise AssertionError(f"unexpected operation {name}")

            def fake_call(path, req=None):
                calls.append(("call", path, req))
                return 200, {"ok": True, "data": {"payload": {"operation_id": op}}}

            check = demo.Check(self._make_args(
                sender_path, receiver_path, skip_cli_client=True, seed_sample_ku=True,
                seed_predicate_label="water", seed_text="Water is a liquid."))
            with patch.object(check.api, "editor", side_effect=make_editor([{"ccid": "c1"}])), \
                    patch.object(check.api, "operation", side_effect=fake_operation), \
                    patch.object(check.api, "call", side_effect=fake_call), \
                    patch.object(check.api, "session", return_value={"s": 1}), \
                    contextlib.redirect_stdout(io.StringIO()):
                check.seed_sample_ku()
            self.assertEqual([c[1] for c in calls],
                             ["catalog", "resolve", "reservations", "draft", "prepare", "save"])
            draft = calls[3][2]
            self.assertEqual((draft["operation_id"], draft["idempotency_key"]), (op, op))
            self.assertEqual(draft["source_ref"], source_ref)
            self.assertEqual(draft["selected_ccid"], "c1")
            self.assertEqual(calls[5][2], {"operation_id": op, "idempotency_key": op,
                                           "object_cids": ["11" * 32]})
            self.assertEqual(len(check.results), 6)
            self.assertTrue(all(r["pass"] for r in check.results), check.results)

            # Ambiguous label: nothing is reserved, drafted or saved.
            calls.clear()
            check_amb = demo.Check(self._make_args(
                sender_path, receiver_path, skip_cli_client=True, seed_sample_ku=True,
                seed_predicate_label="water", seed_text="Water is a liquid."))
            with patch.object(check_amb.api, "editor",
                              side_effect=make_editor([{"ccid": "c1"}, {"ccid": "c2"}])), \
                    patch.object(check_amb.api, "operation", side_effect=fake_operation), \
                    patch.object(check_amb.api, "call", side_effect=fake_call), \
                    contextlib.redirect_stdout(io.StringIO()):
                check_amb.seed_sample_ku()
            self.assertEqual([c[1] for c in calls], ["catalog", "resolve"])
            self.assertFalse(check_amb.results[-1]["pass"])

    def test_cli_share_routes_share_actions_through_onebrain_ku_share(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            sender_path, receiver_path, receiver_secret = self._write_host_configs(root)
            cli_exe = root / "onebrain-fake"
            cli_exe.write_text("placeholder", encoding="utf-8")

            # Mapping: one sub-command per registered editor action; never confirm_public=false.
            self.assertEqual(demo.share_cli_args("share_list", {"query": "-x y", "limit": 5}),
                             ["share", "list", "--query=-x y", "--limit", "5"])
            self.assertEqual(demo.share_cli_args("share_confirm",
                                                 {"prepared_id": "ab" * 32, "confirm_public": True}),
                             ["share", "confirm", "--prepared-id", "ab" * 32])
            with self.assertRaises(ValueError):
                demo.share_cli_args("share_confirm", {"prepared_id": "ab" * 32,
                                                      "confirm_public": False})
            self.assertEqual(demo.share_cli_args("describe_ccids", {"ccids": ["07" * 16, "ab" * 16]}),
                             ["describe", "--ccid", "07" * 16, "--ccid", "ab" * 16])
            self.assertEqual(demo.share_cli_args("share_preview", {"prepared_id": "ab" * 32}),
                             ["share", "preview", "--prepared-id", "ab" * 32])
            self.assertIn("share_preview", demo.SENDER_SHARE_ACTIONS)

            # Missing binary fails before any process starts.
            with self.assertRaisesRegex(SystemExit, "--cli-share needs the CLI binary"):
                demo.Check(self._make_args(sender_path, receiver_path, cli_share=True,
                                           skip_cli_client=True))

            check = demo.Check(self._make_args(sender_path, receiver_path, sender_kind="cli",
                                               receiver_kind="cli", cli_exe=cli_exe,
                                               cli_share=True))
            prepared_id, intent_id, public_cid = "ab" * 32, "cd" * 32, "ef" * 32
            calls: list[tuple[list[str], dict]] = []
            preview_shown = f"{demo.CONFIRM_PREVIEW_MARKER} (read back)\n  public_object_cid: {public_cid}\n"

            def fake_run(argv, **kwargs):
                calls.append((argv, kwargs))
                self.assertIn("--api-token-file", argv)
                self.assertNotIn("--api-token", argv)
                self.assertEqual(kwargs["env"]["ONEBRAIN_API_TOKEN"], demo.IGNORED_ENV_TOKEN)
                res = MagicMock()
                typed = kwargs.get("input")
                if "confirm" in argv and typed != prepared_id + "\n":
                    res.returncode, res.stdout = 2, ""
                    res.stderr = (preview_shown + "vNext CLI error: public share confirmation "
                                  "rejected: type the exact prepared_id; nothing was confirmed")
                elif "intent" in argv:
                    res.returncode = 2
                    res.stdout = json.dumps({"ok": False, "error": {"code": "not_found"}})
                    res.stderr = "vNext CLI error: KU API failure"
                else:
                    self.assertEqual("stdin" in kwargs, typed is None)
                    res.returncode = 0
                    res.stdout = json.dumps({"ok": True, "data": {"payload": {"event": "x"}}})
                    res.stderr = (demo.DESCRIBE_SCOPE_MARKER + " display-only" if "describe" in argv
                                  else demo.SHARE_SCOPE_MARKER + " preview never enqueues")
                return res

            http_calls = []
            with patch.object(demo.subprocess, "run", side_effect=fake_run), \
                    patch.object(check.api, "editor",
                                 side_effect=lambda a, p: http_calls.append(a) or (400, {})), \
                    patch.object(check.receiver.api, "editor",
                                 side_effect=lambda a, p: http_calls.append("r:" + a) or (503, {})), \
                    contextlib.redirect_stdout(io.StringIO()):
                code, _ = check.editor("share_confirm", {"prepared_id": prepared_id,
                                                         "confirm_public": True})
                self.assertEqual(code, 200)
                # confirm_public=false stays on HTTP (host-side refusal).
                check.editor("share_confirm", {"prepared_id": prepared_id, "confirm_public": False})
                check.check_cli_confirm_mistyped({"prepared_id": prepared_id, "intent_id": intent_id,
                                                  "public_object_cid": public_cid}, False)
                # Receiver: CLI only while the networked host is up.
                check.receiver.online = True
                check.receiver.editor("share_list", {})
                check.receiver.editor("describe_ccids", {"ccids": ["07" * 16]})
                check.receiver.online = False
                check.receiver.editor("share_inspect", {"object_cid": "aa" * 32,
                                                        "selector": demo.SELECTOR})
                check.check_share_cli_summary()
            self.assertEqual(http_calls, ["share_confirm", "r:share_inspect"])
            self.assertEqual(calls[0][1]["input"], prepared_id + "\n")
            self.assertTrue(all(r["pass"] for r in check.results), check.results)

            # A token leak in any share call fails the summary.
            check.receiver.share_cli.leaks = 0
            with patch.object(demo.subprocess, "run", side_effect=lambda argv, **kw: MagicMock(
                    returncode=0, stdout="{}", stderr=f"{demo.SHARE_SCOPE_MARKER} {receiver_secret}")), \
                    contextlib.redirect_stdout(io.StringIO()):
                check.receiver.online = True
                check.receiver.editor("share_list", {})
                check.check_share_cli_summary()
            self.assertFalse(check.results[-1]["pass"])

    def test_prepared_share_restart_reads_back_exact_preview_and_replay_is_refused(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            sender_path, receiver_path, _ = self._write_host_configs(root)
            check = demo.Check(self._make_args(sender_path, receiver_path, skip_cli_client=True))
            prepared = {"prepared_id": "ab" * 32, "public_object_cid": "ef" * 32,
                        "intent_id": "cd" * 32, "canonical_base64": "AAE=",
                        "expires_at": 1_800_000_000, "enqueued": False, "payload_sent": False}
            old_web = MagicMock()
            old_web.p.returncode = None
            old_web.stop.side_effect = lambda graceful: setattr(old_web.p, "returncode", 1)
            check.web = old_web
            sid = "11" * 32
            calls: list[tuple[str, dict]] = []
            preview = dict(prepared)

            def fake_editor(action, body):
                calls.append((action, body))
                if action == "share_preview":
                    return 200, {"ok": True, "data": {"payload": preview}}
                if action == "share_confirm":
                    return 404, {"ok": False, "error": {"code": "not_found",
                                                        "message": "prepared_share_missing"}}
                return 404, {"ok": False, "error": {"code": "not_found"}}

            with patch.object(check, "start_web", return_value=(sid, "127.0.0.1:1")) as start, \
                    patch.object(check.api, "editor", side_effect=fake_editor), \
                    contextlib.redirect_stdout(io.StringIO()):
                self.assertTrue(check.check_prepared_survives_sender_restart(prepared, sid, False))
                check.check_confirm_replay_refused(prepared)
            old_web.stop.assert_called_once_with(graceful=False)
            start.assert_called_once()
            self.assertEqual([a for a, _ in calls], ["share_preview", "share_intent", "share_confirm"])
            self.assertIs(calls[-1][1]["confirm_public"], True)
            self.assertTrue(all(r["pass"] for r in check.results), check.results)

            # A changed preview after restart (e.g. extended expiry) fails the check.
            preview["expires_at"] = prepared["expires_at"] + 900
            check.web, check.results = old_web, []
            old_web.p.returncode = None
            with patch.object(check, "start_web", return_value=(sid, "127.0.0.1:1")), \
                    patch.object(check.api, "editor", side_effect=fake_editor), \
                    contextlib.redirect_stdout(io.StringIO()):
                self.assertFalse(check.check_prepared_survives_sender_restart(prepared, sid, True))
            self.assertFalse(check.results[-1]["pass"])


if __name__ == "__main__":
    unittest.main()
