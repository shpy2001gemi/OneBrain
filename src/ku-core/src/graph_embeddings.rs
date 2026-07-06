//! # Graph Embeddings — RotatE int8 Knowledge Graph Embeddings
//!
//! Pure-Rust implementation of RotatE (Sun et al., 2019) with int8 quantization.
//! Each entity is a 64-dimensional vector (32 complex dimensions),
//! each relation is a rotation in complex space.
//!
//! ## Why RotatE?
//! - Models symmetric, antisymmetric, inverse, and composition patterns
//! - Simple scoring: h ∘ r ≈ t (element-wise complex rotation)
//! - int8 quantization: 64 bytes per entity (vs 256 bytes for float32)
//!
//! ## Memory Budget
//! - Per entity: 64 bytes (values) + 6 bytes (metadata) = 70 bytes
//! - All 34 relations: 34 × 64 = 2,176 bytes
//! - 10,000 entities: ~700 KB total

use serde::{Serialize, Deserialize};
use crate::types::RelationType;
use std::collections::HashMap;

// ─────────────────────────────────────────────────────────────────────
// Custom serde helpers for fixed-size i8 arrays (serde only supports [T; N] for N ≤ 32)
// ─────────────────────────────────────────────────────────────────────

mod serde_i8_64 {
    use serde::{self, Serializer, Deserializer, Serialize, Deserialize};
    pub fn serialize<S>(arr: &[i8; 64], serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        let v: Vec<i8> = arr.to_vec();
        v.serialize(serializer)
    }
    pub fn deserialize<'de, D>(deserializer: D) -> Result<[i8; 64], D::Error>
    where D: Deserializer<'de> {
        let v: Vec<i8> = Vec::deserialize(deserializer)?;
        v.try_into().map_err(|v: Vec<i8>| {
            serde::de::Error::custom(format!("expected 64 elements, got {}", v.len()))
        })
    }
}

mod serde_i8_32 {
    use serde::{self, Serializer, Deserializer, Serialize, Deserialize};
    pub fn serialize<S>(arr: &[i8; 32], serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        let v: Vec<i8> = arr.to_vec();
        v.serialize(serializer)
    }
    pub fn deserialize<'de, D>(deserializer: D) -> Result<[i8; 32], D::Error>
    where D: Deserializer<'de> {
        let v: Vec<i8> = Vec::deserialize(deserializer)?;
        v.try_into().map_err(|v: Vec<i8>| {
            serde::de::Error::custom(format!("expected 32 elements, got {}", v.len()))
        })
    }
}

// ─────────────────────────────────────────────────────────────────────
// 1. EntityEmbedding
// ─────────────────────────────────────────────────────────────────────

/// RotatE entity embedding: 32 complex dimensions, int8 quantized.
///
/// Values represent complex numbers as interleaved (real, imag) pairs:
/// `values[0..2]` = (re₀, im₀), `values[2..4]` = (re₁, im₁), …
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityEmbedding {
    /// 64 int8 values = 32 complex dimensions
    #[serde(with = "serde_i8_64")]
    pub values: [i8; 64],
    /// Version counter (incremented on each update)
    pub version: u16,
    /// Last update timestamp (unix seconds)
    pub updated_at: u32,
}

impl EntityEmbedding {
    /// Create a zero embedding.
    pub fn zero() -> Self {
        Self {
            values: [0i8; 64],
            version: 0,
            updated_at: 0,
        }
    }

    /// Create from a seed (deterministic pseudorandom initialization).
    ///
    /// Uses simple byte-mixing to produce diverse initial values from a
    /// 32-byte seed (e.g. a BLAKE3 hash of the entity ID).
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let mut values = [0i8; 64];
        for i in 0..64 {
            let idx = i % 32;
            let mix = seed[idx]
                .wrapping_mul(71)
                .wrapping_add(seed[(idx + 13) % 32]);
            values[i] = mix as i8;
        }
        Self {
            values,
            version: 0,
            updated_at: 0,
        }
    }

    /// L2 distance (squared) to another embedding.
    ///
    /// Operates entirely in `i64` arithmetic — no floats needed.
    pub fn distance_sq(&self, other: &Self) -> i64 {
        self.values
            .iter()
            .zip(other.values.iter())
            .map(|(&a, &b)| {
                let d = (a as i64) - (b as i64);
                d * d
            })
            .sum()
    }

    /// Cosine similarity (returns value in \[-1.0, 1.0\]).
    pub fn cosine_similarity(&self, other: &Self) -> f64 {
        let dot: i64 = self
            .values
            .iter()
            .zip(other.values.iter())
            .map(|(&a, &b)| (a as i64) * (b as i64))
            .sum();
        let norm_a: f64 = self
            .values
            .iter()
            .map(|&v| (v as f64).powi(2))
            .sum::<f64>()
            .sqrt();
        let norm_b: f64 = other
            .values
            .iter()
            .map(|&v| (v as f64).powi(2))
            .sum::<f64>()
            .sqrt();
        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }
        (dot as f64) / (norm_a * norm_b)
    }
}

