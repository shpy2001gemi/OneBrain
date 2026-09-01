from __future__ import annotations

import unittest

from scripts.ci.validate_vnext_contracts import (
    CONCEPT_REGISTRY_PRODUCTION_WORKFLOW,
    CONCEPT_REGISTRY_RUNNER_GUIDE,
    CONCEPT_REGISTRY_RUNNER_SCRIPT,
    ContractError,
    VNEXT_FOUNDATION_WORKFLOW,
    validate_concept_registry_runner_kit,
)


def fixture(path) -> str:
    return path.read_text(encoding="utf-8")


class ConceptRegistryRunnerKitTests(unittest.TestCase):
    def setUp(self) -> None:
        self.runner = fixture(CONCEPT_REGISTRY_RUNNER_SCRIPT)
        self.guide = fixture(CONCEPT_REGISTRY_RUNNER_GUIDE)
        self.production_workflow = fixture(CONCEPT_REGISTRY_PRODUCTION_WORKFLOW)
        self.foundation_workflow = fixture(VNEXT_FOUNDATION_WORKFLOW)

    def validate(
        self,
        *,
        runner: str | None = None,
        guide: str | None = None,
        production_workflow: str | None = None,
        foundation_workflow: str | None = None,
    ) -> tuple[int, int, int, int]:
        return validate_concept_registry_runner_kit(
            self.runner if runner is None else runner,
            self.guide if guide is None else guide,
            self.production_workflow
            if production_workflow is None
            else production_workflow,
            self.foundation_workflow
            if foundation_workflow is None
            else foundation_workflow,
        )

    def test_frozen_runner_kit_is_accepted(self) -> None:
        self.assertEqual(self.validate(), (35, 20, 12, 4))

    def test_production_workflow_is_manual_only(self) -> None:
        mutated = self.production_workflow.replace(
            "  workflow_dispatch:", "  pull_request:\n  workflow_dispatch:"
        )
        with self.assertRaisesRegex(ContractError, "manual-only"):
            self.validate(production_workflow=mutated)

    def test_runner_labels_are_code_owned_and_immutable(self) -> None:
        mutated = self.production_workflow.replace(
            "onebrain-registry-image-v1", "${{ inputs.runner_label }}"
        )
        with self.assertRaisesRegex(ContractError, "immutable runner labels"):
            self.validate(production_workflow=mutated)

    def test_release_identity_cannot_be_dispatch_input(self) -> None:
        mutated = self.production_workflow.replace(
            "      qualification_mode:",
            "      release_request_digest:\n"
            "        description: forbidden override\n"
            "        required: true\n"
            "        type: string\n"
            "      qualification_mode:",
        )
        with self.assertRaisesRegex(ContractError, "release identity override"):
            self.validate(production_workflow=mutated)

    def test_release_dispatch_must_verify_signed_request(self) -> None:
        mutated = self.runner.replace(
            "verify_base_release_request.py", "skip_release_request_verification.py"
        )
        with self.assertRaisesRegex(ContractError, "signed release request"):
            self.validate(runner=mutated)

    def test_prequalification_and_release_modes_are_not_merged(self) -> None:
        mutated = self.runner.replace(
            '[[ "$QUALIFICATION_MODE" == "prequalification" || "$QUALIFICATION_MODE" == "release" ]]',
            '[[ -n "$QUALIFICATION_MODE" ]]',
        )
        with self.assertRaisesRegex(ContractError, "closed qualification mode"):
            self.validate(runner=mutated)

    def test_closure_must_include_previous_candidate_and_environment(self) -> None:
        mutated = self.runner.replace(
            '"candidate/release.stamp.json"', '"candidate/release.stamp.skipped"'
        )
        with self.assertRaisesRegex(ContractError, "closure input"):
            self.validate(runner=mutated)

    def test_closure_digest_cannot_be_overridden(self) -> None:
        mutated = self.runner.replace(
            'readonly REGISTRY_CLOSURE_DIGEST_FILE=',
            'REGISTRY_CLOSURE_DIGEST="${ONEBRAIN_REGISTRY_CLOSURE_DIGEST:-}"\nreadonly REGISTRY_CLOSURE_DIGEST_FILE=',
        )
        with self.assertRaisesRegex(ContractError, "closure override"):
            self.validate(runner=mutated)

    def test_fixture_fallback_is_forbidden(self) -> None:
        mutated = self.runner.replace(
            "fixture fallback is forbidden", "fixture fallback is permitted"
        )
        with self.assertRaisesRegex(ContractError, "fixture fallback"):
            self.validate(runner=mutated)

    def test_raw_receipts_must_be_retained(self) -> None:
        mutated = self.production_workflow.replace(
            "retention-days: 90", "retention-days: 1"
        )
        with self.assertRaisesRegex(ContractError, "raw report retention"):
            self.validate(production_workflow=mutated)

    def test_private_key_path_stays_outside_repository(self) -> None:
        mutated = self.runner.replace(
            'ONEBRAIN_REGISTRY_PRIVATE_KEY_FILE:?external',
            'ONEBRAIN_REGISTRY_PRIVATE_KEY_FILE:-target/private-key.hex',
        )
        with self.assertRaisesRegex(ContractError, "external signing key"):
            self.validate(runner=mutated)

    def test_prequalification_cannot_claim_candidate_binding(self) -> None:
        mutated = self.runner.replace(
            '"base_candidate_bound": False', '"base_candidate_bound": True'
        )
        with self.assertRaisesRegex(ContractError, "non-production summary"):
            self.validate(runner=mutated)

    def test_prequalification_summary_verifies_receipt_signatures(self) -> None:
        mutated = self.runner.replace("_verify_receipt", "trust_receipt_payload")
        with self.assertRaisesRegex(ContractError, "receipt signature"):
            self.validate(runner=mutated)

    def test_staged_release_signature_verification_cannot_disappear(self) -> None:
        mutated = self.runner.replace(
            "STAMP_SIGNATURE_DOMAIN", "UNVERIFIED_STAMP_DOMAIN"
        )
        with self.assertRaisesRegex(ContractError, "staged release signature"):
            self.validate(runner=mutated)

    def test_release_cycle_wrapper_is_the_fixed_candidate_binary(self) -> None:
        mutated = self.runner.replace(
            'readonly CANDIDATE_RELEASE_WRAPPER_TOOL="${REPOSITORY_ROOT}/scripts/release/create_verified_base_release.py"',
            'readonly CANDIDATE_RELEASE_WRAPPER_TOOL="${REPOSITORY_ROOT}/arbitrary"',
        )
        with self.assertRaisesRegex(ContractError, "release-cycle wrapper"):
            self.validate(runner=mutated)

    def test_foundation_lane_remains_fixture_only(self) -> None:
        mutated = self.foundation_workflow.replace(
            "ONEBRAIN_REGISTRY_EVIDENCE_TIER: fixture",
            "ONEBRAIN_REGISTRY_EVIDENCE_TIER: production-reference",
        )
        with self.assertRaisesRegex(ContractError, "fixture-only"):
            self.validate(foundation_workflow=mutated)

    def test_guide_forbids_committing_measured_evidence(self) -> None:
        mutated = self.guide.replace(
            "Never commit measured reports", "Commit measured reports"
        )
        with self.assertRaisesRegex(ContractError, "operations guide"):
            self.validate(guide=mutated)


if __name__ == "__main__":
    unittest.main()
