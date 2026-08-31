//! # Cryptographic Identity — SPEC A §1-2
//!
//! NodeID generation with adaptive crypto puzzle (BLAKE3),
//! Ed25519 keypair management, and DID format.

use crate::error::IdentityError;
use blake3;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use std::fmt;

// ─── Constants (SPEC A §1.2) ───────────────────────────────────────────────

/// Crypto puzzle difficulty for small networks (<1M nodes).
pub const PUZZLE_C_SMALL: u8 = 16;
/// Crypto puzzle difficulty for medium networks (1M–1B nodes).
pub const PUZZLE_C_MEDIUM: u8 = 20;
/// Crypto puzzle difficulty for large networks (>1B nodes).
pub const PUZZLE_C_LARGE: u8 = 24;
/// Maximum devices per DID identity.
pub const DEVICE_GROUP_MAX: u16 = 16;
/// ALPN protocol identifier for OBP QUIC connections.
pub const OBP_ALPN: &[u8] = b"obp/1";
/// Default QUIC port.
pub const OBP_PORT: u16 = 4242;

// ─── NodeId ────────────────────────────────────────────────────────────────

/// 32-byte node identifier derived from BLAKE3(pubkey || nonce).
/// Must satisfy crypto puzzle: leading_zeros(NodeId) >= difficulty.
///
/// SPEC A §1.2: `node_id = BLAKE3(pubkey || puzzle_nonce)[0..32]`
#[derive(Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct NodeId(pub [u8; 32]);

impl NodeId {
    /// Count leading zero bits in the NodeId.
    pub fn leading_zeros(&self) -> u32 {
        let mut count = 0u32;
        for byte in &self.0 {
            if *byte == 0 {
                count += 8;
            } else {
                count += byte.leading_zeros();
                break;
            }
        }
        count
    }

    /// XOR distance to another NodeId (Kademlia metric).
    pub fn xor_distance(&self, other: &NodeId) -> [u8; 32] {
        let mut dist = [0u8; 32];
        for (i, byte) in dist.iter_mut().enumerate() {
            *byte = self.0[i] ^ other.0[i];
        }
        dist
    }

    /// Check if this NodeId satisfies the given puzzle difficulty.
    pub fn satisfies_difficulty(&self, difficulty: u8) -> bool {
        self.leading_zeros() >= difficulty as u32
    }
}

impl fmt::Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NodeId({}...)", hex_prefix(&self.0, 8))
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

// ─── DeviceId ──────────────────────────────────────────────────────────────

/// Device identifier: BLAKE3(device_pubkey)[0..32].
/// Used in personal mesh for multi-device sync (SPEC B §11).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceId(pub [u8; 32]);

impl DeviceId {
    /// Derive DeviceId from a device's public key.
    pub fn from_pubkey(pubkey: &VerifyingKey) -> Self {
        let hash = blake3::hash(pubkey.as_bytes());
        DeviceId(*hash.as_bytes())
    }
}

impl fmt::Debug for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DeviceId({}...)", hex_prefix(&self.0, 8))
    }
}

// ─── KeyPair ───────────────────────────────────────────────────────────────

/// Ed25519 keypair wrapper for OBP node identity.
///
/// SPEC A §1.3: Each node generates an Ed25519 keypair at first launch.
pub struct KeyPair {
    signing_key: SigningKey,
}

impl KeyPair {
    /// Generate a new random Ed25519 keypair.
    pub fn generate() -> Self {
        KeyPair {
            signing_key: SigningKey::generate(&mut OsRng),
        }
    }

    /// Get the public (verifying) key.
    pub fn public_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Sign a message with this keypair.
    pub fn sign(&self, message: &[u8]) -> Signature {
        self.signing_key.sign(message)
    }

    /// Verify a signature against this keypair's public key.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> bool {
        self.public_key().verify(message, signature).is_ok()
    }

    /// Get the raw public key bytes (32 bytes).
    pub fn pubkey_bytes(&self) -> [u8; 32] {
        *self.public_key().as_bytes()
    }
}

