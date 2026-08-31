//! Durable M5-06 runtime generation fence and rollback state.
//!
//! The configured feature budget says which lanes this binary may run. This
//! store records the operator's last durable decision. Startup may apply a
//! configured kill, but it never turns a durably killed lane back on. Only an
//! explicit re-enable advances the generation and restores admission.

#![cfg(feature = "vnext-network-runtime")]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

use ku_core::foundation::dr_m5_failpoint;
use redb::{Database, ReadableTable, TableDefinition};
use thiserror::Error;

use crate::archive::{PortableArchiveRow, PortableArchiveRows};
use crate::error::NodeError;
use onebrain_archive::{ArchiveEntryKind, ArchiveOwner};

const ROLLOUT: TableDefinition<&str, &[u8]> = TableDefinition::new("vnext_runtime_rollout_v1");
const ROLLOUT_DATABASE: &str = "vnext_runtime_rollout.redb";
const TX_RUNTIME_ROLLBACK: &str = "TX-ROL-001";
const STORED_LANE_BYTES: usize = 9;

pub const VNEXT_RUNTIME_ROLLOUT_PROFILE_MAJOR: u64 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum VNextRuntimeLane {
    Network = 0,
    DistributedKql = 1,
    PublicUseEvidencePublish = 2,
    DistributedPomvView = 3,
}

impl VNextRuntimeLane {
    pub const ALL: [Self; 4] = [
        Self::Network,
        Self::DistributedKql,
        Self::PublicUseEvidencePublish,
        Self::DistributedPomvView,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::DistributedKql => "distributed_kql_one_hop",
            Self::PublicUseEvidencePublish => "public_use_evidence_publish",
            Self::DistributedPomvView => "distributed_pomv_view",
        }
    }

    const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VNextRuntimeLaneRequest {
    pub network: bool,
    pub distributed_kql: bool,
    pub public_use_evidence_publish: bool,
    pub distributed_pomv_view: bool,
}

impl VNextRuntimeLaneRequest {
    pub const fn all_enabled() -> Self {
        Self {
            network: true,
            distributed_kql: true,
            public_use_evidence_publish: true,
            distributed_pomv_view: true,
        }
    }

