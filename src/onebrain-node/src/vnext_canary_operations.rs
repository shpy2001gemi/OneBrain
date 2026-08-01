//! P5-01 three-node authenticated-QUIC canary preflight.
//!
//! This harness is deliberately narrower than a production canary. It runs
//! three independent logical nodes on one host, proves a real authenticated
//! QUIC ring, then exercises partition, durable restart, authenticated route
//! replacement, and idempotent reunion replay. A passing report never claims
//! multi-host or release qualification.

#![cfg(feature = "vnext-canary-harness")]

use std::fs;
use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use ku_core::foundation::{
    decode_feed_inception, DeviceId, DisclosureClass, FeedId, FeedInception, NamespaceCommitment,
    NodeId, SelectorCid,
};
use ku_net::vnext_carrier::CarrierRecord;
use ku_net::vnext_reconciliation::BoundPayloadFrame;
use onebrain_protocol::{
    bind_reconciliation_message, encode_reconciliation_message, ReconcileManifestEntry,
    ReconcileManifestKind, ReconciliationBody, ReconciliationBudget, ReconciliationContext,
    ReconciliationResumeMode, ReconciliationSummaryMethod,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::vnext_config::VNextNetworkPolicy;
use crate::vnext_network_runtime::{
    VNextNetworkRuntime, VNextNetworkRuntimeError, VNextNetworkRuntimeState,
};

pub const P5_CANARY_PREFLIGHT_PROFILE: &str = "onebrain/p5-canary-preflight/1";
pub const P5_CANARY_NODE_COUNT: usize = 3;
pub const P5_CANARY_RING_DELIVERIES: usize = 3;
pub const P5_CANARY_ROUTE_OBSERVATIONS: usize = 6;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct P5CanaryPreflightReport {
    pub profile: String,
    pub scope: String,
    pub transport: String,
    pub node_count: usize,
    pub distinct_principals: usize,
    pub initial_ring_deliveries: usize,
    pub authenticated_route_observations: usize,
    pub partition_rejected_old_route: bool,
    pub restarted_principal_stable: bool,
    pub route_address_changed: bool,
    pub route_generation_advanced: bool,
    pub durable_feed_branches_before_restart: usize,
    pub durable_feed_branches_after_replay: usize,
    pub active_sessions_after_quiescence: usize,
    pub changes_wallet_state: bool,
    pub changes_obt_state: bool,
    pub grants_authority: bool,
    pub claims_network_completion: bool,
    pub production_canary_qualified: bool,
    pub preflight_passed: bool,
}

impl P5CanaryPreflightReport {
    pub fn passes(&self) -> bool {
        self.preflight_passed && self.oracle_passes()
    }

    fn oracle_passes(&self) -> bool {
        self.profile == P5_CANARY_PREFLIGHT_PROFILE
            && self.scope == "single-host-three-logical-node"
            && self.transport == "authenticated-real-quic-loopback"
            && self.node_count == P5_CANARY_NODE_COUNT
            && self.distinct_principals == P5_CANARY_NODE_COUNT
            && self.initial_ring_deliveries == P5_CANARY_RING_DELIVERIES
            && self.authenticated_route_observations >= P5_CANARY_ROUTE_OBSERVATIONS
            && self.partition_rejected_old_route
            && self.restarted_principal_stable
            && self.route_address_changed
            && self.route_generation_advanced
            && self.durable_feed_branches_before_restart == 1
            && self.durable_feed_branches_after_replay == 1
            && self.active_sessions_after_quiescence == 0
            && !self.changes_wallet_state
            && !self.changes_obt_state
            && !self.grants_authority
            && !self.claims_network_completion
            && !self.production_canary_qualified
    }
}

pub async fn run_p5_canary_preflight(
    data_root: &Path,
) -> Result<P5CanaryPreflightReport, P5CanaryPreflightError> {
    let [node_a_dir, node_b_dir, node_c_dir] = prepare_node_directories(data_root)?;
    let bind_addr = "127.0.0.1:0"
        .parse::<SocketAddr>()
        .map_err(|error| P5CanaryPreflightError::Fixture(error.to_string()))?;
    let policy = VNextNetworkPolicy::default();
    let mut node_a = VNextNetworkRuntime::start(&node_a_dir, bind_addr, policy).await?;
    let node_b = VNextNetworkRuntime::start(&node_b_dir, bind_addr, policy).await?;
    let mut node_b = Some(node_b);
    let mut node_c = VNextNetworkRuntime::start(&node_c_dir, bind_addr, policy).await?;

    let principal_a = NodeId::from_bytes(node_a.status().principal);
    let principal_b = NodeId::from_bytes(
        node_b
            .as_ref()
            .ok_or(P5CanaryPreflightError::Fixture(
                "node B missing before ring delivery".to_owned(),
            ))?
            .status()
            .principal,
    );
    let principal_c = NodeId::from_bytes(node_c.status().principal);
    let distinct_principals = usize::from(principal_a != principal_b && principal_a != principal_c)
        + usize::from(principal_b != principal_c)
        + 1;

    let (feed_ab, feed_ab_bytes) = signed_feed(0x31)?;
    let (feed_bc, feed_bc_bytes) = signed_feed(0x41)?;
    let (feed_ca, feed_ca_bytes) = signed_feed(0x51)?;

    let node_b_ref = node_b.as_ref().ok_or(P5CanaryPreflightError::Fixture(
        "node B missing during ring delivery".to_owned(),
    ))?;
    send_feed(&node_a, node_b_ref, 0x61, &feed_ab_bytes, feed_ab).await?;
    send_feed(node_b_ref, &node_c, 0x62, &feed_bc_bytes, feed_bc).await?;
    send_feed(&node_c, &node_a, 0x63, &feed_ca_bytes, feed_ca).await?;
    wait_for_no_active_sessions(&node_a).await?;
    wait_for_no_active_sessions(node_b_ref).await?;
    wait_for_no_active_sessions(&node_c).await?;

    let authenticated_route_observations = node_a
        .authenticated_route_count()?
        .checked_add(node_b_ref.authenticated_route_count()?)
        .and_then(|count| count.checked_add(node_c.authenticated_route_count().ok()?))
        .ok_or_else(|| {
            P5CanaryPreflightError::Fixture(
                "authenticated route observation count overflowed".to_owned(),
            )
        })?;
    let durable_feed_branches_before_restart = node_b_ref.feed_inception_branch_count(feed_ab)?;
    let route_before =
        node_a
            .authenticated_route(principal_b)?
            .ok_or(P5CanaryPreflightError::Fixture(
                "node A did not retain authenticated route to node B".to_owned(),
            ))?;

    let mut stopped_b = node_b.take().ok_or(P5CanaryPreflightError::Fixture(
        "node B missing before partition".to_owned(),
    ))?;
    let old_addr = stopped_b.local_addr();
    stopped_b.shutdown().await;
    drop(stopped_b);

    let old_route_guard = bind_old_route(old_addr).await?;
    let partition_attempt =
        tokio::time::timeout(Duration::from_millis(750), node_a.connect(old_addr)).await;
    let partition_rejected_old_route = !matches!(partition_attempt, Ok(Ok(_)));
    if !partition_rejected_old_route {
        return Err(P5CanaryPreflightError::Oracle(
            "partitioned old route accepted a new authenticated session",
        ));
    }

    let restarted_b = VNextNetworkRuntime::start(&node_b_dir, bind_addr, policy).await?;
    let restarted_addr = restarted_b.local_addr();
    drop(old_route_guard);
    let restarted_principal = NodeId::from_bytes(restarted_b.status().principal);
    let restarted_principal_stable = restarted_principal == principal_b;
    let route_address_changed = restarted_addr != old_addr;

    send_feed(&node_a, &restarted_b, 0x64, &feed_ab_bytes, feed_ab).await?;
    wait_for_no_active_sessions(&node_a).await?;
    wait_for_no_active_sessions(&restarted_b).await?;
    let durable_feed_branches_after_replay = restarted_b.feed_inception_branch_count(feed_ab)?;
    let route_after =
        node_a
            .authenticated_route(principal_b)?
            .ok_or(P5CanaryPreflightError::Fixture(
                "node A did not replace the restarted node B route".to_owned(),
            ))?;
    let route_generation_advanced =
        route_after.generation > route_before.generation && route_after.addr == restarted_addr;

    let active_sessions_after_quiescence = node_a
        .status()
        .active_sessions
        .checked_add(restarted_b.status().active_sessions)
        .and_then(|count| count.checked_add(node_c.status().active_sessions))
        .ok_or_else(|| {
            P5CanaryPreflightError::Fixture("active session count overflowed".to_owned())
        })?;
    let claims_network_completion = node_a.status().claims_network_completion
        || restarted_b.status().claims_network_completion
        || node_c.status().claims_network_completion;

    node_b = Some(restarted_b);
    if let Some(node_b) = node_b.as_mut() {
        node_b.shutdown().await;
    }
    node_a.shutdown().await;
    node_c.shutdown().await;

    if node_a.status().state != VNextNetworkRuntimeState::Stopped
        || node_b
            .as_ref()
            .is_some_and(|node| node.status().state != VNextNetworkRuntimeState::Stopped)
        || node_c.status().state != VNextNetworkRuntimeState::Stopped
    {
        return Err(P5CanaryPreflightError::Oracle(
            "canary runtime did not stop cleanly",
        ));
    }

    let mut report = P5CanaryPreflightReport {
        profile: P5_CANARY_PREFLIGHT_PROFILE.to_owned(),
        scope: "single-host-three-logical-node".to_owned(),
        transport: "authenticated-real-quic-loopback".to_owned(),
        node_count: P5_CANARY_NODE_COUNT,
        distinct_principals,
        initial_ring_deliveries: P5_CANARY_RING_DELIVERIES,
        authenticated_route_observations,
        partition_rejected_old_route,
        restarted_principal_stable,
        route_address_changed,
        route_generation_advanced,
        durable_feed_branches_before_restart,
        durable_feed_branches_after_replay,
        active_sessions_after_quiescence,
        changes_wallet_state: false,
        changes_obt_state: false,
        grants_authority: false,
        claims_network_completion,
        production_canary_qualified: false,
        preflight_passed: false,
    };
    report.preflight_passed = report.oracle_passes();
    if !report.preflight_passed {
        return Err(P5CanaryPreflightError::Oracle(
            "three-node preflight report failed a frozen oracle",
        ));
    }
    Ok(report)
}

fn prepare_node_directories(
    data_root: &Path,
) -> Result<[PathBuf; P5_CANARY_NODE_COUNT], P5CanaryPreflightError> {
    fs::create_dir_all(data_root)?;
    let directories = [
        data_root.join("node-a"),
        data_root.join("node-b"),
        data_root.join("node-c"),
    ];
    for directory in &directories {
        if directory.is_file() || (directory.is_dir() && fs::read_dir(directory)?.next().is_some())
        {
            return Err(P5CanaryPreflightError::DataDirectoryNotEmpty(
                directory.clone(),
            ));
        }
        fs::create_dir_all(directory)?;
    }
    Ok(directories)
}

async fn bind_old_route(addr: SocketAddr) -> Result<UdpSocket, P5CanaryPreflightError> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match UdpSocket::bind(addr) {
            Ok(socket) => return Ok(socket),
            Err(error) if Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(20)).await;
                if error.kind() == std::io::ErrorKind::PermissionDenied {
                    return Err(error.into());
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
}

pub(crate) async fn send_feed(
    sender: &VNextNetworkRuntime,
    receiver: &VNextNetworkRuntime,
    marker: u8,
    feed_bytes: &[u8],
    feed_id: FeedId,
) -> Result<(), P5CanaryPreflightError> {
    let accepted_before = receiver.status().accepted_records;
    let session = sender.connect(receiver.local_addr()).await?;
    let context = canary_context(session.authenticated().session_id, marker);
    let frame = BoundPayloadFrame::new(
        &context,
        ReconcileManifestKind::FeedInception,
        feed_bytes.to_vec(),
    )
    .map_err(protocol)?;
    let manifest = bind_reconciliation_message(
        context,
        1,
        ReconciliationBody::Manifest {
            entries: vec![ReconcileManifestEntry {
                kind: frame.kind,
                cid: frame.cid,
                canonical_length: frame.canonical_bytes.len() as u64,
            }],
        },
    )
    .map_err(protocol)?;
    let manifest_record = CarrierRecord::reconciliation_message(
        &encode_reconciliation_message(&manifest).map_err(protocol)?,
    )
    .map_err(|error| P5CanaryPreflightError::Protocol(format!("{error:?}")))?;
    session.send(&manifest_record).await?;
    session.send(&CarrierRecord::BoundPayload(frame)).await?;
    wait_for_feed_acceptance(receiver, feed_id, accepted_before).await?;
    session.close();
    drop(session);
    Ok(())
}

async fn wait_for_feed_acceptance(
    runtime: &VNextNetworkRuntime,
    feed_id: FeedId,
    accepted_before: u64,
) -> Result<usize, P5CanaryPreflightError> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let count = runtime.feed_inception_branch_count(feed_id)?;
            if count >= 1 && runtime.status().accepted_records > accepted_before {
                return Ok(count);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| P5CanaryPreflightError::Timeout("durable feed acceptance"))?
}

