//! Private-local, debt-backed exploration scheduler for complement discovery.
//!
//! Exploration changes exposure opportunity, not semantic validity. Exact CID
//! and administrative lookups bypass randomization. Popularity, aggregate
//! trust, PoMV, age, and reward are deliberately absent from eligibility.

use std::collections::BTreeSet;

use ku_core::foundation::schema_registry::OBJECT_KIND_EXPLORATION_POLICY;
use ku_core::foundation::{
    decode_canonical, encode_canonical, CanonicalValue, DisclosureClass, KnowledgeObjectEnvelope,
    ObjectCid, ObjectKind, ResourceProfile, SchemaVersion,
};

pub const EXPLORATION_POLICY_KIND: ObjectKind = ObjectKind(OBJECT_KIND_EXPLORATION_POLICY);
pub const EXPLORATION_POLICY_MAJOR: u64 = 1;
pub const EXPLORATION_POLICY_MINOR: u64 = 0;
pub const BASIS_POINTS: u16 = 10_000;
pub const MAX_EXPLORATION_CANDIDATES: usize = 4_096;
pub const MAX_EXPLORATION_SELECTIONS_PER_BATCH: usize = 1_024;
pub const MAX_PRIVATE_SELECTION_LOG: usize = 4_096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExplorationPolicyV1 {
    pub floor_basis_points: u16,
    pub default_basis_points: u16,
    pub research_basis_points: u16,
    pub ceiling_basis_points: u16,
    pub starvation_window: u8,
    pub max_exploit_streak: u8,
}

impl ExplorationPolicyV1 {
    pub const fn standard() -> Self {
        Self {
            floor_basis_points: 1_000,
            default_basis_points: 2_000,
            research_basis_points: 3_000,
            ceiling_basis_points: 4_000,
            starvation_window: 10,
            max_exploit_streak: 9,
        }
    }

    pub fn validate(self) -> Result<Self, ExplorationError> {
        if self.floor_basis_points == 0
            || self.floor_basis_points > self.default_basis_points
            || self.default_basis_points > self.research_basis_points
            || self.research_basis_points > self.ceiling_basis_points
            || self.ceiling_basis_points > BASIS_POINTS
            || self.starvation_window == 0
            || self.max_exploit_streak.saturating_add(1) != self.starvation_window
            || u32::from(self.floor_basis_points) * u32::from(self.starvation_window)
                < u32::from(BASIS_POINTS)
        {
            Err(ExplorationError::InvalidPolicy)
        } else {
            Ok(self)
        }
    }

    pub fn target_basis_points(self, profile: DiscoveryProfile) -> u16 {
        match profile {
            DiscoveryProfile::UrgentLatencyBound => self.floor_basis_points,
            DiscoveryProfile::OrdinaryComplement => self.default_basis_points,
            DiscoveryProfile::OpenScientificHighUncertainty => self.research_basis_points,
            DiscoveryProfile::Stalled {
                unchanged_revisions,
            } => {
                let adaptive = if unchanged_revisions < 2 {
                    self.default_basis_points
                } else {
                    self.research_basis_points.saturating_add(
                        u16::from(unchanged_revisions.saturating_sub(2)).saturating_mul(500),
                    )
                };
                adaptive.min(self.ceiling_basis_points)
            }
        }
    }

