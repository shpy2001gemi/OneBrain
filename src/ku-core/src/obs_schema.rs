//! OBS Schema Versioning — Storage migration framework for OneBrain.
//!
//! Provides schema versioning and sequential migration support for all redb databases.
//! Each database gets a `_schema_meta` table tracking the current schema version.
//! Migrations run automatically on `open()`, executing any pending version upgrades
//! in order within a single write transaction.
//!
//! # Design (from Research Topic 4)
//! - Follows SQLite PRAGMA user_version pattern adapted for KV stores
//! - Never migrates Core DNA wire bytes (CID = hash, immutable)
//! - Sequential version increments with rollback-safe transactions
//! - Each storage module registers its own migration chain

use std::fmt;

/// Schema version metadata table name (shared across all redb databases).
pub const SCHEMA_META_TABLE: &str = "_schema_meta";

/// Key for the schema version entry.
pub const VERSION_KEY: &str = "version";

/// Key for the schema name entry (identifies which module owns this DB).
pub const NAME_KEY: &str = "schema_name";

/// Key for the last migration timestamp.
pub const UPDATED_KEY: &str = "updated_at";

// ============================================================================
// Schema Version
// ============================================================================

/// Current schema version for a database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SchemaVersion(pub u32);

impl SchemaVersion {
    pub fn new(v: u32) -> Self {
        Self(v)
    }

    pub fn as_u32(self) -> u32 {
        self.0
    }
}

impl fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}", self.0)
    }
}

// ============================================================================
// Migration Definition
// ============================================================================

/// A single schema migration step.
///
/// Each migration upgrades the database from `from_version` to `from_version + 1`.
/// The `description` field is for logging/debugging.
pub struct Migration {
    /// Version this migration upgrades FROM (target = from_version + 1).
    pub from_version: u32,
    /// Human-readable description of what this migration does.
    pub description: &'static str,
}

impl Migration {
    pub fn new(from_version: u32, description: &'static str) -> Self {
        Self {
            from_version,
            description,
        }
    }

    /// Target version after this migration completes.
    pub fn target_version(&self) -> SchemaVersion {
        SchemaVersion(self.from_version + 1)
    }
}

// ============================================================================
// Migration Registry
// ============================================================================

/// Registry of migrations for a specific storage module.
///
/// # Usage
/// ```ignore
/// let registry = MigrationRegistry::new("ku_storage", 1)
///     .with_migration(Migration::new(0, "Initial schema — 4 tables"));
/// ```
pub struct MigrationRegistry {
    /// Name of the storage module (e.g., "ku_storage", "graph_storage").
    pub schema_name: &'static str,
    /// Current (latest) schema version.
    pub current_version: SchemaVersion,
    /// Ordered list of migrations.
    pub migrations: Vec<Migration>,
}

impl MigrationRegistry {
    /// Create a new registry for a storage module.
    pub fn new(schema_name: &'static str, current_version: u32) -> Self {
        Self {
            schema_name,
            current_version: SchemaVersion(current_version),
            migrations: Vec::new(),
        }
    }

    /// Add a migration step.
    pub fn with_migration(mut self, migration: Migration) -> Self {
        self.migrations.push(migration);
        self
    }

    /// Get migrations that need to run to upgrade from `from_version` to current.
    pub fn pending_migrations(&self, from_version: SchemaVersion) -> Vec<&Migration> {
        self.migrations
            .iter()
            .filter(|m| m.from_version >= from_version.as_u32())
            .collect()
    }

    /// Check if a database needs migration.
    pub fn needs_migration(&self, db_version: SchemaVersion) -> bool {
        db_version < self.current_version
    }

    /// Validate that migrations form a contiguous chain from 0 to current_version.
    pub fn validate(&self) -> Result<(), String> {
        if self.current_version.as_u32() == 0 && self.migrations.is_empty() {
            return Ok(());
        }

        let mut versions: Vec<u32> = self.migrations.iter().map(|m| m.from_version).collect();
        versions.sort();
        versions.dedup();

        for (i, &v) in versions.iter().enumerate() {
            if v != i as u32 {
                return Err(format!(
                    "Migration chain broken: expected from_version={}, found={}",
                    i, v
                ));
            }
        }

        let max_target = versions.last().map(|v| v + 1).unwrap_or(0);
        if max_target != self.current_version.as_u32() {
            return Err(format!(
                "Migration chain incomplete: max target={}, current_version={}",
                max_target, self.current_version
            ));
        }

        Ok(())
    }
}

// ============================================================================
// redb Integration (behind `persist` feature)
// ============================================================================

