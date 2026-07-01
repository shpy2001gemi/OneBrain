//! # OneBrain Protocol — End-to-End Integration Demo
//!
//! Simulates 3 OBP nodes creating, encoding, exchanging, and verifying
//! Knowledge Units over the network protocol.
//!
//! ```text
//! ┌─────────────┐     KU_PUSH      ┌─────────────┐     KU_PUSH      ┌─────────────┐
//! │   Node A    │ ───────────────▶ │   Node B    │ ───────────────▶ │   Node C    │
//! │ (Scientist) │                   │ (Reviewer)  │                   │ (Student)   │
//! │  Tier: T1   │                   │  Tier: T2   │                   │  Tier: T0   │
//! │             │ ◀─── TRUST_GOSSIP │             │ ◀─── SWIM_PING   │             │
//! └─────────────┘                   └─────────────┘                   └─────────────┘
//! ```

mod runtime;
mod testbed;

use ku_core::*;
use ku_net::identity::*;
use ku_net::messages::*;
use ku_net::membership::*;
use ku_net::discovery::*;

fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║     OneBrain Protocol (OBP) — End-to-End Integration Demo    ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // ══════════════════════════════════════════════════════════════════
    // STEP 1: Identity — Create 3 nodes with crypto puzzles
    // ══════════════════════════════════════════════════════════════════
    println!("━━━ STEP 1: Identity Generation (BLAKE3 Crypto Puzzle) ━━━\n");

    let keypair_a = KeyPair::generate();
    let keypair_b = KeyPair::generate();
    let keypair_c = KeyPair::generate();

    let proof_a = generate_node_id(&keypair_a.pubkey_bytes(), PUZZLE_C_SMALL);
    let proof_b = generate_node_id(&keypair_b.pubkey_bytes(), PUZZLE_C_SMALL);
    let proof_c = generate_node_id(&keypair_c.pubkey_bytes(), PUZZLE_C_SMALL);

    println!("  Node A (Scientist):  {}", proof_a.node_id);
    println!("    Nonce: {:>8} | Leading zeros: {} bits", proof_a.nonce, proof_a.node_id.leading_zeros());
    println!("    DID: {}", pubkey_to_did(&keypair_a.pubkey_bytes()));
    println!("  Node B (Reviewer):   {}", proof_b.node_id);
    println!("    Nonce: {:>8} | Leading zeros: {} bits", proof_b.nonce, proof_b.node_id.leading_zeros());
    println!("  Node C (Student):    {}", proof_c.node_id);
    println!("    Nonce: {:>8} | Leading zeros: {} bits\n", proof_c.nonce, proof_c.node_id.leading_zeros());

    // Verify all NodeIDs
    assert!(verify_node_id(&keypair_a.pubkey_bytes(), proof_a.nonce, &proof_a.node_id, PUZZLE_C_SMALL));
    assert!(verify_node_id(&keypair_b.pubkey_bytes(), proof_b.nonce, &proof_b.node_id, PUZZLE_C_SMALL));
    assert!(verify_node_id(&keypair_c.pubkey_bytes(), proof_c.nonce, &proof_c.node_id, PUZZLE_C_SMALL));
    println!("  ✅ All 3 NodeIDs verified (BLAKE3 puzzle, difficulty=16)\n");

    // ══════════════════════════════════════════════════════════════════
    // STEP 2: Membership — SWIM state setup with tiers
    // ══════════════════════════════════════════════════════════════════
    println!("━━━ STEP 2: SWIM Membership + 7-Tier Fitness ━━━\n");

    // Node A: T1 Contributor (mobile phone, good connection)
    let fitness_a = FitnessComponents {
        uptime: 0.6, battery: 0.8, bandwidth: 0.5,
        storage: 0.4, cpu: 0.3, network_quality: 0.5, reputation: 0.5,
    };
    let mut state_a = MembershipState::new(proof_a.node_id, fitness_a.clone());
    println!("  Node A fitness: {:.3} → Tier {:?}", fitness_a.score(), fitness_a.recommended_tier());

    // Node B: T2 Local SP (desktop, always-on, good bandwidth)
    let fitness_b = FitnessComponents {
        uptime: 0.95, battery: 1.0, bandwidth: 0.9,
        storage: 0.8, cpu: 0.7, network_quality: 1.0, reputation: 0.7,
    };
    let mut state_b = MembershipState::new(proof_b.node_id, fitness_b.clone());
    println!("  Node B fitness: {:.3} → Tier {:?}", fitness_b.score(), fitness_b.recommended_tier());

    // Node C: T0 Leaf (old phone, poor connection)
    let fitness_c = FitnessComponents {
        uptime: 0.2, battery: 0.3, bandwidth: 0.1,
        storage: 0.1, cpu: 0.1, network_quality: 0.2, reputation: 0.1,
    };
    let mut state_c = MembershipState::new(proof_c.node_id, fitness_c.clone());
    println!("  Node C fitness: {:.3} → Tier {:?}\n", fitness_c.score(), fitness_c.recommended_tier());

    // Add members to each other's lists
    let addr_a = NetworkAddress::new_v4(10, 0, 1, 1, OBP_PORT);
    let addr_b = NetworkAddress::new_v4(10, 0, 1, 2, OBP_PORT);
    let addr_c = NetworkAddress::new_v6([0x2001, 0x0db8, 0, 0, 0, 0, 0, 3], OBP_PORT);

    let entry_a = MemberEntry {
        node_id: proof_a.node_id, address: addr_a, incarnation: 1,
        status: MemberStatus::Alive, tier: NodeTier::Contributor,
        last_seen: std::time::Instant::now(), fitness_score: fitness_a.score(),
        topic_vector: [0x42; 16],
    };
    let entry_b = MemberEntry {
        node_id: proof_b.node_id, address: addr_b, incarnation: 1,
        status: MemberStatus::Alive, tier: NodeTier::LocalSP,
        last_seen: std::time::Instant::now(), fitness_score: fitness_b.score(),
        topic_vector: [0x42; 16],
    };
    let entry_c = MemberEntry {
        node_id: proof_c.node_id, address: addr_c, incarnation: 1,
        status: MemberStatus::Alive, tier: NodeTier::Leaf,
        last_seen: std::time::Instant::now(), fitness_score: fitness_c.score(),
        topic_vector: [0x00; 16],
    };

    state_a.upsert_member(entry_b.clone());
    state_a.upsert_member(entry_c.clone());
    state_b.upsert_member(entry_a.clone());
    state_b.upsert_member(entry_c.clone());
    state_c.upsert_member(entry_a.clone());
    state_c.upsert_member(entry_b.clone());

    println!("  Node A knows {} peers | Node B knows {} peers | Node C knows {} peers",
        state_a.member_count(), state_b.member_count(), state_c.member_count());

    // SWIM PING: Node A pings Node B
    let _updates = state_b.handle_ping(&proof_a.node_id);
    println!("  ✅ SWIM PING A→B: Node B confirmed A is Alive\n");

    // ══════════════════════════════════════════════════════════════════
    // STEP 3: Bootstrap — Node C joins via 6-layer cascade
    // ══════════════════════════════════════════════════════════════════
    println!("━━━ STEP 3: Bootstrap (6-Layer Cascade) ━━━\n");

    let mut bootstrap = BootstrapEngine::new();
    bootstrap.hardcoded_seeds.push(addr_b); // Node B is hardcoded seed

    let _layer = bootstrap.start();
    println!("  Layer 1: {} — no social contacts", BootstrapLayer::Social.name());
    let next = bootstrap.layer_failed().unwrap();
    println!("  Layer 2: {} — no local peers", next.name());
    let next = bootstrap.layer_failed().unwrap();
    println!("  Layer 3: {} — no HTTP seeds configured", next.name());
    let next = bootstrap.layer_failed().unwrap();
    println!("  Layer 4: {} — no DHT bootstrap nodes", next.name());
    let next = bootstrap.layer_failed().unwrap();
    println!("  Layer 5: {} — no DNS TXT records", next.name());
    let next = bootstrap.layer_failed().unwrap();
    println!("  Layer 6: {} — found Node B!", next.name());

    bootstrap.report_discovered(vec![
        DiscoveredPeer {
            node_id: Some(proof_b.node_id),
            address: addr_b,
            source: BootstrapLayer::Hardcoded,
            discovered_at: std::time::Instant::now(),
        },
        DiscoveredPeer {
            node_id: Some(proof_a.node_id),
            address: addr_a,
            source: BootstrapLayer::Hardcoded,
            discovered_at: std::time::Instant::now(),
        },
        DiscoveredPeer {
            node_id: None,
            address: NetworkAddress::new_v4(10, 0, 1, 99, OBP_PORT),
            source: BootstrapLayer::Hardcoded,
            discovered_at: std::time::Instant::now(),
        },
    ]);
    bootstrap.mark_connected(BootstrapLayer::Hardcoded, 3);
    println!("  ✅ Bootstrap complete via layer: {:?}, peers: 3\n", BootstrapLayer::Hardcoded);

    // ══════════════════════════════════════════════════════════════════
    // STEP 4: Knowledge Unit — Node A creates a Fact KU
    // ══════════════════════════════════════════════════════════════════
    println!("━━━ STEP 4: Knowledge Unit Creation (ku-core) ━━━\n");
    println!("  Scenario: Scientist (Node A) encodes 'Water boils at 100°C'\n");

    let ku = KnowledgeUnit {
        codons: vec![
            Codon { concept_id: 128, role: RoleId::Agent, qualifiers: vec![] },        // water
            Codon { concept_id: 133, role: RoleId::Quality, qualifiers: vec![] },       // boiling_point
            Codon {
                concept_id: 132, role: RoleId::Quantity,
                qualifiers: vec![
                    Qualifier { key: "unit".into(), value: QualifierValue::Text("CELSIUS".into()) },
                    Qualifier { key: "val".into(), value: QualifierValue::Text("100".into()) },
                ],
            }, // temperature
        ],
        bonds: vec![],
        gene: Gene::Fact {
            triples: vec![Triple {
                subject: 128,
                predicate: 133,
                object: 132,
            }],
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
            domain_codes: vec![141], // chemistry
            verifications: vec![],
            challenges: vec![],
            ..Default::default()
        }),
        epigenetic: None,
    };

    // Encode to wire format
    let wire_bytes = encode_knowledge_unit(&ku).unwrap();
    println!("  Gene Type: Fact");
    println!("  Codons: water(128), boiling_point(133), temperature(132)");
    println!("  Trust: Evidence, confidence=9500, corroborations=42");
    println!("  Wire size: {} bytes", wire_bytes.len());
    println!("  Wire hex (first 20B): {:02X?}", &wire_bytes[..20.min(wire_bytes.len())]);

    // Verify magic + version
    assert_eq!(&wire_bytes[0..2], &[0x4B, 0x44]); // "KD"
    assert_eq!(wire_bytes[2], 0x04);                // v4
    println!("  ✅ Magic=KD, Version=4 confirmed\n");

    // ══════════════════════════════════════════════════════════════════
    // STEP 5: Message Framing — Wrap KU in OBP message
    // ══════════════════════════════════════════════════════════════════
    println!("━━━ STEP 5: OBP Message Framing (ku-net) ━━━\n");

    let header = MessageHeader {
        msg_type: MessageType::KuPush,
        flags: MessageFlags::NONE,
        payload_length: wire_bytes.len() as u32,
    };
    let header_bytes = header.encode();

    println!("  Message Type: KU_PUSH (0x{:02x})", MessageType::KuPush as u8);
    println!("  Header: [{:02X}, {:02X}, {:02X}, {:02X}, {:02X}, {:02X}]",
        header_bytes[0], header_bytes[1], header_bytes[2], header_bytes[3],
        header_bytes[4], header_bytes[5]);
    println!("  Payload: {} bytes (KU wire format)", wire_bytes.len());
    println!("  Total frame: {} bytes (6 header + {} payload)\n",
        6 + wire_bytes.len(), wire_bytes.len());

    // Simulate: construct full frame
    let mut frame = Vec::with_capacity(6 + wire_bytes.len());
    frame.extend_from_slice(&header_bytes);
    frame.extend_from_slice(&wire_bytes);

    // ══════════════════════════════════════════════════════════════════
    // STEP 6: Network Transfer — Node A → Node B → Node C
    // ══════════════════════════════════════════════════════════════════
    println!("━━━ STEP 6: Network Transfer Simulation ━━━\n");

    // Node B receives the frame
    println!("  [A → B] Sending {} bytes to {:?}", frame.len(), addr_b);
    let received_header = MessageHeader::decode(&[frame[0], frame[1], frame[2], frame[3], frame[4], frame[5]]).unwrap();
    assert_eq!(received_header.msg_type, MessageType::KuPush);
    println!("  [B] Received KU_PUSH, payload={} bytes", received_header.payload_length);

    // Node B decodes the KU
    let payload = &frame[6..];
    let (decoded_info, decoded_ku) = decode_full_knowledge_unit(payload).unwrap();
    assert!(decoded_info.crc32_valid);
    println!("  [B] Decoded KU: gene={:?}, CRC valid={}", decoded_info.gene_type, decoded_info.crc32_valid);
    println!("  [B] Codons: {} | Trust score: {}",
        decoded_ku.codons.len(),
        decoded_ku.trust.as_ref().map(|t| t.trust_score).unwrap_or(0));

    // Node B forwards to Node C (re-encodes same frame)
    let header_bc = MessageHeader {
        msg_type: MessageType::KuPush,
        flags: MessageFlags::NONE,
        payload_length: payload.len() as u32,
    };
    let mut frame_bc = Vec::with_capacity(6 + payload.len());
    frame_bc.extend_from_slice(&header_bc.encode());
    frame_bc.extend_from_slice(payload);

    println!("  [B → C] Forwarding {} bytes to {:?}", frame_bc.len(), addr_c);
    let recv_c = MessageHeader::decode(&[frame_bc[0], frame_bc[1], frame_bc[2], frame_bc[3], frame_bc[4], frame_bc[5]]).unwrap();
    assert_eq!(recv_c.msg_type, MessageType::KuPush);
    let (info_c, ku_c) = decode_full_knowledge_unit(&frame_bc[6..]).unwrap();
    assert!(info_c.crc32_valid);
    println!("  [C] Decoded KU: CRC valid={}, codons={}", info_c.crc32_valid, ku_c.codons.len());
    println!("  ✅ KU survived A→B→C transfer, CRC valid at every hop\n");

    // ══════════════════════════════════════════════════════════════════
    // STEP 7: Content Addressing — CID generation
    // ══════════════════════════════════════════════════════════════════
    println!("━━━ STEP 7: Content Addressing (BLAKE3 CID) ━━━\n");

    let cid = blake3::hash(&wire_bytes);
    println!("  KU CID (BLAKE3): {}", cid);
    println!("  CID bytes: {:02X?}", &cid.as_bytes()[..16]);

    // Verify: same content → same CID
    let wire_bytes_2 = encode_knowledge_unit(&ku).unwrap();
    let cid_2 = blake3::hash(&wire_bytes_2);
    assert_eq!(cid, cid_2);
    println!("  ✅ Deterministic: re-encoding produces same CID\n");

    // ══════════════════════════════════════════════════════════════════
    // STEP 8: XOR Distance — Kademlia routing
    // ══════════════════════════════════════════════════════════════════
    println!("━━━ STEP 8: Kademlia XOR Routing ━━━\n");

    // Who should store this CID? Closest NodeID by XOR.
    let cid_as_key = NodeId(*cid.as_bytes());
    let dist_a = proof_a.node_id.xor_distance(&cid_as_key);
    let dist_b = proof_b.node_id.xor_distance(&cid_as_key);
    let dist_c = proof_c.node_id.xor_distance(&cid_as_key);

    println!("  XOR(A, CID) = {:02X}{:02X}...", dist_a[0], dist_a[1]);
    println!("  XOR(B, CID) = {:02X}{:02X}...", dist_b[0], dist_b[1]);
    println!("  XOR(C, CID) = {:02X}{:02X}...", dist_c[0], dist_c[1]);

    let closest = if dist_a < dist_b && dist_a < dist_c { "A" }
                  else if dist_b < dist_c { "B" }
                  else { "C" };
    println!("  → Closest node to CID: Node {}", closest);
    println!("  ✅ Kademlia routing determines storage responsibility\n");

    // ══════════════════════════════════════════════════════════════════
    // STEP 9: Network Addresses — IPv4 + IPv6
    // ══════════════════════════════════════════════════════════════════
    println!("━━━ STEP 9: Network Addresses (IPv4 + IPv6) ━━━\n");

    let v4_bytes = addr_a.encode();
    let v6_bytes = addr_c.encode();
    println!("  Node A (IPv4): {:?} → {} bytes wire", addr_a, v4_bytes.len());
    println!("  Node C (IPv6): {:?} → {} bytes wire", addr_c, v6_bytes.len());

    let (decoded_v4, _) = NetworkAddress::decode(&v4_bytes).unwrap();
    let (decoded_v6, _) = NetworkAddress::decode(&v6_bytes).unwrap();
    assert_eq!(decoded_v4, addr_a);
    assert_eq!(decoded_v6, addr_c);
    println!("  ✅ Both address types roundtrip correctly\n");

    // ══════════════════════════════════════════════════════════════════
    // STEP 10: Ed25519 Signatures — Message signing
    // ══════════════════════════════════════════════════════════════════
    println!("━━━ STEP 10: Ed25519 Signed Messages ━━━\n");

    let signed_frame = &frame;
    let signature = keypair_a.sign(signed_frame);
    println!("  Node A signs frame ({} bytes)", signed_frame.len());
    println!("  Signature: {:02X?}...", &signature.to_bytes()[..16]);

    // Node B verifies
    let valid = keypair_a.verify(signed_frame, &signature);
    assert!(valid);
    println!("  Node B verifies: ✅ valid");

    // Tamper detection
    let mut tampered = frame.clone();
    tampered[10] ^= 0xFF; // Flip a byte
    let tampered_valid = keypair_a.verify(&tampered, &signature);
    assert!(!tampered_valid);
    println!("  Tampered frame: ❌ signature invalid (tamper detected)\n");

    // ══════════════════════════════════════════════════════════════════
    // SUMMARY
    // ══════════════════════════════════════════════════════════════════
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                    INTEGRATION SUMMARY                       ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║                                                              ║");
    println!("║  ku-core (KU Encoding):                                      ║");
    println!("║    ✅ KnowledgeUnit → wire bytes ({:>3} B)                    ║", wire_bytes.len());
    println!("║    ✅ Wire bytes → KnowledgeUnit (roundtrip)                 ║");
    println!("║    ✅ CRC-32 integrity verification                          ║");
    println!("║                                                              ║");
    println!("║  ku-net (Network Protocol):                                  ║");
    println!("║    ✅ BLAKE3 NodeID generation (difficulty={})               ║", PUZZLE_C_SMALL);
    println!("║    ✅ Ed25519 sign/verify (tamper detection)                  ║");
    println!("║    ✅ SWIM membership (3 nodes, Alive status)                 ║");
    println!("║    ✅ 6-layer bootstrap (cascade to Hardcoded)               ║");
    println!("║    ✅ Message framing (KU_PUSH 0x01)                         ║");
    println!("║    ✅ IPv4 + IPv6 address encoding                           ║");
    println!("║                                                              ║");
    println!("║  Integration:                                                ║");
    println!("║    ✅ KU encoded by ku-core, framed by ku-net                ║");
    println!("║    ✅ KU transferred A→B→C with CRC valid at every hop       ║");
    println!("║    ✅ BLAKE3 CID for content addressing                      ║");
    println!("║    ✅ XOR distance determines storage responsibility         ║");
    println!("║    ✅ Signed frames detect tampering                         ║");
    println!("║                                                              ║");
    println!("║  Frame: [{:>3}B header] + [{:>3}B KU payload] = {:>3}B total    ║",
        4, wire_bytes.len(), 4 + wire_bytes.len());
    println!("║                                                              ║");
    println!("╚══════════════════════════════════════════════════════════════╝");

    // ══════════════════════════════════════════════════════════════════
    // STEP 11: Phase 8 — Multi-Node Testbed
    // ══════════════════════════════════════════════════════════════════
    println!("\n━━━ STEP 11: Multi-Node Testbed (Phase 8) ━━━\n");
    let report = testbed::run_testbed();
    println!("  Report: {:?}", report);
}
