//! Dual-track OBT issuance evaluation: `D-014` Track A accepted work and `M7` Track B attributed benefit (`§4.1–4.2`).

use std::collections::{BTreeMap, BTreeSet};

use ku_core::foundation::{
    ActorId, AttributionStatus, ConceptCcid, CorrelationDimension, CorrelationEvidence, EventCid,
    EvidenceLimitation, EvidenceStrength, ObjectCid, OutcomeValence, PermitCid,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::codec::domain_digest;
use crate::nullifier::{ScopedNullifier, WorkRoleSlot};
use crate::policy::{
    CausalSupportTier, CorroborationTier, EvidenceQualityTier, PolicyError, RewardPolicyV1,
    WorkClass, MAX_RESOLVED_CONSTRAINTS_CAP, MAX_VERIFIED_STATEMENTS_CAP,
};
use crate::units::{compute_benefit_budget, ObtAmount, ObtMathError, BPS_DENOMINATOR};

const WORK_CLAIM_DOMAIN: &[u8] = b"onebrain:vnext:obt-work-claim:1\0";
const BENEFIT_CLAIM_DOMAIN: &[u8] = b"onebrain:vnext:obt-benefit-claim:1\0";
const BLIND_COMMIT_DOMAIN: &[u8] = b"onebrain:vnext:obt-blind-commit:1\0";
const CORRELATION_GROUP_4D_DOMAIN: &[u8] = b"onebrain:vnext:obt-correlation-4d-group:1\0";

/// Required 4-dimensional correlation lattice dimensions (`P4 §5`, `P5 §4.1`).
pub const REQUIRED_4D_DIMENSIONS: [CorrelationDimension; 4] = [
    CorrelationDimension::AdministrativePrincipal,
    CorrelationDimension::PipelineModelLineage,
    CorrelationDimension::ExecutionEnvironment,
    CorrelationDimension::SourceAcquisitionOrDerivation,
];

/// Validated 4D correlation group identifier extracted from `CryptoBound` or `ExternallyAttested` evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CorrelationGroupId {
    pub group_digest: [u8; 32],
    pub admin_commitment: [u8; 32],
    pub model_commitment: [u8; 32],
    pub env_commitment: [u8; 32],
    pub source_commitment: [u8; 32],
}

impl CorrelationGroupId {
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.group_digest
    }
}

/// Extract a 4D `CorrelationGroupId` iff all 4 dimensions carry non-zero commitments
/// backed by `EvidenceStrength::CryptoBound` or `EvidenceStrength::ExternallyAttested`.
///
/// `SelfClaimed`, `Unknown`, and `EmpiricallyEstimated` dimensions never form evidenced groups.
pub fn extract_4d_correlation_group(evidence: &CorrelationEvidence) -> Option<CorrelationGroupId> {
    if evidence.canonical_value().is_err() {
        return None;
    }
    let mut extracted = [[0u8; 32]; 4];
    let mut hasher = blake3::Hasher::new();
    hasher.update(CORRELATION_GROUP_4D_DOMAIN);

    for (idx, dim) in REQUIRED_4D_DIMENSIONS.iter().enumerate() {
        let item = evidence.dimension(*dim)?;
        if !matches!(
            item.strength,
            EvidenceStrength::CryptoBound | EvidenceStrength::ExternallyAttested
        ) {
            return None;
        }
        let commitment = item.value_commitment?;
        if commitment == [0u8; 32] {
            return None;
        }
        extracted[idx] = commitment;
        hasher.update(&[*dim as u8]);
        hasher.update(&commitment);
    }

    Some(CorrelationGroupId {
        group_digest: *hasher.finalize().as_bytes(),
        admin_commitment: extracted[0],
        model_commitment: extracted[1],
        env_commitment: extracted[2],
        source_commitment: extracted[3],
    })
}

/// Count evidenced-distinct 4D correlation groups.
///
/// Two attestations count toward distinct groups only when their `AdministrativePrincipal`
/// and `PipelineModelLineage` commitments are non-overlapping (`P4 §5`, anti-Sybil).
pub fn count_distinct_4d_groups(groups: &[CorrelationGroupId]) -> usize {
    let mut sorted = groups.to_vec();
    sorted.sort();
    sorted.dedup();

    let mut used_admins = BTreeSet::new();
    let mut used_models = BTreeSet::new();
    let mut count = 0usize;
    for g in sorted {
        if !used_admins.contains(&g.admin_commitment) && !used_models.contains(&g.model_commitment)
        {
            used_admins.insert(g.admin_commitment);
            used_models.insert(g.model_commitment);
            count += 1;
        }
    }
    count
}

/// Requester-authorized delegated work job proof (`D-013` / `D-014`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegatedWorkJobProof {
    pub job_id: [u8; 32],
    pub requester_actor_id: ActorId,
    pub permit_cid: PermitCid,
    pub source_artifact_cid: ObjectCid,
    pub compiler_profile_cid: ObjectCid,
}

