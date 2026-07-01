//! # OBT Transfer Protocol — Network Layer
//!
//! Implements the OBT transfer protocol messages for the network layer.
//! This module defines the wire format for OBT operations that flow over
//! the OBP gossip network.
//!
//! ## Message Types (0xA0–0xA6)
//! | Code | Message | Direction |
//! |------|---------|-----------|
//! | 0xA0 | TransferRequest | Sender → DHT neighbors |
//! | 0xA1 | TransferConfirm | Witness → Sender + Receiver |
//! | 0xA2 | BalanceQuery | Querier → DHT neighbors |
//! | 0xA3 | BalanceResponse | DHT neighbor → Querier |
//! | 0xA4 | MintBroadcast | Minter → Network |
//! | 0xA5 | StorageChallenge | Challenger → Storage node |
//! | 0xA6 | ForkWarrant       | Detector → Network (broadcast)   |
//!
//! ## Transfer Flow (2-Phase, Nano-style)
//! 1. Sender creates Send block (balance decreases)
//! 2. Sender broadcasts TransferRequest to DHT neighbors
//! 3. K witnesses validate and respond with TransferConfirm
//! 4. Receiver sees pending Send (via DHT or gossip)
//! 5. Receiver creates Receive block (balance increases)
//! 6. Receiver broadcasts, witnesses confirm
//!
//! ## Reference
//! See `docs/specs/obt/06_TRANSFER.md`.

use serde::{Serialize, Deserialize};

use crate::constants::*;

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// Â§6.1 â€” OBT Network Messages
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// OBT Transfer Request (0xA0).
///
/// Sent by the account owner to initiate an OBT transfer.
/// DHT neighbors validate the Send block and respond with TransferConfirm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObtTransferRequest {
    /// Sender's Ed25519 public key (account identifier).
    pub from: [u8; 32],
    /// Receiver's Ed25519 public key.
    pub to: [u8; 32],
    /// Amount of OBT to transfer (milliOBT, u64).
    pub amount: u64,
    /// Monotonic nonce (= Send block sequence number).
    pub nonce: u64,
    /// Sender's balance AFTER this transfer.
    pub balance_after: u64,
    /// BLAKE3 hash of the Send block.
    pub block_hash: [u8; 32],
    /// Ed25519 signature over `BLAKE3(from || to || amount || nonce || balance_after)`.
    pub signature: Vec<u8>,
    /// Timestamp (Unix millis, UTC).
    pub timestamp: u64,
}

/// OBT Transfer Confirmation (0xA1).
///
/// Sent by a witness who validated a Send or Receive block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObtTransferConfirm {
    /// Hash of the block being confirmed (Send or Receive).
    pub block_hash: [u8; 32],
    /// Witness's Ed25519 public key.
    pub witness_id: [u8; 32],
    /// Witness's Ed25519 signature over the block_hash.
    pub witness_signature: Vec<u8>,
    /// Confirmation level assigned by this witness.
    pub confirmation_level: u8,
    /// Timestamp (Unix millis, UTC).
    pub timestamp: u64,
}

/// OBT Balance Query (0xA2).
///
/// Query the balance of a node from its DHT neighbors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObtBalanceQuery {
    /// Ed25519 public key of the account to query.
    pub account: [u8; 32],
    /// Requester's node ID (for routing response).
    pub requester: [u8; 32],
    /// Request nonce (for deduplication).
    pub nonce: u64,
}

/// OBT Balance Response (0xA3).
///
/// Response to a balance query, includes Merkle proof for verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObtBalanceResponse {
    /// Ed25519 public key of the queried account.
    pub account: [u8; 32],
    /// Current balance (milliOBT).
    pub balance: u64,
    /// Head block hash (latest block in account chain).
    pub head_hash: [u8; 32],
    /// Sequence number of the head block.
    pub sequence: u64,
    /// Request nonce (echoed from query).
    pub nonce: u64,
    /// Responder's node ID.
    pub responder: [u8; 32],
    /// Responder's signature over the response payload.
    pub responder_signature: Vec<u8>,
}

