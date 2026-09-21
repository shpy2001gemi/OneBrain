use super::*;
use crate::{base_runtime_config_for_api_token, ApiServer};
use axum::body::Body;
use axum::http::{Method, Request as HttpRequest};
use ku_core::foundation::semantic::{
    LiteralValue, SemanticFrameSet, SourceSpan, StatementFrame, StatementId, StatementQualifiers,
    TermRef,
};
use ku_core::foundation::{
    NormalizedText, ObjectReference, ObservationGovernance, ResourceProfile, SourceArtifact,
    SourceArtifactKind, VaultKey,
};
use onebrain_base_contract::ku_payload::KuPayload;
use onebrain_node::concept_registry_runtime::ConceptRegistryReaderLease;
use onebrain_node::ku_product::{
    KuConceptBinding, KuInputProvider, KuResolvedInput, KuRuntimeConfig,
};
use onebrain_node::{NodeConfig, OneBrainNode};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use tower::ServiceExt;

const TOKEN: &str = "ku-api-test-token";

struct Inputs {
    source: Vec<u8>,
    cid: SourceArtifactCID,
    calls: AtomicUsize,
    revoked: AtomicBool,
    unresolved: AtomicBool,
    wait: AtomicBool,
    entered: tokio::sync::Notify,
    resume: tokio::sync::Notify,
}
impl Inputs {
    fn new() -> Self {
        let reference = |n| ObjectReference::new(1, [n; 32]);
        let source = SourceArtifact {
            source_kind: SourceArtifactKind::Text,
            raw_bytes: b"Exact PRIVATE water source".to_vec(),
            media_type_commitment: [2; 32],
            capture_adapter: reference(1),
            capture_sequence: 1,
            governance: ObservationGovernance {
                consent_policy: reference(2),
                consent_receipt: reference(3),
                revocation_policy: reference(4),
                retention_policy: reference(5),
                capture_scope_commitment: [3; 32],
                authorization_assessment_commitment: [4; 32],
                assessed_frontier: [5; 32],
            },
        };
        let (source, cid) = source
            .to_private_object()
            .unwrap()
            .encode(ResourceProfile::ObjectV1)
            .unwrap();
        Self {
            source,
            cid: SourceArtifactCID(cid.into_bytes()),
            calls: AtomicUsize::new(0),
            revoked: AtomicBool::new(false),
            unresolved: AtomicBool::new(false),
            wait: AtomicBool::new(false),
            entered: tokio::sync::Notify::new(),
            resume: tokio::sync::Notify::new(),
        }
    }
}
impl KuInputProvider for Inputs {
    fn implementation(&self, mode: InputMode) -> Option<[u8; 32]> {
        // Advertising a fixture AI implementation must not enable the REST AI lane.
        Some(match mode {
            InputMode::ResolvedSemanticDraft => [4; 32],
            InputMode::LocalRule => [5; 32],
            InputMode::LocalAi => [6; 32],
        })
    }
    fn check_access(
        &self,
        principal: [u8; 32],
        sources: &[[u8; 32]],
    ) -> Result<(), BaseServiceError> {
        if principal == [0; 32] && sources == [self.cid.0] && !self.revoked.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(BaseServiceError::new(
                BaseErrorCodeV1::NotFound,
                "ku_not_found",
            ))
        }
    }
    fn resolve(
        &self,
        principal: [u8; 32],
        request: &KuPrepareV1,
        _: &ConceptRegistryReaderLease,
        _: &ResourceBudgetV1,
    ) -> Result<KuResolvedInput, BaseServiceError> {
        self.check_access(
            principal,
            &request.source_refs.iter().map(|s| s.0).collect::<Vec<_>>(),
        )?;
        self.calls.fetch_add(1, Ordering::SeqCst);
        let version = request.draft_ref.map(|id| id.0[0]).unwrap_or(1);
        let drafts = (0..2)
            .map(|i| SemanticFrameSet {
                statements: vec![StatementFrame {
                    statement_id: StatementId(90 + i),
                    operator_or_predicate: ku_core::foundation::ConceptCcid::from_bytes([7; 16]),
                    arguments: vec![TermRef::Literal(LiteralValue::Text(
                        NormalizedText::new(format!("water version {version} item {i}")).unwrap(),
                    ))],
                    constraints: vec![],
                    qualifiers: StatementQualifiers {
                        source_spans: vec![SourceSpan {
                            source: ObjectReference::new(1, self.cid.0),
                            start: 0,
                            end: 5,
                        }],
                        ..Default::default()
                    },
                }],
            })
            .collect();
        Ok(KuResolvedInput {
            drafts,
            source_objects: vec![self.source.clone()],
            bindings: vec![KuConceptBinding {
                label: "water".into(),
                selected: if self.unresolved.load(Ordering::SeqCst) {
                    None
                } else {
                    Some([7; 16])
                },
            }],
            needs_resolution: false,
            extraction_budget: None,
        })
    }
    fn resolve_async<'a>(
        &'a self,
        principal: [u8; 32],
        request: &'a KuPrepareV1,
        registry: &'a ConceptRegistryReaderLease,
        budget: &'a ResourceBudgetV1,
        _: onebrain_node::ku_product::KuExtractionExecution<'a>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<KuResolvedInput, BaseServiceError>> + Send + 'a,
        >,
    > {
        Box::pin(async move {
            if self.wait.load(Ordering::SeqCst) {
                self.entered.notify_one();
                self.resume.notified().await;
            }
            self.resolve(principal, request, registry, budget)
        })
    }
}

