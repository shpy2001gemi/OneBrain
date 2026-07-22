//! # Knowledge Gap Detector — Phase E2
//!
//! Identifies missing knowledge by analyzing the local knowledge graph.
//!
//! ## Gap Types
//! - **Orphan concepts**: Referenced in bond contexts but no KU defines them.
//! - **Low-confidence regions**: Clusters of KUs with low trust scores.
//! - **Missing evidence**: KUs with trust but zero corroboration.
//! - **Untested hypotheses**: Hypothesis KUs without corroboration or challenge.

use std::collections::HashMap;

use ku_core::KuRuntime;

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/// A detected knowledge gap.
#[derive(Debug, Clone)]
pub struct KnowledgeGap {
    /// Type of gap detected.
    pub gap_type: GapType,
    /// Severity score [0.0, 1.0]. Higher = more important to fill.
    pub severity: f64,
    /// Related concept IDs.
    pub concept_ids: Vec<u64>,
    /// Suggested KQL query to fill this gap.
    pub suggested_query: String,
    /// Human-readable description.
    pub description: String,
}

/// Types of knowledge gaps.
#[derive(Debug, Clone, PartialEq)]
pub enum GapType {
    OrphanConcept,
    LowConfidenceRegion,
    MissingEvidence,
    UntestedHypothesis,
}

/// Report from a gap detection run.
#[derive(Debug, Clone)]
pub struct GapReport {
    pub gaps: Vec<KnowledgeGap>,
    pub kus_analyzed: usize,
    pub concepts_seen: usize,
}

// ═══════════════════════════════════════════════════════════════════════════
// Gap Detector
// ═══════════════════════════════════════════════════════════════════════════

pub struct GapDetector {
    trust_threshold: u16,
    max_gaps: usize,
}

impl GapDetector {
    pub fn new() -> Self {
        Self {
            trust_threshold: 3000,
            max_gaps: 50,
        }
    }

    pub fn with_params(trust_threshold: u16, max_gaps: usize) -> Self {
        Self {
            trust_threshold,
            max_gaps,
        }
    }

