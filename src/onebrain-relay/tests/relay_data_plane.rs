use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signer, SigningKey, Verifier};
use ku_net::transport::{QuicTransport, TransportConfig};
use ku_net::vnext_connection_executor::{
    ConnectionPlannerExecutor, ProductionRelayAssociationClient, ProductionRelayCarrierDialer,
    RelayAssociationClient, RelayCarrierDialer,
};
use ku_net::vnext_reachability_crypto::{
    possession_challenge_digest, possession_proof_signing_bytes, InMemoryReachabilityReplayStore,
    KnownPeerIdentity, PublicEndpointResolver, ReachabilityAdmission,
    ReachabilityAdmissionPreparer, ReachabilityLockFreePreparation, ReachabilityRecordAdmission,
    RelayAdmissionError,
};
use ku_net::vnext_relay_tunnel::{
    connect_authenticated_outer, connect_authenticated_outer_on_transport,
    AlternateRelayProbeObservation, ValidatedRelayDialRoute, ValidatedRelayDialSet,
};
use ku_net::vnext_secure_session_adapter::{
    accept_expected_inbound, authenticate_expected_outbound,
};
use ku_net::vnext_session::principal_node_id as session_principal_node_id;
use onebrain_protocol::{
    connectivity_signing_bytes, decode_connectivity_signaling, decode_relay_control,
    encode_reachability_object, encode_relay_control, reachability_signing_bytes,
    reconciliation_capability, reconciliation_profile, relay_control_signing_bytes,
    ConnectivitySignalingV1, ConnectivitySignatureRoleV1, HostAddressV1, ProtocolVersionV1,
    ReachabilityEndpointV1, ReachabilityObjectV1, ReachabilitySignatureRoleV1, RelayCandidateV1,
    RelayConnectRequestV1, RelayControlSignatureRoleV1, RelayControlV1, RelayDescriptorV1,
    RelayEndpointV1, RelayPossessionProofV1, RelayReservationV1, RelayReserveRequestV1,
    RelayTransportV1, RelayWireFrameV1, RelayWireKindV1,
};
use onebrain_relay::{
    principal_node_id, relay_identity_certificate, tcp443_pinned_round_trip, udp_pinned_round_trip,
    AssociationBinding, DatagramDirectionV1, DurableRelayState, OpaqueDatagramEnvelopeV1,
    OuterConnectionLimiter, RelayDataPlane, RelayDataPlaneError, RelayGlobalBudget,
    RelayProductionService, Tcp443FrameCodec, Tcp443RelayListener, UdpRelayListener,
};

struct TestResolver;

impl PublicEndpointResolver for TestResolver {
    fn resolve(
        &self,
        host: &HostAddressV1,
        _deadline: Instant,
    ) -> Result<Vec<IpAddr>, RelayAdmissionError> {
        match host {
            HostAddressV1::Ipv4(value) => Ok(vec![IpAddr::V4(Ipv4Addr::from(*value))]),
            _ => Err(RelayAdmissionError::DnsResolutionFailed),
        }
    }
}

fn binding() -> AssociationBinding {
    AssociationBinding::new([1; 32], [2; 32], [3; 32], [4; 32], [5; 32], 200).unwrap()
}

#[test]
fn opaque_udp_round_trip_reorders_and_never_decodes_inner_bytes() {
    let payload = (0..1_350).map(|value| value as u8).collect::<Vec<_>>();
    let fragments = OpaqueDatagramEnvelopeV1::fragment(
        [1; 32],
        DatagramDirectionV1::InitiatorToTarget,
        7,
        11,
        &payload,
        240,
    )
    .unwrap();
    assert!(fragments.len() > 1 && fragments.len() <= 8);

    let budget = RelayGlobalBudget::new_for_test(16, 16_384, 8, 16_384);
    let mut relay = RelayDataPlane::new(budget);
    relay.register(binding()).unwrap();
    let mut delivered = None;
    for fragment in fragments.into_iter().rev() {
        let bytes = fragment.encode().unwrap();
        delivered = relay
            .accept_fragment([4; 32], &bytes, 110)
            .unwrap()
            .or(delivered);
    }
    let delivered = delivered.unwrap();
    assert_eq!(delivered.recipient_connection(), [5; 32]);
    assert_eq!(delivered.payload(), payload);
}

