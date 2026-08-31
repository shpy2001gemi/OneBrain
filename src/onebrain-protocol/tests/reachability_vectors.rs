use ku_core::foundation::NodeId;
use onebrain_protocol::{
    decode_reachability_object, encode_reachability_object, reachability_signing_bytes,
    BootstrapManifestV1, DirectCandidateKindV1, DirectCandidateV1, DiscoveryEndpointV1,
    DiscoveryTransportV1, HostAddressV1, ProtocolVersionV1, ReachabilityAdvertisementV1,
    ReachabilityEndpointV1, ReachabilityObjectV1, ReachabilitySignatureRoleV1, RelayCandidateV1,
    RelayDescriptorV1, RelayEndpointV1, RelayReservationV1, RelayTransportV1, RouteFailureCodeV1,
    RouteLimitationCodeV1, RouteLimitationV1, RoutePathKindV1, RoutePlanV1, RouteReceiptV1,
    RouteResourceBudgetV1, RouteTerminalOutcomeV1, MAX_RELAY_DESCRIPTOR_VALIDITY_SECONDS,
};

fn node(byte: u8) -> NodeId {
    NodeId::from_bytes([byte; 32])
}

const GOLDEN_ROOT_BLAKE3: [&str; 6] = [
    "0927f09491b37215685329328a9d535d26ac6c3d3f7228622e17af68f213e9ae",
    "3a531ab98cf3d17c3ab63afd56b3e7eb354a94922683743348f3ea3a7c808059",
    "2ceb611375d4471c35ed8afddd390f28abbb9c60d465581ae01c7b9762f6c9df",
    "12c6e65512fec94779af54dfdc363864982734b14a9fcec8b54def8e78996d51",
    "5420ff9df28c1de035f229e071fdc83e1b79493e7566dc6b28f3d4e66a99bc6f",
    "c9d10465c91c0594fe8cd41ab1d5a7de1ad48674662aaac5ef26cb5df2ec66b5",
];

fn roots() -> Vec<ReachabilityObjectV1> {
    let endpoint = ReachabilityEndpointV1 {
        host: HostAddressV1::Ipv4([1, 1, 1, 1]),
        port: 443,
    };
    let reservation = RelayReservationV1 {
        format: 1,
        relay_node_id: node(1),
        target_node_id: node(2),
        reservation_id: [3; 32],
        transport_scope: vec![RelayTransportV1::TlsTcp443],
        issued_at: 10,
        expires_at: 20,
        target_signature: [4; 64],
        relay_signature: [5; 64],
    };
    vec![
        ReachabilityObjectV1::BootstrapManifest(BootstrapManifestV1 {
            format: 1,
            discovery_source_id: [6; 32],
            discovery_endpoints: vec![DiscoveryEndpointV1 {
                transport: DiscoveryTransportV1::Https,
                host: HostAddressV1::Dns("relay.example".into()),
                port: 443,
                path: "/obp/bootstrap".into(),
            }],
            protocol_versions: vec![ProtocolVersionV1 { major: 1, minor: 0 }],
            sequence: 1,
            issued_at: 10,
            expires_at: 20,
            source_signature: [7; 64],
        }),
        ReachabilityObjectV1::RelayDescriptor(RelayDescriptorV1 {
            format: 1,
            relay_node_id: node(1),
            relay_public_key: [8; 32],
            endpoints: vec![RelayEndpointV1 {
                transport: RelayTransportV1::TlsTcp443,
                host: HostAddressV1::Dns("relay.example".into()),
                port: 443,
            }],
            supported_transports: vec![RelayTransportV1::TlsTcp443],
            protocol_versions: vec![ProtocolVersionV1 { major: 1, minor: 0 }],
            capacity_policy_digest: [9; 32],
            previous_descriptor_blake3: None,
            sequence: 1,
            issued_at: 10,
            expires_at: 20,
            relay_signature: [10; 64],
        }),
        ReachabilityObjectV1::RelayReservation(reservation.clone()),
        ReachabilityObjectV1::Advertisement(ReachabilityAdvertisementV1 {
            format: 1,
            target_node_id: node(2),
            relay_reservations: vec![reservation],
            optional_public_candidates: Vec::new(),
            capability_ceiling: [11; 32],
            sequence: 1,
            issued_at: 10,
            expires_at: 20,
            target_signature: [12; 64],
        }),
        ReachabilityObjectV1::RoutePlan(RoutePlanV1 {
            expected_peer: node(2),
            direct_candidates: vec![DirectCandidateV1 {
                endpoint: endpoint.clone(),
                kind: DirectCandidateKindV1::ServerReflexive,
                priority: 10,
                network_epoch: 3,
                expires_at: 20,
            }],
            relay_candidates: vec![RelayCandidateV1 {
                relay_node_id: node(1),
                reservation_id: [3; 32],
                transport: RelayTransportV1::TlsTcp443,
                endpoint,
                priority: 5,
                expires_at: 20,
            }],
            deadline: 20,
            attempt_budget: 2,
            resource_budget: RouteResourceBudgetV1 {
                max_concurrent_checks: 2,
                max_signature_checks: 4,
                max_probe_bytes: 1024,
            },
            privacy_policy_digest: [13; 32],
        }),
        ReachabilityObjectV1::RouteReceipt(RouteReceiptV1 {
            expected_peer: node(2),
            authenticated_peer: Some(node(2)),
            selected_path_kind: Some(RoutePathKindV1::RelayTcp443),
            selected_carrier_identity: Some(node(1)),
            attempts: Vec::new(),
            transport_binding_digest: Some([14; 32]),
            session_id: Some([15; 32]),
            started_at: 10,
            authenticated_at: Some(11),
            terminal_outcome: RouteTerminalOutcomeV1::Connected,
            limitations: vec![RouteLimitationV1 {
                code: RouteLimitationCodeV1::CandidateBudgetExhausted,
                count: 1,
            }],
            local_signature: [16; 64],
        }),
    ]
}

