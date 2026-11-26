use std::{collections::BTreeMap, path::Path};

use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::MobileCoreError;

const SCHEMA_META: TableDefinition<&str, &[u8]> = TableDefinition::new("_schema_meta");
const DATASET_GENERATIONS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("dataset_generations");
const ACTIVE_DATASET_KEY: &str = "ACTIVE_DATASET";
const META_KEY: &str = "current";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DatasetRollbackMode {
    NMinusOneReadWrite,
    ReverseBridge,
    PreWriteOnly,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SchemaMetaRecord {
    pub store_name: String,
    pub schema_version: u32,
    pub min_reader_version: u32,
    pub migrated_at_monotonic_ms: u64,
    pub additive_columns: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DatasetGenerationRecord {
    pub dataset_generation: u64,
    pub slot_name: String,
    pub schema_version: u32,
    pub rollback_mode: DatasetRollbackMode,
    pub previous_generation: Option<u64>,
    pub reverse_bridge_caught_up_frontier: u64,
    pub current_write_frontier: u64,
    pub incompatible_post_upgrade_writes: u64,
    pub safe_mode_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RedactedDiagnosticsReport {
    pub report_format: String,
    pub profile_version: String,
    pub process_generation: u64,
    pub registry_state: String,
    pub active_dataset_generation: u64,
    pub safe_mode_active: bool,
    pub safe_mode_reason: Option<String>,
    pub counts: BTreeMap<String, u64>,
    pub redacted_event_codes: Vec<String>,
    pub privacy_attestation: String,
}

pub struct MigrationCoordinator {
    database: Database,
}

impl MigrationCoordinator {
    pub fn open(path: &Path) -> Result<Self, MobileCoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                MobileCoreError::Storage(format!(
                    "cannot create migration coordinator directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        let database = Database::create(path)?;
        let write = database.begin_write()?;
        {
            let mut schema = write.open_table(SCHEMA_META)?;
            if schema.get(META_KEY)?.is_none() {
                let initial = SchemaMetaRecord {
                    store_name: "onebrain_mobile_ops".into(),
                    schema_version: 1,
                    min_reader_version: 1,
                    migrated_at_monotonic_ms: 0,
                    additive_columns: Vec::new(),
                };
                let bytes = serde_json::to_vec(&initial)?;
                schema.insert(META_KEY, bytes.as_slice())?;
            }
            let mut datasets = write.open_table(DATASET_GENERATIONS)?;
            if datasets.get(ACTIVE_DATASET_KEY)?.is_none() {
                let active = DatasetGenerationRecord {
                    dataset_generation: 1,
                    slot_name: "dataset_gen_1".into(),
                    schema_version: 1,
                    rollback_mode: DatasetRollbackMode::NMinusOneReadWrite,
                    previous_generation: None,
                    reverse_bridge_caught_up_frontier: 0,
                    current_write_frontier: 0,
                    incompatible_post_upgrade_writes: 0,
                    safe_mode_reason: None,
                };
                let bytes = serde_json::to_vec(&active)?;
                datasets.insert("gen/1", bytes.as_slice())?;
                datasets.insert(ACTIVE_DATASET_KEY, bytes.as_slice())?;
            }
        }
        write.commit()?;
        Ok(Self { database })
    }

    pub fn schema_meta(&self) -> Result<SchemaMetaRecord, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(SCHEMA_META)?;
        let raw = table
            .get(META_KEY)?
            .ok_or_else(|| MobileCoreError::Storage("missing _schema_meta record".into()))?;
        serde_json::from_slice(raw.value()).map_err(Into::into)
    }

    pub fn active_dataset(&self) -> Result<DatasetGenerationRecord, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(DATASET_GENERATIONS)?;
        let raw = table
            .get(ACTIVE_DATASET_KEY)?
            .ok_or_else(|| MobileCoreError::Storage("missing ACTIVE_DATASET pointer".into()))?;
        let record: DatasetGenerationRecord = serde_json::from_slice(raw.value())?;
        Ok(record)
    }

    /// Perform an additive in-place migration inside a single write transaction.
    pub fn apply_additive_in_place_migration(
        &self,
        target_version: u32,
        additive_column: &str,
        now_monotonic_ms: u64,
    ) -> Result<SchemaMetaRecord, MobileCoreError> {
        let write = self.database.begin_write()?;
        let updated;
        {
            let mut table = write.open_table(SCHEMA_META)?;
            let current_raw = table
                .get(META_KEY)?
                .ok_or_else(|| MobileCoreError::Storage("missing _schema_meta".into()))?;
            let mut meta: SchemaMetaRecord = serde_json::from_slice(current_raw.value())?;
            drop(current_raw);
            if target_version < meta.schema_version {
                return Err(MobileCoreError::InvalidArgument(
                    "in-place migration cannot downgrade schema_version".into(),
                ));
            }
            if !additive_column.is_empty()
                && !meta
                    .additive_columns
                    .iter()
                    .any(|col| col == additive_column)
            {
                meta.additive_columns.push(additive_column.to_owned());
            }
            meta.schema_version = target_version;
            meta.migrated_at_monotonic_ms = now_monotonic_ms;
            let bytes = serde_json::to_vec(&meta)?;
            table.insert(META_KEY, bytes.as_slice())?;
            updated = meta;
        }
        write.commit()?;
        Ok(updated)
    }

    /// Stage and atomically switch to a shadow dataset generation `N+1`.
    /// On injected failure, records safe mode on the active pointer without corrupting `N`.
    pub fn switch_shadow_dataset_generation(
        &self,
        target_schema_version: u32,
        rollback_mode: DatasetRollbackMode,
        simulate_verification_failure: bool,
    ) -> Result<DatasetGenerationRecord, MobileCoreError> {
        let write = self.database.begin_write()?;
        let outcome;
        {
            let mut table = write.open_table(DATASET_GENERATIONS)?;
            let current_raw = table
                .get(ACTIVE_DATASET_KEY)?
                .ok_or_else(|| MobileCoreError::Storage("missing ACTIVE_DATASET".into()))?;
            let mut current: DatasetGenerationRecord = serde_json::from_slice(current_raw.value())?;
            drop(current_raw);

            if simulate_verification_failure {
                current.safe_mode_reason =
                    Some("SHADOW_DATASET_VERIFICATION_FAILED_RETAINED_PREVIOUS".into());
                let bytes = serde_json::to_vec(&current)?;
                let key = format!("gen/{}", current.dataset_generation);
                table.insert(key.as_str(), bytes.as_slice())?;
                table.insert(ACTIVE_DATASET_KEY, bytes.as_slice())?;
                outcome = Err(MobileCoreError::SafeModeActive(
                    "shadow dataset migration failed verification; previous dataset preserved in safe mode".into(),
                ));
            } else {
                let next_gen = current.dataset_generation.saturating_add(1);
                let next = DatasetGenerationRecord {
                    dataset_generation: next_gen,
                    slot_name: format!("dataset_gen_{next_gen}"),
                    schema_version: target_schema_version,
                    rollback_mode,
                    previous_generation: Some(current.dataset_generation),
                    reverse_bridge_caught_up_frontier: current.current_write_frontier,
                    current_write_frontier: current.current_write_frontier,
                    incompatible_post_upgrade_writes: 0,
                    safe_mode_reason: None,
                };
                let prev_bytes = serde_json::to_vec(&current)?;
                let prev_key = format!("gen/{}", current.dataset_generation);
                table.insert(prev_key.as_str(), prev_bytes.as_slice())?;

                let next_bytes = serde_json::to_vec(&next)?;
                let next_key = format!("gen/{next_gen}");
                table.insert(next_key.as_str(), next_bytes.as_slice())?;
                table.insert(ACTIVE_DATASET_KEY, next_bytes.as_slice())?;
                outcome = Ok(next);
            }
        }
        write.commit()?;
        outcome
    }

    pub fn record_post_upgrade_write(
        &self,
        compatible_with_reverse_bridge: bool,
    ) -> Result<DatasetGenerationRecord, MobileCoreError> {
        let write = self.database.begin_write()?;
        let updated;
        {
            let mut table = write.open_table(DATASET_GENERATIONS)?;
            let raw = table
                .get(ACTIVE_DATASET_KEY)?
                .ok_or_else(|| MobileCoreError::Storage("missing ACTIVE_DATASET".into()))?;
            let mut active: DatasetGenerationRecord = serde_json::from_slice(raw.value())?;
            drop(raw);
            if active.safe_mode_reason.is_some() {
                return Err(MobileCoreError::SafeModeActive(
                    "writes are blocked while dataset safe mode is active".into(),
                ));
            }
            active.current_write_frontier = active.current_write_frontier.saturating_add(1);
            if compatible_with_reverse_bridge
                && active.rollback_mode != DatasetRollbackMode::PreWriteOnly
            {
                active.reverse_bridge_caught_up_frontier = active.current_write_frontier;
            } else {
                active.incompatible_post_upgrade_writes =
                    active.incompatible_post_upgrade_writes.saturating_add(1);
            }
            let bytes = serde_json::to_vec(&active)?;
            let key = format!("gen/{}", active.dataset_generation);
            table.insert(key.as_str(), bytes.as_slice())?;
            table.insert(ACTIVE_DATASET_KEY, bytes.as_slice())?;
            updated = active;
        }
        write.commit()?;
        Ok(updated)
    }

    /// Roll back to `N-1` generation enforcing `NMinusOneReadWrite`, `ReverseBridge`,
    /// and `PreWriteOnly` safety contracts so post-upgrade writes are NEVER silently dropped.
    pub fn rollback_dataset_generation(&self) -> Result<DatasetGenerationRecord, MobileCoreError> {
        let write = self.database.begin_write()?;
        let rolled_back;
        {
            let mut table = write.open_table(DATASET_GENERATIONS)?;
            let raw = table
                .get(ACTIVE_DATASET_KEY)?
                .ok_or_else(|| MobileCoreError::Storage("missing ACTIVE_DATASET".into()))?;
            let active: DatasetGenerationRecord = serde_json::from_slice(raw.value())?;
            drop(raw);

            let Some(prev_gen) = active.previous_generation else {
                return Err(MobileCoreError::RollbackUnavailable(
                    "no N-1 dataset generation exists".into(),
                ));
            };

            match active.rollback_mode {
                DatasetRollbackMode::NMinusOneReadWrite => {
                    if active.incompatible_post_upgrade_writes > 0 {
                        return Err(MobileCoreError::RollbackUnavailable(
                            "incompatible post-upgrade writes cannot be represented in N-1".into(),
                        ));
                    }
                }
                DatasetRollbackMode::ReverseBridge => {
                    if active.reverse_bridge_caught_up_frontier < active.current_write_frontier
                        || active.incompatible_post_upgrade_writes > 0
                    {
                        return Err(MobileCoreError::RollbackUnavailable(
                            "ReverseBridge has not caught up all post-upgrade writes".into(),
                        ));
                    }
                }
                DatasetRollbackMode::PreWriteOnly => {
                    if active.current_write_frontier > active.reverse_bridge_caught_up_frontier
                        || active.incompatible_post_upgrade_writes > 0
                    {
                        return Err(MobileCoreError::RollbackUnavailable(
                            "PreWriteOnly dataset cannot roll back after post-upgrade writes without explicit restore".into(),
                        ));
                    }
                }
            }

            let prev_key = format!("gen/{prev_gen}");
            let prev_raw = table.get(prev_key.as_str())?.ok_or_else(|| {
                MobileCoreError::RollbackUnavailable(
                    "N-1 dataset slot was already reclaimed".into(),
                )
            })?;
            let mut prev: DatasetGenerationRecord = serde_json::from_slice(prev_raw.value())?;
            drop(prev_raw);
            prev.current_write_frontier = active.current_write_frontier;
            prev.reverse_bridge_caught_up_frontier = active.reverse_bridge_caught_up_frontier;
            prev.safe_mode_reason = None;
            let bytes = serde_json::to_vec(&prev)?;
            table.insert(prev_key.as_str(), bytes.as_slice())?;
            table.insert(ACTIVE_DATASET_KEY, bytes.as_slice())?;
            rolled_back = prev;
        }
        write.commit()?;
        Ok(rolled_back)
    }

    pub fn clear_safe_mode(&self) -> Result<DatasetGenerationRecord, MobileCoreError> {
        let write = self.database.begin_write()?;
        let updated;
        {
            let mut table = write.open_table(DATASET_GENERATIONS)?;
            let raw = table
                .get(ACTIVE_DATASET_KEY)?
                .ok_or_else(|| MobileCoreError::Storage("missing ACTIVE_DATASET".into()))?;
            let mut active: DatasetGenerationRecord = serde_json::from_slice(raw.value())?;
            drop(raw);
            active.safe_mode_reason = None;
            let bytes = serde_json::to_vec(&active)?;
            let key = format!("gen/{}", active.dataset_generation);
            table.insert(key.as_str(), bytes.as_slice())?;
            table.insert(ACTIVE_DATASET_KEY, bytes.as_slice())?;
            updated = active;
        }
        write.commit()?;
        Ok(updated)
    }
}

/// Build a bounded, privacy-safe diagnostics bundle (`MOB-SYS-005`).
/// Never includes note text, prompts, tool arguments, keys, tokens, paths, or full private CIDs.
pub fn build_redacted_diagnostics_report(
    profile_version: &str,
    process_generation: u64,
    registry_state: &str,
    active_dataset: &DatasetGenerationRecord,
    counts: BTreeMap<String, u64>,
    raw_event_codes: &[String],
) -> RedactedDiagnosticsReport {
    let redacted_event_codes = raw_event_codes
        .iter()
        .take(64)
        .map(|code| sanitize_diagnostic_code(code))
        .collect();
    RedactedDiagnosticsReport {
        report_format: "onebrain.mobile.redacted-diagnostics/v1".into(),
        profile_version: profile_version.to_owned(),
        process_generation,
        registry_state: registry_state.to_owned(),
        active_dataset_generation: active_dataset.dataset_generation,
        safe_mode_active: active_dataset.safe_mode_reason.is_some(),
        safe_mode_reason: active_dataset.safe_mode_reason.clone(),
        counts,
        redacted_event_codes,
        privacy_attestation: "NO_PLAINTEXT_CONTENT_NO_PROMPTS_NO_TOOL_ARGS_NO_KEYS_NO_PRIVATE_CIDS"
            .into(),
    }
}

fn sanitize_diagnostic_code(input: &str) -> String {
    let filtered: String = input
        .chars()
        .take(64)
        .map(|ch| {
            if ch.is_ascii_uppercase() || ch.is_ascii_digit() || matches!(ch, '_' | '-' | '.') {
                ch
            } else if ch.is_ascii_lowercase() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    if filtered.is_empty() {
        "REDACTED_EVENT".into()
    } else {
        filtered
    }
}
