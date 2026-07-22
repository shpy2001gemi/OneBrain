//! Blind commit-before-reveal encoding-fidelity workflow.

use std::collections::{BTreeMap, BTreeSet};

use ku_core::foundation::{
    canonicalize_set_by_key, encode_canonical, CanonicalValue, ConceptCcid, CorrelationDimension,
    CorrelationEvidence, DisclosureClass, EncodingAttempt, EncodingAttemptRole,
    EncodingFidelityAttestation, EvidenceStrength, FidelityCheck, FidelityCheckKind,
    FidelityCheckStatus, FidelityError, FidelityPolicy, ObjectCid, ObjectReference,
    ResourceProfile, ValidatedKnowledgeObject,
};

use crate::vnext_executor::{
    cognitive_output_commitment, CognitiveExecutionResult, CognitiveTermination,
};

pub const MAX_CHECK_MEMBERS: usize = 16_384;

type SourceKey = (u64, [u8; 32]);
type TargetKey = (u64, [u8; 32], u64, [u8; 32]);
type CanonicalAlternateSet = BTreeMap<[u8; 32], Vec<u8>>;
type PortfolioSet = BTreeMap<[u8; 32], PortfolioEntry>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlindEncodingRequest {
    pub session_id: [u8; 32],
    pub source_artifact: ObjectReference,
    pub source_input_commitment: [u8; 32],
    pub blind_session_commitment: [u8; 32],
    pub challenge_nonce_commitment: [u8; 32],
    pub policy_ref: ObjectReference,
}

impl BlindEncodingRequest {
    pub const fn contains_candidate_target(&self) -> bool {
        false
    }

