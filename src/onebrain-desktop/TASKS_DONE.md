# onebrain-desktop — Tasks Completed

## Summary
All files for the `onebrain-desktop` Tauri 2.x crate have been created and the workspace
has been updated to include it as a member.

---

## Completed Tasks

### T1: Workspace Member ✅
- **File:** `src/Cargo.toml`
- Added `"onebrain-desktop"` to workspace members under "Interface Projects (Rust)".
- Removed it from the "Non-Rust" comment block.

### T2: Crate Cargo.toml ✅
- **File:** `src/onebrain-desktop/Cargo.toml`
- Dependencies: `onebrain-node`, `onebrain-api`, Tauri 2 + 7 plugins, workspace deps,
  `toml`, `dirs`, `reqwest`, `hostname`, `open`.

### T3: Build Script ✅
- **File:** `src/onebrain-desktop/build.rs`
- Calls `tauri_build::build()`.

### T4: Tauri Configuration ✅
- **File:** `src/onebrain-desktop/tauri.conf.json`
- `productName: "OneBrain"`, `identifier: "live.onebrain.desktop"`.
- Dev server: `http://localhost:5173` (via `onebrain-web`).
- Window: 1280×800, min 900×600, centered, resizable.
- Tray icon, CSP, bundle config with WiX.

### T5: Capabilities ✅
- **File:** `src/onebrain-desktop/capabilities/default.json`
- Permissions: core:default, window close/minimize/set-title/show/hide/set-focus,
  dialog open/save, notification, shell:allow-open, store, window-state.

### T7: Config Module ✅
- **File:** `src/onebrain-desktop/src/config.rs`
- `DesktopConfig` with TOML serialization.
- `config_dir()`, `config_path()`, `default_data_dir()`, `load()`, `save()`.
- `Default` impl uses OS hostname, standard dirs, port 4242 / api_port 4280.
- `to_node_config()` converts to `onebrain_node::NodeConfig`.

### T8: State Module ✅
- **File:** `src/onebrain-desktop/src/state.rs`
- `AppState` with `config: DesktopConfig` (always available) and
  `OnceLock` fields for `node`, `api_port`, `api_token` (set after async init).

### T9: Lib Module (Main Setup) ✅
- **File:** `src/onebrain-desktop/src/lib.rs`
- `pub fn run()`: loads config, builds Tauri app, registers 7 plugins.
- `.setup()`: manages `AppState` immediately, spawns async task for:
  - `OneBrainNode::new()` + `start_network()`
  - Ollama connectivity check
  - API server spawn (`ApiServer::with_shared_node`)
  - Event bridge spawn
  - OnceLock population + `backend-ready` event
- Close-to-tray via `on_window_event`.
- Single-instance plugin focuses existing window.

### T10: Main Entry Point ✅
- **File:** `src/onebrain-desktop/src/main.rs`
- `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`
- Calls `onebrain_desktop::run()`.

### T12: IPC Commands ✅
- **File:** `src/onebrain-desktop/src/commands.rs`
- 11 commands: `get_api_config`, `get_node_data_dir`, `get_app_version`,
  `is_first_run`, `open_data_dir`, `export_ku_file`, `import_knowledge_file`,
  `restart_node`, `quit_app`, `wizard_get_defaults`, `wizard_check_ollama`,
  `wizard_complete`.
- Uses `State<'_, AppState>` with OnceLock-aware access.

### T14: Event Bridge ✅
- **File:** `src/onebrain-desktop/src/events.rs`
- Matches **actual** `NodeEvent` variants: `PeerConnected(PeerInfo)`,
  `KuReceived`, `VerifyResult`, `Notification`, `EncodeProgress`.
- Polls every 300ms, emits `"node-event"` Tauri events with JSON payload.

### T15: System Tray ✅
- **File:** `src/onebrain-desktop/src/tray.rs`
- Menu: Show / Settings / Quit.
- Double-click tray icon shows window.
- Settings emits `"navigate"` event to frontend.

### T24: Setup Helpers ✅
- **File:** `src/onebrain-desktop/src/setup.rs`
- `get_hostname()`, `check_ollama()`, `warmup_model()`.

---

## Key Design Decisions

1. **OnceLock pattern** — `AppState.config` is available immediately; node-related
   fields use `std::sync::OnceLock` so async init can set them without blocking the
   Tauri event loop.

2. **Actual NodeEvent matching** — The event bridge matches the *real* `NodeEvent`
   variants from `onebrain-node` (not the hypothetical variants from the spec).

3. **Close-to-tray** — Window close is intercepted and hidden; the app continues
   running in the system tray until "Quit OneBrain" is selected.

4. **Backend-ready event** — After async init completes, `"backend-ready"` is emitted
   so the frontend knows the API is available.

---

## File Tree
```
onebrain-desktop/
├── build.rs
├── Cargo.toml
├── tauri.conf.json
├── capabilities/
│   └── default.json
└── src/
    ├── main.rs
    ├── lib.rs
    ├── config.rs
    ├── state.rs
    ├── commands.rs
    ├── events.rs
    ├── tray.rs
    └── setup.rs
```