impl DelegatedWorkJobProof {
    pub fn validate(&self) -> Result<(), ClaimError> {
        if self.job_id == [0u8; 32]
            || *self.permit_cid.as_bytes() == [0u8; 32]
            || *self.source_artifact_cid.as_bytes() == [0u8; 32]
            || *self.compiler_profile_cid.as_bytes() == [0u8; 32]
        {
            return Err(ClaimError::InvalidDelegatedJobProof);
        }
        Ok(())
    }
}

/// Blind commit-before-reveal transcript proof (`FID-002`, `D-014`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlindCommitRevealProof {
    pub attempt_object_cid: ObjectCid,
    pub attempt_commit_digest: [u8; 32],
    pub reveal_output_digest: [u8; 32],
    pub blind_session_nonce: [u8; 32],
    pub commit_preceded_reveal: bool,
}

impl BlindCommitRevealProof {
    pub fn compute_commit_digest(
        reveal_output_digest: [u8; 32],
        blind_session_nonce: [u8; 32],
    ) -> [u8; 32] {
        let mut buf = [0u8; 64];
        buf[..32].copy_from_slice(&reveal_output_digest);
        buf[32..].copy_from_slice(&blind_session_nonce);
        domain_digest(BLIND_COMMIT_DOMAIN, &buf)
    }

    pub fn new_committed(
        attempt_object_cid: ObjectCid,
        reveal_output_digest: [u8; 32],
        blind_session_nonce: [u8; 32],
    ) -> Self {
        let attempt_commit_digest =
            Self::compute_commit_digest(reveal_output_digest, blind_session_nonce);
        Self {
            attempt_object_cid,
            attempt_commit_digest,
            reveal_output_digest,
            blind_session_nonce,
            commit_preceded_reveal: true,
        }
    }

    pub fn validate(&self) -> Result<(), ClaimError> {
        if *self.attempt_object_cid.as_bytes() == [0u8; 32]
            || self.attempt_commit_digest == [0u8; 32]
            || self.reveal_output_digest == [0u8; 32]
            || self.blind_session_nonce == [0u8; 32]
            || !self.commit_preceded_reveal
        {
            return Err(ClaimError::InvalidBlindCommitReveal);
        }
        let expected =
            Self::compute_commit_digest(self.reveal_output_digest, self.blind_session_nonce);
        if self.attempt_commit_digest != expected {
            return Err(ClaimError::InvalidBlindCommitReveal);
        }
        Ok(())
    }
}

/// Concrete span-grounded semantic mismatch category (`D-014`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u64)]
pub enum SemanticMismatchKind {
    QuoteSpanDiscrepancy = 0,
    UnitOrQuantityInversion = 1,
    NegationOrModalityFlip = 2,
    ConceptOrGeneMisbinding = 3,
}

/// Track A work verdict (`D-014` Evidenced-Mismatch Symmetry).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkVerdict {
    FaithfulAlignment,
    EvidencedSemanticMismatch {
        span_quote_digest: [u8; 32],
        mismatch_kind: SemanticMismatchKind,
    },
}

impl WorkVerdict {
    pub fn validate(&self) -> Result<(), ClaimError> {
        match self {
            Self::FaithfulAlignment => Ok(()),
            Self::EvidencedSemanticMismatch {
                span_quote_digest, ..
            } => {
                if *span_quote_digest == [0u8; 32] {
                    Err(ClaimError::EmptyMismatchEvidenceDigest)
                } else {
                    Ok(())
                }
            }
        }
    }
}

/// Independent verifier attestation entry for a `WorkRewardClaimV1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifierGroupAttestation {
    pub attestation_event_cid: EventCid,
    pub verifier_actor_id: ActorId,
    pub correlation_evidence: CorrelationEvidence,
    pub verdict: WorkVerdict,
}

/// Track A (`D-014`) direct work reward claim.
///
/// Contains strictly zero UTF-8 or raw byte-length inputs; payout is derived exclusively
/// from `verified_statement_count.min(32)` and `resolved_constraint_count.min(64)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkRewardClaimV1 {
    pub worker_actor_id: ActorId,
    pub work_class: WorkClass,
    pub role_slot: WorkRoleSlot,
    pub epoch_index: u64,
    pub job_proof: DelegatedWorkJobProof,
    pub blind_proof: BlindCommitRevealProof,
    pub verdict: WorkVerdict,
    pub verified_statement_count: u32,
    pub resolved_constraint_count: u32,
    pub worker_correlation: CorrelationEvidence,
    pub verifier_attestations: Vec<VerifierGroupAttestation>,
    pub reward_policy_cid: ObjectCid,
    pub evidence_frontier_root: [u8; 32],
}

impl WorkRewardClaimV1 {
    pub fn nullifier(&self) -> ScopedNullifier {
        ScopedNullifier::for_work(
            self.job_proof.source_artifact_cid,
            self.job_proof.compiler_profile_cid,
            self.role_slot,
            self.worker_actor_id,
            self.reward_policy_cid,
        )
    }

