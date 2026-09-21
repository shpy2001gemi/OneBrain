//! Development-only Ollama chat transport for the real DraftJob scheduler.
//! No production model admission, exact tokenizer preflight, Registry or KU writes.
use ku_encoder::extraction::{
    artifact_sha256, review_draft::DraftJob, ExtractionError, WorkBudget,
};
use serde::Serialize;
use serde_json::{json, value::RawValue, Value};
use std::{
    io::Write,
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};

#[derive(Serialize)]
struct ChatBody {
    #[serde(flatten)]
    fields: Value,
    // Preserve schema property order, just as the managed provider does.
    format: Box<RawValue>,
}

async fn bounded_json(request: reqwest::RequestBuilder) -> Result<Value, ExtractionError> {
    let mut response = request
        .send()
        .await
        .map_err(|_| ExtractionError("chat_transport"))?;
    if !response.status().is_success() {
        return Err(ExtractionError("chat_http_status"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| ExtractionError("chat_response"))?
    {
        if bytes.len().saturating_add(chunk.len()) > 1_048_576 {
            return Err(ExtractionError("chat_response_bytes"));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| ExtractionError("chat_json"))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 5 {
        return Err("usage: review_chat_probe DEVELOPMENT_CASES.json NEW_PRIVATE_REPORT.jsonl INSTALLED_MODEL LOOPBACK_PORT".into());
    }
    let model = args[3].to_str().ok_or("model encoding")?;
    let port: u16 = args[4].to_str().ok_or("port encoding")?.parse()?;
    if port == 0 {
        return Err("invalid port".into());
    }
    let base = format!("http://127.0.0.1:{port}");
    let bytes = std::fs::read(&args[1])?;
    if bytes.len() > 65_536 {
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
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .build()?;
    let tags = bounded_json(
        client
            .get(format!("{base}/api/tags"))
            .timeout(Duration::from_secs(10)),
    )
    .await?;
    let record = tags["models"]
        .as_array()
        .ok_or("model list")?
        .iter()
        .find(|m| m["name"] == model)
        .ok_or("model not installed; this probe does not download models")?;
    let show = bounded_json(
        client
            .post(format!("{base}/api/show"))
            .json(&json!({"model":model}))
            .timeout(Duration::from_secs(10)),
    )
    .await?;
    let version = bounded_json(
        client
            .get(format!("{base}/api/version"))
            .timeout(Duration::from_secs(10)),
    )
    .await?;
    let provider = json!({"transport":"development_ollama_chat","model_record":record,
        "template":show["template"],"parameters":show["parameters"],
        "details":show["details"],"capabilities":show["capabilities"],"ollama":version,
        "think":false,"production_admitted":false,
        "input_accounting":"reserve full 6144-token ceiling; check reported usage after response; no exact tokenizer preflight"});
    let producer = artifact_sha256(&provider)?;
    for case in cases {
        // Human assessment fields in the case file are deliberately not sent.
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
            let system = task.system_prompt();
            let body = ChatBody {
                fields: json!({"model":model,"stream":false,"think":false,"keep_alive":0,
                    "messages":[{"role":"system","content":system},
                        {"role":"user","content":serde_json::to_string(&task.data)?}],
                    "options":{"num_ctx":8192,"num_predict":2048,"temperature":0,"seed":1,"num_gpu":0}}),
                format: serde_json::from_str(&task.wire_schema()?)?,
            };
            let wire_body = serde_json::to_string(&body)?;
            if wire_body.len() > 262_144 {
                return Err("request size bound".into());
            }
            eprintln!(
                "{} {} {:?} call {}",
                model,
                case["id"],
                task.kind,
                job.calls + 1
            );
            let mut trace = json!({"kind":task.kind,"wire_request":wire_body,
                "input_token_reservation":6144,"exact_input_tokens_preflight":null});
            job.reserve(i, &task, 6144)?;
            let now = Instant::now();
            let reply = bounded_json(
                client
                    .post(format!("{base}/api/chat"))
                    .header(reqwest::header::CONTENT_TYPE, "application/json")
                    .body(wire_body)
                    .timeout(task.deadline),
            )
            .await;
            let raw = match reply {
                Err(e) => Err(e),
                Ok(reply) => {
                    let thinking = reply["message"]["thinking"].as_str().unwrap_or("");
                    trace["thinking_chars"] = json!(thinking.chars().count());
                    for key in [
                        "done",
                        "done_reason",
                        "prompt_eval_count",
                        "eval_count",
                        "total_duration",
                        "load_duration",
                        "prompt_eval_duration",
                        "eval_duration",
                    ] {
                        trace[key] = reply[key].clone();
                    }
                    trace["raw_response"] = reply["message"]["content"].clone();
                    if reply["done"] != true || reply["done_reason"] == "length" {
                        Err(ExtractionError("chat_incomplete"))
                    } else if !thinking.is_empty() {
                        Err(ExtractionError("thinking_not_disabled"))
                    } else if reply["prompt_eval_count"]
                        .as_u64()
                        .map_or(true, |n| n > 6144)
                        || reply["eval_count"].as_u64().map_or(true, |n| n > 2048)
                    {
                        Err(ExtractionError("chat_token_budget"))
                    } else {
                        reply["message"]["content"]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .map(|s| s.as_bytes().to_vec())
                            .ok_or(ExtractionError("chat_empty_content"))
                    }
                }
            };
            let elapsed_ms = now.elapsed().as_millis() as u64;
            trace["elapsed_ms"] = json!(elapsed_ms);
            if let Err(error) = &raw {
                trace["error"] = json!(error.0);
            }
            job.finish(
                source,
                raw,
                elapsed_ms,
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
            "{} {} {} {:.2}s",
            model,
            case["id"],
            job.state,
            started.elapsed().as_secs_f64()
        );
        writeln!(
            report,
            "{}",
            json!({"case":case,"model":model,"provider":provider,
            "seconds":started.elapsed().as_secs_f64(),"job":job,"traces":traces,"canonical_ku":false})
        )?;
        report.sync_all()?;
    }
    Ok(())
}
