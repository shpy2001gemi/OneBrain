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
    bad_label: bool,
    wait: bool,
    wait_review: bool,
    entered: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl ExtractionProvider for Proposal {
    fn review_task_tokens(
        &self,
        _: &ku_encoder::extraction::review_draft::TaskRequest,
    ) -> Result<u32, ExtractionError> {
        Ok(100)
    }
    async fn review_task(
        &self,
        request: ku_encoder::extraction::review_draft::TaskRequest,
    ) -> Result<Vec<u8>, ExtractionError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered.notify_one();
        if self.wait || (self.wait_review && request.kind == ku_encoder::extraction::review_draft::TaskKind::RepairSelection) {
            std::future::pending::<()>().await;
        }
        let output = match request.kind {
            ku_encoder::extraction::review_draft::TaskKind::Select => {
                let mut v=json!({"claims":[{"subject":"Copper","predicate":"is","arguments":["conductive"]}]});
                if self.wait_review { v["claims"][0]["anchor"]=json!("conductive"); }
                v
            }
            ku_encoder::extraction::review_draft::TaskKind::RepairSelection => {
                assert_eq!(request.data["FIELDS"],json!(["anchor"]));
                json!({"patch":{"anchor":"Copper is conductive."}})
            }
            ku_encoder::extraction::review_draft::TaskKind::Draft => {
                json!({"statements":[{"subject":"Copper","predicate":"is","arguments":["conductive"],"frequency":[],"negation":[],"condition":[],"time":[],"location":[],"modality":[],"approximation":[],"numbers":[],"relations":[],"evidence":"Copper is conductive."}],"unresolved":[]})
            }
            ku_encoder::extraction::review_draft::TaskKind::Review => {
                json!({"edits":[],"missing":[],"unresolved":[]})
            }
            _ => return Err(ExtractionError("unexpected_test_task")),
        };
        Ok(serde_json::to_vec(&output).unwrap())
    }
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
        if self.bad && !request.repair_errors.is_empty() {
            assert!(request
                .repair_errors
                .iter()
                .any(|e| e.contains("$.statements[0].arguments[0].unit: missing_field")));
        }
        let mut output = json!({"profile":"ku-extraction/1.0","attempt_id":request.input["attempt_id"],"context_sha256":request.input["context_sha256"],
            "concepts":[{"key":"p","label":"is","evidence":{"start":7,"end":9,"quote":"is"}}],
            "statements":[{"key":"s","predicate":"p","arguments":[
                {"kind":"text","value":"Copper","evidence":{"start":0,"end":6,"quote":"Copper"}},
                {"kind":"text","value":"conductive","evidence":{"start":10,"end":20,"quote":"conductive"}}],
                "evidence":[{"start":0,"end":21,"quote":"Copper is conductive."}],"negation":{"value":false,"evidence":[]},"modality":{"value":"asserted","evidence":[]}}],
            "coverage":[{"unit":"source","status":"represented","statements":["s"],"reason":"none"}]});
        if self.bad {
            output["statements"][0]["arguments"][0] = json!({"kind":"quantity","number":{"start":0,"end":1,"quote":"PRIVATE_DIAGNOSTIC_SENTINEL"}});
        }
        if self.bad_label {
            if !request.repair_errors.is_empty() {
                assert!(request
                    .repair_errors
                    .iter()
                    .any(|e| e.contains("$.concepts[0].label: concept_label")));
            }
            output["concepts"][0]["label"] = "PRIVATE_DIAGNOSTIC_SENTINEL".into();
        }
        Ok(serde_json::to_vec(&output).unwrap())
    }
}
fn proposal(bad: bool, wait: bool) -> Arc<Proposal> {
    Arc::new(Proposal {
        manifest: json!({"profile":"ku-extraction-provider/1.0","provider_id":"experimental-ollama-qwen3:test-only-proposal","backend_build_sha256":"01".repeat(32),
        "mode":"json_schema","tools_enabled":false,"max_context_tokens":8192,"peak_bytes_reservation":4096,
        "schema_bundle_sha256":ExtractionWorkflow::bundle_hash(),"model_artifact_sha256":"02".repeat(32),"tokenizer_sha256":"03".repeat(32),"supported_schema_keywords":[],"temperature_milli":0,"seed":1}),
        calls: AtomicUsize::new(0),
        bad,
        bad_label: false,
        wait,
        wait_review: false,
        entered: tokio::sync::Notify::new(),
    })
}
fn inputs(
    registry: Arc<onebrain_node::concept_registry_runtime::ConceptRegistryGenerationManager>,
    provider: Option<Arc<Proposal>>,
) -> Arc<dyn KuInputProvider> {
    inputs_for_profile(registry,provider,false)
}
fn inputs_for_profile(
    registry: Arc<onebrain_node::concept_registry_runtime::ConceptRegistryGenerationManager>,
    provider: Option<Arc<Proposal>>,
    v2: bool,
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
    let inputs=OllamaKuInputs::new([0; 32], manual, registry, models).unwrap();
    Arc::new(if v2 { inputs.with_selection_v2() } else { inputs })
}
async fn fixture(
    provider: Arc<Proposal>,
) -> (
    Fixture,
    NodeConfig,
    Arc<onebrain_node::concept_registry_runtime::ConceptRegistryGenerationManager>,
) {
    fixture_for_profile(provider,false).await
}
async fn fixture_for_profile(provider:Arc<Proposal>,v2:bool) -> (
    Fixture, NodeConfig,
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
        inputs: inputs_for_profile(registry.clone(), Some(provider),v2),
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
async fn review_draft_background_poll_restart_without_model_and_no_save() {
    background_poll_restart(false).await;
}
#[tokio::test]
async fn review_draft_v2_background_poll_restart_without_model_and_no_save() {
    background_poll_restart(true).await;
}
async fn background_poll_restart(v2:bool) {
    let provider = proposal(false, false);
    let (f, config, registry) = fixture_for_profile(provider.clone(),v2).await;
    let session = f.session().await;
    let op = f.reserve(&session).await;
    let payload = json!({"operation_id":op,"idempotency_key":op,"model":"qwen3:test-only","text":"Copper is conductive.","consent":true});
    let (code, started) = edit(&f, &session, "review_start", payload.clone()).await;
    assert_eq!(code, StatusCode::OK, "{started}");
    assert_eq!(
        started["data"]["payload"]["review_job"]["job"]["state"],
        "queued"
    );
    let view = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let (code, v) = edit(&f, &session, "review_get", json!({"operation_id":op})).await;
            assert_eq!(code, StatusCode::OK, "{v}");
            if v["data"]["payload"]["review_job"]["job"]["state"] == "draft_extracted" {
                break v;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(view["data"]["payload"]["review_job"]["canonical_ku"], false);
    if v2 { assert_eq!(view["data"]["payload"]["review_job"]["job"]["windows"][0]["revisions"][0]["draft"]["profile"],"ku-semantic-draft/2.0"); }
    assert_eq!(view["data"]["payload"]["review_job"]["semantic_verification"], "unassessed");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        edit(&f, &session, "review_start", payload).await.0,
        StatusCode::OK
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        f.invoke(&session, "list", json!({"limit":20})).await.1["data"]["payload"]["items"],
        json!([])
    );
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
    let (_,view)=call(router,Method::POST,"/api/vnext/ku/editor",Some(json!({"session":status["data"]["session"],"budget":{"max_items":256,"max_bytes":1048576,"max_work_units":1000000},"request":{"action":"review_get","payload":{"operation_id":op}}})),Some(TOKEN)).await;
    assert_eq!(
        view["data"]["payload"]["review_job"]["job"]["state"], "draft_extracted",
        "{view}"
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn review_draft_returns_before_model_and_cancel_fences_callback() {
    cancel_background(false).await;
}
#[tokio::test]
async fn review_draft_v2_returns_before_model_and_cancel_fences_callback() {
    cancel_background(true).await;
}
async fn cancel_background(v2:bool) {
    let provider = proposal(false, true);
    let (f, _, _) = fixture_for_profile(provider.clone(),v2).await;
    let session = f.session().await;
    let op = f.reserve(&session).await;
    let (_,start)=tokio::time::timeout(std::time::Duration::from_secs(3),edit(&f,&session,"review_start",json!({"operation_id":op,"idempotency_key":op,"model":"qwen3:test-only","text":"Copper is conductive.","consent":true}))).await.unwrap();
    assert!(start["ok"].as_bool().unwrap(), "{start}");
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        provider.entered.notified(),
    )
    .await
    .unwrap();
    let (_, view) = edit(&f, &session, "review_cancel", json!({"operation_id":op})).await;
    assert_eq!(
        view["data"]["payload"]["review_job"]["job"]["state"],
        "canceled"
    );
    tokio::time::sleep(std::time::Duration::from_millis(60)).await;
    let (_, view) = edit(&f, &session, "review_get", json!({"operation_id":op})).await;
    assert_eq!(
        view["data"]["payload"]["review_job"]["job"]["state"],
        "canceled"
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn review_draft_operation_cancel_stops_background_inference() {
    let provider = proposal(false, true);
    let (f, _, _) = fixture(provider.clone()).await;
    let session = f.session().await;
    let op = f.reserve(&session).await;
    let (_,start)=edit(&f,&session,"review_start",json!({"operation_id":op,"idempotency_key":op,"model":"qwen3:test-only","text":"Copper is conductive.","consent":true})).await;
    assert_eq!(start["ok"], true);
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        provider.entered.notified(),
    )
    .await
    .unwrap();
    assert_eq!(
        f.invoke(&session, "cancel", json!({"operation_id":op}))
            .await
            .0,
        StatusCode::OK
    );
    let (_, view) = edit(&f, &session, "review_get", json!({"operation_id":op})).await;
    assert_eq!(
        view["data"]["payload"]["review_job"]["job"]["state"],
        "canceled"
    );
    assert_eq!(
        edit(&f, &session, "review_resume", json!({"operation_id":op}))
            .await
            .0,
        StatusCode::CONFLICT
    );
    tokio::time::sleep(std::time::Duration::from_millis(60)).await;
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn review_draft_rejects_unconsented_or_stale_start_and_unauthenticated_read() {
    let provider = proposal(false, false);
    let (f, _, _) = fixture(provider.clone()).await;
    let session = f.session().await;
    let op = f.reserve(&session).await;
    let mut payload = json!({"operation_id":op,"idempotency_key":op,"model":"qwen3:test-only","text":"Copper is conductive.","consent":false});
    assert_eq!(
        edit(&f, &session, "review_start", payload.clone()).await.0,
        StatusCode::BAD_REQUEST
    );
    payload["consent"] = json!(true);
    let mut stale = session.clone();
    stale["process_generation"] = json!("e1".repeat(32));
    assert_eq!(
        edit(&f, &stale, "review_start", payload).await.0,
        StatusCode::CONFLICT
    );
    let body = json!({"session":session,"budget":{"max_items":256,"max_bytes":1048576,"max_work_units":1000000},"request":{"action":"review_get","payload":{"operation_id":op}}});
    let (code, _) = call(
        f.router.clone(),
        Method::POST,
        "/api/vnext/ku/editor",
        Some(body),
        None,
    )
    .await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
    let (_, list) = edit(&f, &session, "review_list", json!({})).await;
    assert_eq!(list["data"]["payload"]["review_jobs"], json!([]));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn review_draft_drain_restart_requires_explicit_resume_and_keeps_spent_budget() {
    drain_restart(false).await;
}
#[tokio::test]
async fn review_draft_v2_drain_restart_requires_explicit_resume_and_keeps_spent_budget() {
    drain_restart(true).await;
}
async fn drain_restart(v2:bool) {
    let mut waiting = proposal(false, false);
    Arc::get_mut(&mut waiting).unwrap().wait_review = true;
    let (f, _, registry) = fixture_for_profile(waiting.clone(),v2).await;
    let session = f.session().await;
    let op = f.reserve(&session).await;
    let (_,start)=edit(&f,&session,"review_start",json!({"operation_id":op,"idempotency_key":op,"model":"qwen3:test-only","text":"Copper is conductive.","consent":true})).await;
    assert_eq!(start["ok"], true);
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        async { while waiting.calls.load(Ordering::SeqCst) < 2 { waiting.entered.notified().await; } },
    )
    .await
    .unwrap();
    let mut runtime = f.state.node.lock().await.detach_base_runtime().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(3), runtime.close())
        .await
        .unwrap()
        .unwrap();
    drop(runtime);
    let resumed = proposal(false, false);
    let mut runtime = base_runtime_config_for_api_token(TOKEN);
    runtime.ku = Some(KuRuntimeConfig {
        vault_key: VaultKey::from_bytes([8; 32]),
        registry: Some(registry.clone()),
        inputs: inputs(registry, Some(resumed.clone())),
        public: None,
    });
    f.state
        .node
        .lock()
        .await
        .install_base_runtime(runtime)
        .unwrap();
    let fresh = f.session().await;
    let (_, view) = edit(&f, &fresh, "review_get", json!({"operation_id":op})).await;
    let job = &view["data"]["payload"]["review_job"]["job"];
    assert_eq!(job["state"], "interrupted", "{view}");
    assert_eq!(job["calls"], 2);
    assert!(job["charged_ms"].as_u64().unwrap() >= 600000);
    assert_eq!(job["windows"][0]["revisions"].as_array().unwrap().len(), 1);
    let (_, list) = edit(&f, &fresh, "review_list", json!({})).await;
    assert_eq!(
        list["data"]["payload"]["review_jobs"][0]["operation_id"],
        op
    );
    assert_eq!(resumed.calls.load(Ordering::SeqCst), 0);
    let (code, result) = edit(&f, &fresh, "review_resume", json!({"operation_id":op})).await;
    assert_eq!(code, StatusCode::OK, "{result}");
    let view = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let (_, view) = edit(&f, &fresh, "review_get", json!({"operation_id":op})).await;
            if view["data"]["payload"]["review_job"]["job"]["state"] == "draft_extracted" {
                break view;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let job = &view["data"]["payload"]["review_job"]["job"];
    assert_eq!(job["calls"], 3);
    assert!(job["charged_ms"].as_u64().unwrap() >= 600000);
    assert_eq!(resumed.calls.load(Ordering::SeqCst), 1);
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
async fn experimental_concept_label_failure_exposes_grounding_diagnostics() {
    let mut provider = proposal(false, false);
    Arc::get_mut(&mut provider).unwrap().bad_label = true;
    let (f, _, _) = fixture(provider.clone()).await;
    let session = f.session().await;
    let (_, template) = intake(&f, &session).await;
    let (code, failure) = f.invoke(&session, "prepare", template).await;
    assert_ne!(code, StatusCode::OK);
    assert_eq!(
        failure["error"]["failure"]["limitations"][0],
        "concept_label"
    );
    assert!(failure["error"]["failure"]["limitations"]
        .to_string()
        .contains("grounding: $.concepts[0].label: concept_label"));
    assert!(!failure.to_string().contains("PRIVATE_DIAGNOSTIC_SENTINEL"));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        f.invoke(&session, "list", json!({"limit":20})).await.1["data"]["payload"]["items"],
        json!([])
    );
}

#[tokio::test]
async fn experimental_invalid_output_cannot_save_and_cancel_stops_pending_extraction() {
    let provider = proposal(true, false);
    let (f, _, _) = fixture(provider.clone()).await;
    let session = f.session().await;
    let (op, template) = intake(&f, &session).await;
    let (code, failure) = f.invoke(&session, "prepare", template).await;
    assert_ne!(code, StatusCode::OK);
    assert_eq!(failure["error"]["failure"]["limitations"][0], "oneof");
    assert!(failure["error"]["failure"]["limitations"]
        .to_string()
        .contains("$.statements[0].arguments[0].unit: missing_field"));
    assert!(!failure.to_string().contains("PRIVATE_DIAGNOSTIC_SENTINEL"));
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
