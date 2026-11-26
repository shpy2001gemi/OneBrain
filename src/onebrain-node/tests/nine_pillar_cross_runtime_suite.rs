//! Comprehensive 9-Pillar (`P1`, `P2`, `P3`, `P4`, `P5`, `P6`, `P7`, `P8`, `P10`)
//! cross-runtime integration suite for `onebrain-node` (Batch C1).

#![cfg(feature = "vnext-outbound-first")]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ed25519_dalek::{Signer as _, SigningKey};
use ku_ai::vnext_kge_recall::FedROperatorMatrixV1;
use ku_core::blob_store::BlobCid;
use ku_core::concept_lifecycle::{
    AuthoringConceptResolution, ConceptLifecycleRecord, ConceptLifecycleTable, ConceptRedirect,
};
use ku_core::concept_registry::{ConceptCategory, ConceptLookup, ResolveResult, ResolvedConcept};
use ku_core::foundation::personal_mesh::{
    x25519_public_from_secret, PersonalMeshEpochDek, PersonalMeshMemberBindingV1,
    PersonalMeshPrincipals, PersonalMeshRecordClass,
};
use ku_core::foundation::scoped_concept::{attach_scoped_concept_bindings, ConceptDefinitionFrame};
use ku_core::foundation::supersession::{
    KnowledgeSupersessionPayload, RevisionRelation, KNOWLEDGE_SUPERSESSION_EVENT_TYPE,
};
use ku_core::foundation::{
    ActorId, ActorRevocation, AffectedPrincipal, AffectedScope, AttributionStatus, ConceptCcid,
    DeviceId, DisclosureClass, EventCid, FeedEventSigner, FeedId, FeedInception,
    KnowledgeEventEnvelope, NamespaceCommitment, NodeId, ObjectCid, ObjectReference,
    OutcomeObservationPayload, OutcomeValence, PermitCid, PrivateVault, ReceptorAcceptanceProfile,
    ReceptorCardinality, ReceptorDefinition, ReceptorOrigin, RedbVerifiedBackend, ResourceProfile,
    SelectorCid, SemanticFrameSet, StatementFrame, StatementId, StatementLocator,
    StatementQualifiers, TermRef, UnknownConstraintPolicy, UseEvidencePayload, UseMode,
    ValidatedFeedInception, VaultKey, BENEFIT_EVIDENCE_EVENT_TYPE, BENEFIT_EVIDENCE_KIND,
    OUTCOME_OBSERVATION_EVENT_TYPE, OUTCOME_OBSERVATION_KIND, SEMANTIC_KERNEL_OBJECT_KIND,
    USE_EVIDENCE_EVENT_TYPE, USE_EVIDENCE_KIND,
};
use ku_kql::vnext_matcher::MatcherMetricConcepts;
use ku_kql::vnext_obkg_projection::{
    ObkgModelVersion, ObkgMultiPlaneConfig, ObkgMultiPlaneSources, ObkgSourceFrontier,
};
use ku_kql::vnext_private_need::LocalNeedVaultKey;
use ku_mediator::vnext_pam::{PamRecommendationActionKind, UNASSESSED_EPISTEMIC_LABEL};
use ku_net::vnext_reconciliation::{PayloadSinkOutcome, ValidateThenAcceptSink};
use ku_net::vnext_riblt::RibltRuntimeGate;
use ku_net::vnext_session::principal_node_id;
use onebrain_blob_store::blob_storage::{BlobStorage, BlobStorageConfig};
use onebrain_node::archive::OptionalNetworkFloorSnapshot;
use onebrain_node::kql::{
    KqlNeedCompiler, NeedCompileContext, NeedCompileError, NeedQuantityDraftV1,
    NeedStatementDraftV1, NeedTermDraftV1, SiUnitCatalog, StructuredNeedCompileRequestV1,
    COMPILER_PROFILE_ID,
};
use onebrain_node::vnext_delegated_jobs::{
    blind_commit_signing_preimage, blind_reveal_signing_preimage,
    compute_blind_reveal_output_digest, delegated_lease_signing_preimage,
};
use onebrain_node::vnext_remote_cognition::default_cognitive_execution_policy;
use onebrain_node::{
    AcceptedWorkNotice, BaseStorageOwnerId, BenefitClosureNotice, BlobAuthority,
    CausalEvidenceClosureEngine, CausalEvidencePublishPayload, CausalIngestDisposition,
    ConceptRegistryMode, CrossStoreRecoveryCoordinator, DatasetGenerationStore,
    DatasetPathResolver, DelegatedJobCoordinator, DelegatedJobError, DistributedKqlBudget,
    DistributedKqlRuntime, DurableRewardNoticeOutboxV2, KuCompositionRuntime,
    LocalEvictionDecision, NodeBlobTransferCoordinator, NodeConfig, NodeRegistryPullCoordinator,
    ObkgRuntimeService, ObtRuntimeCoordinator, ObtStoreHealth, OneBrainNode,
    OperationalCompactionPolicy, OperationalCompactionStore, OsPendingUploadIdSource,
    OutboundIntentState, OutboundOutbox, OutboundTransferIntent, PamHostBridge,
    PersonalMeshRuntime, PinRetentionReason, PinSetEvaluator, PrepareCausalEvidenceRequest,
    PublicUseEvidencePublisher, PublicUsePublishOutcome, RecoveryPhase, RemoteCognitionRuntime,
    RewardEvidenceNoticeV2, RewardObserveOutcome, SharedVNextValidatedSink,
    SurfaceCapabilityClaimsV1, SurfaceCapabilityError, SurfaceCapabilityRegistry, SurfaceKind,
    SurfaceScope, UnavailableValidatedBlobReferenceSource, VNextExtendedFeature,
    VNextExtendedFeatureConfig, VNextFeature, VNextFeatureConfig, VNextNetworkPolicy,
    VNextNetworkRuntime, VNextRuntimeLane, VNextValidatedSink, COGNITION_DB_FILENAME,
    DEFAULT_KU_COMPOSITION_DB_FILENAME, PERSONAL_MESH_DB_FILENAME,
};
use onebrain_obt::{
    AcceptedWorkKind, BlindCommitRevealProof, NoticeIngestOutcome, ObtAmount, ObtEconomicStatus,
    ObtRuntimeMode, RewardPolicyV1, ScopedNullifier,
};
use onebrain_protocol::pillar_wire::{
    BlindVerifyCommitWireV1, BlindVerifyRevealWireV1, DelegatedJobLeaseWireV1, WatchRootHintV1,
};
use onebrain_protocol::{
    sign_mailbox_delivery_receipt, sign_mailbox_stored_receipt, MailboxDeliveryReceiptV1,
    MailboxStoredReceipt, ReconcileManifestKind, RegistryReleaseDescriptorV1,
};

struct TestSigner {
    signing_key: SigningKey,
}

impl TestSigner {
    fn from_seed(seed: u8) -> Self {
        Self {
            signing_key: SigningKey::from_bytes(&[seed; 32]),
        }
    }

    fn pubkey(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    fn sign_raw(&self, message: &[u8]) -> [u8; 64] {
        self.signing_key.sign(message).to_bytes()
    }
}

impl FeedEventSigner for TestSigner {
    fn public_key(&self) -> [u8; 32] {
        self.pubkey()
    }

    fn sign_feed_event(&self, message: &[u8]) -> Result<[u8; 64], String> {
        Ok(self.signing_key.sign(message).to_bytes())
    }
}

fn make_feed(
    seed: u8,
    actor_byte: u8,
    device_byte: u8,
) -> (TestSigner, Vec<u8>, [u8; 32], ValidatedFeedInception) {
    let signer = TestSigner::from_seed(seed);
    let ns = NamespaceCommitment::derive(b"nine-pillar-feed", [actor_byte; 32]).unwrap();
    let inception = FeedInception::new(
        signer.pubkey(),
        ns,
        0,
        DeviceId::from_bytes([device_byte; 32]),
    );
    let signed = inception.sign(&signer.signing_key).unwrap();
    let bytes = signed.encode().unwrap();
    let validated = ku_core::foundation::decode_feed_inception(&bytes).unwrap();
    let feed_cid = ku_core::foundation::ReservedDomain::FeedInception.digest(&bytes);
    (signer, bytes, feed_cid, validated)
}

struct MockConceptRegistry {
    concepts: Vec<(String, ConceptCcid, ConceptCategory)>,
    lifecycle: ConceptLifecycleTable,
}

impl ConceptLookup for MockConceptRegistry {
    fn resolve(&self, label: &str) -> ResolveResult {
        let lower = label.trim().to_lowercase();
        for (idx, (name, ccid, category)) in self.concepts.iter().enumerate() {
            if name.to_lowercase() == lower {
                return ResolveResult::Found(ResolvedConcept {
                    ccid: *ccid.as_bytes(),
                    qid: (idx + 1) as u32,
                    canonical_name: name.clone(),
                    category: *category,
                });
            }
        }
        ResolveResult::NotFound
    }