#[test]
fn wrong_connection_duplicate_oversize_timeout_and_capacity_reject() {
    let budget = RelayGlobalBudget::new_for_test(1, 1_400, 1, 1_400);
    let mut relay = RelayDataPlane::new(budget);
    relay.register(binding()).unwrap();
    let fragment = OpaqueDatagramEnvelopeV1::fragment(
        [1; 32],
        DatagramDirectionV1::InitiatorToTarget,
        1,
        1,
        &[9; 500],
        1_000,
    )
    .unwrap()
    .remove(0);
    let bytes = fragment.encode().unwrap();
    assert_eq!(
        relay.accept_fragment([8; 32], &bytes, 110).unwrap_err(),
        RelayDataPlaneError::ConnectionMismatch
    );
    relay.accept_fragment([4; 32], &bytes, 110).unwrap();
    assert_eq!(
        relay.accept_fragment([4; 32], &bytes, 110).unwrap_err(),
        RelayDataPlaneError::DuplicateFragment
    );
    assert!(OpaqueDatagramEnvelopeV1::fragment(
        [1; 32],
        DatagramDirectionV1::InitiatorToTarget,
        2,
        2,
        &[0; 1_351],
        1_000,
    )
    .is_err());
}

#[test]
fn tcp443_framing_is_bounded_and_truncation_rejects() {
    let payload = vec![0xa5; 1_350];
    let framed = Tcp443FrameCodec::encode(&payload).unwrap();
    assert_eq!(Tcp443FrameCodec::decode(&framed).unwrap(), payload);
    assert_eq!(
        Tcp443FrameCodec::decode(&framed[..framed.len() - 1]).unwrap_err(),
        RelayDataPlaneError::Truncated
    );
    assert!(Tcp443FrameCodec::encode(&vec![0; 65_537]).is_err());
}

#[test]
fn udp_and_tcp_certificate_spki_is_the_exact_descriptor_identity() {
    let signer = SigningKey::from_bytes(&[44; 32]);
    let public = *signer.verifying_key().as_bytes();
    let mut descriptor = RelayDescriptorV1 {
        format: 1,
        relay_node_id: principal_node_id(&public),
        relay_public_key: public,
        endpoints: vec![],
        supported_transports: vec![RelayTransportV1::QuicUdp, RelayTransportV1::TlsTcp443],
        protocol_versions: vec![ProtocolVersionV1 { major: 1, minor: 0 }],
        capacity_policy_digest: [45; 32],
        previous_descriptor_blake3: None,
        sequence: 1,
        issued_at: 100,
        expires_at: 130,
        relay_signature: [0; 64],
    };
    let certificate = relay_identity_certificate(&signer, &descriptor).unwrap();
    assert_eq!(certificate.spki_ed25519(), public);
    assert!(!certificate.certificate_der().is_empty());
    assert!(!certificate.private_key_der().is_empty());
    descriptor.relay_public_key[0] ^= 1;
    assert_eq!(
        relay_identity_certificate(&signer, &descriptor).unwrap_err(),
        RelayDataPlaneError::IdentityMismatch
    );
}

#[test]
fn pending_and_per_source_outer_limits_release_capacity() {
    let limiter = OuterConnectionLimiter::with_limits(1, 1, 1, 16);
    let source = "192.0.2.10".parse().unwrap();
    let pending = limiter.begin(source, 8).unwrap();
    assert_eq!(
        limiter.begin(source, 8).unwrap_err(),
        RelayDataPlaneError::Capacity
    );
    drop(pending);
    let active = limiter.begin(source, 8).unwrap().promote().unwrap();
    assert_eq!(
        limiter.begin(source, 8).unwrap_err(),
        RelayDataPlaneError::Capacity
    );
    drop(active);
    assert!(limiter.begin(source, 8).is_ok());
}

#[tokio::test]
async fn real_tls_tcp443_round_trip_pins_descriptor_spki() {
    let signer = SigningKey::from_bytes(&[66; 32]);
    let public = *signer.verifying_key().as_bytes();
    let descriptor = RelayDescriptorV1 {
        format: 1,
        relay_node_id: principal_node_id(&public),
        relay_public_key: public,
        endpoints: vec![],
        supported_transports: vec![RelayTransportV1::TlsTcp443],
        protocol_versions: vec![ProtocolVersionV1 { major: 1, minor: 0 }],
        capacity_policy_digest: [67; 32],
        previous_descriptor_blake3: None,
        sequence: 1,
        issued_at: 100,
        expires_at: 130,
        relay_signature: [0; 64],
    };
    let identity = relay_identity_certificate(&signer, &descriptor).unwrap();
    let server = Tcp443RelayListener::bind("127.0.0.1:0".parse().unwrap(), &identity)
        .await
        .unwrap();
    let address = server.local_addr().unwrap();
    let server_task = tokio::spawn(async move { server.accept_echo_once().await.unwrap() });
    assert_eq!(
        tcp443_pinned_round_trip(address, public, b"opaque-inner-quic")
            .await
            .unwrap(),
        b"opaque-inner-quic"
    );
    server_task.await.unwrap();
}

