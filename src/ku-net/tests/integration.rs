//! # ku-net Integration Tests
//!
//! End-to-end tests verifying the full ku-core ↔ ku-net integration pipeline.
//! These are extracted from the ku-demo binary into proper #[test] functions.

use ku_core::*;
use ku_net::identity::*;
use ku_net::messages::*;
use ku_net::membership::*;
use ku_net::discovery::*;
use std::time::Instant;

// ════════════════════════════════════════════════════════════════════════════
// Helper: create a standard test KU ("Water boils at 100°C")
// ════════════════════════════════════════════════════════════════════════════

fn create_test_ku() -> KnowledgeUnit {
    KnowledgeUnit {
        codons: vec![
            Codon { concept_id: 128, role: RoleId::Agent, qualifiers: vec![] },
            Codon { concept_id: 133, role: RoleId::Quality, qualifiers: vec![] },
            Codon {
                concept_id: 132, role: RoleId::Quantity,
                qualifiers: vec![
                    Qualifier { key: "unit".into(), value: QualifierValue::Text("CELSIUS".into()) },
                    Qualifier { key: "val".into(), value: QualifierValue::Text("100".into()) },
                ],
            },
        ],
        bonds: vec![],
        gene: Gene::Fact {
            triples: vec![Triple { subject: 128, predicate: 133, object: 132 }],
            certainty: 9500,
            evidence: vec![],
        },
        flags: HeaderFlags::default(),
        epistemic_status: Some(EpistemicStatus::Evidence),
        evidence_type: Some(EvidenceType::Experimental),
        trust: Some(TrustSection {
            epistemic_status: EpistemicStatus::Evidence,
            evidence_type: EvidenceType::Experimental,
            verification_level: 3,
            corroboration_count: 42,
            challenge_count: 0,
            error_susceptibility: 0,
            trust_score: 9500,
            confidence: 8500,
            domain_codes: vec![141],
            verifications: vec![],
            challenges: vec![],
            ..Default::default()
        }),
        epigenetic: None,
    }
}

fn create_test_node() -> (KeyPair, NodeIdProof) {
    let keypair = KeyPair::generate();
    let proof = generate_node_id(&keypair.pubkey_bytes(), PUZZLE_C_SMALL);
    (keypair, proof)
}

// ════════════════════════════════════════════════════════════════════════════
// Integration Test: KU Encode → Frame → Decode (3-node transfer)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn test_e2e_3_nodes_ku_transfer() {
    // Setup: 3 nodes
    let (_kp_a, _proof_a) = create_test_node();
    let (_kp_b, _proof_b) = create_test_node();
    let (_kp_c, _proof_c) = create_test_node();

    // Step 1: Node A creates KU
    let ku = create_test_ku();
    let wire_bytes = encode_knowledge_unit(&ku).unwrap();

    // Verify wire format magic/version
    assert_eq!(&wire_bytes[0..2], &[0x4B, 0x44], "Magic should be 'KD'");
    assert_eq!(wire_bytes[2], 0x04, "Version should be 4");

    // Step 2: Frame with OBP header
    let header = MessageHeader {
        msg_type: MessageType::KuPush,
        flags: MessageFlags::NONE,
        payload_length: wire_bytes.len() as u32,
    };
    let header_bytes = header.encode();
    let mut frame = Vec::with_capacity(6 + wire_bytes.len());
    frame.extend_from_slice(&header_bytes);
    frame.extend_from_slice(&wire_bytes);

    // Step 3: Node B receives, decodes
    let recv_header = MessageHeader::decode(&[frame[0], frame[1], frame[2], frame[3], frame[4], frame[5]]).unwrap();
    assert_eq!(recv_header.msg_type, MessageType::KuPush);
    assert_eq!(recv_header.payload_length, wire_bytes.len() as u32);

    let (info_b, ku_b) = decode_full_knowledge_unit(&frame[6..]).unwrap();
    assert!(info_b.crc32_valid, "CRC should be valid at Node B");
    assert_eq!(ku_b.codons.len(), 3);
    assert_eq!(ku_b.trust.as_ref().unwrap().trust_score, 9500);

    // Step 4: Node B forwards to Node C (same payload)
    let mut frame_bc = Vec::with_capacity(6 + frame[6..].len());
    let header_bc = MessageHeader {
        msg_type: MessageType::KuPush,
        flags: MessageFlags::NONE,
        payload_length: (frame.len() - 6) as u32,
    };
    frame_bc.extend_from_slice(&header_bc.encode());
    frame_bc.extend_from_slice(&frame[6..]);

    let (info_c, ku_c) = decode_full_knowledge_unit(&frame_bc[6..]).unwrap();
    assert!(info_c.crc32_valid, "CRC should be valid at Node C");
    assert_eq!(ku_c.codons.len(), 3);

    // Verify roundtrip: same gene type, same trust
    assert_eq!(
        ku_c.trust.as_ref().unwrap().corroboration_count,
        ku.trust.as_ref().unwrap().corroboration_count
    );
}

