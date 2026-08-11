//! # OBT Storage Reward
//!
//! R4: Reward for storing KU objects on the DHT.
//!
//! ## Formula
//! ```text
//! storage_reward(node, epoch) = Σ per stored KU:
//!     STORAGE_BASE_RATE × size_w × rarity_w × demand_w × duration_f × trust_f
//! ```
//!
//! ## Five Factors
//! | Factor | Range      | Purpose                          |
//! |--------|-----------|----------------------------------|
//! | `size_w`    | [0.1, 10.0] | Larger KUs cost more to store     |
//! | `rarity_w`  | [0.5, 3.0]  | Under-replicated KUs earn more    |
//! | `demand_w`  | [0.1, 5.0]  | Frequently accessed KUs earn more |
//! | `duration_f`| [0.0, 2.0]  | Long-term storage loyalty bonus   |
//! | `trust_f`   | [0.0, 1.0]  | Higher EigenTrust = higher reward |
//!
//! ## PoS-KU Challenge Protocol
//! Nodes prove storage via cryptographic challenge-response, inspired by
//! Sia Merkle proofs and Arweave SPoRA.
//!
//! ## Reference
//! See `docs/specs/obt/04_STORAGE_REWARD.md`.

use serde::{Deserialize, Serialize};

use crate::obt_constants::*;

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/// Metadata for a KU stored by a node, used to compute per-KU storage reward.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredKuInfo {
    /// CID of the stored KU (BLAKE3 hash).
    pub ku_cid: [u8; 32],

    /// Wire-encoded size in bytes.
    pub wire_bytes_len: u32,

    /// Current replica count on the DHT.
    pub actual_replicas: u32,

    /// Current metabolic rate of this KU (usage events/epoch).
    pub metabolism_rate: f64,

    /// Number of consecutive epochs this node has stored this KU.
    pub epochs_stored: u64,
}

/// PoS-KU Challenge types.
///
/// Three challenge types with different cost/confidence trade-offs:
/// - `FullHash`: O(n) — strongest guarantee, proves complete possession
/// - `ByteRange`: O(1) — cheapest, random-access proof
/// - `FieldExtract`: O(log n) — proves possession AND decode ability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageChallenge {
    /// Type 1: Node must return BLAKE3 hash of entire KU wire bytes.
    FullHash { ku_cid: [u8; 32] },

    /// Type 2: Node must return a specific byte range from the KU.
    ByteRange {
        ku_cid: [u8; 32],
        /// Byte offset into wire-encoded KU (seed-derived).
        offset: u32,
        /// Number of bytes to return (max 256).
        length: u32,
    },

    /// Type 3: Node must extract a specific field and provide Merkle proof.
    FieldExtract { ku_cid: [u8; 32] },
}

impl StorageChallenge {
    /// Return the KU CID targeted by this challenge.
    pub fn ku_cid(&self) -> &[u8; 32] {
        match self {
            Self::FullHash { ku_cid } => ku_cid,
            Self::ByteRange { ku_cid, .. } => ku_cid,
            Self::FieldExtract { ku_cid } => ku_cid,
        }
    }

    /// Human-readable challenge type name.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::FullHash { .. } => "FullHash",
            Self::ByteRange { .. } => "ByteRange",
            Self::FieldExtract { .. } => "FieldExtract",
        }
    }
}

impl std::fmt::Display for StorageChallenge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StorageChallenge::{}", self.type_name())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Individual Factor Functions
// ═══════════════════════════════════════════════════════════════════════════

/// Size weight: proportional to wire-encoded size in KB.
///
/// Larger KUs cost more disk/bandwidth to store.
fn size_weight(wire_bytes_len: u32) -> f64 {
    let kb = wire_bytes_len as f64 / 1024.0;
    kb.clamp(STORAGE_SIZE_WEIGHT_MIN, STORAGE_SIZE_WEIGHT_MAX)
}

/// Rarity weight: inversely proportional to replica count.
///
/// Under-replicated KUs are more valuable to store.
/// `K_TARGET = 20`: a KU with 10 replicas has rarity_w = 2.0.
fn rarity_weight(actual_replicas: u32) -> f64 {
    if actual_replicas == 0 {
        return STORAGE_RARITY_WEIGHT_MAX; // We ARE the only copy
    }
    let ratio = K_TARGET as f64 / actual_replicas as f64;
    ratio.clamp(STORAGE_RARITY_WEIGHT_MIN, STORAGE_RARITY_WEIGHT_MAX)
}