#[tokio::test]
async fn real_udp_quic_datagram_round_trip_pins_descriptor_spki() {
    let signer = SigningKey::from_bytes(&[70; 32]);
    let public = *signer.verifying_key().as_bytes();
    let descriptor = RelayDescriptorV1 {
        format: 1,
        relay_node_id: principal_node_id(&public),
        relay_public_key: public,
        endpoints: vec![],
        supported_transports: vec![RelayTransportV1::QuicUdp],
        protocol_versions: vec![ProtocolVersionV1 { major: 1, minor: 0 }],
        capacity_policy_digest: [71; 32],
        previous_descriptor_blake3: None,
        sequence: 1,
        issued_at: 100,
        expires_at: 130,
        relay_signature: [0; 64],
    };
    let identity = relay_identity_certificate(&signer, &descriptor).unwrap();
    let server = UdpRelayListener::bind("127.0.0.1:0".parse().unwrap(), &identity).unwrap();
    let address = server.local_addr().unwrap();
    let server_task = tokio::spawn(async move { server.accept_echo_once().await });
    let (client_result, server_result) = tokio::join!(
        udp_pinned_round_trip(address, public, b"opaque-outer-datagram"),
        server_task
    );
    assert_eq!(server_result.unwrap(), Ok(()));
    assert_eq!(client_result.unwrap(), b"opaque-outer-datagram");
}

#[tokio::test]
async fn idle_production_listeners_remain_pending_instead_of_reporting_expired() {
    let signer = SigningKey::from_bytes(&[75; 32]);
    let public = *signer.verifying_key().as_bytes();
    let descriptor = RelayDescriptorV1 {
        format: 1,
        relay_node_id: principal_node_id(&public),
        relay_public_key: public,
        endpoints: vec![],
        supported_transports: vec![RelayTransportV1::QuicUdp, RelayTransportV1::TlsTcp443],
        protocol_versions: vec![ProtocolVersionV1 { major: 1, minor: 0 }],
        capacity_policy_digest: [76; 32],
        previous_descriptor_blake3: None,
        sequence: 1,
        issued_at: 100,
        expires_at: 130,
        relay_signature: [0; 64],
    };
    let identity = relay_identity_certificate(&signer, &descriptor).unwrap();
    let tcp = Tcp443RelayListener::bind("127.0.0.1:0".parse().unwrap(), &identity)
        .await
        .unwrap();
    let udp = UdpRelayListener::bind("127.0.0.1:0".parse().unwrap(), &identity).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let durable =
        Arc::new(DurableRelayState::initialize(&directory.path().join("relay.redb")).unwrap());
    let service = Arc::new(RelayProductionService::new(signer, 8, 3, durable).unwrap());

    assert!(tokio::time::timeout(
        Duration::from_millis(150),
        tcp.serve_production_once(Arc::clone(&service)),
    )
    .await
    .is_err());
    assert!(tokio::time::timeout(
        Duration::from_millis(150),
        udp.serve_production_once(service),
    )
    .await
    .is_err());
}

