//! OBP transport projection only. No node, grants, discovery or retry owner here.
use clap::{Args, Subcommand};
use onebrain_node::vnext_product_runtime::obp::contract;
use reqwest::{Method, Url};
use serde_json::{json, Value};
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

const UNKNOWN: &str = "OBP outcome unknown; retain original key/payload/context, read status and reconcile before retry; no automatic replay";

#[derive(Args)]
pub(crate) struct ObpArgs {
    #[arg(long, global = true, default_value = "http://127.0.0.1:4280")]
    api_url: String,
    /// Prefer ONEBRAIN_API_TOKEN to avoid shell history disclosure.
    #[arg(long, global = true)]
    api_token: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Status,
    SourceList(Files),
    ReservationList(Files),
    RouteStatus(Files),
    IntentStatus(Files),
    Configure(Files),
    /// Admit a host-owned input_ref; never upload source bytes or addresses.
    SourceAdmit(Files),
    /// Disable preserves replay floors; there is no source deletion operation.
    SourceSetEnabled(Files),
    Refresh(Files),
    /// Connect only to the exact expected_peer; sends no application payload.
    RouteRequest(Files),
    /// Schedule an eligible pending intent; never reset counters or consent.
    IntentRetry(Files),
    NetworkKill(Files),
    NetworkReenable(Files),
    /// Read recorded metadata only. local_not_found does not prove no effect.
    Reconcile {
        #[arg(long)]
        context_file: PathBuf,
        #[arg(long, value_parser = hex_id)]
        idempotency_key: String,
    },
}

#[derive(Args)]
struct Files {
    /// Exact saved status envelope; never automatically replace session fences.
    #[arg(long)]
    context_file: PathBuf,
    /// Exact accepted request DTO, including stable key/generation for mutations.
    #[arg(long)]
    payload_file: PathBuf,
}

fn hex_id(s: &str) -> Result<String, String> {
    contract::id(&json!(s))
        .map(|_| s.to_owned())
        .map_err(|_| "expected exactly 64 lowercase hexadecimal characters".into())
}
fn read_json(path: &Path) -> Result<Value, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| "cannot open DTO/context file")?
        .take(contract::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read DTO/context file")?;
    contract::parse(&bytes).map_err(|_| "invalid or over-budget JSON file".into())
}
fn validate_dto(name: &str, v: &Value) -> Result<(), String> {
    contract::validate(name, v)
        .map_err(|_| "invalid OBP DTO; reconcile original command before retry".into())
}
fn fields(v: &Value, required: &[&str], optional: &[&str]) -> Result<(), String> {
    contract::fields(v, required, optional).map_err(|_| UNKNOWN.into())
}
fn session(status: &Value) -> Result<Value, String> {
    validate_response("status", status, None)?;
    if status["data"]["available"] != true {
        return Err(
            "OBP service unavailable; host must explicitly supply the shared service and bindings"
                .into(),
        );
    }
    Ok(status["data"]["session"].clone())
}
fn operation(name: &str) -> Result<&'static Value, String> {
    contract::inventory()["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["name"] == name)
        .ok_or_else(|| "unknown OBP operation".into())
}

struct Prepared {
    name: &'static str,
    route: &'static str,
    body: Option<Value>,
    management: bool,
}
impl Command {
    fn prepare(&self) -> Result<Prepared, String> {
        let (name, file) = match self {
            Self::Status => {
                return Ok(Prepared {
                    name: "status",
                    route: "status",
                    body: None,
                    management: false,
                })
            }
            Self::Reconcile {
                context_file,
                idempotency_key,
            } => {
                return Ok(Prepared {
                    name: "reconcile",
                    route: "reconcile",
                    management: false,
                    body: Some(
                        json!({"session":session(&read_json(context_file)?)?, "idempotency_key":idempotency_key}),
                    ),
                });
            }
            Self::SourceList(f) => ("source_list", f),
            Self::ReservationList(f) => ("reservation_list", f),
            Self::RouteStatus(f) => ("route_status", f),
            Self::IntentStatus(f) => ("intent_status", f),
            Self::Configure(f) => ("configure", f),
            Self::SourceAdmit(f) => ("source_admit", f),
            Self::SourceSetEnabled(f) => ("source_set_enabled", f),
            Self::Refresh(f) => ("refresh", f),
            Self::RouteRequest(f) => ("route_request", f),
            Self::IntentRetry(f) => ("intent_retry", f),
            Self::NetworkKill(f) => ("network_kill", f),
            Self::NetworkReenable(f) => ("network_reenable", f),
        };
        let op = operation(name)?;
        let payload = read_json(&file.payload_file)?;
        let command = op["effect"] != "none";
        let request = contract::Request::new(name, payload.clone(), command)
            .map_err(|_| "invalid OBP request DTO")?;
        Ok(Prepared {
            name,
            route: if command { "commands" } else { "query" },
            management: request.management(),
            body: Some(json!({
                "session":session(&read_json(&file.context_file)?)?, "operation":name, "payload":payload
            })),
        })
    }
}

