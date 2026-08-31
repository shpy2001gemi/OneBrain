use std::sync::Arc;

use ed25519_dalek::{Signer, SigningKey};
use ku_net::vnext_connectivity_signaling::{
    ConnectivitySignalingError, ConnectivitySignalingValidator,
};
use ku_net::vnext_reachability_crypto::{
    InMemoryReachabilityReplayStore, KnownPeerIdentity, ReachabilityAdmission,
    ReachabilityRecordAdmission, ValidatedRelayReservation,
};
use ku_net::vnext_session::{principal_node_id, AuthenticatedSession};
use onebrain_protocol::{
    connectivity_signing_bytes, encode_connectivity_signaling, encode_reachability_object,
    reachability_signing_bytes, ConnectivitySignalingV1, ConnectivitySignatureRoleV1,
    HolePunchScheduleV1, HostAddressV1, PrivateCandidateSignalV1, PrivateCandidateV1,
    ReachabilityEndpointV1, ReachabilityObjectV1, ReachabilitySignatureRoleV1,
    ReflexiveObservationV1, RelayAssociationV1, RelayConnectRequestV1, RelayReservationV1,
    RelayTransportV1, SessionProfile,
};

fn signed_reflexive(
    relay_key: &SigningKey,
    target: ku_core::foundation::NodeId,
    reservation_id: [u8; 32],
    sequence: u64,
    endpoint: [u8; 4],
) -> Vec<u8> {
    let mut value = ReflexiveObservationV1 {
        format: 1,
        relay_node_id: principal_node_id(relay_key.verifying_key().as_bytes()),
        target_node_id: target,
        reservation_id,
        observed_endpoint: ReachabilityEndpointV1 {
            host: HostAddressV1::Ipv4(endpoint),
            port: 41_000,
        },
        network_epoch: 1,
        sequence,
        issued_at: 100,
        expires_at: 130,
        relay_signature: [0; 64],
    };
    let root = ConnectivitySignalingV1::ReflexiveObservation(value.clone());
    value.relay_signature = relay_key
        .sign(
            &connectivity_signing_bytes(&root, ConnectivitySignatureRoleV1::ReflexiveRelay)
                .unwrap(),
        )
        .to_bytes();
    encode_connectivity_signaling(&ConnectivitySignalingV1::ReflexiveObservation(value)).unwrap()
}

fn signed_reservation(
    target_key: &SigningKey,
    relay_key: &SigningKey,
    reservation_id: [u8; 32],
    now: u64,
) -> Vec<u8> {
    let mut value = RelayReservationV1 {
        format: 1,
        relay_node_id: principal_node_id(relay_key.verifying_key().as_bytes()),
        target_node_id: principal_node_id(target_key.verifying_key().as_bytes()),
        reservation_id,
        transport_scope: vec![RelayTransportV1::QuicUdp],
        issued_at: now,
        expires_at: now + 900,
        target_signature: [0; 64],
        relay_signature: [0; 64],
    };
    value.target_signature = target_key
        .sign(
            &reachability_signing_bytes(
                &ReachabilityObjectV1::RelayReservation(value.clone()),
                ReachabilitySignatureRoleV1::ReservationTarget,
            )
            .unwrap(),
        )
        .to_bytes();
    value.relay_signature = relay_key
        .sign(
            &reachability_signing_bytes(
                &ReachabilityObjectV1::RelayReservation(value.clone()),
                ReachabilitySignatureRoleV1::ReservationRelay,
            )
            .unwrap(),
        )
        .to_bytes();
    encode_reachability_object(&ReachabilityObjectV1::RelayReservation(value)).unwrap()
}

fn admit_reservation(
    admission: &mut ReachabilityAdmission,
    bytes: &[u8],
    target_key: &SigningKey,
    relay_key: &SigningKey,
    now: u64,
) -> ValidatedRelayReservation {
    admission
        .admit_reservation(
            bytes,
            &KnownPeerIdentity::from_public_key(*target_key.verifying_key().as_bytes()),
            &KnownPeerIdentity::from_public_key(*relay_key.verifying_key().as_bytes()),
            now,
        )
        .unwrap()
}

