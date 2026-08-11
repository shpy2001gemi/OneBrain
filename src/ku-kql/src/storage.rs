//! # Persistent KU Storage — redb backend (v7)
//!
//! ACID-compliant persistent storage for KuRuntime (Core DNA + Epigenetics).
//!
//! ## Storage Architecture
//! - `kus`: CID (BLAKE3 hash) → Core DNA wire bytes (immutable Layer 1)
//! - `epigenetics`: CID → serialized Epigenetics (mutable Layer 2)
//! - `index_trust`: trust_score (u16 BE) + CID → empty (range query index)
//! - `index_concept`: concept_id (u64 BE) + CID → empty (lookup index)

use std::path::Path;

use ku_core::core_dna::decode_core_dna;
use ku_core::obs_schema;
use ku_core::{Epigenetics, KuRuntime};
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

/// CCID index: (ccid 16 bytes + CID 32 bytes) → empty.
/// Enables O(1) lookups of KUs containing a specific concept CCID.
const TABLE_INDEX_CCID: TableDefinition<&[u8], &[u8]> = TableDefinition::new("index_ccid");

// ─── Storage ───────────────────────────────────────────────────────────────

/// Persistent KU storage backed by redb.
///
/// Stores Core DNA wire bytes (immutable) separately from Epigenetics (mutable).
/// Retrieves as `KuRuntime` by reassembling both layers.
pub struct KuStorage {
    db: Database,
    /// Graph edge index storage (6 redb tables for O(1) graph queries)
    graph: crate::graph_storage::GraphStorage,
    access: KuStorageAccess,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KuStorageAccess {
    LegacyWritable,
    BaseReadOnly,
    MigrationEvidence,
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
    /// Base v1 never routes writes through the legacy KU store.
    LegacyReadOnly,
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DatabaseError(msg) => write!(f, "Storage error: {}", msg),
            Self::CodecError(msg) => write!(f, "Codec error: {}", msg),
            Self::NotFound => write!(f, "KU not found"),
            Self::LegacyReadOnly => write!(f, "legacy KU storage is read-only in Base mode"),
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
    ///
    /// Also creates a neighboring graph storage file at `<path>.graph.redb`
    /// for O(1) bond/edge index queries.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        Self::open_with_access(path, KuStorageAccess::LegacyWritable)
    }

    pub fn open_base_read_only(path: &Path) -> Result<Self, StorageError> {
        Self::open_with_access(path, KuStorageAccess::BaseReadOnly)
    }

    pub fn open_migration_evidence(path: &Path) -> Result<Self, StorageError> {
        Self::open_with_access(path, KuStorageAccess::MigrationEvidence)
    }

    fn open_with_access(path: &Path, access: KuStorageAccess) -> Result<Self, StorageError> {
        let db =
            Database::create(path).map_err(|e| StorageError::DatabaseError(format!("{}", e)))?;

        // Ensure tables exist
        let txn = db.begin_write()?;
        {
            let _ = txn.open_table(TABLE_KUS)?;
            let _ = txn.open_table(TABLE_EPI)?;
            let _ = txn.open_table(TABLE_INDEX_TRUST)?;
            let _ = txn.open_table(TABLE_INDEX_CONCEPT)?;
            let _ = txn.open_table(TABLE_INDEX_CCID)?;
        }
        txn.commit()?;

        // Initialize/validate schema version
        obs_schema::redb_schema::ensure_schema(&db, &obs_schema::ku_storage_registry())
            .map_err(|e| StorageError::DatabaseError(format!("Schema init failed: {}", e)))?;

        // Create graph storage with neighboring file
        let graph_path = path.with_extension("graph.redb");
        let graph = crate::graph_storage::GraphStorage::open(&graph_path)?;

        Ok(Self { db, graph, access })
    }

    /// Store a KuRuntime. Returns the CID.
    ///
    /// Core DNA wire bytes go to `kus` table (immutable).
    /// Epigenetics go to `epigenetics` table (can be updated).
    pub fn put(&self, ku: &KuRuntime) -> Result<[u8; 32], StorageError> {
        self.ensure_writable()?;
        let cid = ku.cid;

        // Serialize epigenetics as JSON (compact, human-debuggable)
        let epi_bytes =
            serde_json::to_vec(&ku.epi).map_err(|e| StorageError::CodecError(format!("{}", e)))?;

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

            // CCID index — map each concept CCID to this KU's CID for O(1) lookup
            {
                let mut ccid_idx = txn.open_table(TABLE_INDEX_CCID)?;
                for entry in &ku.dna.concept_table {
                    let mut ccid_key = Vec::with_capacity(48);
                    ccid_key.extend_from_slice(&entry.ccid); // 16 bytes CCID
                    ccid_key.extend_from_slice(&cid); // 32 bytes CID
                    ccid_idx.insert(ccid_key.as_slice(), &[] as &[u8])?;
                }
            }
        }
        txn.commit()?;

