//! DHT Persistence — redb-backed storage for DHT entries and replica metadata.
//!
//! Feature-gated behind `persist` feature. Persists DHT entries and
//! ReplicaTracker state so data survives node restarts.
//!
//! ## Tables
//! - `dht_entries`: `[u8;32]` → CBOR(`DhtEntryRecord`)
//! - `replica_meta`: `[u8;32]` → CBOR(`StoredKuMetaRecord`)
//! - `_schema_meta`: schema versioning (via `ku_core::obs_schema`)

use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
use serde::{Deserialize, Serialize};

// ─── Table Definitions ──────────────────────────────────────────────────────

/// DHT entries table: CID → CBOR(DhtEntryRecord).
const DHT_ENTRIES: TableDefinition<&[u8; 32], &[u8]> = TableDefinition::new("dht_entries");

/// Replica metadata table: CID → CBOR(StoredKuMetaRecord).
const REPLICA_META: TableDefinition<&[u8; 32], &[u8]> = TableDefinition::new("replica_meta");

// ─── Record Types ───────────────────────────────────────────────────────────

/// Serializable DHT entry for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhtEntryRecord {
    /// The stored value bytes.
    pub value: Vec<u8>,
    /// When this entry was stored (epoch seconds).
    pub stored_at: u64,
    /// Optional TTL in seconds (None = permanent).
    pub ttl_secs: Option<u64>,
}

/// Serializable replica metadata for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredKuMetaRecord {
    /// Number of known replicas.
    pub actual_replicas: u32,
    /// First epoch this KU was stored.
    pub first_stored_epoch: u64,
    /// Total epochs stored.
    pub epochs_stored: u64,
}

// ─── DhtPersistence ─────────────────────────────────────────────────────────

/// Persistent storage backend for DHT entries and replica tracking metadata.
///
/// Uses redb for embedded ACID transactions. Schema versioning is managed
/// via `ku_core::obs_schema`.
pub struct DhtPersistence {
    db: Database,
}

impl DhtPersistence {
    /// Open or create the DHT persistence database at the given path.
    ///
    /// Initializes schema versioning on first open.
    pub fn open(path: &std::path::Path) -> Result<Self, String> {
        let db = Database::create(path).map_err(|e| format!("redb create: {}", e))?;

        // Initialize schema via ku_core::obs_schema
        let registry = ku_core::obs_schema::dht_store_registry();
        ku_core::obs_schema::redb_schema::ensure_schema(&db, &registry)?;

        // Create tables proactively so reads never hit TableDoesNotExist
        let txn = db.begin_write().map_err(|e| format!("begin_write: {}", e))?;
        {
            let _ = txn.open_table(DHT_ENTRIES)
                .map_err(|e| format!("init dht_entries: {}", e))?;
            let _ = txn.open_table(REPLICA_META)
                .map_err(|e| format!("init replica_meta: {}", e))?;
        }
        txn.commit().map_err(|e| format!("commit: {}", e))?;

        Ok(Self { db })
    }

    /// Store a single DHT entry.
    pub fn persist_entry(&self, key: &[u8; 32], record: &DhtEntryRecord) -> Result<(), String> {
        let mut buf = Vec::new();
        ciborium::into_writer(record, &mut buf)
            .map_err(|e| format!("CBOR encode: {}", e))?;

        let txn = self.db.begin_write().map_err(|e| format!("begin_write: {}", e))?;
        {
            let mut table = txn
                .open_table(DHT_ENTRIES)
                .map_err(|e| format!("open dht_entries: {}", e))?;
            table
                .insert(key, buf.as_slice())
                .map_err(|e| format!("insert: {}", e))?;
        }
        txn.commit().map_err(|e| format!("commit: {}", e))?;
        Ok(())
    }

    /// Store multiple entries in a single transaction (epoch flush).
    pub fn persist_batch(
        &self,
        entries: &[([u8; 32], DhtEntryRecord)],
    ) -> Result<(), String> {
        let txn = self.db.begin_write().map_err(|e| format!("begin_write: {}", e))?;
        {
            let mut table = txn
                .open_table(DHT_ENTRIES)
                .map_err(|e| format!("open dht_entries: {}", e))?;
            for (key, record) in entries {
                let mut buf = Vec::new();
                ciborium::into_writer(record, &mut buf)
                    .map_err(|e| format!("CBOR encode: {}", e))?;
                table
                    .insert(key, buf.as_slice())
                    .map_err(|e| format!("insert: {}", e))?;
            }
        }
        txn.commit().map_err(|e| format!("commit: {}", e))?;
        Ok(())
    }

