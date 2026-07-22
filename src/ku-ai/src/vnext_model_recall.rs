//! Model-assisted candidate recall with a symbolic validity firewall.
//!
//! KGE, embedding and LLM adapters may add or rank candidates. They never
//! receive an API for declaring mapping validity, materialization or action.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use ku_core::foundation::{
    encode_canonical, CanonicalValue, ConceptCcid, ConstraintEvaluation, ExactRatio,
    MappingKernelCid, ObjectCid, ObjectReference, ResourceProfile, SelectorCid,
};

pub const MAX_RECALL_CANDIDATES: usize = 65_536;
pub const MAX_CANDIDATE_OBJECTS: usize = 4_096;
pub const MAX_SYMBOLIC_CHECKS: usize = 16_384;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecallCandidateSeed {
    candidate_id: [u8; 32],
    candidate_objects: Vec<ObjectReference>,
}

impl RecallCandidateSeed {
    pub fn new(mut candidate_objects: Vec<ObjectReference>) -> Result<Self, ModelRecallError> {
        if candidate_objects.is_empty() || candidate_objects.len() > MAX_CANDIDATE_OBJECTS {
            return Err(ModelRecallError::InvalidCandidate);
        }
        candidate_objects.sort_by_key(reference_key);
        candidate_objects.dedup_by_key(|reference| reference_key(reference));
        if candidate_objects
            .iter()
            .any(|reference| reference.cid == [0; 32])
        {
            return Err(ModelRecallError::InvalidCandidate);
        }
        let value = CanonicalValue::Array(
            candidate_objects
                .iter()
                .map(reference_value)
                .collect::<Vec<_>>(),
        );
        let bytes = encode_canonical(&value, ResourceProfile::ObjectV1)?;
        Ok(Self {
            candidate_id: digest_bytes(b"candidate", &bytes),
            candidate_objects,
        })
    }

    pub const fn candidate_id(&self) -> &[u8; 32] {
        &self.candidate_id
    }

