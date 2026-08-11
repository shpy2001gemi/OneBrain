//! # Graph Dream Mode — Offline Graph Restructuring
//!
//! Inspired by neural sleep consolidation. Runs periodically to:
//! 1. **Replay**: Reinforce frequently-accessed bond patterns
//! 2. **Associate**: Discover cross-domain connections via embedding similarity
//! 3. **Prune**: Remove unused speculative bonds
//!
//! ## Design Philosophy
//! - "Giấc ngủ đông" — dormant knowledge isn't deleted, it sleeps
//! - Speculative bonds are created as "dreams" (Experiential, low weight)
//! - Dreams that get validated through use become permanent
//! - Dreams that nobody uses get pruned after 7 days
//!
//! ## Usage
//! ```rust,ignore
//! let engine = DreamEngine::default();
//! let report = engine.run_dream_cycle(&access_log, &embeddings, &bonds, now);
//! ```

use crate::graph_embeddings::{rotate_score, EntityEmbedding, RelationTable};
use crate::graph_types::{BondEvent, BondMeta, WeakeningReason};
use crate::types::{Creator, DecayRate, EdgeState, RelationType};
use std::collections::HashMap;

pub type DreamBondKey = ([u8; 32], [u8; 32], RelationType);
pub type DreamBondMap = HashMap<DreamBondKey, BondMeta>;
pub type DreamBondCandidate = ([u8; 32], [u8; 32], RelationType, BondMeta);

// ============================================================================
// 1. DreamConfig — Dream Mode configuration
// ============================================================================

/// Dream Mode configuration.
///
/// Controls thresholds, weights, and limits for each phase of a dream cycle.
#[derive(Debug, Clone)]
pub struct DreamConfig {
    /// Minimum access count in the period to trigger reinforcement
    pub replay_min_accesses: u32,
    /// Weight boost per replay reinforcement (added to bond weight)
    pub replay_weight_boost: u16,
    /// Maximum weight after boost (cap)
    pub replay_max_weight: u16,
    /// RotatE score threshold for association (lower = stricter).
    /// Score is negative; higher (less negative) = better match.
    pub association_score_threshold: i32,
    /// Initial weight for speculative (dream) bonds
    pub dream_bond_initial_weight: u16,
    /// Days after which unused dream bonds are pruned
    pub prune_after_days: u64,
    /// Maximum speculative bonds to create per cycle
    pub max_associations_per_cycle: usize,
}

impl Default for DreamConfig {
    fn default() -> Self {
        Self {
            replay_min_accesses: 3,
            replay_weight_boost: 500,
            replay_max_weight: 10000,
            association_score_threshold: -5000,
            dream_bond_initial_weight: 1000,
            prune_after_days: 7,
            max_associations_per_cycle: 50,
        }
    }
}

// ============================================================================
// 2. DreamEngine
// ============================================================================

/// Dream Mode engine for offline graph restructuring.
///
/// Orchestrates three phases of "sleep consolidation":
/// 1. **Replay** — reinforces frequently-used bonds
/// 2. **Associate** — discovers cross-domain connections via embeddings
/// 3. **Prune** — removes stale speculative bonds
#[derive(Default)]
pub struct DreamEngine {
    pub config: DreamConfig,
}

impl DreamEngine {
    /// Create a new DreamEngine with the given configuration.
    pub fn new(config: DreamConfig) -> Self {
        Self { config }
    }
}

// ============================================================================
// 3. AccessRecord & DreamReport
// ============================================================================

/// Record of KU access for replay analysis.
///
/// Tracks how many times a particular bond was accessed during the
/// analysis window, along with the most recent access timestamp.
#[derive(Debug, Clone)]
pub struct AccessRecord {
    /// Source entity CID (BLAKE3 hash)
    pub source_cid: [u8; 32],
    /// Target entity CID (BLAKE3 hash)
    pub target_cid: [u8; 32],
    /// Relation type of the bond
    pub relation: RelationType,
    /// Number of times accessed during this window
    pub access_count: u32,
    /// Timestamp of the most recent access (unix seconds)
    pub last_access: u64,
}

