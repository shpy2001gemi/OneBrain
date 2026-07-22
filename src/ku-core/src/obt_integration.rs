//! # OBT ↔ KU Integration Layer
//!
//! Bridges Knowledge Unit data structures with OBT reward computation.
//! Provides builder functions and quality gate orchestration.
//!
//! ## Integration Points
//! - `KuRuntime` → `FormulaInputs` (minting)
//! - `KuRuntime` → `StoredKuInfo` (storage rewards)
//! - `EncodingConsensus` → quality gates (anti-gaming)
//! - `KUMetabolism` → epoch accumulation
//!
//! ## Reference
//! See `docs/specs/obt/03_MINTING.md` and `docs/specs/obt/04_STORAGE_REWARD.md`.

use crate::crdt::VectorClock;
use crate::encoding_consensus::EncodingConsensus;
use crate::ku_runtime::KuRuntime;
use crate::obt_anti_gaming;
use crate::obt_epoch::epoch_from_timestamp;
use crate::obt_minting::{FormulaInputs, MintActivity, MintProof, StorageFactors};
use crate::obt_storage_reward::StoredKuInfo;
use std::collections::HashSet;

// ═══════════════════════════════════════════════════════════════════════════
// Builder: FormulaInputs from KU
// ═══════════════════════════════════════════════════════════════════════════

