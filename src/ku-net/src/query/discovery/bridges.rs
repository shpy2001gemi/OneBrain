//! # Cross-Domain Bridge Finder — Phase E3
//!
//! Implements Swanson's ABC model for Literature-Based Discovery.
//! Finds shared concepts connecting "strong" (well-known) domains
//! to "weak" (unknown) domains, enabling undiscovered public knowledge.

use std::collections::{HashMap, HashSet};

use ku_core::KuRuntime;

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/// A discovered bridge between two knowledge domains.
#[derive(Debug, Clone)]
pub struct KnowledgeBridge {
    pub source_domain: u64,
    pub target_domain: u64,
    pub bridge_concepts: Vec<u64>,
    pub strength: f64,
    pub suggested_query: String,
    pub description: String,
}

/// Report from bridge analysis.
#[derive(Debug, Clone)]
pub struct BridgeReport {
    pub bridges: Vec<KnowledgeBridge>,
    pub domains_analyzed: usize,
    pub bridge_concepts_found: usize,
}

// ═══════════════════════════════════════════════════════════════════════════
// Bridge Finder
// ═══════════════════════════════════════════════════════════════════════════

pub struct BridgeFinder {
    /// Minimum KUs in a domain to consider it "known".
    pub min_domain_size: usize,
    /// Maximum bridges to report.
    pub max_bridges: usize,
    /// Minimum bridge strength to report.
    pub min_strength: f64,
}

impl BridgeFinder {
    pub fn new() -> Self {
        Self { min_domain_size: 2, max_bridges: 20, min_strength: 0.1 }
    }

    pub fn analyze(&self, kus: &[KuRuntime]) -> BridgeReport {
        let concept_domains = self.build_concept_domain_map(kus);
        let domain_strength = self.build_domain_strength(kus);
        let bridge_concepts = self.find_bridge_concepts(&concept_domains);
        let bridges = self.generate_bridges(&bridge_concepts, &domain_strength);

        BridgeReport {
            bridges,
            domains_analyzed: domain_strength.len(),
            bridge_concepts_found: bridge_concepts.len(),
        }
    }

    /// Map each concept to the "domains" it appears in.
    /// A concept's "domain" = primary concept of its containing KU.
    fn build_concept_domain_map(&self, kus: &[KuRuntime]) -> HashMap<u64, HashSet<u64>> {
        let mut map: HashMap<u64, HashSet<u64>> = HashMap::new();
        for ku in kus {
            let domain = match ku.primary_concept() {
                Some(id) => id,
                None => continue,
            };
            for concept_id in ku.concept_ids() {
                map.entry(concept_id).or_default().insert(domain);
            }
            // Bond context concepts also belong to this domain
            for bond in &ku.epi.bonds {
                for &ctx_id in &bond.context {
                    map.entry(ctx_id).or_default().insert(domain);
                }
            }
        }
        map
    }

    fn build_domain_strength(&self, kus: &[KuRuntime]) -> HashMap<u64, usize> {
        let mut strength: HashMap<u64, usize> = HashMap::new();
        for ku in kus {
            if let Some(primary) = ku.primary_concept() {
                *strength.entry(primary).or_insert(0) += 1;
            }
        }
        strength
    }

    fn find_bridge_concepts(&self, map: &HashMap<u64, HashSet<u64>>) -> Vec<(u64, HashSet<u64>)> {
        map.iter()
            .filter(|(_, domains)| domains.len() >= 2)
            .map(|(&concept, domains)| (concept, domains.clone()))
            .collect()
    }

