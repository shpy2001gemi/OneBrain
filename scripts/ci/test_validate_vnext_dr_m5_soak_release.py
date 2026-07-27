from __future__ import annotations

import copy
import json
import unittest

from scripts.ci.validate_vnext_contracts import (
    ContractError,
    DR_M5_SOAK_RELEASE_PROFILE,
    validate_vnext_dr_m5_soak_release,
)


def frozen_profile() -> dict[str, object]:
    return json.loads(DR_M5_SOAK_RELEASE_PROFILE.read_text(encoding="utf-8"))


class VNextDrM5SoakReleaseTests(unittest.TestCase):
    def test_frozen_profile_is_accepted(self) -> None:
        self.assertEqual(
            validate_vnext_dr_m5_soak_release(frozen_profile()),
            (3, 4, 3, 3, 7),
        )

    def test_release_build_cannot_be_downgraded(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["build"]["cargo_profile"] = "debug"
        with self.assertRaisesRegex(ContractError, "release build"):
            validate_vnext_dr_m5_soak_release(profile)

    def test_real_quic_cannot_be_replaced_by_model_only_transport(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["build"]["transport"] = "modeled"
        with self.assertRaisesRegex(ContractError, "release build"):
            validate_vnext_dr_m5_soak_release(profile)

    def test_nightly_cannot_be_shortened(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["run_profiles"]["nightly-24h"]["minimum_elapsed_seconds"] = 3600
        with self.assertRaisesRegex(ContractError, "duration"):
            validate_vnext_dr_m5_soak_release(profile)

    def test_pre_release_cannot_be_shortened(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["run_profiles"]["pre-release-72h"]["minimum_elapsed_seconds"] = 86400
        with self.assertRaisesRegex(ContractError, "duration"):
            validate_vnext_dr_m5_soak_release(profile)

    def test_smoke_cannot_claim_release_qualification(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["run_profiles"]["smoke"]["release_qualifying"] = True
        with self.assertRaisesRegex(ContractError, "duration"):
            validate_vnext_dr_m5_soak_release(profile)

    def test_percentile_budget_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        del profile["latency_budgets_micros"]["quic_authenticated_connect"]["p99"]
        with self.assertRaisesRegex(ContractError, "latency"):
            validate_vnext_dr_m5_soak_release(profile)

    def test_disk_slope_budget_cannot_expand(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["growth_budgets"]["disk_bytes"]["max_positive_slope_per_cycle"] *= 2
        with self.assertRaisesRegex(ContractError, "growth"):
            validate_vnext_dr_m5_soak_release(profile)

    def test_incremental_cursor_cannot_be_removed(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["incremental_scan"]["durable_selector_type_cursor"] = False
        with self.assertRaisesRegex(ContractError, "incremental"):
            validate_vnext_dr_m5_soak_release(profile)

    def test_fault_family_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["fault_cycle"].remove("partition-reunion")
        with self.assertRaisesRegex(ContractError, "fault"):
            validate_vnext_dr_m5_soak_release(profile)

    def test_operator_rollback_signal_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["operator_signals"].remove("rollback-reason-codes")
        with self.assertRaisesRegex(ContractError, "operator"):
            validate_vnext_dr_m5_soak_release(profile)

    def test_semantic_exit_oracle_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["exit_oracles"].remove(
            "m4-no-truth-benefit-wallet-obt-amplification"
        )
        with self.assertRaisesRegex(ContractError, "exit oracle"):
            validate_vnext_dr_m5_soak_release(profile)


if __name__ == "__main__":
    unittest.main()
