//! Route handler implementations.
//!
//! Each handler acquires `state.node.lock().await`, calls the
//! appropriate `OneBrainNode` method, and returns an `ApiResult<T>`.

use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::Json;
use futures::SinkExt;
use serde::Serialize;
use serde_json::json;

use crate::error::{ApiError, ApiResult};
use crate::server::AppState;
use crate::types::*;

// â”€â”€â”€ Helpers â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Wrap a value in `ApiSuccess` and `Json`.
fn ok<T: Serialize>(data: T) -> Json<ApiSuccess<T>> {
    Json(ApiSuccess::new(data))
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// â”€â”€â”€ Identity â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn get_identity(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let info = node.get_identity_info().map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(info).unwrap()))
}

pub async fn recover_identity(
    State(state): State<AppState>,
    Json(body): Json<RecoverRequest>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let info = node
        .recover_identity(&body.recovery_phrase, &body.new_password)
        .map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(info).unwrap()))
}

// â”€â”€â”€ Knowledge â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn encode_knowledge(
    State(state): State<AppState>,
    Json(body): Json<EncodeRequest>,
) -> ApiResult<serde_json::Value> {
    let _ = body.preview;
    let text = body.text.clone();
    let node_ref = state.node.clone();
    let broadcast_tx = state.event_broadcast.clone();

    // Send initial progress via broadcast (no lock needed)
    let _ = broadcast_tx.send(
        serde_json::to_string(&WsEvent {
            event_type: "encode_progress".to_string(),
            timestamp: now_epoch(),
            data: json!({ "step": 0, "total_steps": 6, "message": "Starting encode pipeline..." }),
        })
        .unwrap_or_default(),
    );

    // Run encode in a spawned task with a 300s timeout.
    let encode_future = async move {
        let mut node = node_ref.lock().await;
        node.encode_and_store_with_progress(&text, Some(&broadcast_tx))
            .await
    };

    let result = tokio::time::timeout(std::time::Duration::from_secs(300), encode_future)
        .await
        .map_err(|_| {
            ApiError(onebrain_node::NodeError::Timeout(
                "Encode timed out after 300 seconds".to_string(),
            ))
        })?
        .map_err(ApiError::from)?;

    // EncodeStoreResult is NOT Serialize, so manually build JSON
    let cid_hex = hex::encode(result.cid);
    let data = json!({
        "cid_hex": cid_hex,
        "wire_size": result.wire_size,
        "instruction_count": result.instruction_count,
        "gene_type": result.gene_type,
        "confidence": result.confidence,
        "source_text": result.source_text,
        "peers_reached": result.peers_reached,
    });
    Ok(ok(data))
}

/// Helper: encode cid bytes to hex (inline, no external dep needed at compile).
mod hex {
    pub fn encode(bytes: [u8; 32]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

pub async fn list_kus(
    State(state): State<AppState>,
    Query(params): Query<KuListParams>,
) -> ApiResult<KuListResponse> {
    let node = state.node.lock().await;
    let type_filter = params.gene_type.as_deref();
    let (kus, total) = node
        .list_kus(params.page, params.limit, type_filter, &params.sort)
        .map_err(ApiError::from)?;
    Ok(ok(KuListResponse {
        kus,
        total,
        page: params.page,
    }))
}

pub async fn get_ku(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let detail = node.get_ku(&cid).map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(detail).unwrap()))
}

pub async fn delete_ku(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let deleted = node.delete_ku(&cid).map_err(ApiError::from)?;
    Ok(ok(json!({ "deleted": deleted, "cid_hex": cid })))
}

pub async fn search_knowledge(
    State(state): State<AppState>,
    Json(body): Json<SearchRequest>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let limit = body.limit.unwrap_or(10);
    let results = node
        .search_text(&body.query, limit)
        .map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(results).unwrap()))
}

pub async fn execute_kql(
    State(state): State<AppState>,
    Json(body): Json<KqlRequest>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let results = node.execute_kql(&body.query).map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(results).unwrap()))
}

#[derive(serde::Deserialize)]
pub struct SuggestQuery {
    pub q: String,
    pub limit: Option<usize>,
}

pub async fn search_suggest(
    State(state): State<AppState>,
    Query(params): Query<SuggestQuery>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let limit = params.limit.unwrap_or(5);
    let suggestions = node
        .search_suggest(&params.q, limit)
        .map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(suggestions).unwrap()))
}

// â”€â”€â”€ Chat â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn chat(
    State(state): State<AppState>,
    Json(body): Json<ChatRequest>,
) -> ApiResult<ChatResponse> {
    let mut node = state.node.lock().await;
    let text = node
        .process_input(&body.message)
        .await
        .map_err(ApiError::from)?;
    Ok(ok(ChatResponse {
        text,
        intent: None,
        suggestions: vec![],
        kus_encoded: 0,
        kus_retrieved: 0,
    }))
}

