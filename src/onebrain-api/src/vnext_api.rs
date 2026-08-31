//! Additive vNext REST contract.
//!
//! These endpoints intentionally use a separate envelope and error vocabulary
//! from the legacy API. Private preparation capabilities stay in this local,
//! authenticated process; durable Need/publication/view state remains owned by
//! `OneBrainNode` and is reached only through `VNextProductServices`.

#![cfg_attr(not(feature = "vnext-network-runtime"), allow(dead_code))]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::rejection::JsonRejection;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::server::AppState;

pub const VNEXT_PRODUCT_PROFILE: &str = "VNEXT_PRODUCT_INTEGRATION_PROFILE_V1";
const MAX_IDEMPOTENCY_KEY_BYTES: usize = 128;
const MAX_LOCAL_QUERY_BYTES: usize = 64 * 1024;
const MAX_PAGE_LIMIT: usize = 500;
const DEFAULT_PAGE_LIMIT: usize = 100;
const MAX_PREPARED_API_CAPABILITIES: usize = 4_096;
const NEED_PREPARE_TTL_SECONDS: u64 = 15 * 60;
const CONTINUATION_PREFIX: &str = "obc1.";

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleV1 {
    Disabled,
    Requested,
    Active,
    Degraded,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoverageV1 {
    LocalOnly,
    Partial,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct VNextMetaV1 {
    pub lifecycle: LifecycleV1,
    pub coverage: CoverageV1,
    pub limitations: Vec<String>,
    pub continuation: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct VNextSuccessV1<T: Serialize> {
    pub ok: bool,
    pub profile: &'static str,
    pub data: T,
    pub meta: VNextMetaV1,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct VNextErrorV1 {
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
    pub limitations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct VNextErrorEnvelopeV1 {
    pub ok: bool,
    pub profile: &'static str,
    pub error: VNextErrorV1,
    pub meta: VNextMetaV1,
}

#[derive(Debug)]
pub struct VNextHttpError {
    status: StatusCode,
    body: Box<VNextErrorEnvelopeV1>,
}

impl VNextHttpError {
    pub(crate) fn new(
        status: StatusCode,
        code: &'static str,
        message: impl Into<String>,
        retryable: bool,
        lifecycle: LifecycleV1,
        coverage: CoverageV1,
        limitations: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        let limitations = limitations.into_iter().map(Into::into).collect::<Vec<_>>();
        Self {
            status,
            body: Box::new(VNextErrorEnvelopeV1 {
                ok: false,
                profile: VNEXT_PRODUCT_PROFILE,
                error: VNextErrorV1 {
                    code,
                    message: message.into(),
                    retryable,
                    limitations: limitations.clone(),
                },
                meta: VNextMetaV1 {
                    lifecycle,
                    coverage,
                    limitations,
                    continuation: None,
                },
            }),
        }
    }

    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            message,
            false,
            LifecycleV1::Degraded,
            CoverageV1::LocalOnly,
            ["request_rejected_before_runtime_side_effects"],
        )
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "not_found",
            message,
            false,
            LifecycleV1::Active,
            CoverageV1::LocalOnly,
            ["exact_typed_identifier_not_present_locally"],
        )
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::CONFLICT,
            "conflict",
            message,
            false,
            LifecycleV1::Degraded,
            CoverageV1::Partial,
            ["explicit_resolution_required"],
        )
    }

    fn expired(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::GONE,
            "expired",
            message,
            false,
            LifecycleV1::Degraded,
            CoverageV1::LocalOnly,
            ["prepare_again_and_review_the_new_exact_intent"],
        )
    }

    fn disabled(message: impl Into<String>, requested: bool) -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "capability_disabled",
            message,
            false,
            if requested {
                LifecycleV1::Requested
            } else {
                LifecycleV1::Disabled
            },
            CoverageV1::LocalOnly,
            ["vnext_lane_has_no_active_runtime_owner"],
        )
    }

    fn dependency(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "dependency_unavailable",
            message,
            true,
            LifecycleV1::Degraded,
            CoverageV1::Partial,
            ["retry_with_the_same_idempotency_identity"],
        )
    }
}

impl IntoResponse for VNextHttpError {
    fn into_response(self) -> Response {
        (self.status, Json(*self.body)).into_response()
    }
}

pub(crate) type VNextResult<T> = Result<Json<VNextSuccessV1<T>>, VNextHttpError>;

pub(crate) fn success<T: Serialize>(data: T, meta: VNextMetaV1) -> Json<VNextSuccessV1<T>> {
    Json(VNextSuccessV1 {
        ok: true,
        profile: VNEXT_PRODUCT_PROFILE,
        data,
        meta,
    })
}

pub(crate) fn active_meta(
    coverage: CoverageV1,
    limitations: impl IntoIterator<Item = impl Into<String>>,
    continuation: Option<String>,
) -> VNextMetaV1 {
    VNextMetaV1 {
        lifecycle: LifecycleV1::Active,
        coverage,
        limitations: limitations.into_iter().map(Into::into).collect(),
        continuation,
    }
}

fn parse_json<T>(body: Result<Json<T>, JsonRejection>) -> Result<T, VNextHttpError> {
    body.map(|Json(value)| value)
        .map_err(|error| VNextHttpError::invalid(format!("invalid JSON body: {error}")))
}

