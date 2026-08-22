//! Outbound-first reachability policy, platform ports and bounded manager state.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::RwLock as StdRwLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ku_core::foundation::NodeId;
use ku_net::transport::QuicTransport;
use ku_net::vnext_reachability_crypto::{
    InMemoryReachabilityReplayStore, KnownPeerIdentity, ReachabilityAdmission,
    ReachabilityIdentitySigner, ReachabilityRecordAdmission, RelayAdmissionError,
    ValidatedRelayDescriptor, ValidatedRelayReservation,
};
use ku_net::vnext_reachability_resolver::ReachabilityAdvertisementResolver;
use ku_net::vnext_relay_discovery::{
    ReachabilityFuture, RelayDiscovery, RelayDiscoveryDelta, RelayDiscoveryLimitation,
    RelayDiscoveryPreparer, RelayDiscoverySource, RelayPossessionClient, StagedRelayAdmission,
    VerifiedRelayDiscovery,
};
use ku_net::vnext_relay_tunnel::{
    connect_authenticated_outer, connect_authenticated_outer_on_transport, prove_relay_possession,
    AuthenticatedOuterRelayConnection, ValidatedRelayDialRoute, ValidatedRelayDialSet,
};
use onebrain_protocol::{
    decode_relay_control, encode_reachability_object, encode_relay_control,
    relay_control_signing_parts, DirectCandidateV1, PrivateCandidateV1, PublicCandidateV1,
    ReachabilityAdvertisementV1, ReachabilityObjectV1, RelayCandidateV1,
    RelayControlSignatureRoleV1, RelayControlV1, RelayDenialCodeV1, RelayKeepaliveV1,
    RelayPossessionProofV1, RelayReserveRequestV1, RelayRevocationActorV1, RelayRevocationReasonV1,
    RelayRevokeV1, RelayTransportV1, RelayWireFrameV1, RelayWireKindV1,
};
use rand::rngs::OsRng;
use rand::RngCore;
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

/// Production route provider: every descriptor endpoint is re-resolved and
/// converted to a sealed dial route immediately before use. UDP is preferred;
/// a descriptor may also be TCP-443-only.
pub struct ProductionRelayDialRouteProvider {
    validator: Arc<ku_net::vnext_reachability_crypto::ReachabilityDialValidator>,
}

impl ProductionRelayDialRouteProvider {
    pub fn new(
        validator: Arc<ku_net::vnext_reachability_crypto::ReachabilityDialValidator>,
    ) -> Self {
        Self { validator }
    }
}

impl RelayDialRouteProvider for ProductionRelayDialRouteProvider {
    fn route_set_for<'a>(
        &'a self,
        relay: &'a ValidatedRelayDescriptor,
        deadline: Instant,
    ) -> ReachabilityFuture<'a, Result<ValidatedRelayDialSet, ReachabilityError>> {
        use ku_net::vnext_reachability_crypto::ReachabilityLockFreeDialValidation;
        Box::pin(async move {
            let mut udp = None;
            let mut tcp = None;
            for (index, endpoint) in relay.canonical().endpoints.iter().enumerate() {
                if Instant::now() >= deadline {
                    return Err(ReachabilityError::Deadline);
                }
                let token = self
                    .validator
                    .validate_relay_dial(relay, index, deadline)
                    .await
                    .map_err(ReachabilityError::Admission)?;
                let route = ValidatedRelayDialRoute::public(relay, token)
                    .map_err(|_| ReachabilityError::CorruptState)?;
                match endpoint.transport {
                    RelayTransportV1::QuicUdp if udp.is_none() => udp = Some(route),
                    RelayTransportV1::TlsTcp443 if tcp.is_none() => tcp = Some(route),
                    _ => {}
                }
            }
            let (primary, fallback) = match (udp, tcp) {
                (Some(primary), fallback) => (primary, fallback),
                (None, Some(primary)) => (primary, None),
                (None, None) => return Err(ReachabilityError::Io),
            };
            ValidatedRelayDialSet::from_admitted_descriptor(primary, fallback)
                .map_err(|_| ReachabilityError::CorruptState)
        })
    }
}

pub struct ProductionRelayPossessionClient {
    signer: Arc<dyn ReachabilityIdentitySigner>,
}

impl ProductionRelayPossessionClient {
    pub fn new(signer: Arc<dyn ReachabilityIdentitySigner>) -> Self {
        Self { signer }
    }
}

impl RelayPossessionClient for ProductionRelayPossessionClient {
    fn prove<'a>(
        &'a self,
        staged: &'a StagedRelayAdmission,
        deadline: Instant,
    ) -> ReachabilityFuture<'a, Result<Vec<RelayPossessionProofV1>, RelayDiscoveryLimitation>> {
        Box::pin(async move {
            let now = unix_now().map_err(|_| RelayDiscoveryLimitation::PoisonedSource)?;
            let mut proofs = Vec::with_capacity(staged.possession_dials().len());
            for dial in staged.possession_dials() {
                proofs.push(
                    prove_relay_possession(dial, self.signer.as_ref(), now, deadline)
                        .await
                        .map_err(|_| RelayDiscoveryLimitation::PoisonedSource)?,
                );
            }
            Ok(proofs)
        })
    }
}

