//! Authenticated outer-client admission and bounded reservation state.

use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::sync::Arc;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use ku_core::foundation::NodeId;
use onebrain_protocol::{
    encode_relay_control, reachability_signing_bytes, relay_control_signing_bytes,
    ReachabilityObjectV1, ReachabilitySignatureRoleV1, RelayControlSignatureRoleV1, RelayControlV1,
    RelayDenialCodeV1, RelayDenialV1, RelayKeepaliveV1, RelayOuterClientChallengeV1,
    RelayOuterClientHelloV1, RelayReservationV1, RelayReserveRequestV1, RelayRevocationActorV1,
    RelayRevokeV1, MAX_RELAY_CONTROL_VALIDITY_SECONDS,
};

use crate::{DurableRelayState, DurableStateKind};

const CLOCK_SKEW_SECONDS: u64 = 30;
const OUTER_CHALLENGE_VALIDITY_SECONDS: u64 = 30;
const MAX_PENDING_CHALLENGES: usize = 64;
const MAX_CONSUMED_NONCES: usize = 4_096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthenticatedOuterClient {
    client_node_id: NodeId,
    client_public_key: [u8; 32],
    outer_connection_binding: [u8; 32],
    authenticated_at: u64,
    expires_at: u64,
    observed_socket: Option<SocketAddr>,
}

impl AuthenticatedOuterClient {
    pub fn client_node_id(&self) -> NodeId {
        self.client_node_id
    }

    pub fn outer_connection_binding(&self) -> [u8; 32] {
        self.outer_connection_binding
    }

    pub fn expires_at(&self) -> u64 {
        self.expires_at
    }

    pub fn observed_socket(&self) -> Option<SocketAddr> {
        self.observed_socket
    }

    pub(crate) fn bind_observed_socket(mut self, observed_socket: SocketAddr) -> Self {
        self.observed_socket = Some(observed_socket);
        self
    }

    pub(crate) fn client_public_key(&self) -> [u8; 32] {
        self.client_public_key
    }
}

pub struct OuterClientAuthenticator {
    relay_node_id: NodeId,
    relay_signer: SigningKey,
    pending: BTreeMap<[u8; 32], RelayOuterClientChallengeV1>,
    consumed: BTreeSet<[u8; 32]>,
    durable: Option<Arc<DurableRelayState>>,
}

impl OuterClientAuthenticator {
    pub fn new(relay_signer: SigningKey) -> Self {
        let relay_node_id = principal_node_id(relay_signer.verifying_key().as_bytes());
        Self {
            relay_node_id,
            relay_signer,
            pending: BTreeMap::new(),
            consumed: BTreeSet::new(),
            durable: None,
        }
    }

    pub fn new_durable(relay_signer: SigningKey, durable: Arc<DurableRelayState>) -> Self {
        let mut value = Self::new(relay_signer);
        value.durable = Some(durable);
        value
    }

    pub fn issue_challenge(
        &mut self,
        nonce: [u8; 32],
        outer_connection_binding: [u8; 32],
        now: u64,
    ) -> Result<RelayOuterClientChallengeV1, ReservationError> {
        if nonce == [0; 32]
            || self.pending.len() >= MAX_PENDING_CHALLENGES
            || self.pending.contains_key(&nonce)
            || self.consumed.contains(&nonce)
        {
            return Err(ReservationError::Replay);
        }
        let mut challenge = RelayOuterClientChallengeV1 {
            format: 1,
            relay_node_id: self.relay_node_id,
            challenge_nonce: nonce,
            outer_connection_binding,
            issued_at: now,
            expires_at: now + OUTER_CHALLENGE_VALIDITY_SECONDS,
            relay_signature: [0; 64],
        };
        challenge.relay_signature = self
            .relay_signer
            .sign(
                &relay_control_signing_bytes(
                    &RelayControlV1::OuterClientChallenge(challenge.clone()),
                    RelayControlSignatureRoleV1::OuterChallengeRelay,
                )
                .map_err(|_| ReservationError::Codec)?,
            )
            .to_bytes();
        self.pending.insert(nonce, challenge.clone());
        Ok(challenge)
    }

