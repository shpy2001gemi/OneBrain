from __future__ import annotations

import copy
import json
import unittest
from unittest import mock

from scripts.ci import validate_vnext_dr_m5
from scripts.ci.validate_vnext_contracts import (
    ContractError,
    DR_M5_CRASH_HARNESS_PROFILE,
    validate_vnext_dr_m5_crash_harness,
)


def frozen_profile() -> dict[str, object]:
    return json.loads(DR_M5_CRASH_HARNESS_PROFILE.read_text(encoding="utf-8"))


class VNextDrM5CrashHarnessTests(unittest.TestCase):
    def test_frozen_profile_is_accepted(self) -> None:
        self.assertEqual(
            validate_vnext_dr_m5_crash_harness(frozen_profile()), (13, 5, 65, 4)
        )

    def test_failpoints_cannot_become_default_enabled(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["feature"]["default_enabled"] = True
        with self.assertRaisesRegex(ContractError, "firewall"):
            validate_vnext_dr_m5_crash_harness(profile)

    def test_failpoint_phase_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["phases"].remove("after_mutation_before_commit")
        with self.assertRaisesRegex(ContractError, "phase"):
            validate_vnext_dr_m5_crash_harness(profile)

    def test_transaction_boundary_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["boundaries"].remove("TX-AUTH-001")
        with self.assertRaisesRegex(ContractError, "boundary"):
            validate_vnext_dr_m5_crash_harness(profile)

    def test_full_process_kill_matrix_cannot_shrink(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["required_process_kill_cases"] = 64
        with self.assertRaisesRegex(ContractError, "case count"):
            validate_vnext_dr_m5_crash_harness(profile)

    def test_restart_cannot_create_a_missing_store(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["process_kill"]["restart_uses_open_not_create"] = False
        with self.assertRaisesRegex(ContractError, "child-process"):
            validate_vnext_dr_m5_crash_harness(profile)

    def test_recovery_oracle_field_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["oracle"]["fields"].remove("authority_decisions")
        with self.assertRaisesRegex(ContractError, "oracle"):
            validate_vnext_dr_m5_crash_harness(profile)

    def test_crash_report_cannot_claim_network_completion(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["report"]["claims_network_completion"] = True
        with self.assertRaisesRegex(ContractError, "report"):
            validate_vnext_dr_m5_crash_harness(profile)

    def test_storage_fault_inventory_cannot_shrink(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["storage_faults"].remove("truncated_store")
        with self.assertRaisesRegex(ContractError, "storage-fault"):
            validate_vnext_dr_m5_crash_harness(profile)

    def test_owner_hook_binding_cannot_drift(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["owner_hooks"][0]["path"] = (
            "src/onebrain-node/src/vnext_crash_harness.rs"
        )
        with self.assertRaisesRegex(ContractError, "owner-hook"):
            validate_vnext_dr_m5_crash_harness(profile)

    def test_crash_report_digest_cannot_drift(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["report"]["expected_sha256"] = "0" * 64
        with self.assertRaisesRegex(ContractError, "report"):
            validate_vnext_dr_m5_crash_harness(profile)

    def test_failpoint_rust_tests_must_run_in_ci(self) -> None:
        real_read = validate_vnext_dr_m5.read

        def read_without_failpoint_step(path):
            text = real_read(path)
            if path == validate_vnext_dr_m5.VNEXT_FOUNDATION_WORKFLOW:
                text = text.replace("--lib foundation::dr_m5_failpoint", "")
            return text

        with mock.patch.object(
            validate_vnext_dr_m5, "read", side_effect=read_without_failpoint_step
        ):
            with self.assertRaisesRegex(ContractError, "CI acceptance gate"):
                validate_vnext_dr_m5_crash_harness(frozen_profile())

    def assert_workflow_mutation_rejected(self, mutate) -> None:
        real_read = validate_vnext_dr_m5.read

        def read_mutated(path):
            text = real_read(path)
            if path == validate_vnext_dr_m5.VNEXT_FOUNDATION_WORKFLOW:
                mutated = mutate(text)
                self.assertNotEqual(mutated, text)
                return mutated
            return text

        with mock.patch.object(validate_vnext_dr_m5, "read", side_effect=read_mutated):
            with self.assertRaisesRegex(ContractError, "CI acceptance gate"):
                validate_vnext_dr_m5_crash_harness(frozen_profile())

    def test_owner_hook_trace_tests_must_run_in_ci_for_every_owner_crate(self) -> None:
        for crate in ("ku-core", "ku-net", "ku-kql", "onebrain-node"):
            with self.subTest(crate=crate):
                command = validate_vnext_dr_m5.dr_m5_owner_hook_test_command(crate)
                self.assert_workflow_mutation_rejected(lambda text: command.sub("", text))

    def test_harness_rust_tests_must_run_in_ci(self) -> None:
        self.assert_workflow_mutation_rejected(
            lambda text: text.replace("--lib vnext_crash_harness::tests", "")
        )

    def test_owner_hook_crate_without_trace_command_is_rejected(self) -> None:
        features = dict(validate_vnext_dr_m5.DR_M5_OWNER_HOOK_TEST_FEATURES)
        del features["ku-kql"]
        with mock.patch.object(
            validate_vnext_dr_m5, "DR_M5_OWNER_HOOK_TEST_FEATURES", features
        ):
            with self.assertRaisesRegex(ContractError, "hook-trace test command"):
                validate_vnext_dr_m5_crash_harness(frozen_profile())


if __name__ == "__main__":
    unittest.main()
