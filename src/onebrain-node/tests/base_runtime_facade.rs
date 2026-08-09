use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use onebrain_base_contract::{
    ActorRootPublicIdV1, ArchiveCapabilityHandleV1, ArchiveChunkV1, ArchiveCredentialKindV1,
    ArchiveRestorePolicyV1, ArchiveSourceBeginV1, ArchiveSourceHandleV1, ArchiveSourcePushV1,
    BaseCapabilityRequirements, BaseCapabilitySet, BaseCommandV1, BaseCompatibilityPolicy,
    BaseCompatibilityTuple, BaseConfirmRequestV1, BaseIdempotencyKey, BaseLocalCommandV1,
    BaseManagementRequestV1, BaseOperationId, BaseOperationKindV1, BasePollEventsRequestV1,
    BasePrepareRequestV1, BaseReleaseVersion, BaseRequestV1, BaseSubscriptionId,
    BaseSubscriptionRequestV1, BoundedSecretIngressV1, CompatibilityDigestV1,
    CompleteSignerReprovisionV1, NegotiatedVersions, ProfileVersion, ResourceBudgetV1,
    SignerDomainV1, SignerProvisionHandleV1, SignerPublicIdV1, SourceCommitIdentity,
    StorageSchemaVersion, TargetTriple, ToolchainIdentity, TopicKindV1, TypedPayloadV1,
    COMPILED_BASE_COMMIT, COMPILED_TARGET_TRIPLE, COMPILED_TOOLCHAIN,
    MAX_BASE_ARCHIVE_DATASET_BYTES,
};
use onebrain_node::base_operation_store::BaseOperationStoreError;
use onebrain_node::{
    ActorRootIdentity, ActorRootPublicKey, BaseHostAuthorizer, BaseLocalOperationAdapter,
    BaseManagementResponseV1, BaseManagementScope, BaseResponseV1, BaseRuntime, BaseRuntimeConfig,
    BaseRuntimeLifecycle, BaseServiceError, BaseServices, DatasetGenerationStore,
    DatasetPathResolver, ExpectedSignerIdentity, IdentityDomain, NodeError, ProcessGenerationId,
    ProcessGenerationIdSource, SignerCapabilitySet, SignerError, SignerPossessionChallengeV1,
    SignerPossessionProof, SignerProvider, SignerProviderId, SignerProviderRegistry,
};

struct TestAuthorizer;

struct EmptySignerRegistry;

impl SignerProviderRegistry for EmptySignerRegistry {
    fn resolve(&self, _id: &SignerProviderId) -> Result<Arc<dyn SignerProvider>, SignerError> {
        Err(SignerError::UnknownProvider)
    }
}

impl BaseHostAuthorizer for TestAuthorizer {
    fn authenticate(&self, principal: [u8; 32], proof: &[u8]) -> bool {
        principal == [9; 32] && proof == b"authenticated-local-host"
    }
}

struct TestLocalAdapter;

struct FixedProcessGenerationSource(ProcessGenerationId);

impl ProcessGenerationIdSource for FixedProcessGenerationSource {
    fn next_id(&self) -> Result<ProcessGenerationId, BaseOperationStoreError> {
        Ok(self.0)
    }
}

struct FailingProcessGenerationSource;

impl ProcessGenerationIdSource for FailingProcessGenerationSource {
    fn next_id(&self) -> Result<ProcessGenerationId, BaseOperationStoreError> {
        Err(BaseOperationStoreError::EntropyUnavailable)
    }
}

impl BaseLocalOperationAdapter for TestLocalAdapter {
    fn query(
        &self,
        request: onebrain_base_contract::BaseQueryRequestV1,
    ) -> Result<
        (
            TypedPayloadV1,
            Option<onebrain_base_contract::BaseOpaqueContinuation>,
        ),
        BaseServiceError,
    > {
        Ok((request.payload, request.continuation))
    }

    fn confirm_local(&self, command: BaseLocalCommandV1) -> Result<Vec<u8>, BaseServiceError> {
        Ok(command.payload.as_bytes().to_vec())
    }
}

