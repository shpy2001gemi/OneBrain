from __future__ import annotations

import copy
import json
import unittest

from scripts.ci.validate_vnext_contracts import (
    ContractError,
    DR_M5_CHAOS_FUZZ_PROFILE,
    validate_vnext_dr_m5_chaos_fuzz,
)


def frozen_profile() -> dict[str, object]:
    return json.loads(DR_M5_CHAOS_FUZZ_PROFILE.read_text(encoding="utf-8"))


class VNextDrM5ChaosFuzzTests(unittest.TestCase):
    def test_frozen_profile_is_accepted(self) -> None:
        self.assertEqual(
            validate_vnext_dr_m5_chaos_fuzz(frozen_profile()), (7, 5, 6, 18, 5)
        )

    def test_chaos_harness_cannot_become_default_enabled(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["feature"]["default_enabled"] = True
        with self.assertRaisesRegex(ContractError, "firewall"):
            validate_vnext_dr_m5_chaos_fuzz(profile)

    def test_real_quic_fault_family_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["real_quic"]["scenarios"].remove("partition_reunion")
        with self.assertRaisesRegex(ContractError, "real-QUIC"):
            validate_vnext_dr_m5_chaos_fuzz(profile)

    def test_flood_bound_cannot_expand(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["flood"]["context_limit"] = 9
        with self.assertRaisesRegex(ContractError, "flood"):
            validate_vnext_dr_m5_chaos_fuzz(profile)

    def test_trace_budget_cannot_shrink(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["property_trace"]["steps_per_seed"] = 4_095
        with self.assertRaisesRegex(ContractError, "long-trace"):
            validate_vnext_dr_m5_chaos_fuzz(profile)

    def test_trace_cannot_claim_network_completion(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["property_trace"]["claims_network_completion"] = True
        with self.assertRaisesRegex(ContractError, "long-trace"):
            validate_vnext_dr_m5_chaos_fuzz(profile)

    def test_frozen_oracle_root_cannot_drift(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["property_trace"]["expected_oracle_blake3"] = "0" * 64
        with self.assertRaisesRegex(ContractError, "long-trace"):
            validate_vnext_dr_m5_chaos_fuzz(profile)

    def test_fuzz_target_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["fuzz"]["targets"].remove("legacy_adapter")
        with self.assertRaisesRegex(ContractError, "fuzz target"):
            validate_vnext_dr_m5_chaos_fuzz(profile)

    def test_corpus_case_count_cannot_shrink(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["fuzz"]["required_pr_corpus_cases"] = 17
        with self.assertRaisesRegex(ContractError, "fuzz target"):
            validate_vnext_dr_m5_chaos_fuzz(profile)

    def test_corpus_digest_cannot_drift(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["fuzz"]["corpus_manifest_sha256"] = "0" * 64
        with self.assertRaisesRegex(ContractError, "fuzz target"):
            validate_vnext_dr_m5_chaos_fuzz(profile)

    def test_nightly_budget_cannot_drift(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["nightly"]["max_total_time_seconds_per_target"] = 59
        with self.assertRaisesRegex(ContractError, "nightly"):
            validate_vnext_dr_m5_chaos_fuzz(profile)

    def test_nightly_toolchain_cannot_drift(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["nightly"]["rust_toolchain"] = "nightly"
        with self.assertRaisesRegex(ContractError, "nightly"):
            validate_vnext_dr_m5_chaos_fuzz(profile)

    def test_exit_oracle_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["exit_oracles"].remove("bounded_state_under_flood")
        with self.assertRaisesRegex(ContractError, "exit oracle"):
            validate_vnext_dr_m5_chaos_fuzz(profile)


if __name__ == "__main__":
    unittest.main()
