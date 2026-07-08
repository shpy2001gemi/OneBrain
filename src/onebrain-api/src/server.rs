//! API server setup and routing.
//!
//! Configures the axum 0.8 router with all REST routes,
//! WebSocket endpoint, CORS, and Bearer-token auth middleware.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::Request;
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use tokio::sync::broadcast;
use axum::Json;
use axum::Router;
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tracing;

use onebrain_node::OneBrainNode;

use crate::handlers;
use crate::types::ApiErrorResponse;

// ─── App State ─────────────────────────────────────────────────────────────

/// Shared application state passed to all handlers.
#[derive(Clone)]
pub struct AppState {
    pub node: Arc<Mutex<OneBrainNode>>,
    pub api_token: String,
    pub start_time: Instant,
    pub web_dir: Option<PathBuf>,
    /// Broadcast channel for sending events to WS clients without holding the node lock.
    pub event_broadcast: broadcast::Sender<String>,
}

// ─── API Server ────────────────────────────────────────────────────────────

/// The headless API server.
pub struct ApiServer {
    state: AppState,
    port: u16,
}

impl ApiServer {
    /// Create a new server, wrapping the node in an `Arc<Mutex<_>>`.
    pub fn new(node: OneBrainNode, api_token: String, port: u16) -> Self {
        let (event_broadcast, _) = broadcast::channel(256);
        Self {
            state: AppState {
                node: Arc::new(Mutex::new(node)),
                api_token,
                start_time: Instant::now(),
                web_dir: None,
                event_broadcast,
            },
            port,
        }
    }

    /// Create from an already-shared node reference.
    pub fn with_shared_node(
        node: Arc<Mutex<OneBrainNode>>,
        api_token: String,
        port: u16,
    ) -> Self {
        let (event_broadcast, _) = broadcast::channel(256);
        Self {
            state: AppState {
                node,
                api_token,
                start_time: Instant::now(),
                web_dir: None,
                event_broadcast,
            },
            port,
        }
    }

    /// Set the directory containing built web dashboard files.
    /// If set, the server will serve the web dashboard at `/`.
    pub fn with_web_dir(mut self, path: PathBuf) -> Self {
        self.state.web_dir = Some(path);
        self
    }

    /// Start the server (blocks until shutdown).
    pub async fn start(self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = format!("127.0.0.1:{}", self.port);

        // Spawn background task: drain node events → broadcast to WS clients
        let drain_node = self.state.node.clone();
        let drain_tx = self.state.event_broadcast.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
                // Try to acquire lock briefly — skip if busy (encode running)
                let events = match drain_node.try_lock() {
                    Ok(mut node) => node.drain_events(),
                    Err(_) => continue, // Lock held by encode, skip this tick
                };
                for event in events {
                    let ws_event = crate::handlers::node_event_to_ws_pub(&event);
                    if let Ok(json) = serde_json::to_string(&ws_event) {
                        let _ = drain_tx.send(json);
                    }
                }
            }
        });

        let router = self.build_router();
        tracing::info!("OneBrain API listening on http://{}", addr);

        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, router).await?;
        Ok(())
    }

    /// Build the axum `Router` with all routes and middleware.
    pub fn build_router(self) -> Router {
        // CORS — allow localhost origins
        let cors = CorsLayer::new()
            .allow_origin([
                "http://localhost:3000".parse::<HeaderValue>().unwrap(),
                "http://localhost:5173".parse::<HeaderValue>().unwrap(),
                "http://127.0.0.1:3000".parse::<HeaderValue>().unwrap(),
                "http://127.0.0.1:5173".parse::<HeaderValue>().unwrap(),
                "http://localhost:8080".parse::<HeaderValue>().unwrap(),
                "http://127.0.0.1:8080".parse::<HeaderValue>().unwrap(),
            ])
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PATCH,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers([
                header::CONTENT_TYPE,
                header::AUTHORIZATION,
                header::ACCEPT,
            ])
            .allow_credentials(true);

        // Build route tree
        let api_routes = Router::new()
            // Identity
            .route("/api/identity", get(handlers::get_identity))
            .route("/api/identity/recover", post(handlers::recover_identity))
            // Knowledge
            .route("/api/encode", post(handlers::encode_knowledge))
            .route("/api/kus", get(handlers::list_kus))
            .route("/api/kus/{cid}", get(handlers::get_ku))
            .route("/api/kus/{cid}", delete(handlers::delete_ku))
            .route("/api/search", post(handlers::search_knowledge))
            .route("/api/kql", post(handlers::execute_kql))
            // Chat
            .route("/api/chat", post(handlers::chat))
            // Network
            .route("/api/status", get(handlers::get_status))
            .route("/api/peers", get(handlers::get_peers))
            .route("/api/peers/connect", post(handlers::connect_peer))
            // Graph
            .route("/api/graph/{cid}", get(handlers::get_graph))
            .route("/api/graph/{cid}/neighbors", get(handlers::get_neighbors))
            // Wallet
            .route("/api/wallet", get(handlers::get_wallet))
            .route("/api/wallet/history", get(handlers::get_wallet_history))
            // Profile & Settings
            .route("/api/profile", get(handlers::get_profile))
            .route("/api/profile", patch(handlers::update_profile))
            .route("/api/settings", get(handlers::get_settings))
            .route("/api/settings", patch(handlers::update_settings))
            // Blobs
            .route("/api/blobs", get(handlers::list_blobs))
            .route("/api/blobs/{cid}", get(handlers::get_blob_meta))
            .route("/api/blobs/{cid}", delete(handlers::delete_blob))
            .route("/api/blobs/stats", get(handlers::blob_stats))
            .route("/api/blobs/gc", post(handlers::blob_gc))
            // AI
            .route("/api/ai/status", get(handlers::ai_status))
            .route("/api/ai/models", get(handlers::list_ai_models))
            .route("/api/ai/model", post(handlers::switch_model));

        // WebSocket (no auth middleware)
        let ws_routes = Router::new()
            .route("/ws/events", get(handlers::ws_events));

        let mut router = Router::new()
            .merge(api_routes)
            .layer(middleware::from_fn_with_state(
                self.state.clone(),
                auth_middleware,
            ))
            .merge(ws_routes)
            .layer(cors);

        // Serve built web dashboard if web_dir is configured
        if let Some(ref web_dir) = self.state.web_dir {
            let index_file = web_dir.join("index.html");
            if web_dir.exists() && index_file.exists() {
                // Serve static files from the web directory
                let serve_dir = ServeDir::new(web_dir.clone());
                router = router.nest_service("/assets", ServeDir::new(web_dir.join("assets")));

                // Serve known static files at root
                let web_dir_clone = web_dir.clone();
                router = router.fallback(move |req: Request| {
                    let web_dir = web_dir_clone.clone();
                    async move {
                        let path = req.uri().path();
                        // Try to serve the exact file first (favicon.svg, icons.svg, etc.)
                        let file_path = web_dir.join(path.trim_start_matches('/'));
                        if file_path.is_file() {
                            match tokio::fs::read(&file_path).await {
                                Ok(content) => {
                                    let mime = mime_from_path(path);
                                    Response::builder()
                                        .status(StatusCode::OK)
                                        .header("Content-Type", mime)
                                        .body(axum::body::Body::from(content))
                                        .unwrap()
                                }
                                Err(_) => serve_index(&web_dir).await,
                            }
                        } else {
                            // SPA fallback: serve index.html for all other routes
                            serve_index(&web_dir).await
                        }
                    }
                });
                tracing::info!("Serving web dashboard from {}", web_dir.display());
            } else {
                tracing::warn!("Web directory not found: {}", web_dir.display());
            }
        }

        router.with_state(self.state)
    }
}