/// Build `FormulaInputs` from a KuRuntime and role information.
///
/// Maps KU data fields to the inputs needed by the minting formula.
pub fn build_formula_inputs(
    ku: &KuRuntime,
    role_multiplier: f64,
    storage_factors: Option<StorageFactors>,
) -> FormulaInputs {
    FormulaInputs {
        raw_size_kb: ku.wire_bytes.len() as f64 / 1024.0,
        role_multiplier,
        pomv_score: ku.epi.pomv_score() as f32,
        storage_factors,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Builder: StoredKuInfo from KU + DHT metadata
// ═══════════════════════════════════════════════════════════════════════════

/// Build `StoredKuInfo` from a KuRuntime and external DHT metadata.
///
/// `actual_replicas` and `epochs_stored` come from the DHT layer (ku-net),
/// not from the KU itself.
pub fn build_stored_ku_info(
    ku: &KuRuntime,
    actual_replicas: u32,
    metabolism_rate: f64,
    epochs_stored: u64,
) -> StoredKuInfo {
    StoredKuInfo {
        ku_cid: ku.cid,
        wire_bytes_len: ku.wire_bytes.len() as u32,
        actual_replicas,
        metabolism_rate,
        epochs_stored,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Bridge: DHT Replica Tracking → Storage Reward Computation
// ═══════════════════════════════════════════════════════════════════════════

/// Replica metadata from DHT layer, passed into ku-core for storage reward computation.
///
/// This mirrors ku-net's `StoredKuMeta` but lives in ku-core to avoid circular dependency.
/// Populated by converting `ReplicaTracker::all_stored()` entries at the network boundary.
#[derive(Debug, Clone)]
pub struct ReplicaSnapshot {
    /// CID of the stored KU (BLAKE3 hash).
    pub ku_cid: [u8; 32],
    /// Current replica count on the DHT.
    pub actual_replicas: u32,
    /// Epoch when this node first stored this KU.
    pub first_stored_epoch: u64,
    /// Number of consecutive epochs this node has stored this KU.
    pub epochs_stored: u64,
    /// Wire-encoded size of the KU in bytes.
    pub wire_bytes: u32,
}

/// Computes storage rewards for a node based on its stored KUs and their replica metadata.
///
/// This is the bridge function that connects DHT replica tracking (ku-net)
/// with storage reward computation (ku-core).
///
/// # Arguments
/// * `replicas` — Snapshot of all KUs stored by this node (from `ReplicaTracker::all_stored()`)
/// * `node_trust` — Node's EigenTrust score [0.0, 1.0]
/// * `median_metabolism` — Network-wide median metabolism rate for demand_weight calculation
/// * `current_epoch` — Current epoch number (reserved for future per-epoch logic)
///
/// # Returns
/// Total storage reward in milliOBT for this node for the current epoch.
pub fn compute_epoch_storage_rewards(
    replicas: &[ReplicaSnapshot],
    node_trust: f64,
    median_metabolism: f64,
    _current_epoch: u64,
) -> u64 {
    use crate::obt_storage_reward::{compute_node_storage_reward, StoredKuInfo};

    let stored_kus: Vec<StoredKuInfo> = replicas
        .iter()
        .map(|r| {
            StoredKuInfo {
                ku_cid: r.ku_cid,
                wire_bytes_len: r.wire_bytes,
                actual_replicas: r.actual_replicas,
                metabolism_rate: 1.0, // Default: at-median; real value from KUMetabolism in future
                epochs_stored: r.epochs_stored,
            }
        })
        .collect();

    // compute_node_storage_reward returns f64 OBT; convert to milliOBT (u64)
    let raw_reward = compute_node_storage_reward(&stored_kus, node_trust, median_metabolism);
    (raw_reward * 1000.0) as u64
}

// ═══════════════════════════════════════════════════════════════════════════
// Quality Gate Orchestration
// ═══════════════════════════════════════════════════════════════════════════

/// Result of running all 4 KU quality gates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityGateResult {
    pub gate_1_size: bool,
    pub gate_2_consensus: bool,
    pub gate_3_pomv: bool,
    pub gate_4_complexity: bool,
}

impl QualityGateResult {
    /// Are all gates passed?
    pub fn all_passed(&self) -> bool {
        self.gate_1_size && self.gate_2_consensus && self.gate_3_pomv && self.gate_4_complexity
    }

    /// Which gates failed? Returns names of failed gates.
    pub fn failed_gates(&self) -> Vec<&'static str> {
        let mut failed = Vec::new();
        if !self.gate_1_size {
            failed.push("Gate1:MinSize");
        }
        if !self.gate_2_consensus {
            failed.push("Gate2:EncodingConsensus");
        }
        if !self.gate_3_pomv {
            failed.push("Gate3:PoMV");
        }
        if !self.gate_4_complexity {
            failed.push("Gate4:Complexity");
        }
        failed
    }
}

/// Run all 4 quality gates on a KU to determine reward eligibility.
///
/// This is the main entry point for quality checking before OBT rewards.
///
/// # Parameters
/// - `ku`: The Knowledge Unit to check
/// - `consensus`: The encoding consensus record (for verifier count)
/// - `known_raw_hashes`: Set of known raw text hashes (for duplicate check)
/// - `current_epoch`: Current epoch number (for age calculation)
pub fn run_quality_gates(
    ku: &KuRuntime,
    consensus: &EncodingConsensus,
    known_raw_hashes: &HashSet<[u8; 32]>,
    current_epoch: u64,
) -> QualityGateResult {
    // Gate 1: Minimum size & content richness
    // gene_count maps to instruction_count (each KU v6 = 1 Gene with N instructions)
    let gate_1 = obt_anti_gaming::gate_1_min_size(ku.wire_bytes.len(), ku.instruction_count());

    // Gate 2: Encoding consensus (enough independent verifiers, not a duplicate)
    let gate_2 = obt_anti_gaming::gate_2_encoding_consensus(
        consensus.verifier_count(),
        consensus.is_duplicate(known_raw_hashes),
    );

    // Gate 3: PoMV score (with grace period for new KUs)
    let creation_epoch = epoch_from_timestamp(consensus.created_at);
    let age_epochs = current_epoch.saturating_sub(creation_epoch);
    let gate_3 = obt_anti_gaming::gate_3_pomv(ku.epi.pomv_score() as f32, age_epochs);

    // Gate 4: Complexity (encoding time and bond richness)
    let gate_4 =
        obt_anti_gaming::gate_4_complexity(consensus.avg_encoding_time_ms(), ku.epi.bonds.len());

    QualityGateResult {
        gate_1_size: gate_1,
        gate_2_consensus: gate_2,
        gate_3_pomv: gate_3,
        gate_4_complexity: gate_4,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// MintProof Builder
// ═══════════════════════════════════════════════════════════════════════════

/// Build a MintProof for an encoding reward (R2).
///
/// Called when an AI encoder's submission is accepted during encoding consensus.
pub fn build_encoding_mint_proof(
    ku: &KuRuntime,
    recipient: [u8; 32],
    obt_amount: u64,
    role_multiplier: f64,
    epoch: u64,
    timestamp: u64,
) -> MintProof {
    MintProof {
        activity: MintActivity::Encoding,
        ku_cid: ku.cid,
        obt_amount,
        formula_inputs: build_formula_inputs(ku, role_multiplier, None),
        epoch,
        recipient,
        witnesses: Vec::new(), // Filled by network layer
        clock: VectorClock::new(),
        timestamp,
    }
}

/// Build a MintProof for a verification reward (R3).
pub fn build_verification_mint_proof(
    ku: &KuRuntime,
    recipient: [u8; 32],
    obt_amount: u64,
    role_multiplier: f64,
    epoch: u64,
    timestamp: u64,
) -> MintProof {
    MintProof {
        activity: MintActivity::Verification,
        ku_cid: ku.cid,
        obt_amount,
        formula_inputs: build_formula_inputs(ku, role_multiplier, None),
        epoch,
        recipient,
        witnesses: Vec::new(),
        clock: VectorClock::new(),
        timestamp,
    }
}

/// Build a MintProof for a PoMV owner reward (R1).
pub fn build_pomv_mint_proof(
    ku: &KuRuntime,
    owner: [u8; 32],
    obt_amount: u64,
    epoch: u64,
    timestamp: u64,
) -> MintProof {
    MintProof {
        activity: MintActivity::PomvReward,
        ku_cid: ku.cid,
        obt_amount,
        formula_inputs: build_formula_inputs(ku, 1.0, None),
        epoch,
        recipient: owner,
        witnesses: Vec::new(),
        clock: VectorClock::new(),
        timestamp,
    }
}

/// Build a MintProof for a storage reward (R4).
pub fn build_storage_mint_proof(
    ku: &KuRuntime,
    storer: [u8; 32],
    obt_amount: u64,
    storage_factors: StorageFactors,
    epoch: u64,
    timestamp: u64,
) -> MintProof {
    MintProof {
        activity: MintActivity::StorageReward,
        ku_cid: ku.cid,
        obt_amount,
        formula_inputs: build_formula_inputs(ku, 1.0, Some(storage_factors)),
        epoch,
        recipient: storer,
        witnesses: Vec::new(),
        clock: VectorClock::new(),
        timestamp,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_dna::{CoreDna, CoreDnaHeader, Instruction};
    use crate::encoding_consensus::EncodingStatus;
    use crate::epigenetics::Epigenetics;

    fn test_dna() -> CoreDna {
        CoreDna {
            header: CoreDnaHeader {
                version: 2,
                gene_type: 0,
                has_concept_table: false,
            },
            concept_table: Vec::new(),
            instructions: vec![
                Instruction::Triple { s: 1, p: 2, o: 3 },
                Instruction::Certainty { level: 9000 },
                Instruction::End,
            ],
        }
    }

    fn test_ku() -> KuRuntime {
        // Create a KU with enough content to pass Gate 1
        let dna = test_dna();
        let wire_bytes = vec![0u8; 300]; // 300 bytes > MIN_KU_RAW_BYTES (256)
        let cid = blake3::hash(&wire_bytes).into();
        KuRuntime {
            cid,
            dna,
            epi: Epigenetics::default(),
            expr: None,
            wire_bytes,
            encoding_status: EncodingStatus::Full,
        }
    }

    fn test_consensus() -> EncodingConsensus {
        EncodingConsensus::new_raw("test knowledge content".to_string(), 1, 1000)
    }

    #[test]
    fn test_build_formula_inputs() {
        let ku = test_ku();
        let inputs = build_formula_inputs(&ku, 2.0, None);
        assert!((inputs.raw_size_kb - 300.0 / 1024.0).abs() < 0.01);
        assert_eq!(inputs.role_multiplier, 2.0);
        assert!(inputs.storage_factors.is_none());
    }

    #[test]
    fn test_build_stored_ku_info() {
        let ku = test_ku();
        let info = build_stored_ku_info(&ku, 15, 0.5, 100);
        assert_eq!(info.ku_cid, ku.cid);
        assert_eq!(info.wire_bytes_len, 300);
        assert_eq!(info.actual_replicas, 15);
        assert!((info.metabolism_rate - 0.5).abs() < 1e-10);
        assert_eq!(info.epochs_stored, 100);
    }

    #[test]
    fn test_quality_gate_result_all_passed() {
        let result = QualityGateResult {
            gate_1_size: true,
            gate_2_consensus: true,
            gate_3_pomv: true,
            gate_4_complexity: true,
        };
        assert!(result.all_passed());
        assert!(result.failed_gates().is_empty());
    }

    #[test]
    fn test_quality_gate_result_some_failed() {
        let result = QualityGateResult {
            gate_1_size: true,
            gate_2_consensus: false,
            gate_3_pomv: true,
            gate_4_complexity: false,
        };
        assert!(!result.all_passed());
        let failed = result.failed_gates();
        assert_eq!(failed.len(), 2);
        assert!(failed.contains(&"Gate2:EncodingConsensus"));
        assert!(failed.contains(&"Gate4:Complexity"));
    }

    #[test]
    fn test_run_quality_gates_new_ku() {
        let ku = test_ku();
        let consensus = test_consensus();
        let known = HashSet::new();

        // New KU with 0 verifiers, 0 bonds — most gates fail
        let result = run_quality_gates(&ku, &consensus, &known, 10);
        // Gate 1: 300 bytes >= 256, 3 instructions >= 2 → PASS
        assert!(result.gate_1_size);
        // Gate 2: 0 verifiers < 3 → FAIL
        assert!(!result.gate_2_consensus);
        // Gate 3: 0 pomv score but age < grace period → PASS
        assert!(result.gate_3_pomv);
        // Gate 4: 0 encoding_time < 100ms → FAIL
        assert!(!result.gate_4_complexity);
    }

    #[test]
    fn test_build_encoding_mint_proof() {
        let ku = test_ku();
        let recipient = [42u8; 32];
        let proof = build_encoding_mint_proof(&ku, recipient, 5000, 2.0, 100, 360000);
        assert_eq!(proof.activity, MintActivity::Encoding);
        assert_eq!(proof.ku_cid, ku.cid);
        assert_eq!(proof.obt_amount, 5000);
        assert_eq!(proof.epoch, 100);
        assert_eq!(proof.recipient, recipient);
    }

    #[test]
    fn test_build_pomv_mint_proof() {
        let ku = test_ku();
        let owner = [1u8; 32];
        let proof = build_pomv_mint_proof(&ku, owner, 1000, 50, 180000);
        assert_eq!(proof.activity, MintActivity::PomvReward);
        assert_eq!(proof.formula_inputs.role_multiplier, 1.0);
    }

    #[test]
    fn test_build_storage_mint_proof() {
        let ku = test_ku();
        let storer = [99u8; 32];
        let factors = StorageFactors {
            size_weight: 1.0,
            rarity_weight: 2.0,
            demand_weight: 1.5,
            duration_factor: 1.0,
            trust_factor: 0.8,
        };
        let proof = build_storage_mint_proof(&ku, storer, 500, factors, 100, 360000);
        assert_eq!(proof.activity, MintActivity::StorageReward);
        assert!(proof.formula_inputs.storage_factors.is_some());
    }

    // ── compute_epoch_storage_rewards bridge ──────────────────────────

    #[test]
    fn test_compute_epoch_storage_rewards_empty() {
        let result = compute_epoch_storage_rewards(&[], 0.8, 1.0, 100);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_compute_epoch_storage_rewards_basic() {
        let replicas = vec![
            ReplicaSnapshot {
                ku_cid: [1u8; 32],
                actual_replicas: 10,
                first_stored_epoch: 0,
                epochs_stored: 200,
                wire_bytes: 2048,
            },
            ReplicaSnapshot {
                ku_cid: [2u8; 32],
                actual_replicas: 20,
                first_stored_epoch: 50,
                epochs_stored: 50,
                wire_bytes: 512,
            },
        ];
        let result = compute_epoch_storage_rewards(&replicas, 0.9, 1.0, 200);
        assert!(
            result > 0,
            "Should earn non-zero storage reward, got {result}"
        );
    }

    #[test]
    fn test_compute_epoch_storage_rewards_trust_impact() {
        let replicas = vec![ReplicaSnapshot {
            ku_cid: [1u8; 32],
            actual_replicas: 10,
            first_stored_epoch: 0,
            epochs_stored: 100,
            wire_bytes: 1024,
        }];
        let high_trust = compute_epoch_storage_rewards(&replicas, 0.9, 1.0, 100);
        let low_trust = compute_epoch_storage_rewards(&replicas, 0.1, 1.0, 100);
        assert!(
            high_trust > low_trust,
            "Higher trust should yield higher reward: high={high_trust}, low={low_trust}"
        );
    }
}
