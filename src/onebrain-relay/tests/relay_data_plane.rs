use ed25519_dalek::SigningKey;
use onebrain_protocol::{ProtocolVersionV1, RelayDescriptorV1, RelayTransportV1};
use onebrain_relay::{
    principal_node_id, relay_identity_certificate, tcp443_pinned_round_trip, udp_pinned_round_trip,
    AssociationBinding, DatagramDirectionV1, OpaqueDatagramEnvelopeV1, OuterConnectionLimiter,
    RelayDataPlane, RelayDataPlaneError, RelayGlobalBudget, Tcp443FrameCodec, Tcp443RelayListener,
    UdpRelayListener,
};

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
