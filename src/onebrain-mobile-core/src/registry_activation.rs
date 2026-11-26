use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use ciborium::value::Value;
use ed25519_dalek::{Signer, SigningKey};
use ku_core::{
    concept_registry::{ConceptCategory, ConceptRegistry, ResolveResult, ResolvedConcept},
    concept_registry_manifest::{ConceptRegistryIndexManifest, ConceptRegistryManifest},
    indexed_concept_registry::{
        IndexedConceptRegistry, CCID_INDEX_MAGIC, LABEL_INDEX_MAGIC, REGISTRY_INDEX_HEADER_SIZE,
        REGISTRY_INDEX_RECORD_SIZE, REGISTRY_INDEX_VERSION,
    },
};
use serde::{Deserialize, Serialize};

use crate::{
    registry_admission::REGISTRY_CHUNK_DOMAIN, BootstrapStore, MobileCoreError,
    RegistryOperationState, RegistryReleaseState,
};

pub const REGISTRY_HEALTH_SUITE_V1: &str = "REGISTRY_HEALTH_V1";
const HEALTH_SAMPLE_DOMAIN: &[u8] = b"onebrain:registry-health-sample:1\0";
const STREAM_BUFFER_BYTES: usize = 64 * 1024;
const DEFAULT_INDEX_CACHE_CAPACITY: usize = 512;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DerivedRegistryReadiness {
    BootstrapOnly,
    Provisioning { phase_code: String },
    ReadyOffline,
    ReadyOfflineUpdateHealthPending { candidate_release_id: String },
    RegistryDegraded { reason_code: String },
}

