//! Node-owned Base archive orchestration and logical backend adapters.
//!
//! Lower crates expose substrate-neutral rows. This module is the only layer
//! that maps those rows to `onebrain-archive` logical entries; database paths
//! and raw Redb files never become archive payloads.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::{Arc, Mutex};

use onebrain_archive::{
    capture_dataset, compute_high_water_root, seal_archive, verify_dataset_archive_v2,
    ArchiveEntryId, ArchiveEntryKind, ArchiveEntryV1, ArchiveError, ArchiveLimits,
    ArchiveLogicalKey, ArchiveOwner, ArchiveRestorePolicyV1, DatasetManifestV1,
    FileSecureSpoolFactory, PortableDataCompatibilityV1, ProducerArtifactIdentityV1, SnapshotLease,
    SnapshotSource,
};
use serde::{Deserialize, Serialize};

use ku_core::blob_store::{BlobCid, BLOB_CID_VERSION};
use ku_core::foundation::{
    AcceptedRecordEntry, AtomicVerifiedBackend, MigrationStateSnapshotPort,
    PortableMigrationSnapshot, PortableVaultRecord, PortableVaultSnapshotPort, QuarantineRecord,
    StoredRecordKind, ValidatedMigrationRestorePort, ValidatedVaultRestorePort,
    VaultQuarantineRecord,
};
use ku_kql::blob_storage::{BlobReferenceOracle as _, BlobStorage};

use crate::archive_capabilities::{
    ArchiveCapabilityRegistry, ArchiveOperationReservationId, ArchiveSecretHandle,
    ReadableArchiveSinkHandle, SealedArchiveSourceHandle, WritableArchiveSinkHandle,
};
use crate::blob_authority::CanonicalBlobReferenceOracle;
use crate::dataset_generation::{
    DatasetGenerationStore, RestoreOperationBinding, StagedDatasetGeneration,
};
use crate::dataset_path::DatasetPathResolver;
use crate::error::NodeError;
use crate::identity_recovery::{recover_staged_identity, IdentityRecoveryReceipt};
use crate::signer_ports::SignerProviderRegistry;
use crate::vnext_validated_sink::SharedVNextValidatedSink;
use crate::DatasetGenerationReceipt;

const SNAPSHOT_BINDING_DOMAIN: &str = "onebrain:base:node-snapshot-binding:1";
const RESTORE_IDEMPOTENCY_DOMAIN: &str = "onebrain:base:restore-idempotency:1";
const MAX_SNAPSHOT_RECORDS: usize = 1_000_000;

/// Substrate-neutral logical row. `key` is an application identity, never a
/// path, and `bytes` are validated logical bytes rather than backend storage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveSnapshotRecord {
    pub kind: ArchiveEntryKind,
    pub owner: ArchiveOwner,
    pub namespace: u16,
    pub key: Vec<u8>,
    pub bytes: Vec<u8>,
    pub required: bool,
}

/// Node-owned bridge over lower-crate bounded scan/validated-restore ports.
/// Archive types do not flow into those lower crates; concrete adapters map at
/// this boundary only.
pub trait SnapshotVerifiedBackend: Send + Sync {
    fn owns(&self, owner: ArchiveOwner) -> bool;
    fn bounded_snapshot(&self) -> Result<Vec<ArchiveSnapshotRecord>, NodeError>;
    fn restore_validated(&self, record: &ArchiveSnapshotRecord) -> Result<(), NodeError>;
    fn reconcile_after_restore(&self) -> Result<(), NodeError> {
        Ok(())
    }
}

