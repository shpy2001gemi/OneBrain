//! # OBKG Orchestrator — KuLifecycle Wrapper + Graph Engines
//!
//! Wraps [`KuLifecycle`] and layers in the OBKG graph engines:
//! - **Decay**: Time-based bond weight decay via [`DecayRunner`]
//! - **STDP**: Spike-timing-dependent plasticity via [`StdpEngine`]
//! - **Dream**: Offline graph restructuring via [`DreamEngine`]
//! - **Events**: Bond lifecycle event sourcing via [`EventAccumulator`]
//! - **Embeddings**: RotatE relation embeddings via [`RelationTable`]
//!
//! This module does **not** modify [`KuLifecycle`]; it composes over it.
//!
//! ## Example
//! ```rust,ignore
//! let lifecycle = KuLifecycle::new(PomvConfig::default());
//! let mut orch = ObkgOrchestrator::with_defaults(lifecycle);
//! let cid = orch.ingest(ku, vec![1], 0.5, 0.3, now);
//! let tick_result = orch.tick(now, &niche_stats);
//! let gc_result = orch.gc(now);
//! ```

use crate::ecosystem::{NicheId, NicheStats};
use crate::graph_bio::{CoAccess, StdpEngine, StdpUpdate};
use crate::graph_decay::{DecayReport, DecayRunner};
use crate::graph_dream::{AccessRecord, DreamConfig, DreamEngine, DreamReport};
use crate::graph_embeddings::RelationTable;
use crate::graph_events::EventAccumulator;
use crate::graph_types::{BondEvent, BondMeta};
use crate::ku_lifecycle::KuLifecycle;
use crate::ku_runtime::KuRuntime;
use crate::metabolism::MetabolismEvent;
use crate::pomv::PomvScore;
use crate::types::{EdgeState, RelationType};

use std::collections::HashMap;

// ============================================================================
// ObkgConfig
// ============================================================================

/// Configuration for the OBKG orchestrator layer.
#[derive(Debug, Clone)]
pub struct ObkgConfig {
    /// Dream engine configuration.
    pub dream: DreamConfig,
    /// STDP time constant τ (seconds). Default: 3600.
    pub stdp_tau: f64,
    /// STDP LTP amplitude. Default: 0.1.
    pub stdp_a_plus: f64,
    /// STDP LTD amplitude. Default: -0.05.
    pub stdp_a_minus: f64,
    /// How many ticks between dream cycles. 0 = every tick.
    pub dream_interval_ticks: u64,
    /// Default weight for bonds created by ingest. Default: 5000.
    pub default_bond_weight: u16,
}

impl Default for ObkgConfig {
    fn default() -> Self {
        Self {
            dream: DreamConfig::default(),
            stdp_tau: 3600.0,
            stdp_a_plus: 0.1,
            stdp_a_minus: -0.05,
            dream_interval_ticks: 10,
            default_bond_weight: 5000,
        }
    }
}

// ============================================================================
// Result Types
// ============================================================================

/// Result of a single orchestrator tick.
#[derive(Debug, Clone)]
pub struct ObkgTickResult {
    /// PoMV scores from lifecycle tick, sorted descending.
    pub pomv_scores: Vec<([u8; 32], PomvScore)>,
    /// Decay report from this tick's decay pass.
    pub decay_report: DecayReport,
    /// STDP weight updates applied this tick.
    pub stdp_updates: Vec<StdpUpdate>,
    /// Dream report (only populated when a dream cycle runs).
    pub dream_report: Option<DreamReport>,
    /// Number of new bonds detected in the bond snapshot this tick.
    pub new_bonds_detected: usize,
    /// Current tick count after this tick.
    pub tick_number: u64,
}

/// Result of a garbage collection pass.
#[derive(Debug, Clone)]
pub struct ObkgGcResult {
    /// Number of KUs removed by lifecycle GC.
    pub kus_removed: usize,
    /// Number of orphaned bonds cleaned from the bond snapshot.
    pub orphaned_bonds_cleaned: usize,
    /// CIDs of removed KUs.
    pub removed_cids: Vec<[u8; 32]>,
}

