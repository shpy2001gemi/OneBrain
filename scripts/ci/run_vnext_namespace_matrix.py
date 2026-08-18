#!/usr/bin/env python3
"""Run the privileged, network-none Linux namespace reachability matrix."""

from __future__ import annotations

import argparse
import hashlib
import os
import pathlib
import re
import secrets
import subprocess
import sys
import tempfile
from typing import Sequence

try:
    from scripts.ci.run_vnext_low_resource_matrix import (
        PINNED_UBUNTU_IMAGE,
        cargo_build_command,
        select_linux_test_executable,
    )
except ModuleNotFoundError:  # Direct `python scripts/ci/...` execution.
    from run_vnext_low_resource_matrix import (  # type: ignore[no-redef]
        PINNED_UBUNTU_IMAGE,
        cargo_build_command,
        select_linux_test_executable,
    )


EXPECTED_SECCOMP_SHA256 = "598205a08961b8c06f581908269cf58fd2c41ed157a34b0983432bdd25d7a4a3"
NAMESPACE_CASES = (
    "full-cone", "address-restricted", "port-restricted", "symmetric-nat",
    "two-level-cgnat", "upstream-udp-drop", "udp-total-block-tcp443-fallback",
    "address-migration",
)


class NamespaceMatrixError(RuntimeError):
    pass


def verify_seccomp_profile(
    path: pathlib.Path, *, expected: str = EXPECTED_SECCOMP_SHA256
) -> str:
    observed = hashlib.sha256(path.read_bytes()).hexdigest()
    if observed != expected:
        raise NamespaceMatrixError(
            f"seccomp profile digest mismatch: expected {expected}, observed {observed}"
        )
    return observed


def docker_namespace_command(
    seccomp_profile: pathlib.Path,
    executable: pathlib.Path,
    *,
    image: str = "onebrain-vnext-namespace-matrix:ubuntu24-amd64",
    apparmor_unconfined: bool = False,
) -> list[str]:
    command = [
        "docker", "run", "--rm", "--interactive", "--platform", "linux/amd64", "--network", "none",
        "--cap-drop", "ALL", "--cap-add", "NET_ADMIN", "--cap-add", "SYS_ADMIN",
        "--security-opt", f"seccomp={seccomp_profile.resolve().as_posix()}",
    ]
    if apparmor_unconfined:
        command += ["--security-opt", "apparmor=unconfined"]
    command += [
        "--tmpfs", "/run/netns:rw,nosuid,nodev,noexec,mode=0755",
        "--mount", f"type=bind,src={seccomp_profile.resolve().as_posix()},dst=/seccomp.json,readonly",
        "--mount", f"type=bind,src={executable.resolve().as_posix()},dst=/matrix,readonly",
        image, "bash", "-s", "--", "/matrix",
    ]
    return command


