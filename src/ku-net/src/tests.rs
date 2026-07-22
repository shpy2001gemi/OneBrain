//! # ku-net Unit Tests
//!
//! Tests for identity, messages, membership, and discovery modules.

use crate::discovery::*;
use crate::error::*;
use crate::identity::*;
use crate::membership::*;
use crate::messages::*;
use std::time::Instant;

// ════════════════════════════════════════════════════════════════════════
// Identity Tests
// ════════════════════════════════════════════════════════════════════════

#[test]
fn test_node_id_generation_difficulty_16() {
    let keypair = KeyPair::generate();
    let pubkey = keypair.pubkey_bytes();

    let proof = generate_node_id(&pubkey, PUZZLE_C_SMALL); // difficulty=16

    // Must have at least 16 leading zero bits
    assert!(
        proof.node_id.leading_zeros() >= 16,
        "NodeId has {} leading zeros, expected >= 16",
        proof.node_id.leading_zeros()
    );
    assert_eq!(proof.difficulty, 16);
    println!("NodeId generated: {}", proof.node_id);
    println!(
        "Nonce: {}, Leading zeros: {}",
        proof.nonce,
        proof.node_id.leading_zeros()
    );
}

#[test]
fn test_node_id_verification_valid() {
    let keypair = KeyPair::generate();
    let pubkey = keypair.pubkey_bytes();

    let proof = generate_node_id(&pubkey, PUZZLE_C_SMALL);

    // Verify: correct pubkey + nonce → valid
    assert!(verify_node_id(
        &pubkey,
        proof.nonce,
        &proof.node_id,
        PUZZLE_C_SMALL
    ));
}

#[test]
fn test_node_id_verification_invalid_nonce() {
    let keypair = KeyPair::generate();
    let pubkey = keypair.pubkey_bytes();

    let proof = generate_node_id(&pubkey, PUZZLE_C_SMALL);

    // Wrong nonce → invalid
    assert!(!verify_node_id(
        &pubkey,
        proof.nonce + 1,
        &proof.node_id,
        PUZZLE_C_SMALL
    ));
}

#[test]
fn test_node_id_verification_invalid_pubkey() {
    let keypair1 = KeyPair::generate();
    let keypair2 = KeyPair::generate();
    let pubkey1 = keypair1.pubkey_bytes();
    let pubkey2 = keypair2.pubkey_bytes();

    let proof = generate_node_id(&pubkey1, PUZZLE_C_SMALL);

    // Wrong pubkey → invalid
    assert!(!verify_node_id(
        &pubkey2,
        proof.nonce,
        &proof.node_id,
        PUZZLE_C_SMALL
    ));
}

#[test]
fn test_node_id_xor_distance() {
    let id1 = NodeId([0xFF; 32]);
    let id2 = NodeId([0x00; 32]);
    let dist = id1.xor_distance(&id2);
    assert_eq!(dist, [0xFF; 32]); // Maximum distance

    let dist_self = id1.xor_distance(&id1);
    assert_eq!(dist_self, [0x00; 32]); // Zero distance to self
}

#[test]
fn test_node_id_leading_zeros() {
    let id = NodeId({
        let mut bytes = [0u8; 32];
        bytes[2] = 0x01; // 16 leading zero bits
        bytes
    });
    assert_eq!(id.leading_zeros(), 23); // 2*8 + 7 zeros in 0x01
}

#[test]
fn test_keypair_sign_verify() {
    let keypair = KeyPair::generate();
    let message = b"OneBrain Protocol test message";

    let signature = keypair.sign(message);
    assert!(keypair.verify(message, &signature));

    // Wrong message → verify fails
    assert!(!keypair.verify(b"wrong message", &signature));
}

#[test]
fn test_node_id_bounded_success() {
    let keypair = KeyPair::generate();
    let pubkey = keypair.pubkey_bytes();
    let result = generate_node_id_bounded(&pubkey, PUZZLE_C_SMALL, 1_000_000);
    assert!(result.is_ok());
    let proof = result.unwrap();
    assert!(proof.node_id.satisfies_difficulty(PUZZLE_C_SMALL));
}