    fn lifecycle_table(&self) -> Option<&ConceptLifecycleTable> {
        Some(&self.lifecycle)
    }
}

const fn assert_frozen_12_vnext_features(f: VNextFeature) -> usize {
    match f {
        VNextFeature::ObjectEventV1 => 0,
        VNextFeature::ObpRp => 1,
        VNextFeature::DistributedKqlOneHop => 2,
        VNextFeature::PublicUseEvidencePublish => 3,
        VNextFeature::DistributedPomvView => 4,
        VNextFeature::InventoryShadow => 5,
        VNextFeature::ProviderLease => 6,
        VNextFeature::Fidelity => 7,
        VNextFeature::RewardEvidenceExport => 8,
        VNextFeature::CheckpointGc => 9,
        VNextFeature::Riblt => 10,
        VNextFeature::LegacyAdapter => 11,
    }
}

#[tokio::test]
async fn default_node_startup_preserves_frozen_invariants_and_creates_zero_extra_redb_files() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = NodeConfig::default();
    cfg.data_dir = dir.path().to_path_buf();
    cfg.concept_registry_mode = ConceptRegistryMode::Disabled;

    // Frozen compatibility invariants:
    assert_eq!(
        assert_frozen_12_vnext_features(VNextFeature::LegacyAdapter),
        11
    );
    assert_eq!(VNextRuntimeLane::ALL.len(), 4);
    assert_ne!(
        onebrain_node::compiled_base_runtime_config()
            .version_status
            .compatibility
            .feature_set_digest
            .0,
        [0u8; 32]
    );

    let node = OneBrainNode::new(cfg).await.unwrap();
    assert_eq!(
        node.vnext_extended(),
        &VNextExtendedFeatureConfig::default()
    );
    for feature in VNextExtendedFeature::ALL {
        assert!(!node.vnext_extended().is_active(feature));
    }

    // Zero lazy Pillar `.redb` files exist on default startup!
    assert!(!dir.path().join(DEFAULT_KU_COMPOSITION_DB_FILENAME).exists());
    assert!(!dir.path().join(PERSONAL_MESH_DB_FILENAME).exists());
    assert!(!dir.path().join(COGNITION_DB_FILENAME).exists());
    assert!(!dir.path().join("obt-ledger-v1").join("obt.redb").exists());
    assert!(!dir.path().join("vnext_reward_outbox_v2.redb").exists());
}

#[test]
fn p1_scoped_concepts_supersession_and_ku_composition_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let verified_db = dir.path().join("verified.redb");
    let backend = RedbVerifiedBackend::open(&verified_db).unwrap();
    let shared_sink = SharedVNextValidatedSink::new(VNextValidatedSink::new(backend));
    let mut sink = shared_sink.clone();

    let signer = TestSigner::from_seed(0x11);
    let ns = NamespaceCommitment::derive(b"nine-pillar-p1-feed", [0x21; 32]).unwrap();
    let mut inception =
        FeedInception::new(signer.pubkey(), ns, 0, DeviceId::from_bytes([0x31; 32]));
    let feed_id = inception.feed_id().unwrap();

    // Register ActorRootDelegation so `KeyStateReducer` authorizes `feed_id` under `actor`!
    let root_key = SigningKey::from_bytes(&[0x91; 32]);
    let actor =
        ku_core::foundation::actor_id_from_root_key(root_key.verifying_key().to_bytes()).unwrap();
    let root_deleg = ku_core::foundation::ActorRootDelegation::new(
        root_key.verifying_key().to_bytes(),
        feed_id,
        DeviceId::from_bytes([0x31; 32]),
        None,
        1,
        10,
    )
    .unwrap()
    .sign(&root_key)
    .unwrap()
    .encode()
    .unwrap();
    let root_cid = ku_core::foundation::ReservedDomain::AuthorityEvent.digest(&root_deleg);
    inception.actor_delegation_ref = Some(root_cid);
    let feed_bytes = inception
        .sign(&signer.signing_key)
        .unwrap()
        .encode()
        .unwrap();
    let feed_cid = ku_core::foundation::ReservedDomain::FeedInception.digest(&feed_bytes);
    let feed = ku_core::foundation::decode_feed_inception(&feed_bytes).unwrap();

    assert_eq!(
        sink.validate_then_accept(ReconcileManifestKind::FeedInception, feed_cid, &feed_bytes)
            .unwrap(),
        PayloadSinkOutcome::ValidatedStored
    );
    assert_eq!(
        sink.validate_then_accept(ReconcileManifestKind::AuthorityEvent, root_cid, &root_deleg)
            .unwrap(),
        PayloadSinkOutcome::ValidatedStored
    );

    // 1. Scoped Concept missing defining SFS -> `DeferredMissingDependency`, then accepted!
    let def_frame = ConceptDefinitionFrame::new(
        "quantum_flux_sensor",
        "Cryogenic magnetic flux quantum sensor",
        vec![ConceptCcid::from_bytes([0x02; 16])],
        Vec::new(),
    )
    .unwrap();
    let (def_envelope, defining_cid, scoped_ccid, _sem_cid) = def_frame
        .to_knowledge_object(DisclosureClass::Public)
        .unwrap();

    let dependent_sfs = SemanticFrameSet {
        statements: vec![StatementFrame {
            statement_id: StatementId(0),
            operator_or_predicate: ConceptCcid::from_bytes([0x03; 16]),
            arguments: vec![
                TermRef::Concept(scoped_ccid),
                TermRef::Concept(ConceptCcid::from_bytes([0x04; 16])),
            ],
            constraints: Vec::new(),
            qualifiers: StatementQualifiers::default(),
        }],
    };
    let mut dep_envelope = dependent_sfs
        .to_knowledge_object(DisclosureClass::Public)
        .unwrap();
    attach_scoped_concept_bindings(&mut dep_envelope, &[(scoped_ccid, defining_cid)]).unwrap();
    let (dep_obj_bytes, dep_obj_cid) = dep_envelope.encode(ResourceProfile::ObjectV1).unwrap();

    // Dependent arrives BEFORE defining SFS -> deferred!
    assert_eq!(
        sink.validate_then_accept(
            ReconcileManifestKind::Object,
            dep_obj_cid.into_bytes(),
            &dep_obj_bytes
        )
        .unwrap(),
        PayloadSinkOutcome::DeferredMissingDependency
    );

    // Defining SFS arrives -> accepted, then dependent SFS is accepted!
    let (def_obj_bytes, actual_def_cid) = def_envelope.encode(ResourceProfile::ObjectV1).unwrap();
    assert_eq!(actual_def_cid, defining_cid);
    assert_eq!(
        sink.validate_then_accept(
            ReconcileManifestKind::Object,
            defining_cid.into_bytes(),
            &def_obj_bytes
        )
        .unwrap(),
        PayloadSinkOutcome::ValidatedStored
    );
    assert_eq!(
        sink.validate_then_accept(
            ReconcileManifestKind::Object,
            dep_obj_cid.into_bytes(),
            &dep_obj_bytes
        )
        .unwrap(),
        PayloadSinkOutcome::ValidatedStored
    );

    // 2. `KnowledgeSupersession` (`Kind 24` + `EventType 8`) accepted & reduced in lineage!
    let replacement_sfs = SemanticFrameSet {
        statements: vec![StatementFrame {
            statement_id: StatementId(0),
            operator_or_predicate: ConceptCcid::from_bytes([0x03; 16]),
            arguments: vec![
                TermRef::Concept(ConceptCcid::from_bytes([0x05; 16])),
                TermRef::Concept(ConceptCcid::from_bytes([0x04; 16])),
            ],
            constraints: Vec::new(),
            qualifiers: StatementQualifiers::default(),
        }],
    };
    let (rep_bytes, rep_cid) = replacement_sfs
        .to_knowledge_object(DisclosureClass::Public)
        .unwrap()
        .encode(ResourceProfile::ObjectV1)
        .unwrap();
    sink.validate_then_accept(
        ReconcileManifestKind::Object,
        rep_cid.into_bytes(),
        &rep_bytes,
    )
    .unwrap();

    shared_sink
        .register_supersession_object_author(dep_obj_cid, actor)
        .unwrap();
    let sup_payload = KnowledgeSupersessionPayload {
        predecessor_object_cid: dep_obj_cid,
        successor_object_cid: rep_cid,
        relation: RevisionRelation::CorrectsEncoding,
        rationale_ref: None,
    };
    let (sup_obj_bytes, sup_obj_cid) = sup_payload
        .to_knowledge_object(DisclosureClass::Public)
        .unwrap()
        .encode(ResourceProfile::ObjectV1)
        .unwrap();
    assert_eq!(
        sink.validate_then_accept(
            ReconcileManifestKind::Object,
            sup_obj_cid.into_bytes(),
            &sup_obj_bytes
        )
        .unwrap(),
        PayloadSinkOutcome::ValidatedStored
    );

    let mut sup_event = KnowledgeEventEnvelope::new(
        KNOWLEDGE_SUPERSESSION_EVENT_TYPE,
        feed.feed_id,
        0,
        DisclosureClass::Public,
        [0x77; 32],
    );
    sup_event.payload_refs = vec![ObjectReference::new(0, sup_obj_cid.into_bytes())];
    let (sup_evt_bytes, sup_evt_cid) = sup_event.sign(&feed, &signer).unwrap().encode().unwrap();
    assert_eq!(
        sink.validate_then_accept(
            ReconcileManifestKind::Event,
            sup_evt_cid.into_bytes(),
            &sup_evt_bytes
        )
        .unwrap(),
        PayloadSinkOutcome::ValidatedStored
    );
    let lineage = shared_sink.supersession_lineage(dep_obj_cid).unwrap();
    let empty_lineage = shared_sink
        .supersession_lineage(ObjectCid::from_bytes([0xFE; 32]))
        .unwrap();
    assert_ne!(lineage, empty_lineage);

    // 3. `KuCompositionRuntime` (`KU-002..008`) lazy open + receptor/affordance/equivalence roundtrip
    let comp = KuCompositionRuntime::new_lazy(dir.path());
    assert!(!comp.db_path().exists());
    let receptor = ReceptorDefinition {
        role: ConceptCcid::from_bytes([0x10; 16]),
        expected_types: vec![ConceptCcid::from_bytes([0x02; 16])],
        hard_constraints: Vec::new(),
        cardinality: ReceptorCardinality::new(1, None).unwrap(),
        origin: ReceptorOrigin::Declared {
            source: StatementLocator {
                object: ObjectReference::new(0, rep_cid.into_bytes()),
                statement_index: 0,
            },
        },
        acceptance: ReceptorAcceptanceProfile {
            policy: ObjectReference::new(0, [0xAA; 32]),
            required_evidence_kinds: Vec::new(),
            unknown_constraint_policy: UnknownConstraintPolicy::KeepUnresolved,
        },
    };
    let rec_cid = comp
        .author_receptor_definition(&receptor, DisclosureClass::Public)
        .unwrap();
    assert!(comp.db_path().exists());
    assert_eq!(
        comp.get_receptor_definition(rec_cid).unwrap(),
        Some(receptor)
    );

    let (aff_cid, _aff) = comp
        .extract_affordance_from_semantic(
            ObjectReference::new(SEMANTIC_KERNEL_OBJECT_KIND.0, rep_cid.into_bytes()),
            &replacement_sfs,
            DisclosureClass::Public,
        )
        .unwrap();
    assert!(comp.get_affordance(aff_cid).unwrap().is_some());

    let eq_kernel = comp
        .build_concept_equivalence(
            ObjectReference::new(0, defining_cid.into_bytes()),
            scoped_ccid,
            ObjectReference::new(SEMANTIC_KERNEL_OBJECT_KIND.0, rep_cid.into_bytes()),
            ConceptCcid::from_bytes([0x05; 16]),
        )
        .unwrap();
    assert_eq!(eq_kernel.correspondences.len(), 1);
}

