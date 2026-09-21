use super::*;
use ku_core::foundation::{DisclosureClass, NamespaceCommitment, SelectorCid};
use onebrain_protocol::ReconcileManifestKind;
use std::sync::{atomic::AtomicBool, Weak};

struct RoutePort {
    runtime: Weak<VNextNetworkRuntime>,
    address: Mutex<SocketAddr>,
    granted: AtomicBool,
    discovered: AtomicBool,
    calls: AtomicUsize,
    epoch: AtomicU64,
}

impl ProductRouteConnector for RoutePort {
    fn network_epoch(&self) -> Option<u64> {
        Some(self.epoch.load(Ordering::SeqCst))
    }
    fn current(&self) -> bool {
        self.granted.load(Ordering::SeqCst)
    }
    fn ready(&self, _: NodeId) -> bool {
        self.discovered.load(Ordering::SeqCst)
    }
    fn connect<'a>(
        &'a self,
        peer: NodeId,
    ) -> ku_net::vnext_relay_discovery::ReachabilityFuture<
        'a,
        Result<RoutedVNextSession, VNextNetworkRuntimeError>,
    > {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let address = *self.address.lock().unwrap();
            self.runtime
                .upgrade()
                .unwrap()
                .connect_expected(peer, address)
                .await
        })
    }
}

fn transfer(peer: NodeId, marker: u8) -> OutboundTransferIntent {
    // Deliberately stale location: only the installed route port can find peer.
    OutboundTransferIntent::new(
        peer,
        "127.0.0.1:1".parse().unwrap(),
        SelectorCid::from_bytes([marker; 32]),
        NamespaceCommitment::from_bytes([4; 32]),
        DisclosureClass::Public,
        ReconcileManifestKind::FeedInception,
        tests::feed_and_event().0,
    )
    .unwrap()
}

async fn sender(path: &Path, target: SocketAddr) -> (Arc<VNextNetworkRuntime>, Arc<RoutePort>) {
    let runtime = Arc::new(
        VNextNetworkRuntime::start_inner(
            path,
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
            false,
        )
        .await
        .unwrap(),
    );
    let port = Arc::new(RoutePort {
        runtime: Arc::downgrade(&runtime),
        address: Mutex::new(target),
        granted: AtomicBool::new(true),
        discovered: AtomicBool::new(true),
        calls: AtomicUsize::new(0),
        epoch: AtomicU64::new(1),
    });
    runtime.install_product_routing(port.clone()).unwrap();
    (runtime, port)
}

async fn stop(runtime: Arc<VNextNetworkRuntime>) {
    let mut runtime = Arc::try_unwrap(runtime)
        .ok()
        .expect("no leaked runtime owner");
    runtime.shutdown().await;
}

#[tokio::test]
async fn routed_ack_checkpoint_and_pending_survive_restart_and_address_change() {
    let left = tempfile::tempdir().unwrap();
    let right = tempfile::tempdir().unwrap();
    let mut receiver = VNextNetworkRuntime::start(
        right.path(),
        "127.0.0.1:0".parse().unwrap(),
        VNextNetworkPolicy::default(),
    )
    .await
    .unwrap();
    let peer = NodeId::from_bytes(receiver.status().principal);
    let first = transfer(peer, 11);
    let next = transfer(peer, 12);
    let (runtime, _) = sender(left.path(), receiver.local_addr()).await;
    runtime.enqueue_outbound(&first).unwrap();
    assert_eq!(
        runtime.deliver_outbound_once(8).await.unwrap().acknowledged,
        1
    );
    let checkpoint = runtime.outbound_checkpoint(peer).unwrap().unwrap();
    assert_eq!(checkpoint.acknowledged_sequence(), 1);
    runtime.enqueue_outbound(&next).unwrap();
    assert_eq!(
        runtime.outbound_checkpoint(peer).unwrap(),
        Some(checkpoint.clone())
    );
    let session = runtime
        .authenticated_routed_route(peer)
        .unwrap()
        .unwrap()
        .session_id;
    stop(runtime).await;
    receiver.shutdown().await;
    drop(receiver);
    let mut receiver = VNextNetworkRuntime::start(
        right.path(),
        "127.0.0.1:0".parse().unwrap(),
        VNextNetworkPolicy::default(),
    )
    .await
    .unwrap();
    assert_eq!(NodeId::from_bytes(receiver.status().principal), peer);
    let (runtime, _) = sender(left.path(), receiver.local_addr()).await;
    assert_eq!(runtime.authenticated_routed_route_count().unwrap(), 0);
    assert_eq!(runtime.outbound_checkpoint(peer).unwrap(), Some(checkpoint));
    let report = runtime.deliver_outbound_once(8).await.unwrap();
    assert_eq!(report.scanned, 1);
    assert_eq!(report.acknowledged, 1);
    assert_ne!(
        runtime
            .authenticated_routed_route(peer)
            .unwrap()
            .unwrap()
            .session_id,
        session
    );
    assert_eq!(
        runtime
            .outbound_checkpoint(peer)
            .unwrap()
            .unwrap()
            .acknowledged_sequence(),
        2
    );
    assert_eq!(
        runtime.outbound_intent(&first.id).unwrap().unwrap().state,
        OutboundIntentState::Acknowledged
    );
    assert_eq!(
        runtime.outbound_intent(&next.id).unwrap().unwrap().state,
        OutboundIntentState::Acknowledged
    );
    let feed = ku_core::foundation::decode_feed_inception(&tests::feed_and_event().0).unwrap();
    assert_eq!(
        receiver.feed_inception_branch_count(feed.feed_id).unwrap(),
        1
    );
    stop(runtime).await;
    receiver.shutdown().await;
}

