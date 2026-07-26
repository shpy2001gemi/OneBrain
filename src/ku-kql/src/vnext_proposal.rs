//! Ephemeral, non-authoritative binding/discovery proposals for KQL vNext.

use std::collections::{BTreeMap, BTreeSet};

use ku_core::foundation::{
    CanonicalValue, ConceptCcid, ConstraintEvaluation, DisclosureClass, EventCid, ExactRatio,
    MappingEnvelope, MappingError, MappingKernel, MappingKernelCid, ObjectReference,
    ResourceProfile,
};

pub const MAX_PROPOSAL_CANDIDATES: usize = 16_384;
pub const MAX_SCORE_COMPONENTS: usize = 1_024;
pub const MAX_CONSTRAINT_OBSERVATIONS: usize = 16_384;
pub const MAX_PROPOSAL_QUARANTINE_RECORDS: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProposalId([u8; 32]);

impl ProposalId {
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum ScoreDirection {
    HigherIsBetter = 0,
    LowerIsBetter = 1,
    DescriptiveOnly = 2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScoreComponent {
    pub metric: ConceptCcid,
    pub value: ExactRatio,
    pub direction: ScoreDirection,
}

impl ScoreComponent {
    fn to_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(self.metric.as_bytes().to_vec())),
            (
                1,
                CanonicalValue::Bytes(self.value.numerator().to_be_bytes().to_vec()),
            ),
            (
                2,
                CanonicalValue::Bytes(self.value.denominator().to_be_bytes().to_vec()),
            ),
            (3, CanonicalValue::Unsigned(self.direction as u64)),
        ])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConstraintObservation {
    /// Stable index into `MappingKernel::constraint_regions`.
    pub constraint_index: u32,
    pub evaluation: ConstraintEvaluation,
    pub required: bool,
}

