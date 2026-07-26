//! Additive vNext CLI surface backed by the authenticated P3.1 REST profile.
//!
//! The client intentionally does not reinterpret legacy `kql`, `status`, or
//! PoMV scalar commands. Public Use confirmation is interactive and bound to
//! the exact prepared intent; there is deliberately no `--yes` bypass.

use std::io::{self, Write};
#[cfg(feature = "vnext-network-runtime")]
use std::path::Path;
use std::path::PathBuf;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use clap::{Args, Subcommand, ValueEnum};
use reqwest::{Method, Url};
use serde_json::{json, Value};

const DEFAULT_API_URL: &str = "http://127.0.0.1:4280";
const API_TOKEN_ENV: &str = "ONEBRAIN_API_TOKEN";
const CONFIRMATION_DOMAIN: &[u8] = b"onebrain:vnext:rest-explicit-confirmation:1\0";
const CONTINUATION_PREFIX: &str = "obc1.";
#[cfg(feature = "vnext-network-runtime")]
const DEVELOPMENT_FEED_KEY_FILE: &str = "vnext_feed_signer.development.key";
#[cfg(feature = "vnext-network-runtime")]
const PRIVATE_NEED_VAULT_KEY_FILE: &str = "vnext_private_need_vault.key";

#[derive(Clone, Debug, Args)]
pub(crate) struct ApiConnectionArgs {
    /// Base URL of the authenticated local OneBrain API.
    #[arg(long, global = true, default_value = DEFAULT_API_URL)]
    api_url: String,
    /// Bearer token. Prefer the ONEBRAIN_API_TOKEN environment variable.
    #[arg(long, global = true)]
    api_token: Option<String>,
}

impl ApiConnectionArgs {
    fn client(&self) -> Result<VNextApiClient, String> {
        let token = self
            .api_token
            .clone()
            .or_else(|| std::env::var(API_TOKEN_ENV).ok())
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| format!("missing API token; pass --api-token or set {API_TOKEN_ENV}"))?;
        VNextApiClient::new(&self.api_url, token)
    }
}

#[derive(Clone, Copy, Debug, Args)]
pub(crate) struct BudgetArgs {
    #[arg(long, default_value_t = 4_096)]
    max_scan_records: u64,
    #[arg(long, default_value_t = 1_024)]
    max_affordances: u64,
    #[arg(long, default_value_t = 65_536)]
    max_pairs: u64,
    #[arg(long, default_value_t = 4_096)]
    max_proposals: u64,
}

impl BudgetArgs {
    fn json(self) -> Value {
        json!({
            "max_scan_records": self.max_scan_records,
            "max_affordances": self.max_affordances,
            "max_pairs": self.max_pairs,
            "max_proposals": self.max_proposals,
        })
    }
}

#[derive(Clone, Debug, Args)]
pub(crate) struct NeedArgs {
    #[command(flatten)]
    connection: ApiConnectionArgs,
    #[command(subcommand)]
    command: NeedCommand,
}

#[derive(Clone, Debug, Subcommand)]
enum NeedCommand {
    /// Prepare a private local query without activating a standing Need.
    Prepare {
        #[arg(long)]
        query: String,
        #[arg(long)]
        idempotency_key: String,
        #[command(flatten)]
        budget: BudgetArgs,
    },
    /// Activate one exact prepared Need intent.
    Activate {
        #[arg(long)]
        intent: String,
        #[arg(long)]
        idempotency_key: String,
    },
    /// List local non-terminal standing Needs.
    List {
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long)]
        continuation: Option<String>,
    },
    /// Run one bounded one-hop delta scan for a standing Need.
    Scan {
        #[arg(long)]
        need: String,
        #[arg(long)]
        idempotency_key: String,
        #[arg(long)]
        continuation: Option<String>,
        #[command(flatten)]
        budget: BudgetArgs,
    },
    /// List quarantined matches for one standing Need.
    Matches {
        #[arg(long)]
        need: String,
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long)]
        continuation: Option<String>,
    },
    /// Retire a standing Need idempotently.
    Retire {
        #[arg(long)]
        need: String,
    },
}

#[derive(Clone, Debug, Args)]
pub(crate) struct PomvArgs {
    #[command(flatten)]
    connection: ApiConnectionArgs,
    #[command(subcommand)]
    command: PomvCommand,
}

#[derive(Clone, Debug, Subcommand)]
enum PomvCommand {
    /// Explicit Public Use preparation, confirmation, and status.
    Use(PomvUseArgs),
    /// Read a partial policy/frontier-relative Metabolic Evidence View.
    View {
        #[arg(long)]
        target: String,
    },
}