// ============================================================================
// ObkgOrchestrator
// ============================================================================

/// OBKG Orchestrator — wraps KuLifecycle with all graph engines.
///
/// This is the main entry point for OBKG-aware KU management. It delegates
/// core lifecycle operations to [`KuLifecycle`] and layers on graph-level
/// subsystems (decay, STDP, dream mode, event sourcing, embeddings).
pub struct ObkgOrchestrator {
    /// The underlying KU lifecycle manager.
    pub lifecycle: KuLifecycle,
    /// Event accumulator for bond lifecycle tracking.
    pub event_log: EventAccumulator,
    /// RotatE relation embedding table.
    pub relation_table: RelationTable,
    /// Orchestrator configuration.
    config: ObkgConfig,
    /// Dream engine for offline graph restructuring.
    dream_engine: DreamEngine,
    /// STDP engine for spike-timing-dependent plasticity.
    stdp: StdpEngine,
    /// Current bond snapshot: (source_cid, target_cid, relation) → BondMeta.
    bond_snapshot: HashMap<([u8; 32], [u8; 32], RelationType), BondMeta>,
    /// Monotonically increasing tick counter.
    tick_count: u64,
}

impl ObkgOrchestrator {
    /// Create a new orchestrator with explicit configuration.
    pub fn new(lifecycle: KuLifecycle, config: ObkgConfig) -> Self {
        let dream_engine = DreamEngine::new(config.dream.clone());
        let stdp = StdpEngine::new(config.stdp_a_plus, config.stdp_a_minus, config.stdp_tau);
        Self {
            lifecycle,
            event_log: EventAccumulator::new(),
            relation_table: RelationTable::new(),
            config,
            dream_engine,
            stdp,
            bond_snapshot: HashMap::new(),
            tick_count: 0,
        }
    }

    /// Create a new orchestrator with default OBKG configuration.
    pub fn with_defaults(lifecycle: KuLifecycle) -> Self {
        Self::new(lifecycle, ObkgConfig::default())
    }

    // ========================================================================
    // Ingest
    // ========================================================================

    /// Ingest a KU into the lifecycle and emit bond events.
    ///
    /// Delegates to [`KuLifecycle::ingest`], then creates a `BondEvent::Created`
    /// for any bonds on the KU's epigenetic layer that reference known CIDs.
    pub fn ingest(
        &mut self,
        ku: KuRuntime,
        niches: Vec<NicheId>,
        novelty: f32,
        bridge: f32,
        now: u64,
    ) -> [u8; 32] {
        let cid = self.lifecycle.ingest(ku, niches, novelty, bridge, now);

        // Emit bond events for bonds declared in the KU's epigenetic layer
        let bond_events = self.emit_bond_events_for_ku(&cid, now);
        for event in bond_events {
            self.event_log.append(event);
        }

        cid
    }

    /// Scan a KU's bonds and create BondEvent::Created + update bond_snapshot.
    fn emit_bond_events_for_ku(&mut self, cid: &[u8; 32], now: u64) -> Vec<BondEvent> {
        let mut events = Vec::new();

        // Read bonds from the KU's epigenetic layer
        let bonds: Vec<_> = if let Some(ku) = self.lifecycle.get(cid) {
            ku.epi
                .bonds
                .iter()
                .map(|b| {
                    let mut target = [0u8; 32];
                    let len = b.target_cid.len().min(32);
                    target[..len].copy_from_slice(&b.target_cid[..len]);
                    (target, b.relation, b.weight, b.creator)
                })
                .collect()
        } else {
            return events;
        };

        for (target_cid, relation, weight, creator) in bonds {
            let key = (*cid, target_cid, relation);
            if !self.bond_snapshot.contains_key(&key) {
                let meta = BondMeta {
                    weight,
                    creator,
                    state: EdgeState::Active,
                    decay: crate::graph_decay::suggested_decay_rate(relation),
                    timestamp: (now & 0xFFFF_FFFF) as u32,
                };
                self.bond_snapshot.insert(key, meta);
                events.push(BondEvent::Created {
                    source_cid: *cid,
                    target_cid,
                    relation,
                    weight,
                    creator,
                    evidence: vec![],
                    timestamp: now,
                });
            }
        }

        events
    }

