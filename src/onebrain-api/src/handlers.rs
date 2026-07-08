//! Route handler implementations.
//!
//! Each handler acquires `state.node.lock().await`, calls the
//! appropriate `OneBrainNode` method, and returns an `ApiResult<T>`.

use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::extract::ws::{Message, WebSocket};
use axum::response::IntoResponse;
use axum::Json;
use futures::SinkExt;
use serde::Serialize;
use serde_json::json;

use crate::error::{ApiError, ApiResult};
use crate::server::AppState;
use crate::types::*;

// ─── Helpers ───────────────────────────────────────────────────────────────

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

// ─── Identity ──────────────────────────────────────────────────────────────

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

// ─── Knowledge ─────────────────────────────────────────────────────────────

pub async fn encode_knowledge(
    State(state): State<AppState>,
    Json(body): Json<EncodeRequest>,
) -> ApiResult<serde_json::Value> {
    let _ = body.preview;
    let text = body.text.clone();
    let node_ref = state.node.clone();
    let broadcast_tx = state.event_broadcast.clone();

    // Send initial progress via broadcast (no lock needed)
    let _ = broadcast_tx.send(serde_json::to_string(&WsEvent {
        event_type: "encode_progress".to_string(),
        timestamp: now_epoch(),
        data: json!({ "step": 0, "total_steps": 6, "message": "Starting encode pipeline..." }),
    }).unwrap_or_default());

    // Run encode in a spawned task with a 300s timeout.
    let encode_future = async move {
        let mut node = node_ref.lock().await;
        node.encode_and_store_with_progress(&text, Some(&broadcast_tx)).await
    };

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(300),
        encode_future,
    )
    .await
    .map_err(|_| ApiError(onebrain_node::NodeError::Timeout(
        "Encode timed out after 300 seconds".to_string(),
    )))?
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
    // execute_kql currently uses plain text matching internally,
    // so pass the raw query directly (no KQL construct needed).
    let results = node.execute_kql(&body.query).map_err(ApiError::from)?;
    let limited: Vec<_> = results
        .into_iter()
        .take(body.limit.unwrap_or(10))
        .collect();
    Ok(ok(serde_json::to_value(limited).unwrap()))
}

pub async fn execute_kql(
    State(state): State<AppState>,
    Json(body): Json<KqlRequest>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let results = node.execute_kql(&body.query).map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(results).unwrap()))
}

// ─── Chat ──────────────────────────────────────────────────────────────────

pub async fn chat(
    State(state): State<AppState>,
    Json(body): Json<ChatRequest>,
) -> ApiResult<ChatResponse> {
    let mut node = state.node.lock().await;
    let text = node.process_input(&body.message).await.map_err(ApiError::from)?;
    Ok(ok(ChatResponse {
        text,
        intent: None,
        suggestions: vec![],
        kus_encoded: 0,
        kus_retrieved: 0,
    }))
}

// ─── Network ───────────────────────────────────────────────────────────────

pub async fn get_status(State(state): State<AppState>) -> ApiResult<StatusResponse> {
    let node = state.node.lock().await;
    let ku_count = node.ku_count().unwrap_or(0);
    let peer_count = node.peer_count();
    let uptime_s = state.start_time.elapsed().as_secs();
    let node_name = node.node_name().to_string();

    // Get balance for tier + obt
    let (tier, obt_balance) = match node.get_balance() {
        Ok(w) => (w.tier, w.balance),
        Err(_) => ("Unknown".to_string(), 0),
    };

    Ok(ok(StatusResponse {
        ku_count,
        peer_count,
        uptime_s,
        node_name,
        tier,
        obt_balance,
        version: env!("CARGO_PKG_VERSION").to_string(),
    }))
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
        ApiError(onebrain_node::NodeError::InvalidArgument(
            format!("Invalid socket address: {}", body.address),
        ))
    })?;
    let node = state.node.lock().await;
    node.connect_to_seed(addr).await.map_err(ApiError::from)?;
    Ok(ok(json!({ "connected": true, "address": body.address })))
}

// ─── Graph ─────────────────────────────────────────────────────────────────

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

// ─── Wallet ────────────────────────────────────────────────────────────────

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

// ─── Profile & Settings ────────────────────────────────────────────────────

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
        node.update_profile("language", lang).map_err(ApiError::from)?;
    }
    if let Some(style) = &body.response_style {
        node.update_profile("style", style).map_err(ApiError::from)?;
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
        node.update_config("ollama_url", url).map_err(ApiError::from)?;
    }
    if let Some(model) = &body.model {
        node.update_config("model", model).map_err(ApiError::from)?;
    }
    let config = node.get_config_view();
    Ok(ok(serde_json::to_value(config).unwrap()))
}

// ─── AI ────────────────────────────────────────────────────────────────────

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
    let mut node = state.node.lock().await;
    node.switch_model(&body.model_name).map_err(ApiError::from)?;
    Ok(ok(json!({ "model": body.model_name, "switched": true })))
}

// ─── Blobs ─────────────────────────────────────────────────────────────────

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

// ─── WebSocket ─────────────────────────────────────────────────────────────

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
        onebrain_node::NodeEvent::EncodeProgress { step, total_steps, message } => WsEvent {
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
