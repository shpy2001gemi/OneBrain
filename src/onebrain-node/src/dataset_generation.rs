use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ku_core::foundation::{
    AcceptedRecordEntry, FeedEventSigner, LocalSourceTextRecordV1, ObjectCid, ReservedDomain,
    SourceTextError, StoredRecordKind, VaultSourceSnapshotRecord,
};
use ku_kql::blob_storage::BlobStorageError;
use onebrain_archive::{
    materialize_verified_dataset, ArchiveEntryId, ArchiveEntryKind, ArchiveEntryV1, ArchiveError,
    ArchiveRestorePolicyV1, DatasetManifestV1, SignerRecoveryDisposition, VerifiedDatasetArchiveV2,
    VerifiedDatasetMaterializer,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::activation_journal::{
    read_current_pointer, read_idempotency_receipt, read_latest_journal, read_receipt,
    write_current_pointer, write_journal, write_receipt, ActivationJournalRecord, ActivationPhase,
    DatasetGenerationReceipt,
};
use crate::dataset_path::{
    ActiveDatasetPathResolver, BaseStorageOwnerId, DatasetGenerationId, DatasetPathResolver,
};
use crate::dataset_root_lease::{DatasetRootLease, DatasetRootLeaseError};
use crate::derived_index::VNextDerivedIndexManager;
use crate::derived_projection::{DerivedProjectionOpenState, RetrieverProjectionService};
use crate::identity_recovery::{
    clear_reprovision_requirement, recover_policies, verify_reprovision_requirement,
    IdentityRecoveryError, IdentityRecoveryOutcome, IdentityRecoveryReceipt, RecoveredSignerSet,
    SignerRecoveryPolicy, SignerReprovisionRequirement,
};
use crate::signer_ports::{ActorRootSigner, SignerPossessionProof, SignerProviderRegistry};

const COMPLETE_PROFILE: &str = "onebrain/base-staged-generation/1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestoreOperationBinding {
    pub operation_id: [u8; 32],
    pub idempotency_key: [u8; 32],
}

pub struct StagedDatasetGeneration {
    generation_root: [u8; 32],
    generation_path: PathBuf,
    manifest: DatasetManifestV1,
    signer_recovery: SignerRecoveryDisposition,
}

pub struct ActivationReadyGeneration {
    generation_root: [u8; 32],
    generation_path: PathBuf,
    manifest: DatasetManifestV1,
    signer_recovery: SignerRecoveryDisposition,
    identity_recovery: IdentityRecoveryReceipt,
    recovered_signers: RecoveredSignerSet,
}

pub struct DatasetGenerationStore {
    root: PathBuf,
    control: PathBuf,
    _root_lease: DatasetRootLease,
    state: Mutex<GenerationState>,
    recovered_signers: Mutex<RecoveredSignerSet>,
}