#[derive(Clone, Debug, Args)]
struct PomvUseArgs {
    #[command(subcommand)]
    command: PomvUseCommand,
}

#[derive(Clone, Debug, Subcommand)]
enum PomvUseCommand {
    /// Prepare and display the exact public/permanent payload; does not publish.
    Prepare {
        #[arg(long)]
        target: String,
        #[arg(long)]
        recipient: String,
        #[arg(long)]
        selector: String,
        #[arg(long)]
        namespace: String,
        #[arg(long)]
        idempotency_key: String,
        #[arg(long)]
        expires_at: u64,
        #[arg(long, value_enum, default_value_t = PublicUseMode::Application)]
        use_mode: PublicUseMode,
        /// Required acknowledgement that the prepared disclosure is public and permanent.
        #[arg(long, required = true, action = clap::ArgAction::SetTrue)]
        public_permanent: bool,
    },
    /// Interactively confirm one exact prepared intent.
    Confirm {
        #[arg(long)]
        intent: String,
    },
    /// Read a durable pending/deferred publication.
    Status {
        #[arg(long)]
        publication: String,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum PublicUseMode {
    Application,
    Transformation,
    Epistemic,
    Transfer,
    Discovery,
    ReceptorDiscovered,
    CandidateEvaluated,
    ConstraintClarified,
    GapPartiallyFilled,
    AssemblyUsed,
    AnalogicalTransfer,
    ComparedOrOpposed,
    CapabilityResultUsed,
}

impl PublicUseMode {
    const fn wire_name(self) -> &'static str {
        match self {
            Self::Application => "application",
            Self::Transformation => "transformation",
            Self::Epistemic => "epistemic",
            Self::Transfer => "transfer",
            Self::Discovery => "discovery",
            Self::ReceptorDiscovered => "receptor_discovered",
            Self::CandidateEvaluated => "candidate_evaluated",
            Self::ConstraintClarified => "constraint_clarified",
            Self::GapPartiallyFilled => "gap_partially_filled",
            Self::AssemblyUsed => "assembly_used",
            Self::AnalogicalTransfer => "analogical_transfer",
            Self::ComparedOrOpposed => "compared_or_opposed",
            Self::CapabilityResultUsed => "capability_result_used",
        }
    }
}

#[derive(Clone, Debug, Args)]
pub(crate) struct VNextArgs {
    #[command(flatten)]
    connection: ApiConnectionArgs,
    #[command(subcommand)]
    command: VNextCommand,
}

#[derive(Clone, Debug, Subcommand)]
enum VNextCommand {
    /// Show compiled/requested/active/kill-switch/signer state separately.
    Status,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub(crate) enum FeedSignerProvider {
    #[default]
    None,
    DevelopmentFile,
}

impl FeedSignerProvider {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::DevelopmentFile => "development-file",
        }
    }
}

#[derive(Clone, Debug, Default, Args)]
pub(crate) struct VNextStartArgs {
    /// Enable the bounded one-hop private Need lane.
    #[arg(long)]
    pub(crate) vnext_kql: bool,
    /// Enable explicitly confirmed Public UseEvidence publication.
    #[arg(long)]
    pub(crate) vnext_public_use: bool,
    /// Enable read-only Metabolic Evidence Views.
    #[arg(long)]
    pub(crate) vnext_pomv_view: bool,
    /// Select the Feed event signer provider explicitly.
    #[arg(long, value_enum, default_value_t = FeedSignerProvider::None)]
    pub(crate) vnext_feed_signer_provider: FeedSignerProvider,
    /// Explicitly permit the non-production local file Feed signer.
    #[arg(long, requires = "vnext_feed_signer_provider")]
    pub(crate) allow_development_file_signer: bool,
    /// Development Feed key path; defaults inside the node data directory.
    #[arg(long)]
    pub(crate) vnext_feed_key_file: Option<PathBuf>,
}

impl VNextStartArgs {
    pub(crate) fn requested(&self) -> bool {
        self.vnext_kql || self.vnext_public_use || self.vnext_pomv_view
    }

