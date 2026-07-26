#!/usr/bin/env python3
"""Dependency-free structural checks for OneBrain vNext contracts."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
PLAN = ROOT / "docs/research/ONEBRAIN_FOUNDATION_IMPLEMENTATION_PLAN_V7_1.md"
VNEXT = ROOT / "docs/specs/vnext"
TRACEABILITY = VNEXT / "TRACEABILITY_MATRIX_V1.md"
VOCABULARY = VNEXT / "NORMATIVE_VOCABULARY_V1.md"
NEGATIVE_ASSERTIONS = VNEXT / "negative_assertions.yaml"
NORMATIVE_COVERAGE = VNEXT / "normative_coverage.json"
VECTORS = ROOT / "src/test-vectors/vnext/foundation/canonical-v1.json"
IDENTITY_OBJECT_VECTORS = (
    ROOT / "src/test-vectors/vnext/foundation/identity-object-v1.json"
)
FEED_EVENT_VECTORS = ROOT / "src/test-vectors/vnext/foundation/feed-event-v1.json"
PRODUCT_PROFILE = (
    ROOT / "src/test-vectors/vnext/product-integration-profile-v1.json"
)
PRIVATE_WS_PROFILE = (
    ROOT / "src/test-vectors/vnext/private-websocket-profile-v1.json"
)
VNEXT_CLI_PROFILE = ROOT / "src/test-vectors/vnext/vnext-cli-profile-v1.json"
VNEXT_DESKTOP_WEB_UX_PROFILE = (
    ROOT / "src/test-vectors/vnext/vnext-desktop-web-ux-profile-v1.json"
)

TASK_ROW = re.compile(r"^\|\s*\[[ x~]\]\s*`([A-Z][A-Z0-9]*-\d{3})`")
TASK_ID = re.compile(r"(?<!ADR-)(?<!NEG-)\b[A-Z][A-Z0-9]*-\d{3}\b")
ADR_ID = re.compile(r"\bADR-[A-Z0-9]+-\d{3}-\d{2}\b")
NEGATIVE_ID = re.compile(r"\bNEG-[A-Z0-9-]+\b")
MARKDOWN_LINK = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
NORMATIVE_KEYWORD = re.compile(r"\b(?:MUST|SHOULD)\b")


class ContractError(RuntimeError):
    pass


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        raise ContractError(f"cannot read {path.relative_to(ROOT)}: {error}") from error


def plan_tasks() -> tuple[set[str], dict[str, set[str]]]:
    definitions: list[str] = []
    dependencies: dict[str, set[str]] = {}
    for line in read(PLAN).splitlines():
        match = TASK_ROW.match(line)
        if not match:
            continue
        task = match.group(1)
        definitions.append(task)
        cells = line.split("|")
        dependency_cell = cells[4] if len(cells) > 4 else ""
        dependencies[task] = set(TASK_ID.findall(dependency_cell))

    unique = set(definitions)
    if len(unique) != len(definitions):
        duplicates = sorted(task for task in unique if definitions.count(task) > 1)
        raise ContractError(f"duplicate plan task definitions: {duplicates}")
    if len(unique) < 99:
        raise ContractError(f"expected at least 99 plan tasks, found {len(unique)}")

    undefined = sorted(
        dependency
        for task_dependencies in dependencies.values()
        for dependency in task_dependencies
        if dependency not in unique
    )
    if undefined:
        raise ContractError(f"undefined task dependencies: {undefined}")
    assert_acyclic(dependencies)
    return unique, dependencies


def assert_acyclic(graph: dict[str, set[str]]) -> None:
    visiting: set[str] = set()
    visited: set[str] = set()

    def visit(task: str, path: list[str]) -> None:
        if task in visiting:
            cycle = " -> ".join(path + [task])
            raise ContractError(f"task dependency cycle: {cycle}")
        if task in visited:
            return
        visiting.add(task)
        for dependency in graph.get(task, set()):
            visit(dependency, path + [task])
        visiting.remove(task)
        visited.add(task)

    for task in graph:
        visit(task, [])


def validate_traceability(tasks: set[str]) -> int:
    text = read(TRACEABILITY)
    referenced_tasks = set(TASK_ID.findall(text))
    undefined = sorted(referenced_tasks - tasks)
    if undefined:
        raise ContractError(f"traceability references undefined tasks: {undefined}")
    adrs = set(ADR_ID.findall(text))
    if len(adrs) < 18:
        raise ContractError(f"expected at least 18 traced ADRs, found {len(adrs)}")
    return len(adrs)


def validate_negative_assertions() -> int:
    yaml_text = read(NEGATIVE_ASSERTIONS)
    ids = re.findall(r"(?m)^\s*-\s+id:\s*([A-Z0-9-]+)\s*$", yaml_text)
    if len(ids) != len(set(ids)):
        raise ContractError("duplicate negative assertion IDs")
    if len(ids) < 37:
        raise ContractError(f"expected at least 37 negative assertions, found {len(ids)}")
    vocabulary_ids = set(NEGATIVE_ID.findall(read(VOCABULARY)))
    missing = sorted(set(ids) - vocabulary_ids)
    if missing:
        raise ContractError(f"negative assertions missing from vocabulary: {missing}")
    return len(ids)


def validate_vectors() -> tuple[int, int, int, int]:
    try:
        vectors = json.loads(read(VECTORS))
    except json.JSONDecodeError as error:
        raise ContractError(f"invalid foundation vector JSON: {error}") from error
    if vectors.get("format") != "onebrain/foundation-vectors/1":
        raise ContractError("unexpected foundation vector format")
    if vectors.get("canonical_profile") != "onebrain/canonical/1":
        raise ContractError("unexpected canonical profile")

    sections = (
        "valid_cbor",
        "invalid_cbor",
        "normalized_text",
        "domain_digests",
        "envelopes",
        "signatures",
    )
    ids: list[str] = []
    for section in sections:
        rows = vectors.get(section)
        if not isinstance(rows, list) or not rows:
            raise ContractError(f"vector section {section} must be a non-empty list")
        ids.extend(row.get("id", "") for row in rows if isinstance(row, dict))
    if any(not vector_id for vector_id in ids):
        raise ContractError("foundation vector without an ID")
    if len(ids) != len(set(ids)):
        raise ContractError("duplicate foundation vector IDs")

    domains = [row.get("domain") for row in vectors["domain_digests"]]
    if len(domains) != 21 or len(set(domains)) != 21:
        raise ContractError("foundation vectors must cover 21 unique reserved domains")

    try:
        schema_vectors = json.loads(read(IDENTITY_OBJECT_VECTORS))
    except json.JSONDecodeError as error:
        raise ContractError(f"invalid identity-object vector JSON: {error}") from error
    if schema_vectors.get("format") != "onebrain/schema-vectors/1":
        raise ContractError("unexpected schema vector format")
    if schema_vectors.get("schema_profile") != "onebrain/identity-object/1":
        raise ContractError("unexpected identity-object profile")
    identities = schema_vectors.get("identities", [])
    objects = schema_vectors.get("objects", [])
    schema_ids = [row.get("id", "") for row in identities + objects]
    if len(identities) < 5 or len(objects) < 3:
        raise ContractError("identity-object vectors lack required coverage")
    if any(not vector_id for vector_id in schema_ids) or len(schema_ids) != len(
        set(schema_ids)
    ):
        raise ContractError("missing or duplicate identity-object vector IDs")
    collision_pair = identities[:2]
    if len(collision_pair) != 2:
        raise ContractError("missing full-width collision pair")
    left = bytes.fromhex(collision_pair[0]["raw_hex"])
    right = bytes.fromhex(collision_pair[1]["raw_hex"])
    if left[:8] != right[:8] or left == right:
        raise ContractError("identity collision pair must share only its 64-bit prefix")

    try:
        event_vectors = json.loads(read(FEED_EVENT_VECTORS))
    except json.JSONDecodeError as error:
        raise ContractError(f"invalid feed-event vector JSON: {error}") from error
    if event_vectors.get("format") != "onebrain/schema-vectors/1":
        raise ContractError("unexpected feed-event vector format")
    if event_vectors.get("schema_profile") != "onebrain/feed-event/1":
        raise ContractError("unexpected feed-event profile")
    feeds = event_vectors.get("feed_inceptions", [])
    events = event_vectors.get("events", [])
    event_ids = [row.get("id", "") for row in feeds + events]
    if len(feeds) < 1 or len(events) < 3:
        raise ContractError("feed-event vectors lack required coverage")
    if any(not vector_id for vector_id in event_ids) or len(event_ids) != len(
        set(event_ids)
    ):
        raise ContractError("missing or duplicate feed-event vector IDs")
    if not any(row.get("error") == "SIGNATURE_INVALID" for row in events):
        raise ContractError("feed-event vectors must include signature rejection")
    if not any(row.get("opaque") is True for row in events):
        raise ContractError("feed-event vectors must include opaque event semantics")
    return len(ids), len(domains), len(schema_ids), len(event_ids)


def validate_product_integration_profile(
    profile: dict[str, object] | None = None,
) -> tuple[int, int]:
    if profile is None:
        try:
            profile = json.loads(read(PRODUCT_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(
                f"invalid product integration profile JSON: {error}"
            ) from error

    if profile.get("format") != "onebrain/vnext-product-integration-profile/1":
        raise ContractError("unexpected product integration profile format")
    if profile.get("profile_id") != "VNEXT_PRODUCT_INTEGRATION_PROFILE_V1":
        raise ContractError("unexpected product integration profile ID")
    if profile.get("version") != 1 or profile.get("base_path") != "/api/vnext":
        raise ContractError("unexpected product integration version/base path")

    wire = profile.get("wire")
    if not isinstance(wire, dict):
        raise ContractError("product integration profile lacks wire rules")
    cid = wire.get("cid")
    expected_cid = {
        "encoding": "lowercase_hex",
        "decoded_bytes": 32,
        "encoded_chars": 64,
        "prefix": "",
        "typed": True,
    }
    if cid != expected_cid:
        raise ContractError("product API CID encoding drift")
    continuation = wire.get("continuation")
    if not isinstance(continuation, dict) or any(
        continuation.get(key) != value
        for key, value in {
            "encoding": "base64url_no_pad",
            "prefix": "obc1.",
            "opaque": True,
            "context_bound": True,
            "max_chars": 2048,
        }.items()
    ):
        raise ContractError("product API continuation contract drift")

    envelope = profile.get("envelope")
    if not isinstance(envelope, dict):
        raise ContractError("product integration profile lacks envelope rules")
    expected_envelope = {
        "success_required": {"ok", "profile", "data", "meta"},
        "error_required": {"ok", "profile", "error", "meta"},
        "meta_required": {"lifecycle", "coverage", "limitations", "continuation"},
        "lifecycle_states": {"disabled", "requested", "active", "degraded"},
        "coverage_states": {"local_only", "partial"},
        "work_states": {"pending", "deferred", "quarantined", "conflict"},
    }
    for field, expected in expected_envelope.items():
        value = envelope.get(field)
        if not isinstance(value, list) or set(value) != expected:
            raise ContractError(f"product envelope field drift: {field}")

    errors = profile.get("errors")
    expected_errors = {
        ("invalid_request", 400, False),
        ("not_found", 404, False),
        ("conflict", 409, False),
        ("expired", 410, False),
        ("rate_limited", 429, True),
        ("capability_disabled", 503, False),
        ("dependency_unavailable", 503, True),
        ("internal_error", 500, False),
    }
    if not isinstance(errors, list):
        raise ContractError("product integration profile lacks error semantics")
    actual_errors = {
        (row.get("code"), row.get("http_status"), row.get("retryable"))
        for row in errors
        if isinstance(row, dict)
    }
    if actual_errors != expected_errors:
        raise ContractError("product error code/status/retryability drift")

    dtos = profile.get("dtos")
    if not isinstance(dtos, dict) or not dtos:
        raise ContractError("product integration profile lacks DTO definitions")
    for name, dto in dtos.items():
        if not isinstance(name, str) or not isinstance(dto, dict):
            raise ContractError("invalid product DTO definition")
        required = dto.get("required")
        if not isinstance(required, list) or not required:
            raise ContractError(f"product DTO lacks required fields: {name}")
        if len(required) != len(set(required)):
            raise ContractError(f"duplicate required field in product DTO: {name}")

    field_types = profile.get("dto_field_types")
    if not isinstance(field_types, dict) or set(field_types) != set(dtos):
        raise ContractError("product DTO type inventory drift")
    for name, fields in field_types.items():
        if (
            not isinstance(fields, dict)
            or set(fields) != set(dtos[name]["required"])
            or any(not isinstance(value, str) or not value for value in fields.values())
        ):
            raise ContractError(f"product DTO field-type drift: {name}")

    expected_endpoints = {
        ("GET", "/api/vnext/workflow"),
        ("GET", "/api/vnext/workflow/{stage}"),
        ("POST", "/api/vnext/kql/needs/prepare"),
        ("POST", "/api/vnext/kql/needs"),
        ("GET", "/api/vnext/kql/needs"),
        ("GET", "/api/vnext/kql/needs/{id}"),
        ("GET", "/api/vnext/kql/needs/{id}/matches"),
        ("POST", "/api/vnext/kql/needs/{id}/scan"),
        ("DELETE", "/api/vnext/kql/needs/{id}"),
        ("POST", "/api/vnext/pomv/public-use/prepare"),
        ("POST", "/api/vnext/pomv/public-use/confirm"),
        ("GET", "/api/vnext/pomv/publications/{id}"),
        ("GET", "/api/vnext/pomv/views/{target}"),
        ("GET", "/api/vnext/runtime/status"),
    }
    endpoints = profile.get("endpoints")
    if not isinstance(endpoints, list):
        raise ContractError("product integration profile lacks endpoints")
    actual_endpoints: set[tuple[str, str]] = set()
    for endpoint in endpoints:
        if not isinstance(endpoint, dict):
            raise ContractError("invalid product endpoint row")
        method = endpoint.get("method")
        path = endpoint.get("path")
        status = endpoint.get("status")
        visibility = endpoint.get("visibility")
        request = endpoint.get("request")
        response = endpoint.get("response")
        if not isinstance(method, str) or not isinstance(path, str):
            raise ContractError("product endpoint lacks method/path")
        if not path.startswith("/api/vnext/"):
            raise ContractError(f"vNext endpoint escaped additive namespace: {path}")
        key = (method, path)
        if key in actual_endpoints:
            raise ContractError(f"duplicate product endpoint: {method} {path}")
        actual_endpoints.add(key)
        if status not in {"implemented_read_only", "reserved"}:
            raise ContractError(f"invalid product endpoint status: {method} {path}")
        expected_visibility = (
            "authenticated_local_private"
            if path.startswith("/api/vnext/kql/needs")
            or path.startswith("/api/vnext/pomv/public-use")
            else "authenticated_local"
        )
        if visibility != expected_visibility:
            raise ContractError(
                f"product endpoint visibility drift: {method} {path}"
            )
        if request is not None and request not in dtos:
            raise ContractError(f"undefined request DTO for {method} {path}: {request}")
        if not isinstance(response, str) or response not in dtos:
            raise ContractError(f"undefined response DTO for {method} {path}: {response}")
    if actual_endpoints != expected_endpoints:
        raise ContractError(
            "product endpoint inventory drift; "
            f"missing={sorted(expected_endpoints - actual_endpoints)}, "
            f"extra={sorted(actual_endpoints - expected_endpoints)}"
        )

    forbidden_client = profile.get("client_supplied_forbidden_fields")
    forbidden_response = profile.get("response_forbidden_fields")
    non_exportable = profile.get("non_exportable_fields")
    if (
        not isinstance(forbidden_client, list)
        or not isinstance(forbidden_response, list)
        or not isinstance(non_exportable, list)
    ):
        raise ContractError("product profile lacks field firewalls")
    client_fields = set(forbidden_client)
    response_fields = set(forbidden_response)
    for endpoint in endpoints:
        request_name = endpoint.get("request")
        if isinstance(request_name, str):
            request_fields = set(dtos[request_name]["required"])
            if request_fields & client_fields:
                raise ContractError(
                    f"client can supply authority/secret field through {request_name}"
                )
        response_name = endpoint["response"]
        output_fields = set(dtos[response_name]["required"])
        if output_fields & response_fields:
            raise ContractError(f"private field leaks through {response_name}")
    if set(non_exportable) != {
        "local_query",
        "standing_need_id",
        "query_definition_cid",
        "single_use_receipt",
    }:
        raise ContractError("non-exportable product field inventory drift")

    legacy = profile.get("legacy_surfaces")
    expected_legacy = {
        ("/api/kql", "legacy_local_kql", True),
        ("/api/watch", "legacy_watch", True),
        ("pomv", "legacy_local_pomv_scalar_v1", True),
        ("pomv_breakdown", "legacy_local_pomv_breakdown_v1", True),
    }
    if not isinstance(legacy, list):
        raise ContractError("product profile lacks legacy boundary inventory")
    actual_legacy = {
        (row.get("path"), row.get("meaning"), row.get("unchanged"))
        for row in legacy
        if isinstance(row, dict)
    }
    if actual_legacy != expected_legacy:
        raise ContractError("legacy product meaning changed or inventory drifted")

    firewalls = profile.get("semantic_firewalls")
    required_firewalls = {
        "proposal_executable",
        "proposal_materializes_mapping",
        "proposal_grants_authority",
        "pomv_establishes_truth",
        "pomv_establishes_benefit",
        "pomv_authorizes_reward",
        "pomv_claims_global_completion",
        "zero_results_claim_global_absence",
    }
    if (
        not isinstance(firewalls, dict)
        or set(firewalls) != required_firewalls
        or any(value is not False for value in firewalls.values())
    ):
        raise ContractError("product semantic firewall must remain fail-closed")

    match_fields = set(dtos["QuarantinedMatchV1"]["required"])
    metabolic_fields = set(dtos["MetabolicEvidenceViewV1"]["required"])
    if not {"state", "executable", "limitations"} <= match_fields:
        raise ContractError("quarantined match DTO lost non-executable fields")
    if (
        field_types["QuarantinedMatchV1"]["state"] != "literal:quarantined"
        or field_types["QuarantinedMatchV1"]["executable"] != "literal:false"
    ):
        raise ContractError("quarantined match DTO weakened its literal types")
    if not {
        "establishes_truth",
        "establishes_benefit",
        "authorizes_reward",
        "claims_global_completion",
        "limitations",
    } <= metabolic_fields:
        raise ContractError("metabolic view DTO lost semantic firewall fields")
    if any(
        field_types["MetabolicEvidenceViewV1"][field] != "literal:false"
        for field in {
            "establishes_truth",
            "establishes_benefit",
            "authorizes_reward",
            "claims_global_completion",
        }
    ):
        raise ContractError("metabolic view DTO weakened its literal false types")

    return len(endpoints), len(dtos)


def validate_private_websocket_profile(
    profile: dict[str, object] | None = None,
) -> tuple[int, int]:
    if profile is None:
        try:
            profile = json.loads(read(PRIVATE_WS_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(
                f"invalid private WebSocket profile JSON: {error}"
            ) from error

    if profile.get("format") != "onebrain/vnext-private-websocket-profile/1":
        raise ContractError("unexpected private WebSocket profile format")
    if profile.get("profile_id") != "VNEXT_PRIVATE_WEBSOCKET_PROFILE_V1":
        raise ContractError("unexpected private WebSocket profile ID")
    if profile.get("version") != 1:
        raise ContractError("unexpected private WebSocket profile version")

    expected_routes = {
        (
            "POST",
            "/api/vnext/ws/tickets",
            "bearer",
            "VNextWsTicketRequestV1",
            "VNextWsTicketV1",
        ),
        (
            "GET",
            "/api/vnext/ws",
            "single_use_ticket",
            "VNextWsTicketQuery",
            "websocket_upgrade",
        ),
    }
    routes = profile.get("routes")
    if not isinstance(routes, list):
        raise ContractError("private WebSocket profile lacks routes")
    actual_routes = {
        (
            row.get("method"),
            row.get("path"),
            row.get("authentication"),
            row.get("request"),
            row.get("response"),
        )
        for row in routes
        if isinstance(row, dict)
    }
    if actual_routes != expected_routes:
        raise ContractError("private WebSocket route inventory drift")

    expected_ticket = {
        "encoding": "base64url_no_pad",
        "prefix": "obw1.",
        "decoded_bytes": 32,
        "single_use": True,
        "ticket_ttl_seconds": 30,
        "session_ttl_seconds": 900,
        "subscription_immutable": True,
    }
    if profile.get("ticket") != expected_ticket:
        raise ContractError("private WebSocket ticket contract drift")

    expected_topics = {"matches", "publications", "views", "runtime"}
    topics = profile.get("topics")
    if not isinstance(topics, list) or set(topics) != expected_topics:
        raise ContractError("private WebSocket topic inventory drift")

    expected_envelope = {
        "profile",
        "event_type",
        "sequence",
        "timestamp",
        "lifecycle",
        "coverage",
        "limitations",
        "data",
    }
    envelope = profile.get("event_envelope_required")
    if not isinstance(envelope, list) or set(envelope) != expected_envelope:
        raise ContractError("private WebSocket event envelope drift")

    expected_events = {
        "subscription_ready",
        "bounded_match_available",
        "publication_queued",
        "publication_delivered",
        "publication_deferred",
        "view_revision",
        "view_conflict",
        "lane_active",
        "lane_disabled",
        "lane_degraded",
    }
    events = profile.get("event_types")
    if not isinstance(events, list) or set(events) != expected_events:
        raise ContractError("private WebSocket event inventory drift")

    expected_limits = {
        "max_topics": 4,
        "max_pending_tickets": 128,
        "max_active_sessions": 64,
        "event_queue_capacity": 32,
        "max_client_message_bytes": 4096,
    }
    if profile.get("limits") != expected_limits:
        raise ContractError("private WebSocket bounded limits drift")

    expected_publication_states = {
        "queued": "pending",
        "delivered": "delivered",
        "deferred": "deferred",
        "delivered_requires_durable_authenticated_acknowledgement": True,
    }
    if profile.get("publication_states") != expected_publication_states:
        raise ContractError("private WebSocket publication state drift")

    expected_non_exportable = {
        "local_query",
        "standing_need_id",
        "query_definition_cid",
        "private_target",
        "proposal_cid",
        "single_use_receipt",
        "client_session",
        "ticket",
    }
    non_exportable = profile.get("event_non_exportable_fields")
    if not isinstance(non_exportable, list) or set(non_exportable) != expected_non_exportable:
        raise ContractError("private WebSocket non-exportable field inventory drift")

    expected_firewalls = {
        "cross_client_delivery",
        "event_is_authority",
        "event_materializes_mapping",
        "event_creates_use_evidence",
        "event_establishes_truth",
        "event_establishes_benefit",
        "event_authorizes_reward",
        "event_claims_global_completion",
        "slow_client_blocks_runtime",
    }
    firewalls = profile.get("semantic_firewalls")
    if (
        not isinstance(firewalls, dict)
        or set(firewalls) != expected_firewalls
        or any(value is not False for value in firewalls.values())
    ):
        raise ContractError("private WebSocket semantic firewall must remain fail-closed")

    return len(events), len(topics)


def validate_vnext_cli_profile(
    profile: dict[str, object] | None = None,
) -> int:
    if profile is None:
        try:
            profile = json.loads(read(VNEXT_CLI_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid vNext CLI profile JSON: {error}") from error

    if profile.get("format") != "onebrain/vnext-cli-profile/1":
        raise ContractError("unexpected vNext CLI profile format")
    if profile.get("profile_id") != "VNEXT_CLI_PROFILE_V1":
        raise ContractError("unexpected vNext CLI profile ID")
    if profile.get("version") != 1:
        raise ContractError("unexpected vNext CLI profile version")

    expected_commands = {
        "need prepare",
        "need activate",
        "need list",
        "need scan",
        "need matches",
        "need retire",
        "pomv use prepare",
        "pomv use confirm",
        "pomv use status",
        "pomv view",
        "vnext status",
    }
    commands = profile.get("commands")
    if not isinstance(commands, list) or set(commands) != expected_commands:
        raise ContractError("vNext CLI command inventory drift")

    transport = profile.get("transport")
    if transport != {
        "contract": "VNEXT_PRODUCT_INTEGRATION_PROFILE_V1",
        "authentication": "bearer",
        "default_api_url": "http://127.0.0.1:4280",
        "token_environment": "ONEBRAIN_API_TOKEN",
    }:
        raise ContractError("vNext CLI transport contract drift")

    need = profile.get("need")
    expected_need = {
        "scope": "one_hop",
        "raw_query_local_only": True,
        "exact_replay_identity": True,
        "match_state": "quarantined",
        "match_executable": False,
        "zero_result_claims_global_absence": False,
        "continuation_opaque": True,
    }
    if need != expected_need:
        raise ContractError("vNext CLI Need firewall drift")

    public_use = profile.get("public_use")
    if not isinstance(public_use, dict):
        raise ContractError("vNext CLI profile lacks Public Use contract")
    expected_preview = {
        "canonical_payload_preview",
        "exact_target",
        "exact_recipient",
        "selector_cid",
        "namespace",
        "disclosure",
        "intent_cid",
        "expires_at",
    }
    if (
        public_use.get("prepare_creates_evidence") is not False
        or public_use.get("prepare_requires_public_permanent_acknowledgement")
        is not True
        or set(public_use.get("preview_required", [])) != expected_preview
        or public_use.get("confirmation_requires_exact_typed_intent") is not True
        or public_use.get("yes_bypass") is not False
        or public_use.get("single_use_receipt_exported") is not False
        or public_use.get("exact_replay_identity") is not True
        or public_use.get("delivery_acknowledgement_inferred") is not False
    ):
        raise ContractError("vNext CLI Public Use contract drift")

    signer = profile.get("feed_signer")
    if not isinstance(signer, dict):
        raise ContractError("vNext CLI profile lacks Feed signer contract")
    if (
        signer.get("selection_explicit") is not True
        or set(signer.get("providers", [])) != {"none", "development-file"}
        or signer.get("development_file_requires_opt_in") is not True
        or signer.get("development_file_warning") is not True
        or signer.get("development_file_production_custody") is not False
        or signer.get("fallback_on_failure") is not False
    ):
        raise ContractError("vNext CLI Feed signer contract drift")

    expected_false = {
        "establishes_truth": False,
        "establishes_benefit": False,
        "authorizes_reward": False,
        "claims_global_completion": False,
        "conflict_displays_authorized": False,
    }
    if profile.get("view_firewalls") != expected_false:
        raise ContractError("vNext CLI view firewall drift")

    expected_status = {
        "compiled",
        "requested",
        "active",
        "kill_switch",
        "signer_ready",
        "lifecycle",
        "coverage",
        "limitations",
    }
    status_fields = profile.get("status_fields")
    if not isinstance(status_fields, list) or set(status_fields) != expected_status:
        raise ContractError("vNext CLI status field inventory drift")

    legacy = profile.get("legacy")
    if legacy != {
        "kql_reinterpreted": False,
        "pomv_scalar_reinterpreted": False,
        "status_reinterpreted": False,
    }:
        raise ContractError("vNext CLI legacy isolation drift")

    return len(commands)


def validate_vnext_desktop_web_ux_profile(
    profile: dict[str, object] | None = None,
) -> int:
    if profile is None:
        try:
            profile = json.loads(read(VNEXT_DESKTOP_WEB_UX_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(
                f"invalid vNext Desktop/Web UX profile JSON: {error}"
            ) from error

    if profile.get("format") != "onebrain/vnext-desktop-web-ux-profile/1":
        raise ContractError("unexpected vNext Desktop/Web UX profile format")
    if profile.get("profile_id") != "VNEXT_DESKTOP_WEB_UX_PROFILE_V1":
        raise ContractError("unexpected vNext Desktop/Web UX profile ID")
    if profile.get("version") != 1:
        raise ContractError("unexpected vNext Desktop/Web UX profile version")

    discovery = profile.get("discovery")
    expected_match_fields = {
        "responder_scope",
        "selector_cid",
        "assessed_frontier",
        "limitations",
        "continuation",
    }
    if (
        not isinstance(discovery, dict)
        or discovery.get("local_kql_contacts_peers") is not False
        or discovery.get("one_hop_is_separate_surface") is not True
        or discovery.get("one_hop_scope") != "one_hop"
        or discovery.get("zero_result_claims_global_absence") is not False
        or discovery.get("match_label") != "quarantined proposal"
        or discovery.get("match_executable") is not False
        or set(discovery.get("required_match_fields", [])) != expected_match_fields
    ):
        raise ContractError("vNext Desktop/Web discovery firewall drift")

    pomv = profile.get("pomv")
    expected_view_fields = {
        "policy_cid",
        "assessed_frontier",
        "revision",
        "conflicts",
        "coverage",
        "limitations",
    }
    if (
        not isinstance(pomv, dict)
        or pomv.get("legacy_scalar_is_separate") is not True
        or pomv.get("view_load_creates_use_evidence") is not False
        or pomv.get("publication_lookup_creates_use_evidence") is not False
        or pomv.get("conflict_displays_authorized") is not False
        or set(pomv.get("required_view_fields", [])) != expected_view_fields
    ):
        raise ContractError("vNext Desktop/Web PoMV firewall drift")

    public_use = profile.get("public_use")
    expected_preview = {
        "canonical_payload_preview",
        "exact_target",
        "exact_recipient",
        "selector_cid",
        "namespace",
        "disclosure",
        "intent_cid",
        "idempotency_key",
        "expires_at",
    }
    if (
        not isinstance(public_use, dict)
        or public_use.get("prepare_creates_use_evidence") is not False
        or public_use.get("public_permanent_acknowledgement_required") is not True
        or public_use.get("exact_typed_intent_required") is not True
        or public_use.get("receipt_derived_after_exact_match") is not True
        or public_use.get("receipt_exported_to_ui") is not False
        or set(public_use.get("required_preview_fields", [])) != expected_preview
        or set(public_use.get("publication_states_visible", []))
        != {"pending", "deferred"}
        or public_use.get("delivery_acknowledgement_inferred") is not False
    ):
        raise ContractError("vNext Desktop/Web Public Use firewall drift")

    expected_status = {
        "compiled",
        "requested",
        "active",
        "kill_switch",
        "signer_ready",
        "lifecycle",
        "coverage",
        "limitations",
    }
    if set(profile.get("settings_fields", [])) != expected_status:
        raise ContractError("vNext Desktop/Web status field inventory drift")

    if profile.get("desktop") != {
        "quit_graceful_shutdown": True,
        "restart_graceful_shutdown": True,
        "restart_rebuilds_process": True,
        "tray_quit_bypasses_shutdown": False,
    }:
        raise ContractError("vNext Desktop lifecycle firewall drift")

    compatibility = profile.get("compatibility")
    if (
        not isinstance(compatibility, dict)
        or any(value is not False for value in compatibility.values())
        or set(compatibility)
        != {
            "legacy_kql_reinterpreted",
            "legacy_pomv_reinterpreted",
            "private_websocket_cross_client_delivery",
            "api_cli_replay_identity_changed",
        }
    ):
        raise ContractError("vNext Desktop/Web compatibility firewall drift")

    vectors = profile.get("receipt_vectors")
    if (
        not isinstance(vectors, list)
        or len(vectors) != 2
        or any(
            not isinstance(row, dict)
            or not re.fullmatch(r"[0-9a-f]{64}", str(row.get("intent_cid", "")))
            or not re.fullmatch(
                r"obc1\.[A-Za-z0-9_-]{43}", str(row.get("receipt", ""))
            )
            for row in vectors
        )
    ):
        raise ContractError("vNext Desktop/Web receipt vector drift")

    source_contract = {
        "src/onebrain-web/src/pages/Discovery.tsx": (
            "Local KQL",
            "One-hop discovery",
        ),
        "src/onebrain-web/src/pages/OneHopDiscovery.tsx": (
            "quarantined proposal",
            "Responder scope",
            "Assessed frontier",
            "Coverage outside this assessed",
        ),
        "src/onebrain-web/src/pages/Pomv.tsx": (
            "Legacy local scalar",
            "vNext Evidence View / Public Use",
        ),
        "src/onebrain-web/src/pages/VNextPomv.tsx": (
            "Exact canonical payload bytes",
            "Type the exact intent CID to confirm",
            "outbox /",
            "UNRESOLVED CONFLICT — not Authorized",
        ),
        "src/onebrain-web/src/api/client.ts": (
            '"single_use_receipt":"[REDACTED]"',
        ),
        "src/onebrain-web/src/pages/Settings.tsx": (
            "Compiled",
            "Requested",
            "Active",
            "Kill switch",
            "Signer ready",
        ),
        "src/onebrain-desktop/src/commands.rs": (
            "shutdown_network().await",
            "shutdown_node(state.node.get().cloned()).await;\n    app.restart()",
            "shutdown_node(state.node.get().cloned()).await;\n    app.exit(0)",
        ),
        "src/onebrain-desktop/src/tray.rs": (
            "crate::commands::shutdown_node(node).await;\n                    app.exit(0)",
        ),
    }
    for relative, needles in source_contract.items():
        text = read(ROOT / relative)
        for needle in needles:
            if needle not in text:
                raise ContractError(
                    f"vNext Desktop/Web implementation evidence missing: "
                    f"{relative}: {needle}"
                )

    return len(vectors)


def validate_markdown_links() -> int:
    files = sorted(VNEXT.rglob("*.md")) + [PLAN]
    checked = 0
    for markdown in files:
        text = read(markdown)
        if text.count("```") % 2:
            raise ContractError(f"unbalanced code fence in {markdown.relative_to(ROOT)}")
        for raw_target in MARKDOWN_LINK.findall(text):
            target = raw_target.strip().strip("<>").split("#", 1)[0]
            if not target or re.match(r"^(?:https?://|mailto:)", target):
                continue
            path = (markdown.parent / target).resolve()
            if not path.exists():
                raise ContractError(
                    f"broken link in {markdown.relative_to(ROOT)}: {raw_target}"
                )
            checked += 1
    return checked


def validate_normative_coverage() -> int:
    try:
        manifest = json.loads(read(NORMATIVE_COVERAGE))
    except json.JSONDecodeError as error:
        raise ContractError(f"invalid normative coverage JSON: {error}") from error
    if manifest.get("format") != "onebrain/normative-coverage/1":
        raise ContractError("unexpected normative coverage format")

    actual: dict[str, int] = {}
    for markdown in sorted(VNEXT.rglob("*.md")):
        count = sum(
            1 for line in read(markdown).splitlines() if NORMATIVE_KEYWORD.search(line)
        )
        if count:
            actual[markdown.relative_to(ROOT).as_posix()] = count

    rows = manifest.get("files")
    if not isinstance(rows, list) or not rows:
        raise ContractError("normative coverage files must be a non-empty list")
    declared: dict[str, int] = {}
    for row in rows:
        if not isinstance(row, dict):
            raise ContractError("invalid normative coverage row")
        path = row.get("path")
        expected = row.get("expected_statement_lines")
        evidence = row.get("evidence")
        rationale = row.get("rationale")
        if not isinstance(path, str) or not isinstance(expected, int) or expected <= 0:
            raise ContractError("invalid normative coverage path/count")
        if path in declared:
            raise ContractError(f"duplicate normative coverage path: {path}")
        if not isinstance(rationale, str) or not rationale.strip():
            raise ContractError(f"normative coverage lacks rationale: {path}")
        if not isinstance(evidence, list) or not evidence:
            raise ContractError(f"normative coverage lacks evidence: {path}")
        for item in evidence:
            if not isinstance(item, dict):
                raise ContractError(f"invalid normative evidence row: {path}")
            evidence_path = item.get("path")
            needle = item.get("needle")
            if not isinstance(evidence_path, str) or not isinstance(needle, str) or not needle:
                raise ContractError(f"invalid normative evidence reference: {path}")
            target = ROOT / evidence_path
            if not target.is_file():
                raise ContractError(f"missing normative evidence file: {evidence_path}")
            if needle not in read(target):
                raise ContractError(
                    f"normative evidence needle missing in {evidence_path}: {needle}"
                )
        declared[path] = expected

    if set(actual) != set(declared):
        missing = sorted(set(actual) - set(declared))
        stale = sorted(set(declared) - set(actual))
        raise ContractError(
            f"normative coverage file mismatch; missing={missing}, stale={stale}"
        )
    drift = sorted(
        (path, actual[path], declared[path])
        for path in actual
        if actual[path] != declared[path]
    )
    if drift:
        raise ContractError(f"normative statement coverage count drift: {drift}")
    return sum(actual.values())


def main() -> int:
    try:
        tasks, _ = plan_tasks()
        adrs = validate_traceability(tasks)
        assertions = validate_negative_assertions()
        vector_count, domains, schema_vectors, event_vectors = validate_vectors()
        product_endpoints, product_dtos = validate_product_integration_profile()
        ws_events, ws_topics = validate_private_websocket_profile()
        cli_commands = validate_vnext_cli_profile()
        ux_receipt_vectors = validate_vnext_desktop_web_ux_profile()
        links = validate_markdown_links()
        normative_lines = validate_normative_coverage()
    except ContractError as error:
        print(f"vNext contract validation failed: {error}", file=sys.stderr)
        return 1

    print(
        "vNext contracts OK: "
        f"{len(tasks)} tasks, {adrs} ADRs, {assertions} negative assertions, "
        f"{vector_count} foundation vectors/{domains} domains, "
        f"{schema_vectors} identity-object vectors, "
        f"{event_vectors} feed-event vectors, {normative_lines} normative lines, "
        f"{product_endpoints} product endpoints/{product_dtos} DTOs, "
        f"{ws_events} private-WS events/{ws_topics} topics, "
        f"{cli_commands} vNext CLI commands, "
        f"{ux_receipt_vectors} Desktop/Web receipt vectors, "
        f"{links} local links"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