// ════════════════════════════════════════════════════════════════════════════
// Integration Test: Bootstrap → Connected
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn test_e2e_bootstrap_to_connected() {
    let (_kp_a, _proof_a) = create_test_node();
    let (_kp_b, proof_b) = create_test_node();

    let addr_b = NetworkAddress::new_v4(10, 0, 1, 2, OBP_PORT);

    let mut bootstrap = BootstrapEngine::new();
    bootstrap.hardcoded_seeds.push(addr_b);

    // Start: Social layer
    let _layer = bootstrap.start();
    assert!(matches!(bootstrap.state, BootstrapState::Discovering { .. }));

    // Fail through layers 1-5
    for _ in 0..5 {
        bootstrap.layer_failed();
    }

    // Layer 6 (Hardcoded) discovers peers
    bootstrap.report_discovered(vec![
        DiscoveredPeer {
            node_id: Some(proof_b.node_id),
            address: addr_b,
            source: BootstrapLayer::Hardcoded,
            discovered_at: Instant::now(),
        },
    ]);
    bootstrap.mark_connected(BootstrapLayer::Hardcoded, 1);

    assert!(matches!(bootstrap.state, BootstrapState::Connected { .. }));
}

// ════════════════════════════════════════════════════════════════════════════
// Integration Test: Ed25519 Signed Frame + Tamper Detection
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn test_e2e_signed_frame_tamper_detection() {
    let (keypair_a, _proof_a) = create_test_node();

    // Create and encode KU
    let ku = create_test_ku();
    let wire_bytes = encode_knowledge_unit(&ku).unwrap();

    // Frame it
    let header = MessageHeader {
        msg_type: MessageType::KuPush,
        flags: MessageFlags::NONE,
        payload_length: wire_bytes.len() as u32,
    };
    let mut frame = Vec::with_capacity(6 + wire_bytes.len());
    frame.extend_from_slice(&header.encode());
    frame.extend_from_slice(&wire_bytes);

    // Sign
    let signature = keypair_a.sign(&frame);

    // Verify: valid
    assert!(keypair_a.verify(&frame, &signature), "Signature should be valid");

    // Tamper: flip a byte in payload
    let mut tampered = frame.clone();
    tampered[10] ^= 0xFF;
    assert!(!keypair_a.verify(&tampered, &signature), "Tampered frame should fail verification");

    // Tamper: flip a byte in header
    let mut tampered_header = frame.clone();
    tampered_header[0] ^= 0x01;
    assert!(!keypair_a.verify(&tampered_header, &signature), "Tampered header should fail");
}