    pub(crate) fn feature_config(&self) -> Result<onebrain_node::VNextFeatureConfig, String> {
        if self.allow_development_file_signer
            && self.vnext_feed_signer_provider != FeedSignerProvider::DevelopmentFile
        {
            return Err(
                "--allow-development-file-signer requires provider development-file".into(),
            );
        }
        if self.vnext_feed_signer_provider == FeedSignerProvider::DevelopmentFile
            && !self.allow_development_file_signer
        {
            return Err(
                "development-file Feed signer requires --allow-development-file-signer".into(),
            );
        }
        if self.vnext_public_use {
            match self.vnext_feed_signer_provider {
                FeedSignerProvider::None => {
                    return Err(
                        "--vnext-public-use requires an explicit --vnext-feed-signer-provider"
                            .into(),
                    );
                }
                FeedSignerProvider::DevelopmentFile if !self.allow_development_file_signer => {
                    return Err(
                        "development-file Feed signer requires --allow-development-file-signer"
                            .into(),
                    );
                }
                FeedSignerProvider::DevelopmentFile => {}
            }
        }
        if self.vnext_feed_key_file.is_some()
            && self.vnext_feed_signer_provider != FeedSignerProvider::DevelopmentFile
        {
            return Err(
                "--vnext-feed-key-file is only valid with provider development-file".into(),
            );
        }

        let mut config = onebrain_node::VNextFeatureConfig::default();
        if self.requested() {
            config.enabled.object_event_v1 = true;
            config.enabled.obp_rp = true;
        }
        config.enabled.distributed_kql_one_hop = self.vnext_kql;
        config.enabled.public_use_evidence_publish = self.vnext_public_use;
        config.enabled.distributed_pomv_view = self.vnext_pomv_view;
        config.validate().map_err(|error| error.to_string())?;
        Ok(config)
    }

    pub(crate) fn describe_signer(&self) -> &'static str {
        self.vnext_feed_signer_provider.name()
    }
}

struct VNextApiClient {
    client: reqwest::Client,
    base: Url,
    token: String,
}

impl VNextApiClient {
    fn new(base: &str, token: String) -> Result<Self, String> {
        let mut base = Url::parse(base).map_err(|error| format!("invalid API URL: {error}"))?;
        if base.scheme() != "http" && base.scheme() != "https" {
            return Err("API URL must use http or https".into());
        }
        if !base.path().ends_with('/') {
            let path = format!("{}/", base.path());
            base.set_path(&path);
        }
        Ok(Self {
            client: reqwest::Client::new(),
            base,
            token,
        })
    }

    async fn call(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<Value>,
    ) -> Result<Value, String> {
        let mut url = self
            .base
            .join(path.trim_start_matches('/'))
            .map_err(|error| format!("invalid API path: {error}"))?;
        if !query.is_empty() {
            let mut pairs = url.query_pairs_mut();
            for (key, value) in query {
                pairs.append_pair(key, value);
            }
        }
        let mut request = self.client.request(method, url).bearer_auth(&self.token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .map_err(|error| format!("vNext API request failed: {error}"))?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|error| format!("vNext API response read failed: {error}"))?;
        let envelope: Value = serde_json::from_slice(&bytes)
            .map_err(|error| format!("vNext API returned non-JSON data: {error}"))?;
        if !status.is_success() || envelope.get("ok") != Some(&Value::Bool(true)) {
            let code = envelope
                .pointer("/error/code")
                .and_then(Value::as_str)
                .unwrap_or("http_error");
            let message = envelope
                .pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or("request was rejected");
            let limitations = string_array(envelope.pointer("/error/limitations"));
            return Err(format!(
                "{code} ({status}): {message}{}",
                format_limitations(&limitations)
            ));
        }
        if envelope.get("profile").and_then(Value::as_str)
            != Some(onebrain_api::vnext_api::VNEXT_PRODUCT_PROFILE)
        {
            return Err("vNext API response used an unexpected profile".into());
        }
        Ok(envelope)
    }
}

