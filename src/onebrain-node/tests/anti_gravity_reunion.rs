use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use ed25519_dalek::SigningKey;
use ku_core::foundation::{
    decode_feed_inception, decode_knowledge_event, decode_knowledge_object, AcceptedInput,
    AffordanceOrigin, AffordanceSemantics, AssemblyLineageId, AssessedExerciseEvidence,
    BindingAcceptance, Budget, CarrierKind, CarrierProfile, ComparisonOperator, ConceptCcid,
    ConstraintEvaluation, ConstraintExpression, CoverageBasis, CoverageLimitation,
    CoverageStatement, CoverageStatus, DeviceId, DisclosureClass, EventCid, ExerciseAuthority,
    ExerciseEvidence, ExerciseEvidencePath, ExerciseRecordOutcome, FeedInception,
    FrontierAssemblyManifest, InMemoryMappingBackend, InventoryRecordKind, KnowledgeAffordance,
    KnowledgeEventEnvelope, KnownObjectKind, MaterializationAuthority, MaterializationIntent,
    MaterializationOutcome, NamespaceCommitment, ObjectCid, ObjectReference, PlacementId,
    ReceptorAcceptanceProfile, ReceptorCardinality, ReceptorDefinition, ReceptorOrigin,
    ReceptorPlacement, ReferenceDisclosureIndex, ResolutionAction, ResolutionActionPayload,
    ResolutionApplyOutcome, ResolutionAuthority, ResolutionState, ResolutionTarget,
    ResourceProfile, Selector, SelectorPurpose, SemanticFrameSet, SignedFeedInception,
    StatementFrame, StatementId, StatementLocator, StatementQualifiers, TermRef, TypedConstraint,
    UnknownConstraintPolicy, UseEvidencePayload, UseMode, ValidatedResolutionEvent,
    ValidatedUseEvidenceEvent, KNOWLEDGE_AFFORDANCE_KIND, RECEPTOR_DEFINITION_KIND,
    RECEPTOR_RESOLUTION_ACTION_KIND, RECEPTOR_RESOLUTION_EVENT_TYPE, USE_EVIDENCE_EVENT_TYPE,
    USE_EVIDENCE_KIND,
};
use ku_kql::vnext_matcher::MatcherMetricConcepts;
use ku_kql::vnext_proposal::ProposalQuarantine;
use ku_kql::vnext_query::{
    QueryChannel, QueryLimitation, QueryResultBatch, QueryResultRef, QueryRun, QueryWorkItem,
};
use ku_kql::vnext_reunion::{
    LocalNeedTarget, ReunionBudget, ReunionFrontier, ValidatedRemoteAffordance,
};
use ku_kql::vnext_standing_need::{RedbStandingNeedBackend, StandingNeed, StandingNeedStore};
use ku_net::vnext_bridge_merge::{BridgePathId, MultiBridgeInbox};
use ku_net::vnext_carrier::{
    CarrierRecord, DeliveryInjection, DeterministicCarrier, FileBundleCarrier, InMemoryCarrier,
};
use ku_net::vnext_reconciliation::{
    BoundPayloadFrame, PayloadSinkOutcome, ReceiverState, ValidateThenAcceptSink,
};
use ku_net::vnext_reconciliation_journal::{
    JournaledReconciliationSession, ReconciliationJournalBackend, ReconciliationJournalConfig,
};
use onebrain_node::{
    DeterministicReunionTrace, LocalMaterializationRequest, LocalVerticalSlice, ReunionTraceEntry,
    ReunionTracePhase,
};
use onebrain_protocol::{
    bind_reconciliation_message, encode_reconciliation_message, ReconcileManifestEntry,
    ReconcileManifestKind, ReconciliationBody, ReconciliationBudget, ReconciliationContext,
    ReconciliationResumeMode, ReconciliationSummaryMethod,
};

fn concept(byte: u8) -> ConceptCcid {
    ConceptCcid::from_bytes([byte; 16])
}

fn reference(byte: u8) -> ObjectReference {
    ObjectReference::new(0, [byte; 32])
}

fn empty_frames() -> SemanticFrameSet {
    SemanticFrameSet { statements: vec![] }
}

