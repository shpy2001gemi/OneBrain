from __future__ import annotations

import unittest

from scripts.ci.validate_vnext_contracts import (
    ContractError,
    VNEXT_FOUNDATION_WORKFLOW,
    VNEXT_MACOS_SOAK_RUNNER_GUIDE,
    VNEXT_MACOS_SOAK_WORKFLOW,
    VNEXT_SOAK_RUNNER_SCRIPT,
    validate_vnext_macos_soak_runner_kit,
)


def fixture(path) -> str:
    return path.read_text(encoding="utf-8")


class VNextMacOsSoakRunnerKitTests(unittest.TestCase):
    def setUp(self) -> None:
        self.script = fixture(VNEXT_SOAK_RUNNER_SCRIPT)
        self.guide = fixture(VNEXT_MACOS_SOAK_RUNNER_GUIDE)
        self.workflow = fixture(VNEXT_MACOS_SOAK_WORKFLOW)
        self.foundation = fixture(VNEXT_FOUNDATION_WORKFLOW)

    def validate(
        self,
        script: str | None = None,
        guide: str | None = None,
        workflow: str | None = None,
        foundation: str | None = None,
    ) -> tuple[int, int, int, int]:
        return validate_vnext_macos_soak_runner_kit(
            script or self.script,
            guide or self.guide,
            workflow or self.workflow,
            foundation or self.foundation,
        )

    def test_macos_runner_kit_is_accepted(self) -> None:
        self.assertEqual(self.validate(), (7, 8, 7, 5))

    def test_native_runner_asset_cannot_change(self) -> None:
        mutated = self.script.replace(
            'RUNNER_ASSET_ID="osx-arm64"',
            'RUNNER_ASSET_ID="osx-x64"',
        )
        with self.assertRaisesRegex(ContractError, "macOS runner safety"):
            self.validate(script=mutated)

    def test_macos_label_cannot_disappear(self) -> None:
        mutated = self.script.replace(
            'DEFAULT_RUNNER_LABELS="onebrain-soak-macos-arm64"',
            'DEFAULT_RUNNER_LABELS="onebrain-soak"',
        )
        with self.assertRaisesRegex(ContractError, "macOS runner safety"):
            self.validate(script=mutated)

    def test_macos_checksum_cannot_disappear(self) -> None:
        mutated = self.script.replace(
            "shasum -a 256",
            "printf checksum-skipped",
        )
        with self.assertRaisesRegex(ContractError, "macOS runner safety"):
            self.validate(script=mutated)

    def test_caffeinate_cannot_disappear(self) -> None:
        mutated = self.script.replace(
            "caffeinate -dimsu",
            "printf sleep-is-allowed",
        )
        with self.assertRaisesRegex(ContractError, "macOS runner safety"):
            self.validate(script=mutated)

    def test_long_soak_cannot_run_from_arbitrary_branch(self) -> None:
        mutated = self.workflow.replace(
            "github.ref == 'refs/heads/main'",
            "true",
        )
        with self.assertRaisesRegex(ContractError, "workflow safety"):
            self.validate(workflow=mutated)

    def test_pull_request_trigger_is_forbidden(self) -> None:
        mutated = self.workflow.replace(
            "  workflow_dispatch:",
            "  pull_request:\n  workflow_dispatch:",
        )
        with self.assertRaisesRegex(ContractError, "manual-only"):
            self.validate(workflow=mutated)

    def test_schedule_trigger_is_forbidden(self) -> None:
        mutated = self.workflow.replace(
            "  workflow_dispatch:",
            '  schedule:\n    - cron: "41 1 * * *"\n  workflow_dispatch:',
        )
        with self.assertRaisesRegex(ContractError, "manual-only"):
            self.validate(workflow=mutated)

    def test_read_only_permission_cannot_disappear(self) -> None:
        mutated = self.workflow.replace(
            "permissions:\n  contents: read",
            "permissions:\n  contents: write",
        )
        with self.assertRaisesRegex(ContractError, "workflow safety"):
            self.validate(workflow=mutated)

    def test_guide_cannot_request_an_inbound_port(self) -> None:
        mutated = self.guide.replace(
            "Không cần mở TCP/UDP inbound",
            "Mở TCP/UDP inbound",
        )
        with self.assertRaisesRegex(ContractError, "runner guide"):
            self.validate(guide=mutated)

    def test_hosted_apple_silicon_lane_cannot_disappear(self) -> None:
        mutated = self.foundation.replace(
            "runs-on: macos-15",
            "runs-on: ubuntu-latest",
        )
        with self.assertRaisesRegex(ContractError, "acceptance lane"):
            self.validate(foundation=mutated)


if __name__ == "__main__":
    unittest.main()
