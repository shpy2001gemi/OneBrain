#![cfg(feature = "vnext-outbound-first")]

use std::net::SocketAddr;

use ku_core::foundation::{DisclosureClass, NamespaceCommitment, NodeId, SelectorCid};
use onebrain_node::vnext_config::VNextNetworkPolicy;
use onebrain_node::vnext_network_runtime::VNextNetworkRuntime;
use onebrain_node::vnext_outbox::{OutboundIntentState, OutboundOutbox, OutboundTransferIntent};
use onebrain_protocol::{
    DirectCandidateKindV1, DirectCandidateV1, HostAddressV1, ReachabilityEndpointV1,
    ReconcileManifestKind, ReconcileReceiptStatus, RoutePathKindV1,
};

fn intent(peer: NodeId) -> OutboundTransferIntent {
    OutboundTransferIntent::new(
        peer,
        "127.0.0.1:42001".parse::<SocketAddr>().unwrap(),
        SelectorCid::from_bytes([2; 32]),
        NamespaceCommitment::from_bytes([3; 32]),
        DisclosureClass::Public,
        ReconcileManifestKind::Object,
        b"task-11-checkpoint-payload".to_vec(),
    )
    .unwrap()
}

#[test]
fn receipt_and_checkpoint_commit_atomically_and_survive_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vnext_outbox.redb");
    let peer = NodeId::from_bytes([7; 32]);
    let transfer = intent(peer);
    let route_root = [9; 32];

    {
        let outbox = OutboundOutbox::open(&path).unwrap();
        outbox.enqueue(&transfer).unwrap();
        let checkpoint = outbox
            .apply_receipt_and_checkpoint(
                &transfer.id,
                ReconcileReceiptStatus::ValidatedStored,
                3,
                1,
                route_root,
            )
            .unwrap();
        assert_eq!(checkpoint.expected_peer(), peer);
        assert_eq!(checkpoint.acknowledged_intent_id(), transfer.id);
        assert_eq!(checkpoint.acknowledged_sequence(), 1);
        assert_eq!(checkpoint.route_journal_root(), route_root);
        assert_ne!(checkpoint.outbox_state_root(), [0; 32]);
        assert_ne!(checkpoint.checkpoint_digest(), [0; 32]);
    }

    let reopened = OutboundOutbox::open(&path).unwrap();
    assert_eq!(
        reopened.get(&transfer.id).unwrap().unwrap().state,
        OutboundIntentState::Acknowledged
    );
    let checkpoint = reopened.latest_checkpoint(peer).unwrap().unwrap();
    assert_eq!(checkpoint.acknowledged_sequence(), 1);
    assert_eq!(checkpoint.route_journal_root(), route_root);
}

#[test]
fn checkpoint_sequence_replay_and_peer_substitution_fail_closed() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vnext_outbox.redb");
    let peer = NodeId::from_bytes([11; 32]);
    let first = intent(peer);
    let outbox = OutboundOutbox::open(&path).unwrap();
    outbox.enqueue(&first).unwrap();
    outbox
        .apply_receipt_and_checkpoint(
            &first.id,
            ReconcileReceiptStatus::AlreadyPresent,
            3,
            1,
            [12; 32],
        )
        .unwrap();

    let replay = outbox.apply_receipt_and_checkpoint(
        &first.id,
        ReconcileReceiptStatus::AlreadyPresent,
        3,
        1,
        [12; 32],
    );
    assert!(replay.is_err());
    assert!(outbox
        .latest_checkpoint(NodeId::from_bytes([99; 32]))
        .unwrap()
        .is_none());
}

#[test]
fn route_journal_is_bounded_and_content_addressed() {
    use onebrain_node::vnext_route_journal::{RouteJournal, RouteJournalEntryV1};

    let journal = RouteJournal::new(2, 4096).unwrap();
    let peer = NodeId::from_bytes([21; 32]);
    journal
        .append(RouteJournalEntryV1::direct(peer, [22; 32], 1, 100))
        .unwrap();
    let first_root = journal.root().unwrap();
    journal
        .append(RouteJournalEntryV1::direct(peer, [23; 32], 2, 101))
        .unwrap();
    assert_ne!(journal.root().unwrap(), first_root);
    assert!(journal
        .append(RouteJournalEntryV1::direct(peer, [24; 32], 3, 102))
        .is_err());
}

