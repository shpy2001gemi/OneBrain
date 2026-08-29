//! Sealed carrier selection and execution without granting route authority.

use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use ku_core::foundation::NodeId;
use onebrain_protocol::{
    decode_connectivity_signaling, encode_connectivity_signaling, ConnectivitySignalingV1,
    DirectCandidateKindV1, DirectCandidateV1, HostAddressV1, ReachabilityEndpointV1,
    RelayConnectRequestV1, RelayEndpointV1, RelayTransportV1, RelayWireFrameV1, RelayWireKindV1,
    RouteAttemptOutcomeV1, RouteAttemptV1, RoutePathKindV1, MAX_ROUTE_PLAN_ATTEMPTS,
};
use rand::rngs::OsRng;
use rand::RngCore;

use crate::transport::{OBPConnection, QuicTransport, TransportConfig};
use crate::vnext_connectivity_signaling::{
    ConnectivitySignalingValidator, ValidatedPunchedCarrier, ValidatedRelayAssociation,
};
use crate::vnext_reachability_crypto::{
    InMemoryReachabilityReplayStore, KnownPeerIdentity, ValidatedPublicDialEndpoint,
    ValidatedPublicDialTransportV1, ValidatedRelayDescriptor, ValidatedRelayReservation,
};
use crate::vnext_relay_discovery::ReachabilityFuture;
use crate::vnext_relay_tunnel::{
    AuthenticatedOuterRelayConnection, RelayDatagramSocket, RelayInboundDatagram,
    RelaySocketDriver, RelaySocketGlobalBudget,
};
use crate::vnext_route_plan::{PlannerAction, RouteFailure};
use crate::vnext_session::AuthenticatedSession;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerifiedDirectSelectionV1 {
    OutboundCandidate {
        endpoint: ReachabilityEndpointV1,
        connected_socket: SocketAddr,
        candidate_kind: DirectCandidateKindV1,
        network_epoch: u64,
    },
    InboundObserved {
        connected_socket: SocketAddr,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedRelaySelectionV1 {
    relay_node_id: NodeId,
    association_id: [u8; 32],
    local_reservation_id: [u8; 32],
    remote_reservation_id: [u8; 32],
    endpoint: RelayEndpointV1,
    connected_socket: SocketAddr,
    outer_connection_binding: [u8; 32],
    transport: RelayTransportV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedHolePunchSelectionV1 {
    relay_node_id: NodeId,
    local_reservation_id: [u8; 32],
    remote_reservation_id: [u8; 32],
    schedule_digest: [u8; 32],
    endpoint: ReachabilityEndpointV1,
    connected_socket: SocketAddr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedPlannerSelection {
    path_kind: RoutePathKindV1,
    carrier_identity: Option<NodeId>,
    direct: Option<VerifiedDirectSelectionV1>,
    relay: Option<VerifiedRelaySelectionV1>,
    hole_punch: Option<VerifiedHolePunchSelectionV1>,
    attempts: Vec<RouteAttemptV1>,
    connection_binding_digest: [u8; 32],
    selection_digest: [u8; 32],
}

impl VerifiedPlannerSelection {
    pub fn path_kind(&self) -> RoutePathKindV1 {
        self.path_kind
    }

    pub fn carrier_identity(&self) -> Option<NodeId> {
        self.carrier_identity
    }

    pub fn direct(&self) -> Option<&VerifiedDirectSelectionV1> {
        self.direct.as_ref()
    }

    pub fn relay(&self) -> Option<&VerifiedRelaySelectionV1> {
        self.relay.as_ref()
    }

    pub fn hole_punch(&self) -> Option<&VerifiedHolePunchSelectionV1> {
        self.hole_punch.as_ref()
    }

    pub fn attempts(&self) -> &[RouteAttemptV1] {
        &self.attempts
    }

    pub fn connection_binding_digest(&self) -> [u8; 32] {
        self.connection_binding_digest
    }

    pub fn selection_digest(&self) -> [u8; 32] {
        self.selection_digest
    }
}

impl VerifiedRelaySelectionV1 {
    pub fn relay_node_id(&self) -> NodeId {
        self.relay_node_id
    }

    pub fn association_id(&self) -> [u8; 32] {
        self.association_id
    }

    pub fn reservation_ids(&self) -> ([u8; 32], [u8; 32]) {
        (self.local_reservation_id, self.remote_reservation_id)
    }

    pub fn endpoint(&self) -> &RelayEndpointV1 {
        &self.endpoint
    }

    pub fn connected_socket(&self) -> SocketAddr {
        self.connected_socket
    }

    pub fn outer_connection_binding(&self) -> [u8; 32] {
        self.outer_connection_binding
    }

    pub fn transport(&self) -> RelayTransportV1 {
        self.transport
    }
}

impl VerifiedHolePunchSelectionV1 {
    pub fn relay_node_id(&self) -> NodeId {
        self.relay_node_id
    }

    pub fn reservation_ids(&self) -> ([u8; 32], [u8; 32]) {
        (self.local_reservation_id, self.remote_reservation_id)
    }

    pub fn schedule_digest(&self) -> [u8; 32] {
        self.schedule_digest
    }

    pub fn endpoint(&self) -> &ReachabilityEndpointV1 {
        &self.endpoint
    }

    pub fn connected_socket(&self) -> SocketAddr {
        self.connected_socket
    }
}

pub struct SelectedCarrier {
    pub(crate) connection: OBPConnection,
    pub(crate) selection: VerifiedPlannerSelection,
}

impl fmt::Debug for SelectedCarrier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SelectedCarrier")
            .field("selection", &self.selection)
            .finish_non_exhaustive()
    }
}

impl SelectedCarrier {
    /// Returns the socket measured by the concrete carrier adapter. Callers
    /// may use it for admission accounting, but it is not peer authority.
    pub fn connected_socket(&self) -> SocketAddr {
        match self.selection.path_kind {
            RoutePathKindV1::Direct => match self.selection.direct.as_ref() {
                Some(VerifiedDirectSelectionV1::OutboundCandidate {
                    connected_socket, ..
                })
                | Some(VerifiedDirectSelectionV1::InboundObserved { connected_socket }) => {
                    *connected_socket
                }
                None => unreachable!("sealed direct selection has direct provenance"),
            },
            RoutePathKindV1::HolePunched => {
                self.selection
                    .hole_punch
                    .as_ref()
                    .expect("sealed hole-punch selection has provenance")
                    .connected_socket
            }
            RoutePathKindV1::RelayUdp | RoutePathKindV1::RelayTcp443 => {
                self.selection
                    .relay
                    .as_ref()
                    .expect("sealed relay selection has provenance")
                    .connected_socket
            }
        }
    }

    pub fn selection(&self) -> &VerifiedPlannerSelection {
        &self.selection
    }
}

pub struct AuthenticatedRouteConnection {
    pub(crate) session: AuthenticatedSession,
    pub(crate) connection: OBPConnection,
    pub(crate) selection: VerifiedPlannerSelection,
    pub(crate) transport_binding_digest: [u8; 32],
    pub(crate) authenticated_peer: NodeId,
}

impl fmt::Debug for AuthenticatedRouteConnection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthenticatedRouteConnection")
            .field("session", &self.session)
            .field("selection", &self.selection)
            .field("authenticated_peer", &self.authenticated_peer)
            .finish_non_exhaustive()
    }
}

