#!/usr/bin/env python3
"""Dependency-free compliance checks for the OneBrain mobile build contract."""

from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "docs/design/mobile/mobile_build_contract_v1.json"

FEATURE_TREE = ROOT / "docs/features/mobile/MOBILE_APP_FEATURE_TREE_V1.md"
FEATURE_DETAILS = ROOT / "docs/features/mobile/MOBILE_APP_FEATURE_DETAILS_V1.md"
SITEMAP = ROOT / "docs/features/mobile/MOBILE_APP_SITEMAP_V1.md"
DESIGN_SYSTEM = ROOT / "docs/design/mobile/MOBILE_DESIGN_SYSTEM_V1.md"
COMPONENT_CATALOG = ROOT / "docs/design/mobile/MOBILE_COMPONENT_CATALOG_V1.md"
SCREEN_PATTERNS = ROOT / "docs/design/mobile/MOBILE_SCREEN_PATTERNS_V1.md"

FEATURE_TREE_ROW = re.compile(r"^(MOB-[A-Z]+-\d{3})\s", re.MULTILINE)
FEATURE_DETAIL_ROW = re.compile(
    r"^\|\s*`(MOB-[A-Z]+-\d{3})`\s*\|", re.MULTILINE
)
SCREEN_ROW = re.compile(
    r"^\|\s*`MOB-SCR-([A-Z]+-\d{3})`\s*\|", re.MULTILINE
)
COMPONENT_ROW = re.compile(
    r"^\|\s*`(OBM-CMP-[A-Z]+-\d{3})`\s*\|", re.MULTILINE
)
PATTERN_HEADING = re.compile(
    r"^###\s+`(OBM-PAT-\d{3})`", re.MULTILINE
)
MAPPING_ROW = re.compile(
    r"^\|\s*([^|]+?)\s*\|\s*`(OBM-PAT-\d{3})`\s*\|", re.MULTILINE
)
SCREEN_SPEC = re.compile(r"`([A-Z]+-\d{3}(?:\.\.\d{3})?)`")
MARKDOWN_LINK = re.compile(r"\[[^\]]*]\(([^)]+)\)")
HEX_COLOR = re.compile(r"^#[0-9A-Fa-f]{6}(?:[0-9A-Fa-f]{2})?$")
WORK_PACKAGE = re.compile(r"^MOB-0[0-9]$")
STABLE_ID = re.compile(
    r"^(?:MOB-[A-Z]+-\d{3}|MOB-SCR-[A-Z]+-\d{3}|"
    r"OBM-CMP-[A-Z]+-\d{3}|OBM-PAT-\d{3})$"
)


class MobileContractError(RuntimeError):
    """Raised when the mobile build contract is not satisfied."""


def relative(path: Path) -> str:
    try:
        return path.relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        raise MobileContractError(
            f"cannot read {relative(path)}: {error}"
        ) from error


def read_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(read_text(path))
    except json.JSONDecodeError as error:
        raise MobileContractError(
            f"invalid JSON in {relative(path)}: {error}"
        ) from error
    if not isinstance(value, dict):
        raise MobileContractError(f"{relative(path)} must contain a JSON object")
    return value


def sha256_file(path: Path) -> str:
    try:
        digest = hashlib.sha256()
        with path.open("rb") as stream:
            for block in iter(lambda: stream.read(1024 * 1024), b""):
                digest.update(block)
        return digest.hexdigest()
    except OSError as error:
        raise MobileContractError(
            f"cannot hash {relative(path)}: {error}"
        ) from error


def authority_set_sha256(authorities: list[dict[str, Any]]) -> str:
    canonical = "\n".join(
        f"{row['path']}:{row['sha256']}" for row in authorities
    )
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


def require_unique(values: list[str], label: str) -> set[str]:
    unique = set(values)
    if len(unique) != len(values):
        duplicates = sorted(
            value for value in unique if values.count(value) > 1
        )
        raise MobileContractError(f"duplicate {label}: {duplicates}")
    return unique


