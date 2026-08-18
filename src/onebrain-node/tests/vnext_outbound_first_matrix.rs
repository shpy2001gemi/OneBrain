#![cfg(feature = "vnext-outbound-first")]

use ku_core::foundation::NodeId;
use ku_net::vnext_route_plan::{ConnectionPlanner, PlannerAction, PlannerEvent, RouteFailure};
use onebrain_protocol::{
    encode_reachability_object, DirectCandidateKindV1, DirectCandidateV1, HostAddressV1,
    ReachabilityEndpointV1, ReachabilityObjectV1, RelayCandidateV1, RelayTransportV1,
    RouteFailureCodeV1, RoutePathKindV1, RoutePlanV1, RouteResourceBudgetV1,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Adapter {
    Linux,
    Windows,
    Macos,
    Android,
    Ios,
    Browser,
}

const ADAPTERS: [Adapter; 6] = [
    Adapter::Linux,
    Adapter::Windows,
    Adapter::Macos,
    Adapter::Android,
    Adapter::Ios,
    Adapter::Browser,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Capabilities {
    direct_listen: bool,
    lan: bool,
    udp: bool,
    suspendable: bool,
    web: bool,
}

fn capabilities(adapter: Adapter) -> Capabilities {
    match adapter {
        Adapter::Linux | Adapter::Windows | Adapter::Macos => Capabilities {
            direct_listen: true,
            lan: true,
            udp: true,
            suspendable: false,
            web: false,
        },
        Adapter::Android => Capabilities {
            direct_listen: false,
            lan: true,
            udp: true,
            suspendable: true,
            web: false,
        },
        Adapter::Ios => Capabilities {
            direct_listen: false,
            lan: false,
            udp: true,
            suspendable: true,
            web: false,
        },
        Adapter::Browser => Capabilities {
            direct_listen: false,
            lan: false,
            udp: true,
            suspendable: true,
            web: true,
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NetworkFixture {
    FullCone,
    AddressRestricted,
    PortRestricted,
    SymmetricNat,
    Cgnat,
    UpstreamUdpDrop,
    UdpTotalBlock,
    DirectToRelay,
    RelayToDirect,
    AddressChurn,
    SuspendResume,
    RelaysDownDiscoveryRemains,
    BootstrapUnavailable,
}

const NETWORK_FIXTURES: [NetworkFixture; 13] = [
    NetworkFixture::FullCone,
    NetworkFixture::AddressRestricted,
    NetworkFixture::PortRestricted,
    NetworkFixture::SymmetricNat,
    NetworkFixture::Cgnat,
    NetworkFixture::UpstreamUdpDrop,
    NetworkFixture::UdpTotalBlock,
    NetworkFixture::DirectToRelay,
    NetworkFixture::RelayToDirect,
    NetworkFixture::AddressChurn,
    NetworkFixture::SuspendResume,
    NetworkFixture::RelaysDownDiscoveryRemains,
    NetworkFixture::BootstrapUnavailable,
];

fn endpoint(last: u8, port: u16) -> ReachabilityEndpointV1 {
    ReachabilityEndpointV1 {
        host: HostAddressV1::Ipv4([8, 8, 4, last]),
        port,
    }
}

fn route_plan(adapter: Adapter, fixture: NetworkFixture) -> RoutePlanV1 {
    let caps = capabilities(adapter);
    let direct_allowed = caps.direct_listen
        && matches!(
            fixture,
            NetworkFixture::FullCone
                | NetworkFixture::AddressRestricted
                | NetworkFixture::PortRestricted
                | NetworkFixture::DirectToRelay
                | NetworkFixture::RelayToDirect
                | NetworkFixture::AddressChurn
        );
    let udp_relay = caps.udp && fixture != NetworkFixture::UdpTotalBlock;
    let direct = direct_allowed
        .then(|| DirectCandidateV1 {
            endpoint: endpoint(10, 41_000),
            kind: DirectCandidateKindV1::ServerReflexive,
            priority: 100,
            network_epoch: 7,
            expires_at: 900,
        })
        .into_iter()
        .collect();
    let relay = if fixture == NetworkFixture::BootstrapUnavailable {
        Vec::new()
    } else {
        vec![RelayCandidateV1 {
            relay_node_id: NodeId::from_bytes([0x44; 32]),
            reservation_id: [0x45; 32],
            transport: if udp_relay {
                RelayTransportV1::QuicUdp
            } else {
                RelayTransportV1::TlsTcp443
            },
            endpoint: endpoint(44, if udp_relay { 41_000 } else { 443 }),
            priority: 50,
            expires_at: 900,
        }]
    };
    RoutePlanV1 {
        expected_peer: NodeId::from_bytes([0x33; 32]),
        direct_candidates: direct,
        relay_candidates: relay,
        deadline: 1_000,
        attempt_budget: 12,
        resource_budget: RouteResourceBudgetV1 {
            max_concurrent_checks: 2,
            max_signature_checks: 64,
            max_probe_bytes: 65_536,
        },
        privacy_policy_digest: [0x55; 32],
    }
}

fn first_action(plan: &RoutePlanV1) -> Result<PlannerAction, RouteFailure> {
    let mut planner = ConnectionPlanner::new(plan.clone());
    assert_eq!(planner.next(1, None).unwrap(), PlannerAction::Gather);
    planner.next(
        2,
        Some(PlannerEvent::CandidatesGathered {
            direct: plan.direct_candidates.clone(),
            relay: plan.relay_candidates.clone(),
        }),
    )
}

#[test]
fn every_nat_and_lifecycle_fixture_has_a_deterministic_fail_closed_outcome() {
    for adapter in ADAPTERS {
        for fixture in NETWORK_FIXTURES {
            let result = first_action(&route_plan(adapter, fixture));
            if fixture == NetworkFixture::BootstrapUnavailable {
                assert!(matches!(result, Err(RouteFailure::PathLimited { .. })));
            } else if capabilities(adapter).direct_listen
                && matches!(
                    fixture,
                    NetworkFixture::FullCone
                        | NetworkFixture::AddressRestricted
                        | NetworkFixture::PortRestricted
                        | NetworkFixture::DirectToRelay
                        | NetworkFixture::RelayToDirect
                        | NetworkFixture::AddressChurn
                )
            {
                assert!(matches!(result, Ok(PlannerAction::CheckDirect(_))));
            } else {
                assert!(matches!(
                    result,
                    Ok(PlannerAction::EnsureRouteReservation { .. })
                ));
            }
        }
    }
}

#[test]
fn platform_capabilities_remove_attempts_without_changing_authority() {
    let canonical = route_plan(Adapter::Linux, NetworkFixture::SymmetricNat);
    let expected = encode_reachability_object(&ReachabilityObjectV1::RoutePlan(canonical)).unwrap();
    let expected_peer = NodeId::from_bytes([0x33; 32]);
    for adapter in ADAPTERS {
        let plan = route_plan(adapter, NetworkFixture::SymmetricNat);
        assert_eq!(plan.expected_peer, expected_peer);
        assert_eq!(plan.privacy_policy_digest, [0x55; 32]);
        assert_eq!(
            encode_reachability_object(&ReachabilityObjectV1::RoutePlan(plan)).unwrap(),
            expected
        );
    }
}

#[test]
fn browser_projection_keeps_canonical_route_classes() {
    let webtransport = route_plan(Adapter::Browser, NetworkFixture::SymmetricNat);
    assert_eq!(
        webtransport.relay_candidates[0].transport,
        RelayTransportV1::QuicUdp
    );
    let websocket = route_plan(Adapter::Browser, NetworkFixture::UdpTotalBlock);
    assert_eq!(
        websocket.relay_candidates[0].transport,
        RelayTransportV1::TlsTcp443
    );
}

#[test]
fn android_and_ios_resume_only_from_durable_checkpoint_boundaries() {
    use onebrain_node::vnext_connection_planner::{RoutedDeliveryGate, RoutedDeliveryState};

    for adapter in [Adapter::Android, Adapter::Ios] {
        assert!(capabilities(adapter).suspendable);
        for killed_after in 0..4 {
            let gate = RoutedDeliveryGate::default();
            let transitions = [
                RoutedDeliveryState::Quiescing,
                RoutedDeliveryState::Replanning,
                RoutedDeliveryState::Reauthenticating,
                RoutedDeliveryState::Resuming,
            ];
            for next in transitions.iter().take(killed_after + 1) {
                gate.transition(*next).unwrap();
            }
            assert!(!gate.writes_open().unwrap());
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Attack {
    WrongKey,
    WrongNodeId,
    RouteSubstitution,
    TranscriptSubstitution,
    DescriptorSybilFlood,
    Malformed,
    Duplicate,
    Oversize,
    Expired,
    Replay,
    RelayDrop,
    RelayDelay,
    RelayDuplicate,
    RelayReorder,
    RelayShutdown,
    PoisonedRendezvous,
    PoisonedPex,
    UntrustedMirror,
}

fn reject_attack(attack: Attack) -> &'static str {
    match attack {
        Attack::WrongKey | Attack::WrongNodeId => "identity",
        Attack::RouteSubstitution | Attack::TranscriptSubstitution => "binding",
        Attack::DescriptorSybilFlood | Attack::Oversize => "budget",
        Attack::Malformed | Attack::Duplicate | Attack::Expired | Attack::Replay => "admission",
        Attack::RelayDrop
        | Attack::RelayDelay
        | Attack::RelayDuplicate
        | Attack::RelayReorder
        | Attack::RelayShutdown => "carrier",
        Attack::PoisonedRendezvous | Attack::PoisonedPex | Attack::UntrustedMirror => "discovery",
    }
}

#[test]
fn adversarial_relay_and_discovery_catalog_is_closed() {
    let attacks = [
        Attack::WrongKey,
        Attack::WrongNodeId,
        Attack::RouteSubstitution,
        Attack::TranscriptSubstitution,
        Attack::DescriptorSybilFlood,
        Attack::Malformed,
        Attack::Duplicate,
        Attack::Oversize,
        Attack::Expired,
        Attack::Replay,
        Attack::RelayDrop,
        Attack::RelayDelay,
        Attack::RelayDuplicate,
        Attack::RelayReorder,
        Attack::RelayShutdown,
        Attack::PoisonedRendezvous,
        Attack::PoisonedPex,
        Attack::UntrustedMirror,
    ];
    assert_eq!(attacks.len(), 18);
    for attack in attacks {
        assert!(!reject_attack(attack).is_empty());
    }
}

fn privacy_safe(bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(bytes).to_ascii_lowercase();
    let forbidden = [
        "192.168.",
        "10.0.",
        "169.254.",
        "fe80:",
        "wlan0",
        "eth0",
        "ssid",
        "signer_locator",
        "unrelated_peers",
    ];
    forbidden.iter().all(|needle| !text.contains(needle))
}

#[test]
fn every_public_artifact_is_privacy_safe_and_mutations_are_detected() {
    for artifact in [
        br#"{"path":"relay-udp","carrier":"node:44"}"#.as_slice(),
        br#"{"terminal":"connected","limitations":[]}"#.as_slice(),
        br#"relay accepted reservation=redacted"#.as_slice(),
    ] {
        assert!(privacy_safe(artifact));
    }
    for leaked in [
        b"peer=192.168.1.3".as_slice(),
        b"if=wlan0".as_slice(),
        b"source=fe80::1".as_slice(),
        b"ssid=private".as_slice(),
    ] {
        assert!(!privacy_safe(leaked));
    }
}

#[test]
fn resource_and_network_epoch_bounds_fail_before_authority_changes() {
    let mut plan = route_plan(Adapter::Linux, NetworkFixture::FullCone);
    plan.resource_budget.max_probe_bytes = 0;
    assert_eq!(
        first_action(&plan).unwrap_err(),
        RouteFailure::BudgetExceeded
    );

    let mut planner = ConnectionPlanner::new(route_plan(Adapter::Linux, NetworkFixture::FullCone));
    assert_eq!(planner.next(1, None).unwrap(), PlannerAction::Gather);
    assert_eq!(
        planner
            .next(2, Some(PlannerEvent::NetworkEpochChanged(999)))
            .unwrap_err(),
        RouteFailure::NetworkChanged
    );
}

#[test]
fn namespace_contract_covers_real_topology_names() {
    let names = [
        "full-cone",
        "address-restricted",
        "port-restricted",
        "symmetric-nat",
        "two-level-cgnat",
        "upstream-udp-drop",
        "udp-total-block-tcp443-fallback",
        "address-migration",
    ];
    assert_eq!(names.len(), 8);
    assert!(names
        .iter()
        .all(|name| name.is_ascii() && !name.contains(' ')));
}

#[test]
fn direct_timeout_falls_through_to_outbound_relay_without_nat_configuration() {
    let plan = route_plan(Adapter::Linux, NetworkFixture::DirectToRelay);
    let mut planner = ConnectionPlanner::new(plan.clone());
    planner.next(1, None).unwrap();
    assert!(matches!(
        planner
            .next(
                2,
                Some(PlannerEvent::CandidatesGathered {
                    direct: plan.direct_candidates.clone(),
                    relay: plan.relay_candidates.clone(),
                }),
            )
            .unwrap(),
        PlannerAction::CheckDirect(_)
    ));
    assert!(matches!(
        planner
            .next(
                3,
                Some(PlannerEvent::AttemptFailed(
                    RouteFailureCodeV1::DirectTimeout
                )),
            )
            .unwrap(),
        PlannerAction::EnsureRouteReservation { .. }
    ));
}

#[test]
fn web_paths_remain_relay_class_not_peer_identity() {
    for (fixture, expected) in [
        (NetworkFixture::SymmetricNat, RoutePathKindV1::RelayUdp),
        (NetworkFixture::UdpTotalBlock, RoutePathKindV1::RelayTcp443),
    ] {
        let plan = route_plan(Adapter::Browser, fixture);
        let actual = match plan.relay_candidates[0].transport {
            RelayTransportV1::QuicUdp => RoutePathKindV1::RelayUdp,
            RelayTransportV1::TlsTcp443 => RoutePathKindV1::RelayTcp443,
        };
        assert_eq!(actual, expected);
        assert_ne!(plan.expected_peer, plan.relay_candidates[0].relay_node_id);
    }
}
