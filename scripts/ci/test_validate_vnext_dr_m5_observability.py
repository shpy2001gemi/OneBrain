from __future__ import annotations

import copy
import json
import unittest

from scripts.ci.validate_vnext_contracts import (
    ContractError,
    DR_M5_OBSERVABILITY_PROFILE,
    validate_vnext_dr_m5_observability,
)


def frozen_profile() -> dict[str, object]:
    return json.loads(DR_M5_OBSERVABILITY_PROFILE.read_text(encoding="utf-8"))


class VNextDrM5ObservabilityTests(unittest.TestCase):
    def test_frozen_profile_is_accepted(self) -> None:
        self.assertEqual(validate_vnext_dr_m5_observability(frozen_profile()), (22, 4, 4))

    def test_reason_code_inventory_cannot_shrink(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["reason_codes"].remove("REJECTED_PROTOCOL")
        with self.assertRaisesRegex(ContractError, "reason-code"):
            validate_vnext_dr_m5_observability(profile)

    def test_finite_histogram_buckets_cannot_drift(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["resource_metrics"]["record_bytes_inclusive_upper_bounds"][0] += 1
        with self.assertRaisesRegex(ContractError, "bucket"):
            validate_vnext_dr_m5_observability(profile)

    def test_private_need_labels_remain_explicitly_forbidden(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["privacy"]["forbidden_metric_labels"].remove("private_need")
        with self.assertRaisesRegex(ContractError, "privacy"):
            validate_vnext_dr_m5_observability(profile)

    def test_swallowed_adversarial_errors_remain_forbidden(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["structured_logging"]["swallowed_adversarial_errors"] = True
        with self.assertRaisesRegex(ContractError, "logging"):
            validate_vnext_dr_m5_observability(profile)

    def test_outbox_age_gauge_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["runtime_gauges"].remove("oldest_pending_outbox_age_seconds")
        with self.assertRaisesRegex(ContractError, "gauge"):
            validate_vnext_dr_m5_observability(profile)

    def test_registry_fallback_state_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["registry_states"].remove("FALLBACK_V1")
        with self.assertRaisesRegex(ContractError, "registry"):
            validate_vnext_dr_m5_observability(profile)

    def test_operator_snapshot_must_remain_local_authenticated(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["operator_snapshot"]["authenticated_local"] = False
        with self.assertRaisesRegex(ContractError, "operator snapshot"):
            validate_vnext_dr_m5_observability(profile)

    def test_profile_cannot_claim_network_completion(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["claims_network_completion"] = True
        with self.assertRaisesRegex(ContractError, "profile"):
            validate_vnext_dr_m5_observability(profile)

    def test_exact_transition_oracle_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["exit_oracles"].remove("exact_counter_transitions_are_reproducible")
        with self.assertRaisesRegex(ContractError, "oracle"):
            validate_vnext_dr_m5_observability(profile)


if __name__ == "__main__":
    unittest.main()