    pub fn claim_cid(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(WORK_CLAIM_DOMAIN);
        hasher.update(self.worker_actor_id.as_bytes());
        hasher.update(&[self.work_class as u8]);
        hasher.update(&self.role_slot.canonical_tag_bytes());
        hasher.update(&self.epoch_index.to_be_bytes());
        hasher.update(&self.job_proof.job_id);
        hasher.update(self.job_proof.requester_actor_id.as_bytes());
        hasher.update(self.job_proof.permit_cid.as_bytes());
        hasher.update(self.job_proof.source_artifact_cid.as_bytes());
        hasher.update(self.job_proof.compiler_profile_cid.as_bytes());
        hasher.update(self.blind_proof.attempt_object_cid.as_bytes());
        hasher.update(&self.blind_proof.attempt_commit_digest);
        hasher.update(&self.verified_statement_count.to_be_bytes());
        hasher.update(&self.resolved_constraint_count.to_be_bytes());
        match self.verdict {
            WorkVerdict::FaithfulAlignment => {
                hasher.update(&[0]);
            }
            WorkVerdict::EvidencedSemanticMismatch {
                span_quote_digest,
                mismatch_kind,
            } => {
                hasher.update(&[1, mismatch_kind as u8]);
                hasher.update(&span_quote_digest);
            }
        }
        hasher.update(self.reward_policy_cid.as_bytes());
        hasher.update(&self.evidence_frontier_root);
        *hasher.finalize().as_bytes()
    }
}

/// Contributor roles receiving integer bps shares in Track B `AttributionGraph` (`§4.2.2`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum ContributorRole {
    KuAuthor = 0,
    MappingAuthor = 1,
    AssemblyAuthor = 2,
    TrackAEncoder = 3,
    TrackAVerifier = 4,
    ProviderLeaseServer = 5,
}

impl ContributorRole {
    pub(crate) const fn from_code(code: u64) -> Option<Self> {
        match code {
            0 => Some(Self::KuAuthor),
            1 => Some(Self::MappingAuthor),
            2 => Some(Self::AssemblyAuthor),
            3 => Some(Self::TrackAEncoder),
            4 => Some(Self::TrackAVerifier),
            5 => Some(Self::ProviderLeaseServer),
            _ => None,
        }
    }
}

/// Causal contributor node along a `DerivationEvidence` DAG for hop-attenuated share synthesis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivationContributorNode {
    pub object_cid: ObjectCid,
    pub role: ContributorRole,
    pub actor: ActorId,
    pub hop_distance: u16,
    pub parent_objects: Vec<ObjectCid>,
    pub base_weight_units: u32,
}

/// Individual contributor payout computed by `AttributionGraph::distribute`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttributedPayout {
    pub role: ContributorRole,
    pub beneficiary: ActorId,
    pub share_bps: u16,
    pub amount: ObtAmount,
}

/// Conservation-verified payout breakdown for a `BenefitBudget`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BenefitDistribution {
    pub total_budget: ObtAmount,
    pub payouts: Vec<AttributedPayout>,
    pub total_allocated: ObtAmount,
    pub unminted_dust: ObtAmount,
}

/// Integer basis-point attribution graph (`§4.2.2`).
///
/// Enforces $\sum s_k \le 10\_000$ and $\sum \lfloor \text{budget} \cdot s_k / 10\_000 \rfloor \le \text{budget}$
/// with unminted dust recorded explicitly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttributionGraph {
    pub shares_bps: Vec<(ContributorRole, ActorId, u16)>,
}

impl AttributionGraph {
    pub fn new(shares_bps: Vec<(ContributorRole, ActorId, u16)>) -> Result<Self, ClaimError> {
        let graph = Self { shares_bps };
        graph.validate()?;
        Ok(graph)
    }

