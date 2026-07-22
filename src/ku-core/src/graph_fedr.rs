//! # FedR — Federated RotatE Training Protocol
//!
//! Enables decentralized knowledge graph embedding training across
//! OneBrain network nodes. Each node:
//! 1. Trains locally on its triples (local SGD)
//! 2. Computes a compact delta (~1 KB per sync)
//! 3. Gossips delta to peers via the network layer
//! 4. Applies received deltas with weighted averaging
//!
//! ## Privacy
//! - Only relation deltas are shared (33 × 64 bytes = 2,112 bytes max)
//! - Entity embeddings stay local (never shared)
//! - No raw triples are transmitted
//!
//! ## Convergence
//! - FedAvg-style weighted averaging ensures convergence
//! - Peer weight based on triple count (more data = more influence)
//! - Staleness detection via epoch counter

use crate::graph_embeddings::{train_step, EntityEmbedding, RelationTable};
use crate::types::RelationType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─────────────────────────────────────────────────────────────────────
// 1. RelationDelta
// ─────────────────────────────────────────────────────────────────────

/// Compact delta for gossip broadcast.
///
/// Contains per-relation changes (Δreal, Δimag) computed as
/// `new_embedding - old_embedding` for each dimension.
/// Total size: 33 relations × 64 bytes ≈ 2,112 bytes max.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationDelta {
    /// Per-relation deltas: (Δreal[32], Δimag[32])
    pub deltas: HashMap<RelationType, ([i8; 32], [i8; 32])>,
    /// Peer identity (for attribution)
    pub peer_id: [u8; 32],
    /// Training epoch (for staleness detection)
    pub epoch: u64,
    /// Number of local triples used in training
    pub triple_count: u32,
}

impl RelationDelta {
    /// Total size in bytes (approximate).
    ///
    /// Each entry: 1 byte key + 32 bytes Δreal + 32 bytes Δimag = 65 bytes.
    /// Header: 32 bytes peer_id + 8 bytes epoch + 4 bytes triple_count = 44 bytes.
    pub fn size_bytes(&self) -> usize {
        self.deltas.len() * 65 + 32 + 8 + 4 // 64 bytes data + 1 byte key per entry + header
    }

    /// Check if this delta is "stale" (too many epochs behind).
    pub fn is_stale(&self, current_epoch: u64, max_staleness: u64) -> bool {
        current_epoch.saturating_sub(self.epoch) > max_staleness
    }
}

// ─────────────────────────────────────────────────────────────────────
// 2. FedRConfig
// ─────────────────────────────────────────────────────────────────────

/// Configuration for federated training.
#[derive(Debug, Clone)]
pub struct FedRConfig {
    /// Learning rate for local SGD (default: 0.01)
    pub learning_rate: f64,
    /// Number of SGD steps per local training round (default: 10)
    pub steps_per_round: usize,
    /// Maximum staleness in epochs before rejecting a delta (default: 5)
    pub max_staleness: u64,
    /// Minimum peer weight for delta application (default: 0.1)
    pub min_peer_weight: f64,
    /// Maximum peer weight for delta application (default: 0.9)
    pub max_peer_weight: f64,
}

