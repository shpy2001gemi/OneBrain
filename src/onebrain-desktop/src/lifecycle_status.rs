//! Bounded text for the Desktop lifecycle indicator.
pub fn describe(
    shutdown_issue: Option<&str>,
    startup_issue: Option<&str>,
    stopped: bool,
    ku_issue: Option<&str>,
    lifecycle_unavailable: bool,
    ready: bool,
) -> String {
    if let Some(reason) = shutdown_issue {
        format!("Desktop shutdown incomplete ({reason}); local API stopped accepting requests. Retry Quit or Restart")
    } else if let Some(reason) = startup_issue {
        format!("Local backend unavailable ({reason}); restart after correcting host inputs")
    } else if stopped {
        "Restart required after lifecycle change".into()
    } else if let Some(reason) = ku_issue {
        format!("Local KU dependency unavailable ({reason}); saved local reads remain available when the backend is ready. Restart after correcting host inputs")
    } else if lifecycle_unavailable {
        "Local-only mode; native lifecycle adapter unavailable, peer networking disabled".into()
    } else if ready {
        "Local API ready; network state is separate".into()
    } else {
        "Local backend starting".into()
    }
}
