//! Exact typed Receptor-to-Affordance matcher with three-state validation.

use std::cmp::Ordering;

use ku_core::foundation::{
    AffordanceSemantics, ComparisonOperator, ConceptCcid, ConstraintEvaluation,
    ConstraintExpression, CorrespondenceKind, DisclosureClass, EventCid, ExactRatio,
    KnowledgeAffordance, LiteralValue, MappingConstraintRegion, MappingEnvelope, MappingError,
    MappingKernel, MappingSide, MappingTermLocator, MappingTransform, Modality, ObjectReference,
    QuantityLiteral, ReceptorDefinition, SemanticFrameSet, StatementFrame, TermCorrespondence,
    TermRef, TypedConstraint, UnmappedRegion,
};

use crate::vnext_proposal::{
    BindingProposal, ConstraintObservation, ProposalError, ProposalExpiry, ScoreComponent,
    ScoreDirection,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchCheckKind {
    OfferedRole,
    RelationStructure,
    ArgumentDirection,
    ArgumentType,
    Negation,
    Modality,
    Time,
    UnitDimension,
    TypedConstraint,
    Applicability,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchCheck {
    pub kind: MatchCheckKind,
    pub evaluation: ConstraintEvaluation,
    pub required: bool,
    pub source_statement: Option<u32>,
    pub target_statement: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatcherMetricConcepts {
    pub structural_fit: ConceptCcid,
    pub constraint_fit: ConceptCcid,
}

#[derive(Clone, Debug)]
pub struct TypedMatchRequest<'a> {
    pub receptor_reference: ObjectReference,
    pub receptor: &'a ReceptorDefinition,
    pub required_semantics: &'a SemanticFrameSet,
    pub local_context: &'a SemanticFrameSet,
    pub affordance_reference: ObjectReference,
    pub affordance: &'a KnowledgeAffordance,
    pub generator: ObjectReference,
    pub derivation_rule: Option<ObjectReference>,
    pub evidence: Vec<ObjectReference>,
    pub index_commitment: Option<ObjectReference>,
    pub rule_commitment: Option<ObjectReference>,
    pub metrics: MatcherMetricConcepts,
    pub unmapped_reason: ConceptCcid,
    pub source_frontier: EventCid,
    pub created_at_evaluation: u64,
    pub expires_after_evaluations: u64,
    pub privacy: DisclosureClass,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatcherOutcome {
    Proposal {
        proposal: Box<BindingProposal>,
        checks: Vec<MatchCheck>,
    },
    HardMismatch {
        checks: Vec<MatchCheck>,
    },
}

impl MatcherOutcome {
    pub fn proposal(&self) -> Option<&BindingProposal> {
        match self {
            Self::Proposal { proposal, .. } => Some(proposal),
            Self::HardMismatch { .. } => None,
        }
    }

    pub fn checks(&self) -> &[MatchCheck] {
        match self {
            Self::Proposal { checks, .. } | Self::HardMismatch { checks } => checks,
        }
    }
}

pub struct ExactTypedMatcher;

impl ExactTypedMatcher {
    pub fn match_affordance(
        request: TypedMatchRequest<'_>,
    ) -> Result<MatcherOutcome, MatcherError> {
        if !matches!(
            request.privacy,
            DisclosureClass::LocalOnly | DisclosureClass::NegotiatedEncrypted
        ) {
            return Err(MatcherError::ProposalMustRemainPrivate);
        }

        let mut checks = vec![MatchCheck {
            kind: MatchCheckKind::OfferedRole,
            evaluation: if request.affordance.supports_role(request.receptor.role) {
                ConstraintEvaluation::Satisfied
            } else {
                ConstraintEvaluation::Violated
            },
            required: true,
            source_statement: None,
            target_statement: None,
        }];
        let targets = flatten_semantics(&request.affordance.semantics);
        let mut correspondences = Vec::new();
        let mut unmapped_regions = Vec::new();

        for (source_index, required) in request.required_semantics.statements.iter().enumerate() {
            let best = targets
                .iter()
                .enumerate()
                .filter(|(_, target)| {
                    required.operator_or_predicate == target.operator_or_predicate
                })
                .map(|(target_index, target)| {
                    assess_statement(required, target, source_index as u32, target_index as u32)
                })
                .max_by_key(StatementAssessment::quality);
            if let Some(best) = best {
                checks.extend(best.checks);
                correspondences.extend(build_correspondences(
                    &request.receptor_reference,
                    &request.affordance_reference,
                    source_index as u32,
                    best.target_index,
                    required,
                    best.target,
                )?);
            } else {
                checks.push(MatchCheck {
                    kind: MatchCheckKind::RelationStructure,
                    evaluation: ConstraintEvaluation::Violated,
                    required: true,
                    source_statement: Some(source_index as u32),
                    target_statement: None,
                });
                unmapped_regions.push(UnmappedRegion {
                    side: MappingSide::Source,
                    locator: MappingTermLocator {
                        object: request.receptor_reference.clone(),
                        statement_index: source_index as u32,
                        argument_index: None,
                    },
                    reason: request.unmapped_reason,
                });
            }
        }

        let mut typed_constraints = request.receptor.hard_constraints.clone();
        typed_constraints.extend(
            request
                .required_semantics
                .statements
                .iter()
                .flat_map(|statement| statement.constraints.iter().cloned()),
        );
        let mut constraint_regions = Vec::new();
        let mut observations = Vec::new();
        for (index, constraint) in typed_constraints.into_iter().enumerate() {
            let evaluation = evaluate_constraint(&constraint);
            checks.push(MatchCheck {
                kind: MatchCheckKind::TypedConstraint,
                evaluation,
                required: constraint.required,
                source_statement: None,
                target_statement: None,
            });
            observations.push(ConstraintObservation {
                constraint_index: index as u32,
                evaluation,
                required: constraint.required,
            });
            constraint_regions.push(MappingConstraintRegion {
                constraint,
                evaluation,
            });
        }
        checks.extend(check_applicability(
            &request.affordance.semantics.preconditions,
            request.local_context,
        ));

        if checks
            .iter()
            .any(|check| check.required && check.evaluation == ConstraintEvaluation::Violated)
        {
            return Ok(MatcherOutcome::HardMismatch { checks });
        }

        let kernel = MappingKernel {
            source_objects: vec![request.receptor_reference.clone()],
            target_objects: vec![request.affordance_reference.clone()],
            correspondences,
            assumptions: request.local_context.clone(),
            constraint_regions,
            unmapped_regions,
        };
        let kernel_id = kernel.cid()?;
        let proposal = BindingProposal {
            mapping_kernel: kernel,
            proposed_envelope: MappingEnvelope {
                kernel: kernel_id,
                generator: request.generator,
                derivation_rule: request.derivation_rule,
                evidence: request.evidence,
                source_event: None,
            },
            candidate_objects: vec![request.affordance_reference],
            index_commitment: request.index_commitment,
            model_commitment: None,
            rule_commitment: request.rule_commitment,
            scores: scores(&checks, request.metrics)?,
            constraints: observations,
            expiry: ProposalExpiry {
                created_at_evaluation: request.created_at_evaluation,
                expires_after_evaluations: request.expires_after_evaluations,
                source_frontier: request.source_frontier,
            },
            privacy: request.privacy,
        };
        proposal.validate()?;
        Ok(MatcherOutcome::Proposal {
            proposal: Box::new(proposal),
            checks,
        })
    }
}

struct StatementAssessment<'a> {
    target_index: u32,
    target: &'a StatementFrame,
    checks: Vec<MatchCheck>,
}

impl StatementAssessment<'_> {
    fn quality(&self) -> (u8, usize, std::cmp::Reverse<u32>) {
        let worst = self
            .checks
            .iter()
            .map(|check| evaluation_quality(check.evaluation))
            .min()
            .unwrap_or(2);
        let satisfied = self
            .checks
            .iter()
            .filter(|check| check.evaluation == ConstraintEvaluation::Satisfied)
            .count();
        (worst, satisfied, std::cmp::Reverse(self.target_index))
    }
}

fn assess_statement<'a>(
    required: &StatementFrame,
    candidate: &'a StatementFrame,
    source_index: u32,
    target_index: u32,
) -> StatementAssessment<'a> {
    let at = |kind, evaluation, required| MatchCheck {
        kind,
        evaluation,
        required,
        source_statement: Some(source_index),
        target_statement: Some(target_index),
    };
    let mut checks = vec![at(
        MatchCheckKind::RelationStructure,
        bool_evaluation(required.arguments.len() == candidate.arguments.len()),
        true,
    )];
    let reversed = required.arguments.len() == 2
        && candidate.arguments.len() == 2
        && term_compatibility(&required.arguments[0], &candidate.arguments[1])
            == ConstraintEvaluation::Satisfied
        && term_compatibility(&required.arguments[1], &candidate.arguments[0])
            == ConstraintEvaluation::Satisfied
        && !same_arguments(required, candidate);
    checks.push(at(
        MatchCheckKind::ArgumentDirection,
        bool_evaluation(!reversed),
        true,
    ));
    for (left, right) in required.arguments.iter().zip(&candidate.arguments) {
        checks.push(at(
            if quantity(left).is_some() || quantity(right).is_some() {
                MatchCheckKind::UnitDimension
            } else {
                MatchCheckKind::ArgumentType
            },
            term_compatibility(left, right),
            true,
        ));
    }
    checks.push(at(
        MatchCheckKind::Negation,
        bool_evaluation(required.qualifiers.negated == candidate.qualifiers.negated),
        true,
    ));
    checks.push(at(
        MatchCheckKind::Modality,
        modality_compatibility(required.qualifiers.modality, candidate.qualifiers.modality),
        true,
    ));
    checks.push(at(
        MatchCheckKind::Time,
        optional_term_compatibility(
            required.qualifiers.time.as_ref(),
            candidate.qualifiers.time.as_ref(),
        ),
        required.qualifiers.time.is_some(),
    ));
    StatementAssessment {
        target_index,
        target: candidate,
        checks,
    }
}

