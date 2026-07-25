//! # Anti-Gaming Module (§5)
//!
//! Implements rate limiting (§5.2), KU quality gates (§5.4), and
//! gaming-pattern detection (§5.5) for the OBT token system.

use crate::obt_constants::{self, NodeTier};
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────
// §5.2  Rate Limiter
// ─────────────────────────────────────────────────────────────────────

/// Tier-based rate-limit configuration.
#[derive(Debug, Clone)]
pub struct RateLimits {
    pub max_ku_per_hour: u32,
    pub max_encode_per_hour: u32,
    pub claim_cooldown_s: u64,
    pub max_mint_per_epoch: u64, // milliOBT
}

/// Consensus and reward rate limits from the frozen OBT specification.
///
/// Development admission settings must never replace these values: peers use
/// this profile when validating economic behavior.
pub const RATE_LEAF: RateLimits = RateLimits {
    max_ku_per_hour: obt_constants::MAX_KU_PER_HOUR_LEAF,
    max_encode_per_hour: obt_constants::MAX_ENCODINGS_PER_HOUR_LEAF,
    claim_cooldown_s: obt_constants::COOLDOWN_LEAF_S,
    max_mint_per_epoch: 10_000,
};

pub const RATE_CONTRIBUTOR: RateLimits = RateLimits {
    max_ku_per_hour: obt_constants::MAX_KU_PER_HOUR_CONTRIBUTOR,
    max_encode_per_hour: obt_constants::MAX_ENCODINGS_PER_HOUR_CONTRIBUTOR,
    claim_cooldown_s: obt_constants::COOLDOWN_CONTRIBUTOR_S,
    max_mint_per_epoch: 50_000,
};

pub const RATE_LOCAL_SP_PLUS: RateLimits = RateLimits {
    max_ku_per_hour: obt_constants::MAX_KU_PER_HOUR_LOCALSP,
    max_encode_per_hour: obt_constants::MAX_ENCODINGS_PER_HOUR_LOCALSP,
    claim_cooldown_s: obt_constants::COOLDOWN_LOCALSP_S,
    max_mint_per_epoch: 100_000,
};

/// Non-economic local admission limits used only by explicit development
/// tooling. The type deliberately contains no cooldown or mint fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalAdmissionLimits {
    pub max_ku_per_hour: u32,
    pub max_encode_per_hour: u32,
}

const DEV_RATE_LEAF: LocalAdmissionLimits = LocalAdmissionLimits {
    max_ku_per_hour: 100,
    max_encode_per_hour: 100,
};

/// Selects local admission behavior without changing consensus or rewards.
///
/// `Development` is intentionally not serializable as node configuration.
/// Callers must opt into it in development-only composition code. Economic
/// checks continue to use [`rate_limits_for_tier`] for every variant.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AntiGamingPolicy {
    #[default]
    Production,
    Development,
}

impl AntiGamingPolicy {
    /// Local, non-economic admission limits for this policy.
    pub fn local_admission_limits(self, tier: NodeTier) -> LocalAdmissionLimits {
        if self == Self::Development && tier == NodeTier::Leaf {
            return DEV_RATE_LEAF;
        }

        let production = rate_limits_for_tier(tier);
        LocalAdmissionLimits {
            max_ku_per_hour: production.max_ku_per_hour,
            max_encode_per_hour: production.max_encode_per_hour,
        }
    }

    /// Consensus/reward limits are immutable across local admission policies.
    pub fn economic_limits(self, tier: NodeTier) -> &'static RateLimits {
        let _ = self;
        rate_limits_for_tier(tier)
    }
}

/// Return the static rate-limit profile for a trust tier.
pub fn rate_limits_for_tier(tier: NodeTier) -> &'static RateLimits {
    match tier {
        NodeTier::Leaf => &RATE_LEAF,
        NodeTier::Contributor => &RATE_CONTRIBUTOR,
        _ => &RATE_LOCAL_SP_PLUS,
    }
}

