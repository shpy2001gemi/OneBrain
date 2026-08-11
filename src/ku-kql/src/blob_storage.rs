//! # Persistent Blob Storage — hybrid redb + filesystem (v7)
//!
//! Stores media/file blobs using a size-based hybrid strategy:
//! - **Small blobs (≤ 1 MB)**: chunks stored inline in redb (`blob_chunks` table)
//! - **Large blobs (> 1 MB)**: chunks stored as files on the filesystem under
//!   `<db_dir>/blobs/v2/<digest-shards>/<full-cid>/chunk_NNNN.bin`; only
//!   metadata stays in redb
//!
//! This avoids bloating the redb database with large media files while keeping
//! small blobs fast and transactional.
//!
//! ## Tables
//! - `blob_meta`: OB-CID (34B) → JSON BlobMeta (always in redb)
//! - `blob_chunks`: OB-CID (34B) + index (4B BE) → raw chunk bytes (small blobs only)

use ku_core::blob_store::{
    mime_from_extension, BlobCid, BlobMeta, BlobType, BLOB_CHUNK_SIZE, BLOB_CID_VERSION,
    BLOB_MAX_PER_KU, BLOB_MAX_SIZE,
};
use ku_core::foundation::dr_m5_failpoint;
use ku_core::foundation::ObjectReference;
use ku_core::obs_schema;
use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::blob_layout::{blob_relative_dir, migrate_blob_layout_v2, BlobLayoutMigrationReport};

/// Threshold: blobs larger than this use filesystem storage for chunks.
const FILESYSTEM_SPILL_THRESHOLD: u64 = 1024 * 1024; // 1 MB
pub const BLOB_META_VERSION: u16 = 2;
const DEFAULT_TOTAL_QUOTA_BYTES: u64 = 10 * 1024 * 1024 * 1024;
const DEFAULT_FREE_SPACE_RESERVE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlobStorageConfig {
    pub total_quota_bytes: u64,
    pub free_space_reserve_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlobReadError {
    LengthMismatch,
    ChunkDigestMismatch { index: u32 },
    ContentDigestMismatch,
    TypeMismatch,
}

impl std::fmt::Display for BlobReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LengthMismatch => write!(f, "blob length mismatch"),
            Self::ChunkDigestMismatch { index } => {
                write!(f, "blob chunk {index} digest mismatch")
            }
            Self::ContentDigestMismatch => write!(f, "blob content digest mismatch"),
            Self::TypeMismatch => write!(f, "blob declared type mismatch"),
        }
    }
}

pub trait BlobReferenceOracle: Send + Sync {
    fn referencing_records(&self, cid: &BlobCid) -> Result<Vec<ObjectReference>, BlobStorageError>;
}

#[derive(Debug, Clone, Default)]
pub struct BlobMetadataMigrationReport {
    pub migrated: u64,
    pub already_v2: u64,
    pub corrupt_cids: Vec<BlobCid>,
}

trait AvailableSpace: Send + Sync {
    fn available_bytes(&self, path: &Path) -> Result<u64, BlobStorageError>;
}

struct FilesystemAvailableSpace;

impl AvailableSpace for FilesystemAvailableSpace {
    fn available_bytes(&self, path: &Path) -> Result<u64, BlobStorageError> {
        fs2::available_space(path).map_err(BlobStorageError::from)
    }
}

struct UnknownReferenceOracle;

impl BlobReferenceOracle for UnknownReferenceOracle {
    fn referencing_records(
        &self,
        _cid: &BlobCid,
    ) -> Result<Vec<ObjectReference>, BlobStorageError> {
        Err(BlobStorageError::ReferenceParityUnknown)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
enum FilesystemIntentKind {
    Write,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FilesystemIntent {
    kind: FilesystemIntentKind,
    cid: BlobCid,
    meta: BlobMeta,
}

/// How chunks are stored for a blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StorageMode {
    /// Chunks stored inline in redb (small blobs ≤ 1 MB).
    Redb,
    /// Chunks stored as files on disk (large blobs > 1 MB).
    Filesystem,
}

impl StorageMode {
    fn as_str(&self) -> &'static str {
        match self {
            StorageMode::Redb => "redb",
            StorageMode::Filesystem => "filesystem",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "filesystem" => StorageMode::Filesystem,
            _ => StorageMode::Redb,
        }
    }
}

// Table definitions
const TABLE_BLOB_META: TableDefinition<&[u8], &[u8]> = TableDefinition::new("blob_meta");
const TABLE_BLOB_CHUNKS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("blob_chunks");
const TABLE_BLOB_FS_INTENTS: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("blob_fs_intents");

/// Blob storage error.
#[derive(Debug)]
pub enum BlobStorageError {
    DatabaseError(String),
    IoError(String),
    NotFound,
    TooLarge(u64),
    QuotaExceeded { used: u64, quota: u64 },
    CodecError(String),
    MigrationBlocked(BlobLayoutMigrationReport),
    MigrationRequired { cid: BlobCid },
    InvalidConfig,
    ReserveSpaceExceeded { available: u64, reserve: u64 },
    ArithmeticOverflow,
    Read(BlobReadError),
    ReferenceParityUnknown,
    OwnerBlobLimitExceeded { limit: usize },
    LegacyReferenceReadOnly,
    IntentConflict(String),
}

impl std::fmt::Display for BlobStorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DatabaseError(msg) => write!(f, "Blob storage error: {}", msg),
            Self::IoError(msg) => write!(f, "IO error: {}", msg),
            Self::NotFound => write!(f, "Blob not found"),
            Self::TooLarge(size) => write!(
                f,
                "Blob too large: {} bytes (max: {} bytes)",
                size, BLOB_MAX_SIZE
            ),
            Self::QuotaExceeded { used, quota } => {
                write!(f, "Blob quota exceeded: {} / {} bytes", used, quota)
            }
            Self::CodecError(msg) => write!(f, "Codec error: {}", msg),
            Self::MigrationBlocked(report) => write!(
                f,
                "Blob layout migration blocked: {} collision group(s), {} corrupt CID(s)",
                report.collision_groups.len(),
                report.corrupt_cids.len()
            ),
            Self::MigrationRequired { cid } => {
                write!(f, "blob metadata migration required for {cid}")
            }
            Self::InvalidConfig => write!(f, "blob storage config must contain nonzero limits"),
            Self::ReserveSpaceExceeded { available, reserve } => write!(
                f,
                "blob free-space reserve exceeded: available {available}, reserve {reserve}"
            ),
            Self::ArithmeticOverflow => write!(f, "blob capacity arithmetic overflow"),
            Self::Read(error) => write!(f, "blob integrity error: {error}"),
            Self::ReferenceParityUnknown => write!(f, "canonical blob reference parity unknown"),
            Self::OwnerBlobLimitExceeded { limit } => {
                write!(f, "owner references more than {limit} blobs")
            }
            Self::LegacyReferenceReadOnly => {
                write!(f, "legacy blob reference metadata is read-only evidence")
            }
            Self::IntentConflict(id) => write!(f, "blob filesystem intent conflict: {id}"),
        }
    }
}

impl From<redb::Error> for BlobStorageError {
    fn from(e: redb::Error) -> Self {
        Self::DatabaseError(format!("{}", e))
    }
}
impl From<redb::DatabaseError> for BlobStorageError {
    fn from(e: redb::DatabaseError) -> Self {
        Self::DatabaseError(format!("{}", e))
    }
}
impl From<redb::TransactionError> for BlobStorageError {
    fn from(e: redb::TransactionError) -> Self {
        Self::DatabaseError(format!("{}", e))
    }
}
impl From<redb::TableError> for BlobStorageError {
    fn from(e: redb::TableError) -> Self {
        Self::DatabaseError(format!("{}", e))
    }
}
impl From<redb::StorageError> for BlobStorageError {
    fn from(e: redb::StorageError) -> Self {
        Self::DatabaseError(format!("{}", e))
    }
}
impl From<redb::CommitError> for BlobStorageError {
    fn from(e: redb::CommitError) -> Self {
        Self::DatabaseError(format!("{}", e))
    }
}
impl From<std::io::Error> for BlobStorageError {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(format!("{}", e))
    }
}

