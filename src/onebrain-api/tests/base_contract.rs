use axum::body::Body;
use axum::http::{Request, StatusCode};
use onebrain_api::ApiServer;
use onebrain_archive::{
    ArchiveEntryKind, ArchiveLimits, ArchiveOwner, PortableDataCompatibilityV1,
    PortableProfileVersion,
};
use onebrain_base as abi;
use onebrain_base_contract::{BaseOperationKindV1, BaseRequestV1};
use onebrain_node::archive::{
    ArchiveSnapshotRecord, BaseArchiveService, LogicalRowsArchiveBackend, SnapshotVerifiedBackend,
    StagedArchiveBackendFactory,
};
use onebrain_node::{
    ActorRootIdentity, ActorRootPublicKey, BaseLocalOperationAdapter, BaseOperationStore,
    BaseServiceError, DatasetGenerationStore, DatasetPathResolver, ExpectedSignerIdentity,
    NodeConfig, NodeError, OneBrainNode, ProcessGenerationId, SignerProviderId,
    SignerRecoveryPolicy,
};
use serde_json::json;
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

async fn server() -> ApiServer {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.keep();
    let config = NodeConfig {
        data_dir: root,
        concept_registry_mode: onebrain_node::ConceptRegistryMode::Disabled,
        ..NodeConfig::default()
    };
    let node = OneBrainNode::new(config).await.unwrap();
    ApiServer::new(node, "base-test-token".into(), 0)
}

#[derive(Clone)]
struct ConformanceMetadataBackend {
    rows: Arc<Mutex<Vec<ArchiveSnapshotRecord>>>,
}

struct EchoLocalAdapter;

impl BaseLocalOperationAdapter for EchoLocalAdapter {
    fn query(
        &self,
        request: onebrain_base_contract::BaseQueryRequestV1,
    ) -> Result<
        (
            onebrain_base_contract::TypedPayloadV1,
            Option<onebrain_base_contract::BaseOpaqueContinuation>,
        ),
        BaseServiceError,
    > {
        Ok((request.payload, None))
    }

    fn confirm_local(
        &self,
        command: onebrain_base_contract::BaseLocalCommandV1,
    ) -> Result<Vec<u8>, BaseServiceError> {
        Ok(command.payload.as_bytes().to_vec())
    }
}

impl ConformanceMetadataBackend {
    fn source() -> Self {
        let signer_policy = SignerRecoveryPolicy::ReprovisionRequired {
            expected: ExpectedSignerIdentity::ActorRoot(ActorRootIdentity {
                public_key: ActorRootPublicKey::from_bytes([0x51; 32]),
            }),
            provider_id: SignerProviderId::new("projection-conformance-unavailable").unwrap(),
        }
        .encode()
        .unwrap();
        let rows = [
            (
                ArchiveEntryKind::AuthorityHighWater,
                ArchiveOwner::CANONICAL,
                b"authority".as_slice(),
                b"0".as_slice(),
            ),
            (
                ArchiveEntryKind::MigrationState,
                ArchiveOwner::MIGRATION,
                b"migration".as_slice(),
                b"complete".as_slice(),
            ),
            (
                ArchiveEntryKind::InterpretationConfig,
                ArchiveOwner::INTERPRETATION_CONFIG,
                b"configuration".as_slice(),
                b"base-v1".as_slice(),
            ),
            (
                ArchiveEntryKind::RegistryHighWater,
                ArchiveOwner::REGISTRY_METADATA,
                b"registry".as_slice(),
                b"0".as_slice(),
            ),
            (
                ArchiveEntryKind::SignerRecoveryPolicy,
                ArchiveOwner::IDENTITY,
                b"signer".as_slice(),
                signer_policy.as_slice(),
            ),
        ]
        .into_iter()
        .map(|(kind, owner, key, bytes)| ArchiveSnapshotRecord {
            kind,
            owner,
            namespace: 1,
            key: key.to_vec(),
            bytes: bytes.to_vec(),
            required: true,
        })
        .collect();
        Self {
            rows: Arc::new(Mutex::new(rows)),
        }
    }