const RATE_WINDOW_S: u64 = 3600; // 1-hour sliding window

/// Per-node tracker for rate-limit enforcement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitTracker {
    ku_timestamps: Vec<u64>,
    encode_timestamps: Vec<u64>,
    last_claim_ts: u64,
    epoch_mint_total: u64,
    current_epoch: u64,
}

impl RateLimitTracker {
    pub fn new(epoch: u64) -> Self {
        Self {
            ku_timestamps: Vec::new(),
            encode_timestamps: Vec::new(),
            last_claim_ts: 0,
            epoch_mint_total: 0,
            current_epoch: epoch,
        }
    }

    /// Returns `true` when a new KU submission is allowed.
    pub fn check_ku_rate(&self, now: u64, tier: NodeTier) -> bool {
        self.check_ku_rate_with_policy(now, tier, AntiGamingPolicy::Production)
    }

    /// Returns `true` when local admission allows a new KU submission.
    ///
    /// This method is not a consensus or reward check.
    pub fn check_ku_rate_with_policy(
        &self,
        now: u64,
        tier: NodeTier,
        policy: AntiGamingPolicy,
    ) -> bool {
        let limits = policy.local_admission_limits(tier);
        let cutoff = now.saturating_sub(RATE_WINDOW_S);
        let count = self.ku_timestamps.iter().filter(|&&ts| ts > cutoff).count() as u32;
        count < limits.max_ku_per_hour
    }

    /// Returns `true` when a new encoding action is allowed.
    pub fn check_encode_rate(&self, now: u64, tier: NodeTier) -> bool {
        self.check_encode_rate_with_policy(now, tier, AntiGamingPolicy::Production)
    }

    /// Returns `true` when local admission allows a new encoding action.
    ///
    /// This method is not a consensus or reward check.
    pub fn check_encode_rate_with_policy(
        &self,
        now: u64,
        tier: NodeTier,
        policy: AntiGamingPolicy,
    ) -> bool {
        let limits = policy.local_admission_limits(tier);
        let cutoff = now.saturating_sub(RATE_WINDOW_S);
        let count = self
            .encode_timestamps
            .iter()
            .filter(|&&ts| ts > cutoff)
            .count() as u32;
        count < limits.max_encode_per_hour
    }

    /// Returns `true` when the claim cooldown has elapsed.
    pub fn check_claim_cooldown(&self, now: u64, tier: NodeTier) -> bool {
        let limits = rate_limits_for_tier(tier);
        now.saturating_sub(self.last_claim_ts) >= limits.claim_cooldown_s
    }

    /// Returns `true` when minting `amount` would stay within the epoch cap.
    pub fn check_mint_cap(&self, amount: u64, tier: NodeTier) -> bool {
        let limits = rate_limits_for_tier(tier);
        self.epoch_mint_total + amount <= limits.max_mint_per_epoch
    }

    pub fn record_ku(&mut self, ts: u64) {
        self.ku_timestamps.push(ts);
    }

    pub fn record_encode(&mut self, ts: u64) {
        self.encode_timestamps.push(ts);
    }

    pub fn record_claim(&mut self, ts: u64) {
        self.last_claim_ts = ts;
    }

    pub fn record_mint(&mut self, amount: u64) {
        self.epoch_mint_total += amount;
    }