fn signed_connect(
    initiator_key: &SigningKey,
    target: ku_core::foundation::NodeId,
    initiator_reservation_id: [u8; 32],
    target_reservation_id: [u8; 32],
    nonce: [u8; 32],
    sequence: u64,
) -> Vec<u8> {
    let mut value = RelayConnectRequestV1 {
        format: 1,
        initiator_node_id: principal_node_id(initiator_key.verifying_key().as_bytes()),
        target_node_id: target,
        initiator_reservation_id,
        target_reservation_id,
        nonce,
        sequence,
        issued_at: 100,
        expires_at: 130,
        initiator_signature: [0; 64],
    };
    let root = ConnectivitySignalingV1::RelayConnectRequest(value.clone());
    value.initiator_signature = initiator_key
        .sign(
            &connectivity_signing_bytes(&root, ConnectivitySignatureRoleV1::RelayConnectInitiator)
                .unwrap(),
        )
        .to_bytes();
    encode_connectivity_signaling(&ConnectivitySignalingV1::RelayConnectRequest(value)).unwrap()
}

fn signed_association(
    relay_key: &SigningKey,
    initiator: ku_core::foundation::NodeId,
    target: ku_core::foundation::NodeId,
    initiator_reservation_id: [u8; 32],
    target_reservation_id: [u8; 32],
) -> Vec<u8> {
    let mut value = RelayAssociationV1 {
        format: 1,
        relay_node_id: principal_node_id(relay_key.verifying_key().as_bytes()),
        initiator_node_id: initiator,
        target_node_id: target,
        initiator_reservation_id,
        target_reservation_id,
        association_id: [31; 32],
        issued_at: 100,
        expires_at: 130,
        relay_signature: [0; 64],
    };
    let root = ConnectivitySignalingV1::RelayAssociation(value.clone());
    value.relay_signature = relay_key
        .sign(
            &connectivity_signing_bytes(&root, ConnectivitySignatureRoleV1::RelayAssociationRelay)
                .unwrap(),
        )
        .to_bytes();
    encode_connectivity_signaling(&ConnectivitySignalingV1::RelayAssociation(value)).unwrap()
}

fn signed_schedule(
    relay_key: &SigningKey,
    association: &ku_net::vnext_connectivity_signaling::ValidatedRelayAssociation,
    token: [u8; 32],
) -> Vec<u8> {
    let admitted = association.canonical();
    let mut value = HolePunchScheduleV1 {
        format: 1,
        relay_node_id: admitted.relay_node_id,
        initiator_node_id: admitted.initiator_node_id,
        responder_node_id: admitted.target_node_id,
        initiator_reservation_id: admitted.initiator_reservation_id,
        responder_reservation_id: admitted.target_reservation_id,
        rendezvous_token: token,
        association_barrier_digest: association.digest(),
        start_delay_ms: 500,
        interval_ms: 200,
        attempt_count: 10,
        expires_at: 130,
        relay_signature: [0; 64],
    };
    let root = ConnectivitySignalingV1::HolePunchSchedule(value.clone());
    value.relay_signature = relay_key
        .sign(
            &connectivity_signing_bytes(&root, ConnectivitySignatureRoleV1::HolePunchRelay)
                .unwrap(),
        )
        .to_bytes();
    encode_connectivity_signaling(&ConnectivitySignalingV1::HolePunchSchedule(value)).unwrap()
}

fn signed_private(
    key: &SigningKey,
    target: ku_core::foundation::NodeId,
    session_id: [u8; 32],
    sequence: u64,
) -> Vec<u8> {
    let mut value = PrivateCandidateSignalV1 {
        format: 1,
        sender_node_id: principal_node_id(key.verifying_key().as_bytes()),
        target_node_id: target,
        session_id,
        network_epoch: 7,
        candidates: vec![PrivateCandidateV1 {
            endpoint: ReachabilityEndpointV1 {
                host: HostAddressV1::Ipv4([10, 1, 2, 3]),
                port: 41_000,
            },
            priority: 1,
            foundation: [4; 16],
        }],
        sequence,
        issued_at: 100,
        expires_at: 130,
        sender_signature: [0; 64],
    };
    let root = ConnectivitySignalingV1::PrivateCandidateSignal(value.clone());
    value.sender_signature = key
        .sign(
            &connectivity_signing_bytes(&root, ConnectivitySignatureRoleV1::PrivateCandidateSender)
                .unwrap(),
        )
        .to_bytes();
    encode_connectivity_signaling(&ConnectivitySignalingV1::PrivateCandidateSignal(value)).unwrap()
}

