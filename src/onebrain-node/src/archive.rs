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
use onebrain_blob_store::blob_storage::{BlobReferenceOracle as _, BlobStorage};

use crate::activation_journal::ActivationOperationContext;
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
const LOGICAL_ROW_KEY_DOMAIN: &str = "onebrain:base:logical-row-key:1";
const LOGICAL_ROW_ENVELOPE_DOMAIN: &str = "onebrain:base:logical-row-envelope:1";
const LOGICAL_ROW_ENVELOPE_MAGIC: &[u8; 8] = b"OBLRV001";
const LOGICAL_ROW_KEY_PREFIX: &[u8] = b"row-v1:";
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

/// Opens the five durable logical stores owned by the vNext network runtime
/// inside a dataset generation.  The same factory is used for the active
/// source generation and the unactivated restore target, so an OBARV002
/// restore can never accidentally write back into the generation being
/// replaced.
#[cfg(feature = "vnext-network-runtime")]
#[derive(Clone, Copy, Debug, Default)]
pub struct VNextStagedArchiveBackendFactory;

#[cfg(feature = "vnext-network-runtime")]
impl StagedArchiveBackendFactory for VNextStagedArchiveBackendFactory {
    fn open_for_staged_generation(
        &self,
        resolver: &dyn DatasetPathResolver,
    ) -> Result<Vec<Arc<dyn SnapshotVerifiedBackend>>, NodeError> {
        use ku_core::foundation::RedbVerifiedBackend;
        use ku_net::vnext_inventory_forest::RedbInventoryForestBackend;
        use ku_net::vnext_reconciliation_journal::RedbReconciliationJournalBackend;

        use crate::dataset_path::BaseStorageOwnerId;
        use crate::vnext_outbox::OutboundOutbox;
        use crate::vnext_record_provenance::RedbRecordProvenance;
        use crate::vnext_validated_sink::{SharedVNextValidatedSink, VNextValidatedSink};

        let canonical = resolver
            .owner_path(BaseStorageOwnerId::CANONICAL)
            .map_err(|error| NodeError::Storage(error.to_string()))?
            .join("vnext_verified.redb");
        let reconciliation = resolver
            .owner_path(BaseStorageOwnerId::RECONCILIATION)
            .map_err(|error| NodeError::Storage(error.to_string()))?
            .join("vnext_reconciliation.redb");
        let inventory = resolver
            .owner_path(BaseStorageOwnerId::INVENTORY)
            .map_err(|error| NodeError::Storage(error.to_string()))?
            .join("vnext_inventory.redb");
        let outbox = resolver
            .owner_path(BaseStorageOwnerId::OUTBOX)
            .map_err(|error| NodeError::Storage(error.to_string()))?
            .join("vnext_outbox.redb");
        let provenance = resolver
            .owner_path(BaseStorageOwnerId::PROVENANCE)
            .map_err(|error| NodeError::Storage(error.to_string()))?
            .join("vnext_record_provenance.redb");

        let sink = SharedVNextValidatedSink::new(VNextValidatedSink::new(
            RedbVerifiedBackend::open(&canonical).map_err(NodeError::Storage)?,
        ));
        let reconciliation =
            RedbReconciliationJournalBackend::open(&reconciliation).map_err(NodeError::Storage)?;
        let inventory = RedbInventoryForestBackend::open(&inventory)
            .map_err(|error| NodeError::Storage(format!("{error:?}")))?;
        let outbox =
            OutboundOutbox::open(&outbox).map_err(|error| NodeError::Storage(error.to_string()))?;
        let provenance = RedbRecordProvenance::open(&provenance).map_err(NodeError::Storage)?;

        let private_kql_dir = resolver
            .owner_path(BaseStorageOwnerId::PRIVATE_KQL)
            .map_err(|error| NodeError::Storage(error.to_string()))?;
        let private_pomv_dir = resolver
            .owner_path(BaseStorageOwnerId::PRIVATE_POMV)
            .map_err(|error| NodeError::Storage(error.to_string()))?;
        let operational_dir = resolver
            .owner_path(BaseStorageOwnerId::OPERATIONAL)
            .map_err(|error| NodeError::Storage(error.to_string()))?;
        let rollout_dir = resolver
            .owner_path(BaseStorageOwnerId::ROLLOUT)
            .map_err(|error| NodeError::Storage(error.to_string()))?;
        let optional_net_dir = resolver
            .owner_path(BaseStorageOwnerId::OPTIONAL_NETWORK)
            .map_err(|error| NodeError::Storage(error.to_string()))?;

        Ok(vec![
            Arc::new(ValidatedCanonicalArchiveBackend::new(sink)),
            Arc::new(ReconciliationArchiveBackend::new(reconciliation)),
            Arc::new(InventoryArchiveBackend::new(inventory)),
            Arc::new(LogicalRowsArchiveBackend::new(outbox)),
            Arc::new(LogicalRowsArchiveBackend::new(provenance)),
            Arc::new(RolloutArchiveBackend::new(rollout_dir)),
            Arc::new(LazyPomvArchiveBackend::new(private_pomv_dir)),
            Arc::new(OptionalNetworkArchiveStores::new(optional_net_dir)),
            Arc::new(LazyPrivateKqlArchiveBackend::new(private_kql_dir)),
            Arc::new(LazyOperationalArchiveBackend::new(operational_dir)),
        ])
    }
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
                let key = logical_row_archive_key(row.table, &row.key);
                let bytes = encode_logical_row_envelope(&row)?;
                Ok(ArchiveSnapshotRecord {
                    kind,
                    owner,
                    namespace: 1,
                    key,
                    bytes,
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
        let row = decode_logical_row_envelope(&record.key, &record.bytes)?;
        self.store.restore_row(&row)
    }

    fn reconcile_after_restore(&self) -> Result<(), NodeError> {
        self.store.reconcile_restored_rows()
    }
}

fn logical_row_archive_key(table: u8, key: &[u8]) -> Vec<u8> {
    let mut binding = Vec::with_capacity(3 + key.len());
    binding.push(table);
    binding.extend_from_slice(&(key.len() as u16).to_be_bytes());
    binding.extend_from_slice(key);
    let digest = blake3::derive_key(LOGICAL_ROW_KEY_DOMAIN, &binding);
    let mut encoded = Vec::with_capacity(LOGICAL_ROW_KEY_PREFIX.len() + 2 + 1 + 64);
    encoded.extend_from_slice(LOGICAL_ROW_KEY_PREFIX);
    push_hex_byte(&mut encoded, table);
    encoded.push(b':');
    for byte in digest {
        push_hex_byte(&mut encoded, byte);
    }
    encoded
}

fn encode_logical_row_envelope(row: &PortableArchiveRow) -> Result<Vec<u8>, NodeError> {
    let key_length = u16::try_from(row.key.len()).map_err(|_| {
        NodeError::ArchiveCapability("logical archive row key exceeds the bound".into())
    })?;
    let value_length = u64::try_from(row.value.len()).map_err(|_| {
        NodeError::ArchiveCapability("logical archive row value exceeds the bound".into())
    })?;
    let mut encoded = Vec::with_capacity(19 + row.key.len() + row.value.len() + 32);
    encoded.extend_from_slice(LOGICAL_ROW_ENVELOPE_MAGIC);
    encoded.push(row.table);
    encoded.extend_from_slice(&key_length.to_be_bytes());
    encoded.extend_from_slice(&value_length.to_be_bytes());
    encoded.extend_from_slice(&row.key);
    encoded.extend_from_slice(&row.value);
    let checksum = blake3::derive_key(LOGICAL_ROW_ENVELOPE_DOMAIN, &encoded);
    encoded.extend_from_slice(&checksum);
    Ok(encoded)
}

fn decode_logical_row_envelope(
    archive_key: &[u8],
    encoded: &[u8],
) -> Result<PortableArchiveRow, NodeError> {
    const HEADER_BYTES: usize = 8 + 1 + 2 + 8;
    const CHECKSUM_BYTES: usize = 32;
    if encoded.len() < HEADER_BYTES + CHECKSUM_BYTES || &encoded[..8] != LOGICAL_ROW_ENVELOPE_MAGIC
    {
        return Err(NodeError::ArchiveCapability(
            "logical archive row envelope is malformed".into(),
        ));
    }
    let table = encoded[8];
    let key_length = u16::from_be_bytes([encoded[9], encoded[10]]) as usize;
    let value_length = u64::from_be_bytes(
        encoded[11..19]
            .try_into()
            .map_err(|_| NodeError::ArchiveCapability("logical row length is malformed".into()))?,
    );
    let value_length = usize::try_from(value_length).map_err(|_| {
        NodeError::ArchiveCapability("logical archive row value exceeds the bound".into())
    })?;
    let payload_end = HEADER_BYTES
        .checked_add(key_length)
        .and_then(|value| value.checked_add(value_length))
        .ok_or_else(|| {
            NodeError::ArchiveCapability("logical archive row length overflow".into())
        })?;
    if payload_end.checked_add(CHECKSUM_BYTES) != Some(encoded.len()) {
        return Err(NodeError::ArchiveCapability(
            "logical archive row length mismatch".into(),
        ));
    }
    let expected = blake3::derive_key(LOGICAL_ROW_ENVELOPE_DOMAIN, &encoded[..payload_end]);
    if encoded[payload_end..] != expected {
        return Err(NodeError::ArchiveCapability(
            "logical archive row checksum mismatch".into(),
        ));
    }
    let key = encoded[HEADER_BYTES..HEADER_BYTES + key_length].to_vec();
    if archive_key != logical_row_archive_key(table, &key) {
        return Err(NodeError::ArchiveCapability(
            "logical archive row key binding mismatch".into(),
        ));
    }
    Ok(PortableArchiveRow {
        table,
        key,
        value: encoded[HEADER_BYTES + key_length..payload_end].to_vec(),
    })
}

fn push_hex_byte(output: &mut Vec<u8>, byte: u8) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    output.push(HEX[(byte >> 4) as usize]);
    output.push(HEX[(byte & 0x0f) as usize]);
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

/// Portable private Vault adapter (`0x0002`).
///
/// Keeps `namespace == 1` 100% identical for `PrivateVault` (`base_gate_integration`
/// compatible) and optionally snapshots/restores `namespace == 2` for `KuStore`
/// (`ku-product-v1/journal.redb` rows when present), transitioning uncommitted
/// `D-062` `ku_public_share_prepared_v1` share-prepare previews to
/// `b"InvalidatedAcrossRestore"` across restore!
pub struct VaultArchiveBackend<T> {
    vault: T,
    ku_journal_dir: Option<std::path::PathBuf>,
}

impl<T> VaultArchiveBackend<T> {
    pub const fn new(vault: T) -> Self {
        Self {
            vault,
            ku_journal_dir: None,
        }
    }

