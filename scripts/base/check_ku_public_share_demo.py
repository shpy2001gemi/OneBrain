#!/usr/bin/env python3
"""Run the task-20 D-045 local-to-peer concept as one repeatable local check.

Starts two real loopback processes on this host and drives the documented
authenticated ``POST /api/vnext/ku/editor`` share actions:

* sender: ``ku_local_web`` built with ``--features vnext-network-runtime`` and a
  host config that contains an opt-in ``"network"`` block, or with
  ``--sender-kind cli`` the ``onebrain start --api --ku-network-bind`` host
  started from the same config fields (data_dir, port, api_token_file,
  registry_root/public key, vault_key_file, sources, ollama, web_dir, network);
* receiver: an isolated ``ku_peer_demo`` (no Registry, Vault key or API token),
  or with ``--receiver-kind web`` a second ``ku_local_web`` host (its own data
  dir, port and opt-in ``"network"`` block). A Web receiver is inspected through
  its authenticated ``share_inspect`` editor action and ``get`` operation, the
  same calls as fieldset ``4. Inspect accepted Public KU`` on its ``/ku`` page.

It checks prepare (nothing enqueued) -> refused ``confirm_public=false`` ->
explicit confirm -> ``Acknowledged`` intent -> receiver inspection of the same
Public CID/bytes with authenticated sender provenance, then restarts both
processes and checks durability and the idempotent same-intent retry.

API tokens are read from each ``ku_local_web`` config's ``api_token_file`` and
are never printed. Private ObjectCIDs are never printed. Use a COPY of your dataset as the
sender ``data_dir`` and a fresh, separate receiver ``data_dir``: the opt-in
network runtime writes node state under each.
This is a concept check on one host, not NAT/production/model qualification.
"""
from __future__ import annotations

import argparse
import base64
import json
import queue
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
EXE = ".exe" if sys.platform == "win32" else ""
DEFAULT_WEB = REPO / "src" / "target" / "debug" / "examples" / f"ku_local_web{EXE}"
DEFAULT_PEER = REPO / "src" / "target" / "debug" / "examples" / f"ku_peer_demo{EXE}"
DEFAULT_CLI = REPO / "src" / "target" / "debug" / f"onebrain{EXE}"
# Synthetic demo scope from the task-20 run instructions; grants no authority.
SELECTOR = "2" * 64
NAMESPACE = "3" * 64
BUDGET = {"max_items": 64, "max_bytes": 1048576, "max_work_units": 1000000}


class Proc:
    """A child process whose stdout/stderr lines are pumped into a queue."""

    def __init__(self, args: list[str], name: str, stdin: bool = False):
        self.name = name
        self.lines: queue.Queue[tuple[str, str]] = queue.Queue()
        self.p = subprocess.Popen(
            args,
            cwd=REPO,
            stdin=subprocess.PIPE if stdin else subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
        )
        self.pumps = [threading.Thread(target=self._pump, args=(stream, tag), daemon=True)
                      for stream, tag in ((self.p.stdout, "out"), (self.p.stderr, "err"))]
        for pump in self.pumps:
            pump.start()

    def _pump(self, stream, tag: str) -> None:
        for line in stream:
            self.lines.put((tag, line.rstrip("\n")))

    def next_line(self, pred, timeout: float = 60) -> str:
        end = time.time() + timeout
        while time.time() < end:
            try:
                tag, line = self.lines.get(timeout=min(0.5, max(0.1, end - time.time())))
            except queue.Empty:
                # Fail fast with the exit code instead of waiting out the timeout
                # (e.g. a config the binary rejects, printed above as stderr).
                if (self.p.poll() is not None and self.lines.empty()
                        and not any(pump.is_alive() for pump in self.pumps)):
                    raise RuntimeError(f"{self.name} exited with code {self.p.returncode} "
                                       "before the expected output; see its stderr above")
                continue
            if tag == "err":
                print(f"  ({self.name} stderr) {line[:300]}", flush=True)
                continue
            if pred(line):
                return line
        raise TimeoutError(f"{self.name}: expected output not seen within {timeout}s")

    def command(self, obj: dict, timeout: float = 60) -> dict:
        self.p.stdin.write(json.dumps(obj) + "\n")
        self.p.stdin.flush()
        return json.loads(self.next_line(lambda l: l.startswith("{"), timeout))

    def stop(self, graceful: bool) -> None:
        if self.p.poll() is not None:
            return
        try:
            if graceful:
                self.p.stdin.write(json.dumps({"command": "shutdown"}) + "\n")
                self.p.stdin.flush()
                self.p.wait(30)
                return
        except Exception:
            pass
        # Same as the documented stop-web.ps1 path (Stop-Process).
        self.p.terminate()
        try:
            self.p.wait(30)
        except subprocess.TimeoutExpired:
            self.p.kill()