fn runtime_config() -> BaseRuntimeConfig {
    let registry = ku_core::foundation::base_v1_profile_registry();
    let tuple = BaseCompatibilityTuple {
        base_version: BaseReleaseVersion {
            major: 1,
            minor: 1,
            patch: 0,
            prerelease: None,
        },
        base_commit: match COMPILED_BASE_COMMIT {
            SourceCommitIdentity::Known(value) => SourceCommitIdentity::Known(value),
            SourceCommitIdentity::Unknown => SourceCommitIdentity::Unknown,
        },
        canonical_schema_digest: CompatibilityDigestV1(registry.canonical_schema_digest),
        domain_registry_digest: CompatibilityDigestV1(registry.domain_registry_digest),
        resource_registry_digest: CompatibilityDigestV1(registry.resource_registry_digest),
        storage_schema: StorageSchemaVersion(1),
        archive_profile: ProfileVersion { major: 1, minor: 0 },
        migration_profile: ProfileVersion { major: 1, minor: 0 },
        registry_profile: ProfileVersion { major: 1, minor: 0 },
        registry_profile_digest: CompatibilityDigestV1([4; 32]),
        wire_session: ProfileVersion { major: 1, minor: 0 },
        product_api: ProfileVersion { major: 1, minor: 1 },
        c_abi: ProfileVersion { major: 1, minor: 0 },
        feature_set_digest: CompatibilityDigestV1([5; 32]),
        target_triple: TargetTriple::try_from_string(COMPILED_TARGET_TRIPLE.to_owned()).unwrap(),
        toolchain: match COMPILED_TOOLCHAIN {
            ToolchainIdentity::Known(value) => ToolchainIdentity::Known(value),
            ToolchainIdentity::Unknown => ToolchainIdentity::Unknown,
        },
    };
    let archive_restore = ArchiveRestorePolicyV1 {
        canonical_schema_digest: tuple.canonical_schema_digest,
        domain_registry_digest: tuple.domain_registry_digest,
        resource_registry_digest: tuple.resource_registry_digest,
        storage_schema: tuple.storage_schema,
        archive_profile: tuple.archive_profile,
        migration_profile: tuple.migration_profile,
        max_dataset_bytes: MAX_BASE_ARCHIVE_DATASET_BYTES,
    };
    let status = tuple.clone().unqualified_status();
    let empty = || BaseCapabilitySet::try_from_discriminators(Vec::new()).unwrap();
    let mut config = BaseRuntimeConfig::new(
        BaseCompatibilityPolicy {
            current: tuple,
            minimum_additive: NegotiatedVersions {
                base_minor: 0,
                wire_session_minor: 0,
                product_api_minor: 0,
                c_abi_minor: 0,
            },
            archive_restore,
        },
        status,
        BaseCapabilityRequirements {
            supported: empty(),
            required: empty(),
        },
    );
    config.host_authorizer = Arc::new(TestAuthorizer);
    config.local_adapter = Arc::new(TestLocalAdapter);
    config
}

fn assert_weak_services(_: &BaseServices) {}

