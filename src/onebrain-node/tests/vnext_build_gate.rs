#[cfg(not(feature = "vnext-network-runtime"))]
#[tokio::test]
async fn active_obp_rp_fails_before_runtime_side_effects_when_build_support_is_absent() {
    use onebrain_node::{ConceptRegistryMode, NodeConfig, OneBrainNode};

    let directory = tempfile::tempdir().unwrap();
    let data_dir = directory.path().join("must-not-be-created");
    let mut config = NodeConfig {
        data_dir: data_dir.clone(),
        concept_registry_mode: ConceptRegistryMode::Disabled,
        ..NodeConfig::default()
    };
    config.vnext.enabled.object_event_v1 = true;
    config.vnext.enabled.obp_rp = true;

    let error = OneBrainNode::new(config).await.err().expect("build gate");
    assert!(error.to_string().contains("vnext-network-runtime"));
    assert!(!data_dir.exists());
}
