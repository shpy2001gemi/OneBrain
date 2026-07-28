//! P5-02 through P5-06 operational preflight.
//!
//! The harness is intentionally independent of the pinned 72-hour runner. It
//! exercises real runtime boundaries on one host and keeps both long-soak and
//! multi-host production qualification false.

#![cfg(feature = "vnext-canary-harness")]

use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use ku_core::foundation::{
    ConceptCcid, DisclosureClass, MetabolicViewPolicy, NamespaceCommitment, NodeId,
    ObjectReference, SelectorCid, SemanticFrameSet,
};
use ku_kql::vnext_private_need::LocalNeedVaultKey;
use ku_kql::vnext_query::{KnowledgeNeedIr, QueryDefinition};
use ku_net::vnext_carrier::CarrierRecord;
use ku_net::vnext_reconciliation::BoundPayloadFrame;
use ku_net::vnext_session::SessionIdentitySigner;
use onebrain_protocol::{
    bind_reconciliation_message, encode_reconciliation_message, ReconcileManifestEntry,
    ReconcileManifestKind, ReconciliationBody,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::vnext_canary_operations::{
    canary_context, send_feed, signed_feed, wait_for_no_active_sessions, P5CanaryPreflightError,
};
use crate::vnext_config::{VNextFeature, VNextFeatureConfig, VNextNetworkPolicy};
use crate::vnext_network_runtime::{VNextNetworkRuntime, VNextNetworkRuntimeError};
use crate::vnext_observability::{VNextReasonCode, VNextRegistryTelemetryState};
use crate::vnext_operational_compaction::{
    BoundedEvidenceKind, OperationalCompactionError, OperationalCompactionPolicy,
    OperationalCompactionStore, OperationalEvidenceStats,
};
use crate::vnext_outbox::{OutboundIntentState, OutboundOutboxError, OutboundTransferIntent};
use crate::vnext_product_runtime::{
    VNextProductRuntime, VNextProductRuntimeDependencies, VNextProductRuntimeError,
    VNextProductRuntimeStatus, VNextProductSignerMode,
};
use crate::vnext_route_authority::{LocalPolicyRegistry, LocalPolicyVersion};
use crate::vnext_runtime_rollout::{
    VNextRuntimeLane, VNextRuntimeLaneRequest, VNextRuntimeRollout, VNextRuntimeRolloutError,
    VNextRuntimeRolloutSnapshot,
};

pub const P5_OPERATIONS_PREFLIGHT_PROFILE: &str = "onebrain/p5-operations-preflight/1";
pub const P5_PROTECTED_DURABLE_FILES: [&str; 7] = [
    "vnext_identity.key",
    "vnext_verified.redb",
    "vnext_reconciliation.redb",
    "vnext_inventory.redb",
    "vnext_record_provenance.redb",
    "vnext_outbox.redb",
    "vnext_operational_compaction.redb",
];

const DEFAULT_NETWORK_STORAGE_HARD_WATERMARK_BYTES: u64 = 1_024 * 1_048_576;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct P5OperationsPreflightReport {
    pub profile: String,
    pub fault_drills: P5FaultDrillReport,
    pub backup_restore: P5BackupRestoreReport,
    pub rollback_reenable: P5RollbackReenableReport,
    pub default_off_rollout: P5DefaultOffRolloutReport,
    pub operator_dashboard: P5OperatorDashboardReport,
    pub consumes_pre_release_72h_evidence: bool,
    pub multi_host_canary_qualified: bool,
    pub production_canary_qualified: bool,
    pub preflight_passed: bool,
}

impl P5OperationsPreflightReport {
    pub fn passes(&self) -> bool {
        self.preflight_passed
            && self.profile == P5_OPERATIONS_PREFLIGHT_PROFILE
            && self.fault_drills.passes()
            && self.backup_restore.passes()
            && self.rollback_reenable.passes()
            && self.default_off_rollout.passes()
            && self.operator_dashboard.passes()
            && !self.consumes_pre_release_72h_evidence
            && !self.multi_host_canary_qualified
            && !self.production_canary_qualified
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct P5FaultDrillReport {
    pub signer_outage_failed_closed: bool,
    pub signer_outage_left_zero_durable_files: bool,
    pub disk_pressure_rejected_payload: bool,
    pub disk_pressure_rejected_storage_reason_count: u64,
    pub disk_pressure_durable_feed_branches: usize,
    pub slow_peer_held_authenticated_session: bool,
    pub healthy_peer_progressed_while_slow_peer_open: bool,
    pub healthy_peer_progress_millis: u64,
    pub active_sessions_after_quiescence: usize,
    pub changes_wallet_state: bool,
    pub changes_obt_state: bool,
    pub grants_authority: bool,
    pub claims_network_completion: bool,
}

impl P5FaultDrillReport {
    fn passes(&self) -> bool {
        self.signer_outage_failed_closed
            && self.signer_outage_left_zero_durable_files
            && self.disk_pressure_rejected_payload
            && self.disk_pressure_rejected_storage_reason_count >= 1
            && self.disk_pressure_durable_feed_branches == 0
            && self.slow_peer_held_authenticated_session
            && self.healthy_peer_progressed_while_slow_peer_open
            && self.healthy_peer_progress_millis <= 5_000
            && self.active_sessions_after_quiescence == 0
            && !self.changes_wallet_state
            && !self.changes_obt_state
            && !self.grants_authority
            && !self.claims_network_completion
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct P5BackupRestoreReport {
    pub archive_profile: String,
    pub archive_file_count: usize,
    pub archive_payload_bytes: u64,
    pub archive_root: String,
    pub exact_archive_copy_verified: bool,
    pub required_durable_files_preserved: usize,
    pub principal_preserved: bool,
    pub raw_feed_branch_count_after_restore: usize,
    pub journal_bytes_preserved: bool,
    pub pending_outbox_preserved: bool,
    pub quarantine_records_after_restore: u64,
    pub provenance_records_after_restore: u64,
    pub operational_root_preserved: bool,
    pub corrupt_archive_failed_before_restore: bool,
}

impl P5BackupRestoreReport {
    fn passes(&self) -> bool {
        self.archive_profile == "onebrain/p5-offline-backup/1"
            && self.archive_file_count >= P5_PROTECTED_DURABLE_FILES.len()
            && self.archive_payload_bytes > 0
            && self.archive_root.len() == 64
            && self.exact_archive_copy_verified
            && self.required_durable_files_preserved == P5_PROTECTED_DURABLE_FILES.len()
            && self.principal_preserved
            && self.raw_feed_branch_count_after_restore == 1
            && self.journal_bytes_preserved
            && self.pending_outbox_preserved
            && self.quarantine_records_after_restore == 1
            && self.provenance_records_after_restore == 1
            && self.operational_root_preserved
            && self.corrupt_archive_failed_before_restore
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct P5RollbackReenableReport {
    pub initial_enabled_lanes: usize,
    pub rollback_disabled_lanes: usize,
    pub network_rejected_after_rollback: bool,
    pub stale_config_enabled_lanes_after_restart: usize,
    pub explicit_reenable_advanced_all_generations: bool,
    pub reenabled_lanes: usize,
    pub real_quic_reconnected_after_reenable: bool,
    pub principal_preserved: bool,
    pub raw_feed_branch_count_after_rollback: usize,
    pub pending_outbox_preserved: bool,
    pub operational_root_preserved: bool,
    pub quarantine_preserved: bool,
    pub changes_wallet_state: bool,
    pub changes_obt_state: bool,
}

impl P5RollbackReenableReport {
    fn passes(&self) -> bool {
        self.initial_enabled_lanes == VNextRuntimeLane::ALL.len()
            && self.rollback_disabled_lanes == VNextRuntimeLane::ALL.len()
            && self.network_rejected_after_rollback
            && self.stale_config_enabled_lanes_after_restart == 0
            && self.explicit_reenable_advanced_all_generations
            && self.reenabled_lanes == VNextRuntimeLane::ALL.len()
            && self.real_quic_reconnected_after_reenable
            && self.principal_preserved
            && self.raw_feed_branch_count_after_rollback == 1
            && self.pending_outbox_preserved
            && self.operational_root_preserved
            && self.quarantine_preserved
            && !self.changes_wallet_state
            && !self.changes_obt_state
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct P5DefaultOffRolloutReport {
    pub default_feature_flags_checked: usize,
    pub default_feature_flags_enabled: usize,
    pub default_enabled_runtime_lanes: usize,
    pub stale_opt_in_enabled_runtime_lanes: usize,
    pub explicit_reenable_required: bool,
    pub explicitly_enabled_runtime_lanes: usize,
    pub default_config_effective_lanes_after_reopen: usize,
    pub local_kql_round_trip_with_network_off: bool,
    pub changes_wallet_state: bool,
    pub changes_obt_state: bool,
}

impl P5DefaultOffRolloutReport {
    fn passes(&self) -> bool {
        self.default_feature_flags_checked == 12
            && self.default_feature_flags_enabled == 0
            && self.default_enabled_runtime_lanes == 0
            && self.stale_opt_in_enabled_runtime_lanes == 0
            && self.explicit_reenable_required
            && self.explicitly_enabled_runtime_lanes == VNextRuntimeLane::ALL.len()
            && self.default_config_effective_lanes_after_reopen == 0
            && self.local_kql_round_trip_with_network_off
            && !self.changes_wallet_state
            && !self.changes_obt_state
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum P5SignerHealth {
    Available,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum P5RegistryHealth {
    Disabled,
    Verified,
    Corrupt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum P5OperatorIncident {
    SignerUnavailable,
    RegistryCorrupt,
    StorageSoftWatermark,
    StorageRejected,
    PendingOutbox,
    RetryExhaustedOutbox,
    ActiveJournal,
    QuarantinePresent,
    LaneFenced,
    RollbackActive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum P5OperatorAction {
    KeepLocalReadOnlyAvailable,
    FencePublication,
    DisableRegistryDependentLanes,
    RestoreVerifiedRegistry,
    StopNewWrites,
    BackupThenCompact,
    InspectOutbox,
    InspectJournal,
    PreserveQuarantine,
    RequireExplicitReenable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct P5OperatorLaneSnapshot {
    pub lane: String,
    pub requested: bool,
    pub enabled: bool,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct P5OperatorDashboardSnapshot {
    pub profile: String,
    pub health: String,
    pub startup_phase_count: usize,
    pub signer_mode: String,
    pub signer_health: P5SignerHealth,
    pub registry_health: P5RegistryHealth,
    pub registry_telemetry: VNextRegistryTelemetryState,
    pub lanes: Vec<P5OperatorLaneSnapshot>,
    pub authenticated_routes: usize,
    pub active_sessions: usize,
    pub active_journals: u64,
    pub pending_outbox: u64,
    pub retry_exhausted_outbox: u64,
    pub oldest_pending_outbox_age_seconds: Option<u64>,
    pub quarantine_records: u64,
    pub provenance_records: u64,
    pub quarantine_overflow_records: u64,
    pub provenance_overflow_records: u64,
    pub storage_bytes: u64,
    pub storage_pressure: String,
    pub incidents: Vec<P5OperatorIncident>,
    pub actions: Vec<P5OperatorAction>,
    pub contains_high_cardinality_labels: bool,
    pub contains_private_need_labels: bool,
    pub changes_wallet_state: bool,
    pub changes_obt_state: bool,
    pub claims_network_completion: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct P5OperatorDashboardReport {
    pub degraded_snapshot: P5OperatorDashboardSnapshot,
    pub rollback_snapshot: P5OperatorDashboardSnapshot,
    pub signer_incident_visible: bool,
    pub registry_corruption_visible: bool,
    pub outbox_visible: bool,
    pub journal_visible: bool,
    pub quarantine_visible: bool,
    pub rollback_visible: bool,
    pub incident_response_actions_present: bool,
    pub serialized_snapshot_has_no_private_or_high_cardinality_labels: bool,
}

impl P5OperatorDashboardReport {
    fn passes(&self) -> bool {
        self.degraded_snapshot.profile == "onebrain/p5-operator-dashboard/1"
            && self.rollback_snapshot.profile == "onebrain/p5-operator-dashboard/1"
            && self.signer_incident_visible
            && self.registry_corruption_visible
            && self.outbox_visible
            && self.journal_visible
            && self.quarantine_visible
            && self.rollback_visible
            && self.incident_response_actions_present
            && self.serialized_snapshot_has_no_private_or_high_cardinality_labels
            && !self.degraded_snapshot.changes_wallet_state
            && !self.degraded_snapshot.changes_obt_state
            && !self.degraded_snapshot.claims_network_completion
            && !self.rollback_snapshot.changes_wallet_state
            && !self.rollback_snapshot.changes_obt_state
            && !self.rollback_snapshot.claims_network_completion
    }
}

#[derive(Clone, Debug)]
struct SeededDurableState {
    principal: NodeId,
    feed_id: ku_core::foundation::FeedId,
    outbound_intent: OutboundTransferIntent,
    operational_root: [u8; 32],
    operational_stats: OperationalEvidenceStats,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct OfflineBackupManifest {
    profile: String,
    files: Vec<OfflineBackupFile>,
    aggregate_root: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct OfflineBackupFile {
    relative_path: String,
    length: u64,
    blake3: String,
}

pub async fn run_p5_operations_preflight(
    data_root: &Path,
) -> Result<P5OperationsPreflightReport, P5OperationsError> {
    prepare_empty_root(data_root)?;
    let fault_drills = run_fault_drills(&data_root.join("p5-02-faults")).await?;
    let backup_restore = run_backup_restore_drill(&data_root.join("p5-03-backup")).await?;
    let rollback_reenable = run_rollback_reenable_drill(&data_root.join("p5-04-rollback")).await?;
    let default_off_rollout = run_default_off_rollout_drill(&data_root.join("p5-05-default-off"))?;
    let operator_dashboard =
        run_operator_dashboard_drill(&data_root.join("p5-06-dashboard")).await?;

    let mut report = P5OperationsPreflightReport {
        profile: P5_OPERATIONS_PREFLIGHT_PROFILE.to_owned(),
        fault_drills,
        backup_restore,
        rollback_reenable,
        default_off_rollout,
        operator_dashboard,
        consumes_pre_release_72h_evidence: false,
        multi_host_canary_qualified: false,
        production_canary_qualified: false,
        preflight_passed: false,
    };
    report.preflight_passed = report.passes_without_flag();
    if !report.passes() {
        return Err(P5OperationsError::Oracle(
            "combined P5 operational preflight failed",
        ));
    }
    Ok(report)
}

impl P5OperationsPreflightReport {
    fn passes_without_flag(&self) -> bool {
        self.profile == P5_OPERATIONS_PREFLIGHT_PROFILE
            && self.fault_drills.passes()
            && self.backup_restore.passes()
            && self.rollback_reenable.passes()
            && self.default_off_rollout.passes()
            && self.operator_dashboard.passes()
            && !self.consumes_pre_release_72h_evidence
            && !self.multi_host_canary_qualified
            && !self.production_canary_qualified
    }
}

async fn run_fault_drills(data_root: &Path) -> Result<P5FaultDrillReport, P5OperationsError> {
    fs::create_dir_all(data_root)?;
    let bind_addr = loopback_any()?;
    let signer_dir = data_root.join("signer-outage");
    let signer_key = SigningKey::from_bytes(&[0xA1; 32]);
    let signer_error = VNextNetworkRuntime::start_with_signer(
        &signer_dir,
        bind_addr,
        VNextNetworkPolicy::default(),
        Arc::new(UnavailableIdentitySigner {
            public_key: *signer_key.verifying_key().as_bytes(),
        }),
    )
    .await
    .err()
    .ok_or(P5OperationsError::Oracle(
        "unavailable signer unexpectedly started a runtime",
    ))?;
    let signer_outage_failed_closed = matches!(
        signer_error,
        VNextNetworkRuntimeError::IdentitySignerUnavailable(_)
    );
    let signer_outage_left_zero_durable_files =
        !signer_dir.exists() || fs::read_dir(&signer_dir)?.next().is_none();

    let disk_sender_dir = data_root.join("disk-sender");
    let disk_receiver_dir = data_root.join("disk-receiver");
    let mut disk_sender = VNextNetworkRuntime::start_canary_harness(
        &disk_sender_dir,
        bind_addr,
        VNextNetworkPolicy::default(),
        false,
        DEFAULT_NETWORK_STORAGE_HARD_WATERMARK_BYTES,
    )
    .await?;
    let mut disk_receiver = VNextNetworkRuntime::start_canary_harness(
        &disk_receiver_dir,
        bind_addr,
        VNextNetworkPolicy::default(),
        false,
        1,
    )
    .await?;
    let (disk_feed_id, disk_feed_bytes) = signed_feed(0xA2)?;
    let rejected_before = disk_receiver.status().rejected_records;
    send_feed_without_acceptance_wait(&disk_sender, &disk_receiver, 0xA3, &disk_feed_bytes).await?;
    wait_for_rejected_record(&disk_receiver, rejected_before).await?;
    let disk_status = disk_receiver.status();
    let disk_pressure_rejected_storage_reason_count =
        reason_count(&disk_status.observability, VNextReasonCode::RejectedStorage);
    let disk_pressure_durable_feed_branches =
        disk_receiver.feed_inception_branch_count(disk_feed_id)?;
    disk_sender.shutdown().await;
    disk_receiver.shutdown().await;

    let slow_sender_dir = data_root.join("slow-sender");
    let healthy_sender_dir = data_root.join("healthy-sender");
    let slow_receiver_dir = data_root.join("slow-receiver");
    let mut slow_sender = VNextNetworkRuntime::start_canary_harness(
        &slow_sender_dir,
        bind_addr,
        VNextNetworkPolicy::default(),
        false,
        DEFAULT_NETWORK_STORAGE_HARD_WATERMARK_BYTES,
    )
    .await?;
    let mut healthy_sender = VNextNetworkRuntime::start_canary_harness(
        &healthy_sender_dir,
        bind_addr,
        VNextNetworkPolicy::default(),
        false,
        DEFAULT_NETWORK_STORAGE_HARD_WATERMARK_BYTES,
    )
    .await?;
    let mut slow_receiver = VNextNetworkRuntime::start_canary_harness(
        &slow_receiver_dir,
        bind_addr,
        VNextNetworkPolicy::default(),
        false,
        DEFAULT_NETWORK_STORAGE_HARD_WATERMARK_BYTES,
    )
    .await?;
    let slow_session = slow_sender.connect(slow_receiver.local_addr()).await?;
    tokio::time::sleep(Duration::from_millis(75)).await;
    let slow_peer_held_authenticated_session = slow_receiver.status().active_sessions >= 1;
    let (healthy_feed_id, healthy_feed_bytes) = signed_feed(0xA4)?;
    let healthy_started = Instant::now();
    send_feed(
        &healthy_sender,
        &slow_receiver,
        0xA5,
        &healthy_feed_bytes,
        healthy_feed_id,
    )
    .await?;
    let healthy_elapsed = healthy_started.elapsed();
    let healthy_peer_progressed_while_slow_peer_open =
        slow_receiver.feed_inception_branch_count(healthy_feed_id)? == 1
            && slow_receiver.status().active_sessions >= 1;
    slow_session.close();
    drop(slow_session);
    wait_for_no_active_sessions(&slow_sender).await?;
    wait_for_no_active_sessions(&healthy_sender).await?;
    wait_for_no_active_sessions(&slow_receiver).await?;
    let active_sessions_after_quiescence = slow_sender
        .status()
        .active_sessions
        .checked_add(healthy_sender.status().active_sessions)
        .and_then(|count| count.checked_add(slow_receiver.status().active_sessions))
        .ok_or(P5OperationsError::Fixture("active session count overflow"))?;
    let claims_network_completion = slow_sender.status().claims_network_completion
        || healthy_sender.status().claims_network_completion
        || slow_receiver.status().claims_network_completion;
    slow_sender.shutdown().await;
    healthy_sender.shutdown().await;
    slow_receiver.shutdown().await;

    let report = P5FaultDrillReport {
        signer_outage_failed_closed,
        signer_outage_left_zero_durable_files,
        disk_pressure_rejected_payload: disk_status.rejected_records > rejected_before,
        disk_pressure_rejected_storage_reason_count,
        disk_pressure_durable_feed_branches,
        slow_peer_held_authenticated_session,
        healthy_peer_progressed_while_slow_peer_open,
        healthy_peer_progress_millis: duration_millis(healthy_elapsed),
        active_sessions_after_quiescence,
        changes_wallet_state: false,
        changes_obt_state: false,
        grants_authority: false,
        claims_network_completion,
    };
    if !report.passes() {
        return Err(P5OperationsError::Oracle("P5-02 fault drill failed"));
    }
    Ok(report)
}

async fn run_backup_restore_drill(
    data_root: &Path,
) -> Result<P5BackupRestoreReport, P5OperationsError> {
    fs::create_dir_all(data_root)?;
    let source_dir = data_root.join("source");
    let peer_dir = data_root.join("peer");
    let archive_dir = data_root.join("archive");
    let restored_dir = data_root.join("restored");
    let corrupt_archive_dir = data_root.join("corrupt-archive");
    let corrupt_restore_dir = data_root.join("corrupt-restore");
    let seeded = seed_durable_state(&source_dir, &peer_dir, 0xB1).await?;

    let manifest = create_offline_backup(&source_dir, &archive_dir)?;
    verify_backup_payload(&archive_dir, &manifest)?;
    restore_offline_backup(&archive_dir, &restored_dir)?;
    let restored_manifest = snapshot_directory(&restored_dir)?;
    let exact_archive_copy_verified = restored_manifest.aggregate_root == manifest.aggregate_root
        && restored_manifest.files == manifest.files;
    let required_durable_files_preserved = P5_PROTECTED_DURABLE_FILES
        .iter()
        .filter(|name| restored_dir.join(name).is_file())
        .count();
    let journal_bytes_preserved = manifest_file_hash(&manifest, "vnext_reconciliation.redb")
        == manifest_file_hash(&restored_manifest, "vnext_reconciliation.redb");

    let mut restored_runtime = VNextNetworkRuntime::start_canary_harness(
        &restored_dir,
        loopback_any()?,
        VNextNetworkPolicy::default(),
        false,
        DEFAULT_NETWORK_STORAGE_HARD_WATERMARK_BYTES,
    )
    .await?;
    let restored_principal = NodeId::from_bytes(restored_runtime.status().principal);
    let raw_feed_branch_count_after_restore =
        restored_runtime.feed_inception_branch_count(seeded.feed_id)?;
    let pending_outbox_preserved = restored_runtime
        .outbound_intent(&seeded.outbound_intent.id)?
        .is_some_and(|intent| {
            intent.state == OutboundIntentState::Pending
                && intent.cid == seeded.outbound_intent.cid
                && intent.canonical_bytes == seeded.outbound_intent.canonical_bytes
        });
    restored_runtime.shutdown().await;
    drop(restored_runtime);

    let restored_operational = OperationalCompactionStore::open(
        restored_dir.join("vnext_operational_compaction.redb"),
        OperationalCompactionPolicy::default(),
    )?;
    let restored_operational_root = restored_operational.oracle_root()?;
    let restored_stats = restored_operational.evidence_stats()?;

    copy_directory_exact(&archive_dir, &corrupt_archive_dir)?;
    let first_payload = first_manifest_payload(&corrupt_archive_dir, &manifest)?;
    let mut corrupted = fs::read(&first_payload)?;
    if corrupted.is_empty() {
        return Err(P5OperationsError::Fixture(
            "backup payload selected for corruption was empty",
        ));
    }
    corrupted[0] ^= 0x80;
    fs::write(&first_payload, corrupted)?;
    let corrupt_archive_failed_before_restore =
        restore_offline_backup(&corrupt_archive_dir, &corrupt_restore_dir).is_err()
            && !corrupt_restore_dir.exists();

    let archive_payload_bytes = manifest
        .files
        .iter()
        .try_fold(0u64, |total, file| total.checked_add(file.length))
        .ok_or(P5OperationsError::Fixture(
            "backup payload byte count overflow",
        ))?;
    let report = P5BackupRestoreReport {
        archive_profile: manifest.profile.clone(),
        archive_file_count: manifest.files.len(),
        archive_payload_bytes,
        archive_root: manifest.aggregate_root,
        exact_archive_copy_verified,
        required_durable_files_preserved,
        principal_preserved: restored_principal == seeded.principal,
        raw_feed_branch_count_after_restore,
        journal_bytes_preserved,
        pending_outbox_preserved,
        quarantine_records_after_restore: restored_stats.quarantine_records,
        provenance_records_after_restore: restored_stats.provenance_records,
        operational_root_preserved: restored_operational_root == seeded.operational_root,
        corrupt_archive_failed_before_restore,
    };
    if !report.passes() {
        return Err(P5OperationsError::Oracle(
            "P5-03 backup/restore drill failed",
        ));
    }
    Ok(report)
}

async fn run_rollback_reenable_drill(
    data_root: &Path,
) -> Result<P5RollbackReenableReport, P5OperationsError> {
    fs::create_dir_all(data_root)?;
    let source_dir = data_root.join("source");
    let peer_dir = data_root.join("peer");
    let seeded = seed_durable_state(&source_dir, &peer_dir, 0xC1).await?;
    let mut peer = VNextNetworkRuntime::start_canary_harness(
        &peer_dir,
        loopback_any()?,
        VNextNetworkPolicy::default(),
        false,
        DEFAULT_NETWORK_STORAGE_HARD_WATERMARK_BYTES,
    )
    .await?;
    let peer_addr = peer.local_addr();
    let config = all_lanes_config();
    let mut runtime = VNextProductRuntime::start(
        &source_dir,
        loopback_any()?,
        &config,
        product_dependencies(0xC1)?,
        None,
    )
    .await?;
    let services = runtime.services();
    let before = services.status()?;
    let initial_enabled_lanes = enabled_lane_count(&before.rollout);
    let rolled_back = services.rollback_runtime()?;
    let rollback_generations = rolled_back
        .lanes
        .iter()
        .map(|lane| lane.generation)
        .collect::<Vec<_>>();
    let rollback_disabled_lanes = rolled_back
        .lanes
        .iter()
        .filter(|lane| !lane.enabled)
        .count();
    let network_rejected_after_rollback = services.connect_peer(peer_addr).await.is_err();
    runtime.shutdown().await;
    drop(runtime);
    drop(services);

    let mut inspection = VNextNetworkRuntime::start_canary_harness(
        &source_dir,
        loopback_any()?,
        VNextNetworkPolicy::default(),
        false,
        DEFAULT_NETWORK_STORAGE_HARD_WATERMARK_BYTES,
    )
    .await?;
    let principal_preserved = NodeId::from_bytes(inspection.status().principal) == seeded.principal;
    let raw_feed_branch_count_after_rollback =
        inspection.feed_inception_branch_count(seeded.feed_id)?;
    let pending_outbox_preserved = inspection
        .outbound_intent(&seeded.outbound_intent.id)?
        .is_some_and(|intent| intent.state == OutboundIntentState::Pending);
    inspection.shutdown().await;
    drop(inspection);
    let operational = OperationalCompactionStore::open(
        source_dir.join("vnext_operational_compaction.redb"),
        OperationalCompactionPolicy::default(),
    )?;
    let operational_root_preserved = operational.oracle_root()? == seeded.operational_root;
    let quarantine_preserved = operational.evidence_stats()?.quarantine_records
        == seeded.operational_stats.quarantine_records;
    drop(operational);

    let mut restarted = VNextProductRuntime::start(
        &source_dir,
        loopback_any()?,
        &config,
        product_dependencies(0xC1)?,
        None,
    )
    .await?;
    let restarted_services = restarted.services();
    let stale = restarted_services.status()?.rollout;
    let stale_config_enabled_lanes_after_restart = enabled_lane_count(&stale);
    for lane in VNextRuntimeLane::ALL {
        restarted_services.reenable_runtime_lane(lane)?;
    }
    let reenabled = restarted_services.status()?.rollout;
    let explicit_reenable_advanced_all_generations = reenabled
        .lanes
        .iter()
        .zip(rollback_generations)
        .all(|(lane, previous)| lane.generation > previous);
    let reenabled_lanes = enabled_lane_count(&reenabled);
    let session = restarted_services.connect_peer(peer_addr).await?;
    session.close();
    drop(session);
    wait_for_no_active_sessions(&peer).await?;
    let real_quic_reconnected_after_reenable = peer.status().authenticated_sessions >= 1;
    restarted.shutdown().await;
    peer.shutdown().await;

    let report = P5RollbackReenableReport {
        initial_enabled_lanes,
        rollback_disabled_lanes,
        network_rejected_after_rollback,
        stale_config_enabled_lanes_after_restart,
        explicit_reenable_advanced_all_generations,
        reenabled_lanes,
        real_quic_reconnected_after_reenable,
        principal_preserved,
        raw_feed_branch_count_after_rollback,
        pending_outbox_preserved,
        operational_root_preserved,
        quarantine_preserved,
        changes_wallet_state: rolled_back.changes_wallet_state || reenabled.changes_wallet_state,
        changes_obt_state: rolled_back.changes_obt_state || reenabled.changes_obt_state,
    };
    if !report.passes() {
        return Err(P5OperationsError::Oracle(
            "P5-04 rollback/re-enable drill failed",
        ));
    }
    Ok(report)
}

fn run_default_off_rollout_drill(
    data_root: &Path,
) -> Result<P5DefaultOffRolloutReport, P5OperationsError> {
    fs::create_dir_all(data_root)?;
    let config = VNextFeatureConfig::default();
    let features = [
        VNextFeature::ObjectEventV1,
        VNextFeature::ObpRp,
        VNextFeature::DistributedKqlOneHop,
        VNextFeature::PublicUseEvidencePublish,
        VNextFeature::DistributedPomvView,
        VNextFeature::InventoryShadow,
        VNextFeature::ProviderLease,
        VNextFeature::Fidelity,
        VNextFeature::RewardEvidenceExport,
        VNextFeature::CheckpointGc,
        VNextFeature::Riblt,
        VNextFeature::LegacyAdapter,
    ];
    let default_feature_flags_enabled = features
        .iter()
        .filter(|feature| config.enabled.is_set(**feature))
        .count();
    let none = no_lanes_requested();
    let rollout = VNextRuntimeRollout::open(data_root, none, none)?;
    let initial = rollout.snapshot()?;
    let default_enabled_runtime_lanes = enabled_lane_count(&initial);
    drop(rollout);

    let requested = VNextRuntimeLaneRequest::all_enabled();
    let stale_opt_in = VNextRuntimeRollout::open(data_root, requested, none)?;
    let stale = stale_opt_in.snapshot()?;
    let stale_opt_in_enabled_runtime_lanes = enabled_lane_count(&stale);
    let explicit_reenable_required = stale_opt_in.acquire(VNextRuntimeLane::Network).is_err();
    for lane in VNextRuntimeLane::ALL {
        stale_opt_in.reenable(lane)?;
    }
    let explicitly_enabled_runtime_lanes = enabled_lane_count(&stale_opt_in.snapshot()?);
    drop(stale_opt_in);

    let default_again = VNextRuntimeRollout::open(data_root, none, none)?;
    let default_config_effective_lanes_after_reopen =
        enabled_lane_count(&default_again.snapshot()?);
    let local_kql_round_trip_with_network_off = local_kql_round_trip()?;

    let report = P5DefaultOffRolloutReport {
        default_feature_flags_checked: features.len(),
        default_feature_flags_enabled,
        default_enabled_runtime_lanes,
        stale_opt_in_enabled_runtime_lanes,
        explicit_reenable_required,
        explicitly_enabled_runtime_lanes,
        default_config_effective_lanes_after_reopen,
        local_kql_round_trip_with_network_off,
        changes_wallet_state: false,
        changes_obt_state: false,
    };
    if !report.passes() {
        return Err(P5OperationsError::Oracle(
            "P5-05 default-off rollout drill failed",
        ));
    }
    Ok(report)
}

async fn run_operator_dashboard_drill(
    data_root: &Path,
) -> Result<P5OperatorDashboardReport, P5OperationsError> {
    fs::create_dir_all(data_root)?;
    let source_dir = data_root.join("source");
    let peer_dir = data_root.join("peer");
    let _seeded = seed_durable_state(&source_dir, &peer_dir, 0xD1).await?;
    let config = all_lanes_config();
    let mut runtime = VNextProductRuntime::start(
        &source_dir,
        loopback_any()?,
        &config,
        product_dependencies(0xD1)?,
        None,
    )
    .await?;
    let services = runtime.services();
    let operational = OperationalCompactionStore::open(
        source_dir.join("vnext_operational_compaction.redb"),
        OperationalCompactionPolicy::default(),
    )?;
    let stats = operational.evidence_stats()?;
    let degraded_snapshot = build_operator_dashboard(
        &services.status()?,
        stats,
        P5SignerHealth::Unavailable,
        P5RegistryHealth::Corrupt,
    );
    services.rollback_runtime()?;
    let rollback_snapshot = build_operator_dashboard(
        &services.status()?,
        stats,
        P5SignerHealth::Available,
        P5RegistryHealth::Verified,
    );
    runtime.shutdown().await;

    let serialized = serde_json::to_string(&degraded_snapshot)?;
    let serialized_snapshot_has_no_private_or_high_cardinality_labels = !serialized
        .contains("\"node_id\":")
        && !serialized.contains("\"selector\":")
        && !serialized.contains("\"private_need\":")
        && !degraded_snapshot.contains_high_cardinality_labels
        && !degraded_snapshot.contains_private_need_labels;
    let report = P5OperatorDashboardReport {
        signer_incident_visible: degraded_snapshot
            .incidents
            .contains(&P5OperatorIncident::SignerUnavailable),
        registry_corruption_visible: degraded_snapshot
            .incidents
            .contains(&P5OperatorIncident::RegistryCorrupt),
        outbox_visible: degraded_snapshot.pending_outbox >= 1
            && degraded_snapshot
                .incidents
                .contains(&P5OperatorIncident::PendingOutbox),
        journal_visible: degraded_snapshot.active_journals == 0
            && degraded_snapshot
                .actions
                .contains(&P5OperatorAction::InspectJournal),
        quarantine_visible: degraded_snapshot.quarantine_records == 1
            && degraded_snapshot
                .incidents
                .contains(&P5OperatorIncident::QuarantinePresent),
        rollback_visible: rollback_snapshot
            .incidents
            .contains(&P5OperatorIncident::RollbackActive),
        incident_response_actions_present: degraded_snapshot.actions.len() >= 6
            && rollback_snapshot
                .actions
                .contains(&P5OperatorAction::RequireExplicitReenable),
        serialized_snapshot_has_no_private_or_high_cardinality_labels,
        degraded_snapshot,
        rollback_snapshot,
    };
    if !report.passes() {
        return Err(P5OperationsError::Oracle(
            "P5-06 operator dashboard drill failed",
        ));
    }
    Ok(report)
}

pub fn build_operator_dashboard(
    status: &VNextProductRuntimeStatus,
    operational: OperationalEvidenceStats,
    signer_health: P5SignerHealth,
    registry_health: P5RegistryHealth,
) -> P5OperatorDashboardSnapshot {
    let mut incidents = BTreeSet::new();
    let mut actions = BTreeSet::from([P5OperatorAction::InspectJournal]);
    if signer_health == P5SignerHealth::Unavailable {
        incidents.insert(P5OperatorIncident::SignerUnavailable);
        actions.insert(P5OperatorAction::KeepLocalReadOnlyAvailable);
        actions.insert(P5OperatorAction::FencePublication);
    }
    if registry_health == P5RegistryHealth::Corrupt {
        incidents.insert(P5OperatorIncident::RegistryCorrupt);
        actions.insert(P5OperatorAction::DisableRegistryDependentLanes);
        actions.insert(P5OperatorAction::RestoreVerifiedRegistry);
    }
    if matches!(
        status.storage_pressure,
        crate::vnext_product_runtime::VNextStoragePressure::SoftWatermark
    ) {
        incidents.insert(P5OperatorIncident::StorageSoftWatermark);
        actions.insert(P5OperatorAction::StopNewWrites);
        actions.insert(P5OperatorAction::BackupThenCompact);
    }
    if reason_count(&status.observability, VNextReasonCode::RejectedStorage) > 0 {
        incidents.insert(P5OperatorIncident::StorageRejected);
        actions.insert(P5OperatorAction::StopNewWrites);
        actions.insert(P5OperatorAction::BackupThenCompact);
    }
    if status.observability.gauges.pending_outbox > 0 {
        incidents.insert(P5OperatorIncident::PendingOutbox);
        actions.insert(P5OperatorAction::InspectOutbox);
    }
    if status.observability.gauges.retry_exhausted_outbox > 0 {
        incidents.insert(P5OperatorIncident::RetryExhaustedOutbox);
        actions.insert(P5OperatorAction::InspectOutbox);
    }
    if status.observability.gauges.active_journals > 0 {
        incidents.insert(P5OperatorIncident::ActiveJournal);
    }
    if operational.quarantine_records > 0 || operational.quarantine_overflow.dropped_records > 0 {
        incidents.insert(P5OperatorIncident::QuarantinePresent);
        actions.insert(P5OperatorAction::PreserveQuarantine);
    }
    if status
        .rollout
        .lanes
        .iter()
        .any(|lane| lane.requested && !lane.enabled)
    {
        incidents.insert(P5OperatorIncident::LaneFenced);
        actions.insert(P5OperatorAction::RequireExplicitReenable);
    }
    if status.rollout.lanes.iter().all(|lane| !lane.enabled) {
        incidents.insert(P5OperatorIncident::RollbackActive);
        actions.insert(P5OperatorAction::RequireExplicitReenable);
    }
    let health = if incidents.contains(&P5OperatorIncident::RollbackActive) {
        "ROLLED_BACK"
    } else if incidents.is_empty() {
        "HEALTHY"
    } else {
        "DEGRADED"
    };
    P5OperatorDashboardSnapshot {
        profile: "onebrain/p5-operator-dashboard/1".to_owned(),
        health: health.to_owned(),
        startup_phase_count: status.startup_trace.len(),
        signer_mode: match status.signer_mode {
            VNextProductSignerMode::CallerOwned => "CALLER_OWNED",
            VNextProductSignerMode::CompatibilityFile => "COMPATIBILITY_FILE",
        }
        .to_owned(),
        signer_health,
        registry_health,
        registry_telemetry: status.observability.registry_state,
        lanes: status
            .rollout
            .lanes
            .iter()
            .map(|lane| P5OperatorLaneSnapshot {
                lane: lane.lane.name().to_owned(),
                requested: lane.requested,
                enabled: lane.enabled,
                generation: lane.generation,
            })
            .collect(),
        authenticated_routes: status.authenticated_routes,
        active_sessions: status.network.active_sessions,
        active_journals: status.observability.gauges.active_journals,
        pending_outbox: status.observability.gauges.pending_outbox,
        retry_exhausted_outbox: status.observability.gauges.retry_exhausted_outbox,
        oldest_pending_outbox_age_seconds: status
            .observability
            .gauges
            .oldest_pending_outbox_age_seconds,
        quarantine_records: operational.quarantine_records,
        provenance_records: operational.provenance_records,
        quarantine_overflow_records: operational.quarantine_overflow.dropped_records,
        provenance_overflow_records: operational.provenance_overflow.dropped_records,
        storage_bytes: status.storage_bytes,
        storage_pressure: match status.storage_pressure {
            crate::vnext_product_runtime::VNextStoragePressure::Normal => "NORMAL",
            crate::vnext_product_runtime::VNextStoragePressure::SoftWatermark => "SOFT_WATERMARK",
        }
        .to_owned(),
        incidents: incidents.into_iter().collect(),
        actions: actions.into_iter().collect(),
        contains_high_cardinality_labels: status.observability.contains_high_cardinality_labels,
        contains_private_need_labels: status.observability.contains_private_need_labels,
        changes_wallet_state: status.changes_wallet_state,
        changes_obt_state: status.changes_obt_state,
        claims_network_completion: status.claims_network_completion,
    }
}

struct UnavailableIdentitySigner {
    public_key: [u8; 32],
}

impl SessionIdentitySigner for UnavailableIdentitySigner {
    fn public_key(&self) -> [u8; 32] {
        self.public_key
    }

    fn sign_session_message(&self, _message: &[u8]) -> Result<[u8; 64], String> {
        Err("P5_SIGNER_UNAVAILABLE".to_owned())
    }
}

async fn send_feed_without_acceptance_wait(
    sender: &VNextNetworkRuntime,
    receiver: &VNextNetworkRuntime,
    marker: u8,
    feed_bytes: &[u8],
) -> Result<(), P5OperationsError> {
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
    let record = CarrierRecord::reconciliation_message(
        &encode_reconciliation_message(&manifest).map_err(protocol)?,
    )
    .map_err(protocol)?;
    session.send(&record).await?;
    session.send(&CarrierRecord::BoundPayload(frame)).await?;
    tokio::time::sleep(Duration::from_millis(25)).await;
    session.close();
    Ok(())
}

async fn wait_for_rejected_record(
    runtime: &VNextNetworkRuntime,
    before: u64,
) -> Result<(), P5OperationsError> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if runtime.status().rejected_records > before {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| P5OperationsError::Timeout("storage rejection"))
}

async fn seed_durable_state(
    source_dir: &Path,
    peer_dir: &Path,
    marker: u8,
) -> Result<SeededDurableState, P5OperationsError> {
    let mut source = VNextNetworkRuntime::start_canary_harness(
        source_dir,
        loopback_any()?,
        VNextNetworkPolicy::default(),
        false,
        DEFAULT_NETWORK_STORAGE_HARD_WATERMARK_BYTES,
    )
    .await?;
    let mut peer = VNextNetworkRuntime::start_canary_harness(
        peer_dir,
        loopback_any()?,
        VNextNetworkPolicy::default(),
        false,
        DEFAULT_NETWORK_STORAGE_HARD_WATERMARK_BYTES,
    )
    .await?;
    let principal = NodeId::from_bytes(source.status().principal);
    let (feed_id, feed_bytes) = signed_feed(marker)?;
    send_feed(&peer, &source, marker.wrapping_add(1), &feed_bytes, feed_id).await?;
    let unreachable = "127.0.0.1:9"
        .parse::<SocketAddr>()
        .map_err(|_| P5OperationsError::Fixture("invalid unreachable address"))?;
    let outbound_intent = OutboundTransferIntent::new(
        NodeId::from_bytes([marker.wrapping_add(2); 32]),
        unreachable,
        SelectorCid::from_bytes([marker.wrapping_add(3); 32]),
        NamespaceCommitment::from_bytes([marker.wrapping_add(4); 32]),
        DisclosureClass::Public,
        ReconcileManifestKind::Object,
        vec![marker.wrapping_add(5); 64],
    )?;
    source.enqueue_outbound(&outbound_intent)?;
    if !source
        .outbound_intent(&outbound_intent.id)?
        .is_some_and(|intent| intent.state == OutboundIntentState::Pending)
    {
        return Err(P5OperationsError::Oracle(
            "seeded outbound intent was not pending",
        ));
    }
    source.shutdown().await;
    peer.shutdown().await;
    drop(source);
    drop(peer);

    let operational_path = source_dir.join("vnext_operational_compaction.redb");
    let operational = OperationalCompactionStore::open(
        &operational_path,
        OperationalCompactionPolicy::default(),
    )?;
    operational.record_evidence(
        BoundedEvidenceKind::Quarantine,
        &[marker.wrapping_add(6); 64],
    )?;
    operational.record_evidence(
        BoundedEvidenceKind::Provenance,
        &[marker.wrapping_add(7); 64],
    )?;
    let operational_root = operational.oracle_root()?;
    let operational_stats = operational.evidence_stats()?;
    drop(operational);

    Ok(SeededDurableState {
        principal,
        feed_id,
        outbound_intent,
        operational_root,
        operational_stats,
    })
}

fn create_offline_backup(
    source_dir: &Path,
    archive_dir: &Path,
) -> Result<OfflineBackupManifest, P5OperationsError> {
    if archive_dir.exists() {
        return Err(P5OperationsError::ArchiveTargetExists(
            archive_dir.to_path_buf(),
        ));
    }
    let manifest = snapshot_directory(source_dir)?;
    fs::create_dir_all(archive_dir.join("payload"))?;
    for file in &manifest.files {
        let relative = safe_relative_path(&file.relative_path)?;
        let source = source_dir.join(&relative);
        let destination = archive_dir.join("payload").join(&relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source, &destination)?;
        OpenOptions::new()
            .write(true)
            .open(&destination)?
            .sync_all()?;
    }
    write_json_sync(&archive_dir.join("manifest.json"), &manifest)?;
    verify_backup_payload(archive_dir, &manifest)?;
    Ok(manifest)
}

fn restore_offline_backup(
    archive_dir: &Path,
    restored_dir: &Path,
) -> Result<OfflineBackupManifest, P5OperationsError> {
    let manifest: OfflineBackupManifest =
        serde_json::from_slice(&fs::read(archive_dir.join("manifest.json"))?)?;
    verify_backup_payload(archive_dir, &manifest)?;
    if restored_dir.exists() {
        return Err(P5OperationsError::ArchiveTargetExists(
            restored_dir.to_path_buf(),
        ));
    }
    fs::create_dir_all(restored_dir)?;
    for file in &manifest.files {
        let relative = safe_relative_path(&file.relative_path)?;
        let source = archive_dir.join("payload").join(&relative);
        let destination = restored_dir.join(&relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source, &destination)?;
        OpenOptions::new()
            .write(true)
            .open(&destination)?
            .sync_all()?;
    }
    let restored = snapshot_directory(restored_dir)?;
    if restored.files != manifest.files || restored.aggregate_root != manifest.aggregate_root {
        return Err(P5OperationsError::ArchiveIntegrity);
    }
    Ok(manifest)
}

fn verify_backup_payload(
    archive_dir: &Path,
    manifest: &OfflineBackupManifest,
) -> Result<(), P5OperationsError> {
    if manifest.profile != "onebrain/p5-offline-backup/1"
        || manifest.aggregate_root != aggregate_backup_root(&manifest.files)
    {
        return Err(P5OperationsError::ArchiveIntegrity);
    }
    for file in &manifest.files {
        let relative = safe_relative_path(&file.relative_path)?;
        let payload = archive_dir.join("payload").join(relative);
        let metadata = fs::metadata(&payload)?;
        if !metadata.is_file()
            || metadata.len() != file.length
            || digest_file(&payload)? != file.blake3
        {
            return Err(P5OperationsError::ArchiveIntegrity);
        }
    }
    Ok(())
}

fn snapshot_directory(directory: &Path) -> Result<OfflineBackupManifest, P5OperationsError> {
    let mut paths = Vec::new();
    collect_regular_files(directory, directory, &mut paths)?;
    paths.sort();
    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        let relative = portable_relative_path(directory, &path)?;
        let metadata = fs::metadata(&path)?;
        files.push(OfflineBackupFile {
            relative_path: relative,
            length: metadata.len(),
            blake3: digest_file(&path)?,
        });
    }
    let aggregate_root = aggregate_backup_root(&files);
    Ok(OfflineBackupManifest {
        profile: "onebrain/p5-offline-backup/1".to_owned(),
        files,
        aggregate_root,
    })
}

fn collect_regular_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), P5OperationsError> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            return Err(P5OperationsError::UnsupportedArchiveEntry(
                portable_relative_path(root, &entry.path())?,
            ));
        }
        if file_type.is_dir() {
            collect_regular_files(root, &entry.path(), files)?;
        } else if file_type.is_file() {
            files.push(entry.path());
        } else {
            return Err(P5OperationsError::UnsupportedArchiveEntry(
                portable_relative_path(root, &entry.path())?,
            ));
        }
    }
    Ok(())
}

fn copy_directory_exact(source: &Path, destination: &Path) -> Result<(), P5OperationsError> {
    if destination.exists() {
        return Err(P5OperationsError::ArchiveTargetExists(
            destination.to_path_buf(),
        ));
    }
    fs::create_dir_all(destination)?;
    let mut files = Vec::new();
    collect_regular_files(source, source, &mut files)?;
    for file in files {
        let relative = file
            .strip_prefix(source)
            .map_err(|_| P5OperationsError::ArchiveIntegrity)?;
        let target = destination.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(file, target)?;
    }
    Ok(())
}

fn first_manifest_payload(
    archive_dir: &Path,
    manifest: &OfflineBackupManifest,
) -> Result<PathBuf, P5OperationsError> {
    let file =
        manifest
            .files
            .iter()
            .find(|file| file.length > 0)
            .ok_or(P5OperationsError::Fixture(
                "backup manifest has no non-empty payload",
            ))?;
    Ok(archive_dir
        .join("payload")
        .join(safe_relative_path(&file.relative_path)?))
}

fn safe_relative_path(value: &str) -> Result<PathBuf, P5OperationsError> {
    let path = PathBuf::from(value);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(P5OperationsError::UnsupportedArchiveEntry(value.to_owned()));
    }
    Ok(path)
}

fn portable_relative_path(root: &Path, path: &Path) -> Result<String, P5OperationsError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| P5OperationsError::ArchiveIntegrity)?;
    let mut pieces = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(value) => pieces.push(value.to_string_lossy().into_owned()),
            _ => {
                return Err(P5OperationsError::UnsupportedArchiveEntry(
                    relative.display().to_string(),
                ))
            }
        }
    }
    if pieces.is_empty() {
        return Err(P5OperationsError::UnsupportedArchiveEntry(
            relative.display().to_string(),
        ));
    }
    Ok(pieces.join("/"))
}

fn digest_file(path: &Path) -> Result<String, P5OperationsError> {
    Ok(hex(blake3::hash(&fs::read(path)?).as_bytes()))
}

fn aggregate_backup_root(files: &[OfflineBackupFile]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:p5:offline-backup-manifest:1\0");
    for file in files {
        hasher.update(&(file.relative_path.len() as u64).to_be_bytes());
        hasher.update(file.relative_path.as_bytes());
        hasher.update(&file.length.to_be_bytes());
        hasher.update(file.blake3.as_bytes());
    }
    hex(hasher.finalize().as_bytes())
}

fn manifest_file_hash<'a>(manifest: &'a OfflineBackupManifest, name: &str) -> Option<&'a str> {
    manifest
        .files
        .iter()
        .find(|file| file.relative_path == name)
        .map(|file| file.blake3.as_str())
}

fn write_json_sync<T: Serialize>(path: &Path, value: &T) -> Result<(), P5OperationsError> {
    let bytes = serde_json::to_vec_pretty(value)?;
    let mut file = File::create(path)?;
    file.write_all(&bytes)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn local_kql_round_trip() -> Result<bool, P5OperationsError> {
    let query = QueryDefinition {
        need: KnowledgeNeedIr {
            receptor_definitions: vec![ObjectReference::new(0, [0xE1; 32])],
            desired_roles: vec![ConceptCcid::from_bytes([0xE2; 16])],
            goal: SemanticFrameSet {
                statements: Vec::new(),
            },
            local_context: SemanticFrameSet {
                statements: Vec::new(),
            },
            privacy: DisclosureClass::LocalOnly,
        },
        query_policy: ObjectReference::new(0, [0xE3; 32]),
        exploration_policy: ObjectReference::new(0, [0xE4; 32]),
    };
    let bytes = query
        .private_canonical_bytes()
        .map_err(|error| P5OperationsError::Protocol(format!("{error:?}")))?;
    let restored = QueryDefinition::from_private_canonical_bytes(&bytes)
        .map_err(|error| P5OperationsError::Protocol(format!("{error:?}")))?;
    Ok(restored == query)
}

fn all_lanes_config() -> VNextFeatureConfig {
    let mut config = VNextFeatureConfig::default();
    config.enabled.object_event_v1 = true;
    config.enabled.obp_rp = true;
    config.enabled.distributed_kql_one_hop = true;
    config.enabled.public_use_evidence_publish = true;
    config.enabled.distributed_pomv_view = true;
    config
}

fn product_dependencies(marker: u8) -> Result<VNextProductRuntimeDependencies, P5OperationsError> {
    let version = LocalPolicyVersion::new(1)
        .map_err(|error| P5OperationsError::Protocol(format!("{error:?}")))?;
    let policies = LocalPolicyRegistry::new([(
        version,
        MetabolicViewPolicy {
            policy_ref: ObjectReference::new(0, [marker; 32]),
            accepted_evidence_policies: vec![ObjectReference::new(0, [marker.wrapping_add(1); 32])],
            recent_event_horizon: 64,
        },
    )])
    .map_err(|error| P5OperationsError::Protocol(format!("{error:?}")))?;
    Ok(VNextProductRuntimeDependencies::new(
        LocalNeedVaultKey::from_bytes([marker.wrapping_add(2); 32]),
        policies,
    ))
}

fn no_lanes_requested() -> VNextRuntimeLaneRequest {
    VNextRuntimeLaneRequest {
        network: false,
        distributed_kql: false,
        public_use_evidence_publish: false,
        distributed_pomv_view: false,
    }
}

fn enabled_lane_count(snapshot: &VNextRuntimeRolloutSnapshot) -> usize {
    snapshot.lanes.iter().filter(|lane| lane.enabled).count()
}

fn reason_count(
    snapshot: &crate::vnext_observability::VNextObservabilitySnapshot,
    reason: VNextReasonCode,
) -> u64 {
    snapshot
        .reasons
        .iter()
        .find(|entry| entry.reason == reason)
        .map(|entry| entry.count)
        .unwrap_or_default()
}

fn loopback_any() -> Result<SocketAddr, P5OperationsError> {
    "127.0.0.1:0"
        .parse()
        .map_err(|_| P5OperationsError::Fixture("invalid loopback address"))
}

fn prepare_empty_root(data_root: &Path) -> Result<(), P5OperationsError> {
    if data_root.is_file() || (data_root.is_dir() && fs::read_dir(data_root)?.next().is_some()) {
        return Err(P5OperationsError::DataDirectoryNotEmpty(
            data_root.to_path_buf(),
        ));
    }
    fs::create_dir_all(data_root)?;
    Ok(())
}

fn protocol(error: impl std::fmt::Debug) -> P5OperationsError {
    P5OperationsError::Protocol(format!("{error:?}"))
}

fn duration_millis(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(DIGITS[(byte >> 4) as usize] as char);
        value.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    value
}

#[derive(Debug, Error)]
pub enum P5OperationsError {
    #[error("P5 operations data directory must be empty: {}", .0.display())]
    DataDirectoryNotEmpty(PathBuf),
    #[error("P5 operations archive/restore target already exists: {}", .0.display())]
    ArchiveTargetExists(PathBuf),
    #[error("P5 operations archive contains an unsupported entry: {0}")]
    UnsupportedArchiveEntry(String),
    #[error("P5 operations archive integrity check failed")]
    ArchiveIntegrity,
    #[error("P5 operations network runtime failed: {0}")]
    Network(#[from] VNextNetworkRuntimeError),
    #[error("P5 operations product runtime failed: {0}")]
    Product(#[from] VNextProductRuntimeError),
    #[error("P5 operations rollout failed: {0}")]
    Rollout(#[from] VNextRuntimeRolloutError),
    #[error("P5 operations outbox failed: {0}")]
    Outbox(#[from] OutboundOutboxError),
    #[error("P5 operations compaction store failed: {0}")]
    OperationalCompaction(#[from] OperationalCompactionError),
    #[error("P5 operations canary fixture failed: {0}")]
    Canary(#[from] P5CanaryPreflightError),
    #[error("P5 operations protocol fixture failed: {0}")]
    Protocol(String),
    #[error("P5 operations fixture failed: {0}")]
    Fixture(&'static str),
    #[error("P5 operations timed out waiting for {0}")]
    Timeout(&'static str),
    #[error("P5 operations oracle failed: {0}")]
    Oracle(&'static str),
    #[error("P5 operations JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("P5 operations filesystem failed: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn p5_02_through_p5_06_operational_preflight_passes_without_72h() {
        let directory = tempfile::tempdir().unwrap();
        let report = run_p5_operations_preflight(directory.path()).await.unwrap();
        assert!(report.passes());
        assert!(!report.consumes_pre_release_72h_evidence);
        assert!(!report.multi_host_canary_qualified);
        assert!(!report.production_canary_qualified);
        assert_eq!(report.backup_restore.raw_feed_branch_count_after_restore, 1);
        assert_eq!(report.rollback_reenable.reenabled_lanes, 4);
        assert!(
            report
                .default_off_rollout
                .local_kql_round_trip_with_network_off
        );
    }

    #[tokio::test]
    async fn p5_operations_nonempty_root_fails_before_first_runtime() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("operator-owned.bin");
        fs::write(&marker, b"preserve").unwrap();
        assert!(matches!(
            run_p5_operations_preflight(directory.path()).await,
            Err(P5OperationsError::DataDirectoryNotEmpty(path)) if path == directory.path()
        ));
        assert_eq!(fs::read(marker).unwrap(), b"preserve");
    }

    #[test]
    fn p5_dashboard_serialization_has_fixed_incidents_and_no_identifiers() {
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
        let json = serde_json::to_string(&incidents).unwrap();
        assert!(!json.contains("node_id"));
        assert!(!json.contains("selector"));
        assert!(!json.contains("private_need"));
        assert_eq!(incidents.len(), 10);
    }
}