def validate_authorities(manifest: dict[str, Any]) -> dict[str, int | str]:
    if manifest.get("format") != "onebrain.mobile.build-contract/1":
        raise MobileContractError("unexpected mobile build-contract format")
    if manifest.get("version") != "1.0.0":
        raise MobileContractError("unexpected mobile build-contract version")

    authorities = manifest.get("authorities")
    required = manifest.get("required_read_set")
    if not isinstance(authorities, list) or not authorities:
        raise MobileContractError("authorities must be a non-empty list")
    if not isinstance(required, list) or len(required) != 10:
        raise MobileContractError(
            "required_read_set must contain the ten owner-selected mobile files"
        )

    paths: list[str] = []
    for row in authorities:
        if not isinstance(row, dict):
            raise MobileContractError("authority rows must be objects")
        path_value = row.get("path")
        expected_hash = row.get("sha256")
        if not isinstance(path_value, str) or not isinstance(expected_hash, str):
            raise MobileContractError("authority path/hash must be strings")
        paths.append(path_value)
        file_path = ROOT / path_value
        if not file_path.is_file():
            raise MobileContractError(f"missing authority file: {path_value}")
        actual_hash = sha256_file(file_path)
        if actual_hash != expected_hash:
            raise MobileContractError(
                f"authority hash drift for {path_value}: "
                f"expected {expected_hash}, got {actual_hash}; "
                "review semantics before updating the manifest"
            )
        heading = row.get("heading")
        if heading and not read_text(file_path).startswith(f"{heading}\n"):
            raise MobileContractError(
                f"authority heading drift for {path_value}: expected {heading!r}"
            )

    path_set = require_unique(paths, "authority paths")
    missing_required = sorted(set(required) - path_set)
    if missing_required:
        raise MobileContractError(
            f"required_read_set files absent from authorities: {missing_required}"
        )
    digest = authority_set_sha256(authorities)
    if manifest.get("authority_set_sha256") != digest:
        raise MobileContractError(
            "manifest authority_set_sha256 does not match pinned authorities"
        )
    return {
        "authorities": len(authorities),
        "required_read_set": len(required),
        "authority_set_sha256": digest,
    }


def expand_screen_spec(spec: str) -> list[str]:
    match = re.fullmatch(r"([A-Z]+)-(\d{3})(?:\.\.(\d{3}))?", spec)
    if not match:
        raise MobileContractError(f"invalid screen range in pattern map: {spec}")
    prefix, start_text, end_text = match.groups()
    start = int(start_text)
    end = int(end_text or start_text)
    if end < start:
        raise MobileContractError(f"descending screen range: {spec}")
    return [f"{prefix}-{number:03d}" for number in range(start, end + 1)]