/// Persistent blob storage — hybrid redb + filesystem.
///
/// Small blobs (≤ 1 MB) store chunks in redb for transactional safety.
/// Large blobs (> 1 MB) store chunks as files on disk to avoid bloating redb.
/// Metadata is always in redb regardless of blob size.
pub struct BlobStorage {
    db: Database,
    /// Base directory for filesystem chunk storage (`<db_dir>/blobs/`).
    chunk_dir: PathBuf,
    config: BlobStorageConfig,
    references: Arc<dyn BlobReferenceOracle>,
    available_space: Arc<dyn AvailableSpace>,
}

impl BlobStorage {
    /// Open or create blob storage at the given path.
    ///
    /// Creates a `blobs/` directory next to the database file for filesystem
    /// chunk storage of large blobs.
    pub fn open(path: &Path) -> Result<Self, BlobStorageError> {
        Self::open_with_config(
            path,
            BlobStorageConfig {
                total_quota_bytes: DEFAULT_TOTAL_QUOTA_BYTES,
                free_space_reserve_bytes: DEFAULT_FREE_SPACE_RESERVE_BYTES,
            },
            Arc::new(UnknownReferenceOracle),
        )
    }

    pub fn open_with_config(
        path: &Path,
        config: BlobStorageConfig,
        references: Arc<dyn BlobReferenceOracle>,
    ) -> Result<Self, BlobStorageError> {
        Self::open_internal(path, config, references, Arc::new(FilesystemAvailableSpace))
    }

    fn open_internal(
        path: &Path,
        config: BlobStorageConfig,
        references: Arc<dyn BlobReferenceOracle>,
        available_space: Arc<dyn AvailableSpace>,
    ) -> Result<Self, BlobStorageError> {
        if config.total_quota_bytes == 0 || config.free_space_reserve_bytes == 0 {
            return Err(BlobStorageError::InvalidConfig);
        }
        let db = Database::create(path)
            .map_err(|e| BlobStorageError::DatabaseError(format!("{}", e)))?;

        // Ensure tables exist
        {
            let txn = db.begin_write()?;
            {
                let _ = txn.open_table(TABLE_BLOB_META)?;
            }
            {
                let _ = txn.open_table(TABLE_BLOB_CHUNKS)?;
            }
            {
                let _ = txn.open_table(TABLE_BLOB_FS_INTENTS)?;
            }
            txn.commit()?;
        }

        // Schema versioning
        obs_schema::redb_schema::ensure_schema(&db, &obs_schema::blob_store_registry())
            .map_err(|e| BlobStorageError::DatabaseError(format!("Schema error: {}", e)))?;

        // Create chunk directory next to db file
        let chunk_dir = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("blobs");
        std::fs::create_dir_all(&chunk_dir)?;

        let storage = Self {
            db,
            chunk_dir,
            config,
            references,
            available_space,
        };
        let metas = storage.list_blobs()?;
        migrate_blob_layout_v2(&storage.chunk_dir, &metas)?;
        storage.recover_pending_filesystem_intents()?;
        Ok(storage)
    }

    /// Directory for a specific blob's filesystem chunks.
    fn blob_chunk_dir(&self, blob_cid: &BlobCid) -> PathBuf {
        self.chunk_dir.join(blob_relative_dir(blob_cid))
    }

    fn write_staging_dir(&self, blob_cid: &BlobCid) -> PathBuf {
        self.chunk_dir
            .join(".intents")
            .join(format!("{}.write", blob_cid.to_hex()))
    }

    fn delete_staging_dir(&self, blob_cid: &BlobCid) -> PathBuf {
        self.chunk_dir
            .join(".intents")
            .join(format!("{}.delete", blob_cid.to_hex()))
    }

    fn begin_filesystem_intent(&self, intent: FilesystemIntent) -> Result<(), BlobStorageError> {
        dr_m5_failpoint::hit("TX-BLOB-001", "before_begin_write");
        let transaction = self.db.begin_write()?;
        dr_m5_failpoint::hit("TX-BLOB-001", "after_begin_write_before_mutation");
        {
            let mut table = transaction.open_table(TABLE_BLOB_FS_INTENTS)?;
            if table.get(intent.cid.0.as_slice())?.is_some() {
                return Err(BlobStorageError::IntentConflict(intent.cid.to_hex()));
            }
            let encoded = serde_json::to_vec(&intent)
                .map_err(|error| BlobStorageError::CodecError(error.to_string()))?;
            table.insert(intent.cid.0.as_slice(), encoded.as_slice())?;
        }
        dr_m5_failpoint::hit("TX-BLOB-001", "after_mutation_before_commit");
        transaction.commit()?;
        dr_m5_failpoint::hit("TX-BLOB-001", "after_commit_before_next_side_effect");
        Ok(())
    }

