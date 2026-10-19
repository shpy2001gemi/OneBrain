//! ARCH-07: an explicitly reviewed simple AI review-draft claim becomes a
//! canonical private KU only through lowering → prepare → preview → save.
//! Deterministic selection double; no real model or holdout is used.
use super::*;
use ku_encoder::extraction::{
    ExtractionError, ExtractionProvider, ExtractionWorkflow, ProviderRequest,
};
use onebrain_node::{ku_manual::ManualKuInputs, ku_ollama::OllamaKuInputs};

struct Selection {
    manifest: Value,
    selection: Value,
}
#[async_trait::async_trait]
impl ExtractionProvider for Selection {
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
        let output = match request.kind {
            ku_encoder::extraction::review_draft::TaskKind::Select => self.selection.clone(),
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
    async fn extract(&self, _: ProviderRequest) -> Result<Vec<u8>, ExtractionError> {
        Err(ExtractionError("unexpected_test_task"))
    }
}

async fn fixture(selection: Value) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let registry = registry_fixture::registry_for_entries(
        dir.path(),
        &[
            ([1; 16], 0, "copper", &["Copper"]),
            ([2; 16], 1, "conducts", &["conducts"]),
            ([3; 16], 0, "electricity", &["electricity"]),
            ([4; 16], 0, "bridge", &["cây cầu"]),
            ([5; 16], 1, "length", &["dài"]),
            ([6; 16], 2, "metre", &["mét"]),
            ([10; 16], 2, "mile", &["dặm"]),
        ],
    );
    let provider = Arc::new(Selection {
        manifest: json!({"profile":"ku-extraction-provider/1.0","provider_id":"experimental-ollama-qwen3:test-only-selection","backend_build_sha256":"01".repeat(32),
        "mode":"json_schema","tools_enabled":false,"max_context_tokens":8192,"peak_bytes_reservation":4096,
        "schema_bundle_sha256":ExtractionWorkflow::bundle_hash(),"model_artifact_sha256":"02".repeat(32),"tokenizer_sha256":"03".repeat(32),"supported_schema_keywords":[],"temperature_milli":0,"seed":1}),
        selection,
    });
    let config = NodeConfig {
        data_dir: dir.path().join("node"),
        concept_registry_mode: onebrain_node::ConceptRegistryMode::Disabled,
        ..Default::default()
    };
    std::fs::create_dir_all(&config.data_dir).unwrap();
    let mut node = OneBrainNode::new(config).await.unwrap();
    let manual = Arc::new(ManualKuInputs::new([0; 32], registry.clone(), vec![]).unwrap());
    let inputs = OllamaKuInputs::new(
        [0; 32],
        manual,
        registry.clone(),
        vec![(
            "qwen3:test-only".into(),
            provider as Arc<dyn ExtractionProvider>,
            4096,
        )],
    )
    .unwrap();
    let mut runtime = base_runtime_config_for_api_token(TOKEN);
    runtime.ku = Some(KuRuntimeConfig {
        vault_key: VaultKey::from_bytes([8; 32]),
        registry: Some(registry.clone()),
        inputs: Arc::new(inputs),
        public: None,
    });
    node.install_base_runtime(runtime).unwrap();
    let server = ApiServer::new(node, TOKEN.into(), 0);
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
    }
}

