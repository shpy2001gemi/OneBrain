use onebrain_mobile_core::{
    run_signed_local_kql_smoke, ActivationArbiter, ActivationPhase, AppLockPolicy, BootstrapStore,
    ExecutionGrant, ExecutionGrantKind, MobileCoreError, MobileFeatureFlags, MobileRuntimeFacade,
    NetworkScope, OnboardingCursor, RegistryChunkRecord, RegistryOperationRecord, ResourceBudgets,
    RuntimeServices, SecurityBootstrapMaterial, SecuritySessionState, TransferLandingRecord,
    SECURITY_BOOTSTRAP_MATERIAL_BYTES,
};
use tempfile::tempdir;

const TEST_HASH: &str = "abababababababababababababababababababababababababababababababab";

#[test]
fn bootstrap_profile_uses_no_model_or_network_and_runs_signed_local_kql() {
    let directory = tempdir().unwrap();
    let facade = MobileRuntimeFacade::open_bootstrap_only(
        RuntimeServices::bootstrap_only(directory.path()),
        MobileFeatureFlags::default(),
        ResourceBudgets::default(),
    )
    .unwrap();
    let snapshot = facade.snapshot();

    assert_eq!(snapshot.profile_version, "MOB-04/1");
    assert_eq!(snapshot.process_generation, 1);
    assert_eq!(snapshot.activation_phase, ActivationPhase::Active);
    assert_eq!(snapshot.active_grant_count, 1);
    assert!(!snapshot.recovered_unclean_start);
    assert!(snapshot.bootstrap_store_opened);
    assert_eq!(snapshot.registry_state, "BootstrapOnly");
    assert!(snapshot.local_kql_fixture_verified);
    assert!(snapshot.private_planner_verified);
    assert_eq!(snapshot.local_kql_rows, 1);
    assert_eq!(snapshot.llm_provider_id, "none");
    assert!(!snapshot.llm_available);
    assert!(!snapshot.signer_available);
    assert!(!snapshot.connectivity_online);
    assert!(!snapshot.background_scheduler_available);
    assert!(snapshot.stale_callback_rejected);
}

#[test]
fn secured_profile_binds_installation_opens_vault_and_locks_without_exposing_signers() {
    let directory = tempdir().unwrap();
    let mut facade = MobileRuntimeFacade::open_secured(
        RuntimeServices::bootstrap_only(directory.path()),
        MobileFeatureFlags::default(),
        ResourceBudgets::default(),
        secure_material(0),
        AppLockPolicy::default(),
    )
    .unwrap();
    let snapshot = facade.snapshot();
    assert!(snapshot.secure_profile_active);
    assert!(snapshot.installation_binding_verified);
    assert!(snapshot.installation_created);
    assert_eq!(
        snapshot.security_session_state,
        SecuritySessionState::Unlocked
    );
    assert!(snapshot.private_vault_ready);
    assert!(snapshot.identity_domains_separated);
    assert!(snapshot.privacy_defaults_fail_safe);
    assert!(snapshot.redacted_history_ready);
    assert!(snapshot.signer_available);

    facade.lock_private_node().unwrap();
    let locked = facade.snapshot();
    assert_eq!(locked.security_session_state, SecuritySessionState::Locked);
    assert!(!locked.private_vault_ready);
    assert!(!locked.signer_available);
    facade.graceful_stop().unwrap();
    drop(facade);

    let store = BootstrapStore::open(&directory.path().join("bootstrap.redb")).unwrap();
    let history = store.recent_security_history(8).unwrap();
    assert_eq!(history[0].event_code, "SECURE_SESSION_LOCKED");
    assert!(history
        .iter()
        .all(|record| !record.event_code.contains("secret")));
}

#[test]
fn matching_platform_material_reopens_but_injected_restore_binding_fails_closed() {
    let directory = tempdir().unwrap();
    let mut first = MobileRuntimeFacade::open_secured(
        RuntimeServices::bootstrap_only(directory.path()),
        MobileFeatureFlags::default(),
        ResourceBudgets::default(),
        secure_material(0),
        AppLockPolicy::default(),
    )
    .unwrap();
    first.graceful_stop().unwrap();
    drop(first);

    let mut reopened = MobileRuntimeFacade::open_secured(
        RuntimeServices::bootstrap_only(directory.path()),
        MobileFeatureFlags::default(),
        ResourceBudgets::default(),
        secure_material(0),
        AppLockPolicy::default(),
    )
    .unwrap();
    assert!(!reopened.snapshot().installation_created);
    reopened.graceful_stop().unwrap();
    drop(reopened);

    assert!(matches!(
        MobileRuntimeFacade::open_secured(
            RuntimeServices::bootstrap_only(directory.path()),
            MobileFeatureFlags::default(),
            ResourceBudgets::default(),
            secure_material(11),
            AppLockPolicy::default(),
        ),
        Err(MobileCoreError::UnexpectedRestore(_))
    ));
}

