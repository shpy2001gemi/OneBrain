use crate::ArchiveError;

pub const ARCHIVE_CHUNK_BYTES: usize = 1024 * 1024;

pub const HARD_MAX_ENTRIES: u32 = 1_000_000;
pub const HARD_MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;
pub const HARD_MAX_ENTRY_BYTES: u64 = 16 * 1024 * 1024 * 1024;
pub const HARD_MAX_TOTAL_PLAINTEXT_BYTES: u64 = 64 * 1024 * 1024 * 1024;
pub const HARD_MAX_SPOOL_BYTES: u64 = 72 * 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArchiveKdfProfileV1 {
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

impl ArchiveKdfProfileV1 {
    pub const PASSWORD: Self = Self {
        memory_kib: 65_536,
        iterations: 3,
        parallelism: 1,
    };

    pub(crate) const RECOVERY_KEY: Self = Self {
        memory_kib: 0,
        iterations: 0,
        parallelism: 0,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArchiveLimits {
    pub max_entries: u32,
    pub max_manifest_bytes: u64,
    pub max_entry_bytes: u64,
    pub max_total_plaintext_bytes: u64,
    pub max_spool_bytes: u64,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            max_entries: 65_536,
            max_manifest_bytes: 1024 * 1024,
            max_entry_bytes: 4 * 1024 * 1024 * 1024,
            max_total_plaintext_bytes: 16 * 1024 * 1024 * 1024,
            max_spool_bytes: 18 * 1024 * 1024 * 1024,
        }
    }
}

impl ArchiveLimits {
    pub(crate) fn validate(self) -> Result<(), ArchiveError> {
        if self.max_entries == 0
            || self.max_entries > HARD_MAX_ENTRIES
            || self.max_manifest_bytes < 16
            || self.max_manifest_bytes > HARD_MAX_MANIFEST_BYTES
            || self.max_entry_bytes == 0
            || self.max_entry_bytes > HARD_MAX_ENTRY_BYTES
            || self.max_total_plaintext_bytes == 0
            || self.max_total_plaintext_bytes > HARD_MAX_TOTAL_PLAINTEXT_BYTES
            || self.max_spool_bytes == 0
            || self.max_spool_bytes > HARD_MAX_SPOOL_BYTES
            || self.max_entry_bytes > self.max_total_plaintext_bytes
        {
            return Err(ArchiveError::Limit);
        }
        Ok(())
    }

    pub(crate) fn max_chunks(self) -> u64 {
        self.max_total_plaintext_bytes
            .div_ceil(ARCHIVE_CHUNK_BYTES as u64)
            .min(self.max_entries as u64)
    }
}