fn parse_query<T>(query: Result<Query<T>, QueryRejection>) -> Result<T, VNextHttpError> {
    query
        .map(|Query(value)| value)
        .map_err(|error| VNextHttpError::invalid(format!("invalid query parameters: {error}")))
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScopeKindV1 {
    OneHop,
    AuthenticatedDirectPeers,
}

fn one_hop() -> u8 {
    1
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScopeV1 {
    pub kind: ScopeKindV1,
    #[serde(default = "one_hop")]
    pub max_hops: u8,
    #[serde(default)]
    pub node_ids: Vec<String>,
}

impl ScopeV1 {
    fn validate_need(&self) -> Result<(), VNextHttpError> {
        if self.kind != ScopeKindV1::OneHop || self.max_hops != 1 || !self.node_ids.is_empty() {
            return Err(VNextHttpError::invalid(
                "Need scope must be exactly one_hop with max_hops=1 and no caller-supplied peers",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BudgetV1 {
    pub max_scan_records: u64,
    pub max_affordances: u64,
    pub max_pairs: u64,
    pub max_proposals: u64,
}

impl Default for BudgetV1 {
    fn default() -> Self {
        Self {
            max_scan_records: 4_096,
            max_affordances: 1_024,
            max_pairs: 65_536,
            max_proposals: 4_096,
        }
    }
}

impl BudgetV1 {
    fn validate(self) -> Result<(), VNextHttpError> {
        if self.max_scan_records == 0
            || self.max_scan_records > 1_000_000
            || self.max_affordances == 0
            || self.max_affordances > 65_536
            || self.max_affordances > self.max_scan_records
            || self.max_pairs == 0
            || self.max_pairs > 1_000_000
            || self.max_proposals == 0
            || self.max_proposals > 65_536
        {
            return Err(VNextHttpError::invalid(
                "budget is outside the bounded distributed KQL profile",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NeedPrepareRequestV1 {
    pub local_query: String,
    pub scope: ScopeV1,
    pub budget: BudgetV1,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct PreparedNeedV1 {
    pub intent_cid: String,
    pub query_definition_cid: String,
    pub selector_cid: String,
    pub scope: ScopeV1,
    pub budget: BudgetV1,
    pub expires_at: u64,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NeedActivationRequestV1 {
    pub intent_cid: String,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct NeedViewV1 {
    pub standing_need_id: String,
    pub state: String,
    pub query_definition_cid: String,
    pub selector_cid: String,
    pub coverage: CoverageV1,
    pub limitations: Vec<String>,
    pub revision: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct NeedPageV1 {
    pub items: Vec<NeedViewV1>,
    pub coverage: CoverageV1,
    pub limitations: Vec<String>,
    pub continuation: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NeedScanRequestV1 {
    pub budget: BudgetV1,
    pub continuation: Option<String>,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct ConstraintObservationV1 {
    pub constraint_index: u32,
    pub evaluation: String,
    pub required: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct ConstraintSetV1 {
    pub observations: Vec<ConstraintObservationV1>,
    pub all_required_satisfied: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct QuarantinedMatchV1 {
    pub proposal_cid: String,
    pub candidate_cid: String,
    pub responder_scope: ScopeV1,
    pub selector_cid: String,
    pub assessed_frontier: String,
    pub constraints: ConstraintSetV1,
    pub limitations: Vec<String>,
    pub state: &'static str,
    pub executable: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct MatchPageV1 {
    pub items: Vec<QuarantinedMatchV1>,
    pub coverage: CoverageV1,
    pub limitations: Vec<String>,
    pub continuation: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UseModeV1 {
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DisclosureV1 {
    pub classification: String,
    pub permanent: bool,
    pub use_mode: UseModeV1,
}

impl DisclosureV1 {
    fn validate(&self) -> Result<(), VNextHttpError> {
        if self.classification != "public" || !self.permanent {
            return Err(VNextHttpError::invalid(
                "Public Use disclosure must explicitly acknowledge public and permanent publication",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicUsePrepareRequestV1 {
    pub target_cid: String,
    pub recipient_node_id: String,
    pub selector_cid: String,
    pub namespace: String,
    pub disclosure: DisclosureV1,
    pub idempotency_key: String,
    pub expires_at: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct PreparedPublicUseV1 {
    pub intent_cid: String,
    pub canonical_payload_preview: String,
    pub exact_target: String,
    pub exact_recipient: String,
    pub selector_cid: String,
    pub namespace: String,
    pub disclosure: DisclosureV1,
    pub idempotency_key: String,
    pub expires_at: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicUseConfirmRequestV1 {
    pub intent_cid: String,
    pub single_use_receipt: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct PublicationViewV1 {
    pub publication_cid: String,
    pub intent_cid: String,
    pub state: String,
    pub attempts: u64,
    pub limitations: Vec<String>,
    pub revision: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct MetabolicEvidenceViewV1 {
    pub target_cid: String,
    pub policy_cid: String,
    pub assessed_frontier: String,
    pub revision: u64,
    pub use_event_root: String,
    pub conflicts: Vec<String>,
    pub coverage: CoverageV1,
    pub limitations: Vec<String>,
    pub establishes_truth: bool,
    pub establishes_benefit: bool,
    pub authorizes_reward: bool,
    pub claims_global_completion: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct RuntimeStatusV1 {
    pub compiled: bool,
    pub requested: bool,
    pub active: bool,
    pub kill_switch: bool,
    pub signer_ready: bool,
    pub lifecycle: LifecycleV1,
    pub coverage: CoverageV1,
    pub observability: onebrain_node::VNextObservabilitySnapshot,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageQueryV1 {
    pub continuation: Option<String>,
    pub limit: Option<usize>,
}

#[cfg(feature = "vnext-network-runtime")]
#[derive(Clone)]
pub struct VNextFeedPublisher {
    pub(crate) author: ku_core::foundation::ValidatedFeedInception,
    pub(crate) signer: Arc<dyn ku_core::foundation::FeedEventSigner>,
}

#[cfg(feature = "vnext-network-runtime")]
impl VNextFeedPublisher {
    pub fn new(
        author: ku_core::foundation::ValidatedFeedInception,
        signer: Arc<dyn ku_core::foundation::FeedEventSigner>,
    ) -> Result<Self, String> {
        ku_core::foundation::ProvenFeedEventSigner::prove_for_public_key(
            signer.as_ref(),
            author.signed.inception.feed_public_key,
        )
        .map_err(|error| error.to_string())?;
        Ok(Self { author, signer })
    }
}

#[derive(Clone, Default)]
pub struct VNextRestCoordinator {
    inner: Arc<Mutex<VNextRestState>>,
}

#[derive(Default)]
struct VNextRestState {
    #[cfg(feature = "vnext-network-runtime")]
    prepared_needs: BTreeMap<[u8; 32], PreparedNeedEntry>,
    #[cfg(feature = "vnext-network-runtime")]
    need_operations: BTreeMap<String, [u8; 32]>,
    #[cfg(feature = "vnext-network-runtime")]
    match_cache: BTreeMap<[u8; 32], MatchCache>,
    #[cfg(feature = "vnext-network-runtime")]
    scan_operations: BTreeMap<String, ScanReplay>,
    retired_needs: BTreeMap<[u8; 32], NeedViewV1>,
    #[cfg(feature = "vnext-network-runtime")]
    public_capabilities: BTreeMap<[u8; 32], PublicConsentCapability>,
    #[cfg(feature = "vnext-network-runtime")]
    feed_publisher: Option<VNextFeedPublisher>,
}

#[cfg(feature = "vnext-network-runtime")]
#[derive(Clone)]
struct PreparedNeedEntry {
    fingerprint: [u8; 32],
    activation_idempotency_key: String,
    prepared: PreparedNeedV1,
    bundle: ku_kql::vnext_private_need::PrivateNeedBundle,
}

#[cfg(feature = "vnext-network-runtime")]
#[derive(Default)]
struct MatchCache {
    items: BTreeMap<[u8; 32], QuarantinedMatchV1>,
    scan_continuation: Option<String>,
}

#[cfg(feature = "vnext-network-runtime")]
#[derive(Clone)]
struct ScanReplay {
    need_id: [u8; 32],
    fingerprint: [u8; 32],
    view: NeedViewV1,
    continuation: Option<String>,
}

#[cfg(feature = "vnext-network-runtime")]
enum PublicConsentCapability {
    Prepared {
        request_fingerprint: [u8; 32],
        namespace: String,
        disclosure: DisclosureV1,
        idempotency_key: String,
        intent: Box<onebrain_node::vnext_distributed_pomv::PreparedPublicUseIntent>,
    },
    Confirmed {
        request: onebrain_node::vnext_distributed_pomv::ConfirmPublicUseEvidenceRequest,
    },
}

impl VNextRestCoordinator {
    #[cfg(feature = "vnext-network-runtime")]
    pub fn set_feed_publisher(&self, publisher: VNextFeedPublisher) {
        self.lock().feed_publisher = Some(publisher);
    }

    pub(crate) fn signer_ready(&self) -> bool {
        #[cfg(feature = "vnext-network-runtime")]
        {
            self.lock().feed_publisher.is_some()
        }
        #[cfg(not(feature = "vnext-network-runtime"))]
        {
            false
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, VNextRestState> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn validate_idempotency_key(value: &str) -> Result<(), VNextHttpError> {
    if value.is_empty()
        || value.len() > MAX_IDEMPOTENCY_KEY_BYTES
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(VNextHttpError::invalid(
            "idempotency_key must be 1..128 visible UTF-8 bytes without surrounding whitespace",
        ));
    }
    Ok(())
}

fn hex32(bytes: &[u8; 32]) -> String {
    let mut value = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(value, "{byte:02x}");
    }
    value
}

fn bytes32(field: &'static str, value: &str) -> Result<[u8; 32], VNextHttpError> {
    if value.len() != 64
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(VNextHttpError::invalid(format!(
            "{field} must be exactly 64 lowercase hexadecimal characters"
        )));
    }
    let mut output = [0u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_nibble(pair[0]).expect("validated lowercase hex");
        let low = hex_nibble(pair[1]).expect("validated lowercase hex");
        output[index] = high << 4 | low;
    }
    if output == [0; 32] {
        return Err(VNextHttpError::invalid(format!(
            "{field} must not be the all-zero identifier"
        )));
    }
    Ok(output)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn digest32(domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    for part in parts {
        hasher.update(&(part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    *hasher.finalize().as_bytes()
}

fn digest16(domain: &[u8], parts: &[&[u8]]) -> [u8; 16] {
    let digest = digest32(domain, parts);
    let mut output = [0u8; 16];
    output.copy_from_slice(&digest[..16]);
    output
}

fn page_limit(limit: Option<usize>) -> Result<usize, VNextHttpError> {
    let limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT);
    if limit == 0 || limit > MAX_PAGE_LIMIT {
        return Err(VNextHttpError::invalid(format!(
            "limit must be between 1 and {MAX_PAGE_LIMIT}"
        )));
    }
    Ok(limit)
}

fn page_context(label: &[u8], typed_context: &[u8]) -> [u8; 32] {
    digest32(
        b"onebrain:vnext:rest-page-context:1\0",
        &[label, typed_context],
    )
}

fn encode_page_continuation(kind: u8, offset: usize, context: [u8; 32]) -> String {
    let mut payload = Vec::with_capacity(73);
    payload.push(kind);
    payload.extend_from_slice(&(offset as u64).to_be_bytes());
    payload.extend_from_slice(&context);
    let checksum = digest32(b"onebrain:vnext:rest-page-token:1\0", &[&payload]);
    payload.extend_from_slice(&checksum);
    format!("{CONTINUATION_PREFIX}{}", URL_SAFE_NO_PAD.encode(payload))
}

fn decode_page_continuation(
    token: Option<&str>,
    expected_kind: u8,
    expected_context: [u8; 32],
) -> Result<usize, VNextHttpError> {
    let Some(token) = token else {
        return Ok(0);
    };
    if token.len() > 2_048 || !token.starts_with(CONTINUATION_PREFIX) {
        return Err(VNextHttpError::invalid(
            "continuation must use the opaque obc1 base64url encoding",
        ));
    }
    let payload = URL_SAFE_NO_PAD
        .decode(&token[CONTINUATION_PREFIX.len()..])
        .map_err(|_| VNextHttpError::invalid("continuation is not valid base64url"))?;
    if payload.len() != 73 || payload[0] != expected_kind {
        return Err(VNextHttpError::invalid(
            "continuation is not valid for this endpoint",
        ));
    }
    let mut context = [0u8; 32];
    context.copy_from_slice(&payload[9..41]);
    let expected_checksum = digest32(b"onebrain:vnext:rest-page-token:1\0", &[&payload[..41]]);
    let checksum_matches = payload[41..]
        .iter()
        .zip(expected_checksum)
        .fold(0u8, |difference, (left, right)| {
            difference | (*left ^ right)
        })
        == 0;
    if context != expected_context || !checksum_matches {
        return Err(VNextHttpError::invalid(
            "continuation is not bound to this query context",
        ));
    }
    let mut offset = [0u8; 8];
    offset.copy_from_slice(&payload[1..9]);
    usize::try_from(u64::from_be_bytes(offset))
        .map_err(|_| VNextHttpError::invalid("continuation offset exceeds this platform"))
}

fn encode_core_continuation(bytes: [u8; 32]) -> String {
    format!("{CONTINUATION_PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes))
}

fn validate_core_continuation(value: &str) -> Result<(), VNextHttpError> {
    if value.len() > 2_048 || !value.starts_with(CONTINUATION_PREFIX) {
        return Err(VNextHttpError::invalid(
            "continuation must use the opaque obc1 base64url encoding",
        ));
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(&value[CONTINUATION_PREFIX.len()..])
        .map_err(|_| VNextHttpError::invalid("continuation is not valid base64url"))?;
    if decoded.len() != 32 {
        return Err(VNextHttpError::invalid(
            "continuation has the wrong decoded length",
        ));
    }
    Ok(())
}

fn public_confirmation_receipt(intent_cid: [u8; 32]) -> String {
    let receipt = digest32(
        b"onebrain:vnext:rest-explicit-confirmation:1\0",
        &[&intent_cid],
    );
    encode_core_continuation(receipt)
}

pub async fn get_runtime_status(State(state): State<AppState>) -> VNextResult<RuntimeStatusV1> {
    let (requested, kill_switch, status) = {
        let node = state.node.lock().await;
        let config = &node.config().vnext;
        let requested = config.enabled.obp_rp
            || config.enabled.distributed_kql_one_hop
            || config.enabled.public_use_evidence_publish
            || config.enabled.distributed_pomv_view;
        let kill_switch = config.kill_switches.obp_rp
            || config.kill_switches.distributed_kql_one_hop
            || config.kill_switches.public_use_evidence_publish
            || config.kill_switches.distributed_pomv_view;
        (requested, kill_switch, node.vnext_status())
    };
    #[cfg(feature = "base-v1")]
    let base_status = match state.base_services().await {
        Some(services) => services.snapshot().ok(),
        None => None,
    };
    let compiled = {
        #[cfg(feature = "base-v1")]
        {
            base_status
                .as_ref()
                .map_or(cfg!(feature = "vnext-network-runtime"), |status| {
                    status.network_compiled
                })
        }
        #[cfg(not(feature = "base-v1"))]
        {
            cfg!(feature = "vnext-network-runtime")
        }
    };
    #[cfg(feature = "vnext-network-runtime")]
    let active = {
        #[cfg(feature = "base-v1")]
        if let Some(status) = &base_status {
            status.network_enabled
        } else {
            state.vnext_product_services().await.is_some()
        }
        #[cfg(not(feature = "base-v1"))]
        state.vnext_product_services().await.is_some()
    };
    #[cfg(not(feature = "vnext-network-runtime"))]
    let active = false;
    let coverage = if status.reachability.standalone {
        CoverageV1::LocalOnly
    } else {
        CoverageV1::Partial
    };
    let lifecycle = if !compiled || !requested {
        LifecycleV1::Disabled
    } else if kill_switch {
        LifecycleV1::Degraded
    } else if active {
        LifecycleV1::Active
    } else {
        LifecycleV1::Requested
    };
    let signer_ready = state.vnext_rest.signer_ready();
    let mut limitations = vec!["coverage_is_never_network_global".to_string()];
    if !compiled {
        limitations.push("vnext_network_runtime_not_compiled".into());
    } else if requested && !active {
        limitations.push("requested_runtime_has_no_active_owner".into());
    }
    if kill_switch {
        limitations.push("one_or_more_requested_lanes_are_killed".into());
    }
    if !signer_ready {
        limitations.push("public_use_feed_signer_not_ready".into());
    }
    if status.reachability.standalone {
        limitations.push("local_store_only".into());
    } else {
        limitations.push("observed_authenticated_paths_only".into());
    }
    let data = RuntimeStatusV1 {
        compiled,
        requested,
        active,
        kill_switch,
        signer_ready,
        lifecycle,
        coverage,
        observability: status.network_runtime.observability.clone(),
        limitations: limitations.clone(),
    };
    Ok(success(
        data,
        VNextMetaV1 {
            lifecycle,
            coverage,
            limitations,
            continuation: None,
        },
    ))
}

#[cfg(feature = "vnext-network-runtime")]
async fn services(state: &AppState) -> Result<onebrain_node::VNextProductServices, VNextHttpError> {
    if let Some(services) = state.vnext_product_services().await {
        return Ok(services);
    }
    let requested = {
        let node = state.node.lock().await;
        node.config().vnext.enabled.obp_rp
    };
    Err(VNextHttpError::disabled(
        "vNext product runtime is not active",
        requested,
    ))
}

#[cfg(feature = "vnext-network-runtime")]
fn map_runtime_error(error: onebrain_node::VNextProductRuntimeError) -> VNextHttpError {
    use ku_kql::vnext_private_need::PrivateNeedError;
    use onebrain_node::vnext_distributed_kql::DistributedKqlError;
    use onebrain_node::vnext_distributed_pomv::DistributedPomvError;
    use onebrain_node::VNextProductRuntimeError;

    let message = error.to_string();
    match error {
        VNextProductRuntimeError::LaneDisabled(feature) => {
            VNextHttpError::disabled(format!("vNext lane {} is disabled", feature.name()), true)
        }
        VNextProductRuntimeError::Stopped => {
            VNextHttpError::dependency("vNext product runtime is stopping or stopped")
        }
        VNextProductRuntimeError::BudgetExceeded(_) => VNextHttpError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "request exceeds the configured runtime budget",
            true,
            LifecycleV1::Degraded,
            CoverageV1::Partial,
            ["retry_only_with_a_narrower_budget"],
        ),
        VNextProductRuntimeError::StorageHardWatermark { .. } => VNextHttpError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            message,
            true,
            LifecycleV1::Degraded,
            CoverageV1::Partial,
            ["storage_hard_watermark"],
        ),
        VNextProductRuntimeError::DistributedKql(DistributedKqlError::InvalidBudget)
        | VNextProductRuntimeError::DistributedKql(DistributedKqlError::StandingNeedInactive) => {
            VNextHttpError::invalid(message)
        }
        VNextProductRuntimeError::DistributedKql(DistributedKqlError::DurableMatchConflict) => {
            VNextHttpError::conflict(message)
        }
        VNextProductRuntimeError::DistributedKql(DistributedKqlError::PrivateNeed(
            PrivateNeedError::NotFound,
        )) => VNextHttpError::not_found(message),
        VNextProductRuntimeError::DistributedKql(DistributedKqlError::PrivateNeed(
            PrivateNeedError::Terminal
            | PrivateNeedError::GenerationMismatch { .. }
            | PrivateNeedError::InvalidTransition,
        )) => VNextHttpError::conflict(message),
        VNextProductRuntimeError::DistributedKql(DistributedKqlError::PrivateNeed(
            PrivateNeedError::Limit,
        )) => VNextHttpError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            message,
            true,
            LifecycleV1::Degraded,
            CoverageV1::LocalOnly,
            ["private_need_capacity_reached"],
        ),
        VNextProductRuntimeError::DistributedKql(DistributedKqlError::PrivateNeed(
            PrivateNeedError::InvalidIntent
            | PrivateNeedError::InvalidLifecycle
            | PrivateNeedError::InvalidTarget,
        )) => VNextHttpError::invalid(message),
        VNextProductRuntimeError::DistributedPomv(DistributedPomvError::ConsentExpired) => {
            VNextHttpError::expired(message)
        }
        VNextProductRuntimeError::DistributedPomv(DistributedPomvError::ConsentIntentNotFound) => {
            VNextHttpError::not_found(message)
        }
        VNextProductRuntimeError::DistributedPomv(
            DistributedPomvError::ConsentIntentMismatch
            | DistributedPomvError::ConsentReceiptInvalid
            | DistributedPomvError::ConsentAlreadyConfirmed
            | DistributedPomvError::IdempotencyConflict,
        ) => VNextHttpError::conflict(message),
        VNextProductRuntimeError::DistributedPomv(
            DistributedPomvError::AuthenticatedRouteUnavailable,
        ) => VNextHttpError::dependency(message),
        VNextProductRuntimeError::DistributedPomv(
            DistributedPomvError::ConsentPreparationLimit
            | DistributedPomvError::PublicationLimit
            | DistributedPomvError::ViewLimitExceeded,
        ) => VNextHttpError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            message,
            true,
            LifecycleV1::Degraded,
            CoverageV1::Partial,
            ["bounded_runtime_capacity_reached"],
        ),
        VNextProductRuntimeError::DistributedPomv(
            DistributedPomvError::InvalidPublishRequest
            | DistributedPomvError::ConsentExpiryTooFar
            | DistributedPomvError::ConsentTargetMismatch
            | DistributedPomvError::InvalidLimit
            | DistributedPomvError::PolicyVersionNotAllowed,
        ) => VNextHttpError::invalid(message),
        _ => VNextHttpError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            message,
            false,
            LifecycleV1::Degraded,
            CoverageV1::Partial,
            ["unclassified_runtime_failure_retry_denied"],
        ),
    }
}

#[cfg(feature = "vnext-network-runtime")]
fn need_prepare_fingerprint(request: &NeedPrepareRequestV1) -> [u8; 32] {
    let scope = serde_json::to_vec(&request.scope).unwrap_or_default();
    let budget = serde_json::to_vec(&request.budget).unwrap_or_default();
    digest32(
        b"onebrain:vnext:rest-need-prepare-request:1\0",
        &[
            request.local_query.as_bytes(),
            &scope,
            &budget,
            request.idempotency_key.as_bytes(),
        ],
    )
}

#[cfg(feature = "vnext-network-runtime")]
fn derived_reference(seed: [u8; 32], label: &[u8]) -> ku_core::foundation::ObjectReference {
    ku_core::foundation::ObjectReference::new(
        0,
        digest32(
            b"onebrain:vnext:rest-derived-reference:1\0",
            &[&seed, label],
        ),
    )
}

#[cfg(feature = "vnext-network-runtime")]
fn derived_concept(seed: [u8; 32], label: &[u8]) -> ku_core::foundation::ConceptCcid {
    ku_core::foundation::ConceptCcid::from_bytes(digest16(
        b"onebrain:vnext:rest-derived-concept:1\0",
        &[&seed, label],
    ))
}

#[cfg(feature = "vnext-network-runtime")]
fn build_private_need(
    request: &NeedPrepareRequestV1,
    created_at: u64,
) -> Result<
    (
        [u8; 32],
        PreparedNeedV1,
        ku_kql::vnext_private_need::PrivateNeedBundle,
    ),
    VNextHttpError,
> {
    use ku_core::foundation::{
        DisclosureClass, EventCid, ObjectReference, ReceptorAcceptanceProfile, ReceptorCardinality,
        ReceptorDefinition, ReceptorOrigin, ResourceProfile, SemanticFrameSet, StatementLocator,
        UnknownConstraintPolicy, RECEPTOR_DEFINITION_KIND,
    };
    use ku_kql::vnext_matcher::MatcherMetricConcepts;
    use ku_kql::vnext_private_need::{adapt_local_intent, LocalIntentSource, LocalIntentTemplate};

    if request.local_query.is_empty()
        || request.local_query.len() > MAX_LOCAL_QUERY_BYTES
        || request.local_query.trim() != request.local_query
    {
        return Err(VNextHttpError::invalid(
            "local_query must be non-empty, trimmed, and at most 65536 bytes",
        ));
    }
    request.scope.validate_need()?;
    request.budget.validate()?;
    validate_idempotency_key(&request.idempotency_key)?;

    let fingerprint = need_prepare_fingerprint(request);
    let selector = ku_core::foundation::SelectorCid::from_bytes(digest32(
        b"onebrain:vnext:rest-private-need-selector:1\0",
        &[&fingerprint],
    ));
    let role = derived_concept(fingerprint, b"receptor-role");
    let acceptance_policy = derived_reference(fingerprint, b"receptor-acceptance-policy");
    let receptor = ReceptorDefinition {
        role,
        expected_types: vec![derived_concept(fingerprint, b"expected-type")],
        hard_constraints: Vec::new(),
        cardinality: ReceptorCardinality::new(1, Some(1))
            .map_err(|error| VNextHttpError::invalid(format!("invalid receptor: {error:?}")))?,
        origin: ReceptorOrigin::Declared {
            source: StatementLocator {
                object: derived_reference(fingerprint, b"local-query-source"),
                statement_index: 0,
            },
        },
        acceptance: ReceptorAcceptanceProfile {
            policy: acceptance_policy,
            required_evidence_kinds: Vec::new(),
            unknown_constraint_policy: UnknownConstraintPolicy::KeepUnresolved,
        },
    };
    let (_, receptor_cid) = receptor
        .to_knowledge_object(DisclosureClass::LocalOnly)
        .map_err(|error| VNextHttpError::invalid(format!("invalid receptor: {error:?}")))?
        .encode(ResourceProfile::ObjectV1)
        .map_err(|error| {
            VNextHttpError::invalid(format!("invalid receptor encoding: {error:?}"))
        })?;
    let observed_frontier = digest32(
        b"onebrain:vnext:rest-private-need-frontier:1\0",
        &[&fingerprint],
    );
    let bundle = adapt_local_intent(
        LocalIntentSource::RawKql(&request.local_query),
        LocalIntentTemplate {
            receptor_definition: ObjectReference::new(
                RECEPTOR_DEFINITION_KIND.0,
                receptor_cid.into_bytes(),
            ),
            receptor,
            desired_roles: vec![role],
            goal: SemanticFrameSet {
                statements: Vec::new(),
            },
            local_context: SemanticFrameSet {
                statements: Vec::new(),
            },
            intent_commitment_predicate: derived_concept(fingerprint, b"local-intent-commitment"),
            query_policy: derived_reference(fingerprint, b"query-policy"),
            exploration_policy: derived_reference(fingerprint, b"exploration-policy"),
            selector,
            watch_policy: derived_reference(fingerprint, b"watch-policy"),
            observed_frontier,
            generator: derived_reference(fingerprint, b"rest-adapter"),
            derivation_rule: None,
            evidence: Vec::new(),
            index_commitment: None,
            rule_commitment: None,
            metrics: MatcherMetricConcepts {
                structural_fit: derived_concept(fingerprint, b"structural-fit"),
                constraint_fit: derived_concept(fingerprint, b"constraint-fit"),
            },
            unmapped_reason: derived_concept(fingerprint, b"unmapped"),
            source_frontier: EventCid::from_bytes(observed_frontier),
            created_at_evaluation: created_at,
            expires_after_evaluations: request.budget.max_scan_records,
        },
    )
    .map_err(|error| VNextHttpError::invalid(format!("local KQL intent is invalid: {error:?}")))?;
    let query_definition_cid = bundle
        .query_definition
        .private_cid()
        .map_err(|error| VNextHttpError::invalid(format!("private query is invalid: {error:?}")))?;
    let expires_at = created_at.saturating_add(NEED_PREPARE_TTL_SECONDS);
    let canonical = bundle
        .canonical_bytes()
        .map_err(|error| VNextHttpError::invalid(format!("private Need is invalid: {error:?}")))?;
    let budget = serde_json::to_vec(&request.budget).unwrap_or_default();
    let intent_cid = digest32(
        b"onebrain:vnext:rest-prepared-private-need:1\0",
        &[
            &canonical,
            &budget,
            request.idempotency_key.as_bytes(),
            &expires_at.to_be_bytes(),
        ],
    );
    let prepared = PreparedNeedV1 {
        intent_cid: hex32(&intent_cid),
        query_definition_cid: hex32(query_definition_cid.as_bytes()),
        selector_cid: hex32(selector.as_bytes()),
        scope: request.scope.clone(),
        budget: request.budget,
        expires_at,
        limitations: vec![
            "raw_query_remains_local_and_is_not_retained".into(),
            "prepared_intent_is_not_an_active_need".into(),
            "one_hop_results_are_path_limited_and_never_network_global".into(),
        ],
    };
    Ok((intent_cid, prepared, bundle))
}

#[cfg(feature = "vnext-network-runtime")]
fn need_view(
    id: ku_kql::vnext_standing_need::StandingNeedId,
    need: &ku_kql::vnext_standing_need::StandingNeed,
) -> NeedViewV1 {
    use ku_kql::vnext_standing_need::StandingNeedState;
    NeedViewV1 {
        standing_need_id: hex32(id.as_bytes()),
        state: match need.state {
            StandingNeedState::Active => "active",
            StandingNeedState::Paused => "paused",
            StandingNeedState::Retired => "retired",
        }
        .into(),
        query_definition_cid: hex32(need.query_definition.as_bytes()),
        selector_cid: hex32(need.selector.as_bytes()),
        coverage: CoverageV1::Partial,
        limitations: vec![
            "standing_need_and_query_definition_are_authenticated_local_private".into(),
            "coverage_is_path_limited_and_never_network_global".into(),
        ],
        revision: need.generation,
    }
}

pub async fn prepare_need(
    State(state): State<AppState>,
    body: Result<Json<NeedPrepareRequestV1>, JsonRejection>,
) -> VNextResult<PreparedNeedV1> {
    let request = parse_json(body)?;
    #[cfg(not(feature = "vnext-network-runtime"))]
    {
        let _ = (state, request);
        Err(VNextHttpError::disabled(
            "distributed KQL runtime is not compiled",
            false,
        ))
    }
    #[cfg(feature = "vnext-network-runtime")]
    {
        let runtime = services(&state).await?;
        // A read through the typed façade proves the KQL lane actually owns
        // its encrypted store before a short-lived API capability is created.
        runtime.standing_needs().map_err(map_runtime_error)?;
        let fingerprint = need_prepare_fingerprint(&request);
        {
            let mut coordinator = state.vnext_rest.lock();
            let now = now_epoch();
            coordinator
                .prepared_needs
                .retain(|_, entry| entry.prepared.expires_at > now);
            if let Some(intent) = coordinator
                .need_operations
                .get(&request.idempotency_key)
                .copied()
            {
                if let Some(existing) = coordinator.prepared_needs.get(&intent) {
                    if existing.fingerprint != fingerprint {
                        return Err(VNextHttpError::conflict(
                            "idempotency_key is already bound to a different prepared Need",
                        ));
                    }
                    let data = existing.prepared.clone();
                    return Ok(success(
                        data,
                        active_meta(
                            CoverageV1::LocalOnly,
                            [
                                "exact_prepare_replay",
                                "raw_query_not_exported",
                                "explicit_activation_required",
                            ],
                            None,
                        ),
                    ));
                }
                coordinator.need_operations.remove(&request.idempotency_key);
            }
            if coordinator.prepared_needs.len() >= MAX_PREPARED_API_CAPABILITIES {
                return Err(VNextHttpError::new(
                    StatusCode::TOO_MANY_REQUESTS,
                    "rate_limited",
                    "prepared Need capability limit reached",
                    true,
                    LifecycleV1::Degraded,
                    CoverageV1::LocalOnly,
                    ["wait_for_existing_preparations_to_expire"],
                ));
            }
        }
        let (intent_cid, prepared, bundle) = build_private_need(&request, now_epoch())?;
        let mut coordinator = state.vnext_rest.lock();
        if let Some(existing_intent) = coordinator
            .need_operations
            .get(&request.idempotency_key)
            .copied()
        {
            let existing = coordinator
                .prepared_needs
                .get(&existing_intent)
                .ok_or_else(|| VNextHttpError::conflict("prepared Need operation changed"))?;
            if existing.fingerprint != fingerprint {
                return Err(VNextHttpError::conflict(
                    "idempotency_key is already bound to a different prepared Need",
                ));
            }
            return Ok(success(
                existing.prepared.clone(),
                active_meta(
                    CoverageV1::LocalOnly,
                    ["exact_prepare_replay", "explicit_activation_required"],
                    None,
                ),
            ));
        }
        coordinator
            .need_operations
            .insert(request.idempotency_key.clone(), intent_cid);
        coordinator.prepared_needs.insert(
            intent_cid,
            PreparedNeedEntry {
                fingerprint,
                activation_idempotency_key: request.idempotency_key,
                prepared: prepared.clone(),
                bundle,
            },
        );
        Ok(success(
            prepared,
            active_meta(
                CoverageV1::LocalOnly,
                [
                    "raw_query_not_exported",
                    "explicit_activation_required",
                    "prepared_capability_expires",
                ],
                None,
            ),
        ))
    }
}

pub async fn activate_need(
    State(state): State<AppState>,
    body: Result<Json<NeedActivationRequestV1>, JsonRejection>,
) -> VNextResult<NeedViewV1> {
    let request = parse_json(body)?;
    #[cfg(not(feature = "vnext-network-runtime"))]
    {
        let _ = (state, request);
        Err(VNextHttpError::disabled(
            "distributed KQL runtime is not compiled",
            false,
        ))
    }
    #[cfg(feature = "vnext-network-runtime")]
    {
        validate_idempotency_key(&request.idempotency_key)?;
        let intent_cid = bytes32("intent_cid", &request.intent_cid)?;
        let entry = {
            let coordinator = state.vnext_rest.lock();
            coordinator
                .prepared_needs
                .get(&intent_cid)
                .cloned()
                .ok_or_else(|| {
                    VNextHttpError::not_found(
                        "prepared Need intent is not present in this local authenticated session",
                    )
                })?
        };
        if entry.prepared.expires_at <= now_epoch() {
            state.vnext_rest.lock().prepared_needs.remove(&intent_cid);
            return Err(VNextHttpError::expired("prepared Need intent has expired"));
        }
        if entry.activation_idempotency_key != request.idempotency_key {
            return Err(VNextHttpError::conflict(
                "activation idempotency_key does not match the exact prepared Need",
            ));
        }
        let runtime = services(&state).await?;
        let (id, outcome) = runtime
            .register_private_need(entry.bundle)
            .map_err(map_runtime_error)?;
        use ku_kql::vnext_standing_need::StandingNeedWriteOutcome;
        if matches!(
            outcome,
            StandingNeedWriteOutcome::StaleGeneration
                | StandingNeedWriteOutcome::GenerationConflict
        ) {
            return Err(VNextHttpError::conflict(
                "prepared Need conflicts with the durable local revision",
            ));
        }
        let need = runtime
            .standing_need(id)
            .map_err(map_runtime_error)?
            .ok_or_else(|| VNextHttpError::not_found("activated StandingNeed was not found"))?;
        let data = need_view(id, &need);
        Ok(success(
            data,
            active_meta(
                CoverageV1::Partial,
                [
                    if outcome == StandingNeedWriteOutcome::ExactReplay {
                        "exact_activation_replay"
                    } else {
                        "standing_need_activated"
                    },
                    "private_identifiers_are_not_exportable",
                    "coverage_is_path_limited",
                ],
                None,
            ),
        ))
    }
}

pub async fn list_needs(
    State(state): State<AppState>,
    query: Result<Query<PageQueryV1>, QueryRejection>,
) -> VNextResult<NeedPageV1> {
    let query = parse_query(query)?;
    #[cfg(not(feature = "vnext-network-runtime"))]
    {
        let _ = (state, query);
        Err(VNextHttpError::disabled(
            "distributed KQL runtime is not compiled",
            false,
        ))
    }
    #[cfg(feature = "vnext-network-runtime")]
    {
        let limit = page_limit(query.limit)?;
        let context = page_context(b"needs", b"authenticated-local-private");
        let offset = decode_page_continuation(query.continuation.as_deref(), 1, context)?;
        let runtime = services(&state).await?;
        let needs = runtime.standing_needs().map_err(map_runtime_error)?;
        if offset > needs.len() {
            return Err(VNextHttpError::invalid(
                "continuation offset no longer exists in this local inventory",
            ));
        }
        let end = offset.saturating_add(limit).min(needs.len());
        let items = needs[offset..end]
            .iter()
            .map(|(id, need)| need_view(*id, need))
            .collect::<Vec<_>>();
        let continuation = (end < needs.len()).then(|| encode_page_continuation(1, end, context));
        let limitations = vec![
            "local_private_inventory_only".to_string(),
            "zero_results_do_not_claim_network_absence".to_string(),
        ];
        Ok(success(
            NeedPageV1 {
                items,
                coverage: CoverageV1::LocalOnly,
                limitations: limitations.clone(),
                continuation: continuation.clone(),
            },
            active_meta(CoverageV1::LocalOnly, limitations, continuation),
        ))
    }
}

pub async fn get_need(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> VNextResult<NeedViewV1> {
    #[cfg(not(feature = "vnext-network-runtime"))]
    {
        let _ = (state, id);
        Err(VNextHttpError::disabled(
            "distributed KQL runtime is not compiled",
            false,
        ))
    }
    #[cfg(feature = "vnext-network-runtime")]
    {
        let id_bytes = bytes32("standing_need_id", &id)?;
        let id = ku_kql::vnext_standing_need::StandingNeedId::from_bytes(id_bytes);
        let runtime = services(&state).await?;
        let need = runtime
            .standing_need(id)
            .map_err(map_runtime_error)?
            .ok_or_else(|| VNextHttpError::not_found("StandingNeed was not found locally"))?;
        Ok(success(
            need_view(id, &need),
            active_meta(
                CoverageV1::LocalOnly,
                [
                    "authenticated_local_private",
                    "private_identifiers_are_not_exportable",
                ],
                None,
            ),
        ))
    }
}

pub async fn retire_need(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> VNextResult<NeedViewV1> {
    #[cfg(not(feature = "vnext-network-runtime"))]
    {
        let _ = (state, id);
        Err(VNextHttpError::disabled(
            "distributed KQL runtime is not compiled",
            false,
        ))
    }
    #[cfg(feature = "vnext-network-runtime")]
    {
        let id_bytes = bytes32("standing_need_id", &id)?;
        if let Some(replay) = state
            .vnext_rest
            .lock()
            .retired_needs
            .get(&id_bytes)
            .cloned()
        {
            return Ok(success(
                replay,
                active_meta(
                    CoverageV1::LocalOnly,
                    ["exact_retire_replay", "retired_need_is_not_scanned"],
                    None,
                ),
            ));
        }
        let typed_id = ku_kql::vnext_standing_need::StandingNeedId::from_bytes(id_bytes);
        let runtime = services(&state).await?;
        let need = runtime
            .standing_need(typed_id)
            .map_err(map_runtime_error)?
            .ok_or_else(|| VNextHttpError::not_found("StandingNeed was not found locally"))?;
        let revision = runtime
            .retire_private_need(typed_id, need.generation)
            .map_err(map_runtime_error)?;
        let mut retired = need_view(typed_id, &need);
        retired.state = "retired".into();
        retired.revision = revision;
        retired
            .limitations
            .push("terminal_tombstone_retains_no_private_bundle".into());
        state
            .vnext_rest
            .lock()
            .retired_needs
            .insert(id_bytes, retired.clone());
        Ok(success(
            retired,
            active_meta(
                CoverageV1::LocalOnly,
                ["standing_need_retired", "terminal_state_is_fail_closed"],
                None,
            ),
        ))
    }
}

#[cfg(feature = "vnext-network-runtime")]
fn scan_fingerprint(
    need_id: [u8; 32],
    request: &NeedScanRequestV1,
) -> Result<[u8; 32], VNextHttpError> {
    if let Some(continuation) = request.continuation.as_deref() {
        validate_core_continuation(continuation)?;
    }
    let budget = serde_json::to_vec(&request.budget).unwrap_or_default();
    Ok(digest32(
        b"onebrain:vnext:rest-need-scan-request:1\0",
        &[
            &need_id,
            &budget,
            request.continuation.as_deref().unwrap_or("").as_bytes(),
            request.idempotency_key.as_bytes(),
        ],
    ))
}

#[cfg(feature = "vnext-network-runtime")]
fn constraint_evaluation(value: ku_core::foundation::ConstraintEvaluation) -> &'static str {
    use ku_core::foundation::ConstraintEvaluation;
    match value {
        ConstraintEvaluation::Satisfied => "satisfied",
        ConstraintEvaluation::Violated => "violated",
        ConstraintEvaluation::Unknown => "unknown",
    }
}

#[cfg(feature = "vnext-network-runtime")]
fn quarantined_match(
    runtime: &onebrain_node::VNextProductServices,
    matched: &onebrain_node::vnext_distributed_kql::DistributedKqlMatch,
) -> Result<QuarantinedMatchV1, VNextHttpError> {
    use ku_core::foundation::ConstraintEvaluation;
    let proposal = runtime
        .proposal(matched.proposal)
        .map_err(map_runtime_error)?
        .ok_or_else(|| {
            VNextHttpError::dependency(
                "quarantined proposal is unavailable in the current runtime session",
            )
        })?;
    let observations = proposal
        .constraints
        .iter()
        .map(|constraint| ConstraintObservationV1 {
            constraint_index: constraint.constraint_index,
            evaluation: constraint_evaluation(constraint.evaluation).into(),
            required: constraint.required,
        })
        .collect::<Vec<_>>();
    let all_required_satisfied = proposal.constraints.iter().all(|constraint| {
        !constraint.required || constraint.evaluation == ConstraintEvaluation::Satisfied
    });
    Ok(QuarantinedMatchV1 {
        proposal_cid: hex32(matched.proposal.as_bytes()),
        candidate_cid: hex32(&matched.affordance.cid),
        responder_scope: ScopeV1 {
            kind: ScopeKindV1::AuthenticatedDirectPeers,
            max_hops: 1,
            node_ids: matched
                .responder_scope
                .iter()
                .map(|node| hex32(node.as_bytes()))
                .collect(),
        },
        selector_cid: hex32(matched.selector.as_bytes()),
        assessed_frontier: hex32(matched.assessed_frontier.as_bytes()),
        constraints: ConstraintSetV1 {
            observations,
            all_required_satisfied,
        },
        limitations: vec![
            "quarantined_proposal_only".into(),
            "does_not_materialize_or_adopt_a_mapping".into(),
            "responder_scope_is_observed_authenticated_paths_only".into(),
        ],
        state: "quarantined",
        executable: false,
    })
}

pub async fn scan_need(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Result<Json<NeedScanRequestV1>, JsonRejection>,
) -> VNextResult<NeedViewV1> {
    let request = parse_json(body)?;
    #[cfg(not(feature = "vnext-network-runtime"))]
    {
        let _ = (state, id, headers, request);
        Err(VNextHttpError::disabled(
            "distributed KQL runtime is not compiled",
            false,
        ))
    }
    #[cfg(feature = "vnext-network-runtime")]
    {
        validate_idempotency_key(&request.idempotency_key)?;
        request.budget.validate()?;
        let id_bytes = bytes32("standing_need_id", &id)?;
        let fingerprint = scan_fingerprint(id_bytes, &request)?;
        if let Some(replay) = state
            .vnext_rest
            .lock()
            .scan_operations
            .get(&request.idempotency_key)
            .cloned()
        {
            if replay.need_id != id_bytes || replay.fingerprint != fingerprint {
                return Err(VNextHttpError::conflict(
                    "scan idempotency_key is already bound to a different Need or continuation",
                ));
            }
            return Ok(success(
                replay.view,
                active_meta(
                    CoverageV1::Partial,
                    [
                        "exact_scan_replay",
                        "zero_matches_do_not_claim_network_absence",
                    ],
                    replay.continuation,
                ),
            ));
        }
        {
            let coordinator = state.vnext_rest.lock();
            let expected = coordinator
                .match_cache
                .get(&id_bytes)
                .and_then(|cache| cache.scan_continuation.as_deref());
            if request.continuation.as_deref() != expected {
                return Err(VNextHttpError::conflict(
                    "scan continuation does not match the latest context-bound cursor",
                ));
            }
            if coordinator.scan_operations.len() >= MAX_PREPARED_API_CAPABILITIES
                || (!coordinator.match_cache.contains_key(&id_bytes)
                    && coordinator.match_cache.len() >= MAX_PREPARED_API_CAPABILITIES)
            {
                return Err(VNextHttpError::new(
                    StatusCode::TOO_MANY_REQUESTS,
                    "rate_limited",
                    "local REST scan projection capacity reached",
                    true,
                    LifecycleV1::Degraded,
                    CoverageV1::Partial,
                    ["restart_or_retire_unused_local_scan_sessions"],
                ));
            }
        }
        let typed_id = ku_kql::vnext_standing_need::StandingNeedId::from_bytes(id_bytes);
        let runtime = services(&state).await?;
        let need = runtime
            .standing_need(typed_id)
            .map_err(map_runtime_error)?
            .ok_or_else(|| VNextHttpError::not_found("StandingNeed was not found locally"))?;
        if need.state != ku_kql::vnext_standing_need::StandingNeedState::Active {
            return Err(VNextHttpError::conflict(
                "only an active StandingNeed can scan one-hop affordances",
            ));
        }
        let report = runtime
            .process_one_hop_affordance_delta(
                need.selector,
                onebrain_node::vnext_distributed_kql::DistributedKqlBudget {
                    max_scan_records: request.budget.max_scan_records,
                    max_affordances: request.budget.max_affordances,
                    max_pairs: request.budget.max_pairs,
                    max_proposals: request.budget.max_proposals,
                },
            )
            .map_err(map_runtime_error)?;
        if report.claims_automatic_materialization
            || report.claims_automatic_adoption
            || report.claims_network_completion
        {
            return Err(VNextHttpError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "distributed KQL report violated the product semantic firewall",
                false,
                LifecycleV1::Degraded,
                CoverageV1::Partial,
                ["proposal_firewall_violation"],
            ));
        }
        let projected = report
            .matches
            .iter()
            .map(|matched| quarantined_match(&runtime, matched))
            .collect::<Result<Vec<_>, _>>()?;
        let scan_continuation = report.coverage.continuation.map(encode_core_continuation);
        let view = need_view(typed_id, &need);
        let replay = ScanReplay {
            need_id: id_bytes,
            fingerprint,
            view: view.clone(),
            continuation: scan_continuation.clone(),
        };
        let new_match_count = {
            let mut coordinator = state.vnext_rest.lock();
            let cache = coordinator.match_cache.entry(id_bytes).or_default();
            let mut new_match_count = 0usize;
            for item in projected {
                let proposal = bytes32("proposal_cid", &item.proposal_cid)
                    .expect("runtime produced a valid proposal CID");
                if cache.items.insert(proposal, item).is_none() {
                    new_match_count = new_match_count.saturating_add(1);
                }
            }
            cache.scan_continuation = scan_continuation.clone();
            coordinator
                .scan_operations
                .insert(request.idempotency_key, replay);
            new_match_count
        };
        state
            .vnext_ws
            .publish_bounded_match(&headers, new_match_count);
        Ok(success(
            view,
            active_meta(
                CoverageV1::Partial,
                [
                    "one_hop_authenticated_paths_only",
                    "matches_remain_quarantined_and_non_executable",
                    "zero_matches_do_not_claim_network_absence",
                ],
                scan_continuation,
            ),
        ))
    }
}

pub async fn list_need_matches(
    State(state): State<AppState>,
    Path(id): Path<String>,
    query: Result<Query<PageQueryV1>, QueryRejection>,
) -> VNextResult<MatchPageV1> {
    let query = parse_query(query)?;
    #[cfg(not(feature = "vnext-network-runtime"))]
    {
        let _ = (state, id, query);
        Err(VNextHttpError::disabled(
            "distributed KQL runtime is not compiled",
            false,
        ))
    }
    #[cfg(feature = "vnext-network-runtime")]
    {
        let id_bytes = bytes32("standing_need_id", &id)?;
        let typed_id = ku_kql::vnext_standing_need::StandingNeedId::from_bytes(id_bytes);
        let runtime = services(&state).await?;
        runtime
            .standing_need(typed_id)
            .map_err(map_runtime_error)?
            .ok_or_else(|| VNextHttpError::not_found("StandingNeed was not found locally"))?;
        let limit = page_limit(query.limit)?;
        let context = page_context(b"matches", &id_bytes);
        let offset = decode_page_continuation(query.continuation.as_deref(), 2, context)?;
        let coordinator = state.vnext_rest.lock();
        let items = coordinator
            .match_cache
            .get(&id_bytes)
            .map(|cache| cache.items.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        if offset > items.len() {
            return Err(VNextHttpError::invalid(
                "continuation offset no longer exists in this match projection",
            ));
        }
        let end = offset.saturating_add(limit).min(items.len());
        let page_items = items[offset..end].to_vec();
        let continuation = (end < items.len()).then(|| encode_page_continuation(2, end, context));
        let limitations = vec![
            "one_hop_authenticated_paths_only".to_string(),
            "runtime_session_quarantine_projection".to_string(),
            "zero_results_do_not_claim_network_absence".to_string(),
            "proposals_are_non_executable".to_string(),
        ];
        Ok(success(
            MatchPageV1 {
                items: page_items,
                coverage: CoverageV1::Partial,
                limitations: limitations.clone(),
                continuation: continuation.clone(),
            },
            active_meta(CoverageV1::Partial, limitations, continuation),
        ))
    }
}

#[cfg(feature = "vnext-network-runtime")]
fn public_use_fingerprint(request: &PublicUsePrepareRequestV1) -> [u8; 32] {
    let disclosure = serde_json::to_vec(&request.disclosure).unwrap_or_default();
    digest32(
        b"onebrain:vnext:rest-public-use-prepare-request:1\0",
        &[
            request.target_cid.as_bytes(),
            request.recipient_node_id.as_bytes(),
            request.selector_cid.as_bytes(),
            request.namespace.as_bytes(),
            &disclosure,
            request.idempotency_key.as_bytes(),
            &request.expires_at.to_be_bytes(),
        ],
    )
}

#[cfg(feature = "vnext-network-runtime")]
fn core_use_mode(mode: UseModeV1) -> ku_core::foundation::UseMode {
    match mode {
        UseModeV1::Application => ku_core::foundation::UseMode::Application,
        UseModeV1::Transformation => ku_core::foundation::UseMode::Transformation,
        UseModeV1::Epistemic => ku_core::foundation::UseMode::Epistemic,
        UseModeV1::Transfer => ku_core::foundation::UseMode::Transfer,
        UseModeV1::Discovery => ku_core::foundation::UseMode::Discovery,
        UseModeV1::ReceptorDiscovered => ku_core::foundation::UseMode::ReceptorDiscovered,
        UseModeV1::CandidateEvaluated => ku_core::foundation::UseMode::CandidateEvaluated,
        UseModeV1::ConstraintClarified => ku_core::foundation::UseMode::ConstraintClarified,
        UseModeV1::GapPartiallyFilled => ku_core::foundation::UseMode::GapPartiallyFilled,
        UseModeV1::AssemblyUsed => ku_core::foundation::UseMode::AssemblyUsed,
        UseModeV1::AnalogicalTransfer => ku_core::foundation::UseMode::AnalogicalTransfer,
        UseModeV1::ComparedOrOpposed => ku_core::foundation::UseMode::ComparedOrOpposed,
        UseModeV1::CapabilityResultUsed => ku_core::foundation::UseMode::CapabilityResultUsed,
    }
}

#[cfg(feature = "vnext-network-runtime")]
fn publication_view(
    record: &onebrain_node::vnext_distributed_pomv::PublicUsePublicationRecord,
) -> PublicationViewV1 {
    PublicationViewV1 {
        publication_cid: hex32(&record.publication.publication_id),
        intent_cid: hex32(record.publication.intent_cid.as_bytes()),
        state: if record.exported_to_network_outbox {
            "pending"
        } else {
            "deferred"
        }
        .into(),
        attempts: 1,
        limitations: if record.exported_to_network_outbox {
            vec![
                "queued_to_authenticated_network_outbox".into(),
                "delivery_acknowledgement_not_yet_projected".into(),
                "publication_does_not_establish_truth_benefit_or_reward".into(),
            ]
        } else {
            vec![
                "awaiting_authenticated_route_or_outbox_handoff".into(),
                "retry_reuses_the_same_publication_identity".into(),
                "publication_does_not_establish_truth_benefit_or_reward".into(),
            ]
        },
        revision: record.publication.author_sequence.saturating_add(1),
    }
}

pub async fn prepare_public_use(
    State(state): State<AppState>,
    body: Result<Json<PublicUsePrepareRequestV1>, JsonRejection>,
) -> VNextResult<PreparedPublicUseV1> {
    let request = parse_json(body)?;
    #[cfg(not(feature = "vnext-network-runtime"))]
    {
        let _ = (state, request);
        Err(VNextHttpError::disabled(
            "Public UseEvidence publication runtime is not compiled",
            false,
        ))
    }
    #[cfg(feature = "vnext-network-runtime")]
    {
        use ku_core::foundation::{
            ConceptCcid, DisclosureClass, NamespaceCommitment, NodeId, ObjectReference,
            SelectorCid, UseEvidencePayload,
        };
        use onebrain_node::vnext_distributed_pomv::PreparePublicUseEvidenceRequest;

        request.disclosure.validate()?;
        validate_idempotency_key(&request.idempotency_key)?;
        if request.namespace.is_empty()
            || request.namespace.len() > 256
            || request.namespace.chars().any(char::is_control)
        {
            return Err(VNextHttpError::invalid(
                "namespace must be 1..256 visible UTF-8 bytes",
            ));
        }
        let target = bytes32("target_cid", &request.target_cid)?;
        let recipient = bytes32("recipient_node_id", &request.recipient_node_id)?;
        let selector = bytes32("selector_cid", &request.selector_cid)?;
        let fingerprint = public_use_fingerprint(&request);
        let publisher = state
            .vnext_rest
            .lock()
            .feed_publisher
            .clone()
            .ok_or_else(|| {
                VNextHttpError::dependency(
                    "a proof-checked caller-owned Feed author/signer is required",
                )
            })?;
        let idempotency_key = digest32(
            b"onebrain:vnext:rest-idempotency-key:1\0",
            &[request.idempotency_key.as_bytes()],
        );
        let namespace = NamespaceCommitment::derive(
            request.namespace.as_bytes(),
            digest32(
                b"onebrain:vnext:rest-namespace-opening:1\0",
                &[request.idempotency_key.as_bytes(), &selector],
            ),
        )
        .map_err(|error| VNextHttpError::invalid(format!("invalid namespace: {error:?}")))?;
        let exact_target = ObjectReference::new(0, target);
        let payload = UseEvidencePayload {
            subjects: vec![exact_target.clone()],
            mode: core_use_mode(request.disclosure.use_mode.clone()),
            actor_class: ConceptCcid::from_bytes(digest16(
                b"onebrain:vnext:rest-public-use-actor-class:1\0",
                &[publisher.author.feed_id.as_bytes()],
            )),
            task_context_commitment: digest32(
                b"onebrain:vnext:rest-public-use-task:1\0",
                &[&target, &idempotency_key],
            ),
            causal_role: ConceptCcid::from_bytes(digest16(
                b"onebrain:vnext:rest-public-use-causal-role:1\0",
                &[&target, &selector],
            )),
            assembly: None,
            mapping: None,
            outcome_observation: None,
            use_policy: ObjectReference::new(
                0,
                digest32(
                    b"onebrain:vnext:rest-public-use-policy:1\0",
                    &[namespace.as_bytes(), &selector],
                ),
            ),
            observed_frontier: digest32(
                b"onebrain:vnext:rest-public-use-frontier:1\0",
                &[&target, &selector],
            ),
        };
        let runtime = services(&state).await?;
        let prepared = runtime
            .prepare_public_use(
                &PreparePublicUseEvidenceRequest {
                    payload,
                    exact_target,
                    expected_peer: NodeId::from_bytes(recipient),
                    selector: SelectorCid::from_bytes(selector),
                    namespace,
                    disclosure: DisclosureClass::Public,
                    idempotency_key,
                    expires_at: request.expires_at,
                },
                &publisher.author,
            )
            .map_err(map_runtime_error)?;
        let intent_cid = prepared.intent_cid.into_bytes();
        let data = PreparedPublicUseV1 {
            intent_cid: hex32(&intent_cid),
            canonical_payload_preview: {
                let mut value = String::with_capacity(prepared.canonical_payload_preview.len() * 2);
                for byte in &prepared.canonical_payload_preview {
                    use std::fmt::Write;
                    let _ = write!(value, "{byte:02x}");
                }
                value
            },
            exact_target: hex32(&prepared.exact_target.cid),
            exact_recipient: hex32(prepared.exact_recipient.as_bytes()),
            selector_cid: hex32(prepared.selector.as_bytes()),
            namespace: request.namespace.clone(),
            disclosure: request.disclosure.clone(),
            idempotency_key: request.idempotency_key.clone(),
            expires_at: prepared.expires_at,
        };
        let mut coordinator = state.vnext_rest.lock();
        if coordinator.public_capabilities.len() >= MAX_PREPARED_API_CAPABILITIES
            && !coordinator.public_capabilities.contains_key(&intent_cid)
        {
            return Err(VNextHttpError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "prepared Public Use capability limit reached",
                true,
                LifecycleV1::Degraded,
                CoverageV1::LocalOnly,
                ["confirm_or_allow_existing_preparations_to_expire"],
            ));
        }
        if let Some(PublicConsentCapability::Prepared {
            request_fingerprint,
            ..
        }) = coordinator.public_capabilities.get(&intent_cid)
        {
            if *request_fingerprint != fingerprint {
                return Err(VNextHttpError::conflict(
                    "prepared Public Use identity is bound to different review fields",
                ));
            }
        }
        // Exact re-prepare intentionally replaces the in-memory typed
        // capability because the core rotates its private receipt commitment.
        coordinator.public_capabilities.insert(
            intent_cid,
            PublicConsentCapability::Prepared {
                request_fingerprint: fingerprint,
                namespace: request.namespace,
                disclosure: request.disclosure,
                idempotency_key: request.idempotency_key,
                intent: Box::new(prepared),
            },
        );
        Ok(success(
            data,
            active_meta(
                CoverageV1::LocalOnly,
                [
                    "exact_payload_recipient_and_public_permanence_require_review",
                    "preparation_does_not_create_use_evidence",
                    "core_confirmation_capability_never_leaves_the_process",
                    "explicit_confirmation_required",
                ],
                None,
            ),
        ))
    }
}

pub async fn confirm_public_use(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<PublicUseConfirmRequestV1>, JsonRejection>,
) -> VNextResult<PublicationViewV1> {
    let request = parse_json(body)?;
    #[cfg(not(feature = "vnext-network-runtime"))]
    {
        let _ = (state, headers, request);
        Err(VNextHttpError::disabled(
            "Public UseEvidence publication runtime is not compiled",
            false,
        ))
    }
    #[cfg(feature = "vnext-network-runtime")]
    {
        let intent_cid = bytes32("intent_cid", &request.intent_cid)?;
        let expected_receipt = public_confirmation_receipt(intent_cid);
        if request.single_use_receipt != expected_receipt {
            return Err(VNextHttpError::conflict(
                "single_use_receipt is not bound to the exact prepared intent",
            ));
        }
        let (publisher, capability) = {
            let mut coordinator = state.vnext_rest.lock();
            let publisher = coordinator.feed_publisher.clone().ok_or_else(|| {
                VNextHttpError::dependency(
                    "a proof-checked caller-owned Feed author/signer is required",
                )
            })?;
            let capability = coordinator
                .public_capabilities
                .remove(&intent_cid)
                .ok_or_else(|| {
                    VNextHttpError::not_found(
                        "prepared Public Use capability is not present in this process",
                    )
                })?;
            (publisher, capability)
        };
        let confirmation = match capability {
            PublicConsentCapability::Prepared {
                namespace,
                disclosure,
                idempotency_key,
                intent,
                ..
            } => {
                // Reading these reviewed fields before consuming the typed
                // capability makes accidental omission visible in code review.
                debug_assert!(!namespace.is_empty());
                debug_assert!(disclosure.permanent);
                debug_assert!(!idempotency_key.is_empty());
                (*intent).confirm()
            }
            PublicConsentCapability::Confirmed { request } => request,
        };
        let runtime = services(&state).await?;
        let result = runtime.publish_confirmed_public_use(
            &confirmation,
            &publisher.author,
            publisher.signer.as_ref(),
        );
        state.vnext_rest.lock().public_capabilities.insert(
            intent_cid,
            PublicConsentCapability::Confirmed {
                request: confirmation,
            },
        );
        let (publication, outcome) = result.map_err(map_runtime_error)?;
        let record = runtime
            .public_use_publication(publication.publication_id)
            .map_err(map_runtime_error)?
            .ok_or_else(|| {
                VNextHttpError::dependency("committed publication projection is unavailable")
            })?;
        let data = publication_view(&record);
        if outcome != onebrain_node::vnext_distributed_pomv::PublicUsePublishOutcome::ExactReplay {
            state
                .vnext_ws
                .publish_publication_state(&headers, &data.state, data.revision, false);
        }
        Ok(success(
            data,
            active_meta(
                CoverageV1::Partial,
                [
                    if outcome
                        == onebrain_node::vnext_distributed_pomv::PublicUsePublishOutcome::ExactReplay
                    {
                        "exact_confirmation_replay"
                    } else {
                        "single_use_consent_committed"
                    },
                    "publication_delivery_is_separate_and_may_be_deferred",
                    "use_evidence_does_not_establish_truth_benefit_or_reward",
                ],
                None,
            ),
        ))
    }
}

pub async fn get_publication(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> VNextResult<PublicationViewV1> {
    #[cfg(not(feature = "vnext-network-runtime"))]
    {
        let _ = (state, id);
        Err(VNextHttpError::disabled(
            "Public UseEvidence publication runtime is not compiled",
            false,
        ))
    }
    #[cfg(feature = "vnext-network-runtime")]
    {
        let id = bytes32("publication_cid", &id)?;
        let runtime = services(&state).await?;
        let record = runtime
            .public_use_publication(id)
            .map_err(map_runtime_error)?
            .ok_or_else(|| VNextHttpError::not_found("publication was not found locally"))?;
        let data = publication_view(&record);
        let limitations = data.limitations.clone();
        Ok(success(
            data,
            active_meta(CoverageV1::Partial, limitations, None),
        ))
    }
}

#[cfg(feature = "vnext-network-runtime")]
fn metabolic_limitation(limitation: ku_core::foundation::MetabolicViewLimitation) -> &'static str {
    use ku_core::foundation::MetabolicViewLimitation;
    match limitation {
        MetabolicViewLimitation::LocalFrontierOnly => "local_frontier_only",
        MetabolicViewLimitation::RecentActivityUsesPerFeedEventDistance => {
            "recent_activity_uses_per_feed_event_distance"
        }
        MetabolicViewLimitation::AuthorityUnresolved => "authority_unresolved",
        MetabolicViewLimitation::UnauthorizedEvidenceExcluded => "unauthorized_evidence_excluded",
        MetabolicViewLimitation::EvidenceBeyondFrontier => "evidence_beyond_frontier",
        MetabolicViewLimitation::EvidencePolicyExcluded => "evidence_policy_excluded",
        MetabolicViewLimitation::LocalEvidenceRetentionBound => "local_evidence_retention_bound",
    }
}

pub async fn get_metabolic_view(
    State(state): State<AppState>,
    Path(target): Path<String>,
    headers: HeaderMap,
) -> VNextResult<MetabolicEvidenceViewV1> {
    #[cfg(not(feature = "vnext-network-runtime"))]
    {
        let _ = (state, target, headers);
        Err(VNextHttpError::disabled(
            "distributed PoMV view runtime is not compiled",
            false,
        ))
    }
    #[cfg(feature = "vnext-network-runtime")]
    {
        use ku_core::foundation::{ExerciseAuthority, ObjectReference, SelectorCid};
        let target_cid = bytes32("target_cid", &target)?;
        let runtime = services(&state).await?;
        let status = runtime.status().map_err(map_runtime_error)?;
        let policy_version = status.policy_versions.first().copied().ok_or_else(|| {
            VNextHttpError::dependency("no allow-listed metabolic view policy is active")
        })?;
        let policy_version =
            onebrain_node::LocalPolicyVersion::new(policy_version).map_err(|error| {
                VNextHttpError::dependency(format!("invalid runtime policy version: {error}"))
            })?;
        let target_reference = ObjectReference::new(0, target_cid);
        let selectors = runtime
            .public_use_selectors_for_target(&target_reference)
            .map_err(map_runtime_error)?;
        let selector = match selectors.as_slice() {
            [selector] => *selector,
            [] => SelectorCid::from_bytes(digest32(
                b"onebrain:vnext:rest-metabolic-view-selector:1\0",
                &[&target_cid],
            )),
            _ => {
                state.vnext_ws.publish_view_state(
                    &headers,
                    target_cid,
                    0,
                    &["multiple_selector_contexts".into()],
                );
                return Err(VNextHttpError::conflict(
                    "target has multiple local selector contexts; an explicit profile revision is required",
                ));
            }
        };
        let report = runtime
            .materialize_public_use_view(selector, target_reference, policy_version)
            .map_err(map_runtime_error)?;
        if report.claims_truth
            || report.claims_benefit
            || report.changes_wallet_state
            || report.changes_obt_state
            || report.claims_network_completion
        {
            return Err(VNextHttpError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "metabolic view violated the product semantic firewall",
                false,
                LifecycleV1::Degraded,
                CoverageV1::Partial,
                ["truth_benefit_reward_global_firewall_violation"],
            ));
        }
        let mut limitations = report
            .view
            .limitations
            .iter()
            .copied()
            .map(metabolic_limitation)
            .map(str::to_string)
            .collect::<Vec<_>>();
        limitations.extend([
            "path_limited_partial_coverage".into(),
            "zero_events_do_not_claim_network_absence".into(),
            "view_does_not_establish_truth_benefit_reward_or_global_completion".into(),
        ]);
        limitations.sort();
        limitations.dedup();
        let conflicts = report
            .observations
            .iter()
            .filter(|observation| observation.authority != ExerciseAuthority::Authorized)
            .map(|observation| hex32(observation.event_cid.as_bytes()))
            .collect::<Vec<_>>();
        state
            .vnext_ws
            .publish_view_state(&headers, target_cid, report.view.revision, &conflicts);
        let data = MetabolicEvidenceViewV1 {
            target_cid: hex32(&report.view.target.cid),
            policy_cid: hex32(&report.view.policy.policy_ref.cid),
            assessed_frontier: hex32(report.view.frontier.authority_frontier()),
            revision: report.view.revision,
            use_event_root: hex32(&report.view.evidence_root),
            conflicts,
            coverage: CoverageV1::Partial,
            limitations: limitations.clone(),
            establishes_truth: false,
            establishes_benefit: false,
            authorizes_reward: false,
            claims_global_completion: false,
        };
        Ok(success(
            data,
            active_meta(CoverageV1::Partial, limitations, None),
        ))
    }
}

#[cfg(test)]
mod tests {
    use axum::body::{to_bytes, Body};
    use axum::http::{Method, Request};
    use serde_json::{json, Value};
    use tower::ServiceExt;

    use super::*;
    use crate::ApiServer;

    async fn test_node(directory: &tempfile::TempDir) -> onebrain_node::OneBrainNode {
        let config = onebrain_node::NodeConfig {
            port: 0,
            data_dir: directory.path().canonicalize().unwrap(),
            concept_registry_mode: onebrain_node::ConceptRegistryMode::Disabled,
            ..Default::default()
        };
        onebrain_node::OneBrainNode::new(config).await.unwrap()
    }

    async fn call(
        router: axum::Router,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        call_with_client_session(router, method, path, body, None).await
    }

    async fn call_with_client_session(
        router: axum::Router,
        method: Method,
        path: &str,
        body: Option<Value>,
        client_session: Option<&str>,
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header("Authorization", "Bearer p3-test-token");
        if let Some(client_session) = client_session {
            builder = builder.header(
                crate::vnext_ws::VNEXT_WS_CLIENT_SESSION_HEADER,
                client_session,
            );
        }
        let body = match body {
            Some(value) => {
                builder = builder.header("Content-Type", "application/json");
                Body::from(serde_json::to_vec(&value).unwrap())
            }
            None => Body::empty(),
        };
        let response = router.oneshot(builder.body(body).unwrap()).await.unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap();
        let body = serde_json::from_slice(&bytes).unwrap();
        (status, body)
    }

    fn valid_need_prepare() -> Value {
        json!({
            "local_query": "FIND (secret:KU) WHERE secret.title = \"UniquePrivateMarker\" SCOPE LOCAL",
            "scope": {
                "kind": "one_hop",
                "max_hops": 1,
                "node_ids": []
            },
            "budget": {
                "max_scan_records": 64,
                "max_affordances": 32,
                "max_pairs": 128,
                "max_proposals": 32
            },
            "idempotency_key": "need-operation-1"
        })
    }

    #[test]
    fn cid_and_continuation_encodings_are_strict_and_context_bound() {
        let cid = [0xabu8; 32];
        assert_eq!(bytes32("cid", &hex32(&cid)).unwrap(), cid);
        assert!(bytes32("cid", &hex32(&cid).to_ascii_uppercase()).is_err());
        assert!(bytes32("cid", &"0".repeat(64)).is_err());

        let context = page_context(b"needs", b"local");
        let token = encode_page_continuation(1, 42, context);
        assert!(token.starts_with("obc1."));
        assert_eq!(
            decode_page_continuation(Some(&token), 1, context).unwrap(),
            42
        );
        assert!(decode_page_continuation(Some(&token), 2, context).is_err());
        assert!(
            decode_page_continuation(Some(&token), 1, page_context(b"matches", b"local")).is_err()
        );
    }

    #[test]
    fn metabolic_projection_serializes_every_fail_closed_flag() {
        let view = MetabolicEvidenceViewV1 {
            target_cid: "11".repeat(32),
            policy_cid: "22".repeat(32),
            assessed_frontier: "33".repeat(32),
            revision: 7,
            use_event_root: "44".repeat(32),
            conflicts: vec!["55".repeat(32)],
            coverage: CoverageV1::Partial,
            limitations: vec!["path_limited".into()],
            establishes_truth: false,
            establishes_benefit: false,
            authorizes_reward: false,
            claims_global_completion: false,
        };
        let value = serde_json::to_value(view).unwrap();
        for field in [
            "establishes_truth",
            "establishes_benefit",
            "authorizes_reward",
            "claims_global_completion",
        ] {
            assert_eq!(value[field], false);
        }
    }

    #[test]
    fn quarantined_match_serializes_literal_non_executable_state() {
        let matched = QuarantinedMatchV1 {
            proposal_cid: "11".repeat(32),
            candidate_cid: "22".repeat(32),
            responder_scope: ScopeV1 {
                kind: ScopeKindV1::AuthenticatedDirectPeers,
                max_hops: 1,
                node_ids: vec!["33".repeat(32)],
            },
            selector_cid: "44".repeat(32),
            assessed_frontier: "55".repeat(32),
            constraints: ConstraintSetV1 {
                observations: Vec::new(),
                all_required_satisfied: true,
            },
            limitations: vec!["quarantined_proposal_only".into()],
            state: "quarantined",
            executable: false,
        };
        let value = serde_json::to_value(matched).unwrap();
        assert_eq!(value["state"], "quarantined");
        assert_eq!(value["executable"], false);
        assert!(value["responder_scope"]["node_ids"].is_array());
        assert!(value["limitations"].is_array());
    }

    #[tokio::test]
    async fn runtime_status_and_disabled_lane_keep_the_vnext_envelope() {
        let directory = tempfile::tempdir().unwrap();
        let node = test_node(&directory).await;
        let router = ApiServer::new(node, "p3-test-token".into(), 0).build_router();
        let (status, body) = call(
            router.clone(),
            Method::GET,
            "/api/vnext/runtime/status",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["ok"], true);
        assert_eq!(body["profile"], VNEXT_PRODUCT_PROFILE);
        assert!(body["data"]["limitations"].is_array());
        assert_eq!(body["data"]["observability"]["profile_major"], 1);
        assert_eq!(
            body["data"]["observability"]["contains_high_cardinality_labels"],
            false
        );
        assert_eq!(
            body["data"]["observability"]["contains_private_need_labels"],
            false
        );
        assert_eq!(
            body["data"]["observability"]["claims_network_completion"],
            false
        );
        assert!(body["meta"]["continuation"].is_null());

        let (status, body) = call(
            router,
            Method::POST,
            "/api/vnext/kql/needs/prepare",
            Some(valid_need_prepare()),
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["ok"], false);
        assert_eq!(body["profile"], VNEXT_PRODUCT_PROFILE);
        assert_eq!(body["error"]["code"], "capability_disabled");
        assert!(body["meta"]["limitations"].is_array());
    }

    #[tokio::test]
    async fn router_exposes_every_frozen_p3_rest_method_and_path() {
        let directory = tempfile::tempdir().unwrap();
        let node = test_node(&directory).await;
        let router = ApiServer::new(node, "p3-test-token".into(), 0).build_router();
        let id = "11".repeat(32);
        let target = "22".repeat(32);
        let cases = [
            (Method::POST, "/api/vnext/kql/needs/prepare".to_string()),
            (Method::POST, "/api/vnext/kql/needs".to_string()),
            (Method::GET, "/api/vnext/kql/needs".to_string()),
            (Method::GET, format!("/api/vnext/kql/needs/{id}")),
            (Method::GET, format!("/api/vnext/kql/needs/{id}/matches")),
            (Method::POST, format!("/api/vnext/kql/needs/{id}/scan")),
            (Method::DELETE, format!("/api/vnext/kql/needs/{id}")),
            (
                Method::POST,
                "/api/vnext/pomv/public-use/prepare".to_string(),
            ),
            (
                Method::POST,
                "/api/vnext/pomv/public-use/confirm".to_string(),
            ),
            (Method::GET, format!("/api/vnext/pomv/publications/{id}")),
            (Method::GET, format!("/api/vnext/pomv/views/{target}")),
            (Method::GET, "/api/vnext/runtime/status".to_string()),
        ];
        for (method, path) in cases {
            let body = (method == Method::POST).then(|| json!({}));
            let (status, _) = call(router.clone(), method, &path, body).await;
            assert_ne!(status, StatusCode::NOT_FOUND, "missing route {path}");
            assert_ne!(
                status,
                StatusCode::METHOD_NOT_ALLOWED,
                "wrong method for route {path}"
            );
        }
    }

    #[cfg(feature = "vnext-network-runtime")]
    fn product_dependencies() -> onebrain_node::VNextProductRuntimeDependencies {
        use ku_core::foundation::{MetabolicViewPolicy, ObjectReference};
        use ku_kql::vnext_private_need::LocalNeedVaultKey;
        let version = onebrain_node::LocalPolicyVersion::new(1).unwrap();
        let policies = onebrain_node::LocalPolicyRegistry::new([(
            version,
            MetabolicViewPolicy {
                policy_ref: ObjectReference::new(0, [0x21; 32]),
                accepted_evidence_policies: vec![ObjectReference::new(0, [0x22; 32])],
                recent_event_horizon: 64,
            },
        )])
        .unwrap();
        onebrain_node::VNextProductRuntimeDependencies::new(
            LocalNeedVaultKey::from_bytes([0x23; 32]),
            policies,
        )
    }

    #[cfg(feature = "vnext-network-runtime")]
    async fn active_router(
        directory: &tempfile::TempDir,
    ) -> (
        axum::Router,
        Arc<tokio::sync::Mutex<onebrain_node::OneBrainNode>>,
        crate::vnext_ws::VNextWsHub,
    ) {
        use ed25519_dalek::SigningKey;
        use ku_core::foundation::{
            decode_feed_inception, DeviceId, FeedInception, NamespaceCommitment,
        };

        let mut config = onebrain_node::NodeConfig {
            port: 0,
            data_dir: directory.path().to_path_buf(),
            concept_registry_mode: onebrain_node::ConceptRegistryMode::Disabled,
            ..Default::default()
        };
        config.vnext.enabled.object_event_v1 = true;
        config.vnext.enabled.obp_rp = true;
        config.vnext.enabled.distributed_kql_one_hop = true;
        config.vnext.enabled.public_use_evidence_publish = true;
        config.vnext.enabled.distributed_pomv_view = true;
        let mut node = onebrain_node::OneBrainNode::new(config).await.unwrap();
        node.set_vnext_identity_signer(Arc::new(SigningKey::from_bytes(&[0x30; 32])));
        node.set_vnext_product_dependencies(product_dependencies())
            .unwrap();
        node.start_network().await.unwrap();

        let feed_key = Arc::new(SigningKey::from_bytes(&[0x31; 32]));
        let feed_bytes = FeedInception::new(
            *feed_key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"p3-rest-feed", [0x32; 32]).unwrap(),
            0,
            DeviceId::from_bytes([0x33; 32]),
        )
        .sign(feed_key.as_ref())
        .unwrap()
        .encode()
        .unwrap();
        let author = decode_feed_inception(&feed_bytes).unwrap();
        let signer: Arc<dyn ku_core::foundation::FeedEventSigner> = feed_key;
        let publisher = VNextFeedPublisher::new(author, signer).unwrap();
        let shared = Arc::new(tokio::sync::Mutex::new(node));
        let server = ApiServer::with_shared_node(Arc::clone(&shared), "p3-test-token".into(), 0)
            .with_vnext_feed_publisher(publisher);
        let hub = server.vnext_ws_hub();
        let router = server.build_router();
        (router, shared, hub)
    }

    #[cfg(feature = "vnext-network-runtime")]
    #[tokio::test]
    async fn need_rest_flow_is_private_idempotent_partial_and_quarantined() {
        let directory = tempfile::tempdir().unwrap();
        let (router, shared, _) = active_router(&directory).await;
        let request = valid_need_prepare();
        let (status, first) = call(
            router.clone(),
            Method::POST,
            "/api/vnext/kql/needs/prepare",
            Some(request.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(first["profile"], VNEXT_PRODUCT_PROFILE);
        assert_eq!(first["data"]["intent_cid"].as_str().unwrap().len(), 64);
        assert!(!first.to_string().contains("UniquePrivateMarker"));

        let (status, replay) = call(
            router.clone(),
            Method::POST,
            "/api/vnext/kql/needs/prepare",
            Some(request),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(replay["data"]["intent_cid"], first["data"]["intent_cid"]);

        let activation = json!({
            "intent_cid": first["data"]["intent_cid"],
            "idempotency_key": "need-operation-1"
        });
        let (status, active) = call(
            router.clone(),
            Method::POST,
            "/api/vnext/kql/needs",
            Some(activation.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(active["data"]["state"], "active");
        let need_id = active["data"]["standing_need_id"]
            .as_str()
            .unwrap()
            .to_string();

        let (_, activation_replay) = call(
            router.clone(),
            Method::POST,
            "/api/vnext/kql/needs",
            Some(activation),
        )
        .await;
        assert_eq!(
            activation_replay["data"]["standing_need_id"],
            active["data"]["standing_need_id"]
        );

        let (_, page) = call(router.clone(), Method::GET, "/api/vnext/kql/needs", None).await;
        assert_eq!(page["data"]["items"].as_array().unwrap().len(), 1);
        assert_eq!(page["data"]["coverage"], "local_only");

        let scan = json!({
            "budget": {
                "max_scan_records": 64,
                "max_affordances": 32,
                "max_pairs": 128,
                "max_proposals": 32
            },
            "continuation": null,
            "idempotency_key": "scan-operation-1"
        });
        let (status, scan_result) = call(
            router.clone(),
            Method::POST,
            &format!("/api/vnext/kql/needs/{need_id}/scan"),
            Some(scan),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(scan_result["meta"]["coverage"], "partial");
        assert!(scan_result
            .to_string()
            .contains("zero_matches_do_not_claim_network_absence"));

        let (_, matches) = call(
            router.clone(),
            Method::GET,
            &format!("/api/vnext/kql/needs/{need_id}/matches"),
            None,
        )
        .await;
        assert_eq!(matches["data"]["items"].as_array().unwrap().len(), 0);
        assert_eq!(matches["data"]["coverage"], "partial");
        assert!(matches
            .to_string()
            .contains("zero_results_do_not_claim_network_absence"));

        let (status, retired) = call(
            router.clone(),
            Method::DELETE,
            &format!("/api/vnext/kql/needs/{need_id}"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(retired["data"]["state"], "retired");
        let (_, retire_replay) = call(
            router,
            Method::DELETE,
            &format!("/api/vnext/kql/needs/{need_id}"),
            None,
        )
        .await;
        assert_eq!(
            retire_replay["data"]["standing_need_id"],
            retired["data"]["standing_need_id"]
        );
        shared.lock().await.shutdown_network().await;
    }

    #[cfg(feature = "vnext-network-runtime")]
    #[tokio::test]
    async fn public_use_requires_exact_explicit_confirmation_and_replays_identity() {
        let directory = tempfile::tempdir().unwrap();
        let (router, shared, hub) = active_router(&directory).await;
        let (client_session, mut ws_events) = hub.open_test_session(&[
            crate::vnext_ws::VNextWsTopicV1::Publications,
            crate::vnext_ws::VNextWsTopicV1::Views,
        ]);
        let expires_at = now_epoch() + 120;
        let prepare = json!({
            "target_cid": "41".repeat(32),
            "recipient_node_id": "42".repeat(32),
            "selector_cid": "43".repeat(32),
            "namespace": "onebrain.public-use.test",
            "disclosure": {
                "classification": "public",
                "permanent": true,
                "use_mode": "application"
            },
            "idempotency_key": "public-use-operation-1",
            "expires_at": expires_at
        });
        let (status, prepared) = call(
            router.clone(),
            Method::POST,
            "/api/vnext/pomv/public-use/prepare",
            Some(prepare),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(prepared["data"]["exact_target"], "41".repeat(32));
        assert!(
            prepared["data"]["canonical_payload_preview"]
                .as_str()
                .unwrap()
                .len()
                > 64
        );
        assert!(!prepared.to_string().contains("single_use_receipt"));

        let intent = prepared["data"]["intent_cid"].as_str().unwrap();
        let (status, rejected) = call(
            router.clone(),
            Method::POST,
            "/api/vnext/pomv/public-use/confirm",
            Some(json!({
                "intent_cid": intent,
                "single_use_receipt": "obc1.wrong"
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(rejected["error"]["code"], "conflict");

        let receipt = public_confirmation_receipt(bytes32("intent_cid", intent).unwrap());
        let confirmation = json!({
            "intent_cid": intent,
            "single_use_receipt": receipt
        });
        let (status, publication) = call_with_client_session(
            router.clone(),
            Method::POST,
            "/api/vnext/pomv/public-use/confirm",
            Some(confirmation.clone()),
            Some(&client_session),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let publication_id = publication["data"]["publication_cid"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(publication_id.len(), 64);
        assert_ne!(publication["data"]["state"], "authorized");
        let publication_event = ws_events.try_recv().unwrap();
        assert!(matches!(
            publication_event.event_type,
            crate::vnext_ws::VNextWsEventTypeV1::PublicationQueued
                | crate::vnext_ws::VNextWsEventTypeV1::PublicationDeferred
        ));

        let (_, replay) = call_with_client_session(
            router.clone(),
            Method::POST,
            "/api/vnext/pomv/public-use/confirm",
            Some(confirmation),
            Some(&client_session),
        )
        .await;
        assert_eq!(
            replay["data"]["publication_cid"],
            publication["data"]["publication_cid"]
        );
        assert!(replay.to_string().contains("exact_confirmation_replay"));
        assert!(ws_events.try_recv().is_err());

        let (status, fetched) = call(
            router.clone(),
            Method::GET,
            &format!("/api/vnext/pomv/publications/{publication_id}"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            fetched["data"]["publication_cid"],
            publication["data"]["publication_cid"]
        );
        let (status, view) = call_with_client_session(
            router,
            Method::GET,
            &format!("/api/vnext/pomv/views/{}", "41".repeat(32)),
            None,
            Some(&client_session),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{view}");
        assert_eq!(view["data"]["coverage"], "partial");
        for field in [
            "establishes_truth",
            "establishes_benefit",
            "authorizes_reward",
            "claims_global_completion",
        ] {
            assert_eq!(view["data"][field], false);
        }
        let view_event = ws_events.try_recv().unwrap();
        assert!(matches!(
            view_event.event_type,
            crate::vnext_ws::VNextWsEventTypeV1::ViewRevision
                | crate::vnext_ws::VNextWsEventTypeV1::ViewConflict
        ));
        let event_wire = serde_json::to_value(view_event).unwrap();
        for field in [
            "establishes_truth",
            "establishes_benefit",
            "authorizes_reward",
            "claims_global_completion",
        ] {
            assert_eq!(event_wire["data"][field], false);
        }
        assert!(!event_wire.to_string().contains(&"41".repeat(32)));
        shared.lock().await.shutdown_network().await;
    }
}