#[test]
fn route_journal_preserves_the_authenticated_carrier_class() {
    use onebrain_node::vnext_route_journal::{RouteJournal, RouteJournalEntryV1};

    let journal = RouteJournal::new(4, 4096).unwrap();
    let peer = NodeId::from_bytes([25; 32]);
    let entry = RouteJournalEntryV1::routed(peer, RoutePathKindV1::RelayUdp, [26; 32], 1, 103);
    assert_eq!(entry.path_kind(), RoutePathKindV1::RelayUdp);
    journal.append(entry).unwrap();
    assert_ne!(journal.root().unwrap(), [0; 32]);
}

#[test]
fn route_journal_survives_restart_with_the_same_root() {
    use onebrain_node::vnext_route_journal::{RouteJournal, RouteJournalEntryV1};

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vnext_route_journal.redb");
    let peer = NodeId::from_bytes([31; 32]);
    let other_peer = NodeId::from_bytes([41; 32]);
    let root = {
        let journal = RouteJournal::open(&path, 8, 8192).unwrap();
        journal
            .append(RouteJournalEntryV1::direct(peer, [32; 32], 1, 200))
            .unwrap();
        journal
            .append(RouteJournalEntryV1::direct(other_peer, [42; 32], 1, 201))
            .unwrap();
        journal
            .append(RouteJournalEntryV1::direct(peer, [33; 32], 2, 202))
            .unwrap()
    };
    let reopened = RouteJournal::open(&path, 8, 8192).unwrap();
    assert_eq!(reopened.len().unwrap(), 3);
    assert_eq!(reopened.root().unwrap(), root);
    assert!(reopened
        .append(RouteJournalEntryV1::direct(peer, [34; 32], 2, 203))
        .is_err());
}

#[test]
fn recovery_gate_closes_writes_until_fresh_route_is_resumed() {
    use onebrain_node::vnext_connection_planner::{RoutedDeliveryGate, RoutedDeliveryState};

    let gate = RoutedDeliveryGate::default();
    assert!(gate.writes_open().unwrap());
    gate.transition(RoutedDeliveryState::Quiescing).unwrap();
    assert!(!gate.writes_open().unwrap());
    gate.transition(RoutedDeliveryState::Replanning).unwrap();
    gate.transition(RoutedDeliveryState::Reauthenticating)
        .unwrap();
    gate.transition(RoutedDeliveryState::Resuming).unwrap();
    assert!(!gate.writes_open().unwrap());
    gate.transition(RoutedDeliveryState::Active).unwrap();
    assert!(gate.writes_open().unwrap());
    assert!(gate.transition(RoutedDeliveryState::Active).is_err());
}

#[tokio::test]
async fn expected_peer_identity_is_checked_before_route_authority_mutates() {
    let left_dir = tempfile::tempdir().unwrap();
    let right_dir = tempfile::tempdir().unwrap();
    let mut left = VNextNetworkRuntime::start(
        left_dir.path(),
        "127.0.0.1:0".parse().unwrap(),
        VNextNetworkPolicy::default(),
    )
    .await
    .unwrap();
    let mut right = VNextNetworkRuntime::start(
        right_dir.path(),
        "127.0.0.1:0".parse().unwrap(),
        VNextNetworkPolicy::default(),
    )
    .await
    .unwrap();

    let wrong = left
        .connect_expected(NodeId::from_bytes([0xEE; 32]), right.local_addr())
        .await;
    assert!(wrong.is_err());
    assert_eq!(left.authenticated_routed_route_count().unwrap(), 0);

    let right_peer = NodeId::from_bytes(right.status().principal);
    let routed = left
        .connect_expected(right_peer, right.local_addr())
        .await
        .unwrap();
    assert_eq!(routed.expected_peer(), right_peer);
    assert_eq!(left.authenticated_routed_route_count().unwrap(), 1);
    let authority = left
        .authenticated_routed_route(right_peer)
        .unwrap()
        .unwrap();
    assert_eq!(authority.peer, right_peer);
    assert_eq!(authority.session_id, routed.authenticated().session_id);

    routed.close();
    left.shutdown().await;
    right.shutdown().await;
}