fn build_correspondences(
    source_object: &ObjectReference,
    target_object: &ObjectReference,
    source_index: u32,
    target_index: u32,
    source: &StatementFrame,
    target: &StatementFrame,
) -> Result<Vec<TermCorrespondence>, MatcherError> {
    source
        .arguments
        .iter()
        .zip(&target.arguments)
        .enumerate()
        .filter(|(_, (left, right))| {
            term_compatibility(left, right) != ConstraintEvaluation::Violated
        })
        .map(|(index, (left, right))| {
            Ok(TermCorrespondence {
                source: MappingTermLocator {
                    object: source_object.clone(),
                    statement_index: source_index,
                    argument_index: Some(index as u32),
                },
                target: MappingTermLocator {
                    object: target_object.clone(),
                    statement_index: target_index,
                    argument_index: Some(index as u32),
                },
                kind: CorrespondenceKind::Equivalent,
                transform: unit_transform(left, right)?,
            })
        })
        .collect()
}

fn unit_transform(left: &TermRef, right: &TermRef) -> Result<MappingTransform, MatcherError> {
    let (Some(left), Some(right)) = (quantity(left), quantity(right)) else {
        return Ok(MappingTransform::Identity);
    };
    if left.source_unit.dimension != right.source_unit.dimension
        || left.source_unit == right.source_unit
    {
        return Ok(MappingTransform::Identity);
    }
    let scale = left
        .source_unit
        .scale_to_base
        .checked_div(right.source_unit.scale_to_base)?;
    let negated_target = right
        .source_unit
        .offset_to_base
        .numerator()
        .checked_neg()
        .ok_or(MatcherError::ArithmeticOverflow)?;
    let offset = left
        .source_unit
        .offset_to_base
        .checked_add(ExactRatio::new(
            negated_target,
            right.source_unit.offset_to_base.denominator(),
        )?)?
        .checked_div(right.source_unit.scale_to_base)?;
    Ok(MappingTransform::AffineUnit {
        source_dimension: left.source_unit.dimension,
        target_dimension: right.source_unit.dimension,
        scale,
        offset,
    })
}

