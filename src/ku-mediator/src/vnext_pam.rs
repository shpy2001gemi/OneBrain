//! P6 Sovereign Personal AI Mediator (`PAM`) 4-Tier Private Vault Runtime (`purpose = 4`),
//! RAG Exposure Firewall (`FW-06` / `NEG-POMV-001`), and `AI-004` Recommendation Guard.
//!
//! Enforced invariants:
//! - **4-Tier Personal Memory Hierarchy (`PamVaultRuntime<B>`):**
//!   - **Tier M0 (`EphemeralTurnBuffer`):** `Zeroizing<Vec<u8>>` wiped immediately via `wipe_now()`
//!     on turn completion, cancellation, or `SomaticImmediate/1` abort; never persisted.
//!   - **Tier M1 (`WorkingSessionThread`):** rolling window `<= 256` turns sealed in `PrivateVault`
//!     under `VaultPurpose::LocalMetadata` (`purpose = 4`, `DisclosureClass::LocalOnly` by default;
//!     `personal_mesh_sync_enabled: bool` default `false` and gated to same `ActorId`).
//!   - **Tier M2 (`EpisodicObservationLinks`):** immutable links binding `AI-003`
//!     `ObservationEvent` + `SourceArtifact` CIDs to `ku-semantic-draft/2.0` private drafts;
//!     blocks `ObservationModality::SomaticBiometricRaw` from any remote `RUN-003` export.
//!   - **Tier M3 (`SovereignActorPreferences`):** domain vocabulary preferences,
//!     `DisclosureClass::LocalOnly` default, and exploration appetite sealed in `PrivateVault`
//!     (`purpose = 4`); never exported to remote `RUN-003` cognition workers.
//! - **RAG Exposure Firewall (`FW-06` / `NEG-POMV-001`):** Quoting/retrieving KUs in `PAM` RAG
//!   records ONLY local `ExposureObservation` (`ExposureKind::CompanionCitation`) and **never**
//!   emits `UseEvidencePayload`.
//! - **Recommendation Card & Honest Qualification (`AI-004`):** `PamRecommendationCard` +
//!   `PamRecommendationGuard` enforce `performs_side_effect() == false` until explicit user
//!   confirmation gesture and honest `model_qualified: false` (`epistemic_label: "unassessed"`).

use std::collections::{BTreeMap, VecDeque};

use ku_core::foundation::{
    decode_canonical, encode_canonical, ActorId, AtomicVerifiedBackend, CanonicalError,
    CanonicalValue, ConceptCcid, DisclosureClass, EventCid, ExposureKind as CoreExposureKind,
    ExposureObservation, ExposureRecordOutcome, ExposureTelemetry, KnownObjectKind,
    MetabolicViewError, ObjectCid, ObjectReference, PrivateVault, PutVerifiedOutcome,
    ResourceProfile, SourceArtifact, VerifiedStoreError, SOURCE_ARTIFACT_KIND,
};
use zeroize::{Zeroize, Zeroizing};

pub const PAM_VAULT_PURPOSE_LOCAL_METADATA: u8 = 4;
pub const MAX_WORKING_SESSION_TURNS: usize = 256;
pub const SEMANTIC_DRAFT_PROFILE_V2: &str = "ku-semantic-draft/2.0";
pub const SOMATIC_IMMEDIATE_ABORT_PROFILE: &str = "SomaticImmediate/1";
pub const UNASSESSED_EPISTEMIC_LABEL: &str = "unassessed";
pub const DEFAULT_PAM_EXPOSURE_CAPACITY: usize = 16_384;
const PAM_SCHEMA_MAJOR: u64 = 1;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum PamError {
    #[error("private vault store error: {0:?}")]
    Vault(VerifiedStoreError),
    #[error("canonical encoding error: {0:?}")]
    Canonical(CanonicalError),
    #[error("exposure telemetry error: {0}")]
    Telemetry(String),
    #[error("source artifact error: {0:?}")]
    Observation(ku_core::foundation::ObservationError),
    #[error("ephemeral turn buffer is already wiped or inactive")]
    TurnBufferWiped,
    #[error("personal mesh sync is disabled by default for Tier M1 working sessions")]
    PersonalMeshSyncDisabled,
    #[error("personal mesh sync across distinct ActorIds is forbidden")]
    CrossActorSyncForbidden,
    #[error("immutable Tier M2 episodic observation link conflict")]
    ImmutableEpisodicLinkConflict,
    #[error("Tier M2 semantic draft must declare profile `{SEMANTIC_DRAFT_PROFILE_V2}` and `LOCAL_ONLY` disclosure")]
    InvalidSemanticDraftBinding,
    #[error("raw somatic/biometric observations (`SomaticBiometricRaw`) are strictly forbidden from remote `RUN-003` export")]
    SomaticBiometricRemoteExportForbidden,
    #[error(
        "Tier M3 sovereign actor preferences are strictly forbidden from remote `RUN-003` export"
    )]
    PreferencesRemoteExportForbidden,
    #[error("unknown Tier M2 episodic link referenced in remote cognition export")]
    UnknownEpisodicLink,
    #[error("invalid preference value or disclosure default")]
    InvalidPreferences,
    #[error("unqualified model cannot claim `model_qualified: true` or assessed epistemic label")]
    UnqualifiedModelClaimForbidden,
    #[error("unknown or invalid recommendation card")]
    InvalidRecommendationCard,
    #[error("explicit user confirmation gesture mismatch or replay")]
    InvalidUserConfirmation,
}

impl From<VerifiedStoreError> for PamError {
    fn from(error: VerifiedStoreError) -> Self {
        Self::Vault(error)
    }
}

impl From<CanonicalError> for PamError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<MetabolicViewError> for PamError {
    fn from(error: MetabolicViewError) -> Self {
        Self::Telemetry(format!("{error:?}"))
    }
}

impl From<ku_core::foundation::ObservationError> for PamError {
    fn from(error: ku_core::foundation::ObservationError) -> Self {
        Self::Observation(error)
    }
}

/// Pluggable backing map for `PrivateVault::seal_local_metadata` (`purpose = 4`) ciphertexts.
/// Only `PrivateVault`-sealed ciphertexts ever touch this store.
pub trait PamSealedMetadataStore {
    fn put_sealed(&mut self, binding: [u8; 32], ciphertext: Vec<u8>) -> Result<(), String>;
    fn get_sealed(&self, binding: &[u8; 32]) -> Result<Option<Vec<u8>>, String>;
}

#[derive(Clone, Debug, Default)]
pub struct InMemoryPamMetadataStore {
    rows: BTreeMap<[u8; 32], Vec<u8>>,
}