    fn validate(&self) -> Result<(), BlindFidelityError> {
        if self.session_id == [0; 32]
            || self.source_input_commitment == [0; 32]
            || self.blind_session_commitment == [0; 32]
            || self.challenge_nonce_commitment == [0; 32]
        {
            Err(BlindFidelityError::InvalidRequest)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlindSessionPhase {
    AwaitingAttemptCommit,
    AwaitingTargetReveal,
    ReadyForChecks,
    Completed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodingAttemptArtifact {
    pub attempt: EncodingAttempt,
    pub object_cid: ObjectCid,
    pub object_bytes: Vec<u8>,
}

impl EncodingAttemptArtifact {
    pub fn as_reference(&self) -> ObjectReference {
        ObjectReference::new(0, self.object_cid.into_bytes())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FidelityCheckPlan {
    pub expected_source_span_commitments: Vec<[u8; 32]>,
    pub expected_gene_selection_commitments: Vec<[u8; 32]>,
    pub expected_concept_ccids: Vec<ConceptCcid>,
    pub source_span_evidence_ref: ObjectReference,
    pub gene_selection_evidence_ref: ObjectReference,
    pub concept_selection_evidence_ref: ObjectReference,
}

impl FidelityCheckPlan {
    fn validate(&self) -> Result<(), BlindFidelityError> {
        if invalid_set(&self.expected_source_span_commitments)
            || invalid_set(&self.expected_gene_selection_commitments)
            || invalid_set(&self.expected_concept_ccids)
        {
            Err(BlindFidelityError::InvalidCheckPlan)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateEncodingInspection {
    pub candidate_encoding: ObjectReference,
    pub source_span_commitments: Vec<[u8; 32]>,
    pub gene_selection_commitments: Vec<[u8; 32]>,
    pub concept_ccids: Vec<ConceptCcid>,
    pub source_span_inspection_complete: bool,
    pub gene_inspection_complete: bool,
    pub concept_inspection_complete: bool,
}

impl CandidateEncodingInspection {
    fn validate(&self) -> Result<(), BlindFidelityError> {
        if self.source_span_commitments.len() > MAX_CHECK_MEMBERS
            || self.gene_selection_commitments.len() > MAX_CHECK_MEMBERS
            || self.concept_ccids.len() > MAX_CHECK_MEMBERS
            || has_duplicates(&self.source_span_commitments)
            || has_duplicates(&self.gene_selection_commitments)
            || has_duplicates(&self.concept_ccids)
        {
            Err(BlindFidelityError::InvalidInspection)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletedBlindAttestation {
    pub attempt_artifact: EncodingAttemptArtifact,
    pub attestation: EncodingFidelityAttestation,
    pub attestation_cid: ObjectCid,
    pub attestation_bytes: Vec<u8>,
}

impl CompletedBlindAttestation {
    pub const fn establishes_cognitive_independence(&self) -> bool {
        false
    }

    pub const fn establishes_proposition_truth(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug)]
struct BlindSession {
    request: BlindEncodingRequest,
    phase: BlindSessionPhase,
    attempt: Option<EncodingAttemptArtifact>,
    correlation: Option<CorrelationEvidence>,
    revealed_candidate: Option<ObjectReference>,
}

#[derive(Default)]
pub struct BlindEncodingCoordinator {
    sessions: BTreeMap<[u8; 32], BlindSession>,
}

impl BlindEncodingCoordinator {
    pub fn open(&mut self, request: BlindEncodingRequest) -> Result<(), BlindFidelityError> {
        request.validate()?;
        if self.sessions.contains_key(&request.session_id) {
            return Err(BlindFidelityError::SessionAlreadyExists);
        }
        self.sessions.insert(
            request.session_id,
            BlindSession {
                request,
                phase: BlindSessionPhase::AwaitingAttemptCommit,
                attempt: None,
                correlation: None,
                revealed_candidate: None,
            },
        );
        Ok(())
    }

    pub fn request(&self, session_id: [u8; 32]) -> Option<&BlindEncodingRequest> {
        self.sessions
            .get(&session_id)
            .map(|session| &session.request)
    }

    pub fn phase(&self, session_id: [u8; 32]) -> Option<BlindSessionPhase> {
        self.sessions.get(&session_id).map(|session| session.phase)
    }

    pub fn reveal_candidate(
        &mut self,
        session_id: [u8; 32],
        candidate: ObjectReference,
    ) -> Result<(), BlindFidelityError> {
        let session = self
            .sessions
            .get_mut(&session_id)
            .ok_or(BlindFidelityError::UnknownSession)?;
        if session.phase != BlindSessionPhase::AwaitingTargetReveal {
            return Err(BlindFidelityError::RevealBeforeCommit);
        }
        session.revealed_candidate = Some(candidate);
        session.phase = BlindSessionPhase::ReadyForChecks;
        Ok(())
    }

    pub fn commit_external_attempt(
        &mut self,
        session_id: [u8; 32],
        execution: &CognitiveExecutionResult,
        pipeline_model_tool_commitments: Vec<[u8; 32]>,
        source_acquisition_or_derivation_commitment: [u8; 32],
        correlation: CorrelationEvidence,
    ) -> Result<EncodingAttemptArtifact, BlindFidelityError> {
        let session = self
            .sessions
            .get_mut(&session_id)
            .ok_or(BlindFidelityError::UnknownSession)?;
        if session.phase != BlindSessionPhase::AwaitingAttemptCommit {
            return Err(BlindFidelityError::AttemptAlreadyCommitted);
        }
        if execution.termination != CognitiveTermination::Completed
            || execution.output_commitment != cognitive_output_commitment(&execution.output)
            || !execution
                .record
                .input_commitments
                .contains(&session.request.source_input_commitment)
        {
            return Err(BlindFidelityError::UnusableExecution);
        }
        execution.record.canonical_body()?;
        validate_correlation_binding(&session.request, &correlation)?;
        let attempt = EncodingAttempt {
            role: EncodingAttemptRole::ExternalBlind,
            source_artifact: session.request.source_artifact.clone(),
            candidate_encoding: None,
            output_commitment: execution.output_commitment,
            pipeline_model_tool_commitments,
            source_acquisition_or_derivation_commitment,
            execution_record_ref: execution_record_reference(execution)?,
            blind_session_commitment: Some(session.request.blind_session_commitment),
            challenge_nonce_commitment: Some(session.request.challenge_nonce_commitment),
        };
        let object = attempt.to_knowledge_object(DisclosureClass::NegotiatedEncrypted)?;
        let (object_bytes, object_cid) = object.encode(ResourceProfile::ObjectV1)?;
        let artifact = EncodingAttemptArtifact {
            attempt,
            object_cid,
            object_bytes,
        };
        session.attempt = Some(artifact.clone());
        session.correlation = Some(correlation);
        session.phase = BlindSessionPhase::AwaitingTargetReveal;
        Ok(artifact)
    }

    pub fn finish_checks(
        &mut self,
        session_id: [u8; 32],
        plan: &FidelityCheckPlan,
        inspection: &CandidateEncodingInspection,
        limitations: Vec<ConceptCcid>,
    ) -> Result<CompletedBlindAttestation, BlindFidelityError> {
        plan.validate()?;
        inspection.validate()?;
        let session = self
            .sessions
            .get_mut(&session_id)
            .ok_or(BlindFidelityError::UnknownSession)?;
        if session.phase != BlindSessionPhase::ReadyForChecks {
            return Err(BlindFidelityError::ChecksBeforeReveal);
        }
        if session.revealed_candidate.as_ref() != Some(&inspection.candidate_encoding) {
            return Err(BlindFidelityError::CandidateMismatch);
        }
        let attempt = session
            .attempt
            .clone()
            .ok_or(BlindFidelityError::AttemptNotCommitted)?;
        let correlation = session
            .correlation
            .clone()
            .ok_or(BlindFidelityError::AttemptNotCommitted)?;
        let checks = exact_fidelity_checks(plan, inspection)?;
        let attestation = EncodingFidelityAttestation {
            source_artifact: session.request.source_artifact.clone(),
            candidate_encoding: inspection.candidate_encoding.clone(),
            blind_attempt_output_commitment: attempt.attempt.output_commitment,
            attempt_ref: attempt.as_reference(),
            execution_record_ref: attempt.attempt.execution_record_ref.clone(),
            correlation_evidence: correlation,
            checks,
            limitations,
            policy_ref: session.request.policy_ref.clone(),
        };
        let object = attestation.to_knowledge_object(DisclosureClass::NegotiatedEncrypted)?;
        let (attestation_bytes, attestation_cid) = object.encode(ResourceProfile::ObjectV1)?;
        session.phase = BlindSessionPhase::Completed;
        Ok(CompletedBlindAttestation {
            attempt_artifact: attempt,
            attestation,
            attestation_cid,
            attestation_bytes,
        })
    }
}

pub fn exact_fidelity_checks(
    plan: &FidelityCheckPlan,
    inspection: &CandidateEncodingInspection,
) -> Result<Vec<FidelityCheck>, BlindFidelityError> {
    plan.validate()?;
    inspection.validate()?;
    Ok(vec![
        exact_check(
            FidelityCheckKind::SourceSpanAlignment,
            &plan.expected_source_span_commitments,
            &inspection.source_span_commitments,
            inspection.source_span_inspection_complete,
            plan.source_span_evidence_ref.clone(),
        )?,
        exact_check(
            FidelityCheckKind::GeneSelection,
            &plan.expected_gene_selection_commitments,
            &inspection.gene_selection_commitments,
            inspection.gene_inspection_complete,
            plan.gene_selection_evidence_ref.clone(),
        )?,
        exact_check(
            FidelityCheckKind::ConceptSelection,
            &plan.expected_concept_ccids,
            &inspection.concept_ccids,
            inspection.concept_inspection_complete,
            plan.concept_selection_evidence_ref.clone(),
        )?,
    ])
}

fn exact_check<T>(
    kind: FidelityCheckKind,
    expected: &[T],
    observed: &[T],
    complete: bool,
    evidence_ref: ObjectReference,
) -> Result<FidelityCheck, BlindFidelityError>
where
    T: Ord + Copy + IntoCheckValue,
{
    let status = if !complete {
        FidelityCheckStatus::Unresolved
    } else if as_set(expected) == as_set(observed) {
        FidelityCheckStatus::ConsistentWithSource
    } else {
        FidelityCheckStatus::HardEncodingMismatch
    };
    let checked_region_commitment = check_region_commitment(kind, expected, observed)?;
    Ok(FidelityCheck {
        kind,
        status,
        checked_region_commitment,
        evidence_ref: Some(evidence_ref),
    })
}

trait IntoCheckValue {
    fn check_value(self) -> CanonicalValue;
}

impl IntoCheckValue for [u8; 32] {
    fn check_value(self) -> CanonicalValue {
        CanonicalValue::Bytes(self.to_vec())
    }
}

impl IntoCheckValue for ConceptCcid {
    fn check_value(self) -> CanonicalValue {
        CanonicalValue::Bytes(self.as_bytes().to_vec())
    }
}

fn check_region_commitment<T: Copy + IntoCheckValue>(
    kind: FidelityCheckKind,
    expected: &[T],
    observed: &[T],
) -> Result<[u8; 32], BlindFidelityError> {
    let expected = expected
        .iter()
        .copied()
        .map(IntoCheckValue::check_value)
        .collect::<Vec<_>>();
    let observed = observed
        .iter()
        .copied()
        .map(IntoCheckValue::check_value)
        .collect::<Vec<_>>();
    let expected = canonicalize_set_by_key(
        expected
            .into_iter()
            .map(|value| (value.clone(), value))
            .collect(),
        ResourceProfile::ObjectV1,
    )?;
    let observed = canonicalize_set_by_key(
        observed
            .into_iter()
            .map(|value| (value.clone(), value))
            .collect(),
        ResourceProfile::ObjectV1,
    )?;
    let bytes = encode_canonical(
        &CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(kind as u64)),
            (1, CanonicalValue::Array(expected)),
            (2, CanonicalValue::Array(observed)),
        ]),
        ResourceProfile::ObjectV1,
    )?;
    Ok(domain_commitment(
        b"onebrain:vnext:fidelity-checked-region:1\0",
        &bytes,
    ))
}

fn validate_correlation_binding(
    request: &BlindEncodingRequest,
    correlation: &CorrelationEvidence,
) -> Result<(), BlindFidelityError> {
    correlation.canonical_value()?;
    for (dimension, expected) in [
        (
            CorrelationDimension::BlindSession,
            request.blind_session_commitment,
        ),
        (
            CorrelationDimension::ChallengeNonce,
            request.challenge_nonce_commitment,
        ),
    ] {
        let evidence = correlation
            .dimension(dimension)
            .ok_or(BlindFidelityError::CorrelationBindingMismatch)?;
        if evidence.value_commitment != Some(expected)
            || !matches!(
                evidence.strength,
                EvidenceStrength::CryptoBound | EvidenceStrength::ExternallyAttested
            )
        {
            return Err(BlindFidelityError::CorrelationBindingMismatch);
        }
    }
    Ok(())
}

pub fn execution_record_reference(
    execution: &CognitiveExecutionResult,
) -> Result<ObjectReference, BlindFidelityError> {
    let bytes = encode_canonical(
        &execution.record.canonical_body()?,
        ResourceProfile::ObjectV1,
    )?;
    Ok(ObjectReference::new(
        1,
        domain_commitment(b"onebrain:vnext:cognitive-execution-record:1\0", &bytes),
    ))
}

fn domain_commitment(domain: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(&(bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

fn invalid_set<T: Ord + Copy>(values: &[T]) -> bool {
    values.is_empty() || values.len() > MAX_CHECK_MEMBERS || has_duplicates(values)
}

fn has_duplicates<T: Ord + Copy>(values: &[T]) -> bool {
    values.iter().copied().collect::<BTreeSet<_>>().len() != values.len()
}

fn as_set<T: Ord + Copy>(values: &[T]) -> BTreeSet<T> {
    values.iter().copied().collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlternateArchiveOutcome {
    Preserved,
    ExactReplay,
}

#[derive(Default)]
pub struct AlternateEncodingArchive {
    by_source: BTreeMap<SourceKey, CanonicalAlternateSet>,
}

impl AlternateEncodingArchive {
    pub fn preserve(
        &mut self,
        source: &ObjectReference,
        alternate: &ValidatedKnowledgeObject,
    ) -> Result<AlternateArchiveOutcome, BlindFidelityError> {
        let candidates = self
            .by_source
            .entry((source.reference_kind, source.cid))
            .or_default();
        let key = alternate.cid().into_bytes();
        match candidates.get(&key) {
            Some(bytes) if bytes == alternate.original_bytes() => {
                Ok(AlternateArchiveOutcome::ExactReplay)
            }
            Some(_) => Err(BlindFidelityError::AlternateIdentityConflict),
            None => {
                candidates.insert(key, alternate.original_bytes().to_vec());
                Ok(AlternateArchiveOutcome::Preserved)
            }
        }
    }

    pub fn alternate_count(&self, source: &ObjectReference) -> usize {
        self.by_source
            .get(&(source.reference_kind, source.cid))
            .map(BTreeMap::len)
            .unwrap_or(0)
    }

    pub const fn selects_winner_or_deletes_alternates(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PortfolioEntry {
    attempt_key: SourceKey,
    group_key: Option<[u8; 32]>,
    hard_mismatch: bool,
}

#[derive(Default)]
pub struct BlindAttemptPortfolio {
    entries: BTreeMap<TargetKey, PortfolioSet>,
}

impl BlindAttemptPortfolio {
    pub fn record(
        &mut self,
        completed: &CompletedBlindAttestation,
        policy: &FidelityPolicy,
    ) -> Result<(), BlindFidelityError> {
        policy.canonical_payload()?;
        let attestation = &completed.attestation;
        let target = (
            attestation.source_artifact.reference_kind,
            attestation.source_artifact.cid,
            attestation.candidate_encoding.reference_kind,
            attestation.candidate_encoding.cid,
        );
        self.entries.entry(target).or_default().insert(
            completed.attestation_cid.into_bytes(),
            PortfolioEntry {
                attempt_key: (
                    attestation.attempt_ref.reference_kind,
                    attestation.attempt_ref.cid,
                ),
                group_key: policy.evidenced_group_key(&attestation.correlation_evidence)?,
                hard_mismatch: attestation.has_hard_encoding_mismatch(),
            },
        );
        Ok(())
    }

    pub fn meets_external_contract(
        &self,
        source: &ObjectReference,
        candidate: &ObjectReference,
        policy: &FidelityPolicy,
    ) -> bool {
        let entries = self
            .entries
            .get(&(
                source.reference_kind,
                source.cid,
                candidate.reference_kind,
                candidate.cid,
            ))
            .into_iter()
            .flat_map(BTreeMap::values)
            .collect::<Vec<_>>();
        let mut attempts: BTreeMap<SourceKey, (bool, BTreeSet<[u8; 32]>)> = BTreeMap::new();
        for entry in entries {
            let state = attempts.entry(entry.attempt_key).or_default();
            state.0 |= entry.hard_mismatch;
            state.1.extend(entry.group_key);
        }
        let eligible = attempts
            .values()
            .filter(|(hard_mismatch, groups)| !hard_mismatch && groups.len() == 1)
            .collect::<Vec<_>>();
        let groups = eligible
            .iter()
            .flat_map(|(_, groups)| groups.iter().copied())
            .collect::<BTreeSet<_>>();
        eligible.len() >= usize::from(policy.minimum_external_blind_attempts)
            && groups.len() >= usize::from(policy.minimum_evidenced_distinct_external_groups)
    }

    pub const fn establishes_cognitive_independence(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlindFidelityError {
    InvalidRequest,
    InvalidCheckPlan,
    InvalidInspection,
    SessionAlreadyExists,
    UnknownSession,
    RevealBeforeCommit,
    AttemptAlreadyCommitted,
    AttemptNotCommitted,
    ChecksBeforeReveal,
    CandidateMismatch,
    UnusableExecution,
    CorrelationBindingMismatch,
    AlternateIdentityConflict,
    Canonical(ku_core::foundation::CanonicalError),
    Capability(ku_core::foundation::CapabilityError),
    Fidelity(FidelityError),
    Object(ku_core::foundation::ObjectError),
}

impl From<ku_core::foundation::CanonicalError> for BlindFidelityError {
    fn from(error: ku_core::foundation::CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<ku_core::foundation::CapabilityError> for BlindFidelityError {
    fn from(error: ku_core::foundation::CapabilityError) -> Self {
        Self::Capability(error)
    }
}

impl From<FidelityError> for BlindFidelityError {
    fn from(error: FidelityError) -> Self {
        Self::Fidelity(error)
    }
}

impl From<ku_core::foundation::ObjectError> for BlindFidelityError {
    fn from(error: ku_core::foundation::ObjectError) -> Self {
        Self::Object(error)
    }
}

#[cfg(test)]
mod tests {
    use ku_core::foundation::{
        decode_knowledge_object, CapabilityExecutionRecordBody, CapabilityExecutionState,
        CorrelationDimensionEvidence, KnownObjectKind, RetentionRule,
    };

    use super::*;

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn execution(source_input_commitment: [u8; 32], output: &[u8]) -> CognitiveExecutionResult {
        let output_commitment = cognitive_output_commitment(output);
        CognitiveExecutionResult {
            output: output.to_vec(),
            output_commitment,
            termination: CognitiveTermination::Completed,
            consumed_work_units: 1,
            completed_steps: 1,
            backend_error_commitment: None,
            record: CapabilityExecutionRecordBody {
                task_id: [1; 32],
                offer_ref: reference(2),
                implementation_manifest: ObjectCid::from_bytes([3; 32]),
                input_commitments: vec![source_input_commitment],
                schema_prompt_parameter_commitments: vec![[4; 32]],
                output_refs_or_commitments: vec![ObjectReference::new(1, output_commitment)],
                state: CapabilityExecutionState::Completed,
                started_at: 10,
                finished_at: 11,
                limitations: vec![],
                log_digest: [5; 32],
                optional_attestation: None,
                retention_claim: RetentionRule::NoTraining,
            },
        }
    }

    fn request(session: u8) -> BlindEncodingRequest {
        BlindEncodingRequest {
            session_id: [session; 32],
            source_artifact: reference(10),
            source_input_commitment: [11; 32],
            blind_session_commitment: [session + 20; 32],
            challenge_nonce_commitment: [session + 30; 32],
            policy_ref: reference(12),
        }
    }

    fn correlation(request: &BlindEncodingRequest, admin: u8, pipeline: u8) -> CorrelationEvidence {
        let item = |dimension, commitment, strength| CorrelationDimensionEvidence {
            dimension,
            value_commitment: Some(commitment),
            strength,
            evidence_refs: vec![],
        };
        CorrelationEvidence {
            dimensions: vec![
                item(
                    CorrelationDimension::AdministrativePrincipal,
                    [admin; 32],
                    EvidenceStrength::CryptoBound,
                ),
                item(
                    CorrelationDimension::PipelineModelLineage,
                    [pipeline; 32],
                    EvidenceStrength::ExternallyAttested,
                ),
                item(
                    CorrelationDimension::BlindSession,
                    request.blind_session_commitment,
                    EvidenceStrength::CryptoBound,
                ),
                item(
                    CorrelationDimension::ChallengeNonce,
                    request.challenge_nonce_commitment,
                    EvidenceStrength::CryptoBound,
                ),
            ],
        }
    }

    fn plan() -> FidelityCheckPlan {
        FidelityCheckPlan {
            expected_source_span_commitments: vec![[40; 32]],
            expected_gene_selection_commitments: vec![[41; 32]],
            expected_concept_ccids: vec![ConceptCcid::from_bytes([42; 16])],
            source_span_evidence_ref: reference(43),
            gene_selection_evidence_ref: reference(44),
            concept_selection_evidence_ref: reference(45),
        }
    }

    fn inspection(candidate: ObjectReference, gene: u8) -> CandidateEncodingInspection {
        CandidateEncodingInspection {
            candidate_encoding: candidate,
            source_span_commitments: vec![[40; 32]],
            gene_selection_commitments: vec![[gene; 32]],
            concept_ccids: vec![ConceptCcid::from_bytes([42; 16])],
            source_span_inspection_complete: true,
            gene_inspection_complete: true,
            concept_inspection_complete: true,
        }
    }

    fn complete(
        coordinator: &mut BlindEncodingCoordinator,
        request: BlindEncodingRequest,
        candidate: ObjectReference,
        admin: u8,
        pipeline: u8,
        gene: u8,
    ) -> CompletedBlindAttestation {
        coordinator.open(request.clone()).unwrap();
        coordinator
            .commit_external_attempt(
                request.session_id,
                &execution(request.source_input_commitment, b"encoded"),
                vec![[50; 32]],
                [51; 32],
                correlation(&request, admin, pipeline),
            )
            .unwrap();
        coordinator
            .reveal_candidate(request.session_id, candidate.clone())
            .unwrap();
        coordinator
            .finish_checks(
                request.session_id,
                &plan(),
                &inspection(candidate, gene),
                vec![],
            )
            .unwrap()
    }

    #[test]
    fn target_cannot_be_revealed_before_output_commit() {
        let request = request(1);
        assert!(!request.contains_candidate_target());
        let mut coordinator = BlindEncodingCoordinator::default();
        coordinator.open(request.clone()).unwrap();
        assert_eq!(
            coordinator.reveal_candidate(request.session_id, reference(60)),
            Err(BlindFidelityError::RevealBeforeCommit)
        );
        let attempt = coordinator
            .commit_external_attempt(
                request.session_id,
                &execution(request.source_input_commitment, b"encoded"),
                vec![[50; 32]],
                [51; 32],
                correlation(&request, 1, 2),
            )
            .unwrap();
        assert!(attempt.attempt.candidate_encoding.is_none());
        assert_eq!(
            coordinator.phase(request.session_id),
            Some(BlindSessionPhase::AwaitingTargetReveal)
        );
    }

    #[test]
    fn exact_span_gene_concept_checks_expose_mismatch_without_truth_verdict() {
        let mut coordinator = BlindEncodingCoordinator::default();
        let completed = complete(&mut coordinator, request(1), reference(60), 1, 2, 99);
        assert!(completed.attestation.has_hard_encoding_mismatch());
        let gene = completed
            .attestation
            .checks
            .iter()
            .find(|check| check.kind == FidelityCheckKind::GeneSelection)
            .unwrap();
        assert_eq!(gene.status, FidelityCheckStatus::HardEncodingMismatch);
        assert!(!completed.establishes_proposition_truth());
        assert!(!completed.attestation.classifies_knowledge_as_wrong());
    }

    #[test]
    fn two_external_attempts_need_two_evidenced_principal_pipeline_groups() {
        let candidate = reference(60);
        let source = request(1).source_artifact;
        let policy = FidelityPolicy::default_v1();
        let mut coordinator = BlindEncodingCoordinator::default();
        let first = complete(&mut coordinator, request(1), candidate.clone(), 1, 2, 41);
        let second = complete(&mut coordinator, request(2), candidate.clone(), 3, 4, 41);
        let mut portfolio = BlindAttemptPortfolio::default();
        portfolio.record(&first, &policy).unwrap();
        assert!(!portfolio.meets_external_contract(&source, &candidate, &policy));
        portfolio.record(&second, &policy).unwrap();
        assert!(portfolio.meets_external_contract(&source, &candidate, &policy));
        assert!(!portfolio.establishes_cognitive_independence());
    }

    #[test]
    fn alternate_archive_retains_all_validated_encodings_even_after_mismatch() {
        fn object(byte: u8) -> ValidatedKnowledgeObject {
            let object = ku_core::foundation::KnowledgeObjectEnvelope::new(
                ku_core::foundation::ObjectKind(900),
                ku_core::foundation::SchemaVersion::new(1, 0),
                DisclosureClass::Public,
                CanonicalValue::Bytes(vec![byte]),
            );
            let (bytes, _) = object.encode(ResourceProfile::ObjectV1).unwrap();
            decode_knowledge_object(
                &bytes,
                ResourceProfile::ObjectV1,
                &[KnownObjectKind::new(
                    ku_core::foundation::ObjectKind(900),
                    1,
                )],
                &[],
            )
            .unwrap()
        }

        let source = reference(10);
        let first = object(1);
        let second = object(2);
        let mut archive = AlternateEncodingArchive::default();
        assert_eq!(
            archive.preserve(&source, &first).unwrap(),
            AlternateArchiveOutcome::Preserved
        );
        assert_eq!(
            archive.preserve(&source, &second).unwrap(),
            AlternateArchiveOutcome::Preserved
        );
        assert_eq!(archive.alternate_count(&source), 2);
        assert!(!archive.selects_winner_or_deletes_alternates());
    }

    #[test]
    fn unfinished_execution_cannot_be_committed_as_blind_attempt() {
        let request = request(1);
        let mut execution = execution(request.source_input_commitment, b"partial");
        execution.termination = CognitiveTermination::ResourceExceeded;
        let mut coordinator = BlindEncodingCoordinator::default();
        coordinator.open(request.clone()).unwrap();
        assert_eq!(
            coordinator.commit_external_attempt(
                request.session_id,
                &execution,
                vec![[50; 32]],
                [51; 32],
                correlation(&request, 1, 2),
            ),
            Err(BlindFidelityError::UnusableExecution)
        );
    }
}
