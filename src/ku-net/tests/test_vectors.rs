//! # OBP Wire Format Test Vectors
//!
//! Hardcoded byte-level verification ensuring binary compatibility.
//! These vectors can be used by any OBP implementation to verify conformance.

use ku_net::identity::*;
use ku_net::messages::*;

// ════════════════════════════════════════════════════════════════════════════
// TV-1: MessageHeader encoding
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn tv01_message_header_ku_push_264() {
    // KU_PUSH, no flags, payload=264 bytes
    let header = MessageHeader {
        msg_type: MessageType::KuPush,
        flags: MessageFlags::NONE,
        payload_length: 264,
    };
    let bytes = header.encode();
    // msg_type=0x01, flags=0x00, length=264 (0x00000108 BE)
    assert_eq!(bytes, [0x01, 0x00, 0x00, 0x00, 0x01, 0x08]);
}

#[test]
fn tv02_message_header_swim_ping() {
    // SWIM_PING, no flags, payload=0
    let header = MessageHeader {
        msg_type: MessageType::SwimPing,
        flags: MessageFlags::NONE,
        payload_length: 0,
    };
    let bytes = header.encode();
    assert_eq!(bytes, [0x10, 0x00, 0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn tv03_message_header_large_payload() {
    // KU_PULL, no flags, payload=65535
    let header = MessageHeader {
        msg_type: MessageType::KuPull,
        flags: MessageFlags::NONE,
        payload_length: 65535,
    };
    let bytes = header.encode();
    assert_eq!(bytes, [0x02, 0x00, 0x00, 0x00, 0xFF, 0xFF]);
}

// ════════════════════════════════════════════════════════════════════════════
// TV-2: NetworkAddress IPv4 encoding
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn tv04_network_address_ipv4_loopback() {
    // 127.0.0.1:4242
    let addr = NetworkAddress::new_v4(127, 0, 0, 1, 4242);
    let bytes = addr.encode();
    // type=0x04, ip=[7F,00,00,01], port=4242 (0x1092 BE)
    assert_eq!(bytes, [0x04, 0x7F, 0x00, 0x00, 0x01, 0x10, 0x92]);
    assert_eq!(bytes.len(), 7);

    // Roundtrip
    let (decoded, consumed) = NetworkAddress::decode(&bytes).unwrap();
    assert_eq!(decoded, addr);
    assert_eq!(consumed, 7);
}

#[test]
fn tv05_network_address_ipv4_private() {
    // 10.0.1.1:4242
    let addr = NetworkAddress::new_v4(10, 0, 1, 1, 4242);
    let bytes = addr.encode();
    assert_eq!(bytes, [0x04, 0x0A, 0x00, 0x01, 0x01, 0x10, 0x92]);
}

// ════════════════════════════════════════════════════════════════════════════
// TV-3: NetworkAddress IPv6 encoding
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn tv06_network_address_ipv6() {
    // 2001:db8::3:4242
    let addr = NetworkAddress::new_v6([0x2001, 0x0db8, 0, 0, 0, 0, 0, 3], 4242);
    let bytes = addr.encode();
    assert_eq!(bytes.len(), 19); // 1 type + 16 addr + 2 port
    assert_eq!(bytes[0], 0x06); // IPv6 type
                                // First 4 bytes of IPv6 addr: 2001:0db8
    assert_eq!(bytes[1..5], [0x20, 0x01, 0x0D, 0xB8]);
    // Last 2 bytes of IPv6 addr: 0003
    assert_eq!(bytes[15..17], [0x00, 0x03]);
    // Port: 4242
    assert_eq!(bytes[17..19], [0x10, 0x92]);

    // Roundtrip
    let (decoded, consumed) = NetworkAddress::decode(&bytes).unwrap();
    assert_eq!(decoded, addr);
    assert_eq!(consumed, 19);
}

// ════════════════════════════════════════════════════════════════════════════
// TV-4: NodeID BLAKE3 puzzle
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn tv07_node_id_blake3_deterministic() {
    // Known pubkey → deterministic hash
    let pubkey = [0xAA; 32];
    let nonce: u64 = 0;

    // BLAKE3(pubkey || nonce_le_bytes) should always produce same result
    let mut hasher = blake3::Hasher::new();
    hasher.update(&pubkey);
    hasher.update(&nonce.to_le_bytes());
    let hash = hasher.finalize();

    // Verify it's deterministic (run twice)
    let mut hasher2 = blake3::Hasher::new();
    hasher2.update(&pubkey);
    hasher2.update(&nonce.to_le_bytes());
    let hash2 = hasher2.finalize();

    assert_eq!(hash, hash2, "BLAKE3 must be deterministic");
    assert_eq!(hash.as_bytes().len(), 32);
}

#[test]
fn tv08_node_id_verification() {
    let pubkey = [0xBB; 32];

    // Generate with known difficulty
    let proof = generate_node_id(&pubkey, PUZZLE_C_SMALL);

    // Verify succeeds with correct params
    assert!(verify_node_id(
        &pubkey,
        proof.nonce,
        &proof.node_id,
        PUZZLE_C_SMALL
    ));

    // Verify fails with wrong pubkey
    let wrong_pubkey = [0xCC; 32];
    assert!(!verify_node_id(
        &wrong_pubkey,
        proof.nonce,
        &proof.node_id,
        PUZZLE_C_SMALL
    ));

    // Verify fails with wrong nonce
    assert!(!verify_node_id(
        &pubkey,
        proof.nonce + 1,
        &proof.node_id,
        PUZZLE_C_SMALL
    ));
}

// ════════════════════════════════════════════════════════════════════════════
// TV-5: KU Wire Format (ku-core)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn tv09_ku_wire_magic_version() {
    use ku_core::*;

    let ku = KnowledgeUnit {
        codons: vec![Codon {
            concept_id: 1,
            role: RoleId::Agent,
            qualifiers: vec![],
        }],
        bonds: vec![],
        gene: Gene::Fact {
            triples: vec![Triple {
                subject: 1,
                predicate: 2,
                object: 3,
            }],
            certainty: 5000,
            evidence: vec![],
        },
        flags: HeaderFlags::default(),
        epistemic_status: None,
        evidence_type: None,
        trust: None,
        epigenetic: None,
    };

    let wire = encode_knowledge_unit(&ku).unwrap();

    // Magic: "KD" = 0x4B44
    assert_eq!(wire[0], 0x4B, "Magic byte 0 = 'K'");
    assert_eq!(wire[1], 0x44, "Magic byte 1 = 'D'");
    // Version is owned by ku-core's current KU wire profile.
    assert_eq!(wire[2], VERSION, "Version matches ku-core");
    // Flags byte present
    assert!(
        wire.len() >= 7,
        "Min wire size: 2 magic + 1 ver + 1 flags + 4 len + 4 crc = 12"
    );

    // Last 4 bytes are CRC-32
    let crc_start = wire.len() - 4;
    let payload_end = crc_start;
    let computed_crc = crc32fast::hash(&wire[..payload_end]);
    let stored_crc = u32::from_be_bytes([
        wire[crc_start],
        wire[crc_start + 1],
        wire[crc_start + 2],
        wire[crc_start + 3],
    ]);
    assert_eq!(computed_crc, stored_crc, "CRC-32 must match");
}

// ════════════════════════════════════════════════════════════════════════════
// TV-6: Message Type ID ranges
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn tv10_message_type_id_ranges() {
    // Layer 0: Data (0x01-0x0F)
    assert!((MessageType::KuPush as u8) >= 0x01 && (MessageType::KuPush as u8) <= 0x0F);

    // Layer 1: Membership (0x10-0x1F)
    assert!((MessageType::SwimPing as u8) >= 0x10 && (MessageType::SwimPing as u8) <= 0x1F);

    // Layer 2: DHT (0x20-0x2F)
    assert!((MessageType::FindNodeReq as u8) >= 0x20 && (MessageType::FindNodeReq as u8) <= 0x2F);

    // Layer 3: Content (0x30-0x3F)
    assert!((MessageType::VacuumFilter as u8) >= 0x30 && (MessageType::VacuumFilter as u8) <= 0x3F);

    // Layer 4: Watch/Trust (0x40-0x4F)
    assert!((MessageType::TrustGossip as u8) >= 0x40 && (MessageType::TrustGossip as u8) <= 0x4F);

    // Layer 5: Query (0x50-0x5F)
    assert!((MessageType::QueryForward as u8) >= 0x50 && (MessageType::QueryForward as u8) <= 0x5F);

    // Security (0x80+)
    assert!((MessageType::PowChallenge as u8) >= 0x80);
}

// ════════════════════════════════════════════════════════════════════════════
// TV-7: Header decode roundtrip for all important message types
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn tv11_header_roundtrip_all_layers() {
    let test_cases: Vec<(MessageType, u32)> = vec![
        (MessageType::KuPush, 100),
        (MessageType::KuPull, 32),
        (MessageType::SwimPing, 0),
        (MessageType::SwimPingReq, 64),
        (MessageType::FindNodeReq, 32),
        (MessageType::StoreReq, 512),
        (MessageType::VacuumFilter, 256),
        (MessageType::QueryForward, 1024),
        (MessageType::TrustGossip, 128),
        (MessageType::PowChallenge, 48),
    ];

    for (msg_type, payload_len) in test_cases {
        let header = MessageHeader {
            msg_type,
            flags: MessageFlags::NONE,
            payload_length: payload_len,
        };
        let encoded = header.encode();
        let decoded = MessageHeader::decode(&encoded).unwrap();
        assert_eq!(
            decoded.msg_type, msg_type,
            "Type mismatch for {:?}",
            msg_type
        );
        assert_eq!(
            decoded.payload_length, payload_len,
            "Length mismatch for {:?}",
            msg_type
        );
    }
}

// ════════════════════════════════════════════════════════════════════════════
// TV-8: Ed25519 signature format
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn tv12_ed25519_signature_size() {
    let keypair = KeyPair::generate();
    let message = b"OBP test vector";
    let signature = keypair.sign(message);

    // Ed25519 signature is always 64 bytes
    assert_eq!(signature.to_bytes().len(), 64);

    // Public key is always 32 bytes
    assert_eq!(keypair.pubkey_bytes().len(), 32);
}
