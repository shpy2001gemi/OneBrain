//! Opt-in host composition. Model and source custody are host capabilities;
//! user text is admitted only with an actual, durable private consent bundle.
use crate::{
    concept_registry_runtime::{ConceptRegistryGenerationManager, ConceptRegistryReaderLease},
    ku_extraction::*,
    ku_manual::*,
    ku_product::*,
    BaseServiceError,
};
use ku_core::foundation::{
    ObjectReference, ObservationGovernance, ResourceProfile, SourceArtifact, SourceArtifactKind,
};
use ku_encoder::extraction::{ExtractionProvider, ExtractionWorkflow};
use onebrain_base_contract::ku_payload::KuPayload;
use onebrain_base_contract::{
    ku::*,
    ku_payload::{decode_hex, hex},
    ResourceBudgetV1,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

pub const CONSENT: &str = "I permit this local host to encode the submitted text with the selected experimental Ollama model and retain the source, consent and KU privately in this dataset until I remove the dataset.";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextIntake {
    pub operation_id: OperationId,
    pub idempotency_key: IdempotencyKey,
    pub model: String,
    pub text: String,
    pub consent: bool,
}
#[derive(Serialize)]
pub struct LocalModel {
    pub model: String,
    pub implementation_commitment: ImplementationCommitment,
    pub experimental: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextIntakeRecord {
    pub principal: [u8; 32],
    pub input: TextIntake,
    pub preparation: KuPrepareV1,
    pub source: Vec<u8>,
    pub records: Value,
}
fn digest(value: &Value) -> Result<[u8; 32], BaseServiceError> {
    decode_hex(&ku_encoder::extraction::artifact_sha256(value).map_err(|_| invalid())?)
        .map_err(|_| invalid())
}
fn material(
    principal: [u8; 32],
    input: &TextIntake,
    implementation: ImplementationCommitment,
) -> Result<(Vec<u8>, SourceArtifactCID, Value), BaseServiceError> {
    if !input.consent
        || input.text.trim().is_empty()
        || input.text.len() > 8192
        || input.model.len() > 80
    {
        return Err(invalid());
    }
    let policy = json!({"profile":"ku-local-text-consent/1","text":CONSENT,"destination":"LOCAL_ONLY","retention":"until_dataset_removal"});
    let adapter = json!({"profile":"ku-local-text-adapter/1","name":"authenticated-ku-text-intake","version":1,"media":"text/plain; charset=utf-8"});
    let receipt = json!({"profile":"ku-local-text-receipt/1","principal":hex(&principal),"request":input,
        "policy":hex(&digest(&policy)?),"implementation":implementation});
    let scope = json!({"receipt":hex(&digest(&receipt)?),"source_sha256":hex(&digest(&json!(input.text))?),"use":"experimental_local_extraction"});
    let assessment = json!({"profile":"ku-local-text-assessment/1","policy":hex(&digest(&policy)?),"scope":hex(&digest(&scope)?),"decision":"permitted","principal":hex(&principal)});
    let frontier = json!({"profile":"ku-local-text-policy-frontier/1","policy":hex(&digest(&policy)?),"revocation":"dataset_owner_removal"});
    let retention = json!({"profile":"ku-local-text-retention/1","policy":hex(&digest(&policy)?),"duration":"until_dataset_removal"});
    let source = SourceArtifact {
        source_kind: SourceArtifactKind::Text,
        raw_bytes: input.text.as_bytes().to_vec(),
        media_type_commitment: digest(&json!("text/plain; charset=utf-8"))?,
        capture_adapter: ObjectReference::new(0, digest(&adapter)?),
        capture_sequence: 0,
        governance: ObservationGovernance {
            consent_policy: ObjectReference::new(0, digest(&policy)?),
            consent_receipt: ObjectReference::new(0, digest(&receipt)?),
            revocation_policy: ObjectReference::new(0, digest(&frontier)?),
            retention_policy: ObjectReference::new(0, digest(&retention)?),
            capture_scope_commitment: digest(&scope)?,
            authorization_assessment_commitment: digest(&assessment)?,
            assessed_frontier: digest(&frontier)?,
        },
    };
    let (bytes, cid) = source
        .to_private_object()
        .map_err(|_| invalid())?
        .encode(ResourceProfile::ObjectV1)
        .map_err(|_| invalid())?;
    Ok((
        bytes,
        SourceArtifactCID(cid.into_bytes()),
        json!({"policy":policy,"adapter":adapter,"receipt":receipt,"scope":scope,"assessment":assessment,"frontier":frontier,"retention":retention}),
    ))
}
#[derive(Default)]
struct Captured {
    sources: BTreeMap<[u8; 32], TextIntakeRecord>,
}
fn validate_record(
    principal: [u8; 32],
    record: &TextIntakeRecord,
) -> Result<SourceArtifactCID, BaseServiceError> {
    if record.principal != principal {
        return Err(not_found());
    }
    let (source, cid, records) = material(
        record.principal,
        &record.input,
        record.preparation.implementation_commitment,
    )?;
    if source != record.source
        || records != record.records
        || record.preparation.source_refs != vec![cid]
        || record.preparation.operation_id != record.input.operation_id
        || record.preparation.idempotency_key != record.input.idempotency_key
        || record.preparation.input_mode != InputMode::LocalAi
        || record.preparation.draft_ref.is_some()
        || record.preparation.destination != Disclosure::LOCALONLY
    {
        return Err(invalid());
    }
    record.preparation.validate().map_err(|_| invalid())?;
    Ok(cid)
}
struct Custody {
    principal: [u8; 32],
    captured: Mutex<Captured>,
}
impl KuExtractionSources for Custody {
    fn check_access(
        &self,
        principal: [u8; 32],
        sources: &[[u8; 32]],
    ) -> Result<(), BaseServiceError> {
        let captured = self.captured.lock().map_err(|_| unavailable())?;
        if principal != self.principal
            || sources.iter().any(|id| !captured.sources.contains_key(id))
        {
            return Err(not_found());
        }
        Ok(())
    }
    fn read_source(
        &self,
        principal: [u8; 32],
        source: [u8; 32],
        max_bytes: usize,
    ) -> Result<Vec<u8>, BaseServiceError> {
        self.check_access(principal, &[source])?;
        let captured = self.captured.lock().map_err(|_| unavailable())?;
        let record = captured.sources.get(&source).ok_or_else(not_found)?;
        if record.source.len() > max_bytes {
            return Err(resource());
        }
        Ok(record.source.clone())
    }
}
pub struct OllamaKuInputs {
    principal: [u8; 32],
    manual: Arc<ManualKuInputs>,
    registry: Arc<ConceptRegistryGenerationManager>,
    custody: Arc<Custody>,
    models: BTreeMap<String, SharedKuExtractionInputs>,
}
impl OllamaKuInputs {
    pub fn new(
        principal: [u8; 32],
        manual: Arc<ManualKuInputs>,
        registry: Arc<ConceptRegistryGenerationManager>,
        providers: Vec<(String, Arc<dyn ExtractionProvider>, u64)>,
    ) -> Result<Self, BaseServiceError> {
        if providers.len() > 8 {
            return Err(invalid());
        }
        let custody = Arc::new(Custody {
            principal,
            captured: Mutex::default(),
        });
        let mut models = BTreeMap::new();
        for (name, provider, memory) in providers {
            let workflow =
                Arc::new(ExtractionWorkflow::new(provider, memory).map_err(|_| unavailable())?);
            let input = SharedKuExtractionInputs::new(
                custody.clone(),
                workflow,
                InputMode::LocalAi,
                "standard",
            )
            .map_err(|_| unavailable())?;
            if models.insert(name, input).is_some() {
                return Err(invalid());
            }
        }
        Ok(Self {
            principal,
            manual,
            registry,
            custody,
            models,
        })
    }
    fn model(&self, request: &KuPrepareV1) -> Result<&SharedKuExtractionInputs, BaseServiceError> {
        self.models
            .values()
            .find(|p| {
                p.implementation(InputMode::LocalAi) == Some(request.implementation_commitment.0)
            })
            .ok_or_else(unavailable)
    }
}
impl KuInputProvider for OllamaKuInputs {
    fn experimental_ai_allowed(&self, commitment: [u8; 32]) -> bool {
        self.models
            .values()
            .any(|p| p.implementation(InputMode::LocalAi) == Some(commitment))
    }
    fn supports_implementation(&self, mode: InputMode, commitment: [u8; 32]) -> bool {
        if mode == InputMode::LocalAi {
            self.experimental_ai_allowed(commitment)
        } else {
            self.manual.supports_implementation(mode, commitment)
        }
    }
    fn implementation(&self, mode: InputMode) -> Option<[u8; 32]> {
        if mode == InputMode::LocalAi {
            self.models.values().next()?.implementation(mode)
        } else {
            self.manual.implementation(mode)
        }
    }
    fn capture_text(
        &self,
        principal: [u8; 32],
        input: TextIntake,
    ) -> Result<TextIntakeRecord, BaseServiceError> {
        self.check_access(principal, &[])?;
        let model = self.models.get(&input.model).ok_or_else(unavailable)?;
        let implementation = ImplementationCommitment(
            model
                .implementation(InputMode::LocalAi)
                .ok_or_else(unavailable)?,
        );
        let (source, cid, records) = material(principal, &input, implementation)?;
        let root = self
            .registry
            .reader_lease()
            .status()
            .release_aggregate_root
            .clone()
            .ok_or_else(unavailable)?;
        let preparation = KuPrepareV1 {
            operation_id: input.operation_id,
            idempotency_key: input.idempotency_key,
            input_mode: InputMode::LocalAi,
            source_refs: vec![cid],
            registry_release_root: ReleaseRoot(decode_hex(&root).map_err(|_| unavailable())?),
            semantic_profile: "ku-semantic-content/1.0".into(),
            implementation_commitment: implementation,
            destination: Disclosure::LOCALONLY,
            draft_ref: None,
        };
        preparation.validate().map_err(|_| invalid())?;
        Ok(TextIntakeRecord {
            principal,
            input,
            preparation,
            source,
            records,
        })
    }
    fn restore_text(&self, record: TextIntakeRecord) -> Result<(), BaseServiceError> {
        let cid = validate_record(self.principal, &record)?;
        let mut captured = self.custody.captured.lock().map_err(|_| unavailable())?;
        if captured.sources.len() >= 256 && !captured.sources.contains_key(&cid.0) {
            return Err(resource());
        }
        captured.sources.insert(cid.0, record);
        Ok(())
    }
    fn check_access(
        &self,
        principal: [u8; 32],
        sources: &[[u8; 32]],
    ) -> Result<(), BaseServiceError> {
        if principal != self.principal {
            return Err(not_found());
        }
        for id in sources {
            if self.custody.check_access(principal, &[*id]).is_err() {
                self.manual.check_access(principal, &[*id])?;
            }
        }
        Ok(())
    }
    fn editor(
        &self,
        principal: [u8; 32],
        request: ManualEditorRequest,
        budget: &ResourceBudgetV1,
    ) -> Result<ManualEditorResponse, BaseServiceError> {
        self.check_access(principal, &[])?;
        match request {
            ManualEditorRequest::Models {} => {
                if self.models.len() > budget.max_items as usize {
                    return Err(resource());
                }
                Ok(ManualEditorResponse::Models {
                    models: self
                        .models
                        .iter()
                        .map(|(name, p)| {
                            Ok(LocalModel {
                                model: name.clone(),
                                implementation_commitment: ImplementationCommitment(
                                    p.implementation(InputMode::LocalAi)
                                        .ok_or_else(unavailable)?,
                                ),
                                experimental: true,
                            })
                        })
                        .collect::<Result<_, BaseServiceError>>()?,
                    limitations: vec!["experimental_model_unqualified".into(), "local_only".into()],
                    consent_text: CONSENT.into(),
                })
            }
            ManualEditorRequest::EncodeText(_) => Err(invalid()), // KuStore must durably stage this.
            other => self.manual.editor(principal, other, budget),
        }
    }
    fn resolve(
        &self,
        principal: [u8; 32],
        request: &KuPrepareV1,
        registry: &ConceptRegistryReaderLease,
        budget: &ResourceBudgetV1,
    ) -> Result<KuResolvedInput, BaseServiceError> {
        self.manual.resolve(principal, request, registry, budget)
    }
    fn resolve_async<'a>(
        &'a self,
        principal: [u8; 32],
        request: &'a KuPrepareV1,
        registry: &'a ConceptRegistryReaderLease,
        budget: &'a ResourceBudgetV1,
        execution: KuExtractionExecution<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<KuResolvedInput, BaseServiceError>> + Send + 'a>> {
        Box::pin(async move {
            if request.input_mode != InputMode::LocalAi {
                return self
                    .manual
                    .resolve_async(principal, request, registry, budget, execution)
                    .await;
            }
            {
                let captured = self.custody.captured.lock().map_err(|_| unavailable())?;
                let record = request
                    .source_refs
                    .first()
                    .and_then(|s| captured.sources.get(&s.0))
                    .ok_or_else(not_found)?;
                if record.principal != principal
                    || serde_json::to_vec(&record.preparation).ok()
                        != serde_json::to_vec(request).ok()
                {
                    return Err(invalid());
                }
            }
            self.model(request)?
                .resolve_async(principal, request, registry, budget, execution)
                .await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> TextIntake {
        TextIntake {
            operation_id: OperationId([1; 32]),
            idempotency_key: IdempotencyKey([2; 32]),
            model: "qwen3:8b".into(),
            text: "Đồng dẫn điện.".into(),
            consent: true,
        }
    }
    fn record() -> TextIntakeRecord {
        let input = input();
        let implementation = ImplementationCommitment([3; 32]);
        let (source, cid, records) = material([0; 32], &input, implementation).unwrap();
        TextIntakeRecord {
            principal: [0; 32],
            preparation: KuPrepareV1 {
                operation_id: input.operation_id,
                idempotency_key: input.idempotency_key,
                input_mode: InputMode::LocalAi,
                source_refs: vec![cid],
                registry_release_root: ReleaseRoot([4; 32]),
                semantic_profile: "ku-semantic-content/1.0".into(),
                implementation_commitment: implementation,
                destination: Disclosure::LOCALONLY,
                draft_ref: None,
            },
            input,
            source,
            records,
        }
    }
    #[test]
    fn ku_text_source_has_resolvable_distinct_governance_and_exact_utf8() {
        let record = record();
        let cid = validate_record([0; 32], &record).unwrap();
        let object = crate::ku_product::decode_object(&record.source).unwrap();
        assert_eq!(object.cid().into_bytes(), cid.0);
        let source = SourceArtifact::from_validated(&object).unwrap();
        assert_eq!(source.raw_bytes, record.input.text.as_bytes());
        assert_eq!(
            source.governance.consent_receipt.cid,
            digest(&record.records["receipt"]).unwrap()
        );
        assert_eq!(
            source.governance.retention_policy.cid,
            digest(&record.records["retention"]).unwrap()
        );
        assert_ne!(
            source.governance.retention_policy,
            source.governance.consent_policy
        );
    }
    #[test]
    fn ku_text_rejects_denied_consent_oversize_and_tampered_custody() {
        let original = record();
        for changed in [
            "consent",
            "text",
            "model",
            "principal",
            "records",
            "source",
            "implementation",
        ] {
            let mut record = original.clone();
            match changed {
                "consent" => record.input.consent = false,
                "text" => record.input.text = "a".repeat(8193),
                "model" => record.input.model = "qwen3:1.7b".into(),
                "principal" => record.principal = [5; 32],
                "records" => record.records["assessment"]["decision"] = "denied".into(),
                "source" => record.source[0] ^= 1,
                _ => {
                    record.preparation.implementation_commitment = ImplementationCommitment([6; 32])
                }
            }
            assert!(validate_record([0; 32], &record).is_err(), "{changed}");
        }
    }
    #[test]
    fn ku_text_transport_rejects_duplicate_null_and_unknown_fields() {
        let valid = serde_json::to_string(&input()).unwrap();
        let duplicate = valid.replacen("{", "{\"consent\":false,", 1);
        assert!(serde_json::from_str::<TextIntake>(&duplicate).is_err());
        let mut value = serde_json::to_value(input()).unwrap();
        value["consent"] = Value::Null;
        assert!(serde_json::from_value::<TextIntake>(value).is_err());
        let mut value = serde_json::to_value(input()).unwrap();
        value["endpoint"] = "http://other.invalid".into();
        assert!(serde_json::from_value::<TextIntake>(value).is_err());
    }
}
