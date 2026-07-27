#![cfg(feature = "vnext-network-runtime")]

use ku_core::foundation::{MetabolicViewPolicy, ObjectReference};
use ku_kql::vnext_private_need::LocalNeedVaultKey;
use onebrain_node::{
    ConceptRegistryMode, LocalPolicyRegistry, LocalPolicyVersion, NetworkRuntimeLifecycle,
    NodeConfig, OneBrainNode, VNextFeature, VNextProductRuntimeDependencies,
    VNextProductRuntimeState, VNextRuntimeLane, VNextStartupPhase,
};
use tokio::net::TcpListener;

fn product_dependencies() -> VNextProductRuntimeDependencies {
    let policy_version = LocalPolicyVersion::new(1).unwrap();
    let policies = LocalPolicyRegistry::new([(
        policy_version,
        MetabolicViewPolicy {
            policy_ref: ObjectReference::new(0, [0x21; 32]),
            accepted_evidence_policies: vec![ObjectReference::new(0, [0x22; 32])],
            recent_event_horizon: 64,
        },
    )])
    .unwrap();
    VNextProductRuntimeDependencies::new(LocalNeedVaultKey::from_bytes([0x23; 32]), policies)
}

#[tokio::test]
async fn feature_flags_start_a_real_node_owned_product_runtime_and_status_tracks_it() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = NodeConfig::default();
    config.name = "vnext-node-test".into();
    config.port = 0;
    config.data_dir = directory.path().to_path_buf();
    config.concept_registry_mode = ConceptRegistryMode::Disabled;
    config.vnext.enabled.object_event_v1 = true;
    config.vnext.enabled.obp_rp = true;
    config.vnext.enabled.distributed_kql_one_hop = true;
    config.vnext.enabled.public_use_evidence_publish = true;
    config.vnext.enabled.distributed_pomv_view = true;
    assert!(config.vnext.is_active(VNextFeature::ObpRp));

    let mut node = OneBrainNode::new(config).await.unwrap();
    node.set_vnext_product_dependencies(product_dependencies())
        .unwrap();
    let before = node.vnext_status();
    assert_eq!(
        before.network_runtime.lifecycle,
        NetworkRuntimeLifecycle::Configured
    );
    assert!(!before.features.obp_rp);

    node.start_network().await.unwrap();
    let quic_addr = node.vnext_listener_addr().expect("real QUIC listener");
    assert_ne!(quic_addr.port(), 0);
    let after = node.vnext_status();
    assert_eq!(
        after.network_runtime.lifecycle,
        NetworkRuntimeLifecycle::Listening
    );
    assert_eq!(
        after.network_runtime.listen_addr,
        Some(quic_addr.to_string())
    );
    assert!(after.features.object_event_v1);
    assert!(after.features.obp_rp);
    assert!(after.features.distributed_kql_one_hop);
    assert!(after.features.public_use_evidence_publish);
    assert!(after.features.distributed_pomv_view);
    assert!(!after.network_runtime.claims_network_completion);
    let product = node
        .vnext_product_runtime_status()
        .unwrap()
        .expect("integrated product runtime status");
    assert_eq!(product.state, VNextProductRuntimeState::Running);
    assert_eq!(product.active_private_needs, 0);
    assert_eq!(product.pending_publications, 0);
    assert_eq!(product.policy_versions, vec![1]);
    assert_eq!(product.active_product_workers, 3);
    assert_eq!(
        product.startup_trace.last(),
        Some(&VNextStartupPhase::Running)
    );
    assert!(!product.changes_wallet_state);
    assert!(!product.changes_obt_state);
    node.shutdown_network().await;
    assert!(node.listener_addr().is_none());
    assert!(node.vnext_listener_addr().is_none());
    assert!(node.vnext_product_runtime_status().unwrap().is_none());
}

#[tokio::test]
async fn active_product_runtime_requires_vault_and_policy_dependencies() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = NodeConfig::default();
    config.port = 0;
    config.data_dir = directory.path().to_path_buf();
    config.concept_registry_mode = ConceptRegistryMode::Disabled;
    config.vnext.enabled.object_event_v1 = true;
    config.vnext.enabled.obp_rp = true;

    let mut node = OneBrainNode::new(config).await.unwrap();
    let error = node.start_network().await.unwrap_err().to_string();
    assert!(error.contains("Vault and Policy dependencies"));
    assert!(node.vnext_listener_addr().is_none());
    assert!(!directory.path().join("vnext_identity.key").exists());
    assert!(!directory.path().join("vnext_verified.redb").exists());
}

