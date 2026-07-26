from __future__ import annotations

import copy
import json
import unittest

from scripts.ci.validate_vnext_contracts import (
    ContractError,
    VNEXT_CLI_PROFILE,
    validate_vnext_cli_profile,
)


def frozen_profile() -> dict[str, object]:
    return json.loads(VNEXT_CLI_PROFILE.read_text(encoding="utf-8"))


class VNextCliProfileTests(unittest.TestCase):
    def test_frozen_profile_is_accepted(self) -> None:
        self.assertEqual(validate_vnext_cli_profile(frozen_profile()), 11)

    def test_public_use_cannot_gain_yes_bypass(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["public_use"]["yes_bypass"] = True
        with self.assertRaisesRegex(ContractError, "Public Use"):
            validate_vnext_cli_profile(profile)

    def test_development_signer_cannot_lose_double_opt_in(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["feed_signer"]["development_file_requires_opt_in"] = False
        with self.assertRaisesRegex(ContractError, "Feed signer"):
            validate_vnext_cli_profile(profile)

    def test_zero_result_cannot_claim_global_absence(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["need"]["zero_result_claims_global_absence"] = True
        with self.assertRaisesRegex(ContractError, "Need firewall"):
            validate_vnext_cli_profile(profile)

    def test_quarantined_match_cannot_become_executable(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["need"]["match_executable"] = True
        with self.assertRaisesRegex(ContractError, "Need firewall"):
            validate_vnext_cli_profile(profile)

    def test_view_cannot_authorize_reward(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["view_firewalls"]["authorizes_reward"] = True
        with self.assertRaisesRegex(ContractError, "view firewall"):
            validate_vnext_cli_profile(profile)


if __name__ == "__main__":
    unittest.main()
