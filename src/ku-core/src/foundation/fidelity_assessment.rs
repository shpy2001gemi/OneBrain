//! Frontier-relative, policy-versioned encoding-fidelity assessment.

use std::collections::{BTreeMap, BTreeSet};

use super::canonical::{encode_canonical, CanonicalError, CanonicalValue, ResourceProfile};
use super::content_id::EventCid;
use super::fidelity::{
    CorrelationDimension, CorrelationEvidence, EncodingAttempt, EncodingAttemptRole,
    EncodingFidelityAttestation, EvidenceStrength, FidelityCheckStatus, FidelityError,
    FidelityPolicy, ValidatedEncodingFidelityAttestationEvent,
};
use super::object::ObjectReference;
use super::semantic::ConceptCcid;

pub const FIDELITY_ASSESSMENT_MAJOR: u64 = 1;
pub const FIDELITY_ASSESSMENT_MINOR: u64 = 0;

/// Canonical 4-dimensional correlation basis required for P5/P3 corroboration
/// group counting: administrative principal, pipeline/model lineage, execution
/// environment, and causal stakeholder relation.
pub const CORROBORATION_DIMENSIONS_4D: [CorrelationDimension; 4] = [
    CorrelationDimension::AdministrativePrincipal,
    CorrelationDimension::PipelineModelLineage,
    CorrelationDimension::ExecutionEnvironment,
    CorrelationDimension::CausalStakeholderRelation,
];

/// Categorical stakeholder relation for `CorrelationDimension::CausalStakeholderRelation`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum CausalStakeholderRole {
    SelfAuthor = 0,
    DirectBeneficiary = 1,
    UnrelatedWitness = 2,
    Unknown = 3,
}

impl CausalStakeholderRole {
    /// Derives a non-zero 32-byte stakeholder commitment scoped to a principal
    /// commitment, or `None` when the stakeholder relation is `Unknown`.
    pub fn commitment_for_principal(self, principal_commitment: &[u8; 32]) -> Option<[u8; 32]> {
        if self == Self::Unknown || principal_commitment == &[0; 32] {
            return None;
        }
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:causal-stakeholder-commitment:1\0");
        hasher.update(&[self as u8]);
        hasher.update(principal_commitment);
        Some(*hasher.finalize().as_bytes())
    }
}

/// Extracted 4D commitment tuple for an attestation evidenced solely by
/// `CryptoBound` or `ExternallyAttested` strengths.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CorroborationTuple4d {
    pub administrative_principal: [u8; 32],
    pub pipeline_model_lineage: [u8; 32],
    pub execution_environment: [u8; 32],
    pub causal_stakeholder_relation: [u8; 32],
}

impl CorroborationTuple4d {
    pub fn group_id(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:corroboration-4d-group:1\0");
        hasher.update(&self.administrative_principal);
        hasher.update(&self.pipeline_model_lineage);
        hasher.update(&self.execution_environment);
        hasher.update(&self.causal_stakeholder_relation);
        *hasher.finalize().as_bytes()
    }