def load_host_config(path: Path, role: str) -> dict:
    cfg = json.loads(Path(path).read_text(encoding="utf-8-sig"))
    network = cfg.get("network") or {}
    if network.get("network_opt_in") is not True:
        raise SystemExit(f"{role} config needs an opt-in \"network\" block (see task 20)")
    return cfg


class Api:
    """Authenticated ``/api/vnext/ku`` client for one ``ku_local_web`` host."""

    def __init__(self, cfg: dict):
        self.port = int(cfg.get("port", 4280))
        self.base = f"http://127.0.0.1:{self.port}/api/vnext/ku"
        self._token = Path(cfg["api_token_file"]).read_text(encoding="utf-8").strip()

    def call(self, path: str, payload=None) -> tuple[int, dict]:
        req = urllib.request.Request(
            f"{self.base}/{path}",
            data=None if payload is None else json.dumps(payload).encode(),
            method="GET" if payload is None else "POST",
            headers={"Authorization": f"Bearer {self._token}", "Content-Type": "application/json"},
        )
        try:
            with urllib.request.urlopen(req, timeout=60) as response:
                return response.status, json.loads(response.read())
        except urllib.error.HTTPError as error:
            return error.code, json.loads(error.read() or b"{}")

    def session(self) -> dict:
        code, env = self.call("status")
        if code != 200 or not env.get("ok"):
            raise RuntimeError(f"status failed: {code} {error_code(env)}")
        return env["data"]["session"]

    def editor(self, action: str, payload: dict) -> tuple[int, dict]:
        return self.call("editor", {"session": self.session(), "budget": BUDGET,
                                    "request": {"action": action, "payload": payload}})

    def operation(self, op: str, payload: dict) -> tuple[int, dict]:
        return self.call("operations", {"session": self.session(), "budget": BUDGET,
                                        "request": {"operation": op, "payload": payload}})


def start_web_host(exe: Path, config: Path, port: int, name: str) -> tuple[Proc, str, str]:
    """Start ``ku_local_web`` and return (process, node_id, listen_addr)."""
    proc = Proc([str(exe), str(config)], name)
    return wait_host_ready(proc, port)


def wait_host_ready(proc: Proc, port: int) -> tuple[Proc, str, str]:
    line = proc.next_line(lambda l: "Opt-in loopback OBP-RP:" in l, 300)
    fields = dict(p.split("=", 1) for p in line.split() if "=" in p)
    for _ in range(240):
        try:
            urllib.request.urlopen(f"http://127.0.0.1:{port}/ku", timeout=2)
            break
        except Exception:
            time.sleep(0.5)
    return proc, fields["node_id"], fields["listen_addr"]


