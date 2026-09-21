"""Validate the accepted local OBP composition; not runtime or wire evidence."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path

if __package__:
    from .validate_ku_product_contract import KuContractError, validate_value as structural_value
else:
    from validate_ku_product_contract import KuContractError, validate_value as structural_value

ROOT = Path(__file__).resolve().parents[2]
PROFILE = ROOT / "src/test-vectors/vnext/obp-product-orchestration-v1.json"


class ObpContractError(ValueError):
    pass


def require(condition: bool, reason: str) -> None:
    if not condition:
        raise ObpContractError(reason)


def load_profile() -> dict:
    return json.loads(PROFILE.read_text(encoding="utf-8"))


def validate_value(profile: dict, name: str, value: object) -> None:
    # Reuse the existing dependency-free bounded DTO type checker. OBP-specific
    # invariants below never extend the KU or canonical wire schema.
    try:
        structural_value(profile, name, value)
    except KuContractError as error:
        raise ObpContractError(str(error)) from error
    if name == "ObpStatusV1":
        if value["active"]:
            require(value["compiled"] and value["requested"] and value["signer_ready"] and not value["kill_switch"], "active dependency/generation gate")
            require(value["lifecycle"] in ("active", "degraded"), "active lifecycle gate")
        if value["lifecycle"] == "active":
            require(value["active"], "lifecycle cannot invent active runtime")
        if value["kill_switch"] or not value["requested"] or not value["compiled"]:
            require(not value["active"] and value["lifecycle"] == "disabled", "disabled lifecycle gate")
        require(value["usable_reservations"] <= profile["core_limits"]["relay_reservations_max"], "reservation ceiling")
    if name == "ObpRouteV1":
        authentication = {"authenticated_peer", "path_kind", "route_receipt_digest"}
        if value["state"] == "connected":
            require(authentication <= value.keys(), "connected needs authenticated receipt")
            require(value["authenticated_peer"] == value["expected_peer"], "expected peer mismatch")
            require("failure" not in value, "connected cannot carry terminal failure")
        else:
            require(not (authentication & value.keys()), "carrier is not peer authentication")
        if value["state"] == "path_limited":
            require("failure" in value and bool(value["limitations"]), "path limited needs finite scope")
    if name == "ObpIntentV1":
        ack = {"acknowledged_sequence", "checkpoint_digest"}
        if value["state"] == "acknowledged":
            require(ack <= value.keys() and value["acknowledged_sequence"] > 0, "acknowledgement needs durable checkpoint")
        else:
            require(not (ack & value.keys()), "pending/failed intent cannot claim acknowledgement")


def validate_contract(profile: dict | None = None) -> tuple[int, int, int]:
    p = load_profile() if profile is None else profile
    require(p["format"] == "onebrain/obp-product-orchestration/1", "profile version")
    require(p["status"] == "accepted-local-implementation" and p["runtime_implementation_authorized"] is True and p.get("owner_decision") == "D-025", "owner review gate")
    require(p["wire_changes"] is False and p["new_rest_routes"] == [] and p["new_ws_events"] == [], "wire/transport review gate")
    require(p["defaults"] == {"outbound_first_requested": False, "advertise_reachability": False}, "default-off gate")
    require(p["limits"] == {"payload_bytes":1048576, "page_items":256, "continuation_characters":2048, "worker_ceiling":8}, "product resource bounds")
    require(len(p["frozen_inputs"]) == 7, "frozen authority inventory")
    for path, expected in p["frozen_inputs"].items():
        target = (ROOT / path).resolve()
        require(target.is_relative_to(ROOT), "frozen path escape")
        actual = hashlib.sha256(target.read_text(encoding="utf-8").replace("\r\n", "\n").encode()).hexdigest()
        require(actual == expected, f"frozen authority changed: {path}")
    wire = json.loads((ROOT / "src/test-vectors/vnext/outbound-first-reachability-v1.json").read_text())
    require(p["core_limits"] == wire["limits"], "frozen core resource bounds")
    require(p["types"]["PathKind"]["values"] == wire["path_kinds"], "frozen path classes")
    require(p["types"]["RouteFailure"]["values"] == wire["failure_kinds"], "frozen failures")
    require(p["types"]["False"] == {"kind":"literal", "value":False}, "authority firewall")
    require(p["types"]["IntentState"]["values"] == ["pending","acknowledged","dead_letter","retry_exhausted"], "existing outbox states")
    require(p["types"]["Lifecycle"]["values"] == ["disabled","requested","active","degraded"], "existing lifecycle")
    forbidden = {"authorized", "authority_frontier", "private_key", "endpoint", "config_path", "canonical_bytes", "candidate_attempts", "claims_delivery"}
    for name, dto in p["dtos"].items():
        require(set(dto) == {"required","optional"}, "closed DTO declaration")
        fields = dto["required"] | dto["optional"]
        require(not forbidden.intersection(fields), "redacted authority boundary")
        require(not (dto["required"].keys() & dto["optional"].keys()), "ambiguous optional field")
        for field_type in fields.values():
            require(field_type in p["types"] or field_type in p["dtos"], "unknown DTO field type")
        if name in ("ObpStatusV1","ObpRouteV1","ObpIntentV1","ObpSourcePageV1","ObpReservationPageV1"):
            require(all(dto["required"].get(k) == "False" for k in ("claims_global_completion","authorizes_reward")), "required authority firewall")
    expected = {"status","configure","source_admit","source_set_enabled","refresh","source_list","reservation_list","route_request","route_status","intent_status","intent_retry","network_kill","network_reenable"}
    require(len(p["operations"]) == len(expected) and {o["name"] for o in p["operations"]} == expected, "finite operation inventory")
    management = {"configure","source_admit","source_set_enabled","network_kill","network_reenable"}
    for op in p["operations"]:
        require(op["request"] in p["dtos"] and op["response"] in p["dtos"], "operation DTO reference")
        require(op["access"] in ("host_management","product_read","product_control"), "operation access class")
        require((op["name"] in management) == (op["access"] == "host_management"), "management capability boundary")
        if op["effect"] != "none":
            fields = p["dtos"][op["request"]]["required"]
            require(fields.get("idempotency_key") == "IdempotencyKey" and fields.get("expected_generation") == "Generation", "mutation replay/generation binding")
    require(len(p["fixtures"]) >= 26, "required contract fixtures")
    require(len({f["name"] for f in p["fixtures"]}) == len(p["fixtures"]), "duplicate fixture names")
    for fixture in p["fixtures"]:
        try:
            validate_value(p, fixture["dto"], fixture["value"])
        except ObpContractError:
            require(fixture["valid"] is False, f"valid fixture rejected: {fixture['name']}")
        else:
            require(fixture["valid"] is True, f"invalid fixture accepted: {fixture['name']}")
    return len(p["operations"]), len(p["dtos"]), len(p["fixtures"])


if __name__ == "__main__":
    operations, dtos, fixtures = validate_contract()
    print(f"OBP product contract OK: {operations} operations/{dtos} DTOs/{fixtures} fixtures; D-025 local implementation accepted")