#[tokio::test]
async fn p2_mailbox_fw05_riblt_blob_chunks_and_registry_pull_roundtrip() {
    let dir = tempfile::tempdir().unwrap();

    // 1. Outbox `PendingMailboxPickup` (`MAGIC_V4`) enforcing `FW-05`
    let outbox_path = dir.path().join("outbox.redb");
    let outbox = OutboundOutbox::open(&outbox_path).unwrap();
    let recipient_signer = TestSigner::from_seed(0x22);
    let relay_signer = TestSigner::from_seed(0x33);
    let peer = principal_node_id(&recipient_signer.pubkey());
    let relay = principal_node_id(&relay_signer.pubkey());
    let selector = SelectorCid::from_bytes([0x44; 32]);
    let namespace = NamespaceCommitment::derive(b"p2-test", [0x55; 32]).unwrap();
    let intent = OutboundTransferIntent::new(
        peer,
        "127.0.0.1:19999".parse().unwrap(),
        selector,
        namespace,
        DisclosureClass::Public,
        ReconcileManifestKind::Object,
        b"canonical-payload-v1".to_vec(),
    )
    .unwrap();
    outbox.enqueue(&intent).unwrap();

    let mut stored_receipt = MailboxStoredReceipt {
        relay_node_id: relay,
        slot_id: [0x77; 32],
        envelope_id: [0x88; 32],
        stored_bytes: 1024,
        stored_at_unix_s: 1_000,
        expires_at_unix_s: 61_000,
        deduplicated_replay: false,
        carrier_staged_only: true,
        relay_signature: [0; 64],
    };
    sign_mailbox_stored_receipt(&mut stored_receipt, &relay_signer.signing_key).unwrap();
    assert_eq!(
        outbox
            .record_mailbox_stored_receipt(&intent.id, &stored_receipt, &relay_signer.pubkey())
            .unwrap(),
        OutboundIntentState::PendingMailboxPickup
    );

    // Reopen outbox to prove `MAGIC_V4` durability and `FW-05` (`!= Acknowledged`)!
    drop(outbox);
    let outbox = OutboundOutbox::open(&outbox_path).unwrap();
    let after_relay = outbox.get(&intent.id).unwrap().unwrap();
    assert_eq!(after_relay.state, OutboundIntentState::PendingMailboxPickup);
    assert_ne!(after_relay.state, OutboundIntentState::Acknowledged);
    assert!(after_relay.mailbox_staging.is_some());

    // Only end-to-end recipient `MailboxDeliveryReceiptV1` transitions to `Acknowledged`!
    let mut delivery_receipt = MailboxDeliveryReceiptV1 {
        slot_id: [0x77; 32],
        envelope_ids: vec![[0x88; 32]],
        recipient_signing_pk: recipient_signer.pubkey(),
        delivered_at_unix_s: 2_000,
        recipient_signature: [0; 64],
    };
    sign_mailbox_delivery_receipt(&mut delivery_receipt, &recipient_signer.signing_key).unwrap();
    assert_eq!(
        outbox
            .record_mailbox_delivery_receipt(&intent.id, &delivery_receipt)
            .unwrap(),
        OutboundIntentState::Acknowledged
    );

    // 2. `NodeBlobTransferCoordinator` rejects unreferenced `BlobCid` (`obp/blob-chunks/1`)
    let ds_root = dir.path().join("ds");
    let generations = Arc::new(DatasetGenerationStore::open_exclusive(&ds_root).unwrap());
    let paths: Arc<dyn DatasetPathResolver> = generations.clone();
    let blob_auth = Arc::new(BlobAuthority::new(
        paths.clone(),
        Arc::new(OsPendingUploadIdSource),
        Arc::new(UnavailableValidatedBlobReferenceSource),
    ));
    let blob_dir = paths.owner_path(BaseStorageOwnerId::BLOB).unwrap();
    std::fs::create_dir_all(&blob_dir).unwrap();
    let blob_storage = Arc::new(
        BlobStorage::open_with_config(
            &blob_dir.join("ku.blob.redb"),
            BlobStorageConfig {
                total_quota_bytes: 64 * 1024 * 1024,
                free_space_reserve_bytes: 1024 * 1024,
            },
            blob_auth.oracle(),
        )
        .unwrap(),
    );
    let blob_coord = NodeBlobTransferCoordinator::new(
        blob_auth.canonical_oracle(),
        blob_storage,
        dir.path().join("blob-staging"),
        &blob_dir,
    );
    let unref_blob = BlobCid::from_content(ku_core::blob_store::BlobType::Raw, b"unref-bytes");
    assert!(blob_coord.request_manifest(unref_blob).is_err());

    // 3. `NodeRegistryPullCoordinator` (`D-012`) rejects unapproved governance signer before chunk 0
    let gov_signer = TestSigner::from_seed(0x61);
    let rogue_signer = TestSigner::from_seed(0x62);
    let pull_coord = NodeRegistryPullCoordinator::new(
        dir.path().join("registry-pull"),
        vec![gov_signer.pubkey()],
        None,
    )
    .unwrap();
    let rogue_desc = RegistryReleaseDescriptorV1 {
        release_sequence: 1,
        release_root: [0x11; 32],
        chunk_merkle_root: [0x22; 32],
        total_bytes: 1024,
        chunk_size_bytes: 4 * 1024 * 1024,
        total_chunks: 1,
        optional_delta_from: None,
        minimum_compatible_reader_version: 1,
        signer_public_key: rogue_signer.pubkey(),
        signature: [0x33; 64],
    };
    assert!(pull_coord.begin_or_resume_pull(rogue_desc).is_err());

    // 4. `VNextNetworkRuntime` `RibltV1` capability gate, sketch encoding & `frontier_summary_page`
    let net_dir = dir.path().join("net");
    let mut net = VNextNetworkRuntime::start(
        &net_dir,
        "127.0.0.1:0".parse().unwrap(),
        VNextNetworkPolicy::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        VNextNetworkRuntime::reconciliation_capabilities(RibltRuntimeGate {
            reconcile_riblt_v1: false,
            kill_switch_tripped: false,
        })
        .len(),
        2
    );
    assert_eq!(
        VNextNetworkRuntime::reconciliation_capabilities(RibltRuntimeGate {
            reconcile_riblt_v1: true,
            kill_switch_tripped: false,
        })
        .len(),
        3
    );
    // Empty selector rejects RIBLT sketch with `CardinalityOutOfBounds` (`P2` §2.1)!
    let empty_err = net
        .encode_riblt_range_sketch(selector, [0x01; 32], None, 16)
        .unwrap_err();
    assert!(format!("{empty_err:?}").contains("CardinalityOutOfBounds"));

    let (entries, cursor) = net
        .frontier_summary_page(selector, None, 16, &BTreeMap::new())
        .unwrap();
    assert!(entries.is_empty());
    assert!(cursor.is_none());

    let repair = net
        .repair_unexplained_hybrid_root_difference(
            selector,
            [0x99; 32],
            &[onebrain_protocol::FrontierSummaryEntryV1 {
                feed_id: [0x44; 32],
                covered_sequence: 3,
                head_event_cid: [0x55; 32],
                checkpoint_cid: Some([0x66; 32]),
                equivocation_proof_cid: None,
            }],
        )
        .unwrap();
    assert!(repair.unexplained_hybrid_root_difference);
    assert_eq!(repair.missing_feed_heads.len(), 1);
    assert_eq!(repair.missing_checkpoints, vec![[0x66; 32]]);
    net.shutdown().await;
}

