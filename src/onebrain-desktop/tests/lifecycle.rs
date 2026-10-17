// Import the product modules without loading a native WebView test harness.
#[path = "../src/config.rs"]
mod config;
#[path = "../src/ku_start.rs"]
mod ku_start;
#[path = "../src/lifecycle_status.rs"]
mod lifecycle_status;
#[path = "../src/local_listener.rs"]
mod local_listener;
#[path = "../src/recovery.rs"]
mod recovery;
#[path = "../../onebrain-api/src/ku_api/tests/registry_fixture.rs"]
mod registry_fixture;
#[path = "../src/supervisor.rs"]
mod supervisor;
use onebrain_node::{ConceptRegistryMode, NodeConfig, OneBrainNode};
use serde_json::{json, Value};
use std::{path::Path, time::Duration};
use supervisor::{HostNode, Supervisor};

#[test]
fn lifecycle_status_reports_shutdown_over_degraded_ku_and_preserves_startup_failure() {
    let degraded = lifecycle_status::describe(
        None,
        None,
        false,
        Some("ku_registry_unavailable"),
        false,
        true,
    );
    assert!(degraded.contains("ku_registry_unavailable"));
    let stopped = lifecycle_status::describe(
        None,
        None,
        true,
        Some("ku_registry_unavailable"),
        false,
        false,
    );
    assert_eq!(stopped, "Restart required after lifecycle change");
    let failed = lifecycle_status::describe(
        Some("desktop_base_drain_failed"),
        None,
        true,
        Some("ku_registry_unavailable"),
        false,
        false,
    );
    assert!(failed.contains("desktop_base_drain_failed"));
    let fatal = lifecycle_status::describe(
        None,
        Some("desktop_config_invalid"),
        true,
        None,
        false,
        false,
    );
    assert!(fatal.contains("desktop_config_invalid"));
}

async fn local(path: &Path) -> OneBrainNode {
    OneBrainNode::new(NodeConfig {
        data_dir: path.into(),
        port: 0,
        concept_registry_mode: ConceptRegistryMode::Disabled,
        ..Default::default()
    })
    .await
    .unwrap()
}

fn ku_config(root: &Path) -> config::DesktopConfig {
    use ku_core::foundation::{
        ObjectReference, ObservationGovernance, ResourceProfile, SourceArtifact, SourceArtifactKind,
    };
    let registry = registry_fixture::registry(root);
    drop(registry);
    let source = SourceArtifact {
        source_kind: SourceArtifactKind::Text,
        raw_bytes: b"Desktop private source".to_vec(),
        media_type_commitment: [2; 32],
        capture_adapter: ObjectReference::new(1, [1; 32]),
        capture_sequence: 1,
        governance: ObservationGovernance {
            consent_policy: ObjectReference::new(1, [2; 32]),
            consent_receipt: ObjectReference::new(1, [3; 32]),
            revocation_policy: ObjectReference::new(1, [4; 32]),
            retention_policy: ObjectReference::new(1, [5; 32]),
            capture_scope_commitment: [3; 32],
            authorization_assessment_commitment: [4; 32],
            assessed_frontier: [5; 32],
        },
    };
    let (canonical, _) = source
        .to_private_object()
        .unwrap()
        .encode(ResourceProfile::ObjectV1)
        .unwrap();
    let source_file = root.join("source.canonical");
    std::fs::write(&source_file, canonical).unwrap();
    let key_file = root.join("vault.key");
    std::fs::write(&key_file, [8; 32]).unwrap();
    let signing = ed25519_dalek::SigningKey::from_bytes(&[0x42; 32]);
    let mut cfg = config::DesktopConfig::default();
    cfg.data_dir = root.join("node");
    cfg.port = 0;
    cfg.api_port = 0;
    cfg.ku_host = Some(onebrain_api::ku_host::KuHostInputsConfig {
        registry_root: root.join("registry"),
        registry_public_key: signing
            .verifying_key()
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        vault_key_file: key_file,
        sources: vec![onebrain_api::ku_host::Source {
            label: "Admitted source".into(),
            canonical_file: source_file,
        }],
        ollama: None,
    });
    cfg
}

async fn ku_http(
    root: &str,
    token: &str,
    path: &str,
    body: Option<Value>,
) -> (reqwest::StatusCode, Value) {
    let request = match body {
        Some(body) => client().post(format!("{root}{path}")).json(&body),
        None => client().get(format!("{root}{path}")),
    };
    let response = request.bearer_auth(token).send().await.unwrap();
    let code = response.status();
    (code, response.json().await.unwrap())
}

fn ku_operation(session: &Value, operation: &str, payload: Value) -> Value {
    json!({"session":session,"budget":{"max_items":64,"max_bytes":1048576,"max_work_units":1000000},"request":{"operation":operation,"payload":payload}})
}

