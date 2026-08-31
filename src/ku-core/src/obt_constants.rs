//! # OBT Token Constants
//!
//! Central registry of **all** OBT protocol constants.
//! Every constant in the OBT specification lives here, organized by category.
//!
//! ## Categories (96 constants total)
//! - **§9.1 Epoch & Timing** — epoch duration, confirmation timeouts
//! - **§9.2 Emission & Rewards** — base emission, multipliers, encoding/storage/PoMV rewards
//! - **§9.3 Rate Limits** — KU creation, encoding, transfer, PoS-KU limits per tier
//! - **§9.4 Quality Gates** — minimum thresholds for valid KUs
//! - **§9.5 Trust & Security** — decay, recovery, gossip gap, connectivity proof
//! - **§9.6 Penalty** — graduated penalty tiers, appeal windows
//! - **§9.7 Transfer & Wire** — message type codes, account-chain constants
//! - **§9.8 Gaming Detection** — anti-gaming pattern thresholds
//! - **§9.9 Confirmation Levels** — block confirmation stages
//!
//! ## Reference
//! See `docs/specs/obt/09_CONSTANTS.md` for rationale and source sections.

use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════════════
// §9.1 — Epoch & Timing
// ═══════════════════════════════════════════════════════════════════════════

/// Duration of one OBT epoch in seconds.
/// 1 hour — compatible with pheromone decay, sufficient PoMV data
/// (3,600 SWIM probes, 120 gossip rounds), reward < 1hr from contribution.
pub const OBT_EPOCH_DURATION_S: u64 = 3_600;

/// Number of epochs in one day (3,600s × 24 = 86,400s).
pub const EPOCHS_PER_DAY: u64 = 24;

/// Number of epochs in one week (24 × 7).
pub const EPOCHS_PER_WEEK: u64 = 168;

/// Max wait for transfer confirmation (seconds).
/// Matches PoS-KU challenge timeout. Sufficient for DHT lookup + gossip propagation.
pub const CONFIRMATION_TIMEOUT_S: u64 = 30;

/// Encoding jobs expire after this many epochs if unclaimed (7 days).
pub const ENCODING_JOB_TTL_EPOCHS: u64 = 168;

// ═══════════════════════════════════════════════════════════════════════════
// §9.2 — Emission & Rewards
// ═══════════════════════════════════════════════════════════════════════════

// ─── Global Emission ───

/// Precision multiplier: all OBT amounts are stored in milliOBT.
/// 1 OBT = 1,000 milliOBT. Avoids f64→u64 truncation losing sub-unit rewards.
pub const OBT_PRECISION_MULTIPLIER: u64 = 1_000;

/// Governance-adjustable base emission per epoch (milliOBT).
/// 10,000 OBT = 10,000,000 milliOBT.
/// At 1,000 nodes × avg PoMV 0.7 = 7,000 OBT/epoch actual.
pub const BASE_EMISSION_PER_EPOCH: u64 = 10_000 * OBT_PRECISION_MULTIPLIER;

/// Activity multiplier target (nodes).
/// A(epoch) = min(active_nodes / TARGET, MAX). Network scales emission with adoption.
pub const ACTIVITY_MULTIPLIER_TARGET: u64 = 1_000;

/// Cap on activity multiplier.
/// At 10,000+ nodes, emission saturates at B × 10.
pub const ACTIVITY_MULTIPLIER_MAX: f64 = 10.0;

// ─── Stream Weights (R1–R4 allocation) ───

/// Default stream weights (governance-adjustable, must sum to 1.0).
///
/// - R1 (Owner/PoMV):  40%
/// - R2 (Encoder):     25%
/// - R3 (Verifier):    15%
/// - R4 (Storage):     20%
pub const STREAM_WEIGHTS: [f64; 4] = [
    0.40, // R1: Owner (PoMV-based)
    0.25, // R2: Encoder
    0.15, // R3: Verifier
    0.20, // R4: Storage
];

// ─── Encoding Rewards (R2/R3) ───

/// Base OBT reward per 1KB of raw text encoded.
/// Linear scaling with knowledge size.
pub const BASE_OBT_PER_KB: u64 = 1;

/// Bonus OBT for the first AI to encode a KU.
/// Incentivizes being first.
pub const FIRST_ENCODER_BONUS: u64 = 5;

