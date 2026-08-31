use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use zeroize::Zeroizing;

use crate::container::{open_v2, verify_v2, ArchiveInspection};
use crate::crypto::ArchiveCredential;
use crate::limits::ArchiveLimits;
use crate::ArchiveError;

const SPOOL_PREFIX: &str = ".onebrain-archive-spool-";
const COPY_BUFFER_BYTES: usize = 64 * 1024;

pub trait EncryptedSpoolCapability: Read + Write + Seek + Send {
    fn sync_all(&mut self) -> Result<(), ArchiveError>;
    fn securely_remove(self: Box<Self>) -> Result<(), ArchiveError>;
}

pub trait SecureSpoolFactory: Send + Sync {
    fn create_new(&self, max_bytes: u64)
        -> Result<Box<dyn EncryptedSpoolCapability>, ArchiveError>;
}

pub trait LogicalRestoreSink {
    fn restore_verified(
        &mut self,
        plaintext: &[u8],
        inspection: &ArchiveInspection,
    ) -> Result<(), ArchiveError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerifiedMaterialization {
    pub inspection: ArchiveInspection,
}

pub struct VerifiedDatasetArchiveV2 {
    spool: Option<Box<dyn EncryptedSpoolCapability>>,
    credential: ArchiveCredential,
    limits: ArchiveLimits,
    archive_length: u64,
    archive_blake3: [u8; 32],
    inspection: ArchiveInspection,
}

impl VerifiedDatasetArchiveV2 {
    pub const fn inspection(&self) -> ArchiveInspection {
        self.inspection
    }

    pub fn materialize_into(
        mut self,
        sink: &mut dyn LogicalRestoreSink,
    ) -> Result<VerifiedMaterialization, ArchiveError> {
        let mut spool = self.spool.take().ok_or(ArchiveError::HandleBinding)?;
        let outcome = (|| {
            let (length, digest) = raw_binding(&mut spool)?;
            if length != self.archive_length || digest != self.archive_blake3 {
                return Err(ArchiveError::HandleBinding);
            }
            let reverified = verify_v2(&mut spool, &self.credential, &self.limits)?;
            if reverified != self.inspection {
                return Err(ArchiveError::HandleBinding);
            }
            // `open_v2` performs another complete authenticated pass and only
            // returns plaintext after count, length, digest, and EOF succeed.
            let (opened, plaintext) = open_v2(&mut spool, &self.credential, &self.limits)?;
            if opened != self.inspection {
                return Err(ArchiveError::HandleBinding);
            }
            sink.restore_verified(&plaintext, &opened)?;
            Ok(VerifiedMaterialization { inspection: opened })
        })();
        let cleanup = spool.securely_remove();
        match (outcome, cleanup) {
            (_, Err(error)) => Err(ArchiveError::CleanupFailed(error.to_string())),
            (result, Ok(())) => result,
        }
    }
}

impl Drop for VerifiedDatasetArchiveV2 {
    fn drop(&mut self) {
        if let Some(spool) = self.spool.take() {
            let _ = spool.securely_remove();
        }
    }
}

pub fn verify_dataset_archive_v2<R: Read + Send + 'static>(
    mut input: R,
    spool_factory: &dyn SecureSpoolFactory,
    credential: &ArchiveCredential,
    limits: &ArchiveLimits,
) -> Result<VerifiedDatasetArchiveV2, ArchiveError> {
    limits.validate()?;
    let mut spool = spool_factory.create_new(limits.max_spool_bytes)?;
    let copy_result = (|| {
        let mut buffer = [0u8; COPY_BUFFER_BYTES];
        let mut length = 0u64;
        let mut hasher = blake3::Hasher::new();
        loop {
            let read = input.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            length = length.checked_add(read as u64).ok_or(ArchiveError::Limit)?;
            if length > limits.max_spool_bytes {
                return Err(ArchiveError::Limit);
            }
            spool.write_all(&buffer[..read])?;
            hasher.update(&buffer[..read]);
        }
        spool.sync_all()?;
        let inspection = verify_v2(&mut spool, credential, limits)?;
        Ok((length, *hasher.finalize().as_bytes(), inspection))
    })();
    match copy_result {
        Ok((archive_length, archive_blake3, inspection)) => Ok(VerifiedDatasetArchiveV2 {
            spool: Some(spool),
            credential: credential.duplicate(),
            limits: *limits,
            archive_length,
            archive_blake3,
            inspection,
        }),
        Err(primary) => match spool.securely_remove() {
            Ok(()) => Err(primary),
            Err(cleanup) => Err(ArchiveError::CleanupFailed(format!(
                "{primary}; cleanup: {cleanup}"
            ))),
        },
    }
}