    pub const fn is_requested(self, lane: VNextRuntimeLane) -> bool {
        match lane {
            VNextRuntimeLane::Network => self.network,
            VNextRuntimeLane::DistributedKql => self.distributed_kql,
            VNextRuntimeLane::PublicUseEvidencePublish => self.public_use_evidence_publish,
            VNextRuntimeLane::DistributedPomvView => self.distributed_pomv_view,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VNextRuntimeLaneSnapshot {
    pub lane: VNextRuntimeLane,
    pub generation: u64,
    pub requested: bool,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VNextRuntimeRolloutSnapshot {
    pub lanes: [VNextRuntimeLaneSnapshot; 4],
    pub changes_wallet_state: bool,
    pub changes_obt_state: bool,
}

impl VNextRuntimeRolloutSnapshot {
    pub fn lane(&self, lane: VNextRuntimeLane) -> VNextRuntimeLaneSnapshot {
        self.lanes[lane.index()]
    }
}

/// Admission token acquired on one exact generation.
///
/// Operations admitted before a kill are allowed to drain on this generation.
/// Once the kill transaction commits, no new token for the lane can be
/// acquired. Re-enable creates a later generation; it never revives a token.
#[derive(Debug)]
pub struct VNextRuntimeGenerationLease {
    lane: VNextRuntimeLane,
    generation: u64,
    rollout: Weak<RolloutInner>,
}

impl VNextRuntimeGenerationLease {
    pub const fn lane(&self) -> VNextRuntimeLane {
        self.lane
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// A session uses this before every new record/side effect. Work already
    /// past this check may finish; later work on an old generation is stale.
    pub fn is_current(&self) -> bool {
        let Some(rollout) = self.rollout.upgrade() else {
            return false;
        };
        let Ok(state) = rollout.state.lock() else {
            return false;
        };
        let stored = state.lanes[self.lane.index()];
        rollout.requested.is_requested(self.lane)
            && stored.enabled
            && stored.generation == self.generation
    }
}

#[derive(Clone)]
pub struct VNextRuntimeRollout {
    inner: Arc<RolloutInner>,
}

struct RolloutInner {
    database: Database,
    state: Mutex<RolloutState>,
    requested: VNextRuntimeLaneRequest,
    path: PathBuf,
}

#[derive(Clone, Copy)]
struct RolloutState {
    lanes: [StoredLane; 4],
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct StoredLane {
    enabled: bool,
    generation: u64,
}

impl VNextRuntimeRollout {
    pub fn open(
        data_dir: &Path,
        requested: VNextRuntimeLaneRequest,
        configured_kills: VNextRuntimeLaneRequest,
    ) -> Result<Self, VNextRuntimeRolloutError> {
        std::fs::create_dir_all(data_dir)?;
        let path = data_dir.join(ROLLOUT_DATABASE);
        let database = Database::create(&path).map_err(backend)?;
        let write = database.begin_write().map_err(backend)?;
        let mut lanes = [StoredLane {
            enabled: false,
            generation: 0,
        }; 4];
        {
            let mut table = write.open_table(ROLLOUT).map_err(backend)?;
            for lane in VNextRuntimeLane::ALL {
                let existing = table
                    .get(lane.name())
                    .map_err(backend)?
                    .map(|value| decode_lane(value.value()))
                    .transpose()?;
                let mut stored = existing.unwrap_or(StoredLane {
                    enabled: requested.is_requested(lane),
                    generation: 1,
                });
                if configured_kills.is_requested(lane) && stored.enabled {
                    stored.enabled = false;
                    stored.generation = next_generation(stored.generation)?;
                }
                if existing != Some(stored) {
                    let encoded = encode_lane(stored);
                    table
                        .insert(lane.name(), encoded.as_slice())
                        .map_err(backend)?;
                }
                lanes[lane.index()] = stored;
            }
        }
        write.commit().map_err(backend)?;
        Ok(Self {
            inner: Arc::new(RolloutInner {
                database,
                state: Mutex::new(RolloutState { lanes }),
                requested,
                path,
            }),
        })
    }

    pub fn path(&self) -> &Path {
        &self.inner.path
    }

    pub fn snapshot(&self) -> Result<VNextRuntimeRolloutSnapshot, VNextRuntimeRolloutError> {
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| VNextRuntimeRolloutError::LockPoisoned)?;
        Ok(VNextRuntimeRolloutSnapshot {
            lanes: VNextRuntimeLane::ALL.map(|lane| {
                let stored = state.lanes[lane.index()];
                let requested = self.inner.requested.is_requested(lane);
                VNextRuntimeLaneSnapshot {
                    lane,
                    generation: stored.generation,
                    requested,
                    enabled: requested && stored.enabled,
                }
            }),
            changes_wallet_state: false,
            changes_obt_state: false,
        })
    }

    pub fn acquire(
        &self,
        lane: VNextRuntimeLane,
    ) -> Result<VNextRuntimeGenerationLease, VNextRuntimeRolloutError> {
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| VNextRuntimeRolloutError::LockPoisoned)?;
        let stored = state.lanes[lane.index()];
        if !self.inner.requested.is_requested(lane) || !stored.enabled {
            return Err(VNextRuntimeRolloutError::LaneFenced {
                lane,
                generation: stored.generation,
            });
        }
        Ok(VNextRuntimeGenerationLease {
            lane,
            generation: stored.generation,
            rollout: Arc::downgrade(&self.inner),
        })
    }

    /// Durably fence one lane. An already-fenced lane is idempotent.
    pub fn kill(
        &self,
        lane: VNextRuntimeLane,
    ) -> Result<VNextRuntimeLaneSnapshot, VNextRuntimeRolloutError> {
        self.mutate(&[lane], false)?;
        Ok(self.snapshot()?.lane(lane))
    }

    /// Explicit operator action required after a durable kill or rollback.
    pub fn reenable(
        &self,
        lane: VNextRuntimeLane,
    ) -> Result<VNextRuntimeLaneSnapshot, VNextRuntimeRolloutError> {
        if !self.inner.requested.is_requested(lane) {
            return Err(VNextRuntimeRolloutError::LaneNotRequested(lane));
        }
        self.mutate(&[lane], true)?;
        Ok(self.snapshot()?.lane(lane))
    }

    /// Atomically disable network and every product lane without deleting any
    /// runtime database. Repeating rollback is idempotent.
    pub fn rollback(&self) -> Result<VNextRuntimeRolloutSnapshot, VNextRuntimeRolloutError> {
        self.mutate(&VNextRuntimeLane::ALL, false)?;
        self.snapshot()
    }

    fn mutate(
        &self,
        lanes: &[VNextRuntimeLane],
        enabled: bool,
    ) -> Result<(), VNextRuntimeRolloutError> {
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| VNextRuntimeRolloutError::LockPoisoned)?;
        if lanes
            .iter()
            .all(|lane| state.lanes[lane.index()].enabled == enabled)
        {
            return Ok(());
        }
        dr_m5_failpoint::hit(TX_RUNTIME_ROLLBACK, "before_begin_write");
        let write = self.inner.database.begin_write().map_err(backend)?;
        dr_m5_failpoint::hit(TX_RUNTIME_ROLLBACK, "after_begin_write_before_mutation");
        let mut next = *state;
        {
            let mut table = write.open_table(ROLLOUT).map_err(backend)?;
            for lane in lanes {
                let stored = &mut next.lanes[lane.index()];
                if stored.enabled == enabled {
                    continue;
                }
                stored.enabled = enabled;
                stored.generation = next_generation(stored.generation)?;
                let encoded = encode_lane(*stored);
                table
                    .insert(lane.name(), encoded.as_slice())
                    .map_err(backend)?;
            }
        }
        dr_m5_failpoint::hit(TX_RUNTIME_ROLLBACK, "after_mutation_before_commit");
        write.commit().map_err(backend)?;
        dr_m5_failpoint::hit(TX_RUNTIME_ROLLBACK, "after_commit_before_next_side_effect");
        *state = next;
        dr_m5_failpoint::hit(TX_RUNTIME_ROLLBACK, "after_next_side_effect_before_ack");
        Ok(())
    }
}

impl PortableArchiveRows for VNextRuntimeRollout {
    fn archive_owner(&self) -> ArchiveOwner {
        ArchiveOwner::ROLLOUT
    }

    fn archive_entry_kind(&self) -> ArchiveEntryKind {
        ArchiveEntryKind::RolloutRecord
    }

    fn archive_rows(&self) -> Result<Vec<PortableArchiveRow>, NodeError> {
        let read = self
            .inner
            .database
            .begin_read()
            .map_err(rollout_archive_error)?;
        let table = read.open_table(ROLLOUT).map_err(rollout_archive_error)?;
        let mut rows = Vec::new();
        for row in table.iter().map_err(rollout_archive_error)? {
            let (key, value) = row.map_err(rollout_archive_error)?;
            let lane = VNextRuntimeLane::ALL
                .into_iter()
                .find(|lane| lane.name() == key.value())
                .ok_or_else(|| NodeError::ArchiveCapability("unknown rollout lane".into()))?;
            let stored = decode_lane(value.value())
                .map_err(|error| NodeError::Storage(error.to_string()))?;
            if stored.generation == 0 || encode_lane(stored).as_slice() != value.value() {
                return Err(NodeError::ArchiveCapability(
                    "rollout row is non-canonical".into(),
                ));
            }
            rows.push(PortableArchiveRow {
                table: 1,
                key: lane.name().as_bytes().to_vec(),
                value: value.value().to_vec(),
            });
        }
        rows.sort_by(|left, right| left.key.cmp(&right.key));
        Ok(rows)
    }

    fn restore_row(&self, row: &PortableArchiveRow) -> Result<(), NodeError> {
        if row.table != 1 {
            return Err(NodeError::ArchiveCapability(
                "rollout archive table is unknown".into(),
            ));
        }
        let key = std::str::from_utf8(&row.key)
            .map_err(|_| NodeError::ArchiveCapability("rollout lane is not UTF-8".into()))?;
        let lane = VNextRuntimeLane::ALL
            .into_iter()
            .find(|lane| lane.name() == key)
            .ok_or_else(|| NodeError::ArchiveCapability("unknown rollout lane".into()))?;
        let stored =
            decode_lane(&row.value).map_err(|error| NodeError::Storage(error.to_string()))?;
        if stored.generation == 0 || encode_lane(stored).as_slice() != row.value.as_slice() {
            return Err(NodeError::ArchiveCapability(
                "rollout row is non-canonical".into(),
            ));
        }
        let write = self
            .inner
            .database
            .begin_write()
            .map_err(rollout_archive_error)?;
        {
            let mut table = write.open_table(ROLLOUT).map_err(rollout_archive_error)?;
            let existing = table
                .get(lane.name())
                .map_err(rollout_archive_error)?
                .map(|value| value.value().to_vec());
            if let Some(existing) = existing {
                if existing.as_slice() != row.value.as_slice() {
                    return Err(NodeError::ArchiveCapability(
                        "rollout archive restore conflict".into(),
                    ));
                }
            } else {
                table
                    .insert(lane.name(), row.value.as_slice())
                    .map_err(rollout_archive_error)?;
            }
        }
        write.commit().map_err(rollout_archive_error)?;
        self.inner
            .state
            .lock()
            .map_err(|_| NodeError::ArchiveCapability("rollout state lock".into()))?
            .lanes[lane.index()] = stored;
        Ok(())
    }
}

fn rollout_archive_error(error: impl std::fmt::Display) -> NodeError {
    NodeError::Storage(error.to_string())
}

fn encode_lane(lane: StoredLane) -> [u8; STORED_LANE_BYTES] {
    let mut bytes = [0u8; STORED_LANE_BYTES];
    bytes[0] = u8::from(lane.enabled);
    bytes[1..].copy_from_slice(&lane.generation.to_be_bytes());
    bytes
}

fn decode_lane(bytes: &[u8]) -> Result<StoredLane, VNextRuntimeRolloutError> {
    if bytes.len() != STORED_LANE_BYTES || bytes[0] > 1 {
        return Err(VNextRuntimeRolloutError::CorruptState);
    }
    let generation = u64::from_be_bytes(
        bytes[1..]
            .try_into()
            .map_err(|_| VNextRuntimeRolloutError::CorruptState)?,
    );
    if generation == 0 {
        return Err(VNextRuntimeRolloutError::CorruptState);
    }
    Ok(StoredLane {
        enabled: bytes[0] == 1,
        generation,
    })
}

fn next_generation(current: u64) -> Result<u64, VNextRuntimeRolloutError> {
    current
        .checked_add(1)
        .ok_or(VNextRuntimeRolloutError::GenerationExhausted)
}

fn backend(error: impl std::fmt::Display) -> VNextRuntimeRolloutError {
    VNextRuntimeRolloutError::Backend(error.to_string())
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum VNextRuntimeRolloutError {
    #[error("vNext runtime rollout backend failed: {0}")]
    Backend(String),
    #[error("vNext runtime rollout state is corrupt")]
    CorruptState,
    #[error("vNext runtime rollout generation exhausted")]
    GenerationExhausted,
    #[error("vNext runtime rollout lock is poisoned")]
    LockPoisoned,
    #[error(
        "vNext runtime lane {lane_name} is fenced at generation {generation}",
        lane_name = .lane.name()
    )]
    LaneFenced {
        lane: VNextRuntimeLane,
        generation: u64,
    },
    #[error(
        "vNext runtime lane {lane_name} was not requested by configuration",
        lane_name = .0.name()
    )]
    LaneNotRequested(VNextRuntimeLane),
    #[error("vNext runtime rollout filesystem operation failed: {0}")]
    Io(String),
}

