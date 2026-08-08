use std::cell::Cell;
use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use onebrain_archive::{
    capture_dataset, compute_entry_id, compute_high_water_root, ArchiveEntryId, ArchiveEntryKind,
    ArchiveEntryV1, ArchiveError, ArchiveLogicalKey, ArchiveOwner, DatasetManifestV1,
    PortableDataCompatibilityV1, PortableProfileVersion, ProducerArtifactIdentityV1, SnapshotLease,
    SnapshotSource,
};

fn compatibility(seed: u8) -> PortableDataCompatibilityV1 {
    PortableDataCompatibilityV1 {
        canonical_schema_digest: [seed; 32],
        domain_registry_digest: [seed.wrapping_add(1); 32],
        resource_registry_digest: [seed.wrapping_add(2); 32],
        storage_schema_version: 1,
        archive_profile: PortableProfileVersion { major: 2, minor: 0 },
        migration_profile: PortableProfileVersion { major: 1, minor: 0 },
    }
}

fn row(kind: ArchiveEntryKind, owner: ArchiveOwner, key: &str, payload: &[u8]) -> ArchiveEntryV1 {
    ArchiveEntryV1::new(
        kind,
        ArchiveLogicalKey::new(owner, 1, key.as_bytes().to_vec()).unwrap(),
        payload.len() as u64,
        *blake3::hash(payload).as_bytes(),
        true,
    )
    .unwrap()
}

fn fixture() -> (Vec<ArchiveEntryV1>, BTreeMap<ArchiveEntryId, Vec<u8>>) {
    let rows = [
        (
            ArchiveEntryKind::AuthorityHighWater,
            ArchiveOwner::CANONICAL,
            "authority-high-water",
            b"authority:7".as_slice(),
        ),
        (
            ArchiveEntryKind::MigrationState,
            ArchiveOwner::MIGRATION,
            "migration-state",
            b"migration:complete".as_slice(),
        ),
        (
            ArchiveEntryKind::InterpretationConfig,
            ArchiveOwner::INTERPRETATION_CONFIG,
            "interpretation-config",
            b"config:v1".as_slice(),
        ),
        (
            ArchiveEntryKind::RegistryHighWater,
            ArchiveOwner::REGISTRY_METADATA,
            "registry-high-water",
            b"registry:42".as_slice(),
        ),
        (
            ArchiveEntryKind::SignerRecoveryPolicy,
            ArchiveOwner::IDENTITY,
            "signer-recovery-policy",
            b"reprovision-required".as_slice(),
        ),
        (
            ArchiveEntryKind::CanonicalObject,
            ArchiveOwner::CANONICAL,
            "object-01",
            b"canonical-object".as_slice(),
        ),
        (
            ArchiveEntryKind::OwnedBlob,
            ArchiveOwner::BLOB,
            "blob-01",
            b"owned-blob".as_slice(),
        ),
        (
            ArchiveEntryKind::AuthorityEvent,
            ArchiveOwner::CANONICAL,
            "authority-event-01",
            b"authority-event".as_slice(),
        ),
    ];
    let mut entries = Vec::new();
    let mut payloads = BTreeMap::new();
    for (kind, owner, key, payload) in rows {
        let entry = row(kind, owner, key, payload);
        payloads.insert(entry.id, payload.to_vec());
        entries.push(entry);
    }
    entries.reverse();
    (entries, payloads)
}

struct FakeSource {
    entries: Vec<ArchiveEntryV1>,
    payloads: BTreeMap<ArchiveEntryId, Vec<u8>>,
    canonical_root: [u8; 32],
    high_water_root: [u8; 32],
    binding: Cell<[u8; 32]>,
    mutate_during_read: bool,
}

impl FakeSource {
    fn valid() -> Self {
        let (entries, payloads) = fixture();
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
            binding: Cell::new([9; 32]),
            mutate_during_read: false,
        }
    }
}

