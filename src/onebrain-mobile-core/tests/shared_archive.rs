//! Shared portable archive container (ARCH-10, D-051/D-053; MOB-03,
//! MOB-DAT-006..008).
//!
//! The mobile core does not keep its own archive format. New archives use the
//! shared `onebrain-archive` `OBARV002` container that the Base runtime uses.
//! Archives written by the retired mobile-only `OBARV001` code stay readable
//! for authenticated inspection only; they never produce verified restore
//! state.

use std::io::Cursor;

use onebrain_archive::{
    inspect_legacy_archive_v1, seal_archive, verify_dataset_archive_v2, ArchiveCredential,
    ArchiveCredentialKind, ArchiveError, ArchiveInspection, ArchiveLimits, FileSecureSpoolFactory,
    LogicalRestoreSink, RecoveryKey, OBAR_V1_MAGIC, OBAR_V2_MAGIC,
};
use onebrain_mobile_core::archive as mobile_archive;
use tempfile::tempdir;

const LEGACY_FIXTURE: &[u8] = include_bytes!("../fixtures/legacy_mobile_obarv001_v1.bin");

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

fn recovery(marker: u8) -> ArchiveCredential {
    ArchiveCredential::RecoveryKey(RecoveryKey::from_bytes([marker; 32]).unwrap())
}

#[test]
fn mobile_core_reexports_the_shared_archive_crate() {
    // The mobile path is the shared crate itself, not a copy.
    assert_eq!(mobile_archive::OBAR_V2_MAGIC, OBAR_V2_MAGIC);
    assert_eq!(mobile_archive::OBAR_V1_MAGIC, OBAR_V1_MAGIC);
    let _: fn(_, _, _, _) -> _ = mobile_archive::seal_archive::<Cursor<Vec<u8>>, Vec<u8>>;
}

#[test]
fn new_mobile_archives_are_shared_v2_and_restore_only_through_verification() {
    let spool = tempdir().unwrap();
    let factory = FileSecureSpoolFactory::new(spool.path()).unwrap();
    let credential = recovery(7);
    let plaintext = b"private mobile node data".repeat(4_096);

    let mut archive = Vec::new();
    let sealed = seal_archive(
        Cursor::new(plaintext.clone()),
        &mut archive,
        &credential,
        &ArchiveLimits::default(),
    )
    .unwrap();
    assert!(archive.starts_with(OBAR_V2_MAGIC));
    assert_eq!(sealed.credential_kind, ArchiveCredentialKind::RecoveryKey);
    assert!(!archive
        .windows(b"private mobile node data".len())
        .any(|window| window == b"private mobile node data"));

    let wrong = verify_dataset_archive_v2(
        Cursor::new(archive.clone()),
        &factory,
        &recovery(8),
        &ArchiveLimits::default(),
    );
    assert!(matches!(wrong, Err(ArchiveError::Authentication)));

    let mut corrupt = archive.clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 0x01;
    assert!(verify_dataset_archive_v2(
        Cursor::new(corrupt),
        &factory,
        &credential,
        &ArchiveLimits::default(),
    )
    .is_err());

    let verified = verify_dataset_archive_v2(
        Cursor::new(archive),
        &factory,
        &credential,
        &ArchiveLimits::default(),
    )
    .unwrap();
    let mut sink = CollectSink::default();
    let materialized = verified.materialize_into(&mut sink).unwrap();
    assert_eq!(materialized.inspection, sealed);
    assert_eq!(sink.bytes, plaintext);
}

#[test]
fn archives_from_the_retired_mobile_v1_code_stay_inspectable_only() {
    // Fixture written by the retired mobile `archive.rs` (`OBARV001`) before it
    // was removed; key bytes 0x5a, kind private_node_data, schema 3.
    assert!(LEGACY_FIXTURE.starts_with(OBAR_V1_MAGIC));
    let key = RecoveryKey::from_bytes([0x5a; 32]).unwrap();
    let inspection =
        inspect_legacy_archive_v1(Cursor::new(LEGACY_FIXTURE), &key, &ArchiveLimits::default())
            .unwrap();
    assert_eq!(inspection.archive_version, 1);
    assert_eq!(inspection.archive_kind, "private_node_data");
    assert_eq!(inspection.source_schema_version, 3);
    assert_eq!(
        inspection.payload_length,
        b"legacy mobile OBARV001 fixture payload".len() as u64
    );

    let wrong = RecoveryKey::from_bytes([0x5b; 32]).unwrap();
    assert!(inspect_legacy_archive_v1(
        Cursor::new(LEGACY_FIXTURE),
        &wrong,
        &ArchiveLimits::default()
    )
    .is_err());

    // A legacy archive is never accepted as a verified v2 restore source.
    let spool = tempdir().unwrap();
    let factory = FileSecureSpoolFactory::new(spool.path()).unwrap();
    assert!(verify_dataset_archive_v2(
        Cursor::new(LEGACY_FIXTURE),
        &factory,
        &ArchiveCredential::RecoveryKey(RecoveryKey::from_bytes([0x5a; 32]).unwrap()),
        &ArchiveLimits::default(),
    )
    .is_err());
}