    fn target() -> Self {
        Self {
            rows: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl SnapshotVerifiedBackend for ConformanceMetadataBackend {
    fn owns(&self, owner: ArchiveOwner) -> bool {
        matches!(
            owner,
            ArchiveOwner::CANONICAL
                | ArchiveOwner::MIGRATION
                | ArchiveOwner::INTERPRETATION_CONFIG
                | ArchiveOwner::REGISTRY_METADATA
                | ArchiveOwner::IDENTITY
        )
    }

    fn bounded_snapshot(&self) -> Result<Vec<ArchiveSnapshotRecord>, NodeError> {
        self.rows
            .lock()
            .map_err(|_| NodeError::Storage("conformance metadata lock failed".into()))
            .map(|rows| rows.clone())
    }

    fn restore_validated(&self, record: &ArchiveSnapshotRecord) -> Result<(), NodeError> {
        if !self.owns(record.owner) || record.bytes.is_empty() {
            return Err(NodeError::ArchiveCapability(
                "invalid conformance metadata record".into(),
            ));
        }
        self.rows
            .lock()
            .map_err(|_| NodeError::Storage("conformance metadata lock failed".into()))?
            .push(record.clone());
        Ok(())
    }
}

struct ConformanceRestoreFactory {
    process_generation: ProcessGenerationId,
}

impl StagedArchiveBackendFactory for ConformanceRestoreFactory {
    fn open_for_staged_generation(
        &self,
        resolver: &dyn DatasetPathResolver,
    ) -> Result<Vec<Arc<dyn SnapshotVerifiedBackend>>, NodeError> {
        let operations = Arc::new(
            BaseOperationStore::open(resolver, self.process_generation)
                .map_err(|error| NodeError::Storage(error.to_string()))?,
        );
        Ok(vec![
            Arc::new(ConformanceMetadataBackend::target()),
            Arc::new(LogicalRowsArchiveBackend::new(operations)),
        ])
    }
}

async fn archive_enabled_server() -> ApiServer {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.keep();
    let config = NodeConfig {
        data_dir: root.clone(),
        concept_registry_mode: onebrain_node::ConceptRegistryMode::Disabled,
        ..NodeConfig::default()
    };
    let mut node = OneBrainNode::new(config).await.unwrap();
    let mut base = onebrain_api::base_runtime_config_for_api_token("base-test-token");
    base.local_adapter = Arc::new(EchoLocalAdapter);
    let tuple = base.compatibility_policy.current.clone();
    let portable = PortableDataCompatibilityV1 {
        canonical_schema_digest: tuple.canonical_schema_digest.0,
        domain_registry_digest: tuple.domain_registry_digest.0,
        resource_registry_digest: tuple.resource_registry_digest.0,
        storage_schema_version: tuple.storage_schema.0,
        archive_profile: PortableProfileVersion {
            major: tuple.archive_profile.major,
            minor: tuple.archive_profile.minor,
        },
        migration_profile: PortableProfileVersion {
            major: tuple.migration_profile.major,
            minor: tuple.migration_profile.minor,
        },
    };
    let spool = root.join("base-v1-conformance-spool");
    base.archive_factory = Some(Arc::new(
        move |capabilities,
              dataset_generations: Arc<DatasetGenerationStore>,
              operation_store: Arc<BaseOperationStore>| {
            let process_generation = operation_store.process_generation();
            BaseArchiveService::new(
                capabilities,
                dataset_generations,
                vec![
                    Arc::new(ConformanceMetadataBackend::source()),
                    Arc::new(LogicalRowsArchiveBackend::new(operation_store)),
                ],
                portable,
                ArchiveLimits::default(),
                &spool,
                None,
                Arc::new(Mutex::new(())),
            )
            .map(|service| {
                service.with_restore_backend_factory(Arc::new(ConformanceRestoreFactory {
                    process_generation,
                }))
            })
        },
    ));
    node.install_base_runtime(base).unwrap();
    ApiServer::new(node, "base-test-token".into(), 0)
}

fn abi_error() -> abi::ObBaseErrorV1 {
    abi::ObBaseErrorV1 {
        struct_size: std::mem::size_of::<abi::ObBaseErrorV1>() as u32,
        abi_major: 1,
        abi_minor: 0,
        code: 0,
        retryable: 0,
        reconcile_before_retry: 0,
        reserved: 0,
        message_ptr: std::ptr::null(),
        message_len: 0,
        allocation_tag: 0,
    }
}

fn abi_output(buffer: &mut [u8]) -> abi::ObBaseOutputV1 {
    abi::ObBaseOutputV1 {
        struct_size: std::mem::size_of::<abi::ObBaseOutputV1>() as u32,
        abi_major: 1,
        abi_minor: 0,
        process_generation: [0; 32],
        dataset_generation: [0; 32],
        response_discriminator: 0,
        reserved: 0,
        operation_id: [0; 32],
        buffer_ptr: if buffer.is_empty() {
            std::ptr::null_mut()
        } else {
            buffer.as_mut_ptr()
        },
        buffer_capacity: buffer.len(),
        required_len: 0,
        written_len: 0,
    }
}

fn abi_call(generation: &abi::ObBaseOutputV1) -> abi::ObBaseCallV1 {
    abi::ObBaseCallV1 {
        struct_size: std::mem::size_of::<abi::ObBaseCallV1>() as u32,
        abi_major: 1,
        abi_minor: 0,
        process_generation: generation.process_generation,
        dataset_generation: generation.dataset_generation,
        request_id: [1; 32],
        operation_id: [0; 32],
        auxiliary_id: [0; 32],
        discriminator: 0,
        flags: 0,
        value0: 1,
        value1: 1_048_576,
        payload_ptr: std::ptr::null(),
        payload_len: 0,
    }
}

fn release_abi_error(error: &mut abi::ObBaseErrorV1) {
    if error.allocation_tag == 0 {
        return;
    }
    let mut owned = abi::ObBaseOwnedBufferV1 {
        struct_size: std::mem::size_of::<abi::ObBaseOwnedBufferV1>() as u32,
        abi_major: 1,
        abi_minor: 0,
        ptr: error.message_ptr,
        len: error.message_len,
        allocation_tag: error.allocation_tag,
    };
    assert_eq!(unsafe { abi::ob_base_buffer_free_v1(&mut owned, error) }, 0);
}

fn request(path: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .header("authorization", "Bearer base-test-token")
        .body(Body::empty())
        .unwrap()
}

fn post_base(body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/base/v1/operations")
        .header("authorization", "Bearer base-test-token")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

async fn response_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn post_base_data(router: &axum::Router, body: serde_json::Value) -> serde_json::Value {
    let response = router.clone().oneshot(post_base(body)).await.unwrap();
    let status = response.status();
    let body = response_json(response).await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    body["data"].clone()
}

#[tokio::test]
async fn base_routes_are_authenticated_and_never_imply_network_activation() {
    let router = server().await.build_router();
    let unauthenticated = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/base/v1/capabilities")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let response = router
        .clone()
        .oneshot(request("/api/base/v1/capabilities"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["data"]["base_v1"], true);
    assert_eq!(body["data"]["network_requested"], false);
    assert_eq!(body["data"]["network_active"], false);

    let status = router
        .clone()
        .oneshot(request("/api/base/v1/status"))
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::OK);

    let reserve = Request::builder()
        .method("POST")
        .uri("/api/base/v1/operations")
        .header("authorization", "Bearer base-test-token")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"operation":"reserve","kind":1}"#))
        .unwrap();
    let response = router.oneshot(reserve).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["data"]["state"], "reserved");
    assert_eq!(body["data"]["operation_id"].as_str().unwrap().len(), 64);
}

#[tokio::test]
async fn compatibility_backup_route_never_falls_back_to_the_legacy_backend() {
    let router = server().await.build_router();
    let request = Request::builder()
        .method("POST")
        .uri("/api/backup")
        .header("authorization", "Bearer base-test-token")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({"password":"projection-password"})).unwrap(),
        ))
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let body = response_json(response).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "base_v1_error_6");
}

