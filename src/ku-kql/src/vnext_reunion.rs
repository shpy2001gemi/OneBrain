//! Frontier-delta reunion joins for remote Affordance/Receptor objects.

use std::collections::BTreeSet;

use ku_core::foundation::{
    AffordanceError, ConceptCcid, DisclosureClass, EventCid, KnowledgeAffordance, ObjectReference,
    ObjectSemantics, ReceptorDefinition, ReceptorError, ResourceProfile, SemanticFrameSet,
    ValidatedKnowledgeObject, KNOWLEDGE_AFFORDANCE_KIND, RECEPTOR_DEFINITION_KIND,
};

use crate::vnext_matcher::{
    ExactTypedMatcher, MatcherError, MatcherMetricConcepts, MatcherOutcome, TypedMatchRequest,
};
use crate::vnext_proposal::{ProposalError, ProposalId, ProposalQuarantine};
use crate::vnext_standing_need::{StandingNeed, StandingNeedId, StandingNeedState};

#[derive(Clone, Debug)]
pub struct ValidatedRemoteAffordance {
    reference: ObjectReference,
    affordance: KnowledgeAffordance,
}

impl ValidatedRemoteAffordance {
    pub fn from_public_object(
        validated: &ValidatedKnowledgeObject,
        affordance: KnowledgeAffordance,
    ) -> Result<Self, ReunionError> {
        validate_known_public_object(validated, KNOWLEDGE_AFFORDANCE_KIND.0)?;
        let (bytes, cid) = affordance
            .to_knowledge_object(DisclosureClass::Public)?
            .encode(ResourceProfile::ObjectV1)?;
        if cid != validated.cid() || bytes != validated.original_bytes() {
            return Err(ReunionError::DecodedValueMismatch);
        }
        Ok(Self {
            reference: ObjectReference::new(KNOWLEDGE_AFFORDANCE_KIND.0, cid.into_bytes()),
            affordance,
        })
    }

