#!/usr/bin/env python3
"""One-command first-run bootstrap for the local ``/ku`` concept demo (task 20.C, D-056).

Standard library only. Run from any directory; paths below are examples.

Check prerequisites only::

    python scripts/base/bootstrap_ku_dev_demo.py --check-only

Fresh clone, DEV-ONLY synthetic Registry (explicit opt-in)::

    python scripts/base/bootstrap_ku_dev_demo.py --demo-root C:/OneBrainLocal/dev-demo --dev-demo-registry

Optional ``--with-receiver`` also writes a second host config and opt-in loopback
``"network"`` blocks so ``check_ku_public_share_demo.py --receiver-kind web`` can
run the D-045 two-node share check.

Operator-supplied real Registry (already verified and deliberately activated)::

    python scripts/base/bootstrap_ku_dev_demo.py --demo-root C:/OneBrainLocal/demo \
        --registry-root C:/OneBrainLocal/registry \
        --registry-public-key-file C:/OneBrainLocal/trust/registry-public-key.txt

The command composes existing tools only: ``concept_registry_dev_demo`` (dev mode)
or read-only ``concept_registry_release status`` (real mode),
``prepare_ku_local_secrets.py`` and ``ku_manual_source``. It never prints the API
token or Vault key, never overwrites existing files, never saves or shares a KU
and never starts a host. On failure the partial demo root is retained; choose a
new ``--demo-root`` for the retry.

The DEV-ONLY Registry is a self-issued synthetic release: it is not operator
Registry trust, its CCIDs are not compatible with real releases, and KUs saved
against it are demo data only.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable, Sequence

REPO = Path(__file__).resolve().parents[2]
MANIFEST = REPO / "src" / "Cargo.toml"
WEB_DIST = REPO / "src" / "onebrain-web" / "dist"
SECRETS_HELPER = REPO / "scripts" / "base" / "prepare_ku_local_secrets.py"
SAMPLE_TEXT = "Water is a liquid.\n"
SAMPLE_OPERATOR = "dev-demo-contributor"
PUBLIC_KEY_RE = re.compile(r"^[0-9a-f]{64}$")
RELEASE_RE = re.compile(r"^ku_dev_demo_registry_ready release_id=([A-Za-z0-9._-]{1,96})$", re.M)
NETWORK_BLOCK = {"network_opt_in": True, "bind_addr": "127.0.0.1:0"}

Runner = Callable[[Sequence[str]], "subprocess.CompletedProcess[str]"]


class BootstrapError(Exception):
    """Actionable guidance; never contains token or key bytes."""


@dataclass
class Prerequisite:
    name: str
    ok: bool
    detail: str
    required: bool


@dataclass
class Plan:
    demo_root: Path
    dev_registry: bool
    registry_root: Path
    public_key_file: Path
    port: int
    with_receiver: bool
    receiver_port: int
    hosts: list[str] = field(default_factory=list)


def default_runner(argv: Sequence[str]) -> "subprocess.CompletedProcess[str]":
    return subprocess.run(list(argv), cwd=REPO, capture_output=True, text=True,
                          encoding="utf-8", errors="replace")


def _tool_version(tool: str, runner: Runner) -> tuple[bool, str]:
    path = shutil.which(tool)
    if not path:
        return False, "not found on PATH"
    try:
        result = runner([path, "--version"])
    except OSError:
        return False, "found but could not be executed"
    text = (result.stdout or result.stderr or "").strip().splitlines()
    return result.returncode == 0, text[0] if text else f"exit {result.returncode}"


def check_prerequisites(runner: Runner = default_runner) -> list[Prerequisite]:
    checks = [Prerequisite(
        "python>=3.10", sys.version_info >= (3, 10), sys.version.split()[0], True)]
    ok, detail = _tool_version("cargo", runner)
    checks.append(Prerequisite("cargo (Rust toolchain)", ok,
                               detail if ok else f"{detail}; install Rust from https://rustup.rs",
                               True))
    if default_posix():
        ok, detail = _tool_version("cc", runner)
        checks.append(Prerequisite(
            "cc (C linker for Rust builds)", ok,
            detail if ok else f"{detail}; install build-essential (Debian/Ubuntu) or the Xcode "
            "Command Line Tools (macOS)", False))
    ok, detail = _tool_version("npm", runner)
    checks.append(Prerequisite("npm (Node.js, builds /ku Web)", ok,
                               detail if ok else f"{detail}; install Node.js 20.19+ or 22.12+ "
                               "(Vite 8) to build the Web",
                               False))
    has_dist = (WEB_DIST / "index.html").is_file()
    checks.append(Prerequisite(
        "src/onebrain-web/dist (built /ku Web)", has_dist,
        "present" if has_dist else "missing; run: cd src/onebrain-web && npm ci && npm run build",
        False))
    return checks


def print_prerequisites(checks: list[Prerequisite]) -> bool:
    for check in checks:
        status = "OK  " if check.ok else ("FAIL" if check.required else "WARN")
        kind = "required" if check.required else "optional"
        print(f"  [{status}] {check.name} ({kind}): {check.detail}", flush=True)
    return all(check.ok for check in checks if check.required)


def _outside_repo(path: Path, field_name: str) -> Path:
    resolved = path.resolve()
    if resolved == REPO or resolved.is_relative_to(REPO):
        raise BootstrapError(
            f"ku_dev_demo_repository_path: keep {field_name} outside the repository; "
            "demo datasets, secrets and Registry files must never be committed.")
    return resolved


def build_plan(args: argparse.Namespace) -> Plan:
    if args.demo_root is None:
        raise BootstrapError("ku_dev_demo_root_required: pass --demo-root <new directory "
                             "outside the repository> (or use --check-only).")
    real = args.registry_root is not None or args.registry_public_key_file is not None
    if args.dev_demo_registry and real:
        raise BootstrapError(
            "ku_dev_demo_mode_conflict: choose either --dev-demo-registry or the real "
            "--registry-root/--registry-public-key-file pair, not both.")
    if not args.dev_demo_registry and not real:
        raise BootstrapError(
            "ku_dev_demo_registry_required: the local /ku host requires a signed, activated "
            "Concept Registry. Pass --registry-root and --registry-public-key-file for an "
            "independently trusted release you already verified/activated, or explicitly "
            "opt in to --dev-demo-registry for a DEV-ONLY synthetic Registry (not operator "
            "trust; CCIDs incompatible with real releases).")
    if real and (args.registry_root is None or args.registry_public_key_file is None):
        raise BootstrapError(
            "ku_dev_demo_registry_incomplete: pass both --registry-root and "
            "--registry-public-key-file for a real Registry.")
    for port_name in ("port", "receiver_port"):
        value = getattr(args, port_name)
        if value is not None and not 1 <= value <= 65535:
            raise BootstrapError(f"ku_dev_demo_port_invalid: --{port_name.replace('_', '-')} "
                                 "must be 1..65535.")
    receiver_port = args.receiver_port if args.receiver_port is not None else args.port + 1
    if args.with_receiver and receiver_port == args.port:
        raise BootstrapError("ku_dev_demo_port_invalid: sender and receiver ports must differ.")

    demo_root = _outside_repo(args.demo_root, "--demo-root")
    if os.path.lexists(demo_root):
        raise BootstrapError(
            "ku_dev_demo_root_exists: --demo-root must not exist. Existing demo custody is "
            "never modified; choose a new directory name.")
    if not demo_root.parent.is_dir():
        raise BootstrapError(
            "ku_dev_demo_parent_missing: create the parent of --demo-root first (a private "
            "location outside the repository); this command does not create parents.")
    if real:
        registry_root = _outside_repo(args.registry_root, "--registry-root")
        public_key_file = args.registry_public_key_file.resolve()
        if not registry_root.is_dir():
            raise BootstrapError(
                "ku_dev_demo_registry_root_missing: --registry-root must be an existing "
                "Registry root containing releases/ and activation state/.")
    else:
        registry_root = demo_root / "registry"
        public_key_file = demo_root / "trust" / "registry-public-key.txt"
    return Plan(demo_root=demo_root, dev_registry=not real, registry_root=registry_root,
                public_key_file=public_key_file, port=args.port,
                with_receiver=args.with_receiver, receiver_port=receiver_port)


def _run_step(runner: Runner, step: str, argv: Sequence[str], guidance: str) -> str:
    print(f"- {step} ...", flush=True)
    try:
        result = runner(argv)
    except OSError:
        raise BootstrapError(f"ku_dev_demo_step_failed: step={step}: could not execute the "
                             f"tool. {guidance}") from None
    if result.returncode != 0:
        tail = "\n".join((result.stderr or "").strip().splitlines()[-6:])
        raise BootstrapError(
            f"ku_dev_demo_step_failed: step={step} exit={result.returncode}. {guidance}"
            + (f"\n  tool stderr (tail):\n  {tail}" if tail else ""))
    return result.stdout or ""


def _cargo_example(package: str, example: str, *args: str,
                   features: Sequence[str] = ()) -> list[str]:
    argv = ["cargo", "run", "--locked", "--quiet", "--manifest-path", str(MANIFEST),
            "-p", package]
    if features:
        argv += ["--features", ",".join(features)]
    return argv + ["--example", example, "--", *args]


def read_public_key(path: Path) -> str:
    try:
        value = path.read_text(encoding="utf-8").strip()
    except (OSError, UnicodeDecodeError):
        raise BootstrapError("ku_dev_demo_public_key_unreadable: cannot read the Registry "
                             "public-key file as UTF-8 text.") from None
    if not PUBLIC_KEY_RE.fullmatch(value):
        raise BootstrapError("ku_dev_demo_public_key_invalid: the Registry public-key file must "
                             "contain exactly 64 lowercase hexadecimal characters.")
    return value


def host_config(plan: Plan, public_key: str, role: str, *, sources: list[dict]) -> dict:
    base = plan.demo_root / role
    config = {
        "data_dir": (base / "dataset").as_posix(),
        "registry_root": plan.registry_root.as_posix(),
        "registry_public_key": public_key,
        "vault_key_file": (base / "secrets" / "vault.key").as_posix(),
        "api_token_file": (base / "secrets" / "api-token.txt").as_posix(),
        "sources": sources,
        "web_dir": WEB_DIST.as_posix(),
        "port": plan.port if role == "sender" else plan.receiver_port,
    }
    if plan.with_receiver:
        config["network"] = dict(NETWORK_BLOCK)
    return config


def _write_new(path: Path, text: str) -> None:
    # Exclusive creation, UTF-8 without BOM (accepted by every demo binary).
    with open(path, "x", encoding="utf-8", newline="\n") as handle:
        handle.write(text)


def bootstrap(plan: Plan, runner: Runner = default_runner) -> None:
    plan.demo_root.mkdir()
    if plan.dev_registry:
        (plan.demo_root / "trust").mkdir()
        out = _run_step(
            runner, "create DEV-ONLY synthetic Registry",
            _cargo_example("ku-core", "concept_registry_dev_demo", str(plan.registry_root),
                           str(plan.public_key_file)),
            "Check the Rust toolchain (cargo build) and free disk space.")
        match = RELEASE_RE.search(out)
        if not match:
            raise BootstrapError("ku_dev_demo_step_failed: step=create DEV-ONLY synthetic "
                                 "Registry: readiness line missing from tool output.")
        print(f"  DEV-ONLY release {match.group(1)} activated (not operator trust).", flush=True)
    public_key = read_public_key(plan.public_key_file)
    if not plan.dev_registry:
        _run_step(
            runner, "verify active real Registry (read-only status)",
            _cargo_example("ku-core", "concept_registry_release", "status",
                           str(plan.registry_root), str(plan.public_key_file)),
            "Verify and deliberately activate the received release first; see 'Resolve a "
            "Registry startup failure' in docs/handoffs/2026-09-ku-obp-productization/outputs/"
            "KU_WEB_001_IMPLEMENTATION.md.")

    roles = ["sender"] + (["receiver"] if plan.with_receiver else [])
    for role in roles:
        (plan.demo_root / role).mkdir()
        _run_step(
            runner, f"create {role} Vault key + API token",
            [sys.executable, str(SECRETS_HELPER),
             "--data-dir", str(plan.demo_root / role / "dataset"),
             "--output-dir", str(plan.demo_root / role / "secrets")],
            "See 'Provision secrets for a new dataset' in KU_WEB_001_IMPLEMENTATION.md.")

    sample = plan.demo_root / "sample"
    sample.mkdir()
    (plan.demo_root / "custody").mkdir()
    text_file = sample / "water.txt"
    _write_new(text_file, SAMPLE_TEXT)
    request_file = sample / "manual-source-request.json"
    source_dir = plan.demo_root / "custody" / "water-source"
    _write_new(request_file, json.dumps({
        "operator": SAMPLE_OPERATOR,
        "text_file": text_file.as_posix(),
        "output_dir": source_dir.as_posix(),
        # The synthetic sample sentence ships with the repository; the person
        # running this command is the local operator recording this consent.
        "consent_local_private": True,
    }, indent=2) + "\n")
    _run_step(
        runner, "provision private sample source (ku_manual_source)",
        _cargo_example("onebrain-api", "ku_manual_source", str(request_file)),
        "See 'Provision a developer-owned manual source' in KU_WEB_001_IMPLEMENTATION.md.")
    sources = [{"label": "Dev demo sample: water",
                "canonical_file": (source_dir / "source.canonical").as_posix()}]

    sender = host_config(plan, public_key, "sender", sources=sources)
    _write_new(plan.demo_root / "host.json", json.dumps(sender, indent=2) + "\n")
    plan.hosts.append("host.json")
    if plan.with_receiver:
        # D-057: the receiver admits the same synthetic sample as its own private
        # source so `/ku` can start a new private draft from a received Public KU.
        # Admission saves or shares nothing.
        receiver_sources = [{"label": "Dev demo sample: water (receiver)",
                             "canonical_file": sources[0]["canonical_file"]}]
        receiver = host_config(plan, public_key, "receiver", sources=receiver_sources)
        _write_new(plan.demo_root / "receiver.json", json.dumps(receiver, indent=2) + "\n")
        plan.hosts.append("receiver.json")


def ps_quote(arg: str) -> str:
    """Quote one argument for a copy-pasteable PowerShell command line."""
    if arg and re.fullmatch(r"[A-Za-z0-9_./:=+@,-]+", arg):
        return arg
    return "'" + arg.replace("'", "''") + "'"


def default_posix() -> bool:
    """Printed commands target POSIX shells everywhere except native Windows."""
    return os.name != "nt"


def sh_quote(arg: str, posix: bool | None = None) -> str:
    """Quote for the shell the next steps are printed for (POSIX sh or PowerShell)."""
    if posix is None:
        posix = default_posix()
    return shlex.quote(arg) if posix else ps_quote(arg)


def python_command(posix: bool | None = None) -> str:
    """``python3`` on POSIX (fresh Ubuntu/Debian/macOS ship no ``python``), else ``python``."""
    if posix is None:
        posix = default_posix()
    return "python3" if posix else "python"


def cli_host_command(config: dict, posix: bool | None = None) -> str:
    """``onebrain start --api`` from the same host.json fields (``onebrain-cli``).

    Mirrors ``check_ku_public_share_demo.build_cli_host_args``: the token stays in
    ``api_token_file`` and the Vault key in ``vault_key_file`` (paths only).
    """
    args = ["start", "--data-dir", config["data_dir"], "--api",
            "--api-port", str(config["port"]), "--api-token-file", config["api_token_file"],
            "--concept-registry-release-root", config["registry_root"],
            "--concept-registry-release-public-key", config["registry_public_key"],
            "--ku-vault-key-file", config["vault_key_file"]]
    if config.get("network"):
        args += ["--ku-network-bind", config["network"]["bind_addr"]]
    for source in config.get("sources", []):
        args += ["--ku-source", f"{source['label']}={source['canonical_file']}"]
    if config.get("web_dir"):
        args += ["--web-dir", config["web_dir"]]
    features = " --features vnext-network-runtime" if config.get("network") else ""
    return (f"cargo run --locked --manifest-path src/Cargo.toml -p onebrain-cli{features} -- "
            + " ".join(sh_quote(arg, posix) for arg in args))


def print_next_steps(plan: Plan, posix: bool | None = None) -> None:
    if posix is None:
        posix = default_posix()
    root = plan.demo_root
    features = " --features vnext-network-runtime" if plan.with_receiver else ""

    def q(path: Path) -> str:
        return sh_quote(path.as_posix(), posix)

    print("\nku_dev_demo_bootstrap_ready: wrote " + ", ".join(plan.hosts)
          + f" under {root.as_posix()}")
    if plan.dev_registry:
        print("  Registry: DEV-ONLY synthetic (self-issued signer discarded; not operator trust; "
              "CCIDs incompatible with real releases; saved KUs are demo data).")
        print("  Concepts: water, liquid, solid, gas, ice, sun, earth, cat, tree "
              "(+ Vietnamese labels).")
    shell = "POSIX shell" if posix else "PowerShell"
    print(f"\nNext steps ({shell}, from the repository root):")
    step = 1
    if not (WEB_DIST / "index.html").is_file():
        build = ("(cd src/onebrain-web && npm ci && npm run build)" if posix else
                 "Push-Location src/onebrain-web; npm ci; npm run build; Pop-Location")
        print(f"  {step}. Build the Web once:  {build}")
        step += 1
    host_step = step
    print(f"  {step}. Start the host:  cargo run --locked --manifest-path src/Cargo.toml "
          f"-p onebrain-api{features} --example ku_local_web -- {q(root / 'host.json')}")
    step += 1
    print(f"  {step}. Open http://127.0.0.1:{plan.port}/ku and sign in with the token from "
          f"{(root / 'sender' / 'secrets' / 'api-token.txt').as_posix()} "
          "(read it locally; it is never printed).")
    step += 1
    print(f"  {step}. In /ku pick the source 'Dev demo sample: water', resolve 'water', enter "
          "the text 'Water is a liquid.', inspect the preview, then click "
          "'Save exact preview privately'. Nothing is saved or shared automatically.")
    step += 1
    sender = json.loads((root / "host.json").read_text(encoding="utf-8"))
    print(f"  {step}. Optional CLI host instead of step {host_step} (same data, port and /ku; "
          "the host runs until you stop it):  " + cli_host_command(sender, posix))
    if plan.with_receiver:
        step += 1
        then = " && " if posix else "; "
        check = f"{python_command(posix)} scripts/base/check_ku_public_share_demo.py"
        configs = (f"--sender-config {q(root / 'host.json')} "
                   f"--receiver-config {q(root / 'receiver.json')}")
        print(f"  {step}. Two-node share check (stop any host from steps {host_step}/{step - 1} "
              "first; the check starts both hosts itself). --seed-sample-ku explicitly saves the "
              "sample KU through the /ku button API only if nothing is saved yet:")
        print("     Web hosts:  cargo build --locked --manifest-path src/Cargo.toml -p onebrain-api "
              f"--features vnext-network-runtime --example ku_local_web{then}"
              f"{check} {configs} --receiver-kind web --skip-cli-client --seed-sample-ku")
        print("     CLI hosts + onebrain ku get/export:  cargo build --locked --manifest-path "
              f"src/Cargo.toml -p onebrain-cli --features vnext-network-runtime{then}"
              f"{check} {configs} "
              "--sender-kind cli --receiver-kind cli --require-cli-client --cli-share "
              "--seed-sample-ku")
    print("  Keep the same demo root, Vault keys and data_dir on restart. Delete the whole demo "
          "root to discard the demo; it is unrelated to any real dataset.")


def main(argv: Sequence[str] | None = None, runner: Runner = default_runner) -> int:
    parser = argparse.ArgumentParser(
        description="First-run bootstrap for the local /ku concept demo: prerequisite check, "
        "Registry (DEV-ONLY synthetic opt-in or operator-supplied), secrets, sample source and "
        "host config. Never saves/shares a KU or prints secrets.")
    parser.add_argument("--check-only", action="store_true",
                        help="only check prerequisites; write nothing")
    parser.add_argument("--demo-root", type=Path,
                        help="NEW directory outside the repository for all demo files")
    parser.add_argument("--dev-demo-registry", action="store_true",
                        help="explicitly create a DEV-ONLY synthetic signed Registry (D-056)")
    parser.add_argument("--registry-root", type=Path,
                        help="existing verified+activated real Registry root")
    parser.add_argument("--registry-public-key-file", type=Path,
                        help="independently trusted Registry public-key file (64 hex)")
    parser.add_argument("--port", type=int, default=4280, help="sender /ku port (default 4280)")
    parser.add_argument("--with-receiver", action="store_true",
                        help="also write receiver.json + opt-in loopback network blocks for the "
                        "D-045 share check")
    parser.add_argument("--receiver-port", type=int, help="receiver port (default --port + 1)")
    args = parser.parse_args(argv)

    print("Prerequisites:", flush=True)
    if not print_prerequisites(check_prerequisites(runner)):
        print("ku_dev_demo_prerequisites_missing: install the required tools above and retry.",
              file=sys.stderr)
        return 1
    if args.check_only:
        print("ku_dev_demo_prerequisites_ok: nothing was written.")
        return 0
    plan: Plan | None = None
    try:
        plan = build_plan(args)
        try:
            bootstrap(plan, runner)
        except OSError:
            raise BootstrapError("ku_dev_demo_write_failed: could not create a demo file or "
                                 "directory; check local permissions and free space.") from None
    except BootstrapError as error:
        print(error, file=sys.stderr)
        if plan is not None and plan.demo_root.exists():
            print("Partial demo root retained for inspection; retry with a NEW --demo-root.",
                  file=sys.stderr)
        return 1
    print_next_steps(plan)
    return 0


if __name__ == "__main__":
    sys.exit(main())