    fn complete_filesystem_intent(&self, cid: &BlobCid) -> Result<(), BlobStorageError> {
        let transaction = self.db.begin_write()?;
        {
            let mut table = transaction.open_table(TABLE_BLOB_FS_INTENTS)?;
            table.remove(cid.0.as_slice())?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn recover_pending_filesystem_intents(&self) -> Result<u64, BlobStorageError> {
        let intents = {
            let transaction = self.db.begin_read()?;
            let table = transaction.open_table(TABLE_BLOB_FS_INTENTS)?;
            let mut intents = Vec::new();
            for row in table.iter()? {
                let (_, value) = row?;
                intents.push(
                    serde_json::from_slice::<FilesystemIntent>(value.value())
                        .map_err(|error| BlobStorageError::CodecError(error.to_string()))?,
                );
            }
            intents
        };

        let mut recovered = 0u64;
        for intent in intents {
            match intent.kind {
                FilesystemIntentKind::Write => {
                    let final_dir = self.blob_chunk_dir(&intent.cid);
                    let staging = self.write_staging_dir(&intent.cid);
                    if final_dir.exists() {
                        self.verify_filesystem_directory(&intent.cid, &intent.meta, &final_dir)?;
                    } else if staging.exists() {
                        if self
                            .verify_filesystem_directory(&intent.cid, &intent.meta, &staging)
                            .is_err()
                        {
                            std::fs::remove_dir_all(&staging)?;
                            self.complete_filesystem_intent(&intent.cid)?;
                            recovered += 1;
                            continue;
                        }
                        let parent = final_dir.parent().ok_or_else(|| {
                            BlobStorageError::IoError("blob v2 directory has no parent".into())
                        })?;
                        std::fs::create_dir_all(parent)?;
                        std::fs::rename(&staging, &final_dir)?;
                        sync_directory(parent)?;
                    } else {
                        self.complete_filesystem_intent(&intent.cid)?;
                        recovered += 1;
                        continue;
                    }
                    let transaction = self.db.begin_write()?;
                    {
                        let encoded = serde_json::to_vec(&intent.meta)
                            .map_err(|error| BlobStorageError::CodecError(error.to_string()))?;
                        let mut meta = transaction.open_table(TABLE_BLOB_META)?;
                        meta.insert(intent.cid.0.as_slice(), encoded.as_slice())?;
                        let mut table = transaction.open_table(TABLE_BLOB_FS_INTENTS)?;
                        table.remove(intent.cid.0.as_slice())?;
                    }
                    transaction.commit()?;
                }
                FilesystemIntentKind::Delete => {
                    let final_dir = self.blob_chunk_dir(&intent.cid);
                    let staging = self.delete_staging_dir(&intent.cid);
                    if final_dir.exists() && !staging.exists() {
                        let parent = staging.parent().ok_or_else(|| {
                            BlobStorageError::IoError("delete staging has no parent".into())
                        })?;
                        std::fs::create_dir_all(parent)?;
                        std::fs::rename(&final_dir, &staging)?;
                    }
                    let transaction = self.db.begin_write()?;
                    {
                        let mut meta = transaction.open_table(TABLE_BLOB_META)?;
                        meta.remove(intent.cid.0.as_slice())?;
                    }
                    transaction.commit()?;
                    if staging.exists() {
                        std::fs::remove_dir_all(&staging)?;
                    }
                    self.complete_filesystem_intent(&intent.cid)?;
                }
            }
            recovered += 1;
        }
        Ok(recovered)
    }

    /// Determine storage mode based on data size.
    fn storage_mode_for(total_size: u64) -> StorageMode {
        if total_size > FILESYSTEM_SPILL_THRESHOLD {
            StorageMode::Filesystem
        } else {
            StorageMode::Redb
        }
    }

    /// Store a file as a blob. Returns metadata.
    ///
    /// 1. Reads entire file into memory
    /// 2. Computes BLAKE3 hash → BlobCid
    /// 3. Checks for dedup (if CID exists, just add reference)
    /// 4. Chunks file into 256KB pieces
    /// 5. Small blobs (≤ 1 MB): chunks → redb (transactional)
    /// 6. Large blobs (> 1 MB): chunks → filesystem (avoids redb bloat)
    pub fn store_file(&self, file_path: &Path) -> Result<BlobMeta, BlobStorageError> {
        self.store_file_internal(file_path, None)
    }

    /// Store a prepared upload only when its exact CID, type, and length match.
    /// The binding is checked before quota admission or any durable mutation.
    pub fn store_file_bound(
        &self,
        file_path: &Path,
        expected_cid: &BlobCid,
        expected_type: BlobType,
        expected_length: u64,
    ) -> Result<BlobMeta, BlobStorageError> {
        self.store_file_internal(
            file_path,
            Some((expected_cid, expected_type, expected_length)),
        )
    }

    fn store_file_internal(
        &self,
        file_path: &Path,
        expected: Option<(&BlobCid, BlobType, u64)>,
    ) -> Result<BlobMeta, BlobStorageError> {
        let total_size = std::fs::metadata(file_path)?.len();
        if total_size > BLOB_MAX_SIZE {
            return Err(BlobStorageError::TooLarge(total_size));
        }
        let mut file = std::fs::File::open(file_path)?;
        let mut magic = [0u8; 12];
        let magic_len = file.read(&mut magic)?;
        file.seek(SeekFrom::Start(0))?;
        let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let blob_type = BlobType::detect(Some(ext), &magic[..magic_len]);
        let mime_type = mime_from_extension(ext).to_string();
        let mut full = blake3::Hasher::new();
        let mut chunk_blake3 = Vec::new();
        let mut observed = 0u64;
        let mut buffer = vec![0u8; BLOB_CHUNK_SIZE];
        loop {
            let read = read_one_chunk(&mut file, &mut buffer)?;
            if read == 0 {
                break;
            }
            observed = observed
                .checked_add(read as u64)
                .ok_or(BlobStorageError::ArithmeticOverflow)?;
            full.update(&buffer[..read]);
            chunk_blake3.push(chunk_digest(&buffer[..read]));
        }
        if observed != total_size {
            return Err(BlobStorageError::Read(BlobReadError::LengthMismatch));
        }
        let digest = *full.finalize().as_bytes();
        let mut cid_bytes = [0u8; 34];
        cid_bytes[0] = BLOB_CID_VERSION;
        cid_bytes[1] = blob_type as u8;
        cid_bytes[2..].copy_from_slice(&digest);
        let blob_cid = BlobCid(cid_bytes);

        if let Some((expected_cid, expected_type, expected_length)) = expected {
            if blob_type != expected_type {
                return Err(BlobStorageError::Read(BlobReadError::TypeMismatch));
            }
            if total_size != expected_length {
                return Err(BlobStorageError::Read(BlobReadError::LengthMismatch));
            }
            if &blob_cid != expected_cid {
                return Err(BlobStorageError::Read(BlobReadError::ContentDigestMismatch));
            }
        }

        // Check dedup
        if self.has_blob(&blob_cid)? {
            return self.get_meta(&blob_cid);
        }
        self.admit_unique_bytes(total_size)?;

        let chunk_count =
            u32::try_from(chunk_blake3.len()).map_err(|_| BlobStorageError::ArithmeticOverflow)?;
        let original_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let mode = Self::storage_mode_for(total_size);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let meta = BlobMeta {
            meta_version: BLOB_META_VERSION,
            blob_cid_hex: blob_cid.to_hex(),
            original_name,
            mime_type,
            total_size,
            chunk_count,
            chunk_size: BLOB_CHUNK_SIZE as u32,
            blob_type: blob_type as u8,
            created_at: now,
            blake3_hex: blob_cid
                .blake3_hash()
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect(),
            referencing_kus: vec![],
            pinned: false,
            storage_mode: mode.as_str().to_string(),
            chunk_blake3,
        };

        // Write chunks according to storage mode
        self.write_file_chunks(file_path, &blob_cid, &meta, mode)?;

        // Write meta in redb (always)
        let txn = self.db.begin_write()?;
        {
            let meta_json = serde_json::to_vec(&meta)
                .map_err(|e| BlobStorageError::CodecError(format!("{}", e)))?;
            let mut table = txn.open_table(TABLE_BLOB_META)?;
            table.insert(blob_cid.0.as_slice(), meta_json.as_slice())?;
        }
        txn.commit()?;
        if mode == StorageMode::Filesystem {
            self.complete_filesystem_intent(&blob_cid)?;
        }

        Ok(meta)
    }

    /// Store raw bytes as a blob (for programmatic use).
    pub fn store_bytes(
        &self,
        name: &str,
        data: &[u8],
        blob_type: BlobType,
    ) -> Result<BlobMeta, BlobStorageError> {
        let total_size = data.len() as u64;
        if total_size > BLOB_MAX_SIZE {
            return Err(BlobStorageError::TooLarge(total_size));
        }

        let blob_cid = BlobCid::from_content(blob_type, data);

        if self.has_blob(&blob_cid)? {
            return self.get_meta(&blob_cid);
        }
        self.admit_unique_bytes(total_size)?;

        let chunk_count = data.len().div_ceil(BLOB_CHUNK_SIZE) as u32;
        let mode = Self::storage_mode_for(total_size);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let meta = BlobMeta {
            meta_version: BLOB_META_VERSION,
            blob_cid_hex: blob_cid.to_hex(),
            original_name: name.to_string(),
            mime_type: "application/octet-stream".to_string(),
            total_size,
            chunk_count,
            chunk_size: BLOB_CHUNK_SIZE as u32,
            blob_type: blob_type as u8,
            created_at: now,
            blake3_hex: blob_cid
                .blake3_hash()
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect(),
            referencing_kus: vec![],
            pinned: false,
            storage_mode: mode.as_str().to_string(),
            chunk_blake3: data.chunks(BLOB_CHUNK_SIZE).map(chunk_digest).collect(),
        };

        self.write_chunks(&blob_cid, data, &meta, mode)?;

        let txn = self.db.begin_write()?;
        {
            let meta_json = serde_json::to_vec(&meta)
                .map_err(|e| BlobStorageError::CodecError(format!("{}", e)))?;
            let mut table = txn.open_table(TABLE_BLOB_META)?;
            table.insert(blob_cid.0.as_slice(), meta_json.as_slice())?;
        }
        txn.commit()?;
        if mode == StorageMode::Filesystem {
            self.complete_filesystem_intent(&blob_cid)?;
        }

        Ok(meta)
    }

    // ── Internal: chunk write/read/delete by storage mode ──────────────

    /// Write chunks to redb or filesystem depending on mode.
    fn write_chunks(
        &self,
        blob_cid: &BlobCid,
        data: &[u8],
        meta: &BlobMeta,
        mode: StorageMode,
    ) -> Result<(), BlobStorageError> {
        match mode {
            StorageMode::Redb => {
                let txn = self.db.begin_write()?;
                {
                    let mut table = txn.open_table(TABLE_BLOB_CHUNKS)?;
                    for i in 0..meta.chunk_count {
                        let start = (i as usize) * BLOB_CHUNK_SIZE;
                        let end = ((i as usize + 1) * BLOB_CHUNK_SIZE).min(data.len());
                        let mut key = Vec::with_capacity(38);
                        key.extend_from_slice(&blob_cid.0);
                        key.extend_from_slice(&i.to_be_bytes());
                        table.insert(key.as_slice(), &data[start..end])?;
                    }
                }
                txn.commit()?;
            }
            StorageMode::Filesystem => {
                self.begin_filesystem_intent(FilesystemIntent {
                    kind: FilesystemIntentKind::Write,
                    cid: *blob_cid,
                    meta: meta.clone(),
                })?;
                let dir = self.write_staging_dir(blob_cid);
                if dir.exists() {
                    return Err(BlobStorageError::IntentConflict(blob_cid.to_hex()));
                }
                let parent = dir.parent().ok_or_else(|| {
                    BlobStorageError::IoError("blob staging directory has no parent".into())
                })?;
                std::fs::create_dir_all(parent)?;
                std::fs::create_dir(&dir)?;
                for i in 0..meta.chunk_count {
                    let start = (i as usize) * BLOB_CHUNK_SIZE;
                    let end = ((i as usize + 1) * BLOB_CHUNK_SIZE).min(data.len());
                    let chunk_path = dir.join(format!("chunk_{:04}.bin", i));
                    let mut file = OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&chunk_path)?;
                    file.write_all(&data[start..end])?;
                    file.sync_all()?;
                }
                let final_dir = self.blob_chunk_dir(blob_cid);
                if final_dir.exists() {
                    return Err(BlobStorageError::IntentConflict(blob_cid.to_hex()));
                }
                let final_parent = final_dir.parent().ok_or_else(|| {
                    BlobStorageError::IoError("blob v2 directory has no parent".into())
                })?;
                std::fs::create_dir_all(final_parent)?;
                std::fs::rename(&dir, &final_dir)?;
                sync_directory(final_parent)?;
                dr_m5_failpoint::hit("TX-BLOB-001", "after_next_side_effect_before_ack");
            }
        }
        Ok(())
    }

    fn write_file_chunks(
        &self,
        file_path: &Path,
        blob_cid: &BlobCid,
        meta: &BlobMeta,
        mode: StorageMode,
    ) -> Result<(), BlobStorageError> {
        let mut source = std::fs::File::open(file_path)?;
        let mut buffer = vec![0u8; BLOB_CHUNK_SIZE];
        match mode {
            StorageMode::Redb => {
                let transaction = self.db.begin_write()?;
                {
                    let mut table = transaction.open_table(TABLE_BLOB_CHUNKS)?;
                    for index in 0..meta.chunk_count {
                        let read = read_one_chunk(&mut source, &mut buffer)?;
                        self.validate_streamed_chunk(meta, index, &buffer[..read])?;
                        let mut key = Vec::with_capacity(38);
                        key.extend_from_slice(&blob_cid.0);
                        key.extend_from_slice(&index.to_be_bytes());
                        table.insert(key.as_slice(), &buffer[..read])?;
                    }
                }
                if read_one_chunk(&mut source, &mut buffer)? != 0 {
                    return Err(BlobStorageError::Read(BlobReadError::LengthMismatch));
                }
                transaction.commit()?;
            }
            StorageMode::Filesystem => {
                self.begin_filesystem_intent(FilesystemIntent {
                    kind: FilesystemIntentKind::Write,
                    cid: *blob_cid,
                    meta: meta.clone(),
                })?;
                let staging = self.write_staging_dir(blob_cid);
                if staging.exists() {
                    return Err(BlobStorageError::IntentConflict(blob_cid.to_hex()));
                }
                let parent = staging.parent().ok_or_else(|| {
                    BlobStorageError::IoError("blob staging directory has no parent".into())
                })?;
                std::fs::create_dir_all(parent)?;
                std::fs::create_dir(&staging)?;
                for index in 0..meta.chunk_count {
                    let read = read_one_chunk(&mut source, &mut buffer)?;
                    self.validate_streamed_chunk(meta, index, &buffer[..read])?;
                    let mut output = OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(staging.join(format!("chunk_{index:04}.bin")))?;
                    output.write_all(&buffer[..read])?;
                    output.sync_all()?;
                }
                if read_one_chunk(&mut source, &mut buffer)? != 0 {
                    return Err(BlobStorageError::Read(BlobReadError::LengthMismatch));
                }
                let final_dir = self.blob_chunk_dir(blob_cid);
                if final_dir.exists() {
                    return Err(BlobStorageError::IntentConflict(blob_cid.to_hex()));
                }
                let parent = final_dir.parent().ok_or_else(|| {
                    BlobStorageError::IoError("blob v2 directory has no parent".into())
                })?;
                std::fs::create_dir_all(parent)?;
                std::fs::rename(&staging, &final_dir)?;
                sync_directory(parent)?;
                dr_m5_failpoint::hit("TX-BLOB-001", "after_next_side_effect_before_ack");
            }
        }
        Ok(())
    }

    fn validate_streamed_chunk(
        &self,
        meta: &BlobMeta,
        index: u32,
        bytes: &[u8],
    ) -> Result<(), BlobStorageError> {
        if bytes.len() as u64 != expected_chunk_length(meta, index)? {
            return Err(BlobStorageError::Read(BlobReadError::LengthMismatch));
        }
        if chunk_digest(bytes) != meta.chunk_blake3[index as usize] {
            return Err(BlobStorageError::Read(BlobReadError::ChunkDigestMismatch {
                index,
            }));
        }
        Ok(())
    }

    /// Read a single chunk from the appropriate backend.
    fn read_chunk_internal(
        &self,
        blob_cid: &BlobCid,
        index: u32,
        mode: StorageMode,
    ) -> Result<Vec<u8>, BlobStorageError> {
        match mode {
            StorageMode::Redb => {
                let mut key = Vec::with_capacity(38);
                key.extend_from_slice(&blob_cid.0);
                key.extend_from_slice(&index.to_be_bytes());
                let txn = self.db.begin_read()?;
                let table = txn.open_table(TABLE_BLOB_CHUNKS)?;
                let guard = table
                    .get(key.as_slice())?
                    .ok_or(BlobStorageError::NotFound)?;
                Ok(guard.value().to_vec())
            }
            StorageMode::Filesystem => {
                let chunk_path = self
                    .blob_chunk_dir(blob_cid)
                    .join(format!("chunk_{:04}.bin", index));
                std::fs::read(&chunk_path).map_err(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        BlobStorageError::NotFound
                    } else {
                        BlobStorageError::IoError(format!("{}", e))
                    }
                })
            }
        }
    }

    /// Delete all chunks for a blob from the appropriate backend.
    fn delete_chunks_internal(
        &self,
        blob_cid: &BlobCid,
        chunk_count: u32,
        mode: StorageMode,
    ) -> Result<(), BlobStorageError> {
        match mode {
            StorageMode::Redb => {
                let txn = self.db.begin_write()?;
                {
                    let mut table = txn.open_table(TABLE_BLOB_CHUNKS)?;
                    for i in 0..chunk_count {
                        let mut key = Vec::with_capacity(38);
                        key.extend_from_slice(&blob_cid.0);
                        key.extend_from_slice(&i.to_be_bytes());
                        let _ = table.remove(key.as_slice())?;
                    }
                }
                txn.commit()?;
            }
            StorageMode::Filesystem => {
                let dir = self.blob_chunk_dir(blob_cid);
                if dir.exists() {
                    std::fs::remove_dir_all(&dir)?;
                }
            }
        }
        Ok(())
    }

    /// Check if a blob exists.
    pub fn has_blob(&self, blob_cid: &BlobCid) -> Result<bool, BlobStorageError> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(TABLE_BLOB_META)?;
        Ok(table.get(blob_cid.0.as_slice())?.is_some())
    }

    /// Get blob metadata.
    pub fn get_meta(&self, blob_cid: &BlobCid) -> Result<BlobMeta, BlobStorageError> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(TABLE_BLOB_META)?;
        let guard = table
            .get(blob_cid.0.as_slice())?
            .ok_or(BlobStorageError::NotFound)?;
        let meta: BlobMeta = serde_json::from_slice(guard.value())
            .map_err(|e| BlobStorageError::CodecError(format!("{}", e)))?;
        Ok(meta)
    }

    /// Read a single chunk (auto-detects storage mode from metadata).
    pub fn get_chunk(&self, blob_cid: &BlobCid, index: u32) -> Result<Vec<u8>, BlobStorageError> {
        let meta = self.get_meta(blob_cid)?;
        self.require_verified_meta(blob_cid, &meta)?;
        if index >= meta.chunk_count {
            return Err(BlobStorageError::NotFound);
        }
        let mode = StorageMode::from_str(&meta.storage_mode);
        let chunk = self.read_chunk_internal(blob_cid, index, mode)?;
        if chunk.len() as u64 != expected_chunk_length(&meta, index)? {
            return Err(BlobStorageError::Read(BlobReadError::LengthMismatch));
        }
        if chunk_digest(&chunk) != meta.chunk_blake3[index as usize] {
            return Err(BlobStorageError::Read(BlobReadError::ChunkDigestMismatch {
                index,
            }));
        }
        Ok(chunk)
    }

    /// Reassemble and return the full blob data.
    pub fn read_full_blob(&self, blob_cid: &BlobCid) -> Result<Vec<u8>, BlobStorageError> {
        let meta = self.get_meta(blob_cid)?;
        self.require_verified_meta(blob_cid, &meta)?;
        self.verify_declared_type(blob_cid, &meta)?;
        let mut result = Vec::with_capacity(meta.total_size as usize);

        for i in 0..meta.chunk_count {
            let chunk = self.get_chunk(blob_cid, i)?;
            result.extend_from_slice(&chunk);
        }
        if result.len() as u64 != meta.total_size {
            return Err(BlobStorageError::Read(BlobReadError::LengthMismatch));
        }
        if blake3::hash(&result).as_bytes() != blob_cid.blake3_hash()
            || encode_hex(blob_cid.blake3_hash()) != meta.blake3_hex
        {
            return Err(BlobStorageError::Read(BlobReadError::ContentDigestMismatch));
        }
        Ok(result)
    }

    /// Export a blob to a file.
    pub fn export_to_file(
        &self,
        blob_cid: &BlobCid,
        output_path: &Path,
    ) -> Result<u64, BlobStorageError> {
        let data = self.read_full_blob(blob_cid)?;
        let size = data.len() as u64;
        std::fs::write(output_path, &data)?;
        Ok(size)
    }

    /// List all blob metadata.
    pub fn list_blobs(&self) -> Result<Vec<BlobMeta>, BlobStorageError> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(TABLE_BLOB_META)?;
        let mut blobs = Vec::new();

        let iter = table.iter()?;
        for entry in iter {
            let (_, value) =
                entry.map_err(|e| BlobStorageError::DatabaseError(format!("{}", e)))?;
            let meta = serde_json::from_slice::<BlobMeta>(value.value()).map_err(|error| {
                BlobStorageError::CodecError(format!("invalid blob metadata: {error}"))
            })?;
            blobs.push(meta);
        }

        Ok(blobs)
    }

    /// Count total blobs.
    pub fn blob_count(&self) -> Result<usize, BlobStorageError> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(TABLE_BLOB_META)?;
        Ok(table.len()? as usize)
    }

    /// Total size of all stored blobs (from metadata).
    pub fn total_blob_size(&self) -> Result<u64, BlobStorageError> {
        let blobs = self.list_blobs()?;
        blobs.iter().try_fold(0u64, |total, meta| {
            total
                .checked_add(meta.total_size)
                .ok_or(BlobStorageError::ArithmeticOverflow)
        })
    }

    /// Add a KU reference to a blob.
    pub fn add_ku_reference(
        &self,
        blob_cid: &BlobCid,
        ku_cid_hex: &str,
    ) -> Result<(), BlobStorageError> {
        let _ = (blob_cid, ku_cid_hex);
        Err(BlobStorageError::LegacyReferenceReadOnly)
    }

    /// Remove a KU reference from a blob.
    pub fn remove_ku_reference(
        &self,
        blob_cid: &BlobCid,
        ku_cid_hex: &str,
    ) -> Result<(), BlobStorageError> {
        let _ = (blob_cid, ku_cid_hex);
        Err(BlobStorageError::LegacyReferenceReadOnly)
    }

    /// Pin/unpin a blob.
    pub fn set_pinned(&self, blob_cid: &BlobCid, pinned: bool) -> Result<(), BlobStorageError> {
        let _ = (blob_cid, pinned);
        Err(BlobStorageError::LegacyReferenceReadOnly)
    }

    /// Delete a blob and all its chunks.
    pub fn delete_blob(&self, blob_cid: &BlobCid) -> Result<bool, BlobStorageError> {
        if !self.canonical_references(blob_cid)?.is_empty() {
            return Ok(false);
        }
        self.delete_blob_unreferenced(blob_cid)
    }

    fn delete_blob_unreferenced(&self, blob_cid: &BlobCid) -> Result<bool, BlobStorageError> {
        let meta = match self.get_meta(blob_cid) {
            Ok(m) => m,
            Err(BlobStorageError::NotFound) => return Ok(false),
            Err(e) => return Err(e),
        };

        let mode = StorageMode::from_str(&meta.storage_mode);
        if mode == StorageMode::Filesystem {
            self.begin_filesystem_intent(FilesystemIntent {
                kind: FilesystemIntentKind::Delete,
                cid: *blob_cid,
                meta: meta.clone(),
            })?;
            let final_dir = self.blob_chunk_dir(blob_cid);
            let staging = self.delete_staging_dir(blob_cid);
            if final_dir.exists() {
                let parent = staging.parent().ok_or_else(|| {
                    BlobStorageError::IoError("delete staging has no parent".into())
                })?;
                std::fs::create_dir_all(parent)?;
                if staging.exists() {
                    return Err(BlobStorageError::IntentConflict(blob_cid.to_hex()));
                }
                std::fs::rename(&final_dir, &staging)?;
                sync_directory(parent)?;
            }
            dr_m5_failpoint::hit("TX-BLOB-001", "after_next_side_effect_before_ack");
        } else {
            self.delete_chunks_internal(blob_cid, meta.chunk_count, mode)?;
        }

        // Delete meta from redb
        let txn = self.db.begin_write()?;
        {
            let mut table = txn.open_table(TABLE_BLOB_META)?;
            let _ = table.remove(blob_cid.0.as_slice())?;
        }
        txn.commit()?;
        if mode == StorageMode::Filesystem {
            let staging = self.delete_staging_dir(blob_cid);
            if staging.exists() {
                std::fs::remove_dir_all(staging)?;
            }
            self.complete_filesystem_intent(blob_cid)?;
        }

        Ok(true)
    }

    /// Garbage collect orphaned blobs (zero references, not pinned).
    /// Returns number of blobs deleted and bytes freed.
    pub fn garbage_collect(&self) -> Result<(usize, u64), BlobStorageError> {
        let blobs = self.list_blobs()?;
        let mut owner_counts: std::collections::BTreeMap<(u64, [u8; 32]), usize> =
            std::collections::BTreeMap::new();
        let mut orphans = Vec::new();
        for meta in &blobs {
            let cid = BlobCid::from_hex(&meta.blob_cid_hex)
                .ok_or_else(|| BlobStorageError::CodecError("invalid blob CID metadata".into()))?;
            let references = self.canonical_references(&cid)?;
            if references.is_empty() {
                orphans.push((cid, meta.total_size));
            }
            for owner in references {
                let count = owner_counts
                    .entry((owner.reference_kind, owner.cid))
                    .or_default();
                *count = count
                    .checked_add(1)
                    .ok_or(BlobStorageError::ArithmeticOverflow)?;
                if *count > BLOB_MAX_PER_KU {
                    return Err(BlobStorageError::OwnerBlobLimitExceeded {
                        limit: BLOB_MAX_PER_KU,
                    });
                }
            }
        }

        let mut deleted = 0;
        let mut freed = 0u64;

        for (cid, size) in orphans {
            if self.delete_blob_unreferenced(&cid)? {
                deleted += 1;
                freed = freed
                    .checked_add(size)
                    .ok_or(BlobStorageError::ArithmeticOverflow)?;
            }
        }

        Ok((deleted, freed))
    }

    fn admit_unique_bytes(&self, incoming: u64) -> Result<(), BlobStorageError> {
        let used = self.total_blob_size()?;
        let projected = used
            .checked_add(incoming)
            .ok_or(BlobStorageError::ArithmeticOverflow)?;
        if projected > self.config.total_quota_bytes {
            return Err(BlobStorageError::QuotaExceeded {
                used: projected,
                quota: self.config.total_quota_bytes,
            });
        }
        let available = self.available_space.available_bytes(&self.chunk_dir)?;
        let remaining =
            available
                .checked_sub(incoming)
                .ok_or(BlobStorageError::ReserveSpaceExceeded {
                    available,
                    reserve: self.config.free_space_reserve_bytes,
                })?;
        if remaining < self.config.free_space_reserve_bytes {
            return Err(BlobStorageError::ReserveSpaceExceeded {
                available,
                reserve: self.config.free_space_reserve_bytes,
            });
        }
        Ok(())
    }

    fn require_verified_meta(
        &self,
        cid: &BlobCid,
        meta: &BlobMeta,
    ) -> Result<(), BlobStorageError> {
        if meta.meta_version != BLOB_META_VERSION
            || meta.chunk_blake3.len() != meta.chunk_count as usize
            || meta
                .chunk_blake3
                .iter()
                .any(|digest| !is_exact_lower_hex(digest, 64))
        {
            return Err(BlobStorageError::MigrationRequired { cid: *cid });
        }
        if meta.total_size > BLOB_MAX_SIZE
            || meta.chunk_size == 0
            || meta.chunk_size as usize > BLOB_CHUNK_SIZE
            || !matches!(meta.storage_mode.as_str(), "redb" | "filesystem")
            || u64::from(meta.chunk_count) != meta.total_size.div_ceil(u64::from(meta.chunk_size))
        {
            return Err(BlobStorageError::Read(BlobReadError::LengthMismatch));
        }
        Ok(())
    }

    fn verify_declared_type(&self, cid: &BlobCid, meta: &BlobMeta) -> Result<(), BlobStorageError> {
        if cid.version() != BLOB_CID_VERSION
            || cid.blob_type() as u8 != cid.0[1]
            || cid.0[1] != meta.blob_type
            || meta.blob_cid_hex != cid.to_hex()
        {
            return Err(BlobStorageError::Read(BlobReadError::TypeMismatch));
        }
        Ok(())
    }

    fn canonical_references(
        &self,
        cid: &BlobCid,
    ) -> Result<Vec<ObjectReference>, BlobStorageError> {
        let mut references = self.references.referencing_records(cid)?;
        references.sort_by_key(|reference| (reference.reference_kind, reference.cid));
        references.dedup_by_key(|reference| (reference.reference_kind, reference.cid));
        Ok(references)
    }

    fn verify_filesystem_directory(
        &self,
        cid: &BlobCid,
        meta: &BlobMeta,
        directory: &Path,
    ) -> Result<(), BlobStorageError> {
        self.require_verified_meta(cid, meta)?;
        self.verify_declared_type(cid, meta)?;
        let mut full = blake3::Hasher::new();
        let mut total = 0u64;
        for index in 0..meta.chunk_count {
            let path = directory.join(format!("chunk_{index:04}.bin"));
            let bytes = std::fs::read(path)?;
            if bytes.len() as u64 != expected_chunk_length(meta, index)? {
                return Err(BlobStorageError::Read(BlobReadError::LengthMismatch));
            }
            if chunk_digest(&bytes) != meta.chunk_blake3[index as usize] {
                return Err(BlobStorageError::Read(BlobReadError::ChunkDigestMismatch {
                    index,
                }));
            }
            total = total
                .checked_add(bytes.len() as u64)
                .ok_or(BlobStorageError::ArithmeticOverflow)?;
            full.update(&bytes);
        }
        if total != meta.total_size {
            return Err(BlobStorageError::Read(BlobReadError::LengthMismatch));
        }
        if full.finalize().as_bytes() != cid.blake3_hash() {
            return Err(BlobStorageError::Read(BlobReadError::ContentDigestMismatch));
        }
        Ok(())
    }

    pub fn migrate_blob_metadata_v2(
        &self,
    ) -> Result<BlobMetadataMigrationReport, BlobStorageError> {
        let metas = self.list_blobs()?;
        let mut report = BlobMetadataMigrationReport::default();
        let mut upgrades = Vec::new();
        for mut meta in metas {
            let Some(cid) = BlobCid::from_hex(&meta.blob_cid_hex) else {
                return Err(BlobStorageError::CodecError(
                    "legacy metadata has a noncanonical CID".into(),
                ));
            };
            if meta.meta_version == BLOB_META_VERSION {
                if self.read_full_blob(&cid).is_err() {
                    report.corrupt_cids.push(cid);
                } else {
                    report.already_v2 += 1;
                }
                continue;
            }
            if meta.total_size > BLOB_MAX_SIZE
                || meta.chunk_size == 0
                || meta.chunk_size as usize > BLOB_CHUNK_SIZE
                || !matches!(meta.storage_mode.as_str(), "redb" | "filesystem")
                || u64::from(meta.chunk_count)
                    != meta.total_size.div_ceil(u64::from(meta.chunk_size))
            {
                report.corrupt_cids.push(cid);
                continue;
            }
            let mode = StorageMode::from_str(&meta.storage_mode);
            let mut full = blake3::Hasher::new();
            let mut total = 0u64;
            let mut chunk_digests = Vec::with_capacity(meta.chunk_count as usize);
            let mut valid = true;
            for index in 0..meta.chunk_count {
                let chunk = match self.read_chunk_internal(&cid, index, mode) {
                    Ok(chunk) => chunk,
                    Err(_) => {
                        valid = false;
                        break;
                    }
                };
                let expected = match expected_chunk_length(&meta, index) {
                    Ok(value) => value,
                    Err(_) => {
                        valid = false;
                        break;
                    }
                };
                if chunk.len() as u64 != expected {
                    valid = false;
                    break;
                }
                total = total
                    .checked_add(chunk.len() as u64)
                    .ok_or(BlobStorageError::ArithmeticOverflow)?;
                full.update(&chunk);
                chunk_digests.push(chunk_digest(&chunk));
            }
            if !valid
                || total != meta.total_size
                || cid.version() != BLOB_CID_VERSION
                || cid.blob_type() as u8 != cid.0[1]
                || cid.0[1] != meta.blob_type
                || full.finalize().as_bytes() != cid.blake3_hash()
                || meta.blake3_hex != encode_hex(cid.blake3_hash())
            {
                report.corrupt_cids.push(cid);
                continue;
            }
            meta.meta_version = BLOB_META_VERSION;
            meta.chunk_blake3 = chunk_digests;
            upgrades.push((cid, meta));
        }
        if !report.corrupt_cids.is_empty() {
            report.corrupt_cids.sort_by_key(|cid| cid.0);
            let blocked = BlobLayoutMigrationReport {
                migrated: 0,
                already_v2: report.already_v2,
                collision_groups: Vec::new(),
                corrupt_cids: report.corrupt_cids,
            };
            return Err(BlobStorageError::MigrationBlocked(blocked));
        }

        let transaction = self.db.begin_write()?;
        {
            let mut table = transaction.open_table(TABLE_BLOB_META)?;
            for (cid, meta) in &upgrades {
                let encoded = serde_json::to_vec(meta)
                    .map_err(|error| BlobStorageError::CodecError(error.to_string()))?;
                table.insert(cid.0.as_slice(), encoded.as_slice())?;
            }
        }
        transaction.commit()?;
        report.migrated = upgrades.len() as u64;
        Ok(report)
    }
}

