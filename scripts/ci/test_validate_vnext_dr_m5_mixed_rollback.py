from __future__ import annotations

import copy
import json
import unittest
from unittest import mock

from scripts.ci import validate_vnext_dr_m5
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
                validate_vnext_dr_m5_mixed_rollback(frozen_profile())

    def test_every_dr_m5_rollback_test_commands_command_must_run_in_its_step(self) -> None:
        commands = validate_vnext_dr_m5.DR_M5_ROLLBACK_TEST_COMMANDS
        self.assertEqual(len(commands), 5)
        for command in commands:
            with self.subTest(command=command.pattern):
                self.assert_workflow_mutation_rejected_arch13(
                    lambda text, command=command: command.sub("", text)
                )

    def test_arch13_step_0_cannot_disappear(self) -> None:
        self.assert_workflow_mutation_rejected_arch13(
            lambda text: text.replace('- name: M5.6 mixed-version runtime rollback and process-kill recovery', '- name: renamed step')
        )

    def test_arch13_commands_match_the_rust_tests(self) -> None:
        expected = {'src/onebrain-node/src/vnext_rollback_profile_tests.rs': ['fn m5_06_rollback_profile_'], 'src/onebrain-node/src/vnext_runtime_rollout.rs': ['fn m5_06_runtime_rollback_process_kill_matrix_recovers_exact_generation'], 'src/onebrain-node/src/vnext_product_runtime.rs': ['async fn runtime_kill_rollback_restart_and_explicit_reenable_use_real_quic'], 'src/onebrain-node/src/vnext_mixed_conformance.rs': ['fn frozen_n_minus_one_tcp_json_corpus_is_byte_exact'], 'src/onebrain-node/tests/vnext_node_runtime.rs': ['legacy_tcp_and_vnext_quic_exchange_concurrently_on_real_loopback']}
        for relative, names in expected.items():
            source = (validate_vnext_dr_m5.ROOT / relative).read_text(encoding="utf-8")
            for name in names:
                with self.subTest(path=relative, name=name):
                    self.assertIn(name, source)

if __name__ == "__main__":
    unittest.main()
