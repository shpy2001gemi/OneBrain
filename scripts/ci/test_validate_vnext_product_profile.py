from __future__ import annotations

import copy
import json
import unittest

from scripts.ci.validate_vnext_contracts import (
    ContractError,
    PRODUCT_PROFILE,
    validate_product_integration_profile,
)


def frozen_profile() -> dict[str, object]:
    return json.loads(PRODUCT_PROFILE.read_text(encoding="utf-8"))


class ProductIntegrationProfileTests(unittest.TestCase):
    def test_frozen_profile_is_accepted(self) -> None:
        endpoints, dtos = validate_product_integration_profile(frozen_profile())
        self.assertEqual(endpoints, 15)
        self.assertEqual(dtos, 20)

    def test_endpoint_cannot_escape_vnext_namespace(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["endpoints"][0]["path"] = "/api/kql"
        with self.assertRaisesRegex(ContractError, "escaped additive namespace"):
            validate_product_integration_profile(profile)

    def test_base_negotiation_requires_additive_minor_1(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["profile_minor"] = 0
        with self.assertRaisesRegex(ContractError, "version"):
            validate_product_integration_profile(profile)

    def test_base_negotiation_endpoint_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["endpoints"] = [
            row
            for row in profile["endpoints"]
            if row["path"] != "/api/vnext/base/negotiate"
        ]
        with self.assertRaisesRegex(ContractError, "endpoint inventory"):
            validate_product_integration_profile(profile)

    def test_client_cannot_supply_authority_frontier(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["dtos"]["NeedScanRequestV1"]["required"].append(
            "authority_frontier"
        )
        profile["dto_field_types"]["NeedScanRequestV1"][
            "authority_frontier"
        ] = "CidHexV1"
        with self.assertRaisesRegex(ContractError, "client can supply"):
            validate_product_integration_profile(profile)

    def test_private_need_endpoint_cannot_become_public(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["endpoints"][2]["visibility"] = "authenticated_local"
        with self.assertRaisesRegex(ContractError, "visibility drift"):
            validate_product_integration_profile(profile)

    def test_proposal_cannot_become_executable(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["semantic_firewalls"]["proposal_executable"] = True
        with self.assertRaisesRegex(ContractError, "fail-closed"):
            validate_product_integration_profile(profile)

    def test_proposal_executable_field_cannot_become_general_boolean(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["dto_field_types"]["QuarantinedMatchV1"]["executable"] = "boolean"
        with self.assertRaisesRegex(ContractError, "weakened its literal types"):
            validate_product_integration_profile(profile)

    def test_metabolic_view_cannot_authorize_reward(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["semantic_firewalls"]["pomv_authorizes_reward"] = True
        with self.assertRaisesRegex(ContractError, "fail-closed"):
            validate_product_integration_profile(profile)

    def test_legacy_endpoint_meaning_cannot_change(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["legacy_surfaces"][0]["meaning"] = "distributed_kql"
        with self.assertRaisesRegex(ContractError, "legacy product meaning"):
            validate_product_integration_profile(profile)


if __name__ == "__main__":
    unittest.main()
