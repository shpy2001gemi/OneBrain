//! # Immune System — Anti-Spam Antibody Detection
//!
//! PoK v2 Signal #4 (Anti-fragile): Detects and mitigates coordinated
//! manipulation without censoring content. Content-agnostic — only looks
//! at SPREAD PATTERNS, not what the KU says.
//!
//! ## Design (Founder decisions):
//! - Privacy first: antibodies gossip pattern_hash only, NEVER NodeID/PII
//! - No content judgment: all signals are behavioral/statistical
//! - Decentralized: each node runs its own immune system locally
//! - Anti-fragile: surviving attacks makes KU STRONGER
//!
//! ## Detection Signals:
//! 1. Temporal burst: too many copies too fast (bot-like)
//! 2. Source concentration: all spread from same source cluster
//! 3. Engagement ratio: high replication but low actual usage
//! 4. Diversity deficit: spread only among similar nodes
//!
//! ## Key Constraint (Founder Q5):
//! "Có những kẻ xấu vì lý do tôn giáo, chính trị ... sẽ nạp kiến thức
//! sai lệch khổng lồ và tự cho phe họ (bot) vote."
//! → We detect PATTERN, not CONTENT.

use serde::{Serialize, Deserialize};

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

/// Temporal burst: max replications per hour before flagging
pub const BURST_THRESHOLD_PER_HOUR: u32 = 50;

/// Source concentration: if >80% of spread from same source cluster → suspicious
pub const SOURCE_CONCENTRATION_THRESHOLD: f32 = 0.80;

/// Engagement ratio: if usage/replication < this → low engagement
pub const ENGAGEMENT_RATIO_THRESHOLD: f32 = 0.05;

/// Diversity deficit: if unique_sources/total_replications < this → low diversity
pub const DIVERSITY_THRESHOLD: f32 = 0.1;

/// Number of detections to confirm an antibody pattern
pub const CONFIRMATION_THRESHOLD: u32 = 3;

/// Survival bonus per confirmed attack survived
pub const SURVIVAL_BONUS: f32 = 0.1;

/// Maximum survival score
pub const MAX_SURVIVAL_SCORE: f32 = 1.0;

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/// Type of suspicious pattern detected
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AntibodyType {
    /// Too many replications too fast
    TemporalBurst,
    /// Most replications from same source cluster
    SourceConcentration,
    /// High replication but near-zero actual usage
    LowEngagement,
    /// Spread only among similar/clustered nodes
    DiversityDeficit,
}

/// An antibody detection record (privacy-safe: no NodeIDs)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Antibody {
    /// BLAKE3 hash of the pattern (NOT the content)
    pub pattern_hash: [u8; 32],
    /// What type of manipulation detected
    pub antibody_type: AntibodyType,
    /// Confidence in detection [0.0, 1.0]
    pub confidence: f32,
    /// When first detected
    pub detected_at: u64,
    /// How many independent nodes confirmed this pattern
    pub confirmation_count: u32,
}

/// Spread observation data for a KU (input to immune analysis)
#[derive(Debug, Clone)]
pub struct SpreadObservation {
    /// Total number of replications across nodes
    pub total_replications: u32,
    /// Number of unique source nodes
    pub unique_sources: u32,
    /// Replications in the last hour
    pub replications_last_hour: u32,
    /// Maximum fraction from any single source cluster
    pub max_source_fraction: f32,
    /// Total actual usage events (query/retrieval/citation)
    pub total_usage_events: u64,
    /// Time since KU creation (seconds)
    pub age_secs: u64,
}

// ═══════════════════════════════════════════════════════════════════════════
// Immune Engine
// ═══════════════════════════════════════════════════════════════════════════

/// Stateless immune analyzer.
///
/// All methods are pure functions — each node runs independently.
/// No central authority decides what's "spam."
pub struct ImmuneEngine;

impl ImmuneEngine {
    /// Analyze a KU's spread pattern for manipulation signals.
    ///
    /// Returns detected antibodies (empty = healthy spread).
    pub fn analyze(
        obs: &SpreadObservation,
        pattern_hash: [u8; 32],
        now: u64,
    ) -> Vec<Antibody> {
        let mut antibodies = Vec::new();

        // Signal 1: Temporal burst
        if let Some(ab) = Self::check_temporal_burst(obs, pattern_hash, now) {
            antibodies.push(ab);
        }

        // Signal 2: Source concentration
        if let Some(ab) = Self::check_source_concentration(obs, pattern_hash, now) {
            antibodies.push(ab);
        }

        // Signal 3: Low engagement ratio
        if let Some(ab) = Self::check_low_engagement(obs, pattern_hash, now) {
            antibodies.push(ab);
        }

        // Signal 4: Diversity deficit
        if let Some(ab) = Self::check_diversity_deficit(obs, pattern_hash, now) {
            antibodies.push(ab);
        }

        antibodies
    }

