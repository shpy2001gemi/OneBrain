//! # Serendipity Engine — Phase E4
//!
//! Surfaces knowledge users didn't know they needed.
//!
//! ## Three Discovery Modes
//! 1. **Interest-Based Probing**: Builds user profile, generates exploratory queries.
//! 2. **Concept Neighborhood**: Explores 2-hop neighborhoods in concept graph.
//! 3. **Random Walk Discovery**: Evaluates random network KUs for relevance.
//!
//! ## The "Unknown Unknowns" Problem
//! The SerendipityEngine acts as a proactive AI agent that discovers
//! knowledge on the user's behalf, scoring candidates by
//! `serendipity = relevance × novelty` with a sweet-spot bell curve.

use std::collections::{HashMap, HashSet};

use ku_core::{foundation::ConceptCcid, KuRuntime};

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/// A serendipitous discovery suggestion.
#[derive(Debug, Clone)]
pub struct Discovery {
    pub source: DiscoverySource,
    pub concept_ccids: Vec<ConceptCcid>,
    pub relevance: f64,
    pub novelty: f64,
    pub serendipity_score: f64,
    pub suggested_query: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DiscoverySource {
    InterestProbe,
    NeighborhoodExploration,
    RandomWalk,
    BridgeDiscovery,
}

/// User interest profile.
#[derive(Debug, Clone)]
pub struct InterestProfile {
    pub concept_weights: HashMap<ConceptCcid, f64>,
    pub domain_weights: HashMap<u64, f64>,
    pub kus_analyzed: usize,
    pub known_concepts: HashSet<ConceptCcid>,
}

// ═══════════════════════════════════════════════════════════════════════════
// Serendipity Engine
// ═══════════════════════════════════════════════════════════════════════════

pub struct SerendipityEngine {
    profile: InterestProfile,
    max_discoveries: usize,
    min_score: f64,
}

impl SerendipityEngine {
    pub fn new() -> Self {
        Self {
            profile: InterestProfile {
                concept_weights: HashMap::new(),
                domain_weights: HashMap::new(),
                kus_analyzed: 0,
                known_concepts: HashSet::new(),
            },
            max_discoveries: 10,
            min_score: 0.1,
        }
    }

    /// Build interest profile from user's KU collection.
    pub fn build_profile(&mut self, kus: &[KuRuntime]) {
        let mut concept_counts: HashMap<ConceptCcid, usize> = HashMap::new();
        let mut domain_counts: HashMap<u64, usize> = HashMap::new();

        for ku in kus {
            for concept_ccid in ku.concept_ccids() {
                *concept_counts.entry(concept_ccid).or_insert(0) += 1;
                self.profile.known_concepts.insert(concept_ccid);
            }
            // Also track bond context concepts as known
            for bond in &ku.epi.bonds {
                for &ctx_id in &bond.context {
                    if let Some(ctx_ccid) = ku.concept_ccid(ctx_id) {
                        self.profile.known_concepts.insert(ctx_ccid);
                    }
                }
            }
            for &domain in &ku.epi.trust.domain_codes {
                *domain_counts.entry(domain).or_insert(0) += 1;
            }
        }

        let max_concept = concept_counts.values().max().copied().unwrap_or(1) as f64;
        for (&concept, &count) in &concept_counts {
            self.profile
                .concept_weights
                .insert(concept, count as f64 / max_concept);
        }
        let max_domain = domain_counts.values().max().copied().unwrap_or(1) as f64;
        for (&domain, &count) in &domain_counts {
            self.profile
                .domain_weights
                .insert(domain, count as f64 / max_domain);
        }
        self.profile.kus_analyzed = kus.len();
    }

