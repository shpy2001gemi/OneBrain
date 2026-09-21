//! Deliberate private development inputs only. No Registry, save or publication.
use ku_encoder::extraction::{
    review_draft::DraftJob, ExtractionProvider, ManagedOllamaProvider, WorkBudget,
};
use serde_json::{json, Value};
use std::{
    io::Write,
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 {
        return Err("usage: review_draft_probe DEVELOPMENT_CASES.json NEW_PRIVATE_REPORT.jsonl; KU_OLLAMA_EXE, KU_OLLAMA_MODELS, KU_OLLAMA_MODEL".into());
    }
    let bytes = std::fs::read(&args[1])?;
    if bytes.len() > 65536 {
        return Err("case size bound".into());
    }
    let cases: Vec<Value> = serde_json::from_slice(&bytes)?;
    if cases.is_empty() || cases.len() > 16 {
        return Err("case count bound".into());
    }
    let mut report = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&args[2])?;
    let model = std::env::var("KU_OLLAMA_MODEL")?;
    let provider = ManagedOllamaProvider::open(
        std::env::var_os("KU_OLLAMA_EXE")
            .ok_or("missing executable")?
            .into(),
        std::env::var_os("KU_OLLAMA_MODELS")
            .ok_or("missing models")?
            .into(),
        &model,
        16 * 1024u64.pow(3),
        Arc::new(tokio::sync::Semaphore::new(1)),
    )?;
    let producer = ku_encoder::extraction::artifact_sha256(provider.manifest())?;
    for case in cases {
        let source = case["text"].as_str().ok_or("case text")?;
        let mut job = if std::env::var("KU_SELECTION").as_deref() == Ok("2") {
            DraftJob::new_selection_v2(source)?
        } else if std::env::var("KU_SELECTION").as_deref() == Ok("1") {
            DraftJob::new_selection(source)?
        } else {
            DraftJob::new(source)?
        };
        let mut traces = Vec::new();
        let started = Instant::now();
        while let Some((i, task)) = job.next(source)? {
            eprintln!("{} {:?} call {}", case["id"], task.kind, job.calls + 1);
            let mut trace = json!({"kind":task.kind,"request":task.data,"prompt":task.prompt()?,"schema":task.wire_schema()?,"input_tokens":provider.review_task_tokens(&task)?});
            job.reserve(i, &task, provider.review_task_tokens(&task)?)?;
            let now = Instant::now();
            let raw = provider.review_task(task).await;
            trace["elapsed_ms"] = json!(now.elapsed().as_millis() as u64);
            if let Ok(bytes) = &raw {
                trace["raw_response"] = json!(String::from_utf8_lossy(bytes));
            }
            if let Err(error) = &raw {
                trace["error"] = json!(error.0);
            }
            job.finish(
                source,
                raw,
                now.elapsed().as_millis() as u64,
                &producer,
                &mut WorkBudget::new(
                    1_000_000,
                    Duration::from_secs(30),
                    Arc::new(AtomicBool::new(false)),
                )?,
            )?;
            traces.push(trace);
        }
        eprintln!(
            "{} {} {:.2}s",
            case["id"],
            job.state,
            started.elapsed().as_secs_f64()
        );
        writeln!(
            report,
            "{}",
            json!({"case":case,"model":model,"provider":provider.manifest(),"seconds":started.elapsed().as_secs_f64(),"job":job,"traces":traces,"canonical_ku":false})
        )?;
        report.sync_all()?;
    }
    Ok(())
}
