//! # ku-net Error Types
//!
//! Unified error hierarchy for all ku-net modules.

use std::fmt;

/// Top-level ku-net error.
#[derive(Debug)]
pub enum NetError {
    Identity(IdentityError),
    Message(crate::messages::MessageError),
    Membership(MembershipError),
    Bootstrap(BootstrapError),
    Transport(TransportError),
    Encoding(EncodingError),
    Obt(ObtError),
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Identity(e) => write!(f, "Identity: {}", e),
            Self::Message(e) => write!(f, "Message: {}", e),
            Self::Membership(e) => write!(f, "Membership: {}", e),
            Self::Bootstrap(e) => write!(f, "Bootstrap: {}", e),
            Self::Transport(e) => write!(f, "Transport: {}", e),
            Self::Encoding(e) => write!(f, "Encoding: {}", e),
            Self::Obt(e) => write!(f, "OBT: {}", e),
        }
    }
}

impl std::error::Error for NetError {}

// ─── Identity Errors ──────────────────────────────────────────────────────

/// Errors from identity/crypto operations.
#[derive(Debug)]
pub enum IdentityError {
    /// Crypto puzzle exceeded max iterations without finding solution.
    PuzzleTimeout { max_iterations: u64 },
    /// Difficulty value is out of valid range (0-32).
    InvalidDifficulty(u8),
    /// Invalid public key bytes.
    InvalidPublicKey,
    /// Signature verification failed.
    SignatureInvalid,
}

impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PuzzleTimeout { max_iterations } => write!(
                f,
                "Crypto puzzle timed out after {} iterations",
                max_iterations
            ),
            Self::InvalidDifficulty(d) => write!(f, "Invalid puzzle difficulty: {} (max 32)", d),
            Self::InvalidPublicKey => write!(f, "Invalid Ed25519 public key"),
            Self::SignatureInvalid => write!(f, "Ed25519 signature verification failed"),
        }
    }
}

impl std::error::Error for IdentityError {}

// ─── Membership Errors ────────────────────────────────────────────────────

/// Errors from SWIM membership operations.
#[derive(Debug)]
pub enum MembershipError {
    /// Membership list at capacity, cannot add new member.
    CapacityExceeded { capacity: usize },
    /// Incarnation number is stale (lower than current).
    StaleIncarnation { current: u32, received: u32 },
    /// Unknown node ID.
    UnknownNode,
}

impl fmt::Display for MembershipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapacityExceeded { capacity } => {
                write!(f, "Membership list at capacity: {}", capacity)
            }
            Self::StaleIncarnation { current, received } => write!(
                f,
                "Stale incarnation: current={}, received={}",
                current, received
            ),
            Self::UnknownNode => write!(f, "Unknown node ID"),
        }
    }
}

impl std::error::Error for MembershipError {}

// ─── Bootstrap Errors ─────────────────────────────────────────────────────

/// Errors from bootstrap/discovery operations.
#[derive(Debug)]
pub enum BootstrapError {
    /// Bootstrap already in progress.
    AlreadyRunning,
    /// All 6 bootstrap layers failed.
    AllLayersFailed,
    /// Specific layer timed out.
    LayerTimeout { layer: &'static str },
    /// Not enough peers found.
    InsufficientPeers { found: usize, required: usize },
}

impl fmt::Display for BootstrapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyRunning => write!(f, "Bootstrap already in progress"),
            Self::AllLayersFailed => write!(f, "All 6 bootstrap layers failed"),
            Self::LayerTimeout { layer } => write!(f, "Bootstrap layer timed out: {}", layer),
            Self::InsufficientPeers { found, required } => {
                write!(f, "Insufficient peers: found {}, need {}", found, required)
            }
        }
    }
}

impl std::error::Error for BootstrapError {}

// ─── Transport Errors ─────────────────────────────────────────────────────

