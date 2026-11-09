"""DR-M5 (M5-00..M5-07) and soak-runner kit checks split from validate_vnext_contracts.py (ARCH-13)."""

from __future__ import annotations

import hashlib
import json
import re

if __package__:
    from .vnext_contract_common import (
        ROOT,
        VNEXT,
        ContractError,
        read,
        DR_M5_TRANSACTION_INVENTORY,
        VNEXT_FOUNDATION_WORKFLOW,
    )
else:
    from vnext_contract_common import (
        ROOT,
        VNEXT,
        ContractError,
        read,
        DR_M5_TRANSACTION_INVENTORY,
        VNEXT_FOUNDATION_WORKFLOW,
    )


DR_M5_BASELINE_PROFILE = ROOT / "src/test-vectors/vnext/dr-m5-baseline-v1.json"
DR_M5_RESOURCE_PROFILE = (
    ROOT / "src/test-vectors/vnext/dr-m5-resource-admission-v1.json"
)
DR_M5_OBSERVABILITY_PROFILE = (
    ROOT / "src/test-vectors/vnext/dr-m5-observability-v1.json"
)
DR_M5_CRASH_HARNESS_PROFILE = (
    ROOT / "src/test-vectors/vnext/dr-m5-crash-harness-v1.json"
)
DR_M5_CHAOS_FUZZ_PROFILE = (
    ROOT / "src/test-vectors/vnext/dr-m5-chaos-fuzz-v1.json"
)
DR_M5_OPERATIONAL_COMPACTION_PROFILE = (
    ROOT / "src/test-vectors/vnext/dr-m5-operational-compaction-v1.json"
)
DR_M5_MIXED_ROLLBACK_PROFILE = (
    ROOT / "src/test-vectors/vnext/dr-m5-mixed-rollback-v1.json"
)
DR_M5_SOAK_RELEASE_PROFILE = (
    ROOT / "src/test-vectors/vnext/dr-m5-soak-release-v1.json"
)
VNEXT_SOAK_WORKFLOW = ROOT / ".github/workflows/vnext-soak.yml"
VNEXT_MACOS_SOAK_WORKFLOW = (
    ROOT / ".github/workflows/vnext-soak-macos-arm64.yml"
)
VNEXT_SOAK_RUNNER_SCRIPT = ROOT / "scripts/runner/onebrain-soak-runner.sh"
VNEXT_SOAK_RUNNER_GUIDE = (
    ROOT / "docs/operations/ONEBRAIN_SOAK_RUNNER_GUIDE_V1.md"
)
VNEXT_MACOS_SOAK_RUNNER_GUIDE = (
    ROOT / "docs/operations/ONEBRAIN_SOAK_RUNNER_MAC_M2_GUIDE_V1.md"
)


