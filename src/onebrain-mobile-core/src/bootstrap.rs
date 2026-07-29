use std::{
    fs,
    path::{Path, PathBuf},
};

use redb::{Database, ReadableTable, TableDefinition};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{MobileCoreError, ResourceBudgets};

const PROCESS_GENERATIONS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("process_generations");
const REGISTRY_OPERATIONS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("registry_operations");
const REGISTRY_CHUNKS: TableDefinition<&str, &[u8]> = TableDefinition::new("registry_chunks");
const TRANSFER_LANDING: TableDefinition<&str, &[u8]> = TableDefinition::new("transfer_landing");
const BOOTSTRAP_OPERATION_IDS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("bootstrap_op_ids");
const CURRENT_PROCESS_KEY: &str = "current";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessLifecycle {
    Started,
    Quiesced,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProcessGenerationRecord {
    pub generation: u64,
    pub lifecycle: ProcessLifecycle,
    pub observed_at_monotonic_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessStart {
    pub generation: u64,
    pub recovered_unclean_start: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RegistryOperationRecord {
    pub operation_id: String,
    pub release_id: String,
    pub state: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RegistryChunkRecord {
    pub operation_id: String,
    pub chunk_index: u32,
    pub expected_hash: String,
    pub expected_length: u64,
    pub state: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransferLandingRecord {
    pub transfer_nonce: String,
    pub operation_id: String,
    pub release_id: String,
    pub artifact_role: String,
    pub chunk_index: u32,
    pub expected_hash: String,
    pub expected_length: u64,
    pub os_transfer_id: Option<String>,
    pub receiving_process_generation: Option<u64>,
    pub app_assigned_callback_sequence: Option<u64>,
    pub landed: bool,
}

pub struct BootstrapStore {
    database: Database,
    path: PathBuf,
}

impl BootstrapStore {
    pub fn open(path: &Path) -> Result<Self, MobileCoreError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                MobileCoreError::Storage(format!(
                    "cannot create bootstrap directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        let database = Database::create(path)?;
        let write = database.begin_write()?;
        {
            let _ = write.open_table(PROCESS_GENERATIONS)?;
            let _ = write.open_table(REGISTRY_OPERATIONS)?;
            let _ = write.open_table(REGISTRY_CHUNKS)?;
            let _ = write.open_table(TRANSFER_LANDING)?;
            let _ = write.open_table(BOOTSTRAP_OPERATION_IDS)?;
        }
        write.commit()?;
        Ok(Self {
            database,
            path: path.to_owned(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn start_process(
        &self,
        observed_at_monotonic_ms: u64,
    ) -> Result<ProcessStart, MobileCoreError> {
        let write = self.database.begin_write()?;
        let (generation, recovered_unclean_start);
        {
            let mut table = write.open_table(PROCESS_GENERATIONS)?;
            let previous = table
                .get(CURRENT_PROCESS_KEY)?
                .map(|value| decode::<ProcessGenerationRecord>(value.value()))
                .transpose()?;
            generation = previous
                .as_ref()
                .map_or(1, |record| record.generation.saturating_add(1));
            if generation == u64::MAX
                && previous
                    .as_ref()
                    .is_some_and(|record| record.generation == u64::MAX)
            {
                return Err(MobileCoreError::Storage(
                    "process generation exhausted".into(),
                ));
            }
            recovered_unclean_start = previous
                .as_ref()
                .is_some_and(|record| record.lifecycle == ProcessLifecycle::Started);
            let record = ProcessGenerationRecord {
                generation,
                lifecycle: ProcessLifecycle::Started,
                observed_at_monotonic_ms,
            };
            let bytes = encode(&record)?;
            table.insert(CURRENT_PROCESS_KEY, bytes.as_slice())?;
        }
        write.commit()?;
        Ok(ProcessStart {
            generation,
            recovered_unclean_start,
        })
    }

    pub fn current_process(&self) -> Result<Option<ProcessGenerationRecord>, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(PROCESS_GENERATIONS)?;
        table
            .get(CURRENT_PROCESS_KEY)?
            .map(|value| decode(value.value()))
            .transpose()
    }

    pub fn quiesce_process(
        &self,
        generation: u64,
        observed_at_monotonic_ms: u64,
    ) -> Result<(), MobileCoreError> {
        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(PROCESS_GENERATIONS)?;
            let current = required_current_process(&table)?;
            ensure_generation(current.generation, generation)?;
            let record = ProcessGenerationRecord {
                generation,
                lifecycle: ProcessLifecycle::Quiesced,
                observed_at_monotonic_ms,
            };
            let bytes = encode(&record)?;
            table.insert(CURRENT_PROCESS_KEY, bytes.as_slice())?;
        }
        write.commit()?;
        Ok(())
    }

    pub fn upsert_registry_operation(
        &self,
        record: &RegistryOperationRecord,
        budgets: &ResourceBudgets,
    ) -> Result<(), MobileCoreError> {
        require_bounded(
            "operation_id",
            &record.operation_id,
            budgets.max_operation_id_bytes,
        )?;
        let bytes = encode(record)?;
        let write = self.database.begin_write()?;
        {
            let mut op_ids = write.open_table(BOOTSTRAP_OPERATION_IDS)?;
            op_ids.insert(record.operation_id.as_str(), b"registry".as_slice())?;
            let mut table = write.open_table(REGISTRY_OPERATIONS)?;
            table.insert(record.operation_id.as_str(), bytes.as_slice())?;
        }
        write.commit()?;
        Ok(())
    }

    pub fn registry_operation(
        &self,
        operation_id: &str,
    ) -> Result<Option<RegistryOperationRecord>, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(REGISTRY_OPERATIONS)?;
        table
            .get(operation_id)?
            .map(|value| decode(value.value()))
            .transpose()
    }

    pub fn upsert_registry_chunk(
        &self,
        record: &RegistryChunkRecord,
        budgets: &ResourceBudgets,
    ) -> Result<(), MobileCoreError> {
        require_bounded(
            "operation_id",
            &record.operation_id,
            budgets.max_operation_id_bytes,
        )?;
        validate_hash(&record.expected_hash)?;
        let key = chunk_key(&record.operation_id, record.chunk_index);
        let bytes = encode(record)?;
        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(REGISTRY_CHUNKS)?;
            table.insert(key.as_str(), bytes.as_slice())?;
        }
        write.commit()?;
        Ok(())
    }

    pub fn registry_chunk(
        &self,
        operation_id: &str,
        chunk_index: u32,
    ) -> Result<Option<RegistryChunkRecord>, MobileCoreError> {
        let key = chunk_key(operation_id, chunk_index);
        let read = self.database.begin_read()?;
        let table = read.open_table(REGISTRY_CHUNKS)?;
        table
            .get(key.as_str())?
            .map(|value| decode(value.value()))
            .transpose()
    }

    pub fn prepare_transfer(
        &self,
        record: &TransferLandingRecord,
        budgets: &ResourceBudgets,
    ) -> Result<(), MobileCoreError> {
        require_bounded(
            "transfer_nonce",
            &record.transfer_nonce,
            budgets.max_transfer_nonce_bytes,
        )?;
        require_bounded(
            "operation_id",
            &record.operation_id,
            budgets.max_operation_id_bytes,
        )?;
        require_bounded(
            "artifact_role",
            &record.artifact_role,
            budgets.max_artifact_role_bytes,
        )?;
        if let Some(os_transfer_id) = &record.os_transfer_id {
            require_bounded(
                "os_transfer_id",
                os_transfer_id,
                budgets.max_os_transfer_id_bytes,
            )?;
        }
        validate_hash(&record.expected_hash)?;
        let bytes = encode(record)?;
        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(TRANSFER_LANDING)?;
            let current = table
                .get(record.transfer_nonce.as_str())?
                .map(|existing| decode::<TransferLandingRecord>(existing.value()))
                .transpose()?;
            if let Some(current) = current {
                if stable_transfer_identity(&current) != stable_transfer_identity(record) {
                    return Err(MobileCoreError::InvalidArgument(
                        "transfer nonce is already bound to another stable identity".into(),
                    ));
                }
            } else {
                table.insert(record.transfer_nonce.as_str(), bytes.as_slice())?;
            }
        }
        write.commit()?;
        Ok(())
    }

    pub fn bind_os_transfer(
        &self,
        transfer_nonce: &str,
        os_transfer_id: &str,
        budgets: &ResourceBudgets,
    ) -> Result<(), MobileCoreError> {
        require_bounded(
            "os_transfer_id",
            os_transfer_id,
            budgets.max_os_transfer_id_bytes,
        )?;
        self.update_transfer(transfer_nonce, |record| {
            record.os_transfer_id = Some(os_transfer_id.to_owned());
            Ok(())
        })
    }

    pub fn claim_transfer_callback(
        &self,
        transfer_nonce: &str,
        receiving_generation: u64,
        callback_sequence: u64,
    ) -> Result<TransferLandingRecord, MobileCoreError> {
        let write = self.database.begin_write()?;
        let updated;
        {
            let process_table = write.open_table(PROCESS_GENERATIONS)?;
            let current = required_current_process(&process_table)?;
            ensure_generation(current.generation, receiving_generation)?;
            drop(process_table);

            let mut transfers = write.open_table(TRANSFER_LANDING)?;
            let existing = transfers
                .get(transfer_nonce)?
                .ok_or_else(|| MobileCoreError::UnknownTransfer(transfer_nonce.to_owned()))?;
            let mut record: TransferLandingRecord = decode(existing.value())?;
            drop(existing);
            if record.receiving_process_generation == Some(receiving_generation) {
                if let Some(current_sequence) = record.app_assigned_callback_sequence {
                    if callback_sequence <= current_sequence {
                        return Err(MobileCoreError::StaleCallbackSequence {
                            received: callback_sequence,
                            current: current_sequence,
                        });
                    }
                }
            }
            record.receiving_process_generation = Some(receiving_generation);
            record.app_assigned_callback_sequence = Some(callback_sequence);
            let bytes = encode(&record)?;
            transfers.insert(transfer_nonce, bytes.as_slice())?;
            updated = record;
        }
        write.commit()?;
        Ok(updated)
    }

    pub fn mark_transfer_landed(
        &self,
        transfer_nonce: &str,
        receiving_generation: u64,
        callback_sequence: u64,
    ) -> Result<TransferLandingRecord, MobileCoreError> {
        let claimed =
            self.claim_transfer_callback(transfer_nonce, receiving_generation, callback_sequence)?;
        self.update_transfer(transfer_nonce, |record| {
            if record.receiving_process_generation != Some(receiving_generation)
                || record.app_assigned_callback_sequence != Some(callback_sequence)
            {
                return Err(MobileCoreError::StaleCallbackSequence {
                    received: callback_sequence,
                    current: record.app_assigned_callback_sequence.unwrap_or(0),
                });
            }
            record.landed = true;
            Ok(())
        })?;
        Ok(TransferLandingRecord {
            landed: true,
            ..claimed
        })
    }

    pub fn transfer(
        &self,
        transfer_nonce: &str,
    ) -> Result<Option<TransferLandingRecord>, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(TRANSFER_LANDING)?;
        table
            .get(transfer_nonce)?
            .map(|value| decode(value.value()))
            .transpose()
    }

    fn update_transfer(
        &self,
        transfer_nonce: &str,
        update: impl FnOnce(&mut TransferLandingRecord) -> Result<(), MobileCoreError>,
    ) -> Result<(), MobileCoreError> {
        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(TRANSFER_LANDING)?;
            let existing = table
                .get(transfer_nonce)?
                .ok_or_else(|| MobileCoreError::UnknownTransfer(transfer_nonce.to_owned()))?;
            let mut record: TransferLandingRecord = decode(existing.value())?;
            drop(existing);
            update(&mut record)?;
            let bytes = encode(&record)?;
            table.insert(transfer_nonce, bytes.as_slice())?;
        }
        write.commit()?;
        Ok(())
    }
}

fn required_current_process(
    table: &impl ReadableTable<&'static str, &'static [u8]>,
) -> Result<ProcessGenerationRecord, MobileCoreError> {
    table
        .get(CURRENT_PROCESS_KEY)?
        .map(|value| decode(value.value()))
        .transpose()?
        .ok_or_else(|| MobileCoreError::Storage("process generation is not initialized".into()))
}

fn ensure_generation(current: u64, received: u64) -> Result<(), MobileCoreError> {
    if current == received {
        Ok(())
    } else {
        Err(MobileCoreError::StaleGeneration { received, current })
    }
}

fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, MobileCoreError> {
    serde_json::to_vec(value).map_err(Into::into)
}

fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, MobileCoreError> {
    serde_json::from_slice(bytes).map_err(Into::into)
}

fn chunk_key(operation_id: &str, chunk_index: u32) -> String {
    format!("{operation_id}/{chunk_index:010}")
}

fn require_bounded(name: &str, value: &str, max: usize) -> Result<(), MobileCoreError> {
    if value.is_empty() || value.len() > max {
        return Err(MobileCoreError::InvalidArgument(format!(
            "{name} must contain between 1 and {max} UTF-8 bytes"
        )));
    }
    Ok(())
}

fn validate_hash(value: &str) -> Result<(), MobileCoreError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(MobileCoreError::InvalidArgument(
            "expected_hash must be a 32-byte lowercase or uppercase hex digest".into(),
        ));
    }
    Ok(())
}

fn stable_transfer_identity(
    record: &TransferLandingRecord,
) -> (&str, &str, &str, &str, u32, &str, u64) {
    (
        record.transfer_nonce.as_str(),
        record.operation_id.as_str(),
        record.release_id.as_str(),
        record.artifact_role.as_str(),
        record.chunk_index,
        record.expected_hash.as_str(),
        record.expected_length,
    )
}
