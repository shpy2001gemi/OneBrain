//! ARCH-13 slice 4: bind the frozen M5-01 resource-admission profile
//! (`dr-m5-resource-admission-v1.json`) to the code that enforces it, instead
//! of checking that constant or function names merely appear in source.

use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr};

use ku_core::foundation::storage::{
    MAX_VERIFIED_ACCEPTED_RECORDS, MAX_VERIFIED_QUARANTINE_RECORDS,
};
use ku_core::foundation::NodeId;
use ku_kql::vnext_proposal::MAX_PROPOSAL_QUARANTINE_RECORDS;
use ku_net::vnext_carrier_adapter::MAX_QUIC_FRAME_BYTES;
use ku_net::vnext_inventory_forest::MAX_INVENTORY_RECORDS;
use ku_net::vnext_resource_gate::{
    admit_length_prefix, AdmissionStage, LengthAdmissionError, ResourceAdmissionError,
    ResourceLane, RuntimeAdmissionController,
};
use ku_net::vnext_session::{SessionReplayGuard, DEFAULT_SESSION_REPLAY_ENTRIES};
use serde_json::Value;

use crate::vnext_config::{VNextNetworkPolicy, VNextRuntimeBudgets};
use crate::vnext_distributed_kql::MAX_DURABLE_KQL_MATCHES;
use crate::vnext_network_runtime::admission_limits;
use crate::vnext_outbox::{OutboundIntentState, MAX_OUTBOX_RECORDS, MAX_OUTBOX_TOMBSTONES};
use crate::vnext_record_provenance::{
    MAX_PROVENANCE_OBSERVATIONS, MAX_SOURCE_PEERS_PER_RECORD, MAX_TYPED_DELTA_PAGE_RECORDS,
    MAX_TYPED_PROVENANCE_PREFIXES, MAX_TYPED_PROVENANCE_RECORDS,
};

fn profile() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../test-vectors/vnext/dr-m5-resource-admission-v1.json"
    );
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn u64_at(value: &Value, key: &str) -> u64 {
    value[key]
        .as_u64()
        .unwrap_or_else(|| panic!("profile field {key} must be an unsigned integer"))
}

fn stage(id: &str) -> AdmissionStage {
    match id {
        "frame" => AdmissionStage::Frame,
        "protocol" => AdmissionStage::Protocol,
        "journal" => AdmissionStage::Journal,
        "application" => AdmissionStage::Application,
        other => panic!("unexpected admission stage {other}"),
    }
}

