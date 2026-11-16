from __future__ import annotations

import copy
import json
import unittest
from unittest import mock

from scripts.ci import validate_vnext_dr_m5
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
                validate_vnext_dr_m5_chaos_fuzz(frozen_profile())

    def test_every_dr_m5_chaos_test_commands_command_must_run_in_its_step(self) -> None:
        commands = validate_vnext_dr_m5.DR_M5_CHAOS_TEST_COMMANDS
        self.assertEqual(len(commands), 3)
        for command in commands:
            with self.subTest(command=command.pattern):
                self.assert_workflow_mutation_rejected_arch13(
                    lambda text, command=command: command.sub("", text)
                )

    def test_dr_m5_fuzz_corpus_smoke_command_must_run_in_its_step(self) -> None:
        command = validate_vnext_dr_m5.DR_M5_FUZZ_CORPUS_SMOKE_COMMAND
        self.assert_workflow_mutation_rejected_arch13(
            lambda text: command.sub("", text)
        )

    def test_arch13_step_0_cannot_disappear(self) -> None:
        self.assert_workflow_mutation_rejected_arch13(
            lambda text: text.replace('- name: M5.4 real-QUIC chaos and adversarial flood', '- name: renamed step')
        )

    def test_arch13_step_1_cannot_disappear(self) -> None:
        self.assert_workflow_mutation_rejected_arch13(
            lambda text: text.replace('- name: M5.4 deterministic parser corpus smoke', '- name: renamed step')
        )

    def test_arch13_commands_match_the_rust_tests(self) -> None:
        expected = {'src/onebrain-node/src/vnext_chaos_profile_tests.rs': ['fn m5_04_chaos_profile_', 'fn m5_04_fuzz_profile_targets_run_their_pr_corpus'], 'src/ku-net/src/vnext_chaos.rs': ['fn long_delivery_traces_converge_to_one_oracle_under_fair_redelivery', 'async fn real_quic_drop_duplicate_delay_reorder_disconnect_and_reunion_converge', 'async fn floods_and_slowloris_remain_bounded_without_state_amplification'], 'src/onebrain-node/src/vnext_fuzz_targets.rs': ['fn every_target_rejects_or_accepts_bounded_adversarial_bytes_without_panicking']}
        for relative, names in expected.items():
            source = (validate_vnext_dr_m5.ROOT / relative).read_text(encoding="utf-8")
            for name in names:
                with self.subTest(path=relative, name=name):
                    self.assertIn(name, source)

if __name__ == "__main__":
    unittest.main()
