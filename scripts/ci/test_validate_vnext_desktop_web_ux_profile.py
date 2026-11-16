from __future__ import annotations

import copy
import json
import unittest

from scripts.ci.validate_vnext_contracts import (
    ContractError,
    VNEXT_DESKTOP_WEB_UX_PROFILE,
    VNEXT_FOUNDATION_WORKFLOW,
    VNEXT_WEB_UX_RENDER_TEST,
    VNEXT_WEB_UX_STEP,
    VNEXT_WEB_UX_VITEST_CONFIG,
    require_vnext_web_ux_render_tests,
    validate_vnext_desktop_web_ux_profile,
)


def frozen_profile() -> dict[str, object]:
    return json.loads(VNEXT_DESKTOP_WEB_UX_PROFILE.read_text(encoding="utf-8"))


class VNextDesktopWebUxProfileTests(unittest.TestCase):
    def test_frozen_profile_is_accepted(self) -> None:
        self.assertEqual(validate_vnext_desktop_web_ux_profile(frozen_profile()), 2)

    def test_zero_result_cannot_claim_global_absence(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["discovery"]["zero_result_claims_global_absence"] = True
        with self.assertRaisesRegex(ContractError, "discovery"):
            validate_vnext_desktop_web_ux_profile(profile)

    def test_match_cannot_lose_quarantine(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["discovery"]["match_executable"] = True
        with self.assertRaisesRegex(ContractError, "discovery"):
            validate_vnext_desktop_web_ux_profile(profile)

    def test_public_use_cannot_skip_exact_confirmation(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["public_use"]["exact_typed_intent_required"] = False
        with self.assertRaisesRegex(ContractError, "Public Use"):
            validate_vnext_desktop_web_ux_profile(profile)

    def test_conflict_cannot_become_authorized(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["pomv"]["conflict_displays_authorized"] = True
        with self.assertRaisesRegex(ContractError, "PoMV"):
            validate_vnext_desktop_web_ux_profile(profile)

    def test_quit_cannot_bypass_shutdown(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["desktop"]["tray_quit_bypasses_shutdown"] = True
        with self.assertRaisesRegex(ContractError, "lifecycle"):
            validate_vnext_desktop_web_ux_profile(profile)


def web_ux_inputs() -> tuple[str, str, str]:
    return (
        VNEXT_FOUNDATION_WORKFLOW.read_text(encoding="utf-8"),
        VNEXT_WEB_UX_VITEST_CONFIG.read_text(encoding="utf-8"),
        VNEXT_WEB_UX_RENDER_TEST.read_text(encoding="utf-8"),
    )


class VNextWebUxRenderTestGateTests(unittest.TestCase):
    """ARCH-13 slice 15: Web labels are proved by vitest render tests in CI."""

    def test_current_workflow_config_and_tests_are_accepted(self) -> None:
        require_vnext_web_ux_render_tests(*web_ux_inputs())

    def test_step_must_run_vitest(self) -> None:
        workflow, config, tests = web_ux_inputs()
        step = workflow.index(f"- name: {VNEXT_WEB_UX_STEP}")
        end = workflow.index("- name:", step + 1)
        body = workflow[step:end]
        self.assertIn("npm run test:ku", body)
        mutated = workflow[:step] + body.replace("npm run test:ku", "") + workflow[end:]
        with self.assertRaisesRegex(ContractError, "test:ku"):
            require_vnext_web_ux_render_tests(mutated, config, tests)

    def test_vitest_elsewhere_does_not_count(self) -> None:
        workflow, config, tests = web_ux_inputs()
        step = workflow.index(f"- name: {VNEXT_WEB_UX_STEP}")
        end = workflow.index("- name:", step + 1)
        body = workflow[step:end].replace("npm run test:ku", "")
        mutated = (
            workflow[:step] + body + workflow[end:]
            + "\n      - name: Other\n        run: |\n          npm run test:ku\n"
        )
        with self.assertRaisesRegex(ContractError, "test:ku"):
            require_vnext_web_ux_render_tests(mutated, config, tests)

    def test_render_tests_must_be_included_by_vitest(self) -> None:
        workflow, config, tests = web_ux_inputs()
        mutated = config.replace('"tests/vnextUx.test.tsx"', "")
        with self.assertRaisesRegex(ContractError, "include"):
            require_vnext_web_ux_render_tests(workflow, mutated, tests)

    def test_render_tests_must_read_the_frozen_profile(self) -> None:
        workflow, config, tests = web_ux_inputs()
        mutated = tests.replace(VNEXT_DESKTOP_WEB_UX_PROFILE.name, "other.json")
        with self.assertRaisesRegex(ContractError, "frozen profile"):
            require_vnext_web_ux_render_tests(workflow, config, mutated)


if __name__ == "__main__":
    unittest.main()