// ─── NodeId Generation (SPEC A §1.5 Algorithm 1) ──────────────────────────

/// Result of NodeId generation including the nonce that solved the puzzle.
#[derive(Debug)]
pub struct NodeIdProof {
    /// The generated NodeId satisfying the puzzle.
    pub node_id: NodeId,
    /// The nonce that, combined with pubkey, produces the NodeId.
    pub nonce: u64,
    /// The difficulty level that was satisfied.
    pub difficulty: u8,
}

/// Generate a NodeId by solving the crypto puzzle.
///
/// Iterates nonces until `BLAKE3(pubkey || nonce)` has `difficulty`
/// leading zero bits. Returns the NodeId and the solving nonce.
///
/// SPEC A §1.5 Algorithm 1:
/// ```text
/// LOOP:
///   candidate ← BLAKE3(pubkey || nonce)[0..32]
///   IF leading_zeros(candidate) >= difficulty:
///     RETURN NodeId(candidate)
///   nonce += 1
/// ```
///
/// Expected iterations: 2^difficulty (e.g., 65K for difficulty=16).
pub fn generate_node_id(pubkey: &[u8; 32], difficulty: u8) -> NodeIdProof {
    generate_node_id_bounded(pubkey, difficulty, u64::MAX)
        .expect("Unbounded puzzle should never fail")
}

/// Generate a NodeId with a maximum iteration limit.
///
/// Returns `Err(IdentityError::PuzzleTimeout)` if no solution found
/// within `max_iterations`.
pub fn generate_node_id_bounded(
    pubkey: &[u8; 32],
    difficulty: u8,
    max_iterations: u64,
) -> Result<NodeIdProof, IdentityError> {
    if difficulty > 32 {
        return Err(IdentityError::InvalidDifficulty(difficulty));
    }
    let mut nonce: u64 = 0;
    while nonce < max_iterations {
        let mut hasher = blake3::Hasher::new();
        hasher.update(pubkey);
        hasher.update(&nonce.to_le_bytes());
        let hash = hasher.finalize();
        let candidate = NodeId(*hash.as_bytes());

        if candidate.satisfies_difficulty(difficulty) {
            return Ok(NodeIdProof {
                node_id: candidate,
                nonce,
                difficulty,
            });
        }
        nonce += 1;
    }
    Err(IdentityError::PuzzleTimeout { max_iterations })
}

/// Verify that a NodeId was correctly generated from a pubkey + nonce.
///
/// SPEC A §2.4: Challenge-response verification.
pub fn verify_node_id(pubkey: &[u8; 32], nonce: u64, node_id: &NodeId, difficulty: u8) -> bool {
    let mut hasher = blake3::Hasher::new();
    hasher.update(pubkey);
    hasher.update(&nonce.to_le_bytes());
    let hash = hasher.finalize();
    let candidate = NodeId(*hash.as_bytes());
    candidate == *node_id && candidate.satisfies_difficulty(difficulty)
}

/// Verify a signature using a raw public key.
pub fn verify_signature(pubkey_bytes: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> bool {
    let Ok(verifying_key) = VerifyingKey::from_bytes(pubkey_bytes) else {
        return false;
    };
    let signature = Signature::from_bytes(signature);
    verifying_key.verify(message, &signature).is_ok()
}

// ─── DID Format (SPEC A §1.2) ─────────────────────────────────────────────

/// Format a public key as a DID (Decentralized Identifier).
///
/// Format: `did:key:z6Mk<base58btc(pubkey)>`
/// For now returns a simplified hex representation.
pub fn pubkey_to_did(pubkey: &[u8; 32]) -> String {
    let mut did = String::from("did:key:z6Mk");
    for byte in pubkey {
        did.push_str(&format!("{:02x}", byte));
    }
    did
}

// ─── Helpers ───────────────────────────────────────────────────────────────

fn hex_prefix(bytes: &[u8], n: usize) -> String {
    bytes.iter().take(n).map(|b| format!("{:02x}", b)).collect()
}
