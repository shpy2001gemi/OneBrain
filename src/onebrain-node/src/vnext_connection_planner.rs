//! Identity-first promotion of authenticated carrier selections.

#![cfg(feature = "vnext-outbound-first")]

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::stream::FuturesUnordered;
use futures::{Stream, StreamExt};
use ku_core::foundation::NodeId;
use ku_net::transport::OBPConnection;
use ku_net::vnext_connection_executor::{
    AdmittedDirectExecution, AdmittedExecutionInput, AdmittedRelayExecution,
    AuthenticatedRouteConnection, ConnectionPlannerExecutor, SelectedCarrier,
    ValidatedDirectDialCandidate, VerifiedDirectSelectionV1, VerifiedPlannerSelection,
};
use ku_net::vnext_reachability_crypto::{
    ReachabilityDialValidator, ReachabilityIdentitySigner, ReachabilityLockFreeDialValidation,
    ValidatedReachabilityAdvertisement,
};
use ku_net::vnext_relay_discovery::{ReachabilityFuture, RelayDiscovery, VerifiedRelayDiscovery};
use ku_net::vnext_resource_gate::SessionAdmission;
use ku_net::vnext_route_plan::RouteFailure;
use ku_net::vnext_session::AuthenticatedSession;
use onebrain_protocol::{
    connectivity_signing_parts, ConnectivitySignalingV1, ConnectivitySignatureRoleV1,
    DirectCandidateKindV1, DirectCandidateV1, PublicCandidateKindV1, ReachabilityEndpointV1,
    RelayCandidateV1, RelayConnectRequestV1, RelayEndpointV1, RelayTransportV1, RoutePathKindV1,
};
use rand::rngs::OsRng;
use rand::RngCore;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::vnext_network_runtime::{
    PendingAuthenticatedRoute, VNextNetworkRuntime, VNextNetworkRuntimeError,
};
use crate::vnext_outbox::DurableCheckpointV1;
use crate::vnext_reachability_manager::RelayReservationManager;

pub trait ExpectedPeerConnector: Send + Sync {
    fn connect_expected<'a>(
        &'a self,
        expected_peer: NodeId,
        advertisement: &'a ValidatedReachabilityAdvertisement,
    ) -> ReachabilityFuture<'a, Result<AuthenticatedRouteConnection, RouteFailure>>;
}

/// Selects a concrete carrier from an already admitted peer advertisement.
/// The selector never grants peer authority: the runtime must still perform
/// the OBP identity handshake over the returned carrier before promotion.
pub trait ExpectedPeerCarrierSelector: Send + Sync {
    fn select_expected<'a>(
        &'a self,
        expected_peer: NodeId,
        advertisement: &'a ValidatedReachabilityAdvertisement,
    ) -> ReachabilityFuture<'a, Result<SelectedCarrier, RouteFailure>>;

    /// Select and authenticate one route to the expected peer. Raw carrier
    /// readiness is not sufficient to choose a winner: an implementation must
    /// either authenticate candidates sequentially or keep every raced carrier
    /// alive until one completes the OBP peer handshake.
    fn connect_expected<'a>(
        &'a self,
        runtime: &'a VNextNetworkRuntime,
        expected_peer: NodeId,
        advertisement: &'a ValidatedReachabilityAdvertisement,
    ) -> ReachabilityFuture<'a, Result<RoutedVNextSession, VNextNetworkRuntimeError>> {
        Box::pin(async move {
            let selected = self
                .select_expected(expected_peer, advertisement)
                .await
                .map_err(route_failure_runtime)?;
            runtime
                .connect_expected_selected(expected_peer, selected)
                .await
        })
    }
}

/// Production direct-path selector. Public candidates are re-resolved at the
/// moment of dialing and the resulting socket is sealed by the connection
/// executor; caller-supplied addresses never enter this path.
pub struct ProductionExpectedPeerCarrierSelector {
    dial_validator: Arc<ReachabilityDialValidator>,
    executor: Arc<ConnectionPlannerExecutor>,
    route_deadline: Duration,
    relay: Option<ProductionRelaySelectionContext>,
}

struct ProductionRelaySelectionContext {
    discovery: Arc<RwLock<RelayDiscovery>>,
    reservations: Arc<RelayReservationManager>,
    signer: Arc<dyn ReachabilityIdentitySigner>,
    connect_sequences: Arc<RelayConnectSequenceAllocator>,
}

type RelayConnectSequenceScope = ([u8; 32], [u8; 32], [u8; 32], [u8; 32]);

const MAX_RELAY_CONNECT_SEQUENCE_SCOPES: usize = 65_536;

struct RelayConnectSequenceAllocator {
    max_scopes: usize,
    next_by_scope: Mutex<BTreeMap<RelayConnectSequenceScope, (u64, u64)>>,
}