/// Admit signed relay descriptors without retaining a discovery lock across
/// DNS resolution or live proof-of-possession network I/O. Every uncommitted
/// permit/staged descriptor is explicitly aborted on failure.
pub async fn admit_relay_records(
    discovery: &Arc<RwLock<RelayDiscovery>>,
    preparer: &RelayDiscoveryPreparer,
    possession: &dyn RelayPossessionClient,
    source: RelayDiscoverySource,
    records: &[Vec<u8>],
    now: u64,
    deadline: Instant,
) -> Result<RelayDiscoveryDelta, ReachabilityError> {
    if records.is_empty() || Instant::now() >= deadline {
        return Err(ReachabilityError::Deadline);
    }
    let lengths: Vec<_> = records.iter().map(Vec::len).collect();
    let permit = discovery
        .write()
        .await
        .reserve_preparation(source, &lengths, now)
        .map_err(ReachabilityError::Discovery)?;
    let prepared = match preparer
        .prepare_records(&permit, records, now, deadline)
        .await
    {
        Ok(value) => value,
        Err(error) => {
            discovery
                .write()
                .await
                .abort_preparation(permit, now)
                .map_err(ReachabilityError::Discovery)?;
            return Err(ReachabilityError::Discovery(error));
        }
    };
    let staged = discovery
        .write()
        .await
        .stage_prepared(permit, prepared, now)
        .map_err(ReachabilityError::Discovery)?;
    let mut aggregate = RelayDiscoveryDelta::default();
    for mut descriptor in staged {
        if let Err(error) = preparer.prepare_possession(&mut descriptor, deadline).await {
            discovery
                .write()
                .await
                .abort_descriptor(descriptor, now)
                .map_err(ReachabilityError::Discovery)?;
            return Err(ReachabilityError::Discovery(error));
        }
        let proofs = match possession.prove(&descriptor, deadline).await {
            Ok(value) => value,
            Err(error) => {
                discovery
                    .write()
                    .await
                    .abort_descriptor(descriptor, now)
                    .map_err(ReachabilityError::Discovery)?;
                return Err(ReachabilityError::Discovery(error));
            }
        };
        let delta = discovery
            .write()
            .await
            .commit_descriptor(descriptor, &proofs, now)
            .map_err(ReachabilityError::Discovery)?;
        aggregate.admitted.extend(delta.admitted);
        aggregate.refreshed.extend(delta.refreshed);
        aggregate.rejected = aggregate.rejected.saturating_add(delta.rejected);
        aggregate.limitations.extend(delta.limitations);
    }
    Ok(aggregate)
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
    fn observe_reflexive<'a>(
        &'a self,
        reservation: &'a ValidatedRelayReservation,
        outer: &'a AuthenticatedOuterRelayConnection,
        network_epoch: u64,
    ) -> ReachabilityFuture<'a, Result<Vec<u8>, ReachabilityError>>;
}

/// Production relay client. It can be shared by every platform adapter; only
/// the injected identity signer and admitted dial routes are platform-owned.
pub struct ProductionRelayReservationClient {
    signer: Arc<dyn ReachabilityIdentitySigner>,
    admission: Mutex<ReachabilityAdmission>,
    shared_quic_transport: StdRwLock<Option<Arc<QuicTransport>>>,
}

impl ProductionRelayReservationClient {
    pub fn new(signer: Arc<dyn ReachabilityIdentitySigner>) -> Self {
        Self {
            signer,
            admission: Mutex::new(ReachabilityAdmission::new(Arc::new(
                InMemoryReachabilityReplayStore::default(),
            ))),
            shared_quic_transport: StdRwLock::new(None),
        }
    }

    pub fn with_admission(
        signer: Arc<dyn ReachabilityIdentitySigner>,
        admission: ReachabilityAdmission,
    ) -> Self {
        Self {
            signer,
            admission: Mutex::new(admission),
            shared_quic_transport: StdRwLock::new(None),
        }
    }

    /// Attach the node's already-bound direct QUIC endpoint before the first
    /// reservation. Replacement after attachment is rejected so a live
    /// reservation can never migrate to a different outer socket.
    pub fn attach_shared_quic_transport(
        &self,
        transport: Arc<QuicTransport>,
    ) -> Result<(), ReachabilityError> {
        let mut current = self
            .shared_quic_transport
            .write()
            .map_err(|_| ReachabilityError::CorruptState)?;
        if current.is_some() {
            return Err(ReachabilityError::CorruptState);
        }
        *current = Some(transport);
        Ok(())
    }

