//! Bounded SME-style typed relational alignment over semantic frame graphs.
//!
//! The aligner emits explainable partial/many-to-many candidate mappings. It
//! does not authorize materialization or replace exact typed validation.

use std::collections::{BTreeMap, BTreeSet};

use ku_core::foundation::{
    ConstraintEvaluation, CorrespondenceKind, ExactRatio, LiteralValue, MappingConstraintRegion,
    MappingError, MappingKernel, MappingSide, MappingTermLocator, MappingTransform,
    ObjectReference, SemanticError, SemanticFrameSet, StatementFrame, StatementId,
    StatementQualifiers, TermCorrespondence, TermRef, TypedConstraint, UnmappedRegion,
};

use crate::vnext_structural_signature::{StructuralSignature, StructuralSignatureKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelationalAlignmentPolicy {
    pub max_pair_evaluations: usize,
    pub max_statement_matches: usize,
    pub max_matches_per_source_statement: usize,
    pub max_matches_per_target_statement: usize,
}

impl RelationalAlignmentPolicy {
    pub const fn bounded_default() -> Self {
        Self {
            max_pair_evaluations: 4_096,
            max_statement_matches: 128,
            max_matches_per_source_statement: 2,
            max_matches_per_target_statement: 2,
        }
    }

    fn validate(self) -> Result<Self, RelationalAlignmentError> {
        if self.max_pair_evaluations == 0
            || self.max_statement_matches == 0
            || self.max_matches_per_source_statement == 0
            || self.max_matches_per_target_statement == 0
        {
            Err(RelationalAlignmentError::InvalidPolicy)
        } else {
            Ok(self)
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AlignmentCursor {
    pub next_source_statement: u32,
    pub next_target_statement: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignmentDirection {
    Direct,
    Reversed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum AlignmentViolation {
    ReversedDirection,
    NegationConflict,
    TermTypeConflict,
    DimensionConflict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum AlignmentAssumptionKind {
    PredicateAnalogy,
    ModalityUnresolved,
    PartialArity,
    OpenTerm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AlignmentAssumption {
    pub kind: AlignmentAssumptionKind,
    pub source_statement: u32,
    pub target_statement: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArgumentAlignment {
    pub source_argument: u32,
    pub target_argument: u32,
    pub evaluation: ConstraintEvaluation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatementAlignment {
    pub source_statement: u32,
    pub target_statement: u32,
    pub source_argument_count: u32,
    pub target_argument_count: u32,
    pub direction: AlignmentDirection,
    pub predicate_exact: bool,
    pub arguments: Vec<ArgumentAlignment>,
    pub systematic_connections: u32,
    pub violations: Vec<AlignmentViolation>,
    pub assumptions: Vec<AlignmentAssumptionKind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum AlignmentUnmappedReason {
    NoStructuralPartner,
    MatchBudget,
    PerStatementDiversityCap,
    PartialArity,
    HardConflict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AlignmentUnmappedRegion {
    pub side: MappingSide,
    pub statement: u32,
    pub argument: Option<u32>,
    pub reason: AlignmentUnmappedReason,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AlignmentEvidenceVector {
    pub matched_statement_pairs: u32,
    pub exact_predicate_pairs: u32,
    pub systematic_connections: u32,
    pub satisfied_argument_pairs: u32,
    pub unknown_argument_pairs: u32,
    pub hard_violation_count: u32,
    pub shared_structural_signature_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationalAlignment {
    pub statement_matches: Vec<StatementAlignment>,
    pub assumptions: Vec<AlignmentAssumption>,
    pub unmapped: Vec<AlignmentUnmappedRegion>,
    pub evidence: AlignmentEvidenceVector,
    pub continuation: Option<AlignmentCursor>,
    pub mapping_kernel: MappingKernel,
}

impl RelationalAlignment {
    pub fn is_actionable_candidate(&self) -> bool {
        !self.statement_matches.is_empty()
            && self
                .statement_matches
                .iter()
                .all(|statement| statement.violations.is_empty())
    }

    pub const fn is_materialization_authority(&self) -> bool {
        false
    }
}

pub struct RelationalAlignmentRequest<'a> {
    pub source_object: ObjectReference,
    pub source_semantics: &'a SemanticFrameSet,
    pub source_signatures: &'a [StructuralSignature],
    pub target_object: ObjectReference,
    pub target_semantics: &'a SemanticFrameSet,
    pub target_signatures: &'a [StructuralSignature],
    pub assumption_predicate: ku_core::foundation::ConceptCcid,
    pub unmapped_source_reason: ku_core::foundation::ConceptCcid,
    pub unmapped_target_reason: ku_core::foundation::ConceptCcid,
    pub policy: RelationalAlignmentPolicy,
    pub cursor: Option<AlignmentCursor>,
}

pub struct TypedRelationalAligner;

impl TypedRelationalAligner {
    pub fn align(
        request: RelationalAlignmentRequest<'_>,
    ) -> Result<RelationalAlignment, RelationalAlignmentError> {
        let policy = request.policy.validate()?;
        if request.source_object.cid == [0; 32] || request.target_object.cid == [0; 32] {
            return Err(RelationalAlignmentError::InvalidObjectReference);
        }
        let source = request.source_semantics.alpha_normalized()?;
        let target = request.target_semantics.alpha_normalized()?;
        let (start_source, start_target) = cursor_start(request.cursor, &source, &target)?;
        let mut candidates = Vec::new();
        let mut evaluated = 0usize;
        let mut continuation = None;
        'source: for source_index in start_source..source.statements.len() {
            let target_start = if source_index == start_source {
                start_target
            } else {
                0
            };
            for target_index in target_start..target.statements.len() {
                if evaluated == policy.max_pair_evaluations {
                    continuation = Some(AlignmentCursor {
                        next_source_statement: index_u32(source_index)?,
                        next_target_statement: index_u32(target_index)?,
                    });
                    break 'source;
                }
                evaluated += 1;
                candidates.push(evaluate_pair(
                    source_index,
                    &source.statements[source_index],
                    target_index,
                    &target.statements[target_index],
                )?);
            }
        }
        let compatible_pairs = candidates
            .iter()
            .map(|candidate| (candidate.source_statement, candidate.target_statement))
            .collect::<BTreeSet<_>>();
        for candidate in &mut candidates {
            candidate.systematic_connections = systematicity(
                candidate,
                &source.statements,
                &target.statements,
                &compatible_pairs,
            );
        }
        candidates.sort_by(|left, right| {
            candidate_priority(right)
                .cmp(&candidate_priority(left))
                .then_with(|| left.source_statement.cmp(&right.source_statement))
                .then_with(|| left.target_statement.cmp(&right.target_statement))
        });
        let mut per_source = BTreeMap::<u32, usize>::new();
        let mut per_target = BTreeMap::<u32, usize>::new();
        let mut matches = Vec::new();
        let mut capped_source = BTreeSet::new();
        let mut capped_target = BTreeSet::new();
        for candidate in candidates {
            if matches.len() == policy.max_statement_matches {
                break;
            }
            let source_count = per_source.entry(candidate.source_statement).or_default();
            let target_count = per_target.entry(candidate.target_statement).or_default();
            if *source_count == policy.max_matches_per_source_statement {
                capped_source.insert(candidate.source_statement);
                continue;
            }
            if *target_count == policy.max_matches_per_target_statement {
                capped_target.insert(candidate.target_statement);
                continue;
            }
            *source_count += 1;
            *target_count += 1;
            matches.push(candidate);
        }
        matches.sort_by_key(|candidate| (candidate.source_statement, candidate.target_statement));
        let assumptions = flatten_assumptions(&matches);
        let mut unmapped = unmapped_regions(
            &source,
            &target,
            &matches,
            continuation.is_some(),
            &capped_source,
            &capped_target,
        )?;
        add_partial_argument_regions(&matches, &mut unmapped);
        unmapped.sort_by_key(|region| {
            (
                region.side as u64,
                region.statement,
                region.argument,
                region.reason as u8,
            )
        });
        unmapped.dedup_by_key(|region| (region.side as u64, region.statement, region.argument));
        let evidence = evidence_vector(
            &matches,
            request.source_signatures,
            request.target_signatures,
        );
        let mapping_kernel = build_mapping_kernel(&request, &matches, &unmapped, &assumptions)?;
        Ok(RelationalAlignment {
            statement_matches: matches,
            assumptions,
            unmapped,
            evidence,
            continuation,
            mapping_kernel,
        })
    }
}

fn cursor_start(
    cursor: Option<AlignmentCursor>,
    source: &SemanticFrameSet,
    target: &SemanticFrameSet,
) -> Result<(usize, usize), RelationalAlignmentError> {
    let cursor = cursor.unwrap_or_default();
    let source_index = cursor.next_source_statement as usize;
    let target_index = cursor.next_target_statement as usize;
    if source_index > source.statements.len()
        || target_index > target.statements.len()
        || (source_index < source.statements.len()
            && !target.statements.is_empty()
            && target_index == target.statements.len())
    {
        return Err(RelationalAlignmentError::InvalidCursor);
    }
    Ok((source_index, target_index))
}

fn evaluate_pair(
    source_index: usize,
    source: &StatementFrame,
    target_index: usize,
    target: &StatementFrame,
) -> Result<StatementAlignment, RelationalAlignmentError> {
    let reversed = source.operator_or_predicate == target.operator_or_predicate
        && source.arguments.len() > 1
        && source.arguments.len() == target.arguments.len()
        && source.arguments != target.arguments
        && source.arguments.iter().eq(target.arguments.iter().rev());
    let direction = if reversed {
        AlignmentDirection::Reversed
    } else {
        AlignmentDirection::Direct
    };
    let mut violations = BTreeSet::new();
    let mut assumptions = BTreeSet::new();
    if reversed {
        violations.insert(AlignmentViolation::ReversedDirection);
    }
    if source.qualifiers.negated != target.qualifiers.negated {
        violations.insert(AlignmentViolation::NegationConflict);
    }
    if source.qualifiers.modality != target.qualifiers.modality {
        assumptions.insert(AlignmentAssumptionKind::ModalityUnresolved);
    }
    if source.operator_or_predicate != target.operator_or_predicate {
        assumptions.insert(AlignmentAssumptionKind::PredicateAnalogy);
    }
    if source.arguments.len() != target.arguments.len() {
        assumptions.insert(AlignmentAssumptionKind::PartialArity);
    }
    let pair_count = source.arguments.len().min(target.arguments.len());
    let mut arguments = Vec::new();
    for source_argument in 0..pair_count {
        let target_argument = match direction {
            AlignmentDirection::Direct => source_argument,
            AlignmentDirection::Reversed => target.arguments.len() - 1 - source_argument,
        };
        let evaluation = term_compatibility(
            &source.arguments[source_argument],
            &target.arguments[target_argument],
        );
        match evaluation {
            TermCompatibility::Satisfied => arguments.push(ArgumentAlignment {
                source_argument: index_u32(source_argument)?,
                target_argument: index_u32(target_argument)?,
                evaluation: ConstraintEvaluation::Satisfied,
            }),
            TermCompatibility::Unknown => {
                assumptions.insert(AlignmentAssumptionKind::OpenTerm);
                arguments.push(ArgumentAlignment {
                    source_argument: index_u32(source_argument)?,
                    target_argument: index_u32(target_argument)?,
                    evaluation: ConstraintEvaluation::Unknown,
                });
            }
            TermCompatibility::DimensionConflict => {
                violations.insert(AlignmentViolation::DimensionConflict);
                arguments.push(ArgumentAlignment {
                    source_argument: index_u32(source_argument)?,
                    target_argument: index_u32(target_argument)?,
                    evaluation: ConstraintEvaluation::Violated,
                });
            }
            TermCompatibility::TypeConflict => {
                violations.insert(AlignmentViolation::TermTypeConflict);
                arguments.push(ArgumentAlignment {
                    source_argument: index_u32(source_argument)?,
                    target_argument: index_u32(target_argument)?,
                    evaluation: ConstraintEvaluation::Violated,
                });
            }
        }
    }
    Ok(StatementAlignment {
        source_statement: index_u32(source_index)?,
        target_statement: index_u32(target_index)?,
        source_argument_count: index_u32(source.arguments.len())?,
        target_argument_count: index_u32(target.arguments.len())?,
        direction,
        predicate_exact: source.operator_or_predicate == target.operator_or_predicate,
        arguments,
        systematic_connections: 0,
        violations: violations.into_iter().collect(),
        assumptions: assumptions.into_iter().collect(),
    })
}

#[derive(Clone, Copy)]
enum TermCompatibility {
    Satisfied,
    Unknown,
    DimensionConflict,
    TypeConflict,
}

fn term_compatibility(source: &TermRef, target: &TermRef) -> TermCompatibility {
    match (source, target) {
        (
            TermRef::Literal(LiteralValue::Quantity(source)),
            TermRef::Literal(LiteralValue::Quantity(target)),
        ) => {
            if source.source_unit.dimension == target.source_unit.dimension {
                TermCompatibility::Satisfied
            } else {
                TermCompatibility::DimensionConflict
            }
        }
        (TermRef::Concept(_), TermRef::Concept(_))
        | (TermRef::Statement(_), TermRef::Statement(_))
        | (TermRef::KnowledgeObject(_), TermRef::KnowledgeObject(_)) => {
            TermCompatibility::Satisfied
        }
        (TermRef::Variable { .. }, _)
        | (_, TermRef::Variable { .. })
        | (TermRef::Receptor { .. }, _)
        | (_, TermRef::Receptor { .. }) => TermCompatibility::Unknown,
        (TermRef::Literal(left), TermRef::Literal(right)) => {
            if std::mem::discriminant(left) == std::mem::discriminant(right) {
                TermCompatibility::Satisfied
            } else {
                TermCompatibility::TypeConflict
            }
        }
        _ => TermCompatibility::TypeConflict,
    }
}

fn systematicity(
    candidate: &StatementAlignment,
    source: &[StatementFrame],
    target: &[StatementFrame],
    compatible_pairs: &BTreeSet<(u32, u32)>,
) -> u32 {
    let source_statement = &source[candidate.source_statement as usize];
    let target_statement = &target[candidate.target_statement as usize];
    candidate
        .arguments
        .iter()
        .filter_map(|argument| {
            let source_ref = match &source_statement.arguments[argument.source_argument as usize] {
                TermRef::Statement(id) => Some(*id),
                _ => None,
            }?;
            let target_ref = match &target_statement.arguments[argument.target_argument as usize] {
                TermRef::Statement(id) => Some(*id),
                _ => None,
            }?;
            let source_index = statement_index(source, source_ref)?;
            let target_index = statement_index(target, target_ref)?;
            compatible_pairs
                .contains(&(source_index, target_index))
                .then_some(())
        })
        .count() as u32
}

fn statement_index(statements: &[StatementFrame], id: StatementId) -> Option<u32> {
    statements
        .iter()
        .position(|statement| statement.statement_id == id)
        .and_then(|index| u32::try_from(index).ok())
}

fn candidate_priority(
    candidate: &StatementAlignment,
) -> (u32, u8, u32, u32, std::cmp::Reverse<u32>) {
    let satisfied = candidate
        .arguments
        .iter()
        .filter(|argument| argument.evaluation == ConstraintEvaluation::Satisfied)
        .count() as u32;
    (
        candidate.systematic_connections,
        u8::from(candidate.predicate_exact),
        satisfied,
        candidate.arguments.len() as u32,
        std::cmp::Reverse(candidate.violations.len() as u32),
    )
}

fn flatten_assumptions(matches: &[StatementAlignment]) -> Vec<AlignmentAssumption> {
    matches
        .iter()
        .flat_map(|statement| {
            statement
                .assumptions
                .iter()
                .copied()
                .map(|kind| AlignmentAssumption {
                    kind,
                    source_statement: statement.source_statement,
                    target_statement: statement.target_statement,
                })
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn unmapped_regions(
    source: &SemanticFrameSet,
    target: &SemanticFrameSet,
    matches: &[StatementAlignment],
    budget_reached: bool,
    capped_source: &BTreeSet<u32>,
    capped_target: &BTreeSet<u32>,
) -> Result<Vec<AlignmentUnmappedRegion>, RelationalAlignmentError> {
    let mapped_source = matches
        .iter()
        .map(|statement| statement.source_statement)
        .collect::<BTreeSet<_>>();
    let mapped_target = matches
        .iter()
        .map(|statement| statement.target_statement)
        .collect::<BTreeSet<_>>();
    let mut output = Vec::new();
    for index in 0..source.statements.len() {
        let index = index_u32(index)?;
        if !mapped_source.contains(&index) {
            output.push(AlignmentUnmappedRegion {
                side: MappingSide::Source,
                statement: index,
                argument: None,
                reason: if capped_source.contains(&index) {
                    AlignmentUnmappedReason::PerStatementDiversityCap
                } else if budget_reached {
                    AlignmentUnmappedReason::MatchBudget
                } else {
                    AlignmentUnmappedReason::NoStructuralPartner
                },
            });
        }
    }
    for index in 0..target.statements.len() {
        let index = index_u32(index)?;
        if !mapped_target.contains(&index) {
            output.push(AlignmentUnmappedRegion {
                side: MappingSide::Target,
                statement: index,
                argument: None,
                reason: if capped_target.contains(&index) {
                    AlignmentUnmappedReason::PerStatementDiversityCap
                } else if budget_reached {
                    AlignmentUnmappedReason::MatchBudget
                } else {
                    AlignmentUnmappedReason::NoStructuralPartner
                },
            });
        }
    }
    Ok(output)
}

fn add_partial_argument_regions(
    matches: &[StatementAlignment],
    unmapped: &mut Vec<AlignmentUnmappedRegion>,
) {
    for statement in matches {
        for argument in &statement.arguments {
            if argument.evaluation == ConstraintEvaluation::Violated {
                unmapped.push(AlignmentUnmappedRegion {
                    side: MappingSide::Source,
                    statement: statement.source_statement,
                    argument: Some(argument.source_argument),
                    reason: AlignmentUnmappedReason::HardConflict,
                });
                unmapped.push(AlignmentUnmappedRegion {
                    side: MappingSide::Target,
                    statement: statement.target_statement,
                    argument: Some(argument.target_argument),
                    reason: AlignmentUnmappedReason::HardConflict,
                });
            }
        }
        let aligned_count = statement.arguments.len() as u32;
        for argument in aligned_count..statement.source_argument_count {
            unmapped.push(AlignmentUnmappedRegion {
                side: MappingSide::Source,
                statement: statement.source_statement,
                argument: Some(argument),
                reason: AlignmentUnmappedReason::PartialArity,
            });
        }
        for argument in aligned_count..statement.target_argument_count {
            unmapped.push(AlignmentUnmappedRegion {
                side: MappingSide::Target,
                statement: statement.target_statement,
                argument: Some(argument),
                reason: AlignmentUnmappedReason::PartialArity,
            });
        }
    }
}

fn evidence_vector(
    matches: &[StatementAlignment],
    source_signatures: &[StructuralSignature],
    target_signatures: &[StructuralSignature],
) -> AlignmentEvidenceVector {
    let source = source_signatures
        .iter()
        .filter(|signature| signature.kind != StructuralSignatureKind::CcidRole)
        .map(|signature| (signature.kind, signature.digest))
        .collect::<BTreeSet<_>>();
    let target = target_signatures
        .iter()
        .filter(|signature| signature.kind != StructuralSignatureKind::CcidRole)
        .map(|signature| (signature.kind, signature.digest))
        .collect::<BTreeSet<_>>();
    AlignmentEvidenceVector {
        matched_statement_pairs: matches.len() as u32,
        exact_predicate_pairs: matches
            .iter()
            .filter(|statement| statement.predicate_exact)
            .count() as u32,
        systematic_connections: matches
            .iter()
            .map(|statement| statement.systematic_connections)
            .sum(),
        satisfied_argument_pairs: matches
            .iter()
            .flat_map(|statement| &statement.arguments)
            .filter(|argument| argument.evaluation == ConstraintEvaluation::Satisfied)
            .count() as u32,
        unknown_argument_pairs: matches
            .iter()
            .flat_map(|statement| &statement.arguments)
            .filter(|argument| argument.evaluation == ConstraintEvaluation::Unknown)
            .count() as u32,
        hard_violation_count: matches
            .iter()
            .map(|statement| statement.violations.len() as u32)
            .sum(),
        shared_structural_signature_count: source.intersection(&target).count() as u32,
    }
}

fn build_mapping_kernel(
    request: &RelationalAlignmentRequest<'_>,
    matches: &[StatementAlignment],
    unmapped: &[AlignmentUnmappedRegion],
    assumptions: &[AlignmentAssumption],
) -> Result<MappingKernel, RelationalAlignmentError> {
    let mut correspondences = Vec::new();
    let mut constraint_regions = Vec::new();
    let mut constrained_source_statements = BTreeSet::new();
    let mut constrained_target_statements = BTreeSet::new();
    for statement in matches {
        correspondences.push(TermCorrespondence {
            source: locator(&request.source_object, statement.source_statement, None),
            target: locator(&request.target_object, statement.target_statement, None),
            kind: if statement.predicate_exact {
                CorrespondenceKind::Equivalent
            } else {
                CorrespondenceKind::Analogous
            },
            transform: MappingTransform::Identity,
        });
        let source_statement =
            &request.source_semantics.statements[statement.source_statement as usize];
        let target_statement =
            &request.target_semantics.statements[statement.target_statement as usize];
        constrained_source_statements.insert(statement.source_statement);
        constrained_target_statements.insert(statement.target_statement);
        for argument in &statement.arguments {
            if argument.evaluation == ConstraintEvaluation::Violated {
                continue;
            }
            correspondences.push(TermCorrespondence {
                source: locator(
                    &request.source_object,
                    statement.source_statement,
                    Some(argument.source_argument),
                ),
                target: locator(
                    &request.target_object,
                    statement.target_statement,
                    Some(argument.target_argument),
                ),
                kind: CorrespondenceKind::StructuralRole,
                transform: term_transform(
                    &source_statement.arguments[argument.source_argument as usize],
                    &target_statement.arguments[argument.target_argument as usize],
                )?,
            });
        }
    }
    for statement in constrained_source_statements {
        constraint_regions.extend(
            request.source_semantics.statements[statement as usize]
                .constraints
                .iter()
                .cloned()
                .map(|constraint| MappingConstraintRegion {
                    constraint,
                    evaluation: ConstraintEvaluation::Unknown,
                }),
        );
    }
    for statement in constrained_target_statements {
        constraint_regions.extend(
            request.target_semantics.statements[statement as usize]
                .constraints
                .iter()
                .cloned()
                .map(|constraint| MappingConstraintRegion {
                    constraint,
                    evaluation: ConstraintEvaluation::Unknown,
                }),
        );
    }
    let mut unique_constraints = Vec::new();
    for region in constraint_regions {
        if !unique_constraints
            .iter()
            .any(|existing: &MappingConstraintRegion| existing == &region)
        {
            unique_constraints.push(region);
        }
    }
    let constraint_regions = unique_constraints;
    let assumptions = assumption_frames(request, assumptions)?;
    let unmapped_regions = unmapped
        .iter()
        .map(|region| UnmappedRegion {
            side: region.side,
            locator: locator(
                match region.side {
                    MappingSide::Source => &request.source_object,
                    MappingSide::Target => &request.target_object,
                },
                region.statement,
                region.argument,
            ),
            reason: match region.side {
                MappingSide::Source => request.unmapped_source_reason,
                MappingSide::Target => request.unmapped_target_reason,
            },
        })
        .collect();
    let kernel = MappingKernel {
        source_objects: vec![request.source_object.clone()],
        target_objects: vec![request.target_object.clone()],
        correspondences,
        assumptions,
        constraint_regions,
        unmapped_regions,
    };
    kernel.canonical_value()?;
    Ok(kernel)
}

fn locator(
    object: &ObjectReference,
    statement_index: u32,
    argument_index: Option<u32>,
) -> MappingTermLocator {
    MappingTermLocator {
        object: object.clone(),
        statement_index,
        argument_index,
    }
}

fn term_transform(
    source: &TermRef,
    target: &TermRef,
) -> Result<MappingTransform, RelationalAlignmentError> {
    match (source, target) {
        (
            TermRef::Literal(LiteralValue::Quantity(source)),
            TermRef::Literal(LiteralValue::Quantity(target)),
        ) if source.source_unit.dimension == target.source_unit.dimension => {
            let scale = source
                .source_unit
                .scale_to_base
                .checked_div(target.source_unit.scale_to_base)?;
            let offset = source
                .source_unit
                .offset_to_base
                .checked_add(ExactRatio::new(
                    target
                        .source_unit
                        .offset_to_base
                        .numerator()
                        .checked_neg()
                        .ok_or(RelationalAlignmentError::ArithmeticOverflow)?,
                    target.source_unit.offset_to_base.denominator(),
                )?)?
                .checked_div(target.source_unit.scale_to_base)?;
            Ok(MappingTransform::AffineUnit {
                source_dimension: source.source_unit.dimension,
                target_dimension: target.source_unit.dimension,
                scale,
                offset,
            })
        }
        _ => Ok(MappingTransform::Identity),
    }
}

fn assumption_frames(
    request: &RelationalAlignmentRequest<'_>,
    assumptions: &[AlignmentAssumption],
) -> Result<SemanticFrameSet, RelationalAlignmentError> {
    let statements = assumptions
        .iter()
        .enumerate()
        .map(|(index, assumption)| {
            Ok(StatementFrame {
                statement_id: StatementId(index_u32(index)?),
                operator_or_predicate: request.assumption_predicate,
                arguments: vec![
                    TermRef::KnowledgeObject(request.source_object.clone()),
                    TermRef::Literal(LiteralValue::Bytes(
                        assumption.source_statement.to_be_bytes().to_vec(),
                    )),
                    TermRef::KnowledgeObject(request.target_object.clone()),
                    TermRef::Literal(LiteralValue::Bytes(
                        assumption.target_statement.to_be_bytes().to_vec(),
                    )),
                    TermRef::Literal(LiteralValue::Bytes(vec![assumption.kind as u8])),
                ],
                constraints: Vec::<TypedConstraint>::new(),
                qualifiers: StatementQualifiers::default(),
            })
        })
        .collect::<Result<Vec<_>, RelationalAlignmentError>>()?;
    let frames = SemanticFrameSet { statements };
    frames.canonical_value()?;
    Ok(frames)
}

fn index_u32(index: usize) -> Result<u32, RelationalAlignmentError> {
    u32::try_from(index).map_err(|_| RelationalAlignmentError::Limit)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelationalAlignmentError {
    InvalidPolicy,
    InvalidObjectReference,
    InvalidCursor,
    Limit,
    ArithmeticOverflow,
    Semantic(SemanticError),
    Mapping(MappingError),
}

impl From<SemanticError> for RelationalAlignmentError {
    fn from(error: SemanticError) -> Self {
        Self::Semantic(error)
    }
}

impl From<MappingError> for RelationalAlignmentError {
    fn from(error: MappingError) -> Self {
        Self::Mapping(error)
    }
}

#[cfg(test)]
mod tests {
    use ku_core::foundation::{
        AcceptedInput, AffordanceOrigin, AffordanceSemantics, ConceptCcid, DimensionVector,
        KnowledgeAffordance, ObjectCid, QuantityLiteral, StatementLocator, UnitRef,
    };

    use crate::vnext_structural_signature::{StructuralSignatureIndex, StructuralSignatureSource};

    use super::*;

    fn concept(byte: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([byte; 16])
    }

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn frame(predicate: u8, left: TermRef, right: TermRef) -> StatementFrame {
        StatementFrame {
            statement_id: StatementId(0),
            operator_or_predicate: concept(predicate),
            arguments: vec![left, right],
            constraints: Vec::new(),
            qualifiers: StatementQualifiers::default(),
        }
    }

    fn frames(statements: Vec<StatementFrame>) -> SemanticFrameSet {
        SemanticFrameSet { statements }
    }

    fn affordance(semantics: SemanticFrameSet, vocabulary: u8) -> KnowledgeAffordance {
        let empty = frames(Vec::new());
        KnowledgeAffordance {
            sources: vec![reference(vocabulary)],
            offered_roles: vec![concept(vocabulary)],
            accepted_inputs: vec![AcceptedInput {
                receptor_definition: reference(vocabulary.wrapping_add(1)),
                role: concept(vocabulary.wrapping_add(1)),
                required: true,
            }],
            semantics: AffordanceSemantics {
                preconditions: empty.clone(),
                outputs: semantics,
                effects: empty.clone(),
                properties: empty.clone(),
                invariants: empty.clone(),
                operating_conditions: empty.clone(),
                limits: empty,
            },
            abstraction_patterns: Vec::new(),
            origin: AffordanceOrigin::Explicit {
                claims: vec![StatementLocator {
                    object: reference(vocabulary),
                    statement_index: 0,
                }],
            },
        }
    }

    fn signatures(affordance: &KnowledgeAffordance, cid: u8) -> Vec<StructuralSignature> {
        let object = ObjectCid::from_bytes([cid; 32]);
        StructuralSignatureIndex::rebuild(&[StructuralSignatureSource::Affordance {
            cid: object,
            affordance,
        }])
        .unwrap()
        .signatures_for(object)
    }

    fn request<'a>(
        source: &'a SemanticFrameSet,
        source_signatures: &'a [StructuralSignature],
        target: &'a SemanticFrameSet,
        target_signatures: &'a [StructuralSignature],
    ) -> RelationalAlignmentRequest<'a> {
        RelationalAlignmentRequest {
            source_object: reference(200),
            source_semantics: source,
            source_signatures,
            target_object: reference(201),
            target_semantics: target,
            target_signatures,
            assumption_predicate: concept(202),
            unmapped_source_reason: concept(203),
            unmapped_target_reason: concept(204),
            policy: RelationalAlignmentPolicy::bounded_default(),
            cursor: None,
        }
    }

    #[test]
    fn ag_struct_002_beats_ag_distractor_001_without_embedding_or_keywords() {
        let corpus = include_str!("../../../docs/specs/vnext/corpus/anti_gravity_v1.yaml");
        assert!(corpus.contains("id: AG-STRUCT-002"));
        assert!(corpus.contains("id: AG-DISTRACTOR-001"));
        let source_frames = frames(vec![frame(
            1,
            TermRef::Concept(concept(2)),
            TermRef::Literal(LiteralValue::Quantity(QuantityLiteral {
                value: ExactRatio::integer(1),
                source_unit: UnitRef::coherent(concept(3), DimensionVector::LENGTH),
            })),
        )]);
        let target_frames = frames(vec![frame(
            91,
            TermRef::Concept(concept(92)),
            TermRef::Literal(LiteralValue::Quantity(QuantityLiteral {
                value: ExactRatio::integer(2),
                source_unit: UnitRef::coherent(concept(93), DimensionVector::LENGTH),
            })),
        )]);
        let source_affordance = affordance(source_frames.clone(), 10);
        let target_affordance = affordance(target_frames.clone(), 110);
        let source_signatures = signatures(&source_affordance, 1);
        let target_signatures = signatures(&target_affordance, 2);
        let aligned = TypedRelationalAligner::align(request(
            &source_frames,
            &source_signatures,
            &target_frames,
            &target_signatures,
        ))
        .unwrap();
        assert_eq!(aligned.statement_matches.len(), 1);
        assert!(aligned.evidence.shared_structural_signature_count > 0);
        assert!(aligned
            .assumptions
            .iter()
            .any(|assumption| assumption.kind == AlignmentAssumptionKind::PredicateAnalogy));
        let distractor = frames(Vec::new());
        let rejected = TypedRelationalAligner::align(request(
            &source_frames,
            &source_signatures,
            &distractor,
            &[],
        ))
        .unwrap();
        assert!(rejected.statement_matches.is_empty());
        assert!(!rejected.is_actionable_candidate());
    }

    #[test]
    fn direction_and_dimension_conflicts_remain_explainable_not_truth_verdicts() {
        let a = TermRef::Concept(concept(20));
        let b = TermRef::Concept(concept(21));
        let source = frames(vec![frame(22, a.clone(), b.clone())]);
        let reversed = frames(vec![frame(22, b, a)]);
        let result = TypedRelationalAligner::align(request(&source, &[], &reversed, &[])).unwrap();
        assert_eq!(
            result.statement_matches[0].direction,
            AlignmentDirection::Reversed
        );
        assert!(result.statement_matches[0]
            .violations
            .contains(&AlignmentViolation::ReversedDirection));
        assert!(!result.is_actionable_candidate());
        assert!(!result.is_materialization_authority());
    }

    #[test]
    fn one_relation_can_align_many_targets_under_explicit_caps() {
        let source = frames(vec![frame(
            30,
            TermRef::Concept(concept(31)),
            TermRef::Concept(concept(32)),
        )]);
        let mut first = frame(
            40,
            TermRef::Concept(concept(41)),
            TermRef::Concept(concept(42)),
        );
        let mut second = frame(
            50,
            TermRef::Concept(concept(51)),
            TermRef::Concept(concept(52)),
        );
        first.statement_id = StatementId(0);
        second.statement_id = StatementId(1);
        let target = frames(vec![first, second]);
        let result = TypedRelationalAligner::align(request(&source, &[], &target, &[])).unwrap();
        assert_eq!(result.statement_matches.len(), 2);
        assert_eq!(
            result
                .statement_matches
                .iter()
                .map(|statement| statement.source_statement)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([0])
        );
        assert!(result.mapping_kernel.correspondences.len() >= 6);
    }

    #[test]
    fn partial_graph_and_budget_return_unmapped_regions_and_real_continuation() {
        let mut source_statements = Vec::new();
        let mut target_statements = Vec::new();
        for index in 0..3u32 {
            let mut source = frame(
                60 + index as u8,
                TermRef::Concept(concept(70)),
                TermRef::Concept(concept(71)),
            );
            source.statement_id = StatementId(index);
            source_statements.push(source);
            let mut target = frame(
                80 + index as u8,
                TermRef::Concept(concept(90)),
                TermRef::Concept(concept(91)),
            );
            target.statement_id = StatementId(index);
            target_statements.push(target);
        }
        let source = frames(source_statements);
        let target = frames(target_statements);
        let mut req = request(&source, &[], &target, &[]);
        req.policy.max_pair_evaluations = 2;
        req.policy.max_statement_matches = 1;
        let result = TypedRelationalAligner::align(req).unwrap();
        assert_eq!(result.statement_matches.len(), 1);
        assert!(result.continuation.is_some());
        assert!(result
            .unmapped
            .iter()
            .any(|region| region.reason == AlignmentUnmappedReason::MatchBudget));
        result.mapping_kernel.canonical_value().unwrap();
    }

    #[test]
    fn exact_affine_unit_transform_is_reified_in_mapping_kernel() {
        let celsius = UnitRef {
            unit: concept(100),
            dimension: DimensionVector::TEMPERATURE,
            scale_to_base: ExactRatio::integer(1),
            offset_to_base: ExactRatio::new(27_315, 100).unwrap(),
        };
        let kelvin = UnitRef::coherent(concept(101), DimensionVector::TEMPERATURE);
        let source = frames(vec![frame(
            102,
            TermRef::Concept(concept(103)),
            TermRef::Literal(LiteralValue::Quantity(QuantityLiteral {
                value: ExactRatio::integer(20),
                source_unit: celsius,
            })),
        )]);
        let target = frames(vec![frame(
            102,
            TermRef::Concept(concept(103)),
            TermRef::Literal(LiteralValue::Quantity(QuantityLiteral {
                value: ExactRatio::new(29_315, 100).unwrap(),
                source_unit: kelvin,
            })),
        )]);
        let result = TypedRelationalAligner::align(request(&source, &[], &target, &[])).unwrap();
        assert!(result
            .mapping_kernel
            .correspondences
            .iter()
            .any(|correspondence| {
                matches!(
                    correspondence.transform,
                    MappingTransform::AffineUnit { .. }
                )
            }));
    }
}