#[tokio::test]
async fn offline_runtime_durably_confirms_and_reconciles_one_local_effect() {
    let temp = tempfile::tempdir().unwrap();
    let generations = Arc::new(DatasetGenerationStore::open_exclusive(temp.path()).unwrap());
    let mut runtime = BaseRuntime::open(generations, runtime_config()).unwrap();
    let services = runtime.services_for_principal([9; 32]).unwrap();
    assert_weak_services(&services);
    let status = services.snapshot().unwrap();
    assert_eq!(status.lifecycle, BaseRuntimeLifecycle::Open);
    assert!(status.local_usable);
    assert!(!status.network_enabled);

    let reservation = match services
        .invoke(BaseRequestV1::ReserveOperation(
            BaseOperationKindV1::ExistingLocalCommand,
        ))
        .await
        .unwrap()
    {
        BaseResponseV1::Reserved(value) => value,
        _ => panic!("unexpected reservation response"),
    };
    let payload = TypedPayloadV1::try_from_bytes(b"canonical-local-effect".to_vec()).unwrap();
    let prepared = match services
        .invoke(BaseRequestV1::Prepare(BasePrepareRequestV1 {
            reservation_id: reservation,
            command: BaseCommandV1::ExistingLocalCommand(BaseLocalCommandV1 { kind: 7, payload }),
        }))
        .await
        .unwrap()
    {
        BaseResponseV1::Prepared(value) => value,
        _ => panic!("unexpected prepare response"),
    };
    let idempotency = BaseIdempotencyKey([8; 32]);
    let first = match services
        .invoke(BaseRequestV1::Confirm(BaseConfirmRequestV1 {
            operation_id: prepared.operation_id,
            idempotency_key: idempotency,
        }))
        .await
        .unwrap()
    {
        BaseResponseV1::Receipt(value) => value,
        _ => panic!("unexpected confirm response"),
    };
    assert_eq!(first.result, b"canonical-local-effect");
    let duplicate = match services
        .invoke(BaseRequestV1::Confirm(BaseConfirmRequestV1 {
            operation_id: prepared.operation_id,
            idempotency_key: idempotency,
        }))
        .await
        .unwrap()
    {
        BaseResponseV1::Receipt(value) => value,
        _ => panic!("unexpected idempotent response"),
    };
    assert_eq!(duplicate.attempts, 1);

    let subscription = match services
        .invoke(BaseRequestV1::Subscribe(BaseSubscriptionRequestV1 {
            topic: TopicKindV1::OperationReceipts,
            cursor: Some(0),
        }))
        .await
        .unwrap()
    {
        BaseResponseV1::Subscription(value) => *value.as_bytes(),
        _ => panic!("unexpected subscription response"),
    };
    let batch = services
        .poll_events(BasePollEventsRequestV1 {
            subscription_id: BaseSubscriptionId::from_opaque_bytes(subscription),
            after_cursor: 0,
            max_items: 256,
        })
        .await
        .unwrap();
    assert!(!batch.events.is_empty());
    services
        .close_subscription(BaseSubscriptionId::from_opaque_bytes(subscription))
        .await
        .unwrap();
    services
        .close_subscription(BaseSubscriptionId::from_opaque_bytes(subscription))
        .await
        .unwrap();

    let drained = services.drain().await.unwrap();
    assert_eq!(drained.lifecycle, BaseRuntimeLifecycle::Draining);
    assert!(services
        .invoke(BaseRequestV1::ReserveOperation(
            BaseOperationKindV1::ExistingLocalCommand,
        ))
        .await
        .is_err());
    runtime.close().await.unwrap();
    assert!(services.snapshot().is_err());
}

#[tokio::test]
async fn runtime_owner_is_unique_and_management_grants_are_principal_bound_one_shot() {
    let temp = tempfile::tempdir().unwrap();
    let generations = Arc::new(DatasetGenerationStore::open_exclusive(temp.path()).unwrap());
    let mut runtime = BaseRuntime::open(generations.clone(), runtime_config()).unwrap();
    assert!(BaseRuntime::open(generations, runtime_config()).is_err());

    let services = runtime.services_for_principal([9; 32]).unwrap();
    let grant = runtime
        .issue_management_grant(
            [9; 32],
            b"authenticated-local-host",
            [BaseManagementScope::ArchiveSource],
            Duration::from_secs(60),
        )
        .unwrap();
    let management = services.management(grant).unwrap();
    let receipt = management.close().await.unwrap();
    assert_eq!(receipt.revoked_capabilities, 0);

    let other = runtime.services_for_principal([7; 32]).unwrap();
    let grant = runtime
        .issue_management_grant(
            [9; 32],
            b"authenticated-local-host",
            [BaseManagementScope::ArchiveSource],
            Duration::from_secs(60),
        )
        .unwrap();
    assert!(other.management(grant).is_err());
    runtime.close().await.unwrap();
}

