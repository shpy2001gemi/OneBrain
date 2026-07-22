//! Offline-first Local Knowledge Companion planning boundary.
//!
//! Context becomes private NeedIR/StandingNeeds and bounded recommendations.
//! The companion performs no disclosure, network send, materialization or
//! publication side effect.

use ku_core::foundation::{
    encode_canonical, Budget, CanonicalValue, ConceptCcid, DisclosureClass, EventCid, ObjectCid,
    ObjectReference, ResourceProfile, Selector, SelectorCid, SemanticFrameSet,
};
use ku_kql::vnext_disclosure::{
    DisclosureConsent, DisclosureError, DisclosureMode, DisclosurePolicy,
};
use ku_kql::vnext_multipath::MultipathQueryPlan;
use ku_kql::vnext_proposal::{BindingProposal, ProposalDisposition, ProposalError, ProposalId};
use ku_kql::vnext_query::{KnowledgeNeedIr, QueryContractError, QueryDefinition};
use ku_kql::vnext_standing_need::{StandingNeed, StandingNeedError};

pub const MAX_COMPANION_RECOMMENDATIONS: usize = 4_096;
pub const MAX_COMPANION_PROVENANCE_EVENTS: usize = 16_384;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompanionContext {
    pub receptor_definitions: Vec<ObjectReference>,
    pub desired_roles: Vec<ConceptCcid>,
    pub goal: SemanticFrameSet,
    pub local_context: SemanticFrameSet,
    pub observation_events: Vec<EventCid>,
    pub observation_proposal_ids: Vec<[u8; 32]>,
    pub observed_frontier: [u8; 32],
}

