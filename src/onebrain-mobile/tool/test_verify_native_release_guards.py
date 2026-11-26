#!/usr/bin/env python3
"""Unit tests for Slice-C `verify_native_release_guards.py` (`MOB-09`)."""

from __future__ import annotations

import shutil
import tempfile
import unittest
from pathlib import Path

import verify_native_release_guards as guards

REAL_MOBILE_ROOT = Path(__file__).resolve().parents[1]


def _seed_minimal_clean_mobile_tree(root: Path) -> None:
    (root / "android" / "app" / "src" / "main" / "res" / "xml").mkdir(parents=True)
    (root / "ios" / "Runner.xcodeproj").mkdir(parents=True)
    (root / "ios" / "Runner").mkdir(parents=True)

    (root / "android" / "settings.gradle.kts").write_text('rootProject.name = "android"\n', encoding="utf-8")
    (root / "android" / "build.gradle.kts").write_text("// root gradle\n", encoding="utf-8")
    (root / "android" / "app" / "build.gradle.kts").write_text("plugins { id(\"com.android.application\") }\n", encoding="utf-8")
    shutil.copy2(
        REAL_MOBILE_ROOT / "android" / "app" / "src" / "main" / "AndroidManifest.xml",
        root / "android" / "app" / "src" / "main" / "AndroidManifest.xml",
    )
    shutil.copy2(
        REAL_MOBILE_ROOT / "android" / "app" / "src" / "main" / "res" / "xml" / "data_extraction_rules.xml",
        root / "android" / "app" / "src" / "main" / "res" / "xml" / "data_extraction_rules.xml",
    )
    shutil.copy2(
        REAL_MOBILE_ROOT / "android" / "app" / "src" / "main" / "res" / "xml" / "backup_rules.xml",
        root / "android" / "app" / "src" / "main" / "res" / "xml" / "backup_rules.xml",
    )
    shutil.copy2(
        REAL_MOBILE_ROOT / "ios" / "Runner" / "Info.plist",
        root / "ios" / "Runner" / "Info.plist",
    )
    shutil.copy2(
        REAL_MOBILE_ROOT / "ios" / "Runner.xcodeproj" / "project.pbxproj",
        root / "ios" / "Runner.xcodeproj" / "project.pbxproj",
    )
    shutil.copy2(
        REAL_MOBILE_ROOT / "ios" / "Runner" / "AppDelegate.swift",
        root / "ios" / "Runner" / "AppDelegate.swift",
    )


class VerifyNativeReleaseGuardsTests(unittest.TestCase):
    def test_real_mobile_workspace_passes(self) -> None:
        self.assertEqual(guards.scan_mobile_root(REAL_MOBILE_ROOT), [])

    def test_rejects_android_play_asset_delivery(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            _seed_minimal_clean_mobile_tree(root)
            gradle = root / "android" / "app" / "build.gradle.kts"
            gradle.write_text(
                gradle.read_text(encoding="utf-8")
                + '\ndependencies { implementation("com.google.android.play:asset-delivery:2.2.2") }\n'
                + "\nandroid { assetPacks += listOf(\":obr_pack\") }\n",
                encoding="utf-8",
            )
            errors = guards.scan_mobile_root(root)
            self.assertTrue(any("android_play_asset_delivery_dependency" in err for err in errors))
            self.assertTrue(any("android_asset_packs_dsl" in err for err in errors))

    def test_rejects_ios_managed_background_assets(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            _seed_minimal_clean_mobile_tree(root)
            plist = root / "ios" / "Runner" / "Info.plist"
            plist.write_text(
                plist.read_text(encoding="utf-8").replace(
                    "</dict>",
                    "  <key>BAManifestURL</key><string>https://example.invalid</string>\n</dict>",
                ),
                encoding="utf-8",
            )
            errors = guards.scan_mobile_root(root)
            self.assertTrue(any("ios_ba_manifest_url" in err for err in errors))

    def test_rejects_backup_inclusions_and_missing_ios_backup_exclusion(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            _seed_minimal_clean_mobile_tree(root)
            extraction = (
                root / "android" / "app" / "src" / "main" / "res" / "xml" / "data_extraction_rules.xml"
            )
            extraction.write_text(
                extraction.read_text(encoding="utf-8").replace(
                    "<cloud-backup>",
                    '<cloud-backup>\n        <include domain="database" path="."/>',
                ),
                encoding="utf-8",
            )
            delegate = root / "ios" / "Runner" / "AppDelegate.swift"
            delegate.write_text(
                delegate.read_text(encoding="utf-8")
                .replace("values.isExcludedFromBackup = true", "values.isExcludedFromBackup = false")
                .replace(
                    "kSecAttrAccessibleWhenUnlockedThisDeviceOnly",
                    "kSecAttrAccessibleWhenUnlocked",
                ),
                encoding="utf-8",
            )
            errors = guards.scan_mobile_root(root)
            self.assertTrue(any("forbidden_include_in_cloud_backup" in err for err in errors))
            self.assertTrue(any("ios_missing_is_excluded_from_backup" in err for err in errors))
            self.assertTrue(any("ios_keychain_non_device_only_accessible" in err for err in errors))


if __name__ == "__main__":
    unittest.main()