impl ConstraintObservation {
    fn to_value(self) -> CanonicalValue {
        let state = match self.evaluation {
            ConstraintEvaluation::Satisfied => 0,
            ConstraintEvaluation::Violated => 1,
            ConstraintEvaluation::Unknown => 2,
        };
        CanonicalValue::Map(vec![
            (
                0,
                CanonicalValue::Unsigned(u64::from(self.constraint_index)),
            ),
            (1, CanonicalValue::Unsigned(state)),
            (2, CanonicalValue::Bool(self.required)),
        ])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProposalExpiry {
    pub created_at_evaluation: u64,
    pub expires_after_evaluations: u64,
    pub source_frontier: EventCid,
}

impl ProposalExpiry {
    pub fn is_expired(self, current_evaluation: u64) -> bool {
        current_evaluation.saturating_sub(self.created_at_evaluation)
            >= self.expires_after_evaluations
    }

    fn to_value(self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(self.created_at_evaluation)),
            (1, CanonicalValue::Unsigned(self.expires_after_evaluations)),
            (
                2,
                CanonicalValue::Bytes(self.source_frontier.as_bytes().to_vec()),
            ),
        ])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProposalDisposition {
    CandidateOnly,
    BlockedHardViolation,
    Expired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingProposal {
    pub mapping_kernel: MappingKernel,
    pub proposed_envelope: MappingEnvelope,
    pub candidate_objects: Vec<ObjectReference>,
    pub index_commitment: Option<ObjectReference>,
    pub model_commitment: Option<ObjectReference>,
    pub rule_commitment: Option<ObjectReference>,
    pub scores: Vec<ScoreComponent>,
    pub constraints: Vec<ConstraintObservation>,
    pub expiry: ProposalExpiry,
    pub privacy: DisclosureClass,
}

impl BindingProposal {
    pub fn kernel_id(&self) -> Result<MappingKernelCid, ProposalError> {
        self.mapping_kernel.cid().map_err(Into::into)
    }

    pub fn validate(&self) -> Result<(), ProposalError> {
        if !matches!(
            self.privacy,
            DisclosureClass::LocalOnly | DisclosureClass::NegotiatedEncrypted
        ) {
            return Err(ProposalError::ProposalMustRemainPrivate);
        }
        if self.expiry.expires_after_evaluations == 0 {
            return Err(ProposalError::InvalidExpiry);
        }
        if self.candidate_objects.len() > MAX_PROPOSAL_CANDIDATES
            || self.scores.len() > MAX_SCORE_COMPONENTS
            || self.constraints.len() > MAX_CONSTRAINT_OBSERVATIONS
        {
            return Err(ProposalError::Limit);
        }
        if self.kernel_id()? != self.proposed_envelope.kernel {
            return Err(ProposalError::KernelEnvelopeMismatch);
        }
        canonical_reference_set(&self.candidate_objects)?;
        canonical_score_set(&self.scores)?;
        canonical_constraint_set(&self.constraints)?;
        if self.constraints.iter().any(|observation| {
            observation.constraint_index as usize >= self.mapping_kernel.constraint_regions.len()
        }) {
            return Err(ProposalError::ConstraintIndexOutOfRange);
        }
        Ok(())
    }

    pub fn disposition(&self, current_evaluation: u64) -> ProposalDisposition {
        if self.expiry.is_expired(current_evaluation) {
            ProposalDisposition::Expired
        } else if self.constraints.iter().any(|constraint| {
            constraint.required && constraint.evaluation == ConstraintEvaluation::Violated
        }) {
            ProposalDisposition::BlockedHardViolation
        } else {
            ProposalDisposition::CandidateOnly
        }
    }

    pub fn proposal_id(&self) -> Result<ProposalId, ProposalError> {
        self.validate()?;
        let bytes = ku_core::foundation::encode_canonical(
            &self.canonical_value()?,
            ResourceProfile::ObjectV1,
        )?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:local-binding-proposal:1\0");
        hasher.update(&bytes);
        Ok(ProposalId(*hasher.finalize().as_bytes()))
    }

    fn canonical_value(&self) -> Result<CanonicalValue, ProposalError> {
        let mut fields = vec![
            (0, CanonicalValue::Unsigned(1)),
            (1, self.mapping_kernel.canonical_value()?),
            (2, self.proposed_envelope.canonical_payload()?),
            (3, canonical_reference_set(&self.candidate_objects)?),
            (7, canonical_score_set(&self.scores)?),
            (8, canonical_constraint_set(&self.constraints)?),
            (9, self.expiry.to_value()),
            (10, CanonicalValue::Unsigned(self.privacy as u64)),
        ];
        if let Some(commitment) = &self.index_commitment {
            fields.push((4, reference_value(commitment)));
        }
        if let Some(commitment) = &self.model_commitment {
            fields.push((5, reference_value(commitment)));
        }
        if let Some(commitment) = &self.rule_commitment {
            fields.push((6, reference_value(commitment)));
        }
        Ok(CanonicalValue::Map(fields))
    }
}

/// Explicitly non-executable, local proposal storage. It has no materialization
/// or graph-projection API.
pub struct ProposalQuarantine {
    proposals: BTreeMap<[u8; 32], BindingProposal>,
    capacity: usize,
}

impl Default for ProposalQuarantine {
    fn default() -> Self {
        Self {
            proposals: BTreeMap::new(),
            capacity: MAX_PROPOSAL_QUARANTINE_RECORDS,
        }
    }
}

impl ProposalQuarantine {
    pub fn with_capacity(capacity: usize) -> Result<Self, ProposalError> {
        if capacity == 0 || capacity > MAX_PROPOSAL_QUARANTINE_RECORDS {
            return Err(ProposalError::Limit);
        }
        Ok(Self {
            proposals: BTreeMap::new(),
            capacity,
        })
    }

    pub fn insert(&mut self, proposal: BindingProposal) -> Result<ProposalId, ProposalError> {
        let id = proposal.proposal_id()?;
        if !self.proposals.contains_key(id.as_bytes()) && self.proposals.len() >= self.capacity {
            return Err(ProposalError::Limit);
        }
        self.proposals.entry(id.0).or_insert(proposal);
        Ok(id)
    }

    pub fn get(&self, id: ProposalId) -> Option<&BindingProposal> {
        self.proposals.get(id.as_bytes())
    }

    pub fn expire(&mut self, current_evaluation: u64) -> usize {
        let before = self.proposals.len();
        self.proposals
            .retain(|_, proposal| !proposal.expiry.is_expired(current_evaluation));
        before - self.proposals.len()
    }

    pub const fn is_executable(&self) -> bool {
        false
    }

    pub fn len(&self) -> usize {
        self.proposals.len()
    }
}

fn canonical_reference_set(values: &[ObjectReference]) -> Result<CanonicalValue, ProposalError> {
    let values = values.iter().map(reference_value).collect::<Vec<_>>();
    canonical_set(values)
}

fn canonical_score_set(values: &[ScoreComponent]) -> Result<CanonicalValue, ProposalError> {
    let unique: BTreeSet<_> = values.iter().map(|score| score.metric).collect();
    if unique.len() != values.len() {
        return Err(ProposalError::DuplicateScoreMetric);
    }
    canonical_set(values.iter().map(ScoreComponent::to_value).collect())
}

fn canonical_constraint_set(
    values: &[ConstraintObservation],
) -> Result<CanonicalValue, ProposalError> {
    let unique: BTreeSet<_> = values.iter().map(|value| value.constraint_index).collect();
    if unique.len() != values.len() {
        return Err(ProposalError::DuplicateConstraintObservation);
    }
    canonical_set(
        values
            .iter()
            .copied()
            .map(ConstraintObservation::to_value)
            .collect(),
    )
}

fn canonical_set(values: Vec<CanonicalValue>) -> Result<CanonicalValue, ProposalError> {
    let values = values
        .into_iter()
        .map(|value| (value.clone(), value))
        .collect();
    Ok(CanonicalValue::Array(
        ku_core::foundation::canonicalize_set_by_key(values, ResourceProfile::ObjectV1)?,
    ))
}

fn reference_value(reference: &ObjectReference) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(reference.reference_kind)),
        (1, CanonicalValue::Bytes(reference.cid.to_vec())),
    ])
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProposalError {
    Canonical(ku_core::foundation::CanonicalError),
    Mapping(MappingError),
    ProposalMustRemainPrivate,
    InvalidExpiry,
    Limit,
    KernelEnvelopeMismatch,
    DuplicateScoreMetric,
    DuplicateConstraintObservation,
    ConstraintIndexOutOfRange,
}