/// Opens validated target-store adapters inside an unactivated dataset
/// generation. Restore refuses to activate when the host has not supplied
/// this factory; source-store adapters are never reused against the old active
/// generation by accident.
pub trait StagedArchiveBackendFactory: Send + Sync {
    fn open_for_staged_generation(
        &self,
        resolver: &dyn DatasetPathResolver,
    ) -> Result<Vec<Arc<dyn SnapshotVerifiedBackend>>, NodeError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortableArchiveRow {
    pub table: u8,
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

/// Port implemented by Node-owned durable stores. Rows are logical table
/// encodings that the store itself validates; no database pages or paths are
/// exposed.
pub trait PortableArchiveRows: Send + Sync {
    fn archive_owner(&self) -> ArchiveOwner;
    fn archive_entry_kind(&self) -> ArchiveEntryKind;
    fn archive_rows(&self) -> Result<Vec<PortableArchiveRow>, NodeError>;
    fn restore_row(&self, row: &PortableArchiveRow) -> Result<(), NodeError>;
    fn reconcile_restored_rows(&self) -> Result<(), NodeError> {
        Ok(())
    }
}

pub struct LogicalRowsArchiveBackend<T> {
    store: T,
}

impl<T> LogicalRowsArchiveBackend<T> {
    pub const fn new(store: T) -> Self {
        Self { store }
    }
}

impl<T: PortableArchiveRows> SnapshotVerifiedBackend for LogicalRowsArchiveBackend<T> {
    fn owns(&self, owner: ArchiveOwner) -> bool {
        self.store.archive_owner() == owner
    }

    fn bounded_snapshot(&self) -> Result<Vec<ArchiveSnapshotRecord>, NodeError> {
        let owner = self.store.archive_owner();
        let kind = self.store.archive_entry_kind();
        self.store
            .archive_rows()?
            .into_iter()
            .map(|row| {
                if row.key.len() >= 256 {
                    return Err(NodeError::ArchiveCapability(
                        "logical archive row key exceeds the bound".into(),
                    ));
                }
                let mut key = Vec::with_capacity(row.key.len() + 1);
                key.push(row.table);
                key.extend_from_slice(&row.key);
                Ok(ArchiveSnapshotRecord {
                    kind,
                    owner,
                    namespace: 1,
                    key,
                    bytes: row.value,
                    required: true,
                })
            })
            .collect()
    }

    fn restore_validated(&self, record: &ArchiveSnapshotRecord) -> Result<(), NodeError> {
        if record.owner != self.store.archive_owner()
            || record.kind != self.store.archive_entry_kind()
        {
            return Err(NodeError::ArchiveCapability(
                "logical archive adapter owner/kind mismatch".into(),
            ));
        }
        let (&table, key) = record.key.split_first().ok_or_else(|| {
            NodeError::ArchiveCapability("logical archive row key is empty".into())
        })?;
        self.store.restore_row(&PortableArchiveRow {
            table,
            key: key.to_vec(),
            value: record.bytes.clone(),
        })
    }

    fn reconcile_after_restore(&self) -> Result<(), NodeError> {
        self.store.reconcile_restored_rows()
    }
}

/// Canonical/Quarantine adapter over the normal validate-then-accept sink.
/// Quarantine rows remain explicitly non-executable evidence.
pub struct ValidatedCanonicalArchiveBackend<B> {
    sink: SharedVNextValidatedSink<B>,
}

impl<B> ValidatedCanonicalArchiveBackend<B> {
    pub const fn new(sink: SharedVNextValidatedSink<B>) -> Self {
        Self { sink }
    }
}

impl<B: AtomicVerifiedBackend + 'static> SnapshotVerifiedBackend
    for ValidatedCanonicalArchiveBackend<B>
{
    fn owns(&self, owner: ArchiveOwner) -> bool {
        owner == ArchiveOwner::CANONICAL || owner == ArchiveOwner::QUARANTINE
    }

    fn bounded_snapshot(&self) -> Result<Vec<ArchiveSnapshotRecord>, NodeError> {
        let snapshot = self
            .sink
            .portable_verified_snapshot()
            .map_err(NodeError::Storage)?;
        let authority_high_water = authority_high_water_bytes(&snapshot.accepted);
        let mut rows = Vec::with_capacity(snapshot.accepted.len() + snapshot.quarantine.len());
        for record in snapshot.accepted {
            let kind = archive_kind(record.record_kind);
            let mut key = Vec::with_capacity(33);
            key.push(record.record_kind as u8);
            key.extend_from_slice(&record.claimed_cid);
            rows.push(ArchiveSnapshotRecord {
                kind,
                owner: ArchiveOwner::CANONICAL,
                namespace: 1,
                key,
                bytes: record.canonical_bytes,
                required: true,
            });
        }
        for record in snapshot.quarantine {
            rows.push(ArchiveSnapshotRecord {
                kind: ArchiveEntryKind::QuarantineRecord,
                owner: ArchiveOwner::QUARANTINE,
                namespace: 1,
                key: record.quarantine_id.to_vec(),
                bytes: encode_quarantine(&record)?,
                required: false,
            });
        }
        rows.push(ArchiveSnapshotRecord {
            kind: ArchiveEntryKind::AuthorityHighWater,
            owner: ArchiveOwner::CANONICAL,
            namespace: 1,
            key: b"authority-high-water-v1".to_vec(),
            bytes: authority_high_water.to_vec(),
            required: true,
        });
        Ok(rows)
    }

    fn restore_validated(&self, record: &ArchiveSnapshotRecord) -> Result<(), NodeError> {
        match record.owner {
            ArchiveOwner::CANONICAL
                if record.kind == ArchiveEntryKind::AuthorityHighWater
                    && record.key == b"authority-high-water-v1" =>
            {
                let snapshot = self
                    .sink
                    .portable_verified_snapshot()
                    .map_err(NodeError::Storage)?;
                if record.bytes.as_slice() != authority_high_water_bytes(&snapshot.accepted) {
                    return Err(NodeError::ArchiveCapability(
                        "authority high-water does not match restored authority events".into(),
                    ));
                }
                Ok(())
            }
            ArchiveOwner::CANONICAL => {
                let (record_kind, claimed_cid) = decode_accepted_key(&record.key)?;
                if archive_kind(record_kind) != record.kind {
                    return Err(NodeError::ArchiveCapability(
                        "canonical archive kind/key mismatch".into(),
                    ));
                }
                self.sink
                    .restore_accepted_record(&AcceptedRecordEntry {
                        record_kind,
                        claimed_cid,
                        canonical_bytes: record.bytes.clone(),
                    })
                    .map_err(NodeError::Storage)
            }
            ArchiveOwner::QUARANTINE if record.kind == ArchiveEntryKind::QuarantineRecord => {
                let quarantine = decode_quarantine(&record.bytes)?;
                if record.key.as_slice() != quarantine.quarantine_id {
                    return Err(NodeError::ArchiveCapability(
                        "quarantine archive key mismatch".into(),
                    ));
                }
                self.sink
                    .restore_quarantine_evidence(&quarantine)
                    .map_err(NodeError::Storage)
            }
            _ => Err(NodeError::ArchiveCapability(
                "canonical adapter received an unsupported owner".into(),
            )),
        }
    }
}

fn authority_high_water_bytes(records: &[AcceptedRecordEntry]) -> [u8; 32] {
    let mut authority = records
        .iter()
        .filter(|record| record.record_kind == StoredRecordKind::AuthorityEvent)
        .collect::<Vec<_>>();
    authority.sort_by_key(|record| record.claimed_cid);
    let mut hasher = blake3::Hasher::new_derive_key("onebrain:base:authority-high-water:1");
    hasher.update(&(authority.len() as u64).to_be_bytes());
    for record in authority {
        hasher.update(&record.claimed_cid);
        hasher.update(&(record.canonical_bytes.len() as u64).to_be_bytes());
        hasher.update(&record.canonical_bytes);
    }
    *hasher.finalize().as_bytes()
}

/// Portable private Vault adapter. Only canonical plaintext crosses this
/// boundary, already inside the authenticated encrypted archive stream; the
/// target port validates it and encrypts it under the target Vault key.
pub struct VaultArchiveBackend<T> {
    vault: T,
}

impl<T> VaultArchiveBackend<T> {
    pub const fn new(vault: T) -> Self {
        Self { vault }
    }
}

impl<T> SnapshotVerifiedBackend for VaultArchiveBackend<T>
where
    T: PortableVaultSnapshotPort + ValidatedVaultRestorePort + Send + Sync,
{
    fn owns(&self, owner: ArchiveOwner) -> bool {
        owner == ArchiveOwner::VAULT
    }

    fn bounded_snapshot(&self) -> Result<Vec<ArchiveSnapshotRecord>, NodeError> {
        let snapshot = self
            .vault
            .portable_vault_snapshot()
            .map_err(|error| NodeError::Storage(error.to_string()))?;
        let mut rows = Vec::with_capacity(snapshot.accepted.len() + snapshot.quarantine.len());
        for record in snapshot.accepted {
            let mut key = Vec::with_capacity(34);
            key.push(0);
            key.push(record.record_kind as u8);
            key.extend_from_slice(&record.claimed_cid);
            rows.push(ArchiveSnapshotRecord {
                kind: ArchiveEntryKind::VaultRecord,
                owner: ArchiveOwner::VAULT,
                namespace: 1,
                key,
                bytes: record.canonical_plaintext,
                required: true,
            });
        }
        for record in snapshot.quarantine {
            let mut key = Vec::with_capacity(33);
            key.push(1);
            key.extend_from_slice(&record.quarantine_id);
            rows.push(ArchiveSnapshotRecord {
                kind: ArchiveEntryKind::QuarantineRecord,
                owner: ArchiveOwner::VAULT,
                namespace: 1,
                key,
                bytes: encode_vault_quarantine(&record)?,
                required: false,
            });
        }
        rows.sort_by(|left, right| left.key.cmp(&right.key));
        Ok(rows)
    }

    fn restore_validated(&self, record: &ArchiveSnapshotRecord) -> Result<(), NodeError> {
        if record.owner != ArchiveOwner::VAULT {
            return Err(NodeError::ArchiveCapability(
                "Vault adapter received the wrong owner".into(),
            ));
        }
        match (record.kind, record.key.first()) {
            (ArchiveEntryKind::VaultRecord, Some(0)) if record.key.len() == 34 => {
                let record_kind = stored_record_kind(record.key[1])?;
                let claimed_cid = record.key[2..]
                    .try_into()
                    .map_err(|_| NodeError::ArchiveCapability("Vault record CID length".into()))?;
                self.vault
                    .restore_vault_record(&PortableVaultRecord {
                        record_kind,
                        claimed_cid,
                        canonical_plaintext: record.bytes.clone(),
                    })
                    .map_err(|error| NodeError::Storage(error.to_string()))
            }
            (ArchiveEntryKind::QuarantineRecord, Some(1)) if record.key.len() == 33 => {
                let quarantine = decode_vault_quarantine(&record.bytes)?;
                if quarantine.quarantine_id.as_slice() != &record.key[1..] {
                    return Err(NodeError::ArchiveCapability(
                        "Vault quarantine key mismatch".into(),
                    ));
                }
                self.vault
                    .restore_vault_quarantine(&quarantine)
                    .map_err(|error| NodeError::Storage(error.to_string()))
            }
            _ => Err(NodeError::ArchiveCapability(
                "Vault archive row is invalid".into(),
            )),
        }
    }
}

fn stored_record_kind(value: u8) -> Result<StoredRecordKind, NodeError> {
    match value {
        1 => Ok(StoredRecordKind::Object),
        2 => Ok(StoredRecordKind::Event),
        3 => Ok(StoredRecordKind::FeedInception),
        4 => Ok(StoredRecordKind::AuthorityEvent),
        _ => Err(NodeError::ArchiveCapability(
            "Vault record kind is unknown".into(),
        )),
    }
}

fn encode_vault_quarantine(record: &VaultQuarantineRecord) -> Result<Vec<u8>, NodeError> {
    encode_quarantine(&QuarantineRecord {
        quarantine_id: record.quarantine_id,
        record_kind: record.record_kind,
        claimed_cid: record.claimed_cid,
        reason_code: record.reason_code.clone(),
        original_bytes: record.plaintext.clone(),
    })
}

fn decode_vault_quarantine(bytes: &[u8]) -> Result<VaultQuarantineRecord, NodeError> {
    let record = decode_quarantine(bytes)?;
    Ok(VaultQuarantineRecord {
        quarantine_id: record.quarantine_id,
        record_kind: record.record_kind,
        claimed_cid: record.claimed_cid,
        reason_code: record.reason_code,
        plaintext: record.original_bytes,
    })
}

#[cfg(feature = "vnext-network-runtime")]
pub struct ReconciliationArchiveBackend {
    backend: ku_net::vnext_reconciliation_journal::RedbReconciliationJournalBackend,
}

#[cfg(feature = "vnext-network-runtime")]
impl ReconciliationArchiveBackend {
    pub const fn new(
        backend: ku_net::vnext_reconciliation_journal::RedbReconciliationJournalBackend,
    ) -> Self {
        Self { backend }
    }
}

#[cfg(feature = "vnext-network-runtime")]
impl SnapshotVerifiedBackend for ReconciliationArchiveBackend {
    fn owns(&self, owner: ArchiveOwner) -> bool {
        owner == ArchiveOwner::RECONCILIATION
    }

    fn bounded_snapshot(&self) -> Result<Vec<ArchiveSnapshotRecord>, NodeError> {
        use ku_net::vnext_reconciliation_journal::ReconciliationJournalArchivePort as _;

        self.backend
            .archive_journal_records()
            .map_err(NodeError::Storage)?
            .into_iter()
            .map(|record| {
                Ok(ArchiveSnapshotRecord {
                    kind: ArchiveEntryKind::ReconciliationJournalRecord,
                    owner: ArchiveOwner::RECONCILIATION,
                    namespace: 1,
                    key: record.binding.to_vec(),
                    bytes: record.canonical_snapshot,
                    required: true,
                })
            })
            .collect()
    }

    fn restore_validated(&self, record: &ArchiveSnapshotRecord) -> Result<(), NodeError> {
        use ku_net::vnext_reconciliation_journal::{
            PortableReconciliationJournalRecord, ReconciliationJournalArchivePort as _,
        };

        if record.kind != ArchiveEntryKind::ReconciliationJournalRecord
            || record.owner != ArchiveOwner::RECONCILIATION
        {
            return Err(NodeError::ArchiveCapability(
                "reconciliation adapter received the wrong logical kind".into(),
            ));
        }
        let binding =
            record.key.as_slice().try_into().map_err(|_| {
                NodeError::ArchiveCapability("reconciliation binding length".into())
            })?;
        self.backend
            .restore_journal_record(&PortableReconciliationJournalRecord {
                binding,
                canonical_snapshot: record.bytes.clone(),
            })
            .map_err(NodeError::Storage)
    }

    fn reconcile_after_restore(&self) -> Result<(), NodeError> {
        // Journal session open performs bounded inflight-reservation cleanup;
        // no pending payload is blindly resumed here.
        Ok(())
    }
}

#[cfg(feature = "vnext-network-runtime")]
pub struct InventoryArchiveBackend {
    backend: ku_net::vnext_inventory_forest::RedbInventoryForestBackend,
}

#[cfg(feature = "vnext-network-runtime")]
impl InventoryArchiveBackend {
    pub const fn new(backend: ku_net::vnext_inventory_forest::RedbInventoryForestBackend) -> Self {
        Self { backend }
    }
}

#[cfg(feature = "vnext-network-runtime")]
impl SnapshotVerifiedBackend for InventoryArchiveBackend {
    fn owns(&self, owner: ArchiveOwner) -> bool {
        owner == ArchiveOwner::INVENTORY
    }

    fn bounded_snapshot(&self) -> Result<Vec<ArchiveSnapshotRecord>, NodeError> {
        use ku_net::vnext_inventory_forest::InventoryForestArchivePort as _;

        self.backend
            .archive_inventory_records()
            .map_err(|error| NodeError::Storage(format!("{error:?}")))?
            .into_iter()
            .map(|record| {
                Ok(ArchiveSnapshotRecord {
                    kind: ArchiveEntryKind::InventoryRecord,
                    owner: ArchiveOwner::INVENTORY,
                    namespace: 1,
                    key: record.selector.as_bytes().to_vec(),
                    bytes: record.canonical_snapshot,
                    required: true,
                })
            })
            .collect()
    }

    fn restore_validated(&self, record: &ArchiveSnapshotRecord) -> Result<(), NodeError> {
        use ku_core::foundation::SelectorCid;
        use ku_net::vnext_inventory_forest::{
            InventoryForestArchivePort as _, PortableInventoryRecord,
        };

        if record.kind != ArchiveEntryKind::InventoryRecord
            || record.owner != ArchiveOwner::INVENTORY
        {
            return Err(NodeError::ArchiveCapability(
                "inventory adapter received the wrong logical kind".into(),
            ));
        }
        let selector: [u8; 32] = record
            .key
            .as_slice()
            .try_into()
            .map_err(|_| NodeError::ArchiveCapability("inventory selector length".into()))?;
        self.backend
            .restore_inventory_record(&PortableInventoryRecord {
                selector: SelectorCid::from_bytes(selector),
                canonical_snapshot: record.bytes.clone(),
            })
            .map_err(|error| NodeError::Storage(format!("{error:?}")))
    }
}

/// Logical owned-blob adapter. Canonical live references are the authority;
/// unowned garbage is omitted, while already-uploaded bytes covered by a
/// pending owner lease remain recoverable together with that intent.
pub struct OwnedBlobArchiveBackend {
    storage: Arc<BlobStorage>,
    oracle: Arc<CanonicalBlobReferenceOracle>,
}

impl OwnedBlobArchiveBackend {
    pub const fn new(storage: Arc<BlobStorage>, oracle: Arc<CanonicalBlobReferenceOracle>) -> Self {
        Self { storage, oracle }
    }
}

impl SnapshotVerifiedBackend for OwnedBlobArchiveBackend {
    fn owns(&self, owner: ArchiveOwner) -> bool {
        owner == ArchiveOwner::BLOB
    }

    fn bounded_snapshot(&self) -> Result<Vec<ArchiveSnapshotRecord>, NodeError> {
        let required = self
            .oracle
            .canonical_retained_blob_cids()
            .map_err(|error| NodeError::Storage(error.to_string()))?
            .into_iter()
            .map(|cid| cid.0)
            .collect::<std::collections::BTreeSet<_>>();
        let mut found_required = std::collections::BTreeSet::new();
        let mut rows = Vec::new();
        for metadata in self
            .storage
            .list_blobs()
            .map_err(|error| NodeError::Storage(error.to_string()))?
        {
            let cid = BlobCid::from_hex(&metadata.blob_cid_hex).ok_or_else(|| {
                NodeError::ArchiveCapability("blob metadata contains an invalid CID".into())
            })?;
            let referenced = self
                .oracle
                .referencing_records(&cid)
                .map_err(|error| NodeError::Storage(error.to_string()))?;
            if referenced.is_empty() && !required.contains(&cid.0) {
                continue;
            }
            let bytes = self
                .storage
                .read_full_blob(&cid)
                .map_err(|error| NodeError::Storage(error.to_string()))?;
            if BlobCid::from_content(cid.blob_type(), &bytes) != cid {
                return Err(NodeError::ArchiveCapability(
                    "owned blob content does not match its CID".into(),
                ));
            }
            if required.contains(&cid.0) {
                found_required.insert(cid.0);
            }
            rows.push(ArchiveSnapshotRecord {
                kind: ArchiveEntryKind::OwnedBlob,
                owner: ArchiveOwner::BLOB,
                namespace: 1,
                key: cid.0.to_vec(),
                bytes,
                required: true,
            });
        }
        if found_required != required {
            return Err(NodeError::ArchiveCapability(
                "canonical owned-blob reference has no verified blob bytes".into(),
            ));
        }
        rows.sort_by(|left, right| left.key.cmp(&right.key));
        Ok(rows)
    }

    fn restore_validated(&self, record: &ArchiveSnapshotRecord) -> Result<(), NodeError> {
        if record.owner != ArchiveOwner::BLOB || record.kind != ArchiveEntryKind::OwnedBlob {
            return Err(NodeError::ArchiveCapability(
                "owned-blob adapter received the wrong logical kind".into(),
            ));
        }
        let bytes: [u8; 34] = record
            .key
            .as_slice()
            .try_into()
            .map_err(|_| NodeError::ArchiveCapability("owned blob CID length".into()))?;
        let cid = BlobCid(bytes);
        if cid.version() != BLOB_CID_VERSION
            || BlobCid::from_content(cid.blob_type(), &record.bytes) != cid
        {
            return Err(NodeError::ArchiveCapability(
                "owned blob archive row fails CID validation".into(),
            ));
        }
        let metadata = self
            .storage
            .store_bytes("restored-owned-blob", &record.bytes, cid.blob_type())
            .map_err(|error| NodeError::Storage(error.to_string()))?;
        if metadata.blob_cid_hex != cid.to_hex() {
            return Err(NodeError::ArchiveCapability(
                "owned blob restore produced a different CID".into(),
            ));
        }
        Ok(())
    }

    fn reconcile_after_restore(&self) -> Result<(), NodeError> {
        self.storage
            .recover_pending_filesystem_intents()
            .map(|_| ())
            .map_err(|error| NodeError::Storage(error.to_string()))
    }
}

/// Portable migration evidence adapter. The lower store owns semantic
/// validation/replay; this Node layer only maps one canonical logical payload
/// to the closed archive owner/kind.
pub struct MigrationArchiveBackend<T> {
    store: T,
}

impl<T> MigrationArchiveBackend<T> {
    pub const fn new(store: T) -> Self {
        Self { store }
    }
}

impl<T> SnapshotVerifiedBackend for MigrationArchiveBackend<T>
where
    T: MigrationStateSnapshotPort + ValidatedMigrationRestorePort + Send + Sync,
{
    fn owns(&self, owner: ArchiveOwner) -> bool {
        owner == ArchiveOwner::MIGRATION
    }

    fn bounded_snapshot(&self) -> Result<Vec<ArchiveSnapshotRecord>, NodeError> {
        let snapshot = self
            .store
            .portable_migration_snapshot()
            .map_err(NodeError::Storage)?;
        let bytes =
            serde_json::to_vec(&snapshot).map_err(|error| NodeError::Storage(error.to_string()))?;
        Ok(vec![ArchiveSnapshotRecord {
            kind: ArchiveEntryKind::MigrationState,
            owner: ArchiveOwner::MIGRATION,
            namespace: 1,
            key: b"migration-state-v1".to_vec(),
            bytes,
            required: true,
        }])
    }

    fn restore_validated(&self, record: &ArchiveSnapshotRecord) -> Result<(), NodeError> {
        if record.owner != ArchiveOwner::MIGRATION
            || record.kind != ArchiveEntryKind::MigrationState
            || record.key != b"migration-state-v1"
        {
            return Err(NodeError::ArchiveCapability(
                "migration adapter received the wrong logical kind".into(),
            ));
        }
        let snapshot: PortableMigrationSnapshot = serde_json::from_slice(&record.bytes)
            .map_err(|error| NodeError::Storage(error.to_string()))?;
        if serde_json::to_vec(&snapshot).map_err(|error| NodeError::Storage(error.to_string()))?
            != record.bytes
        {
            return Err(NodeError::ArchiveCapability(
                "migration archive payload is non-canonical".into(),
            ));
        }
        self.store
            .restore_migration_snapshot(&snapshot)
            .map_err(NodeError::Storage)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetRestoreReceipt {
    pub activation: DatasetGenerationReceipt,
    pub identity: IdentityRecoveryReceipt,
}

pub struct BaseArchiveReceipt {
    pub readable_sink: ReadableArchiveSinkHandle,
    pub manifest_root: [u8; 32],
}

/// Internal archive service. Task 17 wraps this service in durable
/// prepare/confirm/reconcile; Task 18 projects those operations to products.
pub struct BaseArchiveService {
    capabilities: ArchiveCapabilityRegistry,
    dataset_generations: Arc<DatasetGenerationStore>,
    backends: Vec<Arc<dyn SnapshotVerifiedBackend>>,
    portable_compatibility: PortableDataCompatibilityV1,
    limits: ArchiveLimits,
    spool_factory: FileSecureSpoolFactory,
    signer_registry: Option<Arc<dyn SignerProviderRegistry>>,
    quiesce: Arc<Mutex<()>>,
    restore_backend_factory: Option<Arc<dyn StagedArchiveBackendFactory>>,
}

impl BaseArchiveService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        capabilities: ArchiveCapabilityRegistry,
        dataset_generations: Arc<DatasetGenerationStore>,
        backends: Vec<Arc<dyn SnapshotVerifiedBackend>>,
        portable_compatibility: PortableDataCompatibilityV1,
        limits: ArchiveLimits,
        spool_directory: impl AsRef<Path>,
        signer_registry: Option<Arc<dyn SignerProviderRegistry>>,
        quiesce: Arc<Mutex<()>>,
    ) -> Result<Self, NodeError> {
        if backends.is_empty() {
            return Err(NodeError::ArchiveCapability(
                "archive service requires at least one logical backend".into(),
            ));
        }
        Ok(Self {
            capabilities,
            dataset_generations,
            backends,
            portable_compatibility,
            limits,
            spool_factory: FileSecureSpoolFactory::new(spool_directory)?,
            signer_registry,
            quiesce,
            restore_backend_factory: None,
        })
    }

    pub fn with_restore_backend_factory(
        mut self,
        factory: Arc<dyn StagedArchiveBackendFactory>,
    ) -> Self {
        self.restore_backend_factory = Some(factory);
        self
    }

    pub fn capabilities(&self) -> &ArchiveCapabilityRegistry {
        &self.capabilities
    }

    pub async fn create_archive(
        &self,
        destination: WritableArchiveSinkHandle,
        credential: ArchiveSecretHandle,
        producer: ProducerArtifactIdentityV1,
    ) -> Result<BaseArchiveReceipt, NodeError> {
        archive_failpoint("before_begin_write")?;
        let reservation = destination.owner_reservation();
        if credential.owner_reservation() != reservation {
            return Err(NodeError::ArchiveCapability(
                "archive sink and credential belong to different reservations".into(),
            ));
        }
        let credential = self.capabilities.take_credential(credential, reservation)?;
        archive_failpoint("after_begin_write_before_mutation")?;
        let outcome = (|| {
            let _quiesce = self.quiesce.try_lock().map_err(|_| {
                NodeError::ArchiveCapability("archive snapshot quiesce is unavailable".into())
            })?;
            let source = CompositeSnapshotSource::new(
                self.backends.clone(),
                self.portable_compatibility,
                producer,
                self.limits,
            );
            let captured = capture_dataset(&source, self.portable_compatibility, producer)?;
            let manifest_root = captured.manifest.aggregate_root;
            let plaintext = captured.canonical_plaintext()?;
            let mut encrypted = Vec::new();
            seal_archive(
                Cursor::new(plaintext),
                &mut encrypted,
                &credential,
                &self.limits,
            )?;
            archive_failpoint("after_mutation_before_commit")?;
            Ok::<_, NodeError>((manifest_root, encrypted))
        })();
        let (manifest_root, encrypted) = match outcome {
            Ok(value) => value,
            Err(error) => {
                self.capabilities.discard_writable_sink(destination);
                return Err(error);
            }
        };
        let readable_sink = self
            .capabilities
            .publish_sink(destination, reservation, encrypted)?;
        if let Err(error) = archive_failpoint("after_commit_before_next_side_effect") {
            drop(readable_sink);
            return Err(error);
        }
        if let Err(error) = archive_failpoint("after_next_side_effect_before_ack") {
            drop(readable_sink);
            return Err(error);
        }
        Ok(BaseArchiveReceipt {
            readable_sink,
            manifest_root,
        })
    }

    pub async fn restore_archive(
        &self,
        archive: SealedArchiveSourceHandle,
        credential: ArchiveSecretHandle,
        expected: &ArchiveRestorePolicyV1,
    ) -> Result<DatasetRestoreReceipt, NodeError> {
        let factory = self.restore_backend_factory.as_ref().ok_or_else(|| {
            NodeError::ArchiveCapability(
                "archive restore target backend factory is not configured".into(),
            )
        })?;
        let (reservation, bytes) = self.capabilities.take_source(archive)?;
        let credential = self.capabilities.take_credential(credential, reservation)?;
        let archive_digest = *blake3::hash(&bytes).as_bytes();
        let verified = verify_dataset_archive_v2(
            Cursor::new(bytes),
            &self.spool_factory,
            &credential,
            &self.limits,
        )?;
        let staged = self
            .dataset_generations
            .stage_verified_restore(verified, expected)
            .map_err(restore_error)?;
        let restore_result = (|| {
            let resolver = self
                .dataset_generations
                .staged_resolver(&staged)
                .map_err(restore_error)?;
            let target_backends = factory.open_for_staged_generation(&resolver)?;
            let records = self.staged_records(&staged)?;
            restore_records(&target_backends, &records)
        })();
        if let Err(error) = restore_result {
            self.dataset_generations
                .discard_staged_identity_failure(&staged)
                .map_err(restore_error)?;
            return Err(error);
        }
        let ready = recover_staged_identity(
            &self.dataset_generations,
            staged,
            self.signer_registry.as_deref(),
        )
        .map_err(restore_error)?;
        let operation = restore_binding(reservation, archive_digest);
        self.dataset_generations
            .activate_restore(ready, operation)
            .map_err(restore_error)
    }

    /// Applies already-verified logical rows through every target validation
    /// port and reconciles durable intents before admission.
    pub fn restore_logical_backends(
        &self,
        records: &[ArchiveSnapshotRecord],
    ) -> Result<(), NodeError> {
        restore_records(&self.backends, records)
    }

    fn staged_records(
        &self,
        staged: &StagedDatasetGeneration,
    ) -> Result<Vec<ArchiveSnapshotRecord>, NodeError> {
        let mut records = self
            .dataset_generations
            .staged_manifest(staged)
            .entries
            .iter()
            .filter(|entry| entry.kind != ArchiveEntryKind::SignerRecoveryPolicy)
            .map(|entry| {
                Ok(ArchiveSnapshotRecord {
                    kind: entry.kind,
                    owner: entry.logical_key.owner,
                    namespace: entry.logical_key.namespace,
                    key: entry.logical_key.key.as_slice().to_vec(),
                    bytes: self
                        .dataset_generations
                        .staged_entry_payload(staged, entry)
                        .map_err(restore_error)?,
                    required: entry.required,
                })
            })
            .collect::<Result<Vec<_>, NodeError>>()?;
        records.sort_by_key(|record| record.kind as u16);
        Ok(records)
    }
}

fn restore_records(
    backends: &[Arc<dyn SnapshotVerifiedBackend>],
    records: &[ArchiveSnapshotRecord],
) -> Result<(), NodeError> {
    for record in records {
        let backend = backends
            .iter()
            .find(|backend| backend.owns(record.owner))
            .ok_or_else(|| {
                NodeError::ArchiveCapability(format!(
                    "no validated restore adapter for owner {}",
                    record.owner.get()
                ))
            })?;
        backend.restore_validated(record)?;
    }
    for backend in backends {
        backend.reconcile_after_restore()?;
    }
    Ok(())
}

struct CachedSnapshot {
    lease: SnapshotLease,
    entries: Vec<ArchiveEntryV1>,
    payloads: BTreeMap<ArchiveEntryId, Vec<u8>>,
}

struct CompositeSnapshotSource {
    backends: Vec<Arc<dyn SnapshotVerifiedBackend>>,
    compatibility: PortableDataCompatibilityV1,
    producer: ProducerArtifactIdentityV1,
    limits: ArchiveLimits,
    cached: Mutex<Option<CachedSnapshot>>,
}

impl CompositeSnapshotSource {
    fn new(
        backends: Vec<Arc<dyn SnapshotVerifiedBackend>>,
        compatibility: PortableDataCompatibilityV1,
        producer: ProducerArtifactIdentityV1,
        limits: ArchiveLimits,
    ) -> Self {
        Self {
            backends,
            compatibility,
            producer,
            limits,
            cached: Mutex::new(None),
        }
    }

    fn scan(&self) -> Result<CachedSnapshot, ArchiveError> {
        let mut rows = Vec::new();
        for backend in &self.backends {
            rows.extend(
                backend
                    .bounded_snapshot()
                    .map_err(|error| ArchiveError::RestoreSink(error.to_string()))?,
            );
        }
        if rows.is_empty() || rows.len() > MAX_SNAPSHOT_RECORDS {
            return Err(ArchiveError::Limit);
        }
        let mut total = 0u64;
        let mut entries = Vec::with_capacity(rows.len());
        let mut payloads = BTreeMap::new();
        for row in rows {
            total = total
                .checked_add(row.bytes.len() as u64)
                .ok_or(ArchiveError::Limit)?;
            if row.bytes.is_empty()
                || row.bytes.len() as u64 > self.limits.max_entry_bytes
                || total > self.limits.max_total_plaintext_bytes
            {
                return Err(ArchiveError::Limit);
            }
            let logical_key = ArchiveLogicalKey::new(row.owner, row.namespace, row.key)?;
            let entry = ArchiveEntryV1::new(
                row.kind,
                logical_key,
                row.bytes.len() as u64,
                *blake3::hash(&row.bytes).as_bytes(),
                row.required,
            )?;
            if payloads.insert(entry.id, row.bytes).is_some() {
                return Err(ArchiveError::Integrity);
            }
            entries.push(entry);
        }
        let manifest = DatasetManifestV1::build(self.compatibility, self.producer, entries)?;
        let binding = snapshot_binding(&manifest.entries, &payloads);
        Ok(CachedSnapshot {
            lease: SnapshotLease {
                dataset_generation: 1,
                canonical_source_root: manifest.canonical_root,
                high_water_root: compute_high_water_root(&manifest.entries),
                blob_generation: 1,
                retention_generation: 1,
                source_binding: binding,
            },
            entries: manifest.entries,
            payloads,
        })
    }
}

impl SnapshotSource for CompositeSnapshotSource {
    fn acquire_snapshot(&self) -> Result<SnapshotLease, ArchiveError> {
        let snapshot = self.scan()?;
        let lease = snapshot.lease.clone();
        *self.cached.lock().map_err(|_| ArchiveError::Integrity)? = Some(snapshot);
        Ok(lease)
    }

    fn entries(&self, lease: &SnapshotLease) -> Result<Vec<ArchiveEntryV1>, ArchiveError> {
        let cached = self.cached.lock().map_err(|_| ArchiveError::Integrity)?;
        let cached = cached.as_ref().ok_or(ArchiveError::Integrity)?;
        if &cached.lease != lease {
            return Err(ArchiveError::Integrity);
        }
        Ok(cached.entries.clone())
    }

    fn read_entry(
        &self,
        lease: &SnapshotLease,
        id: ArchiveEntryId,
    ) -> Result<Box<dyn Read>, ArchiveError> {
        let cached = self.cached.lock().map_err(|_| ArchiveError::Integrity)?;
        let cached = cached.as_ref().ok_or(ArchiveError::Integrity)?;
        if &cached.lease != lease {
            return Err(ArchiveError::Integrity);
        }
        Ok(Box::new(Cursor::new(
            cached
                .payloads
                .get(&id)
                .cloned()
                .ok_or(ArchiveError::Integrity)?,
        )))
    }

    fn validate_snapshot(&self, lease: &SnapshotLease) -> Result<(), ArchiveError> {
        let fresh = self.scan()?;
        if &fresh.lease != lease {
            return Err(ArchiveError::Integrity);
        }
        Ok(())
    }
}

fn snapshot_binding(
    entries: &[ArchiveEntryV1],
    payloads: &BTreeMap<ArchiveEntryId, Vec<u8>>,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key(SNAPSHOT_BINDING_DOMAIN);
    hasher.update(&(entries.len() as u64).to_be_bytes());
    for entry in entries {
        hasher.update(entry.id.as_bytes());
        hasher.update(&entry.length.to_be_bytes());
        hasher.update(&entry.blake3);
        if let Some(payload) = payloads.get(&entry.id) {
            hasher.update(payload);
        }
    }
    *hasher.finalize().as_bytes()
}

fn restore_binding(
    reservation: ArchiveOperationReservationId,
    archive_digest: [u8; 32],
) -> RestoreOperationBinding {
    let mut input = Vec::with_capacity(64);
    input.extend_from_slice(reservation.as_bytes());
    input.extend_from_slice(&archive_digest);
    RestoreOperationBinding {
        operation_id: *reservation.as_bytes(),
        idempotency_key: blake3::derive_key(RESTORE_IDEMPOTENCY_DOMAIN, &input),
    }
}

fn restore_error(error: crate::dataset_generation::RestoreError) -> NodeError {
    NodeError::ArchiveCapability(format!("archive restore failed: {error}"))
}

fn archive_failpoint(phase: &str) -> Result<(), NodeError> {
    if std::env::var("ONEBRAIN_ARCHIVE_FAILPOINT").ok().as_deref() == Some(phase) {
        return Err(NodeError::ArchiveCapability(format!(
            "TX-ARCH-001 failpoint: {phase}"
        )));
    }
    Ok(())
}

fn archive_kind(kind: StoredRecordKind) -> ArchiveEntryKind {
    match kind {
        StoredRecordKind::Object => ArchiveEntryKind::CanonicalObject,
        StoredRecordKind::Event => ArchiveEntryKind::CanonicalEvent,
        StoredRecordKind::FeedInception => ArchiveEntryKind::FeedInception,
        StoredRecordKind::AuthorityEvent => ArchiveEntryKind::AuthorityEvent,
    }
}

fn decode_accepted_key(key: &[u8]) -> Result<(StoredRecordKind, [u8; 32]), NodeError> {
    let (&kind, cid) = key
        .split_first()
        .ok_or_else(|| NodeError::ArchiveCapability("canonical archive key is empty".into()))?;
    let record_kind = match kind {
        1 => StoredRecordKind::Object,
        2 => StoredRecordKind::Event,
        3 => StoredRecordKind::FeedInception,
        4 => StoredRecordKind::AuthorityEvent,
        _ => {
            return Err(NodeError::ArchiveCapability(
                "canonical archive key has an unknown kind".into(),
            ))
        }
    };
    let cid = cid
        .try_into()
        .map_err(|_| NodeError::ArchiveCapability("canonical archive CID length".into()))?;
    Ok((record_kind, cid))
}

fn encode_quarantine(record: &QuarantineRecord) -> Result<Vec<u8>, NodeError> {
    let reason = record.reason_code.as_bytes();
    let reason_length = u16::try_from(reason.len())
        .map_err(|_| NodeError::ArchiveCapability("quarantine reason is too long".into()))?;
    let original_length = u64::try_from(record.original_bytes.len())
        .map_err(|_| NodeError::ArchiveCapability("quarantine payload is too long".into()))?;
    let mut bytes =
        Vec::with_capacity(1 + 32 + 32 + 2 + reason.len() + 8 + record.original_bytes.len());
    bytes.push(record.record_kind as u8);
    bytes.extend_from_slice(&record.claimed_cid);
    bytes.extend_from_slice(&record.quarantine_id);
    bytes.extend_from_slice(&reason_length.to_be_bytes());
    bytes.extend_from_slice(reason);
    bytes.extend_from_slice(&original_length.to_be_bytes());
    bytes.extend_from_slice(&record.original_bytes);
    Ok(bytes)
}

fn decode_quarantine(bytes: &[u8]) -> Result<QuarantineRecord, NodeError> {
    let mut decoder = ArchiveRowDecoder::new(bytes);
    let record_kind = match decoder.byte()? {
        1 => StoredRecordKind::Object,
        2 => StoredRecordKind::Event,
        3 => StoredRecordKind::FeedInception,
        4 => StoredRecordKind::AuthorityEvent,
        _ => {
            return Err(NodeError::ArchiveCapability(
                "quarantine row has an unknown kind".into(),
            ))
        }
    };
    let claimed_cid = decoder.array()?;
    let quarantine_id = decoder.array()?;
    let reason_length = decoder.u16()? as usize;
    let reason = String::from_utf8(decoder.take(reason_length)?.to_vec())
        .map_err(|_| NodeError::ArchiveCapability("quarantine reason is not UTF-8".into()))?;
    let original_length = usize::try_from(decoder.u64()?)
        .map_err(|_| NodeError::ArchiveCapability("quarantine payload length".into()))?;
    let original_bytes = decoder.take(original_length)?.to_vec();
    decoder.finish()?;
    Ok(QuarantineRecord {
        quarantine_id,
        record_kind,
        claimed_cid,
        reason_code: reason,
        original_bytes,
    })
}

struct ArchiveRowDecoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> ArchiveRowDecoder<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], NodeError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| NodeError::ArchiveCapability("archive row overflow".into()))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| NodeError::ArchiveCapability("archive row is truncated".into()))?;
        self.offset = end;
        Ok(value)
    }

    fn byte(&mut self) -> Result<u8, NodeError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, NodeError> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().map_err(
            |_| NodeError::ArchiveCapability("archive row u16".into()),
        )?))
    }

    fn u64(&mut self) -> Result<u64, NodeError> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().map_err(
            |_| NodeError::ArchiveCapability("archive row u64".into()),
        )?))
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], NodeError> {
        self.take(N)?
            .try_into()
            .map_err(|_| NodeError::ArchiveCapability("archive row array".into()))
    }

    fn finish(self) -> Result<(), NodeError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(NodeError::ArchiveCapability(
                "archive row has trailing bytes".into(),
            ))
        }
    }
}

/// Type-level assertion retained near the service boundary: restore accepts a
/// staged generation only after complete archive authentication.
fn _staged_generation_is_not_publicly_constructible(_: StagedDatasetGeneration) {}