#[tokio::test]
async fn management_archive_capabilities_bind_one_reserved_operation_and_revoke_on_close() {
    let temp = tempfile::tempdir().unwrap();
    let generations = Arc::new(DatasetGenerationStore::open_exclusive(temp.path()).unwrap());
    let mut runtime = BaseRuntime::open(generations, runtime_config()).unwrap();
    let services = runtime.services_for_principal([9; 32]).unwrap();
    let reservation = match services
        .invoke(BaseRequestV1::ReserveOperation(
            BaseOperationKindV1::RestoreArchive,
        ))
        .await
        .unwrap()
    {
        BaseResponseV1::Reserved(value) => value,
        _ => panic!("unexpected reservation response"),
    };
    let grant = runtime
        .issue_management_grant(
            [9; 32],
            b"authenticated-local-host",
            [
                BaseManagementScope::ArchiveSource,
                BaseManagementScope::ArchiveSecret,
            ],
            Duration::from_secs(60),
        )
        .unwrap();
    let management = services.management(grant).unwrap();
    let source = match management
        .invoke(BaseManagementRequestV1::ArchiveSourceBegin(
            ArchiveSourceBeginV1 {
                reservation_id: reservation,
                declared_total_bytes: 3,
            },
        ))
        .await
        .unwrap()
    {
        BaseManagementResponseV1::ArchiveSource(value) => *value.as_bytes(),
        _ => panic!("unexpected source response"),
    };
    management
        .invoke(BaseManagementRequestV1::ArchiveSourcePush(
            ArchiveSourcePushV1 {
                handle: ArchiveSourceHandleV1::from_opaque_bytes(source),
                offset: 0,
                chunk: ArchiveChunkV1::try_from_bytes(vec![1, 2, 3]).unwrap(),
            },
        ))
        .await
        .unwrap();
    management
        .invoke(BaseManagementRequestV1::ArchiveSourceSeal(
            ArchiveCapabilityHandleV1::from_opaque_bytes(source),
        ))
        .await
        .unwrap();
    let secret = management
        .invoke(BaseManagementRequestV1::ArchiveSecretRegister(
            BoundedSecretIngressV1::try_new(ArchiveCredentialKindV1::Password, b"secret".to_vec())
                .unwrap(),
        ))
        .await
        .unwrap();
    assert!(matches!(secret, BaseManagementResponseV1::ArchiveSecret(_)));

    let stale = management.clone();
    let close = management.close().await.unwrap();
    assert_eq!(close.revoked_capabilities, 2);
    assert!(stale
        .invoke(BaseManagementRequestV1::ArchiveCapabilityDestroy(
            ArchiveCapabilityHandleV1::from_opaque_bytes(source),
        ))
        .await
        .is_err());
    runtime.close().await.unwrap();
}

#[tokio::test]
async fn signer_provision_handles_bind_typed_domain_public_id_and_generation() {
    let temp = tempfile::tempdir().unwrap();
    let generations = Arc::new(DatasetGenerationStore::open_exclusive(temp.path()).unwrap());
    let generation = generations.current_generation();
    let mut runtime = BaseRuntime::open(generations, runtime_config()).unwrap();
    let provider_id = SignerProviderId::new("facade-fixture-provider").unwrap();
    let expected = ExpectedSignerIdentity::ActorRoot(ActorRootIdentity {
        public_key: ActorRootPublicKey::from_bytes([7; 32]),
    });
    let requirement = onebrain_node::SignerReprovisionRequirement {
        expected,
        provider_id: provider_id.clone(),
        disabled_capabilities: SignerCapabilitySet::for_domain(IdentityDomain::ActorRoot),
    };
    let proof = SignerPossessionProof::new(
        provider_id,
        SignerPossessionChallengeV1 {
            domain: IdentityDomain::ActorRoot,
            expected_identity_digest: expected.digest(),
            dataset_generation: generation,
            verifier_nonce: [3; 32],
        },
        [0; 64],
    );
    let provision = runtime
        .register_signer_provision(
            [9; 32],
            b"authenticated-local-host",
            requirement,
            proof,
            Arc::new(EmptySignerRegistry),
        )
        .unwrap();
    let provision_id = *provision.as_bytes();
    let grant = runtime
        .issue_management_grant(
            [9; 32],
            b"authenticated-local-host",
            [BaseManagementScope::SignerReprovision],
            Duration::from_secs(60),
        )
        .unwrap();
    let management = runtime
        .services_for_principal([9; 32])
        .unwrap()
        .management(grant)
        .unwrap();

    let mismatch = management
        .invoke(BaseManagementRequestV1::CompleteSignerReprovision(
            CompleteSignerReprovisionV1 {
                domain: SignerDomainV1::ActorRoot,
                expected_public_id: SignerPublicIdV1::ActorRoot(ActorRootPublicIdV1([8; 32])),
                provision_handle: SignerProvisionHandleV1::from_opaque_bytes(provision_id),
            },
        ))
        .await
        .err()
        .unwrap();
    assert_eq!(
        mismatch.code,
        onebrain_base_contract::BaseErrorCodeV1::Conflict
    );

    let unavailable = management
        .invoke(BaseManagementRequestV1::CompleteSignerReprovision(
            CompleteSignerReprovisionV1 {
                domain: SignerDomainV1::ActorRoot,
                expected_public_id: SignerPublicIdV1::ActorRoot(ActorRootPublicIdV1([7; 32])),
                provision_handle: SignerProvisionHandleV1::from_opaque_bytes(provision_id),
            },
        ))
        .await
        .err()
        .unwrap();
    assert_eq!(
        unavailable.code,
        onebrain_base_contract::BaseErrorCodeV1::ReprovisionRequired
    );
    management.close().await.unwrap();
    runtime.close().await.unwrap();
}