    /// Synthesize integer `shares_bps` across hop-attenuated KU/Mapping/Assembly authors,
    /// Track A encoders/verifiers, and `ProviderLease` servers, rejecting DAG cycles.
    pub fn from_causal_lineage(
        nodes: &[DerivationContributorNode],
        hop_attenuation_bps: u32,
    ) -> Result<Self, ClaimError> {
        if nodes.is_empty() {
            return Err(ClaimError::EmptyAttributionGraph);
        }
        if hop_attenuation_bps > 10_000 {
            return Err(ClaimError::Policy(PolicyError::InvalidHopAttenuation {
                bps: hop_attenuation_bps,
            }));
        }

        // Detect self-loops and cycles in the Derivation DAG.
        let mut adj: BTreeMap<[u8; 32], Vec<[u8; 32]>> = BTreeMap::new();
        for node in nodes {
            let child = *node.object_cid.as_bytes();
            let parents: Vec<[u8; 32]> =
                node.parent_objects.iter().map(|p| *p.as_bytes()).collect();
            if parents.contains(&child) {
                return Err(ClaimError::DerivationCycleDetected);
            }
            adj.entry(child).or_default().extend(parents);
        }
        if has_dag_cycle(&adj) {
            return Err(ClaimError::DerivationCycleDetected);
        }

        // Compute attenuated integer score per contributor using exact rational attenuation.
        let mut raw_scores: BTreeMap<(ContributorRole, ActorId), u128> = BTreeMap::new();
        let scale: u128 = 1_000_000_000;
        for node in nodes {
            if node.base_weight_units == 0 {
                continue;
            }
            let mut factor = scale;
            for _ in 0..node.hop_distance {
                factor = (factor * u128::from(hop_attenuation_bps)) / BPS_DENOMINATOR;
            }
            let weighted = u128::from(node.base_weight_units) * factor;
            if weighted > 0 {
                let entry = raw_scores.entry((node.role, node.actor)).or_insert(0);
                *entry = entry
                    .checked_add(weighted)
                    .ok_or(ClaimError::Math(ObtMathError::AdditionOverflow))?;
            }
        }

        let total_raw: u128 = raw_scores.values().copied().sum();
        if total_raw == 0 {
            return Err(ClaimError::EmptyAttributionGraph);
        }

        let mut shares_bps = Vec::with_capacity(raw_scores.len());
        for ((role, actor), score) in raw_scores {
            let bps = ((score * BPS_DENOMINATOR) / total_raw) as u16;
            if bps > 0 {
                shares_bps.push((role, actor, bps));
            }
        }
        Self::new(shares_bps)
    }

    pub fn total_shares_bps(&self) -> Result<u32, ClaimError> {
        let mut sum: u32 = 0;
        for (_, _, bps) in &self.shares_bps {
            if *bps == 0 {
                return Err(ClaimError::ZeroShareEntry);
            }
            sum = sum.checked_add(u32::from(*bps)).ok_or(ClaimError::Math(
                ObtMathError::InvalidShareTotal {
                    total_bps: u32::MAX,
                },
            ))?;
        }
        if sum > BPS_DENOMINATOR as u32 {
            return Err(ClaimError::Math(ObtMathError::InvalidShareTotal {
                total_bps: sum,
            }));
        }
        Ok(sum)
    }

    pub fn validate(&self) -> Result<(), ClaimError> {
        if self.shares_bps.is_empty() {
            return Err(ClaimError::EmptyAttributionGraph);
        }
        let _ = self.total_shares_bps()?;
        Ok(())
    }

    pub fn distribute(&self, budget: ObtAmount) -> Result<BenefitDistribution, ClaimError> {
        self.validate()?;
        let mut payouts = Vec::with_capacity(self.shares_bps.len());
        let mut total_allocated = ObtAmount::ZERO;
        for &(role, beneficiary, share_bps) in &self.shares_bps {
            let (amount, _rem) = budget.mul_bps_floor(u32::from(share_bps))?;
            total_allocated = total_allocated.checked_add(amount)?;
            payouts.push(AttributedPayout {
                role,
                beneficiary,
                share_bps,
                amount,
            });
        }
        let unminted_dust = budget.checked_sub(total_allocated)?;
        Ok(BenefitDistribution {
            total_budget: budget,
            payouts,
            total_allocated,
            unminted_dust,
        })
    }
}

fn has_dag_cycle(adj: &BTreeMap<[u8; 32], Vec<[u8; 32]>>) -> bool {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Color {
        Visiting,
        Done,
    }
    fn dfs(
        node: [u8; 32],
        adj: &BTreeMap<[u8; 32], Vec<[u8; 32]>>,
        state: &mut BTreeMap<[u8; 32], Color>,
    ) -> bool {
        match state.get(&node) {
            Some(Color::Visiting) => return true,
            Some(Color::Done) => return false,
            None => {}
        }
        state.insert(node, Color::Visiting);
        if let Some(neighbors) = adj.get(&node) {
            for &next in neighbors {
                if dfs(next, adj, state) {
                    return true;
                }
            }
        }
        state.insert(node, Color::Done);
        false
    }

    let mut state = BTreeMap::new();
    for &node in adj.keys() {
        if dfs(node, adj, &mut state) {
            return true;
        }
    }
    false
}

/// Convergence state of a `BenefitClosureView` (`P4 §4.2`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BenefitClosureState {
    SupportedUncontested,
    ContestedOrMixed,
    DeferredIncomplete,
    ObservedNonBeneficial,
}

/// Causal `UseEvidence` or `DerivationEvidence` hop cited by a `BenefitRewardClaimV1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UseOrDerivationHop {
    pub event_cid: EventCid,
    pub task_context_commitment: [u8; 32],
    pub input_object_cids: Vec<ObjectCid>,
    pub derived_output_cid: Option<ObjectCid>,
}

/// Summarized `OutcomeObservation` cited by a `BenefitRewardClaimV1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutcomeObservationSummary {
    pub observation_object_cid: ObjectCid,
    pub task_context_commitment: [u8; 32],
    pub valence: OutcomeValence,
    pub has_measurement_evidence: bool,
    pub has_baseline: bool,
    pub limitations: Vec<EvidenceLimitation>,
}