    fn generate_bridges(
        &self,
        bridge_concepts: &[(u64, HashSet<u64>)],
        domain_strength: &HashMap<u64, usize>,
    ) -> Vec<KnowledgeBridge> {
        let mut pair_bridges: HashMap<(u64, u64), Vec<u64>> = HashMap::new();

        for (concept, domains) in bridge_concepts {
            let list: Vec<u64> = domains.iter().copied().collect();
            for i in 0..list.len() {
                for j in (i+1)..list.len() {
                    let pair = if list[i] < list[j] { (list[i], list[j]) } else { (list[j], list[i]) };
                    pair_bridges.entry(pair).or_default().push(*concept);
                }
            }
        }

        let mut bridges: Vec<KnowledgeBridge> = pair_bridges.into_iter()
            .filter_map(|((d1, d2), concepts)| {
                let s1 = *domain_strength.get(&d1).unwrap_or(&0);
                let s2 = *domain_strength.get(&d2).unwrap_or(&0);
                if s1 < self.min_domain_size && s2 < self.min_domain_size {
                    return None;
                }

                let asymmetry = if s1 != s2 {
                    let (strong, weak) = if s1 > s2 { (s1, s2) } else { (s2, s1) };
                    (strong as f64 / weak.max(1) as f64).min(5.0) / 5.0
                } else { 0.5 };

                let strength = (concepts.len() as f64 / 10.0).min(1.0) * asymmetry;
                if strength < self.min_strength { return None; }

                let (source, target) = if s1 >= s2 { (d1, d2) } else { (d2, d1) };
                Some(KnowledgeBridge {
                    source_domain: source,
                    target_domain: target,
                    bridge_concepts: concepts,
                    strength,
                    suggested_query: format!(
                        "FIND (k:KU) WHERE k.codons CONTAINS concept_id = {} SCOPE DHT", target
                    ),
                    description: format!(
                        "Domain {} connects to domain {} via bridge concepts", source, target
                    ),
                })
            })
            .collect();

        bridges.sort_by(|a, b| b.strength.partial_cmp(&a.strength).unwrap_or(std::cmp::Ordering::Equal));
        bridges.truncate(self.max_bridges);
        bridges
    }
}

impl Default for BridgeFinder { fn default() -> Self { Self::new() } }

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use ku_core::{KuRuntime, Epigenetics, RelationType};
    use ku_core::core_dna::{CoreDna, CoreDnaHeader, Instruction};

    fn make_ku(concept_id: u64, ctx_concepts: &[u64]) -> KuRuntime {
        let dna = CoreDna {
            header: CoreDnaHeader { version: 2, gene_type: 0, has_concept_table: false },
            concept_table: Vec::new(),
            instructions: vec![
                Instruction::Triple { s: concept_id, p: 133, o: 132 },
                Instruction::Certainty { level: 9500 },
            ],
        };
        let mut ku = KuRuntime::from_dna(dna).unwrap();
        ku.epi = Epigenetics::with_trust(8000, 8000);
        for &ctx in ctx_concepts {
            ku.epi.add_bond(vec![0x42; 32], RelationType::Extends, 8000);
            if let Some(bond) = ku.epi.bonds.last_mut() {
                bond.context = vec![ctx];
            }
        }
        ku
    }

    #[test]
    fn test_bridge_detection() {
        let mut finder = BridgeFinder::new();
        finder.min_strength = 0.01; // Lower threshold for test
        // Domain A (concept 100) and Domain C (concept 200)
        // share bridge concepts 50, 51 in bond contexts
        let kus = vec![
            make_ku(100, &[50, 51]),  // A → bridges B1, B2
            make_ku(100, &[50]),      // A → B1 (strong domain)
            make_ku(200, &[50, 51]),  // C → bridges B1, B2
        ];
        let report = finder.analyze(&kus);
        assert!(!report.bridges.is_empty(), "Should find bridge between domains 100 and 200");
    }

    #[test]
    fn test_empty() {
        let report = BridgeFinder::new().analyze(&[]);
        assert!(report.bridges.is_empty());
    }

    #[test]
    fn test_sorted() {
        let finder = BridgeFinder::new();
        let kus = vec![
            make_ku(100, &[50, 51, 52]),
            make_ku(100, &[50, 51]),
            make_ku(200, &[50]),
            make_ku(300, &[51, 52]),
        ];
        let report = finder.analyze(&kus);
        for i in 1..report.bridges.len() {
            assert!(report.bridges[i-1].strength >= report.bridges[i].strength);
        }
    }
}