    /// Load all persisted DHT entries.
    pub fn load_entries(&self) -> Result<Vec<([u8; 32], DhtEntryRecord)>, String> {
        let txn = self.db.begin_read().map_err(|e| format!("begin_read: {}", e))?;
        let table = match txn.open_table(DHT_ENTRIES) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(format!("open dht_entries: {}", e)),
        };

        let mut results = Vec::new();
        let iter = table.iter().map_err(|e| format!("iter: {}", e))?;
        for entry in iter {
            let (k, v) = entry.map_err(|e| format!("read entry: {}", e))?;
            let key: [u8; 32] = *k.value();
            let record: DhtEntryRecord = ciborium::from_reader(v.value())
                .map_err(|e| format!("CBOR decode: {}", e))?;
            results.push((key, record));
        }
        Ok(results)
    }

    /// Persist replica tracking metadata.
    pub fn persist_replica_meta(
        &self,
        cid: &[u8; 32],
        meta: &StoredKuMetaRecord,
    ) -> Result<(), String> {
        let mut buf = Vec::new();
        ciborium::into_writer(meta, &mut buf)
            .map_err(|e| format!("CBOR encode: {}", e))?;

        let txn = self.db.begin_write().map_err(|e| format!("begin_write: {}", e))?;
        {
            let mut table = txn
                .open_table(REPLICA_META)
                .map_err(|e| format!("open replica_meta: {}", e))?;
            table
                .insert(cid, buf.as_slice())
                .map_err(|e| format!("insert: {}", e))?;
        }
        txn.commit().map_err(|e| format!("commit: {}", e))?;
        Ok(())
    }

    /// Load all replica metadata.
    pub fn load_replica_meta(&self) -> Result<Vec<([u8; 32], StoredKuMetaRecord)>, String> {
        let txn = self.db.begin_read().map_err(|e| format!("begin_read: {}", e))?;
        let table = match txn.open_table(REPLICA_META) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(format!("open replica_meta: {}", e)),
        };

        let mut results = Vec::new();
        let iter = table.iter().map_err(|e| format!("iter: {}", e))?;
        for entry in iter {
            let (k, v) = entry.map_err(|e| format!("read entry: {}", e))?;
            let key: [u8; 32] = *k.value();
            let record: StoredKuMetaRecord = ciborium::from_reader(v.value())
                .map_err(|e| format!("CBOR decode: {}", e))?;
            results.push((key, record));
        }
        Ok(results)
    }

    /// Remove expired entries (TTL-based cleanup).
    ///
    /// Returns the number of entries removed.
    pub fn remove_expired(&self, now_secs: u64) -> Result<usize, String> {
        // First pass: read and find expired keys
        let expired_keys: Vec<[u8; 32]> = {
            let txn = self.db.begin_read().map_err(|e| format!("begin_read: {}", e))?;
            let table = match txn.open_table(DHT_ENTRIES) {
                Ok(t) => t,
                Err(redb::TableError::TableDoesNotExist(_)) => return Ok(0),
                Err(e) => return Err(format!("open dht_entries: {}", e)),
            };

            let mut keys = Vec::new();
            let iter = table.iter().map_err(|e| format!("iter: {}", e))?;
            for entry in iter {
                let (k, v) = entry.map_err(|e| format!("read entry: {}", e))?;
                let record: DhtEntryRecord = ciborium::from_reader(v.value())
                    .map_err(|e| format!("CBOR decode: {}", e))?;
                if let Some(ttl) = record.ttl_secs {
                    if record.stored_at + ttl <= now_secs {
                        keys.push(*k.value());
                    }
                }
            }
            keys
        };

        if expired_keys.is_empty() {
            return Ok(0);
        }

        // Second pass: remove expired keys in a write transaction
        let count = expired_keys.len();
        let txn = self.db.begin_write().map_err(|e| format!("begin_write: {}", e))?;
        {
            let mut table = txn
                .open_table(DHT_ENTRIES)
                .map_err(|e| format!("open dht_entries: {}", e))?;
            for key in &expired_keys {
                table
                    .remove(key)
                    .map_err(|e| format!("remove: {}", e))?;
            }
        }
        txn.commit().map_err(|e| format!("commit: {}", e))?;
        Ok(count)
    }

    /// Count stored DHT entries.
    pub fn entry_count(&self) -> Result<usize, String> {
        let txn = self.db.begin_read().map_err(|e| format!("begin_read: {}", e))?;
        let table = match txn.open_table(DHT_ENTRIES) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(0),
            Err(e) => return Err(format!("open dht_entries: {}", e)),
        };
        let len = table.len().map_err(|e| format!("len: {}", e))?;
        Ok(len as usize)
    }

    /// Count replica metadata entries.
    pub fn replica_count(&self) -> Result<usize, String> {
        let txn = self.db.begin_read().map_err(|e| format!("begin_read: {}", e))?;
        let table = match txn.open_table(REPLICA_META) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(0),
            Err(e) => return Err(format!("open replica_meta: {}", e)),
        };
        let len = table.len().map_err(|e| format!("len: {}", e))?;
        Ok(len as usize)
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Atomic counter for unique temp file names to avoid test collisions.
    static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_db_path(name: &str) -> std::path::PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!("dht_store_test_{}_{}.redb", name, id))
    }

    fn make_key(byte: u8) -> [u8; 32] {
        let mut k = [0u8; 32];
        k[0] = byte;
        k
    }

    fn make_record(val: &[u8], stored_at: u64, ttl: Option<u64>) -> DhtEntryRecord {
        DhtEntryRecord {
            value: val.to_vec(),
            stored_at,
            ttl_secs: ttl,
        }
    }

    #[test]
    fn test_open_creates_db() {
        let path = temp_db_path("open");
        let _db = DhtPersistence::open(&path).expect("should open");
        assert!(path.exists());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_persist_and_load_entry() {
        let path = temp_db_path("persist_load");
        let db = DhtPersistence::open(&path).unwrap();
        let key = make_key(1);
        let record = make_record(b"hello", 1000, None);

        db.persist_entry(&key, &record).unwrap();
        let entries = db.load_entries().unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, key);
        assert_eq!(entries[0].1.value, b"hello");
        assert_eq!(entries[0].1.stored_at, 1000);
        assert_eq!(entries[0].1.ttl_secs, None);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_persist_batch() {
        let path = temp_db_path("batch");
        let db = DhtPersistence::open(&path).unwrap();

        let entries: Vec<([u8; 32], DhtEntryRecord)> = (0..5)
            .map(|i| (make_key(i), make_record(&[i], 1000 + i as u64, None)))
            .collect();

        db.persist_batch(&entries).unwrap();
        let loaded = db.load_entries().unwrap();
        assert_eq!(loaded.len(), 5);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_load_empty() {
        let path = temp_db_path("empty");
        let db = DhtPersistence::open(&path).unwrap();

        let entries = db.load_entries().unwrap();
        assert!(entries.is_empty());

        let meta = db.load_replica_meta().unwrap();
        assert!(meta.is_empty());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_persist_replica_meta() {
        let path = temp_db_path("replica_persist");
        let db = DhtPersistence::open(&path).unwrap();
        let cid = make_key(0xAA);
        let meta = StoredKuMetaRecord {
            actual_replicas: 5,
            first_stored_epoch: 100,
            epochs_stored: 42,
        };

        db.persist_replica_meta(&cid, &meta).unwrap();
        let loaded = db.load_replica_meta().unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].0, cid);
        assert_eq!(loaded[0].1.actual_replicas, 5);
        assert_eq!(loaded[0].1.first_stored_epoch, 100);
        assert_eq!(loaded[0].1.epochs_stored, 42);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_load_replica_meta() {
        let path = temp_db_path("replica_load");
        let db = DhtPersistence::open(&path).unwrap();

        // Store multiple replica metadata entries
        for i in 0..3u8 {
            let cid = make_key(i);
            let meta = StoredKuMetaRecord {
                actual_replicas: i as u32 + 1,
                first_stored_epoch: 100,
                epochs_stored: i as u64 * 10,
            };
            db.persist_replica_meta(&cid, &meta).unwrap();
        }

        let loaded = db.load_replica_meta().unwrap();
        assert_eq!(loaded.len(), 3);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_remove_expired() {
        let path = temp_db_path("expired");
        let db = DhtPersistence::open(&path).unwrap();

        // Entry that expires at time 1100 (stored_at=1000, ttl=100)
        db.persist_entry(&make_key(1), &make_record(b"exp", 1000, Some(100)))
            .unwrap();
        // Entry that expires at time 1500 (stored_at=1000, ttl=500)
        db.persist_entry(&make_key(2), &make_record(b"later", 1000, Some(500)))
            .unwrap();

        let removed = db.remove_expired(1200).unwrap();
        assert_eq!(removed, 1);
        assert_eq!(db.entry_count().unwrap(), 1);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_remove_expired_keeps_permanent() {
        let path = temp_db_path("permanent");
        let db = DhtPersistence::open(&path).unwrap();

        // Permanent entry (no TTL)
        db.persist_entry(&make_key(1), &make_record(b"perm", 1000, None))
            .unwrap();
        // Expired entry
        db.persist_entry(&make_key(2), &make_record(b"exp", 1000, Some(10)))
            .unwrap();

        let removed = db.remove_expired(2000).unwrap();
        assert_eq!(removed, 1);
        assert_eq!(db.entry_count().unwrap(), 1);

        // Verify the permanent entry is still there
        let entries = db.load_entries().unwrap();
        assert_eq!(entries[0].1.value, b"perm");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_entry_count() {
        let path = temp_db_path("count");
        let db = DhtPersistence::open(&path).unwrap();

        assert_eq!(db.entry_count().unwrap(), 0);
        db.persist_entry(&make_key(1), &make_record(b"a", 1000, None))
            .unwrap();
        assert_eq!(db.entry_count().unwrap(), 1);
        db.persist_entry(&make_key(2), &make_record(b"b", 1000, None))
            .unwrap();
        assert_eq!(db.entry_count().unwrap(), 2);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_replica_count() {
        let path = temp_db_path("replica_count");
        let db = DhtPersistence::open(&path).unwrap();

        assert_eq!(db.replica_count().unwrap(), 0);

        let meta = StoredKuMetaRecord {
            actual_replicas: 7,
            first_stored_epoch: 1,
            epochs_stored: 1,
        };
        db.persist_replica_meta(&make_key(1), &meta).unwrap();
        assert_eq!(db.replica_count().unwrap(), 1);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_overwrite_entry() {
        let path = temp_db_path("overwrite");
        let db = DhtPersistence::open(&path).unwrap();
        let key = make_key(1);

        db.persist_entry(&key, &make_record(b"old", 1000, None))
            .unwrap();
        db.persist_entry(&key, &make_record(b"new", 2000, Some(300)))
            .unwrap();

        let entries = db.load_entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1.value, b"new");
        assert_eq!(entries[0].1.stored_at, 2000);
        assert_eq!(entries[0].1.ttl_secs, Some(300));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_overwrite_replica_meta() {
        let path = temp_db_path("overwrite_meta");
        let db = DhtPersistence::open(&path).unwrap();
        let cid = make_key(1);

        let meta1 = StoredKuMetaRecord {
            actual_replicas: 3,
            first_stored_epoch: 10,
            epochs_stored: 5,
        };
        db.persist_replica_meta(&cid, &meta1).unwrap();

        let meta2 = StoredKuMetaRecord {
            actual_replicas: 7,
            first_stored_epoch: 10,
            epochs_stored: 50,
        };
        db.persist_replica_meta(&cid, &meta2).unwrap();

        let loaded = db.load_replica_meta().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].1.actual_replicas, 7);
        assert_eq!(loaded[0].1.epochs_stored, 50);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_schema_version_initialized() {
        let path = temp_db_path("schema_ver");
        {
            let _db = DhtPersistence::open(&path).unwrap();
            // DhtPersistence dropped here, releasing the database lock
        }

        // Open the raw redb database and verify schema was set
        let raw_db = Database::open(&path).unwrap();
        let version = ku_core::obs_schema::redb_schema::read_version(&raw_db);
        assert_eq!(version.as_u32(), 1);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_persist_entry_large_value() {
        let path = temp_db_path("large_val");
        let db = DhtPersistence::open(&path).unwrap();
        let key = make_key(0xFF);

        // 1 MB value
        let large_value = vec![0xABu8; 1_000_000];
        let record = DhtEntryRecord {
            value: large_value.clone(),
            stored_at: 9999,
            ttl_secs: Some(3600),
        };

        db.persist_entry(&key, &record).unwrap();
        let entries = db.load_entries().unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1.value.len(), 1_000_000);
        assert_eq!(entries[0].1.value[0], 0xAB);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_batch_empty() {
        let path = temp_db_path("batch_empty");
        let db = DhtPersistence::open(&path).unwrap();

        // Empty batch should succeed without error
        db.persist_batch(&[]).unwrap();
        assert_eq!(db.entry_count().unwrap(), 0);

        let _ = std::fs::remove_file(&path);
    }
}
