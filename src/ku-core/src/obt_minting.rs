//! # OBT Minting Model
//!
//! Minting is the OUTPUT of consensus, not INPUT.
//! No participant "requests" minting — it occurs as a deterministic consequence
//! of verified work observed by the network during an epoch.
//!
//! ## Global Emission Formula
//! ```text
//! E(epoch) = B × A(epoch) × Q(epoch)
//! ```
//!
//! Where:
//! - `B` = `BASE_EMISSION_PER_EPOCH` (10,000 OBT/epoch, governance-adjustable)
//! - `A` = `min(active_nodes / 1000, 10.0)` — network activity scale
//! - `Q` = average PoMV score across all KUs
//!
//! ## Four Reward Streams (R1–R4)
//! - R1: Owner reward (PoMV-based) — 40%
//! - R2: Encoder reward (encoding work) — 25%
//! - R3: Verifier reward (verification work) — 15%
//! - R4: Storage reward (DHT storage) — 20%
//!
//! ## Reference
//! See `docs/specs/obt/03_MINTING.md`.

use serde::{Deserialize, Serialize};

use crate::crdt::VectorClock;
use crate::obt_constants::*;

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/// Cryptographic proof that OBT was legitimately minted.
///
/// Every minted OBT batch is accompanied by a `MintProof` — a self-contained,
/// independently verifiable proof. Any node can validate by:
/// 1. Recomputing rewards from formula inputs
/// 2. Verifying witness signatures (K=3)
/// 3. Confirming `obt_amount ≤ per-node cap`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintProof {
    /// What activity generated this mint.
    pub activity: MintActivity,

    /// CID of the KU involved (BLAKE3 hash).
    pub ku_cid: [u8; 32],

    /// Total OBT minted (integer, smallest unit).
    pub obt_amount: u64,

    /// Inputs to the minting formula (for independent re-verification).
    pub formula_inputs: FormulaInputs,

    /// Epoch number this mint belongs to.
    pub epoch: u64,

    /// Ed25519 public key of the recipient node.
    pub recipient: [u8; 32],

    /// Witness attestations (K=3 random witnesses).
    pub witnesses: Vec<WitnessSignature>,

    /// Causal clock snapshot (VectorClock from crdt module).
    pub clock: VectorClock,

    /// Unix timestamp (seconds) when this proof was created.
    pub timestamp: u64,
}

/// Activity type that triggered the mint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MintActivity {
    /// R2: Encoder reward — OBT for encoding raw text into KU.
    Encoding,
    /// R3: Verifier reward — OBT for verifying encoding quality.
    Verification,
    /// R1: Owner/PoMV reward — OBT for contributing knowledge (PoMV-based).
    PomvReward,
    /// R4: Storage reward — OBT for storing KUs on DHT.
    StorageReward,
}

impl MintActivity {
    /// Human-readable name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Encoding => "Encoding",
            Self::Verification => "Verification",
            Self::PomvReward => "PoMV Reward",
            Self::StorageReward => "Storage Reward",
        }
    }
}

impl std::fmt::Display for MintActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Inputs to the minting formula, stored for independent re-verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormulaInputs {
    /// Raw size of the KU in kilobytes.
    pub raw_size_kb: f64,

    /// Multiplier for the participant's role.
    pub role_multiplier: f64,

    /// Composite PoMV score [0.0, 1.0].
    pub pomv_score: f32,

    /// Storage-specific factors (only present for R4 activity).
    pub storage_factors: Option<StorageFactors>,
}

/// Storage-specific weighting factors for R4 mint verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageFactors {
    /// Size weight [0.1, 10.0].
    pub size_weight: f64,
    /// Rarity weight [0.5, 3.0].
    pub rarity_weight: f64,
    /// Demand weight [0.1, 5.0].
    pub demand_weight: f64,
    /// Duration factor [0.0, 2.0].
    pub duration_factor: f64,
    /// Trust factor [0.0, 1.0].
    pub trust_factor: f64,
}

/// Witness attestation signature for a MintProof.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WitnessSignature {
    /// Ed25519 public key of the witness node.
    pub witness_id: [u8; 32],
    /// Ed25519 signature over `BLAKE3(epoch || recipient || obt_amount || ku_cid)`.
    /// Stored as Vec<u8> (64 bytes) for serde compatibility (arrays >32 unsupported).
    #[serde(with = "serde_bytes")]
    pub signature: Vec<u8>,
}