    /// Returns `true` iff two 4D tuples share zero commitments across all 4 axes.
    pub fn is_non_overlapping(&self, other: &Self) -> bool {
        self.administrative_principal != other.administrative_principal
            && self.pipeline_model_lineage != other.pipeline_model_lineage
            && self.execution_environment != other.execution_environment
            && self.causal_stakeholder_relation != other.causal_stakeholder_relation
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorroborationGroupEntry {
    pub group_id: [u8; 32],
    pub tuple: CorroborationTuple4d,
    pub attestation_events: Vec<EventCid>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorroborationGroupSummary {
    pub total_attestations: usize,
    pub eligible_4d_attestations: usize,
    pub ungrouped_weak_or_incomplete: Vec<EventCid>,
    pub groups: Vec<CorroborationGroupEntry>,
    pub distinct_non_overlapping_group_count: usize,
    pub max_independent_group_ids: Vec<[u8; 32]>,
}

impl CorroborationGroupSummary {
    pub const fn establishes_proposition_truth(&self) -> bool {
        false
    }

    pub const fn uses_self_claimed_strength(&self) -> bool {
        false
    }
}

pub trait Has4dCorrelation {
    fn attestation_event_cid(&self) -> EventCid;
    fn correlation_evidence(&self) -> &CorrelationEvidence;
}

impl Has4dCorrelation for ValidatedEncodingFidelityAttestationEvent {
    fn attestation_event_cid(&self) -> EventCid {
        self.event_cid()
    }

    fn correlation_evidence(&self) -> &CorrelationEvidence {
        &self.attestation().correlation_evidence
    }
}

impl Has4dCorrelation for (EventCid, CorrelationEvidence) {
    fn attestation_event_cid(&self) -> EventCid {
        self.0
    }

    fn correlation_evidence(&self) -> &CorrelationEvidence {
        &self.1
    }
}

impl Has4dCorrelation for (EventCid, &CorrelationEvidence) {
    fn attestation_event_cid(&self) -> EventCid {
        self.0
    }

    fn correlation_evidence(&self) -> &CorrelationEvidence {
        self.1
    }
}

/// Groups attestations across `(AdministrativePrincipal, PipelineModelLineage,
/// ExecutionEnvironment, CausalStakeholderRelation)` and counts distinct
/// corroboration groups ONLY when evidenced by `EvidenceStrength::{CryptoBound,
/// ExternallyAttested}` non-overlapping commitments across all 4 dimensions.
pub fn group_attestations_by_4d_correlation<T: Has4dCorrelation>(
    attestations: &[T],
) -> Result<CorroborationGroupSummary, FidelityAssessmentError> {
    let mut ungrouped = BTreeSet::new();
    let mut by_group: BTreeMap<[u8; 32], (CorroborationTuple4d, BTreeSet<[u8; 32]>)> =
        BTreeMap::new();
    let mut eligible_events = BTreeSet::new();

    for item in attestations {
        let cid = item.attestation_event_cid().into_bytes();
        let evidence = item.correlation_evidence();
        evidence.canonical_value()?;
        if let Some(tuple) = extract_evidenced_4d_tuple(evidence) {
            let gid = tuple.group_id();
            eligible_events.insert(cid);
            let entry = by_group
                .entry(gid)
                .or_insert_with(|| (tuple, BTreeSet::new()));
            entry.1.insert(cid);
        } else {
            ungrouped.insert(cid);
        }
    }

    let groups: Vec<CorroborationGroupEntry> = by_group
        .into_iter()
        .map(|(group_id, (tuple, events))| CorroborationGroupEntry {
            group_id,
            tuple,
            attestation_events: events.into_iter().map(EventCid::from_bytes).collect(),
        })
        .collect();

    let max_independent_group_ids = max_non_overlapping_4d_subset(&groups);
    let distinct_non_overlapping_group_count = max_independent_group_ids.len();

    Ok(CorroborationGroupSummary {
        total_attestations: attestations.len(),
        eligible_4d_attestations: eligible_events.len(),
        ungrouped_weak_or_incomplete: ungrouped.into_iter().map(EventCid::from_bytes).collect(),
        groups,
        distinct_non_overlapping_group_count,
        max_independent_group_ids,
    })
}

fn extract_evidenced_4d_tuple(evidence: &CorrelationEvidence) -> Option<CorroborationTuple4d> {
    let mut commitments = [[0u8; 32]; 4];
    for (idx, required_dim) in CORROBORATION_DIMENSIONS_4D.iter().enumerate() {
        let dim = evidence.dimension(*required_dim)?;
        if !matches!(
            dim.strength,
            EvidenceStrength::CryptoBound | EvidenceStrength::ExternallyAttested
        ) {
            return None;
        }
        let commitment = dim.value_commitment?;
        if commitment == [0; 32] {
            return None;
        }
        commitments[idx] = commitment;
    }
    Some(CorroborationTuple4d {
        administrative_principal: commitments[0],
        pipeline_model_lineage: commitments[1],
        execution_environment: commitments[2],
        causal_stakeholder_relation: commitments[3],
    })
}

fn max_non_overlapping_4d_subset(groups: &[CorroborationGroupEntry]) -> Vec<[u8; 32]> {
    let n = groups.len();
    if n == 0 {
        return Vec::new();
    }
    // For larger sets, fall back to deterministic greedy packing in canonical group_id order;
    // for <= 48 groups, solve exact maximum independent set with deterministic tie-breaking.
    if n > 48 {
        let mut chosen: Vec<&CorroborationGroupEntry> = Vec::new();
        for candidate in groups {
            if chosen
                .iter()
                .all(|existing| existing.tuple.is_non_overlapping(&candidate.tuple))
            {
                chosen.push(candidate);
            }
        }
        return chosen.into_iter().map(|entry| entry.group_id).collect();
    }

    let mut adj = vec![0u64; n];
    for i in 0..n {
        for j in (i + 1)..n {
            if !groups[i].tuple.is_non_overlapping(&groups[j].tuple) {
                adj[i] |= 1u64 << j;
                adj[j] |= 1u64 << i;
            }
        }
    }

    fn search(
        idx: usize,
        n: usize,
        adj: &[u64],
        blocked: u64,
        current: &mut Vec<usize>,
        best: &mut Vec<usize>,
    ) {
        if current.len() + (n - idx) < best.len() {
            return;
        }
        if idx == n {
            if current.len() > best.len() {
                *best = current.clone();
            }
            return;
        }
        if (blocked & (1u64 << idx)) == 0 {
            current.push(idx);
            search(idx + 1, n, adj, blocked | adj[idx], current, best);
            current.pop();
        }
        search(idx + 1, n, adj, blocked, current, best);
    }

    let mut current = Vec::new();
    let mut best = Vec::new();
    search(0, n, &adj, 0, &mut current, &mut best);
    best.into_iter().map(|idx| groups[idx].group_id).collect()
}

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

    fn full_4d_correlation(
        admin: u8,
        pipeline: u8,
        env: u8,
        stakeholder: u8,
        stakeholder_strength: EvidenceStrength,
    ) -> CorrelationEvidence {
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
                CorrelationDimensionEvidence {
                    dimension: CorrelationDimension::ExecutionEnvironment,
                    value_commitment: Some([env; 32]),
                    strength: EvidenceStrength::ExternallyAttested,
                    evidence_refs: vec![],
                },
                CorrelationDimensionEvidence {
                    dimension: CorrelationDimension::CausalStakeholderRelation,
                    value_commitment: Some([stakeholder; 32]),
                    strength: stakeholder_strength,
                    evidence_refs: vec![],
                },
            ],
        }
    }