impl Default for EntityEmbedding {
    fn default() -> Self {
        Self::zero()
    }
}

// ─────────────────────────────────────────────────────────────────────
// 2. RelationEmbedding
// ─────────────────────────────────────────────────────────────────────

/// RotatE relation embedding: rotation angles in complex space.
///
/// In RotatE, relation **r** is a rotation: r = (cos θ, sin θ) for each
/// complex dimension.  Scoring: `h ∘ r ≈ t`, where ∘ is complex
/// multiplication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationEmbedding {
    /// Real part of rotation (cos θ), 32 dims
    #[serde(with = "serde_i8_32")]
    pub real: [i8; 32],
    /// Imaginary part of rotation (sin θ), 32 dims
    #[serde(with = "serde_i8_32")]
    pub imag: [i8; 32],
}

impl RelationEmbedding {
    /// Identity rotation (no change).
    ///
    /// `cos(0) = 1 ≈ 127` in int8 scale, `sin(0) = 0`.
    pub fn identity() -> Self {
        let mut real = [0i8; 32];
        for r in real.iter_mut() {
            *r = 127; // cos(0) = 1 ≈ 127 in int8
        }
        Self {
            real,
            imag: [0i8; 32], // sin(0) = 0
        }
    }

    /// Create from angle index (deterministic init per [`RelationType`]).
    ///
    /// Each relation type gets a unique rotation derived from its `u8`
    /// discriminant, ensuring diverse embeddings across the 34 variants.
    pub fn from_relation(rel: RelationType) -> Self {
        let seed = rel as u8;
        let mut real = [0i8; 32];
        let mut imag = [0i8; 32];
        for i in 0..32 {
            let angle =
                ((seed as f64 * 7.0 + i as f64 * 13.0) % 360.0).to_radians();
            real[i] = (angle.cos() * 127.0).round().clamp(-128.0, 127.0) as i8;
            imag[i] = (angle.sin() * 127.0).round().clamp(-128.0, 127.0) as i8;
        }
        Self { real, imag }
    }
}

impl Default for RelationEmbedding {
    fn default() -> Self {
        Self::identity()
    }
}

// ─────────────────────────────────────────────────────────────────────
// 3. RelationTable
// ─────────────────────────────────────────────────────────────────────

/// All 34 relation embeddings, initialized from [`RelationType`] variants.
///
/// Total memory: 34 × 64 = 2,176 bytes (+ HashMap overhead).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationTable {
    pub embeddings: HashMap<RelationType, RelationEmbedding>,
}

/// Number of relation types.
const NUM_RELATIONS: usize = 34;

/// All 34 `RelationType` variants, in definition order.
const ALL_RELATIONS: [RelationType; NUM_RELATIONS] = [
    // A: Epistemic
    RelationType::Extends,
    RelationType::Supplements,
    RelationType::Refutes,
    RelationType::Corroborates,
    RelationType::Supersedes,
    RelationType::Qualifies,
    // B: Structural
    RelationType::PartOf,
    RelationType::InstanceOf,
    RelationType::Specializes,
    RelationType::Generalizes,
    // C: Causal
    RelationType::Causes,
    RelationType::Enables,
    RelationType::Prevents,
    RelationType::DependsOn,
    // D: Derivation
    RelationType::ExampleOf,
    RelationType::AnalogyOf,
    RelationType::AppliesTo,
    RelationType::DerivedFrom,
    // E: Similarity
    RelationType::Duplicates,
    RelationType::Translates,
    RelationType::Paraphrases,
    RelationType::Inspires,
    // F: Temporal
    RelationType::Precedes,
    RelationType::Cooccurs,
    // G: Provenance
    RelationType::Cites,
    RelationType::AuthoredBy,
    RelationType::ReviewedBy,
    // H: Experiential
    RelationType::ReactionTo,
    RelationType::TestimonyAbout,
    RelationType::FormallyProves,
    RelationType::EvolvesInto,
    RelationType::VariantOf,
    RelationType::SensoryEvidenceFor,
    RelationType::CulturallyContextualizes,
];

