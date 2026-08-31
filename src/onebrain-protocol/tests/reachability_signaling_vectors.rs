use ku_core::foundation::NodeId;
use onebrain_protocol::{
    connectivity_signing_bytes, connectivity_signing_parts, decode_connectivity_signaling,
    encode_connectivity_signaling, ConnectivitySignalingV1, ConnectivitySignatureRoleV1,
    HolePunchScheduleV1, HostAddressV1, PrivateCandidateSignalV1, PrivateCandidateV1,
    ReachabilityEndpointV1, ReflexiveObservationV1, RelayAssociationV1, RelayConnectRequestV1,
};

fn node(byte: u8) -> NodeId {
    NodeId::from_bytes([byte; 32])
}

fn objects() -> Vec<(
    ConnectivitySignalingV1,
    ConnectivitySignatureRoleV1,
    &'static [u8],
)> {
    vec![
        (
            ConnectivitySignalingV1::ReflexiveObservation(ReflexiveObservationV1 {
                format: 1,
                relay_node_id: node(1),
                target_node_id: node(2),
                reservation_id: [3; 32],
                observed_endpoint: ReachabilityEndpointV1 {
                    host: HostAddressV1::Ipv4([8, 8, 8, 8]),
                    port: 41_000,
                },
                network_epoch: 4,
                sequence: 1,
                issued_at: 10,
                expires_at: 40,
                relay_signature: [5; 64],
            }),
            ConnectivitySignatureRoleV1::ReflexiveRelay,
            b"onebrain/reachability/reflexive-observation/v1\0",
        ),
        (
            ConnectivitySignalingV1::HolePunchSchedule(HolePunchScheduleV1 {
                format: 1,
                relay_node_id: node(1),
                initiator_node_id: node(2),
                responder_node_id: node(3),
                initiator_reservation_id: [4; 32],
                responder_reservation_id: [5; 32],
                rendezvous_token: [6; 32],
                association_barrier_digest: [7; 32],
                start_delay_ms: 500,
                interval_ms: 200,
                attempt_count: 10,
                expires_at: 40,
                relay_signature: [8; 64],
            }),
            ConnectivitySignatureRoleV1::HolePunchRelay,
            b"onebrain/reachability/hole-punch-schedule/v1\0",
        ),
        (
            ConnectivitySignalingV1::RelayConnectRequest(RelayConnectRequestV1 {
                format: 1,
                initiator_node_id: node(2),
                target_node_id: node(3),
                initiator_reservation_id: [4; 32],
                target_reservation_id: [5; 32],
                nonce: [6; 32],
                sequence: 1,
                issued_at: 10,
                expires_at: 40,
                initiator_signature: [9; 64],
            }),
            ConnectivitySignatureRoleV1::RelayConnectInitiator,
            b"onebrain/reachability/relay-connect-request/v1\0",
        ),
        (
            ConnectivitySignalingV1::RelayAssociation(RelayAssociationV1 {
                format: 1,
                relay_node_id: node(1),
                initiator_node_id: node(2),
                target_node_id: node(3),
                initiator_reservation_id: [4; 32],
                target_reservation_id: [5; 32],
                association_id: [6; 32],
                issued_at: 10,
                expires_at: 40,
                relay_signature: [10; 64],
            }),
            ConnectivitySignatureRoleV1::RelayAssociationRelay,
            b"onebrain/reachability/relay-association/v1\0",
        ),
        (
            ConnectivitySignalingV1::PrivateCandidateSignal(PrivateCandidateSignalV1 {
                format: 1,
                sender_node_id: node(2),
                target_node_id: node(3),
                session_id: [4; 32],
                network_epoch: 5,
                candidates: vec![PrivateCandidateV1 {
                    endpoint: ReachabilityEndpointV1 {
                        host: HostAddressV1::Ipv4([10, 0, 0, 8]),
                        port: 41_000,
                    },
                    priority: 9,
                    foundation: [6; 16],
                }],
                sequence: 1,
                issued_at: 10,
                expires_at: 40,
                sender_signature: [11; 64],
            }),
            ConnectivitySignatureRoleV1::PrivateCandidateSender,
            b"onebrain/reachability/private-candidate-signal/v1\0",
        ),
    ]
}

#[test]
fn five_schema_ids_round_trip_canonically_and_have_exact_domains() {
    for (object, role, domain) in objects() {
        let bytes = encode_connectivity_signaling(&object).unwrap();
        assert_eq!(decode_connectivity_signaling(&bytes).unwrap(), object);
        assert_eq!(
            &connectivity_signing_bytes(&object, role).unwrap()[..domain.len()],
            domain
        );
        let (split_domain, unsigned) = connectivity_signing_parts(&object, role).unwrap();
        assert_eq!(split_domain, domain);
        assert_eq!(
            connectivity_signing_bytes(&object, role).unwrap(),
            [split_domain, unsigned.as_slice()].concat()
        );
    }
}

#[test]
fn wrong_role_unknown_schema_noncanonical_and_private_public_substitution_reject() {
    let (object, _, _) = &objects()[0];
    assert!(connectivity_signing_bytes(
        object,
        ConnectivitySignatureRoleV1::PrivateCandidateSender
    )
    .is_err());
    let mut bytes = encode_connectivity_signaling(object).unwrap();
    bytes[2] = 99;
    assert!(decode_connectivity_signaling(&bytes).is_err());

    let private = encode_connectivity_signaling(&objects()[4].0).unwrap();
    assert!(onebrain_protocol::decode_reachability_object(&private).is_err());
}

#[test]
fn exact_punch_constants_and_private_candidate_ceiling_are_closed() {
    let mut bad = match objects()[1].0.clone() {
        ConnectivitySignalingV1::HolePunchSchedule(value) => value,
        _ => unreachable!(),
    };
    bad.interval_ms = 201;
    assert!(
        encode_connectivity_signaling(&ConnectivitySignalingV1::HolePunchSchedule(bad)).is_err()
    );

    let mut private = match objects()[4].0.clone() {
        ConnectivitySignalingV1::PrivateCandidateSignal(value) => value,
        _ => unreachable!(),
    };
    private.candidates = vec![private.candidates[0].clone(); 9];
    assert!(
        encode_connectivity_signaling(&ConnectivitySignalingV1::PrivateCandidateSignal(private))
            .is_err()
    );
}