    #[test]
    fn group_attestations_by_4d_correlation_enforces_non_overlapping_crypto_or_attested_axes() {
        let g1 = (
            EventCid::from_bytes([1; 32]),
            full_4d_correlation(10, 20, 30, 40, EvidenceStrength::CryptoBound),
        );
        let g2 = (
            EventCid::from_bytes([2; 32]),
            full_4d_correlation(11, 21, 31, 41, EvidenceStrength::ExternallyAttested),
        );
        // Shares ExecutionEnvironment (30) with g1 -> cannot increase distinct count!
        let shares_env = (
            EventCid::from_bytes([3; 32]),
            full_4d_correlation(12, 22, 30, 42, EvidenceStrength::CryptoBound),
        );
        // SelfClaimed stakeholder strength -> rejected from eligible 4D groups!
        let weak_self_claimed = (
            EventCid::from_bytes([4; 32]),
            full_4d_correlation(13, 23, 33, 43, EvidenceStrength::SelfClaimed),
        );
        // Bridge attestation overlapping g1 on admin (10) and g2 on pipeline (21) -> must not grief g1+g2!
        let bridge_grief = (
            EventCid::from_bytes([5; 32]),
            full_4d_correlation(10, 21, 35, 45, EvidenceStrength::CryptoBound),
        );

        let summary = group_attestations_by_4d_correlation(&[
            g1,
            g2,
            shares_env,
            weak_self_claimed,
            bridge_grief,
        ])
        .unwrap();

        assert_eq!(summary.total_attestations, 5);
        assert_eq!(summary.eligible_4d_attestations, 4);
        assert_eq!(
            summary.ungrouped_weak_or_incomplete,
            vec![EventCid::from_bytes([4; 32])]
        );
        assert_eq!(summary.distinct_non_overlapping_group_count, 2);
        assert!(!summary.establishes_proposition_truth());
        assert!(!summary.uses_self_claimed_strength());

        let role_commit = CausalStakeholderRole::UnrelatedWitness
            .commitment_for_principal(&[77; 32])
            .unwrap();
        assert_ne!(role_commit, [0; 32]);
        assert!(CausalStakeholderRole::Unknown
            .commitment_for_principal(&[77; 32])
            .is_none());
    }
}
