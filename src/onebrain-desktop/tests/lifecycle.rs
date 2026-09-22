// Import the product modules without loading a native WebView test harness.
#[path = "../src/config.rs"]
mod config;
#[path = "../src/local_listener.rs"]
mod local_listener;
#[path = "../src/recovery.rs"]
mod recovery;
#[path = "../src/supervisor.rs"]
mod supervisor;
use onebrain_node::{ConceptRegistryMode, NodeConfig, OneBrainNode};
use serde_json::{json, Value};
use std::{path::Path, time::Duration};
use supervisor::{HostNode, Supervisor};

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
    supervisor.shutdown().await;
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
    supervisor.shutdown().await;
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
    supervisor.shutdown().await;
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
    tokio::join!(supervisor.shutdown(), supervisor.shutdown());
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
    supervisor.shutdown().await;
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
    next.shutdown().await;
    let outbox = OutboundOutbox::open(&outbox_path).unwrap();
    let retained = outbox.get(&intent.id).unwrap().unwrap();
    assert_eq!(retained.state, OutboundIntentState::Pending);
    assert_eq!(retained.transport_attempts, attempts_before_restart);
    assert_eq!(retained.validation_retries, 0);
    assert_eq!(retained.canonical_bytes, intent.canonical_bytes);
    assert_eq!(retained.expected_peer, intent.expected_peer);
}
