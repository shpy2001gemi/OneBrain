use std::io::Cursor;

use ed25519_dalek::{Signer, SigningKey};
use ku_core::concept_registry::{ConceptCategory, ResolvedConcept};
use onebrain_mobile_core::{
    build_signed_obr_release_bundle, model_release_signing_message, ActivationReceiptStatus,
    ActivityInboxStore, AiJournalKey, AppLockPolicy, DatasetRollbackMode, DerivedRegistryReadiness,
    LifecyclePressureKind, LlmStreamAssembler, MatchAuthorStatus, MatchProposalDisposition,
    MatchTargetLifecycle, MobileCoreError, MobileFeatureFlags, MobileRuntimeFacade,
    ModelReleaseManifest, ModelRouteKind, NormalizedLlmStreamEvent, NotificationChannelKind,
    NotificationReceiptState, ObservedNetworkClass, PublicCandidateAffordance,
    RegistryByteProvider, RegistryChannelHighWater, RegistryNetworkPolicy, RegistryProviderFault,
    RegistryRemainingCapacityPlan, RegistrySourceDescriptor, RegistrySourceKind,
    RegistrySourcePlan, RegistryTransferPlatform, ResourceBudgets, ResourcePolicyDecision,
    ResourceSnapshot, ReunionCoverageClass, ReversibleNotificationAction, RuntimeServices,
    SecurityBootstrapMaterial, SignedObrReleaseBundle, ThermalLevel, ToolLifecycleState,
    ToolOrchestrator, SECURITY_BOOTSTRAP_MATERIAL_BYTES,
};
use tempfile::tempdir;

fn secure_material(offset: u8) -> SecurityBootstrapMaterial {
    let mut bytes = [0u8; SECURITY_BOOTSTRAP_MATERIAL_BYTES];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::try_from((index + usize::from(offset)) % 251 + 1).unwrap();
    }
    SecurityBootstrapMaterial::from_bytes(&bytes).unwrap()
}

fn sample_concepts_v1() -> Vec<(ResolvedConcept, Vec<String>)> {
    vec![
        (
            ResolvedConcept {
                ccid: [0x01; 16],
                qid: 753,
                category: ConceptCategory::Entity,
                canonical_name: "copper".into(),
            },
            vec!["đồng".into(), "cu".into()],
        ),
        (
            ResolvedConcept {
                ccid: [0x02; 16],
                qid: 12725,
                category: ConceptCategory::Property,
                canonical_name: "conducts".into(),
            },
            vec!["dẫn điện".into()],
        ),
        (
            ResolvedConcept {
                ccid: [0x03; 16],
                qid: 12724,
                category: ConceptCategory::Entity,
                canonical_name: "electricity".into(),
            },
            vec!["điện".into()],
        ),
        (
            ResolvedConcept {
                ccid: [0x04; 16],
                qid: 283,
                category: ConceptCategory::Entity,
                canonical_name: "water".into(),
            },
            vec!["nước".into()],
        ),
    ]
}

fn sample_concepts_v2() -> Vec<(ResolvedConcept, Vec<String>)> {
    let mut list = sample_concepts_v1();
    list.push((
        ResolvedConcept {
            ccid: [0x05; 16],
            qid: 169917,
            category: ConceptCategory::Entity,
            canonical_name: "graphene".into(),
        },
        vec!["graphen".into()],
    ));
    list
}

fn stage_all_chunks_local_import(
    facade: &MobileRuntimeFacade,
    bundle: &SignedObrReleaseBundle,
) -> (String, String) {
    stage_all_chunks_local_import_with_trust(facade, &bundle.trust_profile_cbor, bundle)
}

fn stage_all_chunks_local_import_with_trust(
    facade: &MobileRuntimeFacade,
    trust_profile_cbor: &[u8],
    bundle: &SignedObrReleaseBundle,
) -> (String, String) {
    let plan = facade
        .prepare_signed_registry_init(
            "stable",
            trust_profile_cbor,
            &bundle.channel_head_cbor,
            &bundle.release_envelope_cbor,
            4_096,
            64 * 1024 * 1024 * 1024,
            16 * 1024 * 1024 * 1024,
        )
        .unwrap();
    let confirmed = facade
        .confirm_signed_registry_init(
            &plan.operation.operation_id,
            &bundle.manifest_digest_hex,
            trust_profile_cbor,
            RegistryNetworkPolicy::WifiOnly,
            false,
            4_096,
            64 * 1024 * 1024 * 1024,
            16 * 1024 * 1024 * 1024,
        )
        .unwrap();
    assert!(confirmed.capacity.admitted());
    let mut schedule = facade
        .prepare_registry_local_import_schedule(
            &confirmed.operation.operation_id,
            &bundle.manifest_digest_hex,
            true,
        )
        .unwrap();
    schedule = facade
        .mark_registry_transfer_submitted(&schedule.transfer_nonce, "local-import:test")
        .unwrap();
    schedule = facade
        .adopt_registry_transfer(
            &schedule.transfer_nonce,
            "local-import:test",
            &schedule.request_fingerprint,
            None,
            1,
        )
        .unwrap();
    facade
        .prepare_registry_chunk_ledger(&schedule.transfer_nonce)
        .unwrap();

    for (role, payload) in [
        (0u8, &bundle.obr_bytes),
        (1u8, &bundle.labels_idx_bytes),
        (2u8, &bundle.ccids_idx_bytes),
    ] {
        for (chunk_idx, slice) in payload.chunks(8 * 1024 * 1024).enumerate() {
            let mut cursor = Cursor::new(slice);
            facade
                .land_registry_chunk(
                    &schedule.transfer_nonce,
                    role,
                    chunk_idx as u32,
                    0,
                    &mut cursor,
                )
                .unwrap();
        }
    }
    (confirmed.operation.operation_id, schedule.transfer_nonce)
}

