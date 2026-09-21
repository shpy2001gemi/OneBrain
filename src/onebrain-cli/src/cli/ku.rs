//! Thin local KU client. The node owns consent, preparation, save and recovery.
use std::io::{BufRead, Read, Write};
use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, Subcommand};
use onebrain_base_contract::ku::*;
use onebrain_base_contract::ku_payload::{decode_hex, KuPayload, MAX_KU_PAYLOAD_BYTES};
use reqwest::{Method, Url};
use serde_json::{json, Value};

#[derive(Debug, Args)]
pub(crate) struct KuArgs {
    #[arg(long, global = true, default_value = "http://127.0.0.1:4280")]
    api_url: String,
    /// Prefer ONEBRAIN_API_TOKEN to keep the token out of shell history.
    #[arg(long, global = true)]
    api_token: Option<String>,
    #[arg(long, global = true, default_value_t = 256, value_parser = clap::value_parser!(u32).range(1..=256))]
    max_items: u32,
    #[arg(long, global = true, default_value_t = 1_048_576, value_parser = clap::value_parser!(u64).range(32768..=1048576))]
    max_bytes: u64,
    #[arg(long, global = true, default_value_t = 1_000_000, value_parser = clap::value_parser!(u64).range(1..=1000000))]
    max_work_units: u64,
    #[command(subcommand)]
    command: KuCommand,
}

#[derive(Debug, Subcommand)]
enum KuCommand {
    /// Reserve only; retain the operation ID for preparation and reconciliation.
    Reserve,
    /// Prepare host-admitted inputs; never saves or publishes.
    Prepare(PayloadFile),
    /// Read the exact stored preparation without rerunning the encoder.
    Preview(Operation),
    /// Explicitly save the exact ready object set privately.
    Save(PayloadFile),
    /// Read an exact authorized local ObjectCID and its original bytes.
    Get {
        #[arg(long, value_parser = hex32)]
        object_cid: String,
    },
    /// Page a bounded local snapshot; preserve its opaque continuation.
    List(Page),
    /// Search the local derived index with explicit partial coverage.
    Search {
        #[arg(long)]
        query: String,
        #[command(flatten)]
        page: Page,
    },
    /// Prepare an immutable successor; save remains a separate action.
    Revise(PayloadFile),
    /// Already-public exchange or separately authorized encrypted Base archive.
    Export(PayloadFile),
    /// Inspect service readiness or one recorded operation outcome.
    Status {
        #[arg(long, value_parser = hex32)]
        operation_id: Option<String>,
    },
    /// Type the exact operation ID to cancel eligible staging; no committed undo.
    Cancel(Operation),
    /// Read/repair the durable recorded outcome without rerunning extraction.
    Reconcile(Operation),
}

#[derive(Debug, Args)]
struct PayloadFile {
    /// Exact generated DTO JSON, not a source document or draft proposal.
    #[arg(long)]
    payload_file: PathBuf,
}
#[derive(Debug, Args)]
struct Operation {
    #[arg(long, value_parser = hex32)]
    operation_id: String,
}
#[derive(Debug, Args)]
struct Page {
    #[arg(long, default_value_t = 100, value_parser = clap::value_parser!(u16).range(1..=256))]
    limit: u16,
    #[arg(long)]
    continuation: Option<String>,
}

fn hex32(value: &str) -> Result<String, String> {
    decode_hex::<32>(value)
        .map(|_| value.to_owned())
        .map_err(|_| "expected exactly 64 lowercase hexadecimal characters".into())
}

fn dto<T: KuPayload>(bytes: &[u8]) -> Result<Value, String> {
    let value = T::decode(bytes).map_err(|_| "invalid or over-budget generated KU DTO")?;
    serde_json::to_value(value).map_err(|_| "cannot encode KU DTO".into())
}

fn file_dto<T: KuPayload>(file: &PayloadFile) -> Result<Value, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(&file.payload_file)
        .map_err(|_| "cannot open --payload-file")?
        .take(MAX_KU_PAYLOAD_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read --payload-file")?;
    dto::<T>(&bytes)
}

fn page_value(page: &Page) -> Value {
    let mut payload = json!({"limit":page.limit});
    if let Some(token) = &page.continuation {
        payload["continuation"] = json!(token);
    }
    payload
}

