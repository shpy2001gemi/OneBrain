from __future__ import annotations

import copy
import hashlib
import json
import unittest

from scripts.ci.validate_vnext_contracts import (
    BASE_V1_RUNTIME_INTERFACE_HISTORY,
    BASE_V1_RUNTIME_INTERFACE_PROFILE,
    ContractError,
    validate_base_v1_runtime_baseline_receipt,
    validate_base_v1_runtime_interface,
)


def frozen_profile() -> dict[str, object]:
    return json.loads(BASE_V1_RUNTIME_INTERFACE_PROFILE.read_text(encoding="utf-8"))


def frozen_history() -> dict[str, object]:
    return json.loads(BASE_V1_RUNTIME_INTERFACE_HISTORY.read_text(encoding="utf-8"))


def baseline_receipt() -> tuple[dict[str, object], bytes, bytes]:
    idl_bytes = BASE_V1_RUNTIME_INTERFACE_PROFILE.read_bytes()
    history_bytes = BASE_V1_RUNTIME_INTERFACE_HISTORY.read_bytes()
    history = json.loads(history_bytes)
    receipt = {
        "format": "onebrain/base-v1-idl-baseline-receipt/1",
        "ref": "refs/heads/base-v1-idl-baseline",
        "commit_sha1": "a" * 40,
        "tree_sha1": "b" * 40,
        "idl_sha256": hashlib.sha256(idl_bytes).hexdigest(),
        "history_chain_root_sha256": history["history_chain"]["root_sha256"],
    }
    return receipt, idl_bytes, history_bytes


