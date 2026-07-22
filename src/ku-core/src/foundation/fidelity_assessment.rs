//! Frontier-relative, policy-versioned encoding-fidelity assessment.

use std::collections::{BTreeMap, BTreeSet};

use super::canonical::{encode_canonical, CanonicalError, CanonicalValue, ResourceProfile};
use super::content_id::EventCid;
use super::fidelity::{
    EncodingAttempt, EncodingAttemptRole, EncodingFidelityAttestation, FidelityCheckStatus,
    FidelityError, FidelityPolicy, ValidatedEncodingFidelityAttestationEvent,
};
use super::object::ObjectReference;
use super::semantic::ConceptCcid;

pub const FIDELITY_ASSESSMENT_MAJOR: u64 = 1;
pub const FIDELITY_ASSESSMENT_MINOR: u64 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum FidelityAssessmentStatus {
    SelfAttested = 0,
    PartiallyCorroborated = 1,
    FidelityCorroboratedRelative = 2,
}

impl FidelityAssessmentStatus {
    pub const fn is_global_or_final(self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FidelityEvidenceCoverage {
    pub publisher_attempts: u64,
    pub signed_external_attestations: u64,
    pub eligible_external_attestations: u64,
    pub evidenced_distinct_groups: u64,
    pub hard_mismatch_attestations: u64,
    pub unresolved_check_count: u64,
    pub normalized_legacy_claims: u64,
}

impl FidelityEvidenceCoverage {
    fn canonical_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(self.publisher_attempts)),
            (
                1,
                CanonicalValue::Unsigned(self.signed_external_attestations),
            ),
            (
                2,
                CanonicalValue::Unsigned(self.eligible_external_attestations),
            ),
            (3, CanonicalValue::Unsigned(self.evidenced_distinct_groups)),
            (4, CanonicalValue::Unsigned(self.hard_mismatch_attestations)),
            (5, CanonicalValue::Unsigned(self.unresolved_check_count)),
            (6, CanonicalValue::Unsigned(self.normalized_legacy_claims)),
        ])
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FidelityAssessment {
    pub policy_ref: ObjectReference,
    pub source_artifact: ObjectReference,
    pub encoding_artifact: ObjectReference,
    pub accepted_attestation_set_root: [u8; 32],
    pub evidenced_correlation_groups: Vec<[u8; 32]>,
    pub blind_attempt_count: u64,
    pub coverage: FidelityEvidenceCoverage,
    pub assessed_frontier: EventCid,
    pub status: FidelityAssessmentStatus,
    pub limitations: Vec<ConceptCcid>,
}

impl FidelityAssessment {
    pub fn canonical_value(&self) -> Result<CanonicalValue, FidelityAssessmentError> {
        if self.accepted_attestation_set_root == [0; 32]
            || has_duplicates(&self.evidenced_correlation_groups)
            || has_duplicates(&self.limitations)
        {
            return Err(FidelityAssessmentError::InvalidAssessment);
        }
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(FIDELITY_ASSESSMENT_MAJOR)),
            (1, CanonicalValue::Unsigned(FIDELITY_ASSESSMENT_MINOR)),
            (2, self.policy_ref.to_value()),
            (3, self.source_artifact.to_value()),
            (4, self.encoding_artifact.to_value()),
            (5, canonical_bytes32_set(&self.evidenced_correlation_groups)),
            (
                6,
                CanonicalValue::Bytes(self.accepted_attestation_set_root.to_vec()),
            ),
            (7, CanonicalValue::Unsigned(self.blind_attempt_count)),
            (8, self.coverage.canonical_value()),
            (
                9,
                CanonicalValue::Bytes(self.assessed_frontier.as_bytes().to_vec()),
            ),
            (10, CanonicalValue::Unsigned(self.status as u64)),
            (11, canonical_ccid_set(&self.limitations)),
        ]))
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, FidelityAssessmentError> {
        encode_canonical(&self.canonical_value()?, ResourceProfile::ObjectV1).map_err(Into::into)
    }

    pub const fn establishes_proposition_truth(&self) -> bool {
        false
    }

    pub const fn blocks_preserve_publish_query_or_use(&self) -> bool {
        false
    }

    pub const fn selects_or_deletes_alternate_encodings(&self) -> bool {
        false
    }

    pub const fn creates_reward_or_obt(&self) -> bool {
        false
    }
}

