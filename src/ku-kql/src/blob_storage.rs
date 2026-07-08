//! Persistent Blob Storage — redb backend.
//!
//! Stores media/file blobs as chunked data in a separate `.blob.redb` database.
//! Each blob is split into 256KB chunks. Metadata and chunks are stored in
//! separate tables for efficient access.
//!
//! ## Tables
//! - `blob_meta`: OB-CID (34B) → JSON BlobMeta
//! - `blob_chunks`: OB-CID (34B) + index (4B BE) → raw chunk bytes

use std::path::Path;
use ku_core::blob_store::{
    BlobCid, BlobMeta, BlobType, BLOB_CHUNK_SIZE, BLOB_MAX_SIZE,
    BLOB_CID_VERSION, mime_from_extension,
};
use ku_core::obs_schema;
use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};

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
            Self::TooLarge(size) => write!(f, "Blob too large: {} bytes (max: {} bytes)", size, BLOB_MAX_SIZE),
            Self::QuotaExceeded { used, quota } => write!(f, "Blob quota exceeded: {} / {} bytes", used, quota),
            Self::CodecError(msg) => write!(f, "Codec error: {}", msg),
        }
    }
}

impl From<redb::Error> for BlobStorageError {
    fn from(e: redb::Error) -> Self { Self::DatabaseError(format!("{}", e)) }
}
impl From<redb::DatabaseError> for BlobStorageError {
    fn from(e: redb::DatabaseError) -> Self { Self::DatabaseError(format!("{}", e)) }
}
impl From<redb::TransactionError> for BlobStorageError {
    fn from(e: redb::TransactionError) -> Self { Self::DatabaseError(format!("{}", e)) }
}
impl From<redb::TableError> for BlobStorageError {
    fn from(e: redb::TableError) -> Self { Self::DatabaseError(format!("{}", e)) }
}
impl From<redb::StorageError> for BlobStorageError {
    fn from(e: redb::StorageError) -> Self { Self::DatabaseError(format!("{}", e)) }
}
impl From<redb::CommitError> for BlobStorageError {
    fn from(e: redb::CommitError) -> Self { Self::DatabaseError(format!("{}", e)) }
}
impl From<std::io::Error> for BlobStorageError {
    fn from(e: std::io::Error) -> Self { Self::IoError(format!("{}", e)) }
}

/// Persistent blob storage backed by redb.
pub struct BlobStorage {
    db: Database,
}

impl BlobStorage {
    /// Open or create blob storage at the given path.
    pub fn open(path: &Path) -> Result<Self, BlobStorageError> {
        let db = Database::create(path)
            .map_err(|e| BlobStorageError::DatabaseError(format!("{}", e)))?;
        
        // Ensure tables exist
        {
            let txn = db.begin_write()?;
            { let _ = txn.open_table(TABLE_BLOB_META)?; }
            { let _ = txn.open_table(TABLE_BLOB_CHUNKS)?; }
            txn.commit()?;
        }
        
        // Schema versioning
        obs_schema::redb_schema::ensure_schema(&db, &obs_schema::blob_store_registry())
            .map_err(|e| BlobStorageError::DatabaseError(format!("Schema error: {}", e)))?;
        
        Ok(Self { db })
    }

    /// Store a file as a blob. Returns metadata.
    ///
    /// 1. Reads entire file into memory
    /// 2. Computes BLAKE3 hash → BlobCid
    /// 3. Checks for dedup (if CID exists, just add reference)
    /// 4. Chunks file into 256KB pieces
    /// 5. Stores meta + chunks in a single transaction
    pub fn store_file(&self, file_path: &Path) -> Result<BlobMeta, BlobStorageError> {
        // Read file
        let data = std::fs::read(file_path)?;
        let total_size = data.len() as u64;
        
        // Check size limit
        if total_size > BLOB_MAX_SIZE {
            return Err(BlobStorageError::TooLarge(total_size));
        }
        
        // Detect type
        let ext = file_path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let blob_type = BlobType::detect(Some(ext), &data[..data.len().min(12)]);
        let mime_type = mime_from_extension(ext).to_string();
        
        // Compute CID
        let blob_cid = BlobCid::from_content(blob_type, &data);
        
        // Check dedup
        if self.has_blob(&blob_cid)? {
            // Already exists — return existing meta
            return self.get_meta(&blob_cid);
        }
        
        // Chunk the file
        let chunk_count = ((total_size as usize + BLOB_CHUNK_SIZE - 1) / BLOB_CHUNK_SIZE) as u32;
        let original_name = file_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        
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
            blake3_hex: blob_cid.blake3_hash().iter().map(|b| format!("{:02x}", b)).collect(),
            referencing_kus: vec![],
            pinned: false,
        };
        