pub(crate) async fn wait_for_no_active_sessions(
    runtime: &VNextNetworkRuntime,
) -> Result<(), P5CanaryPreflightError> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut zero_since = None;
        loop {
            if runtime.status().active_sessions == 0 {
                let since = zero_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= Duration::from_millis(250) {
                    return;
                }
            } else {
                zero_since = None;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| P5CanaryPreflightError::Timeout("active session quiescence"))
}

pub(crate) fn signed_feed(marker: u8) -> Result<(FeedId, Vec<u8>), P5CanaryPreflightError> {
    let key = SigningKey::from_bytes(&[marker; 32]);
    let namespace = NamespaceCommitment::derive(
        b"onebrain:p5:canary-preflight:feed:1",
        [marker.wrapping_add(1); 32],
    )
    .map_err(protocol)?;
    let feed = FeedInception::new(
        *key.verifying_key().as_bytes(),
        namespace,
        0,
        DeviceId::from_bytes([marker.wrapping_add(2); 32]),
    )
    .sign(&key)
    .map_err(protocol)?;
    let bytes = feed.encode().map_err(protocol)?;
    let feed_id = decode_feed_inception(&bytes).map_err(protocol)?.feed_id;
    Ok((feed_id, bytes))
}

pub(crate) fn canary_context(session_id: [u8; 32], marker: u8) -> ReconciliationContext {
    ReconciliationContext {
        authenticated_transcript: session_id,
        selector: SelectorCid::from_bytes([marker; 32]),
        namespace: NamespaceCommitment::from_bytes([marker.wrapping_add(1); 32]),
        disclosure: DisclosureClass::Public,
        summary_method: ReconciliationSummaryMethod::RadixForest256V1,
        budget: ReconciliationBudget {
            max_summary_nodes: 8,
            max_diff_ranges: 8,
            max_manifest_entries: 8,
            max_payload_bytes: 4096,
        },
        resume_mode: ReconciliationResumeMode::BoundTokenV1,
    }
}