/// Summary of a dream cycle.
///
/// Contains aggregate statistics and the full list of bond events
/// generated during all three phases.
#[derive(Debug, Clone)]
pub struct DreamReport {
    /// Number of bonds reinforced in replay phase
    pub bonds_reinforced: usize,
    /// Total weight added across all reinforcements
    pub total_weight_added: u64,
    /// Number of new speculative bonds created
    pub associations_created: usize,
    /// Number of expired dream bonds pruned
    pub bonds_pruned: usize,
    /// Bond events generated during this cycle
    pub events: Vec<BondEvent>,
}

// ============================================================================
// 4. Replay Phase
// ============================================================================

impl DreamEngine {
    /// Replay Phase: Reinforce bonds that were frequently accessed.
    ///
    /// For each bond in the access log with `access_count >= replay_min_accesses`,
    /// boost its weight by `replay_weight_boost` (capped at `replay_max_weight`).
    ///
    /// Returns `(reinforced_count, total_weight_added, events)`.
    pub fn replay_phase(
        &self,
        access_log: &[AccessRecord],
        current_bonds: &mut DreamBondMap,
    ) -> (usize, u64, Vec<BondEvent>) {
        let mut reinforced = 0;
        let mut total_added: u64 = 0;
        let mut events = Vec::new();

        for record in access_log {
            if record.access_count < self.config.replay_min_accesses {
                continue;
            }
            let key = (record.source_cid, record.target_cid, record.relation);
            if let Some(meta) = current_bonds.get_mut(&key) {
                let old_weight = meta.weight;
                let new_weight = old_weight
                    .saturating_add(self.config.replay_weight_boost)
                    .min(self.config.replay_max_weight);
                if new_weight > old_weight {
                    let added = (new_weight - old_weight) as u64;
                    meta.weight = new_weight;
                    total_added += added;
                    reinforced += 1;
                    events.push(BondEvent::Reinforced {
                        source_cid: record.source_cid,
                        target_cid: record.target_cid,
                        relation: record.relation,
                        old_weight,
                        new_weight,
                        timestamp: record.last_access,
                    });
                }
            }
        }
        (reinforced, total_added, events)
    }
}

// ============================================================================
// 5. Association Phase
// ============================================================================

impl DreamEngine {
    /// Association Phase: Discover cross-domain connections via embeddings.
    ///
    /// For each entity pair without a direct bond, computes RotatE scores
    /// across all relation types in the relation table. If the best score
    /// exceeds `association_score_threshold`, creates a speculative (dream) bond.
    ///
    /// Returns `(new_bonds, events)`.
    pub fn association_phase(
        &self,
        entities: &[([u8; 32], EntityEmbedding)],
        relation_table: &RelationTable,
        existing_bonds: &DreamBondMap,
        now_secs: u64,
    ) -> (Vec<DreamBondCandidate>, Vec<BondEvent>) {
        let mut new_bonds = Vec::new();
        let mut events = Vec::new();

        // Compare all entity pairs (up to max_associations_per_cycle)
        'outer: for i in 0..entities.len() {
            for j in (i + 1)..entities.len() {
                if new_bonds.len() >= self.config.max_associations_per_cycle {
                    break 'outer;
                }
                let (cid_a, emb_a) = &entities[i];
                let (cid_b, emb_b) = &entities[j];

                // Try each relation type, find the best-scoring one
                let mut best_score = i32::MIN;
                let mut best_relation = RelationType::Extends;

                for (&rel, rel_emb) in &relation_table.embeddings {
                    let score = rotate_score(emb_a, rel_emb, emb_b);
                    if score > best_score {
                        best_score = score;
                        best_relation = rel;
                    }
                }

                if best_score > self.config.association_score_threshold {
                    let key = (*cid_a, *cid_b, best_relation);
                    let reverse_key = (*cid_b, *cid_a, best_relation);
                    if !existing_bonds.contains_key(&key)
                        && !existing_bonds.contains_key(&reverse_key)
                    {
                        let meta = BondMeta {
                            weight: self.config.dream_bond_initial_weight,
                            creator: Creator::System,
                            state: EdgeState::Active,
                            decay: DecayRate::Fast,
                            timestamp: (now_secs & 0xFFFF_FFFF) as u32, // Explicit u32 truncation — Y2106 limit (consistent with BondMeta design)
                        };
                        new_bonds.push((*cid_a, *cid_b, best_relation, meta));
                        events.push(BondEvent::Created {
                            source_cid: *cid_a,
                            target_cid: *cid_b,
                            relation: best_relation,
                            weight: self.config.dream_bond_initial_weight,
                            creator: Creator::System,
                            evidence: vec![],
                            timestamp: now_secs,
                        });
                    }
                }
            }
        }
        (new_bonds, events)
    }
}