class BaseV1RuntimeInterfaceTests(unittest.TestCase):
    def assert_rejected(
        self,
        profile: dict[str, object],
        pattern: str,
        history: dict[str, object] | None = None,
    ) -> None:
        with self.assertRaisesRegex(ContractError, pattern):
            validate_base_v1_runtime_interface(
                profile,
                frozen_history() if history is None else history,
            )

    def test_frozen_runtime_interface_is_accepted(self) -> None:
        self.assertEqual(
            validate_base_v1_runtime_interface(frozen_profile(), frozen_history()),
            (27, 5, 13),
        )

    def test_generator_ready_core_types_are_frozen(self) -> None:
        profile = frozen_profile()
        profile["type_definitions"]["BaseOpaqueContinuation"]["max_bytes"] = 4097
        self.assert_rejected(profile, "private continuation")

    def test_every_required_operation_is_mandatory(self) -> None:
        profile = frozen_profile()
        profile["operations"] = profile["operations"][:-1]
        self.assert_rejected(profile, "operation inventory")

    def test_archive_commands_are_not_optional(self) -> None:
        profile = frozen_profile()
        profile["command_kinds"] = [
            row
            for row in profile["command_kinds"]
            if row["name"] != "CreateArchive"
        ]
        self.assert_rejected(profile, "archive command")

    def test_reserve_must_precede_capability_registration(self) -> None:
        profile = frozen_profile()
        profile["operation_protocol"]["durable_order"][0:2] = [
            "management_capability_registration",
            "reserve_operation",
        ]
        self.assert_rejected(profile, "reserve-before-capability")

    def test_payloads_are_bounded(self) -> None:
        profile = frozen_profile()
        del profile["limits"]["max_payload_bytes"]
        self.assert_rejected(profile, "payload bound")

    def test_continuations_are_bounded_and_opaque(self) -> None:
        profile = frozen_profile()
        profile["limits"]["max_continuation_bytes"] = 0
        self.assert_rejected(profile, "continuation bound")

    def test_archive_chunks_are_bounded(self) -> None:
        profile = frozen_profile()
        profile["archive_capabilities"]["max_chunk_bytes"] = None
        self.assert_rejected(profile, "archive chunk")

    def test_idempotency_is_mandatory(self) -> None:
        profile = frozen_profile()
        profile["operation_protocol"]["confirm_requires_idempotency_key"] = False
        self.assert_rejected(profile, "idempotency")

    def test_process_and_dataset_generations_are_fenced(self) -> None:
        profile = frozen_profile()
        profile["generation_fence"]["required_fields"].remove("dataset_generation")
        self.assert_rejected(profile, "generation fence")

    def test_management_grant_binds_principal_and_scope(self) -> None:
        for binding in ("principal_id", "exact_scopes"):
            with self.subTest(binding=binding):
                profile = frozen_profile()
                profile["management"]["grant_bindings"].remove(binding)
                self.assert_rejected(profile, "management grant")

    def test_capabilities_have_unambiguous_operation_ownership(self) -> None:
        profile = frozen_profile()
        profile["archive_capabilities"]["ownership_binding"].remove("operation_id")
        self.assert_rejected(profile, "capability ownership")

    def test_retry_requires_reconciliation(self) -> None:
        profile = frozen_profile()
        profile["operation_protocol"]["retry_requires_reconcile"] = False
        self.assert_rejected(profile, "retry.*reconcile")

    def test_raw_paths_are_forbidden(self) -> None:
        profile = frozen_profile()
        profile["scalar_types"][0]["wire"] = "raw_path"
        self.assert_rejected(profile, "forbidden exposure")

    def test_private_keys_are_forbidden(self) -> None:
        profile = frozen_profile()
        profile["scalar_types"][0]["wire"] = "private_key"
        self.assert_rejected(profile, "forbidden exposure")

    def test_borrowed_readers_are_forbidden(self) -> None:
        for forbidden in (
            "raw_path",
            "runtime_handle",
            "store_handle",
            "private_key",
            "authority_implementation",
            "borrowed_reader",
            "borrowed_writer",
            "unbounded_string",
        ):
            with self.subTest(forbidden=forbidden):
                profile = frozen_profile()
                profile["scalar_types"][0]["wire"] = forbidden
                self.assert_rejected(profile, "forbidden exposure")

    def test_handwritten_projection_is_forbidden(self) -> None:
        profile = frozen_profile()
        profile["projection_rules"]["source"] = "handwritten"
        self.assert_rejected(profile, "projection source")

    def test_c_abi_requires_struct_size(self) -> None:
        profile = frozen_profile()
        c_abi = next(
            row
            for row in profile["projection_rules"]["targets"]
            if row["name"] == "c_abi"
        )
        c_abi["struct_size_required"] = False
        self.assert_rejected(profile, "struct_size")

    def test_subscription_handle_is_owned_and_closeable(self) -> None:
        profile = frozen_profile()
        profile["subscriptions"]["handle_ownership"] = "borrowed"
        self.assert_rejected(profile, "subscription ownership")

    def test_subscription_operations_are_mandatory(self) -> None:
        for operation in ("subscribe", "poll_events", "close_subscription"):
            with self.subTest(operation=operation):
                profile = frozen_profile()
                profile["subscriptions"]["required_operations"].remove(operation)
                self.assert_rejected(profile, "subscription ownership")

    def test_poll_batch_is_bounded(self) -> None:
        profile = frozen_profile()
        del profile["subscriptions"]["max_batch_items"]
        self.assert_rejected(profile, "subscription batch")

    def test_subscription_cursor_cannot_regress(self) -> None:
        profile = frozen_profile()
        profile["subscriptions"]["cursor_rule"] = "may_regress"
        self.assert_rejected(profile, "cursor")

    def test_retention_gap_requires_explicit_resync(self) -> None:
        profile = frozen_profile()
        profile["subscriptions"]["gap_response"] = "silently_skip"
        self.assert_rejected(profile, "gap.*resync")

    def test_slow_consumer_behavior_is_fail_closed(self) -> None:
        profile = frozen_profile()
        profile["subscriptions"]["slow_consumer"] = "unbounded_buffer"
        self.assert_rejected(profile, "backpressure")

    def test_archive_lifecycle_contains_every_terminal_action(self) -> None:
        for operation in (
            "management.archive_source_begin",
            "management.archive_source_seal",
            "management.archive_sink_commit",
            "management.archive_capability_abort",
            "management.archive_capability_destroy",
        ):
            with self.subTest(operation=operation):
                profile = frozen_profile()
                profile["archive_capabilities"]["lifecycle_operations"].remove(
                    operation
                )
                self.assert_rejected(profile, "archive lifecycle")

    def test_signer_reprovision_lifecycle_is_present(self) -> None:
        profile = frozen_profile()
        profile["management"]["required_operations"].remove(
            "management.complete_signer_reprovision"
        )
        self.assert_rejected(profile, "reprovision lifecycle")

    def test_close_and_drain_authority_is_frozen(self) -> None:
        profile = frozen_profile()
        profile["runtime_lifecycle"]["drain_blocks_new_operations"] = False
        self.assert_rejected(profile, "drain")

    def test_request_response_and_error_discriminators_are_closed(self) -> None:
        profile = frozen_profile()
        profile["errors"][0]["id"] = profile["errors"][1]["id"]
        self.assert_rejected(profile, "closed discriminator")

    def test_history_must_cover_every_live_discriminator(self) -> None:
        history = frozen_history()
        history["entries"].pop()
        self.assert_rejected(frozen_profile(), "history coverage", history)

    def test_history_ids_cannot_be_reused(self) -> None:
        history = frozen_history()
        history["entries"][-1]["id"] = history["entries"][-2]["id"]
        self.assert_rejected(frozen_profile(), "history.*reuse", history)

    def test_history_chain_root_is_verified(self) -> None:
        history = frozen_history()
        history["history_chain"]["root_sha256"] = "00" * 32
        self.assert_rejected(frozen_profile(), "history chain", history)

    def test_baseline_history_is_an_immutable_prefix(self) -> None:
        current_history = frozen_history()
        baseline_history = copy.deepcopy(current_history)
        baseline_history["entries"][0]["name"] = "RewrittenRequestV1"
        with self.assertRaisesRegex(ContractError, "baseline history"):
            validate_base_v1_runtime_interface(
                frozen_profile(),
                current_history,
                baseline_profile=frozen_profile(),
                baseline_history=baseline_history,
            )

    def test_same_major_cannot_retype_a_discriminator(self) -> None:
        current = frozen_profile()
        baseline = frozen_profile()
        current["requests"][0]["name"] = "RetypedOpenRequestV1"
        with self.assertRaisesRegex(ContractError, "breaking-major"):
            validate_base_v1_runtime_interface(
                current,
                frozen_history(),
                baseline_profile=baseline,
                baseline_history=frozen_history(),
            )

    def test_same_major_cannot_widen_a_bound(self) -> None:
        current = frozen_profile()
        baseline = frozen_profile()
        current["limits"]["max_payload_bytes"] += 1
        with self.assertRaisesRegex(ContractError, "bound widening"):
            validate_base_v1_runtime_interface(
                current,
                frozen_history(),
                baseline_profile=baseline,
                baseline_history=frozen_history(),
            )

    def test_same_major_cannot_change_optional_to_required(self) -> None:
        current = frozen_profile()
        baseline = frozen_profile()
        current["type_definitions"]["BaseQueryRequestV1"]["fields"][1][
            "required"
        ] = True
        with self.assertRaisesRegex(ContractError, "optionality"):
            validate_base_v1_runtime_interface(
                current,
                frozen_history(),
                baseline_profile=baseline,
                baseline_history=frozen_history(),
            )

    def test_same_major_cannot_change_field_ownership(self) -> None:
        current = frozen_profile()
        baseline = frozen_profile()
        current["type_definitions"]["BasePrepareRequestV1"]["fields"][0][
            "ownership"
        ] = "value"
        with self.assertRaisesRegex(ContractError, "ownership"):
            validate_base_v1_runtime_interface(
                current,
                frozen_history(),
                baseline_profile=baseline,
                baseline_history=frozen_history(),
            )

    def test_valid_baseline_receipt_binds_ref_tree_and_payloads(self) -> None:
        receipt, idl_bytes, history_bytes = baseline_receipt()
        profile, history = validate_base_v1_runtime_baseline_receipt(
            receipt,
            resolved_commit="a" * 40,
            resolved_tree="b" * 40,
            baseline_idl_bytes=idl_bytes,
            baseline_history_bytes=history_bytes,
            candidate_is_descendant=True,
        )
        self.assertEqual(profile["profile_id"], "BASE_V1_RUNTIME_INTERFACE_V1")
        self.assertEqual(history["history_version"], 1)

    def test_moved_baseline_ref_is_rejected(self) -> None:
        receipt, idl_bytes, history_bytes = baseline_receipt()
        with self.assertRaisesRegex(ContractError, "ref moved"):
            validate_base_v1_runtime_baseline_receipt(
                receipt,
                resolved_commit="c" * 40,
                resolved_tree="b" * 40,
                baseline_idl_bytes=idl_bytes,
                baseline_history_bytes=history_bytes,
                candidate_is_descendant=True,
            )

    def test_non_ancestor_baseline_is_rejected(self) -> None:
        receipt, idl_bytes, history_bytes = baseline_receipt()
        with self.assertRaisesRegex(ContractError, "not a candidate ancestor"):
            validate_base_v1_runtime_baseline_receipt(
                receipt,
                resolved_commit="a" * 40,
                resolved_tree="b" * 40,
                baseline_idl_bytes=idl_bytes,
                baseline_history_bytes=history_bytes,
                candidate_is_descendant=False,
            )

    def test_digest_mismatched_baseline_is_rejected(self) -> None:
        receipt, idl_bytes, history_bytes = baseline_receipt()
        with self.assertRaisesRegex(ContractError, "IDL digest"):
            validate_base_v1_runtime_baseline_receipt(
                receipt,
                resolved_commit="a" * 40,
                resolved_tree="b" * 40,
                baseline_idl_bytes=idl_bytes + b"tamper",
                baseline_history_bytes=history_bytes,
                candidate_is_descendant=True,
            )


if __name__ == "__main__":
    unittest.main()
