//! Setup / first-run wizard helpers.
//!
//! The actual wizard UI lives in `onebrain-web`. These functions provide
//! the backend utilities that the Tauri commands call.

/// Check whether Ollama is reachable at the given URL.
pub async fn check_ollama(url: &str) -> bool {
    let check_url = format!("{}/api/tags", url.trim_end_matches('/'));
    reqwest::get(&check_url)
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}
