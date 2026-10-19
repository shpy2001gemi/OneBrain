//! ARCH-04: the legacy textual KQL surface (`/api/kql`, CLI `kql`) is local
//! and read-only. Writes and standing queries are refused instead of mutating
//! a temporary copy and reporting success.

use onebrain_node::{ConceptRegistryMode, NodeConfig, OneBrainNode};

#[tokio::test]
async fn legacy_kql_is_read_only_and_documented_examples_run() {
    let directory = tempfile::tempdir().unwrap();
    let node = OneBrainNode::new(NodeConfig {
        name: "legacy-kql-read-only".into(),
        port: 0,
        data_dir: directory.path().to_path_buf(),
        concept_registry_mode: ConceptRegistryMode::Disabled,
        ..NodeConfig::default()
    })
    .await
    .unwrap();
    let before = node.ku_count().unwrap();

    for example in ku_kql::read_only::DOCUMENTED_EXAMPLES {
        node.execute_kql(example)
            .unwrap_or_else(|error| panic!("{example} -> {error}"));
    }

    for write in [
        "CREATE (k:KU) { TRIPLE(a, b, c) }",
        "UPDATE (k:KU) SET k.trust_score = 1 SIGNED BY me",
        "DEPRECATE (k:KU) REASON \"x\" SIGNED BY me",
        "WATCH FIND (k:KU) ON CREATE NOTIFY \"x\"",
    ] {
        let error = node.execute_kql(write).unwrap_err().to_string();
        assert!(error.contains("read-only"), "{write} -> {error}");
    }
    let error = node
        .execute_kql("FIND (k:KU) WHERE trust > 0.8")
        .unwrap_err()
        .to_string();
    assert!(error.contains("unknown"), "{error}");

    assert_eq!(node.ku_count().unwrap(), before);
}
