use super::*;
use crate::server::ApiServer;
use axum::{
    body::{to_bytes, Body},
    http::Request as HttpRequest,
};
use tower::ServiceExt;

async fn call(
    router: axum::Router,
    path: &str,
    body: Option<String>,
    auth: Option<&str>,
    management: Option<&str>,
) -> (StatusCode, Value) {
    let mut request = HttpRequest::builder()
        .method(if body.is_some() { "POST" } else { "GET" })
        .uri(path);
    if let Some(token) = auth {
        request = request.header("Authorization", token);
    }
    if let Some(token) = management {
        request = request.header("X-OneBrain-OBP-Management", token);
    }
    if body.is_some() {
        request = request.header("Content-Type", "application/json");
    }
    let response = router
        .oneshot(request.body(Body::from(body.unwrap_or_default())).unwrap())
        .await
        .unwrap();
    assert_eq!(response.headers().get("cache-control").unwrap(), "no-store");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}
async fn node(dir: &tempfile::TempDir) -> onebrain_node::OneBrainNode {
    onebrain_node::OneBrainNode::new(onebrain_node::NodeConfig {
        port: 0,
        data_dir: dir.path().into(),
        concept_registry_mode: onebrain_node::ConceptRegistryMode::Disabled,
        ..Default::default()
    })
    .await
    .unwrap()
}
#[tokio::test]
async fn authentication_and_absent_runtime_do_not_create_an_owner() {
    let dir = tempfile::tempdir().unwrap();
    let server = ApiServer::new(node(&dir).await, "test".into(), 0);
    let router = server.build_router();
    for (token, status) in [
        (None, StatusCode::UNAUTHORIZED),
        (Some("Bearer wrong"), StatusCode::FORBIDDEN),
    ] {
        assert_eq!(
            call(router.clone(), "/api/vnext/obp/status", None, token, None)
                .await
                .0,
            status
        );
    }
    let (status, value) = call(
        router.clone(),
        "/api/vnext/obp/status",
        None,
        Some("Bearer test"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["data"]["available"], false);
    assert!(value["data"].get("session").is_none());
    let (bad, value) = call(
        router.clone(),
        "/api/vnext/obp/status?unexpected=1",
        None,
        Some("Bearer test"),
        None,
    )
    .await;
    assert_eq!(bad, StatusCode::BAD_REQUEST);
    assert_eq!(value["error"]["obp"]["reason"], "invalid_payload");
    for path in ["query", "commands", "reconcile", "ws/tickets"] {
        assert_eq!(
            call(
                router.clone(),
                &format!("/api/vnext/obp/{path}"),
                Some("{}".into()),
                Some("Bearer test"),
                None
            )
            .await
            .0,
            StatusCode::SERVICE_UNAVAILABLE
        );
    }
}

#[cfg(feature = "vnext-outbound-first")]
async fn active(
    dir: &tempfile::TempDir,
) -> (
    ApiServer,
    std::sync::Arc<tokio::sync::Mutex<onebrain_node::OneBrainNode>>,
    String,
) {
    use ku_core::foundation::{MetabolicViewPolicy, ObjectReference};
    let mut config = onebrain_node::NodeConfig {
        port: 0,
        data_dir: dir.path().into(),
        concept_registry_mode: onebrain_node::ConceptRegistryMode::Disabled,
        ..Default::default()
    };
    config.vnext.enabled.object_event_v1 = true;
    config.vnext.enabled.obp_rp = true;
    let mut node = onebrain_node::OneBrainNode::new(config).await.unwrap();
    let policies = onebrain_node::LocalPolicyRegistry::new([(
        onebrain_node::LocalPolicyVersion::new(1).unwrap(),
        MetabolicViewPolicy {
            policy_ref: ObjectReference::new(0, [3; 32]),
            accepted_evidence_policies: vec![ObjectReference::new(0, [4; 32])],
            recent_event_horizon: 64,
        },
    )])
    .unwrap();
    node.set_vnext_identity_signer(std::sync::Arc::new(ed25519_dalek::SigningKey::from_bytes(
        &[7; 32],
    )));
    node.set_vnext_product_dependencies(onebrain_node::VNextProductRuntimeDependencies::new(
        ku_kql::vnext_private_need::LocalNeedVaultKey::from_bytes([5; 32]),
        policies,
    ))
    .unwrap();
    node.start_network().await.unwrap();
    let host = node.obp_host().unwrap();
    let control = host.control([1; 32]).unwrap();
    let management = host
        .management(
            [1; 32],
            vec!["network_kill".into()],
            std::time::Duration::from_secs(300),
        )
        .unwrap();
    let shared = std::sync::Arc::new(tokio::sync::Mutex::new(node));
    let server = ApiServer::with_shared_node(shared.clone(), "test".into(), 0)
        .with_obp_binding(Binding::new([1; 32], Some(control)));
    (server, shared, management)
}
#[cfg(feature = "vnext-outbound-first")]
#[tokio::test]
async fn authenticated_http_calls_real_owner_and_preserves_replay_and_reconcile() {
    let dir = tempfile::tempdir().unwrap();
    let (server, shared, management) = active(&dir).await;
    let router = server.build_router();
    let (_, status) = call(
        router.clone(),
        "/api/vnext/obp/status",
        None,
        Some("Bearer test"),
        None,
    )
    .await;
    let session = status["data"]["session"].clone();
    let generation = status["data"]["status"]["generation"].clone();
    let body = json!({"session":session,"operation":"network_kill","payload":{"idempotency_key":"11".repeat(32),"expected_generation":generation}});
    assert_eq!(
        call(
            router.clone(),
            "/api/vnext/obp/commands",
            Some(body.to_string()),
            Some("Bearer test"),
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (status, first) = call(
        router.clone(),
        "/api/vnext/obp/commands",
        Some(body.to_string()),
        Some("Bearer test"),
        Some(&management),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["data"]["state"], "completed");
    assert_eq!(first["data"]["result"]["kill_switch"], true);
    let (_, again) = call(
        router.clone(),
        "/api/vnext/obp/commands",
        Some(body.to_string()),
        Some("Bearer test"),
        Some(&management),
    )
    .await;
    assert_eq!(first["data"], again["data"]);
    let (_, outcome) = call(
        router.clone(),
        "/api/vnext/obp/reconcile",
        Some(json!({"session":session,"idempotency_key":"11".repeat(32)}).to_string()),
        Some("Bearer test"),
        None,
    )
    .await;
    assert_eq!(outcome["data"]["state"], "completed");
    assert!(outcome["data"].get("result").is_none());
    let bad = body
        .to_string()
        .replacen("\"session\":", "\"authorized\":true,\"session\":", 1);
    assert_eq!(
        call(
            router.clone(),
            "/api/vnext/obp/commands",
            Some(bad),
            Some("Bearer test"),
            Some(&management)
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let duplicate = format!("{{\"session\":{},\"session\":{}}}", session, session);
    let mut invalid_session = body.clone();
    invalid_session["session"]["process_generation"] = "bad".into();
    assert_eq!(
        call(
            router.clone(),
            "/api/vnext/obp/commands",
            Some(invalid_session.to_string()),
            Some("Bearer test"),
            Some(&management)
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(
            router.clone(),
            "/api/vnext/obp/commands",
            Some(duplicate),
            Some("Bearer test"),
            Some(&management)
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let (_, ticket) = call(
        router.clone(),
        "/api/vnext/obp/ws/tickets",
        Some(json!({"session":session,"subscriptions":["network"]}).to_string()),
        Some("Bearer test"),
        None,
    )
    .await;
    assert!(ticket["data"]["ticket"]
        .as_str()
        .unwrap()
        .starts_with("obw1."));
    shared.lock().await.shutdown_network().await;
}

#[cfg(feature = "vnext-outbound-first")]
#[tokio::test]
async fn real_loopback_websocket_consumes_ticket_and_projects_only_network_hints() {
    use futures::StreamExt;
    let dir = tempfile::tempdir().unwrap();
    let (server, shared, _) = active(&dir).await;
    let router = server.build_router();
    let (_, status) = call(
        router.clone(),
        "/api/vnext/obp/status",
        None,
        Some("Bearer test"),
        None,
    )
    .await;
    let (_, ticket) = call(
        router.clone(),
        "/api/vnext/obp/ws/tickets",
        Some(json!({"session":status["data"]["session"],"subscriptions":["network"]}).to_string()),
        Some("Bearer test"),
        None,
    )
    .await;
    let ticket = ticket["data"]["ticket"].as_str().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let url = format!("ws://{address}/api/vnext/obp/ws?ticket={ticket}");
    assert!(
        tokio_tungstenite::connect_async(format!("{url}&ticket={ticket}"))
            .await
            .is_err()
    );
    assert!(tokio_tungstenite::connect_async(format!(
        "ws://{address}/api/vnext/ws?ticket={ticket}"
    ))
    .await
    .is_err());
    let (mut socket, response) = tokio_tungstenite::connect_async(&url).await.unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    for kind in ["subscription_ready", "network_state"] {
        let frame = tokio::time::timeout(std::time::Duration::from_secs(5), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let value: Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
        assert_eq!(value["profile"], "OBP_PRIVATE_WEBSOCKET_PROFILE_V1");
        assert_eq!(value["event_type"], kind);
        for secret in [
            "process_generation",
            "dataset_generation",
            "expected_peer",
            "idempotency_key",
            "checkpoint_digest",
            "input_ref",
        ] {
            assert!(!value.to_string().contains(secret));
        }
    }
    assert!(tokio_tungstenite::connect_async(&url).await.is_err());
    socket.close(None).await.unwrap();
    task.abort();
    let _ = task.await;
    shared.lock().await.shutdown_network().await;
}