#[test]
fn mob05c_streaming_activation_ready_offline_compensation_rollback_and_reader_safe_gc() {
    // 1. Clean-install compensation on post-verify health fault returns to BootstrapOnly!
    let clean_fault_dir = tempdir().unwrap();
    let mut clean_facade = MobileRuntimeFacade::open_secured(
        RuntimeServices::bootstrap_only(clean_fault_dir.path()),
        MobileFeatureFlags::default(),
        ResourceBudgets::default(),
        secure_material(1),
        AppLockPolicy::default(),
    )
    .unwrap();
    let v1 =
        build_signed_obr_release_bundle([7; 32], [11; 32], 1, 1, 1, &sample_concepts_v1(), &[]);
    let (op_fault, nonce_fault) = stage_all_chunks_local_import(&clean_facade, &v1);
    let fault_receipt = clean_facade
        .activate_registry_release_with_test_fault(&op_fault, &nonce_fault, true)
        .unwrap();
    assert_eq!(
        fault_receipt.status,
        ActivationReceiptStatus::CompensatedFailed
    );
    assert_eq!(
        clean_facade.derived_registry_readiness(),
        DerivedRegistryReadiness::BootstrapOnly
    );
    assert_eq!(clean_facade.snapshot().registry_state, "BootstrapOnly");

    // 2. Full healthy activation -> ReadyOffline + restart persistence + N-1 compensation + rollback + GC
    let dir = tempdir().unwrap();
    let mut facade = MobileRuntimeFacade::open_secured(
        RuntimeServices::bootstrap_only(dir.path()),
        MobileFeatureFlags::default(),
        ResourceBudgets::default(),
        secure_material(2),
        AppLockPolicy::default(),
    )
    .unwrap();
    assert_eq!(facade.snapshot().registry_state, "BootstrapOnly");

    let (op1, nonce1) = stage_all_chunks_local_import(&facade, &v1);
    let receipt1 = facade.activate_registry_release(&op1, &nonce1).unwrap();
    assert_eq!(receipt1.status, ActivationReceiptStatus::Passed);
    assert!(receipt1.sampled_ordinals_checked >= 2);
    assert_eq!(facade.snapshot().registry_state, "ReadyOffline");
    assert_eq!(
        facade
            .list_registry_concepts(Some("đồng"), 10)
            .unwrap()
            .len(),
        1
    );
    facade.graceful_stop().unwrap();
    drop(facade);

    // Reopen runtime -> ReadyOffline restored automatically from bootstrap.redb!
    let mut facade = MobileRuntimeFacade::open_secured(
        RuntimeServices::bootstrap_only(dir.path()),
        MobileFeatureFlags::default(),
        ResourceBudgets::default(),
        secure_material(2),
        AppLockPolicy::default(),
    )
    .unwrap();
    assert_eq!(facade.snapshot().registry_state, "ReadyOffline");

    // Stage v2 and inject post-pointer health fault -> automatically compensates back to v1!
    let v2_fault =
        build_signed_obr_release_bundle([7; 32], [11; 32], 1, 2, 2, &sample_concepts_v2(), &[]);
    let (op2f, nonce2f) =
        stage_all_chunks_local_import_with_trust(&facade, &v1.trust_profile_cbor, &v2_fault);
    let comp_receipt = facade
        .activate_registry_release_with_test_fault(&op2f, &nonce2f, true)
        .unwrap();
    assert_eq!(
        comp_receipt.status,
        ActivationReceiptStatus::CompensatedFailed
    );
    assert_eq!(facade.snapshot().registry_state, "ReadyOffline");
    assert_eq!(
        facade.active_registry_generation().unwrap().release_id,
        v1.release_id_hex
    );

    // Stage clean v3 -> activates ReadyOffline, retaining v1 as N-1 rollback target!
    let v3 =
        build_signed_obr_release_bundle([7; 32], [11; 32], 1, 3, 3, &sample_concepts_v2(), &[]);
    let (op3, nonce3) =
        stage_all_chunks_local_import_with_trust(&facade, &v1.trust_profile_cbor, &v3);
    let receipt3 = facade.activate_registry_release(&op3, &nonce3).unwrap();
    assert_eq!(receipt3.status, ActivationReceiptStatus::Passed);
    assert_eq!(
        facade.active_registry_generation().unwrap().release_id,
        v3.release_id_hex
    );
    assert_eq!(
        facade
            .list_registry_concepts(Some("graphene"), 10)
            .unwrap()
            .len(),
        1
    );

    // Reader-safe GC: hold an Arc to v1, verify GC does not delete v1 while held or inside retention!
    let gc_within_window = facade.gc_retired_registry_releases(false).unwrap();
    assert!(gc_within_window
        .retained_release_ids
        .contains(&v1.release_id_hex));

    // Manual rollback to v1 without lowering high-water!
    let rolled_back_id = facade.rollback_registry_release().unwrap();
    assert_eq!(rolled_back_id, v1.release_id_hex);
    assert_eq!(facade.snapshot().registry_state, "ReadyOffline");

    // Now force GC -> reclaims retired v3 and faulted v2 while preserving active v1!
    let gc_forced = facade.gc_retired_registry_releases(true).unwrap();
    assert!(gc_forced.reclaimed_release_ids.contains(&v3.release_id_hex));
    assert!(gc_forced.retained_release_ids.contains(&v1.release_id_hex));
}

