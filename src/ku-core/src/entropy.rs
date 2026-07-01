//! # Entropy Calculator — Novelty & Diversity Scoring
//!
//! PoK v2 Signal #3: Measures how NOVEL a KU is compared to existing knowledge.
//! Uses cosine distance on int8 embeddings (512B) and LSH bucket analysis.
//!
//! ## Design:
//! - High entropy = novel, fills a gap → cold-start boost
//! - Low entropy = duplicate/similar → no bonus
//! - Entropy decays exponentially over 7 days (founder decision Q4)
//! - After decay, metabolism is the only long-term signal
//!
//! ## Reuse:
//! - EpigeneticSection.embedding (512B int8) — already exists
//! - EpigeneticSection.simhash (16B) — already exists
//! - EpigeneticSection.lsh_buckets (16B) — already exists

use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

/// Entropy cold-start boost decay period (7 days in seconds)
pub const ENTROPY_DECAY_PERIOD_SECS: u64 = 7 * 24 * 3600;

/// Ln(2) for exponential decay
const LN2: f64 = 0.693147180559945;

/// Weight for novelty component in combined entropy
pub const WEIGHT_NOVELTY: f32 = 0.6;

/// Weight for bridge component in combined entropy
pub const WEIGHT_BRIDGE: f32 = 0.4;

/// Minimum embedding length for valid calculation
pub const MIN_EMBEDDING_LEN: usize = 8;

// ═══════════════════════════════════════════════════════════════════════════
// Entropy Calculator
// ═══════════════════════════════════════════════════════════════════════════

/// Stateless calculator for KU novelty/entropy.
///
/// All methods are pure functions on embeddings — no internal state.
/// This makes it trivially safe for concurrent use across nodes.
pub struct EntropyCalculator;

impl EntropyCalculator {
    /// Compute novelty score of a new KU vs its K nearest neighbors.
    ///
    /// Returns [0.0, 1.0] where:
    /// - 1.0 = completely novel (max distance from all neighbors)
    /// - 0.0 = exact duplicate
    ///
    /// Uses average cosine distance across neighbors.
    pub fn novelty_score(
        new_embedding: &[u8],
        neighbors: &[&[u8]],
    ) -> f32 {
        if new_embedding.len() < MIN_EMBEDDING_LEN || neighbors.is_empty() {
            return 1.0; // No neighbors = maximum novelty
        }

        let avg_distance: f32 = neighbors.iter()
            .map(|n| Self::cosine_distance(new_embedding, n))
            .sum::<f32>() / neighbors.len() as f32;

        // Clamp to [0, 1]
        avg_distance.clamp(0.0, 1.0)
    }

    /// Cosine distance between two int8 embeddings.
    ///
    /// Interprets bytes as signed int8 [-128, 127].
    /// Returns [0.0, 2.0] where 0.0 = identical, 2.0 = opposite.
    /// Normalized to [0.0, 1.0] for practical use.
    pub fn cosine_distance(a: &[u8], b: &[u8]) -> f32 {
        let len = a.len().min(b.len());
        if len == 0 {
            return 1.0;
        }

        let mut dot: i64 = 0;
        let mut norm_a: i64 = 0;
        let mut norm_b: i64 = 0;

        for i in 0..len {
            let va = a[i] as i8 as i64;
            let vb = b[i] as i8 as i64;
            dot += va * vb;
            norm_a += va * va;
            norm_b += vb * vb;
        }

        if norm_a == 0 || norm_b == 0 {
            return 1.0; // Zero vector = max distance
        }

        let cosine_sim = dot as f64 / ((norm_a as f64).sqrt() * (norm_b as f64).sqrt());
        // Convert similarity [-1, 1] to distance [0, 1]
        ((1.0 - cosine_sim) / 2.0) as f32
    }