#[cfg(feature = "persist")]
pub mod redb_schema {
    use super::*;
    use redb::{Database, TableDefinition};

    /// The `_schema_meta` table definition for redb.
    const SCHEMA_META: TableDefinition<&str, &str> = TableDefinition::new("_schema_meta");

    /// A migration step with an optional data-transform function (redb-specific).
    ///
    /// Unlike `Migration` (which is feature-agnostic), `RedbMigration` can carry
    /// a concrete `fn(&Database) -> Result<(), String>` that runs inside the
    /// upgrade transaction.
    pub struct RedbMigration {
        /// Version this migration upgrades FROM (target = from_version + 1).
        pub from_version: u32,
        /// Human-readable description.
        pub description: &'static str,
        /// Optional data-transform function.  `None` for schema-only changes
        /// (e.g. initial v0→v1 where tables are created by the storage module).
        pub migrate_fn: Option<fn(&Database) -> Result<(), String>>,
    }

    impl RedbMigration {
        pub fn new(
            from_version: u32,
            description: &'static str,
            migrate_fn: Option<fn(&Database) -> Result<(), String>>,
        ) -> Self {
            Self {
                from_version,
                description,
                migrate_fn,
            }
        }

        /// Target version after this migration completes.
        pub fn target_version(&self) -> SchemaVersion {
            SchemaVersion(self.from_version + 1)
        }
    }

    /// Read the current schema version from a redb database.
    /// Returns `SchemaVersion(0)` if the table or key doesn't exist (fresh DB).
    pub fn read_version(db: &Database) -> SchemaVersion {
        let txn = match db.begin_read() {
            Ok(t) => t,
            Err(_) => return SchemaVersion(0),
        };
        let table = match txn.open_table(SCHEMA_META) {
            Ok(t) => t,
            Err(_) => return SchemaVersion(0),
        };
        match table.get(VERSION_KEY) {
            Ok(Some(v)) => {
                let val = v.value();
                SchemaVersion(val.parse::<u32>().unwrap_or(0))
            }
            _ => SchemaVersion(0),
        }
    }

    /// Write schema version and metadata to the `_schema_meta` table.
    /// Called after successful migration or on initial database creation.
    pub fn write_version(db: &Database, registry: &MigrationRegistry) -> Result<(), String> {
        let txn = db
            .begin_write()
            .map_err(|e| format!("begin_write: {}", e))?;
        {
            let mut table = txn
                .open_table(SCHEMA_META)
                .map_err(|e| format!("open _schema_meta: {}", e))?;
            table
                .insert(
                    VERSION_KEY,
                    registry.current_version.as_u32().to_string().as_str(),
                )
                .map_err(|e| format!("insert version: {}", e))?;
            table
                .insert(NAME_KEY, registry.schema_name)
                .map_err(|e| format!("insert name: {}", e))?;
        }
        txn.commit().map_err(|e| format!("commit: {}", e))?;
        Ok(())
    }

    /// Initialize schema meta on a fresh database, or verify+upgrade an existing one.
    ///
    /// This is the main entry point — call from `Storage::open()`.
    /// - Fresh DB → writes initial version
    /// - Existing DB → checks version, runs pending migrations
    pub fn ensure_schema(db: &Database, registry: &MigrationRegistry) -> Result<(), String> {
        ensure_schema_with_redb_migrations(db, registry, &[])
    }

    /// Like `ensure_schema`, but also runs `RedbMigration` data-transform functions
    /// for any pending version steps.
    ///
    /// `redb_migrations` should be sorted by `from_version`.  For each pending
    /// step, if a matching `RedbMigration` with a `Some(migrate_fn)` is found,
    /// the function is called before advancing the stored version.
    pub fn ensure_schema_with_redb_migrations(
        db: &Database,
        registry: &MigrationRegistry,
        redb_migrations: &[RedbMigration],
    ) -> Result<(), String> {
        // Validate migration chain
        registry.validate()?;

        let current = read_version(db);

        if current == SchemaVersion(0) {
            // Fresh database — write initial version
            write_version(db, registry)?;
            return Ok(());
        }

        if current == registry.current_version {
            // Up to date — nothing to do
            return Ok(());
        }

        if current > registry.current_version {
            return Err(format!(
                "Database {} is at {}, but code only supports {}. Downgrade not supported.",
                registry.schema_name, current, registry.current_version
            ));
        }

        // Run pending migrations in order
        let pending = registry.pending_migrations(current);
        for m in &pending {
            // Check if there's a matching RedbMigration with a data-transform fn
            if let Some(redb_m) = redb_migrations
                .iter()
                .find(|rm| rm.from_version == m.from_version)
            {
                if let Some(migrate_fn) = redb_m.migrate_fn {
                    migrate_fn(db).map_err(|e| {
                        format!(
                            "Migration {} failed at v{}→v{}: {}",
                            registry.schema_name,
                            m.from_version,
                            m.from_version + 1,
                            e
                        )
                    })?;
                }
            }
        }

        write_version(db, registry)?;
        Ok(())
    }
}