#[test]
fn private_candidates_require_the_exact_authenticated_session_and_replay_rejects() {
    let sender_key = SigningKey::from_bytes(&[1; 32]);
    let target_key = SigningKey::from_bytes(&[2; 32]);
    let sender = KnownPeerIdentity::from_public_key(*sender_key.verifying_key().as_bytes());
    let target = principal_node_id(target_key.verifying_key().as_bytes());
    let session = AuthenticatedSession {
        session_id: [3; 32],
        transport_binding: [4; 32],
        initiator: sender.node_id,
        responder: target,
        profile: SessionProfile {
            family: 1,
            major: 1,
            minor: 0,
        },
        capabilities: vec![],
        feed_evidence: vec![],
    };
    let bytes = signed_private(&sender_key, target, session.session_id, 1);
    let validator =
        ConnectivitySignalingValidator::new(Arc::new(InMemoryReachabilityReplayStore::default()));
    assert_eq!(
        validator
            .validate_private_candidate_signal(&bytes, &sender, target, None, 110)
            .unwrap_err(),
        ConnectivitySignalingError::PeerNotAuthenticated
    );
    let admitted = validator
        .validate_private_candidate_signal(&bytes, &sender, target, Some(&session), 110)
        .unwrap();
    assert_eq!(admitted.authenticated_session_id(), session.session_id);
    assert_eq!(
        validator
            .validate_private_candidate_signal(&bytes, &sender, target, Some(&session), 110)
            .unwrap_err(),
        ConnectivitySignalingError::Replay
    );
}

#[test]
fn reflexive_observation_is_public_target_bound_fresh_and_replay_safe() {
    let target_key = SigningKey::from_bytes(&[5; 32]);
    let relay_key = SigningKey::from_bytes(&[6; 32]);
    let target = principal_node_id(target_key.verifying_key().as_bytes());
    let relay = KnownPeerIdentity::from_public_key(*relay_key.verifying_key().as_bytes());
    let replay = Arc::new(InMemoryReachabilityReplayStore::default());
    let mut admission = ReachabilityAdmission::new(replay.clone());
    let reservation = admit_reservation(
        &mut admission,
        &signed_reservation(&target_key, &relay_key, [7; 32], 100),
        &target_key,
        &relay_key,
        110,
    );
    let validator = ConnectivitySignalingValidator::new(replay);
    assert_eq!(
        validator
            .validate_reflexive_observation(
                &signed_reflexive(&relay_key, target, [7; 32], 1, [10, 0, 0, 1]),
                &relay,
                target,
                &reservation,
                110,
            )
            .unwrap_err(),
        ConnectivitySignalingError::PublicEndpointInvalid
    );
    let bytes = signed_reflexive(&relay_key, target, [7; 32], 1, [1, 1, 1, 1]);
    validator
        .validate_reflexive_observation(&bytes, &relay, target, &reservation, 110)
        .unwrap();
    assert_eq!(
        validator
            .validate_reflexive_observation(&bytes, &relay, target, &reservation, 110)
            .unwrap_err(),
        ConnectivitySignalingError::Replay
    );
    let expired =
        ConnectivitySignalingValidator::new(Arc::new(InMemoryReachabilityReplayStore::default()));
    assert_eq!(
        expired
            .validate_reflexive_observation(&bytes, &relay, target, &reservation, 200)
            .unwrap_err(),
        ConnectivitySignalingError::Expired
    );
}

