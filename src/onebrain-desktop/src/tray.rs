//! System tray — icon, menu, and event handling.

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager,
};

/// Build and register the system tray icon with a context menu.
pub fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "Show OneBrain", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit OneBrain", true, None::<&str>)?;
    let network = MenuItem::with_id(
        app,
        "network",
        "Network status (local/partial)",
        true,
        None::<&str>,
    )?;
    let restart = MenuItem::with_id(app, "restart", "Restart OneBrain", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[&show, &separator, &network, &settings, &restart, &quit],
    )?;

    TrayIconBuilder::with_id("onebrain")
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "settings" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                    let _ = window.emit("navigate", "/settings");
                }
            }
            "network" => {
                let app_handle = app.clone();
                let state = app.state::<crate::state::AppState>();
                let node = state.node.get().cloned();
                let supervisor = state.supervisor.clone();
                tauri::async_runtime::spawn(async move {
                    let status = if supervisor.stopped() {
                        "Desktop stopped - restart required"
                    } else {
                        crate::supervisor::tray_status(node).await
                    };
                    if let Some(tray) = app_handle.tray_by_id("onebrain") {
                        let _ = tray.set_tooltip(Some(status));
                    }
                });
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                    let _ = window.emit("navigate", "/network");
                }
            }
            "quit" | "restart" => {
                let restart = event.id.as_ref() == "restart";
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    crate::commands::finish_exit(app, restart).await;
                });
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}
