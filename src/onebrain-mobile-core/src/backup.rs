use std::{io::Cursor, path::Path};

use ku_core::concept_registry::ConceptLookup;
use onebrain_archive::{
    inspect_legacy_archive_v1, seal_archive, verify_dataset_archive_v2, ArchiveCredential,
    ArchiveError, ArchiveInspection, ArchiveLimits, FileSecureSpoolFactory, LogicalRestoreSink,
    RecoveryKey, OBAR_V1_MAGIC, OBAR_V2_MAGIC,
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{
    knowledge::{KuLocalOrganizationRecord, MobileKnowledgeEngine},
    migrations::{DatasetRollbackMode, MigrationCoordinator},
    BootstrapStore, MobileCoreError, PrivateDraftStore, RegistryChannelHighWater,
    RegistryReleaseHighWater,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArchivedRawDraftEntry {
    pub content_language: String,
    pub content_utf8: Vec<u8>,
    pub saved_at_monotonic_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArchivedKnowledgeUnitEntry {
    pub object_cid_hex: String,
    pub title: String,
    pub canonical_object_bytes: Vec<u8>,
    pub source_text: Option<String>,
    pub predecessor_cid: Option<[u8; 32]>,
    pub organization: KuLocalOrganizationRecord,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MobileDatasetBackupPayloadV2 {
    pub payload_version: u32,
    pub exported_at_monotonic_ms: u64,
    pub required_registry_trust_profile_generation: u64,
    pub channel_high_waters: Vec<(String, RegistryChannelHighWater)>,
    pub publisher_release_high_water: Option<RegistryReleaseHighWater>,
    pub raw_drafts: Vec<ArchivedRawDraftEntry>,
    pub knowledge_units: Vec<ArchivedKnowledgeUnitEntry>,
    pub preserved_media_refs: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MobileBackupReceipt {
    pub container_magic: String,
    pub archive_bytes_len: u64,
    pub raw_draft_count: usize,
    pub knowledge_unit_count: usize,
    pub preserved_media_count: usize,
    pub required_trust_profile_generation: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MobileRestoreReceipt {
    pub disposition: String,
    pub local_installation_epoch_preserved: String,
    pub restored_raw_drafts: usize,
    pub restored_knowledge_units: usize,
    pub active_dataset_generation: u64,
    pub merged_channel_high_waters: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScopedExportBundle {
    pub export_format: String,
    pub reviewed_by_user: bool,
    pub include_source_text: bool,
    pub exported_items: Vec<ScopedExportItem>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScopedExportItem {
    pub object_cid_hex: String,
    pub title: String,
    pub canonical_object_base64: String,
    pub source_text: Option<String>,
    pub tags: Vec<String>,
}

struct VerifiedPayloadCollector {
    plaintext: Zeroizing<Vec<u8>>,
}

impl Default for VerifiedPayloadCollector {
    fn default() -> Self {
        Self {
            plaintext: Zeroizing::new(Vec::new()),
        }
    }
}

impl LogicalRestoreSink for VerifiedPayloadCollector {
    fn restore_verified(
        &mut self,
        plaintext: &[u8],
        _inspection: &ArchiveInspection,
    ) -> Result<(), ArchiveError> {
        self.plaintext.extend_from_slice(plaintext);
        Ok(())
    }
}

/// Seal a portable `OBARV002` encrypted backup (`MOB-DAT-006`).
/// Never includes non-portable installation keys, hardware wrapping keys, or raw identity material.
#[allow(clippy::too_many_arguments)]
pub fn create_encrypted_backup_v2(
    store: &BootstrapStore,
    draft_store: &PrivateDraftStore,
    knowledge: Option<&MobileKnowledgeEngine>,
    preserved_media_refs: Vec<String>,
    recovery_key_bytes: [u8; 32],
    now_monotonic_ms: u64,
) -> Result<(Vec<u8>, MobileBackupReceipt), MobileCoreError> {
    let credential = ArchiveCredential::RecoveryKey(RecoveryKey::from_bytes(recovery_key_bytes)?);
    let trust_gen = store.accepted_trust_profile_generation()?.unwrap_or(1);
    let channel_high_waters = store.all_channel_high_waters()?;
    let publisher_release_high_water = store.publisher_release_high_water()?;

    let mut raw_drafts = Vec::new();
    for item in draft_store.list_recent(100)? {
        if let Some((receipt, text)) = draft_store.read_decrypted_text(&item.draft_ref)? {
            raw_drafts.push(ArchivedRawDraftEntry {
                content_language: receipt.content_language,
                content_utf8: text.as_bytes().to_vec(),
                saved_at_monotonic_ms: receipt.saved_at_monotonic_ms,
            });
        }
    }

    let mut knowledge_units = Vec::new();
    if let Some(engine) = knowledge {
        for (rec, canonical_bytes, src_text, org) in engine.export_all_for_backup()? {
            knowledge_units.push(ArchivedKnowledgeUnitEntry {
                object_cid_hex: hex::encode(rec.object_cid),
                title: rec.title,
                canonical_object_bytes: canonical_bytes,
                source_text: src_text,
                predecessor_cid: rec.predecessor_cid,
                organization: org,
            });
        }
    }

    let payload = MobileDatasetBackupPayloadV2 {
        payload_version: 2,
        exported_at_monotonic_ms: now_monotonic_ms,
        required_registry_trust_profile_generation: trust_gen,
        channel_high_waters,
        publisher_release_high_water,
        raw_drafts,
        knowledge_units,
        preserved_media_refs,
    };

    let serialized = Zeroizing::new(serde_json::to_vec(&payload)?);
    let mut archive_bytes = Vec::new();
    let _inspection = seal_archive(
        Cursor::new(serialized.as_slice()),
        &mut archive_bytes,
        &credential,
        &ArchiveLimits::default(),
    )?;

    let receipt = MobileBackupReceipt {
        container_magic: String::from_utf8_lossy(OBAR_V2_MAGIC).into_owned(),
        archive_bytes_len: archive_bytes.len() as u64,
        raw_draft_count: payload.raw_drafts.len(),
        knowledge_unit_count: payload.knowledge_units.len(),
        preserved_media_count: payload.preserved_media_refs.len(),
        required_trust_profile_generation: trust_gen,
    };
    Ok((archive_bytes, receipt))
}

/// Inspect and stage-restore a verified `OBARV002` backup (`MOB-DAT-007`) under
/// `ImportDataKeepCurrentIdentity` while enforcing whole-tuple Registry high-water
/// anti-rollback, equivocation fail-closed rejection, and `UpgradeRequiredForRegistryTrustProfile`.
#[allow(clippy::too_many_arguments)]
pub fn restore_encrypted_backup_v2(
    archive_bytes: &[u8],
    spool_root: &Path,
    recovery_key_bytes: [u8; 32],
    embedded_trust_profile_generation: u64,
    store: &BootstrapStore,
    draft_store: &PrivateDraftStore,
    knowledge: Option<(&mut MobileKnowledgeEngine, &dyn ConceptLookup)>,
    migrations: &MigrationCoordinator,
) -> Result<MobileRestoreReceipt, MobileCoreError> {
    if archive_bytes.starts_with(OBAR_V1_MAGIC) {
        let key = RecoveryKey::from_bytes(recovery_key_bytes)?;
        let _ =
            inspect_legacy_archive_v1(Cursor::new(archive_bytes), &key, &ArchiveLimits::default())?;
        return Err(MobileCoreError::Archive(
            "OBARV001 legacy archives are inspection-only and cannot produce verified restore state"
                .into(),
        ));
    }

    std::fs::create_dir_all(spool_root).map_err(|error| {
        MobileCoreError::Storage(format!(
            "cannot create verified backup spool {}: {error}",
            spool_root.display()
        ))
    })?;
    let factory = FileSecureSpoolFactory::new(spool_root)?;
    let credential = ArchiveCredential::RecoveryKey(RecoveryKey::from_bytes(recovery_key_bytes)?);
    let verified = verify_dataset_archive_v2(
        Cursor::new(archive_bytes.to_vec()),
        &factory,
        &credential,
        &ArchiveLimits::default(),
    )?;

    let mut collector = VerifiedPayloadCollector::default();
    verified.materialize_into(&mut collector)?;
    let payload: MobileDatasetBackupPayloadV2 = serde_json::from_slice(&collector.plaintext)?;
    if payload.payload_version != 2 {
        return Err(MobileCoreError::Archive(
            "unsupported mobile backup payload_version".into(),
        ));
    }

    if payload.required_registry_trust_profile_generation > embedded_trust_profile_generation {
        return Err(MobileCoreError::UpgradeRequiredForRegistryTrustProfile {
            required_generation: payload.required_registry_trust_profile_generation,
            embedded_generation: embedded_trust_profile_generation,
        });
    }

    // Preserve local installation epoch (`ImportDataKeepCurrentIdentity`)!
    let local_authority = store.installation_authority()?.ok_or_else(|| {
        MobileCoreError::Security(
            "restore requires an initialized local installation authority".into(),
        )
    })?;

    // Whole-tuple high-water merge (rejecting equivocation fail-closed!).
    store.merge_restored_registry_high_waters(
        &payload.channel_high_waters,
        payload.publisher_release_high_water.as_ref(),
    )?;

    let mut restored_raw_drafts = 0usize;
    for raw in &payload.raw_drafts {
        draft_store.save_text(
            &raw.content_language,
            &raw.content_utf8,
            raw.saved_at_monotonic_ms,
        )?;
        restored_raw_drafts = restored_raw_drafts.saturating_add(1);
    }

    let mut restored_kus = 0usize;
    if !payload.knowledge_units.is_empty() {
        let (engine, registry) = knowledge.ok_or_else(|| {
            MobileCoreError::RegistryNotReady(
                "restoring canonical Knowledge Units requires an active ReadyOffline Concept Registry"
                    .into(),
            )
        })?;
        for ku in &payload.knowledge_units {
            engine.restore_ku_from_backup(
                registry,
                &ku.title,
                &ku.canonical_object_bytes,
                ku.source_text.as_deref(),
                ku.predecessor_cid,
                &ku.organization,
            )?;
            restored_kus = restored_kus.saturating_add(1);
        }
    }

    let switched = migrations.switch_shadow_dataset_generation(
        1,
        DatasetRollbackMode::NMinusOneReadWrite,
        false,
    )?;

    // Verify local installation_epoch was never overwritten by backup bytes!
    let after_authority = store.installation_authority()?.ok_or_else(|| {
        MobileCoreError::Security("local installation authority missing after restore".into())
    })?;
    if after_authority != local_authority {
        return Err(MobileCoreError::Security(
            "restore violated ImportDataKeepCurrentIdentity invariant".into(),
        ));
    }

    Ok(MobileRestoreReceipt {
        disposition: "ImportDataKeepCurrentIdentity".into(),
        local_installation_epoch_preserved: local_authority.installation_epoch,
        restored_raw_drafts,
        restored_knowledge_units: restored_kus,
        active_dataset_generation: switched.dataset_generation,
        merged_channel_high_waters: payload.channel_high_waters.len(),
    })
}

/// Explicit user-reviewed scoped export (`MOB-DAT-008` / `MOB-SCR-DAT-007`).
pub fn export_reviewed_scoped_bundle(
    knowledge: &MobileKnowledgeEngine,
    selected_object_cids_hex: &[String],
    include_source_text: bool,
) -> Result<ScopedExportBundle, MobileCoreError> {
    if selected_object_cids_hex.is_empty() || selected_object_cids_hex.len() > 100 {
        return Err(MobileCoreError::InvalidArgument(
            "scoped export requires 1..=100 explicitly selected private KU references".into(),
        ));
    }
    let all = knowledge.export_all_for_backup()?;
    let mut exported_items = Vec::new();
    for selected in selected_object_cids_hex {
        let norm = selected.trim().to_ascii_lowercase();
        let Some((rec, obj_bytes, src_text, org)) = all
            .iter()
            .find(|(r, _, _, _)| hex::encode(r.object_cid) == norm)
        else {
            return Err(MobileCoreError::InvalidArgument(format!(
                "selected KU '{norm}' does not exist in private workspace"
            )));
        };
        exported_items.push(ScopedExportItem {
            object_cid_hex: norm,
            title: rec.title.clone(),
            canonical_object_base64: onebrain_core::base64_encode_standard(obj_bytes),
            source_text: if include_source_text {
                src_text.clone()
            } else {
                None
            },
            tags: org.tags.clone(),
        });
    }
    Ok(ScopedExportBundle {
        export_format: "onebrain.mobile.scoped-export/v1".into(),
        reviewed_by_user: true,
        include_source_text,
        exported_items,
    })
}
