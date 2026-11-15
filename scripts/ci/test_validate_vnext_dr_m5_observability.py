from __future__ import annotations

import copy
import json
import unittest
from unittest import mock

from scripts.ci import validate_vnext_dr_m5
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


    def assert_read_mutation_rejected(self, path, mutate, message: str) -> None:
        real_read = validate_vnext_dr_m5.read

        def read_mutated(candidate):
            text = real_read(candidate)
            if candidate == path:
                mutated = mutate(text)
                self.assertNotEqual(mutated, text)
                return mutated
            return text

        with mock.patch.object(validate_vnext_dr_m5, "read", side_effect=read_mutated):
            with self.assertRaisesRegex(ContractError, message):
                validate_vnext_dr_m5_observability(frozen_profile())

    def assert_workflow_mutation_rejected(self, mutate) -> None:
        self.assert_read_mutation_rejected(
            validate_vnext_dr_m5.VNEXT_FOUNDATION_WORKFLOW, mutate, "CI acceptance gate"
        )

    def test_every_m5_02_rust_test_command_must_run_in_ci(self) -> None:
        commands = validate_vnext_dr_m5.DR_M5_OBSERVABILITY_TEST_COMMANDS
        self.assertEqual(len(commands), 7)
        for command in commands:
            with self.subTest(command=command.pattern):
                self.assert_workflow_mutation_rejected(
                    lambda text, command=command: command.sub("", text)
                )

    def test_m5_02_commands_must_stay_inside_the_m5_2_step(self) -> None:
        def move_profile_tests_to_m5_1(text: str) -> str:
            command = validate_vnext_dr_m5.DR_M5_OBSERVABILITY_TEST_COMMANDS[1]
            match = command.search(text)
            self.assertIsNotNone(match)
            text = text[: match.start()] + "true" + text[match.end() :]
            moved = text.replace(
                "- name: M5.1 unified resource admission and fair outbox\n        run: >-\n",
                "- name: M5.1 unified resource admission and fair outbox\n        run: >-\n"
                f"          {match.group(0)} &&\n",
            )
            self.assertNotEqual(moved, text)
            self.assertIsNotNone(command.search(moved))
            return moved

        self.assert_workflow_mutation_rejected(move_profile_tests_to_m5_1)

    def test_m5_2_step_cannot_disappear(self) -> None:
        self.assert_workflow_mutation_rejected(
            lambda text: text.replace(
                "- name: M5.2 typed observability and operator snapshot",
                "- name: M5.2 renamed",
            )
        )

    def test_network_runtime_cannot_swallow_an_untyped_error(self) -> None:
        self.assert_read_mutation_rejected(
            validate_vnext_dr_m5.ROOT / "src/onebrain-node/src/vnext_network_runtime.rs",
            lambda text: text + "\n// match result { Err(_) => {} }\n",
            "untyped error",
        )

    def test_m5_02_test_commands_match_the_rust_tests(self) -> None:
        root = validate_vnext_dr_m5.ROOT
        expected = {
            "src/onebrain-node/src/vnext_observability_profile_tests.rs": (
                "fn m5_02_observability_profile_",
            ),
            "src/onebrain-node/src/vnext_product_runtime.rs": (
                "async fn product_operations_feed_the_status_observability_snapshot",
            ),
            "src/onebrain-node/src/vnext_outbox.rs": (
                "fn legacy_v1_and_v2_intents_decode_with_unknown_age",
            ),
            "src/onebrain-node/src/vnext_network_runtime.rs": (
                "fn admission_and_payload_failures_map_to_stable_low_cardinality_reasons",
                "async fn two_runtime_listeners_authenticate_and_reject_unvalidated_payload_bytes",
            ),
            "src/onebrain-api/src/vnext_api.rs": (
                "async fn runtime_status_and_disabled_lane_keep_the_vnext_envelope",
            ),
        }
        for relative, names in expected.items():
            source = (root / relative).read_text(encoding="utf-8")
            for name in names:
                with self.subTest(path=relative, name=name):
                    self.assertIn(name, source)

if __name__ == "__main__":
    unittest.main()
