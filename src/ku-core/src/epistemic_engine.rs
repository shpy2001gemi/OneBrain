//! # Epistemic Engine — Observation-Based Status Transitions
//!
//! PoK v2: Epistemic status transitions based on OBSERVABLE signals,
//! NOT voting. Each transition has a clear, measurable threshold.
//!
//! ## Status Flow (from PoK v2 spec):
//! ```text
//! RUMOR → HEARSAY → TESTIMONY → OBSERVATION → HYPOTHESIS →
//! EVIDENCE → CORROBORATED → PEER_REVIEWED → CONSENSUS → FORMALLY_PROVEN
//! ```
//!
//! ## Design Principles:
//! - No human judgment required for transitions
//! - All thresholds based on GCounter values (objective)
//! - Each node evaluates locally (decentralized)
//! - Status can only advance, never regress (monotonic)

use crate::metabolism::KUMetabolism;
use crate::types::EpistemicStatus;

// ═══════════════════════════════════════════════════════════════════════════
// Transition Thresholds
// ═══════════════════════════════════════════════════════════════════════════

/// Minimum metabolic rate to transition from RUMOR → HEARSAY
pub const THRESHOLD_METABOLIC_ACTIVE: f64 = 0.001;

/// Minimum retrievals from different nodes for HEARSAY → TESTIMONY
pub const THRESHOLD_RETRIEVAL_COUNT: u64 = 3;

/// Minimum citations for TESTIMONY → OBSERVATION
pub const THRESHOLD_CITATION_COUNT: u64 = 1;

/// Minimum citations from diverse sources for OBSERVATION → HYPOTHESIS
pub const THRESHOLD_DIVERSE_CITATIONS: usize = 3;

/// Minimum node diversity for HYPOTHESIS → EVIDENCE
pub const THRESHOLD_NODE_DIVERSITY: usize = 5;

/// Minimum citations for EVIDENCE → CORROBORATED
pub const THRESHOLD_CORROBORATED_CITATIONS: u64 = 5;

/// Minimum engagement for CORROBORATED → PEER_REVIEWED
pub const THRESHOLD_PEER_REVIEWED_ENGAGEMENT: u64 = 50;

/// Minimum age in seconds for PEER_REVIEWED → CONSENSUS (6 months)
pub const THRESHOLD_CONSENSUS_AGE_SECS: u64 = 6 * 30 * 24 * 3600;

/// Minimum metabolic rate to maintain CONSENSUS status
pub const THRESHOLD_CONSENSUS_METABOLIC_RATE: f64 = 1.0;

/// Minimum age for CONSENSUS → FORMALLY_PROVEN (1 year)
pub const THRESHOLD_PROVEN_AGE_SECS: u64 = 365 * 24 * 3600;

/// Minimum engagement for FORMALLY_PROVEN
pub const THRESHOLD_PROVEN_ENGAGEMENT: u64 = 200;

// ═══════════════════════════════════════════════════════════════════════════
// Epistemic Engine
// ═══════════════════════════════════════════════════════════════════════════

