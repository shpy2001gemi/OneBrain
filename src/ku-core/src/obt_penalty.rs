//! # OBT Penalty System
//!
//! Core principle: **"PoMV is non-punitive for NORMAL behavior. FRAUD is punished."**
//!
//! ```text
//! OBT earned = permanent  (G-Counter, no clawback)
//! Trust      = losable    (PN-Counter, can decrease when fraud detected)
//! ```
//!
//! Fraud punishment **never touches OBT balance**. A node that earned 10,000 OBT
//! legitimately then commits fraud keeps those 10,000 OBT. But their trust drops
//! → they cannot earn NEW OBT, cannot serve as witness, cannot participate in consensus.
//!
//! ## Five Penalty Tiers
//!
//! | Tier | Name            | Trust Formula                          |
//! |------|-----------------|----------------------------------------|
//! | 0    | Natural Decay   | Organic exponential decay              |
//! | 1    | Warning         | No trust reduction — flag only         |
//! | 2    | Trust Reduction | `trust × (1 - severity × 0.3)`        |
//! | 3    | Jail            | `trust × 0.2` (80% slash), 7-30 days  |
//! | 4    | Trust Zero      | `trust = 0.001`, banned 180 days       |
//! | 5    | Tombstone       | `trust = 0`, permanent ban             |
//!
//! ## Correlation Penalty (Ethereum-inspired)
//!
//! `multiplier = 1 + log₂(simultaneous_nodes_penalized)`
//!
//! See `docs/specs/obt/08_PENALTY.md`

use serde::{Serialize, Deserialize};

// Import shared constants from the canonical registry
use crate::obt_constants::{
    TRUST_DECAY_LAMBDA,
    TRUST_RECOVERY_MAX_PER_HOUR,
    TRUST_GRACE_PERIOD_HOURS,
};

// ═══════════════════════════════════════════════════════════════════════════
// Penalty-specific Constants (§8.6) — only used in this module
// ═══════════════════════════════════════════════════════════════════════════

/// Tier 1 warning expiry — 90 days in seconds.
pub const TIER1_EXPIRY_SECS: u64 = 90 * 24 * 3600;

/// Tier 2 maximum slash fraction (severity × 0.3, capped at 0.3).
pub const TIER2_MAX_SLASH: f64 = 0.30;

/// Tier 3 slash factor — trust × 0.2 (80% reduction).
pub const TIER3_SLASH_FACTOR: f64 = 0.20;

/// Tier 3 minimum jail duration in days.
pub const TIER3_JAIL_MIN_DAYS: u32 = 7;

/// Tier 3 maximum jail duration in days.
pub const TIER3_JAIL_MAX_DAYS: u32 = 30;

/// Tier 4 near-zero trust floor.
pub const TIER4_TRUST_FLOOR: f64 = 0.001;

/// Tier 4 ban duration — 180 days in seconds.
pub const TIER4_BAN_SECS: u64 = 180 * 24 * 3600;

/// Tier 5 trust value — absolute zero, permanent.
pub const TIER5_TRUST: f64 = 0.0;

/// Appeal trust scar — 30% permanent reduction on successful appeal.
pub const APPEAL_TRUST_SCAR: f64 = 0.30;

/// Global trust floor — never exactly zero except Tombstone.
pub const TRUST_FLOOR: f64 = 0.001;

/// Seconds per day (convenience).
const SECS_PER_DAY: u64 = 24 * 3600;

// ═══════════════════════════════════════════════════════════════════════════
// Penalty Tier
// ═══════════════════════════════════════════════════════════════════════════

/// Penalty tier — graduated from natural decay to permanent ban.
///
/// Tiers are ordered by severity: `NaturalDecay < Warning < ... < Tombstone`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PenaltyTier {
    /// Tier 0 — organic trust decay (NOT punishment).
    NaturalDecay = 0,
    /// Tier 1 — yellow card, expires 90 days.
    Warning = 1,
    /// Tier 2 — soft slash: `trust × (1 - severity × 0.3)`.
    TrustReduction = 2,
    /// Tier 3 — `trust × 0.2`, excluded 7-30 days.
    Jail = 3,
    /// Tier 4 — `trust = 0.001`, banned 180 days.
    TrustZero = 4,
    /// Tier 5 — `trust = 0`, permanent ban. Irreversible absent L4 appeal.
    Tombstone = 5,
}