/// Track B (`M7`) attributed benefit reward claim (`§4.2`, `NEG-P5-003`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BenefitRewardClaimV1 {
    pub is_bare_ku_creation: bool,
    pub contains_exposure_or_rag_telemetry: bool,
    pub benefit_event_cid: Option<EventCid>,
    pub outcome_case_id: [u8; 32],
    pub task_context_commitment: [u8; 32],
    pub outcome_class: ConceptCcid,
    pub use_or_derivation_hops: Vec<UseOrDerivationHop>,
    pub outcome_observations: Vec<OutcomeObservationSummary>,
    pub closure_state: BenefitClosureState,
    pub assessed_valence: OutcomeValence,
    pub attribution_status: AttributionStatus,
    pub has_counterfactual_evidence: bool,
    pub gain_bucket: u16,
    pub causal_tier: CausalSupportTier,
    pub witness_correlations: Vec<CorrelationEvidence>,
    pub attribution_graph: AttributionGraph,
    pub epoch_index: u64,
    pub reward_policy_cid: ObjectCid,
    pub evidence_frontier_root: [u8; 32],
}

impl BenefitRewardClaimV1 {
    pub fn nullifier(&self) -> ScopedNullifier {
        ScopedNullifier::for_benefit(
            self.outcome_case_id,
            self.task_context_commitment,
            self.reward_policy_cid,
        )
    }

    pub fn claim_cid(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(BENEFIT_CLAIM_DOMAIN);
        if let Some(ev) = self.benefit_event_cid {
            hasher.update(&[1]);
            hasher.update(ev.as_bytes());
        } else {
            hasher.update(&[0]);
        }
        hasher.update(&self.outcome_case_id);
        hasher.update(&self.task_context_commitment);
        hasher.update(self.outcome_class.as_bytes());
        hasher.update(&self.epoch_index.to_be_bytes());
        hasher.update(&self.gain_bucket.to_be_bytes());
        hasher.update(self.reward_policy_cid.as_bytes());
        hasher.update(&self.evidence_frontier_root);
        for &(role, actor, bps) in &self.attribution_graph.shares_bps {
            hasher.update(&[role as u8]);
            hasher.update(actor.as_bytes());
            hasher.update(&bps.to_be_bytes());
        }
        *hasher.finalize().as_bytes()
    }
}

/// Errors produced when validating or evaluating Track A / Track B claims.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ClaimError {
    #[error("OBT_CLAIM_MATH: {0}")]
    Math(#[from] ObtMathError),
    #[error("OBT_CLAIM_POLICY: {0}")]
    Policy(#[from] PolicyError),
    #[error("OBT_CLAIM_POLICY_MISMATCH: claim policy_cid does not match active RewardPolicyV1")]
    PolicyCidMismatch,
    #[error("OBT_CLAIM_NULLIFIER_REPLAY: nullifier {0} already consumed")]
    NullifierReplay(ScopedNullifier),
    #[error("OBT_CLAIM_INVALID_JOB_PROOF: requester-authorized DelegatedWorkJobProof invalid")]
    InvalidDelegatedJobProof,
    #[error("OBT_CLAIM_INVALID_BLIND_PROOF: commit-before-reveal proof invalid")]
    InvalidBlindCommitReveal,
    #[error("OBT_CLAIM_EMPTY_MISMATCH_DIGEST: EvidencedSemanticMismatch requires non-zero span_quote_digest")]
    EmptyMismatchEvidenceDigest,
    #[error("OBT_CLAIM_ZERO_STRUCTURAL_WORK: verified statements and constraints are both zero")]
    ZeroStructuralWork,
    #[error("OBT_CLAIM_WORKER_CORRELATION_UNPROVEN: worker 4D CorrelationEvidence lacks CryptoBound/ExternallyAttested commitments")]
    UnprovenWorkerCorrelation,
    #[error("OBT_CLAIM_INSUFFICIENT_VERIFIER_GROUPS: found {found} distinct 4D verifier groups, need >= {required}")]
    InsufficientDistinctVerifierGroups { found: usize, required: usize },
    #[error("OBT_CLAIM_ACTOR_EPOCH_CAP_EXCEEDED: actor epoch cap exceeded in epoch {epoch}")]
    ActorEpochCapExceeded { epoch: u64, actor: ActorId },
    #[error(
        "OBT_CLAIM_GROUP_EPOCH_CAP_EXCEEDED: correlation group epoch cap exceeded in epoch {epoch}"
    )]
    CorrelationGroupEpochCapExceeded { epoch: u64, group: [u8; 32] },
    #[error("NEG-P5-003_BARE_KU_CREATION: bare KU creation cannot mint OBT")]
    BareKuCreationForbidden,
    #[error("NEG-P5-003_EXPOSURE_TELEMETRY: query/RAG exposure telemetry cannot mint OBT")]
    ExposureTelemetryForbidden,
    #[error("NEG-P5-003_STANDALONE_USE_WITHOUT_OUTCOME: Use/Derivation without OutcomeObservation cannot mint OBT")]
    StandaloneUseWithoutOutcome,
    #[error(
        "NEG-P5-003_MISSING_USE_OR_BENEFIT_EVENT: BenefitEvidence requires Use/Derivation linkage"
    )]
    MissingUseOrBenefitLinkage,
    #[error("NEG-P5-003_CONTESTED_OR_HARMFUL_CLOSURE: contested, opposed, or harmful closure cannot mint OBT")]
    ContestedOrHarmfulClosure,
    #[error("NEG-P5-003_BROKEN_TASK_CONTEXT_CONTINUITY: task_context_commitment continuity broken across Use/Outcome/Benefit")]
    BrokenTaskContextContinuity,
    #[error("OBT_CLAIM_DERIVATION_CYCLE: cycle detected in Derivation DAG")]
    DerivationCycleDetected,
    #[error("OBT_CLAIM_EMPTY_ATTRIBUTION: AttributionGraph has no positive shares")]
    EmptyAttributionGraph,
    #[error("OBT_CLAIM_ZERO_SHARE: AttributionGraph contains 0 bps share entry")]
    ZeroShareEntry,
    #[error("OBT_CLAIM_UNKNOWN_GAIN_BUCKET: gain_bucket {0} missing from policy lookup table")]
    UnknownGainBucket(u16),
    #[error("OBT_CLAIM_ZERO_PAYOUT_BUDGET: computed reward budget is zero")]
    ZeroRewardBudget,
}

