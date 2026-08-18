//! Authenticated, replay-safe connectivity signaling for outbound-first routes.

#[cfg(feature = "quic")]
use std::net::SocketAddr;
use std::sync::Arc;

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use ku_core::foundation::NodeId;
#[cfg(feature = "quic")]
use onebrain_protocol::ReachabilityEndpointV1;
use onebrain_protocol::{
    connectivity_signing_bytes, decode_connectivity_signaling, ConnectivitySignalingV1,
    ConnectivitySignatureRoleV1, HolePunchScheduleV1, PrivateCandidateSignalV1,
    ReflexiveObservationV1, RelayAssociationV1, RelayCandidateV1, RelayConnectRequestV1,
    HOLE_PUNCH_ATTEMPT_COUNT, HOLE_PUNCH_INTERVAL_MS, HOLE_PUNCH_START_DELAY_MS,
};

use crate::vnext_candidates::{endpoint_is_public, CandidateBoundaryError, PrivateCandidateSet};
use crate::vnext_reachability_crypto::{
    KnownPeerIdentity, ReachabilityNonceDomainV1, ReachabilityReplayStore,
    ReachabilitySequenceKeyV1, ReachabilitySequenceKindV1, RelayAdmissionError,
    ValidatedRelayReservation,
};
use crate::vnext_session::AuthenticatedSession;

