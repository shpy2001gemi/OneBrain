from __future__ import annotations

import copy
import json
import unittest
from unittest import mock

from scripts.ci import validate_vnext_contracts
from scripts.ci.validate_vnext_contracts import (
    ContractError,
    P5_OPERATIONS_PREFLIGHT_PROFILE,
    validate_vnext_p5_operations_preflight,
)


def frozen_profile() -> dict[str, object]:
    return json.loads(P5_OPERATIONS_PREFLIGHT_PROFILE.read_text(encoding="utf-8"))


class VNextP5OperationsPreflightTests(unittest.TestCase):
    def test_frozen_profile_is_accepted(self) -> None:
        self.assertEqual(
            validate_vnext_p5_operations_preflight(frozen_profile()),
            (3, 7, 4, 12, 10, 10, 3),
        )

    def test_preflight_cannot_consume_72h_evidence(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["scope"]["consumes_pre_release_72h_evidence"] = True
        with self.assertRaisesRegex(ContractError, "scope"):
            validate_vnext_p5_operations_preflight(profile)

    def test_preflight_cannot_claim_production(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["scope"]["production_canary_qualifying"] = True
        with self.assertRaisesRegex(ContractError, "scope"):
            validate_vnext_p5_operations_preflight(profile)

    def test_signer_fault_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["p5_02_fault_drills"]["faults"].remove(
            "session-signer-unavailable-before-durable-side-effect"
        )
        with self.assertRaisesRegex(ContractError, "P5-02 fault"):
            validate_vnext_p5_operations_preflight(profile)

    def test_disk_pressure_oracle_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["p5_02_fault_drills"]["exit_oracles"].remove(
            "rejected-storage-reason-visible"
        )
        with self.assertRaisesRegex(ContractError, "P5-02 exit"):
            validate_vnext_p5_operations_preflight(profile)

    def test_required_durable_file_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["p5_03_backup_restore"]["required_durable_files"].remove(
            "vnext_reconciliation.redb"
        )
        with self.assertRaisesRegex(ContractError, "P5-03 durable"):
            validate_vnext_p5_operations_preflight(profile)

    def test_corruption_gate_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["p5_03_backup_restore"]["integrity"].remove(
            "corruption-fails-before-restore-target-creation"
        )
        with self.assertRaisesRegex(ContractError, "P5-03 integrity"):
            validate_vnext_p5_operations_preflight(profile)

    def test_rollback_lane_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["p5_04_rollback_reenable"]["lanes"].remove("network")
        with self.assertRaisesRegex(ContractError, "P5-04 lane"):
            validate_vnext_p5_operations_preflight(profile)

    def test_stale_config_cannot_reenable(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["p5_05_default_off_rollout"]["stale_config_may_enable"] = True
        with self.assertRaisesRegex(ContractError, "P5-05"):
            validate_vnext_p5_operations_preflight(profile)

    def test_local_kql_offline_oracle_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["p5_05_default_off_rollout"][
            "local_kql_round_trip_with_network_off"
        ] = False
        with self.assertRaisesRegex(ContractError, "P5-05"):
            validate_vnext_p5_operations_preflight(profile)

    def test_dashboard_signal_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["p5_06_operator_dashboard"]["signals"].remove("outbox-count-and-age")
        with self.assertRaisesRegex(ContractError, "P5-06 signal"):
            validate_vnext_p5_operations_preflight(profile)

    def test_dashboard_privacy_cannot_expand(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["p5_06_operator_dashboard"]["privacy"]["contains_node_id"] = True
        with self.assertRaisesRegex(ContractError, "P5-06 privacy"):
            validate_vnext_p5_operations_preflight(profile)

    def test_external_gates_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["remaining_external_gates"].remove("pinned-pre-release-72h-artifact")
        with self.assertRaisesRegex(ContractError, "external gate"):
            validate_vnext_p5_operations_preflight(profile)


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
            with self.assertRaisesRegex(ContractError, 'PR acceptance gate'):
                validate_vnext_p5_operations_preflight(frozen_profile())

    def test_every_p5_operations_test_commands_command_must_run_in_its_step(self) -> None:
        commands = validate_vnext_contracts.P5_OPERATIONS_TEST_COMMANDS
        self.assertEqual(len(commands), 3)
        for command in commands:
            with self.subTest(command=command.pattern):
                self.assert_workflow_mutation_rejected_arch13(
                    lambda text, command=command: command.sub("", text)
                )

    def test_arch13_step_0_cannot_disappear(self) -> None:
        self.assert_workflow_mutation_rejected_arch13(
            lambda text: text.replace('- name: P5.2-P5.6 operational fault backup rollback and dashboard preflight', '- name: renamed step')
        )

    def test_arch13_commands_match_the_rust_tests(self) -> None:
        expected = {'src/onebrain-node/src/vnext_p5_profile_tests.rs': ['fn p5_operations_profile_binds_backup_lanes_and_incident_codes'], 'src/onebrain-node/src/vnext_p5_operations.rs': ['async fn p5_02_through_p5_06_operational_preflight_passes_without_72h']}
        for relative, names in expected.items():
            source = (validate_vnext_contracts.ROOT / relative).read_text(encoding="utf-8")
            for name in names:
                with self.subTest(path=relative, name=name):
                    self.assertIn(name, source)

if __name__ == "__main__":
    unittest.main()