#[derive(Clone, Copy)]
struct GenerationState {
    sequence: u64,
    current_root: [u8; 32],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CompleteMarker {
    profile: String,
    manifest_root: [u8; 32],
    manifest_blake3: [u8; 32],
    entry_count: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CompletedReprovision {
    requirement_digest: [u8; 32],
    proof_digest: [u8; 32],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DurableIdentityRecoveryState {
    revision: u64,
    receipt: IdentityRecoveryReceipt,
    completed: Vec<CompletedReprovision>,
}

#[derive(Serialize, Deserialize)]
struct DurableIdentityRecoveryEnvelope {
    state: DurableIdentityRecoveryState,
    checksum: [u8; 32],
}

#[derive(Debug, Error)]
pub enum RestoreError {
    #[error("dataset root is already in use")]
    DatasetRootInUse,
    #[error("dataset root is unsafe")]
    UnsafeRoot,
    #[error("archive restore failed: {0}")]
    Archive(#[from] ArchiveError),
    #[error("dataset generation I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("dataset generation state is corrupt")]
    CorruptState,
    #[error("target generation is not empty")]
    TargetNonEmpty,
    #[error("restore operation or idempotency key conflicts")]
    OperationConflict,
    #[error("activation health or projection parity failed")]
    HealthCheck,
    #[error("activation failpoint interrupted the operation")]
    InjectedFailure,
    #[error("activation outcome is unknown")]
    UnknownOutcome,
    #[error("identity recovery failed: {0}")]
    IdentityRecovery(#[from] IdentityRecoveryError),
}

impl From<DatasetRootLeaseError> for RestoreError {
    fn from(error: DatasetRootLeaseError) -> Self {
        match error {
            DatasetRootLeaseError::DatasetRootInUse => Self::DatasetRootInUse,
            DatasetRootLeaseError::UnsafeRoot => Self::UnsafeRoot,
            DatasetRootLeaseError::Io(error) => Self::Io(error),
        }
    }
}

impl DatasetGenerationStore {
    pub fn open_exclusive(root: &Path) -> Result<Self, RestoreError> {
        let root_lease = DatasetRootLease::acquire(root)?;
        let root = root_lease.root().to_path_buf();
        let control = root.join("control");
        std::fs::create_dir_all(root.join("datasets/generations"))?;
        std::fs::create_dir_all(root.join("datasets/staging"))?;
        let (sequence, current_root) =
            match read_current_pointer(&control).map_err(|_| RestoreError::CorruptState)? {
                Some(value) => value,
                None => {
                    adopt_pre_generation_vnext(&root)?;
                    bootstrap_generation(&root)?;
                    write_current_pointer(&control, 0, [0; 32])
                        .map_err(|_| RestoreError::CorruptState)?;
                    (0, [0; 32])
                }
            };
        let store = Self {
            root,
            control,
            _root_lease: root_lease,
            state: Mutex::new(GenerationState {
                sequence,
                current_root,
            }),
            recovered_signers: Mutex::new(RecoveredSignerSet::empty()),
        };
        store.recover_latest()?;
        let selected = store
            .state
            .lock()
            .map_err(|_| RestoreError::CorruptState)?
            .current_root;
        verify_published_generation(&generation_path(&store.root, selected), selected)?;
        store.recover_exportable_signers(selected)?;
        Ok(store)
    }

    pub fn current_resolver(&self) -> Result<ActiveDatasetPathResolver, RestoreError> {
        let state = *self.state.lock().map_err(|_| RestoreError::CorruptState)?;
        ActiveDatasetPathResolver::new(&self.root, DatasetGenerationId(state.current_root))
            .map_err(|_| RestoreError::CorruptState)
    }

    pub fn stage_verified_restore(
        &self,
        verified: VerifiedDatasetArchiveV2,
        expected: &ArchiveRestorePolicyV1,
    ) -> Result<StagedDatasetGeneration, RestoreError> {
        let mut materializer = GenerationMaterializer::new(&self.root);
        let result = match materialize_verified_dataset(verified, expected, &mut materializer) {
            Err(ArchiveError::RestoreSink(message)) if message == "TARGET_NON_EMPTY" => {
                return Err(RestoreError::TargetNonEmpty)
            }
            result => result?,
        };
        let generation_path = materializer
            .published_path
            .take()
            .ok_or(RestoreError::CorruptState)?;
        Ok(StagedDatasetGeneration {
            generation_root: result.manifest.aggregate_root,
            generation_path,
            manifest: result.manifest,
            signer_recovery: result.signer_recovery,
        })
    }

    /// Only the identity-recovery module may create the private one-shot
    /// activation token, after canonical/projection health and signer policy
    /// evaluation have both completed.
    pub(crate) fn prepare_activation_after_identity(
        &self,
        staged: StagedDatasetGeneration,
        identity: IdentityRecoveryOutcome,
    ) -> Result<ActivationReadyGeneration, RestoreError> {
        let generation_root = staged.generation_root;
        let generation_path = staged.generation_path.clone();
        let result = (|| {
            verify_generation(&staged.generation_path, &staged.manifest)?;
            rebuild_projection_bindings(&staged.generation_path, staged.generation_root)?;
            persist_identity_recovery_receipt(&staged.generation_path, &identity.receipt)?;
            Ok(ActivationReadyGeneration {
                generation_root: staged.generation_root,
                generation_path: staged.generation_path,
                manifest: staged.manifest,
                signer_recovery: staged.signer_recovery,
                identity_recovery: identity.receipt,
                recovered_signers: identity.signers,
            })
        })();
        if result.is_err() {
            self.cleanup_unactivated_generation(&generation_path, generation_root)?;
        }
        result
    }

    fn cleanup_unactivated_generation(
        &self,
        path: &Path,
        generation_root: [u8; 32],
    ) -> Result<(), RestoreError> {
        let current = self
            .state
            .lock()
            .map_err(|_| RestoreError::CorruptState)?
            .current_root;
        if current == generation_root || path != generation_path(&self.root, generation_root) {
            return Err(RestoreError::CorruptState);
        }
        let generations = self.root.join("datasets/generations").canonicalize()?;
        let candidate = path.canonicalize()?;
        if candidate == generations || !candidate.starts_with(&generations) {
            return Err(RestoreError::UnsafeRoot);
        }
        std::fs::remove_dir_all(candidate)?;
        sync_directory(&generations)?;
        Ok(())
    }

    pub(crate) fn staged_identity_policies(
        &self,
        staged: &StagedDatasetGeneration,
    ) -> Result<Vec<SignerRecoveryPolicy>, RestoreError> {
        load_identity_policies(&staged.generation_path, &staged.manifest)
    }

    pub(crate) fn staged_generation_id(
        &self,
        staged: &StagedDatasetGeneration,
    ) -> DatasetGenerationId {
        DatasetGenerationId(staged.generation_root)
    }

    pub(crate) fn staged_resolver(
        &self,
        staged: &StagedDatasetGeneration,
    ) -> Result<ActiveDatasetPathResolver, RestoreError> {
        ActiveDatasetPathResolver::new(&self.root, DatasetGenerationId(staged.generation_root))
            .map_err(|_| RestoreError::CorruptState)
    }

    pub(crate) fn staged_manifest<'a>(
        &self,
        staged: &'a StagedDatasetGeneration,
    ) -> &'a DatasetManifestV1 {
        &staged.manifest
    }

    pub(crate) fn staged_entry_payload(
        &self,
        staged: &StagedDatasetGeneration,
        entry: &ArchiveEntryV1,
    ) -> Result<Vec<u8>, RestoreError> {
        std::fs::read(entry_path(&staged.generation_path, entry)?).map_err(RestoreError::from)
    }

    pub(crate) fn discard_staged_identity_failure(
        &self,
        staged: &StagedDatasetGeneration,
    ) -> Result<(), RestoreError> {
        self.cleanup_unactivated_generation(&staged.generation_path, staged.generation_root)
    }

    fn recover_exportable_signers(&self, generation: [u8; 32]) -> Result<(), RestoreError> {
        if generation == DatasetGenerationId::BOOTSTRAP.0 {
            return Ok(());
        }
        let path = generation_path(&self.root, generation);
        let manifest =
            DatasetManifestV1::from_canonical_bytes(&std::fs::read(path.join("manifest.bin"))?)?;
        let outcome = recover_policies(
            load_identity_policies(&path, &manifest)?,
            DatasetGenerationId(generation),
            None,
        )?;
        self.recovered_signers
            .lock()
            .map_err(|_| RestoreError::CorruptState)?
            .merge(outcome.signers);
        Ok(())
    }

    pub(crate) fn activate_generation(
        &self,
        ready: ActivationReadyGeneration,
        operation: RestoreOperationBinding,
    ) -> Result<DatasetGenerationReceipt, RestoreError> {
        verify_generation(&ready.generation_path, &ready.manifest)?;
        verify_projection_bindings(&ready.generation_path, ready.generation_root)?;
        let _signer_recovery = ready.signer_recovery;
        if let Some(existing) = read_receipt(&self.control, operation.operation_id)
            .map_err(|_| RestoreError::CorruptState)?
        {
            if existing.idempotency_key == operation.idempotency_key
                && existing.new_generation_root == ready.generation_root
            {
                return Ok(existing);
            }
            return Err(RestoreError::OperationConflict);
        }
        if let Some(existing) = read_idempotency_receipt(&self.control, operation.idempotency_key)
            .map_err(|_| RestoreError::CorruptState)?
        {
            if existing.operation_id == operation.operation_id
                && existing.new_generation_root == ready.generation_root
            {
                return Ok(existing);
            }
            return Err(RestoreError::OperationConflict);
        }

        let mut state = self.state.lock().map_err(|_| RestoreError::CorruptState)?;
        let next_sequence = state
            .sequence
            .checked_add(1)
            .ok_or(RestoreError::CorruptState)?;
        let next_revision =
            match read_latest_journal(&self.control).map_err(|_| RestoreError::CorruptState)? {
                Some(record) => record
                    .revision
                    .checked_add(1)
                    .ok_or(RestoreError::CorruptState)?,
                None => 0,
            };
        let mut journal = ActivationJournalRecord {
            revision: next_revision,
            pointer_sequence: next_sequence,
            operation_id: operation.operation_id,
            idempotency_key: operation.idempotency_key,
            old_generation_root: state.current_root,
            new_generation_root: ready.generation_root,
            phase: ActivationPhase::Prepared,
            receipt: None,
        };
        write_journal(&self.control, &journal).map_err(|_| RestoreError::CorruptState)?;
        failpoint("after_prepared")?;

        write_current_pointer(&self.control, next_sequence, ready.generation_root)
            .map_err(|_| RestoreError::CorruptState)?;
        state.sequence = next_sequence;
        state.current_root = ready.generation_root;
        failpoint("after_pointer")?;

        journal.revision = journal
            .revision
            .checked_add(1)
            .ok_or(RestoreError::CorruptState)?;
        journal.phase = ActivationPhase::PointerPublished;
        write_journal(&self.control, &journal).map_err(|_| RestoreError::CorruptState)?;
        failpoint("after_pointer_journal")?;

        if failpoint("reopen_health_failure")
            .and_then(|()| verify_generation(&ready.generation_path, &ready.manifest))
            .and_then(|()| {
                verify_projection_bindings(&ready.generation_path, ready.generation_root)
            })
            .is_err()
        {
            let rollback_sequence = next_sequence
                .checked_add(1)
                .ok_or(RestoreError::CorruptState)?;
            write_current_pointer(
                &self.control,
                rollback_sequence,
                journal.old_generation_root,
            )
            .map_err(|_| RestoreError::CorruptState)?;
            state.sequence = rollback_sequence;
            state.current_root = journal.old_generation_root;
            let receipt = DatasetGenerationReceipt {
                operation_id: operation.operation_id,
                idempotency_key: operation.idempotency_key,
                old_generation_root: journal.old_generation_root,
                new_generation_root: ready.generation_root,
                generation_sequence: next_sequence,
                phase: ActivationPhase::RolledBack,
            };
            carry_receipt_to_generation(&self.root, journal.old_generation_root, &receipt)?;
            write_receipt(&self.control, &receipt).map_err(|_| RestoreError::CorruptState)?;
            journal.revision = journal
                .revision
                .checked_add(1)
                .ok_or(RestoreError::CorruptState)?;
            journal.phase = ActivationPhase::RolledBack;
            journal.receipt = Some(receipt);
            write_journal(&self.control, &journal).map_err(|_| RestoreError::CorruptState)?;
            return Err(RestoreError::HealthCheck);
        }
        let receipt = DatasetGenerationReceipt {
            operation_id: operation.operation_id,
            idempotency_key: operation.idempotency_key,
            old_generation_root: journal.old_generation_root,
            new_generation_root: ready.generation_root,
            generation_sequence: next_sequence,
            phase: ActivationPhase::Complete,
        };
        carry_receipt_to_generation(&self.root, ready.generation_root, &receipt)?;
        write_receipt(&self.control, &receipt).map_err(|_| RestoreError::CorruptState)?;
        failpoint("after_receipt")?;
        journal.revision = journal
            .revision
            .checked_add(1)
            .ok_or(RestoreError::CorruptState)?;
        journal.phase = ActivationPhase::Complete;
        journal.receipt = Some(receipt.clone());
        write_journal(&self.control, &journal).map_err(|_| RestoreError::CorruptState)?;
        self.recovered_signers
            .lock()
            .map_err(|_| RestoreError::CorruptState)?
            .merge(ready.recovered_signers);
        Ok(receipt)
    }

    pub fn activate_restore(
        &self,
        ready: ActivationReadyGeneration,
        operation: RestoreOperationBinding,
    ) -> Result<crate::archive::DatasetRestoreReceipt, RestoreError> {
        let identity = ready.identity_recovery.clone();
        let activation = self.activate_generation(ready, operation)?;
        Ok(crate::archive::DatasetRestoreReceipt {
            activation,
            identity,
        })
    }

    pub fn complete_reprovision(
        &self,
        requirement: &SignerReprovisionRequirement,
        proof: &SignerPossessionProof,
        registry: &dyn SignerProviderRegistry,
    ) -> Result<IdentityRecoveryReceipt, RestoreError> {
        identity_recovery_failpoint("before_begin_write")?;
        let generation = self
            .state
            .lock()
            .map_err(|_| RestoreError::CorruptState)?
            .current_root;
        if requirement.expected.domain() != proof.challenge.domain
            || proof.challenge.dataset_generation != DatasetGenerationId(generation)
        {
            return Err(IdentityRecoveryError::Signer(
                crate::signer_ports::SignerError::InvalidProof,
            )
            .into());
        }
        let generation_path = generation_path(&self.root, generation);
        let mut durable = read_identity_recovery_state(&generation_path)?;
        identity_recovery_failpoint("after_begin_write_before_mutation")?;
        let requirement_digest = recovery_value_digest("requirement", requirement)?;
        let proof_digest = recovery_value_digest("proof", proof)?;
        if let Some(completed) = durable
            .completed
            .iter()
            .find(|completed| completed.requirement_digest == requirement_digest)
        {
            return if completed.proof_digest == proof_digest {
                let signers = verify_reprovision_requirement(
                    requirement,
                    proof,
                    DatasetGenerationId(generation),
                    registry,
                )?;
                self.recovered_signers
                    .lock()
                    .map_err(|_| RestoreError::CorruptState)?
                    .merge(signers);
                Ok(durable.receipt)
            } else {
                Err(
                    IdentityRecoveryError::Signer(crate::signer_ports::SignerError::InvalidProof)
                        .into(),
                )
            };
        }
        if durable
            .completed
            .iter()
            .any(|completed| completed.proof_digest == proof_digest)
        {
            return Err(IdentityRecoveryError::Signer(
                crate::signer_ports::SignerError::InvalidProof,
            )
            .into());
        }
        if durable.receipt.dataset_generation != DatasetGenerationId(generation) {
            return Err(RestoreError::CorruptState);
        }
        let signers = verify_reprovision_requirement(
            requirement,
            proof,
            DatasetGenerationId(generation),
            registry,
        )?;
        durable.receipt = clear_reprovision_requirement(&durable.receipt, requirement)?;
        durable.completed.push(CompletedReprovision {
            requirement_digest,
            proof_digest,
        });
        durable.revision = durable
            .revision
            .checked_add(1)
            .ok_or(RestoreError::CorruptState)?;
        identity_recovery_failpoint("after_mutation_before_commit")?;
        write_identity_recovery_state(&generation_path, &durable)?;
        identity_recovery_failpoint("after_commit_before_next_side_effect")?;
        self.recovered_signers
            .lock()
            .map_err(|_| RestoreError::CorruptState)?
            .merge(signers);
        identity_recovery_failpoint("after_next_side_effect_before_ack")?;
        Ok(durable.receipt)
    }

    pub fn session_identity_signer(
        &self,
    ) -> Result<
        Option<std::sync::Arc<dyn ku_net::vnext_session::SessionIdentitySigner>>,
        RestoreError,
    > {
        Ok(self
            .recovered_signers
            .lock()
            .map_err(|_| RestoreError::CorruptState)?
            .session
            .clone())
    }

    pub fn actor_root_signer(
        &self,
    ) -> Result<Option<std::sync::Arc<dyn ActorRootSigner>>, RestoreError> {
        Ok(self
            .recovered_signers
            .lock()
            .map_err(|_| RestoreError::CorruptState)?
            .actor_root
            .clone())
    }

    pub fn feed_event_signer(
        &self,
    ) -> Result<Option<std::sync::Arc<dyn FeedEventSigner>>, RestoreError> {
        Ok(self
            .recovered_signers
            .lock()
            .map_err(|_| RestoreError::CorruptState)?
            .feed
            .clone())
    }

    pub fn recover_activation(
        &self,
        operation_id: [u8; 32],
    ) -> Result<DatasetGenerationReceipt, RestoreError> {
        if let Some(receipt) =
            read_receipt(&self.control, operation_id).map_err(|_| RestoreError::CorruptState)?
        {
            return Ok(receipt);
        }
        self.recover_latest()?;
        read_receipt(&self.control, operation_id)
            .map_err(|_| RestoreError::CorruptState)?
            .ok_or(RestoreError::UnknownOutcome)
    }

    fn recover_latest(&self) -> Result<(), RestoreError> {
        let Some(mut journal) =
            read_latest_journal(&self.control).map_err(|_| RestoreError::CorruptState)?
        else {
            return Ok(());
        };
        if matches!(
            journal.phase,
            ActivationPhase::Complete | ActivationPhase::RolledBack
        ) {
            if let Some(receipt) = &journal.receipt {
                write_receipt(&self.control, receipt).map_err(|_| RestoreError::CorruptState)?;
            }
            let selected = if journal.phase == ActivationPhase::Complete {
                journal.new_generation_root
            } else {
                journal.old_generation_root
            };
            let pointer = read_current_pointer(&self.control)
                .map_err(|_| RestoreError::CorruptState)?
                .ok_or(RestoreError::CorruptState)?;
            let sequence = if pointer.1 == selected {
                pointer.0
            } else {
                let sequence = pointer.0.checked_add(1).ok_or(RestoreError::CorruptState)?;
                write_current_pointer(&self.control, sequence, selected)
                    .map_err(|_| RestoreError::CorruptState)?;
                sequence
            };
            let mut state = self.state.lock().map_err(|_| RestoreError::CorruptState)?;
            state.sequence = sequence;
            state.current_root = selected;
            return Ok(());
        }
        let pointer = read_current_pointer(&self.control)
            .map_err(|_| RestoreError::CorruptState)?
            .ok_or(RestoreError::CorruptState)?;
        let new_path = generation_path(&self.root, journal.new_generation_root);
        let new_complete =
            verify_published_generation(&new_path, journal.new_generation_root).is_ok();
        let (selected, phase) = if pointer.1 == journal.new_generation_root && new_complete {
            (journal.new_generation_root, ActivationPhase::Complete)
        } else if pointer.1 == journal.old_generation_root {
            (journal.old_generation_root, ActivationPhase::RolledBack)
        } else if pointer.1 == journal.new_generation_root {
            write_current_pointer(
                &self.control,
                pointer.0.checked_add(1).ok_or(RestoreError::CorruptState)?,
                journal.old_generation_root,
            )
            .map_err(|_| RestoreError::CorruptState)?;
            (journal.old_generation_root, ActivationPhase::RolledBack)
        } else {
            write_current_pointer(
                &self.control,
                pointer.0.checked_add(1).ok_or(RestoreError::CorruptState)?,
                journal.old_generation_root,
            )
            .map_err(|_| RestoreError::CorruptState)?;
            (journal.old_generation_root, ActivationPhase::UnknownOutcome)
        };
        let receipt = DatasetGenerationReceipt {
            operation_id: journal.operation_id,
            idempotency_key: journal.idempotency_key,
            old_generation_root: journal.old_generation_root,
            new_generation_root: journal.new_generation_root,
            generation_sequence: journal.pointer_sequence,
            phase,
        };
        carry_receipt_to_generation(&self.root, selected, &receipt)?;
        write_receipt(&self.control, &receipt).map_err(|_| RestoreError::CorruptState)?;
        journal.revision = journal
            .revision
            .checked_add(1)
            .ok_or(RestoreError::CorruptState)?;
        journal.phase = phase;
        journal.receipt = Some(receipt);
        write_journal(&self.control, &journal).map_err(|_| RestoreError::CorruptState)?;
        let pointer = read_current_pointer(&self.control)
            .map_err(|_| RestoreError::CorruptState)?
            .ok_or(RestoreError::CorruptState)?;
        let mut state = self.state.lock().map_err(|_| RestoreError::CorruptState)?;
        state.sequence = pointer.0;
        state.current_root = selected;
        Ok(())
    }
}

impl DatasetPathResolver for DatasetGenerationStore {
    fn current_generation(&self) -> DatasetGenerationId {
        DatasetGenerationId(
            self.state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .current_root,
        )
    }

    fn owner_path(&self, owner: BaseStorageOwnerId) -> Result<PathBuf, BlobStorageError> {
        let generation = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .current_root;
        ActiveDatasetPathResolver::new(&self.root, DatasetGenerationId(generation))?
            .owner_path(owner)
    }
}

struct GenerationMaterializer {
    root: PathBuf,
    staging_path: Option<PathBuf>,
    manifest: Option<DatasetManifestV1>,
    payloads: BTreeMap<ArchiveEntryId, PathBuf>,
    published_path: Option<PathBuf>,
}

impl GenerationMaterializer {
    fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            staging_path: None,
            manifest: None,
            payloads: BTreeMap::new(),
            published_path: None,
        }
    }
}

impl VerifiedDatasetMaterializer for GenerationMaterializer {
    fn begin(&mut self, manifest: &DatasetManifestV1) -> Result<(), ArchiveError> {
        let staging = self
            .root
            .join("datasets/staging")
            .join(hex(&manifest.aggregate_root));
        let published = generation_path(&self.root, manifest.aggregate_root);
        if staging.exists() || published.exists() {
            return Err(ArchiveError::RestoreSink("TARGET_NON_EMPTY".into()));
        }
        std::fs::create_dir(&staging)?;
        write_new_sync(&staging.join("manifest.bin"), &manifest.canonical_bytes()?)?;
        self.staging_path = Some(staging);
        self.manifest = Some(manifest.clone());
        Ok(())
    }

