use std::fs;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};

use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use onebrain_archive::{
    inspect_legacy_archive_v1, seal_archive, verify_dataset_archive_v2, ArchiveCredential,
    ArchiveError, ArchiveInspection, ArchiveLimits, EncryptedSpoolCapability,
    FileSecureSpoolFactory, LogicalRestoreSink, RecoveryKey, SecureSpoolFactory,
    ARCHIVE_CHUNK_BYTES,
};
use serde::Serialize;
use tempfile::tempdir;

#[test]
fn recovery_key_archive_round_trips_only_through_verified_token() {
    let directory = tempdir().unwrap();
    let factory = FileSecureSpoolFactory::new(directory.path()).unwrap();
    let credential = recovery_credential(7);
    let plaintext = b"private base dataset".repeat(70_000);
    let (archive, sealed) = sealed(&plaintext, &credential, ArchiveLimits::default());
    assert!(archive.starts_with(b"OBARV002"));
    assert!(!archive
        .windows(b"private base dataset".len())
        .any(|window| window == b"private base dataset"));

    let verified = verify_dataset_archive_v2(
        Cursor::new(archive),
        &factory,
        &credential,
        &ArchiveLimits::default(),
    )
    .unwrap();
    assert_eq!(verified.inspection(), sealed);
    let mut sink = CollectSink::default();
    let materialized = verified.materialize_into(&mut sink).unwrap();
    assert_eq!(materialized.inspection, sealed);
    assert_eq!(sink.bytes, plaintext);
    assert!(fs::read_dir(directory.path()).unwrap().next().is_none());
}

#[test]
fn password_wrong_password_corruption_and_trailing_bytes_fail_closed() {
    let directory = tempdir().unwrap();
    let factory = FileSecureSpoolFactory::new(directory.path()).unwrap();
    let password = ArchiveCredential::password(b"correct horse battery staple".to_vec()).unwrap();
    let wrong = ArchiveCredential::password(b"wrong password".to_vec()).unwrap();
    let (archive, _) = sealed(b"password protected", &password, ArchiveLimits::default());

    assert!(matches!(
        verify_dataset_archive_v2(
            Cursor::new(archive.clone()),
            &factory,
            &wrong,
            &ArchiveLimits::default()
        ),
        Err(ArchiveError::Authentication)
    ));

    let mut corrupt = archive.clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    assert!(verify_dataset_archive_v2(
        Cursor::new(corrupt),
        &factory,
        &password,
        &ArchiveLimits::default()
    )
    .is_err());

    let mut trailing = archive;
    trailing.push(0);
    assert!(matches!(
        verify_dataset_archive_v2(
            Cursor::new(trailing),
            &factory,
            &password,
            &ArchiveLimits::default()
        ),
        Err(ArchiveError::TrailingBytes)
    ));
}

#[test]
fn kdf_downgrade_and_huge_parameters_are_rejected_before_argon2() {
    let directory = tempdir().unwrap();
    let factory = FileSecureSpoolFactory::new(directory.path()).unwrap();
    let password = ArchiveCredential::password(b"password".to_vec()).unwrap();
    let (archive, _) = sealed(b"payload", &password, ArchiveLimits::default());

    for replacement in [1u32, u32::MAX] {
        let mut changed = archive.clone();
        changed[12..16].copy_from_slice(&replacement.to_be_bytes());
        assert!(matches!(
            verify_dataset_archive_v2(
                Cursor::new(changed),
                &factory,
                &password,
                &ArchiveLimits::default()
            ),
            Err(ArchiveError::InvalidProfile)
        ));
    }
    let mut downgraded = archive;
    downgraded[16..20].copy_from_slice(&1u32.to_be_bytes());
    assert!(matches!(
        verify_dataset_archive_v2(
            Cursor::new(downgraded),
            &factory,
            &password,
            &ArchiveLimits::default()
        ),
        Err(ArchiveError::InvalidProfile)
    ));
}

