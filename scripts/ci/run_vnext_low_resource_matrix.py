#!/usr/bin/env python3
"""Build and run the outbound-first matrix under the frozen Linux budget."""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import subprocess
import sys
from typing import Sequence


PINNED_UBUNTU_IMAGE = (
    "ubuntu:24.04@sha256:561618e2c15bf2397621dd04f96926663a3b5616c189cf7e38db7e82f5c538ea"
)


class MatrixRunnerError(RuntimeError):
    pass


def cargo_build_command(manifest_path: pathlib.Path, test: str) -> list[str]:
    return [
        "cargo", "test", "--locked", "--manifest-path", manifest_path.as_posix(),
        "-p", "onebrain-node", "--features", "vnext-outbound-first",
        "--test", test, "--no-run", "--message-format=json",
    ]


def select_linux_test_executable(cargo_stdout: str, test: str) -> pathlib.Path:
    matches: list[str] = []
    for line in cargo_stdout.splitlines():
        try:
            row = json.loads(line)
        except json.JSONDecodeError:
            continue
        target = row.get("target", {})
        if (
            row.get("reason") == "compiler-artifact"
            and target.get("name") == test
            and "test" in target.get("kind", [])
            and isinstance(row.get("executable"), str)
        ):
            matches.append(row["executable"])
    if len(matches) != 1:
        raise MatrixRunnerError(
            f"Cargo must produce exactly one executable for test {test!r}; got {len(matches)}"
        )
    raw = matches[0]
    if raw.lower().endswith(".exe") or (len(raw) >= 2 and raw[1] == ":"):
        raise MatrixRunnerError("the constrained runner accepts only a Linux test executable")
    return pathlib.Path(raw)


def docker_matrix_command(executable: pathlib.Path) -> list[str]:
    source = executable.resolve().as_posix()
    cgroup_gate = (
        'test "$(cat /sys/fs/cgroup/memory.max)" = 536870912; '
        'test "$(cat /sys/fs/cgroup/memory.swap.max)" = 0; '
        'test "$(cat /sys/fs/cgroup/pids.max)" = 256; '
        'test "$(cat /sys/fs/cgroup/cpu.max)" = "100000 100000"; '
        'exec /matrix --test-threads=1 --nocapture'
    )
    return [
        "docker", "run", "--rm", "--platform", "linux/amd64",
        "--cpus", "1", "--memory", "536870912", "--memory-swap", "536870912",
        "--pids-limit", "256", "--network", "none", "--read-only",
        "--tmpfs", "/tmp:rw,nosuid,nodev,noexec,size=67108864",
        "--mount", f"type=bind,src={source},dst=/matrix,readonly",
        PINNED_UBUNTU_IMAGE, "sh", "-ceu", cgroup_gate,
    ]


def validate_test_output(output: str, returncode: int) -> None:
    lowered = output.lower()
    if returncode == 137 or "out of memory" in lowered or "oomkilled" in lowered:
        raise MatrixRunnerError("Docker OOM is a hard matrix failure")
    if "profile_bound_violation=" in lowered:
        raise MatrixRunnerError("profile-bound violation reported by matrix")
    if (
        re.search(r"[1-9][0-9]* ignored", lowered)
        or re.search(r"[1-9][0-9]* skipped", lowered)
        or "skip:" in lowered
    ):
        raise MatrixRunnerError("skip/ignore is forbidden in the constrained matrix")
    if returncode != 0:
        raise MatrixRunnerError(f"constrained matrix failed with exit code {returncode}")
    if "test result: ok." not in lowered:
        raise MatrixRunnerError("constrained matrix did not emit a successful Rust test summary")


def run(command: Sequence[str], *, cwd: pathlib.Path | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        list(command), cwd=cwd, text=True, encoding="utf-8", errors="replace",
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False,
    )


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest-path", type=pathlib.Path, required=True)
    parser.add_argument("--test", required=True)
    args = parser.parse_args(argv)

    if not sys.platform.startswith("linux"):
        raise MatrixRunnerError("the real constrained matrix must run on a Linux host")
    built = run(cargo_build_command(args.manifest_path, args.test))
    if built.returncode != 0:
        raise MatrixRunnerError(f"Cargo matrix build failed:\n{built.stdout}")
    executable = select_linux_test_executable(built.stdout, args.test)
    if not executable.is_file():
        raise MatrixRunnerError(f"Cargo executable is missing: {executable}")
    executed = run(docker_matrix_command(executable))
    print(executed.stdout, end="")
    validate_test_output(executed.stdout, executed.returncode)
    print("VNEXT_LOW_RESOURCE_MATRIX_GREEN")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except MatrixRunnerError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
