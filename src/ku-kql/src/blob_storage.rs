//! # Persistent Blob Storage — hybrid redb + filesystem (v7)
//!
//! Stores media/file blobs using a size-based hybrid strategy:
//! - **Small blobs (≤ 1 MB)**: chunks stored inline in redb (`blob_chunks` table)
//! - **Large blobs (> 1 MB)**: chunks stored as files on the filesystem under
//!   `<db_dir>/blobs/<cid_hex>/chunk_NNNN.bin`; only metadata stays in redb
//!
//! This avoids bloating the redb database with large media files while keeping
//! small blobs fast and transactional.
//!
//! ## Tables
//! - `blob_meta`: OB-CID (34B) → JSON BlobMeta (always in redb)
//! - `blob_chunks`: OB-CID (34B) + index (4B BE) → raw chunk bytes (small blobs only)

use ku_core::blob_store::{
    mime_from_extension, BlobCid, BlobMeta, BlobType, BLOB_CHUNK_SIZE, BLOB_MAX_SIZE,
};
use ku_core::obs_schema;
use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
use std::path::{Path, PathBuf};

/// Threshold: blobs larger than this use filesystem storage for chunks.
const FILESYSTEM_SPILL_THRESHOLD: u64 = 1024 * 1024; // 1 MB

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

/// Blob storage error.
#[derive(Debug)]
pub enum BlobStorageError {
    DatabaseError(String),
    IoError(String),
    NotFound,
    TooLarge(u64),
    QuotaExceeded { used: u64, quota: u64 },
    CodecError(String),
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
}

impl BlobStorage {
    /// Open or create blob storage at the given path.
    ///
    /// Creates a `blobs/` directory next to the database file for filesystem
    /// chunk storage of large blobs.
    pub fn open(path: &Path) -> Result<Self, BlobStorageError> {
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

        Ok(Self { db, chunk_dir })
    }

    /// Directory for a specific blob's filesystem chunks.
    fn blob_chunk_dir(&self, blob_cid: &BlobCid) -> PathBuf {
        self.chunk_dir.join(blob_cid.short_hex())
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
        // Read file
        let data = std::fs::read(file_path)?;
        let total_size = data.len() as u64;

        // Check size limit
        if total_size > BLOB_MAX_SIZE {
            return Err(BlobStorageError::TooLarge(total_size));
        }

        // Detect type
        let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let blob_type = BlobType::detect(Some(ext), &data[..data.len().min(12)]);
        let mime_type = mime_from_extension(ext).to_string();

        // Compute CID
        let blob_cid = BlobCid::from_content(blob_type, &data);

        // Check dedup
        if self.has_blob(&blob_cid)? {
            return self.get_meta(&blob_cid);
        }

        let chunk_count = ((total_size as usize + BLOB_CHUNK_SIZE - 1) / BLOB_CHUNK_SIZE) as u32;
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
        };

        // Write chunks according to storage mode
        self.write_chunks(&blob_cid, &data, chunk_count, mode)?;

        // Write meta in redb (always)
        let txn = self.db.begin_write()?;
        {
            let meta_json = serde_json::to_vec(&meta)
                .map_err(|e| BlobStorageError::CodecError(format!("{}", e)))?;
            let mut table = txn.open_table(TABLE_BLOB_META)?;
            table.insert(blob_cid.0.as_slice(), meta_json.as_slice())?;
        }
        txn.commit()?;

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

        let chunk_count = ((data.len() + BLOB_CHUNK_SIZE - 1) / BLOB_CHUNK_SIZE) as u32;
        let mode = Self::storage_mode_for(total_size);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let meta = BlobMeta {
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
        };

        self.write_chunks(&blob_cid, data, chunk_count, mode)?;

        let txn = self.db.begin_write()?;
        {
            let meta_json = serde_json::to_vec(&meta)
                .map_err(|e| BlobStorageError::CodecError(format!("{}", e)))?;
            let mut table = txn.open_table(TABLE_BLOB_META)?;
            table.insert(blob_cid.0.as_slice(), meta_json.as_slice())?;
        }
        txn.commit()?;