#[tokio::test]
async fn ku_preparation_survives_desktop_shutdown_and_restarts_on_same_node_api() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = ku_config(dir.path());
    let first = std::sync::Arc::new(Supervisor::default());
    let host = ku_start::provision_local(cfg.clone(), first.clone(), "FIRST_TOKEN".into())
        .await
        .unwrap();
    assert_eq!(host.ku_issue, None);
    let (node, port) = first.start(host, "FIRST_TOKEN".into(), 0).await.unwrap();
    let root = format!("http://127.0.0.1:{port}/api/vnext/ku");
    assert_eq!(
        ku_http(&root, "wrong", "/status", None).await.0,
        reqwest::StatusCode::FORBIDDEN
    );
    let (code, status) = ku_http(&root, "FIRST_TOKEN", "/status", None).await;
    assert_eq!(code, reqwest::StatusCode::OK, "{status}");
    assert_eq!(status["data"]["payload"]["registry_ready"], true);
    let session = &status["data"]["session"];
    let (_, catalog) = ku_http(&root, "FIRST_TOKEN", "/editor", Some(json!({"session":session,"budget":{"max_items":64,"max_bytes":1048576,"max_work_units":1000000},"request":{"action":"catalog","payload":{}}}))).await;
    let source = &catalog["data"]["payload"]["sources"][0]["source_ref"];
    let (code, rejected) = ku_http(
        &root,
        "FIRST_TOKEN",
        "/editor",
        Some(json!({"session":session,"budget":{"max_items":64,"max_bytes":1048576,"max_work_units":1000000},"request":{"action":"catalog","payload":{"vault_key":"PRIVATE_CANARY","authorized":true}}})),
    ).await;
    assert_eq!(code, reqwest::StatusCode::BAD_REQUEST);
    assert!(!rejected.to_string().contains("PRIVATE_CANARY"));
    let (_, reserved) = ku_http(
        &root,
        "FIRST_TOKEN",
        "/reservations",
        Some(json!({"session":session})),
    )
    .await;
    let op = &reserved["data"]["payload"]["operation_id"];
    let (code, draft) = ku_http(&root, "FIRST_TOKEN", "/editor", Some(json!({"session":session,"budget":{"max_items":64,"max_bytes":1048576,"max_work_units":1000000},"request":{"action":"draft","payload":{"operation_id":op,"idempotency_key":op,"source_ref":source,"predicate_label":"water","selected_ccid":"07".repeat(16),"argument_text":"Desktop private statement"}}}))).await;
    assert_eq!(code, reqwest::StatusCode::OK, "{draft}");
    let (code, prepared) = ku_http(
        &root,
        "FIRST_TOKEN",
        "/operations",
        Some(ku_operation(
            session,
            "prepare",
            draft["data"]["payload"].clone(),
        )),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::OK, "{prepared}");
    assert_eq!(prepared["data"]["payload"]["validity"], "ready");
    let ids = prepared["data"]["payload"]["object_cids"].clone();
    first.shutdown().await.unwrap();
    assert!(node
        .lock()
        .await
        .base_services()
        .unwrap()
        .snapshot()
        .is_err());
    drop(node);

    let second = std::sync::Arc::new(Supervisor::default());
    let host = ku_start::provision_local(cfg.clone(), second.clone(), "SECOND_TOKEN".into())
        .await
        .unwrap();
    let (_, port) = second.start(host, "SECOND_TOKEN".into(), 0).await.unwrap();
    let root = format!("http://127.0.0.1:{port}/api/vnext/ku");
    let (_, status2) = ku_http(&root, "SECOND_TOKEN", "/status", None).await;
    let session2 = &status2["data"]["session"];
    assert_ne!(
        session["process_generation"],
        session2["process_generation"]
    );
    assert_eq!(
        session["dataset_generation"],
        session2["dataset_generation"]
    );
    let (code, recovered) = ku_http(
        &root,
        "SECOND_TOKEN",
        "/operations",
        Some(ku_operation(
            session2,
            "reconcile",
            json!({"operation_id":op}),
        )),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::OK, "{recovered}");
    assert_eq!(recovered["data"]["payload"]["state"], "prepared");
    let (code, preview) = ku_http(
        &root,
        "SECOND_TOKEN",
        "/operations",
        Some(ku_operation(
            session2,
            "preview",
            json!({"operation_id":op}),
        )),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::OK, "{preview}");
    assert_eq!(preview["data"]["payload"]["object_cids"], ids);
    let (code, saved) = ku_http(
        &root,
        "SECOND_TOKEN",
        "/operations",
        Some(ku_operation(
            session2,
            "save",
            json!({"operation_id":op,"idempotency_key":op,"object_cids":ids}),
        )),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::OK, "{saved}");
    assert_eq!(saved["data"]["payload"]["state"], "committed");
    let (code, view) = ku_http(
        &root,
        "SECOND_TOKEN",
        "/operations",
        Some(ku_operation(session2, "get", json!({"object_cid":ids[0]}))),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::OK, "{view}");
    assert_eq!(view["data"]["payload"]["disclosure_class"], "LOCAL_ONLY");
    assert!(!status2.to_string().contains("Desktop private"));
    second.shutdown().await.unwrap();

    let mut degraded = cfg;
    degraded.ku_host.as_mut().unwrap().registry_root = dir.path().join("registry-offline");
    let third = std::sync::Arc::new(Supervisor::default());
    let host = ku_start::provision_local(degraded, third.clone(), "THIRD_TOKEN".into())
        .await
        .unwrap();
    assert_eq!(host.ku_issue, Some("ku_registry_unavailable"));
    let (_, port) = third.start(host, "THIRD_TOKEN".into(), 0).await.unwrap();
    let root = format!("http://127.0.0.1:{port}/api/vnext/ku");
    let (code, status3) = ku_http(&root, "THIRD_TOKEN", "/status", None).await;
    assert_eq!(code, reqwest::StatusCode::OK, "{status3}");
    assert_eq!(status3["data"]["payload"]["registry_ready"], false);
    let session3 = &status3["data"]["session"];
    let (code, view) = ku_http(
        &root,
        "THIRD_TOKEN",
        "/operations",
        Some(ku_operation(session3, "get", json!({"object_cid":ids[0]}))),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::OK, "{view}");
    assert_eq!(view["data"]["payload"]["disclosure_class"], "LOCAL_ONLY");
    let (code, page) = ku_http(
        &root,
        "THIRD_TOKEN",
        "/operations",
        Some(ku_operation(session3, "list", json!({"limit":20}))),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::OK, "{page}");
    assert_eq!(
        page["data"]["payload"]["items"].as_array().unwrap().len(),
        1
    );
    let (code, _) = ku_http(&root, "THIRD_TOKEN", "/editor", Some(json!({"session":session3,"budget":{"max_items":64,"max_bytes":1048576,"max_work_units":1000000},"request":{"action":"catalog","payload":{}}}))).await;
    assert_eq!(code, reqwest::StatusCode::SERVICE_UNAVAILABLE);
    third.shutdown().await.unwrap();
}

