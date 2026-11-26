//! M5-05 operational compaction coordinator primitives.
//!
//! Raw quarantine/provenance evidence is hard-bounded and overflow is retained
//! as a deterministic hash chain. KQL/PoMV derived snapshots are canonical,
//! root-checked and replaceable under the shared compaction generation fence.

#![cfg(feature = "vnext-network-runtime")]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ku_core::foundation::{
    compute_tombstone_merkle_root, decode_canonical, dr_m5_failpoint, encode_canonical,
    prove_tombstone_inclusion, CanonicalValue, OperationalCompactionPermit, ResourceProfile,
    TombstoneEpochAccumulatorV1, TombstoneEpochError, TombstoneMerkleProof,
};
use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
use thiserror::Error;

use crate::archive::{PortableArchiveRow, PortableArchiveRows};
use crate::error::NodeError;
use onebrain_archive::{ArchiveEntryKind, ArchiveOwner};

const QUARANTINE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_compaction_quarantine_v1");
const PROVENANCE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_compaction_provenance_v1");
const OVERFLOW: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_compaction_overflow_v1");
const DERIVED: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_compaction_derived_snapshots_v1");
const HOT_TOMBSTONES: TableDefinition<&[u8], u64> = TableDefinition::new("vnext_hot_tombstones_v1");
const COLD_TOMBSTONE_EPOCHS: TableDefinition<u64, &[u8]> =
    TableDefinition::new("vnext_cold_tombstone_epochs_v1");
const TOMBSTONE_OVERFLOW_ROOT: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_tombstone_overflow_root_v1");
const OVERFLOW_SUMMARY_ROOT_KEY: &[u8] = b"overflow_summary_root_v1";
const DERIVED_MAGIC: &[u8; 8] = b"OBIDXV1\0";
const OVERFLOW_BYTES: usize = 80;