#[test]
fn entry_and_spool_limits_fail_before_unbounded_growth() {
    let credential = recovery_credential(9);
    let entry_limited = ArchiveLimits {
        max_entries: 1,
        max_manifest_bytes: 1024 * 1024,
        max_entry_bytes: 2 * ARCHIVE_CHUNK_BYTES as u64,
        max_total_plaintext_bytes: 2 * ARCHIVE_CHUNK_BYTES as u64,
        max_spool_bytes: 3 * ARCHIVE_CHUNK_BYTES as u64,
    };
    assert!(matches!(
        seal_archive(
            Cursor::new(vec![0u8; ARCHIVE_CHUNK_BYTES + 1]),
            Vec::new(),
            &credential,
            &entry_limited
        ),
        Err(ArchiveError::Limit)
    ));

    let (archive, _) = sealed(b"small", &credential, ArchiveLimits::default());
    let directory = tempdir().unwrap();
    let factory = FileSecureSpoolFactory::new(directory.path()).unwrap();
    let spool_limited = ArchiveLimits {
        max_spool_bytes: (archive.len() - 1) as u64,
        ..ArchiveLimits::default()
    };
    assert!(
        verify_dataset_archive_v2(Cursor::new(archive), &factory, &credential, &spool_limited)
            .is_err()
    );
    assert!(fs::read_dir(directory.path()).unwrap().next().is_none());
}

#[test]
fn legacy_mobile_vector_is_authenticated_and_inspection_only() {
    let key_bytes = [11u8; 32];
    let key = RecoveryKey::from_bytes(key_bytes).unwrap();
    let payload = b"legacy mobile private data".repeat(50_000);
    let archive = legacy_vector(&payload, key_bytes);
    let inspected =
        inspect_legacy_archive_v1(Cursor::new(&archive), &key, &ArchiveLimits::default()).unwrap();
    assert_eq!(inspected.archive_version, 1);
    assert_eq!(inspected.archive_kind, "private_node_data");
    assert_eq!(inspected.source_schema_version, 7);
    assert_eq!(inspected.payload_length, payload.len() as u64);
    assert_eq!(
        inspected.declared_payload_blake3,
        *blake3::hash(&payload).as_bytes()
    );

    let wrong = RecoveryKey::from_bytes([12; 32]).unwrap();
    assert!(matches!(
        inspect_legacy_archive_v1(Cursor::new(&archive), &wrong, &ArchiveLimits::default()),
        Err(ArchiveError::Authentication)
    ));
    let mut huge_manifest = archive;
    huge_manifest[26..30].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(matches!(
        inspect_legacy_archive_v1(Cursor::new(huge_manifest), &key, &ArchiveLimits::default()),
        Err(ArchiveError::Limit)
    ));
}

#[test]
fn file_spool_is_create_new_bounded_and_crash_residue_is_cleaned() {
    let directory = tempdir().unwrap();
    let first_factory = FileSecureSpoolFactory::new(directory.path()).unwrap();
    let mut first = first_factory.create_new(16).unwrap();
    first.write_all(b"first").unwrap();
    let second = first_factory.create_new(16).unwrap();
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    first.seek(SeekFrom::Start(0)).unwrap();
    let mut preserved = String::new();
    first.read_to_string(&mut preserved).unwrap();
    assert_eq!(preserved, "first");
    first.securely_remove().unwrap();
    second.securely_remove().unwrap();

    let mut bounded = first_factory.create_new(4).unwrap();
    assert!(bounded.write_all(b"12345").is_err());
    bounded.securely_remove().unwrap();

    let mut residue = first_factory.create_new(4).unwrap();
    residue.write_all(b"OBAR").unwrap();
    residue.sync_all().unwrap();
    drop(residue); // model process death: the next process has a fresh registry.
    let next_process = FileSecureSpoolFactory::new(directory.path()).unwrap();
    assert_eq!(next_process.cleanup_crash_residue().unwrap(), 1);
    assert!(fs::read_dir(directory.path()).unwrap().next().is_none());

    let regular_file = directory.path().join("not-a-directory");
    fs::write(&regular_file, b"x").unwrap();
    assert!(matches!(
        FileSecureSpoolFactory::new(&regular_file),
        Err(ArchiveError::Io(_)) | Err(ArchiveError::UnsafeSpool)
    ));
}

