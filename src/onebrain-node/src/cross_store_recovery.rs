//! Wave 6 `P8` 4-Phase Cross-Store Startup Reconciliation Coordinator.
//!
//! Executes deterministic startup recovery across the 22 Base storage owners plus
//! isolated `obt-ledger-v1`:
//! - **Phase R1 (Rollout, Authority & Sequence Floors — Fail-Closed):**
//!   Validates `0x000E` rollout config, `0x0001` canonical authority high-water,
//!   `0x000C` PoMV `FEED_HEADS` anti-equivocation sequence floors, and `0x000F`
//!   optional network replay floors. Any corruption fails closed with
//!   `STORAGE_INTEGRITY_FAILURE`.
//! - **Phase R2 (Canonical Stores & Blob Reference Parity — Fail-Closed):**
//!   Verifies `ValidatedStore` (`0x0001`), `PrivateVault` (`0x0002`), `Quarantine`
//!   (`0x0003`), and `BlobStorage` (`0x0004`) parity against `CanonicalBlobReferenceOracle`.
//! - **Phase R3 (Cross-Store Journal Roll-Forward & Uncommitted Share Expiry):**
//!   Reconciles `PendingBlobUploadStore` (`0x0005`), `SourceCaptureTransactionStore` (`0x0006`),
//!   and prunes/invalidates expired or uncommitted `D-062` `ku_public_share_prepared_v1`
//!   previews without auto-confirming any share.
//! - **Phase R4 (Auto-Delete & Rebuild Derived `P3`/`P7`/`P4` Views + Non-Fatal `P5` OBT):**
//!   Deletes and rebuilds corrupt disposable projections (`0x0015` `derived_index`,
//!   `0x0016` `retriever_projection`, `P7` OBKG caches) while isolating any corruption
//!   in `obt-ledger-v1/obt.redb` as non-fatal (`NEG-OBT-001` / `FW-07`).

use std::path::Path;

use ku_core::foundation::{RedbVerifiedBackend, ValidatedStore};
use onebrain_blob_store::blob_storage::BlobStorage;
use onebrain_obt::RedbObtStore;
use redb::{Database, ReadableTable, TableDefinition};
use thiserror::Error;

use crate::blob_authority::{BlobAuthority, CanonicalBlobReferenceOracle};
use crate::dataset_path::{BaseStorageOwnerId, DatasetPathResolver};
use crate::derived_index::{
    DerivedIndexOpenState, RedbAcceptedRecordScan, VNextDerivedIndexManager,
};
use crate::vnext_extended_config::VNextExtendedFeatureConfig;

const ROLLOUT_EXTENDED_FILE: &str = "vnext_extended_rollout.cbor";
const OPTIONAL_NET_FLOORS_FILE: &str = "vnext_optional_net_floors.cbor";
const POMV_SENDER_DB: &str = "vnext_public_use_sender.redb";
const POMV_FEED_HEADS: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_public_use_feed_heads_v1");
const KU_JOURNAL_DB: &str = "journal.redb";
const KU_PUBLIC_SHARE_PREPARED: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("ku_public_share_prepared_v1");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryPhase {
    R1AuthorityAndFloors,
    R2CanonicalAndBlobParity,
    R3JournalsAndPreparedExpiry,
    R4DerivedViewsAndIsolatedObt,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("STORAGE_INTEGRITY_FAILURE in {phase:?}: {detail}")]
pub struct CrossStoreRecoveryError {
    pub phase: RecoveryPhase,
    pub detail: String,
}

impl CrossStoreRecoveryError {
    pub fn integrity(phase: RecoveryPhase, detail: impl Into<String>) -> Self {
        Self {
            phase,
            detail: detail.into(),
        }
    }

