//! Sealed carrier selection and execution without granting route authority.

use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Instant;

use ku_core::foundation::NodeId;
use onebrain_protocol::{
    DirectCandidateKindV1, DirectCandidateV1, HostAddressV1, ReachabilityEndpointV1,
    RelayConnectRequestV1, RelayEndpointV1, RelayTransportV1, RouteAttemptOutcomeV1,
    RouteAttemptV1, RoutePathKindV1, MAX_ROUTE_PLAN_ATTEMPTS,
};

use crate::transport::{OBPConnection, QuicTransport};
use crate::vnext_connectivity_signaling::{ValidatedPunchedCarrier, ValidatedRelayAssociation};
use crate::vnext_reachability_crypto::{
    ValidatedPublicDialEndpoint, ValidatedPublicDialTransportV1, ValidatedRelayDescriptor,
    ValidatedRelayReservation,
};
use crate::vnext_relay_discovery::ReachabilityFuture;
use crate::vnext_relay_tunnel::AuthenticatedOuterRelayConnection;
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
        outer: &'a AuthenticatedOuterRelayConnection,
        deadline: Instant,
    ) -> ReachabilityFuture<'a, Result<OBPConnection, RouteFailure>>;
}

pub trait RelayAssociationClient: Send + Sync {
    fn associate<'a>(
        &'a self,
        request: &'a RelayConnectRequestV1,
        local: &'a ValidatedRelayReservation,
        remote: &'a ValidatedRelayReservation,
        outer: &'a AuthenticatedOuterRelayConnection,
        deadline: Instant,
    ) -> ReachabilityFuture<'a, Result<ValidatedRelayAssociation, RouteFailure>>;
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
                            .associate(request, &input.local, &input.remote, &input.outer, deadline)
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
                    .dial(&input.descriptor, &association, &input.outer, deadline)
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