fn check_applicability(
    preconditions: &SemanticFrameSet,
    context: &SemanticFrameSet,
) -> Vec<MatchCheck> {
    preconditions
        .statements
        .iter()
        .enumerate()
        .map(|(index, required)| {
            let matching = context
                .statements
                .iter()
                .filter(|candidate| {
                    candidate.operator_or_predicate == required.operator_or_predicate
                        && same_arguments(required, candidate)
                })
                .collect::<Vec<_>>();
            let evaluation = if matching
                .iter()
                .any(|candidate| candidate.qualifiers.negated != required.qualifiers.negated)
            {
                ConstraintEvaluation::Violated
            } else if matching
                .iter()
                .any(|candidate| candidate.qualifiers.negated == required.qualifiers.negated)
            {
                ConstraintEvaluation::Satisfied
            } else {
                ConstraintEvaluation::Unknown
            };
            MatchCheck {
                kind: MatchCheckKind::Applicability,
                evaluation,
                required: true,
                source_statement: Some(index as u32),
                target_statement: None,
            }
        })
        .collect()
}

fn same_arguments(left: &StatementFrame, right: &StatementFrame) -> bool {
    left.arguments.len() == right.arguments.len()
        && left
            .arguments
            .iter()
            .zip(&right.arguments)
            .all(|(left, right)| term_compatibility(left, right) == ConstraintEvaluation::Satisfied)
}

