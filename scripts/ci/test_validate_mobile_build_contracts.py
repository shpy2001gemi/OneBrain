from __future__ import annotations

import copy
import unittest

from scripts.ci.validate_mobile_build_contracts import (
    MANIFEST,
    ROOT,
    MobileContractError,
    dart_source_violations,
    expand_screen_spec,
    read_json,
    validate_authorities,
    validate_contract,
    validate_evidence,
)


class MobileBuildContractTests(unittest.TestCase):
    def manifest(self) -> dict[str, object]:
        return read_json(MANIFEST)

    def test_repository_contract_is_valid(self) -> None:
        summary = validate_contract()
        self.assertEqual(summary["structure"]["features"], 123)
        self.assertEqual(summary["structure"]["screens"], 112)
        self.assertEqual(summary["structure"]["components"], 62)
        self.assertEqual(summary["structure"]["patterns"], 13)

    def test_authority_hash_drift_is_rejected(self) -> None:
        manifest = copy.deepcopy(self.manifest())
        manifest["authorities"][0]["sha256"] = "0" * 64
        with self.assertRaisesRegex(MobileContractError, "authority hash drift"):
            validate_authorities(manifest)

    def test_evidence_must_acknowledge_authority_set(self) -> None:
        with self.assertRaisesRegex(
            MobileContractError, "has not acknowledged"
        ):
            validate_evidence(self.manifest(), "0" * 64)

    def test_screen_ranges_expand_deterministically(self) -> None:
        self.assertEqual(
            expand_screen_spec("CAP-006..008"),
            ["CAP-006", "CAP-007", "CAP-008"],
        )
        with self.assertRaisesRegex(MobileContractError, "descending"):
            expand_screen_spec("CAP-008..006")

    def test_direct_dart_database_and_transport_are_rejected(self) -> None:
        guards = self.manifest()["source_guards"]
        path = ROOT / "src/onebrain-mobile/lib/feature/example.dart"
        source = "\n".join(
            (
                "import 'dart:io';",
                "import 'package:sqflite/sqflite.dart';",
                "import 'package:http/http.dart';",
            )
        )
        violations = dart_source_violations(path, source, guards)
        self.assertTrue(any("NO_DART_IO" in row for row in violations))
        self.assertTrue(any("NO_DART_PRODUCT_DATABASE" in row for row in violations))
        self.assertTrue(any("NO_DART_TRANSPORT" in row for row in violations))

    def test_raw_color_is_allowed_only_in_generated_token_projection(self) -> None:
        guards = self.manifest()["source_guards"]
        source = "final color = Color(0xFF007F73);"
        feature_path = ROOT / "src/onebrain-mobile/lib/feature/example.dart"
        generated_path = (
            ROOT
            / "src/onebrain-mobile/lib/design/generated/mobile_design_tokens.g.dart"
        )
        self.assertTrue(
            any(
                "NO_RAW_FLUTTER_COLOR" in row
                for row in dart_source_violations(feature_path, source, guards)
            )
        )
        self.assertFalse(dart_source_violations(generated_path, source, guards))


if __name__ == "__main__":
    unittest.main()