#[tokio::test]
async fn production_udp_outer_authenticates_and_grants_a_signed_reservation() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let relay_key = SigningKey::from_bytes(&[81; 32]);
    let client_key = SigningKey::from_bytes(&[82; 32]);
    let relay_public = *relay_key.verifying_key().as_bytes();
    let mut descriptor = RelayDescriptorV1 {
        format: 1,
        relay_node_id: principal_node_id(&relay_public),
        relay_public_key: relay_public,
        endpoints: vec![RelayEndpointV1 {
            transport: RelayTransportV1::QuicUdp,
            host: HostAddressV1::Ipv4([1, 1, 1, 1]),
            port: 41000,
        }],
        supported_transports: vec![RelayTransportV1::QuicUdp],
        protocol_versions: vec![ProtocolVersionV1 { major: 1, minor: 0 }],
        capacity_policy_digest: [83; 32],
        previous_descriptor_blake3: None,
        sequence: 1,
        issued_at: now,
        expires_at: now + 600,
        relay_signature: [0; 64],
    };
    descriptor.relay_signature = relay_key
        .sign(
            &reachability_signing_bytes(
                &ReachabilityObjectV1::RelayDescriptor(descriptor.clone()),
                ReachabilitySignatureRoleV1::RelayDescriptor,
            )
            .unwrap(),
        )
        .to_bytes();
    let descriptor_bytes =
        encode_reachability_object(&ReachabilityObjectV1::RelayDescriptor(descriptor.clone()))
            .unwrap();
    let preparer = ReachabilityAdmissionPreparer::new(Arc::new(TestResolver), 1).unwrap();
    let prepared = preparer
        .prepare_descriptor(
            &descriptor_bytes,
            now,
            Instant::now() + Duration::from_secs(1),
        )
        .await
        .unwrap();
    let mut admission =
        ReachabilityAdmission::new(Arc::new(InMemoryReachabilityReplayStore::default()));
    let pending = admission
        .register_prepared_descriptor(prepared, [84; 32], now)
        .unwrap();
    let challenge = pending.challenges()[0].clone();
    let proof = RelayPossessionProofV1 {
        challenge_digest: possession_challenge_digest(&challenge),
        connection_binding_digest: [85; 32],
        signature: relay_key
            .sign(&possession_proof_signing_bytes(&challenge, [85; 32]))
            .to_bytes(),
    };
    let validated = admission
        .complete_descriptor_admission(pending, &[proof], now)
        .unwrap();

    let identity = relay_identity_certificate(&relay_key, &descriptor).unwrap();
    let listener = UdpRelayListener::bind("127.0.0.1:0".parse().unwrap(), &identity).unwrap();
    let address = listener.local_addr().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let durable =
        Arc::new(DurableRelayState::initialize(&directory.path().join("relay.redb")).unwrap());
    let service = Arc::new(RelayProductionService::new(relay_key.clone(), 8, 3, durable).unwrap());
    let server = tokio::spawn({
        let service = service.clone();
        async move { listener.serve_production_once(service).await }
    });

    let observation = AlternateRelayProbeObservation::new(
        0,
        address,
        RelayTransportV1::QuicUdp,
        relay_public,
        [86; 32],
        now,
        now + 30,
    )
    .unwrap();
    let route =
        ValidatedRelayDialRoute::alternate_from_verified_probe(&validated, observation).unwrap();
    let routes = ValidatedRelayDialSet::from_admitted_descriptor(route, None).unwrap();
    let shared_transport = QuicTransport::bind(TransportConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..TransportConfig::default()
    })
    .await
    .unwrap();
    let shared_local = shared_transport.local_addr().unwrap();
    let outer = connect_authenticated_outer_on_transport(
        &routes,
        &client_key,
        now,
        Instant::now() + Duration::from_secs(5),
        &shared_transport,
    )
    .await
    .unwrap();

    let target_node_id = principal_node_id(client_key.verifying_key().as_bytes());
    let reservation_id = [87; 32];
    let unsigned_reservation = RelayReservationV1 {
        format: 1,
        relay_node_id: descriptor.relay_node_id,
        target_node_id,
        reservation_id,
        transport_scope: vec![RelayTransportV1::QuicUdp],
        issued_at: now,
        expires_at: now + 30,
        target_signature: [0; 64],
        relay_signature: [0; 64],
    };
    let target_reservation_signature = client_key
        .sign(
            &reachability_signing_bytes(
                &ReachabilityObjectV1::RelayReservation(unsigned_reservation),
                ReachabilitySignatureRoleV1::ReservationTarget,
            )
            .unwrap(),
        )
        .to_bytes();
    let mut reserve = RelayReserveRequestV1 {
        format: 1,
        relay_node_id: descriptor.relay_node_id,
        target_node_id,
        reservation_id,
        transport_scope: vec![RelayTransportV1::QuicUdp],
        sequence: 1,
        issued_at: now,
        expires_at: now + 30,
        target_reservation_signature,
        target_request_signature: [0; 64],
    };
    reserve.target_request_signature = client_key
        .sign(
            &relay_control_signing_bytes(
                &RelayControlV1::Reserve(reserve.clone()),
                RelayControlSignatureRoleV1::ReserveRequestTarget,
            )
            .unwrap(),
        )
        .to_bytes();
    let frame = RelayWireFrameV1::new(
        RelayWireKindV1::Control,
        [88; 16],
        encode_relay_control(&RelayControlV1::Reserve(reserve)).unwrap(),
    )
    .unwrap();
    let response = outer.request_control_frame(&frame).await.unwrap();
    let grant = match decode_relay_control(response.payload()).unwrap() {
        RelayControlV1::Granted(value) => value,
        other => panic!("expected grant, got {other:?}"),
    };
    assert_eq!(grant.reservation_id, reservation_id);
    assert_eq!(grant.target_node_id, target_node_id);
    assert_ne!(grant.relay_signature, [0; 64]);
    let mut observation_payload = Vec::with_capacity(40);
    observation_payload.extend_from_slice(&reservation_id);
    observation_payload.extend_from_slice(&7_u64.to_be_bytes());
    let observation_response = outer
        .request_control_frame(
            &RelayWireFrameV1::new(
                RelayWireKindV1::ReflexiveObservation,
                [89; 16],
                observation_payload,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        observation_response.kind(),
        RelayWireKindV1::ReflexiveObservation
    );
    let ConnectivitySignalingV1::ReflexiveObservation(reflexive) =
        decode_connectivity_signaling(observation_response.payload()).unwrap()
    else {
        panic!("expected relay-signed reflexive observation")
    };
    assert_eq!(reflexive.target_node_id, target_node_id);
    assert_eq!(reflexive.relay_node_id, descriptor.relay_node_id);
    assert_eq!(reflexive.reservation_id, reservation_id);
    assert_eq!(reflexive.network_epoch, 7);
    assert!(matches!(
        reflexive.observed_endpoint.host,
        HostAddressV1::Ipv4([127, 0, 0, 1])
    ));
    assert_ne!(reflexive.observed_endpoint.port, 0);
    assert_eq!(reflexive.observed_endpoint.port, shared_local.port());
    relay_key
        .verifying_key()
        .verify(
            &connectivity_signing_bytes(
                &ConnectivitySignalingV1::ReflexiveObservation(reflexive.clone()),
                ConnectivitySignatureRoleV1::ReflexiveRelay,
            )
            .unwrap(),
            &ed25519_dalek::Signature::from_bytes(&reflexive.relay_signature),
        )
        .unwrap();
    outer.close();
    shared_transport.shutdown().await;
    server.abort();
}