impl RelayConnectSequenceAllocator {
    fn new(initial_sequence: u64) -> Result<Self, RouteFailure> {
        Self::new_with_limit(initial_sequence, MAX_RELAY_CONNECT_SEQUENCE_SCOPES)
    }

    fn new_with_limit(initial_sequence: u64, max_scopes: usize) -> Result<Self, RouteFailure> {
        // The connectivity replay store requires the first request in every
        // exact reservation scope to start at one. A non-one global cursor
        // cannot safely initialize a newly encountered scope.
        if initial_sequence != 1 || max_scopes == 0 {
            return Err(RouteFailure::BudgetExceeded);
        }
        Ok(Self {
            max_scopes,
            next_by_scope: Mutex::new(BTreeMap::new()),
        })
    }

    fn next(
        &self,
        scope: RelayConnectSequenceScope,
        expires_at: u64,
        now: u64,
    ) -> Result<u64, RouteFailure> {
        if expires_at <= now {
            return Err(RouteFailure::CandidateExpired);
        }
        let mut next_by_scope = self
            .next_by_scope
            .lock()
            .map_err(|_| RouteFailure::RelayUnavailable)?;
        if let Some((next, retained_until)) = next_by_scope.get_mut(&scope) {
            let sequence = *next;
            *next = sequence
                .checked_add(1)
                .ok_or(RouteFailure::BudgetExceeded)?;
            *retained_until = (*retained_until).max(expires_at);
            return Ok(sequence);
        }
        if next_by_scope.len() >= self.max_scopes {
            next_by_scope.retain(|_, (_, retained_until)| *retained_until > now);
        }
        if next_by_scope.len() >= self.max_scopes {
            return Err(RouteFailure::BudgetExceeded);
        }
        next_by_scope.insert(scope, (2, expires_at));
        Ok(1)
    }
}

impl ProductionExpectedPeerCarrierSelector {
    pub fn new(
        dial_validator: Arc<ReachabilityDialValidator>,
        executor: Arc<ConnectionPlannerExecutor>,
        route_deadline: Duration,
    ) -> Result<Self, RouteFailure> {
        if route_deadline.is_zero() || route_deadline > Duration::from_secs(20) {
            return Err(RouteFailure::BudgetExceeded);
        }
        Ok(Self {
            dial_validator,
            executor,
            route_deadline,
            relay: None,
        })
    }

    pub fn with_relay(
        mut self,
        discovery: Arc<RwLock<RelayDiscovery>>,
        reservations: Arc<RelayReservationManager>,
        signer: Arc<dyn ReachabilityIdentitySigner>,
        initial_sequence: u64,
    ) -> Result<Self, RouteFailure> {
        let connect_sequences = Arc::new(RelayConnectSequenceAllocator::new(initial_sequence)?);
        self.relay = Some(ProductionRelaySelectionContext {
            discovery,
            reservations,
            signer,
            connect_sequences,
        });
        Ok(self)
    }

    async fn select_direct(
        &self,
        advertisement: &ValidatedReachabilityAdvertisement,
        deadline: Instant,
    ) -> Result<SelectedCarrier, RouteFailure> {
        let canonical = advertisement.canonical();
        let mut last_failure = RouteFailure::DirectTimeout;
        for (index, public) in canonical.optional_public_candidates.iter().enumerate() {
            if Instant::now() >= deadline {
                return Err(RouteFailure::DirectTimeout);
            }
            let direct = DirectCandidateV1 {
                endpoint: public.endpoint.clone(),
                kind: match public.kind {
                    PublicCandidateKindV1::ServerReflexive => {
                        DirectCandidateKindV1::ServerReflexive
                    }
                    PublicCandidateKindV1::ProviderMapped => DirectCandidateKindV1::ProviderMapped,
                },
                priority: public.priority,
                network_epoch: canonical.sequence,
                expires_at: canonical.expires_at,
            };
            let public_dial = match self
                .dial_validator
                .validate_public_candidate_dial(advertisement, index, deadline)
                .await
            {
                Ok(value) => value,
                Err(_) => {
                    last_failure = RouteFailure::CandidateExpired;
                    continue;
                }
            };
            let admitted = match AdmittedDirectExecution::from_public(direct.clone(), public_dial) {
                Ok(value) => value,
                Err(error) => {
                    last_failure = error;
                    continue;
                }
            };
            match self
                .executor
                .execute(
                    ku_net::vnext_route_plan::PlannerAction::CheckDirect(direct),
                    AdmittedExecutionInput::Direct(admitted),
                    Vec::new(),
                    deadline,
                )
                .await
            {
                Ok(selected) => return Ok(selected),
                Err(error) => last_failure = error,
            }
        }
        Err(last_failure)
    }