struct Fixture {
    _dir: tempfile::TempDir,
    state: AppState,
    router: axum::Router,
    inputs: Arc<Inputs>,
    root: String,
}
impl Fixture {
    async fn new() -> Self {
        Self::configured(false).await
    }
    async fn configured(manual: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let registry = registry(dir.path());
        let root = registry
            .reader_lease()
            .status()
            .release_aggregate_root
            .clone()
            .unwrap();
        let config = NodeConfig {
            data_dir: dir.path().join("node"),
            concept_registry_mode: onebrain_node::ConceptRegistryMode::Disabled,
            ..Default::default()
        };
        std::fs::create_dir_all(&config.data_dir).unwrap();
        let mut node = OneBrainNode::new(config).await.unwrap();
        let inputs = Arc::new(Inputs::new());
        let mut runtime = base_runtime_config_for_api_token(TOKEN);
        runtime.ku = Some(KuRuntimeConfig {
            vault_key: VaultKey::from_bytes([8; 32]),
            registry: Some(registry.clone()),
            inputs: if manual {
                Arc::new(
                    onebrain_node::ku_manual::ManualKuInputs::new(
                        [0; 32],
                        registry,
                        vec![(
                            "Operator-admitted test source".into(),
                            inputs.source.clone(),
                        )],
                    )
                    .unwrap(),
                )
            } else {
                inputs.clone()
            },
            public: None,
        });
        node.install_base_runtime(runtime).unwrap();
        let server = ApiServer::new(node, TOKEN.into(), 0);
        let state = server.test_state();
        let router = server.build_router();
        Self {
            _dir: dir,
            state,
            router,
            inputs,
            root,
        }
    }
    async fn session(&self) -> Value {
        let (code, value) = call(
            self.router.clone(),
            Method::GET,
            "/api/vnext/ku/status",
            None,
            Some(TOKEN),
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{value}");
        value["data"]["session"].clone()
    }
    async fn reserve(&self, session: &Value) -> String {
        let (code, value) = call(
            self.router.clone(),
            Method::POST,
            "/api/vnext/ku/reservations",
            Some(json!({"session":session})),
            Some(TOKEN),
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{value}");
        value["data"]["payload"]["operation_id"]
            .as_str()
            .unwrap()
            .into()
    }
    fn prepare(&self, op: &str, version: u8) -> Value {
        json!({"operation_id":op,"idempotency_key":op,"input_mode":"resolved_semantic_draft","source_refs":[self.inputs.cid],"registry_release_root":self.root,"semantic_profile":"ku-semantic-content/1.0","implementation_commitment":"04".repeat(32),"destination":"LOCAL_ONLY","draft_ref":hex(&[version;32])})
    }
    async fn invoke(
        &self,
        session: &Value,
        operation: &str,
        payload: Value,
    ) -> (StatusCode, Value) {
        call(
            self.router.clone(),
            Method::POST,
            "/api/vnext/ku/operations",
            Some(envelope(session, operation, payload)),
            Some(TOKEN),
        )
        .await
    }
}

fn envelope(session: &Value, operation: &str, payload: Value) -> Value {
    json!({"session":session,"budget":{"max_items":256,"max_bytes":1048576,"max_work_units":1000000},"request":{"operation":operation,"payload":payload}})
}
async fn call(
    router: axum::Router,
    method: Method,
    path: &str,
    body: Option<Value>,
    token: Option<&str>,
) -> (StatusCode, Value) {
    raw_call(
        router,
        method,
        path,
        body.map(|v| v.to_string()).unwrap_or_default(),
        token,
    )
    .await
}
async fn raw_call(
    router: axum::Router,
    method: Method,
    path: &str,
    body: String,
    token: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = HttpRequest::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = router
        .oneshot(builder.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let code = response.status();
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let bytes = to_bytes(response.into_body(), 1048576).await.unwrap();
    (code, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn authenticated_local_lifecycle_preserves_exact_outputs_and_paging() {
    let f = Fixture::new().await;
    let session = f.session().await;
    let op = f.reserve(&session).await;
    let mut events = f.state.event_broadcast.subscribe();
    let request = f.prepare(&op, 1);
    let (status, p) = f.invoke(&session, "prepare", request.clone()).await;
    assert_eq!(status, StatusCode::OK, "{p}");
    assert_eq!(p["data"]["payload"]["validity"], "ready");
    assert_eq!(p["data"]["model_qualified"], false);
    let ids = p["data"]["payload"]["object_cids"].clone();
    assert_eq!(ids.as_array().unwrap().len(), 2);
    let (_, before) = f.invoke(&session, "list", json!({"limit":256})).await;
    assert_eq!(before["data"]["payload"]["items"], json!([]));
    let (_, preview) = f
        .invoke(&session, "preview", json!({"operation_id":op}))
        .await;
    assert_eq!(preview["data"]["payload"], p["data"]["payload"]);
    let (_, replay) = f.invoke(&session, "prepare", request.clone()).await;
    assert_eq!(replay["data"]["payload"], p["data"]["payload"]);
    assert_eq!(f.inputs.calls.load(Ordering::SeqCst), 1);
    let save = json!({"operation_id":op,"idempotency_key":op,"object_cids":ids});
    let (code, saved) = f.invoke(&session, "save", save.clone()).await;
    assert_eq!(code, StatusCode::OK, "{saved}");
    assert_eq!(saved["data"]["payload"]["state"], "committed");
    assert_eq!(saved["data"]["payload"]["published"], false);
    assert_eq!(saved["data"]["payload"]["authorizes_reward"], false);
    let (_, replay) = f.invoke(&session, "save", save).await;
    assert_eq!(replay["data"]["payload"], saved["data"]["payload"]);
    let (_, reconciled) = f
        .invoke(&session, "reconcile", json!({"operation_id":op}))
        .await;
    assert_eq!(reconciled["data"]["payload"], saved["data"]["payload"]);
    let (_, page) = f.invoke(&session, "list", json!({"limit":1})).await;
    let continuation = page["data"]["payload"]["continuation"].clone();
    assert!(continuation.as_str().unwrap().starts_with("obc1."));
    assert_eq!(page["meta"]["continuation"], continuation);
    assert_eq!(page["meta"]["coverage"], "partial");
    let (_, second) = f
        .invoke(
            &session,
            "list",
            json!({"limit":1,"continuation":continuation}),
        )
        .await;
    assert_ne!(
        page["data"]["payload"]["items"][0]["object_cid"],
        second["data"]["payload"]["items"][0]["object_cid"]
    );
    let (code, _) = f
        .invoke(
            &session,
            "search",
            json!({"query":"water","limit":1,"continuation":continuation}),
        )
        .await;
    assert_eq!(code, StatusCode::CONFLICT);
    let (_, search) = f
        .invoke(&session, "search", json!({"query":"water","limit":256}))
        .await;
    assert_eq!(
        search["data"]["payload"]["items"].as_array().unwrap().len(),
        2
    );
    let (_, get) = f
        .invoke(&session, "get", json!({"object_cid":ids[0]}))
        .await;
    assert_eq!(get["data"]["payload"]["object_cid"], ids[0]);
    assert_eq!(
        get["data"]["payload"]["canonical_bytes"],
        p["data"]["payload"]["artifacts"][0]["canonical_preview"]
    );
    assert!(events.try_recv().is_err(), "KU must not broadcast");
}

#[tokio::test]
async fn invalid_private_requests_and_stale_generations_have_no_dispatch() {
    let f = Fixture::new().await;
    let session = f.session().await;
    for (token, expected) in [
        (None, StatusCode::UNAUTHORIZED),
        (Some("wrong"), StatusCode::FORBIDDEN),
    ] {
        let (code, body) = call(
            f.router.clone(),
            Method::GET,
            "/api/vnext/ku/status",
            None,
            token,
        )
        .await;
        assert_eq!(code, expected);
        assert_eq!(body["ok"], false);
    }
    let op = f.reserve(&session).await;
    let good = envelope(&session, "prepare", f.prepare(&op, 1));
    for bad in [
        {
            let mut v = good.clone();
            v["session"]["dataset_generation"] = json!("ff".repeat(32));
            v
        },
        {
            let mut v = good.clone();
            v["session"]["process_generation"] = json!("ff".repeat(32));
            v
        },
    ] {
        let (code, _) = call(
            f.router.clone(),
            Method::POST,
            "/api/vnext/ku/operations",
            Some(bad),
            Some(TOKEN),
        )
        .await;
        assert_eq!(code, StatusCode::CONFLICT);
    }
    for bad in [
        {
            let mut v = good.clone();
            v["request"]["payload"]["authorized"] = json!(true);
            v
        },
        {
            let mut v = good.clone();
            v["request"]["payload"]["source_refs"] = json!(["AA".repeat(32)]);
            v
        },
        {
            let mut v = good.clone();
            v["request"]["payload"]["draft_ref"] = Value::Null;
            v
        },
        {
            let mut v = good.clone();
            v["budget"]["max_work_units"] = json!(1000001);
            v
        },
        {
            let mut v = good.clone();
            v["budget"]["max_bytes"] = json!(1);
            v
        },
        {
            let mut v = good.clone();
            v["request"]["payload"]["destination"] = json!("PUBLIC");
            v
        },
    ] {
        let (code, _) = call(
            f.router.clone(),
            Method::POST,
            "/api/vnext/ku/operations",
            Some(bad),
            Some(TOKEN),
        )
        .await;
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }
    let duplicated = good.to_string().replace(
        "\"input_mode\":",
        "\"input_mode\":\"PRIVATE_CANARY\",\"input_mode\":",
    );
    for bad in [duplicated, "PRIVATE_CANARY".repeat(100000)] {
        let (code, value) = raw_call(
            f.router.clone(),
            Method::POST,
            "/api/vnext/ku/operations",
            bad,
            Some(TOKEN),
        )
        .await;
        assert_eq!(code, StatusCode::BAD_REQUEST);
        assert!(!value.to_string().contains("PRIVATE_CANARY"));
    }
    assert_eq!(f.inputs.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn unqualified_ai_and_unresolved_sources_cannot_save() {
    let f = Fixture::new().await;
    let session = f.session().await;
    let op = f.reserve(&session).await;
    let mut ai = f.prepare(&op, 1);
    ai["input_mode"] = json!("local_ai");
    ai.as_object_mut().unwrap().remove("draft_ref");
    let (code, e) = f.invoke(&session, "prepare", ai).await;
    assert_eq!(code, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(e["error"]["failure"]["code"], "CapabilityDisabled");
    assert_eq!(f.inputs.calls.load(Ordering::SeqCst), 0);
    f.inputs.unresolved.store(true, Ordering::SeqCst);
    let (code, p) = f.invoke(&session, "prepare", f.prepare(&op, 1)).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(p["data"]["payload"]["validity"], "needs_resolution");
    assert_eq!(p["meta"]["coverage"], "partial");
    assert_eq!(p["data"]["payload"]["artifacts"], json!([]));
    let (code, _) = f
        .invoke(
            &session,
            "save",
            json!({"operation_id":op,"idempotency_key":op,"object_cids":["11".repeat(32)]}),
        )
        .await;
    assert_ne!(code, StatusCode::OK);
    let (_, cancel) = f
        .invoke(&session, "cancel", json!({"operation_id":op}))
        .await;
    assert_eq!(cancel["data"]["payload"]["state"], "canceled");
}

#[tokio::test]
async fn service_work_releases_node_lock_and_cancel_can_reach_owner() {
    let f = Fixture::new().await;
    let session = f.session().await;
    let op = f.reserve(&session).await;
    f.inputs.wait.store(true, Ordering::SeqCst);
    let router = f.router.clone();
    let body = envelope(&session, "prepare", f.prepare(&op, 1));
    let pending = tokio::spawn(async move {
        call(
            router,
            Method::POST,
            "/api/vnext/ku/operations",
            Some(body),
            Some(TOKEN),
        )
        .await
    });
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        f.inputs.entered.notified(),
    )
    .await
    .unwrap();
    let guard = tokio::time::timeout(std::time::Duration::from_secs(1), f.state.node.lock())
        .await
        .expect("node mutex held across extraction");
    drop(guard);
    let (code, cancel) = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        f.invoke(&session, "cancel", json!({"operation_id":op})),
    )
    .await
    .unwrap();
    assert_eq!(code, StatusCode::OK, "{cancel}");
    f.inputs.resume.notify_one();
    let (code, _) = pending.await.unwrap();
    assert_ne!(code, StatusCode::OK);
    let (_, page) = f.invoke(&session, "list", json!({"limit":256})).await;
    assert_eq!(page["data"]["payload"]["items"], json!([]));
}

#[tokio::test]
async fn revisions_and_export_keep_service_authority_and_exact_predecessors() {
    let f = Fixture::new().await;
    let session = f.session().await;
    let op = f.reserve(&session).await;
    let (_, prepared) = f.invoke(&session, "prepare", f.prepare(&op, 1)).await;
    let ids = prepared["data"]["payload"]["object_cids"].clone();
    let (_, saved) = f
        .invoke(
            &session,
            "save",
            json!({"operation_id":op,"idempotency_key":op,"object_cids":ids}),
        )
        .await;
    let (code, status) = f
        .invoke(&session, "status", json!({"operation_id":op}))
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(
        status["data"]["payload"]["receipt"],
        saved["data"]["payload"]
    );
    let (_, page) = f.invoke(&session, "list", json!({"limit":256})).await;
    let revision = f.reserve(&session).await;
    let (code,revised)=f.invoke(&session,"revise",json!({"preparation":f.prepare(&revision,2),"predecessor_object_cid":ids[0],"expected_revision_frontier":page["data"]["payload"]["snapshot_frontier"]})).await;
    assert_eq!(code, StatusCode::OK, "{revised}");
    let changed = revised["data"]["payload"]["object_cids"].clone();
    assert_ne!(ids, changed);
    let (code, receipt) = f
        .invoke(
            &session,
            "save",
            json!({"operation_id":revision,"idempotency_key":revision,"object_cids":changed}),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{receipt}");
    let (code, original) = f
        .invoke(&session, "get", json!({"object_cid":ids[0]}))
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(
        original["data"]["payload"]["canonical_bytes"],
        prepared["data"]["payload"]["artifacts"][0]["canonical_preview"]
    );
    // A transport adapter cannot turn private objects into public exchange or
    // invent the separate Base-management capability needed for archives.
    for (mode, expected) in [
        ("canonical_public_exchange", StatusCode::CONFLICT),
        ("encrypted_base_archive", StatusCode::SERVICE_UNAVAILABLE),
    ] {
        let (code, value) = f
            .invoke(&session, "export", json!({"mode":mode,"object_cids":ids}))
            .await;
        assert_eq!(code, expected, "{value}");
        assert!(value.get("data").is_none());
    }
    f.inputs.revoked.store(true, Ordering::SeqCst);
    let denied = f.reserve(&session).await;
    let (code, value) = f.invoke(&session, "prepare", f.prepare(&denied, 3)).await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    assert!(!value.to_string().contains("PRIVATE"));
    // Revocation blocks fresh source use; already saved KU follows owner policy.
    let (code, _) = f
        .invoke(&session, "get", json!({"object_cid":ids[0]}))
        .await;
    assert_eq!(code, StatusCode::OK);
}

#[tokio::test]
async fn unavailable_runtime_and_response_overflow_are_typed_not_success() {
    let dir = tempfile::tempdir().unwrap();
    let node = OneBrainNode::new(NodeConfig {
        data_dir: dir.path().to_path_buf(),
        concept_registry_mode: onebrain_node::ConceptRegistryMode::Disabled,
        ..Default::default()
    })
    .await
    .unwrap();
    let router = ApiServer::new(node, TOKEN.into(), 0).build_router();
    let (code, value) = call(
        router,
        Method::GET,
        "/api/vnext/ku/status",
        None,
        Some(TOKEN),
    )
    .await;
    assert_eq!(code, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(value["error"]["failure"]["code"], "DependencyUnavailable");
    let response = success(
        Session {
            process_generation: "00".repeat(32),
            dataset_generation: "00".repeat(32),
        },
        "PRIVATE_CANARY".repeat(4096),
        meta(),
        32768,
    );
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let bytes = to_bytes(response.into_body(), 32768).await.unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["error"]["failure"]["reconcile_before_retry"], true);
    assert!(!value.to_string().contains("PRIVATE_CANARY"));
}

#[tokio::test]
async fn errors_keep_all_base_retry_and_reconcile_policies() {
    use BaseErrorCodeV1::*;
    for code in [
        InvalidRequest,
        NotFound,
        Conflict,
        Expired,
        RateLimited,
        CapabilityDisabled,
        DependencyUnavailable,
        IncompatibleProfile,
        ResourceExhausted,
        CorruptState,
        ReprovisionRequired,
        UnknownOutcome,
        InternalError,
    ] {
        let n = code.discriminator();
        let response = failure(BaseServiceError::new(code, "bounded_reason"));
        let bytes = to_bytes(response.into_body(), 32768).await.unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        let error: KuFailureV1 = serde_json::from_value(value["error"]["failure"].clone()).unwrap();
        error.validate().unwrap();
        assert_eq!(value["error"]["discriminator"], n);
        assert_eq!(error.retryable, code.retryable());
        assert_eq!(error.reconcile_before_retry, code.reconcile_before_retry());
    }
}

// Signed registry fixtures are authored only for temporary integration tests.
mod registry_fixture;
use registry_fixture::registry;
mod experimental_text;

/// Development-only integration: genuine installed model, synthetic signed
/// Registry, fresh explicit consent/text. Never opens qualification holdouts.
#[cfg(windows)]
#[tokio::test]
#[ignore = "requires explicitly configured local Ollama artifacts; real inference"]
async fn experimental_ollama_real_text_roundtrip() {
    use ku_encoder::extraction::ManagedOllamaProvider;
    use onebrain_node::{ku_manual::ManualKuInputs, ku_ollama::OllamaKuInputs};
    let dir = tempfile::tempdir().unwrap();
    let registry = registry_fixture::registry_for_label(dir.path(), "is");
    let model = std::env::var("KU_OLLAMA_MODEL").unwrap_or_else(|_| "qwen3:8b".into());
    let memory = 12 * 1024u64.pow(3);
    let started = std::time::Instant::now();
    let provider = Arc::new(
        ManagedOllamaProvider::open(
            std::env::var_os("KU_OLLAMA_EXE")
                .expect("set KU_OLLAMA_EXE")
                .into(),
            std::env::var_os("KU_OLLAMA_MODELS")
                .expect("set KU_OLLAMA_MODELS")
                .into(),
            &model,
            memory,
            Arc::new(tokio::sync::Semaphore::new(1)),
        )
        .unwrap(),
    );
    eprintln!(
        "Verified development model artifacts in {:?}",
        started.elapsed()
    );
    eprintln!(
        "Development provider pins: {}",
        ku_encoder::extraction::ExtractionProvider::manifest(provider.as_ref())
    );
    let config = NodeConfig {
        data_dir: dir.path().join("node"),
        concept_registry_mode: onebrain_node::ConceptRegistryMode::Disabled,
        ..Default::default()
    };
    std::fs::create_dir_all(&config.data_dir).unwrap();
    let make_inputs = || {
        Arc::new(
            OllamaKuInputs::new(
                [0; 32],
                Arc::new(ManualKuInputs::new([0; 32], registry.clone(), vec![]).unwrap()),
                registry.clone(),
                vec![(model.clone(), provider.clone(), memory)],
            )
            .unwrap(),
        )
    };
    let make_runtime = || {
        let mut runtime = base_runtime_config_for_api_token(TOKEN);
        runtime.ku = Some(KuRuntimeConfig {
            vault_key: VaultKey::from_bytes([8; 32]),
            registry: Some(registry.clone()),
            inputs: make_inputs(),
            public: None,
        });
        runtime
    };
    let mut node = OneBrainNode::new(config.clone()).await.unwrap();
    node.install_base_runtime(make_runtime()).unwrap();
    let server = ApiServer::new(node, TOKEN.into(), 0);
    let f = Fixture {
        _dir: dir,
        state: server.test_state(),
        router: server.build_router(),
        inputs: Arc::new(Inputs::new()),
        root: registry
            .reader_lease()
            .status()
            .release_aggregate_root
            .clone()
            .unwrap(),
    };
    let session = f.session().await;
    let (code, models) = edit(&f, &session, "models", json!({})).await;
    assert_eq!(code, StatusCode::OK, "{models}");
    assert_eq!(models["data"]["model_qualified"], false);
    let op = f.reserve(&session).await;
    let mut intake = json!({"operation_id":op,"idempotency_key":op,"model":model,"text":"Copper is conductive.","consent":false});
    assert_eq!(
        edit(&f, &session, "encode_text", intake.clone()).await.0,
        StatusCode::BAD_REQUEST
    );
    intake["consent"] = true.into();
    let (code, template) = edit(&f, &session, "encode_text", intake.clone()).await;
    assert_eq!(code, StatusCode::OK, "{template}");
    assert_eq!(
        edit(&f, &session, "encode_text", intake.clone()).await.1["data"]["payload"],
        template["data"]["payload"]
    );
    intake["text"] = "Changed text".into();
    assert_eq!(
        edit(&f, &session, "encode_text", intake).await.0,
        StatusCode::CONFLICT
    );
    let inference = std::time::Instant::now();
    let (code, preview) = f
        .invoke(&session, "prepare", template["data"]["payload"].clone())
        .await;
    eprintln!(
        "Development inference and validation: {:?}; HTTP {}",
        inference.elapsed(),
        code
    );
    assert_eq!(code, StatusCode::OK, "{preview}");
    assert_eq!(preview["data"]["payload"]["validity"], "ready", "{preview}");
    let ids = preview["data"]["payload"]["object_cids"].clone();
    let (code, saved) = f
        .invoke(
            &session,
            "save",
            json!({"operation_id":op,"idempotency_key":op,"object_cids":ids}),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{saved}");
    assert_eq!(saved["data"]["payload"]["state"], "committed");
    // Reopen the exact private journal with fresh custody, then read accepted KU.
    drop(f.router);
    drop(f.state);
    let mut node = OneBrainNode::new(config).await.unwrap();
    node.install_base_runtime(make_runtime()).unwrap();
    let server = ApiServer::new(node, TOKEN.into(), 0);
    let router = server.build_router();
    let (_, status) = call(
        router.clone(),
        Method::GET,
        "/api/vnext/ku/status",
        None,
        Some(TOKEN),
    )
    .await;
    let (code, view) = call(
        router,
        Method::POST,
        "/api/vnext/ku/operations",
        Some(envelope(
            &status["data"]["session"],
            "get",
            json!({"object_cid":ids[0]}),
        )),
        Some(TOKEN),
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{view}");
    assert_eq!(view["data"]["payload"]["disclosure_class"], "LOCAL_ONLY");
}

async fn edit(f: &Fixture, session: &Value, action: &str, payload: Value) -> (StatusCode, Value) {
    call(f.router.clone(), Method::POST, "/api/vnext/ku/editor",
        Some(json!({"session":session,"budget":{"max_items":64,"max_bytes":1048576,"max_work_units":1000000},"request":{"action":action,"payload":payload}})), Some(TOKEN)).await
}

#[tokio::test]
async fn manual_editor_real_provider_journey_and_revision() {
    let f = Fixture::configured(true).await;
    let session = f.session().await;
    let (code, catalog) = edit(&f, &session, "catalog", json!({})).await;
    assert_eq!(code, StatusCode::OK, "{catalog}");
    assert_eq!(
        catalog["data"]["payload"]["sources"][0]["source_ref"],
        json!(f.inputs.cid)
    );
    let (code, concepts) = edit(&f, &session, "resolve", json!({"label":"water"})).await;
    assert_eq!(code, StatusCode::OK, "{concepts}");
    assert_eq!(
        concepts["data"]["payload"]["candidates"][0]["ccid"],
        "07".repeat(16)
    );
    let mut predecessor = None;
    let mut frontier = None;
    for text in [
        "Manual private first assertion",
        "Manual private revised assertion",
    ] {
        let op = f.reserve(&session).await;
        let input = json!({"operation_id":op,"idempotency_key":op,"source_ref":f.inputs.cid,"predicate_label":"water","selected_ccid":"07".repeat(16),"argument_text":text});
        let (code, admitted) = edit(&f, &session, "draft", input.clone()).await;
        assert_eq!(code, StatusCode::OK, "{admitted}");
        let (_, replay) = edit(&f, &session, "draft", input.clone()).await;
        assert_eq!(admitted, replay);
        let mut changed = input.clone();
        changed["argument_text"] = json!("changed");
        assert_eq!(
            edit(&f, &session, "draft", changed).await.0,
            StatusCode::CONFLICT
        );
        let preparation = admitted["data"]["payload"].clone();
        let (code, prepared) = if let Some(cid) = &predecessor {
            f.invoke(&session,"revise",json!({"preparation":preparation,"predecessor_object_cid":cid,"expected_revision_frontier":frontier})).await
        } else {
            f.invoke(&session, "prepare", preparation).await
        };
        assert_eq!(code, StatusCode::OK, "{prepared}");
        assert_eq!(prepared["data"]["payload"]["validity"], "ready");
        let artifacts = prepared["data"]["payload"]["artifacts"].as_array().unwrap();
        assert_eq!(artifacts.len(), 1);
        let (_, before) = f.invoke(&session, "list", json!({"limit":20})).await;
        assert_eq!(
            before["data"]["payload"]["items"].as_array().unwrap().len(),
            usize::from(predecessor.is_some())
        );
        let save = json!({"operation_id":op,"idempotency_key":op,"object_cids":prepared["data"]["payload"]["object_cids"]});
        let (code, receipt) = f.invoke(&session, "save", save.clone()).await;
        assert_eq!(code, StatusCode::OK, "{receipt}");
        assert_eq!(receipt["data"]["payload"]["state"], "committed");
        assert_eq!(receipt["data"]["payload"]["published"], false);
        let (_, repeated) = f.invoke(&session, "save", save).await;
        assert_eq!(receipt, repeated);
        let cid = prepared["data"]["payload"]["object_cids"][0].clone();
        let (_, view) = f.invoke(&session, "get", json!({"object_cid":cid})).await;
        assert_eq!(
            view["data"]["payload"]["canonical_bytes"],
            artifacts[0]["canonical_preview"]
        );
        let (_, page) = f
            .invoke(
                &session,
                "search",
                json!({"query":"Manual private","limit":20}),
            )
            .await;
        frontier = Some(page["data"]["payload"]["snapshot_frontier"].clone());
        predecessor = Some(cid);
    }
    assert_eq!(
        f.inputs.calls.load(Ordering::SeqCst),
        0,
        "fixture encoder must never execute"
    );
    let (_, page) = f.invoke(&session, "list", json!({"limit":20})).await;
    assert_eq!(
        page["data"]["payload"]["items"].as_array().unwrap().len(),
        2
    );
}

#[tokio::test]
async fn manual_editor_unresolved_auth_and_closed_input() {
    let f = Fixture::configured(true).await;
    let session = f.session().await;
    assert_eq!(
        edit(&f, &session, "catalog", json!({"authorized":true}))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let op = f.reserve(&session).await;
    let input = json!({"operation_id":op,"idempotency_key":op,"source_ref":f.inputs.cid,"predicate_label":"missing-private-label","argument_text":"PRIVATE-MANUAL-INPUT"});
    for (field, value) in [
        ("authorized", json!(true)),
        ("argument_text", json!("x".repeat(4097))),
        ("predicate_label", json!("x".repeat(257))),
        ("operation_id", json!("ee".repeat(32))),
        ("selected_ccid", Value::Null),
        ("source_ref", json!("ff".repeat(32))),
        ("selected_ccid", json!("ff".repeat(16))),
        ("argument_text", json!("e\u{301}")),
    ] {
        let mut invalid = input.clone();
        invalid[field] = value;
        let (code, failure) = edit(&f, &session, "draft", invalid).await;
        assert!(code.is_client_error(), "{failure}");
        assert!(!failure.to_string().contains("PRIVATE-MANUAL-INPUT"));
    }
    let mut stale = session.clone();
    stale["process_generation"] = json!("ff".repeat(32));
    assert_eq!(
        edit(&f, &stale, "catalog", json!({})).await.0,
        StatusCode::CONFLICT
    );
    let (code, admitted) = edit(&f, &session, "draft", input).await;
    assert_eq!(code, StatusCode::OK, "{admitted}");
    let (code, prepared) = f
        .invoke(&session, "prepare", admitted["data"]["payload"].clone())
        .await;
    assert_eq!(code, StatusCode::OK, "{prepared}");
    assert_eq!(prepared["data"]["payload"]["validity"], "needs_resolution");
    assert_eq!(prepared["data"]["payload"]["artifacts"], json!([]));
    assert_ne!(
        f.invoke(
            &session,
            "save",
            json!({"operation_id":op,"idempotency_key":op,"object_cids":[]})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        f.invoke(&session, "cancel", json!({"operation_id":op}))
            .await
            .0,
        StatusCode::OK
    );
    for token in [None, Some("incorrect")] {
        let (code, _) = call(
            f.router.clone(),
            Method::POST,
            "/api/vnext/ku/editor",
            Some(json!({})),
            token,
        )
        .await;
        assert!(code == StatusCode::UNAUTHORIZED || code == StatusCode::FORBIDDEN);
    }
    let old = Fixture::new().await;
    assert_eq!(
        edit(&old, &old.session().await, "catalog", json!({}))
            .await
            .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
}
