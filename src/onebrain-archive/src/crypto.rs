use std::fmt;

use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::Zeroizing;

use crate::limits::ArchiveKdfProfileV1;
use crate::ArchiveError;

const RECOVERY_KEY_CONTEXT_V2: &str = "onebrain:base:archive-recovery-key:2";
const LEGACY_KEY_CONTEXT: &str = "onebrain:mobile:portable-archive-key:1";
const MAX_PASSWORD_BYTES: usize = 1024;

pub struct RecoveryKey(Zeroizing<[u8; 32]>);

impl RecoveryKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self, ArchiveError> {
        if bytes.iter().all(|byte| *byte == 0) {
            return Err(ArchiveError::InvalidCredential);
        }
        Ok(Self(Zeroizing::new(bytes)))
    }

    pub(crate) fn duplicate(&self) -> Self {
        Self(Zeroizing::new(*self.0))
    }

    pub(crate) fn legacy_key(&self) -> Zeroizing<[u8; 32]> {
        Zeroizing::new(blake3::derive_key(LEGACY_KEY_CONTEXT, self.0.as_slice()))
    }
}

impl fmt::Debug for RecoveryKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RecoveryKey([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ArchiveCredentialKind {
    Password = 1,
    RecoveryKey = 2,
}

pub enum ArchiveCredential {
    Password(Zeroizing<Vec<u8>>),
    RecoveryKey(RecoveryKey),
}

impl ArchiveCredential {
    pub fn password(bytes: impl Into<Vec<u8>>) -> Result<Self, ArchiveError> {
        let bytes = bytes.into();
        if bytes.is_empty() || bytes.len() > MAX_PASSWORD_BYTES {
            return Err(ArchiveError::InvalidCredential);
        }
        Ok(Self::Password(Zeroizing::new(bytes)))
    }

    pub const fn kind(&self) -> ArchiveCredentialKind {
        match self {
            Self::Password(_) => ArchiveCredentialKind::Password,
            Self::RecoveryKey(_) => ArchiveCredentialKind::RecoveryKey,
        }
    }

    pub(crate) fn duplicate(&self) -> Self {
        match self {
            Self::Password(value) => Self::Password(Zeroizing::new(value.to_vec())),
            Self::RecoveryKey(value) => Self::RecoveryKey(value.duplicate()),
        }
    }

    pub(crate) fn derive_v2(
        &self,
        salt: &[u8; 16],
        profile: ArchiveKdfProfileV1,
    ) -> Result<Zeroizing<[u8; 32]>, ArchiveError> {
        match self {
            Self::Password(password) => {
                if password.is_empty() || password.len() > MAX_PASSWORD_BYTES {
                    return Err(ArchiveError::InvalidCredential);
                }
                if profile != ArchiveKdfProfileV1::PASSWORD {
                    return Err(ArchiveError::InvalidProfile);
                }
                let params = Params::new(
                    profile.memory_kib,
                    profile.iterations,
                    profile.parallelism,
                    Some(32),
                )
                .map_err(|_| ArchiveError::InvalidProfile)?;
                let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
                let mut output = Zeroizing::new([0u8; 32]);
                argon
                    .hash_password_into(password, salt, output.as_mut())
                    .map_err(|_| ArchiveError::InvalidCredential)?;
                Ok(output)
            }
            Self::RecoveryKey(recovery_key) => {
                if profile != ArchiveKdfProfileV1::RECOVERY_KEY {
                    return Err(ArchiveError::InvalidProfile);
                }
                let mut hasher = blake3::Hasher::new_derive_key(RECOVERY_KEY_CONTEXT_V2);
                hasher.update(salt);
                hasher.update(recovery_key.0.as_slice());
                Ok(Zeroizing::new(*hasher.finalize().as_bytes()))
            }
        }
    }
}

impl fmt::Debug for ArchiveCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Password(_) => formatter.write_str("ArchiveCredential::Password([REDACTED])"),
            Self::RecoveryKey(_) => {
                formatter.write_str("ArchiveCredential::RecoveryKey([REDACTED])")
            }
        }
    }
}
