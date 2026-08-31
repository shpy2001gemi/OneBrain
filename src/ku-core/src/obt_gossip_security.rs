//! # Gossip Security Module (§7.2 / §7.3)
//!
//! - **GossipGapDetector** — sliding-window offline-event tracker with alert levels.
//! - **ConnectivityProof** — 3-receipt liveness proof validated against witness set.
//! - **EpochSummary** — per-epoch settlement snapshot.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

// ─────────────────────────────────────────────────────────────────────
// §7.2  Gossip Gap Detection
// ─────────────────────────────────────────────────────────────────────

pub const GOSSIP_GAP_WINDOW_S: u64 = 30;
pub const GOSSIP_GAP_ELEVATED_THRESHOLD: u32 = 3;
pub const GOSSIP_GAP_RED_FLAG_THRESHOLD: u32 = 5;
pub const GOSSIP_GAP_WITNESS_MULTIPLIER: u32 = 2;

/// Graduated alert severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AlertLevel {
    Normal,
    ElevatedScrutiny,
    RedFlag,
}

/// Tracks per-timestamp offline events and evaluates the gossip-gap
/// alert level within a sliding window.
#[derive(Debug, Clone, Default)]
pub struct GossipGapDetector {
    /// ts → list of node-ids that went offline at that timestamp
    offline_events: BTreeMap<u64, Vec<u64>>,
}

impl GossipGapDetector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a single node going offline at `timestamp`.
    pub fn record_offline(&mut self, node_id: u64, timestamp: u64) {
        self.offline_events
            .entry(timestamp)
            .or_default()
            .push(node_id);
    }

    /// Evaluate alert level: count *unique nodes* offline in the
    /// sliding window `[current_ts - WINDOW, current_ts]`.
    pub fn check(&self, current_ts: u64) -> AlertLevel {
        let unique = self.nodes_in_window(current_ts).len() as u32;
        if unique >= GOSSIP_GAP_RED_FLAG_THRESHOLD {
            AlertLevel::RedFlag
        } else if unique >= GOSSIP_GAP_ELEVATED_THRESHOLD {
            AlertLevel::ElevatedScrutiny
        } else {
            AlertLevel::Normal
        }
    }

    /// Return the set of unique node-ids that went offline inside the
    /// window `[current_ts - GOSSIP_GAP_WINDOW_S, current_ts]`.
    pub fn nodes_in_window(&self, current_ts: u64) -> HashSet<u64> {
        let start = current_ts.saturating_sub(GOSSIP_GAP_WINDOW_S);
        let mut out = HashSet::new();
        for (_ts, nodes) in self.offline_events.range(start..=current_ts) {
            for n in nodes {
                out.insert(*n);
            }
        }
        out
    }

    /// Remove entries older than `cutoff`.
    pub fn cleanup_old(&mut self, cutoff: u64) {
        // `split_off` keeps keys ≥ cutoff; we replace self.offline_events
        // with the "newer" half.
        let keep = self.offline_events.split_off(&cutoff);
        self.offline_events = keep;
    }
}

// ─────────────────────────────────────────────────────────────────────
// §7.3  Connectivity Proof
// ─────────────────────────────────────────────────────────────────────

pub const CONNECTIVITY_PROOF_COUNT: usize = 3;
pub const CONNECTIVITY_PROOF_TTL_S: u64 = 60;

/// A signed receipt attesting that a remote node recently gossiped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipReceipt {
    pub from_node: [u8; 32],
    pub epoch: u64,
    pub last_gossip_ts: u64,
    pub gossip_hash: [u8; 32],
    pub signature: Vec<u8>,
}

/// Bundle of external receipts proving recent connectivity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectivityProof {
    pub external_receipts: Vec<GossipReceipt>,
    pub proof_timestamp: u64,
}

/// Specific failure reasons during connectivity-proof validation.
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectivityError {
    InsufficientReceipts { required: usize, provided: usize },
    DuplicateReceiptSource,
    ReceiptFromWitness,
    StaleReceipt { age_s: u64, max_s: u64 },
    InvalidSignatureLength,
}