    fn materialize_entry(
        &mut self,
        entry: &ArchiveEntryV1,
        payload: &[u8],
    ) -> Result<(), ArchiveError> {
        let staging = self.staging_path.as_ref().ok_or(ArchiveError::Integrity)?;
        let owner = staging
            .join("owners")
            .join(owner_name(entry.logical_key.owner.get()).ok_or(ArchiveError::InvalidProfile)?)
            .join(format!("{:04x}", entry.logical_key.namespace));
        std::fs::create_dir_all(&owner)?;
        let path = owner.join(format!("{}.bin", hex(entry.id.as_bytes())));
        write_new_sync(&path, payload)?;
        if self.payloads.insert(entry.id, path).is_some() {
            return Err(ArchiveError::Integrity);
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), ArchiveError> {
        let staging = self.staging_path.as_ref().ok_or(ArchiveError::Integrity)?;
        let manifest = self.manifest.as_ref().ok_or(ArchiveError::Integrity)?;
        if self.payloads.len() != manifest.entries.len() {
            return Err(ArchiveError::Integrity);
        }
        let manifest_bytes = manifest.canonical_bytes()?;
        let marker = CompleteMarker {
            profile: COMPLETE_PROFILE.to_string(),
            manifest_root: manifest.aggregate_root,
            manifest_blake3: *blake3::hash(&manifest_bytes).as_bytes(),
            entry_count: manifest.entries.len() as u64,
        };
        write_new_sync(
            &staging.join("complete.json"),
            &serde_json::to_vec(&marker)?,
        )?;
        sync_directory(staging)?;
        let published = generation_path(&self.root, manifest.aggregate_root);
        std::fs::rename(staging, &published)?;
        self.staging_path = None;
        self.published_path = Some(published.clone());
        sync_directory(
            published
                .parent()
                .ok_or_else(|| ArchiveError::RestoreSink("PATH_ESCAPE".into()))?,
        )?;
        Ok(())
    }

    fn cleanup_failed(&mut self) -> Result<(), ArchiveError> {
        if let Some(staging) = self.staging_path.take() {
            let staging_root = self.root.join("datasets/staging").canonicalize()?;
            let candidate = staging.canonicalize()?;
            if !candidate.starts_with(&staging_root) || candidate == staging_root {
                return Err(ArchiveError::RestoreSink("PATH_ESCAPE".into()));
            }
            std::fs::remove_dir_all(candidate)?;
        }
        if let Some(published) = self.published_path.take() {
            let generations_root = self.root.join("datasets/generations").canonicalize()?;
            let candidate = published.canonicalize()?;
            if !candidate.starts_with(&generations_root) || candidate == generations_root {
                return Err(ArchiveError::RestoreSink("PATH_ESCAPE".into()));
            }
            std::fs::remove_dir_all(candidate)?;
        }
        Ok(())
    }
}

fn bootstrap_generation(root: &Path) -> Result<(), RestoreError> {
    let generation = generation_path(root, [0; 32]);
    std::fs::create_dir_all(generation.join("owners"))?;
    let marker = CompleteMarker {
        profile: COMPLETE_PROFILE.to_string(),
        manifest_root: [0; 32],
        manifest_blake3: [0; 32],
        entry_count: 0,
    };
    if !generation.join("complete.json").exists() {
        write_new_sync(
            &generation.join("complete.json"),
            &serde_json::to_vec(&marker).map_err(|_| RestoreError::CorruptState)?,
        )?;
    }
    Ok(())
}

fn adopt_pre_generation_vnext(root: &Path) -> Result<(), RestoreError> {
    let generation = generation_path(root, [0; 32]);
    std::fs::create_dir_all(generation.join("owners"))?;
    let Some(data_dir) = root.parent() else {
        return Err(RestoreError::UnsafeRoot);
    };
    let files = [
        ("vnext_verified.redb", "canonical"),
        ("vnext_private_need_vault.redb", "private_kql"),
        ("vnext_distributed_kql.redb", "private_kql"),
        ("vnext_standing_needs.redb", "private_kql"),
        ("vnext_public_use_sender.redb", "private_pomv"),
        ("vnext_distributed_pomv.redb", "private_pomv"),
        ("vnext_reconciliation.redb", "reconciliation"),
        ("vnext_inventory.redb", "inventory"),
        ("vnext_outbox.redb", "outbox"),
        ("vnext_record_provenance.redb", "provenance"),
        ("vnext_runtime_rollout.redb", "rollout"),
        ("vnext_identity.key", "identity"),
    ];
    for (name, owner) in files {
        let source = data_dir.join(name);
        if !source.exists() {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&source)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(RestoreError::UnsafeRoot);
        }
        let owner_root = generation.join("owners").join(owner);
        std::fs::create_dir_all(&owner_root)?;
        let target = owner_root.join(name);
        if target.exists() {
            return Err(RestoreError::TargetNonEmpty);
        }
        std::fs::rename(source, target)?;
    }