// â”€â”€â”€ Network â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn get_status(State(state): State<AppState>) -> ApiResult<StatusResponse> {
    let node = state.node.lock().await;
    let ku_count = node.ku_count().unwrap_or(0);
    let peer_count = node.peer_count();
    let uptime_s = state.start_time.elapsed().as_secs();
    let node_name = node.node_name().to_string();
    let concept_registry = node.concept_registry_status().clone();
    let vnext = node.vnext_status();

    // Get balance for tier + obt
    let (tier, obt_balance, obt_economic_status) = match node.get_balance() {
        Ok(w) => (w.tier, w.balance, w.economic_status),
        Err(_) => (
            "Unknown".to_string(),
            0,
            onebrain_node::types::WalletEconomicStatus::SimulatedNonEconomic,
        ),
    };

    Ok(ok(StatusResponse {
        ku_count,
        peer_count,
        uptime_s,
        node_name,
        tier,
        obt_balance,
        obt_economic_status,
        version: env!("CARGO_PKG_VERSION").to_string(),
        model: node.config().model.clone(),
        concept_registry,
        vnext,
    }))
}

/// Return the additive, read-only vNext workflow contract.
///
/// This endpoint describes boundaries and the next explicit action. It does not
/// discover candidates, materialize a Mapping, adopt a Mapping, or assert a
/// network-wide result.
pub async fn get_vnext_workflow() -> ApiResult<Vec<onebrain_node::WorkflowStageView>> {
    Ok(ok(onebrain_node::workflow_surface()))
}

/// Return one stage of the additive, read-only vNext workflow contract.
pub async fn get_vnext_workflow_stage(
    Path(stage): Path<String>,
) -> ApiResult<onebrain_node::WorkflowStageView> {
    let stage = onebrain_node::WorkflowStage::parse(&stage).ok_or_else(|| {
        ApiError(onebrain_node::NodeError::InvalidArgument(format!(
            "Unknown vNext workflow stage: {stage}. Expected assembly, receptor, discover, proposal, mapping, or resolution"
        )))
    })?;
    Ok(ok(onebrain_node::workflow_stage_view(stage)))
}

pub async fn get_peers(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let peers = node.peer_list_snapshot();
    // PeerInfo is NOT Serialize, so manually convert
    let list: Vec<serde_json::Value> = peers
        .iter()
        .map(|p| {
            json!({
                "name": p.name,
                "addr": p.addr.to_string(),
                "ku_count": p.ku_count,
            })
        })
        .collect();
    let count = list.len();
    Ok(ok(json!({ "peers": list, "count": count })))
}

pub async fn connect_peer(
    State(state): State<AppState>,
    Json(body): Json<ConnectRequest>,
) -> ApiResult<serde_json::Value> {
    let addr: SocketAddr = body.address.parse().map_err(|_| {
        ApiError(onebrain_node::NodeError::InvalidArgument(format!(
            "Invalid socket address: {}",
            body.address
        )))
    })?;
    let node = state.node.lock().await;
    node.connect_to_seed(addr).await.map_err(ApiError::from)?;
    Ok(ok(json!({ "connected": true, "address": body.address })))
}

// â”€â”€â”€ Graph â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn get_graph(
    State(state): State<AppState>,
    Path(cid): Path<String>,
    Query(params): Query<GraphParams>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let neighbors = node
        .get_neighbors(&cid, params.depth)
        .map_err(ApiError::from)?;
    Ok(ok(json!({
        "cid_hex": cid,
        "depth": params.depth,
        "neighbors": serde_json::to_value(&neighbors).unwrap(),
    })))
}

pub async fn get_neighbors(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let neighbors = node.get_neighbors(&cid, 1).map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(&neighbors).unwrap()))
}

// â”€â”€â”€ Wallet â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn get_wallet(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let info = node.get_balance().map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(info).unwrap()))
}

pub async fn get_wallet_history(
    State(state): State<AppState>,
    Query(params): Query<HistoryParams>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let txns = node
        .get_wallet_history(params.limit)
        .map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(txns).unwrap()))
}

// â”€â”€â”€ Profile & Settings â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn get_profile(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let profile = node.get_profile().map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(profile).unwrap()))
}

