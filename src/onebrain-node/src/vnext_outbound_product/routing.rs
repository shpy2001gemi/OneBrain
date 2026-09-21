//! Product routing owns no second transport or outbox. Only validated peer
//! advertisements reach the existing planner and expected-peer handshake.

use super::*;
use crate::vnext_connection_planner::{
    ExpectedPeerCarrierSelector, ProductionExpectedPeerCarrierSelector, RoutedVNextSession,
};
use crate::vnext_network_runtime::{
    ProductRouteConnector, VNextNetworkRuntime, VNextNetworkRuntimeError,
};
use crate::vnext_product_runtime::VNextProductRuntimeError;
use futures::{stream::FuturesUnordered, StreamExt};
use ku_core::foundation::NodeId;
use ku_net::vnext_connection_executor::{
    ConnectionPlannerExecutor, ProductionRelayAssociationClient, ProductionRelayCarrierDialer,
    QuicDirectCarrierDialer,
};
use ku_net::vnext_relay_discovery::ReachabilityFuture;
use ku_net::vnext_relay_discovery::VerifiedRelayDiscovery;
use std::sync::Weak;

pub(super) struct RoutingOwner {
    pub(super) executor: Arc<ConnectionPlannerExecutor>,
    selector: ProductionExpectedPeerCarrierSelector,
    binding: Mutex<Option<(Weak<VNextNetworkRuntime>, VNextRuntimeRollout)>>,
}

impl RoutingOwner {
    pub(super) fn new(
        validator: Arc<ReachabilityDialValidator>,
        transport: Arc<QuicTransport>,
        signer: Arc<dyn ReachabilityIdentitySigner>,
        manager: &ReachabilityManager,
        optional_paths: Option<Arc<dyn crate::vnext_connection_planner::OptionalPeerPaths>>,
    ) -> Result<Self, &'static str> {
        let executor = Arc::new(ConnectionPlannerExecutor::new(
            Arc::new(QuicDirectCarrierDialer::new(transport)),
            Arc::new(ProductionRelayCarrierDialer::standard()),
            Arc::new(ProductionRelayAssociationClient::new(signer.public_key())),
        ));
        let mut selector = ProductionExpectedPeerCarrierSelector::new(
            validator,
            executor.clone(),
            Duration::from_secs(20),
        )
        .and_then(|selector| {
            selector.with_relay(
                manager.discovery.clone(),
                manager.reservations.clone(),
                signer,
                1,
            )
        })
        .map_err(|_| "routing_planner")?;
        if let Some(paths) = optional_paths {
            selector = selector.with_optional_paths(paths);
        }
        Ok(Self {
            executor,
            selector,
            binding: Mutex::new(None),
        })
    }
}

impl OutboundFirstOwner {
    pub(super) async fn run_inbound(
        &self,
        mut cancel: watch::Receiver<bool>,
        rollout: VNextRuntimeRollout,
    ) {
        let mut cursor = 0usize;
        loop {
            if *cancel.borrow() {
                return;
            }
            let mut grant = self.grant.clone();
            grant.borrow_and_update();
            let round = async {
                if !self.granted() {
                    return;
                }
                let Ok(generation) = rollout.acquire(VNextRuntimeLane::Network) else {
                    return;
                };
                let Ok((network, _)) = self.network_binding() else {
                    return;
                };
                let epoch = self.manager.current_epoch();
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|v| v.as_secs())
                    .unwrap_or(0);
                let discovery = self.discovery.lock().await;
                let peers = discovery.peer_advertisements(now);
                let identities = discovery.peer_identities();
                drop(discovery);
                let mut candidates = Vec::new();
                for peer in peers {
                    let Some(identity) = identities
                        .iter()
                        .find(|identity| identity.node_id == peer.canonical().target_node_id)
                    else {
                        continue;
                    };
                    for remote in peer.reservations() {
                        let relay = remote.canonical().relay_node_id;
                        let descriptor = self
                            .manager
                            .discovery
                            .read()
                            .await
                            .verified_relays()
                            .find(|d| d.canonical().relay_node_id == relay)
                            .cloned();
                        let Some(descriptor) = descriptor else {
                            continue;
                        };
                        let Some((local, outer)) =
                            self.manager.reservations.active_for(relay).await
                        else {
                            continue;
                        };
                        candidates.push((
                            descriptor,
                            remote.clone(),
                            local,
                            identity.clone(),
                            outer,
                        ));
                    }
                }
                if candidates.is_empty() {
                    return;
                }
                let count = candidates.len();
                candidates.rotate_left(cursor % count);
                cursor = (cursor + 4) % count;
                let mut attempts = FuturesUnordered::new();
                for (descriptor, remote, local, identity, outer) in candidates.into_iter().take(4) {
                    let network = network.clone();
                    let generation = &generation;
                    attempts.push(async move {
                        let endpoint = outer.public_endpoint();
                        let candidate = onebrain_protocol::RelayCandidateV1 {
                            relay_node_id: descriptor.canonical().relay_node_id,
                            reservation_id: remote.canonical().reservation_id,
                            transport: outer.transport(),
                            endpoint: onebrain_protocol::ReachabilityEndpointV1 {
                                host: endpoint.host.clone(),
                                port: endpoint.port,
                            },
                            priority: 1,
                            expires_at: local
                                .canonical()
                                .expires_at
                                .min(remote.canonical().expires_at),
                        };
                        let peer = identity.node_id;
                        let carrier = self
                            .route
                            .executor
                            .accept_relay_inbound(
                                descriptor,
                                remote,
                                local,
                                identity,
                                candidate,
                                outer,
                                std::time::Instant::now() + Duration::from_secs(20),
                            )
                            .await
                            .ok()?;
                        if !generation.is_current()
                            || !self.granted()
                            || epoch != self.manager.current_epoch()
                        {
                            return None;
                        }
                        let session = network.accept_expected_selected(peer, carrier).await.ok()?;
                        if !generation.is_current()
                            || !self.granted()
                            || epoch != self.manager.current_epoch()
                        {
                            session.close();
                            return None;
                        }
                        network.serve_product_inbound(session).await.ok()
                    });
                }
                while attempts.next().await.is_some() {}
            };
            tokio::select! {
                biased;
                _ = cancel.changed() => return,
                _ = grant.changed() => {},
                _ = tokio::time::timeout(Duration::from_secs(20), round) => {},
            }
            tokio::select! {
                _ = cancel.changed() => return,
                _ = tokio::time::sleep(Duration::from_secs(2)) => {},
            }
        }
    }

    pub(crate) fn attach_network(
        &self,
        network: Weak<VNextNetworkRuntime>,
        rollout: VNextRuntimeRollout,
    ) -> Result<(), VNextProductRuntimeError> {
        let mut binding = self
            .route
            .binding
            .lock()
            .map_err(|_| VNextProductRuntimeError::OutboundFirst("routing_binding"))?;
        if binding.is_some() {
            return Err(VNextProductRuntimeError::OutboundFirst(
                "routing_already_bound",
            ));
        }
        *binding = Some((network, rollout));
        Ok(())
    }

    fn network_binding(
        &self,
    ) -> Result<(Arc<VNextNetworkRuntime>, VNextRuntimeRollout), VNextNetworkRuntimeError> {
        let binding = self
            .route
            .binding
            .lock()
            .map_err(|_| VNextNetworkRuntimeError::ReplayGuard)?;
        let (network, rollout) = binding
            .as_ref()
            .ok_or_else(|| VNextNetworkRuntimeError::RuntimeFenced("routing not bound".into()))?;
        Ok((
            network
                .upgrade()
                .ok_or_else(|| VNextNetworkRuntimeError::RuntimeFenced("runtime stopped".into()))?,
            rollout.clone(),
        ))
    }
}