// ============================================================================
// Standard registries for each storage module
// ============================================================================

/// Schema registry for KuStorage (ku-kql/storage.rs).
pub fn ku_storage_registry() -> MigrationRegistry {
    MigrationRegistry::new("ku_storage", 1).with_migration(Migration::new(
        0,
        "Initial: kus, epigenetics, index_trust, index_concept tables",
    ))
}

/// Schema registry for GraphStorage (ku-kql/graph_storage.rs).
pub fn graph_storage_registry() -> MigrationRegistry {
    MigrationRegistry::new("graph_storage", 1).with_migration(Migration::new(
        0,
        "Initial: 6 edge index tables (out, in, type, state, weight, time)",
    ))
}

/// Schema registry for PersistentConceptDict (ku-core/persistent_concept_dict.rs).
pub fn concept_dict_registry() -> MigrationRegistry {
    MigrationRegistry::new("concept_dict", 1)
        .with_migration(Migration::new(0, "Initial: concepts, ids, meta tables"))
}

/// Schema registry for DhtPersistence (ku-net/dht_store.rs — Phase 3).
pub fn dht_store_registry() -> MigrationRegistry {
    MigrationRegistry::new("dht_store", 1).with_migration(Migration::new(
        0,
        "Initial: dht_entries, replica_meta tables",
    ))
}