impl CompanionContext {
    fn validate(&self) -> Result<(), CompanionError> {
        if self.receptor_definitions.is_empty()
            || self.desired_roles.is_empty()
            || self.observed_frontier == [0; 32]
            || self.observation_events.len() > MAX_COMPANION_PROVENANCE_EVENTS
            || self.observation_proposal_ids.len() > MAX_COMPANION_PROVENANCE_EVENTS
            || self
                .observation_events
                .iter()
                .any(|event| event.as_bytes() == &[0; 32])
            || self
                .observation_proposal_ids
                .iter()
                .any(|proposal| proposal == &[0; 32])
        {
            return Err(CompanionError::InvalidContext);
        }
        KnowledgeNeedIr {
            receptor_definitions: self.receptor_definitions.clone(),
            desired_roles: self.desired_roles.clone(),
            goal: self.goal.clone(),
            local_context: self.local_context.clone(),
            privacy: DisclosureClass::LocalOnly,
        }
        .validate()?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalCompanionPolicy {
    pub policy_ref: ObjectReference,
    pub query_policy: ObjectReference,
    pub exploration_policy: ObjectReference,
    pub standing_need_watch_policy: ObjectReference,
    pub materialization_policy: ObjectReference,
    pub disclosure_policy: DisclosurePolicy,
    pub route_purpose: ConceptCcid,
    pub share_purpose: ConceptCcid,
    pub share_mode: DisclosureMode,
    pub selector: Selector,
    pub max_recommendations: usize,
}

impl LocalCompanionPolicy {
    fn validate(&self) -> Result<(), CompanionError> {
        self.selector.validate()?;
        if self.policy_ref.cid == [0; 32]
            || self.query_policy.cid == [0; 32]
            || self.exploration_policy.cid == [0; 32]
            || self.standing_need_watch_policy.cid == [0; 32]
            || self.materialization_policy.cid == [0; 32]
            || self.max_recommendations == 0
            || self.max_recommendations > MAX_COMPANION_RECOMMENDATIONS
            || !matches!(
                self.share_mode,
                DisclosureMode::NegotiatedEncrypted | DisclosureMode::PublicProblem
            )
        {
            return Err(CompanionError::InvalidPolicy);
        }
        if self.disclosure_policy.policy_ref != self.policy_ref {
            return Err(CompanionError::InvalidPolicy);
        }
        self.disclosure_policy
            .authorize(DisclosureMode::LocalOnly, None, 0)
            .map_err(|_| CompanionError::InvalidPolicy)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompanionShareGrant {
    pub subject: ObjectReference,
    pub consent: DisclosureConsent,
}

#[derive(Clone, Debug, Default)]
pub struct CompanionDisclosureGrants {
    pub route: Option<DisclosureConsent>,
    pub shares: Vec<CompanionShareGrant>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompanionMaterializationCandidate {
    proposal_id: ProposalId,
    disposition: ProposalDisposition,
}

impl CompanionMaterializationCandidate {
    pub fn from_proposal(
        proposal: &BindingProposal,
        current_evaluation: u64,
    ) -> Result<Self, CompanionError> {
        proposal.validate()?;
        Ok(Self {
            proposal_id: proposal.proposal_id()?,
            disposition: proposal.disposition(current_evaluation),
        })
    }

    pub const fn proposal_id(&self) -> ProposalId {
        self.proposal_id
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompanionOpportunities {
    pub materialization_candidates: Vec<CompanionMaterializationCandidate>,
    pub share_candidates: Vec<ObjectReference>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecommendationGateStatus {
    LocalReadOnly,
    ReadyForExplicitExecutor,
    ConsentRequired,
    ConsentInvalidOrExpired,
    PolicyDisabled,
    ExplicitMaterializationAuthorityRequired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecommendationGuard {
    pub policy_ref: ObjectReference,
    pub consent_commitment: Option<[u8; 32]>,
    pub status: RecommendationGateStatus,
    pub explicit_executor_required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompanionRecommendationKind {
    LocalFetch {
        query_definition: ObjectCid,
        selector: SelectorCid,
        budget: Budget,
    },
    NetworkFetch {
        plan: Option<MultipathQueryPlan>,
        plan_commitment: Option<[u8; 32]>,
        selector: SelectorCid,
        budget: Budget,
    },
    ShareKnowledge {
        subject: ObjectReference,
        mode: DisclosureMode,
    },
    MaterializeMapping {
        proposal_id: ProposalId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompanionRecommendation {
    pub recommendation_id: [u8; 32],
    pub kind: CompanionRecommendationKind,
    pub guard: RecommendationGuard,
}

impl CompanionRecommendation {
    pub const fn performs_side_effect(&self) -> bool {
        false
    }

    pub const fn is_authority_record(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompanionNetworkStatus {
    NotConfigured,
    BlockedByPolicyOrConsent,
    ProposalCompiled,
}

#[derive(Clone, Copy, Debug)]
pub struct CompanionMultipathRequest {
    pub query_definition: ObjectCid,
    pub selector: SelectorCid,
    pub budget: Budget,
    pub route_scope_commitment: [u8; 32],
    pub consent_commitment: [u8; 32],
}

/// A local compiler only. Returning a plan cannot send any packet.
pub trait OptionalCompanionMultipathPlanner {
    fn compile_proposal(
        &mut self,
        request: CompanionMultipathRequest,
    ) -> Result<MultipathQueryPlan, CompanionError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompanionPlan {
    pub private_query: QueryDefinition,
    pub query_definition_cid: ObjectCid,
    pub standing_needs: Vec<StandingNeed>,
    pub recommendations: Vec<CompanionRecommendation>,
    pub network_status: CompanionNetworkStatus,
    pub context_provenance_root: [u8; 32],
}

impl CompanionPlan {
    pub const fn operates_offline(&self) -> bool {
        true
    }

    pub fn performs_side_effects(&self) -> bool {
        self.recommendations
            .iter()
            .any(CompanionRecommendation::performs_side_effect)
    }

    pub const fn can_publish_or_materialize(&self) -> bool {
        false
    }
}

pub struct LocalKnowledgeCompanion;

impl LocalKnowledgeCompanion {
    #[allow(clippy::too_many_arguments)]
    pub fn plan(
        context: CompanionContext,
        policy: LocalCompanionPolicy,
        grants: CompanionDisclosureGrants,
        opportunities: CompanionOpportunities,
        local_tick: u64,
        mut multipath: Option<&mut dyn OptionalCompanionMultipathPlanner>,
    ) -> Result<CompanionPlan, CompanionError> {
        context.validate()?;
        policy.validate()?;
        validate_opportunities(&opportunities)?;
        let selector = policy.selector.cid()?;
        let private_query = QueryDefinition {
            need: KnowledgeNeedIr {
                receptor_definitions: context.receptor_definitions.clone(),
                desired_roles: context.desired_roles.clone(),
                goal: context.goal.clone(),
                local_context: context.local_context.clone(),
                privacy: DisclosureClass::LocalOnly,
            },
            query_policy: policy.query_policy.clone(),
            exploration_policy: policy.exploration_policy.clone(),
        };
        let query_definition_cid = private_query.private_cid()?;
        let mut standing_needs = context
            .receptor_definitions
            .iter()
            .map(|receptor| {
                StandingNeed::new_local(
                    receptor.clone(),
                    query_definition_cid,
                    selector,
                    policy.standing_need_watch_policy.clone(),
                    context.observed_frontier,
                )
            })
            .collect::<Vec<_>>();
        standing_needs.sort_by_key(|need| {
            (
                need.receptor_definition.reference_kind,
                need.receptor_definition.cid,
            )
        });
        for need in &standing_needs {
            need.validate()?;
        }

        let context_provenance_root = context_root(&context)?;
        let mut recommendations = Vec::new();
        push_recommendation(
            &mut recommendations,
            policy.max_recommendations,
            CompanionRecommendationKind::LocalFetch {
                query_definition: query_definition_cid,
                selector,
                budget: policy.selector.budget,
            },
            RecommendationGuard {
                policy_ref: policy.query_policy.clone(),
                consent_commitment: None,
                status: RecommendationGateStatus::LocalReadOnly,
                explicit_executor_required: false,
            },
        );

        let route_scope =
            companion_disclosure_scope(&context, selector, DisclosureMode::RouteMinimal, None)?;
        let route_guard = disclosure_guard(
            &policy,
            DisclosureMode::RouteMinimal,
            policy.route_purpose,
            route_scope,
            grants.route.as_ref(),
            local_tick,
        );
        let mut network_status = CompanionNetworkStatus::NotConfigured;
        if let Some(planner) = multipath.as_mut() {
            let (plan, plan_commitment) =
                if route_guard.status == RecommendationGateStatus::ReadyForExplicitExecutor {
                    let consent = grants.route.as_ref().expect("ready guard has consent");
                    let plan = planner.compile_proposal(CompanionMultipathRequest {
                        query_definition: query_definition_cid,
                        selector,
                        budget: policy.selector.budget,
                        route_scope_commitment: route_scope,
                        consent_commitment: consent.consent_commitment,
                    })?;
                    let commitment = multipath_plan_commitment(&plan);
                    network_status = CompanionNetworkStatus::ProposalCompiled;
                    (Some(plan), Some(commitment))
                } else {
                    network_status = CompanionNetworkStatus::BlockedByPolicyOrConsent;
                    (None, None)
                };
            push_recommendation(
                &mut recommendations,
                policy.max_recommendations,
                CompanionRecommendationKind::NetworkFetch {
                    plan,
                    plan_commitment,
                    selector,
                    budget: policy.selector.budget,
                },
                route_guard,
            );
        }

        let mut materializations = opportunities.materialization_candidates;
        materializations.sort_by_key(|candidate| *candidate.proposal_id.as_bytes());
        materializations.dedup_by_key(|candidate| *candidate.proposal_id.as_bytes());
        for candidate in materializations {
            if candidate.disposition != ProposalDisposition::CandidateOnly {
                continue;
            }
            push_recommendation(
                &mut recommendations,
                policy.max_recommendations,
                CompanionRecommendationKind::MaterializeMapping {
                    proposal_id: candidate.proposal_id,
                },
                RecommendationGuard {
                    policy_ref: policy.materialization_policy.clone(),
                    consent_commitment: None,
                    status: RecommendationGateStatus::ExplicitMaterializationAuthorityRequired,
                    explicit_executor_required: true,
                },
            );
        }

        let mut shares = opportunities.share_candidates;
        shares.sort_by_key(reference_key);
        shares.dedup_by_key(|reference| reference_key(reference));
        for subject in shares {
            let share_scope =
                companion_disclosure_scope(&context, selector, policy.share_mode, Some(&subject))?;
            let consent = grants
                .shares
                .iter()
                .find(|grant| grant.subject == subject)
                .map(|grant| &grant.consent);
            let guard = disclosure_guard(
                &policy,
                policy.share_mode,
                policy.share_purpose,
                share_scope,
                consent,
                local_tick,
            );
            push_recommendation(
                &mut recommendations,
                policy.max_recommendations,
                CompanionRecommendationKind::ShareKnowledge {
                    subject,
                    mode: policy.share_mode,
                },
                guard,
            );
        }
        Ok(CompanionPlan {
            private_query,
            query_definition_cid,
            standing_needs,
            recommendations,
            network_status,
            context_provenance_root,
        })
    }
}

pub fn companion_disclosure_scope(
    context: &CompanionContext,
    selector: SelectorCid,
    mode: DisclosureMode,
    subject: Option<&ObjectReference>,
) -> Result<[u8; 32], CompanionError> {
    context.validate()?;
    let mut fields = vec![
        (0, CanonicalValue::Unsigned(1)),
        (1, CanonicalValue::Bytes(context_root(context)?.to_vec())),
        (2, CanonicalValue::Bytes(selector.as_bytes().to_vec())),
        (3, CanonicalValue::Unsigned(mode as u64)),
    ];
    if let Some(subject) = subject {
        fields.push((4, reference_value(subject)));
    }
    let bytes = encode_canonical(&CanonicalValue::Map(fields), ResourceProfile::ControlV1)?;
    Ok(digest_bytes(b"disclosure-scope", &bytes))
}

fn validate_opportunities(opportunities: &CompanionOpportunities) -> Result<(), CompanionError> {
    if opportunities.share_candidates.iter().any(|reference| {
        reference.cid == [0; 32]
            || matches!(
                reference.reference_kind,
                kind if kind == ku_core::foundation::SOURCE_ARTIFACT_KIND.0
                    || kind == ku_core::foundation::OBSERVATION_EVENT_PAYLOAD_KIND.0
            )
    }) {
        return Err(CompanionError::RawObservationShareForbidden);
    }
    Ok(())
}

fn disclosure_guard(
    policy: &LocalCompanionPolicy,
    mode: DisclosureMode,
    expected_purpose: ConceptCcid,
    expected_scope: [u8; 32],
    consent: Option<&DisclosureConsent>,
    local_tick: u64,
) -> RecommendationGuard {
    let status = match consent {
        None => match policy.disclosure_policy.authorize(mode, None, local_tick) {
            Err(DisclosureError::ModeDisabled) => RecommendationGateStatus::PolicyDisabled,
            _ => RecommendationGateStatus::ConsentRequired,
        },
        Some(consent)
            if consent.purpose != expected_purpose
                || consent.scope_commitment != expected_scope =>
        {
            RecommendationGateStatus::ConsentInvalidOrExpired
        }
        Some(consent) => match policy
            .disclosure_policy
            .authorize(mode, Some(consent), local_tick)
        {
            Ok(()) => RecommendationGateStatus::ReadyForExplicitExecutor,
            Err(DisclosureError::ModeDisabled) => RecommendationGateStatus::PolicyDisabled,
            Err(DisclosureError::ConsentRequired) => RecommendationGateStatus::ConsentRequired,
            Err(_) => RecommendationGateStatus::ConsentInvalidOrExpired,
        },
    };
    RecommendationGuard {
        policy_ref: policy.policy_ref.clone(),
        consent_commitment: consent.map(|consent| consent.consent_commitment),
        status,
        explicit_executor_required: true,
    }
}

fn push_recommendation(
    output: &mut Vec<CompanionRecommendation>,
    limit: usize,
    kind: CompanionRecommendationKind,
    guard: RecommendationGuard,
) {
    if output.len() >= limit {
        return;
    }
    let recommendation_id = recommendation_id(&kind, &guard);
    output.push(CompanionRecommendation {
        recommendation_id,
        kind,
        guard,
    });
}

fn recommendation_id(kind: &CompanionRecommendationKind, guard: &RecommendationGuard) -> [u8; 32] {
    let mut hasher = companion_hasher(b"recommendation");
    match kind {
        CompanionRecommendationKind::LocalFetch {
            query_definition,
            selector,
            budget,
        } => {
            hasher.update(&[0]);
            hasher.update(query_definition.as_bytes());
            hasher.update(selector.as_bytes());
            hash_budget(&mut hasher, *budget);
        }
        CompanionRecommendationKind::NetworkFetch {
            plan_commitment,
            selector,
            budget,
            ..
        } => {
            hasher.update(&[1]);
            hasher.update(&plan_commitment.unwrap_or([0; 32]));
            hasher.update(selector.as_bytes());
            hash_budget(&mut hasher, *budget);
        }
        CompanionRecommendationKind::ShareKnowledge { subject, mode } => {
            hasher.update(&[2]);
            hash_reference(&mut hasher, subject);
            hasher.update(&[*mode as u8]);
        }
        CompanionRecommendationKind::MaterializeMapping { proposal_id } => {
            hasher.update(&[3]);
            hasher.update(proposal_id.as_bytes());
        }
    }
    hash_reference(&mut hasher, &guard.policy_ref);
    hasher.update(&[gate_status_code(guard.status)]);
    if let Some(consent) = guard.consent_commitment {
        hasher.update(&consent);
    }
    *hasher.finalize().as_bytes()
}

fn context_root(context: &CompanionContext) -> Result<[u8; 32], CompanionError> {
    let references = canonical_set(
        context
            .receptor_definitions
            .iter()
            .map(reference_value)
            .collect(),
    )?;
    let roles = canonical_set(
        context
            .desired_roles
            .iter()
            .map(|role| CanonicalValue::Bytes(role.as_bytes().to_vec()))
            .collect(),
    )?;
    let events = canonical_set(
        context
            .observation_events
            .iter()
            .map(|event| CanonicalValue::Bytes(event.as_bytes().to_vec()))
            .collect(),
    )?;
    let proposals = canonical_set(
        context
            .observation_proposal_ids
            .iter()
            .map(|proposal| CanonicalValue::Bytes(proposal.to_vec()))
            .collect(),
    )?;
    let value = CanonicalValue::Map(vec![
        (0, references),
        (1, roles),
        (2, context.goal.canonical_value()?),
        (3, context.local_context.canonical_value()?),
        (4, events),
        (5, proposals),
        (6, CanonicalValue::Bytes(context.observed_frontier.to_vec())),
    ]);
    let bytes = encode_canonical(&value, ResourceProfile::ObjectV1)?;
    Ok(digest_bytes(b"context", &bytes))
}

fn canonical_set(values: Vec<CanonicalValue>) -> Result<CanonicalValue, CompanionError> {
    Ok(CanonicalValue::Array(
        ku_core::foundation::canonicalize_set_by_key(
            values
                .into_iter()
                .map(|value| (value.clone(), value))
                .collect(),
            ResourceProfile::ObjectV1,
        )?,
    ))
}

fn multipath_plan_commitment(plan: &MultipathQueryPlan) -> [u8; 32] {
    let mut hasher = companion_hasher(b"multipath-plan");
    for packet in plan.outbound_packets() {
        hasher.update(&(packet.len() as u64).to_be_bytes());
        hasher.update(packet);
    }
    *hasher.finalize().as_bytes()
}

fn hash_budget(hasher: &mut blake3::Hasher, budget: Budget) {
    hasher.update(&budget.max_records.to_be_bytes());
    hasher.update(&budget.max_bytes.to_be_bytes());
    hasher.update(&budget.max_work_units.to_be_bytes());
    hasher.update(&budget.max_depth.to_be_bytes());
}

fn hash_reference(hasher: &mut blake3::Hasher, reference: &ObjectReference) {
    hasher.update(&reference.reference_kind.to_be_bytes());
    hasher.update(&reference.cid);
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

fn gate_status_code(status: RecommendationGateStatus) -> u8 {
    match status {
        RecommendationGateStatus::LocalReadOnly => 0,
        RecommendationGateStatus::ReadyForExplicitExecutor => 1,
        RecommendationGateStatus::ConsentRequired => 2,
        RecommendationGateStatus::ConsentInvalidOrExpired => 3,
        RecommendationGateStatus::PolicyDisabled => 4,
        RecommendationGateStatus::ExplicitMaterializationAuthorityRequired => 5,
    }
}

fn companion_hasher(label: &[u8]) -> blake3::Hasher {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:local-companion:1\0");
    hasher.update(label);
    hasher.update(&[0]);
    hasher
}

fn digest_bytes(label: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut hasher = companion_hasher(label);
    hasher.update(&(bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

#[derive(Debug, PartialEq, Eq)]
pub enum CompanionError {
    Canonical(ku_core::foundation::CanonicalError),
    Semantic(ku_core::foundation::SemanticError),
    Inventory(ku_core::foundation::InventoryError),
    Query(QueryContractError),
    StandingNeed(StandingNeedError),
    Proposal(ProposalError),
    InvalidContext,
    InvalidPolicy,
    RawObservationShareForbidden,
    Multipath(&'static str),
}

impl From<ku_core::foundation::CanonicalError> for CompanionError {
    fn from(error: ku_core::foundation::CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<ku_core::foundation::SemanticError> for CompanionError {
    fn from(error: ku_core::foundation::SemanticError) -> Self {
        Self::Semantic(error)
    }
}

impl From<ku_core::foundation::InventoryError> for CompanionError {
    fn from(error: ku_core::foundation::InventoryError) -> Self {
        Self::Inventory(error)
    }
}

impl From<QueryContractError> for CompanionError {
    fn from(error: QueryContractError) -> Self {
        Self::Query(error)
    }
}

impl From<StandingNeedError> for CompanionError {
    fn from(error: StandingNeedError) -> Self {
        Self::StandingNeed(error)
    }
}

impl From<ProposalError> for CompanionError {
    fn from(error: ProposalError) -> Self {
        Self::Proposal(error)
    }
}

#[cfg(test)]
mod tests {
    use ku_core::foundation::{
        public_knowledge_exchange_fixture_v1, ExactRatio, MappingEnvelope, MappingKernel,
        SemanticFrameSet,
    };
    use ku_kql::vnext_disclosure::ConsentKind;
    use ku_kql::vnext_multipath::MultipathBranchPacket;
    use ku_kql::vnext_proposal::{BindingProposal, ProposalExpiry, ScoreComponent, ScoreDirection};
    use ku_kql::vnext_query::{
        CoarseRouteToken, CoarseRouteTokenClass, DisclosureCompiler, QueryRun, RouteSketchEntropy,
        MIN_ROUTE_TOKEN_SUPPORT,
    };

    use super::*;

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn concept(byte: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([byte; 16])
    }

    fn context() -> CompanionContext {
        CompanionContext {
            receptor_definitions: vec![reference(1)],
            desired_roles: vec![concept(2)],
            goal: SemanticFrameSet {
                statements: Vec::new(),
            },
            local_context: SemanticFrameSet {
                statements: Vec::new(),
            },
            observation_events: vec![EventCid::from_bytes([3; 32])],
            observation_proposal_ids: vec![[4; 32]],
            observed_frontier: [5; 32],
        }
    }

    fn policy() -> LocalCompanionPolicy {
        let mut disclosure_policy = DisclosurePolicy::private_default(reference(6));
        disclosure_policy.enabled_nonlocal_modes =
            vec![DisclosureMode::RouteMinimal, DisclosureMode::PublicProblem];
        LocalCompanionPolicy {
            policy_ref: reference(6),
            query_policy: reference(7),
            exploration_policy: reference(8),
            standing_need_watch_policy: reference(9),
            materialization_policy: reference(10),
            disclosure_policy,
            route_purpose: concept(11),
            share_purpose: concept(12),
            share_mode: DisclosureMode::PublicProblem,
            selector: public_knowledge_exchange_fixture_v1(),
            max_recommendations: 10,
        }
    }

    fn proposal() -> BindingProposal {
        let mapping_kernel = MappingKernel {
            source_objects: vec![reference(1)],
            target_objects: vec![reference(20)],
            correspondences: Vec::new(),
            assumptions: SemanticFrameSet {
                statements: Vec::new(),
            },
            constraint_regions: Vec::new(),
            unmapped_regions: Vec::new(),
        };
        let kernel = mapping_kernel.cid().unwrap();
        BindingProposal {
            mapping_kernel,
            proposed_envelope: MappingEnvelope {
                kernel,
                generator: reference(21),
                derivation_rule: None,
                evidence: Vec::new(),
                source_event: None,
            },
            candidate_objects: vec![reference(20)],
            index_commitment: None,
            model_commitment: None,
            rule_commitment: None,
            scores: vec![ScoreComponent {
                metric: concept(22),
                value: ExactRatio::integer(1),
                direction: ScoreDirection::DescriptiveOnly,
            }],
            constraints: Vec::new(),
            expiry: ProposalExpiry {
                created_at_evaluation: 1,
                expires_after_evaluations: 10,
                source_frontier: EventCid::from_bytes([23; 32]),
            },
            privacy: DisclosureClass::LocalOnly,
        }
    }

    fn opportunities() -> CompanionOpportunities {
        CompanionOpportunities {
            materialization_candidates: vec![CompanionMaterializationCandidate::from_proposal(
                &proposal(),
                1,
            )
            .unwrap()],
            share_candidates: vec![reference(30)],
        }
    }

    fn consent(mode: DisclosureMode, purpose: ConceptCcid, scope: [u8; 32]) -> DisclosureConsent {
        DisclosureConsent {
            kind: ConsentKind::Explicit,
            policy_ref: reference(6),
            mode,
            purpose,
            scope_commitment: scope,
            consent_commitment: [31; 32],
            not_before: 1,
            expires_at: 100,
        }
    }

    struct Planner {
        calls: usize,
    }

    impl OptionalCompanionMultipathPlanner for Planner {
        fn compile_proposal(
            &mut self,
            request: CompanionMultipathRequest,
        ) -> Result<MultipathQueryPlan, CompanionError> {
            self.calls += 1;
            let run = QueryRun::new(
                [40; 32],
                request.query_definition,
                public_knowledge_exchange_fixture_v1(),
            )
            .unwrap();
            let mut compiler = DisclosureCompiler::default();
            let sketch = compiler
                .compile_route_minimal(
                    &run,
                    CoarseRouteToken {
                        class: CoarseRouteTokenClass::CoarseRole,
                        allowlisted_code: 41,
                    },
                    MIN_ROUTE_TOKEN_SUPPORT,
                    1,
                    20,
                    3,
                    1,
                    RouteSketchEntropy {
                        sketch_id: [42; 32],
                        one_time_reply_capability: [43; 32],
                        replay_nonce: [44; 32],
                        commitment_salt: [45; 32],
                    },
                )
                .unwrap();
            MultipathQueryPlan::new(vec![MultipathBranchPacket {
                local_path_commitment: [46; 32],
                packet_bytes: sketch.network_bytes().unwrap(),
            }])
            .map_err(|_| CompanionError::Multipath("plan"))
        }
    }

    #[test]
    fn fully_offline_plan_builds_private_need_standing_need_and_recommendations() {
        let plan = LocalKnowledgeCompanion::plan(
            context(),
            policy(),
            CompanionDisclosureGrants::default(),
            opportunities(),
            10,
            None,
        )
        .unwrap();
        assert_eq!(plan.private_query.need.privacy, DisclosureClass::LocalOnly);
        assert_eq!(plan.standing_needs.len(), 1);
        assert_eq!(plan.standing_needs[0].privacy, DisclosureClass::LocalOnly);
        assert_eq!(plan.network_status, CompanionNetworkStatus::NotConfigured);
        assert!(plan.operates_offline());
        assert!(!plan.performs_side_effects());
        assert!(!plan.can_publish_or_materialize());
        assert!(plan.recommendations.iter().any(|recommendation| matches!(
            recommendation.kind,
            CompanionRecommendationKind::LocalFetch { .. }
        )));
        assert!(plan.recommendations.iter().any(|recommendation| matches!(
            recommendation.kind,
            CompanionRecommendationKind::MaterializeMapping { .. }
        )));
        let materialize = plan
            .recommendations
            .iter()
            .find(|recommendation| {
                matches!(
                    recommendation.kind,
                    CompanionRecommendationKind::MaterializeMapping { .. }
                )
            })
            .unwrap();
        assert_eq!(
            materialize.guard.status,
            RecommendationGateStatus::ExplicitMaterializationAuthorityRequired
        );
    }

    #[test]
    fn authorized_route_consent_compiles_but_never_sends_multipath_plan() {
        let context = context();
        let policy = policy();
        let selector = policy.selector.cid().unwrap();
        let scope =
            companion_disclosure_scope(&context, selector, DisclosureMode::RouteMinimal, None)
                .unwrap();
        let grants = CompanionDisclosureGrants {
            route: Some(consent(
                DisclosureMode::RouteMinimal,
                policy.route_purpose,
                scope,
            )),
            shares: Vec::new(),
        };
        let mut planner = Planner { calls: 0 };
        let plan = LocalKnowledgeCompanion::plan(
            context,
            policy,
            grants,
            CompanionOpportunities::default(),
            10,
            Some(&mut planner),
        )
        .unwrap();
        assert_eq!(planner.calls, 1);
        assert_eq!(
            plan.network_status,
            CompanionNetworkStatus::ProposalCompiled
        );
        let network = plan
            .recommendations
            .iter()
            .find(|recommendation| {
                matches!(
                    recommendation.kind,
                    CompanionRecommendationKind::NetworkFetch { .. }
                )
            })
            .unwrap();
        assert_eq!(
            network.guard.status,
            RecommendationGateStatus::ReadyForExplicitExecutor
        );
        assert!(!network.performs_side_effect());
    }

    #[test]
    fn missing_route_consent_does_not_call_optional_adapter() {
        let mut planner = Planner { calls: 0 };
        let plan = LocalKnowledgeCompanion::plan(
            context(),
            policy(),
            CompanionDisclosureGrants::default(),
            CompanionOpportunities::default(),
            10,
            Some(&mut planner),
        )
        .unwrap();
        assert_eq!(planner.calls, 0);
        assert_eq!(
            plan.network_status,
            CompanionNetworkStatus::BlockedByPolicyOrConsent
        );
        let network = plan
            .recommendations
            .iter()
            .find(|recommendation| {
                matches!(
                    recommendation.kind,
                    CompanionRecommendationKind::NetworkFetch { .. }
                )
            })
            .unwrap();
        assert_eq!(
            network.guard.status,
            RecommendationGateStatus::ConsentRequired
        );
        let CompanionRecommendationKind::NetworkFetch { ref plan, .. } = network.kind else {
            unreachable!()
        };
        assert!(plan.is_none());
    }

    #[test]
    fn direct_raw_observation_share_is_forbidden() {
        let opportunities = CompanionOpportunities {
            materialization_candidates: Vec::new(),
            share_candidates: vec![ObjectReference::new(
                ku_core::foundation::SOURCE_ARTIFACT_KIND.0,
                [50; 32],
            )],
        };
        assert_eq!(
            LocalKnowledgeCompanion::plan(
                context(),
                policy(),
                CompanionDisclosureGrants::default(),
                opportunities,
                10,
                None
            )
            .unwrap_err(),
            CompanionError::RawObservationShareForbidden
        );
    }

    #[test]
    fn share_recommendation_needs_exact_subject_scope_and_still_does_not_publish() {
        let context = context();
        let policy = policy();
        let subject = reference(30);
        let selector = policy.selector.cid().unwrap();
        let missing = LocalKnowledgeCompanion::plan(
            context.clone(),
            policy.clone(),
            CompanionDisclosureGrants::default(),
            CompanionOpportunities {
                materialization_candidates: Vec::new(),
                share_candidates: vec![subject.clone()],
            },
            10,
            None,
        )
        .unwrap();
        let share = missing
            .recommendations
            .iter()
            .find(|recommendation| {
                matches!(
                    recommendation.kind,
                    CompanionRecommendationKind::ShareKnowledge { .. }
                )
            })
            .unwrap();
        assert_eq!(
            share.guard.status,
            RecommendationGateStatus::ConsentRequired
        );

        let scope =
            companion_disclosure_scope(&context, selector, policy.share_mode, Some(&subject))
                .unwrap();
        let authorized = LocalKnowledgeCompanion::plan(
            context,
            policy.clone(),
            CompanionDisclosureGrants {
                route: None,
                shares: vec![CompanionShareGrant {
                    subject: subject.clone(),
                    consent: consent(policy.share_mode, policy.share_purpose, scope),
                }],
            },
            CompanionOpportunities {
                materialization_candidates: Vec::new(),
                share_candidates: vec![subject],
            },
            10,
            None,
        )
        .unwrap();
        let share = authorized
            .recommendations
            .iter()
            .find(|recommendation| {
                matches!(
                    recommendation.kind,
                    CompanionRecommendationKind::ShareKnowledge { .. }
                )
            })
            .unwrap();
        assert_eq!(
            share.guard.status,
            RecommendationGateStatus::ReadyForExplicitExecutor
        );
        assert!(share.guard.explicit_executor_required);
        assert!(!share.performs_side_effect());
        assert!(!authorized.can_publish_or_materialize());
    }

    #[test]
    fn recommendation_budget_is_a_hard_deterministic_cap() {
        let mut policy = policy();
        policy.max_recommendations = 1;
        let plan = LocalKnowledgeCompanion::plan(
            context(),
            policy,
            CompanionDisclosureGrants::default(),
            opportunities(),
            10,
            None,
        )
        .unwrap();
        assert_eq!(plan.recommendations.len(), 1);
        assert!(matches!(
            plan.recommendations[0].kind,
            CompanionRecommendationKind::LocalFetch { .. }
        ));
    }
}
