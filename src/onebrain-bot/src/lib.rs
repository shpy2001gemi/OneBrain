#![forbid(unsafe_code)]
//! Restricted `BotPrincipal` DMZ chat platform adapter (`P10-SURFACES-SYNC-001` §4.2).
//!
//! Enforces three non-bypassable cross-pillar invariants:
//! 1. **`NEG-P10-003` Multi-Tenant Isolation:** `BotPrincipal` holds zero `PrivateVault`,
//!    `PrivateNeedVault`, `obm1.` admin, or `FeedEventSigner` credentials and may only invoke
//!    `/api/vnext/bot/query` (`PUBLIC` `ValidatedStore` reads) and `/api/vnext/bot/submissions`.
//! 2. **Public-Only Answers with Canonical Epistemic Badges (`P10 §2.2` & `§4.2`):** Non-public
//!    records fail closed before rendering; every response carries compact provenance CIDs and
//!    canonical `[Public · Unassessed]` / `[Public · Fidelity Corroborated (k groups)]` +
//!    `[Local Store Only]` / `[Sampled Reachable Peers: n]` badges (never `FULL`/`GLOBAL`/`CLOSED`).
//! 3. **Quarantined Inbound Submissions (`FW-01` / `FW-02`):** External user chat submissions
//!    always wrap into non-executable quarantine records (`is_executable = false`,
//!    `untrusted_external_provenance = true`) and never confirm step-2 `PUBLIC` publication
//!    or access `P9` somatic streams (`T-BCI-04`).

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Maximum UTF-8 payload accepted from an untrusted external chat message.
pub const MAX_BOT_SUBMISSION_BYTES: usize = 16_384;
/// Maximum bounded query string accepted from a chat command.
pub const MAX_BOT_QUERY_BYTES: usize = 512;
/// Canonical token prefix for scoped Pillar 10 surface capabilities.
pub const SURFACE_TOKEN_PREFIX: &str = "obc_surface1.";
/// Domain separation label for `obc_surface1` keyed BLAKE3 MAC verification.
pub const SURFACE_TOKEN_MAC_DOMAIN: &str = "onebrain:vnext:obc-surface-mac:1";
/// Domain separation label for quarantined external chat submission digests.
pub const BOT_SUBMISSION_DIGEST_DOMAIN: &[u8] = b"onebrain:vnext:bot-quarantine-submission:1\0";

/// Permitted DMZ API endpoints for `onebrain-bot`.
pub const BOT_QUERY_ENDPOINT: &str = "/api/vnext/bot/query";
pub const BOT_SUBMISSIONS_ENDPOINT: &str = "/api/vnext/bot/submissions";

/// Supported external chat platforms bridged through the DMZ adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatPlatform {
    Discord,
    Telegram,
    Slack,
    Matrix,
}

impl ChatPlatform {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Discord => "discord",
            Self::Telegram => "telegram",
            Self::Slack => "slack",
            Self::Matrix => "matrix",
        }
    }
}

/// Decoded claims of an `obc_surface1` token restricted to `surface_kind == "bot"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotSurfaceClaimsV1 {
    pub version: u8,
    pub surface_kind: String,
    pub token_id: String,
    pub channel_binding: String,
    pub permissions: Vec<String>,
    pub issued_at_unix_s: u64,
    pub expires_at_unix_s: u64,
    pub revocation_epoch: u64,
}