pub async fn update_profile(
    State(state): State<AppState>,
    Json(body): Json<ProfileUpdateRequest>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    if let Some(name) = &body.display_name {
        node.update_profile("name", name).map_err(ApiError::from)?;
    }
    if let Some(lang) = &body.language {
        node.update_profile("language", lang)
            .map_err(ApiError::from)?;
    }
    if let Some(style) = &body.response_style {
        node.update_profile("style", style)
            .map_err(ApiError::from)?;
    }
    let profile = node.get_profile().map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(profile).unwrap()))
}

pub async fn get_settings(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let config = node.get_config_view();
    Ok(ok(serde_json::to_value(config).unwrap()))
}

pub async fn update_settings(
    State(state): State<AppState>,
    Json(body): Json<SettingsUpdateRequest>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    if let Some(name) = &body.name {
        node.update_config("name", name).map_err(ApiError::from)?;
    }
    if let Some(url) = &body.ollama_url {
        node.update_config("ollama_url", url)
            .map_err(ApiError::from)?;
    }
    if let Some(model) = &body.model {
        node.update_config("model", model).map_err(ApiError::from)?;
    }
    let config = node.get_config_view();
    Ok(ok(serde_json::to_value(config).unwrap()))
}

// â”€â”€â”€ AI â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn ai_status(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let info = node.test_ai_connection().await.map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(info).unwrap()))
}

pub async fn list_ai_models(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let models = node.list_ai_models().map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(models).unwrap()))
}

pub async fn switch_model(
    State(state): State<AppState>,
    Json(body): Json<SwitchModelRequest>,
) -> ApiResult<serde_json::Value> {
    let (old_model, ollama_url) = {
        let mut node = state.node.lock().await;
        let old = node.config().model.clone();
        let url = node.config().ollama_url.clone();
        node.switch_model(&body.model_name)
            .map_err(ApiError::from)?;
        (old, url)
    };
    // Node lock released here

    // Unload old model from Ollama to free RAM (server-side, reliable)
    let mut unloaded_models = Vec::new();
    if old_model != body.model_name {
        // First, get all currently loaded models from Ollama
        let client = reqwest::Client::new();
        if let Ok(resp) = client.get(format!("{}/api/ps", ollama_url)).send().await {
            if let Ok(ps_json) = resp.json::<serde_json::Value>().await {
                if let Some(models) = ps_json["models"].as_array() {
                    for m in models {
                        if let Some(name) = m["name"].as_str() {
                            // Unload each loaded model
                            let unload_body = json!({
                                "model": name,
                                "keep_alive": 0
                            });
                            if let Ok(res) = client
                                .post(format!("{}/api/generate", ollama_url))
                                .json(&unload_body)
                                .send()
                                .await
                            {
                                // Consume the streaming response body to ensure Ollama processes it
                                let _ = res.text().await;
                                unloaded_models.push(name.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(ok(json!({
        "model": body.model_name,
        "switched": true,
        "old_model": old_model,
        "unloaded": unloaded_models,
    })))
}

// â”€â”€â”€ Blobs â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn list_blobs(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let blobs = node.list_blobs().map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(blobs).unwrap()))
}

pub async fn get_blob_meta(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let meta = node.get_blob_meta(&cid).map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(meta).unwrap()))
}

pub async fn delete_blob(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let deleted = node.delete_blob_file(&cid).map_err(ApiError::from)?;
    Ok(ok(json!({ "deleted": deleted, "blob_cid_hex": cid })))
}

pub async fn blob_stats(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let (count, total_size) = node.blob_stats().map_err(ApiError::from)?;
    Ok(ok(json!({
        "count": count,
        "total_size": total_size,
    })))
}

pub async fn blob_gc(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let (removed, freed) = node.blob_gc().map_err(ApiError::from)?;
    Ok(ok(json!({
        "removed": removed,
        "freed_bytes": freed,
    })))
}

// â”€â”€â”€ WebSocket â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Query params for WS auth.
#[derive(serde::Deserialize)]
pub struct WsAuthParams {
    pub token: Option<String>,
}

pub async fn ws_events(
    State(state): State<AppState>,
    Query(params): Query<WsAuthParams>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    // Validate token from query param
    let token_valid = params
        .token
        .as_ref()
        .map(|t| t == &state.api_token)
        .unwrap_or(false);

    if !token_valid {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            Json(ApiErrorResponse::new(
                "AUTH_REQUIRED",
                "Invalid or missing WebSocket token",
            )),
        )
            .into_response();
    }

    ws.on_upgrade(move |socket| ws_handler(socket, state))
        .into_response()
}

async fn ws_handler(socket: WebSocket, state: AppState) {
    use futures::stream::StreamExt;

    let (mut sender, mut receiver) = socket.split();

    // Subscribe to broadcast channel for real-time events (encode progress, etc.)
    let mut broadcast_rx = state.event_broadcast.subscribe();

    // Spawn a task that receives broadcast events and forwards to WS client
    let send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                // Priority: broadcast events (no lock needed)
                result = broadcast_rx.recv() => {
                    match result {
                        Ok(json) => {
                            if sender.send(Message::Text(json.into())).await.is_err() {
                                return; // Client disconnected
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => return,
                    }
                }
            }
        }
    });

    // Read incoming messages (keep alive / close detection)
    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Close(_) => break,
            _ => {} // Ignore other messages
        }
    }

    send_task.abort();
}

