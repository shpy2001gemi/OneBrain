//! # Spread Analysis — Content-Agnostic Organic Spread Detection
//!
//! PoK v2: Analyzes HOW knowledge spreads, not WHAT it says.
//! Organic spread has distinctive patterns that differ from
//! bot-driven or astroturfed spread.
//!
//! ## Key Insight (Founder Q5):
//! Coordinated disinformation campaigns have specific spread fingerprints:
//! - Temporal: synchronized bursts vs organic ripples
//! - Geographic: cluster vs distributed
//! - Social: echo chamber vs cross-community
//!
//! ## This module provides POSITIVE signal: how organic is the spread?
//! (Immune system provides NEGATIVE signal: how bot-like is the spread?)

use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

/// Weight for temporal organicity
pub const WEIGHT_TEMPORAL: f32 = 0.30;

/// Weight for source diversity
pub const WEIGHT_DIVERSITY: f32 = 0.30;

/// Weight for geographic spread
pub const WEIGHT_GEOGRAPHIC: f32 = 0.20;

/// Weight for engagement authenticity
pub const WEIGHT_ENGAGEMENT: f32 = 0.20;

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/// Spread metrics for a KU (input to analysis)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpreadMetrics {
    /// Timestamps of each replication event
    pub replication_timestamps: Vec<u64>,
    /// Number of unique source nodes
    pub unique_sources: u32,
    /// Total replications
    pub total_replications: u32,
    /// Number of distinct communities/clusters reached
    pub communities_reached: u32,
    /// Average dwell time per retrieval (seconds)
    pub avg_dwell_seconds: f32,
    /// Ratio of retrievals that led to further action (cite, derive)
    pub action_ratio: f32,
    /// How many hops from origin (average)
    pub avg_hop_distance: f32,
}

/// Result of spread analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpreadAnalysis {
    /// Overall organicity score [0.0, 1.0]
    pub organicity: f32,
    /// Temporal pattern score
    pub temporal_score: f32,
    /// Source diversity score
    pub diversity_score: f32,
    /// Geographic distribution score
    pub geographic_score: f32,
    /// Engagement authenticity score
    pub engagement_score: f32,
}

// ═══════════════════════════════════════════════════════════════════════════
// Spread Analyzer
// ═══════════════════════════════════════════════════════════════════════════

/// Stateless spread pattern analyzer.
pub struct SpreadAnalyzer;

impl SpreadAnalyzer {
    /// Analyze the spread pattern of a KU.
    ///
    /// Returns a SpreadAnalysis with organicity [0.0, 1.0]:
    /// - 1.0 = perfectly organic spread pattern
    /// - 0.0 = clearly artificial/bot-driven
    pub fn analyze(metrics: &SpreadMetrics) -> SpreadAnalysis {
        let temporal = Self::temporal_organicity(metrics);
        let diversity = Self::source_diversity(metrics);
        let geographic = Self::geographic_distribution(metrics);
        let engagement = Self::engagement_authenticity(metrics);

        let organicity = WEIGHT_TEMPORAL * temporal
            + WEIGHT_DIVERSITY * diversity
            + WEIGHT_GEOGRAPHIC * geographic
            + WEIGHT_ENGAGEMENT * engagement;

        SpreadAnalysis {
            organicity: organicity.clamp(0.0, 1.0),
            temporal_score: temporal,
            diversity_score: diversity,
            geographic_score: geographic,
            engagement_score: engagement,
        }
    }