#[tokio::test]
async fn production_relay_multiplexes_two_associations_on_one_outer_connection() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let relay_key = SigningKey::from_bytes(&[91; 32]);
    let initiator_key = SigningKey::from_bytes(&[92; 32]);
    let target_key = SigningKey::from_bytes(&[93; 32]);
    let third_key = SigningKey::from_bytes(&[94; 32]);
    let relay_public = *relay_key.verifying_key().as_bytes();
    let mut descriptor = RelayDescriptorV1 {
        format: 1,
        relay_node_id: principal_node_id(&relay_public),
        relay_public_key: relay_public,
        endpoints: vec![RelayEndpointV1 {
            transport: RelayTransportV1::QuicUdp,
            host: HostAddressV1::Ipv4([1, 1, 1, 1]),
            port: 41000,
        }],
        supported_transports: vec![RelayTransportV1::QuicUdp],
        protocol_versions: vec![ProtocolVersionV1 { major: 1, minor: 0 }],
        capacity_policy_digest: [94; 32],
        previous_descriptor_blake3: None,
        sequence: 1,
        issued_at: now,
        expires_at: now + 600,
        relay_signature: [0; 64],
    };
    descriptor.relay_signature = relay_key
        .sign(
            &reachability_signing_bytes(
                &ReachabilityObjectV1::RelayDescriptor(descriptor.clone()),
                ReachabilitySignatureRoleV1::RelayDescriptor,
            )
            .unwrap(),
        )
        .to_bytes();
    let descriptor_bytes =
        encode_reachability_object(&ReachabilityObjectV1::RelayDescriptor(descriptor.clone()))
            .unwrap();
    let preparer = ReachabilityAdmissionPreparer::new(Arc::new(TestResolver), 1).unwrap();
    let prepared = preparer
        .prepare_descriptor(
            &descriptor_bytes,
            now,
            Instant::now() + Duration::from_secs(1),
        )
        .await
        .unwrap();
    let mut admission =
        ReachabilityAdmission::new(Arc::new(InMemoryReachabilityReplayStore::default()));
    let pending = admission
        .register_prepared_descriptor(prepared, [95; 32], now)
        .unwrap();
    let proofs = pending
        .challenges()
        .iter()
        .map(|challenge| RelayPossessionProofV1 {
            challenge_digest: possession_challenge_digest(challenge),
            connection_binding_digest: [96; 32],
            signature: relay_key
                .sign(&possession_proof_signing_bytes(challenge, [96; 32]))
                .to_bytes(),
        })
        .collect::<Vec<_>>();
    let validated = admission
        .complete_descriptor_admission(pending, &proofs, now)
        .unwrap();

    let identity = relay_identity_certificate(&relay_key, &descriptor).unwrap();
    let listener =
        Arc::new(UdpRelayListener::bind("127.0.0.1:0".parse().unwrap(), &identity).unwrap());
    let address = listener.local_addr().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let durable =
        Arc::new(DurableRelayState::initialize(&directory.path().join("relay.redb")).unwrap());
    let service = Arc::new(RelayProductionService::new(relay_key.clone(), 8, 3, durable).unwrap());
    let server_a = tokio::spawn({
        let listener = listener.clone();
        let service = service.clone();
        async move { listener.serve_production_once(service).await }
    });
    let server_b = tokio::spawn({
        let listener = listener.clone();
        let service = service.clone();
        async move { listener.serve_production_once(service).await }
    });
    let server_c = tokio::spawn({
        let listener = listener.clone();
        let service = service.clone();
        async move { listener.serve_production_once(service).await }
    });
    let route = ValidatedRelayDialRoute::alternate_from_verified_probe(
        &validated,
        AlternateRelayProbeObservation::new(
            0,
            address,
            RelayTransportV1::QuicUdp,
            relay_public,
            [97; 32],
            now,
            now + 30,
        )
        .unwrap(),
    )
    .unwrap();
    let routes = ValidatedRelayDialSet::from_admitted_descriptor(route, None).unwrap();
    let (initiator, target, third) = tokio::join!(
        connect_authenticated_outer(
            &routes,
            &initiator_key,
            now,
            Instant::now() + Duration::from_secs(5)
        ),
        connect_authenticated_outer(
            &routes,
            &target_key,
            now,
            Instant::now() + Duration::from_secs(5)
        ),
        connect_authenticated_outer(
            &routes,
            &third_key,
            now,
            Instant::now() + Duration::from_secs(5)
        ),
    );
    let initiator = Arc::new(initiator.unwrap());
    let target = Arc::new(target.unwrap());
    let third = Arc::new(third.unwrap());
    let initiator_node = principal_node_id(initiator_key.verifying_key().as_bytes());
    let target_node = principal_node_id(target_key.verifying_key().as_bytes());
    let third_node = principal_node_id(third_key.verifying_key().as_bytes());
    let initiator_reservation = [98; 32];
    let target_reservation = [99; 32];
    let third_reservation = [103; 32];
    let initiator_grant = reserve_for_test(
        &initiator,
        &initiator_key,
        descriptor.relay_node_id,
        initiator_node,
        initiator_reservation,
        now,
    )
    .await;
    let target_grant = reserve_for_test(
        &target,
        &target_key,
        descriptor.relay_node_id,
        target_node,
        target_reservation,
        now,
    )
    .await;
    let third_grant = reserve_for_test(
        &third,
        &third_key,
        descriptor.relay_node_id,
        third_node,
        third_reservation,
        now,
    )
    .await;
    let relay_identity = KnownPeerIdentity {
        node_id: descriptor.relay_node_id,
        public_key: relay_public,
    };
    let initiator_identity =
        KnownPeerIdentity::from_public_key(*initiator_key.verifying_key().as_bytes());
    let target_identity =
        KnownPeerIdentity::from_public_key(*target_key.verifying_key().as_bytes());
    let third_identity = KnownPeerIdentity::from_public_key(*third_key.verifying_key().as_bytes());
    let mut reservation_admission =
        ReachabilityAdmission::new(Arc::new(InMemoryReachabilityReplayStore::default()));
    let initiator_reservation_admitted = reservation_admission
        .admit_reservation(
            &encode_reachability_object(&ReachabilityObjectV1::RelayReservation(initiator_grant))
                .unwrap(),
            &initiator_identity,
            &relay_identity,
            now,
        )
        .unwrap();
    let target_reservation_admitted = reservation_admission
        .admit_reservation(
            &encode_reachability_object(&ReachabilityObjectV1::RelayReservation(target_grant))
                .unwrap(),
            &target_identity,
            &relay_identity,
            now,
        )
        .unwrap();
    let third_reservation_admitted = reservation_admission
        .admit_reservation(
            &encode_reachability_object(&ReachabilityObjectV1::RelayReservation(third_grant))
                .unwrap(),
            &third_identity,
            &relay_identity,
            now,
        )
        .unwrap();

    let mut connect = RelayConnectRequestV1 {
        format: 1,
        initiator_node_id: initiator_node,
        target_node_id: target_node,
        initiator_reservation_id: initiator_reservation,
        target_reservation_id: target_reservation,
        nonce: [100; 32],
        sequence: 1,
        issued_at: now,
        expires_at: now + 30,
        initiator_signature: [0; 64],
    };
    connect.initiator_signature = initiator_key
        .sign(
            &connectivity_signing_bytes(
                &ConnectivitySignalingV1::RelayConnectRequest(connect.clone()),
                ConnectivitySignatureRoleV1::RelayConnectInitiator,
            )
            .unwrap(),
        )
        .to_bytes();
    let association_client =
        ProductionRelayAssociationClient::new(*initiator_key.verifying_key().as_bytes());
    let target_association_client =
        ProductionRelayAssociationClient::new(*target_key.verifying_key().as_bytes());
    let third_association_client =
        ProductionRelayAssociationClient::new(*third_key.verifying_key().as_bytes());
    let deadline = Instant::now() + Duration::from_secs(5);
    let (initiator_association, target_association) = tokio::join!(
        association_client.associate(
            &connect,
            &initiator_reservation_admitted,
            &target_reservation_admitted,
            Arc::clone(&initiator),
            deadline,
        ),
        target_association_client.accept_inbound(
            &initiator_reservation_admitted,
            &target_reservation_admitted,
            initiator_identity.clone(),
            Arc::clone(&target),
            deadline,
        ),
    );
    let initiator_association = initiator_association.unwrap();
    let target_association = target_association.unwrap();
    assert_eq!(initiator_association, target_association);
    let association_ab = initiator_association.canonical();

    let mut connect_bc = RelayConnectRequestV1 {
        format: 1,
        initiator_node_id: target_node,
        target_node_id: third_node,
        initiator_reservation_id: target_reservation,
        target_reservation_id: third_reservation,
        nonce: [104; 32],
        sequence: 1,
        issued_at: now,
        expires_at: now + 30,
        initiator_signature: [0; 64],
    };
    connect_bc.initiator_signature = target_key
        .sign(
            &connectivity_signing_bytes(
                &ConnectivitySignalingV1::RelayConnectRequest(connect_bc.clone()),
                ConnectivitySignatureRoleV1::RelayConnectInitiator,
            )
            .unwrap(),
        )
        .to_bytes();
    let deadline = Instant::now() + Duration::from_secs(5);
    let (target_initiated_association, third_association) = tokio::join!(
        target_association_client.associate(
            &connect_bc,
            &target_reservation_admitted,
            &third_reservation_admitted,
            Arc::clone(&target),
            deadline,
        ),
        third_association_client.accept_inbound(
            &target_reservation_admitted,
            &third_reservation_admitted,
            target_identity,
            Arc::clone(&third),
            deadline,
        ),
    );
    let target_initiated_association = target_initiated_association.unwrap();
    let third_association = third_association.unwrap();
    assert_eq!(target_initiated_association, third_association);
    let association_bc = target_initiated_association.canonical();

    let fragment_ab = OpaqueDatagramEnvelopeV1::fragment(
        association_ab.association_id,
        DatagramDirectionV1::InitiatorToTarget,
        1,
        1,
        b"inner-obp-packet-ab",
        1200,
    )
    .unwrap()
    .remove(0)
    .encode()
    .unwrap();
    let fragment_cb = OpaqueDatagramEnvelopeV1::fragment(
        association_bc.association_id,
        DatagramDirectionV1::TargetToInitiator,
        1,
        1,
        b"inner-obp-packet-cb",
        1200,
    )
    .unwrap()
    .remove(0)
    .encode()
    .unwrap();
    let (send_ab, send_cb) = tokio::join!(
        initiator.send_opaque_datagram(fragment_ab),
        third.send_opaque_datagram(fragment_cb),
    );
    send_ab.unwrap();
    send_cb.unwrap();
    let (received_ab, received_cb) = tokio::join!(
        target.receive_opaque_for(association_ab.association_id),
        target.receive_opaque_for(association_bc.association_id),
    );
    assert_eq!(
        received_ab.unwrap(),
        b"inner-obp-packet-ab",
        "shared target outer must retain the A-to-B association identity"
    );
    assert_eq!(
        received_cb.unwrap(),
        b"inner-obp-packet-cb",
        "shared target outer must retain the C-to-B association identity"
    );

    let carrier = ProductionRelayCarrierDialer::standard();
    let deadline = Instant::now() + Duration::from_secs(10);
    let (initiator_inner, target_inner_ab, target_inner_bc, third_inner) = tokio::join!(
        carrier.dial(
            &validated,
            &initiator_association,
            Arc::clone(&initiator),
            deadline
        ),
        carrier.dial(
            &validated,
            &initiator_association,
            Arc::clone(&target),
            deadline
        ),
        carrier.dial(
            &validated,
            &target_initiated_association,
            Arc::clone(&target),
            deadline
        ),
        carrier.dial(
            &validated,
            &target_initiated_association,
            Arc::clone(&third),
            deadline
        ),
    );
    assert!(
        initiator_inner.is_ok()
            && target_inner_ab.is_ok()
            && target_inner_bc.is_ok()
            && third_inner.is_ok(),
        "inner carriers: a={:?} b_ab={:?} b_bc={:?} c={:?}",
        initiator_inner.as_ref().err(),
        target_inner_ab.as_ref().err(),
        target_inner_bc.as_ref().err(),
        third_inner.as_ref().err(),
    );
    let initiator_inner = initiator_inner.unwrap();
    let target_inner = target_inner_ab.unwrap();
    let relay_candidate = RelayCandidateV1 {
        relay_node_id: descriptor.relay_node_id,
        reservation_id: initiator_reservation,
        transport: RelayTransportV1::QuicUdp,
        endpoint: ReachabilityEndpointV1 {
            host: descriptor.endpoints[0].host.clone(),
            port: descriptor.endpoints[0].port,
        },
        priority: 100,
        expires_at: association_ab.expires_at,
    };
    let outbound = ConnectionPlannerExecutor::seal_validated_relay(
        relay_candidate.clone(),
        initiator_association.clone(),
        &initiator,
        initiator_inner,
        Vec::new(),
    )
    .unwrap();
    let inbound = ConnectionPlannerExecutor::expect_inbound(
        ConnectionPlannerExecutor::seal_validated_relay(
            relay_candidate,
            initiator_association,
            &target,
            target_inner,
            Vec::new(),
        )
        .unwrap(),
        initiator_node,
    )
    .unwrap();
    let profiles = [reconciliation_profile()];
    let capabilities = [reconciliation_capability()];
    let expected_target = session_principal_node_id(target_key.verifying_key().as_bytes());
    let (accepted, authenticated) = tokio::join!(
        accept_expected_inbound(
            inbound,
            &target_key,
            [102; 32],
            &profiles,
            &capabilities,
            Vec::new(),
        ),
        authenticate_expected_outbound(
            outbound,
            expected_target,
            &initiator_key,
            [101; 32],
            &profiles,
            &capabilities,
            Vec::new(),
        ),
    );
    assert!(
        accepted.is_ok() && authenticated.is_ok(),
        "relay OBP auth: accepted={:?} authenticated={:?}",
        accepted.as_ref().err(),
        authenticated.as_ref().err(),
    );
    let accepted = accepted.unwrap();
    let authenticated = authenticated.unwrap();
    assert_eq!(accepted.session(), authenticated.session());
    assert_eq!(
        authenticated.selection().path_kind(),
        onebrain_protocol::RoutePathKindV1::RelayUdp
    );
    initiator.close();
    target.close();
    third.close();
    server_a.abort();
    server_b.abort();
    server_c.abort();
}