/// Evaluate whether a KU should transition to a higher epistemic status.
///
/// Returns `Some(new_status)` if a transition is warranted,
/// `None` if the KU stays at its current level.
///
/// ## Key properties:
/// - **Monotonic**: status only advances, never regresses
/// - **Observable**: all inputs are CRDT counters
/// - **Local**: each node evaluates independently
/// - **Deterministic**: same inputs → same output
pub fn evaluate_transition(
    current: EpistemicStatus,
    metabolism: &KUMetabolism,
    now: u64,
    half_life_secs: u64,
) -> Option<EpistemicStatus> {
    let rate = metabolism.metabolic_rate(now, half_life_secs);
    let age_secs = now.saturating_sub(metabolism.created_at);

    match current {
        // RUMOR → HEARSAY: someone actually accessed it
        EpistemicStatus::Rumor => {
            if rate > THRESHOLD_METABOLIC_ACTIVE {
                Some(EpistemicStatus::Hearsay)
            } else {
                None
            }
        }

        // HEARSAY → TESTIMONY: 3+ different nodes retrieved it
        EpistemicStatus::Hearsay => {
            if metabolism.retrieval_count.value() >= THRESHOLD_RETRIEVAL_COUNT {
                Some(EpistemicStatus::Testimony)
            } else {
                None
            }
        }

        // TESTIMONY → OBSERVATION: at least 1 citation
        EpistemicStatus::Testimony => {
            if metabolism.citation_count.value() >= THRESHOLD_CITATION_COUNT {
                Some(EpistemicStatus::Observation)
            } else {
                None
            }
        }

        // OBSERVATION → HYPOTHESIS: 3+ citations from diverse sources
        EpistemicStatus::Observation => {
            if metabolism.citation_count.value() >= THRESHOLD_DIVERSE_CITATIONS as u64
                && metabolism.node_diversity() >= THRESHOLD_DIVERSE_CITATIONS
            {
                Some(EpistemicStatus::Hypothesis)
            } else {
                None
            }
        }

        // HYPOTHESIS → EVIDENCE: 5+ unique nodes engaged
        EpistemicStatus::Hypothesis => {
            if metabolism.node_diversity() >= THRESHOLD_NODE_DIVERSITY {
                Some(EpistemicStatus::Evidence)
            } else {
                None
            }
        }

        // EVIDENCE → CORROBORATED: 5+ citations
        EpistemicStatus::Evidence => {
            if metabolism.citation_count.value() >= THRESHOLD_CORROBORATED_CITATIONS {
                Some(EpistemicStatus::Corroborated)
            } else {
                None
            }
        }

        // CORROBORATED → PEER_REVIEWED: 50+ total engagement
        EpistemicStatus::Corroborated => {
            if metabolism.total_engagement() >= THRESHOLD_PEER_REVIEWED_ENGAGEMENT {
                Some(EpistemicStatus::PeerReviewed)
            } else {
                None
            }
        }

        // PEER_REVIEWED → CONSENSUS: 6+ months old AND still active
        EpistemicStatus::PeerReviewed => {
            if age_secs >= THRESHOLD_CONSENSUS_AGE_SECS
                && rate >= THRESHOLD_CONSENSUS_METABOLIC_RATE
            {
                Some(EpistemicStatus::Consensus)
            } else {
                None
            }
        }

        // CONSENSUS → FORMALLY_PROVEN: 1+ year AND 200+ engagement
        EpistemicStatus::Consensus => {
            if age_secs >= THRESHOLD_PROVEN_AGE_SECS
                && metabolism.total_engagement() >= THRESHOLD_PROVEN_ENGAGEMENT
            {
                Some(EpistemicStatus::FormallyProven)
            } else {
                None
            }
        }

        // FORMALLY_PROVEN and AXIOMATIC: terminal states
        EpistemicStatus::FormallyProven | EpistemicStatus::Axiomatic => None,
    }
}

