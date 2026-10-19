use super::*;
use crate::{BaseHostAuthorizer, BaseRuntime, DatasetGenerationStore, NodeConfig, OneBrainNode};
use ku_core::foundation::semantic::{
    LiteralValue, SourceSpan, StatementFrame, StatementId, StatementQualifiers, TermRef,
};
use ku_core::foundation::{
    NormalizedText, ObjectReference, ObservationGovernance, SourceArtifactKind,
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const PRINCIPAL: [u8; 32] = [9; 32];
struct Auth;
impl BaseHostAuthorizer for Auth {
    fn authenticate(&self, principal: [u8; 32], proof: &[u8]) -> bool {
        (principal == PRINCIPAL || principal == [10; 32]) && proof == b"host-proof"
    }
}

struct Inputs {
    allowed: AtomicBool,
    calls: AtomicUsize,
    unresolved: AtomicBool,
    /// ARCH-08: drafts carry only a CCID argument (no text literal).
    concept_only: AtomicBool,
    source: Vec<u8>,
    cid: SourceArtifactCID,
}

impl Inputs {
    fn new() -> Self {
        Self::with_text("Exact PRIVATE source: water\r\n")
    }
    fn with_text(text: &str) -> Self {
        let reference = |id| ObjectReference::new(1, [id; 32]);
        let source = SourceArtifact {
            source_kind: SourceArtifactKind::Text,
            raw_bytes: text.as_bytes().to_vec(),
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
            allowed: AtomicBool::new(true),
            calls: AtomicUsize::new(0),
            unresolved: AtomicBool::new(false),
            concept_only: AtomicBool::new(false),
            source,
            cid: SourceArtifactCID(cid.into_bytes()),
        }
    }
}

impl KuInputProvider for Inputs {
    fn implementation(&self, mode: InputMode) -> Option<[u8; 32]> {
        match mode {
            InputMode::ResolvedSemanticDraft => Some([4; 32]),
            InputMode::LocalRule => Some([5; 32]),
            InputMode::LocalAi => None,
        }
    }
    fn check_access(
        &self,
        principal: [u8; 32],
        sources: &[[u8; 32]],
    ) -> Result<(), BaseServiceError> {
        if self.allowed.load(Ordering::SeqCst) && principal == PRINCIPAL && sources == [self.cid.0]
        {
            Ok(())
        } else {
            Err(not_found())
        }
    }
    fn resolve(
        &self,
        principal: [u8; 32],
        request: &KuPrepareV1,
        _registry: &ConceptRegistryReaderLease,
        _budget: &ResourceBudgetV1,
    ) -> Result<KuResolvedInput, BaseServiceError> {
        self.check_access(principal, &[self.cid.0])?;
        self.calls.fetch_add(1, Ordering::SeqCst);
        // Two controlled encoder outputs, deliberately using different local binder IDs.
        let version = request.draft_ref.map(|v| v.0[0]).unwrap_or(1);
        let drafts = (0..2)
            .map(|i| SemanticFrameSet {
                statements: vec![StatementFrame {
                    statement_id: StatementId(900 + i),
                    operator_or_predicate: ku_core::foundation::semantic::ConceptCcid::from_bytes(
                        [7; 16],
                    ),
                    arguments: if self.concept_only.load(Ordering::SeqCst) {
                        // Arity keeps the two concept-only drafts distinct.
                        let water = ku_core::foundation::semantic::ConceptCcid::from_bytes([7; 16]);
                        vec![TermRef::Concept(water); i as usize + 1]
                    } else {
                        vec![TermRef::Literal(LiteralValue::Text(
                            NormalizedText::new(format!("water version {version} output {i}"))
                                .unwrap(),
                        ))]
                    },
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
            extraction_budget: None,
            needs_resolution: false,
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
        })
    }
}

fn registry(root: &std::path::Path) -> Arc<ConceptRegistryGenerationManager> {
    let signer = if root.join("registry/releases/registry-v1").exists() {
        ed25519_dalek::SigningKey::from_bytes(&[0x42; 32])
    } else {
        crate::concept_registry_runtime::tests::install_signed_release(root)
    };
    let config = NodeConfig {
        data_dir: root.into(),
        concept_registry_mode: crate::config::ConceptRegistryMode::Required,
        concept_registry_release_root: Some(root.join("registry")),
        concept_registry_release_public_key: Some(hex(signer.verifying_key().as_bytes())),
        ..Default::default()
    };
    Arc::new(ConceptRegistryGenerationManager::open(config).unwrap())
}

impl crate::ku_extraction::KuExtractionSources for Inputs {
    fn check_access(
        &self,
        principal: [u8; 32],
        sources: &[[u8; 32]],
    ) -> Result<(), BaseServiceError> {
        KuInputProvider::check_access(self, principal, sources)
    }
    fn read_source(
        &self,
        principal: [u8; 32],
        source: [u8; 32],
        max_bytes: usize,
    ) -> Result<Vec<u8>, BaseServiceError> {
        KuInputProvider::check_access(self, principal, &[source])?;
        if self.source.len() > max_bytes {
            return Err(resource());
        }
        Ok(self.source.clone())
    }
}

struct FixtureInference {
    manifest: serde_json::Value,
    calls: AtomicUsize,
    pending: bool,
    entered: tokio::sync::Notify,
}
impl FixtureInference {
    fn new(pending: bool) -> Self {
        Self {
            manifest: serde_json::json!({"profile":"ku-extraction-provider/1.0","provider_id":"node-fixture","backend_build_sha256":"aa".repeat(32),"mode":"json_schema","tools_enabled":false,"max_context_tokens":8192,"peak_bytes_reservation":16777216,"schema_bundle_sha256":ku_encoder::extraction::ExtractionWorkflow::bundle_hash(),"model_artifact_sha256":"bb".repeat(32),"tokenizer_sha256":"cc".repeat(32),"supported_schema_keywords":["type"],"temperature_milli":0}),
            calls: AtomicUsize::new(0),
            pending,
            entered: tokio::sync::Notify::new(),
        }
    }
}
#[async_trait::async_trait]
impl ku_encoder::extraction::ExtractionProvider for FixtureInference {
    fn manifest(&self) -> &serde_json::Value {
        &self.manifest
    }
    fn input_tokens(
        &self,
        _: &ku_encoder::extraction::ProviderRequest,
    ) -> Result<u32, ku_encoder::extraction::ExtractionError> {
        Ok(100)
    }
    async fn extract(
        &self,
        request: ku_encoder::extraction::ProviderRequest,
    ) -> Result<Vec<u8>, ku_encoder::extraction::ExtractionError> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        self.entered.notify_one();
        if self.pending {
            std::future::pending::<()>().await;
        }
        let input = request.input;
        let unit = &input["required_units"][0];
        let option = input["options"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["lookup_label"] == "water")
            .unwrap();
        let candidate = serde_json::json!({"profile":"ku-extraction/1.0","attempt_id":input["attempt_id"],"context_sha256":input["context_sha256"],
            "concepts":[{"key":"p","label":"water","evidence":option["mention"]}],
            "statements":[{"key":"s","predicate":"p","arguments":[{"kind":"text","value":unit["span"]["quote"],"evidence":unit["span"]}],"evidence":[unit["span"]],"negation":{"value":false,"evidence":[]},"modality":{"value":"asserted","evidence":[]}}],
            "coverage":[{"unit":unit["key"],"status":"represented","statements":["s"],"reason":"none"}]});
        Ok(serde_json::to_vec(&candidate).unwrap())
    }
}

#[tokio::test]
async fn shared_model_adapter_uses_signed_registry_and_durable_exact_preparation() {
    let temp = tempfile::tempdir().unwrap();
    let reg = registry(temp.path());
    let source = Arc::new(Inputs::with_text("water"));
    let provider = Arc::new(FixtureInference::new(false));
    let workflow = Arc::new(
        ku_encoder::extraction::ExtractionWorkflow::new(provider.clone(), 16 * 1024 * 1024)
            .unwrap(),
    );
    let inputs = Arc::new(
        crate::ku_extraction::SharedKuExtractionInputs::new(
            source.clone(),
            workflow,
            InputMode::LocalAi,
            "constrained",
        )
        .unwrap(),
    );
    let commitment = inputs.implementation(InputMode::LocalAi).unwrap();
    let rt = runtime(temp.path(), inputs.clone(), Some(reg.clone()), 11).unwrap();
    let service = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
    let mut r = request(service.reserve().await.unwrap(), &source, &reg, 1);
    r.input_mode = InputMode::LocalAi;
    r.draft_ref = None;
    r.implementation_commitment = ImplementationCommitment(commitment);
    let KuResponseV1::Prepare(prepared) = service
        .invoke(KuRequestV1::Prepare(r.clone()), budget())
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(prepared.validity, Validity::Ready);
    assert_eq!(provider.calls.load(Ordering::Acquire), 1);
    assert!(list(&service).await.items.is_empty());
    drop(rt);
    let rt = runtime(temp.path(), inputs, Some(reg), 11).unwrap();
    let service = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
    let KuResponseV1::Prepare(replayed) = service
        .invoke(KuRequestV1::Prepare(r.clone()), budget())
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(prepared, replayed);
    assert_eq!(provider.calls.load(Ordering::Acquire), 1);
    assert_eq!(
        save(&service, &r, &prepared).await.state,
        BaseState::Committed
    );
}

#[tokio::test]
async fn cancel_reaches_running_model_without_waiting_for_service_mutation_lock() {
    let temp = tempfile::tempdir().unwrap();
    let reg = registry(temp.path());
    let source = Arc::new(Inputs::with_text("water"));
    let provider = Arc::new(FixtureInference::new(true));
    let workflow = Arc::new(
        ku_encoder::extraction::ExtractionWorkflow::new(provider.clone(), 16 * 1024 * 1024)
            .unwrap(),
    );
    let inputs = Arc::new(
        crate::ku_extraction::SharedKuExtractionInputs::new(
            source.clone(),
            workflow,
            InputMode::LocalAi,
            "constrained",
        )
        .unwrap(),
    );
    let commitment = inputs.implementation(InputMode::LocalAi).unwrap();
    let rt = runtime(temp.path(), inputs.clone(), Some(reg.clone()), 11).unwrap();
    let service = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
    let mut r = request(service.reserve().await.unwrap(), &source, &reg, 1);
    r.input_mode = InputMode::LocalAi;
    r.draft_ref = None;
    r.implementation_commitment = ImplementationCommitment(commitment);
    let prep = service.invoke(KuRequestV1::Prepare(r.clone()), budget());
    let cancel = async {
        provider.entered.notified().await;
        service
            .invoke(
                KuRequestV1::Cancel(KuOperationRefV1 {
                    operation_id: r.operation_id,
                }),
                budget(),
            )
            .await
    };
    let (prep, cancel) = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        tokio::join!(prep, cancel)
    })
    .await
    .unwrap();
    assert!(prep.is_err());
    let KuResponseV1::Cancel(receipt) = cancel.unwrap() else {
        panic!()
    };
    assert_eq!(receipt.state, BaseState::Canceled);
    assert!(list(&service).await.items.is_empty());
    assert_eq!(provider.calls.load(Ordering::Acquire), 1);
    drop(rt);
    let rt = runtime(temp.path(), inputs, Some(reg), 11).unwrap();
    let service = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
    assert!(service
        .invoke(KuRequestV1::Prepare(r), budget())
        .await
        .is_err());
    assert_eq!(provider.calls.load(Ordering::Acquire), 1);
}