fn raw_binding(
    spool: &mut Box<dyn EncryptedSpoolCapability>,
) -> Result<(u64, [u8; 32]), ArchiveError> {
    spool.seek(SeekFrom::Start(0))?;
    let mut buffer = [0u8; COPY_BUFFER_BYTES];
    let mut length = 0u64;
    let mut hasher = blake3::Hasher::new();
    loop {
        let read = spool.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        length = length.checked_add(read as u64).ok_or(ArchiveError::Limit)?;
        hasher.update(&buffer[..read]);
    }
    Ok((length, *hasher.finalize().as_bytes()))
}

#[derive(Clone)]
pub struct FileSecureSpoolFactory {
    directory: PathBuf,
    active: Arc<Mutex<BTreeSet<PathBuf>>>,
}

impl FileSecureSpoolFactory {
    pub fn new(directory: impl AsRef<Path>) -> Result<Self, ArchiveError> {
        let directory = directory.as_ref();
        ensure_safe_existing_ancestors(directory)?;
        fs::create_dir_all(directory)?;
        ensure_safe_path(directory, true)?;
        Ok(Self {
            directory: directory.to_path_buf(),
            active: Arc::new(Mutex::new(BTreeSet::new())),
        })
    }

    /// Remove registered crash residue while skipping handles active in this
    /// process. Call during single-owner startup, before accepting restores.
    pub fn cleanup_crash_residue(&self) -> Result<usize, ArchiveError> {
        ensure_safe_path(&self.directory, true)?;
        let active = self
            .active
            .lock()
            .map_err(|_| ArchiveError::UnsafeSpool)?
            .clone();
        let mut removed = 0usize;
        for entry in fs::read_dir(&self.directory)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name();
            if !name.to_string_lossy().starts_with(SPOOL_PREFIX) || active.contains(&path) {
                continue;
            }
            ensure_safe_path(&path, false)?;
            let file = OpenOptions::new().read(true).write(true).open(&path)?;
            let identity = FileIdentity::from_file(&file)?;
            let length = file.metadata()?.len();
            Box::new(FileEncryptedSpool {
                file: Some(file),
                path: path.clone(),
                identity,
                max_bytes: length,
                active: self.active.clone(),
            })
            .securely_remove()?;
            removed += 1;
        }
        Ok(removed)
    }

    fn candidate_path(&self) -> Result<PathBuf, ArchiveError> {
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(|_| ArchiveError::UnsafeSpool)?;
        let mut encoded = String::with_capacity(32);
        for byte in random {
            use std::fmt::Write as _;
            let _ = write!(encoded, "{byte:02x}");
        }
        Ok(self.directory.join(format!("{SPOOL_PREFIX}{encoded}")))
    }
}

impl SecureSpoolFactory for FileSecureSpoolFactory {
    fn create_new(
        &self,
        max_bytes: u64,
    ) -> Result<Box<dyn EncryptedSpoolCapability>, ArchiveError> {
        ensure_safe_path(&self.directory, true)?;
        if max_bytes == 0 {
            return Err(ArchiveError::Limit);
        }
        for _ in 0..16 {
            let path = self.candidate_path()?;
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt as _;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(file) => {
                    ensure_safe_path(&path, false)?;
                    let identity = FileIdentity::from_file(&file)?;
                    self.active
                        .lock()
                        .map_err(|_| ArchiveError::UnsafeSpool)?
                        .insert(path.clone());
                    return Ok(Box::new(FileEncryptedSpool {
                        file: Some(file),
                        path,
                        identity,
                        max_bytes,
                        active: self.active.clone(),
                    }));
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(ArchiveError::UnsafeSpool)
    }
}

struct FileEncryptedSpool {
    file: Option<File>,
    path: PathBuf,
    identity: FileIdentity,
    max_bytes: u64,
    active: Arc<Mutex<BTreeSet<PathBuf>>>,
}

impl Read for FileEncryptedSpool {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.ensure_bound_path()?;
        self.file_mut()?.read(buffer)
    }
}

impl Write for FileEncryptedSpool {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.ensure_bound_path()?;
        let max_bytes = self.max_bytes;
        let file = self.file_mut()?;
        let position = file.stream_position()?;
        if position.saturating_add(buffer.len() as u64) > max_bytes {
            return Err(std::io::Error::new(
                std::io::ErrorKind::StorageFull,
                "secure spool bound exceeded",
            ));
        }
        file.write(buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.ensure_bound_path()?;
        self.file_mut()?.flush()
    }
}

impl Seek for FileEncryptedSpool {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.ensure_bound_path()?;
        self.file_mut()?.seek(position)
    }
}