/// Convert a `NodeEvent` (not Serialize) into a `WsEvent`.
pub fn node_event_to_ws_pub(event: &onebrain_node::NodeEvent) -> WsEvent {
    let ts = now_epoch();
    match event {
        onebrain_node::NodeEvent::PeerConnected(peer) => WsEvent {
            event_type: "peer_connected".to_string(),
            timestamp: ts,
            data: json!({
                "name": peer.name,
                "addr": peer.addr.to_string(),
                "ku_count": peer.ku_count,
            }),
        },
        onebrain_node::NodeEvent::KuReceived {
            cid_hex,
            source_text,
            from,
            ..
        } => WsEvent {
            event_type: "ku_received".to_string(),
            timestamp: ts,
            data: json!({
                "cid_hex": cid_hex,
                "source_text": source_text,
                "from": from,
            }),
        },
        onebrain_node::NodeEvent::VerifyResult {
            cid_hex,
            agreement_score,
            verified,
            from,
        } => WsEvent {
            event_type: "verify_result".to_string(),
            timestamp: ts,
            data: json!({
                "cid_hex": cid_hex,
                "agreement_score": agreement_score,
                "verified": verified,
                "from": from,
            }),
        },
        onebrain_node::NodeEvent::Notification(msg) => WsEvent {
            event_type: "notification".to_string(),
            timestamp: ts,
            data: json!({ "message": msg }),
        },
        onebrain_node::NodeEvent::EncodeProgress {
            step,
            total_steps,
            message,
        } => WsEvent {
            event_type: "encode_progress".to_string(),
            timestamp: ts,
            data: json!({
                "step": step,
                "total_steps": total_steps,
                "message": message,
            }),
        },
    }
}

// â”€â”€â”€ Phase 1: Knowledge Management â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn deprecate_ku(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let deprecated = node.deprecate_ku(&cid).map_err(ApiError::from)?;
    Ok(ok(json!({ "deprecated": deprecated, "cid_hex": cid })))
}

pub async fn save_draft(
    State(state): State<AppState>,
    Json(body): Json<DraftRequest>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let draft = node
        .save_draft(&body.text, body.title.as_deref())
        .map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(draft).unwrap()))
}

pub async fn list_drafts(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let drafts = node.list_drafts();
    Ok(ok(json!({ "drafts": drafts, "total": drafts.len() })))
}

pub async fn get_draft(
    State(state): State<AppState>,
    Path(draft_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let draft = node.get_draft(&draft_id).map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(draft).unwrap()))
}

pub async fn update_draft(
    State(state): State<AppState>,
    Path(draft_id): Path<String>,
    Json(body): Json<DraftRequest>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let draft = node
        .update_draft(&draft_id, &body.text, body.title.as_deref())
        .map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(draft).unwrap()))
}

pub async fn delete_draft(
    State(state): State<AppState>,
    Path(draft_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let deleted = node.delete_draft(&draft_id).map_err(ApiError::from)?;
    Ok(ok(json!({ "deleted": deleted, "draft_id": draft_id })))
}

pub async fn publish_draft(
    State(state): State<AppState>,
    Path(draft_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let result = node
        .publish_draft(&draft_id)
        .await
        .map_err(ApiError::from)?;
    let cid_hex = hex::encode(result.cid);
    Ok(ok(json!({
        "cid_hex": cid_hex,
        "wire_size": result.wire_size,
        "instruction_count": result.instruction_count,
        "gene_type": result.gene_type,
        "confidence": result.confidence,
        "peers_reached": result.peers_reached,
    })))
}

// â”€â”€â”€ Phase 1: Tags â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn add_tag(
    State(state): State<AppState>,
    Path(cid): Path<String>,
    Json(body): Json<TagRequest>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    node.add_tag(&cid, &body.tag).map_err(ApiError::from)?;
    Ok(ok(
        json!({ "added": true, "cid_hex": cid, "tag": body.tag }),
    ))
}

pub async fn remove_tag(
    State(state): State<AppState>,
    Path((cid, tag)): Path<(String, String)>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    node.remove_tag(&cid, &tag).map_err(ApiError::from)?;
    Ok(ok(json!({ "removed": true, "cid_hex": cid, "tag": tag })))
}

pub async fn list_tags(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let tags = node.list_all_tags();
    Ok(ok(json!({ "tags": tags, "count": tags.len() })))
}

/// Get tags for a specific KU.
pub async fn get_ku_tags(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let tags = node.get_ku_tags(&cid);
    Ok(ok(json!({ "tags": tags, "cid_hex": cid })))
}

/// Stake OBT tokens.
pub async fn stake(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let amount = body["amount"].as_u64().unwrap_or(0);
    let mut node = state.node.lock().await;
    let info = node.stake(amount).map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(info).unwrap()))
}

