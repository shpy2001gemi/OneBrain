//! Deterministic proposal double exercises the actual private intake/encoder/API
//! composition. This test double is never used by the local host example.
use super::*;
use ku_encoder::extraction::{
    ExtractionError, ExtractionProvider, ExtractionWorkflow, ProviderRequest,
};
use onebrain_node::{ku_manual::ManualKuInputs, ku_ollama::OllamaKuInputs};

struct Proposal {
    manifest: Value,
    calls: AtomicUsize,
    bad: bool,
    wait: bool,
    entered: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl ExtractionProvider for Proposal {
    fn manifest(&self) -> &Value {
        &self.manifest
    }
    fn input_tokens(&self, _: &ProviderRequest) -> Result<u32, ExtractionError> {
        Ok(100)
    }
    async fn extract(&self, request: ProviderRequest) -> Result<Vec<u8>, ExtractionError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered.notify_one();
        if self.wait {
            std::future::pending::<()>().await;
        }
        if self.bad {
            return Ok(br#"{"profile":"ku-extraction/1.0","profile":"forged"}"#.to_vec());
        }
        let output = json!({"profile":"ku-extraction/1.0","attempt_id":request.input["attempt_id"],"context_sha256":request.input["context_sha256"],
            "concepts":[{"key":"p","label":"is","evidence":{"start":7,"end":9,"quote":"is"}}],
            "statements":[{"key":"s","predicate":"p","arguments":[
                {"kind":"text","value":"Copper","evidence":{"start":0,"end":6,"quote":"Copper"}},
                {"kind":"text","value":"conductive","evidence":{"start":10,"end":20,"quote":"conductive"}}],
                "evidence":[{"start":0,"end":21,"quote":"Copper is conductive."}],"negation":{"value":false,"evidence":[]},"modality":{"value":"asserted","evidence":[]}}],
            "coverage":[{"unit":"source","status":"represented","statements":["s"],"reason":"none"}]});
        Ok(serde_json::to_vec(&output).unwrap())
    }
}
fn proposal(bad: bool, wait: bool) -> Arc<Proposal> {
    Arc::new(Proposal {
        manifest: json!({"profile":"ku-extraction-provider/1.0","provider_id":"test-only-proposal","backend_build_sha256":"01".repeat(32),
        "mode":"json_schema","tools_enabled":false,"max_context_tokens":8192,"peak_bytes_reservation":4096,
        "schema_bundle_sha256":ExtractionWorkflow::bundle_hash(),"model_artifact_sha256":"02".repeat(32),"tokenizer_sha256":"03".repeat(32),"supported_schema_keywords":[],"temperature_milli":0,"seed":1}),
        calls: AtomicUsize::new(0),
        bad,
        wait,
        entered: tokio::sync::Notify::new(),
    })
}
fn inputs(
    registry: Arc<onebrain_node::concept_registry_runtime::ConceptRegistryGenerationManager>,
    provider: Option<Arc<Proposal>>,
) -> Arc<dyn KuInputProvider> {
    let manual = Arc::new(ManualKuInputs::new([0; 32], registry.clone(), vec![]).unwrap());
    let models: Vec<(String, Arc<dyn ExtractionProvider>, u64)> = provider
        .into_iter()
        .map(|p| {
            (
                "qwen3:test-only".into(),
                p as Arc<dyn ExtractionProvider>,
                4096,
            )
        })
        .collect();
    Arc::new(OllamaKuInputs::new([0; 32], manual, registry, models).unwrap())
}
async fn fixture(
    provider: Arc<Proposal>,
) -> (
    Fixture,
    NodeConfig,
    Arc<onebrain_node::concept_registry_runtime::ConceptRegistryGenerationManager>,
) {
    let dir = tempfile::tempdir().unwrap();
    let registry = registry_fixture::registry_for_label(dir.path(), "is");
    let config = NodeConfig {
        data_dir: dir.path().join("node"),
        concept_registry_mode: onebrain_node::ConceptRegistryMode::Disabled,
        ..Default::default()
    };
    std::fs::create_dir_all(&config.data_dir).unwrap();
    let mut node = OneBrainNode::new(config.clone()).await.unwrap();
    let mut runtime = base_runtime_config_for_api_token(TOKEN);
    runtime.ku = Some(KuRuntimeConfig {
        vault_key: VaultKey::from_bytes([8; 32]),
        registry: Some(registry.clone()),
        inputs: inputs(registry.clone(), Some(provider)),
        public: None,
    });
    node.install_base_runtime(runtime).unwrap();
    let server = ApiServer::new(node, TOKEN.into(), 0);
    (
        Fixture {
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
        },
        config,
        registry,
    )
}
async fn intake(f: &Fixture, session: &Value) -> (String, Value) {
    let op = f.reserve(session).await;
    let body = json!({"operation_id":op,"idempotency_key":op,"model":"qwen3:test-only","text":"Copper is conductive.","consent":true});
    let (code, result) = edit(f, session, "encode_text", body).await;
    assert_eq!(code, StatusCode::OK, "{result}");
    (op, result["data"]["payload"].clone())
}
#[tokio::test]
async fn text_intake_save_reopen_without_model_and_no_resampling() {
    let provider = proposal(false, false);
    let (f, config, registry) = fixture(provider.clone()).await;
    let session = f.session().await;
    let (op, template) = intake(&f, &session).await;
    let (code, preview) = f.invoke(&session, "prepare", template).await;
    assert_eq!(code, StatusCode::OK, "{preview}");
    assert_eq!(preview["data"]["payload"]["validity"], "ready");
    let ids = preview["data"]["payload"]["object_cids"].clone();
    assert_eq!(
        f.invoke(&session, "list", json!({"limit":20})).await.1["data"]["payload"]["items"],
        json!([])
    );
    let (code, saved) = f
        .invoke(
            &session,
            "save",
            json!({"operation_id":op,"idempotency_key":op,"object_cids":ids}),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{saved}");
    assert_eq!(saved["data"]["payload"]["state"], "committed");
    drop(f.router);
    drop(f.state);
    let mut node = OneBrainNode::new(config).await.unwrap();
    let mut runtime = base_runtime_config_for_api_token(TOKEN);
    runtime.ku = Some(KuRuntimeConfig {
        vault_key: VaultKey::from_bytes([8; 32]),
        registry: Some(registry.clone()),
        inputs: inputs(registry, None),
        public: None,
    });
    node.install_base_runtime(runtime).unwrap();
    let router = ApiServer::new(node, TOKEN.into(), 0).build_router();
    let (_, status) = call(
        router.clone(),
        Method::GET,
        "/api/vnext/ku/status",
        None,
        Some(TOKEN),
    )
    .await;
    assert_eq!(status["data"]["payload"]["local_encoder_ready"], false);
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
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn text_intake_fences_consent_model_operation_and_generation() {
    let provider = proposal(false, false);
    let (f, _, _) = fixture(provider.clone()).await;
    let session = f.session().await;
    let op = f.reserve(&session).await;
    let valid = json!({"operation_id":op,"idempotency_key":op,"model":"qwen3:test-only","text":"Copper is conductive.","consent":true});
    for (key, value, expected) in [
        ("consent", json!(false), StatusCode::BAD_REQUEST),
        ("text", json!("a".repeat(8193)), StatusCode::BAD_REQUEST),
        (
            "model",
            json!("qwen3:not-installed"),
            StatusCode::SERVICE_UNAVAILABLE,
        ),
        (
            "operation_id",
            json!("f1".repeat(32)),
            StatusCode::NOT_FOUND,
        ),
    ] {
        let mut changed = valid.clone();
        changed[key] = value;
        assert_eq!(
            edit(&f, &session, "encode_text", changed).await.0,
            expected,
            "{key}"
        );
    }
    let mut stale = session.clone();
    stale["process_generation"] = "ee".repeat(32).into();
    assert_eq!(
        edit(&f, &stale, "encode_text", valid.clone()).await.0,
        StatusCode::CONFLICT
    );
    let (code, result) = edit(&f, &session, "encode_text", valid.clone()).await;
    assert_eq!(code, StatusCode::OK, "{result}");
    assert_eq!(
        edit(&f, &session, "encode_text", valid.clone()).await.1["data"]["payload"],
        result["data"]["payload"]
    );
    let mut changed = valid;
    changed["text"] = "Copper is different.".into();
    assert_eq!(
        edit(&f, &session, "encode_text", changed).await.0,
        StatusCode::CONFLICT
    );
    let mut template = result["data"]["payload"].clone();
    template["implementation_commitment"] = "aa".repeat(32).into();
    assert_eq!(
        f.invoke(&session, "prepare", template).await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn experimental_invalid_output_cannot_save_and_cancel_stops_pending_extraction() {
    let provider = proposal(true, false);
    let (f, _, _) = fixture(provider.clone()).await;
    let session = f.session().await;
    let (op, template) = intake(&f, &session).await;
    assert_ne!(
        f.invoke(&session, "prepare", template).await.0,
        StatusCode::OK
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    assert_ne!(
        f.invoke(
            &session,
            "save",
            json!({"operation_id":op,"idempotency_key":op,"object_cids":["aa".repeat(32)]})
        )
        .await
        .0,
        StatusCode::OK
    );
    let provider = proposal(false, true);
    let (f, _, _) = fixture(provider.clone()).await;
    let session = f.session().await;
    let (op, template) = intake(&f, &session).await;
    let router = f.router.clone();
    let body = envelope(&session, "prepare", template);
    let task = tokio::spawn(async move {
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
        provider.entered.notified(),
    )
    .await
    .unwrap();
    let (code, result) = f
        .invoke(&session, "cancel", json!({"operation_id":op}))
        .await;
    assert_eq!(code, StatusCode::OK, "{result}");
    assert_eq!(result["data"]["payload"]["state"], "canceled");
    assert_ne!(
        tokio::time::timeout(std::time::Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap()
            .0,
        StatusCode::OK
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        f.invoke(&session, "list", json!({"limit":20})).await.1["data"]["payload"]["items"],
        json!([])
    );
}
