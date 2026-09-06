//! Concrete bridge from authenticated KU preparation to the shared encoder.
//! Sources remain behind the host custody port; Registry authority comes only
//! from the node's verified immutable reader lease. No model can install a port.
use crate::base_runtime::BaseServiceError;
use crate::concept_registry_runtime::{ConceptRegistryBackendKind, ConceptRegistryReaderLease};
use crate::ku_product::{
    KuConceptBinding, KuExtractionExecution, KuInputProvider, KuResolvedInput,
};
use ku_core::concept_registry::ResolveResult;
use ku_core::foundation::{SourceArtifact, SourceArtifactKind};
use ku_encoder::extraction::*;
use onebrain_base_contract::ku::*;
use onebrain_base_contract::ku_payload::{decode_hex, hex};
use onebrain_base_contract::ResourceBudgetV1;
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

type Result<T> = std::result::Result<T, ExtractionError>;
fn require(ok: bool, code: &'static str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(ExtractionError(code))
    }
}

/// Host-installed source custody capability. Implementations must enforce the
/// current principal/grant/revocation state before each read and obey max_bytes.
/// Opaque source IDs are not access grants and are never filesystem paths.
pub trait KuExtractionSources: Send + Sync {
    fn check_access(
        &self,
        principal: [u8; 32],
        sources: &[[u8; 32]],
    ) -> std::result::Result<(), BaseServiceError>;
    fn read_source(
        &self,
        principal: [u8; 32],
        source: [u8; 32],
        max_bytes: usize,
    ) -> std::result::Result<Vec<u8>, BaseServiceError>;
}

pub struct SharedKuExtractionInputs {
    sources: Arc<dyn KuExtractionSources>,
    workflow: Arc<ExtractionWorkflow>,
    mode: InputMode,
    resource_profile: String,
}
impl SharedKuExtractionInputs {
    pub fn new(
        sources: Arc<dyn KuExtractionSources>,
        workflow: Arc<ExtractionWorkflow>,
        mode: InputMode,
        resource_profile: &str,
    ) -> Result<Self> {
        require(
            matches!(
                (mode, resource_profile),
                (InputMode::LocalRule, "no_llm") | (InputMode::LocalAi, "constrained" | "standard")
            ),
            "resource_profile",
        )?;
        Ok(Self {
            sources,
            workflow,
            mode,
            resource_profile: resource_profile.into(),
        })
    }
}