impl DerivedRegistryReadiness {
    pub const fn as_snapshot_str(&self) -> &'static str {
        match self {
            Self::BootstrapOnly => "BootstrapOnly",
            Self::Provisioning { .. } => "Provisioning",
            Self::ReadyOffline => "ReadyOffline",
            Self::ReadyOfflineUpdateHealthPending { .. } => "ReadyOffline(UpdateHealthPending)",
            Self::RegistryDegraded { .. } => "RegistryDegraded",
        }
    }

    pub const fn is_ready_for_canonical_encode(&self) -> bool {
        matches!(
            self,
            Self::ReadyOffline | Self::ReadyOfflineUpdateHealthPending { .. }
        )
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationReceiptStatus {
    HealthPending,
    Passed,
    CompensatedFailed,
    RolledBack,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RegistryActiveStateRecord {
    pub active_release_id: Option<String>,
    pub active_manifest_digest: Option<String>,
    pub active_trust_profile_digest: Option<String>,
    pub active_release_sequence: Option<u64>,
    pub previous_eligible_release_id: Option<String>,
    pub pointer_epoch: u64,
    pub health_pending_suite: Option<String>,
    pub last_failure_code: Option<String>,
    pub rollback_retain_until_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RegistryActivationReceiptRecord {
    pub operation_id: String,
    pub release_id: String,
    pub manifest_digest: String,
    pub pointer_epoch: u64,
    pub fallback_release_id: Option<String>,
    pub whole_obr_blake3: String,
    pub whole_labels_blake3: String,
    pub whole_ccids_blake3: String,
    pub health_suite_id: String,
    pub sampled_ordinals_checked: u32,
    pub status: ActivationReceiptStatus,
    pub failure_reason: Option<String>,
}

pub struct RegistryGeneration {
    pub release_id: String,
    pub manifest_digest: String,
    pub release_sequence: u64,
    pub slot_dir: PathBuf,
    pub obr_path: PathBuf,
    pub reader: IndexedConceptRegistry,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RegistryConceptView {
    pub ccid_hex: String,
    pub qid: u32,
    pub category: String,
    pub canonical_name: String,
    pub labels: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RegistryGarbageCollectionReport {
    pub reclaimed_release_ids: Vec<String>,
    pub retained_release_ids: Vec<String>,
    pub reclaimed_bytes: u64,
}

/// Ordered 6-step total readiness derivation (`WIP_MOBILE_APP_TECHNICAL_ARCHITECTURE_V1.md` §3.3).
pub fn derive_registry_readiness(
    store: &BootstrapStore,
    live_generation: Option<&Arc<RegistryGeneration>>,
) -> DerivedRegistryReadiness {
    let active_state = match store.registry_active_state() {
        Ok(state) => state,
        Err(_) => {
            return DerivedRegistryReadiness::RegistryDegraded {
                reason_code: "CORRUPT_ACTIVE_STATE_TABLE".into(),
            }
        }
    };

    if let Some(reason) = &active_state.last_failure_code {
        if active_state.active_release_id.is_none() && reason == "ROLLBACK_TARGET_UNAVAILABLE" {
            return DerivedRegistryReadiness::RegistryDegraded {
                reason_code: reason.clone(),
            };
        }
    }

    // Step 2 & 3: Active release pointer present
    if let Some(active_id) = &active_state.active_release_id {
        let catalog = match store.registry_release_catalog(active_id) {
            Ok(Some(record)) => record,
            Ok(None) => {
                return DerivedRegistryReadiness::RegistryDegraded {
                    reason_code: "ACTIVE_RELEASE_MISSING_FROM_CATALOG".into(),
                }
            }
            Err(_) => {
                return DerivedRegistryReadiness::RegistryDegraded {
                    reason_code: "CORRUPT_RELEASE_CATALOG".into(),
                }
            }
        };
        if catalog.state == RegistryReleaseState::Revoked {
            return DerivedRegistryReadiness::RegistryDegraded {
                reason_code: "ACTIVE_RELEASE_REVOKED".into(),
            };
        }
        if active_state.health_pending_suite.is_some() {
            if let Some(prev_id) = &active_state.previous_eligible_release_id {
                if store
                    .registry_release_catalog(prev_id)
                    .ok()
                    .flatten()
                    .is_some_and(|prev| prev.state == RegistryReleaseState::Healthy)
                {
                    return DerivedRegistryReadiness::ReadyOfflineUpdateHealthPending {
                        candidate_release_id: active_id.clone(),
                    };
                }
            }
            return DerivedRegistryReadiness::Provisioning {
                phase_code: "HealthPending".into(),
            };
        }
        let slot_obr = store
            .registry_release_slot_dir(active_id)
            .join("concepts.obr");
        if catalog.state == RegistryReleaseState::Healthy
            && slot_obr.is_file()
            && live_generation.is_some_and(|gen| gen.release_id == *active_id)
        {
            return DerivedRegistryReadiness::ReadyOffline;
        }
        return DerivedRegistryReadiness::RegistryDegraded {
            reason_code: "ACTIVE_RELEASE_READER_OR_FILES_UNAVAILABLE".into(),
        };
    }

    // Step 4: No active release, check if a non-terminal provisioning operation is active
    match store.active_non_terminal_registry_operation_phase() {
        Ok(Some(phase)) => DerivedRegistryReadiness::Provisioning { phase_code: phase },
        Ok(None) => DerivedRegistryReadiness::BootstrapOnly,
        Err(_) => DerivedRegistryReadiness::RegistryDegraded {
            reason_code: "CORRUPT_OPERATION_LEDGER".into(),
        },
    }
}

/// Complete `MOB-05C` streaming activation, A/B slot swap, deterministic `REGISTRY_HEALTH_V1`,
/// and automatic compensation to $N-1$ (or `BootstrapOnly` on clean install).
pub fn activate_verified_registry_release(
    store: &BootstrapStore,
    operation_id: &str,
    transfer_nonce: &str,
    now_monotonic_ms: u64,
    corrupt_staged_sidecar_before_health_for_test: bool,
) -> Result<
    (
        RegistryActivationReceiptRecord,
        Option<Arc<RegistryGeneration>>,
    ),
    MobileCoreError,
> {
    let operation = store
        .registry_operation(operation_id)?
        .ok_or_else(|| MobileCoreError::RegistryAdmission("unknown operation_id".into()))?;
    if !matches!(
        operation.state,
        RegistryOperationState::BytesComplete
            | RegistryOperationState::WholeArtifactsVerified
            | RegistryOperationState::QuerySmokePassed
            | RegistryOperationState::DirectoryCommitted
            | RegistryOperationState::PointerCommitted
            | RegistryOperationState::HealthPending
    ) {
        return Err(MobileCoreError::RegistryAdmission(format!(
            "operation '{}' is in state {:?}, expected BytesComplete for activation",
            operation_id, operation.state
        )));
    }

    let release_id = operation.release_id.clone();
    let catalog = store
        .registry_release_catalog(&release_id)?
        .ok_or_else(|| MobileCoreError::RegistryAdmission("missing release catalog row".into()))?;
    if catalog.state == RegistryReleaseState::Revoked {
        return Err(MobileCoreError::RegistryTrust(
            "revoked Registry release cannot be activated".into(),
        ));
    }

    let staging_dir = store.registry_staging_dir(operation_id);
    if staging_dir.exists() {
        fs::remove_dir_all(&staging_dir)
            .map_err(|error| MobileCoreError::Storage(format!("clean staging dir: {error}")))?;
    }
    fs::create_dir_all(&staging_dir)
        .map_err(|error| MobileCoreError::Storage(format!("create staging dir: {error}")))?;

    let obr_stage = staging_dir.join("concepts.obr");
    let labels_stage = staging_dir.join("concepts.obr.labels.idx");
    let ccids_stage = staging_dir.join("concepts.obr.ccids.idx");

    let (obr_len, obr_hash) = reassemble_role_chunks(store, transfer_nonce, 0, &obr_stage)?;
    let (labels_len, labels_hash) =
        reassemble_role_chunks(store, transfer_nonce, 1, &labels_stage)?;
    let (ccids_len, ccids_hash) = reassemble_role_chunks(store, transfer_nonce, 2, &ccids_stage)?;

    catalog.verify_whole_artifact(0, obr_len, &obr_hash)?;
    catalog.verify_whole_artifact(1, labels_len, &labels_hash)?;
    catalog.verify_whole_artifact(2, ccids_len, &ccids_hash)?;
    store.advance_registry_operation_state(
        operation_id,
        RegistryOperationState::WholeArtifactsVerified,
    )?;

    // Scoped smoke check so Windows mmap/file handles drop cleanly before directory rename!
    {
        let reader = open_indexed_registry_at(
            &obr_stage,
            &obr_hash,
            &labels_hash,
            &ccids_hash,
            labels_len,
            ccids_len,
        )?;
        let _ = reader.metadata();
    }
    store
        .advance_registry_operation_state(operation_id, RegistryOperationState::QuerySmokePassed)?;

    store.revalidate_before_directory_commit(operation_id, &release_id)?;

    let manifest_path = staging_dir.join("release.manifest");
    fs::write(&manifest_path, &catalog.release_envelope_cbor)
        .map_err(|error| MobileCoreError::Storage(format!("write release.manifest: {error}")))?;
    let verification_receipt_json = serde_json::to_vec(&serde_json::json!({
        "operation_id": operation_id,
        "release_id": release_id,
        "manifest_digest": catalog.manifest_digest,
        "obr_blake3": obr_hash,
        "labels_blake3": labels_hash,
        "ccids_blake3": ccids_hash,
        "obr_bytes": obr_len,
        "labels_bytes": labels_len,
        "ccids_bytes": ccids_len,
    }))?;
    let receipt_path = staging_dir.join("artifact_verification.receipt");
    fs::write(&receipt_path, &verification_receipt_json).map_err(|error| {
        MobileCoreError::Storage(format!("write artifact_verification.receipt: {error}"))
    })?;

    if corrupt_staged_sidecar_before_health_for_test {
        // Test-only post-verify fault injection to prove REGISTRY_HEALTH_V1 automatic compensation!
        let mut corrupt = fs::read(&ccids_stage)
            .map_err(|error| MobileCoreError::Storage(format!("read ccids for fault: {error}")))?;
        if corrupt.len() > 20 {
            corrupt[20] ^= 0xff;
            fs::write(&ccids_stage, corrupt).map_err(|error| {
                MobileCoreError::Storage(format!("write faulted ccids: {error}"))
            })?;
        }
    }

    sync_regular_file(&obr_stage)?;
    sync_regular_file(&labels_stage)?;
    sync_regular_file(&ccids_stage)?;
    sync_regular_file(&manifest_path)?;
    sync_regular_file(&receipt_path)?;

    let slot_dir = store.registry_release_slot_dir(&release_id);
    if let Some(parent) = slot_dir.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| MobileCoreError::Storage(format!("create releases root: {error}")))?;
    }
    if slot_dir.exists() {
        fs::remove_dir_all(&slot_dir)
            .map_err(|error| MobileCoreError::Storage(format!("replace stale slot: {error}")))?;
    }
    fs::rename(&staging_dir, &slot_dir).map_err(|error| {
        MobileCoreError::Storage(format!("commit release slot rename: {error}"))
    })?;

    store.mark_release_directory_committed(operation_id, &release_id)?;

    let _pending_receipt = store.commit_registry_pointer_swap(
        operation_id,
        &release_id,
        &obr_hash,
        &labels_hash,
        &ccids_hash,
        REGISTRY_HEALTH_SUITE_V1,
        now_monotonic_ms,
    )?;

    match run_registry_health_v1(store) {
        Ok((generation, sampled_checked)) => {
            let passed =
                store.complete_registry_health_pass(operation_id, &release_id, sampled_checked)?;
            let _ = store.cleanup_verified_landing_chunks(transfer_nonce);
            Ok((passed, Some(Arc::new(generation))))
        }
        Err(health_error) => {
            let reason = health_error.to_string();
            let (compensated_receipt, fallback_release) =
                store.compensate_failed_registry_health(operation_id, &release_id, &reason)?;
            let restored_gen = if let Some(prev_id) = fallback_release {
                match open_verified_generation_from_slot(store, &prev_id) {
                    Ok(gen) => Some(Arc::new(gen)),
                    Err(_) => {
                        store.mark_registry_degraded_no_rollback("FALLBACK_SLOT_OPEN_FAILED")?;
                        None
                    }
                }
            } else {
                None
            };
            Ok((compensated_receipt, restored_gen))
        }
    }
}

/// Reopen the currently active healthy release on runtime startup (recovering
/// any interrupted `HealthPending` pointer swap automatically!).
pub fn recover_or_open_active_registry(
    store: &BootstrapStore,
) -> Result<Option<Arc<RegistryGeneration>>, MobileCoreError> {
    store.cleanup_orphan_staging_directories()?;
    let state = store.registry_active_state()?;
    let Some(active_id) = state.active_release_id.clone() else {
        return Ok(None);
    };
    if state.health_pending_suite.is_some() {
        if let Some(op_id) = store.operation_id_for_release(&active_id)? {
            match run_registry_health_v1(store) {
                Ok((gen, sampled)) => {
                    store.complete_registry_health_pass(&op_id, &active_id, sampled)?;
                    return Ok(Some(Arc::new(gen)));
                }
                Err(error) => {
                    let (_, fallback) = store.compensate_failed_registry_health(
                        &op_id,
                        &active_id,
                        &error.to_string(),
                    )?;
                    if let Some(prev_id) = fallback {
                        return open_verified_generation_from_slot(store, &prev_id)
                            .map(|gen| Some(Arc::new(gen)));
                    }
                    return Ok(None);
                }
            }
        }
    }
    let gen = open_verified_generation_from_slot(store, &active_id)?;
    Ok(Some(Arc::new(gen)))
}

/// Manual rollback (`MOB-DAT-003`) to the retained non-revoked $N-1$ release
/// without lowering channel/publisher high-water marks.
pub fn rollback_to_previous_registry_release(
    store: &BootstrapStore,
) -> Result<Arc<RegistryGeneration>, MobileCoreError> {
    let state = store.registry_active_state()?;
    let prev_id = state.previous_eligible_release_id.ok_or_else(|| {
        MobileCoreError::RollbackUnavailable(
            "no eligible N-1 Concept Registry release is retained on disk".into(),
        )
    })?;
    let prev_catalog = store
        .registry_release_catalog(&prev_id)?
        .ok_or_else(|| MobileCoreError::RollbackUnavailable("N-1 catalog entry missing".into()))?;
    if prev_catalog.state == RegistryReleaseState::Revoked {
        return Err(MobileCoreError::RegistryTrust(
            "N-1 Concept Registry release has been revoked and cannot be restored".into(),
        ));
    }
    let gen = open_verified_generation_from_slot(store, &prev_id)?;
    // Verify health on N-1 before committing rollback pointer!
    let manifest_bytes = decode_hex_32(&prev_catalog.manifest_digest)?;
    let obr_hash = prev_catalog.artifact_whole_hash(0)?;
    let _ = verify_sidecar_samples(&gen.slot_dir, &gen.reader, &manifest_bytes, &obr_hash)?;
    store.commit_manual_registry_rollback(&prev_id)?;
    Ok(Arc::new(gen))
}

/// Reader-safe $N-1$ slot garbage collection (`MOB-05C` / `MOB-DAT-004`).
pub fn gc_retired_registry_releases(
    store: &BootstrapStore,
    held_generations: &[Arc<RegistryGeneration>],
    now_monotonic_ms: u64,
    force_reclaim_rollback_slot: bool,
) -> Result<RegistryGarbageCollectionReport, MobileCoreError> {
    let active = store.registry_active_state()?;
    let releases_root = store.registry_releases_root();
    let mut reclaimed = Vec::new();
    let mut retained = Vec::new();
    let mut reclaimed_bytes = 0u64;

    if !releases_root.is_dir() {
        return Ok(RegistryGarbageCollectionReport {
            reclaimed_release_ids: reclaimed,
            retained_release_ids: retained,
            reclaimed_bytes,
        });
    }

    for entry in fs::read_dir(&releases_root)
        .map_err(|error| MobileCoreError::Storage(format!("read releases root: {error}")))?
    {
        let entry =
            entry.map_err(|error| MobileCoreError::Storage(format!("dir entry: {error}")))?;
        if !entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
            continue;
        }
        let release_id = entry.file_name().to_string_lossy().to_string();
        if active.active_release_id.as_deref() == Some(release_id.as_str()) {
            retained.push(release_id);
            continue;
        }
        if active.previous_eligible_release_id.as_deref() == Some(release_id.as_str())
            && !force_reclaim_rollback_slot
            && now_monotonic_ms < active.rollback_retain_until_ms
        {
            retained.push(release_id);
            continue;
        }
        let reader_busy = held_generations
            .iter()
            .any(|gen| gen.release_id == release_id && Arc::strong_count(gen) > 1);
        if reader_busy {
            retained.push(release_id);
            continue;
        }

        let dir_path = entry.path();
        let bytes = directory_size_bytes(&dir_path);
        fs::remove_dir_all(&dir_path).map_err(|error| {
            MobileCoreError::Storage(format!("reclaim retired release {release_id}: {error}"))
        })?;
        store.mark_release_retired_reclaimed(&release_id)?;
        reclaimed_bytes = reclaimed_bytes.saturating_add(bytes);
        reclaimed.push(release_id);
    }

    Ok(RegistryGarbageCollectionReport {
        reclaimed_release_ids: reclaimed,
        retained_release_ids: retained,
        reclaimed_bytes,
    })
}

/// Inspect concepts in the active demand-paged OBR (`MOB-LIB-002` / `MOB-LIB-005`).
pub fn list_registry_concepts(
    generation: &RegistryGeneration,
    query: Option<&str>,
    limit: usize,
) -> Result<Vec<RegistryConceptView>, MobileCoreError> {
    let bounded = limit.clamp(1, 100);
    let needle = query
        .map(|q| q.trim().to_lowercase())
        .filter(|q| !q.is_empty());
    let mut file = File::open(&generation.obr_path)
        .map_err(|error| MobileCoreError::Storage(format!("open OBR for browse: {error}")))?;
    let file_len = file
        .metadata()
        .map_err(|error| MobileCoreError::Storage(format!("stat OBR: {error}")))?
        .len();
    if file_len < 32 {
        return Err(MobileCoreError::Security("OBR header truncated".into()));
    }
    let mut header = [0u8; 32];
    file.read_exact(&mut header)
        .map_err(|error| MobileCoreError::Storage(format!("read OBR header: {error}")))?;
    let entry_count = u64::from_le_bytes(header[8..16].try_into().unwrap());

    let mut results = Vec::new();
    for _ in 0..entry_count {
        let pos = file
            .stream_position()
            .map_err(|error| MobileCoreError::Storage(format!("OBR cursor: {error}")))?;
        if pos >= file_len {
            break;
        }
        let parsed = read_obr_entry_at(&mut file, pos)?;
        let matches = match &needle {
            None => true,
            Some(q) => {
                parsed.canonical_name.to_lowercase().contains(q)
                    || hex::encode(parsed.ccid).contains(q)
                    || parsed
                        .labels
                        .iter()
                        .any(|label| label.to_lowercase().contains(q))
            }
        };
        if matches {
            results.push(RegistryConceptView {
                ccid_hex: hex::encode(parsed.ccid),
                qid: parsed.qid,
                category: format!("{:?}", parsed.category),
                canonical_name: parsed.canonical_name,
                labels: parsed.labels,
            });
            if results.len() >= bounded {
                break;
            }
        }
    }
    Ok(results)
}

pub fn run_registry_health_v1(
    store: &BootstrapStore,
) -> Result<(RegistryGeneration, u32), MobileCoreError> {
    let active = store.registry_active_state()?;
    let release_id = active.active_release_id.ok_or_else(|| {
        MobileCoreError::Security("REGISTRY_HEALTH_V1 invoked with no active_release_id".into())
    })?;
    let catalog = store
        .registry_release_catalog(&release_id)?
        .ok_or_else(|| MobileCoreError::Security("active release missing from catalog".into()))?;
    if catalog.state == RegistryReleaseState::Revoked {
        return Err(MobileCoreError::RegistryTrust(
            "active release is revoked".into(),
        ));
    }

    let gen = open_verified_generation_from_slot(store, &release_id)?;
    let manifest_bytes = decode_hex_32(&catalog.manifest_digest)?;
    let obr_hash = catalog.artifact_whole_hash(0)?;
    let sampled = verify_sidecar_samples(&gen.slot_dir, &gen.reader, &manifest_bytes, &obr_hash)?;
    Ok((gen, sampled))
}

fn open_verified_generation_from_slot(
    store: &BootstrapStore,
    release_id: &str,
) -> Result<RegistryGeneration, MobileCoreError> {
    let catalog = store
        .registry_release_catalog(release_id)?
        .ok_or_else(|| MobileCoreError::Security("release slot missing catalog entry".into()))?;
    let slot_dir = store.registry_release_slot_dir(release_id);
    let manifest_file = slot_dir.join("release.manifest");
    let receipt_file = slot_dir.join("artifact_verification.receipt");
    let obr_path = slot_dir.join("concepts.obr");
    let labels_path = slot_dir.join("concepts.obr.labels.idx");
    let ccids_path = slot_dir.join("concepts.obr.ccids.idx");

    if !manifest_file.is_file() || !receipt_file.is_file() || !obr_path.is_file() {
        return Err(MobileCoreError::Security(
            "release slot is missing release.manifest, receipt, or concepts.obr".into(),
        ));
    }
    let stored_envelope = fs::read(&manifest_file)
        .map_err(|error| MobileCoreError::Storage(format!("read release.manifest: {error}")))?;
    if stored_envelope != catalog.release_envelope_cbor {
        return Err(MobileCoreError::Security(
            "release.manifest bytes do not match accepted catalog envelope".into(),
        ));
    }

    let obr_expected_len = catalog.artifact_final_length(0)?;
    let labels_expected_len = catalog.artifact_final_length(1)?;
    let ccids_expected_len = catalog.artifact_final_length(2)?;
    let obr_hash = catalog.artifact_whole_hash(0)?;
    let labels_hash = catalog.artifact_whole_hash(1)?;
    let ccids_hash = catalog.artifact_whole_hash(2)?;

    if fs::metadata(&obr_path)
        .map_err(|e| MobileCoreError::Storage(e.to_string()))?
        .len()
        != obr_expected_len
    {
        return Err(MobileCoreError::Security(
            "concepts.obr length mismatch".into(),
        ));
    }
    if fs::metadata(&labels_path)
        .map_err(|e| MobileCoreError::Storage(e.to_string()))?
        .len()
        != labels_expected_len
    {
        return Err(MobileCoreError::Security(
            "concepts.obr.labels.idx length mismatch".into(),
        ));
    }
    if fs::metadata(&ccids_path)
        .map_err(|e| MobileCoreError::Storage(e.to_string()))?
        .len()
        != ccids_expected_len
    {
        return Err(MobileCoreError::Security(
            "concepts.obr.ccids.idx length mismatch".into(),
        ));
    }

    let reader = open_indexed_registry_at(
        &obr_path,
        &obr_hash,
        &labels_hash,
        &ccids_hash,
        labels_expected_len,
        ccids_expected_len,
    )?;
    Ok(RegistryGeneration {
        release_id: release_id.to_owned(),
        manifest_digest: catalog.manifest_digest,
        release_sequence: catalog.release_sequence,
        slot_dir,
        obr_path,
        reader,
    })
}

fn open_indexed_registry_at(
    obr_path: &Path,
    obr_blake3_hex: &str,
    labels_blake3_hex: &str,
    ccids_blake3_hex: &str,
    labels_file_len: u64,
    ccids_file_len: u64,
) -> Result<IndexedConceptRegistry, MobileCoreError> {
    let header = ConceptRegistry::inspect_obr(obr_path)
        .map_err(|error| MobileCoreError::Security(format!("invalid OBR header: {error}")))?;
    if labels_file_len < REGISTRY_INDEX_HEADER_SIZE
        || !(labels_file_len - REGISTRY_INDEX_HEADER_SIZE)
            .is_multiple_of(REGISTRY_INDEX_RECORD_SIZE)
        || ccids_file_len < REGISTRY_INDEX_HEADER_SIZE
        || !(ccids_file_len - REGISTRY_INDEX_HEADER_SIZE).is_multiple_of(REGISTRY_INDEX_RECORD_SIZE)
    {
        return Err(MobileCoreError::Security(
            "sidecar index file length is not aligned to 24-byte records".into(),
        ));
    }
    let label_records = (labels_file_len - REGISTRY_INDEX_HEADER_SIZE) / REGISTRY_INDEX_RECORD_SIZE;
    let ccid_records = (ccids_file_len - REGISTRY_INDEX_HEADER_SIZE) / REGISTRY_INDEX_RECORD_SIZE;
    let manifest = ConceptRegistryManifest {
        manifest_version: 1,
        obr_schema_version: header.schema_version,
        builder_version: "mobile-v1".to_owned(),
        dedup_policy_version: "v1".to_owned(),
        built_at_utc: "2026-01-01T00:00:00Z".to_owned(),
        obr_blake3: obr_blake3_hex.to_owned(),
        entry_count: header.entry_count,
        label_count: header.label_count,
        sources: std::collections::BTreeMap::new(),
        label_index: Some(ConceptRegistryIndexManifest {
            schema_version: REGISTRY_INDEX_VERSION,
            record_size: REGISTRY_INDEX_RECORD_SIZE as u32,
            record_count: label_records,
            blake3: labels_blake3_hex.to_owned(),
            file_size: labels_file_len,
        }),
        ccid_index: Some(ConceptRegistryIndexManifest {
            schema_version: REGISTRY_INDEX_VERSION,
            record_size: REGISTRY_INDEX_RECORD_SIZE as u32,
            record_count: ccid_records,
            blake3: ccids_blake3_hex.to_owned(),
            file_size: ccids_file_len,
        }),
        lifecycle_blake3: None,
    };
    IndexedConceptRegistry::open(obr_path, &manifest, DEFAULT_INDEX_CACHE_CAPACITY)
        .map_err(|error| MobileCoreError::Security(format!("open IndexedConceptRegistry: {error}")))
}

fn verify_sidecar_samples(
    slot_dir: &Path,
    reader: &IndexedConceptRegistry,
    manifest_digest: &[u8; 32],
    expected_obr_blake3_hex: &str,
) -> Result<u32, MobileCoreError> {
    let expected_obr_checksum = decode_hex_32(expected_obr_blake3_hex)?;
    let obr_path = slot_dir.join("concepts.obr");
    let mut obr_file = File::open(&obr_path)
        .map_err(|error| MobileCoreError::Storage(format!("open OBR for health: {error}")))?;
    let mut total_checked = 0u32;

    for (role_u8, filename, expected_magic) in [
        (1u8, "concepts.obr.labels.idx", LABEL_INDEX_MAGIC),
        (2u8, "concepts.obr.ccids.idx", CCID_INDEX_MAGIC),
    ] {
        let sidecar_path = slot_dir.join(filename);
        let mut sidecar = File::open(&sidecar_path).map_err(|error| {
            MobileCoreError::Storage(format!("open {filename} for health: {error}"))
        })?;
        let sidecar_len = sidecar
            .metadata()
            .map_err(|error| MobileCoreError::Storage(error.to_string()))?
            .len();
        if sidecar_len < REGISTRY_INDEX_HEADER_SIZE {
            return Err(MobileCoreError::Security(format!(
                "{filename} truncated header"
            )));
        }
        let mut hdr = [0u8; REGISTRY_INDEX_HEADER_SIZE as usize];
        sidecar
            .read_exact(&mut hdr)
            .map_err(|error| MobileCoreError::Storage(error.to_string()))?;
        if hdr[0..4] != expected_magic {
            return Err(MobileCoreError::Security(format!("{filename} bad magic")));
        }
        let version = u32::from_le_bytes(hdr[4..8].try_into().unwrap());
        if version != REGISTRY_INDEX_VERSION {
            return Err(MobileCoreError::Security(format!(
                "{filename} unsupported version {version}"
            )));
        }
        let n = u64::from_le_bytes(hdr[8..16].try_into().unwrap());
        if hdr[16..48] != expected_obr_checksum {
            return Err(MobileCoreError::Security(format!(
                "{filename} OBR checksum binding mismatch"
            )));
        }
        if sidecar_len != REGISTRY_INDEX_HEADER_SIZE + n * REGISTRY_INDEX_RECORD_SIZE {
            return Err(MobileCoreError::Security(format!(
                "{filename} record count does not match file length"
            )));
        }
        if n == 0 {
            continue;
        }

        let mut ordinals = BTreeSet::new();
        ordinals.insert(0u64);
        ordinals.insert(n - 1);
        for i in 0..62u32 {
            let mut hasher = blake3::Hasher::new();
            hasher.update(HEALTH_SAMPLE_DOMAIN);
            hasher.update(manifest_digest);
            hasher.update(&[role_u8]);
            hasher.update(&i.to_le_bytes());
            let sample_val =
                u64::from_le_bytes(hasher.finalize().as_bytes()[0..8].try_into().unwrap());
            ordinals.insert(sample_val % n);
        }

        for ordinal in ordinals {
            sidecar
                .seek(SeekFrom::Start(
                    REGISTRY_INDEX_HEADER_SIZE + ordinal * REGISTRY_INDEX_RECORD_SIZE,
                ))
                .map_err(|error| MobileCoreError::Storage(error.to_string()))?;
            let mut rec = [0u8; REGISTRY_INDEX_RECORD_SIZE as usize];
            sidecar
                .read_exact(&mut rec)
                .map_err(|error| MobileCoreError::Storage(error.to_string()))?;
            let mut key = [0u8; 16];
            key.copy_from_slice(&rec[0..16]);
            let obr_offset = u64::from_le_bytes(rec[16..24].try_into().unwrap());

            let parsed = read_obr_entry_at(&mut obr_file, obr_offset)?;
            let by_ccid = reader
                .get_by_ccid_checked(&parsed.ccid)
                .map_err(|error| MobileCoreError::Security(error.to_string()))?
                .ok_or_else(|| MobileCoreError::Security("CCID round-trip returned None".into()))?;
            if by_ccid.ccid != parsed.ccid || by_ccid.canonical_name != parsed.canonical_name {
                return Err(MobileCoreError::Security(
                    "CCID round-trip returned mismatched concept".into(),
                ));
            }

            if role_u8 == 2 {
                if key != parsed.ccid {
                    return Err(MobileCoreError::Security(
                        "CCID sidecar key does not match OBR entry CCID".into(),
                    ));
                }
            } else {
                let matching_label = std::iter::once(parsed.canonical_name.as_str())
                    .chain(parsed.labels.iter().map(String::as_str))
                    .find(|candidate| label_prefix_key(candidate) == key)
                    .ok_or_else(|| {
                        MobileCoreError::Security(
                            "label sidecar key does not match any label at target OBR offset"
                                .into(),
                        )
                    })?;
                match reader
                    .resolve_checked(matching_label)
                    .map_err(|error| MobileCoreError::Security(error.to_string()))?
                {
                    ResolveResult::Found(concept) if concept.ccid == parsed.ccid => {}
                    ResolveResult::Ambiguous(list)
                        if list.iter().any(|item| item.ccid == parsed.ccid) => {}
                    _ => {
                        return Err(MobileCoreError::Security(
                            "label sidecar lookup failed to resolve back to OBR entry".into(),
                        ))
                    }
                }
            }
            total_checked = total_checked.saturating_add(1);
        }
    }
    Ok(total_checked)
}

struct ParsedObrEntry {
    ccid: [u8; 16],
    qid: u32,
    category: ConceptCategory,
    canonical_name: String,
    labels: Vec<String>,
}

fn read_obr_entry_at(file: &mut File, offset: u64) -> Result<ParsedObrEntry, MobileCoreError> {
    if offset < 32 {
        return Err(MobileCoreError::Security(
            "OBR offset precedes header".into(),
        ));
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| MobileCoreError::Security(format!("OBR seek failed: {error}")))?;
    let mut fixed = [0u8; 24];
    file.read_exact(&mut fixed)
        .map_err(|error| MobileCoreError::Security(format!("OBR entry header read: {error}")))?;
    let mut ccid = [0u8; 16];
    ccid.copy_from_slice(&fixed[0..16]);
    let qid = u32::from_le_bytes(fixed[16..20].try_into().unwrap());
    let category = ConceptCategory::from_u8(fixed[21]);
    let name_len = u16::from_le_bytes(fixed[22..24].try_into().unwrap()) as usize;
    let mut name_bytes = vec![0u8; name_len];
    file.read_exact(&mut name_bytes)
        .map_err(|error| MobileCoreError::Security(format!("OBR name read: {error}")))?;
    let canonical_name = String::from_utf8(name_bytes)
        .map_err(|_| MobileCoreError::Security("OBR canonical_name invalid UTF-8".into()))?;
    let mut label_count_bytes = [0u8; 2];
    file.read_exact(&mut label_count_bytes)
        .map_err(|error| MobileCoreError::Security(format!("OBR label count read: {error}")))?;
    let n_labels = u16::from_le_bytes(label_count_bytes) as usize;
    let mut labels = Vec::with_capacity(n_labels);
    for _ in 0..n_labels {
        let mut len_bytes = [0u8; 2];
        file.read_exact(&mut len_bytes)
            .map_err(|error| MobileCoreError::Security(format!("OBR label len: {error}")))?;
        let l_len = u16::from_le_bytes(len_bytes) as usize;
        let mut l_bytes = vec![0u8; l_len];
        file.read_exact(&mut l_bytes)
            .map_err(|error| MobileCoreError::Security(format!("OBR label bytes: {error}")))?;
        labels.push(
            String::from_utf8(l_bytes)
                .map_err(|_| MobileCoreError::Security("OBR label invalid UTF-8".into()))?,
        );
    }
    Ok(ParsedObrEntry {
        ccid,
        qid,
        category,
        canonical_name,
        labels,
    })
}

fn label_prefix_key(label: &str) -> [u8; 16] {
    let lower = label.to_lowercase();
    let digest = blake3::hash(lower.as_bytes());
    digest.as_bytes()[0..16].try_into().unwrap()
}

fn reassemble_role_chunks(
    store: &BootstrapStore,
    transfer_nonce: &str,
    role: u8,
    destination: &Path,
) -> Result<(u64, String), MobileCoreError> {
    let chunk_paths = store.verified_chunk_paths_for_role(transfer_nonce, role)?;
    let mut out = File::create(destination)
        .map_err(|error| MobileCoreError::Storage(format!("create staged artifact: {error}")))?;
    let mut hasher = blake3::Hasher::new();
    let mut total_bytes = 0u64;
    let mut buffer = [0u8; STREAM_BUFFER_BYTES];

    for chunk_path in chunk_paths {
        let mut input = File::open(&chunk_path).map_err(|error| {
            MobileCoreError::Storage(format!(
                "open verified chunk {}: {error}",
                chunk_path.display()
            ))
        })?;
        loop {
            let read = input.read(&mut buffer).map_err(|error| {
                MobileCoreError::Storage(format!("read verified chunk: {error}"))
            })?;
            if read == 0 {
                break;
            }
            out.write_all(&buffer[..read]).map_err(|error| {
                MobileCoreError::Storage(format!("write staged artifact: {error}"))
            })?;
            hasher.update(&buffer[..read]);
            total_bytes = total_bytes.saturating_add(read as u64);
        }
    }
    out.sync_all()
        .map_err(|error| MobileCoreError::Storage(format!("sync staged artifact: {error}")))?;
    Ok((total_bytes, hasher.finalize().to_hex().to_string()))
}

fn sync_regular_file(path: &Path) -> Result<(), MobileCoreError> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| MobileCoreError::Storage(format!("sync {}: {error}", path.display())))
}

