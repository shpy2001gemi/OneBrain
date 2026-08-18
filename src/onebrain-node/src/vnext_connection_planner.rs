//! Identity-first promotion of authenticated carrier selections.

#![cfg(feature = "vnext-outbound-first")]

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use ku_core::foundation::NodeId;
use ku_net::transport::OBPConnection;
use ku_net::vnext_connection_executor::{
    AuthenticatedRouteConnection, ValidatedDirectDialCandidate, VerifiedDirectSelectionV1,
    VerifiedPlannerSelection,
};
use ku_net::vnext_reachability_crypto::ValidatedReachabilityAdvertisement;
use ku_net::vnext_relay_discovery::ReachabilityFuture;
use ku_net::vnext_resource_gate::SessionAdmission;
use ku_net::vnext_route_plan::RouteFailure;
use ku_net::vnext_session::AuthenticatedSession;
use onebrain_protocol::{
    DirectCandidateKindV1, ReachabilityEndpointV1, RelayEndpointV1, RelayTransportV1,
    RoutePathKindV1,
};
use thiserror::Error;

use crate::vnext_network_runtime::VNextNetworkRuntimeError;
use crate::vnext_outbox::DurableCheckpointV1;

pub trait ExpectedPeerConnector: Send + Sync {
    fn connect_expected<'a>(
        &'a self,
        expected_peer: NodeId,
        advertisement: &'a ValidatedReachabilityAdvertisement,
    ) -> ReachabilityFuture<'a, Result<AuthenticatedRouteConnection, RouteFailure>>;
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
}
