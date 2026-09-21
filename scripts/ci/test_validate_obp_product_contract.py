"""Contract mutations; no network activation or simulated qualification."""
import copy
import unittest

from scripts.ci.validate_obp_product_contract import (
    ObpContractError, load_profile, validate_contract, validate_value,
)


class ObpProductContractTests(unittest.TestCase):
    def setUp(self):
        self.profile = load_profile()

    def reject(self, path, value):
        profile = copy.deepcopy(self.profile)
        cursor = profile
        for key in path[:-1]:
            cursor = cursor[key]
        cursor[path[-1]] = value
        with self.assertRaises(ObpContractError):
            validate_contract(profile)

    def test_complete_proposal(self):
        self.assertEqual(validate_contract(self.profile), (13, 18, 26))

    def test_review_and_default_off_cannot_be_bypassed(self):
        self.reject(("runtime_implementation_authorized",), False)
        self.reject(("owner_decision",), "unreviewed")
        self.reject(("status",), "owner-review-required")
        self.reject(("defaults", "outbound_first_requested"), True)
        self.reject(("defaults", "advertise_reachability"), True)
        self.reject(("new_rest_routes",), ["/api/vnext/network/connect"])
        self.reject(("new_ws_events",), ["private_route_detail"])

    def test_wire_and_frozen_resources_stay_exact(self):
        self.reject(("wire_changes",), True)
        self.reject(("core_limits", "attempts"), 13)
        self.reject(("types", "PathKind", "values"), ["trusted-relay"])
        path = next(iter(self.profile["frozen_inputs"]))
        self.reject(("frozen_inputs", path), "0" * 64)

    def test_management_and_generation_fences(self):
        self.reject(("operations", 1, "access"), "product_control")
        self.reject(("dtos", "ObpConfigureV1", "required", "expected_generation"), "Count")
        self.reject(("dtos", "ObpSourceAdmitV1", "optional"), {"private_key":"InputRef"})

    def test_no_authority_or_invented_outbox_state(self):
        self.reject(("types", "False", "value"), True)
        self.reject(("types", "IntentState", "values"), ["pending","delivered"])
        self.reject(("dtos", "ObpRouteV1", "required", "authorizes_reward"), "Boolean")

    def test_adversarial_fixtures_are_rejected(self):
        for fixture in self.profile["fixtures"]:
            if not fixture["valid"]:
                with self.subTest(fixture=fixture["name"]):
                    with self.assertRaises(ObpContractError):
                        validate_value(self.profile, fixture["dto"], fixture["value"])


if __name__ == "__main__":
    unittest.main()
