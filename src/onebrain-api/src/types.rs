//! API request/response types.
//!
//! Shared DTOs for REST endpoints and WebSocket events.

use serde::{Deserialize, Serialize};

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

/// Identity recovery request.
#[derive(Debug, Deserialize)]
pub struct RecoverRequest {
    pub recovery_phrase: Vec<String>,
    pub new_password: String,
}

// ─── Query Parameters ──────────────────────────────────────────────────────

fn default_page() -> usize { 1 }
fn default_ku_limit() -> usize { 20 }
fn default_sort() -> String { "created".to_string() }
fn default_history_limit() -> usize { 50 }
fn default_export_format() -> String { "json".to_string() }
fn default_graph_depth() -> u32 { 2 }
fn default_graph_limit() -> usize { 100 }

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
    #[serde(default = "default_export_format")]
    pub format: String,
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
    pub version: String,
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
