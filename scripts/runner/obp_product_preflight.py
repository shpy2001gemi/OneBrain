"""OBP-QA-001 local regression collection; never a qualification controller.

Only fixed repository tests run. No host provisioning, remote control, model
inference, live configuration, or P5 production entrypoint is exposed.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parents[2]
FORMAT = "onebrain/obp-product-preflight/1"
CARGO = ["cargo", "test", "--offline", "--locked", "--manifest-path", "src/Cargo.toml"]
SUITES = {
    "node": CARGO + ["-p", "onebrain-node", "--features", "vnext-outbound-first", "--lib", "-q"],
    "routes": CARGO + ["-p", "onebrain-node", "--features", "vnext-outbound-first", "--test", "vnext_node_runtime", "--test", "vnext_reachability_manager", "--test", "vnext_outbound_first_runtime", "--test", "vnext_relay_matrix_live", "--test", "vnext_outbound_first_matrix", "-q"],
    "relay": CARGO + ["-p", "onebrain-relay", "-q"],
    "core": CARGO + ["-p", "ku-net", "--features", "outbound-first", "--lib", "-q"],
    "api": CARGO + ["-p", "onebrain-api", "--features", "vnext-outbound-first", "--lib", "--test", "base_contract", "-q"],
    "api-off": CARGO + ["-p", "onebrain-api", "--no-default-features", "--lib", "-q"],
    "cli": CARGO + ["-p", "onebrain-cli", "--features", "vnext-outbound-first", "-q"],
    "desktop": CARGO + ["-p", "onebrain-desktop", "--features", "vnext-outbound-first", "--test", "lifecycle", "-q"],
    "desktop-off": CARGO + ["-p", "onebrain-desktop", "--no-default-features", "--test", "lifecycle", "-q"],
    "web": ["npm", "run", "test:ku", "--prefix", "src/onebrain-web"],
    "receipts": ["npm", "run", "test:vnext", "--prefix", "src/onebrain-web"],
    "contracts": ["python", "-m", "unittest", "scripts.ci.test_obp_desktop_packaging", "scripts.ci.test_validate_obp_local_api", "scripts.ci.test_validate_obp_product_contract", "scripts.ci.test_validate_vnext_cli_profile", "scripts.ci.test_validate_vnext_desktop_web_ux_profile", "scripts.ci.test_validate_vnext_outbound_reachability", "scripts.ci.test_validate_vnext_p5_multi_host", "scripts.runner.test_onebrain_p5_multi_host_v2", "scripts.runner.test_onebrain_p5_multi_host", "scripts.release.test_validate_evidence_carry_forward", "scripts.runner.test_obp_product_preflight"],
    "vnext": ["python", "scripts/ci/validate_vnext_contracts.py"],
}
CLAIMS = {name: False for name in (
    "product_acceptance_qualified", "multi_host_qualified", "base_gate_v1_qualified",
    "windows_qualified", "macos_qualified", "mobile_qualified", "browser_qualified",
)}


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def verify(report: dict, directory: Path) -> None:
    """Check local collection integrity, never derive production qualification."""
    if report.get("format") != FORMAT or report.get("scope") != "single-machine-local-preflight":
        raise ValueError("wrong preflight format/scope")
    claims = report.get("claims", {})
    if set(claims) != set(CLAIMS) or any(value is not False for value in claims.values()):
        raise ValueError("local evidence cannot qualify any product/platform")
    rows = report.get("suites", [])
    if [row["id"] for row in rows] != list(SUITES):
        raise ValueError("missing, duplicate or reordered suite")
    for row in rows:
        name = row["id"]
        if row.get("argv") != SUITES[name] or row.get("log") != name + ".log":
            raise ValueError("changed command or log path")
        if digest(directory / row["log"]) != row.get("sha256"):
            raise ValueError("log digest mismatch")
    passed = all(type(row.get("exit_code")) is int and row["exit_code"] == 0 for row in rows)
    if report.get("preflight_passed") is not passed:
        raise ValueError("incorrect local pass derivation")


def collect(directory: Path) -> int:
    directory.mkdir(parents=True, exist_ok=False)
    report = {
        "format": FORMAT, "scope": "single-machine-local-preflight",
        "started_utc": datetime.now(timezone.utc).isoformat(),
        "base_commit": git("rev-parse", "HEAD"), "base_tree": git("rev-parse", "HEAD^{tree}"),
        "working_tree_status": git("status", "--porcelain=v1"),
        "collector_sha256": digest(Path(__file__)),
        "claims": CLAIMS.copy(), "suites": [], "preflight_passed": False,
    }
    env = os.environ.copy()
    env["CARGO_NET_OFFLINE"] = "true"
    env["NO_COLOR"] = "1"
    # Local tests use explicit temporary fixtures; inherited host credentials
    # and opt-in model/probe settings must not enter this collection.
    for name in list(env):
        if name.startswith(("ONEBRAIN_", "OLLAMA_", "OBP_")):
            env.pop(name)
    for name, argv in SUITES.items():
        log = directory / (name + ".log")
        command = [sys.executable if argv[0] == "python" else (shutil.which(argv[0]) or argv[0]), *argv[1:]]
        print(f"START {name}", flush=True)
        with log.open("xb") as output:
            try:
                result = subprocess.run(command, cwd=ROOT, env=env, stdout=output,
                                        stderr=subprocess.STDOUT, timeout=1200, check=False)
                code = result.returncode
            except (OSError, subprocess.TimeoutExpired) as exc:
                output.write(("\nCOLLECTION FAILURE: " + type(exc).__name__ + "\n").encode())
                code = -1
        report["suites"].append({"id": name, "argv": argv, "exit_code": code,
                                  "log": log.name, "sha256": digest(log)})
        (directory / "partial.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(f"END {name}: exit={code}", flush=True)
    report["preflight_passed"] = all(row["exit_code"] == 0 for row in report["suites"])
    report["finished_utc"] = datetime.now(timezone.utc).isoformat()
    verify(report, directory)
    with (directory / "report.json").open("x", encoding="utf-8") as output:
        output.write(json.dumps(report, indent=2) + "\n")
    return 0 if report["preflight_passed"] else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument("--list", action="store_true")
    modes.add_argument("--run", type=Path, metavar="NEW_LOCAL_DIRECTORY")
    modes.add_argument("--verify", type=Path, metavar="REPORT_JSON")
    args = parser.parse_args()
    if args.list:
        print(json.dumps(SUITES, indent=2))
        return 0
    if args.verify:
        report = json.loads(args.verify.read_text(encoding="utf-8"))
        verify(report, args.verify.parent)
        print("Local collection integrity valid; qualification remains false.")
        return 0 if report["preflight_passed"] else 1
    return collect(args.run.resolve())


if __name__ == "__main__":
    raise SystemExit(main())