/// Normalized legacy evidence. It deliberately contains no legacy wire token,
/// enum discriminant or claimed `FULL`/`GLOBAL` state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyEncodingClaim {
    pub source_artifact: ObjectReference,
    pub encoding_artifact: ObjectReference,
    pub imported_evidence_ref: ObjectReference,
    pub adapter_profile_commitment: [u8; 32],
    pub normalized_claim_commitment: [u8; 32],
    pub limitations: Vec<ConceptCcid>,
}

impl LegacyEncodingClaim {
    pub fn canonical_value(&self) -> Result<CanonicalValue, FidelityAssessmentError> {
        if self.adapter_profile_commitment == [0; 32]
            || self.normalized_claim_commitment == [0; 32]
            || has_duplicates(&self.limitations)
        {
            return Err(FidelityAssessmentError::InvalidLegacyClaim);
        }
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(1)),
            (1, self.source_artifact.to_value()),
            (2, self.encoding_artifact.to_value()),
            (3, self.imported_evidence_ref.to_value()),
            (
                4,
                CanonicalValue::Bytes(self.adapter_profile_commitment.to_vec()),
            ),
            (
                5,
                CanonicalValue::Bytes(self.normalized_claim_commitment.to_vec()),
            ),
            (6, canonical_ccid_set(&self.limitations)),
        ]))
    }

    pub const fn contains_legacy_wire_status(&self) -> bool {
        false
    }

    pub const fn establishes_corroborated_fidelity(&self) -> bool {
        false
    }

    pub const fn selects_or_deletes_alternate_encodings(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PublisherAttemptRecord {
    attempt_ref: ObjectReference,
    attempt: EncodingAttempt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FidelityRecordOutcome {
    Added,
    ExactReplay,
}

pub struct FidelityAssessmentReducer {
    policy_ref: ObjectReference,
    policy: FidelityPolicy,
    source_artifact: ObjectReference,
    encoding_artifact: ObjectReference,
    assessed_frontier: EventCid,
    publisher_attempts: BTreeMap<(u64, [u8; 32]), PublisherAttemptRecord>,
    attestations: BTreeMap<[u8; 32], ValidatedEncodingFidelityAttestationEvent>,
    legacy_claims: BTreeMap<[u8; 32], LegacyEncodingClaim>,
}

impl FidelityAssessmentReducer {
    pub fn new(
        policy_ref: ObjectReference,
        policy: FidelityPolicy,
        source_artifact: ObjectReference,
        encoding_artifact: ObjectReference,
        assessed_frontier: EventCid,
    ) -> Result<Self, FidelityAssessmentError> {
        policy.canonical_payload()?;
        Ok(Self {
            policy_ref,
            policy,
            source_artifact,
            encoding_artifact,
            assessed_frontier,
            publisher_attempts: BTreeMap::new(),
            attestations: BTreeMap::new(),
            legacy_claims: BTreeMap::new(),
        })
    }

    pub fn advance_frontier(&mut self, frontier: EventCid) {
        self.assessed_frontier = frontier;
    }

    pub fn record_publisher_attempt(
        &mut self,
        attempt_ref: ObjectReference,
        attempt: EncodingAttempt,
    ) -> Result<FidelityRecordOutcome, FidelityAssessmentError> {
        attempt.canonical_payload()?;
        if attempt.role != EncodingAttemptRole::Publisher
            || attempt.source_artifact != self.source_artifact
            || attempt.candidate_encoding.as_ref() != Some(&self.encoding_artifact)
        {
            return Err(FidelityAssessmentError::TargetMismatch);
        }
        let key = (attempt_ref.reference_kind, attempt_ref.cid);
        match self.publisher_attempts.get(&key) {
            Some(existing) if existing.attempt == attempt => Ok(FidelityRecordOutcome::ExactReplay),
            Some(_) => Err(FidelityAssessmentError::EvidenceIdentityConflict),
            None => {
                self.publisher_attempts.insert(
                    key,
                    PublisherAttemptRecord {
                        attempt_ref,
                        attempt,
                    },
                );
                Ok(FidelityRecordOutcome::Added)
            }
        }
    }

    pub fn record_attestation(
        &mut self,
        event: ValidatedEncodingFidelityAttestationEvent,
    ) -> Result<FidelityRecordOutcome, FidelityAssessmentError> {
        let attestation = event.attestation();
        if attestation.source_artifact != self.source_artifact
            || attestation.candidate_encoding != self.encoding_artifact
            || attestation.policy_ref != self.policy_ref
        {
            return Err(FidelityAssessmentError::TargetMismatch);
        }
        let key = event.event_cid().into_bytes();
        match self.attestations.get(&key) {
            Some(existing) if existing == &event => Ok(FidelityRecordOutcome::ExactReplay),
            Some(_) => Err(FidelityAssessmentError::EvidenceIdentityConflict),
            None => {
                self.attestations.insert(key, event);
                Ok(FidelityRecordOutcome::Added)
            }
        }
    }

    pub fn record_legacy_claim(
        &mut self,
        claim: LegacyEncodingClaim,
    ) -> Result<FidelityRecordOutcome, FidelityAssessmentError> {
        if claim.source_artifact != self.source_artifact
            || claim.encoding_artifact != self.encoding_artifact
        {
            return Err(FidelityAssessmentError::TargetMismatch);
        }
        let bytes = encode_canonical(&claim.canonical_value()?, ResourceProfile::ObjectV1)?;
        let key = domain_digest(
            b"onebrain:vnext:normalized-legacy-encoding-claim:1\0",
            &bytes,
        );
        match self.legacy_claims.get(&key) {
            Some(existing) if existing == &claim => Ok(FidelityRecordOutcome::ExactReplay),
            Some(_) => Err(FidelityAssessmentError::EvidenceIdentityConflict),
            None => {
                self.legacy_claims.insert(key, claim);
                Ok(FidelityRecordOutcome::Added)
            }
        }
    }

    pub fn assess(&self) -> Result<FidelityAssessment, FidelityAssessmentError> {
        if self.publisher_attempts.is_empty() && self.attestations.is_empty() {
            return Err(FidelityAssessmentError::NoVNextEvidence);
        }

        #[derive(Default)]
        struct AttemptState {
            hard_mismatch: bool,
            eligible_groups: BTreeSet<[u8; 32]>,
        }

        let mut hard_mismatch_count = 0_u64;
        let mut unresolved_check_count = 0_u64;
        let mut attempt_states: BTreeMap<(u64, [u8; 32]), AttemptState> = BTreeMap::new();
        let mut limitations = BTreeSet::new();
        for event in self.attestations.values() {
            let attestation = event.attestation();
            let attempt_state = attempt_states
                .entry((
                    attestation.attempt_ref.reference_kind,
                    attestation.attempt_ref.cid,
                ))
                .or_default();
            limitations.extend(attestation.limitations.iter().copied());
            unresolved_check_count += attestation
                .checks
                .iter()
                .filter(|check| check.status == FidelityCheckStatus::Unresolved)
                .count() as u64;
            if attestation.has_hard_encoding_mismatch() {
                hard_mismatch_count += 1;
                attempt_state.hard_mismatch = true;
                continue;
            }
            if !has_required_checks(attestation, &self.policy) {
                continue;
            }
            let Some(group) = self
                .policy
                .evidenced_group_key(&attestation.correlation_evidence)?
            else {
                continue;
            };
            attempt_state.eligible_groups.insert(group);
        }
        for claim in self.legacy_claims.values() {
            limitations.extend(claim.limitations.iter().copied());
        }

        let mut groups = BTreeSet::new();
        let mut eligible_count = 0_u64;
        for attempt in attempt_states.values() {
            if !attempt.hard_mismatch && attempt.eligible_groups.len() == 1 {
                groups.extend(attempt.eligible_groups.iter().copied());
                eligible_count += 1;
            }
        }

        let publisher_count = self.publisher_attempts.len() as u64;
        let corroborated = self.policy.publisher_attempt_required
            && publisher_count > 0
            && eligible_count >= u64::from(self.policy.minimum_external_blind_attempts)
            && groups.len() >= usize::from(self.policy.minimum_evidenced_distinct_external_groups);
        let status = if corroborated {
            FidelityAssessmentStatus::FidelityCorroboratedRelative
        } else if !self.attestations.is_empty() {
            FidelityAssessmentStatus::PartiallyCorroborated
        } else {
            FidelityAssessmentStatus::SelfAttested
        };
        let event_ids = self.attestations.keys().copied().collect::<Vec<_>>();
        let evidenced_distinct_groups = groups.len() as u64;
        let assessment = FidelityAssessment {
            policy_ref: self.policy_ref.clone(),
            source_artifact: self.source_artifact.clone(),
            encoding_artifact: self.encoding_artifact.clone(),
            accepted_attestation_set_root: attestation_set_root(&event_ids)?,
            evidenced_correlation_groups: groups.into_iter().collect(),
            blind_attempt_count: attempt_states.len() as u64,
            coverage: FidelityEvidenceCoverage {
                publisher_attempts: publisher_count,
                signed_external_attestations: self.attestations.len() as u64,
                eligible_external_attestations: eligible_count,
                evidenced_distinct_groups,
                hard_mismatch_attestations: hard_mismatch_count,
                unresolved_check_count,
                normalized_legacy_claims: self.legacy_claims.len() as u64,
            },
            assessed_frontier: self.assessed_frontier,
            status,
            limitations: limitations.into_iter().collect(),
        };
        assessment.canonical_value()?;
        Ok(assessment)
    }

    pub const fn deletes_or_selects_alternates(&self) -> bool {
        false
    }
}

fn has_required_checks(attestation: &EncodingFidelityAttestation, policy: &FidelityPolicy) -> bool {
    policy.required_checks.iter().all(|required| {
        attestation
            .checks
            .iter()
            .any(|check| check.kind == *required)
    })
}

fn attestation_set_root(event_ids: &[[u8; 32]]) -> Result<[u8; 32], FidelityAssessmentError> {
    let bytes = encode_canonical(&canonical_bytes32_set(event_ids), ResourceProfile::ObjectV1)?;
    Ok(domain_digest(
        b"onebrain:vnext:accepted-fidelity-attestation-set:1\0",
        &bytes,
    ))
}

fn canonical_bytes32_set(values: &[[u8; 32]]) -> CanonicalValue {
    let mut values = values.to_vec();
    values.sort();
    values.dedup();
    CanonicalValue::Array(
        values
            .into_iter()
            .map(|value| CanonicalValue::Bytes(value.to_vec()))
            .collect(),
    )
}

fn canonical_ccid_set(values: &[ConceptCcid]) -> CanonicalValue {
    let mut values = values.to_vec();
    values.sort();
    values.dedup();
    CanonicalValue::Array(
        values
            .into_iter()
            .map(|value| CanonicalValue::Bytes(value.as_bytes().to_vec()))
            .collect(),
    )
}

fn domain_digest(domain: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(&(bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

fn has_duplicates<T: Ord + Copy>(values: &[T]) -> bool {
    values.iter().copied().collect::<BTreeSet<_>>().len() != values.len()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FidelityAssessmentError {
    Canonical(CanonicalError),
    Fidelity(FidelityError),
    InvalidAssessment,
    InvalidLegacyClaim,
    TargetMismatch,
    EvidenceIdentityConflict,
    NoVNextEvidence,
}

impl From<CanonicalError> for FidelityAssessmentError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<FidelityError> for FidelityAssessmentError {
    fn from(error: FidelityError) -> Self {
        Self::Fidelity(error)
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;

    use super::*;
    use crate::foundation::{
        decode_feed_inception, decode_knowledge_event, decode_knowledge_object,
        CorrelationDimension, CorrelationDimensionEvidence, CorrelationEvidence, DeviceId,
        DisclosureClass, EncodingFidelityAttestation, EvidenceStrength, FeedInception,
        FidelityCheck, FidelityCheckKind, KnowledgeEventEnvelope, KnownObjectKind,
        NamespaceCommitment, ResourceProfile, SignedFeedInception,
        ENCODING_FIDELITY_ATTESTATION_EVENT_TYPE, ENCODING_FIDELITY_ATTESTATION_KIND,
        FIDELITY_PROFILE_MAJOR,
    };

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn publisher(source: ObjectReference, encoding: ObjectReference) -> EncodingAttempt {
        EncodingAttempt {
            role: EncodingAttemptRole::Publisher,
            source_artifact: source,
            candidate_encoding: Some(encoding),
            output_commitment: [10; 32],
            pipeline_model_tool_commitments: vec![[11; 32]],
            source_acquisition_or_derivation_commitment: [12; 32],
            execution_record_ref: reference(13),
            blind_session_commitment: None,
            challenge_nonce_commitment: None,
        }
    }

    fn correlation(admin: u8, pipeline: u8) -> CorrelationEvidence {
        CorrelationEvidence {
            dimensions: vec![
                CorrelationDimensionEvidence {
                    dimension: CorrelationDimension::AdministrativePrincipal,
                    value_commitment: Some([admin; 32]),
                    strength: EvidenceStrength::CryptoBound,
                    evidence_refs: vec![],
                },
                CorrelationDimensionEvidence {
                    dimension: CorrelationDimension::PipelineModelLineage,
                    value_commitment: Some([pipeline; 32]),
                    strength: EvidenceStrength::ExternallyAttested,
                    evidence_refs: vec![],
                },
            ],
        }
    }

    fn attestation(
        source: ObjectReference,
        encoding: ObjectReference,
        policy: ObjectReference,
        admin: u8,
        pipeline: u8,
        hard_mismatch: bool,
    ) -> EncodingFidelityAttestation {
        let check = |kind, byte| FidelityCheck {
            kind,
            status: if hard_mismatch && kind == FidelityCheckKind::GeneSelection {
                FidelityCheckStatus::HardEncodingMismatch
            } else {
                FidelityCheckStatus::ConsistentWithSource
            },
            checked_region_commitment: [byte; 32],
            evidence_ref: None,
        };
        EncodingFidelityAttestation {
            source_artifact: source,
            candidate_encoding: encoding,
            blind_attempt_output_commitment: [20; 32],
            attempt_ref: ObjectReference::new(0, [admin.wrapping_add(20); 32]),
            execution_record_ref: reference(22),
            correlation_evidence: correlation(admin, pipeline),
            checks: vec![
                check(FidelityCheckKind::SourceSpanAlignment, 23),
                check(FidelityCheckKind::GeneSelection, 24),
                check(FidelityCheckKind::ConceptSelection, 25),
            ],
            limitations: vec![],
            policy_ref: policy,
        }
    }

    fn validated_event(
        sequence: u64,
        payload: EncodingFidelityAttestation,
    ) -> ValidatedEncodingFidelityAttestationEvent {
        let key = SigningKey::from_bytes(&[30; 32]);
        let inception = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"assessment", [31; 32]).unwrap(),
            0,
            DeviceId::from_bytes([32; 32]),
        );
        let signed: SignedFeedInception = inception.sign(&key).unwrap();
        let feed = decode_feed_inception(&signed.encode().unwrap()).unwrap();
        let object = payload
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap();
        let (object_bytes, object_cid) = object.encode(ResourceProfile::ObjectV1).unwrap();
        let object = decode_knowledge_object(
            &object_bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(
                ENCODING_FIDELITY_ATTESTATION_KIND,
                FIDELITY_PROFILE_MAJOR,
            )],
            &[],
        )
        .unwrap();
        let mut event = KnowledgeEventEnvelope::new(
            ENCODING_FIDELITY_ATTESTATION_EVENT_TYPE,
            feed.feed_id,
            sequence,
            DisclosureClass::Public,
            [sequence as u8; 32],
        );
        event.payload_refs = vec![ObjectReference::new(0, object_cid.into_bytes())];
        let (event_bytes, _) = event.sign(&feed, &key).unwrap().encode().unwrap();
        let event = decode_knowledge_event(
            &event_bytes,
            &feed,
            &[ENCODING_FIDELITY_ATTESTATION_EVENT_TYPE],
        )
        .unwrap();
        ValidatedEncodingFidelityAttestationEvent::bind(&event, &object).unwrap()
    }

    fn reducer() -> FidelityAssessmentReducer {
        let source = reference(1);
        let encoding = reference(2);
        let policy_ref = reference(3);
        let mut reducer = FidelityAssessmentReducer::new(
            policy_ref,
            FidelityPolicy::default_v1(),
            source.clone(),
            encoding.clone(),
            EventCid::from_bytes([4; 32]),
        )
        .unwrap();
        reducer
            .record_publisher_attempt(reference(5), publisher(source, encoding))
            .unwrap();
        reducer
    }

    #[test]
    fn assessment_rebuild_is_order_independent_and_policy_relative() {
        let source = reference(1);
        let encoding = reference(2);
        let policy = reference(3);
        let first = validated_event(
            1,
            attestation(
                source.clone(),
                encoding.clone(),
                policy.clone(),
                1,
                2,
                false,
            ),
        );
        let second = validated_event(2, attestation(source, encoding, policy, 3, 4, false));
        let mut left = reducer();
        left.record_attestation(first.clone()).unwrap();
        left.record_attestation(second.clone()).unwrap();
        let mut right = reducer();
        right.record_attestation(second).unwrap();
        right.record_attestation(first).unwrap();
        let left = left.assess().unwrap();
        let right = right.assess().unwrap();
        assert_eq!(left, right);
        assert_eq!(
            left.status,
            FidelityAssessmentStatus::FidelityCorroboratedRelative
        );
        assert!(!left.status.is_global_or_final());
        assert!(!left.establishes_proposition_truth());
    }

    #[test]
    fn hundred_same_pipeline_attestations_never_reach_two_groups() {
        let mut reducer = reducer();
        for sequence in 1..=100 {
            reducer
                .record_attestation(validated_event(
                    sequence,
                    attestation(reference(1), reference(2), reference(3), 1, 2, false),
                ))
                .unwrap();
        }
        let assessment = reducer.assess().unwrap();
        assert_eq!(
            assessment.status,
            FidelityAssessmentStatus::PartiallyCorroborated
        );
        assert_eq!(assessment.coverage.evidenced_distinct_groups, 1);
        assert_eq!(assessment.coverage.signed_external_attestations, 100);
    }

    #[test]
    fn hard_mismatch_is_retained_and_never_deletes_or_blocks_knowledge() {
        let mut reducer = reducer();
        reducer
            .record_attestation(validated_event(
                1,
                attestation(reference(1), reference(2), reference(3), 1, 2, true),
            ))
            .unwrap();
        let assessment = reducer.assess().unwrap();
        assert_eq!(assessment.coverage.hard_mismatch_attestations, 1);
        assert_eq!(
            assessment.status,
            FidelityAssessmentStatus::PartiallyCorroborated
        );
        assert!(!assessment.blocks_preserve_publish_query_or_use());
        assert!(!assessment.selects_or_deletes_alternate_encodings());
        assert!(!reducer.deletes_or_selects_alternates());
    }

    #[test]
    fn normalized_legacy_claim_never_upgrades_status_or_enters_attestation_root() {
        let mut baseline = reducer();
        let before = baseline.assess().unwrap();
        let claim = LegacyEncodingClaim {
            source_artifact: reference(1),
            encoding_artifact: reference(2),
            imported_evidence_ref: reference(40),
            adapter_profile_commitment: [41; 32],
            normalized_claim_commitment: [42; 32],
            limitations: vec![ConceptCcid::from_bytes([43; 16])],
        };
        assert!(!claim.contains_legacy_wire_status());
        assert!(!claim.establishes_corroborated_fidelity());
        baseline.record_legacy_claim(claim).unwrap();
        let after = baseline.assess().unwrap();
        assert_eq!(before.status, after.status);
        assert_eq!(
            before.accepted_attestation_set_root,
            after.accepted_attestation_set_root
        );
        assert_eq!(after.coverage.normalized_legacy_claims, 1);
    }
}