    /// Check for temporal burst (bot-like rapid replication).
    fn check_temporal_burst(
        obs: &SpreadObservation,
        pattern_hash: [u8; 32],
        now: u64,
    ) -> Option<Antibody> {
        if obs.replications_last_hour > BURST_THRESHOLD_PER_HOUR {
            let ratio = obs.replications_last_hour as f32 / BURST_THRESHOLD_PER_HOUR as f32;
            let confidence = (1.0 - 1.0 / ratio).clamp(0.0, 1.0);
            Some(Antibody {
                pattern_hash,
                antibody_type: AntibodyType::TemporalBurst,
                confidence,
                detected_at: now,
                confirmation_count: 1,
            })
        } else {
            None
        }
    }

    /// Check for source concentration (astroturfing).
    fn check_source_concentration(
        obs: &SpreadObservation,
        pattern_hash: [u8; 32],
        now: u64,
    ) -> Option<Antibody> {
        if obs.total_replications > 5 && obs.max_source_fraction > SOURCE_CONCENTRATION_THRESHOLD {
            let excess = obs.max_source_fraction - SOURCE_CONCENTRATION_THRESHOLD;
            let confidence = (excess / (1.0 - SOURCE_CONCENTRATION_THRESHOLD)).clamp(0.0, 1.0);
            Some(Antibody {
                pattern_hash,
                antibody_type: AntibodyType::SourceConcentration,
                confidence,
                detected_at: now,
                confirmation_count: 1,
            })
        } else {
            None
        }
    }

    /// Check for low engagement (high replication, low actual usage).
    fn check_low_engagement(
        obs: &SpreadObservation,
        pattern_hash: [u8; 32],
        now: u64,
    ) -> Option<Antibody> {
        if obs.total_replications < 10 {
            return None; // Too few replications to judge
        }

        let engagement_ratio = obs.total_usage_events as f32 / obs.total_replications as f32;
        if engagement_ratio < ENGAGEMENT_RATIO_THRESHOLD {
            let confidence = (1.0 - engagement_ratio / ENGAGEMENT_RATIO_THRESHOLD).clamp(0.0, 1.0);
            Some(Antibody {
                pattern_hash,
                antibody_type: AntibodyType::LowEngagement,
                confidence,
                detected_at: now,
                confirmation_count: 1,
            })
        } else {
            None
        }
    }

    /// Check for diversity deficit (echo chamber spread).
    fn check_diversity_deficit(
        obs: &SpreadObservation,
        pattern_hash: [u8; 32],
        now: u64,
    ) -> Option<Antibody> {
        if obs.total_replications < 5 {
            return None; // Too few to measure diversity
        }

        let diversity = obs.unique_sources as f32 / obs.total_replications as f32;
        if diversity < DIVERSITY_THRESHOLD {
            let confidence = (1.0 - diversity / DIVERSITY_THRESHOLD).clamp(0.0, 1.0);
            Some(Antibody {
                pattern_hash,
                antibody_type: AntibodyType::DiversityDeficit,
                confidence,
                detected_at: now,
                confirmation_count: 1,
            })
        } else {
            None
        }
    }

    /// Compute survival score for a KU that has been attacked.
    ///
    /// Anti-fragile: surviving attacks makes it STRONGER.
    /// - Each confirmed antibody that was later resolved (false alarm)
    ///   adds a survival bonus
    /// - KU must still be metabolically alive after the attack
    pub fn survival_score(
        attacks_survived: u32,
        is_still_alive: bool,
    ) -> f32 {
        if !is_still_alive {
            return 0.0; // Dead KU gets no survival bonus
        }
        (attacks_survived as f32 * SURVIVAL_BONUS).min(MAX_SURVIVAL_SCORE)
    }

    /// Convert survival score to u16 [0, 10000] for TrustSection.
    pub fn survival_to_u16(score: f32) -> u16 {
        (score * 10000.0).clamp(0.0, 10000.0) as u16
    }