    let bootstrap_owners = data_dir.join("base-bootstrap/owners");
    for owner in [
        "vault",
        "pending_blob_intent",
        "source_capture_intent",
        "reconciliation",
        "inventory",
        "outbox",
        "provenance",
        "private_kql",
        "private_pomv",
        "operational",
        "rollout",
        "optional_network",
        "migration",
        "base_operations",
        "interpretation_config",
        "identity",
        "registry_metadata",
    ] {
        let source = bootstrap_owners.join(owner);
        if !source.exists() {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&source)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(RestoreError::UnsafeRoot);
        }
        let target = generation.join("owners").join(owner);
        std::fs::create_dir_all(&target)?;
        for entry in std::fs::read_dir(&source)? {
            let entry = entry?;
            let from = entry.path();
            let to = target.join(entry.file_name());
            if to.exists() {
                return Err(RestoreError::TargetNonEmpty);
            }
            if std::fs::symlink_metadata(&from)?.file_type().is_symlink() {
                return Err(RestoreError::UnsafeRoot);
            }
            std::fs::rename(from, to)?;
        }
        std::fs::remove_dir(source)?;
    }
    Ok(())
}

fn verify_generation(path: &Path, manifest: &DatasetManifestV1) -> Result<(), RestoreError> {
    verify_complete_marker(path, manifest.aggregate_root)?;
    let stored_manifest = std::fs::read(path.join("manifest.bin"))?;
    if DatasetManifestV1::from_canonical_bytes(&stored_manifest)? != *manifest {
        return Err(RestoreError::HealthCheck);
    }
    for entry in &manifest.entries {
        let path = entry_path(path, entry)?;
        let bytes = std::fs::read(path)?;
        if bytes.len() as u64 != entry.length || *blake3::hash(&bytes).as_bytes() != entry.blake3 {
            return Err(RestoreError::HealthCheck);
        }
    }
    Ok(())
}