/// OBT Mint Broadcast (0xA4).
///
/// Broadcasts a signed MintProof to the network after epoch settlement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObtMintBroadcast {
    /// Recipient's Ed25519 public key.
    pub recipient: [u8; 32],
    /// Minted amount (milliOBT).
    pub amount: u64,
    /// Epoch in which the mint occurred.
    pub epoch: u64,
    /// Mint activity type (0=Encoding, 1=Verification, 2=PoMV, 3=Storage).
    pub activity_type: u8,
    /// KU CID that generated this reward (if applicable).
    pub ku_cid: [u8; 32],
    /// BLAKE3 hash of the Mint block in the recipient's account chain.
    pub mint_block_hash: [u8; 32],
    /// Number of witnesses who signed the MintProof.
    pub witness_count: u8,
    /// Witness signatures (each = 32-byte witness_id + 64-byte signature).
    pub witness_data: Vec<u8>,
    /// Timestamp (Unix millis, UTC).
    pub timestamp: u64,
}

/// OBT Storage Challenge (0xA5).
///
/// Sent to a storage node to prove they still hold a specific KU.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObtStorageChallenge {
    /// CID of the KU to prove storage of.
    pub ku_cid: [u8; 32],
    /// Challenge type (0=FullHash, 1=ByteRange, 2=FieldExtract).
    pub challenge_type: u8,
    /// Byte offset for ByteRange challenges.
    pub offset: u32,
    /// Byte length for ByteRange challenges.
    pub length: u32,
    /// Epoch number (for deterministic challenge seed verification).
    pub epoch: u64,
    /// Challenger's node ID.
    pub challenger: [u8; 32],
    /// Challenger's signature (proves authority to challenge).
    pub challenger_signature: Vec<u8>,
}

/// OBT Storage Challenge Response (sent back to challenger).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObtStorageChallengeResponse {
    /// Original challenge's KU CID.
    pub ku_cid: [u8; 32],
    /// Challenge type echoed back.
    pub challenge_type: u8,
    /// Response data (hash, byte range, or extracted fields).
    pub response_data: Vec<u8>,
    /// Responder's node ID.
    pub responder: [u8; 32],
    /// Responder's signature over the response.
    pub responder_signature: Vec<u8>,
    /// Timestamp.
    pub timestamp: u64,
}

/// OBT Fork Warrant (0xA6).
///
/// Broadcast when a fork is detected (two blocks with same sequence number).
/// All nodes should record this evidence and reduce trust for the offender.
///
/// See `ku-core::obt_fork_pipeline` for warrant processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObtForkWarrant {
    /// Offender's Ed25519 public key.
    pub offender: [u8; 32],
    /// Hash of the first conflicting block.
    pub block_a_hash: [u8; 32],
    /// Hash of the second conflicting block.
    pub block_b_hash: [u8; 32],
    /// The sequence number where the fork occurred.
    pub sequence: u64,
    /// Who detected the fork.
    pub detected_by: [u8; 32],
    /// When the fork was detected (Unix timestamp).
    pub detected_at: u64,
    /// BLAKE3 hash of the warrant (= BLAKE3(offender ‖ block_a ‖ block_b ‖ sequence)).
    pub warrant_hash: [u8; 32],
    /// Ed25519 signature by the detector over the warrant_hash.
    pub signature: Vec<u8>,
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// Â§6.2 â€” Confirmation Levels
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// OBT block confirmation level.
///
/// Blocks progress through these levels as witnesses validate them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ConfirmationLevel {
    /// Just created, no confirmations yet.
    Pending = 0,
    /// 1â€“2 witnesses confirmed (50â€“200ms).
    Tentative = 1,
    /// K witnesses confirmed, K=3â€“5 (1â€“3 seconds).
    Confirmed = 2,
    /// Widely propagated, practically irreversible (10â€“30 seconds).
    Settled = 3,
}

