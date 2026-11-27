use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use onebrain_api::ApiServer;
use onebrain_node::{NodeConfig, OneBrainNode};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn call_json(
    router: &axum::Router,
    method: &str,
    uri: &str,
    bearer: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .header(header::CONTENT_TYPE, "application/json");
    let raw = match body {
        Some(v) => Body::from(serde_json::to_vec(&v).unwrap()),
        None => Body::empty(),
    };
    let resp = router
        .clone()
        .oneshot(builder.body(raw).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let parsed = serde_json::from_slice::<Value>(&bytes).unwrap_or_else(|_| json!({}));
    (status, parsed)
}

#[tokio::test]
async fn p10_personal_mesh_spake2_revocation_need_compiler_and_surface_ceilings_work() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = NodeConfig {
        data_dir: dir.path().to_path_buf(),
        ..Default::default()
    };
    let node = OneBrainNode::new(cfg).await.unwrap();
    let host_token = "test-host-owner-token-1234567890abcdef".to_owned();
    let router = ApiServer::new(node, host_token.clone(), 0).build_router();

    // 1. Personal Mesh status -> pair/start -> SAS confirm -> revoke (mesh_epoch 1 -> 2)
    let (st, status_json) =
        call_json(&router, "GET", "/api/vnext/mesh/status", &host_token, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(status_json["data"]["mesh_epoch"], 1);
    assert_eq!(
        status_json["data"]["principals"]["private_signing_keys_shared"],
        false
    );

    let (st, pair_json) = call_json(
        &router,
        "POST",
        "/api/vnext/mesh/pair/start",
        &host_token,
        Some(json!({ "device_label": "Pixel 9 Mesh", "surface_kind": "mobile" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let sid = pair_json["data"]["session_id"].as_str().unwrap().to_owned();
    let sas = pair_json["data"]["sas_digits"].as_str().unwrap().to_owned();
    let new_dev_id = pair_json["data"]["candidate_device_id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Unconfirmed SAS rejected fail-closed
    let (st, _) = call_json(
        &router,
        "POST",
        "/api/vnext/mesh/pair/confirm",
        &host_token,
        Some(json!({ "session_id": sid, "sas_confirmed": false })),
    )
    .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);

    let (st, confirm_json) = call_json(
        &router,
        "POST",
        "/api/vnext/mesh/pair/confirm",
        &host_token,
        Some(json!({ "session_id": sid, "sas_confirmed": true, "sas_digits": sas })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(confirm_json["data"]["private_keys_transmitted"], false);

    let (st, rev_json) = call_json(
        &router,
        "POST",
        "/api/vnext/mesh/revoke",
        &host_token,
        Some(json!({ "device_id": new_dev_id, "reason": "lost" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(rev_json["data"]["new_mesh_epoch"], 2);
    assert_eq!(rev_json["data"]["epoch_dek_rotated"], true);

    // 2. KQL-CMP-001 NeedIR compile rejects non-semantic fields with 422
    let (st, err_json) = call_json(
        &router,
        "POST",
        "/api/vnext/kql/needs/compile",
        &host_token,
        Some(json!({ "local_query": "FIND (k:KU) WHERE k.obt_balance > 10" })),
    )
    .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        err_json["error"]["code"],
        "non_semantic_field_forbidden_in_need_ir"
    );

    let (st, ok_cmp) = call_json(
        &router,
        "POST",
        "/api/vnext/kql/needs/compile",
        &host_token,
        Some(json!({ "local_query": "FIND (?x)-[:boils_at]->(?t)" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(ok_cmp["data"]["compiler_profile"], "kql-need-compiler/1");

    // 3. OBT status reports simulated_non_economic
    let (st, obt_json) =
        call_json(&router, "GET", "/api/vnext/obt/status", &host_token, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(
        obt_json["data"]["economic_status"],
        "simulated_non_economic"
    );

    // 4. Mint obc_surface1 tokens for Extension, Bot, Glasses & enforce FW-10 / NEG-P10-002 / NEG-P10-003
    let (st, ext_mint) = call_json(
        &router,
        "POST",
        "/api/vnext/surfaces/tokens",
        &host_token,
        Some(json!({
            "surface_kind": "extension",
            "label": "Chrome MV3",
            "bound_channel_or_origin": "chrome-extension://onebrain"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let ext_token = ext_mint["data"]["issued_capability_handle"]
        .as_str()
        .unwrap()
        .to_owned();

    // Extension can capture sanitized draft, CANNOT read /api/kus, CANNOT mint tokens, CANNOT confirm PUBLIC
    let (st, cap_json) = call_json(
        &router,
        "POST",
        "/api/vnext/extension/capture",
        &ext_token,
        Some(json!({ "sanitized_url": "https://example.org/article", "text": "Clean excerpt" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(cap_json["data"]["step2_public_confirm_allowed"], false);

    let (st, _) = call_json(&router, "GET", "/api/kus", &ext_token, None).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _) = call_json(
        &router,
        "GET",
        "/api/vnext/surfaces/tokens",
        &ext_token,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _) = call_json(
        &router,
        "POST",
        "/api/vnext/hud/session/heartbeat",
        &ext_token,
        Some(json!({ "monotonic_ms": 10, "wear_contact_active": true })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // Bot token can ONLY access /api/vnext/bot/query & /api/vnext/bot/submissions
    let (_, bot_mint) = call_json(
        &router,
        "POST",
        "/api/vnext/surfaces/tokens",
        &host_token,
        Some(json!({
            "surface_kind": "bot",
            "label": "Discord Bot",
            "bound_channel_or_origin": "discord:guild-1"
        })),
    )
    .await;
    let bot_token = bot_mint["data"]["issued_capability_handle"]
        .as_str()
        .unwrap()
        .to_owned();
    let (st, sub_json) = call_json(
        &router,
        "POST",
        "/api/vnext/bot/submissions",
        &bot_token,
        Some(json!({ "submission_digest_hex": "aa".repeat(32), "sanitized_text": "Hello" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(sub_json["is_executable"], false);

    let (st, _) = call_json(&router, "GET", "/api/vnext/mesh/status", &bot_token, None).await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // Glasses token enforces <=40ms doff-lock & blocks voice-only step-2 confirm
    let (_, hud_mint) = call_json(
        &router,
        "POST",
        "/api/vnext/surfaces/tokens",
        &host_token,
        Some(json!({
            "surface_kind": "glasses",
            "label": "AR Glasses",
            "bound_channel_or_origin": "obp/companion-hud/1"
        })),
    )
    .await;
    let hud_token = hud_mint["data"]["issued_capability_handle"]
        .as_str()
        .unwrap()
        .to_owned();
    let (st, _) = call_json(
        &router,
        "POST",
        "/api/vnext/hud/confirm/commit",
        &hud_token,
        Some(json!({
            "modality": "voice_only",
            "prepared_receipt_digest": "bb".repeat(32)
        })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
}
