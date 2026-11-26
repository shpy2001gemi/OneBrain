//! Deterministic 5-Stage `P7 <-> P3` Cross-Domain Discovery & Assembly Pipeline
//! (`P7-OBKG-UNIFIED-001` §3–§4.1).
//!
//! Orchestrates:
//! 1. **Stage D1:** `RebuildableSemanticIndex` (with `ConceptLifecycleTable` expansion) +
//!    `ReunionFrontier` incremental delta joins.
//! 2. **Stage D2:** `StructuralSignatureIndex` vocabulary-neutral cross-domain recall +
//!    firewalled recall priors.
//! 3. **Stage D3:** `TypedRelationalAligner` bounded SME structural alignment.
//! 4. **Stage D4:** `AssemblySearcher` weighted 3-state CSP search across Assembly sizes `2..=4`.
//! 5. **Stage D5:** `ExplorationScheduler` (`10 / 20 / 30 / 40` cohorts + `starvation_debt`) +
//!    `LocalSalienceTieBreaker` restricted strictly to tie-breaking inside the `40%` exploit
//!    cohort + local `ExposureObservation` inverse-propensity logging (`PropensityAwareLearner`).
//!
//! Every surviving candidate terminates exclusively as a non-executable [`BindingProposal`]
//! inside [`ProposalQuarantine`] (`FW-02`).

use std::collections::{BTreeMap, BTreeSet};

use ku_core::concept_lifecycle::ConceptLifecycleTable;
use ku_core::foundation::{
    ConceptCcid, ConstraintEvaluation, DisclosureClass, EventCid, ExactRatio, ExposureKind,
    ExposureObservation, KnowledgeAffordance, MappingConstraintRegion, MappingEnvelope, ObjectCid,
    ObjectReference, ReceptorDefinition, SemanticFrameSet, SEMANTIC_KERNEL_OBJECT_KIND,
};

use crate::vnext_assembly_search::{
    AssemblyCompositionCandidate, AssemblyFragmentCandidate, AssemblyParetoPortfolio,
    AssemblySearchError, AssemblySearchPolicy, AssemblySearchRequest, AssemblySearcher,
    PairCompatibility, PlacementFit, PlacementRequirement,
};
use crate::vnext_exploration::{
    CandidateEligibility, CandidateLane, DiscoveryProfile, ExplorationCandidate, ExplorationCohort,
    ExplorationError, ExplorationPolicyV1, ExplorationScheduler, ExplorationState, SelectedLane,
    SelectionBatchRequest, SelectionScope,
};
use crate::vnext_matcher::{MatcherError, MatcherMetricConcepts};
use crate::vnext_proposal::{
    BindingProposal, ConstraintObservation, ProposalError, ProposalExpiry, ProposalId,
    ProposalQuarantine, ScoreComponent, ScoreDirection,
};
use crate::vnext_query::KnowledgeNeedIr;
use crate::vnext_query_view::{PresentedFeedback, PropensityAwareLearner, QueryViewError};
use crate::vnext_relational_alignment::{
    RelationalAlignment, RelationalAlignmentError, RelationalAlignmentPolicy,
    RelationalAlignmentRequest, TypedRelationalAligner,
};
use crate::vnext_reunion::{
    LocalNeedTarget, ReunionBudget, ReunionDeltaReport, ReunionError, ReunionFrontier,
    ValidatedRemoteAffordance,
};
use crate::vnext_semantic_index::{RebuildableSemanticIndex, SemanticIndexSource};
use crate::vnext_structural_signature::{
    StructuralSignatureError, StructuralSignatureIndex, StructuralSignatureKind,
    StructuralSignatureSource, VocabularySensitivity,
};

/// Disposable `LOCAL_ONLY` salience cache (`P7` §4.1, `P8` `purpose = 4`).
///
/// Stores local activation/decay/STDP salience weights. Strictly restricted to tie-breaking
/// among already-selected or equal-priority candidates inside the `Exploit` cohort — it can
/// NEVER evict canonical objects (`NEG-KU-002`), override the `KQL-010` exploration floor,
/// or enter canonical/network state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalSalienceTieBreaker {
    weights_milli: BTreeMap<[u8; 32], u64>,
}