/// Validate a [`ConnectivityProof`] against the current epoch state.
///
/// Rules:
/// 1. At least `CONNECTIVITY_PROOF_COUNT` receipts.
/// 2. All `from_node` values are unique.
/// 3. No `from_node` belongs to the witness set.
/// 4. Every receipt is fresh: `current_ts - last_gossip_ts < TTL`.
/// 5. Every signature has length 64 (Ed25519 placeholder).
pub fn validate_connectivity_proof(
    proof: &ConnectivityProof,
    witness_set: &HashSet<[u8; 32]>,
    current_ts: u64,
) -> Result<(), ConnectivityError> {
    // 1. Count
    let n = proof.external_receipts.len();
    if n < CONNECTIVITY_PROOF_COUNT {
        return Err(ConnectivityError::InsufficientReceipts {
            required: CONNECTIVITY_PROOF_COUNT,
            provided: n,
        });
    }

    let mut seen = HashSet::new();
    for r in &proof.external_receipts {
        // 2. Uniqueness
        if !seen.insert(r.from_node) {
            return Err(ConnectivityError::DuplicateReceiptSource);
        }
        // 3. Not a witness
        if witness_set.contains(&r.from_node) {
            return Err(ConnectivityError::ReceiptFromWitness);
        }
        // 4. Freshness
        let age = current_ts.saturating_sub(r.last_gossip_ts);
        if age >= CONNECTIVITY_PROOF_TTL_S {
            return Err(ConnectivityError::StaleReceipt {
                age_s: age,
                max_s: CONNECTIVITY_PROOF_TTL_S,
            });
        }
        // 5. Signature length
        if r.signature.len() != 64 {
            return Err(ConnectivityError::InvalidSignatureLength);
        }
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────
// Epoch Settlement
// ─────────────────────────────────────────────────────────────────────

pub const OBT_EPOCH_DURATION_S: u64 = 3600;

/// Per-epoch settlement snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpochSummary {
    pub epoch: u64,
    pub start_ts: u64,
    pub end_ts: u64,
    pub active_nodes: u32,
    pub total_minted: u64,
    pub avg_pomv_score: f64,
    pub alert_level: AlertLevel,
}

/// Compute `(start_ts, end_ts)` for the given epoch number.
pub fn compute_epoch_boundaries(epoch: u64) -> (u64, u64) {
    let start = epoch * OBT_EPOCH_DURATION_S;
    let end = start + OBT_EPOCH_DURATION_S;
    (start, end)
}

/// Determine which epoch a timestamp belongs to.
pub fn epoch_from_timestamp(ts: u64) -> u64 {
    ts / OBT_EPOCH_DURATION_S
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Gossip Gap Detector ─────────────────────────────────────────

    #[test]
    fn test_empty_detector_normal() {
        let d = GossipGapDetector::new();
        assert_eq!(d.check(100), AlertLevel::Normal);
    }

    #[test]
    fn test_detector_elevated() {
        let mut d = GossipGapDetector::new();
        for i in 0..3 {
            d.record_offline(i, 100);
        }
        assert_eq!(d.check(100), AlertLevel::ElevatedScrutiny);
    }

    #[test]
    fn test_detector_red_flag() {
        let mut d = GossipGapDetector::new();
        for i in 0..5 {
            d.record_offline(i, 100);
        }
        assert_eq!(d.check(100), AlertLevel::RedFlag);
    }

    #[test]
    fn test_detector_window_expiry() {
        let mut d = GossipGapDetector::new();
        for i in 0..5 {
            d.record_offline(i, 50);
        }
        // 31 seconds later: window start = 81 - 30 = 51 > 50
        assert_eq!(d.check(81), AlertLevel::Normal);
    }

    #[test]
    fn test_detector_unique_nodes() {
        let mut d = GossipGapDetector::new();
        // Same node going offline multiple times should count once
        d.record_offline(1, 100);
        d.record_offline(1, 101);
        d.record_offline(1, 102);
        assert_eq!(d.nodes_in_window(102).len(), 1);
        assert_eq!(d.check(102), AlertLevel::Normal);
    }

    #[test]
    fn test_detector_cleanup_old() {
        let mut d = GossipGapDetector::new();
        d.record_offline(1, 10);
        d.record_offline(2, 20);
        d.record_offline(3, 30);
        d.cleanup_old(20);
        // Only entries with ts >= 20 remain
        assert!(!d.offline_events.contains_key(&10));
        assert!(d.offline_events.contains_key(&20));
    }

    // ── Connectivity Proof ──────────────────────────────────────────

    fn make_receipt(id_byte: u8, ts: u64) -> GossipReceipt {
        let mut from = [0u8; 32];
        from[0] = id_byte;
        GossipReceipt {
            from_node: from,
            epoch: 1,
            last_gossip_ts: ts,
            gossip_hash: [0u8; 32],
            signature: vec![0u8; 64],
        }
    }

    fn empty_witnesses() -> HashSet<[u8; 32]> {
        HashSet::new()
    }

    #[test]
    fn test_valid_proof() {
        let proof = ConnectivityProof {
            external_receipts: vec![
                make_receipt(1, 100),
                make_receipt(2, 100),
                make_receipt(3, 100),
            ],
            proof_timestamp: 110,
        };
        assert!(validate_connectivity_proof(&proof, &empty_witnesses(), 110).is_ok());
    }

    #[test]
    fn test_insufficient_receipts() {
        let proof = ConnectivityProof {
            external_receipts: vec![make_receipt(1, 100), make_receipt(2, 100)],
            proof_timestamp: 110,
        };
        assert_eq!(
            validate_connectivity_proof(&proof, &empty_witnesses(), 110),
            Err(ConnectivityError::InsufficientReceipts {
                required: 3,
                provided: 2
            })
        );
    }

    #[test]
    fn test_duplicate_source() {
        let proof = ConnectivityProof {
            external_receipts: vec![
                make_receipt(1, 100),
                make_receipt(1, 101),
                make_receipt(2, 100),
            ],
            proof_timestamp: 110,
        };
        assert_eq!(
            validate_connectivity_proof(&proof, &empty_witnesses(), 110),
            Err(ConnectivityError::DuplicateReceiptSource)
        );
    }

    #[test]
    fn test_receipt_from_witness() {
        let mut ws = HashSet::new();
        let mut w = [0u8; 32];
        w[0] = 2;
        ws.insert(w);

        let proof = ConnectivityProof {
            external_receipts: vec![
                make_receipt(1, 100),
                make_receipt(2, 100),
                make_receipt(3, 100),
            ],
            proof_timestamp: 110,
        };
        assert_eq!(
            validate_connectivity_proof(&proof, &ws, 110),
            Err(ConnectivityError::ReceiptFromWitness)
        );
    }

    #[test]
    fn test_stale_receipt() {
        let proof = ConnectivityProof {
            external_receipts: vec![
                make_receipt(1, 100),
                make_receipt(2, 100),
                make_receipt(3, 40), // age = 150 - 40 = 110 ≥ 60
            ],
            proof_timestamp: 150,
        };
        assert_eq!(
            validate_connectivity_proof(&proof, &empty_witnesses(), 150),
            Err(ConnectivityError::StaleReceipt {
                age_s: 110,
                max_s: 60,
            })
        );
    }

    #[test]
    fn test_invalid_signature_length() {
        let mut r = make_receipt(3, 100);
        r.signature = vec![0u8; 32]; // wrong length
        let proof = ConnectivityProof {
            external_receipts: vec![make_receipt(1, 100), make_receipt(2, 100), r],
            proof_timestamp: 110,
        };
        assert_eq!(
            validate_connectivity_proof(&proof, &empty_witnesses(), 110),
            Err(ConnectivityError::InvalidSignatureLength)
        );
    }

    // ── Epoch Helpers ───────────────────────────────────────────────

    #[test]
    fn test_epoch_boundaries() {
        let (s, e) = compute_epoch_boundaries(0);
        assert_eq!(s, 0);
        assert_eq!(e, 3600);

        let (s, e) = compute_epoch_boundaries(5);
        assert_eq!(s, 18000);
        assert_eq!(e, 21600);
    }

    #[test]
    fn test_epoch_from_timestamp() {
        assert_eq!(epoch_from_timestamp(0), 0);
        assert_eq!(epoch_from_timestamp(3599), 0);
        assert_eq!(epoch_from_timestamp(3600), 1);
        assert_eq!(epoch_from_timestamp(7201), 2);
    }

    #[test]
    fn test_epoch_roundtrip() {
        for ep in [0, 1, 42, 999] {
            let (start, _) = compute_epoch_boundaries(ep);
            assert_eq!(epoch_from_timestamp(start), ep);
        }
    }

    #[test]
    fn test_epoch_summary_construction() {
        let summary = EpochSummary {
            epoch: 1,
            start_ts: 3600,
            end_ts: 7200,
            active_nodes: 100,
            total_minted: 50_000,
            avg_pomv_score: 0.75,
            alert_level: AlertLevel::Normal,
        };
        assert_eq!(summary.epoch, 1);
        assert_eq!(summary.alert_level, AlertLevel::Normal);
    }

    #[test]
    fn test_alert_level_ordering() {
        assert!(AlertLevel::Normal < AlertLevel::ElevatedScrutiny);
        assert!(AlertLevel::ElevatedScrutiny < AlertLevel::RedFlag);
    }
}