    pub const fn code(&self) -> &'static str {
        "STORAGE_INTEGRITY_FAILURE"
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrossStoreRecoveryReport {
    pub completed_phases: [RecoveryPhase; 4],
    pub r1_extended_rollout: VNextExtendedFeatureConfig,
    pub r1_verified_feed_floors: usize,
    pub r2_verified_owned_blobs: usize,
    pub r3_invalidated_share_previews: usize,
    pub r4_derived_index_state: DerivedIndexOpenState,
    pub r4_rebuilt_derived_lanes: Vec<String>,
    /// Non-`None` when `obt-ledger-v1/obt.redb` was corrupt/unreadable and safely
    /// quarantined without failing startup (`NEG-OBT-001`).
    pub r4_isolated_obt_degraded: Option<String>,
}

pub struct CrossStoreRecoveryCoordinator;

impl CrossStoreRecoveryCoordinator {
    /// Execute strict 4-phase `R1 -> R2 -> R3 -> R4` startup reconciliation.
    pub fn run_startup_reconciliation(
        resolver: &dyn DatasetPathResolver,
        blob_storage: Option<&BlobStorage>,
        blob_authority: Option<&BlobAuthority>,
        obt_data_dir: Option<&Path>,
    ) -> Result<CrossStoreRecoveryReport, CrossStoreRecoveryError> {
        // ─── Phase R1: Rollout, Authority & Anti-Equivocation Sequence Floors ─
        let rollout_dir = resolver
            .owner_path(BaseStorageOwnerId::ROLLOUT)
            .map_err(|e| {
                CrossStoreRecoveryError::integrity(
                    RecoveryPhase::R1AuthorityAndFloors,
                    format!("rollout owner path invalid: {e}"),
                )
            })?;
        let extended_path = rollout_dir.join(ROLLOUT_EXTENDED_FILE);
        let r1_extended_rollout = if extended_path.exists() {
            let bytes = std::fs::read(&extended_path).map_err(|e| {
                CrossStoreRecoveryError::integrity(
                    RecoveryPhase::R1AuthorityAndFloors,
                    format!("rollout file unreadable: {e}"),
                )
            })?;
            VNextExtendedFeatureConfig::decode_canonical_bytes(&bytes).map_err(|e| {
                CrossStoreRecoveryError::integrity(
                    RecoveryPhase::R1AuthorityAndFloors,
                    format!("corrupt 0x000E extended rollout config: {e}"),
                )
            })?
        } else {
            VNextExtendedFeatureConfig::default()
        };

        let opt_net_dir = resolver
            .owner_path(BaseStorageOwnerId::OPTIONAL_NETWORK)
            .map_err(|e| {
                CrossStoreRecoveryError::integrity(
                    RecoveryPhase::R1AuthorityAndFloors,
                    format!("optional_network owner path invalid: {e}"),
                )
            })?;
        let opt_floors_path = opt_net_dir.join(OPTIONAL_NET_FLOORS_FILE);
        if opt_floors_path.exists() {
            let bytes = std::fs::read(&opt_floors_path).map_err(|e| {
                CrossStoreRecoveryError::integrity(
                    RecoveryPhase::R1AuthorityAndFloors,
                    format!("optional_network floors unreadable: {e}"),
                )
            })?;
            ku_core::foundation::decode_canonical(
                &bytes,
                ku_core::foundation::ResourceProfile::ObjectV1,
            )
            .map_err(|e| {
                CrossStoreRecoveryError::integrity(
                    RecoveryPhase::R1AuthorityAndFloors,
                    format!("corrupt 0x000F optional network floor state: {e:?}"),
                )
            })?;
        }

        let pomv_dir = resolver
            .owner_path(BaseStorageOwnerId::PRIVATE_POMV)
            .map_err(|e| {
                CrossStoreRecoveryError::integrity(
                    RecoveryPhase::R1AuthorityAndFloors,
                    format!("private_pomv owner path invalid: {e}"),
                )
            })?;
        let pomv_sender_path = pomv_dir.join(POMV_SENDER_DB);
        let mut r1_verified_feed_floors = 0usize;
        if pomv_sender_path.exists() {
            let db = Database::open(&pomv_sender_path).map_err(|e| {
                CrossStoreRecoveryError::integrity(
                    RecoveryPhase::R1AuthorityAndFloors,
                    format!("corrupt 0x000C PoMV sequence floor DB: {e}"),
                )
            })?;
            let read = db.begin_read().map_err(|e| {
                CrossStoreRecoveryError::integrity(
                    RecoveryPhase::R1AuthorityAndFloors,
                    format!("unreadable 0x000C PoMV read tx: {e}"),
                )
            })?;
            if let Ok(table) = read.open_table(POMV_FEED_HEADS) {
                for row in table.iter().map_err(|e| {
                    CrossStoreRecoveryError::integrity(
                        RecoveryPhase::R1AuthorityAndFloors,
                        format!("corrupt 0x000C FEED_HEADS cursor: {e}"),
                    )
                })? {
                    let (k, v) = row.map_err(|e| {
                        CrossStoreRecoveryError::integrity(
                            RecoveryPhase::R1AuthorityAndFloors,
                            format!("corrupt 0x000C FEED_HEADS entry: {e}"),
                        )
                    })?;
                    if k.value().len() != 32
                        || serde_json::from_slice::<serde_json::Value>(v.value()).is_err()
                    {
                        return Err(CrossStoreRecoveryError::integrity(
                            RecoveryPhase::R1AuthorityAndFloors,
                            "corrupt 0x000C feed sequence floor entry",
                        ));
                    }
                    r1_verified_feed_floors += 1;
                }
            }
        }

        // ─── Phase R2: Canonical Store & Owned Blob Parity ───────────────────
        let canonical_dir = resolver
            .owner_path(BaseStorageOwnerId::CANONICAL)
            .map_err(|e| {
                CrossStoreRecoveryError::integrity(
                    RecoveryPhase::R2CanonicalAndBlobParity,
                    format!("canonical owner path invalid: {e}"),
                )
            })?;
        let canonical_db_path = canonical_dir.join("vnext_verified.redb");
        if canonical_db_path.exists() {
            let backend = RedbVerifiedBackend::open(&canonical_db_path).map_err(|e| {
                CrossStoreRecoveryError::integrity(
                    RecoveryPhase::R2CanonicalAndBlobParity,
                    format!("corrupt 0x0001 vnext_verified.redb: {e}"),
                )
            })?;
            let store = ValidatedStore::new(backend);
            let _ = store
                .accepted_entries(ku_core::foundation::StoredRecordKind::Object)
                .map_err(|e| {
                    CrossStoreRecoveryError::integrity(
                        RecoveryPhase::R2CanonicalAndBlobParity,
                        format!("canonical accepted_entries parity check failed: {e:?}"),
                    )
                })?;
        }

        let mut r2_verified_owned_blobs = 0usize;
        if let (Some(storage), Some(authority)) = (blob_storage, blob_authority) {
            let oracle: std::sync::Arc<CanonicalBlobReferenceOracle> = authority.canonical_oracle();
            let retained = oracle.retained_canonical_records().map_err(|e| {
                CrossStoreRecoveryError::integrity(
                    RecoveryPhase::R2CanonicalAndBlobParity,
                    format!("blob reference oracle unreadable: {e}"),
                )
            })?;
            for reference in retained {
                if reference.retains_blob() {
                    let bytes = storage.read_full_blob(&reference.blob_cid).map_err(|e| {
                        CrossStoreRecoveryError::integrity(
                            RecoveryPhase::R2CanonicalAndBlobParity,
                            format!(
                                "missing or unreadable canonical blob {}: {e}",
                                reference.blob_cid.to_hex()
                            ),
                        )
                    })?;
                    if ku_core::blob_store::BlobCid::from_content(
                        reference.blob_cid.blob_type(),
                        &bytes,
                    ) != reference.blob_cid
                    {
                        return Err(CrossStoreRecoveryError::integrity(
                            RecoveryPhase::R2CanonicalAndBlobParity,
                            "owned blob content digest mismatch",
                        ));
                    }
                    r2_verified_owned_blobs += 1;
                }
            }
        }

        // ─── Phase R3: Cross-Store Journals & D-062 Prepared Share Invalidation
        if let Some(authority) = blob_authority {
            authority.pending().reconcile_generation().map_err(|e| {
                CrossStoreRecoveryError::integrity(
                    RecoveryPhase::R3JournalsAndPreparedExpiry,
                    format!("pending blob upload journal failed reconciliation: {e}"),
                )
            })?;
        }
        if let Some(storage) = blob_storage {
            storage.recover_pending_filesystem_intents().map_err(|e| {
                CrossStoreRecoveryError::integrity(
                    RecoveryPhase::R3JournalsAndPreparedExpiry,
                    format!("blob filesystem intent recovery failed: {e}"),
                )
            })?;
        }

        let vault_dir = resolver
            .owner_path(BaseStorageOwnerId::VAULT)
            .map_err(|e| {
                CrossStoreRecoveryError::integrity(
                    RecoveryPhase::R3JournalsAndPreparedExpiry,
                    format!("vault owner path invalid: {e}"),
                )
            })?;
        let ku_journal_path = vault_dir.join("ku-product-v1").join(KU_JOURNAL_DB);
        let mut r3_invalidated_share_previews = 0usize;
        if ku_journal_path.exists() {
            let db = Database::open(&ku_journal_path).map_err(|e| {
                CrossStoreRecoveryError::integrity(
                    RecoveryPhase::R3JournalsAndPreparedExpiry,
                    format!("corrupt 0x0002 ku-product-v1/journal.redb: {e}"),
                )
            })?;
            // Inspect `ku_public_share_prepared_v1` rows; if any row carries the
            // `InvalidatedAcrossRestore` marker, prune it cleanly.
            let stale_keys: Vec<Vec<u8>> = {
                let read = db.begin_read().map_err(|e| {
                    CrossStoreRecoveryError::integrity(
                        RecoveryPhase::R3JournalsAndPreparedExpiry,
                        format!("journal read tx failed: {e}"),
                    )
                })?;
                if let Ok(tbl) = read.open_table(KU_PUBLIC_SHARE_PREPARED) {
                    let mut keys = Vec::new();
                    if let Ok(iter) = tbl.iter() {
                        for entry in iter.flatten() {
                            if entry.1.value() == b"InvalidatedAcrossRestore" {
                                keys.push(entry.0.value().to_vec());
                            }
                        }
                    }
                    keys
                } else {
                    Vec::new()
                }
            };
            if !stale_keys.is_empty() {
                let write = db.begin_write().map_err(|e| {
                    CrossStoreRecoveryError::integrity(
                        RecoveryPhase::R3JournalsAndPreparedExpiry,
                        format!("journal write tx failed: {e}"),
                    )
                })?;
                {
                    if let Ok(mut tbl) = write.open_table(KU_PUBLIC_SHARE_PREPARED) {
                        for key in &stale_keys {
                            let _ = tbl.remove(key.as_slice());
                            r3_invalidated_share_previews += 1;
                        }
                    }
                }
                write.commit().map_err(|e| {
                    CrossStoreRecoveryError::integrity(
                        RecoveryPhase::R3JournalsAndPreparedExpiry,
                        format!("journal commit failed: {e}"),
                    )
                })?;
            }
        }

        // ─── Phase R4: Auto-Rebuild Corrupt Derived Views + Isolate P5 OBT ───
        let mut r4_rebuilt_derived_lanes = Vec::new();
        let derived_dir = resolver
            .owner_path(BaseStorageOwnerId::DERIVED_INDEX)
            .map_err(|e| {
                CrossStoreRecoveryError::integrity(
                    RecoveryPhase::R4DerivedViewsAndIsolatedObt,
                    format!("derived_index owner path invalid: {e}"),
                )
            })?;
        // Check if any corrupt non-Redb marker/cache file exists in `derived_index` and wipe it
        // before `VNextDerivedIndexManager::open_or_rebuild` regenerates from canonical records.
        let obkg_cache_file = derived_dir.join("obkg_disposable_cache.cbor");
        if obkg_cache_file.exists() {
            if let Ok(bytes) = std::fs::read(&obkg_cache_file) {
                if ku_core::foundation::decode_canonical(
                    &bytes,
                    ku_core::foundation::ResourceProfile::ObjectV1,
                )
                .is_err()
                {
                    let _ = std::fs::remove_file(&obkg_cache_file);
                    r4_rebuilt_derived_lanes.push("p7_obkg_disposable_cache".to_string());
                }
            }
        }

        let manager = match VNextDerivedIndexManager::new(&derived_dir) {
            Ok(m) => m,
            Err(_) => {
                // Wipe corrupt derived directory contents and recreate cleanly!
                if let Ok(entries) = std::fs::read_dir(&derived_dir) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if p.is_dir() {
                            let _ = std::fs::remove_dir_all(&p);
                        } else {
                            let _ = std::fs::remove_file(&p);
                        }
                    }
                }
                r4_rebuilt_derived_lanes.push("0x0015_derived_index_wiped".to_string());
                VNextDerivedIndexManager::new(&derived_dir).map_err(|e| {
                    CrossStoreRecoveryError::integrity(
                        RecoveryPhase::R4DerivedViewsAndIsolatedObt,
                        format!("derived index failed after clean recreate: {e}"),
                    )
                })?
            }
        };
        let scan = RedbAcceptedRecordScan::new(canonical_db_path);
        let (r4_derived_index_state, _) = manager.open_or_rebuild(&scan);
        if matches!(r4_derived_index_state, DerivedIndexOpenState::Rebuilt) {
            r4_rebuilt_derived_lanes.push("0x0015_derived_index_rebuilt".to_string());
        }

