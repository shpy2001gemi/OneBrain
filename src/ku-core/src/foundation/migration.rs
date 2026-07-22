//! Additive, rollback-safe migration storage for legacy OneBrain rows.
//!
//! Migration never rewrites a v1 row. Every attempt first preserves the exact
//! source bytes, then atomically records either a derived vNext row or a local
//! quarantine record together with a per-row journal entry. Batch manifests
//! make kill/restart replay deterministic without a coordinator.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

pub const MIGRATION_PROFILE_MAJOR: u16 = 1;
pub const MAX_LEGACY_PRIMARY_KEY_BYTES: usize = 4_096;
pub const MAX_LEGACY_ROW_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_MIGRATION_REASON_BYTES: usize = 1_024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum LegacyDataClass {
    IdentityCounter = 1,
    AggregateVectorClock = 2,
    OrSetSnapshot = 3,
    EncodingStatus = 4,
    KqlSavedSearch = 5,
    DhtProvider = 6,
    Watch = 7,
    UnsignedGraphEvent = 8,
    PomvAggregate = 9,
    CheckpointSnapshot = 10,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LegacyRowKey {
    class: LegacyDataClass,
    primary_key: Vec<u8>,
}

impl LegacyRowKey {
    pub fn new(class: LegacyDataClass, primary_key: Vec<u8>) -> Result<Self, MigrationError> {
        if primary_key.is_empty() || primary_key.len() > MAX_LEGACY_PRIMARY_KEY_BYTES {
            return Err(MigrationError::InvalidPrimaryKey);
        }
        Ok(Self { class, primary_key })
    }

    pub const fn class(&self) -> LegacyDataClass {
        self.class
    }

    pub fn primary_key(&self) -> &[u8] {
        &self.primary_key
    }

    fn storage_key(&self) -> Vec<u8> {
        let mut key = Vec::with_capacity(5 + self.primary_key.len());
        key.push(self.class as u8);
        key.extend_from_slice(&(self.primary_key.len() as u32).to_be_bytes());
        key.extend_from_slice(&self.primary_key);
        key
    }
}

/// An explicit legacy-sized identity fragment. It has deliberately no
/// conversion into `NodeId`, `ActorId`, `FeedId`, or any other 256-bit role.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyIdentityPrefix {
    pub legacy_u64: u64,
    pub source_row_digest: [u8; 32],
}

impl LegacyIdentityPrefix {
    pub const fn new(legacy_u64: u64, source_row_digest: [u8; 32]) -> Self {
        Self {
            legacy_u64,
            source_row_digest,
        }
    }

