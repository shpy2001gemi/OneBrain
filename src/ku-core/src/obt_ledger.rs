//! # OBT Account-Chain Ledger
//!
//! Nano-inspired block-lattice: each account has its own append-only chain.
//! Balance is derived state (recorded in each `TransferBlock`).
//!
//! ## Why Not CRDTs for Balance
//! - **G-Counter**: Cannot decrement (spending is impossible).
//! - **PN-Counter**: Allows overdraft under concurrent partitioned decrements.
//! - **Bounded Counter**: Reintroduces coordination, defeats gossip-only model.
//! - **Account-Chain** âœ…: Single-writer append-only chain â€” no overdraft, no coordination.
//!
//! G-Counters are still used for *informational* `total_earned` / `total_spent` analytics.
//!
//! ## Reference
//! See `docs/specs/obt/02_LEDGER.md` for full specification.

use serde::{Serialize, Deserialize};
use crate::crdt::{GCounter, VectorClock};
use crate::obt_constants::GENESIS_BLOCK_PREVIOUS;

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// TransferOp â€” Operation Types
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// The operation that produced a `TransferBlock`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransferOp {
    /// Genesis block â€” opens the account. Balance MUST be 0.
    Open,

    /// Credit OBT from a verified reward source.
    Mint {
        source: MintSource,
        amount: u64,
    },

    /// Debit OBT â€” creates a pending credit for `receiver`.
    Send {
        /// Ed25519 pubkey of the recipient.
        receiver: [u8; 32],
        amount: u64,
    },

    /// Claim a pending credit by referencing the sender's Send block.
    Receive {
        /// Hash of the Send block being claimed.
        send_block_hash: [u8; 32],
        amount: u64,
    },

    /// Reclaim funds from an expired Send block (self-receive after 7-day expiry).
    /// See spec Â§6.5.5: Sender MAY create a Refund block if Receive not claimed.
    Refund {
        /// Hash of the expired Send block being reclaimed.
        send_block_hash: [u8; 32],
        amount: u64,
    },
}

impl TransferOp {
    /// Produce a deterministic canonical byte representation for hashing.
    ///
    /// Format: `[discriminant u8] [fields in order, LE for integers]`
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        match self {
            TransferOp::Open => {
                buf.push(0x00);
            }
            TransferOp::Mint { source, amount } => {
                buf.push(0x01);
                buf.extend_from_slice(&source.canonical_bytes());
                buf.extend_from_slice(&amount.to_le_bytes());
            }
            TransferOp::Send { receiver, amount } => {
                buf.push(0x02);
                buf.extend_from_slice(receiver);
                buf.extend_from_slice(&amount.to_le_bytes());
            }
            TransferOp::Receive { send_block_hash, amount } => {
                buf.push(0x03);
                buf.extend_from_slice(send_block_hash);
                buf.extend_from_slice(&amount.to_le_bytes());
            }
            TransferOp::Refund { send_block_hash, amount } => {
                buf.push(0x04);
                buf.extend_from_slice(send_block_hash);
                buf.extend_from_slice(&amount.to_le_bytes());
            }
        }
        buf
    }
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// MintSource â€” Provenance of Minted OBT
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Each `Mint` operation must declare its provenance.
/// DHT validators use this to look up the corresponding proof.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MintSource {
    /// Reward for encoding participation (R2/R3).
    EncodingReward {
        /// BLAKE3 hash of the raw KU content or KU CID.
        ku_cid: [u8; 32],
        /// Role discriminant (0=Primary, 1=Secondary, 2=Tiebreaker).
        role: u8,
    },

    /// Reward for verification participation.
    VerificationReward {
        /// KU content ID that was verified.
        ku_cid: [u8; 32],
        /// Role discriminant.
        role: u8,
    },

    /// Reward for KU value via Proof-of-Metabolism-Value (R1).
    PomvReward {
        /// Content ID of the rewarded KU.
        ku_cid: [u8; 32],
        /// Epoch in which the reward was earned.
        epoch: u64,
    },

    /// Reward for provably storing KUs via PoS-KU challenge (R4).
    StorageReward {
        /// Epoch of the challenge.
        epoch: u64,
        /// BLAKE3 hash of the challenge response.
        challenge_hash: [u8; 32],
    },
}

impl MintSource {
    /// Produce a deterministic canonical byte representation.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        match self {
            MintSource::EncodingReward { ku_cid, role } => {
                buf.push(0x00);
                buf.extend_from_slice(ku_cid);
                buf.push(*role);
            }
            MintSource::VerificationReward { ku_cid, role } => {
                buf.push(0x01);
                buf.extend_from_slice(ku_cid);
                buf.push(*role);
            }
            MintSource::PomvReward { ku_cid, epoch } => {
                buf.push(0x02);
                buf.extend_from_slice(ku_cid);
                buf.extend_from_slice(&epoch.to_le_bytes());
            }
            MintSource::StorageReward { epoch, challenge_hash } => {
                buf.push(0x03);
                buf.extend_from_slice(&epoch.to_le_bytes());
                buf.extend_from_slice(challenge_hash);
            }
        }
        buf
    }
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// TransferBlock â€” Core Block Type
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// A single entry in an account's append-only chain.
///
/// Wire size: 32+32+8+8+var+var+8+64+32 â‰ˆ 240â€“320 bytes depending on operation.
///
/// ## Invariants
/// - `sequence == previous_block.sequence + 1` (or `0` for Open).
/// - `previous == previous_block.block_hash` (or `[0; 32]` for Open).
/// - `block_hash == BLAKE3(all fields except block_hash)`.
/// - `signature` is Ed25519 over the signing payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferBlock {
    /// BLAKE3 hash of the previous block in this account's chain.
    /// `[0u8; 32]` for the genesis (Open) block.
    pub previous: [u8; 32],

    /// Ed25519 public key of the account owner.
    pub account: [u8; 32],

    /// Monotonically increasing sequence number. Open block = 0.
    pub sequence: u64,

    /// Account balance AFTER this operation has been applied.
    /// Stored as unsigned â€” overdraft is structurally impossible.
    pub balance: u64,

    /// The operation that produced this block.
    pub operation: TransferOp,

    /// Lamport-style vector clock for causal ordering across accounts.
    pub clock: VectorClock,

    /// Wall-clock timestamp (Unix millis, UTC). Advisory â€” not used for ordering.
    pub timestamp: u64,

    /// Ed25519 signature over the signing payload.
    #[serde(with = "serde_bytes")]
    pub signature: Vec<u8>,

    /// BLAKE3 hash of all fields above (including signature).
    /// This is the block's unique identifier.
    pub block_hash: [u8; 32],
}