/// Unstake OBT tokens.
pub async fn unstake(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let amount = body["amount"].as_u64().unwrap_or(0);
    let mut node = state.node.lock().await;
    let info = node.unstake(amount).map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(info).unwrap()))
}

// ─── Profile & Settings ────────────────────────────────────────────

pub async fn pin_ku(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let pinned = node.pin_ku(&cid).map_err(ApiError::from)?;
    Ok(ok(json!({ "pinned": pinned, "cid_hex": cid })))
}

pub async fn unpin_ku(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let unpinned = node.unpin_ku(&cid).map_err(ApiError::from)?;
    Ok(ok(json!({ "unpinned": unpinned, "cid_hex": cid })))
}

pub async fn list_pinned_kus(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let kus = node.pinned_kus();
    Ok(ok(serde_json::to_value(&kus).unwrap()))
}

// â”€â”€â”€ Phase 1: Social & Discovery â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn follow_node(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    node.follow_node(&node_id).map_err(ApiError::from)?;
    Ok(ok(json!({ "followed": true, "node_id": node_id })))
}

pub async fn unfollow_node(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    node.unfollow_node(&node_id).map_err(ApiError::from)?;
    Ok(ok(json!({ "unfollowed": true, "node_id": node_id })))
}

pub async fn list_following(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let following = node.following_list();
    Ok(ok(serde_json::to_value(&following).unwrap()))
}

pub async fn get_peer_profile(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    match node.get_peer_profile(&node_id) {
        Some(profile) => Ok(ok(serde_json::to_value(&profile).unwrap())),
        None => Err(ApiError(onebrain_node::NodeError::KuNotFound(format!(
            "Node not found: {}",
            node_id
        )))),
    }
}

// â”€â”€â”€ Phase 1: Multi-Device â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn list_devices(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let devices = node.list_devices();
    Ok(ok(serde_json::to_value(&devices).unwrap()))
}

pub async fn sync_status(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let status = node.sync_status();
    Ok(ok(serde_json::to_value(&status).unwrap()))
}

// â”€â”€â”€ Phase 1: Bulk Operations â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn bulk_delete_kus(
    State(state): State<AppState>,
    Json(body): Json<BulkDeleteRequest>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let result = node
        .bulk_delete(body.gene_type.as_deref(), body.before_timestamp)
        .map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(&result).unwrap()))
}

// â”€â”€â”€ Phase 1: Watch (Standing Queries) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn create_watch(
    State(state): State<AppState>,
    Json(body): Json<WatchRequest>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let watch_id = node.create_watch(&body.query).map_err(ApiError::from)?;
    Ok(ok(json!({ "watch_id": watch_id, "query": body.query })))
}

pub async fn list_watches(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let watches = node.list_watches();
    Ok(ok(serde_json::to_value(&watches).unwrap()))
}

pub async fn delete_watch(
    State(state): State<AppState>,
    Path(watch_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let deleted = node.delete_watch(&watch_id).map_err(ApiError::from)?;
    Ok(ok(json!({ "deleted": deleted, "watch_id": watch_id })))
}

// â”€â”€â”€ Phase 1: Blob Extensions â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn add_blob_ku_ref(
    State(state): State<AppState>,
    Path(cid): Path<String>,
    Json(body): Json<BlobRefRequest>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    node.blob_add_ku_ref(&cid, &body.ku_cid)
        .map_err(ApiError::from)?;
    Ok(ok(
        json!({ "linked": true, "blob_cid": cid, "ku_cid": body.ku_cid }),
    ))
}

pub async fn pin_blob(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let pinned = node.pin_blob(&cid).map_err(ApiError::from)?;
    Ok(ok(json!({ "pinned": pinned, "blob_cid": cid })))
}

pub async fn unpin_blob(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let unpinned = node.unpin_blob(&cid).map_err(ApiError::from)?;
    Ok(ok(json!({ "unpinned": unpinned, "blob_cid": cid })))
}