/// Bonus OBT for pro-bono encoding (helping someone without AI).
pub const PRO_BONO_BONUS: u64 = 10;

/// Multiplier for correctors (found and fixed encoding errors).
/// Finding errors = 3× base reward.
pub const CORRECTOR_MULTIPLIER: u64 = 3;

// ─── Storage Rewards (R4) ───

/// Base storage reward per KU per epoch (OBT).
/// Low because storage is passive.
pub const STORAGE_BASE_RATE: f64 = 0.001;

/// Minimum size weight (KU 16 bytes = 0.1×).
pub const STORAGE_SIZE_WEIGHT_MIN: f64 = 0.1;

/// Maximum size weight (KU > 10KB capped).
pub const STORAGE_SIZE_WEIGHT_MAX: f64 = 10.0;

/// Minimum rarity weight (over-replicated KU).
pub const STORAGE_RARITY_WEIGHT_MIN: f64 = 0.5;

/// Maximum rarity weight (under-replicated KU = bonus).
pub const STORAGE_RARITY_WEIGHT_MAX: f64 = 3.0;

/// Minimum demand weight (unused KU = near-zero reward).
pub const STORAGE_DEMAND_WEIGHT_MIN: f64 = 0.1;

/// Maximum demand weight (hot KU = 5× bonus).
pub const STORAGE_DEMAND_WEIGHT_MAX: f64 = 5.0;

/// Maximum duration factor (100+ epochs = 2× bonus).
pub const STORAGE_DURATION_FACTOR_MAX: f64 = 2.0;

/// Number of epochs to reach maximum duration factor.
pub const DURATION_MATURITY_EPOCHS: u64 = 100;

/// Per-node per-epoch cap for storage rewards (OBT). Prevents domination.
pub const STORAGE_MAX_REWARD_PER_NODE_EPOCH: u64 = 10 * OBT_PRECISION_MULTIPLIER;

// ─── PoMV Rewards (R1) ───

/// DHT replication factor. 20 replicas per KU in DHT.
pub const K_TARGET: u32 = 20;

// ─── Trust Multipliers (per-tier reward cap) ───

/// Trust multiplier array indexed by NodeTier (0..=6).
/// `[Leaf=0.10, Contributor=0.50, LocalSP=1.00, RegionalSP=1.25,
///   NationalSP=1.50, ContinentalSP=1.75, GlobalSP=2.00]`
///
/// Effective max reward = E(epoch) / nodes × TRUST_MULTIPLIER[tier].
pub const TRUST_MULTIPLIER: [f64; 7] = [0.10, 0.50, 1.00, 1.25, 1.50, 1.75, 2.00];

/// Number of defined node tiers.
pub const NODE_TIER_COUNT: usize = 7;

// ═══════════════════════════════════════════════════════════════════════════
// §9.3 — Rate Limits
// ═══════════════════════════════════════════════════════════════════════════

// ─── KU Creation Rate Limits ───

/// Max KU creations per hour for Leaf (T0) nodes.
pub const MAX_KU_PER_HOUR_LEAF: u32 = 1;
/// Max KU creations per hour for Contributor (T1) nodes.
pub const MAX_KU_PER_HOUR_CONTRIBUTOR: u32 = 5;
/// Max KU creations per hour for LocalSP+ (T2+) nodes.
pub const MAX_KU_PER_HOUR_LOCALSP: u32 = 10;

/// Max encoding claims per hour for Leaf (T0) nodes.
pub const MAX_ENCODINGS_PER_HOUR_LEAF: u32 = 2;
/// Max encoding claims per hour for Contributor (T1) nodes.
pub const MAX_ENCODINGS_PER_HOUR_CONTRIBUTOR: u32 = 5;
/// Max encoding claims per hour for LocalSP+ (T2+) nodes.
pub const MAX_ENCODINGS_PER_HOUR_LOCALSP: u32 = 10;

/// Encoding claim cooldown for Leaf (T0) in seconds (60 min).
pub const COOLDOWN_LEAF_S: u64 = 3_600;
/// Encoding claim cooldown for Contributor (T1) in seconds (12 min).
pub const COOLDOWN_CONTRIBUTOR_S: u64 = 720;
/// Encoding claim cooldown for LocalSP+ (T2+) in seconds (6 min).
pub const COOLDOWN_LOCALSP_S: u64 = 360;