fn term_compatibility(left: &TermRef, right: &TermRef) -> ConstraintEvaluation {
    match (left, right) {
        (TermRef::Concept(left), TermRef::Concept(right)) => bool_evaluation(left == right),
        (
            TermRef::Variable {
                type_constraint: left,
                ..
            },
            TermRef::Variable {
                type_constraint: right,
                ..
            },
        )
        | (
            TermRef::Receptor {
                expected_type: left,
                ..
            },
            TermRef::Receptor {
                expected_type: right,
                ..
            },
        ) => match (left, right) {
            (Some(left), Some(right)) => bool_evaluation(left == right),
            _ => ConstraintEvaluation::Unknown,
        },
        (
            TermRef::Literal(LiteralValue::Quantity(left)),
            TermRef::Literal(LiteralValue::Quantity(right)),
        ) => {
            if left.source_unit.dimension != right.source_unit.dimension {
                ConstraintEvaluation::Violated
            } else {
                match left.compare(right) {
                    Ok(Ordering::Equal) => ConstraintEvaluation::Satisfied,
                    Ok(_) | Err(_) => ConstraintEvaluation::Unknown,
                }
            }
        }
        (TermRef::Literal(left), TermRef::Literal(right)) => bool_evaluation(left == right),
        (TermRef::KnowledgeObject(left), TermRef::KnowledgeObject(right)) => {
            bool_evaluation(left == right)
        }
        (TermRef::Statement(left), TermRef::Statement(right)) => bool_evaluation(left == right),
        (TermRef::Variable { .. }, _) | (TermRef::Receptor { .. }, _) => {
            ConstraintEvaluation::Unknown
        }
        _ => ConstraintEvaluation::Violated,
    }
}

fn optional_term_compatibility(
    required: Option<&TermRef>,
    candidate: Option<&TermRef>,
) -> ConstraintEvaluation {
    match (required, candidate) {
        (None, _) => ConstraintEvaluation::Satisfied,
        (Some(_), None) => ConstraintEvaluation::Unknown,
        (Some(required), Some(candidate)) => term_compatibility(required, candidate),
    }
}