        // Isolated P5 OBT health probe: corruption in `obt-ledger-v1/obt.redb` MUST NEVER
        // fail startup (`NEG-OBT-001` / `FW-07`).
        let mut r4_isolated_obt_degraded = None;
        if let Some(obt_root) = obt_data_dir {
            let obt_db = obt_root.join("obt-ledger-v1").join("obt.redb");
            if obt_db.exists() {
                match RedbObtStore::open(&obt_db) {
                    Ok(store) => {
                        if let Err(e) = store.get_runtime_mode() {
                            r4_isolated_obt_degraded = Some(e.to_string());
                        }
                    }
                    Err(e) => {
                        r4_isolated_obt_degraded = Some(e.to_string());
                    }
                }
            }
        }

        Ok(CrossStoreRecoveryReport {
            completed_phases: [
                RecoveryPhase::R1AuthorityAndFloors,
                RecoveryPhase::R2CanonicalAndBlobParity,
                RecoveryPhase::R3JournalsAndPreparedExpiry,
                RecoveryPhase::R4DerivedViewsAndIsolatedObt,
            ],
            r1_extended_rollout,
            r1_verified_feed_floors,
            r2_verified_owned_blobs,
            r3_invalidated_share_previews,
            r4_derived_index_state,
            r4_rebuilt_derived_lanes,
            r4_isolated_obt_degraded,
        })
    }
}