impl LocalSalienceTieBreaker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_salience_milli(&mut self, object_cid: [u8; 32], weight_milli: u64) {
        self.weights_milli.insert(object_cid, weight_milli);
    }

    pub fn salience_milli(&self, object_cid: &[u8; 32]) -> u64 {
        self.weights_milli.get(object_cid).copied().unwrap_or(0)
    }

    /// Apply integer exponential decay `w' = w * retain_bps / 10_000` locally.
    pub fn apply_decay(&mut self, retain_bps: u16) {
        let clamped = u64::from(retain_bps.min(10_000));
        for weight in self.weights_milli.values_mut() {
            *weight = (*weight).saturating_mul(clamped) / 10_000;
        }
    }

    /// Record spike-timing co-exercise increment locally without touching `ValidatedStore`.
    pub fn record_co_activation(&mut self, object_cid: [u8; 32], delta_milli: u64) {
        let entry = self.weights_milli.entry(object_cid).or_insert(0);
        *entry = entry.saturating_add(delta_milli);
    }

    /// Sort `Exploit`-lane records only; `Explore` records retain their exact cohort-scheduled
    /// positions and are never demoted or suppressed by salience.
    pub fn order_exploit_ties_only(&self, ordered_ids: &mut [([u8; 32], SelectedLane)]) {
        // Partition exploit indices so explore positions remain untouched, then sort exploit
        // items by descending salience (breaking ties deterministically by candidate_id).
        let exploit_positions: Vec<usize> = ordered_ids
            .iter()
            .enumerate()
            .filter_map(|(idx, (_, lane))| (*lane == SelectedLane::Exploit).then_some(idx))
            .collect();
        let mut exploit_items: Vec<([u8; 32], SelectedLane)> = exploit_positions
            .iter()
            .map(|&idx| ordered_ids[idx])
            .collect();
        exploit_items.sort_by(|(id_a, _), (id_b, _)| {
            self.salience_milli(id_b)
                .cmp(&self.salience_milli(id_a))
                .then_with(|| id_a.cmp(id_b))
        });
        for (pos, item) in exploit_positions.into_iter().zip(exploit_items) {
            ordered_ids[pos] = item;
        }
    }

    pub const fn is_local_only(&self) -> bool {
        true
    }

    pub const fn can_evict_validated_store(&self) -> bool {
        false
    }

    pub const fn can_override_exploration_floor(&self) -> bool {
        false
    }
}

/// Candidate KU or Affordance supplied to the 5-stage discovery pipeline.
#[derive(Clone, Debug)]
pub struct DiscoveryCorpusEntry {
    pub object: ObjectReference,
    pub semantic: SemanticFrameSet,
    pub explicit_affordance: Option<KnowledgeAffordance>,
    pub zero_use: bool,
    pub evidence_domain: [u8; 32],
    pub supporting_evidence_count: u32,
}

/// Request into [`DiscoveryPipelineOrchestrator::run`].
pub struct DiscoveryPipelineRequest<'a> {
    pub need: &'a KnowledgeNeedIr,
    pub receptor: &'a ReceptorDefinition,
    pub local_reunion_targets: &'a [LocalNeedTarget],
    pub remote_affordance_deltas: Vec<ValidatedRemoteAffordance>,
    pub reunion_budget: Option<ReunionBudget>,
    pub corpus: &'a [DiscoveryCorpusEntry],
    pub lifecycle_table: Option<&'a ConceptLifecycleTable>,
    /// Firewalled recall priors (`AI-002` / KGE); never grant symbolic validity.
    pub recall_priors: &'a [(ObjectReference, ExactRatio)],
    pub placements: &'a [PlacementRequirement],
    pub pair_compatibilities: &'a [PairCompatibility],
    pub alignment_policy: RelationalAlignmentPolicy,
    pub assembly_policy: AssemblySearchPolicy,
    pub exploration_policy: ExplorationPolicyV1,
    pub discovery_profile: DiscoveryProfile,
    pub presentation_slots: usize,
    pub salience_tie_breaker: &'a LocalSalienceTieBreaker,
    pub generator: ObjectReference,
    pub assumption_predicate: ConceptCcid,
    pub unmapped_source_reason: ConceptCcid,
    pub unmapped_target_reason: ConceptCcid,
    pub metrics: MatcherMetricConcepts,
    pub source_frontier: EventCid,
    pub frontier_digest: [u8; 32],
    pub private_context_commitment: [u8; 32],
    pub created_at_evaluation: u64,
    pub expires_after_evaluations: u64,
}