    pub fn candidate_objects(&self) -> &[ObjectReference] {
        &self.candidate_objects
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelRecallEvidence {
    pub capability_definition: ObjectCid,
    pub implementation_manifest: ObjectReference,
    pub model_version: ObjectReference,
    pub invocation_commitment: [u8; 32],
    pub query_commitment: [u8; 32],
}

impl ModelRecallEvidence {
    fn validate(&self, query_commitment: [u8; 32]) -> Result<(), ModelRecallError> {
        if self.capability_definition.as_bytes() == &[0; 32]
            || self.implementation_manifest.cid == [0; 32]
            || self.model_version.cid == [0; 32]
            || self.invocation_commitment == [0; 32]
            || self.query_commitment == [0; 32]
            || self.query_commitment != query_commitment
        {
            Err(ModelRecallError::InvalidModelEvidence)
        } else {
            Ok(())
        }
    }

    pub const fn grants_authority(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelScoredCandidate {
    pub candidate: RecallCandidateSeed,
    pub recall_score: ExactRatio,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelRecallPage {
    pub evidence: ModelRecallEvidence,
    pub candidates: Vec<ModelScoredCandidate>,
    pub consumed_work_units: u64,
    pub continuation: Option<[u8; 32]>,
}

#[derive(Clone, Copy, Debug)]
pub struct ModelRecallRequest {
    pub query_commitment: [u8; 32],
    pub selector: SelectorCid,
    pub max_candidates: usize,
    pub max_work_units: u64,
    pub continuation: Option<[u8; 32]>,
}

impl ModelRecallRequest {
    fn validate(self) -> Result<Self, ModelRecallError> {
        if self.query_commitment == [0; 32]
            || self.selector.as_bytes() == &[0; 32]
            || self.max_candidates == 0
            || self.max_candidates > MAX_RECALL_CANDIDATES
            || self.max_work_units == 0
            || self.continuation == Some([0; 32])
        {
            Err(ModelRecallError::InvalidRequest)
        } else {
            Ok(self)
        }
    }
}

pub trait CandidateRecallAdapter {
    fn recall(&mut self, request: ModelRecallRequest) -> Result<ModelRecallPage, ModelRecallError>;
}

#[derive(Clone, Copy, Debug)]
pub struct SymbolicValidationRequest<'a> {
    pub candidate_id: [u8; 32],
    pub candidate_objects: &'a [ObjectReference],
    pub validation_context: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolicCheck {
    pub check_kind: ConceptCcid,
    pub evaluation: ConstraintEvaluation,
    pub required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolicMappingAssessment {
    pub candidate_id: [u8; 32],
    pub mapping_kernel: MappingKernelCid,
    pub validation_context: [u8; 32],
    pub validator_version: ObjectReference,
    pub checks: Vec<SymbolicCheck>,
}

impl SymbolicMappingAssessment {
    fn validate(&self, request: SymbolicValidationRequest<'_>) -> Result<(), ModelRecallError> {
        if self.candidate_id != request.candidate_id
            || self.validation_context != request.validation_context
            || self.mapping_kernel.as_bytes() == &[0; 32]
            || self.validator_version.cid == [0; 32]
            || self.checks.is_empty()
            || self.checks.len() > MAX_SYMBOLIC_CHECKS
        {
            return Err(ModelRecallError::InvalidSymbolicAssessment);
        }
        let mut kinds = BTreeSet::new();
        if self
            .checks
            .iter()
            .any(|check| !kinds.insert(check.check_kind))
        {
            return Err(ModelRecallError::InvalidSymbolicAssessment);
        }
        Ok(())
    }

    pub fn disposition(&self) -> SymbolicDisposition {
        if self
            .checks
            .iter()
            .any(|check| check.required && check.evaluation == ConstraintEvaluation::Violated)
        {
            SymbolicDisposition::RejectedRequiredViolation
        } else if self
            .checks
            .iter()
            .any(|check| check.required && check.evaluation == ConstraintEvaluation::Unknown)
        {
            SymbolicDisposition::DeferredRequiredUnknown
        } else {
            SymbolicDisposition::EligibleProposalCandidate
        }
    }
}

pub trait SymbolicMappingValidator {
    fn validate_mapping(
        &mut self,
        request: SymbolicValidationRequest<'_>,
    ) -> Result<SymbolicMappingAssessment, ModelRecallError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolicDisposition {
    EligibleProposalCandidate,
    DeferredRequiredUnknown,
    RejectedRequiredViolation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecallOrigin {
    DeterministicSeed,
    ModelAdapter,
    Both,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecallEvaluation {
    pub candidate: RecallCandidateSeed,
    pub origin: RecallOrigin,
    pub model_score: Option<ExactRatio>,
    pub assessment: SymbolicMappingAssessment,
    pub disposition: SymbolicDisposition,
}

impl RecallEvaluation {
    pub const fn is_materialization_authority(&self) -> bool {
        false
    }

    pub const fn is_execution_authority(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelRecallResult {
    pub model_used: bool,
    pub model_evidence: Option<ModelRecallEvidence>,
    pub evaluations: Vec<RecallEvaluation>,
    pub continuation: Option<[u8; 32]>,
    pub consumed_work_units: u64,
}

impl ModelRecallResult {
    pub fn assessment_for(&self, mapping: MappingKernelCid) -> Option<&SymbolicMappingAssessment> {
        self.evaluations
            .iter()
            .find(|evaluation| evaluation.assessment.mapping_kernel == mapping)
            .map(|evaluation| &evaluation.assessment)
    }

    pub const fn can_adopt_or_execute(&self) -> bool {
        false
    }
}

pub struct ModelRecallFirewall;

impl ModelRecallFirewall {
    pub fn discover(
        request: ModelRecallRequest,
        validation_context: [u8; 32],
        deterministic_seeds: Vec<RecallCandidateSeed>,
        model_adapter: Option<&mut dyn CandidateRecallAdapter>,
        validator: &mut dyn SymbolicMappingValidator,
    ) -> Result<ModelRecallResult, ModelRecallError> {
        let request = request.validate()?;
        if validation_context == [0; 32]
            || deterministic_seeds.len() > request.max_candidates
            || deterministic_seeds.len() > MAX_RECALL_CANDIDATES
        {
            return Err(ModelRecallError::InvalidRequest);
        }
        let mut candidates = BTreeMap::<[u8; 32], CandidateAggregate>::new();
        for candidate in deterministic_seeds {
            insert_candidate(&mut candidates, candidate, CandidateSource::Seed, None)?;
        }

        let mut model_evidence = None;
        let mut continuation = None;
        let mut consumed_work_units = 0;
        if let Some(adapter) = model_adapter {
            let page = adapter.recall(request)?;
            page.evidence.validate(request.query_commitment)?;
            if page.candidates.len() > request.max_candidates
                || page.consumed_work_units > request.max_work_units
                || page.continuation == Some([0; 32])
            {
                return Err(ModelRecallError::ModelBudgetExceeded);
            }
            for scored in page.candidates {
                insert_candidate(
                    &mut candidates,
                    scored.candidate,
                    CandidateSource::Model,
                    Some(scored.recall_score),
                )?;
            }
            model_evidence = Some(page.evidence);
            continuation = page.continuation;
            consumed_work_units = page.consumed_work_units;
        }
        if candidates.len() > request.max_candidates {
            return Err(ModelRecallError::CandidateBudgetExceeded);
        }

        let mut ordered = candidates.into_values().collect::<Vec<_>>();
        ordered.sort_by(|left, right| {
            model_score_cmp(right.model_score, left.model_score).then_with(|| {
                left.candidate
                    .candidate_id
                    .cmp(&right.candidate.candidate_id)
            })
        });

        let mut evaluations = Vec::with_capacity(ordered.len());
        for aggregate in ordered {
            let symbolic_request = SymbolicValidationRequest {
                candidate_id: *aggregate.candidate.candidate_id(),
                candidate_objects: aggregate.candidate.candidate_objects(),
                validation_context,
            };
            let assessment = validator.validate_mapping(symbolic_request)?;
            assessment.validate(symbolic_request)?;
            let disposition = assessment.disposition();
            evaluations.push(RecallEvaluation {
                candidate: aggregate.candidate,
                origin: aggregate.source.origin(),
                model_score: aggregate.model_score,
                assessment,
                disposition,
            });
        }
        Ok(ModelRecallResult {
            model_used: model_evidence.is_some(),
            model_evidence,
            evaluations,
            continuation,
            consumed_work_units,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CandidateSource {
    Seed,
    Model,
    Both,
}

impl CandidateSource {
    const fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Seed, Self::Seed) => Self::Seed,
            (Self::Model, Self::Model) => Self::Model,
            _ => Self::Both,
        }
    }

    const fn origin(self) -> RecallOrigin {
        match self {
            Self::Seed => RecallOrigin::DeterministicSeed,
            Self::Model => RecallOrigin::ModelAdapter,
            Self::Both => RecallOrigin::Both,
        }
    }
}

struct CandidateAggregate {
    candidate: RecallCandidateSeed,
    source: CandidateSource,
    model_score: Option<ExactRatio>,
}

fn insert_candidate(
    candidates: &mut BTreeMap<[u8; 32], CandidateAggregate>,
    candidate: RecallCandidateSeed,
    source: CandidateSource,
    score: Option<ExactRatio>,
) -> Result<(), ModelRecallError> {
    let id = *candidate.candidate_id();
    match candidates.get_mut(&id) {
        Some(existing) if existing.candidate != candidate => {
            Err(ModelRecallError::CandidateIdentityConflict)
        }
        Some(existing) => {
            existing.source = existing.source.merge(source);
            if let Some(score) = score {
                existing.model_score = match existing.model_score {
                    Some(current) if ratio_cmp(current, score) != Ordering::Less => Some(current),
                    _ => Some(score),
                };
            }
            Ok(())
        }
        None => {
            candidates.insert(
                id,
                CandidateAggregate {
                    candidate,
                    source,
                    model_score: score,
                },
            );
            Ok(())
        }
    }
}

fn model_score_cmp(left: Option<ExactRatio>, right: Option<ExactRatio>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => ratio_cmp(left, right),
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => Ordering::Equal,
    }
}

fn ratio_cmp(left: ExactRatio, right: ExactRatio) -> Ordering {
    (i128::from(left.numerator()) * i128::from(right.denominator()))
        .cmp(&(i128::from(right.numerator()) * i128::from(left.denominator())))
}

fn reference_key(reference: &ObjectReference) -> (u64, [u8; 32]) {
    (reference.reference_kind, reference.cid)
}

fn reference_value(reference: &ObjectReference) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(reference.reference_kind)),
        (1, CanonicalValue::Bytes(reference.cid.to_vec())),
    ])
}

fn digest_bytes(label: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:model-recall-firewall:1\0");
    hasher.update(label);
    hasher.update(&[0]);
    hasher.update(&(bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

#[derive(Debug, PartialEq, Eq)]
pub enum ModelRecallError {
    Canonical(ku_core::foundation::CanonicalError),
    InvalidCandidate,
    InvalidRequest,
    InvalidModelEvidence,
    InvalidSymbolicAssessment,
    ModelBudgetExceeded,
    CandidateBudgetExceeded,
    CandidateIdentityConflict,
    Adapter(&'static str),
    Validator(&'static str),
}

impl From<ku_core::foundation::CanonicalError> for ModelRecallError {
    fn from(error: ku_core::foundation::CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn seed(byte: u8) -> RecallCandidateSeed {
        RecallCandidateSeed::new(vec![reference(byte)]).unwrap()
    }

    fn request() -> ModelRecallRequest {
        ModelRecallRequest {
            query_commitment: [1; 32],
            selector: SelectorCid::from_bytes([2; 32]),
            max_candidates: 10,
            max_work_units: 100,
            continuation: None,
        }
    }

    struct FakeAdapter {
        candidates: Vec<ModelScoredCandidate>,
        calls: usize,
    }

    impl CandidateRecallAdapter for FakeAdapter {
        fn recall(
            &mut self,
            request: ModelRecallRequest,
        ) -> Result<ModelRecallPage, ModelRecallError> {
            self.calls += 1;
            Ok(ModelRecallPage {
                evidence: ModelRecallEvidence {
                    capability_definition: ObjectCid::from_bytes([3; 32]),
                    implementation_manifest: reference(4),
                    model_version: reference(5),
                    invocation_commitment: [6; 32],
                    query_commitment: request.query_commitment,
                },
                candidates: self.candidates.clone(),
                consumed_work_units: 5,
                continuation: None,
            })
        }
    }

    struct FakeValidator;

    impl SymbolicMappingValidator for FakeValidator {
        fn validate_mapping(
            &mut self,
            request: SymbolicValidationRequest<'_>,
        ) -> Result<SymbolicMappingAssessment, ModelRecallError> {
            let marker = request.candidate_objects[0].cid[0];
            let evaluation = match marker {
                8 => ConstraintEvaluation::Violated,
                9 => ConstraintEvaluation::Unknown,
                _ => ConstraintEvaluation::Satisfied,
            };
            Ok(SymbolicMappingAssessment {
                candidate_id: request.candidate_id,
                mapping_kernel: MappingKernelCid::from_bytes([marker; 32]),
                validation_context: request.validation_context,
                validator_version: reference(20),
                checks: vec![SymbolicCheck {
                    check_kind: ConceptCcid::from_bytes([21; 16]),
                    evaluation,
                    required: true,
                }],
            })
        }
    }

    #[test]
    fn model_off_ablation_changes_recall_or_rank_not_common_mapping_validity() {
        let baseline = vec![seed(7), seed(9)];
        let off = ModelRecallFirewall::discover(
            request(),
            [30; 32],
            baseline.clone(),
            None,
            &mut FakeValidator,
        )
        .unwrap();
        let mut adapter = FakeAdapter {
            candidates: vec![
                ModelScoredCandidate {
                    candidate: seed(7),
                    recall_score: ExactRatio::new(1, 10).unwrap(),
                },
                ModelScoredCandidate {
                    candidate: seed(6),
                    recall_score: ExactRatio::new(9, 10).unwrap(),
                },
            ],
            calls: 0,
        };
        let on = ModelRecallFirewall::discover(
            request(),
            [30; 32],
            baseline,
            Some(&mut adapter),
            &mut FakeValidator,
        )
        .unwrap();
        assert_eq!(adapter.calls, 1);
        assert_eq!(off.evaluations.len(), 2);
        assert_eq!(on.evaluations.len(), 3);
        assert_eq!(
            on.evaluations[0].assessment.mapping_kernel.as_bytes(),
            &[6; 32]
        );
        let common = MappingKernelCid::from_bytes([7; 32]);
        assert_eq!(off.assessment_for(common), on.assessment_for(common));
        assert_eq!(
            on.assessment_for(common).unwrap().disposition(),
            SymbolicDisposition::EligibleProposalCandidate
        );
    }

    #[test]
    fn arbitrarily_high_model_score_cannot_override_required_violation() {
        let mut adapter = FakeAdapter {
            candidates: vec![ModelScoredCandidate {
                candidate: seed(8),
                recall_score: ExactRatio::integer(i64::MAX),
            }],
            calls: 0,
        };
        let result = ModelRecallFirewall::discover(
            request(),
            [30; 32],
            Vec::new(),
            Some(&mut adapter),
            &mut FakeValidator,
        )
        .unwrap();
        assert_eq!(
            result.evaluations[0].disposition,
            SymbolicDisposition::RejectedRequiredViolation
        );
        assert!(!result.can_adopt_or_execute());
        assert!(!result.evaluations[0].is_materialization_authority());
    }

    #[test]
    fn required_unknown_is_deferred_not_coerced_to_false() {
        let result = ModelRecallFirewall::discover(
            request(),
            [30; 32],
            vec![seed(9)],
            None,
            &mut FakeValidator,
        )
        .unwrap();
        assert_eq!(
            result.evaluations[0].disposition,
            SymbolicDisposition::DeferredRequiredUnknown
        );
    }

    struct PanicAdapter;

    impl CandidateRecallAdapter for PanicAdapter {
        fn recall(
            &mut self,
            _request: ModelRecallRequest,
        ) -> Result<ModelRecallPage, ModelRecallError> {
            panic!("disabled model adapter must not be called")
        }
    }

    #[test]
    fn offline_disabled_model_path_never_calls_adapter() {
        let mut adapter = PanicAdapter;
        let result = ModelRecallFirewall::discover(
            request(),
            [30; 32],
            vec![seed(7)],
            None,
            &mut FakeValidator,
        )
        .unwrap();
        let _ = &mut adapter;
        assert!(!result.model_used);
        assert_eq!(result.evaluations.len(), 1);
    }

    #[test]
    fn mismatched_model_query_binding_fails_before_symbolic_output() {
        let mut adapter = FakeAdapter {
            candidates: vec![ModelScoredCandidate {
                candidate: seed(7),
                recall_score: ExactRatio::integer(1),
            }],
            calls: 0,
        };
        struct WrongBinding<'a>(&'a mut FakeAdapter);
        impl CandidateRecallAdapter for WrongBinding<'_> {
            fn recall(
                &mut self,
                request: ModelRecallRequest,
            ) -> Result<ModelRecallPage, ModelRecallError> {
                let mut page = self.0.recall(request)?;
                page.evidence.query_commitment = [99; 32];
                Ok(page)
            }
        }
        assert_eq!(
            ModelRecallFirewall::discover(
                request(),
                [30; 32],
                vec![seed(7)],
                Some(&mut WrongBinding(&mut adapter)),
                &mut FakeValidator
            )
            .unwrap_err(),
            ModelRecallError::InvalidModelEvidence
        );
    }
}
