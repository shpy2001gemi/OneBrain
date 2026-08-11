//! Durable cross-store source capture intent. Canonical storage and the Vault
//! are deliberately reconciled; they are never presented as one transaction.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ku_core::foundation::{
    dr_m5_failpoint, AtomicVerifiedBackend, LocalSourceTextRecordV1, ObjectCid, ObjectReference,
    PrivateVault, VaultStagingId,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::Zeroizing;

use crate::dataset_path::{BaseStorageOwnerId, DatasetGenerationId, DatasetPathResolver};

pub const SOURCE_CAPTURE_BOUNDARY: &str = "TX-SOURCE-001";
pub const MAX_SOURCE_CAPTURE_INTENTS: usize = 4_096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceCaptureRecoveryState {
    Complete,
    FinishVaultBinding,
    QuarantineOrphanSource,
    SourceCaptureIncomplete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncryptedSourceCaptureIntentV1 {
    pub subject: ObjectReference,
    pub canonical_digest: [u8; 32],
    pub source_digest: [u8; 32],
    pub vault_staging_id: VaultStagingId,
    pub target_vault_record: ObjectCid,
    pub dataset_generation: DatasetGenerationId,
    pub metadata_auth_tag: [u8; 32],
}

pub struct SourceCaptureTransactionStore {
    resolver: Arc<dyn DatasetPathResolver>,
    vault_staging_root: PathBuf,
}

impl SourceCaptureTransactionStore {
    pub fn new(
        resolver: Arc<dyn DatasetPathResolver>,
        vault_staging_root: impl AsRef<Path>,
    ) -> Self {
        Self {
            resolver,
            vault_staging_root: vault_staging_root.as_ref().to_path_buf(),
        }
    }

    pub fn prepare<B: AtomicVerifiedBackend>(
        &self,
        vault: &PrivateVault<B>,
        subject: ObjectReference,
        canonical_digest: [u8; 32],
        source_text: String,
    ) -> Result<EncryptedSourceCaptureIntentV1, SourceCaptureError> {
        let source_text = Zeroizing::new(source_text);
        let record = LocalSourceTextRecordV1::new(subject.clone(), source_text.to_string())
            .map_err(|error| SourceCaptureError::Source(error.to_string()))?;
        let (_, target_vault_record) = record
            .encode()
            .map_err(|error| SourceCaptureError::Source(error.to_string()))?;
        let mut staging_bytes = [0; 32];
        getrandom::fill(&mut staging_bytes).map_err(|_| SourceCaptureError::EntropyUnavailable)?;
        let mut intent = EncryptedSourceCaptureIntentV1 {
            subject,
            canonical_digest,
            source_digest: record.source_digest,
            vault_staging_id: VaultStagingId(staging_bytes),
            target_vault_record,
            dataset_generation: self.resolver.current_generation(),
            metadata_auth_tag: [0; 32],
        };
        let metadata = IntentMetadata::from_intent(&intent);
        let metadata_bytes = serde_json::to_vec(&metadata)?;
        intent.metadata_auth_tag = vault.source_intent_auth_tag(&metadata_bytes);

        let directory = self.intent_directory()?;
        let pending_count = std::fs::read_dir(&directory)?
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.path().extension().and_then(|value| value.to_str()) == Some("json")
            })
            .count();
        if pending_count >= MAX_SOURCE_CAPTURE_INTENTS {
            return Err(SourceCaptureError::PendingLimit);
        }
        let final_path = intent_path(&directory, intent.vault_staging_id, "json");
        let temporary_path = intent_path(&directory, intent.vault_staging_id, "preparing");
        if final_path.exists() || temporary_path.exists() {
            return Err(SourceCaptureError::IdCollision);
        }

        dr_m5_failpoint::hit(SOURCE_CAPTURE_BOUNDARY, "before_begin_write");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)?;
        dr_m5_failpoint::hit(SOURCE_CAPTURE_BOUNDARY, "after_begin_write_before_mutation");
        let bytes = serde_json::to_vec(&IntentWire::from_intent(&intent))?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        dr_m5_failpoint::hit(SOURCE_CAPTURE_BOUNDARY, "after_mutation_before_commit");
        std::fs::rename(&temporary_path, &final_path)?;
        sync_directory(&directory)?;
        dr_m5_failpoint::hit(
            SOURCE_CAPTURE_BOUNDARY,
            "after_commit_before_next_side_effect",
        );
        vault
            .stage_source_text(&self.vault_staging_root, intent.vault_staging_id, &record)
            .map_err(|error| SourceCaptureError::Vault(error.to_string()))?;
        dr_m5_failpoint::hit(SOURCE_CAPTURE_BOUNDARY, "after_next_side_effect_before_ack");
        Ok(intent)
    }

    pub fn reconcile<B: AtomicVerifiedBackend>(
        &self,
        vault: &PrivateVault<B>,
        canonical_is_durable: impl Fn(&ObjectReference, &[u8; 32]) -> bool,
    ) -> Result<Vec<(EncryptedSourceCaptureIntentV1, SourceCaptureRecoveryState)>, SourceCaptureError>
    {
        let directory = self.intent_directory()?;
        let mut outcomes = Vec::new();
        let entries = std::fs::read_dir(&directory)?.collect::<Result<Vec<_>, std::io::Error>>()?;
        for entry in &entries {
            if entry.path().extension().and_then(|value| value.to_str()) == Some("preparing") {
                std::fs::remove_file(entry.path())?;
            }
        }
        sync_directory(&directory)?;
        for entry in entries {
            if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let wire: IntentWire = serde_json::from_slice(&std::fs::read(entry.path())?)?;
            let intent = wire.into_intent()?;
            self.authenticate(vault, &intent)?;
            let staged_exists =
                vault.staged_source_exists(&self.vault_staging_root, intent.vault_staging_id);
            let canonical = canonical_is_durable(&intent.subject, &intent.canonical_digest);
            let staged =
                vault.inspect_staged_source(&self.vault_staging_root, intent.vault_staging_id);
            let state = if intent.dataset_generation != self.resolver.current_generation() {
                match staged {
                    Ok(_) => {
                        vault
                            .quarantine_staged_source(
                                &self.vault_staging_root,
                                intent.vault_staging_id,
                                "STALE_DATASET_GENERATION",
                            )
                            .map_err(|error| SourceCaptureError::Vault(error.to_string()))?;
                        SourceCaptureRecoveryState::QuarantineOrphanSource
                    }
                    Err(_) if staged_exists => SourceCaptureRecoveryState::SourceCaptureIncomplete,
                    Err(_) => SourceCaptureRecoveryState::Complete,
                }
            } else if canonical {
                match staged {
                    Ok(record)
                        if record.subject == intent.subject
                            && record.source_digest == intent.source_digest =>
                    {
                        vault
                            .bind_staged_source(
                                &self.vault_staging_root,
                                intent.vault_staging_id,
                                intent.target_vault_record,
                            )
                            .map_err(|error| SourceCaptureError::Vault(error.to_string()))?;
                        SourceCaptureRecoveryState::FinishVaultBinding
                    }
                    Ok(_) => return Err(SourceCaptureError::BindingMismatch),
                    Err(_) => match vault
                        .get_source_text(intent.target_vault_record)
                        .map_err(|error| SourceCaptureError::Vault(error.to_string()))?
                    {
                        Some(record)
                            if record.subject == intent.subject
                                && record.source_digest == intent.source_digest =>
                        {
                            SourceCaptureRecoveryState::Complete
                        }
                        _ => SourceCaptureRecoveryState::SourceCaptureIncomplete,
                    },
                }
            } else {
                match staged {
                    Ok(_) => {
                        vault
                            .quarantine_staged_source(
                                &self.vault_staging_root,
                                intent.vault_staging_id,
                                "UNREFERENCED_SOURCE_CAPTURE",
                            )
                            .map_err(|error| SourceCaptureError::Vault(error.to_string()))?;
                        SourceCaptureRecoveryState::QuarantineOrphanSource
                    }
                    Err(_) if staged_exists => SourceCaptureRecoveryState::SourceCaptureIncomplete,
                    Err(_) => SourceCaptureRecoveryState::Complete,
                }
            };
            if state != SourceCaptureRecoveryState::SourceCaptureIncomplete {
                std::fs::remove_file(entry.path())?;
                sync_directory(&directory)?;
            }
            outcomes.push((intent, state));
        }
        outcomes.sort_by_key(|(intent, _)| intent.vault_staging_id.0);
        Ok(outcomes)
    }

    fn authenticate<B: AtomicVerifiedBackend>(
        &self,
        vault: &PrivateVault<B>,
        intent: &EncryptedSourceCaptureIntentV1,
    ) -> Result<(), SourceCaptureError> {
        let metadata = serde_json::to_vec(&IntentMetadata::from_intent(intent))?;
        if vault.source_intent_auth_tag(&metadata) != intent.metadata_auth_tag {
            return Err(SourceCaptureError::AuthenticationFailed);
        }
        Ok(())
    }

    fn intent_directory(&self) -> Result<PathBuf, SourceCaptureError> {
        self.resolver
            .owner_path(BaseStorageOwnerId::SOURCE_CAPTURE_INTENT)
            .map_err(|error| SourceCaptureError::Path(error.to_string()))
    }
}