impl PenaltyTier {
    /// Human-readable name with icon.
    pub fn name(&self) -> &'static str {
        match self {
            Self::NaturalDecay  => "🌿 Natural Decay",
            Self::Warning       => "⚠️ Warning",
            Self::TrustReduction => "🟡 Trust Reduction",
            Self::Jail          => "🔴 Jail",
            Self::TrustZero     => "⛔ Trust Zero",
            Self::Tombstone     => "☠️ Tombstone",
        }
    }
}

impl std::fmt::Display for PenaltyTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Fraud Type
// ═══════════════════════════════════════════════════════════════════════════

/// Fraud type detected by the antibody / immune system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FraudType {
    /// Fake KU spam — re-verified encoding shows PoMV = 0 after 7 days.
    FakeKuSpam,
    /// Fake PoMV signals — metabolism anomaly, circular access.
    FakePomvSignals,
    /// Quick isolation attack — gossip gap detected.
    QuickIsolationAttack,
    /// Long-con isolation attack — sustained gossip gap.
    LongConIsolationAttack,
    /// Collusion ring (2-3 nodes).
    CollusionRingSmall,
    /// Collusion ring (4+ nodes, may include ring leader → Tombstone).
    CollusionRingLarge,
    /// Identity forgery — Ed25519 key impersonation.
    IdentityForgery,
    /// Rate-limit violation — per-tier rate exceeded.
    RateLimitViolation,
    /// Storage proof failure — PoS-KU challenge timeout or wrong answer.
    StorageProofFailure,
    /// Double-spend — Account-Chain fork detection.
    DoubleSpend,
}