    /// Temporal organicity: organic spread has natural variance.
    ///
    /// Bots replicate at regular intervals (low variance).
    /// Organic spread has variable intervals (high variance).
    ///
    /// Uses coefficient of variation (CV = std/mean) of inter-event times.
    fn temporal_organicity(metrics: &SpreadMetrics) -> f32 {
        let timestamps = &metrics.replication_timestamps;
        if timestamps.len() < 3 {
            return 0.5; // Not enough data
        }

        // Compute inter-event intervals
        let mut sorted = timestamps.clone();
        sorted.sort();
        let intervals: Vec<f64> = sorted.windows(2).map(|w| (w[1] - w[0]) as f64).collect();

        if intervals.is_empty() {
            return 0.5;
        }

        let mean = intervals.iter().sum::<f64>() / intervals.len() as f64;
        if mean <= 0.0 {
            return 0.0; // All same timestamp = bot-like
        }

        let variance =
            intervals.iter().map(|&x| (x - mean).powi(2)).sum::<f64>() / intervals.len() as f64;
        let cv = variance.sqrt() / mean;

        // CV < 0.3 = very regular (bot-like) → low score
        // CV 0.5-1.5 = natural variance → high score
        // CV > 2.0 = erratic (also suspicious) → medium score
        if cv < 0.3 {
            cv as f32 / 0.3 * 0.5 // [0, 0.5]
        } else if cv <= 1.5 {
            0.5 + ((cv - 0.3) / 1.2 * 0.5) as f32 // [0.5, 1.0]
        } else {
            (1.0 - ((cv - 1.5) / 2.0).min(0.5)) as f32 // decay for erratic
        }
    }

    /// Source diversity: organic spread reaches many independent sources.
    fn source_diversity(metrics: &SpreadMetrics) -> f32 {
        if metrics.total_replications == 0 {
            return 0.5;
        }

        let ratio = metrics.unique_sources as f32 / metrics.total_replications as f32;
        // Ideal: 30-70% unique sources
        // Too low: echo chamber, Too high: unlikely organic
        if ratio < 0.1 {
            ratio / 0.1 * 0.3 // Low diversity penalty
        } else if ratio <= 0.7 {
            0.3 + (ratio - 0.1) / 0.6 * 0.7 // Linear in sweet spot
        } else {
            1.0 // Very high diversity = great
        }
    }

    /// Geographic distribution: organic spread crosses community boundaries.
    fn geographic_distribution(metrics: &SpreadMetrics) -> f32 {
        if metrics.total_replications < 5 {
            return 0.5; // Not enough data
        }

        // More communities = more organic
        let community_ratio =
            metrics.communities_reached as f32 / metrics.total_replications as f32;

        // Also factor in hop distance (organic spreads further)
        let hop_factor = (metrics.avg_hop_distance / 5.0).min(1.0);

        (community_ratio * 0.6 + hop_factor * 0.4).clamp(0.0, 1.0)
    }

    /// Engagement authenticity: real users spend time and take action.
    fn engagement_authenticity(metrics: &SpreadMetrics) -> f32 {
        // Dwell time: real reading takes > 5 seconds
        let dwell_score = if metrics.avg_dwell_seconds < 1.0 {
            0.0 // Sub-second = bot
        } else if metrics.avg_dwell_seconds < 5.0 {
            metrics.avg_dwell_seconds / 5.0 * 0.5
        } else if metrics.avg_dwell_seconds < 60.0 {
            0.5 + (metrics.avg_dwell_seconds - 5.0) / 55.0 * 0.5
        } else {
            1.0 // 60+ seconds = real engagement
        };

        // Action ratio: real users sometimes cite/derive
        let action_score = if metrics.action_ratio < 0.01 {
            0.0 // Nobody took action = possibly not valuable
        } else {
            (metrics.action_ratio * 5.0).min(1.0)
        };

        dwell_score * 0.6 + action_score * 0.4
    }

