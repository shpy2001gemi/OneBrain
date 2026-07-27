from __future__ import annotations

import copy
import json
import unittest

from scripts.ci.validate_vnext_contracts import (
    ContractError,
    DR_M5_MIXED_ROLLBACK_PROFILE,
    validate_vnext_dr_m5_mixed_rollback,
)


def frozen_profile() -> dict[str, object]:
    return json.loads(DR_M5_MIXED_ROLLBACK_PROFILE.read_text(encoding="utf-8"))


class VNextDrM5MixedRollbackTests(unittest.TestCase):
    def test_frozen_profile_is_accepted(self) -> None:
        self.assertEqual(
            validate_vnext_dr_m5_mixed_rollback(frozen_profile()),
            (4, 3, 5, 7),
        )

    def test_transports_must_be_real_and_simultaneous(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["transports"]["simultaneous"] = False
        with self.assertRaisesRegex(ContractError, "transport"):
            validate_vnext_dr_m5_mixed_rollback(profile)

    def test_n_minus_one_frame_prefix_is_frozen(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["legacy_n_minus_one"]["corpus"][0]["framed_hex"] = "00000000"
        with self.assertRaisesRegex(ContractError, "frame"):
            validate_vnext_dr_m5_mixed_rollback(profile)

    def test_legacy_fixture_cannot_gain_vnext_authority(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["legacy_n_minus_one"]["vnext_authority"] = True
        with self.assertRaisesRegex(ContractError, "N-1"):
            validate_vnext_dr_m5_mixed_rollback(profile)

    def test_stale_config_cannot_reenable(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["runtime_generation_fence"]["startup_config_may_reenable"] = True
        with self.assertRaisesRegex(ContractError, "generation"):
            validate_vnext_dr_m5_mixed_rollback(profile)

    def test_session_must_recheck_generation_per_record(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["runtime_generation_fence"]["session_rule"] = "session-only"
        with self.assertRaisesRegex(ContractError, "generation"):
            validate_vnext_dr_m5_mixed_rollback(profile)

    def test_rollback_cannot_drop_quarantine(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["rollback"]["preserves"].remove("quarantine")
        with self.assertRaisesRegex(ContractError, "preservation"):
            validate_vnext_dr_m5_mixed_rollback(profile)

    def test_rollback_cannot_change_wallet(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["rollback"]["changes_wallet_state"] = True
        with self.assertRaisesRegex(ContractError, "preservation"):
            validate_vnext_dr_m5_mixed_rollback(profile)

    def test_process_kill_phase_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["process_kill"]["phases"].pop()
        with self.assertRaisesRegex(ContractError, "process-kill"):
            validate_vnext_dr_m5_mixed_rollback(profile)

    def test_explicit_reenable_cannot_be_removed(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["runtime_generation_fence"]["explicit_reenable_required"] = False
        with self.assertRaisesRegex(ContractError, "generation"):
            validate_vnext_dr_m5_mixed_rollback(profile)

    def test_exit_oracle_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["exit_oracles"].remove("legacy_and_vnext_real_transports_coexist")
        with self.assertRaisesRegex(ContractError, "exit oracle"):
            validate_vnext_dr_m5_mixed_rollback(profile)


if __name__ == "__main__":
    unittest.main()
