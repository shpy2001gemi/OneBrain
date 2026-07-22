//! Bounded multi-fragment Assembly search over a weighted, three-state CSP.
//!
//! Beam ordering is only a local scheduling policy. Returned candidates retain
//! a Pareto objective vector and never authorize Mapping materialization,
//! Assembly adoption, or a truth judgment.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use ku_core::foundation::{ConstraintEvaluation, ObjectReference, PlacementId};

pub const MAX_ASSEMBLY_SEARCH_INPUTS: usize = 4_096;
pub const MAX_ASSEMBLY_SEARCH_PLACEMENTS: usize = 1_024;
pub const MAX_ASSEMBLY_SEARCH_COMPATIBILITIES: usize = 65_536;
pub const MAX_ASSEMBLY_SEARCH_EVIDENCE_DOMAINS: usize = 256;
pub const MAX_ASSEMBLY_SEARCH_SIZE: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssemblySearchPolicy {
    pub min_size: usize,
    pub max_size: usize,
    pub beam_width: usize,
    pub max_expansions_per_page: usize,
    pub max_portfolio_per_page: usize,
}

impl AssemblySearchPolicy {
    pub const fn bounded_default() -> Self {
        Self {
            min_size: 2,
            max_size: 4,
            beam_width: 128,
            max_expansions_per_page: 4_096,
            max_portfolio_per_page: 64,
        }
    }

