//! Tauri IPC commands — invoked from the frontend via `invoke()`.

use crate::state::AppState;
use onebrain_node::OneBrainNode;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

// ─── API / Node Info ───────────────────────────────────────────────────────

/// Return the API base URL and bearer token so the frontend can call the
/// REST/WebSocket API directly.
#[tauri::command]
pub async fn get_api_config(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let port = state
        .api_port
        .get()
        .copied()
        .unwrap_or(state.config.api_port);
    let token = state.api_token.get().cloned().unwrap_or_default();

    Ok(json!({
        "baseUrl": format!("http://127.0.0.1:{}", port),
        "token": token,
        "ready": state.node.get().is_some(),
    }))
}

/// Return the node data directory as a display string.
#[tauri::command]
pub fn get_node_data_dir(state: State<'_, AppState>) -> String {
    state.config.data_dir.display().to_string()
}

/// Return the crate version.
#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Whether the first-run wizard should be shown.
#[tauri::command]
pub fn is_first_run(state: State<'_, AppState>) -> bool {
    !state.config.first_run_done
}

// ─── File Operations ───────────────────────────────────────────────────────

/// Open the data directory in the OS file manager.
#[tauri::command]
pub async fn open_data_dir(state: State<'_, AppState>) -> Result<(), String> {
    let path = &state.config.data_dir;
    open::that(path).map_err(|e| e.to_string())
}

/// Export a KU to a file (stub — full implementation depends on dialog).
#[tauri::command]
pub async fn export_ku_file(
    _state: State<'_, AppState>,
    _cid: String,
    _format: String,
) -> Result<String, String> {
    Ok("Export not yet implemented".to_string())
}

/// Import a knowledge file (stub — full implementation depends on dialog).
#[tauri::command]
pub async fn import_knowledge_file(
    _state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    Ok(json!({ "status": "Import not yet implemented" }))
}

// ─── Node Control ──────────────────────────────────────────────────────────

/// Fence and drain every node-owned network/runtime task before process exit.
pub(crate) async fn shutdown_node(node: Option<Arc<Mutex<OneBrainNode>>>) {
    if let Some(node) = node {
        node.lock().await.shutdown_network().await;
    }
}

/// Gracefully stop the node and restart the whole desktop process so
/// caller-owned vNext runtime dependencies are rebuilt safely.
#[tauri::command]
pub async fn restart_node(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    shutdown_node(state.node.get().cloned()).await;
    app.restart()
}

/// Gracefully quit the application.
#[tauri::command]
pub async fn quit_app(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    shutdown_node(state.node.get().cloned()).await;
    app.exit(0);
    Ok(())
}

// ─── First-Run Wizard ──────────────────────────────────────────────────────

/// Return sensible defaults for the wizard form.
#[tauri::command]
pub fn wizard_get_defaults() -> Result<serde_json::Value, String> {
    let config = crate::config::DesktopConfig::default();
    Ok(json!({
        "node_name": config.node_name,
        "data_dir": config.data_dir.display().to_string(),
        "ollama_url": config.ollama_url,
        "model": config.model,
    }))
}

/// Check whether Ollama is reachable.
#[tauri::command]
pub async fn wizard_check_ollama(url: String) -> Result<bool, String> {
    Ok(crate::setup::check_ollama(&url).await)
}

/// Persist the wizard results and mark first-run as complete.
#[tauri::command]
pub async fn wizard_complete(
    _state: State<'_, AppState>,
    node_name: String,
    data_dir: String,
    ollama_url: String,
    model: String,
) -> Result<(), String> {
    let mut config = crate::config::DesktopConfig::load().unwrap_or_default();

    config.node_name = node_name;
    config.data_dir = PathBuf::from(data_dir);
    config.ollama_url = ollama_url;
    config.model = model;
    config.first_run_done = true;

    config.save().map_err(|e| e.to_string())
}