    pub fn to_private_knowledge_object(self) -> Result<KnowledgeObjectEnvelope, ExplorationError> {
        self.validate()?;
        Ok(KnowledgeObjectEnvelope::new(
            EXPLORATION_POLICY_KIND,
            SchemaVersion::new(EXPLORATION_POLICY_MAJOR, EXPLORATION_POLICY_MINOR),
            DisclosureClass::LocalOnly,
            CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(EXPLORATION_POLICY_MAJOR)),
                (1, CanonicalValue::Unsigned(EXPLORATION_POLICY_MINOR)),
                (
                    2,
                    CanonicalValue::Unsigned(u64::from(self.floor_basis_points)),
                ),
                (
                    3,
                    CanonicalValue::Unsigned(u64::from(self.default_basis_points)),
                ),
                (
                    4,
                    CanonicalValue::Unsigned(u64::from(self.research_basis_points)),
                ),
                (
                    5,
                    CanonicalValue::Unsigned(u64::from(self.ceiling_basis_points)),
                ),
                (
                    6,
                    CanonicalValue::Unsigned(u64::from(self.starvation_window)),
                ),
                (
                    7,
                    CanonicalValue::Unsigned(u64::from(self.max_exploit_streak)),
                ),
            ]),
        ))
    }

    pub fn policy_cid(self) -> Result<ObjectCid, ExplorationError> {
        Ok(self
            .to_private_knowledge_object()?
            .encode(ResourceProfile::ObjectV1)?
            .1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscoveryProfile {
    UrgentLatencyBound,
    OrdinaryComplement,
    OpenScientificHighUncertainty,
    Stalled { unchanged_revisions: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionScope {
    ExactCid,
    Administrative,
    Complement(DiscoveryProfile),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum ExplorationCohort {
    CrossDomainStructural = 0,
    OppositionAlternative = 1,
    ColdOldLongTail = 2,
}

impl ExplorationCohort {
    const ALL: [Self; 3] = [
        Self::CrossDomainStructural,
        Self::OppositionAlternative,
        Self::ColdOldLongTail,
    ];

    fn from_tag(tag: u64) -> Result<Self, ExplorationError> {
        match tag {
            0 => Ok(Self::CrossDomainStructural),
            1 => Ok(Self::OppositionAlternative),
            2 => Ok(Self::ColdOldLongTail),
            _ => Err(ExplorationError::InvalidPrivateState),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateLane {
    Exploit,
    Explore(ExplorationCohort),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CandidateEligibility {
    pub decodable: bool,
    pub has_compatibility_path: bool,
    pub privacy_permitted: bool,
    pub consent_permitted: bool,
    pub schema_supported: bool,
    pub within_resource_limits: bool,
}

impl CandidateEligibility {
    pub const fn eligible(self) -> bool {
        self.decodable
            && self.has_compatibility_path
            && self.privacy_permitted
            && self.consent_permitted
            && self.schema_supported
            && self.within_resource_limits
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExplorationCandidate {
    pub candidate_id: [u8; 32],
    pub lane: CandidateLane,
    pub eligibility: CandidateEligibility,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectionPropensity {
    pub numerator: u64,
    pub denominator: u64,
}

impl SelectionPropensity {
    fn new(numerator: u64, denominator: u64) -> Result<Self, ExplorationError> {
        if numerator == 0 || denominator == 0 || numerator > denominator {
            return Err(ExplorationError::InvalidPropensity);
        }
        let divisor = gcd(numerator, denominator);
        Ok(Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectedLane {
    Exploit,
    Explore(ExplorationCohort),
    ExactBypass,
    AdministrativeBypass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionReason {
    RandomExplore,
    RandomExploit,
    StarvationDebt,
    MaxExploitStreak,
    OnlyEligibleLane,
    ExactBypass,
    AdministrativeBypass,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivateSelectionRecord {
    pub selection_ordinal: u64,
    pub candidate_id: [u8; 32],
    pub lane: SelectedLane,
    pub reason: SelectionReason,
    pub propensity: SelectionPropensity,
    pub policy_cid: ObjectCid,
    pub frontier_digest: [u8; 32],
    pub rng_counter_start: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExplorationState {
    trace_id: [u8; 32],
    policy_cid: ObjectCid,
    frontier_digest: [u8; 32],
    seed: [u8; 32],
    completed_opportunities: u64,
    exploration_selections: u64,
    selection_count: u64,
    exploit_streak: u8,
    floor_debt_basis_points: u32,
    cohort_cursor: u8,
    rng_counter: u64,
    records: Vec<PrivateSelectionRecord>,
}

impl ExplorationState {
    pub fn new(
        trace_id: [u8; 32],
        policy_cid: ObjectCid,
        frontier_digest: [u8; 32],
        seed: [u8; 32],
    ) -> Result<Self, ExplorationError> {
        if trace_id == [0; 32] || frontier_digest == [0; 32] || seed == [0; 32] {
            return Err(ExplorationError::InvalidPrivateState);
        }
        Ok(Self {
            trace_id,
            policy_cid,
            frontier_digest,
            seed,
            completed_opportunities: 0,
            exploration_selections: 0,
            selection_count: 0,
            exploit_streak: 0,
            floor_debt_basis_points: 0,
            cohort_cursor: 0,
            rng_counter: 0,
            records: Vec::new(),
        })
    }

    pub const fn completed_opportunities(&self) -> u64 {
        self.completed_opportunities
    }

    pub const fn exploration_selections(&self) -> u64 {
        self.exploration_selections
    }

    pub const fn exploit_streak(&self) -> u8 {
        self.exploit_streak
    }

    pub const fn floor_debt_basis_points(&self) -> u32 {
        self.floor_debt_basis_points
    }

    pub const fn rng_counter(&self) -> u64 {
        self.rng_counter
    }

    pub fn records(&self) -> &[PrivateSelectionRecord] {
        &self.records
    }

    /// Canonical snapshot for an encrypted private-local backend. This is not a
    /// public Knowledge Object and has no OBP/disclosure conversion.
    pub fn to_private_bytes(&self) -> Result<Vec<u8>, ExplorationError> {
        if self.records.len() > MAX_PRIVATE_SELECTION_LOG {
            return Err(ExplorationError::PrivateLogLimit);
        }
        let records = self
            .records
            .iter()
            .map(selection_record_value)
            .collect::<Vec<_>>();
        Ok(encode_canonical(
            &CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(1)),
                (1, CanonicalValue::Bytes(self.trace_id.to_vec())),
                (
                    2,
                    CanonicalValue::Bytes(self.policy_cid.as_bytes().to_vec()),
                ),
                (3, CanonicalValue::Bytes(self.frontier_digest.to_vec())),
                (4, CanonicalValue::Bytes(self.seed.to_vec())),
                (5, CanonicalValue::Unsigned(self.completed_opportunities)),
                (6, CanonicalValue::Unsigned(self.exploration_selections)),
                (7, CanonicalValue::Unsigned(self.selection_count)),
                (8, CanonicalValue::Unsigned(u64::from(self.exploit_streak))),
                (
                    9,
                    CanonicalValue::Unsigned(u64::from(self.floor_debt_basis_points)),
                ),
                (10, CanonicalValue::Unsigned(u64::from(self.cohort_cursor))),
                (11, CanonicalValue::Unsigned(self.rng_counter)),
                (12, CanonicalValue::Array(records)),
                (
                    13,
                    CanonicalValue::Unsigned(DisclosureClass::LocalOnly as u64),
                ),
            ]),
            ResourceProfile::ObjectV1,
        )?)
    }

    pub fn from_private_bytes(bytes: &[u8]) -> Result<Self, ExplorationError> {
        let value = decode_canonical(bytes, ResourceProfile::ObjectV1)?;
        let map = value_map(&value)?;
        if unsigned(map, 0)? != 1 || unsigned(map, 13)? != DisclosureClass::LocalOnly as u64 {
            return Err(ExplorationError::InvalidPrivateState);
        }
        let records = match required(map, 12)? {
            CanonicalValue::Array(records) if records.len() <= MAX_PRIVATE_SELECTION_LOG => records
                .iter()
                .map(parse_selection_record)
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(ExplorationError::InvalidPrivateState),
        };
        let state = Self {
            trace_id: bytes32(map, 1)?,
            policy_cid: ObjectCid::from_bytes(bytes32(map, 2)?),
            frontier_digest: bytes32(map, 3)?,
            seed: bytes32(map, 4)?,
            completed_opportunities: unsigned(map, 5)?,
            exploration_selections: unsigned(map, 6)?,
            selection_count: unsigned(map, 7)?,
            exploit_streak: u8::try_from(unsigned(map, 8)?)
                .map_err(|_| ExplorationError::InvalidPrivateState)?,
            floor_debt_basis_points: u32::try_from(unsigned(map, 9)?)
                .map_err(|_| ExplorationError::InvalidPrivateState)?,
            cohort_cursor: u8::try_from(unsigned(map, 10)?)
                .map_err(|_| ExplorationError::InvalidPrivateState)?,
            rng_counter: unsigned(map, 11)?,
            records,
        };
        state.validate_internal()?;
        if state.to_private_bytes()? != bytes {
            return Err(ExplorationError::NonCanonicalPrivateState);
        }
        Ok(state)
    }

    fn validate_internal(&self) -> Result<(), ExplorationError> {
        if self.trace_id == [0; 32]
            || self.policy_cid.as_bytes() == &[0; 32]
            || self.frontier_digest == [0; 32]
            || self.seed == [0; 32]
            || self.cohort_cursor >= 3
            || self.exploit_streak > ExplorationPolicyV1::standard().max_exploit_streak
            || self.floor_debt_basis_points >= u32::from(BASIS_POINTS)
            || self.exploration_selections > self.completed_opportunities
            || self.records.len() > MAX_PRIVATE_SELECTION_LOG
            || self.records.iter().enumerate().any(|(index, record)| {
                record.selection_ordinal != (index as u64).saturating_add(1)
                    || record.policy_cid != self.policy_cid
                    || record.candidate_id == [0; 32]
                    || record.frontier_digest == [0; 32]
            })
            || self.selection_count != self.records.len() as u64
        {
            Err(ExplorationError::InvalidPrivateState)
        } else {
            Ok(())
        }
    }

    pub const fn claims_network_authority(&self) -> bool {
        false
    }
}

pub struct SelectionBatchRequest<'a> {
    pub scope: SelectionScope,
    pub frontier_digest: [u8; 32],
    pub slots: usize,
    pub candidates: &'a [ExplorationCandidate],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionBatchResult {
    pub selected: Vec<PrivateSelectionRecord>,
    pub unfilled_slots: usize,
    pub ineligible_candidates: usize,
    pub missing_exploration_cohorts: Vec<ExplorationCohort>,
}

pub struct ExplorationScheduler;

impl ExplorationScheduler {
    pub fn select_batch(
        policy: ExplorationPolicyV1,
        state: &mut ExplorationState,
        request: SelectionBatchRequest<'_>,
    ) -> Result<SelectionBatchResult, ExplorationError> {
        let mut next_state = state.clone();
        let result = Self::select_batch_in_place(policy, &mut next_state, request)?;
        *state = next_state;
        Ok(result)
    }

    fn select_batch_in_place(
        policy: ExplorationPolicyV1,
        state: &mut ExplorationState,
        request: SelectionBatchRequest<'_>,
    ) -> Result<SelectionBatchResult, ExplorationError> {
        let policy = policy.validate()?;
        if state.policy_cid != policy.policy_cid()? {
            return Err(ExplorationError::PolicyMismatch);
        }
        if request.frontier_digest == [0; 32]
            || request.slots == 0
            || request.slots > MAX_EXPLORATION_SELECTIONS_PER_BATCH
            || request.candidates.len() > MAX_EXPLORATION_CANDIDATES
        {
            return Err(ExplorationError::InvalidRequest);
        }
        if state.records.len().saturating_add(request.slots) > MAX_PRIVATE_SELECTION_LOG {
            return Err(ExplorationError::PrivateLogLimit);
        }

        let mut candidates = request.candidates.to_vec();
        candidates.sort_by_key(|candidate| candidate.candidate_id);
        if candidates
            .iter()
            .any(|candidate| candidate.candidate_id == [0; 32])
        {
            return Err(ExplorationError::InvalidCandidate);
        }
        if candidates
            .windows(2)
            .any(|pair| pair[0].candidate_id == pair[1].candidate_id)
        {
            return Err(ExplorationError::DuplicateCandidate);
        }
        let ineligible_candidates = candidates
            .iter()
            .filter(|candidate| !candidate.eligibility.eligible())
            .count();
        candidates.retain(|candidate| candidate.eligibility.eligible());
        state.frontier_digest = request.frontier_digest;

        let mut selected_ids = BTreeSet::new();
        let mut selected = Vec::new();
        match request.scope {
            SelectionScope::ExactCid | SelectionScope::Administrative => {
                for candidate in candidates.iter().take(request.slots) {
                    let (lane, reason) = match request.scope {
                        SelectionScope::ExactCid => {
                            (SelectedLane::ExactBypass, SelectionReason::ExactBypass)
                        }
                        SelectionScope::Administrative => (
                            SelectedLane::AdministrativeBypass,
                            SelectionReason::AdministrativeBypass,
                        ),
                        SelectionScope::Complement(_) => unreachable!(),
                    };
                    let record = state.record(
                        candidate.candidate_id,
                        lane,
                        reason,
                        SelectionPropensity::new(1, 1)?,
                        state.rng_counter,
                    )?;
                    selected.push(record);
                }
            }
            SelectionScope::Complement(profile) => {
                let target = policy.target_basis_points(profile);
                while selected.len() < request.slots {
                    let exploit = candidates
                        .iter()
                        .filter(|candidate| {
                            candidate.lane == CandidateLane::Exploit
                                && !selected_ids.contains(&candidate.candidate_id)
                        })
                        .collect::<Vec<_>>();
                    let explore_available = candidates.iter().any(|candidate| {
                        matches!(candidate.lane, CandidateLane::Explore(_))
                            && !selected_ids.contains(&candidate.candidate_id)
                    });
                    if exploit.is_empty() && !explore_available {
                        break;
                    }
                    let rng_counter_start = state.rng_counter;
                    let (choose_explore, reason, lane_numerator, lane_denominator) = choose_lane(
                        policy,
                        state,
                        target,
                        !exploit.is_empty(),
                        explore_available,
                    )?;

                    let chosen = if choose_explore {
                        select_exploration_candidate(state, &candidates, &selected_ids)?
                    } else if exploit.is_empty() {
                        None
                    } else {
                        let index = uniform_below(state, exploit.len() as u64)? as usize;
                        Some(ExplorationPoolSelection {
                            candidate: exploit[index],
                            cohort: None,
                            pool_size: exploit.len(),
                        })
                    };
                    let Some(chosen) = chosen else {
                        break;
                    };
                    let propensity = SelectionPropensity::new(
                        lane_numerator,
                        lane_denominator.saturating_mul(chosen.pool_size as u64),
                    )?;
                    let lane = match chosen.cohort {
                        Some(cohort) => SelectedLane::Explore(cohort),
                        None => SelectedLane::Exploit,
                    };
                    let record = state.record(
                        chosen.candidate.candidate_id,
                        lane,
                        reason,
                        propensity,
                        rng_counter_start,
                    )?;
                    selected_ids.insert(chosen.candidate.candidate_id);
                    state.complete_opportunity(
                        policy,
                        matches!(lane, SelectedLane::Explore(_)),
                        explore_available,
                    );
                    selected.push(record);
                }
            }
        }

        let missing_exploration_cohorts = if selected
            .iter()
            .filter(|record| matches!(record.lane, SelectedLane::Explore(_)))
            .count()
            >= 3
        {
            ExplorationCohort::ALL
                .into_iter()
                .filter(|cohort| {
                    !selected
                        .iter()
                        .any(|record| record.lane == SelectedLane::Explore(*cohort))
                })
                .collect()
        } else {
            Vec::new()
        };
        let unfilled_slots = request.slots.saturating_sub(selected.len());
        Ok(SelectionBatchResult {
            selected,
            unfilled_slots,
            ineligible_candidates,
            missing_exploration_cohorts,
        })
    }
}

impl ExplorationState {
    fn record(
        &mut self,
        candidate_id: [u8; 32],
        lane: SelectedLane,
        reason: SelectionReason,
        propensity: SelectionPropensity,
        rng_counter_start: u64,
    ) -> Result<PrivateSelectionRecord, ExplorationError> {
        if self.records.len() >= MAX_PRIVATE_SELECTION_LOG {
            return Err(ExplorationError::PrivateLogLimit);
        }
        self.selection_count = self.selection_count.saturating_add(1);
        let record = PrivateSelectionRecord {
            selection_ordinal: self.selection_count,
            candidate_id,
            lane,
            reason,
            propensity,
            policy_cid: self.policy_cid,
            frontier_digest: self.frontier_digest,
            rng_counter_start,
        };
        self.records.push(record.clone());
        Ok(record)
    }

    fn complete_opportunity(
        &mut self,
        policy: ExplorationPolicyV1,
        explored: bool,
        exploration_was_available: bool,
    ) {
        if !exploration_was_available {
            return;
        }
        self.completed_opportunities = self.completed_opportunities.saturating_add(1);
        let projected = self
            .floor_debt_basis_points
            .saturating_add(u32::from(policy.floor_basis_points));
        if explored {
            self.exploration_selections = self.exploration_selections.saturating_add(1);
            self.exploit_streak = 0;
            self.floor_debt_basis_points = projected.saturating_sub(u32::from(BASIS_POINTS));
        } else {
            self.exploit_streak = self.exploit_streak.saturating_add(1);
            self.floor_debt_basis_points = projected;
        }
    }
}

fn choose_lane(
    policy: ExplorationPolicyV1,
    state: &mut ExplorationState,
    target_basis_points: u16,
    exploit_available: bool,
    explore_available: bool,
) -> Result<(bool, SelectionReason, u64, u64), ExplorationError> {
    if !exploit_available {
        return Ok((true, SelectionReason::OnlyEligibleLane, 1, 1));
    }
    if !explore_available {
        return Ok((false, SelectionReason::OnlyEligibleLane, 1, 1));
    }
    if state.exploit_streak >= policy.max_exploit_streak {
        return Ok((true, SelectionReason::MaxExploitStreak, 1, 1));
    }
    if state
        .floor_debt_basis_points
        .saturating_add(u32::from(policy.floor_basis_points))
        >= u32::from(BASIS_POINTS)
    {
        return Ok((true, SelectionReason::StarvationDebt, 1, 1));
    }
    let draw = uniform_below(state, u64::from(BASIS_POINTS))?;
    if draw < u64::from(target_basis_points) {
        Ok((
            true,
            SelectionReason::RandomExplore,
            u64::from(target_basis_points),
            u64::from(BASIS_POINTS),
        ))
    } else {
        Ok((
            false,
            SelectionReason::RandomExploit,
            u64::from(BASIS_POINTS - target_basis_points),
            u64::from(BASIS_POINTS),
        ))
    }
}

struct ExplorationPoolSelection<'a> {
    candidate: &'a ExplorationCandidate,
    cohort: Option<ExplorationCohort>,
    pool_size: usize,
}

fn select_exploration_candidate<'a>(
    state: &mut ExplorationState,
    candidates: &'a [ExplorationCandidate],
    selected: &BTreeSet<[u8; 32]>,
) -> Result<Option<ExplorationPoolSelection<'a>>, ExplorationError> {
    for offset in 0..ExplorationCohort::ALL.len() {
        let cohort_index = (usize::from(state.cohort_cursor) + offset) % 3;
        let cohort = ExplorationCohort::ALL[cohort_index];
        let pool = candidates
            .iter()
            .filter(|candidate| {
                candidate.lane == CandidateLane::Explore(cohort)
                    && !selected.contains(&candidate.candidate_id)
            })
            .collect::<Vec<_>>();
        if !pool.is_empty() {
            let index = uniform_below(state, pool.len() as u64)? as usize;
            state.cohort_cursor = ((cohort_index + 1) % 3) as u8;
            return Ok(Some(ExplorationPoolSelection {
                candidate: pool[index],
                cohort: Some(cohort),
                pool_size: pool.len(),
            }));
        }
    }
    Ok(None)
}

fn uniform_below(state: &mut ExplorationState, upper: u64) -> Result<u64, ExplorationError> {
    if upper == 0 {
        return Err(ExplorationError::RngRange);
    }
    let zone = u64::MAX - (u64::MAX % upper);
    for _ in 0..32 {
        let counter = state.rng_counter;
        state.rng_counter = state.rng_counter.saturating_add(1);
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:exploration-seeded-draw:1\0");
        hasher.update(&state.seed);
        hasher.update(&state.trace_id);
        hasher.update(state.policy_cid.as_bytes());
        hasher.update(&state.frontier_digest);
        hasher.update(&counter.to_be_bytes());
        let digest = hasher.finalize();
        let mut first = [0u8; 8];
        first.copy_from_slice(&digest.as_bytes()[..8]);
        let value = u64::from_be_bytes(first);
        if value < zone {
            return Ok(value % upper);
        }
    }
    Err(ExplorationError::RngRejectionLimit)
}

fn selection_record_value(record: &PrivateSelectionRecord) -> CanonicalValue {
    let (lane, cohort) = match record.lane {
        SelectedLane::Exploit => (0, None),
        SelectedLane::Explore(cohort) => (1, Some(cohort as u64)),
        SelectedLane::ExactBypass => (2, None),
        SelectedLane::AdministrativeBypass => (3, None),
    };
    let mut fields = vec![
        (0, CanonicalValue::Unsigned(record.selection_ordinal)),
        (1, CanonicalValue::Bytes(record.candidate_id.to_vec())),
        (2, CanonicalValue::Unsigned(lane)),
        (4, CanonicalValue::Unsigned(reason_tag(record.reason))),
        (5, CanonicalValue::Unsigned(record.propensity.numerator)),
        (6, CanonicalValue::Unsigned(record.propensity.denominator)),
        (
            7,
            CanonicalValue::Bytes(record.policy_cid.as_bytes().to_vec()),
        ),
        (8, CanonicalValue::Bytes(record.frontier_digest.to_vec())),
        (9, CanonicalValue::Unsigned(record.rng_counter_start)),
    ];
    if let Some(cohort) = cohort {
        fields.push((3, CanonicalValue::Unsigned(cohort)));
    }
    CanonicalValue::Map(fields)
}

fn parse_selection_record(
    value: &CanonicalValue,
) -> Result<PrivateSelectionRecord, ExplorationError> {
    let map = value_map(value)?;
    let lane_tag = unsigned(map, 2)?;
    let lane = match lane_tag {
        0 => SelectedLane::Exploit,
        1 => SelectedLane::Explore(ExplorationCohort::from_tag(unsigned(map, 3)?)?),
        2 => SelectedLane::ExactBypass,
        3 => SelectedLane::AdministrativeBypass,
        _ => return Err(ExplorationError::InvalidPrivateState),
    };
    Ok(PrivateSelectionRecord {
        selection_ordinal: unsigned(map, 0)?,
        candidate_id: bytes32(map, 1)?,
        lane,
        reason: parse_reason(unsigned(map, 4)?)?,
        propensity: SelectionPropensity::new(unsigned(map, 5)?, unsigned(map, 6)?)?,
        policy_cid: ObjectCid::from_bytes(bytes32(map, 7)?),
        frontier_digest: bytes32(map, 8)?,
        rng_counter_start: unsigned(map, 9)?,
    })
}

fn reason_tag(reason: SelectionReason) -> u64 {
    match reason {
        SelectionReason::RandomExplore => 0,
        SelectionReason::RandomExploit => 1,
        SelectionReason::StarvationDebt => 2,
        SelectionReason::MaxExploitStreak => 3,
        SelectionReason::OnlyEligibleLane => 4,
        SelectionReason::ExactBypass => 5,
        SelectionReason::AdministrativeBypass => 6,
    }
}

fn parse_reason(tag: u64) -> Result<SelectionReason, ExplorationError> {
    match tag {
        0 => Ok(SelectionReason::RandomExplore),
        1 => Ok(SelectionReason::RandomExploit),
        2 => Ok(SelectionReason::StarvationDebt),
        3 => Ok(SelectionReason::MaxExploitStreak),
        4 => Ok(SelectionReason::OnlyEligibleLane),
        5 => Ok(SelectionReason::ExactBypass),
        6 => Ok(SelectionReason::AdministrativeBypass),
        _ => Err(ExplorationError::InvalidPrivateState),
    }
}

fn value_map(value: &CanonicalValue) -> Result<&[(u64, CanonicalValue)], ExplorationError> {
    match value {
        CanonicalValue::Map(map) => Ok(map),
        _ => Err(ExplorationError::InvalidPrivateState),
    }
}

fn required(map: &[(u64, CanonicalValue)], key: u64) -> Result<&CanonicalValue, ExplorationError> {
    map.iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .ok_or(ExplorationError::InvalidPrivateState)
}

fn unsigned(map: &[(u64, CanonicalValue)], key: u64) -> Result<u64, ExplorationError> {
    match required(map, key)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(ExplorationError::InvalidPrivateState),
    }
}

fn bytes32(map: &[(u64, CanonicalValue)], key: u64) -> Result<[u8; 32], ExplorationError> {
    match required(map, key)? {
        CanonicalValue::Bytes(bytes) if bytes.len() == 32 => {
            let mut output = [0u8; 32];
            output.copy_from_slice(bytes);
            Ok(output)
        }
        _ => Err(ExplorationError::InvalidPrivateState),
    }
}

fn gcd(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExplorationError {
    Canonical(ku_core::foundation::CanonicalError),
    Object(ku_core::foundation::ObjectError),
    InvalidPolicy,
    InvalidRequest,
    InvalidCandidate,
    DuplicateCandidate,
    PolicyMismatch,
    InvalidPropensity,
    InvalidPrivateState,
    NonCanonicalPrivateState,
    PrivateLogLimit,
    RngRange,
    RngRejectionLimit,
}

impl From<ku_core::foundation::CanonicalError> for ExplorationError {
    fn from(error: ku_core::foundation::CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<ku_core::foundation::ObjectError> for ExplorationError {
    fn from(error: ku_core::foundation::ObjectError) -> Self {
        Self::Object(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eligible() -> CandidateEligibility {
        CandidateEligibility {
            decodable: true,
            has_compatibility_path: true,
            privacy_permitted: true,
            consent_permitted: true,
            schema_supported: true,
            within_resource_limits: true,
        }
    }

    fn candidate(id: u8, lane: CandidateLane) -> ExplorationCandidate {
        ExplorationCandidate {
            candidate_id: [id; 32],
            lane,
            eligibility: eligible(),
        }
    }

    fn state(policy: ExplorationPolicyV1) -> ExplorationState {
        ExplorationState::new([1; 32], policy.policy_cid().unwrap(), [2; 32], [3; 32]).unwrap()
    }

    #[test]
    fn standard_policy_has_frozen_profile_and_private_cid() {
        let policy = ExplorationPolicyV1::standard();
        assert_eq!(
            policy.target_basis_points(DiscoveryProfile::UrgentLatencyBound),
            1_000
        );
        assert_eq!(
            policy.target_basis_points(DiscoveryProfile::OrdinaryComplement),
            2_000
        );
        assert_eq!(
            policy.target_basis_points(DiscoveryProfile::OpenScientificHighUncertainty),
            3_000
        );
        assert_eq!(
            policy.target_basis_points(DiscoveryProfile::Stalled {
                unchanged_revisions: 9,
            }),
            4_000
        );
        let object = policy.to_private_knowledge_object().unwrap();
        assert_eq!(object.disclosure, DisclosureClass::LocalOnly);
        assert_ne!(policy.policy_cid().unwrap().as_bytes(), &[0; 32]);
    }

    #[test]
    fn no_ten_exploit_opportunities_when_exploration_is_eligible() {
        let policy = ExplorationPolicyV1::standard();
        let mut state = state(policy);
        for round in 0..40u8 {
            let candidates = vec![
                candidate(10 + round, CandidateLane::Exploit),
                candidate(
                    100 + round,
                    CandidateLane::Explore(ExplorationCohort::ColdOldLongTail),
                ),
            ];
            ExplorationScheduler::select_batch(
                policy,
                &mut state,
                SelectionBatchRequest {
                    scope: SelectionScope::Complement(DiscoveryProfile::UrgentLatencyBound),
                    frontier_digest: [round.saturating_add(1); 32],
                    slots: 1,
                    candidates: &candidates,
                },
            )
            .unwrap();
            assert!(state.exploit_streak() <= 9);
        }
        assert!(state.exploration_selections() >= 4);
    }

    #[test]
    fn debt_survives_restart_revision_and_partition_frontier_change() {
        let policy = ExplorationPolicyV1::standard();
        let mut before = state(policy);
        before.completed_opportunities = 9;
        before.exploit_streak = 9;
        before.floor_debt_basis_points = 9_000;
        let bytes = before.to_private_bytes().unwrap();
        let mut restarted = ExplorationState::from_private_bytes(&bytes).unwrap();
        let candidates = vec![
            candidate(1, CandidateLane::Exploit),
            candidate(
                2,
                CandidateLane::Explore(ExplorationCohort::CrossDomainStructural),
            ),
        ];
        let result = ExplorationScheduler::select_batch(
            policy,
            &mut restarted,
            SelectionBatchRequest {
                scope: SelectionScope::Complement(DiscoveryProfile::UrgentLatencyBound),
                frontier_digest: [99; 32],
                slots: 1,
                candidates: &candidates,
            },
        )
        .unwrap();
        assert!(matches!(result.selected[0].lane, SelectedLane::Explore(_)));
        assert!(matches!(
            result.selected[0].reason,
            SelectionReason::MaxExploitStreak | SelectionReason::StarvationDebt
        ));
        assert_eq!(result.selected[0].frontier_digest, [99; 32]);
        assert_eq!(restarted.exploit_streak(), 0);
    }

    #[test]
    fn exact_and_admin_bypass_do_not_consume_rng_or_debt() {
        let policy = ExplorationPolicyV1::standard();
        let candidates = vec![
            candidate(3, CandidateLane::Exploit),
            candidate(
                1,
                CandidateLane::Explore(ExplorationCohort::ColdOldLongTail),
            ),
        ];
        for scope in [SelectionScope::ExactCid, SelectionScope::Administrative] {
            let mut state = state(policy);
            state.floor_debt_basis_points = 9_000;
            let result = ExplorationScheduler::select_batch(
                policy,
                &mut state,
                SelectionBatchRequest {
                    scope,
                    frontier_digest: [8; 32],
                    slots: 2,
                    candidates: &candidates,
                },
            )
            .unwrap();
            assert_eq!(result.selected[0].candidate_id, [1; 32]);
            assert_eq!(state.rng_counter(), 0);
            assert_eq!(state.floor_debt_basis_points(), 9_000);
            assert_eq!(state.completed_opportunities(), 0);
        }
    }

    #[test]
    fn three_exploration_slots_cover_all_available_cohorts() {
        let policy = ExplorationPolicyV1::standard();
        let mut state = state(policy);
        let candidates = vec![
            candidate(
                1,
                CandidateLane::Explore(ExplorationCohort::CrossDomainStructural),
            ),
            candidate(
                2,
                CandidateLane::Explore(ExplorationCohort::OppositionAlternative),
            ),
            candidate(
                3,
                CandidateLane::Explore(ExplorationCohort::ColdOldLongTail),
            ),
        ];
        let result = ExplorationScheduler::select_batch(
            policy,
            &mut state,
            SelectionBatchRequest {
                scope: SelectionScope::Complement(DiscoveryProfile::OpenScientificHighUncertainty),
                frontier_digest: [4; 32],
                slots: 3,
                candidates: &candidates,
            },
        )
        .unwrap();
        assert_eq!(result.selected.len(), 3);
        assert!(result.missing_exploration_cohorts.is_empty());
        assert_eq!(
            result
                .selected
                .iter()
                .filter_map(|record| match record.lane {
                    SelectedLane::Explore(cohort) => Some(cohort),
                    _ => None,
                })
                .collect::<BTreeSet<_>>()
                .len(),
            3
        );
    }

    #[test]
    fn seeded_replay_logs_policy_frontier_and_exact_propensity() {
        let policy = ExplorationPolicyV1::standard();
        let candidates = vec![
            candidate(1, CandidateLane::Exploit),
            candidate(2, CandidateLane::Exploit),
            candidate(
                3,
                CandidateLane::Explore(ExplorationCohort::CrossDomainStructural),
            ),
            candidate(
                4,
                CandidateLane::Explore(ExplorationCohort::CrossDomainStructural),
            ),
        ];
        let mut left = state(policy);
        let mut right = left.clone();
        let request = || SelectionBatchRequest {
            scope: SelectionScope::Complement(DiscoveryProfile::OrdinaryComplement),
            frontier_digest: [7; 32],
            slots: 2,
            candidates: &candidates,
        };
        let left_result = ExplorationScheduler::select_batch(policy, &mut left, request()).unwrap();
        let right_result =
            ExplorationScheduler::select_batch(policy, &mut right, request()).unwrap();
        assert_eq!(left_result, right_result);
        let private_bytes = left.to_private_bytes().unwrap();
        assert_eq!(private_bytes, right.to_private_bytes().unwrap());
        assert_eq!(
            ExplorationState::from_private_bytes(&private_bytes).unwrap(),
            left
        );
        for record in &left_result.selected {
            assert_eq!(record.policy_cid, policy.policy_cid().unwrap());
            assert_eq!(record.frontier_digest, [7; 32]);
            assert!(record.propensity.numerator > 0);
            assert!(record.propensity.numerator <= record.propensity.denominator);
        }
        assert!(!left.claims_network_authority());
    }

    #[test]
    fn failed_batch_does_not_partially_mutate_private_state() {
        let policy = ExplorationPolicyV1::standard();
        let mut state = state(policy);
        let before = state.clone();
        let duplicate = vec![
            candidate(1, CandidateLane::Exploit),
            candidate(
                1,
                CandidateLane::Explore(ExplorationCohort::ColdOldLongTail),
            ),
        ];
        let error = ExplorationScheduler::select_batch(
            policy,
            &mut state,
            SelectionBatchRequest {
                scope: SelectionScope::Complement(DiscoveryProfile::OrdinaryComplement),
                frontier_digest: [9; 32],
                slots: 1,
                candidates: &duplicate,
            },
        )
        .unwrap_err();
        assert_eq!(error, ExplorationError::DuplicateCandidate);
        assert_eq!(state, before);
    }
}
