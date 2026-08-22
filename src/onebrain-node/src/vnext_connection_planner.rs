//! Identity-first promotion of authenticated carrier selections.

#![cfg(feature = "vnext-outbound-first")]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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

use crate::vnext_network_runtime::VNextNetworkRuntimeError;
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
    next_sequence: AtomicU64,
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
        if initial_sequence == 0 {
            return Err(RouteFailure::BudgetExceeded);
        }
        self.relay = Some(ProductionRelaySelectionContext {
            discovery,
            reservations,
            signer,
            next_sequence: AtomicU64::new(initial_sequence),
        });
        Ok(self)
    }

    async fn select_relay(
        &self,
        expected_peer: NodeId,
        advertisement: &ValidatedReachabilityAdvertisement,
        deadline: Instant,
    ) -> Result<SelectedCarrier, RouteFailure> {
        let context = self.relay.as_ref().ok_or(RouteFailure::RelayUnavailable)?;
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
            let now = unix_now_seconds()?;
            let expires_at = local_value
                .expires_at
                .min(remote_value.expires_at)
                .min(now.saturating_add(30));
            if expires_at <= now {
                continue;
            }
            let sequence = context.next_sequence.fetch_add(1, Ordering::AcqRel);
            if sequence == 0 || sequence == u64::MAX {
                return Err(RouteFailure::BudgetExceeded);
            }
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
            request.initiator_signature = context
                .signer
                .sign_reachability_message(domain, &unsigned)
                .map_err(|_| RouteFailure::RelayDenied)?;
            let association = self
                .executor
                .associate_relay(&request, &local, remote, Arc::clone(&outer), deadline)
                .await?;
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
            let action =
                ConnectionPlannerExecutor::admitted_relay_action(candidate.clone(), &association)?;
            let admitted = AdmittedRelayExecution::from_validated_association(
                descriptor,
                local,
                remote.clone(),
                association,
                outer,
            )?;
            return self
                .executor
                .execute(
                    action,
                    AdmittedExecutionInput::Relay(admitted),
                    Vec::new(),
                    deadline,
                )
                .await;
        }
        Err(RouteFailure::RelayUnavailable)
    }
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
                        PublicCandidateKindV1::ProviderMapped => {
                            DirectCandidateKindV1::ProviderMapped
                        }
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
                let admitted =
                    match AdmittedDirectExecution::from_public(direct.clone(), public_dial) {
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