#[test]
fn six_roots_round_trip_with_byte_identical_reencoding() {
    let roots = roots();
    assert_eq!(roots.len(), 6);
    for (index, root) in roots.into_iter().enumerate() {
        let encoded = encode_reachability_object(&root).unwrap();
        assert_eq!(
            blake3::hash(&encoded).to_hex().as_str(),
            GOLDEN_ROOT_BLAKE3[index]
        );
        let decoded = decode_reachability_object(&encoded).unwrap();
        assert_eq!(decoded, root);
        assert_eq!(encode_reachability_object(&decoded).unwrap(), encoded);
    }
}

#[test]
fn signing_preimage_excludes_only_the_selected_signature() {
    for root in roots() {
        let role = match root {
            ReachabilityObjectV1::BootstrapManifest(_) => {
                ReachabilitySignatureRoleV1::BootstrapSource
            }
            ReachabilityObjectV1::RelayDescriptor(_) => {
                ReachabilitySignatureRoleV1::RelayDescriptor
            }
            ReachabilityObjectV1::RelayReservation(_) => {
                ReachabilitySignatureRoleV1::ReservationTarget
            }
            ReachabilityObjectV1::Advertisement(_) => {
                ReachabilitySignatureRoleV1::AdvertisementTarget
            }
            ReachabilityObjectV1::RoutePlan(_) => continue,
            ReachabilityObjectV1::RouteReceipt(_) => ReachabilitySignatureRoleV1::RouteReceiptLocal,
        };
        let first = reachability_signing_bytes(&root, role).unwrap();
        let expected_domain = match role {
            ReachabilitySignatureRoleV1::BootstrapSource => {
                b"onebrain/reachability/bootstrap-manifest/v1\0".as_slice()
            }
            ReachabilitySignatureRoleV1::RelayDescriptor => {
                b"onebrain/reachability/relay-descriptor/v1\0".as_slice()
            }
            ReachabilitySignatureRoleV1::ReservationTarget => {
                b"onebrain/reachability/relay-reservation-target/v1\0".as_slice()
            }
            ReachabilitySignatureRoleV1::AdvertisementTarget => {
                b"onebrain/reachability/advertisement/v1\0".as_slice()
            }
            ReachabilitySignatureRoleV1::RouteReceiptLocal => {
                b"onebrain/reachability/route-receipt/v1\0".as_slice()
            }
            _ => unreachable!(),
        };
        assert!(first.starts_with(expected_domain));
        assert_ne!(first, encode_reachability_object(&root).unwrap());
    }
}