#[tokio::test]
async fn shared_no_llm_encoder_prepares_exact_sem_then_saves_via_existing_vault_only() {
    use crate::ku_extraction::SharedKuExtractionInputs;
    use ku_encoder::extraction::{ExtractionWorkflow, NoLlmProvider};
    let temp = tempfile::tempdir().unwrap();
    let reg = registry(temp.path());
    let source = Arc::new(Inputs::with_text(
        "@ku1 (\"PRIVATE left\") [water] (\"PRIVATE right\")",
    ));
    let workflow = Arc::new(
        ExtractionWorkflow::new(Arc::new(NoLlmProvider::new()), 16 * 1024 * 1024).unwrap(),
    );
    let inputs = Arc::new(
        SharedKuExtractionInputs::new(source.clone(), workflow, InputMode::LocalRule, "no_llm")
            .unwrap(),
    );
    let commitment = inputs.implementation(InputMode::LocalRule).unwrap();
    let rt = runtime(temp.path(), inputs.clone(), Some(reg.clone()), 11).unwrap();
    let service = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
    let mut r = request(service.reserve().await.unwrap(), &source, &reg, 1);
    r.input_mode = InputMode::LocalRule;
    r.draft_ref = None;
    r.implementation_commitment = ImplementationCommitment(commitment);
    let KuResponseV1::Prepare(p) = service
        .invoke(KuRequestV1::Prepare(r.clone()), budget())
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(p.validity, Validity::Ready);
    assert_eq!(p.artifacts.len(), 1);
    assert!(list(&service).await.items.is_empty());
    let KuResponseV1::Prepare(replay) = service
        .invoke(KuRequestV1::Prepare(r.clone()), budget())
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(replay, p);
    let receipt = save(&service, &r, &p).await;
    assert_eq!(receipt.state, BaseState::Committed);
    assert!(!receipt.published && !receipt.authorizes_reward);
    drop(rt);
    // Same encryption key reopens both prepared and extraction journals. Existing
    // KU results remain readable even when the source grant has been revoked.
    source.allowed.store(false, Ordering::Release);
    let rt = runtime(temp.path(), inputs, None, 11).unwrap();
    let service = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
    assert_eq!(list(&service).await.items.len(), 1);
    drop(rt);
    let mut files = vec![temp.path().to_path_buf()];
    let mut checked = 0;
    while let Some(path) = files.pop() {
        if path.is_dir() {
            for entry in std::fs::read_dir(path).unwrap() {
                files.push(entry.unwrap().path());
            }
        } else if path.file_name().is_some_and(|n| n == "journal.redb") {
            let raw = std::fs::read(path).unwrap();
            assert!(!raw
                .windows(b"PRIVATE left".len())
                .any(|w| w == b"PRIVATE left"));
            checked += 1;
        }
    }
    assert!(checked > 0);
}

#[tokio::test]
async fn shared_no_llm_arbitrary_prose_has_no_artifacts_and_cannot_save() {
    use ku_encoder::extraction::{ExtractionWorkflow, NoLlmProvider};
    let temp = tempfile::tempdir().unwrap();
    let reg = registry(temp.path());
    let source = Arc::new(Inputs::with_text("water is clear"));
    let inputs = Arc::new(
        crate::ku_extraction::SharedKuExtractionInputs::new(
            source.clone(),
            Arc::new(
                ExtractionWorkflow::new(Arc::new(NoLlmProvider::new()), 16 * 1024 * 1024).unwrap(),
            ),
            InputMode::LocalRule,
            "no_llm",
        )
        .unwrap(),
    );
    let commitment = inputs.implementation(InputMode::LocalRule).unwrap();
    let rt = runtime(temp.path(), inputs, Some(reg.clone()), 11).unwrap();
    let service = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
    let mut r = request(service.reserve().await.unwrap(), &source, &reg, 1);
    r.input_mode = InputMode::LocalRule;
    r.draft_ref = None;
    r.implementation_commitment = ImplementationCommitment(commitment);
    let KuResponseV1::Prepare(p) = service
        .invoke(KuRequestV1::Prepare(r.clone()), budget())
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(p.validity, Validity::NeedsResolution);
    assert!(p.object_cids.is_empty() && p.artifacts.is_empty());
    assert!(service
        .invoke(
            KuRequestV1::Save(KuSaveV1 {
                operation_id: r.operation_id,
                idempotency_key: r.idempotency_key,
                object_cids: vec![]
            }),
            budget()
        )
        .await
        .is_err());
    assert!(list(&service).await.items.is_empty());
}