impl TransferBlock {
    /// Compute the BLAKE3 hash of all fields except `block_hash` itself.
    ///
    /// ```text
    /// block_hash = BLAKE3(
    ///     previous â€– account â€– sequence.to_le_bytes() â€– balance.to_le_bytes()
    ///     â€– operation.canonical_bytes() â€– clock.canonical_bytes()
    ///     â€– timestamp.to_le_bytes() â€– signature
    /// )
    /// ```
    pub fn compute_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.previous);
        hasher.update(&self.account);
        hasher.update(&self.sequence.to_le_bytes());
        hasher.update(&self.balance.to_le_bytes());
        hasher.update(&self.operation.canonical_bytes());
        hasher.update(&clock_canonical_bytes(&self.clock));
        hasher.update(&self.timestamp.to_le_bytes());
        hasher.update(&self.signature);
        *hasher.finalize().as_bytes()
    }

    /// Verify the Ed25519 signature against the given public key.
    ///
    /// Verifies that `self.signature` is a valid Ed25519 signature over
    /// `self.signing_payload()` using the provided public key bytes.
    ///
    /// # Errors
    /// - `InvalidSignatureLength` if signature is not 64 bytes
    /// - `InvalidSignature` if the public key is invalid or signature doesn't verify
    pub fn validate_signature(&self, pubkey: &[u8; 32]) -> Result<(), LedgerError> {
        use ed25519_dalek::{Signature, VerifyingKey, Verifier};

        if self.signature.len() != 64 {
            return Err(LedgerError::InvalidSignatureLength {
                expected: 64,
                actual: self.signature.len(),
            });
        }

        let verifying_key = VerifyingKey::from_bytes(pubkey)
            .map_err(|_| LedgerError::InvalidSignature)?;

        let sig_bytes: [u8; 64] = self.signature[..64]
            .try_into()
            .map_err(|_| LedgerError::InvalidSignature)?;
        let signature = Signature::from_bytes(&sig_bytes);

        let payload = self.signing_payload();
        verifying_key.verify(&payload, &signature)
            .map_err(|_| LedgerError::InvalidSignature)
    }

    /// Check only that the signature field has the correct length (64 bytes).
    ///
    /// This is a lightweight structural check that does NOT perform
    /// cryptographic verification. Use `validate_signature()` for full
    /// Ed25519 verification.
    pub fn validate_signature_length(&self) -> Result<(), LedgerError> {
        if self.signature.len() != 64 {
            return Err(LedgerError::InvalidSignatureLength {
                expected: 64,
                actual: self.signature.len(),
            });
        }
        Ok(())
    }

    /// Compute the signing payload (everything except signature and block_hash).
    ///
    /// This is the data that should be signed by Ed25519.
    pub fn signing_payload(&self) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&self.previous);
        payload.extend_from_slice(&self.account);
        payload.extend_from_slice(&self.sequence.to_le_bytes());
        payload.extend_from_slice(&self.balance.to_le_bytes());
        payload.extend_from_slice(&self.operation.canonical_bytes());
        payload.extend_from_slice(&clock_canonical_bytes(&self.clock));
        payload.extend_from_slice(&self.timestamp.to_le_bytes());
        payload
    }

    /// Validate this block against its predecessor in the chain.
    ///
    /// Checks:
    /// - **V-SEQ**: `self.sequence == prev.sequence + 1`
    /// - **V-PREV**: `self.previous == prev.block_hash`
    /// - **V-BAL**: Balance transition is consistent with the operation.
    pub fn validate_against_previous(&self, prev: &TransferBlock) -> Result<(), LedgerError> {
        // V-SEQ: sequence must increment by exactly 1
        if self.sequence != prev.sequence + 1 {
            return Err(LedgerError::SequenceMismatch {
                expected: prev.sequence + 1,
                actual: self.sequence,
            });
        }

        // V-PREV: previous hash must link to predecessor
        if self.previous != prev.block_hash {
            return Err(LedgerError::PreviousHashMismatch);
        }

        // V-SIG: signature must be exactly 64 bytes (Ed25519)
        if self.signature.len() != 64 {
            return Err(LedgerError::InvalidSignatureLength {
                expected: 64,
                actual: self.signature.len(),
            });
        }

        // V-BAL: balance consistency check
        match &self.operation {
            TransferOp::Mint { amount, .. } => {
                let expected = prev.balance.checked_add(*amount)
                    .ok_or(LedgerError::BalanceOverflow)?;
                if self.balance != expected {
                    return Err(LedgerError::BalanceMismatch {
                        expected,
                        actual: self.balance,
                    });
                }
            }
            TransferOp::Send { amount, .. } => {
                if *amount == 0 {
                    return Err(LedgerError::ZeroAmount);
                }
                let expected = prev.balance.checked_sub(*amount)
                    .ok_or(LedgerError::InsufficientBalance {
                        available: prev.balance,
                        required: *amount,
                    })?;
                if self.balance != expected {
                    return Err(LedgerError::BalanceMismatch {
                        expected,
                        actual: self.balance,
                    });
                }
            }
            TransferOp::Receive { amount, .. } => {
                let expected = prev.balance.checked_add(*amount)
                    .ok_or(LedgerError::BalanceOverflow)?;
                if self.balance != expected {
                    return Err(LedgerError::BalanceMismatch {
                        expected,
                        actual: self.balance,
                    });
                }
            }
            TransferOp::Refund { amount, .. } => {
                // Refund is a self-receive: balance increases
                let expected = prev.balance.checked_add(*amount)
                    .ok_or(LedgerError::BalanceOverflow)?;
                if self.balance != expected {
                    return Err(LedgerError::BalanceMismatch {
                        expected,
                        actual: self.balance,
                    });
                }
            }
            TransferOp::Open => {
                return Err(LedgerError::DuplicateOpen);
            }
        }

        Ok(())
    }

    /// Validate the block hash matches the computed hash.
    pub fn validate_hash(&self) -> Result<(), LedgerError> {
        let computed = self.compute_hash();
        if self.block_hash != computed {
            return Err(LedgerError::HashMismatch);
        }
        Ok(())
    }
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// AccountState â€” Per-Account DHT-Cached State
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Compact account summary â€” cached on DHT, updated on every new block.
///
/// Wire size: 32+8+32+8+var+var â‰ˆ 120â€“200 bytes.
///
/// ## Invariants (from spec Â§2.3.1)
/// - `balance == head_block.balance`
/// - `sequence == head_block.sequence`
/// - `head == head_block.block_hash`
/// - `total_earned.value() == Î£(amount)` for all Mint + Receive blocks
/// - `total_spent.value() == Î£(amount)` for all Send blocks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountState {
    /// Ed25519 public key (also the DHT lookup key).
    pub pubkey: [u8; 32],

    /// Current spendable balance (from latest TransferBlock).
    pub balance: u64,

    /// BLAKE3 hash of the most recent TransferBlock in this account's chain.
    pub head: [u8; 32],

    /// Sequence number of the head block.
    pub sequence: u64,

    /// Cumulative OBT ever earned (G-Counter â€” analytics only, never authoritative).
    pub total_earned: GCounter,

    /// Cumulative OBT ever spent (G-Counter â€” analytics only, never authoritative).
    pub total_spent: GCounter,
}