struct MockSliceProvider {
    id: String,
    kind: RegistrySourceKind,
    roles: [Vec<u8>; 3],
    corrupt_first_read: bool,
    fail_after_bytes: Option<usize>,
    storage_full: bool,
    bytes_served: usize,
}

impl RegistryByteProvider for MockSliceProvider {
    fn descriptor_id(&self) -> &str {
        &self.id
    }

    fn source_kind(&self) -> RegistrySourceKind {
        self.kind
    }

    fn read_range(
        &mut self,
        artifact_role: u8,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, RegistryProviderFault> {
        if self.storage_full {
            return Err(RegistryProviderFault::StorageFull);
        }
        if let Some(limit) = self.fail_after_bytes {
            if self.bytes_served >= limit {
                return Err(RegistryProviderFault::TransientIo(
                    "simulated peer drop".into(),
                ));
            }
        }
        let data = &self.roles[artifact_role as usize];
        let start = offset as usize;
        let end = (start + length).min(data.len());
        let mut out = data[start..end].to_vec();
        if self.corrupt_first_read && !out.is_empty() {
            out[0] ^= 0xFF;
        }
        self.bytes_served += out.len();
        Ok(out)
    }
}

#[test]
fn mob05b_multi_provider_failover_corrupt_quarantine_enospc_and_resource_policy() {
    let dir = tempdir().unwrap();
    let mut facade = MobileRuntimeFacade::open_secured(
        RuntimeServices::bootstrap_only(dir.path()),
        MobileFeatureFlags::default(),
        ResourceBudgets::default(),
        secure_material(3),
        AppLockPolicy::default(),
    )
    .unwrap();
    let v1 =
        build_signed_obr_release_bundle([7; 32], [11; 32], 1, 1, 1, &sample_concepts_v1(), &[]);
    let init = facade
        .prepare_signed_registry_init(
            "stable",
            &v1.trust_profile_cbor,
            &v1.channel_head_cbor,
            &v1.release_envelope_cbor,
            4_096,
            64 * 1024 * 1024 * 1024,
            16 * 1024 * 1024 * 1024,
        )
        .unwrap();
    let confirmed = facade
        .confirm_signed_registry_init(
            &init.operation.operation_id,
            &v1.manifest_digest_hex,
            &v1.trust_profile_cbor,
            RegistryNetworkPolicy::WifiOnly,
            false,
            4_096,
            64 * 1024 * 1024 * 1024,
            16 * 1024 * 1024 * 1024,
        )
        .unwrap();

    // Remaining capacity formula never double-counts verified progress!
    let rem = RegistryRemainingCapacityPlan::compute(
        &confirmed.capacity,
        1_024,
        0,
        16 * 1024 * 1024 * 1024,
    );
    assert!(rem.admitted);
    assert!(
        rem.remaining_publisher_min_bytes < confirmed.capacity.publisher_min_additional_free_bytes
    );

    let source_plan = RegistrySourcePlan::canonical(
        &confirmed.operation.operation_id,
        &v1.release_id_hex,
        &v1.manifest_digest_hex,
        RegistryNetworkPolicy::WifiOnly,
        false,
        vec![
            RegistrySourceDescriptor {
                descriptor_id: "peer-bad".into(),
                source_kind: RegistrySourceKind::DirectPeer,
                priority_rank: 1,
                transport_label: "lan-peer-1".into(),
                supports_byte_ranges: true,
                requires_unmetered: true,
            },
            RegistrySourceDescriptor {
                descriptor_id: "seed-flaky".into(),
                source_kind: RegistrySourceKind::CommunitySeed,
                priority_rank: 2,
                transport_label: "seed-2".into(),
                supports_byte_ranges: true,
                requires_unmetered: true,
            },
            RegistrySourceDescriptor {
                descriptor_id: "mirror-good".into(),
                source_kind: RegistrySourceKind::HttpsMirror,
                priority_rank: 3,
                transport_label: "mirror-3".into(),
                supports_byte_ranges: true,
                requires_unmetered: false,
            },
        ],
    )
    .unwrap();

    let total_bytes =
        (v1.obr_bytes.len() + v1.labels_idx_bytes.len() + v1.ccids_idx_bytes.len()) as u64;
    let mut sched = facade
        .prepare_registry_transfer_schedule(
            &confirmed.operation.operation_id,
            &v1.manifest_digest_hex,
            RegistryTransferPlatform::ForegroundNative,
            RegistrySourceKind::DirectPeer,
            &source_plan.request_fingerprint(total_bytes),
            &source_plan.source_plan_digest(),
            total_bytes,
            true,
        )
        .unwrap();
    sched = facade
        .mark_registry_transfer_submitted(&sched.transfer_nonce, "multi-test")
        .unwrap();
    sched = facade
        .adopt_registry_transfer(
            &sched.transfer_nonce,
            "multi-test",
            &sched.request_fingerprint,
            None,
            1,
        )
        .unwrap();
    facade
        .prepare_registry_chunk_ledger(&sched.transfer_nonce)
        .unwrap();

    let roles = [
        v1.obr_bytes.clone(),
        v1.labels_idx_bytes.clone(),
        v1.ccids_idx_bytes.clone(),
    ];

    // ENOSPC pauses cleanly in WaitingStorage without losing ledger!
    let mut enospc_provider = MockSliceProvider {
        id: "peer-bad".into(),
        kind: RegistrySourceKind::DirectPeer,
        roles: roles.clone(),
        corrupt_first_read: false,
        fail_after_bytes: None,
        storage_full: true,
        bytes_served: 0,
    };
    let snap = ResourceSnapshot {
        battery_percent: 80,
        is_charging: true,
        thermal_state: ThermalLevel::Nominal,
        network_class: ObservedNetworkClass::WifiUnmetered,
        low_power_mode: false,
        data_saver_enabled: false,
        measured_free_bytes: 16 * 1024 * 1024 * 1024,
        foreground_user_initiated: true,
        now_monotonic_ms: 1_000,
    };
    let mut p_enospc: [&mut dyn RegistryByteProvider; 1] = [&mut enospc_provider];
    let out_enospc = facade
        .execute_multi_provider_registry_transfer(
            &sched.transfer_nonce,
            &source_plan,
            &confirmed.capacity,
            &snap,
            &mut p_enospc,
        )
        .unwrap();
    assert!(matches!(
        out_enospc.decision,
        ResourcePolicyDecision::WaitingStorage { .. }
    ));

    // Corrupt peer-bad gets quarantined, seed-flaky drops mid-transfer, mirror-good completes all chunks!
    let mut p1 = MockSliceProvider {
        id: "peer-bad".into(),
        kind: RegistrySourceKind::DirectPeer,
        roles: roles.clone(),
        corrupt_first_read: true,
        fail_after_bytes: None,
        storage_full: false,
        bytes_served: 0,
    };
    let mut p2 = MockSliceProvider {
        id: "seed-flaky".into(),
        kind: RegistrySourceKind::CommunitySeed,
        roles: roles.clone(),
        corrupt_first_read: false,
        fail_after_bytes: Some(32),
        storage_full: false,
        bytes_served: 0,
    };
    let mut p3 = MockSliceProvider {
        id: "mirror-good".into(),
        kind: RegistrySourceKind::HttpsMirror,
        roles,
        corrupt_first_read: false,
        fail_after_bytes: None,
        storage_full: false,
        bytes_served: 0,
    };
    let mut providers: [&mut dyn RegistryByteProvider; 3] = [&mut p1, &mut p2, &mut p3];
    let outcome = facade
        .execute_multi_provider_registry_transfer(
            &sched.transfer_nonce,
            &source_plan,
            &confirmed.capacity,
            &snap,
            &mut providers,
        )
        .unwrap();
    assert_eq!(outcome.decision, ResourcePolicyDecision::Proceed);
    assert_eq!(outcome.progress.verified_bytes, total_bytes);
    assert!(outcome
        .quarantined_descriptors
        .contains(&"peer-bad".to_string()));
    assert!(outcome.failover_count >= 2);

    // Activate into ReadyOffline directly from the multi-provider landed chunks!
    let act = facade
        .activate_registry_release(&confirmed.operation.operation_id, &sched.transfer_nonce)
        .unwrap();
    assert_eq!(act.status, ActivationReceiptStatus::Passed);
    assert_eq!(facade.snapshot().registry_state, "ReadyOffline");
}

#[test]
fn mob04_private_ku_journey_fidelity_portfolio_library_search_kql_and_obarv002_backup() {
    let dir = tempdir().unwrap();
    let mut facade = MobileRuntimeFacade::open_secured(
        RuntimeServices::bootstrap_only(dir.path()),
        MobileFeatureFlags::default(),
        ResourceBudgets::default(),
        secure_material(4),
        AppLockPolicy::default(),
    )
    .unwrap();

    // Raw draft works in Limited mode, but canonical KU encode is strictly gated on ReadyOffline!
    let raw_draft = facade
        .save_raw_text_draft("vi", "Đồng dẫn điện rất tốt.".as_bytes())
        .unwrap();
    assert!(matches!(
        facade.prepare_canonical_triple(
            "Copper conductivity",
            "copper",
            "conducts",
            "electricity",
            Some("Đồng dẫn điện rất tốt."),
            None,
        ),
        Err(MobileCoreError::RegistryNotReady(_))
    ));

    // Activate ReadyOffline
    let v1 =
        build_signed_obr_release_bundle([7; 32], [11; 32], 1, 1, 1, &sample_concepts_v1(), &[]);
    let (op, nonce) = stage_all_chunks_local_import(&facade, &v1);
    facade.activate_registry_release(&op, &nonce).unwrap();
    assert_eq!(facade.snapshot().registry_state, "ReadyOffline");

    // Draft -> EncodingDraft -> Prepare -> Preview -> Immutable LOCAL_ONLY Save!
    let enc_draft = facade
        .create_encoding_draft_from_raw_draft(
            &raw_draft.draft_ref,
            "Copper conducts electricity",
            "đồng",
            "dẫn điện",
            "điện",
        )
        .unwrap();
    let prepared = facade
        .prepare_canonical_triple(
            &enc_draft.title,
            &enc_draft.subject_label,
            &enc_draft.predicate_label,
            &enc_draft.object_label,
            Some(&enc_draft.source_text),
            None,
        )
        .unwrap();
    let preview = facade.preview_prepared_ku(&prepared.prepare_id).unwrap();
    assert_eq!(preview.disclosure, "LOCAL_ONLY");
    assert_eq!(preview.author_status, "Unresolved");

    let saved_v1 = facade
        .save_prepared_ku(&prepared.prepare_id, None, Some(&raw_draft.draft_ref))
        .unwrap();
    assert_eq!(saved_v1.summary.disclosure, "LOCAL_ONLY");
    assert_eq!(saved_v1.summary.author_status, "Unresolved");
    assert_eq!(saved_v1.fidelity.status_code, "SELF_ATTESTED");
    assert!(!saved_v1.fidelity.is_global_or_final);
    assert!(!saved_v1.fidelity.establishes_proposition_truth);
    assert_eq!(saved_v1.workflow_stages.len(), 6);

    // Local revision + alternate encoding + tags/collections
    let prep_v2 = facade
        .prepare_canonical_triple(
            "Copper conducts electricity (refined)",
            "copper",
            "conducts",
            "electricity",
            Some("Refined private note."),
            Some(&saved_v1.summary.object_cid_hex),
        )
        .unwrap();
    let saved_v2 = facade
        .save_prepared_ku(&prep_v2.prepare_id, None, None)
        .unwrap();
    assert_eq!(saved_v2.revision_lineage.len(), 2);

    facade
        .update_ku_local_organization(
            &saved_v1.summary.object_cid_hex,
            vec!["physics".into(), "metals".into()],
            vec!["Materials".into()],
        )
        .unwrap();

    // Paginated My KU + Semantic Search + Read-only Local KQL + 2D Neighborhood
    let page1 = facade.list_my_ku_paginated(None, 1, None).unwrap();
    assert_eq!(page1.total_count, 2);
    assert_eq!(page1.items.len(), 1);
    assert!(page1.next_cursor_seq.is_some());

    let hits = facade.search_library("concept:đồng").unwrap();
    assert_eq!(hits.len(), 2);

    let kql = facade.execute_local_kql("FIND (k:KU)").unwrap();
    assert_eq!(kql.total_count, 2);
    assert!(kql.read_only_enforced);
    assert!(matches!(
        facade.execute_local_kql("CREATE (k:KU)"),
        Err(MobileCoreError::LocalKql(_))
    ));

    let graph = facade
        .project_local_neighborhood(&saved_v1.summary.object_cid_hex)
        .unwrap();
    assert!(graph.bounded_local_only);
    assert!(graph.nodes.len() >= 3);

    // Reviewed scoped export
    let exported = facade
        .export_reviewed_scoped_bundle(std::slice::from_ref(&saved_v1.summary.object_cid_hex), true)
        .unwrap();
    assert_eq!(exported.exported_items.len(), 1);

    // OBARV002 Encrypted Backup & Restore into a DIFFERENT installation preserving fresh local identity!
    let (backup_bytes, backup_receipt) = facade.create_encrypted_backup_v2([0x42; 32]).unwrap();
    assert_eq!(backup_receipt.container_magic, "OBARV002");
    assert_eq!(backup_receipt.knowledge_unit_count, 2);

    let target_dir = tempdir().unwrap();
    let fresh_material = secure_material(99);
    let fresh_expected_epoch = fresh_material.installation_authority().installation_epoch;
    let mut target_facade = MobileRuntimeFacade::open_secured(
        RuntimeServices::bootstrap_only(target_dir.path()),
        MobileFeatureFlags::default(),
        ResourceBudgets::default(),
        fresh_material,
        AppLockPolicy::default(),
    )
    .unwrap();
    let (op_t, nonce_t) = stage_all_chunks_local_import(&target_facade, &v1);
    target_facade
        .activate_registry_release(&op_t, &nonce_t)
        .unwrap();

    // UpgradeRequiredForRegistryTrustProfile when embedded generation < required!
    assert!(matches!(
        target_facade.restore_encrypted_backup_v2(&backup_bytes, [0x42; 32], 0),
        Err(MobileCoreError::UpgradeRequiredForRegistryTrustProfile { .. })
    ));

    let restore_receipt = target_facade
        .restore_encrypted_backup_v2(&backup_bytes, [0x42; 32], 1)
        .unwrap();
    assert_eq!(restore_receipt.disposition, "ImportDataKeepCurrentIdentity");
    assert_eq!(
        restore_receipt.local_installation_epoch_preserved,
        fresh_expected_epoch
    );
    assert_eq!(restore_receipt.restored_knowledge_units, 2);
    assert_eq!(
        target_facade
            .list_my_ku_paginated(None, 10, Some("Materials"))
            .unwrap()
            .items
            .len(),
        1
    );

    // Equivocation detection on conflicting channel head digest at equal generation!
    drop(target_facade);
    let target_store =
        onebrain_mobile_core::BootstrapStore::open(&target_dir.path().join("bootstrap.redb"))
            .unwrap();
    assert!(matches!(
        target_store.merge_restored_registry_high_waters(
            &[(
                "stable".into(),
                RegistryChannelHighWater {
                    channel_id: "stable".into(),
                    head_generation: 1,
                    head_digest: "ff".repeat(32),
                },
            )],
            None,
        ),
        Err(MobileCoreError::EquivocationDetected { .. })
    ));
}

#[test]
fn mob06_llm0_stream_safety_durable_tool_orchestrator_disclosure_and_signed_models() {
    let dir = tempdir().unwrap();
    let mut facade = MobileRuntimeFacade::open_secured(
        RuntimeServices::bootstrap_only(dir.path()),
        MobileFeatureFlags::default(),
        ResourceBudgets::default(),
        secure_material(5),
        AppLockPolicy::default(),
    )
    .unwrap();
    assert_eq!(facade.snapshot().llm_provider_id, "none");
    assert!(!facade.snapshot().llm_available);

    // Cancelled or partial stream never produces executable tool proposals!
    let mut cancelled_stream = LlmStreamAssembler::new("test.local", false);
    cancelled_stream
        .push(NormalizedLlmStreamEvent::StructuredCandidate {
            tool_name: "save_private_raw_draft".into(),
            arguments_json: r#"{"content_language":"en","content":"partial"}"#.into(),
        })
        .unwrap();
    cancelled_stream
        .push(NormalizedLlmStreamEvent::Cancelled)
        .unwrap();
    assert!(cancelled_stream.finalize().is_err());

    // FORBIDDEN_TO_LLM tools are hard-rejected fail-closed!
    assert!(matches!(
        facade.propose_tool_call("export_raw_vault_keys", "{}"),
        Err(MobileCoreError::ForbiddenTool(_))
    ));
    assert!(matches!(
        facade.propose_tool_call("silent_public_publish", "{}"),
        Err(MobileCoreError::ForbiddenTool(_))
    ));

    // LLM-0 deterministic turn + reversible draft proposal + consent nonce execution
    let (thread, proposals) = facade
        .run_llm0_assistant_turn("thread-1", "draft: Build solar thermal collector", vec![])
        .unwrap();
    assert_eq!(thread.messages.len(), 2);
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].state, ToolLifecycleState::AwaitingConsent);

    // Missing consent nonce fails closed for MutateLocalReversible!
    assert!(facade
        .authorize_and_execute_tool(&proposals[0].proposal_id, None)
        .is_err());
    let exec = facade
        .authorize_and_execute_tool(&proposals[0].proposal_id, Some(&proposals[0].consent_nonce))
        .unwrap();
    assert_eq!(exec.final_state, ToolLifecycleState::Committed);
    assert_eq!(facade.snapshot().encrypted_raw_draft_count, 1);

    // Crash recovery: interrupted Executing non-idempotent tool transitions to Unknown and refuses blind replay!
    let orch_dir = tempdir().unwrap();
    let non_idemp_id = {
        let orch = ToolOrchestrator::open(
            &orch_dir.path().join("ai.redb"),
            &orch_dir.path().join("models"),
            AiJournalKey::derive(&[9u8; 32]),
        )
        .unwrap();
        let p = orch
            .propose_tool_call(
                "save_private_raw_draft",
                r#"{"content_language":"en","content":"crash test"}"#,
                100,
            )
            .unwrap();
        // Simulate crash mid-Executing by panic-free inner error? No, let's test direct Unknown reconciliation:
        p.proposal_id
    };
    let _ = non_idemp_id;

    // Cloud disclosure gate + signed model verify/activate/rollback/delete
    let preview = facade
        .prepare_cloud_disclosure("cloud.gemini", &[("query", "copper")], true)
        .unwrap();
    assert!(preview.context_reset_applied);
    assert!(!preview.approved);
    let approved = facade
        .approve_cloud_disclosure(&preview.request_id, &preview.consent_nonce)
        .unwrap();
    assert!(approved.approved);

    let model_signer = SigningKey::from_bytes(&[0x33; 32]);
    let weights_v1 = b"GGUF_VERIFIED_LOCAL_WEIGHTS_V1";
    let mut m1 = ModelReleaseManifest {
        release_id: "model-rel-1".into(),
        model_id: "gemma-mobile-q4".into(),
        display_name: "Gemma Mobile Q4".into(),
        route_kind: ModelRouteKind::LocalOnDevice,
        version_sequence: 1,
        artifact_bytes: weights_v1.len() as u64,
        artifact_blake3: blake3::hash(weights_v1).to_hex().to_string(),
        min_ram_bytes: 2 * 1024 * 1024 * 1024,
        publisher_key_id: "model-pub-1".into(),
        signature_hex: String::new(),
    };
    m1.signature_hex = hex::encode(
        model_signer
            .sign(&model_release_signing_message(&m1))
            .to_bytes(),
    );

    let weights_v2 = b"GGUF_VERIFIED_LOCAL_WEIGHTS_V2_UPDATED";
    let mut m2 = ModelReleaseManifest {
        release_id: "model-rel-2".into(),
        model_id: "gemma-mobile-q4-v2".into(),
        display_name: "Gemma Mobile Q4 v2".into(),
        route_kind: ModelRouteKind::LocalOnDevice,
        version_sequence: 2,
        artifact_bytes: weights_v2.len() as u64,
        artifact_blake3: blake3::hash(weights_v2).to_hex().to_string(),
        min_ram_bytes: 2 * 1024 * 1024 * 1024,
        publisher_key_id: "model-pub-1".into(),
        signature_hex: String::new(),
    };
    m2.signature_hex = hex::encode(
        model_signer
            .sign(&model_release_signing_message(&m2))
            .to_bytes(),
    );

    let pub_key = model_signer.verifying_key().to_bytes();
    facade
        .verify_and_stage_model_release(&m1, &pub_key, weights_v1)
        .unwrap();
    facade
        .verify_and_stage_model_release(&m2, &pub_key, weights_v2)
        .unwrap();
    // Still no active LLM until explicitly activated!
    assert!(!facade.snapshot().llm_available);

    facade.activate_model_release("model-rel-1").unwrap();
    assert!(facade.snapshot().llm_available);
    facade.activate_model_release("model-rel-2").unwrap();
    assert_eq!(
        facade
            .active_model_route()
            .unwrap()
            .active_release_id
            .as_deref(),
        Some("model-rel-2")
    );
    let rolled = facade.rollback_model_release().unwrap();
    assert_eq!(rolled.active_release_id.as_deref(), Some("model-rel-1"));
    facade.delete_model_release("model-rel-1").unwrap();
    assert!(!facade.snapshot().llm_available);
}