/// Type alias for backward compatibility — CausalClock is just a VectorClock.
pub type CausalClock = VectorClock;

// ═══════════════════════════════════════════════════════════════════════════
// Global Emission
// ═══════════════════════════════════════════════════════════════════════════

/// Calculate global emission for an epoch.
///
/// ```text
/// E(epoch) = B × A(epoch) × Q(epoch)
///
/// A = min(active_nodes / 1000, 10.0)
/// Q = avg_pomv_score  ∈ [0.0, 1.0]
/// ```
///
/// # Examples
///
/// | active_nodes | avg_pomv | E(epoch)  |
/// |-------------|----------|-----------|
/// | 100         | 0.50     | 500       |
/// | 1,000       | 0.70     | 7,000     |
/// | 10,000      | 0.90     | 90,000    |
/// | 50,000      | 0.95     | 95,000    |
pub fn compute_epoch_emission(active_nodes: u32, avg_pomv_score: f64) -> u64 {
    let a = (active_nodes as f64 / ACTIVITY_MULTIPLIER_TARGET as f64).min(ACTIVITY_MULTIPLIER_MAX);
    let q = avg_pomv_score.clamp(0.0, 1.0);

    (BASE_EMISSION_PER_EPOCH as f64 * a * q) as u64
}

// ═══════════════════════════════════════════════════════════════════════════
// Per-Node Reward Cap
// ═══════════════════════════════════════════════════════════════════════════

/// Return the trust multiplier for a node tier.
fn trust_multiplier(tier: NodeTier) -> f64 {
    tier.tier_weight()
}

/// Calculate per-node reward cap for an epoch.
///
/// ```text
/// max_node_reward = E(epoch) / active_nodes × TrustMultiplier(tier)
/// ```
///
/// No single node can capture a disproportionate share of emission.
/// - Leaf (tier 0): 10% of fair share
/// - GlobalBackbone (tier 6): 200% of fair share
pub fn compute_node_reward_cap(epoch_emission: u64, active_nodes: u32, tier: NodeTier) -> u64 {
    if active_nodes == 0 {
        return 0;
    }
    let fair_share = epoch_emission as f64 / active_nodes as f64;
    (fair_share * trust_multiplier(tier)) as u64
}

// ═══════════════════════════════════════════════════════════════════════════
// R1 — Owner Reward (PoMV-based)
// ═══════════════════════════════════════════════════════════════════════════

/// Calculate R1 (owner) reward for a KU.
///
/// ```text
/// R1(owner, epoch) = pomv_score × max_reward_per_epoch
/// ```
///
/// Where `max_reward_per_epoch = R1_budget / active_ku_count`.
/// `pomv_score` ∈ [0.0, 1.0] — composite from 6 weighted signals.
pub fn compute_owner_reward(pomv_score: f32, max_reward_per_epoch: f64) -> u64 {
    let reward = pomv_score as f64 * max_reward_per_epoch;
    reward.max(0.0) as u64
}

// ═══════════════════════════════════════════════════════════════════════════
// R2 — Encoder Reward
// ═══════════════════════════════════════════════════════════════════════════

