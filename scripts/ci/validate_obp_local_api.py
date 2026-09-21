"""Offline transport proposal checks; no handler or runtime conformance claim."""
from __future__ import annotations

import hashlib
import json
import math
from pathlib import Path

if __package__:
    from .validate_obp_product_contract import load_profile as load_parent, validate_value
    from .validate_ku_product_contract import KuContractError
else:
    from validate_obp_product_contract import load_profile as load_parent, validate_value
    from validate_ku_product_contract import KuContractError

ROOT = Path(__file__).resolve().parents[2]
PROFILE = ROOT / "src/test-vectors/vnext/obp-local-api-v1.json"


class ObpApiContractError(ValueError):
    pass


def require(ok, reason):
    if not ok:
        raise ObpApiContractError(reason)


def load_profile():
    return json.loads(PROFILE.read_text(encoding="utf-8"))


def exact(value, fields):
    require(isinstance(value, dict) and set(value) == set(fields), "closed object fields")


def typed(parent, name, value):
    try:
        validate_value(parent, name, value)
    except (ValueError, KuContractError) as error:
        raise ObpApiContractError(str(error)) from error


def strict_json(raw: bytes, limits=None):
    limits = limits or load_profile()["limits"]
    require(len(raw) <= limits["payload_bytes"], "request byte ceiling")

    def pairs(items):
        value = {}
        for key, item in items:
            require(key not in value, "duplicate JSON key")
            value[key] = item
        return value

    def reject_constant(_):
        raise ObpApiContractError("nonfinite JSON number")

    try:
        value = json.loads(raw.decode("utf-8"), object_pairs_hook=pairs,
                           parse_constant=reject_constant)
    except (UnicodeError, ValueError, RecursionError) as error:
        raise ObpApiContractError("invalid bounded JSON") from error

    def walk(item, depth):
        require(depth <= limits["json_depth"], "JSON nesting ceiling")
        require(not isinstance(item, float) or math.isfinite(item), "nonfinite JSON number")
        if isinstance(item, dict):
            for nested in item.values():
                walk(nested, depth + 1)
        elif isinstance(item, list):
            for nested in item:
                walk(nested, depth + 1)
    walk(value, 0)
    require(isinstance(value, dict), "request object required")
    return value


