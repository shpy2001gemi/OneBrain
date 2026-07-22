//! # OBT Gossip Protocol Handlers
//!
//! Handles OBT-specific gossip messages beyond basic transfers:
//! - ForkWarrant validation and propagation
//! - MintProof relay validation
//! - Epoch summary gossip
//!
//! ## Reference
//! See `docs/specs/obt/07_GOSSIP_SECURITY.md`.

use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════════════
// §7.1 — Fork Warrant Validation
// ═══════════════════════════════════════════════════════════════════════════

/// Strategy for propagating fork warrants through the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForkWarrantStrategy {
    /// Broadcast to all known peers (emergency flood).
    Broadcast,
    /// Send to K closest DHT neighbors (targeted).
    DhtNeighbors,
    /// Use PubSub topic for fork alerts.
    PubSub,
}

/// Errors during fork warrant validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForkWarrantError {
    IdenticalBlocks,
    InvalidWarrantHash,
    InvalidSignatureLength { expected: usize, actual: usize },
    FutureTimestamp { detected_at: u64, current_ts: u64 },
}

impl std::fmt::Display for ForkWarrantError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IdenticalBlocks => write!(f, "Fork warrant has identical block hashes"),
            Self::InvalidWarrantHash => write!(f, "Warrant hash verification failed"),
            Self::InvalidSignatureLength { expected, actual } => write!(
                f,
                "Invalid signature length: expected {}, got {}",
                expected, actual
            ),
            Self::FutureTimestamp {
                detected_at,
                current_ts,
            } => write!(
                f,
                "Warrant timestamp {} is in the future (current: {})",
                detected_at, current_ts
            ),
        }
    }
}

impl std::error::Error for ForkWarrantError {}