/// Demand weight: proportional to relative metabolism (usage rate).
///
/// Frequently accessed KUs are more valuable to keep stored.
/// Returns 1.0 if median_metabolism is zero (no network activity → neutral).
fn demand_weight(metabolism_rate: f64, median_metabolism: f64) -> f64 {
    if median_metabolism <= 0.0 {
        return 1.0; // No network activity → neutral
    }
    let ratio = metabolism_rate / median_metabolism;
    ratio.clamp(STORAGE_DEMAND_WEIGHT_MIN, STORAGE_DEMAND_WEIGHT_MAX)
}

/// Duration factor: loyalty bonus for long-term storage.
///
/// Linear ramp from 0.0 to `DURATION_FACTOR_MAX` over
/// `DURATION_MATURITY_EPOCHS` epochs.
fn duration_factor(epochs_stored: u64) -> f64 {
    let ratio = epochs_stored as f64 / DURATION_MATURITY_EPOCHS as f64;
    ratio.min(STORAGE_DURATION_FACTOR_MAX)
}

/// Trust factor: directly uses the node's EigenTrust score.
///
/// Sybil nodes with near-zero trust earn near-zero storage rewards.
fn trust_factor(node_trust: f64) -> f64 {
    node_trust.clamp(0.0, 1.0)
}

// ═══════════════════════════════════════════════════════════════════════════
// Per-KU Storage Reward
// ═══════════════════════════════════════════════════════════════════════════

/// Compute storage reward for a single KU.
///
/// ```text
/// reward = STORAGE_BASE_RATE × size_w × rarity_w × demand_w × duration_f × trust_f
/// ```
///
/// # Arguments
/// * `wire_bytes_len` — Wire-encoded size in bytes
/// * `actual_replicas` — Current replica count on DHT
/// * `metabolism_rate` — KU's current metabolic rate
/// * `median_metabolism` — Network-wide median KU metabolism this epoch
/// * `epochs_stored` — Consecutive epochs this node has stored this KU
/// * `node_trust` — Node's EigenTrust score [0.0, 1.0]
pub fn compute_ku_storage_reward(
    wire_bytes_len: u32,
    actual_replicas: u32,
    metabolism_rate: f64,
    median_metabolism: f64,
    epochs_stored: u64,
    node_trust: f64,
) -> f64 {
    let size_w = size_weight(wire_bytes_len);
    let rarity_w = rarity_weight(actual_replicas);
    let demand_w = demand_weight(metabolism_rate, median_metabolism);
    let duration_f = duration_factor(epochs_stored);
    let trust_f = trust_factor(node_trust);

    STORAGE_BASE_RATE * size_w * rarity_w * demand_w * duration_f * trust_f
}

// ═══════════════════════════════════════════════════════════════════════════
// Per-Node Storage Reward (aggregate)
// ═══════════════════════════════════════════════════════════════════════════

/// Compute total storage reward for a node (sum over all stored KUs).
///
/// This is the R4 reward for a single node in a single epoch.
///
/// # Arguments
/// * `stored_kus` — Metadata for each KU this node stores
/// * `node_trust` — Node's EigenTrust score [0.0, 1.0]
/// * `median_metabolism` — Network-wide median KU metabolism this epoch
pub fn compute_node_storage_reward(
    stored_kus: &[StoredKuInfo],
    node_trust: f64,
    median_metabolism: f64,
) -> f64 {
    stored_kus
        .iter()
        .map(|ku| {
            compute_ku_storage_reward(
                ku.wire_bytes_len,
                ku.actual_replicas,
                ku.metabolism_rate,
                median_metabolism,
                ku.epochs_stored,
                node_trust,
            )
        })
        .sum()
}

// ═══════════════════════════════════════════════════════════════════════════
// PoS-KU Challenge Generation
// ═══════════════════════════════════════════════════════════════════════════