pub(crate) async fn execute_need(args: NeedArgs) -> Result<(), String> {
    let client = args.connection.client()?;
    let (envelope, safety) = match args.command {
        NeedCommand::Prepare {
            query,
            idempotency_key,
            budget,
        } => {
            validate_idempotency_key(&idempotency_key)?;
            if query.trim().is_empty() {
                return Err("--query cannot be empty".into());
            }
            (
                client
                    .call(
                        Method::POST,
                        "/api/vnext/kql/needs/prepare",
                        &[],
                        Some(json!({
                            "local_query": query,
                            "scope": {
                                "kind": "one_hop",
                                "max_hops": 1,
                                "node_ids": []
                            },
                            "budget": budget.json(),
                            "idempotency_key": idempotency_key
                        })),
                    )
                    .await?,
                "Prepared only: no standing Need was activated and the raw query stayed local.",
            )
        }
        NeedCommand::Activate {
            intent,
            idempotency_key,
        } => {
            validate_hex32("intent", &intent)?;
            validate_idempotency_key(&idempotency_key)?;
            (
                client
                    .call(
                        Method::POST,
                        "/api/vnext/kql/needs",
                        &[],
                        Some(json!({
                            "intent_cid": intent,
                            "idempotency_key": idempotency_key
                        })),
                    )
                    .await?,
                "Activation is local-private; exact replay preserves the same standing_need_id.",
            )
        }
        NeedCommand::List {
            limit,
            continuation,
        } => {
            validate_page(limit, continuation.as_deref())?;
            let mut query = vec![("limit", limit.to_string())];
            if let Some(continuation) = continuation {
                query.push(("continuation", continuation));
            }
            let envelope = client
                .call(Method::GET, "/api/vnext/kql/needs", &query, None)
                .await?;
            let count = envelope
                .pointer("/data/items")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            let safety = if count == 0 {
                "0 local non-terminal Needs; this is a local page, not a network-wide absence claim."
            } else {
                "Listed authenticated local-private Need projections only."
            };
            (envelope, safety)
        }
        NeedCommand::Scan {
            need,
            idempotency_key,
            continuation,
            budget,
        } => {
            validate_hex32("need", &need)?;
            validate_idempotency_key(&idempotency_key)?;
            validate_continuation(continuation.as_deref())?;
            (
                client
                    .call(
                        Method::POST,
                        &format!("/api/vnext/kql/needs/{need}/scan"),
                        &[],
                        Some(json!({
                            "budget": budget.json(),
                            "continuation": continuation,
                            "idempotency_key": idempotency_key
                        })),
                    )
                    .await?,
                "Bounded one-hop delta scan only; zero new matches does not prove network-wide absence.",
            )
        }
        NeedCommand::Matches {
            need,
            limit,
            continuation,
        } => {
            validate_hex32("need", &need)?;
            validate_page(limit, continuation.as_deref())?;
            let mut query = vec![("limit", limit.to_string())];
            if let Some(continuation) = continuation {
                query.push(("continuation", continuation));
            }
            let envelope = client
                .call(
                    Method::GET,
                    &format!("/api/vnext/kql/needs/{need}/matches"),
                    &query,
                    None,
                )
                .await?;
            enforce_quarantined_matches(&envelope)?;
            let count = envelope
                .pointer("/data/items")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            let safety = if count == 0 {
                "No match in this bounded partial page; this is not evidence of network-wide absence."
            } else {
                "Every result is a quarantined proposal (executable=false); listing cannot materialize or adopt it."
            };
            (envelope, safety)
        }
        NeedCommand::Retire { need } => {
            validate_hex32("need", &need)?;
            (
                client
                    .call(
                        Method::DELETE,
                        &format!("/api/vnext/kql/needs/{need}"),
                        &[],
                        None,
                    )
                    .await?,
                "Retirement is terminal and idempotent; exact replay preserves the same identity.",
            )
        }
    };
    print_envelope(&envelope, safety)
}

pub(crate) async fn execute_pomv(args: PomvArgs) -> Result<(), String> {
    let client = args.connection.client()?;
    let (envelope, safety) = match args.command {
        PomvCommand::Use(use_args) => match use_args.command {
            PomvUseCommand::Prepare {
                target,
                recipient,
                selector,
                namespace,
                idempotency_key,
                expires_at,
                use_mode,
                public_permanent,
            } => {
                validate_hex32("target", &target)?;
                validate_hex32("recipient", &recipient)?;
                validate_hex32("selector", &selector)?;
                validate_idempotency_key(&idempotency_key)?;
                if namespace.trim().is_empty() {
                    return Err("--namespace cannot be empty".into());
                }
                if !public_permanent {
                    return Err("--public-permanent acknowledgement is required".into());
                }
                (
                    client
                        .call(
                            Method::POST,
                            "/api/vnext/pomv/public-use/prepare",
                            &[],
                            Some(json!({
                                "target_cid": target,
                                "recipient_node_id": recipient,
                                "selector_cid": selector,
                                "namespace": namespace,
                                "disclosure": {
                                    "classification": "public",
                                    "permanent": true,
                                    "use_mode": use_mode.wire_name()
                                },
                                "idempotency_key": idempotency_key,
                                "expires_at": expires_at
                            })),
                        )
                        .await?,
                    "PREPARED ONLY: no UseEvidence was created. Review exact payload, recipient, Public/permanent disclosure, and intent_cid before confirm.",
                )
            }
            PomvUseCommand::Confirm { intent } => {
                validate_hex32("intent", &intent)?;
                let typed = read_exact_intent_confirmation(&intent)?;
                verify_exact_confirmation(&intent, &typed)?;
                let receipt = confirmation_receipt(&intent)?;
                (
                    client
                        .call(
                            Method::POST,
                            "/api/vnext/pomv/public-use/confirm",
                            &[],
                            Some(json!({
                                "intent_cid": intent,
                                "single_use_receipt": receipt
                            })),
                        )
                        .await?,
                    "Public Use is committed only for the exact reviewed intent; delivery remains pending or deferred until separately acknowledged.",
                )
            }
            PomvUseCommand::Status { publication } => {
                validate_hex32("publication", &publication)?;
                (
                    client
                        .call(
                            Method::GET,
                            &format!("/api/vnext/pomv/publications/{publication}"),
                            &[],
                            None,
                        )
                        .await?,
                    "Publication status is pending/deferred unless durable authenticated delivery is separately acknowledged.",
                )
            }
        },
        PomvCommand::View { target } => {
            validate_hex32("target", &target)?;
            let envelope = client
                .call(
                    Method::GET,
                    &format!("/api/vnext/pomv/views/{target}"),
                    &[],
                    None,
                )
                .await?;
            enforce_view_firewalls(&envelope)?;
            (
                envelope,
                "Read-only partial evidence view: it establishes no truth or benefit, authorizes no reward, and claims no global completion.",
            )
        }
    };
    print_envelope(&envelope, safety)
}