/// Calculate R2 (encoder) reward for encoding work on a KU.
///
/// Uses the same formula engine as `encoding_reward.rs`:
///
/// | Role | Multiplier |
/// |------|-----------|
/// | 0 (Contributor)    | 0 (paid via PoMV)     |
/// | 1 (FirstEncoder)   | base×2 + FIRST_ENCODER_BONUS |
/// | 2 (Verifier)       | base                  |
/// | 3 (Corrector)      | base × CORRECTOR_MULTIPLIER |
/// | 4 (ProBono)        | base×2 + PRO_BONO_BONUS |
pub fn compute_encoding_reward(raw_size_kb: f64, role: u8) -> u64 {
    let base = (raw_size_kb as u64).max(1) * BASE_OBT_PER_KB;

    match role {
        0 => 0,                              // Contributor — paid through PoMV
        1 => base * 2 + FIRST_ENCODER_BONUS, // FirstEncoder
        2 => base,                           // Verifier
        3 => base * CORRECTOR_MULTIPLIER,    // Corrector
        4 => base * 2 + PRO_BONO_BONUS,      // ProBono
        _ => 0,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// R3 — Verifier Reward
// ═══════════════════════════════════════════════════════════════════════════

/// Calculate R3 (verifier) reward for verification work.
///
/// ```text
/// base_reward = max(raw_size_kb, 1) × BASE_OBT_PER_KB
/// selection_bonus = base_reward / 2  (if encoding was selected)
/// ```
///
/// # Arguments
/// * `raw_size_kb` — Raw KU size in kilobytes
/// * `role` — 0 = verifier (not selected), 1 = verifier (selected)
pub fn compute_verification_reward(raw_size_kb: f64, role: u8) -> u64 {
    let base = (raw_size_kb as u64).max(1) * BASE_OBT_PER_KB;
    let selection_bonus = if role >= 1 { base / 2 } else { 0 };
    base + selection_bonus
}

// ═══════════════════════════════════════════════════════════════════════════
// Stream Budget
// ═══════════════════════════════════════════════════════════════════════════

/// Compute the budget for a specific reward stream.
///
/// # Arguments
/// * `epoch_emission` — Total emission for the epoch
/// * `stream_idx` — 0=R1(Owner), 1=R2(Encoder), 2=R3(Verifier), 3=R4(Storage)
pub fn compute_stream_budget(epoch_emission: u64, stream_idx: usize) -> u64 {
    if stream_idx >= STREAM_WEIGHTS.len() {
        return 0;
    }
    (epoch_emission as f64 * STREAM_WEIGHTS[stream_idx]) as u64
}

// ═══════════════════════════════════════════════════════════════════════════
// Verification
// ═══════════════════════════════════════════════════════════════════════════

/// Verify a MintProof by re-computing the formula and checking consistency.
///
/// This performs the **deterministic** checks:
/// 1. Re-derive the reward amount from `formula_inputs`
/// 2. Compare against the claimed `obt_amount`
/// 3. Check minimum witness count
///
/// > **Note**: Signature verification requires Ed25519 public keys and is
/// > NOT performed here — that must be done by the networking layer.
pub fn verify_mint_proof(proof: &MintProof) -> bool {
    // 1. Re-compute reward from formula inputs
    let recomputed = match proof.activity {
        MintActivity::Encoding => compute_encoding_reward(
            proof.formula_inputs.raw_size_kb,
            proof.formula_inputs.role_multiplier as u8,
        ),
        MintActivity::Verification => compute_verification_reward(
            proof.formula_inputs.raw_size_kb,
            proof.formula_inputs.role_multiplier as u8,
        ),
        MintActivity::PomvReward => {
            // For PoMV: pomv_score × (role_multiplier serves as max_reward_per_epoch)
            compute_owner_reward(
                proof.formula_inputs.pomv_score,
                proof.formula_inputs.role_multiplier,
            )
        }
        MintActivity::StorageReward => {
            // Storage rewards are verified via the storage module;
            // here we only check that storage_factors are present
            if proof.formula_inputs.storage_factors.is_none() {
                return false;
            }
            let sf = proof.formula_inputs.storage_factors.as_ref().unwrap();
            let reward = STORAGE_BASE_RATE
                * sf.size_weight
                * sf.rarity_weight
                * sf.demand_weight
                * sf.duration_factor
                * sf.trust_factor;
            reward as u64
        }
    };

    // 2. Amount must match
    if recomputed != proof.obt_amount {
        return false;
    }

    // 3. Minimum witness count
    if proof.witnesses.len() < MIN_WITNESSES as usize {
        return false;
    }

    true
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── Epoch Emission ─────────────────────────────────────────────────

    #[test]
    fn test_epoch_emission_early_network() {
        // 100 nodes, avg_pomv=0.5 → A=0.1, Q=0.5 → 10_000_000*0.1*0.5 = 500_000 milliOBT
        let emission = compute_epoch_emission(100, 0.50);
        assert_eq!(emission, 500_000);
    }

    #[test]
    fn test_epoch_emission_baseline() {
        // 1000 nodes, avg_pomv=0.7 → A=1.0, Q=0.7 → 10_000_000*1.0*0.7 = 7_000_000 milliOBT
        let emission = compute_epoch_emission(1000, 0.70);
        assert_eq!(emission, 7_000_000);
    }

    #[test]
    fn test_epoch_emission_at_scale() {
        // 10000 nodes, avg_pomv=0.9 → A=10.0, Q=0.9 → 10_000_000*10*0.9 = 90_000_000 milliOBT
        let emission = compute_epoch_emission(10000, 0.90);
        assert_eq!(emission, 90_000_000);
    }

    #[test]
    fn test_epoch_emission_capped() {
        // 50000 nodes → A is capped at 10.0 → 10_000_000*10*0.95 = 95_000_000 milliOBT
        let emission = compute_epoch_emission(50000, 0.95);
        assert_eq!(emission, 95_000_000);
    }

    #[test]
    fn test_epoch_emission_zero_nodes() {
        let emission = compute_epoch_emission(0, 0.5);
        assert_eq!(emission, 0);
    }

    #[test]
    fn test_epoch_emission_zero_quality() {
        let emission = compute_epoch_emission(1000, 0.0);
        assert_eq!(emission, 0);
    }

    // ── Per-Node Reward Cap ──────────────────────────────────────────

    #[test]
    fn test_node_reward_cap_leaf() {
        // 7_000_000 milliOBT emission, 1000 nodes, Leaf(0) → 7000*0.10 = 700 milliOBT
        let cap = compute_node_reward_cap(7_000_000, 1000, NodeTier::Leaf);
        assert_eq!(cap, 700);
    }

    #[test]
    fn test_node_reward_cap_local_sp() {
        // 7_000_000 milliOBT, 1000 nodes, LocalSP(2) → 7000*1.0 = 7000 milliOBT
        let cap = compute_node_reward_cap(7_000_000, 1000, NodeTier::LocalSP);
        assert_eq!(cap, 7000);
    }

    #[test]
    fn test_node_reward_cap_global_backbone() {
        // 7_000_000 milliOBT, 1000 nodes, GlobalBackbone(6) → 7000*2.0 = 14000 milliOBT
        let cap = compute_node_reward_cap(7_000_000, 1000, NodeTier::GlobalBackbone);
        assert_eq!(cap, 14000);
    }

    #[test]
    fn test_node_reward_cap_zero_nodes() {
        let cap = compute_node_reward_cap(7000, 0, NodeTier::LocalSP);
        assert_eq!(cap, 0);
    }

    // ── R1: Owner Reward ─────────────────────────────────────────────

    #[test]
    fn test_owner_reward() {
        // pomv=0.8, max_reward=100 → 80
        let reward = compute_owner_reward(0.8, 100.0);
        assert_eq!(reward, 80);
    }

    #[test]
    fn test_owner_reward_zero_pomv() {
        let reward = compute_owner_reward(0.0, 100.0);
        assert_eq!(reward, 0);
    }

    // ── R2: Encoder Reward ───────────────────────────────────────────

    #[test]
    fn test_encoding_reward_contributor() {
        assert_eq!(compute_encoding_reward(2.0, 0), 0);
    }

    #[test]
    fn test_encoding_reward_first_encoder() {
        // 2KB: base=2, FirstEncoder → 2*2+5 = 9
        assert_eq!(compute_encoding_reward(2.0, 1), 9);
    }

    #[test]
    fn test_encoding_reward_corrector() {
        // 2KB: base=2, Corrector → 2*3 = 6
        assert_eq!(compute_encoding_reward(2.0, 3), 6);
    }

    #[test]
    fn test_encoding_reward_pro_bono() {
        // 2KB: base=2, ProBono → 2*2+10 = 14
        assert_eq!(compute_encoding_reward(2.0, 4), 14);
    }

    // ── R3: Verifier Reward ──────────────────────────────────────────

    #[test]
    fn test_verification_reward_not_selected() {
        // 2KB: base=2, not selected → 2
        assert_eq!(compute_verification_reward(2.0, 0), 2);
    }

    #[test]
    fn test_verification_reward_selected() {
        // 2KB: base=2, selected → 2 + 1 = 3
        assert_eq!(compute_verification_reward(2.0, 1), 3);
    }

    // ── Stream Budget ────────────────────────────────────────────────

    #[test]
    fn test_stream_budget() {
        let emission = 10_000;
        assert_eq!(compute_stream_budget(emission, 0), 4000); // R1: 40%
        assert_eq!(compute_stream_budget(emission, 1), 2500); // R2: 25%
        assert_eq!(compute_stream_budget(emission, 2), 1500); // R3: 15%
        assert_eq!(compute_stream_budget(emission, 3), 2000); // R4: 20%
    }

    #[test]
    fn test_stream_budget_invalid_index() {
        assert_eq!(compute_stream_budget(10_000, 99), 0);
    }

    // ── MintProof Verification ───────────────────────────────────────

    fn make_dummy_witness() -> WitnessSignature {
        WitnessSignature {
            witness_id: [0u8; 32],
            signature: vec![0u8; 64],
        }
    }

    #[test]
    fn test_verify_encoding_proof() {
        let proof = MintProof {
            activity: MintActivity::Encoding,
            ku_cid: [1u8; 32],
            obt_amount: 9, // FirstEncoder, 2KB → 9
            formula_inputs: FormulaInputs {
                raw_size_kb: 2.0,
                role_multiplier: 1.0, // FirstEncoder
                pomv_score: 0.0,
                storage_factors: None,
            },
            epoch: 42,
            recipient: [2u8; 32],
            witnesses: vec![make_dummy_witness(); 3],
            clock: CausalClock::default(),
            timestamp: 1_700_000_000,
        };
        assert!(verify_mint_proof(&proof));
    }

    #[test]
    fn test_verify_proof_bad_amount() {
        let proof = MintProof {
            activity: MintActivity::Encoding,
            ku_cid: [1u8; 32],
            obt_amount: 999, // Wrong!
            formula_inputs: FormulaInputs {
                raw_size_kb: 2.0,
                role_multiplier: 1.0,
                pomv_score: 0.0,
                storage_factors: None,
            },
            epoch: 42,
            recipient: [2u8; 32],
            witnesses: vec![make_dummy_witness(); 3],
            clock: CausalClock::default(),
            timestamp: 1_700_000_000,
        };
        assert!(!verify_mint_proof(&proof));
    }

    #[test]
    fn test_verify_proof_insufficient_witnesses() {
        let proof = MintProof {
            activity: MintActivity::Encoding,
            ku_cid: [1u8; 32],
            obt_amount: 9,
            formula_inputs: FormulaInputs {
                raw_size_kb: 2.0,
                role_multiplier: 1.0,
                pomv_score: 0.0,
                storage_factors: None,
            },
            epoch: 42,
            recipient: [2u8; 32],
            witnesses: vec![make_dummy_witness(); 2], // Only 2 — insufficient
            clock: CausalClock::default(),
            timestamp: 1_700_000_000,
        };
        assert!(!verify_mint_proof(&proof));
    }

    #[test]
    fn test_verify_pomv_proof() {
        // pomv_score=0.5, max_reward(role_multiplier)=100 → 50
        let proof = MintProof {
            activity: MintActivity::PomvReward,
            ku_cid: [1u8; 32],
            obt_amount: 50,
            formula_inputs: FormulaInputs {
                raw_size_kb: 0.0,
                role_multiplier: 100.0,
                pomv_score: 0.5,
                storage_factors: None,
            },
            epoch: 10,
            recipient: [3u8; 32],
            witnesses: vec![make_dummy_witness(); 3],
            clock: CausalClock::default(),
            timestamp: 1_700_000_000,
        };
        assert!(verify_mint_proof(&proof));
    }

    #[test]
    fn test_verify_storage_proof_missing_factors() {
        let proof = MintProof {
            activity: MintActivity::StorageReward,
            ku_cid: [1u8; 32],
            obt_amount: 0,
            formula_inputs: FormulaInputs {
                raw_size_kb: 0.0,
                role_multiplier: 0.0,
                pomv_score: 0.0,
                storage_factors: None, // Missing!
            },
            epoch: 10,
            recipient: [3u8; 32],
            witnesses: vec![make_dummy_witness(); 3],
            clock: CausalClock::default(),
            timestamp: 1_700_000_000,
        };
        assert!(!verify_mint_proof(&proof));
    }
}
