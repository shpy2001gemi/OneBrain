//! OneBrain Desktop — Tauri 2 application entry point.
//!
//! This module wires together:
//! - [`config`] — TOML-based desktop configuration
//! - [`state`]  — Tauri-managed `AppState` (config + node + API info)
//! - [`commands`] — IPC handlers invoked by the frontend
//! - [`events`] — background bridge that forwards `NodeEvent`s as Tauri events
//! - [`tray`] — system-tray icon and menu
//! - [`setup`] — first-run wizard backend helpers

mod commands;
mod config;
mod events;
mod setup;
mod state;
mod tray;

use config::DesktopConfig;
use onebrain_node::OneBrainNode;
use state::AppState;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tokio::sync::Mutex;

/// Generate a 32-character hex token for API authentication.
fn generate_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 16] = rng.gen();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Main entry point — builds and runs the Tauri application.
pub fn run() {
    // ── 1. Load or create default config (sync) ────────────────────────
    let config = DesktopConfig::load().unwrap_or_default();
    let node_config = config.to_node_config();
    std::fs::create_dir_all(&node_config.data_dir).ok();

    // ── 2. Build the Tauri app ─────────────────────────────────────────
    tauri::Builder::default()
        // ── Plugins ────────────────────────────────────────────────────
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Focus the existing window when a second instance is launched.
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(
            tauri_plugin_log::Builder::default()
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("onebrain".into()),
                    },
                ))
                .build(),
        )
        // ── Setup ──────────────────────────────────────────────────────
        .setup(move |app| {
            // Manage config-based state immediately so commands can
            // access it even before the async node init finishes.
            let app_state = AppState::new(config.clone());
            app.manage(app_state);

            // Set up the system tray.
            tray::setup_tray(app)?;

            // Spawn the async initialisation task.
            let handle = app.handle().clone();
            let cfg = config.clone();
            tauri::async_runtime::spawn(async move {
                // Create the node.
                let mut node = match OneBrainNode::new(node_config).await {
                    Ok(n) => n,
                    Err(e) => {
                        eprintln!("[onebrain-desktop] Failed to init node: {}", e);
                        return;
                    }
                };

                // Start P2P networking (best-effort — may fail if offline).
                match node.start_network().await {
                    Ok(addr) => {
                        println!("[onebrain-desktop] Network started on {}", addr);
                    }
                    Err(e) => {
                        eprintln!(
                            "[onebrain-desktop] Network start failed (continuing): {}",
                            e
                        );
                    }
                }

                // Check Ollama connectivity.
                let ollama_ok = setup::check_ollama(&cfg.ollama_url).await;
                if ollama_ok {
                    println!("[onebrain-desktop] Ollama reachable at {}", cfg.ollama_url);
                } else {
                    eprintln!(
                        "[onebrain-desktop] Ollama NOT reachable at {} — AI features disabled",
                        cfg.ollama_url
                    );
                }

                // Wrap in Arc<Mutex> for shared access.
                let shared = Arc::new(Mutex::new(node));
                let token = generate_token();
                let api_port = cfg.api_port;

                // Spawn the REST/WebSocket API server.
                let api_node = shared.clone();
                let api_token = token.clone();
                tokio::spawn(async move {
                    let server =
                        onebrain_api::ApiServer::with_shared_node(api_node, api_token, api_port);
                    if let Err(e) = server.start().await {
                        eprintln!("[onebrain-desktop] API server error: {}", e);
                    }
                });

                // Spawn the event bridge (NodeEvent → Tauri event).
                let event_handle = handle.clone();
                let event_node = shared.clone();
                tokio::spawn(async move {
                    events::run_event_bridge(event_handle, event_node).await;
                });

                // Publish node-related state to AppState's OnceLock fields.
                if let Some(state) = handle.try_state::<AppState>() {
                    let _ = state.node.set(shared);
                    let _ = state.api_port.set(api_port);
                    let _ = state.api_token.set(token);
                }

                // Notify the frontend that the backend is ready.
                let _ = handle.emit("backend-ready", ());

                println!("[onebrain-desktop] Initialisation complete");
            });

            Ok(())
        })
        // ── IPC Command Handlers ───────────────────────────────────────
        .invoke_handler(tauri::generate_handler![
            commands::get_api_config,
            commands::get_node_data_dir,
            commands::get_app_version,
            commands::is_first_run,
            commands::open_data_dir,
            commands::export_ku_file,
            commands::import_knowledge_file,
            commands::restart_node,
            commands::quit_app,
            commands::wizard_get_defaults,
            commands::wizard_check_ollama,
            commands::wizard_complete,
        ])
        // ── Window Close → Hide (close-to-tray) ───────────────────────
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Hide instead of closing so the tray keeps running.
                let _ = window.hide();
                api.prevent_close();
            }
        })
        // ── Run ────────────────────────────────────────────────────────
        .run(tauri::generate_context!())
        .expect("error while running OneBrain Desktop");
}