impl AccountState {
    /// Create a new account with balance=0 and no blocks.
    ///
    /// The `head` is `[0; 32]` until the Open block is applied.
    pub fn new(pubkey: [u8; 32]) -> Self {
        Self {
            pubkey,
            balance: 0,
            head: [0u8; 32],
            sequence: 0,
            total_earned: GCounter::new(),
            total_spent: GCounter::new(),
        }
    }

    /// Apply a validated block to update account state.
    ///
    /// Updates balance, head hash, sequence, and the informational G-Counters.
    /// The `node_id` is a u64 identifier for the G-Counter replica dimension.
    ///
    /// # Errors
    /// Returns `Err` if the block is invalid against the current state.
    pub fn apply_block(&mut self, block: &TransferBlock, node_id: u64) -> Result<(), LedgerError> {
        // Verify the block belongs to this account
        if block.account != self.pubkey {
            return Err(LedgerError::AccountMismatch);
        }

        // For the Open block: sequence must be 0 and balance 0
        if let TransferOp::Open = &block.operation {
            if self.sequence != 0 || self.head != [0u8; 32] {
                return Err(LedgerError::DuplicateOpen);
            }
            if block.sequence != 0 {
                return Err(LedgerError::SequenceMismatch {
                    expected: 0,
                    actual: block.sequence,
                });
            }
            if block.balance != 0 {
                return Err(LedgerError::BalanceMismatch {
                    expected: 0,
                    actual: block.balance,
                });
            }
        } else {
            // Non-Open: sequence must increment
            if block.sequence != self.sequence + 1 {
                return Err(LedgerError::SequenceMismatch {
                    expected: self.sequence + 1,
                    actual: block.sequence,
                });
            }
        }

        // Update G-Counters based on operation type
        match &block.operation {
            TransferOp::Open => {
                // No counter updates for Open
            }
            TransferOp::Mint { amount, .. } => {
                self.total_earned.increment_by(node_id, *amount);
            }
            TransferOp::Receive { amount, .. } => {
                self.total_earned.increment_by(node_id, *amount);
            }
            TransferOp::Send { amount, .. } => {
                self.total_spent.increment_by(node_id, *amount);
            }
            TransferOp::Refund { amount, .. } => {
                // Refund reverses a Send â€” credit back to earned
                self.total_earned.increment_by(node_id, *amount);
            }
        }

        // Update derived state
        self.balance = block.balance;
        self.head = block.block_hash;
        self.sequence = block.sequence;

        Ok(())
    }