#[tokio::test]
async fn undiscovered_or_revoked_routes_preserve_pending_without_dial_or_budget_reset() {
    let left = tempfile::tempdir().unwrap();
    let (runtime, port) = sender(left.path(), "127.0.0.1:2".parse().unwrap()).await;
    let intent = transfer(NodeId::from_bytes([22; 32]), 1);
    runtime.enqueue_outbound(&intent).unwrap();
    port.discovered.store(false, Ordering::SeqCst);
    assert_eq!(runtime.deliver_outbound_once(1).await.unwrap().deferred, 1);
    port.discovered.store(true, Ordering::SeqCst);
    port.granted.store(false, Ordering::SeqCst);
    assert_eq!(runtime.deliver_outbound_once(1).await.unwrap().deferred, 1);
    assert_eq!(port.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        runtime
            .outbound_intent(&intent.id)
            .unwrap()
            .unwrap()
            .transport_attempts,
        0
    );
    assert!(runtime
        .outbound_checkpoint(intent.expected_peer)
        .unwrap()
        .is_none());
    stop(runtime).await;
}

#[tokio::test]
async fn wrong_peer_route_never_sends_or_creates_checkpoint_and_retry_is_bounded() {
    let left = tempfile::tempdir().unwrap();
    let right = tempfile::tempdir().unwrap();
    let mut receiver = VNextNetworkRuntime::start(
        right.path(),
        "127.0.0.1:0".parse().unwrap(),
        VNextNetworkPolicy::default(),
    )
    .await
    .unwrap();
    let (runtime, port) = sender(left.path(), receiver.local_addr()).await;
    let intent = transfer(NodeId::from_bytes([23; 32]), 1);
    runtime.enqueue_outbound(&intent).unwrap();
    let limit = runtime.outbound.policy.max_retries_per_record;
    for _ in 0..limit {
        assert_eq!(
            runtime.deliver_outbound_once(1).await.unwrap().acknowledged,
            0
        );
    }
    assert_eq!(
        runtime.outbound_intent(&intent.id).unwrap().unwrap().state,
        OutboundIntentState::RetryExhausted
    );
    let calls = port.calls.load(Ordering::SeqCst);
    assert_eq!(runtime.deliver_outbound_once(1).await.unwrap().scanned, 0);
    assert_eq!(port.calls.load(Ordering::SeqCst), calls);
    assert_eq!(receiver.status().accepted_records, 0);
    assert!(runtime
        .outbound_checkpoint(intent.expected_peer)
        .unwrap()
        .is_none());
    stop(runtime).await;
    receiver.shutdown().await;
}

#[tokio::test]
async fn old_session_stays_fenced_after_grant_reenabled_or_epoch_changed() {
    let left = tempfile::tempdir().unwrap();
    let right = tempfile::tempdir().unwrap();
    let mut receiver = VNextNetworkRuntime::start(
        right.path(),
        "127.0.0.1:0".parse().unwrap(),
        VNextNetworkPolicy::default(),
    )
    .await
    .unwrap();
    let peer = NodeId::from_bytes(receiver.status().principal);
    let (runtime, port) = sender(left.path(), receiver.local_addr()).await;
    let routed = port.connect(peer).await.unwrap();
    let mut session = runtime.outbound.routed_outbound(routed).unwrap();
    let (grant, rx) = watch::channel(true);
    session.product_grant = Some(port.clone());
    session.route_epoch = Some(1);
    session.execution_grant = Some(rx);
    assert!(session.ensure_runtime_generation().is_ok());
    grant.send(false).unwrap();
    grant.send(true).unwrap();
    assert!(session.ensure_runtime_generation().is_err());
    session.execution_grant = None;
    port.epoch.store(2, Ordering::SeqCst);
    assert!(session.ensure_runtime_generation().is_err());
    drop(session);
    stop(runtime).await;
    receiver.shutdown().await;
}