#[test]
fn m5_01_resource_profile_binds_lanes_and_admission_pipeline() {
    let profile = profile();
    assert_eq!(profile["format"], "onebrain/dr-m5-resource-admission/1");
    assert_eq!(profile["claims_network_completion"], false);

    let lanes = profile["allocation_lanes"].as_array().unwrap();
    let ids: Vec<&str> = lanes
        .iter()
        .map(|lane| lane["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        ["session_control", "carrier_frame", "protocol_payload"]
    );
    for lane in lanes {
        let resource_lane = match lane["id"].as_str().unwrap() {
            "session_control" => ResourceLane::SessionControl,
            "carrier_frame" => ResourceLane::CarrierFrame,
            "protocol_payload" => ResourceLane::ProtocolPayload,
            other => panic!("unexpected lane {other}"),
        };
        let max = u64_at(lane, "max_bytes");
        assert_eq!(resource_lane.max_bytes(), max);
        assert_eq!(admit_length_prefix(max, resource_lane), Ok(max as usize));
        assert_eq!(
            admit_length_prefix(max + 1, resource_lane),
            Err(LengthAdmissionError::LaneLimit)
        );
        assert_eq!(
            admit_length_prefix(0, resource_lane),
            Err(LengthAdmissionError::Empty)
        );
        if lane["prefix_checked_before_allocation"] == true {
            // The QUIC carrier reads its length prefix against this lane cap
            // before allocating (`recv_length_prefixed_uni`, ku-net test).
            assert_eq!(MAX_QUIC_FRAME_BYTES as u64, max);
        }
    }

    // The ordered pipeline is enforced by a real record admission: every
    // later stage must follow its predecessor, and none can be skipped.
    let pipeline: Vec<&str> = profile["pipeline"]
        .as_array()
        .unwrap()
        .iter()
        .map(|stage| stage.as_str().unwrap())
        .collect();
    assert_eq!(pipeline.first(), Some(&"stream_read"));
    let controller =
        RuntimeAdmissionController::new(admission_limits(VNextNetworkPolicy::default())).unwrap();
    let session = controller
        .try_begin_handshake(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .unwrap()
        .promote(NodeId::from_bytes([0x51; 32]))
        .unwrap();
    for skipped in 1..pipeline.len() {
        let mut record = session.begin_record(64).unwrap();
        assert_eq!(record.stage(), AdmissionStage::StreamRead);
        for next in &pipeline[1..skipped] {
            record.advance(stage(next), 1).unwrap();
        }
        if skipped + 1 < pipeline.len() {
            assert_eq!(
                record.advance(stage(pipeline[skipped + 1]), 1),
                Err(ResourceAdmissionError::StageOrder),
                "stage {} must not be skippable",
                pipeline[skipped]
            );
        }
    }
    let mut record = session.begin_record(64).unwrap();
    for next in &pipeline[1..] {
        assert!(!record.is_complete());
        record.advance(stage(next), 1).unwrap();
    }
    assert!(record.is_complete());
}

#[test]
fn m5_01_resource_profile_binds_default_quotas_to_the_runtime_controller() {
    let profile = profile();
    let quotas = &profile["default_quotas"];
    let policy = VNextNetworkPolicy::default();
    let limits = RuntimeAdmissionController::new(admission_limits(policy))
        .unwrap()
        .limits();

    assert_eq!(
        limits.max_handshakes_global,
        u64_at(quotas, "handshakes_global")
    );
    assert_eq!(
        limits.max_handshakes_per_ip,
        u64_at(quotas, "handshakes_per_ip")
    );
    assert_eq!(
        limits.max_sessions_global,
        u64_at(quotas, "sessions_global")
    );
    assert_eq!(
        limits.max_sessions_per_ip,
        u64_at(quotas, "sessions_per_ip")
    );
    assert_eq!(
        limits.max_sessions_per_peer,
        u64_at(quotas, "sessions_per_node_id")
    );
    assert_eq!(
        limits.max_contexts_per_session,
        u64_at(quotas, "contexts_per_session")
    );
    assert_eq!(
        limits.rate_window.as_secs(),
        u64_at(quotas, "rate_window_seconds")
    );
    assert_eq!(
        limits.per_session.records,
        u64_at(quotas, "records_per_session")
    );
    assert_eq!(
        limits.per_session.bytes,
        u64_at(quotas, "bytes_per_session")
    );
    assert_eq!(limits.per_session.work, u64_at(quotas, "work_per_session"));
    let per_peer = limits.per_peer_per_window;
    assert_eq!(
        per_peer.records,
        u64_at(quotas, "records_per_node_id_window")
    );
    assert_eq!(per_peer.bytes, u64_at(quotas, "bytes_per_node_id_window"));
    assert_eq!(per_peer.work, u64_at(quotas, "work_per_node_id_window"));

    assert_eq!(
        quotas["per_ip_window_derivation"],
        "per_node_id_window_times_sessions_per_ip"
    );
    let per_ip_factor = u64_at(quotas, "sessions_per_ip");
    assert_eq!(
        limits.per_ip_per_window.records,
        per_peer.records * per_ip_factor
    );
    assert_eq!(
        limits.per_ip_per_window.bytes,
        per_peer.bytes * per_ip_factor
    );
    assert_eq!(limits.per_ip_per_window.work, per_peer.work * per_ip_factor);
    assert_eq!(
        quotas["global_window_derivation"],
        "per_node_id_window_times_sessions_global"
    );
    let global_factor = u64_at(quotas, "sessions_global");
    assert_eq!(
        limits.global_per_window.records,
        per_peer.records * global_factor
    );
    assert_eq!(
        limits.global_per_window.bytes,
        per_peer.bytes * global_factor
    );
    assert_eq!(limits.global_per_window.work, per_peer.work * global_factor);

    let replay_entries = u64_at(quotas, "replay_entries") as usize;
    assert_eq!(policy.max_replay_entries, replay_entries);
    assert_eq!(DEFAULT_SESSION_REPLAY_ENTRIES, replay_entries);
    assert_eq!(SessionReplayGuard::default().capacity(), replay_entries);
    assert_eq!(
        SessionReplayGuard::with_capacity(policy.max_replay_entries)
            .unwrap()
            .capacity(),
        replay_entries
    );
}

#[test]
fn m5_01_resource_profile_binds_durable_bounds_scans_and_outbox_states() {
    let profile = profile();
    let state = &profile["bounded_durable_state"];
    let expected_keys: BTreeSet<&str> = [
        "proposal_quarantine_records",
        "accepted_records",
        "verified_quarantine_records",
        "inventory_records",
        "provenance_observations",
        "typed_provenance_records",
        "typed_provenance_prefixes",
        "source_peers_per_record",
        "distributed_kql_matches",
        "outbox_records",
        "outbox_tombstones",
        "storage_soft_watermark_bytes",
        "storage_hard_watermark_bytes",
    ]
    .into_iter()
    .collect();
    let keys: BTreeSet<&str> = state
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys, expected_keys,
        "every durable bound must be bound below"
    );

    assert_eq!(
        MAX_PROPOSAL_QUARANTINE_RECORDS as u64,
        u64_at(state, "proposal_quarantine_records")
    );
    assert_eq!(
        MAX_VERIFIED_ACCEPTED_RECORDS,
        u64_at(state, "accepted_records")
    );
    assert_eq!(
        MAX_VERIFIED_QUARANTINE_RECORDS,
        u64_at(state, "verified_quarantine_records")
    );
    assert_eq!(
        MAX_INVENTORY_RECORDS as u64,
        u64_at(state, "inventory_records")
    );
    assert_eq!(
        MAX_PROVENANCE_OBSERVATIONS,
        u64_at(state, "provenance_observations")
    );
    assert_eq!(
        MAX_TYPED_PROVENANCE_RECORDS,
        u64_at(state, "typed_provenance_records")
    );
    assert_eq!(
        MAX_TYPED_PROVENANCE_PREFIXES,
        u64_at(state, "typed_provenance_prefixes")
    );
    assert_eq!(
        MAX_SOURCE_PEERS_PER_RECORD as u64,
        u64_at(state, "source_peers_per_record")
    );
    assert_eq!(
        MAX_DURABLE_KQL_MATCHES,
        u64_at(state, "distributed_kql_matches")
    );
    assert_eq!(MAX_OUTBOX_RECORDS, u64_at(state, "outbox_records"));
    assert_eq!(MAX_OUTBOX_TOMBSTONES, u64_at(state, "outbox_tombstones"));
    let budgets = VNextRuntimeBudgets::default();
    assert_eq!(
        budgets.storage_soft_watermark_bytes,
        u64_at(state, "storage_soft_watermark_bytes")
    );
    assert_eq!(
        budgets.storage_hard_watermark_bytes,
        u64_at(state, "storage_hard_watermark_bytes")
    );

    let scans = &profile["incremental_scans"];
    assert_eq!(
        MAX_TYPED_DELTA_PAGE_RECORDS as u64,
        u64_at(scans, "max_page_records")
    );

    let outbox = &profile["outbox"];
    let states: Vec<String> = [
        OutboundIntentState::Pending,
        OutboundIntentState::Acknowledged,
        OutboundIntentState::DeadLetter,
        OutboundIntentState::RetryExhausted,
    ]
    .into_iter()
    .enumerate()
    .map(|(discriminant, state)| {
        assert_eq!(state as u8 as usize, discriminant);
        assert_eq!(state.is_terminal(), discriminant != 0);
        format!("{state:?}")
    })
    .collect();
    assert_eq!(outbox["states"], serde_json::json!(states));
    // One fair-delivery quantum inspects at most the hard store cap.
    assert_eq!(
        MAX_OUTBOX_RECORDS,
        u64_at(outbox, "max_records_inspected_per_quantum")
    );
}
