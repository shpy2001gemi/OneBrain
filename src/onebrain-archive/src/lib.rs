//! Bounded, authenticated Base archive primitives.
//!
//! `OBARV001` is accepted only for authenticated legacy inspection. New Base
//! archives use `OBARV002` and must pass through the owned verified-spool token
//! before any logical restore sink can observe plaintext.

mod container;
mod crypto;
mod limits;
mod verified;

use thiserror::Error;

pub use container::{
    inspect_legacy_archive_v1, seal_archive, ArchiveInspection, LegacyArchiveInspection,
    OBAR_V1_MAGIC, OBAR_V2_MAGIC,
};
pub use crypto::{ArchiveCredential, ArchiveCredentialKind, RecoveryKey};
pub use limits::{ArchiveKdfProfileV1, ArchiveLimits, ARCHIVE_CHUNK_BYTES};
pub use verified::{
    verify_dataset_archive_v2, EncryptedSpoolCapability, FileSecureSpoolFactory,
    LogicalRestoreSink, SecureSpoolFactory, VerifiedDatasetArchiveV2, VerifiedMaterialization,
};

#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error("archive I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("archive JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("archive credential is invalid")]
    InvalidCredential,
    #[error("archive version, profile, or canonical encoding is invalid")]
    InvalidProfile,
    #[error("archive is malformed or truncated")]
    Malformed,
    #[error("archive authentication failed")]
    Authentication,
    #[error("archive length, count, or digest does not match")]
    Integrity,
    #[error("archive exceeds a frozen resource limit")]
    Limit,
    #[error("archive contains unbound trailing bytes")]
    TrailingBytes,
    #[error("verified spool handle no longer matches its bound bytes")]
    HandleBinding,
    #[error("secure spool path is unsafe or was replaced")]
    UnsafeSpool,
    #[error("secure spool cleanup failed: {0}")]
    CleanupFailed(String),
    #[error("logical restore sink rejected verified plaintext: {0}")]
    RestoreSink(String),
}