fn decode_hex_32(hex_value: &str) -> Result<[u8; 32], MobileCoreError> {
    let bytes = hex::decode(hex_value)
        .map_err(|_| MobileCoreError::Security("digest is not canonical hex".into()))?;
    bytes
        .try_into()
        .map_err(|_| MobileCoreError::Security("digest must be 32 bytes".into()))
}

fn directory_size_bytes(path: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    let mut sum = 0u64;
    for entry in entries.flatten() {
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                sum = sum.saturating_add(meta.len());
            } else if meta.is_dir() {
                sum = sum.saturating_add(directory_size_bytes(&entry.path()));
            }
        }
    }
    sum
}

/// Deterministic builder for valid `OBR1` + `OBLI` + `OBCI` binary artifacts and
/// Ed25519-signed `ConceptRegistryTrustProfile` / `ChannelHead` / `ReleaseManifest`
/// envelopes (supports single-chunk and 8 MiB multi-chunk releases for tests & local fixtures).
#[derive(Clone, Debug)]
pub struct SignedObrReleaseBundle {
    pub trust_profile_cbor: Vec<u8>,
    pub channel_head_cbor: Vec<u8>,
    pub release_envelope_cbor: Vec<u8>,
    pub release_id_hex: String,
    pub manifest_digest_hex: String,
    pub obr_bytes: Vec<u8>,
    pub labels_idx_bytes: Vec<u8>,
    pub ccids_idx_bytes: Vec<u8>,
}