impl FraudType {
    /// Base severity [0.0, 1.0] as defined in §8.4.
    pub fn base_severity(&self) -> f64 {
        match self {
            Self::FakeKuSpam            => 0.3,
            Self::FakePomvSignals       => 0.5,
            Self::QuickIsolationAttack  => 0.8,
            Self::LongConIsolationAttack => 0.8,
            Self::CollusionRingSmall    => 1.0,
            Self::CollusionRingLarge    => 1.0,
            Self::IdentityForgery       => 1.0,
            Self::RateLimitViolation    => 0.2,
            Self::StorageProofFailure   => 0.4,
            Self::DoubleSpend           => 0.7,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Appeal Status
// ═══════════════════════════════════════════════════════════════════════════

/// Status of the appeal process for a penalty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppealStatus {
    /// No appeal filed.
    None,
    /// L2 dispute window is open (48 hours before execution).
    DisputeWindowOpen,
    /// Appeal submitted (L3 retrospective or L4 tombstone).
    AppealSubmitted,
    /// Appeal was granted — trust partially restored.
    AppealGranted,
    /// Appeal was denied — penalty stands.
    AppealDenied,
}

// ═══════════════════════════════════════════════════════════════════════════
// Penalty Record
// ═══════════════════════════════════════════════════════════════════════════

/// Penalty record for a node.
///
/// Stored in the node's local penalty log. Each penalty creates one record
/// with before/after trust snapshots and an expiry timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PenaltyRecord {
    /// Ed25519 public key of the penalized node.
    pub node_id: [u8; 32],
    /// Penalty tier applied.
    pub tier: PenaltyTier,
    /// Fraud type that triggered the penalty.
    pub fraud_type: FraudType,
    /// Trust score before penalty application.
    pub trust_before: f64,
    /// Trust score after penalty application.
    pub trust_after: f64,
    /// Correlation multiplier (`1 + log₂(n)`).
    pub correlation_multiplier: f64,
    /// Epoch timestamp when penalty was applied.
    pub timestamp: u64,
    /// Epoch timestamp when penalty expires. `None` = permanent (Tombstone).
    pub expires: Option<u64>,
    /// Current appeal status.
    pub appeal_status: AppealStatus,
}

// ═══════════════════════════════════════════════════════════════════════════
// Core Functions
// ═══════════════════════════════════════════════════════════════════════════

/// Determine the default penalty tier for a fraud type.
///
/// Maps each [`FraudType`] to its default tier as defined in §8.4.
/// Actual tier may escalate via correlation penalty or repeat offenses.
pub fn determine_penalty_tier(fraud_type: FraudType) -> PenaltyTier {
    match fraud_type {
        FraudType::RateLimitViolation    => PenaltyTier::Warning,
        FraudType::FakeKuSpam            => PenaltyTier::TrustReduction,
        FraudType::FakePomvSignals       => PenaltyTier::TrustReduction,
        FraudType::StorageProofFailure   => PenaltyTier::TrustReduction,
        FraudType::QuickIsolationAttack  => PenaltyTier::Jail,
        FraudType::LongConIsolationAttack => PenaltyTier::Jail,
        FraudType::CollusionRingSmall    => PenaltyTier::Jail,
        FraudType::DoubleSpend           => PenaltyTier::Jail,
        FraudType::CollusionRingLarge    => PenaltyTier::TrustZero,
        FraudType::IdentityForgery       => PenaltyTier::Tombstone,
    }
}

/// Calculate correlation multiplier for simultaneous penalties.
///
/// `multiplier = 1 + log₂(n)` where n = number of nodes penalized simultaneously.
/// Isolated incidents (n ≤ 1) get ×1.0; coordinated attacks scale logarithmically.
///
/// # Examples
/// ```
/// # use ku_core::obt_penalty::correlation_multiplier;
/// assert_eq!(correlation_multiplier(1), 1.0);
/// assert_eq!(correlation_multiplier(2), 2.0);
/// assert_eq!(correlation_multiplier(4), 3.0);
/// ```
pub fn correlation_multiplier(simultaneous_nodes: u32) -> f64 {
    if simultaneous_nodes <= 1 {
        return 1.0;
    }
    1.0 + (simultaneous_nodes as f64).log2()
}

/// Calculate trust after penalty application.
///
/// Applies the tier-specific trust formula with correlation scaling:
///
/// - **Warning**: No trust change.
/// - **TrustReduction**: `trust × (1 - severity × 0.3 × corr)`, floored at [`TRUST_FLOOR`].
/// - **Jail**: `trust × TIER3_SLASH_FACTOR` (0.2), further scaled by correlation.
/// - **TrustZero**: [`TIER4_TRUST_FLOOR`] (0.001).
/// - **Tombstone**: [`TIER5_TRUST`] (0.0).
/// - **NaturalDecay**: unchanged (use [`compute_trust_decay`] instead).
pub fn compute_trust_after_penalty(
    current_trust: f64,
    tier: PenaltyTier,
    fraud_type: FraudType,
    correlation_mult: f64,
) -> f64 {
    match tier {
        PenaltyTier::NaturalDecay => current_trust,
        PenaltyTier::Warning => current_trust,
        PenaltyTier::TrustReduction => {
            let severity = fraud_type.base_severity();
            let loss = (severity * TIER2_MAX_SLASH * correlation_mult).min(1.0);
            let new_trust = current_trust * (1.0 - loss);
            new_trust.max(TRUST_FLOOR)
        }
        PenaltyTier::Jail => {
            // 80% slash, then correlation can push remaining down further
            let base = current_trust * TIER3_SLASH_FACTOR;
            let corr_loss = (1.0 - 1.0 / correlation_mult).min(0.9);
            let new_trust = base * (1.0 - corr_loss);
            new_trust.max(TRUST_FLOOR)
        }
        PenaltyTier::TrustZero => TIER4_TRUST_FLOOR,
        PenaltyTier::Tombstone => TIER5_TRUST,
    }
}

/// Calculate trust decay when a node is offline.
///
/// Uses exponential decay: `trust × e^(-λ × hours)`
/// where λ = [`TRUST_DECAY_LAMBDA`] (0.01).
///
/// **Grace period**: < 1 hour offline = NO decay (allows reboot/upgrade).
/// See spec §7.1.
///
/// This is Tier 0 (Natural Decay) — NOT punishment.
pub fn compute_trust_decay(trust: f64, offline_hours: f64) -> f64 {
    // Grace period: short outages don't trigger decay
    if offline_hours < TRUST_GRACE_PERIOD_HOURS {
        return trust;
    }
    let decayed = trust * (-TRUST_DECAY_LAMBDA * offline_hours).exp();
    decayed.max(TRUST_FLOOR)
}

/// Calculate trust recovery rate per hour of active interaction.
///
/// Recovery is capped at [`TRUST_RECOVERY_MAX_PER_HOUR`] (0.05).
/// The actual rate scales with interaction quality (0.0 = idle, 1.0 = fully active).
///
/// Returns the trust increment per hour.
pub fn compute_trust_recovery(interaction_rate: f64) -> f64 {
    let rate = interaction_rate.clamp(0.0, 1.0);
    rate * TRUST_RECOVERY_MAX_PER_HOUR
}

/// Check whether a penalty has expired.
///
/// Tombstone penalties (`expires == None`) never expire.
pub fn is_penalty_expired(record: &PenaltyRecord, current_time: u64) -> bool {
    match record.expires {
        None => false,              // Tombstone — permanent
        Some(expiry) => current_time >= expiry,
    }
}

/// Compute restored trust after a successful appeal (L3 retrospective).
///
/// Applies a 30% permanent scar: `pre_penalty_trust × 0.7`.
/// Even successful appeals leave a mark — prevents gaming the appeal system.
pub fn compute_appeal_restored_trust(pre_penalty_trust: f64) -> f64 {
    let restored = pre_penalty_trust * (1.0 - APPEAL_TRUST_SCAR);
    restored.max(TRUST_FLOOR)
}

/// Calculate jail duration (in seconds) based on fraud type and correlation.
///
/// Base duration is mapped from fraud severity:
/// - Low severity → 7 days
/// - High severity → 30 days
///
/// Correlation multiplier extends duration (capped at [`TIER3_JAIL_MAX_DAYS`]).
pub fn compute_jail_duration(fraud_type: FraudType, correlation_mult: f64) -> u64 {
    let severity = fraud_type.base_severity();
    let range = (TIER3_JAIL_MAX_DAYS - TIER3_JAIL_MIN_DAYS) as f64;
    let base_days = TIER3_JAIL_MIN_DAYS as f64 + severity * range;
    let scaled_days = (base_days * correlation_mult).min(TIER3_JAIL_MAX_DAYS as f64);
    (scaled_days as u64) * SECS_PER_DAY
}

/// Compute the expiry timestamp for a penalty.
///
/// Returns `None` for Tombstone (permanent). Other tiers get duration-based expiry.
pub fn compute_penalty_expiry(
    tier: PenaltyTier,
    fraud_type: FraudType,
    correlation_mult: f64,
    timestamp: u64,
) -> Option<u64> {
    match tier {
        PenaltyTier::NaturalDecay  => Some(timestamp), // Instant — not a real penalty
        PenaltyTier::Warning       => Some(timestamp + TIER1_EXPIRY_SECS),
        PenaltyTier::TrustReduction => Some(timestamp + TIER1_EXPIRY_SECS), // Must re-earn, flag expires
        PenaltyTier::Jail          => Some(timestamp + compute_jail_duration(fraud_type, correlation_mult)),
        PenaltyTier::TrustZero     => Some(timestamp + TIER4_BAN_SECS),
        PenaltyTier::Tombstone     => None, // PERMANENT
    }
}

/// Build a complete [`PenaltyRecord`] from detection parameters.
///
/// Convenience function that computes trust_after, expiry, and correlation
/// in one call.
pub fn build_penalty_record(
    node_id: [u8; 32],
    fraud_type: FraudType,
    current_trust: f64,
    simultaneous_nodes: u32,
    timestamp: u64,
) -> PenaltyRecord {
    let tier = determine_penalty_tier(fraud_type);
    let corr = correlation_multiplier(simultaneous_nodes);
    let trust_after = compute_trust_after_penalty(current_trust, tier, fraud_type, corr);
    let expires = compute_penalty_expiry(tier, fraud_type, corr, timestamp);

    PenaltyRecord {
        node_id,
        tier,
        fraud_type,
        trust_before: current_trust,
        trust_after,
        correlation_multiplier: corr,
        timestamp,
        expires,
        appeal_status: AppealStatus::None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Transfer Eligibility Check
// ═══════════════════════════════════════════════════════════════════════════

/// Check if a node is currently able to perform transfers.
///
/// Nodes with active Jail (Tier 3) or TrustZero/Tombstone (Tier 4/5) penalties
/// are NOT allowed to send OBT transfers. They can still receive.
///
/// Returns `Ok(())` if the node can transfer, `Err(reason)` if blocked.
pub fn check_transfer_eligibility(
    penalty_tier: PenaltyTier,
    jail_until: Option<u64>,  // Unix timestamp when jail expires
    current_ts: u64,
) -> Result<(), String> {
    match penalty_tier {
        PenaltyTier::Tombstone => {
            Err("node is permanently banned (Tombstone)".to_string())
        }
        PenaltyTier::TrustZero => {
            if let Some(until) = jail_until {
                if current_ts < until {
                    let remaining_days = (until - current_ts) / SECS_PER_DAY;
                    return Err(format!("node is banned for {} more days", remaining_days));
                }
            }
            Ok(()) // Ban expired
        }
        PenaltyTier::Jail => {
            if let Some(until) = jail_until {
                if current_ts < until {
                    let remaining_days = (until - current_ts) / SECS_PER_DAY;
                    return Err(format!("node is jailed for {} more days", remaining_days));
                }
            }
            Ok(()) // Jail expired
        }
        _ => Ok(()), // NaturalDecay, Warning, TrustReduction — can transfer
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── Penalty tier mapping ─────────────────────────────────────────────

    #[test]
    fn test_tier_for_rate_limit_is_warning() {
        assert_eq!(
            determine_penalty_tier(FraudType::RateLimitViolation),
            PenaltyTier::Warning,
        );
    }

    #[test]
    fn test_tier_for_fake_ku_is_trust_reduction() {
        assert_eq!(
            determine_penalty_tier(FraudType::FakeKuSpam),
            PenaltyTier::TrustReduction,
        );
    }

    #[test]
    fn test_tier_for_isolation_attack_is_jail() {
        assert_eq!(
            determine_penalty_tier(FraudType::QuickIsolationAttack),
            PenaltyTier::Jail,
        );
    }

    #[test]
    fn test_tier_for_large_collusion_is_trust_zero() {
        assert_eq!(
            determine_penalty_tier(FraudType::CollusionRingLarge),
            PenaltyTier::TrustZero,
        );
    }

    #[test]
    fn test_tier_for_identity_forgery_is_tombstone() {
        assert_eq!(
            determine_penalty_tier(FraudType::IdentityForgery),
            PenaltyTier::Tombstone,
        );
    }

    // ── Correlation multiplier ───────────────────────────────────────────

    #[test]
    fn test_correlation_multiplier_single_node() {
        assert_eq!(correlation_multiplier(1), 1.0);
        assert_eq!(correlation_multiplier(0), 1.0);
    }

    #[test]
    fn test_correlation_multiplier_scaling() {
        // 2 nodes → 1 + log₂(2) = 2.0
        assert!((correlation_multiplier(2) - 2.0).abs() < 1e-10);
        // 4 nodes → 1 + log₂(4) = 3.0
        assert!((correlation_multiplier(4) - 3.0).abs() < 1e-10);
        // 8 nodes → 1 + log₂(8) = 4.0
        assert!((correlation_multiplier(8) - 4.0).abs() < 1e-10);
        // 16 nodes → 1 + log₂(16) = 5.0
        assert!((correlation_multiplier(16) - 5.0).abs() < 1e-10);
    }

    // ── Trust after penalty ──────────────────────────────────────────────

    #[test]
    fn test_warning_does_not_change_trust() {
        let trust = compute_trust_after_penalty(
            0.8, PenaltyTier::Warning, FraudType::RateLimitViolation, 1.0,
        );
        assert_eq!(trust, 0.8);
    }

    #[test]
    fn test_trust_reduction_soft_slash() {
        // FakeKuSpam: severity=0.3, corr=1.0
        // loss = 0.3 * 0.3 * 1.0 = 0.09 → trust = 0.8 * 0.91 = 0.728
        let trust = compute_trust_after_penalty(
            0.8, PenaltyTier::TrustReduction, FraudType::FakeKuSpam, 1.0,
        );
        assert!((trust - 0.728).abs() < 1e-10, "Expected ~0.728, got {}", trust);
    }

    #[test]
    fn test_jail_slash_80_percent() {
        // Jail: trust × 0.2, corr=1.0 → corr_loss = 0
        let trust = compute_trust_after_penalty(
            1.0, PenaltyTier::Jail, FraudType::QuickIsolationAttack, 1.0,
        );
        assert!((trust - 0.2).abs() < 1e-10, "Expected 0.2, got {}", trust);
    }

    #[test]
    fn test_trust_zero_floor() {
        let trust = compute_trust_after_penalty(
            0.9, PenaltyTier::TrustZero, FraudType::CollusionRingLarge, 1.0,
        );
        assert_eq!(trust, TIER4_TRUST_FLOOR);
    }

    #[test]
    fn test_tombstone_trust_zero() {
        let trust = compute_trust_after_penalty(
            0.9, PenaltyTier::Tombstone, FraudType::IdentityForgery, 1.0,
        );
        assert_eq!(trust, TIER5_TRUST);
        assert_eq!(trust, 0.0);
    }

    // ── Trust decay ──────────────────────────────────────────────────────

    #[test]
    fn test_trust_decay_over_time() {
        let initial = 1.0;
        // After 100 hours: 1.0 × e^(-0.01 × 100) = e^(-1) ≈ 0.3679
        let decayed = compute_trust_decay(initial, 100.0);
        assert!((decayed - 0.3679).abs() < 0.001, "Expected ~0.368, got {}", decayed);

        // After 0 hours: no change
        assert_eq!(compute_trust_decay(initial, 0.0), 1.0);

        // Monotonically decreasing
        let d1 = compute_trust_decay(initial, 10.0);
        let d2 = compute_trust_decay(initial, 50.0);
        assert!(d1 > d2, "Decay must be monotonically decreasing");
    }

    // ── Trust recovery ───────────────────────────────────────────────────

    #[test]
    fn test_trust_recovery() {
        // Full interaction → max 0.05/hour
        assert_eq!(compute_trust_recovery(1.0), TRUST_RECOVERY_MAX_PER_HOUR);
        // No interaction → 0
        assert_eq!(compute_trust_recovery(0.0), 0.0);
        // Half interaction → 0.025
        assert!((compute_trust_recovery(0.5) - 0.025).abs() < 1e-10);
        // Clamped above 1.0
        assert_eq!(compute_trust_recovery(2.0), TRUST_RECOVERY_MAX_PER_HOUR);
    }

    // ── Appeal restoration ───────────────────────────────────────────────

    #[test]
    fn test_appeal_restored_trust() {
        // 30% scar: 0.9 * 0.7 = 0.63
        let restored = compute_appeal_restored_trust(0.9);
        assert!((restored - 0.63).abs() < 1e-10, "Expected 0.63, got {}", restored);

        // Very low trust → floor
        let restored_low = compute_appeal_restored_trust(0.001);
        assert!(restored_low >= TRUST_FLOOR);
    }

    // ── Tombstone permanence ─────────────────────────────────────────────

    #[test]
    fn test_tombstone_is_permanent() {
        let record = build_penalty_record(
            [0xAA; 32],
            FraudType::IdentityForgery,
            0.9,
            1,
            1_000_000,
        );

        assert_eq!(record.tier, PenaltyTier::Tombstone);
        assert_eq!(record.trust_after, 0.0);
        assert_eq!(record.expires, None);

        // Never expires — even far in the future
        assert!(!is_penalty_expired(&record, u64::MAX));
    }

    // ── Jail duration ────────────────────────────────────────────────────

    #[test]
    fn test_jail_duration_scaling() {
        // Low severity (0.2 — RateLimitViolation): ~7 + 0.2*23 = 11.6 → 11 days
        let dur_low = compute_jail_duration(FraudType::RateLimitViolation, 1.0);
        assert!(dur_low >= (TIER3_JAIL_MIN_DAYS as u64) * SECS_PER_DAY);
        assert!(dur_low <= (TIER3_JAIL_MAX_DAYS as u64) * SECS_PER_DAY);

        // High severity (1.0 — CollusionRingSmall): 7 + 1.0*23 = 30 days
        let dur_high = compute_jail_duration(FraudType::CollusionRingSmall, 1.0);
        assert_eq!(dur_high, (TIER3_JAIL_MAX_DAYS as u64) * SECS_PER_DAY);

        // Correlation stretches but caps at max
        let dur_corr = compute_jail_duration(FraudType::FakeKuSpam, 5.0);
        assert!(dur_corr <= (TIER3_JAIL_MAX_DAYS as u64) * SECS_PER_DAY);
    }

    // ── Penalty expiry ───────────────────────────────────────────────────

    #[test]
    fn test_penalty_expiry() {
        let ts = 1_000_000;

        // Warning: expires after 90 days
        let warning = build_penalty_record([1; 32], FraudType::RateLimitViolation, 0.8, 1, ts);
        assert_eq!(warning.expires, Some(ts + TIER1_EXPIRY_SECS));
        assert!(!is_penalty_expired(&warning, ts + 1));
        assert!(is_penalty_expired(&warning, ts + TIER1_EXPIRY_SECS));

        // TrustZero: expires after 180 days
        let tz = build_penalty_record([2; 32], FraudType::CollusionRingLarge, 0.8, 1, ts);
        assert_eq!(tz.expires, Some(ts + TIER4_BAN_SECS));
        assert!(!is_penalty_expired(&tz, ts + TIER4_BAN_SECS - 1));
        assert!(is_penalty_expired(&tz, ts + TIER4_BAN_SECS));

        // Tombstone: never expires
        let tomb = build_penalty_record([3; 32], FraudType::IdentityForgery, 0.8, 1, ts);
        assert_eq!(tomb.expires, None);
        assert!(!is_penalty_expired(&tomb, u64::MAX));
    }

    // ── Transfer eligibility ─────────────────────────────────────────────

    #[test]
    fn test_transfer_eligibility_normal() {
        assert!(check_transfer_eligibility(PenaltyTier::NaturalDecay, None, 1000).is_ok());
        assert!(check_transfer_eligibility(PenaltyTier::Warning, None, 1000).is_ok());
        assert!(check_transfer_eligibility(PenaltyTier::TrustReduction, None, 1000).is_ok());
    }

    #[test]
    fn test_transfer_eligibility_jailed() {
        // Currently jailed
        let result = check_transfer_eligibility(PenaltyTier::Jail, Some(2000), 1000);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("jailed"));

        // Jail expired
        assert!(check_transfer_eligibility(PenaltyTier::Jail, Some(500), 1000).is_ok());
    }

    #[test]
    fn test_transfer_eligibility_banned() {
        let result = check_transfer_eligibility(PenaltyTier::TrustZero, Some(999999), 1000);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("banned"));
    }

    #[test]
    fn test_transfer_eligibility_tombstone() {
        let result = check_transfer_eligibility(PenaltyTier::Tombstone, None, 1000);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("permanently banned"));
    }
}