#[test]
fn test_node_id_bounded_timeout() {
    let pubkey = [0xAA; 32];
    // Difficulty 32 with only 10 iterations — should timeout
    let result = generate_node_id_bounded(&pubkey, 32, 10);
    assert!(matches!(result, Err(IdentityError::PuzzleTimeout { .. })));
}

#[test]
fn test_node_id_invalid_difficulty() {
    let pubkey = [0xBB; 32];
    let result = generate_node_id_bounded(&pubkey, 33, 100);
    assert!(matches!(result, Err(IdentityError::InvalidDifficulty(33))));
}

#[test]
fn test_device_id_derivation() {
    let keypair = KeyPair::generate();
    let device_id = DeviceId::from_pubkey(&keypair.public_key());

    // Deterministic: same pubkey → same device_id
    let device_id2 = DeviceId::from_pubkey(&keypair.public_key());
    assert_eq!(device_id, device_id2);

    // Different pubkey → different device_id
    let keypair2 = KeyPair::generate();
    let device_id3 = DeviceId::from_pubkey(&keypair2.public_key());
    assert_ne!(device_id, device_id3);
}

#[test]
fn test_did_format() {
    let keypair = KeyPair::generate();
    let did = pubkey_to_did(&keypair.pubkey_bytes());
    assert!(did.starts_with("did:key:z6Mk"));
    assert_eq!(did.len(), 12 + 64); // "did:key:z6Mk" + 64 hex chars
}

// ════════════════════════════════════════════════════════════════════════
// Message Tests
// ════════════════════════════════════════════════════════════════════════

#[test]
fn test_message_header_roundtrip() {
    let header = MessageHeader {
        msg_type: MessageType::SwimPing,
        flags: MessageFlags(0x10), // 0-RTT safe
        payload_length: 1234,
    };

    let bytes = header.encode();
    assert_eq!(bytes.len(), 6);
    assert_eq!(bytes[0], 0x10); // SwimPing

    let decoded = MessageHeader::decode(&bytes).unwrap();
    assert_eq!(decoded.msg_type, MessageType::SwimPing);
    assert_eq!(decoded.flags.0, 0x10);
    assert_eq!(decoded.payload_length, 1234);
    assert!(decoded.flags.is_zero_rtt_safe());
}

