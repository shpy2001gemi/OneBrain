//! Pillar 6 PAM Companion Host Bridge (`PAM-MEM-001`, `FW-04`, `FW-06`, `AI-004`).
//!
//! Host-level coordinator wrapping `ku_mediator::vnext_pam::PamVaultRuntime` with:
//! - Tier M0 (`EphemeralTurnBuffer` + `SomaticImmediate/1` emergency wipe),
//! - Tier M1 (`WorkingSessionThread` sealed under `PrivateVault` `purpose = 4`),
//! - Tier M2 (`SourceArtifact` + immutable `EpisodicObservationLinks`, blocking
//!   `SomaticBiometricRaw` from remote cognition and Personal Mesh export),
//! - Tier M3 (`SovereignActorPreferences`, never exported remotely),
//! - `FW-06` (`NEG-POMV-001`): RAG citation emits local `ExposureObservation` only and
//!   zero `UseEvidencePayload`,
//! - `AI-004`: `PamRecommendationCard` + `PamRecommendationGuard` enforcing
//!   `model_qualified == false` and `epistemic_label == "unassessed"` until qualified.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ku_core::foundation::{
    ActorId, AtomicVerifiedBackend, DisclosureClass, ExposureRecordOutcome, ObjectCid,
    ObjectReference, PrivateVault, PutVerifiedOutcome, SourceArtifact,
};
use ku_mediator::vnext_pam::{
    ConfirmedPamRecommendation, EphemeralTurnBuffer, EpisodicObservationLink,
    EpisodicObservationLinks, InMemoryPamMetadataStore, PamCompanionExposure, PamError,
    PamRecommendationActionKind, PamRecommendationCard, PamRecommendationGuard,
    PamSealedMetadataStore, PamVaultRuntime, SanitizedRemoteCognitionBundle,
    SovereignActorPreferences, UserConfirmationGesture, WorkingSessionThread,
    UNASSESSED_EPISTEMIC_LABEL,
};
use redb::{Database, TableDefinition};

const PAM_METADATA_TABLE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_pam_sealed_metadata_v1");
pub const PAM_METADATA_DB_FILENAME: &str = "vnext_pam_metadata.redb";

/// Lazy-opened Redb implementation of `PamSealedMetadataStore` storing only
/// already `PrivateVault::seal_local_metadata`-encrypted ciphertext (`purpose = 4`).
pub struct LazyRedbPamMetadataStore {
    path: PathBuf,
    db: Mutex<Option<Database>>,
}

impl LazyRedbPamMetadataStore {
    pub fn new_lazy(data_dir: impl AsRef<Path>) -> Self {
        Self {
            path: data_dir.as_ref().join(PAM_METADATA_DB_FILENAME),
            db: Mutex::new(None),
        }
    }

    pub fn db_path(&self) -> &Path {
        &self.path
    }

    fn with_db<T>(
        &self,
        create_if_missing: bool,
        f: impl FnOnce(&Database) -> Result<T, String>,
    ) -> Result<Option<T>, String> {
        let mut guard = self
            .db
            .lock()
            .map_err(|_| "PAM metadata store mutex poisoned".to_owned())?;
        if guard.is_none() {
            if !create_if_missing && !self.path.exists() {
                return Ok(None);
            }
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let db = Database::create(&self.path).map_err(|e| e.to_string())?;
            let write = db.begin_write().map_err(|e| e.to_string())?;
            {
                write
                    .open_table(PAM_METADATA_TABLE)
                    .map_err(|e| e.to_string())?;
            }
            write.commit().map_err(|e| e.to_string())?;
            *guard = Some(db);
        }
        f(guard.as_ref().expect("opened above")).map(Some)
    }
}

impl PamSealedMetadataStore for LazyRedbPamMetadataStore {
    fn put_sealed(&mut self, binding: [u8; 32], ciphertext: Vec<u8>) -> Result<(), String> {
        self.with_db(true, |db| {
            let write = db.begin_write().map_err(|e| e.to_string())?;
            {
                let mut table = write
                    .open_table(PAM_METADATA_TABLE)
                    .map_err(|e| e.to_string())?;
                table
                    .insert(binding.as_slice(), ciphertext.as_slice())
                    .map_err(|e| e.to_string())?;
            }
            write.commit().map_err(|e| e.to_string())
        })?;
        Ok(())
    }

    fn get_sealed(&self, binding: &[u8; 32]) -> Result<Option<Vec<u8>>, String> {
        let res = self.with_db(false, |db| {
            let read = db.begin_read().map_err(|e| e.to_string())?;
            let table = read
                .open_table(PAM_METADATA_TABLE)
                .map_err(|e| e.to_string())?;
            Ok(table
                .get(binding.as_slice())
                .map_err(|e| e.to_string())?
                .map(|g| g.value().to_vec()))
        })?;
        Ok(res.flatten())
    }
}

/// Host-level PAM companion facade enforcing `model_qualified: false` (`AI-004`)
/// and zero `UseEvidence` on RAG citations (`FW-06`).
pub struct PamHostBridge<
    B: AtomicVerifiedBackend,
    M: PamSealedMetadataStore = InMemoryPamMetadataStore,
> {
    runtime: PamVaultRuntime<B, M>,
}