struct Client {
    http: reqwest::Client,
    base: Url,
    token: String,
    management: Option<String>,
}
impl Client {
    fn new(args: &ObpArgs, management: Option<String>) -> Result<Self, String> {
        let base = Url::parse(&args.api_url).map_err(|_| "invalid local API origin")?;
        if !base
            .host_str()
            .unwrap_or("")
            .trim_matches(['[', ']'])
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
            || !matches!(base.scheme(), "http" | "https")
            || base.path() != "/"
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
        {
            return Err("OBP requires a numeric loopback HTTP(S) origin without credentials, path, query or fragment".into());
        }
        let token = args
            .api_token
            .clone()
            .or_else(|| std::env::var("ONEBRAIN_API_TOKEN").ok())
            .filter(|s| !s.trim().is_empty())
            .ok_or("set ONEBRAIN_API_TOKEN or --api-token")?;
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| "cannot create local API client")?;
        Ok(Self {
            http,
            base,
            token,
            management,
        })
    }
    fn has_secret(&self, v: &Value) -> bool {
        contains_secret(v, &self.token)
            || self
                .management
                .as_ref()
                .is_some_and(|s| contains_secret(v, s))
    }
    async fn call(&self, p: &Prepared) -> Result<Value, String> {
        if p.management && self.management.as_ref().is_none_or(|s| s.is_empty()) {
            return Err(
                "host-scoped ONEBRAIN_OBP_MANAGEMENT_TOKEN required; CLI cannot mint authority"
                    .into(),
            );
        }
        let mut request = self
            .http
            .request(
                if p.body.is_some() {
                    Method::POST
                } else {
                    Method::GET
                },
                self.base
                    .join(&format!("/api/vnext/obp/{}", p.route))
                    .unwrap(),
            )
            .bearer_auth(&self.token);
        if p.management {
            request = request.header(
                "X-OneBrain-OBP-Management",
                self.management.as_ref().unwrap(),
            );
        }
        if let Some(body) = &p.body {
            let bytes = serde_json::to_vec(body).unwrap();
            if bytes.len() > contract::MAX_BYTES || self.has_secret(body) {
                return Err("request over budget or contains credentials in body".into());
            }
            request = request
                .header("Content-Type", "application/json")
                .body(bytes);
        }
        let mut response = request.send().await.map_err(|_| UNKNOWN)?;
        let status = response.status();
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| UNKNOWN)? {
            if chunk.len() > contract::MAX_BYTES.saturating_sub(bytes.len()) {
                return Err(UNKNOWN.into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let v = contract::parse(&bytes).map_err(|_| UNKNOWN)?;
        if self.has_secret(&v) {
            return Err(
                "API reflected credentials; reconcile original command before retry".into(),
            );
        }
        if !status.is_success() || v["ok"] != true {
            if matches!(status.as_u16(), 401 | 403) {
                return Err("OBP authentication/capability rejected; host must provide current scoped authority".into());
            }
            validate_error(&v)?;
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
            return Err("OBP API failure; inspect error.obp; local_not_found is current-dataset only; no automatic replay".into());
        }
        validate_response(p.name, &v, p.body.as_ref())?;
        Ok(v)
    }
}
fn contains_secret(v: &Value, s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    match v {
        Value::String(v) => v.contains(s),
        Value::Array(a) => a.iter().any(|v| contains_secret(v, s)),
        Value::Object(m) => m
            .iter()
            .any(|(k, v)| k.contains(s) || contains_secret(v, s)),
        _ => false,
    }
}

fn envelope(v: &Value, ok: bool) -> Result<(), String> {
    fields(
        v,
        &["ok", "profile", if ok { "data" } else { "error" }, "meta"],
        &[],
    )?;
    if v["ok"] != ok || v["profile"] != onebrain_api::vnext_api::VNEXT_PRODUCT_PROFILE {
        return Err(UNKNOWN.into());
    }
    fields(
        &v["meta"],
        &["lifecycle", "coverage", "limitations", "continuation"],
        &[],
    )?;
    validate_dto("Lifecycle", &v["meta"]["lifecycle"])?;
    validate_dto("Coverage", &v["meta"]["coverage"])?;
    validate_dto("Limitations", &v["meta"]["limitations"])?;
    if !v["meta"]["continuation"].is_null() {
        validate_dto("Continuation", &v["meta"]["continuation"])?;
    }
    Ok(())
}
fn validate_error(v: &Value) -> Result<(), String> {
    envelope(v, false)?;
    fields(
        &v["error"],
        &["code", "message", "retryable", "limitations", "obp"],
        &[],
    )?;
    fields(
        &v["error"]["obp"],
        &["reason", "outcome", "reconcile_before_retry"],
        &["idempotency_key"],
    )?;
    let e = &v["error"]["obp"];
    let reason = e["reason"].as_str().ok_or(UNKNOWN)?;
    let (code, unknown, retryable) = match reason {
        "invalid_payload" => ("invalid_request", false, false),
        "local_not_found" => ("not_found", false, false),
        "generation_conflict" | "idempotency_conflict" | "session_conflict" => {
            ("conflict", false, false)
        }
        "input_expired" | "snapshot_expired" => ("expired", false, false),
        "admission_limit" => ("rate_limited", false, true),
        "feature_disabled" => ("capability_disabled", false, false),
        "dependency_unavailable" => ("dependency_unavailable", false, true),
        "outcome_unknown" => ("dependency_unavailable", true, true),
        "storage_corrupt" | "response_overflow" => ("internal_error", true, false),
        _ => return Err(UNKNOWN.into()),
    };
    if v["error"]["code"] != code
        || v["error"]["message"] != reason
        || v["error"]["retryable"] != retryable
        || e["reconcile_before_retry"] != unknown
        || e["outcome"] != if unknown { "unknown" } else { "not_admitted" }
    {
        return Err(UNKNOWN.into());
    }
    validate_dto("Limitations", &v["error"]["limitations"])?;
    if e.get("idempotency_key").is_some() {
        validate_dto("IdempotencyKey", &e["idempotency_key"])?;
    }
    Ok(())
}
fn validate_response(name: &str, v: &Value, request: Option<&Value>) -> Result<(), String> {
    envelope(v, true)?;
    let d = &v["data"];
    if name == "status" {
        if d["available"] == false {
            fields(d, &["available", "compiled"], &[])?;
            if !d["compiled"].is_boolean() || v["meta"]["lifecycle"] != "disabled" {
                return Err(UNKNOWN.into());
            }
        } else {
            fields(d, &["available", "session", "status"], &[])?;
            if d["available"] != true {
                return Err(UNKNOWN.into());
            }
            fields(
                &d["session"],
                &["process_generation", "dataset_generation"],
                &[],
            )?;
            for key in ["process_generation", "dataset_generation"] {
                validate_dto("IdempotencyKey", &d["session"][key])?;
            }
            validate_dto("ObpStatusV1", &d["status"])?;
        }
        return Ok(());
    }
    if name == "reconcile" || operation(name)?["effect"] != "none" {
        fields(
            d,
            &[
                "idempotency_key",
                "operation",
                "state",
                "reconcile_before_retry",
            ],
            if name == "reconcile" {
                &[]
            } else {
                &["result", "failure"]
            },
        )?;
        validate_dto("IdempotencyKey", &d["idempotency_key"])?;
        let op = operation(d["operation"].as_str().ok_or(UNKNOWN)?)?;
        if op["effect"] == "none" {
            return Err(UNKNOWN.into());
        }
        if name != "reconcile" && d["operation"] != name {
            return Err(UNKNOWN.into());
        }
        if let Some(r) = request {
            let key = if name == "reconcile" {
                &r["idempotency_key"]
            } else {
                &r["payload"]["idempotency_key"]
            };
            if &d["idempotency_key"] != key {
                return Err(UNKNOWN.into());
            }
        }
        let unknown = matches!(d["state"].as_str(), Some("admitted" | "reconcile_required"));
        if d["reconcile_before_retry"] != unknown {
            return Err(UNKNOWN.into());
        }
        match d["state"].as_str() {
            Some("completed") if name != "reconcile" => {
                if d.get("failure").is_some() {
                    return Err(UNKNOWN.into());
                }
                validate_dto(op["response"].as_str().ok_or(UNKNOWN)?, &d["result"])?;
                check_target(d["operation"].as_str().unwrap(), &d["result"], request)?;
            }
            Some("failed_no_effect") if name != "reconcile" => {
                if d.get("result").is_some() {
                    return Err(UNKNOWN.into());
                }
                let policy: Value = serde_json::from_str(include_str!(
                    "../../../test-vectors/vnext/obp-local-api-v1.json"
                ))
                .unwrap();
                if d["failure"]["outcome"] != "not_admitted"
                    || !policy["errors"].as_array().unwrap().contains(&d["failure"])
                {
                    return Err(UNKNOWN.into());
                }
            }
            Some("completed" | "failed_no_effect") if name == "reconcile" => {}
            Some("admitted" | "reconcile_required") => {
                if d.get("result").is_some() || d.get("failure").is_some() {
                    return Err(UNKNOWN.into());
                }
            }
            _ => return Err(UNKNOWN.into()),
        }
        return Ok(());
    }
    validate_dto(operation(name)?["response"].as_str().ok_or(UNKNOWN)?, d)?;
    check_target(name, d, request)
}
fn check_target(name: &str, data: &Value, request: Option<&Value>) -> Result<(), String> {
    if let Some(r) = request {
        let field = match name {
            "route_request" => "expected_peer",
            "route_status" => "route_id",
            "intent_status" | "intent_retry" => "intent_id",
            "source_set_enabled" => "source_id",
            _ => return Ok(()),
        };
        if data[field] != r["payload"][field] {
            return Err(UNKNOWN.into());
        }
    }
    Ok(())
}
fn confirmation(key: &str) -> Result<String, String> {
    eprint!("Type exact idempotency_key {key} to submit this bounded command: ");
    std::io::stderr()
        .flush()
        .map_err(|_| "cannot show confirmation")?;
    let mut typed = String::new();
    std::io::stdin()
        .lock()
        .take(128)
        .read_line(&mut typed)
        .map_err(|_| "cannot read confirmation")?;
    Ok(typed)
}
async fn run(
    args: &ObpArgs,
    management: Option<String>,
    confirm: impl FnOnce(&str) -> Result<String, String>,
) -> Result<Value, String> {
    let p = args.command.prepare()?;
    let client = Client::new(args, management)?;
    if p.management && client.management.as_ref().is_none_or(|s| s.is_empty()) {
        return Err(
            "host-scoped ONEBRAIN_OBP_MANAGEMENT_TOKEN required; CLI cannot mint authority".into(),
        );
    }
    if p.route == "commands" {
        let body = p.body.as_ref().unwrap();
        if client.has_secret(body) {
            return Err("credentials forbidden in command body".into());
        }
        eprintln!(
            "{}: {}",
            p.name,
            serde_json::to_string_pretty(&body["payload"]).unwrap()
        );
        let key = body["payload"]["idempotency_key"].as_str().unwrap();
        if confirm(key)?.trim_end_matches(['\r', '\n']) != key {
            return Err("command rejected: exact idempotency key confirmation required".into());
        }
    }
    client.call(&p).await
}
pub(crate) async fn execute(args: ObpArgs) -> Result<(), String> {
    let value = run(
        &args,
        std::env::var("ONEBRAIN_OBP_MANAGEMENT_TOKEN").ok(),
        confirmation,
    )
    .await?;
    println!("{}", serde_json::to_string_pretty(&value).unwrap());
    eprintln!("Bounded local view. Empty/path_limited is not global absence. Discovery and reservations confer no trust; command completion is not delivery, publication or reward. Preserve original keys and reconcile unknown outcomes before retry.");
    Ok(())
}
#[cfg(test)]
mod tests;
