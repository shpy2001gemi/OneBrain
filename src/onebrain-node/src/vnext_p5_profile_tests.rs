//! ARCH-13 slice 10: bind the frozen P5 canary and operations preflight
//! profiles (`p5-canary-preflight-v1.json`, `p5-operations-preflight-v1.json`)
//! to the harness constants and operator vocabularies, instead of checking
//! that names merely appear in source.

use serde_json::Value;

use crate::vnext_canary_operations::{
    P5_CANARY_NODE_COUNT, P5_CANARY_PREFLIGHT_PROFILE, P5_CANARY_RING_DELIVERIES,
    P5_CANARY_ROUTE_OBSERVATIONS,
};
use crate::vnext_p5_operations::{
    P5OperatorIncident, P5_OPERATIONS_PREFLIGHT_PROFILE, P5_PROTECTED_DURABLE_FILES,
};
use crate::vnext_runtime_rollout::VNextRuntimeLane;

fn vector(name: &str) -> Value {
    let path = format!(
        "{}/../test-vectors/vnext/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("profile field must be an array: {value}"))
        .iter()
        .map(|item| item.as_str().unwrap().to_owned())
        .collect()
}

fn u64_at(value: &Value, key: &str) -> u64 {
    value[key]
        .as_u64()
        .unwrap_or_else(|| panic!("profile field {key} must be an unsigned integer"))
}

/// Exhaustive on purpose: a new incident code must be added to the profile.
fn incident_index(incident: P5OperatorIncident) -> usize {
    match incident {
        P5OperatorIncident::SignerUnavailable => 0,
        P5OperatorIncident::RegistryCorrupt => 1,
        P5OperatorIncident::StorageSoftWatermark => 2,
        P5OperatorIncident::StorageRejected => 3,
        P5OperatorIncident::PendingOutbox => 4,
        P5OperatorIncident::RetryExhaustedOutbox => 5,
        P5OperatorIncident::ActiveJournal => 6,
        P5OperatorIncident::QuarantinePresent => 7,
        P5OperatorIncident::LaneFenced => 8,
        P5OperatorIncident::RollbackActive => 9,
    }
}

#[test]
fn p5_01_canary_profile_binds_topology_constants() {
    let profile = vector("p5-canary-preflight-v1.json");
    assert_eq!(profile["format"], P5_CANARY_PREFLIGHT_PROFILE);
    let scope = &profile["scope"];
    assert_eq!(
        u64_at(scope, "logical_node_count"),
        P5_CANARY_NODE_COUNT as u64
    );
    assert_eq!(scope["production_canary_qualifying"], false);
    let topology = &profile["topology"];
    assert_eq!(
        u64_at(topology, "independent_durable_directories"),
        P5_CANARY_NODE_COUNT as u64
    );
    assert_eq!(
        u64_at(topology, "independent_principals"),
        P5_CANARY_NODE_COUNT as u64
    );
    assert_eq!(
        u64_at(topology, "ring_deliveries"),
        P5_CANARY_RING_DELIVERIES as u64
    );
    assert_eq!(
        u64_at(topology, "minimum_authenticated_route_observations"),
        P5_CANARY_ROUTE_OBSERVATIONS as u64
    );
}

#[test]
fn p5_operations_profile_binds_backup_lanes_and_incident_codes() {
    let profile = vector("p5-operations-preflight-v1.json");
    assert_eq!(profile["format"], P5_OPERATIONS_PREFLIGHT_PROFILE);
    assert_eq!(profile["scope"]["production_canary_qualifying"], false);

    assert_eq!(
        strings(&profile["p5_03_backup_restore"]["required_durable_files"]),
        P5_PROTECTED_DURABLE_FILES
    );

    let lanes: Vec<_> = VNextRuntimeLane::ALL.map(VNextRuntimeLane::name).to_vec();
    assert_eq!(strings(&profile["p5_04_rollback_reenable"]["lanes"]), lanes);
    assert_eq!(
        u64_at(&profile["p5_05_default_off_rollout"], "runtime_lane_count"),
        VNextRuntimeLane::ALL.len() as u64
    );

    let codes = strings(&profile["p5_06_operator_dashboard"]["incident_codes"]);
    let incidents = [
        P5OperatorIncident::SignerUnavailable,
        P5OperatorIncident::RegistryCorrupt,
        P5OperatorIncident::StorageSoftWatermark,
        P5OperatorIncident::StorageRejected,
        P5OperatorIncident::PendingOutbox,
        P5OperatorIncident::RetryExhaustedOutbox,
        P5OperatorIncident::ActiveJournal,
        P5OperatorIncident::QuarantinePresent,
        P5OperatorIncident::LaneFenced,
        P5OperatorIncident::RollbackActive,
    ];
    assert_eq!(codes.len(), incidents.len());
    for incident in incidents {
        assert_eq!(
            serde_json::to_value(incident).unwrap(),
            Value::String(codes[incident_index(incident)].clone())
        );
    }
}