#[tokio::test]
async fn armed_direct_inbound_is_single_use_peer_bound_and_preserves_the_connection() {
    let left_dir = tempfile::tempdir().unwrap();
    let right_dir = tempfile::tempdir().unwrap();
    let mut left = VNextNetworkRuntime::start(
        left_dir.path(),
        "127.0.0.1:0".parse().unwrap(),
        VNextNetworkPolicy::default(),
    )
    .await
    .unwrap();
    let mut right = VNextNetworkRuntime::start(
        right_dir.path(),
        "127.0.0.1:0".parse().unwrap(),
        VNextNetworkPolicy::default(),
    )
    .await
    .unwrap();
    let left_peer = NodeId::from_bytes(left.status().principal);
    let right_peer = NodeId::from_bytes(right.status().principal);
    let armed = right.arm_expected_direct(left_peer).unwrap();
    assert!(right.arm_expected_direct(left_peer).is_err());

    let (outbound, inbound) = tokio::join!(
        left.connect_expected(right_peer, right.local_addr()),
        right.accept_armed_direct(armed),
    );
    let outbound = outbound.unwrap();
    let inbound = inbound.unwrap();
    assert_eq!(outbound.carrier().path_kind(), RoutePathKindV1::Direct);
    assert_eq!(inbound.carrier().path_kind(), RoutePathKindV1::Direct);
    assert_eq!(
        outbound.authenticated().session_id,
        inbound.authenticated().session_id
    );
    outbound.send_uni(b"armed-direct-marker").await.unwrap();
    assert_eq!(inbound.recv_uni(64).await.unwrap(), b"armed-direct-marker");
    assert_eq!(right.authenticated_routed_route_count().unwrap(), 1);

    outbound.close();
    inbound.close();
    left.shutdown().await;
    right.shutdown().await;
}

#[tokio::test]
async fn sealed_selected_carrier_uses_the_same_identity_first_promotion_gate() {
    use ku_net::transport::{QuicTransport, TransportConfig};
    use ku_net::vnext_connection_executor::ConnectionPlannerExecutor;
    use ku_net::vnext_route_plan::PlannerAction;

    let left_dir = tempfile::tempdir().unwrap();
    let right_dir = tempfile::tempdir().unwrap();
    let mut left = VNextNetworkRuntime::start(
        left_dir.path(),
        "127.0.0.1:0".parse().unwrap(),
        VNextNetworkPolicy::default(),
    )
    .await
    .unwrap();
    let mut right = VNextNetworkRuntime::start(
        right_dir.path(),
        "127.0.0.1:0".parse().unwrap(),
        VNextNetworkPolicy::default(),
    )
    .await
    .unwrap();
    let dialer = QuicTransport::bind(TransportConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..TransportConfig::default()
    })
    .await
    .unwrap();
    let address = right.local_addr();
    let connection = dialer.connect(address).await.unwrap();
    let candidate = DirectCandidateV1 {
        endpoint: ReachabilityEndpointV1 {
            host: match address.ip() {
                std::net::IpAddr::V4(value) => HostAddressV1::Ipv4(value.octets()),
                std::net::IpAddr::V6(value) => HostAddressV1::Ipv6(value.octets()),
            },
            port: address.port(),
        },
        kind: DirectCandidateKindV1::Host,
        priority: 1,
        network_epoch: 1,
        expires_at: u64::MAX,
    };
    let selected = ConnectionPlannerExecutor::seal_connected_direct(
        PlannerAction::CheckDirect(candidate),
        connection,
        Vec::new(),
    )
    .unwrap();
    let right_peer = NodeId::from_bytes(right.status().principal);
    let routed = left
        .connect_expected_selected(right_peer, selected)
        .await
        .unwrap();
    assert_eq!(routed.carrier().path_kind(), RoutePathKindV1::Direct);
    assert_eq!(left.authenticated_routed_route_count().unwrap(), 1);

    routed.close();
    dialer.shutdown().await;
    left.shutdown().await;
    right.shutdown().await;
}