        // Write meta + chunks in single transaction
        let txn = self.db.begin_write()?;
        {
            // Write meta
            let meta_json = serde_json::to_vec(&meta)
                .map_err(|e| BlobStorageError::CodecError(format!("{}", e)))?;
            let mut table = txn.open_table(TABLE_BLOB_META)?;
            table.insert(blob_cid.0.as_slice(), meta_json.as_slice())?;
        }
        {
            // Write chunks
            let mut table = txn.open_table(TABLE_BLOB_CHUNKS)?;
            for i in 0..chunk_count {
                let start = (i as usize) * BLOB_CHUNK_SIZE;
                let end = ((i as usize + 1) * BLOB_CHUNK_SIZE).min(data.len());
                let chunk_data = &data[start..end];
                
                // Key: [blob_cid:34B][index:4B BE]
                let mut key = Vec::with_capacity(38);
                key.extend_from_slice(&blob_cid.0);
                key.extend_from_slice(&i.to_be_bytes());
                
                table.insert(key.as_slice(), chunk_data)?;
            }
        }
        txn.commit()?;
        
        Ok(meta)
    }

    /// Store raw bytes as a blob (for programmatic use).
    pub fn store_bytes(&self, name: &str, data: &[u8], blob_type: BlobType) -> Result<BlobMeta, BlobStorageError> {
        let total_size = data.len() as u64;
        if total_size > BLOB_MAX_SIZE {
            return Err(BlobStorageError::TooLarge(total_size));
        }
        
        let blob_cid = BlobCid::from_content(blob_type, data);
        
        if self.has_blob(&blob_cid)? {
            return self.get_meta(&blob_cid);
        }
        
        let chunk_count = ((data.len() + BLOB_CHUNK_SIZE - 1) / BLOB_CHUNK_SIZE) as u32;
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
            blake3_hex: blob_cid.blake3_hash().iter().map(|b| format!("{:02x}", b)).collect(),
            referencing_kus: vec![],
            pinned: false,
        };
        
        let txn = self.db.begin_write()?;
        {
            let meta_json = serde_json::to_vec(&meta)
                .map_err(|e| BlobStorageError::CodecError(format!("{}", e)))?;
            let mut table = txn.open_table(TABLE_BLOB_META)?;
            table.insert(blob_cid.0.as_slice(), meta_json.as_slice())?;
        }
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
        
        Ok(meta)
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
        let guard = table.get(blob_cid.0.as_slice())?
            .ok_or(BlobStorageError::NotFound)?;
        let meta: BlobMeta = serde_json::from_slice(guard.value())
            .map_err(|e| BlobStorageError::CodecError(format!("{}", e)))?;
        Ok(meta)
    }

    /// Read a single chunk.
    pub fn get_chunk(&self, blob_cid: &BlobCid, index: u32) -> Result<Vec<u8>, BlobStorageError> {
        let mut key = Vec::with_capacity(38);
        key.extend_from_slice(&blob_cid.0);
        key.extend_from_slice(&index.to_be_bytes());
        
        let txn = self.db.begin_read()?;
        let table = txn.open_table(TABLE_BLOB_CHUNKS)?;
        let guard = table.get(key.as_slice())?
            .ok_or(BlobStorageError::NotFound)?;
        Ok(guard.value().to_vec())
    }

    /// Reassemble and return the full blob data.
    pub fn read_full_blob(&self, blob_cid: &BlobCid) -> Result<Vec<u8>, BlobStorageError> {
        let meta = self.get_meta(blob_cid)?;
        let mut result = Vec::with_capacity(meta.total_size as usize);
        
        for i in 0..meta.chunk_count {
            let chunk = self.get_chunk(blob_cid, i)?;
            result.extend_from_slice(&chunk);
        }
        
        Ok(result)
    }

    /// Export a blob to a file.
    pub fn export_to_file(&self, blob_cid: &BlobCid, output_path: &Path) -> Result<u64, BlobStorageError> {
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
            let (_, value) = entry.map_err(|e| BlobStorageError::DatabaseError(format!("{}", e)))?;
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
    pub fn add_ku_reference(&self, blob_cid: &BlobCid, ku_cid_hex: &str) -> Result<(), BlobStorageError> {
        let mut meta = self.get_meta(blob_cid)?;
        if !meta.referencing_kus.contains(&ku_cid_hex.to_string()) {
            meta.referencing_kus.push(ku_cid_hex.to_string());
            self.update_meta(blob_cid, &meta)?;
        }
        Ok(())
    }

    /// Remove a KU reference from a blob.
    pub fn remove_ku_reference(&self, blob_cid: &BlobCid, ku_cid_hex: &str) -> Result<(), BlobStorageError> {
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
        
        let txn = self.db.begin_write()?;
        {
            // Delete chunks
            let mut table = txn.open_table(TABLE_BLOB_CHUNKS)?;
            for i in 0..meta.chunk_count {
                let mut key = Vec::with_capacity(38);
                key.extend_from_slice(&blob_cid.0);
                key.extend_from_slice(&i.to_be_bytes());
                let _ = table.remove(key.as_slice())?;
            }
        }
        {
            // Delete meta
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
        let orphans: Vec<_> = blobs.iter()
            .filter(|b| b.is_orphaned())
            .collect();
        
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
        let meta_json = serde_json::to_vec(meta)
            .map_err(|e| BlobStorageError::CodecError(format!("{}", e)))?;
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
}