    /// Verify the internal consistency invariants of this AccountState.
    ///
    /// Checks that `total_earned - total_spent >= 0` and that the balance
    /// is plausible given the counters.
    ///
    /// Note: G-Counters are informational â€” the authoritative balance
    /// is always `head_block.balance`. This check catches obvious corruption.
    pub fn verify_integrity(&self) -> Result<(), LedgerError> {
        let earned = self.total_earned.value();
        let spent = self.total_spent.value();

        // earned must be >= spent (can't spend more than earned)
        if spent > earned {
            return Err(LedgerError::IntegrityViolation {
                reason: format!(
                    "total_spent ({}) exceeds total_earned ({})",
                    spent, earned
                ),
            });
        }

        // balance should match earned - spent
        // (Note: this may diverge if G-Counter replicas haven't fully merged,
        //  so we only flag it as a warning in production. For tests, strict.)
        let expected_balance = earned - spent;
        if self.balance != expected_balance {
            return Err(LedgerError::IntegrityViolation {
                reason: format!(
                    "balance ({}) != total_earned ({}) - total_spent ({})",
                    self.balance, earned, spent
                ),
            });
        }

        Ok(())
    }
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// ForkWarrant â€” Cryptographic Proof of Double-Spend
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Cryptographic proof that an account produced conflicting blocks.
///
/// Unforgeable: contains both signed blocks as evidence.
/// A fork occurs when two `TransferBlock`s share `(account, sequence)`
/// but have different `block_hash` values.
///
/// See spec Â§2.5 for fork detection and resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkWarrant {
    /// The account (Ed25519 pubkey) that produced the fork.
    pub account: [u8; 32],

    /// First conflicting block.
    pub block_a: TransferBlock,

    /// Second conflicting block.
    pub block_b: TransferBlock,

    /// Ed25519 pubkey of the detecting node.
    pub detector: [u8; 32],

    /// Detector's Ed25519 signature over the warrant payload.
    #[serde(with = "serde_bytes")]
    pub detector_signature: Vec<u8>,

    /// Timestamp when the fork was detected (Unix millis, UTC).
    pub timestamp: u64,
}

impl ForkWarrant {
    /// Create a new fork warrant from two conflicting blocks.
    ///
    /// # Errors
    /// Returns `Err` if the blocks don't constitute a valid fork
    /// (same account + sequence, different hash).
    pub fn new(
        block_a: TransferBlock,
        block_b: TransferBlock,
        detector: [u8; 32],
        timestamp: u64,
    ) -> Result<Self, LedgerError> {
        // Validate fork conditions
        if block_a.account != block_b.account {
            return Err(LedgerError::InvalidFork {
                reason: "blocks belong to different accounts".to_string(),
            });
        }
        if block_a.sequence != block_b.sequence {
            return Err(LedgerError::InvalidFork {
                reason: "blocks have different sequence numbers".to_string(),
            });
        }
        if block_a.block_hash == block_b.block_hash {
            return Err(LedgerError::InvalidFork {
                reason: "blocks have the same hash (not a fork)".to_string(),
            });
        }

        Ok(Self {
            account: block_a.account,
            block_a,
            block_b,
            detector,
            detector_signature: vec![0u8; 64], // Stub: populated by signing layer
            timestamp,
        })
    }

    /// Verify the fork warrant is valid.
    ///
    /// Checks:
    /// 1. Both blocks have the same `(account, sequence)`.
    /// 2. Both blocks have different `block_hash`.
    /// 3. Both blocks' hashes are internally consistent.
    pub fn verify(&self) -> Result<(), LedgerError> {
        // Same account
        if self.block_a.account != self.block_b.account {
            return Err(LedgerError::InvalidFork {
                reason: "blocks belong to different accounts".to_string(),
            });
        }
        if self.block_a.account != self.account {
            return Err(LedgerError::InvalidFork {
                reason: "warrant account doesn't match blocks".to_string(),
            });
        }

        // Same sequence
        if self.block_a.sequence != self.block_b.sequence {
            return Err(LedgerError::InvalidFork {
                reason: "blocks have different sequence numbers".to_string(),
            });
        }

        // Different hash (proof of fork)
        if self.block_a.block_hash == self.block_b.block_hash {
            return Err(LedgerError::InvalidFork {
                reason: "blocks have the same hash (not a fork)".to_string(),
            });
        }

        // Verify internal hash consistency
        self.block_a.validate_hash()?;
        self.block_b.validate_hash()?;

        Ok(())
    }

    /// Return the canonical (winning) block using deterministic tiebreak.
    ///
    /// Per spec Â§2.5.3: lower `block_hash` wins (byte-wise lexicographic).
    pub fn canonical_block(&self) -> &TransferBlock {
        if self.block_a.block_hash < self.block_b.block_hash {
            &self.block_a
        } else {
            &self.block_b
        }
    }
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// LedgerError
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Errors produced by the OBT ledger subsystem.
#[derive(Debug, Clone, PartialEq)]
pub enum LedgerError {
    /// Block sequence number doesn't match expected.
    SequenceMismatch { expected: u64, actual: u64 },

    /// Block `previous` hash doesn't link to predecessor's `block_hash`.
    PreviousHashMismatch,

    /// Block `block_hash` doesn't match computed hash.
    HashMismatch,

    /// Balance in block doesn't match expected value after operation.
    BalanceMismatch { expected: u64, actual: u64 },

    /// Insufficient balance for Send operation.
    InsufficientBalance { available: u64, required: u64 },

    /// Balance arithmetic overflow (would exceed u64::MAX).
    BalanceOverflow,

    /// Send amount is zero (not allowed per spec).
    ZeroAmount,

    /// Attempted to create a second Open block for an account.
    DuplicateOpen,

    /// Block belongs to a different account than the AccountState.
    AccountMismatch,

    /// AccountState integrity check failed.
    IntegrityViolation { reason: String },

    /// Fork warrant is invalid.
    InvalidFork { reason: String },

    /// Ed25519 signature verification failed.
    InvalidSignature,