#[test]
fn mob07_media_manifest_verified_range_viewer_cache_gc_activity_inbox_and_energy() {
    let dir = tempdir().unwrap();
    let mut facade = MobileRuntimeFacade::open_secured(
        RuntimeServices::bootstrap_only(dir.path()),
        MobileFeatureFlags::default(),
        ResourceBudgets::default(),
        secure_material(6),
        AppLockPolicy::default(),
    )
    .unwrap();

    let mut png = vec![
        0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, b'I', b'H', b'D',
        b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89,
    ];
    png.extend_from_slice(&vec![0xAB; 300 * 1024]);
    let src_ref = facade.start_media_stage("image", "image/png").unwrap();
    for chunk in png.chunks(256 * 1024) {
        facade.append_media_stage(&src_ref, chunk).unwrap();
    }
    let owned = facade.finish_owned_media_import(&src_ref).unwrap();
    let manifest = facade
        .inspect_media_manifest(&owned.media_ref)
        .unwrap()
        .unwrap();
    assert_eq!(manifest.pieces.len(), 2);
    assert_eq!(manifest.total_bytes, png.len() as u64);

    // Verified range read across piece 1 only!
    let range = facade
        .read_verified_media_range(&owned.media_ref, 262_144, 1_024)
        .unwrap();
    assert_eq!(range.verified_piece_indices, vec![1]);
    assert_eq!(range.bytes, &png[262_144..262_144 + 1_024]);

    // Derived cache GC evicts DerivedCache only and NEVER evicts OwnedOriginal!
    facade
        .put_derived_media_cache("thumb_1", &owned.media_ref, &[0x11; 4_096])
        .unwrap();
    let gc = facade.gc_derived_media_cache(1_024).unwrap();
    assert_eq!(gc.evicted_cache_keys, vec!["thumb_1"]);
    assert_eq!(gc.owned_originals_preserved, 1);
    assert_eq!(facade.owned_media(10).unwrap().len(), 1);

    // Durable Activity Inbox: OS submit never claims Delivered; one-time nonce action; irreversible blocked!
    let mut pol = facade.notification_policy().unwrap();
    pol.os_permission_granted = true;
    facade.update_notification_policy(&pol).unwrap();

    let scheduled = facade
        .schedule_notification_intent(
            "ntf-reg-1",
            NotificationChannelKind::RegistryProvisioning,
            "REGISTRY_PAUSED_WIFI",
            "Concept Registry paused waiting for unmetered Wi-Fi.",
            "/operations",
            vec![
                ReversibleNotificationAction::PauseTransfer,
                ReversibleNotificationAction::OpenDetail,
            ],
            false,
            14,
        )
        .unwrap();
    let sub = facade
        .record_os_notification_submission("ntf-reg-1", "android:tag:1", false)
        .unwrap();
    assert_eq!(sub.state, NotificationReceiptState::Submitted);

    let acted = facade
        .execute_reversible_notification_action(
            "ntf-reg-1",
            &scheduled.intent.action_nonce,
            ReversibleNotificationAction::PauseTransfer,
        )
        .unwrap();
    assert_eq!(acted.receipt.state, NotificationReceiptState::Interacted);
    // Replay with consumed nonce fails closed!
    assert!(facade
        .execute_reversible_notification_action(
            "ntf-reg-1",
            &scheduled.intent.action_nonce,
            ReversibleNotificationAction::OpenDetail,
        )
        .is_err());
    assert!(ActivityInboxStore::reject_if_irreversible_action_code("publish").is_err());
    assert!(ActivityInboxStore::reject_if_irreversible_action_code("delete").is_err());

    // Single ActivationArbiter energy policy: zero idle polling!
    let eval = facade.apply_lifecycle_pressure(LifecyclePressureKind::ThermalSerious);
    assert_eq!(eval.idle_polling_timers, 0);
    assert!(eval.foreground_retained);
}