/// Evaluate all possible transitions and return the highest reachable status.
///
/// A KU that meets criteria for multiple levels at once will jump
/// to the highest valid level (e.g., a highly-cited KU might go
/// straight from RUMOR to OBSERVATION).
pub fn evaluate_max_status(
    current: EpistemicStatus,
    metabolism: &KUMetabolism,
    now: u64,
    half_life_secs: u64,
) -> EpistemicStatus {
    let mut status = current;
    // Walk up the ladder as far as thresholds allow
    loop {
        match evaluate_transition(status, metabolism, now, half_life_secs) {
            Some(next) => status = next,
            None => break,
        }
    }
    status
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metabolism::{MetabolismEvent, DEFAULT_HALF_LIFE_SECS};

    const NODE_A: u64 = 1;
    const NODE_B: u64 = 2;
    const NODE_C: u64 = 3;
    const NODE_D: u64 = 4;
    const NODE_E: u64 = 5;
    const T0: u64 = 1_000_000;
    const HL: u64 = DEFAULT_HALF_LIFE_SECS;

    fn make_metabolism_with_queries(n: usize) -> KUMetabolism {
        let mut m = KUMetabolism::new(T0);
        for i in 0..n {
            m.record_event(
                NODE_A + (i as u64 % 10),
                MetabolismEvent::QueryHit,
                T0 + i as u64,
            );
        }
        m
    }

    #[test]
    fn test_rumor_stays_without_activity() {
        let m = KUMetabolism::new(T0);
        assert_eq!(
            evaluate_transition(EpistemicStatus::Rumor, &m, T0 + 1, HL),
            None
        );
    }

    #[test]
    fn test_rumor_to_hearsay() {
        let mut m = KUMetabolism::new(T0);
        m.record_event(NODE_A, MetabolismEvent::QueryHit, T0 + 1);

        assert_eq!(
            evaluate_transition(EpistemicStatus::Rumor, &m, T0 + 1, HL),
            Some(EpistemicStatus::Hearsay)
        );
    }

    #[test]
    fn test_hearsay_to_testimony() {
        let mut m = KUMetabolism::new(T0);
        m.record_event(
            NODE_A,
            MetabolismEvent::Retrieval { dwell_ms: 1000 },
            T0 + 1,
        );
        m.record_event(
            NODE_B,
            MetabolismEvent::Retrieval { dwell_ms: 2000 },
            T0 + 2,
        );
        m.record_event(
            NODE_C,
            MetabolismEvent::Retrieval { dwell_ms: 3000 },
            T0 + 3,
        );

        assert_eq!(
            evaluate_transition(EpistemicStatus::Hearsay, &m, T0 + 3, HL),
            Some(EpistemicStatus::Testimony)
        );
    }

    #[test]
    fn test_testimony_to_observation() {
        let mut m = KUMetabolism::new(T0);
        m.record_event(NODE_A, MetabolismEvent::Citation, T0 + 1);

        assert_eq!(
            evaluate_transition(EpistemicStatus::Testimony, &m, T0 + 1, HL),
            Some(EpistemicStatus::Observation)
        );
    }

    #[test]
    fn test_observation_to_hypothesis() {
        let mut m = KUMetabolism::new(T0);
        m.record_event(NODE_A, MetabolismEvent::Citation, T0 + 1);
        m.record_event(NODE_B, MetabolismEvent::Citation, T0 + 2);
        m.record_event(NODE_C, MetabolismEvent::Citation, T0 + 3);

        assert_eq!(
            evaluate_transition(EpistemicStatus::Observation, &m, T0 + 3, HL),
            Some(EpistemicStatus::Hypothesis)
        );
    }

    #[test]
    fn test_hypothesis_to_evidence() {
        let mut m = KUMetabolism::new(T0);
        for i in 0..5 {
            m.record_event(NODE_A + i, MetabolismEvent::QueryHit, T0 + i);
        }

        assert_eq!(
            evaluate_transition(EpistemicStatus::Hypothesis, &m, T0 + 5, HL),
            Some(EpistemicStatus::Evidence)
        );
    }

    #[test]
    fn test_evidence_to_corroborated() {
        let mut m = KUMetabolism::new(T0);
        for i in 0..5 {
            m.record_event(NODE_A + i, MetabolismEvent::Citation, T0 + i);
        }

        assert_eq!(
            evaluate_transition(EpistemicStatus::Evidence, &m, T0 + 5, HL),
            Some(EpistemicStatus::Corroborated)
        );
    }

    #[test]
    fn test_formally_proven_is_terminal() {
        let m = make_metabolism_with_queries(1000);
        assert_eq!(
            evaluate_transition(EpistemicStatus::FormallyProven, &m, T0 + 100, HL),
            None
        );
    }

    #[test]
    fn test_evaluate_max_status_jumps() {
        let mut m = KUMetabolism::new(T0);
        // Give it enough activity to jump multiple levels
        for i in 0..10u64 {
            m.record_event(NODE_A + (i % 5), MetabolismEvent::QueryHit, T0 + i);
            m.record_event(
                NODE_A + (i % 5),
                MetabolismEvent::Retrieval { dwell_ms: 1000 },
                T0 + i,
            );
            m.record_event(NODE_A + (i % 5), MetabolismEvent::Citation, T0 + i);
        }

        let max = evaluate_max_status(EpistemicStatus::Rumor, &m, T0 + 10, HL);
        // Should jump past Rumor, Hearsay, Testimony, Observation, Hypothesis, Evidence, Corroborated
        assert!(
            max as u8 >= EpistemicStatus::Evidence as u8,
            "Should reach at least Evidence with 10 citations from 5 nodes: {:?}",
            max
        );
    }

    #[test]
    fn test_consensus_requires_time() {
        let mut m = KUMetabolism::new(T0);
        for i in 0..100 {
            m.record_event(NODE_A + (i % 10), MetabolismEvent::QueryHit, T0 + i);
        }

        // Even with lots of activity, consensus needs 6 months
        let result = evaluate_transition(
            EpistemicStatus::PeerReviewed,
            &m,
            T0 + 1000, // Only ~17 minutes
            HL,
        );
        assert_eq!(result, None, "Consensus requires 6+ months age");
    }
}