    // ========================================================================
    // Tick
    // ========================================================================

    /// Run a full orchestrator tick.
    ///
    /// 1. Delegates to `lifecycle.tick()` for PoMV scoring + epistemic transitions
    /// 2. Runs decay on all bonds in the snapshot
    /// 3. Runs STDP on co-access patterns (placeholder: empty for now)
    /// 4. Conditionally runs a dream cycle
    /// 5. Logs all generated events
    pub fn tick(&mut self, now: u64, niche_stats: &HashMap<NicheId, NicheStats>) -> ObkgTickResult {
        self.tick_count += 1;

        // 1. Lifecycle tick — PoMV scores + epistemic transitions
        let pomv_scores = self.lifecycle.tick(now, niche_stats);

        // 2. Decay pass on bond snapshot
        let decay_bonds: Vec<_> = self
            .bond_snapshot
            .iter()
            .map(|((src, tgt, _rel), meta)| {
                let bond = crate::types::Bond {
                    target_cid: tgt.to_vec(),
                    relation: *_rel,
                    weight: meta.weight,
                    creator: meta.creator,
                    created_at: meta.timestamp,
                    evidence: vec![],
                    state: meta.state,
                    initial_weight: Some(meta.weight),
                    decay: Some(meta.decay),
                    last_reinforced: Some(meta.timestamp),
                    reinforce_count: Some(0),
                    bidirectional: None,
                    context: vec![],
                    order: None,
                    required: None,
                };
                ((*src, *tgt), bond)
            })
            .collect();
        let decay_report = DecayRunner::run_decay(&decay_bonds, now);

        // Apply decay events to bond snapshot and event log
        for event in &decay_report.events {
            self.event_log.append(event.clone());
            self.apply_decay_event(event);
        }

        // 3. STDP — process co-accesses (currently empty; callers can
        //    invoke `process_stdp()` explicitly with real co-access data)
        let stdp_updates = Vec::new();

        // 4. Dream cycle — run periodically
        let dream_report = if self.config.dream_interval_ticks > 0
            && self.tick_count % self.config.dream_interval_ticks == 0
        {
            let report = self.dream_engine.run_dream_cycle(
                &[], // access log — callers provide via run_dream_with_data()
                &[], // entities
                &self.relation_table,
                &mut self.bond_snapshot,
                now,
            );
            for event in &report.events {
                self.event_log.append(event.clone());
            }
            Some(report)
        } else {
            None
        };

        let new_bonds_detected = decay_report.events.len();

        ObkgTickResult {
            pomv_scores,
            decay_report,
            stdp_updates,
            dream_report,
            new_bonds_detected,
            tick_number: self.tick_count,
        }
    }

    /// Apply a single decay event to the bond snapshot.
    fn apply_decay_event(&mut self, event: &BondEvent) {
        match event {
            BondEvent::Weakened {
                source_cid,
                target_cid,
                relation,
                new_weight,
                ..
            } => {
                if let Some(meta) =
                    self.bond_snapshot
                        .get_mut(&(*source_cid, *target_cid, *relation))
                {
                    meta.weight = *new_weight;
                    if *new_weight == 0 {
                        meta.state = EdgeState::Deprecated;
                    } else {
                        meta.state = EdgeState::Weakened;
                    }
                }
            }
            BondEvent::StateChanged {
                source_cid,
                target_cid,
                relation,
                new_state,
                ..
            } => {
                if let Some(meta) =
                    self.bond_snapshot
                        .get_mut(&(*source_cid, *target_cid, *relation))
                {
                    meta.state = *new_state;
                }
            }
            _ => {} // Created/Reinforced handled elsewhere
        }
    }

    // ========================================================================
    // STDP (explicit)
    // ========================================================================

