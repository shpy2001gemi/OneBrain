//! # Persistent KU Storage — redb backend (v6)
//!
//! ACID-compliant persistent storage for KuRuntime (Core DNA + Epigenetics).
//!
//! ## Storage Architecture
//! - `kus`: CID (BLAKE3 hash) → Core DNA wire bytes (immutable Layer 1)
//! - `epigenetics`: CID → serialized Epigenetics (mutable Layer 2)
//! - `index_trust`: trust_score (u16 BE) + CID → empty (range query index)
//! - `index_concept`: concept_id (u64 BE) + CID → empty (lookup index)

use std::path::Path;

use ku_core::{KuRuntime, Epigenetics, TrustSection};
use ku_core::core_dna::decode_core_dna;
use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};

// ─── Table Definitions ─────────────────────────────────────────────────────

/// Main KU table: CID (32 bytes) → Core DNA wire bytes (Layer 1, immutable).
const TABLE_KUS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("kus");

/// Epigenetics table: CID (32 bytes) → serialized Epigenetics (Layer 2, mutable).
const TABLE_EPI: TableDefinition<&[u8], &[u8]> = TableDefinition::new("epigenetics");

/// Trust score index: (trust_score as u16 BE + CID) → empty.
/// Enables range queries on trust_score.
const TABLE_INDEX_TRUST: TableDefinition<&[u8], &[u8]> = TableDefinition::new("index_trust");

/// Concept index: (concept_id as u64 BE + CID) → empty.
/// Enables lookups by concept_id.
const TABLE_INDEX_CONCEPT: TableDefinition<&[u8], &[u8]> = TableDefinition::new("index_concept");

// ─── Storage ───────────────────────────────────────────────────────────────

/// Persistent KU storage backed by redb.
///
/// Stores Core DNA wire bytes (immutable) separately from Epigenetics (mutable).
/// Retrieves as `KuRuntime` by reassembling both layers.
pub struct KuStorage {
    db: Database,
}

/// Storage operation errors.
#[derive(Debug)]
pub enum StorageError {
    /// Database I/O error.
    DatabaseError(String),
    /// Encoding/decoding error.
    CodecError(String),
    /// Key not found.
    NotFound,
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DatabaseError(msg) => write!(f, "Storage error: {}", msg),
            Self::CodecError(msg) => write!(f, "Codec error: {}", msg),
            Self::NotFound => write!(f, "KU not found"),
        }
    }
}

impl From<redb::Error> for StorageError {
    fn from(e: redb::Error) -> Self {
        Self::DatabaseError(format!("{}", e))
    }
}

impl From<redb::DatabaseError> for StorageError {
    fn from(e: redb::DatabaseError) -> Self {
        Self::DatabaseError(format!("{}", e))
    }
}

impl From<redb::TransactionError> for StorageError {
    fn from(e: redb::TransactionError) -> Self {
        Self::DatabaseError(format!("{}", e))
    }
}

impl From<redb::TableError> for StorageError {
    fn from(e: redb::TableError) -> Self {
        Self::DatabaseError(format!("{}", e))
    }
}

impl From<redb::CommitError> for StorageError {
    fn from(e: redb::CommitError) -> Self {
        Self::DatabaseError(format!("{}", e))
    }
}

impl From<redb::StorageError> for StorageError {
    fn from(e: redb::StorageError) -> Self {
        Self::DatabaseError(format!("{}", e))
    }
}

impl KuStorage {
    /// Open or create a storage database at the given path.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let db = Database::create(path)
            .map_err(|e| StorageError::DatabaseError(format!("{}", e)))?;

        // Ensure tables exist
        let txn = db.begin_write()?;
        {
            let _ = txn.open_table(TABLE_KUS)?;
            let _ = txn.open_table(TABLE_EPI)?;
            let _ = txn.open_table(TABLE_INDEX_TRUST)?;
            let _ = txn.open_table(TABLE_INDEX_CONCEPT)?;
        }
        txn.commit()?;

