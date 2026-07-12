//! KU Lifecycle — Integration layer between KuRuntime and PomvRuntime.
//!
//! This module orchestrates the full lifecycle:
//! 1. **Creation**: Create KuRuntime + register in PomvRuntime
//! 2. **Events**: Record metabolism events (access, citation, challenge)
//! 3. **Tick**: Compute PoMV scores → apply epistemic transitions → update KuRuntime
//! 4. **GC**: Remove dead KUs from both runtimes
//!
//! ## Example
//! ```ignore
//! let mut store = KuLifecycle::new(PomvConfig::default());
//! let ku = KuRuntime::from_dna(dna).unwrap();
//! let cid = store.ingest(ku, vec![1], 0.5, 0.3, now);
//! store.record_event(&cid, MetabolismEvent::Retrieved { by_node: 1 }, now);
//! let updates = store.tick(now, &niche_stats);
//! ```

use crate::encoding_consensus::EncodingStatus;
use crate::ku_runtime::KuRuntime;
use crate::pomv_runtime::{PomvRuntime, PomvConfig};
use crate::pomv::PomvScore;
use crate::ecosystem::{NicheId, NicheStats};
use crate::metabolism::MetabolismEvent;
use std::collections::HashMap;

// ============================================================================
// KuLifecycle
// ============================================================================

/// Full lifecycle manager tying KuRuntime storage with PomvRuntime computation.
pub struct KuLifecycle {
    /// All KuRuntime instances by CID.
    pub kus: HashMap<[u8; 32], KuRuntime>,
    /// PoMV runtime for scoring and epistemic transitions.
    pub pomv: PomvRuntime,
}

impl KuLifecycle {
    /// Create a new lifecycle manager.
    pub fn new(config: PomvConfig) -> Self {
        Self {
            kus: HashMap::new(),
            pomv: PomvRuntime::new(config),
        }
    }

    /// Ingest a KuRuntime: store it and register with PomvRuntime.
    ///
    /// Returns the CID bytes for future reference.
    pub fn ingest(
        &mut self,
        ku: KuRuntime,
        niches: Vec<NicheId>,
        novelty: f32,
        bridge: f32,
        now: u64,
    ) -> [u8; 32] {
        let cid = ku.cid_bytes();

        // Register with PomvRuntime (metabolism, entropy, ecosystem)
        self.pomv.register_ku(cid, now, niches, novelty, bridge);

        // Store KuRuntime
        self.kus.insert(cid, ku);

        cid
    }

    /// Record a metabolism event (access, citation, challenge, etc.).
    pub fn record_event(&mut self, cid: &[u8; 32], event: MetabolismEvent, now: u64) {
        self.pomv.record_event(*cid, event, now);
    }

    /// Run a tick: compute PoMV scores, evaluate epistemic transitions,
    /// and apply TrustSectionUpdates back to KuRuntimes.
    ///
    /// Returns scored results for reward distribution.
    pub fn tick(
        &mut self,
        now: u64,
        niche_stats: &HashMap<NicheId, NicheStats>,
    ) -> Vec<([u8; 32], PomvScore)> {
        let results = self.pomv.tick(now, niche_stats);

        let mut scored = Vec::new();
        for (cid, score, update) in &results {
            // Apply trust update to KuRuntime
            if let Some(ku) = self.kus.get_mut(cid) {
                ku.apply_pomv_update(update);
            }
            scored.push((*cid, score.clone()));
        }

        scored
    }

    /// Garbage-collect dead KUs from both stores.
    pub fn gc(&mut self, now: u64) -> usize {
        let removed = self.pomv.gc(now);

        // Remove KUs whose metabolism is dead
        let dead_cids: Vec<[u8; 32]> = self.kus.keys()
            .filter(|cid| !self.pomv.ku_states.contains_key(*cid))
            .copied()
            .collect();

        for cid in &dead_cids {
            self.kus.remove(cid);
        }

        removed + dead_cids.len()
    }

    /// Get a KuRuntime by CID.
    pub fn get(&self, cid: &[u8; 32]) -> Option<&KuRuntime> {
        self.kus.get(cid)
    }

    /// Get a mutable KuRuntime by CID.
    pub fn get_mut(&mut self, cid: &[u8; 32]) -> Option<&mut KuRuntime> {
        self.kus.get_mut(cid)
    }

    /// Number of active KUs.
    pub fn len(&self) -> usize {
        self.kus.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.kus.is_empty()
    }

    // ========================================================================
    // Encoding Consensus Integration
    // ========================================================================

    /// Advance a KU's encoding status.
    ///
    /// Status can only move forward: RAW → SELF → PART → FULL.
    /// FULL is immutable — cannot be changed.
    ///
    /// Returns `true` if the status was advanced, `false` if rejected.
    pub fn advance_encoding_status(
        &mut self,
        cid: &[u8; 32],
        new_status: EncodingStatus,
    ) -> bool {
        if let Some(ku) = self.kus.get_mut(cid) {
            let current = ku.encoding_status as u8;
            let target = new_status as u8;

            // Only forward transitions allowed; FULL is immutable
            if target > current && !ku.encoding_status.is_finalized() {
                ku.encoding_status = new_status;
                return true;
            }
        }
        false
    }