impl AuthenticatedRouteConnection {
    pub fn session(&self) -> &AuthenticatedSession {
        &self.session
    }

    pub fn selection(&self) -> &VerifiedPlannerSelection {
        &self.selection
    }

    pub fn authenticated_peer(&self) -> NodeId {
        self.authenticated_peer
    }

    pub fn transport_binding_digest(&self) -> [u8; 32] {
        self.transport_binding_digest
    }

    pub fn verified_session_source(
        &self,
    ) -> crate::vnext_relay_discovery::VerifiedAuthenticatedSessionSource {
        crate::vnext_relay_discovery::VerifiedAuthenticatedSessionSource::from_verified_route(
            self.authenticated_peer,
            self.session.session_id,
            self.transport_binding_digest,
            self.selection.selection_digest,
        )
    }

    pub fn into_parts(
        self,
    ) -> (
        AuthenticatedSession,
        OBPConnection,
        VerifiedPlannerSelection,
    ) {
        (self.session, self.connection, self.selection)
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedDirectDialCandidate {
    candidate: DirectCandidateV1,
    public_dial: Option<ValidatedPublicDialEndpoint>,
    authenticated_private_cache_binding: Option<[u8; 32]>,
}

impl ValidatedDirectDialCandidate {
    pub fn from_public(
        candidate: DirectCandidateV1,
        public_dial: ValidatedPublicDialEndpoint,
    ) -> Result<Self, RouteFailure> {
        if candidate.kind == DirectCandidateKindV1::Host
            || public_dial.transport() != ValidatedPublicDialTransportV1::DirectQuicUdp
            || candidate.endpoint.host != *public_dial.signed_host()
            || candidate.endpoint.port != public_dial.port()
            || candidate.expires_at > public_dial.expires_at()
        {
            return Err(RouteFailure::PeerIdentityMismatch);
        }
        Ok(Self {
            candidate,
            public_dial: Some(public_dial),
            authenticated_private_cache_binding: None,
        })
    }

    #[allow(dead_code)]
    pub(crate) fn from_authenticated_private_cache(
        candidate: DirectCandidateV1,
        binding: [u8; 32],
    ) -> Result<Self, RouteFailure> {
        if binding == [0; 32]
            || candidate.kind != DirectCandidateKindV1::Host
            || candidate.network_epoch == 0
        {
            return Err(RouteFailure::NetworkChanged);
        }
        Ok(Self {
            candidate,
            public_dial: None,
            authenticated_private_cache_binding: Some(binding),
        })
    }

    pub fn candidate(&self) -> &DirectCandidateV1 {
        &self.candidate
    }
}

pub struct ConnectedDirectCarrier {
    pub(crate) connection: OBPConnection,
    pub(crate) connected_socket: SocketAddr,
}

impl ConnectedDirectCarrier {
    fn measured(connection: OBPConnection) -> Self {
        let connected_socket = connection.remote_address();
        Self {
            connection,
            connected_socket,
        }
    }
}

pub trait DirectCarrierDialer: Send + Sync {
    fn dial<'a>(
        &'a self,
        candidate: &'a ValidatedDirectDialCandidate,
        deadline: Instant,
    ) -> ReachabilityFuture<'a, Result<ConnectedDirectCarrier, RouteFailure>>;
}

pub struct QuicDirectCarrierDialer {
    transport: Arc<QuicTransport>,
}

impl QuicDirectCarrierDialer {
    pub fn new(transport: Arc<QuicTransport>) -> Self {
        Self { transport }
    }
}

impl DirectCarrierDialer for QuicDirectCarrierDialer {
    fn dial<'a>(
        &'a self,
        candidate: &'a ValidatedDirectDialCandidate,
        deadline: Instant,
    ) -> ReachabilityFuture<'a, Result<ConnectedDirectCarrier, RouteFailure>> {
        Box::pin(async move {
            if Instant::now() >= deadline {
                return Err(RouteFailure::DirectTimeout);
            }
            let address = if let Some(public) = &candidate.public_dial {
                *public
                    .dial_addresses()
                    .first()
                    .ok_or(RouteFailure::CandidateExpired)?
            } else {
                let _binding = candidate
                    .authenticated_private_cache_binding
                    .ok_or(RouteFailure::PeerIdentityMismatch)?;
                endpoint_socket(&candidate.candidate.endpoint)
                    .ok_or(RouteFailure::CandidateExpired)?
            };
            let connection = self
                .transport
                .connect(address)
                .await
                .map_err(|_| RouteFailure::DirectTimeout)?;
            Ok(ConnectedDirectCarrier::measured(connection))
        })
    }
}

pub trait RelayCarrierDialer: Send + Sync {
    fn dial<'a>(
        &'a self,
        relay: &'a ValidatedRelayDescriptor,
        association: &'a ValidatedRelayAssociation,
        outer: Arc<AuthenticatedOuterRelayConnection>,
        deadline: Instant,
    ) -> ReachabilityFuture<'a, Result<OBPConnection, RouteFailure>>;
}

pub trait RelayAssociationClient: Send + Sync {
    fn associate<'a>(
        &'a self,
        request: &'a RelayConnectRequestV1,
        local: &'a ValidatedRelayReservation,
        remote: &'a ValidatedRelayReservation,
        outer: Arc<AuthenticatedOuterRelayConnection>,
        deadline: Instant,
    ) -> ReachabilityFuture<'a, Result<ValidatedRelayAssociation, RouteFailure>>;

    fn accept_inbound<'a>(
        &'a self,
        initiator_reservation: &'a ValidatedRelayReservation,
        target_reservation: &'a ValidatedRelayReservation,
        initiator: KnownPeerIdentity,
        outer: Arc<AuthenticatedOuterRelayConnection>,
        deadline: Instant,
    ) -> ReachabilityFuture<'a, Result<ValidatedRelayAssociation, RouteFailure>>;
}

/// Production association client for an already authenticated outer relay
/// connection. Both the caller request and relay response are re-admitted
/// through the canonical connectivity validator; the relay wire response is
/// never accepted as authority by itself.
pub struct ProductionRelayAssociationClient {
    local_public_key: [u8; 32],
    validator: ConnectivitySignalingValidator,
}

impl ProductionRelayAssociationClient {
    pub fn new(local_public_key: [u8; 32]) -> Self {
        Self {
            local_public_key,
            validator: ConnectivitySignalingValidator::new(Arc::new(
                InMemoryReachabilityReplayStore::default(),
            )),
        }
    }
}