// â”€â”€â”€ Phase 1: Data Portability â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn export_kus(
    State(state): State<AppState>,
    Query(params): Query<ExportParams>,
) -> Result<axum::response::Response, ApiError> {
    let node = state.node.lock().await;
    let temp_dir = tempfile::tempdir().map_err(|e| ApiError(onebrain_node::NodeError::Io(e)))?;
    let ext = match params.mode {
        DataPortabilityMode::CanonicalV1 => "obx",
        DataPortabilityMode::JsonViewV1 => "json",
        DataPortabilityMode::CsvViewV1 => "csv",
        DataPortabilityMode::TextDraftsV1 => {
            return Err(ApiError(onebrain_node::NodeError::InvalidArgument(
                "text-drafts-v1 is import-only".into(),
            )))
        }
    };
    let file_path = temp_dir.path().join(format!("export.{}", ext));
    let count = node
        .export_data(params.mode.as_str(), &file_path)
        .map_err(ApiError::from)?;
    drop(node);

    let data = tokio::fs::read(&file_path)
        .await
        .map_err(|e| ApiError(onebrain_node::NodeError::Io(e)))?;

    let content_type = match params.mode {
        DataPortabilityMode::CanonicalV1 => "application/vnd.onebrain.obx-v1",
        DataPortabilityMode::JsonViewV1 => "application/json",
        DataPortabilityMode::CsvViewV1 => "text/csv",
        DataPortabilityMode::TextDraftsV1 => unreachable!(),
    };
    let filename = format!("onebrain_export_{}.{}", count, ext);

    Ok(axum::response::Response::builder()
        .status(200)
        .header("Content-Type", content_type)
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", filename),
        )
        .header("X-Export-Count", count.to_string())
        .body(axum::body::Body::from(data))
        .unwrap())
}

pub async fn import_kus(
    State(state): State<AppState>,
    Query(params): Query<ImportParams>,
    mut multipart: axum::extract::Multipart,
) -> ApiResult<serde_json::Value> {
    let temp_dir = tempfile::tempdir().map_err(|e| ApiError(onebrain_node::NodeError::Io(e)))?;
    let mut file_path = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        ApiError(onebrain_node::NodeError::InvalidArgument(format!(
            "Multipart error: {}",
            e
        )))
    })? {
        if field.name() == Some("file") {
            let filename = field.file_name().unwrap_or("import.txt").to_string();
            let path = temp_dir.path().join(&filename);
            let data = field.bytes().await.map_err(|e| {
                ApiError(onebrain_node::NodeError::InvalidArgument(format!(
                    "Read error: {}",
                    e
                )))
            })?;
            tokio::fs::write(&path, &data)
                .await
                .map_err(|e| ApiError(onebrain_node::NodeError::Io(e)))?;
            file_path = Some(path);
        }
    }

    let path = file_path.ok_or_else(|| {
        ApiError(onebrain_node::NodeError::InvalidArgument(
            "No file field in multipart".into(),
        ))
    })?;

    let mut node = state.node.lock().await;
    let result = match params.mode {
        DataPortabilityMode::CanonicalV1 => node
            .import_canonical_exchange(&path)
            .map_err(ApiError::from)?,
        DataPortabilityMode::TextDraftsV1 => node
            .import_text_drafts(&path)
            .await
            .map_err(ApiError::from)?,
        DataPortabilityMode::JsonViewV1 | DataPortabilityMode::CsvViewV1 => {
            return Err(ApiError(onebrain_node::NodeError::InvalidArgument(
                "JSON/CSV views are not importable".into(),
            )))
        }
    };
    Ok(ok(serde_json::to_value(&result).unwrap()))
}

pub async fn create_backup(
    State(state): State<AppState>,
    Json(body): Json<BackupRequest>,
) -> Result<axum::response::Response, ApiError> {
    let node = state.node.lock().await;
    let temp_dir = tempfile::tempdir().map_err(|e| ApiError(onebrain_node::NodeError::Io(e)))?;
    let file_path = temp_dir.path().join("backup.onebrain");
    let info = node
        .create_backup(&file_path, &body.password)
        .map_err(ApiError::from)?;
    drop(node);

    let data = tokio::fs::read(&file_path)
        .await
        .map_err(|e| ApiError(onebrain_node::NodeError::Io(e)))?;

    let filename = format!("onebrain_backup_{}.onebrain", info.timestamp);

    Ok(axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/octet-stream")
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", filename),
        )
        .header("X-Backup-KU-Count", info.ku_count.to_string())
        .header("X-Backup-Size", info.size.to_string())
        .body(axum::body::Body::from(data))
        .unwrap())
}

