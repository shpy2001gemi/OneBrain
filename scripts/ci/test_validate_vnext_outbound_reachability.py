from __future__ import annotations

import copy
import json
import unittest

from scripts.ci.validate_vnext_contracts import (
    ContractError,
    OUTBOUND_FIRST_REACHABILITY_PROFILE,
    validate_vnext_outbound_reachability,
)


def frozen_profile() -> dict[str, object]:
    return json.loads(OUTBOUND_FIRST_REACHABILITY_PROFILE.read_text(encoding="utf-8"))


class VNextOutboundFirstReachabilityTests(unittest.TestCase):
    def assert_rejected(self, profile: dict[str, object], message: str) -> None:
        with self.assertRaisesRegex(ContractError, message):
            validate_vnext_outbound_reachability(profile)

    def test_frozen_profile_is_accepted(self) -> None:
        self.assertEqual(
            validate_vnext_outbound_reachability(frozen_profile()),
            (6, 19, 4, 8, 12),
        )

    def test_unknown_and_missing_root_fields_are_rejected(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["unknown"] = True
        self.assert_rejected(profile, "fields")

        profile = copy.deepcopy(frozen_profile())
        del profile["canonical_objects"]
        self.assert_rejected(profile, "fields")

    def test_unknown_object_field_and_noncanonical_bytes_are_rejected(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["canonical_objects"]["relay_descriptor"]["fields"].append("vendor")
        self.assert_rejected(profile, "object")

        profile = copy.deepcopy(frozen_profile())
        profile["canonical_encoding"]["byte_for_byte_reencode_required"] = False
        self.assert_rejected(profile, "canonical")

    def test_wrong_signature_domain_is_rejected(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["signature_domains"]["relay_descriptor"] = "onebrain/wrong"
        self.assert_rejected(profile, "signature")

    def test_every_frozen_limit_is_exact_and_one_over_is_rejected(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["limits"]["endpoints_per_object"] = 9
        self.assert_rejected(profile, "limits")

        profile = copy.deepcopy(frozen_profile())
        del profile["limits"]["relay_global_queue_bytes"]
        self.assert_rejected(profile, "limits")

    def test_private_candidates_and_nat_configuration_are_rejected(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["privacy"]["public_candidate_kinds"].append("host-private")
        self.assert_rejected(profile, "privacy")

        profile = copy.deepcopy(frozen_profile())
        profile["outbound_only_baseline"]["operator_nat_configuration"] = "optional"
        self.assert_rejected(profile, "outbound")

    def test_expiry_sequence_and_replay_guards_cannot_be_weakened(self) -> None:
        for key in ("expiry_required", "monotonic_sequence_required", "replay_rejected"):
            with self.subTest(key=key):
                profile = copy.deepcopy(frozen_profile())
                profile["admission"][key] = False
                self.assert_rejected(profile, "admission")

    def test_platform_capabilities_are_closed_non_authoritative_and_os_neutral(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["platform_capabilities"]["capabilities"].append("linux-systemd")
        self.assert_rejected(profile, "platform")

        profile = copy.deepcopy(frozen_profile())
        profile["platform_capabilities"]["may_create_authority"] = True
        self.assert_rejected(profile, "platform")

    def test_no_mandatory_central_service_and_permissionless_relays_are_frozen(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["discovery"]["mandatory_onebrain_service"] = "seed.onebrain.example"
        self.assert_rejected(profile, "discovery")

        profile = copy.deepcopy(frozen_profile())
        profile["relay_governance"]["owner_approval_required"] = True
        self.assert_rejected(profile, "relay")

    def test_route_path_classes_and_web_projection_are_exact(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["path_classes"]["direct_class"].remove("hole-punched")
        self.assert_rejected(profile, "path")

        profile = copy.deepcopy(frozen_profile())
        profile["platform_capabilities"]["web_path_projection"]["websocket-tls"] = "direct"
        self.assert_rejected(profile, "platform")

    def test_mutation_catalog_is_complete(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["required_mutations"].remove("checkpoint-mismatch")
        self.assert_rejected(profile, "mutation")


if __name__ == "__main__":
    unittest.main()