/// Epoch issuance ceiling tracker for per-`ActorId` and per-4D-correlation-group caps (`§4.1.3`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EpochCapTracker {
    pub actor_totals: BTreeMap<(u64, ActorId), ObtAmount>,
    pub group_totals: BTreeMap<(u64, [u8; 32]), ObtAmount>,
    pub consumed_nullifiers: BTreeSet<ScopedNullifier>,
}

impl EpochCapTracker {
    pub fn actor_issued(&self, epoch: u64, actor: ActorId) -> ObtAmount {
        self.actor_totals
            .get(&(epoch, actor))
            .copied()
            .unwrap_or(ObtAmount::ZERO)
    }

    pub fn group_issued(&self, epoch: u64, group: &[u8; 32]) -> ObtAmount {
        self.group_totals
            .get(&(epoch, *group))
            .copied()
            .unwrap_or(ObtAmount::ZERO)
    }

    pub fn is_nullifier_consumed(&self, nullifier: &ScopedNullifier) -> bool {
        self.consumed_nullifiers.contains(nullifier)
    }
}

/// Approved evaluation outcome produced by `ClaimEvaluator`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaimEvaluationOutcome {
    WorkApproved {
        claim_cid: [u8; 32],
        nullifier: ScopedNullifier,
        beneficiary: ActorId,
        amount: ObtAmount,
        distinct_verifier_groups: usize,
    },
    BenefitApproved {
        claim_cid: [u8; 32],
        nullifier: ScopedNullifier,
        distribution: BenefitDistribution,
        distinct_witness_groups: usize,
    },
}

/// Stateless/stateful evaluator for Track A and Track B claims.
pub struct ClaimEvaluator;

impl ClaimEvaluator {
    /// Pure complexity-bounded Track A reward computation (`D-014` Evidenced-Mismatch Symmetry).
    ///
    /// Notice that `WorkVerdict::FaithfulAlignment` and `WorkVerdict::EvidencedSemanticMismatch`
    /// produce the exact same `ObtAmount` for identical `(work_class, verified_statement_count, resolved_constraint_count)`.
    pub fn compute_work_amount(
        claim: &WorkRewardClaimV1,
        policy: &RewardPolicyV1,
    ) -> Result<ObtAmount, ClaimError> {
        policy.validate()?;
        claim.verdict.validate()?;
        let stmts = claim
            .verified_statement_count
            .min(MAX_VERIFIED_STATEMENTS_CAP);
        let constraints = claim
            .resolved_constraint_count
            .min(MAX_RESOLVED_CONSTRAINTS_CAP);
        if stmts == 0 && constraints == 0 {
            return Err(ClaimError::ZeroStructuralWork);
        }
        let weights = policy.work_weights(claim.work_class);
        let stmt_part = weights
            .per_statement_quanta
            .0
            .checked_mul(u128::from(stmts))
            .map(ObtAmount)
            .ok_or(ObtMathError::AdditionOverflow)?;
        let constraint_part = weights
            .per_constraint_quanta
            .0
            .checked_mul(u128::from(constraints))
            .map(ObtAmount)
            .ok_or(ObtMathError::AdditionOverflow)?;
        let total = weights
            .base_quanta
            .checked_add(stmt_part)?
            .checked_add(constraint_part)?;
        if total.is_zero() {
            return Err(ClaimError::ZeroRewardBudget);
        }
        Ok(total)
    }

