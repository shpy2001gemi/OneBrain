//! Versioned filesystem layout and fail-closed migration for spilled blobs.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use ku_core::blob_store::{BlobCid, BlobMeta, BLOB_CHUNK_SIZE, BLOB_CID_VERSION, BLOB_MAX_SIZE};

use crate::blob_storage::BlobStorageError;

/// Current filesystem layout for spilled blob chunks.
pub const BLOB_LAYOUT_VERSION: u16 = 2;

/// Outcome of a complete layout-v2 migration attempt.
#[derive(Debug, Clone, Default)]
pub struct BlobLayoutMigrationReport {
    pub migrated: u64,
    pub already_v2: u64,
    pub collision_groups: Vec<String>,
    pub corrupt_cids: Vec<BlobCid>,
}

/// Return `v2/<digest-byte-0>/<digest-byte-1>/<full-typed-cid>`.
pub fn blob_relative_dir(cid: &BlobCid) -> PathBuf {
    let digest = cid.blake3_hash();
    PathBuf::from(format!("v{BLOB_LAYOUT_VERSION}"))
        .join(format!("{:02x}", digest[0]))
        .join(format!("{:02x}", digest[1]))
        .join(cid.to_hex())
}

/// Verify and migrate all filesystem-backed metadata records to layout v2.
///
/// Validation is a global preflight: no directory is moved when any legacy
/// prefix is ambiguous or any selected source directory fails full typed-CID
/// verification. A staged directory is safe to publish on an idempotent rerun.
pub fn migrate_blob_layout_v2(
    root: &Path,
    metas: &[BlobMeta],
) -> Result<BlobLayoutMigrationReport, BlobStorageError> {
    let entries = filesystem_entries(metas)?;
    let mut report = BlobLayoutMigrationReport::default();

    let mut legacy_claims: BTreeMap<String, Vec<BlobCid>> = BTreeMap::new();
    for entry in &entries {
        legacy_claims
            .entry(entry.cid.short_hex())
            .or_default()
            .push(entry.cid);
    }
    for (prefix, mut cids) in legacy_claims {
        cids.sort_by_key(|cid| cid.0);
        cids.dedup();
        if cids.len() > 1 && root.join(&prefix).exists() {
            report.collision_groups.push(prefix);
        }
    }
    if !report.collision_groups.is_empty() {
        return Err(BlobStorageError::MigrationBlocked(report));
    }

    let mut actions = Vec::with_capacity(entries.len());
    for entry in entries {
        let legacy = root.join(entry.cid.short_hex());
        let final_dir = root.join(blob_relative_dir(&entry.cid));
        let staging = migrating_dir(&final_dir);
        let legacy_exists = legacy.exists();
        let staging_exists = staging.exists();
        let final_exists = final_dir.exists();

        let action = match (legacy_exists, staging_exists, final_exists) {
            (false, false, true) => MigrationAction::AlreadyV2 {
                final_dir: final_dir.clone(),
            },
            (false, true, false) => MigrationAction::PublishStaging {
                staging: staging.clone(),
                final_dir,
            },
            (true, false, false) => MigrationAction::MigrateLegacy {
                legacy: legacy.clone(),
                staging,
                final_dir,
            },
            _ => {
                report.corrupt_cids.push(entry.cid);
                continue;
            }
        };

        let source = match &action {
            MigrationAction::AlreadyV2 { final_dir } => final_dir.as_path(),
            MigrationAction::PublishStaging { staging, .. } => staging.as_path(),
            MigrationAction::MigrateLegacy { legacy, .. } => legacy.as_path(),
        };
        if verify_blob_directory(source, &entry).is_err() {
            report.corrupt_cids.push(entry.cid);
            continue;
        }
        actions.push(action);
    }

    report.corrupt_cids.sort_by_key(|cid| cid.0);
    report.corrupt_cids.dedup();
    if !report.corrupt_cids.is_empty() {
        return Err(BlobStorageError::MigrationBlocked(report));
    }

    for action in actions {
        match action {
            MigrationAction::AlreadyV2 { .. } => report.already_v2 += 1,
            MigrationAction::PublishStaging { staging, final_dir } => {
                publish_staging(&staging, &final_dir)?;
                report.migrated += 1;
            }
            MigrationAction::MigrateLegacy {
                legacy,
                staging,
                final_dir,
            } => {
                let parent = final_dir.parent().ok_or_else(|| {
                    BlobStorageError::IoError("v2 blob directory has no parent".into())
                })?;
                fs::create_dir_all(parent)?;
                fs::rename(&legacy, &staging)?;
                publish_staging(&staging, &final_dir)?;
                report.migrated += 1;
            }
        }
    }

    Ok(report)
}

#[derive(Debug)]
struct MigrationEntry<'a> {
    cid: BlobCid,
    meta: &'a BlobMeta,
}

#[derive(Debug)]
enum MigrationAction {
    AlreadyV2 {
        final_dir: PathBuf,
    },
    PublishStaging {
        staging: PathBuf,
        final_dir: PathBuf,
    },
    MigrateLegacy {
        legacy: PathBuf,
        staging: PathBuf,
        final_dir: PathBuf,
    },
}

