from __future__ import annotations

import copy
import json
import unittest

from scripts.ci.validate_vnext_contracts import (
    BASE_V1_DERIVED_PROJECTION_PROFILE,
    BASE_V1_STORAGE_INTEGRITY_PROFILE,
    ContractError,
    validate_base_v1_storage_integrity,
)


def frozen_storage() -> dict[str, object]:
    return json.loads(BASE_V1_STORAGE_INTEGRITY_PROFILE.read_text(encoding="utf-8"))


def frozen_projection() -> dict[str, object]:
    return json.loads(BASE_V1_DERIVED_PROJECTION_PROFILE.read_text(encoding="utf-8"))


class BaseV1StorageIntegrityTests(unittest.TestCase):
    def test_frozen_profiles_are_accepted(self) -> None:
        self.assertEqual(
            validate_base_v1_storage_integrity(frozen_storage(), frozen_projection()),
            (10, 9),
        )

    def test_short_blob_path_is_rejected(self) -> None:
        profile = copy.deepcopy(frozen_storage())
        profile["blob_layout"]["relative_path"] = "v2/<short-cid>"
        with self.assertRaisesRegex(ContractError, "blob layout"):
            validate_base_v1_storage_integrity(profile, frozen_projection())

    def test_missing_full_read_hash_is_rejected(self) -> None:
        profile = copy.deepcopy(frozen_storage())
        profile["blob_read_integrity"]["required_checks"].remove("full-payload-blake3")
        with self.assertRaisesRegex(ContractError, "blob read integrity"):
            validate_base_v1_storage_integrity(profile, frozen_projection())

    def test_missing_total_quota_is_rejected(self) -> None:
        profile = copy.deepcopy(frozen_storage())
        del profile["capacity_admission"]["total_quota_bytes"]
        with self.assertRaisesRegex(ContractError, "capacity admission"):
            validate_base_v1_storage_integrity(profile, frozen_projection())

    def test_graph_best_effort_requires_dirty_generation(self) -> None:
        profile = copy.deepcopy(frozen_storage())
        profile["derived_store_policy"]["publication_failure_state"] = "serve-stale-clean"
        with self.assertRaisesRegex(ContractError, "derived store policy"):
            validate_base_v1_storage_integrity(profile, frozen_projection())

    def test_vacuous_projection_mapping_is_rejected(self) -> None:
        projection = copy.deepcopy(frozen_projection())
        projection["object_mappings"] = []
        with self.assertRaisesRegex(ContractError, "object mapping"):
            validate_base_v1_storage_integrity(frozen_storage(), projection)

    def test_projection_rows_require_source_and_index_roots(self) -> None:
        projection = copy.deepcopy(frozen_projection())
        projection["row_binding"].remove("index-root")
        with self.assertRaisesRegex(ContractError, "projection row_binding"):
            validate_base_v1_storage_integrity(frozen_storage(), projection)

    def test_unknown_projection_mapping_is_rejected(self) -> None:
        projection = copy.deepcopy(frozen_projection())
        projection["object_mappings"][0]["kind"] = "unknown-kind"
        with self.assertRaisesRegex(ContractError, "object mapping"):
            validate_base_v1_storage_integrity(frozen_storage(), projection)

    def test_legacy_ku_metadata_cannot_authorize_blob_retention(self) -> None:
        profile = copy.deepcopy(frozen_storage())
        profile["owned_blob_reference"]["authority_source"] = "legacy-ku-metadata"
        with self.assertRaisesRegex(ContractError, "blob reference"):
            validate_base_v1_storage_integrity(profile, frozen_projection())

    def test_corrupt_retriever_cannot_make_canonical_startup_fatal(self) -> None:
        profile = copy.deepcopy(frozen_storage())
        profile["derived_store_policy"]["corrupt_reopen"] = "fatal-startup"
        with self.assertRaisesRegex(ContractError, "derived store policy"):
            validate_base_v1_storage_integrity(profile, frozen_projection())

    def test_update_delete_parity_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_storage())
        profile["derived_store_policy"]["parity_operations"].remove("delete")
        with self.assertRaisesRegex(ContractError, "derived store policy"):
            validate_base_v1_storage_integrity(profile, frozen_projection())

    def test_preview_cannot_slice_utf8_bytes(self) -> None:
        profile = copy.deepcopy(frozen_storage())
        profile["text_preview"]["truncation_unit"] = "utf8-byte"
        with self.assertRaisesRegex(ContractError, "text preview"):
            validate_base_v1_storage_integrity(profile, frozen_projection())

    def test_failpoint_vocabulary_cannot_gain_a_sixth_phase(self) -> None:
        profile = copy.deepcopy(frozen_storage())
        profile["failpoint_phases"].append("reopen")
        with self.assertRaisesRegex(ContractError, "failpoint phases"):
            validate_base_v1_storage_integrity(profile, frozen_projection())

    def test_crash_oracle_requires_child_process_kill_and_reopen(self) -> None:
        profile = copy.deepcopy(frozen_storage())
        profile["crash_oracle"]["runner"] = "in-process-simulation"
        with self.assertRaisesRegex(ContractError, "crash oracle"):
            validate_base_v1_storage_integrity(profile, frozen_projection())

    def test_owner_table_cannot_lose_an_owner(self) -> None:
        profile = copy.deepcopy(frozen_storage())
        profile["storage_owner_table"]["owners"].pop()
        with self.assertRaisesRegex(ContractError, "owner table"):
            validate_base_v1_storage_integrity(profile, frozen_projection())

    def test_owner_code_cannot_be_duplicated_or_reused(self) -> None:
        profile = copy.deepcopy(frozen_storage())
        profile["storage_owner_table"]["owners"][1]["code_u16"] = 1
        profile["storage_owner_table"]["owners"][1]["code_hex"] = "0x0001"
        profile["storage_owner_table"]["owners"][1]["base_storage_owner_bytes"] = "0001"
        profile["storage_owner_table"]["owners"][1]["archive_owner_bytes"] = "0001"
        with self.assertRaisesRegex(ContractError, "owner table"):
            validate_base_v1_storage_integrity(profile, frozen_projection())

    def test_reserved_owner_code_is_rejected(self) -> None:
        profile = copy.deepcopy(frozen_storage())
        owner = profile["storage_owner_table"]["owners"][0]
        owner["code_u16"] = 0
        owner["code_hex"] = "0x0000"
        owner["base_storage_owner_bytes"] = "0000"
        owner["archive_owner_bytes"] = "0000"
        with self.assertRaisesRegex(ContractError, "owner table"):
            validate_base_v1_storage_integrity(profile, frozen_projection())

    def test_endian_swapped_owner_code_is_rejected(self) -> None:
        profile = copy.deepcopy(frozen_storage())
        profile["storage_owner_table"]["owners"][0]["archive_owner_bytes"] = "0100"
        with self.assertRaisesRegex(ContractError, "owner table"):
            validate_base_v1_storage_integrity(profile, frozen_projection())

    def test_non_bijective_owner_mapping_is_rejected(self) -> None:
        profile = copy.deepcopy(frozen_storage())
        profile["storage_owner_table"]["owners"][0]["archive_owner"] = "vault"
        with self.assertRaisesRegex(ContractError, "owner table"):
            validate_base_v1_storage_integrity(profile, frozen_projection())


if __name__ == "__main__":
    unittest.main()
