//! ARCH-13 slice 8: bind the frozen M5-06 mixed-version rollback profile
//! (`dr-m5-mixed-rollback-v1.json`) to the runtime rollout store, instead of
//! checking that names merely appear in source.

use ku_core::foundation::dr_m5_failpoint::FAILPOINT_PHASES;
use serde_json::Value;

use crate::vnext_runtime_rollout::{
    VNextRuntimeLane, VNextRuntimeLaneRequest, VNextRuntimeRollout, VNextRuntimeRolloutError,
    TX_RUNTIME_ROLLBACK,
};

fn profile() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../test-vectors/vnext/dr-m5-mixed-rollback-v1.json"
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

fn no_lanes() -> VNextRuntimeLaneRequest {
    VNextRuntimeLaneRequest {
        network: false,
        distributed_kql: false,
        public_use_evidence_publish: false,
        distributed_pomv_view: false,
    }
}

fn only(lane: VNextRuntimeLane) -> VNextRuntimeLaneRequest {
    VNextRuntimeLaneRequest {
        network: lane == VNextRuntimeLane::Network,
        distributed_kql: lane == VNextRuntimeLane::DistributedKql,
        public_use_evidence_publish: lane == VNextRuntimeLane::PublicUseEvidencePublish,
        distributed_pomv_view: lane == VNextRuntimeLane::DistributedPomvView,
    }
}

#[test]
fn m5_06_rollback_profile_binds_lanes_boundary_and_phases() {
    let profile = profile();
    assert_eq!(profile["format"], "onebrain/dr-m5-mixed-rollback/1");
    let fence = &profile["runtime_generation_fence"];
    let names: Vec<_> = VNextRuntimeLane::ALL.map(VNextRuntimeLane::name).to_vec();
    assert_eq!(strings(&fence["lanes"]), names);

    let process_kill = &profile["process_kill"];
    assert_eq!(process_kill["boundary"], TX_RUNTIME_ROLLBACK);
    assert_eq!(strings(&process_kill["phases"]), FAILPOINT_PHASES);
    assert_eq!(
        process_kill["required_process_kill_cases"].as_u64(),
        Some(FAILPOINT_PHASES.len() as u64)
    );
}

#[test]
fn m5_06_rollback_profile_generation_fence_rules_hold_for_every_lane() {
    let profile = profile();
    let fence = &profile["runtime_generation_fence"];
    assert_eq!(fence["kill_is_idempotent"], true);
    assert_eq!(fence["reenable_advances_generation"], true);
    assert_eq!(fence["explicit_reenable_required"], true);
    assert_eq!(fence["startup_config_may_disable"], true);
    assert_eq!(fence["startup_config_may_reenable"], false);

    for lane in VNextRuntimeLane::ALL {
        let directory = tempfile::tempdir().unwrap();
        let all = VNextRuntimeLaneRequest::all_enabled();
        let rollout = VNextRuntimeRollout::open(directory.path(), all, no_lanes()).unwrap();
        let admitted = rollout.acquire(lane).unwrap();

        // Kill fences existing leases and is idempotent.
        let killed = rollout.kill(lane).unwrap();
        assert!(!killed.enabled, "{}", lane.name());
        assert!(!admitted.is_current(), "{}", lane.name());
        assert_eq!(rollout.kill(lane).unwrap(), killed, "{}", lane.name());
        drop(rollout);

        // Startup configuration cannot re-enable a durably killed lane.
        let restarted = VNextRuntimeRollout::open(directory.path(), all, no_lanes()).unwrap();
        assert!(matches!(
            restarted.acquire(lane),
            Err(VNextRuntimeRolloutError::LaneFenced { .. })
        ));

        // Only an explicit re-enable reopens it, on a later generation.
        let enabled = restarted.reenable(lane).unwrap();
        assert!(enabled.enabled);
        assert!(enabled.generation > killed.generation);
        assert!(restarted.acquire(lane).unwrap().is_current());
        assert!(!admitted.is_current());
        drop(restarted);

        // Startup configuration may disable a lane.
        let configured = VNextRuntimeRollout::open(directory.path(), all, only(lane)).unwrap();
        assert!(!configured.snapshot().unwrap().lane(lane).enabled);
    }
}

#[test]
fn m5_06_rollback_profile_rollback_is_atomic_and_preserves_wallet_and_obt() {
    let profile = profile();
    let rollback = &profile["rollback"];
    assert_eq!(rollback["atomic_all_lane_disable"], true);
    assert_eq!(rollback["stale_config_reenable"], false);

    let directory = tempfile::tempdir().unwrap();
    let all = VNextRuntimeLaneRequest::all_enabled();
    let rollout = VNextRuntimeRollout::open(directory.path(), all, no_lanes()).unwrap();
    let snapshot = rollout.rollback().unwrap();
    assert!(snapshot.lanes.iter().all(|lane| !lane.enabled));
    assert_eq!(
        Value::Bool(snapshot.changes_wallet_state),
        rollback["changes_wallet_state"]
    );
    assert_eq!(
        Value::Bool(snapshot.changes_obt_state),
        rollback["changes_obt_state"]
    );
    drop(rollout);

    let restarted = VNextRuntimeRollout::open(directory.path(), all, no_lanes()).unwrap();
    assert!(restarted
        .snapshot()
        .unwrap()
        .lanes
        .iter()
        .all(|lane| !lane.enabled));
}
