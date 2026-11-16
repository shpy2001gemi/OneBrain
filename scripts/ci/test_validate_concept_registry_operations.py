from __future__ import annotations

import copy
import json
import unittest
from unittest import mock

from scripts.ci import validate_vnext_contracts
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
            (5, 5, 11, 1),
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
        profile["remaining_qualification_gates"].remove(
            "CONCEPT_REGISTRY_PRODUCTION_QUALIFICATION_PROFILE_V1"
        )
        with self.assertRaisesRegex(ContractError, "remaining qualification"):
            validate_concept_registry_operations(profile)

    def test_production_profile_cannot_claim_measured_completion(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["production_qualification"]["completion_claimed"] = True
        with self.assertRaisesRegex(ContractError, "production qualification"):
            validate_concept_registry_operations(profile)


    def assert_workflow_mutation_rejected_arch13(self, mutate) -> None:
        real_read = validate_vnext_contracts.read

        def read_mutated(candidate):
            text = real_read(candidate)
            if candidate == validate_vnext_contracts.VNEXT_FOUNDATION_WORKFLOW:
                mutated = mutate(text)
                self.assertNotEqual(mutated, text)
                return mutated
            return text

        with mock.patch.object(validate_vnext_contracts, "read", side_effect=read_mutated):
            with self.assertRaisesRegex(ContractError, 'CI acceptance gate'):
                validate_concept_registry_operations(frozen_profile())

    def test_every_concept_registry_fixture_commands_command_must_run_in_its_step(self) -> None:
        commands = validate_vnext_contracts.CONCEPT_REGISTRY_FIXTURE_COMMANDS
        self.assertEqual(len(commands), 5)
        for command in commands:
            with self.subTest(command=command.pattern):
                self.assert_workflow_mutation_rejected_arch13(
                    lambda text, command=command: command.sub("", text)
                )

    def test_every_concept_registry_release_commands_command_must_run_in_its_step(self) -> None:
        commands = validate_vnext_contracts.CONCEPT_REGISTRY_RELEASE_COMMANDS
        self.assertEqual(len(commands), 3)
        for command in commands:
            with self.subTest(command=command.pattern):
                self.assert_workflow_mutation_rejected_arch13(
                    lambda text, command=command: command.sub("", text)
                )

    def test_arch13_step_0_cannot_disappear(self) -> None:
        self.assert_workflow_mutation_rejected_arch13(
            lambda text: text.replace('- name: Build and verify a small Concept Registry fixture', '- name: renamed step')
        )

    def test_arch13_step_1_cannot_disappear(self) -> None:
        self.assert_workflow_mutation_rejected_arch13(
            lambda text: text.replace('- name: Concept Registry signed release and atomic activation', '- name: renamed step')
        )

    def test_arch13_commands_match_the_rust_tests(self) -> None:
        expected = {'src/ku-core/src/concept_registry_release.rs': ['fn concept_registry_release_profile_binds_artifacts_sources_and_distribution'], 'src/onebrain-node/src/concept_registry_runtime.rs': ['fn required_mode_loads_only_the_verified_active_release_without_cache_mutation', 'fn required_release_mode_never_falls_back_when_activation_is_missing']}
        for relative, names in expected.items():
            source = (validate_vnext_contracts.ROOT / relative).read_text(encoding="utf-8")
            for name in names:
                with self.subTest(path=relative, name=name):
                    self.assertIn(name, source)

if __name__ == "__main__":
    unittest.main()
