//! Setup / first-run wizard helpers.
//!
//! The actual wizard UI lives in `onebrain-web`. These functions provide
//! the backend utilities that the Tauri commands call.

/// Get the OS hostname for pre-filling the node name.
pub fn get_hostname() -> String {
    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "OneBrain".to_string())
}

/// Check whether Ollama is reachable at the given URL.
pub async fn check_ollama(url: &str) -> bool {
    let check_url = format!("{}/api/tags", url.trim_end_matches('/'));
    reqwest::get(&check_url)
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Send a tiny prompt to load the model into GPU memory.
pub async fn warmup_model(ollama_url: &str, model: &str) {
    let url = format!("{}/api/generate", ollama_url.trim_end_matches('/'));
    let _ = reqwest::Client::new()
        .post(&url)
        .json(&serde_json::json!({
            "model": model,
            "prompt": "hello",
            "stream": false,
            "options": { "num_predict": 1 }
        }))
        .send()
        .await;
}