pub fn build_signed_obr_release_bundle(
    channel_seed: [u8; 32],
    release_seed: [u8; 32],
    profile_generation: u64,
    head_generation: u64,
    release_sequence: u64,
    concepts: &[(ResolvedConcept, Vec<String>)],
    revoked_release_ids: &[[u8; 32]],
) -> SignedObrReleaseBundle {
    let (obr_bytes, labels_idx_bytes, ccids_idx_bytes) = build_obr_and_sidecar_bytes(concepts);
    let channel_signing = SigningKey::from_bytes(&channel_seed);
    let release_signing = SigningKey::from_bytes(&release_seed);
    let fixed_chunk: usize = 8 * 1024 * 1024;

    let role_payloads = [&obr_bytes, &labels_idx_bytes, &ccids_idx_bytes];
    let artifacts: Vec<Value> = role_payloads
        .iter()
        .enumerate()
        .map(|(role_idx, payload)| {
            let role = role_idx as u8;
            let chunks: Vec<Value> = payload
                .chunks(fixed_chunk)
                .enumerate()
                .map(|(chunk_idx, slice)| {
                    let mut h = blake3::Hasher::new();
                    h.update(REGISTRY_CHUNK_DOMAIN);
                    h.update(&[role]);
                    h.update(&(chunk_idx as u32).to_le_bytes());
                    h.update(&(slice.len() as u32).to_le_bytes());
                    h.update(slice);
                    cbor_map(vec![
                        cbor_uint(chunk_idx as u64),
                        cbor_uint(slice.len() as u64),
                        Value::Bytes(h.finalize().as_bytes().to_vec()),
                    ])
                })
                .collect();
            cbor_map(vec![
                cbor_uint(u64::from(role)),
                cbor_uint(payload.len() as u64),
                Value::Bytes(blake3::hash(payload).as_bytes().to_vec()),
                Value::Array(vec![cbor_uint(1), cbor_uint(0)]),
                cbor_uint(chunks.len() as u64),
                Value::Array(chunks),
            ])
        })
        .collect();

    let runtime_range = cbor_map(vec![
        cbor_uint(7),
        cbor_uint(7),
        cbor_uint(1),
        cbor_uint(u64::from(u32::MAX)),
        cbor_uint(1),
        cbor_uint(u64::from(u32::MAX)),
    ]);

    let revocations_cbor: Vec<Value> = revoked_release_ids
        .iter()
        .map(|id| Value::Bytes(id.to_vec()))
        .collect();

    let manifest_body = cbor_encode(&cbor_map(vec![
        cbor_uint(1),
        cbor_uint(release_sequence),
        Value::Array(vec![cbor_uint(1), cbor_uint(release_sequence)]),
        Value::Text(format!("obr-release-{release_sequence}")),
        Value::Array(artifacts),
        runtime_range.clone(),
        cbor_uint(64 * 1024 * 1024),
        cbor_map(vec![Value::Null, Value::Null]),
        cbor_map(vec![
            Value::Bytes([0x31; 32].to_vec()),
            Value::Bytes([0x32; 32].to_vec()),
            Value::Bytes([0x33; 32].to_vec()),
        ]),
        cbor_map(vec![Value::Array(revocations_cbor)]),
    ]));
    let manifest_digest = domain_hash(
        b"onebrain:concept-registry-manifest-body:1\0",
        &manifest_body,
    );
    let release_id = domain_hash(b"onebrain:concept-registry-release:1\0", &manifest_digest);

    let head_body = cbor_encode(&cbor_map(vec![
        cbor_uint(1),
        Value::Text("stable".into()),
        cbor_uint(head_generation),
        cbor_uint(release_sequence),
        Value::Bytes(release_id.to_vec()),
        Value::Bytes(manifest_digest.to_vec()),
        runtime_range,
        cbor_uint(1),
    ]));
    let head_digest = domain_hash(
        b"onebrain:concept-registry-channel-head-body:1\0",
        &head_body,
    );

    let release_sig = release_signing
        .sign(&sig_payload(
            b"onebrain:concept-registry-envelope:1\0",
            &[&release_id, &manifest_digest],
            "release-v1",
        ))
        .to_bytes();
    let release_envelope_cbor = cbor_encode(&cbor_map(vec![
        cbor_uint(1),
        Value::Bytes(manifest_body),
        Value::Bytes(release_id.to_vec()),
        Value::Bytes(manifest_digest.to_vec()),
        cbor_uint(1),
        Value::Text("release-v1".into()),
        Value::Bytes(release_sig.to_vec()),
    ]));

    let channel_sig = channel_signing
        .sign(&sig_payload(
            b"onebrain:concept-registry-channel-head:1\0",
            &[&head_digest],
            "channel-v1",
        ))
        .to_bytes();
    let channel_head_cbor = cbor_encode(&cbor_map(vec![
        cbor_uint(1),
        Value::Bytes(head_body),
        Value::Bytes(head_digest.to_vec()),
        cbor_uint(1),
        Value::Text("channel-v1".into()),
        Value::Bytes(channel_sig.to_vec()),
    ]));

    let trust_profile_cbor = cbor_encode(&cbor_map(vec![
        cbor_uint(1),
        cbor_uint(profile_generation),
        Value::Array(vec![cbor_map(vec![
            Value::Text("channel-v1".into()),
            Value::Bytes(channel_signing.verifying_key().to_bytes().to_vec()),
        ])]),
        Value::Array(vec![cbor_map(vec![
            Value::Text("release-v1".into()),
            Value::Bytes(release_signing.verifying_key().to_bytes().to_vec()),
        ])]),
        Value::Array(vec![cbor_map(vec![
            Value::Text("stable".into()),
            cbor_uint(1),
            Value::Bytes(head_digest.to_vec()),
            cbor_uint(1),
            Value::Bytes(release_id.to_vec()),
            Value::Bytes(manifest_digest.to_vec()),
        ])]),
        cbor_uint(1),
    ]));

    SignedObrReleaseBundle {
        trust_profile_cbor,
        channel_head_cbor,
        release_envelope_cbor,
        release_id_hex: hex::encode(release_id),
        manifest_digest_hex: hex::encode(manifest_digest),
        obr_bytes,
        labels_idx_bytes,
        ccids_idx_bytes,
    }
}