/// Errors from QUIC transport operations (Phase 5.6).
#[derive(Debug)]
pub enum TransportError {
    /// Failed to bind to address.
    BindFailed(String),
    /// Connection failed.
    ConnectionFailed(String),
    /// Send failed.
    SendFailed(String),
    /// Receive failed.
    RecvFailed(String),
    /// Connection timed out.
    Timeout,
    /// TLS/certificate error.
    TlsError(String),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BindFailed(e) => write!(f, "Bind failed: {}", e),
            Self::ConnectionFailed(e) => write!(f, "Connection failed: {}", e),
            Self::SendFailed(e) => write!(f, "Send failed: {}", e),
            Self::RecvFailed(e) => write!(f, "Receive failed: {}", e),
            Self::Timeout => write!(f, "Connection timed out"),
            Self::TlsError(e) => write!(f, "TLS error: {}", e),
        }
    }
}

impl std::error::Error for TransportError {}

// ── Encoding Errors ───────────────────────────────────────────────────

/// Errors from encoding consensus operations.
#[derive(Debug)]
pub enum EncodingError {
    /// Encoding job not found on DHT.
    JobNotFound,
    /// Claim request was rejected.
    ClaimRejected(String),
    /// Consensus timed out (not enough verifiers).
    ConsensusTimeout,
    /// Invalid or expired claim token.
    InvalidClaimToken,
    /// Verification failed.
    VerificationFailed(String),
    /// Job has expired (TTL exceeded).
    JobExpired,
}

impl fmt::Display for EncodingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::JobNotFound => write!(f, "Encoding job not found on DHT"),
            Self::ClaimRejected(reason) => write!(f, "Claim rejected: {}", reason),
            Self::ConsensusTimeout => write!(f, "Consensus timed out (not enough verifiers)"),
            Self::InvalidClaimToken => write!(f, "Invalid or expired claim token"),
            Self::VerificationFailed(reason) => write!(f, "Verification failed: {}", reason),
            Self::JobExpired => write!(f, "Job expired (TTL exceeded)"),
        }
    }
}

impl std::error::Error for EncodingError {}

// ─── OBT Errors ───────────────────────────────────────────────────────────

/// Errors from OBT token operations.
#[derive(Debug)]
pub enum ObtError {
    /// OBT transfer failed validation.
    TransferFailed(String),
    /// Insufficient balance for transfer.
    InsufficientBalance { required: u64, available: u64 },
    /// Fork detected for an account.
    ForkDetected { offender: [u8; 32] },
    /// Node is under penalty (jailed/banned), operation blocked.
    PenaltyActive(String),
    /// Storage challenge response timed out.
    StorageChallengeTimeout,
    /// Mint proof validation failed.
    MintValidationFailed(String),
}

impl fmt::Display for ObtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TransferFailed(reason) => write!(f, "Transfer failed: {}", reason),
            Self::InsufficientBalance {
                required,
                available,
            } => write!(
                f,
                "Insufficient balance: required {} milliOBT, available {}",
                required, available
            ),
            Self::ForkDetected { offender } => write!(
                f,
                "Fork detected for {:02x}{:02x}...",
                offender[0], offender[1]
            ),
            Self::PenaltyActive(reason) => write!(f, "Penalty active: {}", reason),
            Self::StorageChallengeTimeout => write!(f, "Storage challenge response timed out"),
            Self::MintValidationFailed(reason) => write!(f, "Mint validation failed: {}", reason),
        }
    }
}

impl std::error::Error for ObtError {}

impl From<ObtError> for NetError {
    fn from(e: ObtError) -> Self {
        NetError::Obt(e)
    }
}

// ─── Conversions ──────────────────────────────────────────────────────────

impl From<IdentityError> for NetError {
    fn from(e: IdentityError) -> Self {
        NetError::Identity(e)
    }
}

impl From<crate::messages::MessageError> for NetError {
    fn from(e: crate::messages::MessageError) -> Self {
        NetError::Message(e)
    }
}

impl From<MembershipError> for NetError {
    fn from(e: MembershipError) -> Self {
        NetError::Membership(e)
    }
}

impl From<BootstrapError> for NetError {
    fn from(e: BootstrapError) -> Self {
        NetError::Bootstrap(e)
    }
}

impl From<TransportError> for NetError {
    fn from(e: TransportError) -> Self {
        NetError::Transport(e)
    }
}

impl From<EncodingError> for NetError {
    fn from(e: EncodingError) -> Self {
        NetError::Encoding(e)
    }
}