const CLOCK_SKEW_SECONDS: u64 = 30;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectivitySignalingError {
    Codec,
    IdentityMismatch,
    SignatureInvalid,
    ReservationMismatch,
    PublicEndpointInvalid,
    Expired,
    Replay,
    StateUnavailable,
    PeerNotAuthenticated,
    SessionMismatch,
    ScheduleMismatch,
    AssociationMismatch,
    RouteDeadlineExceeded,
    AsymmetricDelivery,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedReflexiveObservation {
    canonical: ReflexiveObservationV1,
    digest: [u8; 32],
}

impl ValidatedReflexiveObservation {
    pub fn canonical(&self) -> &ReflexiveObservationV1 {
        &self.canonical
    }

    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedRelayConnectRequest {
    canonical: RelayConnectRequestV1,
    digest: [u8; 32],
}

impl ValidatedRelayConnectRequest {
    pub fn canonical(&self) -> &RelayConnectRequestV1 {
        &self.canonical
    }

    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedRelayAssociation {
    canonical: RelayAssociationV1,
    digest: [u8; 32],
}

impl ValidatedRelayAssociation {
    pub fn canonical(&self) -> &RelayAssociationV1 {
        &self.canonical
    }

    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }

    pub fn validate_datagram_binding(
        &self,
        association_id: [u8; 32],
        initiator_reservation_id: [u8; 32],
        target_reservation_id: [u8; 32],
    ) -> Result<(), ConnectivitySignalingError> {
        if self.canonical.association_id != association_id
            || self.canonical.initiator_reservation_id != initiator_reservation_id
            || self.canonical.target_reservation_id != target_reservation_id
        {
            return Err(ConnectivitySignalingError::AssociationMismatch);
        }
        Ok(())
    }

    pub fn admitted_relay_path(
        &self,
        candidate: RelayCandidateV1,
    ) -> Result<crate::vnext_route_plan::AdmittedRelayPath, ConnectivitySignalingError> {
        crate::vnext_route_plan::AdmittedRelayPath::from_validated_association(candidate, self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedHolePunchSchedule {
    canonical: HolePunchScheduleV1,
    digest: [u8; 32],
    association_digest: [u8; 32],
}

impl ValidatedHolePunchSchedule {
    pub fn canonical(&self) -> &HolePunchScheduleV1 {
        &self.canonical
    }

    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }

    pub fn association_digest(&self) -> [u8; 32] {
        self.association_digest
    }

    pub fn admitted_hole_punch(
        &self,
        priority: u32,
    ) -> crate::vnext_route_plan::AdmittedHolePunchCandidate {
        crate::vnext_route_plan::AdmittedHolePunchCandidate::from_validated_schedule(self, priority)
    }

    /// Build the ten relative send instants after both authenticated sides
    /// have acknowledged the same schedule. A delivery skew equal to one
    /// interval is rejected because it no longer guarantees overlap.
    pub fn coordinated_attempts(
        &self,
        initiator_received_ms: u64,
        responder_received_ms: u64,
        route_deadline_ms: u64,
    ) -> Result<Vec<u64>, ConnectivitySignalingError> {
        if initiator_received_ms.abs_diff(responder_received_ms) >= self.canonical.interval_ms {
            return Err(ConnectivitySignalingError::AsymmetricDelivery);
        }
        let barrier = initiator_received_ms.max(responder_received_ms);
        let first = barrier
            .checked_add(self.canonical.start_delay_ms)
            .ok_or(ConnectivitySignalingError::RouteDeadlineExceeded)?;
        let mut attempts = Vec::with_capacity(self.canonical.attempt_count as usize);
        for index in 0..self.canonical.attempt_count {
            let offset = index
                .checked_mul(self.canonical.interval_ms)
                .ok_or(ConnectivitySignalingError::RouteDeadlineExceeded)?;
            let instant = first
                .checked_add(offset)
                .ok_or(ConnectivitySignalingError::RouteDeadlineExceeded)?;
            if instant > route_deadline_ms {
                return Err(ConnectivitySignalingError::RouteDeadlineExceeded);
            }
            attempts.push(instant);
        }
        Ok(attempts)
    }
}

#[cfg(feature = "quic")]
pub struct ValidatedPunchedCarrier {
    connection: crate::transport::OBPConnection,
    schedule: ValidatedHolePunchSchedule,
    connected_endpoint: ReachabilityEndpointV1,
    connected_socket: SocketAddr,
    transport_binding_digest: [u8; 32],
}

#[cfg(feature = "quic")]
impl ValidatedPunchedCarrier {
    #[allow(dead_code)]
    pub(crate) fn seal(
        connection: crate::transport::OBPConnection,
        schedule: &ValidatedHolePunchSchedule,
        association: &ValidatedRelayAssociation,
        connected_endpoint: ReachabilityEndpointV1,
        connected_socket: SocketAddr,
    ) -> Result<Self, ConnectivitySignalingError> {
        if schedule.association_digest != association.digest
            || schedule.canonical.initiator_reservation_id
                != association.canonical.initiator_reservation_id
            || schedule.canonical.responder_reservation_id
                != association.canonical.target_reservation_id
            || !endpoint_matches_socket(&connected_endpoint, connected_socket)
        {
            return Err(ConnectivitySignalingError::AssociationMismatch);
        }
        let transport_binding = connection
            .transport_binding()
            .map_err(|_| ConnectivitySignalingError::AssociationMismatch)?;
        let transport_binding_digest = *blake3::hash(&transport_binding).as_bytes();
        Ok(Self {
            connection,
            schedule: schedule.clone(),
            connected_endpoint,
            connected_socket,
            transport_binding_digest,
        })
    }

    pub fn schedule_digest(&self) -> [u8; 32] {
        self.schedule.digest
    }

    pub fn relay_node_id(&self) -> NodeId {
        self.schedule.canonical.relay_node_id
    }

    pub fn reservation_ids(&self) -> ([u8; 32], [u8; 32]) {
        (
            self.schedule.canonical.initiator_reservation_id,
            self.schedule.canonical.responder_reservation_id,
        )
    }

    pub fn connected_endpoint(&self) -> &ReachabilityEndpointV1 {
        &self.connected_endpoint
    }

    pub fn connected_socket(&self) -> SocketAddr {
        self.connected_socket
    }

    pub fn transport_binding_digest(&self) -> [u8; 32] {
        self.transport_binding_digest
    }

    pub fn into_connection(self) -> crate::transport::OBPConnection {
        self.connection
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthenticatedPrivateCandidateSignal {
    canonical: PrivateCandidateSignalV1,
    authenticated_session_id: [u8; 32],
    candidates: PrivateCandidateSet,
    digest: [u8; 32],
}

impl AuthenticatedPrivateCandidateSignal {
    pub fn authenticated_session_id(&self) -> [u8; 32] {
        self.authenticated_session_id
    }

    pub fn candidate_count(&self) -> usize {
        self.candidates.len()
    }

    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }

    pub fn is_eligible(
        &self,
        expected_peer: NodeId,
        authenticated_session: [u8; 32],
        network_epoch: u64,
        now: u64,
    ) -> bool {
        now <= self.canonical.expires_at
            && self
                .candidates
                .is_eligible(expected_peer, authenticated_session, network_epoch)
    }
}

pub struct ConnectivitySignalingValidator {
    replay: Arc<dyn ReachabilityReplayStore>,
}

impl ConnectivitySignalingValidator {
    pub fn new(replay: Arc<dyn ReachabilityReplayStore>) -> Self {
        Self { replay }
    }

    pub fn validate_reflexive_observation(
        &self,
        bytes: &[u8],
        relay: &KnownPeerIdentity,
        expected_target: NodeId,
        reservation: &ValidatedRelayReservation,
        now: u64,
    ) -> Result<ValidatedReflexiveObservation, ConnectivitySignalingError> {
        let root = decode(bytes)?;
        let ConnectivitySignalingV1::ReflexiveObservation(value) = root.clone() else {
            return Err(ConnectivitySignalingError::Codec);
        };
        validate_identity(relay)?;
        if value.relay_node_id != relay.node_id
            || value.target_node_id != expected_target
            || value.reservation_id != reservation.canonical().reservation_id
            || reservation.canonical().relay_node_id != relay.node_id
            || reservation.canonical().target_node_id != expected_target
        {
            return Err(ConnectivitySignalingError::ReservationMismatch);
        }
        if !endpoint_is_public(&value.observed_endpoint) {
            return Err(ConnectivitySignalingError::PublicEndpointInvalid);
        }
        fresh(value.issued_at, value.expires_at, now)?;
        verify(&root, ConnectivitySignatureRoleV1::ReflexiveRelay, relay)?;
        let digest = digest(bytes);
        self.advance(
            ReachabilitySequenceKindV1::ReflexiveObservation,
            relay,
            reservation.digest(),
            value.sequence,
            digest,
            value.expires_at,
        )?;
        Ok(ValidatedReflexiveObservation {
            canonical: value,
            digest,
        })
    }

    pub fn validate_connect_request(
        &self,
        bytes: &[u8],
        initiator: &KnownPeerIdentity,
        expected_target: NodeId,
        initiator_reservation: &ValidatedRelayReservation,
        target_reservation: &ValidatedRelayReservation,
        now: u64,
    ) -> Result<ValidatedRelayConnectRequest, ConnectivitySignalingError> {
        let root = decode(bytes)?;
        let ConnectivitySignalingV1::RelayConnectRequest(value) = root.clone() else {
            return Err(ConnectivitySignalingError::Codec);
        };
        validate_identity(initiator)?;
        let relay_id = initiator_reservation.canonical().relay_node_id;
        if value.initiator_node_id != initiator.node_id
            || value.target_node_id != expected_target
            || value.initiator_reservation_id != initiator_reservation.canonical().reservation_id
            || value.target_reservation_id != target_reservation.canonical().reservation_id
            || initiator_reservation.canonical().target_node_id != initiator.node_id
            || target_reservation.canonical().target_node_id != expected_target
            || target_reservation.canonical().relay_node_id != relay_id
        {
            return Err(ConnectivitySignalingError::ReservationMismatch);
        }
        fresh(value.issued_at, value.expires_at, now)?;
        verify(
            &root,
            ConnectivitySignatureRoleV1::RelayConnectInitiator,
            initiator,
        )?;
        let digest = digest(bytes);
        let scope = pair_scope(
            value.initiator_node_id,
            value.target_node_id,
            value.initiator_reservation_id,
            value.target_reservation_id,
        );
        self.advance(
            ReachabilitySequenceKindV1::RelayConnectRequest,
            initiator,
            &scope,
            value.sequence,
            digest,
            value.expires_at,
        )?;
        self.replay
            .consume_nonce(
                ReachabilityNonceDomainV1::RelayConnect,
                scope,
                value.nonce,
                value.expires_at,
            )
            .map_err(map_replay)?;
        Ok(ValidatedRelayConnectRequest {
            canonical: value,
            digest,
        })
    }

    pub fn validate_association(
        &self,
        bytes: &[u8],
        relay: &KnownPeerIdentity,
        request: &ValidatedRelayConnectRequest,
        initiator_reservation: &ValidatedRelayReservation,
        target_reservation: &ValidatedRelayReservation,
        now: u64,
    ) -> Result<ValidatedRelayAssociation, ConnectivitySignalingError> {
        let root = decode(bytes)?;
        let ConnectivitySignalingV1::RelayAssociation(value) = root.clone() else {
            return Err(ConnectivitySignalingError::Codec);
        };
        validate_identity(relay)?;
        let request = request.canonical();
        if value.relay_node_id != relay.node_id
            || value.initiator_node_id != request.initiator_node_id
            || value.target_node_id != request.target_node_id
            || value.initiator_reservation_id != request.initiator_reservation_id
            || value.target_reservation_id != request.target_reservation_id
            || initiator_reservation.canonical().reservation_id != value.initiator_reservation_id
            || target_reservation.canonical().reservation_id != value.target_reservation_id
            || initiator_reservation.canonical().relay_node_id != relay.node_id
            || target_reservation.canonical().relay_node_id != relay.node_id
        {
            return Err(ConnectivitySignalingError::AssociationMismatch);
        }
        fresh(value.issued_at, value.expires_at, now)?;
        verify(
            &root,
            ConnectivitySignatureRoleV1::RelayAssociationRelay,
            relay,
        )?;
        Ok(ValidatedRelayAssociation {
            canonical: value,
            digest: digest(bytes),
        })
    }

    pub fn validate_hole_punch_schedule(
        &self,
        bytes: &[u8],
        relay: &KnownPeerIdentity,
        association: &ValidatedRelayAssociation,
        now: u64,
        route_deadline_unix_ms: u64,
    ) -> Result<ValidatedHolePunchSchedule, ConnectivitySignalingError> {
        let root = decode(bytes)?;
        let ConnectivitySignalingV1::HolePunchSchedule(value) = root.clone() else {
            return Err(ConnectivitySignalingError::Codec);
        };
        let admitted = association.canonical();
        if value.relay_node_id != relay.node_id
            || value.initiator_node_id != admitted.initiator_node_id
            || value.responder_node_id != admitted.target_node_id
            || value.initiator_reservation_id != admitted.initiator_reservation_id
            || value.responder_reservation_id != admitted.target_reservation_id
            || value.association_barrier_digest != association.digest
            || value.start_delay_ms != HOLE_PUNCH_START_DELAY_MS
            || value.interval_ms != HOLE_PUNCH_INTERVAL_MS
            || value.attempt_count != HOLE_PUNCH_ATTEMPT_COUNT
        {
            return Err(ConnectivitySignalingError::ScheduleMismatch);
        }
        if value.expires_at.saturating_add(CLOCK_SKEW_SECONDS) < now {
            return Err(ConnectivitySignalingError::Expired);
        }
        let last_offset = value
            .attempt_count
            .saturating_sub(1)
            .saturating_mul(value.interval_ms);
        let finish_ms = now
            .saturating_mul(1_000)
            .saturating_add(value.start_delay_ms)
            .saturating_add(last_offset);
        if finish_ms > route_deadline_unix_ms || finish_ms > value.expires_at.saturating_mul(1_000)
        {
            return Err(ConnectivitySignalingError::RouteDeadlineExceeded);
        }
        verify(&root, ConnectivitySignatureRoleV1::HolePunchRelay, relay)?;
        self.replay
            .consume_nonce(
                ReachabilityNonceDomainV1::HolePunchToken,
                association.digest,
                value.rendezvous_token,
                value.expires_at,
            )
            .map_err(map_replay)?;
        Ok(ValidatedHolePunchSchedule {
            canonical: value,
            digest: digest(bytes),
            association_digest: association.digest,
        })
    }

    pub fn validate_private_candidate_signal(
        &self,
        bytes: &[u8],
        sender: &KnownPeerIdentity,
        expected_target: NodeId,
        authenticated_session: Option<&AuthenticatedSession>,
        now: u64,
    ) -> Result<AuthenticatedPrivateCandidateSignal, ConnectivitySignalingError> {
        let session =
            authenticated_session.ok_or(ConnectivitySignalingError::PeerNotAuthenticated)?;
        let root = decode(bytes)?;
        let ConnectivitySignalingV1::PrivateCandidateSignal(value) = root.clone() else {
            return Err(ConnectivitySignalingError::Codec);
        };
        validate_identity(sender)?;
        let session_pair_matches = (session.initiator == sender.node_id
            && session.responder == expected_target)
            || (session.responder == sender.node_id && session.initiator == expected_target);
        if !session_pair_matches
            || value.sender_node_id != sender.node_id
            || value.target_node_id != expected_target
            || value.session_id != session.session_id
        {
            return Err(ConnectivitySignalingError::SessionMismatch);
        }
        fresh(value.issued_at, value.expires_at, now)?;
        verify(
            &root,
            ConnectivitySignatureRoleV1::PrivateCandidateSender,
            sender,
        )?;
        let candidates = PrivateCandidateSet::authenticated(
            expected_target,
            session.session_id,
            value.network_epoch,
            value.candidates.clone(),
        )
        .map_err(map_candidate)?;
        let digest = digest(bytes);
        self.advance(
            ReachabilitySequenceKindV1::PrivateCandidateSignal,
            sender,
            &session.session_id,
            value.sequence,
            digest,
            value.expires_at,
        )?;
        Ok(AuthenticatedPrivateCandidateSignal {
            authenticated_session_id: session.session_id,
            canonical: value,
            candidates,
            digest,
        })
    }

    fn advance(
        &self,
        kind: ReachabilitySequenceKindV1,
        signer: &KnownPeerIdentity,
        scope: &[u8; 32],
        sequence: u64,
        digest: [u8; 32],
        expires_at: u64,
    ) -> Result<(), ConnectivitySignalingError> {
        self.replay
            .check_and_advance_sequence(
                ReachabilitySequenceKeyV1 {
                    kind,
                    signer: signer.public_key,
                    scope: *scope,
                },
                sequence,
                digest,
                expires_at,
            )
            .map_err(map_replay)
    }
}

fn decode(bytes: &[u8]) -> Result<ConnectivitySignalingV1, ConnectivitySignalingError> {
    decode_connectivity_signaling(bytes).map_err(|_| ConnectivitySignalingError::Codec)
}

fn validate_identity(identity: &KnownPeerIdentity) -> Result<(), ConnectivitySignalingError> {
    identity
        .validate()
        .map_err(|_| ConnectivitySignalingError::IdentityMismatch)
}

fn verify(
    root: &ConnectivitySignalingV1,
    role: ConnectivitySignatureRoleV1,
    identity: &KnownPeerIdentity,
) -> Result<(), ConnectivitySignalingError> {
    let signature_bytes = match root {
        ConnectivitySignalingV1::ReflexiveObservation(value) => value.relay_signature,
        ConnectivitySignalingV1::HolePunchSchedule(value) => value.relay_signature,
        ConnectivitySignalingV1::RelayConnectRequest(value) => value.initiator_signature,
        ConnectivitySignalingV1::RelayAssociation(value) => value.relay_signature,
        ConnectivitySignalingV1::PrivateCandidateSignal(value) => value.sender_signature,
    };
    let key = VerifyingKey::from_bytes(&identity.public_key)
        .map_err(|_| ConnectivitySignalingError::SignatureInvalid)?;
    let signing_bytes =
        connectivity_signing_bytes(root, role).map_err(|_| ConnectivitySignalingError::Codec)?;
    key.verify(&signing_bytes, &Signature::from_bytes(&signature_bytes))
        .map_err(|_| ConnectivitySignalingError::SignatureInvalid)
}

fn fresh(issued_at: u64, expires_at: u64, now: u64) -> Result<(), ConnectivitySignalingError> {
    if issued_at > expires_at
        || issued_at > now.saturating_add(CLOCK_SKEW_SECONDS)
        || expires_at.saturating_add(CLOCK_SKEW_SECONDS) < now
    {
        return Err(ConnectivitySignalingError::Expired);
    }
    Ok(())
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    *blake3::hash(bytes).as_bytes()
}

fn pair_scope(
    initiator: NodeId,
    target: NodeId,
    initiator_reservation: [u8; 32],
    target_reservation: [u8; 32],
) -> [u8; 32] {
    let mut hash = blake3::Hasher::new();
    hash.update(b"onebrain/reachability/connect-scope/v1\0");
    hash.update(initiator.as_bytes());
    hash.update(target.as_bytes());
    hash.update(&initiator_reservation);
    hash.update(&target_reservation);
    *hash.finalize().as_bytes()
}

fn map_replay(error: RelayAdmissionError) -> ConnectivitySignalingError {
    match error {
        RelayAdmissionError::Replay
        | RelayAdmissionError::SequenceRollback
        | RelayAdmissionError::ChallengeConsumed => ConnectivitySignalingError::Replay,
        _ => ConnectivitySignalingError::StateUnavailable,
    }
}

fn map_candidate(_error: CandidateBoundaryError) -> ConnectivitySignalingError {
    ConnectivitySignalingError::SessionMismatch
}

#[cfg(feature = "quic")]
#[allow(dead_code)]
fn endpoint_matches_socket(endpoint: &ReachabilityEndpointV1, socket: SocketAddr) -> bool {
    if endpoint.port != socket.port() {
        return false;
    }
    match (&endpoint.host, socket.ip()) {
        (onebrain_protocol::HostAddressV1::Ipv4(expected), std::net::IpAddr::V4(actual)) => {
            *expected == actual.octets()
        }
        (onebrain_protocol::HostAddressV1::Ipv6(expected), std::net::IpAddr::V6(actual)) => {
            *expected == actual.octets()
        }
        (onebrain_protocol::HostAddressV1::Dns(_), _) => true,
        _ => false,
    }
}