def validate_structure(manifest: dict[str, Any]) -> dict[str, int]:
    expected = manifest.get("expected_structure")
    if not isinstance(expected, dict):
        raise MobileContractError("expected_structure must be an object")

    tree_ids = require_unique(
        FEATURE_TREE_ROW.findall(read_text(FEATURE_TREE)), "feature-tree IDs"
    )
    detail_ids = require_unique(
        FEATURE_DETAIL_ROW.findall(read_text(FEATURE_DETAILS)),
        "feature-detail IDs",
    )
    if tree_ids != detail_ids:
        raise MobileContractError(
            "feature tree/details mismatch: "
            f"tree-only={sorted(tree_ids - detail_ids)}, "
            f"details-only={sorted(detail_ids - tree_ids)}"
        )

    screens = require_unique(
        SCREEN_ROW.findall(read_text(SITEMAP)), "sitemap screen IDs"
    )
    components = require_unique(
        COMPONENT_ROW.findall(read_text(COMPONENT_CATALOG)),
        "component IDs",
    )
    patterns = require_unique(
        PATTERN_HEADING.findall(read_text(SCREEN_PATTERNS)), "pattern IDs"
    )

    pattern_text = read_text(SCREEN_PATTERNS)
    try:
        mapping_text = pattern_text.split(
            "## 3. Primary pattern mapping", maxsplit=1
        )[1].split("## 4. Critical journey composition", maxsplit=1)[0]
    except IndexError as error:
        raise MobileContractError(
            "cannot locate primary screen-pattern mapping section"
        ) from error

    mapped: list[str] = []
    used_patterns: set[str] = set()
    for screen_cell, pattern_id in MAPPING_ROW.findall(mapping_text):
        used_patterns.add(pattern_id)
        for spec in SCREEN_SPEC.findall(screen_cell):
            mapped.extend(expand_screen_spec(spec))
    mapped_set = require_unique(mapped, "mapped sitemap screens")
    if mapped_set != screens:
        raise MobileContractError(
            "screen-pattern mapping mismatch: "
            f"missing={sorted(screens - mapped_set)}, "
            f"extra={sorted(mapped_set - screens)}"
        )
    if used_patterns - patterns:
        raise MobileContractError(
            f"mapping uses undefined patterns: {sorted(used_patterns - patterns)}"
        )

    actual = {
        "features": len(tree_ids),
        "screens": len(screens),
        "components": len(components),
        "patterns": len(patterns),
        "mapped_screens": len(mapped_set),
    }
    for key, value in actual.items():
        if expected.get(key) != value:
            raise MobileContractError(
                f"expected_structure.{key}={expected.get(key)!r}, got {value}"
            )
    return actual


def iter_color_values(value: Any) -> list[str]:
    colors: list[str] = []
    if isinstance(value, str) and value.startswith("#"):
        colors.append(value)
    elif isinstance(value, dict):
        for child in value.values():
            colors.extend(iter_color_values(child))
    elif isinstance(value, list):
        for child in value:
            colors.extend(iter_color_values(child))
    return colors


def relative_luminance(color: str) -> float:
    channels = [int(color[index : index + 2], 16) / 255 for index in (1, 3, 5)]

    def linear(channel: float) -> float:
        if channel <= 0.04045:
            return channel / 12.92
        return ((channel + 0.055) / 1.055) ** 2.4

    red, green, blue = (linear(channel) for channel in channels)
    return 0.2126 * red + 0.7152 * green + 0.0722 * blue


def contrast_ratio(first: str, second: str) -> float:
    first_luminance = relative_luminance(first)
    second_luminance = relative_luminance(second)
    lighter = max(first_luminance, second_luminance)
    darker = min(first_luminance, second_luminance)
    return (lighter + 0.05) / (darker + 0.05)