impl RelayAssociationClient for ProductionRelayAssociationClient {
    fn associate<'a>(
        &'a self,
        request: &'a RelayConnectRequestV1,
        local: &'a ValidatedRelayReservation,
        remote: &'a ValidatedRelayReservation,
        outer: Arc<AuthenticatedOuterRelayConnection>,
        deadline: Instant,
    ) -> ReachabilityFuture<'a, Result<ValidatedRelayAssociation, RouteFailure>> {
        Box::pin(async move {
            if Instant::now() >= deadline {
                return Err(RouteFailure::RelayPathFailed(
                    "association preflight: deadline elapsed".into(),
                ));
            }
            if !outer.is_open() {
                return Err(RouteFailure::RelayPathFailed(
                    "association preflight: outer relay closed".into(),
                ));
            }
            if outer.client_node_id() != request.initiator_node_id {
                return Err(RouteFailure::RelayPathFailed(
                    "association preflight: outer client identity mismatch".into(),
                ));
            }
            if local.canonical().target_node_id != request.initiator_node_id {
                return Err(RouteFailure::RelayPathFailed(
                    "association preflight: local reservation identity mismatch".into(),
                ));
            }
            if remote.canonical().target_node_id != request.target_node_id {
                return Err(RouteFailure::RelayPathFailed(
                    "association preflight: remote reservation identity mismatch".into(),
                ));
            }
            if local.canonical().relay_node_id != outer.relay_node_id() {
                return Err(RouteFailure::RelayPathFailed(
                    "association preflight: local reservation relay mismatch".into(),
                ));
            }
            if remote.canonical().relay_node_id != outer.relay_node_id() {
                return Err(RouteFailure::RelayPathFailed(
                    "association preflight: remote reservation relay mismatch".into(),
                ));
            }
            let request_root = ConnectivitySignalingV1::RelayConnectRequest(request.clone());
            let request_bytes = encode_connectivity_signaling(&request_root).map_err(|error| {
                RouteFailure::RelayPathFailed(format!("association request encode: {error:?}"))
            })?;
            let initiator = KnownPeerIdentity::from_public_key(self.local_public_key);
            let admitted_request = self
                .validator
                .validate_connect_request(
                    &request_bytes,
                    &initiator,
                    request.target_node_id,
                    local,
                    remote,
                    unix_now_seconds().map_err(|error| {
                        RouteFailure::RelayPathFailed(format!(
                            "association request clock: {error:?}"
                        ))
                    })?,
                )
                .map_err(|error| {
                    RouteFailure::RelayPathFailed(format!(
                        "association request validation: {error:?}"
                    ))
                })?;
            let request_id = random_request_id();
            let frame =
                RelayWireFrameV1::new(RelayWireKindV1::ConnectRequest, request_id, request_bytes)
                    .map_err(|_| RouteFailure::RelayDenied)?;
            let response = tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                outer.request_control_frame(&frame),
            )
            .await
            .map_err(|_| {
                RouteFailure::RelayPathFailed("association response: deadline elapsed".into())
            })?
            .map_err(|error| {
                RouteFailure::RelayPathFailed(format!("association response I/O: {error:?}"))
            })?;
            if response.kind() != RelayWireKindV1::Association
                || response.request_id() != request_id
            {
                return Err(RouteFailure::RelayPathFailed(
                    "association response: kind or request-id mismatch".into(),
                ));
            }
            let root = decode_connectivity_signaling(response.payload()).map_err(|error| {
                RouteFailure::RelayPathFailed(format!("association response decode: {error:?}"))
            })?;
            if !matches!(root, ConnectivitySignalingV1::RelayAssociation(_)) {
                return Err(RouteFailure::RelayPathFailed(
                    "association response: non-association payload".into(),
                ));
            }
            let descriptor = outer.route().descriptor().canonical();
            let relay = KnownPeerIdentity {
                node_id: descriptor.relay_node_id,
                public_key: descriptor.relay_public_key,
            };
            self.validator
                .validate_association(
                    response.payload(),
                    &relay,
                    &admitted_request,
                    local,
                    remote,
                    unix_now_seconds().map_err(|error| {
                        RouteFailure::RelayPathFailed(format!(
                            "association response clock: {error:?}"
                        ))
                    })?,
                )
                .map_err(|error| {
                    RouteFailure::RelayPathFailed(format!(
                        "association response validation: {error:?}"
                    ))
                })
        })
    }

    fn accept_inbound<'a>(
        &'a self,
        initiator_reservation: &'a ValidatedRelayReservation,
        target_reservation: &'a ValidatedRelayReservation,
        initiator: KnownPeerIdentity,
        outer: Arc<AuthenticatedOuterRelayConnection>,
        deadline: Instant,
    ) -> ReachabilityFuture<'a, Result<ValidatedRelayAssociation, RouteFailure>> {
        Box::pin(async move {
            if Instant::now() >= deadline {
                return Err(RouteFailure::RelayPathFailed(
                    "inbound association preflight: deadline elapsed".into(),
                ));
            }
            if !outer.is_open() {
                return Err(RouteFailure::RelayPathFailed(
                    "inbound association preflight: outer relay closed".into(),
                ));
            }
            if outer.client_node_id() != target_reservation.canonical().target_node_id {
                return Err(RouteFailure::RelayPathFailed(
                    "inbound association preflight: outer client identity mismatch".into(),
                ));
            }
            if initiator.node_id != initiator_reservation.canonical().target_node_id {
                return Err(RouteFailure::RelayPathFailed(
                    "inbound association preflight: initiator reservation identity mismatch".into(),
                ));
            }
            if initiator_reservation.canonical().relay_node_id != outer.relay_node_id() {
                return Err(RouteFailure::RelayPathFailed(
                    "inbound association preflight: initiator reservation relay mismatch".into(),
                ));
            }
            if target_reservation.canonical().relay_node_id != outer.relay_node_id() {
                return Err(RouteFailure::RelayPathFailed(
                    "inbound association preflight: target reservation relay mismatch".into(),
                ));
            }
            let request_frame = tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                outer.receive_control_frame(),
            )
            .await
            .map_err(|_| {
                RouteFailure::RelayPathFailed("inbound connect request: deadline elapsed".into())
            })?
            .map_err(|error| {
                RouteFailure::RelayPathFailed(format!("inbound connect request I/O: {error:?}"))
            })?;
            if request_frame.kind() != RelayWireKindV1::ConnectRequest {
                return Err(RouteFailure::RelayPathFailed(format!(
                    "inbound connect request: unexpected frame kind {:?}",
                    request_frame.kind()
                )));
            }
            let request_root =
                decode_connectivity_signaling(request_frame.payload()).map_err(|error| {
                    RouteFailure::RelayPathFailed(format!(
                        "inbound connect request decode: {error:?}"
                    ))
                })?;
            let ConnectivitySignalingV1::RelayConnectRequest(_request) = request_root else {
                return Err(RouteFailure::RelayPathFailed(
                    "inbound connect request: non-connect payload".into(),
                ));
            };
            let admitted_request = self
                .validator
                .validate_connect_request(
                    request_frame.payload(),
                    &initiator,
                    target_reservation.canonical().target_node_id,
                    initiator_reservation,
                    target_reservation,
                    unix_now_seconds().map_err(|error| {
                        RouteFailure::RelayPathFailed(format!(
                            "inbound connect request clock: {error:?}"
                        ))
                    })?,
                )
                .map_err(|error| {
                    RouteFailure::RelayPathFailed(format!(
                        "inbound connect request validation: {error:?}"
                    ))
                })?;
            let association_frame = tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                outer.receive_control_frame(),
            )
            .await
            .map_err(|_| {
                RouteFailure::RelayPathFailed(
                    "inbound association response: deadline elapsed".into(),
                )
            })?
            .map_err(|error| {
                RouteFailure::RelayPathFailed(format!(
                    "inbound association response I/O: {error:?}"
                ))
            })?;
            if association_frame.kind() != RelayWireKindV1::Association
                || association_frame.request_id() != request_frame.request_id()
            {
                return Err(RouteFailure::RelayPathFailed(
                    "inbound association response: kind or request-id mismatch".into(),
                ));
            }
            let descriptor = outer.route().descriptor().canonical();
            let relay = KnownPeerIdentity {
                node_id: descriptor.relay_node_id,
                public_key: descriptor.relay_public_key,
            };
            self.validator
                .validate_association(
                    association_frame.payload(),
                    &relay,
                    &admitted_request,
                    initiator_reservation,
                    target_reservation,
                    unix_now_seconds().map_err(|error| {
                        RouteFailure::RelayPathFailed(format!(
                            "inbound association response clock: {error:?}"
                        ))
                    })?,
                )
                .map_err(|error| {
                    RouteFailure::RelayPathFailed(format!(
                        "inbound association response validation: {error:?}"
                    ))
                })
        })
    }
}