fn verify_complete_marker(path: &Path, root: [u8; 32]) -> Result<(), RestoreError> {
    let bytes = std::fs::read(path.join("complete.json"))?;
    let marker: CompleteMarker =
        serde_json::from_slice(&bytes).map_err(|_| RestoreError::CorruptState)?;
    if marker.profile != COMPLETE_PROFILE || marker.manifest_root != root {
        return Err(RestoreError::HealthCheck);
    }
    Ok(())
}

fn verify_published_generation(path: &Path, root: [u8; 32]) -> Result<(), RestoreError> {
    verify_complete_marker(path, root)?;
    if root == DatasetGenerationId::BOOTSTRAP.0 {
        return Ok(());
    }
    let manifest =
        DatasetManifestV1::from_canonical_bytes(&std::fs::read(path.join("manifest.bin"))?)?;
    if manifest.aggregate_root != root {
        return Err(RestoreError::HealthCheck);
    }
    verify_generation(path, &manifest)?;
    verify_projection_bindings(path, root)?;
    let identity = read_identity_recovery_state(path)?;
    if identity.receipt.dataset_generation != DatasetGenerationId(root) {
        return Err(RestoreError::HealthCheck);
    }
    Ok(())
}

fn persist_identity_recovery_receipt(
    generation: &Path,
    receipt: &IdentityRecoveryReceipt,
) -> Result<(), RestoreError> {
    let state_root = generation.join("owners/identity/identity-recovery-state");
    if state_root.exists() {
        let existing = read_identity_recovery_state(generation)?;
        return if existing.receipt == *receipt {
            Ok(())
        } else {
            Err(RestoreError::OperationConflict)
        };
    }
    write_identity_recovery_state(
        generation,
        &DurableIdentityRecoveryState {
            revision: 0,
            receipt: receipt.clone(),
            completed: Vec::new(),
        },
    )
}