pub(crate) async fn execute_vnext(args: VNextArgs) -> Result<(), String> {
    let client = args.connection.client()?;
    match args.command {
        VNextCommand::Status => {
            let envelope = client
                .call(Method::GET, "/api/vnext/runtime/status", &[], None)
                .await?;
            print_envelope(
                &envelope,
                "compiled, requested, active, kill-switch, signer readiness, lifecycle, and coverage are independent fields.",
            )
        }
    }
}

fn print_envelope(envelope: &Value, safety: &str) -> Result<(), String> {
    let formatted = serde_json::to_string_pretty(envelope)
        .map_err(|error| format!("could not format response: {error}"))?;
    println!("{formatted}");
    eprintln!("Safety: {safety}");
    Ok(())
}

fn read_exact_intent_confirmation(intent: &str) -> Result<String, String> {
    eprintln!("WARNING: confirmation creates a Public and permanent UseEvidence record.");
    eprintln!("Exact intent_cid: {intent}");
    eprint!("Type the exact intent_cid to confirm: ");
    io::stderr()
        .flush()
        .map_err(|error| format!("could not flush confirmation prompt: {error}"))?;
    let mut typed = String::new();
    io::stdin()
        .read_line(&mut typed)
        .map_err(|error| format!("could not read confirmation: {error}"))?;
    Ok(typed.trim().to_string())
}

fn verify_exact_confirmation(intent: &str, typed: &str) -> Result<(), String> {
    if intent != typed {
        return Err(
            "confirmation rejected: typed value did not equal the exact prepared intent_cid".into(),
        );
    }
    Ok(())
}

fn confirmation_receipt(intent: &str) -> Result<String, String> {
    validate_hex32("intent", intent)?;
    let mut intent_bytes = [0u8; 32];
    for (index, pair) in intent.as_bytes().chunks_exact(2).enumerate() {
        intent_bytes[index] = (decode_nibble(pair[0])? << 4) | decode_nibble(pair[1])?;
    }
    let mut digest = blake3::Hasher::new();
    digest.update(CONFIRMATION_DOMAIN);
    digest.update(&(intent_bytes.len() as u64).to_be_bytes());
    digest.update(&intent_bytes);
    Ok(format!(
        "{CONTINUATION_PREFIX}{}",
        URL_SAFE_NO_PAD.encode(digest.finalize().as_bytes())
    ))
}

fn decode_nibble(value: u8) -> Result<u8, String> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err("typed identifiers must use lowercase hexadecimal".into()),
    }
}

fn validate_hex32(name: &str, value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!(
            "--{name} must be exactly 64 lowercase hexadecimal characters"
        ));
    }
    Ok(())
}

fn validate_idempotency_key(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err("--idempotency-key must be 1..128 visible UTF-8 bytes".into());
    }
    Ok(())
}

fn validate_continuation(value: Option<&str>) -> Result<(), String> {
    if let Some(value) = value {
        if value.len() > 2_048 || !value.starts_with(CONTINUATION_PREFIX) {
            return Err("--continuation must be an opaque obc1 token".into());
        }
    }
    Ok(())
}

fn validate_page(limit: usize, continuation: Option<&str>) -> Result<(), String> {
    if limit == 0 || limit > 500 {
        return Err("--limit must be between 1 and 500".into());
    }
    validate_continuation(continuation)
}