def validate_example(p, kind, value):
    parent = load_parent()
    if kind in {"query", "commands", "reconcile", "tickets"}:
        fields = {"session", "operation", "payload"} if kind in {"query", "commands"} else (
            {"session", "idempotency_key"} if kind == "reconcile" else {"session", "subscriptions"})
        exact(value, fields)
        exact(value["session"], {"process_generation", "dataset_generation"})
        for fence in value["session"].values():
            typed(parent, "InputRef", fence)
        if kind == "reconcile":
            typed(parent, "IdempotencyKey", value["idempotency_key"])
        elif kind == "tickets":
            require(value["subscriptions"] == ["network"], "immutable OBP ticket topic")
        else:
            candidates = [o for o in p["operations"] if o["name"] == value["operation"]
                          and o["route_class"] == kind]
            require(len(candidates) == 1, "operation route binding")
            typed(parent, candidates[0]["request"], value["payload"])
    elif kind == "outcome":
        require(isinstance(value, dict), "operation outcome object")
        required = {"idempotency_key", "operation", "state", "reconcile_before_retry"}
        require(required <= value.keys() <= required | {"result", "failure"}, "outcome fields")
        typed(parent, "IdempotencyKey", value["idempotency_key"])
        operations = [o for o in p["operations"] if o["name"] == value["operation"]
                      and o["route_class"] == "commands"]
        require(len(operations) == 1, "outcome command identity")
        state = value["state"]
        require(state in p["operation_states"], "outcome state")
        require(value["reconcile_before_retry"] is (state in {"admitted", "reconcile_required"}),
                "unknown effect requires reconciliation")
        require(("result" in value) == (state == "completed"), "completed result binding")
        require(("failure" in value) == (state == "failed_no_effect"), "failed effect binding")
        if "result" in value:
            typed(parent, operations[0]["response"], value["result"])
        if "failure" in value:
            error = value["failure"]
            require(error in p["errors"] and error["outcome"] == "not_admitted",
                    "no-effect failure cannot hide unknown effects")
    elif kind == "event":
        exact(value, {"profile", "event_type", "sequence", "timestamp", "lifecycle",
                      "coverage", "limitations", "data"})
        require(value["profile"] == p["ws"]["profile"], "WS profile isolation")
        require(value["event_type"] in p["ws"]["events"], "WS event vocabulary")
        typed(parent, "Generation", value["sequence"])
        for field, name in [("timestamp", "Count"), ("lifecycle", "Lifecycle"),
                            ("coverage", "Coverage"), ("limitations", "Limitations")]:
            typed(parent, name, value[field])
        data = value["data"]
        if value["event_type"] == "subscription_ready":
            exact(data, {"subscriptions", "session_expires_at"})
            require(data["subscriptions"] == ["network"], "ready topic isolation")
            typed(parent, "Count", data["session_expires_at"])
        else:
            exact(data, p["ws"]["network_fields"])
            for field, name in p["ws"]["network_fields"].items():
                typed(parent, name, data[field])
            # Reuse the accepted status invariants; generation is not a WS field.
            typed(parent, "ObpStatusV1", dict(data, generation=1,
                  lifecycle=value["lifecycle"], coverage=value["coverage"],
                  limitations=value["limitations"]))
    else:
        raise ObpApiContractError("unknown fixture kind")