fn protocol(error: impl std::fmt::Debug) -> P5CanaryPreflightError {
    P5CanaryPreflightError::Protocol(format!("{error:?}"))
}

#[derive(Debug, Error)]
pub enum P5CanaryPreflightError {
    #[error("P5-01 canary data directory must be empty: {}", .0.display())]
    DataDirectoryNotEmpty(PathBuf),
    #[error("P5-01 canary network runtime failed: {0}")]
    Network(#[from] VNextNetworkRuntimeError),
    #[error("P5-01 canary protocol fixture failed: {0}")]
    Protocol(String),
    #[error("P5-01 canary fixture failed: {0}")]
    Fixture(String),
    #[error("P5-01 canary timed out waiting for {0}")]
    Timeout(&'static str),
    #[error("P5-01 canary oracle failed: {0}")]
    Oracle(&'static str),
    #[error("P5-01 canary filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn p5_01_three_node_real_quic_partition_restart_route_change_reunion() {
        let directory = tempfile::tempdir().unwrap();
        let report = run_p5_canary_preflight(directory.path()).await.unwrap();
        assert!(report.passes());
        assert_eq!(report.node_count, 3);
        assert_eq!(report.distinct_principals, 3);
        assert_eq!(report.initial_ring_deliveries, 3);
        assert!(report.authenticated_route_observations >= 6);
        assert!(report.partition_rejected_old_route);
        assert!(report.restarted_principal_stable);
        assert!(report.route_address_changed);
        assert!(report.route_generation_advanced);
        assert_eq!(report.durable_feed_branches_before_restart, 1);
        assert_eq!(report.durable_feed_branches_after_replay, 1);
        assert_eq!(report.active_sessions_after_quiescence, 0);
        assert!(!report.production_canary_qualified);
        assert!(serde_json::to_vec(&report).unwrap().len() < 4096);
    }

    #[tokio::test]
    async fn p5_01_nonempty_node_directory_fails_before_runtime_start() {
        let directory = tempfile::tempdir().unwrap();
        let node_a = directory.path().join("node-a");
        fs::create_dir_all(&node_a).unwrap();
        fs::write(node_a.join("operator-owned.bin"), b"preserve").unwrap();
        assert!(matches!(
            run_p5_canary_preflight(directory.path()).await,
            Err(P5CanaryPreflightError::DataDirectoryNotEmpty(path)) if path == node_a
        ));
        assert_eq!(
            fs::read(node_a.join("operator-owned.bin")).unwrap(),
            b"preserve"
        );
        assert!(!directory.path().join("node-b").exists());
        assert!(!directory.path().join("node-c").exists());
    }
}