impl KuCommand {
    fn request(&self) -> Result<Option<(&'static str, Value)>, String> {
        let (name, payload) = match self {
            Self::Reserve | Self::Status { operation_id: None } => return Ok(None),
            Self::Prepare(file) => ("prepare", file_dto::<KuPrepareV1>(file)?),
            Self::Revise(file) => ("revise", file_dto::<KuReviseV1>(file)?),
            Self::Save(file) => ("save", file_dto::<KuSaveV1>(file)?),
            Self::Export(file) => ("export", file_dto::<KuExportV1>(file)?),
            Self::Preview(op) => ("preview", json!({"operation_id":op.operation_id})),
            Self::Cancel(op) => ("cancel", json!({"operation_id":op.operation_id})),
            Self::Reconcile(op) => ("reconcile", json!({"operation_id":op.operation_id})),
            Self::Status {
                operation_id: Some(id),
            } => ("status", json!({"operation_id":id})),
            Self::Get { object_cid } => ("get", json!({"object_cid":object_cid})),
            Self::List(page) => (
                "list",
                dto::<KuListV1>(&serde_json::to_vec(&page_value(page)).unwrap())?,
            ),
            Self::Search { query, page } => {
                let mut value = page_value(page);
                value["query"] = json!(query);
                (
                    "search",
                    dto::<KuSearchV1>(&serde_json::to_vec(&value).unwrap())?,
                )
            }
        };
        Ok(Some((name, payload)))
    }
}

struct Client {
    http: reqwest::Client,
    base: Url,
    token: String,
    max_bytes: usize,
}

impl Client {
    fn new(args: &KuArgs) -> Result<Self, String> {
        let base = Url::parse(&args.api_url).map_err(|_| "invalid local API URL")?;
        let host = base.host_str().unwrap_or("").trim_matches(['[', ']']);
        let loopback = host == "localhost"
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback());
        if !loopback
            || !matches!(base.scheme(), "http" | "https")
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
            || base.path() != "/"
        {
            return Err("KU requires a loopback http(s) API origin without credentials, path, query or fragment".into());
        }
        let token = args
            .api_token
            .clone()
            .or_else(|| std::env::var("ONEBRAIN_API_TOKEN").ok())
            .filter(|v| !v.trim().is_empty())
            .ok_or("set ONEBRAIN_API_TOKEN or pass --api-token")?;
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(660))
            .build()
            .map_err(|_| "cannot create local API client")?;
        Ok(Self {
            http,
            base,
            token,
            max_bytes: args.max_bytes as usize,
        })
    }

    async fn call(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value, String> {
        let operation = body
            .as_ref()
            .and_then(|v| v.pointer("/request/operation"))
            .and_then(Value::as_str)
            .unwrap_or(if path.ends_with("reservations") {
                "reserve"
            } else {
                "status"
            })
            .to_owned();
        let url = self.base.join(path).map_err(|_| "invalid API path")?;
        let mut request = self.http.request(method, url).bearer_auth(&self.token);
        if let Some(body) = body {
            let bytes = serde_json::to_vec(&body).map_err(|_| "cannot encode request")?;
            if bytes.len() > self.max_bytes {
                return Err("request exceeds --max-bytes".into());
            }
            request = request
                .header("Content-Type", "application/json")
                .body(bytes);
        }
        let unknown = "local API transport outcome unknown; refresh status and reconcile the original operation before retry; no automatic replay";
        let mut response = request.send().await.map_err(|_| unknown)?;
        let status = response.status();
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| unknown)? {
            if chunk.len() > self.max_bytes.saturating_sub(bytes.len()) {
                return Err(
                    "response exceeds --max-bytes; reconcile original operation before retry"
                        .into(),
                );
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| unknown)?;
        // Never render a token even if a malformed host reflects it in a response.
        if contains_secret(&value, &self.token) {
            return Err(
                "API response reflected credentials; reconcile original operation before retry"
                    .into(),
            );
        }
        if !status.is_success() || value["ok"] != true {
            if value["ok"] != false
                || value["profile"] != onebrain_api::vnext_api::VNEXT_PRODUCT_PROFILE
                || !value["error"].is_object()
            {
                return Err(unknown.into());
            }
            // Keep the exact bounded Base discriminator/retry/reconcile policy visible.
            println!("{}", serde_json::to_string_pretty(&value).unwrap());
            return Err("KU API failure; inspect error.failure and reconcile_before_retry; no automatic replay".into());
        }
        validate_response(&operation, &value)?;
        Ok(value)
    }
}

