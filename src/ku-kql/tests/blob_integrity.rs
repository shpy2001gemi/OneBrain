#![cfg(feature = "storage")]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use ku_core::blob_store::{BlobCid, BlobType, BLOB_MAX_PER_KU};
use ku_core::foundation::ObjectReference;
use ku_kql::blob_layout::blob_relative_dir;
use ku_kql::blob_storage::{
    BlobReadError, BlobReferenceOracle, BlobStorage, BlobStorageConfig, BlobStorageError,
};
use redb::{Database, ReadableTable, TableDefinition};

const CHUNKS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("blob_chunks");
const META: TableDefinition<&[u8], &[u8]> = TableDefinition::new("blob_meta");

#[derive(Default)]
struct TestOracle {
    references: Mutex<BTreeMap<String, Vec<ObjectReference>>>,
    dirty: Mutex<bool>,
}

impl TestOracle {
    fn set(&self, cid: BlobCid, references: Vec<ObjectReference>) {
        self.references
            .lock()
            .unwrap()
            .insert(cid.to_hex(), references);
    }

    fn set_dirty(&self) {
        *self.dirty.lock().unwrap() = true;
    }
}

impl BlobReferenceOracle for TestOracle {
    fn referencing_records(&self, cid: &BlobCid) -> Result<Vec<ObjectReference>, BlobStorageError> {
        if *self.dirty.lock().unwrap() {
            return Err(BlobStorageError::ReferenceParityUnknown);
        }
        Ok(self
            .references
            .lock()
            .unwrap()
            .get(&cid.to_hex())
            .cloned()
            .unwrap_or_default())
    }
}

#[test]
fn cid_parser_requires_exact_lowercase_68_characters() {
    let cid = BlobCid::from_content(BlobType::Raw, b"cid");
    let canonical = cid.to_hex();
    assert_eq!(BlobCid::from_hex(&canonical), Some(cid));
    assert!(BlobCid::from_hex(&canonical[..67]).is_none());
    assert!(BlobCid::from_hex(&(canonical.clone() + "00")).is_none());
    assert!(BlobCid::from_hex(&canonical.to_uppercase()).is_none());
    let mut wrong_version = canonical.clone();
    wrong_version.replace_range(..2, "02");
    assert!(BlobCid::from_hex(&wrong_version).is_none());
    let mut unknown_type = canonical;
    unknown_type.replace_range(2..4, "ff");
    assert!(BlobCid::from_hex(&unknown_type).is_none());
}

#[test]
fn corrupt_inline_chunk_is_rejected_before_bytes_escape() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("inline.redb");
    let oracle = Arc::new(TestOracle::default());
    let meta = {
        let storage = open(&db_path, oracle.clone(), 1024);
        storage
            .store_bytes("inline", b"verified inline", BlobType::Raw)
            .unwrap()
    };
    let cid = BlobCid::from_hex(&meta.blob_cid_hex).unwrap();
    replace_inline_chunk(&db_path, cid, 0, b"tampered inline");

    let storage = open(&db_path, oracle, 1024);
    assert!(matches!(
        storage.get_chunk(&cid, 0),
        Err(BlobStorageError::Read(BlobReadError::ChunkDigestMismatch {
            index: 0
        }))
    ));
    assert!(matches!(
        storage.read_full_blob(&cid),
        Err(BlobStorageError::Read(_))
    ));
}

#[test]
fn prepared_file_binding_rejects_wrong_cid_before_any_write() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("bound.redb");
    let file_path = temp.path().join("upload.bin");
    std::fs::write(&file_path, b"actual bytes").unwrap();
    let expected = BlobCid::from_content(BlobType::Raw, b"different bytes");
    let storage = open(&db_path, Arc::new(TestOracle::default()), 1024);

    assert!(matches!(
        storage.store_file_bound(
            &file_path,
            &expected,
            BlobType::Raw,
            b"actual bytes".len() as u64,
        ),
        Err(BlobStorageError::Read(BlobReadError::ContentDigestMismatch))
    ));
    assert_eq!(storage.blob_count().unwrap(), 0);
}

#[test]
fn corrupt_spilled_chunk_and_length_changes_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("spilled.redb");
    let oracle = Arc::new(TestOracle::default());
    let data = vec![0x5a; 1024 * 1024 + 9];
    let storage = open(&db_path, oracle, 4 * 1024 * 1024);
    let meta = storage
        .store_bytes("spilled", &data, BlobType::Raw)
        .unwrap();
    let cid = BlobCid::from_hex(&meta.blob_cid_hex).unwrap();
    let chunk = temp
        .path()
        .join("blobs")
        .join(blob_relative_dir(&cid))
        .join("chunk_0000.bin");

    std::fs::write(&chunk, vec![0x44; meta.chunk_size as usize]).unwrap();
    assert!(matches!(
        storage.read_full_blob(&cid),
        Err(BlobStorageError::Read(BlobReadError::ChunkDigestMismatch {
            index: 0
        }))
    ));

    std::fs::write(&chunk, vec![0x5a; meta.chunk_size as usize - 1]).unwrap();
    assert!(matches!(
        storage.read_full_blob(&cid),
        Err(BlobStorageError::Read(BlobReadError::LengthMismatch))
            | Err(BlobStorageError::Read(BlobReadError::ChunkDigestMismatch {
                index: 0
            }))
    ));
}

