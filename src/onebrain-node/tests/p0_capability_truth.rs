use onebrain_node::{types::WalletEconomicStatus, ConceptRegistryMode, NodeConfig, OneBrainNode};

#[tokio::test]
async fn legacy_wallet_is_explicitly_non_economic_and_mutations_fail_closed() {
    let directory = tempfile::tempdir().unwrap();
    let config = NodeConfig {
        name: "p0-capability-truth".into(),
        port: 0,
        data_dir: directory.path().to_path_buf(),
        concept_registry_mode: ConceptRegistryMode::Disabled,
        ..NodeConfig::default()
    };

    let mut node = OneBrainNode::new(config).await.unwrap();
    let wallet = node.get_balance().unwrap();

    assert_eq!(
        wallet.economic_status,
        WalletEconomicStatus::SimulatedNonEconomic
    );
    assert!(!wallet.limitations.is_empty());

    let json = serde_json::to_value(&wallet).unwrap();
    assert_eq!(
        json["economic_status"],
        serde_json::json!("simulated_non_economic")
    );

    let stake_error = node.stake(1).unwrap_err().to_string();
    assert!(stake_error.contains("disabled"));
    assert!(stake_error.contains("non-economic"));

    let unstake_error = node.unstake(1).unwrap_err().to_string();
    assert!(unstake_error.contains("disabled"));
    assert!(unstake_error.contains("non-economic"));
}