/// Encoding claim cooldown for Leaf (T0) in minutes.
pub const CLAIM_COOLDOWN_LEAF_MIN: u32 = 60;
/// Encoding claim cooldown for Contributor (T1) in minutes.
pub const CLAIM_COOLDOWN_CONTRIBUTOR_MIN: u32 = 12;
/// Encoding claim cooldown for LocalSP+ (T2+) in minutes.
pub const CLAIM_COOLDOWN_LOCALSP_MIN: u32 = 6;

// ─── Transfer Rate Limits ───

/// Anti-wash-trading: max transfers per epoch.
/// 100 transfers/hour sufficient for legitimate use.
pub const MAX_TRANSFERS_PER_EPOCH: u32 = 100;

/// Floor to prevent dust attacks (millions of micro-transfers).
/// Represents 0.001 OBT in milliunits.
pub const MIN_TRANSFER_AMOUNT: u64 = 1;

// ─── PoS-KU Challenge Limits ───

/// PoS-KU response timeout (seconds).
/// Fast enough for disk read, too fast for network fetch from elsewhere.
pub const POS_KU_RESPONSE_TIMEOUT_S: u64 = 30;

/// K=3 DHT-selected witnesses verify challenge responses.
pub const POS_KU_WITNESSES: u32 = 3;

/// Challenge rate: fraction of stored KUs challenged per epoch.
pub const CHALLENGE_RATE: f64 = 0.10;

// ═══════════════════════════════════════════════════════════════════════════
// §9.4 — Quality Gates
// ═══════════════════════════════════════════════════════════════════════════

/// ~50 words minimum. KU must contain meaningful content.
pub const MIN_KU_RAW_SIZE: usize = 256;

/// At least 2 Knowledge DNA genes. Ensures structural complexity.
pub const MIN_GENE_COUNT: u8 = 2;

/// Encoding Consensus needs 3+ independent AI verifiers.
pub const MIN_ENCODING_VERIFY_COUNT: u32 = 3;

/// PoMV ≥ 0.01 after 7 days. KU must show some metabolic activity.
pub const MIN_POMV_7D: f32 = 0.01;

/// PoMV ≥ 0.05 after 30 days. Long-term viability check.
pub const MIN_POMV_30D: f32 = 0.05;

/// Minimum encoding processing time (ms). Prevents pre-computed spam.
pub const MIN_ENCODING_TIME_MS: u64 = 100;

/// At least 1 synaptic bond (inter-KU connection).
pub const MIN_BOND_COUNT: u8 = 1;

// ═══════════════════════════════════════════════════════════════════════════
// §9.5 — Trust & Security
// ═══════════════════════════════════════════════════════════════════════════

// ─── Trust Decay (D6) ───

/// Exponential decay rate per hour.
/// Half-life = ln(2)/0.01 ≈ 69.3h ≈ 3 days.
/// Balanced between tolerating maintenance and detecting abandonment.
pub const TRUST_DECAY_LAMBDA: f64 = 0.01;

/// < 1 hour offline = no decay. Allows reboot/upgrade without penalty.
pub const TRUST_GRACE_PERIOD_HOURS: f64 = 1.0;

/// Derived: grace period in seconds.
pub const TRUST_GRACE_PERIOD_S: u64 = 3_600;

/// Max trust recovery rate per hour.
/// 0→1.0 takes 20h active. Recovery intentionally SLOWER than decay.
pub const TRUST_RECOVERY_MAX_PER_HOUR: f64 = 0.05;

/// Raw recovery = interactions × 0.01, capped at TRUST_RECOVERY_MAX_PER_HOUR.
pub const TRUST_RECOVERY_INTERACTION_FACTOR: f64 = 0.01;

// ─── Gossip Gap Detection (D7) ───

/// Window to detect simultaneous offline events (seconds).
pub const GOSSIP_GAP_WINDOW_S: u64 = 30;

/// ≥3 nodes offline in window = ELEVATED_SCRUTINY.
pub const GOSSIP_GAP_THRESHOLD_NODES: u32 = 3;

