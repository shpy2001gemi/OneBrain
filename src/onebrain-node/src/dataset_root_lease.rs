use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use fs2::FileExt;
use thiserror::Error;

const LOCK_FILE: &str = "dataset-root.lock";

#[derive(Debug, Error)]
pub enum DatasetRootLeaseError {
    #[error("dataset root is already in use")]
    DatasetRootInUse,
    #[error("dataset root path is unsafe")]
    UnsafeRoot,
    #[error("dataset root I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

/// Lifetime-held cross-process authority for the non-switched control plane.
/// The inert lock file is never interpreted as a live/stale owner.
pub struct DatasetRootLease {
    canonical_root: PathBuf,
    file: File,
}

impl DatasetRootLease {
    pub fn acquire(root: &Path) -> Result<Self, DatasetRootLeaseError> {
        ensure_safe_existing_ancestors(root)?;
        std::fs::create_dir_all(root)?;
        ensure_safe_path(root, true)?;
        let canonical_root = root.canonicalize()?;
        let control = canonical_root.join("control");
        std::fs::create_dir_all(&control)?;
        ensure_safe_path(&control, true)?;
        let lock_path = control.join(LOCK_FILE);
        let file = match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                ensure_safe_path(&lock_path, false)?;
                OpenOptions::new().read(true).write(true).open(&lock_path)?
            }
            Err(error) => return Err(error.into()),
        };
        ensure_safe_path(&lock_path, false)?;
        file.try_lock_exclusive().map_err(|error| {
            if error.kind() == std::io::ErrorKind::WouldBlock
                || cfg!(windows) && error.raw_os_error() == Some(33)
            {
                DatasetRootLeaseError::DatasetRootInUse
            } else {
                DatasetRootLeaseError::Io(error)
            }
        })?;
        Ok(Self {
            canonical_root,
            file,
        })
    }

    pub fn root(&self) -> &Path {
        &self.canonical_root
    }
}

impl Drop for DatasetRootLease {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

fn ensure_safe_existing_ancestors(path: &Path) -> Result<(), DatasetRootLeaseError> {
    for ancestor in path.ancestors() {
        match std::fs::symlink_metadata(ancestor) {
            Ok(metadata) => reject_link_or_reparse(&metadata)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn ensure_safe_path(path: &Path, directory: bool) -> Result<(), DatasetRootLeaseError> {
    let metadata = std::fs::symlink_metadata(path)?;
    reject_link_or_reparse(&metadata)?;
    if (directory && !metadata.is_dir()) || (!directory && !metadata.is_file()) {
        return Err(DatasetRootLeaseError::UnsafeRoot);
    }
    Ok(())
}

fn reject_link_or_reparse(metadata: &std::fs::Metadata) -> Result<(), DatasetRootLeaseError> {
    if metadata.file_type().is_symlink() {
        return Err(DatasetRootLeaseError::UnsafeRoot);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(DatasetRootLeaseError::UnsafeRoot);
        }
    }
    Ok(())
}
