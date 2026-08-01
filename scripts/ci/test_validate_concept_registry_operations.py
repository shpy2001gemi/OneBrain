from __future__ import annotations

import copy
import json
import unittest

from scripts.ci.validate_vnext_contracts import (
    CONCEPT_REGISTRY_OPERATIONS_PROFILE,
    ContractError,
    validate_concept_registry_operations,
)


def frozen_profile() -> dict[str, object]:
    return json.loads(CONCEPT_REGISTRY_OPERATIONS_PROFILE.read_text(encoding="utf-8"))


class ConceptRegistryOperationsContractTests(unittest.TestCase):
    def test_frozen_profile_is_accepted(self) -> None:
        self.assertEqual(
            validate_concept_registry_operations(frozen_profile()),
            (5, 5, 11, 7),
        )

    def test_artifact_cannot_be_removed(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["release_package"]["artifacts"].pop()
        with self.assertRaisesRegex(ContractError, "artifact set"):
            validate_concept_registry_operations(profile)

    def test_existing_release_cannot_be_overwritten(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["release_package"]["overwrite_existing_release"] = True
        with self.assertRaisesRegex(ContractError, "security"):
            validate_concept_registry_operations(profile)

    def test_source_download_hash_cannot_be_dropped(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["provenance"]["required_source_fields"].remove("download_blake3")
        with self.assertRaisesRegex(ContractError, "source provenance"):
            validate_concept_registry_operations(profile)

    def test_activation_cannot_become_mutable_pointer_swap(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["activation"]["publication"] = "overwrite-active-pointer"
        with self.assertRaisesRegex(ContractError, "activation/rollback"):
            validate_concept_registry_operations(profile)

    def test_required_mode_cannot_fallback(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["runtime"]["required_mode_fallback"] = True
        with self.assertRaisesRegex(ContractError, "fail-closed"):
            validate_concept_registry_operations(profile)

    def test_obp_artifact_gossip_cannot_be_enabled(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["distribution"]["obp_artifact_gossip"] = True
        with self.assertRaisesRegex(ContractError, "distribution"):
            validate_concept_registry_operations(profile)

    def test_ccid_diff_must_compare_actual_obr_ccids(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["ccid_stability_diff"]["compares_actual_obr_ccids"] = False
        with self.assertRaisesRegex(ContractError, "CCID stability"):
            validate_concept_registry_operations(profile)

    def test_resource_fixture_cannot_replace_full_registry_evidence(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["resource_qualification"]["full_registry_evidence_required"] = False
        with self.assertRaisesRegex(ContractError, "resource qualification"):
            validate_concept_registry_operations(profile)

    def test_capacity_check_cannot_move_after_staging(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["release_package"]["capacity_preflight"]["before_staging"] = False
        with self.assertRaisesRegex(ContractError, "capacity preflight"):
            validate_concept_registry_operations(profile)

    def test_failure_fixture_cannot_claim_production_qualification(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["failure_qualification"]["production_qualified_by_ci_fixture"] = True
        with self.assertRaisesRegex(ContractError, "failure qualification"):
            validate_concept_registry_operations(profile)

    def test_capacity_fault_injection_must_remain_feature_gated(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["failure_qualification"]["fault_injection_scope"] = "production-api"
        with self.assertRaisesRegex(ContractError, "failure qualification"):
            validate_concept_registry_operations(profile)

    def test_remaining_resource_gates_cannot_be_hidden(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["remaining_qualification_gates"].remove("low-ram-profile")
        with self.assertRaisesRegex(ContractError, "remaining qualification"):
            validate_concept_registry_operations(profile)


if __name__ == "__main__":
    unittest.main()