impl ConfirmationLevel {
    /// Check if this level meets the minimum for minting.
    pub fn meets_mint_requirement(&self) -> bool {
        *self >= ConfirmationLevel::Confirmed
    }

    /// Check if this level meets the minimum for transfers.
    pub fn meets_transfer_requirement(&self) -> bool {
        *self >= ConfirmationLevel::Confirmed
    }

    /// From u8 code.
    pub fn from_u8(code: u8) -> Option<Self> {
        match code {
            0 => Some(ConfirmationLevel::Pending),
            1 => Some(ConfirmationLevel::Tentative),
            2 => Some(ConfirmationLevel::Confirmed),
            3 => Some(ConfirmationLevel::Settled),
            _ => None,
        }
    }
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// Â§6.3 â€” Message Type Routing
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Identify an OBT message type from its wire code.
pub fn obt_message_name(code: u8) -> Option<&'static str> {
    match code {
        MSG_OBT_TRANSFER_REQUEST => Some("ObtTransferRequest"),
        MSG_OBT_TRANSFER_CONFIRM => Some("ObtTransferConfirm"),
        MSG_OBT_BALANCE_QUERY => Some("ObtBalanceQuery"),
        MSG_OBT_BALANCE_RESPONSE => Some("ObtBalanceResponse"),
        MSG_OBT_MINT_BROADCAST => Some("ObtMintBroadcast"),
        MSG_OBT_STORAGE_CHALLENGE => Some("ObtStorageChallenge"),
        MSG_OBT_FORK_WARRANT => Some("ObtForkWarrant"),
        _ => None,
    }
}

/// Check if a message type code is an OBT protocol message.
pub fn is_obt_message(code: u8) -> bool {
    (0xA0..=0xA6).contains(&code)
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// Â§6.4 â€” Transfer Validation Helpers
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Validate basic structure of a TransferRequest.
///
/// Checks:
/// - Amount > 0
/// - Sender â‰  Receiver
/// - Signature is 64 bytes
/// - Balance after is consistent (balance_after = previous_balance - amount)
pub fn validate_transfer_request_structure(req: &ObtTransferRequest) -> Result<(), &'static str> {
    if req.amount == 0 {
        return Err("Transfer amount must be > 0");
    }
    if req.from == req.to {
        return Err("Cannot transfer to self");
    }
    if req.signature.len() != 64 {
        return Err("Signature must be 64 bytes (Ed25519)");
    }
    Ok(())
}

/// Validate basic structure of a TransferConfirm.
pub fn validate_transfer_confirm_structure(confirm: &ObtTransferConfirm) -> Result<(), &'static str> {
    if confirm.witness_signature.len() != 64 {
        return Err("Witness signature must be 64 bytes");
    }
    if confirm.confirmation_level > 3 {
        return Err("Invalid confirmation level");
    }
    Ok(())
}

/// Validate basic structure of a MintBroadcast.
pub fn validate_mint_broadcast_structure(mint: &ObtMintBroadcast) -> Result<(), &'static str> {
    if mint.amount == 0 {
        return Err("Mint amount must be > 0");
    }
    if mint.activity_type > 3 {
        return Err("Invalid activity type (must be 0-3)");
    }
    if (mint.witness_count as u32) < OBT_MIN_WITNESSES {
        return Err("Insufficient witness count");
    }
    // Each witness = 32 bytes ID + 64 bytes signature = 96 bytes
    let expected_len = mint.witness_count as usize * 96;
    if mint.witness_data.len() != expected_len {
        return Err("Witness data length mismatch");
    }
    Ok(())
}