def validate_contract(p=None):
    p = load_profile() if p is None else p
    require(p["format"] == "onebrain/obp-local-api/1", "transport version")
    require(p["status"] == "accepted-local-implementation"
            and p["handlers_registered"] is True and p["owner_acceptance"] == "D-029",
            "accepted transport and implementation registration gate")
    require(p["parent"] == "onebrain/obp-product-orchestration/1", "parent profile")
    frozen = {
        "docs/specs/vnext/OBP_PRODUCT_ORCHESTRATION_PROFILE_V1.md",
        "src/test-vectors/vnext/obp-product-orchestration-v1.json",
        "docs/specs/vnext/VNEXT_REST_API_PROFILE_V1.md",
        "docs/specs/vnext/VNEXT_PRIVATE_WEBSOCKET_PROFILE_V1.md",
        "src/test-vectors/vnext/private-websocket-profile-v1.json",
    }
    require(set(p["frozen_inputs"]) == frozen, "frozen parent inventory")
    for name, digest in p["frozen_inputs"].items():
        actual = hashlib.sha256((ROOT / name).read_text(encoding="utf-8").encode()).hexdigest()
        require(actual == digest, f"parent drift: {name}")
    expected_routes = [
        ("GET", "status", "product_read"), ("POST", "query", "product_read"),
        ("POST", "commands", "operation_access"), ("POST", "reconcile", "product_read"),
        ("POST", "ws/tickets", "product_read"),
        ("GET", "ws", "single_use_endpoint_bound_ticket"),
    ]
    require(p["routes"] == [dict(method=m, path="/api/vnext/obp/" + n, access=a)
                             for m, n, a in expected_routes], "exact route/auth inventory")
    parent = load_parent()
    reads = {"source_list", "reservation_list", "route_status", "intent_status"}
    expected_operations = [dict(o, route_class="status" if o["name"] == "status" else
                               "query" if o["name"] in reads else "commands")
                           for o in parent["operations"]]
    require(p["operations"] == expected_operations, "accepted operation/DTO/access binding")
    require(p["limits"] == dict(payload_bytes=1048576, json_depth=16, page_items=256,
        continuation_characters=2048, operation_records=4096, operation_bytes=16777216,
        concurrent_commands=4, management_grants=32, management_ttl_seconds=300,
        input_refs=8, input_bytes=1048576, input_ttl_seconds=300,
        snapshots=32, snapshot_bytes=1048576, snapshot_ttl_seconds=60), "bounded transport")
    require(p["authentication"] == dict(bearer="existing_constant_time_local",
        principal="host_installed", control_grant="host_installed_revocable",
        management_header="X-OneBrain-OBP-Management", management_token_prefix="obm1.",
        management_token_bytes=32, management_mint_route=False, default_control_grant=False,
        host_intake="in_process_typed_reference_only", raw_upload_route=False),
        "host authority/intake separation")
    require(p["operation_states"] == ["admitted", "completed", "failed_no_effect",
                                      "reconcile_required"], "command outcome vocabulary")
    ws = p["ws"]
    require(ws["profile"] == "OBP_PRIVATE_WEBSOCKET_PROFILE_V1"
            and ws["topics"] == ["network"]
            and ws["events"] == ["subscription_ready", "network_state"], "separate WS vocabulary")
    require(ws["shared_hub_limits"] == dict(pending_tickets=128, active_sessions=64,
        queue_frames=32, client_frame_bytes=4096, ticket_ttl_seconds=30,
        session_ttl_seconds=900, token_bytes=32), "shared bounded hub")
    require(ws["ticket_endpoint_bound"] is True and ws["broadcast"] is False
            and ws["autonomous_polling"] is False, "targeted no-side-effect notification")
    require(ws["network_fields"] == {k: v for k, v in
        parent["dtos"]["ObpStatusV1"]["required"].items()
        if k not in {"generation", "lifecycle", "coverage", "limitations"}}, "WS redaction allowlist")
    expected_errors = {
        "invalid_payload": ("invalid_request", 400), "local_not_found": ("not_found", 404),
        "generation_conflict": ("conflict", 409), "idempotency_conflict": ("conflict", 409),
        "session_conflict": ("conflict", 409), "input_expired": ("expired", 410),
        "snapshot_expired": ("expired", 410), "admission_limit": ("rate_limited", 429),
        "feature_disabled": ("capability_disabled", 503),
        "dependency_unavailable": ("dependency_unavailable", 503),
        "outcome_unknown": ("dependency_unavailable", 503),
        "storage_corrupt": ("internal_error", 500), "response_overflow": ("internal_error", 500),
    }
    require(len(p["errors"]) == len(expected_errors) and
            {e["reason"] for e in p["errors"]} == set(expected_errors), "failure inventory")
    for error in p["errors"]:
        unknown = error["reason"] in {"outcome_unknown", "storage_corrupt", "response_overflow"}
        code, http = expected_errors[error["reason"]]
        require(error == dict(reason=error["reason"], code=code, http=http,
            outcome="unknown" if unknown else "not_admitted", reconcile_before_retry=unknown,
            retryable=code in {"rate_limited", "dependency_unavailable"}), "failure/retry binding")
    require(len(p["fixtures"]) >= 26 and len({f["name"] for f in p["fixtures"]}) ==
            len(p["fixtures"]), "fixture inventory")
    for fixture in p["fixtures"]:
        try:
            value = strict_json(json.dumps(fixture["value"]).encode(), p["limits"])
            validate_example(p, fixture["kind"], value)
        except ObpApiContractError:
            require(fixture["valid"] is False, "valid fixture rejected: " + fixture["name"])
        else:
            require(fixture["valid"] is True, "invalid fixture accepted: " + fixture["name"])
    return len(p["routes"]), len(p["operations"]), len(p["fixtures"])


if __name__ == "__main__":
    routes, operations, fixtures = validate_contract()
    print(f"OBP API proposal OK: {routes} routes/{operations} operations/{fixtures} fixtures; "
          "D-029 accepted, handlers registered; runtime evidence is separate")
