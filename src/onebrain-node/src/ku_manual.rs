//! Opt-in, host-admitted manual editor. No model, signer or browser authority.
use crate::concept_registry_runtime::{
    ConceptRegistryGenerationManager, ConceptRegistryReaderLease,
};
use crate::ku_product::{KuConceptBinding, KuInputProvider, KuResolvedInput};
use crate::BaseServiceError;
use ku_core::concept_registry::ResolveResult;
use ku_core::foundation::semantic::{
    LiteralValue, SemanticFrameSet, SourceSpan, StatementFrame, StatementId, StatementQualifiers,
    TermRef,
};
use ku_core::foundation::{
    ConceptCcid, DisclosureClass, NormalizedText, ObjectReference, SourceArtifact,
    SourceArtifactKind,
};
use onebrain_base_contract::ku::*;
use onebrain_base_contract::ku_payload::{decode_hex, hex};
use onebrain_base_contract::{BaseErrorCodeV1, ResourceBudgetV1};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

fn error(code: BaseErrorCodeV1, reason: &'static str) -> BaseServiceError {
    BaseServiceError::new(code, reason)
}
fn invalid() -> BaseServiceError {
    error(BaseErrorCodeV1::InvalidRequest, "invalid_manual_draft")
}
fn unavailable() -> BaseServiceError {
    error(
        BaseErrorCodeV1::DependencyUnavailable,
        "manual_editor_not_installed",
    )
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManualDraft {
    pub operation_id: OperationId,
    pub idempotency_key: IdempotencyKey,
    pub source_ref: SourceArtifactCID,
    pub predicate_label: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "selected_ccid"
    )]
    pub selected_ccid: Option<String>,
    pub argument_text: String,
}
fn selected_ccid<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    String::deserialize(d).map(Some)
}
#[derive(Deserialize)]
#[serde(
    tag = "action",
    content = "payload",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ManualEditorRequest {
    Catalog {},
    Resolve { label: String },
    Draft(ManualDraft),
}
#[derive(Serialize)]
pub struct ManualSource {
    source_ref: SourceArtifactCID,
    label: String,
}
#[derive(Serialize)]
pub struct ManualCandidate {
    ccid: String,
}
#[derive(Serialize)]
#[serde(untagged)]
pub enum ManualEditorResponse {
    Catalog {
        sources: Vec<ManualSource>,
        limitations: Vec<String>,
    },
    Candidates {
        candidates: Vec<ManualCandidate>,
        limitations: Vec<String>,
    },
    Draft(KuPrepareV1),
}
struct Source {
    label: String,
    bytes: Vec<u8>,
    length: u64,
}
struct Draft {
    input: ManualDraft,
    exact: Vec<u8>,
    preparation: KuPrepareV1,
}
#[derive(Default)]
struct Drafts {
    items: BTreeMap<[u8; 32], Draft>,
    bytes: usize,
}
pub struct ManualKuInputs {
    principal: [u8; 32],
    registry: Arc<ConceptRegistryGenerationManager>,
    sources: BTreeMap<[u8; 32], Source>,
    drafts: Mutex<Drafts>,
}
impl ManualKuInputs {
    /// Caller is the trusted host custody owner, not a product/API principal.
    /// Each source must already carry the operator's actual governance records.
    pub fn new(
        principal: [u8; 32],
        registry: Arc<ConceptRegistryGenerationManager>,
        admitted: Vec<(String, Vec<u8>)>,
    ) -> Result<Self, BaseServiceError> {
        if admitted.is_empty() || admitted.len() > 64 {
            return Err(invalid());
        }
        let mut sources = BTreeMap::new();
        let mut total = 0usize;
        for (label, bytes) in admitted {
            total += bytes.len();
            if label.is_empty()
                || label.len() > 128
                || bytes.len() > 65536
                || total > 4 * 1024 * 1024
            {
                return Err(invalid());
            }
            let object = crate::ku_product::decode_object(&bytes)?;
            if object.disclosure() != DisclosureClass::LocalOnly {
                return Err(invalid());
            }
            let source = SourceArtifact::from_validated(&object).map_err(|_| invalid())?;
            if source.source_kind != SourceArtifactKind::Text {
                return Err(invalid());
            }
            let cid = object.cid().into_bytes();
            if sources
                .insert(
                    cid,
                    Source {
                        label,
                        length: source.raw_bytes.len() as u64,
                        bytes,
                    },
                )
                .is_some()
            {
                return Err(invalid());
            }
        }
        let lease = registry.reader_lease();
        if lease.status().release_aggregate_root.is_none() {
            return Err(unavailable());
        }
        Ok(Self {
            principal,
            registry,
            sources,
            drafts: Mutex::default(),
        })
    }
    fn candidates(
        lease: &ConceptRegistryReaderLease,
        label: &str,
    ) -> Result<Vec<ManualCandidate>, BaseServiceError> {
        if label.is_empty() || label.len() > 256 {
            return Err(invalid());
        }
        let values = match lease.resolve_checked(label).map_err(|_| unavailable())? {
            ResolveResult::Found(v) | ResolveResult::Fuzzy(v) => vec![v],
            ResolveResult::Ambiguous(v) => v,
            ResolveResult::NotFound => vec![],
        };
        if values.len() > 64 {
            return Err(error(
                BaseErrorCodeV1::ResourceExhausted,
                "manual_candidates_limit",
            ));
        }
        Ok(values
            .into_iter()
            .map(|v| ManualCandidate { ccid: hex(&v.ccid) })
            .collect())
    }
}
impl KuInputProvider for ManualKuInputs {
    fn implementation(&self, mode: InputMode) -> Option<[u8; 32]> {
        (mode == InputMode::ResolvedSemanticDraft)
            .then(|| *blake3::hash(b"onebrain:manual-predicate-text-editor:1").as_bytes())
    }
    fn check_access(
        &self,
        principal: [u8; 32],
        sources: &[[u8; 32]],
    ) -> Result<(), BaseServiceError> {
        if principal != self.principal || sources.iter().any(|s| !self.sources.contains_key(s)) {
            return Err(error(BaseErrorCodeV1::NotFound, "ku_not_found"));
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
        let limits = || {
            vec![
                "manual_assertion_unassessed".into(),
                "host_admitted_sources_only".into(),
            ]
        };
        match request {
            ManualEditorRequest::Catalog {} => {
                if self.sources.len() > budget.max_items as usize {
                    return Err(invalid());
                }
                Ok(ManualEditorResponse::Catalog {
                    sources: self
                        .sources
                        .iter()
                        .map(|(cid, s)| ManualSource {
                            source_ref: SourceArtifactCID(*cid),
                            label: s.label.clone(),
                        })
                        .collect(),
                    limitations: limits(),
                })
            }
            ManualEditorRequest::Resolve { label } => {
                let candidates = Self::candidates(&self.registry.reader_lease(), &label)?;
                if candidates.len() > budget.max_items as usize {
                    return Err(invalid());
                }
                Ok(ManualEditorResponse::Candidates {
                    candidates,
                    limitations: limits(),
                })
            }
            ManualEditorRequest::Draft(input) => {
                self.check_access(principal, &[input.source_ref.0])?;
                if input.argument_text.trim().is_empty() || input.argument_text.len() > 4096 {
                    return Err(invalid());
                }
                NormalizedText::new(input.argument_text.clone()).map_err(|_| invalid())?;
                let lease = self.registry.reader_lease();
                let candidates = Self::candidates(&lease, &input.predicate_label)?;
                if let Some(selected) = &input.selected_ccid {
                    decode_hex::<16>(selected).map_err(|_| invalid())?;
                    if !candidates.iter().any(|c| &c.ccid == selected) {
                        return Err(invalid());
                    }
                }
                let exact = serde_json::to_vec(&input).map_err(|_| invalid())?;
                if exact.len() as u64 > budget.max_bytes {
                    return Err(invalid());
                }
                let mut drafts = self.drafts.lock().map_err(|_| unavailable())?;
                if let Some(old) = drafts.items.get(&input.operation_id.0) {
                    if old.exact != exact {
                        return Err(error(
                            BaseErrorCodeV1::Conflict,
                            "manual_draft_identity_conflict",
                        ));
                    }
                    return Ok(ManualEditorResponse::Draft(old.preparation.clone()));
                }
                if drafts.items.len() >= 256 || drafts.bytes + exact.len() > 4 * 1024 * 1024 {
                    return Err(error(
                        BaseErrorCodeV1::ResourceExhausted,
                        "manual_draft_limit_restart_host",
                    ));
                }
                let mut reference = [0u8; 32];
                getrandom::fill(&mut reference).map_err(|_| unavailable())?;
                let preparation = KuPrepareV1 {
                    operation_id: input.operation_id,
                    idempotency_key: input.idempotency_key,
                    input_mode: InputMode::ResolvedSemanticDraft,
                    source_refs: vec![input.source_ref],
                    registry_release_root: ReleaseRoot(
                        decode_hex(
                            lease
                                .status()
                                .release_aggregate_root
                                .as_ref()
                                .ok_or_else(unavailable)?,
                        )
                        .map_err(|_| unavailable())?,
                    ),
                    semantic_profile: "ku-semantic-content/1.0".into(),
                    implementation_commitment: ImplementationCommitment(
                        self.implementation(InputMode::ResolvedSemanticDraft)
                            .unwrap(),
                    ),
                    destination: Disclosure::LOCALONLY,
                    draft_ref: Some(ObjectCID(reference)),
                };
                drafts.bytes += exact.len();
                drafts.items.insert(
                    input.operation_id.0,
                    Draft {
                        input,
                        exact,
                        preparation: preparation.clone(),
                    },
                );
                Ok(ManualEditorResponse::Draft(preparation))
            }
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
        let drafts = self.drafts.lock().map_err(|_| unavailable())?;
        let draft = drafts
            .items
            .get(&request.operation_id.0)
            .ok_or_else(|| error(BaseErrorCodeV1::NotFound, "manual_draft_expired"))?;
        if serde_json::to_vec(&draft.preparation).ok() != serde_json::to_vec(request).ok() {
            return Err(invalid());
        }
        let source = self
            .sources
            .get(&draft.input.source_ref.0)
            .ok_or_else(invalid)?;
        let selected = draft
            .input
            .selected_ccid
            .as_ref()
            .map(|s| decode_hex::<16>(s).map_err(|_| invalid()))
            .transpose()?;
        let frames = selected
            .map(|ccid| SemanticFrameSet {
                statements: vec![StatementFrame {
                    statement_id: StatementId(1),
                    operator_or_predicate: ConceptCcid::from_bytes(ccid),
                    arguments: vec![TermRef::Literal(LiteralValue::Text(
                        NormalizedText::new(draft.input.argument_text.clone())
                            .expect("validated intake"),
                    ))],
                    constraints: vec![],
                    qualifiers: StatementQualifiers {
                        source_spans: vec![SourceSpan {
                            source: ObjectReference::new(1, draft.input.source_ref.0),
                            start: 0,
                            end: source.length,
                        }],
                        ..Default::default()
                    },
                }],
            })
            .into_iter()
            .collect();
        Ok(KuResolvedInput {
            drafts: frames,
            source_objects: vec![source.bytes.clone()],
            bindings: vec![KuConceptBinding {
                label: draft.input.predicate_label.clone(),
                selected,
            }],
            needs_resolution: selected.is_none(),
            extraction_budget: None,
        })
    }
}