#[tokio::test]
async fn ku_dependency_failure_is_bounded_and_keeps_legacy_local_reads() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = ku_config(dir.path());
    cfg.ku_host.as_mut().unwrap().vault_key_file = dir.path().join("missing-key");
    let supervisor = std::sync::Arc::new(Supervisor::default());
    let host = ku_start::provision_local(cfg, supervisor.clone(), "TOKEN".into())
        .await
        .unwrap();
    assert_eq!(host.ku_issue, Some("ku_host_input_unavailable"));
    let (_, port) = supervisor.start(host, "TOKEN".into(), 0).await.unwrap();
    let root = format!("http://127.0.0.1:{port}");
    let response = client()
        .get(format!("{root}/api/status"))
        .bearer_auth("TOKEN")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let (code, failure) = ku_http(&root, "TOKEN", "/api/vnext/ku/status", None).await;
    assert_eq!(code, reqwest::StatusCode::SERVICE_UNAVAILABLE);
    assert!(!failure.to_string().contains("missing-key"));
    supervisor.shutdown().await.unwrap();
}
fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}

#[tokio::test]
async fn default_is_local_only_and_ready_requires_a_bound_authenticated_listener() {
    assert!(!config::DesktopConfig::default().auto_start);
    let dir = tempfile::tempdir().unwrap();
    let supervisor = Supervisor::default();
    assert!(!supervisor.ready());
    assert!(!*supervisor.execution().borrow());
    let (node, port) = supervisor
        .start(HostNode::local(local(dir.path()).await), "TOKEN".into(), 0)
        .await
        .unwrap();
    assert!(supervisor.ready());
    assert_eq!(
        supervisor::tray_status(Some(node.clone())).await,
        "OBP unavailable - local observation"
    );
    assert!(node.lock().await.listener_addr().is_none());
    let url = format!("http://127.0.0.1:{port}/api/vnext/obp/status");
    assert_eq!(client().get(&url).send().await.unwrap().status(), 401);
    let response = client()
        .get(&url)
        .bearer_auth("TOKEN")
        .header("Origin", "http://tauri.localhost")
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.headers()["access-control-allow-origin"],
        "http://tauri.localhost"
    );
    assert_eq!(response.headers()["cache-control"], "no-store");
    let status: Value = response.json().await.unwrap();
    assert_eq!(status["data"]["available"], false);
    supervisor.shutdown().await.unwrap();
    assert!(!supervisor.ready());
    assert!(client()
        .get(&url)
        .bearer_auth("TOKEN")
        .send()
        .await
        .is_err());
    let _rebound = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .unwrap();
}

#[tokio::test]
async fn occupied_port_fails_closed_without_fallback_and_drains_the_supplied_node() {
    let dir = tempfile::tempdir().unwrap();
    let occupied = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let supervisor = Supervisor::default();
    let result = supervisor
        .start(
            HostNode::local(local(dir.path()).await),
            "TOKEN".into(),
            occupied.local_addr().unwrap().port(),
        )
        .await;
    assert_eq!(result.err(), Some("desktop_api_bind_failed"));
    assert!(!supervisor.ready());
    supervisor.shutdown().await.unwrap();
}

#[tokio::test]
async fn base_close_error_still_releases_the_local_listener() {
    let dir = tempfile::tempdir().unwrap();
    let mut node = local(dir.path()).await;
    node.install_base_runtime(onebrain_api::base_runtime_config_for_api_token("TOKEN"))
        .unwrap();
    // A previously closed Base returns a typed close error. The supervisor
    // must still stop accepting sockets while reporting incomplete shutdown.
    node.base_services().unwrap().close().await.unwrap();
    let supervisor = Supervisor::default();
    let (_, port) = supervisor
        .start(HostNode::local(node), "TOKEN".into(), 0)
        .await
        .unwrap();
    assert_eq!(
        supervisor.shutdown().await,
        Err("desktop_base_drain_failed")
    );
    assert!(!supervisor.ready());
    let _rebound = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .unwrap();
}