/// Start a private review job and wait until it is idle with a draft.
async fn reviewed_job(f: &Fixture, session: &Value, text: &str) -> String {
    let op = f.reserve(session).await;
    let payload = json!({"operation_id":op,"idempotency_key":op,"model":"qwen3:test-only","text":text,"consent":true});
    let (code, started) = edit(f, session, "review_start", payload).await;
    assert_eq!(code, StatusCode::OK, "{started}");
    let view = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let (_, v) = edit(f, session, "review_get", json!({"operation_id":op})).await;
            let job = &v["data"]["payload"]["review_job"]["job"];
            if job["active"].is_null()
                && !matches!(
                    job["state"].as_str(),
                    Some("queued" | "extracting" | "reviewing" | "repairing")
                )
            {
                break v;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let draft =
        &view["data"]["payload"]["review_job"]["job"]["windows"][0]["revisions"][0]["draft"];
    assert_eq!(draft["statements"].as_array().unwrap().len(), 1, "{view}");
    op
}

async fn lower(
    f: &Fixture,
    session: &Value,
    review: &str,
    reviewed: bool,
) -> (String, StatusCode, Value) {
    let op = f.reserve(session).await;
    let (code, value) = edit(
        f,
        session,
        "review_lower",
        json!({"operation_id":op,"idempotency_key":op,"review_operation_id":review,"window":0,"statement":0,"reviewed":reviewed}),
    )
    .await;
    (op, code, value)
}

async fn save_lowered(f: &Fixture, session: &Value, text: &str) -> Value {
    let review = reviewed_job(f, session, text).await;
    let (_, code, refused) = lower(f, session, &review, false).await;
    assert_eq!(code, StatusCode::BAD_REQUEST, "{refused}");
    let (op, code, lowered) = lower(f, session, &review, true).await;
    assert_eq!(code, StatusCode::OK, "{lowered}");
    let result = &lowered["data"]["payload"]["review_lowering"];
    assert_eq!(result["saveable"], true, "{lowered}");
    let preparation = result["preparation"].clone();
    assert_eq!(preparation["input_mode"], "resolved_semantic_draft");
    // Lowering alone stores nothing; save stays an explicit later step.
    let (_, page) = f.invoke(session, "list", json!({"limit":20})).await;
    let before = page["data"]["payload"]["items"].as_array().unwrap().len();
    let (code, prepared) = f.invoke(session, "prepare", preparation.clone()).await;
    assert_eq!(code, StatusCode::OK, "{prepared}");
    assert_eq!(
        prepared["data"]["payload"]["validity"], "ready",
        "{prepared}"
    );
    assert_eq!(prepared["data"]["model_qualified"], false);
    let ids = prepared["data"]["payload"]["object_cids"].clone();
    assert_eq!(ids.as_array().unwrap().len(), 1);
    let (_, preview) = f
        .invoke(session, "preview", json!({"operation_id":op}))
        .await;
    assert_eq!(preview["data"]["payload"], prepared["data"]["payload"]);
    let (code, saved) = f
        .invoke(
            session,
            "save",
            json!({"operation_id":op,"idempotency_key":op,"object_cids":ids}),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{saved}");
    assert_eq!(saved["data"]["payload"]["state"], "committed");
    assert_eq!(saved["data"]["payload"]["published"], false);
    let (code, view) = f.invoke(session, "get", json!({"object_cid":ids[0]})).await;
    assert_eq!(code, StatusCode::OK, "{view}");
    assert_eq!(view["data"]["payload"]["disclosure_class"], "LOCAL_ONLY");
    assert_eq!(
        view["data"]["payload"]["canonical_bytes"],
        prepared["data"]["payload"]["artifacts"][0]["canonical_preview"]
    );
    let (_, page) = f.invoke(session, "list", json!({"limit":20})).await;
    assert_eq!(
        page["data"]["payload"]["items"].as_array().unwrap().len(),
        before + 1
    );
    view
}

#[tokio::test]
async fn english_concept_predicate_concept_review_is_saved_as_canonical_ku() {
    let f = fixture(
        json!({"claims":[{"subject":"Copper","predicate":"conducts","arguments":["electricity"]}]}),
    )
    .await;
    let session = f.session().await;
    save_lowered(&f, &session, "Copper conducts electricity.").await;
}

#[tokio::test]
async fn vietnamese_quantity_review_is_saved_as_canonical_ku() {
    let f = fixture(json!({"claims":[{"subject":"Cây cầu","predicate":"dài","quantities":[{"quote":"120 mét","unit":"mét"}]}]})).await;
    let session = f.session().await;
    save_lowered(&f, &session, "Cây cầu dài 120 mét.").await;
}

#[tokio::test]
async fn unresolved_review_terms_stay_unsaveable() {
    for (text, selection, role) in [
        (
            "Copper conducts heat.",
            json!({"claims":[{"subject":"Copper","predicate":"conducts","arguments":["heat"]}]}),
            "object",
        ),
        (
            "Cây cầu dài 3 dặm.",
            json!({"claims":[{"subject":"Cây cầu","predicate":"dài","quantities":[{"quote":"3 dặm","unit":"dặm"}]}]}),
            "unit",
        ),
    ] {
        let f = fixture(selection).await;
        let session = f.session().await;
        let review = reviewed_job(&f, &session, text).await;
        let (op, code, lowered) = lower(&f, &session, &review, true).await;
        assert_eq!(code, StatusCode::OK, "{lowered}");
        let result = &lowered["data"]["payload"]["review_lowering"];
        assert_eq!(result["saveable"], false, "{lowered}");
        assert!(result.get("preparation").is_none());
        assert_eq!(result["unresolved"][0]["role"], role);
        // No lowered draft exists, so no preparation can resolve for this operation.
        let (code, _) = f
            .invoke(
                &session,
                "save",
                json!({"operation_id":op,"idempotency_key":op,"object_cids":["11".repeat(32)]}),
            )
            .await;
        assert_ne!(code, StatusCode::OK);
        let (_, page) = f.invoke(&session, "list", json!({"limit":20})).await;
        assert_eq!(page["data"]["payload"]["items"], json!([]));
    }
}