    async fn exchange(
        outer: &AuthenticatedOuterRelayConnection,
        control: RelayControlV1,
    ) -> Result<RelayControlV1, ReachabilityError> {
        let mut request_id = [0u8; 16];
        OsRng.fill_bytes(&mut request_id);
        let frame = RelayWireFrameV1::new(
            RelayWireKindV1::Control,
            request_id,
            encode_relay_control(&control).map_err(|_| ReachabilityError::CorruptState)?,
        )
        .map_err(|_| ReachabilityError::CorruptState)?;
        let response = outer
            .request_control_frame(&frame)
            .await
            .map_err(|_| ReachabilityError::Io)?;
        if response.kind() != RelayWireKindV1::Control {
            return Err(ReachabilityError::CorruptState);
        }
        decode_relay_control(response.payload()).map_err(|_| ReachabilityError::CorruptState)
    }

    fn sign_control(
        &self,
        control: &RelayControlV1,
        role: RelayControlSignatureRoleV1,
    ) -> Result<[u8; 64], ReachabilityError> {
        let (domain, message) = relay_control_signing_parts(control, role)
            .map_err(|_| ReachabilityError::CorruptState)?;
        self.signer
            .sign_reachability_message(domain, &message)
            .map_err(|_| ReachabilityError::Io)
    }
}