    pub fn authenticate(
        &mut self,
        hello: RelayOuterClientHelloV1,
        outer_connection_binding: [u8; 32],
        now: u64,
    ) -> Result<AuthenticatedOuterClient, ReservationError> {
        let challenge = self
            .pending
            .get(&hello.challenge_nonce)
            .ok_or(ReservationError::ChallengeMissing)?
            .clone();
        fresh(hello.issued_at, hello.expires_at, now)?;
        fresh(challenge.issued_at, challenge.expires_at, now)?;
        if hello.relay_node_id != self.relay_node_id
            || hello.outer_connection_binding != outer_connection_binding
            || challenge.outer_connection_binding != outer_connection_binding
            || principal_node_id(&hello.client_public_key) != hello.client_node_id
        {
            return Err(ReservationError::IdentityMismatch);
        }
        verify_control(
            &RelayControlV1::OuterClientHello(hello.clone()),
            RelayControlSignatureRoleV1::OuterHelloClient,
            hello.client_public_key,
            hello.client_signature,
        )?;
        if self.consumed.len() >= MAX_CONSUMED_NONCES {
            return Err(ReservationError::Capacity);
        }
        if let Some(durable) = &self.durable {
            durable
                .create_new(
                    DurableStateKind::ConsumedNonce,
                    &hello.challenge_nonce,
                    &hello.outer_connection_binding,
                )
                .map_err(|_| ReservationError::State)?;
        }
        self.pending.remove(&hello.challenge_nonce);
        self.consumed.insert(hello.challenge_nonce);
        // The challenge is a short-lived, single-use proof against handshake
        // replay.  It must not also become the lifetime of the authenticated
        // outer carrier: reservations and relay associations intentionally
        // continue after the 30-second challenge window.  Bound the client-
        // asserted session lifetime independently by the canonical relay-
        // control lifetime.
        let session_expires_at = hello
            .expires_at
            .min(now.saturating_add(MAX_RELAY_CONTROL_VALIDITY_SECONDS));
        Ok(AuthenticatedOuterClient {
            client_node_id: hello.client_node_id,
            client_public_key: hello.client_public_key,
            outer_connection_binding,
            authenticated_at: now,
            expires_at: session_expires_at,
            observed_socket: None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredReservation {
    pub canonical: RelayReservationV1,
    pub last_keepalive_sequence: u64,
    pub last_keepalive_at: u64,
    pub bound_outer_connection: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReservationDecision {
    Granted(RelayReservationV1),
    Denied(RelayDenialV1),
}

pub struct ReservationStore {
    reservations: BTreeMap<[u8; 32], StoredReservation>,
    per_target: BTreeMap<NodeId, usize>,
    request_sequences: BTreeMap<NodeId, u64>,
    relay_node_id: NodeId,
    relay_signer: SigningKey,
    max_total: usize,
    max_per_target: usize,
    durable: Option<Arc<DurableRelayState>>,
}

impl ReservationStore {
    pub fn new(
        relay_signer: SigningKey,
        max_total: usize,
        max_per_target: usize,
    ) -> Result<Self, ReservationError> {
        if max_total == 0 || max_per_target == 0 || max_per_target > max_total {
            return Err(ReservationError::InvalidCapacity);
        }
        Ok(Self {
            reservations: BTreeMap::new(),
            per_target: BTreeMap::new(),
            request_sequences: BTreeMap::new(),
            relay_node_id: principal_node_id(relay_signer.verifying_key().as_bytes()),
            relay_signer,
            max_total,
            max_per_target,
            durable: None,
        })
    }

    pub fn new_durable(
        relay_signer: SigningKey,
        max_total: usize,
        max_per_target: usize,
        durable: Arc<DurableRelayState>,
    ) -> Result<Self, ReservationError> {
        let mut value = Self::new(relay_signer, max_total, max_per_target)?;
        value.durable = Some(durable);
        Ok(value)
    }

    pub fn reserve(
        &mut self,
        request: RelayReserveRequestV1,
        client: &AuthenticatedOuterClient,
        now: u64,
    ) -> Result<ReservationDecision, ReservationError> {
        self.verify_client(client, now)?;
        fresh(request.issued_at, request.expires_at, now)?;
        // A reservation is usable only while its exact authenticated outer
        // carrier is alive and until its signed expiry.  Reclaim expired
        // entries before applying the bounded total/per-target admission;
        // otherwise dead grants permanently consume capacity until the relay
        // process is restarted.
        self.prune_expired(now);
        if request.relay_node_id != self.relay_node_id
            || request.target_node_id != client.client_node_id
        {
            return Err(ReservationError::IdentityMismatch);
        }
        if self.reservations.contains_key(&request.reservation_id) {
            return Err(ReservationError::DuplicateReservationId);
        }
        let previous = self.request_sequences.get(&request.target_node_id).copied();
        if request.sequence == 0 || previous.is_some_and(|value| request.sequence != value + 1) {
            return Err(ReservationError::Replay);
        }
        verify_control(
            &RelayControlV1::Reserve(request.clone()),
            RelayControlSignatureRoleV1::ReserveRequestTarget,
            client.client_public_key,
            request.target_request_signature,
        )?;
        let unsigned_grant = RelayReservationV1 {
            format: 1,
            relay_node_id: request.relay_node_id,
            target_node_id: request.target_node_id,
            reservation_id: request.reservation_id,
            transport_scope: request.transport_scope.clone(),
            issued_at: request.issued_at,
            expires_at: request.expires_at,
            target_signature: request.target_reservation_signature,
            relay_signature: [0; 64],
        };
        verify_reachability(
            &ReachabilityObjectV1::RelayReservation(unsigned_grant.clone()),
            ReachabilitySignatureRoleV1::ReservationTarget,
            client.client_public_key,
            request.target_reservation_signature,
        )?;

        let target_count = self
            .per_target
            .get(&request.target_node_id)
            .copied()
            .unwrap_or(0);
        if self.reservations.len() >= self.max_total || target_count >= self.max_per_target {
            let denial = self.signed_denial(&request, RelayDenialCodeV1::Capacity, now)?;
            self.persist_control_sequence(
                request.target_node_id,
                request.sequence,
                &encode_relay_control(&RelayControlV1::Denied(denial.clone()))
                    .map_err(|_| ReservationError::Codec)?,
            )?;
            self.request_sequences
                .insert(request.target_node_id, request.sequence);
            return Ok(ReservationDecision::Denied(denial));
        }
        let mut grant = unsigned_grant;
        grant.relay_signature = self
            .relay_signer
            .sign(
                &reachability_signing_bytes(
                    &ReachabilityObjectV1::RelayReservation(grant.clone()),
                    ReachabilitySignatureRoleV1::ReservationRelay,
                )
                .map_err(|_| ReservationError::Codec)?,
            )
            .to_bytes();
        if let Some(durable) = &self.durable {
            durable
                .create_new(
                    DurableStateKind::Reservation,
                    &request.reservation_id,
                    &encode_relay_control(&RelayControlV1::Granted(grant.clone()))
                        .map_err(|_| ReservationError::Codec)?,
                )
                .map_err(|_| ReservationError::State)?;
        }
        self.persist_control_sequence(
            request.target_node_id,
            request.sequence,
            &request.reservation_id,
        )?;
        self.request_sequences
            .insert(request.target_node_id, request.sequence);
        self.reservations.insert(
            request.reservation_id,
            StoredReservation {
                canonical: grant.clone(),
                last_keepalive_sequence: request.sequence,
                last_keepalive_at: now,
                bound_outer_connection: client.outer_connection_binding,
            },
        );
        self.per_target
            .insert(request.target_node_id, target_count + 1);
        Ok(ReservationDecision::Granted(grant))
    }

    pub fn keepalive(
        &mut self,
        keepalive: RelayKeepaliveV1,
        client: &AuthenticatedOuterClient,
        now: u64,
    ) -> Result<(), ReservationError> {
        self.verify_client(client, now)?;
        fresh(keepalive.issued_at, keepalive.expires_at, now)?;
        let stored = self
            .reservations
            .get(&keepalive.reservation_id)
            .ok_or(ReservationError::UnknownReservation)?;
        if keepalive.relay_node_id != self.relay_node_id
            || keepalive.target_node_id != client.client_node_id
            || stored.canonical.target_node_id != client.client_node_id
            || stored.bound_outer_connection != client.outer_connection_binding
        {
            return Err(ReservationError::ConnectionMismatch);
        }
        if keepalive.sequence != stored.last_keepalive_sequence + 1 {
            return Err(ReservationError::Replay);
        }
        verify_control(
            &RelayControlV1::Keepalive(keepalive.clone()),
            RelayControlSignatureRoleV1::KeepaliveTarget,
            client.client_public_key,
            keepalive.target_signature,
        )?;
        self.persist_control_sequence(
            keepalive.target_node_id,
            keepalive.sequence,
            &encode_relay_control(&RelayControlV1::Keepalive(keepalive.clone()))
                .map_err(|_| ReservationError::Codec)?,
        )?;
        let stored = self
            .reservations
            .get_mut(&keepalive.reservation_id)
            .ok_or(ReservationError::UnknownReservation)?;
        stored.last_keepalive_sequence = keepalive.sequence;
        stored.last_keepalive_at = now;
        Ok(())
    }

    pub fn revoke(
        &mut self,
        revoke: RelayRevokeV1,
        client: &AuthenticatedOuterClient,
        now: u64,
    ) -> Result<RelayReservationV1, ReservationError> {
        self.verify_client(client, now)?;
        fresh(revoke.issued_at, revoke.expires_at, now)?;
        if revoke.actor != RelayRevocationActorV1::Target {
            return Err(ReservationError::IdentityMismatch);
        }
        let stored = self
            .reservations
            .get(&revoke.reservation_id)
            .ok_or(ReservationError::UnknownReservation)?;
        if revoke.relay_node_id != self.relay_node_id
            || revoke.target_node_id != client.client_node_id
            || stored.bound_outer_connection != client.outer_connection_binding
            || revoke.sequence != stored.last_keepalive_sequence + 1
        {
            return Err(ReservationError::ConnectionMismatch);
        }
        verify_control(
            &RelayControlV1::Revoke(revoke.clone()),
            RelayControlSignatureRoleV1::RevokeActor,
            client.client_public_key,
            revoke.actor_signature,
        )?;
        if let Some(durable) = &self.durable {
            durable
                .create_new(
                    DurableStateKind::Revocation,
                    &revoke.reservation_id,
                    &encode_relay_control(&RelayControlV1::Revoke(revoke.clone()))
                        .map_err(|_| ReservationError::Codec)?,
                )
                .map_err(|_| ReservationError::State)?;
        }
        let removed = self
            .remove_reservation(revoke.reservation_id)
            .expect("validated reservation must still exist");
        Ok(removed.canonical)
    }

    /// Reclaim every reservation bound to a carrier that has closed.
    ///
    /// Reservations cannot migrate to another authenticated outer connection:
    /// keepalive, revoke, reflexive observation, and association all bind the
    /// grant to `bound_outer_connection`.  Retaining such a grant after that
    /// carrier closes therefore creates an unusable capacity leak.
    pub fn release_connection(&mut self, binding: [u8; 32]) -> usize {
        let reservation_ids = self
            .reservations
            .iter()
            .filter_map(|(reservation_id, stored)| {
                (stored.bound_outer_connection == binding).then_some(*reservation_id)
            })
            .collect::<Vec<_>>();
        let released = reservation_ids.len();
        for reservation_id in reservation_ids {
            self.remove_reservation(reservation_id);
        }
        released
    }

    pub fn get(&self, reservation_id: [u8; 32]) -> Option<&StoredReservation> {
        self.reservations.get(&reservation_id)
    }

    pub fn len(&self) -> usize {
        self.reservations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.reservations.is_empty()
    }

    fn prune_expired(&mut self, now: u64) -> usize {
        let reservation_ids = self
            .reservations
            .iter()
            .filter_map(|(reservation_id, stored)| {
                (stored.canonical.expires_at <= now).then_some(*reservation_id)
            })
            .collect::<Vec<_>>();
        let released = reservation_ids.len();
        for reservation_id in reservation_ids {
            self.remove_reservation(reservation_id);
        }
        released
    }

    fn remove_reservation(&mut self, reservation_id: [u8; 32]) -> Option<StoredReservation> {
        let removed = self.reservations.remove(&reservation_id)?;
        let target = removed.canonical.target_node_id;
        if let Some(count) = self.per_target.get_mut(&target) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.per_target.remove(&target);
            }
        }
        Some(removed)
    }

    fn verify_client(
        &self,
        client: &AuthenticatedOuterClient,
        now: u64,
    ) -> Result<(), ReservationError> {
        if client.expires_at.saturating_add(CLOCK_SKEW_SECONDS) < now
            || principal_node_id(&client.client_public_key) != client.client_node_id
            || client.authenticated_at > now.saturating_add(CLOCK_SKEW_SECONDS)
        {
            return Err(ReservationError::ClientExpired);
        }
        Ok(())
    }

    fn signed_denial(
        &self,
        request: &RelayReserveRequestV1,
        code: RelayDenialCodeV1,
        now: u64,
    ) -> Result<RelayDenialV1, ReservationError> {
        let mut denial = RelayDenialV1 {
            format: 1,
            relay_node_id: self.relay_node_id,
            target_node_id: request.target_node_id,
            reservation_id: request.reservation_id,
            code,
            retry_after: 20,
            issued_at: now,
            expires_at: now + 30,
            relay_signature: [0; 64],
        };
        denial.relay_signature = self
            .relay_signer
            .sign(
                &relay_control_signing_bytes(
                    &RelayControlV1::Denied(denial.clone()),
                    RelayControlSignatureRoleV1::DenialRelay,
                )
                .map_err(|_| ReservationError::Codec)?,
            )
            .to_bytes();
        Ok(denial)
    }

    fn persist_control_sequence(
        &self,
        target: NodeId,
        sequence: u64,
        bytes: &[u8],
    ) -> Result<(), ReservationError> {
        if let Some(durable) = &self.durable {
            let mut key = Vec::with_capacity(40);
            key.extend_from_slice(target.as_bytes());
            key.extend_from_slice(&sequence.to_be_bytes());
            durable
                .create_new(DurableStateKind::ControlFloor, &key, bytes)
                .map_err(|_| ReservationError::State)?;
        }
        Ok(())
    }
}

fn fresh(issued_at: u64, expires_at: u64, now: u64) -> Result<(), ReservationError> {
    if issued_at > expires_at
        || issued_at > now.saturating_add(CLOCK_SKEW_SECONDS)
        || expires_at.saturating_add(CLOCK_SKEW_SECONDS) < now
    {
        Err(ReservationError::Expired)
    } else {
        Ok(())
    }
}

fn verify_control(
    value: &RelayControlV1,
    role: RelayControlSignatureRoleV1,
    public_key: [u8; 32],
    signature: [u8; 64],
) -> Result<(), ReservationError> {
    let key = VerifyingKey::from_bytes(&public_key).map_err(|_| ReservationError::Signature)?;
    let bytes = relay_control_signing_bytes(value, role).map_err(|_| ReservationError::Codec)?;
    key.verify(&bytes, &Signature::from_bytes(&signature))
        .map_err(|_| ReservationError::Signature)
}

fn verify_reachability(
    value: &ReachabilityObjectV1,
    role: ReachabilitySignatureRoleV1,
    public_key: [u8; 32],
    signature: [u8; 64],
) -> Result<(), ReservationError> {
    let key = VerifyingKey::from_bytes(&public_key).map_err(|_| ReservationError::Signature)?;
    let bytes = reachability_signing_bytes(value, role).map_err(|_| ReservationError::Codec)?;
    key.verify(&bytes, &Signature::from_bytes(&signature))
        .map_err(|_| ReservationError::Signature)
}

pub fn principal_node_id(public_key: &[u8; 32]) -> NodeId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:session-node-principal:1\0");
    hasher.update(public_key);
    NodeId::from_bytes(*hasher.finalize().as_bytes())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReservationError {
    Codec,
    Signature,
    IdentityMismatch,
    ChallengeMissing,
    ConnectionMismatch,
    DuplicateReservationId,
    UnknownReservation,
    InvalidCapacity,
    Capacity,
    Replay,
    Expired,
    ClientExpired,
    State,
}

impl std::fmt::Display for ReservationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "OBP_RELAY_RESERVATION: {self:?}")
    }
}