    /// Roll over to a new epoch, resetting the mint counter.
    pub fn advance_epoch(&mut self, new_epoch: u64) {
        if new_epoch > self.current_epoch {
            self.current_epoch = new_epoch;
            self.epoch_mint_total = 0;
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// §5.4  KU Quality Gates
// ─────────────────────────────────────────────────────────────────────

pub const MIN_KU_RAW_BYTES: usize = obt_constants::MIN_KU_RAW_SIZE;
pub const MIN_GENE_COUNT: usize = obt_constants::MIN_GENE_COUNT as usize;
pub const MIN_ENCODING_TIME_MS: u64 = obt_constants::MIN_ENCODING_TIME_MS;
pub const MIN_BOND_COUNT: usize = obt_constants::MIN_BOND_COUNT as usize;
pub const POMV_GATE_7D_THRESHOLD: f32 = obt_constants::MIN_POMV_7D;
pub const POMV_GATE_30D_THRESHOLD: f32 = obt_constants::MIN_POMV_30D;
pub const POMV_GRACE_PERIOD_EPOCHS: u64 = 168;
pub const ENCODING_CONSENSUS_MIN_VERIFIERS: u32 = obt_constants::MIN_ENCODING_VERIFY_COUNT;

/// Gate 1 – minimum payload size and gene count.
pub fn gate_1_min_size(raw_size: usize, gene_count: usize) -> bool {
    raw_size >= MIN_KU_RAW_BYTES && gene_count >= MIN_GENE_COUNT
}

/// Gate 2 – encoding consensus: enough independent verifiers, no duplicate.
pub fn gate_2_encoding_consensus(verifier_count: u32, is_duplicate: bool) -> bool {
    verifier_count >= ENCODING_CONSENSUS_MIN_VERIFIERS && !is_duplicate
}

/// Quality Gate 3: PoMV score threshold.
///
/// - New KUs (age <= 168 epochs = 7 days): always pass (grace period)
/// - Young KUs (age <= 720 epochs = 30 days): must meet 7-day threshold (0.01)
/// - Mature KUs (age > 720 epochs): must meet 30-day threshold (0.05)
pub fn gate_3_pomv(pomv_score: f32, age_epochs: u64) -> bool {
    if age_epochs <= POMV_GRACE_PERIOD_EPOCHS {
        return true; // grace window
    }
    // 30-day boundary: 720 epochs (30 * 24)
    if age_epochs > 720 {
        return pomv_score >= POMV_GATE_30D_THRESHOLD;
    }
    pomv_score >= POMV_GATE_7D_THRESHOLD
}

/// Gate 4 – complexity: encoding time and bond diversity.
pub fn gate_4_complexity(encoding_time_ms: u64, bond_count: usize) -> bool {
    encoding_time_ms >= MIN_ENCODING_TIME_MS && bond_count >= MIN_BOND_COUNT
}

// ─────────────────────────────────────────────────────────────────────
// §5.5  Pattern Detection
// ─────────────────────────────────────────────────────────────────────

/// Known gaming-attack patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GamingPattern {
    IsolationAttack,
    BurstSpam,
    WashTrading,
    TrustFarming,
}

/// Weighted composite score for a single pattern.
#[derive(Debug, Clone)]
pub struct PatternScore {
    pub pattern: GamingPattern,
    pub score: f64, // 0.0 – 1.0
}

/// Recommended enforcement action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PenaltyRecommendation {
    None,
    ElevatedScrutiny,
    Warning,
    TrustReduction,
    Jail,
    /// Reserved for manual escalation — never auto-recommended by `recommend_penalty()`.
    Tombstone,
}

fn weighted_sum(signals: &[f64], weights: &[f64]) -> f64 {
    let raw: f64 = signals.iter().zip(weights).map(|(s, w)| s * w).sum();
    raw.clamp(0.0, 1.0)
}

/// Isolation-attack detector (§5.5).
/// Weights: simultaneous_offline 0.40, gossip_gap 0.30,
///          internal_witnesses_pct 0.20, burst_rate 0.10
pub fn compute_isolation_score(
    simultaneous_offline: u32,
    gossip_gap: bool,
    internal_witnesses_pct: f64,
    burst_rate: f64,
) -> PatternScore {
    let signals = [
        (simultaneous_offline as f64 / 10.0).clamp(0.0, 1.0),
        if gossip_gap { 1.0 } else { 0.0 },
        internal_witnesses_pct.clamp(0.0, 1.0),
        burst_rate.clamp(0.0, 1.0),
    ];
    PatternScore {
        pattern: GamingPattern::IsolationAttack,
        score: weighted_sum(&signals, &[0.40, 0.30, 0.20, 0.10]),
    }
}

/// Burst-spam detector (§5.5).
/// Weights: rate_ratio 0.35, avg_size_ratio 0.25,
///          content_similarity 0.25, bond_diversity 0.15
pub fn compute_burst_score(
    rate_ratio: f64,
    avg_size_ratio: f64,
    content_similarity: f64,
    bond_diversity: f64,
) -> PatternScore {
    let signals = [
        rate_ratio.clamp(0.0, 1.0),
        avg_size_ratio.clamp(0.0, 1.0),
        content_similarity.clamp(0.0, 1.0),
        bond_diversity.clamp(0.0, 1.0),
    ];
    PatternScore {
        pattern: GamingPattern::BurstSpam,
        score: weighted_sum(&signals, &[0.35, 0.25, 0.25, 0.15]),
    }
}

/// Wash-trading detector (§5.5).
/// Weights: has_cycle 0.40, same_subnet 0.20,
///          amount_return_ratio 0.25, timing_regularity 0.15
pub fn compute_wash_score(
    has_cycle: bool,
    same_subnet: bool,
    amount_return_ratio: f64,
    timing_regularity: f64,
) -> PatternScore {
    let signals = [
        if has_cycle { 1.0 } else { 0.0 },
        if same_subnet { 1.0 } else { 0.0 },
        amount_return_ratio.clamp(0.0, 1.0),
        timing_regularity.clamp(0.0, 1.0),
    ];
    PatternScore {
        pattern: GamingPattern::WashTrading,
        score: weighted_sum(&signals, &[0.40, 0.20, 0.25, 0.15]),
    }
}

/// Long-con / trust-farming detector (§5.5).
/// Weights: trust_quality_gap 0.35, activity_spike_ratio 0.25,
///          witness_concentration 0.25, centrality_drop 0.15
pub fn compute_longcon_score(
    trust_quality_gap: f64,
    activity_spike_ratio: f64,
    witness_concentration: f64,
    centrality_drop: f64,
) -> PatternScore {
    let signals = [
        trust_quality_gap.clamp(0.0, 1.0),
        activity_spike_ratio.clamp(0.0, 1.0),
        witness_concentration.clamp(0.0, 1.0),
        centrality_drop.clamp(0.0, 1.0),
    ];
    PatternScore {
        pattern: GamingPattern::TrustFarming,
        score: weighted_sum(&signals, &[0.35, 0.25, 0.25, 0.15]),
    }
}

/// Map a composite score to a penalty recommendation.
pub fn recommend_penalty(score: f64) -> PenaltyRecommendation {
    if score >= 0.7 {
        PenaltyRecommendation::Jail
    } else if score >= 0.5 {
        PenaltyRecommendation::TrustReduction
    } else if score >= 0.4 {
        PenaltyRecommendation::Warning
    } else if score >= 0.3 {
        PenaltyRecommendation::ElevatedScrutiny
    } else {
        PenaltyRecommendation::None
    }
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Rate Limiter ────────────────────────────────────────────────

    #[test]
    fn test_rate_limits_for_tier_leaf() {
        let r = rate_limits_for_tier(NodeTier::Leaf);
        assert_eq!(r.max_ku_per_hour, 1);
        assert_eq!(r.claim_cooldown_s, 3600);
    }

    #[test]
    fn test_rate_limits_for_tier_contributor() {
        let r = rate_limits_for_tier(NodeTier::Contributor);
        assert_eq!(r.max_ku_per_hour, 5);
        assert_eq!(r.max_mint_per_epoch, 50_000);
    }

    #[test]
    fn test_rate_limits_for_tier_sp_fallback() {
        // Any tier ≥ 2 maps to LOCAL_SP_PLUS
        let r = rate_limits_for_tier(NodeTier::ContinentalSP);
        assert_eq!(r.max_ku_per_hour, 10);
    }

    #[test]
    fn development_policy_only_relaxes_local_leaf_admission() {
        let local = AntiGamingPolicy::Development.local_admission_limits(NodeTier::Leaf);
        assert_eq!(local.max_ku_per_hour, 100);
        assert_eq!(local.max_encode_per_hour, 100);

        let production = AntiGamingPolicy::Development.economic_limits(NodeTier::Leaf);
        assert_eq!(production.max_ku_per_hour, 1);
        assert_eq!(production.max_encode_per_hour, 2);
        assert_eq!(production.claim_cooldown_s, 3_600);
        assert_eq!(production.max_mint_per_epoch, 10_000);
    }

    #[test]
    fn anti_gaming_policy_defaults_to_production() {
        assert_eq!(AntiGamingPolicy::default(), AntiGamingPolicy::Production);
        assert_eq!(
            AntiGamingPolicy::default().local_admission_limits(NodeTier::Leaf),
            LocalAdmissionLimits {
                max_ku_per_hour: 1,
                max_encode_per_hour: 2,
            }
        );
    }

    #[test]
    fn test_tracker_ku_rate_allowed() {
        let t = RateLimitTracker::new(1);
        assert!(t.check_ku_rate(1000, NodeTier::Leaf)); // leaf: 1/hr, none recorded
    }

    #[test]
    fn test_tracker_ku_rate_exceeded() {
        let mut t = RateLimitTracker::new(1);
        t.record_ku(1000);
        assert!(!t.check_ku_rate(1001, NodeTier::Leaf)); // leaf: already 1 within window
        assert!(t.check_ku_rate_with_policy(1001, NodeTier::Leaf, AntiGamingPolicy::Development));
    }

    #[test]
    fn test_tracker_ku_rate_window_expiry() {
        let mut t = RateLimitTracker::new(1);
        t.record_ku(1000);
        // 3601 s later the old entry falls outside the 1-hour window
        assert!(t.check_ku_rate(4602, NodeTier::Leaf));
    }

    #[test]
    fn test_tracker_encode_rate() {
        let mut t = RateLimitTracker::new(1);
        t.record_encode(100);
        t.record_encode(200);
        assert!(!t.check_encode_rate(300, NodeTier::Leaf)); // leaf: 2/hr limit hit
        assert!(t.check_encode_rate(300, NodeTier::Contributor)); // contributor: 5/hr limit
    }

    #[test]
    fn test_tracker_claim_cooldown() {
        let mut t = RateLimitTracker::new(1);
        t.record_claim(1000);
        assert!(!t.check_claim_cooldown(2000, NodeTier::Leaf)); // leaf: 3600s cooldown
        assert!(t.check_claim_cooldown(5000, NodeTier::Leaf)); // 4000s > 3600s
    }

    #[test]
    fn test_tracker_mint_cap() {
        let mut t = RateLimitTracker::new(1);
        t.record_mint(9_000);
        assert!(t.check_mint_cap(1_000, NodeTier::Leaf)); // leaf: 10_000 cap
        assert!(!t.check_mint_cap(1_001, NodeTier::Leaf)); // over by 1
    }

    #[test]
    fn test_tracker_advance_epoch_resets_mint() {
        let mut t = RateLimitTracker::new(1);
        t.record_mint(5_000);
        t.advance_epoch(2);
        assert_eq!(t.epoch_mint_total, 0);
        assert_eq!(t.current_epoch, 2);
    }

    #[test]
    fn test_tracker_advance_epoch_no_regression() {
        let mut t = RateLimitTracker::new(5);
        t.record_mint(1_000);
        t.advance_epoch(3); // older epoch — should be a no-op
        assert_eq!(t.epoch_mint_total, 1_000);
        assert_eq!(t.current_epoch, 5);
    }

    // ── Quality Gates ───────────────────────────────────────────────

    #[test]
    fn test_gate_1_pass() {
        assert!(gate_1_min_size(256, 2));
    }

    #[test]
    fn test_gate_1_fail_size() {
        assert!(!gate_1_min_size(255, 2));
    }

    #[test]
    fn test_gate_1_fail_genes() {
        assert!(!gate_1_min_size(512, 1));
    }

    #[test]
    fn test_gate_2_pass() {
        assert!(gate_2_encoding_consensus(3, false));
    }

    #[test]
    fn test_gate_2_fail_duplicate() {
        assert!(!gate_2_encoding_consensus(5, true));
    }

    #[test]
    fn test_gate_2_fail_verifiers() {
        assert!(!gate_2_encoding_consensus(2, false));
    }

    #[test]
    fn test_gate_3_grace_period() {
        // Inside grace period — should pass even with bad score
        assert!(gate_3_pomv(0.0, 100));
    }

    #[test]
    fn test_gate_3_pass_after_grace() {
        assert!(gate_3_pomv(0.02, 200));
    }

    #[test]
    fn test_gate_3_fail_after_grace() {
        assert!(!gate_3_pomv(0.005, 200));
    }

    #[test]
    fn test_gate_3_pomv_30d_threshold() {
        // Mature KU (>720 epochs) must meet higher 30D threshold (0.05)
        assert!(!gate_3_pomv(0.03, 800)); // 0.03 < 0.05 threshold
        assert!(gate_3_pomv(0.06, 800)); // 0.06 >= 0.05 threshold
                                         // Young KU (168-720 epochs) only needs 7D threshold (0.01)
        assert!(gate_3_pomv(0.03, 500)); // 0.03 >= 0.01 threshold
    }

    #[test]
    fn test_gate_4_pass() {
        assert!(gate_4_complexity(100, 1));
    }

    #[test]
    fn test_gate_4_fail_time() {
        assert!(!gate_4_complexity(99, 2));
    }

    #[test]
    fn test_gate_4_fail_bonds() {
        assert!(!gate_4_complexity(200, 0));
    }

    // ── Pattern Detection ───────────────────────────────────────────

    #[test]
    fn test_isolation_score_weights() {
        // All signals maxed → 0.40+0.30+0.20+0.10 = 1.0
        let ps = compute_isolation_score(10, true, 1.0, 1.0);
        assert!((ps.score - 1.0).abs() < 1e-9);
        assert_eq!(ps.pattern, GamingPattern::IsolationAttack);
    }

    #[test]
    fn test_isolation_score_zero() {
        let ps = compute_isolation_score(0, false, 0.0, 0.0);
        assert!((ps.score).abs() < 1e-9);
    }

    #[test]
    fn test_burst_score_partial() {
        let ps = compute_burst_score(0.5, 0.5, 0.5, 0.5);
        assert!((ps.score - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_wash_score_cycle_only() {
        let ps = compute_wash_score(true, false, 0.0, 0.0);
        assert!((ps.score - 0.40).abs() < 1e-9);
    }

    #[test]
    fn test_longcon_score_full() {
        let ps = compute_longcon_score(1.0, 1.0, 1.0, 1.0);
        assert!((ps.score - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_recommend_penalty_none() {
        assert_eq!(recommend_penalty(0.0), PenaltyRecommendation::None);
        assert_eq!(recommend_penalty(0.29), PenaltyRecommendation::None);
    }

    #[test]
    fn test_recommend_penalty_elevated() {
        assert_eq!(
            recommend_penalty(0.3),
            PenaltyRecommendation::ElevatedScrutiny
        );
    }

    #[test]
    fn test_recommend_penalty_warning() {
        assert_eq!(recommend_penalty(0.45), PenaltyRecommendation::Warning);
    }

    #[test]
    fn test_recommend_penalty_trust_reduction() {
        assert_eq!(
            recommend_penalty(0.6),
            PenaltyRecommendation::TrustReduction
        );
    }

    #[test]
    fn test_recommend_penalty_jail() {
        assert_eq!(recommend_penalty(0.8), PenaltyRecommendation::Jail);
    }

    #[test]
    fn test_pattern_score_clamped() {
        // Values > 1.0 should clamp
        let ps = compute_burst_score(2.0, 3.0, 4.0, 5.0);
        assert!((ps.score - 1.0).abs() < 1e-9);
    }
}