        Ok(meta)
    }

    // ── Internal: chunk write/read/delete by storage mode ──────────────

    /// Write chunks to redb or filesystem depending on mode.
    fn write_chunks(
        &self,
        blob_cid: &BlobCid,
        data: &[u8],
        chunk_count: u32,
        mode: StorageMode,
    ) -> Result<(), BlobStorageError> {
        match mode {
            StorageMode::Redb => {
                let txn = self.db.begin_write()?;
                {
                    let mut table = txn.open_table(TABLE_BLOB_CHUNKS)?;
                    for i in 0..chunk_count {
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
                let dir = self.blob_chunk_dir(blob_cid);
                std::fs::create_dir_all(&dir)?;
                for i in 0..chunk_count {
                    let start = (i as usize) * BLOB_CHUNK_SIZE;
                    let end = ((i as usize + 1) * BLOB_CHUNK_SIZE).min(data.len());
                    let chunk_path = dir.join(format!("chunk_{:04}.bin", i));
                    std::fs::write(&chunk_path, &data[start..end])?;
                }
            }
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
        let mode = StorageMode::from_str(&meta.storage_mode);
        self.read_chunk_internal(blob_cid, index, mode)
    }

    /// Reassemble and return the full blob data.
    pub fn read_full_blob(&self, blob_cid: &BlobCid) -> Result<Vec<u8>, BlobStorageError> {
        let meta = self.get_meta(blob_cid)?;
        let mode = StorageMode::from_str(&meta.storage_mode);
        let mut result = Vec::with_capacity(meta.total_size as usize);

        for i in 0..meta.chunk_count {
            let chunk = self.read_chunk_internal(blob_cid, i, mode)?;
            result.extend_from_slice(&chunk);
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
            if let Ok(meta) = serde_json::from_slice::<BlobMeta>(value.value()) {
                blobs.push(meta);
            }
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
        Ok(blobs.iter().map(|b| b.total_size).sum())
    }

    /// Add a KU reference to a blob.
    pub fn add_ku_reference(
        &self,
        blob_cid: &BlobCid,
        ku_cid_hex: &str,
    ) -> Result<(), BlobStorageError> {
        let mut meta = self.get_meta(blob_cid)?;
        if !meta.referencing_kus.contains(&ku_cid_hex.to_string()) {
            meta.referencing_kus.push(ku_cid_hex.to_string());
            self.update_meta(blob_cid, &meta)?;
        }
        Ok(())
    }

    /// Remove a KU reference from a blob.
    pub fn remove_ku_reference(
        &self,
        blob_cid: &BlobCid,
        ku_cid_hex: &str,
    ) -> Result<(), BlobStorageError> {
        let mut meta = self.get_meta(blob_cid)?;
        meta.referencing_kus.retain(|k| k != ku_cid_hex);
        self.update_meta(blob_cid, &meta)?;
        Ok(())
    }

    /// Pin/unpin a blob.
    pub fn set_pinned(&self, blob_cid: &BlobCid, pinned: bool) -> Result<(), BlobStorageError> {
        let mut meta = self.get_meta(blob_cid)?;
        meta.pinned = pinned;
        self.update_meta(blob_cid, &meta)?;
        Ok(())
    }

    /// Delete a blob and all its chunks.
    pub fn delete_blob(&self, blob_cid: &BlobCid) -> Result<bool, BlobStorageError> {
        let meta = match self.get_meta(blob_cid) {
            Ok(m) => m,
            Err(BlobStorageError::NotFound) => return Ok(false),
            Err(e) => return Err(e),
        };

        // Delete chunks from the appropriate backend
        let mode = StorageMode::from_str(&meta.storage_mode);
        self.delete_chunks_internal(blob_cid, meta.chunk_count, mode)?;

        // Delete meta from redb
        let txn = self.db.begin_write()?;
        {
            let mut table = txn.open_table(TABLE_BLOB_META)?;
            let _ = table.remove(blob_cid.0.as_slice())?;
        }
        txn.commit()?;

        Ok(true)
    }

    /// Garbage collect orphaned blobs (zero references, not pinned).
    /// Returns number of blobs deleted and bytes freed.
    pub fn garbage_collect(&self) -> Result<(usize, u64), BlobStorageError> {
        let blobs = self.list_blobs()?;
        let orphans: Vec<_> = blobs.iter().filter(|b| b.is_orphaned()).collect();

        let mut deleted = 0;
        let mut freed = 0u64;

        for orphan in &orphans {
            if let Some(cid) = BlobCid::from_hex(&orphan.blob_cid_hex) {
                if self.delete_blob(&cid)? {
                    deleted += 1;
                    freed += orphan.total_size;
                }
            }
        }

        Ok((deleted, freed))
    }

    // Internal: update metadata
    fn update_meta(&self, blob_cid: &BlobCid, meta: &BlobMeta) -> Result<(), BlobStorageError> {
        let meta_json =
            serde_json::to_vec(meta).map_err(|e| BlobStorageError::CodecError(format!("{}", e)))?;
        let txn = self.db.begin_write()?;
        {
            let mut table = txn.open_table(TABLE_BLOB_META)?;
            table.insert(blob_cid.0.as_slice(), meta_json.as_slice())?;
        }
        txn.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_db() -> (tempfile::TempDir, BlobStorage) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.blob.redb");
        let storage = BlobStorage::open(&db_path).unwrap();
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
        let (dir, storage) = temp_db();
        let f1 = temp_file(dir.path(), "orphan.txt", b"orphan");
        let f2 = temp_file(dir.path(), "referenced.txt", b"referenced");

        let meta1 = storage.store_file(&f1).unwrap();
        let meta2 = storage.store_file(&f2).unwrap();

        // Add reference to f2
        let cid2 = BlobCid::from_hex(&meta2.blob_cid_hex).unwrap();
        storage.add_ku_reference(&cid2, "deadbeef").unwrap();

        // GC should delete f1 (orphan) but keep f2 (referenced)
        let (deleted, freed) = storage.garbage_collect().unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(freed, meta1.total_size);
        assert_eq!(storage.blob_count().unwrap(), 1);
    }

    #[test]
    fn pin_prevents_gc() {
        let (dir, storage) = temp_db();
        let f = temp_file(dir.path(), "pinned.txt", b"important");
        let meta = storage.store_file(&f).unwrap();
        let cid = BlobCid::from_hex(&meta.blob_cid_hex).unwrap();

        // Pin it
        storage.set_pinned(&cid, true).unwrap();

        // GC should NOT delete it despite zero references
        let (deleted, _) = storage.garbage_collect().unwrap();
        assert_eq!(deleted, 0);
        assert_eq!(storage.blob_count().unwrap(), 1);
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
}