/// Errors enforced fail-closed by `onebrain-bot`.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum BotAdapterError {
    #[error("management token obm1. is prohibited on bot surfaces (NEG-P10-003)")]
    AdminTokenProhibited,
    #[error("bot surface requires a scoped obc_surface1 token")]
    InvalidTokenFormat,
    #[error("obc_surface1 MAC verification failed")]
    InvalidTokenMac,
    #[error("obc_surface1 surface_kind '{0}' is not 'bot'")]
    WrongSurfaceKind(String),
    #[error("obc_surface1 token expired at {expires_at_unix_s} (now {now_unix_s})")]
    TokenExpired {
        expires_at_unix_s: u64,
        now_unix_s: u64,
    },
    #[error("obc_surface1 channel binding mismatch: expected '{expected}', got '{actual}'")]
    ChannelBindingMismatch { expected: String, actual: String },
    #[error("obc_surface1 contains prohibited permission '{0}' for BotPrincipal")]
    ProhibitedPermission(String),
    #[error("endpoint '{0}' is forbidden for BotPrincipal (NEG-P10-003 / T-BCI-04)")]
    ForbiddenEndpoint(String),
    #[error("step-2 PUBLIC confirmation is prohibited on bot surfaces (NEG-P10-003)")]
    PublicConfirmProhibited,
    #[error("somatic P9 access is prohibited on bot surfaces (T-BCI-04)")]
    SomaticAccessProhibited,
    #[error("non-public disclosure '{0}' refused by BotPrincipal public-only firewall")]
    NonPublicDisclosureRefused(String),
    #[error("forbidden completeness alias '{0}' detected in surface badge")]
    ForbiddenCompletenessAlias(String),
    #[error("query is empty or exceeds {MAX_BOT_QUERY_BYTES} bytes")]
    InvalidQueryBounds,
    #[error("submission text is empty or exceeds {MAX_BOT_SUBMISSION_BYTES} bytes")]
    InvalidSubmissionBounds,
    #[error("transport error: {0}")]
    Transport(String),
}

/// Restricted DMZ principal holding ONLY a validated `obc_surface1` bot capability.
///
/// Contains zero `PrivateVault`, `PrivateNeedVault`, `obm1`, or `FeedEventSigner` fields.
#[derive(Debug, Clone)]
pub struct BotPrincipal {
    raw_token: String,
    claims: BotSurfaceClaimsV1,
}

impl BotPrincipal {
    /// Allowed minimal permission vocabulary for `BotPrincipal`.
    const ALLOWED_PERMISSIONS: &'static [&'static str] =
        &["public_validated_read", "quarantined_external_submit"];

    /// Parse and validate a scoped `obc_surface1` token for a specific platform channel binding.
    pub fn from_scoped_token(
        raw_token: &str,
        expected_channel_binding: &str,
        now_unix_s: u64,
        optional_mac_key: Option<&[u8; 32]>,
    ) -> Result<Self, BotAdapterError> {
        let trimmed = raw_token.trim();
        if trimmed.starts_with("obm1.") {
            return Err(BotAdapterError::AdminTokenProhibited);
        }
        let Some(rest) = trimmed.strip_prefix(SURFACE_TOKEN_PREFIX) else {
            return Err(BotAdapterError::InvalidTokenFormat);
        };
        let mut parts = rest.split('.');
        let (Some(claims_b64), Some(mac_b64), None) = (parts.next(), parts.next(), parts.next())
        else {
            return Err(BotAdapterError::InvalidTokenFormat);
        };
        let claims_bytes = URL_SAFE_NO_PAD
            .decode(claims_b64)
            .map_err(|_| BotAdapterError::InvalidTokenFormat)?;
        let mac_bytes = URL_SAFE_NO_PAD
            .decode(mac_b64)
            .map_err(|_| BotAdapterError::InvalidTokenFormat)?;
        if mac_bytes.len() != 32 {
            return Err(BotAdapterError::InvalidTokenFormat);
        }
        if let Some(mac_key) = optional_mac_key {
            let expected_mac = compute_surface_token_mac(mac_key, &claims_bytes);
            if !constant_time_eq_32(&expected_mac, &mac_bytes) {
                return Err(BotAdapterError::InvalidTokenMac);
            }
        }

        let claims: BotSurfaceClaimsV1 = serde_json::from_slice(&claims_bytes)
            .map_err(|_| BotAdapterError::InvalidTokenFormat)?;
        if claims.version != 1 {
            return Err(BotAdapterError::InvalidTokenFormat);
        }
        if claims.surface_kind != "bot" {
            return Err(BotAdapterError::WrongSurfaceKind(claims.surface_kind));
        }
        if now_unix_s >= claims.expires_at_unix_s {
            return Err(BotAdapterError::TokenExpired {
                expires_at_unix_s: claims.expires_at_unix_s,
                now_unix_s,
            });
        }
        if claims.channel_binding != expected_channel_binding {
            return Err(BotAdapterError::ChannelBindingMismatch {
                expected: expected_channel_binding.to_owned(),
                actual: claims.channel_binding,
            });
        }
        for perm in &claims.permissions {
            if !Self::ALLOWED_PERMISSIONS.contains(&perm.as_str()) {
                return Err(BotAdapterError::ProhibitedPermission(perm.clone()));
            }
        }

        Ok(Self {
            raw_token: trimmed.to_owned(),
            claims,
        })
    }

    pub fn bearer_header_value(&self) -> String {
        format!("Bearer {}", self.raw_token)
    }

    pub fn claims(&self) -> &BotSurfaceClaimsV1 {
        &self.claims
    }

    /// Enforce route-level capability ceiling before dispatching any outbound HTTP call.
    pub fn authorize_endpoint(&self, method: &str, path: &str) -> Result<(), BotAdapterError> {
        if path.contains("/bci") || path.contains("/somatic") || path.contains("/hud") {
            return Err(BotAdapterError::SomaticAccessProhibited);
        }
        if path.contains("share_confirm") || path.contains("public-use/confirm") {
            return Err(BotAdapterError::PublicConfirmProhibited);
        }
        let allowed = matches!(
            (method, path),
            ("POST", BOT_QUERY_ENDPOINT) | ("POST", BOT_SUBMISSIONS_ENDPOINT)
        );
        if !allowed {
            return Err(BotAdapterError::ForbiddenEndpoint(format!(
                "{method} {path}"
            )));
        }
        Ok(())
    }
}