fn read_identity_recovery_state(
    generation: &Path,
) -> Result<DurableIdentityRecoveryState, RestoreError> {
    let root = generation.join("owners/identity/identity-recovery-state");
    let mut valid = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let envelope: DurableIdentityRecoveryEnvelope =
            match serde_json::from_slice(&std::fs::read(entry.path())?) {
                Ok(value) => value,
                Err(_) => continue,
            };
        let checksum = recovery_value_digest("state", &envelope.state)?;
        if checksum == envelope.checksum && completed_state_valid(&envelope.state.completed) {
            valid.push(envelope.state);
        }
    }
    valid.sort_by_key(|state| state.revision);
    if valid
        .windows(2)
        .any(|pair| pair[0].revision == pair[1].revision)
    {
        return Err(RestoreError::CorruptState);
    }
    valid.pop().ok_or(RestoreError::CorruptState)
}

fn completed_state_valid(completed: &[CompletedReprovision]) -> bool {
    if completed.len() > 3 {
        return false;
    }
    let requirements: BTreeMap<_, _> = completed
        .iter()
        .map(|entry| (entry.requirement_digest, entry.proof_digest))
        .collect();
    let proofs: BTreeMap<_, _> = completed
        .iter()
        .map(|entry| (entry.proof_digest, entry.requirement_digest))
        .collect();
    requirements.len() == completed.len() && proofs.len() == completed.len()
}