/// Production inner-QUIC carrier over one admitted relay association. The
/// association initiator opens the inner QUIC connection and the target
/// accepts it, avoiding a second, ambiguous connection race.
pub struct ProductionRelayCarrierDialer {
    global_budget: RelaySocketGlobalBudget,
}

impl ProductionRelayCarrierDialer {
    pub fn standard() -> Self {
        Self {
            global_budget: RelaySocketGlobalBudget::standard(),
        }
    }
}

impl Default for ProductionRelayCarrierDialer {
    fn default() -> Self {
        Self::standard()
    }
}

impl RelayCarrierDialer for ProductionRelayCarrierDialer {
    fn dial<'a>(
        &'a self,
        relay: &'a ValidatedRelayDescriptor,
        association: &'a ValidatedRelayAssociation,
        outer: Arc<AuthenticatedOuterRelayConnection>,
        deadline: Instant,
    ) -> ReachabilityFuture<'a, Result<OBPConnection, RouteFailure>> {
        Box::pin(async move {
            let value = association.canonical();
            if Instant::now() >= deadline
                || !outer.is_open()
                || value.relay_node_id != relay.canonical().relay_node_id
                || value.relay_node_id != outer.relay_node_id()
                || (outer.client_node_id() != value.initiator_node_id
                    && outer.client_node_id() != value.target_node_id)
            {
                return Err(RouteFailure::PeerIdentityMismatch);
            }
            let initiator = outer.client_node_id() == value.initiator_node_id;
            let local_addr: SocketAddr = if initiator {
                "127.0.0.1:41011".parse().expect("fixed relay socket")
            } else {
                "127.0.0.1:41012".parse().expect("fixed relay socket")
            };
            let peer_addr: SocketAddr = if initiator {
                "127.0.0.1:41012".parse().expect("fixed relay socket")
            } else {
                "127.0.0.1:41011".parse().expect("fixed relay socket")
            };
            let (socket, driver) =
                RelayDatagramSocket::pair(local_addr, self.global_budget.clone());
            let transport = QuicTransport::bind_abstract(TransportConfig::default(), socket)
                .map_err(|_| RouteFailure::RelayDenied)?;
            spawn_relay_socket_pump(
                driver,
                Arc::clone(&outer),
                value.association_id,
                initiator,
                peer_addr,
            );
            let operation = async {
                if initiator {
                    transport.connect(peer_addr).await
                } else {
                    transport.accept().await
                }
            };
            tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), operation)
                .await
                .map_err(|_| RouteFailure::RelayUnavailable)?
                .map_err(|_| RouteFailure::RelayDenied)
        })
    }
}

pub struct AdmittedDirectExecution {
    candidate: ValidatedDirectDialCandidate,
}

impl AdmittedDirectExecution {
    pub fn from_public(
        candidate: DirectCandidateV1,
        public_dial: ValidatedPublicDialEndpoint,
    ) -> Result<Self, RouteFailure> {
        Ok(Self {
            candidate: ValidatedDirectDialCandidate::from_public(candidate, public_dial)?,
        })
    }
}

pub struct AdmittedHolePunchExecution {
    punched: ValidatedPunchedCarrier,
}

impl AdmittedHolePunchExecution {
    pub fn from_validated(punched: ValidatedPunchedCarrier) -> Self {
        Self { punched }
    }
}

pub struct AdmittedRelayExecution {
    descriptor: ValidatedRelayDescriptor,
    local: ValidatedRelayReservation,
    remote: ValidatedRelayReservation,
    request: Option<RelayConnectRequestV1>,
    association: Option<ValidatedRelayAssociation>,
    outer: Arc<AuthenticatedOuterRelayConnection>,
}

impl AdmittedRelayExecution {
    pub fn from_validated_association(
        descriptor: ValidatedRelayDescriptor,
        local: ValidatedRelayReservation,
        remote: ValidatedRelayReservation,
        association: ValidatedRelayAssociation,
        outer: Arc<AuthenticatedOuterRelayConnection>,
    ) -> Result<Self, RouteFailure> {
        validate_relay_parts(&descriptor, &local, &remote, &association, &outer)?;
        Ok(Self {
            descriptor,
            local,
            remote,
            request: None,
            association: Some(association),
            outer,
        })
    }
}

pub enum AdmittedExecutionInput {
    Direct(AdmittedDirectExecution),
    HolePunch(AdmittedHolePunchExecution),
    Relay(AdmittedRelayExecution),
}

pub struct UnboundDirectInboundCarrier {
    pub(crate) carrier: ConnectedDirectCarrier,
    pub(crate) selection: VerifiedPlannerSelection,
}

pub struct ExpectedInboundCarrier {
    pub(crate) expected_peer: NodeId,
    pub(crate) connection: OBPConnection,
    pub(crate) selection: VerifiedPlannerSelection,
}

impl ExpectedInboundCarrier {
    pub fn connected_socket(&self) -> SocketAddr {
        self.connection.remote_addr()
    }

    pub fn expected_peer(&self) -> NodeId {
        self.expected_peer
    }
}

pub enum AdmittedInboundCarrier {
    UnboundDirect(UnboundDirectInboundCarrier),
    Expected(ExpectedInboundCarrier),
}

pub trait InboundCarrierAcceptor: Send + Sync {
    fn accept(
        &self,
        deadline: Instant,
    ) -> ReachabilityFuture<'_, Result<AdmittedInboundCarrier, RouteFailure>>;
}

