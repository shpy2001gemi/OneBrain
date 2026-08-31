//! API request/response types.
//!
//! Shared DTOs for REST endpoints and WebSocket events.

use serde::{Deserialize, Serialize};

/// Bounded product-neutral operation projection. Archive capabilities remain
/// scoped handles; no path or Rust runtime object is accepted here.
#[derive(Debug, Deserialize)]
pub struct BaseOperationProjectionRequest {
    pub operation: String,
    /// Opaque REST management-session identifier returned by
    /// `management_open`; never a Rust pointer or runtime reference.
    #[serde(default)]
    pub management_handle: Option<String>,
    /// Explicit scopes selected by authenticated local host policy.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Opaque archive capability identifier returned by a management action.
    #[serde(default)]
    pub capability_id: Option<String>,
    #[serde(default)]
    pub kind: Option<u16>,
    #[serde(default)]
    pub operation_id: Option<String>,
    #[serde(default)]
    pub reservation_id: Option<String>,
    #[serde(default)]
    pub idempotency_key: Option<String>,
    #[serde(default)]
    pub auxiliary_id: Option<String>,
    #[serde(default)]
    pub payload: Option<String>,
    /// Lossless binary payload/credential projection. Supplying both this
    /// field and `payload` is rejected rather than choosing one implicitly.
    #[serde(default)]
    pub payload_hex: Option<String>,
    #[serde(default)]
    pub topic: Option<u16>,
    #[serde(default)]
    pub cursor: Option<u64>,
    #[serde(default)]
    pub offset: Option<u64>,
    #[serde(default)]
    pub declared_total_bytes: Option<u64>,
    #[serde(default)]
    pub credential_kind: Option<u8>,
    /// Binary archive chunks are represented as bounded hexadecimal text.
    #[serde(default)]
    pub chunk_hex: Option<String>,
    #[serde(default)]
    pub max_items: Option<u32>,
    #[serde(default)]
    pub max_bytes: Option<u64>,
    #[serde(default)]
    pub max_work_units: Option<u64>,
}

// ─── Success / Error Wrappers ──────────────────────────────────────────────

/// Uniform success envelope.
#[derive(Debug, Serialize)]
pub struct ApiSuccess<T: Serialize> {
    pub ok: bool,
    pub data: T,
}

impl<T: Serialize> ApiSuccess<T> {
    pub fn new(data: T) -> Self {
        Self { ok: true, data }
    }
}

/// Uniform error envelope.
#[derive(Debug, Serialize)]
pub struct ApiErrorResponse {
    pub ok: bool,
    pub error: ErrorDetail,
}

/// Error detail body.
#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ApiErrorResponse {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: ErrorDetail {
                code: code.into(),
                message: message.into(),
                details: None,
            },
        }
    }

    pub fn with_details(mut self, value: serde_json::Value) -> Self {
        self.error.details = Some(value);
        self
    }
}

// ─── Request Bodies ────────────────────────────────────────────────────────

/// Encode text into a KU.
#[derive(Debug, Deserialize)]
pub struct EncodeRequest {
    pub text: String,
    #[serde(default)]
    pub preview: bool,
}

/// Chat / process_input request.
#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
}

/// KQL query request.
#[derive(Debug, Deserialize)]
pub struct KqlRequest {
    pub query: String,
}

/// Search request.
#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub limit: Option<usize>,
    pub gene_type: Option<String>,
}

/// Connect to a peer address.
#[derive(Debug, Deserialize)]
pub struct ConnectRequest {
    pub address: String,
}

/// Update user profile fields.
#[derive(Debug, Deserialize)]
pub struct ProfileUpdateRequest {
    pub display_name: Option<String>,
    pub language: Option<String>,
    pub response_style: Option<String>,
}

/// Update node settings.
#[derive(Debug, Deserialize)]
pub struct SettingsUpdateRequest {
    pub name: Option<String>,
    pub ollama_url: Option<String>,
    pub model: Option<String>,
}

/// Switch AI model.
#[derive(Debug, Deserialize)]
pub struct SwitchModelRequest {
    pub model_name: String,
}

// ─── Query Parameters ──────────────────────────────────────────────────────

fn default_page() -> usize {
    1
}
fn default_ku_limit() -> usize {
    20
}
fn default_sort() -> String {
    "created".to_string()
}
fn default_history_limit() -> usize {
    50
}
fn default_graph_depth() -> u32 {
    2
}
fn default_graph_limit() -> usize {
    100
}

/// Query params for listing KUs.
#[derive(Debug, Deserialize)]
pub struct KuListParams {
    #[serde(default = "default_page")]
    pub page: usize,
    #[serde(default = "default_ku_limit")]
    pub limit: usize,
    pub gene_type: Option<String>,
    #[serde(default = "default_sort")]
    pub sort: String,
}

/// Query params for wallet history.
#[derive(Debug, Deserialize)]
pub struct HistoryParams {
    #[serde(default = "default_history_limit")]
    pub limit: usize,
}

/// Query params for export.
#[derive(Debug, Deserialize)]
pub struct ExportParams {
    pub mode: DataPortabilityMode,
}