fn write_identity_recovery_state(
    generation: &Path,
    state: &DurableIdentityRecoveryState,
) -> Result<(), RestoreError> {
    let root = generation.join("owners/identity/identity-recovery-state");
    std::fs::create_dir_all(&root)?;
    let envelope = DurableIdentityRecoveryEnvelope {
        checksum: recovery_value_digest("state", state)?,
        state: state.clone(),
    };
    let bytes = serde_json::to_vec(&envelope).map_err(|_| RestoreError::CorruptState)?;
    write_new_sync(&root.join(format!("{:020}.json", state.revision)), &bytes)?;
    sync_directory(&root)?;
    Ok(())
}

fn recovery_value_digest(label: &str, value: &impl Serialize) -> Result<[u8; 32], RestoreError> {
    let bytes = serde_json::to_vec(value).map_err(|_| RestoreError::CorruptState)?;
    let mut hasher = blake3::Hasher::new_derive_key("onebrain:base-v1:identity-recovery-state:1");
    hasher.update(label.as_bytes());
    hasher.update(&bytes);
    Ok(*hasher.finalize().as_bytes())
}

fn identity_recovery_failpoint(name: &str) -> Result<(), RestoreError> {
    if std::env::var("ONEBRAIN_IDENTITY_RECOVERY_FAILPOINT")
        .ok()
        .as_deref()
        == Some(name)
    {
        Err(RestoreError::InjectedFailure)
    } else {
        Ok(())
    }
}

fn rebuild_projection_bindings(path: &Path, root: [u8; 32]) -> Result<(), RestoreError> {
    let (accepted, vault_sources, vault_source_root) = generation_projection_inputs(path)?;
    let derived_root = path.join("owners/derived_index");
    let retriever_root = path.join("owners/retriever_projection");
    let manager =
        VNextDerivedIndexManager::new(&derived_root).map_err(|_| RestoreError::HealthCheck)?;
    let report = manager
        .rebuild_entries(accepted)
        .map_err(|_| RestoreError::HealthCheck)?;
    let (_, state) = RetrieverProjectionService::open_or_rebuild(
        retriever_root,
        report.source_root,
        vault_source_root,
        vault_sources,
    );
    if !matches!(
        state,
        DerivedProjectionOpenState::Ready | DerivedProjectionOpenState::Rebuilt
    ) {
        return Err(RestoreError::HealthCheck);
    }
    let binding = serde_json::to_vec(&serde_json::json!({
        "profile": "onebrain/base-generation-projection-binding/1",
        "manifest_root": root,
        "canonical_source_root": report.source_root,
        "vault_source_root": vault_source_root,
        "status": "rebuilt"
    }))
    .map_err(|_| RestoreError::CorruptState)?;
    write_new_sync(&path.join("projection-binding.json"), &binding)?;
    Ok(())
}

fn verify_projection_bindings(path: &Path, root: [u8; 32]) -> Result<(), RestoreError> {
    let bytes = std::fs::read(path.join("projection-binding.json"))?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| RestoreError::HealthCheck)?;
    if value.get("manifest_root") != Some(&serde_json::json!(root))
        || value.get("status") != Some(&serde_json::json!("rebuilt"))
    {
        return Err(RestoreError::HealthCheck);
    }
    let (accepted, vault_sources, vault_source_root) = generation_projection_inputs(path)?;
    let manager = VNextDerivedIndexManager::new(path.join("owners/derived_index"))
        .map_err(|_| RestoreError::HealthCheck)?;
    let report = manager
        .verify_entries(accepted)
        .map_err(|_| RestoreError::HealthCheck)?;
    RetrieverProjectionService::verify_generation(
        path.join("owners/retriever_projection"),
        report.source_root,
        vault_source_root,
        vault_sources,
    )
    .map_err(|_| RestoreError::HealthCheck)?;
    Ok(())
}

fn generation_projection_inputs(
    path: &Path,
) -> Result<
    (
        Vec<AcceptedRecordEntry>,
        Vec<VaultSourceSnapshotRecord>,
        [u8; 32],
    ),
    RestoreError,
