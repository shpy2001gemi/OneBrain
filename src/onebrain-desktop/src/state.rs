//! Application state managed by Tauri.
//!
//! [`AppState`] is registered via `app.manage()` and available to all
//! `#[tauri::command]` handlers through `State<'_, AppState>`.

use crate::config::DesktopConfig;
use onebrain_node::OneBrainNode;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;

/// Tauri-managed application state.
///
/// `config` is available immediately (sync init).
/// Node-related fields are populated asynchronously via [`OnceLock`] after
/// the background setup task completes — commands should check `.get()`
/// before using them.
pub struct AppState {
    pub supervisor: Arc<crate::supervisor::Supervisor>,
    pub startup: std::sync::Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    pub native_events: OnceLock<crate::platform::NativeEvents>,
    pub exit_started: std::sync::atomic::AtomicBool,
    pub lifecycle_unavailable: std::sync::atomic::AtomicBool,
    pub startup_issue: OnceLock<&'static str>,
    pub recovery_lock: Mutex<()>,
    /// Desktop configuration (always available).
    pub config: DesktopConfig,
    /// The shared OneBrain node instance (set after async init).
    pub node: OnceLock<Arc<Mutex<OneBrainNode>>>,
    /// REST/WebSocket API port (set after async init).
    pub api_port: OnceLock<u16>,
    /// API bearer token (set after async init).
    pub api_token: OnceLock<String>,
}

impl AppState {
    /// Create a new state with only the config populated.
    /// Node fields are left empty and set later via [`OnceLock::set`].
    pub fn new(config: DesktopConfig) -> Self {
        Self {
            supervisor: Arc::new(crate::supervisor::Supervisor::default()),
            startup: std::sync::Mutex::new(None),
            native_events: OnceLock::new(),
            exit_started: std::sync::atomic::AtomicBool::new(false),
            lifecycle_unavailable: std::sync::atomic::AtomicBool::new(false),
            startup_issue: OnceLock::new(),
            recovery_lock: Mutex::new(()),
            config,
            node: OnceLock::new(),
            api_port: OnceLock::new(),
            api_token: OnceLock::new(),
        }
    }
}