def start_cli_host(exe: Path, cfg: dict, name: str) -> tuple[Proc, str, str]:
    """Start ``onebrain start --api`` from the same ``ku_local_web`` JSON fields.

    Uses ``--api-token-file`` and ``--ku-network-bind`` (binary built with
    ``--features vnext-network-runtime``), plus ``--ku-ollama-*`` if ``ollama``
    is configured. The token stays in ``api_token_file`` (never passed on argv
    or printed). stdin stays open because the REPL exits on EOF.
    """
    network = cfg["network"]
    args = [str(exe), "start", "--data-dir", str(cfg["data_dir"]), "--api",
            "--api-port", str(int(cfg.get("port", 4280))),
            "--api-token-file", str(cfg["api_token_file"]),
            "--concept-registry-release-root", str(cfg["registry_root"]),
            "--concept-registry-release-public-key", cfg["registry_public_key"],
            "--ku-vault-key-file", str(cfg["vault_key_file"]),
            "--ku-network-bind", network["bind_addr"]]
    for source in cfg.get("sources", []):
        args += ["--ku-source", f"{source['label']}={source['canonical_file']}"]
    ollama = cfg.get("ollama") or {}
    if ollama:
        args += ["--ku-ollama-executable", str(ollama["executable"]),
                 "--ku-ollama-models-dir", str(ollama["models_dir"])]
        for model in ollama.get("models", []):
            args += ["--ku-ollama-model", model]
        if ollama.get("memory_limit_bytes") is not None:
            args += ["--ku-ollama-memory-limit-bytes", str(ollama["memory_limit_bytes"])]
    if cfg.get("web_dir"):
        args += ["--web-dir", str(cfg["web_dir"])]
    proc = Proc(args, name, stdin=True)
    return wait_host_ready(proc, int(cfg.get("port", 4280)))


class PeerReceiver:
    """Isolated ``ku_peer_demo`` receiver inspected over its stdin commands."""

    kind = "ku_peer_demo"

    def __init__(self, args: argparse.Namespace):
        self.args = args
        self.proc: Proc | None = None

    def start(self) -> tuple[str, str]:
        self.proc = Proc([str(self.args.peer_exe), str(self.args.receiver_config)], "receiver", stdin=True)
        started = json.loads(self.proc.next_line(lambda l: l.startswith("{")))
        return started["node_id"], started["listen_addr"]

    def inspect(self, public_cid: str) -> dict:
        return self.proc.command({"command": "inspect", "object_cid": public_cid, "selector": SELECTOR})

    def stop(self) -> None:
        if self.proc:
            self.proc.stop(graceful=True)

    def stopped(self) -> bool:
        return self.proc is not None and self.proc.p.returncode is not None


class WebReceiver:
    """Second ``ku_local_web`` receiver inspected through its authenticated API."""

    kind = "ku_local_web"

    def __init__(self, args: argparse.Namespace):
        self.args = args
        self.cfg = load_host_config(args.receiver_config, "receiver")
        self.api = Api(self.cfg)
        self.proc: Proc | None = None

    def start(self) -> tuple[str, str]:
        self.proc, node_id, addr = start_web_host(self.args.receiver_exe or self.args.web_exe,
                                                  self.args.receiver_config, self.api.port, "receiver")
        return node_id, addr

    def inspect(self, public_cid: str) -> dict:
        # Same call as the receiver page's fieldset 4; poll briefly for arrival.
        code, env = 0, {}
        for _ in range(30):
            code, env = self.api.editor("share_inspect", {"object_cid": public_cid, "selector": SELECTOR})
            if ok(code, env):
                return payload(env)
            time.sleep(1)
        return {"event": "error", "code": f"{code} {error_code(env)}"}

    def stop(self) -> None:
        if self.proc:
            self.proc.stop(graceful=False)

    def stopped(self) -> bool:
        return self.proc is not None and self.proc.p.returncode is not None