> {
    let manifest =
        DatasetManifestV1::from_canonical_bytes(&std::fs::read(path.join("manifest.bin"))?)?;
    let mut accepted = Vec::new();
    let mut vault_sources = Vec::new();
    for entry in &manifest.entries {
        let bytes = std::fs::read(entry_path(path, entry)?)?;
        let kind = match entry.kind {
            ArchiveEntryKind::CanonicalObject => {
                Some((StoredRecordKind::Object, ReservedDomain::Object))
            }
            ArchiveEntryKind::CanonicalEvent => {
                Some((StoredRecordKind::Event, ReservedDomain::Event))
            }
            ArchiveEntryKind::FeedInception => Some((
                StoredRecordKind::FeedInception,
                ReservedDomain::FeedInception,
            )),
            ArchiveEntryKind::AuthorityEvent => Some((
                StoredRecordKind::AuthorityEvent,
                ReservedDomain::AuthorityEvent,
            )),
            _ => None,
        };
        if let Some((record_kind, domain)) = kind {
            accepted.push(AcceptedRecordEntry {
                record_kind,
                claimed_cid: domain.digest(&bytes),
                canonical_bytes: bytes.clone(),
            });
        }
        if entry.kind == ArchiveEntryKind::VaultRecord {
            match LocalSourceTextRecordV1::decode(&bytes) {
                Ok(record) => {
                    let (_, source_record) =
                        record.encode().map_err(|_| RestoreError::HealthCheck)?;
                    vault_sources.push(VaultSourceSnapshotRecord {
                        subject: record.subject,
                        source_record: ObjectCid::from_bytes(source_record.into_bytes()),
                        source_digest: record.source_digest,
                        source_text: record.source_text,
                    });
                }
                Err(SourceTextError::NotSourceText) => {}
                Err(_) => return Err(RestoreError::HealthCheck),
            }
        }
    }
    accepted.sort_by_key(|record| (record.record_kind as u8, record.claimed_cid));
    vault_sources.sort_by_key(|record| {
        (
            record.subject.reference_kind,
            record.subject.cid,
            record.source_record.into_bytes(),
        )
    });
    let mut hasher = blake3::Hasher::new_derive_key("onebrain:vnext:vault-source-root:1");
    for record in &vault_sources {
        hasher.update(&record.subject.reference_kind.to_be_bytes());
        hasher.update(&record.subject.cid);
        hasher.update(record.source_record.as_bytes());
        hasher.update(&record.source_digest);
    }
    let vault_source_root = *hasher.finalize().as_bytes();
    Ok((accepted, vault_sources, vault_source_root))
}

fn entry_path(root: &Path, entry: &ArchiveEntryV1) -> Result<PathBuf, RestoreError> {
    Ok(root
        .join("owners")
        .join(owner_name(entry.logical_key.owner.get()).ok_or(RestoreError::CorruptState)?)
        .join(format!("{:04x}", entry.logical_key.namespace))
        .join(format!("{}.bin", hex(entry.id.as_bytes()))))
}

fn load_identity_policies(
    generation: &Path,
    manifest: &DatasetManifestV1,
) -> Result<Vec<SignerRecoveryPolicy>, RestoreError> {
    let mut policies = Vec::new();
    for entry in &manifest.entries {
        if entry.kind == ArchiveEntryKind::SignerRecoveryPolicy {
            policies.push(SignerRecoveryPolicy::decode(&std::fs::read(entry_path(
                generation, entry,
            )?)?)?);
        }
    }
    if policies.is_empty() {
        return Err(IdentityRecoveryError::InvalidPolicy.into());
    }
    Ok(policies)
}

fn generation_path(root: &Path, generation_root: [u8; 32]) -> PathBuf {
    root.join("datasets/generations")
        .join(hex(&generation_root))
}

fn owner_name(owner: u16) -> Option<&'static str> {
    const OWNERS: [&str; 22] = [
        "canonical",
        "vault",
        "quarantine",
        "blob",
        "pending_blob_intent",
        "source_capture_intent",
        "reconciliation",
        "inventory",
        "outbox",
        "provenance",
        "private_kql",
        "private_pomv",
        "operational",
        "rollout",
        "optional_network",
        "migration",
        "base_operations",
        "interpretation_config",
        "identity",
        "registry_metadata",
        "derived_index",
        "retriever_projection",
    ];
    owner
        .checked_sub(1)
        .and_then(|index| OWNERS.get(index as usize).copied())
}

fn write_new_sync(path: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    let mut output = OpenOptions::new().write(true).create_new(true).open(path)?;
    output.write_all(bytes)?;
    output.sync_all()
}

fn carry_receipt_to_generation(
    root: &Path,
    generation_root: [u8; 32],
    receipt: &DatasetGenerationReceipt,
) -> Result<(), RestoreError> {
    let directory =
        generation_path(root, generation_root).join("owners/base_operations/activation-receipts");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(format!("{}.json", hex(&receipt.operation_id)));
    let bytes = serde_json::to_vec(receipt).map_err(|_| RestoreError::CorruptState)?;
    if path.exists() {
        return if std::fs::read(path)? == bytes {
            Ok(())
        } else {
            Err(RestoreError::OperationConflict)
        };
    }
    write_new_sync(&path, &bytes)?;
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), std::io::Error> {
    std::fs::File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), std::io::Error> {
    Ok(())
}

fn failpoint(name: &str) -> Result<(), RestoreError> {
    if std::env::var("ONEBRAIN_DATASET_FAILPOINT").ok().as_deref() == Some(name) {
        Err(RestoreError::InjectedFailure)
    } else {
        Ok(())
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_adopts_only_allowlisted_vnext_files_without_rewriting_bytes() {
        let data = tempfile::tempdir().unwrap();
        let canonical = data.path().join("vnext_verified.redb");
        std::fs::write(&canonical, b"exact-vnext-bytes").unwrap();
        std::fs::write(data.path().join("ku.redb"), b"legacy-stays-outside").unwrap();
        let store = DatasetGenerationStore::open_exclusive(&data.path().join("base")).unwrap();
        let target = store
            .owner_path(BaseStorageOwnerId::CANONICAL)
            .unwrap()
            .join("vnext_verified.redb");
        assert_eq!(std::fs::read(target).unwrap(), b"exact-vnext-bytes");
        assert!(!canonical.exists());
        assert_eq!(
            std::fs::read(data.path().join("ku.redb")).unwrap(),
            b"legacy-stays-outside"
        );
    }

    #[test]
    fn closed_owner_table_resolves_to_distinct_generation_paths() {
        let data = tempfile::tempdir().unwrap();
        let store = DatasetGenerationStore::open_exclusive(data.path()).unwrap();
        let mut paths = std::collections::BTreeSet::new();
        for code in 1..=22 {
            let owner = BaseStorageOwnerId::new(code).unwrap();
            assert!(paths.insert(store.owner_path(owner).unwrap()));
        }
        assert_eq!(paths.len(), 22);
    }
}