/// ≥5 nodes offline in window = RED_FLAG → manual review.
pub const GOSSIP_GAP_RED_FLAG_THRESHOLD: u32 = 5;

/// Under ELEVATED_SCRUTINY, require 2× the normal witness count.
pub const GOSSIP_GAP_WITNESS_MULTIPLIER: u32 = 2;

/// ELEVATED_SCRUTINY duration = gap_duration × 10, max 24h.
pub const GOSSIP_GAP_SCRUTINY_MULTIPLIER: u32 = 10;

/// Maximum ELEVATED_SCRUTINY duration (hours).
pub const GOSSIP_GAP_SCRUTINY_MAX_HOURS: u32 = 24;

// ─── Connectivity Proof (D8) ───

/// Minimum external gossip receipts required in MintProof.
pub const CONNECTIVITY_PROOF_COUNT: u32 = 3;

/// Receipts must be < 60s old. Prevents using cached/stale receipts.
pub const CONNECTIVITY_PROOF_TTL_S: u64 = 60;

// ─── Witness Selection ───

/// Minimum witnesses for any MintProof.
pub const MIN_WITNESSES: u32 = 3;

/// Maximum witnesses. K = min(max(3, active_nodes/100), 7).
pub const MAX_WITNESSES: u32 = 7;

// ═══════════════════════════════════════════════════════════════════════════
// §9.6 — Penalty System
// ═══════════════════════════════════════════════════════════════════════════

// ─── Tier Thresholds ───

/// Warning (yellow card) auto-expires after 90 days.
pub const PENALTY_WARNING_EXPIRY_DAYS: u64 = 90;

/// 3 active Tier 1 warnings → automatic Tier 2 escalation.
pub const TIER1_TO_TIER2_COUNT: u32 = 3;

/// trust_new = trust × (1 - severity × 0.3). Max 30% loss per slash.
pub const TIER2_SEVERITY_FACTOR: f64 = 0.3;

/// 3 Tier 2 offenses → escalate to Tier 3.
pub const TIER2_TO_TIER3_COUNT: u32 = 3;

/// trust_new = trust × 0.2 (80% slash).
pub const TIER3_SLASH_FACTOR: f64 = 0.2;

/// Minimum jail duration (days).
pub const PENALTY_JAIL_MIN_DAYS: u64 = 7;

/// Maximum jail duration (days).
pub const PENALTY_JAIL_MAX_DAYS: u64 = 30;

/// 2 Tier 3 within 1 year → escalate to Tier 4.
pub const TIER3_TO_TIER4_COUNT: u32 = 2;

/// Near-zero trust floor (Tier 4). Not permanently banned.
pub const MIN_TRUST: f64 = 0.001;

/// 6-month ban for Tier 4, restart as Leaf after.
pub const PENALTY_TRUST_ZERO_BAN_DAYS: u64 = 180;

/// Permanent zero trust (Tier 5 — Tombstone).
pub const TIER5_TRUST: f64 = 0.0;

// ─── Correlation Penalty ───

/// multiplier = 1 + log₂(n). Base for isolated incident.
pub const CORRELATION_PENALTY_BASE: f64 = 1.0;

// ─── Appeal Windows ───

/// L2: Time for accused to submit counter-evidence before penalty executes (hours).
pub const DISPUTE_WINDOW_HOURS: u32 = 48;

/// L3: Time to file retrospective appeal after penalty execution (days).
pub const RETROSPECTIVE_WINDOW_DAYS: u32 = 30;

/// Successful appeal restores trust × 0.7 (30% permanent scar).
pub const APPEAL_TRUST_SCAR: f64 = 0.30;

/// L4: >80% of top-tier nodes must agree to review Tombstone.
pub const TOMBSTONE_APPEAL_THRESHOLD: f64 = 0.80;

/// L1: Pre-penalty check needs ≥2 antibody types.
pub const AUTO_PROTECTION_MIN_ANTIBODIES: u32 = 2;

/// L1: Combined antibody confidence must exceed 0.7.
pub const AUTO_PROTECTION_MIN_CONFIDENCE: f64 = 0.70;

// ═══════════════════════════════════════════════════════════════════════════
// §9.7 — Transfer & Wire Protocol
// ═══════════════════════════════════════════════════════════════════════════