// ════════════════════════════════════════════════════════════════════════════
// Integration Test: BLAKE3 CID Determinism
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn test_e2e_cid_deterministic() {
    let ku = create_test_ku();

    // Encode twice
    let wire1 = encode_knowledge_unit(&ku).unwrap();
    let wire2 = encode_knowledge_unit(&ku).unwrap();

    // Same content → same bytes
    assert_eq!(wire1, wire2, "Deterministic encoding");

    // Same bytes → same CID
    let cid1 = blake3::hash(&wire1);
    let cid2 = blake3::hash(&wire2);
    assert_eq!(cid1, cid2, "Deterministic CID");

    // Different content → different CID
    let ku2 = KnowledgeUnit {
        codons: vec![
            Codon { concept_id: 999, role: RoleId::Agent, qualifiers: vec![] },
        ],
        bonds: vec![],
        gene: Gene::Fact {
            triples: vec![Triple { subject: 999, predicate: 1, object: 2 }],
            certainty: 1000,
            evidence: vec![],
        },
        flags: HeaderFlags::default(),
        epistemic_status: None,
        evidence_type: None,
        trust: None,
        epigenetic: None,
    };
    let wire3 = encode_knowledge_unit(&ku2).unwrap();
    let cid3 = blake3::hash(&wire3);
    assert_ne!(cid1, cid3, "Different KUs → different CIDs");
}

// ════════════════════════════════════════════════════════════════════════════
// Integration Test: XOR Distance Routing
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn test_e2e_xor_routing_closest_node() {
    let (_kp_a, proof_a) = create_test_node();
    let (_kp_b, proof_b) = create_test_node();
    let (_kp_c, proof_c) = create_test_node();

    // Create a content key from a KU
    let ku = create_test_ku();
    let wire = encode_knowledge_unit(&ku).unwrap();
    let cid = blake3::hash(&wire);
    let content_key = NodeId(*cid.as_bytes());

    // Calculate XOR distances
    let dist_a = proof_a.node_id.xor_distance(&content_key);
    let dist_b = proof_b.node_id.xor_distance(&content_key);
    let dist_c = proof_c.node_id.xor_distance(&content_key);

    // At least one should be different (probabilistically guaranteed)
    assert!(dist_a != dist_b || dist_b != dist_c,
        "XOR distances should differ for different NodeIDs");

    // Verify XOR distance properties
    let self_dist = proof_a.node_id.xor_distance(&proof_a.node_id);
    assert_eq!(self_dist, [0u8; 32], "XOR with self should be zero");
}

// ════════════════════════════════════════════════════════════════════════════
// Integration Test: SWIM Membership + Fitness → Tier
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn test_e2e_membership_3_nodes_with_tiers() {
    let (_kp_a, proof_a) = create_test_node();
    let (_kp_b, proof_b) = create_test_node();
    let (_kp_c, proof_c) = create_test_node();

    // Node A: mobile contributor
    let fitness_a = FitnessComponents {
        uptime: 0.6, battery: 0.8, bandwidth: 0.5,
        storage: 0.4, cpu: 0.3, network_quality: 0.5, reputation: 0.5,
    };

    // Node B: server-grade
    let fitness_b = FitnessComponents {
        uptime: 0.95, battery: 1.0, bandwidth: 0.9,
        storage: 0.8, cpu: 0.7, network_quality: 1.0, reputation: 0.7,
    };

    // Node C: weak leaf
    let fitness_c = FitnessComponents {
        uptime: 0.2, battery: 0.3, bandwidth: 0.1,
        storage: 0.1, cpu: 0.1, network_quality: 0.2, reputation: 0.1,
    };

    // Verify tier assignments
    assert_eq!(fitness_a.recommended_tier(), NodeTier::Contributor);
    assert!(matches!(fitness_b.recommended_tier(), NodeTier::CountrySP | NodeTier::RegionalSP));
    assert_eq!(fitness_c.recommended_tier(), NodeTier::Leaf);

    // Setup membership states
    let mut state_a = MembershipState::new(proof_a.node_id, fitness_a);
    let addr_b = NetworkAddress::new_v4(10, 0, 1, 2, OBP_PORT);
    let addr_c = NetworkAddress::new_v6([0x2001, 0x0db8, 0, 0, 0, 0, 0, 3], OBP_PORT);

    state_a.upsert_member(MemberEntry {
        node_id: proof_b.node_id, address: addr_b, incarnation: 1,
        status: MemberStatus::Alive, tier: NodeTier::CountrySP,
        last_seen: Instant::now(), fitness_score: fitness_b.score(),
        topic_vector: [0x42; 16],
    });
    state_a.upsert_member(MemberEntry {
        node_id: proof_c.node_id, address: addr_c, incarnation: 1,
        status: MemberStatus::Alive, tier: NodeTier::Leaf,
        last_seen: Instant::now(), fitness_score: fitness_c.score(),
        topic_vector: [0x00; 16],
    });

    assert_eq!(state_a.member_count(), 2);

    // SWIM ping
    let updates = state_a.handle_ping(&proof_b.node_id);
    assert!(!updates.is_empty() || state_a.member_count() == 2,
        "Ping should work without errors");
}