pub async fn restore_backup(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> ApiResult<serde_json::Value> {
    let temp_dir = tempfile::tempdir().map_err(|e| ApiError(onebrain_node::NodeError::Io(e)))?;
    let mut file_path = None;
    let mut password = String::new();

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        ApiError(onebrain_node::NodeError::InvalidArgument(format!(
            "Multipart error: {}",
            e
        )))
    })? {
        match field.name() {
            Some("file") => {
                let path = temp_dir.path().join("restore.onebrain");
                let data = field.bytes().await.map_err(|e| {
                    ApiError(onebrain_node::NodeError::InvalidArgument(format!(
                        "Read error: {}",
                        e
                    )))
                })?;
                tokio::fs::write(&path, &data)
                    .await
                    .map_err(|e| ApiError(onebrain_node::NodeError::Io(e)))?;
                file_path = Some(path);
            }
            Some("password") => {
                password = field.text().await.map_err(|e| {
                    ApiError(onebrain_node::NodeError::InvalidArgument(format!(
                        "Read error: {}",
                        e
                    )))
                })?;
            }
            _ => {}
        }
    }

    let path = file_path.ok_or_else(|| {
        ApiError(onebrain_node::NodeError::InvalidArgument(
            "No file field in multipart".into(),
        ))
    })?;

    let mut node = state.node.lock().await;
    node.restore_backup(&path, &password)
        .map_err(ApiError::from)?;
    Ok(ok(json!({ "restored": true })))
}

// â”€â”€â”€ Phase 1: Blob Upload & Download â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn upload_blob(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> ApiResult<serde_json::Value> {
    let temp_dir = tempfile::tempdir().map_err(|e| ApiError(onebrain_node::NodeError::Io(e)))?;
    let mut file_path = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        ApiError(onebrain_node::NodeError::InvalidArgument(format!(
            "Multipart error: {}",
            e
        )))
    })? {
        if field.name() == Some("file") {
            let filename = field.file_name().unwrap_or("upload").to_string();
            let path = temp_dir.path().join(&filename);
            let data = field.bytes().await.map_err(|e| {
                ApiError(onebrain_node::NodeError::InvalidArgument(format!(
                    "Read error: {}",
                    e
                )))
            })?;
            tokio::fs::write(&path, &data)
                .await
                .map_err(|e| ApiError(onebrain_node::NodeError::Io(e)))?;
            file_path = Some(path);
        }
    }

    let path = file_path.ok_or_else(|| {
        ApiError(onebrain_node::NodeError::InvalidArgument(
            "No file field in multipart".into(),
        ))
    })?;

    let node = state.node.lock().await;
    let meta = node.store_blob(&path).map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(&meta).unwrap()))
}

pub async fn download_blob(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> Result<axum::response::Response, ApiError> {
    let node = state.node.lock().await;
    let meta = node.get_blob_meta(&cid).map_err(ApiError::from)?;
    let temp_dir = tempfile::tempdir().map_err(|e| ApiError(onebrain_node::NodeError::Io(e)))?;
    let file_path = temp_dir.path().join(&meta.original_name);
    node.export_blob(&cid, &file_path).map_err(ApiError::from)?;
    drop(node);

    let data = tokio::fs::read(&file_path)
        .await
        .map_err(|e| ApiError(onebrain_node::NodeError::Io(e)))?;

    Ok(axum::response::Response::builder()
        .status(200)
        .header("Content-Type", &meta.mime_type)
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", meta.original_name),
        )
        .body(axum::body::Body::from(data))
        .unwrap())
}

// ═══════════════════════════════════════════════════════════════════════════
// Tier C — Search History
// ═══════════════════════════════════════════════════════════════════════════

pub async fn record_search(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let query = body["query"].as_str().unwrap_or("");
    let result_count = body["result_count"].as_u64().unwrap_or(0) as usize;
    let mut node = state.node.lock().await;
    let entry = node.record_search(query, result_count);
    Ok(ok(serde_json::to_value(&entry).unwrap()))
}

pub async fn list_search_history(
    State(state): State<AppState>,
    Query(params): Query<LimitQuery>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let limit = params.limit.unwrap_or(50);
    let history = node.list_search_history(limit);
    Ok(ok(json!({ "history": history })))
}

pub async fn clear_search_history(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    node.clear_search_history();
    Ok(ok(json!({ "cleared": true })))
}

// ═══════════════════════════════════════════════════════════════════════════
// Tier C — Notification Preferences
// ═══════════════════════════════════════════════════════════════════════════

pub async fn get_notification_prefs(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let prefs = node.get_notification_prefs();
    Ok(ok(serde_json::to_value(&prefs).unwrap()))
}

pub async fn set_notification_prefs(
    State(state): State<AppState>,
    Json(prefs): Json<onebrain_node::types::NotificationPrefs>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    node.set_notification_prefs(prefs.clone());
    Ok(ok(serde_json::to_value(&prefs).unwrap()))
}