fn expected_chunk_length(meta: &BlobMeta, index: u32) -> Result<u64, BlobStorageError> {
    if meta.chunk_size == 0 || index >= meta.chunk_count {
        return Err(BlobStorageError::Read(BlobReadError::LengthMismatch));
    }
    let offset = u64::from(index)
        .checked_mul(u64::from(meta.chunk_size))
        .ok_or(BlobStorageError::ArithmeticOverflow)?;
    let remaining = meta
        .total_size
        .checked_sub(offset)
        .ok_or(BlobStorageError::Read(BlobReadError::LengthMismatch))?;
    Ok(remaining.min(u64::from(meta.chunk_size)))
}

fn read_one_chunk(reader: &mut impl Read, buffer: &mut [u8]) -> Result<usize, BlobStorageError> {
    let mut filled = 0usize;
    while filled < buffer.len() {
        let read = reader.read(&mut buffer[filled..])?;
        if read == 0 {
            break;
        }
        filled = filled
            .checked_add(read)
            .ok_or(BlobStorageError::ArithmeticOverflow)?;
    }
    Ok(filled)
}

fn chunk_digest(bytes: &[u8]) -> String {
    encode_hex(blake3::hash(bytes).as_bytes())
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn is_exact_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), BlobStorageError> {
    std::fs::File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), BlobStorageError> {
    // Windows does not expose a portable FlushFileBuffers operation for
    // directory handles. Synced create-new files and the redb intent remain
    // the durable recovery authority around the atomic rename.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Mutex;

    struct NoReferences;

    impl BlobReferenceOracle for NoReferences {
        fn referencing_records(
            &self,
            _cid: &BlobCid,
        ) -> Result<Vec<ObjectReference>, BlobStorageError> {
            Ok(Vec::new())
        }
    }

    #[derive(Default)]
    struct MutableReference {
        cid: Mutex<Option<BlobCid>>,
    }

    impl BlobReferenceOracle for MutableReference {
        fn referencing_records(
            &self,
            cid: &BlobCid,
        ) -> Result<Vec<ObjectReference>, BlobStorageError> {
            if self.cid.lock().unwrap().as_ref() == Some(cid) {
                Ok(vec![ObjectReference::new(0, [0x44; 32])])
            } else {
                Ok(Vec::new())
            }
        }
    }

    struct FixedAvailable(u64);

    impl AvailableSpace for FixedAvailable {
        fn available_bytes(&self, _path: &Path) -> Result<u64, BlobStorageError> {
            Ok(self.0)
        }
    }

    fn temp_db() -> (tempfile::TempDir, BlobStorage) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.blob.redb");
        let storage = BlobStorage::open_internal(
            &db_path,
            BlobStorageConfig {
                total_quota_bytes: DEFAULT_TOTAL_QUOTA_BYTES,
                free_space_reserve_bytes: 1,
            },
            Arc::new(NoReferences),
            Arc::new(FilesystemAvailableSpace),
        )
        .unwrap();
        (dir, storage)
    }

    fn temp_file(dir: &Path, name: &str, content: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content).unwrap();
        path
    }

    #[test]
    fn store_and_retrieve_blob() {
        let (dir, storage) = temp_db();
        let content = b"Hello blob world! This is test content.";
        let file_path = temp_file(dir.path(), "test.txt", content);

        let meta = storage.store_file(&file_path).unwrap();
        assert_eq!(meta.original_name, "test.txt");
        assert_eq!(meta.total_size, content.len() as u64);
        assert_eq!(meta.chunk_count, 1); // < 256KB = 1 chunk
        assert_eq!(meta.blob_type, BlobType::Document as u8);

        // Read back
        let cid = BlobCid::from_hex(&meta.blob_cid_hex).unwrap();
        let data = storage.read_full_blob(&cid).unwrap();
        assert_eq!(data, content);
    }

    #[test]
    fn dedup_identical_files() {
        let (dir, storage) = temp_db();
        let content = b"identical content for dedup test";
        let f1 = temp_file(dir.path(), "a.txt", content);
        let f2 = temp_file(dir.path(), "b.txt", content);

        let meta1 = storage.store_file(&f1).unwrap();
        let meta2 = storage.store_file(&f2).unwrap();

        // Same CID
        assert_eq!(meta1.blob_cid_hex, meta2.blob_cid_hex);
        // Only 1 blob stored
        assert_eq!(storage.blob_count().unwrap(), 1);
    }

    #[test]
    fn multi_chunk_blob() {
        let (dir, storage) = temp_db();
        // Create 600KB file → should be 3 chunks
        let content = vec![0xABu8; 600 * 1024];
        let file_path = temp_file(dir.path(), "large.bin", &content);

        let meta = storage.store_file(&file_path).unwrap();
        assert_eq!(meta.chunk_count, 3); // ceil(600/256) = 3

        // Read back and verify
        let cid = BlobCid::from_hex(&meta.blob_cid_hex).unwrap();
        let data = storage.read_full_blob(&cid).unwrap();
        assert_eq!(data.len(), content.len());
        assert_eq!(data, content);
    }

    #[test]
    fn garbage_collection() {
        let dir = tempfile::tempdir().unwrap();
        let oracle = Arc::new(MutableReference::default());
        let storage = BlobStorage::open_internal(
            &dir.path().join("gc.redb"),
            BlobStorageConfig {
                total_quota_bytes: DEFAULT_TOTAL_QUOTA_BYTES,
                free_space_reserve_bytes: 1,
            },
            oracle.clone(),
            Arc::new(FilesystemAvailableSpace),
        )
        .unwrap();
        let f1 = temp_file(dir.path(), "orphan.txt", b"orphan");
        let f2 = temp_file(dir.path(), "referenced.txt", b"referenced");

        let meta1 = storage.store_file(&f1).unwrap();
        let meta2 = storage.store_file(&f2).unwrap();

        let cid2 = BlobCid::from_hex(&meta2.blob_cid_hex).unwrap();
        *oracle.cid.lock().unwrap() = Some(cid2);

        // GC should delete f1 (orphan) but keep f2 (referenced)
        let (deleted, freed) = storage.garbage_collect().unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(freed, meta1.total_size);
        assert_eq!(storage.blob_count().unwrap(), 1);
    }

    #[test]
    fn legacy_pin_does_not_prevent_canonical_gc() {
        let (dir, storage) = temp_db();
        let f = temp_file(dir.path(), "pinned.txt", b"important");
        let meta = storage.store_file(&f).unwrap();
        let cid = BlobCid::from_hex(&meta.blob_cid_hex).unwrap();

        assert!(matches!(
            storage.set_pinned(&cid, true),
            Err(BlobStorageError::LegacyReferenceReadOnly)
        ));
        let (deleted, _) = storage.garbage_collect().unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(storage.blob_count().unwrap(), 0);
    }

    #[test]
    fn export_blob_to_file() {
        let (dir, storage) = temp_db();
        let content = b"export test content";
        let f = temp_file(dir.path(), "source.txt", content);
        let meta = storage.store_file(&f).unwrap();

        let cid = BlobCid::from_hex(&meta.blob_cid_hex).unwrap();
        let output = dir.path().join("exported.txt");
        let size = storage.export_to_file(&cid, &output).unwrap();

        assert_eq!(size, content.len() as u64);
        assert_eq!(std::fs::read(&output).unwrap(), content);
    }

    #[test]
    fn small_blob_uses_redb_mode() {
        let (dir, storage) = temp_db();
        let content = b"Small blob under 1MB threshold";
        let f = temp_file(dir.path(), "small.txt", content);
        let meta = storage.store_file(&f).unwrap();
        assert_eq!(meta.storage_mode, "redb");

        // Verify chunks are NOT on filesystem
        let cid = BlobCid::from_hex(&meta.blob_cid_hex).unwrap();
        let chunk_dir = storage.blob_chunk_dir(&cid);
        assert!(
            !chunk_dir.exists(),
            "Small blob should not create filesystem chunk dir"
        );
    }

    #[test]
    fn large_blob_uses_filesystem_mode() {
        let (dir, storage) = temp_db();
        // 1.5 MB blob → exceeds 1MB threshold → filesystem mode
        let content = vec![0xCDu8; 1536 * 1024];
        let f = temp_file(dir.path(), "large.bin", &content);
        let meta = storage.store_file(&f).unwrap();

        assert_eq!(meta.storage_mode, "filesystem");
        // ceil(1536KB / 256KB) = 6 chunks
        assert_eq!(meta.chunk_count, 6);

        // Verify chunks exist on filesystem
        let cid = BlobCid::from_hex(&meta.blob_cid_hex).unwrap();
        let chunk_dir = storage.blob_chunk_dir(&cid);
        assert!(
            chunk_dir.exists(),
            "Large blob should create filesystem chunk dir"
        );
        assert!(chunk_dir.join("chunk_0000.bin").exists());
        assert!(chunk_dir.join("chunk_0005.bin").exists());

        // Read back and verify integrity
        let data = storage.read_full_blob(&cid).unwrap();
        assert_eq!(data.len(), content.len());
        assert_eq!(data, content);

        // Delete and verify cleanup
        assert!(storage.delete_blob(&cid).unwrap());
        assert!(
            !chunk_dir.exists(),
            "Chunk dir should be deleted after blob deletion"
        );
        assert_eq!(storage.blob_count().unwrap(), 0);
    }

    #[test]
    fn large_blob_store_bytes() {
        let (_dir, storage) = temp_db();
        // 2 MB blob via store_bytes
        let content = vec![0xABu8; 2 * 1024 * 1024];
        let meta = storage
            .store_bytes("big.dat", &content, BlobType::Raw)
            .unwrap();

        assert_eq!(meta.storage_mode, "filesystem");
        assert_eq!(meta.chunk_count, 8); // ceil(2MB / 256KB) = 8

        let cid = BlobCid::from_hex(&meta.blob_cid_hex).unwrap();
        let data = storage.read_full_blob(&cid).unwrap();
        assert_eq!(data, content);
    }

    #[test]
    fn large_blob_gc_cleans_filesystem() {
        let (dir, storage) = temp_db();
        let content = vec![0xEEu8; 1100 * 1024]; // 1.1 MB
        let f = temp_file(dir.path(), "orphan_large.bin", &content);
        let meta = storage.store_file(&f).unwrap();

        assert_eq!(meta.storage_mode, "filesystem");
        let cid = BlobCid::from_hex(&meta.blob_cid_hex).unwrap();
        let chunk_dir = storage.blob_chunk_dir(&cid);
        assert!(chunk_dir.exists());

        // GC should delete it (orphan, not pinned)
        let (deleted, freed) = storage.garbage_collect().unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(freed, meta.total_size);
        assert!(!chunk_dir.exists(), "GC should clean up filesystem chunks");
    }

    #[test]
    fn filesystem_intent_recovery_completes_stage_activation_delete_and_is_idempotent() {
        let (_dir, storage) = temp_db();
        let content = vec![0xA5; 1100 * 1024];
        let meta = storage
            .store_bytes("recover.bin", &content, BlobType::Raw)
            .unwrap();
        let cid = BlobCid::from_hex(&meta.blob_cid_hex).unwrap();
        let final_dir = storage.blob_chunk_dir(&cid);
        let staging = storage.write_staging_dir(&cid);

        storage
            .begin_filesystem_intent(FilesystemIntent {
                kind: FilesystemIntentKind::Write,
                cid,
                meta: meta.clone(),
            })
            .unwrap();
        std::fs::create_dir_all(staging.parent().unwrap()).unwrap();
        std::fs::rename(&final_dir, &staging).unwrap();
        remove_meta(&storage, &cid);
        assert_eq!(storage.recover_pending_filesystem_intents().unwrap(), 1);
        assert_eq!(storage.read_full_blob(&cid).unwrap(), content);
        assert_eq!(storage.recover_pending_filesystem_intents().unwrap(), 0);

        storage
            .begin_filesystem_intent(FilesystemIntent {
                kind: FilesystemIntentKind::Write,
                cid,
                meta: meta.clone(),
            })
            .unwrap();
        assert_eq!(storage.recover_pending_filesystem_intents().unwrap(), 1);
        assert_eq!(storage.read_full_blob(&cid).unwrap(), content);

        storage
            .begin_filesystem_intent(FilesystemIntent {
                kind: FilesystemIntentKind::Delete,
                cid,
                meta,
            })
            .unwrap();
        let delete_staging = storage.delete_staging_dir(&cid);
        std::fs::rename(&final_dir, &delete_staging).unwrap();
        assert_eq!(storage.recover_pending_filesystem_intents().unwrap(), 1);
        assert!(matches!(
            storage.get_meta(&cid),
            Err(BlobStorageError::NotFound)
        ));
        assert!(!final_dir.exists());
        assert!(!delete_staging.exists());
        assert_eq!(storage.recover_pending_filesystem_intents().unwrap(), 0);
    }

    #[test]
    fn incomplete_filesystem_stage_rolls_back_to_exact_prestate() {
        let (_dir, storage) = temp_db();
        let content = vec![0x5A; 1100 * 1024];
        let meta = storage
            .store_bytes("partial.bin", &content, BlobType::Raw)
            .unwrap();
        let cid = BlobCid::from_hex(&meta.blob_cid_hex).unwrap();
        let final_dir = storage.blob_chunk_dir(&cid);
        let staging = storage.write_staging_dir(&cid);
        storage
            .begin_filesystem_intent(FilesystemIntent {
                kind: FilesystemIntentKind::Write,
                cid,
                meta,
            })
            .unwrap();
        std::fs::create_dir_all(staging.parent().unwrap()).unwrap();
        std::fs::rename(&final_dir, &staging).unwrap();
        std::fs::remove_file(staging.join("chunk_0000.bin")).unwrap();
        remove_meta(&storage, &cid);

        assert_eq!(storage.recover_pending_filesystem_intents().unwrap(), 1);
        assert!(!staging.exists());
        assert!(!final_dir.exists());
        assert!(matches!(
            storage.get_meta(&cid),
            Err(BlobStorageError::NotFound)
        ));
        assert_eq!(storage.recover_pending_filesystem_intents().unwrap(), 0);
    }

    #[test]
    fn oversized_sparse_file_is_rejected_before_chunk_allocation() {
        let (dir, storage) = temp_db();
        let path = dir.path().join("oversized.bin");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(BLOB_MAX_SIZE + 1).unwrap();
        drop(file);

        assert!(matches!(
            storage.store_file(&path),
            Err(BlobStorageError::TooLarge(size)) if size == BLOB_MAX_SIZE + 1
        ));
        assert_eq!(storage.blob_count().unwrap(), 0);
    }

    #[test]
    fn injected_available_space_rejects_before_any_write() {
        let dir = tempfile::tempdir().unwrap();
        let storage = BlobStorage::open_internal(
            &dir.path().join("reserve.redb"),
            BlobStorageConfig {
                total_quota_bytes: 100,
                free_space_reserve_bytes: 5,
            },
            Arc::new(NoReferences),
            Arc::new(FixedAvailable(10)),
        )
        .unwrap();
        assert!(matches!(
            storage.store_bytes("reserve", b"123456", BlobType::Raw),
            Err(BlobStorageError::ReserveSpaceExceeded { .. })
        ));
        assert_eq!(storage.blob_count().unwrap(), 0);
    }

    fn remove_meta(storage: &BlobStorage, cid: &BlobCid) {
        let transaction = storage.db.begin_write().unwrap();
        {
            let mut table = transaction.open_table(TABLE_BLOB_META).unwrap();
            table.remove(cid.0.as_slice()).unwrap();
        }
        transaction.commit().unwrap();
    }
}
