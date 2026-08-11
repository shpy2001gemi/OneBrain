//! Offline-first vNext vertical slice.
//!
//! The runtime composes existing contracts without merging their authority
//! boundaries: matching creates a private proposal, materialization requires an
//! explicit command/authority decision, and resolution changes only after a
//! separately signed and policy-assessed event.

use ku_core::foundation::{
    assess_resolution_event, AtomicMappingBackend, BindingAcceptance, DisclosureClass, EventCid,
    FrontierAssemblyManifest, KnowledgeAffordance, MappingMaterializer, MaterializationAuthority,
    MaterializationError, MaterializationIntent, MaterializedMapping, ObjectCid, ObjectReference,
    PermitCid, PlacementId, ReceptorDefinition, ReferenceDisclosureIndex, ResolutionApplyOutcome,
    ResolutionAuthority, ResolutionError, ResolutionReducer, ResolutionState, ResolutionTarget,
    ResolutionView, SemanticFrameSet, ValidatedResolutionEvent,
};
use ku_core::foundation::{ActorId, MappingKernelCid};
use ku_kql::vnext_matcher::{
    ExactTypedMatcher, MatchCheck, MatcherError, MatcherMetricConcepts, MatcherOutcome,
    TypedMatchRequest,
};
use ku_kql::vnext_proposal::{
    BindingProposal, ProposalDisposition, ProposalError, ProposalId, ProposalQuarantine,
};
use ku_kql::vnext_query::{KnowledgeNeedIr, QueryContractError, QueryDefinition};