#[test]
fn unknown_missing_noncanonical_and_over_limit_inputs_reject() {
    assert!(decode_reachability_object(&[]).is_err());
    assert!(decode_reachability_object(&[0xa1, 0x00, 0x01]).is_err());
    let oversized = RoutePlanV1 {
        expected_peer: node(1),
        direct_candidates: Vec::new(),
        relay_candidates: Vec::new(),
        deadline: 1,
        attempt_budget: 65,
        resource_budget: RouteResourceBudgetV1 {
            max_concurrent_checks: 1,
            max_signature_checks: 1,
            max_probe_bytes: 1,
        },
        privacy_policy_digest: [1; 32],
    };
    assert!(encode_reachability_object(&ReachabilityObjectV1::RoutePlan(oversized)).is_err());
}

#[test]
fn cross_object_signature_role_substitution_rejects() {
    let descriptor = roots().remove(1);
    assert!(reachability_signing_bytes(
        &descriptor,
        ReachabilitySignatureRoleV1::AdvertisementTarget
    )
    .is_err());
}

#[test]
fn failure_codes_remain_protocol_local() {
    assert_ne!(
        RouteFailureCodeV1::RelayUnavailable,
        RouteFailureCodeV1::DirectTimeout
    );
}

#[test]
fn relay_descriptor_accepts_thirty_minutes_but_rejects_one_second_more() {
    let mut descriptor = match roots().remove(1) {
        ReachabilityObjectV1::RelayDescriptor(value) => value,
        _ => unreachable!(),
    };
    descriptor.issued_at = 1_000;
    descriptor.expires_at = descriptor.issued_at + MAX_RELAY_DESCRIPTOR_VALIDITY_SECONDS;
    assert!(
        encode_reachability_object(&ReachabilityObjectV1::RelayDescriptor(descriptor.clone()))
            .is_ok()
    );

    descriptor.expires_at += 1;
    assert!(
        encode_reachability_object(&ReachabilityObjectV1::RelayDescriptor(descriptor)).is_err()
    );
}

#[test]
fn unordered_sets_and_non_public_addresses_reject() {
    let mut descriptor = match roots().remove(1) {
        ReachabilityObjectV1::RelayDescriptor(value) => value,
        _ => unreachable!(),
    };
    descriptor.supported_transports = vec![RelayTransportV1::TlsTcp443, RelayTransportV1::QuicUdp];
    descriptor.endpoints.push(RelayEndpointV1 {
        transport: RelayTransportV1::QuicUdp,
        host: HostAddressV1::Ipv4([1, 1, 1, 1]),
        port: 41000,
    });
    assert!(
        encode_reachability_object(&ReachabilityObjectV1::RelayDescriptor(descriptor)).is_err()
    );

    let mut descriptor = match roots().remove(1) {
        ReachabilityObjectV1::RelayDescriptor(value) => value,
        _ => unreachable!(),
    };
    descriptor.endpoints[0].host = HostAddressV1::Ipv4([192, 168, 1, 1]);
    assert!(
        encode_reachability_object(&ReachabilityObjectV1::RelayDescriptor(descriptor)).is_err()
    );
}

#[test]
fn private_host_candidate_is_allowed_only_in_the_local_route_plan() {
    let mut plan = match roots().remove(4) {
        ReachabilityObjectV1::RoutePlan(value) => value,
        _ => unreachable!(),
    };
    plan.direct_candidates[0].kind = DirectCandidateKindV1::Host;
    plan.direct_candidates[0].endpoint.host = HostAddressV1::Ipv4([192, 168, 1, 1]);
    assert!(encode_reachability_object(&ReachabilityObjectV1::RoutePlan(plan)).is_ok());
}
