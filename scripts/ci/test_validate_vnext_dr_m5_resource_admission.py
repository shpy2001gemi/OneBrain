from __future__ import annotations

import copy
import json
import unittest

from scripts.ci.validate_vnext_contracts import (
    ContractError,
    DR_M5_RESOURCE_PROFILE,
    validate_vnext_dr_m5_resource_admission,
)


def frozen_profile() -> dict[str, object]:
    return json.loads(DR_M5_RESOURCE_PROFILE.read_text(encoding="utf-8"))


class VNextDrM5ResourceAdmissionTests(unittest.TestCase):
    def test_frozen_profile_is_accepted(self) -> None:
        self.assertEqual(
            validate_vnext_dr_m5_resource_admission(frozen_profile()), (3, 13, 3)
        )

    def test_pipeline_stage_cannot_be_skipped(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["pipeline"].remove("journal")
        with self.assertRaisesRegex(ContractError, "pipeline"):
            validate_vnext_dr_m5_resource_admission(profile)

    def test_carrier_preallocation_cap_cannot_grow(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["allocation_lanes"][1]["max_bytes"] += 1
        with self.assertRaisesRegex(ContractError, "allocation lane"):
            validate_vnext_dr_m5_resource_admission(profile)

    def test_per_ip_quota_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        del profile["default_quotas"]["sessions_per_ip"]
        with self.assertRaisesRegex(ContractError, "quota"):
            validate_vnext_dr_m5_resource_admission(profile)

    def test_durable_store_cannot_become_unbounded(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["bounded_durable_state"]["accepted_records"] = 0
        with self.assertRaisesRegex(ContractError, "durable-state"):
            validate_vnext_dr_m5_resource_admission(profile)

    def test_incremental_page_cannot_become_full_scan(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["incremental_scans"]["max_page_records"] = 65_536
        with self.assertRaisesRegex(ContractError, "incremental scan"):
            validate_vnext_dr_m5_resource_admission(profile)

    def test_retry_exhausted_cannot_reenter_pending_state_set(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["outbox"]["states"].remove("RetryExhausted")
        with self.assertRaisesRegex(ContractError, "outbox"):
            validate_vnext_dr_m5_resource_admission(profile)

    def test_scoped_status_cannot_claim_network_completion(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["claims_network_completion"] = True
        with self.assertRaisesRegex(ContractError, "profile"):
            validate_vnext_dr_m5_resource_admission(profile)


if __name__ == "__main__":
    unittest.main()