#[derive(Clone, Debug)]
pub struct LocalCandidateInput<'a> {
    pub receptor: &'a ReceptorDefinition,
    pub required_semantics: &'a SemanticFrameSet,
    pub affordance_reference: ObjectReference,
    pub affordance: &'a KnowledgeAffordance,
    pub generator: ObjectReference,
    pub derivation_rule: Option<ObjectReference>,
    pub evidence: Vec<ObjectReference>,
    pub index_commitment: Option<ObjectReference>,
    pub rule_commitment: Option<ObjectReference>,
    pub metrics: MatcherMetricConcepts,
    pub unmapped_reason: ku_core::foundation::ConceptCcid,
    pub source_frontier: EventCid,
    pub created_at_evaluation: u64,
    pub expires_after_evaluations: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalCandidateOutcome {
    Quarantined {
        proposal_id: ProposalId,
        checks: Vec<MatchCheck>,
    },
    HardMismatch {
        checks: Vec<MatchCheck>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalMaterializationRequest {
    pub proposal_id: ProposalId,
    pub current_evaluation: u64,
    pub intent: MaterializationIntent,
    pub authorization_ref: Option<PermitCid>,
    pub destination: DisclosureClass,
    pub idempotency_key: [u8; 32],
    pub requester: ActorId,
    pub authority: MaterializationAuthority,
}

pub struct LocalVerticalSlice<B> {
    receptor_reference: ObjectReference,
    local_context: SemanticFrameSet,
    target: ResolutionTarget,
    resolution_policy: ObjectReference,
    proposals: ProposalQuarantine,
    materializer: MappingMaterializer<B>,
    resolution: ResolutionReducer,
}

impl<B: AtomicMappingBackend> LocalVerticalSlice<B> {
    pub fn new(
        assembly: &FrontierAssemblyManifest,
        assembly_cid: ObjectCid,
        placement_id: PlacementId,
        assessed_frontier: [u8; 32],
        mapping_backend: B,
    ) -> Result<Self, LocalRuntimeError> {
        let placement = assembly
            .placement(placement_id)
            .ok_or(LocalRuntimeError::PlacementNotFound)?;
        let resolution_policy = placement
            .resolution_policy_override
            .clone()
            .unwrap_or_else(|| assembly.default_resolution_policy.clone());
        let target = ResolutionTarget {
            assembly_lineage: assembly.lineage,
            assembly_revision: assembly_cid,
            placement: placement_id,
        };
        Ok(Self {
            receptor_reference: placement.receptor_definition.clone(),
            local_context: placement.local_context.clone(),
            target,
            resolution_policy: resolution_policy.clone(),
            proposals: ProposalQuarantine::default(),
            materializer: MappingMaterializer::new(mapping_backend),
            resolution: ResolutionReducer::new(target, resolution_policy, assessed_frontier),
        })
    }

    pub const fn target(&self) -> ResolutionTarget {
        self.target
    }

    pub const fn resolution_policy(&self) -> &ObjectReference {
        &self.resolution_policy
    }

    pub const fn receptor_reference(&self) -> &ObjectReference {
        &self.receptor_reference
    }

    pub fn build_query_definition(
        &self,
        receptor: &ReceptorDefinition,
        goal: SemanticFrameSet,
        query_policy: ObjectReference,
        exploration_policy: ObjectReference,
    ) -> Result<QueryDefinition, LocalRuntimeError> {
        let query = QueryDefinition {
            need: KnowledgeNeedIr {
                receptor_definitions: vec![self.receptor_reference.clone()],
                desired_roles: vec![receptor.role],
                goal,
                local_context: self.local_context.clone(),
                privacy: DisclosureClass::LocalOnly,
            },
            query_policy,
            exploration_policy,
        };
        query.private_cid().map_err(LocalRuntimeError::Query)?;
        Ok(query)
    }

    pub fn propose(
        &mut self,
        input: LocalCandidateInput<'_>,
    ) -> Result<LocalCandidateOutcome, LocalRuntimeError> {
        let outcome = ExactTypedMatcher::match_affordance(TypedMatchRequest {
            receptor_reference: self.receptor_reference.clone(),
            receptor: input.receptor,
            required_semantics: input.required_semantics,
            local_context: &self.local_context,
            affordance_reference: input.affordance_reference,
            affordance: input.affordance,
            generator: input.generator,
            derivation_rule: input.derivation_rule,
            evidence: input.evidence,
            index_commitment: input.index_commitment,
            rule_commitment: input.rule_commitment,
            metrics: input.metrics,
            unmapped_reason: input.unmapped_reason,
            source_frontier: input.source_frontier,
            created_at_evaluation: input.created_at_evaluation,
            expires_after_evaluations: input.expires_after_evaluations,
            privacy: DisclosureClass::LocalOnly,
        })
        .map_err(LocalRuntimeError::Matcher)?;
        match outcome {
            MatcherOutcome::Proposal { proposal, checks } => {
                let proposal_id = self
                    .proposals
                    .insert(*proposal)
                    .map_err(LocalRuntimeError::Proposal)?;
                Ok(LocalCandidateOutcome::Quarantined {
                    proposal_id,
                    checks,
                })
            }
            MatcherOutcome::HardMismatch { checks } => {
                Ok(LocalCandidateOutcome::HardMismatch { checks })
            }
        }
    }

    pub fn proposal(&self, id: ProposalId) -> Option<&BindingProposal> {
        self.proposals.get(id)
    }

    /// Imports a proposal emitted by the local reunion-delta join. This remains
    /// a quarantine-to-quarantine operation and grants no materialization or
    /// resolution authority.
    pub fn import_reunion_proposal(
        &mut self,
        proposal: BindingProposal,
    ) -> Result<ProposalId, LocalRuntimeError> {
        if !proposal
            .mapping_kernel
            .source_objects
            .contains(&self.receptor_reference)
        {
            return Err(LocalRuntimeError::ProposalTargetMismatch);
        }
        self.proposals
            .insert(proposal)
            .map_err(LocalRuntimeError::Proposal)
    }

    pub const fn proposal_store_is_executable(&self) -> bool {
        self.proposals.is_executable()
    }

    pub fn materialize(
        &self,
        request: LocalMaterializationRequest,
        disclosures: &ReferenceDisclosureIndex,
    ) -> Result<MaterializedMapping, LocalRuntimeError> {
        let proposal = self
            .proposals
            .get(request.proposal_id)
            .ok_or(LocalRuntimeError::ProposalNotFound)?;
        match proposal.disposition(request.current_evaluation) {
            ProposalDisposition::CandidateOnly => {}
            ProposalDisposition::BlockedHardViolation => {
                return Err(LocalRuntimeError::ProposalBlocked)
            }
            ProposalDisposition::Expired => return Err(LocalRuntimeError::ProposalExpired),
        }
        let command = ku_core::foundation::MaterializeMappingCommand {
            mapping_kernel: proposal.mapping_kernel.clone(),
            mapping_envelope: proposal.proposed_envelope.clone(),
            intent: request.intent,
            authorization_ref: request.authorization_ref,
            destination: request.destination,
            idempotency_key: request.idempotency_key,
            requester: request.requester,
        };
        self.materializer
            .materialize(&command, request.authority, disclosures)
            .map_err(LocalRuntimeError::Materialization)
    }

    pub fn apply_resolution(
        &mut self,
        event: ValidatedResolutionEvent,
        authority: ResolutionAuthority,
        acceptance: Option<BindingAcceptance>,
    ) -> Result<ResolutionApplyOutcome, LocalRuntimeError> {
        let assessed = assess_resolution_event(event, authority, acceptance, &self.materializer)
            .map_err(LocalRuntimeError::Resolution)?;
        self.resolution
            .apply(assessed)
            .map_err(LocalRuntimeError::Resolution)
    }

    pub fn resolution_view(&self) -> Result<ResolutionView, LocalRuntimeError> {
        self.resolution
            .view()
            .map_err(LocalRuntimeError::Resolution)
    }

    pub fn is_mapping_materialized(
        &self,
        mapping: MappingKernelCid,
    ) -> Result<bool, LocalRuntimeError> {
        self.materializer
            .is_materialized(mapping)
            .map_err(LocalRuntimeError::Materialization)
    }

    pub fn is_satisfied_relative(&self) -> Result<bool, LocalRuntimeError> {
        Ok(self.resolution_view()?.state == ResolutionState::SatisfiedRelative)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalRuntimeError {
    PlacementNotFound,
    Query(QueryContractError),
    Matcher(MatcherError),
    Proposal(ProposalError),
    ProposalNotFound,
    ProposalBlocked,
    ProposalExpired,
    ProposalTargetMismatch,
    Materialization(MaterializationError),
    Resolution(ResolutionError),
}

impl std::fmt::Display for LocalRuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "LOCAL_VERTICAL_SLICE: {self:?}")
    }
}

impl std::error::Error for LocalRuntimeError {}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;
    use ku_core::foundation::{
        decode_feed_inception, decode_knowledge_event, decode_knowledge_object, AcceptedInput,
        AffordanceSemantics, AssemblyLineageId, Budget, CarrierKind, CarrierProfile, DeviceId,
        FeedInception, InMemoryMappingBackend, InventoryRecordKind, KnowledgeEventEnvelope,
        KnownObjectKind, NamespaceCommitment, ReceptorAcceptanceProfile, ReceptorCardinality,
        ReceptorPlacement, ResolutionAction, ResolutionActionPayload, Selector, SelectorPurpose,
        SignedFeedInception, SourceSpan, StatementFrame, StatementId, StatementLocator,
        StatementQualifiers, TermRef, UnknownConstraintPolicy, RECEPTOR_RESOLUTION_ACTION_KIND,
        RECEPTOR_RESOLUTION_EVENT_TYPE,
    };
    use ku_encoder::{
        AffordanceExtractionDraft, ConstraintCoverage, ExplicitAffordanceDraft, ReceptorEncoder,
        ReceptorEncodingDraft, ReceptorEncodingOutcome, ReceptorOriginDraft,
        RuleBasedAffordanceExtractor,
    };
    use ku_kql::vnext_standing_need::{
        MappingViewRecord, MinimalKnowledgeViews, ReceptorResolutionProjection,
        RedbStandingNeedBackend, StandingNeed, StandingNeedStore,
    };

    use super::*;

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn concept(byte: u8) -> ku_core::foundation::ConceptCcid {
        ku_core::foundation::ConceptCcid::from_bytes([byte; 16])
    }

    fn frame() -> StatementFrame {
        StatementFrame {
            statement_id: StatementId(0),
            operator_or_predicate: concept(30),
            arguments: vec![TermRef::Concept(concept(31))],
            constraints: Vec::new(),
            qualifiers: StatementQualifiers::default(),
        }
    }

    fn empty_frames() -> SemanticFrameSet {
        SemanticFrameSet {
            statements: Vec::new(),
        }
    }

    fn receptor_and_reference() -> (ReceptorDefinition, ObjectReference) {
        let source = reference(1);
        let draft = ReceptorEncodingDraft {
            role: Some(concept(2)),
            expected_types: vec![concept(3)],
            hard_constraints: Vec::new(),
            cardinality: ReceptorCardinality::new(1, Some(1)).unwrap(),
            origin: ReceptorOriginDraft::Declared {
                source: StatementLocator {
                    object: source.clone(),
                    statement_index: 0,
                },
                source_span: Some(SourceSpan {
                    source,
                    start: 0,
                    end: 12,
                }),
            },
            acceptance: Some(ReceptorAcceptanceProfile {
                policy: reference(4),
                required_evidence_kinds: Vec::new(),
                unknown_constraint_policy: UnknownConstraintPolicy::KeepUnresolved,
            }),
            constraint_coverage: ConstraintCoverage::CompleteForSource,
            requested_disclosure: Some(DisclosureClass::Public),
            declared_limitations: Vec::new(),
        };
        let ReceptorEncodingOutcome::Encoded(encoded) = ReceptorEncoder.encode(draft).unwrap()
        else {
            panic!("fixture receptor must encode")
        };
        (
            encoded.definition().clone(),
            ObjectReference::new(0, encoded.cid().into_bytes()),
        )
    }

    fn affordance() -> (KnowledgeAffordance, ObjectReference) {
        let empty = empty_frames();
        let semantics = AffordanceSemantics {
            preconditions: empty.clone(),
            outputs: SemanticFrameSet {
                statements: vec![frame()],
            },
            effects: empty.clone(),
            properties: empty.clone(),
            invariants: empty.clone(),
            operating_conditions: empty.clone(),
            limits: empty,
        };
        let extracted = RuleBasedAffordanceExtractor::new(reference(80), reference(81), 1)
            .unwrap()
            .extract(AffordanceExtractionDraft::Explicit(
                ExplicitAffordanceDraft {
                    sources: vec![reference(10)],
                    offered_roles: vec![concept(2)],
                    accepted_inputs: vec![AcceptedInput {
                        receptor_definition: reference(11),
                        role: concept(3),
                        required: true,
                    }],
                    semantics,
                    abstraction_patterns: Vec::new(),
                    author_claims: vec![StatementLocator {
                        object: reference(10),
                        statement_index: 0,
                    }],
                },
            ))
            .unwrap();
        (
            extracted.affordance().clone(),
            ObjectReference::new(0, extracted.cid().into_bytes()),
        )
    }

    fn assembly(receptor: ObjectReference) -> (FrontierAssemblyManifest, ObjectCid, PlacementId) {
        let placement = PlacementId::from_bytes([20; 32]);
        let assembly = FrontierAssemblyManifest {
            lineage: AssemblyLineageId::from_bytes([21; 32]),
            revision: 0,
            predecessor: None,
            source_objects: vec![reference(1)],
            placements: vec![ReceptorPlacement {
                placement_id: placement,
                receptor_definition: receptor,
                cardinality: ReceptorCardinality::new(1, Some(1)).unwrap(),
                required: true,
                local_context: empty_frames(),
                resolution_policy_override: None,
            }],
            default_resolution_policy: reference(4),
        };
        let cid = assembly
            .to_knowledge_object(DisclosureClass::LocalOnly)
            .unwrap()
            .encode(ku_core::foundation::ResourceProfile::ObjectV1)
            .unwrap()
            .1;
        (assembly, cid, placement)
    }

    fn selector() -> Selector {
        Selector {
            purpose: SelectorPurpose::PublicKnowledgeExchange,
            namespace: NamespaceCommitment::derive(b"local-vertical-slice", [1; 32]).unwrap(),
            record_kinds: vec![InventoryRecordKind::Object],
            object_kinds: vec![ku_core::foundation::KNOWLEDGE_AFFORDANCE_KIND],
            disclosure_classes: vec![DisclosureClass::Public],
            frontier: Vec::new(),
            budget: Budget::new(32, 1 << 20, 10_000, 8).unwrap(),
            carrier: CarrierProfile {
                kind: CarrierKind::InMemory,
                max_frame_bytes: 64 * 1024,
                max_bundle_bytes: 1 << 20,
                store_carry_forward: false,
                bidirectional: true,
            },
        }
    }

    fn resolution_event(
        target: ResolutionTarget,
        policy: ObjectReference,
        mapping: MappingKernelCid,
    ) -> ValidatedResolutionEvent {
        let payload = ResolutionActionPayload {
            target,
            action: ResolutionAction::AdoptBinding { mapping },
            receptor_claim: None,
            acceptance_evidence: vec![reference(60)],
            resolution_policy: policy,
            observed_frontier: [22; 32],
        };
        let action = payload
            .to_knowledge_object(DisclosureClass::LocalOnly)
            .unwrap();
        let (action_bytes, action_cid) = action
            .encode(ku_core::foundation::ResourceProfile::ObjectV1)
            .unwrap();
        let action = decode_knowledge_object(
            &action_bytes,
            ku_core::foundation::ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(RECEPTOR_RESOLUTION_ACTION_KIND, 1)],
            &[],
        )
        .unwrap();

        let key = SigningKey::from_bytes(&[7; 32]);
        let inception = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"local-resolution", [8; 32]).unwrap(),
            0,
            DeviceId::from_bytes([9; 32]),
        );
        let signed: SignedFeedInception = inception.sign(&key).unwrap();
        let author = decode_feed_inception(&signed.encode().unwrap()).unwrap();
        let mut event = KnowledgeEventEnvelope::new(
            RECEPTOR_RESOLUTION_EVENT_TYPE,
            author.feed_id,
            0,
            DisclosureClass::LocalOnly,
            [61; 32],
        );
        event.payload_refs = vec![ObjectReference::new(0, action_cid.into_bytes())];
        let bytes = event.sign(&author, &key).unwrap().encode().unwrap().0;
        let event =
            decode_knowledge_event(&bytes, &author, &[RECEPTOR_RESOLUTION_EVENT_TYPE]).unwrap();
        ValidatedResolutionEvent::bind(&event, &action).unwrap()
    }

    #[test]
    fn offline_full_flow_crosses_restart_and_keeps_boundaries_separate() {
        let (receptor, receptor_reference) = receptor_and_reference();
        let (affordance, affordance_reference) = affordance();
        let (assembly, assembly_cid, placement) = assembly(receptor_reference.clone());
        let before_restart = LocalVerticalSlice::new(
            &assembly,
            assembly_cid,
            placement,
            [22; 32],
            InMemoryMappingBackend::default(),
        )
        .unwrap();
        let query = before_restart
            .build_query_definition(
                &receptor,
                SemanticFrameSet {
                    statements: vec![frame()],
                },
                reference(40),
                reference(41),
            )
            .unwrap();
        let query_cid = query.private_cid().unwrap();
        let selector = selector();
        let standing_need = StandingNeed::new_local(
            receptor_reference.clone(),
            query_cid,
            selector.cid().unwrap(),
            reference(42),
            [22; 32],
        );

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("standing-needs.redb");
        let need_id;
        {
            let store = StandingNeedStore::new(RedbStandingNeedBackend::open(&path).unwrap());
            need_id = store.put(&standing_need).unwrap().0;
        }
        drop(before_restart);
        let reopened_need = {
            let store = StandingNeedStore::new(RedbStandingNeedBackend::open(&path).unwrap());
            store.get(need_id).unwrap().unwrap()
        };
        assert_eq!(reopened_need, standing_need);

        let mut runtime = LocalVerticalSlice::new(
            &assembly,
            assembly_cid,
            placement,
            [22; 32],
            InMemoryMappingBackend::default(),
        )
        .unwrap();
        let candidate = runtime
            .propose(LocalCandidateInput {
                receptor: &receptor,
                required_semantics: &query.need.goal,
                affordance_reference: affordance_reference.clone(),
                affordance: &affordance,
                generator: reference(50),
                derivation_rule: Some(reference(51)),
                evidence: vec![reference(52)],
                index_commitment: None,
                rule_commitment: Some(reference(53)),
                metrics: MatcherMetricConcepts {
                    structural_fit: concept(54),
                    constraint_fit: concept(55),
                },
                unmapped_reason: concept(56),
                source_frontier: EventCid::from_bytes([57; 32]),
                created_at_evaluation: 1,
                expires_after_evaluations: 100,
            })
            .unwrap();
        let LocalCandidateOutcome::Quarantined { proposal_id, .. } = candidate else {
            panic!("compatible evidence must produce a proposal")
        };
        assert!(!runtime.proposal_store_is_executable());

        let mapping = runtime.proposal(proposal_id).unwrap().kernel_id().unwrap();
        let mut disclosures = ReferenceDisclosureIndex::default();
        for reference in [
            receptor_reference,
            affordance_reference,
            reference(50),
            reference(51),
            reference(52),
        ] {
            disclosures
                .declare(&reference, DisclosureClass::LocalOnly)
                .unwrap();
        }
        let materialized = runtime
            .materialize(
                LocalMaterializationRequest {
                    proposal_id,
                    current_evaluation: 2,
                    intent: MaterializationIntent::DurableUse,
                    authorization_ref: None,
                    destination: DisclosureClass::LocalOnly,
                    idempotency_key: [58; 32],
                    requester: ActorId::from_bytes([59; 32]),
                    authority: MaterializationAuthority::Authorized,
                },
                &disclosures,
            )
            .unwrap();
        assert_eq!(
            runtime.resolution_view().unwrap().state,
            ResolutionState::Open
        );
        assert!(runtime.is_mapping_materialized(mapping).unwrap());

        let event = resolution_event(
            runtime.target(),
            runtime.resolution_policy().clone(),
            mapping,
        );
        runtime
            .apply_resolution(event.clone(), ResolutionAuthority::Unauthorized, None)
            .unwrap();
        assert_eq!(
            runtime.resolution_view().unwrap().state,
            ResolutionState::Open
        );
        runtime
            .apply_resolution(
                event,
                ResolutionAuthority::Authorized,
                Some(BindingAcceptance::Satisfied),
            )
            .unwrap();
        let resolution = runtime.resolution_view().unwrap();
        assert!(runtime.is_satisfied_relative().unwrap());

        let views = MinimalKnowledgeViews::rebuild(
            &[reopened_need],
            &[ReceptorResolutionProjection {
                receptor_definition: runtime.receptor_reference().clone(),
                view: resolution,
            }],
            &[MappingViewRecord {
                kernel: materialized.kernel_cid,
                envelope: materialized.envelope_cid,
                disclosure: materialized.destination,
            }],
        )
        .unwrap();
        assert!(views.mapping(mapping).is_some());
        assert_eq!(views.adopted_targets(mapping), 1);
    }
}