impl SnapshotSource for FakeSource {
    fn acquire_snapshot(&self) -> Result<SnapshotLease, ArchiveError> {
        Ok(SnapshotLease {
            dataset_generation: 4,
            canonical_source_root: self.canonical_root,
            high_water_root: self.high_water_root,
            blob_generation: 8,
            retention_generation: 9,
            source_binding: self.binding.get(),
        })
    }

    fn entries(&self, _lease: &SnapshotLease) -> Result<Vec<ArchiveEntryV1>, ArchiveError> {
        Ok(self.entries.clone())
    }

    fn read_entry(
        &self,
        _lease: &SnapshotLease,
        id: ArchiveEntryId,
    ) -> Result<Box<dyn Read>, ArchiveError> {
        if self.mutate_during_read {
            self.binding.set([10; 32]);
        }
        Ok(Box::new(Cursor::new(
            self.payloads
                .get(&id)
                .cloned()
                .ok_or(ArchiveError::Integrity)?,
        )))
    }

    fn validate_snapshot(&self, lease: &SnapshotLease) -> Result<(), ArchiveError> {
        if lease.source_binding != self.binding.get() {
            return Err(ArchiveError::Integrity);
        }
        Ok(())
    }
}

#[test]
fn stable_ids_sorted_manifest_and_plaintext_are_deterministic() {
    let source = FakeSource::valid();
    let captured = capture_dataset(
        &source,
        compatibility(1),
        ProducerArtifactIdentityV1::Unknown,
    )
    .unwrap();
    assert!(captured
        .manifest
        .entries
        .windows(2)
        .all(|pair| pair[0].id < pair[1].id));
    let key = ArchiveLogicalKey::new(ArchiveOwner::CANONICAL, 1, b"object-01".to_vec()).unwrap();
    assert_eq!(
        compute_entry_id(ArchiveEntryKind::CanonicalObject, &key).as_bytes(),
        &[
            0xb8, 0x3b, 0xe4, 0x5e, 0xda, 0x7c, 0xe7, 0xbc, 0xdb, 0xc3, 0xe6, 0xf9, 0xf0, 0xee,
            0xcc, 0xfe, 0x4f, 0xeb, 0xdd, 0x7f, 0x47, 0x1b, 0x42, 0x40, 0xc9, 0x20, 0x46, 0xb7,
            0x3b, 0xf7, 0x21, 0x0d,
        ]
    );
    assert_eq!(
        captured.manifest.aggregate_root,
        [
            0x84, 0x7f, 0xef, 0xaf, 0xdc, 0x85, 0x3d, 0x6e, 0xf6, 0xc0, 0x2e, 0x67, 0xea, 0xdd,
            0x86, 0xf2, 0xd3, 0xfb, 0xdc, 0x73, 0xe3, 0x7a, 0x96, 0x01, 0x69, 0x2f, 0x46, 0xe2,
            0x4d, 0xc6, 0x39, 0xf5,
        ]
    );
    let first = captured.canonical_plaintext().unwrap();
    let second = capture_dataset(
        &source,
        compatibility(1),
        ProducerArtifactIdentityV1::Unknown,
    )
    .unwrap()
    .canonical_plaintext()
    .unwrap();
    assert_eq!(first, second);
    assert!(first.starts_with(b"OBDSV001"));
}

#[test]
fn bounds_paths_duplicates_and_missing_required_metadata_fail_closed() {
    assert!(ArchiveOwner::new(0).is_err());
    assert!(ArchiveOwner::new(0x17).is_err());
    assert!(ArchiveLogicalKey::new(ArchiveOwner::CANONICAL, 0, b"key".to_vec()).is_err());
    assert!(ArchiveLogicalKey::new(ArchiveOwner::CANONICAL, 1, b"C:\\data".to_vec()).is_err());
    assert!(ArchiveLogicalKey::new(ArchiveOwner::CANONICAL, 1, vec![b'x'; 257]).is_err());

    let (mut entries, _) = fixture();
    entries.retain(|entry| entry.kind != ArchiveEntryKind::SignerRecoveryPolicy);
    assert!(DatasetManifestV1::build(
        compatibility(1),
        ProducerArtifactIdentityV1::Unknown,
        entries
    )
    .is_err());

    let (mut entries, _) = fixture();
    entries.push(entries[0].clone());
    assert!(DatasetManifestV1::build(
        compatibility(1),
        ProducerArtifactIdentityV1::Unknown,
        entries
    )
    .is_err());
}

