from __future__ import annotations

import copy
import json
import unittest

from scripts.ci.validate_vnext_contracts import (
    ContractError,
    VNEXT_DESKTOP_WEB_UX_PROFILE,
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


if __name__ == "__main__":
    unittest.main()
