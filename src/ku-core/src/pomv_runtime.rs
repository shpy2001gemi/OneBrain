//! # PoMV Runtime — Orchestrator for PoK v2
//!
//! Ties all 10 PoK v2 modules together into a single runtime that:
//! 1. Accepts metabolism events
//! 2. Periodically computes all 6 PoMV signals
//! 3. Updates TrustSection on each KU
//! 4. Runs epistemic status transitions
//! 5. Runs immune system analysis
//!
//! This is the "tick" function that each node runs locally.

use crate::ecosystem::{EcosystemAnalyzer, KUNicheProfile, NicheId, NicheStats};
use crate::entropy::EntropyCalculator;
use crate::epistemic_engine;
use crate::immune::ImmuneEngine;
use crate::metabolism::{KUMetabolism, MetabolismEvent, DEFAULT_HALF_LIFE_SECS};
use crate::metabolism_store::MetabolismStore;
use crate::pomv::{PomvCalculator, PomvScore, PomvSignals, PomvWeights, DEFAULT_WEIGHTS};
use crate::prediction::PredictionRegistry;
use crate::synaptic::{CentralityCalculator, SynapticMap};
use crate::types::{EpistemicStatus, TrustSection};

use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════════════════
// Runtime Configuration
// ═══════════════════════════════════════════════════════════════════════════

/// Configuration for the PoMV runtime
#[derive(Debug, Clone)]
pub struct PomvConfig {
    /// PoMV signal weights
    pub weights: PomvWeights,
    /// Half-life for metabolic decay (seconds)
    pub half_life_secs: u64,
    /// Entropy decay period (seconds)
    pub entropy_decay_secs: u64,
    /// Node ID (for metabolism store)
    pub node_id: u64,
}