    /// Compute the organicity-adjusted PoMV multiplier.
    ///
    /// Organic spread → multiplier close to 1.0
    /// Artificial spread → multiplier dampened (e.g., 0.3)
    pub fn organicity_multiplier(organicity: f32) -> f32 {
        // Smooth step: 0.3 + 0.7 × organicity²
        // Even fully artificial gets 0.3 (don't zero out completely)
        0.3 + 0.7 * organicity * organicity
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn organic_metrics() -> SpreadMetrics {
        SpreadMetrics {
            replication_timestamps: vec![100, 200, 350, 500, 800, 1200, 1800, 2500, 3500, 5000],
            unique_sources: 8,
            total_replications: 10,
            communities_reached: 4,
            avg_dwell_seconds: 30.0,
            action_ratio: 0.3,
            avg_hop_distance: 3.0,
        }
    }

    fn bot_metrics() -> SpreadMetrics {
        SpreadMetrics {
            // Regular intervals = bot-like
            replication_timestamps: (0..100).map(|i| i * 10).collect(),
            unique_sources: 3,
            total_replications: 100,
            communities_reached: 1,
            avg_dwell_seconds: 0.1,
            action_ratio: 0.0,
            avg_hop_distance: 1.0,
        }
    }

    #[test]
    fn test_organic_high_score() {
        let analysis = SpreadAnalyzer::analyze(&organic_metrics());
        assert!(
            analysis.organicity > 0.5,
            "Organic spread should score > 0.5: {}",
            analysis.organicity
        );
    }

    #[test]
    fn test_bot_low_score() {
        let analysis = SpreadAnalyzer::analyze(&bot_metrics());
        assert!(
            analysis.organicity < 0.4,
            "Bot spread should score < 0.4: {}",
            analysis.organicity
        );
    }

    #[test]
    fn test_organic_beats_bot() {
        let organic = SpreadAnalyzer::analyze(&organic_metrics());
        let bot = SpreadAnalyzer::analyze(&bot_metrics());
        assert!(
            organic.organicity > bot.organicity,
            "Organic ({}) > Bot ({})",
            organic.organicity,
            bot.organicity
        );
    }

    #[test]
    fn test_temporal_regular_intervals_bot() {
        let metrics = SpreadMetrics {
            replication_timestamps: (0..20).map(|i| i * 100).collect(), // Perfect intervals
            ..Default::default()
        };
        let score = SpreadAnalyzer::temporal_organicity(&metrics);
        assert!(score < 0.5, "Regular intervals = bot-like: {}", score);
    }

    #[test]
    fn test_temporal_varied_intervals_organic() {
        let metrics = SpreadMetrics {
            replication_timestamps: vec![0, 50, 200, 250, 800, 850, 2000, 5000, 5100, 10000],
            ..Default::default()
        };
        let score = SpreadAnalyzer::temporal_organicity(&metrics);
        assert!(score > 0.4, "Varied intervals = organic: {}", score);
    }

    #[test]
    fn test_source_diversity_high() {
        let metrics = SpreadMetrics {
            unique_sources: 80,
            total_replications: 100,
            ..Default::default()
        };
        let score = SpreadAnalyzer::source_diversity(&metrics);
        assert!(score > 0.8, "80% unique = high diversity: {}", score);
    }

    #[test]
    fn test_source_diversity_low() {
        let metrics = SpreadMetrics {
            unique_sources: 2,
            total_replications: 100,
            ..Default::default()
        };
        let score = SpreadAnalyzer::source_diversity(&metrics);
        assert!(score < 0.3, "2% unique = low diversity: {}", score);
    }

    #[test]
    fn test_engagement_bot_dwell() {
        let metrics = SpreadMetrics {
            avg_dwell_seconds: 0.1,
            action_ratio: 0.0,
            ..Default::default()
        };
        let score = SpreadAnalyzer::engagement_authenticity(&metrics);
        assert_eq!(score, 0.0, "Sub-second dwell = bot");
    }

    #[test]
    fn test_engagement_real_user() {
        let metrics = SpreadMetrics {
            avg_dwell_seconds: 45.0,
            action_ratio: 0.2,
            ..Default::default()
        };
        let score = SpreadAnalyzer::engagement_authenticity(&metrics);
        assert!(score > 0.6, "45s dwell + 20% action = real: {}", score);
    }

    #[test]
    fn test_organicity_multiplier() {
        assert!((SpreadAnalyzer::organicity_multiplier(1.0) - 1.0).abs() < 0.01);
        assert!((SpreadAnalyzer::organicity_multiplier(0.0) - 0.3).abs() < 0.01);
        assert!(SpreadAnalyzer::organicity_multiplier(0.5) > 0.3);
        assert!(SpreadAnalyzer::organicity_multiplier(0.5) < 1.0);
    }

    #[test]
    fn test_empty_metrics_neutral() {
        let metrics = SpreadMetrics::default();
        let analysis = SpreadAnalyzer::analyze(&metrics);
        assert!(
            (analysis.organicity - 0.5).abs() < 0.2,
            "Empty = roughly neutral: {}",
            analysis.organicity
        );
    }
}
