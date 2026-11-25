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
    /// Prefer --api-token-file or ONEBRAIN_API_TOKEN to keep the token out of shell history.
    #[arg(long, global = true, conflicts_with = "api_token_file")]
    api_token: Option<String>,
    /// Private UTF-8 file containing the API Bearer token (32..1024 ASCII letters,
    /// digits, '-' or '_'). Conflicts with --api-token; overrides ONEBRAIN_API_TOKEN.
    #[arg(long, global = true, value_name = "PATH", conflicts_with = "api_token")]
    api_token_file: Option<PathBuf>,
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
    /// D-045 Public KU share through the host's registered editor actions.
    #[command(subcommand)]
    Share(ShareCommand),
    /// D-058: show this host's active Registry canonical name for each CCID
    /// (display only; not verification, fidelity, adoption or reward).
    Describe {
        /// Concept CCID (32 lowercase hex); repeat up to 64 times.
        #[arg(long = "ccid", required = true, value_parser = hex16)]
        ccids: Vec<String>,
    },
}

/// Thin projection of the registered `POST /api/vnext/ku/editor` share actions
/// (`share_prepare`, `share_confirm`, `share_intent`, `share_inspect`,
/// `share_list`). The host owns consent, the outbox and the network runtime.
#[derive(Debug, Subcommand)]
enum ShareCommand {
    /// Preview one saved private KU as a Public KU for one expected peer; never enqueues or sends.
    Prepare {
        /// Saved private ObjectCID (one predicate + one text literal only).
        #[arg(long, value_parser = hex32)]
        object_cid: String,
        /// Expected receiver NodeID; the session fails closed on any other peer.
        #[arg(long, value_parser = hex32)]
        expected_node_id: String,
        /// Receiver listen address, e.g. 127.0.0.1:4433.
        #[arg(long)]
        address: std::net::SocketAddr,
        #[arg(long, value_parser = hex32)]
        selector: String,
        #[arg(long, value_parser = hex32)]
        namespace: String,
    },
    /// Type the exact prepared_id on stdin to confirm the previewed Public share; no --yes bypass.
    Confirm {
        #[arg(long, value_parser = hex32)]
        prepared_id: String,
    },
    /// Read one durable outbound intent; read-only, never resends.
    Intent {
        #[arg(long, value_parser = hex32)]
        intent_id: String,
    },
    /// Receiver: inspect an accepted Public KU and its transport provenance.
    Inspect {
        #[arg(long, value_parser = hex32)]
        object_cid: String,
        #[arg(long, value_parser = hex32)]
        selector: String,
    },
    /// Receiver: browse accepted Public KUs, separate from private saved knowledge.
    List {
        #[arg(long)]
        query: Option<String>,
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=256))]
        limit: Option<u32>,
    },
}

impl ShareCommand {
    fn request(&self) -> (&'static str, Value) {
        match self {
            Self::Prepare {
                object_cid,
                expected_node_id,
                address,
                selector,
                namespace,
            } => (
                "share_prepare",
                json!({"object_cid":object_cid,"expected_node_id":expected_node_id,"address":address.to_string(),"selector":selector,"namespace":namespace}),
            ),
            // Only reached after the exact typed prepared_id matched.
            Self::Confirm { prepared_id } => (
                "share_confirm",
                json!({"prepared_id":prepared_id,"confirm_public":true}),
            ),
            Self::Intent { intent_id } => ("share_intent", json!({"intent_id":intent_id})),
            Self::Inspect {
                object_cid,
                selector,
            } => (
                "share_inspect",
                json!({"object_cid":object_cid,"selector":selector}),
            ),
            Self::List { query, limit } => {
                let mut payload = json!({});
                if let Some(query) = query {
                    payload["query"] = json!(query);
                }
                if let Some(limit) = limit {
                    payload["limit"] = json!(limit);
                }
                ("share_list", payload)
            }
        }
    }
}

impl KuCommand {
    /// Registered editor action for `share …` / `describe`; `None` otherwise.
    fn editor_request(&self) -> Result<Option<(&'static str, Value)>, String> {
        match self {
            Self::Share(share) => Ok(Some(share.request())),
            Self::Describe { ccids } => {
                if ccids.len() > MAX_DESCRIBE_CCIDS {
                    return Err("at most 64 --ccid values".into());
                }
                Ok(Some(("describe_ccids", json!({ "ccids": ccids }))))
            }
            _ => Ok(None),
        }
    }
}

const CANCEL_PROMPT: &str =
    "Cancel eligible private staging (committed work cannot be undone). Type exact operation_id";