    pub fn reference(&self) -> &ObjectReference {
        &self.reference
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedRemoteReceptor {
    reference: ObjectReference,
    receptor: ReceptorDefinition,
}

impl ValidatedRemoteReceptor {
    pub fn from_public_object(
        validated: &ValidatedKnowledgeObject,
        receptor: ReceptorDefinition,
    ) -> Result<Self, ReunionError> {
        validate_known_public_object(validated, RECEPTOR_DEFINITION_KIND.0)?;
        let (bytes, cid) = receptor
            .to_knowledge_object(DisclosureClass::Public)?
            .encode(ResourceProfile::ObjectV1)?;
        if cid != validated.cid() || bytes != validated.original_bytes() {
            return Err(ReunionError::DecodedValueMismatch);
        }
        Ok(Self {
            reference: ObjectReference::new(RECEPTOR_DEFINITION_KIND.0, cid.into_bytes()),
            receptor,
        })
    }

    pub fn reference(&self) -> &ObjectReference {
        &self.reference
    }
}

fn validate_known_public_object(
    validated: &ValidatedKnowledgeObject,
    expected_kind: u64,
) -> Result<(), ReunionError> {
    if validated.disclosure() != DisclosureClass::Public || validated.is_opaque() {
        return Err(ReunionError::RemoteObjectMustBeValidatedPublic);
    }
    match validated.semantics() {
        ObjectSemantics::Known(envelope) if envelope.kind.0 == expected_kind => Ok(()),
        _ => Err(ReunionError::RemoteObjectKindMismatch),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalNeedTarget {
    pub need: StandingNeed,
    pub receptor: ReceptorDefinition,
    pub required_semantics: SemanticFrameSet,
    pub local_context: SemanticFrameSet,
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
}

impl LocalNeedTarget {
    pub fn validate(&self) -> Result<StandingNeedId, ReunionError> {
        self.need.validate()?;
        if self.need.state != StandingNeedState::Active {
            return Err(ReunionError::StandingNeedNotActive);
        }
        self.need.id().map_err(Into::into)
    }
}

#[derive(Clone, Debug)]
pub struct RemoteReceptorDelta {
    pub receptor: ValidatedRemoteReceptor,
    pub required_semantics: SemanticFrameSet,
    pub public_context: SemanticFrameSet,
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
}

#[derive(Clone, Debug)]
pub struct LocalAffordanceTarget {
    pub reference: ObjectReference,
    pub affordance: KnowledgeAffordance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReunionBudget {
    pub max_delta_objects: u64,
    pub max_pairs: u64,
    pub max_proposals: u64,
}

impl ReunionBudget {
    fn validate(self) -> Result<(), ReunionError> {
        if self.max_delta_objects == 0
            || self.max_delta_objects > 65_536
            || self.max_pairs == 0
            || self.max_pairs > 1_000_000
            || self.max_proposals == 0
            || self.max_proposals > 65_536
        {
            return Err(ReunionError::InvalidBudget);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReunionDirection {
    RemoteAffordanceToLocalNeed,
    RemoteReceptorToLocalAffordance,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReunionProposalRecord {
    pub proposal: ProposalId,
    pub local_need: Option<StandingNeedId>,
    pub direction: ReunionDirection,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReunionDeltaReport {
    pub processed_delta_objects: u64,
    pub processed_pairs: u64,
    pub hard_mismatches: u64,
    pub duplicate_frontier_objects: u64,
    pub budget_deferred_objects: u64,
    pub proposals: Vec<ReunionProposalRecord>,
}

#[derive(Default)]
pub struct ReunionFrontier {
    remote_affordances: BTreeSet<[u8; 32]>,
    remote_receptors: BTreeSet<[u8; 32]>,
}

impl ReunionFrontier {
    pub fn has_processed_affordance(&self, cid: [u8; 32]) -> bool {
        self.remote_affordances.contains(&cid)
    }

    pub fn join_affordance_delta(
        &mut self,
        mut delta: Vec<ValidatedRemoteAffordance>,
        local_targets: &[LocalNeedTarget],
        quarantine: &mut ProposalQuarantine,
        budget: ReunionBudget,
    ) -> Result<ReunionDeltaReport, ReunionError> {
        budget.validate()?;
        let mut targets = local_targets
            .iter()
            .map(|target| target.validate().map(|id| (id, target)))
            .collect::<Result<Vec<_>, _>>()?;
        targets.sort_by_key(|(id, _)| *id.as_bytes());
        delta.sort_by_key(|value| value.reference.cid);
        let mut report = ReunionDeltaReport::default();
        for affordance in delta {
            if self.remote_affordances.contains(&affordance.reference.cid) {
                report.duplicate_frontier_objects += 1;
                continue;
            }
            if report.processed_delta_objects >= budget.max_delta_objects
                || report.processed_pairs + targets.len() as u64 > budget.max_pairs
                || report.proposals.len() as u64 + targets.len() as u64 > budget.max_proposals
            {
                report.budget_deferred_objects += 1;
                continue;
            }
            for (need_id, target) in &targets {
                let outcome = ExactTypedMatcher::match_affordance(TypedMatchRequest {
                    receptor_reference: target.need.receptor_definition.clone(),
                    receptor: &target.receptor,
                    required_semantics: &target.required_semantics,
                    local_context: &target.local_context,
                    affordance_reference: affordance.reference.clone(),
                    affordance: &affordance.affordance,
                    generator: target.generator.clone(),
                    derivation_rule: target.derivation_rule.clone(),
                    evidence: target.evidence.clone(),
                    index_commitment: target.index_commitment.clone(),
                    rule_commitment: target.rule_commitment.clone(),
                    metrics: target.metrics,
                    unmapped_reason: target.unmapped_reason,
                    source_frontier: target.source_frontier,
                    created_at_evaluation: target.created_at_evaluation,
                    expires_after_evaluations: target.expires_after_evaluations,
                    privacy: DisclosureClass::LocalOnly,
                })?;
                report.processed_pairs += 1;
                match outcome {
                    MatcherOutcome::Proposal { proposal, .. }
                        if (report.proposals.len() as u64) < budget.max_proposals =>
                    {
                        let proposal = quarantine.insert(proposal)?;
                        report.proposals.push(ReunionProposalRecord {
                            proposal,
                            local_need: Some(*need_id),
                            direction: ReunionDirection::RemoteAffordanceToLocalNeed,
                        });
                    }
                    MatcherOutcome::Proposal { .. } => {}
                    MatcherOutcome::HardMismatch { .. } => report.hard_mismatches += 1,
                }
            }
            self.remote_affordances.insert(affordance.reference.cid);
            report.processed_delta_objects += 1;
        }
        report
            .proposals
            .sort_by_key(|record| *record.proposal.as_bytes());
        Ok(report)
    }

    pub fn join_receptor_delta(
        &mut self,
        mut delta: Vec<RemoteReceptorDelta>,
        local_affordances: &[LocalAffordanceTarget],
        quarantine: &mut ProposalQuarantine,
        budget: ReunionBudget,
    ) -> Result<ReunionDeltaReport, ReunionError> {
        budget.validate()?;
        let mut targets = local_affordances.iter().collect::<Vec<_>>();
        targets.sort_by_key(|target| target.reference.cid);
        delta.sort_by_key(|value| value.receptor.reference.cid);
        let mut report = ReunionDeltaReport::default();
        for remote in delta {
            if self
                .remote_receptors
                .contains(&remote.receptor.reference.cid)
            {
                report.duplicate_frontier_objects += 1;
                continue;
            }
            if report.processed_delta_objects >= budget.max_delta_objects
                || report.processed_pairs + targets.len() as u64 > budget.max_pairs
                || report.proposals.len() as u64 + targets.len() as u64 > budget.max_proposals
            {
                report.budget_deferred_objects += 1;
                continue;
            }
            for target in &targets {
                let outcome = ExactTypedMatcher::match_affordance(TypedMatchRequest {
                    receptor_reference: remote.receptor.reference.clone(),
                    receptor: &remote.receptor.receptor,
                    required_semantics: &remote.required_semantics,
                    local_context: &remote.public_context,
                    affordance_reference: target.reference.clone(),
                    affordance: &target.affordance,
                    generator: remote.generator.clone(),
                    derivation_rule: remote.derivation_rule.clone(),
                    evidence: remote.evidence.clone(),
                    index_commitment: remote.index_commitment.clone(),
                    rule_commitment: remote.rule_commitment.clone(),
                    metrics: remote.metrics,
                    unmapped_reason: remote.unmapped_reason,
                    source_frontier: remote.source_frontier,
                    created_at_evaluation: remote.created_at_evaluation,
                    expires_after_evaluations: remote.expires_after_evaluations,
                    privacy: DisclosureClass::LocalOnly,
                })?;
                report.processed_pairs += 1;
                match outcome {
                    MatcherOutcome::Proposal { proposal, .. }
                        if (report.proposals.len() as u64) < budget.max_proposals =>
                    {
                        let proposal = quarantine.insert(proposal)?;
                        report.proposals.push(ReunionProposalRecord {
                            proposal,
                            local_need: None,
                            direction: ReunionDirection::RemoteReceptorToLocalAffordance,
                        });
                    }
                    MatcherOutcome::Proposal { .. } => {}
                    MatcherOutcome::HardMismatch { .. } => report.hard_mismatches += 1,
                }
            }
            self.remote_receptors.insert(remote.receptor.reference.cid);
            report.processed_delta_objects += 1;
        }
        report
            .proposals
            .sort_by_key(|record| *record.proposal.as_bytes());
        Ok(report)
    }

    pub const fn exports_private_need_state(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReunionError {
    Object(ku_core::foundation::ObjectError),
    Affordance(AffordanceError),
    Receptor(ReceptorError),
    Matcher(MatcherError),
    Proposal(ProposalError),
    StandingNeed(crate::vnext_standing_need::StandingNeedError),
    RemoteObjectMustBeValidatedPublic,
    RemoteObjectKindMismatch,
    DecodedValueMismatch,
    StandingNeedNotActive,
    InvalidBudget,
}

impl From<ku_core::foundation::ObjectError> for ReunionError {
    fn from(error: ku_core::foundation::ObjectError) -> Self {
        Self::Object(error)
    }
}

impl From<AffordanceError> for ReunionError {
    fn from(error: AffordanceError) -> Self {
        Self::Affordance(error)
    }
}

impl From<ReceptorError> for ReunionError {
    fn from(error: ReceptorError) -> Self {
        Self::Receptor(error)
    }
}

impl From<MatcherError> for ReunionError {
    fn from(error: MatcherError) -> Self {
        Self::Matcher(error)
    }
}

impl From<ProposalError> for ReunionError {
    fn from(error: ProposalError) -> Self {
        Self::Proposal(error)
    }
}

impl From<crate::vnext_standing_need::StandingNeedError> for ReunionError {
    fn from(error: crate::vnext_standing_need::StandingNeedError) -> Self {
        Self::StandingNeed(error)
    }
}

#[cfg(test)]
mod tests {
    use ku_core::foundation::{
        decode_knowledge_object, AcceptedInput, AffordanceOrigin, AffordanceSemantics,
        KnownObjectKind, ObjectCid, ReceptorAcceptanceProfile, ReceptorCardinality, ReceptorOrigin,
        SelectorCid, StatementFrame, StatementId, StatementLocator, StatementQualifiers, TermRef,
        UnknownConstraintPolicy,
    };

    use super::*;

    fn concept(byte: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([byte; 16])
    }

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn empty() -> SemanticFrameSet {
        SemanticFrameSet { statements: vec![] }
    }

    fn frames(marker: u8) -> SemanticFrameSet {
        SemanticFrameSet {
            statements: vec![StatementFrame {
                statement_id: StatementId(1),
                operator_or_predicate: concept(3),
                arguments: vec![TermRef::Concept(concept(marker))],
                constraints: vec![],
                qualifiers: StatementQualifiers::default(),
            }],
        }
    }

    fn receptor() -> ReceptorDefinition {
        ReceptorDefinition {
            role: concept(1),
            expected_types: vec![concept(2)],
            hard_constraints: vec![],
            cardinality: ReceptorCardinality::new(1, Some(1)).unwrap(),
            origin: ReceptorOrigin::Declared {
                source: StatementLocator {
                    object: reference(10),
                    statement_index: 0,
                },
            },
            acceptance: ReceptorAcceptanceProfile {
                policy: reference(11),
                required_evidence_kinds: vec![],
                unknown_constraint_policy: UnknownConstraintPolicy::KeepUnresolved,
            },
        }
    }

    fn affordance(marker: u8) -> KnowledgeAffordance {
        let empty = empty();
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
                outputs: frames(marker),
                effects: empty.clone(),
                properties: empty.clone(),
                invariants: empty.clone(),
                operating_conditions: empty.clone(),
                limits: empty,
            },
            abstraction_patterns: vec![],
            origin: AffordanceOrigin::Explicit {
                claims: vec![StatementLocator {
                    object: reference(20),
                    statement_index: 0,
                }],
            },
        }
    }

    fn validated_affordance(marker: u8) -> ValidatedRemoteAffordance {
        let affordance = affordance(marker);
        let object = affordance
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap();
        let (bytes, _) = object.encode(ResourceProfile::ObjectV1).unwrap();
        let validated = decode_knowledge_object(
            &bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(KNOWLEDGE_AFFORDANCE_KIND, 1)],
            &[],
        )
        .unwrap();
        ValidatedRemoteAffordance::from_public_object(&validated, affordance).unwrap()
    }

    fn validated_receptor() -> (ValidatedKnowledgeObject, ReceptorDefinition) {
        let receptor = receptor();
        let object = receptor
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap();
        let (bytes, _) = object.encode(ResourceProfile::ObjectV1).unwrap();
        let validated = decode_knowledge_object(
            &bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(RECEPTOR_DEFINITION_KIND, 1)],
            &[],
        )
        .unwrap();
        (validated, receptor)
    }

    fn target() -> LocalNeedTarget {
        let receptor_object = receptor()
            .to_knowledge_object(DisclosureClass::LocalOnly)
            .unwrap();
        let (_, receptor_cid) = receptor_object.encode(ResourceProfile::ObjectV1).unwrap();
        LocalNeedTarget {
            need: StandingNeed::new_local(
                ObjectReference::new(RECEPTOR_DEFINITION_KIND.0, receptor_cid.into_bytes()),
                ObjectCid::from_bytes([30; 32]),
                SelectorCid::from_bytes([31; 32]),
                reference(32),
                [33; 32],
            ),
            receptor: receptor(),
            required_semantics: frames(60),
            local_context: empty(),
            generator: reference(34),
            derivation_rule: Some(reference(35)),
            evidence: vec![reference(36)],
            index_commitment: Some(reference(37)),
            rule_commitment: Some(reference(38)),
            metrics: MatcherMetricConcepts {
                structural_fit: concept(40),
                constraint_fit: concept(41),
            },
            unmapped_reason: concept(42),
            source_frontier: EventCid::from_bytes([43; 32]),
            created_at_evaluation: 1,
            expires_after_evaluations: 10,
        }
    }

    fn budget() -> ReunionBudget {
        ReunionBudget {
            max_delta_objects: 8,
            max_pairs: 32,
            max_proposals: 8,
        }
    }

    #[test]
    fn remote_affordance_delta_triggers_only_matching_local_standing_need_once() {
        let mut frontier = ReunionFrontier::default();
        let mut quarantine = ProposalQuarantine::default();
        let matching = validated_affordance(60);
        let report = frontier
            .join_affordance_delta(
                vec![matching.clone()],
                &[target()],
                &mut quarantine,
                budget(),
            )
            .unwrap();
        assert_eq!(report.proposals.len(), 1);
        assert_eq!(
            report.proposals[0].local_need,
            Some(target().need.id().unwrap())
        );
        assert!(!quarantine.is_executable());
        assert!(!frontier.exports_private_need_state());

        let replay = frontier
            .join_affordance_delta(vec![matching], &[target()], &mut quarantine, budget())
            .unwrap();
        assert_eq!(replay.proposals.len(), 0);
        assert_eq!(replay.duplicate_frontier_objects, 1);
    }

    #[test]
    fn delta_only_processing_defers_whole_object_when_pair_budget_is_insufficient() {
        let mut frontier = ReunionFrontier::default();
        let mut quarantine = ProposalQuarantine::default();
        let constrained = ReunionBudget {
            max_delta_objects: 1,
            max_pairs: 1,
            max_proposals: 1,
        };
        let report = frontier
            .join_affordance_delta(
                vec![validated_affordance(60)],
                &[target(), target()],
                &mut quarantine,
                constrained,
            )
            .unwrap();
        assert_eq!(report.processed_pairs, 0);
        assert_eq!(report.budget_deferred_objects, 1);
    }

    #[test]
    fn delta_object_is_not_consumed_when_proposal_budget_cannot_cover_candidates() {
        let mut frontier = ReunionFrontier::default();
        let mut quarantine = ProposalQuarantine::default();
        let constrained = ReunionBudget {
            max_delta_objects: 1,
            max_pairs: 2,
            max_proposals: 1,
        };
        let delta = validated_affordance(60);
        let report = frontier
            .join_affordance_delta(
                vec![delta.clone()],
                &[target(), target()],
                &mut quarantine,
                constrained,
            )
            .unwrap();
        assert_eq!(report.processed_delta_objects, 0);
        assert_eq!(report.budget_deferred_objects, 1);

        let retried = frontier
            .join_affordance_delta(vec![delta], &[target()], &mut quarantine, budget())
            .unwrap();
        assert_eq!(retried.processed_delta_objects, 1);
        assert_eq!(retried.proposals.len(), 1);
    }

    #[test]
    fn inverse_join_requires_exact_validated_public_receptor_and_stays_proposal_only() {
        let (validated, receptor) = validated_receptor();
        let remote = ValidatedRemoteReceptor::from_public_object(&validated, receptor).unwrap();
        let delta = RemoteReceptorDelta {
            receptor: remote,
            required_semantics: frames(60),
            public_context: empty(),
            generator: reference(50),
            derivation_rule: None,
            evidence: vec![reference(51)],
            index_commitment: None,
            rule_commitment: None,
            metrics: MatcherMetricConcepts {
                structural_fit: concept(40),
                constraint_fit: concept(41),
            },
            unmapped_reason: concept(42),
            source_frontier: EventCid::from_bytes([52; 32]),
            created_at_evaluation: 1,
            expires_after_evaluations: 10,
        };
        let local = LocalAffordanceTarget {
            reference: reference(53),
            affordance: affordance(60),
        };
        let mut frontier = ReunionFrontier::default();
        let mut quarantine = ProposalQuarantine::default();
        let report = frontier
            .join_receptor_delta(vec![delta], &[local], &mut quarantine, budget())
            .unwrap();
        assert_eq!(report.proposals.len(), 1);
        assert_eq!(report.proposals[0].local_need, None);
        assert!(!quarantine.is_executable());
    }

    #[test]
    fn private_or_wrong_kind_remote_object_cannot_enter_inverse_join() {
        let receptor = receptor();
        let object = receptor
            .to_knowledge_object(DisclosureClass::LocalOnly)
            .unwrap();
        let (bytes, _) = object.encode(ResourceProfile::ObjectV1).unwrap();
        let validated = decode_knowledge_object(
            &bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(RECEPTOR_DEFINITION_KIND, 1)],
            &[],
        )
        .unwrap();
        assert_eq!(
            ValidatedRemoteReceptor::from_public_object(&validated, receptor).unwrap_err(),
            ReunionError::RemoteObjectMustBeValidatedPublic
        );
    }
}
