from __future__ import annotations

import copy
import json
import unittest

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


if __name__ == "__main__":
    unittest.main()