// ============================================================================
// 6. Pruning Phase
// ============================================================================

impl DreamEngine {
    /// Pruning Phase: Remove dream bonds that weren't accessed.
    ///
    /// Dream bonds have initial low weight. If they haven't been reinforced
    /// (weight unchanged at or below `dream_bond_initial_weight`) after
    /// `prune_after_days` and haven't been accessed, they are pruned.
    ///
    /// Pruned bonds emit a `BondEvent::Weakened` event (weight → 0, reason = Decay).
    ///
    /// Returns `(pruned_count, events)`.
    pub fn pruning_phase(
        &self,
        bonds: &mut DreamBondMap,
        access_log: &[AccessRecord],
        now_secs: u64,
    ) -> (usize, Vec<BondEvent>) {
        let cutoff = now_secs.saturating_sub(self.config.prune_after_days * 86400);
        let mut pruned = 0;
        let mut events = Vec::new();

        // Build set of accessed bond keys
        let accessed: std::collections::HashSet<([u8; 32], [u8; 32], RelationType)> = access_log
            .iter()
            .map(|r| (r.source_cid, r.target_cid, r.relation))
            .collect();

        // Find dream bonds to prune:
        // - weight at or below initial dream weight (never reinforced)
        // - created before the cutoff
        // - not in the access log
        let keys_to_prune: Vec<_> = bonds
            .iter()
            .filter(|(key, meta)| {
                meta.weight <= self.config.dream_bond_initial_weight
                    && (meta.timestamp as u64) < cutoff
                    && !accessed.contains(key)
            })
            .map(|(key, meta)| (*key, meta.weight))
            .collect();

        for (key, old_weight) in keys_to_prune {
            bonds.remove(&key);
            pruned += 1;
            events.push(BondEvent::Weakened {
                source_cid: key.0,
                target_cid: key.1,
                relation: key.2,
                old_weight,
                new_weight: 0,
                reason: WeakeningReason::Decay,
                timestamp: now_secs,
            });
        }
        (pruned, events)
    }
}

// ============================================================================
// 7. Full Dream Cycle
// ============================================================================