/// Query params for import. The mode is mandatory so a view or text draft can
/// never be mistaken for canonical bytes.
#[derive(Debug, Deserialize)]
pub struct ImportParams {
    pub mode: DataPortabilityMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum DataPortabilityMode {
    #[serde(rename = "canonical-v1")]
    CanonicalV1,
    #[serde(rename = "json-view-v1")]
    JsonViewV1,
    #[serde(rename = "csv-view-v1")]
    CsvViewV1,
    #[serde(rename = "text-drafts-v1")]
    TextDraftsV1,
}

impl DataPortabilityMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CanonicalV1 => "canonical-v1",
            Self::JsonViewV1 => "json-view-v1",
            Self::CsvViewV1 => "csv-view-v1",
            Self::TextDraftsV1 => "text-drafts-v1",
        }
    }
}

/// Query params for graph traversal.
#[derive(Debug, Deserialize)]
pub struct GraphParams {
    #[serde(default = "default_graph_depth")]
    pub depth: u32,
}

/// Query params for graph list endpoints.
#[derive(Debug, Deserialize)]
pub struct GraphListParams {
    #[serde(default = "default_graph_limit")]
    pub limit: usize,
}

// ─── Response Bodies ───────────────────────────────────────────────────────

/// Node status response.
#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub ku_count: usize,
    pub peer_count: usize,
    pub uptime_s: u64,
    pub node_name: String,
    pub tier: String,
    pub obt_balance: u64,
    /// Explicitly distinguishes the placeholder from economic OBT.
    pub obt_economic_status: onebrain_node::types::WalletEconomicStatus,
    pub version: String,
    pub model: String,
    /// Effective external Concept Registry policy and startup result.
    pub concept_registry: onebrain_node::ConceptRegistryStatus,
    /// Scope-aware vNext status. This projection is display-only.
    pub vnext: onebrain_node::vnext_status::VNextStatusSnapshot,
}

/// Paginated KU list response.
#[derive(Debug, Serialize)]
pub struct KuListResponse {
    pub kus: Vec<onebrain_node::types::KuListItem>,
    pub total: usize,
    pub page: usize,
}

/// Chat response.
#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    pub suggestions: Vec<String>,
    pub kus_encoded: u64,
    pub kus_retrieved: u64,
}

/// WebSocket event envelope.
#[derive(Debug, Serialize)]
pub struct WsEvent {
    pub event_type: String,
    pub timestamp: u64,
    pub data: serde_json::Value,
}

// ─── Phase 1: New Request Bodies ───────────────────────────────────────────

/// Backup creation request.
#[derive(Debug, Deserialize)]
pub struct BackupRequest {
    #[serde(default = "default_backup_password")]
    pub password: String,
}

fn default_backup_password() -> String {
    String::new()
}

/// Blob reference request — link a blob to a KU.
#[derive(Debug, Deserialize)]
pub struct BlobRefRequest {
    pub ku_cid: String,
}

/// Tag add/remove request.
#[derive(Debug, Deserialize)]
pub struct TagRequest {
    pub tag: String,
}

/// Bulk delete request.
#[derive(Debug, Deserialize)]
pub struct BulkDeleteRequest {
    pub gene_type: Option<String>,
    pub before_timestamp: Option<u64>,
}

/// WATCH query creation request.
#[derive(Debug, Deserialize)]
pub struct WatchRequest {
    pub query: String,
}

/// Draft creation request.
#[derive(Debug, Deserialize)]
pub struct DraftRequest {
    pub text: String,
    pub title: Option<String>,
}

/// Restore backup request (password only — file comes via multipart).
#[derive(Debug, Deserialize)]
pub struct RestoreRequest {
    #[serde(default)]
    pub password: String,
}

// ─── Phase 1: New Query Parameters ─────────────────────────────────────────

fn default_search_history_limit() -> usize {
    50
}

/// Query params for search history.
#[derive(Debug, Deserialize)]
pub struct SearchHistoryParams {
    #[serde(default = "default_search_history_limit")]
    pub limit: usize,
}

/// Generic limit query parameter.
#[derive(Debug, Deserialize)]
pub struct LimitQuery {
    pub limit: Option<usize>,
}

/// Generic pagination query parameters.
#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub page: Option<usize>,
    pub limit: Option<usize>,
}

#[cfg(test)]
mod data_portability_tests {
    use super::*;

    #[test]
    fn export_and_import_modes_are_explicit_and_closed() {
        assert!(serde_json::from_value::<ExportParams>(serde_json::json!({})).is_err());
        assert!(serde_json::from_value::<ImportParams>(serde_json::json!({})).is_err());
        for (wire, expected) in [
            ("canonical-v1", DataPortabilityMode::CanonicalV1),
            ("json-view-v1", DataPortabilityMode::JsonViewV1),
            ("csv-view-v1", DataPortabilityMode::CsvViewV1),
            ("text-drafts-v1", DataPortabilityMode::TextDraftsV1),
        ] {
            let parsed: ExportParams =
                serde_json::from_value(serde_json::json!({ "mode": wire })).unwrap();
            assert_eq!(parsed.mode, expected);
            assert_eq!(parsed.mode.as_str(), wire);
        }
        assert!(serde_json::from_value::<ExportParams>(serde_json::json!({
            "mode": "json"
        }))
        .is_err());
    }
}