#[tokio::test]
async fn lifecycle_fence_is_idempotent_revokes_execution_and_prevents_late_start() {
    let dir = tempfile::tempdir().unwrap();
    let supervisor = Supervisor::default();
    let execution = supervisor.execution();
    supervisor.grant_execution().unwrap();
    assert!(*execution.borrow());
    assert!(supervisor.fence());
    assert!(!supervisor.fence());
    assert!(!*execution.borrow());
    assert!(supervisor.grant_execution().is_err());
    assert!(supervisor
        .start(HostNode::local(local(dir.path()).await), "TOKEN".into(), 0)
        .await
        .is_err());
    supervisor.shutdown().await.unwrap();
}

#[tokio::test]
async fn shutdown_joins_owned_auxiliary_and_is_safe_when_concurrent() {
    let supervisor = std::sync::Arc::new(Supervisor::default());
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    supervisor
        .own_auxiliary(tokio::spawn(async move {
            let _tx = tx;
            std::future::pending::<()>().await;
        }))
        .await;
    let (first, second) = tokio::join!(supervisor.shutdown(), supervisor.shutdown());
    first.unwrap();
    second.unwrap();
    assert!(rx.await.is_err());
}

#[cfg(feature = "vnext-outbound-first")]
fn record() -> String {
    json!({"origin":"http://127.0.0.1:4280","session":{"process_generation":"11".repeat(32),"dataset_generation":"22".repeat(32)},"operation":"network_kill","payload":{"idempotency_key":"33".repeat(32),"expected_generation":1}}).to_string()
}
#[cfg(feature = "vnext-outbound-first")]
#[test]
fn recovery_survives_reopen_refuses_overwrite_and_rejects_credentials_corruption_and_overflow() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pending.json");
    recovery::save(&path, &record()).unwrap();
    assert_eq!(recovery::load(&path).unwrap(), Some(record()));
    recovery::save(&path, &record()).unwrap();
    assert!(recovery::save(&path, &record().replace(&"33".repeat(32), &"44".repeat(32))).is_err());
    recovery::clear(&path).unwrap();
    let mut secret: Value = serde_json::from_str(&record()).unwrap();
    secret["token"] = "SECRET".into();
    assert!(recovery::save(&path, &secret.to_string()).is_err());
    assert!(recovery::save(&path, &"x".repeat(1_048_577)).is_err());
    std::fs::write(&path, b"broken").unwrap();
    assert!(recovery::load(&path).is_err());
    assert!(recovery::save(&path, &record()).is_err());
}