// ─── Message Type Codes ───

/// Send OBT: from, to, amount, nonce, signature.
pub const MSG_OBT_TRANSFER_REQUEST: u8 = 0xA0;
/// Witness confirmation: tx_id, witness_signature.
pub const MSG_OBT_TRANSFER_CONFIRM: u8 = 0xA1;
/// Query balance of a node_id.
pub const MSG_OBT_BALANCE_QUERY: u8 = 0xA2;
/// Response: node_id, balance, head_hash, Merkle proof.
pub const MSG_OBT_BALANCE_RESPONSE: u8 = 0xA3;
/// Broadcast signed MintProof to network.
pub const MSG_OBT_MINT_BROADCAST: u8 = 0xA4;
/// PoS-KU challenge: ku_cid, challenge_type, params.
pub const MSG_OBT_STORAGE_CHALLENGE: u8 = 0xA5;

// ─── Account-Chain Constants ───

/// Genesis block has all-zero previous hash.
pub const GENESIS_BLOCK_PREVIOUS: [u8; 32] = [0u8; 32];

/// Account state replicated to K=20 DHT nodes (same as KU replication).
pub const DHT_ACCOUNT_STATE_K: u32 = 20;

/// Max retries for transfer confirmation.
pub const TRANSFER_MAX_RETRIES: u8 = 3;

/// Unreceived Send blocks expire after 7 days (seconds).
pub const UNRECEIVED_SEND_EXPIRY_S: u64 = 7 * 24 * 3_600;

/// Number of random PoS-KU storage challenges per epoch.
pub const STORAGE_CHALLENGE_COUNT: u8 = 10;

/// Timeout for each storage challenge response (seconds).
pub const STORAGE_CHALLENGE_TIMEOUT_S: u64 = 30;

/// Per-node per-epoch cap for storage reward (OBT). Alias for consistency.
pub const MAX_STORAGE_REWARD_PER_NODE: u64 = 10;

/// Grace period in seconds for late events at epoch boundary.
pub const EPOCH_GRACE_PERIOD_S: u64 = 30;

// ═══════════════════════════════════════════════════════════════════════════
// §9.8 — Gaming Pattern Detection
// ═══════════════════════════════════════════════════════════════════════════

/// Same as GOSSIP_GAP_WINDOW_S. ≥3 nodes offline/online within 30s.
pub const ISOLATION_PATTERN_WINDOW_S: u64 = 30;

/// If rate > 2× tier limit → burst spam detected.
pub const BURST_SPAM_RATE_MULTIPLIER: f64 = 2.0;

/// KU content similarity > 0.8 between burst submissions = spam.
pub const BURST_SPAM_SIMILARITY_THRESHOLD: f64 = 0.8;

/// A→B→C→A within 1 epoch = wash trading suspected.
pub const CIRCULAR_TRANSFER_WINDOW_EPOCHS: u64 = 1;

/// High trust but KU quality divergence > 0.3 = trust farming suspected.
pub const LONG_CON_DIVERGENCE_THRESHOLD: f64 = 0.3;

// ═══════════════════════════════════════════════════════════════════════════
// §9.9 — Confirmation Levels
// ═══════════════════════════════════════════════════════════════════════════

/// Just created, no confirmations.
pub const LEVEL_PENDING: u8 = 0;
/// 1–2 witnesses confirmed.
pub const LEVEL_TENTATIVE: u8 = 1;
/// K witnesses confirmed (K=3–7).
pub const LEVEL_CONFIRMED: u8 = 2;
/// Widely propagated, practically irreversible.
pub const LEVEL_SETTLED: u8 = 3;
/// Mint requires CONFIRMED+.
pub const MIN_LEVEL_FOR_MINT: u8 = LEVEL_CONFIRMED;
/// Transfer requires CONFIRMED+.
pub const MIN_LEVEL_FOR_TRANSFER: u8 = LEVEL_CONFIRMED;

// ═══════════════════════════════════════════════════════════════════════════
// Helper Functions — Tier-Based Lookups
// ═══════════════════════════════════════════════════════════════════════════