impl RelayReservationClient for ProductionRelayReservationClient {
    fn authenticate<'a>(
        &'a self,
        relay: &'a ValidatedRelayDescriptor,
        routes: &'a ValidatedRelayDialSet,
        deadline: Instant,
    ) -> ReachabilityFuture<'a, Result<Arc<AuthenticatedOuterRelayConnection>, ReachabilityError>>
    {
        Box::pin(async move {
            let shared = self
                .shared_quic_transport
                .read()
                .map_err(|_| ReachabilityError::CorruptState)?
                .clone();
            let connection = match shared {
                Some(transport) => {
                    connect_authenticated_outer_on_transport(
                        routes,
                        self.signer.as_ref(),
                        unix_now()?,
                        deadline,
                        transport.as_ref(),
                    )
                    .await
                }
                None => {
                    connect_authenticated_outer(routes, self.signer.as_ref(), unix_now()?, deadline)
                        .await
                }
            }
            .map_err(|_| ReachabilityError::Io)?;
            if connection.relay_node_id() != relay.canonical().relay_node_id
                || connection.client_node_id()
                    != ku_net::vnext_session::principal_node_id(&self.signer.public_key())
            {
                return Err(ReachabilityError::CorruptState);
            }
            Ok(Arc::new(connection))
        })
    }

    fn reserve<'a>(
        &'a self,
        relay: &'a ValidatedRelayDescriptor,
        outer: &'a AuthenticatedOuterRelayConnection,
        request: RelayReserveRequestV1,
    ) -> ReachabilityFuture<'a, Result<ValidatedRelayReservation, ReachabilityError>> {
        Box::pin(async move {
            match Self::exchange(outer, RelayControlV1::Reserve(request)).await? {
                RelayControlV1::Granted(grant) => {
                    let bytes =
                        encode_reachability_object(&ReachabilityObjectV1::RelayReservation(grant))
                            .map_err(|_| ReachabilityError::CorruptState)?;
                    let target = KnownPeerIdentity::from_public_key(self.signer.public_key());
                    let relay_identity = KnownPeerIdentity {
                        node_id: relay.canonical().relay_node_id,
                        public_key: relay.canonical().relay_public_key,
                    };
                    self.admission
                        .lock()
                        .map_err(|_| ReachabilityError::CorruptState)?
                        .admit_reservation(&bytes, &target, &relay_identity, unix_now()?)
                        .map_err(ReachabilityError::Admission)
                }
                RelayControlV1::Denied(value) => {
                    Err(ReachabilityError::ReservationDenied(value.code))
                }
                _ => Err(ReachabilityError::CorruptState),
            }
        })
    }

    fn keepalive<'a>(
        &'a self,
        reservation: &'a ValidatedRelayReservation,
        outer: &'a AuthenticatedOuterRelayConnection,
        sequence: u64,
    ) -> ReachabilityFuture<'a, Result<(), ReachabilityError>> {
        Box::pin(async move {
            let now = unix_now()?;
            let mut value = RelayKeepaliveV1 {
                format: 1,
                relay_node_id: reservation.canonical().relay_node_id,
                target_node_id: reservation.canonical().target_node_id,
                reservation_id: reservation.canonical().reservation_id,
                sequence,
                issued_at: now,
                expires_at: reservation
                    .canonical()
                    .expires_at
                    .min(now.saturating_add(30)),
                target_signature: [0; 64],
            };
            value.target_signature = self.sign_control(
                &RelayControlV1::Keepalive(value.clone()),
                RelayControlSignatureRoleV1::KeepaliveTarget,
            )?;
            match Self::exchange(outer, RelayControlV1::Keepalive(value.clone())).await? {
                RelayControlV1::Keepalive(ack) if ack == value => Ok(()),
                _ => Err(ReachabilityError::CorruptState),
            }
        })
    }

    fn revoke<'a>(
        &'a self,
        reservation: &'a ValidatedRelayReservation,
        outer: &'a AuthenticatedOuterRelayConnection,
        sequence: u64,
    ) -> ReachabilityFuture<'a, Result<(), ReachabilityError>> {
        Box::pin(async move {
            let now = unix_now()?;
            let mut value = RelayRevokeV1 {
                format: 1,
                relay_node_id: reservation.canonical().relay_node_id,
                target_node_id: reservation.canonical().target_node_id,
                reservation_id: reservation.canonical().reservation_id,
                actor: RelayRevocationActorV1::Target,
                reason: RelayRevocationReasonV1::TargetClosed,
                sequence,
                issued_at: now,
                expires_at: reservation
                    .canonical()
                    .expires_at
                    .min(now.saturating_add(30)),
                actor_signature: [0; 64],
            };
            value.actor_signature = self.sign_control(
                &RelayControlV1::Revoke(value.clone()),
                RelayControlSignatureRoleV1::RevokeActor,
            )?;
            match Self::exchange(outer, RelayControlV1::Revoke(value.clone())).await? {
                RelayControlV1::Revoke(ack) if ack == value => Ok(()),
                _ => Err(ReachabilityError::CorruptState),
            }
        })
    }

    fn observe_reflexive<'a>(
        &'a self,
        reservation: &'a ValidatedRelayReservation,
        outer: &'a AuthenticatedOuterRelayConnection,
        network_epoch: u64,
    ) -> ReachabilityFuture<'a, Result<Vec<u8>, ReachabilityError>> {
        Box::pin(async move {
            if network_epoch == 0
                || reservation.canonical().relay_node_id != outer.relay_node_id()
                || reservation.canonical().target_node_id != outer.client_node_id()
            {
                return Err(ReachabilityError::CorruptState);
            }
            let mut request_id = [0u8; 16];
            OsRng.fill_bytes(&mut request_id);
            let mut payload = Vec::with_capacity(40);
            payload.extend_from_slice(&reservation.canonical().reservation_id);
            payload.extend_from_slice(&network_epoch.to_be_bytes());
            let request =
                RelayWireFrameV1::new(RelayWireKindV1::ReflexiveObservation, request_id, payload)
                    .map_err(|_| ReachabilityError::CorruptState)?;
            let response = outer
                .request_control_frame(&request)
                .await
                .map_err(|_| ReachabilityError::Io)?;
            if response.kind() != RelayWireKindV1::ReflexiveObservation
                || response.request_id() != request_id
            {
                return Err(ReachabilityError::CorruptState);
            }
            Ok(response.payload().to_vec())
        })
    }
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

    pub async fn active_reservations(&self) -> Vec<ValidatedRelayReservation> {
        self.active
            .read()
            .await
            .values()
            .filter(|active| active.outer.is_open())
            .map(|active| active.reservation.clone())
            .collect()
    }

    pub async fn active_for(
        &self,
        relay: NodeId,
    ) -> Option<(
        ValidatedRelayReservation,
        Arc<AuthenticatedOuterRelayConnection>,
    )> {
        self.active.read().await.get(&relay).and_then(|active| {
            active
                .outer
                .is_open()
                .then(|| (active.reservation.clone(), Arc::clone(&active.outer)))
        })
    }

    /// Obtain relay-signed server-reflexive observations for every live UDP
    /// reservation. The caller must still validate each canonical object
    /// against its admitted relay descriptor and reservation.
    pub async fn reflexive_observations(
        &self,
        network_epoch: u64,
    ) -> Result<Vec<(ValidatedRelayReservation, Vec<u8>)>, ReachabilityError> {
        if network_epoch == 0 {
            return Err(ReachabilityError::CorruptState);
        }
        let active: Vec<_> = self
            .active
            .read()
            .await
            .values()
            .filter(|value| {
                value.outer.is_open() && value.outer.transport() == RelayTransportV1::QuicUdp
            })
            .map(|value| (value.reservation.clone(), Arc::clone(&value.outer)))
            .collect();
        let mut output = Vec::with_capacity(active.len());
        for (reservation, outer) in active {
            let bytes = self
                .client
                .observe_reflexive(&reservation, &outer, network_epoch)
                .await?;
            output.push((reservation, bytes));
        }
        Ok(output)
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

fn unix_now() -> Result<u64, ReachabilityError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .map_err(|_| ReachabilityError::CorruptState)
}
