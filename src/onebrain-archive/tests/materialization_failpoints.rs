use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use onebrain_archive::{
    capture_dataset, compute_high_water_root, materialize_verified_dataset, seal_archive,
    verify_dataset_archive_v2, ArchiveCredential, ArchiveEntryId, ArchiveEntryKind, ArchiveEntryV1,
    ArchiveError, ArchiveLimits, ArchiveLogicalKey, ArchiveOwner, ArchiveRestorePolicyV1,
    DatasetManifestV1, FileSecureSpoolFactory, PortableDataCompatibilityV1, PortableProfileVersion,
    ProducerArtifactIdentityV1, SignerRecoveryDisposition, SnapshotLease, SnapshotSource,
    VerifiedDatasetMaterializer,
};
use tempfile::tempdir;

fn compatibility(seed: u8) -> PortableDataCompatibilityV1 {
    PortableDataCompatibilityV1 {
        canonical_schema_digest: [seed; 32],
        domain_registry_digest: [seed + 1; 32],
        resource_registry_digest: [seed + 2; 32],
        storage_schema_version: 1,
        archive_profile: PortableProfileVersion { major: 2, minor: 0 },
        migration_profile: PortableProfileVersion { major: 1, minor: 0 },
    }
}

fn policy(seed: u8) -> ArchiveRestorePolicyV1 {
    let value = compatibility(seed);
    ArchiveRestorePolicyV1 {
        canonical_schema_digest: value.canonical_schema_digest,
        domain_registry_digest: value.domain_registry_digest,
        resource_registry_digest: value.resource_registry_digest,
        storage_schema_version: value.storage_schema_version,
        archive_profile: value.archive_profile,
        migration_profile: value.migration_profile,
        max_dataset_bytes: 1024 * 1024,
    }
}

struct Source {
    entries: Vec<ArchiveEntryV1>,
    payloads: BTreeMap<ArchiveEntryId, Vec<u8>>,
    canonical_root: [u8; 32],
    high_water_root: [u8; 32],
}

impl Source {
    fn new() -> Self {
        let rows = [
            (
                ArchiveEntryKind::AuthorityHighWater,
                ArchiveOwner::CANONICAL,
                "authority",
                b"7".as_slice(),
            ),
            (
                ArchiveEntryKind::MigrationState,
                ArchiveOwner::MIGRATION,
                "migration",
                b"complete".as_slice(),
            ),
            (
                ArchiveEntryKind::InterpretationConfig,
                ArchiveOwner::INTERPRETATION_CONFIG,
                "config",
                b"v1".as_slice(),
            ),
            (
                ArchiveEntryKind::RegistryHighWater,
                ArchiveOwner::REGISTRY_METADATA,
                "registry",
                b"42".as_slice(),
            ),
            (
                ArchiveEntryKind::SignerRecoveryPolicy,
                ArchiveOwner::IDENTITY,
                "signer",
                b"reprovision-required".as_slice(),
            ),
            (
                ArchiveEntryKind::CanonicalObject,
                ArchiveOwner::CANONICAL,
                "object",
                b"canonical-object".as_slice(),
            ),
        ];
        let mut entries = Vec::new();
        let mut payloads = BTreeMap::new();
        for (kind, owner, key, payload) in rows {
            let logical = ArchiveLogicalKey::new(owner, 1, key.as_bytes().to_vec()).unwrap();
            let entry = ArchiveEntryV1::new(
                kind,
                logical,
                payload.len() as u64,
                *blake3::hash(payload).as_bytes(),
                true,
            )
            .unwrap();
            payloads.insert(entry.id, payload.to_vec());
            entries.push(entry);
        }
        let manifest = DatasetManifestV1::build(
            compatibility(1),
            ProducerArtifactIdentityV1::Unknown,
            entries.clone(),
        )
        .unwrap();
        Self {
            entries,
            payloads,
            canonical_root: manifest.canonical_root,
            high_water_root: compute_high_water_root(&manifest.entries),
        }
    }
}

impl SnapshotSource for Source {
    fn acquire_snapshot(&self) -> Result<SnapshotLease, ArchiveError> {
        Ok(SnapshotLease {
            dataset_generation: 1,
            canonical_source_root: self.canonical_root,
            high_water_root: self.high_water_root,
            blob_generation: 1,
            retention_generation: 1,
            source_binding: [5; 32],
        })
    }

    fn entries(&self, _: &SnapshotLease) -> Result<Vec<ArchiveEntryV1>, ArchiveError> {
        Ok(self.entries.clone())
    }

    fn read_entry(
        &self,
        _: &SnapshotLease,
        id: ArchiveEntryId,
    ) -> Result<Box<dyn Read>, ArchiveError> {
        Ok(Box::new(Cursor::new(self.payloads[&id].clone())))
    }

    fn validate_snapshot(&self, _: &SnapshotLease) -> Result<(), ArchiveError> {
        Ok(())
    }
}