/// Get the trust multiplier for a given node tier (0..=6).
///
/// Returns the multiplier that caps per-node rewards relative to the
/// fair share `E(epoch) / active_nodes`.
///
/// # Panics
/// Panics if `tier > 6`.
pub fn trust_multiplier_for_tier(tier: u8) -> f64 {
    assert!(tier <= 6, "NodeTier must be 0..=6, got {}", tier);
    TRUST_MULTIPLIER[tier as usize]
}

/// Get the max KU creations per hour for a given node tier.
///
/// Tiers 0=Leaf, 1=Contributor, 2+=LocalSP and above.
pub fn max_ku_per_hour_for_tier(tier: u8) -> u32 {
    match tier {
        0 => MAX_KU_PER_HOUR_LEAF,
        1 => MAX_KU_PER_HOUR_CONTRIBUTOR,
        _ => MAX_KU_PER_HOUR_LOCALSP, // T2+ all share the highest limit
    }
}

/// Get the max encoding claims per hour for a given node tier.
pub fn max_encodings_per_hour_for_tier(tier: u8) -> u32 {
    match tier {
        0 => MAX_ENCODINGS_PER_HOUR_LEAF,
        1 => MAX_ENCODINGS_PER_HOUR_CONTRIBUTOR,
        _ => MAX_ENCODINGS_PER_HOUR_LOCALSP,
    }
}

/// Get the encoding claim cooldown in seconds for a given node tier.
pub fn cooldown_for_tier(tier: u8) -> u64 {
    match tier {
        0 => COOLDOWN_LEAF_S,
        1 => COOLDOWN_CONTRIBUTOR_S,
        _ => COOLDOWN_LOCALSP_S,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// NodeTier Enum (§5.1)
// ═══════════════════════════════════════════════════════════════════════════

/// 7-tier node hierarchy. Trust-gated promotions.
///
/// Each tier has a TierWeight for Effective Trust and a TrustMultiplier for rewards.
/// Promotion requires EigenTrust ≥ promotion threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum NodeTier {
    Leaf = 0,
    Contributor = 1,
    LocalSP = 2,
    RegionalSP = 3,
    CountrySP = 4,
    ContinentalSP = 5,
    GlobalBackbone = 6,
}

impl NodeTier {
    /// Promotion threshold (minimum EigenTrust score to reach this tier).
    pub fn promotion_threshold(&self) -> f64 {
        match self {
            Self::Leaf => 0.00,
            Self::Contributor => 0.30,
            Self::LocalSP => 0.60,
            Self::RegionalSP => 0.75,
            Self::CountrySP => 0.85,
            Self::ContinentalSP => 0.92,
            Self::GlobalBackbone => 0.97,
        }
    }

    /// TierWeight for Effective Trust calculation.
    pub fn tier_weight(&self) -> f64 {
        TRUST_MULTIPLIER[*self as usize]
    }

    /// Convert from u8. Returns Leaf for unknown values (safe default).
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Leaf,
            1 => Self::Contributor,
            2 => Self::LocalSP,
            3 => Self::RegionalSP,
            4 => Self::CountrySP,
            5 => Self::ContinentalSP,
            6 => Self::GlobalBackbone,
            _ => Self::Leaf, // Safe default
        }
    }
}