/// Structured trace & output of a deterministic 5-stage discovery execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveryPipelineReport {
    pub d1_exact_recalled_cids: Vec<ObjectCid>,
    pub d1_lifecycle_redirect_applied: bool,
    pub d1_reunion_report: Option<ReunionDeltaReport>,
    pub d2_structural_recalled_cids: Vec<ObjectCid>,
    pub d3_actionable_alignments: usize,
    pub d3_hard_rejected: usize,
    pub d4_assembly_portfolio: Vec<AssemblyCompositionCandidate>,
    pub d5_selected_candidates: Vec<([u8; 32], SelectedLane)>,
    pub d5_logged_exposures: usize,
    pub quarantined_proposal_ids: Vec<ProposalId>,
}

impl DiscoveryPipelineReport {
    pub const fn emits_use_evidence(&self) -> bool {
        false
    }

    pub const fn auto_adopts_or_materializes(&self) -> bool {
        false
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum DiscoveryPipelineError {
    Reunion(ReunionError),
    StructuralSignature(StructuralSignatureError),
    Alignment(RelationalAlignmentError),
    AssemblySearch(AssemblySearchError),
    Exploration(ExplorationError),
    QueryView(QueryViewError),
    Proposal(ProposalError),
    Matcher(MatcherError),
    Semantic(ku_core::foundation::SemanticError),
    MissingReceptorReference,
}

impl From<ReunionError> for DiscoveryPipelineError {
    fn from(err: ReunionError) -> Self {
        Self::Reunion(err)
    }
}

impl From<StructuralSignatureError> for DiscoveryPipelineError {
    fn from(err: StructuralSignatureError) -> Self {
        Self::StructuralSignature(err)
    }
}

impl From<RelationalAlignmentError> for DiscoveryPipelineError {
    fn from(err: RelationalAlignmentError) -> Self {
        Self::Alignment(err)
    }
}

impl From<AssemblySearchError> for DiscoveryPipelineError {
    fn from(err: AssemblySearchError) -> Self {
        Self::AssemblySearch(err)
    }
}

impl From<ExplorationError> for DiscoveryPipelineError {
    fn from(err: ExplorationError) -> Self {
        Self::Exploration(err)
    }
}

impl From<QueryViewError> for DiscoveryPipelineError {
    fn from(err: QueryViewError) -> Self {
        Self::QueryView(err)
    }
}

impl From<ProposalError> for DiscoveryPipelineError {
    fn from(err: ProposalError) -> Self {
        Self::Proposal(err)
    }
}

impl From<MatcherError> for DiscoveryPipelineError {
    fn from(err: MatcherError) -> Self {
        Self::Matcher(err)
    }
}

impl From<ku_core::foundation::SemanticError> for DiscoveryPipelineError {
    fn from(err: ku_core::foundation::SemanticError) -> Self {
        Self::Semantic(err)
    }
}

/// Deterministic 5-Stage `P7 <-> P3` Discovery Orchestrator.
pub struct DiscoveryPipelineOrchestrator;

impl DiscoveryPipelineOrchestrator {
    pub fn run(
        mut req: DiscoveryPipelineRequest<'_>,
        reunion_frontier: &mut ReunionFrontier,
        exploration_state: &mut ExplorationState,
        learner: &mut PropensityAwareLearner,
        quarantine: &mut ProposalQuarantine,
    ) -> Result<DiscoveryPipelineReport, DiscoveryPipelineError> {
        let receptor_ref = req
            .need
            .receptor_definitions
            .first()
            .cloned()
            .ok_or(DiscoveryPipelineError::MissingReceptorReference)?;

        // ─── Stage D1: Exact Semantic Index + Reunion Delta Join ─────────────
        let mut sem_sources = Vec::with_capacity(req.corpus.len() + 1);
        sem_sources.push(SemanticIndexSource::Receptor {
            cid: ObjectCid::from_bytes(receptor_ref.cid),
            definition: req.receptor,
        });
        for entry in req.corpus {
            let cid = ObjectCid::from_bytes(entry.object.cid);
            if let Some(aff) = &entry.explicit_affordance {
                sem_sources.push(SemanticIndexSource::Affordance {
                    cid,
                    affordance: aff,
                });
            } else {
                sem_sources.push(SemanticIndexSource::KnowledgeUnit {
                    cid,
                    semantic: &entry.semantic,
                });
            }
        }
        let semantic_index = RebuildableSemanticIndex::rebuild(&sem_sources);
        let d1_recall =
            semantic_index.recall_for_need_with_lifecycle(req.need, req.lifecycle_table);
        let d1_set: BTreeSet<[u8; 32]> = d1_recall
            .candidate_cids
            .iter()
            .map(|c| c.into_bytes())
            .collect();

        let remote_deltas = std::mem::take(&mut req.remote_affordance_deltas);
        let d1_reunion_report = if !remote_deltas.is_empty() {
            let budget = req.reunion_budget.unwrap_or(ReunionBudget {
                max_delta_objects: 64,
                max_pairs: 1_024,
                max_proposals: 64,
            });
            Some(reunion_frontier.join_affordance_delta(
                remote_deltas,
                req.local_reunion_targets,
                quarantine,
                budget,
            )?)
        } else {
            None
        };

        // ─── Stage D2: Vocabulary-Neutral Structural Signatures + Recall Priors
        let need_goal_cid =
            ObjectCid::from_bytes(*blake3::hash(&req.need.goal.canonical_bytes()?).as_bytes());
        let mut sig_sources = Vec::with_capacity(req.corpus.len() + 2);
        sig_sources.push(StructuralSignatureSource::Receptor {
            cid: ObjectCid::from_bytes(receptor_ref.cid),
            definition: req.receptor,
        });
        sig_sources.push(StructuralSignatureSource::KnowledgeUnit {
            cid: need_goal_cid,
            semantic: &req.need.goal,
        });
        for entry in req.corpus {
            let cid = ObjectCid::from_bytes(entry.object.cid);
            if let Some(aff) = &entry.explicit_affordance {
                sig_sources.push(StructuralSignatureSource::Affordance {
                    cid,
                    affordance: aff,
                });
            } else {
                sig_sources.push(StructuralSignatureSource::KnowledgeUnit {
                    cid,
                    semantic: &entry.semantic,
                });
            }
        }
        let sig_index = StructuralSignatureIndex::rebuild(&sig_sources)?;
        let query_sigs = sig_index.signatures_for(need_goal_cid);
        let mut d2_set = BTreeSet::<[u8; 32]>::new();
        for sig in &query_sigs {
            if sig.sensitivity == VocabularySensitivity::VocabularyIndependent
                && matches!(
                    sig.kind,
                    StructuralSignatureKind::FunctionBehaviorStructure
                        | StructuralSignatureKind::OperatorAst
                        | StructuralSignatureKind::GraphShingle
                        | StructuralSignatureKind::Dimension
                        | StructuralSignatureKind::UnitSemantics
                )
            {
                for hit_cid in sig_index.objects_for(sig.kind, sig.digest) {
                    if hit_cid != need_goal_cid && hit_cid.as_bytes() != &receptor_ref.cid {
                        d2_set.insert(hit_cid.into_bytes());
                    }
                }
            }
        }
        for (prior_ref, _) in req.recall_priors {
            d2_set.insert(prior_ref.cid);
        }
        let d2_structural_recalled_cids: Vec<ObjectCid> =
            d2_set.iter().copied().map(ObjectCid::from_bytes).collect();

        // Candidate pool = D1 union D2
        let candidate_cids: BTreeSet<[u8; 32]> = d1_set.union(&d2_set).copied().collect();

        // ─── Stage D3: Bounded SME Relational Alignment ──────────────────────
        struct AlignedEntry<'a> {
            corpus_entry: &'a DiscoveryCorpusEntry,
            alignment: RelationalAlignment,
            lane: CandidateLane,
            proposal: BindingProposal,
        }

        let mut aligned_entries = Vec::new();
        let mut d3_hard_rejected = 0usize;

        for entry in req.corpus {
            if !candidate_cids.contains(&entry.object.cid) {
                continue;
            }
            let target_sigs = sig_index.signatures_for(ObjectCid::from_bytes(entry.object.cid));

            // Check whether this is an opposing claim candidate (same predicate & arguments,
            // opposite negation) before general SME alignment.
            let is_opposing = is_opposing_claim(&req.need.goal, &entry.semantic);

            let alignment = TypedRelationalAligner::align(RelationalAlignmentRequest {
                source_object: receptor_ref.clone(),
                source_semantics: &req.need.goal,
                source_signatures: &query_sigs,
                target_object: entry.object.clone(),
                target_semantics: &entry.semantic,
                target_signatures: &target_sigs,
                assumption_predicate: req.assumption_predicate,
                unmapped_source_reason: req.unmapped_source_reason,
                unmapped_target_reason: req.unmapped_target_reason,
                policy: req.alignment_policy,
                cursor: None,
            })?;

            let actionable = alignment.is_actionable_candidate()
                || (is_opposing && !alignment.statement_matches.is_empty());
            if !actionable {
                d3_hard_rejected += 1;
                continue;
            }

            let lane = classify_candidate_lane(
                entry,
                &alignment,
                d1_set.contains(&entry.object.cid),
                is_opposing,
            );
            let proposal = build_proposal_from_alignment(&alignment, entry, &req)?;
            aligned_entries.push(AlignedEntry {
                corpus_entry: entry,
                alignment,
                lane,
                proposal,
            });
        }

        let d3_actionable_alignments = aligned_entries.len();

        // ─── Stage D4: Multi-KU Assembly Search (sizes 2..=4) ────────────────
        let mut assembly_portfolio = AssemblyParetoPortfolio::default();
        if !req.placements.is_empty() && aligned_entries.len() >= req.assembly_policy.min_size {
            let mut fragments = Vec::with_capacity(aligned_entries.len());
            for item in &aligned_entries {
                let pid = item.proposal.proposal_id()?;
                let eval = if item.alignment.evidence.unknown_argument_pairs == 0
                    && item.alignment.evidence.hard_violation_count == 0
                {
                    ConstraintEvaluation::Satisfied
                } else {
                    ConstraintEvaluation::Unknown
                };
                let fits: Vec<PlacementFit> = req
                    .placements
                    .iter()
                    .map(|p| PlacementFit {
                        placement_id: p.placement_id,
                        evaluation: eval,
                    })
                    .collect();
                fragments.push(AssemblyFragmentCandidate {
                    candidate_id: item.corpus_entry.object.cid,
                    object: item.corpus_entry.object.clone(),
                    mapping_proposal_commitment: Some(*pid.as_bytes()),
                    placement_fits: fits,
                    systematic_connections: item.alignment.evidence.systematic_connections,
                    supporting_evidence_count: item.corpus_entry.supporting_evidence_count,
                    evidence_domains: vec![item.corpus_entry.evidence_domain],
                    hard_violation_count: 0,
                });
            }
            let page = AssemblySearcher::search(AssemblySearchRequest {
                placements: req.placements,
                candidates: &fragments,
                pair_compatibilities: req.pair_compatibilities,
                policy: req.assembly_policy,
                continuation: None,
            })?;
            assembly_portfolio.merge_page(&page)?;
        }

        // ─── Stage D5: Exploration Portfolio + Salience Tie-Break + Exposure ─
        let mut by_cid: BTreeMap<[u8; 32], AlignedEntry<'_>> = BTreeMap::new();
        let mut exploration_candidates = Vec::with_capacity(aligned_entries.len());
        for item in aligned_entries {
            let cid = item.corpus_entry.object.cid;
            exploration_candidates.push(ExplorationCandidate {
                candidate_id: cid,
                lane: item.lane,
                eligibility: CandidateEligibility {
                    decodable: true,
                    has_compatibility_path: true,
                    privacy_permitted: true,
                    consent_permitted: true,
                    schema_supported: true,
                    within_resource_limits: true,
                },
            });
            by_cid.insert(cid, item);
        }

        let mut d5_selected_candidates = Vec::new();
        let mut d5_logged_exposures = 0usize;
        let mut quarantined_proposal_ids = Vec::new();

        if !exploration_candidates.is_empty() && req.presentation_slots > 0 {
            let batch = ExplorationScheduler::select_batch(
                req.exploration_policy,
                exploration_state,
                SelectionBatchRequest {
                    scope: SelectionScope::Complement(req.discovery_profile),
                    frontier_digest: req.frontier_digest,
                    slots: req.presentation_slots,
                    candidates: &exploration_candidates,
                },
            )?;

            for (ord, sel_record) in batch.selected.iter().enumerate() {
                d5_selected_candidates.push((sel_record.candidate_id, sel_record.lane));

                // Log local inverse-propensity ExposureObservation (NEVER UseEvidence!).
                let obs_id = derive_exposure_observation_id(
                    &req.frontier_digest,
                    &sel_record.candidate_id,
                    sel_record.selection_ordinal,
                );
                let target_ref = by_cid
                    .get(&sel_record.candidate_id)
                    .map(|item| item.corpus_entry.object.clone())
                    .unwrap_or_else(|| {
                        ObjectReference::new(SEMANTIC_KERNEL_OBJECT_KIND.0, sel_record.candidate_id)
                    });
                learner.record_exposure(
                    ExposureObservation {
                        observation_id: obs_id,
                        target: target_ref,
                        kind: ExposureKind::Presented,
                        local_sequence: ord as u64,
                        private_context_commitment: req.private_context_commitment,
                    },
                    PresentedFeedback::NoObservedResponse,
                    Some(sel_record),
                )?;
                d5_logged_exposures += 1;
            }

            // Break ties strictly inside the 40% Exploit cohort; Explore positions are untouched.
            req.salience_tie_breaker
                .order_exploit_ties_only(&mut d5_selected_candidates);

            // Terminate every selected candidate exclusively in ProposalQuarantine (`FW-02`).
            for (cand_cid, _) in &d5_selected_candidates {
                if let Some(item) = by_cid.remove(cand_cid) {
                    let pid = quarantine.insert(item.proposal)?;
                    quarantined_proposal_ids.push(pid);
                }
            }
        }

        if let Some(reunion_rep) = &d1_reunion_report {
            for r in &reunion_rep.proposals {
                if !quarantined_proposal_ids.contains(&r.proposal) {
                    quarantined_proposal_ids.push(r.proposal);
                }
            }
        }

        Ok(DiscoveryPipelineReport {
            d1_exact_recalled_cids: d1_recall.candidate_cids,
            d1_lifecycle_redirect_applied: d1_recall.lifecycle_redirect_applied,
            d1_reunion_report,
            d2_structural_recalled_cids,
            d3_actionable_alignments,
            d3_hard_rejected,
            d4_assembly_portfolio: assembly_portfolio.candidates().to_vec(),
            d5_selected_candidates,
            d5_logged_exposures,
            quarantined_proposal_ids,
        })
    }
}

fn is_opposing_claim(goal: &SemanticFrameSet, candidate: &SemanticFrameSet) -> bool {
    goal.statements.iter().any(|g| {
        candidate.statements.iter().any(|c| {
            g.operator_or_predicate == c.operator_or_predicate
                && g.arguments.len() == c.arguments.len()
                && g.qualifiers.negated != c.qualifiers.negated
        })
    })
}

fn classify_candidate_lane(
    entry: &DiscoveryCorpusEntry,
    alignment: &RelationalAlignment,
    in_d1_exact: bool,
    is_opposing: bool,
) -> CandidateLane {
    if is_opposing {
        CandidateLane::Explore(ExplorationCohort::OppositionAlternative)
    } else if !in_d1_exact && alignment.evidence.exact_predicate_pairs == 0 {
        CandidateLane::Explore(ExplorationCohort::CrossDomainStructural)
    } else if entry.zero_use {
        CandidateLane::Explore(ExplorationCohort::ColdOldLongTail)
    } else {
        CandidateLane::Exploit
    }
}

fn build_proposal_from_alignment(
    alignment: &RelationalAlignment,
    entry: &DiscoveryCorpusEntry,
    req: &DiscoveryPipelineRequest<'_>,
) -> Result<BindingProposal, DiscoveryPipelineError> {
    let mut kernel = alignment.mapping_kernel.clone();
    if kernel.constraint_regions.is_empty() {
        for c in &req.receptor.hard_constraints {
            kernel.constraint_regions.push(MappingConstraintRegion {
                constraint: c.clone(),
                evaluation: ConstraintEvaluation::Unknown,
            });
        }
    }
    let observations: Vec<ConstraintObservation> = kernel
        .constraint_regions
        .iter()
        .enumerate()
        .map(|(idx, region)| ConstraintObservation {
            constraint_index: idx as u32,
            evaluation: region.evaluation,
            required: region.constraint.required,
        })
        .collect();
    let total_pairs = alignment.evidence.matched_statement_pairs.max(1);
    let structural_ratio = ExactRatio::new(
        i64::from(alignment.evidence.satisfied_argument_pairs.min(total_pairs)),
        u64::from(total_pairs),
    )
    .unwrap_or(ExactRatio::integer(1));
    let constraint_ratio = ExactRatio::new(
        i64::from(alignment.evidence.exact_predicate_pairs.min(total_pairs)),
        u64::from(total_pairs),
    )
    .unwrap_or(ExactRatio::integer(0));

    let kernel_cid = kernel.cid().map_err(ProposalError::Mapping)?;
    let proposal = BindingProposal {
        mapping_kernel: kernel,
        proposed_envelope: MappingEnvelope {
            kernel: kernel_cid,
            generator: req.generator.clone(),
            derivation_rule: None,
            evidence: Vec::new(),
            source_event: None,
        },
        candidate_objects: vec![entry.object.clone()],
        index_commitment: None,
        model_commitment: None,
        rule_commitment: None,
        scores: vec![
            ScoreComponent {
                metric: req.metrics.structural_fit,
                value: structural_ratio,
                direction: ScoreDirection::HigherIsBetter,
            },
            ScoreComponent {
                metric: req.metrics.constraint_fit,
                value: constraint_ratio,
                direction: ScoreDirection::HigherIsBetter,
            },
        ],
        constraints: observations,
        expiry: ProposalExpiry {
            created_at_evaluation: req.created_at_evaluation,
            expires_after_evaluations: req.expires_after_evaluations,
            source_frontier: req.source_frontier,
        },
        privacy: DisclosureClass::LocalOnly,
    };
    proposal.validate()?;
    Ok(proposal)
}

fn derive_exposure_observation_id(
    frontier_digest: &[u8; 32],
    candidate_id: &[u8; 32],
    ordinal: u64,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:discovery-pipeline-exposure:1\0");
    hasher.update(frontier_digest);
    hasher.update(candidate_id);
    hasher.update(&ordinal.to_be_bytes());
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use ku_core::foundation::{
        DimensionVector, LiteralValue, PlacementId, QuantityLiteral, ReceptorAcceptanceProfile,
        ReceptorCardinality, ReceptorOrigin, StatementFrame, StatementId, StatementLocator,
        StatementQualifiers, TermRef, UnitRef, UnknownConstraintPolicy, RECEPTOR_DEFINITION_KIND,
    };

    use super::*;

    fn concept(byte: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([byte; 16])
    }

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn ku_frame(pred: u8, subj: u8, qty: i64, negated: bool) -> SemanticFrameSet {
        SemanticFrameSet {
            statements: vec![StatementFrame {
                statement_id: StatementId(0),
                operator_or_predicate: concept(pred),
                arguments: vec![
                    TermRef::Concept(concept(subj)),
                    TermRef::Literal(LiteralValue::Quantity(QuantityLiteral {
                        value: ExactRatio::integer(qty),
                        source_unit: UnitRef::coherent(concept(90), DimensionVector::LENGTH),
                    })),
                ],
                constraints: Vec::new(),
                qualifiers: StatementQualifiers {
                    negated,
                    ..StatementQualifiers::default()
                },
            }],
        }
    }

    #[test]
    fn five_stage_pipeline_recalls_cross_domain_runs_assembly_and_enforces_salience_and_quarantine()
    {
        let receptor_def = ReceptorDefinition {
            role: concept(10),
            expected_types: vec![concept(11)],
            hard_constraints: Vec::new(),
            cardinality: ReceptorCardinality::new(1, None).unwrap(),
            origin: ReceptorOrigin::Declared {
                source: StatementLocator {
                    object: reference(2),
                    statement_index: 0,
                },
            },
            acceptance: ReceptorAcceptanceProfile {
                policy: reference(3),
                required_evidence_kinds: Vec::new(),
                unknown_constraint_policy: UnknownConstraintPolicy::KeepUnresolved,
            },
        };
        let need = KnowledgeNeedIr {
            receptor_definitions: vec![ObjectReference::new(RECEPTOR_DEFINITION_KIND.0, [5; 32])],
            desired_roles: vec![concept(10)],
            goal: ku_frame(10, 11, 5, false),
            local_context: SemanticFrameSet {
                statements: Vec::new(),
            },
            privacy: DisclosureClass::LocalOnly,
        };

        // Two Exploit candidates (exact predicate 10, zero_use = false), one Cross-Domain
        // structural candidate (predicate 77, completely different CCIDs, identical FBS/Dimension!),
        // and one Cold Long-Tail candidate (zero_use = true).
        let exploit_low_sal = DiscoveryCorpusEntry {
            object: ObjectReference::new(SEMANTIC_KERNEL_OBJECT_KIND.0, [21; 32]),
            semantic: ku_frame(10, 11, 5, false),
            explicit_affordance: None,
            zero_use: false,
            evidence_domain: [1; 32],
            supporting_evidence_count: 2,
        };
        let exploit_high_sal = DiscoveryCorpusEntry {
            object: ObjectReference::new(SEMANTIC_KERNEL_OBJECT_KIND.0, [22; 32]),
            semantic: ku_frame(10, 11, 5, false),
            explicit_affordance: None,
            zero_use: false,
            evidence_domain: [2; 32],
            supporting_evidence_count: 2,
        };
        let cross_domain = DiscoveryCorpusEntry {
            object: ObjectReference::new(SEMANTIC_KERNEL_OBJECT_KIND.0, [23; 32]),
            semantic: ku_frame(77, 88, 5, false),
            explicit_affordance: None,
            zero_use: true,
            evidence_domain: [3; 32],
            supporting_evidence_count: 1,
        };
        let cold_tail = DiscoveryCorpusEntry {
            object: ObjectReference::new(SEMANTIC_KERNEL_OBJECT_KIND.0, [24; 32]),
            semantic: ku_frame(10, 11, 5, false),
            explicit_affordance: None,
            zero_use: true,
            evidence_domain: [4; 32],
            supporting_evidence_count: 0,
        };

        let mut salience = LocalSalienceTieBreaker::new();
        // Give [22; 32] higher salience than [21; 32]; explore candidates have 0 salience.
        salience.set_salience_milli([21; 32], 100);
        salience.set_salience_milli([22; 32], 9_000);
        assert!(salience.is_local_only());
        assert!(!salience.can_evict_validated_store());
        assert!(!salience.can_override_exploration_floor());

        let policy = ExplorationPolicyV1::standard();
        let mut exp_state =
            ExplorationState::new([30; 32], policy.policy_cid().unwrap(), [31; 32], [32; 32])
                .unwrap();
        let mut learner = PropensityAwareLearner::new(128).unwrap();
        let mut quarantine = ProposalQuarantine::default();
        let mut reunion = ReunionFrontier::default();

        let placements = [
            PlacementRequirement {
                placement_id: PlacementId::from_bytes([41; 32]),
                required: true,
                weight: 10,
            },
            PlacementRequirement {
                placement_id: PlacementId::from_bytes([42; 32]),
                required: true,
                weight: 10,
            },
        ];

        let corpus = [exploit_low_sal, exploit_high_sal, cross_domain, cold_tail];
        let report = DiscoveryPipelineOrchestrator::run(
            DiscoveryPipelineRequest {
                need: &need,
                receptor: &receptor_def,
                local_reunion_targets: &[],
                remote_affordance_deltas: Vec::new(),
                reunion_budget: None,
                corpus: &corpus,
                lifecycle_table: None,
                recall_priors: &[],
                placements: &placements,
                pair_compatibilities: &[],
                alignment_policy: RelationalAlignmentPolicy::bounded_default(),
                assembly_policy: AssemblySearchPolicy::bounded_default(),
                exploration_policy: policy,
                discovery_profile: DiscoveryProfile::OpenScientificHighUncertainty,
                presentation_slots: 4,
                salience_tie_breaker: &salience,
                generator: reference(50),
                assumption_predicate: concept(51),
                unmapped_source_reason: concept(52),
                unmapped_target_reason: concept(53),
                metrics: MatcherMetricConcepts {
                    structural_fit: concept(60),
                    constraint_fit: concept(61),
                },
                source_frontier: EventCid::from_bytes([70; 32]),
                frontier_digest: [31; 32],
                private_context_commitment: [71; 32],
                created_at_evaluation: 1,
                expires_after_evaluations: 10,
            },
            &mut reunion,
            &mut exp_state,
            &mut learner,
            &mut quarantine,
        )
        .unwrap();

        // Cross-domain candidate [23; 32] was NOT in D1 exact postings, WAS recalled in D2!
        assert!(!report
            .d1_exact_recalled_cids
            .contains(&ObjectCid::from_bytes([23; 32])));
        assert!(report
            .d2_structural_recalled_cids
            .contains(&ObjectCid::from_bytes([23; 32])));
        assert_eq!(report.d3_actionable_alignments, 4);
        assert!(!report.d4_assembly_portfolio.is_empty());
        assert_eq!(report.d5_logged_exposures, 4);
        assert_eq!(report.quarantined_proposal_ids.len(), 4);
        assert_eq!(quarantine.len(), 4);
        assert!(!quarantine.is_executable());
        assert!(!report.emits_use_evidence());

        // Verify salience tie-breaker ordered [22; 32] ahead of [21; 32] among Exploit slots
        // without suppressing Explore cohorts!
        let exploit_order: Vec<[u8; 32]> = report
            .d5_selected_candidates
            .iter()
            .filter_map(|(id, lane)| (*lane == SelectedLane::Exploit).then_some(*id))
            .collect();
        assert_eq!(exploit_order, vec![[22; 32], [21; 32]]);
        assert!(report
            .d5_selected_candidates
            .iter()
            .any(|(_, lane)| matches!(lane, SelectedLane::Explore(_))));
    }
}
