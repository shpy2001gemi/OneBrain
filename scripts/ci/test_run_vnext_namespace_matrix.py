from __future__ import annotations

import pathlib
import unittest

from scripts.ci.run_vnext_namespace_matrix import (
    EXPECTED_SECCOMP_SHA256,
    NAMESPACE_CASES,
    NamespaceMatrixError,
    docker_namespace_command,
    render_namespace_script,
    verify_seccomp_profile,
)


class VNextNamespaceMatrixRunnerTests(unittest.TestCase):
    def test_seccomp_profile_is_content_addressed(self) -> None:
        profile = pathlib.Path(__file__).with_name("vnext_namespace_seccomp.json")
        self.assertEqual(verify_seccomp_profile(profile), EXPECTED_SECCOMP_SHA256)
        with self.assertRaisesRegex(NamespaceMatrixError, "digest"):
            verify_seccomp_profile(profile, expected="00" * 32)

    def test_docker_argv_is_networkless_minimal_and_platform_pinned(self) -> None:
        command = docker_namespace_command(
            pathlib.Path("/tmp/seccomp.json"), pathlib.Path("/tmp/matrix")
        )
        joined = " ".join(command)
        for literal in (
            "--interactive", "--platform linux/amd64", "--network none", "--cap-drop ALL",
            "--cap-add NET_ADMIN", "--cap-add SYS_ADMIN",
            "--security-opt seccomp=",
            "--tmpfs /run/netns:rw,nosuid,nodev,noexec,mode=0755",
        ):
            self.assertIn(literal, joined)
        self.assertNotIn("--privileged", command)

    def test_fixture_script_covers_every_frozen_nat_class_and_cleanup(self) -> None:
        script = render_namespace_script("obp12deadbeef")
        for case in NAMESPACE_CASES:
            self.assertIn(case, script)
        self.assertIn("ip netns add", script)
        self.assertIn("nft list ruleset", script)
        self.assertIn("trap cleanup EXIT", script)
        self.assertIn("NAMESPACE_MATRIX_GREEN", script)

    def test_prefix_must_be_safe_and_unique(self) -> None:
        with self.assertRaisesRegex(NamespaceMatrixError, "prefix"):
            render_namespace_script("../unsafe")


if __name__ == "__main__":
    unittest.main()