#[tokio::test]
async fn p3_and_p7_kql_compiler_dual_kind_matching_m6a_and_obkg_five_plane() {
    let dir = tempfile::tempdir().unwrap();

    // 1. `onebrain_node::kql::KqlNeedCompiler` (`KQL-CMP-001`)
    let old_temp_ccid = ConceptCcid::from_bytes([0x11; 16]);
    let canon_temp_ccid = ConceptCcid::from_bytes([0x12; 16]);
    let pred_ccid = ConceptCcid::from_bytes([0x13; 16]);
    let sensor_type_ccid = ConceptCcid::from_bytes([0x14; 16]);
    let kelvin_ccid = ConceptCcid::from_bytes([0x15; 16]);

    let lifecycle =
        ConceptLifecycleTable::from_records([ConceptLifecycleRecord::Redirect(ConceptRedirect {
            deprecated_ccid: old_temp_ccid,
            canonical_ccid: canon_temp_ccid,
            since_release_seq: 1,
            provenance_ref: "release-1".into(),
        })])
        .unwrap();

    let registry = MockConceptRegistry {
        concepts: vec![
            (
                "deprecated_temperature".into(),
                old_temp_ccid,
                ConceptCategory::Property,
            ),
            (
                "temperature".into(),
                canon_temp_ccid,
                ConceptCategory::Property,
            ),
            ("measures".into(), pred_ccid, ConceptCategory::Property),
            ("sensor".into(), sensor_type_ccid, ConceptCategory::Entity),
            ("kelvin".into(), kelvin_ccid, ConceptCategory::Unit),
        ],
        lifecycle: lifecycle.clone(),
    };

    let selector = SelectorCid::from_bytes([0x71; 32]);
    let allowed_selectors = [selector];
    let unit_catalog = SiUnitCatalog::standard();
    let compile_ctx = NeedCompileContext {
        registry: &registry,
        registry_release_root: [0x81; 32],
        unit_catalog: &unit_catalog,
        allowed_public_selectors: &allowed_selectors,
        selector,
        query_policy: ObjectReference::new(0, [0x91; 32]),
        exploration_policy: ObjectReference::new(0, [0x92; 32]),
        watch_policy: ObjectReference::new(0, [0x93; 32]),
        acceptance_policy: ObjectReference::new(0, [0x94; 32]),
        observed_frontier: [0x95; 32],
        source_frontier: EventCid::from_bytes([0x96; 32]),
        generator: ObjectReference::new(0, [0x97; 32]),
        metrics: MatcherMetricConcepts {
            structural_fit: ConceptCcid::from_bytes([0xA1; 16]),
            constraint_fit: ConceptCcid::from_bytes([0xA2; 16]),
        },
        unmapped_reason: ConceptCcid::from_bytes([0xA7; 16]),
        created_at_evaluation: 1,
        expires_after_evaluations: 100,
    };

    // Reject IEEE-754 float inputs fail-closed!
    let float_req = StructuredNeedCompileRequestV1::single_statement(NeedStatementDraftV1 {
        predicate: "measures".into(),
        arguments: vec![
            NeedTermDraftV1::OpenSlot {
                name: "?s".into(),
                type_constraint: Some("sensor".into()),
            },
            NeedTermDraftV1::Quantity(NeedQuantityDraftV1::FloatForbidden(298.15)),
        ],
        constraints: Vec::new(),
        qualifiers: Default::default(),
    });
    assert!(matches!(
        KqlNeedCompiler::compile_structured(&float_req, &compile_ctx),
        Err(NeedCompileError::FloatLiteralForbidden)
    ));

    // Reject non-semantic runtime/economic fields fail-closed!
    assert!(KqlNeedCompiler::compile_kql(
        "FIND ?x WHERE ?x.obt_balance > 10 RETURN ?x",
        &compile_ctx
    )
    .is_err());

    // Compile valid structured Need using deprecated concept -> automatically redirected to canonical CCID!
    let valid_req = StructuredNeedCompileRequestV1::single_statement(NeedStatementDraftV1 {
        predicate: "measures".into(),
        arguments: vec![
            NeedTermDraftV1::OpenSlot {
                name: "?sensor_a".into(),
                type_constraint: Some("sensor".into()),
            },
            NeedTermDraftV1::ConceptToken("deprecated_temperature".into()),
        ],
        constraints: Vec::new(),
        qualifiers: Default::default(),
    });
    let compiled = KqlNeedCompiler::compile_structured(&valid_req, &compile_ctx).unwrap();
    assert_eq!(compiled.provenance.compiler_profile, COMPILER_PROFILE_ID);
    assert!(compiled.omits_hash_derived_limitation());
    assert_eq!(compiled.provenance.lifecycle_redirects_applied.len(), 1);
    assert_eq!(
        compiled.provenance.lifecycle_redirects_applied[0].canonical_ccid,
        canon_temp_ccid
    );
    assert_eq!(
        registry.resolve_for_authoring(old_temp_ccid),
        AuthoringConceptResolution::Redirected {
            from: old_temp_ccid,
            to: canon_temp_ccid,
        }
    );

    // 2. `DistributedKqlRuntime` M6A.3 (`WatchRootHintV1` `TTL <= 3600s`) & dual-kind SFS + Affordance delta scan
    let mut kql_rt = DistributedKqlRuntime::open(
        &dir.path().join("kql"),
        LocalNeedVaultKey::from_bytes([0x51; 32]),
    )
    .unwrap();
    kql_rt.register_private_need(compiled.bundle).unwrap();

    kql_rt
        .subscribe_selector_watch(selector, 1_800, 10_000)
        .unwrap();
    assert!(kql_rt
        .subscribe_selector_watch(selector, 7_200, 10_000)
        .is_err()); // > 3600s rejected!
    let scheduled = kql_rt
        .handle_watch_root_hint(
            WatchRootHintV1 {
                selector_cid: selector,
                new_root: [0x99; 32],
                ttl_secs: 600,
            },
            10_500,
        )
        .unwrap();
    assert_eq!(scheduled.unwrap().new_root, [0x99; 32]);

    let net_recv_dir = dir.path().join("kql-net-recv");
    let mut recv_net = VNextNetworkRuntime::start(
        &net_recv_dir,
        "127.0.0.1:0".parse().unwrap(),
        VNextNetworkPolicy::default(),
    )
    .await
    .unwrap();
    let empty_report = kql_rt
        .process_one_hop_semantic_and_affordance_delta(
            &recv_net,
            selector,
            DistributedKqlBudget::default(),
            Some(&lifecycle),
        )
        .unwrap();
    assert!(!empty_report.claims_automatic_materialization);
    recv_net.shutdown().await;

    // 3. `ObkgRuntimeService` 5-plane projection, `NEG-P7-001` wall-clock rejection, delete & rebuild, `PrivateVault` salience, `FedR` gate
    let mut obkg = ObkgRuntimeService::new();
    let frontier_1 =
        ObkgSourceFrontier::new(selector, [0x12; 32], [(FeedId::from_bytes([0x13; 32]), 1)])
            .unwrap();
    let mp_config = ObkgMultiPlaneConfig {
        source_frontier: frontier_1.clone(),
        checkpoint_anchor: None,
        resolution_policy_ref: ObjectReference::new(0, [0x21; 32]),
        fidelity_policy_ref: ObjectReference::new(0, [0x22; 32]),
        metabolic_policy_ref: ObjectReference::new(0, [0x23; 32]),
        registry_release_root: [0x81; 32],
        model_version: ObkgModelVersion::DeterministicNoModel,
        current_evaluation: 1,
    };
    let empty_sources = || ObkgMultiPlaneSources {
        objects: &[],
        authorized_vault_kus: &[],
        lifecycle_table: Some(&lifecycle),
        standing_needs: &[],
        resolutions: &[],
        materialized_mappings: &[],
        candidate_proposals: &[],
        metabolic_views: &[],
        benefit_closures: &[],
        fidelity_assessments: &[],
        exercises: &[],
    };
    let (_root_1, commit_1) = obkg
        .project_and_record(mp_config.clone(), empty_sources())
        .unwrap();

    // `NEG-P7-001`: wall-clock timestamp historical lookup MUST fail closed!
    assert!(obkg.at_wall_clock_timestamp(1_700_000_000).is_err());

    // Delete disposable caches & rebuild deterministically -> identical `snapshot_commitment_cid`!
    obkg.clear_disposable_caches();
    assert!(obkg.at_frontier(&frontier_1).is_err());
    let (_root_2, commit_2) = obkg.project_and_record(mp_config, empty_sources()).unwrap();
    assert_eq!(commit_1, commit_2);

    // `PrivateVault`-sealed `LocalSalienceTieBreaker` (`purpose = 4`)
    let vault_backend = RedbVerifiedBackend::open(&dir.path().join("salience_vault.redb")).unwrap();
    let vault = PrivateVault::new(vault_backend, VaultKey::from_bytes([0x44; 32]));
    let actor = ActorId::from_bytes([0x45; 32]);
    obkg.set_local_salience([0xAA; 32], 750);
    let (_binding, sealed_salience) = obkg.seal_salience_cache(actor, &vault).unwrap();
    let mut obkg_restored = ObkgRuntimeService::new();
    obkg_restored
        .open_salience_cache(actor, &vault, &sealed_salience)
        .unwrap();
    assert_eq!(
        obkg_restored.salience_cache().salience_milli(&[0xAA; 32]),
        750
    );

    // `FedRQuarantineValidator` default-off feature gate & quarantine isolation (`OBS-002`)
    let mut ext = VNextExtendedFeatureConfig::default();
    let space_spec_cid = ObjectCid::from_bytes([0x88; 32]);
    let fedr_matrix = FedROperatorMatrixV1::build(
        actor,
        pred_ccid,
        [0x81; 32],
        space_spec_cid,
        2,
        vec![1_000_000, 0, 0, 1_000_000],
    )
    .unwrap();
    assert!(obkg
        .ingest_fedr_operator(&ext, [0x81; 32], space_spec_cid, 5_000_000, &fedr_matrix)
        .is_err());
    ext.enabled.obkg_fedr_v1 = true;
    let fedr_ok = obkg
        .ingest_fedr_operator(&ext, [0x81; 32], space_spec_cid, 5_000_000, &fedr_matrix)
        .unwrap();
    assert!(!fedr_ok.quarantine_record.is_executable());
    assert!(!fedr_ok.emits_obkg_rewards());
}

