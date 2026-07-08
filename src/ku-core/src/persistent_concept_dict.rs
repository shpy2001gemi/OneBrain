//! PersistentConceptDict — redb-backed persistent ConceptDict.
//!
//! Pure Rust storage (no C compiler needed). Uses redb for ACID transactions.
//! Provides the same API as in-memory `ConceptDict` but persists across sessions.
//!
//! # Storage Layout
//! | Table        | Key        | Value                |
//! |-------------|------------|----------------------|
//! | `concepts`  | name (str) | JSON ConceptEntry    |
//! | `ids`       | id (u64)   | name (str)           |
//! | `meta`      | "next_id"  | u64                  |

use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
use crate::obs_schema;
use crate::types::ConceptId;
use crate::error::KuError;
use crate::concept_dict::ConceptEntry;
use std::path::Path;

// Table definitions
const CONCEPTS: TableDefinition<&str, &str> = TableDefinition::new("concepts");
const IDS: TableDefinition<u64, &str> = TableDefinition::new("ids");
const META: TableDefinition<&str, u64> = TableDefinition::new("meta");

// ============================================================================
// PersistentConceptDict
// ============================================================================

/// redb-backed persistent concept dictionary.
pub struct PersistentConceptDict {
    db: Database,
}

impl PersistentConceptDict {
    /// Open or create a persistent concept dictionary.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, KuError> {
        let db = Database::create(path)
            .map_err(|e| KuError::InvalidData(format!("DB open error: {}", e)))?;

        // Initialize/validate schema version
        obs_schema::redb_schema::ensure_schema(&db, &obs_schema::concept_dict_registry())
            .map_err(|e| KuError::InvalidData(format!("Schema init failed: {}", e)))?;