/// Mint a deterministic `obc_surface1` bot token (used by host tests & token fixtures).
pub fn mint_bot_surface_token(
    claims: &BotSurfaceClaimsV1,
    mac_key: &[u8; 32],
) -> Result<String, serde_json::Error> {
    let claims_bytes = serde_json::to_vec(claims)?;
    let mac = compute_surface_token_mac(mac_key, &claims_bytes);
    Ok(format!(
        "{SURFACE_TOKEN_PREFIX}{}.{}",
        URL_SAFE_NO_PAD.encode(&claims_bytes),
        URL_SAFE_NO_PAD.encode(mac)
    ))
}

pub fn compute_surface_token_mac(mac_key: &[u8; 32], claims_bytes: &[u8]) -> [u8; 32] {
    let derived = blake3::derive_key(SURFACE_TOKEN_MAC_DOMAIN, mac_key);
    *blake3::keyed_hash(&derived, claims_bytes).as_bytes()
}

fn constant_time_eq_32(expected: &[u8; 32], actual: &[u8]) -> bool {
    if actual.len() != 32 {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in expected.iter().zip(actual.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// Public-only query request sent to `POST /api/vnext/bot/query`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotPublicQueryRequestV1 {
    pub platform: ChatPlatform,
    pub channel_id: String,
    pub query: String,
    pub max_results: u8,
}

/// Single public KU item returned from `ValidatedStore` (`PUBLIC` only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotPublicHitV1 {
    pub object_cid: String,
    pub semantic_cid: String,
    pub summary: String,
    pub disclosure_class: String,
    pub model_qualified: bool,
    pub corroborated_fidelity_groups: u32,
}

/// Query coverage scope reported honestly without `FULL`/`GLOBAL`/`CLOSED` aliases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BotCoverageScope {
    LocalStoreOnly,
    SampledReachablePeers { peer_count: u32 },
}