/// Schema registry for blob storage.
pub fn blob_store_registry() -> MigrationRegistry {
    MigrationRegistry::new("blob_store", 1)
        .with_migration(Migration::new(0, "Initial: blob_meta, blob_chunks tables"))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_version_ordering() {
        assert!(SchemaVersion(0) < SchemaVersion(1));
        assert!(SchemaVersion(1) < SchemaVersion(2));
        assert_eq!(SchemaVersion(1), SchemaVersion(1));
    }

    #[test]
    fn test_schema_version_display() {
        assert_eq!(format!("{}", SchemaVersion(1)), "v1");
        assert_eq!(format!("{}", SchemaVersion(42)), "v42");
    }

    #[test]
    fn test_migration_target_version() {
        let m = Migration::new(0, "Initial");
        assert_eq!(m.target_version(), SchemaVersion(1));

        let m2 = Migration::new(3, "Add index");
        assert_eq!(m2.target_version(), SchemaVersion(4));
    }

    #[test]
    fn test_registry_validate_valid() {
        let reg = MigrationRegistry::new("test", 2)
            .with_migration(Migration::new(0, "Initial"))
            .with_migration(Migration::new(1, "Add index"));
        assert!(reg.validate().is_ok());
    }

    #[test]
    fn test_registry_validate_broken_chain() {
        let reg = MigrationRegistry::new("test", 3)
            .with_migration(Migration::new(0, "Initial"))
            .with_migration(Migration::new(2, "Skip v1")); // Missing v1
        assert!(reg.validate().is_err());
    }

    #[test]
    fn test_registry_validate_empty() {
        let reg = MigrationRegistry::new("test", 0);
        assert!(reg.validate().is_ok());
    }

    #[test]
    fn test_pending_migrations() {
        let reg = MigrationRegistry::new("test", 3)
            .with_migration(Migration::new(0, "v0→v1"))
            .with_migration(Migration::new(1, "v1→v2"))
            .with_migration(Migration::new(2, "v2→v3"));

        let pending = reg.pending_migrations(SchemaVersion(1));
        assert_eq!(pending.len(), 2); // v1→v2 and v2→v3
    }

    #[test]
    fn test_needs_migration() {
        let reg = MigrationRegistry::new("test", 2)
            .with_migration(Migration::new(0, "v0→v1"))
            .with_migration(Migration::new(1, "v1→v2"));

        assert!(reg.needs_migration(SchemaVersion(0)));
        assert!(reg.needs_migration(SchemaVersion(1)));
        assert!(!reg.needs_migration(SchemaVersion(2)));
    }

    #[cfg(feature = "persist")]
    mod redb_tests {
        use super::super::redb_schema;
        use super::*;

        #[test]
        fn test_fresh_db_schema_init() {
            let tmp = std::env::temp_dir().join("obs_schema_test_fresh.redb");
            let _ = std::fs::remove_file(&tmp);
            let db = redb::Database::create(&tmp).unwrap();

            let reg = ku_storage_registry();
            redb_schema::ensure_schema(&db, &reg).unwrap();

            let version = redb_schema::read_version(&db);
            assert_eq!(version, SchemaVersion(1));

            let _ = std::fs::remove_file(&tmp);
        }

        #[test]
        fn test_idempotent_schema_check() {
            let tmp = std::env::temp_dir().join("obs_schema_test_idem.redb");
            let _ = std::fs::remove_file(&tmp);
            let db = redb::Database::create(&tmp).unwrap();

            let reg = ku_storage_registry();
            redb_schema::ensure_schema(&db, &reg).unwrap();
            redb_schema::ensure_schema(&db, &reg).unwrap(); // Second call should be no-op

            let version = redb_schema::read_version(&db);
            assert_eq!(version, SchemaVersion(1));

            let _ = std::fs::remove_file(&tmp);
        }

        #[test]
        fn test_downgrade_rejected() {
            let tmp = std::env::temp_dir().join("obs_schema_test_down.redb");
            let _ = std::fs::remove_file(&tmp);
            let db = redb::Database::create(&tmp).unwrap();

            // Write version 5
            let reg_v5 = MigrationRegistry::new("test", 5)
                .with_migration(Migration::new(0, "v0"))
                .with_migration(Migration::new(1, "v1"))
                .with_migration(Migration::new(2, "v2"))
                .with_migration(Migration::new(3, "v3"))
                .with_migration(Migration::new(4, "v4"));
            redb_schema::ensure_schema(&db, &reg_v5).unwrap();

            // Try to "downgrade" to v2
            let reg_v2 = MigrationRegistry::new("test", 2)
                .with_migration(Migration::new(0, "v0"))
                .with_migration(Migration::new(1, "v1"));
            let result = redb_schema::ensure_schema(&db, &reg_v2);
            assert!(result.is_err());

            let _ = std::fs::remove_file(&tmp);
        }

        #[test]
        fn test_redb_migration_target_version() {
            let m = redb_schema::RedbMigration::new(0, "Initial", None);
            assert_eq!(m.target_version(), SchemaVersion(1));

            let m2 = redb_schema::RedbMigration::new(2, "Add column", None);
            assert_eq!(m2.target_version(), SchemaVersion(3));
        }

        #[test]
        fn test_ensure_schema_with_redb_migrations_runs_fn() {
            let tmp = std::env::temp_dir().join("obs_schema_test_redb_mig.redb");
            let _ = std::fs::remove_file(&tmp);
            let db = redb::Database::create(&tmp).unwrap();

            // Set up at v1 first
            let reg_v1 =
                MigrationRegistry::new("test_mig", 1).with_migration(Migration::new(0, "Initial"));
            redb_schema::ensure_schema(&db, &reg_v1).unwrap();
            assert_eq!(redb_schema::read_version(&db), SchemaVersion(1));

            // Now upgrade to v2 with a migration function that writes a marker
            let reg_v2 = MigrationRegistry::new("test_mig", 2)
                .with_migration(Migration::new(0, "Initial"))
                .with_migration(Migration::new(1, "Add marker"));

            let marker_table: redb::TableDefinition<&str, &str> =
                redb::TableDefinition::new("_test_marker");

            fn write_marker(db: &redb::Database) -> Result<(), String> {
                let marker: redb::TableDefinition<&str, &str> =
                    redb::TableDefinition::new("_test_marker");
                let txn = db.begin_write().map_err(|e| format!("{}", e))?;
                {
                    let mut t = txn.open_table(marker).map_err(|e| format!("{}", e))?;
                    t.insert("migrated", "yes").map_err(|e| format!("{}", e))?;
                }
                txn.commit().map_err(|e| format!("{}", e))?;
                Ok(())
            }

            let redb_migs = [redb_schema::RedbMigration::new(
                1,
                "Add marker",
                Some(write_marker),
            )];

            redb_schema::ensure_schema_with_redb_migrations(&db, &reg_v2, &redb_migs).unwrap();
            assert_eq!(redb_schema::read_version(&db), SchemaVersion(2));

            // Verify the marker was written by the migration function
            let txn = db.begin_read().unwrap();
            let table = txn.open_table(marker_table).unwrap();
            let val = table.get("migrated").unwrap().unwrap();
            assert_eq!(val.value(), "yes");

            let _ = std::fs::remove_file(&tmp);
        }
    }
}