    /// Get all KUs that still need encoding verification.
    pub fn pending_encodings(&self) -> Vec<[u8; 32]> {
        self.kus.iter()
            .filter(|(_, ku)| ku.encoding_status.needs_verification())
            .map(|(cid, _)| *cid)
            .collect()
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
    use crate::types::EpistemicStatus;

    fn make_ku(concept_id: u64, gene_type: u8) -> KuRuntime {
        let dna = CoreDna {
            header: CoreDnaHeader { version: 2, gene_type, has_concept_table: false },
            concept_table: Vec::new(),
            instructions: vec![
                Instruction::Triple { s: concept_id, p: 133, o: 132 },
                Instruction::Certainty { level: 9000 },
            ],
        };
        let mut ku = KuRuntime::from_dna(dna).unwrap();
        ku.epi = Epigenetics::with_trust(5000, 8000);
        ku
    }

    #[test]
    fn test_ingest_and_get() {
        let mut store = KuLifecycle::new(PomvConfig::default());
        let ku = make_ku(100, 0);
        let cid = store.ingest(ku, vec![1], 0.5, 0.3, 1000);

        assert_eq!(store.len(), 1);
        assert!(store.get(&cid).is_some());
        assert_eq!(store.get(&cid).unwrap().gene_type(), 0);
    }

    #[test]
    fn test_record_event() {
        let mut store = KuLifecycle::new(PomvConfig::default());
        let ku = make_ku(200, 0);
        let cid = store.ingest(ku, vec![1], 0.5, 0.3, 1000);

        // Record several events
        store.record_event(&cid, MetabolismEvent::Retrieval { dwell_ms: 500 }, 1100);
        store.record_event(&cid, MetabolismEvent::Retrieval { dwell_ms: 300 }, 1200);
        store.record_event(&cid, MetabolismEvent::Citation, 1300);

        // KU should still exist
        assert!(store.get(&cid).is_some());
    }

    #[test]
    fn test_tick_applies_pomv_updates() {
        let mut store = KuLifecycle::new(PomvConfig::default());
        let ku = make_ku(300, 0);
        let cid = store.ingest(ku, vec![1], 0.5, 0.3, 1000);

        // Generate activity
        for i in 0..5 {
            store.record_event(&cid, MetabolismEvent::Retrieval { dwell_ms: 500 }, 1100 + i * 100);
        }
        store.record_event(&cid, MetabolismEvent::Citation, 2000);

        // Tick
        let mut niche_stats = HashMap::new();
        niche_stats.insert(1u64, NicheStats {
            population: 10,
            total_metabolic_rate: 5.0,
            avg_metabolic_rate: 0.5,
            source_diversity: 3,
        });
        let results = store.tick(3000, &niche_stats);

        // Should have results
        assert!(!results.is_empty());

        // KuRuntime should have updated trust section
        let ku = store.get(&cid).unwrap();
        // After tick, metabolic_rate should be > 0
        assert!(ku.epi.trust.metabolic_rate > 0, "metabolic_rate should be updated by tick");
    }

    #[test]
    fn test_lifecycle_epistemic_progression() {
        let mut store = KuLifecycle::new(PomvConfig::default());
        let ku = make_ku(400, 0); // Fact
        let cid = store.ingest(ku, vec![1], 0.5, 0.3, 1000);

        // Initially should be Rumor
        assert_eq!(store.get(&cid).unwrap().epi.trust.epistemic_status, EpistemicStatus::Rumor);

        // Generate enough activity for RUMOR → HEARSAY (metabolic activity)
        store.record_event(&cid, MetabolismEvent::Retrieval { dwell_ms: 500 }, 1100);

        let mut niche_stats = HashMap::new();
        niche_stats.insert(1u64, NicheStats {
            population: 10,
            total_metabolic_rate: 5.0,
            avg_metabolic_rate: 0.5,
            source_diversity: 3,
        });
        store.tick(2000, &niche_stats);

        // Epistemic status should have advanced (at least HEARSAY = 1)
        let status = store.get(&cid).unwrap().epi.trust.epistemic_status;
        assert!(status != EpistemicStatus::Rumor, "Expected at least HEARSAY, got {:?}", status);
    }

    #[test]
    fn test_multiple_kus() {
        let mut store = KuLifecycle::new(PomvConfig::default());

        let ku1 = make_ku(500, 0); // Fact
        let ku2 = make_ku(600, 1); // Hypothesis
        let cid1 = store.ingest(ku1, vec![1], 0.5, 0.3, 1000);
        let cid2 = store.ingest(ku2, vec![1, 2], 0.8, 0.1, 1000);

        assert_eq!(store.len(), 2);
        assert_eq!(store.get(&cid1).unwrap().gene_type(), 0);
        assert_eq!(store.get(&cid2).unwrap().gene_type(), 1);
    }
}