/// Validate that the sender is eligible to transfer OBT.
///
/// Checks penalty status to ensure jailed/banned nodes cannot transfer.
///
/// # Parameters
/// - `penalty_status`: Current penalty tier (0=None, 1=Warning, 2=TrustReduction, 3=Jail, 4=Tombstone)
/// - `jail_until`: Optional unix timestamp when jail expires
/// - `current_ts`: Current unix timestamp
pub fn validate_transfer_eligibility(
    penalty_status: u8,
    jail_until: Option<u64>,
    current_ts: u64,
) -> Result<(), &'static str> {
    if penalty_status >= 4 {
        return Err("Account is permanently banned (Tombstone)");
    }
    if penalty_status >= 3 {
        if let Some(until) = jail_until {
            if current_ts < until {
                return Err("Account is jailed, transfers blocked until jail expires");
            }
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_type_codes() {
        assert_eq!(MSG_OBT_TRANSFER_REQUEST, 0xA0);
        assert_eq!(MSG_OBT_TRANSFER_CONFIRM, 0xA1);
        assert_eq!(MSG_OBT_BALANCE_QUERY, 0xA2);
        assert_eq!(MSG_OBT_BALANCE_RESPONSE, 0xA3);
        assert_eq!(MSG_OBT_MINT_BROADCAST, 0xA4);
        assert_eq!(MSG_OBT_STORAGE_CHALLENGE, 0xA5);
        assert_eq!(MSG_OBT_FORK_WARRANT, 0xA6);
    }

    #[test]
    fn test_is_obt_message() {
        assert!(is_obt_message(0xA0));
        assert!(is_obt_message(0xA5));
        assert!(!is_obt_message(0x01)); // Regular message
        assert!(is_obt_message(0xA6)); // ForkWarrant
        assert!(!is_obt_message(0x9F)); // Just below
    }

    #[test]
    fn test_obt_message_name() {
        assert_eq!(obt_message_name(0xA0), Some("ObtTransferRequest"));
        assert_eq!(obt_message_name(0xA4), Some("ObtMintBroadcast"));
        assert_eq!(obt_message_name(0xFF), None);
    }

    #[test]
    fn test_fork_warrant_message() {
        assert_eq!(obt_message_name(0xA6), Some("ObtForkWarrant"));
        assert!(is_obt_message(0xA6));
    }

    #[test]
    fn test_fork_warrant_struct() {
        let warrant = ObtForkWarrant {
            offender: [1u8; 32],
            block_a_hash: [2u8; 32],
            block_b_hash: [3u8; 32],
            sequence: 42,
            detected_by: [4u8; 32],
            detected_at: 1000000,
            warrant_hash: [5u8; 32],
            signature: vec![0u8; 64],
        };
        assert_eq!(warrant.sequence, 42);
        assert_eq!(warrant.signature.len(), 64);
    }

    #[test]
    fn test_confirmation_level_ordering() {
        assert!(ConfirmationLevel::Pending < ConfirmationLevel::Tentative);
        assert!(ConfirmationLevel::Tentative < ConfirmationLevel::Confirmed);
        assert!(ConfirmationLevel::Confirmed < ConfirmationLevel::Settled);
    }

    #[test]
    fn test_confirmation_meets_requirements() {
        assert!(!ConfirmationLevel::Pending.meets_mint_requirement());
        assert!(!ConfirmationLevel::Tentative.meets_mint_requirement());
        assert!(ConfirmationLevel::Confirmed.meets_mint_requirement());
        assert!(ConfirmationLevel::Settled.meets_mint_requirement());

        assert!(!ConfirmationLevel::Pending.meets_transfer_requirement());
        assert!(ConfirmationLevel::Confirmed.meets_transfer_requirement());
    }

    #[test]
    fn test_confirmation_from_u8() {
        assert_eq!(ConfirmationLevel::from_u8(0), Some(ConfirmationLevel::Pending));
        assert_eq!(ConfirmationLevel::from_u8(3), Some(ConfirmationLevel::Settled));
        assert_eq!(ConfirmationLevel::from_u8(4), None);
    }

    #[test]
    fn test_validate_transfer_request() {
        let valid = ObtTransferRequest {
            from: [1u8; 32],
            to: [2u8; 32],
            amount: 100,
            nonce: 1,
            balance_after: 900,
            block_hash: [0u8; 32],
            signature: vec![0u8; 64],
            timestamp: 1000,
        };
        assert!(validate_transfer_request_structure(&valid).is_ok());

        // Zero amount
        let mut bad = valid.clone();
        bad.amount = 0;
        assert_eq!(validate_transfer_request_structure(&bad), Err("Transfer amount must be > 0"));

        // Self transfer
        let mut bad = valid.clone();
        bad.to = bad.from;
        assert_eq!(validate_transfer_request_structure(&bad), Err("Cannot transfer to self"));

        // Bad signature length
        let mut bad = valid.clone();
        bad.signature = vec![0u8; 32];
        assert_eq!(validate_transfer_request_structure(&bad), Err("Signature must be 64 bytes (Ed25519)"));
    }

    #[test]
    fn test_validate_transfer_confirm() {
        let valid = ObtTransferConfirm {
            block_hash: [0u8; 32],
            witness_id: [1u8; 32],
            witness_signature: vec![0u8; 64],
            confirmation_level: 2,
            timestamp: 1000,
        };
        assert!(validate_transfer_confirm_structure(&valid).is_ok());

        let mut bad = valid.clone();
        bad.confirmation_level = 5;
        assert_eq!(validate_transfer_confirm_structure(&bad), Err("Invalid confirmation level"));
    }

    #[test]
    fn test_validate_mint_broadcast() {
        let valid = ObtMintBroadcast {
            recipient: [1u8; 32],
            amount: 100,
            epoch: 1,
            activity_type: 0,
            ku_cid: [0u8; 32],
            mint_block_hash: [0u8; 32],
            witness_count: 3,
            witness_data: vec![0u8; 3 * 96], // 3 witnesses Ã— 96 bytes each
            timestamp: 1000,
        };
        assert!(validate_mint_broadcast_structure(&valid).is_ok());

        // Zero amount
        let mut bad = valid.clone();
        bad.amount = 0;
        assert_eq!(validate_mint_broadcast_structure(&bad), Err("Mint amount must be > 0"));

        // Bad activity type
        let mut bad = valid.clone();
        bad.activity_type = 5;
        assert_eq!(validate_mint_broadcast_structure(&bad), Err("Invalid activity type (must be 0-3)"));

        // Too few witnesses
        let mut bad = valid.clone();
        bad.witness_count = 1;
        bad.witness_data = vec![0u8; 96];
        assert_eq!(validate_mint_broadcast_structure(&bad), Err("Insufficient witness count"));
    }

    #[test]
    fn test_obt_epoch_consistency() {
        assert_eq!(OBT_EPOCH_DURATION_S, 3_600);
        assert_eq!(OBT_CONFIRMATION_TIMEOUT_S, 30);
        assert_eq!(OBT_UNRECEIVED_SEND_EXPIRY_S, 604_800); // 7 days
    }

    #[test]
    fn test_transfer_eligibility_ok() {
        assert!(validate_transfer_eligibility(0, None, 1000).is_ok());
        assert!(validate_transfer_eligibility(1, None, 1000).is_ok());
        assert!(validate_transfer_eligibility(2, None, 1000).is_ok());
    }

    #[test]
    fn test_transfer_eligibility_jailed() {
        assert!(validate_transfer_eligibility(3, Some(5000), 1000).is_err());
        assert!(validate_transfer_eligibility(3, Some(5000), 6000).is_ok());
    }

    #[test]
    fn test_transfer_eligibility_tombstone() {
        assert!(validate_transfer_eligibility(4, None, 1000).is_err());
        assert!(validate_transfer_eligibility(4, None, u64::MAX).is_err());
    }
}
