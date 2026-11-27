"""Offline KU-ENC-003 preparation audit; never admits or executes a model run.

Consumes the existing private intake/draft format. Hashes workbook bytes without
opening sheets, and reads only intake commitments. Exact file integrity is not
source authority, reviewer independence, historical blindness or a final lock.
Exit 2 means a valid preparation was audited but execution remains blocked.
"""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[2]
BUNDLE = Path("docs/specs/vnext/ku-encoder-v1")
MAX_JSON = 4 * 1024 * 1024
PENDING = ("reviewer_acknowledgement", "source_grants", "harness_build",
           "model_tuples", "analysis_lock")


def require(condition, code):
    if not condition:
        raise ValueError(code)


def sha256(raw):
    return hashlib.sha256(raw).hexdigest()


def is_digest(value):
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None


def strict_json(raw):
    require(len(raw) <= MAX_JSON, "json_size_limit")

    def pairs(items):
        result = {}
        for key, value in items:
            require(key not in result, "duplicate_json_key")
            result[key] = value
        return result

    def invalid(_):
        raise ValueError("noninteger_json_number")

    try:
        return json.loads(raw, object_pairs_hook=pairs,
                          parse_constant=invalid, parse_float=invalid)
    except (UnicodeError, json.JSONDecodeError, RecursionError):
        raise ValueError("invalid_json") from None


def local_path(value):
    require(isinstance(value, (str, Path)), "invalid_path")
    path = Path(value)
    # A UNC path is a remote-host read even when exposed as a filesystem path.
    require(path.is_absolute() and not str(path).startswith(("\\\\", "//")),
            "absolute_local_path_required")
    resolved = path.resolve()
    require(not str(resolved).startswith(("\\\\", "//")), "remote_path_forbidden")
    return resolved


def allowed_path(value, roots):
    path = local_path(value)
    require(any(path.is_relative_to(root) for root in roots), "input_outside_allowed_roots")
    require(path.is_file(), "input_file_missing")
    return path


def read_json(path):
    with path.open("rb") as stream:
        return strict_json(stream.read(MAX_JSON + 1))


def verify_pin(pin, roots):
    require(isinstance(pin, dict), "invalid_pin")
    require(is_digest(pin.get("sha256")), "invalid_pin_digest")
    require(type(pin.get("bytes")) is int and pin["bytes"] >= 0, "invalid_pin_size")
    path = allowed_path(pin.get("path"), roots)
    with path.open("rb") as stream:
        size = 0
        digest = hashlib.sha256()
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            size += len(block)
            require(size <= pin["bytes"], "artifact_size_mismatch")
            digest.update(block)
    require(size == pin["bytes"], "artifact_size_mismatch")
    require(digest.hexdigest() == pin["sha256"], "artifact_digest_mismatch")
    return path


def snapshot_json(pin, roots):
    path = verify_pin(pin, roots)
    with path.open("rb") as stream:
        raw = stream.read(MAX_JSON + 1)
    require(len(raw) == pin["bytes"] and sha256(raw) == pin["sha256"], "json_changed")
    return strict_json(raw)