const SHARE_CONFIRM_PROMPT: &str = "Confirm PUBLIC share of the exact previewed Public KU to the expected peer; a Public copy cannot be made private again. Type exact prepared_id";
const SHARE_SCOPE: &str = "Public share scope: preview never enqueues or sends; confirm creates a durable pending intent only; Acknowledged is a transport receipt, not adoption, verification or reward. Private ObjectCID and source stay withheld. Fidelity remains unassessed; models are unqualified.";
const DESCRIBE_SCOPE: &str = "Registry label scope: display-only canonical names from this host's active Registry (null = not in it); not verification, fidelity, adoption or reward.";
const MAX_DESCRIBE_CCIDS: usize = 64;

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

fn hex16(value: &str) -> Result<String, String> {
    if value.len() == 32
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        Ok(value.to_owned())
    } else {
        Err("expected exactly 32 lowercase hexadecimal characters".into())
    }
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
            Self::Share(_) | Self::Describe { .. } => {
                return Err("share/describe actions use the registered editor transport".into())
            }
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
        let token = crate::resolve_client_api_token(
            args.api_token.as_deref(),
            args.api_token_file.as_deref(),
        )?;
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
            .and_then(|v| {
                v.pointer("/request/operation")
                    .or_else(|| v.pointer("/request/action"))
            })
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
    let payload = &value["data"]["payload"];
    if operation == "describe_ccids" {
        if payload["event"] != "ccid_labels"
            || payload["authorizes_reward"] != false
            || !payload["items"].is_array()
        {
            return Err("invalid KU describe response".into());
        }
        return Ok(());
    }
    let share_event = match operation {
        "share_prepare" => Some("share_prepared"),
        "share_confirm" => Some("share_confirmed"),
        "share_intent" => Some("intent"),
        "share_inspect" => Some("public_object"),
        "share_list" => Some("public_objects"),
        _ => None,
    };
    if let Some(event) = share_event {
        let pending_preview = operation != "share_prepare"
            || (payload["enqueued"] == false && payload["payload_sent"] == false);
        if payload["event"] != event
            || payload.get("authorizes_reward").is_some_and(|v| v != false)
            || !pending_preview
        {
            return Err("invalid KU share response; inspect the intent before any retry".into());
        }
        return Ok(());
    }
    let bytes = serde_json::to_vec(payload).unwrap();
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

fn verify_share_confirm(prepared_id: &str, typed: &str) -> Result<(), String> {
    if typed.trim_end_matches(['\r', '\n']) != prepared_id {
        return Err(
            "public share confirmation rejected: type the exact prepared_id; nothing was confirmed"
                .into(),
        );
    }
    Ok(())
}

async fn run(args: &KuArgs) -> Result<Value, String> {
    let prompt = match &args.command {
        KuCommand::Share(ShareCommand::Confirm { .. }) => SHARE_CONFIRM_PROMPT,
        _ => CANCEL_PROMPT,
    };
    run_with_confirmation(args, |id| read_confirmation(prompt, id)).await
}

fn read_confirmation(prompt: &str, id: &str) -> Result<String, String> {
    eprint!("{prompt} {id}: ");
    std::io::stderr()
        .flush()
        .map_err(|_| "cannot show confirmation prompt")?;
    let mut typed = String::new();
    std::io::stdin()
        .lock()
        .take(128)
        .read_line(&mut typed)
        .map_err(|_| "cannot read confirmation")?;
    Ok(typed)
}

async fn run_with_confirmation(
    args: &KuArgs,
    confirm: impl FnOnce(&str) -> Result<String, String>,
) -> Result<Value, String> {
    if let Some((action, payload)) = args.command.editor_request()? {
        let client = Client::new(args)?;
        if let KuCommand::Share(ShareCommand::Confirm { prepared_id }) = &args.command {
            verify_share_confirm(prepared_id, &confirm(prepared_id)?)?;
        }
        let status = client
            .call(Method::GET, "/api/vnext/ku/status", None)
            .await?;
        let session = session(&status)?;
        return client.call(Method::POST, "/api/vnext/ku/editor", Some(json!({
            "session":session,
            "budget":{"max_items":args.max_items,"max_bytes":args.max_bytes,"max_work_units":args.max_work_units},
            "request":{"action":action,"payload":payload}
        }))).await;
    }
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
    if matches!(args.command, KuCommand::Share(_)) {
        eprintln!("{SHARE_SCOPE}");
        return Ok(());
    }
    if matches!(args.command, KuCommand::Describe { .. }) {
        eprintln!("{DESCRIBE_SCOPE}");
        return Ok(());
    }
    eprintln!("Local authorized scope only. Save is private; preparation is pending intent, not accepted KU. Partial/empty pages are not network absence. Fidelity remains unassessed unless evidenced; models are unqualified. No publication, delivery or reward is implied.");
    Ok(())
}

#[cfg(test)]
mod tests;