#[tokio::test]
async fn inactive_vnext_creates_no_product_runtime_resources() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = NodeConfig::default();
    config.port = 0;
    config.data_dir = directory.path().to_path_buf();
    config.concept_registry_mode = ConceptRegistryMode::Disabled;

    let mut node = OneBrainNode::new(config).await.unwrap();
    node.start_network().await.unwrap();
    assert!(node.vnext_listener_addr().is_none());
    assert!(node.vnext_product_runtime_status().unwrap().is_none());
    for file in [
        "vnext_identity.key",
        "vnext_private_need_vault.redb",
        "vnext_distributed_kql.redb",
        "vnext_public_use_sender.redb",
        "vnext_distributed_pomv.redb",
        "vnext_verified.redb",
    ] {
        assert!(!directory.path().join(file).exists(), "unexpected {file}");
    }
}

#[tokio::test]
async fn legacy_tcp_and_vnext_quic_exchange_concurrently_on_real_loopback() {
    let left_dir = tempfile::tempdir().unwrap();
    let right_dir = tempfile::tempdir().unwrap();
    let mut left_config = NodeConfig {
        name: "mixed-left".into(),
        port: 0,
        data_dir: left_dir.path().to_path_buf(),
        concept_registry_mode: ConceptRegistryMode::Disabled,
        ..NodeConfig::default()
    };
    left_config.vnext.enabled.object_event_v1 = true;
    left_config.vnext.enabled.obp_rp = true;
    let mut right_config = left_config.clone();
    right_config.name = "mixed-right".into();
    right_config.data_dir = right_dir.path().to_path_buf();

    let mut left = OneBrainNode::new(left_config).await.unwrap();
    let mut right = OneBrainNode::new(right_config).await.unwrap();
    left.set_vnext_product_dependencies(product_dependencies())
        .unwrap();
    right
        .set_vnext_product_dependencies(product_dependencies())
        .unwrap();
    let left_tcp = left.start_network().await.unwrap();
    let right_tcp = right.start_network().await.unwrap();
    let left_quic = left.vnext_listener_addr().unwrap();
    let right_quic = right.vnext_listener_addr().unwrap();
    assert_ne!(left_tcp, left_quic);
    assert_ne!(right_tcp, right_quic);

    let right_tcp_loopback = ([127, 0, 0, 1], right_tcp.port()).into();
    let right_quic_loopback = ([127, 0, 0, 1], right_quic.port()).into();
    let (legacy, vnext) = tokio::join!(
        left.connect_to_seed(right_tcp_loopback),
        left.connect_vnext_peer(right_quic_loopback)
    );
    legacy.unwrap();
    let vnext = vnext.unwrap();
    vnext.close();
    assert_eq!(left.listener_addr(), Some(left_tcp));
    assert_eq!(left.vnext_listener_addr(), Some(left_quic));

    left.rollback_vnext_runtime().unwrap();
    assert!(!left.vnext_status().features.obp_rp);
    left.connect_to_seed(right_tcp_loopback).await.unwrap();
    assert!(
        left.connect_vnext_peer(right_quic_loopback).await.is_err(),
        "runtime rollback did not fence the vNext network lane"
    );
    left.reenable_vnext_runtime_lane(VNextRuntimeLane::Network)
        .unwrap();
    assert!(left.vnext_status().features.obp_rp);
    left.connect_vnext_peer(right_quic_loopback)
        .await
        .unwrap()
        .close();

    left.shutdown_network().await;
    right.shutdown_network().await;
}

#[tokio::test]
async fn legacy_tcp_bind_failure_rolls_back_successful_vnext_startup() {
    let occupied = TcpListener::bind("0.0.0.0:0").await.unwrap();
    let occupied_port = occupied.local_addr().unwrap().port();
    let directory = tempfile::tempdir().unwrap();
    let mut config = NodeConfig::default();
    config.port = occupied_port;
    config.data_dir = directory.path().to_path_buf();
    config.concept_registry_mode = ConceptRegistryMode::Disabled;
    config.vnext.enabled.object_event_v1 = true;
    config.vnext.enabled.obp_rp = true;
    config.vnext.enabled.distributed_kql_one_hop = true;
    config.vnext.enabled.public_use_evidence_publish = true;
    config.vnext.enabled.distributed_pomv_view = true;

    let mut node = OneBrainNode::new(config).await.unwrap();
    node.set_vnext_product_dependencies(product_dependencies())
        .unwrap();
    let error = node.start_network().await.unwrap_err().to_string();
    assert!(error.contains("Failed to bind TCP"));
    assert!(node.listener_addr().is_none());
    assert!(node.vnext_listener_addr().is_none());
    for file in [
        "vnext_identity.key",
        "vnext_private_need_vault.redb",
        "vnext_distributed_kql.redb",
        "vnext_public_use_sender.redb",
        "vnext_distributed_pomv.redb",
        "vnext_verified.redb",
        "vnext_reconciliation.redb",
        "vnext_inventory.redb",
        "vnext_record_provenance.redb",
        "vnext_outbox.redb",
    ] {
        assert!(!directory.path().join(file).exists(), "unexpected {file}");
    }
}
