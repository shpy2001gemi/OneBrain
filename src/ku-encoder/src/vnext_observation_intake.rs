//! Offline-first private observation intake.
//!
//! Raw text/file/sensor bytes are persisted to the encrypted Private Vault
//! before local extraction. Successful extraction produces a signed private
//! ObservationEvent and a non-executable Receptor encoding proposal.

use ed25519_dalek::SigningKey;
use ku_core::foundation::event::EventError;
use ku_core::foundation::{
    decode_knowledge_event, decode_knowledge_object, AtomicVerifiedBackend, ConceptCcid, EventCid,
    InMemoryVerifiedBackend, KnowledgeEventEnvelope, KnownObjectKind, ObjectCid, ObjectReference,
    ObservationError, ObservationEventPayload, ObservationGovernance, PrivateVault,
    PutVerifiedOutcome, ReceptorAcceptanceProfile, ReceptorCardinality, ResourceProfile,
    SourceArtifact, SourceArtifactKind, SourceSpan, TypedConstraint, ValidatedFeedInception,
    ValidatedObservationEvent, VaultKey, VerifiedStoreError, OBSERVATION_EVENT_PAYLOAD_KIND,
    OBSERVATION_EVENT_TYPE, SOURCE_ARTIFACT_KIND,
};

use crate::vnext_receptor_encoder::{
    ConstraintCoverage, ReceptorEncodingDraft, ReceptorOriginDraft,
};

