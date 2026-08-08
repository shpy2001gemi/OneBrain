#![cfg(feature = "storage")]

use std::sync::Arc;

use ku_core::blob_store::{BlobCid, BlobType, BLOB_CHUNK_SIZE};
use ku_core::foundation::ObjectReference;
use ku_kql::blob_layout::blob_relative_dir;
use ku_kql::blob_storage::{
    BlobReferenceOracle, BlobStorage, BlobStorageConfig, BlobStorageError, BLOB_META_VERSION,
};
use redb::{Database, TableDefinition};

const CHUNKS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("blob_chunks");
const META: TableDefinition<&[u8], &[u8]> = TableDefinition::new("blob_meta");

struct NoReferences;

impl BlobReferenceOracle for NoReferences {
    fn referencing_records(
        &self,
        _cid: &BlobCid,
    ) -> Result<Vec<ObjectReference>, BlobStorageError> {
        Ok(Vec::new())
    }
}

#[test]
fn legacy_inline_metadata_requires_then_completes_atomic_v2_migration() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("legacy.redb");
    let data = b"legacy inline metadata";
    let cid = BlobCid::from_content(BlobType::Raw, data);
    write_legacy_inline(&path, cid, data);

    let storage = open(&path);
    assert!(matches!(
        storage.read_full_blob(&cid),
        Err(BlobStorageError::MigrationRequired { .. })
    ));
    let report = storage.migrate_blob_metadata_v2().unwrap();
    assert_eq!(report.migrated, 1);
    assert_eq!(report.already_v2, 0);

    let meta = storage.get_meta(&cid).unwrap();
    assert_eq!(meta.meta_version, BLOB_META_VERSION);
    assert_eq!(meta.chunk_blake3.len(), 1);
    assert_eq!(storage.read_full_blob(&cid).unwrap(), data);

    let rerun = storage.migrate_blob_metadata_v2().unwrap();
    assert_eq!(rerun.migrated, 0);
    assert_eq!(rerun.already_v2, 1);
}

#[test]
fn corrupt_legacy_metadata_migration_preserves_original_record_and_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("corrupt.redb");
    let data = b"expected legacy bytes";
    let cid = BlobCid::from_content(BlobType::Raw, data);
    write_legacy_inline(&path, cid, b"different legacy bytes");

    let storage = open(&path);
    let before = storage.get_meta(&cid).unwrap();
    assert_eq!(before.meta_version, 0);
    let error = storage.migrate_blob_metadata_v2().unwrap_err();
    assert!(matches!(error, BlobStorageError::MigrationBlocked(_)));
    let after = storage.get_meta(&cid).unwrap();
    assert_eq!(after.meta_version, 0);
    assert!(after.chunk_blake3.is_empty());
}

#[test]
fn legacy_spilled_metadata_is_reassembled_and_upgraded_atomically() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("legacy-spilled.redb");
    let data = vec![0xC3; 1100 * 1024];
    let cid = BlobCid::from_content(BlobType::Raw, &data);
    write_legacy_spilled(&path, cid, &data);

    let storage = open(&path);
    assert!(matches!(
        storage.read_full_blob(&cid),
        Err(BlobStorageError::MigrationRequired { .. })
    ));
    let report = storage.migrate_blob_metadata_v2().unwrap();
    assert_eq!(report.migrated, 1);
    let meta = storage.get_meta(&cid).unwrap();
    assert_eq!(meta.meta_version, BLOB_META_VERSION);
    assert_eq!(meta.chunk_blake3.len() as u32, meta.chunk_count);
    assert_eq!(storage.read_full_blob(&cid).unwrap(), data);
}

fn open(path: &std::path::Path) -> BlobStorage {
    BlobStorage::open_with_config(
        path,
        BlobStorageConfig {
            total_quota_bytes: 1024 * 1024,
            free_space_reserve_bytes: 1,
        },
        Arc::new(NoReferences),
    )
    .unwrap()
}

fn write_legacy_inline(path: &std::path::Path, cid: BlobCid, data: &[u8]) {
    let database = Database::create(path).unwrap();
    let transaction = database.begin_write().unwrap();
    {
        let mut meta_table = transaction.open_table(META).unwrap();
        let mut chunks = transaction.open_table(CHUNKS).unwrap();
        let legacy = serde_json::json!({
            "blob_cid_hex": cid.to_hex(),
            "original_name": "legacy.bin",
            "mime_type": "application/octet-stream",
            "total_size": data.len(),
            "chunk_count": 1,
            "chunk_size": BLOB_CHUNK_SIZE,
            "blob_type": cid.0[1],
            "created_at": 0,
            "blake3_hex": cid.blake3_hash().iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "referencing_kus": [],
            "pinned": false,
            "storage_mode": "redb"
        });
        let encoded = serde_json::to_vec(&legacy).unwrap();
        meta_table
            .insert(cid.0.as_slice(), encoded.as_slice())
            .unwrap();
        let mut chunk_key = cid.0.to_vec();
        chunk_key.extend_from_slice(&0u32.to_be_bytes());
        chunks.insert(chunk_key.as_slice(), data).unwrap();
    }
    transaction.commit().unwrap();
}

fn write_legacy_spilled(path: &std::path::Path, cid: BlobCid, data: &[u8]) {
    let database = Database::create(path).unwrap();
    let chunk_count = data.len().div_ceil(BLOB_CHUNK_SIZE) as u32;
    let transaction = database.begin_write().unwrap();
    {
        let mut meta_table = transaction.open_table(META).unwrap();
        let legacy = serde_json::json!({
            "blob_cid_hex": cid.to_hex(),
            "original_name": "legacy-spilled.bin",
            "mime_type": "application/octet-stream",
            "total_size": data.len(),
            "chunk_count": chunk_count,
            "chunk_size": BLOB_CHUNK_SIZE,
            "blob_type": cid.0[1],
            "created_at": 0,
            "blake3_hex": cid.blake3_hash().iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "referencing_kus": [],
            "pinned": false,
            "storage_mode": "filesystem"
        });
        let encoded = serde_json::to_vec(&legacy).unwrap();
        meta_table
            .insert(cid.0.as_slice(), encoded.as_slice())
            .unwrap();
    }
    transaction.commit().unwrap();
    drop(database);

    let directory = path
        .parent()
        .unwrap()
        .join("blobs")
        .join(blob_relative_dir(&cid));
    std::fs::create_dir_all(&directory).unwrap();
    for (index, chunk) in data.chunks(BLOB_CHUNK_SIZE).enumerate() {
        std::fs::write(directory.join(format!("chunk_{index:04}.bin")), chunk).unwrap();
    }
}
