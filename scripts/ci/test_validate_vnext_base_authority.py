from __future__ import annotations

import copy
import json
import unittest

from scripts.ci.validate_vnext_contracts import (
    BASE_V1_AUTHORITY_RECOVERY_PROFILE,
    ContractError,
    validate_base_v1_authority_recovery,
)


def frozen_profile() -> dict[str, object]:
    return json.loads(BASE_V1_AUTHORITY_RECOVERY_PROFILE.read_text(encoding="utf-8"))


class BaseV1AuthorityRecoveryTests(unittest.TestCase):
    def test_frozen_profile_is_accepted(self) -> None:
        self.assertEqual(validate_base_v1_authority_recovery(frozen_profile()), (3, 10))

    def assert_top_level_field_is_frozen(self, field: str, replacement: object) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile[field] = replacement
        with self.assertRaisesRegex(ContractError, field):
            validate_base_v1_authority_recovery(profile)

    def test_format_is_frozen(self) -> None:
        self.assert_top_level_field_is_frozen("format", "onebrain/base-v1-authority-recovery/2")

    def test_canonical_write_path_is_frozen(self) -> None:
        self.assert_top_level_field_is_frozen("canonical_write_path", "legacy-ku")

    def test_legacy_boundary_is_frozen(self) -> None:
        self.assert_top_level_field_is_frozen("legacy_boundary", "dual-write")

    def test_recovery_profile_is_frozen(self) -> None:
        self.assert_top_level_field_is_frozen("recovery_profile", "mnemonic-v1")

    def test_archive_profiles_are_frozen(self) -> None:
        self.assert_top_level_field_is_frozen("archive_profiles", ["password-argon2id-v1"])

    def test_registry_required_states_are_frozen(self) -> None:
        self.assert_top_level_field_is_frozen(
            "registry_required_states", ["registry-dependent-encoding"]
        )

    def test_network_default_active_lane_count_is_zero(self) -> None:
        self.assert_top_level_field_is_frozen("network_default_active_lane_count", 1)

    def test_delete_never_rewrites_history(self) -> None:
        self.assert_top_level_field_is_frozen("delete_semantics", "hard-delete")

    def test_password_kdf_parameters_are_exact(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["archive_crypto"]["password_argon2id_v1"]["memory_kib"] = 32768
        with self.assertRaisesRegex(ContractError, "password Argon2id"):
            validate_base_v1_authority_recovery(profile)

    def test_archive_aead_is_xchacha20_poly1305(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["archive_crypto"]["aead"] = "aes-256-gcm"
        with self.assertRaisesRegex(ContractError, "archive crypto"):
            validate_base_v1_authority_recovery(profile)

    def test_archive_scope_is_exact(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["archive_scope"]["included"].remove("owned-original-blobs")
        with self.assertRaisesRegex(ContractError, "archive scope"):
            validate_base_v1_authority_recovery(profile)

    def test_node_actor_and_feed_domains_are_distinct(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["signer_recovery"]["feed_author"]["domain"] = profile[
            "signer_recovery"
        ]["node_transport"]["domain"]
        with self.assertRaisesRegex(ContractError, "signer recovery domains"):
            validate_base_v1_authority_recovery(profile)

    def test_non_exportable_signers_require_reprovisioning(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["signer_recovery"]["actor_root"]["non_exportable_unavailable"] = (
            "restored"
        )
        with self.assertRaisesRegex(ContractError, "non-exportable signer"):
            validate_base_v1_authority_recovery(profile)

    def test_registry_states_fail_closed_without_exact_release(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["registry_policy"]["missing_exact_release"] = "continue-with-cache"
        with self.assertRaisesRegex(ContractError, "Registry policy"):
            validate_base_v1_authority_recovery(profile)


if __name__ == "__main__":
    unittest.main()