pub const MAX_OBSERVATION_CAUSAL_PARENTS: usize = 1_024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservationAuthorizationState {
    Authorized,
    Denied,
    Revoked,
    Unresolved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObservationAuthorization {
    pub state: ObservationAuthorizationState,
    pub assessment_commitment: [u8; 32],
    pub assessed_frontier: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservationCapture {
    pub source_kind: SourceArtifactKind,
    pub raw_bytes: Vec<u8>,
    pub media_type_commitment: [u8; 32],
    pub capture_adapter: ObjectReference,
    pub capture_sequence: u64,
    pub governance: ObservationGovernance,
    pub observed_frontier: [u8; 32],
    pub author_sequence: u64,
    pub event_idempotency_key: [u8; 32],
    pub causal_parents: Vec<EventCid>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ObservationSourceRange {
    pub start: u64,
    pub end: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservationReceptorDraft {
    pub role: Option<ConceptCcid>,
    pub expected_types: Vec<ConceptCcid>,
    pub hard_constraints: Vec<TypedConstraint>,
    pub cardinality: ReceptorCardinality,
    pub acceptance: Option<ReceptorAcceptanceProfile>,
    pub constraint_coverage: ConstraintCoverage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservationExtraction {
    pub observation_kind: ConceptCcid,
    pub source_ranges: Vec<ObservationSourceRange>,
    pub limitations: Vec<ConceptCcid>,
    pub receptor: ObservationReceptorDraft,
}

pub struct StoredSourceArtifactView<'a> {
    pub artifact_reference: ObjectReference,
    pub source_kind: SourceArtifactKind,
    pub raw_bytes: &'a [u8],
    pub capture_sequence: u64,
}

pub trait LocalObservationAdapter {
    fn extractor_reference(&self) -> ObjectReference;

    fn extract(
        &mut self,
        source: StoredSourceArtifactView<'_>,
    ) -> Result<ObservationExtraction, ObservationIntakeError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservationEncodingProposal {
    proposal_id: [u8; 32],
    pub source_artifact: ObjectCid,
    pub observation_payload: ObjectCid,
    pub observation_event: EventCid,
    pub governance: ObservationGovernance,
    draft: ReceptorEncodingDraft,
}

impl ObservationEncodingProposal {
    pub const fn proposal_id(&self) -> &[u8; 32] {
        &self.proposal_id
    }

    pub const fn draft(&self) -> &ReceptorEncodingDraft {
        &self.draft
    }

    pub const fn is_executable(&self) -> bool {
        false
    }

    pub const fn is_publication_instruction(&self) -> bool {
        false
    }

    pub const fn retains_consent_and_revocation_path(&self) -> bool {
        true
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservationIntakeOutcome {
    pub source_artifact: ObjectCid,
    pub source_store: PutVerifiedOutcome,
    pub observation_payload: ObjectCid,
    pub payload_store: PutVerifiedOutcome,
    pub observation_event: ValidatedObservationEvent,
    pub event_store: PutVerifiedOutcome,
    pub proposal: ObservationEncodingProposal,
}

impl ObservationIntakeOutcome {
    pub const fn auto_published(&self) -> bool {
        false
    }
}

pub struct LocalObservationIntake<B> {
    vault: PrivateVault<B>,
}

impl LocalObservationIntake<InMemoryVerifiedBackend> {
    pub fn in_memory(key: VaultKey) -> Self {
        Self::new(InMemoryVerifiedBackend::default(), key)
    }
}

impl<B: AtomicVerifiedBackend> LocalObservationIntake<B> {
    pub fn new(backend: B, key: VaultKey) -> Self {
        Self {
            vault: PrivateVault::new(backend, key),
        }
    }

    pub fn ingest(
        &self,
        capture: ObservationCapture,
        authorization: ObservationAuthorization,
        adapter: &mut dyn LocalObservationAdapter,
        author: &ValidatedFeedInception,
        signing_key: &SigningKey,
    ) -> Result<ObservationIntakeOutcome, ObservationIntakeError> {
        validate_authorization(&capture, authorization)?;
        if capture.observed_frontier == [0; 32]
            || capture.event_idempotency_key == [0; 32]
            || capture.causal_parents.len() > MAX_OBSERVATION_CAUSAL_PARENTS
        {
            return Err(ObservationIntakeError::InvalidCapture);
        }

        let artifact = SourceArtifact {
            source_kind: capture.source_kind,
            raw_bytes: capture.raw_bytes,
            media_type_commitment: capture.media_type_commitment,
            capture_adapter: capture.capture_adapter,
            capture_sequence: capture.capture_sequence,
            governance: capture.governance.clone(),
        };
        let artifact_object = artifact.to_private_object()?;
        let (artifact_bytes, artifact_cid) = artifact_object.encode(ResourceProfile::ObjectV1)?;
        let source_store = self.vault.put_verified_object(
            artifact_cid,
            &artifact_bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(SOURCE_ARTIFACT_KIND, 1)],
            &[],
        )?;
        require_accepted(source_store)?;
        let artifact_reference =
            ObjectReference::new(SOURCE_ARTIFACT_KIND.0, artifact_cid.into_bytes());

        let extractor = adapter.extractor_reference();
        if extractor.cid == [0; 32] {
            return Err(ObservationIntakeError::InvalidAdapter);
        }
        let extraction = adapter.extract(StoredSourceArtifactView {
            artifact_reference: artifact_reference.clone(),
            source_kind: artifact.source_kind,
            raw_bytes: &artifact.raw_bytes,
            capture_sequence: artifact.capture_sequence,
        })?;
        let source_spans = validate_ranges(
            extraction.source_ranges,
            &artifact_reference,
            artifact.raw_bytes.len(),
        )?;
        let mut limitations = extraction.limitations;
        limitations.sort_unstable();
        limitations.dedup();

        let payload = ObservationEventPayload {
            source_artifact: artifact_reference.clone(),
            observation_kind: extraction.observation_kind,
            source_spans: source_spans.clone(),
            extractor: extractor.clone(),
            limitations,
            governance: capture.governance.clone(),
            observed_frontier: capture.observed_frontier,
        };
        let payload_object = payload.to_private_object()?;
        let (payload_bytes, payload_cid) = payload_object.encode(ResourceProfile::ObjectV1)?;
        let payload_store = self.vault.put_verified_object(
            payload_cid,
            &payload_bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(OBSERVATION_EVENT_PAYLOAD_KIND, 1)],
            &[],
        )?;
        require_accepted(payload_store)?;

        let mut causal_parents = capture.causal_parents;
        causal_parents.sort_by_key(|event| event.into_bytes());
        causal_parents.dedup();
        let mut event = KnowledgeEventEnvelope::new(
            OBSERVATION_EVENT_TYPE,
            author.feed_id,
            capture.author_sequence,
            ku_core::foundation::DisclosureClass::LocalOnly,
            capture.event_idempotency_key,
        );
        event.payload_refs = vec![ObjectReference::new(0, payload_cid.into_bytes())];
        event.causal_parents = causal_parents;
        let (event_bytes, event_cid) = event.sign(author, signing_key)?.encode()?;
        let event_store = self.vault.put_verified_event(
            event_cid,
            &event_bytes,
            author,
            &[OBSERVATION_EVENT_TYPE],
        )?;
        require_accepted(event_store)?;

        let validated_payload = decode_knowledge_object(
            &payload_bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(OBSERVATION_EVENT_PAYLOAD_KIND, 1)],
            &[],
        )?;
        let validated_event =
            decode_knowledge_event(&event_bytes, author, &[OBSERVATION_EVENT_TYPE])?;
        let observation_event =
            ValidatedObservationEvent::bind(&validated_event, &validated_payload)?;

        let draft = ReceptorEncodingDraft {
            role: extraction.receptor.role,
            expected_types: extraction.receptor.expected_types,
            hard_constraints: extraction.receptor.hard_constraints,
            cardinality: extraction.receptor.cardinality,
            origin: ReceptorOriginDraft::Emergent {
                detector: extractor,
                observations: vec![artifact_reference],
                evidence_spans: source_spans,
            },
            acceptance: extraction.receptor.acceptance,
            constraint_coverage: extraction.receptor.constraint_coverage,
            requested_disclosure: None,
            declared_limitations: Vec::new(),
        };
        let proposal_id = proposal_id(event_cid, artifact_cid, payload_cid);
        let proposal = ObservationEncodingProposal {
            proposal_id,
            source_artifact: artifact_cid,
            observation_payload: payload_cid,
            observation_event: event_cid,
            governance: capture.governance,
            draft,
        };
        Ok(ObservationIntakeOutcome {
            source_artifact: artifact_cid,
            source_store,
            observation_payload: payload_cid,
            payload_store,
            observation_event,
            event_store,
            proposal,
        })
    }

    pub fn load_private_object(
        &self,
        cid: ObjectCid,
    ) -> Result<Option<Vec<u8>>, ObservationIntakeError> {
        self.vault.get_object(cid).map_err(Into::into)
    }

    pub const fn is_offline_capable(&self) -> bool {
        true
    }

    pub const fn has_publication_path(&self) -> bool {
        false
    }
}

fn validate_authorization(
    capture: &ObservationCapture,
    authorization: ObservationAuthorization,
) -> Result<(), ObservationIntakeError> {
    capture.governance.validate()?;
    if authorization.state != ObservationAuthorizationState::Authorized {
        return Err(ObservationIntakeError::CaptureNotAuthorized(
            authorization.state,
        ));
    }
    if authorization.assessment_commitment == [0; 32]
        || authorization.assessed_frontier == [0; 32]
        || authorization.assessment_commitment
            != capture.governance.authorization_assessment_commitment
        || authorization.assessed_frontier != capture.governance.assessed_frontier
    {
        return Err(ObservationIntakeError::AuthorizationBindingMismatch);
    }
    Ok(())
}

fn validate_ranges(
    ranges: Vec<ObservationSourceRange>,
    artifact: &ObjectReference,
    raw_len: usize,
) -> Result<Vec<SourceSpan>, ObservationIntakeError> {
    if ranges.is_empty() {
        return Err(ObservationIntakeError::InvalidSourceRange);
    }
    let raw_len = u64::try_from(raw_len).map_err(|_| ObservationIntakeError::InvalidSourceRange)?;
    let mut ranges = ranges;
    ranges.sort_unstable();
    ranges.dedup();
    if ranges
        .iter()
        .any(|range| range.start >= range.end || range.end > raw_len)
    {
        return Err(ObservationIntakeError::InvalidSourceRange);
    }
    Ok(ranges
        .into_iter()
        .map(|range| SourceSpan {
            source: artifact.clone(),
            start: range.start,
            end: range.end,
        })
        .collect())
}

fn require_accepted(outcome: PutVerifiedOutcome) -> Result<(), ObservationIntakeError> {
    match outcome {
        PutVerifiedOutcome::Stored | PutVerifiedOutcome::AlreadyPresent => Ok(()),
        PutVerifiedOutcome::Quarantined { .. } => Err(ObservationIntakeError::VaultQuarantined),
    }
}

fn proposal_id(event: EventCid, artifact: ObjectCid, payload: ObjectCid) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:observation-encoding-proposal:1\0");
    hasher.update(event.as_bytes());
    hasher.update(artifact.as_bytes());
    hasher.update(payload.as_bytes());
    *hasher.finalize().as_bytes()
}

#[derive(Debug, PartialEq, Eq)]
pub enum ObservationIntakeError {
    Observation(ObservationError),
    Object(ku_core::foundation::ObjectError),
    Event(EventError),
    Store(VerifiedStoreError),
    CaptureNotAuthorized(ObservationAuthorizationState),
    AuthorizationBindingMismatch,
    InvalidCapture,
    InvalidAdapter,
    InvalidSourceRange,
    VaultQuarantined,
    Adapter(&'static str),
}

impl From<ObservationError> for ObservationIntakeError {
    fn from(error: ObservationError) -> Self {
        Self::Observation(error)
    }
}

impl From<ku_core::foundation::ObjectError> for ObservationIntakeError {
    fn from(error: ku_core::foundation::ObjectError) -> Self {
        Self::Object(error)
    }
}

impl From<EventError> for ObservationIntakeError {
    fn from(error: EventError) -> Self {
        Self::Event(error)
    }
}

impl From<VerifiedStoreError> for ObservationIntakeError {
    fn from(error: VerifiedStoreError) -> Self {
        Self::Store(error)
    }
}

#[cfg(test)]
mod tests {
    use ku_core::foundation::{
        decode_feed_inception, ConceptCcid, DeviceId, DisclosureClass, FeedInception,
        NamespaceCommitment, SignedFeedInception, UnknownConstraintPolicy,
    };

    use super::*;
    use crate::vnext_receptor_encoder::{ReceptorEncoder, ReceptorEncodingOutcome};

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn governance() -> ObservationGovernance {
        ObservationGovernance {
            consent_policy: reference(1),
            consent_receipt: reference(2),
            revocation_policy: reference(3),
            retention_policy: reference(4),
            capture_scope_commitment: [5; 32],
            authorization_assessment_commitment: [6; 32],
            assessed_frontier: [7; 32],
        }
    }

    fn authorization(state: ObservationAuthorizationState) -> ObservationAuthorization {
        ObservationAuthorization {
            state,
            assessment_commitment: [6; 32],
            assessed_frontier: [7; 32],
        }
    }

    fn capture(kind: SourceArtifactKind) -> ObservationCapture {
        ObservationCapture {
            source_kind: kind,
            raw_bytes: b"wheel moment anomaly".to_vec(),
            media_type_commitment: [8; 32],
            capture_adapter: reference(9),
            capture_sequence: 11,
            governance: governance(),
            observed_frontier: [10; 32],
            author_sequence: 0,
            event_idempotency_key: [11; 32],
            causal_parents: Vec::new(),
        }
    }

    fn author() -> (ValidatedFeedInception, SigningKey) {
        let key = SigningKey::from_bytes(&[12; 32]);
        let inception = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"observation-intake-test", [13; 32]).unwrap(),
            0,
            DeviceId::from_bytes([14; 32]),
        );
        let signed: SignedFeedInception = inception.sign(&key).unwrap();
        (
            decode_feed_inception(&signed.encode().unwrap()).unwrap(),
            key,
        )
    }

    struct Adapter {
        calls: usize,
        range: ObservationSourceRange,
    }

    impl LocalObservationAdapter for Adapter {
        fn extractor_reference(&self) -> ObjectReference {
            reference(15)
        }

        fn extract(
            &mut self,
            source: StoredSourceArtifactView<'_>,
        ) -> Result<ObservationExtraction, ObservationIntakeError> {
            self.calls += 1;
            assert_eq!(source.raw_bytes, b"wheel moment anomaly");
            Ok(ObservationExtraction {
                observation_kind: ConceptCcid::from_bytes([16; 16]),
                source_ranges: vec![self.range],
                limitations: vec![ConceptCcid::from_bytes([17; 16])],
                receptor: ObservationReceptorDraft {
                    role: Some(ConceptCcid::from_bytes([18; 16])),
                    expected_types: vec![ConceptCcid::from_bytes([19; 16])],
                    hard_constraints: Vec::new(),
                    cardinality: ReceptorCardinality::new(1, Some(1)).unwrap(),
                    acceptance: Some(ReceptorAcceptanceProfile {
                        policy: reference(20),
                        required_evidence_kinds: Vec::new(),
                        unknown_constraint_policy: UnknownConstraintPolicy::KeepUnresolved,
                    }),
                    constraint_coverage: ConstraintCoverage::CompleteForSource,
                },
            })
        }
    }

    #[test]
    fn raw_source_and_signed_observation_stay_private_with_full_provenance() {
        let intake = LocalObservationIntake::in_memory(VaultKey::from_bytes([21; 32]));
        let (author, key) = author();
        let mut adapter = Adapter {
            calls: 0,
            range: ObservationSourceRange { start: 0, end: 12 },
        };
        let outcome = intake
            .ingest(
                capture(SourceArtifactKind::Text),
                authorization(ObservationAuthorizationState::Authorized),
                &mut adapter,
                &author,
                &key,
            )
            .unwrap();
        assert_eq!(adapter.calls, 1);
        let source_bytes = intake
            .load_private_object(outcome.source_artifact)
            .unwrap()
            .unwrap();
        let source = decode_knowledge_object(
            &source_bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(SOURCE_ARTIFACT_KIND, 1)],
            &[],
        )
        .unwrap();
        let source = SourceArtifact::from_validated(&source).unwrap();
        assert_eq!(source.raw_bytes, b"wheel moment anomaly");
        assert_eq!(
            source.to_private_object().unwrap().disclosure,
            DisclosureClass::LocalOnly
        );
        assert_eq!(
            outcome.observation_event.payload().source_spans[0]
                .source
                .cid,
            outcome.source_artifact.into_bytes()
        );
        assert!(outcome.proposal.retains_consent_and_revocation_path());
        assert!(!outcome.auto_published());
        assert!(!intake.has_publication_path());

        let encoded = ReceptorEncoder
            .encode(outcome.proposal.draft().clone())
            .unwrap();
        let ReceptorEncodingOutcome::Encoded(encoded) = encoded else {
            panic!("complete proposal should encode locally");
        };
        assert_eq!(encoded.disclosure(), DisclosureClass::LocalOnly);
        assert_eq!(encoded.trace().source_spans()[0].start, 0);
        assert_eq!(
            encoded.trace().source_spans()[0].source.cid,
            outcome.source_artifact.into_bytes()
        );
    }

    #[test]
    fn denied_revoked_and_unresolved_consent_fail_before_adapter() {
        let intake = LocalObservationIntake::in_memory(VaultKey::from_bytes([21; 32]));
        let (author, key) = author();
        for state in [
            ObservationAuthorizationState::Denied,
            ObservationAuthorizationState::Revoked,
            ObservationAuthorizationState::Unresolved,
        ] {
            let mut adapter = Adapter {
                calls: 0,
                range: ObservationSourceRange { start: 0, end: 1 },
            };
            assert_eq!(
                intake
                    .ingest(
                        capture(SourceArtifactKind::Sensor),
                        authorization(state),
                        &mut adapter,
                        &author,
                        &key
                    )
                    .unwrap_err(),
                ObservationIntakeError::CaptureNotAuthorized(state)
            );
            assert_eq!(adapter.calls, 0);
        }
    }

    #[test]
    fn adapter_cannot_claim_a_span_outside_raw_source() {
        let intake = LocalObservationIntake::in_memory(VaultKey::from_bytes([21; 32]));
        let (author, key) = author();
        let mut adapter = Adapter {
            calls: 0,
            range: ObservationSourceRange {
                start: 0,
                end: 1000,
            },
        };
        assert_eq!(
            intake
                .ingest(
                    capture(SourceArtifactKind::File),
                    authorization(ObservationAuthorizationState::Authorized),
                    &mut adapter,
                    &author,
                    &key
                )
                .unwrap_err(),
            ObservationIntakeError::InvalidSourceRange
        );
    }

    #[test]
    fn text_file_and_sensor_use_the_same_offline_private_boundary() {
        let (author, key) = author();
        for (offset, kind) in [
            SourceArtifactKind::Text,
            SourceArtifactKind::File,
            SourceArtifactKind::Sensor,
        ]
        .into_iter()
        .enumerate()
        {
            let intake =
                LocalObservationIntake::in_memory(VaultKey::from_bytes([30 + offset as u8; 32]));
            let mut capture = capture(kind);
            capture.event_idempotency_key = [31 + offset as u8; 32];
            let mut adapter = Adapter {
                calls: 0,
                range: ObservationSourceRange { start: 0, end: 5 },
            };
            let outcome = intake
                .ingest(
                    capture,
                    authorization(ObservationAuthorizationState::Authorized),
                    &mut adapter,
                    &author,
                    &key,
                )
                .unwrap();
            assert!(!outcome.proposal.is_executable());
            assert!(!outcome.proposal.is_publication_instruction());
            assert!(intake.is_offline_capable());
        }
    }
}