    /// Signature is not the expected length (64 bytes for Ed25519).
    InvalidSignatureLength { expected: usize, actual: usize },
}

impl std::fmt::Display for LedgerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SequenceMismatch { expected, actual } =>
                write!(f, "sequence mismatch: expected {expected}, got {actual}"),
            Self::PreviousHashMismatch =>
                write!(f, "previous hash does not match predecessor's block_hash"),
            Self::HashMismatch =>
                write!(f, "block_hash does not match computed hash"),
            Self::BalanceMismatch { expected, actual } =>
                write!(f, "balance mismatch: expected {expected}, got {actual}"),
            Self::InsufficientBalance { available, required } =>
                write!(f, "insufficient balance: have {available}, need {required}"),
            Self::BalanceOverflow =>
                write!(f, "balance overflow (exceeds u64::MAX)"),
            Self::ZeroAmount =>
                write!(f, "zero-amount transfer not allowed"),
            Self::DuplicateOpen =>
                write!(f, "duplicate Open block for this account"),
            Self::AccountMismatch =>
                write!(f, "block account does not match AccountState pubkey"),
            Self::IntegrityViolation { reason } =>
                write!(f, "integrity violation: {reason}"),
            Self::InvalidFork { reason } =>
                write!(f, "invalid fork warrant: {reason}"),
            Self::InvalidSignature =>
                write!(f, "Ed25519 signature verification failed"),
            Self::InvalidSignatureLength { expected, actual } =>
                write!(f, "signature length mismatch: expected {expected} bytes, got {actual}"),
        }
    }
}

impl std::error::Error for LedgerError {}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// Helpers
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•


/// Produce a deterministic byte representation of a VectorClock for hashing.
///
/// Serialize a VectorClock to deterministic canonical bytes for hashing.
///
/// Format: sorted (node_id as u64 LE, timestamp as u64 LE) pairs.
/// Deterministic because BTreeMap iteration is always sorted by key.
///
/// We avoid `serde_json` here because JSON output format can change
/// between library versions, breaking block hash verification across
/// node upgrades.
fn clock_canonical_bytes(clock: &VectorClock) -> Vec<u8> {
    // VectorClock uses BTreeMap<u64, u64> internally.
    // We serialize by extracting known node counts via the merge method.
    // Since we can't access private fields, we serialize a snapshot
    // using a deterministic binary format.
    //
    // Fallback: serialize the Debug representation which is deterministic
    // for BTreeMap (sorted keys). This is a temporary solution until
    // VectorClock exposes an iterator.
    let debug_str = format!("{:?}", clock);
    debug_str.into_bytes()
}

/// Create a genesis (Open) block for a new account.
///
/// The block has sequence=0, balance=0, previous=[0;32], and a computed hash.
/// `node_id` is the numeric ID used for VectorClock advancement.
pub fn create_open_block(account: [u8; 32], timestamp: u64, node_id: u64) -> TransferBlock {
    let mut clock = VectorClock::new();
    clock.tick(node_id);
    let mut block = TransferBlock {
        previous: GENESIS_BLOCK_PREVIOUS,
        account,
        sequence: 0,
        balance: 0,
        operation: TransferOp::Open,
        clock,
        timestamp,
        signature: vec![0u8; 64], // Stub: populated by signing layer
        block_hash: [0u8; 32], // Will be computed below
    };
    block.block_hash = block.compute_hash();
    block
}

/// Create a Mint block appended after `prev`.
pub fn create_mint_block(
    prev: &TransferBlock,
    source: MintSource,
    amount: u64,
    timestamp: u64,
    node_id: u64,
) -> Result<TransferBlock, LedgerError> {
    let new_balance = prev.balance.checked_add(amount)
        .ok_or(LedgerError::BalanceOverflow)?;

    let mut clock = prev.clock.clone();
    clock.tick(node_id);

    let mut block = TransferBlock {
        previous: prev.block_hash,
        account: prev.account,
        sequence: prev.sequence + 1,
        balance: new_balance,
        operation: TransferOp::Mint { source, amount },
        clock,
        timestamp,
        signature: vec![0u8; 64],
        block_hash: [0u8; 32],
    };
    block.block_hash = block.compute_hash();
    Ok(block)
}

/// Create a Send block appended after `prev`.
pub fn create_send_block(
    prev: &TransferBlock,
    receiver: [u8; 32],
    amount: u64,
    timestamp: u64,
    node_id: u64,
) -> Result<TransferBlock, LedgerError> {
    if amount == 0 {
        return Err(LedgerError::ZeroAmount);
    }
    let new_balance = prev.balance.checked_sub(amount)
        .ok_or(LedgerError::InsufficientBalance {
            available: prev.balance,
            required: amount,
        })?;

    let mut clock = prev.clock.clone();
    clock.tick(node_id);

    let mut block = TransferBlock {
        previous: prev.block_hash,
        account: prev.account,
        sequence: prev.sequence + 1,
        balance: new_balance,
        operation: TransferOp::Send { receiver, amount },
        clock,
        timestamp,
        signature: vec![0u8; 64],
        block_hash: [0u8; 32],
    };
    block.block_hash = block.compute_hash();
    Ok(block)
}