#[tokio::test]
async fn compatibility_backup_and_restore_routes_use_the_scoped_base_facade() {
    let router = archive_enabled_server().await.build_router();
    let backup = Request::builder()
        .method("POST")
        .uri("/api/backup")
        .header("authorization", "Bearer base-test-token")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({"password":"projection-password"})).unwrap(),
        ))
        .unwrap();
    let response = router.clone().oneshot(backup).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"],
        "application/vnd.onebrain.obar-v2"
    );
    let archive = axum::body::to_bytes(response.into_body(), 16 * 1024 * 1024)
        .await
        .unwrap();
    assert!(archive.starts_with(b"OBARV002"));

    let boundary = "onebrain-base-v1-boundary";
    let mut multipart = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"password\"\r\n\r\nprojection-password\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"archive.obar\"\r\nContent-Type: application/octet-stream\r\n\r\n"
    )
    .into_bytes();
    multipart.extend_from_slice(&archive);
    multipart.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let restore = Request::builder()
        .method("POST")
        .uri("/api/restore")
        .header("authorization", "Bearer base-test-token")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(multipart))
        .unwrap();
    let response = router.oneshot(restore).await.unwrap();
    let status = response.status();
    let body = response_json(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["restored"], true);
    assert_eq!(body["data"]["reconciled"], true);
    assert_eq!(body["data"]["operation_id"].as_str().unwrap().len(), 64);
}