#[derive(Serialize)]
struct IntentMetadata {
    version: u8,
    reference_kind: u64,
    subject_cid: String,
    canonical_digest: String,
    source_digest: String,
    vault_staging_id: String,
    target_vault_record: String,
    dataset_generation: String,
}

impl IntentMetadata {
    fn from_intent(intent: &EncryptedSourceCaptureIntentV1) -> Self {
        Self {
            version: 1,
            reference_kind: intent.subject.reference_kind,
            subject_cid: encode_hex(&intent.subject.cid),
            canonical_digest: encode_hex(&intent.canonical_digest),
            source_digest: encode_hex(&intent.source_digest),
            vault_staging_id: encode_hex(&intent.vault_staging_id.0),
            target_vault_record: encode_hex(intent.target_vault_record.as_bytes()),
            dataset_generation: encode_hex(&intent.dataset_generation.0),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IntentWire {
    version: u8,
    reference_kind: u64,
    subject_cid: String,
    canonical_digest: String,
    source_digest: String,
    vault_staging_id: String,
    target_vault_record: String,
    dataset_generation: String,
    metadata_auth_tag: String,
}

impl IntentWire {
    fn from_intent(intent: &EncryptedSourceCaptureIntentV1) -> Self {
        let metadata = IntentMetadata::from_intent(intent);
        Self {
            version: metadata.version,
            reference_kind: metadata.reference_kind,
            subject_cid: metadata.subject_cid,
            canonical_digest: metadata.canonical_digest,
            source_digest: metadata.source_digest,
            vault_staging_id: metadata.vault_staging_id,
            target_vault_record: metadata.target_vault_record,
            dataset_generation: metadata.dataset_generation,
            metadata_auth_tag: encode_hex(&intent.metadata_auth_tag),
        }
    }

    fn into_intent(self) -> Result<EncryptedSourceCaptureIntentV1, SourceCaptureError> {
        if self.version != 1 {
            return Err(SourceCaptureError::UnknownVersion);
        }
        Ok(EncryptedSourceCaptureIntentV1 {
            subject: ObjectReference::new(self.reference_kind, decode_hex(&self.subject_cid)?),
            canonical_digest: decode_hex(&self.canonical_digest)?,
            source_digest: decode_hex(&self.source_digest)?,
            vault_staging_id: VaultStagingId(decode_hex(&self.vault_staging_id)?),
            target_vault_record: ObjectCid::from_bytes(decode_hex(&self.target_vault_record)?),
            dataset_generation: DatasetGenerationId(decode_hex(&self.dataset_generation)?),
            metadata_auth_tag: decode_hex(&self.metadata_auth_tag)?,
        })
    }
}

fn intent_path(root: &Path, id: VaultStagingId, extension: &str) -> PathBuf {
    root.join(format!("{}.{}", encode_hex(&id.0), extension))
}

fn sync_directory(directory: &Path) -> std::io::Result<()> {
    match std::fs::File::open(directory) {
        Ok(file) => file.sync_all(),
        Err(error) if cfg!(windows) && error.kind() == std::io::ErrorKind::PermissionDenied => {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

fn encode_hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex(value: &str) -> Result<[u8; 32], SourceCaptureError> {
    if value.len() != 64 {
        return Err(SourceCaptureError::Malformed);
    }
    let mut decoded = [0; 32];
    for (index, byte) in decoded.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| SourceCaptureError::Malformed)?;
    }
    Ok(decoded)
}

#[derive(Debug, Error)]
pub enum SourceCaptureError {
    #[error("source capture I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("source capture JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("source capture entropy unavailable")]
    EntropyUnavailable,
    #[error("source capture ID collision")]
    IdCollision,
    #[error("source capture pending-intent limit reached")]
    PendingLimit,
    #[error("source capture metadata authentication failed")]
    AuthenticationFailed,
    #[error("source capture binding mismatch")]
    BindingMismatch,
    #[error("unknown source capture version")]
    UnknownVersion,
    #[error("malformed source capture intent")]
    Malformed,
    #[error("source record failed: {0}")]
    Source(String),
    #[error("Vault operation failed: {0}")]
    Vault(String),
    #[error("dataset path failed: {0}")]
    Path(String),
}