impl std::fmt::Display for NodeTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Leaf => write!(f, "Leaf"),
            Self::Contributor => write!(f, "Contributor"),
            Self::LocalSP => write!(f, "LocalSP"),
            Self::RegionalSP => write!(f, "RegionalSP"),
            Self::CountrySP => write!(f, "CountrySP"),
            Self::ContinentalSP => write!(f, "ContinentalSP"),
            Self::GlobalBackbone => write!(f, "GlobalBackbone"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Aliases (backward compatibility — prefer canonical names above)
// ═══════════════════════════════════════════════════════════════════════════

/// Alias for `ACTIVITY_MULTIPLIER_TARGET`. Prefer the canonical name.
#[deprecated(note = "Use ACTIVITY_MULTIPLIER_TARGET instead")]
pub const ACTIVITY_SCALE_DENOMINATOR: u64 = ACTIVITY_MULTIPLIER_TARGET;
/// Alias for `ACTIVITY_MULTIPLIER_MAX`. Prefer the canonical name.
#[deprecated(note = "Use ACTIVITY_MULTIPLIER_MAX instead")]
pub const ACTIVITY_SCALE_MAX: f64 = ACTIVITY_MULTIPLIER_MAX;
/// Alias for `TRUST_MULTIPLIER`. Prefer `NodeTier::tier_weight()`.
#[deprecated(note = "Use TRUST_MULTIPLIER or NodeTier::tier_weight() instead")]
pub const TIER_TRUST_MULTIPLIERS: [f64; 7] = TRUST_MULTIPLIER;

/// Alias for `STORAGE_SIZE_WEIGHT_MIN`. Prefer the canonical name.
#[deprecated(note = "Use STORAGE_SIZE_WEIGHT_MIN instead")]
pub const SIZE_WEIGHT_MIN: f64 = STORAGE_SIZE_WEIGHT_MIN;
/// Alias for `STORAGE_SIZE_WEIGHT_MAX`. Prefer the canonical name.
#[deprecated(note = "Use STORAGE_SIZE_WEIGHT_MAX instead")]
pub const SIZE_WEIGHT_MAX: f64 = STORAGE_SIZE_WEIGHT_MAX;
/// Alias for `STORAGE_RARITY_WEIGHT_MIN`. Prefer the canonical name.
#[deprecated(note = "Use STORAGE_RARITY_WEIGHT_MIN instead")]
pub const RARITY_WEIGHT_MIN: f64 = STORAGE_RARITY_WEIGHT_MIN;
/// Alias for `STORAGE_RARITY_WEIGHT_MAX`. Prefer the canonical name.
#[deprecated(note = "Use STORAGE_RARITY_WEIGHT_MAX instead")]
pub const RARITY_WEIGHT_MAX: f64 = STORAGE_RARITY_WEIGHT_MAX;
/// Alias for `STORAGE_DEMAND_WEIGHT_MIN`. Prefer the canonical name.
#[deprecated(note = "Use STORAGE_DEMAND_WEIGHT_MIN instead")]
pub const DEMAND_WEIGHT_MIN: f64 = STORAGE_DEMAND_WEIGHT_MIN;
/// Alias for `STORAGE_DEMAND_WEIGHT_MAX`. Prefer the canonical name.
#[deprecated(note = "Use STORAGE_DEMAND_WEIGHT_MAX instead")]
pub const DEMAND_WEIGHT_MAX: f64 = STORAGE_DEMAND_WEIGHT_MAX;
/// Alias for `STORAGE_DURATION_FACTOR_MAX`. Prefer the canonical name.
#[deprecated(note = "Use STORAGE_DURATION_FACTOR_MAX instead")]
pub const DURATION_FACTOR_MAX: f64 = STORAGE_DURATION_FACTOR_MAX;

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_epoch_derived_constants() {
        assert_eq!(
            OBT_EPOCH_DURATION_S * EPOCHS_PER_DAY,
            86_400,
            "1 day in seconds"
        );
        assert_eq!(EPOCHS_PER_WEEK, EPOCHS_PER_DAY * 7);
    }

    #[test]
    fn test_stream_weights_sum_to_one() {
        let sum: f64 = STREAM_WEIGHTS.iter().sum();
        assert!(
            (sum - 1.0).abs() < 1e-10,
            "Stream weights must sum to 1.0, got {sum}"
        );
    }

    #[test]
    fn test_trust_multiplier_array() {
        assert_eq!(TRUST_MULTIPLIER.len(), 7);
        assert!((TRUST_MULTIPLIER[0] - 0.10).abs() < f64::EPSILON);
        assert!((TRUST_MULTIPLIER[2] - 1.00).abs() < f64::EPSILON);
        assert!((TRUST_MULTIPLIER[6] - 2.00).abs() < f64::EPSILON);
        // Multipliers must be monotonically increasing
        for i in 1..TRUST_MULTIPLIER.len() {
            assert!(
                TRUST_MULTIPLIER[i] >= TRUST_MULTIPLIER[i - 1],
                "Trust multipliers must be non-decreasing"
            );
        }
    }

    #[test]
    fn test_trust_multiplier_for_tier() {
        assert!((trust_multiplier_for_tier(0) - 0.10).abs() < f64::EPSILON);
        assert!((trust_multiplier_for_tier(1) - 0.50).abs() < f64::EPSILON);
        assert!((trust_multiplier_for_tier(2) - 1.00).abs() < f64::EPSILON);
        assert!((trust_multiplier_for_tier(6) - 2.00).abs() < f64::EPSILON);
    }

    #[test]
    #[should_panic(expected = "NodeTier must be 0..=6")]
    fn test_trust_multiplier_invalid_tier() {
        trust_multiplier_for_tier(7);
    }

    #[test]
    fn test_max_ku_per_hour_for_tier() {
        assert_eq!(max_ku_per_hour_for_tier(0), 1);
        assert_eq!(max_ku_per_hour_for_tier(1), 5);
        assert_eq!(max_ku_per_hour_for_tier(2), 10);
        assert_eq!(max_ku_per_hour_for_tier(5), 10, "T5 should use T2+ limit");
    }

    #[test]
    fn test_max_encodings_per_hour_for_tier() {
        assert_eq!(max_encodings_per_hour_for_tier(0), 2);
        assert_eq!(max_encodings_per_hour_for_tier(1), 5);
        assert_eq!(max_encodings_per_hour_for_tier(2), 10);
    }

    #[test]
    fn test_cooldown_for_tier() {
        assert_eq!(cooldown_for_tier(0), 3_600); // 60 minutes
        assert_eq!(cooldown_for_tier(1), 720); // 12 minutes
        assert_eq!(cooldown_for_tier(2), 360); // 6 minutes
        assert_eq!(cooldown_for_tier(3), 360); // T3+ uses T2 cooldown
    }

    #[test]
    fn test_message_type_codes_are_unique() {
        let codes = [
            MSG_OBT_TRANSFER_REQUEST,
            MSG_OBT_TRANSFER_CONFIRM,
            MSG_OBT_BALANCE_QUERY,
            MSG_OBT_BALANCE_RESPONSE,
            MSG_OBT_MINT_BROADCAST,
            MSG_OBT_STORAGE_CHALLENGE,
        ];
        for i in 0..codes.len() {
            for j in (i + 1)..codes.len() {
                assert_ne!(codes[i], codes[j], "Message type codes must be unique");
            }
        }
    }

    #[test]
    fn test_confirmation_levels_ordered() {
        const {
            assert!(LEVEL_PENDING < LEVEL_TENTATIVE);
            assert!(LEVEL_TENTATIVE < LEVEL_CONFIRMED);
            assert!(LEVEL_CONFIRMED < LEVEL_SETTLED);
        }
        assert_eq!(MIN_LEVEL_FOR_MINT, LEVEL_CONFIRMED);
        assert_eq!(MIN_LEVEL_FOR_TRANSFER, LEVEL_CONFIRMED);
    }

    #[test]
    fn test_penalty_jail_range() {
        const {
            assert!(PENALTY_JAIL_MIN_DAYS <= PENALTY_JAIL_MAX_DAYS);
            assert!(PENALTY_JAIL_MIN_DAYS > 0);
        }
    }

    #[test]
    fn test_genesis_block_previous_is_zeroed() {
        assert_eq!(GENESIS_BLOCK_PREVIOUS, [0u8; 32]);
    }

    #[test]
    fn test_quality_gates_non_zero() {
        const {
            assert!(MIN_KU_RAW_SIZE > 0);
            assert!(MIN_GENE_COUNT > 0);
            assert!(MIN_POMV_7D > 0.0);
            assert!(MIN_POMV_30D > MIN_POMV_7D);
        }
    }

    #[test]
    fn test_unreceived_send_expiry() {
        assert_eq!(UNRECEIVED_SEND_EXPIRY_S, 604_800, "7 days in seconds");
    }

    #[test]
    fn test_tier_multipliers_monotonic() {
        for i in 1..NODE_TIER_COUNT {
            assert!(
                TRUST_MULTIPLIER[i] >= TRUST_MULTIPLIER[i - 1],
                "Tier multipliers must be non-decreasing: tier {} ({}) < tier {} ({})",
                i,
                TRUST_MULTIPLIER[i],
                i - 1,
                TRUST_MULTIPLIER[i - 1],
            );
        }
    }

    #[test]
    fn test_epoch_timing_consistency() {
        assert_eq!(86_400 / OBT_EPOCH_DURATION_S, EPOCHS_PER_DAY);
    }
}
