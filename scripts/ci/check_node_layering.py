#!/usr/bin/env python3
"""ARCH-09 layering check for the OneBrain Rust workspace (ratchet).

Uses `cargo tree` on the default (normal-edge) build to enforce:

1. The node runtime core (`onebrain-node`) stays desktop/HTTP-agnostic: it
   must not depend on surface crates (API, CLI, Desktop, Web host) or on
   HTTP-server / desktop-shell libraries.
2. Surface crates (`onebrain-api`, `onebrain-cli`) reach domain crates only
   through the node facade. Direct workspace edges that exist today are listed
   in KNOWN_BYPASSES and tolerated until ARCH-09 removes them; any *new*
   direct edge to a workspace crate fails. Removing a bypass also fails until
   the list is updated, so the ratchet only tightens.

Run from the repository root:  python scripts/ci/check_node_layering.py
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORKSPACE = ROOT / "src"

# Surface crates and the workspace crates they may depend on directly (the
# node facade plus the shared Base contract DTOs).
SURFACES = {
    "onebrain-api": {"onebrain-node", "onebrain-base-contract"},
    "onebrain-cli": {"onebrain-node", "onebrain-base-contract", "onebrain-api"},
}

# Existing direct bypasses of the node facade (review §2). Owned by ARCH-09;
# shrink this list as each bypass moves behind `onebrain-node`.
KNOWN_BYPASSES = {
    "onebrain-api": {"ku-core", "ku-encoder", "ku-kql"},
    "onebrain-cli": {"ku-core", "ku-kql"},
}

# The node core must not pull in surfaces or server/desktop shells. The
# `reqwest` HTTP client (with tower-http/hyper-util) still reaches the core
# through `ku-ai` (Ollama) and `ku-encoder`; moving that host adapter out of
# the core is a remaining ARCH-09 item, so it is not forbidden yet.
NODE = "onebrain-node"
NODE_FORBIDDEN = {
    "onebrain-api",
    "onebrain-cli",
    "onebrain-desktop",
    "onebrain-mobile-core",
    "axum",
    "tauri",
    "wry",
}

LINE = re.compile(r"^(\d+)([A-Za-z0-9_.\-]+) v")

# Simulation/benchmark/canary harness modules that must stay out of the
# default node core: each `pub mod` for them in lib.rs must be preceded by a
# `#[cfg(feature = "...")]` (or `#[cfg(test)]`) gate.
NODE_LIB = WORKSPACE / "onebrain-node" / "src" / "lib.rs"
HARNESS_MODULES = {
    "vnext_m5_benchmark",
    "vnext_m6_model",
    "vnext_scale_simulation",
    "vnext_crash_harness",
    "vnext_fuzz_targets",
    "vnext_soak_release",
    "vnext_canary_operations",
    "vnext_p5_operations",
}
MOD_LINE = re.compile(r"^\s*pub\s+mod\s+([A-Za-z0-9_]+)\s*;")
GATE_LINE = re.compile(r"^\s*#\[cfg\((feature\s*=|test\))")


def ungated_harness_modules(lib_text: str) -> list[str]:
    """Return harness modules declared in lib.rs without a cfg gate."""
    found: dict[str, bool] = {}
    previous = ""
    for line in lib_text.splitlines():
        match = MOD_LINE.match(line)
        if match and match.group(1) in HARNESS_MODULES:
            found[match.group(1)] = bool(GATE_LINE.match(previous))
        if line.strip():
            previous = line
    return sorted(name for name, gated in found.items() if not gated)


def cargo_tree(package: str, depth: int | None) -> list[tuple[int, str]]:
    """Return (depth, crate) rows for the default build's normal edges."""
    command = ["cargo", "tree", "-e", "normal", "--prefix", "depth", "-p", package]
    if depth is not None:
        command += ["--depth", str(depth)]
    output = subprocess.run(
        command, cwd=WORKSPACE, check=True, capture_output=True, text=True
    ).stdout
    rows = []
    for raw in output.splitlines():
        match = LINE.match(raw.strip())
        if match:
            rows.append((int(match.group(1)), match.group(2)))
    return rows


def workspace_crates() -> set[str]:
    output = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=WORKSPACE,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return {package["name"] for package in json.loads(output)["packages"]}


def evaluate(node_tree: set[str], direct: dict[str, set[str]]) -> list[str]:
    """Pure ratchet decision over the node closure and surface direct edges."""
    failures: list[str] = []
    leaked = sorted(node_tree & NODE_FORBIDDEN)
    if leaked:
        failures.append(f"{NODE} (default build) depends on surface/host crates: {leaked}")
    for surface, allowed in SURFACES.items():
        edges = direct.get(surface, set())
        bypasses = KNOWN_BYPASSES.get(surface, set())
        unexpected = sorted(edges - allowed - bypasses)
        if unexpected:
            failures.append(
                f"{surface} gained direct workspace dependencies outside the node facade: "
                f"{unexpected} (route them through {NODE})"
            )
        resolved = sorted(bypasses - edges)
        if resolved:
            failures.append(
                f"{surface} no longer depends on {resolved}; remove them from "
                "KNOWN_BYPASSES in scripts/ci/check_node_layering.py"
            )
    return failures


def check() -> list[str]:
    members = workspace_crates()
    if NODE not in members:
        return [f"{NODE} is not a workspace member"]
    node_tree = {crate for _, crate in cargo_tree(NODE, None)}
    direct = {
        surface: {
            crate
            for depth, crate in cargo_tree(surface, 1)
            if depth == 1 and crate in members
        }
        for surface in SURFACES
    }
    failures = evaluate(node_tree, direct)
    ungated = ungated_harness_modules(NODE_LIB.read_text(encoding="utf-8"))
    if ungated:
        failures.append(
            f"{NODE} lib.rs compiles harness modules in the default core: {ungated} "
            "(gate them behind a cfg(feature) such as vnext-sim-harness)"
        )
    return failures


def main() -> int:
    failures = check()
    if failures:
        for failure in failures:
            print(f"layering: FAIL {failure}", file=sys.stderr)
        return 1
    tolerated = sum(len(v) for v in KNOWN_BYPASSES.values())
    print(
        f"layering: PASS ({NODE} host-agnostic; surfaces limited to facade + "
        f"{tolerated} known ARCH-09 bypasses)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