#[tokio::test]
async fn resource_budget_remains_finitely_bounded_by_the_generated_contract() {
    assert!(ResourceBudgetV1::try_new(256, 1_048_576, 1_000_000).is_ok());
    assert!(ResourceBudgetV1::try_new(257, 1, 1).is_err());

    let temp = tempfile::tempdir().unwrap();
    let generations = Arc::new(DatasetGenerationStore::open_exclusive(temp.path()).unwrap());
    let mut runtime = BaseRuntime::open(generations, runtime_config()).unwrap();
    let error = runtime
        .services()
        .unwrap()
        .invoke(BaseRequestV1::Query(
            onebrain_base_contract::BaseQueryRequestV1 {
                payload: TypedPayloadV1::try_from_bytes(b"three".to_vec()).unwrap(),
                continuation: None,
                budget: ResourceBudgetV1::try_new(1, 2, 1).unwrap(),
            },
        ))
        .await
        .err()
        .unwrap();
    assert_eq!(
        error.code,
        onebrain_base_contract::BaseErrorCodeV1::ResourceExhausted
    );
    runtime.close().await.unwrap();
}

#[tokio::test]
async fn entropy_failure_and_process_generation_reuse_fail_before_admission() {
    let temp = tempfile::tempdir().unwrap();
    let generations = Arc::new(DatasetGenerationStore::open_exclusive(temp.path()).unwrap());
    let mut failing = runtime_config();
    failing.process_generation_source = Arc::new(FailingProcessGenerationSource);
    assert!(BaseRuntime::open(generations.clone(), failing).is_err());

    let mut partial_start = runtime_config();
    partial_start.archive_factory = Some(Arc::new(|_, _, _| {
        Err(NodeError::Config("injected archive factory failure".into()))
    }));
    assert!(BaseRuntime::open(generations.clone(), partial_start).is_err());
    let mut recovered = BaseRuntime::open(generations.clone(), runtime_config()).unwrap();
    recovered.close().await.unwrap();

    let fixed = ProcessGenerationId([0xA5; 32]);
    let mut first_config = runtime_config();
    first_config.process_generation_source = Arc::new(FixedProcessGenerationSource(fixed));
    let mut first = BaseRuntime::open(generations.clone(), first_config).unwrap();
    first.close().await.unwrap();

    let mut reused = runtime_config();
    reused.process_generation_source = Arc::new(FixedProcessGenerationSource(fixed));
    assert!(BaseRuntime::open(generations, reused).is_err());
}

#[test]
fn base_runtime_child_process() {
    let Ok(mode) = std::env::var("ONEBRAIN_BASE_RUNTIME_CHILD_MODE") else {
        return;
    };
    let root =
        std::path::PathBuf::from(std::env::var_os("ONEBRAIN_BASE_RUNTIME_CHILD_ROOT").unwrap());
    match mode.as_str() {
        "holder" => {
            let generations = Arc::new(DatasetGenerationStore::open_exclusive(&root).unwrap());
            let _runtime = BaseRuntime::open(generations, runtime_config()).unwrap();
            std::fs::write(root.with_extension("ready"), b"ready").unwrap();
            loop {
                thread::sleep(Duration::from_millis(20));
            }
        }
        "contender" => {
            assert!(DatasetGenerationStore::open_exclusive(&root).is_err());
        }
        "operation_failpoint" => {
            let generations = Arc::new(DatasetGenerationStore::open_exclusive(&root).unwrap());
            let runtime = BaseRuntime::open(generations, runtime_config()).unwrap();
            let services = runtime.services().unwrap();
            let executor = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let _ = executor.block_on(services.invoke(BaseRequestV1::ReserveOperation(
                BaseOperationKindV1::ExistingLocalCommand,
            )));
            std::process::abort();
        }
        _ => panic!("unknown Base runtime child mode"),
    }
}

