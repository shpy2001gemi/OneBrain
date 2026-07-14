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
            config,
            node: OnceLock::new(),
            api_port: OnceLock::new(),
            api_token: OnceLock::new(),
        }
    }
}