impl RelationTable {
    /// Create with default embeddings for all 34 relation types.
    pub fn new() -> Self {
        let mut embeddings = HashMap::with_capacity(ALL_RELATIONS.len());
        for &rel in &ALL_RELATIONS {
            embeddings.insert(rel, RelationEmbedding::from_relation(rel));
        }
        Self { embeddings }
    }

    /// Get relation embedding.
    pub fn get(&self, rel: RelationType) -> Option<&RelationEmbedding> {
        self.embeddings.get(&rel)
    }

    /// Total memory size in bytes (payload only, excludes HashMap overhead).
    pub fn size_bytes(&self) -> usize {
        self.embeddings.len() * 64 // 32 real + 32 imag per relation
    }
}

impl Default for RelationTable {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────
// 4. Scoring Functions
// ─────────────────────────────────────────────────────────────────────

/// RotatE scoring: `score = -‖h ∘ r − t‖²`
///
/// Complex multiplication per dimension:
/// `(a + bi)(c + di) = (ac − bd) + (ad + bc)i`
///
/// Higher score ⇒ better triple `(head, relation, tail)`.
pub fn rotate_score(
    head: &EntityEmbedding,
    relation: &RelationEmbedding,
    tail: &EntityEmbedding,
) -> i32 {
    let mut score: i64 = 0;
    for i in 0..32 {
        let h_re = head.values[i * 2] as i64;
        let h_im = head.values[i * 2 + 1] as i64;
        let r_re = relation.real[i] as i64;
        let r_im = relation.imag[i] as i64;
        let t_re = tail.values[i * 2] as i64;
        let t_im = tail.values[i * 2 + 1] as i64;

        // Complex multiply: h ∘ r  (scale back from int8×int8)
        let hr_re = (h_re * r_re - h_im * r_im) / 127;
        let hr_im = (h_re * r_im + h_im * r_re) / 127;

        // Distance: ‖h∘r − t‖²
        let diff_re = hr_re - t_re;
        let diff_im = hr_im - t_im;
        score -= diff_re * diff_re + diff_im * diff_im;
    }
    score.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

/// Link prediction: find top-k best targets for `(head, relation, ?)`.
///
/// Returns `Vec<(candidate_index, score)>`, sorted by **descending** score.
pub fn predict_tail(
    head: &EntityEmbedding,
    relation: &RelationEmbedding,
    candidates: &[EntityEmbedding],
    top_k: usize,
) -> Vec<(usize, i32)> {
    let mut scored: Vec<(usize, i32)> = candidates
        .iter()
        .enumerate()
        .map(|(idx, tail)| (idx, rotate_score(head, relation, tail)))
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1)); // descending
    scored.truncate(top_k);
    scored
}

/// Anomaly detection: how well does this bond match the embedding space?
///
/// Returns anomaly score in `[0.0, 1.0]`. Higher = more anomalous.
/// The `_weight` parameter is reserved for future weighted scoring.
pub fn bond_anomaly_score(
    head: &EntityEmbedding,
    relation: &RelationEmbedding,
    tail: &EntityEmbedding,
    _weight: u16,
) -> f64 {
    let score = rotate_score(head, relation, tail);
    // Normalize: score is typically in [-large, 0], map to anomaly [0, 1]
    // Very negative = poor fit = high anomaly
    let normalized = (-score as f64) / (32.0 * 127.0 * 127.0);
    normalized.clamp(0.0, 1.0)
}

// ─────────────────────────────────────────────────────────────────────
// 5. SGD Training Step
// ─────────────────────────────────────────────────────────────────────

