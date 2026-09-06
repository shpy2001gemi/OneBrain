//! Host-controlled inference and replay. Journal/authority ports are privileged
//! dependencies; providers receive neither port and can only return raw proposals.
use super::compiler::compile;
use super::*;
use ku_core::foundation::semantic::SemanticFrameSet;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractionJob {
    pub principal: [u8; 32],
    pub operation: [u8; 32],
    pub process: [u8; 32],
    pub dataset: [u8; 32],
    /// Host-built contexts, never accepted from the inference provider.
    pub contexts: Vec<Value>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractionCheckpoint {
    pub binding: String,
    pub process: [u8; 32],
    pub phase: String,
    pub calls: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub work_charged: u64,
    pub elapsed_ms: u64,
    pub candidates: Vec<Value>,
    pub resolutions: Vec<Value>,
    pub context_calls: Vec<u32>,
    pub reason: String,
    pub contexts: Vec<Value>,
    pub attempts: Vec<Value>,
}

/// Uses the node's existing encrypted KU journal. Implementations must atomically
/// compare the prior binding/counters, enforce byte/item capacities and fsync each
/// checkpoint; no plaintext fallback, competing Vault writer or public namespace.
pub trait ExtractionJournal: Send + Sync {
    fn load(&self, job: &ExtractionJob) -> Result<Option<ExtractionCheckpoint>>;
    fn store(&self, job: &ExtractionJob, checkpoint: &ExtractionCheckpoint) -> Result<()>;
}

/// Host authorization, not provider attestations. Concrete node integration checks
/// immutable SourceArtifact bytes/current grant, signed pinned Registry generation,
/// complete lookup sets and separately authenticated review evidence.
pub trait ExtractionAuthority: Send + Sync {
    fn check_context(
        &self,
        job: &ExtractionJob,
        context: &Value,
        budget: &mut WorkBudget,
    ) -> Result<()>;
    fn resolve(
        &self,
        job: &ExtractionJob,
        context: &Value,
        candidate: &Value,
        budget: &mut WorkBudget,
    ) -> Result<Value>;
    fn check_resolution(
        &self,
        job: &ExtractionJob,
        context: &Value,
        candidate: &Value,
        resolution: &Value,
        budget: &mut WorkBudget,
    ) -> Result<()>;
}

pub struct ExtractionOutput {
    /// No partial SEM escapes if any context is unresolved.
    pub drafts: Vec<SemanticFrameSet>,
    pub needs_resolution: bool,
    pub resolutions: Vec<Value>,
    /// The same remaining work/deadline fence continues through KU preparation.
    pub budget: WorkBudget,
}

pub struct ExtractionWorkflow {
    gate: Semaphore,
    provider: Arc<dyn ExtractionProvider>,
    admitted_memory_bytes: u64,
}

impl ExtractionWorkflow {
    fn checkpoint(
        &self,
        job: &ExtractionJob,
        state: &mut ExtractionCheckpoint,
        journal: &dyn ExtractionJournal,
        max_ms: u64,
        budget: &mut WorkBudget,
        initial_work: u64,
        allowance: u64,
    ) -> Result<()> {
        let mut attempts = Vec::new();
        let provider_hash = hash(self.provider.manifest())?;
        for (i, context) in job.contexts.iter().enumerate() {
            let terminal = matches!(state.phase.as_str(), "failed" | "canceled" | "interrupted");
            let phase = if terminal {
                state.phase.as_str()
            } else if i < state.resolutions.len() {
                if state.phase == "validated" {
                    "validated"
                } else {
                    "resolving"
                }
            } else if i < state.candidates.len() {
                "candidate_recorded"
            } else if state.context_calls[i] > 0 {
                "extracting"
            } else {
                "admitted"
            };
            let reason = if !terminal {
                "none"
            } else {
                match state.reason.as_str() {
                    "canceled" => "canceled",
                    "interrupted" => "interrupted",
                    "source_revoked" => "source_revoked",
                    "resource" | "deadline" | "call_budget" | "call_tokens"
                    | "aggregate_budget" => "budget_exhausted",
                    "provider_failed" | "journal_unavailable" => "dependency_unavailable",
                    _ => "invalid_candidate",
                }
            };
            budget.charge(
                serde_json::to_vec(context)
                    .map_err(|_| ExtractionError("invalid_json"))?
                    .len(),
            )?;
            let mut attempt = json!({"profile":"ku-extraction-attempt/1.0","attempt_id":context["attempt_id"],
                "operation_id":hash(&json!({"principal":hex(&job.principal),"operation":hex(&job.operation)}))?,
                "principal_binding":hex(&job.principal),"process_generation":hex(&state.process),"dataset_generation":hex(&job.dataset),
                "source_ref":context["source_ref"],"context_sha256":hash(context)?,"registry_root":context["registry_root"],
                "bundle_sha256":Self::bundle_hash(),"provider_manifest_sha256":provider_hash,"phase":phase,
                "calls_reserved":state.calls,"input_tokens_reserved":state.input_tokens,"output_tokens_reserved":state.output_tokens,
                "work_units_charged":state.work_charged,"remaining_deadline_ms":max_ms.saturating_sub(state.elapsed_ms),"reason":reason});
            if let Some(candidate) = state.candidates.get(i) {
                attempt["candidate_sha256"] = hash(candidate)?.into();
            }
            if let Some(resolution) = state.resolutions.get(i) {
                attempt["resolution_sha256"] = hash(resolution)?.into();
            }
            check(&attempt, "Attempt", budget)?;
            attempts.push(attempt);
        }
        state.work_charged = initial_work + allowance - budget.remaining();
        for attempt in &mut attempts {
            attempt["work_units_charged"] = state.work_charged.into();
        }
        state.attempts = attempts;
        journal.store(job, state)
    }
    pub fn new(provider: Arc<dyn ExtractionProvider>, admitted_memory_bytes: u64) -> Result<Self> {
        let mut budget = WorkBudget::new(
            1_000_000,
            Duration::from_secs(1),
            Arc::new(AtomicBool::new(false)),
        )?;
        check(provider.manifest(), "ProviderManifest", &mut budget)?;
        let manifest = provider.manifest();
        require(
            manifest["peak_bytes_reservation"]
                .as_u64()
                .is_some_and(|n| n <= admitted_memory_bytes),
            "memory_admission",
        )?;
        require(
            manifest["schema_bundle_sha256"].as_str() == Some(Self::bundle_hash().as_str()),
            "bundle_binding",
        )?;
        if manifest["mode"] == "rules" {
            require(
                manifest["max_context_tokens"] == 0
                    && manifest.get("model_artifact_sha256").is_none()
                    && manifest.get("tokenizer_sha256").is_none(),
                "provider_identity",
            )?;
        } else {
            require(
                manifest["model_artifact_sha256"].is_string()
                    && manifest["tokenizer_sha256"].is_string()
                    && manifest["max_context_tokens"]
                        .as_u64()
                        .is_some_and(|n| n > 0),
                "provider_identity",
            )?;
        }
        if manifest["mode"] == "grammar" {
            require(manifest["grammar_sha256"].is_string(), "provider_identity")?;
        } else {
            require(
                manifest.get("grammar_sha256").is_none(),
                "provider_identity",
            )?;
        }
        let keywords = manifest["supported_schema_keywords"]
            .as_array()
            .ok_or(ExtractionError("provider_identity"))?;
        require(
            keywords
                .iter()
                .filter_map(Value::as_str)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == keywords.len(),
            "provider_identity",
        )?;
        Ok(Self {
            gate: Semaphore::new(1),
            provider,
            admitted_memory_bytes,
        })
    }
    pub fn bundle_hash() -> String {
        hex(&Sha256::digest(include_bytes!(
            "../../../../docs/specs/vnext/ku-encoder-v1/bundle.manifest.json"
        )))
    }
    pub fn native_source_hash() -> String {
        let mut hash = Sha256::new();
        for bytes in [
            include_bytes!("mod.rs").as_slice(),
            include_bytes!("schema.rs"),
            include_bytes!("compiler.rs"),
            include_bytes!("provider.rs"),
            include_bytes!("rules.rs"),
            include_bytes!("workflow.rs"),
            include_bytes!("../../../ku-ai/src/backend/ollama.rs"),
            include_bytes!("../../../Cargo.lock"),
        ] {
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
        hex(&hash.finalize())
    }
    pub fn implementation_commitment(&self) -> Result<[u8; 32]> {
        unhex(&hash(
            &json!({"bundle":Self::bundle_hash(),"native":Self::native_source_hash(),"provider":self.provider.manifest(),"memory":self.admitted_memory_bytes}),
        )?)
    }
    pub async fn run(
        &self,
        job: &ExtractionJob,
        authority: &dyn ExtractionAuthority,
        journal: &dyn ExtractionJournal,
        work_limit: u64,
        cancel: Arc<AtomicBool>,
    ) -> Result<ExtractionOutput> {
        self.run_admitted(job, authority, journal, work_limit, 0, 0, cancel)
            .await
    }
    /// Includes work/time already spent by the host on source custody and planning
    /// in this invocation. Replay adds these costs to prior durable counters.
    #[allow(clippy::too_many_arguments)]
    pub async fn run_admitted(
        &self,
        job: &ExtractionJob,
        authority: &dyn ExtractionAuthority,
        journal: &dyn ExtractionJournal,
        work_limit: u64,
        precharged_work: u64,
        pre_elapsed_ms: u64,
        cancel: Arc<AtomicBool>,
    ) -> Result<ExtractionOutput> {
        let _lease = self
            .gate
            .try_acquire()
            .map_err(|_| ExtractionError("provider_busy"))?;
        require(
            !job.contexts.is_empty() && job.contexts.len() <= 16,
            "job_chunks",
        )?;
        let profile = job.contexts[0]["resource_profile"]
            .as_str()
            .ok_or(ExtractionError("resource_profile"))?;
        require(
            matches!(profile, "no_llm" | "constrained" | "standard"),
            "resource_profile",
        )?;
        require(
            (profile == "no_llm") == (self.provider.manifest()["mode"] == "rules"),
            "no_llm_provider",
        )?;
        require(
            job.contexts
                .iter()
                .all(|c| c["resource_profile"] == profile),
            "job_binding",
        )?;
        let limits: Value = serde_json::from_str(include_str!(
            "../../../../docs/specs/vnext/ku-encoder-v1/profile.json"
        ))
        .map_err(|_| ExtractionError("schema"))?;
        let limits = &limits["resource_profiles"][profile];
        let max_ms = limits["deadline_ms"]
            .as_u64()
            .ok_or(ExtractionError("schema"))?;
        let started = Instant::now();
        let binding = hash(
            &json!({"job":job,"provider":self.provider.manifest(),"bundle":Self::bundle_hash(),"implementation":hex(&self.implementation_commitment()?)}),
        )?;
        let prior = journal.load(job)?;
        let mut state = match prior {
            Some(mut state) => {
                if state.process != job.process {
                    state.phase = "interrupted".into();
                    state.reason = "interrupted".into();
                    state.elapsed_ms = max_ms;
                    for attempt in &mut state.attempts {
                        attempt["phase"] = "interrupted".into();
                        attempt["reason"] = "interrupted".into();
                        attempt["remaining_deadline_ms"] = 0.into();
                    }
                    journal.store(job, &state)?;
                    return Err(ExtractionError("interrupted"));
                }
                require(state.binding == binding, "replay_binding")?;
                require(
                    !matches!(
                        state.phase.as_str(),
                        "extracting" | "failed" | "canceled" | "interrupted"
                    ),
                    "reconcile_required",
                )?;
                require(state.contexts == job.contexts, "replay_binding")?;
                state
            }
            None => ExtractionCheckpoint {
                binding,
                process: job.process,
                phase: "admitted".into(),
                calls: 0,
                input_tokens: 0,
                output_tokens: 0,
                work_charged: 0,
                elapsed_ms: 0,
                candidates: vec![],
                resolutions: vec![],
                context_calls: vec![0; job.contexts.len()],
                reason: "none".into(),
                contexts: job.contexts.clone(),
                attempts: vec![],
            },
        };
        state.elapsed_ms = state
            .elapsed_ms
            .checked_add(pre_elapsed_ms)
            .ok_or(ExtractionError("resource"))?;
        state.work_charged = state
            .work_charged
            .checked_add(precharged_work)
            .ok_or(ExtractionError("resource"))?;
        require(
            state.elapsed_ms < max_ms && state.work_charged < work_limit,
            "resource",
        )?;
        require(
            state.context_calls.len() == job.contexts.len()
                && state.candidates.len() <= job.contexts.len()
                && state.resolutions.len() <= state.candidates.len(),
            "checkpoint_shape",
        )?;
        let initial_work = state.work_charged;
        let prior_ms = state.elapsed_ms;
        let allowance = work_limit - initial_work;
        let mut budget = WorkBudget::new(
            allowance,
            Duration::from_millis(max_ms - prior_ms),
            cancel.clone(),
        )?;
        let execute=async {
            for context in &job.contexts {check(context,"Context",&mut budget)?;authority.check_context(job,context,&mut budget)?;}
            validate_manifest(&job.contexts,&mut budget)?;
            self.checkpoint(job,&mut state,journal,max_ms,&mut budget,initial_work,allowance)?;
            let mut drafts=Vec::new();let mut unresolved=false;
            for (i,context) in job.contexts.iter().enumerate() {
                if state.candidates.len()<=i && profile=="no_llm" {
                    state.candidates.push(super::rules::candidate(context,&mut budget)?);
                    state.phase="candidate_recorded".into();state.work_charged=initial_work+allowance-budget.remaining();
                    state.elapsed_ms=prior_ms+started.elapsed().as_millis() as u64;
                    authority.check_context(job,context,&mut budget)?;self.checkpoint(job,&mut state,journal,max_ms,&mut budget,initial_work,allowance)?;
                }
                if state.candidates.len()<=i {
                    let input=provider_input(context,&mut budget)?;
                    let mut errors=vec![];
                    loop {
                        budget.charge(1)?;
                        require(state.context_calls[i]<2 && state.calls<limits["job_calls"].as_u64().unwrap_or(0) as u32,"call_budget")?;
                        let elapsed=prior_ms+started.elapsed().as_millis() as u64;
                        require(elapsed<max_ms,"deadline")?;
                        let request=ProviderRequest {input:input.clone(),repair_errors:errors.clone(),deadline:Duration::from_millis(max_ms-elapsed),
                            output_tokens:limits["call_output_tokens"].as_u64().unwrap_or(0) as u32,max_response_bytes:1_048_576};
                        let input_tokens=self.provider.input_tokens(&request)? as u64;
                        let output_tokens=request.output_tokens as u64;
                        require(input_tokens<=limits["call_input_tokens"].as_u64().unwrap_or(0) &&
                            input_tokens+output_tokens<=self.provider.manifest()["max_context_tokens"].as_u64().unwrap_or(0),"call_tokens")?;
                        require(state.input_tokens+input_tokens<=limits["job_input_tokens"].as_u64().unwrap_or(0) &&
                            state.output_tokens+output_tokens<=limits["job_output_tokens"].as_u64().unwrap_or(0),"aggregate_budget")?;
                        authority.check_context(job,context,&mut budget)?;
                        state.calls+=1;state.context_calls[i]+=1;state.input_tokens+=input_tokens;state.output_tokens+=output_tokens;
                        state.phase="extracting".into();state.work_charged=initial_work+allowance-budget.remaining();state.elapsed_ms=elapsed;
                        self.checkpoint(job,&mut state,journal,max_ms,&mut budget,initial_work,allowance)?; // durable reservation precedes model dispatch
                        let deadline=request.deadline;
                        let extraction=self.provider.extract(request);
                        let canceled=async {
                            loop {
                                if cancel.load(Ordering::Acquire) {break;}
                                tokio::time::sleep(Duration::from_millis(10)).await;
                            }
                        };
                        let raw=tokio::select! {
                            result=tokio::time::timeout(deadline,extraction)=>result.map_err(|_|ExtractionError("deadline"))??,
                            _=canceled=>return Err(ExtractionError("canceled")),
                        };
                        budget.charge(1)?; // Reject canceled/deadline callbacks before parsing or journal updates.
                        match parse(&raw,&mut budget).and_then(|candidate| {check(&candidate,"Candidate",&mut budget)?;Ok(candidate)}) {
                            Ok(candidate)=>{
                                state.candidates.push(candidate);state.phase="candidate_recorded".into();
                                state.work_charged=initial_work+allowance-budget.remaining();state.elapsed_ms=prior_ms+started.elapsed().as_millis() as u64;
                                authority.check_context(job,context,&mut budget)?;self.checkpoint(job,&mut state,journal,max_ms,&mut budget,initial_work,allowance)?;break;
                            }
                            Err(e) if state.context_calls[i]<2 && !matches!(e.0,"resource"|"canceled"|"deadline")=>errors=vec![e.0],
                            Err(e)=>return Err(e),
                        }
                    }
                }
                authority.check_context(job,context,&mut budget)?;
                if state.resolutions.len()<=i {
                    state.resolutions.push(authority.resolve(job,context,&state.candidates[i],&mut budget)?);
                    state.phase="resolving".into();state.work_charged=initial_work+allowance-budget.remaining();
                    state.elapsed_ms=prior_ms+started.elapsed().as_millis() as u64;self.checkpoint(job,&mut state,journal,max_ms,&mut budget,initial_work,allowance)?;
                }
                authority.check_resolution(job,context,&state.candidates[i],&state.resolutions[i],&mut budget)?;
                match compile(context,&state.candidates[i],&state.resolutions[i],&mut budget)? {
                    Some(sem)=>drafts.push(sem),None=>unresolved=true,
                }
            }
            require(drafts.iter().map(|s|s.statements.len()).sum::<usize>()<=256,"job_statements")?;
            if unresolved {drafts.clear();} else {drafts=vec![assemble(drafts,&mut budget)?];}
            for context in &job.contexts {authority.check_context(job,context,&mut budget)?;}
            budget.charge(1)?;
            state.phase="validated".into();state.work_charged=initial_work+allowance-budget.remaining();
            state.elapsed_ms=prior_ms+started.elapsed().as_millis() as u64;
            self.checkpoint(job,&mut state,journal,max_ms,&mut budget,initial_work,allowance)?;
            Ok(ExtractionOutput {drafts,needs_resolution:unresolved,resolutions:state.resolutions.clone(),budget:budget.clone()})
        }.await;
        if let Err(e) = execute {
            state.phase = if e.0 == "canceled" {
                "canceled"
            } else {
                "failed"
            }
            .into();
            state.reason = e.0.into();
            state.work_charged = initial_work + allowance - budget.remaining();
            state.elapsed_ms = prior_ms + started.elapsed().as_millis() as u64;
            // A canceled/exhausted budget cannot authorize more source processing.
            // Update already-bound attempts without hashing or interpreting output.
            for attempt in &mut state.attempts {
                attempt["phase"] = state.phase.clone().into();
                attempt["reason"] = match e.0 {
                    "canceled" => "canceled",
                    "source_revoked" => "source_revoked",
                    "resource" | "deadline" => "budget_exhausted",
                    _ => "invalid_candidate",
                }
                .into();
                attempt["remaining_deadline_ms"] = max_ms.saturating_sub(state.elapsed_ms).into();
                attempt["work_units_charged"] = state.work_charged.into();
            }
            journal.store(job, &state)?;
            return Err(e);
        }
        execute
    }
}

// A job has one immutable source and one pinned interpretation context. Focus
// intervals (not supporting context windows) define ordered, disjoint scope.
pub(super) fn validate_manifest(contexts: &[Value], budget: &mut WorkBudget) -> Result<()> {
    let first = &contexts[0];
    let mut attempts = std::collections::BTreeSet::new();
    let mut focus_end = 0;
    let mut unit_end = 0;
    for context in contexts {
        budget.charge(1)?;
        require(
            attempts.insert(
                context["attempt_id"]
                    .as_str()
                    .ok_or(ExtractionError("job_binding"))?,
            ),
            "duplicate_attempt",
        )?;
        for field in [
            "source_ref",
            "source_text",
            "registry_root",
            "profile",
            "resource_profile",
        ] {
            require(context[field] == first[field], "job_binding")?;
        }
        for window in context["windows"]
            .as_array()
            .ok_or(ExtractionError("windows"))?
            .iter()
            .filter(|w| w["role"] == "focus")
        {
            budget.charge(1)?;
            let start = window["start"]
                .as_u64()
                .ok_or(ExtractionError("window_bounds"))?;
            let end = window["end"]
                .as_u64()
                .ok_or(ExtractionError("window_bounds"))?;
            require(start >= focus_end && end > start, "overlapping_focus")?;
            focus_end = end;
        }
        for unit in context["required_units"]
            .as_array()
            .ok_or(ExtractionError("coverage"))?
        {
            budget.charge(1)?;
            let start = unit["span"]["start"]
                .as_u64()
                .ok_or(ExtractionError("span_bounds"))?;
            let end = unit["span"]["end"]
                .as_u64()
                .ok_or(ExtractionError("span_bounds"))?;
            require(start >= unit_end && end > start, "overlapping_coverage")?;
            unit_end = end;
        }
    }
    Ok(())
}

pub(super) fn assemble(
    chunks: Vec<SemanticFrameSet>,
    budget: &mut WorkBudget,
) -> Result<SemanticFrameSet> {
    use ku_core::foundation::semantic::{StatementId, TermRef};
    let mut result = SemanticFrameSet { statements: vec![] };
    for chunk in chunks {
        let offset = result.statements.len() as u32;
        let rebase = |term: &mut TermRef| {
            if let TermRef::Statement(id) = term {
                id.0 += offset;
            }
        };
        for mut statement in chunk.statements {
            budget.charge(1 + statement.arguments.len())?;
            require(statement.constraints.is_empty(), "unsupported_constraint")?;
            statement.statement_id = StatementId(statement.statement_id.0 + offset);
            for term in &mut statement.arguments {
                rebase(term);
            }
            let q = &mut statement.qualifiers;
            if let Some(id) = &mut q.condition {
                id.0 += offset;
            }
            for term in [&mut q.time, &mut q.location, &mut q.perspective]
                .into_iter()
                .flatten()
            {
                rebase(term);
            }
            result.statements.push(statement);
        }
    }
    let encoded = result
        .canonical_bytes()
        .map_err(|_| ExtractionError("sem_shape"))?;
    budget.charge(encoded.len())?;
    Ok(result)
}

pub(crate) fn provider_input(context: &Value, budget: &mut WorkBudget) -> Result<Value> {
    let raw = context["source_text"]
        .as_str()
        .ok_or(ExtractionError("source_text"))?;
    let mut windows = Vec::new();
    for w in context["windows"]
        .as_array()
        .ok_or(ExtractionError("windows"))?
    {
        let mut window = w.clone();
        let start = w["start"]
            .as_u64()
            .ok_or(ExtractionError("window_bounds"))? as usize;
        let end = w["end"].as_u64().ok_or(ExtractionError("window_bounds"))? as usize;
        let text = raw
            .get(start..end)
            .ok_or(ExtractionError("window_bounds"))?;
        budget.charge(text.len())?;
        window["text"] = text.into();
        windows.push(window);
    }
    let options = context["options"]
        .as_array()
        .ok_or(ExtractionError("options"))?
        .iter()
        .map(|o| {
            let mut v =
                json!({"key":o["key"],"lookup_label":o["lookup_label"],"mention":o["mention"]});
            if let Some(d) = o.get("description") {
                v["description"] = d.clone();
            }
            v
        })
        .collect::<Vec<_>>();
    let input = json!({"profile":context["profile"],"attempt_id":context["attempt_id"],"context_sha256":hash(context)?,"windows":windows,
        "required_units":context["required_units"],"options":options});
    check(&input, "ProviderInput", budget)?;
    Ok(input)
}