impl From<ku_core::foundation::CanonicalError> for ProposalError {
    fn from(error: ku_core::foundation::CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<MappingError> for ProposalError {
    fn from(error: MappingError) -> Self {
        Self::Mapping(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ku_core::foundation::{
        ComparisonOperator, ConstraintExpression, CorrespondenceKind, MappingConstraintRegion,
        MappingSide, MappingTermLocator, MappingTransform, SemanticFrameSet, TermCorrespondence,
        TermRef, TypedConstraint, UnmappedRegion,
    };

    fn concept(byte: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([byte; 16])
    }

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn mapping_kernel() -> MappingKernel {
        let source = MappingTermLocator {
            object: reference(1),
            statement_index: 0,
            argument_index: Some(0),
        };
        let target = MappingTermLocator {
            object: reference(2),
            statement_index: 0,
            argument_index: Some(1),
        };
        MappingKernel {
            source_objects: vec![reference(1)],
            target_objects: vec![reference(2)],
            correspondences: vec![TermCorrespondence {
                source: source.clone(),
                target,
                kind: CorrespondenceKind::Analogous,
                transform: MappingTransform::Identity,
            }],
            assumptions: SemanticFrameSet {
                statements: Vec::new(),
            },
            constraint_regions: vec![
                MappingConstraintRegion {
                    constraint: TypedConstraint {
                        expression: ConstraintExpression::Compare {
                            left: TermRef::Concept(concept(40)),
                            operator: ComparisonOperator::Equal,
                            right: TermRef::Concept(concept(41)),
                        },
                        required: true,
                    },
                    evaluation: ConstraintEvaluation::Unknown,
                },
                MappingConstraintRegion {
                    constraint: TypedConstraint {
                        expression: ConstraintExpression::Compare {
                            left: TermRef::Concept(concept(42)),
                            operator: ComparisonOperator::Equal,
                            right: TermRef::Concept(concept(43)),
                        },
                        required: false,
                    },
                    evaluation: ConstraintEvaluation::Unknown,
                },
            ],
            unmapped_regions: vec![UnmappedRegion {
                side: MappingSide::Source,
                locator: source,
                reason: concept(9),
            }],
        }
    }

    fn proposal(constraints: Vec<ConstraintObservation>) -> BindingProposal {
        let kernel = mapping_kernel();
        let kernel_id = kernel.cid().unwrap();
        BindingProposal {
            mapping_kernel: kernel,
            proposed_envelope: MappingEnvelope {
                kernel: kernel_id,
                generator: reference(10),
                derivation_rule: Some(reference(11)),
                evidence: vec![reference(12)],
                source_event: None,
            },
            candidate_objects: vec![reference(1), reference(2)],
            index_commitment: Some(reference(20)),
            model_commitment: Some(reference(21)),
            rule_commitment: Some(reference(22)),
            scores: vec![ScoreComponent {
                metric: concept(30),
                value: ExactRatio::new(4, 5).unwrap(),
                direction: ScoreDirection::HigherIsBetter,
            }],
            constraints,
            expiry: ProposalExpiry {
                created_at_evaluation: 10,
                expires_after_evaluations: 5,
                source_frontier: EventCid::from_bytes([40; 32]),
            },
            privacy: DisclosureClass::LocalOnly,
        }
    }

    #[test]
    fn proposal_is_candidate_only_and_never_executable() {
        let mut store = ProposalQuarantine::default();
        let proposal = proposal(vec![ConstraintObservation {
            constraint_index: 0,
            evaluation: ConstraintEvaluation::Unknown,
            required: true,
        }]);
        assert_eq!(proposal.disposition(11), ProposalDisposition::CandidateOnly);
        let id = store.insert(proposal).unwrap();
        assert!(store.get(id).is_some());
        assert!(!store.is_executable());
    }

    #[test]
    fn proposal_quarantine_rejects_new_identity_at_capacity() {
        let mut store = ProposalQuarantine::with_capacity(1).unwrap();
        let first = proposal(Vec::new());
        let mut second = proposal(Vec::new());
        second.expiry.created_at_evaluation += 1;
        assert!(store.insert(first.clone()).is_ok());
        assert_eq!(store.len(), 1);
        assert!(store.insert(first).is_ok());
        assert_eq!(store.insert(second), Err(ProposalError::Limit));
        assert_eq!(
            ProposalQuarantine::with_capacity(0).err(),
            Some(ProposalError::Limit)
        );
    }

    #[test]
    fn hard_violation_blocks_action_but_preserves_proposal() {
        let mut store = ProposalQuarantine::default();
        let proposal = proposal(vec![ConstraintObservation {
            constraint_index: 0,
            evaluation: ConstraintEvaluation::Violated,
            required: true,
        }]);
        assert_eq!(
            proposal.disposition(11),
            ProposalDisposition::BlockedHardViolation
        );
        let id = store.insert(proposal).unwrap();
        assert!(store.get(id).is_some());
    }

    #[test]
    fn expiry_removes_only_ephemeral_proposal_not_source_references() {
        let mut store = ProposalQuarantine::default();
        let proposal = proposal(Vec::new());
        let sources = proposal.candidate_objects.clone();
        store.insert(proposal).unwrap();
        assert_eq!(store.expire(15), 1);
        assert_eq!(sources, vec![reference(1), reference(2)]);
    }

    #[test]
    fn public_or_kernel_mismatched_proposal_is_rejected() {
        let mut public = proposal(Vec::new());
        public.privacy = DisclosureClass::Public;
        assert_eq!(
            public.validate().unwrap_err(),
            ProposalError::ProposalMustRemainPrivate
        );

        let mut mismatched = proposal(Vec::new());
        mismatched.proposed_envelope.kernel = MappingKernelCid::from_bytes([99; 32]);
        assert_eq!(
            mismatched.validate().unwrap_err(),
            ProposalError::KernelEnvelopeMismatch
        );
    }

    #[test]
    fn score_vector_is_not_a_scalar_and_metric_duplicates_are_rejected() {
        let mut duplicate = proposal(Vec::new());
        duplicate.scores.push(duplicate.scores[0].clone());
        assert_eq!(
            duplicate.validate().unwrap_err(),
            ProposalError::DuplicateScoreMetric
        );
    }

    #[test]
    fn constraint_observations_are_bound_to_unique_kernel_regions() {
        let mut duplicate = proposal(vec![
            ConstraintObservation {
                constraint_index: 0,
                evaluation: ConstraintEvaluation::Satisfied,
                required: true,
            },
            ConstraintObservation {
                constraint_index: 0,
                evaluation: ConstraintEvaluation::Unknown,
                required: false,
            },
        ]);
        assert_eq!(
            duplicate.validate().unwrap_err(),
            ProposalError::DuplicateConstraintObservation
        );

        duplicate.constraints[1].constraint_index = 1;
        duplicate.validate().unwrap();
    }

    #[test]
    fn constraint_observation_cannot_point_outside_the_mapping_kernel() {
        let invalid = proposal(vec![ConstraintObservation {
            constraint_index: 2,
            evaluation: ConstraintEvaluation::Unknown,
            required: true,
        }]);
        assert_eq!(
            invalid.validate().unwrap_err(),
            ProposalError::ConstraintIndexOutOfRange
        );
    }
}