impl EncryptedSpoolCapability for FileEncryptedSpool {
    fn sync_all(&mut self) -> Result<(), ArchiveError> {
        self.file_mut()?.flush()?;
        self.file_ref()?.sync_all()?;
        Ok(())
    }

    fn securely_remove(mut self: Box<Self>) -> Result<(), ArchiveError> {
        let result = (|| {
            ensure_safe_path(&self.path, false)?;
            let path_file = OpenOptions::new().read(true).write(true).open(&self.path)?;
            if FileIdentity::from_file(&path_file)? != self.identity {
                return Err(ArchiveError::UnsafeSpool);
            }
            drop(path_file);
            let file = self.file.as_mut().ok_or(ArchiveError::UnsafeSpool)?;
            let length = file.metadata()?.len();
            file.seek(SeekFrom::Start(0))?;
            let zeros = Zeroizing::new(vec![0u8; COPY_BUFFER_BYTES]);
            let mut remaining = length;
            while remaining > 0 {
                let write = remaining.min(zeros.len() as u64) as usize;
                file.write_all(&zeros[..write])?;
                remaining -= write as u64;
            }
            file.set_len(0)?;
            file.sync_all()?;
            self.file.take();
            fs::remove_file(&self.path)?;
            Ok(())
        })();
        if let Ok(mut active) = self.active.lock() {
            active.remove(&self.path);
        }
        result
    }
}

impl FileEncryptedSpool {
    fn file_mut(&mut self) -> std::io::Result<&mut File> {
        self.file.as_mut().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::BrokenPipe, "secure spool is closed")
        })
    }

    fn file_ref(&self) -> std::io::Result<&File> {
        self.file.as_ref().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::BrokenPipe, "secure spool is closed")
        })
    }

    fn ensure_bound_path(&self) -> std::io::Result<()> {
        let metadata = fs::symlink_metadata(&self.path)?;
        let path_file = OpenOptions::new().read(true).open(&self.path)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || FileIdentity::from_file(&path_file)? != self.identity
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "secure spool path no longer names the bound handle",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileIdentity {
    first: u64,
    second: u64,
}

impl FileIdentity {
    #[cfg(unix)]
    fn from_file(file: &File) -> std::io::Result<Self> {
        use std::os::unix::fs::MetadataExt as _;
        let metadata = file.metadata()?;
        Ok(Self {
            first: metadata.dev(),
            second: metadata.ino(),
        })
    }

    #[cfg(windows)]
    fn from_file(file: &File) -> std::io::Result<Self> {
        use std::mem::MaybeUninit;
        use std::os::windows::io::AsRawHandle as _;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };

        let mut information = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::zeroed();
        // SAFETY: `file` owns a live Windows handle and `information` points to
        // writable storage of the exact structure required by the OS call.
        let succeeded = unsafe {
            GetFileInformationByHandle(file.as_raw_handle() as _, information.as_mut_ptr())
        };
        if succeeded == 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: a successful OS call initialized the whole structure.
        let information = unsafe { information.assume_init() };
        Ok(Self {
            first: information.dwVolumeSerialNumber as u64,
            second: ((information.nFileIndexHigh as u64) << 32) | information.nFileIndexLow as u64,
        })
    }

    #[cfg(not(any(unix, windows)))]
    fn from_file(file: &File) -> std::io::Result<Self> {
        let metadata = file.metadata()?;
        Ok(Self {
            first: metadata.len(),
            second: 0,
        })
    }
}

fn ensure_safe_path(path: &Path, directory: bool) -> Result<(), ArchiveError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || (directory && !metadata.is_dir())
        || (!directory && !metadata.is_file())
    {
        return Err(ArchiveError::UnsafeSpool);
    }
    for ancestor in path.ancestors().skip(1) {
        let ancestor_metadata = fs::symlink_metadata(ancestor)?;
        if ancestor_metadata.file_type().is_symlink() {
            return Err(ArchiveError::UnsafeSpool);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt as _;
            const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
            if ancestor_metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err(ArchiveError::UnsafeSpool);
            }
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(ArchiveError::UnsafeSpool);
        }
    }
    Ok(())
}

fn ensure_safe_existing_ancestors(path: &Path) -> Result<(), ArchiveError> {
    for ancestor in path.ancestors() {
        let metadata = match fs::symlink_metadata(ancestor) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() {
            return Err(ArchiveError::UnsafeSpool);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt as _;
            const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
            if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err(ArchiveError::UnsafeSpool);
            }
        }
    }
    Ok(())
}