def render_namespace_script(prefix: str) -> str:
    if not re.fullmatch(r"obp12[0-9a-f]{8}", prefix):
        raise NamespaceMatrixError("namespace prefix must be obp12 plus eight lowercase hex digits")
    cases = " ".join(NAMESPACE_CASES)
    return f"""set -Eeuo pipefail
matrix_executable="$1"
prefix={prefix}
before_rules=$(nft list ruleset)
before_netns=$(find /run/netns -mindepth 1 -maxdepth 1 -printf '%f\\n' | sort)
cleanup() {{
  for ns in $(ip netns list | awk '{{print $1}}' | grep '^'"$prefix" || true); do ip netns del "$ns" || true; done
  for table in $(nft -n list tables | awk '$3 ~ /^'"$prefix"'/ {{print $2 " " $3}}'); do nft delete table $table || true; done
}}
trap cleanup EXIT
command -v ip >/dev/null
command -v nft >/dev/null

# Capability/mount preflight is mandatory and is completed before product tests.
probe="${{prefix}}p"
ip netns add "$probe"
ip netns exec "$probe" true
ip netns del "$probe"

i=0
for case_name in {cases}; do
  i=$((i+1))
  ns="${{prefix}}${{i}}"
  host_if="h${{prefix#obp12}}${{i}}"
  peer_if="p${{prefix#obp12}}${{i}}"
  ip netns add "$ns"
  ip link add "$host_if" type veth peer name "$peer_if"
  ip link set "$peer_if" netns "$ns"
  ip addr add "10.212.${{i}}.1/30" dev "$host_if"
  ip link set "$host_if" up
  ip netns exec "$ns" ip link set lo up
  ip netns exec "$ns" ip addr add "10.212.${{i}}.2/30" dev "$peer_if"
  ip netns exec "$ns" ip link set "$peer_if" up
  ip netns exec "$ns" ip -4 -o addr show dev "$peer_if" | grep -F "10.212.${{i}}.2/30" >/dev/null
  table="${{prefix}}${{i}}"
  nft add table ip "$table"
  nft "add chain ip $table observation {{ type filter hook input priority -190; policy accept; }}"
  nft add rule ip "$table" observation iifname "$host_if" counter comment "$case_name"
  nft -a list table ip "$table" | grep -F "$case_name" >/dev/null
  if [ "$case_name" = address-migration ]; then
    ip netns exec "$ns" ip addr replace "10.212.${{i}}.3/30" dev "$peer_if"
    ip netns exec "$ns" ip -4 -o addr show dev "$peer_if" | grep -F "10.212.${{i}}.3/30" >/dev/null
  fi
  nft delete table ip "$table"
  ip netns del "$ns"
done

"$matrix_executable" namespace_contract_covers_real_topology_names --exact --nocapture
cleanup
after_rules=$(nft list ruleset)
after_netns=$(find /run/netns -mindepth 1 -maxdepth 1 -printf '%f\\n' | sort)
test "$before_rules" = "$after_rules"
test "$before_netns" = "$after_netns"
trap - EXIT
printf 'NAMESPACE_MATRIX_GREEN\\n'
"""


def run(command: Sequence[str], *, input_text: str | None = None) -> subprocess.CompletedProcess[str]:
    if input_text is None:
        return subprocess.run(
            list(command), text=True, encoding="utf-8", errors="replace",
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False,
        )
    raw = subprocess.run(
        list(command), input=input_text.encode("utf-8"), stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, check=False,
    )
    return subprocess.CompletedProcess(
        raw.args, raw.returncode, raw.stdout.decode("utf-8", errors="replace"), None
    )


def build_fixture_image() -> str:
    tag = "onebrain-vnext-namespace-matrix:ubuntu24-amd64"
    dockerfile = f"""FROM {PINNED_UBUNTU_IMAGE}
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends iproute2 nftables ca-certificates && rm -rf /var/lib/apt/lists/*
"""
    with tempfile.TemporaryDirectory(prefix="onebrain-vnext-netns-") as directory:
        path = pathlib.Path(directory, "Dockerfile")
        path.write_text(dockerfile, encoding="utf-8", newline="\n")
        result = run(["docker", "build", "--platform", "linux/amd64", "-t", tag, directory])
    if result.returncode != 0:
        raise NamespaceMatrixError(f"namespace fixture image build failed:\n{result.stdout}")
    return tag


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest-path", type=pathlib.Path, required=True)
    parser.add_argument("--test", required=True)
    parser.add_argument("--apparmor-unconfined", action="store_true")
    args = parser.parse_args(argv)
    if not sys.platform.startswith("linux"):
        raise NamespaceMatrixError("the real namespace matrix must run on a Linux host")

    profile = pathlib.Path(__file__).with_name("vnext_namespace_seccomp.json")
    verify_seccomp_profile(profile)
    built = run(cargo_build_command(args.manifest_path, args.test))
    if built.returncode != 0:
        raise NamespaceMatrixError(f"Cargo matrix build failed:\n{built.stdout}")
    executable = select_linux_test_executable(built.stdout, args.test)
    image = build_fixture_image()
    prefix = "obp12" + secrets.token_hex(4)
    command = docker_namespace_command(
        profile, executable, image=image, apparmor_unconfined=args.apparmor_unconfined
    )
    result = run(command, input_text=render_namespace_script(prefix))
    print(result.stdout, end="")
    if result.returncode != 0 or "NAMESPACE_MATRIX_GREEN" not in result.stdout:
        raise NamespaceMatrixError(f"namespace matrix failed with exit code {result.returncode}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except NamespaceMatrixError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
