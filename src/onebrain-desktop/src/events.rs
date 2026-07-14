//! Event bridge — drains [`NodeEvent`]s from the node and emits Tauri events.
//!
//! The frontend listens to `"node-event"` via `listen()` to update the UI
//! in real time (encode progress, peer connections, etc.).

use onebrain_node::{NodeEvent, OneBrainNode};
use serde_json::json;
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::Mutex;

/// Run the event bridge loop.
///
/// Polls the node's event channel every 300 ms, converts each
/// [`NodeEvent`] to a JSON payload, and emits it as a Tauri event
/// named `"node-event"`.
pub async fn run_event_bridge(app: tauri::AppHandle, node: Arc<Mutex<OneBrainNode>>) {
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        // Try to acquire the lock without blocking other tasks.
        let events = match node.try_lock() {
            Ok(mut n) => n.drain_events(),
            Err(_) => continue,
        };

        for event in events {
            let (event_type, data) = match &event {
                NodeEvent::PeerConnected(peer) => (
                    "peer_connected",
                    json!({
                        "name": peer.name,
                        "addr": peer.addr.to_string(),
                        "ku_count": peer.ku_count,
                    }),
                ),
                NodeEvent::KuReceived {
                    cid_hex,
                    source_text,
                    from,
                    ..
                } => (
                    "ku_received",
                    json!({
                        "cid": cid_hex,
                        "source_text": source_text,
                        "from_peer": from,
                    }),
                ),
                NodeEvent::VerifyResult {
                    cid_hex,
                    agreement_score,
                    verified,
                    from,
                } => (
                    "verify_result",
                    json!({
                        "cid": cid_hex,
                        "agreement_score": agreement_score,
                        "verified": verified,
                        "from": from,
                    }),
                ),
                NodeEvent::Notification(msg) => ("notification", json!({ "message": msg })),
                NodeEvent::EncodeProgress {
                    step,
                    total_steps,
                    message,
                } => (
                    "encode_progress",
                    json!({
                        "step": step,
                        "total_steps": total_steps,
                        "message": message,
                    }),
                ),
            };

            let ws_event = json!({
                "event_type": event_type,
                "data": data,
                "timestamp": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            });

            let _ = app.emit("node-event", &ws_event);
        }
    }
}