/// Validate a received fork warrant before relaying.
pub fn validate_fork_warrant(
    offender: &[u8; 32],
    block_a_hash: &[u8; 32],
    block_b_hash: &[u8; 32],
    sequence: u64,
    warrant_hash: &[u8; 32],
    signature: &[u8],
    detected_at: u64,
    current_ts: u64,
) -> Result<(), ForkWarrantError> {
    if block_a_hash == block_b_hash {
        return Err(ForkWarrantError::IdenticalBlocks);
    }
    let mut hash_input = Vec::with_capacity(32 + 32 + 32 + 8);
    hash_input.extend_from_slice(offender);
    hash_input.extend_from_slice(block_a_hash);
    hash_input.extend_from_slice(block_b_hash);
    hash_input.extend_from_slice(&sequence.to_le_bytes());
    let expected: [u8; 32] = blake3::hash(&hash_input).into();
    if &expected != warrant_hash {
        return Err(ForkWarrantError::InvalidWarrantHash);
    }
    if signature.len() != 64 {
        return Err(ForkWarrantError::InvalidSignatureLength {
            expected: 64,
            actual: signature.len(),
        });
    }
    if detected_at > current_ts + 60 {
        return Err(ForkWarrantError::FutureTimestamp {
            detected_at,
            current_ts,
        });
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// §7.2 — Mint Proof Relay
// ═══════════════════════════════════════════════════════════════════════════

/// Validate basic structure of a MintBroadcast before relaying.
pub fn validate_mint_broadcast_relay(
    obt_amount: u64,
    witness_count: usize,
    witness_sig_lengths: &[usize],
    ku_cid: &[u8; 32],
) -> Result<(), &'static str> {
    if obt_amount == 0 {
        return Err("Mint amount must be > 0");
    }
    if witness_count < 3 {
        return Err("Insufficient witnesses (need >= 3)");
    }
    for &len in witness_sig_lengths {
        if len != 64 {
            return Err("Witness signature must be 64 bytes");
        }
    }
    if ku_cid == &[0u8; 32] {
        return Err("KU CID cannot be all zeros");
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// §7.3 — Epoch Summary Gossip
// ═══════════════════════════════════════════════════════════════════════════

/// Network-level epoch summary for gossip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpochSummaryGossip {
    pub epoch: u64,
    pub node_id: [u8; 32],
    pub stored_ku_count: u32,
    pub witnessed_mints: u32,
    pub avg_pomv_score: f64,
    pub alert_level: u8,
}

impl EpochSummaryGossip {
    pub fn new(epoch: u64, node_id: [u8; 32]) -> Self {
        Self {
            epoch,
            node_id,
            stored_ku_count: 0,
            witnessed_mints: 0,
            avg_pomv_score: 0.0,
            alert_level: 0,
        }
    }
    pub fn is_current(&self, current_epoch: u64) -> bool {
        self.epoch == current_epoch
    }
    pub fn is_stale(&self, current_epoch: u64) -> bool {
        current_epoch > self.epoch + 1
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_fork_warrant_ok() {
        let offender = [1u8; 32];
        let block_a = [2u8; 32];
        let block_b = [3u8; 32];
        let seq = 42u64;
        let mut input = Vec::new();
        input.extend_from_slice(&offender);
        input.extend_from_slice(&block_a);
        input.extend_from_slice(&block_b);
        input.extend_from_slice(&seq.to_le_bytes());
        let wh: [u8; 32] = blake3::hash(&input).into();
        assert!(validate_fork_warrant(
            &offender,
            &block_a,
            &block_b,
            seq,
            &wh,
            &vec![0u8; 64],
            1000,
            1000
        )
        .is_ok());
    }

    #[test]
    fn test_fork_warrant_identical_blocks() {
        let b = [2u8; 32];
        assert_eq!(
            validate_fork_warrant(
                &[1u8; 32],
                &b,
                &b,
                42,
                &[0u8; 32],
                &vec![0u8; 64],
                1000,
                1000
            ),
            Err(ForkWarrantError::IdenticalBlocks)
        );
    }

    #[test]
    fn test_fork_warrant_bad_hash() {
        assert_eq!(
            validate_fork_warrant(
                &[1u8; 32],
                &[2u8; 32],
                &[3u8; 32],
                42,
                &[0u8; 32],
                &vec![0u8; 64],
                1000,
                1000
            ),
            Err(ForkWarrantError::InvalidWarrantHash)
        );
    }

    #[test]
    fn test_fork_warrant_bad_sig() {
        let offender = [1u8; 32];
        let block_a = [2u8; 32];
        let block_b = [3u8; 32];
        let seq = 42u64;
        let mut input = Vec::new();
        input.extend_from_slice(&offender);
        input.extend_from_slice(&block_a);
        input.extend_from_slice(&block_b);
        input.extend_from_slice(&seq.to_le_bytes());
        let wh: [u8; 32] = blake3::hash(&input).into();
        assert_eq!(
            validate_fork_warrant(
                &offender,
                &block_a,
                &block_b,
                seq,
                &wh,
                &vec![0u8; 32],
                1000,
                1000
            ),
            Err(ForkWarrantError::InvalidSignatureLength {
                expected: 64,
                actual: 32
            })
        );
    }

    #[test]
    fn test_fork_warrant_future_ts() {
        let offender = [1u8; 32];
        let block_a = [2u8; 32];
        let block_b = [3u8; 32];
        let seq = 42u64;
        let mut input = Vec::new();
        input.extend_from_slice(&offender);
        input.extend_from_slice(&block_a);
        input.extend_from_slice(&block_b);
        input.extend_from_slice(&seq.to_le_bytes());
        let wh: [u8; 32] = blake3::hash(&input).into();
        assert_eq!(
            validate_fork_warrant(
                &offender,
                &block_a,
                &block_b,
                seq,
                &wh,
                &vec![0u8; 64],
                2000,
                1000
            ),
            Err(ForkWarrantError::FutureTimestamp {
                detected_at: 2000,
                current_ts: 1000
            })
        );
    }

    #[test]
    fn test_mint_broadcast_relay_ok() {
        assert!(validate_mint_broadcast_relay(1000, 3, &[64, 64, 64], &[1u8; 32]).is_ok());
    }

    #[test]
    fn test_mint_broadcast_relay_zero() {
        assert!(validate_mint_broadcast_relay(0, 3, &[64, 64, 64], &[1u8; 32]).is_err());
    }

    #[test]
    fn test_mint_broadcast_relay_few_witnesses() {
        assert!(validate_mint_broadcast_relay(1000, 2, &[64, 64], &[1u8; 32]).is_err());
    }

    #[test]
    fn test_mint_broadcast_relay_zero_cid() {
        assert!(validate_mint_broadcast_relay(1000, 3, &[64, 64, 64], &[0u8; 32]).is_err());
    }

    #[test]
    fn test_epoch_summary() {
        let s = EpochSummaryGossip::new(100, [1u8; 32]);
        assert!(s.is_current(100));
        assert!(!s.is_current(101));
        assert!(!s.is_stale(101));
        assert!(s.is_stale(102));
    }
}