    /// Compute bridge score: does a KU connect disconnected knowledge clusters?
    ///
    /// A KU is a "bridge" if its LSH buckets are rare — meaning it sits
    /// between existing clusters rather than inside one.
    ///
    /// Returns [0.0, 1.0]:
    /// - 1.0 = unique LSH bucket (bridges two clusters)
    /// - 0.0 = common bucket (inside existing cluster)
    pub fn bridge_score(
        new_lsh: &[u8],
        existing_lsh_counts: &HashMap<Vec<u8>, usize>,
    ) -> f32 {
        if new_lsh.is_empty() || existing_lsh_counts.is_empty() {
            return 1.0; // First KU = maximum bridge potential
        }

        let key = new_lsh.to_vec();
        match existing_lsh_counts.get(&key) {
            None => 1.0, // New bucket = maximum bridge
            Some(&count) => {
                // Inverse frequency: 1 / (1 + count)
                1.0 / (1.0 + count as f32)
            }
        }
    }

    /// SimHash distance (Hamming distance on 128-bit simhash).
    ///
    /// Returns [0, 128] — number of differing bits.
    pub fn simhash_distance(a: &[u8], b: &[u8]) -> u32 {
        if a.len() != 16 || b.len() != 16 {
            return 128; // Max distance if invalid
        }

        let mut distance: u32 = 0;
        for i in 0..16 {
            distance += (a[i] ^ b[i]).count_ones();
        }
        distance
    }

    /// Is this a near-duplicate based on SimHash?
    ///
    /// Threshold: < 10 bits different out of 128 = ~92% similar.
    pub fn is_near_duplicate(simhash_a: &[u8], simhash_b: &[u8]) -> bool {
        Self::simhash_distance(simhash_a, simhash_b) < 10
    }

    /// Combined entropy value with 7-day exponential decay.
    ///
    /// entropy(t) = (w_novelty × novelty + w_bridge × bridge) × decay(age)
    /// decay(age) = e^(-ln2 × age / DECAY_PERIOD)
    ///
    /// After 7 days, entropy ≈ 0.5 (half)
    /// After 14 days, entropy ≈ 0.25
    /// After 21 days, entropy ≈ 0.125 (negligible)
    pub fn entropy_value(
        novelty: f32,
        bridge: f32,
        age_secs: u64,
    ) -> f32 {
        let raw = WEIGHT_NOVELTY * novelty + WEIGHT_BRIDGE * bridge;

        // Exponential decay over 7 days
        let decay = (-LN2 * age_secs as f64 / ENTROPY_DECAY_PERIOD_SECS as f64).exp();

        (raw as f64 * decay) as f32
    }