    /// Evaluate and commit a Track A `WorkRewardClaimV1` against `EpochCapTracker`.
    pub fn evaluate_work_claim(
        claim: &WorkRewardClaimV1,
        policy: &RewardPolicyV1,
        tracker: &mut EpochCapTracker,
    ) -> Result<ClaimEvaluationOutcome, ClaimError> {
        policy.validate()?;
        if claim.reward_policy_cid != policy.policy_cid() {
            return Err(ClaimError::PolicyCidMismatch);
        }
        if claim.evidence_frontier_root == [0u8; 32] {
            return Err(ClaimError::InvalidDelegatedJobProof);
        }
        claim.job_proof.validate()?;
        claim.blind_proof.validate()?;
        claim.verdict.validate()?;

        let nullifier = claim.nullifier();
        if tracker.is_nullifier_consumed(&nullifier) {
            return Err(ClaimError::NullifierReplay(nullifier));
        }

        let worker_group = extract_4d_correlation_group(&claim.worker_correlation)
            .ok_or(ClaimError::UnprovenWorkerCorrelation)?;

        // Gather verifier attestations matching the claim's verdict and distinct from the worker.
        let mut verifier_groups = Vec::new();
        for att in &claim.verifier_attestations {
            if att.verifier_actor_id == claim.worker_actor_id || att.verdict != claim.verdict {
                continue;
            }
            if let Some(group) = extract_4d_correlation_group(&att.correlation_evidence) {
                if group.admin_commitment != worker_group.admin_commitment {
                    verifier_groups.push(group);
                }
            }
        }
        let distinct_verifier_groups = count_distinct_4d_groups(&verifier_groups);
        let required_groups = usize::from(policy.minimum_work_verifier_groups);
        if distinct_verifier_groups < required_groups {
            return Err(ClaimError::InsufficientDistinctVerifierGroups {
                found: distinct_verifier_groups,
                required: required_groups,
            });
        }

        let amount = Self::compute_work_amount(claim, policy)?;

        // Check per-actor and per-correlation-group epoch ceilings before mutating tracker.
        let next_actor = tracker
            .actor_issued(claim.epoch_index, claim.worker_actor_id)
            .checked_add(amount)?;
        if next_actor > policy.per_actor_epoch_ceiling {
            return Err(ClaimError::ActorEpochCapExceeded {
                epoch: claim.epoch_index,
                actor: claim.worker_actor_id,
            });
        }
        let next_group = tracker
            .group_issued(claim.epoch_index, worker_group.as_bytes())
            .checked_add(amount)?;
        if next_group > policy.per_correlation_group_epoch_ceiling {
            return Err(ClaimError::CorrelationGroupEpochCapExceeded {
                epoch: claim.epoch_index,
                group: *worker_group.as_bytes(),
            });
        }

        tracker.consumed_nullifiers.insert(nullifier);
        tracker
            .actor_totals
            .insert((claim.epoch_index, claim.worker_actor_id), next_actor);
        tracker
            .group_totals
            .insert((claim.epoch_index, *worker_group.as_bytes()), next_group);

        Ok(ClaimEvaluationOutcome::WorkApproved {
            claim_cid: claim.claim_cid(),
            nullifier,
            beneficiary: claim.worker_actor_id,
            amount,
            distinct_verifier_groups,
        })
    }