impl<B: AtomicVerifiedBackend> PamHostBridge<B, InMemoryPamMetadataStore> {
    pub fn new_in_memory(owner_actor: ActorId, vault: PrivateVault<B>) -> Result<Self, PamError> {
        Ok(Self {
            runtime: PamVaultRuntime::new(owner_actor, vault)?,
        })
    }
}

impl<B: AtomicVerifiedBackend> PamHostBridge<B, LazyRedbPamMetadataStore> {
    pub fn new_lazy_redb(
        owner_actor: ActorId,
        vault: PrivateVault<B>,
        data_dir: impl AsRef<Path>,
    ) -> Result<Self, PamError> {
        Ok(Self {
            runtime: PamVaultRuntime::with_metadata_store(
                owner_actor,
                vault,
                LazyRedbPamMetadataStore::new_lazy(data_dir),
            )?,
        })
    }
}

impl<B: AtomicVerifiedBackend, M: PamSealedMetadataStore> PamHostBridge<B, M> {
    pub const fn model_qualified(&self) -> bool {
        false
    }

    pub const fn epistemic_label(&self) -> &'static str {
        UNASSESSED_EPISTEMIC_LABEL
    }

    pub const fn emitted_use_evidence_payload_count(&self) -> usize {
        self.runtime.emitted_use_evidence_payload_count()
    }

    pub fn begin_turn_m0(&mut self, turn_id: [u8; 32], prompt_bytes: &[u8]) {
        self.runtime.begin_turn_m0(turn_id, prompt_bytes);
    }

    pub fn active_turn_m0(&self) -> Option<&EphemeralTurnBuffer> {
        self.runtime.active_turn_m0()
    }

    pub fn complete_turn_m0(&mut self) {
        self.runtime.complete_turn_m0();
    }

    pub fn abort_somatic_immediate(&mut self) {
        self.runtime.abort_somatic_immediate();
    }

    pub fn save_working_session_m1(
        &mut self,
        session: &WorkingSessionThread,
    ) -> Result<[u8; 32], PamError> {
        self.runtime.save_working_session_m1(session)
    }

    pub fn load_working_session_m1(
        &self,
        thread_id: [u8; 32],
    ) -> Result<Option<WorkingSessionThread>, PamError> {
        self.runtime.load_working_session_m1(thread_id)
    }

    pub fn ingest_source_artifact_m2(
        &self,
        artifact: &SourceArtifact,
    ) -> Result<(ObjectCid, PutVerifiedOutcome), PamError> {
        self.runtime.ingest_source_artifact_m2(artifact)
    }

    pub fn record_episodic_link_m2(
        &mut self,
        link: EpisodicObservationLink,
    ) -> Result<[u8; 32], PamError> {
        self.runtime.record_episodic_link_m2(link)
    }

    pub fn load_episodic_links_m2(&self) -> Result<EpisodicObservationLinks, PamError> {
        self.runtime.load_episodic_links_m2()
    }

    pub fn prepare_remote_cognition_bundle(
        &self,
        episodic_link_ids: &[[u8; 32]],
        include_preferences: bool,
    ) -> Result<SanitizedRemoteCognitionBundle, PamError> {
        self.runtime
            .prepare_remote_cognition_bundle(episodic_link_ids, include_preferences)
    }

    pub fn save_preferences_m3(
        &mut self,
        preferences: &SovereignActorPreferences,
    ) -> Result<[u8; 32], PamError> {
        self.runtime.save_preferences_m3(preferences)
    }

    pub fn load_preferences_m3(&self) -> Result<SovereignActorPreferences, PamError> {
        self.runtime.load_preferences_m3()
    }

    pub fn record_rag_citation(
        &mut self,
        target: ObjectReference,
        turn_id: [u8; 32],
    ) -> Result<ExposureRecordOutcome, PamError> {
        self.runtime.record_rag_citation(target, turn_id)
    }

    pub fn companion_exposures(&self) -> &[PamCompanionExposure] {
        self.runtime.companion_exposures()
    }

    /// Stage an unqualified (`model_qualified: false`, `"unassessed"`) recommendation card.
    pub fn stage_unassessed_recommendation(
        &mut self,
        card_id: [u8; 32],
        action_kind: PamRecommendationActionKind,
        summary: impl Into<String>,
        target_refs: Vec<ObjectReference>,
        target_disclosure: DisclosureClass,
    ) -> Result<PamRecommendationCard, PamError> {
        let card = PamRecommendationCard::new_unassessed(
            card_id,
            action_kind,
            summary,
            target_refs,
            target_disclosure,
        )?;
        debug_assert!(!card.model_qualified);
        debug_assert_eq!(card.epistemic_label, UNASSESSED_EPISTEMIC_LABEL);
        self.runtime
            .recommendation_guard_mut()
            .stage_card(card.clone())?;
        Ok(card)
    }

    pub fn stage_card(&mut self, card: PamRecommendationCard) -> Result<(), PamError> {
        self.runtime.recommendation_guard_mut().stage_card(card)
    }

    pub fn confirm_recommendation(
        &mut self,
        gesture: UserConfirmationGesture,
    ) -> Result<ConfirmedPamRecommendation, PamError> {
        self.runtime
            .recommendation_guard_mut()
            .confirm_with_user_gesture(gesture)
    }

    pub fn recommendation_guard(&self) -> &PamRecommendationGuard {
        self.runtime.recommendation_guard()
    }
}