        // Index bonds in graph storage (best-effort: don't fail KU storage if graph indexing fails)
        for bond in &ku.epi.bonds {
            if bond.target_cid.len() == 32 {
                let target: [u8; 32] = bond.target_cid[..32].try_into().unwrap();
                let meta = ku_core::graph_types::BondMeta::from_bond(bond);
                let _ = self.graph.insert_bond(&cid, &target, bond.relation, &meta);
            }
        }

        // Index concept-to-concept edges from concept_table (best-effort)
        // Each instruction pair (e.g., Triple(S,P,O) → S-[Extends]→O) is resolved via
        // the concept_table (local_id → CCID[16]) and stored as a graph edge.
        let concept_edges = ku.concept_graph_edges();
        for edge in &concept_edges {
            let meta = ku_core::graph_types::BondMeta {
                weight: 500, // default weight for concept edges
                creator: ku_core::types::Creator::System,
                state: ku_core::types::EdgeState::Active,
                decay: ku_core::types::DecayRate::None,
                timestamp: 0, // concept edges have no specific timestamp
            };
            let _ = self
                .graph
                .insert_bond(&edge.src, &edge.tgt, edge.rel, &meta);
        }

        Ok(cid)
    }

    /// Update Epigenetics only (Core DNA remains immutable).
    pub fn update_epi(&self, cid: &[u8; 32], epi: &Epigenetics) -> Result<(), StorageError> {
        self.ensure_writable()?;
        let epi_bytes =
            serde_json::to_vec(epi).map_err(|e| StorageError::CodecError(format!("{}", e)))?;

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
        let value = table.get(cid.as_slice())?.ok_or(StorageError::NotFound)?;
        let wire_bytes = value.value().to_vec();

        // Verify content integrity: BLAKE3(wire_bytes) must match the CID key
        let computed_cid = blake3::hash(&wire_bytes);
        if computed_cid.as_bytes() != cid {
            return Err(StorageError::CodecError(
                "CID mismatch: stored data corrupted".into(),
            ));
        }

        // Decode Core DNA
        let core_dna =
            decode_core_dna(&wire_bytes).map_err(|e| StorageError::CodecError(format!("{}", e)))?;
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
        self.ensure_writable()?;
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

    /// Explicit primary-row scan for the one legacy migration adapter.
    pub fn scan_migration_evidence(&self) -> Result<Vec<KuRuntime>, StorageError> {
        if self.access != KuStorageAccess::MigrationEvidence {
            return Err(StorageError::LegacyReadOnly);
        }
        self.get_all()
    }

    fn ensure_writable(&self) -> Result<(), StorageError> {
        if self.access == KuStorageAccess::LegacyWritable {
            Ok(())
        } else {
            Err(StorageError::LegacyReadOnly)
        }
    }

    /// Access the graph edge index storage for direct queries.
    pub fn graph(&self) -> &crate::graph_storage::GraphStorage {
        &self.graph
    }

    // ─── Concept Graph Queries (convenience) ────────────────────────────

    /// Find all concepts directly related to a given concept (outgoing).
    ///
    /// Convenience wrapper around `graph().concept_outgoing()`.
    pub fn concept_relations_outgoing(
        &self,
        ccid: &[u8; 16],
    ) -> Result<Vec<crate::graph_storage::ConceptRelation>, StorageError> {
        self.graph.concept_outgoing(ccid)
    }

    /// Find all concepts that point to a given concept (incoming).
    ///
    /// Convenience wrapper around `graph().concept_incoming()`.
    pub fn concept_relations_incoming(
        &self,
        ccid: &[u8; 16],
    ) -> Result<Vec<crate::graph_storage::ConceptRelation>, StorageError> {
        self.graph.concept_incoming(ccid)
    }

    /// BFS traversal of concept neighbors up to `max_depth` hops.
    ///
    /// Convenience wrapper around `graph().concept_neighbors()`.
    pub fn concept_neighbors(
        &self,
        ccid: &[u8; 16],
        max_depth: usize,
        filter_rel: Option<ku_core::types::RelationType>,
    ) -> Result<Vec<(crate::graph_storage::ConceptRelation, usize)>, StorageError> {
        self.graph.concept_neighbors(ccid, max_depth, filter_rel)
    }

    /// Find KUs that contain a given concept (by CCID) in their concept_table.
    ///
    /// Returns CIDs of KUs whose concept_table maps any local_id to the given CCID.
    /// Uses the CCID index table for O(1) prefix scan instead of full table scan.
    pub fn find_kus_by_concept(&self, ccid: &[u8; 16]) -> Result<Vec<[u8; 32]>, StorageError> {
        let txn = self
            .db
            .begin_read()
            .map_err(|e| StorageError::DatabaseError(format!("{}", e)))?;

        // Try CCID index first (O(1) prefix scan)
        match txn.open_table(TABLE_INDEX_CCID) {
            Ok(table) => {
                let mut ku_cids: Vec<[u8; 32]> = Vec::new();
                for result in table.range::<&[u8]>(ccid.as_slice()..)? {
                    let (key, _) = result?;
                    let k = key.value();
                    if !k.starts_with(ccid) {
                        break; // past our prefix
                    }
                    if k.len() == 48 {
                        let mut cid = [0u8; 32];
                        cid.copy_from_slice(&k[16..48]);
                        ku_cids.push(cid);
                    }
                }
                Ok(ku_cids)
            }
            Err(_) => {
                // Fallback: full scan if index table doesn't exist (old DB)
                let kus_table = txn
                    .open_table(TABLE_KUS)
                    .map_err(|e| StorageError::DatabaseError(format!("{}", e)))?;
                let mut ku_cids: Vec<[u8; 32]> = Vec::new();
                for result in kus_table
                    .iter()
                    .map_err(|e| StorageError::DatabaseError(format!("{}", e)))?
                {
                    let (key, value) =
                        result.map_err(|e| StorageError::DatabaseError(format!("{}", e)))?;
                    let k = key.value();
                    if k.len() != 32 {
                        continue;
                    }
                    let v = value.value();
                    if let Ok(dna) = decode_core_dna(v) {
                        for entry in &dna.concept_table {
                            if entry.ccid == *ccid {
                                let mut cid = [0u8; 32];
                                cid.copy_from_slice(k);
                                ku_cids.push(cid);
                                break;
                            }
                        }
                    }
                }
                Ok(ku_cids)
            }
        }
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
                version: 2,
                gene_type: 0, // Fact
                has_concept_table: false,
            },
            concept_table: Vec::new(),
            instructions: vec![
                Instruction::Triple {
                    s: 128,
                    p: 133,
                    o: 132,
                },
                Instruction::Certainty { level: 9500 },
            ],
        };
        let mut ku = KuRuntime::from_dna(dna).expect("create KuRuntime");
        ku.epi = Epigenetics::with_trust(trust_score, 8000);
        ku.epi.trust.epistemic_status = EpistemicStatus::Evidence;
        ku
    }

    use std::sync::atomic::{AtomicU64, Ordering};
    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_db_path() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("ku_test_v7");
        fs::create_dir_all(&dir).ok();
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        dir.join(format!("test_{}_{}.redb", std::process::id(), id))
    }

    fn cleanup(path: &Path) {
        fs::remove_file(path).ok();
        // Also clean up the neighboring graph storage file
        let graph_path = path.with_extension("graph.redb");
        fs::remove_file(&graph_path).ok();
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
            Instruction::Triple {
                s: 200,
                p: 133,
                o: 132,
            },
            Instruction::Certainty { level: 9500 },
        ];
        ku2.recompute();
        ku3.dna.instructions = vec![
            Instruction::Triple {
                s: 300,
                p: 133,
                o: 132,
            },
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
        new_epi.trust.epistemic_status = EpistemicStatus::Consensus;
        storage.update_epi(&cid, &new_epi).unwrap();

        // Verify Core DNA unchanged, epigenetics updated
        let retrieved = storage.get(&cid).unwrap();
        assert_eq!(retrieved.trust_score(), 9999);
        assert_eq!(
            retrieved.epi.trust.epistemic_status,
            EpistemicStatus::Consensus
        );
        assert_eq!(retrieved.gene_type(), 0); // Core DNA unchanged

        cleanup(&path);
    }

    // ─── Graph integration tests ─────────────────────────────────────────

    #[test]
    fn test_graph_accessor() {
        let path = temp_db_path();
        let storage = KuStorage::open(&path).unwrap();

        // Verify graph() returns a working GraphStorage
        let stats = storage.graph().stats().unwrap();
        assert_eq!(stats.total_edges, 0);

        cleanup(&path);
    }

    #[test]
    fn test_put_indexes_bonds() {
        use ku_core::types::{Bond, Creator, EdgeState, RelationType};

        let path = temp_db_path();
        let storage = KuStorage::open(&path).unwrap();

        // Create a target KU first (we need a real 32-byte CID)
        let target_ku = make_test_ku(5000);
        let target_cid = storage.put(&target_ku).unwrap();

        // Create a source KU with a bond pointing to target
        let mut source_ku = make_test_ku(7000);
        // Modify DNA to get a different CID
        source_ku.dna.instructions = vec![
            Instruction::Triple {
                s: 999,
                p: 133,
                o: 132,
            },
            Instruction::Certainty { level: 9500 },
        ];
        source_ku.recompute();
        source_ku.epi.bonds.push(Bond {
            target_cid: target_cid.to_vec(),
            relation: RelationType::Extends,
            weight: 8000,
            creator: Creator::Human,
            created_at: 1_700_000_000,
            evidence: vec![],
            state: EdgeState::Active,
            initial_weight: None,
            decay: None,
            last_reinforced: None,
            reinforce_count: None,
            bidirectional: None,
            context: vec![],
            order: None,
            required: None,
        });
        let source_cid = storage.put(&source_ku).unwrap();

        // Verify bond was auto-indexed in graph storage
        let outgoing = storage.graph().outgoing_bonds(&source_cid).unwrap();
        assert_eq!(outgoing.len(), 1);
        assert_eq!(outgoing[0].0, RelationType::Extends);
        assert_eq!(outgoing[0].1, target_cid);
        assert_eq!(outgoing[0].2.weight, 8000);

        // Verify incoming from target's perspective
        let incoming = storage.graph().incoming_bonds(&target_cid).unwrap();
        assert_eq!(incoming.len(), 1);
        assert_eq!(incoming[0].1, source_cid);

        cleanup(&path);
    }

    #[test]
    fn test_put_indexes_multiple_bonds() {
        use ku_core::types::{Bond, Creator, EdgeState, RelationType};

        let path = temp_db_path();
        let storage = KuStorage::open(&path).unwrap();

        // Create two target KUs
        let tgt1 = make_test_ku(5000);
        let cid1 = storage.put(&tgt1).unwrap();

        let mut tgt2 = make_test_ku(6000);
        tgt2.dna.instructions = vec![
            Instruction::Triple {
                s: 500,
                p: 133,
                o: 132,
            },
            Instruction::Certainty { level: 9500 },
        ];
        tgt2.recompute();
        let cid2 = storage.put(&tgt2).unwrap();

        // Source KU with 2 bonds
        let mut src = make_test_ku(9000);
        src.dna.instructions = vec![
            Instruction::Triple {
                s: 888,
                p: 133,
                o: 132,
            },
            Instruction::Certainty { level: 9500 },
        ];
        src.recompute();
        src.epi.bonds.push(Bond {
            target_cid: cid1.to_vec(),
            relation: RelationType::Extends,
            weight: 8000,
            creator: Creator::Human,
            created_at: 1_700_000_000,
            evidence: vec![],
            state: EdgeState::Active,
            initial_weight: None,
            decay: None,
            last_reinforced: None,
            reinforce_count: None,
            bidirectional: None,
            context: vec![],
            order: None,
            required: None,
        });
        src.epi.bonds.push(Bond {
            target_cid: cid2.to_vec(),
            relation: RelationType::PartOf,
            weight: 6000,
            creator: Creator::Ai,
            created_at: 1_700_000_100,
            evidence: vec![],
            state: EdgeState::Active,
            initial_weight: None,
            decay: None,
            last_reinforced: None,
            reinforce_count: None,
            bidirectional: None,
            context: vec![],
            order: None,
            required: None,
        });
        let src_cid = storage.put(&src).unwrap();

        // Both bonds should be indexed
        let outgoing = storage.graph().outgoing_bonds(&src_cid).unwrap();
        assert_eq!(outgoing.len(), 2);

        // Filter by type
        let extends = storage
            .graph()
            .outgoing_by_type(&src_cid, RelationType::Extends)
            .unwrap();
        assert_eq!(extends.len(), 1);
        assert_eq!(extends[0].0, cid1);

        let partof = storage
            .graph()
            .outgoing_by_type(&src_cid, RelationType::PartOf)
            .unwrap();
        assert_eq!(partof.len(), 1);
        assert_eq!(partof[0].0, cid2);

        cleanup(&path);
    }

    #[test]
    fn test_put_skips_invalid_bond_cid_length() {
        use ku_core::types::{Bond, Creator, EdgeState, RelationType};

        let path = temp_db_path();
        let storage = KuStorage::open(&path).unwrap();

        // Create a KU with a bond that has invalid CID length (< 32 bytes)
        let mut ku = make_test_ku(7000);
        ku.dna.instructions = vec![
            Instruction::Triple {
                s: 777,
                p: 133,
                o: 132,
            },
            Instruction::Certainty { level: 9500 },
        ];
        ku.recompute();
        ku.epi.bonds.push(Bond {
            target_cid: vec![0xAA; 16], // Too short — should be skipped
            relation: RelationType::Extends,
            weight: 5000,
            creator: Creator::Human,
            created_at: 1_700_000_000,
            evidence: vec![],
            state: EdgeState::Active,
            initial_weight: None,
            decay: None,
            last_reinforced: None,
            reinforce_count: None,
            bidirectional: None,
            context: vec![],
            order: None,
            required: None,
        });
        let cid = storage.put(&ku).unwrap();

        // Bond should NOT be indexed (CID was too short)
        let outgoing = storage.graph().outgoing_bonds(&cid).unwrap();
        assert_eq!(outgoing.len(), 0);

        cleanup(&path);
    }

    #[test]
    fn test_graph_stats_after_puts() {
        use ku_core::types::{Bond, Creator, EdgeState, RelationType};

        let path = temp_db_path();
        let storage = KuStorage::open(&path).unwrap();

        // Create target
        let target = make_test_ku(5000);
        let target_cid = storage.put(&target).unwrap();

        // Create source with bond
        let mut src = make_test_ku(8000);
        src.dna.instructions = vec![
            Instruction::Triple {
                s: 666,
                p: 133,
                o: 132,
            },
            Instruction::Certainty { level: 9500 },
        ];
        src.recompute();
        src.epi.bonds.push(Bond {
            target_cid: target_cid.to_vec(),
            relation: RelationType::Causes,
            weight: 7500,
            creator: Creator::System,
            created_at: 1_700_000_000,
            evidence: vec![],
            state: EdgeState::Active,
            initial_weight: None,
            decay: None,
            last_reinforced: None,
            reinforce_count: None,
            bidirectional: None,
            context: vec![],
            order: None,
            required: None,
        });
        storage.put(&src).unwrap();

        // Graph stats should reflect 1 edge
        let stats = storage.graph().stats().unwrap();
        assert_eq!(stats.total_edges, 1);
        assert_eq!(stats.active_edges, 1);

        cleanup(&path);
    }

    #[test]
    fn test_cid_verification_on_get() {
        let path = temp_db_path();
        let storage = KuStorage::open(&path).unwrap();

        let ku = make_test_ku(7500);
        let cid = storage.put(&ku).unwrap();

        // Verify that get() succeeds with valid CID
        let retrieved = storage.get(&cid).unwrap();
        assert_eq!(retrieved.cid, cid);
        assert_eq!(retrieved.trust_score(), 7500);

        // Verify that get() with a non-existent CID returns NotFound
        let fake_cid = [0xDE; 32];
        assert!(matches!(
            storage.get(&fake_cid),
            Err(StorageError::NotFound)
        ));

        cleanup(&path);
    }

    #[test]
    fn test_put_indexes_concept_edges() {
        use ku_core::core_dna::ConceptTableEntry;
        use ku_core::types::RelationType;

        let path = temp_db_path();
        let storage = KuStorage::open(&path).unwrap();

        // Create CCIDs for concepts
        let ccid_water = ku_core::ccid::ccid(b"wd:Q283");
        let ccid_hydrogen = ku_core::ccid::ccid(b"wd:Q556");

        // Build a KU with concept_table + PartOf instruction
        let water_id: u64 = 16512;
        let hydrogen_id: u64 = 16513;

        let dna = CoreDna {
            header: CoreDnaHeader {
                version: 2,
                gene_type: 0,
                has_concept_table: true,
            },
            concept_table: vec![
                ConceptTableEntry {
                    local_id: water_id,
                    ccid: ccid_water,
                },
                ConceptTableEntry {
                    local_id: hydrogen_id,
                    ccid: ccid_hydrogen,
                },
            ],
            instructions: vec![
                Instruction::PartOf {
                    part: hydrogen_id,
                    whole: water_id,
                },
                Instruction::Certainty { level: 9000 },
            ],
        };

        let mut ku = KuRuntime::from_dna(dna).expect("create KuRuntime");
        ku.epi = Epigenetics::with_trust(8000, 8000);

        // Verify concept_graph_edges returns 1 edge before put
        assert_eq!(ku.concept_graph_edges().len(), 1);

        // Put the KU — should index the concept edge into GraphStorage
        let _cid = storage.put(&ku).unwrap();

        // Graph should have 1 concept edge: hydrogen -[PartOf]→ water
        let stats = storage.graph().stats().unwrap();
        assert_eq!(stats.total_edges, 1, "Expected 1 concept edge");

        // Query the edge: hydrogen → water should exist
        let mut hydrogen_padded = [0u8; 32];
        hydrogen_padded[..16].copy_from_slice(&ccid_hydrogen);
        let outgoing = storage.graph().outgoing_bonds(&hydrogen_padded).unwrap();
        assert_eq!(outgoing.len(), 1, "Expected 1 outgoing edge from hydrogen");
        // outgoing_bonds returns (RelationType, [u8; 32], BondMeta)
        assert_eq!(outgoing[0].0, RelationType::PartOf);

        // Target should be water (padded)
        let mut water_padded = [0u8; 32];
        water_padded[..16].copy_from_slice(&ccid_water);
        assert_eq!(outgoing[0].1, water_padded);

        cleanup(&path);
    }

    // ─── Task 12d: Integration tests for graph operations ───────────────

    /// Helper: create a KU with concept_table + instructions for integration tests.
    fn make_concept_ku(
        concepts: Vec<(u64, [u8; 16])>,
        instructions: Vec<Instruction>,
        trust: u16,
    ) -> KuRuntime {
        use ku_core::core_dna::ConceptTableEntry;

        let dna = CoreDna {
            header: CoreDnaHeader {
                version: 2,
                gene_type: 0,
                has_concept_table: !concepts.is_empty(),
            },
            concept_table: concepts
                .iter()
                .map(|(id, ccid)| ConceptTableEntry {
                    local_id: *id,
                    ccid: *ccid,
                })
                .collect(),
            instructions,
        };
        let mut ku = KuRuntime::from_dna(dna).expect("create KuRuntime");
        ku.epi = Epigenetics::with_trust(trust, 8000);
        ku
    }

    #[test]
    fn test_multi_ku_shared_concepts() {
        // Two KUs share concept "water" — both contribute edges to the graph
        use ku_core::types::RelationType;

        let path = temp_db_path();
        let storage = KuStorage::open(&path).unwrap();

        let ccid_water = ku_core::ccid::ccid(b"wd:Q283");
        let ccid_hydrogen = ku_core::ccid::ccid(b"wd:Q556");
        let ccid_molecule = ku_core::ccid::ccid(b"wd:Q11369");
        let ccid_liquid = ku_core::ccid::ccid(b"wd:Q11435");

        let w: u64 = 16512;
        let h: u64 = 16513;
        let m: u64 = 16514;
        let l: u64 = 16515;

        // KU1: hydrogen -[PartOf]→ water, water -[Extends]→ molecule
        let ku1 = make_concept_ku(
            vec![(w, ccid_water), (h, ccid_hydrogen), (m, ccid_molecule)],
            vec![
                Instruction::PartOf { part: h, whole: w },
                Instruction::Triple { s: w, p: 100, o: m },
                Instruction::Certainty { level: 9000 },
            ],
            8000,
        );
        storage.put(&ku1).unwrap();

        // KU2: water -[Qualifies]→ liquid (different KU, same "water" concept)
        let ku2 = make_concept_ku(
            vec![(w, ccid_water), (l, ccid_liquid)],
            vec![
                Instruction::Quality { s: w, q: l },
                Instruction::Certainty { level: 8500 },
            ],
            7500,
        );
        storage.put(&ku2).unwrap();

        // Graph should have 3 edges total: PartOf + Extends + Qualifies
        let stats = storage.graph().stats().unwrap();
        assert_eq!(stats.total_edges, 3, "Expected 3 edges from 2 KUs");

        // Water should have 2 outgoing edges (Extends→molecule, Qualifies→liquid)
        let water_out = storage.concept_relations_outgoing(&ccid_water).unwrap();
        assert_eq!(water_out.len(), 2, "Water should have 2 outgoing");

        // Water should have 1 incoming edge (hydrogen→PartOf→water)
        let water_in = storage.concept_relations_incoming(&ccid_water).unwrap();
        assert_eq!(water_in.len(), 1, "Water should have 1 incoming");
        assert_eq!(water_in[0].relation, RelationType::PartOf);
        assert_eq!(water_in[0].ccid, ccid_hydrogen);

        cleanup(&path);
    }

    #[test]
    fn test_find_kus_by_concept_integration() {
        // Put 3 KUs, 2 contain "water" → find_kus_by_concept should return 2
        let path = temp_db_path();
        let storage = KuStorage::open(&path).unwrap();

        let ccid_water = ku_core::ccid::ccid(b"wd:Q283");
        let ccid_fire = ku_core::ccid::ccid(b"wd:Q3196");
        let ccid_steam = ku_core::ccid::ccid(b"wd:Q5765");

        let w: u64 = 16512;
        let f: u64 = 16513;
        let s: u64 = 16514;

        // KU1: water -[Causes]→ steam
        let ku1 = make_concept_ku(
            vec![(w, ccid_water), (s, ccid_steam)],
            vec![Instruction::Causal {
                cause: w,
                effect: s,
            }],
            8000,
        );
        let cid1 = storage.put(&ku1).unwrap();

        // KU2: fire (no water)
        let ku2 = make_concept_ku(
            vec![(f, ccid_fire)],
            vec![Instruction::Certainty { level: 7000 }],
            7000,
        );
        let _cid2 = storage.put(&ku2).unwrap();

        // KU3: water -[Quality]→ steam (another KU with water)
        let ku3 = make_concept_ku(
            vec![(w, ccid_water), (s, ccid_steam)],
            vec![Instruction::Quality { s: w, q: s }],
            9000,
        );
        let cid3 = storage.put(&ku3).unwrap();

        // Find KUs containing water
        let water_kus = storage.find_kus_by_concept(&ccid_water).unwrap();
        assert_eq!(water_kus.len(), 2, "Expected 2 KUs with water");
        assert!(water_kus.contains(&cid1));
        assert!(water_kus.contains(&cid3));

        // Find KUs containing fire
        let fire_kus = storage.find_kus_by_concept(&ccid_fire).unwrap();
        assert_eq!(fire_kus.len(), 1, "Expected 1 KU with fire");

        // Find KUs containing nonexistent concept
        let missing_ccid = ku_core::ccid::ccid(b"wd:Q999999");
        let missing = storage.find_kus_by_concept(&missing_ccid).unwrap();
        assert!(missing.is_empty(), "Expected 0 KUs for nonexistent concept");

        cleanup(&path);
    }

    #[test]
    fn test_concept_neighbors_via_kustorage() {
        // Build concept chain: A→B→C via KuStorage convenience methods
        use ku_core::types::RelationType;

        let path = temp_db_path();
        let storage = KuStorage::open(&path).unwrap();

        let ccid_a = ku_core::ccid::ccid(b"concept:A");
        let ccid_b = ku_core::ccid::ccid(b"concept:B");
        let ccid_c = ku_core::ccid::ccid(b"concept:C");

        let a: u64 = 16512;
        let b: u64 = 16513;
        let c: u64 = 16514;

        // KU1: A -[Extends]→ B
        let ku1 = make_concept_ku(
            vec![(a, ccid_a), (b, ccid_b)],
            vec![Instruction::Triple { s: a, p: 100, o: b }],
            8000,
        );
        storage.put(&ku1).unwrap();

        // KU2: B -[Extends]→ C
        let ku2 = make_concept_ku(
            vec![(b, ccid_b), (c, ccid_c)],
            vec![Instruction::Triple { s: b, p: 100, o: c }],
            8000,
        );
        storage.put(&ku2).unwrap();

        // Depth 1 from A → only B
        let n1 = storage.concept_neighbors(&ccid_a, 1, None).unwrap();
        assert_eq!(n1.len(), 1);
        assert_eq!(n1[0].0.ccid, ccid_b);
        assert_eq!(n1[0].1, 1);

        // Depth 2 from A → B + C
        let n2 = storage.concept_neighbors(&ccid_a, 2, None).unwrap();
        assert_eq!(n2.len(), 2);
        assert_eq!(n2[1].0.ccid, ccid_c);
        assert_eq!(n2[1].1, 2);

        // Filter by Extends
        let ext = storage
            .concept_neighbors(&ccid_a, 2, Some(RelationType::Extends))
            .unwrap();
        assert_eq!(ext.len(), 2, "Both hops are Extends");

        // Filter by Causes → empty (no Causes edges)
        let cau = storage
            .concept_neighbors(&ccid_a, 2, Some(RelationType::Causes))
            .unwrap();
        assert!(cau.is_empty());

        cleanup(&path);
    }

    #[test]
    fn test_mixed_bonds_and_concept_edges() {
        // A KU with BOTH epigenetic bonds AND concept_table edges
        use ku_core::types::{Bond, Creator, EdgeState, RelationType};

        let path = temp_db_path();
        let storage = KuStorage::open(&path).unwrap();

        // Create a target KU for the bond
        let target_ku = make_test_ku(5000);
        let target_cid = storage.put(&target_ku).unwrap();

        let ccid_water = ku_core::ccid::ccid(b"wd:Q283");
        let ccid_hydrogen = ku_core::ccid::ccid(b"wd:Q556");
        let w: u64 = 16512;
        let h: u64 = 16513;

        // Create a KU with concept_table + a bond
        let mut ku = make_concept_ku(
            vec![(w, ccid_water), (h, ccid_hydrogen)],
            vec![
                Instruction::PartOf { part: h, whole: w },
                Instruction::Certainty { level: 9000 },
            ],
            8000,
        );

        // Add an epigenetic bond to another KU
        ku.epi.bonds.push(Bond {
            target_cid: target_cid.to_vec(),
            relation: RelationType::Extends,
            weight: 7000,
            creator: Creator::Human,
            created_at: 1_700_000_000,
            evidence: vec![],
            state: EdgeState::Active,
            initial_weight: None,
            decay: None,
            last_reinforced: None,
            reinforce_count: None,
            bidirectional: None,
            context: vec![],
            order: None,
            required: None,
        });

        let ku_cid = storage.put(&ku).unwrap();

        // Graph should have 2 edges: 1 concept edge + 1 bond edge
        let stats = storage.graph().stats().unwrap();
        assert_eq!(
            stats.total_edges, 2,
            "Expected 2 edges (1 concept + 1 bond)"
        );

        // Bond edge: ku_cid -[Extends]→ target_cid
        let ku_out = storage.graph().outgoing_bonds(&ku_cid).unwrap();
        assert_eq!(ku_out.len(), 1);
        assert_eq!(ku_out[0].0, RelationType::Extends);
        assert_eq!(ku_out[0].1, target_cid);

        // Concept edge: hydrogen -[PartOf]→ water (via padded CCIDs)
        let h_out = storage.graph().concept_outgoing(&ccid_hydrogen).unwrap();
        assert_eq!(h_out.len(), 1);
        assert_eq!(h_out[0].relation, RelationType::PartOf);
        assert_eq!(h_out[0].ccid, ccid_water);

        cleanup(&path);
    }

    #[test]
    fn test_multi_instruction_diverse_edges() {
        // A single KU with many different instruction types → verify diverse edge types
        use ku_core::types::RelationType;

        let path = temp_db_path();
        let storage = KuStorage::open(&path).unwrap();

        let ccid_a = ku_core::ccid::ccid(b"concept:alpha");
        let ccid_b = ku_core::ccid::ccid(b"concept:beta");
        let ccid_c = ku_core::ccid::ccid(b"concept:gamma");
        let ccid_d = ku_core::ccid::ccid(b"concept:delta");
        let ccid_e = ku_core::ccid::ccid(b"concept:epsilon");
        let ccid_f = ku_core::ccid::ccid(b"concept:zeta");

        let a: u64 = 16512;
        let b: u64 = 16513;
        let c: u64 = 16514;
        let d: u64 = 16515;
        let e: u64 = 16516;
        let f: u64 = 16517;

        let ku = make_concept_ku(
            vec![
                (a, ccid_a),
                (b, ccid_b),
                (c, ccid_c),
                (d, ccid_d),
                (e, ccid_e),
                (f, ccid_f),
            ],
            vec![
                Instruction::Triple { s: a, p: 100, o: b }, // a-[Extends]→b
                Instruction::PartOf { part: b, whole: c },  // b-[PartOf]→c
                Instruction::Causal {
                    cause: c,
                    effect: d,
                }, // c-[Causes]→d
                Instruction::Agent {
                    actor: d,
                    action: e,
                }, // d-[Enables]→e
                Instruction::Simulates { s: e, model: f },  // e-[AnalogyOf]→f
                Instruction::Certainty { level: 9500 },
            ],
            9000,
        );
        storage.put(&ku).unwrap();

        // 5 edges total
        let stats = storage.graph().stats().unwrap();
        assert_eq!(stats.total_edges, 5, "Expected 5 diverse edges");

        // Verify each edge type
        let a_out = storage.graph().concept_outgoing(&ccid_a).unwrap();
        assert_eq!(a_out.len(), 1);
        assert_eq!(a_out[0].relation, RelationType::Extends);
        assert_eq!(a_out[0].ccid, ccid_b);

        let b_out = storage.graph().concept_outgoing(&ccid_b).unwrap();
        assert_eq!(b_out.len(), 1);
        assert_eq!(b_out[0].relation, RelationType::PartOf);

        let c_out = storage.graph().concept_outgoing(&ccid_c).unwrap();
        assert_eq!(c_out.len(), 1);
        assert_eq!(c_out[0].relation, RelationType::Causes);

        let d_out = storage.graph().concept_outgoing(&ccid_d).unwrap();
        assert_eq!(d_out.len(), 1);
        assert_eq!(d_out[0].relation, RelationType::Enables);

        let e_out = storage.graph().concept_outgoing(&ccid_e).unwrap();
        assert_eq!(e_out.len(), 1);
        assert_eq!(e_out[0].relation, RelationType::AnalogyOf);
        assert_eq!(e_out[0].ccid, ccid_f);

        // BFS from a, depth 5 → should reach all 5 concepts (b,c,d,e,f)
        let neighbors = storage.concept_neighbors(&ccid_a, 5, None).unwrap();
        assert_eq!(neighbors.len(), 5, "Chain of 5 hops from alpha");

        cleanup(&path);
    }
}
