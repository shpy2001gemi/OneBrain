#![forbid(unsafe_code)]
//! Synchronous, `tokio`-free, socket-free canonical KU & composition engine shared by
//! `onebrain-node` and `onebrain-mobile-core` (`ARCH-10` / `AD-01`).

pub mod composition;
pub mod error;
pub mod fidelity_portfolio;
pub mod ku_engine;

pub use composition::{RedbCompositionStore, RedbMappingBackend};
pub use error::CoreEngineError;
pub use fidelity_portfolio::{
    FidelityPortfolioStatusView, FidelityPortfolioTargetKey, RedbFidelityPortfolio,
};
pub use ku_engine::{
    PreparedKuArtifact, PrivateKuDetail, RedirectRewriteNotice, RevisionLineageEntry,
    SavedPrivateKuRecord, SyncKuWorkspace, KU_PREPARED_DRAFTS_V1, KU_PRIVATE_JOURNAL_V1,
};

const B64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Pure RFC 4648 standard Base64 encoder for canonical preview payloads.
pub fn base64_encode_standard(input: &[u8]) -> String {
    if input.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);

        out.push(B64_ALPHABET[(b0 >> 2) as usize] as char);
        out.push(B64_ALPHABET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64_ALPHABET[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64_ALPHABET[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;

    use super::*;
    use ku_core::concept_lifecycle::{
        ConceptLifecycleRecord, ConceptLifecycleTable, ConceptRedirect, ConceptRetire,
        ConceptRetireReason, ConceptSplit,
    };
    use ku_core::concept_registry::{ConceptCategory, ConceptRegistry, ResolvedConcept};
    use ku_core::foundation::{
        actor_id_from_root_key, decode_feed_inception, ActorRootDelegation, AssemblyLineageId,
        BindingAcceptance, ConceptCcid, CorrelationDimension, CorrelationDimensionEvidence,
        CorrelationEvidence, CorrespondenceKind, DeviceId, DimensionVector, DisclosureClass,
        EncodingAttempt, EncodingAttemptRole, EncodingFidelityAttestation, EventCid,
        EvidenceStrength, ExactRatio, FeedInception, FidelityAssessmentStatus, FidelityCheck,
        FidelityCheckKind, FidelityCheckStatus, FidelityPolicy, FrontierAssemblyManifest,
        KnowledgeEventEnvelope, KnowledgeSupersessionPayload, LegacyEncodingClaim, LiteralValue,
        MappingEnvelope, MappingKernel, MappingTermLocator, MappingTransform,
        MaterializationAuthority, MaterializationIntent, MaterializeMappingCommand,
        NamespaceCommitment, ObjectCid, ObjectReference, PlacementId, QuantityLiteral,
        ReceptorAcceptanceProfile, ReceptorCardinality, ReceptorDefinition, ReceptorOrigin,
        ReceptorPlacement, ResolutionAction, ResolutionActionPayload, ResolutionAuthority,
        ResolutionState, ResolutionTarget, ResourceProfile, RevisionRelation, SemanticFrameSet,
        StatementFrame, StatementId, StatementLocator, StatementQualifiers, TermCorrespondence,
        TermRef, UnitRef, UnknownConstraintPolicy, ENCODING_FIDELITY_ATTESTATION_EVENT_TYPE,
        KNOWLEDGE_SUPERSESSION_EVENT_TYPE, RECEPTOR_RESOLUTION_EVENT_TYPE,
    };
    use ku_kql::vnext_proposal::{BindingProposal, ProposalExpiry as KqlProposalExpiry};

    #[test]
    fn mobile_isolation_guard_forbids_tokio_and_std_net_in_onebrain_core() {
        let tokio_token = concat!("tokio", "::");
        let std_net_token = concat!("std::", "net::");
        for (name, src) in [
            ("lib.rs", include_str!("lib.rs")),
            ("error.rs", include_str!("error.rs")),
            ("ku_engine.rs", include_str!("ku_engine.rs")),
            ("composition.rs", include_str!("composition.rs")),
            (
                "fidelity_portfolio.rs",
                include_str!("fidelity_portfolio.rs"),
            ),
        ] {
            assert!(
                !src.contains(tokio_token),
                "{name} must never reference tokio"
            );
            assert!(
                !src.contains(std_net_token),
                "{name} must never reference std::net"
            );
        }
    }

    #[test]
    fn sync_ku_workspace_enforces_lifecycle_supports_search_kql_revisions_and_restart() {
        let dir = tempfile::tempdir().unwrap();
        let vault_path = dir.path().join("vault.redb");
        let journal_path = dir.path().join("journal.redb");

        let c_copper_old = [1u8; 16];
        let c_copper_canon = [2u8; 16];
        let c_conducts = [3u8; 16];
        let c_electricity = [4u8; 16];
        let c_split_parent = [5u8; 16];
        let c_split_b1 = [6u8; 16];
        let c_split_b2 = [7u8; 16];
        let c_retired = [8u8; 16];
        let c_meter = [9u8; 16];

        let mut registry = ConceptRegistry::new();
        let mut add = |ccid: [u8; 16], label: &'static str, category: ConceptCategory| {
            registry.add(
                ResolvedConcept {
                    ccid,
                    qid: 0,
                    category,
                    canonical_name: label.to_string(),
                },
                &[label],
            );
        };
        add(c_copper_old, "cu_legacy", ConceptCategory::Entity);
        add(c_copper_canon, "copper", ConceptCategory::Entity);
        add(c_conducts, "conducts", ConceptCategory::Property);
        add(c_electricity, "electricity", ConceptCategory::Entity);
        add(c_split_parent, "mercury", ConceptCategory::Entity);
        add(c_retired, "phlogiston", ConceptCategory::Entity);
        add(c_meter, "meter", ConceptCategory::Unit);

        let lifecycle = ConceptLifecycleTable::from_records(vec![
            ConceptLifecycleRecord::Redirect(ConceptRedirect {
                deprecated_ccid: ConceptCcid::from_bytes(c_copper_old),
                canonical_ccid: ConceptCcid::from_bytes(c_copper_canon),
                since_release_seq: 1,
                provenance_ref: "dedup-1".to_string(),
            }),
            ConceptLifecycleRecord::Split(ConceptSplit {
                parent_ccid: ConceptCcid::from_bytes(c_split_parent),
                branch_ccids: vec![
                    ConceptCcid::from_bytes(c_split_b1),
                    ConceptCcid::from_bytes(c_split_b2),
                ],
                disambiguation_role_ccid: ConceptCcid::from_bytes(c_conducts),
                since_release_seq: 2,
            }),
            ConceptLifecycleRecord::Retire(ConceptRetire {
                retired_ccid: ConceptCcid::from_bytes(c_retired),
                reason_code: ConceptRetireReason::RetractedUpstream,
                since_release_seq: 3,
            }),
        ])
        .unwrap();
        registry.set_lifecycle_table(lifecycle);

        let saved_v1_cid = {
            let mut ws = SyncKuWorkspace::open(&vault_path, &journal_path, [0x55; 32]).unwrap();

            // Split concept fails closed on prepare!
            let split_err = ws
                .prepare_manual_triple(
                    &registry,
                    "Ambiguous mercury",
                    "mercury",
                    "conducts",
                    "electricity",
                    None,
                    None,
                )
                .unwrap_err();
            assert!(matches!(
                split_err,
                CoreEngineError::SplitDisambiguationRequired { .. }
            ));

            // Retired concept fails closed on prepare!
            let ret_err = ws
                .prepare_manual_triple(
                    &registry,
                    "Obsolete phlogiston",
                    "phlogiston",
                    "conducts",
                    "electricity",
                    None,
                    None,
                )
                .unwrap_err();
            assert!(matches!(ret_err, CoreEngineError::RetiredConcept { .. }));

            // Redirected concept rewrites to canonical CCID with explicit notice!
            let prep = ws
                .prepare_manual_triple(
                    &registry,
                    "Copper conducts electricity",
                    "cu_legacy",
                    "conducts",
                    "electricity",
                    Some("Copper conducts electricity."),
                    None,
                )
                .unwrap();
            assert_eq!(prep.redirect_rewrites.len(), 1);
            assert_eq!(prep.redirect_rewrites[0].canonical_ccid, c_copper_canon);

            let previewed = ws.preview_prepared(&prep.prepare_id).unwrap();
            assert_eq!(previewed, prep);

            let saved = ws.save_prepared(&prep.prepare_id).unwrap();
            assert_eq!(saved.disclosure, "LOCAL_ONLY");

            // Save a second revision with a quantity (unit + dimension) referencing saved v1!
            let v1_cid = ObjectCid::from_bytes(saved.object_cid);
            let qty_draft = SemanticFrameSet {
                statements: vec![StatementFrame {
                    statement_id: StatementId(0),
                    operator_or_predicate: ConceptCcid::from_bytes(c_conducts),
                    arguments: vec![
                        TermRef::Concept(ConceptCcid::from_bytes(c_copper_canon)),
                        TermRef::Literal(LiteralValue::Quantity(QuantityLiteral {
                            value: ExactRatio::new(10, 1).unwrap(),
                            source_unit: UnitRef {
                                unit: ConceptCcid::from_bytes(c_meter),
                                dimension: DimensionVector::new([1, 0, 0, 0, 0, 0, 0]),
                                scale_to_base: ExactRatio::new(1, 1).unwrap(),
                                offset_to_base: ExactRatio::new(0, 1).unwrap(),
                            },
                        })),
                    ],
                    constraints: Vec::new(),
                    qualifiers: StatementQualifiers::default(),
                }],
            };
            let prep_v2 = ws
                .prepare_semantic_draft(
                    &registry,
                    "Copper conducts over 10m",
                    &qty_draft,
                    None,
                    Some(v1_cid),
                )
                .unwrap();
            ws.save_prepared(&prep_v2.prepare_id).unwrap();
            v1_cid
        };

        // Reopen from redb to verify restart durability & index rebuild!
        let reopened = SyncKuWorkspace::open(&vault_path, &journal_path, [0x55; 32]).unwrap();
        assert_eq!(reopened.list().len(), 2);
        let detail = reopened.get(saved_v1_cid).unwrap();
        assert_eq!(
            detail.source_text.as_deref(),
            Some("Copper conducts electricity.")
        );

        // Redirect-expanded search: searching deprecated `cu_legacy` finds both canonical KUs!
        assert_eq!(
            reopened
                .search("concept:cu_legacy", Some(&registry))
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            reopened
                .search(
                    "predicate:conducts unit:meter dimension:1,0,0,0,0,0,0",
                    Some(&registry)
                )
                .unwrap()
                .len(),
            1
        );

        let lineage = reopened.revision_lineage(saved_v1_cid).unwrap();
        assert_eq!(lineage.len(), 2);
        assert_eq!(lineage[0].object_cid, saved_v1_cid);
        assert_eq!(lineage[1].predecessor_cid, Some(saved_v1_cid));

        // Read-only KQL allows FIND and rejects CREATE/WATCH fail-closed.
        let kql_res = reopened.execute_read_only_kql("FIND (k:KU)").unwrap();
        assert_eq!(kql_res.total_count, 2);
        assert!(matches!(
            reopened.execute_read_only_kql("CREATE (k:KU)").unwrap_err(),
            CoreEngineError::KqlRejected(_)
        ));
    }

    #[test]
    fn redb_composition_store_supports_ku_002_to_008_and_supersession_across_restart() {
        let dir = tempfile::tempdir().unwrap();
        let comp_path = dir.path().join("composition.redb");

        let sk = SigningKey::from_bytes(&[0x21; 32]);
        let actor = actor_id_from_root_key(*sk.verifying_key().as_bytes()).unwrap();
        let ns = NamespaceCommitment::derive(b"comp-test", [0x22; 32]).unwrap();
        let dev = DeviceId::from_bytes([0x23; 32]);
        let mut inc = FeedInception::new(*sk.verifying_key().as_bytes(), ns, 0, dev);
        let feed_id = inc.feed_id().unwrap();
        let root_del_bytes = ActorRootDelegation::new(
            *sk.verifying_key().as_bytes(),
            feed_id,
            dev,
            Some(ns),
            0,
            10,
        )
        .unwrap()
        .sign(&sk)
        .unwrap()
        .encode()
        .unwrap();
        let root_cid = ku_core::foundation::decode_actor_root_delegation(&root_del_bytes)
            .unwrap()
            .cid;
        inc.actor_delegation_ref = Some(root_cid.into_bytes());
        let feed = decode_feed_inception(&inc.sign(&sk).unwrap().encode().unwrap()).unwrap();

        let role_ccid = ConceptCcid::from_bytes([0x11; 16]);
        let arg_ccid = ConceptCcid::from_bytes([0x12; 16]);
        let ku_ref = ObjectReference::new(0, [0x44; 32]);
        let policy_ref = ObjectReference::new(0, [0x45; 32]);
        let lineage = AssemblyLineageId::from_bytes([0x46; 32]);
        let placement_id = PlacementId::from_bytes([0x47; 32]);

        let (target, pred_ku) = {
            let store = RedbCompositionStore::open(&comp_path).unwrap();
            store
                .declare_reference_disclosure(&ku_ref, DisclosureClass::LocalOnly)
                .unwrap();
            store
                .declare_reference_disclosure(&policy_ref, DisclosureClass::LocalOnly)
                .unwrap();

            // 1. Author ReceptorDefinition
            let receptor = ReceptorDefinition {
                role: role_ccid,
                expected_types: vec![arg_ccid],
                cardinality: ReceptorCardinality::new(1, Some(1)).unwrap(),
                hard_constraints: Vec::new(),
                acceptance: ReceptorAcceptanceProfile {
                    policy: policy_ref.clone(),
                    required_evidence_kinds: Vec::new(),
                    unknown_constraint_policy: UnknownConstraintPolicy::KeepUnresolved,
                },
                origin: ReceptorOrigin::Declared {
                    source: StatementLocator {
                        object: ku_ref.clone(),
                        statement_index: 0,
                    },
                },
            };
            let rec_cid = store
                .author_receptor_definition(&receptor, DisclosureClass::LocalOnly)
                .unwrap();

            // 2. Project & store virtual KnowledgeAffordance over KU SFS
            let sfs = SemanticFrameSet {
                statements: vec![StatementFrame {
                    statement_id: StatementId(0),
                    operator_or_predicate: role_ccid,
                    arguments: vec![TermRef::Concept(arg_ccid)],
                    constraints: Vec::new(),
                    qualifiers: StatementQualifiers::default(),
                }],
            };
            let (aff_cid, _) = store
                .project_and_store_virtual_affordance(
                    ku_ref.clone(),
                    &sfs,
                    DisclosureClass::LocalOnly,
                )
                .unwrap();

            // 3. Store FrontierAssemblyManifest
            let manifest = FrontierAssemblyManifest {
                lineage,
                revision: 0,
                predecessor: None,
                source_objects: vec![ku_ref.clone()],
                placements: vec![ReceptorPlacement {
                    placement_id,
                    receptor_definition: ObjectReference::new(0, rec_cid.into_bytes()),
                    cardinality: ReceptorCardinality::new(1, Some(1)).unwrap(),
                    required: true,
                    local_context: SemanticFrameSet {
                        statements: Vec::new(),
                    },
                    resolution_policy_override: None,
                }],
                default_resolution_policy: policy_ref.clone(),
            };
            let manifest_cid = store
                .store_assembly_manifest(&manifest, DisclosureClass::LocalOnly)
                .unwrap();

            // 4. Stage BindingProposal & Materialize MappingKernel + MappingEnvelope
            let rec_obj_ref = ObjectReference::new(0, rec_cid.into_bytes());
            let aff_obj_ref = ObjectReference::new(0, aff_cid.into_bytes());
            let kernel = MappingKernel {
                source_objects: vec![rec_obj_ref.clone()],
                target_objects: vec![aff_obj_ref.clone()],
                correspondences: vec![TermCorrespondence {
                    source: MappingTermLocator {
                        object: rec_obj_ref,
                        statement_index: 0,
                        argument_index: Some(0),
                    },
                    target: MappingTermLocator {
                        object: aff_obj_ref.clone(),
                        statement_index: 0,
                        argument_index: Some(0),
                    },
                    kind: CorrespondenceKind::Equivalent,
                    transform: MappingTransform::Identity,
                }],
                assumptions: SemanticFrameSet {
                    statements: Vec::new(),
                },
                constraint_regions: Vec::new(),
                unmapped_regions: Vec::new(),
            };
            let kernel_cid = kernel.cid().unwrap();
            let envelope = MappingEnvelope {
                kernel: kernel_cid,
                generator: policy_ref.clone(),
                derivation_rule: None,
                evidence: vec![ku_ref.clone()],
                source_event: None,
            };
            let proposal = BindingProposal {
                mapping_kernel: kernel.clone(),
                proposed_envelope: envelope.clone(),
                candidate_objects: vec![ObjectReference::new(0, aff_cid.into_bytes())],
                index_commitment: None,
                model_commitment: None,
                rule_commitment: None,
                scores: Vec::new(),
                constraints: Vec::new(),
                expiry: KqlProposalExpiry {
                    created_at_evaluation: 1,
                    expires_after_evaluations: 10,
                    source_frontier: EventCid::from_bytes([0x50; 32]),
                },
                privacy: DisclosureClass::LocalOnly,
            };
            let prop_id = store.stage_binding_proposal(proposal).unwrap();
            assert!(store.get_binding_proposal(prop_id).is_some());

            let mat = store
                .materialize_mapping(
                    &MaterializeMappingCommand {
                        mapping_kernel: kernel,
                        mapping_envelope: envelope,
                        intent: MaterializationIntent::PinPrivate,
                        authorization_ref: None,
                        destination: DisclosureClass::LocalOnly,
                        idempotency_key: [0x61; 32],
                        requester: actor,
                    },
                    MaterializationAuthority::Authorized,
                )
                .unwrap();

            // 5. Record signed ValidatedResolutionEvent -> SatisfiedRelative
            let target = ResolutionTarget {
                assembly_lineage: lineage,
                assembly_revision: manifest_cid,
                placement: placement_id,
            };
            let res_payload = ResolutionActionPayload {
                target,
                action: ResolutionAction::AdoptBinding {
                    mapping: mat.kernel_cid,
                },
                receptor_claim: None,
                acceptance_evidence: vec![ku_ref.clone()],
                resolution_policy: policy_ref.clone(),
                observed_frontier: [0x71; 32],
            };
            let (act_bytes, act_cid) = res_payload
                .to_knowledge_object(DisclosureClass::LocalOnly)
                .unwrap()
                .encode(ResourceProfile::ObjectV1)
                .unwrap();
            let mut res_ev = KnowledgeEventEnvelope::new(
                RECEPTOR_RESOLUTION_EVENT_TYPE,
                feed.feed_id,
                1,
                DisclosureClass::LocalOnly,
                [0x72; 32],
            );
            res_ev.payload_refs = vec![ObjectReference::new(0, act_cid.into_bytes())];
            let (res_ev_bytes, _) = res_ev.sign(&feed, &sk).unwrap().encode().unwrap();

            let (_, view) = store
                .record_resolution_event(
                    &feed,
                    &act_bytes,
                    &res_ev_bytes,
                    ResolutionAuthority::Authorized,
                    Some(BindingAcceptance::Satisfied),
                )
                .unwrap();
            assert_eq!(view.state, ResolutionState::SatisfiedRelative);

            // 6. Record signed KnowledgeSupersessionEvent
            let pred_ku = ObjectCid::from_bytes([0x81; 32]);
            let succ_ku = ObjectCid::from_bytes([0x82; 32]);
            store
                .register_supersession_root_delegation(&feed, &root_del_bytes)
                .unwrap();
            store
                .register_supersession_object_author(pred_ku, actor)
                .unwrap();
            let sup_payload = KnowledgeSupersessionPayload {
                predecessor_object_cid: pred_ku,
                successor_object_cid: succ_ku,
                relation: RevisionRelation::Refines,
                rationale_ref: None,
            };
            let (sup_obj_bytes, sup_obj_cid) = sup_payload
                .to_knowledge_object(DisclosureClass::LocalOnly)
                .unwrap()
                .encode(ResourceProfile::ObjectV1)
                .unwrap();
            let mut sup_ev = KnowledgeEventEnvelope::new(
                KNOWLEDGE_SUPERSESSION_EVENT_TYPE,
                feed.feed_id,
                2,
                DisclosureClass::LocalOnly,
                [0x83; 32],
            );
            sup_ev.payload_refs = vec![ObjectReference::new(0, sup_obj_cid.into_bytes())];
            let (sup_ev_bytes, _) = sup_ev.sign(&feed, &sk).unwrap().encode().unwrap();
            let (_, sup_view) = store
                .record_supersession_event(&feed, &sup_obj_bytes, &sup_ev_bytes)
                .unwrap();
            assert_eq!(sup_view.branches.len(), 1);
            (target, pred_ku)
        };

        // Reopen composition store and confirm both ResolutionView and SupersessionLineageView survive!
        let reopened = RedbCompositionStore::open(&comp_path).unwrap();
        let view = reopened.resolution_view(target).unwrap().unwrap();
        assert_eq!(view.state, ResolutionState::SatisfiedRelative);
        let sup_view = reopened.supersession_lineage(pred_ku);
        assert_eq!(sup_view.branches.len(), 1);
    }

    #[test]
    fn redb_fidelity_portfolio_persists_4d_corroboration_and_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let fid_path = dir.path().join("fidelity.redb");

        let source_ref = ObjectReference::new(0, [1; 32]);
        let encoding_ref = ObjectReference::new(0, [2; 32]);
        let policy_ref = ObjectReference::new(0, [3; 32]);

        let make_att = |seed: u8, seq: u64| {
            let sk = SigningKey::from_bytes(&[seed; 32]);
            let inc = FeedInception::new(
                *sk.verifying_key().as_bytes(),
                NamespaceCommitment::derive(b"fid-port", [seed.wrapping_add(1); 32]).unwrap(),
                0,
                DeviceId::from_bytes([seed.wrapping_add(2); 32]),
            );
            let feed = decode_feed_inception(&inc.sign(&sk).unwrap().encode().unwrap()).unwrap();
            let att = EncodingFidelityAttestation {
                source_artifact: source_ref.clone(),
                candidate_encoding: encoding_ref.clone(),
                blind_attempt_output_commitment: [seed.wrapping_add(10); 32],
                attempt_ref: ObjectReference::new(0, [seed.wrapping_add(20); 32]),
                execution_record_ref: ObjectReference::new(0, [seed.wrapping_add(30); 32]),
                correlation_evidence: CorrelationEvidence {
                    dimensions: vec![
                        CorrelationDimensionEvidence {
                            dimension: CorrelationDimension::AdministrativePrincipal,
                            value_commitment: Some([seed; 32]),
                            strength: EvidenceStrength::CryptoBound,
                            evidence_refs: vec![],
                        },
                        CorrelationDimensionEvidence {
                            dimension: CorrelationDimension::PipelineModelLineage,
                            value_commitment: Some([seed.wrapping_add(40); 32]),
                            strength: EvidenceStrength::ExternallyAttested,
                            evidence_refs: vec![],
                        },
                        CorrelationDimensionEvidence {
                            dimension: CorrelationDimension::ExecutionEnvironment,
                            value_commitment: Some([seed.wrapping_add(50); 32]),
                            strength: EvidenceStrength::ExternallyAttested,
                            evidence_refs: vec![],
                        },
                        CorrelationDimensionEvidence {
                            dimension: CorrelationDimension::CausalStakeholderRelation,
                            value_commitment: Some([seed.wrapping_add(60); 32]),
                            strength: EvidenceStrength::CryptoBound,
                            evidence_refs: vec![],
                        },
                    ],
                },
                checks: vec![
                    FidelityCheck {
                        kind: FidelityCheckKind::SourceSpanAlignment,
                        status: FidelityCheckStatus::ConsistentWithSource,
                        checked_region_commitment: [91; 32],
                        evidence_ref: None,
                    },
                    FidelityCheck {
                        kind: FidelityCheckKind::GeneSelection,
                        status: FidelityCheckStatus::ConsistentWithSource,
                        checked_region_commitment: [92; 32],
                        evidence_ref: None,
                    },
                    FidelityCheck {
                        kind: FidelityCheckKind::ConceptSelection,
                        status: FidelityCheckStatus::ConsistentWithSource,
                        checked_region_commitment: [93; 32],
                        evidence_ref: None,
                    },
                ],
                limitations: vec![],
                policy_ref: policy_ref.clone(),
            };
            let (obj_bytes, obj_cid) = att
                .to_knowledge_object(DisclosureClass::Public)
                .unwrap()
                .encode(ResourceProfile::ObjectV1)
                .unwrap();
            let mut ev = KnowledgeEventEnvelope::new(
                ENCODING_FIDELITY_ATTESTATION_EVENT_TYPE,
                feed.feed_id,
                seq,
                DisclosureClass::Public,
                [seed.wrapping_add(70); 32],
            );
            ev.payload_refs = vec![ObjectReference::new(0, obj_cid.into_bytes())];
            let (ev_bytes, _) = ev.sign(&feed, &sk).unwrap().encode().unwrap();
            (feed, obj_bytes, ev_bytes)
        };

        let target_key = {
            let mut port = RedbFidelityPortfolio::open(&fid_path).unwrap();
            let tk = port
                .register_target(
                    policy_ref.clone(),
                    FidelityPolicy::default_v1(),
                    source_ref.clone(),
                    encoding_ref.clone(),
                    EventCid::from_bytes([9; 32]),
                )
                .unwrap();
            assert_eq!(port.status_view(tk).unwrap().relative_status(), None);

            port.record_publisher_attempt(
                tk,
                ObjectReference::new(0, [10; 32]),
                EncodingAttempt {
                    role: EncodingAttemptRole::Publisher,
                    source_artifact: source_ref.clone(),
                    candidate_encoding: Some(encoding_ref.clone()),
                    output_commitment: [11; 32],
                    pipeline_model_tool_commitments: vec![[12; 32]],
                    source_acquisition_or_derivation_commitment: [13; 32],
                    execution_record_ref: ObjectReference::new(0, [14; 32]),
                    blind_session_commitment: None,
                    challenge_nonce_commitment: None,
                },
            )
            .unwrap();
            assert_eq!(
                port.status_view(tk).unwrap().relative_status(),
                Some(FidelityAssessmentStatus::SelfAttested)
            );

            let (f1, o1, e1) = make_att(20, 1);
            let (f2, o2, e2) = make_att(80, 1);
            port.record_signed_attestation(tk, &f1, &o1, &e1).unwrap();
            port.record_signed_attestation(tk, &f2, &o2, &e2).unwrap();
            port.record_legacy_claim(
                tk,
                LegacyEncodingClaim {
                    source_artifact: source_ref,
                    encoding_artifact: encoding_ref,
                    imported_evidence_ref: ObjectReference::new(0, [99; 32]),
                    adapter_profile_commitment: [98; 32],
                    normalized_claim_commitment: [97; 32],
                    limitations: vec![],
                },
            )
            .unwrap();
            tk
        };

        let reopened = RedbFidelityPortfolio::open(&fid_path).unwrap();
        let view = reopened.status_view(target_key).unwrap();
        assert_eq!(
            view.relative_status(),
            Some(FidelityAssessmentStatus::FidelityCorroboratedRelative)
        );
        assert_eq!(
            view.corroboration_4d.distinct_non_overlapping_group_count,
            2
        );
        assert_eq!(
            view.assessment
                .as_ref()
                .unwrap()
                .coverage
                .normalized_legacy_claims,
            1
        );
        assert!(!view.is_global_or_final());
        assert!(!view.establishes_proposition_truth());
        assert!(!view.blocks_preserve_publish_query_or_use());
    }
}