impl InMemoryPamMetadataStore {
    pub fn raw_ciphertext(&self, binding: &[u8; 32]) -> Option<&[u8]> {
        self.rows.get(binding).map(Vec::as_slice)
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

impl PamSealedMetadataStore for InMemoryPamMetadataStore {
    fn put_sealed(&mut self, binding: [u8; 32], ciphertext: Vec<u8>) -> Result<(), String> {
        self.rows.insert(binding, ciphertext);
        Ok(())
    }

    fn get_sealed(&self, binding: &[u8; 32]) -> Result<Option<Vec<u8>>, String> {
        Ok(self.rows.get(binding).cloned())
    }
}

// ---------------------------------------------------------------------------
// Tier M0: EphemeralTurnBuffer (`Zeroizing<Vec<u8>>`)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PamTurnAbortReason {
    TurnCompleted,
    UserCancelled,
    SomaticImmediateV1,
}

impl PamTurnAbortReason {
    pub const fn profile_label(self) -> &'static str {
        match self {
            Self::TurnCompleted => "TurnCompleted/1",
            Self::UserCancelled => "UserCancelled/1",
            Self::SomaticImmediateV1 => SOMATIC_IMMEDIATE_ABORT_PROFILE,
        }
    }
}

/// Tier M0 transient prompt/scratchpad buffer backed by `Zeroizing<Vec<u8>>`.
/// Never persisted to disk or `PrivateVault`.
#[derive(Debug)]
pub struct EphemeralTurnBuffer {
    turn_id: [u8; 32],
    scratch: Zeroizing<Vec<u8>>,
    wiped: bool,
    last_wipe_reason: Option<PamTurnAbortReason>,
}

impl EphemeralTurnBuffer {
    pub fn new(turn_id: [u8; 32], initial_bytes: &[u8]) -> Self {
        let mut buf = Vec::with_capacity(initial_bytes.len());
        buf.extend_from_slice(initial_bytes);
        Self {
            turn_id,
            scratch: Zeroizing::new(buf),
            wiped: false,
            last_wipe_reason: None,
        }
    }

    pub const fn turn_id(&self) -> &[u8; 32] {
        &self.turn_id
    }

    pub const fn is_wiped(&self) -> bool {
        self.wiped
    }

    pub const fn last_wipe_reason(&self) -> Option<PamTurnAbortReason> {
        self.last_wipe_reason
    }

    pub const fn is_persisted(&self) -> bool {
        false
    }

    pub fn append_transient(&mut self, bytes: &[u8]) -> Result<(), PamError> {
        if self.wiped {
            return Err(PamError::TurnBufferWiped);
        }
        self.scratch.extend_from_slice(bytes);
        Ok(())
    }

    pub fn as_bytes(&self) -> Result<&[u8], PamError> {
        if self.wiped {
            return Err(PamError::TurnBufferWiped);
        }
        Ok(self.scratch.as_slice())
    }

    pub fn len(&self) -> usize {
        self.scratch.len()
    }

    pub fn is_empty(&self) -> bool {
        self.scratch.is_empty()
    }

    /// Immediately zeroize and clear the transient turn buffer in RAM.
    pub fn wipe_now(&mut self) {
        self.wipe_with_reason(PamTurnAbortReason::UserCancelled);
    }

    pub fn wipe_with_reason(&mut self, reason: PamTurnAbortReason) {
        self.scratch.zeroize();
        self.scratch.clear();
        self.wiped = true;
        self.last_wipe_reason = Some(reason);
    }
}

// ---------------------------------------------------------------------------
// Tier M1: WorkingSessionThread (`<= 256` rolling window, `LOCAL_ONLY` default)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum SessionTurnSpeaker {
    User = 0,
    Companion = 1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionTurnEntry {
    pub sequence: u64,
    pub speaker: SessionTurnSpeaker,
    pub content: String,
    pub cited_objects: Vec<ObjectReference>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkingSessionThread {
    pub thread_id: [u8; 32],
    pub disclosure: DisclosureClass,
    pub personal_mesh_sync_enabled: bool,
    pub next_sequence: u64,
    pub turns: VecDeque<SessionTurnEntry>,
}

impl WorkingSessionThread {
    pub fn new(thread_id: [u8; 32]) -> Self {
        Self {
            thread_id,
            disclosure: DisclosureClass::LocalOnly,
            personal_mesh_sync_enabled: false,
            next_sequence: 0,
            turns: VecDeque::new(),
        }
    }

    pub fn push_turn(
        &mut self,
        speaker: SessionTurnSpeaker,
        content: impl Into<String>,
        cited_objects: Vec<ObjectReference>,
    ) -> u64 {
        let seq = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        if self.turns.len() >= MAX_WORKING_SESSION_TURNS {
            self.turns.pop_front();
        }
        self.turns.push_back(SessionTurnEntry {
            sequence: seq,
            speaker,
            content: content.into(),
            cited_objects,
        });
        seq
    }

    fn to_canonical_bytes(&self) -> Result<Vec<u8>, PamError> {
        let turns_val = CanonicalValue::Array(
            self.turns
                .iter()
                .map(|t| {
                    CanonicalValue::Map(vec![
                        (0, CanonicalValue::Unsigned(t.sequence)),
                        (1, CanonicalValue::Unsigned(t.speaker as u64)),
                        (2, CanonicalValue::Text(t.content.clone())),
                        (
                            3,
                            CanonicalValue::Array(
                                t.cited_objects.iter().map(pam_obj_ref_to_value).collect(),
                            ),
                        ),
                    ])
                })
                .collect(),
        );
        let root = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(PAM_SCHEMA_MAJOR)),
            (1, CanonicalValue::Bytes(self.thread_id.to_vec())),
            (2, CanonicalValue::Unsigned(self.disclosure as u64)),
            (3, CanonicalValue::Bool(self.personal_mesh_sync_enabled)),
            (4, CanonicalValue::Unsigned(self.next_sequence)),
            (5, turns_val),
        ]);
        encode_canonical(&root, ResourceProfile::ObjectV1).map_err(Into::into)
    }

    fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, PamError> {
        let value = decode_canonical(bytes, ResourceProfile::ObjectV1)?;
        let CanonicalValue::Map(fields) = value else {
            return Err(PamError::InvalidPreferences);
        };
        if fields.len() != 6 || fields.iter().enumerate().any(|(i, (k, _))| *k != i as u64) {
            return Err(PamError::InvalidPreferences);
        }
        let CanonicalValue::Unsigned(PAM_SCHEMA_MAJOR) = fields[0].1 else {
            return Err(PamError::InvalidPreferences);
        };
        let thread_id = fixed_pam_bytes::<32>(&fields[1].1)?;
        let CanonicalValue::Unsigned(disc_u64) = fields[2].1 else {
            return Err(PamError::InvalidPreferences);
        };
        let disclosure = disclosure_from_u64(disc_u64)?;
        let CanonicalValue::Bool(personal_mesh_sync_enabled) = fields[3].1 else {
            return Err(PamError::InvalidPreferences);
        };
        let CanonicalValue::Unsigned(next_sequence) = fields[4].1 else {
            return Err(PamError::InvalidPreferences);
        };
        let CanonicalValue::Array(turns_arr) = &fields[5].1 else {
            return Err(PamError::InvalidPreferences);
        };
        if turns_arr.len() > MAX_WORKING_SESSION_TURNS {
            return Err(PamError::InvalidPreferences);
        }
        let mut turns = VecDeque::with_capacity(turns_arr.len());
        for item in turns_arr {
            let CanonicalValue::Map(tf) = item else {
                return Err(PamError::InvalidPreferences);
            };
            if tf.len() != 4 || tf.iter().enumerate().any(|(i, (k, _))| *k != i as u64) {
                return Err(PamError::InvalidPreferences);
            }
            let CanonicalValue::Unsigned(sequence) = tf[0].1 else {
                return Err(PamError::InvalidPreferences);
            };
            let CanonicalValue::Unsigned(spk_u64) = tf[1].1 else {
                return Err(PamError::InvalidPreferences);
            };
            let speaker = match spk_u64 {
                0 => SessionTurnSpeaker::User,
                1 => SessionTurnSpeaker::Companion,
                _ => return Err(PamError::InvalidPreferences),
            };
            let CanonicalValue::Text(content) = &tf[2].1 else {
                return Err(PamError::InvalidPreferences);
            };
            let CanonicalValue::Array(objs_arr) = &tf[3].1 else {
                return Err(PamError::InvalidPreferences);
            };
            let mut cited_objects = Vec::with_capacity(objs_arr.len());
            for obj_val in objs_arr {
                cited_objects.push(pam_obj_ref_from_value(obj_val)?);
            }
            turns.push_back(SessionTurnEntry {
                sequence,
                speaker,
                content: content.clone(),
                cited_objects,
            });
        }
        Ok(Self {
            thread_id,
            disclosure,
            personal_mesh_sync_enabled,
            next_sequence,
            turns,
        })
    }
}