impl KuInputProvider for SharedKuExtractionInputs {
    fn implementation(&self, mode: InputMode) -> Option<[u8; 32]> {
        if mode != self.mode {
            return None;
        }
        let workflow = self.workflow.implementation_commitment().ok()?;
        let digest=artifact_sha256(&json!({"workflow":hex(&workflow),"planner_source":include_str!("ku_extraction.rs"),"resource_profile":self.resource_profile})).ok()?;
        decode_hex(&digest).ok()
    }
    fn check_access(
        &self,
        principal: [u8; 32],
        sources: &[[u8; 32]],
    ) -> std::result::Result<(), BaseServiceError> {
        self.sources.check_access(principal, sources)
    }
    fn resolve(
        &self,
        _: [u8; 32],
        _: &KuPrepareV1,
        _: &ConceptRegistryReaderLease,
        _: &ResourceBudgetV1,
    ) -> std::result::Result<KuResolvedInput, BaseServiceError> {
        Err(crate::ku_product::unavailable()) // Inference cannot run through the old synchronous port.
    }
    fn resolve_async<'a>(
        &'a self,
        principal: [u8; 32],
        request: &'a KuPrepareV1,
        registry: &'a ConceptRegistryReaderLease,
        budget: &'a ResourceBudgetV1,
        execution: KuExtractionExecution<'a>,
    ) -> Pin<
        Box<
            dyn Future<Output = std::result::Result<KuResolvedInput, BaseServiceError>> + Send + 'a,
        >,
    > {
        Box::pin(async move {
            let result=async {
                let started=std::time::Instant::now();
                require(request.input_mode==self.mode,"resource_profile")?;
                require(request.source_refs.len()==1,"source_scope_unsupported")?;
                let mut work=WorkBudget::new(budget.max_work_units,std::time::Duration::from_secs(if self.resource_profile=="standard" {120}else{30}),execution.canceled.clone())?;
                let source_ids:Vec<_>=request.source_refs.iter().map(|s|s.0).collect();
                self.sources.check_access(principal,&source_ids).map_err(|_|ExtractionError("source_unavailable"))?;
                verify_registry(registry,request.registry_release_root.0)?;
                let mut contexts=Vec::new();let mut source_objects=Vec::new();let mut total=0usize;
                for (i,id) in source_ids.iter().enumerate() {
                    let bytes=self.sources.read_source(principal,*id,(budget.max_bytes as usize).saturating_sub(total)).map_err(|_|ExtractionError("source_unavailable"))?;
                    total=total.checked_add(bytes.len()).ok_or(ExtractionError("resource"))?;
                    require(total<=budget.max_bytes as usize,"resource")?;work.charge(bytes.len())?;
                    let object=crate::ku_product::decode_object(&bytes).map_err(|_|ExtractionError("source_invalid"))?;
                    require(object.cid().into_bytes()==*id,"source_binding")?;
                    let source=SourceArtifact::from_validated(&object).map_err(|_|ExtractionError("source_invalid"))?;
                    require(source.source_kind==SourceArtifactKind::Text,"source_unsupported")?;
                    let text=std::str::from_utf8(&source.raw_bytes).map_err(|_|ExtractionError("source_invalid"))?;
                    // First host planner keeps each source whole. Oversize prose
                    // fails explicitly; it is never split at an unsafe semantic boundary.
                    require(!text.is_empty() && text.chars().count()<=8192,"source_context_limit")?;
                    let attempt=artifact_sha256(&json!({"operation":hex(&request.operation_id.0),"source":hex(id),"position":i,"principal":hex(&principal),"process":hex(&execution.process),"dataset":hex(&execution.dataset)}))?;
                    let span=json!({"start":0,"end":text.len(),"quote":text});
                    let options=lookup_options(text,registry,&mut work)?;
                    contexts.push(json!({"profile":"ku-extraction/1.0","attempt_id":attempt,"source_ref":hex(id),"source_text":text,
                        "registry_root":hex(&request.registry_release_root.0),"resource_profile":self.resource_profile,
                        "windows":[{"key":"focus","start":0,"end":text.len(),"role":"focus"}],
                        "required_units":[{"key":"source","span":span}],"options":options}));
                    source_objects.push(bytes);
                }
                let job=ExtractionJob {principal,operation:request.operation_id.0,process:execution.process,dataset:execution.dataset,contexts};
                let authority=NodeAuthority {sources:self.sources.as_ref(),registry,root:request.registry_release_root.0};
                let output=self.workflow.run_admitted(&job,&authority,execution.journal,budget.max_work_units,
                    budget.max_work_units-work.remaining(),started.elapsed().as_millis() as u64,execution.canceled).await?;
                let mut bindings=Vec::new();
                for (context,resolution) in job.contexts.iter().zip(&output.resolutions) {
                    for binding in resolution["bindings"].as_array().ok_or(ExtractionError("resolution"))? {
                        let option=context["options"].as_array().ok_or(ExtractionError("options"))?.iter().find(|o|o["key"]==binding["option"]).ok_or(ExtractionError("resolution"))?;
                        bindings.push(KuConceptBinding {label:option["lookup_label"].as_str().ok_or(ExtractionError("resolution"))?.into(),selected:Some(decode_hex(option["ccid"].as_str().unwrap_or("")).map_err(|_|ExtractionError("resolution"))?)});
                    }
                }
                Ok(KuResolvedInput {drafts:output.drafts,source_objects,bindings,needs_resolution:output.needs_resolution,extraction_budget:Some(output.budget)})
            }.await;
            result.map_err(|e: ExtractionError| match e.0 {
                "canceled" | "replay_binding" | "reconcile_required" | "interrupted" => {
                    crate::ku_product::conflict()
                }
                "resource"
                | "source_context_limit"
                | "memory_admission"
                | "call_tokens"
                | "call_budget"
                | "aggregate_budget"
                | "deadline" => BaseServiceError::new(
                    onebrain_base_contract::BaseErrorCodeV1::ResourceExhausted,
                    e.0,
                ),
                "source_unavailable" | "source_revoked" => crate::ku_product::not_found(),
                "journal_unavailable" | "journal_corrupt" => crate::ku_product::unknown(),
                _ => BaseServiceError::new(
                    onebrain_base_contract::BaseErrorCodeV1::DependencyUnavailable,
                    e.0,
                ),
            })
        })
    }
}

fn verify_registry(registry: &ConceptRegistryReaderLease, root: [u8; 32]) -> Result<()> {
    let status = registry.status();
    require(
        status.release_aggregate_root.as_deref() == Some(hex(&root).as_str())
            && status.release_id.is_some()
            && status.failure_kind.is_none()
            && status.backend == Some(ConceptRegistryBackendKind::IndexedOnDemand),
        "registry_unavailable",
    )
}