#[test]
fn onboarding_cursor_resumes_after_kill_and_completion_cannot_regress() {
    let directory = tempdir().unwrap();
    let first = MobileRuntimeFacade::open_secured(
        RuntimeServices::bootstrap_only(directory.path()),
        MobileFeatureFlags::default(),
        ResourceBudgets::default(),
        secure_material(0),
        AppLockPolicy::default(),
    )
    .unwrap();
    assert_eq!(
        first.snapshot().onboarding_cursor,
        OnboardingCursor::Welcome
    );
    first
        .set_onboarding_cursor(OnboardingCursor::Preflight)
        .unwrap();
    first
        .set_onboarding_cursor(OnboardingCursor::Identity)
        .unwrap();
    drop(first);

    let recovered = MobileRuntimeFacade::open_secured(
        RuntimeServices::bootstrap_only(directory.path()),
        MobileFeatureFlags::default(),
        ResourceBudgets::default(),
        secure_material(0),
        AppLockPolicy::default(),
    )
    .unwrap();
    assert_eq!(
        recovered.snapshot().onboarding_cursor,
        OnboardingCursor::Identity
    );
    recovered
        .set_onboarding_cursor(OnboardingCursor::Security)
        .unwrap();
    recovered
        .set_onboarding_cursor(OnboardingCursor::InitHandoff)
        .unwrap();
    recovered
        .set_onboarding_cursor(OnboardingCursor::LimitedHome)
        .unwrap();
    assert!(matches!(
        recovered.set_onboarding_cursor(OnboardingCursor::Welcome),
        Err(MobileCoreError::InvalidArgument(_))
    ));
}

#[test]
fn dropped_runtime_is_an_unclean_start_but_graceful_stop_is_not() {
    let directory = tempdir().unwrap();
    let first = MobileRuntimeFacade::open_bootstrap_only(
        RuntimeServices::bootstrap_only(directory.path()),
        MobileFeatureFlags::default(),
        ResourceBudgets::default(),
    )
    .unwrap();
    assert_eq!(first.snapshot().process_generation, 1);
    drop(first);

    let mut recovered = MobileRuntimeFacade::open_bootstrap_only(
        RuntimeServices::bootstrap_only(directory.path()),
        MobileFeatureFlags::default(),
        ResourceBudgets::default(),
    )
    .unwrap();
    assert_eq!(recovered.snapshot().process_generation, 2);
    assert!(recovered.snapshot().recovered_unclean_start);
    recovered.graceful_stop().unwrap();
    assert_eq!(
        recovered.snapshot().activation_phase,
        ActivationPhase::Dormant
    );
    drop(recovered);

    let clean = MobileRuntimeFacade::open_bootstrap_only(
        RuntimeServices::bootstrap_only(directory.path()),
        MobileFeatureFlags::default(),
        ResourceBudgets::default(),
    )
    .unwrap();
    assert_eq!(clean.snapshot().process_generation, 3);
    assert!(!clean.snapshot().recovered_unclean_start);
}