pub struct ConnectionPlannerExecutor {
    direct: Arc<dyn DirectCarrierDialer>,
    relay: Arc<dyn RelayCarrierDialer>,
    association: Arc<dyn RelayAssociationClient>,
}

impl ConnectionPlannerExecutor {
    pub fn new(
        direct: Arc<dyn DirectCarrierDialer>,
        relay: Arc<dyn RelayCarrierDialer>,
        association: Arc<dyn RelayAssociationClient>,
    ) -> Self {
        Self {
            direct,
            relay,
            association,
        }
    }

    pub async fn associate_relay(
        &self,
        request: &RelayConnectRequestV1,
        local: &ValidatedRelayReservation,
        remote: &ValidatedRelayReservation,
        outer: Arc<AuthenticatedOuterRelayConnection>,
        deadline: Instant,
    ) -> Result<ValidatedRelayAssociation, RouteFailure> {
        self.association
            .associate(request, local, remote, outer, deadline)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn accept_relay_inbound(
        &self,
        descriptor: ValidatedRelayDescriptor,
        initiator_reservation: ValidatedRelayReservation,
        target_reservation: ValidatedRelayReservation,
        initiator: KnownPeerIdentity,
        candidate: onebrain_protocol::RelayCandidateV1,
        outer: Arc<AuthenticatedOuterRelayConnection>,
        deadline: Instant,
    ) -> Result<ExpectedInboundCarrier, RouteFailure> {
        let association = self
            .association
            .accept_inbound(
                &initiator_reservation,
                &target_reservation,
                initiator,
                Arc::clone(&outer),
                deadline,
            )
            .await
            .map_err(|error| {
                RouteFailure::RelayPathFailed(format!("association acceptance: {error:?}"))
            })?;
        validate_relay_target_parts(
            &descriptor,
            &initiator_reservation,
            &target_reservation,
            &association,
            &outer,
        )
        .map_err(|error| {
            RouteFailure::RelayPathFailed(format!("association binding: {error:?}"))
        })?;
        let connection = self
            .relay
            .dial(&descriptor, &association, Arc::clone(&outer), deadline)
            .await
            .map_err(|error| RouteFailure::RelayPathFailed(format!("inner carrier: {error:?}")))?;
        let expected_peer = association.canonical().initiator_node_id;
        let selected =
            Self::seal_validated_relay(candidate, association, &outer, connection, Vec::new())?;
        Self::expect_inbound(selected, expected_peer)
    }

    pub fn admitted_relay_action(
        candidate: onebrain_protocol::RelayCandidateV1,
        association: &ValidatedRelayAssociation,
    ) -> Result<PlannerAction, RouteFailure> {
        let admitted = crate::vnext_route_plan::AdmittedRelayPath::from_validated_association(
            candidate,
            association,
        )
        .map_err(|_| RouteFailure::RelayDenied)?;
        Ok(PlannerAction::ConnectRelay(admitted))
    }

    pub fn seal_validated_relay(
        candidate: onebrain_protocol::RelayCandidateV1,
        association: ValidatedRelayAssociation,
        outer: &AuthenticatedOuterRelayConnection,
        connection: OBPConnection,
        attempts: Vec<RouteAttemptV1>,
    ) -> Result<SelectedCarrier, RouteFailure> {
        let action = Self::admitted_relay_action(candidate, &association)?;
        Self::seal_relay(action, association, outer, connection, attempts)
    }

    pub async fn execute(
        &self,
        action: PlannerAction,
        input: AdmittedExecutionInput,
        attempts: Vec<RouteAttemptV1>,
        deadline: Instant,
    ) -> Result<SelectedCarrier, RouteFailure> {
        match (action, input) {
            (action @ PlannerAction::CheckDirect(_), AdmittedExecutionInput::Direct(input)) => {
                let PlannerAction::CheckDirect(candidate) = &action else {
                    unreachable!();
                };
                if candidate != input.candidate.candidate() {
                    return Err(RouteFailure::PeerIdentityMismatch);
                }
                let connected = self.direct.dial(&input.candidate, deadline).await?;
                Self::seal_measured_direct(action, connected, attempts)
            }
            (
                action @ PlannerAction::CoordinateHolePunch(_),
                AdmittedExecutionInput::HolePunch(input),
            ) => Self::seal_hole_punched(action, input.punched, attempts),
            (action @ PlannerAction::ConnectRelay(_), AdmittedExecutionInput::Relay(input)) => {
                let association = match input.association {
                    Some(value) => value,
                    None => {
                        let request = input.request.as_ref().ok_or(RouteFailure::RelayDenied)?;
                        self.association
                            .associate(
                                request,
                                &input.local,
                                &input.remote,
                                Arc::clone(&input.outer),
                                deadline,
                            )
                            .await?
                    }
                };
                validate_relay_parts(
                    &input.descriptor,
                    &input.local,
                    &input.remote,
                    &association,
                    &input.outer,
                )?;
                let connection = self
                    .relay
                    .dial(
                        &input.descriptor,
                        &association,
                        Arc::clone(&input.outer),
                        deadline,
                    )
                    .await?;
                Self::seal_relay(action, association, &input.outer, connection, attempts)
            }
            _ => Err(RouteFailure::PeerIdentityMismatch),
        }
    }

    pub fn seal_connected_direct(
        action: PlannerAction,
        connection: OBPConnection,
        attempts: Vec<RouteAttemptV1>,
    ) -> Result<SelectedCarrier, RouteFailure> {
        Self::seal_measured_direct(
            action,
            ConnectedDirectCarrier::measured(connection),
            attempts,
        )
    }

    fn seal_measured_direct(
        action: PlannerAction,
        carrier: ConnectedDirectCarrier,
        attempts: Vec<RouteAttemptV1>,
    ) -> Result<SelectedCarrier, RouteFailure> {
        validate_attempts(&attempts)?;
        let PlannerAction::CheckDirect(candidate) = action else {
            return Err(RouteFailure::PeerIdentityMismatch);
        };
        if candidate.network_epoch == 0
            || !endpoint_matches_socket(&candidate.endpoint, carrier.connected_socket)
        {
            return Err(RouteFailure::PeerIdentityMismatch);
        }
        let binding = carrier
            .connection
            .transport_binding()
            .map_err(|_| RouteFailure::PeerIdentityMismatch)?;
        let direct = VerifiedDirectSelectionV1::OutboundCandidate {
            endpoint: candidate.endpoint,
            connected_socket: carrier.connected_socket,
            candidate_kind: candidate.kind,
            network_epoch: candidate.network_epoch,
        };
        let selection = build_selection(
            RoutePathKindV1::Direct,
            None,
            Some(direct),
            None,
            None,
            attempts,
            binding,
        );
        Ok(SelectedCarrier {
            connection: carrier.connection,
            selection,
        })
    }

    pub fn seal_unbound_direct_inbound(
        connection: OBPConnection,
        attempts: Vec<RouteAttemptV1>,
    ) -> Result<UnboundDirectInboundCarrier, RouteFailure> {
        validate_attempts(&attempts)?;
        let carrier = ConnectedDirectCarrier::measured(connection);
        let binding = carrier
            .connection
            .transport_binding()
            .map_err(|_| RouteFailure::PeerIdentityMismatch)?;
        let selection = build_selection(
            RoutePathKindV1::Direct,
            None,
            Some(VerifiedDirectSelectionV1::InboundObserved {
                connected_socket: carrier.connected_socket,
            }),
            None,
            None,
            attempts,
            binding,
        );
        Ok(UnboundDirectInboundCarrier { carrier, selection })
    }

    pub fn expect_inbound(
        carrier: SelectedCarrier,
        expected_peer: NodeId,
    ) -> Result<ExpectedInboundCarrier, RouteFailure> {
        verify_selection_integrity(&carrier.connection, &carrier.selection)?;
        Ok(ExpectedInboundCarrier {
            expected_peer,
            connection: carrier.connection,
            selection: carrier.selection,
        })
    }

    pub fn seal_hole_punched(
        action: PlannerAction,
        punched: ValidatedPunchedCarrier,
        attempts: Vec<RouteAttemptV1>,
    ) -> Result<SelectedCarrier, RouteFailure> {
        validate_attempts(&attempts)?;
        let PlannerAction::CoordinateHolePunch(admitted) = action else {
            return Err(RouteFailure::PeerIdentityMismatch);
        };
        let candidate = admitted.candidate();
        let reservation_ids = punched.reservation_ids();
        if admitted.schedule_digest() != punched.schedule_digest()
            || candidate.relay_node_id != punched.relay_node_id()
            || candidate.local_reservation_id != reservation_ids.0
            || candidate.remote_reservation_id != reservation_ids.1
        {
            return Err(RouteFailure::PeerIdentityMismatch);
        }
        let binding_digest = punched.transport_binding_digest();
        let endpoint = punched.connected_endpoint().clone();
        let connected_socket = punched.connected_socket();
        let connection = punched.into_connection();
        let binding = connection
            .transport_binding()
            .map_err(|_| RouteFailure::PeerIdentityMismatch)?;
        if *blake3::hash(&binding).as_bytes() != binding_digest {
            return Err(RouteFailure::PeerIdentityMismatch);
        }
        let hole = VerifiedHolePunchSelectionV1 {
            relay_node_id: candidate.relay_node_id,
            local_reservation_id: reservation_ids.0,
            remote_reservation_id: reservation_ids.1,
            schedule_digest: admitted.schedule_digest(),
            endpoint,
            connected_socket,
        };
        let selection = build_selection(
            RoutePathKindV1::HolePunched,
            Some(candidate.relay_node_id),
            None,
            None,
            Some(hole),
            attempts,
            binding,
        );
        Ok(SelectedCarrier {
            connection,
            selection,
        })
    }

    pub fn seal_relay(
        action: PlannerAction,
        association: ValidatedRelayAssociation,
        outer: &AuthenticatedOuterRelayConnection,
        connection: OBPConnection,
        attempts: Vec<RouteAttemptV1>,
    ) -> Result<SelectedCarrier, RouteFailure> {
        validate_attempts(&attempts)?;
        let PlannerAction::ConnectRelay(admitted) = action else {
            return Err(RouteFailure::PeerIdentityMismatch);
        };
        let canonical = association.canonical();
        let (local_reservation_id, remote_reservation_id) = admitted.reservation_ids();
        if !outer.is_open()
            || admitted.candidate().relay_node_id != outer.relay_node_id()
            || admitted.candidate().transport != outer.transport()
            || admitted.candidate().endpoint.host != outer.public_endpoint().host
            || admitted.candidate().endpoint.port != outer.public_endpoint().port
            || admitted.association_id() != canonical.association_id
            || local_reservation_id != canonical.initiator_reservation_id
            || remote_reservation_id != canonical.target_reservation_id
        {
            return Err(RouteFailure::PeerIdentityMismatch);
        }
        let path = match outer.transport() {
            RelayTransportV1::QuicUdp => RoutePathKindV1::RelayUdp,
            RelayTransportV1::TlsTcp443 => RoutePathKindV1::RelayTcp443,
        };
        let binding = connection
            .transport_binding()
            .map_err(|_| RouteFailure::PeerIdentityMismatch)?;
        let relay = VerifiedRelaySelectionV1 {
            relay_node_id: outer.relay_node_id(),
            association_id: canonical.association_id,
            local_reservation_id,
            remote_reservation_id,
            endpoint: RelayEndpointV1 {
                transport: outer.transport(),
                host: admitted.candidate().endpoint.host.clone(),
                port: admitted.candidate().endpoint.port,
            },
            connected_socket: outer.connected_socket(),
            outer_connection_binding: outer.connection_binding(),
            transport: outer.transport(),
        };
        let selection = build_selection(
            path,
            Some(outer.relay_node_id()),
            None,
            Some(relay),
            None,
            attempts,
            binding,
        );
        Ok(SelectedCarrier {
            connection,
            selection,
        })
    }
}

fn validate_relay_parts(
    descriptor: &ValidatedRelayDescriptor,
    local: &ValidatedRelayReservation,
    remote: &ValidatedRelayReservation,
    association: &ValidatedRelayAssociation,
    outer: &AuthenticatedOuterRelayConnection,
) -> Result<(), RouteFailure> {
    let relay = descriptor.canonical().relay_node_id;
    let local = local.canonical();
    let remote = remote.canonical();
    let association = association.canonical();
    if !outer.is_open()
        || outer.relay_node_id() != relay
        || outer.route().descriptor().digest() != descriptor.digest()
        || local.relay_node_id != relay
        || remote.relay_node_id != relay
        || outer.client_node_id() != local.target_node_id
        || !local.transport_scope.contains(&outer.transport())
        || !remote.transport_scope.contains(&outer.transport())
        || local.reservation_id != association.initiator_reservation_id
        || remote.reservation_id != association.target_reservation_id
        || local.target_node_id != association.initiator_node_id
        || remote.target_node_id != association.target_node_id
        || association.relay_node_id != relay
    {
        return Err(RouteFailure::PeerIdentityMismatch);
    }
    Ok(())
}

fn validate_relay_target_parts(
    descriptor: &ValidatedRelayDescriptor,
    initiator: &ValidatedRelayReservation,
    target: &ValidatedRelayReservation,
    association: &ValidatedRelayAssociation,
    outer: &AuthenticatedOuterRelayConnection,
) -> Result<(), RouteFailure> {
    let relay = descriptor.canonical().relay_node_id;
    let initiator = initiator.canonical();
    let target = target.canonical();
    let association = association.canonical();
    if !outer.is_open()
        || outer.relay_node_id() != relay
        || outer.route().descriptor().digest() != descriptor.digest()
        || initiator.relay_node_id != relay
        || target.relay_node_id != relay
        || outer.client_node_id() != target.target_node_id
        || !initiator.transport_scope.contains(&outer.transport())
        || !target.transport_scope.contains(&outer.transport())
        || initiator.reservation_id != association.initiator_reservation_id
        || target.reservation_id != association.target_reservation_id
        || initiator.target_node_id != association.initiator_node_id
        || target.target_node_id != association.target_node_id
        || association.relay_node_id != relay
    {
        return Err(RouteFailure::PeerIdentityMismatch);
    }
    Ok(())
}

fn build_selection(
    path_kind: RoutePathKindV1,
    carrier_identity: Option<NodeId>,
    direct: Option<VerifiedDirectSelectionV1>,
    relay: Option<VerifiedRelaySelectionV1>,
    hole_punch: Option<VerifiedHolePunchSelectionV1>,
    attempts: Vec<RouteAttemptV1>,
    connection_binding: [u8; 32],
) -> VerifiedPlannerSelection {
    let connection_binding_digest = *blake3::hash(&connection_binding).as_bytes();
    let mut selection = VerifiedPlannerSelection {
        path_kind,
        carrier_identity,
        direct,
        relay,
        hole_punch,
        attempts,
        connection_binding_digest,
        selection_digest: [0; 32],
    };
    selection.selection_digest = selection_digest(&selection);
    selection
}

pub(crate) fn verify_selection_integrity(
    connection: &OBPConnection,
    selection: &VerifiedPlannerSelection,
) -> Result<(), RouteFailure> {
    let binding = connection
        .transport_binding()
        .map_err(|_| RouteFailure::PeerIdentityMismatch)?;
    if *blake3::hash(&binding).as_bytes() != selection.connection_binding_digest
        || selection_digest(selection) != selection.selection_digest
    {
        return Err(RouteFailure::PeerIdentityMismatch);
    }
    let shape_valid = match selection.path_kind {
        RoutePathKindV1::Direct => {
            selection.carrier_identity.is_none()
                && selection.direct.is_some()
                && selection.relay.is_none()
                && selection.hole_punch.is_none()
        }
        RoutePathKindV1::HolePunched => {
            selection.carrier_identity.is_some()
                && selection.direct.is_none()
                && selection.relay.is_none()
                && selection.hole_punch.is_some()
        }
        RoutePathKindV1::RelayUdp | RoutePathKindV1::RelayTcp443 => {
            selection.carrier_identity.is_some()
                && selection.direct.is_none()
                && selection.relay.is_some()
                && selection.hole_punch.is_none()
        }
    };
    if !shape_valid {
        return Err(RouteFailure::PeerIdentityMismatch);
    }
    Ok(())
}

fn selection_digest(selection: &VerifiedPlannerSelection) -> [u8; 32] {
    let mut hash = blake3::Hasher::new();
    hash.update(b"onebrain/verified-planner-selection/v1\0");
    hash.update(&[path_tag(selection.path_kind)]);
    match selection.carrier_identity {
        Some(value) => {
            hash.update(&[1]);
            hash.update(value.as_bytes());
        }
        None => {
            hash.update(&[0]);
        }
    }
    hash.update(&selection.connection_binding_digest);
    if let Some(direct) = &selection.direct {
        hash.update(&[1]);
        match direct {
            VerifiedDirectSelectionV1::OutboundCandidate {
                endpoint,
                connected_socket,
                candidate_kind,
                network_epoch,
            } => {
                hash.update(&[1, direct_kind_tag(*candidate_kind)]);
                hash_endpoint(&mut hash, endpoint);
                hash_socket(&mut hash, *connected_socket);
                hash.update(&network_epoch.to_be_bytes());
            }
            VerifiedDirectSelectionV1::InboundObserved { connected_socket } => {
                hash.update(&[2]);
                hash_socket(&mut hash, *connected_socket);
            }
        }
    } else {
        hash.update(&[0]);
    }
    if let Some(relay) = &selection.relay {
        hash.update(&[1]);
        hash.update(relay.relay_node_id.as_bytes());
        hash.update(&relay.association_id);
        hash.update(&relay.local_reservation_id);
        hash.update(&relay.remote_reservation_id);
        hash_relay_endpoint(&mut hash, &relay.endpoint);
        hash_socket(&mut hash, relay.connected_socket);
        hash.update(&relay.outer_connection_binding);
        hash.update(&[transport_tag(relay.transport)]);
    } else {
        hash.update(&[0]);
    }
    if let Some(hole) = &selection.hole_punch {
        hash.update(&[1]);
        hash.update(hole.relay_node_id.as_bytes());
        hash.update(&hole.local_reservation_id);
        hash.update(&hole.remote_reservation_id);
        hash.update(&hole.schedule_digest);
        hash_endpoint(&mut hash, &hole.endpoint);
        hash_socket(&mut hash, hole.connected_socket);
    } else {
        hash.update(&[0]);
    }
    hash.update(&(selection.attempts.len() as u64).to_be_bytes());
    for attempt in &selection.attempts {
        hash.update(&[path_tag(attempt.path_kind)]);
        if let Some(carrier) = attempt.carrier_identity {
            hash.update(&[1]);
            hash.update(carrier.as_bytes());
        } else {
            hash.update(&[0]);
        }
        hash.update(&attempt.started_at.to_be_bytes());
        hash.update(&attempt.finished_at.to_be_bytes());
        match attempt.outcome {
            RouteAttemptOutcomeV1::Connected => {
                hash.update(&[0]);
            }
            RouteAttemptOutcomeV1::Failed(code) => {
                hash.update(&[1, code as u8]);
            }
        }
    }
    *hash.finalize().as_bytes()
}

fn validate_attempts(attempts: &[RouteAttemptV1]) -> Result<(), RouteFailure> {
    if attempts.len() > MAX_ROUTE_PLAN_ATTEMPTS
        || attempts
            .iter()
            .any(|attempt| attempt.finished_at < attempt.started_at)
    {
        Err(RouteFailure::BudgetExceeded)
    } else {
        Ok(())
    }
}

const OPAQUE_RELAY_MAGIC: [u8; 4] = *b"OBPR";
const OPAQUE_RELAY_VERSION: u8 = 1;
const OPAQUE_RELAY_HEADER_BYTES: usize = 60;
const OPAQUE_RELAY_FRAGMENT_BYTES: usize = 1_024;
const OPAQUE_RELAY_MAX_INNER_BYTES: usize = 1_350;

fn spawn_relay_socket_pump(
    mut driver: RelaySocketDriver,
    outer: Arc<AuthenticatedOuterRelayConnection>,
    association_id: [u8; 32],
    initiator: bool,
    peer_addr: SocketAddr,
) {
    tokio::spawn(async move {
        let mut sequence = random_nonzero_u64();
        let mut message_id = random_nonzero_u64();
        loop {
            tokio::select! {
                outbound = driver.recv_outbound() => {
                    let Some(outbound) = outbound else { break; };
                    if outbound.contents.is_empty()
                        || outbound.contents.len() > OPAQUE_RELAY_MAX_INNER_BYTES
                    {
                        driver.fail(std::io::ErrorKind::InvalidData);
                        break;
                    }
                    let fragments = encode_relay_fragments(
                        association_id,
                        initiator,
                        sequence,
                        message_id,
                        &outbound.contents,
                    );
                    let Ok(fragments) = fragments else {
                        driver.fail(std::io::ErrorKind::InvalidData);
                        break;
                    };
                    for fragment in fragments {
                        if send_outer_opaque(&outer, fragment).await.is_err() {
                            driver.fail(std::io::ErrorKind::BrokenPipe);
                            return;
                        }
                    }
                    let Some(next_sequence) = sequence.checked_add(1) else {
                        driver.fail(std::io::ErrorKind::InvalidData);
                        break;
                    };
                    let Some(next_message) = message_id.checked_add(1) else {
                        driver.fail(std::io::ErrorKind::InvalidData);
                        break;
                    };
                    sequence = next_sequence;
                    message_id = next_message;
                }
                inbound = receive_outer_opaque(&outer, association_id) => {
                    match inbound {
                        Ok(contents) if !contents.is_empty()
                            && contents.len() <= OPAQUE_RELAY_MAX_INNER_BYTES => {
                                if driver.push_inbound(RelayInboundDatagram {
                                    source: peer_addr,
                                    destination_ip: None,
                                    ecn: None,
                                    contents,
                                }).is_err() {
                                    driver.fail(std::io::ErrorKind::WouldBlock);
                                    break;
                                }
                            }
                        _ => {
                            driver.fail(std::io::ErrorKind::BrokenPipe);
                            break;
                        }
                    }
                }
            }
        }
    });
}

async fn send_outer_opaque(
    outer: &AuthenticatedOuterRelayConnection,
    payload: Vec<u8>,
) -> Result<(), ()> {
    match outer.transport() {
        RelayTransportV1::QuicUdp => outer.send_opaque_datagram(payload).await.map_err(|_| ()),
        RelayTransportV1::TlsTcp443 => {
            let frame = RelayWireFrameV1::new(
                RelayWireKindV1::OpaqueDatagram,
                random_request_id(),
                payload,
            )
            .map_err(|_| ())?;
            outer.send_control_frame(&frame).await.map_err(|_| ())
        }
    }
}

async fn receive_outer_opaque(
    outer: &AuthenticatedOuterRelayConnection,
    association_id: [u8; 32],
) -> Result<Vec<u8>, ()> {
    outer
        .receive_opaque_for(association_id)
        .await
        .map_err(|_| ())
}

fn encode_relay_fragments(
    association_id: [u8; 32],
    initiator: bool,
    datagram_sequence: u64,
    message_id: u64,
    payload: &[u8],
) -> Result<Vec<Vec<u8>>, ()> {
    if association_id == [0; 32]
        || datagram_sequence == 0
        || message_id == 0
        || payload.is_empty()
        || payload.len() > OPAQUE_RELAY_MAX_INNER_BYTES
    {
        return Err(());
    }
    let count = payload.len().div_ceil(OPAQUE_RELAY_FRAGMENT_BYTES);
    if count == 0 || count > 8 {
        return Err(());
    }
    payload
        .chunks(OPAQUE_RELAY_FRAGMENT_BYTES)
        .enumerate()
        .map(|(index, part)| {
            let mut output = Vec::with_capacity(OPAQUE_RELAY_HEADER_BYTES + part.len());
            output.extend_from_slice(&OPAQUE_RELAY_MAGIC);
            output.push(OPAQUE_RELAY_VERSION);
            output.extend_from_slice(&association_id);
            output.push(if initiator { 0 } else { 1 });
            output.extend_from_slice(&datagram_sequence.to_be_bytes());
            output.extend_from_slice(&message_id.to_be_bytes());
            output.push(index as u8);
            output.push(count as u8);
            output.extend_from_slice(&(payload.len() as u16).to_be_bytes());
            output.extend_from_slice(&(part.len() as u16).to_be_bytes());
            output.extend_from_slice(part);
            Ok(output)
        })
        .collect()
}

fn random_request_id() -> [u8; 16] {
    let mut value = [0u8; 16];
    OsRng.fill_bytes(&mut value);
    value
}

fn random_nonzero_u64() -> u64 {
    loop {
        let value = OsRng.next_u64();
        if value != 0 {
            return value;
        }
    }
}

fn unix_now_seconds() -> Result<u64, ()> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .map_err(|_| ())
}

