from __future__ import annotations

import json
import pathlib
import unittest

from scripts.ci.run_vnext_low_resource_matrix import (
    PINNED_UBUNTU_IMAGE,
    MatrixRunnerError,
    cargo_build_command,
    docker_matrix_command,
    select_linux_test_executable,
    validate_test_output,
)


class VNextLowResourceMatrixRunnerTests(unittest.TestCase):
    def test_cargo_builds_only_the_named_feature_enabled_test(self) -> None:
        self.assertEqual(
            cargo_build_command(pathlib.Path("src/Cargo.toml"), "matrix"),
            [
                "cargo", "test", "--locked", "--manifest-path", "src/Cargo.toml",
                "-p", "onebrain-node", "--features", "vnext-outbound-first",
                "--test", "matrix", "--no-run", "--message-format=json",
            ],
        )

    def test_cargo_json_selects_exactly_one_linux_test(self) -> None:
        rows = [
            {"reason": "compiler-artifact", "target": {"name": "matrix", "kind": ["test"]}, "executable": "/tmp/matrix-a"},
            {"reason": "build-finished", "success": True},
        ]
        selected = select_linux_test_executable(
            "\n".join(json.dumps(row) for row in rows), "matrix"
        )
        self.assertEqual(selected, pathlib.Path("/tmp/matrix-a"))

        duplicate = rows[:1] + [{**rows[0], "executable": "/tmp/matrix-b"}]
        with self.assertRaisesRegex(MatrixRunnerError, "exactly one"):
            select_linux_test_executable(
                "\n".join(json.dumps(row) for row in duplicate), "matrix"
            )

        windows = [{**rows[0], "executable": "C:\\tmp\\matrix.exe"}]
        with self.assertRaisesRegex(MatrixRunnerError, "Linux"):
            select_linux_test_executable(json.dumps(windows[0]), "matrix")

    def test_docker_argv_freezes_every_resource_and_mount_boundary(self) -> None:
        command = docker_matrix_command(pathlib.Path("/tmp/matrix"))
        joined = " ".join(command)
        self.assertEqual(command[:3], ["docker", "run", "--rm"])
        for literal in (
            "--platform linux/amd64", "--cpus 1", "--memory 536870912",
            "--memory-swap 536870912", "--pids-limit 256", "--network none",
            "--read-only", "--tmpfs /tmp:rw,nosuid,nodev,noexec,size=67108864",
            f"{PINNED_UBUNTU_IMAGE} sh -ceu",
            "memory.max", "memory.swap.max", "pids.max", "cpu.max",
            "exec /matrix --test-threads=1 --nocapture",
        ):
            self.assertIn(literal, joined)
        self.assertIn("dst=/matrix,readonly", joined)

    def test_skip_oom_and_profile_violation_never_pass(self) -> None:
        validate_test_output("test result: ok. 12 passed; 0 failed; 0 ignored", 0)
        for output, code, message in (
            ("test result: ok. 1 passed; 0 failed; 1 ignored", 0, "skip"),
            ("Killed", 137, "OOM"),
            ("PROFILE_BOUND_VIOLATION=queue-bytes", 0, "profile"),
        ):
            with self.subTest(message=message):
                with self.assertRaisesRegex(MatrixRunnerError, message):
                    validate_test_output(output, code)


if __name__ == "__main__":
    unittest.main()
