//! Node-owned outbound-first lifecycle, discovery and standing reservations.
//! Construction never publishes or fabricates a live route; routing is separate.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ku_net::transport::QuicTransport;
use ku_net::vnext_reachability_crypto::{
    PublicEndpointResolver, ReachabilityAdmission, ReachabilityCryptoError,
    ReachabilityDialValidator, ReachabilityIdentitySigner,
};
use ku_net::vnext_reachability_resolver::ReachabilityAdvertisementResolver;
use ku_net::vnext_relay_discovery::{
    InMemoryAuthenticatedSessionRegistry, RelayDiscovery, RelayDiscoveryPolicy,
};
use ku_net::vnext_session::SessionIdentitySigner;
use tokio::sync::{watch, RwLock};

use crate::vnext_reachability_manager::{
    AdvertisementPublisher, CandidateGatherer, ProductionRelayDialRouteProvider,
    ProductionRelayReservationClient, ReachabilityManager, RelayReservationManager,
    VNextReachabilityPolicy,
};
use crate::vnext_reachability_replay_store::RedbReachabilityReplayStore;
use crate::vnext_runtime_rollout::{VNextRuntimeLane, VNextRuntimeRollout};

mod cache;
mod discovery;
#[cfg(test)]
mod discovery_tests;
mod routing;
mod standing;
pub use discovery::{
    DiscoveryInput, DiscoveryRecordSource, DiscoverySourceStatus, DiscoveryTransport,
    HttpsDiscoveryTransport,
};

/// Trusted host ports, supplied before node startup. Presence explicitly requests
/// outbound-first; absence is the default and creates no reachability worker/store.
/// Grant withdrawal cancels pending candidate collection. Ports must be bounded
/// and cancellation-safe; none may create detached background work.
pub struct OutboundFirstDependencies {
    pub(crate) gatherer: Arc<dyn CandidateGatherer>,
    pub(crate) publisher: Arc<dyn AdvertisementPublisher>,
    pub(crate) resolver: Arc<dyn PublicEndpointResolver>,
    pub(crate) execution_grant: watch::Receiver<bool>,
    pub(crate) advertise_reachability: bool,
    discovery_inputs: Vec<DiscoveryInput>,
    capability_ceiling: Option<[u8; 32]>,
    optional_paths: Option<Arc<dyn crate::vnext_connection_planner::OptionalPeerPaths>>,
}

impl OutboundFirstDependencies {
    pub fn new(
        gatherer: Arc<dyn CandidateGatherer>,
        publisher: Arc<dyn AdvertisementPublisher>,
        resolver: Arc<dyn PublicEndpointResolver>,
        execution_grant: watch::Receiver<bool>,
    ) -> Self {
        Self {
            gatherer,
            publisher,
            resolver,
            execution_grant,
            advertise_reachability: false,
            discovery_inputs: Vec::new(),
            capability_ceiling: None,
            optional_paths: None,
        }
    }

    /// Publication requires this explicit opt-in, live validated reservations
    /// and an independently supplied capability ceiling.
    pub fn with_advertising(mut self, requested: bool) -> Self {
        self.advertise_reachability = requested;
        self
    }

    pub fn with_discovery(mut self, inputs: Vec<DiscoveryInput>) -> Result<Self, &'static str> {
        discovery::validate_inputs(&inputs)?;
        self.discovery_inputs = inputs;
        Ok(self)
    }

    /// The trusted host supplies its existing capability commitment. This does
    /// not opt in to publication; `with_advertising(true)` remains separate.
    pub fn with_advertisement_capabilities(mut self, ceiling: [u8; 32]) -> Self {
        self.capability_ceiling = Some(ceiling);
        self
    }

    pub fn with_optional_paths(
        mut self,
        paths: Arc<dyn crate::vnext_connection_planner::OptionalPeerPaths>,
    ) -> Self {
        self.optional_paths = Some(paths);
        self
    }

    pub(crate) fn granted(&self) -> bool {
        self.execution_grant.has_changed().is_ok() && *self.execution_grant.borrow()
    }

    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        discovery::validate_inputs(&self.discovery_inputs)
    }
}