fn modality_compatibility(required: Modality, candidate: Modality) -> ConstraintEvaluation {
    if required == candidate {
        ConstraintEvaluation::Satisfied
    } else {
        ConstraintEvaluation::Unknown
    }
}

fn evaluate_constraint(constraint: &TypedConstraint) -> ConstraintEvaluation {
    match &constraint.expression {
        ConstraintExpression::Compare {
            left,
            operator,
            right,
        } => evaluate_comparison(left, *operator, right),
        ConstraintExpression::Dimension { term, expected } => quantity(term)
            .map(|quantity| bool_evaluation(quantity.source_unit.dimension == *expected))
            .unwrap_or(ConstraintEvaluation::Unknown),
        ConstraintExpression::Range {
            term,
            lower,
            upper,
            include_lower,
            include_upper,
        } => evaluate_range(term, lower, upper, *include_lower, *include_upper),
    }
}

fn evaluate_comparison(
    left: &TermRef,
    operator: ComparisonOperator,
    right: &TermRef,
) -> ConstraintEvaluation {
    match (left, right) {
        (
            TermRef::Literal(LiteralValue::Quantity(left)),
            TermRef::Literal(LiteralValue::Quantity(right)),
        ) => {
            if left.source_unit.dimension != right.source_unit.dimension {
                return ConstraintEvaluation::Violated;
            }
            left.compare(right)
                .map(|ordering| compare_ordering(ordering, operator))
                .map(bool_evaluation)
                .unwrap_or(ConstraintEvaluation::Unknown)
        }
        (TermRef::Concept(left), TermRef::Concept(right)) => match operator {
            ComparisonOperator::Equal => bool_evaluation(left == right),
            ComparisonOperator::NotEqual => bool_evaluation(left != right),
            _ => ConstraintEvaluation::Unknown,
        },
        (TermRef::Literal(left), TermRef::Literal(right)) => match operator {
            ComparisonOperator::Equal => bool_evaluation(left == right),
            ComparisonOperator::NotEqual => bool_evaluation(left != right),
            _ => ConstraintEvaluation::Unknown,
        },
        _ => ConstraintEvaluation::Unknown,
    }
}

fn compare_ordering(ordering: Ordering, operator: ComparisonOperator) -> bool {
    match operator {
        ComparisonOperator::Equal => ordering == Ordering::Equal,
        ComparisonOperator::NotEqual => ordering != Ordering::Equal,
        ComparisonOperator::LessThan => ordering == Ordering::Less,
        ComparisonOperator::LessThanOrEqual => ordering != Ordering::Greater,
        ComparisonOperator::GreaterThan => ordering == Ordering::Greater,
        ComparisonOperator::GreaterThanOrEqual => ordering != Ordering::Less,
    }
}

fn evaluate_range(
    term: &TermRef,
    lower: &QuantityLiteral,
    upper: &QuantityLiteral,
    include_lower: bool,
    include_upper: bool,
) -> ConstraintEvaluation {
    let Some(value) = quantity(term) else {
        return ConstraintEvaluation::Unknown;
    };
    if value.source_unit.dimension != lower.source_unit.dimension
        || value.source_unit.dimension != upper.source_unit.dimension
    {
        return ConstraintEvaluation::Violated;
    }
    let (Ok(lower_cmp), Ok(upper_cmp)) = (value.compare(lower), value.compare(upper)) else {
        return ConstraintEvaluation::Unknown;
    };
    bool_evaluation(
        (lower_cmp == Ordering::Greater || (include_lower && lower_cmp == Ordering::Equal))
            && (upper_cmp == Ordering::Less || (include_upper && upper_cmp == Ordering::Equal)),
    )
}