    /// Evaluate and commit a Track B `BenefitRewardClaimV1` (`M7`, `NEG-P5-003`).
    pub fn evaluate_benefit_claim(
        claim: &BenefitRewardClaimV1,
        policy: &RewardPolicyV1,
        tracker: &mut EpochCapTracker,
    ) -> Result<ClaimEvaluationOutcome, ClaimError> {
        policy.validate()?;
        if claim.reward_policy_cid != policy.policy_cid() {
            return Err(ClaimError::PolicyCidMismatch);
        }

        // Hard rejections per NEG-P5-003:
        if claim.is_bare_ku_creation {
            return Err(ClaimError::BareKuCreationForbidden);
        }
        if claim.contains_exposure_or_rag_telemetry {
            return Err(ClaimError::ExposureTelemetryForbidden);
        }
        if claim.outcome_observations.is_empty() {
            return Err(ClaimError::StandaloneUseWithoutOutcome);
        }
        if claim.use_or_derivation_hops.is_empty() || claim.benefit_event_cid.is_none() {
            return Err(ClaimError::MissingUseOrBenefitLinkage);
        }
        if claim.closure_state != BenefitClosureState::SupportedUncontested
            || claim.assessed_valence != OutcomeValence::Beneficial
            || claim.attribution_status != AttributionStatus::Supported
            || claim
                .outcome_observations
                .iter()
                .any(|obs| obs.valence != OutcomeValence::Beneficial)
        {
            return Err(ClaimError::ContestedOrHarmfulClosure);
        }

        // Enforce continuous non-zero task_context_commitment across Use/Derivation -> Outcome -> Benefit.
        if claim.task_context_commitment == [0u8; 32]
            || claim.outcome_case_id == [0u8; 32]
            || claim.evidence_frontier_root == [0u8; 32]
            || claim
                .use_or_derivation_hops
                .iter()
                .any(|hop| hop.task_context_commitment != claim.task_context_commitment)
            || claim
                .outcome_observations
                .iter()
                .any(|obs| obs.task_context_commitment != claim.task_context_commitment)
        {
            return Err(ClaimError::BrokenTaskContextContinuity);
        }

        // Reject cycles in Use/Derivation hops.
        let mut hop_adj: BTreeMap<[u8; 32], Vec<[u8; 32]>> = BTreeMap::new();
        for hop in &claim.use_or_derivation_hops {
            if let Some(out) = hop.derived_output_cid {
                let out_bytes = *out.as_bytes();
                for inp in &hop.input_object_cids {
                    if *inp.as_bytes() == out_bytes {
                        return Err(ClaimError::DerivationCycleDetected);
                    }
                    hop_adj.entry(out_bytes).or_default().push(*inp.as_bytes());
                }
            }
        }
        if has_dag_cycle(&hop_adj) {
            return Err(ClaimError::DerivationCycleDetected);
        }

        let nullifier = claim.nullifier();
        if tracker.is_nullifier_consumed(&nullifier) {
            return Err(ClaimError::NullifierReplay(nullifier));
        }

        // Derive discrete evidence & corroboration tiers.
        let witness_groups: Vec<CorrelationGroupId> = claim
            .witness_correlations
            .iter()
            .filter_map(extract_4d_correlation_group)
            .collect();
        let distinct_witness_groups = count_distinct_4d_groups(&witness_groups);

        let is_self_reported_or_unwitnessed = distinct_witness_groups == 0
            || claim.outcome_observations.iter().any(|obs| {
                !obs.has_measurement_evidence
                    || obs.limitations.contains(&EvidenceLimitation::SelfReported)
                    || obs
                        .limitations
                        .contains(&EvidenceLimitation::UnwitnessedObservation)
            });

        let evidence_tier = if is_self_reported_or_unwitnessed {
            EvidenceQualityTier::SelfReportedOrUnwitnessed
        } else if claim.has_counterfactual_evidence {
            EvidenceQualityTier::CounterfactualSupported
        } else if claim
            .outcome_observations
            .iter()
            .all(|obs| obs.has_measurement_evidence && obs.has_baseline)
            && claim.outcome_observations.len() >= 2
        {
            EvidenceQualityTier::MultiMeasurementWithBaseline
        } else {
            EvidenceQualityTier::SingleWitnessed
        };

        let corroboration_tier = match distinct_witness_groups {
            0 => CorroborationTier::Uncorroborated,
            1 => CorroborationTier::SingleGroup,
            2 => CorroborationTier::TwoDistinctGroups,
            _ => CorroborationTier::ThreePlusDistinctGroups,
        };

        let g_norm_bps = *policy
            .gain_norm_bps_table
            .get(&claim.gain_bucket)
            .ok_or(ClaimError::UnknownGainBucket(claim.gain_bucket))?;
        let f_evidence_bps = policy.evidence_bps_table[&evidence_tier];
        let f_causal_bps = policy.causal_bps_table[&claim.causal_tier];
        let f_corr_bps = policy.corroboration_bps_table[&corroboration_tier];

        let base = policy.base_budget_for(claim.outcome_class);
        let mut budget =
            compute_benefit_budget(base, g_norm_bps, f_evidence_bps, f_causal_bps, f_corr_bps)?;
        if is_self_reported_or_unwitnessed && budget > policy.self_reported_micro_ceiling {
            budget = policy.self_reported_micro_ceiling;
        }
        if budget.is_zero() {
            return Err(ClaimError::ZeroRewardBudget);
        }

        let distribution = claim.attribution_graph.distribute(budget)?;

        // Stage per-beneficiary epoch cap updates atomically before mutating tracker.
        let mut staged_actor_totals = tracker.actor_totals.clone();
        for payout in &distribution.payouts {
            if payout.amount.is_zero() {
                continue;
            }
            let current = staged_actor_totals
                .get(&(claim.epoch_index, payout.beneficiary))
                .copied()
                .unwrap_or(ObtAmount::ZERO);
            let next = current.checked_add(payout.amount)?;
            if next > policy.per_actor_epoch_ceiling {
                return Err(ClaimError::ActorEpochCapExceeded {
                    epoch: claim.epoch_index,
                    actor: payout.beneficiary,
                });
            }
            staged_actor_totals.insert((claim.epoch_index, payout.beneficiary), next);
        }

        tracker.consumed_nullifiers.insert(nullifier);
        tracker.actor_totals = staged_actor_totals;

        Ok(ClaimEvaluationOutcome::BenefitApproved {
            claim_cid: claim.claim_cid(),
            nullifier,
            distribution,
            distinct_witness_groups,
        })
    }
}