    async fn relay_carrier_attempts(
        &self,
        expected_peer: NodeId,
        advertisement: &ValidatedReachabilityAdvertisement,
        deadline: Instant,
    ) -> Result<Vec<ReachabilityFuture<'static, Result<SelectedCarrier, RouteFailure>>>, RouteFailure>
    {
        let context = self.relay.as_ref().ok_or(RouteFailure::RelayUnavailable)?;
        let mut attempts = Vec::new();
        for remote in advertisement.reservations() {
            if Instant::now() >= deadline {
                return Err(RouteFailure::RelayUnavailable);
            }
            let remote_value = remote.canonical();
            if remote_value.target_node_id != expected_peer {
                return Err(RouteFailure::PeerIdentityMismatch);
            }
            let relay_id = remote_value.relay_node_id;
            let descriptor = {
                let discovery = context.discovery.read().await;
                let matched = discovery
                    .verified_relays()
                    .find(|value| value.canonical().relay_node_id == relay_id)
                    .cloned();
                matched
            };
            let Some(descriptor) = descriptor else {
                continue;
            };
            let Some((local, outer)) = context.reservations.active_for(relay_id).await else {
                continue;
            };
            let local_value = local.canonical();
            if local_value.target_node_id
                != ku_net::vnext_session::principal_node_id(&context.signer.public_key())
                || !local_value.transport_scope.contains(&outer.transport())
                || !remote_value.transport_scope.contains(&outer.transport())
            {
                return Err(RouteFailure::PeerIdentityMismatch);
            }
            let executor = Arc::clone(&self.executor);
            let signer = Arc::clone(&context.signer);
            let connect_sequences = Arc::clone(&context.connect_sequences);
            let remote = remote.clone();
            attempts.push(Box::pin(async move {
                // Allocate only when this candidate is actually attempted.
                // Each relay reservation pair is a distinct replay scope, so
                // a second relay must start at sequence one independently.
                let local_value = local.canonical();
                let remote_value = remote.canonical();
                let scope = (
                    *local_value.target_node_id.as_bytes(),
                    *expected_peer.as_bytes(),
                    local_value.reservation_id,
                    remote_value.reservation_id,
                );
                let now = unix_now_seconds()?;
                let expires_at = local_value
                    .expires_at
                    .min(remote_value.expires_at)
                    .min(now.saturating_add(30));
                if expires_at <= now {
                    return Err(RouteFailure::CandidateExpired);
                }
                let sequence = connect_sequences.next(scope, expires_at, now)?;
                let mut nonce = [0u8; 32];
                OsRng.fill_bytes(&mut nonce);
                let mut request = RelayConnectRequestV1 {
                    format: 1,
                    initiator_node_id: local_value.target_node_id,
                    target_node_id: expected_peer,
                    initiator_reservation_id: local_value.reservation_id,
                    target_reservation_id: remote_value.reservation_id,
                    nonce,
                    sequence,
                    issued_at: now,
                    expires_at,
                    initiator_signature: [0; 64],
                };
                let root = ConnectivitySignalingV1::RelayConnectRequest(request.clone());
                let (domain, unsigned) = connectivity_signing_parts(
                    &root,
                    ConnectivitySignatureRoleV1::RelayConnectInitiator,
                )
                .map_err(|_| RouteFailure::RelayDenied)?;
                request.initiator_signature = signer
                    .sign_reachability_message(domain, &unsigned)
                    .map_err(|_| RouteFailure::RelayDenied)?;
                let public = outer.public_endpoint();
                let candidate = RelayCandidateV1 {
                    relay_node_id: relay_id,
                    reservation_id: local_value.reservation_id,
                    transport: outer.transport(),
                    endpoint: ReachabilityEndpointV1 {
                        host: public.host.clone(),
                        port: public.port,
                    },
                    priority: 1,
                    expires_at,
                };
                let association = executor
                    .associate_relay(&request, &local, &remote, Arc::clone(&outer), deadline)
                    .await
                    .map_err(|error| {
                        RouteFailure::RelayPathFailed(format!("outbound association: {error:?}"))
                    })?;
                let action = ConnectionPlannerExecutor::admitted_relay_action(
                    candidate.clone(),
                    &association,
                )
                .map_err(|error| {
                    RouteFailure::RelayPathFailed(format!(
                        "outbound association binding: {error:?}"
                    ))
                })?;
                let admitted = AdmittedRelayExecution::from_validated_association(
                    descriptor,
                    local,
                    remote,
                    association,
                    outer,
                )
                .map_err(|error| {
                    RouteFailure::RelayPathFailed(format!("outbound admitted path: {error:?}"))
                })?;
                executor
                    .execute(
                        action,
                        AdmittedExecutionInput::Relay(admitted),
                        Vec::new(),
                        deadline,
                    )
                    .await
                    .map_err(|error| {
                        RouteFailure::RelayPathFailed(format!("outbound inner carrier: {error:?}"))
                    })
            })
                as ReachabilityFuture<'static, Result<SelectedCarrier, RouteFailure>>);
        }
        if attempts.is_empty() {
            return Err(RouteFailure::RelayUnavailable);
        }
        Ok(attempts)
    }

    async fn select_relay(
        &self,
        expected_peer: NodeId,
        advertisement: &ValidatedReachabilityAdvertisement,
        deadline: Instant,
    ) -> Result<SelectedCarrier, RouteFailure> {
        let attempts: FuturesUnordered<_> = self
            .relay_carrier_attempts(expected_peer, advertisement, deadline)
            .await?
            .into_iter()
            .collect();
        await_first_relay_attempt(attempts, deadline).await
    }

    async fn connect_relay_authenticated(
        &self,
        runtime: &VNextNetworkRuntime,
        expected_peer: NodeId,
        advertisement: &ValidatedReachabilityAdvertisement,
        deadline: Instant,
    ) -> Result<RoutedVNextSession, VNextNetworkRuntimeError> {
        let carrier_attempts = self
            .relay_carrier_attempts(expected_peer, advertisement, deadline)
            .await
            .map_err(route_failure_runtime)?;
        // Only one initiator association may be live at a time. Racing two
        // fully authenticated associations can let each endpoint promote a
        // different successful relay, after which both endpoints drop the
        // peer half of the route they retained. Inbound may wait on every
        // common relay, but the initiator advances through candidates in a
        // bounded sequence so there is exactly one possible winner.
        let mut attempts = Vec::with_capacity(carrier_attempts.len());
        for carrier_attempt in carrier_attempts {
            attempts.push(Box::pin(async move {
                let selected = carrier_attempt.await.map_err(route_failure_runtime)?;
                runtime
                    .authenticate_expected_candidate(expected_peer, selected)
                    .await
            })
                as ReachabilityFuture<
                    '_,
                    Result<PendingAuthenticatedRoute, VNextNetworkRuntimeError>,
                >);
        }
        let pending = await_authenticated_relay_attempts(attempts, deadline).await?;
        runtime.promote_expected_candidate(expected_peer, pending)
    }
}

/// Relay candidates are independently authenticated paths. A failed, closed,
/// or peer-mismatched relay must not suppress the remaining owner-admitted
/// candidates. A mismatch never grants authority: that path is discarded and
/// the race only succeeds if another path authenticates the expected peer.
/// Network-epoch and budget failures remain global and fail closed.
fn record_relay_attempt_failure(
    last_failure: &mut RouteFailure,
    error: RouteFailure,
) -> Result<(), RouteFailure> {
    if matches!(
        error,
        RouteFailure::NetworkChanged
            | RouteFailure::BudgetExceeded
            | RouteFailure::PathLimited { .. }
    ) {
        return Err(error);
    }
    *last_failure = error;
    Ok(())
}

async fn await_first_relay_attempt<S, T>(
    mut attempts: S,
    deadline: Instant,
) -> Result<T, RouteFailure>
where
    S: Stream<Item = Result<T, RouteFailure>> + Unpin,
{
    let deadline = tokio::time::Instant::from_std(deadline);
    let mut last_failure = RouteFailure::RelayUnavailable;
    loop {
        match tokio::time::timeout_at(deadline, attempts.next()).await {
            Ok(Some(Ok(value))) => return Ok(value),
            Ok(Some(Err(error))) => {
                record_relay_attempt_failure(&mut last_failure, error)?;
            }
            Ok(None) => return Err(last_failure),
            Err(_) => return Err(last_failure),
        }
    }
}

async fn await_authenticated_relay_attempts<T>(
    attempts: Vec<ReachabilityFuture<'_, Result<T, VNextNetworkRuntimeError>>>,
    deadline: Instant,
) -> Result<T, VNextNetworkRuntimeError> {
    let mut last_failure = VNextNetworkRuntimeError::Session(
        "all relay carrier authentication attempts failed".into(),
    );
    let mut remaining_attempts = attempts.len();
    for attempt in attempts {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        let remaining = deadline.saturating_duration_since(now);
        let slice = remaining / u32::try_from(remaining_attempts).unwrap_or(u32::MAX);
        let attempt_deadline = tokio::time::Instant::from_std(now + slice);
        match tokio::time::timeout_at(attempt_deadline, attempt).await {
            Ok(Ok(value)) => return Ok(value),
            Ok(Err(error)) => last_failure = error,
            Err(_) => {
                last_failure = VNextNetworkRuntimeError::HandshakeTimeout;
            }
        }
        remaining_attempts = remaining_attempts.saturating_sub(1);
    }
    Err(last_failure)
}

impl ExpectedPeerCarrierSelector for ProductionExpectedPeerCarrierSelector {
    fn select_expected<'a>(
        &'a self,
        expected_peer: NodeId,
        advertisement: &'a ValidatedReachabilityAdvertisement,
    ) -> ReachabilityFuture<'a, Result<SelectedCarrier, RouteFailure>> {
        Box::pin(async move {
            let canonical = advertisement.canonical();
            if canonical.target_node_id != expected_peer || canonical.sequence == 0 {
                return Err(RouteFailure::PeerIdentityMismatch);
            }
            let deadline = Instant::now() + self.route_deadline;
            let direct = self.select_direct(advertisement, deadline).await;
            if let Ok(selected) = direct {
                return Ok(selected);
            }
            let last_failure = direct.expect_err("successful direct selection returned above");
            match self
                .select_relay(expected_peer, advertisement, deadline)
                .await
            {
                Ok(selected) => Ok(selected),
                Err(RouteFailure::RelayUnavailable) if self.relay.is_none() => Err(last_failure),
                Err(error) => Err(error),
            }
        })
    }

    fn connect_expected<'a>(
        &'a self,
        runtime: &'a VNextNetworkRuntime,
        expected_peer: NodeId,
        advertisement: &'a ValidatedReachabilityAdvertisement,
    ) -> ReachabilityFuture<'a, Result<RoutedVNextSession, VNextNetworkRuntimeError>> {
        Box::pin(async move {
            let canonical = advertisement.canonical();
            if canonical.target_node_id != expected_peer || canonical.sequence == 0 {
                return Err(route_failure_runtime(RouteFailure::PeerIdentityMismatch));
            }
            let deadline = Instant::now() + self.route_deadline;
            match self.select_direct(advertisement, deadline).await {
                Ok(selected) => {
                    runtime
                        .connect_expected_selected(expected_peer, selected)
                        .await
                }
                Err(direct_failure) => {
                    if self.relay.is_none() {
                        Err(route_failure_runtime(direct_failure))
                    } else {
                        self.connect_relay_authenticated(
                            runtime,
                            expected_peer,
                            advertisement,
                            deadline,
                        )
                        .await
                    }
                }
            }
        })
    }
}