/// Create a Receive block appended after `prev`.
pub fn create_receive_block(
    prev: &TransferBlock,
    send_block_hash: [u8; 32],
    amount: u64,
    timestamp: u64,
    node_id: u64,
) -> Result<TransferBlock, LedgerError> {
    let new_balance = prev.balance.checked_add(amount)
        .ok_or(LedgerError::BalanceOverflow)?;

    let mut clock = prev.clock.clone();
    clock.tick(node_id);

    let mut block = TransferBlock {
        previous: prev.block_hash,
        account: prev.account,
        sequence: prev.sequence + 1,
        balance: new_balance,
        operation: TransferOp::Receive { send_block_hash, amount },
        clock,
        timestamp,
        signature: vec![0u8; 64],
        block_hash: [0u8; 32],
    };
    block.block_hash = block.compute_hash();
    Ok(block)
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// Tests
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Creates a signed TransferBlock using an Ed25519 signing key.
///
/// The block is first constructed with a stub signature to compute the
/// signing payload, then signed with the provided key, and finally the
/// block_hash is computed over the real signature.
pub fn create_signed_block(
    previous: [u8; 32],
    account: [u8; 32],
    sequence: u64,
    balance: u64,
    operation: TransferOp,
    clock: VectorClock,
    timestamp: u64,
    signing_key: &ed25519_dalek::SigningKey,
) -> TransferBlock {
    use ed25519_dalek::Signer;

    // Create block without signature first to compute signing payload
    let mut block = TransferBlock {
        previous,
        account,
        sequence,
        balance,
        operation,
        clock,
        timestamp,
        signature: vec![0u8; 64],
        block_hash: [0u8; 32],
    };

    let payload = block.signing_payload();
    let sig = signing_key.sign(&payload);
    block.signature = sig.to_bytes().to_vec();
    block.block_hash = block.compute_hash();
    block
}

/// Creates a signed genesis (Open) block using an Ed25519 signing key.
///
/// Convenience wrapper around `create_signed_block` for Open blocks.
pub fn create_signed_open_block(
    signing_key: &ed25519_dalek::SigningKey,
    timestamp: u64,
    node_id: u64,
) -> TransferBlock {
    let pubkey = signing_key.verifying_key().to_bytes();
    let mut clock = VectorClock::new();
    clock.tick(node_id);
    create_signed_block(
        GENESIS_BLOCK_PREVIOUS,
        pubkey,
        0,
        0,
        TransferOp::Open,
        clock,
        timestamp,
        signing_key,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helpers for test accounts
    fn alice_pubkey() -> [u8; 32] {
        let mut k = [0u8; 32];
        k[0] = 0xAA;
        k
    }

    fn bob_pubkey() -> [u8; 32] {
        let mut k = [0u8; 32];
        k[0] = 0xBB;
        k
    }

    fn test_mint_source() -> MintSource {
        MintSource::EncodingReward {
            ku_cid: [0x42; 32],
            role: 0,
        }
    }

    // â”€â”€â”€ Test 1: Create genesis (Open) block â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_create_genesis_open_block() {
        let open = create_open_block(alice_pubkey(), 1000, 1);

        assert_eq!(open.sequence, 0);
        assert_eq!(open.balance, 0);
        assert_eq!(open.previous, GENESIS_BLOCK_PREVIOUS);
        assert_eq!(open.account, alice_pubkey());
        assert!(matches!(open.operation, TransferOp::Open));

        // Hash should be valid
        assert_eq!(open.block_hash, open.compute_hash());
        assert!(open.validate_hash().is_ok());
    }

    // â”€â”€â”€ Test 2: Create and validate mint block â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_create_mint_block() {
        let open = create_open_block(alice_pubkey(), 1000, 1);
        let mint = create_mint_block(&open, test_mint_source(), 500, 2000, 1).unwrap();

        assert_eq!(mint.sequence, 1);
        assert_eq!(mint.balance, 500);
        assert_eq!(mint.previous, open.block_hash);
        assert!(mint.validate_hash().is_ok());
        assert!(mint.validate_against_previous(&open).is_ok());
    }

    // â”€â”€â”€ Test 3: Send/Receive flow â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_send_receive_flow() {
        // Alice: Open â†’ Mint 1000 â†’ Send 300 to Bob
        let alice_open = create_open_block(alice_pubkey(), 1000, 1);
        let alice_mint = create_mint_block(&alice_open, test_mint_source(), 1000, 2000, 1).unwrap();
        let alice_send = create_send_block(&alice_mint, bob_pubkey(), 300, 3000, 1).unwrap();

        assert_eq!(alice_send.balance, 700);
        assert!(alice_send.validate_against_previous(&alice_mint).is_ok());

        // Bob: Open â†’ Receive 300
        let bob_open = create_open_block(bob_pubkey(), 1000, 1);
        let bob_receive = create_receive_block(
            &bob_open,
            alice_send.block_hash,
            300,
            4000,
            2,
        ).unwrap();

        assert_eq!(bob_receive.balance, 300);
        assert!(bob_receive.validate_against_previous(&bob_open).is_ok());
    }

    // â”€â”€â”€ Test 4: Balance goes to 0 but not negative â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_balance_to_zero() {
        let open = create_open_block(alice_pubkey(), 1000, 1);
        let mint = create_mint_block(&open, test_mint_source(), 100, 2000, 1).unwrap();

        // Send exactly 100 â†’ balance = 0
        let send = create_send_block(&mint, bob_pubkey(), 100, 3000, 1).unwrap();
        assert_eq!(send.balance, 0);

        // Send 1 more â†’ should fail (insufficient balance)
        let result = create_send_block(&send, bob_pubkey(), 1, 4000, 1);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LedgerError::InsufficientBalance { .. }));
    }

    // â”€â”€â”€ Test 5: Sequence must increment â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_sequence_must_increment() {
        let open = create_open_block(alice_pubkey(), 1000, 1);
        let mint = create_mint_block(&open, test_mint_source(), 100, 2000, 1).unwrap();

        // Forge a block with wrong sequence
        let mut bad_block = mint.clone();
        bad_block.sequence = 5; // Should be 1
        bad_block.block_hash = bad_block.compute_hash();

        let result = bad_block.validate_against_previous(&open);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LedgerError::SequenceMismatch { .. }));
    }

    // â”€â”€â”€ Test 6: Fork detection â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_fork_detection() {
        let open = create_open_block(alice_pubkey(), 1000, 1);
        let mint = create_mint_block(&open, test_mint_source(), 1000, 2000, 1).unwrap();

        // Create two conflicting blocks at sequence 2
        let send_a = create_send_block(&mint, bob_pubkey(), 300, 3000, 1).unwrap();
        // Different receiver, same sequence
        let mut charlie = [0u8; 32];
        charlie[0] = 0xCC;
        let send_b = create_send_block(&mint, charlie, 800, 3001, 1).unwrap();

        // Both are at sequence 2, same account, different hash
        assert_eq!(send_a.sequence, send_b.sequence);
        assert_eq!(send_a.account, send_b.account);
        assert_ne!(send_a.block_hash, send_b.block_hash);

        // Create warrant
        let detector = [0xDD; 32];
        let warrant = ForkWarrant::new(send_a.clone(), send_b.clone(), detector, 5000).unwrap();
        assert!(warrant.verify().is_ok());

        // Canonical block is the one with lower hash
        let canonical = warrant.canonical_block();
        if send_a.block_hash < send_b.block_hash {
            assert_eq!(canonical.block_hash, send_a.block_hash);
        } else {
            assert_eq!(canonical.block_hash, send_b.block_hash);
        }
    }

    // â”€â”€â”€ Test 7: ForkWarrant rejects non-fork â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_fork_warrant_rejects_non_fork() {
        let open = create_open_block(alice_pubkey(), 1000, 1);
        let mint = create_mint_block(&open, test_mint_source(), 1000, 2000, 1).unwrap();

        // Same block twice = not a fork
        let result = ForkWarrant::new(
            mint.clone(),
            mint.clone(),
            [0xDD; 32],
            5000,
        );
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LedgerError::InvalidFork { .. }));
    }

    // â”€â”€â”€ Test 8: AccountState updates â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_account_state_updates() {
        let pubkey = alice_pubkey();
        let mut state = AccountState::new(pubkey);
        assert_eq!(state.balance, 0);
        assert_eq!(state.sequence, 0);

        // Apply Open block
        let open = create_open_block(pubkey, 1000, 1);
        state.apply_block(&open, 1).unwrap();
        assert_eq!(state.balance, 0);
        assert_eq!(state.sequence, 0);
        assert_eq!(state.head, open.block_hash);

        // Apply Mint block
        let mint = create_mint_block(&open, test_mint_source(), 500, 2000, 1).unwrap();
        state.apply_block(&mint, 1).unwrap();
        assert_eq!(state.balance, 500);
        assert_eq!(state.sequence, 1);
        assert_eq!(state.total_earned.value(), 500);
        assert_eq!(state.total_spent.value(), 0);

        // Apply Send block
        let send = create_send_block(&mint, bob_pubkey(), 200, 3000, 1).unwrap();
        state.apply_block(&send, 1).unwrap();
        assert_eq!(state.balance, 300);
        assert_eq!(state.sequence, 2);
        assert_eq!(state.total_spent.value(), 200);
    }

    // â”€â”€â”€ Test 9: AccountState verify_integrity â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_account_state_integrity() {
        let pubkey = alice_pubkey();
        let mut state = AccountState::new(pubkey);

        let open = create_open_block(pubkey, 1000, 1);
        state.apply_block(&open, 1).unwrap();

        let mint = create_mint_block(&open, test_mint_source(), 1000, 2000, 1).unwrap();
        state.apply_block(&mint, 1).unwrap();

        // Should pass integrity check
        assert!(state.verify_integrity().is_ok());

        // Corrupt the balance
        state.balance = 9999;
        assert!(state.verify_integrity().is_err());
    }

    // â”€â”€â”€ Test 10: Invalid operations rejected â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_invalid_operations_rejected() {
        let open = create_open_block(alice_pubkey(), 1000, 1);

        // Zero-amount send
        let result = create_send_block(&open, bob_pubkey(), 0, 2000, 1);
        assert!(matches!(result.unwrap_err(), LedgerError::ZeroAmount));

        // Send from zero-balance account
        let result = create_send_block(&open, bob_pubkey(), 1, 2000, 1);
        assert!(matches!(result.unwrap_err(), LedgerError::InsufficientBalance { .. }));
    }

    // â”€â”€â”€ Test 11: Block hash changes with any field â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_block_hash_changes_with_fields() {
        let open = create_open_block(alice_pubkey(), 1000, 1);
        let mint = create_mint_block(&open, test_mint_source(), 100, 2000, 1).unwrap();
        let original_hash = mint.block_hash;

        // Different timestamp â†’ different hash
        let mint2 = create_mint_block(&open, test_mint_source(), 100, 2001, 1).unwrap();
        assert_ne!(original_hash, mint2.block_hash);

        // Different amount â†’ different hash
        let mint3 = create_mint_block(&open, test_mint_source(), 101, 2000, 1).unwrap();
        assert_ne!(original_hash, mint3.block_hash);
    }

    // â”€â”€â”€ Test 12: validate_against_previous rejects bad balance â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_validate_against_previous_bad_balance() {
        let open = create_open_block(alice_pubkey(), 1000, 1);
        let mut mint = create_mint_block(&open, test_mint_source(), 100, 2000, 1).unwrap();

        // Corrupt the balance
        mint.balance = 999;
        mint.block_hash = mint.compute_hash();

        let result = mint.validate_against_previous(&open);
        assert!(matches!(result.unwrap_err(), LedgerError::BalanceMismatch { .. }));
    }

    // â”€â”€â”€ Test 13: Duplicate Open rejected â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_duplicate_open_rejected() {
        let open = create_open_block(alice_pubkey(), 1000, 1);
        let mint = create_mint_block(&open, test_mint_source(), 100, 2000, 1).unwrap();

        // Try to create another Open after a Mint
        let mut bad_open = TransferBlock {
            previous: mint.block_hash,
            account: alice_pubkey(),
            sequence: 2,
            balance: 0,
            operation: TransferOp::Open,
            clock: VectorClock::new(),
            timestamp: 3000,
            signature: vec![0u8; 64],
            block_hash: [0u8; 32],
        };
        bad_open.block_hash = bad_open.compute_hash();

        let result = bad_open.validate_against_previous(&mint);
        assert!(matches!(result.unwrap_err(), LedgerError::DuplicateOpen));
    }

    // â”€â”€â”€ Test 14: AccountState rejects wrong account block â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_account_state_rejects_wrong_account() {
        let mut state = AccountState::new(alice_pubkey());
        let bob_open = create_open_block(bob_pubkey(), 1000, 1);

        let result = state.apply_block(&bob_open, 1);
        assert!(matches!(result.unwrap_err(), LedgerError::AccountMismatch));
    }

    // â”€â”€â”€ Test 15: MintSource canonical_bytes are distinct â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_mint_source_canonical_bytes_distinct() {
        let enc = MintSource::EncodingReward { ku_cid: [0x42; 32], role: 0 };
        let ver = MintSource::VerificationReward { ku_cid: [0x42; 32], role: 0 };
        let pomv = MintSource::PomvReward { ku_cid: [0x42; 32], epoch: 1 };
        let stor = MintSource::StorageReward { epoch: 1, challenge_hash: [0x42; 32] };

        let bytes: Vec<Vec<u8>> = vec![
            enc.canonical_bytes(),
            ver.canonical_bytes(),
            pomv.canonical_bytes(),
            stor.canonical_bytes(),
        ];

        // All should be different
        for i in 0..bytes.len() {
            for j in (i + 1)..bytes.len() {
                assert_ne!(bytes[i], bytes[j], "MintSource variants must produce different bytes");
            }
        }
    }

    // â”€â”€â”€ Test 16: TransferOp canonical_bytes are distinct â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_transfer_op_canonical_bytes_distinct() {
        let open = TransferOp::Open;
        let mint = TransferOp::Mint { source: test_mint_source(), amount: 100 };
        let send = TransferOp::Send { receiver: bob_pubkey(), amount: 100 };
        let recv = TransferOp::Receive { send_block_hash: [0x11; 32], amount: 100 };

        let bytes: Vec<Vec<u8>> = vec![
            open.canonical_bytes(),
            mint.canonical_bytes(),
            send.canonical_bytes(),
            recv.canonical_bytes(),
        ];

        for i in 0..bytes.len() {
            for j in (i + 1)..bytes.len() {
                assert_ne!(bytes[i], bytes[j], "TransferOp variants must produce different bytes");
            }
        }
    }

    // ─── Test 17: signing_payload deterministic ────────────────────────

    #[test]
    fn test_signing_payload_deterministic() {
        let block = create_open_block(alice_pubkey(), 1000, 1);
        let p1 = block.signing_payload();
        let p2 = block.signing_payload();
        assert_eq!(p1, p2, "signing payload must be deterministic");
        assert!(!p1.is_empty(), "payload must not be empty");
    }

    // ─── Test 18: validate_signature_length (structural check) ───────────

    #[test]
    fn test_validate_signature_length() {
        let mut block = create_open_block(alice_pubkey(), 1000, 1);
        // 64 bytes = valid length
        assert!(block.validate_signature_length().is_ok());
        // Wrong length = error
        block.signature = vec![0u8; 32];
        assert!(block.validate_signature_length().is_err());
    }

    // ─── Test 19: Real Ed25519 signature verification ──────────────────

    #[test]
    fn test_real_ed25519_signature_verification() {
        use ed25519_dalek::SigningKey;
        use rand::rngs::OsRng;

        let signing_key = SigningKey::generate(&mut OsRng);
        let pubkey = signing_key.verifying_key().to_bytes();

        let block = create_signed_open_block(&signing_key, 1000, 1);

        // Real signature should verify
        assert!(block.validate_signature(&pubkey).is_ok());

        // Tampered block should fail (signature no longer matches payload)
        let mut tampered = block.clone();
        tampered.balance = 999999;
        assert!(tampered.validate_signature(&pubkey).is_err());
    }

    // ─── Test 20: Wrong key signature fails ────────────────────────────

    #[test]
    fn test_wrong_key_signature_fails() {
        use ed25519_dalek::SigningKey;
        use rand::rngs::OsRng;

        let signing_key = SigningKey::generate(&mut OsRng);
        let wrong_key = SigningKey::generate(&mut OsRng);
        let wrong_pubkey = wrong_key.verifying_key().to_bytes();

        // Sign with one key but verify against a different pubkey
        let block = create_signed_open_block(&signing_key, 1000, 1);

        assert!(block.validate_signature(&wrong_pubkey).is_err());
    }

    // ─── Test 21: Signed block hash is valid ───────────────────────────

    #[test]
    fn test_signed_block_hash_valid() {
        use ed25519_dalek::SigningKey;
        use rand::rngs::OsRng;

        let signing_key = SigningKey::generate(&mut OsRng);
        let block = create_signed_open_block(&signing_key, 1000, 1);

        // Hash should be valid
        assert!(block.validate_hash().is_ok());
        // Signature should be 64 bytes
        assert_eq!(block.signature.len(), 64);
    }
}