    pub const fn is_full_width_identity(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacySourceRow {
    key: LegacyRowKey,
    raw_bytes: Vec<u8>,
    source_digest: [u8; 32],
}

impl LegacySourceRow {
    pub fn new(key: LegacyRowKey, raw_bytes: Vec<u8>) -> Result<Self, MigrationError> {
        if raw_bytes.len() > MAX_LEGACY_ROW_BYTES {
            return Err(MigrationError::RowTooLarge);
        }
        let source_digest = digest(b"legacy-source-row/1", &[&key.storage_key(), &raw_bytes]);
        Ok(Self {
            key,
            raw_bytes,
            source_digest,
        })
    }

    pub const fn key(&self) -> &LegacyRowKey {
        &self.key
    }

    pub fn raw_bytes(&self) -> &[u8] {
        &self.raw_bytes
    }

    pub const fn source_digest(&self) -> [u8; 32] {
        self.source_digest
    }
}

/// Read-only v1 view used by rollback binaries and dual-read fallback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadOnlyLegacyRow(LegacySourceRow);

impl ReadOnlyLegacyRow {
    pub const fn key(&self) -> &LegacyRowKey {
        self.0.key()
    }

    pub fn raw_bytes(&self) -> &[u8] {
        self.0.raw_bytes()
    }

    pub const fn source_digest(&self) -> [u8; 32] {
        self.0.source_digest()
    }

    pub const fn is_mutable(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedLegacyRow {
    bytes: Vec<u8>,
}

impl NormalizedLegacyRow {
    pub fn new(bytes: Vec<u8>) -> Result<Self, MigrationError> {
        if bytes.is_empty() || bytes.len() > MAX_LEGACY_ROW_BYTES {
            return Err(MigrationError::InvalidNormalizedRow);
        }
        Ok(Self { bytes })
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationRejection {
    reason_code: String,
}

impl MigrationRejection {
    pub fn new(reason_code: impl Into<String>) -> Result<Self, MigrationError> {
        let reason_code = reason_code.into();
        if reason_code.is_empty() || reason_code.len() > MAX_MIGRATION_REASON_BYTES {
            return Err(MigrationError::InvalidReason);
        }
        Ok(Self { reason_code })
    }

    pub fn reason_code(&self) -> &str {
        &self.reason_code
    }
}

pub trait LegacyRowNormalizer {
    fn normalize(&self, row: &LegacySourceRow) -> Result<NormalizedLegacyRow, MigrationRejection>;
}

impl<F> LegacyRowNormalizer for F
where
    F: Fn(&LegacySourceRow) -> Result<NormalizedLegacyRow, MigrationRejection>,
{
    fn normalize(&self, row: &LegacySourceRow) -> Result<NormalizedLegacyRow, MigrationRejection> {
        self(row)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationDisposition {
    VNextDerived,
    Quarantined,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredVNextMigration {
    pub source_digest: [u8; 32],
    pub class: LegacyDataClass,
    pub normalized_bytes: Vec<u8>,
    pub normalized_digest: [u8; 32],
}

impl StoredVNextMigration {
    pub const fn is_verified_source_of_record(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationQuarantineRecord {
    pub row_key: LegacyRowKey,
    pub source_digest: [u8; 32],
    pub reason_code: String,
    pub original_bytes: Vec<u8>,
    pub quarantine_id: [u8; 32],
}

impl MigrationQuarantineRecord {
    pub const fn is_executable(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationJournalEntry {
    pub batch_id: [u8; 32],
    pub row_key: LegacyRowKey,
    pub source_digest: [u8; 32],
    pub disposition: MigrationDisposition,
    pub output_digest: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationBatchJournal {
    pub batch_id: [u8; 32],
    pub manifest_digest: [u8; 32],
    pub expected_rows: u64,
    pub complete: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendMigrationOutcome {
    Committed,
    ExactReplay,
}

#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationCommit {
    raw: LegacySourceRow,
    journal: MigrationJournalEntry,
    accepted: Option<StoredVNextMigration>,
    quarantine: Option<MigrationQuarantineRecord>,
}

pub trait AtomicMigrationBackend: Send + Sync {
    fn begin_batch(&self, batch: &MigrationBatchJournal) -> Result<(), String>;
    fn commit_row(&self, commit: &MigrationCommit) -> Result<BackendMigrationOutcome, String>;
    fn complete_batch(&self, batch_id: &[u8; 32], manifest_digest: &[u8; 32])
        -> Result<(), String>;
    fn get_batch(&self, batch_id: &[u8; 32]) -> Result<Option<MigrationBatchJournal>, String>;
    fn get_journal(
        &self,
        batch_id: &[u8; 32],
        key: &LegacyRowKey,
    ) -> Result<Option<MigrationJournalEntry>, String>;
    fn get_raw(&self, key: &LegacyRowKey) -> Result<Option<LegacySourceRow>, String>;
    fn get_vnext(&self, key: &LegacyRowKey) -> Result<Option<StoredVNextMigration>, String>;
    fn get_quarantine(
        &self,
        key: &LegacyRowKey,
    ) -> Result<Option<MigrationQuarantineRecord>, String>;
}

#[derive(Default)]
struct InMemoryMigrationState {
    batches: BTreeMap<[u8; 32], MigrationBatchJournal>,
    journals: BTreeMap<([u8; 32], LegacyRowKey), MigrationJournalEntry>,
    raw: BTreeMap<LegacyRowKey, LegacySourceRow>,
    vnext: BTreeMap<LegacyRowKey, StoredVNextMigration>,
    quarantine: BTreeMap<LegacyRowKey, MigrationQuarantineRecord>,
}

#[derive(Default)]
pub struct InMemoryMigrationBackend {
    state: Mutex<InMemoryMigrationState>,
}

impl AtomicMigrationBackend for InMemoryMigrationBackend {
    fn begin_batch(&self, batch: &MigrationBatchJournal) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "MIGRATION_LOCK".to_string())?;
        match state.batches.get(&batch.batch_id) {
            Some(existing)
                if existing.manifest_digest == batch.manifest_digest
                    && existing.expected_rows == batch.expected_rows =>
            {
                Ok(())
            }
            Some(_) => Err("MIGRATION_BATCH_CONFLICT".into()),
            None => {
                state.batches.insert(batch.batch_id, batch.clone());
                Ok(())
            }
        }
    }

    fn commit_row(&self, commit: &MigrationCommit) -> Result<BackendMigrationOutcome, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "MIGRATION_LOCK".to_string())?;
        let batch = state
            .batches
            .get(&commit.journal.batch_id)
            .ok_or_else(|| "MIGRATION_BATCH_NOT_STARTED".to_string())?;
        if batch.complete {
            if state
                .journals
                .get(&(commit.journal.batch_id, commit.raw.key.clone()))
                == Some(&commit.journal)
            {
                return Ok(BackendMigrationOutcome::ExactReplay);
            }
            return Err("MIGRATION_COMPLETED_BATCH_MUTATION".into());
        }
        let journal_key = (commit.journal.batch_id, commit.raw.key.clone());
        if let Some(existing) = state.journals.get(&journal_key) {
            return if existing == &commit.journal {
                Ok(BackendMigrationOutcome::ExactReplay)
            } else {
                Err("MIGRATION_ROW_JOURNAL_CONFLICT".into())
            };
        }
        if let Some(existing) = state.raw.get(&commit.raw.key) {
            if existing != &commit.raw {
                return Err("MIGRATION_RAW_LEGACY_COLLISION".into());
            }
        }
        if let Some(accepted) = &commit.accepted {
            if state
                .vnext
                .get(&commit.raw.key)
                .is_some_and(|existing| existing != accepted)
            {
                return Err("MIGRATION_VNEXT_COLLISION".into());
            }
        }
        if let Some(quarantine) = &commit.quarantine {
            if state
                .quarantine
                .get(&commit.raw.key)
                .is_some_and(|existing| existing != quarantine)
            {
                return Err("MIGRATION_QUARANTINE_COLLISION".into());
            }
        }
        state
            .raw
            .entry(commit.raw.key.clone())
            .or_insert_with(|| commit.raw.clone());
        if let Some(accepted) = &commit.accepted {
            state
                .vnext
                .entry(commit.raw.key.clone())
                .or_insert_with(|| accepted.clone());
        }
        if let Some(quarantine) = &commit.quarantine {
            state
                .quarantine
                .entry(commit.raw.key.clone())
                .or_insert_with(|| quarantine.clone());
        }
        state.journals.insert(journal_key, commit.journal.clone());
        Ok(BackendMigrationOutcome::Committed)
    }

    fn complete_batch(
        &self,
        batch_id: &[u8; 32],
        manifest_digest: &[u8; 32],
    ) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "MIGRATION_LOCK".to_string())?;
        let count = state
            .journals
            .keys()
            .filter(|(journal_batch, _)| journal_batch == batch_id)
            .count() as u64;
        let batch = state
            .batches
            .get_mut(batch_id)
            .ok_or_else(|| "MIGRATION_BATCH_NOT_STARTED".to_string())?;
        if &batch.manifest_digest != manifest_digest || count != batch.expected_rows {
            return Err("MIGRATION_BATCH_INCOMPLETE".into());
        }
        batch.complete = true;
        Ok(())
    }

    fn get_batch(&self, batch_id: &[u8; 32]) -> Result<Option<MigrationBatchJournal>, String> {
        Ok(self
            .state
            .lock()
            .map_err(|_| "MIGRATION_LOCK".to_string())?
            .batches
            .get(batch_id)
            .cloned())
    }

    fn get_journal(
        &self,
        batch_id: &[u8; 32],
        key: &LegacyRowKey,
    ) -> Result<Option<MigrationJournalEntry>, String> {
        Ok(self
            .state
            .lock()
            .map_err(|_| "MIGRATION_LOCK".to_string())?
            .journals
            .get(&(*batch_id, key.clone()))
            .cloned())
    }

    fn get_raw(&self, key: &LegacyRowKey) -> Result<Option<LegacySourceRow>, String> {
        Ok(self
            .state
            .lock()
            .map_err(|_| "MIGRATION_LOCK".to_string())?
            .raw
            .get(key)
            .cloned())
    }

    fn get_vnext(&self, key: &LegacyRowKey) -> Result<Option<StoredVNextMigration>, String> {
        Ok(self
            .state
            .lock()
            .map_err(|_| "MIGRATION_LOCK".to_string())?
            .vnext
            .get(key)
            .cloned())
    }

    fn get_quarantine(
        &self,
        key: &LegacyRowKey,
    ) -> Result<Option<MigrationQuarantineRecord>, String> {
        Ok(self
            .state
            .lock()
            .map_err(|_| "MIGRATION_LOCK".to_string())?
            .quarantine
            .get(key)
            .cloned())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MigrationBatchOutcome {
    Complete { committed: u64, exact_replays: u64 },
    Interrupted { committed: u64, exact_replays: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DualReadRecord {
    VerifiedVNext(StoredVNextMigration),
    RawLegacy(ReadOnlyLegacyRow),
}

pub struct MigrationStore<B> {
    backend: B,
}

impl<B: AtomicMigrationBackend> MigrationStore<B> {
    pub const fn new(backend: B) -> Self {
        Self { backend }
    }

    pub fn run_batch<N: LegacyRowNormalizer>(
        &self,
        batch_id: [u8; 32],
        rows: &[LegacySourceRow],
        max_rows_this_run: usize,
        normalizer: &N,
    ) -> Result<MigrationBatchOutcome, MigrationError> {
        let (manifest_digest, ordered) = prepare_manifest(rows)?;
        let batch = MigrationBatchJournal {
            batch_id,
            manifest_digest,
            expected_rows: ordered.len() as u64,
            complete: false,
        };
        self.backend
            .begin_batch(&batch)
            .map_err(MigrationError::Backend)?;
        let mut committed = 0;
        let mut exact_replays = 0;
        for row in ordered.into_iter().take(max_rows_this_run) {
            let commit = prepare_commit(batch_id, row, normalizer);
            match self
                .backend
                .commit_row(&commit)
                .map_err(MigrationError::Backend)?
            {
                BackendMigrationOutcome::Committed => committed += 1,
                BackendMigrationOutcome::ExactReplay => exact_replays += 1,
            }
        }
        let processed = committed + exact_replays;
        if processed < rows.len() as u64 {
            return Ok(MigrationBatchOutcome::Interrupted {
                committed,
                exact_replays,
            });
        }
        self.backend
            .complete_batch(&batch_id, &manifest_digest)
            .map_err(MigrationError::Backend)?;
        Ok(MigrationBatchOutcome::Complete {
            committed,
            exact_replays,
        })
    }

    pub fn read_prefer_verified<F>(
        &self,
        key: &LegacyRowKey,
        verify_vnext: F,
    ) -> Result<Option<DualReadRecord>, MigrationError>
    where
        F: FnOnce(&StoredVNextMigration) -> bool,
    {
        if let Some(record) = self
            .backend
            .get_vnext(key)
            .map_err(MigrationError::Backend)?
        {
            if verify_vnext(&record) {
                return Ok(Some(DualReadRecord::VerifiedVNext(record)));
            }
        }
        self.read_raw_for_rollback(key)
            .map(|row| row.map(DualReadRecord::RawLegacy))
    }

    pub fn copy_on_read<N, F>(
        &self,
        batch_id: [u8; 32],
        row: LegacySourceRow,
        normalizer: &N,
        verify_vnext: F,
    ) -> Result<Option<DualReadRecord>, MigrationError>
    where
        N: LegacyRowNormalizer,
        F: FnOnce(&StoredVNextMigration) -> bool,
    {
        if self
            .backend
            .get_vnext(row.key())
            .map_err(MigrationError::Backend)?
            .is_none()
            && self
                .backend
                .get_quarantine(row.key())
                .map_err(MigrationError::Backend)?
                .is_none()
        {
            self.run_batch(batch_id, std::slice::from_ref(&row), 1, normalizer)?;
        }
        self.read_prefer_verified(row.key(), verify_vnext)
    }

    pub fn read_raw_for_rollback(
        &self,
        key: &LegacyRowKey,
    ) -> Result<Option<ReadOnlyLegacyRow>, MigrationError> {
        self.backend
            .get_raw(key)
            .map(|row| row.map(ReadOnlyLegacyRow))
            .map_err(MigrationError::Backend)
    }

    pub fn quarantine(
        &self,
        key: &LegacyRowKey,
    ) -> Result<Option<MigrationQuarantineRecord>, MigrationError> {
        self.backend
            .get_quarantine(key)
            .map_err(MigrationError::Backend)
    }

    pub fn batch_journal(
        &self,
        batch_id: &[u8; 32],
    ) -> Result<Option<MigrationBatchJournal>, MigrationError> {
        self.backend
            .get_batch(batch_id)
            .map_err(MigrationError::Backend)
    }

    pub fn row_journal(
        &self,
        batch_id: &[u8; 32],
        key: &LegacyRowKey,
    ) -> Result<Option<MigrationJournalEntry>, MigrationError> {
        self.backend
            .get_journal(batch_id, key)
            .map_err(MigrationError::Backend)
    }
}

fn prepare_manifest(
    rows: &[LegacySourceRow],
) -> Result<([u8; 32], Vec<&LegacySourceRow>), MigrationError> {
    let mut ordered: Vec<_> = rows.iter().collect();
    ordered.sort_by(|left, right| left.key.cmp(&right.key));
    let mut unique = BTreeSet::new();
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:migration-batch-manifest/1\0");
    for row in &ordered {
        if !unique.insert(row.key.clone()) {
            return Err(MigrationError::DuplicateRowKey);
        }
        let key = row.key.storage_key();
        hasher.update(&(key.len() as u64).to_be_bytes());
        hasher.update(&key);
        hasher.update(&row.source_digest);
    }
    Ok((*hasher.finalize().as_bytes(), ordered))
}

fn prepare_commit<N: LegacyRowNormalizer>(
    batch_id: [u8; 32],
    row: &LegacySourceRow,
    normalizer: &N,
) -> MigrationCommit {
    match normalizer.normalize(row) {
        Ok(normalized) => {
            let normalized_digest = digest(
                b"legacy-normalized-row/1",
                &[
                    &[row.key.class as u8],
                    &row.source_digest,
                    normalized.bytes(),
                ],
            );
            let accepted = StoredVNextMigration {
                source_digest: row.source_digest,
                class: row.key.class,
                normalized_bytes: normalized.bytes,
                normalized_digest,
            };
            MigrationCommit {
                raw: row.clone(),
                journal: MigrationJournalEntry {
                    batch_id,
                    row_key: row.key.clone(),
                    source_digest: row.source_digest,
                    disposition: MigrationDisposition::VNextDerived,
                    output_digest: normalized_digest,
                },
                accepted: Some(accepted),
                quarantine: None,
            }
        }
        Err(rejection) => {
            let quarantine_id = digest(
                b"legacy-migration-quarantine/1",
                &[
                    &row.source_digest,
                    rejection.reason_code.as_bytes(),
                    &row.raw_bytes,
                ],
            );
            let quarantine = MigrationQuarantineRecord {
                row_key: row.key.clone(),
                source_digest: row.source_digest,
                reason_code: rejection.reason_code,
                original_bytes: row.raw_bytes.clone(),
                quarantine_id,
            };
            MigrationCommit {
                raw: row.clone(),
                journal: MigrationJournalEntry {
                    batch_id,
                    row_key: row.key.clone(),
                    source_digest: row.source_digest,
                    disposition: MigrationDisposition::Quarantined,
                    output_digest: quarantine_id,
                },
                accepted: None,
                quarantine: Some(quarantine),
            }
        }
    }
}

fn digest(domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:");
    hasher.update(domain);
    hasher.update(&[0]);
    for part in parts {
        hasher.update(&(part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    *hasher.finalize().as_bytes()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MigrationError {
    InvalidPrimaryKey,
    RowTooLarge,
    InvalidNormalizedRow,
    InvalidReason,
    DuplicateRowKey,
    Backend(String),
}

impl fmt::Display for MigrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPrimaryKey => f.write_str("MIGRATION_PRIMARY_KEY"),
            Self::RowTooLarge => f.write_str("MIGRATION_ROW_TOO_LARGE"),
            Self::InvalidNormalizedRow => f.write_str("MIGRATION_NORMALIZED_ROW"),
            Self::InvalidReason => f.write_str("MIGRATION_REASON"),
            Self::DuplicateRowKey => f.write_str("MIGRATION_DUPLICATE_ROW_KEY"),
            Self::Backend(message) => write!(f, "MIGRATION_BACKEND: {message}"),
        }
    }
}

impl std::error::Error for MigrationError {}

#[cfg(feature = "persist")]
mod persistent {
    use std::path::Path;

    use redb::{Database, ReadableTable, TableDefinition};

    use super::*;

    const BATCHES: TableDefinition<&[u8], &[u8]> = TableDefinition::new("vnext_migration_batches");
    const JOURNALS: TableDefinition<&[u8], &[u8]> =
        TableDefinition::new("vnext_migration_row_journal");
    const RAW: TableDefinition<&[u8], &[u8]> = TableDefinition::new("legacy_v1_raw_read_only");
    const VNEXT: TableDefinition<&[u8], &[u8]> = TableDefinition::new("vnext_migrated_rows");
    const QUARANTINE: TableDefinition<&[u8], &[u8]> =
        TableDefinition::new("vnext_migration_quarantine");

    pub struct RedbMigrationBackend {
        db: Database,
    }

    impl RedbMigrationBackend {
        pub fn open(path: &Path) -> Result<Self, String> {
            let db = Database::create(path).map_err(|error| error.to_string())?;
            let write = db.begin_write().map_err(|error| error.to_string())?;
            {
                write
                    .open_table(BATCHES)
                    .map_err(|error| error.to_string())?;
                write
                    .open_table(JOURNALS)
                    .map_err(|error| error.to_string())?;
                write.open_table(RAW).map_err(|error| error.to_string())?;
                write.open_table(VNEXT).map_err(|error| error.to_string())?;
                write
                    .open_table(QUARANTINE)
                    .map_err(|error| error.to_string())?;
            }
            write.commit().map_err(|error| error.to_string())?;
            Ok(Self { db })
        }

        fn journal_key(batch_id: &[u8; 32], key: &LegacyRowKey) -> Vec<u8> {
            let row_key = key.storage_key();
            let mut output = Vec::with_capacity(32 + row_key.len());
            output.extend_from_slice(batch_id);
            output.extend_from_slice(&row_key);
            output
        }
    }

    impl AtomicMigrationBackend for RedbMigrationBackend {
        fn begin_batch(&self, batch: &MigrationBatchJournal) -> Result<(), String> {
            let bytes = encode(batch)?;
            let write = self.db.begin_write().map_err(|error| error.to_string())?;
            {
                let mut table = write
                    .open_table(BATCHES)
                    .map_err(|error| error.to_string())?;
                let existing = table
                    .get(batch.batch_id.as_slice())
                    .map_err(|error| error.to_string())?
                    .map(|value| value.value().to_vec());
                match existing {
                    Some(existing) => {
                        let existing: MigrationBatchJournal = decode(&existing)?;
                        if existing.manifest_digest != batch.manifest_digest
                            || existing.expected_rows != batch.expected_rows
                        {
                            return Err("MIGRATION_BATCH_CONFLICT".into());
                        }
                    }
                    None => {
                        table
                            .insert(batch.batch_id.as_slice(), bytes.as_slice())
                            .map_err(|error| error.to_string())?;
                    }
                }
            }
            write.commit().map_err(|error| error.to_string())
        }

        fn commit_row(&self, commit: &MigrationCommit) -> Result<BackendMigrationOutcome, String> {
            let row_key = commit.raw.key.storage_key();
            let journal_key = Self::journal_key(&commit.journal.batch_id, &commit.raw.key);
            let write = self.db.begin_write().map_err(|error| error.to_string())?;

            let batch = {
                let table = write
                    .open_table(BATCHES)
                    .map_err(|error| error.to_string())?;
                let bytes = table
                    .get(commit.journal.batch_id.as_slice())
                    .map_err(|error| error.to_string())?
                    .map(|value| value.value().to_vec())
                    .ok_or_else(|| "MIGRATION_BATCH_NOT_STARTED".to_string())?;
                decode::<MigrationBatchJournal>(&bytes)?
            };

            let existing_journal = {
                let table = write
                    .open_table(JOURNALS)
                    .map_err(|error| error.to_string())?;
                let value = table
                    .get(journal_key.as_slice())
                    .map_err(|error| error.to_string())?
                    .map(|value| value.value().to_vec());
                value
            };
            if let Some(bytes) = existing_journal {
                let existing: MigrationJournalEntry = decode(&bytes)?;
                if existing == commit.journal {
                    return Ok(BackendMigrationOutcome::ExactReplay);
                }
                return Err("MIGRATION_ROW_JOURNAL_CONFLICT".into());
            }
            if batch.complete {
                return Err("MIGRATION_COMPLETED_BATCH_MUTATION".into());
            }

            let existing_raw = {
                let table = write.open_table(RAW).map_err(|error| error.to_string())?;
                let value = table
                    .get(row_key.as_slice())
                    .map_err(|error| error.to_string())?
                    .map(|value| value.value().to_vec());
                value
            };
            if existing_raw
                .as_deref()
                .map(decode::<LegacySourceRow>)
                .transpose()?
                .is_some_and(|existing| existing != commit.raw)
            {
                return Err("MIGRATION_RAW_LEGACY_COLLISION".into());
            }

            if let Some(accepted) = &commit.accepted {
                let existing = {
                    let table = write.open_table(VNEXT).map_err(|error| error.to_string())?;
                    let value = table
                        .get(row_key.as_slice())
                        .map_err(|error| error.to_string())?
                        .map(|value| value.value().to_vec());
                    value
                };
                if existing
                    .as_deref()
                    .map(decode::<StoredVNextMigration>)
                    .transpose()?
                    .is_some_and(|existing| existing != *accepted)
                {
                    return Err("MIGRATION_VNEXT_COLLISION".into());
                }
            }
            if let Some(quarantine) = &commit.quarantine {
                let existing = {
                    let table = write
                        .open_table(QUARANTINE)
                        .map_err(|error| error.to_string())?;
                    let value = table
                        .get(row_key.as_slice())
                        .map_err(|error| error.to_string())?
                        .map(|value| value.value().to_vec());
                    value
                };
                if existing
                    .as_deref()
                    .map(decode::<MigrationQuarantineRecord>)
                    .transpose()?
                    .is_some_and(|existing| existing != *quarantine)
                {
                    return Err("MIGRATION_QUARANTINE_COLLISION".into());
                }
            }

            {
                let mut table = write.open_table(RAW).map_err(|error| error.to_string())?;
                if existing_raw.is_none() {
                    let bytes = encode(&commit.raw)?;
                    table
                        .insert(row_key.as_slice(), bytes.as_slice())
                        .map_err(|error| error.to_string())?;
                }
            }
            if let Some(accepted) = &commit.accepted {
                let mut table = write.open_table(VNEXT).map_err(|error| error.to_string())?;
                let bytes = encode(accepted)?;
                table
                    .insert(row_key.as_slice(), bytes.as_slice())
                    .map_err(|error| error.to_string())?;
            }
            if let Some(quarantine) = &commit.quarantine {
                let mut table = write
                    .open_table(QUARANTINE)
                    .map_err(|error| error.to_string())?;
                let bytes = encode(quarantine)?;
                table
                    .insert(row_key.as_slice(), bytes.as_slice())
                    .map_err(|error| error.to_string())?;
            }
            {
                let mut table = write
                    .open_table(JOURNALS)
                    .map_err(|error| error.to_string())?;
                let bytes = encode(&commit.journal)?;
                table
                    .insert(journal_key.as_slice(), bytes.as_slice())
                    .map_err(|error| error.to_string())?;
            }
            write.commit().map_err(|error| error.to_string())?;
            Ok(BackendMigrationOutcome::Committed)
        }

        fn complete_batch(
            &self,
            batch_id: &[u8; 32],
            manifest_digest: &[u8; 32],
        ) -> Result<(), String> {
            let write = self.db.begin_write().map_err(|error| error.to_string())?;
            let mut batch = {
                let table = write
                    .open_table(BATCHES)
                    .map_err(|error| error.to_string())?;
                let bytes = table
                    .get(batch_id.as_slice())
                    .map_err(|error| error.to_string())?
                    .map(|value| value.value().to_vec())
                    .ok_or_else(|| "MIGRATION_BATCH_NOT_STARTED".to_string())?;
                decode::<MigrationBatchJournal>(&bytes)?
            };
            let count = {
                let table = write
                    .open_table(JOURNALS)
                    .map_err(|error| error.to_string())?;
                let prefix = batch_id.as_slice();
                table
                    .iter()
                    .map_err(|error| error.to_string())?
                    .filter_map(Result::ok)
                    .filter(|(key, _)| key.value().starts_with(prefix))
                    .count() as u64
            };
            if &batch.manifest_digest != manifest_digest || count != batch.expected_rows {
                return Err("MIGRATION_BATCH_INCOMPLETE".into());
            }
            batch.complete = true;
            let bytes = encode(&batch)?;
            {
                let mut table = write
                    .open_table(BATCHES)
                    .map_err(|error| error.to_string())?;
                table
                    .insert(batch_id.as_slice(), bytes.as_slice())
                    .map_err(|error| error.to_string())?;
            }
            write.commit().map_err(|error| error.to_string())
        }

        fn get_batch(&self, batch_id: &[u8; 32]) -> Result<Option<MigrationBatchJournal>, String> {
            self.get(BATCHES, batch_id)
                .and_then(|bytes| bytes.map(|bytes| decode(&bytes)).transpose())
        }

        fn get_journal(
            &self,
            batch_id: &[u8; 32],
            key: &LegacyRowKey,
        ) -> Result<Option<MigrationJournalEntry>, String> {
            self.get(JOURNALS, &Self::journal_key(batch_id, key))
                .and_then(|bytes| bytes.map(|bytes| decode(&bytes)).transpose())
        }

        fn get_raw(&self, key: &LegacyRowKey) -> Result<Option<LegacySourceRow>, String> {
            self.get(RAW, &key.storage_key())
                .and_then(|bytes| bytes.map(|bytes| decode(&bytes)).transpose())
        }

        fn get_vnext(&self, key: &LegacyRowKey) -> Result<Option<StoredVNextMigration>, String> {
            self.get(VNEXT, &key.storage_key())
                .and_then(|bytes| bytes.map(|bytes| decode(&bytes)).transpose())
        }

        fn get_quarantine(
            &self,
            key: &LegacyRowKey,
        ) -> Result<Option<MigrationQuarantineRecord>, String> {
            self.get(QUARANTINE, &key.storage_key())
                .and_then(|bytes| bytes.map(|bytes| decode(&bytes)).transpose())
        }
    }

    impl RedbMigrationBackend {
        fn get(
            &self,
            definition: TableDefinition<&[u8], &[u8]>,
            key: &[u8],
        ) -> Result<Option<Vec<u8>>, String> {
            let read = self.db.begin_read().map_err(|error| error.to_string())?;
            let table = read
                .open_table(definition)
                .map_err(|error| error.to_string())?;
            table
                .get(key)
                .map_err(|error| error.to_string())
                .map(|value| value.map(|guard| guard.value().to_vec()))
        }
    }

    fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
        serde_json::to_vec(value).map_err(|error| error.to_string())
    }

    fn decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, String> {
        serde_json::from_slice(bytes).map_err(|error| error.to_string())
    }

    pub use RedbMigrationBackend as PublicRedbMigrationBackend;
}

#[cfg(feature = "persist")]
pub use persistent::PublicRedbMigrationBackend as RedbMigrationBackend;

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: u8, bytes: &[u8]) -> LegacySourceRow {
        LegacySourceRow::new(
            LegacyRowKey::new(LegacyDataClass::IdentityCounter, vec![id]).unwrap(),
            bytes.to_vec(),
        )
        .unwrap()
    }

    fn normalizer(row: &LegacySourceRow) -> Result<NormalizedLegacyRow, MigrationRejection> {
        if row.raw_bytes().starts_with(b"bad") {
            return Err(MigrationRejection::new("LEGACY_PARSE").unwrap());
        }
        let mut bytes = b"vnext:".to_vec();
        bytes.extend_from_slice(row.raw_bytes());
        NormalizedLegacyRow::new(bytes)
            .map_err(|_| MigrationRejection::new("NORMALIZED_LIMIT").unwrap())
    }

    #[test]
    fn killed_batch_restarts_idempotently_and_preserves_raw() {
        let store = MigrationStore::new(InMemoryMigrationBackend::default());
        let rows = vec![row(1, b"one"), row(2, b"two"), row(3, b"three")];
        let batch = [7; 32];
        assert_eq!(
            store.run_batch(batch, &rows, 1, &normalizer).unwrap(),
            MigrationBatchOutcome::Interrupted {
                committed: 1,
                exact_replays: 0
            }
        );
        assert_eq!(
            store
                .run_batch(batch, &rows, usize::MAX, &normalizer)
                .unwrap(),
            MigrationBatchOutcome::Complete {
                committed: 2,
                exact_replays: 1
            }
        );
        assert!(store.batch_journal(&batch).unwrap().unwrap().complete);
        for source in rows {
            let raw = store.read_raw_for_rollback(source.key()).unwrap().unwrap();
            assert_eq!(raw.raw_bytes(), source.raw_bytes());
            assert!(!raw.is_mutable());
        }
    }

    #[test]
    fn dual_read_prefers_verified_vnext_and_falls_back_to_v1() {
        let store = MigrationStore::new(InMemoryMigrationBackend::default());
        let source = row(1, b"one");
        store
            .run_batch([1; 32], std::slice::from_ref(&source), 1, &normalizer)
            .unwrap();
        assert!(matches!(
            store.read_prefer_verified(source.key(), |_| true).unwrap(),
            Some(DualReadRecord::VerifiedVNext(_))
        ));
        let fallback = store
            .read_prefer_verified(source.key(), |_| false)
            .unwrap()
            .unwrap();
        assert!(matches!(fallback, DualReadRecord::RawLegacy(_)));
    }

    #[test]
    fn copy_on_read_is_idempotent() {
        let store = MigrationStore::new(InMemoryMigrationBackend::default());
        let source = row(9, b"lazy");
        for _ in 0..2 {
            assert!(matches!(
                store
                    .copy_on_read([9; 32], source.clone(), &normalizer, |_| true)
                    .unwrap(),
                Some(DualReadRecord::VerifiedVNext(_))
            ));
        }
        assert!(store.row_journal(&[9; 32], source.key()).unwrap().is_some());
    }

    #[test]
    fn corrupt_row_is_non_executable_quarantine_with_raw_rollback() {
        let store = MigrationStore::new(InMemoryMigrationBackend::default());
        let source = row(2, b"bad-json");
        store
            .run_batch([2; 32], std::slice::from_ref(&source), 1, &normalizer)
            .unwrap();
        let quarantine = store.quarantine(source.key()).unwrap().unwrap();
        assert!(!quarantine.is_executable());
        assert_eq!(quarantine.original_bytes, b"bad-json");
        assert!(matches!(
            store.read_prefer_verified(source.key(), |_| true).unwrap(),
            Some(DualReadRecord::RawLegacy(_))
        ));
    }

    #[test]
    fn legacy_u64_never_claims_full_width_identity() {
        let prefix = LegacyIdentityPrefix::new(42, [3; 32]);
        assert_eq!(prefix.legacy_u64, 42);
        assert!(!prefix.is_full_width_identity());
    }

    #[cfg(feature = "persist")]
    #[test]
    fn redb_reopen_retains_journal_vnext_and_raw_v1() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "onebrain-migration-{}-{nonce}.redb",
            std::process::id()
        ));
        let source = row(4, b"persistent");
        {
            let store = MigrationStore::new(RedbMigrationBackend::open(&path).unwrap());
            store
                .run_batch([4; 32], std::slice::from_ref(&source), 1, &normalizer)
                .unwrap();
        }
        {
            let store = MigrationStore::new(RedbMigrationBackend::open(&path).unwrap());
            assert!(store.batch_journal(&[4; 32]).unwrap().unwrap().complete);
            assert_eq!(
                store
                    .read_raw_for_rollback(source.key())
                    .unwrap()
                    .unwrap()
                    .raw_bytes(),
                b"persistent"
            );
            assert!(matches!(
                store.read_prefer_verified(source.key(), |_| true).unwrap(),
                Some(DualReadRecord::VerifiedVNext(_))
            ));
        }
        let _ = std::fs::remove_file(path);
    }
}
