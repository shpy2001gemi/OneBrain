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
pub mod config;
mod events;
mod local_listener;
mod platform;
mod recovery;
mod setup;
mod state;
pub mod supervisor;
mod tray;

use config::DesktopConfig;
use onebrain_node::OneBrainNode;
use state::AppState;
use std::future::Future;
use std::sync::Arc;
use tauri::{Emitter, Manager};

/// Generate a 256-bit random hex token for API authentication.
fn generate_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.gen();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Main entry point — builds and runs the Tauri application.
pub fn run() {
    run_host(true, |config, supervisor| async move {
        let mut node = OneBrainNode::new(config.to_node_config())
            .await
            .map_err(|_| "desktop_node_init_failed")?;
        if config.auto_start && !supervisor.stopped() {
            node.start_network()
                .await
                .map_err(|_| "desktop_network_start_failed")?;
        }
        Ok(supervisor::HostNode::local(node))
    });
}

/// Trusted embedding port. A host supplies custody, policy and explicit execution
/// grants here, using the same node. The packaged default installs no OBP grants.
pub fn run_with_host<F, Fut>(host: F)
where
    F: FnOnce(DesktopConfig, Arc<supervisor::Supervisor>) -> Fut + Send + 'static,
    Fut: Future<Output = Result<supervisor::HostNode, &'static str>> + Send + 'static,
{
    run_host(false, host);
}

fn run_host<F, Fut>(local_fallback: bool, host: F)
where
    F: FnOnce(DesktopConfig, Arc<supervisor::Supervisor>) -> Fut + Send + 'static,
    Fut: Future<Output = Result<supervisor::HostNode, &'static str>> + Send + 'static,
{
    let config = DesktopConfig::load().unwrap_or_default();
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

            let state = app.state::<AppState>();
            let supervisor = state.supervisor.clone();
            let mut cfg = config.clone();
            match platform::NativeEvents::register(supervisor.clone(), app.handle().clone()) {
                Ok(events) => {
                    let _ = state.native_events.set(events);
                }
                Err(reason) => {
                    if local_fallback {
                        // The stock host has no vNext dependencies; preserve local
                        // usefulness while refusing legacy automatic networking.
                        cfg.auto_start = false;
                        state
                            .lifecycle_unavailable
                            .store(true, std::sync::atomic::Ordering::SeqCst);
                    } else {
                        supervisor.fence();
                    }
                    tracing::error!(reason);
                }
            }
            let handle = app.handle().clone();
            let startup = tauri::async_runtime::spawn(async move {
                if supervisor.stopped() {
                    return;
                }
                let boot = match host(cfg.clone(), supervisor.clone()).await {
                    Ok(boot) => boot,
                    Err(reason) => {
                        supervisor.fence();
                        tracing::error!(reason);
                        return;
                    }
                };
                let token = generate_token();
                let (node, port) = match supervisor.start(boot, token.clone(), cfg.api_port).await {
                    Ok(ready) => ready,
                    Err(reason) => {
                        supervisor.fence();
                        tracing::error!(reason);
                        return;
                    }
                };
                let task = tokio::spawn(events::run_event_bridge(handle.clone(), node.clone()));
                supervisor.own_auxiliary(task).await;
                let state = handle.state::<AppState>();
                let _ = state.node.set(node);
                let _ = state.api_port.set(port);
                let _ = state.api_token.set(token);
                if supervisor.ready() {
                    let _ = handle.emit("backend-ready", ());
                }
            });
            *state.startup.lock().unwrap() = Some(startup);

            Ok(())
        })
        // ── IPC Command Handlers ───────────────────────────────────────
        .invoke_handler(tauri::generate_handler![
            commands::get_api_config,
            commands::desktop_recovery_load,
            commands::desktop_recovery_save,
            commands::desktop_recovery_clear,
            commands::desktop_lifecycle_status,
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
        .plugin(
            tauri::plugin::Builder::<tauri::Wry, ()>::new("local-navigation")
                .on_navigation(|_webview, url| {
                    matches!(
                        (url.scheme(), url.host_str()),
                        ("tauri", Some("localhost")) | ("http", Some("tauri.localhost"))
                    ) || (cfg!(debug_assertions)
                        && url.scheme() == "http"
                        && url.host_str() == Some("localhost")
                        && url.port() == Some(5173))
                })
                .build(),
        )
        .build(tauri::generate_context!())
        .expect("error while building OneBrain Desktop")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                let state = app.state::<AppState>();
                if !state.exit_started.load(std::sync::atomic::Ordering::SeqCst) {
                    api.prevent_exit();
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        commands::finish_exit(app, false).await;
                    });
                }
            }
        });
}