impl From<std::io::Error> for VNextRuntimeRolloutError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "vnext-crash-harness")]
    use std::process::{Command, Stdio};
    #[cfg(feature = "vnext-crash-harness")]
    use std::thread;
    #[cfg(feature = "vnext-crash-harness")]
    use std::time::{Duration, Instant};

    use super::*;

    fn no_kills() -> VNextRuntimeLaneRequest {
        VNextRuntimeLaneRequest {
            network: false,
            distributed_kql: false,
            public_use_evidence_publish: false,
            distributed_pomv_view: false,
        }
    }

    #[test]
    fn durable_kill_is_idempotent_and_stale_config_cannot_reenable() {
        let directory = tempfile::tempdir().unwrap();
        let rollout = VNextRuntimeRollout::open(
            directory.path(),
            VNextRuntimeLaneRequest::all_enabled(),
            no_kills(),
        )
        .unwrap();
        let initial = rollout
            .snapshot()
            .unwrap()
            .lane(VNextRuntimeLane::PublicUseEvidencePublish);
        let admitted = rollout
            .acquire(VNextRuntimeLane::PublicUseEvidencePublish)
            .unwrap();
        assert_eq!(admitted.generation(), initial.generation);
        let killed = rollout
            .kill(VNextRuntimeLane::PublicUseEvidencePublish)
            .unwrap();
        assert!(!admitted.is_current());
        assert!(!killed.enabled);
        assert_eq!(killed.generation, initial.generation + 1);
        assert_eq!(
            rollout
                .kill(VNextRuntimeLane::PublicUseEvidencePublish)
                .unwrap(),
            killed
        );
        drop(rollout);

        // The same old, enabled configuration is not an operator re-enable.
        let restarted = VNextRuntimeRollout::open(
            directory.path(),
            VNextRuntimeLaneRequest::all_enabled(),
            no_kills(),
        )
        .unwrap();
        assert_eq!(
            restarted
                .snapshot()
                .unwrap()
                .lane(VNextRuntimeLane::PublicUseEvidencePublish),
            killed
        );
        assert!(matches!(
            restarted.acquire(VNextRuntimeLane::PublicUseEvidencePublish),
            Err(VNextRuntimeRolloutError::LaneFenced { .. })
        ));

        let enabled = restarted
            .reenable(VNextRuntimeLane::PublicUseEvidencePublish)
            .unwrap();
        assert!(enabled.enabled);
        assert_eq!(enabled.generation, killed.generation + 1);
        let current = restarted
            .acquire(VNextRuntimeLane::PublicUseEvidencePublish)
            .unwrap();
        assert!(current.is_current());
        assert!(!admitted.is_current());
    }

    #[test]
    fn rollback_is_atomic_idempotent_and_does_not_touch_runtime_evidence() {
        let directory = tempfile::tempdir().unwrap();
        for file in [
            "vnext_verified.redb",
            "vnext_reconciliation.redb",
            "vnext_quarantine.redb",
            "wallet.redb",
            "obt.redb",
        ] {
            std::fs::write(directory.path().join(file), file.as_bytes()).unwrap();
        }
        let rollout = VNextRuntimeRollout::open(
            directory.path(),
            VNextRuntimeLaneRequest::all_enabled(),
            no_kills(),
        )
        .unwrap();
        let rolled_back = rollout.rollback().unwrap();
        assert!(rolled_back.lanes.iter().all(|lane| !lane.enabled));
        assert!(!rolled_back.changes_wallet_state);
        assert!(!rolled_back.changes_obt_state);
        assert_eq!(rollout.rollback().unwrap(), rolled_back);
        for file in [
            "vnext_verified.redb",
            "vnext_reconciliation.redb",
            "vnext_quarantine.redb",
            "wallet.redb",
            "obt.redb",
        ] {
            assert_eq!(
                std::fs::read(directory.path().join(file)).unwrap(),
                file.as_bytes()
            );
        }
    }

    #[test]
    fn configured_kill_only_moves_state_toward_disabled() {
        let directory = tempfile::tempdir().unwrap();
        let kills = VNextRuntimeLaneRequest {
            network: false,
            distributed_kql: true,
            public_use_evidence_publish: false,
            distributed_pomv_view: false,
        };
        let rollout = VNextRuntimeRollout::open(
            directory.path(),
            VNextRuntimeLaneRequest::all_enabled(),
            kills,
        )
        .unwrap();
        let snapshot = rollout.snapshot().unwrap();
        assert!(!snapshot.lane(VNextRuntimeLane::DistributedKql).enabled);
        assert!(
            snapshot
                .lane(VNextRuntimeLane::PublicUseEvidencePublish)
                .enabled
        );
    }

    #[cfg(feature = "vnext-crash-harness")]
    const ROLLBACK_CHILD_ENV: &str = "ONEBRAIN_M5_06_ROLLBACK_CHILD";
    #[cfg(feature = "vnext-crash-harness")]
    const ROLLBACK_DATABASE_ENV: &str = "ONEBRAIN_M5_06_ROLLBACK_DATABASE";
    #[cfg(feature = "vnext-crash-harness")]
    const ROLLBACK_CHILD_TEST: &str = "vnext_runtime_rollout::tests::m5_06_runtime_rollback_worker";

    #[cfg(feature = "vnext-crash-harness")]
    #[test]
    fn m5_06_runtime_rollback_worker() {
        if std::env::var_os(ROLLBACK_CHILD_ENV).is_none() {
            return;
        }
        let directory = PathBuf::from(std::env::var_os(ROLLBACK_DATABASE_ENV).unwrap());
        VNextRuntimeRollout::open(
            &directory,
            VNextRuntimeLaneRequest::all_enabled(),
            no_kills(),
        )
        .unwrap()
        .rollback()
        .unwrap();
    }

    #[cfg(feature = "vnext-crash-harness")]
    #[test]
    fn m5_06_runtime_rollback_process_kill_matrix_recovers_exact_generation() {
        for phase in dr_m5_failpoint::FAILPOINT_PHASES {
            let directory = tempfile::tempdir().unwrap();
            VNextRuntimeRollout::open(
                directory.path(),
                VNextRuntimeLaneRequest::all_enabled(),
                no_kills(),
            )
            .unwrap();
            let marker = directory.path().join("armed.json");
            let token = format!("runtime-rollback-{phase}-{}", std::process::id());
            let mut child = Command::new(std::env::current_exe().unwrap())
                .arg("--exact")
                .arg(ROLLBACK_CHILD_TEST)
                .arg("--nocapture")
                .env(ROLLBACK_CHILD_ENV, "1")
                .env(ROLLBACK_DATABASE_ENV, directory.path())
                .env(dr_m5_failpoint::ENABLE_ENV, "1")
                .env(
                    dr_m5_failpoint::FAILPOINT_ENV,
                    format!("{TX_RUNTIME_ROLLBACK}:{phase}"),
                )
                .env(dr_m5_failpoint::MARKER_ENV, &marker)
                .env(dr_m5_failpoint::TOKEN_ENV, &token)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            wait_for_rollback_marker(&mut child, &marker, &token, phase);
            child.kill().unwrap();
            assert!(!child.wait().unwrap().success());

            let recovered = VNextRuntimeRollout::open(
                directory.path(),
                VNextRuntimeLaneRequest::all_enabled(),
                no_kills(),
            )
            .unwrap();
            let snapshot = recovered.rollback().unwrap();
            assert!(snapshot
                .lanes
                .iter()
                .all(|lane| !lane.enabled && lane.generation == 2));
            drop(recovered);

            let stale_restart = VNextRuntimeRollout::open(
                directory.path(),
                VNextRuntimeLaneRequest::all_enabled(),
                no_kills(),
            )
            .unwrap()
            .snapshot()
            .unwrap();
            assert!(stale_restart
                .lanes
                .iter()
                .all(|lane| !lane.enabled && lane.generation == 2));
        }
    }

    #[cfg(feature = "vnext-crash-harness")]
    fn wait_for_rollback_marker(
        child: &mut std::process::Child,
        marker: &Path,
        token: &str,
        phase: &str,
    ) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if marker.is_file() {
                let body = std::fs::read_to_string(marker).unwrap();
                assert!(body.contains(&format!("\"boundary\":\"{TX_RUNTIME_ROLLBACK}\"")));
                assert!(body.contains(&format!("\"phase\":\"{phase}\"")));
                assert!(body.contains(&format!("\"token\":\"{token}\"")));
                return;
            }
            if let Some(status) = child.try_wait().unwrap() {
                panic!("runtime rollback {phase} exited before marker: {status}");
            }
            assert!(
                Instant::now() < deadline,
                "runtime rollback {phase} marker timeout"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}