// ═══════════════════════════════════════════════════════════════════════════
// Tier C — Saved Searches
// ═══════════════════════════════════════════════════════════════════════════

pub async fn save_search(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let name = body["name"].as_str().unwrap_or("");
    let query = body["query"].as_str().unwrap_or("");
    let is_kql = body["is_kql"].as_bool().unwrap_or(false);
    let mut node = state.node.lock().await;
    let saved = node
        .save_search(name, query, is_kql)
        .map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(&saved).unwrap()))
}

pub async fn list_saved_searches(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let searches = node.list_saved_searches();
    Ok(ok(json!({ "saved_searches": searches })))
}

pub async fn delete_saved_search(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let deleted = node.delete_saved_search(&id).map_err(ApiError::from)?;
    Ok(ok(json!({ "deleted": deleted })))
}

// ═══════════════════════════════════════════════════════════════════════════
// Tier C — Collections
// ═══════════════════════════════════════════════════════════════════════════

pub async fn create_collection(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let name = body["name"].as_str().unwrap_or("");
    let description = body["description"].as_str().unwrap_or("");
    let mut node = state.node.lock().await;
    let coll = node
        .create_collection(name, description)
        .map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(&coll).unwrap()))
}

pub async fn list_collections(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let collections = node.list_collections();
    Ok(ok(json!({ "collections": collections })))
}

pub async fn get_collection(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let coll = node.get_collection(&id).ok_or_else(|| {
        ApiError(onebrain_node::NodeError::InvalidArgument(format!(
            "Collection '{}' not found",
            id
        )))
    })?;
    Ok(ok(serde_json::to_value(&coll).unwrap()))
}

pub async fn add_to_collection(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let cid_hex = body["cid_hex"].as_str().unwrap_or("");
    let mut node = state.node.lock().await;
    node.add_to_collection(&id, cid_hex)
        .map_err(ApiError::from)?;
    Ok(ok(json!({ "added": true })))
}

pub async fn remove_from_collection(
    State(state): State<AppState>,
    Path((id, cid)): Path<(String, String)>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    node.remove_from_collection(&id, &cid)
        .map_err(ApiError::from)?;
    Ok(ok(json!({ "removed": true })))
}

pub async fn delete_collection(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let deleted = node.delete_collection(&id).map_err(ApiError::from)?;
    Ok(ok(json!({ "deleted": deleted })))
}

// ═══════════════════════════════════════════════════════════════════════════
// Tier C — KU Version Chain
// ═══════════════════════════════════════════════════════════════════════════

pub async fn get_version_chain(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let chain = node.get_ku_version_chain(&cid).map_err(ApiError::from)?;
    Ok(ok(json!({ "versions": chain })))
}

// ═══════════════════════════════════════════════════════════════════════════
// Tier C — Trending KUs
// ═══════════════════════════════════════════════════════════════════════════

pub async fn trending_kus(
    State(state): State<AppState>,
    Query(params): Query<LimitQuery>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let limit = params.limit.unwrap_or(10);
    let trending = node.trending_kus(limit).map_err(ApiError::from)?;
    Ok(ok(json!({ "trending": trending })))
}

// ═══════════════════════════════════════════════════════════════════════════
// Tier C — Recommendations
// ═══════════════════════════════════════════════════════════════════════════

pub async fn recommended_kus(
    State(state): State<AppState>,
    Query(params): Query<LimitQuery>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let limit = params.limit.unwrap_or(10);
    let recs = node.recommended_kus(limit).map_err(ApiError::from)?;
    Ok(ok(json!({ "recommendations": recs })))
}

// ═══════════════════════════════════════════════════════════════════════════
// Tier C — Analytics
// ═══════════════════════════════════════════════════════════════════════════

pub async fn get_analytics(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let analytics = node.get_analytics().map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(&analytics).unwrap()))
}

// ═══════════════════════════════════════════════════════════════════════════
// Tier C — Domain Taxonomy
// ═══════════════════════════════════════════════════════════════════════════

pub async fn list_domains(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let domains = node.list_domains().map_err(ApiError::from)?;
    Ok(ok(json!({ "domains": domains })))
}

pub async fn kus_by_domain(
    State(state): State<AppState>,
    Path(domain): Path<String>,
    Query(params): Query<PaginationQuery>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let page = params.page.unwrap_or(1);
    let limit = params.limit.unwrap_or(20);
    let (kus, total) = node
        .kus_by_domain(&domain, page, limit)
        .map_err(ApiError::from)?;
    Ok(ok(json!({ "kus": kus, "total": total, "page": page })))
}