pub fn build_obr_and_sidecar_bytes(
    concepts: &[(ResolvedConcept, Vec<String>)],
) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let total_labels: u64 = concepts
        .iter()
        .map(|(_, aliases)| (aliases.len() as u64).saturating_add(1))
        .sum();
    let mut obr = Vec::new();
    obr.extend_from_slice(b"OBR1");
    obr.extend_from_slice(&1u32.to_le_bytes());
    obr.extend_from_slice(&(concepts.len() as u64).to_le_bytes());
    obr.extend_from_slice(&total_labels.to_le_bytes());
    obr.extend_from_slice(&[0u8; 8]);

    let mut label_records: Vec<([u8; 16], u64)> = Vec::new();
    let mut ccid_records: Vec<([u8; 16], u64)> = Vec::new();

    for (concept, aliases) in concepts {
        let offset = obr.len() as u64;
        obr.extend_from_slice(&concept.ccid);
        obr.extend_from_slice(&concept.qid.to_le_bytes());
        obr.push(1u8); // source priority 1
        let cat_u8 = concept.category as u8;
        obr.push(cat_u8);
        let name_bytes = concept.canonical_name.as_bytes();
        obr.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        obr.extend_from_slice(name_bytes);
        obr.extend_from_slice(&(aliases.len() as u16).to_le_bytes());

        ccid_records.push((concept.ccid, offset));
        let mut seen_keys = BTreeSet::new();
        let canon_key = label_prefix_key(&concept.canonical_name);
        seen_keys.insert(canon_key);
        label_records.push((canon_key, offset));

        for alias in aliases {
            let a_bytes = alias.as_bytes();
            obr.extend_from_slice(&(a_bytes.len() as u16).to_le_bytes());
            obr.extend_from_slice(a_bytes);
            let k = label_prefix_key(alias);
            if seen_keys.insert(k) {
                label_records.push((k, offset));
            }
        }
    }

    let obr_checksum = *blake3::hash(&obr).as_bytes();
    label_records.sort_by_key(|(k, off)| (*k, *off));
    ccid_records.sort_by_key(|(k, off)| (*k, *off));

    let labels_idx = encode_sidecar_index(LABEL_INDEX_MAGIC, obr_checksum, &label_records);
    let ccids_idx = encode_sidecar_index(CCID_INDEX_MAGIC, obr_checksum, &ccid_records);
    (obr, labels_idx, ccids_idx)
}