fn enforce_quarantined_matches(envelope: &Value) -> Result<(), String> {
    let items = envelope
        .pointer("/data/items")
        .and_then(Value::as_array)
        .ok_or_else(|| "match response omitted its bounded items array".to_string())?;
    if items.iter().any(|item| {
        item.get("state").and_then(Value::as_str) != Some("quarantined")
            || item.get("executable").and_then(Value::as_bool) != Some(false)
    }) {
        return Err("match response violated the quarantined proposal firewall".into());
    }
    Ok(())
}

fn enforce_view_firewalls(envelope: &Value) -> Result<(), String> {
    let data = envelope
        .get("data")
        .ok_or_else(|| "view response omitted data".to_string())?;
    for field in [
        "establishes_truth",
        "establishes_benefit",
        "authorizes_reward",
        "claims_global_completion",
    ] {
        if data.get(field).and_then(Value::as_bool) != Some(false) {
            return Err(format!("view response violated the {field}=false firewall"));
        }
    }
    Ok(())
}

fn string_array(value: Option<&Value>) -> Vec<&str> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

fn format_limitations(limitations: &[&str]) -> String {
    if limitations.is_empty() {
        String::new()
    } else {
        format!("; limitations: {}", limitations.join(", "))
    }
}

#[cfg(feature = "vnext-network-runtime")]
pub(crate) fn prepare_runtime_dependencies(
    data_dir: &Path,
) -> Result<onebrain_node::VNextProductRuntimeDependencies, String> {
    use ku_core::foundation::{MetabolicViewPolicy, ObjectReference};
    use ku_kql::vnext_private_need::LocalNeedVaultKey;
    use onebrain_node::{LocalPolicyRegistry, LocalPolicyVersion};

    let vault_key = load_or_create_development_key(&data_dir.join(PRIVATE_NEED_VAULT_KEY_FILE))?;
    let policy_version = LocalPolicyVersion::new(1).map_err(|error| error.to_string())?;
    let policy_ref = ObjectReference::new(
        0,
        domain_hash(b"onebrain:cli:vnext-metabolic-policy:1\0", &vault_key),
    );
    let evidence_policy = ObjectReference::new(
        0,
        domain_hash(b"onebrain:cli:vnext-evidence-policy:1\0", &vault_key),
    );
    let policies = LocalPolicyRegistry::new([(
        policy_version,
        MetabolicViewPolicy {
            policy_ref,
            accepted_evidence_policies: vec![evidence_policy],
            recent_event_horizon: 1_024,
        },
    )])
    .map_err(|error| error.to_string())?;
    Ok(onebrain_node::VNextProductRuntimeDependencies::new(
        LocalNeedVaultKey::from_bytes(vault_key),
        policies,
    ))
}

#[cfg(feature = "vnext-network-runtime")]
pub(crate) fn prepare_feed_publisher(
    start: &VNextStartArgs,
    data_dir: &Path,
) -> Result<Option<onebrain_api::VNextFeedPublisher>, String> {
    use std::sync::Arc;

    use ed25519_dalek::SigningKey;
    use ku_core::foundation::{
        decode_feed_inception, DeviceId, FeedEventSigner, FeedInception, NamespaceCommitment,
    };

    match start.vnext_feed_signer_provider {
        FeedSignerProvider::None => Ok(None),
        FeedSignerProvider::DevelopmentFile => {
            if !start.allow_development_file_signer {
                return Err(
                    "development-file Feed signer requires --allow-development-file-signer".into(),
                );
            }
            let path = start
                .vnext_feed_key_file
                .clone()
                .unwrap_or_else(|| data_dir.join(DEVELOPMENT_FEED_KEY_FILE));
            let key = Arc::new(SigningKey::from_bytes(&load_or_create_development_key(
                &path,
            )?));
            let public_key = *key.verifying_key().as_bytes();
            let namespace = NamespaceCommitment::derive(
                b"onebrain-cli-development-feed",
                domain_hash(b"onebrain:cli:vnext-feed-namespace:1\0", &public_key),
            )
            .map_err(|error| format!("could not derive development Feed namespace: {error:?}"))?;
            let signed = FeedInception::new(
                public_key,
                namespace,
                0,
                DeviceId::from_bytes(domain_hash(
                    b"onebrain:cli:vnext-feed-device:1\0",
                    &public_key,
                )),
            )
            .sign(key.as_ref())
            .map_err(|error| format!("could not sign development Feed inception: {error}"))?;
            let author = decode_feed_inception(
                &signed
                    .encode()
                    .map_err(|error| format!("could not encode Feed inception: {error}"))?,
            )
            .map_err(|error| format!("could not validate Feed inception: {error}"))?;
            let signer: Arc<dyn FeedEventSigner> = key;
            onebrain_api::VNextFeedPublisher::new(author, signer).map(Some)
        }
    }
}