fn scores(
    checks: &[MatchCheck],
    metrics: MatcherMetricConcepts,
) -> Result<Vec<ScoreComponent>, MatcherError> {
    let structural = checks
        .iter()
        .filter(|check| check.kind != MatchCheckKind::TypedConstraint)
        .collect::<Vec<_>>();
    let constraints = checks
        .iter()
        .filter(|check| check.kind == MatchCheckKind::TypedConstraint)
        .collect::<Vec<_>>();
    Ok(vec![
        ScoreComponent {
            metric: metrics.structural_fit,
            value: satisfied_fraction(&structural)?,
            direction: ScoreDirection::HigherIsBetter,
        },
        ScoreComponent {
            metric: metrics.constraint_fit,
            value: satisfied_fraction(&constraints)?,
            direction: ScoreDirection::HigherIsBetter,
        },
    ])
}

fn satisfied_fraction(checks: &[&MatchCheck]) -> Result<ExactRatio, MatcherError> {
    if checks.is_empty() {
        return Ok(ExactRatio::integer(1));
    }
    let satisfied = checks
        .iter()
        .filter(|check| check.evaluation == ConstraintEvaluation::Satisfied)
        .count();
    Ok(ExactRatio::new(satisfied as i64, checks.len() as u64)?)
}

fn flatten_semantics(semantics: &AffordanceSemantics) -> Vec<&StatementFrame> {
    [
        &semantics.outputs,
        &semantics.effects,
        &semantics.properties,
        &semantics.invariants,
        &semantics.operating_conditions,
        &semantics.limits,
    ]
    .into_iter()
    .flat_map(|frames| frames.statements.iter())
    .collect()
}

fn quantity(term: &TermRef) -> Option<&QuantityLiteral> {
    match term {
        TermRef::Literal(LiteralValue::Quantity(quantity)) => Some(quantity),
        _ => None,
    }
}

fn bool_evaluation(value: bool) -> ConstraintEvaluation {
    if value {
        ConstraintEvaluation::Satisfied
    } else {
        ConstraintEvaluation::Violated
    }
}