impl std::error::Error for ReservationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use onebrain_protocol::RelayTransportV1;

    fn authenticated_client(
        relay_key: &SigningKey,
        client_key: &SigningKey,
        binding: [u8; 32],
        nonce: [u8; 32],
    ) -> AuthenticatedOuterClient {
        let mut authenticator = OuterClientAuthenticator::new(relay_key.clone());
        authenticator.issue_challenge(nonce, binding, 100).unwrap();
        let mut hello = RelayOuterClientHelloV1 {
            format: 1,
            relay_node_id: principal_node_id(relay_key.verifying_key().as_bytes()),
            client_node_id: principal_node_id(client_key.verifying_key().as_bytes()),
            client_public_key: *client_key.verifying_key().as_bytes(),
            challenge_nonce: nonce,
            outer_connection_binding: binding,
            issued_at: 100,
            expires_at: 130,
            client_signature: [0; 64],
        };
        hello.client_signature = client_key
            .sign(
                &relay_control_signing_bytes(
                    &RelayControlV1::OuterClientHello(hello.clone()),
                    RelayControlSignatureRoleV1::OuterHelloClient,
                )
                .unwrap(),
            )
            .to_bytes();
        authenticator.authenticate(hello, binding, 110).unwrap()
    }

    fn request(
        relay_key: &SigningKey,
        client_key: &SigningKey,
        reservation_id: [u8; 32],
        sequence: u64,
    ) -> RelayReserveRequestV1 {
        request_at(relay_key, client_key, reservation_id, sequence, 100, 130)
    }

    fn request_at(
        relay_key: &SigningKey,
        client_key: &SigningKey,
        reservation_id: [u8; 32],
        sequence: u64,
        issued_at: u64,
        expires_at: u64,
    ) -> RelayReserveRequestV1 {
        let mut request = RelayReserveRequestV1 {
            format: 1,
            relay_node_id: principal_node_id(relay_key.verifying_key().as_bytes()),
            target_node_id: principal_node_id(client_key.verifying_key().as_bytes()),
            reservation_id,
            transport_scope: vec![RelayTransportV1::QuicUdp],
            sequence,
            issued_at,
            expires_at,
            target_reservation_signature: [0; 64],
            target_request_signature: [0; 64],
        };
        let unsigned = RelayReservationV1 {
            format: 1,
            relay_node_id: request.relay_node_id,
            target_node_id: request.target_node_id,
            reservation_id,
            transport_scope: request.transport_scope.clone(),
            issued_at: request.issued_at,
            expires_at: request.expires_at,
            target_signature: [0; 64],
            relay_signature: [0; 64],
        };
        request.target_reservation_signature = client_key
            .sign(
                &reachability_signing_bytes(
                    &ReachabilityObjectV1::RelayReservation(unsigned),
                    ReachabilitySignatureRoleV1::ReservationTarget,
                )
                .unwrap(),
            )
            .to_bytes();
        request.target_request_signature = client_key
            .sign(
                &relay_control_signing_bytes(
                    &RelayControlV1::Reserve(request.clone()),
                    RelayControlSignatureRoleV1::ReserveRequestTarget,
                )
                .unwrap(),
            )
            .to_bytes();
        request
    }

    #[test]
    fn reservation_store_authenticates_grants_replays_and_capacity() {
        let relay_key = SigningKey::from_bytes(&[1; 32]);
        let client_a = SigningKey::from_bytes(&[2; 32]);
        let client_b = SigningKey::from_bytes(&[3; 32]);
        let auth_a = authenticated_client(&relay_key, &client_a, [4; 32], [5; 32]);
        let auth_b = authenticated_client(&relay_key, &client_b, [6; 32], [7; 32]);
        let mut store = ReservationStore::new(relay_key.clone(), 1, 1).unwrap();
        let first = request(&relay_key, &client_a, [8; 32], 1);
        assert!(matches!(
            store.reserve(first.clone(), &auth_a, 110).unwrap(),
            ReservationDecision::Granted(_)
        ));
        assert_eq!(
            store.reserve(first, &auth_a, 110).unwrap_err(),
            ReservationError::DuplicateReservationId
        );
        assert!(matches!(
            store
                .reserve(request(&relay_key, &client_b, [9; 32], 1), &auth_b, 110)
                .unwrap(),
            ReservationDecision::Denied(RelayDenialV1 {
                code: RelayDenialCodeV1::Capacity,
                ..
            })
        ));
    }

    #[test]
    fn closed_connection_reclaims_per_target_capacity_without_resetting_sequence() {
        let relay_key = SigningKey::from_bytes(&[31; 32]);
        let client_key = SigningKey::from_bytes(&[32; 32]);
        let first_binding = [33; 32];
        let first = authenticated_client(&relay_key, &client_key, first_binding, [34; 32]);
        let mut store = ReservationStore::new(relay_key.clone(), 1, 1).unwrap();

        assert!(matches!(
            store
                .reserve(request(&relay_key, &client_key, [35; 32], 1), &first, 110)
                .unwrap(),
            ReservationDecision::Granted(_)
        ));
        assert_eq!(store.release_connection(first_binding), 1);
        assert!(store.is_empty());

        let second = authenticated_client(&relay_key, &client_key, [36; 32], [37; 32]);
        assert!(matches!(
            store
                .reserve(request(&relay_key, &client_key, [38; 32], 2), &second, 110)
                .unwrap(),
            ReservationDecision::Granted(_)
        ));
    }

    #[test]
    fn expired_reservation_is_pruned_before_capacity_admission() {
        let relay_key = SigningKey::from_bytes(&[41; 32]);
        let client_key = SigningKey::from_bytes(&[42; 32]);
        let client = authenticated_client(&relay_key, &client_key, [43; 32], [44; 32]);
        let mut store = ReservationStore::new(relay_key.clone(), 1, 1).unwrap();

        assert!(matches!(
            store
                .reserve(request(&relay_key, &client_key, [45; 32], 1), &client, 110)
                .unwrap(),
            ReservationDecision::Granted(_)
        ));
        assert!(matches!(
            store
                .reserve(
                    request_at(&relay_key, &client_key, [46; 32], 2, 131, 160),
                    &client,
                    131,
                )
                .unwrap(),
            ReservationDecision::Granted(_)
        ));
        assert_eq!(store.len(), 1);
        assert!(store.get([46; 32]).is_some());
    }

    #[test]
    fn cross_connection_and_copied_outer_hello_reject() {
        let relay_key = SigningKey::from_bytes(&[11; 32]);
        let client_key = SigningKey::from_bytes(&[12; 32]);
        let auth = authenticated_client(&relay_key, &client_key, [13; 32], [14; 32]);
        let mut store = ReservationStore::new(relay_key.clone(), 2, 2).unwrap();
        store
            .reserve(request(&relay_key, &client_key, [15; 32], 1), &auth, 110)
            .unwrap();
        let other = authenticated_client(&relay_key, &client_key, [16; 32], [17; 32]);
        let mut keepalive = RelayKeepaliveV1 {
            format: 1,
            relay_node_id: principal_node_id(relay_key.verifying_key().as_bytes()),
            target_node_id: auth.client_node_id(),
            reservation_id: [15; 32],
            sequence: 2,
            issued_at: 111,
            expires_at: 130,
            target_signature: [0; 64],
        };
        keepalive.target_signature = client_key
            .sign(
                &relay_control_signing_bytes(
                    &RelayControlV1::Keepalive(keepalive.clone()),
                    RelayControlSignatureRoleV1::KeepaliveTarget,
                )
                .unwrap(),
            )
            .to_bytes();
        assert_eq!(
            store.keepalive(keepalive, &other, 112).unwrap_err(),
            ReservationError::ConnectionMismatch
        );

        let mut authenticator = OuterClientAuthenticator::new(relay_key.clone());
        authenticator
            .issue_challenge([18; 32], [19; 32], 100)
            .unwrap();
        let mut hello = RelayOuterClientHelloV1 {
            format: 1,
            relay_node_id: principal_node_id(relay_key.verifying_key().as_bytes()),
            client_node_id: principal_node_id(client_key.verifying_key().as_bytes()),
            client_public_key: *client_key.verifying_key().as_bytes(),
            challenge_nonce: [18; 32],
            outer_connection_binding: [19; 32],
            issued_at: 100,
            expires_at: 130,
            client_signature: [0; 64],
        };
        hello.client_signature = client_key
            .sign(
                &relay_control_signing_bytes(
                    &RelayControlV1::OuterClientHello(hello.clone()),
                    RelayControlSignatureRoleV1::OuterHelloClient,
                )
                .unwrap(),
            )
            .to_bytes();
        authenticator
            .authenticate(hello.clone(), [19; 32], 110)
            .unwrap();
        assert_eq!(
            authenticator
                .authenticate(hello, [19; 32], 110)
                .unwrap_err(),
            ReservationError::ChallengeMissing
        );
    }

    #[test]
    fn challenge_expiry_does_not_truncate_authenticated_outer_session() {
        let relay_key = SigningKey::from_bytes(&[21; 32]);
        let client_key = SigningKey::from_bytes(&[22; 32]);
        let binding = [23; 32];
        let nonce = [24; 32];
        let mut authenticator = OuterClientAuthenticator::new(relay_key.clone());
        let challenge = authenticator.issue_challenge(nonce, binding, 100).unwrap();
        assert_eq!(challenge.expires_at, 130);

        let requested_session_expiry = 900;
        let mut hello = RelayOuterClientHelloV1 {
            format: 1,
            relay_node_id: principal_node_id(relay_key.verifying_key().as_bytes()),
            client_node_id: principal_node_id(client_key.verifying_key().as_bytes()),
            client_public_key: *client_key.verifying_key().as_bytes(),
            challenge_nonce: nonce,
            outer_connection_binding: binding,
            issued_at: 110,
            expires_at: requested_session_expiry,
            client_signature: [0; 64],
        };
        hello.client_signature = client_key
            .sign(
                &relay_control_signing_bytes(
                    &RelayControlV1::OuterClientHello(hello.clone()),
                    RelayControlSignatureRoleV1::OuterHelloClient,
                )
                .unwrap(),
            )
            .to_bytes();

        let authenticated = authenticator.authenticate(hello, binding, 110).unwrap();
        assert_eq!(authenticated.expires_at(), requested_session_expiry);
        assert!(authenticated.expires_at() > challenge.expires_at);
    }

    #[test]
    fn authenticated_outer_session_accepts_canonical_maximum_lifetime() {
        let relay_key = SigningKey::from_bytes(&[25; 32]);
        let client_key = SigningKey::from_bytes(&[26; 32]);
        let binding = [27; 32];
        let nonce = [28; 32];
        let now = 110;
        let mut authenticator = OuterClientAuthenticator::new(relay_key.clone());
        authenticator.issue_challenge(nonce, binding, 100).unwrap();
        let mut hello = RelayOuterClientHelloV1 {
            format: 1,
            relay_node_id: principal_node_id(relay_key.verifying_key().as_bytes()),
            client_node_id: principal_node_id(client_key.verifying_key().as_bytes()),
            client_public_key: *client_key.verifying_key().as_bytes(),
            challenge_nonce: nonce,
            outer_connection_binding: binding,
            issued_at: now,
            expires_at: now + MAX_RELAY_CONTROL_VALIDITY_SECONDS,
            client_signature: [0; 64],
        };
        hello.client_signature = client_key
            .sign(
                &relay_control_signing_bytes(
                    &RelayControlV1::OuterClientHello(hello.clone()),
                    RelayControlSignatureRoleV1::OuterHelloClient,
                )
                .unwrap(),
            )
            .to_bytes();

        let authenticated = authenticator.authenticate(hello, binding, now).unwrap();
        assert_eq!(
            authenticated.expires_at(),
            now + MAX_RELAY_CONTROL_VALIDITY_SECONDS
        );
    }
}