fn frames(marker: u8) -> SemanticFrameSet {
    SemanticFrameSet {
        statements: vec![StatementFrame {
            statement_id: StatementId(1),
            operator_or_predicate: concept(30),
            arguments: vec![TermRef::Concept(concept(marker))],
            constraints: vec![TypedConstraint {
                expression: ConstraintExpression::Compare {
                    left: TermRef::Concept(concept(61)),
                    operator: ComparisonOperator::Equal,
                    right: TermRef::Concept(concept(61)),
                },
                required: true,
            }],
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

fn affordance(source: u8) -> KnowledgeAffordance {
    let empty = empty_frames();
    KnowledgeAffordance {
        sources: vec![reference(source)],
        offered_roles: vec![concept(1)],
        accepted_inputs: vec![AcceptedInput {
            receptor_definition: reference(21),
            role: concept(2),
            required: true,
        }],
        semantics: AffordanceSemantics {
            preconditions: empty.clone(),
            outputs: frames(60),
            effects: empty.clone(),
            properties: empty.clone(),
            invariants: empty.clone(),
            operating_conditions: empty.clone(),
            limits: empty,
        },
        abstraction_patterns: vec![],
        origin: AffordanceOrigin::Explicit {
            claims: vec![StatementLocator {
                object: reference(source),
                statement_index: 0,
            }],
        },
    }
}

fn public_affordance(source: u8) -> (KnowledgeAffordance, Vec<u8>, ObjectCid) {
    let affordance = affordance(source);
    let (bytes, cid) = affordance
        .to_knowledge_object(DisclosureClass::Public)
        .unwrap()
        .encode(ResourceProfile::ObjectV1)
        .unwrap();
    (affordance, bytes, cid)
}

fn validated_remote(affordance: KnowledgeAffordance, bytes: &[u8]) -> ValidatedRemoteAffordance {
    let object = decode_knowledge_object(
        bytes,
        ResourceProfile::ObjectV1,
        &[KnownObjectKind::new(KNOWLEDGE_AFFORDANCE_KIND, 1)],
        &[],
    )
    .unwrap();
    ValidatedRemoteAffordance::from_public_object(&object, affordance).unwrap()
}

fn public_selector() -> Selector {
    Selector {
        purpose: SelectorPurpose::PublicKnowledgeExchange,
        namespace: NamespaceCommitment::derive(b"anti-gravity-public-fixture", [1; 32]).unwrap(),
        record_kinds: vec![InventoryRecordKind::Object],
        object_kinds: vec![KNOWLEDGE_AFFORDANCE_KIND],
        disclosure_classes: vec![DisclosureClass::Public],
        frontier: vec![EventCid::from_bytes([2; 32])],
        budget: Budget::new(32, 1 << 20, 10_000, 8).unwrap(),
        carrier: CarrierProfile {
            kind: CarrierKind::InMemory,
            max_frame_bytes: 64 * 1024,
            max_bundle_bytes: 1 << 20,
            store_carry_forward: true,
            bidirectional: true,
        },
    }
}

fn context() -> ReconciliationContext {
    let selector = public_selector();
    ReconciliationContext {
        authenticated_transcript: [3; 32],
        selector: selector.cid().unwrap(),
        namespace: selector.namespace,
        disclosure: DisclosureClass::Public,
        summary_method: ReconciliationSummaryMethod::RadixForest256V1,
        budget: ReconciliationBudget {
            max_summary_nodes: 32,
            max_diff_ranges: 32,
            max_manifest_entries: 32,
            max_payload_bytes: 1 << 20,
        },
        resume_mode: ReconciliationResumeMode::BoundTokenV1,
    }
}

fn carrier_records() -> (Vec<CarrierRecord>, Vec<[u8; 32]>) {
    let (_, partial, partial_cid) = public_affordance(70);
    let (_, satisfied, satisfied_cid) = public_affordance(71);
    let frames = [partial, satisfied]
        .into_iter()
        .map(|bytes| {
            BoundPayloadFrame::new(&context(), ReconcileManifestKind::Object, bytes).unwrap()
        })
        .collect::<Vec<_>>();
    let mut entries = frames
        .iter()
        .map(|frame| ReconcileManifestEntry {
            kind: frame.kind,
            cid: frame.cid,
            canonical_length: frame.canonical_bytes.len() as u64,
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| (entry.kind as u64, entry.cid));
    let manifest =
        bind_reconciliation_message(context(), 1, ReconciliationBody::Manifest { entries })
            .unwrap();
    let mut records = vec![CarrierRecord::reconciliation_message(
        &encode_reconciliation_message(&manifest).unwrap(),
    )
    .unwrap()];
    records.extend(frames.iter().cloned().map(CarrierRecord::BoundPayload));

    // Same claimed CID with different bytes is retained as a transport variant
    // but rejected by the receiver before the validated store boundary.
    let mut corrupt = frames[0].clone();
    corrupt.canonical_bytes.push(0);
    records.push(CarrierRecord::BoundPayload(corrupt));
    let mut expected = vec![partial_cid.into_bytes(), satisfied_cid.into_bytes()];
    expected.sort();
    (records, expected)
}

#[derive(Clone, Default)]
struct SharedJournal(Arc<Mutex<BTreeMap<[u8; 32], Vec<u8>>>>);

impl ReconciliationJournalBackend for SharedJournal {
    fn load(&self, binding: &[u8; 32]) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.lock().unwrap().get(binding).cloned())
    }

    fn store_atomically(&self, binding: &[u8; 32], bytes: &[u8]) -> Result<(), String> {
        self.0.lock().unwrap().insert(*binding, bytes.to_vec());
        Ok(())
    }

    fn compare_and_swap(
        &self,
        binding: &[u8; 32],
        expected: &[u8],
        replacement: &[u8],
    ) -> Result<bool, String> {
        let mut snapshots = self.0.lock().unwrap();
        let Some(current) = snapshots.get(binding) else {
            return Ok(false);
        };
        if current.as_slice() != expected {
            return Ok(false);
        }
        snapshots.insert(*binding, replacement.to_vec());
        Ok(true)
    }
}

#[derive(Clone, Default)]
struct ValidatedPublicSink {
    objects: Arc<Mutex<BTreeMap<[u8; 32], Vec<u8>>>>,
    insertions: Arc<Mutex<u64>>,
}

impl ValidateThenAcceptSink for ValidatedPublicSink {
    fn validate_then_accept(
        &mut self,
        kind: ReconcileManifestKind,
        cid: [u8; 32],
        bytes: &[u8],
    ) -> Result<PayloadSinkOutcome, String> {
        if kind != ReconcileManifestKind::Object {
            return Ok(PayloadSinkOutcome::RejectedInvalid);
        }
        let object = match decode_knowledge_object(
            bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(KNOWLEDGE_AFFORDANCE_KIND, 1)],
            &[],
        ) {
            Ok(object) if object.cid().into_bytes() == cid => object,
            _ => return Ok(PayloadSinkOutcome::RejectedInvalid),
        };
        if object.disclosure() != DisclosureClass::Public || object.is_opaque() {
            return Ok(PayloadSinkOutcome::RejectedInvalid);
        }
        let mut objects = self.objects.lock().unwrap();
        if objects.contains_key(&cid) {
            return Ok(PayloadSinkOutcome::AlreadyPresent);
        }
        objects.insert(cid, bytes.to_vec());
        *self.insertions.lock().unwrap() += 1;
        Ok(PayloadSinkOutcome::ValidatedStored)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NetworkOutcome {
    accepted: Vec<[u8; 32]>,
    stored: BTreeMap<[u8; 32], Vec<u8>>,
    semantic_digest: [u8; 32],
    trace_digest: [u8; 32],
    insertions: u64,
    state: ReceiverState,
}

fn run_network(bridges: usize, file_bundle: bool) -> NetworkOutcome {
    let (source, expected) = carrier_records();
    let records = if file_bundle {
        let directory = tempfile::tempdir().unwrap();
        let mut carrier = FileBundleCarrier::open(directory.path().join("reunion.obp")).unwrap();
        for record in &source {
            carrier.enqueue(record.clone()).unwrap();
        }
        drop(carrier);
        FileBundleCarrier::open(directory.path().join("reunion.obp"))
            .unwrap()
            .deliver(&DeliveryInjection::default())
            .unwrap()
    } else {
        let mut carrier = InMemoryCarrier::default();
        for record in &source {
            carrier.enqueue(record.clone()).unwrap();
        }
        carrier.deliver(&DeliveryInjection::default()).unwrap()
    };

    let mut inbox = MultiBridgeInbox::new(context());
    for bridge in 0..bridges {
        let path = BridgePathId::from_bytes([(bridge + 1) as u8; 32]);
        for record in &records {
            match record {
                CarrierRecord::ReconciliationMessage(bytes) => {
                    inbox.ingest_message(path, bytes).unwrap();
                }
                CarrierRecord::BoundPayload(frame) => {
                    inbox.ingest_payload(path, frame.clone()).unwrap();
                }
            }
        }
    }
    let semantic_digest = inbox.semantic_delivery_digest();
    assert!(!inbox.grants_authority());

    let backend = SharedJournal::default();
    let sink = ValidatedPublicSink::default();
    let mut session = JournaledReconciliationSession::open(
        backend.clone(),
        context(),
        ReconciliationJournalConfig {
            max_retries_per_record: 4,
            max_inflight_bytes: 1 << 20,
        },
        sink.clone(),
    )
    .unwrap();
    let first_delivery = inbox.deliver(&mut session);
    assert_eq!(first_delivery.errors, 0);
    assert!(first_delivery.payload_outcomes.iter().any(|(_, outcome)| {
        matches!(
            outcome,
            ku_net::vnext_reconciliation::PayloadIngestOutcome::Rejected(
                ku_net::vnext_reconciliation::PayloadRejectReason::ContentCid
                    | ku_net::vnext_reconciliation::PayloadRejectReason::UndeclaredLength
            )
        )
    }));
    assert_eq!(session.accepted_cids(), expected);
    drop(session);

    // Restart, then replay each canonical bundle record 1,000 times. The sink
    // and journal must retain a single validated insertion per public object.
    let mut replay_carrier = InMemoryCarrier::default();
    for record in &source {
        replay_carrier.enqueue(record.clone()).unwrap();
    }
    let replay = replay_carrier
        .deliver(&DeliveryInjection {
            copies_per_record: 1_000,
            ..DeliveryInjection::default()
        })
        .unwrap();
    let mut replay_inbox = MultiBridgeInbox::new(context());
    for record in replay {
        match record {
            CarrierRecord::ReconciliationMessage(bytes) => {
                replay_inbox
                    .ingest_message(BridgePathId::from_bytes([99; 32]), &bytes)
                    .unwrap();
            }
            CarrierRecord::BoundPayload(frame) => {
                replay_inbox
                    .ingest_payload(BridgePathId::from_bytes([99; 32]), frame)
                    .unwrap();
            }
        }
    }
    let mut reopened = JournaledReconciliationSession::open(
        backend,
        context(),
        ReconciliationJournalConfig {
            max_retries_per_record: 4,
            max_inflight_bytes: 1 << 20,
        },
        sink.clone(),
    )
    .unwrap();
    assert_eq!(replay_inbox.deliver(&mut reopened).errors, 0);
    assert_eq!(reopened.accepted_cids(), expected);
    assert_eq!(sink.objects.lock().unwrap().len(), 2);

    let mut trace = DeterministicReunionTrace::default();
    for cid in &expected {
        trace.record(ReunionTraceEntry {
            phase: ReunionTracePhase::ValidatedAcceptance,
            subject: *cid,
            outcome: 1,
        });
    }
    let insertions = *sink.insertions.lock().unwrap();
    let stored = sink.objects.lock().unwrap().clone();
    NetworkOutcome {
        accepted: reopened.accepted_cids(),
        stored,
        semantic_digest,
        trace_digest: trace.digest(),
        insertions,
        state: reopened.state(),
    }
}

fn assembly(
    receptor_reference: ObjectReference,
) -> (FrontierAssemblyManifest, ObjectCid, PlacementId) {
    let placement = PlacementId::from_bytes([20; 32]);
    let manifest = FrontierAssemblyManifest {
        lineage: AssemblyLineageId::from_bytes([21; 32]),
        revision: 0,
        predecessor: None,
        source_objects: vec![reference(10)],
        placements: vec![ReceptorPlacement {
            placement_id: placement,
            receptor_definition: receptor_reference,
            cardinality: ReceptorCardinality::new(1, Some(1)).unwrap(),
            required: true,
            local_context: empty_frames(),
            resolution_policy_override: None,
        }],
        default_resolution_policy: reference(11),
    };
    let cid = manifest
        .to_knowledge_object(DisclosureClass::LocalOnly)
        .unwrap()
        .encode(ResourceProfile::ObjectV1)
        .unwrap()
        .1;
    (manifest, cid, placement)
}

fn validated_receptor_reference(receptor: &ReceptorDefinition) -> ObjectReference {
    let (_, cid) = receptor
        .to_knowledge_object(DisclosureClass::LocalOnly)
        .unwrap()
        .encode(ResourceProfile::ObjectV1)
        .unwrap();
    ObjectReference::new(RECEPTOR_DEFINITION_KIND.0, cid.into_bytes())
}

fn local_target(need: StandingNeed, receptor: ReceptorDefinition) -> LocalNeedTarget {
    LocalNeedTarget {
        need,
        receptor,
        required_semantics: frames(60),
        local_context: empty_frames(),
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
        expires_after_evaluations: 100,
    }
}

fn resolution_event(
    sequence: u64,
    target: ResolutionTarget,
    policy: ObjectReference,
    action: ResolutionAction,
    parents: Vec<EventCid>,
) -> ValidatedResolutionEvent {
    let payload = ResolutionActionPayload {
        target,
        action,
        receptor_claim: None,
        acceptance_evidence: vec![reference(60)],
        resolution_policy: policy,
        observed_frontier: [22; 32],
    };
    let action = payload
        .to_knowledge_object(DisclosureClass::LocalOnly)
        .unwrap();
    let (action_bytes, action_cid) = action.encode(ResourceProfile::ObjectV1).unwrap();
    let action = decode_knowledge_object(
        &action_bytes,
        ResourceProfile::ObjectV1,
        &[KnownObjectKind::new(RECEPTOR_RESOLUTION_ACTION_KIND, 1)],
        &[],
    )
    .unwrap();
    let key = SigningKey::from_bytes(&[7; 32]);
    let inception = FeedInception::new(
        *key.verifying_key().as_bytes(),
        NamespaceCommitment::derive(b"anti-gravity-resolution", [8; 32]).unwrap(),
        0,
        DeviceId::from_bytes([9; 32]),
    );
    let signed: SignedFeedInception = inception.sign(&key).unwrap();
    let author = decode_feed_inception(&signed.encode().unwrap()).unwrap();
    let mut event = KnowledgeEventEnvelope::new(
        RECEPTOR_RESOLUTION_EVENT_TYPE,
        author.feed_id,
        sequence,
        DisclosureClass::LocalOnly,
        [61; 32],
    );
    event.causal_parents = parents;
    event.payload_refs = vec![ObjectReference::new(0, action_cid.into_bytes())];
    let bytes = event.sign(&author, &key).unwrap().encode().unwrap().0;
    let event = decode_knowledge_event(&bytes, &author, &[RECEPTOR_RESOLUTION_EVENT_TYPE]).unwrap();
    ValidatedResolutionEvent::bind(&event, &action).unwrap()
}

#[test]
fn one_two_and_five_bridges_memory_and_file_reunion_have_one_semantic_outcome() {
    let one = run_network(1, false);
    let two = run_network(2, true);
    let five = run_network(5, false);
    assert_eq!(one.accepted, two.accepted);
    assert_eq!(one.accepted, five.accepted);
    assert_eq!(one.semantic_digest, two.semantic_digest);
    assert_eq!(one.semantic_digest, five.semantic_digest);
    assert_eq!(one.trace_digest, two.trace_digest);
    assert_eq!(one.trace_digest, five.trace_digest);
    assert_eq!(one.insertions, 2);
    assert_eq!(two.insertions, 2);
    assert_eq!(five.insertions, 2);
    // All network sessions are gone after `run_network`; accepted public KU
    // bytes remain independently decodable in the local validated store.
    for (cid, bytes) in &one.stored {
        let object = decode_knowledge_object(
            bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(KNOWLEDGE_AFFORDANCE_KIND, 1)],
            &[],
        )
        .unwrap();
        assert_eq!(object.cid().as_bytes(), cid);
        assert_eq!(object.disclosure(), DisclosureClass::Public);
    }
    assert!(matches!(
        one.state,
        ReceiverState::PartialInvalid { pending: 0, .. } | ReceiverState::ManifestBatchComplete
    ));
    assert!(!one.state.is_globally_complete());
}

#[test]
fn private_need_restart_delta_proposal_materialization_and_resolution_remain_separate() {
    let receptor = receptor();
    let receptor_reference = validated_receptor_reference(&receptor);
    let (assembly, assembly_cid, placement) = assembly(receptor_reference.clone());
    let mut runtime = LocalVerticalSlice::new(
        &assembly,
        assembly_cid,
        placement,
        [22; 32],
        InMemoryMappingBackend::default(),
    )
    .unwrap();
    let query = runtime
        .build_query_definition(&receptor, frames(60), reference(44), reference(45))
        .unwrap();
    assert_eq!(query.need.privacy, DisclosureClass::LocalOnly);
    let private_query_cid = query.private_cid().unwrap();
    let selector = public_selector();
    let need = StandingNeed::new_local(
        receptor_reference.clone(),
        private_query_cid,
        selector.cid().unwrap(),
        reference(46),
        [22; 32],
    );

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("anti-gravity-standing-need.redb");
    let need_id;
    {
        let store = StandingNeedStore::new(RedbStandingNeedBackend::open(&path).unwrap());
        need_id = store.put(&need).unwrap().0;
    }
    let reopened_need = {
        let store = StandingNeedStore::new(RedbStandingNeedBackend::open(&path).unwrap());
        store.get(need_id).unwrap().unwrap()
    };
    assert_eq!(reopened_need, need);
    let public_wire = carrier_records()
        .0
        .into_iter()
        .flat_map(|record| record.canonical_bytes().unwrap())
        .collect::<Vec<_>>();
    for private_identifier in [
        private_query_cid.into_bytes(),
        receptor_reference.cid,
        assembly_cid.into_bytes(),
        *need_id.as_bytes(),
    ] {
        assert!(!public_wire
            .windows(private_identifier.len())
            .any(|window| window == private_identifier));
    }

    let (partial_affordance, partial_bytes, _) = public_affordance(70);
    let partial_remote = validated_remote(partial_affordance, &partial_bytes);
    let target = local_target(reopened_need.clone(), receptor.clone());
    let mut frontier = ReunionFrontier::default();
    let mut quarantine = ProposalQuarantine::default();
    let report = frontier
        .join_affordance_delta(
            vec![partial_remote],
            std::slice::from_ref(&target),
            &mut quarantine,
            ReunionBudget {
                max_delta_objects: 8,
                max_pairs: 16,
                max_proposals: 8,
            },
        )
        .unwrap();
    assert_eq!(report.proposals.len(), 1);
    assert!(!frontier.exports_private_need_state());
    assert!(!quarantine.is_executable());
    assert_eq!(
        runtime.resolution_view().unwrap().state,
        ResolutionState::Open
    );

    let proposal = quarantine
        .get(report.proposals[0].proposal)
        .unwrap()
        .clone();
    assert!(!proposal.mapping_kernel.correspondences.is_empty());
    assert_eq!(proposal.constraints.len(), 1);
    assert_eq!(
        proposal.constraints[0].evaluation,
        ConstraintEvaluation::Satisfied
    );
    let proposal_id = runtime.import_reunion_proposal(proposal).unwrap();
    assert!(!runtime.proposal_store_is_executable());
    assert_eq!(
        runtime.resolution_view().unwrap().state,
        ResolutionState::Open
    );

    let mapping = runtime.proposal(proposal_id).unwrap().kernel_id().unwrap();
    let candidate = runtime.proposal(proposal_id).unwrap().candidate_objects[0].clone();
    let mut disclosures = ReferenceDisclosureIndex::default();
    for value in [
        receptor_reference.clone(),
        candidate,
        reference(34),
        reference(35),
        reference(36),
    ] {
        disclosures
            .declare(&value, DisclosureClass::LocalOnly)
            .unwrap();
    }
    let request = LocalMaterializationRequest {
        proposal_id,
        current_evaluation: 2,
        intent: MaterializationIntent::DurableUse,
        authorization_ref: None,
        destination: DisclosureClass::LocalOnly,
        idempotency_key: [80; 32],
        requester: ku_core::foundation::ActorId::from_bytes([81; 32]),
        authority: MaterializationAuthority::Authorized,
    };
    let first_materialized = runtime.materialize(request, &disclosures).unwrap();
    assert_eq!(
        runtime.resolution_view().unwrap().state,
        ResolutionState::Open
    );
    for _ in 0..1_000 {
        let replay = runtime.materialize(request, &disclosures).unwrap();
        assert_eq!(replay.kernel_cid, first_materialized.kernel_cid);
        assert_eq!(replay.envelope_cid, first_materialized.envelope_cid);
        assert_eq!(replay.destination, first_materialized.destination);
        assert_eq!(replay.outcome, MaterializationOutcome::IdempotentReplay);
    }
    assert!(runtime.is_mapping_materialized(mapping).unwrap());

    let partial_event = resolution_event(
        0,
        runtime.target(),
        runtime.resolution_policy().clone(),
        ResolutionAction::AdoptBinding { mapping },
        vec![],
    );
    assert_eq!(
        runtime
            .apply_resolution(
                partial_event.clone(),
                ResolutionAuthority::Authorized,
                Some(BindingAcceptance::Partial),
            )
            .unwrap(),
        ResolutionApplyOutcome::Added
    );
    assert_eq!(
        runtime.resolution_view().unwrap().state,
        ResolutionState::PartiallySatisfied
    );
    for _ in 0..1_000 {
        assert_eq!(
            runtime
                .apply_resolution(
                    partial_event.clone(),
                    ResolutionAuthority::Authorized,
                    Some(BindingAcceptance::Partial),
                )
                .unwrap(),
            ResolutionApplyOutcome::ExactReplay
        );
    }

    let (satisfied_affordance, satisfied_bytes, _) = public_affordance(71);
    let second = frontier
        .join_affordance_delta(
            vec![validated_remote(satisfied_affordance, &satisfied_bytes)],
            &[target],
            &mut quarantine,
            ReunionBudget {
                max_delta_objects: 8,
                max_pairs: 16,
                max_proposals: 8,
            },
        )
        .unwrap();
    let second_proposal = quarantine
        .get(second.proposals[0].proposal)
        .unwrap()
        .clone();
    let second_id = runtime.import_reunion_proposal(second_proposal).unwrap();
    let second_mapping = runtime.proposal(second_id).unwrap().kernel_id().unwrap();
    let second_candidate = runtime.proposal(second_id).unwrap().candidate_objects[0].clone();
    disclosures
        .declare(&second_candidate, DisclosureClass::LocalOnly)
        .unwrap();
    runtime
        .materialize(
            LocalMaterializationRequest {
                proposal_id: second_id,
                idempotency_key: [82; 32],
                ..request
            },
            &disclosures,
        )
        .unwrap();
    let satisfied_event = resolution_event(
        1,
        runtime.target(),
        runtime.resolution_policy().clone(),
        ResolutionAction::AdoptBinding {
            mapping: second_mapping,
        },
        vec![partial_event.event_cid()],
    );
    runtime
        .apply_resolution(
            satisfied_event,
            ResolutionAuthority::Authorized,
            Some(BindingAcceptance::Satisfied),
        )
        .unwrap();
    assert_eq!(
        runtime.resolution_view().unwrap().state,
        ResolutionState::SatisfiedRelative
    );

    let concurrent_reopen = resolution_event(
        2,
        runtime.target(),
        runtime.resolution_policy().clone(),
        ResolutionAction::Reopen,
        vec![partial_event.event_cid()],
    );
    runtime
        .apply_resolution(concurrent_reopen, ResolutionAuthority::Authorized, None)
        .unwrap();
    let concurrent = runtime.resolution_view().unwrap();
    assert_eq!(concurrent.state, ResolutionState::Concurrent);
    assert!(concurrent
        .branches
        .iter()
        .any(|branch| branch.state == ResolutionState::Open));
    assert!(concurrent
        .branches
        .iter()
        .any(|branch| branch.state == ResolutionState::SatisfiedRelative));
}

#[test]
fn transcript_privacy_corpus_boundary_and_partial_query_scope_are_machine_checked() {
    let selector = public_selector();
    selector.validate().unwrap();
    assert_eq!(selector.disclosure_classes, vec![DisclosureClass::Public]);
    assert_eq!(selector.object_kinds, vec![KNOWLEDGE_AFFORDANCE_KIND]);
    let mut private_selector = selector.clone();
    private_selector.disclosure_classes = vec![DisclosureClass::LocalOnly];
    assert!(private_selector.validate().is_err());

    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/specs/vnext/corpus/anti_gravity_v1.yaml"
    ));
    for required in [
        "scientific_status: fictional-thought-experiment",
        "independent_of_private_need: true",
        "AG-STRUCT-001",
        "AG-PARTIAL-002",
        "AG-ASSEMBLY-001",
        "AG-PRIVACY-001",
    ] {
        assert!(
            corpus.contains(required),
            "missing corpus oracle: {required}"
        );
    }
    for forbidden in [
        "private_need_ir",
        "receptor_id",
        "assembly_id",
        "user_id",
        "goal_cid",
        "goal_commitment",
        "commitment_opening",
        "commitment_nonce",
    ] {
        assert!(corpus.contains(&format!("- {forbidden}")));
    }

    let definition = ObjectCid::from_bytes([90; 32]);
    let run = QueryRun::new([91; 32], definition, selector.clone()).unwrap();
    let boundary = selector.cid().unwrap();
    let work = QueryWorkItem {
        work_id: [92; 32],
        run_id: [91; 32],
        channel: QueryChannel::ExactTypedIndex,
        boundary,
        budget: selector.budget,
        continuation: None,
    };
    let batch = QueryResultBatch {
        run_id: [91; 32],
        work_id: [92; 32],
        boundary,
        results: vec![QueryResultRef::Object(reference(70))],
        coverage: CoverageStatement {
            selector: boundary,
            assessed_frontier: selector.frontier.clone(),
            basis: CoverageBasis::ExactInventory,
            status: CoverageStatus::Partial,
            returned_records: 1,
            returned_bytes: 128,
            continuation: Some([93; 32]),
            limitations: vec![CoverageLimitation::FrontierIncomplete],
        },
        limitations: vec![QueryLimitation::FrontierIncomplete],
        continuation: Some([93; 32]),
    };
    batch.validate_for(&run, &work).unwrap();
    assert!(!batch.is_globally_complete());

    let message_bytes = match carrier_records().0.remove(0) {
        CarrierRecord::ReconciliationMessage(bytes) => bytes,
        CarrierRecord::BoundPayload(_) => unreachable!(),
    };
    let mut tampered = context();
    tampered.selector = ku_core::foundation::SelectorCid::from_bytes([94; 32]);
    let mut inbox = MultiBridgeInbox::new(tampered);
    assert!(inbox
        .ingest_message(BridgePathId::from_bytes([1; 32]), &message_bytes,)
        .is_err());

    let trace = DeterministicReunionTrace::default();
    assert!(!trace.includes_private_need_state());
    assert!(!trace.requires_obt());
    assert!(!trace.claims_global_completeness());
}

#[test]
fn signed_use_evidence_is_idempotent_and_never_becomes_truth_benefit_or_reward() {
    let payload = UseEvidencePayload {
        subjects: vec![reference(70)],
        mode: UseMode::Application,
        actor_class: concept(71),
        task_context_commitment: [72; 32],
        causal_role: concept(73),
        assembly: Some(reference(74)),
        mapping: None,
        outcome_observation: None,
        use_policy: reference(75),
        observed_frontier: [76; 32],
    };
    let object = payload
        .to_knowledge_object(DisclosureClass::LocalOnly)
        .unwrap();
    let (object_bytes, object_cid) = object.encode(ResourceProfile::ObjectV1).unwrap();
    let object = decode_knowledge_object(
        &object_bytes,
        ResourceProfile::ObjectV1,
        &[KnownObjectKind::new(USE_EVIDENCE_KIND, 1)],
        &[],
    )
    .unwrap();

    let key = SigningKey::from_bytes(&[77; 32]);
    let inception = FeedInception::new(
        *key.verifying_key().as_bytes(),
        NamespaceCommitment::derive(b"anti-gravity-use", [78; 32]).unwrap(),
        0,
        DeviceId::from_bytes([79; 32]),
    );
    let author = decode_feed_inception(&inception.sign(&key).unwrap().encode().unwrap()).unwrap();
    let mut event = KnowledgeEventEnvelope::new(
        USE_EVIDENCE_EVENT_TYPE,
        author.feed_id,
        0,
        DisclosureClass::LocalOnly,
        [80; 32],
    );
    event.payload_refs = vec![ObjectReference::new(0, object_cid.into_bytes())];
    let event_bytes = event.sign(&author, &key).unwrap().encode().unwrap().0;
    let event = decode_knowledge_event(&event_bytes, &author, &[USE_EVIDENCE_EVENT_TYPE]).unwrap();
    let evidence = ExerciseEvidence::Use(ValidatedUseEvidenceEvent::bind(&event, &object).unwrap());
    let assessed = AssessedExerciseEvidence {
        evidence: evidence.clone(),
        authority: ExerciseAuthority::Authorized,
    };
    let mut path = ExerciseEvidencePath::default();
    assert_eq!(path.record(assessed.clone()), ExerciseRecordOutcome::Added);
    for _ in 0..1_000 {
        assert_eq!(
            path.record(assessed.clone()),
            ExerciseRecordOutcome::ExactReplay
        );
    }
    assert_eq!(path.unique_event_count(), 1);
    assert!(!evidence.establishes_truth());
    assert!(!evidence.establishes_benefit());
    assert!(!evidence.is_reward_instruction());
}