def audit(draft_path, input_roots, root=ROOT):
    root = local_path(root)
    roots = [root, *(local_path(p) for p in input_roots)]
    draft_path = allowed_path(draft_path, roots)
    require(not draft_path.is_relative_to(root), "private_draft_required")
    with draft_path.open("rb") as stream:
        raw = stream.read(MAX_JSON + 1)
    draft = strict_json(raw)
    require(isinstance(draft, dict) and draft.get("kind") == "qualification_run_preparation_draft",
            "unsupported_preparation_format")
    require(draft.get("locked") is False and draft.get("model_qualified") is False,
            "draft_cannot_assert_lock_or_qualification")
    require(draft.get("existing_host_reused_or_modified") is False, "live_host_reuse_not_supported")
    require(isinstance(draft.get("candidate_git_head"), str) and
            re.fullmatch(r"[0-9a-f]{40}", draft["candidate_git_head"]), "invalid_candidate_ref")

    contracts = draft.get("contract_inputs")
    require(isinstance(contracts, list) and len(contracts) == 2, "contract_pins_missing")
    contract_paths = {verify_pin(pin, roots) for pin in contracts}
    require(contract_paths == {root / BUNDLE / n for n in ("profile.json", "bundle.manifest.json")},
            "wrong_contract_inputs")
    manifest = read_json(root / BUNDLE / "bundle.manifest.json")
    require(manifest.get("hash_algorithm") == "sha256", "unsupported_bundle_digest")
    artifacts = manifest.get("artifacts")
    require(isinstance(artifacts, dict) and bool(artifacts), "empty_bundle")
    for name, expected in artifacts.items():
        require(isinstance(name, str) and not Path(name).is_absolute(), "bundle_path_escape")
        path = allowed_path(root / name, [root])
        require(is_digest(expected) and sha256(path.read_bytes()) == expected, "bundle_drift")
    profile = read_json(root / BUNDLE / "profile.json")

    intake = snapshot_json(draft.get("intake"), roots)
    require(intake.get("kind") == "private_intake_inventory_not_run_lock" and
            intake.get("model_qualified") is False and type(intake.get("inference_calls")) is int and
            intake["inference_calls"] == 0, "invalid_intake_status")
    books = intake.get("workbooks")
    development = intake.get("development_exclusion_inventory")
    require(isinstance(books, list) and bool(books), "workbooks_missing")
    require(isinstance(development, list) and bool(development), "development_inventory_missing")
    metadata = draft.get("registry_metadata_candidates")
    require(isinstance(metadata, list) and bool(metadata), "registry_inventory_missing")
    pins = [*books, *development, draft.get("existing_host_config"), *metadata]
    paths = [verify_pin(pin, roots) for pin in pins]
    require(len(set(paths)) == len(paths), "duplicate_artifact_path")

    counts = Counter()
    reviewer_missing = 0
    blockers = ["final_run_lock_missing", "custody_permission_exposure_unverified",
                "group_independence_and_near_duplicates_unverified",
                "reviewed_labels_rubric_and_fid_plan_unverified",
                "exact_utf8_source_export_not_bound",
                "native_registry_signature_and_scope_unverified",
                "candidate_build_and_runtime_admission_unverified"]
    source_ids = set()
    text_hashes = set()
    for book in books:
        sources, labels = book.get("source_commitments"), book.get("label_commitments")
        require(isinstance(sources, list) and bool(sources) and isinstance(labels, list),
                "row_commitments_missing")
        for key, rows in (("source_count", sources), ("label_count", labels)):
            require(type(book.get(key)) is int and book[key] == len(rows), "row_count_mismatch")
        ids = [row.get("source_id_sha256") for row in sources]
        label_ids = [row.get("source_id_sha256") for row in labels]
        require(all(is_digest(v) for v in [*ids, *label_ids]), "invalid_row_id")
        require(len(set(ids)) == len(ids) and len(set(label_ids)) == len(label_ids), "duplicate_row_id")
        require(set(ids) == set(label_ids), "source_label_commitments_mismatch")
        require(not source_ids.intersection(ids), "cross_workbook_duplicate_id")
        source_ids.update(ids)
        texts = [row.get("source_text_sha256") for row in sources]
        require(all(is_digest(v) for v in texts) and
                all(is_digest(row.get("row_sha256")) for row in labels), "invalid_row_digest")
        if len(set(texts)) != len(texts) or text_hashes.intersection(texts):
            blockers.append("duplicate_nfc_source_commitment")
        text_hashes.update(texts)
        for field in ("duplicate_source_ids", "duplicate_label_ids", "nfc_exact_duplicate_sources",
                      "nfc_exact_development_overlap", "reviewer_fields_missing", "declared_groups"):
            require(type(book.get(field)) is int and book[field] >= 0, "invalid_intake_count")
        require(book["duplicate_source_ids"] == 0 and book["duplicate_label_ids"] == 0 and
                book.get("source_label_ids_match") is True, "invalid_intake_identity_summary")
        require(book["reviewer_fields_missing"] <= len(labels), "invalid_reviewer_count")
        reviewer_missing += book["reviewer_fields_missing"]
        if book["nfc_exact_development_overlap"]:
            blockers.append("development_overlap_reported")
        if book["nfc_exact_duplicate_sources"]:
            blockers.append("duplicate_nfc_source_reported")
        languages = book.get("language_counts")
        require(isinstance(languages, dict) and all(k in ("vi", "en") and
                type(v) is int and v > 0 for k, v in languages.items()) and
                sum(languages.values()) == len(sources), "invalid_language_counts")
        counts.update(languages)
        if book.get("split_counts") != {"holdout": len(sources)}:
            blockers.append("non_holdout_split")
    if reviewer_missing:
        blockers.append("reviewer_provenance_fields_missing")
    for language in profile["qualification"]["languages"]:
        if counts[language] < profile["qualification"]["min_blind_sources_per_language"]:
            blockers.append("insufficient_declared_source_count")
    # Presence never establishes truth or authorization. Populated future fields
    # require their own validators; this preparation utility cannot promote them.
    for field in PENDING:
        blockers.append(field + ("_missing" if not draft.get(field) else "_unverified"))
    repeats = profile["qualification"]["repeats_per_source"]
    return {
        "kind": "ku-enc-003-preparation-audit/1", "execution_admitted": False,
        "model_qualified": False, "inference_calls": 0,
        "auditor_sha256": sha256(Path(__file__).read_bytes()),
        "draft_sha256": sha256(raw), "candidate_git_head_claim": draft["candidate_git_head"],
        "candidate_build_verified": False,
        "verified_private_and_contract_pins": len(pins) + len(contracts) + 1,
        "verified_bundle_artifacts": len(artifacts),
        "declared_sources_by_language": dict(sorted(counts.items())),
        "source_count_is_independence_evidence": False,
        "missing_reviewer_fields": reviewer_missing,
        "repeats_per_source": repeats,
        "planned_rows_per_tuple_before_additional_sets": sum(counts.values()) * repeats,
        "attempt_rows_created": 0,
        "source_text_commitments": "NFC comparison hashes; not exact UTF-8 export bindings",
        "thresholds": profile["qualification"], "resource_profiles": profile["resource_profiles"],
        "blockers": sorted(set(blockers)),
    }


def write_report(target, report, root=ROOT):
    target = local_path(target)
    require(not target.is_relative_to(local_path(root)), "private_output_required")
    target.parent.mkdir(parents=True, exist_ok=True)
    # Exclusive create preserves every previous report, including failed attempts.
    with target.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(report, stream, ensure_ascii=False, indent=2)
        stream.write("\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--draft", required=True, type=Path)
    parser.add_argument("--input-root", required=True, action="append", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        report = audit(args.draft, args.input_root)
        write_report(args.output, report)
    except (ValueError, OSError, KeyError, TypeError, AttributeError):
        # Never print an exception that might contain a private path or value.
        print("Preparation audit failed; no execution admitted; no input was modified.")
        return 1
    print("Preparation audited; execution BLOCKED; 0 inference calls; 0 qualified tuples.")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