impl Default for FedRConfig {
    fn default() -> Self {
        Self {
            learning_rate: 0.01,
            steps_per_round: 10,
            max_staleness: 5,
            min_peer_weight: 0.1,
            max_peer_weight: 0.9,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// 3. FedRProtocol
// ─────────────────────────────────────────────────────────────────────

/// Federated RotatE training protocol.
///
/// Orchestrates local training, delta computation, and peer delta application.
pub struct FedRProtocol {
    /// Protocol configuration.
    pub config: FedRConfig,
    /// Current training epoch.
    pub current_epoch: u64,
}

impl Default for FedRProtocol {
    fn default() -> Self {
        Self {
            config: FedRConfig::default(),
            current_epoch: 0,
        }
    }
}

impl FedRProtocol {
    /// Create a new protocol instance with the given config.
    pub fn new(config: FedRConfig) -> Self {
        Self {
            config,
            current_epoch: 0,
        }
    }

    /// Local training: Run SGD on local triples, return updated count.
    ///
    /// Trains for `steps_per_round` steps, updating entity and relation embeddings.
    /// Entity embeddings are modified in-place via `train_step`; relation embeddings
    /// are additionally updated with RotatE relation gradients (since `train_step`
    /// only touches entity parameters).
    ///
    /// Returns total number of individual triple updates performed.
    pub fn local_train(
        &self,
        triples: &mut [(EntityEmbedding, RelationType, EntityEmbedding)],
        relation_table: &mut RelationTable,
    ) -> usize {
        let lr = self.config.learning_rate;
        let mut updates = 0;
        for _step in 0..self.config.steps_per_round {
            for (head, rel_type, tail) in triples.iter_mut() {
                if let Some(rel_emb) = relation_table.embeddings.get_mut(rel_type) {
                    // Snapshot head before train_step modifies it, for relation gradient
                    let h_snap = head.values;

                    // Update entity embeddings (head & tail)
                    train_step(head, rel_emb, tail, lr);

                    // Also update relation embedding with gradients.
                    // Gradient of RotatE distance w.r.t. relation r:
                    //   ∂/∂r_re = 2(hr_re - t_re) * h_re / 127
                    //   ∂/∂r_im = 2(hr_im - t_im) * h_im / 127
                    // We use the pre-update head snapshot for consistency.
                    for i in 0..32 {
                        let h_re = h_snap[i * 2] as f64;
                        let h_im = h_snap[i * 2 + 1] as f64;
                        let r_re = rel_emb.real[i] as f64;
                        let r_im = rel_emb.imag[i] as f64;
                        let t_re = tail.values[i * 2] as f64;
                        let t_im = tail.values[i * 2 + 1] as f64;

                        let hr_re = (h_re * r_re - h_im * r_im) / 127.0;
                        let hr_im = (h_re * r_im + h_im * r_re) / 127.0;

                        let grad_re = 2.0 * (hr_re - t_re);
                        let grad_im = 2.0 * (hr_im - t_im);

                        // Update relation: r -= lr * ∂L/∂r
                        let new_r_re = (r_re - lr * grad_re * h_re / 127.0).clamp(-128.0, 127.0);
                        let new_r_im = (r_im - lr * grad_im * h_im / 127.0).clamp(-128.0, 127.0);
                        rel_emb.real[i] = new_r_re as i8;
                        rel_emb.imag[i] = new_r_im as i8;
                    }

                    updates += 1;
                }
            }
        }
        updates
    }

    /// Compute delta between old and new relation tables.
    ///
    /// Returns a compact [`RelationDelta`] containing per-dimension differences.
    /// Only relations with actual changes are included.
    pub fn compute_delta(
        &self,
        old_table: &RelationTable,
        new_table: &RelationTable,
        peer_id: [u8; 32],
        triple_count: u32,
    ) -> RelationDelta {
        let mut deltas = HashMap::new();

        for (&rel, new_emb) in &new_table.embeddings {
            if let Some(old_emb) = old_table.embeddings.get(&rel) {
                let mut d_real = [0i8; 32];
                let mut d_imag = [0i8; 32];
                let mut has_change = false;

                for i in 0..32 {
                    d_real[i] = new_emb.real[i].wrapping_sub(old_emb.real[i]);
                    d_imag[i] = new_emb.imag[i].wrapping_sub(old_emb.imag[i]);
                    if d_real[i] != 0 || d_imag[i] != 0 {
                        has_change = true;
                    }
                }

                if has_change {
                    deltas.insert(rel, (d_real, d_imag));
                }
            }
        }

        RelationDelta {
            deltas,
            peer_id,
            epoch: self.current_epoch,
            triple_count,
        }
    }

    /// Apply a received delta from a peer with weighted averaging.
    ///
    /// Weight is based on peer's triple count relative to ours,
    /// clamped to `[min_peer_weight, max_peer_weight]`.
    ///
    /// Returns the number of relations updated, or an error if the delta
    /// is too stale.
    pub fn apply_delta(
        &self,
        table: &mut RelationTable,
        delta: &RelationDelta,
        local_triple_count: u32,
    ) -> Result<usize, FedRError> {
        // Staleness check
        if delta.is_stale(self.current_epoch, self.config.max_staleness) {
            return Err(FedRError::StaleDelta {
                delta_epoch: delta.epoch,
                current_epoch: self.current_epoch,
            });
        }

        // Compute peer weight based on relative data size
        let raw_weight = if local_triple_count == 0 {
            self.config.max_peer_weight
        } else {
            (delta.triple_count as f64) / (local_triple_count as f64 + delta.triple_count as f64)
        };
        let weight = raw_weight.clamp(self.config.min_peer_weight, self.config.max_peer_weight);

        let mut applied = 0;
        for (&rel, &(d_real, d_imag)) in &delta.deltas {
            if let Some(emb) = table.embeddings.get_mut(&rel) {
                for i in 0..32 {
                    let scaled_real = (d_real[i] as f64 * weight).round() as i8;
                    let scaled_imag = (d_imag[i] as f64 * weight).round() as i8;
                    emb.real[i] = emb.real[i].saturating_add(scaled_real);
                    emb.imag[i] = emb.imag[i].saturating_add(scaled_imag);
                }
                applied += 1;
            }
        }

        Ok(applied)
    }

    /// Advance the training epoch.
    pub fn advance_epoch(&mut self) {
        self.current_epoch += 1;
    }
}

// ─────────────────────────────────────────────────────────────────────
// 4. FedRError
// ─────────────────────────────────────────────────────────────────────

/// Errors from FedR operations.
#[derive(Debug, Clone, PartialEq)]
pub enum FedRError {
    /// Delta is too old to apply.
    StaleDelta {
        delta_epoch: u64,
        current_epoch: u64,
    },
}

impl std::fmt::Display for FedRError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FedRError::StaleDelta {
                delta_epoch,
                current_epoch,
            } => {
                write!(
                    f,
                    "stale delta: delta epoch {delta_epoch}, current epoch {current_epoch}"
                )
            }
        }
    }
}