/// One SGD training step on a single positive triple.
///
/// Updates entity and relation embeddings to make `h ∘ r` closer to `t`.
/// Gradients are computed analytically from the RotatE distance function.
pub fn train_step(
    head: &mut EntityEmbedding,
    relation: &mut RelationEmbedding,
    tail: &mut EntityEmbedding,
    learning_rate: f64,
) {
    for i in 0..32 {
        let h_re = head.values[i * 2] as f64;
        let h_im = head.values[i * 2 + 1] as f64;
        let r_re = relation.real[i] as f64;
        let r_im = relation.imag[i] as f64;
        let t_re = tail.values[i * 2] as f64;
        let t_im = tail.values[i * 2 + 1] as f64;

        // h ∘ r (complex multiply, scale = 127)
        let hr_re = (h_re * r_re - h_im * r_im) / 127.0;
        let hr_im = (h_re * r_im + h_im * r_re) / 127.0;

        // Gradient: d/d_x ‖h∘r − t‖² = 2(h∘r − t)
        let grad_re = 2.0 * (hr_re - t_re);
        let grad_im = 2.0 * (hr_im - t_im);

        // Update tail (move closer to h∘r)
        let new_t_re = (t_re + learning_rate * grad_re).clamp(-128.0, 127.0);
        let new_t_im = (t_im + learning_rate * grad_im).clamp(-128.0, 127.0);
        tail.values[i * 2] = new_t_re as i8;
        tail.values[i * 2 + 1] = new_t_im as i8;

        // Update head (move h∘r closer to t)
        // Gradient w.r.t. h is more complex due to rotation
        let new_h_re =
            (h_re - learning_rate * grad_re * r_re / 127.0).clamp(-128.0, 127.0);
        let new_h_im =
            (h_im - learning_rate * grad_im * r_im / 127.0).clamp(-128.0, 127.0);
        head.values[i * 2] = new_h_re as i8;
        head.values[i * 2 + 1] = new_h_im as i8;
    }
    head.version += 1;
    tail.version += 1;
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RelationType;

    // ── EntityEmbedding ──────────────────────────────────────────────

    #[test]
    fn entity_embedding_zero() {
        let e = EntityEmbedding::zero();
        assert_eq!(e.values, [0i8; 64]);
        assert_eq!(e.version, 0);
        assert_eq!(e.updated_at, 0);
    }

    #[test]
    fn entity_embedding_from_seed() {
        let seed_a = [1u8; 32];
        let seed_b = [2u8; 32];
        let a = EntityEmbedding::from_seed(&seed_a);
        let b = EntityEmbedding::from_seed(&seed_b);
        // Same seed → same embedding
        assert_eq!(a, EntityEmbedding::from_seed(&seed_a));
        // Different seeds → different embeddings
        assert_ne!(a.values, b.values);
    }

    #[test]
    fn entity_embedding_distance_sq_zero() {
        let e = EntityEmbedding::from_seed(&[42u8; 32]);
        assert_eq!(e.distance_sq(&e), 0);
    }

    #[test]
    fn entity_embedding_distance_sq_nonzero() {
        let a = EntityEmbedding::from_seed(&[1u8; 32]);
        let b = EntityEmbedding::from_seed(&[2u8; 32]);
        assert!(a.distance_sq(&b) > 0);
    }

    #[test]
    fn entity_embedding_cosine_self() {
        let e = EntityEmbedding::from_seed(&[7u8; 32]);
        let sim = e.cosine_similarity(&e);
        assert!((sim - 1.0).abs() < 1e-9, "cosine(x,x) should be 1.0, got {sim}");
    }

    #[test]
    fn entity_embedding_cosine_orthogonal() {
        let e = EntityEmbedding::from_seed(&[7u8; 32]);
        let z = EntityEmbedding::zero();
        let sim = e.cosine_similarity(&z);
        assert_eq!(sim, 0.0, "cosine(x, zero) should be 0.0");
    }

    // ── RelationEmbedding ────────────────────────────────────────────

    #[test]
    fn relation_embedding_identity() {
        let id = RelationEmbedding::identity();
        for &r in &id.real {
            assert_eq!(r, 127);
        }
        for &im in &id.imag {
            assert_eq!(im, 0);
        }
    }

    #[test]
    fn relation_embedding_from_relation() {
        let a = RelationEmbedding::from_relation(RelationType::Extends);
        let b = RelationEmbedding::from_relation(RelationType::Refutes);
        assert_ne!(a.real, b.real, "different relations should have different real parts");
        // Also verify determinism
        let a2 = RelationEmbedding::from_relation(RelationType::Extends);
        assert_eq!(a, a2, "same relation should produce same embedding");
    }

    // ── RelationTable ────────────────────────────────────────────────

    #[test]
    fn relation_table_has_all_34() {
        let table = RelationTable::new();
        assert_eq!(
            table.embeddings.len(),
            NUM_RELATIONS,
            "table should contain all 34 relation types"
        );
    }

    #[test]
    fn relation_table_size_bytes() {
        let table = RelationTable::new();
        assert_eq!(table.size_bytes(), NUM_RELATIONS * 64, "34 × 64 = 2176 bytes");
    }

    // ── Scoring ──────────────────────────────────────────────────────

    #[test]
    fn rotate_score_identity() {
        // With identity rotation, h ∘ id ≈ h, so score(h, id, h) should be
        // close to 0 (best possible score since score = -distance²).
        let h = EntityEmbedding::from_seed(&[10u8; 32]);
        let id = RelationEmbedding::identity();
        let score = rotate_score(&h, &id, &h);
        // Score should be close to 0 (perfect match)
        // Due to int8 rounding, allow some slack
        assert!(
            score > -5000,
            "identity rotation on same entity should yield near-zero score, got {score}"
        );
    }

    #[test]
    fn rotate_score_different_worse() {
        let h = EntityEmbedding::from_seed(&[10u8; 32]);
        let t = EntityEmbedding::from_seed(&[99u8; 32]);
        let id = RelationEmbedding::identity();
        let score_same = rotate_score(&h, &id, &h);
        let score_diff = rotate_score(&h, &id, &t);
        assert!(
            score_same > score_diff,
            "same entity should score better than different: same={score_same} vs diff={score_diff}"
        );
    }

    // ── Link prediction ──────────────────────────────────────────────

    #[test]
    fn predict_tail_returns_k() {
        let h = EntityEmbedding::from_seed(&[1u8; 32]);
        let r = RelationEmbedding::from_relation(RelationType::Causes);
        let candidates: Vec<EntityEmbedding> = (0..10)
            .map(|i| EntityEmbedding::from_seed(&[i as u8; 32]))
            .collect();
        let results = predict_tail(&h, &r, &candidates, 3);
        assert_eq!(results.len(), 3, "should return exactly top_k=3 results");
    }

    #[test]
    fn predict_tail_ordered() {
        let h = EntityEmbedding::from_seed(&[1u8; 32]);
        let r = RelationEmbedding::from_relation(RelationType::Enables);
        let candidates: Vec<EntityEmbedding> = (0..20)
            .map(|i| EntityEmbedding::from_seed(&[i as u8; 32]))
            .collect();
        let results = predict_tail(&h, &r, &candidates, 5);
        for window in results.windows(2) {
            assert!(
                window[0].1 >= window[1].1,
                "results should be sorted descending: {} >= {}",
                window[0].1,
                window[1].1
            );
        }
    }

    // ── Anomaly score ────────────────────────────────────────────────

    #[test]
    fn bond_anomaly_score_range() {
        let h = EntityEmbedding::from_seed(&[3u8; 32]);
        let t = EntityEmbedding::from_seed(&[4u8; 32]);
        let r = RelationEmbedding::from_relation(RelationType::DependsOn);
        let anomaly = bond_anomaly_score(&h, &r, &t, 100);
        assert!(
            (0.0..=1.0).contains(&anomaly),
            "anomaly score should be in [0, 1], got {anomaly}"
        );
    }

    #[test]
    fn bond_anomaly_low_for_good_triple() {
        // A head with identity rotation scored against itself should
        // have very low anomaly (good fit).
        let h = EntityEmbedding::from_seed(&[50u8; 32]);
        let id = RelationEmbedding::identity();
        let anomaly = bond_anomaly_score(&h, &id, &h, 100);
        assert!(
            anomaly < 0.2,
            "well-matching triple should have low anomaly, got {anomaly}"
        );
    }

    // ── Training ─────────────────────────────────────────────────────

    #[test]
    fn train_step_improves_score() {
        let mut head = EntityEmbedding::from_seed(&[10u8; 32]);
        let mut tail = EntityEmbedding::from_seed(&[20u8; 32]);
        let mut rel = RelationEmbedding::from_relation(RelationType::Extends);
        let score_before = rotate_score(&head, &rel, &tail);

        // Run several training steps
        for _ in 0..50 {
            train_step(&mut head, &mut rel, &mut tail, 0.1);
        }
        let score_after = rotate_score(&head, &rel, &tail);
        assert!(
            score_after >= score_before,
            "score should improve (or stay equal) after training: before={score_before}, after={score_after}"
        );
    }

    #[test]
    fn train_step_increments_version() {
        let mut head = EntityEmbedding::from_seed(&[10u8; 32]);
        let mut tail = EntityEmbedding::from_seed(&[20u8; 32]);
        let mut rel = RelationEmbedding::from_relation(RelationType::Causes);
        assert_eq!(head.version, 0);
        assert_eq!(tail.version, 0);

        train_step(&mut head, &mut rel, &mut tail, 0.1);
        assert_eq!(head.version, 1);
        assert_eq!(tail.version, 1);

        train_step(&mut head, &mut rel, &mut tail, 0.1);
        assert_eq!(head.version, 2);
        assert_eq!(tail.version, 2);
    }
}
