//! # Ecosystem — Ecological Niche Fitness Scoring
//!
//! PoK v2 Signal #6: Measures how well a KU fills its ecological niche.
//! Inspired by ecology: diverse ecosystems are healthier.
//!
//! ## Niche Concept:
//! - Each KU occupies a niche defined by its domain (concept codons)
//! - Niche fitness = how much VALUE it adds to its ecosystem
//! - A niche with 100 KUs saying the same thing → low fitness per KU
//! - A KU filling a gap in its niche → high fitness
//!
//! ## Signals:
//! 1. Niche density: how crowded is the KU's domain?
//! 2. Niche uniqueness: how different is it from niche neighbors?
//! 3. Cross-niche bridging: does it connect disparate domains?
//! 4. Metabolic share: what fraction of niche metabolism does it capture?

use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

/// Weight for density component
pub const WEIGHT_DENSITY: f32 = 0.25;

/// Weight for uniqueness component
pub const WEIGHT_UNIQUENESS: f32 = 0.30;

/// Weight for cross-niche bridging
pub const WEIGHT_BRIDGE: f32 = 0.20;

/// Weight for metabolic share
pub const WEIGHT_METABOLIC_SHARE: f32 = 0.25;

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/// A niche identifier (from concept codons / domain codes)
pub type NicheId = u64;

/// Statistics about an ecological niche
#[derive(Debug, Clone, Default)]
pub struct NicheStats {
    /// Number of KUs in this niche
    pub population: usize,
    /// Total metabolic rate across all KUs in niche
    pub total_metabolic_rate: f64,
    /// Average metabolic rate per KU
    pub avg_metabolic_rate: f64,
    /// Diversity: number of unique sources
    pub source_diversity: usize,
}

/// Input data for computing a KU's niche fitness
#[derive(Debug, Clone)]
pub struct KUNicheProfile {
    /// The KU's primary niche(s)
    pub niches: Vec<NicheId>,
    /// The KU's metabolic rate in this niche
    pub metabolic_rate: f64,
    /// The KU's novelty score [0, 1]
    pub novelty: f32,
    /// Number of OTHER niches this KU bridges to
    pub cross_niche_count: usize,
}

// ═══════════════════════════════════════════════════════════════════════════
// Ecosystem Analyzer
// ═══════════════════════════════════════════════════════════════════════════

/// Stateless ecosystem analyzer.
///
/// Computes niche fitness for KUs based on ecological principles.
pub struct EcosystemAnalyzer;

impl EcosystemAnalyzer {
    /// Compute niche fitness for a KU given its niche context.
    ///
    /// Returns [0.0, 1.0]:
    /// - 1.0 = perfectly fills an underserved niche
    /// - 0.0 = redundant in an overcrowded niche
    pub fn niche_fitness(
        profile: &KUNicheProfile,
        niche_stats: &HashMap<NicheId, NicheStats>,
    ) -> f32 {
        if profile.niches.is_empty() {
            return 0.5; // No niche info = neutral
        }

        let density = Self::density_score(profile, niche_stats);
        let uniqueness = profile.novelty; // Reuse entropy's novelty score
        let bridge = Self::bridge_score(profile);
        let metabolic = Self::metabolic_share(profile, niche_stats);

        let fitness = WEIGHT_DENSITY * density
            + WEIGHT_UNIQUENESS * uniqueness
            + WEIGHT_BRIDGE * bridge
            + WEIGHT_METABOLIC_SHARE * metabolic;

        fitness.clamp(0.0, 1.0)
    }

    /// Density score: inverse of niche crowdedness.
    ///
    /// Sparse niche = high score (there's room for this KU).
    /// Dense niche = low score (niche is saturated).
    fn density_score(
        profile: &KUNicheProfile,
        niche_stats: &HashMap<NicheId, NicheStats>,
    ) -> f32 {
        let mut total_population = 0usize;
        let mut niche_count = 0;

        for niche_id in &profile.niches {
            if let Some(stats) = niche_stats.get(niche_id) {
                total_population += stats.population;
                niche_count += 1;
            }
        }

        if niche_count == 0 {
            return 1.0; // New niche = maximum opportunity
        }

        let avg_population = total_population as f32 / niche_count as f32;

        // Inverse sigmoid: 1 / (1 + avg_pop/10)
        // Pop=0 → 1.0, Pop=10 → 0.5, Pop=100 → 0.09
        1.0 / (1.0 + avg_population / 10.0)
    }

    /// Bridge score: how many distinct niches does this KU connect?
    ///
    /// More cross-niche connections = higher bridge value.
    fn bridge_score(profile: &KUNicheProfile) -> f32 {
        let total_niches = profile.niches.len() + profile.cross_niche_count;
        if total_niches <= 1 {
            return 0.0; // Single niche = no bridging
        }

        // Logarithmic scaling: ln(niches) / ln(10)
        // 2 niches → 0.30, 5 niches → 0.70, 10 niches → 1.0
        ((total_niches as f32).ln() / 10.0_f32.ln()).clamp(0.0, 1.0)
    }

    /// Metabolic share: what fraction of niche metabolism does this KU own?
    ///
    /// High metabolic share = this KU is the "go-to" in its niche.
    fn metabolic_share(
        profile: &KUNicheProfile,
        niche_stats: &HashMap<NicheId, NicheStats>,
    ) -> f32 {
        if profile.metabolic_rate <= 0.0 {
            return 0.0;
        }

        let mut total_niche_metabolism = 0.0_f64;
        let mut niche_count = 0;

        for niche_id in &profile.niches {
            if let Some(stats) = niche_stats.get(niche_id) {
                total_niche_metabolism += stats.total_metabolic_rate;
                niche_count += 1;
            }
        }

        if niche_count == 0 || total_niche_metabolism <= 0.0 {
            return 1.0; // Only KU in niche = 100% share
        }

        let avg_niche_metabolism = total_niche_metabolism / niche_count as f64;
        let share = profile.metabolic_rate / avg_niche_metabolism;

        // Normalize: sigmoid that peaks at share=1.0
        // share=0 → 0.0, share=0.5 → 0.5, share=1.0 → 1.0
        (share as f32).clamp(0.0, 1.0)
    }