    pub fn with_ku_journal_dir(mut self, ku_journal_dir: impl Into<std::path::PathBuf>) -> Self {
        self.ku_journal_dir = Some(ku_journal_dir.into());
        self
    }
}

const KU_JOURNAL_NS2_TABLES: [(&str, u8); 5] = [
    ("ku_private_journal_v1", 1),
    ("ku_private_extraction_v1", 2),
    ("ku_private_text_intake_v1", 3),
    ("ku_private_terminal_v1", 4),
    ("ku_public_share_prepared_v1", 5),
];

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

        if let Some(dir) = &self.ku_journal_dir {
            let journal_path = dir.join("journal.redb");
            if journal_path.exists() {
                let mut ns2_rows = snapshot_ku_journal_namespace2(&journal_path)?;
                rows.append(&mut ns2_rows);
            }
        }
        Ok(rows)
    }

    fn restore_validated(&self, record: &ArchiveSnapshotRecord) -> Result<(), NodeError> {
        if record.owner != ArchiveOwner::VAULT {
            return Err(NodeError::ArchiveCapability(
                "Vault adapter received the wrong owner".into(),
            ));
        }
        if record.namespace == 2 {
            let Some(dir) = &self.ku_journal_dir else {
                return Err(NodeError::ArchiveCapability(
                    "Vault namespace 2 KuStore journal directory is not configured".into(),
                ));
            };
            std::fs::create_dir_all(dir).map_err(|e| NodeError::Storage(e.to_string()))?;
            return restore_ku_journal_namespace2(&dir.join("journal.redb"), record);
        }
        if record.namespace != 1 {
            return Err(NodeError::ArchiveCapability(
                "Vault archive row has an unknown namespace".into(),
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

fn snapshot_ku_journal_namespace2(
    journal_path: &Path,
) -> Result<Vec<ArchiveSnapshotRecord>, NodeError> {
    use redb::ReadableTable as _;
    let db = redb::Database::open(journal_path).map_err(|e| NodeError::Storage(e.to_string()))?;
    let read = db
        .begin_read()
        .map_err(|e| NodeError::Storage(e.to_string()))?;
    let mut out = Vec::new();
    for (table_name, table_id) in KU_JOURNAL_NS2_TABLES {
        let def: redb::TableDefinition<&[u8], &[u8]> = redb::TableDefinition::new(table_name);
        let Ok(tbl) = read.open_table(def) else {
            continue;
        };
        for entry in tbl.iter().map_err(|e| NodeError::Storage(e.to_string()))? {
            let (k, v) = entry.map_err(|e| NodeError::Storage(e.to_string()))?;
            let row = PortableArchiveRow {
                table: table_id,
                key: k.value().to_vec(),
                value: v.value().to_vec(),
            };
            let key = logical_row_archive_key(row.table, &row.key);
            let bytes = encode_logical_row_envelope(&row)?;
            out.push(ArchiveSnapshotRecord {
                kind: ArchiveEntryKind::VaultRecord,
                owner: ArchiveOwner::VAULT,
                namespace: 2,
                key,
                bytes,
                required: true,
            });
        }
    }
    out.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(out)
}

fn restore_ku_journal_namespace2(
    journal_path: &Path,
    record: &ArchiveSnapshotRecord,
) -> Result<(), NodeError> {
    if record.kind != ArchiveEntryKind::VaultRecord {
        return Err(NodeError::ArchiveCapability(
            "Vault namespace 2 row kind mismatch".into(),
        ));
    }
    let row = decode_logical_row_envelope(&record.key, &record.bytes)?;
    let Some((table_name, _)) = KU_JOURNAL_NS2_TABLES
        .iter()
        .find(|(_, id)| *id == row.table)
    else {
        return Err(NodeError::ArchiveCapability(
            "Vault namespace 2 table ID is unknown".into(),
        ));
    };
    let db = redb::Database::create(journal_path).map_err(|e| NodeError::Storage(e.to_string()))?;
    let write = db
        .begin_write()
        .map_err(|e| NodeError::Storage(e.to_string()))?;
    {
        let def: redb::TableDefinition<&[u8], &[u8]> = redb::TableDefinition::new(table_name);
        let mut tbl = write
            .open_table(def)
            .map_err(|e| NodeError::Storage(e.to_string()))?;
        // D-062 uncommitted public share preparations transition to `InvalidatedAcrossRestore`
        // so an archive restore NEVER silently preserves or auto-resumes a share consent preview!
        let restored_value: &[u8] = if row.table == 5 {
            b"InvalidatedAcrossRestore"
        } else {
            row.value.as_slice()
        };
        tbl.insert(row.key.as_slice(), restored_value)
            .map_err(|e| NodeError::Storage(e.to_string()))?;
    }
    write
        .commit()
        .map_err(|e| NodeError::Storage(e.to_string()))
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
        self.restore_archive_bound(archive, credential, expected, None)
            .await
    }

    pub(crate) async fn restore_archive_for_base(
        &self,
        archive: SealedArchiveSourceHandle,
        credential: ArchiveSecretHandle,
        expected: &ArchiveRestorePolicyV1,
        operation: RestoreOperationBinding,
        operation_context: ActivationOperationContext,
    ) -> Result<DatasetRestoreReceipt, NodeError> {
        self.restore_archive_bound(
            archive,
            credential,
            expected,
            Some((operation, operation_context)),
        )
        .await
    }

    async fn restore_archive_bound(
        &self,
        archive: SealedArchiveSourceHandle,
        credential: ArchiveSecretHandle,
        expected: &ArchiveRestorePolicyV1,
        base_operation: Option<(RestoreOperationBinding, ActivationOperationContext)>,
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
        let operation = base_operation
            .as_ref()
            .map(|(operation, _)| *operation)
            .unwrap_or_else(|| restore_binding(reservation, archive_digest));
        if operation.operation_id != *reservation.as_bytes() {
            return Err(NodeError::ArchiveCapability(
                "restore capability is bound to a different Base operation".into(),
            ));
        }
        match base_operation {
            Some((operation, context)) => self
                .dataset_generations
                .activate_restore_with_context(ready, operation, context)
                .map_err(restore_error),
            None => self
                .dataset_generations
                .activate_restore(ready, operation)
                .map_err(restore_error),
        }
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
    // Wave 6 `P8` deterministic restore phase order:
    // 1. `0x000E` (`ROLLOUT`): conservative union of kill switches + max version fence,
    // 2. `0x000C` (`PRIVATE_POMV`): `max(local_seq_floor, archive_highest_issued_seq)` (`NEG-P8-001`),
    // 3. `0x000F` (`OPTIONAL_NETWORK`): `ProviderRetire` / replay floors + `reconciliation_restore_salt` rotation,
    // 4. `0x000B` (`PRIVATE_KQL`): wrapped `LocalNeedVaultKey` & tombstones BEFORE active `StandingNeed`s,
    // 5. `0x000D` (`OPERATIONAL`): overflow summary roots & cold tombstone epochs,
    // 6. `0x0001..0x000A` & remaining owners (with `AuthorityHighWater` verified after `AuthorityEvent`s).
    let mut ordered: Vec<&ArchiveSnapshotRecord> = records.iter().collect();
    ordered.sort_by(|left, right| {
        restore_owner_priority(left)
            .cmp(&restore_owner_priority(right))
            .then_with(|| (left.kind as u16).cmp(&(right.kind as u16)))
            .then_with(|| left.namespace.cmp(&right.namespace))
            .then_with(|| left.key.cmp(&right.key))
    });

    for record in ordered {
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

fn restore_owner_priority(record: &ArchiveSnapshotRecord) -> (u8, u8) {
    match record.owner {
        ArchiveOwner::ROLLOUT => (0, 0),
        ArchiveOwner::PRIVATE_POMV => (1, 0),
        ArchiveOwner::OPTIONAL_NETWORK => (2, 0),
        ArchiveOwner::PRIVATE_KQL => {
            // Within 0x000B: wrapped key metadata (namespace 2) & tombstones precede active needs!
            let sub = if record.namespace == 2 {
                0
            } else if is_private_kql_tombstone_row(record) {
                1
            } else {
                2
            };
            (3, sub)
        }
        ArchiveOwner::OPERATIONAL => (4, 0),
        ArchiveOwner::CANONICAL if record.kind == ArchiveEntryKind::AuthorityHighWater => (9, 1),
        _ => (5, 0),
    }
}

fn is_private_kql_tombstone_row(record: &ArchiveSnapshotRecord) -> bool {
    if let Ok(row) = decode_logical_row_envelope(&record.key, &record.bytes) {
        if row.table == 1 {
            // In `ku_kql::vnext_private_need::PrivateNeedRecord`, field 3 (`lifecycle`) is
            // `Canceled` (2) or `Retired` (3) with `Null` bundle for tombstones.
            if let Ok(ku_core::foundation::CanonicalValue::Map(fields)) =
                ku_core::foundation::decode_canonical(
                    &row.value,
                    ku_core::foundation::ResourceProfile::ObjectV1,
                )
            {
                return fields.iter().any(|(k, v)| {
                    *k == 3 && matches!(v, ku_core::foundation::CanonicalValue::Unsigned(2 | 3))
                });
            }
        }
    }
    false
}

// ─── Wave 6 `P8` Archive Owner Adapters (`0x000B..0x000F`) ───────────────────

/// `0x000E` (`ArchiveOwner::ROLLOUT`) adapter enforcing conservative union of kill switches
/// and `max(local_version_fence, archive_version_fence)`.
pub struct RolloutArchiveBackend {
    rollout_dir: std::path::PathBuf,
}

impl RolloutArchiveBackend {
    pub const FILE_NAME: &'static str = "vnext_extended_rollout.cbor";

    pub fn new(rollout_dir: impl Into<std::path::PathBuf>) -> Self {
        Self {
            rollout_dir: rollout_dir.into(),
        }
    }
}

impl SnapshotVerifiedBackend for RolloutArchiveBackend {
    fn owns(&self, owner: ArchiveOwner) -> bool {
        owner == ArchiveOwner::ROLLOUT
    }

    fn bounded_snapshot(&self) -> Result<Vec<ArchiveSnapshotRecord>, NodeError> {
        let path = self.rollout_dir.join(Self::FILE_NAME);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let raw = std::fs::read(&path).map_err(|e| NodeError::Storage(e.to_string()))?;
        let cfg =
            crate::vnext_extended_config::VNextExtendedFeatureConfig::decode_canonical_bytes(&raw)
                .map_err(|e| NodeError::ArchiveCapability(e.to_string()))?;
        let bytes = cfg
            .encode_canonical_bytes()
            .map_err(|e| NodeError::ArchiveCapability(e.to_string()))?;
        Ok(vec![ArchiveSnapshotRecord {
            kind: ArchiveEntryKind::RolloutRecord,
            owner: ArchiveOwner::ROLLOUT,
            namespace: 1,
            key: b"vnext-extended-rollout-v1".to_vec(),
            bytes,
            required: true,
        }])
    }

    fn restore_validated(&self, record: &ArchiveSnapshotRecord) -> Result<(), NodeError> {
        if record.owner != ArchiveOwner::ROLLOUT
            || record.kind != ArchiveEntryKind::RolloutRecord
            || record.key != b"vnext-extended-rollout-v1"
        {
            return Err(NodeError::ArchiveCapability(
                "0x000E rollout record key/kind invalid".into(),
            ));
        }
        let incoming =
            crate::vnext_extended_config::VNextExtendedFeatureConfig::decode_canonical_bytes(
                &record.bytes,
            )
            .map_err(|e| NodeError::ArchiveCapability(e.to_string()))?;
        std::fs::create_dir_all(&self.rollout_dir)
            .map_err(|e| NodeError::Storage(e.to_string()))?;
        let path = self.rollout_dir.join(Self::FILE_NAME);
        let merged = if path.exists() {
            let local_bytes =
                std::fs::read(&path).map_err(|e| NodeError::Storage(e.to_string()))?;
            let local =
                crate::vnext_extended_config::VNextExtendedFeatureConfig::decode_canonical_bytes(
                    &local_bytes,
                )
                .map_err(|e| NodeError::Storage(e.to_string()))?;
            local.merge_conservative_restore(&incoming)
        } else {
            incoming
        };
        let encoded = merged
            .encode_canonical_bytes()
            .map_err(|e| NodeError::Storage(e.to_string()))?;
        std::fs::write(&path, encoded).map_err(|e| NodeError::Storage(e.to_string()))
    }
}

/// Portable state persisted under `0x000F` (`ArchiveOwner::OPTIONAL_NETWORK`):
/// `ProviderRetire` generation floors, `RoutePacketReplayGuard` & `CognitiveTaskReplayGuard`
/// tombstones, and `reconciliation_restore_salt` (rotated deterministically on every restore!).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OptionalNetworkFloorSnapshot {
    pub provider_retire_floors: Vec<([u8; 32], u64)>,
    pub route_packet_replay_tags: Vec<([u8; 32], u64)>,
    pub cognitive_task_replay_entries: Vec<([u8; 32], [u8; 32])>,
    pub reconciliation_restore_salt: [u8; 32],
}

impl OptionalNetworkFloorSnapshot {
    pub fn to_canonical_bytes(&self) -> Result<Vec<u8>, NodeError> {
        use ku_core::foundation::{encode_canonical, CanonicalValue, ResourceProfile};
        let retires = self
            .provider_retire_floors
            .iter()
            .map(|(tuple_digest, floor)| {
                CanonicalValue::Array(vec![
                    CanonicalValue::Bytes(tuple_digest.to_vec()),
                    CanonicalValue::Unsigned(*floor),
                ])
            })
            .collect();
        let routes = self
            .route_packet_replay_tags
            .iter()
            .map(|(tag, exp)| {
                CanonicalValue::Array(vec![
                    CanonicalValue::Bytes(tag.to_vec()),
                    CanonicalValue::Unsigned(*exp),
                ])
            })
            .collect();
        let tasks = self
            .cognitive_task_replay_entries
            .iter()
            .map(|(tid, commit)| {
                CanonicalValue::Array(vec![
                    CanonicalValue::Bytes(tid.to_vec()),
                    CanonicalValue::Bytes(commit.to_vec()),
                ])
            })
            .collect();
        let root = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(1)),
            (1, CanonicalValue::Array(retires)),
            (2, CanonicalValue::Array(routes)),
            (3, CanonicalValue::Array(tasks)),
            (
                4,
                CanonicalValue::Bytes(self.reconciliation_restore_salt.to_vec()),
            ),
        ]);
        encode_canonical(&root, ResourceProfile::ObjectV1)
            .map_err(|e| NodeError::ArchiveCapability(format!("{e:?}")))
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, NodeError> {
        use ku_core::foundation::{decode_canonical, CanonicalValue, ResourceProfile};
        let val = decode_canonical(bytes, ResourceProfile::ObjectV1)
            .map_err(|e| NodeError::ArchiveCapability(format!("{e:?}")))?;
        let CanonicalValue::Map(fields) = val else {
            return Err(NodeError::ArchiveCapability("0x000F map".into()));
        };
        if fields.len() != 5 {
            return Err(NodeError::ArchiveCapability("0x000F field count".into()));
        }
        let parse_b32_u64 = |v: &CanonicalValue| -> Result<Vec<([u8; 32], u64)>, NodeError> {
            let CanonicalValue::Array(arr) = v else {
                return Err(NodeError::ArchiveCapability("0x000F array".into()));
            };
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                let CanonicalValue::Array(pair) = item else {
                    return Err(NodeError::ArchiveCapability("0x000F pair".into()));
                };
                if pair.len() != 2 {
                    return Err(NodeError::ArchiveCapability("0x000F pair len".into()));
                }
                let CanonicalValue::Bytes(b) = &pair[0] else {
                    return Err(NodeError::ArchiveCapability("0x000F bytes".into()));
                };
                let b32: [u8; 32] = b
                    .as_slice()
                    .try_into()
                    .map_err(|_| NodeError::ArchiveCapability("0x000F 32B".into()))?;
                let CanonicalValue::Unsigned(u) = pair[1] else {
                    return Err(NodeError::ArchiveCapability("0x000F u64".into()));
                };
                out.push((b32, u));
            }
            Ok(out)
        };
        let CanonicalValue::Array(task_arr) = &fields[3].1 else {
            return Err(NodeError::ArchiveCapability("0x000F tasks".into()));
        };
        let mut cognitive_task_replay_entries = Vec::with_capacity(task_arr.len());
        for item in task_arr {
            let CanonicalValue::Array(pair) = item else {
                return Err(NodeError::ArchiveCapability("0x000F task pair".into()));
            };
            if pair.len() != 2 {
                return Err(NodeError::ArchiveCapability("0x000F task pair len".into()));
            }
            let CanonicalValue::Bytes(a) = &pair[0] else {
                return Err(NodeError::ArchiveCapability("0x000F task id".into()));
            };
            let CanonicalValue::Bytes(b) = &pair[1] else {
                return Err(NodeError::ArchiveCapability("0x000F task commit".into()));
            };
            let tid: [u8; 32] = a
                .as_slice()
                .try_into()
                .map_err(|_| NodeError::ArchiveCapability("0x000F task id len".into()))?;
            let commit: [u8; 32] = b
                .as_slice()
                .try_into()
                .map_err(|_| NodeError::ArchiveCapability("0x000F commit len".into()))?;
            cognitive_task_replay_entries.push((tid, commit));
        }
        let CanonicalValue::Bytes(salt_b) = &fields[4].1 else {
            return Err(NodeError::ArchiveCapability("0x000F salt".into()));
        };
        let reconciliation_restore_salt: [u8; 32] = salt_b
            .as_slice()
            .try_into()
            .map_err(|_| NodeError::ArchiveCapability("0x000F salt len".into()))?;
        Ok(Self {
            provider_retire_floors: parse_b32_u64(&fields[1].1)?,
            route_packet_replay_tags: parse_b32_u64(&fields[2].1)?,
            cognitive_task_replay_entries,
            reconciliation_restore_salt,
        })
    }

    pub fn rotated_for_restore(&self, restore_entropy_binding: &[u8]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:reconciliation-restore-salt:1\0");
        hasher.update(&self.reconciliation_restore_salt);
        hasher.update(restore_entropy_binding);
        let mut next = self.clone();
        next.reconciliation_restore_salt = *hasher.finalize().as_bytes();
        next
    }
}

/// `0x000F` (`ArchiveOwner::OPTIONAL_NETWORK`) archive adapter.
pub struct OptionalNetworkArchiveStores {
    optional_net_dir: std::path::PathBuf,
}

impl OptionalNetworkArchiveStores {
    pub const FILE_NAME: &'static str = "vnext_optional_net_floors.cbor";

    pub fn new(optional_net_dir: impl Into<std::path::PathBuf>) -> Self {
        Self {
            optional_net_dir: optional_net_dir.into(),
        }
    }

    pub fn write_snapshot(&self, snapshot: &OptionalNetworkFloorSnapshot) -> Result<(), NodeError> {
        std::fs::create_dir_all(&self.optional_net_dir)
            .map_err(|e| NodeError::Storage(e.to_string()))?;
        let bytes = snapshot.to_canonical_bytes()?;
        std::fs::write(self.optional_net_dir.join(Self::FILE_NAME), bytes)
            .map_err(|e| NodeError::Storage(e.to_string()))
    }

    pub fn read_snapshot(&self) -> Result<Option<OptionalNetworkFloorSnapshot>, NodeError> {
        let path = self.optional_net_dir.join(Self::FILE_NAME);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&path).map_err(|e| NodeError::Storage(e.to_string()))?;
        OptionalNetworkFloorSnapshot::from_canonical_bytes(&bytes).map(Some)
    }
}

impl SnapshotVerifiedBackend for OptionalNetworkArchiveStores {
    fn owns(&self, owner: ArchiveOwner) -> bool {
        owner == ArchiveOwner::OPTIONAL_NETWORK
    }

    fn bounded_snapshot(&self) -> Result<Vec<ArchiveSnapshotRecord>, NodeError> {
        let Some(snap) = self.read_snapshot()? else {
            return Ok(Vec::new());
        };
        Ok(vec![ArchiveSnapshotRecord {
            kind: ArchiveEntryKind::OperationalRecord,
            owner: ArchiveOwner::OPTIONAL_NETWORK,
            namespace: 1,
            key: b"optional-network-floors-v1".to_vec(),
            bytes: snap.to_canonical_bytes()?,
            required: true,
        }])
    }

    fn restore_validated(&self, record: &ArchiveSnapshotRecord) -> Result<(), NodeError> {
        if record.owner != ArchiveOwner::OPTIONAL_NETWORK
            || record.kind != ArchiveEntryKind::OperationalRecord
            || record.key != b"optional-network-floors-v1"
        {
            return Err(NodeError::ArchiveCapability(
                "0x000F optional network row key/kind invalid".into(),
            ));
        }
        let incoming = OptionalNetworkFloorSnapshot::from_canonical_bytes(&record.bytes)?;
        // Merge retire floors monotonically & rotate `reconciliation_restore_salt`!
        let mut merged = if let Some(local) = self.read_snapshot()? {
            let mut retire_map: BTreeMap<[u8; 32], u64> =
                local.provider_retire_floors.into_iter().collect();
            for (k, v) in incoming.provider_retire_floors {
                retire_map
                    .entry(k)
                    .and_modify(|cur| *cur = (*cur).max(v))
                    .or_insert(v);
            }
            let mut route_map: BTreeMap<[u8; 32], u64> =
                local.route_packet_replay_tags.into_iter().collect();
            for (k, v) in incoming.route_packet_replay_tags {
                route_map
                    .entry(k)
                    .and_modify(|cur| *cur = (*cur).max(v))
                    .or_insert(v);
            }
            let mut task_map: BTreeMap<[u8; 32], [u8; 32]> =
                local.cognitive_task_replay_entries.into_iter().collect();
            for (tid, commit) in incoming.cognitive_task_replay_entries {
                if let Some(existing) = task_map.get(&tid) {
                    if *existing != commit {
                        return Err(NodeError::ArchiveCapability(
                            "0x000F cognitive task replay conflict".into(),
                        ));
                    }
                } else {
                    task_map.insert(tid, commit);
                }
            }
            OptionalNetworkFloorSnapshot {
                provider_retire_floors: retire_map.into_iter().collect(),
                route_packet_replay_tags: route_map.into_iter().collect(),
                cognitive_task_replay_entries: task_map.into_iter().collect(),
                reconciliation_restore_salt: incoming.reconciliation_restore_salt,
            }
        } else {
            incoming
        };
        let binding = self.optional_net_dir.to_string_lossy();
        merged = merged.rotated_for_restore(binding.as_bytes());
        self.write_snapshot(&merged)
    }
}

#[cfg(feature = "vnext-network-runtime")]
struct LazyPomvArchiveBackend {
    dir: std::path::PathBuf,
}

#[cfg(feature = "vnext-network-runtime")]
impl LazyPomvArchiveBackend {
    fn new(dir: std::path::PathBuf) -> Self {
        Self { dir }
    }

    fn open_adapter(
        &self,
    ) -> Result<
        LogicalRowsArchiveBackend<crate::vnext_distributed_pomv::PomvArchiveStores>,
        NodeError,
    > {
        let publisher = Arc::new(
            crate::vnext_distributed_pomv::PublicUseEvidencePublisher::open(&self.dir)
                .map_err(|e| NodeError::Storage(e.to_string()))?,
        );
        let policies = crate::vnext_route_authority::LocalPolicyRegistry::new(std::iter::empty())
            .map_err(|e| NodeError::Storage(e.to_string()))?;
        let runtime = Arc::new(
            crate::vnext_distributed_pomv::DistributedPomvRuntime::open(&self.dir, 1_024, policies)
                .map_err(|e| NodeError::Storage(e.to_string()))?,
        );
        Ok(LogicalRowsArchiveBackend::new(
            crate::vnext_distributed_pomv::PomvArchiveStores::new(publisher, runtime),
        ))
    }
}

#[cfg(feature = "vnext-network-runtime")]
impl SnapshotVerifiedBackend for LazyPomvArchiveBackend {
    fn owns(&self, owner: ArchiveOwner) -> bool {
        owner == ArchiveOwner::PRIVATE_POMV
    }

    fn bounded_snapshot(&self) -> Result<Vec<ArchiveSnapshotRecord>, NodeError> {
        if !self.dir.join("vnext_public_use_sender.redb").exists()
            && !self.dir.join("vnext_distributed_pomv.redb").exists()
        {
            return Ok(Vec::new());
        }
        self.open_adapter()?.bounded_snapshot()
    }

    fn restore_validated(&self, record: &ArchiveSnapshotRecord) -> Result<(), NodeError> {
        self.open_adapter()?.restore_validated(record)
    }

    fn reconcile_after_restore(&self) -> Result<(), NodeError> {
        if !self.dir.join("vnext_public_use_sender.redb").exists()
            && !self.dir.join("vnext_distributed_pomv.redb").exists()
        {
            return Ok(());
        }
        self.open_adapter()?.reconcile_after_restore()
    }
}

#[cfg(feature = "vnext-network-runtime")]
struct LazyPrivateKqlArchiveBackend {
    dir: std::path::PathBuf,
    vault_key_override: Option<[u8; 32]>,
}

#[cfg(feature = "vnext-network-runtime")]
impl LazyPrivateKqlArchiveBackend {
    const WRAPPED_KEY_FILE: &'static str = "wrapped_local_need_vault_key.bin";

    fn new(dir: std::path::PathBuf) -> Self {
        Self {
            dir,
            vault_key_override: None,
        }
    }

    fn derive_generation_key(&self) -> ku_kql::vnext_private_need::LocalNeedVaultKey {
        if let Some(arr) = self.vault_key_override {
            return ku_kql::vnext_private_need::LocalNeedVaultKey::from_bytes(arr);
        }
        let wrapped_path = self.dir.join(Self::WRAPPED_KEY_FILE);
        if let Ok(bytes) = std::fs::read(&wrapped_path) {
            if let Ok(arr) = <[u8; 32]>::try_from(bytes.as_slice()) {
                if arr != [0; 32] {
                    return ku_kql::vnext_private_need::LocalNeedVaultKey::from_bytes(arr);
                }
            }
        }
        let derived = blake3::derive_key(
            "onebrain:vnext:staged-private-kql-key:1",
            self.dir.to_string_lossy().as_bytes(),
        );
        ku_kql::vnext_private_need::LocalNeedVaultKey::from_bytes(derived)
    }
}

#[cfg(feature = "vnext-network-runtime")]
impl SnapshotVerifiedBackend for LazyPrivateKqlArchiveBackend {
    fn owns(&self, owner: ArchiveOwner) -> bool {
        owner == ArchiveOwner::PRIVATE_KQL
    }

    fn bounded_snapshot(&self) -> Result<Vec<ArchiveSnapshotRecord>, NodeError> {
        if !self.dir.join("vnext_private_need_vault.redb").exists()
            && !self.dir.join("vnext_distributed_kql.redb").exists()
        {
            return Ok(Vec::new());
        }
        let kql = crate::vnext_distributed_kql::DistributedKqlRuntime::open(
            &self.dir,
            self.derive_generation_key(),
        )
        .map_err(|e| NodeError::Storage(e.to_string()))?;
        let mut rows = LogicalRowsArchiveBackend::new(kql).bounded_snapshot()?;
        let wrapped_path = self.dir.join(Self::WRAPPED_KEY_FILE);
        if wrapped_path.exists() {
            let key_bytes =
                std::fs::read(&wrapped_path).map_err(|e| NodeError::Storage(e.to_string()))?;
            if key_bytes.len() == 32 {
                rows.push(ArchiveSnapshotRecord {
                    kind: ArchiveEntryKind::PrivateNeedRecord,
                    owner: ArchiveOwner::PRIVATE_KQL,
                    namespace: 2,
                    key: b"wrapped-local-need-vault-key-v1".to_vec(),
                    bytes: key_bytes,
                    required: true,
                });
            }
        }
        Ok(rows)
    }

    fn restore_validated(&self, record: &ArchiveSnapshotRecord) -> Result<(), NodeError> {
        if record.namespace == 2 && record.key == b"wrapped-local-need-vault-key-v1" {
            if record.bytes.len() != 32 || record.bytes.as_slice() == [0; 32] {
                return Err(NodeError::ArchiveCapability(
                    "invalid wrapped LocalNeedVaultKey".into(),
                ));
            }
            std::fs::create_dir_all(&self.dir).map_err(|e| NodeError::Storage(e.to_string()))?;
            return std::fs::write(self.dir.join(Self::WRAPPED_KEY_FILE), &record.bytes)
                .map_err(|e| NodeError::Storage(e.to_string()));
        }
        let kql = crate::vnext_distributed_kql::DistributedKqlRuntime::open(
            &self.dir,
            self.derive_generation_key(),
        )
        .map_err(|e| NodeError::Storage(e.to_string()))?;
        LogicalRowsArchiveBackend::new(kql).restore_validated(record)
    }
}

#[cfg(feature = "vnext-network-runtime")]
struct LazyOperationalArchiveBackend {
    dir: std::path::PathBuf,
}

#[cfg(feature = "vnext-network-runtime")]
impl LazyOperationalArchiveBackend {
    fn new(dir: std::path::PathBuf) -> Self {
        Self { dir }
    }
}

#[cfg(feature = "vnext-network-runtime")]
impl SnapshotVerifiedBackend for LazyOperationalArchiveBackend {
    fn owns(&self, owner: ArchiveOwner) -> bool {
        owner == ArchiveOwner::OPERATIONAL
    }

    fn bounded_snapshot(&self) -> Result<Vec<ArchiveSnapshotRecord>, NodeError> {
        let db_path = self.dir.join("vnext_operational_compaction.redb");
        if !db_path.exists() {
            return Ok(Vec::new());
        }
        let store = crate::vnext_operational_compaction::OperationalCompactionStore::open(
            &db_path,
            crate::vnext_operational_compaction::OperationalCompactionPolicy::default(),
        )
        .map_err(|e| NodeError::Storage(e.to_string()))?;
        LogicalRowsArchiveBackend::new(store).bounded_snapshot()
    }

    fn restore_validated(&self, record: &ArchiveSnapshotRecord) -> Result<(), NodeError> {
        std::fs::create_dir_all(&self.dir).map_err(|e| NodeError::Storage(e.to_string()))?;
        let db_path = self.dir.join("vnext_operational_compaction.redb");
        let store = crate::vnext_operational_compaction::OperationalCompactionStore::open(
            &db_path,
            crate::vnext_operational_compaction::OperationalCompactionPolicy::default(),
        )
        .map_err(|e| NodeError::Storage(e.to_string()))?;
        LogicalRowsArchiveBackend::new(store).restore_validated(record)
    }
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
