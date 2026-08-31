//! Canonical target-scoped connectivity signaling objects.

use ku_core::foundation::NodeId;

use crate::{PrivateCandidateV1, ReachabilityEndpointV1};

pub const MAX_CONNECTIVITY_SIGNAL_BYTES: usize = 65_536;
pub const MAX_PRIVATE_SIGNAL_CANDIDATES: usize = 8;
pub const HOLE_PUNCH_START_DELAY_MS: u64 = 500;
pub const HOLE_PUNCH_INTERVAL_MS: u64 = 200;
pub const HOLE_PUNCH_ATTEMPT_COUNT: u64 = 10;

pub mod connectivity_schema_id {
    pub const REFLEXIVE_OBSERVATION: u64 = 56;
    pub const HOLE_PUNCH_SCHEDULE: u64 = 57;
    pub const RELAY_CONNECT_REQUEST: u64 = 58;
    pub const RELAY_ASSOCIATION: u64 = 59;
    pub const PRIVATE_CANDIDATE_SIGNAL: u64 = 60;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReflexiveObservationV1 {
    pub format: u64,
    pub relay_node_id: NodeId,
    pub target_node_id: NodeId,
    pub reservation_id: [u8; 32],
    pub observed_endpoint: ReachabilityEndpointV1,
    pub network_epoch: u64,
    pub sequence: u64,
    pub issued_at: u64,
    pub expires_at: u64,
    pub relay_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HolePunchScheduleV1 {
    pub format: u64,
    pub relay_node_id: NodeId,
    pub initiator_node_id: NodeId,
    pub responder_node_id: NodeId,
    pub initiator_reservation_id: [u8; 32],
    pub responder_reservation_id: [u8; 32],
    pub rendezvous_token: [u8; 32],
    pub association_barrier_digest: [u8; 32],
    pub start_delay_ms: u64,
    pub interval_ms: u64,
    pub attempt_count: u64,
    pub expires_at: u64,
    pub relay_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelayConnectRequestV1 {
    pub format: u64,
    pub initiator_node_id: NodeId,
    pub target_node_id: NodeId,
    pub initiator_reservation_id: [u8; 32],
    pub target_reservation_id: [u8; 32],
    pub nonce: [u8; 32],
    pub sequence: u64,
    pub issued_at: u64,
    pub expires_at: u64,
    pub initiator_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelayAssociationV1 {
    pub format: u64,
    pub relay_node_id: NodeId,
    pub initiator_node_id: NodeId,
    pub target_node_id: NodeId,
    pub initiator_reservation_id: [u8; 32],
    pub target_reservation_id: [u8; 32],
    pub association_id: [u8; 32],
    pub issued_at: u64,
    pub expires_at: u64,
    pub relay_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivateCandidateSignalV1 {
    pub format: u64,
    pub sender_node_id: NodeId,
    pub target_node_id: NodeId,
    pub session_id: [u8; 32],
    pub network_epoch: u64,
    pub candidates: Vec<PrivateCandidateV1>,
    pub sequence: u64,
    pub issued_at: u64,
    pub expires_at: u64,
    pub sender_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectivitySignalingV1 {
    ReflexiveObservation(ReflexiveObservationV1),
    HolePunchSchedule(HolePunchScheduleV1),
    RelayConnectRequest(RelayConnectRequestV1),
    RelayAssociation(RelayAssociationV1),
    PrivateCandidateSignal(PrivateCandidateSignalV1),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectivitySignatureRoleV1 {
    ReflexiveRelay,
    HolePunchRelay,
    RelayConnectInitiator,
    RelayAssociationRelay,
    PrivateCandidateSender,
}