#[test]
fn payload_root_high_water_and_aggregate_mutations_fail_closed() {
    let mut bad_payload = FakeSource::valid();
    let id = *bad_payload.payloads.keys().next().unwrap();
    let payload = bad_payload.payloads.get_mut(&id).unwrap();
    payload[0] ^= 1;
    assert!(capture_dataset(
        &bad_payload,
        compatibility(1),
        ProducerArtifactIdentityV1::Unknown
    )
    .is_err());

    let mut bad_root = FakeSource::valid();
    bad_root.canonical_root[0] ^= 1;
    assert!(capture_dataset(
        &bad_root,
        compatibility(1),
        ProducerArtifactIdentityV1::Unknown
    )
    .is_err());

    let mut bad_high_water = FakeSource::valid();
    bad_high_water.high_water_root[0] ^= 1;
    assert!(capture_dataset(
        &bad_high_water,
        compatibility(1),
        ProducerArtifactIdentityV1::Unknown
    )
    .is_err());

    let (entries, _) = fixture();
    let mut manifest = DatasetManifestV1::build(
        compatibility(1),
        ProducerArtifactIdentityV1::Unknown,
        entries,
    )
    .unwrap();
    manifest.aggregate_root[0] ^= 1;
    assert!(manifest.validate().is_err());
}

#[test]
fn lease_mutation_and_metadata_length_or_hash_mutation_fail_closed() {
    let mut source = FakeSource::valid();
    source.mutate_during_read = true;
    assert!(capture_dataset(
        &source,
        compatibility(1),
        ProducerArtifactIdentityV1::Unknown
    )
    .is_err());

    let (entries, _) = fixture();
    let mut manifest = DatasetManifestV1::build(
        compatibility(1),
        ProducerArtifactIdentityV1::Unknown,
        entries,
    )
    .unwrap();
    manifest.entries[0].length += 1;
    assert!(manifest.validate().is_err());

    let (entries, _) = fixture();
    let mut manifest = DatasetManifestV1::build(
        compatibility(1),
        ProducerArtifactIdentityV1::Unknown,
        entries,
    )
    .unwrap();
    manifest.entries[0].blake3[0] ^= 1;
    assert!(manifest.validate().is_err());
}

#[test]
fn portable_data_gates_restore_while_producer_identity_is_provenance() {
    let (entries, _) = fixture();
    let development = DatasetManifestV1::build(
        compatibility(1),
        ProducerArtifactIdentityV1::Unknown,
        entries.clone(),
    )
    .unwrap();
    let other_target = DatasetManifestV1::build(
        compatibility(1),
        ProducerArtifactIdentityV1::Known([77; 32]),
        entries.clone(),
    )
    .unwrap();
    assert!(development.portable_compatible_with(&other_target));
    assert!(!development.supports_qualified_release_claim());
    assert!(other_target.supports_qualified_release_claim());

    for incompatible in [
        compatibility(2),
        PortableDataCompatibilityV1 {
            storage_schema_version: 2,
            ..compatibility(1)
        },
        PortableDataCompatibilityV1 {
            archive_profile: PortableProfileVersion { major: 3, minor: 0 },
            ..compatibility(1)
        },
        PortableDataCompatibilityV1 {
            migration_profile: PortableProfileVersion { major: 2, minor: 0 },
            ..compatibility(1)
        },
    ] {
        let target = DatasetManifestV1::build(
            incompatible,
            ProducerArtifactIdentityV1::Known([77; 32]),
            entries.clone(),
        )
        .unwrap();
        assert!(!development.portable_compatible_with(&target));
    }
}