    /// Compute niche stats from a set of KU profiles.
    pub fn compute_niche_stats(
        profiles: &[(NicheId, f64)], // (niche_id, metabolic_rate) pairs
    ) -> HashMap<NicheId, NicheStats> {
        let mut stats: HashMap<NicheId, NicheStats> = HashMap::new();

        for &(niche_id, rate) in profiles {
            let entry = stats.entry(niche_id).or_default();
            entry.population += 1;
            entry.total_metabolic_rate += rate;
        }

        // Compute averages
        for stat in stats.values_mut() {
            if stat.population > 0 {
                stat.avg_metabolic_rate = stat.total_metabolic_rate / stat.population as f64;
            }
        }

        stats
    }

    /// Convert fitness to u16 [0, 10000] for TrustSection.
    pub fn fitness_to_u16(fitness: f32) -> u16 {
        (fitness * 10000.0).clamp(0.0, 10000.0) as u16
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_niche_neutral() {
        let profile = KUNicheProfile {
            niches: vec![],
            metabolic_rate: 1.0,
            novelty: 0.5,
            cross_niche_count: 0,
        };
        let stats = HashMap::new();
        let fitness = EcosystemAnalyzer::niche_fitness(&profile, &stats);
        assert!((fitness - 0.5).abs() < 0.01, "No niche = neutral: {}", fitness);
    }

    #[test]
    fn test_new_niche_high_fitness() {
        let profile = KUNicheProfile {
            niches: vec![42],
            metabolic_rate: 1.0,
            novelty: 1.0, // Very novel
            cross_niche_count: 2,
        };
        let stats = HashMap::new(); // No existing KUs in niche
        let fitness = EcosystemAnalyzer::niche_fitness(&profile, &stats);
        assert!(fitness > 0.7, "New niche + novel = high fitness: {}", fitness);
    }

    #[test]
    fn test_crowded_niche_low_fitness() {
        let profile = KUNicheProfile {
            niches: vec![42],
            metabolic_rate: 0.01,
            novelty: 0.1, // Not novel
            cross_niche_count: 0,
        };
        let mut stats = HashMap::new();
        stats.insert(42, NicheStats {
            population: 1000,
            total_metabolic_rate: 500.0,
            avg_metabolic_rate: 0.5,
            source_diversity: 100,
        });

        let fitness = EcosystemAnalyzer::niche_fitness(&profile, &stats);
        assert!(fitness < 0.3, "Crowded + not novel = low fitness: {}", fitness);
    }

    #[test]
    fn test_bridge_ku_higher_fitness() {
        let bridge_profile = KUNicheProfile {
            niches: vec![1, 2, 3],
            metabolic_rate: 1.0,
            novelty: 0.5,
            cross_niche_count: 5,
        };
        let single_profile = KUNicheProfile {
            niches: vec![1],
            metabolic_rate: 1.0,
            novelty: 0.5,
            cross_niche_count: 0,
        };
        let stats = HashMap::new();

        let bridge_fit = EcosystemAnalyzer::niche_fitness(&bridge_profile, &stats);
        let single_fit = EcosystemAnalyzer::niche_fitness(&single_profile, &stats);

        assert!(bridge_fit > single_fit,
            "Bridge KU ({}) > single niche ({})", bridge_fit, single_fit);
    }

    #[test]
    fn test_metabolic_leader_high_share() {
        let profile = KUNicheProfile {
            niches: vec![42],
            metabolic_rate: 10.0, // Dominates
            novelty: 0.5,
            cross_niche_count: 0,
        };
        let mut stats = HashMap::new();
        stats.insert(42, NicheStats {
            population: 10,
            total_metabolic_rate: 10.0,
            avg_metabolic_rate: 1.0,
            source_diversity: 5,
        });

        let fitness = EcosystemAnalyzer::niche_fitness(&profile, &stats);
        assert!(fitness > 0.4, "Metabolic leader has good fitness: {}", fitness);
    }

    #[test]
    fn test_compute_niche_stats() {
        let profiles = vec![
            (1, 1.0),
            (1, 2.0),
            (1, 3.0),
            (2, 5.0),
        ];
        let stats = EcosystemAnalyzer::compute_niche_stats(&profiles);

        assert_eq!(stats[&1].population, 3);
        assert!((stats[&1].total_metabolic_rate - 6.0).abs() < 0.001);
        assert!((stats[&1].avg_metabolic_rate - 2.0).abs() < 0.001);
        assert_eq!(stats[&2].population, 1);
    }

    #[test]
    fn test_fitness_to_u16() {
        assert_eq!(EcosystemAnalyzer::fitness_to_u16(0.0), 0);
        assert_eq!(EcosystemAnalyzer::fitness_to_u16(1.0), 10000);
        assert_eq!(EcosystemAnalyzer::fitness_to_u16(0.5), 5000);
    }

    #[test]
    fn test_density_score_empty_niche() {
        let profile = KUNicheProfile {
            niches: vec![99],
            metabolic_rate: 1.0,
            novelty: 0.5,
            cross_niche_count: 0,
        };
        let stats = HashMap::new();
        let density = EcosystemAnalyzer::density_score(&profile, &stats);
        assert_eq!(density, 1.0, "Empty niche = max density score");
    }
}
