//! Exact typed Receptor-to-Affordance matcher with three-state validation.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use ku_core::concept_lifecycle::{AuthoringConceptResolution, ConceptLifecycleTable};
use ku_core::foundation::{
    project_virtual_sfs_affordance, AffordanceError, AffordanceSemantics, ComparisonOperator,
    ConceptCcid, ConstraintEvaluation, ConstraintExpression, CorrespondenceKind, CoverageBasis,
    CoverageStatement, CoverageStatus, DisclosureClass, EventCid, ExactRatio, InventoryError,
    KnowledgeAffordance, LiteralValue, MappingConstraintRegion, MappingEnvelope, MappingError,
    MappingKernel, MappingSide, MappingTermLocator, MappingTransform, Modality, ObjectCid,
    ObjectReference, QuantityLiteral, ReceptorDefinition, ReceptorSlotId, SelectorCid,
    SemanticFrameSet, StatementFrame, StatementId, TermCorrespondence, TermRef, TypedConstraint,
    UnknownConstraintPolicy, UnmappedRegion, VariableId, VIRTUAL_SFS_AFFORDANCE_PROFILE,
};

use crate::vnext_proposal::{
    BindingProposal, ConstraintObservation, ProposalError, ProposalExpiry, ScoreComponent,
    ScoreDirection,
};
use crate::vnext_query::KnowledgeNeedIr;
use crate::vnext_semantic_index::RebuildableSemanticIndex;

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
    Location,
    Perspective,
    Condition,
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

    /// Direct committed `SemanticFrameSet` <-> `KnowledgeNeedIr` virtual affordance matching
    /// (`P3` §3.1–3.2).
    ///
    /// Projects `semantic` via `ku_core::foundation::project_virtual_sfs_affordance`
    /// (`virtual-sfs-affordance/1`), performs rational affine SI unit conversion
    /// (`km -> m`, `degC -> K`), evaluates 3-state qualifiers (`Satisfied | Violated | Unknown`),
    /// and emits a non-executable [`BindingProposal`] paired with an explicit [`CoverageStatement`].
    pub fn match_semantic_frame_set(
        need: &KnowledgeNeedIr,
        receptor: &ReceptorDefinition,
        ku_ref: ObjectReference,
        semantic: &SemanticFrameSet,
        ctx: &SfsMatchContext<'_>,
    ) -> Result<SfsMatchResult, MatcherError> {
        if !matches!(
            need.privacy,
            DisclosureClass::LocalOnly | DisclosureClass::NegotiatedEncrypted
        ) {
            return Err(MatcherError::ProposalMustRemainPrivate);
        }

        let receptor_reference = need
            .receptor_definitions
            .first()
            .cloned()
            .ok_or(MatcherError::MissingReceptorReference)?;

        let virtual_affordance = project_virtual_sfs_affordance(ku_ref.clone(), semantic)?;
        let canonicalize_ccid = |c: ConceptCcid| -> ConceptCcid {
            match ctx.lifecycle {
                Some(table) => match table.resolve_for_authoring(c) {
                    AuthoringConceptResolution::Redirected { to, .. } => to,
                    _ => c,
                },
                None => c,
            }
        };

        let receptor_role = canonicalize_ccid(receptor.role);
        let role_supported = virtual_affordance
            .offered_roles
            .iter()
            .any(|offered| canonicalize_ccid(*offered) == receptor_role)
            || need.desired_roles.iter().any(|desired| {
                let d = canonicalize_ccid(*desired);
                virtual_affordance
                    .offered_roles
                    .iter()
                    .any(|offered| canonicalize_ccid(*offered) == d)
            });

        let mut checks = vec![MatchCheck {
            kind: MatchCheckKind::OfferedRole,
            evaluation: bool_evaluation(role_supported),
            required: true,
            source_statement: None,
            target_statement: None,
        }];

        let targets = &virtual_affordance.semantics.outputs.statements;
        let mut correspondences = Vec::new();
        let mut unmapped_regions = Vec::new();
        let mut var_bindings: BTreeMap<VariableId, TermRef> = BTreeMap::new();
        let mut slot_bindings: BTreeMap<ReceptorSlotId, TermRef> = BTreeMap::new();

        for (source_index, required) in need.goal.statements.iter().enumerate() {
            let req_pred = canonicalize_ccid(required.operator_or_predicate);
            let best = targets
                .iter()
                .enumerate()
                .filter(|(_, target)| canonicalize_ccid(target.operator_or_predicate) == req_pred)
                .map(|(target_index, target)| {
                    assess_sfs_statement(
                        required,
                        target,
                        &virtual_affordance.semantics.outputs,
                        &need.local_context,
                        source_index as u32,
                        target_index as u32,
                        ctx.lifecycle,
                    )
                })
                .max_by_key(StatementAssessment::quality);

            if let Some(best) = best {
                checks.extend(best.checks);
                for (arg_idx, (left, right)) in required
                    .arguments
                    .iter()
                    .zip(&best.target.arguments)
                    .enumerate()
                {
                    match left {
                        TermRef::Variable {
                            id,
                            type_constraint,
                        } => {
                            if let (Some(expected_type), TermRef::Concept(actual)) =
                                (type_constraint, right)
                            {
                                checks.push(MatchCheck {
                                    kind: MatchCheckKind::ArgumentType,
                                    evaluation: bool_evaluation(
                                        canonicalize_ccid(*expected_type)
                                            == canonicalize_ccid(*actual),
                                    ),
                                    required: true,
                                    source_statement: Some(source_index as u32),
                                    target_statement: Some(best.target_index),
                                });
                            }
                            if let Some(prev) = var_bindings.get(id) {
                                let compat =
                                    term_compatibility_with_lifecycle(prev, right, ctx.lifecycle);
                                checks.push(MatchCheck {
                                    kind: if quantity(prev).is_some() || quantity(right).is_some() {
                                        MatchCheckKind::UnitDimension
                                    } else {
                                        MatchCheckKind::ArgumentType
                                    },
                                    evaluation: compat,
                                    required: true,
                                    source_statement: Some(source_index as u32),
                                    target_statement: Some(best.target_index),
                                });
                            } else {
                                var_bindings.insert(*id, right.clone());
                            }
                        }
                        TermRef::Receptor {
                            slot,
                            expected_type,
                        } => {
                            if let (Some(expected_type), TermRef::Concept(actual)) =
                                (expected_type, right)
                            {
                                checks.push(MatchCheck {
                                    kind: MatchCheckKind::ArgumentType,
                                    evaluation: bool_evaluation(
                                        canonicalize_ccid(*expected_type)
                                            == canonicalize_ccid(*actual),
                                    ),
                                    required: true,
                                    source_statement: Some(source_index as u32),
                                    target_statement: Some(best.target_index),
                                });
                            }
                            slot_bindings.insert(*slot, right.clone());
                        }
                        _ => {}
                    }
                    let eval = term_compatibility_with_lifecycle(left, right, ctx.lifecycle);
                    if eval != ConstraintEvaluation::Violated {
                        let resolved_left = substitute_term(left, &var_bindings, &slot_bindings);
                        correspondences.push(TermCorrespondence {
                            source: MappingTermLocator {
                                object: receptor_reference.clone(),
                                statement_index: source_index as u32,
                                argument_index: Some(arg_idx as u32),
                            },
                            target: MappingTermLocator {
                                object: ku_ref.clone(),
                                statement_index: best.target_index,
                                argument_index: Some(arg_idx as u32),
                            },
                            kind: CorrespondenceKind::Equivalent,
                            transform: sfs_affine_transform(
                                &resolved_left,
                                left,
                                right,
                                &need.goal,
                                receptor,
                            )?,
                        });
                    }
                }
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
                        object: receptor_reference.clone(),
                        statement_index: source_index as u32,
                        argument_index: None,
                    },
                    reason: ctx.unmapped_reason,
                });
            }
        }

        let mut all_constraints = receptor.hard_constraints.clone();
        for stmt in &need.goal.statements {
            for c in &stmt.constraints {
                if !all_constraints.contains(c) {
                    all_constraints.push(c.clone());
                }
            }
        }

        let mut constraint_regions = Vec::new();
        let mut observations = Vec::new();
        for (index, constraint) in all_constraints.into_iter().enumerate() {
            let substituted = substitute_constraint(&constraint, &var_bindings, &slot_bindings);
            let evaluation = evaluate_constraint_with_lifecycle(&substituted, ctx.lifecycle);
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
            &virtual_affordance.semantics.preconditions,
            &need.local_context,
        ));

        let reject_unknown = matches!(
            receptor.acceptance.unknown_constraint_policy,
            UnknownConstraintPolicy::RejectBinding
        );
        let hard_blocked = checks.iter().any(|check| {
            check.required
                && (check.evaluation == ConstraintEvaluation::Violated
                    || (reject_unknown && check.evaluation == ConstraintEvaluation::Unknown))
        });

        let returned_records = u64::from(!hard_blocked);
        let coverage = CoverageStatement {
            selector: ctx.selector,
            assessed_frontier: vec![ctx.source_frontier],
            basis: CoverageBasis::ExactInventory,
            status: CoverageStatus::CompleteWithinSelector,
            returned_records,
            returned_bytes: 0,
            continuation: None,
            limitations: Vec::new(),
        };
        coverage.validate()?;

        if hard_blocked {
            return Ok(SfsMatchResult {
                outcome: MatcherOutcome::HardMismatch { checks },
                virtual_affordance,
                coverage,
                coverage_scope: ctx.coverage_scope,
            });
        }

        let rule_commitment = virtual_sfs_rule_commitment();
        let kernel = MappingKernel {
            source_objects: vec![receptor_reference],
            target_objects: vec![ku_ref.clone()],
            correspondences,
            assumptions: need.local_context.clone(),
            constraint_regions,
            unmapped_regions,
        };
        let kernel_id = kernel.cid()?;
        let proposal = BindingProposal {
            mapping_kernel: kernel,
            proposed_envelope: MappingEnvelope {
                kernel: kernel_id,
                generator: ctx.generator.clone(),
                derivation_rule: Some(rule_commitment.clone()),
                evidence: ctx.evidence.clone(),
                source_event: None,
            },
            candidate_objects: vec![ku_ref],
            index_commitment: ctx.index_commitment.clone(),
            model_commitment: None,
            rule_commitment: Some(rule_commitment),
            scores: scores(&checks, ctx.metrics)?,
            constraints: observations,
            expiry: ProposalExpiry {
                created_at_evaluation: ctx.created_at_evaluation,
                expires_after_evaluations: ctx.expires_after_evaluations,
                source_frontier: ctx.source_frontier,
            },
            privacy: need.privacy,
        };
        proposal.validate()?;
        Ok(SfsMatchResult {
            outcome: MatcherOutcome::Proposal {
                proposal: Box::new(proposal),
                checks,
            },
            virtual_affordance,
            coverage,
            coverage_scope: ctx.coverage_scope,
        })
    }

    /// Two-phase local evaluation (`Phase 1 Recall -> Phase 2 Exact 3-State Verify`, `P3` §3.2).
    pub fn evaluate_two_phase_local(
        index: &RebuildableSemanticIndex,
        kus: &[(ObjectReference, SemanticFrameSet)],
        need: &KnowledgeNeedIr,
        receptor: &ReceptorDefinition,
        ctx: &SfsMatchContext<'_>,
    ) -> Result<TwoPhaseSfsEvaluation, MatcherError> {
        let recall = index.recall_for_need_with_lifecycle(need, ctx.lifecycle);
        let recalled_set: std::collections::BTreeSet<[u8; 32]> = recall
            .candidate_cids
            .iter()
            .map(|cid| cid.into_bytes())
            .collect();

        let mut proposals = Vec::new();
        let mut hard_mismatches = 0usize;
        for (ku_ref, semantic) in kus {
            if !recalled_set.contains(&ku_ref.cid) {
                continue;
            }
            let matched =
                Self::match_semantic_frame_set(need, receptor, ku_ref.clone(), semantic, ctx)?;
            match matched.outcome {
                MatcherOutcome::Proposal { proposal, .. } => proposals.push(*proposal),
                MatcherOutcome::HardMismatch { .. } => hard_mismatches += 1,
            }
        }

        let coverage = CoverageStatement {
            selector: ctx.selector,
            assessed_frontier: vec![ctx.source_frontier],
            basis: CoverageBasis::ExactInventory,
            status: CoverageStatus::CompleteWithinSelector,
            returned_records: proposals.len() as u64,
            returned_bytes: 0,
            continuation: None,
            limitations: Vec::new(),
        };
        coverage.validate()?;

        Ok(TwoPhaseSfsEvaluation {
            recalled_cids: recall.candidate_cids,
            lifecycle_redirect_applied: recall.lifecycle_redirect_applied,
            proposals,
            hard_mismatches,
            coverage,
            coverage_scope: ctx.coverage_scope,
        })
    }
}