#[test]
fn p4_p6_and_p5_causal_closure_remote_cognition_delegated_jobs_pam_and_isolated_obt() {
    let dir = tempfile::tempdir().unwrap();

    // 1. `P4` Generalized 2-step `Prepare -> Confirm` + out-of-order `CausalEvidenceClosureEngine`
    let (_ku_author_signer, _, _, ku_author_feed) = make_feed(0x21, 0x22, 0x23);
    let (exerciser_signer, _, _, exerciser_feed) = make_feed(0x31, 0x32, 0x33);

    let publisher = PublicUseEvidencePublisher::open(&dir.path().join("pomv-pub")).unwrap();
    let ku_target = ObjectReference::new(SEMANTIC_KERNEL_OBJECT_KIND.0, [0x41; 32]);
    let policy_ref = ObjectReference::new(0, [0x42; 32]);
    let shared_ctx = [0x43; 32];
    let peer = NodeId::from_bytes([0x45; 32]);
    let selector = SelectorCid::from_bytes([0x46; 32]);
    let namespace = NamespaceCommitment::derive(b"p4-m6b", [0x47; 32]).unwrap();
    let now_s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Publish UseEvidence (seq 0)
    let use_payload = UseEvidencePayload {
        subjects: vec![ku_target.clone()],
        mode: UseMode::Application,
        actor_class: ConceptCcid::from_bytes([0x21; 16]),
        task_context_commitment: shared_ctx,
        causal_role: ConceptCcid::from_bytes([0x22; 16]),
        assembly: None,
        mapping: None,
        outcome_observation: None,
        use_policy: policy_ref.clone(),
        observed_frontier: [0x52; 32],
    };
    let use_prep = publisher
        .prepare_causal_evidence(
            &PrepareCausalEvidenceRequest {
                payload: CausalEvidencePublishPayload::Use(use_payload),
                expected_peer: peer,
                selector,
                namespace,
                idempotency_key: [0x51; 32],
                expires_at: now_s + 600,
            },
            &exerciser_feed,
        )
        .unwrap();
    let (use_pub, use_out) = publisher
        .publish_confirmed_causal_evidence(&use_prep.confirm(), &exerciser_feed, &exerciser_signer)
        .unwrap();
    assert_eq!(use_out, PublicUsePublishOutcome::Stored);

    // Publish OutcomeObservation (seq 1)
    let outcome_payload = OutcomeObservationPayload {
        task_context_commitment: shared_ctx,
        outcome_class: ConceptCcid::from_bytes([0x31; 16]),
        valence: OutcomeValence::Beneficial,
        affected: AffectedScope {
            principal: Some(AffectedPrincipal::Actor(ActorId::from_bytes([0x32; 32]))),
            assembly: None,
        },
        measurement_evidence: vec![ObjectReference::new(0, [0x56; 32])],
        baseline: Some(ObjectReference::new(0, [0x57; 32])),
        limitations: Vec::new(),
        observation_policy: policy_ref.clone(),
        observed_frontier: [0x52; 32],
    };
    let out_prep = publisher
        .prepare_causal_evidence(
            &PrepareCausalEvidenceRequest {
                payload: CausalEvidencePublishPayload::OutcomeObservation(outcome_payload),
                expected_peer: peer,
                selector,
                namespace,
                idempotency_key: [0x53; 32],
                expires_at: now_s + 600,
            },
            &exerciser_feed,
        )
        .unwrap();
    let (out_pub, _) = publisher
        .publish_confirmed_causal_evidence(&out_prep.confirm(), &exerciser_feed, &exerciser_signer)
        .unwrap();

    // Broken `task_context_commitment` continuity MUST be rejected at Prepare!
    let bad_benefit = ku_core::foundation::BenefitEvidencePayload {
        task_context_commitment: [0x99; 32],
        outcome_observations: vec![ObjectReference::new(
            OUTCOME_OBSERVATION_KIND.0,
            out_pub.object_cid.into_bytes(),
        )],
        use_events: vec![use_pub.event_cid],
        knowledge_subjects: vec![ku_target.clone()],
        assessed_valence: OutcomeValence::Beneficial,
        attribution: AttributionStatus::Supported,
        causal_evidence: vec![ObjectReference::new(0, [0x58; 32])],
        counterfactual_evidence: vec![ObjectReference::new(0, [0x59; 32])],
        limitations: Vec::new(),
        benefit_policy: policy_ref.clone(),
        assessed_frontier: [0x52; 32],
    };
    assert!(publisher
        .prepare_causal_evidence(
            &PrepareCausalEvidenceRequest {
                payload: CausalEvidencePublishPayload::BenefitEvidence {
                    payload: bad_benefit,
                    expected_task_context_commitment: shared_ctx,
                },
                expected_peer: peer,
                selector,
                namespace,
                idempotency_key: [0x54; 32],
                expires_at: now_s + 600,
            },
            &exerciser_feed,
        )
        .is_err());

    // Valid BenefitEvidence (seq 2)
    let benefit_payload = ku_core::foundation::BenefitEvidencePayload {
        task_context_commitment: shared_ctx,
        outcome_observations: vec![ObjectReference::new(
            OUTCOME_OBSERVATION_KIND.0,
            out_pub.object_cid.into_bytes(),
        )],
        use_events: vec![use_pub.event_cid],
        knowledge_subjects: vec![ku_target.clone()],
        assessed_valence: OutcomeValence::Beneficial,
        attribution: AttributionStatus::Supported,
        causal_evidence: vec![ObjectReference::new(0, [0x58; 32])],
        counterfactual_evidence: vec![ObjectReference::new(0, [0x59; 32])],
        limitations: Vec::new(),
        benefit_policy: policy_ref,
        assessed_frontier: [0x52; 32],
    };
    let ben_prep = publisher
        .prepare_causal_evidence(
            &PrepareCausalEvidenceRequest {
                payload: CausalEvidencePublishPayload::BenefitEvidence {
                    payload: benefit_payload,
                    expected_task_context_commitment: shared_ctx,
                },
                expected_peer: peer,
                selector,
                namespace,
                idempotency_key: [0x55; 32],
                expires_at: now_s + 600,
            },
            &exerciser_feed,
        )
        .unwrap();
    let (ben_pub, _) = publisher
        .publish_confirmed_causal_evidence(&ben_prep.confirm(), &exerciser_feed, &exerciser_signer)
        .unwrap();

    // Out-of-order ingestion: Benefit FIRST -> Outcome SECOND -> Use THIRD!
    let mut closure =
        CausalEvidenceClosureEngine::open_durable(&dir.path().join("closure_staging.redb"))
            .unwrap();
    closure.register_feed(ku_author_feed.clone());
    closure.register_feed(exerciser_feed.clone());
    closure.register_ku_author(ku_target, ku_author_feed.feed_id);

    let (d1, w1) = closure
        .ingest_evidence_artifact(
            BENEFIT_EVIDENCE_KIND.0,
            BENEFIT_EVIDENCE_EVENT_TYPE.0,
            &ben_pub.object_bytes,
            &ben_pub.event_bytes,
        )
        .unwrap();
    assert_eq!(d1, CausalIngestDisposition::DeferredMissingDependency);
    assert_eq!(w1, 0);

    let (d2, w2) = closure
        .ingest_evidence_artifact(
            OUTCOME_OBSERVATION_KIND.0,
            OUTCOME_OBSERVATION_EVENT_TYPE.0,
            &out_pub.object_bytes,
            &out_pub.event_bytes,
        )
        .unwrap();
    assert_eq!(d2, CausalIngestDisposition::DeferredMissingDependency);
    assert_eq!(w2, 0);

    // Arrival of `UseEvidence` wakes `OutcomeObservation` AND `BenefitEvidence` in topological order!
    let (d3, w3) = closure
        .ingest_evidence_artifact(
            USE_EVIDENCE_KIND.0,
            USE_EVIDENCE_EVENT_TYPE.0,
            &use_pub.object_bytes,
            &use_pub.event_bytes,
        )
        .unwrap();
    assert_eq!(d3, CausalIngestDisposition::AppliedReady);
    assert_eq!(w3, 2);
    assert_eq!(closure.staged_count(), 0);
    assert!(!closure.changes_obt_or_wallet_state());

    // 2. `P6` `DelegatedJobCoordinator`: bounded lease (`<= 300_000 ms`) & two-phase BlindVerify commit-before-reveal
    let mut jobs = DelegatedJobCoordinator::new();
    let req_signer = TestSigner::from_seed(0x61);
    let ver_signer = TestSigner::from_seed(0x62);
    let job_id = [0x63; 32];
    jobs.register_encode_job(
        job_id,
        ActorId::from_bytes([0x64; 32]),
        req_signer.pubkey(),
        PermitCid::from_bytes([0x65; 32]),
        ObjectCid::from_bytes([0x66; 32]),
        ObjectCid::from_bytes([0x67; 32]),
    )
    .unwrap();
    // > 300_000 ms lease rejected
    let over_lease = DelegatedJobLeaseWireV1 {
        job_id,
        worker_node_id: peer,
        permit_cid: PermitCid::from_bytes([0x65; 32]),
        lease_ttl_ms: 300_001,
        issued_at_unix_ms: 1_000,
        requester_signature: [0x11; 64],
    };
    assert!(matches!(
        jobs.claim_encode_lease(over_lease, 1_000),
        Err(DelegatedJobError::InvalidLeaseTtl(300_001))
    ));
    let valid_lease_preimage = delegated_lease_signing_preimage(
        job_id,
        peer,
        PermitCid::from_bytes([0x65; 32]),
        60_000,
        1_000,
    );
    let valid_lease = DelegatedJobLeaseWireV1 {
        job_id,
        worker_node_id: peer,
        permit_cid: PermitCid::from_bytes([0x65; 32]),
        lease_ttl_ms: 60_000,
        issued_at_unix_ms: 1_000,
        requester_signature: req_signer.sign_raw(&valid_lease_preimage),
    };
    jobs.claim_encode_lease(valid_lease, 1_000).unwrap();

    // Two-phase BlindVerify (`FID-002`): reveal BEFORE commit MUST fail!
    let blind_job_id = [0x68; 32];
    let span_cid = ObjectCid::from_bytes([0x69; 32]);
    let ver_feed = FeedId::from_bytes([0x6A; 32]);
    jobs.register_blind_verify_job(blind_job_id, span_cid, ver_feed, ver_signer.pubkey())
        .unwrap();

    let attempt_bytes = b"blind-encoding-attempt-v1";
    let fidelity_bytes = b"blind-fidelity-attestation-v1";
    let reveal_salt = [0x6B; 32];
    let output_digest = compute_blind_reveal_output_digest(attempt_bytes, fidelity_bytes);
    let salted_commit = BlindCommitRevealProof::compute_commit_digest(output_digest, reveal_salt);

    let premature_reveal = BlindVerifyRevealWireV1 {
        job_id: blind_job_id,
        commitment_ack_digest: [0x77; 32],
        reveal_salt,
        encoding_attempt_bytes: attempt_bytes.to_vec(),
        fidelity_attestation_bytes: fidelity_bytes.to_vec(),
        verifier_signature: [0x88; 64],
    };
    assert_eq!(
        jobs.submit_blind_verify_reveal(
            premature_reveal,
            ObjectCid::from_bytes([0x71; 32]),
            EventCid::from_bytes([0x72; 32]),
            [0x73; 32],
            ObjectCid::from_bytes([0x74; 32]),
        ),
        Err(DelegatedJobError::CommitMustPrecedeReveal)
    );

    let corr_bytes = b"4d-correlation-decl";
    let commit_preimage =
        blind_commit_signing_preimage(blind_job_id, ver_feed, span_cid, salted_commit, corr_bytes);
    let ack_digest = jobs
        .submit_blind_verify_commit(BlindVerifyCommitWireV1 {
            job_id: blind_job_id,
            verifier_feed_id: ver_feed,
            source_span_cid: span_cid,
            attempt_semantic_cid_salted_hash: salted_commit,
            correlation_declaration_bytes: corr_bytes.to_vec(),
            verifier_signature: ver_signer.sign_raw(&commit_preimage),
        })
        .unwrap();

    let reveal_preimage = blind_reveal_signing_preimage(
        blind_job_id,
        ack_digest,
        reveal_salt,
        attempt_bytes,
        fidelity_bytes,
    );
    let (_proof, work_notice) = jobs
        .submit_blind_verify_reveal(
            BlindVerifyRevealWireV1 {
                job_id: blind_job_id,
                commitment_ack_digest: ack_digest,
                reveal_salt,
                encoding_attempt_bytes: attempt_bytes.to_vec(),
                fidelity_attestation_bytes: fidelity_bytes.to_vec(),
                verifier_signature: ver_signer.sign_raw(&reveal_preimage),
            },
            ObjectCid::from_bytes([0x71; 32]),
            EventCid::from_bytes([0x72; 32]),
            [0x73; 32],
            ObjectCid::from_bytes([0x74; 32]),
        )
        .unwrap();
    assert!(!work_notice.contains_token_authority());

    // 3. `P6` `RemoteCognitionRuntime` default-off gate & `PamHostBridge` (`FW-06` + `AI-004`)
    let cog = RemoteCognitionRuntime::new_lazy(dir.path(), default_cognitive_execution_policy());
    assert!(!cog.db_path().exists());
    assert!(!cog.grants_validated_store_authority());

    let pam_vault_backend = RedbVerifiedBackend::open(&dir.path().join("pam_vault.redb")).unwrap();
    let pam_vault = PrivateVault::new(pam_vault_backend, VaultKey::from_bytes([0x81; 32]));
    let mut pam = PamHostBridge::new_in_memory(ActorId::from_bytes([0x82; 32]), pam_vault).unwrap();
    pam.record_rag_citation(
        ObjectReference::new(SEMANTIC_KERNEL_OBJECT_KIND.0, [0x83; 32]),
        [0x84; 32],
    )
    .unwrap();
    assert_eq!(pam.emitted_use_evidence_payload_count(), 0); // FW-06!
    let card = pam
        .stage_unassessed_recommendation(
            [0x85; 32],
            PamRecommendationActionKind::SavePrivateKuDraft,
            "Consider capturing sensor calibration note",
            vec![ObjectReference::new(
                SEMANTIC_KERNEL_OBJECT_KIND.0,
                [0x83; 32],
            )],
            DisclosureClass::LocalOnly,
        )
        .unwrap();
    assert!(!card.model_qualified);
    assert_eq!(card.epistemic_label, UNASSESSED_EPISTEMIC_LABEL);

    // 4. `P5` `ObtRuntimeCoordinator`: `Disabled` -> `ShadowEvaluate` (zero mints) -> `ActiveDomainFinal` -> corrupt `obt.redb` non-fatal isolation (`NEG-OBT-001`)!
    let mut obt = ObtRuntimeCoordinator::new_lazy(dir.path());
    assert!(!obt.db_path().exists());
    assert!(!obt.can_gate_knowledge_plane());
    assert_eq!(
        obt.economic_status(),
        ObtEconomicStatus::SimulatedNonEconomic
    );
    assert_eq!(obt.economic_status_label(), "simulated_non_economic");

    let mut obt_ext = VNextExtendedFeatureConfig::default();
    obt_ext.enabled.obt_ledger_v1 = true;
    obt.sync_feature_flags(true, false, &obt_ext);

    let policy = RewardPolicyV1::default_v1();
    let policy_cid = obt.register_reward_policy(policy).unwrap();

    let benefit_notice = RewardEvidenceNoticeV2::BenefitClosure(BenefitClosureNotice {
        benefit_event_cid: ben_pub.event_cid,
        outcome_case_id: [0x44; 32],
        evidence_frontier_root: [0x92; 32],
        policy_cid,
    });
    let work_notice_bound = RewardEvidenceNoticeV2::AcceptedWork(AcceptedWorkNotice {
        work_record_cid: ObjectCid::from_bytes([0x71; 32]),
        attestation_event_cid: EventCid::from_bytes([0x72; 32]),
        work_kind: AcceptedWorkKind::BlindFidelityVerify,
        evidence_frontier_root: [0x92; 32],
        policy_cid,
    });

    // `ShadowEvaluate`: evaluates diagnostics only, creates ZERO mints or account blocks!
    obt.configure_mode(
        ObtRuntimeMode::ShadowEvaluate,
        ObtEconomicStatus::SimulatedNonEconomic,
    )
    .unwrap();
    let mut outbox_v2 = DurableRewardNoticeOutboxV2::new_lazy(dir.path(), true, 128);
    assert_eq!(
        outbox_v2.observe_committed_v2(benefit_notice),
        RewardObserveOutcome::QueuedAfterKnowledgeCommit
    );
    assert_eq!(outbox_v2.drain_v2(&mut obt, 16).exported, 1);
    let shadow_drain = obt.drain_post_commit_notices(|notice, econ| {
        Ok(ObtRuntimeCoordinator::build_shadow_candidate(
            notice,
            ScopedNullifier::from_bytes([0xC1; 32]),
            true,
            ObtAmount(1_000),
            "SHADOW_OK",
            econ,
            Vec::new(),
        ))
    });
    assert_eq!(shadow_drain.shadow_evaluations_persisted, 1);
    assert_eq!(shadow_drain.pending_mints_created, 0);
    assert_eq!(shadow_drain.account_blocks_created, 0);
    assert!(obt.pending_mints().is_empty());

    // `ActiveDomainFinal`: opens isolated `obt-ledger-v1/obt.redb`
    obt.configure_mode(
        ObtRuntimeMode::ActiveDomainFinal,
        ObtEconomicStatus::SimulatedNonEconomic,
    )
    .unwrap();
    assert!(obt.db_path().exists());
    assert!(!matches!(
        obt.ingest_post_commit_notice(work_notice_bound),
        NoticeIngestOutcome::DroppedFirewallSafe { .. }
    ));

    // Corrupt `obt-ledger-v1/obt.redb` on disk and prove `NEG-OBT-001` non-fatal isolation!
    let obt_db_path = obt.db_path();
    drop(obt);
    std::fs::write(&obt_db_path, b"CORRUPT_OBT_REDB_HEADER_GARBAGE").unwrap();

    let mut degraded_obt = ObtRuntimeCoordinator::new_lazy(dir.path());
    degraded_obt.sync_feature_flags(true, false, &obt_ext);
    let _ = degraded_obt.configure_mode(
        ObtRuntimeMode::ActiveDomainFinal,
        ObtEconomicStatus::SimulatedNonEconomic,
    );
    assert!(matches!(
        degraded_obt.store_health(),
        ObtStoreHealth::IsolatedDegraded(_)
    ));
    // Post-commit firewall exports NEVER fail knowledge commits even when `obt.redb` is corrupt!
    assert!(matches!(
        degraded_obt.ingest_post_commit_notice(benefit_notice),
        NoticeIngestOutcome::DroppedFirewallSafe { .. }
    ));
}

