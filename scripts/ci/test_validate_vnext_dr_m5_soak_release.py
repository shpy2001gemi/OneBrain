from __future__ import annotations

import copy
import json
import unittest
from unittest import mock

from scripts.ci import validate_vnext_dr_m5
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


    def assert_workflow_mutation_rejected_arch13(self, mutate) -> None:
        real_read = validate_vnext_dr_m5.read

        def read_mutated(candidate):
            text = real_read(candidate)
            if candidate == validate_vnext_dr_m5.VNEXT_FOUNDATION_WORKFLOW:
                mutated = mutate(text)
                self.assertNotEqual(mutated, text)
                return mutated
            return text

        with mock.patch.object(validate_vnext_dr_m5, "read", side_effect=read_mutated):
            with self.assertRaisesRegex(ContractError, 'PR acceptance gate'):
                validate_vnext_dr_m5_soak_release(frozen_profile())

    def test_every_dr_m5_soak_test_commands_command_must_run_in_its_step(self) -> None:
        commands = validate_vnext_dr_m5.DR_M5_SOAK_TEST_COMMANDS
        self.assertEqual(len(commands), 3)
        for command in commands:
            with self.subTest(command=command.pattern):
                self.assert_workflow_mutation_rejected_arch13(
                    lambda text, command=command: command.sub("", text)
                )

    def test_arch13_step_0_cannot_disappear(self) -> None:
        self.assert_workflow_mutation_rejected_arch13(
            lambda text: text.replace('- name: M5.7 optimized real-QUIC soak and performance release smoke', '- name: renamed step')
        )

    def test_arch13_commands_match_the_rust_tests(self) -> None:
        expected = {'src/onebrain-node/src/vnext_soak_profile_tests.rs': ['fn m5_07_soak_profile_'], 'src/onebrain-node/src/vnext_soak_release.rs': ['async fn m5_07_release_smoke_uses_real_quic_fsync_and_all_fault_cycles', 'fn m5_07_percentiles_and_growth_budgets_fail_closed', 'fn m5_07_short_run_cannot_claim_nightly_or_pre_release_qualification', 'fn m5_07_macos_proc_metrics_are_available']}
        for relative, names in expected.items():
            source = (validate_vnext_dr_m5.ROOT / relative).read_text(encoding="utf-8")
            for name in names:
                with self.subTest(path=relative, name=name):
                    self.assertIn(name, source)

if __name__ == "__main__":
    unittest.main()