impl ProductRouteConnector for OutboundFirstOwner {
    fn network_epoch(&self) -> Option<u64> {
        Some(self.manager.current_epoch().get())
    }
    fn execution_grant(&self) -> Option<watch::Receiver<bool>> {
        let mut grant = self.grant.clone();
        grant.borrow_and_update();
        Some(grant)
    }

    fn ready(&self, peer: NodeId) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|v| v.as_secs())
            .unwrap_or(u64::MAX);
        self.discovery.try_lock().is_ok_and(|discovery| {
            discovery
                .peer_advertisements(now)
                .iter()
                .any(|ad| ad.canonical().target_node_id == peer)
        })
    }

    fn current(&self) -> bool {
        self.granted()
            && self
                .network_binding()
                .is_ok_and(|(_, rollout)| rollout.acquire(VNextRuntimeLane::Network).is_ok())
    }

    fn connect<'a>(
        &'a self,
        peer: NodeId,
    ) -> ReachabilityFuture<'a, Result<RoutedVNextSession, VNextNetworkRuntimeError>> {
        Box::pin(async move {
            let (network, rollout) = self.network_binding()?;
            let generation = rollout
                .acquire(VNextRuntimeLane::Network)
                .map_err(|e| VNextNetworkRuntimeError::RuntimeFenced(e.to_string()))?;
            if !self.granted() {
                return Err(VNextNetworkRuntimeError::RuntimeFenced(
                    "execution grant unavailable".into(),
                ));
            }
            let epoch = self.manager.current_epoch();
            let mut grant = self.grant.clone();
            grant.borrow_and_update();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| VNextNetworkRuntimeError::Config("clock".into()))?
                .as_secs();
            let advertisement = self
                .discovery
                .lock()
                .await
                .peer_advertisements(now)
                .into_iter()
                .find(|ad| ad.canonical().target_node_id == peer)
                .ok_or_else(|| VNextNetworkRuntimeError::Session("NoBootstrapReachable".into()))?;
            let result = tokio::select! {
                biased;
                _ = grant.changed() => Err(VNextNetworkRuntimeError::RuntimeFenced("execution grant changed".into())),
                result = tokio::time::timeout(Duration::from_secs(20), self.route.selector.connect_expected(&network, peer, &advertisement)) =>
                    result.unwrap_or(Err(VNextNetworkRuntimeError::HandshakeTimeout)),
            };
            let mut session = result?;
            if !generation.is_current() || !self.granted() || self.manager.current_epoch() != epoch
            {
                session.close();
                return Err(VNextNetworkRuntimeError::RuntimeFenced(
                    "route generation changed".into(),
                ));
            }
            let checkpoint = match network.outbound_checkpoint(peer) {
                Ok(checkpoint) => checkpoint,
                Err(error) => {
                    session.close();
                    return Err(error);
                }
            };
            if let Some(checkpoint) = checkpoint {
                if let Err(error) = session.attach_checkpoint(checkpoint) {
                    session.close();
                    return Err(VNextNetworkRuntimeError::Session(error.to_string()));
                }
            }
            Ok(session)
        })
    }
}