    /// Run gap detection on a collection of KUs.
    pub fn analyze(&self, kus: &[KuRuntime]) -> GapReport {
        let mut gaps = Vec::new();
        let (defined_concepts, referenced_concepts) = self.build_concept_maps(kus);

        gaps.extend(self.find_orphans(&defined_concepts, &referenced_concepts));
        gaps.extend(self.find_low_confidence(kus));
        gaps.extend(self.find_missing_evidence(kus));
        gaps.extend(self.find_untested_hypotheses(kus));

        gaps.sort_by(|a, b| {
            b.severity
                .partial_cmp(&a.severity)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        gaps.truncate(self.max_gaps);

        GapReport {
            gaps,
            kus_analyzed: kus.len(),
            concepts_seen: defined_concepts.len() + referenced_concepts.len(),
        }
    }

    /// Build maps of defined (in codons) and referenced (in bond contexts) concepts.
    fn build_concept_maps(&self, kus: &[KuRuntime]) -> (HashMap<u64, usize>, HashMap<u64, usize>) {
        let mut defined: HashMap<u64, usize> = HashMap::new();
        let mut referenced: HashMap<u64, usize> = HashMap::new();

        for ku in kus {
            for concept_id in ku.concept_ids() {
                *defined.entry(concept_id).or_insert(0) += 1;
            }
            // Bond context concept IDs are references to related concepts
            for bond in &ku.epi.bonds {
                for &ctx_id in &bond.context {
                    *referenced.entry(ctx_id).or_insert(0) += 1;
                }
            }
        }
        (defined, referenced)
    }

    fn find_orphans(
        &self,
        defined: &HashMap<u64, usize>,
        referenced: &HashMap<u64, usize>,
    ) -> Vec<KnowledgeGap> {
        referenced
            .iter()
            .filter(|(cid, _)| !defined.contains_key(cid))
            .map(|(&concept_id, &ref_count)| KnowledgeGap {
                gap_type: GapType::OrphanConcept,
                severity: (ref_count as f64 / 10.0).min(1.0),
                concept_ids: vec![concept_id],
                suggested_query: format!(
                    "FIND (k:KU) WHERE k.codons CONTAINS concept_id = {} SCOPE DHT",
                    concept_id
                ),
                description: format!(
                    "Concept {} referenced {} times but has no defining KU",
                    concept_id, ref_count
                ),
            })
            .collect()
    }

    fn find_low_confidence(&self, kus: &[KuRuntime]) -> Vec<KnowledgeGap> {
        let mut low_conf: HashMap<u64, Vec<u16>> = HashMap::new();
        for ku in kus {
            let ts = ku.trust_score();
            if ts < self.trust_threshold {
                if let Some(primary) = ku.primary_concept() {
                    low_conf.entry(primary).or_default().push(ts);
                }
            }
        }
        low_conf.into_iter()
            .filter(|(_, scores)| scores.len() >= 2)
            .map(|(concept_id, scores)| {
                let avg = scores.iter().map(|&s| s as f64).sum::<f64>() / scores.len() as f64;
                KnowledgeGap {
                    gap_type: GapType::LowConfidenceRegion,
                    severity: 1.0 - (avg / self.trust_threshold as f64),
                    concept_ids: vec![concept_id],
                    suggested_query: format!(
                        "FIND (k:KU) WHERE k.codons CONTAINS concept_id = {} AND k.trust_score > {} SCOPE CLUSTER",
                        concept_id, self.trust_threshold
                    ),
                    description: format!(
                        "Concept {} has {} KUs with avg trust {:.0} (threshold: {})",
                        concept_id, scores.len(), avg, self.trust_threshold
                    ),
                }
            })
            .collect()
    }

    fn find_missing_evidence(&self, kus: &[KuRuntime]) -> Vec<KnowledgeGap> {
        kus.iter()
            .filter(|ku| {
                ku.epi.trust.corroboration_count == 0 && ku.trust_score() > 0
            })
            .filter_map(|ku| {
                let ts = ku.trust_score();
                ku.primary_concept().map(|primary| {
                    KnowledgeGap {
                        gap_type: GapType::MissingEvidence,
                        severity: 0.5 + (ts as f64 / 20_000.0),
                        concept_ids: vec![primary],
                        suggested_query: format!(
                            "FIND (k:KU) WHERE k.codons CONTAINS concept_id = {} AND k.corroboration_count > 0 SCOPE CLUSTER",
                            primary
                        ),
                        description: format!(
                            "Concept {} has trust {} but zero corroboration", primary, ts
                        ),
                    }
                })
            })
            .collect()
    }

    fn find_untested_hypotheses(&self, kus: &[KuRuntime]) -> Vec<KnowledgeGap> {
        kus.iter()
            .filter(|ku| ku.gene_type() == 1) // Hypothesis gene type
            .filter(|ku| ku.epi.trust.corroboration_count == 0 && ku.epi.trust.challenge_count == 0)
            .filter_map(|ku| {
                ku.primary_concept().map(|primary| KnowledgeGap {
                    gap_type: GapType::UntestedHypothesis,
                    severity: 0.8,
                    concept_ids: vec![primary],
                    suggested_query: format!(
                        "FIND (k:KU) WHERE k.codons CONTAINS concept_id = {} SCOPE DHT",
                        primary
                    ),
                    description: format!("Hypothesis about concept {} untested", primary),
                })
            })
            .collect()
    }
}

impl Default for GapDetector {
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
    use ku_core::core_dna::{CoreDna, CoreDnaHeader, Instruction};
    use ku_core::{Epigenetics, KuRuntime, RelationType};

    fn make_ku(concept_id: u64, trust_score: u16) -> KuRuntime {
        let dna = CoreDna {
            header: CoreDnaHeader {
                version: 2,
                gene_type: 0,
                has_concept_table: false,
            },
            concept_table: Vec::new(),
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
        ku.epi = Epigenetics::with_trust(trust_score, 8000);
        ku.epi.trust.corroboration_count = 3;
        ku
    }

    fn make_ku_with_ctx(trust_score: u16, concept_id: u64, ctx_concepts: &[u64]) -> KuRuntime {
        let mut ku = make_ku(concept_id, trust_score);
        ku.epi.add_bond(vec![0x42; 32], RelationType::Extends, 8000);
        if let Some(bond) = ku.epi.bonds.last_mut() {
            bond.context = ctx_concepts.iter().copied().collect();
        }
        ku
    }

    fn make_low_trust_ku(concept_id: u64) -> KuRuntime {
        let mut ku = make_ku(concept_id, 1000);
        ku.epi.bonds.clear();
        ku
    }

    #[test]
    fn test_orphan_detection() {
        let detector = GapDetector::new();
        let kus = vec![
            make_ku_with_ctx(9000, 1, &[99]),
            make_ku_with_ctx(8000, 2, &[99]),
        ];
        let report = detector.analyze(&kus);
        let orphans: Vec<_> = report
            .gaps
            .iter()
            .filter(|g| g.gap_type == GapType::OrphanConcept)
            .collect();
        assert!(!orphans.is_empty(), "Should detect orphan concept 99");
    }

    #[test]
    fn test_low_confidence() {
        let detector = GapDetector::new();
        let kus = vec![make_low_trust_ku(42), make_low_trust_ku(42)];
        let report = detector.analyze(&kus);
        let low: Vec<_> = report
            .gaps
            .iter()
            .filter(|g| g.gap_type == GapType::LowConfidenceRegion)
            .collect();
        assert!(!low.is_empty());
    }

    #[test]
    fn test_missing_evidence() {
        let detector = GapDetector::new();
        let mut ku = make_ku_with_ctx(7000, 42, &[]);
        ku.epi.trust.corroboration_count = 0;
        let report = detector.analyze(&[ku]);
        let missing: Vec<_> = report
            .gaps
            .iter()
            .filter(|g| g.gap_type == GapType::MissingEvidence)
            .collect();
        assert!(!missing.is_empty());
    }

    #[test]
    fn test_untested_hypothesis() {
        let detector = GapDetector::new();
        // gene_type: 1 = Hypothesis
        let dna = CoreDna {
            header: CoreDnaHeader {
                version: 2,
                gene_type: 1,
                has_concept_table: false,
            },
            concept_table: Vec::new(),
            instructions: vec![
                Instruction::Triple {
                    s: 42,
                    p: 133,
                    o: 132,
                },
                Instruction::Certainty { level: 5000 },
            ],
        };
        let mut ku = KuRuntime::from_dna(dna).unwrap();
        ku.epi = Epigenetics::with_trust(3000, 5000);
        ku.epi.trust.corroboration_count = 0;
        ku.epi.trust.challenge_count = 0;
        let report = detector.analyze(&[ku]);
        assert!(report
            .gaps
            .iter()
            .any(|g| g.gap_type == GapType::UntestedHypothesis));
    }

    #[test]
    fn test_empty() {
        let report = GapDetector::new().analyze(&[]);
        assert!(report.gaps.is_empty());
    }

    #[test]
    fn test_sorted_by_severity() {
        let detector = GapDetector::new();
        let kus = vec![
            make_ku_with_ctx(9000, 1, &[999]),
            make_ku_with_ctx(8000, 2, &[999]),
            make_low_trust_ku(42),
            make_low_trust_ku(42),
        ];
        let report = detector.analyze(&kus);
        for i in 1..report.gaps.len() {
            assert!(report.gaps[i - 1].severity >= report.gaps[i].severity);
        }
    }
}