/// Explicit local coverage boundary label (`P3` §3.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SfsCoverageScope {
    LocalValidatedAndAuthorizedVault,
    PublicStoreOnly,
}

impl SfsCoverageScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalValidatedAndAuthorizedVault => "local_validated_and_authorized_vault",
            Self::PublicStoreOnly => "public_store_only",
        }
    }
}

/// Evaluation context for [`ExactTypedMatcher::match_semantic_frame_set`].
#[derive(Clone)]
pub struct SfsMatchContext<'a> {
    pub selector: SelectorCid,
    pub coverage_scope: SfsCoverageScope,
    pub lifecycle: Option<&'a ConceptLifecycleTable>,
    pub generator: ObjectReference,
    pub evidence: Vec<ObjectReference>,
    pub index_commitment: Option<ObjectReference>,
    pub metrics: MatcherMetricConcepts,
    pub unmapped_reason: ConceptCcid,
    pub source_frontier: EventCid,
    pub created_at_evaluation: u64,
    pub expires_after_evaluations: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SfsMatchResult {
    pub outcome: MatcherOutcome,
    pub virtual_affordance: KnowledgeAffordance,
    pub coverage: CoverageStatement,
    pub coverage_scope: SfsCoverageScope,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwoPhaseSfsEvaluation {
    pub recalled_cids: Vec<ObjectCid>,
    pub lifecycle_redirect_applied: bool,
    pub proposals: Vec<BindingProposal>,
    pub hard_mismatches: usize,
    pub coverage: CoverageStatement,
    pub coverage_scope: SfsCoverageScope,
}

fn virtual_sfs_rule_commitment() -> ObjectReference {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:virtual-sfs-affordance:rule:1\0");
    hasher.update(VIRTUAL_SFS_AFFORDANCE_PROFILE.as_bytes());
    ObjectReference::new(0, *hasher.finalize().as_bytes())
}

fn substitute_term(
    term: &TermRef,
    vars: &BTreeMap<VariableId, TermRef>,
    slots: &BTreeMap<ReceptorSlotId, TermRef>,
) -> TermRef {
    match term {
        TermRef::Variable { id, .. } => vars.get(id).cloned().unwrap_or_else(|| term.clone()),
        TermRef::Receptor { slot, .. } => slots.get(slot).cloned().unwrap_or_else(|| term.clone()),
        _ => term.clone(),
    }
}

fn substitute_constraint(
    constraint: &TypedConstraint,
    vars: &BTreeMap<VariableId, TermRef>,
    slots: &BTreeMap<ReceptorSlotId, TermRef>,
) -> TypedConstraint {
    let expression = match &constraint.expression {
        ConstraintExpression::Compare {
            left,
            operator,
            right,
        } => ConstraintExpression::Compare {
            left: substitute_term(left, vars, slots),
            operator: *operator,
            right: substitute_term(right, vars, slots),
        },
        ConstraintExpression::Dimension { term, expected } => ConstraintExpression::Dimension {
            term: substitute_term(term, vars, slots),
            expected: *expected,
        },
        ConstraintExpression::Range {
            term,
            lower,
            upper,
            include_lower,
            include_upper,
        } => ConstraintExpression::Range {
            term: substitute_term(term, vars, slots),
            lower: lower.clone(),
            upper: upper.clone(),
            include_lower: *include_lower,
            include_upper: *include_upper,
        },
    };
    TypedConstraint {
        expression,
        required: constraint.required,
    }
}

fn sfs_affine_transform(
    resolved_left: &TermRef,
    original_left: &TermRef,
    right: &TermRef,
    goal: &SemanticFrameSet,
    receptor: &ReceptorDefinition,
) -> Result<MappingTransform, MatcherError> {
    if quantity(resolved_left).is_some() && quantity(original_left).is_some() {
        return unit_transform(original_left, right);
    }
    // When original_left is an open variable constrained by a Quantity comparison/range in the
    // NeedIR (e.g. `?x >= 100 degC` matched against `373.15 K`), record the affine SI unit
    // transform between the NeedIR constraint unit and the KU quantity unit.
    if matches!(
        original_left,
        TermRef::Variable { .. } | TermRef::Receptor { .. }
    ) {
        for c in receptor
            .hard_constraints
            .iter()
            .chain(goal.statements.iter().flat_map(|s| s.constraints.iter()))
        {
            match &c.expression {
                ConstraintExpression::Compare {
                    left: c_left,
                    right: c_right,
                    ..
                } => {
                    if c_left == original_left && quantity(c_right).is_some() {
                        return unit_transform(c_right, right);
                    }
                    if c_right == original_left && quantity(c_left).is_some() {
                        return unit_transform(c_left, right);
                    }
                }
                ConstraintExpression::Range {
                    term: c_term,
                    lower,
                    ..
                } if c_term == original_left => {
                    let lower_term = TermRef::Literal(LiteralValue::Quantity(lower.clone()));
                    return unit_transform(&lower_term, right);
                }
                _ => {}
            }
        }
    }
    unit_transform(resolved_left, right)
}

#[allow(clippy::too_many_arguments)]
fn assess_sfs_statement<'a>(
    required: &StatementFrame,
    candidate: &'a StatementFrame,
    candidate_frames: &'a SemanticFrameSet,
    local_context: &SemanticFrameSet,
    source_index: u32,
    target_index: u32,
    lifecycle: Option<&ConceptLifecycleTable>,
) -> StatementAssessment<'a> {
    let at = |kind, evaluation, req| MatchCheck {
        kind,
        evaluation,
        required: req,
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
        && term_compatibility_with_lifecycle(
            &required.arguments[0],
            &candidate.arguments[1],
            lifecycle,
        ) == ConstraintEvaluation::Satisfied
        && term_compatibility_with_lifecycle(
            &required.arguments[1],
            &candidate.arguments[0],
            lifecycle,
        ) == ConstraintEvaluation::Satisfied
        && !(term_compatibility_with_lifecycle(
            &required.arguments[0],
            &candidate.arguments[0],
            lifecycle,
        ) == ConstraintEvaluation::Satisfied
            && term_compatibility_with_lifecycle(
                &required.arguments[1],
                &candidate.arguments[1],
                lifecycle,
            ) == ConstraintEvaluation::Satisfied);
    checks.push(at(
        MatchCheckKind::ArgumentDirection,
        bool_evaluation(!reversed),
        true,
    ));
    for (left, right) in required.arguments.iter().zip(&candidate.arguments) {
        let eval = match left {
            TermRef::Variable { .. } | TermRef::Receptor { .. } => ConstraintEvaluation::Satisfied,
            _ => term_compatibility_with_lifecycle(left, right, lifecycle),
        };
        checks.push(at(
            if quantity(left).is_some() || quantity(right).is_some() {
                MatchCheckKind::UnitDimension
            } else {
                MatchCheckKind::ArgumentType
            },
            eval,
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
        strict_qualifier_compatibility(
            required.qualifiers.time.as_ref(),
            candidate.qualifiers.time.as_ref(),
            lifecycle,
        ),
        required.qualifiers.time.is_some(),
    ));
    checks.push(at(
        MatchCheckKind::Location,
        strict_qualifier_compatibility(
            required.qualifiers.location.as_ref(),
            candidate.qualifiers.location.as_ref(),
            lifecycle,
        ),
        required.qualifiers.location.is_some(),
    ));
    checks.push(at(
        MatchCheckKind::Perspective,
        strict_qualifier_compatibility(
            required.qualifiers.perspective.as_ref(),
            candidate.qualifiers.perspective.as_ref(),
            lifecycle,
        ),
        required.qualifiers.perspective.is_some(),
    ));
    if let Some(cond_id) = candidate.qualifiers.condition {
        let cond_eval =
            evaluate_statement_condition(cond_id, candidate_frames, local_context, lifecycle);
        checks.push(at(MatchCheckKind::Condition, cond_eval, true));
    }
    StatementAssessment {
        target_index,
        target: candidate,
        checks,
    }
}

fn evaluate_statement_condition(
    cond_id: StatementId,
    candidate_frames: &SemanticFrameSet,
    local_context: &SemanticFrameSet,
    lifecycle: Option<&ConceptLifecycleTable>,
) -> ConstraintEvaluation {
    let Some(cond_stmt) = candidate_frames
        .statements
        .iter()
        .find(|s| s.statement_id == cond_id)
    else {
        return ConstraintEvaluation::Unknown;
    };
    let matching: Vec<_> = local_context
        .statements
        .iter()
        .filter(|ctx_stmt| {
            concepts_equivalent(
                ctx_stmt.operator_or_predicate,
                cond_stmt.operator_or_predicate,
                lifecycle,
            ) && ctx_stmt.arguments.len() == cond_stmt.arguments.len()
                && ctx_stmt
                    .arguments
                    .iter()
                    .zip(&cond_stmt.arguments)
                    .all(|(a, b)| {
                        term_compatibility_with_lifecycle(a, b, lifecycle)
                            == ConstraintEvaluation::Satisfied
                    })
        })
        .collect();
    if matching
        .iter()
        .any(|m| m.qualifiers.negated != cond_stmt.qualifiers.negated)
    {
        ConstraintEvaluation::Violated
    } else if matching
        .iter()
        .any(|m| m.qualifiers.negated == cond_stmt.qualifiers.negated)
    {
        ConstraintEvaluation::Satisfied
    } else {
        ConstraintEvaluation::Unknown
    }
}

fn strict_qualifier_compatibility(
    required: Option<&TermRef>,
    candidate: Option<&TermRef>,
    lifecycle: Option<&ConceptLifecycleTable>,
) -> ConstraintEvaluation {
    match (required, candidate) {
        (None, _) => ConstraintEvaluation::Satisfied,
        (Some(_), None) => ConstraintEvaluation::Unknown,
        (
            Some(TermRef::Literal(LiteralValue::Quantity(req))),
            Some(TermRef::Literal(LiteralValue::Quantity(cand))),
        ) => {
            if req.source_unit.dimension != cand.source_unit.dimension {
                ConstraintEvaluation::Violated
            } else {
                match req.compare(cand) {
                    Ok(Ordering::Equal) => ConstraintEvaluation::Satisfied,
                    Ok(_) => ConstraintEvaluation::Violated,
                    Err(_) => ConstraintEvaluation::Unknown,
                }
            }
        }
        (Some(req), Some(cand)) => term_compatibility_with_lifecycle(req, cand, lifecycle),
    }
}

fn concepts_equivalent(
    left: ConceptCcid,
    right: ConceptCcid,
    lifecycle: Option<&ConceptLifecycleTable>,
) -> bool {
    if left == right {
        return true;
    }
    let Some(table) = lifecycle else {
        return false;
    };
    let canon = |c: ConceptCcid| match table.resolve_for_authoring(c) {
        AuthoringConceptResolution::Redirected { to, .. } => Some(to),
        AuthoringConceptResolution::Canonical(x) => Some(x),
        AuthoringConceptResolution::NeedsSplitDisambiguation { .. }
        | AuthoringConceptResolution::Retired { .. } => None,
    };
    match (canon(left), canon(right)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

fn term_compatibility_with_lifecycle(
    left: &TermRef,
    right: &TermRef,
    lifecycle: Option<&ConceptLifecycleTable>,
) -> ConstraintEvaluation {
    match (left, right) {
        (TermRef::Concept(a), TermRef::Concept(b)) => {
            bool_evaluation(concepts_equivalent(*a, *b, lifecycle))
        }
        _ => term_compatibility(left, right),
    }
}

fn evaluate_constraint_with_lifecycle(
    constraint: &TypedConstraint,
    lifecycle: Option<&ConceptLifecycleTable>,
) -> ConstraintEvaluation {
    match &constraint.expression {
        ConstraintExpression::Compare {
            left: TermRef::Concept(a),
            operator,
            right: TermRef::Concept(b),
        } => match operator {
            ComparisonOperator::Equal => bool_evaluation(concepts_equivalent(*a, *b, lifecycle)),
            ComparisonOperator::NotEqual => {
                bool_evaluation(!concepts_equivalent(*a, *b, lifecycle))
            }
            _ => ConstraintEvaluation::Unknown,
        },
        _ => evaluate_constraint(constraint),
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
    Affordance(AffordanceError),
    Inventory(InventoryError),
    ProposalMustRemainPrivate,
    MissingReceptorReference,
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

impl From<AffordanceError> for MatcherError {
    fn from(error: AffordanceError) -> Self {
        Self::Affordance(error)
    }
}

impl From<InventoryError> for MatcherError {
    fn from(error: InventoryError) -> Self {
        Self::Inventory(error)
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

    #[test]
    fn match_semantic_frame_set_performs_affine_si_conversion_and_3state_qualifiers() {
        use crate::vnext_semantic_index::SemanticIndexSource;
        use ku_core::foundation::{
            public_knowledge_exchange_fixture_v1, SEMANTIC_KERNEL_OBJECT_KIND,
        };

        let deg_c = UnitRef {
            unit: concept(70),
            dimension: DimensionVector::TEMPERATURE,
            scale_to_base: ExactRatio::integer(1),
            offset_to_base: ExactRatio::new(27_315, 100).unwrap(),
        };
        let kelvin = UnitRef::coherent(concept(71), DimensionVector::TEMPERATURE);
        let km = UnitRef {
            unit: concept(72),
            dimension: DimensionVector::LENGTH,
            scale_to_base: ExactRatio::integer(1_000),
            offset_to_base: ExactRatio::integer(0),
        };
        let meter = UnitRef::coherent(concept(73), DimensionVector::LENGTH);

        // Need: (?water)-[:boiling_point]->(?t) with ?t >= 100 degC and location = atm_loc
        let pred_boil = concept(3);
        let subj_water = concept(60);
        let loc_sea_level = concept(65);
        let var_t = VariableId(0);
        let goal = SemanticFrameSet {
            statements: vec![StatementFrame {
                statement_id: StatementId(0),
                operator_or_predicate: pred_boil,
                arguments: vec![
                    TermRef::Concept(subj_water),
                    TermRef::Variable {
                        id: var_t,
                        type_constraint: None,
                    },
                ],
                constraints: vec![TypedConstraint {
                    expression: ConstraintExpression::Compare {
                        left: TermRef::Variable {
                            id: var_t,
                            type_constraint: None,
                        },
                        operator: ComparisonOperator::GreaterThanOrEqual,
                        right: TermRef::Literal(LiteralValue::Quantity(QuantityLiteral {
                            value: ExactRatio::integer(100),
                            source_unit: deg_c,
                        })),
                    },
                    required: true,
                }],
                qualifiers: StatementQualifiers {
                    location: Some(TermRef::Concept(loc_sea_level)),
                    ..StatementQualifiers::default()
                },
            }],
        }
        .alpha_normalized()
        .unwrap();

        let mut rec = receptor();
        rec.role = pred_boil;
        rec.hard_constraints = goal.statements[0].constraints.clone();

        let need = KnowledgeNeedIr {
            receptor_definitions: vec![reference(10)],
            desired_roles: vec![pred_boil],
            goal,
            local_context: empty_frames(),
            privacy: DisclosureClass::LocalOnly,
        };

        // KU 1: water boiling_point 373.15 K (no location -> Unknown qualifier, kept unresolved)
        let ku_kelvin = SemanticFrameSet {
            statements: vec![StatementFrame {
                statement_id: StatementId(0),
                operator_or_predicate: pred_boil,
                arguments: vec![
                    TermRef::Concept(subj_water),
                    TermRef::Literal(LiteralValue::Quantity(QuantityLiteral {
                        value: ExactRatio::new(37_315, 100).unwrap(),
                        source_unit: kelvin.clone(),
                    })),
                ],
                constraints: Vec::new(),
                qualifiers: StatementQualifiers::default(),
            }],
        };

        // KU 2: water boiling_point 350 K (< 373.15 K -> Violated hard constraint)
        let ku_too_cool = SemanticFrameSet {
            statements: vec![StatementFrame {
                statement_id: StatementId(0),
                operator_or_predicate: pred_boil,
                arguments: vec![
                    TermRef::Concept(subj_water),
                    TermRef::Literal(LiteralValue::Quantity(QuantityLiteral {
                        value: ExactRatio::integer(350),
                        source_unit: kelvin,
                    })),
                ],
                constraints: Vec::new(),
                qualifiers: StatementQualifiers::default(),
            }],
        };

        let selector = public_knowledge_exchange_fixture_v1().cid().unwrap();
        let ctx = SfsMatchContext {
            selector,
            coverage_scope: SfsCoverageScope::LocalValidatedAndAuthorizedVault,
            lifecycle: None,
            generator: reference(30),
            evidence: Vec::new(),
            index_commitment: None,
            metrics: MatcherMetricConcepts {
                structural_fit: concept(40),
                constraint_fit: concept(41),
            },
            unmapped_reason: concept(42),
            source_frontier: EventCid::from_bytes([50; 32]),
            created_at_evaluation: 1,
            expires_after_evaluations: 10,
        };

        let ku1_ref = ObjectReference::new(SEMANTIC_KERNEL_OBJECT_KIND.0, [81; 32]);
        let ku2_ref = ObjectReference::new(SEMANTIC_KERNEL_OBJECT_KIND.0, [82; 32]);
        let res1 = ExactTypedMatcher::match_semantic_frame_set(
            &need,
            &rec,
            ku1_ref.clone(),
            &ku_kelvin,
            &ctx,
        )
        .unwrap();
        let prop = res1
            .outcome
            .proposal()
            .expect("373.15 K == 100 degC satisfies >= 100 degC");
        assert_eq!(res1.coverage.returned_records, 1);
        assert_eq!(
            res1.coverage_scope.as_str(),
            "local_validated_and_authorized_vault"
        );
        // Location qualifier was absent on KU 1 -> Unknown
        assert!(res1.outcome.checks().iter().any(|c| {
            c.kind == MatchCheckKind::Location && c.evaluation == ConstraintEvaluation::Unknown
        }));
        // Affine transform degC -> K recorded on argument 1
        assert!(prop
            .mapping_kernel
            .correspondences
            .iter()
            .any(|corr| { matches!(corr.transform, MappingTransform::AffineUnit { .. }) }));

        // 350 K violates >= 100 degC
        let res2 = ExactTypedMatcher::match_semantic_frame_set(
            &need,
            &rec,
            ku2_ref.clone(),
            &ku_too_cool,
            &ctx,
        )
        .unwrap();
        assert!(res2.outcome.proposal().is_none());

        // Also verify km -> m two-phase evaluation
        let km_ku = SemanticFrameSet {
            statements: vec![StatementFrame {
                statement_id: StatementId(0),
                operator_or_predicate: pred_boil,
                arguments: vec![
                    TermRef::Concept(subj_water),
                    TermRef::Literal(LiteralValue::Quantity(QuantityLiteral {
                        value: ExactRatio::new(3, 2).unwrap(), // 1.5 km = 1500 m
                        source_unit: km,
                    })),
                ],
                constraints: Vec::new(),
                qualifiers: StatementQualifiers::default(),
            }],
        };
        let km_ref = ObjectReference::new(SEMANTIC_KERNEL_OBJECT_KIND.0, [83; 32]);
        let len_goal = SemanticFrameSet {
            statements: vec![StatementFrame {
                statement_id: StatementId(0),
                operator_or_predicate: pred_boil,
                arguments: vec![
                    TermRef::Concept(subj_water),
                    TermRef::Literal(LiteralValue::Quantity(QuantityLiteral {
                        value: ExactRatio::integer(1_500),
                        source_unit: meter,
                    })),
                ],
                constraints: Vec::new(),
                qualifiers: StatementQualifiers::default(),
            }],
        };
        let mut len_rec = receptor();
        len_rec.role = pred_boil;
        let len_need = KnowledgeNeedIr {
            receptor_definitions: vec![reference(10)],
            desired_roles: vec![pred_boil],
            goal: len_goal,
            local_context: empty_frames(),
            privacy: DisclosureClass::LocalOnly,
        };
        let index = RebuildableSemanticIndex::rebuild(&[SemanticIndexSource::KnowledgeUnit {
            cid: ObjectCid::from_bytes(km_ref.cid),
            semantic: &km_ku,
        }]);
        let two_phase = ExactTypedMatcher::evaluate_two_phase_local(
            &index,
            &[(km_ref, km_ku)],
            &len_need,
            &len_rec,
            &ctx,
        )
        .unwrap();
        assert_eq!(two_phase.proposals.len(), 1);
    }
}
