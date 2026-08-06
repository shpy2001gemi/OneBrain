from __future__ import annotations

import unittest

from scripts.ci.validate_vnext_contracts import (
    ContractError,
    VNEXT_SOAK_RUNNER_GUIDE,
    VNEXT_SOAK_RUNNER_SCRIPT,
    VNEXT_SOAK_WORKFLOW,
    validate_vnext_soak_runner_kit,
)


def fixture(path) -> str:
    return path.read_text(encoding="utf-8")


class VNextSoakRunnerKitTests(unittest.TestCase):
    def setUp(self) -> None:
        self.script = fixture(VNEXT_SOAK_RUNNER_SCRIPT)
        self.guide = fixture(VNEXT_SOAK_RUNNER_GUIDE)
        self.workflow = fixture(VNEXT_SOAK_WORKFLOW)

    def test_portable_runner_kit_is_accepted(self) -> None:
        self.assertEqual(
            validate_vnext_soak_runner_kit(
                self.script, self.guide, self.workflow
            ),
            (23, 9, 4),
        )

    def test_ephemeral_default_cannot_disappear(self) -> None:
        mutated = self.script.replace(
            "setup_runner ephemeral", "setup_runner persistent"
        )
        with self.assertRaisesRegex(ContractError, "portable runner safety"):
            validate_vnext_soak_runner_kit(mutated, self.guide, self.workflow)

    def test_non_root_guard_cannot_disappear(self) -> None:
        mutated = self.script.replace("require_non_root", "allow_root")
        with self.assertRaisesRegex(ContractError, "portable runner safety"):
            validate_vnext_soak_runner_kit(mutated, self.guide, self.workflow)

    def test_dnf_dependency_support_cannot_disappear(self) -> None:
        mutated = self.script.replace("command_exists dnf", "command_exists rpm")
        with self.assertRaisesRegex(ContractError, "portable runner safety"):
            validate_vnext_soak_runner_kit(mutated, self.guide, self.workflow)

    def test_yum_dependency_support_cannot_disappear(self) -> None:
        mutated = self.script.replace("command_exists yum", "command_exists rpm")
        with self.assertRaisesRegex(ContractError, "portable runner safety"):
            validate_vnext_soak_runner_kit(mutated, self.guide, self.workflow)

    def test_eol_rhel_guard_cannot_disappear(self) -> None:
        mutated = self.script.replace(
            "require_supported_distribution", "allow_eol_distribution"
        )
        with self.assertRaisesRegex(ContractError, "portable runner safety"):
            validate_vnext_soak_runner_kit(mutated, self.guide, self.workflow)

    def test_runner_archive_checksum_cannot_disappear(self) -> None:
        mutated = self.script.replace(
            "sha256sum --check --status", "printf checksum-skipped"
        )
        with self.assertRaisesRegex(ContractError, "portable runner safety"):
            validate_vnext_soak_runner_kit(mutated, self.guide, self.workflow)

    def test_broad_home_delete_is_forbidden(self) -> None:
        mutated = self.script.replace(
            'rm -rf -- "$RUNNER_HOME"', 'rm -rf -- "$HOME"'
        )
        with self.assertRaisesRegex(ContractError, "portable runner"):
            validate_vnext_soak_runner_kit(mutated, self.guide, self.workflow)

    def test_long_soak_cannot_run_from_arbitrary_branch(self) -> None:
        mutated = self.workflow.replace(
            "github.ref == 'refs/heads/main' &&", "true &&"
        )
        with self.assertRaisesRegex(ContractError, "workflow safety"):
            validate_vnext_soak_runner_kit(self.script, self.guide, mutated)

    def test_pull_request_trigger_is_forbidden(self) -> None:
        mutated = self.workflow.replace(
            "  workflow_dispatch:", "  pull_request:\n  workflow_dispatch:"
        )
        with self.assertRaisesRegex(ContractError, "pull requests"):
            validate_vnext_soak_runner_kit(self.script, self.guide, mutated)

    def test_long_soak_cache_must_remain_restore_only(self) -> None:
        mutated = self.workflow.replace("save-if: false", "save-if: true")
        with self.assertRaisesRegex(ContractError, "workflow safety"):
            validate_vnext_soak_runner_kit(self.script, self.guide, mutated)

    def test_guide_cannot_request_an_inbound_port(self) -> None:
        mutated = self.guide.replace(
            "Không cần mở TCP/UDP inbound", "Mở TCP/UDP inbound"
        )
        with self.assertRaisesRegex(ContractError, "runner guide"):
            validate_vnext_soak_runner_kit(self.script, mutated, self.workflow)


if __name__ == "__main__":
    unittest.main()
