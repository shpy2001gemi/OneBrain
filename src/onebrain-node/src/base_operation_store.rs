//! Durable Base operation protocol and process-generation fencing.
//!
//! Records are append-only revisions beneath the active dataset generation.
//! A revision is create-new and fsynced before it becomes observable in memory,
//! so reopening sees either the prior complete revision or the next complete
//! revision. Non-terminal rows restored from another generation are converted
//! to `UnknownOutcome` and can only proceed through reconciliation.

use std::collections::BTreeMap;
#[cfg(not(windows))]
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use onebrain_archive::{ArchiveEntryKind, ArchiveOwner};
use onebrain_base_contract::{
    BaseErrorCodeV1, BaseIdempotencyKey, BaseOperationId, BaseOperationKindV1,
    BaseOperationReservationId, MigrationVectorBindingV1,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::activation_journal::{ActivationPhase, DatasetGenerationReceipt};
use crate::archive::{PortableArchiveRow, PortableArchiveRows};
use crate::dataset_path::{BaseStorageOwnerId, DatasetGenerationId, DatasetPathResolver};
use crate::error::NodeError;

const OPERATION_RECORD_DOMAIN: &str = "onebrain:base:operation-record:1";
const SUBSCRIPTION_RECORD_DOMAIN: &str = "onebrain:base:subscription-record:1";
const GAP_RECORD_DOMAIN: &str = "onebrain:base:event-gap-record:1";
const AUTHORITY_RECORD_DOMAIN: &str = "onebrain:base:authority-record:1";
const PROCESS_GENERATION_DOMAIN: &str = "onebrain:base:process-generation:1";
const MAX_OPERATION_RECORDS: usize = 4096;
const MAX_AUTHORITY_RECORDS: usize = 4096;
const MAX_OPERATION_REVISIONS: u64 = 64;
const MAX_PROCESS_TOMBSTONES: usize = 32;
const MAX_RESULT_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProcessGenerationId(pub [u8; 32]);

impl ProcessGenerationId {
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

pub trait ProcessGenerationIdSource: Send + Sync {
    fn next_id(&self) -> Result<ProcessGenerationId, BaseOperationStoreError>;
}

#[derive(Default)]
pub struct OsProcessGenerationIdSource;

impl ProcessGenerationIdSource for OsProcessGenerationIdSource {
    fn next_id(&self) -> Result<ProcessGenerationId, BaseOperationStoreError> {
        let mut bytes = [0; 32];
        getrandom::fill(&mut bytes).map_err(|_| BaseOperationStoreError::EntropyUnavailable)?;
        if bytes == [0; 32] {
            return Err(BaseOperationStoreError::EntropyUnavailable);
        }
        Ok(ProcessGenerationId(bytes))
    }
}

#[derive(Debug, Error)]
pub enum BaseOperationStoreError {
    #[error("OS entropy is unavailable")]
    EntropyUnavailable,
    #[error("operation store capacity is exhausted")]
    Capacity,
    #[error("operation record is missing")]
    NotFound,
    #[error("operation state or binding conflicts")]
    Conflict,
    #[error("operation generation is stale")]
    StaleGeneration,
    #[error("operation outcome requires reconciliation")]
    UnknownOutcome,
    #[error("operation store is corrupt")]
    CorruptState,
    #[error("operation store I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaseOperationStateV1 {
    Reserved,
    Prepared,
    Confirming,
    Committed,
    Canceled,
    Failed,
    UnknownOutcome,
}

impl BaseOperationStateV1 {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Committed | Self::Canceled | Self::Failed)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedBaseIntentV1 {
    pub operation_id: BaseOperationId,
    pub command_blake3: [u8; 32],
    pub migration: Option<MigrationVectorBindingV1>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseOperationReceiptV1 {
    pub operation_id: BaseOperationId,
    pub state: BaseOperationStateV1,
    pub attempts: u32,
    pub idempotency_key: Option<BaseIdempotencyKey>,
    pub result_blake3: Option<[u8; 32]>,
    pub result: Vec<u8>,
    pub error: Option<BaseErrorCodeV1>,
    pub migration: Option<MigrationVectorBindingV1>,
    pub reconcile_required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReconciliationResultV1 {
    pub receipt: BaseOperationReceiptV1,
    pub resumed_effect: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DurableMigrationBinding {
    pub vector_id: String,
    pub vector_blake3: [u8; 32],
    pub trust_policy_digest: [u8; 32],
}

impl DurableMigrationBinding {
    pub(crate) fn from_contract(value: &MigrationVectorBindingV1) -> Self {
        Self {
            vector_id: value.vector_id.as_str().to_owned(),
            vector_blake3: value.vector_blake3.0,
            trust_policy_digest: value.trust_policy_digest.0,
        }
    }

    fn to_contract(&self) -> Result<MigrationVectorBindingV1, BaseOperationStoreError> {
        Ok(MigrationVectorBindingV1 {
            vector_id: onebrain_base_contract::MigrationVectorIdV1::try_from_string(
                self.vector_id.clone(),
            )
            .map_err(|_| BaseOperationStoreError::CorruptState)?,
            vector_blake3: onebrain_base_contract::CompatibilityDigestV1(self.vector_blake3),
            trust_policy_digest: onebrain_base_contract::CompatibilityDigestV1(
                self.trust_policy_digest,
            ),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DurableOperationRecord {
    revision: u64,
    reservation_id: [u8; 32],
    operation_id: [u8; 32],
    kind: u16,
    principal_digest: [u8; 32],
    process_generation: ProcessGenerationId,
    dataset_generation: DatasetGenerationId,
    state: BaseOperationStateV1,
    command: Vec<u8>,
    command_blake3: Option<[u8; 32]>,
    migration: Option<DurableMigrationBinding>,
    idempotency_key: Option<[u8; 32]>,
    attempts: u32,
    result: Vec<u8>,
    result_blake3: Option<[u8; 32]>,
    error: Option<u16>,
    reconcile_required: bool,
}

#[derive(Serialize, Deserialize)]
struct DurableEnvelope {
    record: DurableOperationRecord,
    checksum_blake3: [u8; 32],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DurableSubscriptionRecord {
    revision: u64,
    subscription_id: [u8; 32],
    principal_digest: [u8; 32],
    process_generation: ProcessGenerationId,
    dataset_generation: DatasetGenerationId,
    topic: u16,
    cursor: u64,
    closed: bool,
}

#[derive(Serialize, Deserialize)]
struct DurableSubscriptionEnvelope {
    record: DurableSubscriptionRecord,
    checksum_blake3: [u8; 32],
}

#[derive(Serialize, Deserialize)]
struct DurableGapRecord {
    sequence: u64,
    earliest_available_cursor: u64,
    process_generation: ProcessGenerationId,
    dataset_generation: DatasetGenerationId,
    checksum_blake3: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BaseAuthorityKindV1 {
    ManagementGrant,
    ManagementHandle,
    ArchiveCapability,
    SignerProvision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BaseAuthorityStateV1 {
    Registered,
    Active,
    Sealed,
    Committed,
    UnknownOutcome,
    Revoked,
    Aborted,
    Destroyed,
}

impl BaseAuthorityStateV1 {
    const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Committed
                | Self::UnknownOutcome
                | Self::Revoked
                | Self::Aborted
                | Self::Destroyed
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DurableAuthorityRecord {
    revision: u64,
    authority_id: [u8; 32],
    authority_kind: BaseAuthorityKindV1,
    state: BaseAuthorityStateV1,
    principal_digest: [u8; 32],
    operation_id: Option<[u8; 32]>,
    capability_kind: Option<u16>,
    process_generation: ProcessGenerationId,
    dataset_generation: DatasetGenerationId,
}

#[derive(Serialize, Deserialize)]
struct DurableAuthorityEnvelope {
    record: DurableAuthorityRecord,
    checksum_blake3: [u8; 32],
}

pub(crate) struct ProcessGenerationLease {
    id: ProcessGenerationId,
    active_path: PathBuf,
    tombstone_directory: PathBuf,
    closed: Mutex<bool>,
}

impl ProcessGenerationLease {
    pub(crate) fn allocate(
        control_root: &Path,
        source: &dyn ProcessGenerationIdSource,
    ) -> Result<Self, BaseOperationStoreError> {
        let root = control_root.join("base-runtime/process-generations");
        let active = root.join("active");
        let tombstones = root.join("tombstones");
        std::fs::create_dir_all(&active)?;
        std::fs::create_dir_all(&tombstones)?;
        retire_prior_active(&active, &tombstones)?;
        prune_tombstones(&tombstones)?;

        for _ in 0..32 {
            let id = source.next_id()?;
            let name = format!("{}.json", hex(id.as_bytes()));
            if tombstones.join(&name).exists() {
                continue;
            }
            let path = active.join(name);
            let receipt = process_generation_receipt(id);
            match write_new_synced(&path, &receipt) {
                Ok(()) => {
                    sync_directory(&active)?;
                    return Ok(Self {
                        id,
                        active_path: path,
                        tombstone_directory: tombstones,
                        closed: Mutex::new(false),
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(BaseOperationStoreError::EntropyUnavailable)
    }

    pub(crate) const fn id(&self) -> ProcessGenerationId {
        self.id
    }

    pub(crate) fn close(&self) -> Result<(), BaseOperationStoreError> {
        let mut closed = self
            .closed
            .lock()
            .map_err(|_| BaseOperationStoreError::CorruptState)?;
        if *closed {
            return Ok(());
        }
        if self.active_path.exists() {
            let target = self.tombstone_directory.join(
                self.active_path
                    .file_name()
                    .ok_or(BaseOperationStoreError::CorruptState)?,
            );
            std::fs::rename(&self.active_path, target)?;
            sync_directory(&self.tombstone_directory)?;
        }
        *closed = true;
        Ok(())
    }
}

impl Drop for ProcessGenerationLease {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

pub struct BaseOperationStore {
    root: PathBuf,
    process_generation: ProcessGenerationId,
    dataset_generation: DatasetGenerationId,
    records: Mutex<BTreeMap<[u8; 32], DurableOperationRecord>>,
    subscriptions: Mutex<BTreeMap<[u8; 32], DurableSubscriptionRecord>>,
    authority_records: Mutex<BTreeMap<[u8; 32], DurableAuthorityRecord>>,
    next_gap_sequence: Mutex<u64>,
}

impl BaseOperationStore {
    pub fn open(
        resolver: &dyn DatasetPathResolver,
        process_generation: ProcessGenerationId,
    ) -> Result<Self, BaseOperationStoreError> {
        let root = resolver
            .owner_path(BaseStorageOwnerId::BASE_OPERATIONS)
            .map_err(|_| BaseOperationStoreError::CorruptState)?
            .join("operations-v1");
        std::fs::create_dir_all(root.join("records"))?;
        std::fs::create_dir_all(root.join("subscriptions"))?;
        std::fs::create_dir_all(root.join("gaps"))?;
        std::fs::create_dir_all(root.join("authority"))?;
        let records = load_records(&root, process_generation, resolver.current_generation())?;
        let subscriptions =
            load_subscriptions(&root, process_generation, resolver.current_generation())?;
        let next_gap_sequence = load_gap_sequence(&root)?;
        let authority_records =
            load_authority_records(&root, process_generation, resolver.current_generation())?;
        Ok(Self {
            root,
            process_generation,
            dataset_generation: resolver.current_generation(),
            records: Mutex::new(records),
            subscriptions: Mutex::new(subscriptions),
            authority_records: Mutex::new(authority_records),
            next_gap_sequence: Mutex::new(next_gap_sequence),
        })
    }

    pub const fn process_generation(&self) -> ProcessGenerationId {
        self.process_generation
    }

    pub const fn dataset_generation(&self) -> DatasetGenerationId {
        self.dataset_generation
    }

    pub fn reserve_operation(
        &self,
        kind: BaseOperationKindV1,
        principal_digest: [u8; 32],
    ) -> Result<BaseOperationReservationId, BaseOperationStoreError> {
        base_ops_failpoint("before_begin_write")?;
        let mut records = self.lock_records()?;
        base_ops_failpoint("after_begin_write_before_mutation")?;
        if records.len() >= MAX_OPERATION_RECORDS {
            return Err(BaseOperationStoreError::Capacity);
        }
        for _ in 0..32 {
            let id = random_id()?;
            if records.contains_key(&id) {
                continue;
            }
            let record = DurableOperationRecord {
                revision: 0,
                reservation_id: id,
                operation_id: id,
                kind: kind.discriminator(),
                principal_digest,
                process_generation: self.process_generation,
                dataset_generation: self.dataset_generation,
                state: BaseOperationStateV1::Reserved,
                command: Vec::new(),
                command_blake3: None,
                migration: None,
                idempotency_key: None,
                attempts: 0,
                result: Vec::new(),
                result_blake3: None,
                error: None,
                reconcile_required: false,
            };
            self.append_revision(&record)?;
            records.insert(id, record);
            base_ops_failpoint("after_next_side_effect_before_ack")?;
            return Ok(BaseOperationReservationId(id));
        }
        Err(BaseOperationStoreError::EntropyUnavailable)
    }

    pub fn validate_reservation(
        &self,
        reservation: BaseOperationReservationId,
        expected_kind: BaseOperationKindV1,
        principal_digest: [u8; 32],
    ) -> Result<(), BaseOperationStoreError> {
        let records = self.lock_records()?;
        let record = records
            .get(&reservation.0)
            .ok_or(BaseOperationStoreError::NotFound)?;
        self.validate_generation(record)?;
        if record.kind != expected_kind.discriminator()
            || record.principal_digest != principal_digest
            || record.state != BaseOperationStateV1::Reserved
        {
            return Err(BaseOperationStoreError::Conflict);
        }
        Ok(())
    }

    pub(crate) fn operation_kind(
        &self,
        operation_id: BaseOperationId,
        principal_digest: [u8; 32],
    ) -> Result<BaseOperationKindV1, BaseOperationStoreError> {
        let records = self.lock_records()?;
        let record = records
            .get(&operation_id.0)
            .ok_or(BaseOperationStoreError::NotFound)?;
        self.validate_generation(record)?;
        if record.principal_digest != principal_digest {
            return Err(BaseOperationStoreError::Conflict);
        }
        match record.kind {
            value if value == BaseOperationKindV1::ExistingLocalCommand.discriminator() => {
                Ok(BaseOperationKindV1::ExistingLocalCommand)
            }
            value if value == BaseOperationKindV1::CreateArchive.discriminator() => {
                Ok(BaseOperationKindV1::CreateArchive)
            }
            value if value == BaseOperationKindV1::RestoreArchive.discriminator() => {
                Ok(BaseOperationKindV1::RestoreArchive)
            }
            _ => Err(BaseOperationStoreError::CorruptState),
        }
    }

    pub fn prepare(
        &self,
        reservation: BaseOperationReservationId,
        command_kind: BaseOperationKindV1,
        exact_command: Vec<u8>,
        migration: Option<&MigrationVectorBindingV1>,
        principal_digest: [u8; 32],
    ) -> Result<PreparedBaseIntentV1, BaseOperationStoreError> {
        base_ops_failpoint("before_begin_write")?;
        if exact_command.is_empty() || exact_command.len() > MAX_RESULT_BYTES {
            return Err(BaseOperationStoreError::Conflict);
        }
        let command_blake3 = *blake3::hash(&exact_command).as_bytes();
        let durable_migration = migration.map(DurableMigrationBinding::from_contract);
        let mut records = self.lock_records()?;
        base_ops_failpoint("after_begin_write_before_mutation")?;
        let current = records
            .get(&reservation.0)
            .cloned()
            .ok_or(BaseOperationStoreError::NotFound)?;
        self.validate_generation(&current)?;
        if current.kind != command_kind.discriminator()
            || current.principal_digest != principal_digest
        {
            return Err(BaseOperationStoreError::Conflict);
        }
        if current.state == BaseOperationStateV1::Prepared {
            if current.command_blake3 == Some(command_blake3)
                && current.migration == durable_migration
            {
                return self.prepared_intent(&current);
            }
            return Err(BaseOperationStoreError::Conflict);
        }
        if current.state != BaseOperationStateV1::Reserved {
            return Err(BaseOperationStoreError::Conflict);
        }
        let mut next = current;
        next.revision = next_revision(next.revision)?;
        next.state = BaseOperationStateV1::Prepared;
        next.command = exact_command;
        next.command_blake3 = Some(command_blake3);
        next.migration = durable_migration;
        self.append_revision(&next)?;
        records.insert(next.operation_id, next.clone());
        base_ops_failpoint("after_next_side_effect_before_ack")?;
        self.prepared_intent(&next)
    }

    pub fn begin_confirm(
        &self,
        operation_id: BaseOperationId,
        idempotency_key: BaseIdempotencyKey,
        principal_digest: [u8; 32],
    ) -> Result<Option<BaseOperationReceiptV1>, BaseOperationStoreError> {
        base_ops_failpoint("before_begin_write")?;
        let mut records = self.lock_records()?;
        base_ops_failpoint("after_begin_write_before_mutation")?;
        if records.values().any(|other| {
            other.operation_id != operation_id.0 && other.idempotency_key == Some(idempotency_key.0)
        }) {
            return Err(BaseOperationStoreError::Conflict);
        }
        let current = records
            .get(&operation_id.0)
            .cloned()
            .ok_or(BaseOperationStoreError::NotFound)?;
        self.validate_generation(&current)?;
        if current.principal_digest != principal_digest {
            return Err(BaseOperationStoreError::Conflict);
        }
        if current.state.is_terminal() {
            return if current.idempotency_key == Some(idempotency_key.0) {
                Ok(Some(self.receipt(&current)?))
            } else {
                Err(BaseOperationStoreError::Conflict)
            };
        }
        if matches!(
            current.state,
            BaseOperationStateV1::Confirming | BaseOperationStateV1::UnknownOutcome
        ) {
            return Err(BaseOperationStoreError::UnknownOutcome);
        }
        if current.state != BaseOperationStateV1::Prepared {
            return Err(BaseOperationStoreError::Conflict);
        }
        let mut next = current;
        next.revision = next_revision(next.revision)?;
        next.state = BaseOperationStateV1::Confirming;
        next.idempotency_key = Some(idempotency_key.0);
        next.attempts = next
            .attempts
            .checked_add(1)
            .ok_or(BaseOperationStoreError::CorruptState)?;
        next.reconcile_required = true;
        self.append_revision(&next)?;
        records.insert(next.operation_id, next);
        base_ops_failpoint("after_next_side_effect_before_ack")?;
        Ok(None)
    }

    pub fn complete(
        &self,
        operation_id: BaseOperationId,
        result: Vec<u8>,
    ) -> Result<BaseOperationReceiptV1, BaseOperationStoreError> {
        self.finish(
            operation_id,
            BaseOperationStateV1::Committed,
            result,
            None,
            false,
        )
    }

    pub fn fail(
        &self,
        operation_id: BaseOperationId,
        error: BaseErrorCodeV1,
    ) -> Result<BaseOperationReceiptV1, BaseOperationStoreError> {
        self.finish(
            operation_id,
            BaseOperationStateV1::Failed,
            Vec::new(),
            Some(error),
            error.reconcile_before_retry(),
        )
    }

    pub fn mark_unknown(
        &self,
        operation_id: BaseOperationId,
    ) -> Result<BaseOperationReceiptV1, BaseOperationStoreError> {
        self.finish(
            operation_id,
            BaseOperationStateV1::UnknownOutcome,
            Vec::new(),
            Some(BaseErrorCodeV1::UnknownOutcome),
            true,
        )
    }

    pub fn cancel(
        &self,
        operation_id: BaseOperationId,
        principal_digest: [u8; 32],
    ) -> Result<BaseOperationReceiptV1, BaseOperationStoreError> {
        base_ops_failpoint("before_begin_write")?;
        let mut records = self.lock_records()?;
        base_ops_failpoint("after_begin_write_before_mutation")?;
        let current = records
            .get(&operation_id.0)
            .cloned()
            .ok_or(BaseOperationStoreError::NotFound)?;
        self.validate_generation(&current)?;
        if current.principal_digest != principal_digest {
            return Err(BaseOperationStoreError::Conflict);
        }
        if current.state == BaseOperationStateV1::Canceled {
            return self.receipt(&current);
        }
        if matches!(
            current.state,
            BaseOperationStateV1::Committed | BaseOperationStateV1::Confirming
        ) {
            return Err(BaseOperationStoreError::Conflict);
        }
        let mut next = current;
        next.revision = next_revision(next.revision)?;
        next.state = BaseOperationStateV1::Canceled;
        next.reconcile_required = false;
        self.append_revision(&next)?;
        records.insert(next.operation_id, next.clone());
        base_ops_failpoint("after_next_side_effect_before_ack")?;
        self.receipt(&next)
    }

    pub fn reconcile(
        &self,
        operation_id: BaseOperationId,
        principal_digest: [u8; 32],
    ) -> Result<ReconciliationResultV1, BaseOperationStoreError> {
        let records = self.lock_records()?;
        let record = records
            .get(&operation_id.0)
            .ok_or(BaseOperationStoreError::NotFound)?;
        if record.principal_digest != principal_digest {
            return Err(BaseOperationStoreError::Conflict);
        }
        Ok(ReconciliationResultV1 {
            receipt: self.receipt(record)?,
            resumed_effect: false,
        })
    }

    pub(crate) fn migration(
        &self,
        operation_id: BaseOperationId,
    ) -> Result<Option<MigrationVectorBindingV1>, BaseOperationStoreError> {
        let records = self.lock_records()?;
        records
            .get(&operation_id.0)
            .ok_or(BaseOperationStoreError::NotFound)?
            .migration
            .as_ref()
            .map(DurableMigrationBinding::to_contract)
            .transpose()
    }

    pub(crate) fn carry_record_to(
        &self,
        operation_id: BaseOperationId,
        target: &Self,
        force_unknown: bool,
    ) -> Result<(), BaseOperationStoreError> {
        let source = self
            .lock_records()?
            .get(&operation_id.0)
            .cloned()
            .ok_or(BaseOperationStoreError::NotFound)?;
        let mut carried = source;
        carried.revision = 0;
        carried.process_generation = target.process_generation;
        carried.dataset_generation = target.dataset_generation;
        if force_unknown || !carried.state.is_terminal() {
            carried.state = BaseOperationStateV1::UnknownOutcome;
            carried.error = Some(BaseErrorCodeV1::UnknownOutcome.discriminator());
            carried.reconcile_required = true;
        }
        let mut target_records = target.lock_records()?;
        if let Some(existing) = target_records.get(&operation_id.0) {
            if records_equivalent(existing, &carried) {
                return Ok(());
            }
            return Err(BaseOperationStoreError::Conflict);
        }
        target.append_revision(&carried)?;
        target_records.insert(operation_id.0, carried);
        Ok(())
    }

    pub(crate) fn import_activation_receipt(
        &self,
        receipt: &DatasetGenerationReceipt,
    ) -> Result<(), BaseOperationStoreError> {
        let context = receipt
            .operation_context
            .as_ref()
            .ok_or(BaseOperationStoreError::CorruptState)?;
        let mut records = self.lock_records()?;
        if let Some(existing) = records.get(&receipt.operation_id) {
            return if existing.idempotency_key == Some(receipt.idempotency_key) {
                Ok(())
            } else {
                Err(BaseOperationStoreError::Conflict)
            };
        }
        let state = match receipt.phase {
            ActivationPhase::Complete => BaseOperationStateV1::Committed,
            ActivationPhase::RolledBack => BaseOperationStateV1::Failed,
            ActivationPhase::Prepared
            | ActivationPhase::PointerPublished
            | ActivationPhase::UnknownOutcome => BaseOperationStateV1::UnknownOutcome,
        };
        let mut result = Vec::with_capacity(105);
        result.extend_from_slice(&receipt.operation_id);
        result.extend_from_slice(&receipt.old_generation_root);
        result.extend_from_slice(&receipt.new_generation_root);
        result.extend_from_slice(&receipt.generation_sequence.to_le_bytes());
        result.push(receipt.phase as u8);
        let migration = match (
            context.migration_vector_id.clone(),
            context.migration_vector_blake3,
            context.migration_trust_policy_digest,
        ) {
            (Some(vector_id), Some(vector_blake3), Some(trust_policy_digest)) => {
                Some(DurableMigrationBinding {
                    vector_id,
                    vector_blake3,
                    trust_policy_digest,
                })
            }
            (None, None, None) => None,
            _ => return Err(BaseOperationStoreError::CorruptState),
        };
        let record = DurableOperationRecord {
            revision: 0,
            reservation_id: receipt.operation_id,
            operation_id: receipt.operation_id,
            kind: BaseOperationKindV1::RestoreArchive.discriminator(),
            principal_digest: context.principal_digest,
            process_generation: self.process_generation,
            dataset_generation: self.dataset_generation,
            state,
            command: vec![BaseOperationKindV1::RestoreArchive.discriminator() as u8],
            command_blake3: None,
            migration,
            idempotency_key: Some(receipt.idempotency_key),
            attempts: 1,
            result_blake3: Some(*blake3::hash(&result).as_bytes()),
            result,
            error: match state {
                BaseOperationStateV1::Failed => Some(BaseErrorCodeV1::Conflict.discriminator()),
                BaseOperationStateV1::UnknownOutcome => {
                    Some(BaseErrorCodeV1::UnknownOutcome.discriminator())
                }
                _ => None,
            },
            reconcile_required: state == BaseOperationStateV1::UnknownOutcome,
        };
        self.append_revision(&record)?;
        records.insert(record.operation_id, record);
        Ok(())
    }

    pub(crate) fn create_subscription(
        &self,
        subscription_id: [u8; 32],
        principal_digest: [u8; 32],
        topic: u16,
        cursor: u64,
    ) -> Result<(), BaseOperationStoreError> {
        if subscription_id == [0; 32] || !(1..=5).contains(&topic) {
            return Err(BaseOperationStoreError::Conflict);
        }
        let mut subscriptions = self
            .subscriptions
            .lock()
            .map_err(|_| BaseOperationStoreError::CorruptState)?;
        if subscriptions.contains_key(&subscription_id) {
            return Err(BaseOperationStoreError::Conflict);
        }
        let record = DurableSubscriptionRecord {
            revision: 0,
            subscription_id,
            principal_digest,
            process_generation: self.process_generation,
            dataset_generation: self.dataset_generation,
            topic,
            cursor,
            closed: false,
        };
        self.append_subscription(&record)?;
        subscriptions.insert(subscription_id, record);
        Ok(())
    }

    pub(crate) fn advance_subscription(
        &self,
        subscription_id: [u8; 32],
        principal_digest: [u8; 32],
        cursor: u64,
    ) -> Result<(), BaseOperationStoreError> {
        let mut subscriptions = self
            .subscriptions
            .lock()
            .map_err(|_| BaseOperationStoreError::CorruptState)?;
        let current = subscriptions
            .get(&subscription_id)
            .cloned()
            .ok_or(BaseOperationStoreError::NotFound)?;
        if current.closed || current.principal_digest != principal_digest || cursor < current.cursor
        {
            return Err(BaseOperationStoreError::Conflict);
        }
        if cursor == current.cursor {
            return Ok(());
        }
        let mut next = current;
        next.revision = next_revision(next.revision)?;
        next.cursor = cursor;
        self.append_subscription(&next)?;
        subscriptions.insert(subscription_id, next);
        Ok(())
    }

    pub(crate) fn close_subscription(
        &self,
        subscription_id: [u8; 32],
        principal_digest: [u8; 32],
    ) -> Result<(), BaseOperationStoreError> {
        let mut subscriptions = self
            .subscriptions
            .lock()
            .map_err(|_| BaseOperationStoreError::CorruptState)?;
        let current = subscriptions
            .get(&subscription_id)
            .cloned()
            .ok_or(BaseOperationStoreError::NotFound)?;
        if current.principal_digest != principal_digest {
            return Err(BaseOperationStoreError::Conflict);
        }
        if current.closed {
            return Ok(());
        }
        let mut next = current;
        next.revision = next_revision(next.revision)?;
        next.closed = true;
        self.append_subscription(&next)?;
        subscriptions.insert(subscription_id, next);
        Ok(())
    }

    pub(crate) fn record_gap_marker(
        &self,
        earliest_available_cursor: u64,
    ) -> Result<(), BaseOperationStoreError> {
        let mut sequence = self
            .next_gap_sequence
            .lock()
            .map_err(|_| BaseOperationStoreError::CorruptState)?;
        let current = *sequence;
        let mut record = DurableGapRecord {
            sequence: current,
            earliest_available_cursor,
            process_generation: self.process_generation,
            dataset_generation: self.dataset_generation,
            checksum_blake3: [0; 32],
        };
        let bytes = serde_json::to_vec(&(
            record.sequence,
            record.earliest_available_cursor,
            record.process_generation,
            record.dataset_generation,
        ))
        .map_err(|_| BaseOperationStoreError::CorruptState)?;
        record.checksum_blake3 = blake3::derive_key(GAP_RECORD_DOMAIN, &bytes);
        write_new_synced(
            &self.root.join("gaps").join(format!("{current:020}.json")),
            &record,
        )?;
        sync_directory(&self.root.join("gaps"))?;
        *sequence = current
            .checked_add(1)
            .ok_or(BaseOperationStoreError::CorruptState)?;
        Ok(())
    }

    pub(crate) fn register_authority(
        &self,
        authority_id: [u8; 32],
        authority_kind: BaseAuthorityKindV1,
        principal_digest: [u8; 32],
        operation_id: Option<[u8; 32]>,
        capability_kind: Option<u16>,
    ) -> Result<(), BaseOperationStoreError> {
        if authority_id == [0; 32] {
            return Err(BaseOperationStoreError::Conflict);
        }
        let mut records = self
            .authority_records
            .lock()
            .map_err(|_| BaseOperationStoreError::CorruptState)?;
        if records.len() >= MAX_AUTHORITY_RECORDS || records.contains_key(&authority_id) {
            return Err(BaseOperationStoreError::Capacity);
        }
        let record = DurableAuthorityRecord {
            revision: 0,
            authority_id,
            authority_kind,
            state: BaseAuthorityStateV1::Registered,
            principal_digest,
            operation_id,
            capability_kind,
            process_generation: self.process_generation,
            dataset_generation: self.dataset_generation,
        };
        self.append_authority(&record)?;
        records.insert(authority_id, record);
        Ok(())
    }

    pub(crate) fn transition_authority(
        &self,
        authority_id: [u8; 32],
        authority_kind: BaseAuthorityKindV1,
        principal_digest: [u8; 32],
        state: BaseAuthorityStateV1,
    ) -> Result<(), BaseOperationStoreError> {
        let mut records = self
            .authority_records
            .lock()
            .map_err(|_| BaseOperationStoreError::CorruptState)?;
        let current = records
            .get(&authority_id)
            .cloned()
            .ok_or(BaseOperationStoreError::NotFound)?;
        if current.authority_kind != authority_kind
            || current.principal_digest != principal_digest
            || current.process_generation != self.process_generation
            || current.dataset_generation != self.dataset_generation
        {
            return Err(BaseOperationStoreError::Conflict);
        }
        if current.state == state {
            return Ok(());
        }
        if current.state.is_terminal() {
            return Err(BaseOperationStoreError::Conflict);
        }
        let valid = matches!(
            (current.state, state),
            (
                BaseAuthorityStateV1::Registered,
                BaseAuthorityStateV1::Active
            ) | (
                BaseAuthorityStateV1::Registered | BaseAuthorityStateV1::Active,
                BaseAuthorityStateV1::Revoked
                    | BaseAuthorityStateV1::Aborted
                    | BaseAuthorityStateV1::Destroyed
                    | BaseAuthorityStateV1::UnknownOutcome
            ) | (
                BaseAuthorityStateV1::Active,
                BaseAuthorityStateV1::Sealed | BaseAuthorityStateV1::Committed
            ) | (
                BaseAuthorityStateV1::Sealed,
                BaseAuthorityStateV1::Committed
                    | BaseAuthorityStateV1::Revoked
                    | BaseAuthorityStateV1::Aborted
                    | BaseAuthorityStateV1::Destroyed
                    | BaseAuthorityStateV1::UnknownOutcome
            )
        );
        if !valid {
            return Err(BaseOperationStoreError::Conflict);
        }
        let mut next = current;
        next.revision = next_revision(next.revision)?;
        next.state = state;
        self.append_authority(&next)?;
        records.insert(authority_id, next);
        Ok(())
    }

    fn finish(
        &self,
        operation_id: BaseOperationId,
        state: BaseOperationStateV1,
        result: Vec<u8>,
        error: Option<BaseErrorCodeV1>,
        reconcile_required: bool,
    ) -> Result<BaseOperationReceiptV1, BaseOperationStoreError> {
        base_ops_failpoint("before_begin_write")?;
        if result.len() > MAX_RESULT_BYTES {
            return Err(BaseOperationStoreError::Capacity);
        }
        let mut records = self.lock_records()?;
        base_ops_failpoint("after_begin_write_before_mutation")?;
        let current = records
            .get(&operation_id.0)
            .cloned()
            .ok_or(BaseOperationStoreError::NotFound)?;
        if current.state.is_terminal() {
            return self.receipt(&current);
        }
        if !matches!(
            current.state,
            BaseOperationStateV1::Confirming | BaseOperationStateV1::UnknownOutcome
        ) {
            return Err(BaseOperationStoreError::Conflict);
        }
        let mut next = current;
        next.revision = next_revision(next.revision)?;
        next.state = state;
        next.result_blake3 = (!result.is_empty()).then(|| *blake3::hash(&result).as_bytes());
        next.result = result;
        next.error = error.map(BaseErrorCodeV1::discriminator);
        next.reconcile_required = reconcile_required;
        self.append_revision(&next)?;
        records.insert(next.operation_id, next.clone());
        base_ops_failpoint("after_next_side_effect_before_ack")?;
        self.receipt(&next)
    }

    fn prepared_intent(
        &self,
        record: &DurableOperationRecord,
    ) -> Result<PreparedBaseIntentV1, BaseOperationStoreError> {
        Ok(PreparedBaseIntentV1 {
            operation_id: BaseOperationId(record.operation_id),
            command_blake3: record
                .command_blake3
                .ok_or(BaseOperationStoreError::CorruptState)?,
            migration: record
                .migration
                .as_ref()
                .map(DurableMigrationBinding::to_contract)
                .transpose()?,
        })
    }

    fn receipt(
        &self,
        record: &DurableOperationRecord,
    ) -> Result<BaseOperationReceiptV1, BaseOperationStoreError> {
        Ok(BaseOperationReceiptV1 {
            operation_id: BaseOperationId(record.operation_id),
            state: record.state,
            attempts: record.attempts,
            idempotency_key: record.idempotency_key.map(BaseIdempotencyKey),
            result_blake3: record.result_blake3,
            result: record.result.clone(),
            error: record.error.map(error_from_code).transpose()?,
            migration: record
                .migration
                .as_ref()
                .map(DurableMigrationBinding::to_contract)
                .transpose()?,
            reconcile_required: record.reconcile_required,
        })
    }

    fn validate_generation(
        &self,
        record: &DurableOperationRecord,
    ) -> Result<(), BaseOperationStoreError> {
        if record.process_generation != self.process_generation
            || record.dataset_generation != self.dataset_generation
        {
            return Err(BaseOperationStoreError::StaleGeneration);
        }
        Ok(())
    }

    fn append_revision(
        &self,
        record: &DurableOperationRecord,
    ) -> Result<(), BaseOperationStoreError> {
        if record.revision >= MAX_OPERATION_REVISIONS {
            return Err(BaseOperationStoreError::Capacity);
        }
        let directory = self.root.join("records").join(hex(&record.operation_id));
        std::fs::create_dir_all(&directory)?;
        let envelope = envelope(record.clone())?;
        let path = directory.join(format!("{:020}.json", record.revision));
        base_ops_failpoint("after_mutation_before_commit")?;
        write_new_synced(&path, &envelope)?;
        sync_directory(&directory)?;
        base_ops_failpoint("after_commit_before_next_side_effect")?;
        Ok(())
    }

    fn append_subscription(
        &self,
        record: &DurableSubscriptionRecord,
    ) -> Result<(), BaseOperationStoreError> {
        let directory = self
            .root
            .join("subscriptions")
            .join(hex(&record.subscription_id));
        std::fs::create_dir_all(&directory)?;
        let bytes =
            serde_json::to_vec(record).map_err(|_| BaseOperationStoreError::CorruptState)?;
        let envelope = DurableSubscriptionEnvelope {
            record: record.clone(),
            checksum_blake3: blake3::derive_key(SUBSCRIPTION_RECORD_DOMAIN, &bytes),
        };
        write_new_synced(
            &directory.join(format!("{:020}.json", record.revision)),
            &envelope,
        )?;
        sync_directory(&directory)?;
        Ok(())
    }

    fn append_authority(
        &self,
        record: &DurableAuthorityRecord,
    ) -> Result<(), BaseOperationStoreError> {
        if record.revision >= MAX_OPERATION_REVISIONS {
            return Err(BaseOperationStoreError::Capacity);
        }
        let directory = self.root.join("authority").join(hex(&record.authority_id));
        std::fs::create_dir_all(&directory)?;
        let bytes =
            serde_json::to_vec(record).map_err(|_| BaseOperationStoreError::CorruptState)?;
        let envelope = DurableAuthorityEnvelope {
            record: record.clone(),
            checksum_blake3: blake3::derive_key(AUTHORITY_RECORD_DOMAIN, &bytes),
        };
        write_new_synced(
            &directory.join(format!("{:020}.json", record.revision)),
            &envelope,
        )?;
        sync_directory(&directory)?;
        Ok(())
    }

    fn lock_records(
        &self,
    ) -> Result<
        std::sync::MutexGuard<'_, BTreeMap<[u8; 32], DurableOperationRecord>>,
        BaseOperationStoreError,
    > {
        self.records
            .lock()
            .map_err(|_| BaseOperationStoreError::CorruptState)
    }
}

impl PortableArchiveRows for Arc<BaseOperationStore> {
    fn archive_owner(&self) -> ArchiveOwner {
        ArchiveOwner::BASE_OPERATIONS
    }

    fn archive_entry_kind(&self) -> ArchiveEntryKind {
        ArchiveEntryKind::BaseOperationRecord
    }

    fn archive_rows(&self) -> Result<Vec<PortableArchiveRow>, NodeError> {
        let mut rows = self
            .lock_records()
            .map_err(store_node_error)?
            .values()
            .cloned()
            .map(|record| {
                Ok(PortableArchiveRow {
                    table: 1,
                    key: record.operation_id.to_vec(),
                    value: serde_json::to_vec(&envelope(record).map_err(store_node_error)?)
                        .map_err(|_| {
                            NodeError::Storage("Base operation row encoding failed".into())
                        })?,
                })
            })
            .collect::<Result<Vec<_>, NodeError>>()?;
        rows.extend(
            self.subscriptions
                .lock()
                .map_err(|_| NodeError::Storage("Base subscription store lock failed".into()))?
                .values()
                .cloned()
                .map(|record| {
                    let bytes = serde_json::to_vec(&record).map_err(|_| {
                        NodeError::Storage("Base subscription row encoding failed".into())
                    })?;
                    let envelope = DurableSubscriptionEnvelope {
                        checksum_blake3: blake3::derive_key(SUBSCRIPTION_RECORD_DOMAIN, &bytes),
                        record,
                    };
                    Ok(PortableArchiveRow {
                        table: 2,
                        key: envelope.record.subscription_id.to_vec(),
                        value: serde_json::to_vec(&envelope).map_err(|_| {
                            NodeError::Storage("Base subscription envelope encoding failed".into())
                        })?,
                    })
                })
                .collect::<Result<Vec<_>, NodeError>>()?,
        );
        rows.extend(
            self.authority_records
                .lock()
                .map_err(|_| NodeError::Storage("Base authority store lock failed".into()))?
                .values()
                .cloned()
                .map(|mut record| {
                    record.state = BaseAuthorityStateV1::Revoked;
                    let bytes = serde_json::to_vec(&record).map_err(|_| {
                        NodeError::Storage("Base authority row encoding failed".into())
                    })?;
                    let envelope = DurableAuthorityEnvelope {
                        checksum_blake3: blake3::derive_key(AUTHORITY_RECORD_DOMAIN, &bytes),
                        record,
                    };
                    Ok(PortableArchiveRow {
                        table: 3,
                        key: envelope.record.authority_id.to_vec(),
                        value: serde_json::to_vec(&envelope).map_err(|_| {
                            NodeError::Storage("Base authority envelope encoding failed".into())
                        })?,
                    })
                })
                .collect::<Result<Vec<_>, NodeError>>()?,
        );
        Ok(rows)
    }

    fn restore_row(&self, row: &PortableArchiveRow) -> Result<(), NodeError> {
        if row.key.len() != 32 || !matches!(row.table, 1..=3) {
            return Err(NodeError::Storage(
                "invalid Base operation archive row".into(),
            ));
        }
        if row.table == 3 {
            let envelope: DurableAuthorityEnvelope =
                serde_json::from_slice(&row.value).map_err(|_| {
                    NodeError::Storage("invalid Base authority archive envelope".into())
                })?;
            verify_authority_envelope(&envelope).map_err(store_node_error)?;
            if envelope.record.authority_id.as_slice() != row.key.as_slice() {
                return Err(NodeError::Storage(
                    "Base authority archive key mismatch".into(),
                ));
            }
            let mut restored = envelope.record;
            restored.revision = 0;
            restored.process_generation = self.process_generation;
            restored.dataset_generation = self.dataset_generation;
            restored.state = BaseAuthorityStateV1::Revoked;
            let mut authority = self
                .authority_records
                .lock()
                .map_err(|_| NodeError::Storage("Base authority store lock failed".into()))?;
            if authority.contains_key(&restored.authority_id) {
                return Err(NodeError::Storage(
                    "duplicate Base authority archive row".into(),
                ));
            }
            self.append_authority(&restored).map_err(store_node_error)?;
            authority.insert(restored.authority_id, restored);
            return Ok(());
        }
        if row.table == 2 {
            let envelope: DurableSubscriptionEnvelope = serde_json::from_slice(&row.value)
                .map_err(|_| {
                    NodeError::Storage("invalid Base subscription archive envelope".into())
                })?;
            verify_subscription_envelope(&envelope).map_err(store_node_error)?;
            if envelope.record.subscription_id.as_slice() != row.key.as_slice() {
                return Err(NodeError::Storage(
                    "Base subscription archive key mismatch".into(),
                ));
            }
            let mut restored = envelope.record;
            restored.revision = 0;
            restored.process_generation = self.process_generation;
            restored.dataset_generation = self.dataset_generation;
            restored.closed = true;
            let mut subscriptions = self
                .subscriptions
                .lock()
                .map_err(|_| NodeError::Storage("Base subscription store lock failed".into()))?;
            if subscriptions.contains_key(&restored.subscription_id) {
                return Err(NodeError::Storage(
                    "duplicate Base subscription archive row".into(),
                ));
            }
            self.append_subscription(&restored)
                .map_err(store_node_error)?;
            subscriptions.insert(restored.subscription_id, restored);
            return Ok(());
        }
        let envelope: DurableEnvelope = serde_json::from_slice(&row.value)
            .map_err(|_| NodeError::Storage("invalid Base operation archive envelope".into()))?;
        verify_envelope(&envelope).map_err(store_node_error)?;
        if envelope.record.operation_id.as_slice() != row.key.as_slice() {
            return Err(NodeError::Storage(
                "Base operation archive key mismatch".into(),
            ));
        }
        let mut restored = envelope.record;
        restored.revision = 0;
        restored.process_generation = self.process_generation;
        restored.dataset_generation = self.dataset_generation;
        if !restored.state.is_terminal() {
            restored.state = BaseOperationStateV1::UnknownOutcome;
            restored.error = Some(BaseErrorCodeV1::UnknownOutcome.discriminator());
            restored.reconcile_required = true;
        }
        let mut records = self.lock_records().map_err(store_node_error)?;
        if records.contains_key(&restored.operation_id) {
            return Err(NodeError::Storage(
                "duplicate Base operation archive row".into(),
            ));
        }
        self.append_revision(&restored).map_err(store_node_error)?;
        records.insert(restored.operation_id, restored);
        Ok(())
    }
}

fn load_subscriptions(
    root: &Path,
    process_generation: ProcessGenerationId,
    dataset_generation: DatasetGenerationId,
) -> Result<BTreeMap<[u8; 32], DurableSubscriptionRecord>, BaseOperationStoreError> {
    let mut records = BTreeMap::new();
    for subscription in std::fs::read_dir(root.join("subscriptions"))? {
        let subscription = subscription?;
        if !subscription.file_type()?.is_dir() {
            return Err(BaseOperationStoreError::CorruptState);
        }
        let mut revisions = std::fs::read_dir(subscription.path())?
            .filter_map(|entry| match entry {
                Ok(entry) if entry.file_name() == ".onebrain-directory-sync" => None,
                other => Some(other.map(|entry| entry.path())),
            })
            .collect::<Result<Vec<_>, _>>()?;
        revisions.sort();
        if revisions.is_empty() || revisions.len() > MAX_OPERATION_REVISIONS as usize {
            return Err(BaseOperationStoreError::CorruptState);
        }
        let mut latest = None;
        for (expected_revision, path) in revisions.into_iter().enumerate() {
            let envelope: DurableSubscriptionEnvelope =
                serde_json::from_slice(&std::fs::read(path)?)
                    .map_err(|_| BaseOperationStoreError::CorruptState)?;
            verify_subscription_envelope(&envelope)?;
            if envelope.record.revision != expected_revision as u64 {
                return Err(BaseOperationStoreError::CorruptState);
            }
            latest = Some(envelope.record);
        }
        let mut record = latest.ok_or(BaseOperationStoreError::CorruptState)?;
        if record.process_generation != process_generation
            || record.dataset_generation != dataset_generation
        {
            record.revision = next_revision(record.revision)?;
            record.process_generation = process_generation;
            record.dataset_generation = dataset_generation;
            record.closed = true;
            let bytes =
                serde_json::to_vec(&record).map_err(|_| BaseOperationStoreError::CorruptState)?;
            let envelope = DurableSubscriptionEnvelope {
                checksum_blake3: blake3::derive_key(SUBSCRIPTION_RECORD_DOMAIN, &bytes),
                record: record.clone(),
            };
            write_new_synced(
                &subscription
                    .path()
                    .join(format!("{:020}.json", record.revision)),
                &envelope,
            )?;
            sync_directory(&subscription.path())?;
        }
        if records.insert(record.subscription_id, record).is_some() {
            return Err(BaseOperationStoreError::CorruptState);
        }
    }
    Ok(records)
}

fn load_authority_records(
    root: &Path,
    process_generation: ProcessGenerationId,
    dataset_generation: DatasetGenerationId,
) -> Result<BTreeMap<[u8; 32], DurableAuthorityRecord>, BaseOperationStoreError> {
    let mut records = BTreeMap::new();
    for authority in std::fs::read_dir(root.join("authority"))? {
        let authority = authority?;
        if !authority.file_type()?.is_dir() {
            return Err(BaseOperationStoreError::CorruptState);
        }
        let mut revisions = std::fs::read_dir(authority.path())?
            .filter_map(|entry| match entry {
                Ok(entry) if entry.file_name() == ".onebrain-directory-sync" => None,
                other => Some(other.map(|entry| entry.path())),
            })
            .collect::<Result<Vec<_>, _>>()?;
        revisions.sort();
        if revisions.is_empty() || revisions.len() > MAX_OPERATION_REVISIONS as usize {
            return Err(BaseOperationStoreError::CorruptState);
        }
        let mut latest = None;
        for (expected_revision, path) in revisions.into_iter().enumerate() {
            let envelope: DurableAuthorityEnvelope = serde_json::from_slice(&std::fs::read(path)?)
                .map_err(|_| BaseOperationStoreError::CorruptState)?;
            verify_authority_envelope(&envelope)?;
            if envelope.record.revision != expected_revision as u64 {
                return Err(BaseOperationStoreError::CorruptState);
            }
            latest = Some(envelope.record);
        }
        let mut record = latest.ok_or(BaseOperationStoreError::CorruptState)?;
        if record.process_generation != process_generation
            || record.dataset_generation != dataset_generation
        {
            record.revision = next_revision(record.revision)?;
            record.process_generation = process_generation;
            record.dataset_generation = dataset_generation;
            record.state = BaseAuthorityStateV1::Revoked;
            let bytes =
                serde_json::to_vec(&record).map_err(|_| BaseOperationStoreError::CorruptState)?;
            let envelope = DurableAuthorityEnvelope {
                checksum_blake3: blake3::derive_key(AUTHORITY_RECORD_DOMAIN, &bytes),
                record: record.clone(),
            };
            write_new_synced(
                &authority
                    .path()
                    .join(format!("{:020}.json", record.revision)),
                &envelope,
            )?;
            sync_directory(&authority.path())?;
        }
        if records.insert(record.authority_id, record).is_some()
            || records.len() > MAX_AUTHORITY_RECORDS
        {
            return Err(BaseOperationStoreError::CorruptState);
        }
    }
    Ok(records)
}

fn load_gap_sequence(root: &Path) -> Result<u64, BaseOperationStoreError> {
    let mut maximum = None;
    for entry in std::fs::read_dir(root.join("gaps"))? {
        let entry = entry?;
        if entry.file_name() == ".onebrain-directory-sync" {
            continue;
        }
        let record: DurableGapRecord = serde_json::from_slice(&std::fs::read(entry.path())?)
            .map_err(|_| BaseOperationStoreError::CorruptState)?;
        let bytes = serde_json::to_vec(&(
            record.sequence,
            record.earliest_available_cursor,
            record.process_generation,
            record.dataset_generation,
        ))
        .map_err(|_| BaseOperationStoreError::CorruptState)?;
        if blake3::derive_key(GAP_RECORD_DOMAIN, &bytes) != record.checksum_blake3 {
            return Err(BaseOperationStoreError::CorruptState);
        }
        maximum = Some(maximum.map_or(record.sequence, |value: u64| value.max(record.sequence)));
    }
    maximum
        .map(|value| {
            value
                .checked_add(1)
                .ok_or(BaseOperationStoreError::CorruptState)
        })
        .transpose()
        .map(Option::unwrap_or_default)
}

fn verify_subscription_envelope(
    envelope: &DurableSubscriptionEnvelope,
) -> Result<(), BaseOperationStoreError> {
    let bytes =
        serde_json::to_vec(&envelope.record).map_err(|_| BaseOperationStoreError::CorruptState)?;
    if blake3::derive_key(SUBSCRIPTION_RECORD_DOMAIN, &bytes) != envelope.checksum_blake3
        || envelope.record.subscription_id == [0; 32]
        || !(1..=5).contains(&envelope.record.topic)
    {
        return Err(BaseOperationStoreError::CorruptState);
    }
    Ok(())
}

fn verify_authority_envelope(
    envelope: &DurableAuthorityEnvelope,
) -> Result<(), BaseOperationStoreError> {
    let bytes =
        serde_json::to_vec(&envelope.record).map_err(|_| BaseOperationStoreError::CorruptState)?;
    if blake3::derive_key(AUTHORITY_RECORD_DOMAIN, &bytes) != envelope.checksum_blake3
        || envelope.record.authority_id == [0; 32]
    {
        return Err(BaseOperationStoreError::CorruptState);
    }
    Ok(())
}

fn load_records(
    root: &Path,
    process_generation: ProcessGenerationId,
    dataset_generation: DatasetGenerationId,
) -> Result<BTreeMap<[u8; 32], DurableOperationRecord>, BaseOperationStoreError> {
    let mut records = BTreeMap::new();
    let directory = root.join("records");
    for operation in std::fs::read_dir(&directory)? {
        let operation = operation?;
        if !operation.file_type()?.is_dir() {
            return Err(BaseOperationStoreError::CorruptState);
        }
        let mut revisions = Vec::new();
        for revision in std::fs::read_dir(operation.path())? {
            let revision = revision?;
            if revision.file_name() == ".onebrain-directory-sync" {
                continue;
            }
            if !revision.file_type()?.is_file() {
                return Err(BaseOperationStoreError::CorruptState);
            }
            revisions.push(revision.path());
        }
        revisions.sort();
        if revisions.is_empty() {
            // A kill before the first create-new revision may leave only the
            // operation directory. It has no authoritative bytes and is safe
            // to remove with the non-recursive empty-directory operation.
            std::fs::remove_dir(operation.path())?;
            continue;
        }
        if revisions.len() > MAX_OPERATION_REVISIONS as usize {
            return Err(BaseOperationStoreError::CorruptState);
        }
        let mut latest = None;
        for (expected_revision, path) in revisions.into_iter().enumerate() {
            let envelope: DurableEnvelope = serde_json::from_slice(&std::fs::read(path)?)
                .map_err(|_| BaseOperationStoreError::CorruptState)?;
            verify_envelope(&envelope)?;
            if envelope.record.revision != expected_revision as u64 {
                return Err(BaseOperationStoreError::CorruptState);
            }
            latest = Some(envelope.record);
        }
        let mut record = latest.ok_or(BaseOperationStoreError::CorruptState)?;
        if record.process_generation != process_generation
            || record.dataset_generation != dataset_generation
        {
            if !record.state.is_terminal() {
                record.revision = next_revision(record.revision)?;
                record.process_generation = process_generation;
                record.dataset_generation = dataset_generation;
                record.state = BaseOperationStateV1::UnknownOutcome;
                record.error = Some(BaseErrorCodeV1::UnknownOutcome.discriminator());
                record.reconcile_required = true;
                let envelope = envelope(record.clone())?;
                let revision_path = operation
                    .path()
                    .join(format!("{:020}.json", record.revision));
                write_new_synced(&revision_path, &envelope)?;
                sync_directory(&operation.path())?;
            } else {
                record.process_generation = process_generation;
                record.dataset_generation = dataset_generation;
            }
        }
        if records.insert(record.operation_id, record).is_some() {
            return Err(BaseOperationStoreError::CorruptState);
        }
    }
    if records.len() > MAX_OPERATION_RECORDS {
        return Err(BaseOperationStoreError::Capacity);
    }
    Ok(records)
}

fn envelope(record: DurableOperationRecord) -> Result<DurableEnvelope, BaseOperationStoreError> {
    let bytes = serde_json::to_vec(&record).map_err(|_| BaseOperationStoreError::CorruptState)?;
    let checksum_blake3 = blake3::derive_key(OPERATION_RECORD_DOMAIN, &bytes);
    Ok(DurableEnvelope {
        record,
        checksum_blake3,
    })
}

fn verify_envelope(envelope: &DurableEnvelope) -> Result<(), BaseOperationStoreError> {
    let bytes =
        serde_json::to_vec(&envelope.record).map_err(|_| BaseOperationStoreError::CorruptState)?;
    if blake3::derive_key(OPERATION_RECORD_DOMAIN, &bytes) != envelope.checksum_blake3
        || envelope.record.operation_id != envelope.record.reservation_id
        || envelope.record.operation_id == [0; 32]
        || envelope.record.command.len() > MAX_RESULT_BYTES
        || envelope.record.result.len() > MAX_RESULT_BYTES
    {
        return Err(BaseOperationStoreError::CorruptState);
    }
    Ok(())
}

fn process_generation_receipt(id: ProcessGenerationId) -> serde_json::Value {
    let checksum = blake3::derive_key(PROCESS_GENERATION_DOMAIN, id.as_bytes());
    serde_json::json!({
        "profile": PROCESS_GENERATION_DOMAIN,
        "generation": hex(id.as_bytes()),
        "checksum_blake3": hex(&checksum),
    })
}

fn retire_prior_active(active: &Path, tombstones: &Path) -> Result<(), BaseOperationStoreError> {
    let entries = std::fs::read_dir(active)?
        .filter_map(|entry| match entry {
            Ok(entry) if entry.file_name() == ".onebrain-directory-sync" => None,
            other => Some(other),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if entries.len() > 1 {
        return Err(BaseOperationStoreError::CorruptState);
    }
    for entry in entries {
        if !entry.file_type()?.is_file() {
            return Err(BaseOperationStoreError::CorruptState);
        }
        validate_process_receipt(&entry.path())?;
        std::fs::rename(entry.path(), tombstones.join(entry.file_name()))?;
        sync_directory(tombstones)?;
    }
    Ok(())
}

fn validate_process_receipt(path: &Path) -> Result<(), BaseOperationStoreError> {
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)
        .map_err(|_| BaseOperationStoreError::CorruptState)?;
    let generation = value
        .get("generation")
        .and_then(serde_json::Value::as_str)
        .ok_or(BaseOperationStoreError::CorruptState)?;
    let bytes = decode_hex_32(generation)?;
    let expected = hex(&blake3::derive_key(PROCESS_GENERATION_DOMAIN, &bytes));
    if value.get("profile").and_then(serde_json::Value::as_str) != Some(PROCESS_GENERATION_DOMAIN)
        || value
            .get("checksum_blake3")
            .and_then(serde_json::Value::as_str)
            != Some(&expected)
    {
        return Err(BaseOperationStoreError::CorruptState);
    }
    Ok(())
}

fn prune_tombstones(directory: &Path) -> Result<(), BaseOperationStoreError> {
    let mut paths = std::fs::read_dir(directory)?
        .filter_map(|entry| match entry {
            Ok(entry) if entry.file_name() == ".onebrain-directory-sync" => None,
            other => Some(other.map(|entry| entry.path())),
        })
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    let remove_count = paths.len().saturating_sub(MAX_PROCESS_TOMBSTONES);
    for path in paths.into_iter().take(remove_count) {
        validate_process_receipt(&path)?;
        std::fs::remove_file(path)?;
    }
    sync_directory(directory)?;
    Ok(())
}

fn next_revision(revision: u64) -> Result<u64, BaseOperationStoreError> {
    let next = revision
        .checked_add(1)
        .ok_or(BaseOperationStoreError::CorruptState)?;
    if next >= MAX_OPERATION_REVISIONS {
        return Err(BaseOperationStoreError::Capacity);
    }
    Ok(next)
}

fn random_id() -> Result<[u8; 32], BaseOperationStoreError> {
    let mut id = [0; 32];
    getrandom::fill(&mut id).map_err(|_| BaseOperationStoreError::EntropyUnavailable)?;
    if id == [0; 32] {
        return Err(BaseOperationStoreError::EntropyUnavailable);
    }
    Ok(id)
}

fn write_new_synced(path: &Path, value: &impl Serialize) -> Result<(), std::io::Error> {
    let bytes = serde_json::to_vec(value).map_err(std::io::Error::other)?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(&bytes)?;
    file.sync_all()
}

fn sync_directory(path: &Path) -> Result<(), std::io::Error> {
    #[cfg(windows)]
    {
        // `FlushFileBuffers` on a directory handle is denied on supported
        // Windows filesystems. A stable barrier file in that exact directory
        // gives us a metadata-journal entry plus an fsynced file handle.
        let barrier = path.join(".onebrain-directory-sync");
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(barrier)?;
        file.write_all(b"onebrain-directory-sync-v1")?;
        file.sync_all()
    }
    #[cfg(not(windows))]
    {
        File::open(path)?.sync_all()
    }
}

fn records_equivalent(left: &DurableOperationRecord, right: &DurableOperationRecord) -> bool {
    left.operation_id == right.operation_id
        && left.kind == right.kind
        && left.principal_digest == right.principal_digest
        && left.state == right.state
        && left.command_blake3 == right.command_blake3
        && left.migration == right.migration
        && left.idempotency_key == right.idempotency_key
        && left.result_blake3 == right.result_blake3
        && left.error == right.error
}

fn error_from_code(value: u16) -> Result<BaseErrorCodeV1, BaseOperationStoreError> {
    Ok(match value {
        1 => BaseErrorCodeV1::InvalidRequest,
        2 => BaseErrorCodeV1::NotFound,
        3 => BaseErrorCodeV1::Conflict,
        4 => BaseErrorCodeV1::Expired,
        5 => BaseErrorCodeV1::RateLimited,
        6 => BaseErrorCodeV1::CapabilityDisabled,
        7 => BaseErrorCodeV1::DependencyUnavailable,
        8 => BaseErrorCodeV1::IncompatibleProfile,
        9 => BaseErrorCodeV1::ResourceExhausted,
        10 => BaseErrorCodeV1::CorruptState,
        11 => BaseErrorCodeV1::ReprovisionRequired,
        12 => BaseErrorCodeV1::UnknownOutcome,
        13 => BaseErrorCodeV1::InternalError,
        _ => return Err(BaseOperationStoreError::CorruptState),
    })
}

fn decode_hex_32(value: &str) -> Result<[u8; 32], BaseOperationStoreError> {
    if value.len() != 64 {
        return Err(BaseOperationStoreError::CorruptState);
    }
    let mut output = [0; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| BaseOperationStoreError::CorruptState)?;
    }
    Ok(output)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn base_ops_failpoint(phase: &str) -> Result<(), BaseOperationStoreError> {
    #[cfg(test)]
    if BASE_OPS_TEST_FAILPOINT.with(|configured| configured.get() == Some(phase)) {
        return Err(BaseOperationStoreError::Io(std::io::Error::other(format!(
            "TX-BASE-OPS-001 failpoint: {phase}"
        ))));
    }
    if std::env::var("ONEBRAIN_BASE_OPS_FAILPOINT").ok().as_deref() == Some(phase) {
        return Err(BaseOperationStoreError::Io(std::io::Error::other(format!(
            "TX-BASE-OPS-001 failpoint: {phase}"
        ))));
    }
    Ok(())
}

#[cfg(test)]
thread_local! {
    static BASE_OPS_TEST_FAILPOINT: std::cell::Cell<Option<&'static str>> = const {
        std::cell::Cell::new(None)
    };
}

fn store_node_error(error: BaseOperationStoreError) -> NodeError {
    NodeError::Storage(format!("Base operation store: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset_path::BootstrapDatasetPathResolver;

    #[test]
    fn durable_protocol_reopens_unknown_without_replaying() {
        let temp = tempfile::tempdir().unwrap();
        let resolver = BootstrapDatasetPathResolver::new(temp.path()).unwrap();
        let process = ProcessGenerationId([7; 32]);
        let store = BaseOperationStore::open(&resolver, process).unwrap();
        let reservation = store
            .reserve_operation(BaseOperationKindV1::ExistingLocalCommand, [9; 32])
            .unwrap();
        let prepared = store
            .prepare(
                reservation,
                BaseOperationKindV1::ExistingLocalCommand,
                vec![1, 2, 3],
                None,
                [9; 32],
            )
            .unwrap();
        store
            .begin_confirm(prepared.operation_id, BaseIdempotencyKey([8; 32]), [9; 32])
            .unwrap();
        drop(store);

        let reopened = BaseOperationStore::open(&resolver, ProcessGenerationId([6; 32])).unwrap();
        let reconciled = reopened.reconcile(prepared.operation_id, [9; 32]).unwrap();
        assert_eq!(
            reconciled.receipt.state,
            BaseOperationStateV1::UnknownOutcome
        );
        assert!(!reconciled.resumed_effect);
    }

    #[test]
    fn tx_base_ops_five_phase_reopen_never_observes_a_partial_revision() {
        for phase in [
            "before_begin_write",
            "after_begin_write_before_mutation",
            "after_mutation_before_commit",
            "after_commit_before_next_side_effect",
            "after_next_side_effect_before_ack",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let resolver = BootstrapDatasetPathResolver::new(temp.path()).unwrap();
            let process = ProcessGenerationId([7; 32]);
            let store = BaseOperationStore::open(&resolver, process).unwrap();
            BASE_OPS_TEST_FAILPOINT.with(|configured| configured.set(Some(phase)));
            assert!(store
                .reserve_operation(BaseOperationKindV1::ExistingLocalCommand, [9; 32])
                .is_err());
            BASE_OPS_TEST_FAILPOINT.with(|configured| configured.set(None));
            drop(store);
            let reopened = BaseOperationStore::open(&resolver, process).unwrap();
            let count = reopened.records.lock().unwrap().len();
            let expected = usize::from(matches!(
                phase,
                "after_commit_before_next_side_effect" | "after_next_side_effect_before_ack"
            ));
            assert_eq!(count, expected, "phase {phase}");
        }
    }

    #[test]
    fn archive_adapter_restores_nonterminal_operations_as_reconcile_only() {
        let source_temp = tempfile::tempdir().unwrap();
        let source_resolver = BootstrapDatasetPathResolver::new(source_temp.path()).unwrap();
        let source = Arc::new(
            BaseOperationStore::open(&source_resolver, ProcessGenerationId([1; 32])).unwrap(),
        );
        let reservation = source
            .reserve_operation(BaseOperationKindV1::RestoreArchive, [9; 32])
            .unwrap();
        source.create_subscription([4; 32], [9; 32], 2, 7).unwrap();
        source
            .register_authority(
                [5; 32],
                BaseAuthorityKindV1::ArchiveCapability,
                [9; 32],
                Some(reservation.0),
                Some(1),
            )
            .unwrap();
        source
            .transition_authority(
                [5; 32],
                BaseAuthorityKindV1::ArchiveCapability,
                [9; 32],
                BaseAuthorityStateV1::Active,
            )
            .unwrap();
        let rows = source.archive_rows().unwrap();
        assert!(rows.iter().any(|row| row.table == 1));
        assert!(rows.iter().any(|row| row.table == 2));
        assert!(rows.iter().any(|row| row.table == 3));

        let target_temp = tempfile::tempdir().unwrap();
        let target_resolver = BootstrapDatasetPathResolver::new(target_temp.path()).unwrap();
        let target = Arc::new(
            BaseOperationStore::open(&target_resolver, ProcessGenerationId([2; 32])).unwrap(),
        );
        for row in rows {
            target.restore_row(&row).unwrap();
        }
        let receipt = target
            .reconcile(BaseOperationId(reservation.0), [9; 32])
            .unwrap()
            .receipt;
        assert_eq!(receipt.state, BaseOperationStateV1::UnknownOutcome);
        assert!(receipt.reconcile_required);
        assert!(target
            .subscriptions
            .lock()
            .unwrap()
            .values()
            .all(|row| row.closed));
        assert!(target
            .authority_records
            .lock()
            .unwrap()
            .values()
            .all(|row| row.state == BaseAuthorityStateV1::Revoked));
    }

    #[test]
    fn migration_binding_is_exact_across_prepare_unknown_and_reconcile() {
        let temp = tempfile::tempdir().unwrap();
        let resolver = BootstrapDatasetPathResolver::new(temp.path()).unwrap();
        let store = BaseOperationStore::open(&resolver, ProcessGenerationId([7; 32])).unwrap();
        let reservation = store
            .reserve_operation(BaseOperationKindV1::RestoreArchive, [9; 32])
            .unwrap();
        let vector = MigrationVectorBindingV1 {
            vector_id: onebrain_base_contract::MigrationVectorIdV1::try_from_string(
                "migration-v1".into(),
            )
            .unwrap(),
            vector_blake3: onebrain_base_contract::CompatibilityDigestV1([3; 32]),
            trust_policy_digest: onebrain_base_contract::CompatibilityDigestV1([4; 32]),
        };
        let prepared = store
            .prepare(
                reservation,
                BaseOperationKindV1::RestoreArchive,
                vec![3, 1],
                Some(&vector),
                [9; 32],
            )
            .unwrap();
        let conflicting = MigrationVectorBindingV1 {
            vector_blake3: onebrain_base_contract::CompatibilityDigestV1([8; 32]),
            ..vector.clone()
        };
        assert!(matches!(
            store.prepare(
                reservation,
                BaseOperationKindV1::RestoreArchive,
                vec![3, 1],
                Some(&conflicting),
                [9; 32],
            ),
            Err(BaseOperationStoreError::Conflict)
        ));
        store
            .begin_confirm(prepared.operation_id, BaseIdempotencyKey([6; 32]), [9; 32])
            .unwrap();
        let receipt = store.mark_unknown(prepared.operation_id).unwrap();
        assert_eq!(receipt.migration, Some(vector));
        assert!(receipt.reconcile_required);
    }

    #[test]
    fn authority_records_are_durable_but_never_reactivated_after_restart() {
        let temp = tempfile::tempdir().unwrap();
        let resolver = BootstrapDatasetPathResolver::new(temp.path()).unwrap();
        let store = BaseOperationStore::open(&resolver, ProcessGenerationId([7; 32])).unwrap();
        store
            .register_authority(
                [5; 32],
                BaseAuthorityKindV1::ManagementGrant,
                [9; 32],
                None,
                None,
            )
            .unwrap();
        store
            .transition_authority(
                [5; 32],
                BaseAuthorityKindV1::ManagementGrant,
                [9; 32],
                BaseAuthorityStateV1::Active,
            )
            .unwrap();
        drop(store);

        let reopened = BaseOperationStore::open(&resolver, ProcessGenerationId([8; 32])).unwrap();
        let record = reopened
            .authority_records
            .lock()
            .unwrap()
            .get(&[5; 32])
            .cloned()
            .unwrap();
        assert_eq!(record.state, BaseAuthorityStateV1::Revoked);
        assert_eq!(record.process_generation, ProcessGenerationId([8; 32]));
        assert!(matches!(
            reopened.transition_authority(
                [5; 32],
                BaseAuthorityKindV1::ManagementGrant,
                [9; 32],
                BaseAuthorityStateV1::Active,
            ),
            Err(BaseOperationStoreError::Conflict)
        ));
    }
}