fn endpoint_socket(endpoint: &ReachabilityEndpointV1) -> Option<SocketAddr> {
    let ip = match endpoint.host {
        HostAddressV1::Ipv4(value) => IpAddr::V4(value.into()),
        HostAddressV1::Ipv6(value) => IpAddr::V6(value.into()),
        HostAddressV1::Dns(_) => return None,
    };
    Some(SocketAddr::new(ip, endpoint.port))
}

fn endpoint_matches_socket(endpoint: &ReachabilityEndpointV1, socket: SocketAddr) -> bool {
    endpoint_socket(endpoint) == Some(socket)
}

fn hash_endpoint(hash: &mut blake3::Hasher, endpoint: &ReachabilityEndpointV1) {
    hash_host(hash, &endpoint.host);
    hash.update(&endpoint.port.to_be_bytes());
}

fn hash_relay_endpoint(hash: &mut blake3::Hasher, endpoint: &RelayEndpointV1) {
    hash.update(&[transport_tag(endpoint.transport)]);
    hash_host(hash, &endpoint.host);
    hash.update(&endpoint.port.to_be_bytes());
}

fn hash_host(hash: &mut blake3::Hasher, host: &HostAddressV1) {
    match host {
        HostAddressV1::Dns(value) => {
            hash.update(&[1]);
            hash.update(&(value.len() as u64).to_be_bytes());
            hash.update(value.as_bytes());
        }
        HostAddressV1::Ipv4(value) => {
            hash.update(&[2]);
            hash.update(value);
        }
        HostAddressV1::Ipv6(value) => {
            hash.update(&[3]);
            hash.update(value);
        }
    }
}

fn hash_socket(hash: &mut blake3::Hasher, socket: SocketAddr) {
    match socket.ip() {
        IpAddr::V4(value) => {
            hash.update(&[4]);
            hash.update(&value.octets());
        }
        IpAddr::V6(value) => {
            hash.update(&[6]);
            hash.update(&value.octets());
        }
    }
    hash.update(&socket.port().to_be_bytes());
}

fn path_tag(value: RoutePathKindV1) -> u8 {
    match value {
        RoutePathKindV1::Direct => 1,
        RoutePathKindV1::HolePunched => 2,
        RoutePathKindV1::RelayUdp => 3,
        RoutePathKindV1::RelayTcp443 => 4,
    }
}

fn direct_kind_tag(value: DirectCandidateKindV1) -> u8 {
    match value {
        DirectCandidateKindV1::Host => 1,
        DirectCandidateKindV1::ServerReflexive => 2,
        DirectCandidateKindV1::ProviderMapped => 3,
    }
}

fn transport_tag(value: RelayTransportV1) -> u8 {
    match value {
        RelayTransportV1::QuicUdp => 1,
        RelayTransportV1::TlsTcp443 => 2,
    }
}