impl DreamEngine {
    /// Run a complete dream cycle: Replay → Associate → Prune.
    ///
    /// This is the main entry point for dream mode. It executes all three
    /// phases in sequence and returns a consolidated report.
    pub fn run_dream_cycle(
        &self,
        access_log: &[AccessRecord],
        entities: &[([u8; 32], EntityEmbedding)],
        relation_table: &RelationTable,
        bonds: &mut DreamBondMap,
        now_secs: u64,
    ) -> DreamReport {
        // Phase 1: Replay — reinforce frequently-accessed bonds
        let (reinforced, weight_added, mut all_events) = self.replay_phase(access_log, bonds);

        // Phase 2: Associate — discover new cross-domain connections
        let (new_bonds, assoc_events) =
            self.association_phase(entities, relation_table, bonds, now_secs);
        let associations_created = new_bonds.len();
        for (src, tgt, rel, meta) in new_bonds {
            bonds.insert((src, tgt, rel), meta);
        }
        all_events.extend(assoc_events);

        // Phase 3: Prune — remove stale dream bonds
        let (pruned, prune_events) = self.pruning_phase(bonds, access_log, now_secs);
        all_events.extend(prune_events);

        DreamReport {
            bonds_reinforced: reinforced,
            total_weight_added: weight_added,
            associations_created,
            bonds_pruned: pruned,
            events: all_events,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph_embeddings::EntityEmbedding;
    use crate::types::{Creator, DecayRate, EdgeState, RelationType};

    // ── Test Helpers ─────────────────────────────────────────────────────

    fn make_cid(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn make_bond_meta(weight: u16, created_at: u32) -> BondMeta {
        BondMeta {
            weight,
            creator: Creator::System,
            state: EdgeState::Active,
            decay: DecayRate::Fast,
            timestamp: created_at,
        }
    }

    fn make_access(src: u8, tgt: u8, rel: RelationType, count: u32, ts: u64) -> AccessRecord {
        AccessRecord {
            source_cid: make_cid(src),
            target_cid: make_cid(tgt),
            relation: rel,
            access_count: count,
            last_access: ts,
        }
    }

    fn make_bonds_map(entries: Vec<(u8, u8, RelationType, u16, u32)>) -> DreamBondMap {
        entries
            .into_iter()
            .map(|(src, tgt, rel, w, ts)| {
                ((make_cid(src), make_cid(tgt), rel), make_bond_meta(w, ts))
            })
            .collect()
    }

    // ── 1. Config defaults ──────────────────────────────────────────────

    #[test]
    fn default_config() {
        let cfg = DreamConfig::default();
        assert_eq!(cfg.replay_min_accesses, 3);
        assert_eq!(cfg.replay_weight_boost, 500);
        assert_eq!(cfg.replay_max_weight, 10000);
        assert_eq!(cfg.association_score_threshold, -5000);
        assert_eq!(cfg.dream_bond_initial_weight, 1000);
        assert_eq!(cfg.prune_after_days, 7);
        assert_eq!(cfg.max_associations_per_cycle, 50);
    }

    // ── 2. Replay reinforces frequent bonds ─────────────────────────────

    #[test]
    fn replay_reinforces_frequent() {
        let engine = DreamEngine::default();
        let mut bonds = make_bonds_map(vec![(1, 2, RelationType::Extends, 5000, 1000)]);
        let log = vec![make_access(1, 2, RelationType::Extends, 5, 2000)];

        let (reinforced, added, _) = engine.replay_phase(&log, &mut bonds);
        assert_eq!(reinforced, 1);
        assert_eq!(added, 500);

        let key = (make_cid(1), make_cid(2), RelationType::Extends);
        assert_eq!(bonds[&key].weight, 5500);
    }

    // ── 3. Replay skips infrequent bonds ────────────────────────────────

    #[test]
    fn replay_skips_infrequent() {
        let engine = DreamEngine::default();
        let mut bonds = make_bonds_map(vec![(1, 2, RelationType::Extends, 5000, 1000)]);
        let log = vec![
            make_access(1, 2, RelationType::Extends, 2, 2000), // below threshold of 3
        ];

        let (reinforced, added, events) = engine.replay_phase(&log, &mut bonds);
        assert_eq!(reinforced, 0);
        assert_eq!(added, 0);
        assert!(events.is_empty());

        let key = (make_cid(1), make_cid(2), RelationType::Extends);
        assert_eq!(bonds[&key].weight, 5000); // unchanged
    }

    // ── 4. Replay caps at max weight ────────────────────────────────────

    #[test]
    fn replay_caps_at_max() {
        let engine = DreamEngine::default();
        let mut bonds = make_bonds_map(vec![(1, 2, RelationType::Extends, 9800, 1000)]);
        let log = vec![make_access(1, 2, RelationType::Extends, 10, 2000)];

        let (reinforced, added, _) = engine.replay_phase(&log, &mut bonds);
        assert_eq!(reinforced, 1);
        // 9800 + 500 = 10300, capped to 10000 → added = 200
        assert_eq!(added, 200);

        let key = (make_cid(1), make_cid(2), RelationType::Extends);
        assert_eq!(bonds[&key].weight, 10000);
    }

    // ── 5. Replay generates Reinforced events ───────────────────────────

    #[test]
    fn replay_generates_events() {
        let engine = DreamEngine::default();
        let mut bonds = make_bonds_map(vec![
            (1, 2, RelationType::Causes, 3000, 1000),
            (3, 4, RelationType::PartOf, 7000, 1000),
        ]);
        let log = vec![
            make_access(1, 2, RelationType::Causes, 5, 2000),
            make_access(3, 4, RelationType::PartOf, 4, 2500),
        ];

        let (_, _, events) = engine.replay_phase(&log, &mut bonds);
        assert_eq!(events.len(), 2);
        for event in &events {
            match event {
                BondEvent::Reinforced {
                    old_weight,
                    new_weight,
                    ..
                } => {
                    assert!(new_weight > old_weight);
                }
                _ => panic!("expected Reinforced event, got {:?}", event),
            }
        }
    }

    // ── 6. Replay with empty log ────────────────────────────────────────

    #[test]
    fn replay_empty_log() {
        let engine = DreamEngine::default();
        let mut bonds = make_bonds_map(vec![(1, 2, RelationType::Extends, 5000, 1000)]);
        let log: Vec<AccessRecord> = vec![];

        let (reinforced, added, events) = engine.replay_phase(&log, &mut bonds);
        assert_eq!(reinforced, 0);
        assert_eq!(added, 0);
        assert!(events.is_empty());
    }

    // ── 7. Association finds similar entities ───────────────────────────

    #[test]
    fn association_finds_similar() {
        // Use a very permissive threshold so that even somewhat-distant
        // embeddings can form associations in tests.
        let config = DreamConfig {
            association_score_threshold: i32::MIN,
            ..DreamConfig::default()
        };
        let engine = DreamEngine::new(config);

        // Two entities with identical embeddings should score well
        let seed = [42u8; 32];
        let emb = EntityEmbedding::from_seed(&seed);
        let entities = vec![(make_cid(1), emb.clone()), (make_cid(2), emb.clone())];
        let relation_table = RelationTable::new();
        let existing = DreamBondMap::new();
        let now = 100_000u64;

        let (new_bonds, events) =
            engine.association_phase(&entities, &relation_table, &existing, now);
        assert!(
            !new_bonds.is_empty(),
            "should create at least one association"
        );
        assert_eq!(new_bonds.len(), events.len());
    }

    // ── 8. Association skips existing bonds ──────────────────────────────

    #[test]
    fn association_skips_existing() {
        let config = DreamConfig {
            association_score_threshold: i32::MIN,
            ..DreamConfig::default()
        };
        let engine = DreamEngine::new(config);

        let seed = [42u8; 32];
        let emb = EntityEmbedding::from_seed(&seed);
        let entities = vec![(make_cid(1), emb.clone()), (make_cid(2), emb.clone())];
        let relation_table = RelationTable::new();

        // First run: discover the best relation
        let existing = DreamBondMap::new();
        let (first_bonds, _) =
            engine.association_phase(&entities, &relation_table, &existing, 100_000);
        assert!(!first_bonds.is_empty());
        let best_rel = first_bonds[0].2;

        // Pre-populate existing bonds with that relation
        let mut existing2 = HashMap::new();
        existing2.insert(
            (make_cid(1), make_cid(2), best_rel),
            make_bond_meta(5000, 1000),
        );

        let (new_bonds, _) =
            engine.association_phase(&entities, &relation_table, &existing2, 100_000);
        // Should not create the same bond again
        for (src, tgt, rel, _) in &new_bonds {
            assert!(
                !(*src == make_cid(1) && *tgt == make_cid(2) && *rel == best_rel),
                "should not duplicate existing bond"
            );
        }
    }

    // ── 9. Association respects max limit ────────────────────────────────

    #[test]
    fn association_respects_max() {
        let config = DreamConfig {
            association_score_threshold: i32::MIN,
            max_associations_per_cycle: 2,
            ..DreamConfig::default()
        };
        let engine = DreamEngine::new(config);

        // Create enough entities to potentially produce many pairs
        let entities: Vec<_> = (0..10u8)
            .map(|i| {
                let mut seed = [0u8; 32];
                seed[0] = i;
                (make_cid(i), EntityEmbedding::from_seed(&seed))
            })
            .collect();
        let relation_table = RelationTable::new();
        let existing = DreamBondMap::new();

        let (new_bonds, _) =
            engine.association_phase(&entities, &relation_table, &existing, 100_000);
        assert!(
            new_bonds.len() <= 2,
            "should respect max_associations_per_cycle=2, got {}",
            new_bonds.len()
        );
    }

    // ── 10. Association generates Created events ────────────────────────

    #[test]
    fn association_generates_events() {
        let config = DreamConfig {
            association_score_threshold: i32::MIN,
            ..DreamConfig::default()
        };
        let engine = DreamEngine::new(config);

        let seed = [42u8; 32];
        let emb = EntityEmbedding::from_seed(&seed);
        let entities = vec![(make_cid(1), emb.clone()), (make_cid(2), emb.clone())];
        let relation_table = RelationTable::new();
        let existing = DreamBondMap::new();

        let (_, events) = engine.association_phase(&entities, &relation_table, &existing, 100_000);
        assert!(!events.is_empty());
        for event in &events {
            match event {
                BondEvent::Created {
                    weight, creator, ..
                } => {
                    assert_eq!(*weight, 1000);
                    assert_eq!(*creator, Creator::System);
                }
                _ => panic!("expected Created event, got {:?}", event),
            }
        }
    }

    // ── 11. Pruning removes old unused dream bonds ──────────────────────

    #[test]
    fn pruning_removes_old_unused() {
        let engine = DreamEngine::default();
        // Bond created at timestamp 100, now is 100 + 8 days → past 7-day cutoff
        let now = 100 + 8 * 86400;
        let mut bonds = make_bonds_map(vec![
            (1, 2, RelationType::Extends, 1000, 100), // dream bond, low weight
        ]);
        let log: Vec<AccessRecord> = vec![]; // not accessed

        let (pruned, events) = engine.pruning_phase(&mut bonds, &log, now);
        assert_eq!(pruned, 1);
        assert!(bonds.is_empty());
        assert_eq!(events.len(), 1);
    }

    // ── 12. Pruning keeps accessed bonds ────────────────────────────────

    #[test]
    fn pruning_keeps_accessed() {
        let engine = DreamEngine::default();
        let now = 100 + 8 * 86400;
        let mut bonds = make_bonds_map(vec![(1, 2, RelationType::Extends, 1000, 100)]);
        // Bond IS in the access log → should be kept
        let log = vec![make_access(1, 2, RelationType::Extends, 1, now)];

        let (pruned, events) = engine.pruning_phase(&mut bonds, &log, now);
        assert_eq!(pruned, 0);
        assert_eq!(bonds.len(), 1);
        assert!(events.is_empty());
    }

    // ── 13. Pruning keeps reinforced bonds ──────────────────────────────

    #[test]
    fn pruning_keeps_reinforced() {
        let engine = DreamEngine::default();
        let now = 100 + 8 * 86400;
        // Weight is above dream_bond_initial_weight (1000) → has been reinforced
        let mut bonds = make_bonds_map(vec![(1, 2, RelationType::Extends, 2000, 100)]);
        let log: Vec<AccessRecord> = vec![];

        let (pruned, _) = engine.pruning_phase(&mut bonds, &log, now);
        assert_eq!(pruned, 0);
        assert_eq!(bonds.len(), 1);
    }

    // ── 14. Pruning generates Weakened events ───────────────────────────

    #[test]
    fn pruning_generates_events() {
        let engine = DreamEngine::default();
        let now = 100 + 8 * 86400;
        let mut bonds = make_bonds_map(vec![
            (1, 2, RelationType::Extends, 1000, 100),
            (3, 4, RelationType::Causes, 500, 50),
        ]);
        let log: Vec<AccessRecord> = vec![];

        let (pruned, events) = engine.pruning_phase(&mut bonds, &log, now);
        assert_eq!(pruned, 2);
        assert_eq!(events.len(), 2);
        for event in &events {
            match event {
                BondEvent::Weakened {
                    new_weight, reason, ..
                } => {
                    assert_eq!(*new_weight, 0);
                    assert_eq!(*reason, WeakeningReason::Decay);
                }
                _ => panic!("expected Weakened event, got {:?}", event),
            }
        }
    }

    // ── 15. Pruning keeps recent dream bonds ────────────────────────────

    #[test]
    fn pruning_keeps_recent() {
        let engine = DreamEngine::default();
        let now = 100 + 3 * 86400; // only 3 days, less than prune_after_days=7
        let mut bonds = make_bonds_map(vec![(1, 2, RelationType::Extends, 1000, 100)]);
        let log: Vec<AccessRecord> = vec![];

        let (pruned, _) = engine.pruning_phase(&mut bonds, &log, now);
        assert_eq!(pruned, 0);
        assert_eq!(bonds.len(), 1);
    }

    // ── 16. Full dream cycle ────────────────────────────────────────────

    #[test]
    fn full_dream_cycle() {
        let config = DreamConfig {
            association_score_threshold: i32::MIN,
            ..DreamConfig::default()
        };
        let engine = DreamEngine::new(config);

        let now = 1_000_000u64;
        let mut bonds = make_bonds_map(vec![
            // Bond that will be reinforced (high access count)
            (1, 2, RelationType::Extends, 5000, (now - 1000) as u32),
            // Dream bond that will be pruned (old, low weight, not accessed)
            (5, 6, RelationType::Causes, 1000, 100),
        ]);

        let access_log = vec![make_access(1, 2, RelationType::Extends, 10, now)];

        // Entities for association phase
        let seed = [42u8; 32];
        let emb = EntityEmbedding::from_seed(&seed);
        let entities = vec![(make_cid(10), emb.clone()), (make_cid(11), emb.clone())];
        let relation_table = RelationTable::new();

        let report =
            engine.run_dream_cycle(&access_log, &entities, &relation_table, &mut bonds, now);

        // Phase 1: replay reinforced 1 bond
        assert_eq!(report.bonds_reinforced, 1);
        assert_eq!(report.total_weight_added, 500);

        // Phase 2: association created at least 1 bond
        assert!(report.associations_created >= 1);

        // Phase 3: pruning removed the old dream bond
        assert_eq!(report.bonds_pruned, 1);

        // Events from all phases
        assert!(!report.events.is_empty());
    }

    // ── 17. Full dream cycle with empty inputs ──────────────────────────

    #[test]
    fn full_dream_cycle_empty() {
        let engine = DreamEngine::default();
        let mut bonds = DreamBondMap::new();
        let access_log: Vec<AccessRecord> = vec![];
        let entities: Vec<([u8; 32], EntityEmbedding)> = vec![];
        let relation_table = RelationTable::new();

        let report =
            engine.run_dream_cycle(&access_log, &entities, &relation_table, &mut bonds, 100_000);

        assert_eq!(report.bonds_reinforced, 0);
        assert_eq!(report.total_weight_added, 0);
        assert_eq!(report.associations_created, 0);
        assert_eq!(report.bonds_pruned, 0);
        assert!(report.events.is_empty());
    }

    // ── 18. Replay with bond not in map (no-op) ─────────────────────────

    #[test]
    fn replay_missing_bond_is_noop() {
        let engine = DreamEngine::default();
        let mut bonds = DreamBondMap::new();
        // Access log references a bond that doesn't exist in the map
        let log = vec![make_access(99, 100, RelationType::PartOf, 10, 2000)];

        let (reinforced, added, events) = engine.replay_phase(&log, &mut bonds);
        assert_eq!(reinforced, 0);
        assert_eq!(added, 0);
        assert!(events.is_empty());
    }

    // ── 19. Custom config ───────────────────────────────────────────────

    #[test]
    fn custom_config_works() {
        let config = DreamConfig {
            replay_min_accesses: 1,
            replay_weight_boost: 100,
            replay_max_weight: 5000,
            association_score_threshold: -1000,
            dream_bond_initial_weight: 500,
            prune_after_days: 3,
            max_associations_per_cycle: 10,
        };
        let engine = DreamEngine::new(config);

        // With min_accesses=1, even 1 access should trigger reinforcement
        let mut bonds = make_bonds_map(vec![(1, 2, RelationType::Extends, 2000, 1000)]);
        let log = vec![make_access(1, 2, RelationType::Extends, 1, 2000)];

        let (reinforced, added, _) = engine.replay_phase(&log, &mut bonds);
        assert_eq!(reinforced, 1);
        assert_eq!(added, 100);
    }

    // ── 20. Replay already at max weight ────────────────────────────────

    #[test]
    fn replay_already_at_max() {
        let engine = DreamEngine::default();
        let mut bonds = make_bonds_map(vec![(1, 2, RelationType::Extends, 10000, 1000)]);
        let log = vec![make_access(1, 2, RelationType::Extends, 5, 2000)];

        let (reinforced, added, events) = engine.replay_phase(&log, &mut bonds);
        // Weight is already at max, no increase possible
        assert_eq!(reinforced, 0);
        assert_eq!(added, 0);
        assert!(events.is_empty());
    }
}