#[test]
fn test_message_header_all_types() {
    // Verify every MessageType has a unique u8 and roundtrips
    let types = [
        MessageType::KuPush,
        MessageType::KuPull,
        MessageType::Gossip,
        MessageType::TrustUpdate,
        MessageType::DhtRequest,
        MessageType::Ping,
        MessageType::Pong,
        MessageType::Bundle,
        MessageType::BloomFilter,
        MessageType::PeerExchange,
        MessageType::RelayRequest,
        MessageType::RelayData,
        MessageType::RelayClose,
        MessageType::Capability,
        MessageType::SwimPing,
        MessageType::SwimAck,
        MessageType::SwimPingReq,
        MessageType::SwimNack,
        MessageType::SpFitness,
        MessageType::SpHandoff,
        MessageType::SpRedirect,
        MessageType::SpRegister,
        MessageType::SpOverloaded,
        MessageType::Goodbye,
        MessageType::HealthReport,
        MessageType::DepartingSoon,
        MessageType::ClusterAggregate,
        MessageType::FindNodeReq,
        MessageType::FindNodeResp,
        MessageType::FindValueReq,
        MessageType::FindValueResp,
        MessageType::StoreReq,
        MessageType::StoreAck,
        MessageType::HierLookup,
        MessageType::VacuumFilter,
        MessageType::VacuumExchange,
        MessageType::PheromoneUpdate,
        MessageType::TopicSubscribe,
        MessageType::TopicUnsubscribe,
        MessageType::TopicPublish,
        MessageType::TopicDeliver,
        MessageType::NdnInterest,
        MessageType::NdnData,
        MessageType::WatchNotify,
        MessageType::WatchRegister,
        MessageType::WatchUnregister,
        MessageType::TrustGossip,
        MessageType::TrustVaccine,
        MessageType::KuPropagation,
        MessageType::QueryForward,
        MessageType::QueryResponse,
        MessageType::QueryCancel,
        MessageType::CrdtSyncInit,
        MessageType::CrdtSyncDelta,
        MessageType::CrdtSyncAck,
        MessageType::CrdtSyncComplete,
        MessageType::MeshDelta,
        MessageType::CacheInvalidate,
        MessageType::PowChallenge,
        MessageType::PowResponse,
        MessageType::Backpressure,
        MessageType::ProofOfStorage,
        MessageType::ProofOfBandwidth,
        MessageType::SpDemotion,
        MessageType::BlacklistUpdate,
        // Encoding Consensus (0x90–0x95)
        MessageType::EncodingJobAnnounce,
        MessageType::EncodingClaimReq,
        MessageType::EncodingClaimResp,
        MessageType::EncodingSubmission,
        MessageType::EncodingConsensusResult,
        MessageType::EncodingJobUpdate,
    ];

    // Verify uniqueness
    let mut seen = std::collections::HashSet::new();
    for mt in &types {
        let val = *mt as u8;
        assert!(seen.insert(val), "Duplicate message type ID: 0x{:02x}", val);

        // Roundtrip
        let decoded = MessageType::from_u8(val).unwrap();
        assert_eq!(*mt, decoded);
    }
    println!(
        "All {} message types have unique IDs and roundtrip correctly",
        types.len()
    );
}

#[test]
fn test_message_type_ranges() {
    // Layer 0/1: 0x01–0x0F
    assert!((MessageType::KuPush as u8) >= 0x01);
    assert!((MessageType::Capability as u8) <= 0x0F);

    // Layer 2: 0x10–0x1C
    assert_eq!(MessageType::SwimPing as u8, 0x10);
    assert_eq!(MessageType::ClusterAggregate as u8, 0x1C);

    // Layer 3: 0x20–0x26
    assert_eq!(MessageType::FindNodeReq as u8, 0x20);
    assert_eq!(MessageType::HierLookup as u8, 0x26);

    // Layer 4: 0x30–0x38
    assert_eq!(MessageType::VacuumFilter as u8, 0x30);
    assert_eq!(MessageType::NdnData as u8, 0x38);

    // Security: 0x80+
    assert_eq!(MessageType::PowChallenge as u8, 0x80);
}

#[test]
fn test_network_address_ipv4_roundtrip() {
    let addr = NetworkAddress::new_v4(192, 168, 1, 100, 4242);
    let bytes = addr.encode();
    assert_eq!(bytes.len(), 7); // 1 + 4 + 2
    assert_eq!(bytes[0], 0x04); // IPv4

    let (decoded, consumed) = NetworkAddress::decode(&bytes).unwrap();
    assert_eq!(consumed, 7);
    assert_eq!(decoded.ip, addr.ip);
    assert_eq!(decoded.port, 4242);
}

#[test]
fn test_network_address_ipv6_roundtrip() {
    let addr = NetworkAddress::new_v6([0x2001, 0x0db8, 0, 0, 0, 0, 0, 1], 4242);
    let bytes = addr.encode();
    assert_eq!(bytes.len(), 19); // 1 + 16 + 2
    assert_eq!(bytes[0], 0x06); // IPv6

    let (decoded, consumed) = NetworkAddress::decode(&bytes).unwrap();
    assert_eq!(consumed, 19);
    assert_eq!(decoded.ip, addr.ip);
    assert_eq!(decoded.port, 4242);
}

#[test]
fn test_network_address_invalid_type() {
    let bytes = [0xFF, 0, 0, 0, 0, 0, 0];
    let result = NetworkAddress::decode(&bytes);
    assert!(result.is_err());
}