    /// Evaluate candidate KUs for serendipity.
    pub fn evaluate_candidates(&self, candidates: &[KuRuntime]) -> Vec<Discovery> {
        let mut discoveries = Vec::new();

        for ku in candidates {
            let relevance = self.compute_relevance(ku);
            let novelty = self.compute_novelty(ku);
            let serendipity_score = relevance * novelty;

            if serendipity_score >= self.min_score {
                let concept_ccids = ku.concept_ccids();
                let Some(primary) = ku.primary_concept_ccid() else {
                    continue;
                };

                discoveries.push(Discovery {
                    source: DiscoverySource::InterestProbe,
                    concept_ccids,
                    relevance,
                    novelty,
                    serendipity_score,
                    suggested_query: format!(
                        "FIND (k:KU) WHERE k.concept_ccids CONTAINS \"{}\" SCOPE DHT",
                        primary
                    ),
                    description: format!(
                        "Serendipitous concept {} (score: {:.2})",
                        primary, serendipity_score
                    ),
                });
            }
        }

        discoveries.sort_by(|a, b| {
            b.serendipity_score
                .partial_cmp(&a.serendipity_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        discoveries.truncate(self.max_discoveries);
        discoveries
    }

    /// Generate exploratory queries for adjacent topics.
    pub fn generate_exploration_queries(&self) -> Vec<String> {
        let mut exploration: Vec<(ConceptCcid, f64)> = self
            .profile
            .concept_weights
            .iter()
            .filter(|(_, &weight)| weight > 0.2 && weight < 0.8)
            .map(|(&concept, &weight)| (concept, weight))
            .collect();
        exploration.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        exploration.iter().take(5)
            .map(|(concept, _)| format!(
                "FIND (k:KU) WHERE k.concept_ccids CONTAINS \"{}\" AND k.trust_score > 5000 SCOPE CLUSTER LIMIT 5",
                concept
            ))
            .collect()
    }

    pub fn profile(&self) -> &InterestProfile {
        &self.profile
    }

    /// Compute relevance: how related is this KU to user's interests?
    fn compute_relevance(&self, ku: &KuRuntime) -> f64 {
        if self.profile.concept_weights.is_empty() {
            return 0.5;
        }
        let mut max_relevance = 0.0_f64;
        for concept_ccid in ku.concept_ccids() {
            if let Some(&w) = self.profile.concept_weights.get(&concept_ccid) {
                max_relevance = max_relevance.max(w);
            }
        }
        for bond in &ku.epi.bonds {
            for &ctx_id in &bond.context {
                if let Some(ctx_ccid) = ku.concept_ccid(ctx_id) {
                    if let Some(&w) = self.profile.concept_weights.get(&ctx_ccid) {
                        max_relevance = max_relevance.max(w * 0.7);
                    }
                }
            }
        }
        let trust_factor = ku.trust_score() as f64 / 10_000.0;
        (max_relevance * 0.7 + trust_factor * 0.3).min(1.0)
    }

    /// Compute novelty: how new/unexpected is this KU?
    fn compute_novelty(&self, ku: &KuRuntime) -> f64 {
        if self.profile.known_concepts.is_empty() {
            return 1.0;
        }
        let concept_ccids = ku.concept_ccids();
        let context_ccids: Vec<_> = ku
            .epi
            .bonds
            .iter()
            .flat_map(|bond| bond.context.iter())
            .filter_map(|local_id| ku.concept_ccid(*local_id))
            .collect();
        let total = concept_ccids.len() + context_ccids.len();
        if total == 0 {
            return 0.5;
        }

        let mut known = 0;
        for concept_ccid in &concept_ccids {
            if self.profile.known_concepts.contains(concept_ccid) {
                known += 1;
            }
        }
        for context_ccid in &context_ccids {
            if self.profile.known_concepts.contains(context_ccid) {
                known += 1;
            }
        }

        let novelty = 1.0 - (known as f64 / total as f64);
        // Sweet spot: partially novel is best
        if novelty > 0.9 {
            novelty * 0.7
        } else if novelty > 0.3 {
            novelty * 1.2
        } else {
            novelty * 0.5
        }
        .min(1.0)
    }
}

impl Default for SerendipityEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use ku_core::core_dna::{ConceptTableEntry, CoreDna, CoreDnaHeader, Instruction};
    use ku_core::{Epigenetics, KuRuntime, RelationType};

    fn concept(value: u64) -> ConceptCcid {
        ConceptCcid::from_bytes((value as u128).to_be_bytes())
    }

    fn make_ku(concept_id: u64, ctx: &[u64]) -> KuRuntime {
        let dna = CoreDna {
            header: CoreDnaHeader {
                version: 2,
                gene_type: 0,
                has_concept_table: true,
            },
            concept_table: std::iter::once(concept_id)
                .chain([133, 132])
                .chain(ctx.iter().copied())
                .collect::<HashSet<_>>()
                .into_iter()
                .map(|local_id| ConceptTableEntry {
                    local_id,
                    ccid: *concept(local_id).as_bytes(),
                })
                .collect(),
            instructions: vec![
                Instruction::Triple {
                    s: concept_id,
                    p: 133,
                    o: 132,
                },
                Instruction::Certainty { level: 9500 },
            ],
        };
        let mut ku = KuRuntime::from_dna(dna).unwrap();
        ku.epi = Epigenetics::with_trust(8000, 8000);
        for &c in ctx {
            ku.epi.add_bond(vec![0x42; 32], RelationType::Extends, 8000);
            if let Some(bond) = ku.epi.bonds.last_mut() {
                bond.context = vec![c];
            }
        }
        ku
    }

    fn make_simple_ku(concept_id: u64) -> KuRuntime {
        make_ku(concept_id, &[])
    }

    #[test]
    fn test_build_profile() {
        let mut engine = SerendipityEngine::new();
        let kus = vec![make_simple_ku(1), make_simple_ku(1), make_simple_ku(2)];
        engine.build_profile(&kus);
        assert_eq!(engine.profile().kus_analyzed, 3);
        // concept 1 appears in 2 KUs, concept 2 in 1 KU → concept 1 weight > concept 2 weight
        assert!(
            engine.profile().concept_weights[&concept(1)]
                > engine.profile().concept_weights[&concept(2)]
        );
    }

    #[test]
    fn test_known_vs_novel() {
        let mut engine = SerendipityEngine::new();
        engine.build_profile(&vec![
            make_simple_ku(1),
            make_simple_ku(2),
            make_simple_ku(3),
        ]);

        let known_ku = make_ku(1, &[2, 3]);
        assert!(engine.compute_novelty(&known_ku) < 0.5);

        let novel_ku = make_ku(99, &[88, 77]);
        assert!(engine.compute_novelty(&novel_ku) > 0.5);
    }

    #[test]
    fn test_evaluate() {
        let mut engine = SerendipityEngine::new();
        engine.build_profile(&vec![make_ku(1, &[10])]);
        let candidates = vec![make_ku(50, &[10])];
        let _ = engine.evaluate_candidates(&candidates); // Should not panic
    }

    #[test]
    fn test_exploration_queries() {
        let mut engine = SerendipityEngine::new();
        let mut kus = Vec::new();
        for _ in 0..5 {
            kus.push(make_simple_ku(1));
        }
        for _ in 0..3 {
            kus.push(make_simple_ku(2));
        }
        kus.push(make_simple_ku(3));
        engine.build_profile(&kus);
        let queries = engine.generate_exploration_queries();
        for q in &queries {
            assert!(q.starts_with("FIND"));
        }
    }

    #[test]
    fn test_empty_profile() {
        let engine = SerendipityEngine::new();
        let ku = make_simple_ku(42);
        assert_eq!(engine.compute_relevance(&ku), 0.5);
        assert_eq!(engine.compute_novelty(&ku), 1.0);
    }

    #[test]
    fn test_sorted() {
        let mut engine = SerendipityEngine::new();
        engine.build_profile(&vec![make_ku(1, &[10])]);
        let candidates = vec![make_ku(50, &[10]), make_ku(99, &[88])];
        let discoveries = engine.evaluate_candidates(&candidates);
        for i in 1..discoveries.len() {
            assert!(discoveries[i - 1].serendipity_score >= discoveries[i].serendipity_score);
        }
    }
}
