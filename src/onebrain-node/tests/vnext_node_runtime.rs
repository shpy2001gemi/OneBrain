#![cfg(feature = "vnext-network-runtime")]

use onebrain_node::{
    ConceptRegistryMode, NetworkRuntimeLifecycle, NodeConfig, OneBrainNode, VNextFeature,
};

#[tokio::test]
async fn feature_flags_start_a_real_node_owned_listener_and_status_tracks_it() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = NodeConfig::default();
    config.name = "vnext-node-test".into();
    config.port = 0;
    config.data_dir = directory.path().to_path_buf();
    config.concept_registry_mode = ConceptRegistryMode::Disabled;
    config.vnext.enabled.object_event_v1 = true;
    config.vnext.enabled.obp_rp = true;
    assert!(config.vnext.is_active(VNextFeature::ObpRp));

    let mut node = OneBrainNode::new(config).await.unwrap();
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
    assert!(!after.network_runtime.claims_network_completion);
}