impl Default for PomvConfig {
    fn default() -> Self {
        Self {
            weights: DEFAULT_WEIGHTS,
            half_life_secs: DEFAULT_HALF_LIFE_SECS,
            entropy_decay_secs: crate::entropy::ENTROPY_DECAY_PERIOD_SECS,
            node_id: 0,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Per-KU PoMV State (all signals for one KU)
// ═══════════════════════════════════════════════════════════════════════════

/// Complete PoMV state for a single KU
#[derive(Debug, Clone)]
pub struct KUPomvState {
    /// Prediction registry
    pub predictions: PredictionRegistry,
    /// Synaptic bond map
    pub synaptic: SynapticMap,
    /// Entropy at creation (frozen)
    pub entropy_at_creation: f32,
    /// Bridge score at creation (frozen)
    pub bridge_at_creation: f32,
    /// Creation timestamp
    pub created_at: u64,
    /// Number of attacks survived
    pub attacks_survived: u32,
    /// Niche IDs this KU belongs to
    pub niches: Vec<NicheId>,
    /// Cross-niche count
    pub cross_niche_count: usize,
    /// Current epistemic status
    pub epistemic_status: EpistemicStatus,
}

impl KUPomvState {
    pub fn new(created_at: u64) -> Self {
        Self {
            predictions: PredictionRegistry::new(),
            synaptic: SynapticMap::new(),
            entropy_at_creation: 0.0,
            bridge_at_creation: 0.0,
            created_at,
            attacks_survived: 0,
            niches: Vec::new(),
            cross_niche_count: 0,
            epistemic_status: EpistemicStatus::default(),
        }
    }

    /// Set initial entropy (call once at KU creation)
    pub fn set_initial_entropy(&mut self, novelty: f32, bridge: f32) {
        self.entropy_at_creation = novelty;
        self.bridge_at_creation = bridge;
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PoMV Runtime
// ═══════════════════════════════════════════════════════════════════════════

/// The main PoMV runtime — orchestrates all PoK v2 modules.
///
/// Each node runs one instance. No central coordination.
#[derive(Debug)]
pub struct PomvRuntime {
    /// Metabolism store (CRDT-based usage counters)
    pub metabolism_store: MetabolismStore,
    /// Per-KU PoMV state
    pub ku_states: HashMap<[u8; 32], KUPomvState>,
    /// Configuration
    pub config: PomvConfig,
}

impl PomvRuntime {
    pub fn new(config: PomvConfig) -> Self {
        Self {
            metabolism_store: MetabolismStore::new(config.node_id),
            ku_states: HashMap::new(),
            config,
        }
    }

    /// Register a new KU in the runtime.
    pub fn register_ku(
        &mut self,
        cid: [u8; 32],
        created_at: u64,
        niches: Vec<NicheId>,
        novelty: f32,
        bridge: f32,
    ) {
        let mut state = KUPomvState::new(created_at);
        state.set_initial_entropy(novelty, bridge);
        state.niches = niches;
        self.ku_states.insert(cid, state);
    }

    /// Record a metabolism event for a KU.
    pub fn record_event(&mut self, cid: [u8; 32], event: MetabolismEvent, timestamp: u64) {
        self.metabolism_store.record_event(cid, event, timestamp);
    }

    /// Merge remote metabolism data (from gossip).
    pub fn merge_remote_metabolism(&mut self, cid: [u8; 32], remote: &KUMetabolism) {
        self.metabolism_store.merge_remote(cid, remote);
    }

    /// Compute all 6 PoMV signals for a single KU.
    ///
    /// Returns (PomvScore, updated EpistemicStatus).
    pub fn compute_ku(
        &self,
        cid: &[u8; 32],
        now: u64,
        niche_stats: &HashMap<NicheId, NicheStats>,
    ) -> Option<(PomvScore, EpistemicStatus)> {
        let metabolism = self.metabolism_store.get(cid)?;
        let state = self.ku_states.get(cid)?;

        // Signal 1: Metabolism
        let _metabolic_rate = metabolism.metabolic_rate(now, self.config.half_life_secs);

        // Signal 2: Prediction
        let prediction_score = state.predictions.prediction_score() as f32;

        // Signal 3: Entropy (with decay)
        let age_secs = now.saturating_sub(state.created_at);
        let entropy = EntropyCalculator::entropy_value(
            state.entropy_at_creation,
            state.bridge_at_creation,
            age_secs,
        );

        // Signal 4: Survival (anti-fragile)
        let is_alive = metabolism.is_alive(now, self.config.half_life_secs);
        let survival = ImmuneEngine::survival_score(state.attacks_survived, is_alive);

        // Signal 5: Synaptic centrality — simplified (local only)
        let synaptic =
            state.synaptic.total_strength() / (state.synaptic.bond_count() as f32 + 1.0).sqrt();
        let synaptic_normalized = synaptic.clamp(0.0, 1.0);

        // Signal 6: Niche fitness
        let niche_profile = KUNicheProfile {
            niches: state.niches.clone(),
            metabolic_rate: _metabolic_rate,
            novelty: state.entropy_at_creation,
            cross_niche_count: state.cross_niche_count,
        };
        let niche_fitness = EcosystemAnalyzer::niche_fitness(&niche_profile, niche_stats);

        // Aggregate
        let signals = PomvSignals {
            metabolism: metabolism.rate_to_u16(now, self.config.half_life_secs) as f32 / 10000.0,
            prediction: prediction_score,
            entropy,
            survival,
            synaptic: synaptic_normalized,
            niche_fitness,
        };

        let score = PomvCalculator::compute(&signals, &self.config.weights);

        // Epistemic status transition
        let new_status = epistemic_engine::evaluate_max_status(
            state.epistemic_status,
            metabolism,
            now,
            self.config.half_life_secs,
        );

        Some((score, new_status))
    }

    /// Run a full tick: compute PoMV for ALL KUs and update TrustSections.
    ///
    /// Returns results sorted by PoMV score descending.
    pub fn tick(
        &mut self,
        now: u64,
        niche_stats: &HashMap<NicheId, NicheStats>,
    ) -> Vec<([u8; 32], PomvScore, TrustSectionUpdate)> {
        let mut results = Vec::new();

        let cids: Vec<[u8; 32]> = self.ku_states.keys().copied().collect();

        for cid in &cids {
            if let Some((score, new_status)) = self.compute_ku(cid, now, niche_stats) {
                let metabolism = self.metabolism_store.get(cid).unwrap();
                let state = self.ku_states.get(cid).unwrap();

                let update = TrustSectionUpdate {
                    epistemic_status: new_status,
                    metabolic_rate: metabolism.rate_to_u16(now, self.config.half_life_secs),
                    prediction_score: state.predictions.score_to_u16(),
                    entropy_at_creation: EntropyCalculator::entropy_to_u16(
                        EntropyCalculator::entropy_value(
                            state.entropy_at_creation,
                            state.bridge_at_creation,
                            now.saturating_sub(state.created_at),
                        ),
                    ),
                    survival_score: ImmuneEngine::survival_to_u16(ImmuneEngine::survival_score(
                        state.attacks_survived,
                        metabolism.is_alive(now, self.config.half_life_secs),
                    )),
                    synaptic_centrality: CentralityCalculator::centrality_to_u16(
                        state.synaptic.total_strength()
                            / (state.synaptic.bond_count() as f32 + 1.0).sqrt(),
                    ),
                    niche_fitness: EcosystemAnalyzer::fitness_to_u16(
                        score.contributions.niche_fitness,
                    ),
                    pomv_total: score.total,
                };

                results.push((*cid, score, update));
            }
        }

        // Update epistemic statuses
        for (cid, _, update) in &results {
            if let Some(state) = self.ku_states.get_mut(cid) {
                state.epistemic_status = update.epistemic_status;
            }
        }

        // Sort by PoMV score descending (for reward distribution)
        results.sort_by(|a, b| {
            b.1.total
                .partial_cmp(&a.1.total)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }

    /// Garbage-collect dead KUs from metabolism store.
    pub fn gc(&mut self, now: u64) -> usize {
        // gc_dead returns count, not CID list. We gc states separately.
        let removed_count = self.metabolism_store.gc_dead(now);
        // Also remove state entries for KUs no longer in metabolism store
        self.ku_states
            .retain(|cid, _| self.metabolism_store.get(cid).is_some());
        removed_count
    }

    /// Evaporate synaptic bonds (call periodically, e.g. daily).
    pub fn evaporate_bonds(&mut self) {
        for state in self.ku_states.values_mut() {
            state.synaptic.evaporate();
        }
    }
}

/// Fields to update on TrustSection after a tick
#[derive(Debug, Clone)]
pub struct TrustSectionUpdate {
    pub epistemic_status: EpistemicStatus,
    pub metabolic_rate: u16,
    pub prediction_score: u16,
    pub entropy_at_creation: u16,
    pub survival_score: u16,
    pub synaptic_centrality: u16,
    pub niche_fitness: u16,
    pub pomv_total: f32,
}

impl TrustSectionUpdate {
    /// Apply this update to a TrustSection.
    pub fn apply_to(&self, trust: &mut TrustSection) {
        trust.epistemic_status = self.epistemic_status;
        trust.metabolic_rate = self.metabolic_rate;
        trust.prediction_score = self.prediction_score;
        trust.entropy_at_creation = self.entropy_at_creation;
        trust.survival_score = self.survival_score;
        trust.synaptic_centrality = self.synaptic_centrality;
        trust.niche_fitness = self.niche_fitness;
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metabolism::MetabolismEvent;

    const T0: u64 = 1_000_000;
    const NODE_A: u64 = 1;

    fn test_cid(id: u8) -> [u8; 32] {
        let mut cid = [0u8; 32];
        cid[0] = id;
        cid
    }

    fn test_config() -> PomvConfig {
        PomvConfig {
            node_id: NODE_A,
            ..PomvConfig::default()
        }
    }

    #[test]
    fn test_runtime_new() {
        let rt = PomvRuntime::new(test_config());
        assert!(rt.ku_states.is_empty());
    }

    #[test]
    fn test_register_and_record() {
        let mut rt = PomvRuntime::new(test_config());
        let cid = test_cid(1);

        rt.register_ku(cid, T0, vec![42], 0.8, 0.5);
        rt.record_event(cid, MetabolismEvent::QueryHit, T0 + 100);
        rt.record_event(cid, MetabolismEvent::Retrieval { dwell_ms: 5000 }, T0 + 200);

        assert!(rt.metabolism_store.get(&cid).is_some());
        assert!(rt.ku_states.contains_key(&cid));
    }

    #[test]
    fn test_compute_single_ku() {
        let mut rt = PomvRuntime::new(test_config());
        let cid = test_cid(1);

        rt.register_ku(cid, T0, vec![42], 0.8, 0.5);

        // Record several events
        for i in 0..10 {
            rt.record_event(cid, MetabolismEvent::QueryHit, T0 + i * 100);
        }
        rt.record_event(cid, MetabolismEvent::Citation, T0 + 1000);

        let niche_stats = HashMap::new();
        let result = rt.compute_ku(&cid, T0 + 2000, &niche_stats);

        assert!(result.is_some());
        let (score, _status) = result.unwrap();
        assert!(
            score.total > 0.0,
            "Score should be positive: {}",
            score.total
        );
    }

    #[test]
    fn test_tick_multiple_kus() {
        let mut rt = PomvRuntime::new(test_config());

        // Register two KUs
        rt.register_ku(test_cid(1), T0, vec![42], 0.9, 0.5);
        rt.register_ku(test_cid(2), T0, vec![42], 0.1, 0.1);

        // KU1 is very active
        for i in 0..20 {
            rt.record_event(test_cid(1), MetabolismEvent::QueryHit, T0 + i * 100);
        }
        rt.record_event(test_cid(1), MetabolismEvent::Citation, T0 + 2000);

        // KU2 has minimal activity
        rt.record_event(test_cid(2), MetabolismEvent::QueryHit, T0 + 100);

        let niche_stats = HashMap::new();
        let results = rt.tick(T0 + 3000, &niche_stats);

        assert_eq!(results.len(), 2);
        // Results are sorted descending — KU1 should be first
        assert_eq!(results[0].0, test_cid(1), "Most active KU first");
        assert!(results[0].1.total >= results[1].1.total, "Sorted by score");
    }

    #[test]
    fn test_trust_section_update_apply() {
        let update = TrustSectionUpdate {
            epistemic_status: EpistemicStatus::Testimony,
            metabolic_rate: 5000,
            prediction_score: 7000,
            entropy_at_creation: 3000,
            survival_score: 1000,
            synaptic_centrality: 4000,
            niche_fitness: 6000,
            pomv_total: 0.65,
        };

        let mut trust = TrustSection::default();
        update.apply_to(&mut trust);

        assert_eq!(trust.epistemic_status, EpistemicStatus::Testimony);
        assert_eq!(trust.metabolic_rate, 5000);
        assert_eq!(trust.prediction_score, 7000);
        assert_eq!(trust.survival_score, 1000);
        assert_eq!(trust.synaptic_centrality, 4000);
        assert_eq!(trust.niche_fitness, 6000);
    }

    #[test]
    fn test_evaporate_bonds() {
        let mut rt = PomvRuntime::new(test_config());
        let cid = test_cid(1);

        rt.register_ku(cid, T0, vec![], 0.5, 0.5);

        // Add a bond
        rt.ku_states.get_mut(&cid).unwrap().synaptic.reinforce(
            test_cid(2),
            crate::synaptic::BondReason::CoRetrieval,
            T0,
        );

        let before = rt.ku_states[&cid].synaptic.total_strength();
        rt.evaporate_bonds();
        let after = rt.ku_states[&cid].synaptic.total_strength();

        assert!(after < before, "Evaporation reduces strength");
    }

    #[test]
    fn test_merge_remote() {
        let mut rt = PomvRuntime::new(test_config());
        let cid = test_cid(1);

        rt.register_ku(cid, T0, vec![], 0.5, 0.5);
        rt.record_event(cid, MetabolismEvent::QueryHit, T0);

        // Create remote metabolism
        let mut remote = KUMetabolism::new(T0);
        remote.record_event(2, MetabolismEvent::Citation, T0 + 100);
        remote.record_event(3, MetabolismEvent::QueryHit, T0 + 200);

        rt.merge_remote_metabolism(cid, &remote);

        let merged = rt.metabolism_store.get(&cid).unwrap();
        let total = merged.total_engagement();
        assert!(
            total >= 3,
            "Merged should have local + remote events: {}",
            total
        );
    }

    #[test]
    fn test_full_lifecycle() {
        let mut rt = PomvRuntime::new(test_config());
        let cid = test_cid(1);

        // 1. Register KU with high novelty
        rt.register_ku(cid, T0, vec![10, 20], 0.9, 0.7);

        // 2. Simulate organic usage over time
        let events = [
            (T0 + 100, MetabolismEvent::QueryHit),
            (T0 + 200, MetabolismEvent::Retrieval { dwell_ms: 15000 }),
            (T0 + 500, MetabolismEvent::QueryHit),
            (T0 + 800, MetabolismEvent::Citation),
            (T0 + 1200, MetabolismEvent::Derivative),
            (T0 + 2000, MetabolismEvent::QueryHit),
            (T0 + 3000, MetabolismEvent::Retrieval { dwell_ms: 30000 }),
            (T0 + 5000, MetabolismEvent::Citation),
            (T0 + 8000, MetabolismEvent::Refutation),
        ];

        for (ts, event) in &events {
            rt.record_event(cid, event.clone(), *ts);
        }

        // 3. Compute PoMV
        let niche_stats = HashMap::new();
        let result = rt.compute_ku(&cid, T0 + 10000, &niche_stats).unwrap();

        // Should have a meaningful score
        assert!(
            result.0.total > 0.1,
            "Active KU should have score > 0.1: {}",
            result.0.total
        );

        // 4. Apply to TrustSection
        let mut trust = TrustSection::default();
        let results = rt.tick(T0 + 10000, &niche_stats);
        assert!(!results.is_empty());
        results[0].2.apply_to(&mut trust);

        assert!(trust.metabolic_rate > 0, "Metabolic rate should be set");
        assert!(trust.entropy_at_creation > 0, "Entropy should be set");
    }

    #[test]
    fn test_epistemic_status_advances() {
        let mut rt = PomvRuntime::new(test_config());
        let cid = test_cid(1);

        rt.register_ku(cid, T0, vec![], 0.5, 0.5);

        // Generate enough activity to advance status
        for i in 0..50 {
            rt.record_event(cid, MetabolismEvent::QueryHit, T0 + i * 10);
        }
        for i in 0..5 {
            rt.record_event(
                cid,
                MetabolismEvent::Retrieval { dwell_ms: 10000 },
                T0 + 1000 + i * 100,
            );
        }
        for _ in 0..3 {
            rt.record_event(cid, MetabolismEvent::Citation, T0 + 2000);
        }

        let niche_stats = HashMap::new();
        let results = rt.tick(T0 + 3000, &niche_stats);

        assert!(!results.is_empty());
        let new_status = results[0].2.epistemic_status;
        // Should have advanced beyond Rumor
        assert!(
            new_status as u8 > EpistemicStatus::Rumor as u8,
            "Status should advance: {:?}",
            new_status
        );
    }
}