    /// Should this KU be quarantined (damped propagation)?
    ///
    /// Only quarantine if multiple independent signals confirm manipulation.
    /// Single signals can be false positives.
    pub fn should_quarantine(antibodies: &[Antibody]) -> bool {
        // Need at least 2 different antibody types
        let unique_types: std::collections::HashSet<_> = antibodies.iter()
            .map(|a| a.antibody_type)
            .collect();

        // AND average confidence > 0.7
        let avg_confidence = if antibodies.is_empty() {
            0.0
        } else {
            antibodies.iter().map(|a| a.confidence).sum::<f32>() / antibodies.len() as f32
        };

        unique_types.len() >= 2 && avg_confidence > 0.7
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_000_000;

    fn test_hash(id: u8) -> [u8; 32] {
        let mut h = [0u8; 32];
        h[0] = id;
        h
    }

    fn healthy_spread() -> SpreadObservation {
        SpreadObservation {
            total_replications: 100,
            unique_sources: 50,
            replications_last_hour: 5,
            max_source_fraction: 0.1,
            total_usage_events: 200,
            age_secs: 86400,
        }
    }

    fn bot_spread() -> SpreadObservation {
        SpreadObservation {
            total_replications: 500,
            unique_sources: 3,
            replications_last_hour: 200,
            max_source_fraction: 0.95,
            total_usage_events: 2,
            age_secs: 3600,
        }
    }

    #[test]
    fn test_healthy_spread_no_antibodies() {
        let abs = ImmuneEngine::analyze(&healthy_spread(), test_hash(1), T0);
        assert!(abs.is_empty(), "Healthy spread = no antibodies: {:?}", abs);
    }

    #[test]
    fn test_temporal_burst_detected() {
        let obs = SpreadObservation {
            replications_last_hour: 200,
            ..healthy_spread()
        };
        let abs = ImmuneEngine::analyze(&obs, test_hash(1), T0);
        assert!(abs.iter().any(|a| a.antibody_type == AntibodyType::TemporalBurst));
    }

    #[test]
    fn test_source_concentration_detected() {
        let obs = SpreadObservation {
            max_source_fraction: 0.95,
            total_replications: 100,
            ..healthy_spread()
        };
        let abs = ImmuneEngine::analyze(&obs, test_hash(1), T0);
        assert!(abs.iter().any(|a| a.antibody_type == AntibodyType::SourceConcentration));
    }

    #[test]
    fn test_low_engagement_detected() {
        let obs = SpreadObservation {
            total_replications: 500,
            total_usage_events: 1,
            ..healthy_spread()
        };
        let abs = ImmuneEngine::analyze(&obs, test_hash(1), T0);
        assert!(abs.iter().any(|a| a.antibody_type == AntibodyType::LowEngagement));
    }

    #[test]
    fn test_diversity_deficit_detected() {
        let obs = SpreadObservation {
            total_replications: 100,
            unique_sources: 3,
            ..healthy_spread()
        };
        let abs = ImmuneEngine::analyze(&obs, test_hash(1), T0);
        assert!(abs.iter().any(|a| a.antibody_type == AntibodyType::DiversityDeficit));
    }

    #[test]
    fn test_bot_spread_multiple_signals() {
        let abs = ImmuneEngine::analyze(&bot_spread(), test_hash(1), T0);
        assert!(abs.len() >= 3, "Bot spread triggers multiple signals: {}", abs.len());
    }

    #[test]
    fn test_quarantine_requires_multiple_types() {
        let abs = vec![
            Antibody {
                pattern_hash: test_hash(1),
                antibody_type: AntibodyType::TemporalBurst,
                confidence: 0.9,
                detected_at: T0,
                confirmation_count: 1,
            },
        ];
        assert!(!ImmuneEngine::should_quarantine(&abs), "Single type = no quarantine");

        let abs2 = vec![
            Antibody {
                pattern_hash: test_hash(1),
                antibody_type: AntibodyType::TemporalBurst,
                confidence: 0.9,
                detected_at: T0,
                confirmation_count: 1,
            },
            Antibody {
                pattern_hash: test_hash(1),
                antibody_type: AntibodyType::SourceConcentration,
                confidence: 0.8,
                detected_at: T0,
                confirmation_count: 1,
            },
        ];
        assert!(ImmuneEngine::should_quarantine(&abs2), "Two types + high confidence = quarantine");
    }

    #[test]
    fn test_survival_score_anti_fragile() {
        assert_eq!(ImmuneEngine::survival_score(0, true), 0.0);
        assert!((ImmuneEngine::survival_score(1, true) - 0.1).abs() < 0.001);
        assert!((ImmuneEngine::survival_score(5, true) - 0.5).abs() < 0.001);
        assert!(ImmuneEngine::survival_score(20, true) <= MAX_SURVIVAL_SCORE);
    }

    #[test]
    fn test_dead_ku_no_survival_bonus() {
        assert_eq!(ImmuneEngine::survival_score(10, false), 0.0,
            "Dead KU = no survival bonus");
    }

    #[test]
    fn test_survival_to_u16() {
        assert_eq!(ImmuneEngine::survival_to_u16(0.0), 0);
        assert_eq!(ImmuneEngine::survival_to_u16(1.0), 10000);
        assert_eq!(ImmuneEngine::survival_to_u16(0.5), 5000);
    }

    #[test]
    fn test_too_few_replications_no_flags() {
        let obs = SpreadObservation {
            total_replications: 3,
            unique_sources: 1,
            replications_last_hour: 3,
            max_source_fraction: 1.0,
            total_usage_events: 0,
            age_secs: 3600,
        };
        let abs = ImmuneEngine::analyze(&obs, test_hash(1), T0);
        // Should NOT flag diversity/engagement with too few replications
        assert!(!abs.iter().any(|a| a.antibody_type == AntibodyType::LowEngagement));
        assert!(!abs.iter().any(|a| a.antibody_type == AntibodyType::DiversityDeficit));
    }
}