fn route_failure_runtime(error: RouteFailure) -> VNextNetworkRuntimeError {
    VNextNetworkRuntimeError::Session(format!("outbound route failed: {error:?}"))
}

fn unix_now_seconds() -> Result<u64, RouteFailure> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .map_err(|_| RouteFailure::NetworkChanged)
}

pub trait RoutedNetworkRuntime: Send + Sync {
    fn connect_expected<'a>(
        &'a self,
        expected_peer: NodeId,
        advertisement: &'a ValidatedReachabilityAdvertisement,
    ) -> ReachabilityFuture<'a, Result<RoutedVNextSession, VNextNetworkRuntimeError>>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoutedDeliveryState {
    Active,
    Quiescing,
    Replanning,
    Reauthenticating,
    Resuming,
}

#[derive(Clone)]
pub struct RoutedDeliveryGate {
    state: Arc<Mutex<RoutedDeliveryState>>,
}

impl Default for RoutedDeliveryGate {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(RoutedDeliveryState::Active)),
        }
    }
}

impl RoutedDeliveryGate {
    pub fn state(&self) -> Result<RoutedDeliveryState, RoutedSessionError> {
        self.state
            .lock()
            .map(|state| *state)
            .map_err(|_| RoutedSessionError::StateUnavailable)
    }