def validate_tokens(manifest: dict[str, Any]) -> dict[str, int | float]:
    contract = manifest.get("design_tokens")
    if not isinstance(contract, dict):
        raise MobileContractError("design_tokens must be an object")
    token_path = ROOT / str(contract.get("path", ""))
    tokens = read_json(token_path)
    if tokens.get("format") != contract.get("format"):
        raise MobileContractError("unexpected mobile design-token format")
    if tokens.get("version") != contract.get("version"):
        raise MobileContractError("unexpected mobile design-token version")

    colors = iter_color_values(tokens.get("color"))
    invalid_colors = sorted(color for color in colors if not HEX_COLOR.fullmatch(color))
    if invalid_colors:
        raise MobileContractError(f"invalid token colors: {invalid_colors}")

    semantic_pairs = (
        ("background", "onBackground"),
        ("surface", "onSurface"),
        ("primary", "onPrimary"),
        ("primaryContainer", "onPrimaryContainer"),
        ("secondary", "onSecondary"),
        ("secondaryContainer", "onSecondaryContainer"),
        ("tertiary", "onTertiary"),
        ("tertiaryContainer", "onTertiaryContainer"),
        ("error", "onError"),
        ("errorContainer", "onErrorContainer"),
        ("info", "onInfo"),
        ("infoContainer", "onInfoContainer"),
        ("success", "onSuccess"),
        ("successContainer", "onSuccessContainer"),
        ("warning", "onWarning"),
        ("warningContainer", "onWarningContainer"),
        ("attention", "onAttention"),
        ("attentionContainer", "onAttentionContainer"),
        ("disabledContainer", "disabledContent"),
    )
    minimum = float(contract.get("minimum_text_contrast", 4.5))
    failures: list[str] = []
    appearance = tokens.get("appearance", {})
    inheritance = appearance.get("inheritance", {})
    semantic = tokens.get("color", {}).get("semantic", {})
    for theme_name in (
        "light",
        "dark",
        "highContrastLight",
        "highContrastDark",
    ):
        parent_name = inheritance.get(theme_name)
        resolved: dict[str, str] = {}
        if parent_name:
            resolved.update(semantic.get(parent_name, {}))
        resolved.update(semantic.get(theme_name, {}))
        for background, foreground in semantic_pairs:
            if background not in resolved or foreground not in resolved:
                continue
            ratio = contrast_ratio(resolved[background], resolved[foreground])
            if ratio < minimum:
                failures.append(
                    f"{theme_name}.{foreground}/{background}={ratio:.2f}"
                )

    statuses = tokens.get("color", {}).get("status", {})
    for theme_name in ("light", "dark"):
        for status_name, status in statuses.get(theme_name, {}).items():
            ratio = contrast_ratio(status["container"], status["content"])
            if ratio < minimum:
                failures.append(
                    f"status.{theme_name}.{status_name}={ratio:.2f}"
                )
    if failures:
        raise MobileContractError(
            f"design-token contrast below {minimum}: {failures}"
        )
    return {
        "color_values": len(colors),
        "contrast_failures": len(failures),
        "minimum_text_contrast": minimum,
    }


def validate_markdown(manifest: dict[str, Any]) -> dict[str, int]:
    markdown_paths = [
        ROOT / row["path"]
        for row in manifest["authorities"]
        if str(row["path"]).endswith(".md")
    ]
    markdown_paths.extend(
        (
            ROOT / "AGENTS.md",
            ROOT / "src/onebrain-mobile/AGENTS.md",
            ROOT / "src/onebrain-mobile/README.md",
            ROOT / "docs/design/mobile/MOBILE_BUILD_HARNESS_V1.md",
        )
    )
    paths = sorted(set(markdown_paths))
    broken_links: list[str] = []
    odd_fences: list[str] = []
    encoding_markers: list[str] = []
    mojibake = re.compile(
        r"[\u00c3\u00c2]|\u00e2(?:\u20ac|\u2122|\u0153|\u009d|\u201d|\u201c)"
    )
    for document in paths:
        text = read_text(document)
        if text.count("```") % 2:
            odd_fences.append(relative(document))
        if mojibake.search(text):
            encoding_markers.append(relative(document))
        for raw_target in MARKDOWN_LINK.findall(text):
            target = raw_target.strip().removeprefix("<").removesuffix(">")
            target = target.split("#", maxsplit=1)[0]
            if not target or re.match(r"^(?:https?:|mailto:|app:)", target):
                continue
            linked = (document.parent / target).resolve()
            if not linked.exists():
                broken_links.append(
                    f"{relative(document)} -> {raw_target.strip()}"
                )
    if broken_links:
        raise MobileContractError(f"broken local Markdown links: {broken_links}")
    if odd_fences:
        raise MobileContractError(f"unbalanced Markdown fences: {odd_fences}")
    if encoding_markers:
        raise MobileContractError(
            f"probable UTF-8 mojibake in: {encoding_markers}"
        )
    return {
        "markdown_files": len(paths),
        "broken_links": 0,
        "odd_fences": 0,
        "encoding_markers": 0,
    }