#[test]
fn registry_operation_chunk_and_transfer_identity_survive_restart() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("bootstrap.redb");
    let budgets = ResourceBudgets::default();
    let store = BootstrapStore::open(&path).unwrap();
    let first = store.start_process(10).unwrap();
    store
        .upsert_registry_operation(
            &RegistryOperationRecord {
                operation_id: "init-release-1".into(),
                release_id: "concepts-2026-07".into(),
                state: "prepared".into(),
            },
            &budgets,
        )
        .unwrap();
    store
        .upsert_registry_chunk(
            &RegistryChunkRecord {
                operation_id: "init-release-1".into(),
                chunk_index: 7,
                expected_hash: TEST_HASH.into(),
                expected_length: 1_048_576,
                state: "requested".into(),
            },
            &budgets,
        )
        .unwrap();
    store
        .prepare_transfer(
            &TransferLandingRecord {
                transfer_nonce: "nonce-stable-across-processes".into(),
                operation_id: "init-release-1".into(),
                release_id: "concepts-2026-07".into(),
                artifact_role: "concepts_obr_chunk".into(),
                chunk_index: 7,
                expected_hash: TEST_HASH.into(),
                expected_length: 1_048_576,
                os_transfer_id: None,
                receiving_process_generation: None,
                app_assigned_callback_sequence: None,
                landed: false,
            },
            &budgets,
        )
        .unwrap();
    store
        .bind_os_transfer(
            "nonce-stable-across-processes",
            "android-download-manager-42",
            &budgets,
        )
        .unwrap();
    drop(store);

    let recovered = BootstrapStore::open(&path).unwrap();
    let second = recovered.start_process(20).unwrap();
    assert_eq!(second.generation, first.generation + 1);
    assert!(second.recovered_unclean_start);
    assert_eq!(
        recovered
            .registry_operation("init-release-1")
            .unwrap()
            .unwrap()
            .release_id,
        "concepts-2026-07"
    );
    assert_eq!(
        recovered
            .registry_chunk("init-release-1", 7)
            .unwrap()
            .unwrap()
            .expected_length,
        1_048_576
    );
    recovered
        .prepare_transfer(
            &TransferLandingRecord {
                transfer_nonce: "nonce-stable-across-processes".into(),
                operation_id: "init-release-1".into(),
                release_id: "concepts-2026-07".into(),
                artifact_role: "concepts_obr_chunk".into(),
                chunk_index: 7,
                expected_hash: TEST_HASH.into(),
                expected_length: 1_048_576,
                os_transfer_id: None,
                receiving_process_generation: None,
                app_assigned_callback_sequence: None,
                landed: false,
            },
            &budgets,
        )
        .unwrap();
    assert_eq!(
        recovered
            .transfer("nonce-stable-across-processes")
            .unwrap()
            .unwrap()
            .os_transfer_id
            .as_deref(),
        Some("android-download-manager-42"),
        "idempotent prepare must preserve OS transfer reassociation"
    );
    assert!(matches!(
        recovered.claim_transfer_callback("nonce-stable-across-processes", first.generation, 1),
        Err(MobileCoreError::StaleGeneration { .. })
    ));
    let claimed = recovered
        .claim_transfer_callback("nonce-stable-across-processes", second.generation, 1)
        .unwrap();
    assert_eq!(
        claimed.os_transfer_id.as_deref(),
        Some("android-download-manager-42")
    );
    assert!(matches!(
        recovered.claim_transfer_callback("nonce-stable-across-processes", second.generation, 1),
        Err(MobileCoreError::StaleCallbackSequence { .. })
    ));
    let landed = recovered
        .mark_transfer_landed("nonce-stable-across-processes", second.generation, 2)
        .unwrap();
    assert!(landed.landed);
}

#[test]
fn repeated_graceful_start_stop_never_reports_an_unclean_generation() {
    let directory = tempdir().unwrap();
    for expected_generation in 1..=8 {
        let mut facade = MobileRuntimeFacade::open_bootstrap_only(
            RuntimeServices::bootstrap_only(directory.path()),
            MobileFeatureFlags::default(),
            ResourceBudgets::default(),
        )
        .unwrap();
        assert_eq!(facade.snapshot().process_generation, expected_generation);
        assert!(!facade.snapshot().recovered_unclean_start);
        facade.graceful_stop().unwrap();
        drop(facade);
    }
}

#[test]
fn activation_grants_are_bounded_expire_and_never_authorize_network() {
    let budgets = ResourceBudgets {
        max_active_grants: 1,
        ..ResourceBudgets::default()
    };
    let mut arbiter = ActivationArbiter::starting(9, &budgets);
    arbiter
        .register_grant(
            ExecutionGrant {
                grant_id: "foreground".into(),
                process_generation: 9,
                kind: ExecutionGrantKind::Foreground,
                user_visible: true,
                deadline_monotonic_ms: Some(100),
                network_scope: NetworkScope::None,
            },
            10,
        )
        .unwrap();
    assert_eq!(arbiter.phase(), ActivationPhase::Active);
    assert!(matches!(
        arbiter.register_grant(
            ExecutionGrant {
                grant_id: "second".into(),
                process_generation: 9,
                kind: ExecutionGrantKind::BackgroundRefresh,
                user_visible: false,
                deadline_monotonic_ms: None,
                network_scope: NetworkScope::None,
            },
            10,
        ),
        Err(MobileCoreError::BudgetExceeded(_))
    ));
    assert_eq!(arbiter.expire_deadlines(100), 1);
    assert_eq!(arbiter.phase(), ActivationPhase::Draining);
    assert!(matches!(
        arbiter.register_grant(
            ExecutionGrant {
                grant_id: "network".into(),
                process_generation: 9,
                kind: ExecutionGrantKind::BackgroundTransferCallback,
                user_visible: false,
                deadline_monotonic_ms: None,
                network_scope: NetworkScope::RegistryOnly,
            },
            101,
        ),
        Err(MobileCoreError::InvalidArgument(_))
    ));
}

#[test]
fn signed_fixture_executes_within_result_budget() {
    let smoke = run_signed_local_kql_smoke(&ResourceBudgets::default()).unwrap();
    assert!(smoke.signature_verified);
    assert!(smoke.query_scope_local);
    assert_eq!(smoke.rows, 1);
    assert!(smoke.private_planner_verified);
}

fn secure_material(offset: u8) -> SecurityBootstrapMaterial {
    let mut bytes = [0u8; SECURITY_BOOTSTRAP_MATERIAL_BYTES];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::try_from((index + usize::from(offset)) % 251 + 1).unwrap();
    }
    SecurityBootstrapMaterial::from_bytes(&bytes).unwrap()
}
