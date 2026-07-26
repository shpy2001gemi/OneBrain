//! M5-05 operational compaction coordinator primitives.
//!
//! Raw quarantine/provenance evidence is hard-bounded and overflow is retained
//! as a deterministic hash chain. KQL/PoMV derived snapshots are canonical,
//! root-checked and replaceable under the shared compaction generation fence.

#![cfg(feature = "vnext-network-runtime")]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ku_core::foundation::{dr_m5_failpoint, OperationalCompactionPermit};
use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
use thiserror::Error;

const QUARANTINE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_compaction_quarantine_v1");
const PROVENANCE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_compaction_provenance_v1");
const OVERFLOW: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_compaction_overflow_v1");
const DERIVED: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_compaction_derived_snapshots_v1");
const DERIVED_MAGIC: &[u8; 8] = b"OBIDXV1\0";
const OVERFLOW_BYTES: usize = 80;

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
    const fn key(self) -> &'static [u8] {
        match self {
            Self::Quarantine => b"quarantine",
            Self::Provenance => b"provenance",
        }
    }

    const fn boundary(self) -> &'static str {
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
        let rows = vec![row(3, 96), row(1, 32), row(2, 64)];
        let kql = DerivedIndexSnapshot::new(DerivedIndexLane::Kql, 1, rows.clone()).unwrap();
        assert_eq!(
            kql.source_root,
            root("0ca08333f6db371de7674d19cb99db26df952b72a87ca6ee37226a9bf0872910")
        );
        assert_eq!(
            kql.projection_root,
            root("230a443d1bd69814e05fb2a2173c4d895556b262b39586204120fa48d8442194")
        );

        let pomv = DerivedIndexSnapshot::new(DerivedIndexLane::Pomv, 1, rows).unwrap();
        assert_eq!(
            pomv.source_root,
            root("7fbcf8ee16d00a0c31391f45dcf0c424387c53dafa2ea77d0a4a37a5f799f689")
        );
        assert_eq!(
            pomv.projection_root,
            root("73f25199ee54961a10dc3585ed28d8fc08e1be432ced37f1f0b3a92582ccc571")
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
