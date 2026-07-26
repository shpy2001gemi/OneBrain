from __future__ import annotations

import copy
import json
import unittest

from scripts.ci.validate_vnext_contracts import (
    ContractError,
    DR_M5_OPERATIONAL_COMPACTION_PROFILE,
    validate_vnext_dr_m5_operational_compaction,
)


def frozen_profile() -> dict[str, object]:
    return json.loads(
        DR_M5_OPERATIONAL_COMPACTION_PROFILE.read_text(encoding="utf-8")
    )


class VNextDrM5OperationalCompactionTests(unittest.TestCase):
    def test_frozen_profile_is_accepted(self) -> None:
        self.assertEqual(
            validate_vnext_dr_m5_operational_compaction(frozen_profile()),
            (5, 5, 25, 2, 5),
        )

    def test_compaction_harness_cannot_become_default_enabled(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["feature"]["default_enabled"] = True
        with self.assertRaisesRegex(ContractError, "firewall"):
            validate_vnext_dr_m5_operational_compaction(profile)

    def test_kill_switch_cannot_start_enabled(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["kill_switch"]["default_enabled"] = True
        with self.assertRaisesRegex(ContractError, "kill-switch"):
            validate_vnext_dr_m5_operational_compaction(profile)

    def test_stale_permit_cannot_commit(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["kill_switch"]["stale_permit_commits"] = True
        with self.assertRaisesRegex(ContractError, "kill-switch"):
            validate_vnext_dr_m5_operational_compaction(profile)

    def test_pending_journal_state_cannot_become_compactable(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["journal"]["allowed_compactable_states"].append("pending")
        with self.assertRaisesRegex(ContractError, "journal"):
            validate_vnext_dr_m5_operational_compaction(profile)

    def test_missing_dependency_cannot_lose_protection(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["journal"]["protected_states"].remove("missing_dependency")
        with self.assertRaisesRegex(ContractError, "journal"):
            validate_vnext_dr_m5_operational_compaction(profile)

    def test_outbox_audit_cannot_move_after_delete(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["outbox"]["audit_before_delete"] = False
        with self.assertRaisesRegex(ContractError, "outbox"):
            validate_vnext_dr_m5_operational_compaction(profile)

    def test_pending_outbox_cannot_become_terminal(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["outbox"]["terminal_states"].append("pending")
        with self.assertRaisesRegex(ContractError, "outbox"):
            validate_vnext_dr_m5_operational_compaction(profile)

    def test_evidence_record_cap_cannot_expand(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["bounded_evidence"]["max_records_per_lane"] = 4_097
        with self.assertRaisesRegex(ContractError, "bounded evidence"):
            validate_vnext_dr_m5_operational_compaction(profile)

    def test_overflow_idempotency_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["bounded_evidence"]["retry_last_overflow_idempotent"] = False
        with self.assertRaisesRegex(ContractError, "bounded evidence"):
            validate_vnext_dr_m5_operational_compaction(profile)

    def test_derived_snapshot_root_cannot_drift(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["derived_snapshots"]["fixture"]["kql_source_root_blake3"] = "0" * 64
        with self.assertRaisesRegex(ContractError, "derived snapshot"):
            validate_vnext_dr_m5_operational_compaction(profile)

    def test_process_kill_boundary_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["process_kill"]["boundaries"].remove("TX-CMP-IDX-001")
        with self.assertRaisesRegex(ContractError, "process-kill"):
            validate_vnext_dr_m5_operational_compaction(profile)

    def test_process_kill_case_count_cannot_shrink(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["process_kill"]["required_process_kill_cases"] = 24
        with self.assertRaisesRegex(ContractError, "process-kill"):
            validate_vnext_dr_m5_operational_compaction(profile)

    def test_physical_disk_exit_oracle_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["exit_oracles"].remove("physical_disk_bytes_decrease")
        with self.assertRaisesRegex(ContractError, "exit oracle"):
            validate_vnext_dr_m5_operational_compaction(profile)


if __name__ == "__main__":
    unittest.main()