#[test]
fn association_requires_the_exact_dual_signed_reservation_pair_and_bound_datagrams() {
    let initiator_key = SigningKey::from_bytes(&[11; 32]);
    let target_key = SigningKey::from_bytes(&[12; 32]);
    let relay_key = SigningKey::from_bytes(&[13; 32]);
    let replay = Arc::new(InMemoryReachabilityReplayStore::default());
    let mut admission = ReachabilityAdmission::new(replay.clone());
    let initiator_reservation = admit_reservation(
        &mut admission,
        &signed_reservation(&initiator_key, &relay_key, [21; 32], 100),
        &initiator_key,
        &relay_key,
        110,
    );
    let target_reservation = admit_reservation(
        &mut admission,
        &signed_reservation(&target_key, &relay_key, [22; 32], 100),
        &target_key,
        &relay_key,
        110,
    );
    let initiator = KnownPeerIdentity::from_public_key(*initiator_key.verifying_key().as_bytes());
    let target = principal_node_id(target_key.verifying_key().as_bytes());
    let relay = KnownPeerIdentity::from_public_key(*relay_key.verifying_key().as_bytes());
    let validator = ConnectivitySignalingValidator::new(replay);
    let request_bytes = signed_connect(&initiator_key, target, [21; 32], [22; 32], [23; 32], 1);
    let request = validator
        .validate_connect_request(
            &request_bytes,
            &initiator,
            target,
            &initiator_reservation,
            &target_reservation,
            110,
        )
        .unwrap();
    assert_eq!(
        validator
            .validate_connect_request(
                &request_bytes,
                &initiator,
                target,
                &initiator_reservation,
                &target_reservation,
                110,
            )
            .unwrap_err(),
        ConnectivitySignalingError::Replay
    );
    let association = validator
        .validate_association(
            &signed_association(&relay_key, initiator.node_id, target, [21; 32], [22; 32]),
            &relay,
            &request,
            &initiator_reservation,
            &target_reservation,
            110,
        )
        .unwrap();
    association
        .validate_datagram_binding([31; 32], [21; 32], [22; 32])
        .unwrap();
    assert_eq!(
        association
            .validate_datagram_binding([31; 32], [21; 32], [99; 32])
            .unwrap_err(),
        ConnectivitySignalingError::AssociationMismatch
    );
}

#[test]
fn punch_schedule_is_one_use_deadline_bound_and_monotonic() {
    let initiator_key = SigningKey::from_bytes(&[41; 32]);
    let target_key = SigningKey::from_bytes(&[42; 32]);
    let relay_key = SigningKey::from_bytes(&[43; 32]);
    let replay = Arc::new(InMemoryReachabilityReplayStore::default());
    let mut admission = ReachabilityAdmission::new(replay.clone());
    let initiator_reservation = admit_reservation(
        &mut admission,
        &signed_reservation(&initiator_key, &relay_key, [51; 32], 100),
        &initiator_key,
        &relay_key,
        110,
    );
    let target_reservation = admit_reservation(
        &mut admission,
        &signed_reservation(&target_key, &relay_key, [52; 32], 100),
        &target_key,
        &relay_key,
        110,
    );
    let initiator = KnownPeerIdentity::from_public_key(*initiator_key.verifying_key().as_bytes());
    let target = principal_node_id(target_key.verifying_key().as_bytes());
    let relay = KnownPeerIdentity::from_public_key(*relay_key.verifying_key().as_bytes());
    let validator = ConnectivitySignalingValidator::new(replay);
    let request = validator
        .validate_connect_request(
            &signed_connect(&initiator_key, target, [51; 32], [52; 32], [53; 32], 1),
            &initiator,
            target,
            &initiator_reservation,
            &target_reservation,
            110,
        )
        .unwrap();
    let association = validator
        .validate_association(
            &signed_association(&relay_key, initiator.node_id, target, [51; 32], [52; 32]),
            &relay,
            &request,
            &initiator_reservation,
            &target_reservation,
            110,
        )
        .unwrap();
    let bytes = signed_schedule(&relay_key, &association, [54; 32]);
    assert_eq!(
        validator
            .validate_hole_punch_schedule(&bytes, &relay, &association, 110, 112_299)
            .unwrap_err(),
        ConnectivitySignalingError::RouteDeadlineExceeded
    );
    let schedule = validator
        .validate_hole_punch_schedule(&bytes, &relay, &association, 110, 112_300)
        .unwrap();
    let attempts = schedule.coordinated_attempts(1_000, 1_199, 3_499).unwrap();
    assert_eq!(attempts.len(), 10);
    assert!(attempts.windows(2).all(|pair| pair[1] - pair[0] == 200));
    assert_eq!(
        schedule
            .coordinated_attempts(1_000, 1_200, 3_500)
            .unwrap_err(),
        ConnectivitySignalingError::AsymmetricDelivery
    );
    assert_eq!(
        validator
            .validate_hole_punch_schedule(&bytes, &relay, &association, 110, 112_300)
            .unwrap_err(),
        ConnectivitySignalingError::Replay
    );
}