def validate_vnext_dr_m5_baseline(
    profile: dict[str, object] | None = None,
) -> tuple[int, int]:
    if profile is None:
        try:
            profile = json.loads(read(DR_M5_BASELINE_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid DR-M5 baseline JSON: {error}") from error

    if (
        profile.get("format") != "onebrain/dr-m5-baseline/1"
        or profile.get("profile_id")
        != "DISTRIBUTED_RUNTIME_HARDENING_BASELINE_V1"
        or profile.get("version") != 1
    ):
        raise ContractError("unexpected DR-M5 baseline profile")

    if profile.get("runtime_change_globs") != ["src/**"]:
        raise ContractError("DR-M5 runtime path trigger drift")

    expected_steps = {
        "Compile product entrypoints with the runtime feature",
        "M2 authenticated OBP-RP over real QUIC",
        "M3 private-vault-rehydrated distributed KQL over real peers",
        "M4 strong Public Use consent and metabolic view over real peers",
        "P1.5 authenticated route and local authority boundary",
        "P2.1 node-owned product runtime aggregate",
        "P2.2 independent product lanes and hard budgets",
        "P2.3 ordered lifecycle and partial-start rollback",
        "P2.4 durable selector/type incremental processing",
        "P2.5 cloneable service handles and concurrency fence",
        "P3.1 authenticated REST API contract and real-runtime flow",
        "P3.2 private WebSocket isolation and backpressure",
        "P3.3 authenticated CLI contract and real-runtime replay",
        "Node-owned real QUIC lifecycle",
    }
    gate = profile.get("real_quic_gate")
    required_steps = gate.get("required_steps") if isinstance(gate, dict) else None
    if (
        not isinstance(gate, dict)
        or gate.get("workflow_path") != ".github/workflows/vnext-foundation.yml"
        or gate.get("job_id") != "vnext-network-runtime"
        or not isinstance(gate.get("timeout_minutes"), int)
        or not 0 < gate["timeout_minutes"] <= 45
        or not isinstance(required_steps, list)
        or any(not isinstance(step, str) for step in required_steps)
        or set(required_steps) != expected_steps
    ):
        raise ContractError("DR-M5 real-QUIC gate drift")

    expected_phases = [
        "before_begin_write",
        "after_begin_write_before_mutation",
        "after_mutation_before_commit",
        "after_commit_before_next_side_effect",
        "after_next_side_effect_before_ack",
    ]
    if profile.get("failpoint_phases") != expected_phases:
        raise ContractError("DR-M5 failpoint phase vocabulary drift")

    expected_fields = {
        "accepted_object_cids",
        "accepted_event_cids",
        "selector_inventory_roots",
        "reconciliation_journals",
        "pending_outbox",
        "authority_decisions",
        "private_need_records",
        "distributed_kql_matches",
        "prepared_public_use",
        "public_use_publications",
        "metabolic_views",
    }
    oracle = profile.get("invariant_oracle")
    oracle_fields = oracle.get("fields") if isinstance(oracle, dict) else None
    if (
        not isinstance(oracle, dict)
        or oracle.get("format") != "onebrain/dr-m5-oracle/1"
        or oracle.get("canonicalization")
        != "json-sort-keys-no-whitespace-utf8"
        or oracle.get("digest_algorithm") != "sha256"
        or not isinstance(oracle_fields, list)
        or any(not isinstance(field, str) for field in oracle_fields)
        or set(oracle_fields) != expected_fields
    ):
        raise ContractError("DR-M5 invariant oracle drift")

    expected_boundaries = {
        "TX-PUSE-000",
        "TX-PUSE-001",
        "TX-PUSE-002",
        "TX-OUT-001",
        "TX-OUT-002",
        "TX-JRN-001",
        "TX-VAL-001",
        "TX-INV-001",
        "TX-AUTH-001",
        "TX-KQL-000",
        "TX-KQL-001",
        "TX-POMV-001",
        "TX-POMV-002",
    }
    boundaries = profile.get("transaction_boundaries")
    if not isinstance(boundaries, list):
        raise ContractError("DR-M5 transaction boundary inventory missing")
    boundary_ids: list[str] = []
    covered_oracle_fields: set[str] = set()
    for boundary in boundaries:
        if not isinstance(boundary, dict):
            raise ContractError("invalid DR-M5 transaction boundary row")
        boundary_id = boundary.get("id")
        owner = boundary.get("durable_owner")
        components = boundary.get("oracle_components")
        if (
            not isinstance(boundary_id, str)
            or not isinstance(owner, str)
            or not owner.strip()
            or not isinstance(components, list)
            or not components
            or any(not isinstance(component, str) for component in components)
            or any(component not in expected_fields for component in components)
        ):
            raise ContractError("invalid DR-M5 transaction boundary contract")
        boundary_ids.append(boundary_id)
        covered_oracle_fields.update(components)
    if (
        set(boundary_ids) != expected_boundaries
        or len(boundary_ids) != len(set(boundary_ids))
    ):
        raise ContractError("DR-M5 transaction boundary ID drift")
    if covered_oracle_fields != expected_fields:
        raise ContractError("DR-M5 transaction boundaries do not cover the oracle")

    specimen = profile.get("empty_oracle_specimen")
    if not isinstance(specimen, dict) or not isinstance(specimen.get("snapshot"), dict):
        raise ContractError("DR-M5 empty oracle specimen missing")
    snapshot = specimen["snapshot"]
    if (
        snapshot.get("format") != "onebrain/dr-m5-oracle/1"
        or snapshot.get("version") != 1
        or set(snapshot) != expected_fields | {"format", "version"}
        or any(snapshot.get(field) != [] for field in expected_fields)
    ):
        raise ContractError("DR-M5 empty oracle snapshot drift")
    canonical = json.dumps(
        snapshot, ensure_ascii=False, separators=(",", ":"), sort_keys=True
    ).encode("utf-8")
    if specimen.get("sha256") != hashlib.sha256(canonical).hexdigest():
        raise ContractError("DR-M5 empty oracle digest drift")

    inventory = read(DR_M5_TRANSACTION_INVENTORY)
    for boundary_id in expected_boundaries:
        if f"| `{boundary_id}` |" not in inventory:
            raise ContractError(
                f"DR-M5 boundary missing from transaction inventory: {boundary_id}"
            )
    for index, phase in enumerate(expected_phases, start=1):
        if f"{index}. `{phase}`" not in inventory:
            raise ContractError(f"DR-M5 failpoint phase missing from inventory: {phase}")

    workflow = read(VNEXT_FOUNDATION_WORKFLOW)
    if workflow.count('- "src/**"') < 2:
        raise ContractError("DR-M5 src/** trigger missing from PR or push workflow")
    job_match = re.search(
        r"(?ms)^  vnext-network-runtime:\s*$"
        r"(?P<body>.*?)(?=^  [A-Za-z0-9_-]+:\s*$|\Z)",
        workflow,
    )
    if not job_match:
        raise ContractError("DR-M5 real-QUIC workflow job missing")
    job = job_match.group("body")
    timeout_match = re.search(r"(?m)^\s+timeout-minutes:\s*(\d+)\s*$", job)
    if (
        not timeout_match
        or int(timeout_match.group(1)) != gate["timeout_minutes"]
        or any(f"- name: {step}" not in job for step in expected_steps)
    ):
        raise ContractError("DR-M5 real-QUIC workflow contract drift")
    if "python -m unittest scripts.ci.test_validate_vnext_dr_m5_baseline" not in workflow:
        raise ContractError("DR-M5 baseline mutation tests missing from CI")

    return len(boundary_ids), len(expected_fields)


def validate_vnext_dr_m5_resource_admission(
    profile: dict[str, object] | None = None,
) -> tuple[int, int, int]:
    if profile is None:
        try:
            profile = json.loads(read(DR_M5_RESOURCE_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid M5-01 resource profile JSON: {error}") from error

    if (
        profile.get("format") != "onebrain/dr-m5-resource-admission/1"
        or profile.get("profile_id") != "UNIFIED_RESOURCE_ADMISSION_AND_FAIRNESS_V1"
        or profile.get("version") != 1
        or profile.get("claims_network_completion") is not False
    ):
        raise ContractError("unexpected M5-01 resource admission profile")

    expected_pipeline = [
        "stream_read",
        "frame",
        "protocol",
        "journal",
        "application",
    ]
    if profile.get("pipeline") != expected_pipeline:
        raise ContractError("M5-01 ordered admission pipeline drift")

    expected_lanes = [
        {
            "id": "session_control",
            "max_bytes": 262_144,
            "prefix_checked_before_allocation": False,
        },
        {
            "id": "carrier_frame",
            "max_bytes": 4_194_304,
            "prefix_checked_before_allocation": True,
        },
        {
            "id": "protocol_payload",
            "max_bytes": 1_048_576,
            "prefix_checked_before_allocation": False,
        },
    ]
    if profile.get("allocation_lanes") != expected_lanes:
        raise ContractError("M5-01 allocation lane contract drift")

    expected_quotas = {
        "handshakes_global": 128,
        "handshakes_per_ip": 8,
        "sessions_global": 64,
        "sessions_per_ip": 8,
        "sessions_per_node_id": 4,
        "contexts_per_session": 64,
        "replay_entries": 65_536,
        "rate_window_seconds": 60,
        "records_per_session": 4_096,
        "bytes_per_session": 16_777_216,
        "work_per_session": 1_000_000,
        "records_per_node_id_window": 8_192,
        "bytes_per_node_id_window": 16_777_216,
        "work_per_node_id_window": 2_000_000,
        "per_ip_window_derivation": "per_node_id_window_times_sessions_per_ip",
        "global_window_derivation": "per_node_id_window_times_sessions_global",
    }
    if profile.get("default_quotas") != expected_quotas:
        raise ContractError("M5-01 identity or rate quota drift")

    expected_state_bounds = {
        "proposal_quarantine_records": 65_536,
        "accepted_records": 65_536,
        "verified_quarantine_records": 65_536,
        "inventory_records": 65_536,
        "provenance_observations": 262_144,
        "typed_provenance_records": 65_536,
        "typed_provenance_prefixes": 65_536,
        "source_peers_per_record": 64,
        "distributed_kql_matches": 65_536,
        "outbox_records": 65_536,
        "outbox_tombstones": 65_536,
        "storage_soft_watermark_bytes": 536_870_912,
        "storage_hard_watermark_bytes": 1_073_741_824,
    }
    if profile.get("bounded_durable_state") != expected_state_bounds:
        raise ContractError("M5-01 durable-state bound drift")

    scans = profile.get("incremental_scans")
    if scans != {
        "index": "selector_kind_type_prefix",
        "cursor": "monotonic_sequence",
        "max_page_records": 4_096,
        "consumers": ["distributed_kql", "distributed_pomv"],
    }:
        raise ContractError("M5-01 incremental scan contract drift")

    outbox = profile.get("outbox")
    if outbox != {
        "states": ["Pending", "Acknowledged", "DeadLetter", "RetryExhausted"],
        "fair_cursor": "persisted_round_robin",
        "transport_counter": "transport_attempts",
        "validation_counter": "validation_retries",
        "terminal_order": "terminal_sequence",
        "terminal_compaction": "tombstone_before_payload_delete",
        "max_records_inspected_per_quantum": 65_536,
    }:
        raise ContractError("M5-01 fair outbox contract drift")

    expected_oracles = [
        "flood_peer_bounded_with_constant_controller_overhead",
        "healthy_peer_progresses_within_finite_quanta",
        "retry_exhausted_prefix_cannot_starve_pending_work",
    ]
    if profile.get("exit_oracles") != expected_oracles:
        raise ContractError("M5-01 exit oracle drift")

    source_contract = {
        "src/ku-net/src/vnext_resource_gate.rs": (
            "pub fn admit_length_prefix",
            "pub struct RuntimeAdmissionController",
            "pub fn try_begin_handshake",
            "pub fn begin_record",
            "fn rejected_flood_does_not_grow_identity_maps",
        ),
        "src/ku-net/src/transport.rs": ("recv_length_prefixed_uni", "read_exact(&mut prefix)"),
        "src/ku-net/src/vnext_session.rs": (
            "DEFAULT_SESSION_REPLAY_ENTRIES",
            "pub fn with_capacity",
        ),
        "src/onebrain-node/src/vnext_outbox.rs": (
            "DeadLetter",
            "RetryExhausted",
            "pending_fair",
            "transport_attempts",
            "validation_retries",
            "compact_terminal",
            "exhausted_first_page_cannot_starve_healthy_pending_work",
        ),
        "src/onebrain-node/src/vnext_record_provenance.rs": (
            "MAX_TYPED_DELTA_PAGE_RECORDS",
            "MAX_SOURCE_PEERS_PER_RECORD",
            "typed_delta",
        ),
        "src/ku-core/src/foundation/storage.rs": (
            "MAX_VERIFIED_ACCEPTED_RECORDS",
            "MAX_VERIFIED_QUARANTINE_RECORDS",
        ),
    }
    for relative, needles in source_contract.items():
        text = read(ROOT / relative)
        for needle in needles:
            if needle not in text:
                raise ContractError(
                    f"M5-01 implementation evidence missing: {relative}: {needle}"
                )

    spec = read(VNEXT / "UNIFIED_RESOURCE_ADMISSION_AND_FAIRNESS_V1.md")
    if "dr-m5-resource-admission-v1.json" not in spec:
        raise ContractError("M5-01 normative profile is not linked to machine contract")
    workflow = read(VNEXT_FOUNDATION_WORKFLOW)
    if (
        "- name: M5.1 unified resource admission and fair outbox" not in workflow
        or "python -m unittest scripts.ci.test_validate_vnext_dr_m5_resource_admission"
        not in workflow
    ):
        raise ContractError("M5-01 CI acceptance gate missing")

    return len(expected_lanes), len(expected_state_bounds), len(expected_oracles)


def validate_vnext_dr_m5_observability(
    profile: dict[str, object] | None = None,
) -> tuple[int, int, int]:
    if profile is None:
        try:
            profile = json.loads(read(DR_M5_OBSERVABILITY_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid M5-02 observability profile JSON: {error}") from error

    if (
        profile.get("format") != "onebrain/dr-m5-observability/1"
        or profile.get("profile_id") != "STRUCTURED_OBSERVABILITY_PROFILE_V1"
        or profile.get("version") != 1
        or profile.get("profile_major") != 1
        or profile.get("claims_network_completion") is not False
    ):
        raise ContractError("unexpected M5-02 observability profile")

    expected_reasons = [
        "ACCEPTED_NEW",
        "ALREADY_PRESENT",
        "REPLAYED",
        "DEFERRED_MISSING_DEPENDENCY",
        "DEFERRED_BUDGET",
        "QUARANTINED_INVALID",
        "REJECTED_CONTEXT_BINDING",
        "REJECTED_SELECTOR",
        "REJECTED_LENGTH",
        "REJECTED_CONTENT_CID",
        "REJECTED_SINK",
        "REJECTED_AUTHORITY",
        "REJECTED_STORAGE",
        "REJECTED_RATE_LIMIT",
        "REJECTED_REPLAY",
        "REJECTED_SESSION",
        "REJECTED_PROTOCOL",
        "JOURNAL_FAILURE",
        "OUTBOX_RETRY_EXHAUSTED",
        "POMV_IDENTITY_CONFLICT",
        "REGISTRY_FALLBACK",
        "TRANSPORT_FAILURE",
    ]
    if profile.get("reason_codes") != expected_reasons:
        raise ContractError("M5-02 typed reason-code inventory drift")

    expected_outcomes = [
        "accepted_new",
        "already_present",
        "replayed",
        "deferred",
        "quarantined",
        "rejected",
    ]
    if profile.get("outcome_counters") != expected_outcomes:
        raise ContractError("M5-02 outcome counter inventory drift")

    max_u64 = 18_446_744_073_709_551_615
    resources = profile.get("resource_metrics")
    if resources != {
        "counters": ["admitted_bytes", "admitted_work_units", "rate_limited"],
        "record_bytes_inclusive_upper_bounds": [
            64,
            1_024,
            4_096,
            16_384,
            65_536,
            262_144,
            1_048_576,
            max_u64,
        ],
        "work_units_inclusive_upper_bounds": [1, 2, 4, 8, 16, 64, 256, max_u64],
    }:
        raise ContractError("M5-02 resource metric or finite bucket drift")

    expected_gauges = [
        "active_journals",
        "pending_outbox",
        "retry_exhausted_outbox",
        "oldest_pending_outbox_age_seconds",
    ]
    if profile.get("runtime_gauges") != expected_gauges:
        raise ContractError("M5-02 runtime gauge inventory drift")
    if profile.get("journal_age_seconds_inclusive_upper_bounds") != [
        0,
        1,
        5,
        30,
        60,
        300,
        900,
        max_u64,
    ]:
        raise ContractError("M5-02 journal age bucket drift")

    reconciliation = profile.get("reconciliation")
    if reconciliation != {
        "counters": [
            "selector_scans",
            "partial_selector_scans",
            "assessed_frontier_items",
        ],
        "latest_gauge": "latest_lag_records",
        "lag_records_inclusive_upper_bounds": [0, 1, 2, 4, 8, 16, 64, max_u64],
    }:
        raise ContractError("M5-02 reconciliation metric drift")
    if profile.get("pomv") != ["identity_conflicts", "latest_view_revision"]:
        raise ContractError("M5-02 PoMV metric drift")
    if profile.get("registry_states") != [
        "UNKNOWN",
        "DISABLED",
        "LOADED",
        "FALLBACK_V1",
    ]:
        raise ContractError("M5-02 registry-state inventory drift")

    logging = profile.get("structured_logging")
    if logging != {
        "target": "onebrain::vnext::observability",
        "required_fields": ["reason_code", "count", "bytes", "work_units"],
        "free_form_identity_fields": False,
        "swallowed_adversarial_errors": False,
    }:
        raise ContractError("M5-02 structured logging contract drift")

    expected_forbidden_labels = [
        "node_id",
        "peer_id",
        "selector",
        "feed_id",
        "object_cid",
        "event_cid",
        "standing_need_id",
        "private_need",
        "local_query",
    ]
    privacy = profile.get("privacy")
    if privacy != {
        "forbidden_metric_labels": expected_forbidden_labels,
        "contains_high_cardinality_labels": False,
        "contains_private_need_labels": False,
    }:
        raise ContractError("M5-02 privacy label firewall drift")

    if profile.get("operator_snapshot") != {
        "rest_method": "GET",
        "rest_path": "/api/vnext/runtime/status",
        "field": "observability",
        "authenticated_local": True,
        "claims_network_completion": False,
    }:
        raise ContractError("M5-02 operator snapshot contract drift")

    expected_oracles = [
        "every_adversarial_outcome_has_one_typed_reason_transition",
        "exact_counter_transitions_are_reproducible",
        "operator_snapshot_contains_no_private_or_high_cardinality_labels",
        "status_never_claims_network_completeness",
    ]
    if profile.get("exit_oracles") != expected_oracles:
        raise ContractError("M5-02 exit oracle drift")

    source_contract = {
        "src/onebrain-node/src/vnext_observability.rs": (
            "pub const REASON_CODE_COUNT: usize = 22",
            "pub fn record_count",
            "pub fn begin_journal",
            "pub fn observe_outbox",
            "pub fn observe_selector_coverage",
            "serialized_snapshot_has_no_identity_selector_or_private_need_label_surface",
        ),
        "src/onebrain-node/src/vnext_outbox.rs": (
            "const MAGIC_V3",
            "pub fn stats",
            "oldest_pending_age_seconds",
            "legacy_v1_and_v2_intents_decode_with_unknown_age",
        ),
        "src/onebrain-node/src/vnext_network_runtime.rs": (
            "observable_resource_admission_error",
            "payload_reject_reason",
            "admission_and_payload_failures_map_to_stable_low_cardinality_reasons",
            "two_runtime_listeners_authenticate_and_reject_unvalidated_payload_bytes",
        ),
        "src/onebrain-node/src/vnext_product_runtime.rs": (
            "observe_selector_coverage",
            "observe_pomv",
            "observe_registry_state",
        ),
        "src/onebrain-api/src/vnext_api.rs": (
            "pub observability: onebrain_node::VNextObservabilitySnapshot",
            'body["data"]["observability"]["contains_high_cardinality_labels"]',
        ),
    }
    for relative, needles in source_contract.items():
        text = read(ROOT / relative)
        for needle in needles:
            if needle not in text:
                raise ContractError(
                    f"M5-02 implementation evidence missing: {relative}: {needle}"
                )

    if "Err(_)" in read(ROOT / "src/onebrain-node/src/vnext_network_runtime.rs"):
        raise ContractError("M5-02 network runtime still swallows an untyped error")
    spec = read(VNEXT / "STRUCTURED_OBSERVABILITY_PROFILE_V1.md")
    if "dr-m5-observability-v1.json" not in spec:
        raise ContractError("M5-02 normative profile is not linked to machine contract")
    workflow = read(VNEXT_FOUNDATION_WORKFLOW)
    if (
        "- name: M5.2 typed observability and operator snapshot" not in workflow
        or "python -m unittest scripts.ci.test_validate_vnext_dr_m5_observability"
        not in workflow
    ):
        raise ContractError("M5-02 CI acceptance gate missing")

    return len(expected_reasons), len(expected_gauges), len(expected_oracles)


def validate_vnext_dr_m5_crash_harness(
    profile: dict[str, object] | None = None,
) -> tuple[int, int, int, int]:
    if profile is None:
        try:
            profile = json.loads(read(DR_M5_CRASH_HARNESS_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid M5-03 crash-harness profile JSON: {error}") from error

    if (
        profile.get("format") != "onebrain/dr-m5-crash-harness/1"
        or profile.get("profile_id") != "REAL_REDB_PROCESS_CRASH_HARNESS_V1"
        or profile.get("version") != 1
    ):
        raise ContractError("unexpected M5-03 crash-harness profile")

    feature = profile.get("feature")
    if feature != {
        "name": "vnext-crash-harness",
        "default_enabled": False,
        "kill_switch_env": "ONEBRAIN_DR_M5_FAILPOINTS_ENABLED",
        "kill_switch_value": "1",
        "requires_exact_failpoint": True,
        "requires_fsynced_marker": True,
        "requires_per_case_token": True,
    }:
        raise ContractError("M5-03 default-off failpoint firewall drift")

    process_kill = profile.get("process_kill")
    if process_kill != {
        "worker": "vnext_crash_harness::tests::dr_m5_process_kill_worker",
        "parent_action": "wait_for_fsynced_marker_then_kill_child",
        "marker_timeout_seconds": 10,
        "child_success_is_failure": True,
        "restart_uses_open_not_create": True,
    }:
        raise ContractError("M5-03 child-process kill contract drift")

    expected_boundaries = [
        "TX-PUSE-000",
        "TX-PUSE-001",
        "TX-PUSE-002",
        "TX-OUT-001",
        "TX-OUT-002",
        "TX-JRN-001",
        "TX-VAL-001",
        "TX-INV-001",
        "TX-AUTH-001",
        "TX-KQL-000",
        "TX-KQL-001",
        "TX-POMV-001",
        "TX-POMV-002",
    ]
    if profile.get("boundaries") != expected_boundaries:
        raise ContractError("M5-03 boundary inventory drift")
    expected_phases = [
        "before_begin_write",
        "after_begin_write_before_mutation",
        "after_mutation_before_commit",
        "after_commit_before_next_side_effect",
        "after_next_side_effect_before_ack",
    ]
    if profile.get("phases") != expected_phases:
        raise ContractError("M5-03 failpoint phase inventory drift")
    expected_case_count = len(expected_boundaries) * len(expected_phases)
    if profile.get("required_process_kill_cases") != expected_case_count:
        raise ContractError("M5-03 process-kill case count drift")

    expected_oracle_fields = [
        "accepted_object_cids",
        "accepted_event_cids",
        "selector_inventory_roots",
        "reconciliation_journals",
        "pending_outbox",
        "authority_decisions",
        "private_need_records",
        "distributed_kql_matches",
        "prepared_public_use",
        "public_use_publications",
        "metabolic_views",
    ]
    oracle = profile.get("oracle")
    if oracle != {
        "format": "onebrain/dr-m5-oracle/1",
        "canonicalization": "json-sort-keys-no-whitespace-utf8",
        "digest_algorithm": "sha256",
        "fields": expected_oracle_fields,
        "expected_complete_fixture_sha256": (
            "9c312d251b2347c65149f16fd6a55327cd962ee8d5806bb5bcb642648d9c4aeb"
        ),
    }:
        raise ContractError("M5-03 recovery oracle or digest drift")

    expected_case_fields = [
        "boundary",
        "phase",
        "child_exit",
        "restart_result",
        "oracle_sha256",
        "canonical_rows",
        "side_effect_rows",
        "ack_rows",
        "exact_replay_digest",
    ]
    report = profile.get("report")
    if report != {
        "format": "onebrain/dr-m5-crash-report/1",
        "required_case_fields": expected_case_fields,
        "expected_sha256": (
            "9457130a211e12924c5e6322631a0b6c8ac811de90f67c435a2fd0ed11ed4dcd"
        ),
        "claims_network_completion": False,
    }:
        raise ContractError("M5-03 crash report schema, digest, or claim drift")

    expected_faults = ["disk_full", "read_only", "corrupt_store", "truncated_store"]
    if profile.get("storage_faults") != expected_faults:
        raise ContractError("M5-03 storage-fault inventory drift")
    expected_invariants = [
        "accepted_and_pending_state_is_not_lost",
        "replay_does_not_add_rows_or_change_oracle_digest",
        "authority_decision_remains_fail_closed_without_amplification",
        "corrupt_or_truncated_store_fails_explicitly_without_recreation",
    ]
    if profile.get("recovery_invariants") != expected_invariants:
        raise ContractError("M5-03 recovery invariant inventory drift")

    expected_hook_paths = {
        "TX-PUSE-000": "src/onebrain-node/src/vnext_distributed_pomv.rs",
        "TX-PUSE-001": "src/onebrain-node/src/vnext_distributed_pomv.rs",
        "TX-PUSE-002": "src/onebrain-node/src/vnext_distributed_pomv.rs",
        "TX-OUT-001": "src/onebrain-node/src/vnext_outbox.rs",
        "TX-OUT-002": "src/onebrain-node/src/vnext_outbox.rs",
        "TX-JRN-001": "src/ku-net/src/vnext_reconciliation_journal.rs",
        "TX-VAL-001": "src/ku-core/src/foundation/storage.rs",
        "TX-INV-001": "src/ku-net/src/vnext_inventory_forest.rs",
        "TX-AUTH-001": "src/ku-core/src/foundation/storage.rs",
        "TX-KQL-000": "src/ku-kql/src/vnext_private_need.rs",
        "TX-KQL-001": "src/onebrain-node/src/vnext_distributed_kql.rs",
        "TX-POMV-001": "src/onebrain-node/src/vnext_distributed_pomv.rs",
        "TX-POMV-002": "src/onebrain-node/src/vnext_distributed_pomv.rs",
    }
    hooks = profile.get("owner_hooks")
    if not isinstance(hooks, list) or len(hooks) != len(expected_hook_paths):
        raise ContractError("M5-03 owner-hook inventory drift")
    observed_hooks: dict[str, str] = {}
    for hook in hooks:
        if not isinstance(hook, dict):
            raise ContractError("M5-03 invalid owner-hook row")
        boundary = hook.get("boundary")
        path = hook.get("path")
        if not isinstance(boundary, str) or not isinstance(path, str):
            raise ContractError("M5-03 invalid owner-hook binding")
        if boundary in observed_hooks:
            raise ContractError(f"M5-03 duplicate owner hook: {boundary}")
        observed_hooks[boundary] = path
    if observed_hooks != expected_hook_paths:
        raise ContractError("M5-03 owner-hook path binding drift")
    for boundary, relative in observed_hooks.items():
        source = read(ROOT / relative)
        if boundary not in source:
            raise ContractError(f"M5-03 owner hook missing: {relative}: {boundary}")
        for phase in expected_phases:
            if phase not in source:
                raise ContractError(f"M5-03 phase hook missing: {relative}: {phase}")

    failpoint_source = read(ROOT / "src/ku-core/src/foundation/dr_m5_failpoint.rs")
    for needle in (
        "file.sync_all()",
        ".create_new(true)",
        "ONEBRAIN_DR_M5_FAILPOINTS_ENABLED",
        "ONEBRAIN_DR_M5_TOKEN",
    ):
        if needle not in failpoint_source:
            raise ContractError(f"M5-03 authenticated failpoint evidence missing: {needle}")
    harness_source = read(ROOT / "src/onebrain-node/src/vnext_crash_harness.rs")
    for needle in (
        "child.kill()",
        "Database::open(path)",
        "child_process_kill_matrix_recovers_exactly_once_with_stable_oracle",
        "disk_full_and_read_only_faults_are_explicit_and_non_mutating",
        "corrupt_and_truncated_store_fail_explicitly_without_recreation",
        "DR_M5_CRASH_REPORT_SHA256",
        "claims_network_completion: false",
    ):
        if needle not in harness_source:
            raise ContractError(f"M5-03 harness implementation evidence missing: {needle}")

    node_manifest = read(ROOT / "src/onebrain-node/Cargo.toml")
    if (
        'default = ["base-v1"]' not in node_manifest
        or "vnext-crash-harness = [" not in node_manifest
        or '"ku-core/dr-m5-crash-harness"' not in node_manifest
        or '"ku-net/dr-m5-crash-harness"' not in node_manifest
        or '"ku-kql/dr-m5-crash-harness"' not in node_manifest
    ):
        raise ContractError("M5-03 feature firewall wiring missing")

    inventory = read(DR_M5_TRANSACTION_INVENTORY)
    if "M5-03 process-kill coverage complete" not in inventory:
        raise ContractError("M5-03 transaction inventory status is stale")
    spec = read(VNEXT / "REAL_REDB_PROCESS_CRASH_HARNESS_V1.md")
    if "dr-m5-crash-harness-v1.json" not in spec:
        raise ContractError("M5-03 normative profile is not linked to machine contract")
    workflow = read(VNEXT_FOUNDATION_WORKFLOW)
    if (
        "- name: M5.3 real Redb process-kill crash matrix" not in workflow
        or "python -m unittest scripts.ci.test_validate_vnext_dr_m5_crash_harness"
        not in workflow
        or "--features vnext-crash-harness" not in workflow
    ):
        raise ContractError("M5-03 CI acceptance gate missing")

    return (
        len(expected_boundaries),
        len(expected_phases),
        expected_case_count,
        len(expected_faults),
    )


def validate_vnext_dr_m5_chaos_fuzz(
    profile: dict[str, object] | None = None,
) -> tuple[int, int, int, int, int]:
    if profile is None:
        try:
            profile = json.loads(read(DR_M5_CHAOS_FUZZ_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid M5-04 profile JSON: {error}") from error
    if (
        profile.get("format") != "onebrain/dr-m5-chaos-fuzz/1"
        or profile.get("profile_id") != "CHAOS_AND_FUZZ_PROFILE_V1"
        or profile.get("version") != 1
    ):
        raise ContractError("M5-04 profile identity drift")

    feature = profile.get("feature")
    if feature != {"name": "vnext-chaos-harness", "default_enabled": False}:
        raise ContractError("M5-04 feature firewall drift")

    expected_chaos = [
        "drop",
        "duplicate",
        "delay",
        "reorder",
        "disconnect",
        "partition_reunion",
        "slow_reader_writer",
    ]
    real_quic = profile.get("real_quic")
    if not isinstance(real_quic, dict) or (
        real_quic.get("scenarios") != expected_chaos
        or real_quic.get("timeout_seconds") != 15
        or real_quic.get("authenticated") is not True
        or real_quic.get("reconnect_reauthenticates") is not True
        or real_quic.get("fair_redelivery") is not True
    ):
        raise ContractError("M5-04 real-QUIC scenario contract drift")

    expected_floods = [
        "pre_auth",
        "authenticated_sessions",
        "contexts_manifests",
        "unique_invalid_cids",
        "slowloris",
    ]
    flood = profile.get("flood")
    if not isinstance(flood, dict) or (
        flood.get("scenarios") != expected_floods
        or flood.get("pre_auth_attempts") != 20_000
        or flood.get("authenticated_session_attempts") != 1_024
        or flood.get("context_manifest_attempts") != 1_024
        or flood.get("context_limit") != 8
        or flood.get("unique_invalid_cids") != 4_096
        or flood.get("slowloris_deadline_ms") != 75
    ):
        raise ContractError("M5-04 flood bound drift")

    trace = profile.get("property_trace")
    expected_oracle = (
        "a93a054ece2eabd5afacaaa21a233137a1987c82d646a6e1138598dc225c5a53"
    )
    if not isinstance(trace, dict) or (
        trace.get("seeds") != 64
        or trace.get("steps_per_seed") != 4_096
        or trace.get("record_count") != 64
        or trace.get("oracle_algorithm") != "blake3"
        or trace.get("expected_oracle_blake3") != expected_oracle
        or trace.get("claims_network_completion") is not False
        or trace.get("grants_authority") is not False
    ):
        raise ContractError("M5-04 long-trace oracle drift")

    expected_targets = [
        "canonical_codec",
        "session_reconciliation_codec",
        "carrier_frame",
        "journal_token_snapshot",
        "domain_records",
        "legacy_adapter",
    ]
    fuzz = profile.get("fuzz")
    expected_corpus_digest = (
        "465d554e235738511b69e37c33c0b5e6fcccbc09f8b30e010d7d3eac916c66fd"
    )
    if not isinstance(fuzz, dict) or (
        fuzz.get("cargo_fuzz_version") != "0.13.2"
        or fuzz.get("libfuzzer_sys_version") != "0.4.13"
        or fuzz.get("max_input_bytes") != 4_096
        or fuzz.get("targets") != expected_targets
        or fuzz.get("pr_corpus_seeds_per_target") != 3
        or fuzz.get("required_pr_corpus_cases") != 18
        or fuzz.get("corpus_manifest_sha256") != expected_corpus_digest
    ):
        raise ContractError("M5-04 fuzz target/corpus contract drift")

    nightly = profile.get("nightly")
    if not isinstance(nightly, dict) or nightly != {
        "workflow": ".github/workflows/vnext-fuzz-nightly.yml",
        "rust_toolchain": "nightly-2026-07-20",
        "max_total_time_seconds_per_target": 60,
        "timeout_seconds_per_input": 10,
        "matrix_targets": 6,
        "artifact_retention_days": 14,
    }:
        raise ContractError("M5-04 nightly budget drift")

    expected_exit = [
        "zero_panic_oom_hang_privacy_or_invariant_failure",
        "bounded_state_under_flood",
        "fair_redelivery_same_oracle_root",
        "pr_corpus_smoke_all_targets",
        "versioned_nightly_budget",
    ]
    if profile.get("exit_oracles") != expected_exit:
        raise ContractError("M5-04 exit oracle drift")

    chaos_source = read(ROOT / "src/ku-net/src/vnext_chaos.rs")
    for needle in (
        "real_quic_drop_duplicate_delay_reorder_disconnect_and_reunion_converge",
        "floods_and_slowloris_remain_bounded_without_state_amplification",
        "long_delivery_traces_converge_to_one_oracle_under_fair_redelivery",
        "private-standing-need-must-not-cross-chaos-wire",
        "accepted.extend(source)",
        "claims_network_completion: false",
    ):
        if needle not in chaos_source:
            raise ContractError(f"M5-04 chaos implementation evidence missing: {needle}")

    shared_target_source = read(
        ROOT / "src/onebrain-node/src/vnext_fuzz_targets.rs"
    )
    for needle in (
        "decode_canonical",
        "decode_session_message",
        "decode_reconciliation_message",
        "QuicRecordAdapter::decode",
        "fuzz_decode_journal_token_and_snapshot",
        "decode_knowledge_object",
        "decode_knowledge_event",
        "decode_feed_inception",
        "decode_actor_delegation",
        "decode_actor_revocation",
        "UseEvidencePayload::from_validated_object",
        "legacy::parse_peer_message",
        "assert!(!adapter.grants_vnext_authority())",
    ):
        if needle not in shared_target_source:
            raise ContractError(f"M5-04 parser target evidence missing: {needle}")
    journal_source = read(
        ROOT / "src/ku-net/src/vnext_reconciliation_journal.rs"
    )
    if "validate_token_against(&projection, &token, token_key)" not in journal_source:
        raise ContractError("M5-04 journal token validator fuzz evidence missing")

    fuzz_manifest = read(ROOT / "src/fuzz/Cargo.toml")
    if (
        'libfuzzer-sys = "=0.4.13"' not in fuzz_manifest
        or 'features = ["vnext-chaos-harness"]' not in fuzz_manifest
    ):
        raise ContractError("M5-04 cargo-fuzz manifest/version wiring drift")
    fuzz_lock = read(ROOT / "src/fuzz/Cargo.lock")
    if not re.search(
        r'(?m)^name = "libfuzzer-sys"\nversion = "0\.4\.13"$',
        fuzz_lock,
    ):
        raise ContractError("M5-04 cargo-fuzz lockfile version drift")
    for target in expected_targets:
        relative = f"fuzz_targets/{target}.rs"
        if f'name = "{target}"' not in fuzz_manifest or relative not in fuzz_manifest:
            raise ContractError(f"M5-04 fuzz manifest target missing: {target}")
        target_source = read(ROOT / "src/fuzz" / relative)
        if (
            f'run_target("{target}", data)' not in target_source
            or "MAX_FUZZ_INPUT_BYTES" not in target_source
        ):
            raise ContractError(f"M5-04 fuzz target wrapper drift: {target}")

    corpus_root = ROOT / "src/fuzz/corpus"
    attributes = read(ROOT / ".gitattributes")
    if "src/fuzz/corpus/** -text" not in attributes:
        raise ContractError("M5-04 corpus byte-preservation attribute missing")
    digest = hashlib.sha256()
    digest.update(b"onebrain:dr-m5:fuzz-corpus:1\0")
    corpus_cases = 0

    def digest_field(value: bytes) -> None:
        digest.update(len(value).to_bytes(8, "big"))
        digest.update(value)

    for target in expected_targets:
        digest_field(target.encode("utf-8"))
        directory = corpus_root / target
        if not directory.is_dir():
            raise ContractError(f"M5-04 corpus directory missing: {target}")
        files = sorted(path for path in directory.iterdir() if path.is_file())
        if len(files) != 3:
            raise ContractError(f"M5-04 corpus seed count drift: {target}")
        for path in files:
            data = path.read_bytes()
            if not data or len(data) > 4_096:
                raise ContractError(f"M5-04 corpus seed bound drift: {path.name}")
            digest_field(path.name.encode("utf-8"))
            digest_field(data)
            corpus_cases += 1
    if digest.hexdigest() != expected_corpus_digest:
        raise ContractError("M5-04 corpus manifest digest drift")

    nightly_source = read(ROOT / ".github/workflows/vnext-fuzz-nightly.yml")
    for needle in (
        "RUSTUP_TOOLCHAIN: nightly-2026-07-20",
        'rustup toolchain install "$RUSTUP_TOOLCHAIN" --profile minimal --no-self-update',
        "cargo install cargo-fuzz --version 0.13.2 --locked",
        "-max_total_time=60 -timeout=10 -max_len=4096",
        "retention-days: 14",
    ):
        if needle not in nightly_source:
            raise ContractError(f"M5-04 nightly workflow evidence missing: {needle}")
    for target in expected_targets:
        if f"- {target}" not in nightly_source:
            raise ContractError(f"M5-04 nightly target missing: {target}")

    node_manifest = read(ROOT / "src/onebrain-node/Cargo.toml")
    if (
        'default = ["base-v1"]' not in node_manifest
        or "vnext-chaos-harness = [" not in node_manifest
        or '"ku-net/dr-m5-chaos-harness"' not in node_manifest
        or 'required-features = ["vnext-chaos-harness"]' not in node_manifest
    ):
        raise ContractError("M5-04 feature firewall wiring missing")
    spec = read(VNEXT / "CHAOS_AND_FUZZ_PROFILE_V1.md")
    if "dr-m5-chaos-fuzz-v1.json" not in spec:
        raise ContractError("M5-04 normative profile is not linked to machine contract")
    workflow = read(VNEXT_FOUNDATION_WORKFLOW)
    for needle in (
        "python -m unittest scripts.ci.test_validate_vnext_dr_m5_chaos_fuzz",
        "- name: M5.4 real-QUIC chaos and adversarial flood",
        "- name: M5.4 deterministic parser corpus smoke",
        "--features vnext-chaos-harness",
    ):
        if needle not in workflow:
            raise ContractError(f"M5-04 PR acceptance gate missing: {needle}")

    return (
        len(expected_chaos),
        len(expected_floods),
        len(expected_targets),
        corpus_cases,
        len(expected_exit),
    )


def validate_vnext_dr_m5_operational_compaction(
    profile: dict[str, object] | None = None,
) -> tuple[int, int, int, int, int]:
    if profile is None:
        try:
            profile = json.loads(read(DR_M5_OPERATIONAL_COMPACTION_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid M5-05 profile JSON: {error}") from error
    if (
        profile.get("format") != "onebrain/dr-m5-operational-compaction/1"
        or profile.get("profile_id") != "OPERATIONAL_COMPACTION_PROFILE_V1"
        or profile.get("version") != 1
    ):
        raise ContractError("M5-05 profile identity drift")

    feature = profile.get("feature")
    if feature != {
        "name": "vnext-compaction-harness",
        "default_enabled": False,
    }:
        raise ContractError("M5-05 feature firewall drift")

    kill_switch = profile.get("kill_switch")
    if kill_switch != {
        "default_enabled": False,
        "generation_fenced": True,
        "stale_permit_commits": False,
    }:
        raise ContractError("M5-05 kill-switch generation fence drift")

    journal = profile.get("journal")
    if journal != {
        "allowed_compactable_states": ["completed", "superseded"],
        "protected_states": [
            "pending",
            "retrying",
            "inflight",
            "missing_dependency",
        ],
        "audit_identity": "full_manifest_digest",
        "exact_canonical_length_required": True,
        "semantic_root_unchanged": True,
    }:
        raise ContractError("M5-05 journal eligibility/protection drift")

    outbox = profile.get("outbox")
    if outbox != {
        "terminal_states": [
            "acknowledged",
            "dead_letter",
            "retry_exhausted",
        ],
        "protected_states": ["pending"],
        "audit_before_delete": True,
        "audit_and_delete_atomic": True,
        "payload_digest": "blake3",
        "max_tombstones": 65_536,
        "physical_disk_decrease_required": True,
    }:
        raise ContractError("M5-05 outbox audit-before-delete contract drift")

    bounded = profile.get("bounded_evidence")
    expected_overflow_fields = [
        "dropped_records",
        "dropped_bytes",
        "chain_root",
        "last_dropped_id",
    ]
    if bounded != {
        "lanes": ["quarantine", "provenance"],
        "max_records_per_lane": 4_096,
        "max_record_bytes": 1_048_576,
        "overflow_fields": expected_overflow_fields,
        "retry_last_overflow_idempotent": True,
    }:
        raise ContractError("M5-05 bounded evidence/overflow contract drift")

    derived = profile.get("derived_snapshots")
    frozen_fixture = {
        "rows": [
            {"key_hex": "01", "value_byte": 1, "value_length": 32},
            {"key_hex": "02", "value_byte": 2, "value_length": 64},
            {"key_hex": "03", "value_byte": 3, "value_length": 96},
        ],
        "kql_source_root_blake3": (
            "0ca08333f6db371de7674d19cb99db26df952b72a87ca6ee37226a9bf0872910"
        ),
        "kql_projection_root_blake3": (
            "230a443d1bd69814e05fb2a2173c4d895556b262b39586204120fa48d8442194"
        ),
        "pomv_source_root_blake3": (
            "7fbcf8ee16d00a0c31391f45dcf0c424387c53dafa2ea77d0a4a37a5f799f689"
        ),
        "pomv_projection_root_blake3": (
            "73f25199ee54961a10dc3585ed28d8fc08e1be432ced37f1f0b3a92582ccc571"
        ),
    }
    if derived != {
        "lanes": ["kql", "pomv"],
        "reducer_version": 1,
        "max_rows": 65_536,
        "max_snapshot_bytes": 16_777_216,
        "canonical_exact_restore": True,
        "fixture": frozen_fixture,
    }:
        raise ContractError("M5-05 derived snapshot/root contract drift")

    expected_boundaries = [
        "TX-CMP-JRN-001",
        "TX-CMP-OUT-001",
        "TX-CMP-QAR-001",
        "TX-CMP-PRV-001",
        "TX-CMP-IDX-001",
    ]
    expected_phases = [
        "before_begin_write",
        "after_begin_write_before_mutation",
        "after_mutation_before_commit",
        "after_commit_before_next_side_effect",
        "after_next_side_effect_before_ack",
    ]
    process_kill = profile.get("process_kill")
    expected_case_count = len(expected_boundaries) * len(expected_phases)
    if not isinstance(process_kill, dict) or (
        process_kill.get("boundaries") != expected_boundaries
        or process_kill.get("phases") != expected_phases
        or process_kill.get("required_process_kill_cases") != expected_case_count
        or process_kill.get("real_redb_reopen") is not True
        or process_kill.get("retry_after_reopen") is not True
    ):
        raise ContractError("M5-05 process-kill matrix drift")

    expected_exit = [
        "exact_root_after_every_crash",
        "pending_work_continues",
        "semantic_result_unchanged",
        "logical_payload_bytes_decrease",
        "physical_disk_bytes_decrease",
    ]
    if profile.get("exit_oracles") != expected_exit:
        raise ContractError("M5-05 exit oracle drift")

    switch_source = read(
        ROOT / "src/ku-core/src/foundation/operational_compaction.rs"
    )
    for needle in (
        "pub struct OperationalCompactionSwitch",
        "AtomicBool::new(false)",
        ".fetch_add(1, Ordering::AcqRel)",
        "pub struct OperationalCompactionPermit",
        "pub fn run_if_current",
        "switch_is_default_off_and_stale_permits_never_revive",
    ):
        if needle not in switch_source:
            raise ContractError(
                f"M5-05 kill-switch implementation evidence missing: {needle}"
            )

    journal_source = read(
        ROOT / "src/ku-net/src/vnext_reconciliation_journal.rs"
    )
    for needle in (
        "compact_completed_manifests",
        "accepted.canonical_length == entry.canonical_length",
        "next.compacted_manifests.insert(*digest)",
        "store_compaction_atomically",
        "m5_05_journal_process_kill_matrix_restores_exact_root",
    ):
        if needle not in journal_source:
            raise ContractError(
                f"M5-05 journal implementation evidence missing: {needle}"
            )

    outbox_source = read(ROOT / "src/onebrain-node/src/vnext_outbox.rs")
    for needle in (
        "pub struct OutboundAuditTombstone",
        "if intent.state.is_terminal()",
        "let mut tombstones",
        "outbox.remove",
        "physical_reclaim_reduces_disk_after_terminal_payload_compaction",
        "m5_05_outbox_process_kill_matrix_restores_exact_root",
    ):
        if needle not in outbox_source:
            raise ContractError(
                f"M5-05 outbox implementation evidence missing: {needle}"
            )

    operational_source = read(
        ROOT / "src/onebrain-node/src/vnext_operational_compaction.rs"
    )
    for needle in (
        "MAX_OPERATIONAL_EVIDENCE_RECORDS: u64 = 4_096",
        "MAX_OPERATIONAL_EVIDENCE_BYTES: usize = 1_048_576",
        "previous.last_dropped_id == id",
        "MAX_DERIVED_SNAPSHOT_ROWS: usize = 65_536",
        "MAX_DERIVED_SNAPSHOT_BYTES: usize = 16 * 1_048_576",
        "frozen_kql_and_pomv_snapshot_roots_match_profile",
        "m5_05_operational_process_kill_matrix_restores_exact_root",
    ):
        if needle not in operational_source:
            raise ContractError(
                f"M5-05 operational implementation evidence missing: {needle}"
            )
    for boundary in expected_boundaries[2:]:
        if boundary not in operational_source:
            raise ContractError(
                f"M5-05 operational boundary evidence missing: {boundary}"
            )

    node_manifest = read(ROOT / "src/onebrain-node/Cargo.toml")
    if (
        'default = ["base-v1"]' not in node_manifest
        or "vnext-compaction-harness = [" not in node_manifest
        or '"ku-core/dr-m5-crash-harness"' not in node_manifest
        or '"ku-net/dr-m5-crash-harness"' not in node_manifest
    ):
        raise ContractError("M5-05 feature firewall wiring missing")

    inventory = read(DR_M5_TRANSACTION_INVENTORY)
    for boundary in expected_boundaries:
        if boundary not in inventory:
            raise ContractError(
                f"M5-05 transaction inventory missing boundary: {boundary}"
            )
    spec = read(VNEXT / "OPERATIONAL_COMPACTION_PROFILE_V1.md")
    if "dr-m5-operational-compaction-v1.json" not in spec:
        raise ContractError("M5-05 normative profile is not linked to machine contract")
    workflow = read(VNEXT_FOUNDATION_WORKFLOW)
    for needle in (
        "python -m unittest scripts.ci.test_validate_vnext_dr_m5_operational_compaction",
        "- name: M5.5 operational compaction and process-kill recovery",
        "--features persist,dr-m5-crash-harness",
        "--features vnext-compaction-harness",
    ):
        if needle not in workflow:
            raise ContractError(f"M5-05 PR acceptance gate missing: {needle}")

    return (
        len(expected_boundaries),
        len(expected_phases),
        expected_case_count,
        len(derived["lanes"]),
        len(expected_exit),
    )


def validate_vnext_dr_m5_mixed_rollback(
    profile: dict[str, object] | None = None,
) -> tuple[int, int, int, int]:
    if profile is None:
        try:
            profile = json.loads(read(DR_M5_MIXED_ROLLBACK_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid M5-06 profile JSON: {error}") from error
    if (
        profile.get("format") != "onebrain/dr-m5-mixed-rollback/1"
        or profile.get("profile_id") != "MIXED_VERSION_RUNTIME_ROLLBACK_PROFILE_V1"
        or profile.get("version") != 1
    ):
        raise ContractError("M5-06 profile identity drift")

    if profile.get("transports") != {
        "simultaneous": True,
        "legacy": "tcp-json-v1",
        "vnext": "authenticated-quic-obp-rp-v1",
        "real_loopback_required": True,
    }:
        raise ContractError("M5-06 simultaneous real-transport contract drift")

    legacy = profile.get("legacy_n_minus_one")
    if not isinstance(legacy, dict) or (
        legacy.get("fixture_kind") != "frozen-wire-corpus"
        or legacy.get("source_release") != "onebrain-legacy-tcp-json-v1"
        or legacy.get("byte_exact_reserialization") is not True
        or legacy.get("vnext_authority") is not False
    ):
        raise ContractError("M5-06 N-1 fixture contract drift")
    corpus = legacy.get("corpus")
    if not isinstance(corpus, list) or len(corpus) != 3:
        raise ContractError("M5-06 N-1 corpus coverage drift")
    ids: set[str] = set()
    for row in corpus:
        if not isinstance(row, dict):
            raise ContractError("M5-06 N-1 corpus row is not an object")
        fixture_id = row.get("id")
        payload_hex = row.get("payload_hex")
        framed_hex = row.get("framed_hex")
        if (
            not isinstance(fixture_id, str)
            or fixture_id in ids
            or not isinstance(payload_hex, str)
            or not isinstance(framed_hex, str)
        ):
            raise ContractError("M5-06 N-1 corpus identity/hex drift")
        ids.add(fixture_id)
        try:
            payload = bytes.fromhex(payload_hex)
            frame = bytes.fromhex(framed_hex)
        except ValueError as error:
            raise ContractError("M5-06 N-1 corpus contains invalid hex") from error
        if len(frame) < 4 or int.from_bytes(frame[:4], "big") != len(payload):
            raise ContractError("M5-06 N-1 frame length drift")
        if frame[4:] != payload:
            raise ContractError("M5-06 N-1 frame payload drift")
        try:
            decoded = json.loads(payload)
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise ContractError("M5-06 N-1 payload is not frozen JSON") from error
        if not isinstance(decoded, dict) or len(decoded) != 1:
            raise ContractError("M5-06 N-1 legacy message shape drift")

    expected_lanes = [
        "network",
        "distributed_kql_one_hop",
        "public_use_evidence_publish",
        "distributed_pomv_view",
    ]
    generation = profile.get("runtime_generation_fence")
    if generation != {
        "lanes": expected_lanes,
        "startup_config_may_disable": True,
        "startup_config_may_reenable": False,
        "explicit_reenable_required": True,
        "kill_is_idempotent": True,
        "reenable_advances_generation": True,
        "session_rule": "existing-session-must-recheck-before-each-record",
        "inflight_rule": "operation-past-generation-check-may-drain",
    }:
        raise ContractError("M5-06 runtime generation fence drift")

    expected_preserved = [
        "raw",
        "journal",
        "outbox",
        "quarantine",
        "provenance",
        "wallet",
        "obt",
    ]
    rollback = profile.get("rollback")
    if rollback != {
        "atomic_all_lane_disable": True,
        "preserves": expected_preserved,
        "stale_config_reenable": False,
        "legacy_local_offline_available": True,
        "changes_wallet_state": False,
        "changes_obt_state": False,
    }:
        raise ContractError("M5-06 rollback preservation contract drift")

    expected_phases = [
        "before_begin_write",
        "after_begin_write_before_mutation",
        "after_mutation_before_commit",
        "after_commit_before_next_side_effect",
        "after_next_side_effect_before_ack",
    ]
    process_kill = profile.get("process_kill")
    if process_kill != {
        "boundary": "TX-ROL-001",
        "phases": expected_phases,
        "required_process_kill_cases": 5,
        "real_redb_reopen": True,
        "retry_after_reopen": True,
    }:
        raise ContractError("M5-06 process-kill matrix drift")

    expected_exit = [
        "no_new_session_after_network_kill",
        "no_new_publish_after_publication_kill",
        "independent_product_lane_fences",
        "stale_config_cannot_reenable",
        "rollback_preserves_durable_evidence",
        "legacy_and_vnext_real_transports_coexist",
        "wallet_and_obt_unchanged",
    ]
    if profile.get("exit_oracles") != expected_exit:
        raise ContractError("M5-06 exit oracle drift")

    rollout_source = read(
        ROOT / "src/onebrain-node/src/vnext_runtime_rollout.rs"
    )
    for needle in (
        'TableDefinition::new("vnext_runtime_rollout_v1")',
        'const TX_RUNTIME_ROLLBACK: &str = "TX-ROL-001"',
        "Startup may apply a",
        "pub fn kill(",
        "pub fn reenable(",
        "pub fn rollback(",
        "m5_06_runtime_rollback_process_kill_matrix_recovers_exact_generation",
    ):
        if needle not in rollout_source:
            raise ContractError(
                f"M5-06 rollout implementation evidence missing: {needle}"
            )

    product_source = read(
        ROOT / "src/onebrain-node/src/vnext_product_runtime.rs"
    )
    for needle in (
        "kill_runtime_lane",
        "reenable_runtime_lane",
        "rollback_runtime",
        "runtime_kill_rollback_restart_and_explicit_reenable_use_real_quic",
        "VNextRuntimeLane::DistributedKql",
        "VNextRuntimeLane::PublicUseEvidencePublish",
        "VNextRuntimeLane::DistributedPomvView",
    ):
        if needle not in product_source:
            raise ContractError(
                f"M5-06 product fence evidence missing: {needle}"
            )

    network_source = read(
        ROOT / "src/onebrain-node/src/vnext_network_runtime.rs"
    )
    for needle in (
        "network generation changed",
        "generation.is_current()",
        'connection.close("OBP-RP runtime generation fenced")',
    ):
        if needle not in network_source:
            raise ContractError(
                f"M5-06 network generation evidence missing: {needle}"
            )

    mixed_source = read(
        ROOT / "src/onebrain-node/src/vnext_mixed_conformance.rs"
    )
    if "verify_frozen_n_minus_one_wire_corpus" not in mixed_source:
        raise ContractError("M5-06 frozen N-1 executable evidence missing")
    node_test = read(
        ROOT / "src/onebrain-node/tests/vnext_node_runtime.rs"
    )
    if (
        "legacy_tcp_and_vnext_quic_exchange_concurrently_on_real_loopback"
        not in node_test
    ):
        raise ContractError("M5-06 simultaneous transport evidence missing")

    inventory = read(DR_M5_TRANSACTION_INVENTORY)
    if "TX-ROL-001" not in inventory:
        raise ContractError("M5-06 transaction inventory missing TX-ROL-001")
    spec = read(VNEXT / "MIXED_VERSION_RUNTIME_ROLLBACK_PROFILE_V1.md")
    if "dr-m5-mixed-rollback-v1.json" not in spec:
        raise ContractError("M5-06 normative profile is not linked to machine contract")
    workflow = read(VNEXT_FOUNDATION_WORKFLOW)
    for needle in (
        "python -m unittest scripts.ci.test_validate_vnext_dr_m5_mixed_rollback",
        "- name: M5.6 mixed-version runtime rollback and process-kill recovery",
        "--features vnext-crash-harness",
        "--lib vnext_runtime_rollout",
    ):
        if needle not in workflow:
            raise ContractError(f"M5-06 PR acceptance gate missing: {needle}")

    return len(expected_lanes), len(corpus), len(expected_phases), len(expected_exit)


def validate_vnext_soak_runner_kit(
    runner_script: str | None = None,
    runner_guide: str | None = None,
    soak_workflow: str | None = None,
) -> tuple[int, int, int]:
    if runner_script is None:
        runner_script = read(VNEXT_SOAK_RUNNER_SCRIPT)
    if runner_guide is None:
        runner_guide = read(VNEXT_SOAK_RUNNER_GUIDE)
    if soak_workflow is None:
        soak_workflow = read(VNEXT_SOAK_WORKFLOW)

    script_needles = (
        "require_non_root",
        "sha256sum --check --status",
        "--labels \"$RUNNER_LABELS\"",
        "--ephemeral",
        "setup_runner ephemeral",
        "setup-run",
        "./config.sh remove --token",
        "resolve_path \"$RUNNER_HOME\"",
        "realpath -m \"$path\"",
        'rm -rf -- "$RUNNER_HOME"',
        "No inbound firewall port is required",
        "run_privileged",
        "command_exists dnf",
        "command_exists yum",
        "require_supported_distribution",
        "Darwin/arm64",
        'RUNNER_ASSET_ID="osx-arm64"',
        'DEFAULT_RUNNER_LABELS="onebrain-soak-macos-arm64"',
        "shasum -a 256",
        "caffeinate -dimsu",
        "brew install python@3.13 cmake pkgconf",
        '[[ "$HOST_KIND" == "linux-x64" ]] || return 0',
        '[[ "$HOST_KIND" == "macos-arm64" ]] || return 0',
    )
    for needle in script_needles:
        if needle not in runner_script:
            raise ContractError(f"M5-07 portable runner safety missing: {needle}")
    for forbidden in (
        "RUNNER_ALLOW_RUNASROOT",
        'rm -rf -- "$HOME"',
        'rm -rf "$HOME"',
        "sudo ufw",
        "--no-default-labels",
        "--disableupdate",
        'DEFAULT_RUNNER_HOME="${HOME}/Library/Application Support',
    ):
        if forbidden in runner_script:
            raise ContractError(
                f"M5-07 portable runner contains forbidden behavior: {forbidden}"
            )

    guide_needles = (
        "Không cần mở TCP/UDP inbound",
        "sudo ufw allow out 443/tcp",
        "setup-run",
        "pre-release-72h",
        "uninstall",
        "repo public",
        "không chứa SSH key",
        "Rocky/Alma/RHEL",
        "CentOS 7 không được hỗ trợ",
    )
    for needle in guide_needles:
        if needle not in runner_guide:
            raise ContractError(f"M5-07 portable runner guide missing: {needle}")

    workflow_needles = (
        "permissions:\n  contents: read",
        "github.ref == 'refs/heads/main'",
        "runs-on: [self-hosted, linux, x64, onebrain-soak]",
        "save-if: false",
    )
    for needle in workflow_needles:
        if needle not in soak_workflow:
            raise ContractError(
                f"M5-07 self-hosted workflow safety missing: {needle}"
            )
    if "pull_request:" in soak_workflow:
        raise ContractError("M5-07 self-hosted workflow must not run on pull requests")

    return len(script_needles), len(guide_needles), len(workflow_needles)


def validate_vnext_macos_soak_runner_kit(
    runner_script: str | None = None,
    runner_guide: str | None = None,
    soak_workflow: str | None = None,
    foundation_workflow: str | None = None,
) -> tuple[int, int, int, int]:
    if runner_script is None:
        runner_script = read(VNEXT_SOAK_RUNNER_SCRIPT)
    if runner_guide is None:
        runner_guide = read(VNEXT_MACOS_SOAK_RUNNER_GUIDE)
    if soak_workflow is None:
        soak_workflow = read(VNEXT_MACOS_SOAK_WORKFLOW)
    if foundation_workflow is None:
        foundation_workflow = read(VNEXT_FOUNDATION_WORKFLOW)

    script_needles = (
        "Darwin/arm64",
        'RUNNER_ASSET_ID="osx-arm64"',
        'DEFAULT_RUNNER_HOME="${HOME}/onebrain-actions-runner"',
        'DEFAULT_RUNNER_LABELS="onebrain-soak-macos-arm64"',
        "shasum -a 256",
        "caffeinate -dimsu",
        "brew install python@3.13 cmake pkgconf",
        "macOS purge target must be below HOME",
        "Cannot write runner parent directory",
        '[[ "$HOST_KIND" == "linux-x64" ]] || return 0',
        '[[ "$HOST_KIND" == "macos-arm64" ]] || return 0',
    )
    for needle in script_needles:
        if needle not in runner_script:
            raise ContractError(f"M5-07 macOS runner safety missing: {needle}")

    guide_needles = (
        "Không cần mở TCP/UDP inbound",
        "macOS ARM64 (Darwin/arm64)",
        "actions-runner-osx-arm64",
        "onebrain-soak-macos-arm64",
        "caffeinate",
        "pre-release-72h",
        "uninstall",
        "~/onebrain-actions-runner",
    )
    for needle in guide_needles:
        if needle not in runner_guide:
            raise ContractError(f"M5-07 macOS runner guide missing: {needle}")

    workflow_needles = (
        "permissions:\n  contents: read",
        "workflow_dispatch:",
        "github.ref == 'refs/heads/main'",
        "runs-on: [self-hosted, macOS, ARM64, onebrain-soak-macos-arm64]",
        "timeout-minutes: 4440",
        "caffeinate -dimsu",
        "actions/upload-artifact@v4",
        "save-if: false",
    )
    for needle in workflow_needles:
        if needle not in soak_workflow:
            raise ContractError(
                f"M5-07 macOS self-hosted workflow safety missing: {needle}"
            )
    for forbidden in ("pull_request:", "schedule:"):
        if forbidden in soak_workflow:
            raise ContractError(
                f"M5-07 macOS workflow must remain manual-only: {forbidden}"
            )

    foundation_needles = (
        "runs-on: macos-15",
        'test "$(uname -m)" = "arm64"',
        "m5_07_macos_proc_metrics_are_available",
        "m5_07_release_smoke_uses_real_quic_fsync_and_all_fault_cycles",
        "python -m unittest scripts.ci.test_validate_vnext_macos_soak_runner_kit",
    )
    for needle in foundation_needles:
        if needle not in foundation_workflow:
            raise ContractError(
                f"M5-07 hosted macOS acceptance lane missing: {needle}"
            )

    return (
        len(script_needles),
        len(guide_needles),
        len(workflow_needles),
        len(foundation_needles),
    )


def validate_vnext_dr_m5_soak_release(
    profile: dict[str, object] | None = None,
) -> tuple[int, int, int, int, int]:
    if profile is None:
        try:
            profile = json.loads(read(DR_M5_SOAK_RELEASE_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid M5-07 profile JSON: {error}") from error
    if (
        profile.get("format") != "onebrain/dr-m5-soak-release/1"
        or profile.get("profile_id") != "SOAK_PERFORMANCE_RELEASE_GATE_PROFILE_V1"
        or profile.get("version") != 1
    ):
        raise ContractError("M5-07 profile identity drift")

    if profile.get("build") != {
        "cargo_profile": "release",
        "transport": "authenticated-real-quic-loopback",
        "long_runner": ["self-hosted", "linux", "x64", "onebrain-soak"],
        "github_hosted_job_limit_hours": 6,
    }:
        raise ContractError("M5-07 release build/runner contract drift")

    run_profiles = profile.get("run_profiles")
    expected_profiles = {
        "smoke": {
            "minimum_elapsed_seconds": 0,
            "minimum_fault_cycles": 3,
            "release_qualifying": False,
        },
        "nightly-24h": {
            "minimum_elapsed_seconds": 86_400,
            "minimum_fault_cycles": 3,
            "release_qualifying": False,
        },
        "pre-release-72h": {
            "minimum_elapsed_seconds": 259_200,
            "minimum_fault_cycles": 3,
            "release_qualifying": True,
        },
    }
    if run_profiles != expected_profiles:
        raise ContractError("M5-07 duration qualification drift")

    expected_latency = {
        "quic_authenticated_connect": {
            "p50": 500_000,
            "p95": 1_000_000,
            "p99": 2_000_000,
        },
        "fsync_4k": {
            "p50": 100_000,
            "p95": 500_000,
            "p99": 2_000_000,
        },
        "kql_incremental_scan_max": 250_000,
        "pomv_incremental_scan_max": 250_000,
    }
    if profile.get("latency_budgets_micros") != expected_latency:
        raise ContractError("M5-07 latency percentile budget drift")

    expected_growth = {
        "rss_bytes": {
            "hard_cap": 536_870_912,
            "max_growth": 134_217_728,
            "max_positive_slope_per_cycle": 8_388_608,
        },
        "disk_bytes": {
            "hard_cap": 536_870_912,
            "max_growth": 33_554_432,
            "max_positive_slope_per_cycle": 2_097_152,
        },
        "task_count": {
            "hard_cap": 512,
            "max_growth": 16,
            "max_positive_slope_per_cycle": 4,
        },
    }
    if profile.get("growth_budgets") != expected_growth:
        raise ContractError("M5-07 resource growth budget drift")

    expected_lanes = ["distributed-kql-one-hop", "distributed-pomv-view"]
    if profile.get("incremental_scan") != {
        "lanes": expected_lanes,
        "max_records_per_scan": 64,
        "drained_scan_records": 0,
        "durable_selector_type_cursor": True,
    }:
        raise ContractError("M5-07 incremental scan contract drift")

    expected_faults = [
        "slow-peer",
        "bounded-session-flood",
        "partition-reunion",
    ]
    if profile.get("fault_cycle") != expected_faults:
        raise ContractError("M5-07 fault cycle coverage drift")

    expected_signals = [
        "quic-latency-percentiles",
        "fsync-latency-percentiles",
        "rss-growth-and-slope",
        "disk-growth-and-slope",
        "task-growth-and-leak",
        "incremental-scan-budget",
        "runtime-bounded-counters",
        "rollback-reason-codes",
    ]
    if profile.get("operator_signals") != expected_signals:
        raise ContractError("M5-07 operator signal coverage drift")

    expected_exit = [
        "no-unbounded-memory-disk-task-slope",
        "no-task-or-session-leak",
        "m3-fair-redelivery-root-preserved",
        "m4-no-truth-benefit-wallet-obt-amplification",
        "operator-can-detect-and-rollback",
        "short-run-cannot-claim-long-soak",
        "pre-release-requires-72-real-hours",
    ]
    if profile.get("exit_oracles") != expected_exit:
        raise ContractError("M5-07 exit oracle drift")

    source = read(ROOT / "src/onebrain-node/src/vnext_soak_release.rs")
    for needle in (
        'pub const SOAK_RELEASE_PROFILE: &str = "onebrain/dr-m5-soak-release/1"',
        "pub async fn run_soak_release(",
        "measure_quic_connects(",
        "measure_fsync(",
        "measure_incremental_scans(",
        "run_slow_peer_cycle(",
        "run_bounded_flood_cycle(",
        "run_partition_reunion_cycle(",
        "DURATION_OR_CYCLE_EVIDENCE_INCOMPLETE",
        "m5_07_release_smoke_uses_real_quic_fsync_and_all_fault_cycles",
        "pub host_os: String",
        "pub host_arch: String",
        "proc_pidinfo(",
        "m5_07_macos_proc_metrics_are_available",
    ):
        if needle not in source:
            raise ContractError(f"M5-07 implementation evidence missing: {needle}")

    cargo = read(ROOT / "src/onebrain-node/Cargo.toml")
    for needle in (
        'name = "dr_m5_soak_release"',
        'required-features = ["vnext-soak-harness"]',
        "vnext-soak-harness = [",
        '"ku-net/dr-m5-chaos-harness"',
    ):
        if needle not in cargo:
            raise ContractError(f"M5-07 Cargo gate missing: {needle}")

    foundation = read(VNEXT_FOUNDATION_WORKFLOW)
    for needle in (
        "python -m unittest scripts.ci.test_validate_vnext_dr_m5_soak_release",
        "- name: M5.7 optimized real-QUIC soak and performance release smoke",
        "cargo test --locked --release -p onebrain-node",
        "--features vnext-soak-harness",
        "--profile smoke",
    ):
        if needle not in foundation:
            raise ContractError(f"M5-07 PR acceptance gate missing: {needle}")

    soak_workflow = read(VNEXT_SOAK_WORKFLOW)
    for needle in (
        'cron: "41 1 * * *"',
        "runs-on: [self-hosted, linux, x64, onebrain-soak]",
        "timeout-minutes: 4440",
        "nightly-24h",
        "pre-release-72h",
        "actions/upload-artifact@v4",
    ):
        if needle not in soak_workflow:
            raise ContractError(f"M5-07 long-soak workflow missing: {needle}")

    validate_vnext_soak_runner_kit(soak_workflow=soak_workflow)
    validate_vnext_macos_soak_runner_kit(
        soak_workflow=read(VNEXT_MACOS_SOAK_WORKFLOW),
        foundation_workflow=foundation,
    )

    spec = read(VNEXT / "SOAK_PERFORMANCE_RELEASE_GATE_PROFILE_V1.md")
    if "dr-m5-soak-release-v1.json" not in spec:
        raise ContractError("M5-07 normative profile is not linked to machine contract")

    return (
        len(expected_profiles),
        len(expected_latency),
        len(expected_growth),
        len(expected_faults),
        len(expected_exit),
    )
