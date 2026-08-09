from __future__ import annotations

import copy
import json
import unittest

from scripts.ci.validate_vnext_contracts import (
    ContractError,
    P5_MULTI_HOST_PRODUCTION_PROFILE,
    validate_vnext_p5_multi_host,
)


def frozen_profile() -> dict[str, object]:
    return json.loads(P5_MULTI_HOST_PRODUCTION_PROFILE.read_text(encoding="utf-8"))


class VNextP5MultiHostProductionTests(unittest.TestCase):
    def assert_rejected(
        self, profile: dict[str, object], message: str
    ) -> None:
        with self.assertRaisesRegex(ContractError, message):
            validate_vnext_p5_multi_host(profile)

    def test_frozen_profile_is_accepted(self) -> None:
        self.assertEqual(
            validate_vnext_p5_multi_host(frozen_profile()),
            (3, 3, 13, 10, 4),
        )

    def test_one_physical_host_cannot_claim_multi_host(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["scope"]["physical_host_count"] = 1
        self.assert_rejected(profile, "scope")

    def test_shared_durable_root_or_principal_is_rejected(self) -> None:
        for field in ("durable_root_slot", "principal_slot"):
            with self.subTest(field=field):
                profile = copy.deepcopy(frozen_profile())
                profile["topology"]["hosts"][1][field] = profile["topology"][
                    "hosts"
                ][0][field]
                self.assert_rejected(profile, "topology")

    def test_model_transport_is_rejected(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["scope"]["transport"] = "modeled"
        self.assert_rejected(profile, "scope")

    def test_signed_control_receipt_is_mandatory(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["control_plane"]["signed_agent_receipt_required"] = False
        self.assert_rejected(profile, "control")

    def test_release_request_digest_is_mandatory_and_identical(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["child_receipt"]["required_bindings"].remove(
            "release_request_digest"
        )
        self.assert_rejected(profile, "receipt")

        profile = copy.deepcopy(frozen_profile())
        profile["aggregate"]["identical_child_bindings"].remove(
            "release_request_digest"
        )
        self.assert_rejected(profile, "aggregate")

        profile = copy.deepcopy(frozen_profile())
        profile["aggregate"]["mixed_binding_policy"] = "accept"
        self.assert_rejected(profile, "aggregate")

    def test_valid_but_unlisted_signer_is_rejected(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["trust_policy"]["valid_unlisted_signature"] = "accept"
        self.assert_rejected(profile, "trust policy")

    def test_wrong_role_and_cross_host_key_reuse_are_rejected(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["trust_policy"]["policy"]["role_bindings"][0]["role"] = (
            "p5-orchestrator"
        )
        self.assert_rejected(profile, "role")

        profile = copy.deepcopy(frozen_profile())
        bindings = profile["trust_policy"]["policy"]["role_bindings"]
        bindings[1]["public_key_hex"] = bindings[0]["public_key_hex"]
        bindings[1]["fingerprint_hex"] = bindings[0]["fingerprint_hex"]
        self.assert_rejected(profile, "key reuse")

    def test_wrong_release_request_binding_is_rejected(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["child_receipt"]["binding_match"] = "caller-selected"
        self.assert_rejected(profile, "receipt")

    def test_trust_policy_and_session_must_match_across_children(self) -> None:
        for binding in ("trust_policy_digest", "qualification_session_id"):
            with self.subTest(binding=binding):
                profile = copy.deepcopy(frozen_profile())
                profile["aggregate"]["identical_child_bindings"].remove(binding)
                self.assert_rejected(profile, "aggregate")

    def test_aggregate_root_excludes_report_and_signature(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["aggregate"]["root_inputs"].append("aggregate_report")
        self.assert_rejected(profile, "aggregate")

    def test_three_distinct_hosts_are_mandatory(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["topology"]["hosts"].pop()
        self.assert_rejected(profile, "topology")

    def test_complete_fault_matrix_is_mandatory(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["fault_matrix"].remove("partition")
        self.assert_rejected(profile, "fault")

    def test_before_after_roots_are_mandatory(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["child_receipt"]["required_bindings"].remove("before_roots")
        self.assert_rejected(profile, "receipt")

    def test_resource_bounds_are_mandatory(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        del profile["resource_bounds"]["max_peak_rss_bytes_per_host"]
        self.assert_rejected(profile, "resource")

    def test_preflight_cannot_claim_production(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["preflight_boundary"]["single_host_multi_host_qualified"] = True
        self.assert_rejected(profile, "preflight")

    def test_reference_identity_cannot_be_overridden(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["reference_environment"]["producer_override"] = True
        self.assert_rejected(profile, "reference environment")

    def test_legacy_backup_remains_preflight_only(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["archive_restore"]["preflight_profile_may_qualify"] = True
        self.assert_rejected(profile, "archive")


if __name__ == "__main__":
    unittest.main()
