#![forbid(unsafe_code)]
//! Lightweight `onebrain-glasses` companion HUD inspector CLI.

use onebrain_glasses::{
    COMPANION_HUD_PROTOCOL_V1, HEARTBEAT_INTERVAL_MS, MAX_CONFIRM_WINDOW_MS,
    MAX_DOFF_LOCK_LATENCY_MS, MIN_GESTURE_DWELL_MS, SOMATIC_IMMEDIATE_PROFILE_V1,
};

fn main() {
    let status = serde_json::json!({
        "surface_kind": "glasses",
        "protocol": COMPANION_HUD_PROTOCOL_V1,
        "somatic_freshness_profile": SOMATIC_IMMEDIATE_PROFILE_V1,
        "heartbeat_interval_ms": HEARTBEAT_INTERVAL_MS,
        "max_doff_lock_latency_ms": MAX_DOFF_LOCK_LATENCY_MS,
        "min_gesture_dwell_ms": MIN_GESTURE_DWELL_MS,
        "max_confirm_window_ms": MAX_CONFIRM_WINDOW_MS,
        "at_rest_master_secrets": 0,
        "epistemic_badge": "[Private · Local Only]",
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&status).expect("valid static status JSON")
    );
}