// ─── Auth Middleware ───────────────────────────────────────────────────────

/// Bearer-token authentication middleware.
///
/// Skips authentication for WebSocket paths (`/ws/`).
async fn auth_middleware(
    axum::extract::State(state): axum::extract::State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path();
    // Skip auth for WebSocket paths and static web files
    if path.starts_with("/ws/") || !path.starts_with("/api/") {
        return next.run(req).await;
    }

    // Extract and validate Bearer token
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    match auth_header {
        Some(value) if value.starts_with("Bearer ") => {
            let token = &value[7..];
            if constant_time_eq(token.as_bytes(), state.api_token.as_bytes()) {
                next.run(req).await
            } else {
                let body = ApiErrorResponse::new("AUTH_INVALID_TOKEN", "Invalid API token");
                (StatusCode::FORBIDDEN, Json(body)).into_response()
            }
        }
        _ => {
            let body = ApiErrorResponse::new(
                "AUTH_REQUIRED",
                "Missing or malformed Authorization header",
            );
            (StatusCode::UNAUTHORIZED, Json(body)).into_response()
        }
    }
}

/// Constant-time byte comparison to prevent timing attacks.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// Serve the SPA index.html file.
async fn serve_index(web_dir: &std::path::Path) -> Response {
    let index_path = web_dir.join("index.html");
    match tokio::fs::read(&index_path).await {
        Ok(content) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/html; charset=utf-8")
            .body(axum::body::Body::from(content))
            .unwrap(),
        Err(_) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(axum::body::Body::from("index.html not found"))
            .unwrap(),
    }
}

/// Simple MIME type lookup from file extension.
fn mime_from_path(path: &str) -> &'static str {
    if path.ends_with(".html") { "text/html; charset=utf-8" }
    else if path.ends_with(".js") { "application/javascript" }
    else if path.ends_with(".css") { "text/css" }
    else if path.ends_with(".svg") { "image/svg+xml" }
    else if path.ends_with(".png") { "image/png" }
    else if path.ends_with(".ico") { "image/x-icon" }
    else if path.ends_with(".json") { "application/json" }
    else if path.ends_with(".woff2") { "font/woff2" }
    else if path.ends_with(".woff") { "font/woff" }
    else { "application/octet-stream" }
}
