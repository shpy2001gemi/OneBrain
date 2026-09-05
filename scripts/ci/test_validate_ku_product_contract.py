"""Adversarial contract checks. These do not simulate production KU writes."""
from __future__ import annotations

import copy
import unittest

from scripts.ci.validate_ku_product_contract import (
    KuContractError,
    load_profile,
    validate_contract,
    validate_value,
)


class KuProductContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.profile = load_profile()

    def mutate(self, path: tuple, value: object, reason: str) -> None:
        p = copy.deepcopy(self.profile)
        cursor = p
        for key in path[:-1]:
            cursor = cursor[key]
        cursor[path[-1]] = value
        with self.assertRaisesRegex(KuContractError, reason):
            validate_contract(p)

    def test_candidate_and_dto_fixtures_pass(self) -> None:
        self.assertEqual(validate_contract(self.profile), (11, 18, 11))

    def test_no_silent_freeze_or_enable(self) -> None:
        self.mutate(("implementation_enabled",), True, "silently enable")
        self.mutate(("status",), "frozen", "silently enable")

    def test_no_unapproved_numeric_wire_or_domain_id(self) -> None:
        self.mutate(("approval", "base_local_command_ids"), {"save": 99}, "unapproved")
        self.mutate(("approval", "domain_registry_allocated"), True, "unapproved")

    def test_new_routes_and_ws_events_require_review(self) -> None:
        self.mutate(("approval", "rest_endpoints"), ["/api/encode"], "unapproved")
        self.mutate(("approval", "ws_events"), ["ku_saved"], "unapproved")

    def test_semantic_identity_is_not_artifact_alias(self) -> None:
        self.mutate(("identity", "semantic_cid_is_artifact_alias"), True, "artifact identity")
        self.mutate(("identity", "artifact_domain"), "semantic-content/1", "artifact identity")

    def test_hash_preimage_and_domain_are_exact(self) -> None:
        self.mutate(("identity", "preimage"), "JSON(draft)", "preimage")
        self.mutate(("identity", "proposed_domain"), "object/1", "profile/domain")

    def test_provenance_and_semantic_qualifiers_do_not_mix(self) -> None:
        for field in ("negation", "condition", "source_unit_ccid", "statement_order"):
            with self.subTest(field=field):
                self.mutate(("identity", "preserve"), [v for v in self.profile["identity"]["preserve"] if v != field], "qualifier loss")
        self.mutate(("identity", "exclude"), ["reward"], "provenance")

    def test_private_fingerprint_is_not_public_commitment(self) -> None:
        self.mutate(("identity", "private_fingerprint_public"), True, "privacy")

    def test_unresolved_concepts_cannot_pick_first(self) -> None:
        self.mutate(("identity", "unresolved_concepts"), "pick_first", "fallback")

    def test_registry_remains_signed_and_pinned_for_replay(self) -> None:
        for field in ("signed_release_required", "pin_for_whole_run", "replay_uses_original_release"):
            with self.subTest(field=field):
                self.mutate(("registry", field), False, "Registry run pinning")
        self.mutate(("registry", "hot_refresh_during_run"), True, "Registry run pinning")

    def test_private_save_cannot_select_public_or_plaintext(self) -> None:
        self.mutate(("privacy", "save_destinations"), ["PUBLIC"], "private save")
        self.mutate(("privacy", "plaintext_fallback"), True, "private save")

    def test_all_side_effect_firewalls_are_fail_closed(self) -> None:
        for field in self.profile["firewalls"]:
            with self.subTest(field=field):
                self.mutate(("firewalls", field), True, "semantic firewall")

    def test_base_lifecycle_cannot_commit_from_prepared(self) -> None:
        transitions = self.profile["operation_protocol"]["transitions"] + ["prepared->committed"]
        self.mutate(("operation_protocol", "transitions"), transitions, "Base operation")

    def test_base_generation_and_budget_envelope_cannot_disappear(self) -> None:
        self.mutate(("base_envelope_fields",), ["operation_id"], "Base envelope")

    def test_atomicity_and_reconciliation_are_required(self) -> None:
        self.mutate(("idempotency", "partial_visibility"), True, "atomicity")
        self.mutate(("idempotency", "unknown_outcome"), "retry", "idempotency")
        self.mutate(("idempotency", "changed_reuse"), "overwrite", "idempotency")

    def test_principal_and_frontier_bound_continuation(self) -> None:
        self.mutate(("continuation", "binds"), ["last_full_cid"], "snapshot continuation")
        self.mutate(("continuation", "evicted_snapshot"), "empty_success", "snapshot continuation")

    def test_revision_cannot_overwrite_or_claim_global_authority(self) -> None:
        self.mutate(("revision", "concurrent_successors"), "latest_wins", "immutable revision")
        self.mutate(("revision", "replicated_authority"), True, "immutable revision")

    def test_base_error_properties_survive_rest_mapping(self) -> None:
        self.mutate(("errors", 11, "reconcile_before_retry"), False, "Base typed error")
        self.mutate(("errors", 11, "rest_code"), "internal_error", "REST error")

    def test_every_surface_uses_the_same_operation_boundary(self) -> None:
        self.mutate(("operations", 2, "surfaces"), ["web"], "surface drift")
        self.mutate(("operations", 2, "effect"), "publish", "side-effect boundary")
        self.mutate(("operations", 2, "base_boundary"), "query", "side-effect boundary")

    def test_operation_and_dto_inventory_cannot_drop_or_dangle(self) -> None:
        self.mutate(("operations",), self.profile["operations"][:-1], "operation inventory")
        self.mutate(("operations", 0, "request"), "Anything", "DTO reference")
        self.mutate(("operations", 2, "response"), "KuPageV1", "DTO binding")
        self.mutate(("dtos", "KuReceiptV1", "required"), {"state": "BaseState"}, "required DTO")

    def test_client_authority_and_arbitrary_fields_rejected(self) -> None:
        fields = self.profile["dtos"]["KuSaveV1"]["optional"] | {"authorized": "Boolean"}
        self.mutate(("dtos", "KuSaveV1", "optional"), fields, "authority/privacy")
        self.mutate(("dtos", "KuSaveV1", "additional_fields"), True, "authority/privacy")

    def test_cid_roles_and_literal_flags_cannot_weaken(self) -> None:
        self.mutate(("dtos", "KuGetV1", "required", "object_cid"), "SemanticContentCID", "typed boundary")
        self.mutate(("types", "ObjectCID", "bytes"), 8, "width/role")
        self.mutate(("dtos", "KuReceiptV1", "required", "published"), "Boolean", "flag weakened")
        self.mutate(("dtos", "KuPrepareV1", "required", "source_refs"), "ObjectIDs", "typed field ownership")

    def test_budgets_cannot_expand(self) -> None:
        self.mutate(("limits", "payload_bytes"), 2**32, "finite product bounds")
        self.mutate(("types", "PageLimit", "max"), 257, "DTO bounds")
        self.mutate(("types", "Continuation", "max_bytes"), 4096, "DTO bounds")

    def test_d012_to_d014_cannot_be_erased(self) -> None:
        self.mutate(("dependencies",), self.profile["dependencies"][:-1], "dependency inventory")
        self.mutate(("dependencies", 1, "required"), False, "dependency inventory")
        self.mutate(("dependencies", 2, "blocks"), ["unrelated_work"], "implementation gates")

    def test_direct_issuance_does_not_require_benefit_or_agreement(self) -> None:
        self.mutate(("dependencies", 4, "requires_later_benefit_event"), True, "D-014")
        self.mutate(("dependencies", 4, "correct_mismatch_eligible"), False, "D-014")
        self.mutate(("dependencies", 4, "trigger"), "bounty", "D-014")
        self.mutate(("dependencies", 4, "separate_reward_authorization"), False, "D-014")

    def test_continuation_canonical_bits_and_bounds(self) -> None:
        validate_value(self.profile, "Continuation", "obc1.AQ")
        for value in ("obc1.AR", "obc1.AQ=", "obc1.A", "obc1." + "A" * 2048):
            with self.subTest(value=value[:15]), self.assertRaises(KuContractError):
                validate_value(self.profile, "Continuation", value)

    def test_nonnumeric_bool_and_nonliteral_zero_rejected(self) -> None:
        for kind, value in (("PageLimit", True), ("False", 0)):
            with self.subTest(kind=kind), self.assertRaises(KuContractError):
                validate_value(self.profile, kind, value)

    def test_ready_preview_has_semantic_identity_but_issues_do_not(self) -> None:
        value = copy.deepcopy(self.profile["fixtures"][0]["value"])
        value["validity"] = "needs_resolution"
        with self.assertRaisesRegex(KuContractError, "unresolved"):
            validate_value(self.profile, "KuPreparedV1", value)

    def test_unknown_outcome_cannot_advertise_blind_retry(self) -> None:
        value = {"code": "UnknownOutcome", "retryable": True, "reconcile_before_retry": False, "limitations": []}
        with self.assertRaisesRegex(KuContractError, "retry/reconcile"):
            validate_value(self.profile, "KuFailureV1", value)

    def test_source_bound_prepare_and_mode_are_checked(self) -> None:
        value = copy.deepcopy(self.profile["fixtures"][3]["value"])
        value["destination"] = "LOCAL_ONLY"
        validate_value(self.profile, "KuPrepareV1", value)
        value["input_mode"] = "resolved_semantic_draft"
        with self.assertRaisesRegex(KuContractError, "mode/draft"):
            validate_value(self.profile, "KuPrepareV1", value)

    def test_archive_cannot_return_plaintext_public_payload(self) -> None:
        value = {"mode": "encrypted_base_archive", "object_cids": ["01" * 32], "limitations": [], "requires_base_management": True, "public_records": "oA=="}
        with self.assertRaisesRegex(KuContractError, "export payload"):
            validate_value(self.profile, "KuExportViewV1", value)

    def test_all_prepared_outputs_have_corresponding_identity_and_preview(self) -> None:
        value = copy.deepcopy(self.profile["fixtures"][0]["value"])
        value["object_cids"].append("02" * 32)
        with self.assertRaisesRegex(KuContractError, "preview/identity"):
            validate_value(self.profile, "KuPreparedV1", value)
        value["artifacts"].append({"object_cid": "02" * 32, "semantic_content_cid": "03" * 32, "canonical_preview": "oA=="})
        validate_value(self.profile, "KuPreparedV1", value)

    def test_aggregate_preview_budget_includes_all_outputs(self) -> None:
        value = copy.deepcopy(self.profile["fixtures"][0]["value"])
        value["object_cids"] = ["01" * 32, "02" * 32]
        value["artifacts"] = [{"object_cid": cid, "semantic_content_cid": cid, "canonical_preview": "AAAA" * 150000} for cid in value["object_cids"]]
        with self.assertRaisesRegex(KuContractError, "aggregate payload"):
            validate_value(self.profile, "KuPreparedV1", value)


if __name__ == "__main__":
    unittest.main()