/// Generate deterministic storage challenges for a node in an epoch.
///
/// Uses BLAKE3-based seeding to:
/// 1. Select ~10% of stored KUs for challenge (deterministic, unpredictable)
/// 2. Assign challenge types based on seed hash
///
/// The challenge set is capped at `max_challenges` to bound verification work.
///
/// # Arguments
/// * `epoch` — Current epoch number
/// * `node_id` — 32-byte node ID (Ed25519 pubkey)
/// * `stored_cids` — CIDs of all KUs the node claims to store
/// * `max_challenges` — Maximum number of challenges to generate
pub fn generate_storage_challenges(
    epoch: u64,
    node_id: &[u8; 32],
    stored_cids: &[[u8; 32]],
    max_challenges: usize,
) -> Vec<StorageChallenge> {
    if stored_cids.is_empty() || max_challenges == 0 {
        return Vec::new();
    }

    // Generate epoch+node seed
    let mut hasher = blake3::Hasher::new();
    hasher.update(&epoch.to_le_bytes());
    hasher.update(node_id);
    let seed = *hasher.finalize().as_bytes();

    let mut challenges = Vec::new();

    for cid in stored_cids {
        if challenges.len() >= max_challenges {
            break;
        }

        // Deterministic selection: challenge if hash < CHALLENGE_RATE threshold
        let mut h = blake3::Hasher::new();
        h.update(&seed);
        h.update(cid);
        let hash = h.finalize();
        let val = u64::from_le_bytes(hash.as_bytes()[..8].try_into().unwrap());
        let threshold = (CHALLENGE_RATE * u64::MAX as f64) as u64;

        if val < threshold {
            // Determine challenge type from hash
            let challenge = select_challenge_type(&seed, cid);
            challenges.push(challenge);
        }
    }

    challenges
}