fn runtime(
    root: &std::path::Path,
    input: Arc<dyn KuInputProvider>,
    registry: Option<Arc<ConceptRegistryGenerationManager>>,
    key: u8,
) -> Result<BaseRuntime, BaseServiceError> {
    let generations =
        Arc::new(DatasetGenerationStore::open_exclusive(&root.join("dataset")).unwrap());
    let mut config = crate::compiled_base_runtime_config();
    config.host_authorizer = Arc::new(Auth);
    config.ku = Some(KuRuntimeConfig {
        vault_key: VaultKey::from_bytes([key; 32]),
        registry,
        inputs: input,
        public: None,
    });
    BaseRuntime::open(generations, config)
}

fn budget() -> ResourceBudgetV1 {
    ResourceBudgetV1 {
        max_items: 256,
        max_bytes: 1048576,
        max_work_units: 1000000,
    }
}
fn request(
    op: OperationId,
    input: &Inputs,
    registry: &ConceptRegistryGenerationManager,
    version: u8,
) -> KuPrepareV1 {
    KuPrepareV1 {
        operation_id: op,
        idempotency_key: IdempotencyKey(op.0),
        input_mode: InputMode::ResolvedSemanticDraft,
        source_refs: vec![input.cid],
        registry_release_root: ReleaseRoot(
            decode_hex(
                registry
                    .reader_lease()
                    .status()
                    .release_aggregate_root
                    .as_ref()
                    .unwrap(),
            )
            .unwrap(),
        ),
        semantic_profile: SEMANTIC_CONTENT_PROFILE.into(),
        implementation_commitment: ImplementationCommitment([4; 32]),
        destination: Disclosure::LOCALONLY,
        draft_ref: Some(ObjectCID([version; 32])),
    }
}
async fn prepare(
    service: &KuServices,
    input: &Inputs,
    registry: &ConceptRegistryGenerationManager,
    version: u8,
) -> (KuPrepareV1, KuPreparedV1) {
    let r = request(service.reserve().await.unwrap(), input, registry, version);
    let KuResponseV1::Prepare(p) = service
        .invoke(KuRequestV1::Prepare(r.clone()), budget())
        .await
        .unwrap()
    else {
        panic!()
    };
    (r, p)
}
async fn save(service: &KuServices, r: &KuPrepareV1, p: &KuPreparedV1) -> KuReceiptV1 {
    let KuResponseV1::Save(receipt) = service
        .invoke(
            KuRequestV1::Save(KuSaveV1 {
                operation_id: r.operation_id,
                idempotency_key: r.idempotency_key,
                object_cids: p.object_cids.clone(),
            }),
            budget(),
        )
        .await
        .unwrap()
    else {
        panic!()
    };
    receipt
}
async fn list(service: &KuServices) -> KuPageV1 {
    let KuResponseV1::List(page) = service
        .invoke(
            KuRequestV1::List(KuListV1 {
                limit: 256,
                continuation: None,
            }),
            budget(),
        )
        .await
        .unwrap()
    else {
        panic!()
    };
    page
}

