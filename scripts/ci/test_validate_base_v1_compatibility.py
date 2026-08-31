from __future__ import annotations

import copy
import json
import unittest

from scripts.ci.validate_vnext_contracts import (
    BASE_V1_COMPATIBILITY_PROFILE,
    BASE_V1_RUNTIME_INTERFACE_PROFILE,
    ContractError,
    validate_base_v1_compatibility,
)


class BaseV1CompatibilityContractTests(unittest.TestCase):
    def profile(self) -> dict[str, object]:
        return json.loads(BASE_V1_COMPATIBILITY_PROFILE.read_text(encoding="utf-8"))

    def assert_rejected(self, profile: dict[str, object], pattern: str) -> None:
        with self.assertRaisesRegex(ContractError, pattern):
            validate_base_v1_compatibility(profile)

    def test_frozen_compatibility_vectors_are_accepted(self) -> None:
        self.assertEqual(validate_base_v1_compatibility(self.profile()), 34)

    def test_digest_domains_are_frozen(self) -> None:
        profile = self.profile()
        profile["domains"]["candidate_semantic"] = "wrong"
        self.assert_rejected(profile, "digest domain")

    def test_tuple_order_cannot_drift(self) -> None:
        profile = self.profile()
        profile["tuple_fields"].reverse()
        self.assert_rejected(profile, "field order")

    def test_qualification_is_not_a_digest_field(self) -> None:
        profile = self.profile()
        profile["candidate_fields"].append("qualification")
        self.assert_rejected(profile, "field order")

    def test_machine_tuple_field_cannot_be_retyped(self) -> None:
        runtime = json.loads(
            BASE_V1_RUNTIME_INTERFACE_PROFILE.read_text(encoding="utf-8")
        )
        tuple_fields = runtime["type_definitions"]["BaseCompatibilityTuple"][
            "fields"
        ]
        next(field for field in tuple_fields if field["name"] == "toolchain")[
            "type"
        ] = "u32"
        with self.assertRaisesRegex(ContractError, "field declaration"):
            validate_base_v1_compatibility(self.profile(), runtime_profile=runtime)

    def test_qualification_cannot_be_moved_into_the_tuple(self) -> None:
        runtime = json.loads(
            BASE_V1_RUNTIME_INTERFACE_PROFILE.read_text(encoding="utf-8")
        )
        runtime["type_definitions"]["BaseCompatibilityTuple"]["fields"].append(
            {
                "id": 17,
                "name": "qualification",
                "type": "BaseQualificationState",
                "required": True,
                "ownership": "owned",
            }
        )
        with self.assertRaisesRegex(ContractError, "field declaration"):
            validate_base_v1_compatibility(self.profile(), runtime_profile=runtime)

    def test_golden_digests_are_exact(self) -> None:
        profile = self.profile()
        profile["golden_digests"]["artifact_tuple"] = "not-a-digest"
        self.assert_rejected(profile, "golden digest")

    def test_target_and_toolchain_are_artifact_only(self) -> None:
        profile = self.profile()
        target = next(case for case in profile["cases"] if case["id"] == "target")
        target["semantic_digest_changed"] = True
        self.assert_rejected(profile, "artifact-only")

    def test_every_tuple_field_has_a_case(self) -> None:
        profile = self.profile()
        profile["cases"] = [
            case for case in profile["cases"] if case["field"] != "registry_profile"
        ]
        self.assert_rejected(profile, "vector count|coverage")

    def test_unknown_identity_can_only_be_unqualified(self) -> None:
        profile = self.profile()
        unknown = next(
            case for case in profile["cases"] if case["id"] == "toolchain-unknown"
        )
        unknown["qualification"] = "eligible"
        self.assert_rejected(profile, "unqualified")

    def test_migration_binding_includes_trust_policy(self) -> None:
        profile = self.profile()
        del profile["migration_vector"]["trust_policy_digest"]
        self.assert_rejected(profile, "migration binding")

    def test_required_capability_failure_is_explicit(self) -> None:
        profile = self.profile()
        required = next(
            case for case in profile["cases"] if case["id"] == "required-feature"
        )
        required["outcome"] = "compatible"
        self.assert_rejected(profile, "decision vector")

    def test_minor_floors_are_independent(self) -> None:
        profile = self.profile()
        minimum = copy.deepcopy(profile["minimum_additive"])
        minimum["wire_session_minor"] = minimum.pop("base_minor")
        profile["minimum_additive"] = minimum
        self.assert_rejected(profile, "minor floors")


if __name__ == "__main__":
    unittest.main()