def dart_source_violations(
    file_path: Path, text: str, guards: dict[str, Any]
) -> list[str]:
    path_value = relative(file_path)
    violations: list[str] = []
    for rule in guards.get("forbidden_dart_patterns", []):
        if not isinstance(rule, dict):
            raise MobileContractError("Dart source guard rules must be objects")
        allowed_prefixes = rule.get("allowed_prefixes", [])
        if any(path_value.startswith(prefix) for prefix in allowed_prefixes):
            continue
        try:
            pattern = re.compile(str(rule["pattern"]), re.MULTILINE)
        except (KeyError, re.error) as error:
            raise MobileContractError(
                f"invalid Dart guard {rule.get('id')}: {error}"
            ) from error
        if pattern.search(text):
            violations.append(f"{rule.get('id')}:{path_value}")
    return violations


def validate_source_guards(manifest: dict[str, Any]) -> dict[str, int | str]:
    guards = manifest.get("source_guards")
    if not isinstance(guards, dict):
        raise MobileContractError("source_guards must be an object")
    source_root = ROOT / str(manifest.get("scope_root", ""))
    pubspec = source_root / "pubspec.yaml"

    violations: list[str] = []
    dart_files = list(source_root.glob("lib/**/*.dart")) if source_root.exists() else []
    for dart_file in dart_files:
        violations.extend(
            dart_source_violations(dart_file, read_text(dart_file), guards)
        )

    if pubspec.is_file():
        pubspec_text = read_text(pubspec)
        for dependency in guards.get("forbidden_pubspec_dependencies", []):
            pattern = re.compile(
                rf"(?m)^\s{{2,}}{re.escape(str(dependency))}\s*:"
            )
            if pattern.search(pubspec_text):
                violations.append(f"FORBIDDEN_DEPENDENCY:{dependency}")

    forbidden_suffixes = tuple(
        str(value).lower()
        for value in guards.get("forbidden_packaged_suffixes", [])
    )
    forbidden_names = {
        str(value).lower() for value in guards.get("forbidden_packaged_names", [])
    }
    maximum_bytes = int(guards.get("maximum_unlisted_asset_bytes", 0))
    package_files = 0
    for root_value in guards.get("package_roots", []):
        package_root = ROOT / str(root_value)
        if not package_root.exists():
            continue
        for packaged in package_root.rglob("*"):
            if not packaged.is_file():
                continue
            package_files += 1
            lowered = packaged.name.lower()
            if lowered in forbidden_names or lowered.endswith(forbidden_suffixes):
                violations.append(f"FORBIDDEN_PACKAGED_DATA:{relative(packaged)}")
            if maximum_bytes and packaged.stat().st_size > maximum_bytes:
                violations.append(f"OVERSIZED_PACKAGED_ASSET:{relative(packaged)}")

    if violations:
        raise MobileContractError(f"mobile source/package guard failures: {violations}")
    return {
        "dart_files_scanned": len(dart_files),
        "package_files_scanned": package_files,
        "source_guard_failures": 0,
    }