#[test]
fn declared_type_mismatch_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("type.redb");
    let oracle = Arc::new(TestOracle::default());
    let meta = {
        let storage = open(&db_path, oracle.clone(), 1024);
        storage
            .store_bytes("typed", b"typed", BlobType::Raw)
            .unwrap()
    };
    let cid = BlobCid::from_hex(&meta.blob_cid_hex).unwrap();
    rewrite_meta(&db_path, cid, |value| {
        value["blob_type"] = serde_json::json!(BlobType::Document as u8);
    });
    let storage = open(&db_path, oracle, 1024);
    assert!(matches!(
        storage.read_full_blob(&cid),
        Err(BlobStorageError::Read(BlobReadError::TypeMismatch))
    ));
}

#[test]
fn quota_counts_unique_bytes_and_dedup_costs_zero() {
    let temp = tempfile::tempdir().unwrap();
    let oracle = Arc::new(TestOracle::default());
    let storage = open(&temp.path().join("quota.redb"), oracle, 8);
    storage
        .store_bytes("first", b"12345", BlobType::Raw)
        .unwrap();
    storage
        .store_bytes("dedup", b"12345", BlobType::Raw)
        .unwrap();
    assert!(matches!(
        storage.store_bytes("second", b"67890", BlobType::Raw),
        Err(BlobStorageError::QuotaExceeded { .. })
    ));
    assert_eq!(storage.total_blob_size().unwrap(), 5);
}

#[test]
fn gc_fails_closed_on_dirty_parity_and_per_owner_overflow() {
    let temp = tempfile::tempdir().unwrap();
    let oracle = Arc::new(TestOracle::default());
    let storage = open(&temp.path().join("refs.redb"), oracle.clone(), 4096);
    let owner = ObjectReference::new(0, [0x33; 32]);
    for marker in 0..=BLOB_MAX_PER_KU {
        let meta = storage
            .store_bytes("owned", &[marker as u8], BlobType::Raw)
            .unwrap();
        oracle.set(
            BlobCid::from_hex(&meta.blob_cid_hex).unwrap(),
            vec![owner.clone()],
        );
    }
    assert!(matches!(
        storage.garbage_collect(),
        Err(BlobStorageError::OwnerBlobLimitExceeded { .. })
    ));

    oracle.set_dirty();
    assert!(matches!(
        storage.garbage_collect(),
        Err(BlobStorageError::ReferenceParityUnknown)
    ));
}

#[test]
fn legacy_pin_and_ku_reference_mutations_are_read_only() {
    let temp = tempfile::tempdir().unwrap();
    let storage = open(
        &temp.path().join("legacy.redb"),
        Arc::new(TestOracle::default()),
        1024,
    );
    let meta = storage
        .store_bytes("legacy", b"legacy", BlobType::Raw)
        .unwrap();
    let cid = BlobCid::from_hex(&meta.blob_cid_hex).unwrap();
    assert!(matches!(
        storage.add_ku_reference(&cid, "legacy-ku"),
        Err(BlobStorageError::LegacyReferenceReadOnly)
    ));
    assert!(matches!(
        storage.remove_ku_reference(&cid, "legacy-ku"),
        Err(BlobStorageError::LegacyReferenceReadOnly)
    ));
    assert!(matches!(
        storage.set_pinned(&cid, true),
        Err(BlobStorageError::LegacyReferenceReadOnly)
    ));
}

fn open(path: &std::path::Path, oracle: Arc<TestOracle>, quota: u64) -> BlobStorage {
    BlobStorage::open_with_config(
        path,
        BlobStorageConfig {
            total_quota_bytes: quota,
            free_space_reserve_bytes: 1,
        },
        oracle,
    )
    .unwrap()
}

fn replace_inline_chunk(path: &std::path::Path, cid: BlobCid, index: u32, bytes: &[u8]) {
    let database = Database::create(path).unwrap();
    let transaction = database.begin_write().unwrap();
    {
        let mut table = transaction.open_table(CHUNKS).unwrap();
        let mut key = cid.0.to_vec();
        key.extend_from_slice(&index.to_be_bytes());
        table.insert(key.as_slice(), bytes).unwrap();
    }
    transaction.commit().unwrap();
}

fn rewrite_meta(path: &std::path::Path, cid: BlobCid, mutate: impl FnOnce(&mut serde_json::Value)) {
    let database = Database::create(path).unwrap();
    let transaction = database.begin_write().unwrap();
    {
        let mut table = transaction.open_table(META).unwrap();
        let current = table.get(cid.0.as_slice()).unwrap().unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(current.value()).unwrap();
        drop(current);
        mutate(&mut value);
        let encoded = serde_json::to_vec(&value).unwrap();
        table.insert(cid.0.as_slice(), encoded.as_slice()).unwrap();
    }
    transaction.commit().unwrap();
}