impl BotCoverageScope {
    pub fn canonical_badge(self) -> String {
        match self {
            Self::LocalStoreOnly => "[Local Store Only]".to_owned(),
            Self::SampledReachablePeers { peer_count } => {
                format!("[Sampled Reachable Peers: {peer_count}]")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotPublicQueryResponseV1 {
    pub coverage: BotCoverageScope,
    pub hits: Vec<BotPublicHitV1>,
}

/// Untrusted external chat submission routed strictly into `QuarantineRecord`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalChatSubmissionInput {
    pub platform: ChatPlatform,
    pub channel_id: String,
    pub external_user_id: String,
    pub message_id: String,
    pub raw_text: String,
    pub observed_at_unix_s: u64,
}

/// Non-executable quarantined submission payload (`is_executable = false` invariant).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuarantinedBotSubmissionV1 {
    pub platform: ChatPlatform,
    pub channel_id: String,
    pub external_user_id: String,
    pub message_id: String,
    pub sanitized_text: String,
    pub submission_digest_hex: String,
    pub observed_at_unix_s: u64,
    pub target_store: String,
    pub is_executable: bool,
    pub untrusted_external_provenance: bool,
    pub disclosure_ceiling: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuarantinedBotSubmissionReceiptV1 {
    pub quarantine_record_id: String,
    pub submission_digest_hex: String,
    pub is_executable: bool,
    pub status_badge: String,
}

/// Sanitize and wrap an external user's chat submission for quarantine intake.
pub fn build_quarantined_submission(
    input: ExternalChatSubmissionInput,
) -> Result<QuarantinedBotSubmissionV1, BotAdapterError> {
    let cleaned: String = input
        .raw_text
        .chars()
        .filter(|ch| !matches!(*ch, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\0'))
        .collect::<String>()
        .trim()
        .to_owned();
    if cleaned.is_empty() || cleaned.len() > MAX_BOT_SUBMISSION_BYTES {
        return Err(BotAdapterError::InvalidSubmissionBounds);
    }

    let mut hasher = blake3::Hasher::new();
    hasher.update(BOT_SUBMISSION_DIGEST_DOMAIN);
    hasher.update(input.platform.as_str().as_bytes());
    hasher.update(&[0]);
    hasher.update(input.channel_id.as_bytes());
    hasher.update(&[0]);
    hasher.update(input.external_user_id.as_bytes());
    hasher.update(&[0]);
    hasher.update(input.message_id.as_bytes());
    hasher.update(&[0]);
    hasher.update(cleaned.as_bytes());
    let submission_digest_hex = hex::encode(hasher.finalize().as_bytes());

    Ok(QuarantinedBotSubmissionV1 {
        platform: input.platform,
        channel_id: input.channel_id,
        external_user_id: input.external_user_id,
        message_id: input.message_id,
        sanitized_text: cleaned,
        submission_digest_hex,
        observed_at_unix_s: input.observed_at_unix_s,
        target_store: "quarantine_only".to_owned(),
        is_executable: false,
        untrusted_external_provenance: true,
        disclosure_ceiling: "quarantined_unassessed".to_owned(),
    })
}

/// Render a public-only channel response with mandatory provenance CIDs and epistemic badges.
pub fn render_public_channel_reply(
    response: &BotPublicQueryResponseV1,
) -> Result<String, BotAdapterError> {
    let coverage_badge = response.coverage.canonical_badge();
    reject_completeness_aliases(&coverage_badge)?;

    if response.hits.is_empty() {
        return Ok(format!(
            "{coverage_badge} No matching PUBLIC Knowledge Units in local store (not a claim of global absence)."
        ));
    }

    let mut lines = Vec::with_capacity(response.hits.len() + 1);
    lines.push(format!(
        "{coverage_badge} Public ValidatedStore Results ({}):",
        response.hits.len()
    ));

    for hit in &response.hits {
        if hit.disclosure_class != "public" {
            return Err(BotAdapterError::NonPublicDisclosureRefused(
                hit.disclosure_class.clone(),
            ));
        }
        let epistemic_badge = if hit.corroborated_fidelity_groups >= 2 {
            format!(
                "[Public · Fidelity Corroborated ({} groups)]",
                hit.corroborated_fidelity_groups
            )
        } else {
            "[Public · Unassessed]".to_owned()
        };
        reject_completeness_aliases(&epistemic_badge)?;
        let compact_obj = compact_cid(&hit.object_cid);
        let compact_sem = compact_cid(&hit.semantic_cid);
        lines.push(format!(
            "- {epistemic_badge} `{compact_obj}` (sem `{compact_sem}`): {}",
            hit.summary.trim()
        ));
    }

    Ok(lines.join("\n"))
}

fn compact_cid(cid: &str) -> &str {
    if cid.len() > 16 {
        &cid[..16]
    } else {
        cid
    }
}

fn reject_completeness_aliases(text: &str) -> Result<(), BotAdapterError> {
    for forbidden in ["FULL", "GLOBAL", "CLOSED"] {
        if text
            .split(|c: char| !c.is_ascii_alphabetic())
            .any(|w| w == forbidden)
        {
            return Err(BotAdapterError::ForbiddenCompletenessAlias(
                forbidden.to_owned(),
            ));
        }
    }
    Ok(())
}

/// Narrow DMZ transport port so unit/integration tests and live HTTP adapters share identical guards.
pub trait BotApiTransport {
    fn post_json(
        &self,
        endpoint: &str,
        bearer_header: &str,
        body_json: &str,
    ) -> Result<String, String>;
}

/// Stateless DMZ bot adapter enforcing `BotPrincipal` route and disclosure boundaries.
pub struct BotDmzAdapter<T> {
    principal: BotPrincipal,
    transport: T,
}

impl<T: BotApiTransport> BotDmzAdapter<T> {
    pub fn new(principal: BotPrincipal, transport: T) -> Self {
        Self {
            principal,
            transport,
        }
    }

    pub fn principal(&self) -> &BotPrincipal {
        &self.principal
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Execute a `PUBLIC`-only query and format the channel message.
    pub fn handle_channel_query(
        &self,
        platform: ChatPlatform,
        channel_id: &str,
        raw_query: &str,
        max_results: u8,
    ) -> Result<String, BotAdapterError> {
        let query = raw_query.trim();
        if query.is_empty() || query.len() > MAX_BOT_QUERY_BYTES {
            return Err(BotAdapterError::InvalidQueryBounds);
        }
        self.principal
            .authorize_endpoint("POST", BOT_QUERY_ENDPOINT)?;
        let req = BotPublicQueryRequestV1 {
            platform,
            channel_id: channel_id.to_owned(),
            query: query.to_owned(),
            max_results: max_results.clamp(1, 10),
        };
        let payload =
            serde_json::to_string(&req).map_err(|e| BotAdapterError::Transport(e.to_string()))?;
        let raw_resp = self
            .transport
            .post_json(
                BOT_QUERY_ENDPOINT,
                &self.principal.bearer_header_value(),
                &payload,
            )
            .map_err(BotAdapterError::Transport)?;
        let parsed: BotPublicQueryResponseV1 = serde_json::from_str(&raw_resp)
            .map_err(|e| BotAdapterError::Transport(e.to_string()))?;
        render_public_channel_reply(&parsed)
    }

    /// Submit an untrusted external user message strictly into `QuarantineRecord`.
    pub fn handle_external_submission(
        &self,
        input: ExternalChatSubmissionInput,
    ) -> Result<QuarantinedBotSubmissionReceiptV1, BotAdapterError> {
        self.principal
            .authorize_endpoint("POST", BOT_SUBMISSIONS_ENDPOINT)?;
        let submission = build_quarantined_submission(input)?;
        let payload = serde_json::to_string(&submission)
            .map_err(|e| BotAdapterError::Transport(e.to_string()))?;
        let raw_resp = self
            .transport
            .post_json(
                BOT_SUBMISSIONS_ENDPOINT,
                &self.principal.bearer_header_value(),
                &payload,
            )
            .map_err(BotAdapterError::Transport)?;
        let receipt: QuarantinedBotSubmissionReceiptV1 = serde_json::from_str(&raw_resp)
            .map_err(|e| BotAdapterError::Transport(e.to_string()))?;
        if receipt.is_executable {
            return Err(BotAdapterError::Transport(
                "quarantine violation: server returned executable=true".to_owned(),
            ));
        }
        Ok(receipt)
    }

    /// Explicit test/guard probe verifying arbitrary non-bot endpoints are blocked before network I/O.
    pub fn probe_raw_endpoint(&self, method: &str, path: &str) -> Result<(), BotAdapterError> {
        self.principal.authorize_endpoint(method, path)
    }
}