def validate_evidence(
    manifest: dict[str, Any], authority_digest: str
) -> dict[str, int | str]:
    evidence_contract = manifest.get("evidence")
    if not isinstance(evidence_contract, dict):
        raise MobileContractError("evidence must be an object")
    evidence_path = ROOT / str(evidence_contract.get("path", ""))
    evidence = read_json(evidence_path)
    if evidence.get("format") != evidence_contract.get("format"):
        raise MobileContractError("unexpected mobile build-evidence format")
    if evidence.get("contract_id") != manifest.get("contract_id"):
        raise MobileContractError("build evidence uses a different contract_id")
    if evidence.get("authority_set_sha256") != authority_digest:
        raise MobileContractError(
            "build evidence has not acknowledged the current authority set"
        )

    phase = evidence.get("phase")
    phases = ("pre_scaffold", "foundation", "feature", "release")
    if phase not in phases:
        raise MobileContractError(f"invalid mobile evidence phase: {phase!r}")
    work_package = evidence.get("work_package")
    if not isinstance(work_package, str) or not WORK_PACKAGE.fullmatch(work_package):
        raise MobileContractError(
            "evidence work_package must be one of MOB-00..MOB-09"
        )

    source_root = ROOT / str(manifest.get("scope_root", ""))
    pubspec_exists = (source_root / "pubspec.yaml").is_file()
    if phase == "pre_scaffold" and pubspec_exists:
        raise MobileContractError(
            "pubspec.yaml exists but evidence phase is still pre_scaffold"
        )
    if phase != "pre_scaffold":
        if not pubspec_exists:
            raise MobileContractError(
                f"evidence phase {phase} requires a Flutter pubspec.yaml"
            )
        missing_foundation = [
            value
            for value in evidence_contract.get("foundation_required_paths", [])
            if not (ROOT / str(value)).is_file()
        ]
        if missing_foundation:
            raise MobileContractError(
                f"mobile foundation evidence paths are missing: {missing_foundation}"
            )

    acknowledged = evidence.get("acknowledged_required_read_set")
    if phase != "pre_scaffold" and acknowledged != manifest.get("required_read_set"):
        raise MobileContractError(
            "implementation evidence must acknowledge the complete ordered "
            "required_read_set"
        )

    affected_ids = evidence.get("affected_ids")
    if not isinstance(affected_ids, list):
        raise MobileContractError("evidence affected_ids must be a list")
    invalid_ids = sorted(
        value
        for value in affected_ids
        if not isinstance(value, str) or not STABLE_ID.fullmatch(value)
    )
    if invalid_ids:
        raise MobileContractError(f"invalid affected stable IDs: {invalid_ids}")
    if phase in ("feature", "release") and not affected_ids:
        raise MobileContractError(f"{phase} evidence requires affected stable IDs")

    evidence_rows = evidence.get("evidence", [])
    if not isinstance(evidence_rows, list):
        raise MobileContractError("evidence.evidence must be a list")
    if phase in ("feature", "release") and not evidence_rows:
        raise MobileContractError(f"{phase} phase requires executable evidence")
    for row in evidence_rows:
        if not isinstance(row, dict) or not row.get("id") or not row.get("path"):
            raise MobileContractError("each evidence row requires id and path")
        if not (ROOT / str(row["path"])).exists():
            raise MobileContractError(
                f"recorded implementation evidence is missing: {row['path']}"
            )

    deviations = evidence.get("deviations", [])
    if not isinstance(deviations, list):
        raise MobileContractError("evidence deviations must be a list")
    for deviation in deviations:
        if (
            not isinstance(deviation, dict)
            or deviation.get("owner_approved") is not True
            or not deviation.get("adr")
            or not (ROOT / str(deviation["adr"])).is_file()
        ):
            raise MobileContractError(
                "every deviation requires owner_approved=true and an existing ADR"
            )
    return {
        "phase": phase,
        "work_package": work_package,
        "affected_ids": len(affected_ids),
        "evidence_rows": len(evidence_rows),
        "approved_deviations": len(deviations),
    }


def validate_contract(
    manifest_path: Path = MANIFEST,
) -> dict[str, dict[str, int | float | str]]:
    manifest = read_json(manifest_path)
    authority = validate_authorities(manifest)
    structure = validate_structure(manifest)
    tokens = validate_tokens(manifest)
    markdown = validate_markdown(manifest)
    source = validate_source_guards(manifest)
    evidence = validate_evidence(
        manifest, str(authority["authority_set_sha256"])
    )
    return {
        "authority": authority,
        "structure": structure,
        "tokens": tokens,
        "markdown": markdown,
        "source": source,
        "evidence": evidence,
    }


def main() -> int:
    try:
        summary = validate_contract()
    except MobileContractError as error:
        print(f"mobile build contract: FAIL: {error}", file=sys.stderr)
        return 1
    print("mobile build contract: PASS")
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