class Check:
    def __init__(self, args: argparse.Namespace):
        self.args = args
        self.cfg = load_host_config(args.sender_config, "sender")
        self.api = Api(self.cfg)
        self.port = self.api.port
        self.receiver = WebReceiver(args) if args.receiver_kind == "web" else PeerReceiver(args)
        if isinstance(self.receiver, WebReceiver) and self.receiver.api.port == self.port:
            raise SystemExit("receiver config must use a different port than the sender")
        self.results: list[dict] = []
        self.web: Proc | None = None

    # ---- reporting -------------------------------------------------------
    def check(self, name: str, ok, detail: str = "") -> bool:
        self.results.append({"check": name, "pass": bool(ok), "detail": detail})
        print(f"[{'PASS' if ok else 'FAIL'}] {name} {detail}".rstrip(), flush=True)
        return bool(ok)

    # ---- processes ---------------------------------------------------------
    def start_web(self) -> tuple[str, str]:
        if self.args.sender_kind == "cli":
            self.web, node_id, addr = start_cli_host(self.args.cli_exe, self.cfg, "sender")
        else:
            self.web, node_id, addr = start_web_host(self.args.web_exe, self.args.sender_config,
                                                     self.port, "sender")
        return node_id, addr

    # ---- authenticated sender KU API ---------------------------------------
    def editor(self, action: str, payload: dict) -> tuple[int, dict]:
        return self.api.editor(action, payload)

    def operation(self, op: str, payload: dict) -> tuple[int, dict]:
        return self.api.operation(op, payload)

    # ---- journey -----------------------------------------------------------
    def run(self) -> None:
        rid, raddr = self.receiver.start()
        self.check(f"receiver {self.receiver.kind} started", len(rid) == 64, f"node_id={rid} addr={raddr}")
        sid, saddr = self.start_web()
        sender = "onebrain start --api" if self.args.sender_kind == "cli" else "ku_local_web"
        self.check(f"sender {sender} opt-in network started", len(sid) == 64 and sid != rid,
                   f"node_id={sid} addr={saddr}")

        code, env = self.operation("list", {"limit": 64})
        items = payload(env).get("items", []) if code == 200 else []
        self.check("list saved local KUs", items, f"count={len(items)}")

        prepared, private_cid = None, None
        for index, item in enumerate(items):
            code, env = self.editor("share_prepare", {
                "object_cid": item["object_cid"], "expected_node_id": rid, "address": raddr,
                "selector": SELECTOR, "namespace": NAMESPACE})
            if code == 200 and env.get("ok"):
                prepared, private_cid = payload(env), item["object_cid"]
                break
            print(f"  saved item #{index}: not shareable {error_code(env)}", flush=True)
        if not self.check("share_prepare on a supported saved LOCAL_ONLY KU", prepared):
            return
        pub, intent_id = prepared["public_object_cid"], prepared["intent_id"]
        self.check("prepare: nothing enqueued or sent",
                   prepared["enqueued"] is False and prepared["payload_sent"] is False)
        self.check("prepare: new Public CID differs from the private CID", pub != private_cid,
                   f"public={pub}")
        statements = prepared["semantic"]["statements"]
        self.check("prepare: one predicate + one text literal", len(statements) == 1,
                   json.dumps(statements, ensure_ascii=False))
        canonical = base64.b64decode(prepared["canonical_base64"])
        private = [private_cid.encode()]
        if len(private_cid) == 64:
            private.append(bytes.fromhex(private_cid))
        for source in self.cfg.get("sources", []):
            private += [source["label"].encode(), source["canonical_file"].encode()]
        self.check("public bytes exclude private CID and source label/path",
                   not any(p in canonical for p in private), f"bytes={len(canonical)}")

        code, env = self.editor("share_intent", {"intent_id": intent_id})
        if ok(code, env):
            # Rerun on state from an earlier run: the durable intent already exists,
            # so this run cannot show that prepare alone enqueues nothing.
            print(f"[INFO] intent already exists from an earlier run (state="
                  f"{payload(env).get('state')}); use fresh data dirs to check prepare-only", flush=True)
        else:
            self.check("prepare alone creates no outbound intent", True, f"{code} {error_code(env)}")
        code, env = self.editor("share_confirm", {"prepared_id": prepared["prepared_id"],
                                                  "confirm_public": False})
        self.check("confirm_public=false is refused", not ok(code, env), f"{code} {error_code(env)}")
        code, env = self.editor("share_confirm", {"prepared_id": prepared["prepared_id"],
                                                  "confirm_public": True})
        confirmed = payload(env) if ok(code, env) else {}
        self.check("explicit share_confirm creates the durable intent",
                   confirmed.get("intent_id") == intent_id and confirmed.get("authorizes_reward") is False,
                   f"{code} outbox={confirmed.get('outbox')}")
        self.check_delivery_note(confirmed, "")

        state, view = None, {}
        for _ in range(self.args.ack_timeout):
            code, env = self.editor("share_intent", {"intent_id": intent_id})
            if ok(code, env):
                view = payload(env)
                state = view["state"]
                if state == "Acknowledged":
                    break
            time.sleep(1)
        self.check("sender intent reaches Acknowledged", state == "Acknowledged",
                   f"state={state} claims_network_completion={view.get('claims_network_completion')}")

        received = self.receiver.inspect(pub)
        self.check_received(received, prepared, sid, "")
        self.check_receiver_public_get(prepared, "")
        code, env = self.operation("get", {"object_cid": private_cid})
        self.check("private KU stays readable on the sender", ok(code, env))

        # Restart both: receiver gracefully, sender as stop-web.ps1 does.
        self.receiver.stop()
        self.web.stop(graceful=False)
        self.check("both processes stopped", self.receiver.stopped()
                   and self.web.p.returncode is not None)

        rid2, raddr2 = self.receiver.start()
        self.check("receiver restart keeps its NodeID", rid2 == rid, f"addr={raddr2}")
        received = self.receiver.inspect(pub)
        self.check_received(received, prepared, sid, " after restart")
        self.check_receiver_public_get(prepared, " after restart")
        sid2, _ = self.start_web()
        self.check("sender restart keeps its NodeID", sid2 == sid)
        code, env = self.editor("share_intent", {"intent_id": intent_id})
        state = payload(env).get("state") if ok(code, env) else error_code(env)
        self.check("sender intent still Acknowledged after restart", state == "Acknowledged",
                   f"state={state}")
        code, env = self.editor("share_prepare", {
            "object_cid": private_cid, "expected_node_id": rid, "address": raddr2,
            "selector": SELECTOR, "namespace": NAMESPACE})
        again = payload(env) if ok(code, env) else {}
        self.check("fresh prepare after restart: same Public CID and intent id",
                   again.get("public_object_cid") == pub and again.get("intent_id") == intent_id)
        if again:
            code, env = self.editor("share_confirm", {"prepared_id": again["prepared_id"],
                                                      "confirm_public": True})
            retried = payload(env) if ok(code, env) else {}
            self.check("re-confirm reuses the same intent", retried.get("intent_id") == intent_id,
                       f"{code} outbox={retried.get('outbox')}")
            self.check_delivery_note(retried, " (re-confirm)")
        code, env = self.operation("get", {"object_cid": private_cid})
        self.check("private KU readable after restart", ok(code, env))

    def check_delivery_note(self, confirmed: dict, when: str) -> None:
        # Only a newly Added intent is pending at confirmation; a reused intent
        # (Existing/RouteUpdated) keeps its state and must not claim "pending".
        outbox, delivery = confirmed.get("outbox"), str(confirmed.get("delivery", ""))
        expected = delivery.startswith("pending") if outbox == "Added" else (
            outbox in ("Existing", "RouteUpdated") and "pending" not in delivery
            and delivery.endswith("inspect intent for its current state"))
        self.check(f"confirm{when}: delivery note matches outbox outcome", expected,
                   f"outbox={outbox} delivery={delivery!r}")

    def check_receiver_public_get(self, prepared: dict, when: str) -> None:
        # Only a Web receiver has the registered operations API (Saved local knowledge).
        if not isinstance(self.receiver, WebReceiver):
            return
        pub_cid = prepared["public_object_cid"]
        sem_cid = prepared["semantic_content_cid"]
        code, env = self.receiver.api.operation("get", {"object_cid": pub_cid})
        view = payload(env) if ok(code, env) else {}
        self.check(f"receiver{when}: get serves the accepted KU as PUBLIC, same semantic identity",
                   view.get("disclosure_class") == "PUBLIC"
                   and view.get("semantic_content_cid") == sem_cid
                   and view.get("executable") is False,
                   f"{code} {view.get('disclosure_class') or error_code(env)}")
        code, env = self.receiver.api.operation("list", {"limit": 64})
        items = payload(env).get("items", []) if ok(code, env) else None
        self.check(f"receiver{when}: private Saved local knowledge list stays separate (0 owned items)",
                   items == [],
                   f"{code} count={None if items is None else len(items)}")
        code, env = self.receiver.api.editor("share_list", {})
        listed_items = payload(env).get("items", []) if ok(code, env) else []
        listed = next((item for item in listed_items if item.get("object_cid") == pub_cid), None)
        self.check(f"receiver{when}: share_list browses the accepted Public KU",
                   listed is not None
                   and listed.get("semantic", {}).get("semantic_content_cid") == sem_cid
                   and listed.get("authorizes_reward") is False,
                   f"{code} count={len(listed_items)}")
        stmt = (prepared.get("semantic", {}).get("statements") or [{}])[0]
        query = (stmt.get("text_arguments") or [stmt.get("predicate_ccid", "")])[0][:64]
        code, env = self.receiver.api.editor("share_list", {"query": query})
        hits = payload(env).get("items", []) if ok(code, env) else []
        found = next((item for item in hits if item.get("object_cid") == pub_cid), None)
        self.check(f"receiver{when}: share_list filter finds the accepted Public KU",
                   found is not None,
                   f"{code} hits={len(hits)}")

    def check_received(self, view: dict, prepared: dict, sender: str, when: str) -> None:
        self.check(f"receiver inspect{when}: public_object", view.get("event") == "public_object",
                   view.get("code", ""))
        if view.get("event") != "public_object":
            return
        self.check(f"receiver{when}: same Public CID, bytes and semantic identity",
                   view["object_cid"] == prepared["public_object_cid"]
                   and view["canonical_base64"] == prepared["canonical_base64"]
                   and view["semantic"]["semantic_content_cid"] == prepared["semantic_content_cid"])
        self.check(f"receiver{when}: authenticated sender provenance, fidelity unassessed, no reward",
                   sender in view.get("authenticated_source_peers", [])
                   and view.get("fidelity") == "unassessed" and view.get("authorizes_reward") is False)

    def finish(self) -> int:
        self.receiver.stop()
        if self.web:
            self.web.stop(graceful=False)
        passed = sum(r["pass"] for r in self.results)
        print(f"SUMMARY {passed}/{len(self.results)} passed", flush=True)
        if self.args.result:
            Path(self.args.result).write_text(json.dumps(
                {"run": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "checks": self.results}, indent=1),
                encoding="utf-8")
        return 0 if self.results and passed == len(self.results) else 1