#[test]
fn symlink_or_reparse_spool_root_is_rejected() {
    let directory = tempdir().unwrap();
    let actual = directory.path().join("actual");
    fs::create_dir(&actual).unwrap();
    let linked = directory.path().join("linked");

    #[cfg(unix)]
    std::os::unix::fs::symlink(&actual, &linked).unwrap();
    #[cfg(windows)]
    if let Err(error) = std::os::windows::fs::symlink_dir(&actual, &linked) {
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            return;
        }
        panic!("cannot create reparse-point vector: {error}");
    }

    assert!(matches!(
        FileSecureSpoolFactory::new(&linked),
        Err(ArchiveError::UnsafeSpool)
    ));
    let nested = linked.join("must-not-be-created");
    assert!(matches!(
        FileSecureSpoolFactory::new(&nested),
        Err(ArchiveError::UnsafeSpool)
    ));
    assert!(!actual.join("must-not-be-created").exists());
}

#[test]
fn permission_cleanup_and_handle_replacement_fail_closed_before_sink() {
    let credential = recovery_credential(5);
    let (archive, _) = sealed(b"bound handle", &credential, ArchiveLimits::default());
    assert!(matches!(
        verify_dataset_archive_v2(
            Cursor::new(archive.clone()),
            &PermissionDeniedFactory,
            &credential,
            &ArchiveLimits::default()
        ),
        Err(ArchiveError::Io(_))
    ));
    assert!(matches!(
        verify_dataset_archive_v2(
            Cursor::new(b"not an archive".to_vec()),
            &CleanupFailureFactory,
            &credential,
            &ArchiveLimits::default()
        ),
        Err(ArchiveError::CleanupFailed(_))
    ));

    let directory = tempdir().unwrap();
    let factory = FileSecureSpoolFactory::new(directory.path()).unwrap();
    let verified = verify_dataset_archive_v2(
        Cursor::new(archive),
        &factory,
        &credential,
        &ArchiveLimits::default(),
    )
    .unwrap();
    let spool_path = fs::read_dir(directory.path())
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let moved = directory.path().join("moved-bound-handle");
    fs::rename(&spool_path, &moved).unwrap();
    fs::write(&spool_path, b"replacement").unwrap();
    let mut sink = CollectSink::default();
    assert!(verified.materialize_into(&mut sink).is_err());
    assert!(sink.bytes.is_empty());
    assert_eq!(fs::read(&spool_path).unwrap(), b"replacement");
    fs::remove_file(spool_path).unwrap();
    fs::remove_file(moved).unwrap();
}

#[derive(Default)]
struct CollectSink {
    bytes: Vec<u8>,
}

impl LogicalRestoreSink for CollectSink {
    fn restore_verified(
        &mut self,
        plaintext: &[u8],
        _inspection: &ArchiveInspection,
    ) -> Result<(), ArchiveError> {
        self.bytes.extend_from_slice(plaintext);
        Ok(())
    }
}

struct PermissionDeniedFactory;

impl SecureSpoolFactory for PermissionDeniedFactory {
    fn create_new(
        &self,
        _max_bytes: u64,
    ) -> Result<Box<dyn EncryptedSpoolCapability>, ArchiveError> {
        Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied").into())
    }
}

struct CleanupFailureFactory;

impl SecureSpoolFactory for CleanupFailureFactory {
    fn create_new(
        &self,
        max_bytes: u64,
    ) -> Result<Box<dyn EncryptedSpoolCapability>, ArchiveError> {
        Ok(Box::new(MemorySpool {
            cursor: Cursor::new(Vec::new()),
            max_bytes,
            cleanup_fails: true,
        }))
    }
}