    fn validate(self) -> Result<Self, AssemblySearchError> {
        if self.min_size < 2
            || self.min_size > self.max_size
            || self.max_size > MAX_ASSEMBLY_SEARCH_SIZE
            || self.beam_width < self.min_size
            || self.beam_width > MAX_ASSEMBLY_SEARCH_INPUTS
            || self.max_expansions_per_page == 0
            || self.max_portfolio_per_page == 0
        {
            Err(AssemblySearchError::InvalidPolicy)
        } else {
            Ok(self)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlacementRequirement {
    pub placement_id: PlacementId,
    pub required: bool,
    /// Local policy weight used for beam scheduling and retained objectives.
    pub weight: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlacementFit {
    pub placement_id: PlacementId,
    pub evaluation: ConstraintEvaluation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssemblyFragmentCandidate {
    /// Stable local commitment to this candidate/proposal input.
    pub candidate_id: [u8; 32],
    pub object: ObjectReference,
    /// Optional exact commitment to the KQL-004/KQL-008 mapping proposal.
    pub mapping_proposal_commitment: Option<[u8; 32]>,
    pub placement_fits: Vec<PlacementFit>,
    pub systematic_connections: u32,
    pub supporting_evidence_count: u32,
    pub evidence_domains: Vec<[u8; 32]>,
    /// Proposal-level hard violations. Any non-zero value excludes the input.
    pub hard_violation_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PairCompatibility {
    pub left_candidate: [u8; 32],
    pub right_candidate: [u8; 32],
    pub evaluation: ConstraintEvaluation,
    /// A violated required relation is a hard CSP violation.
    pub required: bool,
}

pub struct AssemblySearchRequest<'a> {
    pub placements: &'a [PlacementRequirement],
    pub candidates: &'a [AssemblyFragmentCandidate],
    pub pair_compatibilities: &'a [PairCompatibility],
    pub policy: AssemblySearchPolicy,
    pub continuation: Option<AssemblySearchCursor>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlacementCoverageState {
    Satisfied,
    Unknown,
    Unmet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlacementCoverage {
    pub placement_id: PlacementId,
    pub state: PlacementCoverageState,
    pub required: bool,
    pub weight: u32,
}

/// Multi-objective evidence retained without a scalar aggregate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AssemblyObjectiveVector {
    pub required_satisfied_weight: u64,
    pub optional_satisfied_weight: u64,
    pub required_unknown_weight: u64,
    pub required_unmet_weight: u64,
    pub compatibility_unknown_count: u32,
    pub soft_conflict_count: u32,
    pub systematic_connections: u64,
    pub supporting_evidence_count: u64,
    pub evidence_domain_count: u32,
    pub candidate_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssemblyCandidateDisposition {
    /// Hard-safe and fully specified enough to enter exact validation.
    ReadyForExactValidation,
    /// Hard-safe, but missing/unknown evidence remains explicit.
    PartialOrUnknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssemblyCompositionCandidate {
    pub member_ids: Vec<[u8; 32]>,
    pub member_objects: Vec<ObjectReference>,
    pub mapping_proposal_commitments: Vec<[u8; 32]>,
    pub placement_coverage: Vec<PlacementCoverage>,
    pub objectives: AssemblyObjectiveVector,
    pub disposition: AssemblyCandidateDisposition,
}

impl AssemblyCompositionCandidate {
    pub fn eligible_for_exact_validation(&self) -> bool {
        self.disposition == AssemblyCandidateDisposition::ReadyForExactValidation
    }

    pub const fn is_materialization_authority(&self) -> bool {
        false
    }

    pub const fn is_adoption_authority(&self) -> bool {
        false
    }
}

/// Cursor names the exact next lexicographic combination inside the committed
/// beam candidate pool. Changing semantic inputs invalidates it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssemblySearchCursor {
    pub context_root: [u8; 32],
    pub next_size: u8,
    pub next_indices: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssemblySearchCoverage {
    /// Every size 2--4 combination in the selected beam was evaluated.
    ExhaustedSelectedBeam,
    /// More combinations in the same selected beam remain.
    PartialWithContinuation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssemblySearchPage {
    pub portfolio: Vec<AssemblyCompositionCandidate>,
    pub continuation: Option<AssemblySearchCursor>,
    pub coverage: AssemblySearchCoverage,
    pub context_root: [u8; 32],
    pub evaluated_combinations: usize,
    pub hard_blocked_combinations: usize,
    pub hard_blocked_candidates: usize,
    pub irrelevant_candidates: usize,
    pub beam_pruned_candidates: usize,
}

impl AssemblySearchPage {
    pub const fn claims_global_completeness(&self) -> bool {
        false
    }
}

/// Incremental Pareto merger for pages from the same search context.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssemblyParetoPortfolio {
    context_root: Option<[u8; 32]>,
    candidates: Vec<AssemblyCompositionCandidate>,
}

impl AssemblyParetoPortfolio {
    pub fn merge_page(&mut self, page: &AssemblySearchPage) -> Result<(), AssemblySearchError> {
        match self.context_root {
            Some(root) if root != page.context_root => {
                return Err(AssemblySearchError::ContinuationContextMismatch)
            }
            None => self.context_root = Some(page.context_root),
            _ => {}
        }
        for candidate in page.portfolio.iter().cloned() {
            pareto_insert(&mut self.candidates, candidate);
        }
        sort_portfolio(&mut self.candidates);
        Ok(())
    }

    pub fn candidates(&self) -> &[AssemblyCompositionCandidate] {
        &self.candidates
    }
}

pub struct AssemblySearcher;

impl AssemblySearcher {
    pub fn search(
        request: AssemblySearchRequest<'_>,
    ) -> Result<AssemblySearchPage, AssemblySearchError> {
        let prepared = PreparedSearch::new(request)?;
        let context_root = prepared.context_root;
        let pool_len = prepared.pool.len();
        let mut next = match prepared.continuation.as_ref() {
            Some(cursor) => {
                validate_cursor(cursor, context_root, pool_len, prepared.policy)?;
                Some(
                    cursor
                        .next_indices
                        .iter()
                        .map(|index| *index as usize)
                        .collect::<Vec<_>>(),
                )
            }
            None => first_combination(pool_len, prepared.policy),
        };
        let mut portfolio = Vec::new();
        let mut evaluated = 0usize;
        let mut hard_blocked = 0usize;

        while let Some(indices) = next.clone() {
            evaluated += 1;
            match prepared.evaluate(&indices) {
                CombinationEvaluation::HardBlocked => hard_blocked += 1,
                CombinationEvaluation::Candidate(candidate) => {
                    pareto_insert(&mut portfolio, candidate);
                }
            }
            next = next_combination(&indices, pool_len, prepared.policy.max_size);
            if evaluated >= prepared.policy.max_expansions_per_page
                || portfolio.len() >= prepared.policy.max_portfolio_per_page
            {
                break;
            }
        }

        sort_portfolio(&mut portfolio);
        let continuation = next.map(|indices| AssemblySearchCursor {
            context_root,
            next_size: indices.len() as u8,
            next_indices: indices.into_iter().map(|index| index as u32).collect(),
        });
        let coverage = if continuation.is_some() {
            AssemblySearchCoverage::PartialWithContinuation
        } else {
            AssemblySearchCoverage::ExhaustedSelectedBeam
        };

        Ok(AssemblySearchPage {
            portfolio,
            continuation,
            coverage,
            context_root,
            evaluated_combinations: evaluated,
            hard_blocked_combinations: hard_blocked,
            hard_blocked_candidates: prepared.hard_blocked_candidates,
            irrelevant_candidates: prepared.irrelevant_candidates,
            beam_pruned_candidates: prepared.beam_pruned_candidates,
        })
    }
}

#[derive(Clone)]
struct PreparedSearch {
    placements: Vec<PlacementRequirement>,
    pool: Vec<AssemblyFragmentCandidate>,
    compatibilities: BTreeMap<([u8; 32], [u8; 32]), PairCompatibility>,
    policy: AssemblySearchPolicy,
    continuation: Option<AssemblySearchCursor>,
    context_root: [u8; 32],
    hard_blocked_candidates: usize,
    irrelevant_candidates: usize,
    beam_pruned_candidates: usize,
}

impl PreparedSearch {
    fn new(request: AssemblySearchRequest<'_>) -> Result<Self, AssemblySearchError> {
        let policy = request.policy.validate()?;
        validate_limits(&request)?;

        let mut placements = request.placements.to_vec();
        placements.sort_by_key(|placement| *placement.placement_id.as_bytes());
        let placement_ids = placements
            .iter()
            .map(|placement| placement.placement_id)
            .collect::<BTreeSet<_>>();
        if placement_ids.len() != placements.len() {
            return Err(AssemblySearchError::DuplicatePlacement);
        }
        if placements.iter().any(|placement| placement.weight == 0) {
            return Err(AssemblySearchError::ZeroPlacementWeight);
        }

        let mut candidates = request.candidates.to_vec();
        candidates.sort_by_key(|candidate| candidate.candidate_id);
        let candidate_ids = candidates
            .iter()
            .map(|candidate| candidate.candidate_id)
            .collect::<BTreeSet<_>>();
        if candidate_ids.len() != candidates.len() {
            return Err(AssemblySearchError::DuplicateCandidate);
        }
        for candidate in &mut candidates {
            validate_candidate(candidate, &placement_ids)?;
            candidate
                .placement_fits
                .sort_by_key(|fit| *fit.placement_id.as_bytes());
            candidate.evidence_domains.sort_unstable();
        }

        let mut compatibilities = BTreeMap::new();
        for compatibility in request.pair_compatibilities {
            if compatibility.left_candidate == compatibility.right_candidate {
                return Err(AssemblySearchError::SelfCompatibility);
            }
            if !candidate_ids.contains(&compatibility.left_candidate)
                || !candidate_ids.contains(&compatibility.right_candidate)
            {
                return Err(AssemblySearchError::UnknownCompatibilityCandidate);
            }
            let key = ordered_pair(compatibility.left_candidate, compatibility.right_candidate);
            if compatibilities.insert(key, *compatibility).is_some() {
                return Err(AssemblySearchError::DuplicateCompatibility);
            }
        }

        let context_root = context_root(&placements, &candidates, &compatibilities, policy);
        let hard_blocked_candidates = candidates
            .iter()
            .filter(|candidate| candidate.hard_violation_count > 0)
            .count();
        let irrelevant_candidates = candidates
            .iter()
            .filter(|candidate| {
                candidate.hard_violation_count == 0
                    && !candidate.placement_fits.iter().any(|fit| {
                        matches!(
                            fit.evaluation,
                            ConstraintEvaluation::Satisfied | ConstraintEvaluation::Unknown
                        )
                    })
            })
            .count();
        candidates.retain(|candidate| {
            candidate.hard_violation_count == 0
                && candidate.placement_fits.iter().any(|fit| {
                    matches!(
                        fit.evaluation,
                        ConstraintEvaluation::Satisfied | ConstraintEvaluation::Unknown
                    )
                })
        });
        candidates.sort_by(|left, right| beam_candidate_order(left, right, &placements));
        let beam_pruned_candidates = candidates.len().saturating_sub(policy.beam_width);
        candidates.truncate(policy.beam_width);

        Ok(Self {
            placements,
            pool: candidates,
            compatibilities,
            policy,
            continuation: request.continuation,
            context_root,
            hard_blocked_candidates,
            irrelevant_candidates,
            beam_pruned_candidates,
        })
    }

    fn evaluate(&self, indices: &[usize]) -> CombinationEvaluation {
        let members = indices
            .iter()
            .map(|index| &self.pool[*index])
            .collect::<Vec<_>>();
        let mut compatibility_unknown_count = 0u32;
        let mut soft_conflict_count = 0u32;
        for left in 0..members.len() {
            for right in (left + 1)..members.len() {
                let key = ordered_pair(members[left].candidate_id, members[right].candidate_id);
                match self.compatibilities.get(&key) {
                    Some(compatibility)
                        if compatibility.required
                            && compatibility.evaluation == ConstraintEvaluation::Violated =>
                    {
                        return CombinationEvaluation::HardBlocked
                    }
                    Some(compatibility)
                        if compatibility.evaluation == ConstraintEvaluation::Unknown =>
                    {
                        compatibility_unknown_count += 1;
                    }
                    Some(compatibility)
                        if !compatibility.required
                            && compatibility.evaluation == ConstraintEvaluation::Violated =>
                    {
                        soft_conflict_count += 1;
                    }
                    None => compatibility_unknown_count += 1,
                    _ => {}
                }
            }
        }

        let mut coverage = Vec::with_capacity(self.placements.len());
        let mut objectives = AssemblyObjectiveVector {
            compatibility_unknown_count,
            soft_conflict_count,
            candidate_count: members.len() as u32,
            ..AssemblyObjectiveVector::default()
        };
        for placement in &self.placements {
            let mut saw_satisfied = false;
            let mut saw_unknown = false;
            for candidate in &members {
                if let Some(fit) = candidate
                    .placement_fits
                    .iter()
                    .find(|fit| fit.placement_id == placement.placement_id)
                {
                    saw_satisfied |= fit.evaluation == ConstraintEvaluation::Satisfied;
                    saw_unknown |= fit.evaluation == ConstraintEvaluation::Unknown;
                }
            }
            let state = if saw_satisfied {
                PlacementCoverageState::Satisfied
            } else if saw_unknown {
                PlacementCoverageState::Unknown
            } else {
                PlacementCoverageState::Unmet
            };
            let weight = u64::from(placement.weight);
            match (placement.required, state) {
                (true, PlacementCoverageState::Satisfied) => {
                    objectives.required_satisfied_weight += weight
                }
                (false, PlacementCoverageState::Satisfied) => {
                    objectives.optional_satisfied_weight += weight
                }
                (true, PlacementCoverageState::Unknown) => {
                    objectives.required_unknown_weight += weight
                }
                (true, PlacementCoverageState::Unmet) => objectives.required_unmet_weight += weight,
                (false, _) => {}
            }
            coverage.push(PlacementCoverage {
                placement_id: placement.placement_id,
                state,
                required: placement.required,
                weight: placement.weight,
            });
        }

        let mut evidence_domains = BTreeSet::new();
        let mut member_ids = Vec::with_capacity(members.len());
        let mut member_objects = Vec::with_capacity(members.len());
        let mut commitments = Vec::new();
        for member in members {
            member_ids.push(member.candidate_id);
            member_objects.push(member.object.clone());
            if let Some(commitment) = member.mapping_proposal_commitment {
                commitments.push(commitment);
            }
            objectives.systematic_connections += u64::from(member.systematic_connections);
            objectives.supporting_evidence_count += u64::from(member.supporting_evidence_count);
            evidence_domains.extend(member.evidence_domains.iter().copied());
        }
        commitments.sort_unstable();
        commitments.dedup();
        objectives.evidence_domain_count = evidence_domains.len() as u32;
        let disposition = if objectives.required_unknown_weight == 0
            && objectives.required_unmet_weight == 0
            && objectives.compatibility_unknown_count == 0
            && objectives.soft_conflict_count == 0
        {
            AssemblyCandidateDisposition::ReadyForExactValidation
        } else {
            AssemblyCandidateDisposition::PartialOrUnknown
        };

        CombinationEvaluation::Candidate(AssemblyCompositionCandidate {
            member_ids,
            member_objects,
            mapping_proposal_commitments: commitments,
            placement_coverage: coverage,
            objectives,
            disposition,
        })
    }
}

enum CombinationEvaluation {
    HardBlocked,
    Candidate(AssemblyCompositionCandidate),
}

fn validate_limits(request: &AssemblySearchRequest<'_>) -> Result<(), AssemblySearchError> {
    if request.candidates.len() > MAX_ASSEMBLY_SEARCH_INPUTS
        || request.placements.len() > MAX_ASSEMBLY_SEARCH_PLACEMENTS
        || request.pair_compatibilities.len() > MAX_ASSEMBLY_SEARCH_COMPATIBILITIES
    {
        Err(AssemblySearchError::Limit)
    } else {
        Ok(())
    }
}

fn validate_candidate(
    candidate: &AssemblyFragmentCandidate,
    placements: &BTreeSet<PlacementId>,
) -> Result<(), AssemblySearchError> {
    if candidate.candidate_id == [0; 32] || candidate.object.cid == [0; 32] {
        return Err(AssemblySearchError::InvalidCandidateCommitment);
    }
    if candidate.evidence_domains.len() > MAX_ASSEMBLY_SEARCH_EVIDENCE_DOMAINS
        || candidate.placement_fits.len() > MAX_ASSEMBLY_SEARCH_PLACEMENTS
    {
        return Err(AssemblySearchError::Limit);
    }
    let fit_ids = candidate
        .placement_fits
        .iter()
        .map(|fit| fit.placement_id)
        .collect::<BTreeSet<_>>();
    if fit_ids.len() != candidate.placement_fits.len() {
        return Err(AssemblySearchError::DuplicatePlacementFit);
    }
    if !fit_ids.is_subset(placements) {
        return Err(AssemblySearchError::UnknownPlacementFit);
    }
    Ok(())
}

fn beam_candidate_order(
    left: &AssemblyFragmentCandidate,
    right: &AssemblyFragmentCandidate,
    placements: &[PlacementRequirement],
) -> Ordering {
    let left_schedule = candidate_schedule(left, placements);
    let right_schedule = candidate_schedule(right, placements);
    right_schedule
        .required_satisfied
        .cmp(&left_schedule.required_satisfied)
        .then_with(|| {
            right_schedule
                .required_unknown
                .cmp(&left_schedule.required_unknown)
        })
        .then_with(|| {
            right_schedule
                .optional_satisfied
                .cmp(&left_schedule.optional_satisfied)
        })
        .then_with(|| {
            right
                .systematic_connections
                .cmp(&left.systematic_connections)
        })
        .then_with(|| {
            right
                .supporting_evidence_count
                .cmp(&left.supporting_evidence_count)
        })
        .then_with(|| left.candidate_id.cmp(&right.candidate_id))
}

#[derive(Default)]
struct CandidateSchedule {
    required_satisfied: u64,
    required_unknown: u64,
    optional_satisfied: u64,
}

fn candidate_schedule(
    candidate: &AssemblyFragmentCandidate,
    placements: &[PlacementRequirement],
) -> CandidateSchedule {
    let mut schedule = CandidateSchedule::default();
    for placement in placements {
        let evaluation = candidate
            .placement_fits
            .iter()
            .find(|fit| fit.placement_id == placement.placement_id)
            .map(|fit| fit.evaluation);
        match (placement.required, evaluation) {
            (true, Some(ConstraintEvaluation::Satisfied)) => {
                schedule.required_satisfied += u64::from(placement.weight)
            }
            (true, Some(ConstraintEvaluation::Unknown)) => {
                schedule.required_unknown += u64::from(placement.weight)
            }
            (false, Some(ConstraintEvaluation::Satisfied)) => {
                schedule.optional_satisfied += u64::from(placement.weight)
            }
            _ => {}
        }
    }
    schedule
}

fn first_combination(pool_len: usize, policy: AssemblySearchPolicy) -> Option<Vec<usize>> {
    if pool_len < policy.min_size {
        None
    } else {
        Some((0..policy.min_size).collect())
    }
}

fn next_combination(current: &[usize], pool_len: usize, max_size: usize) -> Option<Vec<usize>> {
    let size = current.len();
    let mut next = current.to_vec();
    for position in (0..size).rev() {
        let maximum = pool_len - (size - position);
        if next[position] < maximum {
            next[position] += 1;
            for suffix in (position + 1)..size {
                next[suffix] = next[suffix - 1] + 1;
            }
            return Some(next);
        }
    }
    if size < max_size && size < pool_len {
        Some((0..(size + 1)).collect())
    } else {
        None
    }
}

fn validate_cursor(
    cursor: &AssemblySearchCursor,
    context_root: [u8; 32],
    pool_len: usize,
    policy: AssemblySearchPolicy,
) -> Result<(), AssemblySearchError> {
    if cursor.context_root != context_root {
        return Err(AssemblySearchError::ContinuationContextMismatch);
    }
    let size = usize::from(cursor.next_size);
    if size != cursor.next_indices.len()
        || size < policy.min_size
        || size > policy.max_size
        || size > pool_len
    {
        return Err(AssemblySearchError::InvalidContinuation);
    }
    let mut previous = None;
    for index in &cursor.next_indices {
        let index = *index as usize;
        if index >= pool_len || previous.is_some_and(|value| index <= value) {
            return Err(AssemblySearchError::InvalidContinuation);
        }
        previous = Some(index);
    }
    Ok(())
}

fn pareto_insert(
    portfolio: &mut Vec<AssemblyCompositionCandidate>,
    candidate: AssemblyCompositionCandidate,
) {
    if portfolio.iter().any(|existing| {
        existing.member_ids == candidate.member_ids || dominates(existing, &candidate)
    }) {
        return;
    }
    portfolio.retain(|existing| !dominates(&candidate, existing));
    portfolio.push(candidate);
}

fn dominates(left: &AssemblyCompositionCandidate, right: &AssemblyCompositionCandidate) -> bool {
    let left = left.objectives;
    let right = right.objectives;
    let no_worse = left.required_satisfied_weight >= right.required_satisfied_weight
        && left.optional_satisfied_weight >= right.optional_satisfied_weight
        && left.required_unknown_weight <= right.required_unknown_weight
        && left.required_unmet_weight <= right.required_unmet_weight
        && left.compatibility_unknown_count <= right.compatibility_unknown_count
        && left.soft_conflict_count <= right.soft_conflict_count
        && left.systematic_connections >= right.systematic_connections
        && left.supporting_evidence_count >= right.supporting_evidence_count
        && left.evidence_domain_count >= right.evidence_domain_count
        && left.candidate_count <= right.candidate_count;
    let strictly_better = left != right;
    no_worse && strictly_better
}

fn sort_portfolio(portfolio: &mut [AssemblyCompositionCandidate]) {
    portfolio.sort_by(|left, right| left.member_ids.cmp(&right.member_ids));
}

fn ordered_pair(left: [u8; 32], right: [u8; 32]) -> ([u8; 32], [u8; 32]) {
    if left < right {
        (left, right)
    } else {
        (right, left)
    }
}

fn context_root(
    placements: &[PlacementRequirement],
    candidates: &[AssemblyFragmentCandidate],
    compatibilities: &BTreeMap<([u8; 32], [u8; 32]), PairCompatibility>,
    policy: AssemblySearchPolicy,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:kql-assembly-search-context:1\0");
    put_u64(&mut hasher, policy.min_size as u64);
    put_u64(&mut hasher, policy.max_size as u64);
    put_u64(&mut hasher, policy.beam_width as u64);
    put_u64(&mut hasher, placements.len() as u64);
    for placement in placements {
        hasher.update(placement.placement_id.as_bytes());
        hasher.update(&[u8::from(placement.required)]);
        hasher.update(&placement.weight.to_be_bytes());
    }
    put_u64(&mut hasher, candidates.len() as u64);
    for candidate in candidates {
        hasher.update(&candidate.candidate_id);
        put_u64(&mut hasher, candidate.object.reference_kind);
        hasher.update(&candidate.object.cid);
        match candidate.mapping_proposal_commitment {
            Some(commitment) => {
                hasher.update(&[1]);
                hasher.update(&commitment);
            }
            None => {
                hasher.update(&[0]);
            }
        }
        put_u64(&mut hasher, candidate.placement_fits.len() as u64);
        for fit in &candidate.placement_fits {
            hasher.update(fit.placement_id.as_bytes());
            hasher.update(&[evaluation_tag(fit.evaluation)]);
        }
        hasher.update(&candidate.systematic_connections.to_be_bytes());
        hasher.update(&candidate.supporting_evidence_count.to_be_bytes());
        put_u64(&mut hasher, candidate.evidence_domains.len() as u64);
        for domain in &candidate.evidence_domains {
            hasher.update(domain);
        }
        hasher.update(&candidate.hard_violation_count.to_be_bytes());
    }
    put_u64(&mut hasher, compatibilities.len() as u64);
    for ((left, right), compatibility) in compatibilities {
        hasher.update(left);
        hasher.update(right);
        hasher.update(&[evaluation_tag(compatibility.evaluation)]);
        hasher.update(&[u8::from(compatibility.required)]);
    }
    *hasher.finalize().as_bytes()
}

fn evaluation_tag(evaluation: ConstraintEvaluation) -> u8 {
    match evaluation {
        ConstraintEvaluation::Satisfied => 0,
        ConstraintEvaluation::Violated => 1,
        ConstraintEvaluation::Unknown => 2,
    }
}

fn put_u64(hasher: &mut blake3::Hasher, value: u64) {
    hasher.update(&value.to_be_bytes());
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssemblySearchError {
    InvalidPolicy,
    Limit,
    DuplicatePlacement,
    ZeroPlacementWeight,
    DuplicateCandidate,
    InvalidCandidateCommitment,
    DuplicatePlacementFit,
    UnknownPlacementFit,
    SelfCompatibility,
    UnknownCompatibilityCandidate,
    DuplicateCompatibility,
    InvalidContinuation,
    ContinuationContextMismatch,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placement(id: u8, required: bool, weight: u32) -> PlacementRequirement {
        PlacementRequirement {
            placement_id: PlacementId::from_bytes([id; 32]),
            required,
            weight,
        }
    }

    fn candidate(
        id: u8,
        fits: &[(u8, ConstraintEvaluation)],
        systematic_connections: u32,
    ) -> AssemblyFragmentCandidate {
        AssemblyFragmentCandidate {
            candidate_id: [id; 32],
            object: ObjectReference::new(7, [id + 20; 32]),
            mapping_proposal_commitment: Some([id + 40; 32]),
            placement_fits: fits
                .iter()
                .map(|(placement, evaluation)| PlacementFit {
                    placement_id: PlacementId::from_bytes([*placement; 32]),
                    evaluation: *evaluation,
                })
                .collect(),
            systematic_connections,
            supporting_evidence_count: systematic_connections + 1,
            evidence_domains: vec![[id + 60; 32]],
            hard_violation_count: 0,
        }
    }

    fn compatibility(
        left: u8,
        right: u8,
        evaluation: ConstraintEvaluation,
        required: bool,
    ) -> PairCompatibility {
        PairCompatibility {
            left_candidate: [left; 32],
            right_candidate: [right; 32],
            evaluation,
            required,
        }
    }

    fn policy() -> AssemblySearchPolicy {
        AssemblySearchPolicy {
            min_size: 2,
            max_size: 4,
            beam_width: 16,
            max_expansions_per_page: 1_000,
            max_portfolio_per_page: 100,
        }
    }

    #[test]
    fn required_hard_violation_never_enters_portfolio() {
        let placements = vec![placement(1, true, 5), placement(2, true, 3)];
        let candidates = vec![
            candidate(1, &[(1, ConstraintEvaluation::Satisfied)], 2),
            candidate(2, &[(2, ConstraintEvaluation::Satisfied)], 2),
            candidate(3, &[(2, ConstraintEvaluation::Unknown)], 1),
        ];
        let compatibilities = vec![
            compatibility(1, 2, ConstraintEvaluation::Violated, true),
            compatibility(1, 3, ConstraintEvaluation::Satisfied, true),
            compatibility(2, 3, ConstraintEvaluation::Satisfied, true),
        ];
        let page = AssemblySearcher::search(AssemblySearchRequest {
            placements: &placements,
            candidates: &candidates,
            pair_compatibilities: &compatibilities,
            policy: policy(),
            continuation: None,
        })
        .unwrap();

        assert!(page.hard_blocked_combinations > 0);
        assert!(page
            .portfolio
            .iter()
            .all(|candidate| candidate.member_ids != vec![[1; 32], [2; 32]]));
        assert!(page
            .portfolio
            .iter()
            .all(|candidate| !candidate.is_materialization_authority()));
    }

    #[test]
    fn pareto_keeps_smaller_and_more_systematic_tradeoffs() {
        let placements = vec![placement(1, true, 5), placement(2, true, 5)];
        let candidates = vec![
            candidate(1, &[(1, ConstraintEvaluation::Satisfied)], 1),
            candidate(2, &[(2, ConstraintEvaluation::Satisfied)], 1),
            candidate(3, &[(1, ConstraintEvaluation::Satisfied)], 20),
        ];
        let compatibilities = vec![
            compatibility(1, 2, ConstraintEvaluation::Satisfied, true),
            compatibility(1, 3, ConstraintEvaluation::Satisfied, true),
            compatibility(2, 3, ConstraintEvaluation::Satisfied, true),
        ];
        let mut selected_policy = policy();
        selected_policy.max_size = 3;
        let page = AssemblySearcher::search(AssemblySearchRequest {
            placements: &placements,
            candidates: &candidates,
            pair_compatibilities: &compatibilities,
            policy: selected_policy,
            continuation: None,
        })
        .unwrap();

        assert!(page.portfolio.iter().any(|candidate| {
            candidate.objectives.candidate_count == 2
                && candidate.objectives.required_satisfied_weight == 10
        }));
        assert!(page.portfolio.iter().any(|candidate| {
            candidate.objectives.candidate_count == 3
                && candidate.objectives.systematic_connections == 22
        }));
    }

    #[test]
    fn continuation_names_exact_next_combination_and_merges_pages() {
        let placements = vec![placement(1, true, 1), placement(2, true, 1)];
        let candidates = vec![
            candidate(1, &[(1, ConstraintEvaluation::Satisfied)], 1),
            candidate(2, &[(2, ConstraintEvaluation::Satisfied)], 1),
            candidate(3, &[(2, ConstraintEvaluation::Unknown)], 3),
        ];
        let compatibilities = vec![
            compatibility(1, 2, ConstraintEvaluation::Satisfied, true),
            compatibility(1, 3, ConstraintEvaluation::Unknown, true),
            compatibility(2, 3, ConstraintEvaluation::Satisfied, true),
        ];
        let mut paged_policy = policy();
        paged_policy.max_expansions_per_page = 1;
        let first = AssemblySearcher::search(AssemblySearchRequest {
            placements: &placements,
            candidates: &candidates,
            pair_compatibilities: &compatibilities,
            policy: paged_policy,
            continuation: None,
        })
        .unwrap();
        let repeated = AssemblySearcher::search(AssemblySearchRequest {
            placements: &placements,
            candidates: &candidates,
            pair_compatibilities: &compatibilities,
            policy: paged_policy,
            continuation: None,
        })
        .unwrap();
        assert_eq!(first, repeated);

        let second = AssemblySearcher::search(AssemblySearchRequest {
            placements: &placements,
            candidates: &candidates,
            pair_compatibilities: &compatibilities,
            policy: paged_policy,
            continuation: first.continuation.clone(),
        })
        .unwrap();
        assert_ne!(first.portfolio, second.portfolio);
        let mut merged = AssemblyParetoPortfolio::default();
        merged.merge_page(&first).unwrap();
        merged.merge_page(&second).unwrap();
        assert!(!merged.candidates().is_empty());
    }

    #[test]
    fn absent_compatibility_is_unknown_not_false_or_ready() {
        let placements = vec![placement(1, true, 1), placement(2, true, 1)];
        let candidates = vec![
            candidate(1, &[(1, ConstraintEvaluation::Satisfied)], 1),
            candidate(2, &[(2, ConstraintEvaluation::Satisfied)], 1),
        ];
        let page = AssemblySearcher::search(AssemblySearchRequest {
            placements: &placements,
            candidates: &candidates,
            pair_compatibilities: &[],
            policy: policy(),
            continuation: None,
        })
        .unwrap();
        assert_eq!(page.portfolio.len(), 1);
        assert_eq!(page.portfolio[0].objectives.compatibility_unknown_count, 1);
        assert_eq!(
            page.portfolio[0].disposition,
            AssemblyCandidateDisposition::PartialOrUnknown
        );
    }

    #[test]
    fn input_order_does_not_change_context_or_results() {
        let placements = vec![placement(2, true, 3), placement(1, true, 5)];
        let mut candidates = vec![
            candidate(2, &[(2, ConstraintEvaluation::Satisfied)], 1),
            candidate(1, &[(1, ConstraintEvaluation::Satisfied)], 1),
        ];
        let compatibilities = vec![compatibility(2, 1, ConstraintEvaluation::Satisfied, true)];
        let first = AssemblySearcher::search(AssemblySearchRequest {
            placements: &placements,
            candidates: &candidates,
            pair_compatibilities: &compatibilities,
            policy: policy(),
            continuation: None,
        })
        .unwrap();
        candidates.reverse();
        let mut reversed_placements = placements.clone();
        reversed_placements.reverse();
        let second = AssemblySearcher::search(AssemblySearchRequest {
            placements: &reversed_placements,
            candidates: &candidates,
            pair_compatibilities: &compatibilities,
            policy: policy(),
            continuation: None,
        })
        .unwrap();
        assert_eq!(first.context_root, second.context_root);
        assert_eq!(first.portfolio, second.portfolio);
    }

    #[test]
    fn search_emits_only_configured_sizes_two_through_four() {
        let placements = vec![placement(1, true, 1)];
        let candidates = (1..=5)
            .map(|id| candidate(id, &[(1, ConstraintEvaluation::Satisfied)], id as u32))
            .collect::<Vec<_>>();
        let page = AssemblySearcher::search(AssemblySearchRequest {
            placements: &placements,
            candidates: &candidates,
            pair_compatibilities: &[],
            policy: policy(),
            continuation: None,
        })
        .unwrap();
        assert!(page.portfolio.iter().all(|candidate| {
            (2..=4).contains(&(candidate.objectives.candidate_count as usize))
        }));
        assert!(!page.claims_global_completeness());
    }
}
