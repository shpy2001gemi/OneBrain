from __future__ import annotations

import copy
import json
import unittest

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


if __name__ == "__main__":
    unittest.main()