fn filesystem_entries(metas: &[BlobMeta]) -> Result<Vec<MigrationEntry<'_>>, BlobStorageError> {
    metas
        .iter()
        .filter(|meta| meta.storage_mode == "filesystem")
        .map(|meta| {
            if meta.blob_cid_hex.len() != 68
                || !meta
                    .blob_cid_hex
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(BlobStorageError::CodecError(format!(
                    "invalid canonical blob CID: {}",
                    meta.blob_cid_hex
                )));
            }
            let cid = BlobCid::from_hex(&meta.blob_cid_hex).ok_or_else(|| {
                BlobStorageError::CodecError(format!(
                    "invalid canonical blob CID: {}",
                    meta.blob_cid_hex
                ))
            })?;
            Ok(MigrationEntry { cid, meta })
        })
        .collect()
}

fn verify_blob_directory(
    directory: &Path,
    entry: &MigrationEntry<'_>,
) -> Result<(), BlobStorageError> {
    if entry.meta.chunk_size == 0
        || entry.meta.chunk_size as usize > BLOB_CHUNK_SIZE
        || entry.meta.total_size > BLOB_MAX_SIZE
    {
        return Err(BlobStorageError::CodecError(format!(
            "invalid size metadata for blob {}",
            entry.meta.blob_cid_hex
        )));
    }
    let expected_chunks = entry
        .meta
        .total_size
        .div_ceil(u64::from(entry.meta.chunk_size));
    let expected_chunks = u32::try_from(expected_chunks).map_err(|_| {
        BlobStorageError::CodecError(format!(
            "chunk count overflows u32 for blob {}",
            entry.meta.blob_cid_hex
        ))
    })?;
    if entry.cid.version() != BLOB_CID_VERSION
        || entry.cid.blob_type() as u8 != entry.cid.0[1]
        || entry.cid.0[1] != entry.meta.blob_type
        || entry.meta.blake3_hex.len() != 64
        || entry.meta.blake3_hex != encode_hex(entry.cid.blake3_hash())
        || expected_chunks != entry.meta.chunk_count
    {
        return Err(BlobStorageError::CodecError(format!(
            "inconsistent metadata for blob {}",
            entry.meta.blob_cid_hex
        )));
    }
    if !directory.is_dir() {
        return Err(BlobStorageError::NotFound);
    }

    let mut actual_indices = BTreeSet::new();
    for item in fs::read_dir(directory)? {
        let item = item?;
        if !item.file_type()?.is_file() {
            return Err(BlobStorageError::IoError(format!(
                "non-file entry in blob directory {}",
                directory.display()
            )));
        }
        let name = item.file_name();
        let name = name.to_str().ok_or_else(|| {
            BlobStorageError::IoError(format!("non-UTF-8 chunk name in {}", directory.display()))
        })?;
        let index = name
            .strip_prefix("chunk_")
            .and_then(|value| value.strip_suffix(".bin"))
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|index| format!("chunk_{index:04}.bin") == name)
            .ok_or_else(|| BlobStorageError::IoError(format!("unexpected chunk name {name}")))?;
        if index >= entry.meta.chunk_count || !actual_indices.insert(index) {
            return Err(BlobStorageError::IoError(format!(
                "unexpected chunk index for {}",
                entry.cid
            )));
        }
    }
    if actual_indices.len() != entry.meta.chunk_count as usize {
        return Err(BlobStorageError::IoError(format!(
            "unexpected chunk set for {}",
            entry.cid
        )));
    }

    let mut hasher = blake3::Hasher::new();
    let mut total_size = 0u64;
    for index in 0..entry.meta.chunk_count {
        let chunk_path = directory.join(format!("chunk_{index:04}.bin"));
        let expected_size = expected_chunk_size(entry.meta, index)?;
        if fs::metadata(&chunk_path)?.len() != expected_size {
            return Err(BlobStorageError::IoError(format!(
                "invalid chunk length for {} chunk {}",
                entry.cid, index
            )));
        }
        let bytes = fs::read(chunk_path)?;
        total_size = total_size
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| BlobStorageError::IoError("blob length overflow".into()))?;
        hasher.update(&bytes);
    }
    if total_size != entry.meta.total_size
        || hasher.finalize().as_bytes() != entry.cid.blake3_hash()
    {
        return Err(BlobStorageError::IoError(format!(
            "full CID verification failed for {}",
            entry.cid
        )));
    }
    Ok(())
}

fn expected_chunk_size(meta: &BlobMeta, index: u32) -> Result<u64, BlobStorageError> {
    let offset = u64::from(index)
        .checked_mul(u64::from(meta.chunk_size))
        .ok_or_else(|| BlobStorageError::IoError("chunk offset overflow".into()))?;
    let remaining = meta
        .total_size
        .checked_sub(offset)
        .ok_or_else(|| BlobStorageError::IoError("chunk offset exceeds blob length".into()))?;
    Ok(remaining.min(u64::from(meta.chunk_size)))
}

fn publish_staging(staging: &Path, final_dir: &Path) -> Result<(), BlobStorageError> {
    if final_dir.exists() {
        return Err(BlobStorageError::IoError(format!(
            "refusing to overwrite v2 blob directory {}",
            final_dir.display()
        )));
    }
    fs::rename(staging, final_dir)?;
    Ok(())
}

fn migrating_dir(final_dir: &Path) -> PathBuf {
    final_dir.with_extension("migrating")
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