    pub fn writes_open(&self) -> Result<bool, RoutedSessionError> {
        self.state()
            .map(|state| state == RoutedDeliveryState::Active)
    }

    pub fn transition(&self, next: RoutedDeliveryState) -> Result<(), RoutedSessionError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| RoutedSessionError::StateUnavailable)?;
        let valid = matches!(
            (*state, next),
            (RoutedDeliveryState::Active, RoutedDeliveryState::Quiescing)
                | (
                    RoutedDeliveryState::Quiescing,
                    RoutedDeliveryState::Replanning
                )
                | (
                    RoutedDeliveryState::Replanning,
                    RoutedDeliveryState::Reauthenticating
                )
                | (
                    RoutedDeliveryState::Reauthenticating,
                    RoutedDeliveryState::Resuming
                )
                | (RoutedDeliveryState::Resuming, RoutedDeliveryState::Active)
        );
        if !valid {
            return Err(RoutedSessionError::InvalidStateTransition);
        }
        *state = next;
        Ok(())
    }
}

pub trait RoutedRecovery: Send + Sync {
    fn recover_from_carrier_failure<'a>(
        &'a self,
        expected_peer: NodeId,
        failed: VerifiedCarrierIdentity,
        acknowledged_checkpoint: DurableCheckpointV1,
        advertisement: &'a ValidatedReachabilityAdvertisement,
    ) -> ReachabilityFuture<'a, Result<RoutedVNextSession, VNextNetworkRuntimeError>>;

    fn upgrade_to_direct<'a>(
        &'a self,
        current: RoutedVNextSession,
        candidate: ValidatedDirectDialCandidate,
        acknowledged_checkpoint: DurableCheckpointV1,
    ) -> ReachabilityFuture<'a, Result<RoutedVNextSession, VNextNetworkRuntimeError>>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerifiedDirectProvenanceV1 {
    OutboundCandidate {
        endpoint: ReachabilityEndpointV1,
        candidate_kind: DirectCandidateKindV1,
        network_epoch: u64,
    },
    InboundObserved,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerifiedCarrierIdentity {
    Direct {
        connected_socket: SocketAddr,
        provenance: VerifiedDirectProvenanceV1,
    },
    HolePunched {
        relay_node_id: NodeId,
        local_reservation_id: [u8; 32],
        remote_reservation_id: [u8; 32],
        schedule_digest: [u8; 32],
        connected_socket: SocketAddr,
    },
    Relay {
        relay_node_id: NodeId,
        association_id: [u8; 32],
        local_reservation_id: [u8; 32],
        remote_reservation_id: [u8; 32],
        endpoint: RelayEndpointV1,
        connected_socket: SocketAddr,
        outer_connection_binding: [u8; 32],
        transport: RelayTransportV1,
    },
}