#[tokio::test]
async fn child_process_root_race_and_crash_reopen_preserve_exclusive_generation_fencing() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("base-root");
    let ready = root.with_extension("ready");
    let executable = std::env::current_exe().unwrap();
    let spawn = |mode: &str| {
        Command::new(&executable)
            .arg("--exact")
            .arg("base_runtime_child_process")
            .arg("--nocapture")
            .env("ONEBRAIN_BASE_RUNTIME_CHILD_MODE", mode)
            .env("ONEBRAIN_BASE_RUNTIME_CHILD_ROOT", &root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    };
    let mut holder = spawn("holder");
    for _ in 0..500 {
        if ready.exists() {
            break;
        }
        assert!(
            holder.try_wait().unwrap().is_none(),
            "holder exited before ready"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert!(ready.exists(), "holder did not acquire the root lease");
    let contender = spawn("contender").wait_with_output().unwrap();
    assert!(contender.status.success());

    holder.kill().unwrap();
    holder.wait().unwrap();
    let generations = Arc::new(DatasetGenerationStore::open_exclusive(&root).unwrap());
    let mut reopened = BaseRuntime::open(generations, runtime_config()).unwrap();
    let status = reopened.services().unwrap().snapshot().unwrap();
    assert_ne!(status.process_generation.0, [0; 32]);
    reopened.close().await.unwrap();
}

#[tokio::test]
async fn child_process_five_phase_operation_crashes_reopen_without_partial_or_replay() {
    for (phase, expected_records) in [
        ("before_begin_write", 0),
        ("after_begin_write_before_mutation", 0),
        ("after_mutation_before_commit", 0),
        ("after_commit_before_next_side_effect", 1),
        ("after_next_side_effect_before_ack", 1),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let status = Command::new(std::env::current_exe().unwrap())
            .arg("base_runtime_child_process")
            .arg("--exact")
            .arg("--nocapture")
            .env("ONEBRAIN_BASE_RUNTIME_CHILD_MODE", "operation_failpoint")
            .env("ONEBRAIN_BASE_RUNTIME_CHILD_ROOT", temp.path())
            .env("ONEBRAIN_BASE_OPS_FAILPOINT", phase)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(!status.success(), "phase {phase} did not crash the child");

        let generations = Arc::new(DatasetGenerationStore::open_exclusive(temp.path()).unwrap());
        let mut runtime = BaseRuntime::open(generations, runtime_config()).unwrap();
        let operation_ids = operation_record_ids(temp.path());
        assert_eq!(operation_ids.len(), expected_records, "phase {phase}");
        if let Some(operation_id) = operation_ids.first() {
            let reconciled = runtime
                .services()
                .unwrap()
                .invoke(BaseRequestV1::Reconcile(BaseOperationId(*operation_id)))
                .await
                .unwrap();
            let BaseResponseV1::Reconciled(reconciled) = reconciled else {
                panic!("phase {phase} did not return reconciliation");
            };
            assert_eq!(
                reconciled.receipt.state,
                onebrain_node::BaseOperationStateV1::UnknownOutcome
            );
            assert!(!reconciled.resumed_effect);
        }
        runtime.close().await.unwrap();
    }
}

fn operation_record_ids(root: &std::path::Path) -> Vec<[u8; 32]> {
    fn visit(path: &std::path::Path, output: &mut Vec<[u8; 32]>) {
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        if path.file_name().and_then(|name| name.to_str()) == Some("records") {
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    if let Some(decoded) =
                        entry.file_name().to_str().and_then(decode_hex_operation_id)
                    {
                        output.push(decoded);
                    }
                }
            }
            return;
        }
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                visit(&entry.path(), output);
            }
        }
    }

    let mut output = Vec::new();
    visit(root, &mut output);
    output.sort();
    output
}

fn decode_hex_operation_id(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 {
        return None;
    }
    let mut output = [0; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(output)
}