// Indexed Registry already bounds each collision range at 1,024 entries and
// errors instead of truncating. Admit that finite upper bound before each read.
fn lookup(
    label: &str,
    registry: &ConceptRegistryReaderLease,
    budget: &mut WorkBudget,
) -> Result<Vec<ku_core::concept_registry::ResolvedConcept>> {
    budget.charge(label.len() + 1024)?;
    match registry
        .resolve_checked(label)
        .map_err(|_| ExtractionError("registry_unavailable"))?
    {
        ResolveResult::Found(c) => Ok(vec![c]),
        ResolveResult::Ambiguous(c) => {
            require(c.len() <= 256, "registry_ambiguous_limit")?;
            Ok(c)
        }
        ResolveResult::NotFound | ResolveResult::Fuzzy(_) => Ok(vec![]),
    }
}

fn lookup_options(
    text: &str,
    registry: &ConceptRegistryReaderLease,
    budget: &mut WorkBudget,
) -> Result<Vec<Value>> {
    let token =
        regex::Regex::new(r#"[^\s\[\]()".,;!?]+"#).map_err(|_| ExtractionError("planner"))?;
    let tokens: Vec<_> = token.find_iter(text).collect();
    let mut ranges = std::collections::BTreeSet::new();
    for i in 0..tokens.len() {
        for n in 1..=4 {
            if i + n <= tokens.len() {
                ranges.insert((tokens[i].start(), tokens[i + n - 1].end()));
            }
        }
    }
    if let (Some(start), Some(end)) = (text.find('['), text.find(']')) {
        if start + 1 < end {
            ranges.insert((start + 1, end));
        }
    }
    let mut options = Vec::new();
    for (start, end) in ranges {
        let label = &text[start..end];
        if label.chars().count() > 256 {
            continue;
        }
        for c in lookup(label, registry, budget)? {
            require(options.len() < 256, "registry_ambiguous_limit")?;
            options.push(json!({"key":format!("o{}",options.len()),"ccid":hex(&c.ccid),"lookup_label":label,"mention":{"start":start,"end":end,"quote":label}}));
        }
    }
    Ok(options)
}

struct NodeAuthority<'a> {
    sources: &'a dyn KuExtractionSources,
    registry: &'a ConceptRegistryReaderLease,
    root: [u8; 32],
}
impl ExtractionAuthority for NodeAuthority<'_> {
    fn check_context(
        &self,
        job: &ExtractionJob,
        context: &Value,
        budget: &mut WorkBudget,
    ) -> Result<()> {
        verify_registry(self.registry, self.root)?;
        let id = decode_hex::<32>(context["source_ref"].as_str().unwrap_or(""))
            .map_err(|_| ExtractionError("source_binding"))?;
        self.sources
            .check_access(job.principal, &[id])
            .map_err(|_| ExtractionError("source_revoked"))?;
        require(
            context["registry_root"] == hex(&self.root),
            "registry_binding",
        )?;
        budget.charge(1)
    }
    fn resolve(
        &self,
        _: &ExtractionJob,
        context: &Value,
        candidate: &Value,
        budget: &mut WorkBudget,
    ) -> Result<Value> {
        let mut bindings = Vec::new();
        for concept in candidate["concepts"]
            .as_array()
            .ok_or(ExtractionError("candidate"))?
        {
            let label = concept["label"]
                .as_str()
                .ok_or(ExtractionError("candidate"))?;
            let candidates = lookup(label, self.registry, budget)?;
            // Ambiguous/unknown concepts and unbound unit transforms remain
            // unresolved; neither model_proposal nor first-match is authority.
            if candidates.len() != 1 {
                continue;
            }
            let c = &candidates[0];
            if c.category == ku_core::concept_registry::ConceptCategory::Unit {
                continue;
            }
            let options = context["options"]
                .as_array()
                .ok_or(ExtractionError("options"))?
                .iter()
                .filter(|o| o["mention"] == concept["evidence"])
                .collect::<Vec<_>>();
            if options.len() != 1 || options[0]["ccid"] != hex(&c.ccid) {
                continue;
            }
            let option = options[0];
            bindings.push(json!({"concept":concept["key"],"option":option["key"],"selection":"exact_label","provenance_sha256":artifact_sha256(&json!({"root":hex(&self.root),"option":option}))?}));
        }
        Ok(
            json!({"attempt_id":context["attempt_id"],"context_sha256":artifact_sha256(context)?,"bindings":bindings}),
        )
    }
    fn check_resolution(
        &self,
        job: &ExtractionJob,
        context: &Value,
        candidate: &Value,
        resolution: &Value,
        budget: &mut WorkBudget,
    ) -> Result<()> {
        require(
            self.resolve(job, context, candidate, budget)? == *resolution,
            "resolution_authority",
        )
    }
}