impl VerifiedCarrierIdentity {
    pub fn path_kind(&self) -> RoutePathKindV1 {
        match self {
            Self::Direct { .. } => RoutePathKindV1::Direct,
            Self::HolePunched { .. } => RoutePathKindV1::HolePunched,
            Self::Relay {
                transport: RelayTransportV1::QuicUdp,
                ..
            } => RoutePathKindV1::RelayUdp,
            Self::Relay {
                transport: RelayTransportV1::TlsTcp443,
                ..
            } => RoutePathKindV1::RelayTcp443,
        }
    }

    pub fn connected_socket(&self) -> SocketAddr {
        match self {
            Self::Direct {
                connected_socket, ..
            }
            | Self::HolePunched {
                connected_socket, ..
            }
            | Self::Relay {
                connected_socket, ..
            } => *connected_socket,
        }
    }
}

pub struct RoutedVNextSession {
    expected_peer: NodeId,
    authenticated: AuthenticatedSession,
    connection: OBPConnection,
    carrier: VerifiedCarrierIdentity,
    transport_binding_digest: [u8; 32],
    checkpoint: Option<DurableCheckpointV1>,
    admission: Option<SessionAdmission>,
}

impl RoutedVNextSession {
    pub(crate) fn promote(
        expected_peer: NodeId,
        authenticated: AuthenticatedRouteConnection,
    ) -> Result<Self, RoutedSessionError> {
        if authenticated.authenticated_peer() != expected_peer {
            return Err(RoutedSessionError::PeerIdentityMismatch);
        }
        let binding = authenticated.transport_binding_digest();
        if binding == [0; 32] {
            return Err(RoutedSessionError::InvalidCarrier);
        }
        let (session, connection, selection) = authenticated.into_parts();
        if session.initiator != expected_peer && session.responder != expected_peer {
            return Err(RoutedSessionError::PeerIdentityMismatch);
        }
        let carrier = carrier_from_selection(&selection)?;
        Ok(Self {
            expected_peer,
            authenticated: session,
            connection,
            carrier,
            transport_binding_digest: binding,
            checkpoint: None,
            admission: None,
        })
    }

    pub fn expected_peer(&self) -> NodeId {
        self.expected_peer
    }

    pub fn authenticated(&self) -> &AuthenticatedSession {
        &self.authenticated
    }

    pub fn carrier(&self) -> &VerifiedCarrierIdentity {
        &self.carrier
    }

    pub fn transport_binding_digest(&self) -> [u8; 32] {
        self.transport_binding_digest
    }

    pub fn checkpoint(&self) -> Option<&DurableCheckpointV1> {
        self.checkpoint.as_ref()
    }

    pub fn route_receipt_digest(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:routed-session:1\0");
        hasher.update(self.expected_peer.as_bytes());
        hasher.update(&self.authenticated.session_id);
        hasher.update(&self.transport_binding_digest);
        hasher.update(&[self.carrier.path_kind() as u8]);
        *hasher.finalize().as_bytes()
    }

    pub fn attach_checkpoint(
        &mut self,
        checkpoint: DurableCheckpointV1,
    ) -> Result<(), RoutedSessionError> {
        if checkpoint.expected_peer() != self.expected_peer {
            return Err(RoutedSessionError::PeerIdentityMismatch);
        }
        self.checkpoint = Some(checkpoint);
        Ok(())
    }

    pub(crate) fn attach_admission(&mut self, admission: SessionAdmission) {
        self.admission = Some(admission);
    }

    pub fn close(&self) {
        self.connection.close("OBP routed session complete");
    }

    pub async fn send_uni(&self, payload: &[u8]) -> Result<(), RoutedSessionError> {
        if payload.is_empty() {
            return Err(RoutedSessionError::InvalidCarrier);
        }
        self.connection
            .send_uni(payload)
            .await
            .map_err(|error| RoutedSessionError::Transport(error.to_string()))
    }