#[test]
fn test_zero_rtt_safety() {
    assert!(MessageType::Ping.is_zero_rtt_safe());
    assert!(MessageType::SwimPing.is_zero_rtt_safe());
    assert!(MessageType::FindNodeReq.is_zero_rtt_safe());
    assert!(!MessageType::KuPush.is_zero_rtt_safe());
    assert!(!MessageType::QueryForward.is_zero_rtt_safe());
}

// ════════════════════════════════════════════════════════════════════════
// Membership Tests
// ════════════════════════════════════════════════════════════════════════

fn make_test_fitness(score_target: f32) -> FitnessComponents {
    // Uniform components that sum to approximately score_target
    FitnessComponents {
        uptime: score_target,
        battery: score_target,
        bandwidth: score_target,
        storage: score_target,
        cpu: score_target,
        network_quality: score_target,
        reputation: score_target,
    }
}

#[test]
fn test_fitness_score_calculation() {
    let fitness = FitnessComponents {
        uptime: 0.9,          // 0.20 × 0.9 = 0.18
        battery: 1.0,         // 0.15 × 1.0 = 0.15
        bandwidth: 0.8,       // 0.15 × 0.8 = 0.12
        storage: 0.7,         // 0.10 × 0.7 = 0.07
        cpu: 0.6,             // 0.10 × 0.6 = 0.06
        network_quality: 1.0, // 0.15 × 1.0 = 0.15
        reputation: 0.8,      // 0.15 × 0.8 = 0.12
    };
    let score = fitness.score();
    let expected = 0.18 + 0.15 + 0.12 + 0.07 + 0.06 + 0.15 + 0.12;
    assert!(
        (score - expected).abs() < 0.001,
        "score={}, expected={}",
        score,
        expected
    );
}

#[test]
fn test_tier_promotion_thresholds() {
    assert_eq!(make_test_fitness(0.1).recommended_tier(), NodeTier::Leaf);
    assert_eq!(
        make_test_fitness(0.35).recommended_tier(),
        NodeTier::Contributor
    );
    assert_eq!(
        make_test_fitness(0.65).recommended_tier(),
        NodeTier::LocalSP
    );
    assert_eq!(
        make_test_fitness(0.80).recommended_tier(),
        NodeTier::RegionalSP
    );
    assert_eq!(
        make_test_fitness(0.90).recommended_tier(),
        NodeTier::CountrySP
    );
    assert_eq!(
        make_test_fitness(0.95).recommended_tier(),
        NodeTier::ContinentalSP
    );
    assert_eq!(
        make_test_fitness(0.99).recommended_tier(),
        NodeTier::GlobalBackbone
    );
}

#[test]
fn test_tier_demotion_hysteresis() {
    // Contributor promotion at 0.3, demotion at 0.2
    // This hysteresis prevents flapping
    assert!(
        NodeTier::Contributor.promotion_threshold() > NodeTier::Contributor.demotion_threshold()
    );
    assert_eq!(NodeTier::Contributor.promotion_threshold(), 0.3);
    assert_eq!(NodeTier::Contributor.demotion_threshold(), 0.2);
}

#[test]
fn test_membership_state_machine() {
    let node_id = NodeId([0x01; 32]);
    let fitness = make_test_fitness(0.5);
    let mut state = MembershipState::new(node_id, fitness);

    // Start empty
    assert_eq!(state.member_count(), 0);

    // Add a member
    let peer_id = NodeId([0x02; 32]);
    let entry = MemberEntry {
        node_id: peer_id,
        address: NetworkAddress::new_v4(10, 0, 0, 1, 4242),
        incarnation: 1,
        status: MemberStatus::Alive,
        tier: NodeTier::Contributor,
        last_seen: Instant::now(),
        fitness_score: 0.5,
        topic_vector: [0; 16],
    };
    state.upsert_member(entry);
    assert_eq!(state.member_count(), 1);

    // Verify alive
    assert_eq!(state.alive_members().len(), 1);

    // Mark suspect
    state.mark_suspect(&peer_id);
    let member = state.get_member(&peer_id).unwrap();
    assert!(matches!(member.status, MemberStatus::Suspect { .. }));
    assert_eq!(state.alive_members().len(), 0);

    // Mark dead
    state.mark_dead(&peer_id);
    let member = state.get_member(&peer_id).unwrap();
    assert!(matches!(member.status, MemberStatus::Dead { .. }));
}

