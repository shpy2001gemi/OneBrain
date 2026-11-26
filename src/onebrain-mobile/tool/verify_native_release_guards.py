#!/usr/bin/env python3
"""Slice-C native release guard scanner for OneBrain Mobile (`MOB-09`).

Verifies static Android and iOS project configurations:
1. Rejects Android Play Asset Delivery (PAD) and Instant Apps declarations.
2. Rejects iOS Managed Background Assets declarations and extensions.
3. Verifies Android `allowBackup="false"`, zero `INTERNET` permission in the
   Bootstrap manifest, and complete `data_extraction_rules.xml` + `backup_rules.xml`
   domain exclusions with zero `<include>` rules.
4. Verifies iOS `isExcludedFromBackup = true` and Keychain
   `kSecAttrAccessibleWhenUnlockedThisDeviceOnly` with no iCloud Keychain sync.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

ANDROID_NS = "{http://schemas.android.com/apk/res/android}"

REQUIRED_BACKUP_DOMAINS = {
    "root",
    "file",
    "database",
    "sharedpref",
    "external",
    "device_root",
    "device_file",
    "device_database",
    "device_sharedpref",
}

FORBIDDEN_ANDROID_PAD_PATTERNS: tuple[tuple[str, re.Pattern[str]], ...] = (
    (
        "android_play_asset_delivery_dependency",
        re.compile(r"com\.google\.android\.play:asset-delivery"),
    ),
    (
        "android_asset_packs_dsl",
        re.compile(r"\bassetPacks\b"),
    ),
    (
        "android_asset_pack_plugin",
        re.compile(r"com\.android\.asset-pack(?:-bundle)?"),
    ),
    (
        "android_instant_apps_dependency",
        re.compile(r"play-services-instantapps"),
    ),
    (
        "android_dynamic_distribution_module",
        re.compile(r"\bdist:(?:module|install-time|fast-follow|on-demand)\b"),
    ),
)

FORBIDDEN_IOS_BACKGROUND_ASSETS_PATTERNS: tuple[tuple[str, re.Pattern[str]], ...] = (
    (
        "ios_ba_initial_download_restrictions",
        re.compile(r"\bBAInitialDownloadRestrictions\b"),
    ),
    (
        "ios_ba_manifest_url",
        re.compile(r"\bBAManifestURL\b"),
    ),
    (
        "ios_ba_essential",
        re.compile(r"\bBAEssential\b"),
    ),
    (
        "ios_ba_download_allowance",
        re.compile(r"\bBADownloadAllowance\b"),
    ),
    (
        "ios_background_assets_framework",
        re.compile(r"\bBackgroundAssets(?:\.framework)?\b"),
    ),
    (
        "ios_background_assets_extension",
        re.compile(r"com\.apple\.(?:background-asset-downloader-extension|developer\.background-assets)"),
    ),
)

FORBIDDEN_IOS_PORTABLE_KEYCHAIN_PATTERNS: tuple[tuple[str, re.Pattern[str]], ...] = (
    (
        "ios_keychain_synchronizable",
        re.compile(r"\bkSecAttrSynchronizable\b"),
    ),
    (
        "ios_keychain_non_device_only_accessible",
        re.compile(
            r"\bkSecAttrAccessible(?:WhenUnlocked|AfterFirstUnlock|Always|WhenPasscodeSet)(?!ThisDeviceOnly)\b"
        ),
    ),
)


def scan_mobile_root(mobile_root: Path) -> list[str]:
    errors: list[str] = []

    android_files = [
        mobile_root / "android" / "settings.gradle.kts",
        mobile_root / "android" / "build.gradle.kts",
        mobile_root / "android" / "app" / "build.gradle.kts",
        mobile_root / "android" / "app" / "src" / "main" / "AndroidManifest.xml",
    ]
    for path in android_files:
        if not path.is_file():
            errors.append(f"missing_required_android_config:{path.relative_to(mobile_root).as_posix()}")
            continue
        text = path.read_text(encoding="utf-8")
        rel = path.relative_to(mobile_root).as_posix()
        for rule_id, pattern in FORBIDDEN_ANDROID_PAD_PATTERNS:
            if pattern.search(text):
                errors.append(f"{rule_id}:{rel}")

    manifest_path = mobile_root / "android" / "app" / "src" / "main" / "AndroidManifest.xml"
    if manifest_path.is_file():
        errors.extend(_verify_android_manifest(manifest_path, mobile_root))

    extraction_path = (
        mobile_root / "android" / "app" / "src" / "main" / "res" / "xml" / "data_extraction_rules.xml"
    )
    if not extraction_path.is_file():
        errors.append("missing_data_extraction_rules_xml")
    else:
        errors.extend(_verify_data_extraction_rules(extraction_path, mobile_root))

    backup_rules_path = (
        mobile_root / "android" / "app" / "src" / "main" / "res" / "xml" / "backup_rules.xml"
    )
    if not backup_rules_path.is_file():
        errors.append("missing_backup_rules_xml")
    else:
        errors.extend(_verify_backup_rules(backup_rules_path, mobile_root))

    ios_info_plist = mobile_root / "ios" / "Runner" / "Info.plist"
    ios_pbxproj = mobile_root / "ios" / "Runner.xcodeproj" / "project.pbxproj"
    ios_app_delegate = mobile_root / "ios" / "Runner" / "AppDelegate.swift"

    for required_ios in (ios_info_plist, ios_pbxproj, ios_app_delegate):
        if not required_ios.is_file():
            errors.append(
                f"missing_required_ios_config:{required_ios.relative_to(mobile_root).as_posix()}"
            )

    ios_scan_targets: list[Path] = []
    if (mobile_root / "ios").is_dir():
        for ext in ("*.plist", "*.pbxproj", "*.swift", "*.entitlements"):
            ios_scan_targets.extend((mobile_root / "ios").rglob(ext))

    for path in sorted(set(ios_scan_targets)):
        rel = path.relative_to(mobile_root).as_posix()
        text = path.read_text(encoding="utf-8")
        for rule_id, pattern in FORBIDDEN_IOS_BACKGROUND_ASSETS_PATTERNS:
            if pattern.search(text):
                errors.append(f"{rule_id}:{rel}")

    if ios_app_delegate.is_file():
        delegate_text = ios_app_delegate.read_text(encoding="utf-8")
        rel = ios_app_delegate.relative_to(mobile_root).as_posix()
        if not re.search(r"\bisExcludedFromBackup\s*=\s*true\b", delegate_text):
            errors.append(f"ios_missing_is_excluded_from_backup:{rel}")
        if "kSecAttrAccessibleWhenUnlockedThisDeviceOnly" not in delegate_text:
            errors.append(f"ios_missing_this_device_only_keychain_attr:{rel}")
        for rule_id, pattern in FORBIDDEN_IOS_PORTABLE_KEYCHAIN_PATTERNS:
            if pattern.search(delegate_text):
                errors.append(f"{rule_id}:{rel}")

    return errors


def _verify_android_manifest(manifest_path: Path, mobile_root: Path) -> list[str]:
    rel = manifest_path.relative_to(mobile_root).as_posix()
    errors: list[str] = []
    root = ET.fromstring(manifest_path.read_text(encoding="utf-8"))

    for perm in root.findall("uses-permission"):
        name = perm.attrib.get(f"{ANDROID_NS}name", "")
        if name == "android.permission.INTERNET":
            errors.append(f"android_bootstrap_internet_permission_forbidden:{rel}")

    app = root.find("application")
    if app is None:
        errors.append(f"android_manifest_missing_application:{rel}")
        return errors

    if app.attrib.get(f"{ANDROID_NS}allowBackup") != "false":
        errors.append(f"android_allow_backup_must_be_false:{rel}")
    if app.attrib.get(f"{ANDROID_NS}dataExtractionRules") != "@xml/data_extraction_rules":
        errors.append(f"android_missing_data_extraction_rules_ref:{rel}")
    if app.attrib.get(f"{ANDROID_NS}fullBackupContent") != "@xml/backup_rules":
        errors.append(f"android_missing_full_backup_content_ref:{rel}")
    return errors


def _verify_data_extraction_rules(xml_path: Path, mobile_root: Path) -> list[str]:
    rel = xml_path.relative_to(mobile_root).as_posix()
    errors: list[str] = []
    root = ET.fromstring(xml_path.read_text(encoding="utf-8"))
    for section_name in ("cloud-backup", "device-transfer"):
        section = root.find(section_name)
        if section is None:
            errors.append(f"missing_{section_name.replace('-', '_')}_section:{rel}")
            continue
        if section.findall("include"):
            errors.append(f"forbidden_include_in_{section_name.replace('-', '_')}:{rel}")
        excluded = {
            elem.attrib.get("domain", "")
            for elem in section.findall("exclude")
            if elem.attrib.get("path") == "."
        }
        missing = sorted(REQUIRED_BACKUP_DOMAINS - excluded)
        for domain in missing:
            errors.append(f"missing_{section_name.replace('-', '_')}_exclude_{domain}:{rel}")
    return errors


def _verify_backup_rules(xml_path: Path, mobile_root: Path) -> list[str]:
    rel = xml_path.relative_to(mobile_root).as_posix()
    errors: list[str] = []
    root = ET.fromstring(xml_path.read_text(encoding="utf-8"))
    if root.findall("include"):
        errors.append(f"forbidden_include_in_backup_rules:{rel}")
    excluded = {
        elem.attrib.get("domain", "")
        for elem in root.findall("exclude")
        if elem.attrib.get("path") == "."
    }
    for domain in sorted(REQUIRED_BACKUP_DOMAINS - excluded):
        errors.append(f"missing_backup_rules_exclude_{domain}:{rel}")
    return errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--mobile-root",
        type=Path,
        default=Path(__file__).resolve().parents[1],
        help="Path to src/onebrain-mobile",
    )
    args = parser.parse_args(argv)
    mobile_root = args.mobile_root.resolve()
    errors = scan_mobile_root(mobile_root)
    payload = {
        "scanner": "verify_native_release_guards",
        "mobile_root": mobile_root.as_posix(),
        "passed": len(errors) == 0,
        "error_count": len(errors),
        "errors": errors,
    }
    print(json.dumps(payload, indent=2))
    return 0 if not errors else 1


if __name__ == "__main__":
    sys.exit(main())