        let dict = Self { db };
        dict.init_tables()?;
        Ok(dict)
    }

    /// Initialize tables if they don't exist.
    fn init_tables(&self) -> Result<(), KuError> {
        let txn = self.db.begin_write()
            .map_err(|e| KuError::InvalidData(format!("Txn error: {}", e)))?;
        {
            let _ = txn.open_table(CONCEPTS);
            let _ = txn.open_table(IDS);
            let mut meta = txn.open_table(META)
                .map_err(|e| KuError::InvalidData(format!("Meta table error: {}", e)))?;
            // Set next_id if not present
            if meta.get("next_id").map_err(|e| KuError::InvalidData(format!("{}", e)))?.is_none() {
                meta.insert("next_id", 128u64)
                    .map_err(|e| KuError::InvalidData(format!("{}", e)))?;
            }
        }
        txn.commit().map_err(|e| KuError::InvalidData(format!("Commit error: {}", e)))?;
        Ok(())
    }

    // ────────────────────────────────────────────────────────────────────
    // Lookup
    // ────────────────────────────────────────────────────────────────────

    /// Resolve a text name to a ConceptId (case-insensitive).
    pub fn resolve(&self, name: &str) -> Result<ConceptId, KuError> {
        self.try_resolve(name)
            .ok_or_else(|| KuError::InvalidData(format!("Concept not found: '{}'", name)))
    }

    /// Try to resolve, returning None instead of error.
    pub fn try_resolve(&self, name: &str) -> Option<ConceptId> {
        let key = name.to_lowercase();
        let txn = self.db.begin_read().ok()?;
        let table = txn.open_table(CONCEPTS).ok()?;
        let entry_json = table.get(key.as_str()).ok()??;
        let entry: ConceptEntry = serde_json::from_str(entry_json.value()).ok()?;
        Some(entry.id)
    }

    /// Get the canonical name for a ConceptId.
    pub fn name(&self, id: ConceptId) -> Option<String> {
        let txn = self.db.begin_read().ok()?;
        let table = txn.open_table(IDS).ok()?;
        let name_guard = table.get(id).ok()??;
        Some(name_guard.value().to_string())
    }

    /// Get name in a specific language.
    pub fn name_lang(&self, id: ConceptId, lang: &str) -> Option<String> {
        let txn = self.db.begin_read().ok()?;
        let ids = txn.open_table(IDS).ok()?;
        let name_guard = ids.get(id).ok()??;
        let canonical = name_guard.value().to_string();
        drop(name_guard);

        let concepts = txn.open_table(CONCEPTS).ok()?;
        let entry_guard = concepts.get(canonical.to_lowercase().as_str()).ok()??;
        let entry: ConceptEntry = serde_json::from_str(entry_guard.value()).ok()?;

        match lang {
            "vi" => entry.name_vi.or(Some(entry.name)),
            "en" => entry.name_en.or(Some(entry.name)),
            _ => Some(entry.name),
        }
    }

    // ────────────────────────────────────────────────────────────────────
    // Registration
    // ────────────────────────────────────────────────────────────────────

    /// Register a new concept and return its assigned ConceptId.
    pub fn register(&self, name: &str) -> Result<ConceptId, KuError> {
        // Check if already exists
        if let Some(id) = self.try_resolve(name) {
            return Ok(id);
        }

        let txn = self.db.begin_write()
            .map_err(|e| KuError::InvalidData(format!("Txn error: {}", e)))?;

        let id;
        {
            // Get and increment next_id
            let mut meta = txn.open_table(META)
                .map_err(|e| KuError::InvalidData(format!("{}", e)))?;
            id = meta.get("next_id")
                .map_err(|e| KuError::InvalidData(format!("{}", e)))?
                .map(|g| g.value())
                .unwrap_or(128);
            meta.insert("next_id", id + 1)
                .map_err(|e| KuError::InvalidData(format!("{}", e)))?;

            let entry = ConceptEntry {
                id,
                name: name.to_string(),
                name_vi: None,
                name_en: None,
                tier: Self::tier_for_id(id),
                category: None,
            };
            let json = serde_json::to_string(&entry)
                .map_err(|e| KuError::InvalidData(format!("JSON error: {}", e)))?;

            let mut concepts = txn.open_table(CONCEPTS)
                .map_err(|e| KuError::InvalidData(format!("{}", e)))?;
            concepts.insert(name.to_lowercase().as_str(), json.as_str())
                .map_err(|e| KuError::InvalidData(format!("{}", e)))?;

            let mut ids = txn.open_table(IDS)
                .map_err(|e| KuError::InvalidData(format!("{}", e)))?;
            ids.insert(id, name)
                .map_err(|e| KuError::InvalidData(format!("{}", e)))?;
        }

        txn.commit().map_err(|e| KuError::InvalidData(format!("{}", e)))?;
        Ok(id)
    }

    /// Register with multilingual names.
    pub fn register_multilingual(
        &self,
        name: &str,
        name_vi: Option<&str>,
        name_en: Option<&str>,
    ) -> Result<ConceptId, KuError> {
        if let Some(id) = self.try_resolve(name) {
            return Ok(id);
        }

        let txn = self.db.begin_write()
            .map_err(|e| KuError::InvalidData(format!("Txn error: {}", e)))?;

        let id;
        {
            let mut meta = txn.open_table(META)
                .map_err(|e| KuError::InvalidData(format!("{}", e)))?;
            id = meta.get("next_id")
                .map_err(|e| KuError::InvalidData(format!("{}", e)))?
                .map(|g| g.value())
                .unwrap_or(128);
            meta.insert("next_id", id + 1)
                .map_err(|e| KuError::InvalidData(format!("{}", e)))?;

            let entry = ConceptEntry {
                id,
                name: name.to_string(),
                name_vi: name_vi.map(|s| s.to_string()),
                name_en: name_en.map(|s| s.to_string()),
                tier: Self::tier_for_id(id),
                category: None,
            };
            let json = serde_json::to_string(&entry)
                .map_err(|e| KuError::InvalidData(format!("JSON error: {}", e)))?;

            let mut concepts = txn.open_table(CONCEPTS)
                .map_err(|e| KuError::InvalidData(format!("{}", e)))?;
            // Index by all name variants
            concepts.insert(name.to_lowercase().as_str(), json.as_str())
                .map_err(|e| KuError::InvalidData(format!("{}", e)))?;
            if let Some(vi) = name_vi {
                concepts.insert(vi.to_lowercase().as_str(), json.as_str())
                    .map_err(|e| KuError::InvalidData(format!("{}", e)))?;
            }
            if let Some(en) = name_en {
                concepts.insert(en.to_lowercase().as_str(), json.as_str())
                    .map_err(|e| KuError::InvalidData(format!("{}", e)))?;
            }

            let mut ids = txn.open_table(IDS)
                .map_err(|e| KuError::InvalidData(format!("{}", e)))?;
            ids.insert(id, name)
                .map_err(|e| KuError::InvalidData(format!("{}", e)))?;
        }

        txn.commit().map_err(|e| KuError::InvalidData(format!("{}", e)))?;
        Ok(id)
    }

    /// Resolve or auto-register a concept.
    pub fn resolve_or_register(&self, name: &str) -> Result<ConceptId, KuError> {
        match self.try_resolve(name) {
            Some(id) => Ok(id),
            None => self.register(name),
        }
    }

    // ────────────────────────────────────────────────────────────────────
    // Batch Operations
    // ────────────────────────────────────────────────────────────────────

    /// Bulk insert entries (for seed data loading).
    pub fn bulk_insert(&self, entries: &[ConceptEntry]) -> Result<usize, KuError> {
        let txn = self.db.begin_write()
            .map_err(|e| KuError::InvalidData(format!("Txn error: {}", e)))?;

        let count;
        {
            let mut concepts = txn.open_table(CONCEPTS)
                .map_err(|e| KuError::InvalidData(format!("{}", e)))?;
            let mut ids = txn.open_table(IDS)
                .map_err(|e| KuError::InvalidData(format!("{}", e)))?;
            let mut meta = txn.open_table(META)
                .map_err(|e| KuError::InvalidData(format!("{}", e)))?;

            let mut max_id = 127u64;
            count = entries.len();

            for entry in entries {
                let json = serde_json::to_string(entry)
                    .map_err(|e| KuError::InvalidData(format!("JSON error: {}", e)))?;
                concepts.insert(entry.name.to_lowercase().as_str(), json.as_str())
                    .map_err(|e| KuError::InvalidData(format!("{}", e)))?;
                ids.insert(entry.id, entry.name.as_str())
                    .map_err(|e| KuError::InvalidData(format!("{}", e)))?;

                if entry.id > max_id {
                    max_id = entry.id;
                }
            }

            meta.insert("next_id", max_id + 1)
                .map_err(|e| KuError::InvalidData(format!("{}", e)))?;
        }

        txn.commit().map_err(|e| KuError::InvalidData(format!("{}", e)))?;
        Ok(count)
    }

    /// Number of concepts (by ID count).
    pub fn len(&self) -> Result<usize, KuError> {
        let txn = self.db.begin_read()
            .map_err(|e| KuError::InvalidData(format!("{}", e)))?;
        let ids = txn.open_table(IDS)
            .map_err(|e| KuError::InvalidData(format!("{}", e)))?;
        ids.len().map(|n| n as usize)
            .map_err(|e| KuError::InvalidData(format!("{}", e)))
    }

    /// Whether the dictionary is empty.
    pub fn is_empty(&self) -> Result<bool, KuError> {
        Ok(self.len()? == 0)
    }

    /// Determine varint tier for a given ID.
    fn tier_for_id(id: ConceptId) -> u8 {
        match id {
            0..=127 => 0,
            128..=16_383 => 1,
            16_384..=2_097_151 => 2,
            2_097_152..=268_435_455 => 3,
            _ => 4,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_CTR: AtomicU64 = AtomicU64::new(0);

    fn temp_db_path() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("ku_concept_test");
        std::fs::create_dir_all(&dir).ok();
        let id = TEST_CTR.fetch_add(1, Ordering::SeqCst);
        dir.join(format!("test_{}_{}.redb", std::process::id(), id))
    }

    fn cleanup(path: &std::path::Path) {
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn test_register_and_resolve() {
        let path = temp_db_path();
        let dict = PersistentConceptDict::open(&path).unwrap();

        let id = dict.register("water").unwrap();
        assert!(id >= 128);

        // Re-register returns same ID
        let id2 = dict.register("water").unwrap();
        assert_eq!(id, id2);

        // Resolve works
        assert_eq!(dict.resolve("water").unwrap(), id);

        // Case-insensitive
        assert_eq!(dict.try_resolve("WATER"), Some(id));

        cleanup(&path);
    }

    #[test]
    fn test_multilingual() {
        let path = temp_db_path();
        let dict = PersistentConceptDict::open(&path).unwrap();

        let id = dict.register_multilingual("water", Some("nước"), Some("water")).unwrap();

        // Resolve by Vietnamese
        assert_eq!(dict.try_resolve("nước"), Some(id));

        // Name by lang
        assert_eq!(dict.name_lang(id, "vi").unwrap(), "nước");

        cleanup(&path);
    }

    #[test]
    fn test_resolve_or_register() {
        let path = temp_db_path();
        let dict = PersistentConceptDict::open(&path).unwrap();

        let id1 = dict.resolve_or_register("alpha").unwrap();
        let id2 = dict.resolve_or_register("alpha").unwrap();
        let id3 = dict.resolve_or_register("beta").unwrap();

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);

        cleanup(&path);
    }

    #[test]
    fn test_bulk_insert() {
        let path = temp_db_path();
        let dict = PersistentConceptDict::open(&path).unwrap();

        let entries = vec![
            ConceptEntry { id: 200, name: "alpha".into(), name_vi: None, name_en: None, tier: 1, category: None },
            ConceptEntry { id: 201, name: "beta".into(), name_vi: None, name_en: None, tier: 1, category: None },
        ];

        dict.bulk_insert(&entries).unwrap();
        assert_eq!(dict.len().unwrap(), 2);
        assert_eq!(dict.resolve("alpha").unwrap(), 200);
        assert_eq!(dict.name(201).unwrap(), "beta");

        cleanup(&path);
    }

    #[test]
    fn test_persistence_across_reopen() {
        let path = temp_db_path();

        // Write
        {
            let dict = PersistentConceptDict::open(&path).unwrap();
            dict.register("persistent_concept").unwrap();
        }

        // Reopen and read
        {
            let dict = PersistentConceptDict::open(&path).unwrap();
            assert!(dict.try_resolve("persistent_concept").is_some());
        }

        cleanup(&path);
    }

    #[test]
    fn test_next_id_continuity() {
        let path = temp_db_path();
        let dict = PersistentConceptDict::open(&path).unwrap();

        let id1 = dict.register("first").unwrap();
        let id2 = dict.register("second").unwrap();
        assert_eq!(id2, id1 + 1);

        cleanup(&path);
    }
}