struct MemorySpool {
    cursor: Cursor<Vec<u8>>,
    max_bytes: u64,
    cleanup_fails: bool,
}

impl Read for MemorySpool {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.cursor.read(buffer)
    }
}

impl Write for MemorySpool {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if self.cursor.position().saturating_add(buffer.len() as u64) > self.max_bytes {
            return Err(std::io::Error::new(std::io::ErrorKind::StorageFull, "full"));
        }
        self.cursor.write(buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Seek for MemorySpool {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.cursor.seek(position)
    }
}

impl EncryptedSpoolCapability for MemorySpool {
    fn sync_all(&mut self) -> Result<(), ArchiveError> {
        Ok(())
    }

    fn securely_remove(self: Box<Self>) -> Result<(), ArchiveError> {
        if self.cleanup_fails {
            Err(ArchiveError::CleanupFailed("injected".into()))
        } else {
            Ok(())
        }
    }
}

fn recovery_credential(marker: u8) -> ArchiveCredential {
    ArchiveCredential::RecoveryKey(RecoveryKey::from_bytes([marker; 32]).unwrap())
}

fn sealed(
    plaintext: &[u8],
    credential: &ArchiveCredential,
    limits: ArchiveLimits,
) -> (Vec<u8>, ArchiveInspection) {
    let mut archive = Vec::new();
    let inspection =
        seal_archive(Cursor::new(plaintext), &mut archive, credential, &limits).unwrap();
    (archive, inspection)
}

#[derive(Serialize)]
struct LegacyManifest {
    archive_version: u16,
    archive_kind: String,
    source_schema_version: u32,
    payload_length: u64,
    payload_digest: String,
    chunk_bytes: u32,
    chunk_count: u32,
}

fn legacy_vector(payload: &[u8], recovery_key: [u8; 32]) -> Vec<u8> {
    const CHUNK: usize = 1024 * 1024;
    const CONTEXT: &[u8] = b"onebrain:mobile:portable-archive:1\0";
    let manifest = LegacyManifest {
        archive_version: 1,
        archive_kind: "private_node_data".into(),
        source_schema_version: 7,
        payload_length: payload.len() as u64,
        payload_digest: blake3::hash(payload).to_hex().to_string(),
        chunk_bytes: CHUNK as u32,
        chunk_count: payload.len().div_ceil(CHUNK) as u32,
    };
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    let key = blake3::derive_key("onebrain:mobile:portable-archive-key:1", &recovery_key);
    let cipher = XChaCha20Poly1305::new((&key).into());
    let prefix = [3u8; 16];
    let seal = |index: u64, bytes: &[u8]| {
        let mut nonce = [0u8; 24];
        nonce[..16].copy_from_slice(&prefix);
        nonce[16..].copy_from_slice(&index.to_be_bytes());
        let mut aad = Vec::from(CONTEXT);
        aad.extend_from_slice(&1u16.to_be_bytes());
        aad.extend_from_slice(&index.to_be_bytes());
        let nonce: XNonce = nonce.into();
        cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: bytes,
                    aad: &aad,
                },
            )
            .unwrap()
    };
    let encrypted_manifest = seal(0, &manifest_bytes);
    let mut archive = Vec::new();
    archive.extend_from_slice(b"OBARV001");
    archive.extend_from_slice(&1u16.to_be_bytes());
    archive.extend_from_slice(&prefix);
    archive.extend_from_slice(&(encrypted_manifest.len() as u32).to_be_bytes());
    archive.extend_from_slice(&encrypted_manifest);
    archive.extend_from_slice(&manifest.chunk_count.to_be_bytes());
    for (index, chunk) in payload.chunks(CHUNK).enumerate() {
        let encrypted = seal(index as u64 + 1, chunk);
        archive.extend_from_slice(&(encrypted.len() as u32).to_be_bytes());
        archive.extend_from_slice(&encrypted);
    }
    archive
}
