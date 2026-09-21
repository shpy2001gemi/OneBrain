//! Opt-in development runner: real provider and shared workflow, no Web/Vault.
//! Only use deliberate non-sensitive development inputs. Reports contain source
//! and model proposals; choose a private output directory. No Registry authority
//! or production/model-qualification claims are made by this synthetic context.
use async_trait::async_trait;
use ku_encoder::extraction::*;
use serde_json::{json, Value};
use std::{
    fs::OpenOptions,
    io::Write,
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::Instant,
};

#[derive(Default)]
struct Journal(Mutex<Option<ExtractionCheckpoint>>);
impl ExtractionJournal for Journal {
    fn load(&self, _: &ExtractionJob) -> Result<Option<ExtractionCheckpoint>, ExtractionError> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn store(
        &self,
        _: &ExtractionJob,
        state: &ExtractionCheckpoint,
    ) -> Result<(), ExtractionError> {
        *self.0.lock().unwrap() = Some(state.clone());
        Ok(())
    }
}
struct NoRegistryAuthority;
impl ExtractionAuthority for NoRegistryAuthority {
    fn check_context(
        &self,
        _: &ExtractionJob,
        _: &Value,
        b: &mut WorkBudget,
    ) -> Result<(), ExtractionError> {
        b.charge(1)
    }
    fn resolve(
        &self,
        _: &ExtractionJob,
        c: &Value,
        _: &Value,
        b: &mut WorkBudget,
    ) -> Result<Value, ExtractionError> {
        b.charge(1)?;
        Ok(json!({"attempt_id":c["attempt_id"],"context_sha256":artifact_sha256(c)?,"bindings":[]}))
    }
    fn check_resolution(
        &self,
        _: &ExtractionJob,
        _: &Value,
        _: &Value,
        r: &Value,
        b: &mut WorkBudget,
    ) -> Result<(), ExtractionError> {
        b.charge(1)?;
        if r["bindings"] != json!([]) {
            return Err(ExtractionError("probe_authority"));
        }
        Ok(())
    }
}
struct TracedProvider {
    inner: Arc<dyn ExtractionProvider>,
    calls: Mutex<Vec<Value>>,
}
#[async_trait]
impl ExtractionProvider for TracedProvider {
    fn manifest(&self) -> &Value {
        self.inner.manifest()
    }
    fn input_tokens(&self, r: &ProviderRequest) -> Result<u32, ExtractionError> {
        self.inner.input_tokens(r)
    }
    async fn extract(&self, r: ProviderRequest) -> Result<Vec<u8>, ExtractionError> {
        let mut trace = json!({"input_tokens":self.inner.input_tokens(&r)?,"reserved_output_tokens":r.output_tokens,"repair_errors":r.repair_errors,"provider_input":r.input});
        let start = Instant::now();
        let result = self.inner.extract(r).await;
        trace["elapsed_ms"] = json!(start.elapsed().as_millis() as u64);
        match &result {
            Ok(raw) => {
                trace["response_bytes"] = json!(raw.len());
                trace["raw_response"] = json!(String::from_utf8_lossy(raw));
            }
            Err(e) => trace["error"] = json!(e.0),
        }
        self.calls.lock().unwrap().push(trace);
        result
    }
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 || args[1] != "--development-inputs" {
        return Err("Usage: extraction_probe --development-inputs CASES.json NEW_REPORT.jsonl; set KU_OLLAMA_EXE, KU_OLLAMA_MODELS, optionally KU_OLLAMA_MODEL".into());
    }
    let bytes = std::fs::read(&args[2])?;
    if bytes.len() > 65536 {
        return Err("development input too large".into());
    }
    let cases: Vec<Value> = serde_json::from_slice(&bytes)?;
    if cases.is_empty() || cases.len() > 16 {
        return Err("use 1..16 cases".into());
    }
    for case in &cases {
        let s = case["text"].as_str().ok_or("missing text")?;
        if s.is_empty() || s.len() > 8192 {
            return Err("invalid text size".into());
        }
    }
    let mut report = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[3])?;
    let model = std::env::var("KU_OLLAMA_MODEL").unwrap_or_else(|_| "qwen3:8b".into());
    let memory = 16 * 1024u64.pow(3);
    let inner = Arc::new(ManagedOllamaProvider::open(
        std::env::var_os("KU_OLLAMA_EXE")
            .ok_or("set KU_OLLAMA_EXE")?
            .into(),
        std::env::var_os("KU_OLLAMA_MODELS")
            .ok_or("set KU_OLLAMA_MODELS")?
            .into(),
        &model,
        memory,
        Arc::new(tokio::sync::Semaphore::new(1)),
    )?);
    for (i, case) in cases.iter().enumerate() {
        let text = case["text"].as_str().unwrap();
        let context = json!({"profile":"ku-extraction/1.0","attempt_id":artifact_sha256(&json!(["probe",i,text]))?,"source_ref":artifact_sha256(&json!(text))?,"source_text":text,"registry_root":"00".repeat(32),"resource_profile":"standard","windows":[{"key":"focus","start":0,"end":text.len(),"role":"focus"}],"required_units":[{"key":"source","span":{"start":0,"end":text.len(),"quote":text}}],"options":[]});
        let provider = Arc::new(TracedProvider {
            inner: inner.clone(),
            calls: Mutex::new(vec![]),
        });
        let workflow = ExtractionWorkflow::new_experimental_ollama(provider.clone(), memory)?;
        let journal = Journal::default();
        let job = ExtractionJob {
            principal: [1; 32],
            operation: [i as u8 + 1; 32],
            process: [3; 32],
            dataset: [4; 32],
            contexts: vec![context],
        };
        eprintln!("Development case {} / {} started", i + 1, cases.len());
        let start = Instant::now();
        let result = workflow
            .run(
                &job,
                &NoRegistryAuthority,
                &journal,
                1_000_000,
                Arc::new(AtomicBool::new(false)),
            )
            .await;
        let checkpoint = journal.0.lock().unwrap().clone();
        let status = match &result {
            Ok(_) => "validated_without_registry_binding",
            Err(_) => "failed",
        };
        let row = json!({"scope":"development-only; no Registry binding, save, publication or qualification","case":case,"model":model,"provider":provider.manifest(),"status":status,"elapsed_ms":start.elapsed().as_millis() as u64,"error":result.as_ref().err().map(|e|e.0),"calls":provider.calls.lock().unwrap().clone(),"checkpoint":checkpoint});
        writeln!(report, "{}", serde_json::to_string(&row)?)?;
        report.sync_all()?;
        eprintln!(
            "Development case {} finished: {} ({:.1}s), error {:?}",
            i + 1,
            status,
            start.elapsed().as_secs_f64(),
            result.as_ref().err().map(|e| e.0)
        );
    }
    Ok(())
}