    pub async fn recv_uni(&self, max_bytes: usize) -> Result<Vec<u8>, RoutedSessionError> {
        if max_bytes == 0 || max_bytes > 4_096 {
            return Err(RoutedSessionError::InvalidCarrier);
        }
        self.connection
            .recv_uni_with_limit(max_bytes)
            .await
            .map_err(|error| RoutedSessionError::Transport(error.to_string()))
    }
}

fn carrier_from_selection(
    selection: &VerifiedPlannerSelection,
) -> Result<VerifiedCarrierIdentity, RoutedSessionError> {
    match selection.path_kind() {
        RoutePathKindV1::Direct => match selection.direct() {
            Some(VerifiedDirectSelectionV1::OutboundCandidate {
                endpoint,
                connected_socket,
                candidate_kind,
                network_epoch,
            }) => Ok(VerifiedCarrierIdentity::Direct {
                connected_socket: *connected_socket,
                provenance: VerifiedDirectProvenanceV1::OutboundCandidate {
                    endpoint: endpoint.clone(),
                    candidate_kind: *candidate_kind,
                    network_epoch: *network_epoch,
                },
            }),
            Some(VerifiedDirectSelectionV1::InboundObserved { connected_socket }) => {
                Ok(VerifiedCarrierIdentity::Direct {
                    connected_socket: *connected_socket,
                    provenance: VerifiedDirectProvenanceV1::InboundObserved,
                })
            }
            None => Err(RoutedSessionError::InvalidCarrier),
        },
        RoutePathKindV1::HolePunched => {
            let value = selection
                .hole_punch()
                .ok_or(RoutedSessionError::InvalidCarrier)?;
            let (local_reservation_id, remote_reservation_id) = value.reservation_ids();
            Ok(VerifiedCarrierIdentity::HolePunched {
                relay_node_id: value.relay_node_id(),
                local_reservation_id,
                remote_reservation_id,
                schedule_digest: value.schedule_digest(),
                connected_socket: value.connected_socket(),
            })
        }
        RoutePathKindV1::RelayUdp | RoutePathKindV1::RelayTcp443 => {
            let value = selection
                .relay()
                .ok_or(RoutedSessionError::InvalidCarrier)?;
            let (local_reservation_id, remote_reservation_id) = value.reservation_ids();
            Ok(VerifiedCarrierIdentity::Relay {
                relay_node_id: value.relay_node_id(),
                association_id: value.association_id(),
                local_reservation_id,
                remote_reservation_id,
                endpoint: value.endpoint().clone(),
                connected_socket: value.connected_socket(),
                outer_connection_binding: value.outer_connection_binding(),
                transport: value.transport(),
            })
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RoutedSessionError {
    #[error("authenticated peer does not match the expected peer")]
    PeerIdentityMismatch,
    #[error("selected carrier provenance is incomplete or inconsistent")]
    InvalidCarrier,
    #[error("routed delivery state is unavailable")]
    StateUnavailable,
    #[error("routed delivery state transition is invalid")]
    InvalidStateTransition,
    #[error("routed carrier transport failed: {0}")]
    Transport(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn relay_connect_sequences_start_at_one_per_exact_reservation_scope() {
        let allocator = RelayConnectSequenceAllocator::new(1).expect("valid replay sequence");
        let first_scope = ([1; 32], [2; 32], [3; 32], [4; 32]);
        let second_scope = ([1; 32], [2; 32], [5; 32], [6; 32]);

        assert_eq!(allocator.next(first_scope, 200, 100), Ok(1));
        assert_eq!(allocator.next(second_scope, 200, 100), Ok(1));
        assert_eq!(allocator.next(first_scope, 200, 100), Ok(2));
        assert_eq!(allocator.next(second_scope, 200, 100), Ok(2));
    }

    #[test]
    fn relay_connect_sequences_reject_noncanonical_initial_value() {
        assert!(matches!(
            RelayConnectSequenceAllocator::new(2),
            Err(RouteFailure::BudgetExceeded)
        ));
    }

    #[test]
    fn relay_connect_sequence_scopes_are_bounded_and_expiry_reclaims_capacity() {
        let allocator =
            RelayConnectSequenceAllocator::new_with_limit(1, 1).expect("bounded allocator");
        let first_scope = ([1; 32], [2; 32], [3; 32], [4; 32]);
        let second_scope = ([1; 32], [2; 32], [5; 32], [6; 32]);

        assert_eq!(allocator.next(first_scope, 200, 100), Ok(1));
        assert_eq!(
            allocator.next(second_scope, 300, 150),
            Err(RouteFailure::BudgetExceeded)
        );
        assert_eq!(allocator.next(second_scope, 300, 201), Ok(1));
    }

    #[test]
    fn relay_candidate_failure_policy_retries_path_local_failures() {
        let mut last = RouteFailure::RelayUnavailable;
        assert_eq!(
            record_relay_attempt_failure(&mut last, RouteFailure::RelayDenied),
            Ok(())
        );
        assert_eq!(last, RouteFailure::RelayDenied);
        assert_eq!(
            record_relay_attempt_failure(&mut last, RouteFailure::RelayUnavailable),
            Ok(())
        );
        assert_eq!(last, RouteFailure::RelayUnavailable);
        assert_eq!(
            record_relay_attempt_failure(&mut last, RouteFailure::PeerIdentityMismatch),
            Ok(())
        );
        assert_eq!(last, RouteFailure::PeerIdentityMismatch);

        assert_eq!(
            record_relay_attempt_failure(&mut last, RouteFailure::NetworkChanged),
            Err(RouteFailure::NetworkChanged)
        );
    }

    #[tokio::test]
    async fn outbound_relay_race_keeps_working_when_the_first_candidate_fails() {
        let attempts = FuturesUnordered::new();
        attempts.push(Box::pin(async { Err::<u8, _>(RouteFailure::RelayDenied) })
            as ReachabilityFuture<'static, _>);
        attempts.push(Box::pin(async {
            tokio::task::yield_now().await;
            Ok::<u8, RouteFailure>(7)
        }) as ReachabilityFuture<'static, _>);

        assert_eq!(
            await_first_relay_attempt(attempts, Instant::now() + Duration::from_secs(1)).await,
            Ok(7u8)
        );
    }

    #[tokio::test]
    async fn outbound_relay_race_keeps_working_after_peer_mismatch() {
        let attempts = FuturesUnordered::new();
        attempts.push(
            Box::pin(async { Err::<u8, _>(RouteFailure::PeerIdentityMismatch) })
                as ReachabilityFuture<'static, _>,
        );
        attempts.push(Box::pin(async {
            tokio::task::yield_now().await;
            Ok::<u8, RouteFailure>(9)
        }) as ReachabilityFuture<'static, _>);

        assert_eq!(
            await_first_relay_attempt(attempts, Instant::now() + Duration::from_secs(1)).await,
            Ok(9u8)
        );
    }

    #[tokio::test]
    async fn outbound_relay_race_fails_closed_when_every_peer_mismatches() {
        let attempts = FuturesUnordered::new();
        attempts.push(
            Box::pin(async { Err::<u8, _>(RouteFailure::PeerIdentityMismatch) })
                as ReachabilityFuture<'static, _>,
        );
        attempts.push(Box::pin(async {
            tokio::task::yield_now().await;
            Err::<u8, _>(RouteFailure::PeerIdentityMismatch)
        }) as ReachabilityFuture<'static, _>);

        assert_eq!(
            await_first_relay_attempt(attempts, Instant::now() + Duration::from_secs(1)).await,
            Err(RouteFailure::PeerIdentityMismatch)
        );
    }

    #[tokio::test]
    async fn authenticated_relay_selection_advances_only_after_a_failed_handshake() {
        let stage = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut attempts = Vec::new();
        let first_stage = Arc::clone(&stage);
        attempts.push(Box::pin(async move {
            // This represents the carrier that became ready first but failed
            // the expected-peer OBP handshake.
            assert_eq!(first_stage.fetch_add(1, Ordering::SeqCst), 0);
            tokio::task::yield_now().await;
            first_stage.store(2, Ordering::SeqCst);
            Err::<u8, _>(VNextNetworkRuntimeError::Session(
                "first raw carrier failed peer authentication".into(),
            ))
        }) as ReachabilityFuture<'static, _>);
        let second_stage = Arc::clone(&stage);
        attempts.push(Box::pin(async move {
            assert_eq!(second_stage.load(Ordering::SeqCst), 2);
            Ok::<u8, VNextNetworkRuntimeError>(11)
        }) as ReachabilityFuture<'static, _>);

        assert_eq!(
            await_authenticated_relay_attempts(attempts, Instant::now() + Duration::from_secs(1))
                .await
                .expect("second authenticated relay must remain eligible"),
            11u8
        );
    }

    #[tokio::test]
    async fn authenticated_relay_selection_bounds_a_stalled_candidate_before_failover() {
        let mut attempts = Vec::new();
        attempts.push(
            Box::pin(std::future::pending::<Result<u8, VNextNetworkRuntimeError>>())
                as ReachabilityFuture<'static, _>,
        );
        attempts.push(Box::pin(async { Ok::<u8, VNextNetworkRuntimeError>(13) })
            as ReachabilityFuture<'static, _>);

        assert_eq!(
            await_authenticated_relay_attempts(
                attempts,
                Instant::now() + Duration::from_millis(100)
            )
            .await
            .expect("a stalled first relay must leave time for the next candidate"),
            13u8
        );
    }
}