pub const HOT_TOMBSTONE_CAPACITY: usize = 65_536;
pub const HOT_TOMBSTONE_ROLLOVER_THRESHOLD: usize = 49_152; // 75% of 65,536

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TombstoneRecordOutcome {
    InsertedHot {
        hot_count: usize,
    },
    AlreadyPresent,
    RolledOverToColdEpoch {
        epoch_seq: u64,
        overflow_summary_root: [u8; 32],
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TombstoneBatchReport {
    pub inserted: usize,
    pub duplicates: usize,
    pub rolled_over_epochs: Vec<u64>,
    pub latest_overflow_summary_root: Option<[u8; 32]>,
}

pub const OPERATIONAL_COMPACTION_PROFILE_MAJOR: u64 = 1;
pub const MAX_OPERATIONAL_EVIDENCE_RECORDS: u64 = 4_096;
pub const MAX_OPERATIONAL_EVIDENCE_BYTES: usize = 1_048_576;
pub const MAX_DERIVED_SNAPSHOT_ROWS: usize = 65_536;
pub const MAX_DERIVED_SNAPSHOT_BYTES: usize = 16 * 1_048_576;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperationalCompactionPolicy {
    pub max_quarantine_records: u64,
    pub max_provenance_records: u64,
}

impl OperationalCompactionPolicy {
    pub fn validate(self) -> Result<Self, OperationalCompactionError> {
        if self.max_quarantine_records == 0
            || self.max_quarantine_records > MAX_OPERATIONAL_EVIDENCE_RECORDS
            || self.max_provenance_records == 0
            || self.max_provenance_records > MAX_OPERATIONAL_EVIDENCE_RECORDS
        {
            return Err(OperationalCompactionError::InvalidPolicy);
        }
        Ok(self)
    }
}

impl Default for OperationalCompactionPolicy {
    fn default() -> Self {
        Self {
            max_quarantine_records: 1_024,
            max_provenance_records: 2_048,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BoundedEvidenceKind {
    Quarantine = 1,
    Provenance = 2,
}

impl BoundedEvidenceKind {
    pub(crate) const fn key(self) -> &'static [u8] {
        match self {
            Self::Quarantine => b"quarantine",
            Self::Provenance => b"provenance",
        }
    }

    pub(crate) const fn boundary(self) -> &'static str {
        match self {
            Self::Quarantine => "TX-CMP-QAR-001",
            Self::Provenance => "TX-CMP-PRV-001",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OverflowEvidence {
    pub dropped_records: u64,
    pub dropped_bytes: u64,
    pub chain_root: [u8; 32],
    pub last_dropped_id: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvidenceRecordOutcome {
    Stored {
        id: [u8; 32],
    },
    Existing {
        id: [u8; 32],
    },
    OverflowRecorded {
        id: [u8; 32],
        evidence: OverflowEvidence,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperationalEvidenceStats {
    pub quarantine_records: u64,
    pub provenance_records: u64,
    pub quarantine_overflow: OverflowEvidence,
    pub provenance_overflow: OverflowEvidence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DerivedIndexLane {
    Kql = 1,
    Pomv = 2,
}

impl DerivedIndexLane {
    fn parse(value: u8) -> Result<Self, OperationalCompactionError> {
        match value {
            1 => Ok(Self::Kql),
            2 => Ok(Self::Pomv),
            _ => Err(OperationalCompactionError::CorruptSnapshot),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DerivedIndexRow {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivedIndexSnapshot {
    pub lane: DerivedIndexLane,
    pub reducer_version: u64,
    pub source_root: [u8; 32],
    pub projection_root: [u8; 32],
    pub rows: Vec<DerivedIndexRow>,
}

impl DerivedIndexSnapshot {
    pub fn new(
        lane: DerivedIndexLane,
        reducer_version: u64,
        mut rows: Vec<DerivedIndexRow>,
    ) -> Result<Self, OperationalCompactionError> {
        if reducer_version == 0 || rows.len() > MAX_DERIVED_SNAPSHOT_ROWS {
            return Err(OperationalCompactionError::SnapshotLimit);
        }
        rows.sort();
        rows.dedup();
        validate_rows(&rows)?;
        let source_root = derived_root(
            b"onebrain:vnext:derived-index-source:1\0",
            lane,
            reducer_version,
            &rows,
        );
        let projection_root = derived_root(
            b"onebrain:vnext:derived-index-projection:1\0",
            lane,
            reducer_version,
            &rows,
        );
        let snapshot = Self {
            lane,
            reducer_version,
            source_root,
            projection_root,
            rows,
        };
        if snapshot.encode()?.len() > MAX_DERIVED_SNAPSHOT_BYTES {
            return Err(OperationalCompactionError::SnapshotLimit);
        }
        Ok(snapshot)
    }

    pub fn encode(&self) -> Result<Vec<u8>, OperationalCompactionError> {
        validate_rows(&self.rows)?;
        if self.rows.len() > MAX_DERIVED_SNAPSHOT_ROWS
            || self.source_root
                != derived_root(
                    b"onebrain:vnext:derived-index-source:1\0",
                    self.lane,
                    self.reducer_version,
                    &self.rows,
                )
            || self.projection_root
                != derived_root(
                    b"onebrain:vnext:derived-index-projection:1\0",
                    self.lane,
                    self.reducer_version,
                    &self.rows,
                )
        {
            return Err(OperationalCompactionError::CorruptSnapshot);
        }
        let mut bytes = Vec::new();
        bytes.extend_from_slice(DERIVED_MAGIC);
        bytes.push(self.lane as u8);
        bytes.extend_from_slice(&self.reducer_version.to_be_bytes());
        bytes.extend_from_slice(&self.source_root);
        bytes.extend_from_slice(&self.projection_root);
        bytes.extend_from_slice(&(self.rows.len() as u32).to_be_bytes());
        for row in &self.rows {
            bytes.extend_from_slice(&(row.key.len() as u32).to_be_bytes());
            bytes.extend_from_slice(&(row.value.len() as u32).to_be_bytes());
            bytes.extend_from_slice(&row.key);
            bytes.extend_from_slice(&row.value);
        }
        if bytes.len() > MAX_DERIVED_SNAPSHOT_BYTES {
            return Err(OperationalCompactionError::SnapshotLimit);
        }
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, OperationalCompactionError> {
        if bytes.len() < 85
            || bytes.len() > MAX_DERIVED_SNAPSHOT_BYTES
            || &bytes[..8] != DERIVED_MAGIC
        {
            return Err(OperationalCompactionError::CorruptSnapshot);
        }
        let lane = DerivedIndexLane::parse(bytes[8])?;
        let reducer_version = read_u64(bytes, 9)?;
        let source_root = read_32(bytes, 17)?;
        let projection_root = read_32(bytes, 49)?;
        let count = read_u32(bytes, 81)? as usize;
        if reducer_version == 0 || count > MAX_DERIVED_SNAPSHOT_ROWS {
            return Err(OperationalCompactionError::SnapshotLimit);
        }
        let mut cursor = 85usize;
        let mut rows = Vec::with_capacity(count);
        for _ in 0..count {
            let key_len = read_u32(bytes, cursor)? as usize;
            cursor = cursor
                .checked_add(4)
                .ok_or(OperationalCompactionError::CorruptSnapshot)?;
            let value_len = read_u32(bytes, cursor)? as usize;
            cursor = cursor
                .checked_add(4)
                .ok_or(OperationalCompactionError::CorruptSnapshot)?;
            let key_end = cursor
                .checked_add(key_len)
                .ok_or(OperationalCompactionError::CorruptSnapshot)?;
            let value_end = key_end
                .checked_add(value_len)
                .ok_or(OperationalCompactionError::CorruptSnapshot)?;
            if value_end > bytes.len() {
                return Err(OperationalCompactionError::CorruptSnapshot);
            }
            rows.push(DerivedIndexRow {
                key: bytes[cursor..key_end].to_vec(),
                value: bytes[key_end..value_end].to_vec(),
            });
            cursor = value_end;
        }
        if cursor != bytes.len() {
            return Err(OperationalCompactionError::CorruptSnapshot);
        }
        let snapshot = Self {
            lane,
            reducer_version,
            source_root,
            projection_root,
            rows,
        };
        if snapshot.encode()? != bytes {
            return Err(OperationalCompactionError::CorruptSnapshot);
        }
        Ok(snapshot)
    }
}

#[derive(Clone)]
pub struct OperationalCompactionStore {
    db: Arc<Database>,
    path: Arc<PathBuf>,
    policy: OperationalCompactionPolicy,
}

impl OperationalCompactionStore {
    pub fn open(
        path: impl AsRef<Path>,
        policy: OperationalCompactionPolicy,
    ) -> Result<Self, OperationalCompactionError> {
        let policy = policy.validate()?;
        let path = path.as_ref().to_path_buf();
        let db = Database::create(&path).map_err(backend)?;
        let write = db.begin_write().map_err(backend)?;
        {
            write.open_table(QUARANTINE).map_err(backend)?;
            write.open_table(PROVENANCE).map_err(backend)?;
            write.open_table(OVERFLOW).map_err(backend)?;
            write.open_table(DERIVED).map_err(backend)?;
        }
        write.commit().map_err(backend)?;
        Ok(Self {
            db: Arc::new(db),
            path: Arc::new(path),
            policy,
        })
    }

    pub fn record_evidence(
        &self,
        kind: BoundedEvidenceKind,
        bytes: &[u8],
    ) -> Result<EvidenceRecordOutcome, OperationalCompactionError> {
        if bytes.is_empty() || bytes.len() > MAX_OPERATIONAL_EVIDENCE_BYTES {
            return Err(OperationalCompactionError::EvidenceLimit);
        }
        let id = evidence_id(kind, bytes);
        let boundary = kind.boundary();
        dr_m5_failpoint::hit(boundary, "before_begin_write");
        let write = self.db.begin_write().map_err(backend)?;
        dr_m5_failpoint::hit(boundary, "after_begin_write_before_mutation");
        let outcome;
        {
            let (definition, limit) = match kind {
                BoundedEvidenceKind::Quarantine => (QUARANTINE, self.policy.max_quarantine_records),
                BoundedEvidenceKind::Provenance => (PROVENANCE, self.policy.max_provenance_records),
            };
            let mut table = write.open_table(definition).map_err(backend)?;
            if table.get(id.as_slice()).map_err(backend)?.is_some() {
                outcome = EvidenceRecordOutcome::Existing { id };
            } else if table.len().map_err(backend)? < limit {
                table.insert(id.as_slice(), bytes).map_err(backend)?;
                outcome = EvidenceRecordOutcome::Stored { id };
            } else {
                drop(table);
                let mut overflow = write.open_table(OVERFLOW).map_err(backend)?;
                let previous = overflow
                    .get(kind.key())
                    .map_err(backend)?
                    .map(|value| decode_overflow(value.value()))
                    .transpose()?
                    .unwrap_or_default();
                let evidence = if previous.last_dropped_id == id {
                    previous
                } else {
                    let evidence = next_overflow(previous, kind, id, bytes.len() as u64);
                    let encoded = encode_overflow(evidence);
                    overflow
                        .insert(kind.key(), encoded.as_slice())
                        .map_err(backend)?;
                    evidence
                };
                outcome = EvidenceRecordOutcome::OverflowRecorded { id, evidence };
            }
        }
        dr_m5_failpoint::hit(boundary, "after_mutation_before_commit");
        write.commit().map_err(backend)?;
        dr_m5_failpoint::hit(boundary, "after_commit_before_next_side_effect");
        let _ = self.oracle_root()?;
        dr_m5_failpoint::hit(boundary, "after_next_side_effect_before_ack");
        Ok(outcome)
    }

    pub fn evidence_stats(&self) -> Result<OperationalEvidenceStats, OperationalCompactionError> {
        let read = self.db.begin_read().map_err(backend)?;
        let quarantine_records = read
            .open_table(QUARANTINE)
            .map_err(backend)?
            .len()
            .map_err(backend)?;
        let provenance_records = read
            .open_table(PROVENANCE)
            .map_err(backend)?
            .len()
            .map_err(backend)?;
        let overflow = read.open_table(OVERFLOW).map_err(backend)?;
        let quarantine_overflow = overflow
            .get(BoundedEvidenceKind::Quarantine.key())
            .map_err(backend)?
            .map(|value| decode_overflow(value.value()))
            .transpose()?
            .unwrap_or_default();
        let provenance_overflow = overflow
            .get(BoundedEvidenceKind::Provenance.key())
            .map_err(backend)?
            .map(|value| decode_overflow(value.value()))
            .transpose()?
            .unwrap_or_default();
        Ok(OperationalEvidenceStats {
            quarantine_records,
            provenance_records,
            quarantine_overflow,
            provenance_overflow,
        })
    }

    pub fn store_derived_snapshot(
        &self,
        permit: &OperationalCompactionPermit,
        snapshot: &DerivedIndexSnapshot,
    ) -> Result<(), OperationalCompactionError> {
        permit
            .ensure_current()
            .map_err(|_| OperationalCompactionError::CompactionFenced)?;
        let bytes = snapshot.encode()?;
        dr_m5_failpoint::hit("TX-CMP-IDX-001", "before_begin_write");
        let write = self.db.begin_write().map_err(backend)?;
        dr_m5_failpoint::hit("TX-CMP-IDX-001", "after_begin_write_before_mutation");
        let lane_key = [snapshot.lane as u8];
        {
            write
                .open_table(DERIVED)
                .map_err(backend)?
                .insert(lane_key.as_slice(), bytes.as_slice())
                .map_err(backend)?;
        }
        dr_m5_failpoint::hit("TX-CMP-IDX-001", "after_mutation_before_commit");
        permit
            .run_if_current(|| write.commit())
            .map_err(|_| OperationalCompactionError::CompactionFenced)?
            .map_err(backend)?;
        dr_m5_failpoint::hit("TX-CMP-IDX-001", "after_commit_before_next_side_effect");
        let restored = self
            .load_derived_snapshot(snapshot.lane)?
            .ok_or(OperationalCompactionError::CorruptSnapshot)?;
        if restored != *snapshot {
            return Err(OperationalCompactionError::SemanticDrift);
        }
        dr_m5_failpoint::hit("TX-CMP-IDX-001", "after_next_side_effect_before_ack");
        Ok(())
    }

    pub fn load_derived_snapshot(
        &self,
        lane: DerivedIndexLane,
    ) -> Result<Option<DerivedIndexSnapshot>, OperationalCompactionError> {
        let read = self.db.begin_read().map_err(backend)?;
        let table = read.open_table(DERIVED).map_err(backend)?;
        let lane_key = [lane as u8];
        table
            .get(lane_key.as_slice())
            .map_err(backend)?
            .map(|value| DerivedIndexSnapshot::decode(value.value()))
            .transpose()
    }

    pub fn oracle_root(&self) -> Result<[u8; 32], OperationalCompactionError> {
        let read = self.db.begin_read().map_err(backend)?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:operational-compaction-oracle:1\0");
        hash_table(
            &mut hasher,
            1,
            &read.open_table(QUARANTINE).map_err(backend)?,
        )?;
        hash_table(
            &mut hasher,
            2,
            &read.open_table(PROVENANCE).map_err(backend)?,
        )?;
        hash_table(&mut hasher, 3, &read.open_table(OVERFLOW).map_err(backend)?)?;
        hash_table(&mut hasher, 4, &read.open_table(DERIVED).map_err(backend)?)?;
        Ok(*hasher.finalize().as_bytes())
    }

    pub fn disk_bytes(&self) -> Result<u64, OperationalCompactionError> {
        std::fs::metadata(self.path.as_ref())
            .map(|metadata| metadata.len())
            .map_err(backend)
    }

    pub fn reclaim_disk(
        &mut self,
        permit: &OperationalCompactionPermit,
    ) -> Result<bool, OperationalCompactionError> {
        permit
            .ensure_current()
            .map_err(|_| OperationalCompactionError::CompactionFenced)?;
        let database =
            Arc::get_mut(&mut self.db).ok_or(OperationalCompactionError::DatabaseBusy)?;
        let mut reclaimed = false;
        for _ in 0..64 {
            if !permit
                .run_if_current(|| database.compact())
                .map_err(|_| OperationalCompactionError::CompactionFenced)?
                .map_err(backend)?
            {
                break;
            }
            reclaimed = true;
        }
        Ok(reclaimed)
    }

    // ─── Wave 6 `P8` Tombstone Epoch Accumulator Rollover (`49,152 / 65,536`) ─

    fn cold_tombstones_dir(&self) -> PathBuf {
        self.path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("cold-tombstones")
    }

    fn cold_epoch_file_path(&self, epoch_seq: u64) -> PathBuf {
        self.cold_tombstones_dir().join(format!("{epoch_seq}.cbor"))
    }

    pub fn hot_tombstone_count(&self) -> Result<u64, OperationalCompactionError> {
        let read = self.db.begin_read().map_err(backend)?;
        let Ok(table) = read.open_table(HOT_TOMBSTONES) else {
            return Ok(0);
        };
        table.len().map_err(backend)
    }

    pub fn tombstone_overflow_summary_root(
        &self,
    ) -> Result<Option<[u8; 32]>, OperationalCompactionError> {
        let read = self.db.begin_read().map_err(backend)?;
        let Ok(table) = read.open_table(TOMBSTONE_OVERFLOW_ROOT) else {
            return Ok(None);
        };
        let Some(guard) = table.get(OVERFLOW_SUMMARY_ROOT_KEY).map_err(backend)? else {
            return Ok(None);
        };
        let root: [u8; 32] = guard
            .value()
            .try_into()
            .map_err(|_| OperationalCompactionError::CorruptEvidence)?;
        Ok(Some(root))
    }

    pub fn cold_epoch_accumulators(
        &self,
    ) -> Result<Vec<TombstoneEpochAccumulatorV1>, OperationalCompactionError> {
        let read = self.db.begin_read().map_err(backend)?;
        let Ok(table) = read.open_table(COLD_TOMBSTONE_EPOCHS) else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for entry in table.iter().map_err(backend)? {
            let (_, val) = entry.map_err(backend)?;
            let acc = TombstoneEpochAccumulatorV1::decode_bytes(val.value())
                .map_err(OperationalCompactionError::TombstoneEpoch)?;
            out.push(acc);
        }
        Ok(out)
    }

    /// Lossless anti-replay check across both hot Redb tombstones and rolled-over
    /// cold `TombstoneEpochAccumulatorV1` epochs (256 KiB Bloom pre-filter + exact
    /// cold `.cbor` Merkle inclusion proof verification).
    pub fn contains_anti_replay_tombstone(
        &self,
        tombstone: &[u8; 32],
    ) -> Result<bool, OperationalCompactionError> {
        let read = self.db.begin_read().map_err(backend)?;
        if let Ok(hot) = read.open_table(HOT_TOMBSTONES) {
            if hot.get(tombstone.as_slice()).map_err(backend)?.is_some() {
                return Ok(true);
            }
        }
        if let Ok(cold) = read.open_table(COLD_TOMBSTONE_EPOCHS) {
            for entry in cold.iter().map_err(backend)? {
                let (epoch_key, val) = entry.map_err(backend)?;
                let epoch_seq = epoch_key.value();
                let acc = TombstoneEpochAccumulatorV1::decode_bytes(val.value())
                    .map_err(OperationalCompactionError::TombstoneEpoch)?;
                if !acc.might_contain(tombstone) {
                    continue;
                }
                // Bloom hit: verify exact membership against `<owner_dir>/cold-tombstones/<epoch_seq>.cbor`
                let cold_list = self.read_cold_epoch_tombstones(epoch_seq, &acc)?;
                if let Ok(proof) = prove_tombstone_inclusion(&cold_list, tombstone) {
                    if acc.verify_tombstone_inclusion(&proof) {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

    pub fn prove_cold_tombstone(
        &self,
        epoch_seq: u64,
        tombstone: &[u8; 32],
    ) -> Result<(TombstoneEpochAccumulatorV1, TombstoneMerkleProof), OperationalCompactionError>
    {
        let read = self.db.begin_read().map_err(backend)?;
        let cold = read.open_table(COLD_TOMBSTONE_EPOCHS).map_err(backend)?;
        let bytes = cold
            .get(epoch_seq)
            .map_err(backend)?
            .map(|g| g.value().to_vec())
            .ok_or(OperationalCompactionError::TombstoneEpoch(
                TombstoneEpochError::TombstoneNotFound,
            ))?;
        let acc = TombstoneEpochAccumulatorV1::decode_bytes(&bytes)
            .map_err(OperationalCompactionError::TombstoneEpoch)?;
        let list = self.read_cold_epoch_tombstones(epoch_seq, &acc)?;
        let proof = prove_tombstone_inclusion(&list, tombstone)
            .map_err(OperationalCompactionError::TombstoneEpoch)?;
        if !acc.verify_tombstone_inclusion(&proof) {
            return Err(OperationalCompactionError::CorruptEvidence);
        }
        Ok((acc, proof))
    }

    /// Record a single anti-replay tombstone, automatically rolling over at
    /// `HOT_TOMBSTONE_ROLLOVER_THRESHOLD` (`49,152 / 65,536`).
    pub fn record_anti_replay_tombstone(
        &self,
        tombstone: [u8; 32],
        sequence: u64,
    ) -> Result<TombstoneRecordOutcome, OperationalCompactionError> {
        if tombstone == [0; 32] {
            return Err(OperationalCompactionError::EvidenceLimit);
        }
        if self.contains_anti_replay_tombstone(&tombstone)? {
            return Ok(TombstoneRecordOutcome::AlreadyPresent);
        }

        let write = self.db.begin_write().map_err(backend)?;
        let outcome = {
            let mut hot = write.open_table(HOT_TOMBSTONES).map_err(backend)?;
            hot.insert(tombstone.as_slice(), sequence)
                .map_err(backend)?;
            let hot_len = usize::try_from(hot.len().map_err(backend)?)
                .map_err(|_| OperationalCompactionError::EvidenceLimit)?;
            if hot_len >= HOT_TOMBSTONE_ROLLOVER_THRESHOLD {
                drop(hot);
                let (epoch_seq, summary_root) = self.rollover_hot_tombstones_in_tx(&write)?;
                TombstoneRecordOutcome::RolledOverToColdEpoch {
                    epoch_seq,
                    overflow_summary_root: summary_root,
                }
            } else {
                TombstoneRecordOutcome::InsertedHot { hot_count: hot_len }
            }
        };
        write.commit().map_err(backend)?;
        Ok(outcome)
    }

    /// High-throughput batched anti-replay tombstone insertion (ideal for bulk replay/compaction
    /// windows exceeding `65,536` tombstones), rolling over deterministically at every `49,152`
    /// hot entries.
    pub fn record_anti_replay_tombstones_batch(
        &self,
        items: &[([u8; 32], u64)],
    ) -> Result<TombstoneBatchReport, OperationalCompactionError> {
        let mut report = TombstoneBatchReport::default();
        let cold_accs = self.cold_epoch_accumulators()?;
        let mut cold_lists: Vec<(TombstoneEpochAccumulatorV1, Vec<[u8; 32]>)> =
            Vec::with_capacity(cold_accs.len());
        for acc in cold_accs {
            let list = self.read_cold_epoch_tombstones(acc.epoch_seq, &acc)?;
            cold_lists.push((acc, list));
        }

        let write = self.db.begin_write().map_err(backend)?;
        {
            for &(tombstone, seq) in items {
                if tombstone == [0; 32] {
                    return Err(OperationalCompactionError::EvidenceLimit);
                }
                let in_cold = cold_lists.iter().any(|(acc, list)| {
                    acc.might_contain(&tombstone) && list.binary_search(&tombstone).is_ok()
                });
                if in_cold {
                    report.duplicates += 1;
                    continue;
                }
                let mut need_rollover = false;
                {
                    let mut hot = write.open_table(HOT_TOMBSTONES).map_err(backend)?;
                    if hot.get(tombstone.as_slice()).map_err(backend)?.is_some() {
                        report.duplicates += 1;
                        continue;
                    }
                    hot.insert(tombstone.as_slice(), seq).map_err(backend)?;
                    report.inserted += 1;
                    let hot_len = usize::try_from(hot.len().map_err(backend)?)
                        .map_err(|_| OperationalCompactionError::EvidenceLimit)?;
                    if hot_len >= HOT_TOMBSTONE_ROLLOVER_THRESHOLD {
                        need_rollover = true;
                    }
                }
                if need_rollover {
                    let (epoch_seq, root, acc, sorted_list) =
                        self.rollover_hot_tombstones_with_list(&write)?;
                    report.rolled_over_epochs.push(epoch_seq);
                    report.latest_overflow_summary_root = Some(root);
                    cold_lists.push((acc, sorted_list));
                }
            }
        }
        write.commit().map_err(backend)?;
        if report.latest_overflow_summary_root.is_none() {
            report.latest_overflow_summary_root = self.tombstone_overflow_summary_root()?;
        }
        Ok(report)
    }

    fn rollover_hot_tombstones_in_tx(
        &self,
        write: &redb::WriteTransaction,
    ) -> Result<(u64, [u8; 32]), OperationalCompactionError> {
        let (epoch_seq, root, _, _) = self.rollover_hot_tombstones_with_list(write)?;
        Ok((epoch_seq, root))
    }

    fn rollover_hot_tombstones_with_list(
        &self,
        write: &redb::WriteTransaction,
    ) -> Result<
        (u64, [u8; 32], TombstoneEpochAccumulatorV1, Vec<[u8; 32]>),
        OperationalCompactionError,
    > {
        let mut hot = write.open_table(HOT_TOMBSTONES).map_err(backend)?;
        let mut sorted_tombstones = Vec::new();
        let mut cutoff_sequence_floor = 0u64;
        for entry in hot.iter().map_err(backend)? {
            let (k, v) = entry.map_err(backend)?;
            let tb: [u8; 32] = k
                .value()
                .try_into()
                .map_err(|_| OperationalCompactionError::CorruptEvidence)?;
            cutoff_sequence_floor = cutoff_sequence_floor.max(v.value());
            sorted_tombstones.push(tb);
        }
        sorted_tombstones.sort();
        sorted_tombstones.dedup();

        let mut cold_epochs = write.open_table(COLD_TOMBSTONE_EPOCHS).map_err(backend)?;
        let next_epoch_seq = cold_epochs
            .iter()
            .map_err(backend)?
            .next_back()
            .transpose()
            .map_err(backend)?
            .map(|(k, _)| k.value().saturating_add(1))
            .unwrap_or(0);

        let acc = TombstoneEpochAccumulatorV1::build(
            next_epoch_seq,
            cutoff_sequence_floor,
            &sorted_tombstones,
        )
        .map_err(OperationalCompactionError::TombstoneEpoch)?;
        let acc_bytes = acc
            .encode_bytes()
            .map_err(OperationalCompactionError::TombstoneEpoch)?;

        // Write `<owner_dir>/cold-tombstones/<epoch_seq>.cbor` atomically via temp file + rename.
        let cold_dir = self.cold_tombstones_dir();
        std::fs::create_dir_all(&cold_dir).map_err(backend)?;
        let cold_file = self.cold_epoch_file_path(next_epoch_seq);
        let cold_tmp = cold_dir.join(format!("{next_epoch_seq}.cbor.tmp"));
        let cold_payload = encode_cold_tombstone_list(
            next_epoch_seq,
            acc.sorted_tombstone_merkle_root,
            &sorted_tombstones,
        )?;
        std::fs::write(&cold_tmp, &cold_payload).map_err(backend)?;
        std::fs::rename(&cold_tmp, &cold_file).map_err(backend)?;

        cold_epochs
            .insert(next_epoch_seq, acc_bytes.as_slice())
            .map_err(backend)?;
        drop(cold_epochs);

        let mut root_tbl = write.open_table(TOMBSTONE_OVERFLOW_ROOT).map_err(backend)?;
        let prev_root = root_tbl
            .get(OVERFLOW_SUMMARY_ROOT_KEY)
            .map_err(backend)?
            .map(|g| {
                let mut r = [0u8; 32];
                r.copy_from_slice(g.value());
                r
            })
            .unwrap_or([0; 32]);
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:tombstone-overflow-summary-root:1\0");
        hasher.update(&prev_root);
        hasher.update(&next_epoch_seq.to_be_bytes());
        hasher.update(&acc.sorted_tombstone_merkle_root);
        let summary_root = *hasher.finalize().as_bytes();
        root_tbl
            .insert(OVERFLOW_SUMMARY_ROOT_KEY, summary_root.as_slice())
            .map_err(backend)?;
        drop(root_tbl);

        for tb in &sorted_tombstones {
            hot.remove(tb.as_slice()).map_err(backend)?;
        }

        Ok((next_epoch_seq, summary_root, acc, sorted_tombstones))
    }

    fn read_cold_epoch_tombstones(
        &self,
        epoch_seq: u64,
        expected_acc: &TombstoneEpochAccumulatorV1,
    ) -> Result<Vec<[u8; 32]>, OperationalCompactionError> {
        let bytes = std::fs::read(self.cold_epoch_file_path(epoch_seq)).map_err(backend)?;
        let (decoded_seq, merkle_root, list) = decode_cold_tombstone_list(&bytes)?;
        if decoded_seq != epoch_seq
            || merkle_root != expected_acc.sorted_tombstone_merkle_root
            || compute_tombstone_merkle_root(&list) != expected_acc.sorted_tombstone_merkle_root
        {
            return Err(OperationalCompactionError::CorruptEvidence);
        }
        Ok(list)
    }
}

fn encode_cold_tombstone_list(
    epoch_seq: u64,
    merkle_root: [u8; 32],
    sorted_tombstones: &[[u8; 32]],
) -> Result<Vec<u8>, OperationalCompactionError> {
    let entries = sorted_tombstones
        .iter()
        .map(|tb| CanonicalValue::Bytes(tb.to_vec()))
        .collect();
    let root = CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(1)),
        (1, CanonicalValue::Unsigned(epoch_seq)),
        (2, CanonicalValue::Bytes(merkle_root.to_vec())),
        (3, CanonicalValue::Array(entries)),
    ]);
    encode_canonical(&root, ResourceProfile::ManifestV1)
        .map_err(|_| OperationalCompactionError::CorruptEvidence)
}

fn decode_cold_tombstone_list(
    bytes: &[u8],
) -> Result<(u64, [u8; 32], Vec<[u8; 32]>), OperationalCompactionError> {
    let val = decode_canonical(bytes, ResourceProfile::ManifestV1)
        .map_err(|_| OperationalCompactionError::CorruptEvidence)?;
    let CanonicalValue::Map(fields) = val else {
        return Err(OperationalCompactionError::CorruptEvidence);
    };
    if fields.len() != 4 {
        return Err(OperationalCompactionError::CorruptEvidence);
    }
    let CanonicalValue::Unsigned(1) = fields[0].1 else {
        return Err(OperationalCompactionError::CorruptEvidence);
    };
    let CanonicalValue::Unsigned(epoch_seq) = fields[1].1 else {
        return Err(OperationalCompactionError::CorruptEvidence);
    };
    let CanonicalValue::Bytes(root_bytes) = &fields[2].1 else {
        return Err(OperationalCompactionError::CorruptEvidence);
    };
    let merkle_root: [u8; 32] = root_bytes
        .as_slice()
        .try_into()
        .map_err(|_| OperationalCompactionError::CorruptEvidence)?;
    let CanonicalValue::Array(items) = &fields[3].1 else {
        return Err(OperationalCompactionError::CorruptEvidence);
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let CanonicalValue::Bytes(b) = item else {
            return Err(OperationalCompactionError::CorruptEvidence);
        };
        let tb: [u8; 32] = b
            .as_slice()
            .try_into()
            .map_err(|_| OperationalCompactionError::CorruptEvidence)?;
        out.push(tb);
    }
    Ok((epoch_seq, merkle_root, out))
}

impl PortableArchiveRows for OperationalCompactionStore {
    fn archive_owner(&self) -> ArchiveOwner {
        ArchiveOwner::OPERATIONAL
    }

    fn archive_entry_kind(&self) -> ArchiveEntryKind {
        ArchiveEntryKind::OperationalRecord
    }

    fn archive_rows(&self) -> Result<Vec<PortableArchiveRow>, NodeError> {
        let read = self.db.begin_read().map_err(operational_archive_error)?;
        let mut rows = Vec::new();
        for (table_id, table) in [
            (
                1u8,
                read.open_table(QUARANTINE)
                    .map_err(operational_archive_error)?,
            ),
            (
                2u8,
                read.open_table(PROVENANCE)
                    .map_err(operational_archive_error)?,
            ),
            (
                3u8,
                read.open_table(OVERFLOW)
                    .map_err(operational_archive_error)?,
            ),
        ] {
            for row in table.iter().map_err(operational_archive_error)? {
                let (key, value) = row.map_err(operational_archive_error)?;
                let row = PortableArchiveRow {
                    table: table_id,
                    key: key.value().to_vec(),
                    value: value.value().to_vec(),
                };
                validate_operational_archive_row(&row)?;
                rows.push(row);
            }
        }
        if let Ok(cold) = read.open_table(COLD_TOMBSTONE_EPOCHS) {
            for row in cold.iter().map_err(operational_archive_error)? {
                let (key, value) = row.map_err(operational_archive_error)?;
                let row = PortableArchiveRow {
                    table: 4,
                    key: key.value().to_be_bytes().to_vec(),
                    value: value.value().to_vec(),
                };
                validate_operational_archive_row(&row)?;
                rows.push(row);
            }
        }
        if let Ok(root_tbl) = read.open_table(TOMBSTONE_OVERFLOW_ROOT) {
            for row in root_tbl.iter().map_err(operational_archive_error)? {
                let (key, value) = row.map_err(operational_archive_error)?;
                let row = PortableArchiveRow {
                    table: 5,
                    key: key.value().to_vec(),
                    value: value.value().to_vec(),
                };
                validate_operational_archive_row(&row)?;
                rows.push(row);
            }
        }
        rows.sort_by(|left, right| (left.table, &left.key).cmp(&(right.table, &right.key)));
        Ok(rows)
    }

    fn restore_row(&self, row: &PortableArchiveRow) -> Result<(), NodeError> {
        validate_operational_archive_row(row)?;
        let write = self.db.begin_write().map_err(operational_archive_error)?;
        match row.table {
            1 => restore_operational_value(&write, QUARANTINE, row)?,
            2 => restore_operational_value(&write, PROVENANCE, row)?,
            3 => restore_operational_value(&write, OVERFLOW, row)?,
            4 => {
                let epoch_seq = u64::from_be_bytes(
                    row.key
                        .as_slice()
                        .try_into()
                        .map_err(|_| NodeError::ArchiveCapability("cold epoch key".into()))?,
                );
                let mut table = write
                    .open_table(COLD_TOMBSTONE_EPOCHS)
                    .map_err(operational_archive_error)?;
                let existing = table
                    .get(epoch_seq)
                    .map_err(operational_archive_error)?
                    .map(|g| g.value().to_vec());
                if let Some(existing_bytes) = existing {
                    if existing_bytes.as_slice() != row.value.as_slice() {
                        return Err(NodeError::ArchiveCapability(
                            "cold tombstone epoch restore conflict".into(),
                        ));
                    }
                } else {
                    table
                        .insert(epoch_seq, row.value.as_slice())
                        .map_err(operational_archive_error)?;
                }
            }
            5 => restore_operational_value(&write, TOMBSTONE_OVERFLOW_ROOT, row)?,
            _ => {
                return Err(NodeError::ArchiveCapability(
                    "operational archive table is unknown".into(),
                ))
            }
        }
        write.commit().map_err(operational_archive_error)
    }
}

fn validate_operational_archive_row(row: &PortableArchiveRow) -> Result<(), NodeError> {
    match row.table {
        1 | 2 => {
            let id: [u8; 32] = row
                .key
                .as_slice()
                .try_into()
                .map_err(|_| NodeError::ArchiveCapability("operational evidence ID".into()))?;
            let kind = if row.table == 1 {
                BoundedEvidenceKind::Quarantine
            } else {
                BoundedEvidenceKind::Provenance
            };
            if row.value.is_empty()
                || row.value.len() > MAX_OPERATIONAL_EVIDENCE_BYTES
                || evidence_id(kind, &row.value) != id
            {
                return Err(NodeError::ArchiveCapability(
                    "operational evidence row is non-canonical".into(),
                ));
            }
        }
        3 => {
            match row.key.as_slice() {
                b"quarantine" | b"provenance" => {}
                _ => {
                    return Err(NodeError::ArchiveCapability(
                        "operational overflow key is unknown".into(),
                    ))
                }
            };
            let evidence = decode_overflow(&row.value)
                .map_err(|error| NodeError::Storage(error.to_string()))?;
            if encode_overflow(evidence).as_slice() != row.value.as_slice() {
                return Err(NodeError::ArchiveCapability(
                    "operational overflow row is non-canonical".into(),
                ));
            }
        }
        4 => {
            let epoch_seq = u64::from_be_bytes(
                row.key
                    .as_slice()
                    .try_into()
                    .map_err(|_| NodeError::ArchiveCapability("cold epoch key".into()))?,
            );
            let acc = TombstoneEpochAccumulatorV1::decode_bytes(&row.value)
                .map_err(|e| NodeError::ArchiveCapability(format!("{e:?}")))?;
            if acc.epoch_seq != epoch_seq {
                return Err(NodeError::ArchiveCapability(
                    "cold tombstone epoch sequence mismatch".into(),
                ));
            }
        }
        5 => {
            if row.key.as_slice() != OVERFLOW_SUMMARY_ROOT_KEY || row.value.len() != 32 {
                return Err(NodeError::ArchiveCapability(
                    "tombstone overflow summary root row invalid".into(),
                ));
            }
        }
        _ => {
            return Err(NodeError::ArchiveCapability(
                "operational archive table is unknown".into(),
            ))
        }
    }
    Ok(())
}

fn restore_operational_value(
    write: &redb::WriteTransaction,
    definition: TableDefinition<&[u8], &[u8]>,
    row: &PortableArchiveRow,
) -> Result<(), NodeError> {
    let mut table = write
        .open_table(definition)
        .map_err(operational_archive_error)?;
    if let Some(existing) = table
        .get(row.key.as_slice())
        .map_err(operational_archive_error)?
    {
        if existing.value() == row.value.as_slice() {
            return Ok(());
        }
        return Err(NodeError::ArchiveCapability(
            "operational archive restore conflict".into(),
        ));
    }
    table
        .insert(row.key.as_slice(), row.value.as_slice())
        .map_err(operational_archive_error)?;
    Ok(())
}

fn operational_archive_error(error: impl std::fmt::Display) -> NodeError {
    NodeError::Storage(error.to_string())
}

fn validate_rows(rows: &[DerivedIndexRow]) -> Result<(), OperationalCompactionError> {
    let mut previous = None;
    let mut total = 85usize;
    for row in rows {
        if row.key.is_empty()
            || row.value.is_empty()
            || row.key.len() > 4_096
            || row.value.len() > MAX_OPERATIONAL_EVIDENCE_BYTES
            || previous.is_some_and(|candidate: &DerivedIndexRow| candidate >= row)
        {
            return Err(OperationalCompactionError::CorruptSnapshot);
        }
        total = total
            .checked_add(8)
            .and_then(|value| value.checked_add(row.key.len()))
            .and_then(|value| value.checked_add(row.value.len()))
            .ok_or(OperationalCompactionError::SnapshotLimit)?;
        if total > MAX_DERIVED_SNAPSHOT_BYTES {
            return Err(OperationalCompactionError::SnapshotLimit);
        }
        previous = Some(row);
    }
    Ok(())
}

fn derived_root(
    domain: &[u8],
    lane: DerivedIndexLane,
    reducer_version: u64,
    rows: &[DerivedIndexRow],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(&[lane as u8]);
    hasher.update(&reducer_version.to_be_bytes());
    for row in rows {
        hasher.update(&(row.key.len() as u64).to_be_bytes());
        hasher.update(&row.key);
        hasher.update(&(row.value.len() as u64).to_be_bytes());
        hasher.update(&row.value);
    }
    *hasher.finalize().as_bytes()
}

fn evidence_id(kind: BoundedEvidenceKind, bytes: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:bounded-operational-evidence:1\0");
    hasher.update(&[kind as u8]);
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

fn next_overflow(
    previous: OverflowEvidence,
    kind: BoundedEvidenceKind,
    id: [u8; 32],
    bytes: u64,
) -> OverflowEvidence {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:operational-overflow:1\0");
    hasher.update(&[kind as u8]);
    hasher.update(&previous.chain_root);
    hasher.update(&id);
    hasher.update(&bytes.to_be_bytes());
    OverflowEvidence {
        dropped_records: previous.dropped_records.saturating_add(1),
        dropped_bytes: previous.dropped_bytes.saturating_add(bytes),
        chain_root: *hasher.finalize().as_bytes(),
        last_dropped_id: id,
    }
}

fn encode_overflow(evidence: OverflowEvidence) -> [u8; OVERFLOW_BYTES] {
    let mut bytes = [0u8; OVERFLOW_BYTES];
    bytes[..8].copy_from_slice(&evidence.dropped_records.to_be_bytes());
    bytes[8..16].copy_from_slice(&evidence.dropped_bytes.to_be_bytes());
    bytes[16..48].copy_from_slice(&evidence.chain_root);
    bytes[48..].copy_from_slice(&evidence.last_dropped_id);
    bytes
}

fn decode_overflow(bytes: &[u8]) -> Result<OverflowEvidence, OperationalCompactionError> {
    if bytes.len() != OVERFLOW_BYTES {
        return Err(OperationalCompactionError::CorruptEvidence);
    }
    Ok(OverflowEvidence {
        dropped_records: read_u64(bytes, 0)?,
        dropped_bytes: read_u64(bytes, 8)?,
        chain_root: read_32(bytes, 16)?,
        last_dropped_id: read_32(bytes, 48)?,
    })
}

fn hash_table(
    hasher: &mut blake3::Hasher,
    lane: u8,
    table: &redb::ReadOnlyTable<&[u8], &[u8]>,
) -> Result<(), OperationalCompactionError> {
    hasher.update(&[lane]);
    for entry in table.iter().map_err(backend)? {
        let (key, value) = entry.map_err(backend)?;
        hasher.update(&(key.value().len() as u64).to_be_bytes());
        hasher.update(key.value());
        hasher.update(&(value.value().len() as u64).to_be_bytes());
        hasher.update(value.value());
    }
    Ok(())
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, OperationalCompactionError> {
    let end = offset
        .checked_add(4)
        .ok_or(OperationalCompactionError::CorruptSnapshot)?;
    Ok(u32::from_be_bytes(
        bytes
            .get(offset..end)
            .ok_or(OperationalCompactionError::CorruptSnapshot)?
            .try_into()
            .map_err(|_| OperationalCompactionError::CorruptSnapshot)?,
    ))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, OperationalCompactionError> {
    let end = offset
        .checked_add(8)
        .ok_or(OperationalCompactionError::CorruptSnapshot)?;
    Ok(u64::from_be_bytes(
        bytes
            .get(offset..end)
            .ok_or(OperationalCompactionError::CorruptSnapshot)?
            .try_into()
            .map_err(|_| OperationalCompactionError::CorruptSnapshot)?,
    ))
}

fn read_32(bytes: &[u8], offset: usize) -> Result<[u8; 32], OperationalCompactionError> {
    let end = offset
        .checked_add(32)
        .ok_or(OperationalCompactionError::CorruptSnapshot)?;
    bytes
        .get(offset..end)
        .ok_or(OperationalCompactionError::CorruptSnapshot)?
        .try_into()
        .map_err(|_| OperationalCompactionError::CorruptSnapshot)
}

fn backend(error: impl std::fmt::Display) -> OperationalCompactionError {
    OperationalCompactionError::Backend(error.to_string())
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OperationalCompactionError {
    #[error("operational compaction backend failed: {0}")]
    Backend(String),
    #[error("operational compaction policy is invalid")]
    InvalidPolicy,
    #[error("operational evidence exceeds the hard limit")]
    EvidenceLimit,
    #[error("operational evidence is corrupt")]
    CorruptEvidence,
    #[error("derived snapshot exceeds the hard limit")]
    SnapshotLimit,
    #[error("derived snapshot is corrupt or non-canonical")]
    CorruptSnapshot,
    #[error("operational compaction generation is disabled or stale")]
    CompactionFenced,
    #[error("derived snapshot restore changed the semantic result")]
    SemanticDrift,
    #[error("operational database has live clones and cannot reclaim disk")]
    DatabaseBusy,
    #[error("tombstone epoch accumulator failed: {0:?}")]
    TombstoneEpoch(TombstoneEpochError),
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "vnext-compaction-harness")]
    use std::fs;
    #[cfg(feature = "vnext-compaction-harness")]
    use std::process::{Command, Stdio};
    #[cfg(feature = "vnext-compaction-harness")]
    use std::thread;
    #[cfg(feature = "vnext-compaction-harness")]
    use std::time::{Duration, Instant};

    use ku_core::foundation::OperationalCompactionSwitch;

    use super::*;

    const ZERO_ROOT: [u8; 32] = [0; 32];

    fn row(marker: u8, bytes: usize) -> DerivedIndexRow {
        DerivedIndexRow {
            key: vec![marker],
            value: vec![marker; bytes],
        }
    }

    fn root(hex: &str) -> [u8; 32] {
        assert_eq!(hex.len(), 64);
        let mut value = [0u8; 32];
        for (index, byte) in value.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap();
        }
        value
    }

    #[test]
    fn quarantine_and_provenance_are_bounded_with_overflow_evidence() {
        let directory = tempfile::tempdir().unwrap();
        let store = OperationalCompactionStore::open(
            directory.path().join("bounded.redb"),
            OperationalCompactionPolicy {
                max_quarantine_records: 2,
                max_provenance_records: 1,
            },
        )
        .unwrap();
        for marker in 1..=4 {
            store
                .record_evidence(BoundedEvidenceKind::Quarantine, &[marker; 16])
                .unwrap();
        }
        store
            .record_evidence(BoundedEvidenceKind::Quarantine, &[4; 16])
            .unwrap();
        for marker in 5..=7 {
            store
                .record_evidence(BoundedEvidenceKind::Provenance, &[marker; 24])
                .unwrap();
        }
        let stats = store.evidence_stats().unwrap();
        assert_eq!(stats.quarantine_records, 2);
        assert_eq!(stats.provenance_records, 1);
        assert_eq!(stats.quarantine_overflow.dropped_records, 2);
        assert_eq!(stats.quarantine_overflow.dropped_bytes, 32);
        assert_ne!(stats.quarantine_overflow.chain_root, ZERO_ROOT);
        assert_eq!(stats.provenance_overflow.dropped_records, 2);
        assert_eq!(stats.provenance_overflow.dropped_bytes, 48);
        assert_ne!(stats.provenance_overflow.chain_root, ZERO_ROOT);
    }

    #[test]
    fn kql_and_pomv_snapshots_restore_exact_roots_and_rows() {
        let directory = tempfile::tempdir().unwrap();
        let store = OperationalCompactionStore::open(
            directory.path().join("derived.redb"),
            OperationalCompactionPolicy::default(),
        )
        .unwrap();
        let switch = OperationalCompactionSwitch::new_disabled();
        switch.enable();
        let permit = switch.acquire().unwrap();
        for lane in [DerivedIndexLane::Kql, DerivedIndexLane::Pomv] {
            let snapshot =
                DerivedIndexSnapshot::new(lane, 1, vec![row(3, 64), row(1, 32), row(2, 48)])
                    .unwrap();
            store.store_derived_snapshot(&permit, &snapshot).unwrap();
            let restored = store.load_derived_snapshot(lane).unwrap().unwrap();
            assert_eq!(restored, snapshot);
            assert_eq!(restored.encode().unwrap(), snapshot.encode().unwrap());
        }
    }

    #[test]
    fn frozen_kql_and_pomv_snapshot_roots_match_profile() {
        // ARCH-13 slice 7: rows, reducer version and roots come from the
        // frozen M5-05 profile fixture.
        let profile: serde_json::Value = serde_json::from_slice(
            &std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../test-vectors/vnext/dr-m5-operational-compaction-v1.json"
            ))
            .unwrap(),
        )
        .unwrap();
        let derived = &profile["derived_snapshots"];
        let fixture = &derived["fixture"];
        let reducer = derived["reducer_version"].as_u64().unwrap();
        let mut rows = Vec::new();
        for entry in fixture["rows"].as_array().unwrap().iter().rev() {
            let key = u8::from_str_radix(entry["key_hex"].as_str().unwrap(), 16).unwrap();
            assert_eq!(entry["value_byte"].as_u64(), Some(u64::from(key)));
            rows.push(row(key, entry["value_length"].as_u64().unwrap() as usize));
        }
        let fixed_root = |name: &str| root(fixture[name].as_str().unwrap());

        let kql = DerivedIndexSnapshot::new(DerivedIndexLane::Kql, reducer, rows.clone()).unwrap();
        assert_eq!(kql.source_root, fixed_root("kql_source_root_blake3"));
        assert_eq!(
            kql.projection_root,
            fixed_root("kql_projection_root_blake3")
        );

        let pomv = DerivedIndexSnapshot::new(DerivedIndexLane::Pomv, reducer, rows).unwrap();
        assert_eq!(pomv.source_root, fixed_root("pomv_source_root_blake3"));
        assert_eq!(
            pomv.projection_root,
            fixed_root("pomv_projection_root_blake3")
        );
    }

    #[test]
    fn stale_generation_cannot_replace_a_derived_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let store = OperationalCompactionStore::open(
            directory.path().join("fenced.redb"),
            OperationalCompactionPolicy::default(),
        )
        .unwrap();
        let snapshot =
            DerivedIndexSnapshot::new(DerivedIndexLane::Kql, 1, vec![row(1, 32)]).unwrap();
        let switch = OperationalCompactionSwitch::new_disabled();
        switch.enable();
        let stale = switch.acquire().unwrap();
        switch.disable();
        assert_eq!(
            store.store_derived_snapshot(&stale, &snapshot),
            Err(OperationalCompactionError::CompactionFenced)
        );
        assert!(store
            .load_derived_snapshot(DerivedIndexLane::Kql)
            .unwrap()
            .is_none());
    }

    #[test]
    fn corrupt_or_noncanonical_snapshot_never_restores() {
        let snapshot =
            DerivedIndexSnapshot::new(DerivedIndexLane::Pomv, 7, vec![row(1, 32), row(2, 64)])
                .unwrap();
        let mut bytes = snapshot.encode().unwrap();
        bytes[49] ^= 1;
        assert_eq!(
            DerivedIndexSnapshot::decode(&bytes),
            Err(OperationalCompactionError::CorruptSnapshot)
        );

        let mut trailing = snapshot.encode().unwrap();
        trailing.push(0);
        assert_eq!(
            DerivedIndexSnapshot::decode(&trailing),
            Err(OperationalCompactionError::CorruptSnapshot)
        );
    }

    #[cfg(feature = "vnext-compaction-harness")]
    const COMPACTION_CHILD_ENV: &str = "ONEBRAIN_M5_05_OPERATIONAL_CHILD";
    #[cfg(feature = "vnext-compaction-harness")]
    const COMPACTION_DATABASE_ENV: &str = "ONEBRAIN_M5_05_OPERATIONAL_DATABASE";
    #[cfg(feature = "vnext-compaction-harness")]
    const COMPACTION_BOUNDARY_ENV: &str = "ONEBRAIN_M5_05_OPERATIONAL_BOUNDARY";
    #[cfg(feature = "vnext-compaction-harness")]
    const COMPACTION_CHILD_TEST: &str =
        "vnext_operational_compaction::tests::m5_05_operational_compaction_worker";
    #[cfg(feature = "vnext-compaction-harness")]
    const COMPACTION_BOUNDARIES: [&str; 3] = ["TX-CMP-QAR-001", "TX-CMP-PRV-001", "TX-CMP-IDX-001"];

    #[cfg(feature = "vnext-compaction-harness")]
    #[test]
    fn m5_05_operational_compaction_worker() {
        if std::env::var_os(COMPACTION_CHILD_ENV).is_none() {
            return;
        }
        let database = std::env::var_os(COMPACTION_DATABASE_ENV).unwrap();
        let boundary = std::env::var(COMPACTION_BOUNDARY_ENV).unwrap();
        apply_operational_boundary(Path::new(&database), &boundary);
    }

    #[cfg(feature = "vnext-compaction-harness")]
    #[test]
    fn m5_05_operational_process_kill_matrix_restores_exact_root() {
        for boundary in COMPACTION_BOUNDARIES {
            let expected_directory = tempfile::tempdir().unwrap();
            let expected_path = expected_directory.path().join("expected.redb");
            initialize_operational_store(&expected_path);
            let expected = apply_operational_boundary(&expected_path, boundary);

            for phase in dr_m5_failpoint::FAILPOINT_PHASES {
                let directory = tempfile::tempdir().unwrap();
                let database = directory.path().join("operational.redb");
                let marker = directory.path().join("armed.json");
                initialize_operational_store(&database);
                let token = format!("operational-{boundary}-{phase}-{}", std::process::id());
                let mut child = Command::new(std::env::current_exe().unwrap())
                    .arg("--exact")
                    .arg(COMPACTION_CHILD_TEST)
                    .arg("--nocapture")
                    .env(COMPACTION_CHILD_ENV, "1")
                    .env(COMPACTION_DATABASE_ENV, &database)
                    .env(COMPACTION_BOUNDARY_ENV, boundary)
                    .env(dr_m5_failpoint::ENABLE_ENV, "1")
                    .env(
                        dr_m5_failpoint::FAILPOINT_ENV,
                        format!("{boundary}:{phase}"),
                    )
                    .env(dr_m5_failpoint::MARKER_ENV, &marker)
                    .env(dr_m5_failpoint::TOKEN_ENV, &token)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap();
                wait_for_operational_marker(&mut child, &marker, &token, boundary, phase);
                child.kill().unwrap();
                assert!(!child.wait().unwrap().success());

                let recovered = apply_operational_boundary(&database, boundary);
                assert_eq!(
                    recovered, expected,
                    "operational boundary {boundary} phase {phase}"
                );
            }
        }
    }

    #[cfg(feature = "vnext-compaction-harness")]
    fn operational_policy() -> OperationalCompactionPolicy {
        OperationalCompactionPolicy {
            max_quarantine_records: 1,
            max_provenance_records: 1,
        }
    }

    #[cfg(feature = "vnext-compaction-harness")]
    fn initialize_operational_store(path: &Path) {
        let store = OperationalCompactionStore::open(path, operational_policy()).unwrap();
        store
            .record_evidence(BoundedEvidenceKind::Quarantine, b"quarantine-retained")
            .unwrap();
        store
            .record_evidence(BoundedEvidenceKind::Provenance, b"provenance-retained")
            .unwrap();
    }

    #[cfg(feature = "vnext-compaction-harness")]
    fn operational_snapshot() -> DerivedIndexSnapshot {
        DerivedIndexSnapshot::new(
            DerivedIndexLane::Kql,
            1,
            vec![row(3, 96), row(1, 32), row(2, 64)],
        )
        .unwrap()
    }

    #[cfg(feature = "vnext-compaction-harness")]
    fn apply_operational_boundary(
        path: &Path,
        boundary: &str,
    ) -> (
        [u8; 32],
        OperationalEvidenceStats,
        Option<DerivedIndexSnapshot>,
        Option<DerivedIndexSnapshot>,
    ) {
        let store = OperationalCompactionStore::open(path, operational_policy()).unwrap();
        match boundary {
            "TX-CMP-QAR-001" => {
                store
                    .record_evidence(BoundedEvidenceKind::Quarantine, b"quarantine-overflow")
                    .unwrap();
            }
            "TX-CMP-PRV-001" => {
                store
                    .record_evidence(BoundedEvidenceKind::Provenance, b"provenance-overflow")
                    .unwrap();
            }
            "TX-CMP-IDX-001" => {
                let switch = OperationalCompactionSwitch::new_disabled();
                switch.enable();
                let permit = switch.acquire().unwrap();
                store
                    .store_derived_snapshot(&permit, &operational_snapshot())
                    .unwrap();
            }
            _ => panic!("unexpected operational boundary: {boundary}"),
        }
        (
            store.oracle_root().unwrap(),
            store.evidence_stats().unwrap(),
            store.load_derived_snapshot(DerivedIndexLane::Kql).unwrap(),
            store.load_derived_snapshot(DerivedIndexLane::Pomv).unwrap(),
        )
    }

    #[cfg(feature = "vnext-compaction-harness")]
    fn wait_for_operational_marker(
        child: &mut std::process::Child,
        marker: &Path,
        token: &str,
        boundary: &str,
        phase: &str,
    ) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if marker.is_file() {
                let body = fs::read_to_string(marker).unwrap();
                assert!(body.contains(&format!("\"boundary\":\"{boundary}\"")));
                assert!(body.contains(&format!("\"phase\":\"{phase}\"")));
                assert!(body.contains(&format!("\"token\":\"{token}\"")));
                return;
            }
            if let Some(status) = child.try_wait().unwrap() {
                panic!("operational {boundary}:{phase} exited before marker: {status}");
            }
            assert!(
                Instant::now() < deadline,
                "operational {boundary}:{phase} marker timeout"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}