#[tokio::test]
async fn offline_node_owned_bundle_replays_and_survives_restart_without_registry_or_encoder() {
    let temp = tempfile::tempdir().unwrap();
    let reg = registry(temp.path());
    let input = Arc::new(Inputs::new());
    let rt = runtime(temp.path(), input.clone(), Some(reg.clone()), 11).unwrap();
    let service = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
    assert!(rt.ku_services(PRINCIPAL, b"wrong").is_err());
    let (r, p) = prepare(&service, &input, &reg, 1).await;
    assert_eq!(p.artifacts.len(), 2);
    assert!(list(&service).await.items.is_empty());
    let first = save(&service, &r, &p).await;
    assert_eq!(save(&service, &r, &p).await, first);
    assert!(!first.published && !first.authorizes_reward);
    let other = rt.ku_services([10; 32], b"host-proof").unwrap();
    assert!(list(&other).await.items.is_empty());
    let error = other
        .invoke(
            KuRequestV1::Get(KuGetV1 {
                object_cid: p.object_cids[0],
            }),
            budget(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, BaseErrorCodeV1::NotFound);
    drop(rt);
    assert!(service.reserve().await.is_err());
    let empty_input = Arc::new(Inputs::new());
    empty_input.allowed.store(false, Ordering::SeqCst);
    let rt = runtime(temp.path(), empty_input.clone(), None, 11).unwrap();
    let service = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
    let KuResponseV1::Reconcile(restored) = service
        .invoke(
            KuRequestV1::Reconcile(KuOperationRefV1 {
                operation_id: r.operation_id,
            }),
            budget(),
        )
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(restored, first);
    let KuResponseV1::Get(view) = service
        .invoke(
            KuRequestV1::Get(KuGetV1 {
                object_cid: p.object_cids[0],
            }),
            budget(),
        )
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(view.canonical_bytes, p.artifacts[0].canonical_preview);
    assert_eq!(list(&service).await.items.len(), 2);
    assert_eq!(empty_input.calls.load(Ordering::SeqCst), 0);
    drop(rt);
    assert_eq!(
        runtime(temp.path(), Arc::new(Inputs::new()), None, 12)
            .err()
            .unwrap()
            .code,
        BaseErrorCodeV1::CorruptState
    );
}

#[tokio::test]
async fn prepared_restart_uses_exact_staging_and_fails_closed_on_missing_pinned_release() {
    let temp = tempfile::tempdir().unwrap();
    let reg = registry(temp.path());
    let input = Arc::new(Inputs::new());
    let rt = runtime(temp.path(), input.clone(), Some(reg.clone()), 11).unwrap();
    let service = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
    let (r, p) = prepare(&service, &input, &reg, 1).await;
    drop(rt);
    let fresh = Arc::new(Inputs::new());
    let rt = runtime(temp.path(), fresh.clone(), None, 11).unwrap();
    let service = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
    let KuResponseV1::Preview(restored) = service
        .invoke(
            KuRequestV1::Preview(KuOperationRefV1 {
                operation_id: r.operation_id,
            }),
            budget(),
        )
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(restored, p);
    let e = service
        .invoke(
            KuRequestV1::Save(KuSaveV1 {
                operation_id: r.operation_id,
                idempotency_key: r.idempotency_key,
                object_cids: p.object_cids.clone(),
            }),
            budget(),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code, BaseErrorCodeV1::DependencyUnavailable);
    drop(rt);
    let rt = runtime(temp.path(), fresh.clone(), Some(reg), 11).unwrap();
    let service = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
    save(&service, &r, &p).await;
    assert_eq!(fresh.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn changed_reuse_source_revocation_cancel_and_unresolved_are_fenced() {
    let temp = tempfile::tempdir().unwrap();
    let reg = registry(temp.path());
    let input = Arc::new(Inputs::new());
    let rt = runtime(temp.path(), input.clone(), Some(reg.clone()), 11).unwrap();
    let s = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
    let (r, p) = prepare(&s, &input, &reg, 1).await;
    let KuResponseV1::Prepare(replayed) = s
        .invoke(KuRequestV1::Prepare(r.clone()), budget())
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(replayed, p);
    assert_eq!(input.calls.load(Ordering::SeqCst), 1);
    let mut changed = r.clone();
    changed.destination = Disclosure::NEGOTIATEDENCRYPTED;
    assert_eq!(
        s.invoke(KuRequestV1::Prepare(changed), budget())
            .await
            .unwrap_err()
            .code,
        BaseErrorCodeV1::Conflict
    );
    input.allowed.store(false, Ordering::SeqCst);
    assert_eq!(
        s.invoke(
            KuRequestV1::Save(KuSaveV1 {
                operation_id: r.operation_id,
                idempotency_key: r.idempotency_key,
                object_cids: p.object_cids.clone()
            }),
            budget()
        )
        .await
        .unwrap_err()
        .code,
        BaseErrorCodeV1::NotFound
    );
    assert!(list(&s).await.items.is_empty());
    input.allowed.store(true, Ordering::SeqCst);
    s.invoke(
        KuRequestV1::Cancel(KuOperationRefV1 {
            operation_id: r.operation_id,
        }),
        budget(),
    )
    .await
    .unwrap();
    assert_eq!(
        s.invoke(
            KuRequestV1::Save(KuSaveV1 {
                operation_id: r.operation_id,
                idempotency_key: r.idempotency_key,
                object_cids: p.object_cids
            }),
            budget()
        )
        .await
        .unwrap_err()
        .code,
        BaseErrorCodeV1::Conflict
    );
    input.unresolved.store(true, Ordering::SeqCst);
    let (_, p) = prepare(&s, &input, &reg, 2).await;
    assert_eq!(p.validity, Validity::NeedsResolution);
    assert!(p.artifacts.is_empty() && p.object_cids.is_empty());
}

#[tokio::test]
async fn snapshot_pagination_is_stable_context_bound_and_opaque() {
    let temp = tempfile::tempdir().unwrap();
    let reg = registry(temp.path());
    let input = Arc::new(Inputs::new());
    let rt = runtime(temp.path(), input.clone(), Some(reg.clone()), 11).unwrap();
    let s = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
    let (r, p) = prepare(&s, &input, &reg, 1).await;
    save(&s, &r, &p).await;
    let KuResponseV1::List(first) = s
        .invoke(
            KuRequestV1::List(KuListV1 {
                limit: 1,
                continuation: None,
            }),
            budget(),
        )
        .await
        .unwrap()
    else {
        panic!()
    };
    let (r, p) = prepare(&s, &input, &reg, 2).await;
    save(&s, &r, &p).await;
    let continuation = first.continuation.clone();
    let KuResponseV1::List(second) = s
        .invoke(
            KuRequestV1::List(KuListV1 {
                limit: 256,
                continuation: continuation.clone(),
            }),
            budget(),
        )
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(second.items.len(), 1);
    assert!(first.items[0].object_cid.0 < second.items[0].object_cid.0);
    assert_eq!(first.snapshot_frontier, second.snapshot_frontier);
    assert_eq!(list(&s).await.items.len(), 4);
    assert_eq!(
        s.invoke(
            KuRequestV1::Search(KuSearchV1 {
                query: "water".into(),
                limit: 1,
                continuation: continuation.clone()
            }),
            budget()
        )
        .await
        .unwrap_err()
        .code,
        BaseErrorCodeV1::Conflict
    );
    let other = rt.ku_services([10; 32], b"host-proof").unwrap();
    assert_eq!(
        other
            .invoke(
                KuRequestV1::List(KuListV1 {
                    limit: 1,
                    continuation
                }),
                budget()
            )
            .await
            .unwrap_err()
            .code,
        BaseErrorCodeV1::Conflict
    );
    let KuResponseV1::Search(found) = s
        .invoke(
            KuRequestV1::Search(KuSearchV1 {
                query: "version 1".into(),
                limit: 256,
                continuation: None,
            }),
            budget(),
        )
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(found.items.len(), 2);
    drop(rt);
    let rt = runtime(temp.path(), input, Some(reg), 11).unwrap();
    let s = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
    assert_eq!(
        s.invoke(
            KuRequestV1::List(KuListV1 {
                limit: 1,
                continuation: first.continuation
            }),
            budget()
        )
        .await
        .unwrap_err()
        .code,
        BaseErrorCodeV1::Expired
    );
}

#[tokio::test]
async fn revisions_preserve_predecessor_and_branches_and_reject_stale_frontier() {
    let temp = tempfile::tempdir().unwrap();
    let reg = registry(temp.path());
    let input = Arc::new(Inputs::new());
    let rt = runtime(temp.path(), input.clone(), Some(reg.clone()), 11).unwrap();
    let s = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
    let (r, p) = prepare(&s, &input, &reg, 1).await;
    save(&s, &r, &p).await;
    let predecessor = p.object_cids[0];
    let frontier = list(&s).await.snapshot_frontier;
    let a = request(s.reserve().await.unwrap(), &input, &reg, 2);
    let b = request(s.reserve().await.unwrap(), &input, &reg, 3);
    let revise = |r: KuPrepareV1, f| {
        KuRequestV1::Revise(KuReviseV1 {
            preparation: r,
            predecessor_object_cid: predecessor,
            expected_revision_frontier: f,
        })
    };
    let KuResponseV1::Revise(pa) = s
        .invoke(revise(a.clone(), frontier), budget())
        .await
        .unwrap()
    else {
        panic!()
    };
    let KuResponseV1::Revise(pb) = s
        .invoke(revise(b.clone(), frontier), budget())
        .await
        .unwrap()
    else {
        panic!()
    };
    save(&s, &a, &pa).await;
    assert_eq!(
        s.invoke(
            KuRequestV1::Save(KuSaveV1 {
                operation_id: b.operation_id,
                idempotency_key: b.idempotency_key,
                object_cids: pb.object_cids
            }),
            budget()
        )
        .await
        .unwrap_err()
        .code,
        BaseErrorCodeV1::Conflict
    );
    let c = request(s.reserve().await.unwrap(), &input, &reg, 3);
    let frontier = list(&s).await.snapshot_frontier;
    let KuResponseV1::Revise(pc) = s
        .invoke(revise(c.clone(), frontier), budget())
        .await
        .unwrap()
    else {
        panic!()
    };
    save(&s, &c, &pc).await;
    assert_eq!(list(&s).await.items.len(), 6);
    assert!(s
        .invoke(
            KuRequestV1::Get(KuGetV1 {
                object_cid: predecessor
            }),
            budget()
        )
        .await
        .is_ok());
}

#[tokio::test]
async fn onebrain_node_exposes_weak_authenticated_service_without_network_or_ai() {
    let temp = tempfile::tempdir().unwrap();
    let reg = registry(temp.path());
    let input = Arc::new(Inputs::new());
    std::fs::create_dir_all(temp.path().join("node")).unwrap();
    let mut node = OneBrainNode::new(NodeConfig {
        data_dir: temp.path().join("node"),
        concept_registry_mode: crate::config::ConceptRegistryMode::Disabled,
        ..Default::default()
    })
    .await
    .unwrap();
    let mut config = crate::compiled_base_runtime_config();
    config.host_authorizer = Arc::new(Auth);
    config.ku = Some(KuRuntimeConfig {
        vault_key: VaultKey::from_bytes([11; 32]),
        registry: Some(reg.clone()),
        inputs: input.clone(),
        public: None,
    });
    node.install_base_runtime(config).unwrap();
    let service = node.ku_services(PRINCIPAL, b"host-proof").unwrap();
    let owner = Arc::new(tokio::sync::Mutex::new(node));
    let _held = owner.lock().await;
    let (r, p) = prepare(&service, &input, &reg, 1).await;
    save(&service, &r, &p).await;
    assert_eq!(list(&service).await.items.len(), 2);
}

#[test]
fn crash_child_worker() {
    let Some(root) = std::env::var_os("ONEBRAIN_KU_TEST_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let reg = registry(&root);
    let input = Arc::new(Inputs::new());
    let rt = runtime(&root, input.clone(), Some(reg.clone()), 11).unwrap();
    let service = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let (r, p) = prepare(&service, &input, &reg, 1).await;
            std::fs::write(
                root.join("test_operation.json"),
                serde_json::to_vec(&r).unwrap(),
            )
            .unwrap();
            let phase = std::env::var("ONEBRAIN_KU_TEST_PHASE").unwrap();
            FAILPOINT.with(|p| *p.borrow_mut() = Some(format!("crash:{phase}")));
            save(&service, &r, &p).await;
            panic!("expected process kill");
        });
}

#[tokio::test]
async fn real_process_kills_at_every_save_boundary_reconcile_without_model_replay() {
    for phase in [
        "before_objects",
        "after_object_0",
        "after_object_1",
        "after_object_2",
        "before_commit_marker",
        "after_commit_marker",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "ku_product::tests::crash_child_worker",
                "--nocapture",
            ])
            .env("ONEBRAIN_KU_TEST_ROOT", temp.path())
            .env("ONEBRAIN_KU_TEST_PHASE", phase)
            .output()
            .unwrap();
        assert_eq!(
            status.status.code(),
            Some(86),
            "{phase}: {}",
            String::from_utf8_lossy(&status.stderr)
        );
        let r: KuPrepareV1 = serde_json::from_slice(
            &std::fs::read(temp.path().join("test_operation.json")).unwrap(),
        )
        .unwrap();
        let reg = registry(temp.path());
        let input = Arc::new(Inputs::new());
        let rt = runtime(temp.path(), input.clone(), Some(reg), 11).unwrap();
        let s = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
        assert_eq!(
            list(&s).await.items.len(),
            if phase == "after_commit_marker" { 2 } else { 0 },
            "partial product visibility at {phase}"
        );
        let KuResponseV1::Reconcile(receipt) = s
            .invoke(
                KuRequestV1::Reconcile(KuOperationRefV1 {
                    operation_id: r.operation_id,
                }),
                budget(),
            )
            .await
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(receipt.state, BaseState::Committed);
        assert_eq!(list(&s).await.items.len(), 2);
        assert_eq!(input.calls.load(Ordering::SeqCst), 0);
        drop(rt);
        let generations =
            DatasetGenerationStore::open_exclusive(&temp.path().join("dataset")).unwrap();
        let path = generations
            .current_resolver()
            .unwrap()
            .owner_path(BaseStorageOwnerId::VAULT)
            .unwrap()
            .join("ku-product-v1");
        for filename in ["journal.redb", "objects.redb"] {
            let bytes = std::fs::read(path.join(filename)).unwrap();
            assert!(
                !bytes
                    .windows(b"Exact PRIVATE".len())
                    .any(|w| w == b"Exact PRIVATE"),
                "plaintext in {filename}"
            );
        }
    }
}