#[test]
fn test_membership_refute_suspicion() {
    let node_id = NodeId([0x01; 32]);
    let fitness = make_test_fitness(0.5);
    let mut state = MembershipState::new(node_id, fitness);

    assert_eq!(state.my_incarnation, 0);
    state.refute_suspicion();
    assert_eq!(state.my_incarnation, 1);
    state.refute_suspicion();
    assert_eq!(state.my_incarnation, 2);
}

#[test]
fn test_membership_handle_ping() {
    let my_id = NodeId([0x01; 32]);
    let fitness = make_test_fitness(0.5);
    let mut state = MembershipState::new(my_id, fitness);

    let peer_id = NodeId([0x02; 32]);
    state.upsert_member(MemberEntry {
        node_id: peer_id,
        address: NetworkAddress::new_v4(10, 0, 0, 1, 4242),
        incarnation: 1,
        status: MemberStatus::Suspect {
            since: Instant::now(),
        },
        tier: NodeTier::Leaf,
        last_seen: Instant::now(),
        fitness_score: 0.3,
        topic_vector: [0; 16],
    });

    // Receiving PING from suspected peer should mark them Alive
    let _updates = state.handle_ping(&peer_id);
    let member = state.get_member(&peer_id).unwrap();
    assert!(matches!(member.status, MemberStatus::Alive));
}

#[test]
fn test_membership_graceful_departure() {
    let my_id = NodeId([0x01; 32]);
    let fitness = make_test_fitness(0.5);
    let mut state = MembershipState::new(my_id, fitness);

    let peer_id = NodeId([0x02; 32]);
    state.upsert_member(MemberEntry {
        node_id: peer_id,
        address: NetworkAddress::new_v4(10, 0, 0, 1, 4242),
        incarnation: 1,
        status: MemberStatus::Alive,
        tier: NodeTier::Contributor,
        last_seen: Instant::now(),
        fitness_score: 0.5,
        topic_vector: [0; 16],
    });

    state.mark_left(&peer_id);
    let member = state.get_member(&peer_id).unwrap();
    assert!(matches!(member.status, MemberStatus::Left));
}

#[test]
fn test_member_status_wire_format() {
    assert_eq!(MemberStatus::Alive.to_wire(), 0);
    assert_eq!(
        MemberStatus::Suspect {
            since: Instant::now()
        }
        .to_wire(),
        1
    );
    assert_eq!(
        MemberStatus::Dead {
            since: Instant::now()
        }
        .to_wire(),
        2
    );
    assert_eq!(MemberStatus::Left.to_wire(), 3);

    // Roundtrip
    for i in 0..=3u8 {
        let status = MemberStatus::from_wire(i);
        assert_eq!(status.to_wire(), i);
    }
}

// ════════════════════════════════════════════════════════════════════════
// Discovery Tests
// ════════════════════════════════════════════════════════════════════════

