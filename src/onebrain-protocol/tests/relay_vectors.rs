use ku_core::foundation::NodeId;
use onebrain_protocol::{
    decode_relay_control, encode_relay_control, relay_control_signing_bytes,
    RelayControlSignatureRoleV1, RelayControlV1, RelayDenialCodeV1, RelayDenialV1,
    RelayKeepaliveV1, RelayOuterClientChallengeV1, RelayOuterClientHelloV1,
    RelayPossessionChallengeV1, RelayPossessionProofV1, RelayReservationV1, RelayReserveRequestV1,
    RelayRevocationActorV1, RelayRevocationReasonV1, RelayRevokeV1, RelayTransportV1,
};

fn node(byte: u8) -> NodeId {
    NodeId::from_bytes([byte; 32])
}

fn controls() -> Vec<RelayControlV1> {
    vec![
        RelayControlV1::Reserve(RelayReserveRequestV1 {
            format: 1,
            relay_node_id: node(1),
            target_node_id: node(2),
            reservation_id: [3; 32],
            transport_scope: vec![RelayTransportV1::QuicUdp, RelayTransportV1::TlsTcp443],
            sequence: 1,
            issued_at: 100,
            expires_at: 130,
            target_reservation_signature: [4; 64],
            target_request_signature: [5; 64],
        }),
        RelayControlV1::Granted(RelayReservationV1 {
            format: 1,
            relay_node_id: node(1),
            target_node_id: node(2),
            reservation_id: [3; 32],
            transport_scope: vec![RelayTransportV1::QuicUdp],
            issued_at: 100,
            expires_at: 130,
            target_signature: [4; 64],
            relay_signature: [6; 64],
        }),
        RelayControlV1::Keepalive(RelayKeepaliveV1 {
            format: 1,
            relay_node_id: node(1),
            target_node_id: node(2),
            reservation_id: [3; 32],
            sequence: 2,
            issued_at: 110,
            expires_at: 140,
            target_signature: [7; 64],
        }),
        RelayControlV1::Revoke(RelayRevokeV1 {
            format: 1,
            relay_node_id: node(1),
            target_node_id: node(2),
            reservation_id: [3; 32],
            actor: RelayRevocationActorV1::Target,
            reason: RelayRevocationReasonV1::TargetClosed,
            sequence: 3,
            issued_at: 120,
            expires_at: 150,
            actor_signature: [8; 64],
        }),
        RelayControlV1::PossessionChallenge(RelayPossessionChallengeV1 {
            relay_node_id: node(1),
            descriptor_digest: [9; 32],
            endpoint_index: 0,
            transport: RelayTransportV1::QuicUdp,
            verifier_context: [10; 32],
            nonce: [11; 32],
            issued_at: 100,
            expires_at: 130,
        }),
        RelayControlV1::PossessionProof(RelayPossessionProofV1 {
            challenge_digest: [12; 32],
            connection_binding_digest: [13; 32],
            signature: [14; 64],
        }),
        RelayControlV1::Denied(RelayDenialV1 {
            format: 1,
            relay_node_id: node(1),
            target_node_id: node(2),
            reservation_id: [3; 32],
            code: RelayDenialCodeV1::Capacity,
            retry_after: 30,
            issued_at: 100,
            expires_at: 130,
            relay_signature: [15; 64],
        }),
        RelayControlV1::OuterClientChallenge(RelayOuterClientChallengeV1 {
            format: 1,
            relay_node_id: node(1),
            challenge_nonce: [16; 32],
            outer_connection_binding: [17; 32],
            issued_at: 100,
            expires_at: 130,
            relay_signature: [18; 64],
        }),
        RelayControlV1::OuterClientHello(RelayOuterClientHelloV1 {
            format: 1,
            relay_node_id: node(1),
            client_node_id: node(2),
            client_public_key: [19; 32],
            challenge_nonce: [16; 32],
            outer_connection_binding: [17; 32],
            issued_at: 100,
            expires_at: 130,
            client_signature: [20; 64],
        }),
    ]
}

#[test]
fn all_nine_relay_control_schemas_round_trip_canonically() {
    for value in controls() {
        let bytes = encode_relay_control(&value).unwrap();
        let decoded = decode_relay_control(&bytes).unwrap();
        assert_eq!(decoded, value);
        assert_eq!(encode_relay_control(&decoded).unwrap(), bytes);
    }
}

#[test]
fn exact_signature_domains_are_role_separated() {
    let values = controls();
    let reserve = relay_control_signing_bytes(
        &values[0],
        RelayControlSignatureRoleV1::ReserveRequestTarget,
    )
    .unwrap();
    assert!(reserve.starts_with(b"onebrain/reachability/relay-reserve-request/v1\0"));
    let keepalive =
        relay_control_signing_bytes(&values[2], RelayControlSignatureRoleV1::KeepaliveTarget)
            .unwrap();
    assert!(keepalive.starts_with(b"onebrain/reachability/relay-keepalive/v1\0"));
    assert!(
        relay_control_signing_bytes(&values[2], RelayControlSignatureRoleV1::DenialRelay).is_err()
    );
}

#[test]
fn unknown_noncanonical_and_invalid_scope_reject() {
    let mut bytes = encode_relay_control(&controls()[0]).unwrap();
    bytes[2] = 99;
    assert!(decode_relay_control(&bytes).is_err());

    let mut invalid = match controls().remove(0) {
        RelayControlV1::Reserve(value) => value,
        _ => unreachable!(),
    };
    invalid.transport_scope = vec![];
    assert!(encode_relay_control(&RelayControlV1::Reserve(invalid)).is_err());
}