#[test]
fn extraction_crash_child_worker() {
    let Some(root) = std::env::var_os("ONEBRAIN_EXTRACTION_TEST_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let reg = registry(&root);
            let source = Arc::new(Inputs::with_text("water"));
            let provider = Arc::new(FixtureInference::new(false));
            let workflow = Arc::new(
                ku_encoder::extraction::ExtractionWorkflow::new(provider, 16 * 1024 * 1024)
                    .unwrap(),
            );
            let inputs = Arc::new(
                crate::ku_extraction::SharedKuExtractionInputs::new(
                    source.clone(),
                    workflow,
                    InputMode::LocalAi,
                    "constrained",
                )
                .unwrap(),
            );
            let commitment = inputs.implementation(InputMode::LocalAi).unwrap();
            let rt = runtime(&root, inputs, Some(reg.clone()), 11).unwrap();
            let service = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
            let mut r = request(service.reserve().await.unwrap(), &source, &reg, 1);
            r.input_mode = InputMode::LocalAi;
            r.draft_ref = None;
            r.implementation_commitment = ImplementationCommitment(commitment);
            std::fs::write(
                root.join("extraction_operation.json"),
                serde_json::to_vec(&r).unwrap(),
            )
            .unwrap();
            let phase = std::env::var("ONEBRAIN_EXTRACTION_TEST_PHASE").unwrap();
            FAILPOINT.with(|p| *p.borrow_mut() = Some(format!("crash:{phase}")));
            service
                .invoke(KuRequestV1::Prepare(r), budget())
                .await
                .unwrap();
            panic!("expected process kill");
        });
}

#[tokio::test]
async fn extraction_process_kills_never_resample_or_expose_partial_artifacts() {
    for phase in [
        "after_extraction_reservation",
        "after_extraction_candidate",
        "after_extraction_validated",
        "after_extraction_bundle",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "ku_product::tests::extraction_crash_child_worker",
                "--nocapture",
            ])
            .env("ONEBRAIN_EXTRACTION_TEST_ROOT", temp.path())
            .env("ONEBRAIN_EXTRACTION_TEST_PHASE", phase)
            .output()
            .unwrap();
        assert_eq!(
            status.status.code(),
            Some(86),
            "{phase}: {}",
            String::from_utf8_lossy(&status.stderr)
        );
        let r: KuPrepareV1 = serde_json::from_slice(
            &std::fs::read(temp.path().join("extraction_operation.json")).unwrap(),
        )
        .unwrap();
        let reg = registry(temp.path());
        let source = Arc::new(Inputs::with_text("water"));
        let provider = Arc::new(FixtureInference::new(false));
        let workflow = Arc::new(
            ku_encoder::extraction::ExtractionWorkflow::new(provider.clone(), 16 * 1024 * 1024)
                .unwrap(),
        );
        let inputs = Arc::new(
            crate::ku_extraction::SharedKuExtractionInputs::new(
                source,
                workflow,
                InputMode::LocalAi,
                "constrained",
            )
            .unwrap(),
        );
        let rt = runtime(temp.path(), inputs, Some(reg), 11).unwrap();
        let service = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
        assert!(list(&service).await.items.is_empty());
        assert!(service
            .invoke(KuRequestV1::Prepare(r.clone()), budget())
            .await
            .is_err());
        assert!(service
            .invoke(
                KuRequestV1::Preview(KuOperationRefV1 {
                    operation_id: r.operation_id
                }),
                budget()
            )
            .await
            .is_err());
        let KuResponseV1::Reconcile(receipt) = service
            .invoke(
                KuRequestV1::Reconcile(KuOperationRefV1 {
                    operation_id: r.operation_id,
                }),
                budget(),
            )
            .await
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(receipt.state, BaseState::Failed, "{phase}");
        assert!(receipt.object_cids.is_empty());
        assert_eq!(provider.calls.load(Ordering::Acquire), 0);
        assert!(list(&service).await.items.is_empty());
    }
}

