//! Budgeted multi-channel complement planner with resumable partial output.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};

use ku_core::foundation::{ObjectReference, SelectorCid};

use crate::vnext_proposal::{BindingProposal, ProposalDisposition, ProposalError, ProposalId};
use crate::vnext_query::{QueryChannel, QueryContractError, QueryRun, QueryWorkItem};

pub const MAX_PLANNER_CHANNELS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlannerBudget {
    pub max_candidates: u64,
    pub max_validations: u64,
    pub max_proposals: u64,
    pub max_work_units: u64,
}

impl PlannerBudget {
    pub fn validate(self) -> Result<(), PlannerError> {
        if self.max_candidates == 0
            || self.max_validations == 0
            || self.max_proposals == 0
            || self.max_work_units == 0
        {
            Err(PlannerError::InvalidBudget)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlannerUsage {
    pub generated_candidates: u64,
    pub validations: u64,
    pub accepted_proposals: u64,
    pub work_units: u64,
}

impl PlannerUsage {
    fn can_generate(self, budget: PlannerBudget, count: u64, work: u64) -> bool {
        self.generated_candidates.saturating_add(count) <= budget.max_candidates
            && self.work_units.saturating_add(work) <= budget.max_work_units
    }

    fn can_validate(self, budget: PlannerBudget) -> bool {
        self.validations < budget.max_validations
    }

    fn can_accept(self, budget: PlannerBudget) -> bool {
        self.accepted_proposals < budget.max_proposals
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateSeed {
    pub candidate_id: [u8; 32],
    pub candidate_objects: Vec<ObjectReference>,
    pub channel: QueryChannel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidatePage {
    pub candidates: Vec<CandidateSeed>,
    pub consumed_work_units: u64,
    pub continuation: Option<[u8; 32]>,
}

#[derive(Clone, Copy, Debug)]
pub struct CandidateRequest {
    pub run_id: [u8; 32],
    pub work_id: [u8; 32],
    pub boundary: SelectorCid,
    pub remaining_candidates: u64,
    pub remaining_work_units: u64,
    pub continuation: Option<[u8; 32]>,
}

pub trait CandidateGenerator {
    fn channel(&self) -> QueryChannel;
    fn generate(&mut self, request: CandidateRequest) -> Result<CandidatePage, PlannerError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CandidateValidation {
    Proposal(Box<BindingProposal>),
    Rejected { reason: &'static str },
    Deferred { reason: &'static str },
}

pub trait ProposalValidator {
    fn validate(&mut self, candidate: &CandidateSeed) -> Result<CandidateValidation, PlannerError>;
}

#[derive(Default)]
pub struct CancellationToken {
    cancelled: AtomicBool,
}

impl CancellationToken {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlannerContinuation {
    pub channel_tokens: BTreeMap<QueryChannel, [u8; 32]>,
    pub deferred_candidates: Vec<[u8; 32]>,
}

#[derive(Clone, Debug, Default)]
pub struct ProposalPortfolio {
    proposals: BTreeMap<[u8; 32], BindingProposal>,
}

impl ProposalPortfolio {
    pub fn insert(&mut self, proposal: BindingProposal) -> Result<bool, PlannerError> {
        proposal.validate()?;
        let id = proposal.proposal_id()?;
        Ok(self.proposals.insert(*id.as_bytes(), proposal).is_none())
    }

    pub fn get(&self, id: ProposalId) -> Option<&BindingProposal> {
        self.proposals.get(id.as_bytes())
    }

    pub fn proposals(&self) -> impl Iterator<Item = &BindingProposal> {
        self.proposals.values()
    }

    pub fn len(&self) -> usize {
        self.proposals.len()
    }

    pub fn is_empty(&self) -> bool {
        self.proposals.is_empty()
    }

    pub fn dispositions(&self, current_evaluation: u64) -> Vec<ProposalDisposition> {
        self.proposals
            .values()
            .map(|proposal| proposal.disposition(current_evaluation))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlannerOutcome {
    CompleteForCurrentChannelPages,
    PartialBudget,
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct PlannerResult {
    pub portfolio: ProposalPortfolio,
    pub rejected_candidates: Vec<([u8; 32], &'static str)>,
    pub continuation: PlannerContinuation,
    pub usage: PlannerUsage,
    pub examined_channels: Vec<QueryChannel>,
    pub outcome: PlannerOutcome,
}

pub struct ComplementPlanner {
    budget: PlannerBudget,
}

impl ComplementPlanner {
    pub fn new(budget: PlannerBudget) -> Result<Self, PlannerError> {
        budget.validate()?;
        Ok(Self { budget })
    }

    pub fn run(
        &self,
        run: &QueryRun,
        work: &QueryWorkItem,
        generators: &mut [&mut dyn CandidateGenerator],
        validator: &mut dyn ProposalValidator,
        prior: PlannerContinuation,
        cancellation: &CancellationToken,
    ) -> Result<PlannerResult, PlannerError> {
        work.validate_for(run)?;
        if generators.len() > MAX_PLANNER_CHANNELS {
            return Err(PlannerError::TooManyChannels);
        }
        let mut seen_channels = BTreeSet::new();
        for generator in generators.iter() {
            if !seen_channels.insert(generator.channel()) {
                return Err(PlannerError::DuplicateChannel);
            }
        }
        let channels = seen_channels.iter().copied().collect::<Vec<_>>();

        let mut result = PlannerResult {
            portfolio: ProposalPortfolio::default(),
            rejected_candidates: Vec::new(),
            continuation: PlannerContinuation::default(),
            usage: PlannerUsage::default(),
            examined_channels: Vec::new(),
            outcome: PlannerOutcome::CompleteForCurrentChannelPages,
        };
        result.continuation.deferred_candidates = prior.deferred_candidates.clone();

        for generator in generators.iter_mut() {
            if cancellation.is_cancelled() {
                result.outcome = PlannerOutcome::Cancelled;
                copy_remaining_tokens(&mut result.continuation, &prior, &channels);
                return Ok(result);
            }
            if result.usage.generated_candidates >= self.budget.max_candidates
                || result.usage.work_units >= self.budget.max_work_units
                || result.usage.validations >= self.budget.max_validations
                || result.usage.accepted_proposals >= self.budget.max_proposals
            {
                result.outcome = PlannerOutcome::PartialBudget;
                copy_remaining_tokens(&mut result.continuation, &prior, &channels);
                return Ok(result);
            }
            let channel = generator.channel();
            result.examined_channels.push(channel);
            let request = CandidateRequest {
                run_id: *run.run_id(),
                work_id: work.work_id,
                boundary: work.boundary,
                remaining_candidates: self
                    .budget
                    .max_candidates
                    .saturating_sub(result.usage.generated_candidates),
                remaining_work_units: self
                    .budget
                    .max_work_units
                    .saturating_sub(result.usage.work_units),
                continuation: prior.channel_tokens.get(&channel).copied(),
            };
            let page = generator.generate(request)?;
            let candidate_count = page.candidates.len() as u64;
            if !result
                .usage
                .can_generate(self.budget, candidate_count, page.consumed_work_units)
                || page
                    .candidates
                    .iter()
                    .any(|candidate| candidate.channel != channel)
            {
                return Err(PlannerError::GeneratorBudgetViolation);
            }
            result.usage.generated_candidates += candidate_count;
            result.usage.work_units += page.consumed_work_units;
            if let Some(token) = page.continuation {
                result.continuation.channel_tokens.insert(channel, token);
            }

            for candidate in page.candidates {
                if cancellation.is_cancelled() {
                    result
                        .continuation
                        .deferred_candidates
                        .push(candidate.candidate_id);
                    result.outcome = PlannerOutcome::Cancelled;
                    copy_remaining_tokens(&mut result.continuation, &prior, &channels);
                    return Ok(result);
                }
                if !result.usage.can_validate(self.budget) || !result.usage.can_accept(self.budget)
                {
                    result
                        .continuation
                        .deferred_candidates
                        .push(candidate.candidate_id);
                    result.outcome = PlannerOutcome::PartialBudget;
                    copy_remaining_tokens(&mut result.continuation, &prior, &channels);
                    return Ok(result);
                }
                result.usage.validations += 1;
                match validator.validate(&candidate)? {
                    CandidateValidation::Proposal(proposal) => {
                        if result.portfolio.insert(*proposal)? {
                            result.usage.accepted_proposals += 1;
                        }
                    }
                    CandidateValidation::Rejected { reason } => result
                        .rejected_candidates
                        .push((candidate.candidate_id, reason)),
                    CandidateValidation::Deferred { .. } => result
                        .continuation
                        .deferred_candidates
                        .push(candidate.candidate_id),
                }
            }
        }
        Ok(result)
    }
}

fn copy_remaining_tokens(
    output: &mut PlannerContinuation,
    prior: &PlannerContinuation,
    channels: &[QueryChannel],
) {
    for channel in channels {
        if let Some(token) = prior.channel_tokens.get(channel) {
            output.channel_tokens.entry(*channel).or_insert(*token);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlannerError {
    Query(QueryContractError),
    Proposal(ProposalError),
    InvalidBudget,
    TooManyChannels,
    DuplicateChannel,
    GeneratorBudgetViolation,
    Generator(&'static str),
    Validator(&'static str),
}

impl From<QueryContractError> for PlannerError {
    fn from(error: QueryContractError) -> Self {
        Self::Query(error)
    }
}

impl From<ProposalError> for PlannerError {
    fn from(error: ProposalError) -> Self {
        Self::Proposal(error)
    }
}

#[cfg(test)]
mod tests {
    use ku_core::foundation::{
        public_knowledge_exchange_fixture_v1, ConceptCcid, CorrespondenceKind, DisclosureClass,
        EventCid, ExactRatio, MappingEnvelope, MappingKernel, MappingSide, MappingTermLocator,
        MappingTransform, SemanticFrameSet, TermCorrespondence, UnmappedRegion,
    };

    use crate::vnext_proposal::{
        ConstraintObservation, ProposalExpiry, ScoreComponent, ScoreDirection,
    };
    use crate::vnext_query::{KnowledgeNeedIr, QueryDefinition};

    use super::*;

    fn concept(byte: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([byte; 16])
    }

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn run_and_work() -> (QueryRun, QueryWorkItem) {
        let definition = QueryDefinition {
            need: KnowledgeNeedIr {
                receptor_definitions: vec![reference(1)],
                desired_roles: vec![concept(2)],
                goal: SemanticFrameSet {
                    statements: Vec::new(),
                },
                local_context: SemanticFrameSet {
                    statements: Vec::new(),
                },
                privacy: DisclosureClass::LocalOnly,
            },
            query_policy: reference(3),
            exploration_policy: reference(4),
        };
        let run = QueryRun::new(
            [5; 32],
            definition.private_cid().unwrap(),
            public_knowledge_exchange_fixture_v1(),
        )
        .unwrap();
        let work = QueryWorkItem {
            work_id: [6; 32],
            run_id: *run.run_id(),
            channel: QueryChannel::Structural,
            boundary: run.selector_cid().unwrap(),
            budget: ku_core::foundation::Budget::new(100, 1_000_000, 100, 8).unwrap(),
            continuation: None,
        };
        (run, work)
    }

    struct FixedGenerator {
        channel: QueryChannel,
        candidates: Vec<CandidateSeed>,
        continuation: Option<[u8; 32]>,
    }

    impl CandidateGenerator for FixedGenerator {
        fn channel(&self) -> QueryChannel {
            self.channel
        }

        fn generate(&mut self, _request: CandidateRequest) -> Result<CandidatePage, PlannerError> {
            Ok(CandidatePage {
                candidates: self.candidates.clone(),
                consumed_work_units: 1,
                continuation: self.continuation,
            })
        }
    }

    struct ProposalValidatorStub;

    impl ProposalValidator for ProposalValidatorStub {
        fn validate(
            &mut self,
            candidate: &CandidateSeed,
        ) -> Result<CandidateValidation, PlannerError> {
            Ok(CandidateValidation::Proposal(Box::new(proposal(
                candidate.candidate_id[0],
            ))))
        }
    }

    fn seed(byte: u8, channel: QueryChannel) -> CandidateSeed {
        CandidateSeed {
            candidate_id: [byte; 32],
            candidate_objects: vec![reference(byte)],
            channel,
        }
    }

    fn proposal(byte: u8) -> BindingProposal {
        let source = MappingTermLocator {
            object: reference(byte),
            statement_index: 0,
            argument_index: Some(0),
        };
        let target = MappingTermLocator {
            object: reference(byte + 1),
            statement_index: 0,
            argument_index: Some(1),
        };
        let kernel = MappingKernel {
            source_objects: vec![source.object.clone()],
            target_objects: vec![target.object.clone()],
            correspondences: vec![TermCorrespondence {
                source: source.clone(),
                target,
                kind: CorrespondenceKind::Analogous,
                transform: MappingTransform::Identity,
            }],
            assumptions: SemanticFrameSet {
                statements: Vec::new(),
            },
            constraint_regions: Vec::new(),
            unmapped_regions: vec![UnmappedRegion {
                side: MappingSide::Source,
                locator: source,
                reason: concept(9),
            }],
        };
        let kernel_id = kernel.cid().unwrap();
        BindingProposal {
            mapping_kernel: kernel,
            proposed_envelope: MappingEnvelope {
                kernel: kernel_id,
                generator: reference(10),
                derivation_rule: None,
                evidence: vec![reference(11)],
                source_event: None,
            },
            candidate_objects: vec![reference(byte)],
            index_commitment: None,
            model_commitment: None,
            rule_commitment: None,
            scores: vec![
                ScoreComponent {
                    metric: concept(12),
                    value: ExactRatio::new(byte as i64, 10).unwrap(),
                    direction: ScoreDirection::HigherIsBetter,
                },
                ScoreComponent {
                    metric: concept(13),
                    value: ExactRatio::new(10 - byte as i64, 10).unwrap(),
                    direction: ScoreDirection::LowerIsBetter,
                },
            ],
            constraints: Vec::<ConstraintObservation>::new(),
            expiry: ProposalExpiry {
                created_at_evaluation: 0,
                expires_after_evaluations: 10,
                source_frontier: EventCid::from_bytes([14; 32]),
            },
            privacy: DisclosureClass::LocalOnly,
        }
    }

    fn budget(candidates: u64, validations: u64, proposals: u64) -> PlannerBudget {
        PlannerBudget {
            max_candidates: candidates,
            max_validations: validations,
            max_proposals: proposals,
            max_work_units: 100,
        }
    }

    #[test]
    fn zero_from_one_channel_does_not_exclude_later_channels() {
        let (run, work) = run_and_work();
        let mut empty = FixedGenerator {
            channel: QueryChannel::ExactTypedIndex,
            candidates: Vec::new(),
            continuation: None,
        };
        let mut structural = FixedGenerator {
            channel: QueryChannel::Structural,
            candidates: vec![seed(1, QueryChannel::Structural)],
            continuation: None,
        };
        let mut generators: Vec<&mut dyn CandidateGenerator> = vec![&mut empty, &mut structural];
        let result = ComplementPlanner::new(budget(10, 10, 10))
            .unwrap()
            .run(
                &run,
                &work,
                &mut generators,
                &mut ProposalValidatorStub,
                PlannerContinuation::default(),
                &CancellationToken::default(),
            )
            .unwrap();
        assert_eq!(
            result.examined_channels,
            vec![QueryChannel::ExactTypedIndex, QueryChannel::Structural]
        );
        assert_eq!(result.portfolio.len(), 1);
    }

    #[test]
    fn exhausted_validation_budget_returns_partial_and_deferred_candidate() {
        let (run, work) = run_and_work();
        let mut generator = FixedGenerator {
            channel: QueryChannel::Structural,
            candidates: vec![
                seed(1, QueryChannel::Structural),
                seed(2, QueryChannel::Structural),
            ],
            continuation: Some([20; 32]),
        };
        let mut generators: Vec<&mut dyn CandidateGenerator> = vec![&mut generator];
        let result = ComplementPlanner::new(budget(10, 1, 10))
            .unwrap()
            .run(
                &run,
                &work,
                &mut generators,
                &mut ProposalValidatorStub,
                PlannerContinuation::default(),
                &CancellationToken::default(),
            )
            .unwrap();
        assert_eq!(result.outcome, PlannerOutcome::PartialBudget);
        assert_eq!(result.portfolio.len(), 1);
        assert_eq!(result.continuation.deferred_candidates, vec![[2; 32]]);
        assert_eq!(
            result
                .continuation
                .channel_tokens
                .get(&QueryChannel::Structural),
            Some(&[20; 32])
        );
    }

    #[test]
    fn cancellation_returns_partial_without_erasing_prior_continuations() {
        let (run, work) = run_and_work();
        let mut generator = FixedGenerator {
            channel: QueryChannel::LongTail,
            candidates: vec![seed(1, QueryChannel::LongTail)],
            continuation: None,
        };
        let mut generators: Vec<&mut dyn CandidateGenerator> = vec![&mut generator];
        let cancellation = CancellationToken::default();
        cancellation.cancel();
        let mut prior = PlannerContinuation::default();
        prior
            .channel_tokens
            .insert(QueryChannel::LongTail, [30; 32]);
        let result = ComplementPlanner::new(budget(10, 10, 10))
            .unwrap()
            .run(
                &run,
                &work,
                &mut generators,
                &mut ProposalValidatorStub,
                prior,
                &cancellation,
            )
            .unwrap();
        assert_eq!(result.outcome, PlannerOutcome::Cancelled);
        assert_eq!(
            result
                .continuation
                .channel_tokens
                .get(&QueryChannel::LongTail),
            Some(&[30; 32])
        );
    }

    #[test]
    fn portfolio_preserves_multiple_vector_scored_proposals_without_scalar_winner() {
        let mut portfolio = ProposalPortfolio::default();
        portfolio.insert(proposal(1)).unwrap();
        portfolio.insert(proposal(2)).unwrap();
        assert_eq!(portfolio.len(), 2);
        assert!(portfolio
            .proposals()
            .all(|proposal| proposal.scores.len() == 2));
        assert_eq!(
            portfolio.dispositions(1),
            vec![
                ProposalDisposition::CandidateOnly,
                ProposalDisposition::CandidateOnly
            ]
        );
    }
}