/// Redacted local projection; no transport, signer, raw candidate or store escapes.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct OutboundFirstStatus {
    pub compiled: bool,
    pub requested: bool,
    pub active: bool,
    pub kill_switch: bool,
    pub signer_ready: bool,
    pub generation: u64,
    pub lifecycle: &'static str,
    pub source_count: usize,
    pub usable_reservations: usize,
    pub authenticated_routes: usize,
    pub pending_intents: u64,
    pub advertisement_state: &'static str,
    pub coverage: &'static str,
    pub limitations: Vec<&'static str>,
    pub claims_global_completion: bool,
    pub authorizes_reward: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutboundReservationStatus {
    pub relay_node_id: ku_core::foundation::NodeId,
    pub state: &'static str,
    pub expires_at_unix_seconds: u64,
    pub limitations: Vec<&'static str>,
}

pub(crate) struct OutboundFirstOwner {
    product_enabled: AtomicBool,
    product_advertise: AtomicBool,
    pub(crate) manager: ReachabilityManager,
    route: routing::RoutingOwner,
    grant: watch::Receiver<bool>,
    limitation: Mutex<&'static str>,
    discovery: tokio::sync::Mutex<discovery::DiscoveryOwner>,
    pub(crate) source_statuses: Arc<RwLock<Vec<DiscoverySourceStatus>>>,
    standing: tokio::sync::Mutex<standing::StandingOwner>,
    pub(crate) advertisement_status: RwLock<(&'static str, Option<u64>)>,
    pub(crate) reservation_statuses: RwLock<Vec<OutboundReservationStatus>>,
}

impl OutboundFirstOwner {
    pub(crate) fn new(
        ports: OutboundFirstDependencies,
        signer: Arc<dyn SessionIdentitySigner>,
        replay: RedbReachabilityReplayStore,
        transport: Arc<QuicTransport>,
    ) -> Result<Self, &'static str> {
        let policy = VNextReachabilityPolicy::default();
        let signer: Arc<dyn ReachabilityIdentitySigner> = Arc::new(SessionSignerBridge {
            public_key: signer.public_key(),
            signer,
        });
        let replay = Arc::new(replay);
        let client = Arc::new(ProductionRelayReservationClient::with_admission(
            signer.clone(),
            ReachabilityAdmission::new(replay.clone()),
        ));
        client
            .attach_shared_quic_transport(transport.clone())
            .map_err(|_| "shared_transport")?;
        let preparer = Arc::new(
            ku_net::vnext_reachability_crypto::ReachabilityAdmissionPreparer::new(
                ports.resolver.clone(),
                policy.max_concurrent_checks,
            )
            .map_err(|_| "discovery_preparer")?,
        );
        let validator = Arc::new(
            ReachabilityDialValidator::new(ports.resolver, policy.max_concurrent_checks)
                .map_err(|_| "dial_validator")?,
        );
        let reservations = Arc::new(
            RelayReservationManager::new(
                client,
                Arc::new(ProductionRelayDialRouteProvider::new(validator.clone())),
                policy,
            )
            .map_err(|_| "reservation_owner")?,
        );
        let discovery_policy = RelayDiscoveryPolicy::default();
        let cached = discovery::cached_inputs(&ports.discovery_inputs, &replay)?;
        let recovery = Arc::new(
            cache::CacheRecovery::new(replay.clone(), &cached).map_err(|_| "discovery_cache")?,
        );
        for input in &ports.discovery_inputs {
            if let DiscoveryInput::ManualPeer { invitation, .. } = input {
                let peer = ku_net::vnext_relay_discovery::decode_manual_peer_invitation(invitation)
                    .map_err(|_| "peer_invitation")?;
                for bytes in replay
                    .cached_records(*peer.identity().node_id.as_bytes())
                    .map_err(|_| "discovery_cache")?
                {
                    if matches!(
                        onebrain_protocol::decode_reachability_object(&bytes),
                        Ok(onebrain_protocol::ReachabilityObjectV1::Advertisement(_))
                    ) {
                        recovery
                            .allow_advertisement(peer.identity().public_key, &bytes)
                            .map_err(|_| "discovery_cache")?;
                    }
                }
            }
            if let DiscoveryInput::Bootstrap { source, .. } = input {
                for bytes in replay
                    .cached_records(*source.source_id())
                    .map_err(|_| "discovery_cache")?
                {
                    if matches!(
                        onebrain_protocol::decode_reachability_object(&bytes),
                        Ok(onebrain_protocol::ReachabilityObjectV1::BootstrapManifest(
                            _
                        ))
                    ) {
                        recovery
                            .allow_manifest(*source.public_key(), &bytes)
                            .map_err(|_| "discovery_cache")?;
                    }
                }
            }
        }
        let sessions = Arc::new(InMemoryAuthenticatedSessionRegistry::default());
        let discovery = RelayDiscovery::new(
            discovery_policy.clone(),
            ReachabilityAdmission::new(recovery.clone()),
            sessions.clone(),
        );
        let discovery_owner = discovery::DiscoveryOwner::new(
            ports.discovery_inputs,
            ReachabilityAdmission::new(recovery),
            replay.clone(),
            preparer,
            validator.clone(),
            Arc::new(
                crate::vnext_reachability_manager::ProductionRelayPossessionClient::new(
                    signer.clone(),
                ),
            ),
            sessions,
        )?;
        let source_statuses = discovery_owner.statuses.clone();
        let standing =
            standing::StandingOwner::new(signer.clone(), replay, ports.capability_ceiling);
        let manager = ReachabilityManager::new(
            Arc::new(RwLock::new(discovery)),
            Arc::new(ReachabilityAdvertisementResolver::new(
                Vec::new(),
                discovery_policy,
            )),
            reservations,
            ports.gatherer,
            ports.publisher,
            signer.clone(),
            policy,
        )
        .map_err(|_| "reachability_owner")?;
        let route = routing::RoutingOwner::new(
            validator,
            transport,
            signer,
            &manager,
            ports.optional_paths,
        )?;
        Ok(Self {
            product_enabled: AtomicBool::new(true),
            product_advertise: AtomicBool::new(ports.advertise_reachability),
            manager,
            route,
            grant: ports.execution_grant,
            limitation: Mutex::new("candidate_collection_pending"),
            discovery: tokio::sync::Mutex::new(discovery_owner),
            source_statuses,
            standing: tokio::sync::Mutex::new(standing),
            advertisement_status: RwLock::new((
                if ports.advertise_reachability {
                    "pending"
                } else {
                    "disabled"
                },
                None,
            )),
            reservation_statuses: RwLock::new(vec![]),
        })
    }