fn encode_sidecar_index(
    magic: [u8; 4],
    obr_checksum: [u8; 32],
    records: &[([u8; 16], u64)],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(64 + records.len() * 24);
    out.extend_from_slice(&magic);
    out.extend_from_slice(&REGISTRY_INDEX_VERSION.to_le_bytes());
    out.extend_from_slice(&(records.len() as u64).to_le_bytes());
    out.extend_from_slice(&obr_checksum);
    out.extend_from_slice(&[0u8; 16]);
    for (key, offset) in records {
        out.extend_from_slice(key);
        out.extend_from_slice(&offset.to_le_bytes());
    }
    out
}

fn cbor_map(values: Vec<Value>) -> Value {
    Value::Map(
        values
            .into_iter()
            .enumerate()
            .map(|(idx, val)| (cbor_uint(idx as u64), val))
            .collect(),
    )
}

fn cbor_uint(val: u64) -> Value {
    Value::Integer(val.into())
}

fn cbor_encode(val: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    ciborium::ser::into_writer(val, &mut out).expect("encode deterministic CBOR");
    out
}

fn domain_hash(domain: &[u8], payload: &[u8]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(domain);
    h.update(payload);
    *h.finalize().as_bytes()
}

fn sig_payload(domain: &[u8], digests: &[&[u8; 32]], key_id: &str) -> Vec<u8> {
    let mut msg = domain.to_vec();
    for d in digests {
        msg.extend_from_slice(*d);
    }
    msg.extend_from_slice(&1u64.to_le_bytes());
    msg.extend_from_slice(&(key_id.len() as u16).to_le_bytes());
    msg.extend_from_slice(key_id.as_bytes());
    msg
}
