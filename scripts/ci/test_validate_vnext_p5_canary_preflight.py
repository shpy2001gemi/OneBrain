from __future__ import annotations

import copy
import json
import unittest
from unittest import mock

from scripts.ci import validate_vnext_contracts
from scripts.ci.validate_vnext_contracts import (
    ContractError,
    P5_CANARY_PREFLIGHT_PROFILE,
    validate_vnext_p5_canary_preflight,
)


def frozen_profile() -> dict[str, object]:
    return json.loads(P5_CANARY_PREFLIGHT_PROFILE.read_text(encoding="utf-8"))


class VNextP5CanaryPreflightTests(unittest.TestCase):
    def test_frozen_profile_is_accepted(self) -> None:
        self.assertEqual(
            validate_vnext_p5_canary_preflight(frozen_profile()),
            (3, 3, 6, 4, 8),
        )

    def test_preflight_cannot_claim_production_qualification(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["scope"]["production_canary_qualifying"] = True
        with self.assertRaisesRegex(ContractError, "scope"):
            validate_vnext_p5_canary_preflight(profile)

    def test_real_quic_cannot_be_replaced_by_a_model(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["scope"]["transport"] = "modeled"
        with self.assertRaisesRegex(ContractError, "scope"):
            validate_vnext_p5_canary_preflight(profile)

    def test_three_independent_nodes_cannot_be_collapsed(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["topology"]["independent_principals"] = 1
        with self.assertRaisesRegex(ContractError, "topology"):
            validate_vnext_p5_canary_preflight(profile)

    def test_route_observation_floor_cannot_be_lowered(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["topology"]["minimum_authenticated_route_observations"] = 3
        with self.assertRaisesRegex(ContractError, "topology"):
            validate_vnext_p5_canary_preflight(profile)

    def test_partition_drill_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["fault_drills"].remove("old-route-partition")
        with self.assertRaisesRegex(ContractError, "fault drill"):
            validate_vnext_p5_canary_preflight(profile)

    def test_idempotency_oracle_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["exit_oracles"].remove("replayed-feed-has-one-durable-branch")
        with self.assertRaisesRegex(ContractError, "exit oracle"):
            validate_vnext_p5_canary_preflight(profile)

    def test_72h_gate_cannot_be_bypassed(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["production_gate"]["requires_pre_release_72h"] = False
        with self.assertRaisesRegex(ContractError, "production gate"):
            validate_vnext_p5_canary_preflight(profile)

    def test_multi_host_gate_cannot_be_bypassed(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["production_gate"]["requires_multi_host_canary"] = False
        with self.assertRaisesRegex(ContractError, "production gate"):
            validate_vnext_p5_canary_preflight(profile)


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
                validate_vnext_p5_canary_preflight(frozen_profile())

    def test_every_p5_canary_test_commands_command_must_run_in_its_step(self) -> None:
        commands = validate_vnext_contracts.P5_CANARY_TEST_COMMANDS
        self.assertEqual(len(commands), 2)
        for command in commands:
            with self.subTest(command=command.pattern):
                self.assert_workflow_mutation_rejected_arch13(
                    lambda text, command=command: command.sub("", text)
                )

    def test_arch13_step_0_cannot_disappear(self) -> None:
        self.assert_workflow_mutation_rejected_arch13(
            lambda text: text.replace('- name: P5.1 three-node authenticated QUIC canary preflight', '- name: renamed step')
        )

    def test_arch13_commands_match_the_rust_tests(self) -> None:
        expected = {'src/onebrain-node/src/vnext_p5_profile_tests.rs': ['fn p5_01_canary_profile_binds_topology_constants'], 'src/onebrain-node/src/vnext_canary_operations.rs': ['async fn p5_01_three_node_real_quic_partition_restart_route_change_reunion', 'async fn p5_01_nonempty_node_directory_fails_before_runtime_start']}
        for relative, names in expected.items():
            source = (validate_vnext_contracts.ROOT / relative).read_text(encoding="utf-8")
            for name in names:
                with self.subTest(path=relative, name=name):
                    self.assertIn(name, source)

if __name__ == "__main__":
    unittest.main()