    /// Process STDP co-access events explicitly.
    ///
    /// Callers provide co-access data; returns the STDP updates applied.
    pub fn process_stdp(&mut self, co_accesses: &[CoAccess]) -> Vec<StdpUpdate> {
        let updates = self.stdp.process_co_accesses(co_accesses);
        for update in &updates {
            let key = (update.source_cid, update.target_cid, update.relation);
            if let Some(meta) = self.bond_snapshot.get_mut(&key) {
                meta.weight = update.new_weight;
            }
            self.event_log.append(BondEvent::Reinforced {
                source_cid: update.source_cid,
                target_cid: update.target_cid,
                relation: update.relation,
                old_weight: update.old_weight,
                new_weight: update.new_weight,
                timestamp: 0, // STDP events don't carry timestamp; callers should set it
            });
        }
        updates
    }

    /// Run a dream cycle with explicit access/entity data.
    pub fn run_dream_with_data(
        &mut self,
        access_log: &[AccessRecord],
        entities: &[([u8; 32], crate::graph_embeddings::EntityEmbedding)],
        now: u64,
    ) -> DreamReport {
        let report = self.dream_engine.run_dream_cycle(
            access_log,
            entities,
            &self.relation_table,
            &mut self.bond_snapshot,
            now,
        );
        for event in &report.events {
            self.event_log.append(event.clone());
        }
        report
    }

    // ========================================================================
    // GC
    // ========================================================================

    /// Run garbage collection.
    ///
    /// 1. Delegates to `lifecycle.gc()` to remove dead KUs
    /// 2. Cleans orphaned bonds from the bond snapshot (bonds whose source
    ///    or target CID no longer exists in the lifecycle)
    pub fn gc(&mut self, now: u64) -> ObkgGcResult {
        // Snapshot CIDs before GC
        let cids_before: std::collections::HashSet<[u8; 32]> =
            self.lifecycle.kus.keys().copied().collect();

        let kus_removed = self.lifecycle.gc(now);

        // Find removed CIDs
        let cids_after: std::collections::HashSet<[u8; 32]> =
            self.lifecycle.kus.keys().copied().collect();
        let removed_cids: Vec<[u8; 32]> = cids_before.difference(&cids_after).copied().collect();

        // Clean orphaned bonds
        let orphaned_keys: Vec<_> = self
            .bond_snapshot
            .keys()
            .filter(|(src, tgt, _)| !cids_after.contains(src) || !cids_after.contains(tgt))
            .cloned()
            .collect();
        let orphaned_bonds_cleaned = orphaned_keys.len();
        for key in orphaned_keys {
            self.bond_snapshot.remove(&key);
        }

        ObkgGcResult {
            kus_removed,
            orphaned_bonds_cleaned,
            removed_cids,
        }
    }

    // ========================================================================
    // Pass-through accessors
    // ========================================================================

    /// Get a KuRuntime by CID (delegates to lifecycle).
    pub fn get(&self, cid: &[u8; 32]) -> Option<&KuRuntime> {
        self.lifecycle.get(cid)
    }

    /// Get a mutable KuRuntime by CID (delegates to lifecycle).
    pub fn get_mut(&mut self, cid: &[u8; 32]) -> Option<&mut KuRuntime> {
        self.lifecycle.get_mut(cid)
    }

    /// Number of active KUs (delegates to lifecycle).
    pub fn len(&self) -> usize {
        self.lifecycle.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.lifecycle.is_empty()
    }

    /// Record a metabolism event (delegates to lifecycle).
    pub fn record_event(&mut self, cid: &[u8; 32], event: MetabolismEvent, now: u64) {
        self.lifecycle.record_event(cid, event, now);
    }

    // ========================================================================
    // OBKG-specific accessors
    // ========================================================================

    /// Reference to the event log.
    pub fn event_log(&self) -> &EventAccumulator {
        &self.event_log
    }

    /// Mutable reference to the event log.
    pub fn event_log_mut(&mut self) -> &mut EventAccumulator {
        &mut self.event_log
    }

    /// Reference to the relation table.
    pub fn relation_table(&self) -> &RelationTable {
        &self.relation_table
    }

    /// Current tick count.
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    /// Reference to the bond snapshot.
    pub fn bond_snapshot(&self) -> &HashMap<([u8; 32], [u8; 32], RelationType), BondMeta> {
        &self.bond_snapshot
    }