#[cfg(feature = "vnext-outbound-first")]
async fn provision(path: &Path) -> (HostNode, String) {
    use ku_core::foundation::{MetabolicViewPolicy, ObjectReference};
    use onebrain_node::{LocalPolicyRegistry, LocalPolicyVersion, VNextProductRuntimeDependencies};
    let mut config = NodeConfig {
        data_dir: path.into(),
        port: 0,
        concept_registry_mode: ConceptRegistryMode::Disabled,
        ..Default::default()
    };
    config.vnext.enabled.object_event_v1 = true;
    config.vnext.enabled.obp_rp = true;
    let mut node = OneBrainNode::new(config).await.unwrap();
    let policies = LocalPolicyRegistry::new([(
        LocalPolicyVersion::new(1).unwrap(),
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
    node.set_vnext_product_dependencies(VNextProductRuntimeDependencies::new(
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
            Duration::from_secs(300),
        )
        .unwrap();
    (
        HostNode {
            node,
            ku_issue: None,
            binding: Some(onebrain_api::obp_api::Binding::new([1; 32], Some(control))),
        },
        management,
    )
}

#[cfg(feature = "vnext-outbound-first")]
#[tokio::test]
async fn real_shared_service_preserves_disabled_generation_and_reconciles_original_key_after_restart(
) {
    let dir = tempfile::tempdir().unwrap();
    use ku_core::foundation::{DisclosureClass, NamespaceCommitment, NodeId, SelectorCid};
    use onebrain_node::{
        dataset_path::BaseStorageOwnerId,
        vnext_outbox::{OutboundIntentState, OutboundOutbox, OutboundTransferIntent},
    };
    let seed = local(dir.path()).await;
    let outbox_path = seed
        .shared
        .lock()
        .await
        .dataset_paths
        .owner_path(BaseStorageOwnerId::OUTBOX)
        .unwrap()
        .join("vnext_outbox.redb");
    let intent = OutboundTransferIntent::new(
        NodeId::from_bytes([9; 32]),
        "127.0.0.1:42001".parse().unwrap(),
        SelectorCid::from_bytes([2; 32]),
        NamespaceCommitment::from_bytes([3; 32]),
        DisclosureClass::Public,
        onebrain_protocol::ReconcileManifestKind::Object,
        b"desktop-durable-pending-fixture".to_vec(),
    )
    .unwrap();
    {
        let outbox = OutboundOutbox::open(&outbox_path).unwrap();
        outbox.enqueue(&intent).unwrap();
        outbox.record_transport_attempt(&intent.id).unwrap();
    }
    drop(seed);
    let (host, management) = provision(dir.path()).await;
    let supervisor = Supervisor::default();
    let (node, port) = supervisor.start(host, "TOKEN".into(), 0).await.unwrap();
    let root = format!("http://127.0.0.1:{port}/api/vnext/obp");
    let initial: Value = client()
        .get(format!("{root}/status"))
        .bearer_auth("TOKEN")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let services = node.lock().await.vnext_product_services().unwrap();
    let ticket: Value = client()
        .post(format!("{root}/ws/tickets"))
        .bearer_auth("TOKEN")
        .json(&json!({"session":initial["data"]["session"], "subscriptions":["network"]}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let (mut socket, _) = tokio_tungstenite::connect_async(format!(
        "ws://127.0.0.1:{port}/api/vnext/obp/ws?ticket={}",
        ticket["data"]["ticket"].as_str().unwrap()
    ))
    .await
    .unwrap();
    use futures::StreamExt;
    for _ in 0..2 {
        socket.next().await.unwrap().unwrap();
    }
    // Reads use a weak service handle, independent of the node mutex.
    let owner_lock = node.lock().await;
    assert_eq!(
        services.obp_status().await.unwrap(),
        initial["data"]["status"]
    );
    drop(owner_lock);
    let command = json!({"session":initial["data"]["session"],"operation":"network_kill","payload":{"idempotency_key":"33".repeat(32),"expected_generation":initial["data"]["status"]["generation"]}});
    let result: Value = client()
        .post(format!("{root}/commands"))
        .bearer_auth("TOKEN")
        .header("X-OneBrain-OBP-Management", management)
        .json(&command)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(result["data"]["state"], "completed", "{result}");
    let generation = result["data"]["result"]["generation"].clone();
    supervisor.shutdown().await.unwrap();
    let closed = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap();
    assert!(matches!(
        closed,
        None | Some(Err(_)) | Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_)))
    ));
    assert!(services.obp_status().await.is_err());
    drop(node);
    drop(services);
    // Initial startup has its canonical bounded outbox drain. Capture its
    // actual counter; the subsequent durably killed restart must not add work.
    let attempts_before_restart = {
        let outbox = OutboundOutbox::open(&outbox_path).unwrap();
        outbox.get(&intent.id).unwrap().unwrap().transport_attempts
    };
    assert!(attempts_before_restart >= 1);
    let (host, _) = provision(dir.path()).await;
    let next = Supervisor::default();
    let (_, port) = next.start(host, "NEW_TOKEN".into(), 0).await.unwrap();
    let root = format!("http://127.0.0.1:{port}/api/vnext/obp");
    let current: Value = client()
        .get(format!("{root}/status"))
        .bearer_auth("NEW_TOKEN")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(current["data"]["status"]["kill_switch"], true);
    assert_eq!(current["data"]["status"]["generation"], generation);
    assert_eq!(current["data"]["status"]["active"], false);
    assert_eq!(current["data"]["status"]["requested"], false);
    assert_eq!(current["data"]["status"]["pending_intents"], 1);
    assert_eq!(
        initial["data"]["session"]["dataset_generation"],
        current["data"]["session"]["dataset_generation"]
    );
    assert_ne!(
        initial["data"]["session"]["process_generation"],
        current["data"]["session"]["process_generation"]
    );
    let reconciled: Value = client()
        .post(format!("{root}/reconcile"))
        .bearer_auth("NEW_TOKEN")
        .json(&json!({"session":current["data"]["session"],"idempotency_key":"33".repeat(32)}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(reconciled["data"]["state"], "completed");
    assert!(reconciled["data"].get("result").is_none());
    next.shutdown().await.unwrap();
    let outbox = OutboundOutbox::open(&outbox_path).unwrap();
    let retained = outbox.get(&intent.id).unwrap().unwrap();
    assert_eq!(retained.state, OutboundIntentState::Pending);
    assert_eq!(retained.transport_attempts, attempts_before_restart);
    assert_eq!(retained.validation_retries, 0);
    assert_eq!(retained.canonical_bytes, intent.canonical_bytes);
    assert_eq!(retained.expected_peer, intent.expected_peer);
}

#[cfg(feature = "vnext-network-runtime")]
async fn provision_ku_with_opt_in_vnext(
    cfg: &config::DesktopConfig,
    token: &str,
    signer_seed: u8,
    install_base_before_network: bool,
) -> (HostNode, String, std::net::SocketAddr) {
    use ku_core::foundation::{MetabolicViewPolicy, ObjectReference};
    use onebrain_base_contract::ku_payload::hex;
    use onebrain_node::{LocalPolicyRegistry, LocalPolicyVersion, VNextProductRuntimeDependencies};

    let mut node_config = cfg.to_node_config();
    node_config.vnext.enabled.object_event_v1 = true;
    node_config.vnext.enabled.obp_rp = true;
    std::fs::create_dir_all(&node_config.data_dir).unwrap();

    let ku_cfg = cfg.ku_host.as_ref().unwrap();
    let (ku_runtime, ku_issue) =
        onebrain_api::ku_host::prepare_ku_runtime(&node_config, ku_cfg).unwrap();
    assert_eq!(ku_issue, None);

    let mut node = OneBrainNode::new(node_config).await.unwrap();
    let policies = LocalPolicyRegistry::new([(
        LocalPolicyVersion::new(1).unwrap(),
        MetabolicViewPolicy {
            policy_ref: ObjectReference::new(0, [3; 32]),
            accepted_evidence_policies: vec![ObjectReference::new(0, [4; 32])],
            recent_event_horizon: 64,
        },
    )])
    .unwrap();
    node.set_vnext_identity_signer(std::sync::Arc::new(ed25519_dalek::SigningKey::from_bytes(
        &[signer_seed; 32],
    )));
    node.set_vnext_product_dependencies(VNextProductRuntimeDependencies::new(
        ku_kql::vnext_private_need::LocalNeedVaultKey::from_bytes([signer_seed ^ 0x55; 32]),
        policies,
    ))
    .unwrap();

    let mut base = onebrain_api::base_runtime_config_for_api_token(token);
    base.ku = Some(ku_runtime);
    if install_base_before_network {
        node.install_base_runtime(base).unwrap();
        node.start_network().await.unwrap();
    } else {
        node.start_network().await.unwrap();
        node.install_base_runtime(base).unwrap();
    }

    let status = node.vnext_product_runtime_status().unwrap().unwrap();
    let peer_hex = hex(&status.network.principal);
    let peer_addr = std::net::SocketAddr::from((
        std::net::Ipv4Addr::LOCALHOST,
        status.network.listen_addr.port(),
    ));
    #[cfg(feature = "vnext-outbound-first")]
    let binding = {
        let host = node.obp_host().unwrap();
        let control = host.control([1; 32]).unwrap();
        Some(onebrain_api::obp_api::Binding::new([1; 32], Some(control)))
    };
    (
        HostNode {
            node,
            ku_issue: None,
            #[cfg(feature = "vnext-outbound-first")]
            binding,
        },
        peer_hex,
        peer_addr,
    )
}

#[cfg(feature = "vnext-network-runtime")]
#[tokio::test]
async fn vnext_product_runtime_binds_canonical_public_reader_for_desktop_ku_share_and_get_export() {
    let sender_dir = tempfile::tempdir().unwrap();
    let receiver_dir = tempfile::tempdir().unwrap();
    let sender_cfg = ku_config(sender_dir.path());
    let receiver_cfg = ku_config(receiver_dir.path());

    let (sender_host, sender_peer_hex, _) =
        provision_ku_with_opt_in_vnext(&sender_cfg, "SENDER_TOKEN", 0x31, true).await;
    let (receiver_host, receiver_peer_hex, receiver_vnext_addr) =
        provision_ku_with_opt_in_vnext(&receiver_cfg, "RECEIVER_TOKEN", 0x32, false).await;

    let sender_sup = Supervisor::default();
    let receiver_sup = Supervisor::default();
    let (sender_node, sender_port) = sender_sup
        .start(sender_host, "SENDER_TOKEN".into(), 0)
        .await
        .unwrap();
    let (_, receiver_port) = receiver_sup
        .start(receiver_host, "RECEIVER_TOKEN".into(), 0)
        .await
        .unwrap();

    let sender_ku_root = format!("http://127.0.0.1:{sender_port}/api/vnext/ku");
    let receiver_ku_root = format!("http://127.0.0.1:{receiver_port}/api/vnext/ku");

    let (code, sender_status) = ku_http(&sender_ku_root, "SENDER_TOKEN", "/status", None).await;
    assert_eq!(code, reqwest::StatusCode::OK, "{sender_status}");
    let sender_session = &sender_status["data"]["session"];

    let (code, receiver_status) =
        ku_http(&receiver_ku_root, "RECEIVER_TOKEN", "/status", None).await;
    assert_eq!(code, reqwest::StatusCode::OK, "{receiver_status}");
    let receiver_session = &receiver_status["data"]["session"];

    let editor_req = |session: &Value, action: &str, payload: Value| {
        json!({
            "session": session,
            "budget": {
                "max_items": 64,
                "max_bytes": 1048576,
                "max_work_units": 1000000
            },
            "request": {
                "action": action,
                "payload": payload
            }
        })
    };

    let (_, catalog) = ku_http(
        &sender_ku_root,
        "SENDER_TOKEN",
        "/editor",
        Some(editor_req(sender_session, "catalog", json!({}))),
    )
    .await;
    let source = &catalog["data"]["payload"]["sources"][0]["source_ref"];
    let (_, reserved) = ku_http(
        &sender_ku_root,
        "SENDER_TOKEN",
        "/reservations",
        Some(json!({"session": sender_session})),
    )
    .await;
    let op = &reserved["data"]["payload"]["operation_id"];
    let (code, draft) = ku_http(
        &sender_ku_root,
        "SENDER_TOKEN",
        "/editor",
        Some(editor_req(
            sender_session,
            "draft",
            json!({
                "operation_id": op,
                "idempotency_key": op,
                "source_ref": source,
                "predicate_label": "water",
                "selected_ccid": "07".repeat(16),
                "argument_text": "Desktop opt-in public share statement"
            }),
        )),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::OK, "{draft}");
    let (code, prepared) = ku_http(
        &sender_ku_root,
        "SENDER_TOKEN",
        "/operations",
        Some(ku_operation(
            sender_session,
            "prepare",
            draft["data"]["payload"].clone(),
        )),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::OK, "{prepared}");
    let private_cid_hex = prepared["data"]["payload"]["object_cids"][0]
        .as_str()
        .unwrap()
        .to_owned();
    let sem_cid_hex = prepared["data"]["payload"]["artifacts"][0]["semantic_content_cid"]
        .as_str()
        .unwrap()
        .to_owned();
    let (code, saved) = ku_http(
        &sender_ku_root,
        "SENDER_TOKEN",
        "/operations",
        Some(ku_operation(
            sender_session,
            "save",
            json!({
                "operation_id": op,
                "idempotency_key": op,
                "object_cids": [private_cid_hex]
            }),
        )),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::OK, "{saved}");

    let selector_hex = "02".repeat(32);
    let namespace_hex = "03".repeat(32);
    let (code, share_prepared) = ku_http(
        &sender_ku_root,
        "SENDER_TOKEN",
        "/editor",
        Some(editor_req(
            sender_session,
            "share_prepare",
            json!({
                "object_cid": private_cid_hex,
                "expected_node_id": receiver_peer_hex,
                "address": receiver_vnext_addr.to_string(),
                "selector": selector_hex,
                "namespace": namespace_hex
            }),
        )),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::OK, "{share_prepared}");
    let preview = &share_prepared["data"]["payload"];
    assert_eq!(preview["event"], "share_prepared");
    assert_eq!(preview["enqueued"], false);
    assert_eq!(preview["payload_sent"], false);
    let prepared_id = preview["prepared_id"].as_str().unwrap().to_owned();
    let public_cid_hex = preview["public_object_cid"].as_str().unwrap().to_owned();
    assert_ne!(public_cid_hex, private_cid_hex);
    assert_eq!(preview["semantic_content_cid"], sem_cid_hex);
    assert!(!preview.to_string().contains(&private_cid_hex));
    assert!(!preview.to_string().contains("Desktop private source"));

    // Explicit confirmation is required.
    let (code, unconfirmed) = ku_http(
        &sender_ku_root,
        "SENDER_TOKEN",
        "/editor",
        Some(editor_req(
            sender_session,
            "share_confirm",
            json!({
                "prepared_id": prepared_id,
                "confirm_public": false
            }),
        )),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::BAD_REQUEST, "{unconfirmed}");
    assert_eq!(
        unconfirmed["error"]["message"],
        "explicit_public_confirmation_required"
    );

    // Killing VNextRuntimeLane::Network fences share_confirm and share_inspect with
    // capability_disabled while preserving the single-use prepared_id.
    sender_node
        .lock()
        .await
        .kill_vnext_runtime_lane(onebrain_node::VNextRuntimeLane::Network)
        .unwrap();
    let (code, fenced_confirm) = ku_http(
        &sender_ku_root,
        "SENDER_TOKEN",
        "/editor",
        Some(editor_req(
            sender_session,
            "share_confirm",
            json!({
                "prepared_id": prepared_id,
                "confirm_public": true
            }),
        )),
    )
    .await;
    assert_eq!(
        code,
        reqwest::StatusCode::SERVICE_UNAVAILABLE,
        "{fenced_confirm}"
    );
    assert_eq!(fenced_confirm["error"]["code"], "capability_disabled");
    assert_eq!(
        fenced_confirm["error"]["message"],
        "ku_public_share_network_unavailable"
    );

    sender_node
        .lock()
        .await
        .reenable_vnext_runtime_lane(onebrain_node::VNextRuntimeLane::Network)
        .unwrap();

    let (code, confirmed_resp) = ku_http(
        &sender_ku_root,
        "SENDER_TOKEN",
        "/editor",
        Some(editor_req(
            sender_session,
            "share_confirm",
            json!({
                "prepared_id": prepared_id,
                "confirm_public": true
            }),
        )),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::OK, "{confirmed_resp}");
    let confirmed = &confirmed_resp["data"]["payload"];
    assert_eq!(confirmed["event"], "share_confirmed");
    assert_eq!(confirmed["public_object_cid"], public_cid_hex);
    assert_eq!(confirmed["authorizes_reward"], false);
    let intent_id = confirmed["intent_id"].as_str().unwrap().to_owned();

    let (code, replay_confirm) = ku_http(
        &sender_ku_root,
        "SENDER_TOKEN",
        "/editor",
        Some(editor_req(
            sender_session,
            "share_confirm",
            json!({
                "prepared_id": prepared_id,
                "confirm_public": true
            }),
        )),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::NOT_FOUND, "{replay_confirm}");
    assert_eq!(replay_confirm["error"]["message"], "prepared_share_missing");

    let (code, intent_resp) = ku_http(
        &sender_ku_root,
        "SENDER_TOKEN",
        "/editor",
        Some(editor_req(
            sender_session,
            "share_intent",
            json!({ "intent_id": intent_id }),
        )),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::OK, "{intent_resp}");
    assert_eq!(intent_resp["data"]["payload"]["event"], "intent");
    assert_eq!(
        intent_resp["data"]["payload"]["public_object_cid"],
        public_cid_hex
    );
    assert_eq!(
        intent_resp["data"]["payload"]["claims_network_completion"],
        false
    );

    let mut accepted = None;
    for _ in 0..60 {
        let (code, inspect_resp) = ku_http(
            &receiver_ku_root,
            "RECEIVER_TOKEN",
            "/editor",
            Some(editor_req(
                receiver_session,
                "share_inspect",
                json!({
                    "object_cid": public_cid_hex,
                    "selector": selector_hex
                }),
            )),
        )
        .await;
        if code == reqwest::StatusCode::OK {
            accepted = Some(inspect_resp["data"]["payload"].clone());
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let accepted = accepted.expect("receiver VNextProductRuntime should accept shared public KU");
    assert_eq!(accepted["event"], "public_object");
    assert_eq!(accepted["semantic"]["semantic_content_cid"], sem_cid_hex);
    assert_eq!(
        accepted["authenticated_source_peers"],
        json!([sender_peer_hex])
    );
    assert_eq!(
        accepted["provenance"],
        "transport/selector only; original source withheld and unverifiable"
    );
    assert_eq!(accepted["fidelity"], "unassessed");
    assert_eq!(accepted["authorizes_reward"], false);

    let (code, get_resp) = ku_http(
        &receiver_ku_root,
        "RECEIVER_TOKEN",
        "/operations",
        Some(ku_operation(
            receiver_session,
            "get",
            json!({ "object_cid": public_cid_hex }),
        )),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::OK, "{get_resp}");
    let view = &get_resp["data"]["payload"];
    assert_eq!(view["object_cid"], public_cid_hex);
    assert_eq!(view["disclosure_class"], "PUBLIC");
    assert_eq!(view["artifact_validity"], "accepted_known");
    assert_eq!(view["semantic_content_cid"], sem_cid_hex);
    assert_eq!(view["executable"], false);

    let (code, export_resp) = ku_http(
        &receiver_ku_root,
        "RECEIVER_TOKEN",
        "/operations",
        Some(ku_operation(
            receiver_session,
            "export",
            json!({
                "mode": "canonical_public_exchange",
                "object_cids": [public_cid_hex]
            }),
        )),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::OK, "{export_resp}");
    assert!(export_resp["data"]["payload"]["public_records"].is_string());

    let (code, private_get) = ku_http(
        &receiver_ku_root,
        "RECEIVER_TOKEN",
        "/operations",
        Some(ku_operation(
            receiver_session,
            "get",
            json!({ "object_cid": private_cid_hex }),
        )),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::NOT_FOUND, "{private_get}");

    sender_sup.shutdown().await.unwrap();
    receiver_sup.shutdown().await.unwrap();
    drop(sender_node);

    // Restart receiver in default local-only mode; committed public KU remains readable
    // and exportable from paths.network.canonical (vnext_verified.redb) while share_inspect
    // fails closed with capability_disabled.
    let local_receiver_sup = std::sync::Arc::new(Supervisor::default());
    let local_receiver_host = ku_start::provision_local(
        receiver_cfg,
        local_receiver_sup.clone(),
        "LOCAL_RECEIVER_TOKEN".into(),
    )
    .await
    .unwrap();
    let (_, local_receiver_port) = local_receiver_sup
        .start(local_receiver_host, "LOCAL_RECEIVER_TOKEN".into(), 0)
        .await
        .unwrap();
    let local_receiver_root = format!("http://127.0.0.1:{local_receiver_port}/api/vnext/ku");
    let (code, local_status) = ku_http(
        &local_receiver_root,
        "LOCAL_RECEIVER_TOKEN",
        "/status",
        None,
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::OK, "{local_status}");
    let local_session = &local_status["data"]["session"];

    let (code, local_get) = ku_http(
        &local_receiver_root,
        "LOCAL_RECEIVER_TOKEN",
        "/operations",
        Some(ku_operation(
            local_session,
            "get",
            json!({ "object_cid": public_cid_hex }),
        )),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::OK, "{local_get}");
    assert_eq!(local_get["data"]["payload"]["disclosure_class"], "PUBLIC");

    let (code, local_export) = ku_http(
        &local_receiver_root,
        "LOCAL_RECEIVER_TOKEN",
        "/operations",
        Some(ku_operation(
            local_session,
            "export",
            json!({
                "mode": "canonical_public_exchange",
                "object_cids": [public_cid_hex]
            }),
        )),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::OK, "{local_export}");
    assert!(local_export["data"]["payload"]["public_records"].is_string());

    let (code, local_inspect) = ku_http(
        &local_receiver_root,
        "LOCAL_RECEIVER_TOKEN",
        "/editor",
        Some(editor_req(
            local_session,
            "share_inspect",
            json!({
                "object_cid": public_cid_hex,
                "selector": selector_hex
            }),
        )),
    )
    .await;
    assert_eq!(
        code,
        reqwest::StatusCode::SERVICE_UNAVAILABLE,
        "{local_inspect}"
    );
    assert_eq!(local_inspect["error"]["code"], "capability_disabled");
    assert_eq!(
        local_inspect["error"]["message"],
        "ku_public_share_network_unavailable"
    );

    local_receiver_sup.shutdown().await.unwrap();
}
