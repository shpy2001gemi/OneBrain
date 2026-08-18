//! Outbound-first reachability policy, platform ports and bounded manager state.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ku_core::foundation::NodeId;
use ku_net::vnext_reachability_crypto::{
    ReachabilityIdentitySigner, RelayAdmissionError, ValidatedRelayDescriptor,
    ValidatedRelayReservation,
};
use ku_net::vnext_reachability_resolver::ReachabilityAdvertisementResolver;
use ku_net::vnext_relay_discovery::{ReachabilityFuture, RelayDiscovery, RelayDiscoveryLimitation};
use ku_net::vnext_relay_tunnel::{AuthenticatedOuterRelayConnection, ValidatedRelayDialSet};
use onebrain_protocol::{
    DirectCandidateV1, PrivateCandidateV1, PublicCandidateV1, ReachabilityAdvertisementV1,
    RelayCandidateV1, RelayDenialCodeV1, RelayReserveRequestV1,
};
use tokio::sync::{watch, RwLock};

pub const MAX_DIRECT_CANDIDATES: usize = 8;
pub const MAX_RELAY_CANDIDATES: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NetworkEpoch(u64);

impl NetworkEpoch {
    pub const fn initial() -> Self {
        Self(1)
    }

    pub const fn from_u64(value: u64) -> Option<Self> {
        if value == 0 {
            None
        } else {
            Some(Self(value))
        }
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub fn next(self) -> Result<Self, ReachabilityError> {
        self.0
            .checked_add(1)
            .and_then(Self::from_u64)
            .ok_or(ReachabilityError::CorruptState)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VNextReachabilityPolicy {
    pub route_deadline: Duration,
    pub direct_timeout: Duration,
    pub hole_punch_timeout: Duration,
    pub relay_connect_timeout: Duration,
    pub max_concurrent_checks: usize,
    pub max_probe_bytes: u64,
    pub min_relay_reservations: usize,
    pub target_relay_reservations: usize,
    pub max_relay_reservations: usize,
    pub reservation_refresh_margin: Duration,
    pub keepalive_interval: Duration,
    pub max_route_receipts: usize,
    pub max_route_journal_bytes: u64,
}

impl Default for VNextReachabilityPolicy {
    fn default() -> Self {
        Self {
            route_deadline: Duration::from_secs(20),
            direct_timeout: Duration::from_millis(2_500),
            hole_punch_timeout: Duration::from_secs(5),
            relay_connect_timeout: Duration::from_secs(5),
            max_concurrent_checks: 4,
            max_probe_bytes: 1_048_576,
            min_relay_reservations: 2,
            target_relay_reservations: 3,
            max_relay_reservations: 3,
            reservation_refresh_margin: Duration::from_secs(180),
            keepalive_interval: Duration::from_secs(20),
            max_route_receipts: 4_096,
            max_route_journal_bytes: 16_777_216,
        }
    }
}

impl VNextReachabilityPolicy {
    pub fn validate(self) -> Result<(), ReachabilityError> {
        let frozen = Self::default();
        if self.route_deadline != frozen.route_deadline
            || self.direct_timeout != frozen.direct_timeout
            || self.hole_punch_timeout != frozen.hole_punch_timeout
            || self.relay_connect_timeout != frozen.relay_connect_timeout
            || self.max_concurrent_checks == 0
            || self.max_concurrent_checks > frozen.max_concurrent_checks
            || self.max_probe_bytes == 0
            || self.max_probe_bytes > frozen.max_probe_bytes
            || self.min_relay_reservations != 2
            || self.target_relay_reservations != 3
            || self.max_relay_reservations != 3
            || self.reservation_refresh_margin != frozen.reservation_refresh_margin
            || self.keepalive_interval != frozen.keepalive_interval
            || self.max_route_receipts == 0
            || self.max_route_receipts > frozen.max_route_receipts
            || self.max_route_journal_bytes == 0
            || self.max_route_journal_bytes > frozen.max_route_journal_bytes
        {
            Err(ReachabilityError::InvalidPolicy)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivateCandidateSet {
    candidates: Vec<PrivateCandidateV1>,
    expected_peer: Option<NodeId>,
    authenticated_session: Option<[u8; 32]>,
    network_epoch: NetworkEpoch,
}

impl PrivateCandidateSet {
    pub fn local(
        candidates: Vec<PrivateCandidateV1>,
        epoch: NetworkEpoch,
    ) -> Result<Self, ReachabilityError> {
        if candidates.len() > MAX_DIRECT_CANDIDATES {
            return Err(ReachabilityError::InvalidCandidates);
        }
        Ok(Self {
            candidates,
            expected_peer: None,
            authenticated_session: None,
            network_epoch: epoch,
        })
    }

    pub fn authenticated_for_peer(
        candidates: Vec<PrivateCandidateV1>,
        expected_peer: NodeId,
        session: [u8; 32],
        epoch: NetworkEpoch,
    ) -> Result<Self, ReachabilityError> {
        if session == [0; 32] || candidates.len() > MAX_DIRECT_CANDIDATES {
            return Err(ReachabilityError::InvalidCandidates);
        }
        Ok(Self {
            candidates,
            expected_peer: Some(expected_peer),
            authenticated_session: Some(session),
            network_epoch: epoch,
        })
    }

    pub fn candidates(&self) -> &[PrivateCandidateV1] {
        &self.candidates
    }

    pub fn network_epoch(&self) -> NetworkEpoch {
        self.network_epoch
    }

    pub fn is_publishable(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatheredCandidates {
    pub private: PrivateCandidateSet,
    pub public: Vec<PublicCandidateV1>,
    pub direct: Vec<DirectCandidateV1>,
    pub relay: Vec<RelayCandidateV1>,
    pub epoch: NetworkEpoch,
    pub observed_at: u64,
}

impl GatheredCandidates {
    pub fn validate(&self) -> Result<(), ReachabilityError> {
        if self.private.network_epoch != self.epoch
            || self.public.len() > MAX_DIRECT_CANDIDATES
            || self.direct.len() > MAX_DIRECT_CANDIDATES
            || self.relay.len() > MAX_RELAY_CANDIDATES
        {
            Err(ReachabilityError::InvalidCandidates)
        } else {
            Ok(())
        }
    }
}

pub trait CandidateGatherer: Send + Sync {
    fn gather(
        &self,
        epoch: NetworkEpoch,
    ) -> ReachabilityFuture<'_, Result<GatheredCandidates, ReachabilityError>>;
}

pub trait AdvertisementPublisher: Send + Sync {
    fn publish<'a>(
        &'a self,
        advertisement: &'a ReachabilityAdvertisementV1,
    ) -> ReachabilityFuture<'a, Result<(), ReachabilityError>>;
}

pub trait RelayDialRouteProvider: Send + Sync {
    fn route_set_for<'a>(
        &'a self,
        relay: &'a ValidatedRelayDescriptor,
        deadline: Instant,
    ) -> ReachabilityFuture<'a, Result<ValidatedRelayDialSet, ReachabilityError>>;
}

pub trait RelayReservationClient: Send + Sync {
    fn authenticate<'a>(
        &'a self,
        relay: &'a ValidatedRelayDescriptor,
        routes: &'a ValidatedRelayDialSet,
        deadline: Instant,
    ) -> ReachabilityFuture<'a, Result<Arc<AuthenticatedOuterRelayConnection>, ReachabilityError>>;
    fn reserve<'a>(
        &'a self,
        relay: &'a ValidatedRelayDescriptor,
        outer: &'a AuthenticatedOuterRelayConnection,
        request: RelayReserveRequestV1,
    ) -> ReachabilityFuture<'a, Result<ValidatedRelayReservation, ReachabilityError>>;
    fn keepalive<'a>(
        &'a self,
        reservation: &'a ValidatedRelayReservation,
        outer: &'a AuthenticatedOuterRelayConnection,
        sequence: u64,
    ) -> ReachabilityFuture<'a, Result<(), ReachabilityError>>;
    fn revoke<'a>(
        &'a self,
        reservation: &'a ValidatedRelayReservation,
        outer: &'a AuthenticatedOuterRelayConnection,
        sequence: u64,
    ) -> ReachabilityFuture<'a, Result<(), ReachabilityError>>;
}

pub struct ActiveRelayReservation {
    reservation: ValidatedRelayReservation,
    outer: Arc<AuthenticatedOuterRelayConnection>,
}

impl ActiveRelayReservation {
    pub fn reservation(&self) -> &ValidatedRelayReservation {
        &self.reservation
    }

    pub fn outer(&self) -> &Arc<AuthenticatedOuterRelayConnection> {
        &self.outer
    }
}

pub struct RelayReservationManager {
    client: Arc<dyn RelayReservationClient>,
    routes: Arc<dyn RelayDialRouteProvider>,
    active: RwLock<BTreeMap<NodeId, ActiveRelayReservation>>,
    policy: VNextReachabilityPolicy,
}

impl RelayReservationManager {
    pub fn new(
        client: Arc<dyn RelayReservationClient>,
        routes: Arc<dyn RelayDialRouteProvider>,
        policy: VNextReachabilityPolicy,
    ) -> Result<Self, ReachabilityError> {
        policy.validate()?;
        Ok(Self {
            client,
            routes,
            active: RwLock::new(BTreeMap::new()),
            policy,
        })
    }

    pub async fn ensure_route_reservation(
        &self,
        relay: &ValidatedRelayDescriptor,
        request: RelayReserveRequestV1,
        deadline: Instant,
    ) -> Result<ValidatedRelayReservation, ReachabilityError> {
        if Instant::now() >= deadline || request.relay_node_id != relay.canonical().relay_node_id {
            return Err(ReachabilityError::Deadline);
        }
        let refresh_before = request
            .issued_at
            .checked_add(self.policy.reservation_refresh_margin.as_secs())
            .ok_or(ReachabilityError::CorruptState)?;
        if let Some(active) = self.active.read().await.get(&request.relay_node_id) {
            if active.outer.is_open() && active.reservation.canonical().expires_at > refresh_before
            {
                return Ok(active.reservation.clone());
            }
        }
        let routes = self.routes.route_set_for(relay, deadline).await?;
        let outer = self.client.authenticate(relay, &routes, deadline).await?;
        if !outer.is_open() || outer.relay_node_id() != relay.canonical().relay_node_id {
            return Err(ReachabilityError::Io);
        }
        let reservation = self.client.reserve(relay, &outer, request).await?;
        if reservation.canonical().relay_node_id != relay.canonical().relay_node_id {
            return Err(ReachabilityError::CorruptState);
        }
        let mut active = self.active.write().await;
        if !active.contains_key(&reservation.canonical().relay_node_id)
            && active.len() >= self.policy.max_relay_reservations
        {
            return Err(ReachabilityError::ReservationCapacity);
        }
        active.insert(
            reservation.canonical().relay_node_id,
            ActiveRelayReservation {
                reservation: reservation.clone(),
                outer,
            },
        );
        Ok(reservation)
    }

    pub async fn active_count(&self) -> usize {
        self.active.read().await.len()
    }

    pub async fn keepalive_all(&self, sequence: u64) -> Result<usize, ReachabilityError> {
        let snapshot: Vec<_> = self
            .active
            .read()
            .await
            .iter()
            .map(|(relay, active)| {
                (
                    *relay,
                    active.reservation.clone(),
                    Arc::clone(&active.outer),
                )
            })
            .collect();
        let mut failed = Vec::new();
        let mut first_error = None;
        let mut refreshed = 0;
        for (relay, reservation, outer) in snapshot {
            if !outer.is_open() {
                failed.push(relay);
                continue;
            }
            match self.client.keepalive(&reservation, &outer, sequence).await {
                Ok(()) => refreshed += 1,
                Err(error) => {
                    failed.push(relay);
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        if !failed.is_empty() {
            self.active
                .write()
                .await
                .retain(|relay, _| !failed.contains(relay));
        }
        if let Some(error) = first_error {
            Err(error)
        } else {
            Ok(refreshed)
        }
    }

    pub async fn revoke(&self, relay: NodeId, sequence: u64) -> Result<(), ReachabilityError> {
        let (reservation, outer) = self
            .active
            .read()
            .await
            .get(&relay)
            .map(|active| (active.reservation.clone(), Arc::clone(&active.outer)))
            .ok_or(ReachabilityError::ReservationDenied(
                RelayDenialCodeV1::Policy,
            ))?;
        if !outer.is_open() {
            self.active.write().await.remove(&relay);
            return Err(ReachabilityError::Io);
        }
        self.client.revoke(&reservation, &outer, sequence).await?;
        let digest = *reservation.digest();
        self.active.write().await.retain(|candidate, active| {
            *candidate != relay || *active.reservation.digest() != digest
        });
        Ok(())
    }

    pub async fn invalidate_closed(&self) {
        self.active
            .write()
            .await
            .retain(|_, value| value.outer.is_open());
    }
}

pub struct ReachabilityManager {
    pub discovery: Arc<RwLock<RelayDiscovery>>,
    pub resolver: Arc<ReachabilityAdvertisementResolver>,
    pub reservations: Arc<RelayReservationManager>,
    gatherer: Arc<dyn CandidateGatherer>,
    publisher: Arc<dyn AdvertisementPublisher>,
    signer: Arc<dyn ReachabilityIdentitySigner>,
    policy: VNextReachabilityPolicy,
    network_epoch: AtomicU64,
}

impl ReachabilityManager {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        discovery: Arc<RwLock<RelayDiscovery>>,
        resolver: Arc<ReachabilityAdvertisementResolver>,
        reservations: Arc<RelayReservationManager>,
        gatherer: Arc<dyn CandidateGatherer>,
        publisher: Arc<dyn AdvertisementPublisher>,
        signer: Arc<dyn ReachabilityIdentitySigner>,
        policy: VNextReachabilityPolicy,
    ) -> Result<Self, ReachabilityError> {
        policy.validate()?;
        Ok(Self {
            discovery,
            resolver,
            reservations,
            gatherer,
            publisher,
            signer,
            policy,
            network_epoch: AtomicU64::new(1),
        })
    }

    pub fn current_epoch(&self) -> NetworkEpoch {
        NetworkEpoch(self.network_epoch.load(Ordering::Acquire))
    }

    pub fn advance_network_epoch(&self) -> Result<NetworkEpoch, ReachabilityError> {
        let previous = self
            .network_epoch
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                value.checked_add(1)
            })
            .map_err(|_| ReachabilityError::CorruptState)?;
        Ok(NetworkEpoch(previous + 1))
    }

    pub async fn publish(
        &self,
        advertisement: &ReachabilityAdvertisementV1,
    ) -> Result<(), ReachabilityError> {
        self.publisher.publish(advertisement).await
    }

    pub async fn run(&self, mut cancel: watch::Receiver<bool>) -> Result<(), ReachabilityError> {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        loop {
            tokio::select! {
                changed = cancel.changed() => {
                    if changed.is_err() || *cancel.borrow() {
                        return Ok(());
                    }
                }
                _ = interval.tick() => {
                    let gathered = self.gatherer.gather(self.current_epoch()).await?;
                    gathered.validate()?;
                    self.reservations.invalidate_closed().await;
                    let _ = (&self.signer, &self.resolver, &self.discovery, self.policy);
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum ReachabilityError {
    UnsupportedPlatform,
    Discovery(RelayDiscoveryLimitation),
    Admission(RelayAdmissionError),
    ReservationDenied(RelayDenialCodeV1),
    Deadline,
    NetworkChanged,
    Io,
    CorruptState,
    InvalidPolicy,
    InvalidCandidates,
    ReservationCapacity,
}