fn contains_secret(value: &Value, token: &str) -> bool {
    match value {
        Value::String(text) => text.contains(token),
        Value::Array(items) => items.iter().any(|item| contains_secret(item, token)),
        Value::Object(fields) => fields
            .iter()
            .any(|(key, value)| key.contains(token) || contains_secret(value, token)),
        _ => false,
    }
}

fn session(value: &Value) -> Result<Value, String> {
    let value = &value["data"]["session"];
    for key in ["process_generation", "dataset_generation"] {
        hex32(value[key].as_str().ok_or("missing KU session fence")?)?;
    }
    Ok(value.clone())
}

fn validate_response(operation: &str, value: &Value) -> Result<(), String> {
    if value["profile"] != onebrain_api::vnext_api::VNEXT_PRODUCT_PROFILE
        || value["data"]["model_qualified"] != false
        || !value["meta"].is_object()
    {
        return Err(
            "invalid KU response envelope; reconcile original operation before retry".into(),
        );
    }
    session(value)?;
    let bytes = serde_json::to_vec(&value["data"]["payload"]).unwrap();
    match operation {
        "reserve" => dto::<KuOperationRefV1>(&bytes),
        "prepare" | "preview" | "revise" => dto::<KuPreparedV1>(&bytes),
        "save" | "cancel" | "reconcile" => dto::<KuReceiptV1>(&bytes),
        "get" => dto::<KuViewV1>(&bytes),
        "list" | "search" => dto::<KuPageV1>(&bytes),
        "export" => dto::<KuExportViewV1>(&bytes),
        "status" => dto::<KuStatusV1>(&bytes),
        _ => return Err("unknown KU response operation".into()),
    }
    .map_err(|_| "invalid KU response DTO; reconcile original operation before retry")?;
    Ok(())
}

fn verify_cancel(operation_id: &str, typed: &str) -> Result<(), String> {
    if typed.trim_end_matches(['\r', '\n']) != operation_id {
        return Err("cancellation rejected: type the exact operation ID".into());
    }
    Ok(())
}

async fn run(args: &KuArgs) -> Result<Value, String> {
    run_with_confirmation(args, read_cancellation).await
}

fn read_cancellation(operation_id: &str) -> Result<String, String> {
    eprint!("Cancel eligible private staging (committed work cannot be undone). Type exact operation_id {operation_id}: ");
    std::io::stderr()
        .flush()
        .map_err(|_| "cannot show cancellation prompt")?;
    let mut typed = String::new();
    std::io::stdin()
        .lock()
        .take(128)
        .read_line(&mut typed)
        .map_err(|_| "cannot read cancellation")?;
    Ok(typed)
}

async fn run_with_confirmation(
    args: &KuArgs,
    confirm: impl FnOnce(&str) -> Result<String, String>,
) -> Result<Value, String> {
    let operation = args.command.request()?;
    let client = Client::new(args)?;
    if let KuCommand::Cancel(op) = &args.command {
        verify_cancel(&op.operation_id, &confirm(&op.operation_id)?)?;
    }
    let status = client
        .call(Method::GET, "/api/vnext/ku/status", None)
        .await?;
    let session = session(&status)?;
    if matches!(args.command, KuCommand::Reserve) {
        return client
            .call(
                Method::POST,
                "/api/vnext/ku/reservations",
                Some(json!({"session":session})),
            )
            .await;
    }
    let Some((operation, payload)) = operation else {
        return Ok(status);
    };
    client.call(Method::POST, "/api/vnext/ku/operations", Some(json!({
        "session":session,
        "budget":{"max_items":args.max_items,"max_bytes":args.max_bytes,"max_work_units":args.max_work_units},
        "request":{"operation":operation,"payload":payload}
    }))).await
}

pub(crate) async fn execute(args: KuArgs) -> Result<(), String> {
    let envelope = run(&args).await?;
    println!("{}", serde_json::to_string_pretty(&envelope).unwrap());
    eprintln!("Local authorized scope only. Save is private; preparation is pending intent, not accepted KU. Partial/empty pages are not network absence. Fidelity remains unassessed unless evidenced; models are unqualified. No publication, delivery or reward is implied.");
    Ok(())
}

#[cfg(test)]
mod tests;