#[tokio::test]
async fn unknown_save_cannot_be_canceled_or_blindly_retried_and_revocation_keeps_reconcile_required(
) {
    let temp = tempfile::tempdir().unwrap();
    let reg = registry(temp.path());
    let input = Arc::new(Inputs::new());
    let rt = runtime(temp.path(), input.clone(), Some(reg.clone()), 11).unwrap();
    let s = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
    let (r, p) = prepare(&s, &input, &reg, 1).await;
    let save_request = KuRequestV1::Save(KuSaveV1 {
        operation_id: r.operation_id,
        idempotency_key: r.idempotency_key,
        object_cids: p.object_cids,
    });
    FAILPOINT.with(|p| *p.borrow_mut() = Some("after_object_0".into()));
    let error = s.invoke(save_request.clone(), budget()).await.unwrap_err();
    FAILPOINT.with(|p| *p.borrow_mut() = None);
    assert_eq!(error.code, BaseErrorCodeV1::UnknownOutcome);
    assert!(error.reconcile_before_retry);
    assert!(list(&s).await.items.is_empty());
    assert_eq!(
        s.base
            .invoke(onebrain_base_contract::BaseRequestV1::Cancel(
                onebrain_base_contract::BaseOperationId(r.operation_id.0)
            ))
            .await
            .err()
            .unwrap()
            .code,
        BaseErrorCodeV1::Conflict
    );
    assert_eq!(
        s.invoke(
            KuRequestV1::Cancel(KuOperationRefV1 {
                operation_id: r.operation_id
            }),
            budget()
        )
        .await
        .unwrap_err()
        .code,
        BaseErrorCodeV1::Conflict
    );
    assert_eq!(
        s.invoke(save_request, budget()).await.unwrap_err().code,
        BaseErrorCodeV1::UnknownOutcome
    );
    input.allowed.store(false, Ordering::SeqCst);
    let error = s
        .invoke(
            KuRequestV1::Reconcile(KuOperationRefV1 {
                operation_id: r.operation_id,
            }),
            budget(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, BaseErrorCodeV1::UnknownOutcome);
    assert!(error.reconcile_before_retry);
    input.allowed.store(true, Ordering::SeqCst);
    s.invoke(
        KuRequestV1::Reconcile(KuOperationRefV1 {
            operation_id: r.operation_id,
        }),
        budget(),
    )
    .await
    .unwrap();
    assert_eq!(list(&s).await.items.len(), 2);
    assert_eq!(input.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn admission_precedes_encoder_and_confirmation_and_local_rule_matches_resolved_draft() {
    let temp = tempfile::tempdir().unwrap();
    let reg = registry(temp.path());
    let input = Arc::new(Inputs::new());
    let rt = runtime(temp.path(), input.clone(), Some(reg.clone()), 11).unwrap();
    let s = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
    let r = request(s.reserve().await.unwrap(), &input, &reg, 1);
    let mut tiny = budget();
    tiny.max_bytes = 1;
    assert_eq!(
        s.invoke(KuRequestV1::Prepare(r.clone()), tiny)
            .await
            .unwrap_err()
            .code,
        BaseErrorCodeV1::ResourceExhausted
    );
    assert_eq!(input.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        s.invoke_payload(0xffff, b"{}", budget())
            .await
            .unwrap_err()
            .code,
        BaseErrorCodeV1::InvalidRequest
    );
    let KuResponseV1::Prepare(p) = s
        .invoke(KuRequestV1::Prepare(r.clone()), budget())
        .await
        .unwrap()
    else {
        panic!()
    };
    let save_request = KuRequestV1::Save(KuSaveV1 {
        operation_id: r.operation_id,
        idempotency_key: r.idempotency_key,
        object_cids: p.object_cids.clone(),
    });
    let projected = KuReceiptV1 {
        operation_id: r.operation_id,
        state: BaseState::Committed,
        object_cids: p.object_cids.clone(),
        limitations: vec!["fidelity_unassessed".into()],
        published: false,
        authorizes_reward: false,
    };
    let mut narrow = budget();
    narrow.max_bytes = projected.encode().unwrap().len() as u64 - 1;
    assert!(save_request.payload_bytes().unwrap().len() as u64 <= narrow.max_bytes);
    assert_eq!(
        s.invoke(save_request, narrow).await.unwrap_err().code,
        BaseErrorCodeV1::ResourceExhausted
    );
    assert!(list(&s).await.items.is_empty());
    let KuResponseV1::Status(status) = s
        .invoke(
            KuRequestV1::Status(KuStatusRequestV1 {
                operation_id: Some(r.operation_id),
            }),
            budget(),
        )
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(status.receipt.unwrap().state, BaseState::Prepared);
    let mut rule = request(s.reserve().await.unwrap(), &input, &reg, 1);
    rule.input_mode = InputMode::LocalRule;
    rule.draft_ref = None;
    rule.implementation_commitment = ImplementationCommitment([5; 32]);
    let KuResponseV1::Prepare(rule_preview) = s
        .invoke(KuRequestV1::Prepare(rule), budget())
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(p.artifacts, rule_preview.artifacts);
    let mut ai = request(s.reserve().await.unwrap(), &input, &reg, 1);
    ai.input_mode = InputMode::LocalAi;
    ai.draft_ref = None;
    assert_eq!(
        s.invoke(KuRequestV1::Prepare(ai), budget())
            .await
            .unwrap_err()
            .code,
        BaseErrorCodeV1::DependencyUnavailable
    );
    save(&s, &r, &p).await;
}

#[tokio::test]
async fn public_reader_exports_only_public_bytes_and_keeps_unknown_schema_opaque() {
    use crate::vnext_validated_sink::{SharedVNextValidatedSink, VNextValidatedSink};
    use ku_core::foundation::{AcceptedRecordEntry, InMemoryVerifiedBackend, StoredRecordKind};
    let temp = tempfile::tempdir().unwrap();
    let reg = registry(temp.path());
    let input = Arc::new(Inputs::new());
    let sink =
        SharedVNextValidatedSink::new(VNextValidatedSink::new(InMemoryVerifiedBackend::default()));
    let semantic = SemanticFrameSet {
        statements: vec![StatementFrame {
            statement_id: StatementId(0),
            operator_or_predicate: ku_core::foundation::semantic::ConceptCcid::from_bytes([7; 16]),
            arguments: vec![],
            constraints: vec![],
            qualifiers: Default::default(),
        }],
    };
    let known = semantic
        .to_knowledge_object(DisclosureClass::Public)
        .unwrap();
    let mut opaque = known.clone();
    opaque.kind = ObjectKind(999);
    let mut public_ids = Vec::new();
    for object in [known, opaque] {
        let (bytes, cid) = object.encode(ResourceProfile::ObjectV1).unwrap();
        sink.restore_accepted_record(&AcceptedRecordEntry {
            record_kind: StoredRecordKind::Object,
            claimed_cid: cid.into_bytes(),
            canonical_bytes: bytes,
        })
        .unwrap();
        public_ids.push(ObjectCID(cid.into_bytes()));
    }
    let generations =
        Arc::new(DatasetGenerationStore::open_exclusive(&temp.path().join("dataset")).unwrap());
    let mut config = crate::compiled_base_runtime_config();
    config.host_authorizer = Arc::new(Auth);
    config.ku = Some(KuRuntimeConfig {
        vault_key: VaultKey::from_bytes([11; 32]),
        registry: Some(reg.clone()),
        inputs: input.clone(),
        public: Some(Arc::new(sink)),
    });
    let rt = BaseRuntime::open(generations, config).unwrap();
    let s = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
    let KuResponseV1::Get(view) = s
        .invoke(
            KuRequestV1::Get(KuGetV1 {
                object_cid: public_ids[1],
            }),
            budget(),
        )
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(view.artifact_validity, ArtifactValidity::AcceptedOpaque);
    assert!(!view.executable);
    assert!(view.semantic_content_cid.is_none());
    let KuResponseV1::Export(export) = s
        .invoke(
            KuRequestV1::Export(KuExportV1 {
                mode: ExportMode::CanonicalPublicExchange,
                object_cids: vec![public_ids[0]],
            }),
            budget(),
        )
        .await
        .unwrap()
    else {
        panic!()
    };
    let bytes = STANDARD.decode(export.public_records.unwrap()).unwrap();
    assert_eq!(
        crate::canonical_exchange::read_canonical_exchange(bytes.as_slice())
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        s.invoke(
            KuRequestV1::Export(KuExportV1 {
                mode: ExportMode::CanonicalPublicExchange,
                object_cids: vec![public_ids[1]]
            }),
            budget()
        )
        .await
        .unwrap_err()
        .code,
        BaseErrorCodeV1::DependencyUnavailable
    );
    let (r, p) = prepare(&s, &input, &reg, 1).await;
    save(&s, &r, &p).await;
    assert_eq!(
        s.invoke(
            KuRequestV1::Export(KuExportV1 {
                mode: ExportMode::CanonicalPublicExchange,
                object_cids: p.object_cids.clone()
            }),
            budget()
        )
        .await
        .unwrap_err()
        .code,
        BaseErrorCodeV1::Conflict
    );
    assert_eq!(
        s.invoke(
            KuRequestV1::Export(KuExportV1 {
                mode: ExportMode::EncryptedBaseArchive,
                object_cids: p.object_cids
            }),
            budget()
        )
        .await
        .unwrap_err()
        .code,
        BaseErrorCodeV1::DependencyUnavailable
    );
}

#[tokio::test]
async fn interrupted_preparation_reconciles_to_no_effect_failure_and_drain_fences_new_work() {
    let temp = tempfile::tempdir().unwrap();
    let reg = registry(temp.path());
    let input = Arc::new(Inputs::new());
    let rt = runtime(temp.path(), input.clone(), Some(reg.clone()), 11).unwrap();
    let s = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
    let r = request(s.reserve().await.unwrap(), &input, &reg, 1);
    FAILPOINT.with(|p| *p.borrow_mut() = Some("after_prepared_journal".into()));
    assert_eq!(
        s.invoke(KuRequestV1::Prepare(r.clone()), budget())
            .await
            .unwrap_err()
            .code,
        BaseErrorCodeV1::UnknownOutcome
    );
    FAILPOINT.with(|p| *p.borrow_mut() = None);
    drop(rt);
    let rt = runtime(temp.path(), input.clone(), Some(reg), 11).unwrap();
    let s = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
    let KuResponseV1::Reconcile(receipt) = s
        .invoke(
            KuRequestV1::Reconcile(KuOperationRefV1 {
                operation_id: r.operation_id,
            }),
            budget(),
        )
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(receipt.state, BaseState::Failed);
    assert!(list(&s).await.items.is_empty());
    assert_eq!(input.calls.load(Ordering::SeqCst), 1);
    s.base.drain().await.unwrap();
    assert!(s.reserve().await.is_err());
    assert!(s.invoke(KuRequestV1::Prepare(r), budget()).await.is_err());
    s.invoke(
        KuRequestV1::Status(KuStatusRequestV1 { operation_id: None }),
        budget(),
    )
    .await
    .unwrap();
    s.base.close().await.unwrap();
    assert!(s
        .invoke(
            KuRequestV1::Status(KuStatusRequestV1 { operation_id: None }),
            budget()
        )
        .await
        .is_err());
}

#[tokio::test]
async fn concurrent_exact_confirmations_share_one_durable_result() {
    let temp = tempfile::tempdir().unwrap();
    let reg = registry(temp.path());
    let input = Arc::new(Inputs::new());
    let rt = runtime(temp.path(), input.clone(), Some(reg.clone()), 11).unwrap();
    let s = rt.ku_services(PRINCIPAL, b"host-proof").unwrap();
    let (r, p) = prepare(&s, &input, &reg, 1).await;
    let request = KuRequestV1::Save(KuSaveV1 {
        operation_id: r.operation_id,
        idempotency_key: r.idempotency_key,
        object_cids: p.object_cids,
    });
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let workers: Vec<_> = (0..2)
        .map(|_| {
            let s = s.clone();
            let request = request.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(s.invoke(request, budget()))
            })
        })
        .collect();
    let outcomes: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    assert!(outcomes.iter().any(Result::is_ok));
    for error in outcomes.iter().filter_map(|r| r.as_ref().err()) {
        assert_eq!(error.code, BaseErrorCodeV1::UnknownOutcome);
    }
    let KuResponseV1::Reconcile(receipt) = s
        .invoke(
            KuRequestV1::Reconcile(KuOperationRefV1 {
                operation_id: r.operation_id,
            }),
            budget(),
        )
        .await
        .unwrap()
    else {
        panic!()
    };
    for result in outcomes.into_iter().filter_map(Result::ok) {
        let KuResponseV1::Save(saved) = result else {
            panic!()
        };
        assert_eq!(saved, receipt);
    }
    assert_eq!(receipt.state, BaseState::Committed);
    assert_eq!(list(&s).await.items.len(), 2);
    assert_eq!(input.calls.load(Ordering::SeqCst), 1);
}

// ---- ARCH-03: journal capacity, compaction and saved-data preservation ----

fn open_direct(
    generations: &DatasetGenerationStore,
    input: Arc<Inputs>,
    registry: Option<Arc<ConceptRegistryGenerationManager>>,
) -> KuStore {
    KuStore::open(
        generations,
        KuRuntimeConfig {
            vault_key: VaultKey::from_bytes([11; 32]),
            registry,
            inputs: input,
            public: None,
        },
        [1; 32],
    )
    .unwrap()
}

fn table_len(store: &KuStore, table: TableDefinition<&[u8], &[u8]>) -> u64 {
    use redb::ReadableTableMetadata;
    let tx = store.journal.begin_read().unwrap();
    let table = tx.open_table(table).unwrap();
    table.len().unwrap()
}

fn operation(i: usize, tag: u8) -> OperationId {
    let mut id = [tag; 32];
    id[..8].copy_from_slice(&(i as u64 + 1).to_le_bytes());
    OperationId(id)
}

fn save_request(r: &KuPrepareV1, p: &KuPreparedV1) -> KuSaveV1 {
    KuSaveV1 {
        operation_id: r.operation_id,
        idempotency_key: r.idempotency_key,
        object_cids: p.object_cids.clone(),
    }
}

#[tokio::test]
async fn journal_compacts_finished_operations_past_active_cap_and_restart_keeps_saved_data() {
    let temp = tempfile::tempdir().unwrap();
    let reg = registry(temp.path());
    let input = Arc::new(Inputs::new());
    let generations = DatasetGenerationStore::open_exclusive(&temp.path().join("dataset")).unwrap();
    let store = open_direct(&generations, input.clone(), Some(reg.clone()));
    // Before ARCH-03 the 1,025th finished operation was refused forever.
    let total = MAX_OPERATIONS + 6;
    let mut saved = Vec::new();
    let mut canceled = Vec::new();
    let mut saved_cids = BTreeSet::new();
    for i in 0..total {
        let r = request(operation(i, 0xA3), &input, &reg, (i % 251) as u8 + 1);
        let p = store
            .prepare(PRINCIPAL, r.clone(), None, &budget())
            .await
            .unwrap();
        if i % 7 == 3 {
            store.cancel(PRINCIPAL, r.operation_id).unwrap();
            canceled.push(r);
        } else {
            let receipt = store.save(PRINCIPAL, &save_request(&r, &p)).unwrap();
            assert_eq!(receipt.state, BaseState::Committed);
            saved_cids.extend(p.object_cids.iter().map(|c| c.0));
            saved.push((r, p, receipt));
        }
    }
    // Finished operations no longer occupy active journal slots.
    assert_eq!(table_len(&store, JOURNAL), 0);
    assert_eq!(table_len(&store, TERMINAL), total as u64);

    // The active cap now bounds only pending preparations and reports an
    // honest non-retryable storage bound instead of a retryable rate limit.
    for i in 0..MAX_OPERATIONS {
        let r = request(operation(i, 0xB4), &input, &reg, (i % 251) as u8 + 1);
        store.prepare(PRINCIPAL, r, None, &budget()).await.unwrap();
    }
    let r = request(operation(MAX_OPERATIONS, 0xB4), &input, &reg, 1);
    let full = store
        .prepare(PRINCIPAL, r, None, &budget())
        .await
        .unwrap_err();
    assert_eq!(full.code, BaseErrorCodeV1::ResourceExhausted);
    assert_eq!(full.reason, KU_STORAGE_FULL);
    assert!(!full.retryable && !full.reconcile_before_retry);
    // Saving a pending one frees an active slot and still succeeds.
    let pending = request(operation(0, 0xB4), &input, &reg, 1);
    let preview = store.preview(PRINCIPAL, pending.operation_id).unwrap();
    store
        .save(PRINCIPAL, &save_request(&pending, &preview))
        .unwrap();
    assert_eq!(table_len(&store, JOURNAL), MAX_OPERATIONS as u64 - 1);
    drop(store);
    drop(generations);

    // Restart without registry or source access: saved data stays readable and
    // finished operations keep idempotent receipts from the compact record.
    input.allowed.store(false, Ordering::SeqCst);
    let generations = DatasetGenerationStore::open_exclusive(&temp.path().join("dataset")).unwrap();
    let store = open_direct(&generations, input.clone(), None);
    assert_eq!(
        store
            .catalog
            .lock()
            .unwrap()
            .items
            .get(&PRINCIPAL)
            .map(|m| m.len())
            .unwrap(),
        saved_cids.len()
    );
    for (r, p, receipt) in [&saved[0], saved.last().unwrap()] {
        assert_eq!(
            store.saved_receipt(PRINCIPAL, r.operation_id).unwrap(),
            Some(receipt.clone())
        );
        assert_eq!(
            &store.save(PRINCIPAL, &save_request(r, p)).unwrap(),
            receipt
        );
        let view = store.get(PRINCIPAL, p.object_cids[0]).unwrap();
        assert_eq!(view.canonical_bytes, p.artifacts[0].canonical_preview);
        assert!(store.owns_operation(PRINCIPAL, r.operation_id).unwrap());
    }
    let c = &canceled[0];
    let finished = store.read_bundle(c.operation_id).unwrap().unwrap();
    // Cancellation keeps its full encrypted staging as evidence (frozen
    // VALIDATED_STORAGE_PROFILE_V1); only its slot in the active table is freed.
    assert!(finished.compacted && finished.canceled && !finished.sources.is_empty());
    let committed = store.read_bundle(saved[0].0.operation_id).unwrap().unwrap();
    assert!(committed.compacted && committed.committed && committed.sources.is_empty());
    assert!(!committed.private_semantic_inputs.is_empty());
    assert!(store.owns_operation(PRINCIPAL, c.operation_id).unwrap());
    assert!(!store.is_prepared(PRINCIPAL, c.operation_id).unwrap());
    assert_eq!(
        store.preview(PRINCIPAL, c.operation_id).unwrap_err().code,
        BaseErrorCodeV1::Conflict
    );
    assert_eq!(table_len(&store, TERMINAL), total as u64 + 1);
    assert_eq!(
        input.calls.load(Ordering::SeqCst),
        total + MAX_OPERATIONS + 1
    );
}

#[tokio::test]
async fn legacy_finished_bundle_in_active_journal_migrates_on_open() {
    let temp = tempfile::tempdir().unwrap();
    let reg = registry(temp.path());
    let input = Arc::new(Inputs::new());
    let generations = DatasetGenerationStore::open_exclusive(&temp.path().join("dataset")).unwrap();
    let store = open_direct(&generations, input.clone(), Some(reg.clone()));
    let r = request(operation(0, 0xC5), &input, &reg, 1);
    let p = store
        .prepare(PRINCIPAL, r.clone(), None, &budget())
        .await
        .unwrap();
    let mut legacy = store.read_bundle(r.operation_id).unwrap().unwrap();
    assert!(!legacy.compacted && !legacy.sources.is_empty());
    let receipt = store.save(PRINCIPAL, &save_request(&r, &p)).unwrap();
    // Rewrite the record exactly as a pre-ARCH-03 journal held it: a full
    // committed bundle in the active table, without the `compacted` field.
    legacy.committed = true;
    let mut json = serde_json::to_value(&legacy).unwrap();
    json.as_object_mut().unwrap().remove("compacted");
    let sealed = store
        .vault
        .seal_local_metadata(
            store.bundle_key(r.operation_id),
            &serde_json::to_vec(&json).unwrap(),
        )
        .unwrap();
    let tx = store.journal.begin_write().unwrap();
    {
        tx.open_table(TERMINAL)
            .unwrap()
            .remove(r.operation_id.0.as_slice())
            .unwrap();
        tx.open_table(JOURNAL)
            .unwrap()
            .insert(r.operation_id.0.as_slice(), sealed.as_slice())
            .unwrap();
    }
    tx.commit().unwrap();
    drop(store);
    drop(generations);

    let generations = DatasetGenerationStore::open_exclusive(&temp.path().join("dataset")).unwrap();
    let store = open_direct(&generations, input.clone(), None);
    assert_eq!(table_len(&store, JOURNAL), 0);
    assert_eq!(table_len(&store, TERMINAL), 1);
    let migrated = store.read_bundle(r.operation_id).unwrap().unwrap();
    assert!(migrated.compacted && migrated.committed && migrated.sources.is_empty());
    assert_eq!(
        store.saved_receipt(PRINCIPAL, r.operation_id).unwrap(),
        Some(receipt)
    );
    assert_eq!(
        store
            .get(PRINCIPAL, p.object_cids[0])
            .unwrap()
            .canonical_bytes,
        p.artifacts[0].canonical_preview
    );
}

// ---- ARCH-08: structured KU search on the vnext_semantic_index projection ----

fn search_cids(store: &KuStore, query: &str) -> BTreeSet<[u8; 32]> {
    store
        .page(PRINCIPAL, Some(query.into()), 256, None, &budget())
        .unwrap()
        .items
        .iter()
        .map(|s| s.object_cid.0)
        .collect()
}

#[tokio::test]
async fn concept_only_ku_is_findable_and_semantic_index_rebuilds_identically_after_restart() {
    let temp = tempfile::tempdir().unwrap();
    let reg = registry(temp.path());
    let input = Arc::new(Inputs::new());
    let generations = DatasetGenerationStore::open_exclusive(&temp.path().join("dataset")).unwrap();
    let store = open_direct(&generations, input.clone(), Some(reg.clone()));
    // One literal-text KU pair and one concept-only KU pair.
    let r = request(operation(0, 0xD6), &input, &reg, 1);
    let text = store
        .prepare(PRINCIPAL, r.clone(), None, &budget())
        .await
        .unwrap();
    store.save(PRINCIPAL, &save_request(&r, &text)).unwrap();
    input.concept_only.store(true, Ordering::SeqCst);
    let r = request(operation(1, 0xD6), &input, &reg, 2);
    let concept = store
        .prepare(PRINCIPAL, r.clone(), None, &budget())
        .await
        .unwrap();
    store.save(PRINCIPAL, &save_request(&r, &concept)).unwrap();
    let text_cids: BTreeSet<_> = text.object_cids.iter().map(|c| c.0).collect();
    let concept_cids: BTreeSet<_> = concept.object_cids.iter().map(|c| c.0).collect();
    assert_eq!(concept_cids.len(), 2);
    let all: BTreeSet<_> = text_cids.union(&concept_cids).copied().collect();

    // Concept-only KUs have no literal text, so the old `text.contains`
    // search never found them; the Registry label and CCID postings do.
    let water = hex(&[7; 16]);
    assert_eq!(search_cids(&store, "WATER"), all);
    assert_eq!(search_cids(&store, &format!("concept:{water}")), all);
    assert_eq!(search_cids(&store, &format!("predicate:{water}")), all);
    // Case-folded literal text still matches literal-text KUs only.
    assert_eq!(search_cids(&store, "Water  VERSION 1"), text_cids);
    // Structured terms AND with the free-text phrase.
    assert_eq!(
        search_cids(&store, &format!("concept:{water} version 1")),
        text_cids
    );
    assert!(search_cids(&store, &format!("unit:{water}")).is_empty());
    assert!(search_cids(&store, "dimension:1,0,0,0,0,0,0").is_empty());
    assert!(search_cids(&store, "steam").is_empty());
    let page = store
        .page(PRINCIPAL, Some("water".into()), 256, None, &budget())
        .unwrap();
    assert!(page
        .limitations
        .contains(&"index_version_ku_semantic_1".into()));
    assert!(!page
        .limitations
        .contains(&"label_resolution_unavailable".into()));
    assert_eq!(
        store
            .page(
                PRINCIPAL,
                Some("concept:nothex".into()),
                256,
                None,
                &budget()
            )
            .unwrap_err()
            .code,
        BaseErrorCodeV1::InvalidRequest
    );
    // Another principal sees nothing through the shared store.
    assert!(store
        .page([10; 32], Some("water".into()), 256, None, &budget())
        .unwrap()
        .items
        .is_empty());
    let before = store.catalog.lock().unwrap().semantic[&PRINCIPAL].snapshot();
    assert_eq!(before.source_count, 4);
    drop(store);
    drop(generations);

    // Restart: the projection is rebuilt from committed records only and is
    // identical; without a Registry, label search degrades honestly while
    // CCID search keeps working.
    let generations = DatasetGenerationStore::open_exclusive(&temp.path().join("dataset")).unwrap();
    let store = open_direct(&generations, input.clone(), None);
    assert_eq!(
        store.catalog.lock().unwrap().semantic[&PRINCIPAL].snapshot(),
        before
    );
    assert_eq!(search_cids(&store, &format!("concept:{water}")), all);
    let page = store
        .page(PRINCIPAL, Some("water".into()), 256, None, &budget())
        .unwrap();
    assert!(page
        .limitations
        .contains(&"label_resolution_unavailable".into()));
    let found: BTreeSet<_> = page.items.iter().map(|s| s.object_cid.0).collect();
    assert_eq!(found, text_cids);
    drop(store);
    drop(generations);
    let generations = DatasetGenerationStore::open_exclusive(&temp.path().join("dataset")).unwrap();
    let store = open_direct(&generations, input, Some(reg));
    assert_eq!(
        store.catalog.lock().unwrap().semantic[&PRINCIPAL].snapshot(),
        before
    );
    assert_eq!(search_cids(&store, "water"), all);
}