async fn reserve_for_test(
    outer: &ku_net::vnext_relay_tunnel::AuthenticatedOuterRelayConnection,
    key: &SigningKey,
    relay_node_id: ku_core::foundation::NodeId,
    target_node_id: ku_core::foundation::NodeId,
    reservation_id: [u8; 32],
    now: u64,
) -> RelayReservationV1 {
    let unsigned = RelayReservationV1 {
        format: 1,
        relay_node_id,
        target_node_id,
        reservation_id,
        transport_scope: vec![RelayTransportV1::QuicUdp],
        issued_at: now,
        expires_at: now + 30,
        target_signature: [0; 64],
        relay_signature: [0; 64],
    };
    let target_reservation_signature = key
        .sign(
            &reachability_signing_bytes(
                &ReachabilityObjectV1::RelayReservation(unsigned),
                ReachabilitySignatureRoleV1::ReservationTarget,
            )
            .unwrap(),
        )
        .to_bytes();
    let mut reserve = RelayReserveRequestV1 {
        format: 1,
        relay_node_id,
        target_node_id,
        reservation_id,
        transport_scope: vec![RelayTransportV1::QuicUdp],
        sequence: 1,
        issued_at: now,
        expires_at: now + 30,
        target_reservation_signature,
        target_request_signature: [0; 64],
    };
    reserve.target_request_signature = key
        .sign(
            &relay_control_signing_bytes(
                &RelayControlV1::Reserve(reserve.clone()),
                RelayControlSignatureRoleV1::ReserveRequestTarget,
            )
            .unwrap(),
        )
        .to_bytes();
    let response = outer
        .request_control_frame(
            &RelayWireFrameV1::new(
                RelayWireKindV1::Control,
                reservation_id[..16].try_into().unwrap(),
                encode_relay_control(&RelayControlV1::Reserve(reserve)).unwrap(),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    match decode_relay_control(response.payload()).unwrap() {
        RelayControlV1::Granted(value) => value,
        other => panic!("expected reservation grant, got {other:?}"),
    }
}
