"""Validate the owner-review KU inventory, not runtime/hash conformance."""
from __future__ import annotations

import base64
import binascii
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PROFILE = ROOT / "src/test-vectors/vnext/ku-product-workflow-v1.json"
BASE = ROOT / "src/test-vectors/vnext/base-v1-runtime-interface-v1.json"


class KuContractError(ValueError):
    pass


def require(condition: bool, reason: str) -> None:
    if not condition:
        raise KuContractError(reason)


def load_profile() -> dict:
    return json.loads(PROFILE.read_text(encoding="utf-8"))


def validate_value(profile: dict, name: str, value: object, depth: int = 0) -> None:
    require(depth <= 16, "DTO nesting budget")
    if depth == 0:
        require(len(json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode("utf-8")) <= profile["limits"]["payload_bytes"], "aggregate payload bound")
    if name in profile["dtos"]:
        spec = profile["dtos"][name]
        require(isinstance(value, dict), f"{name}: expected object")
        required, optional = spec["required"], spec["optional"]
        require(set(required) <= value.keys(), f"{name}: missing required field")
        require(value.keys() <= (required.keys() | optional.keys()), f"{name}: unknown field")
        for field, item in value.items():
            validate_value(profile, (required | optional)[field], item, depth + 1)
        if name == "KuFailureV1":
            rule = next(e for e in profile["errors"] if e["name"] == value["code"])
            require(all(value[k] is rule[k] for k in ("retryable", "reconcile_before_retry")), "error retry/reconcile drift")
        if name == "KuPreparedV1":
            require(value["semantic_profile"] == profile["identity"]["profile"], "unsupported semantic profile")
            if value["validity"] == "ready":
                ids = value["object_cids"]
                require(bool(ids) and len(set(ids)) == len(ids), "ready preview needs unique identities")
                require(ids == [a["object_cid"] for a in value["artifacts"]], "preview/identity set mismatch")
            else:
                require(not value["artifacts"] and not value["object_cids"], "unresolved draft cannot claim semantic identity")
        if name == "KuPrepareV1":
            require(value["semantic_profile"] == profile["identity"]["profile"], "unsupported semantic profile")
            require(bool(value["source_refs"]), "encoding needs exact source bindings")
            require(("draft_ref" in value) == (value["input_mode"] == "resolved_semantic_draft"), "input mode/draft binding")
        if name in ("KuSaveV1", "KuExportV1"):
            require(bool(value["object_cids"]), "empty object selection")
        if name in ("KuViewV1", "KuSummaryV1"):
            require(("fidelity_policy_cid" in value) == ("fidelity_frontier" in value), "fidelity policy/frontier pair")
            if value["artifact_validity"] == "accepted_opaque":
                require("semantic_content_cid" not in value, "opaque object cannot project semantic identity")
        if name == "KuExportViewV1":
            require(value["requires_base_management"] is (value["mode"] == "encrypted_base_archive"), "archive management boundary")
            archive = value["mode"] == "encrypted_base_archive"
            require(("archive_operation_id" in value) == archive and ("public_records" in value) != archive, "export payload/capability separation")
        return
    require(name in profile["types"], f"unknown type {name}")
    spec = profile["types"][name]
    kind = spec["kind"]
    if kind == "hex":
        require(isinstance(value, str) and re.fullmatch(r"[0-9a-f]{" + str(spec["bytes"] * 2) + "}", value) is not None, f"{name}: typed lowercase hex")
    elif kind == "integer":
        require(type(value) is int and spec["min"] <= value <= spec["max"], f"{name}: integer bound")
    elif kind == "boolean":
        require(type(value) is bool, f"{name}: boolean")
    elif kind == "literal":
        require(type(value) is type(spec["value"]) and value == spec["value"], f"{name}: literal drift")
    elif kind == "enum":
        require(isinstance(value, str) and value in spec["values"], f"{name}: closed enum")
    elif kind == "string":
        require(isinstance(value, str) and len(value.encode("utf-8")) <= spec["max_bytes"], f"{name}: string bound")
        if name == "Continuation":
            require(re.fullmatch(r"obc1\.[A-Za-z0-9_-]+", value) is not None, "continuation encoding")
            encoded = value[5:]
            try:
                decoded = base64.b64decode(encoded + "=" * (-len(encoded) % 4), altchars=b"-_", validate=True)
            except (ValueError, binascii.Error) as error:
                raise KuContractError("continuation encoding") from error
            require(base64.urlsafe_b64encode(decoded).decode().rstrip("=") == encoded, "continuation noncanonical bits")
    elif kind == "array":
        require(isinstance(value, list) and len(value) <= spec["max_items"], f"{name}: array bound")
        for item in value:
            validate_value(profile, spec["items"], item, depth + 1)
    elif kind == "base64":
        require(isinstance(value, str) and len(value) <= 4 * ((spec["max_decoded_bytes"] + 2) // 3), "preview allocation bound")
        try:
            decoded = base64.b64decode(value, validate=True)
        except (ValueError, binascii.Error) as error:
            raise KuContractError("preview base64") from error
        require(len(decoded) <= spec["max_decoded_bytes"] and base64.b64encode(decoded).decode() == value, "preview encoding/bound")
    else:
        raise KuContractError(f"unsupported type kind {kind}")


def validate_contract(profile: dict | None = None) -> tuple[int, int, int]:
    p = load_profile() if profile is None else profile
    base = json.loads(BASE.read_text(encoding="utf-8"))
    require(p["format"] == "onebrain/ku-product-workflow/1" and p["profile_id"] == "KU_PRODUCT_WORKFLOW_PROFILE_V1" and p["version"] == "1.0", "candidate identity")
    require(p["status"] == "owner_review_candidate" and p["implementation_enabled"] is False, "candidate cannot silently enable implementation")
    require(p["approval"] == {"required": ["KU-PC-A", "KU-PC-B", "KU-PC-C"], "accepted": False, "base_local_command_ids": {}, "rest_endpoints": [], "ws_events": [], "domain_registry_allocated": False}, "unapproved wire/domain allocation")
    identity = p["identity"]
    require(identity["profile"] == "ku-semantic-content/1.0" and identity["proposed_domain"] == "semantic-content/1" and identity["algorithm"] == "BLAKE3-256", "semantic identity profile/domain")
    require(identity["preimage"] == "UTF8(onebrain:vnext:semantic-content:1) || NUL || canonical_semantic_root" and identity["canonical_profile"] == "onebrain/canonical/1" and identity["semantic_root_version"] == [1, 0], "semantic preimage drift")
    require(identity["artifact_domain"] == "object/1" and identity["artifact_bytes_preserved"] is True and identity["semantic_cid_is_artifact_alias"] is False and identity["private_fingerprint_public"] is False, "artifact identity/privacy firewall")
    require(identity["normalize"] == ["resolved_ccid16", "extract_source_spans_to_private_binding", "alpha_normalize_sem_v1", "reduce_checked_exact_rationals", "reject_non_nfc"], "normalization drift")
    require(identity["preserve"] == ["statement_order", "argument_order", "constraints", "negation", "modality", "condition", "time", "location", "perspective", "tolerance", "source_unit_ccid", "unit_dimension", "unit_scale", "unit_offset"], "semantic qualifier loss")
    require(identity["exclude"] == ["source_spans", "source_artifact_cid", "model_commitment", "node_id", "run_id", "registry_release_root", "disclosure_class", "reward", "assessment"], "provenance in semantic identity")
    require(identity["unresolved_concepts"] == "needs_resolution_no_fallback" and identity["profile_change"] == "new_identity_domain_version", "concept fallback/profile drift")
    require(p["registry"] == {"signed_release_required": True, "pin_for_whole_run": True, "replay_uses_original_release": True, "hot_refresh_during_run": False}, "Registry run pinning")
    require(p["privacy"] == {"default_destination": "LOCAL_ONLY", "save_destinations": ["LOCAL_ONLY", "NEGOTIATED_ENCRYPTED"], "source_bytes": "exact_private", "private_failures": "encrypted_private_quarantine", "plaintext_fallback": False, "authority_from_client": False, "source_path_argument": False}, "private save/source firewall")
    require(p["limits"] == {"payload_bytes": 1048576, "page_items": 256, "work_units": 1000000, "text_bytes": 4096, "limitations": 64, "limitation_bytes": 128, "continuation_chars": 2048}, "finite product bounds")
    require(p["operation_protocol"] == base["operation_protocol"], "Base operation lifecycle drift")
    require(p["base_envelope_fields"] == base["common_cross_projection_fields"], "Base envelope fencing drift")
    require(p["idempotency"] == {"scope": ["principal", "operation_id", "idempotency_key", "dataset_generation"], "binds": ["command", "object_set", "destination", "source_bindings", "registry_profile", "predecessor", "expected_revision_frontier"], "exact_replay": "same_receipt", "changed_reuse": "conflict_no_write", "unknown_outcome": "reconcile_before_retry", "committed_means": "all_objects_provenance_revision_and_receipt_durable", "partial_visibility": False}, "atomicity/idempotency drift")
    require(p["continuation"] == {"prefix": "obc1.", "binds": ["principal", "dataset_generation", "store_set", "query_filter", "sort_index_version", "snapshot_frontier", "last_full_cid"], "changed_context": "conflict", "evicted_snapshot": "expired", "coverage": ["local_only", "partial"]}, "snapshot continuation drift")
    require(p["revision"] == {"owner": "private_local_journal", "predecessor_exact": True, "expected_frontier_required": True, "concurrent_successors": "preserve_branches", "identical_successor": "same_artifact_no_self_cycle", "replicated_authority": False}, "immutable revision drift")
    firewalls = {"save_publishes", "display_creates_use", "preview_writes_accepted", "save_materializes_mapping", "save_adopts", "result_authorizes_reward", "reward_blocks_local_ku", "semantic_alias_grants_disclosure", "legacy_dual_write", "new_ws_payloads"}
    require(set(p["firewalls"]) == firewalls and all(v is False for v in p["firewalls"].values()), "semantic firewall")
    errors = p["errors"]
    require([{k: v for k, v in e.items() if k != "rest_code"} for e in errors] == base["errors"], "Base typed error drift")
    expected_outer = ["invalid_request", "not_found", "conflict", "expired", "rate_limited", "capability_disabled", "dependency_unavailable", "conflict", "rate_limited", "internal_error", "dependency_unavailable", "dependency_unavailable", "internal_error"]
    require([e["rest_code"] for e in errors] == expected_outer, "REST error mapping")
    types, dtos = p["types"], p["dtos"]
    require(not (types.keys() & dtos.keys()), "type/DTO name collision")
    for name, spec in types.items():
        kind = spec["kind"]
        require(kind in {"hex", "integer", "boolean", "literal", "enum", "string", "array", "base64"}, "unknown type descriptor")
        if kind == "hex":
            require(spec["bytes"] == (16 if name == "CCID" else 32) and spec["role"] == name, "typed CID width/role")
        if kind in {"string", "array", "base64"}:
            bound = spec.get("max_bytes", spec.get("max_items", spec.get("max_decoded_bytes")))
            require(type(bound) is int and 0 < bound <= p["limits"]["payload_bytes"], "unbounded type")
        if kind == "array":
            require(spec["items"] in types or spec["items"] in dtos, "unknown array item type")
            require(spec["max_items"] <= 256, "array item ceiling")
    require(types["ObjectIDs"] == {"kind": "array", "items": "ObjectCID", "max_items": 256}, "artifact selection must use ObjectCID")
    require(types["Sources"] == {"kind": "array", "items": "SourceArtifactCID", "max_items": 256}, "typed source binding")
    require(types["Disclosure"]["values"] == p["privacy"]["save_destinations"], "save disclosure enum")
    require(types["BaseState"]["values"] == base["operation_protocol"]["states"] and types["BaseError"]["values"] == [e["name"] for e in errors], "Base DTO vocabulary")
    require(types["Continuation"] == {"kind": "string", "max_bytes": 2048} and types["PageLimit"] == {"kind": "integer", "min": 1, "max": 256}, "page/continuation DTO bounds")
    require(types["False"] == {"kind": "literal", "value": False}, "literal false weakened")
    for name, expected in {
        "Text": {"kind": "string", "max_bytes": 4096},
        "Limitation": {"kind": "string", "max_bytes": 128},
        "Limitations": {"kind": "array", "items": "Limitation", "max_items": 64},
        "CanonicalPreview": {"kind": "base64", "max_decoded_bytes": 1048576},
        "PreparedArtifacts": {"kind": "array", "items": "KuPreparedArtifactV1", "max_items": 256},
        "KuViews": {"kind": "array", "items": "KuSummaryV1", "max_items": 256},
        "Validity": {"kind": "enum", "values": ["ready", "needs_resolution", "rejected"]},
        "Coverage": {"kind": "enum", "values": ["local_only", "partial"]},
        "ArtifactValidity": {"kind": "enum", "values": ["accepted_known", "accepted_opaque"]},
    }.items():
        require(types[name] == expected, f"typed bound/vocabulary drift: {name}")
    forbidden = {"authorized", "authority_frontier", "private_key", "source_path", "wallet_balance"}
    for name, spec in dtos.items():
        fields = spec["required"] | spec["optional"]
        require(not (spec["required"].keys() & spec["optional"].keys()), "required/optional overlap")
        require(spec["additional_fields"] is False and not (fields.keys() & forbidden), "DTO authority/privacy bypass")
        require(all(t in types or t in dtos for t in fields.values()), "unknown DTO field type")
        for field in ("executable", "published", "authorizes_reward", "remote_encoding_enabled", "direct_issuance_enabled"):
            if field in fields:
                require(fields[field] == "False", "DTO effect flag weakened")
    for name, field, typ in [("KuGetV1", "object_cid", "ObjectCID"), ("KuReviseV1", "predecessor_object_cid", "ObjectCID"), ("KuReviseV1", "expected_revision_frontier", "RevisionFrontier"), ("KuPrepareV1", "registry_release_root", "ReleaseRoot"), ("KuPreparedArtifactV1", "canonical_preview", "CanonicalPreview"), ("KuPreparedV1", "artifacts", "PreparedArtifacts"), ("KuViewV1", "canonical_bytes", "CanonicalPreview")]:
        require(dtos[name]["required"].get(field) == typ, "required typed boundary drift")
    required_fields = {
        "KuPrepareV1": {"operation_id", "idempotency_key", "input_mode", "source_refs", "registry_release_root", "semantic_profile", "implementation_commitment", "destination"},
        "KuPreparedV1": {"operation_id", "validity", "object_cids", "registry_release_root", "semantic_profile", "destination", "limitations", "artifacts", "executable"},
        "KuPreparedArtifactV1": {"object_cid", "semantic_content_cid", "canonical_preview"},
        "KuOperationRefV1": {"operation_id"},
        "KuSaveV1": {"operation_id", "idempotency_key", "object_cids"},
        "KuReceiptV1": {"operation_id", "state", "object_cids", "limitations", "published", "authorizes_reward"},
        "KuGetV1": {"object_cid"},
        "KuViewV1": {"object_cid", "disclosure_class", "artifact_validity", "coverage", "limitations", "executable", "canonical_bytes"},
        "KuSummaryV1": {"object_cid", "disclosure_class", "artifact_validity", "coverage", "limitations", "executable"},
        "KuListV1": {"limit"}, "KuSearchV1": {"query", "limit"},
        "KuPageV1": {"items", "coverage", "snapshot_frontier", "limitations"},
        "KuReviseV1": {"preparation", "predecessor_object_cid", "expected_revision_frontier"},
        "KuExportV1": {"object_cids", "mode"},
        "KuExportViewV1": {"mode", "object_cids", "limitations", "requires_base_management"},
        "KuStatusV1": {"lifecycle", "coverage", "limitations", "registry_ready", "local_encoder_ready", "remote_encoding_enabled", "direct_issuance_enabled"},
        "KuStatusRequestV1": set(),
        "KuFailureV1": {"code", "retryable", "reconcile_before_retry", "limitations"},
    }
    require(dtos.keys() == required_fields.keys(), "DTO inventory drift")
    for name, fields in required_fields.items():
        require(dtos[name]["required"].keys() == fields, f"required DTO fields drift: {name}")
    optional_fields = {
        "KuPrepareV1": {"draft_ref"}, "KuListV1": {"continuation"},
        "KuSearchV1": {"continuation"}, "KuPageV1": {"continuation"},
        "KuViewV1": {"semantic_content_cid", "fidelity_policy_cid", "fidelity_frontier"},
        "KuSummaryV1": {"semantic_content_cid", "fidelity_policy_cid", "fidelity_frontier"},
        "KuExportViewV1": {"public_records", "archive_operation_id"},
        "KuStatusV1": {"receipt"}, "KuStatusRequestV1": {"operation_id"},
    }
    field_types = {
        "operation_id": "OperationId", "idempotency_key": "IdempotencyKey", "input_mode": "InputMode",
        "source_refs": "Sources", "registry_release_root": "ReleaseRoot", "semantic_profile": "Text",
        "implementation_commitment": "ImplementationCommitment", "destination": "Disclosure", "draft_ref": "ObjectCID",
        "validity": "Validity", "object_cids": "ObjectIDs", "limitations": "Limitations", "executable": "False",
        "artifacts": "PreparedArtifacts", "state": "BaseState", "published": "False", "authorizes_reward": "False",
        "object_cid": "ObjectCID", "disclosure_class": "ArtifactDisclosure", "artifact_validity": "ArtifactValidity",
        "coverage": "Coverage", "canonical_bytes": "CanonicalPreview", "semantic_content_cid": "SemanticContentCID",
        "fidelity_policy_cid": "PolicyCID", "fidelity_frontier": "RevisionFrontier", "limit": "PageLimit",
        "continuation": "Continuation", "query": "Text", "items": "KuViews", "snapshot_frontier": "RevisionFrontier",
        "preparation": "KuPrepareV1", "predecessor_object_cid": "ObjectCID", "expected_revision_frontier": "RevisionFrontier",
        "mode": "ExportMode", "requires_base_management": "Boolean", "public_records": "CanonicalPreview",
        "archive_operation_id": "OperationId", "lifecycle": "Lifecycle", "registry_ready": "Boolean",
        "local_encoder_ready": "Boolean", "remote_encoding_enabled": "False", "direct_issuance_enabled": "False",
        "receipt": "KuReceiptV1", "code": "BaseError", "retryable": "Boolean", "reconcile_before_retry": "Boolean",
        "canonical_preview": "CanonicalPreview",
    }
    for name, spec in dtos.items():
        require(spec["optional"].keys() == optional_fields.get(name, set()), f"optional DTO fields drift: {name}")
        require(all(field_types.get(k) == v for k, v in (spec["required"] | spec["optional"]).items()), f"DTO typed field ownership: {name}")
    boundaries = {"prepare": ("reserve_prepare", "private_staging"), "preview": ("query", "none"), "save": ("confirm", "atomic_private_save"), "get": ("query", "none"), "list": ("query", "none"), "search": ("query", "none"), "revise": ("reserve_prepare", "private_staging"), "export": ("query_or_CreateArchive", "explicit_export"), "status": ("status", "none"), "cancel": ("cancel", "cancel_staging"), "reconcile": ("reconcile", "journal_recovery")}
    ops = p["operations"]
    dto_bindings = {
        "prepare": ("KuPrepareV1", "KuPreparedV1"), "preview": ("KuOperationRefV1", "KuPreparedV1"),
        "save": ("KuSaveV1", "KuReceiptV1"), "get": ("KuGetV1", "KuViewV1"),
        "list": ("KuListV1", "KuPageV1"), "search": ("KuSearchV1", "KuPageV1"),
        "revise": ("KuReviseV1", "KuPreparedV1"), "export": ("KuExportV1", "KuExportViewV1"),
        "status": ("KuStatusRequestV1", "KuStatusV1"), "cancel": ("KuOperationRefV1", "KuReceiptV1"),
        "reconcile": ("KuOperationRefV1", "KuReceiptV1"),
    }
    require(len(ops) == len(boundaries) and {o["name"] for o in ops} == boundaries.keys(), "operation inventory")
    for op in ops:
        require((op["base_boundary"], op["effect"]) == boundaries[op["name"]], "operation side-effect boundary")
        require(op["wire_id"] is None and op["visibility"] == "authenticated_local_private" and op["surfaces"] == ["node", "rest", "cli", "web", "desktop"], "operation authority/surface drift")
        require(op["request"] in dtos and op["response"] in dtos, "operation DTO reference")
        require((op["request"], op["response"]) == dto_bindings[op["name"]], "operation DTO binding")
    deps = p["dependencies"]
    require(len(deps) == 5 and all(d["required"] is True and d["blocks"] for d in deps), "required dependency inventory")
    require([d["decision"] for d in deps] == ["D-011", "D-012", "D-013", "D-013", "D-014"], "owner decision dependency drift")
    required_work = [
        ("semantic_domain_registration_and_golden_vectors", ["semantic_identity_dispatch"]),
        ("signed_registry_publisher_peer_distribution", ["automatic_registry_sync"]),
        ("durable_capability_worker_claims", ["remote_encode", "automatic_work_claims"]),
        ("durable_blind_fidelity_attempts", ["remote_verify"]),
        ("economic_amendment_and_accepted_work_settlement", ["direct_obt_issuance"]),
    ]
    require([(d["work"], d["blocks"]) for d in deps] == required_work, "linked implementation gates drift")
    reward = deps[-1]
    require(reward["trigger"] == "accepted_encode_or_verify" and reward["requires_later_benefit_event"] is False and reward["correct_mismatch_eligible"] is True and reward["separate_reward_authorization"] is True, "D-014 direct issuance boundary")
    fixtures = p["fixtures"]
    require(len(fixtures) >= 10 and len({f["name"] for f in fixtures}) == len(fixtures), "fixture inventory")
    for f in fixtures:
        try:
            validate_value(p, f["dto"], f["value"])
            accepted = True
        except KuContractError:
            accepted = False
        require(type(f["valid"]) is bool and accepted == f["valid"], f"fixture result drift: {f['name']}")
    return len(ops), len(dtos), len(fixtures)


if __name__ == "__main__":
    try:
        operations, dtos, fixtures = validate_contract()
        print(f"KU candidate OK: {operations} operations, {dtos} DTOs, {fixtures} DTO fixtures; not runtime qualification")
    except (KuContractError, KeyError, TypeError, OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"KU candidate invalid: {error}")