    pub(crate) async fn configure_product(&self, enabled: bool, advertise: bool) {
        self.product_enabled.store(enabled, Ordering::Release);
        // Drain any publication already in progress before acknowledging opt-out.
        let _standing = self.standing.lock().await;
        self.product_advertise.store(advertise, Ordering::Release);
        if !enabled {
            let _ = self.manager.advance_network_epoch();
            self.manager.reservations.close_all().await;
        }
    }
    pub(crate) async fn admit_product_source(
        &self,
        input: DiscoveryInput,
        current: &(dyn Fn() -> bool + Send + Sync),
    ) -> Result<DiscoverySourceStatus, &'static str> {
        let mut discovery = self.discovery.lock().await;
        if !current() {
            return Err("grant_revoked");
        }
        discovery.add_product_source(input).await
    }
    pub(crate) async fn toggle_product_source(
        &self,
        id: [u8; 32],
        enabled: bool,
    ) -> Result<DiscoverySourceStatus, &'static str> {
        self.toggle_product_source_guarded(id, enabled, &|| true)
            .await
    }
    pub(crate) async fn toggle_product_source_guarded(
        &self,
        id: [u8; 32],
        enabled: bool,
        current: &(dyn Fn() -> bool + Send + Sync),
    ) -> Result<DiscoverySourceStatus, &'static str> {
        let mut discovery = self.discovery.lock().await;
        if !current() {
            return Err("grant_revoked");
        }
        let status = discovery.toggle_product_source(id, enabled).await?;
        if !enabled {
            self.manager
                .advance_network_epoch()
                .map_err(|_| "network_epoch")?;
            self.manager.reservations.close_all().await;
            self.manager
                .discovery
                .write()
                .await
                .retain_product_relays(&discovery.enabled_relay_ids());
        }
        Ok(status)
    }
    pub(crate) async fn refresh_product(
        &self,
        current: &(dyn Fn() -> bool + Send + Sync),
    ) -> Result<(), &'static str> {
        let mut discovery = self.discovery.lock().await;
        discovery.request_refresh();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "clock")?
            .as_secs();
        tokio::time::timeout(
            Duration::from_secs(20),
            discovery.refresh_until(
                &self.manager.discovery,
                now,
                &|| current() && self.granted(),
                std::time::Instant::now() + Duration::from_secs(20),
            ),
        )
        .await
        .map_err(|_| "deadline")?
        .map_err(|_| "refresh")
    }

    pub(crate) fn granted(&self) -> bool {
        self.product_enabled.load(Ordering::Acquire)
            && self.grant.has_changed().is_ok()
            && *self.grant.borrow()
    }

    pub(crate) fn limitation(&self) -> &'static str {
        self.limitation
            .lock()
            .map(|value| *value)
            .unwrap_or("state_unavailable")
    }

    fn record(&self, value: &'static str) {
        if let Ok(mut current) = self.limitation.lock() {
            *current = value;
        }
    }

    pub(crate) async fn run(&self, cancel: watch::Receiver<bool>, rollout: VNextRuntimeRollout) {
        tokio::join!(
            self.run_maintenance(cancel.clone(), rollout.clone()),
            self.run_inbound(cancel, rollout)
        );
    }

    async fn run_maintenance(
        &self,
        mut cancel: watch::Receiver<bool>,
        rollout: VNextRuntimeRollout,
    ) {
        let mut grant = self.grant.clone();
        let mut tick = tokio::time::interval(Duration::from_secs(2));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut next_attempt = tokio::time::Instant::now();
        let mut failure_delay = 2_u64;
        loop {
            if *cancel.borrow() {
                break;
            }
            tokio::select! {
                biased;
                _ = cancel.changed() => break,
                _ = tick.tick() => {
                    if tokio::time::Instant::now() < next_attempt { continue; }
                    if !self.granted() { self.manager.reservations.close_all().await; self.record("execution_grant_unavailable"); continue; }
                    let Ok(generation) = rollout.acquire(VNextRuntimeLane::Network) else { self.manager.reservations.close_all().await; continue; };
                    let result = tokio::select! {
                        biased;
                        _ = cancel.changed() => break,
                        _ = grant.changed() => { self.manager.reservations.close_all().await; self.record("execution_grant_changed"); continue; },
                        result = tokio::time::timeout(Duration::from_secs(20), async {
                            self.manager.maintenance_once().await?;
                            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|_| crate::vnext_reachability_manager::ReachabilityError::CorruptState)?.as_secs();
                            let current = || generation.is_current() && self.granted();
                            self.discovery.lock().await.refresh_until(&self.manager.discovery, now, &current, std::time::Instant::now() + Duration::from_secs(8)).await?;
                            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|_| crate::vnext_reachability_manager::ReachabilityError::CorruptState)?.as_secs();
                            let mut standing = self.standing.lock().await;
                            standing.maintain(&self.manager, self.product_advertise.load(Ordering::Acquire), now, &current).await?;
                            *self.advertisement_status.write().await = (standing.advertisement_state, standing.published_expiry());
                            *self.reservation_statuses.write().await = standing.reservation_statuses.values().cloned().collect();
                            self.record(standing.limitation.unwrap_or("route_requires_fresh_peer_advertisement"));
                            Ok::<(), crate::vnext_reachability_manager::ReachabilityError>(())
                        }) => result,
                    };
                    if !generation.is_current() { self.manager.reservations.close_all().await; self.record("network_generation_changed"); continue; }
                    if matches!(result, Ok(Ok(()))) {
                        failure_delay = 2;
                    } else {
                        next_attempt = tokio::time::Instant::now() + Duration::from_secs(failure_delay);
                        failure_delay = (failure_delay * 2).min(20);
                    }
                    self.record(match result {
                        Ok(Ok(())) => self.limitation(),
                        Ok(Err(_)) => "candidate_collection_unavailable",
                        Err(_) => "candidate_collection_deadline",
                    });
                }
            }
        }
    }

    pub(crate) async fn close(&self) {
        self.manager.reservations.close_all().await;
    }
}

// Forward the already proof-checked node identity. Domain bytes are the existing
// reachability domain; no new key or signing domain is introduced.
struct SessionSignerBridge {
    signer: Arc<dyn SessionIdentitySigner>,
    public_key: [u8; 32],
}

impl ReachabilityIdentitySigner for SessionSignerBridge {
    fn public_key(&self) -> [u8; 32] {
        self.public_key
    }

    fn sign_reachability_message(
        &self,
        domain: &'static [u8],
        message: &[u8],
    ) -> Result<[u8; 64], ReachabilityCryptoError> {
        if self.signer.public_key() != self.public_key {
            return Err(ReachabilityCryptoError::SignerUnavailable);
        }
        let mut preimage = Vec::with_capacity(domain.len() + message.len());
        preimage.extend_from_slice(domain);
        preimage.extend_from_slice(message);
        self.signer
            .sign_session_message(&preimage)
            .map_err(|_| ReachabilityCryptoError::SignerUnavailable)
    }
}