        Ok(Self { db })
    }

    /// Store a KuRuntime. Returns the CID.
    ///
    /// Core DNA wire bytes go to `kus` table (immutable).
    /// Epigenetics go to `epigenetics` table (can be updated).
    pub fn put(&self, ku: &KuRuntime) -> Result<[u8; 32], StorageError> {
        let cid = ku.cid;

        // Serialize epigenetics as JSON (compact, human-debuggable)
        let epi_bytes = serde_json::to_vec(&ku.epi)
            .map_err(|e| StorageError::CodecError(format!("{}", e)))?;

        let txn = self.db.begin_write()?;
        {
            // Core DNA table (immutable wire bytes)
            let mut table = txn.open_table(TABLE_KUS)?;
            table.insert(cid.as_slice(), ku.wire_bytes.as_slice())?;

            // Epigenetics table (mutable)
            let mut epi_table = txn.open_table(TABLE_EPI)?;
            epi_table.insert(cid.as_slice(), epi_bytes.as_slice())?;

            // Trust index
            let mut trust_key = Vec::with_capacity(34);
            trust_key.extend_from_slice(&ku.epi.trust.trust_score.to_be_bytes());
            trust_key.extend_from_slice(&cid);
            let mut idx = txn.open_table(TABLE_INDEX_TRUST)?;
            idx.insert(trust_key.as_slice(), &[] as &[u8])?;

            // Concept index — extract concept IDs from Core DNA instructions
            for concept_id in ku.concept_ids() {
                let mut concept_key = Vec::with_capacity(40);
                concept_key.extend_from_slice(&concept_id.to_be_bytes());
                concept_key.extend_from_slice(&cid);
                let mut idx = txn.open_table(TABLE_INDEX_CONCEPT)?;
                idx.insert(concept_key.as_slice(), &[] as &[u8])?;
            }
        }
        txn.commit()?;

        Ok(cid)
    }

    /// Update Epigenetics only (Core DNA remains immutable).
    pub fn update_epi(&self, cid: &[u8; 32], epi: &Epigenetics) -> Result<(), StorageError> {
        let epi_bytes = serde_json::to_vec(epi)
            .map_err(|e| StorageError::CodecError(format!("{}", e)))?;

        let txn = self.db.begin_write()?;
        {
            // Verify KU exists
            let table = txn.open_table(TABLE_KUS)?;
            if table.get(cid.as_slice())?.is_none() {
                return Err(StorageError::NotFound);
            }

            // Update epigenetics
            let mut epi_table = txn.open_table(TABLE_EPI)?;
            epi_table.insert(cid.as_slice(), epi_bytes.as_slice())?;
        }
        txn.commit()?;
        Ok(())
    }

    /// Get a KuRuntime by its CID.
    ///
    /// Reassembles from Core DNA wire bytes + Epigenetics.
    pub fn get(&self, cid: &[u8; 32]) -> Result<KuRuntime, StorageError> {
        let txn = self.db.begin_read()?;

        // Read Core DNA wire bytes
        let table = txn.open_table(TABLE_KUS)?;
        let value = table.get(cid.as_slice())?
            .ok_or(StorageError::NotFound)?;
        let wire_bytes = value.value().to_vec();

        // Decode Core DNA
        let core_dna = decode_core_dna(&wire_bytes)
            .map_err(|e| StorageError::CodecError(format!("{}", e)))?;
        let mut ku = KuRuntime::from_dna(core_dna)
            .map_err(|e| StorageError::CodecError(format!("{}", e)))?;

        // Read Epigenetics (if exists)
        let epi_table = txn.open_table(TABLE_EPI)?;
        if let Some(epi_val) = epi_table.get(cid.as_slice())? {
            let epi: Epigenetics = serde_json::from_slice(epi_val.value())
                .map_err(|e| StorageError::CodecError(format!("{}", e)))?;
            ku.epi = epi;
        }

        Ok(ku)
    }

    /// Check if a CID exists in storage.
    pub fn has(&self, cid: &[u8; 32]) -> Result<bool, StorageError> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(TABLE_KUS)?;
        Ok(table.get(cid.as_slice())?.is_some())
    }

    /// Delete a KU by CID (both Core DNA and Epigenetics).
    pub fn delete(&self, cid: &[u8; 32]) -> Result<bool, StorageError> {
        let txn = self.db.begin_write()?;
        let existed;
        {
            let mut table = txn.open_table(TABLE_KUS)?;
            existed = table.remove(cid.as_slice())?.is_some();

            let mut epi_table = txn.open_table(TABLE_EPI)?;
            epi_table.remove(cid.as_slice())?;
        }
        txn.commit()?;
        Ok(existed)
    }

    /// Count all KUs in storage.
    pub fn count(&self) -> Result<usize, StorageError> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(TABLE_KUS)?;
        Ok(table.len()? as usize)
    }

    /// Get all KUs (for small datasets / testing).
    pub fn get_all(&self) -> Result<Vec<KuRuntime>, StorageError> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(TABLE_KUS)?;
        let epi_table = txn.open_table(TABLE_EPI)?;

        let mut results = Vec::new();
        let iter = table.iter()?;
        for entry in iter {
            let entry = entry.map_err(|e| StorageError::DatabaseError(format!("{}", e)))?;
            let cid_bytes = entry.0.value();
            let wire_bytes = entry.1.value().to_vec();

            let core_dna = decode_core_dna(&wire_bytes)
                .map_err(|e| StorageError::CodecError(format!("{}", e)))?;
            let mut ku = KuRuntime::from_dna(core_dna)
                .map_err(|e| StorageError::CodecError(format!("{}", e)))?;

            // Load epigenetics
            if let Some(epi_val) = epi_table.get(cid_bytes)? {
                let epi: Epigenetics = serde_json::from_slice(epi_val.value())
                    .map_err(|e| StorageError::CodecError(format!("{}", e)))?;
                ku.epi = epi;
            }

            results.push(ku);
        }

        Ok(results)
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ku_core::core_dna::{CoreDna, CoreDnaHeader, Instruction};
    use ku_core::EpistemicStatus;
    use std::fs;

    fn make_test_ku(trust_score: u16) -> KuRuntime {
        let dna = CoreDna {
            header: CoreDnaHeader {
                version: 1,
                gene_type: 0, // Fact
                has_qualifiers: false,
            },
            instructions: vec![
                Instruction::Triple { s: 128, p: 133, o: 132 },
                Instruction::Certainty { level: 9500 },
            ],
        };
        let mut ku = KuRuntime::from_dna(dna).expect("create KuRuntime");
        ku.epi = Epigenetics::with_trust(trust_score, 8000);
        ku.epi.set_epistemic_status(EpistemicStatus::Evidence);
        ku
    }

    use std::sync::atomic::{AtomicU64, Ordering};
    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_db_path() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("ku_test_v6");
        fs::create_dir_all(&dir).ok();
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        dir.join(format!("test_{}_{}.redb", std::process::id(), id))
    }

    fn cleanup(path: &Path) {
        fs::remove_file(path).ok();
    }

    #[test]
    fn test_open_create_db() {
        let path = temp_db_path();
        let _storage = KuStorage::open(&path).unwrap();
        cleanup(&path);
    }

    #[test]
    fn test_put_and_get() {
        let path = temp_db_path();
        let storage = KuStorage::open(&path).unwrap();

        let ku = make_test_ku(9000);
        let cid = storage.put(&ku).unwrap();
        assert_eq!(cid.len(), 32);

        let retrieved = storage.get(&cid).unwrap();
        assert_eq!(retrieved.trust_score(), 9000);
        assert_eq!(retrieved.gene_type(), 0); // Fact
        assert_eq!(retrieved.epi.trust.confidence, 8000);

        cleanup(&path);
    }

    #[test]
    fn test_has() {
        let path = temp_db_path();
        let storage = KuStorage::open(&path).unwrap();

        let cid = storage.put(&make_test_ku(5000)).unwrap();
        assert!(storage.has(&cid).unwrap());
        assert!(!storage.has(&[0xAA; 32]).unwrap());

        cleanup(&path);
    }

    #[test]
    fn test_delete() {
        let path = temp_db_path();
        let storage = KuStorage::open(&path).unwrap();

        let cid = storage.put(&make_test_ku(7000)).unwrap();
        assert_eq!(storage.count().unwrap(), 1);

        assert!(storage.delete(&cid).unwrap());
        assert_eq!(storage.count().unwrap(), 0);
        assert!(!storage.has(&cid).unwrap());

        cleanup(&path);
    }

    #[test]
    fn test_count_and_get_all() {
        let path = temp_db_path();
        let storage = KuStorage::open(&path).unwrap();

        // Use different concept IDs to produce different CIDs
        // (trust_score is Epigenetics, doesn't affect CID)
        let mut ku1 = make_test_ku(1000);
        let mut ku2 = make_test_ku(2000);
        let mut ku3 = make_test_ku(3000);
        // Modify Core DNA to make CIDs unique
        ku2.dna.instructions = vec![
            Instruction::Triple { s: 200, p: 133, o: 132 },
            Instruction::Certainty { level: 9500 },
        ];
        ku2.recompute();
        ku3.dna.instructions = vec![
            Instruction::Triple { s: 300, p: 133, o: 132 },
            Instruction::Certainty { level: 9500 },
        ];
        ku3.recompute();

        storage.put(&ku1).unwrap();
        storage.put(&ku2).unwrap();
        storage.put(&ku3).unwrap();

        assert_eq!(storage.count().unwrap(), 3);

        let all = storage.get_all().unwrap();
        assert_eq!(all.len(), 3);

        cleanup(&path);
    }

    #[test]
    fn test_deterministic_cid() {
        let path = temp_db_path();
        let storage = KuStorage::open(&path).unwrap();

        let ku = make_test_ku(8000);
        let cid1 = storage.put(&ku).unwrap();
        let cid2 = storage.put(&ku).unwrap();

        // Same content → same CID (idempotent)
        assert_eq!(cid1, cid2);
        assert_eq!(storage.count().unwrap(), 1, "Duplicate should overwrite");

        cleanup(&path);
    }

    #[test]
    fn test_not_found() {
        let path = temp_db_path();
        let storage = KuStorage::open(&path).unwrap();

        let result = storage.get(&[0xFF; 32]);
        assert!(matches!(result, Err(StorageError::NotFound)));

        cleanup(&path);
    }

    #[test]
    fn test_update_epigenetics() {
        let path = temp_db_path();
        let storage = KuStorage::open(&path).unwrap();

        let ku = make_test_ku(5000);
        let cid = storage.put(&ku).unwrap();

        // Update epigenetics
        let mut new_epi = ku.epi.clone();
        new_epi.trust.trust_score = 9999;
        new_epi.set_epistemic_status(EpistemicStatus::Consensus);
        storage.update_epi(&cid, &new_epi).unwrap();

        // Verify Core DNA unchanged, epigenetics updated
        let retrieved = storage.get(&cid).unwrap();
        assert_eq!(retrieved.trust_score(), 9999);
        assert_eq!(retrieved.epi.epistemic_status, EpistemicStatus::Consensus);
        assert_eq!(retrieved.gene_type(), 0); // Core DNA unchanged

        cleanup(&path);
    }
}