fn archive() -> (Vec<u8>, ArchiveCredential) {
    let dataset = capture_dataset(
        &Source::new(),
        compatibility(1),
        ProducerArtifactIdentityV1::Unknown,
    )
    .unwrap()
    .canonical_plaintext()
    .unwrap();
    let credential = ArchiveCredential::password(b"task-11-password".to_vec()).unwrap();
    let mut archive = Vec::new();
    seal_archive(
        Cursor::new(dataset),
        &mut archive,
        &credential,
        &ArchiveLimits::default(),
    )
    .unwrap();
    (archive, credential)
}

#[derive(Default)]
struct RecordingMaterializer {
    rows: Vec<(ArchiveEntryId, Vec<u8>)>,
    began: bool,
    flushed: bool,
    cleaned: bool,
    fail_entry: Option<usize>,
    fail_flush: bool,
}

impl VerifiedDatasetMaterializer for RecordingMaterializer {
    fn begin(&mut self, _: &DatasetManifestV1) -> Result<(), ArchiveError> {
        self.began = true;
        Ok(())
    }

    fn materialize_entry(
        &mut self,
        entry: &ArchiveEntryV1,
        payload: &[u8],
    ) -> Result<(), ArchiveError> {
        if self.fail_entry == Some(self.rows.len()) {
            return Err(ArchiveError::RestoreSink("entry failpoint".into()));
        }
        self.rows.push((entry.id, payload.to_vec()));
        Ok(())
    }

    fn flush(&mut self) -> Result<(), ArchiveError> {
        if self.fail_flush {
            return Err(ArchiveError::RestoreSink("flush failpoint".into()));
        }
        self.flushed = true;
        Ok(())
    }

    fn cleanup_failed(&mut self) -> Result<(), ArchiveError> {
        self.rows.clear();
        self.cleaned = true;
        Ok(())
    }
}

#[test]
fn verified_token_materializes_sorted_entries_once_and_reports_reprovision() {
    let directory = tempdir().unwrap();
    let factory = FileSecureSpoolFactory::new(directory.path()).unwrap();
    let (archive, credential) = archive();
    let verified = verify_dataset_archive_v2(
        Cursor::new(archive),
        &factory,
        &credential,
        &ArchiveLimits::default(),
    )
    .unwrap();
    let mut sink = RecordingMaterializer::default();
    let receipt = materialize_verified_dataset(verified, &policy(1), &mut sink).unwrap();
    assert!(sink.began && sink.flushed && !sink.cleaned);
    assert_eq!(sink.rows.len(), receipt.entry_count as usize);
    assert_eq!(
        receipt.signer_recovery,
        SignerRecoveryDisposition::ReprovisionRequired
    );
    assert!(sink.rows.windows(2).all(|rows| rows[0].0 < rows[1].0));
}

#[test]
fn policy_mismatch_fails_before_target_side_effects() {
    let directory = tempdir().unwrap();
    let factory = FileSecureSpoolFactory::new(directory.path()).unwrap();
    let (archive, credential) = archive();
    let verified = verify_dataset_archive_v2(
        Cursor::new(archive),
        &factory,
        &credential,
        &ArchiveLimits::default(),
    )
    .unwrap();
    let mut sink = RecordingMaterializer::default();
    assert!(materialize_verified_dataset(verified, &policy(9), &mut sink).is_err());
    assert!(!sink.began && sink.rows.is_empty());
}

#[test]
fn entry_and_flush_failpoints_cleanup_without_completed_generation() {
    for mut sink in [
        RecordingMaterializer {
            fail_entry: Some(2),
            ..Default::default()
        },
        RecordingMaterializer {
            fail_flush: true,
            ..Default::default()
        },
    ] {
        let directory = tempdir().unwrap();
        let factory = FileSecureSpoolFactory::new(directory.path()).unwrap();
        let (archive, credential) = archive();
        let verified = verify_dataset_archive_v2(
            Cursor::new(archive),
            &factory,
            &credential,
            &ArchiveLimits::default(),
        )
        .unwrap();
        assert!(materialize_verified_dataset(verified, &policy(1), &mut sink).is_err());
        assert!(sink.cleaned);
        assert!(sink.rows.is_empty());
    }
}

#[test]
fn wrong_credential_and_corrupt_container_never_begin_materialization() {
    let directory = tempdir().unwrap();
    let factory = FileSecureSpoolFactory::new(directory.path()).unwrap();
    let (archive, _) = archive();
    let wrong = ArchiveCredential::password(b"wrong".to_vec()).unwrap();
    assert!(verify_dataset_archive_v2(
        Cursor::new(archive.clone()),
        &factory,
        &wrong,
        &ArchiveLimits::default()
    )
    .is_err());
    let mut corrupt = archive;
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    assert!(verify_dataset_archive_v2(
        Cursor::new(corrupt),
        &factory,
        &wrong,
        &ArchiveLimits::default()
    )
    .is_err());
}