fn evaluation_quality(evaluation: ConstraintEvaluation) -> u8 {
    match evaluation {
        ConstraintEvaluation::Violated => 0,
        ConstraintEvaluation::Unknown => 1,
        ConstraintEvaluation::Satisfied => 2,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatcherError {
    Proposal(ProposalError),
    Mapping(MappingError),
    Semantic(ku_core::foundation::SemanticError),
    ProposalMustRemainPrivate,
    ArithmeticOverflow,
}

impl From<ProposalError> for MatcherError {
    fn from(error: ProposalError) -> Self {
        Self::Proposal(error)
    }
}

impl From<MappingError> for MatcherError {
    fn from(error: MappingError) -> Self {
        Self::Mapping(error)
    }
}

impl From<ku_core::foundation::SemanticError> for MatcherError {
    fn from(error: ku_core::foundation::SemanticError) -> Self {
        Self::Semantic(error)
    }
}

#[cfg(test)]
mod tests {
    use ku_core::foundation::{
        AcceptedInput, AffordanceOrigin, DimensionVector, ReceptorAcceptanceProfile,
        ReceptorCardinality, ReceptorOrigin, StatementId, StatementLocator, StatementQualifiers,
        UnitRef, UnknownConstraintPolicy,
    };

    use super::*;

    fn concept(byte: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([byte; 16])
    }

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn receptor() -> ReceptorDefinition {
        ReceptorDefinition {
            role: concept(1),
            expected_types: vec![concept(2)],
            hard_constraints: Vec::new(),
            cardinality: ReceptorCardinality::new(1, Some(1)).unwrap(),
            origin: ReceptorOrigin::Declared {
                source: StatementLocator {
                    object: reference(10),
                    statement_index: 0,
                },
            },
            acceptance: ReceptorAcceptanceProfile {
                policy: reference(11),
                required_evidence_kinds: Vec::new(),
                unknown_constraint_policy: UnknownConstraintPolicy::KeepUnresolved,
            },
        }
    }

    fn frame(arguments: Vec<TermRef>) -> StatementFrame {
        StatementFrame {
            statement_id: StatementId(1),
            operator_or_predicate: concept(3),
            arguments,
            constraints: Vec::new(),
            qualifiers: StatementQualifiers::default(),
        }
    }

    fn empty_frames() -> SemanticFrameSet {
        SemanticFrameSet {
            statements: Vec::new(),
        }
    }

    fn affordance(output: StatementFrame) -> KnowledgeAffordance {
        let empty = empty_frames();
        KnowledgeAffordance {
            sources: vec![reference(20)],
            offered_roles: vec![concept(1)],
            accepted_inputs: vec![AcceptedInput {
                receptor_definition: reference(21),
                role: concept(2),
                required: true,
            }],
            semantics: AffordanceSemantics {
                preconditions: empty.clone(),
                outputs: SemanticFrameSet {
                    statements: vec![output],
                },
                effects: empty.clone(),
                properties: empty.clone(),
                invariants: empty.clone(),
                operating_conditions: empty.clone(),
                limits: empty,
            },
            abstraction_patterns: Vec::new(),
            origin: AffordanceOrigin::Explicit {
                claims: vec![StatementLocator {
                    object: reference(20),
                    statement_index: 0,
                }],
            },
        }
    }

    fn request<'a>(
        receptor: &'a ReceptorDefinition,
        required: &'a SemanticFrameSet,
        context: &'a SemanticFrameSet,
        affordance: &'a KnowledgeAffordance,
    ) -> TypedMatchRequest<'a> {
        TypedMatchRequest {
            receptor_reference: reference(30),
            receptor,
            required_semantics: required,
            local_context: context,
            affordance_reference: reference(31),
            affordance,
            generator: reference(32),
            derivation_rule: Some(reference(33)),
            evidence: vec![reference(34)],
            index_commitment: Some(reference(35)),
            rule_commitment: Some(reference(36)),
            metrics: MatcherMetricConcepts {
                structural_fit: concept(40),
                constraint_fit: concept(41),
            },
            unmapped_reason: concept(42),
            source_frontier: EventCid::from_bytes([43; 32]),
            created_at_evaluation: 1,
            expires_after_evaluations: 10,
            privacy: DisclosureClass::LocalOnly,
        }
    }

    #[test]
    fn compatible_affine_units_create_valid_explainable_proposal() {
        let celsius = UnitRef {
            unit: concept(50),
            dimension: DimensionVector::TEMPERATURE,
            scale_to_base: ExactRatio::integer(1),
            offset_to_base: ExactRatio::new(27_315, 100).unwrap(),
        };
        let kelvin = UnitRef::coherent(concept(51), DimensionVector::TEMPERATURE);
        let required = SemanticFrameSet {
            statements: vec![frame(vec![TermRef::Literal(LiteralValue::Quantity(
                QuantityLiteral {
                    value: ExactRatio::integer(20),
                    source_unit: celsius,
                },
            ))])],
        };
        let candidate = affordance(frame(vec![TermRef::Literal(LiteralValue::Quantity(
            QuantityLiteral {
                value: ExactRatio::new(29_315, 100).unwrap(),
                source_unit: kelvin,
            },
        ))]));
        let receptor = receptor();
        let context = empty_frames();
        let outcome = ExactTypedMatcher::match_affordance(request(
            &receptor, &required, &context, &candidate,
        ))
        .unwrap();
        let proposal = outcome.proposal().unwrap();
        proposal.validate().unwrap();
        assert!(matches!(
            proposal.mapping_kernel.correspondences[0].transform,
            MappingTransform::AffineUnit { .. }
        ));
        assert_eq!(proposal.scores.len(), 2);
    }

    #[test]
    fn reversed_direction_and_negation_are_hard_mismatches() {
        let required = SemanticFrameSet {
            statements: vec![frame(vec![
                TermRef::Concept(concept(60)),
                TermRef::Concept(concept(61)),
            ])],
        };
        let mut reversed = frame(vec![
            TermRef::Concept(concept(61)),
            TermRef::Concept(concept(60)),
        ]);
        reversed.qualifiers.negated = true;
        let receptor = receptor();
        let candidate = affordance(reversed);
        let context = empty_frames();
        let outcome = ExactTypedMatcher::match_affordance(request(
            &receptor, &required, &context, &candidate,
        ))
        .unwrap();
        assert!(outcome.proposal().is_none());
        for kind in [MatchCheckKind::ArgumentDirection, MatchCheckKind::Negation] {
            assert!(outcome.checks().iter().any(|check| {
                check.kind == kind && check.evaluation == ConstraintEvaluation::Violated
            }));
        }
    }

    #[test]
    fn missing_time_modality_and_applicability_remain_unknown() {
        let mut required_frame = frame(vec![TermRef::Concept(concept(60))]);
        required_frame.qualifiers.time = Some(TermRef::Concept(concept(70)));
        required_frame.qualifiers.modality = Modality::Desired;
        let required = SemanticFrameSet {
            statements: vec![required_frame],
        };
        let mut output = frame(vec![TermRef::Concept(concept(60))]);
        output.qualifiers.modality = Modality::Possible;
        let mut candidate = affordance(output);
        candidate.semantics.preconditions = SemanticFrameSet {
            statements: vec![frame(vec![TermRef::Concept(concept(80))])],
        };
        let receptor = receptor();
        let context = empty_frames();
        let outcome = ExactTypedMatcher::match_affordance(request(
            &receptor, &required, &context, &candidate,
        ))
        .unwrap();
        assert!(outcome.proposal().is_some());
        for kind in [
            MatchCheckKind::Time,
            MatchCheckKind::Modality,
            MatchCheckKind::Applicability,
        ] {
            assert!(outcome.checks().iter().any(|check| {
                check.kind == kind && check.evaluation == ConstraintEvaluation::Unknown
            }));
        }
    }

    #[test]
    fn role_and_dimension_mismatch_emit_no_proposal() {
        let required = SemanticFrameSet {
            statements: vec![frame(vec![TermRef::Literal(LiteralValue::Quantity(
                QuantityLiteral {
                    value: ExactRatio::integer(1),
                    source_unit: UnitRef::coherent(concept(90), DimensionVector::LENGTH),
                },
            ))])],
        };
        let mut candidate = affordance(frame(vec![TermRef::Literal(LiteralValue::Quantity(
            QuantityLiteral {
                value: ExactRatio::integer(1),
                source_unit: UnitRef::coherent(concept(91), DimensionVector::TIME),
            },
        ))]));
        candidate.offered_roles = vec![concept(99)];
        let receptor = receptor();
        let context = empty_frames();
        let outcome = ExactTypedMatcher::match_affordance(request(
            &receptor, &required, &context, &candidate,
        ))
        .unwrap();
        assert!(outcome.proposal().is_none());
        assert!(outcome.checks().iter().any(|check| {
            check.kind == MatchCheckKind::UnitDimension
                && check.evaluation == ConstraintEvaluation::Violated
        }));
    }

    #[test]
    fn required_typed_constraint_violation_blocks_proposal() {
        let mut receptor = receptor();
        receptor.hard_constraints = vec![TypedConstraint {
            expression: ConstraintExpression::Compare {
                left: TermRef::Concept(concept(1)),
                operator: ComparisonOperator::Equal,
                right: TermRef::Concept(concept(2)),
            },
            required: true,
        }];
        let required = SemanticFrameSet {
            statements: vec![frame(vec![TermRef::Concept(concept(60))])],
        };
        let candidate = affordance(frame(vec![TermRef::Concept(concept(60))]));
        let context = empty_frames();
        let outcome = ExactTypedMatcher::match_affordance(request(
            &receptor, &required, &context, &candidate,
        ))
        .unwrap();
        assert!(outcome.proposal().is_none());
        assert!(outcome.checks().iter().any(|check| {
            check.kind == MatchCheckKind::TypedConstraint
                && check.evaluation == ConstraintEvaluation::Violated
        }));
    }
}