/// Deterministically select challenge type from seed + CID.
///
/// Distribution: ~20% FullHash, ~50% ByteRange, ~30% FieldExtract.
///
/// - ByteRange is most common: cheap, fast, hard to fake
/// - FieldExtract ensures nodes can decode, not just store blobs
/// - FullHash is rare but provides strongest guarantee
fn select_challenge_type(seed: &[u8; 32], ku_cid: &[u8; 32]) -> StorageChallenge {
    let mut h = blake3::Hasher::new();
    h.update(seed);
    h.update(ku_cid);
    h.update(b"challenge_type");
    let hash = h.finalize();
    let selector = hash.as_bytes()[0]; // 0–255

    match selector {
        0..=50 => {
            // FullHash (~20%)
            StorageChallenge::FullHash { ku_cid: *ku_cid }
        }
        51..=178 => {
            // ByteRange (~50%) — derive offset/length from hash
            let offset = u32::from_le_bytes(hash.as_bytes()[1..5].try_into().unwrap());
            let length_raw = u16::from_le_bytes(hash.as_bytes()[5..7].try_into().unwrap());
            let length = ((length_raw as u32) % 256).max(1); // 1..256 bytes

            StorageChallenge::ByteRange {
                ku_cid: *ku_cid,
                offset,
                length,
            }
        }
        179..=255 => {
            // FieldExtract (~30%)
            StorageChallenge::FieldExtract { ku_cid: *ku_cid }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Challenge Verification
// ═══════════════════════════════════════════════════════════════════════════

/// Verify a storage challenge response by comparing against expected value.
///
/// For all challenge types, the verification is a simple byte comparison:
/// - `FullHash`: response = BLAKE3(entire_ku_bytes), expected = known hash
/// - `ByteRange`: response = ku_bytes[offset..offset+length], expected = known bytes
/// - `FieldExtract`: response = extracted field value, expected = known value
///
/// Returns `true` if the response matches the expected value exactly.
pub fn verify_challenge_response(
    challenge: &StorageChallenge,
    response: &[u8],
    expected: &[u8],
) -> bool {
    // Validate response format matches challenge type
    match challenge {
        StorageChallenge::FullHash { .. } => {
            // FullHash response must be exactly 32 bytes (BLAKE3 hash)
            if response.len() != 32 {
                return false;
            }
        }
        StorageChallenge::ByteRange { length, .. } => {
            // ByteRange response must match the requested length
            if response.len() != *length as usize {
                return false;
            }
        }
        StorageChallenge::FieldExtract { .. } => {
            // FieldExtract response must be non-empty
            if response.is_empty() {
                return false;
            }
        }
    }

    if response.len() != expected.len() {
        return false;
    }
    // Constant-time comparison to prevent timing side-channels
    let mut diff = 0u8;
    for (a, b) in response.iter().zip(expected.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── Factor Functions ─────────────────────────────────────────────

    #[test]
    fn test_size_weight_clamp() {
        // Tiny KU (50 bytes = 0.049 KB) → clamped to STORAGE_SIZE_WEIGHT_MIN (0.1)
        assert!((size_weight(50) - STORAGE_SIZE_WEIGHT_MIN).abs() < 1e-10);

        // 1KB → 1.0
        assert!((size_weight(1024) - 1.0).abs() < 1e-10);

        // 5KB → 5.0
        assert!((size_weight(5 * 1024) - 5.0).abs() < 1e-10);

        // 20KB → clamped to STORAGE_SIZE_WEIGHT_MAX (10.0)
        assert!((size_weight(20 * 1024) - STORAGE_SIZE_WEIGHT_MAX).abs() < 1e-10);
    }

    #[test]
    fn test_rarity_weight_edge_cases() {
        // 0 replicas → max rarity
        assert!((rarity_weight(0) - STORAGE_RARITY_WEIGHT_MAX).abs() < 1e-10);

        // K_TARGET replicas → ratio = 1.0 (exactly replicated)
        assert!((rarity_weight(K_TARGET) - 1.0).abs() < 1e-10);

        // 2 replicas → K_TARGET/2 = 10.0 → clamped to STORAGE_RARITY_WEIGHT_MAX (3.0)
        assert!((rarity_weight(2) - STORAGE_RARITY_WEIGHT_MAX).abs() < 1e-10);

        // 50 replicas → 20/50 = 0.4 → clamped to STORAGE_RARITY_WEIGHT_MIN (0.5)
        assert!((rarity_weight(50) - STORAGE_RARITY_WEIGHT_MIN).abs() < 1e-10);
    }

    #[test]
    fn test_demand_weight() {
        // No network activity → neutral (1.0)
        assert!((demand_weight(1.0, 0.0) - 1.0).abs() < 1e-10);

        // At median → 1.0
        assert!((demand_weight(1.0, 1.0) - 1.0).abs() < 1e-10);

        // 3× median → 3.0
        assert!((demand_weight(3.0, 1.0) - 3.0).abs() < 1e-10);

        // 10× median → clamped to STORAGE_DEMAND_WEIGHT_MAX (5.0)
        assert!((demand_weight(10.0, 1.0) - STORAGE_DEMAND_WEIGHT_MAX).abs() < 1e-10);

        // 0.01× median → clamped to STORAGE_DEMAND_WEIGHT_MIN (0.1)
        assert!((demand_weight(0.01, 1.0) - STORAGE_DEMAND_WEIGHT_MIN).abs() < 1e-10);
    }

    #[test]
    fn test_duration_factor() {
        // 0 epochs → 0.0
        assert!((duration_factor(0) - 0.0).abs() < 1e-10);

        // 50 epochs → 0.5
        assert!((duration_factor(50) - 0.5).abs() < 1e-10);

        // 100 epochs → 1.0
        assert!((duration_factor(100) - 1.0).abs() < 1e-10);

        // 200 epochs → capped at 2.0
        assert!((duration_factor(200) - STORAGE_DURATION_FACTOR_MAX).abs() < 1e-10);

        // 500 epochs → still capped at 2.0
        assert!((duration_factor(500) - STORAGE_DURATION_FACTOR_MAX).abs() < 1e-10);
    }

    #[test]
    fn test_trust_factor_clamp() {
        assert!((trust_factor(-0.5) - 0.0).abs() < 1e-10);
        assert!((trust_factor(0.5) - 0.5).abs() < 1e-10);
        assert!((trust_factor(1.5) - 1.0).abs() < 1e-10);
    }

    // ── Per-KU Storage Reward ────────────────────────────────────────

    #[test]
    fn test_compute_ku_storage_reward_basic() {
        // 1KB, K_TARGET replicas, at median metabolism, 50 epochs, trust=0.5
        let reward = compute_ku_storage_reward(
            1024,     // 1KB → size_w = 1.0
            K_TARGET, // K_TARGET replicas → rarity_w = 1.0
            1.0,      // at median → demand_w = 1.0
            1.0,      // median_metabolism
            50,       // 50 epochs → duration_f = 0.5
            0.5,      // trust_f = 0.5
        );
        let expected = STORAGE_BASE_RATE * 1.0 * 1.0 * 1.0 * 0.5 * 0.5;
        assert!((reward - expected).abs() < 1e-12);
    }

    #[test]
    fn test_compute_ku_storage_reward_rare_hot() {
        // 5KB, 2 replicas, 4× median, 100 epochs, trust=0.5
        let reward = compute_ku_storage_reward(
            5 * 1024, // 5KB → size_w = 5.0
            2,        // 2 replicas → rarity_w = 3.0 (clamped from 4.0)
            4.0,      // 4× median → demand_w = 4.0
            1.0,      // median
            100,      // 100 epochs → duration_f = 1.0
            0.5,      // trust_f = 0.5
        );
        let expected = STORAGE_BASE_RATE * 5.0 * 3.0 * 4.0 * 1.0 * 0.5;
        assert!((reward - expected).abs() < 1e-10);
        assert!((reward - 0.03).abs() < 1e-10);
    }

    #[test]
    fn test_compute_ku_storage_reward_sybil() {
        // Sybil node: trust ≈ MIN_TRUST → reward ≈ 0
        let reward = compute_ku_storage_reward(5 * 1024, 4, 2.0, 1.0, 100, 0.001);
        assert!(
            reward < 0.0001,
            "Sybil node should earn nearly nothing: {reward}"
        );
    }

    // ── Per-Node Aggregate Reward ────────────────────────────────────

    #[test]
    fn test_compute_node_storage_reward() {
        let stored_kus = vec![
            StoredKuInfo {
                ku_cid: [1u8; 32],
                wire_bytes_len: 1024,
                actual_replicas: K_TARGET,
                metabolism_rate: 1.0,
                epochs_stored: 50,
            },
            StoredKuInfo {
                ku_cid: [2u8; 32],
                wire_bytes_len: 2048,
                actual_replicas: 4,
                metabolism_rate: 2.0,
                epochs_stored: 100,
            },
        ];

        let total = compute_node_storage_reward(&stored_kus, 0.7, 1.0);
        let ku1 = compute_ku_storage_reward(1024, K_TARGET, 1.0, 1.0, 50, 0.7);
        let ku2 = compute_ku_storage_reward(2048, 4, 2.0, 1.0, 100, 0.7);
        assert!((total - (ku1 + ku2)).abs() < 1e-12);
    }

    #[test]
    fn test_compute_node_storage_reward_empty() {
        let total = compute_node_storage_reward(&[], 0.5, 1.0);
        assert!((total - 0.0).abs() < 1e-12);
    }

    // ── Challenge Generation ─────────────────────────────────────────

    #[test]
    fn test_generate_challenges_empty_storage() {
        let challenges = generate_storage_challenges(42, &[0u8; 32], &[], 10);
        assert!(challenges.is_empty());
    }

    #[test]
    fn test_generate_challenges_deterministic() {
        let node_id = [1u8; 32];
        let cids: Vec<[u8; 32]> = (0..100u8)
            .map(|i| {
                let mut c = [0u8; 32];
                c[0] = i;
                c
            })
            .collect();

        let c1 = generate_storage_challenges(10, &node_id, &cids, 50);
        let c2 = generate_storage_challenges(10, &node_id, &cids, 50);

        // Same inputs → same challenges
        assert_eq!(c1.len(), c2.len());
        for (a, b) in c1.iter().zip(c2.iter()) {
            assert_eq!(a.ku_cid(), b.ku_cid());
            assert_eq!(a.type_name(), b.type_name());
        }
    }

    #[test]
    fn test_generate_challenges_different_epochs() {
        let node_id = [1u8; 32];
        let cids: Vec<[u8; 32]> = (0..100u8)
            .map(|i| {
                let mut c = [0u8; 32];
                c[0] = i;
                c
            })
            .collect();

        let c_epoch_1 = generate_storage_challenges(1, &node_id, &cids, 50);
        let c_epoch_2 = generate_storage_challenges(2, &node_id, &cids, 50);

        // Different epochs should (very likely) produce different challenge sets
        let cids_1: Vec<_> = c_epoch_1.iter().map(|c| *c.ku_cid()).collect();
        let cids_2: Vec<_> = c_epoch_2.iter().map(|c| *c.ku_cid()).collect();
        // Not guaranteed to differ, but overwhelmingly likely with 100 KUs
        assert!(
            cids_1 != cids_2 || c_epoch_1.is_empty(),
            "Different epochs should produce different challenge targets"
        );
    }

    #[test]
    fn test_generate_challenges_max_cap() {
        let node_id = [1u8; 32];
        // Generate 1000 CIDs to ensure many would be selected
        let cids: Vec<[u8; 32]> = (0..1000u16)
            .map(|i| {
                let mut c = [0u8; 32];
                c[0..2].copy_from_slice(&i.to_le_bytes());
                c
            })
            .collect();

        let challenges = generate_storage_challenges(42, &node_id, &cids, 5);
        assert!(challenges.len() <= 5, "Should be capped at max_challenges");
    }

    #[test]
    fn test_challenge_type_distribution() {
        // Generate many challenges and verify rough distribution
        let node_id = [42u8; 32];
        let cids: Vec<[u8; 32]> = (0..10000u16)
            .map(|i| {
                let mut c = [0u8; 32];
                c[0..2].copy_from_slice(&i.to_le_bytes());
                c
            })
            .collect();

        let challenges = generate_storage_challenges(99, &node_id, &cids, 5000);

        let full_hash = challenges
            .iter()
            .filter(|c| matches!(c, StorageChallenge::FullHash { .. }))
            .count();
        let byte_range = challenges
            .iter()
            .filter(|c| matches!(c, StorageChallenge::ByteRange { .. }))
            .count();
        let field_extract = challenges
            .iter()
            .filter(|c| matches!(c, StorageChallenge::FieldExtract { .. }))
            .count();

        let total = challenges.len();
        if total > 50 {
            // With enough samples, distribution should be roughly:
            // FullHash ~20%, ByteRange ~50%, FieldExtract ~30%
            let fh_pct = full_hash as f64 / total as f64;
            let br_pct = byte_range as f64 / total as f64;
            let fe_pct = field_extract as f64 / total as f64;

            assert!(
                fh_pct > 0.05 && fh_pct < 0.40,
                "FullHash should be ~20%, got {:.1}%",
                fh_pct * 100.0
            );
            assert!(
                br_pct > 0.30 && br_pct < 0.70,
                "ByteRange should be ~50%, got {:.1}%",
                br_pct * 100.0
            );
            assert!(
                fe_pct > 0.10 && fe_pct < 0.50,
                "FieldExtract should be ~30%, got {:.1}%",
                fe_pct * 100.0
            );
        }
    }

    // ── Challenge Verification ───────────────────────────────────────

    #[test]
    fn test_verify_challenge_response_match() {
        // FullHash requires exactly 32 bytes (BLAKE3 hash)
        let challenge = StorageChallenge::FullHash { ku_cid: [0u8; 32] };
        let hash = [0xABu8; 32];
        assert!(verify_challenge_response(&challenge, &hash, &hash));
    }

    #[test]
    fn test_verify_challenge_response_mismatch() {
        let challenge = StorageChallenge::FullHash { ku_cid: [0u8; 32] };
        let response = [0xABu8; 32];
        let mut expected = [0xABu8; 32];
        expected[31] = 0xAC; // Off by one byte
        assert!(!verify_challenge_response(&challenge, &response, &expected));
    }

    #[test]
    fn test_verify_challenge_response_length_mismatch() {
        let challenge = StorageChallenge::FullHash { ku_cid: [0u8; 32] };
        // FullHash response must be 32 bytes — these are not
        assert!(!verify_challenge_response(&challenge, b"short", b"longer"));
    }

    #[test]
    fn test_verify_challenge_response_empty() {
        // FullHash with empty response → rejected (must be 32 bytes)
        let challenge = StorageChallenge::FullHash { ku_cid: [0u8; 32] };
        assert!(!verify_challenge_response(&challenge, b"", b""));
    }

    #[test]
    fn test_verify_challenge_response_byte_range() {
        // ByteRange with matching length
        let challenge = StorageChallenge::ByteRange {
            ku_cid: [0u8; 32],
            offset: 0,
            length: 5,
        };
        assert!(verify_challenge_response(&challenge, b"hello", b"hello"));
        assert!(!verify_challenge_response(&challenge, b"hello", b"world"));
    }
}