#[cfg(feature = "vnext-network-runtime")]
fn load_or_create_development_key(path: &Path) -> Result<[u8; 32], String> {
    use std::fs::OpenOptions;
    use std::io::{Read, Write};

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    match OpenOptions::new().read(true).open(path) {
        Ok(mut file) => {
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .map_err(|error| format!("could not read {}: {error}", path.display()))?;
            return bytes.try_into().map_err(|bytes: Vec<u8>| {
                format!(
                    "{} must contain exactly 32 raw bytes, found {}",
                    path.display(),
                    bytes.len()
                )
            });
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("could not open {}: {error}", path.display())),
    }

    let mut bytes = [0u8; 32];
    use rand::RngCore;
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("could not create {}: {error}", path.display()))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("could not persist {}: {error}", path.display()))?;
    Ok(bytes)
}

#[cfg(feature = "vnext-network-runtime")]
fn domain_hash(domain: &[u8], value: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(value);
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmation_requires_the_exact_intent_and_has_frozen_receipt() {
        let intent = "11".repeat(32);
        assert!(verify_exact_confirmation(&intent, &intent).is_ok());
        assert!(verify_exact_confirmation(&intent, &"12".repeat(32)).is_err());
        assert_eq!(
            confirmation_receipt(&intent).unwrap(),
            "obc1.Xty15PTowg0NrRClYO_Oqp3UAyS1HOQx_-N8DwH1Rh4"
        );
    }

    #[test]
    fn match_and_view_firewalls_fail_closed() {
        let matches = json!({
            "data": {"items": [{"state": "quarantined", "executable": false}]}
        });
        assert!(enforce_quarantined_matches(&matches).is_ok());
        let executable = json!({
            "data": {"items": [{"state": "authorized", "executable": true}]}
        });
        assert!(enforce_quarantined_matches(&executable).is_err());

        let view = json!({"data": {
            "establishes_truth": false,
            "establishes_benefit": false,
            "authorizes_reward": false,
            "claims_global_completion": false
        }});
        assert!(enforce_view_firewalls(&view).is_ok());
        let mut unsafe_view = view;
        unsafe_view["data"]["authorizes_reward"] = Value::Bool(true);
        assert!(enforce_view_firewalls(&unsafe_view).is_err());
    }

    #[test]
    fn start_configuration_is_safe_default_and_development_signer_is_double_opt_in() {
        let safe = VNextStartArgs::default();
        assert!(!safe.requested());
        assert_eq!(
            safe.feature_config().unwrap(),
            onebrain_node::VNextFeatureConfig::default()
        );

        let rejected = VNextStartArgs {
            vnext_public_use: true,
            vnext_feed_signer_provider: FeedSignerProvider::DevelopmentFile,
            ..Default::default()
        };
        assert!(rejected.feature_config().is_err());

        let enabled = VNextStartArgs {
            vnext_kql: true,
            vnext_public_use: true,
            vnext_pomv_view: true,
            vnext_feed_signer_provider: FeedSignerProvider::DevelopmentFile,
            allow_development_file_signer: true,
            ..Default::default()
        };
        let config = enabled.feature_config().unwrap();
        assert!(config.enabled.distributed_kql_one_hop);
        assert!(config.enabled.public_use_evidence_publish);
        assert!(config.enabled.distributed_pomv_view);
    }

    #[test]
    fn zero_result_and_quarantined_wording_remain_scope_honest() {
        let zero =
            "No match in this bounded partial page; this is not evidence of network-wide absence.";
        let nonzero = "Every result is a quarantined proposal (executable=false); listing cannot materialize or adopt it.";
        assert!(zero.contains("not evidence of network-wide absence"));
        assert!(nonzero.contains("quarantined proposal"));
        assert!(!nonzero.contains("Authorized"));
    }

    #[cfg(feature = "vnext-network-runtime")]
    #[tokio::test]
    async fn real_cli_client_preserves_need_and_public_use_replay_identity() {
        use std::sync::Arc;
        use std::time::{Duration, SystemTime, UNIX_EPOCH};

        let directory = tempfile::tempdir().unwrap();
        let start = VNextStartArgs {
            vnext_kql: true,
            vnext_public_use: true,
            vnext_pomv_view: true,
            vnext_feed_signer_provider: FeedSignerProvider::DevelopmentFile,
            allow_development_file_signer: true,
            ..Default::default()
        };
        let config = onebrain_node::NodeConfig {
            port: 0,
            data_dir: directory.path().to_path_buf(),
            concept_registry_mode: onebrain_node::ConceptRegistryMode::Disabled,
            vnext: start.feature_config().unwrap(),
            ..Default::default()
        };
        let mut node = onebrain_node::OneBrainNode::new(config).await.unwrap();
        node.set_vnext_product_dependencies(
            prepare_runtime_dependencies(directory.path()).unwrap(),
        )
        .unwrap();
        node.start_network().await.unwrap();
        let shared = Arc::new(tokio::sync::Mutex::new(node));
        let publisher = prepare_feed_publisher(&start, directory.path())
            .unwrap()
            .unwrap();

        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let api_port = probe.local_addr().unwrap().port();
        drop(probe);
        let server = onebrain_api::ApiServer::with_shared_node(
            Arc::clone(&shared),
            "p3-cli-test-token".into(),
            api_port,
        )
        .with_vnext_feed_publisher(publisher);
        let server_task = tokio::spawn(async move {
            server.start().await.unwrap();
        });
        let client = VNextApiClient::new(
            &format!("http://127.0.0.1:{api_port}"),
            "p3-cli-test-token".into(),
        )
        .unwrap();
        let mut ready = false;
        for _ in 0..50 {
            if client
                .call(Method::GET, "/api/vnext/runtime/status", &[], None)
                .await
                .is_ok()
            {
                ready = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(ready, "test API did not become ready");

        let need_request = json!({
            "local_query": "FIND (secret:KU) WHERE secret.title = \"CliPrivateMarker\" SCOPE LOCAL",
            "scope": {"kind": "one_hop", "max_hops": 1, "node_ids": []},
            "budget": BudgetArgs {
                max_scan_records: 64,
                max_affordances: 32,
                max_pairs: 128,
                max_proposals: 32,
            }.json(),
            "idempotency_key": "p3-cli-need-1"
        });
        let prepared = client
            .call(
                Method::POST,
                "/api/vnext/kql/needs/prepare",
                &[],
                Some(need_request.clone()),
            )
            .await
            .unwrap();
        let prepared_replay = client
            .call(
                Method::POST,
                "/api/vnext/kql/needs/prepare",
                &[],
                Some(need_request),
            )
            .await
            .unwrap();
        assert_eq!(
            prepared.pointer("/data/intent_cid"),
            prepared_replay.pointer("/data/intent_cid")
        );
        let intent = prepared
            .pointer("/data/intent_cid")
            .and_then(Value::as_str)
            .unwrap();
        let activation = json!({
            "intent_cid": intent,
            "idempotency_key": "p3-cli-need-1"
        });
        let active = client
            .call(
                Method::POST,
                "/api/vnext/kql/needs",
                &[],
                Some(activation.clone()),
            )
            .await
            .unwrap();
        let active_replay = client
            .call(Method::POST, "/api/vnext/kql/needs", &[], Some(activation))
            .await
            .unwrap();
        assert_eq!(
            active.pointer("/data/standing_need_id"),
            active_replay.pointer("/data/standing_need_id")
        );

        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 300;
        let public_prepare = client
            .call(
                Method::POST,
                "/api/vnext/pomv/public-use/prepare",
                &[],
                Some(json!({
                    "target_cid": "41".repeat(32),
                    "recipient_node_id": "42".repeat(32),
                    "selector_cid": "43".repeat(32),
                    "namespace": "p3-cli-public-use",
                    "disclosure": {
                        "classification": "public",
                        "permanent": true,
                        "use_mode": "application"
                    },
                    "idempotency_key": "p3-cli-public-use-1",
                    "expires_at": expires_at
                })),
            )
            .await
            .unwrap();
        assert!(public_prepare
            .to_string()
            .contains("exact_payload_recipient_and_public_permanence_require_review"));
        assert!(!public_prepare.to_string().contains("single_use_receipt"));
        let public_intent = public_prepare
            .pointer("/data/intent_cid")
            .and_then(Value::as_str)
            .unwrap();
        let confirmation = json!({
            "intent_cid": public_intent,
            "single_use_receipt": confirmation_receipt(public_intent).unwrap()
        });
        let publication = client
            .call(
                Method::POST,
                "/api/vnext/pomv/public-use/confirm",
                &[],
                Some(confirmation.clone()),
            )
            .await
            .unwrap();
        let publication_replay = client
            .call(
                Method::POST,
                "/api/vnext/pomv/public-use/confirm",
                &[],
                Some(confirmation),
            )
            .await
            .unwrap();
        assert_eq!(
            publication.pointer("/data/publication_cid"),
            publication_replay.pointer("/data/publication_cid")
        );

        server_task.abort();
        shared.lock().await.shutdown_network().await;
    }
}