#[tokio::test]
async fn scoped_management_capabilities_follow_the_authenticated_rest_lifecycle() {
    let router = server().await.build_router();

    let reservation = router
        .clone()
        .oneshot(post_base(json!({"operation":"reserve","kind":3})))
        .await
        .unwrap();
    assert_eq!(reservation.status(), StatusCode::OK);
    let reservation = response_json(reservation).await["data"]["operation_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let opened = router
        .clone()
        .oneshot(post_base(json!({
            "operation":"management_open",
            "scopes":["archive_source","archive_secret"]
        })))
        .await
        .unwrap();
    assert_eq!(opened.status(), StatusCode::OK);
    let management = response_json(opened).await["data"]["management_handle"]
        .as_str()
        .unwrap()
        .to_owned();

    let begun = router
        .clone()
        .oneshot(post_base(json!({
            "operation":"archive_source_begin",
            "management_handle":management,
            "reservation_id":reservation,
            "declared_total_bytes":3
        })))
        .await
        .unwrap();
    assert_eq!(begun.status(), StatusCode::OK);
    let source = response_json(begun).await["data"]["archive_source_handle"]
        .as_str()
        .unwrap()
        .to_owned();

    let secret = router
        .clone()
        .oneshot(post_base(json!({
            "operation":"archive_secret_register",
            "management_handle":management,
            "credential_kind":1,
            "payload":"bounded-password"
        })))
        .await
        .unwrap();
    assert_eq!(secret.status(), StatusCode::OK);

    let pushed = router
        .clone()
        .oneshot(post_base(json!({
            "operation":"archive_source_push",
            "management_handle":management,
            "capability_id":source,
            "offset":0,
            "chunk_hex":"616263"
        })))
        .await
        .unwrap();
    assert_eq!(pushed.status(), StatusCode::OK);

    let sealed = router
        .clone()
        .oneshot(post_base(json!({
            "operation":"archive_source_seal",
            "management_handle":management,
            "capability_id":source
        })))
        .await
        .unwrap();
    assert_eq!(sealed.status(), StatusCode::OK);

    let closed = router
        .clone()
        .oneshot(post_base(json!({
            "operation":"management_close",
            "management_handle":management
        })))
        .await
        .unwrap();
    assert_eq!(closed.status(), StatusCode::OK);
    assert!(
        response_json(closed).await["data"]["revoked_capabilities"]
            .as_u64()
            .unwrap()
            >= 2
    );

    let reused = router
        .oneshot(post_base(json!({
            "operation":"management_close",
            "management_handle":management
        })))
        .await
        .unwrap();
    assert_eq!(reused.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn authenticated_rest_archive_roundtrip_is_lossless_and_reconciles_after_activation() {
    let router = archive_enabled_server().await.build_router();
    let before = post_base_data(&router, json!({ "operation": "status" })).await;
    assert!(before["compatibility"]["canonical_schema_digest"].is_string());

    let binary = post_base_data(
        &router,
        json!({
            "operation":"query",
            "payload_hex":"ff00fe",
            "max_items":1,
            "max_bytes":3,
            "max_work_units":3
        }),
    )
    .await;
    assert_eq!(binary["payload_hex"], "ff00fe");

    let create_reservation = post_base_data(&router, json!({ "operation":"reserve", "kind":2 }))
        .await["operation_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let create_management = post_base_data(
        &router,
        json!({
            "operation":"management_open",
            "scopes":["archive_sink", "archive_secret"]
        }),
    )
    .await["management_handle"]
        .as_str()
        .unwrap()
        .to_owned();
    let writable_sink = post_base_data(
        &router,
        json!({
            "operation":"archive_sink_begin",
            "management_handle":create_management,
            "reservation_id":create_reservation,
            "declared_total_bytes":16 * 1024 * 1024
        }),
    )
    .await["archive_sink_handle"]
        .as_str()
        .unwrap()
        .to_owned();
    let create_secret = post_base_data(
        &router,
        json!({
            "operation":"archive_secret_register",
            "management_handle":create_management,
            "credential_kind":1,
            "payload_hex":"70726f6a656374696f6e2d70617373776f7264"
        }),
    )
    .await["archive_secret_handle"]
        .as_str()
        .unwrap()
        .to_owned();
    let create_operation = post_base_data(
        &router,
        json!({
            "operation":"prepare",
            "kind":2,
            "reservation_id":create_reservation,
            "auxiliary_id":writable_sink,
            "operation_id":create_secret
        }),
    )
    .await["operation_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let created = post_base_data(
        &router,
        json!({
            "operation":"confirm",
            "operation_id":create_operation,
            "idempotency_key":"6161616161616161616161616161616161616161616161616161616161616161"
        }),
    )
    .await;
    assert!(created["error"].is_null(), "{created}");
    let create_result = created["result_hex"].as_str().unwrap();
    assert_eq!(create_result.len(), 128);
    let readable_sink = create_result[..64].to_owned();

    let mut archive = Vec::new();
    let mut offset = 0u64;
    loop {
        let chunk = post_base_data(
            &router,
            json!({
                "operation":"archive_sink_read",
                "management_handle":create_management,
                "capability_id":readable_sink,
                "offset":offset,
                "max_items":65536
            }),
        )
        .await;
        assert_eq!(chunk["offset"].as_u64().unwrap(), offset);
        let encoded = chunk["chunk_hex"].as_str().unwrap().as_bytes();
        assert_eq!(encoded.len() & 1, 0);
        for pair in encoded.chunks_exact(2) {
            archive.push(u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap());
        }
        offset = archive.len() as u64;
        if chunk["eof"].as_bool().unwrap() {
            break;
        }
    }
    assert!(archive.starts_with(b"OBARV002"));
    post_base_data(
        &router,
        json!({
            "operation":"archive_sink_commit",
            "management_handle":create_management,
            "capability_id":readable_sink
        }),
    )
    .await;
    post_base_data(
        &router,
        json!({
            "operation":"management_close",
            "management_handle":create_management
        }),
    )
    .await;

    let restore_reservation = post_base_data(&router, json!({ "operation":"reserve", "kind":3 }))
        .await["operation_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let restore_management = post_base_data(
        &router,
        json!({
            "operation":"management_open",
            "scopes":["archive_source", "archive_secret"]
        }),
    )
    .await["management_handle"]
        .as_str()
        .unwrap()
        .to_owned();
    let source = post_base_data(
        &router,
        json!({
            "operation":"archive_source_begin",
            "management_handle":restore_management,
            "reservation_id":restore_reservation,
            "declared_total_bytes":archive.len()
        }),
    )
    .await["archive_source_handle"]
        .as_str()
        .unwrap()
        .to_owned();
    for (index, chunk) in archive.chunks(512 * 1024).enumerate() {
        let encoded = chunk
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        post_base_data(
            &router,
            json!({
                "operation":"archive_source_push",
                "management_handle":restore_management,
                "capability_id":source,
                "offset":index * 512 * 1024,
                "chunk_hex":encoded
            }),
        )
        .await;
    }
    post_base_data(
        &router,
        json!({
            "operation":"archive_source_seal",
            "management_handle":restore_management,
            "capability_id":source
        }),
    )
    .await;
    let restore_secret = post_base_data(
        &router,
        json!({
            "operation":"archive_secret_register",
            "management_handle":restore_management,
            "credential_kind":1,
            "payload_hex":"70726f6a656374696f6e2d70617373776f7264"
        }),
    )
    .await["archive_secret_handle"]
        .as_str()
        .unwrap()
        .to_owned();
    let restore_operation = post_base_data(
        &router,
        json!({
            "operation":"prepare",
            "kind":3,
            "reservation_id":restore_reservation,
            "auxiliary_id":source,
            "operation_id":restore_secret
        }),
    )
    .await["operation_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let restored = post_base_data(
        &router,
        json!({
            "operation":"confirm",
            "operation_id":restore_operation,
            "idempotency_key":"6262626262626262626262626262626262626262626262626262626262626262"
        }),
    )
    .await;
    assert!(restored["error"].is_null(), "{restored}");
    // DatasetRestoreReceiptV1 is the composite activation receipt:
    // operation + old/new roots + sequence + terminal phase (105 bytes).
    assert_eq!(restored["result_hex"].as_str().unwrap().len(), 210);

    let reconciled = post_base_data(
        &router,
        json!({
            "operation":"reconcile",
            "operation_id":restore_operation
        }),
    )
    .await;
    assert_eq!(reconciled["reconcile_required"], false);
    assert_eq!(reconciled["resumed_effect"], false);
    let after = post_base_data(&router, json!({ "operation": "status" })).await;
    assert_ne!(before["dataset_generation"], after["dataset_generation"]);
}

#[tokio::test]
async fn management_scope_is_enforced_before_archive_capability_ingress() {
    let router = server().await.build_router();
    let reservation = router
        .clone()
        .oneshot(post_base(json!({"operation":"reserve","kind":3})))
        .await
        .unwrap();
    let reservation = response_json(reservation).await["data"]["operation_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let opened = router
        .clone()
        .oneshot(post_base(json!({
            "operation":"management_open",
            "scopes":["archive_sink"]
        })))
        .await
        .unwrap();
    let management = response_json(opened).await["data"]["management_handle"]
        .as_str()
        .unwrap()
        .to_owned();
    let response = router
        .oneshot(post_base(json!({
            "operation":"archive_source_begin",
            "management_handle":management,
            "reservation_id":reservation,
            "declared_total_bytes":3
        })))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = response_json(response).await;
    assert_eq!(body["error"]["code"], "base_v1_error_6");
}

#[tokio::test]
async fn rust_axum_and_c_share_one_status_and_reservation_semantics() {
    let temp = tempfile::tempdir().unwrap();
    let config = NodeConfig {
        data_dir: temp.keep(),
        concept_registry_mode: onebrain_node::ConceptRegistryMode::Disabled,
        ..NodeConfig::default()
    };
    let mut node = OneBrainNode::new(config).await.unwrap();
    node.install_base_runtime(onebrain_api::base_runtime_config_for_api_token(
        "base-test-token",
    ))
    .unwrap();
    let rust = node.base_services().unwrap();
    let rust_status = rust.snapshot().unwrap();

    let registration =
        abi::register_base_services_for_abi(rust.clone(), b"cross-projection-host").unwrap();
    let open = abi::ObBaseOpenRequestV1 {
        struct_size: std::mem::size_of::<abi::ObBaseOpenRequestV1>() as u32,
        abi_major: 1,
        abi_minor: 0,
        registration_token: registration.token,
        host_trust_digest: registration.host_trust_digest,
    };
    let mut c_handle = std::ptr::null_mut();
    let mut empty = [];
    let mut c_generation = abi_output(&mut empty);
    let mut c_error = abi_error();
    assert_eq!(
        unsafe { abi::ob_base_open_v1(&open, &mut c_handle, &mut c_generation, &mut c_error,) },
        0
    );

    let router = ApiServer::new(node, "base-test-token".into(), 0).build_router();
    let axum_status = router
        .clone()
        .oneshot(request("/api/base/v1/status"))
        .await
        .unwrap();
    assert_eq!(axum_status.status(), StatusCode::OK);
    let axum_status = response_json(axum_status).await;

    let c_call = abi_call(&c_generation);
    let mut c_sizing = abi_output(&mut empty);
    assert_eq!(
        unsafe { abi::ob_base_status_v1(c_handle, &c_call, &mut c_sizing, &mut c_error) },
        9
    );
    release_abi_error(&mut c_error);
    let mut c_status_bytes = vec![0; c_sizing.required_len];
    let mut c_status_output = abi_output(&mut c_status_bytes);
    assert_eq!(
        unsafe { abi::ob_base_status_v1(c_handle, &c_call, &mut c_status_output, &mut c_error) },
        0
    );
    let c_status: serde_json::Value =
        serde_json::from_slice(&c_status_bytes[..c_status_output.written_len]).unwrap();

    let generation_hex = rust_status
        .process_generation
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(axum_status["data"]["process_generation"], generation_hex);
    assert_eq!(c_status["process_generation"], generation_hex);
    assert_eq!(
        axum_status["data"]["candidate_semantic_digest"],
        c_status["candidate_semantic_digest"]
    );
    assert_eq!(
        axum_status["data"]["artifact_tuple_digest"],
        c_status["artifact_tuple_digest"]
    );
    assert_eq!(
        axum_status["data"]["compatibility"],
        c_status["compatibility"]
    );
    assert_eq!(axum_status["data"]["network_active"], false);
    assert_eq!(c_status["network_active"], false);

    let rust_reserved = rust
        .invoke(BaseRequestV1::ReserveOperation(
            BaseOperationKindV1::ExistingLocalCommand,
        ))
        .await
        .unwrap();
    assert!(matches!(
        rust_reserved,
        onebrain_node::BaseResponseV1::Reserved(_)
    ));
    let axum_reserved = router
        .clone()
        .oneshot(post_base(json!({"operation":"reserve","kind":1})))
        .await
        .unwrap();
    assert_eq!(axum_reserved.status(), StatusCode::OK);
    assert_eq!(
        response_json(axum_reserved).await["data"]["state"],
        "reserved"
    );
    let mut c_reserve = abi_call(&c_generation);
    c_reserve.discriminator = 1;
    let mut c_reserve_output = abi_output(&mut empty);
    assert_eq!(
        unsafe {
            abi::ob_base_reserve_operation_v1(
                c_handle,
                &c_reserve,
                &mut c_reserve_output,
                &mut c_error,
            )
        },
        0
    );
    assert_ne!(c_reserve_output.operation_id, [0; 32]);

    let mut close_bytes = vec![0; 256];
    let mut close_output = abi_output(&mut close_bytes);
    assert_eq!(
        unsafe { abi::ob_base_close_v1(c_handle, &c_call, &mut close_output, &mut c_error) },
        0
    );
}

#[cfg(not(feature = "legacy-read-compat"))]
#[tokio::test]
async fn legacy_runtime_enable_is_rejected_when_code_is_compiled_out() {
    let server = server().await;
    #[cfg(not(feature = "legacy-read-compat"))]
    assert!(server.with_legacy_read_compat(true).is_err());
}

#[cfg(feature = "legacy-read-compat")]
#[tokio::test]
async fn legacy_route_exists_only_when_both_compiled_and_runtime_enabled() {
    let disabled = server().await.build_router();
    let response = disabled
        .oneshot(request("/api/base/v1/legacy/status"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let enabled = server()
        .await
        .with_legacy_read_compat(true)
        .unwrap()
        .build_router();
    let response = enabled
        .oneshot(request("/api/base/v1/legacy/status"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["data"]["writes"], "capability_disabled");
    assert_eq!(body["data"]["automatic_fallback"], false);
}