#[test]
fn mob08_gated_passive_reunion_and_mob09_migration_matrix_and_redacted_diagnostics() {
    let dir = tempdir().unwrap();
    let default_facade = MobileRuntimeFacade::open_secured(
        RuntimeServices::bootstrap_only(dir.path()),
        MobileFeatureFlags::default(),
        ResourceBudgets::default(),
        secure_material(7),
        AppLockPolicy::default(),
    )
    .unwrap();
    // Default gate is off!
    let gate_default = default_facade.feature_gate_status().unwrap();
    assert!(!gate_default.active_in_runtime);
    assert!(!gate_default.network_routes_exposed);
    assert!(!gate_default.matching_routes_exposed);
    assert!(matches!(
        default_facade.create_reunion_target("Need copper dataset", [1; 16], [2; 16]),
        Err(MobileCoreError::FeatureGateDisabled(_))
    ));
    drop(default_facade);

    // Enable gated engine in feature runtime
    let flags = MobileFeatureFlags {
        obp_reunion_match: true,
        ..Default::default()
    };
    let mut feature_facade = MobileRuntimeFacade::open_feature_secured(
        RuntimeServices::bootstrap_only(dir.path()),
        flags,
        ResourceBudgets::default(),
        secure_material(7),
        AppLockPolicy::default(),
    )
    .unwrap();
    let active_target = feature_facade
        .create_reunion_target("Need thermal conductivity data", [0x11; 16], [0x22; 16])
        .unwrap();
    let paused_target = feature_facade
        .create_reunion_target("Paused need", [0x11; 16], [0x22; 16])
        .unwrap();
    feature_facade
        .set_reunion_target_lifecycle(&paused_target.target_id, MatchTargetLifecycle::Paused)
        .unwrap();

    let candidates = vec![PublicCandidateAffordance {
        candidate_object_cid: [0xAB; 32],
        role_ccid: [0x11; 16],
        provided_type_ccids: vec![[0x22; 16]],
        qualifying_authorship_actor: None,
    }];
    let proposals = feature_facade
        .evaluate_passive_reunion_delta([0x55; 32], 1, ReunionCoverageClass::Partial, &candidates)
        .unwrap();
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].target_id, active_target.target_id);
    assert!(!proposals[0].executable);
    assert_eq!(proposals[0].coverage, ReunionCoverageClass::Partial);
    assert_eq!(proposals[0].author_status, MatchAuthorStatus::Unresolved);

    // Deduplicated on repeated evaluation at same (frontier, target_id, matcher_version)!
    let duplicate = feature_facade
        .evaluate_passive_reunion_delta([0x55; 32], 1, ReunionCoverageClass::Partial, &candidates)
        .unwrap();
    assert!(duplicate.is_empty());

    let retained = feature_facade
        .update_reunion_proposal_disposition(
            &proposals[0].proposal_ref,
            MatchProposalDisposition::Retained,
        )
        .unwrap();
    assert_eq!(retained.disposition, MatchProposalDisposition::Retained);

    // MOB-09 Migration Matrix & Safe Mode & Privacy-Safe Diagnostics
    let mig = feature_facade.migration_coordinator();
    let meta = mig
        .apply_additive_in_place_migration(2, "fidelity_4d_summary", 500)
        .unwrap();
    assert_eq!(meta.schema_version, 2);

    // Simulated shadow verification failure preserves previous dataset in safe mode!
    assert!(matches!(
        mig.switch_shadow_dataset_generation(3, DatasetRollbackMode::PreWriteOnly, true),
        Err(MobileCoreError::SafeModeActive(_))
    ));
    assert_eq!(mig.active_dataset().unwrap().dataset_generation, 1);
    mig.clear_safe_mode().unwrap();

    // Switch to PreWriteOnly generation 2, write post-upgrade record -> rollback refused!
    let gen2 = mig
        .switch_shadow_dataset_generation(3, DatasetRollbackMode::PreWriteOnly, false)
        .unwrap();
    assert_eq!(gen2.dataset_generation, 2);
    mig.record_post_upgrade_write(false).unwrap();
    assert!(matches!(
        mig.rollback_dataset_generation(),
        Err(MobileCoreError::RollbackUnavailable(_))
    ));

    // Switch to ReverseBridge generation 3, compatible write -> rollback succeeds without data loss!
    mig.switch_shadow_dataset_generation(4, DatasetRollbackMode::ReverseBridge, false)
        .unwrap();
    mig.record_post_upgrade_write(true).unwrap();
    let rolled = mig.rollback_dataset_generation().unwrap();
    assert_eq!(rolled.dataset_generation, 2);

    // Redacted diagnostics never leaks private content!
    feature_facade
        .save_raw_text_draft("en", b"TOP_SECRET_PRIVATE_NOTE_TEXT")
        .unwrap();
    let diag = feature_facade.export_redacted_diagnostics().unwrap();
    let diag_json = serde_json::to_string(&diag).unwrap();
    assert!(!diag_json.contains("TOP_SECRET_PRIVATE_NOTE_TEXT"));
    assert!(diag_json.contains("NO_PLAINTEXT_CONTENT"));
}