def ok(code: int, env: dict) -> bool:
    return code == 200 and bool(env.get("ok"))


def payload(env: dict) -> dict:
    return (env.get("data") or {}).get("payload") or {}


def error_code(env: dict):
    error = env.get("error") or {}
    return error.get("code")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--sender-config", required=True, type=Path,
                        help="ku_local_web host JSON with an opt-in \"network\" block")
    parser.add_argument("--receiver-config", required=True, type=Path,
                        help="ku_peer_demo JSON (data_dir, bind_addr 127.0.0.1:0, network_opt_in), "
                             "or with --receiver-kind web a second ku_local_web host JSON")
    parser.add_argument("--receiver-kind", choices=("peer", "web"), default="peer",
                        help="peer: ku_peer_demo (default); web: second ku_local_web host")
    parser.add_argument("--receiver-exe", type=Path,
                        help="ku_local_web binary for --receiver-kind web (default: --web-exe)")
    parser.add_argument("--sender-kind", choices=("web", "cli"), default="web",
                        help="web: ku_local_web (default); cli: onebrain start --api "
                             "--ku-network-bind built from the same sender JSON fields")
    parser.add_argument("--cli-exe", type=Path, default=DEFAULT_CLI,
                        help="onebrain binary built with --features vnext-network-runtime")
    parser.add_argument("--web-exe", type=Path, default=DEFAULT_WEB)
    parser.add_argument("--peer-exe", type=Path, default=DEFAULT_PEER)
    parser.add_argument("--ack-timeout", type=int, default=60, help="seconds to wait for Acknowledged")
    parser.add_argument("--result", type=Path, help="optional JSON file for the check list")
    args = parser.parse_args(argv)
    check = Check(args)
    try:
        check.run()
    except Exception as error:  # report, then always stop the processes
        check.check("journey completed without error", False, type(error).__name__ + ": " + str(error)[:200])
    return check.finish()


if __name__ == "__main__":
    sys.exit(main())