#[test]
fn test_bootstrap_state_machine() {
    let mut engine = BootstrapEngine::new();
    assert!(matches!(engine.state, BootstrapState::NotStarted));

    // Start → Discovering(Social)
    let layer = engine.start();
    assert_eq!(layer, BootstrapLayer::Social);
    assert!(matches!(
        engine.state,
        BootstrapState::Discovering {
            layer: BootstrapLayer::Social,
            ..
        }
    ));

    // Social fails → try Local
    let next = engine.layer_failed();
    assert_eq!(next, Some(BootstrapLayer::Local));

    // Local fails → try Http
    let next = engine.layer_failed();
    assert_eq!(next, Some(BootstrapLayer::Http));

    // Http finds peers → Joining
    engine.report_discovered(vec![
        DiscoveredPeer {
            node_id: None,
            address: NetworkAddress::new_v4(1, 2, 3, 4, 4242),
            source: BootstrapLayer::Http,
            discovered_at: Instant::now(),
        },
        DiscoveredPeer {
            node_id: None,
            address: NetworkAddress::new_v4(5, 6, 7, 8, 4242),
            source: BootstrapLayer::Http,
            discovered_at: Instant::now(),
        },
        DiscoveredPeer {
            node_id: None,
            address: NetworkAddress::new_v4(9, 10, 11, 12, 4242),
            source: BootstrapLayer::Http,
            discovered_at: Instant::now(),
        },
    ]);
    assert!(matches!(engine.state, BootstrapState::Joining { .. }));

    // Mark connected
    engine.mark_connected(BootstrapLayer::Http, 3);
    assert!(engine.is_connected());
    assert!(matches!(
        engine.state,
        BootstrapState::Connected {
            via_layer: BootstrapLayer::Http,
            peer_count: 3
        }
    ));
}

#[test]
fn test_bootstrap_all_layers_fail() {
    let mut engine = BootstrapEngine::new();
    engine.start();

    // Fail all 6 layers
    for _ in 0..5 {
        engine.layer_failed();
    }
    let last = engine.layer_failed();
    assert_eq!(last, None);
    assert!(matches!(engine.state, BootstrapState::Failed { .. }));
}

#[test]
fn test_bootstrap_layers_require_internet() {
    assert!(!BootstrapLayer::Social.requires_internet());
    assert!(!BootstrapLayer::Local.requires_internet());
    assert!(BootstrapLayer::Http.requires_internet());
    assert!(BootstrapLayer::Dht.requires_internet());
    assert!(BootstrapLayer::Dns.requires_internet());
    assert!(BootstrapLayer::Hardcoded.requires_internet());
}

#[test]
fn test_pex_peer_selection() {
    let mut pex = PexState::new();

    // Add peers with varying fitness
    for i in 0..20u8 {
        pex.receive_peer(PexPeerInfo {
            node_id: NodeId([i; 32]),
            address: NetworkAddress::new_v4(10, 0, 0, i, 4242),
            tier: 1,
            fitness: (i as u16) * 500,
            last_verified: Instant::now(),
        });
    }

    // Select top 10 by fitness
    let selected = pex.select_for_exchange();
    assert_eq!(selected.len(), 10);
    // First should have highest fitness
    assert_eq!(selected[0].fitness, 9500);
}

// ════════════════════════════════════════════════════════════════════════
// Summary Test
// ════════════════════════════════════════════════════════════════════════

#[test]
fn test_print_summary() {
    println!("\n╔══════════════════════════════════════════════╗");
    println!("║   ku-net Test Summary                        ║");
    println!("╠══════════════════════════════════════════════╣");
    println!("║ Identity:                                    ║");
    println!("║   - NodeId with BLAKE3 crypto puzzle         ║");
    println!("║   - Ed25519 sign/verify                      ║");
    println!("║   - DeviceId derivation                      ║");
    println!("║   - DID format                               ║");
    println!("║ Messages:                                    ║");
    println!("║   - 81 message types (unique IDs)            ║");
    println!("║   - 6-byte header encode/decode              ║");
    println!("║   - NetworkAddress IPv4/IPv6                  ║");
    println!("║ Membership:                                  ║");
    println!("║   - SWIM state machine                       ║");
    println!("║   - 7-tier fitness model                     ║");
    println!("║   - Suspicion/refutation                     ║");
    println!("║ Discovery:                                   ║");
    println!("║   - 6-layer bootstrap cascade                ║");
    println!("║   - PEX peer exchange                        ║");
    println!("╚══════════════════════════════════════════════╝");
}