impl std::error::Error for FedRError {}

// ─────────────────────────────────────────────────────────────────────
// 5. Multi-Peer Aggregation
// ─────────────────────────────────────────────────────────────────────

/// Aggregate multiple deltas (FedAvg-style).
///
/// Each delta is weighted by its `triple_count`. Returns `None` if the
/// input slice is empty or total triple count is zero.
pub fn aggregate_deltas(deltas: &[RelationDelta]) -> Option<RelationDelta> {
    if deltas.is_empty() {
        return None;
    }

    let total_triples: u64 = deltas.iter().map(|d| d.triple_count as u64).sum();
    if total_triples == 0 {
        return None;
    }

    let mut agg: HashMap<RelationType, ([f64; 32], [f64; 32])> = HashMap::new();

    for delta in deltas {
        let weight = delta.triple_count as f64 / total_triples as f64;
        for (&rel, &(d_real, d_imag)) in &delta.deltas {
            let entry = agg.entry(rel).or_insert(([0.0; 32], [0.0; 32]));
            for i in 0..32 {
                entry.0[i] += d_real[i] as f64 * weight;
                entry.1[i] += d_imag[i] as f64 * weight;
            }
        }
    }

    // Convert back to i8
    let mut result_deltas = HashMap::new();
    for (rel, (real_f, imag_f)) in agg {
        let mut real = [0i8; 32];
        let mut imag = [0i8; 32];
        for i in 0..32 {
            real[i] = real_f[i].round().clamp(-128.0, 127.0) as i8;
            imag[i] = imag_f[i].round().clamp(-128.0, 127.0) as i8;
        }
        result_deltas.insert(rel, (real, imag));
    }

    Some(RelationDelta {
        deltas: result_deltas,
        peer_id: [0u8; 32], // aggregated, no single peer
        epoch: deltas.iter().map(|d| d.epoch).max().unwrap_or(0),
        triple_count: total_triples as u32,
    })
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a small set of test triples.
    fn test_triples() -> Vec<(EntityEmbedding, RelationType, EntityEmbedding)> {
        vec![
            (
                EntityEmbedding::from_seed(&[1u8; 32]),
                RelationType::Extends,
                EntityEmbedding::from_seed(&[2u8; 32]),
            ),
            (
                EntityEmbedding::from_seed(&[3u8; 32]),
                RelationType::Causes,
                EntityEmbedding::from_seed(&[4u8; 32]),
            ),
            (
                EntityEmbedding::from_seed(&[5u8; 32]),
                RelationType::PartOf,
                EntityEmbedding::from_seed(&[6u8; 32]),
            ),
        ]
    }

    // ── 1. default_config ────────────────────────────────────────────

    #[test]
    fn default_config() {
        let cfg = FedRConfig::default();
        assert!((cfg.learning_rate - 0.01).abs() < f64::EPSILON);
        assert_eq!(cfg.steps_per_round, 10);
        assert_eq!(cfg.max_staleness, 5);
        assert!((cfg.min_peer_weight - 0.1).abs() < f64::EPSILON);
        assert!((cfg.max_peer_weight - 0.9).abs() < f64::EPSILON);
    }

    // ── 2. local_train_updates_embeddings ────────────────────────────

    #[test]
    fn local_train_updates_embeddings() {
        let proto = FedRProtocol::default();
        let mut table = RelationTable::new();
        let mut triples = test_triples();

        // Snapshot before
        let head_before = triples[0].0.clone();
        let tail_before = triples[0].2.clone();

        proto.local_train(&mut triples, &mut table);

        // At least one embedding should have changed
        assert!(
            triples[0].0 != head_before || triples[0].2 != tail_before,
            "entity embeddings should change after training"
        );
    }

    // ── 3. local_train_step_count ────────────────────────────────────

    #[test]
    fn local_train_step_count() {
        let cfg = FedRConfig {
            steps_per_round: 3,
            ..FedRConfig::default()
        };
        let proto = FedRProtocol::new(cfg);
        let mut table = RelationTable::new();
        let mut triples = test_triples();

        let updates = proto.local_train(&mut triples, &mut table);
        // 3 triples × 3 steps = 9
        assert_eq!(updates, 9, "should have 3 triples × 3 steps = 9 updates");
    }

    // ── 4. compute_delta_detects_changes ─────────────────────────────

    #[test]
    fn compute_delta_detects_changes() {
        let proto = FedRProtocol::default();
        let old_table = RelationTable::new();
        let mut new_table = old_table.clone();
        let mut triples = test_triples();

        proto.local_train(&mut triples, &mut new_table);

        let delta = proto.compute_delta(&old_table, &new_table, [1u8; 32], 3);
        assert!(
            !delta.deltas.is_empty(),
            "delta should detect changes after training"
        );
    }

    // ── 5. compute_delta_no_change ───────────────────────────────────

    #[test]
    fn compute_delta_no_change() {
        let proto = FedRProtocol::default();
        let table = RelationTable::new();

        let delta = proto.compute_delta(&table, &table, [0u8; 32], 0);
        assert!(
            delta.deltas.is_empty(),
            "identical tables should produce empty delta"
        );
    }

    // ── 6. compute_delta_size ────────────────────────────────────────

    #[test]
    fn compute_delta_size() {
        let proto = FedRProtocol::default();
        let old_table = RelationTable::new();
        let mut new_table = old_table.clone();
        let mut triples = test_triples();

        proto.local_train(&mut triples, &mut new_table);

        let delta = proto.compute_delta(&old_table, &new_table, [1u8; 32], 3);
        let size = delta.size_bytes();
        // Should be well under 4 KB
        assert!(
            size < 4096,
            "delta size should be compact, got {size} bytes"
        );
        // Should be > 0 since we have changes
        assert!(size > 44, "delta with changes should be > header size");
    }

    // ── 7. apply_delta_modifies_table ────────────────────────────────

    #[test]
    fn apply_delta_modifies_table() {
        let proto = FedRProtocol::default();
        let old_table = RelationTable::new();
        let mut new_table = old_table.clone();
        let mut triples = test_triples();

        proto.local_train(&mut triples, &mut new_table);

        let delta = proto.compute_delta(&old_table, &new_table, [1u8; 32], 100);

        // Apply to a fresh table
        let mut target = RelationTable::new();
        let target_before = target.clone();
        let applied = proto.apply_delta(&mut target, &delta, 100).unwrap();

        assert!(applied > 0, "should apply at least one relation delta");

        // At least one embedding should differ from the original
        let mut any_changed = false;
        for (&rel, emb) in &target.embeddings {
            if let Some(before_emb) = target_before.embeddings.get(&rel) {
                if emb != before_emb {
                    any_changed = true;
                    break;
                }
            }
        }
        assert!(any_changed, "at least one relation embedding should change");
    }

    // ── 8. apply_delta_weighted ──────────────────────────────────────

    #[test]
    fn apply_delta_weighted() {
        let proto = FedRProtocol::default();
        let old_table = RelationTable::new();
        let mut new_table = old_table.clone();
        let mut triples = test_triples();

        proto.local_train(&mut triples, &mut new_table);
        let delta = proto.compute_delta(&old_table, &new_table, [1u8; 32], 1000);

        // Apply with high local count (peer has less relative weight)
        let mut table_low = RelationTable::new();
        let _ = proto.apply_delta(&mut table_low, &delta, 10000).unwrap();

        // Apply with low local count (peer has more relative weight)
        let mut table_high = RelationTable::new();
        let _ = proto.apply_delta(&mut table_high, &delta, 10).unwrap();

        // The table with low local count should diverge more from the original
        let original = RelationTable::new();
        let mut diff_low: i64 = 0;
        let mut diff_high: i64 = 0;

        for (&rel, orig_emb) in &original.embeddings {
            if let (Some(low_emb), Some(high_emb)) = (
                table_low.embeddings.get(&rel),
                table_high.embeddings.get(&rel),
            ) {
                for i in 0..32 {
                    diff_low += (low_emb.real[i] as i64 - orig_emb.real[i] as i64).abs();
                    diff_high += (high_emb.real[i] as i64 - orig_emb.real[i] as i64).abs();
                }
            }
        }

        assert!(
            diff_high >= diff_low,
            "higher peer weight should cause more change: diff_high={diff_high}, diff_low={diff_low}"
        );
    }

    // ── 9. apply_delta_stale_rejected ────────────────────────────────

    #[test]
    fn apply_delta_stale_rejected() {
        let mut proto = FedRProtocol::default();
        // Advance epoch to 10
        for _ in 0..10 {
            proto.advance_epoch();
        }

        let delta = RelationDelta {
            deltas: HashMap::new(),
            peer_id: [1u8; 32],
            epoch: 0, // epoch 0 is 10 behind, max_staleness = 5
            triple_count: 100,
        };

        let mut table = RelationTable::new();
        let result = proto.apply_delta(&mut table, &delta, 100);
        assert_eq!(
            result,
            Err(FedRError::StaleDelta {
                delta_epoch: 0,
                current_epoch: 10,
            })
        );
    }

    // ── 10. apply_delta_staleness_boundary ────────────────────────────

    #[test]
    fn apply_delta_staleness_boundary() {
        let mut proto = FedRProtocol::default();
        // Advance epoch to 5
        for _ in 0..5 {
            proto.advance_epoch();
        }
        assert_eq!(proto.current_epoch, 5);

        // Delta at epoch 0, staleness = 5 (exactly at boundary = NOT stale)
        let delta = RelationDelta {
            deltas: HashMap::new(),
            peer_id: [1u8; 32],
            epoch: 0,
            triple_count: 100,
        };

        let mut table = RelationTable::new();
        let result = proto.apply_delta(&mut table, &delta, 100);
        assert!(
            result.is_ok(),
            "delta at exact staleness boundary should be accepted"
        );
    }

    // ── 11. advance_epoch ────────────────────────────────────────────

    #[test]
    fn advance_epoch() {
        let mut proto = FedRProtocol::default();
        assert_eq!(proto.current_epoch, 0);
        proto.advance_epoch();
        assert_eq!(proto.current_epoch, 1);
        proto.advance_epoch();
        assert_eq!(proto.current_epoch, 2);
    }

    // ── 12. aggregate_deltas_empty ───────────────────────────────────

    #[test]
    fn aggregate_deltas_empty() {
        let result = aggregate_deltas(&[]);
        assert!(result.is_none(), "empty deltas should return None");
    }

    // ── 13. aggregate_deltas_single ──────────────────────────────────

    #[test]
    fn aggregate_deltas_single() {
        let mut deltas_map = HashMap::new();
        deltas_map.insert(RelationType::Extends, ([10i8; 32], [5i8; 32]));

        let delta = RelationDelta {
            deltas: deltas_map,
            peer_id: [1u8; 32],
            epoch: 3,
            triple_count: 100,
        };

        let result = aggregate_deltas(&[delta]).unwrap();
        // Single delta with weight 1.0 → same values
        let (real, imag) = result.deltas.get(&RelationType::Extends).unwrap();
        assert_eq!(*real, [10i8; 32]);
        assert_eq!(*imag, [5i8; 32]);
        assert_eq!(result.epoch, 3);
    }

    // ── 14. aggregate_deltas_weighted ────────────────────────────────

    #[test]
    fn aggregate_deltas_weighted() {
        let mut d1_map = HashMap::new();
        d1_map.insert(RelationType::Extends, ([100i8; 32], [0i8; 32]));

        let mut d2_map = HashMap::new();
        d2_map.insert(RelationType::Extends, ([0i8; 32], [0i8; 32]));

        let d1 = RelationDelta {
            deltas: d1_map,
            peer_id: [1u8; 32],
            epoch: 1,
            triple_count: 300, // 75% weight
        };
        let d2 = RelationDelta {
            deltas: d2_map,
            peer_id: [2u8; 32],
            epoch: 2,
            triple_count: 100, // 25% weight
        };

        let result = aggregate_deltas(&[d1, d2]).unwrap();
        let (real, _imag) = result.deltas.get(&RelationType::Extends).unwrap();
        // Expected: 100 * 0.75 + 0 * 0.25 = 75
        assert_eq!(
            real[0], 75,
            "weighted average should be 75, got {}",
            real[0]
        );
        assert_eq!(result.epoch, 2, "epoch should be max of inputs");
    }

    // ── 15. full_round_trip ──────────────────────────────────────────

    #[test]
    fn full_round_trip() {
        // Simulate: Node A trains → computes delta → Node B applies
        let proto_a = FedRProtocol::default();
        let proto_b = FedRProtocol::default();

        // Both start with the same relation table
        let original_table = RelationTable::new();
        let mut table_a = original_table.clone();
        let mut table_b = original_table.clone();

        // Node A trains locally
        let mut triples_a = test_triples();
        let updates = proto_a.local_train(&mut triples_a, &mut table_a);
        assert!(updates > 0);

        // Node A computes delta
        let delta = proto_a.compute_delta(
            &original_table,
            &table_a,
            [0xAA; 32],
            triples_a.len() as u32,
        );
        assert!(!delta.deltas.is_empty());

        // Node B applies the delta
        let applied = proto_b.apply_delta(&mut table_b, &delta, 100).unwrap();
        assert!(applied > 0);

        // Node B's table should now be closer to Node A's table
        // (at least for the relations that were trained on)
        for (&rel, &(d_real, d_imag)) in &delta.deltas {
            let emb_b = table_b.embeddings.get(&rel).unwrap();
            let emb_orig = original_table.embeddings.get(&rel).unwrap();
            // B should differ from original (delta was applied)
            let mut any_diff = false;
            for i in 0..32 {
                if emb_b.real[i] != emb_orig.real[i] || emb_b.imag[i] != emb_orig.imag[i] {
                    any_diff = true;
                    break;
                }
            }
            // Only check if delta actually had non-zero values that would survive scaling
            let has_nonzero = d_real.iter().any(|&v| v != 0) || d_imag.iter().any(|&v| v != 0);
            if has_nonzero {
                assert!(
                    any_diff,
                    "Node B's embedding for {rel:?} should differ from original after delta application"
                );
            }
        }
    }

    // ── 16. relation_delta_is_stale ──────────────────────────────────

    #[test]
    fn relation_delta_is_stale() {
        let delta = RelationDelta {
            deltas: HashMap::new(),
            peer_id: [0u8; 32],
            epoch: 3,
            triple_count: 10,
        };

        // Not stale: current=5, max_staleness=5, diff=2
        assert!(!delta.is_stale(5, 5));
        // Not stale: exactly at boundary: current=8, max_staleness=5, diff=5
        assert!(!delta.is_stale(8, 5));
        // Stale: current=9, max_staleness=5, diff=6
        assert!(delta.is_stale(9, 5));
        // Not stale: same epoch
        assert!(!delta.is_stale(3, 0));
        // Stale: one behind with zero staleness
        assert!(delta.is_stale(4, 0));
    }
}