    /// Convert entropy value to u16 [0, 10000] for TrustSection storage.
    pub fn entropy_to_u16(entropy: f32) -> u16 {
        (entropy * 10000.0).clamp(0.0, 10000.0) as u16
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn make_embedding(val: i8, len: usize) -> Vec<u8> {
        vec![val as u8; len]
    }

    fn make_lsh(id: u8) -> Vec<u8> {
        let mut lsh = vec![0u8; 16];
        lsh[0] = id;
        lsh
    }

    #[test]
    fn test_cosine_distance_identical() {
        let a = make_embedding(50, 512);
        let dist = EntropyCalculator::cosine_distance(&a, &a);
        assert!(dist < 0.01, "Identical vectors: distance = {}", dist);
    }

    #[test]
    fn test_cosine_distance_opposite() {
        let a = make_embedding(50, 512);
        let b = make_embedding(-50, 512);
        let dist = EntropyCalculator::cosine_distance(&a, &b);
        assert!(dist > 0.9, "Opposite vectors: distance = {}", dist);
    }

    #[test]
    fn test_cosine_distance_orthogonal() {
        // Create roughly orthogonal vectors
        let mut a = vec![0u8; 512];
        let mut b = vec![0u8; 512];
        for i in 0..256 {
            a[i] = 100u8; // positive in first half
            b[256 + i] = 100u8; // positive in second half
        }
        let dist = EntropyCalculator::cosine_distance(&a, &b);
        assert!((dist - 0.5).abs() < 0.1, "Orthogonal: distance ≈ 0.5, got {}", dist);
    }

    #[test]
    fn test_novelty_no_neighbors() {
        let emb = make_embedding(42, 512);
        let score = EntropyCalculator::novelty_score(&emb, &[]);
        assert_eq!(score, 1.0, "No neighbors = max novelty");
    }

    #[test]
    fn test_novelty_identical_neighbor() {
        let emb = make_embedding(42, 512);
        let neighbors = vec![emb.as_slice()];
        let score = EntropyCalculator::novelty_score(&emb, &neighbors);
        assert!(score < 0.01, "Identical neighbor = low novelty: {}", score);
    }

    #[test]
    fn test_novelty_distant_neighbors() {
        let emb = make_embedding(50, 512);
        let neighbor = make_embedding(-50, 512);
        let neighbors = vec![neighbor.as_slice()];
        let score = EntropyCalculator::novelty_score(&emb, &neighbors);
        assert!(score > 0.8, "Distant neighbor = high novelty: {}", score);
    }

    #[test]
    fn test_bridge_score_new_bucket() {
        let lsh = make_lsh(99);
        let existing = HashMap::new();
        let score = EntropyCalculator::bridge_score(&lsh, &existing);
        assert_eq!(score, 1.0, "Empty ecosystem = max bridge");
    }

    #[test]
    fn test_bridge_score_common_bucket() {
        let lsh = make_lsh(1);
        let mut existing = HashMap::new();
        existing.insert(make_lsh(1), 100);
        let score = EntropyCalculator::bridge_score(&lsh, &existing);
        assert!(score < 0.02, "100 KUs in same bucket = low bridge: {}", score);
    }

    #[test]
    fn test_bridge_score_rare_bucket() {
        let lsh = make_lsh(5);
        let mut existing = HashMap::new();
        existing.insert(make_lsh(5), 1);
        let score = EntropyCalculator::bridge_score(&lsh, &existing);
        assert!((score - 0.5).abs() < 0.01, "1 existing = bridge 0.5: {}", score);
    }

    #[test]
    fn test_simhash_identical() {
        let a = vec![0xABu8; 16];
        let dist = EntropyCalculator::simhash_distance(&a, &a);
        assert_eq!(dist, 0, "Identical simhash = 0 distance");
    }

    #[test]
    fn test_simhash_max_distance() {
        let a = vec![0x00u8; 16];
        let b = vec![0xFFu8; 16];
        let dist = EntropyCalculator::simhash_distance(&a, &b);
        assert_eq!(dist, 128, "All bits differ = 128 distance");
    }

    #[test]
    fn test_near_duplicate_detection() {
        let a = vec![0xABu8; 16];
        let mut b = a.clone();
        b[0] ^= 0x01; // Flip 1 bit
        assert!(EntropyCalculator::is_near_duplicate(&a, &b), "1 bit flip = near duplicate");

        let c = vec![0x00u8; 16]; // Very different
        assert!(!EntropyCalculator::is_near_duplicate(&a, &c), "All different ≠ duplicate");
    }

    #[test]
    fn test_entropy_decay_7_days() {
        let fresh = EntropyCalculator::entropy_value(1.0, 1.0, 0);
        let after_7d = EntropyCalculator::entropy_value(1.0, 1.0, ENTROPY_DECAY_PERIOD_SECS);

        let ratio = after_7d / fresh;
        assert!(
            (ratio - 0.5).abs() < 0.05,
            "After 7 days, entropy should halve: ratio = {}",
            ratio
        );
    }

    #[test]
    fn test_entropy_nearly_zero_after_21_days() {
        let val = EntropyCalculator::entropy_value(1.0, 1.0, 3 * ENTROPY_DECAY_PERIOD_SECS);
        assert!(val < 0.15, "After 21 days, entropy ≈ 0: {}", val);
    }

    #[test]
    fn test_entropy_to_u16() {
        assert_eq!(EntropyCalculator::entropy_to_u16(0.0), 0);
        assert_eq!(EntropyCalculator::entropy_to_u16(1.0), 10000);
        assert_eq!(EntropyCalculator::entropy_to_u16(0.5), 5000);
        assert_eq!(EntropyCalculator::entropy_to_u16(1.5), 10000); // clamped
    }
}