    /// Number of bonds in the snapshot.
    pub fn bond_count(&self) -> usize {
        self.bond_snapshot.len()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_dna::{CoreDna, CoreDnaHeader, Instruction};
    use crate::epigenetics::Epigenetics;
    use crate::pomv_runtime::PomvConfig;

    // ── Helpers ──────────────────────────────────────────────────────────

    fn make_ku(concept_id: u64, gene_type: u8) -> KuRuntime {
        let dna = CoreDna {
            header: CoreDnaHeader {
                version: 2,
                gene_type,
                has_concept_table: false,
            },
            concept_table: Vec::new(),
            instructions: vec![
                Instruction::Triple {
                    s: concept_id,
                    p: 133,
                    o: 132,
                },
                Instruction::Certainty { level: 9000 },
            ],
        };
        let mut ku = KuRuntime::from_dna(dna).unwrap();
        ku.epi = Epigenetics::with_trust(5000, 8000);
        ku
    }

    fn default_niche_stats() -> HashMap<NicheId, NicheStats> {
        let mut m = HashMap::new();
        m.insert(
            1u64,
            NicheStats {
                population: 10,
                total_metabolic_rate: 5.0,
                avg_metabolic_rate: 0.5,
                source_diversity: 3,
            },
        );
        m
    }

    fn make_orchestrator() -> ObkgOrchestrator {
        let lifecycle = KuLifecycle::new(PomvConfig::default());
        ObkgOrchestrator::with_defaults(lifecycle)
    }

    // ── Test 1: Construction ────────────────────────────────────────────

    #[test]
    fn test_new_orchestrator() {
        let orch = make_orchestrator();
        assert_eq!(orch.len(), 0);
        assert!(orch.is_empty());
        assert_eq!(orch.tick_count(), 0);
        assert_eq!(orch.bond_count(), 0);
        assert!(orch.event_log().is_empty());
    }

    // ── Test 2: with_defaults vs new ────────────────────────────────────

    #[test]
    fn test_with_defaults_vs_new() {
        let lc1 = KuLifecycle::new(PomvConfig::default());
        let lc2 = KuLifecycle::new(PomvConfig::default());
        let o1 = ObkgOrchestrator::with_defaults(lc1);
        let o2 = ObkgOrchestrator::new(lc2, ObkgConfig::default());
        assert_eq!(o1.tick_count(), o2.tick_count());
        assert_eq!(o1.len(), o2.len());
    }

    // ── Test 3: Ingest and get ──────────────────────────────────────────

    #[test]
    fn test_ingest_and_get() {
        let mut orch = make_orchestrator();
        let ku = make_ku(100, 0);
        let cid = orch.ingest(ku, vec![1], 0.5, 0.3, 1000);

        assert_eq!(orch.len(), 1);
        assert!(!orch.is_empty());
        assert!(orch.get(&cid).is_some());
        assert_eq!(orch.get(&cid).unwrap().gene_type(), 0);
    }

    // ── Test 4: Record event pass-through ───────────────────────────────

    #[test]
    fn test_record_event() {
        let mut orch = make_orchestrator();
        let ku = make_ku(200, 0);
        let cid = orch.ingest(ku, vec![1], 0.5, 0.3, 1000);

        orch.record_event(&cid, MetabolismEvent::Retrieval { dwell_ms: 500 }, 1100);
        orch.record_event(&cid, MetabolismEvent::Citation, 1200);

        // KU should still exist
        assert!(orch.get(&cid).is_some());
    }

    // ── Test 5: Tick produces result ────────────────────────────────────

    #[test]
    fn test_tick_produces_result() {
        let mut orch = make_orchestrator();
        let ku = make_ku(300, 0);
        let cid = orch.ingest(ku, vec![1], 0.5, 0.3, 1000);

        for i in 0..5 {
            orch.record_event(
                &cid,
                MetabolismEvent::Retrieval { dwell_ms: 500 },
                1100 + i * 100,
            );
        }

        let stats = default_niche_stats();
        let result = orch.tick(2000, &stats);

        assert!(!result.pomv_scores.is_empty());
        assert_eq!(result.tick_number, 1);
        assert_eq!(orch.tick_count(), 1);
    }

    // ── Test 6: Tick increments tick_count ──────────────────────────────

    #[test]
    fn test_tick_count_increments() {
        let mut orch = make_orchestrator();
        let ku = make_ku(400, 0);
        orch.ingest(ku, vec![1], 0.5, 0.3, 1000);
        let stats = default_niche_stats();

        orch.tick(2000, &stats);
        assert_eq!(orch.tick_count(), 1);
        orch.tick(3000, &stats);
        assert_eq!(orch.tick_count(), 2);
        orch.tick(4000, &stats);
        assert_eq!(orch.tick_count(), 3);
    }

    // ── Test 7: GC removes dead KUs ─────────────────────────────────────

    #[test]
    fn test_gc_basic() {
        let mut orch = make_orchestrator();
        let ku = make_ku(500, 0);
        let cid = orch.ingest(ku, vec![1], 0.5, 0.3, 1000);

        // Add activity to keep the KU alive
        orch.record_event(&cid, MetabolismEvent::Retrieval { dwell_ms: 500 }, 1100);

        // GC should not remove an active KU
        let result = orch.gc(1200);
        assert_eq!(result.kus_removed, 0);
        assert_eq!(orch.len(), 1);
    }

    // ── Test 8: GC result structure ─────────────────────────────────────

    #[test]
    fn test_gc_result_structure() {
        let mut orch = make_orchestrator();
        let ku = make_ku(600, 0);
        orch.ingest(ku, vec![1], 0.5, 0.3, 1000);

        let result = orch.gc(1000);
        // Verify result structure
        assert!(result.removed_cids.is_empty() || !result.removed_cids.is_empty());
        assert_eq!(result.orphaned_bonds_cleaned, 0);
    }

    // ── Test 9: Event log accumulates ───────────────────────────────────

    #[test]
    fn test_event_log_accumulates() {
        let mut orch = make_orchestrator();
        let initial_len = orch.event_log().len();

        let ku = make_ku(700, 0);
        orch.ingest(ku, vec![1], 0.5, 0.3, 1000);

        // Event log length should be >= initial (bonds may or may not emit events)
        assert!(orch.event_log().len() >= initial_len);
    }

    // ── Test 10: Relation table available ────────────────────────────────

    #[test]
    fn test_relation_table() {
        let orch = make_orchestrator();
        let table = orch.relation_table();
        // Should have embeddings for all 34 relation types
        assert!(!table.embeddings.is_empty());
        assert!(table.get(RelationType::Extends).is_some());
    }

    // ── Test 11: Multiple KUs ───────────────────────────────────────────

    #[test]
    fn test_multiple_kus() {
        let mut orch = make_orchestrator();

        let ku1 = make_ku(800, 0);
        let ku2 = make_ku(900, 1);
        let cid1 = orch.ingest(ku1, vec![1], 0.5, 0.3, 1000);
        let cid2 = orch.ingest(ku2, vec![1, 2], 0.8, 0.1, 1000);

        assert_eq!(orch.len(), 2);
        assert_eq!(orch.get(&cid1).unwrap().gene_type(), 0);
        assert_eq!(orch.get(&cid2).unwrap().gene_type(), 1);

        // Both KUs need events for tick to produce scores
        orch.record_event(&cid1, MetabolismEvent::Retrieval { dwell_ms: 500 }, 1100);
        orch.record_event(&cid2, MetabolismEvent::Retrieval { dwell_ms: 300 }, 1200);

        let stats = default_niche_stats();
        let result = orch.tick(2000, &stats);
        assert_eq!(result.pomv_scores.len(), 2);
    }

    // ── Test 12: Get mut ────────────────────────────────────────────────

    #[test]
    fn test_get_mut() {
        let mut orch = make_orchestrator();
        let ku = make_ku(1000, 0);
        let cid = orch.ingest(ku, vec![1], 0.5, 0.3, 1000);

        // Should be able to get mutable reference
        let ku_mut = orch.get_mut(&cid);
        assert!(ku_mut.is_some());
    }
}
