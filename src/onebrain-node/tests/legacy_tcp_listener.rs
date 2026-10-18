//! ARCH-02: the unauthenticated legacy TCP/JSON listener is opt-in.

use onebrain_node::network::{recv_message, send_message, NetMessage};
use onebrain_node::{ConceptRegistryMode, LegacyTcpListener, NodeConfig, OneBrainNode};
use tokio::net::TcpStream;

fn config(directory: &std::path::Path, listener: LegacyTcpListener) -> NodeConfig {
    NodeConfig {
        name: "legacy-listener-test".into(),
        port: 0,
        data_dir: directory.to_path_buf(),
        concept_registry_mode: ConceptRegistryMode::Disabled,
        legacy_tcp_listener: listener,
        ..NodeConfig::default()
    }
}

#[test]
fn default_and_legacy_serialized_configs_keep_the_listener_closed() {
    assert_eq!(
        NodeConfig::default().legacy_tcp_listener,
        LegacyTcpListener::Disabled
    );
    let legacy: NodeConfig = serde_json::from_str(
        r#"{
            "name":"old",
            "port":4242,
            "data_dir":"./data",
            "ollama_url":"http://localhost:11434",
            "model":"test-model",
            "seeds":[]
        }"#,
    )
    .unwrap();
    assert_eq!(legacy.legacy_tcp_listener, LegacyTcpListener::Disabled);
    assert_eq!(LegacyTcpListener::Disabled.bind_addr(4242), None);
    assert_eq!(
        "all-interfaces".parse::<LegacyTcpListener>(),
        Ok(LegacyTcpListener::AllInterfaces)
    );
    assert!("public".parse::<LegacyTcpListener>().is_err());
}

#[tokio::test]
async fn default_start_opens_no_unauthenticated_port() {
    let directory = tempfile::tempdir().unwrap();
    let mut node = OneBrainNode::new(config(directory.path(), LegacyTcpListener::Disabled))
        .await
        .unwrap();
    assert!(!node.network_listener_requested());
    let error = node.start_network().await.unwrap_err().to_string();
    assert!(
        error.contains("no network listener requested"),
        "unexpected error: {error}"
    );
    assert!(node.listener_addr().is_none());
}

#[tokio::test]
async fn loopback_opt_in_serves_legacy_peers_on_127_0_0_1_only() {
    let directory = tempfile::tempdir().unwrap();
    let mut node = OneBrainNode::new(config(directory.path(), LegacyTcpListener::Loopback))
        .await
        .unwrap();
    assert!(node.network_listener_requested());
    let addr = node.start_network().await.unwrap();
    assert!(addr.ip().is_loopback(), "bound {addr}");
    assert_eq!(node.listener_addr(), Some(addr));

    // The opted-in legacy path still works end to end.
    let mut stream = TcpStream::connect(addr).await.unwrap();
    send_message(
        &mut stream,
        &NetMessage::PeerHello {
            name: "legacy-peer".into(),
            port: 0,
            ku_count: 0,
        },
    )
    .await
    .unwrap();
    let reply = tokio::time::timeout(std::time::Duration::from_secs(5), recv_message(&mut stream))
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(reply, NetMessage::PeerHello { .. }));
    node.shutdown_network().await;
    assert!(node.listener_addr().is_none());
}

#[tokio::test]
async fn all_interfaces_requires_an_explicit_choice() {
    let directory = tempfile::tempdir().unwrap();
    let mut node = OneBrainNode::new(config(directory.path(), LegacyTcpListener::AllInterfaces))
        .await
        .unwrap();
    let addr = node.start_network().await.unwrap();
    assert!(addr.ip().is_unspecified(), "bound {addr}");
    node.shutdown_network().await;
}

#[tokio::test]
async fn declared_length_alone_does_not_hold_the_legacy_listener() {
    use tokio::io::AsyncWriteExt;
    let directory = tempfile::tempdir().unwrap();
    let mut node = OneBrainNode::new(config(directory.path(), LegacyTcpListener::Loopback))
        .await
        .unwrap();
    let addr = node.start_network().await.unwrap();
    // A peer that announces a 16 MiB message and then disconnects must not
    // break service for the next peer.
    let mut abusive = TcpStream::connect(addr).await.unwrap();
    abusive
        .write_all(&(16u32 * 1024 * 1024).to_be_bytes())
        .await
        .unwrap();
    drop(abusive);
    let mut stream = TcpStream::connect(addr).await.unwrap();
    send_message(
        &mut stream,
        &NetMessage::PeerHello {
            name: "after-abuse".into(),
            port: 0,
            ku_count: 0,
        },
    )
    .await
    .unwrap();
    let reply = tokio::time::timeout(std::time::Duration::from_secs(5), recv_message(&mut stream))
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(reply, NetMessage::PeerHello { .. }));
    node.shutdown_network().await;
}
