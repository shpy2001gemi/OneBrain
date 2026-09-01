from __future__ import annotations

import copy
import json
import unittest

import blake3

from scripts.ci.validate_vnext_contracts import (
    CONCEPT_REGISTRY_PRODUCTION_QUALIFICATION_PROFILE,
    ContractError,
    validate_concept_registry_production_qualification,
)


def frozen_profile() -> dict[str, object]:
    return json.loads(
        CONCEPT_REGISTRY_PRODUCTION_QUALIFICATION_PROFILE.read_text(encoding="utf-8")
    )


class ConceptRegistryProductionQualificationContractTests(unittest.TestCase):
    def test_frozen_profile_is_accepted(self) -> None:
        self.assertEqual(
            validate_concept_registry_production_qualification(frozen_profile()),
            (5, 4, 7, 1),
        )

    def test_undersized_registry_data_payload_is_rejected(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["release_package"]["registry_data_size_bytes"]["minimum"] = 2_199_999_999
        with self.assertRaisesRegex(ContractError, "data size"):
            validate_concept_registry_production_qualification(profile)

    def test_oversized_registry_data_payload_is_rejected(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["release_package"]["registry_data_size_bytes"]["maximum"] = 2_500_000_001
        with self.assertRaisesRegex(ContractError, "data size"):
            validate_concept_registry_production_qualification(profile)

    def test_registry_data_payload_artifact_set_is_closed(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["release_package"]["registry_data_size_bytes"]["artifacts"].append(
            "sbom.spdx.json"
        )
        with self.assertRaisesRegex(ContractError, "data size"):
            validate_concept_registry_production_qualification(profile)

    def test_budget_cannot_be_changed(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["resource_profiles"]["ssd"]["max_lookup_p95_us"] = 100_001
        with self.assertRaisesRegex(ContractError, "resource budget"):
            validate_concept_registry_production_qualification(profile)

    def test_ssd_os_evidence_is_required(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["resource_profiles"]["ssd"]["storage_evidence"].pop()
        with self.assertRaisesRegex(ContractError, "storage evidence"):
            validate_concept_registry_production_qualification(profile)

    def test_hdd_os_evidence_is_required(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["resource_profiles"]["hdd"]["storage_evidence"] = [
            "operator-label"
        ]
        with self.assertRaisesRegex(ContractError, "storage evidence"):
            validate_concept_registry_production_qualification(profile)

    def test_reference_target_cannot_change(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["reference_environment"]["target_triple"] = "x86_64-pc-windows-msvc"
        with self.assertRaisesRegex(ContractError, "reference environment"):
            validate_concept_registry_production_qualification(profile)

    def test_toolchain_must_come_from_signed_release_request(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["reference_environment"]["identity_source"] = "operator-input"
        with self.assertRaisesRegex(ContractError, "reference environment"):
            validate_concept_registry_production_qualification(profile)

    def test_toolchain_digest_binding_cannot_be_replaced(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        fields = profile["reference_environment"]["required_pinned_fields"]
        fields[fields.index("rust_toolchain_digest")] = "rust_toolchain_label"
        with self.assertRaisesRegex(ContractError, "reference environment"):
            validate_concept_registry_production_qualification(profile)

    def test_probe_must_be_byte_identical_across_hosts(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["reference_environment"]["cross_host_equality"].remove(
            "probe_blake3"
        )
        with self.assertRaisesRegex(ContractError, "reference environment"):
            validate_concept_registry_production_qualification(profile)

    def test_fixture_evidence_cannot_claim_production(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["evidence_classes"]["fixture"]["production_eligible"] = True
        with self.assertRaisesRegex(ContractError, "evidence classification"):
            validate_concept_registry_production_qualification(profile)

    def test_release_stamp_cannot_be_in_the_root_it_attests(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["release_package"]["aggregate_root"][
            "includes_verification_stamp"
        ] = True
        with self.assertRaisesRegex(ContractError, "aggregate root"):
            validate_concept_registry_production_qualification(profile)

    def test_stamp_signature_empty_member_is_part_of_signed_bytes(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["release_package"]["verification_stamp"]["signed_fields"].remove(
            "signature"
        )
        with self.assertRaisesRegex(ContractError, "verification stamp"):
            validate_concept_registry_production_qualification(profile)

    def test_mismatched_roots_cannot_be_tolerated(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["release_package"]["root_match_policy"] = "warn-only"
        with self.assertRaisesRegex(ContractError, "aggregate root"):
            validate_concept_registry_production_qualification(profile)

    def test_release_request_digest_cannot_be_missing(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["qualification_run_context"]["release"]["required_fields"].remove(
            "release_request_digest"
        )
        with self.assertRaisesRegex(ContractError, "release context"):
            validate_concept_registry_production_qualification(profile)

    def test_qualification_session_cannot_be_missing(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["qualification_run_context"]["release"]["required_fields"].remove(
            "qualification_session_id"
        )
        with self.assertRaisesRegex(ContractError, "release context"):
            validate_concept_registry_production_qualification(profile)

    def test_release_session_mixing_must_fail_closed(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["qualification_run_context"]["release"][
            "mixed_context_policy"
        ] = "warn"
        with self.assertRaisesRegex(ContractError, "release context"):
            validate_concept_registry_production_qualification(profile)

    def test_valid_but_unlisted_signer_is_rejected(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["trust_policy"]["valid_unlisted_signature"] = "accept"
        with self.assertRaisesRegex(ContractError, "signer trust"):
            validate_concept_registry_production_qualification(profile)

    def test_receipt_signature_domain_cannot_change(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["qualification_receipt_envelope"]["signature_domain_hex"] = (
            "00" * 32
        )
        with self.assertRaisesRegex(ContractError, "receipt envelope"):
            validate_concept_registry_production_qualification(profile)

    def test_receipt_usage_cannot_escape_the_allowlist(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["qualification_receipt_envelope"]["usage"] = "registry-anything"
        with self.assertRaisesRegex(ContractError, "receipt envelope"):
            validate_concept_registry_production_qualification(profile)

    def test_prequalification_receipt_cannot_be_candidate_bound(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        bindings = profile["qualification_receipt_envelope"][
            "payload_binding_sets"
        ]
        bindings["prequalification"]["base_candidate_bound"] = True
        with self.assertRaisesRegex(ContractError, "receipt envelope"):
            validate_concept_registry_production_qualification(profile)

    def test_trust_policy_digest_cannot_change(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["trust_policy"]["digest_hex"] = "00" * 32
        with self.assertRaisesRegex(ContractError, "signer trust"):
            validate_concept_registry_production_qualification(profile)

    def test_approved_fingerprint_and_policy_digest_recompute(self) -> None:
        trust = frozen_profile()["trust_policy"]
        signer = trust["policy"]["signers"][0]
        public_key = bytes.fromhex(signer["public_key_hex"])
        fingerprint = blake3.blake3(
            public_key,
            derive_key_context=signer["fingerprint_context"],
        ).hexdigest()
        self.assertEqual(fingerprint, signer["fingerprint_hex"])

        canonical_policy = json.dumps(
            trust["policy"],
            sort_keys=True,
            separators=(",", ":"),
            ensure_ascii=False,
        ).encode("utf-8")
        digest = blake3.blake3(
            canonical_policy,
            derive_key_context=trust["digest_context"],
        ).hexdigest()
        self.assertEqual(digest, trust["digest_hex"])

    def test_kill_gate_cannot_be_removed(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["failure_gates"].remove("update-interruption-process-kill")
        with self.assertRaisesRegex(ContractError, "failure gate"):
            validate_concept_registry_production_qualification(profile)

    def test_live_reader_gate_cannot_be_removed(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["failure_gates"].remove("live-reader-generation-swap")
        with self.assertRaisesRegex(ContractError, "failure gate"):
            validate_concept_registry_production_qualification(profile)

    def test_quarterly_update_is_not_a_signed_release_cycle(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["signed_release_cycle"]["accepted_harnesses"].append(
            "quarterly_update.py"
        )
        with self.assertRaisesRegex(ContractError, "release cycle"):
            validate_concept_registry_production_qualification(profile)


if __name__ == "__main__":
    unittest.main()