#[test]
fn p8_and_p10_rollout_merge_cold_tombstones_startup_recovery_personal_mesh_and_surface_ceilings() {
    let dir = tempfile::tempdir().unwrap();

    // 1. `P8` `0x000E` conservative union of kill switches & max version fence + `0x000F` salt rotation + `0x000C` anti-equivocation sequence floor (`NEG-P8-001`)
    let mut local_ext = VNextExtendedFeatureConfig::default();
    local_ext.kill_switches.obt_ledger_v1 = true;
    local_ext.version_fence = 5;
    let mut archive_ext = VNextExtendedFeatureConfig::default();
    archive_ext.enabled.obt_ledger_v1 = true;
    archive_ext.enabled.personal_mesh_v1 = true;
    archive_ext.kill_switches.remote_cognition_executor = true;
    archive_ext.version_fence = 3;

    let merged_ext = local_ext.merge_conservative_restore(&archive_ext);
    assert_eq!(merged_ext.version_fence, 5);
    assert!(merged_ext.kill_switches.obt_ledger_v1);
    assert!(merged_ext.kill_switches.remote_cognition_executor);
    assert!(!merged_ext.is_active(VNextExtendedFeature::ObtLedgerV1));
    assert!(merged_ext.is_active(VNextExtendedFeature::PersonalMeshV1));

    let net_floor = OptionalNetworkFloorSnapshot {
        provider_retire_floors: Vec::new(),
        route_packet_replay_tags: Vec::new(),
        cognitive_task_replay_entries: Vec::new(),
        reconciliation_restore_salt: [0x11; 32],
    };
    let rotated = net_floor.rotated_for_restore(&[0x22; 32]);
    assert_ne!(
        rotated.reconciliation_restore_salt,
        net_floor.reconciliation_restore_salt
    );

    let pub_floor_dir = dir.path().join("seq-floor");
    let pub_floor = PublicUseEvidencePublisher::open(&pub_floor_dir).unwrap();
    let feed_id = FeedId::from_bytes([0x31; 32]);
    pub_floor
        .enforce_sequence_floor(feed_id, 10, Some(EventCid::from_bytes([0xAA; 32])))
        .unwrap();
    pub_floor
        .enforce_sequence_floor(feed_id, 4, Some(EventCid::from_bytes([0xBB; 32])))
        .unwrap();
    // `max(10, 4) == 10` (`NEG-P8-001` — sequence floor never regresses!)
    assert_eq!(pub_floor.next_sequence_floor(feed_id).unwrap(), 10);

    // 2. `P8` `OperationalCompactionStore` `> 65,536` (`66,000`) anti-replay tombstones rolling over at `49,152` into cold Merkle-verified epochs!
    let comp_dir = dir.path().join("op-compaction");
    let comp_store =
        OperationalCompactionStore::open(&comp_dir, OperationalCompactionPolicy::default())
            .unwrap();
    let mut tombstones = Vec::with_capacity(66_000);
    for i in 0..66_000u64 {
        let mut t = [0u8; 32];
        t[..8].copy_from_slice(&i.to_be_bytes());
        t[31] = 0xEE;
        tombstones.push((t, 1_000 + i));
    }
    let batch_report = comp_store
        .record_anti_replay_tombstones_batch(&tombstones)
        .unwrap();
    assert_eq!(batch_report.inserted, 66_000);
    assert!(!batch_report.rolled_over_epochs.is_empty());
    assert!(comp_store
        .tombstone_overflow_summary_root()
        .unwrap()
        .is_some());

    // Every single tombstone (cold epoch 0 AND hot table) remains losslessly verifiable!
    let first_tombstone = tombstones[0].0;
    let last_tombstone = tombstones[65_999].0;
    assert!(comp_store
        .contains_anti_replay_tombstone(&first_tombstone)
        .unwrap());
    assert!(comp_store
        .contains_anti_replay_tombstone(&last_tombstone)
        .unwrap());
    let first_epoch_seq = batch_report.rolled_over_epochs[0];
    let (acc, proof) = comp_store
        .prove_cold_tombstone(first_epoch_seq, &first_tombstone)
        .unwrap();
    assert_eq!(acc.epoch_seq, first_epoch_seq);
    assert_eq!(proof.tombstone, first_tombstone);

    // 3. `P8` 4-Phase `CrossStoreRecoveryCoordinator` (`R1..R4`) + non-fatal corrupt `obt.redb` + `PinSetEvaluator`
    let ds_root = dir.path().join("recovery-ds");
    let generations = DatasetGenerationStore::open_exclusive(&ds_root).unwrap();
    let obt_corrupt_dir = dir.path().join("corrupt-obt-host");
    std::fs::create_dir_all(obt_corrupt_dir.join("obt-ledger-v1")).unwrap();
    std::fs::write(
        obt_corrupt_dir.join("obt-ledger-v1").join("obt.redb"),
        b"CORRUPT_OBT_DATA",
    )
    .unwrap();

    let recovery_report = CrossStoreRecoveryCoordinator::run_startup_reconciliation(
        &generations,
        None,
        None,
        Some(&obt_corrupt_dir),
    )
    .unwrap();
    assert_eq!(
        recovery_report.completed_phases,
        [
            RecoveryPhase::R1AuthorityAndFloors,
            RecoveryPhase::R2CanonicalAndBlobParity,
            RecoveryPhase::R3JournalsAndPreparedExpiry,
            RecoveryPhase::R4DerivedViewsAndIsolatedObt,
        ]
    );
    assert!(recovery_report.r4_isolated_obt_degraded.is_some());

    let mut gc_cfg = VNextFeatureConfig::default();
    gc_cfg.enabled.checkpoint_gc = true;
    let mut pins = PinSetEvaluator::new();
    let pinned_cid = ObjectCid::from_bytes([0x77; 32]);
    pins.pin_local_collection_objects([pinned_cid]);
    assert_eq!(
        pins.evaluate_object_eviction(&gc_cfg, &pinned_cid).unwrap(),
        LocalEvictionDecision::Pinned(vec![PinRetentionReason::LocalCollection])
    );
    assert_eq!(
        pins.evaluate_object_eviction(&gc_cfg, &ObjectCid::from_bytes([0x78; 32]))
            .unwrap(),
        LocalEvictionDecision::EligibleForEviction
    );

    // 4. `P10` `PersonalMeshRuntime` 2-round SPAKE2+, epoch initialization, device revocation (`mesh_epoch + 1`), and record-class firewall (`FW-04` / `FW-10`)
    let mut mesh_a = PersonalMeshRuntime::new_lazy(dir.path().join("mesh-a"));
    let mut mesh_b = PersonalMeshRuntime::new_lazy(dir.path().join("mesh-b"));
    assert!(!mesh_a.db_path().exists());

    let actor = ActorId::from_bytes([0xA1; 32]);
    let dev_a = DeviceId::from_bytes([0xA2; 32]);
    let dev_b = DeviceId::from_bytes([0xB2; 32]);
    let principals_a = PersonalMeshPrincipals::new(
        actor,
        dev_a,
        FeedId::from_bytes([0xA3; 32]),
        NodeId::from_bytes([0xA4; 32]),
    )
    .unwrap();
    let principals_b = PersonalMeshPrincipals::new(
        actor,
        dev_b,
        FeedId::from_bytes([0xB3; 32]),
        NodeId::from_bytes([0xB4; 32]),
    )
    .unwrap();

    // 2-round SPAKE2+ handshake
    let session_id = [0xC1; 32];
    let pass = b"correct-horse-battery-staple";
    let r1_a = mesh_a
        .pair_start(session_id, pass, &principals_a, &principals_b, [0x11; 32])
        .unwrap();
    let (r1_b, chall_b) = mesh_b
        .pair_respond(
            session_id,
            pass,
            &principals_a,
            &principals_b,
            [0x22; 32],
            &r1_a,
        )
        .unwrap();
    let chall_a = mesh_a
        .pair_initiator_process_round1(session_id, &r1_b)
        .unwrap();
    let est_a = mesh_a
        .pair_confirm(session_id, &chall_b.outbound_confirmation)
        .unwrap();
    let est_b = mesh_b
        .pair_confirm(session_id, &chall_a.outbound_confirmation)
        .unwrap();
    assert_eq!(est_a.transcript_digest, est_b.transcript_digest);

    // Initialize epoch 1 across dev_a & dev_b, sync encrypted record, then revoke dev_b -> epoch 2!
    let dev_a_signer = TestSigner::from_seed(0xD1);
    let dev_b_signer = TestSigner::from_seed(0xD2);
    let (author_signer, _, _, author_feed) = make_feed(0xD3, 0xA1, 0xA2);
    let pub_a = x25519_public_from_secret(&[0x41; 32]);
    let pub_b = x25519_public_from_secret(&[0x42; 32]);

    let binding_a = PersonalMeshMemberBindingV1::sign(
        principals_a,
        EventCid::from_bytes([0xE1; 32]),
        pub_a,
        1,
        &dev_a_signer.signing_key,
    )
    .unwrap();
    let binding_b = PersonalMeshMemberBindingV1::sign(
        principals_b,
        EventCid::from_bytes([0xE2; 32]),
        pub_b,
        1,
        &dev_b_signer.signing_key,
    )
    .unwrap();

    let initial_dek = PersonalMeshEpochDek::new(1, [0x55; 32]).unwrap();
    mesh_a
        .initialize_mesh(
            actor,
            1,
            vec![binding_a, binding_b],
            initial_dek,
            [0x56; 32],
        )
        .unwrap();
    assert_eq!(mesh_a.mesh_epoch().unwrap(), Some(1));
    assert!(mesh_a.db_path().exists());

    // Encrypt allowed `PrivateKuDraft` record; verify `SomaticBiometricRaw` (`FW-04`) and `Obm1ManagementToken` (`FW-10`) fail closed!
    let encrypted = mesh_a
        .export_encrypted_records(&[(
            PersonalMeshRecordClass::PrivateKuDraft,
            [0x61; 32],
            [0x62; 24],
            b"private-draft-v1".to_vec(),
        )])
        .unwrap();
    assert_eq!(encrypted.len(), 1);
    assert!(mesh_a
        .export_encrypted_records(&[(
            PersonalMeshRecordClass::SomaticBiometricRaw,
            [0x63; 32],
            [0x64; 24],
            b"forbidden-biometrics".to_vec(),
        )])
        .is_err());
    assert!(mesh_a
        .export_encrypted_records(&[(
            PersonalMeshRecordClass::Obm1ManagementToken,
            [0x65; 32],
            [0x66; 24],
            b"obm1.forbidden".to_vec(),
        )])
        .is_err());

    // Revoke `dev_b` -> forward-secret rotation `mesh_epoch: 1 -> 2`!
    let revoc_bytes = ActorRevocation::new(
        actor,
        EventCid::from_bytes([0xE2; 32]),
        dev_b,
        0,
        EventCid::from_bytes([0xE1; 32]),
        author_feed.feed_id,
    )
    .unwrap()
    .sign(&author_feed, &author_signer.signing_key)
    .unwrap()
    .encode()
    .unwrap();
    let validated_revoc =
        ku_core::foundation::decode_actor_revocation(&revoc_bytes, &author_feed).unwrap();
    let new_epoch = mesh_a
        .revoke_device(&validated_revoc, [0x71; 32], [0x72; 32])
        .unwrap();
    assert_eq!(new_epoch, 2);
    assert!(mesh_a.is_device_revoked(&dev_b).unwrap());
    assert!(mesh_a.wrapped_dek_for(&dev_b).unwrap().is_none());

    // 5. `P10` `SurfaceCapabilityRegistry` (`obc_surface1.<claims>.<mac>`) across all 6 `SurfaceKind`s & `FW-10` `obm1.*` isolation
    let mut surf = SurfaceCapabilityRegistry::new([0x99; 32]).unwrap();
    assert_eq!(SurfaceKind::ALL.len(), 6);

    // Extension cannot mint `ConfirmExplicitUserGesture` (`P10` §4 ceiling)
    let ext_claims = SurfaceCapabilityClaimsV1 {
        token_id: [0x01; 32],
        surface_kind: SurfaceKind::Extension,
        actor_id: actor,
        device_id: dev_a,
        allowed_scopes: BTreeSet::from([SurfaceScope::ConfirmExplicitUserGesture]),
        issued_at_unix_s: 1_000,
        expires_at_unix_s: 2_000,
    };
    assert_eq!(
        surf.mint_token(ext_claims),
        Err(SurfaceCapabilityError::CeilingExceeded {
            surface: SurfaceKind::Extension,
            forbidden_scope: SurfaceScope::ConfirmExplicitUserGesture,
        })
    );

    // `obm1.*` management credential rejected on `obc_surface1` channel (`FW-10`)
    assert_eq!(
        surf.verify_token("obm1.secret-admin-token", 1_500),
        Err(SurfaceCapabilityError::ManagementTokenOnSurfaceChannel)
    );

    // Mint valid `Glasses` token (`SomaticEmergencyAbort` + `CaptureSourceAndDraft`) and check route ceilings!
    let glasses_claims = SurfaceCapabilityClaimsV1 {
        token_id: [0x02; 32],
        surface_kind: SurfaceKind::Glasses,
        actor_id: actor,
        device_id: dev_a,
        allowed_scopes: BTreeSet::from([
            SurfaceScope::ReadStatusAndQuery,
            SurfaceScope::CaptureSourceAndDraft,
            SurfaceScope::SomaticEmergencyAbort,
        ]),
        issued_at_unix_s: 1_000,
        expires_at_unix_s: 2_000,
    };
    let glasses_token = surf.mint_token(glasses_claims).unwrap();
    assert!(glasses_token.starts_with("obc_surface1."));
    assert!(surf
        .authorize_token_for_route(
            &glasses_token,
            "POST",
            "/api/pam/somatic-abort",
            false,
            1_500
        )
        .is_ok());
    assert!(surf
        .authorize_token_for_route(&glasses_token, "POST", "/api/share/confirm", false, 1_500)
        .is_err());

    // Desktop token requires `has_valid_obm1_header == true` for `/api/admin/*` (`FW-10`)!
    let desktop_claims = SurfaceCapabilityClaimsV1 {
        token_id: [0x03; 32],
        surface_kind: SurfaceKind::Desktop,
        actor_id: actor,
        device_id: dev_a,
        allowed_scopes: SurfaceKind::Desktop.max_allowed_scopes(),
        issued_at_unix_s: 1_000,
        expires_at_unix_s: 2_000,
    };
    let desktop_token = surf.mint_token(desktop_claims).unwrap();
    assert!(matches!(
        surf.authorize_token_for_route(&desktop_token, "POST", "/api/admin/rollout", false, 1_500),
        Err(SurfaceCapabilityError::MissingObm1ManagementHeader { .. })
    ));
    assert!(surf
        .authorize_token_for_route(&desktop_token, "POST", "/api/admin/rollout", true, 1_500)
        .is_ok());
}