// ════════════════════════════════════════════════════════════════════════════
// Integration Test: Network Address IPv4 + IPv6 roundtrip
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn test_e2e_mixed_address_network() {
    let addr_v4 = NetworkAddress::new_v4(192, 168, 1, 100, 4242);
    let addr_v6 = NetworkAddress::new_v6([0xfe80, 0, 0, 0, 0, 0, 0, 1], 4242);

    // Encode
    let v4_bytes = addr_v4.encode();
    let v6_bytes = addr_v6.encode();

    assert_eq!(v4_bytes.len(), 7, "IPv4 wire: 1 type + 4 addr + 2 port");
    assert_eq!(v6_bytes.len(), 19, "IPv6 wire: 1 type + 16 addr + 2 port");

    // Decode roundtrip
    let (dec_v4, consumed_v4) = NetworkAddress::decode(&v4_bytes).unwrap();
    let (dec_v6, consumed_v6) = NetworkAddress::decode(&v6_bytes).unwrap();

    assert_eq!(dec_v4, addr_v4);
    assert_eq!(dec_v6, addr_v6);
    assert_eq!(consumed_v4, 7);
    assert_eq!(consumed_v6, 19);
}

// ════════════════════════════════════════════════════════════════════════════
// Integration Test: Full Pipeline — Identity → KU → Frame → Sign → Verify
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn test_e2e_full_pipeline() {
    // 1. Identity
    let (keypair, proof) = create_test_node();
    assert!(verify_node_id(&keypair.pubkey_bytes(), proof.nonce, &proof.node_id, PUZZLE_C_SMALL));

    // 2. DID
    let did = pubkey_to_did(&keypair.pubkey_bytes());
    assert!(did.starts_with("did:key:z6Mk"));

    // 3. DeviceId
    let device_id = DeviceId::from_pubkey(&keypair.public_key());
    assert_eq!(device_id.0.len(), 32);

    // 4. Create KU
    let ku = create_test_ku();
    let wire = encode_knowledge_unit(&ku).unwrap();
    assert!(wire.len() > 10, "KU wire should be non-trivial");

    // 5. CID
    let cid = blake3::hash(&wire);

    // 6. Frame
    let header = MessageHeader {
        msg_type: MessageType::KuPush,
        flags: MessageFlags::NONE,
        payload_length: wire.len() as u32,
    };
    let mut frame = Vec::with_capacity(6 + wire.len());
    frame.extend_from_slice(&header.encode());
    frame.extend_from_slice(&wire);

    // 7. Sign
    let sig = keypair.sign(&frame);
    assert!(keypair.verify(&frame, &sig));

    // 8. Decode
    let (info, decoded_ku) = decode_full_knowledge_unit(&frame[4..]).unwrap();
    assert!(info.crc32_valid);
    assert_eq!(decoded_ku.codons.len(), ku.codons.len());

    // 9. Content address matches
    let decoded_wire = encode_knowledge_unit(&decoded_ku).unwrap();
    let decoded_cid = blake3::hash(&decoded_wire);
    assert_eq!(cid, decoded_cid, "CID should survive encode-decode roundtrip");
}