// ---------------------------------------------------------------------------
// Tier M2: EpisodicObservationLinks (`AI-003` -> `ku-semantic-draft/2.0`)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum ObservationModality {
    TextNote = 0,
    VoiceTranscript = 1,
    DocumentSpan = 2,
    /// Raw somatic / biometric sensor capture — strictly `LOCAL_ONLY`, never exported to `RUN-003`.
    SomaticBiometricRaw = 3,
}

impl ObservationModality {
    const fn from_u64(code: u64) -> Option<Self> {
        match code {
            0 => Some(Self::TextNote),
            1 => Some(Self::VoiceTranscript),
            2 => Some(Self::DocumentSpan),
            3 => Some(Self::SomaticBiometricRaw),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpisodicObservationLink {
    pub link_id: [u8; 32],
    pub observation_event_cid: EventCid,
    pub source_artifact: ObjectReference,
    pub semantic_draft_profile: String,
    pub semantic_draft_cid: [u8; 32],
    pub modality: ObservationModality,
    pub disclosure: DisclosureClass,
}

impl EpisodicObservationLink {
    pub fn new(
        observation_event_cid: EventCid,
        source_artifact: ObjectReference,
        semantic_draft_cid: [u8; 32],
        modality: ObservationModality,
    ) -> Result<Self, PamError> {
        if observation_event_cid.as_bytes() == &[0; 32]
            || source_artifact.reference_kind != SOURCE_ARTIFACT_KIND.0
            || source_artifact.cid == [0; 32]
            || semantic_draft_cid == [0; 32]
        {
            return Err(PamError::InvalidSemanticDraftBinding);
        }
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:pam-episodic-link:1\0");
        hasher.update(observation_event_cid.as_bytes());
        hasher.update(&source_artifact.cid);
        hasher.update(&semantic_draft_cid);
        let link_id = *hasher.finalize().as_bytes();
        Ok(Self {
            link_id,
            observation_event_cid,
            source_artifact,
            semantic_draft_profile: SEMANTIC_DRAFT_PROFILE_V2.to_string(),
            semantic_draft_cid,
            modality,
            disclosure: DisclosureClass::LocalOnly,
        })
    }

    pub const fn allows_remote_cognition_export(&self) -> bool {
        !matches!(self.modality, ObservationModality::SomaticBiometricRaw)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EpisodicObservationLinks {
    links: BTreeMap<[u8; 32], EpisodicObservationLink>,
}

impl EpisodicObservationLinks {
    pub fn record_immutable(&mut self, link: EpisodicObservationLink) -> Result<(), PamError> {
        if link.semantic_draft_profile != SEMANTIC_DRAFT_PROFILE_V2
            || link.disclosure != DisclosureClass::LocalOnly
            || link.source_artifact.reference_kind != SOURCE_ARTIFACT_KIND.0
        {
            return Err(PamError::InvalidSemanticDraftBinding);
        }
        match self.links.get(&link.link_id) {
            Some(existing) if existing == &link => Ok(()),
            Some(_) => Err(PamError::ImmutableEpisodicLinkConflict),
            None => {
                self.links.insert(link.link_id, link);
                Ok(())
            }
        }
    }

    pub fn get(&self, link_id: &[u8; 32]) -> Option<&EpisodicObservationLink> {
        self.links.get(link_id)
    }

    pub fn len(&self) -> usize {
        self.links.len()
    }

    pub fn is_empty(&self) -> bool {
        self.links.is_empty()
    }

    fn to_canonical_bytes(&self) -> Result<Vec<u8>, PamError> {
        let arr = CanonicalValue::Array(
            self.links
                .values()
                .map(|l| {
                    CanonicalValue::Map(vec![
                        (0, CanonicalValue::Bytes(l.link_id.to_vec())),
                        (
                            1,
                            CanonicalValue::Bytes(l.observation_event_cid.as_bytes().to_vec()),
                        ),
                        (2, pam_obj_ref_to_value(&l.source_artifact)),
                        (3, CanonicalValue::Text(l.semantic_draft_profile.clone())),
                        (4, CanonicalValue::Bytes(l.semantic_draft_cid.to_vec())),
                        (5, CanonicalValue::Unsigned(l.modality as u64)),
                        (6, CanonicalValue::Unsigned(l.disclosure as u64)),
                    ])
                })
                .collect(),
        );
        let root = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(PAM_SCHEMA_MAJOR)),
            (1, arr),
        ]);
        encode_canonical(&root, ResourceProfile::ObjectV1).map_err(Into::into)
    }

    fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, PamError> {
        let value = decode_canonical(bytes, ResourceProfile::ObjectV1)?;
        let CanonicalValue::Map(fields) = value else {
            return Err(PamError::InvalidSemanticDraftBinding);
        };
        if fields.len() != 2 || fields[0].0 != 0 || fields[1].0 != 1 {
            return Err(PamError::InvalidSemanticDraftBinding);
        }
        let CanonicalValue::Unsigned(PAM_SCHEMA_MAJOR) = fields[0].1 else {
            return Err(PamError::InvalidSemanticDraftBinding);
        };
        let CanonicalValue::Array(items) = &fields[1].1 else {
            return Err(PamError::InvalidSemanticDraftBinding);
        };
        let mut out = Self::default();
        for item in items {
            let CanonicalValue::Map(lf) = item else {
                return Err(PamError::InvalidSemanticDraftBinding);
            };
            if lf.len() != 7 || lf.iter().enumerate().any(|(i, (k, _))| *k != i as u64) {
                return Err(PamError::InvalidSemanticDraftBinding);
            }
            let link_id = fixed_pam_bytes::<32>(&lf[0].1)?;
            let observation_event_cid = EventCid::from_bytes(fixed_pam_bytes::<32>(&lf[1].1)?);
            let source_artifact = pam_obj_ref_from_value(&lf[2].1)
                .map_err(|_| PamError::InvalidSemanticDraftBinding)?;
            let CanonicalValue::Text(profile) = &lf[3].1 else {
                return Err(PamError::InvalidSemanticDraftBinding);
            };
            let semantic_draft_cid = fixed_pam_bytes::<32>(&lf[4].1)?;
            let CanonicalValue::Unsigned(mod_u64) = lf[5].1 else {
                return Err(PamError::InvalidSemanticDraftBinding);
            };
            let modality = ObservationModality::from_u64(mod_u64)
                .ok_or(PamError::InvalidSemanticDraftBinding)?;
            let CanonicalValue::Unsigned(disc_u64) = lf[6].1 else {
                return Err(PamError::InvalidSemanticDraftBinding);
            };
            let disclosure = disclosure_from_u64(disc_u64)?;
            let link = EpisodicObservationLink {
                link_id,
                observation_event_cid,
                source_artifact,
                semantic_draft_profile: profile.clone(),
                semantic_draft_cid,
                modality,
                disclosure,
            };
            out.record_immutable(link)?;
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Tier M3: SovereignActorPreferences (`LOCAL_ONLY`, never exported to `RUN-003`)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SovereignActorPreferences {
    pub vocabulary_preferences: BTreeMap<String, ConceptCcid>,
    pub default_disclosure: DisclosureClass,
    /// Basis points in `0..=10_000` (e.g., `2_000` = 20% exploration appetite).
    pub exploration_appetite_bps: u16,
}

impl Default for SovereignActorPreferences {
    fn default() -> Self {
        Self {
            vocabulary_preferences: BTreeMap::new(),
            default_disclosure: DisclosureClass::LocalOnly,
            exploration_appetite_bps: 2_000,
        }
    }
}

impl SovereignActorPreferences {
    pub fn validate(&self) -> Result<(), PamError> {
        if self.exploration_appetite_bps > 10_000
            || self.default_disclosure != DisclosureClass::LocalOnly
            || self
                .vocabulary_preferences
                .iter()
                .any(|(term, ccid)| term.trim().is_empty() || ccid.as_bytes() == &[0; 16])
        {
            return Err(PamError::InvalidPreferences);
        }
        Ok(())
    }

    pub const fn allows_remote_worker_export(&self) -> bool {
        false
    }

    fn to_canonical_bytes(&self) -> Result<Vec<u8>, PamError> {
        self.validate()?;
        let vocab = CanonicalValue::Array(
            self.vocabulary_preferences
                .iter()
                .map(|(k, v)| {
                    CanonicalValue::Array(vec![
                        CanonicalValue::Text(k.clone()),
                        CanonicalValue::Bytes(v.as_bytes().to_vec()),
                    ])
                })
                .collect(),
        );
        let root = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(PAM_SCHEMA_MAJOR)),
            (1, vocab),
            (2, CanonicalValue::Unsigned(self.default_disclosure as u64)),
            (
                3,
                CanonicalValue::Unsigned(u64::from(self.exploration_appetite_bps)),
            ),
        ]);
        encode_canonical(&root, ResourceProfile::ObjectV1).map_err(Into::into)
    }

    fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, PamError> {
        let value = decode_canonical(bytes, ResourceProfile::ObjectV1)?;
        let CanonicalValue::Map(fields) = value else {
            return Err(PamError::InvalidPreferences);
        };
        if fields.len() != 4 || fields.iter().enumerate().any(|(i, (k, _))| *k != i as u64) {
            return Err(PamError::InvalidPreferences);
        }
        let CanonicalValue::Unsigned(PAM_SCHEMA_MAJOR) = fields[0].1 else {
            return Err(PamError::InvalidPreferences);
        };
        let CanonicalValue::Array(vocab_arr) = &fields[1].1 else {
            return Err(PamError::InvalidPreferences);
        };
        let mut vocabulary_preferences = BTreeMap::new();
        for pair in vocab_arr {
            let CanonicalValue::Array(items) = pair else {
                return Err(PamError::InvalidPreferences);
            };
            if items.len() != 2 {
                return Err(PamError::InvalidPreferences);
            }
            let CanonicalValue::Text(term) = &items[0] else {
                return Err(PamError::InvalidPreferences);
            };
            let ccid = ConceptCcid::from_bytes(fixed_pam_bytes::<16>(&items[1])?);
            vocabulary_preferences.insert(term.clone(), ccid);
        }
        let CanonicalValue::Unsigned(disc_u64) = fields[2].1 else {
            return Err(PamError::InvalidPreferences);
        };
        let default_disclosure = disclosure_from_u64(disc_u64)?;
        let CanonicalValue::Unsigned(appetite_u64) = fields[3].1 else {
            return Err(PamError::InvalidPreferences);
        };
        let exploration_appetite_bps =
            u16::try_from(appetite_u64).map_err(|_| PamError::InvalidPreferences)?;
        let prefs = Self {
            vocabulary_preferences,
            default_disclosure,
            exploration_appetite_bps,
        };
        prefs.validate()?;
        Ok(prefs)
    }
}

// ---------------------------------------------------------------------------
// FW-06 (`NEG-POMV-001`) Companion Citation Exposure Firewall
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExposureKind {
    QueryHit,
    Retrieval,
    Presented,
    CompanionCitation,
}

impl ExposureKind {
    const fn to_core_kind(self) -> CoreExposureKind {
        match self {
            Self::QueryHit => CoreExposureKind::QueryHit,
            Self::Retrieval => CoreExposureKind::Retrieval,
            Self::Presented => CoreExposureKind::Presented,
            Self::CompanionCitation => CoreExposureKind::Presented,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PamCompanionExposure {
    pub observation_id: [u8; 32],
    pub target: ObjectReference,
    pub kind: ExposureKind,
    pub local_sequence: u64,
    pub private_context_commitment: [u8; 32],
}

impl PamCompanionExposure {
    pub const fn is_use_evidence(&self) -> bool {
        false
    }

    pub const fn is_local_only(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// AI-004 Recommendation Card & Confirmation Guard
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PamRecommendationActionKind {
    SavePrivateKuDraft,
    PublishPublicKu,
    AdoptReceptorBinding,
    RegisterStandingNeed,
    DelegateRemoteCognition,
}

/// Non-side-effecting recommendation card (`AI-004`). Never executes state mutations
/// directly and honestly labels unqualified models `"unassessed"`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PamRecommendationCard {
    pub card_id: [u8; 32],
    pub action_kind: PamRecommendationActionKind,
    pub summary: String,
    pub target_refs: Vec<ObjectReference>,
    pub target_disclosure: DisclosureClass,
    pub model_qualified: bool,
    pub epistemic_label: &'static str,
}

impl PamRecommendationCard {
    pub fn new_unassessed(
        card_id: [u8; 32],
        action_kind: PamRecommendationActionKind,
        summary: impl Into<String>,
        target_refs: Vec<ObjectReference>,
        target_disclosure: DisclosureClass,
    ) -> Result<Self, PamError> {
        let summary = summary.into();
        if card_id == [0; 32] || summary.trim().is_empty() {
            return Err(PamError::InvalidRecommendationCard);
        }
        Ok(Self {
            card_id,
            action_kind,
            summary,
            target_refs,
            target_disclosure,
            model_qualified: false,
            epistemic_label: UNASSESSED_EPISTEMIC_LABEL,
        })
    }

    pub const fn performs_side_effect(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserConfirmationGesture {
    pub actor: ActorId,
    pub card_id: [u8; 32],
    pub gesture_nonce: [u8; 32],
    pub local_tick: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfirmedPamRecommendation {
    pub card: PamRecommendationCard,
    pub confirmed_by: ActorId,
    pub confirmation_commitment: [u8; 32],
    pub confirmed_at_tick: u64,
}

impl ConfirmedPamRecommendation {
    pub const fn performs_side_effect(&self) -> bool {
        true
    }
}

#[derive(Clone, Debug, Default)]
pub struct PamRecommendationGuard {
    owner_actor: Option<ActorId>,
    staged: BTreeMap<[u8; 32], PamRecommendationCard>,
    used_gesture_nonces: std::collections::BTreeSet<[u8; 32]>,
}

impl PamRecommendationGuard {
    pub fn new(owner_actor: ActorId) -> Self {
        Self {
            owner_actor: Some(owner_actor),
            staged: BTreeMap::new(),
            used_gesture_nonces: std::collections::BTreeSet::new(),
        }
    }

    pub const fn performs_side_effect(&self) -> bool {
        false
    }

    pub fn stage_card(&mut self, card: PamRecommendationCard) -> Result<(), PamError> {
        if card.card_id == [0; 32] || card.summary.trim().is_empty() {
            return Err(PamError::InvalidRecommendationCard);
        }
        if card.model_qualified || card.epistemic_label != UNASSESSED_EPISTEMIC_LABEL {
            return Err(PamError::UnqualifiedModelClaimForbidden);
        }
        self.staged.insert(card.card_id, card);
        Ok(())
    }

    pub fn pending_card(&self, card_id: &[u8; 32]) -> Option<&PamRecommendationCard> {
        self.staged.get(card_id)
    }

    pub fn confirm_with_user_gesture(
        &mut self,
        gesture: UserConfirmationGesture,
    ) -> Result<ConfirmedPamRecommendation, PamError> {
        if let Some(owner) = self.owner_actor {
            if gesture.actor != owner {
                return Err(PamError::InvalidUserConfirmation);
            }
        }
        if gesture.gesture_nonce == [0; 32]
            || !self.used_gesture_nonces.insert(gesture.gesture_nonce)
        {
            return Err(PamError::InvalidUserConfirmation);
        }
        let card = self
            .staged
            .remove(&gesture.card_id)
            .ok_or(PamError::InvalidRecommendationCard)?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:pam-user-confirmation:1\0");
        hasher.update(gesture.actor.as_bytes());
        hasher.update(&gesture.card_id);
        hasher.update(&gesture.gesture_nonce);
        hasher.update(&gesture.local_tick.to_be_bytes());
        Ok(ConfirmedPamRecommendation {
            card,
            confirmed_by: gesture.actor,
            confirmation_commitment: *hasher.finalize().as_bytes(),
            confirmed_at_tick: gesture.local_tick,
        })
    }
}

// ---------------------------------------------------------------------------
// Sanitized RUN-003 Remote Cognition Envelope
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SanitizedRemoteCognitionBundle {
    pub allowed_draft_cids: Vec<[u8; 32]>,
    pub contains_somatic_biometric_raw: bool,
    pub contains_tier_m3_preferences: bool,
}

// ---------------------------------------------------------------------------
// PamVaultRuntime<B, M>
// ---------------------------------------------------------------------------

pub struct PamVaultRuntime<
    B: AtomicVerifiedBackend,
    M: PamSealedMetadataStore = InMemoryPamMetadataStore,
> {
    owner_actor: ActorId,
    vault: PrivateVault<B>,
    metadata_store: M,
    active_turn_m0: Option<EphemeralTurnBuffer>,
    exposure_telemetry: ExposureTelemetry,
    companion_exposures: Vec<PamCompanionExposure>,
    next_exposure_sequence: u64,
    recommendation_guard: PamRecommendationGuard,
}

impl<B: AtomicVerifiedBackend> PamVaultRuntime<B, InMemoryPamMetadataStore> {
    pub fn new(owner_actor: ActorId, vault: PrivateVault<B>) -> Result<Self, PamError> {
        Self::with_metadata_store(owner_actor, vault, InMemoryPamMetadataStore::default())
    }
}

impl<B: AtomicVerifiedBackend, M: PamSealedMetadataStore> PamVaultRuntime<B, M> {
    pub fn with_metadata_store(
        owner_actor: ActorId,
        vault: PrivateVault<B>,
        metadata_store: M,
    ) -> Result<Self, PamError> {
        Ok(Self {
            owner_actor,
            vault,
            metadata_store,
            active_turn_m0: None,
            exposure_telemetry: ExposureTelemetry::new(DEFAULT_PAM_EXPOSURE_CAPACITY)?,
            companion_exposures: Vec::new(),
            next_exposure_sequence: 0,
            recommendation_guard: PamRecommendationGuard::new(owner_actor),
        })
    }

    pub const fn owner_actor(&self) -> ActorId {
        self.owner_actor
    }

    pub const fn vault(&self) -> &PrivateVault<B> {
        &self.vault
    }

    pub const fn metadata_store(&self) -> &M {
        &self.metadata_store
    }

    pub const fn vault_purpose(&self) -> u8 {
        PAM_VAULT_PURPOSE_LOCAL_METADATA
    }

    // --- Tier M0: EphemeralTurnBuffer ---

    pub fn begin_turn_m0(&mut self, turn_id: [u8; 32], prompt_bytes: &[u8]) {
        if let Some(prev) = &mut self.active_turn_m0 {
            prev.wipe_with_reason(PamTurnAbortReason::TurnCompleted);
        }
        self.active_turn_m0 = Some(EphemeralTurnBuffer::new(turn_id, prompt_bytes));
    }

    pub fn active_turn_m0(&self) -> Option<&EphemeralTurnBuffer> {
        self.active_turn_m0.as_ref()
    }

    pub fn complete_turn_m0(&mut self) {
        if let Some(buf) = &mut self.active_turn_m0 {
            buf.wipe_with_reason(PamTurnAbortReason::TurnCompleted);
        }
    }

    pub fn cancel_turn_m0(&mut self) {
        if let Some(buf) = &mut self.active_turn_m0 {
            buf.wipe_with_reason(PamTurnAbortReason::UserCancelled);
        }
    }

    /// Immediate `SomaticImmediate/1` emergency wipe of Tier M0 transient memory.
    pub fn abort_somatic_immediate(&mut self) {
        if let Some(buf) = &mut self.active_turn_m0 {
            buf.wipe_with_reason(PamTurnAbortReason::SomaticImmediateV1);
        }
    }

    // --- Tier M1: WorkingSessionThread (`purpose = 4`) ---

    pub fn save_working_session_m1(
        &mut self,
        session: &WorkingSessionThread,
    ) -> Result<[u8; 32], PamError> {
        if session.turns.len() > MAX_WORKING_SESSION_TURNS {
            return Err(PamError::InvalidPreferences);
        }
        let binding = tier_m1_binding(self.owner_actor, session.thread_id);
        let plaintext = zeroize::Zeroizing::new(session.to_canonical_bytes()?);
        let sealed = self.vault.seal_local_metadata(binding, &plaintext)?;
        self.metadata_store
            .put_sealed(binding, sealed)
            .map_err(|e| PamError::Vault(VerifiedStoreError::Backend(e)))?;
        Ok(binding)
    }

    pub fn load_working_session_m1(
        &self,
        thread_id: [u8; 32],
    ) -> Result<Option<WorkingSessionThread>, PamError> {
        let binding = tier_m1_binding(self.owner_actor, thread_id);
        let Some(sealed) = self
            .metadata_store
            .get_sealed(&binding)
            .map_err(|e| PamError::Vault(VerifiedStoreError::Backend(e)))?
        else {
            return Ok(None);
        };
        let plaintext = zeroize::Zeroizing::new(self.vault.open_local_metadata(binding, &sealed)?);
        let session = WorkingSessionThread::from_canonical_bytes(&plaintext)?;
        Ok(Some(session))
    }

    /// Prepare sealed Tier M1 payload for same-actor Personal Mesh sync iff explicitly enabled.
    pub fn prepare_personal_mesh_sync_m1(
        &self,
        thread_id: [u8; 32],
        recipient_actor: ActorId,
    ) -> Result<Vec<u8>, PamError> {
        if recipient_actor != self.owner_actor {
            return Err(PamError::CrossActorSyncForbidden);
        }
        let session = self
            .load_working_session_m1(thread_id)?
            .ok_or(PamError::PersonalMeshSyncDisabled)?;
        if !session.personal_mesh_sync_enabled {
            return Err(PamError::PersonalMeshSyncDisabled);
        }
        let binding = tier_m1_binding(self.owner_actor, thread_id);
        let plaintext = zeroize::Zeroizing::new(session.to_canonical_bytes()?);
        self.vault
            .seal_local_metadata(binding, &plaintext)
            .map_err(Into::into)
    }

    // --- Tier M2: EpisodicObservationLinks (`purpose = 4` + PrivateVault objects) ---

    pub fn ingest_source_artifact_m2(
        &self,
        artifact: &SourceArtifact,
    ) -> Result<(ObjectCid, PutVerifiedOutcome), PamError> {
        let object = artifact.to_private_object()?;
        let (bytes, cid) = object
            .encode(ResourceProfile::ObjectV1)
            .map_err(ku_core::foundation::ObservationError::from)?;
        let outcome = self.vault.put_verified_object(
            cid,
            &bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(SOURCE_ARTIFACT_KIND, 1)],
            &[],
        )?;
        Ok((cid, outcome))
    }

    pub fn record_episodic_link_m2(
        &mut self,
        link: EpisodicObservationLink,
    ) -> Result<[u8; 32], PamError> {
        let mut links = self.load_episodic_links_m2()?;
        let link_id = link.link_id;
        links.record_immutable(link)?;
        let binding = tier_m2_binding(self.owner_actor);
        let plaintext = zeroize::Zeroizing::new(links.to_canonical_bytes()?);
        let sealed = self.vault.seal_local_metadata(binding, &plaintext)?;
        self.metadata_store
            .put_sealed(binding, sealed)
            .map_err(|e| PamError::Vault(VerifiedStoreError::Backend(e)))?;
        Ok(link_id)
    }

    pub fn load_episodic_links_m2(&self) -> Result<EpisodicObservationLinks, PamError> {
        let binding = tier_m2_binding(self.owner_actor);
        let Some(sealed) = self
            .metadata_store
            .get_sealed(&binding)
            .map_err(|e| PamError::Vault(VerifiedStoreError::Backend(e)))?
        else {
            return Ok(EpisodicObservationLinks::default());
        };
        let plaintext = zeroize::Zeroizing::new(self.vault.open_local_metadata(binding, &sealed)?);
        EpisodicObservationLinks::from_canonical_bytes(&plaintext)
    }

    /// Gate remote `RUN-003` delegation inputs: blocks `SomaticBiometricRaw` (`Tier M2`) and
    /// omits `SovereignActorPreferences` (`Tier M3`).
    pub fn prepare_remote_cognition_bundle(
        &self,
        episodic_link_ids: &[[u8; 32]],
        include_preferences: bool,
    ) -> Result<SanitizedRemoteCognitionBundle, PamError> {
        if include_preferences {
            return Err(PamError::PreferencesRemoteExportForbidden);
        }
        let links = self.load_episodic_links_m2()?;
        let mut allowed_draft_cids = Vec::with_capacity(episodic_link_ids.len());
        for id in episodic_link_ids {
            let link = links.get(id).ok_or(PamError::UnknownEpisodicLink)?;
            if !link.allows_remote_cognition_export() {
                return Err(PamError::SomaticBiometricRemoteExportForbidden);
            }
            allowed_draft_cids.push(link.semantic_draft_cid);
        }
        Ok(SanitizedRemoteCognitionBundle {
            allowed_draft_cids,
            contains_somatic_biometric_raw: false,
            contains_tier_m3_preferences: false,
        })
    }

    // --- Tier M3: SovereignActorPreferences (`purpose = 4`, never exported) ---

    pub fn save_preferences_m3(
        &mut self,
        preferences: &SovereignActorPreferences,
    ) -> Result<[u8; 32], PamError> {
        preferences.validate()?;
        let binding = tier_m3_binding(self.owner_actor);
        let plaintext = zeroize::Zeroizing::new(preferences.to_canonical_bytes()?);
        let sealed = self.vault.seal_local_metadata(binding, &plaintext)?;
        self.metadata_store
            .put_sealed(binding, sealed)
            .map_err(|e| PamError::Vault(VerifiedStoreError::Backend(e)))?;
        Ok(binding)
    }

    pub fn load_preferences_m3(&self) -> Result<SovereignActorPreferences, PamError> {
        let binding = tier_m3_binding(self.owner_actor);
        let Some(sealed) = self
            .metadata_store
            .get_sealed(&binding)
            .map_err(|e| PamError::Vault(VerifiedStoreError::Backend(e)))?
        else {
            return Ok(SovereignActorPreferences::default());
        };
        let plaintext = zeroize::Zeroizing::new(self.vault.open_local_metadata(binding, &sealed)?);
        SovereignActorPreferences::from_canonical_bytes(&plaintext)
    }

    // --- FW-06 (`NEG-POMV-001`): RAG Citation Exposure Firewall ---

    /// Record a KU retrieval/quote during PAM companion RAG. Emits ONLY a local
    /// `ExposureObservation` (`ExposureKind::CompanionCitation`) and NEVER a `UseEvidencePayload`.
    pub fn record_rag_citation(
        &mut self,
        target: ObjectReference,
        turn_id: [u8; 32],
    ) -> Result<ExposureRecordOutcome, PamError> {
        let seq = self.next_exposure_sequence;
        self.next_exposure_sequence = self.next_exposure_sequence.saturating_add(1);

        let mut ctx_hasher = blake3::Hasher::new();
        ctx_hasher.update(b"onebrain:vnext:pam-rag-context:1\0");
        ctx_hasher.update(self.owner_actor.as_bytes());
        ctx_hasher.update(&turn_id);
        let private_context_commitment = *ctx_hasher.finalize().as_bytes();

        let mut id_hasher = blake3::Hasher::new();
        id_hasher.update(b"onebrain:vnext:pam-rag-exposure-id:1\0");
        id_hasher.update(&private_context_commitment);
        id_hasher.update(&target.reference_kind.to_be_bytes());
        id_hasher.update(&target.cid);
        id_hasher.update(&seq.to_be_bytes());
        let observation_id = *id_hasher.finalize().as_bytes();

        let outcome = self.exposure_telemetry.record(ExposureObservation {
            observation_id,
            target: target.clone(),
            kind: ExposureKind::CompanionCitation.to_core_kind(),
            local_sequence: seq,
            private_context_commitment,
        });
        if outcome == ExposureRecordOutcome::Added {
            self.companion_exposures.push(PamCompanionExposure {
                observation_id,
                target,
                kind: ExposureKind::CompanionCitation,
                local_sequence: seq,
                private_context_commitment,
            });
        }
        Ok(outcome)
    }

    pub fn companion_exposures(&self) -> &[PamCompanionExposure] {
        &self.companion_exposures
    }

    pub const fn exposure_telemetry(&self) -> &ExposureTelemetry {
        &self.exposure_telemetry
    }

    pub const fn emitted_use_evidence_payload_count(&self) -> usize {
        0
    }

    // --- AI-004: Recommendation Card & User Confirmation Guard ---

    pub fn recommendation_guard(&self) -> &PamRecommendationGuard {
        &self.recommendation_guard
    }

    pub fn recommendation_guard_mut(&mut self) -> &mut PamRecommendationGuard {
        &mut self.recommendation_guard
    }
}

fn tier_m1_binding(actor: ActorId, thread_id: [u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:pam:tier-m1:1\0");
    hasher.update(actor.as_bytes());
    hasher.update(&thread_id);
    *hasher.finalize().as_bytes()
}

fn tier_m2_binding(actor: ActorId) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:pam:tier-m2:1\0");
    hasher.update(actor.as_bytes());
    *hasher.finalize().as_bytes()
}

fn tier_m3_binding(actor: ActorId) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:pam:tier-m3:1\0");
    hasher.update(actor.as_bytes());
    *hasher.finalize().as_bytes()
}

fn fixed_pam_bytes<const N: usize>(value: &CanonicalValue) -> Result<[u8; N], PamError> {
    let CanonicalValue::Bytes(bytes) = value else {
        return Err(PamError::InvalidPreferences);
    };
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| PamError::InvalidPreferences)
}

fn pam_obj_ref_to_value(reference: &ObjectReference) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(reference.reference_kind)),
        (1, CanonicalValue::Bytes(reference.cid.to_vec())),
    ])
}

fn pam_obj_ref_from_value(value: &CanonicalValue) -> Result<ObjectReference, PamError> {
    let CanonicalValue::Map(fields) = value else {
        return Err(PamError::InvalidPreferences);
    };
    if fields.len() != 2 || fields[0].0 != 0 || fields[1].0 != 1 {
        return Err(PamError::InvalidPreferences);
    }
    let CanonicalValue::Unsigned(kind) = fields[0].1 else {
        return Err(PamError::InvalidPreferences);
    };
    let cid = fixed_pam_bytes::<32>(&fields[1].1)?;
    Ok(ObjectReference::new(kind, cid))
}

fn disclosure_from_u64(code: u64) -> Result<DisclosureClass, PamError> {
    match code {
        0 => Ok(DisclosureClass::Public),
        1 => Ok(DisclosureClass::NegotiatedEncrypted),
        2 => Ok(DisclosureClass::RouteMinimal),
        3 => Ok(DisclosureClass::LocalOnly),
        _ => Err(PamError::InvalidPreferences),
    }
}

#[cfg(test)]
mod tests {
    use ku_core::foundation::{
        InMemoryVerifiedBackend, ObservationGovernance, SourceArtifactKind, VaultKey,
    };

    use super::*;

    fn test_runtime() -> PamVaultRuntime<InMemoryVerifiedBackend, InMemoryPamMetadataStore> {
        let vault = PrivateVault::new(
            InMemoryVerifiedBackend::default(),
            VaultKey::from_bytes([0x42; 32]),
        );
        PamVaultRuntime::new(ActorId::from_bytes([0x01; 32]), vault).unwrap()
    }

    fn governance() -> ObservationGovernance {
        ObservationGovernance {
            consent_policy: ObjectReference::new(0, [0x10; 32]),
            consent_receipt: ObjectReference::new(0, [0x11; 32]),
            revocation_policy: ObjectReference::new(0, [0x12; 32]),
            retention_policy: ObjectReference::new(0, [0x13; 32]),
            capture_scope_commitment: [0x14; 32],
            authorization_assessment_commitment: [0x15; 32],
            assessed_frontier: [0x16; 32],
        }
    }

    #[test]
    fn tier_m0_wipes_immediately_on_completion_cancel_and_somatic_immediate_abort() {
        let mut runtime = test_runtime();
        let secret = b"raw sensitive somatic prompt scratchpad";
        runtime.begin_turn_m0([0xAA; 32], secret);
        assert_eq!(
            runtime.active_turn_m0().unwrap().as_bytes().unwrap(),
            secret
        );
        assert!(!runtime.active_turn_m0().unwrap().is_persisted());

        runtime.abort_somatic_immediate();
        let m0 = runtime.active_turn_m0().unwrap();
        assert!(m0.is_wiped());
        assert!(m0.is_empty());
        assert_eq!(
            m0.last_wipe_reason(),
            Some(PamTurnAbortReason::SomaticImmediateV1)
        );
        assert_eq!(
            m0.last_wipe_reason().unwrap().profile_label(),
            SOMATIC_IMMEDIATE_ABORT_PROFILE
        );
        assert_eq!(m0.as_bytes(), Err(PamError::TurnBufferWiped));
        assert!(runtime.metadata_store().is_empty());
    }

    #[test]
    fn tier_m1_enforces_256_turn_window_purpose_4_sealing_and_default_off_mesh_sync() {
        let mut runtime = test_runtime();
        let thread_id = [0x22; 32];
        let mut session = WorkingSessionThread::new(thread_id);
        assert_eq!(session.disclosure, DisclosureClass::LocalOnly);
        assert!(!session.personal_mesh_sync_enabled);

        for idx in 0..300u64 {
            session.push_turn(
                SessionTurnSpeaker::User,
                format!("private-turn-marker-{idx}"),
                Vec::new(),
            );
        }
        assert_eq!(session.turns.len(), MAX_WORKING_SESSION_TURNS);
        assert_eq!(session.turns.front().unwrap().sequence, 44);
        assert_eq!(session.turns.back().unwrap().sequence, 299);

        let binding = runtime.save_working_session_m1(&session).unwrap();
        let ciphertext = runtime.metadata_store().raw_ciphertext(&binding).unwrap();
        assert!(!ciphertext
            .windows(b"private-turn-marker-299".len())
            .any(|w| w == b"private-turn-marker-299"));

        let loaded = runtime.load_working_session_m1(thread_id).unwrap().unwrap();
        assert_eq!(loaded, session);

        // Default-off personal mesh sync rejects export.
        assert_eq!(
            runtime.prepare_personal_mesh_sync_m1(thread_id, ActorId::from_bytes([0x01; 32])),
            Err(PamError::PersonalMeshSyncDisabled)
        );
        // Cross-actor sync is always rejected even if enabled.
        session.personal_mesh_sync_enabled = true;
        runtime.save_working_session_m1(&session).unwrap();
        assert_eq!(
            runtime.prepare_personal_mesh_sync_m1(thread_id, ActorId::from_bytes([0x02; 32])),
            Err(PamError::CrossActorSyncForbidden)
        );
        assert!(runtime
            .prepare_personal_mesh_sync_m1(thread_id, ActorId::from_bytes([0x01; 32]))
            .is_ok());
    }

    #[test]
    fn tier_m2_and_m3_block_somatic_biometrics_and_preferences_from_remote_cognition() {
        let mut runtime = test_runtime();
        let artifact = SourceArtifact {
            source_kind: SourceArtifactKind::Sensor,
            raw_bytes: b"hrv_rr_ms=812".to_vec(),
            media_type_commitment: [0x31; 32],
            capture_adapter: ObjectReference::new(0, [0x32; 32]),
            capture_sequence: 1,
            governance: governance(),
        };
        let (artifact_cid, _) = runtime.ingest_source_artifact_m2(&artifact).unwrap();
        let artifact_ref = ObjectReference::new(SOURCE_ARTIFACT_KIND.0, artifact_cid.into_bytes());

        let somatic_link = EpisodicObservationLink::new(
            EventCid::from_bytes([0x41; 32]),
            artifact_ref.clone(),
            [0x51; 32],
            ObservationModality::SomaticBiometricRaw,
        )
        .unwrap();
        let text_link = EpisodicObservationLink::new(
            EventCid::from_bytes([0x42; 32]),
            artifact_ref,
            [0x52; 32],
            ObservationModality::TextNote,
        )
        .unwrap();

        let somatic_id = runtime
            .record_episodic_link_m2(somatic_link.clone())
            .unwrap();
        let text_id = runtime.record_episodic_link_m2(text_link).unwrap();

        // Mutating an existing link_id fails closed (`ImmutableEpisodicLinkConflict`).
        let mut mutated = somatic_link;
        mutated.modality = ObservationModality::TextNote;
        assert_eq!(
            runtime.record_episodic_link_m2(mutated),
            Err(PamError::ImmutableEpisodicLinkConflict)
        );

        // SomaticBiometricRaw is blocked from remote RUN-003 delegation.
        assert_eq!(
            runtime.prepare_remote_cognition_bundle(&[somatic_id], false),
            Err(PamError::SomaticBiometricRemoteExportForbidden)
        );

        // Tier M3 preferences are stored under purpose=4 and blocked from remote RUN-003 export.
        let mut prefs = SovereignActorPreferences::default();
        prefs
            .vocabulary_preferences
            .insert("mass".to_string(), ConceptCcid::from_bytes([0x77; 16]));
        runtime.save_preferences_m3(&prefs).unwrap();
        assert_eq!(runtime.load_preferences_m3().unwrap(), prefs);
        assert!(!prefs.allows_remote_worker_export());
        assert_eq!(
            runtime.prepare_remote_cognition_bundle(&[text_id], true),
            Err(PamError::PreferencesRemoteExportForbidden)
        );

        let sanitized = runtime
            .prepare_remote_cognition_bundle(&[text_id], false)
            .unwrap();
        assert_eq!(sanitized.allowed_draft_cids, vec![[0x52; 32]]);
        assert!(!sanitized.contains_somatic_biometric_raw);
        assert!(!sanitized.contains_tier_m3_preferences);
    }

    #[test]
    fn fw06_rag_citation_records_local_companion_exposure_only_and_zero_use_evidence() {
        let mut runtime = test_runtime();
        let ku_ref = ObjectReference::new(2, [0x88; 32]);
        assert_eq!(
            runtime
                .record_rag_citation(ku_ref.clone(), [0x91; 32])
                .unwrap(),
            ExposureRecordOutcome::Added
        );
        assert_eq!(runtime.companion_exposures().len(), 1);
        assert_eq!(
            runtime.companion_exposures()[0].kind,
            ExposureKind::CompanionCitation
        );
        assert!(!runtime.companion_exposures()[0].is_use_evidence());
        assert!(runtime.exposure_telemetry().is_local_private());
        assert!(!runtime.exposure_telemetry().counts_as_use());
        assert_eq!(runtime.exposure_telemetry().local_count_for(&ku_ref), 1);
        assert_eq!(runtime.emitted_use_evidence_payload_count(), 0);
    }

    #[test]
    fn ai004_recommendation_guard_enforces_unassessed_label_and_user_confirmation() {
        let mut runtime = test_runtime();
        let card = PamRecommendationCard::new_unassessed(
            [0xC1; 32],
            PamRecommendationActionKind::SavePrivateKuDraft,
            "Save extracted thermodynamics claim to Private Vault",
            vec![ObjectReference::new(2, [0xD1; 32])],
            DisclosureClass::LocalOnly,
        )
        .unwrap();
        assert!(!card.performs_side_effect());
        assert!(!card.model_qualified);
        assert_eq!(card.epistemic_label, UNASSESSED_EPISTEMIC_LABEL);

        // Dishonest model qualification claim fails closed.
        let mut dishonest = card.clone();
        dishonest.model_qualified = true;
        assert_eq!(
            runtime.recommendation_guard_mut().stage_card(dishonest),
            Err(PamError::UnqualifiedModelClaimForbidden)
        );

        runtime.recommendation_guard_mut().stage_card(card).unwrap();
        assert!(!runtime.recommendation_guard().performs_side_effect());

        // Wrong actor cannot confirm.
        assert_eq!(
            runtime
                .recommendation_guard_mut()
                .confirm_with_user_gesture(UserConfirmationGesture {
                    actor: ActorId::from_bytes([0x99; 32]),
                    card_id: [0xC1; 32],
                    gesture_nonce: [0xE1; 32],
                    local_tick: 10,
                }),
            Err(PamError::InvalidUserConfirmation)
        );

        // Explicit owner gesture confirms once; replay fails closed.
        let confirmed = runtime
            .recommendation_guard_mut()
            .confirm_with_user_gesture(UserConfirmationGesture {
                actor: ActorId::from_bytes([0x01; 32]),
                card_id: [0xC1; 32],
                gesture_nonce: [0xE2; 32],
                local_tick: 11,
            })
            .unwrap();
        assert!(confirmed.performs_side_effect());
        assert_eq!(
            runtime
                .recommendation_guard_mut()
                .confirm_with_user_gesture(UserConfirmationGesture {
                    actor: ActorId::from_bytes([0x01; 32]),
                    card_id: [0xC1; 32],
                    gesture_nonce: [0xE2; 32],
                    local_tick: 12,
                }),
            Err(PamError::InvalidUserConfirmation)
        );
    }
}
