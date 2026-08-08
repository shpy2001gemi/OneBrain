use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use onebrain_archive::{
    capture_dataset, compute_high_water_root, seal_archive, verify_dataset_archive_v2,
    ArchiveCredential, ArchiveEntryId, ArchiveEntryKind, ArchiveEntryV1, ArchiveError,
    ArchiveLimits, ArchiveLogicalKey, ArchiveOwner, ArchiveRestorePolicyV1, DatasetManifestV1,
    FileSecureSpoolFactory, PortableDataCompatibilityV1, PortableProfileVersion,
    ProducerArtifactIdentityV1, SnapshotLease, SnapshotSource,
};
use onebrain_node::{
    ActivationPhase, DatasetGenerationId, DatasetGenerationStore, DatasetPathResolver,
    RestoreError, RestoreOperationBinding,
};
use tempfile::tempdir;

fn compatibility() -> PortableDataCompatibilityV1 {
    PortableDataCompatibilityV1 {
        canonical_schema_digest: [1; 32],
        domain_registry_digest: [2; 32],
        resource_registry_digest: [3; 32],
        storage_schema_version: 1,
        archive_profile: PortableProfileVersion { major: 2, minor: 0 },
        migration_profile: PortableProfileVersion { major: 1, minor: 0 },
    }
}

fn policy() -> ArchiveRestorePolicyV1 {
    let value = compatibility();
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
    fn with_registry_high_water(registry_high_water: &[u8]) -> Self {
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
                "Config",
                b"v1".as_slice(),
            ),
            (
                ArchiveEntryKind::RegistryHighWater,
                ArchiveOwner::REGISTRY_METADATA,
                "config",
                registry_high_water,
            ),
            (
                ArchiveEntryKind::SignerRecoveryPolicy,
                ArchiveOwner::IDENTITY,
                "signer",
                b"reprovision-required".as_slice(),
            ),
            (
                ArchiveEntryKind::OperationalRecord,
                ArchiveOwner::OPERATIONAL,
                "Case",
                b"upper".as_slice(),
            ),
            (
                ArchiveEntryKind::OperationalRecord,
                ArchiveOwner::OPERATIONAL,
                "case",
                b"lower".as_slice(),
            ),
        ];
        let mut entries = Vec::new();
        let mut payloads = BTreeMap::new();
        for (kind, owner, key, payload) in rows {
            let entry = ArchiveEntryV1::new(
                kind,
                ArchiveLogicalKey::new(owner, 1, key.as_bytes().to_vec()).unwrap(),
                payload.len() as u64,
                *blake3::hash(payload).as_bytes(),
                true,
            )
            .unwrap();
            payloads.insert(entry.id, payload.to_vec());
            entries.push(entry);
        }
        let manifest = DatasetManifestV1::build(
            compatibility(),
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
            source_binding: [8; 32],
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
    archive_with_registry_high_water(b"42")
}

fn archive_with_registry_high_water(registry_high_water: &[u8]) -> (Vec<u8>, ArchiveCredential) {
    let plaintext = capture_dataset(
        &Source::with_registry_high_water(registry_high_water),
        compatibility(),
        ProducerArtifactIdentityV1::Unknown,
    )
    .unwrap()
    .canonical_plaintext()
    .unwrap();
    let credential = ArchiveCredential::password(b"generation-password".to_vec()).unwrap();
    let mut archive = Vec::new();
    seal_archive(
        Cursor::new(plaintext),
        &mut archive,
        &credential,
        &ArchiveLimits::default(),
    )
    .unwrap();
    (archive, credential)
}

fn stage_ready(
    store: &DatasetGenerationStore,
    spool: &Path,
) -> onebrain_node::ActivationReadyGeneration {
    stage_ready_with_registry_high_water(store, spool, b"42")
}

fn stage_ready_with_registry_high_water(
    store: &DatasetGenerationStore,
    spool: &Path,
    registry_high_water: &[u8],
) -> onebrain_node::ActivationReadyGeneration {
    let (archive, credential) = archive_with_registry_high_water(registry_high_water);
    let factory = FileSecureSpoolFactory::new(spool).unwrap();
    let verified = verify_dataset_archive_v2(
        Cursor::new(archive),
        &factory,
        &credential,
        &ArchiveLimits::default(),
    )
    .unwrap();
    let staged = store.stage_verified_restore(verified, &policy()).unwrap();
    store.prepare_activation(staged).unwrap()
}

#[test]
fn reused_idempotency_key_with_a_different_archive_root_fails_closed() {
    let directory = tempdir().unwrap();
    let spool = tempdir().unwrap();
    let store = DatasetGenerationStore::open_exclusive(directory.path()).unwrap();
    let first = RestoreOperationBinding {
        operation_id: [41; 32],
        idempotency_key: [99; 32],
    };
    let receipt = store
        .activate(stage_ready(&store, spool.path()), first)
        .unwrap();
    let second = RestoreOperationBinding {
        operation_id: [42; 32],
        idempotency_key: [99; 32],
    };
    let ready = stage_ready_with_registry_high_water(&store, spool.path(), b"43");
    assert!(matches!(
        store.activate(ready, second),
        Err(RestoreError::OperationConflict)
    ));
    assert_eq!(store.current_generation().0, receipt.new_generation_root);
}

#[test]
fn activation_reopen_and_original_operation_reconcile_to_new_complete() {
    for (marker, failpoint) in [
        (11, "after_pointer"),
        (12, "after_pointer_journal"),
        (13, "after_receipt"),
    ] {
        let directory = tempdir().unwrap();
        let spool = tempdir().unwrap();
        let operation = RestoreOperationBinding {
            operation_id: [marker; 32],
            idempotency_key: [marker + 20; 32],
        };
        let new_root;
        {
            let store = DatasetGenerationStore::open_exclusive(directory.path()).unwrap();
            let ready = stage_ready(&store, spool.path());
            std::env::set_var("ONEBRAIN_DATASET_FAILPOINT", failpoint);
            assert!(matches!(
                store.activate(ready, operation),
                Err(RestoreError::InjectedFailure)
            ));
            std::env::remove_var("ONEBRAIN_DATASET_FAILPOINT");
            new_root = store.current_generation();
            assert_ne!(new_root, DatasetGenerationId::BOOTSTRAP);
        }
        let reopened = DatasetGenerationStore::open_exclusive(directory.path()).unwrap();
        let receipt = reopened.recover_activation(operation.operation_id).unwrap();
        assert_eq!(receipt.phase, ActivationPhase::Complete);
        assert_eq!(reopened.current_generation(), new_root);
    }
}

#[test]
fn prepared_without_pointer_recovers_old_complete_and_target_is_not_reused() {
    let directory = tempdir().unwrap();
    let spool = tempdir().unwrap();
    let operation = RestoreOperationBinding {
        operation_id: [21; 32],
        idempotency_key: [22; 32],
    };
    {
        let store = DatasetGenerationStore::open_exclusive(directory.path()).unwrap();
        let ready = stage_ready(&store, spool.path());
        std::env::set_var("ONEBRAIN_DATASET_FAILPOINT", "after_prepared");
        assert!(matches!(
            store.activate(ready, operation),
            Err(RestoreError::InjectedFailure)
        ));
        std::env::remove_var("ONEBRAIN_DATASET_FAILPOINT");
    }
    let reopened = DatasetGenerationStore::open_exclusive(directory.path()).unwrap();
    let receipt = reopened.recover_activation(operation.operation_id).unwrap();
    assert_eq!(receipt.phase, ActivationPhase::RolledBack);
    assert_eq!(
        reopened.current_resolver().unwrap().current_generation(),
        DatasetGenerationId::BOOTSTRAP
    );

    let (archive, credential) = archive();
    let factory = FileSecureSpoolFactory::new(spool.path()).unwrap();
    let verified = verify_dataset_archive_v2(
        Cursor::new(archive),
        &factory,
        &credential,
        &ArchiveLimits::default(),
    )
    .unwrap();
    assert!(reopened
        .stage_verified_restore(verified, &policy())
        .is_err());
}

#[test]
fn reopen_health_failure_rolls_pointer_back_and_persists_receipt() {
    let directory = tempdir().unwrap();
    let spool = tempdir().unwrap();
    let operation = RestoreOperationBinding {
        operation_id: [23; 32],
        idempotency_key: [24; 32],
    };
    let store = DatasetGenerationStore::open_exclusive(directory.path()).unwrap();
    let ready = stage_ready(&store, spool.path());
    std::env::set_var("ONEBRAIN_DATASET_FAILPOINT", "reopen_health_failure");
    assert!(matches!(
        store.activate(ready, operation),
        Err(RestoreError::HealthCheck)
    ));
    std::env::remove_var("ONEBRAIN_DATASET_FAILPOINT");
    assert_eq!(store.current_generation(), DatasetGenerationId::BOOTSTRAP);
    let receipt = store.recover_activation(operation.operation_id).unwrap();
    assert_eq!(receipt.phase, ActivationPhase::RolledBack);
    drop(store);

    let reopened = DatasetGenerationStore::open_exclusive(directory.path()).unwrap();
    assert_eq!(
        reopened.current_generation(),
        DatasetGenerationId::BOOTSTRAP
    );
    assert_eq!(
        reopened
            .recover_activation(operation.operation_id)
            .unwrap()
            .phase,
        ActivationPhase::RolledBack
    );
}

#[test]
fn crash_before_reopen_with_corrupt_projection_recovers_old_generation() {
    let directory = tempdir().unwrap();
    let spool = tempdir().unwrap();
    let operation = RestoreOperationBinding {
        operation_id: [25; 32],
        idempotency_key: [26; 32],
    };
    let new_root;
    {
        let store = DatasetGenerationStore::open_exclusive(directory.path()).unwrap();
        let ready = stage_ready(&store, spool.path());
        std::env::set_var("ONEBRAIN_DATASET_FAILPOINT", "after_pointer");
        assert!(matches!(
            store.activate(ready, operation),
            Err(RestoreError::InjectedFailure)
        ));
        std::env::remove_var("ONEBRAIN_DATASET_FAILPOINT");
        new_root = store.current_generation().0;
    }
    let generation = directory.path().join("datasets/generations").join(
        new_root
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
    );
    std::fs::write(generation.join("projection-binding.json"), b"{corrupt").unwrap();

    let reopened = DatasetGenerationStore::open_exclusive(directory.path()).unwrap();
    assert_eq!(
        reopened.current_generation(),
        DatasetGenerationId::BOOTSTRAP
    );
    assert_eq!(
        reopened
            .recover_activation(operation.operation_id)
            .unwrap()
            .phase,
        ActivationPhase::RolledBack
    );
}

#[test]
fn torn_newest_pointer_slot_is_repaired_from_terminal_journal() {
    let directory = tempdir().unwrap();
    let spool = tempdir().unwrap();
    let operation = RestoreOperationBinding {
        operation_id: [31; 32],
        idempotency_key: [32; 32],
    };
    let new_root;
    {
        let store = DatasetGenerationStore::open_exclusive(directory.path()).unwrap();
        let receipt = store
            .activate(stage_ready(&store, spool.path()), operation)
            .unwrap();
        new_root = DatasetGenerationId(receipt.new_generation_root);
    }
    std::fs::write(directory.path().join("control/activation.a.json"), b"{torn").unwrap();
    let repaired_journal = DatasetGenerationStore::open_exclusive(directory.path()).unwrap();
    assert_eq!(repaired_journal.current_generation(), new_root);
    drop(repaired_journal);

    std::fs::write(directory.path().join("control/current.b.json"), b"{torn").unwrap();
    let reopened = DatasetGenerationStore::open_exclusive(directory.path()).unwrap();
    assert_eq!(
        reopened.current_resolver().unwrap().current_generation(),
        new_root
    );
    assert_eq!(
        reopened
            .recover_activation(operation.operation_id)
            .unwrap()
            .phase,
        ActivationPhase::Complete
    );
}

#[test]
fn root_lease_rejects_same_root_and_alias_without_mutating_control_state() {
    let directory = tempdir().unwrap();
    let store = DatasetGenerationStore::open_exclusive(directory.path()).unwrap();
    let before = std::fs::read(directory.path().join("control/current.a.json")).unwrap();
    assert!(matches!(
        DatasetGenerationStore::open_exclusive(&directory.path().join(".")),
        Err(RestoreError::DatasetRootInUse)
    ));
    assert_eq!(
        std::fs::read(directory.path().join("control/current.a.json")).unwrap(),
        before
    );
    drop(store);
    DatasetGenerationStore::open_exclusive(directory.path()).unwrap();
}

#[test]
fn child_process_lock_is_released_only_when_holder_dies() {
    let directory = tempdir().unwrap();
    let ready = directory.path().join("child-ready");
    let executable = std::env::current_exe().unwrap();
    let mut child = Command::new(executable)
        .args([
            "--ignored",
            "--exact",
            "root_lease_child_helper",
            "--nocapture",
        ])
        .env("ONEBRAIN_CHILD_ROOT", directory.path())
        .env("ONEBRAIN_CHILD_READY", &ready)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(ready.exists());
    assert!(matches!(
        DatasetGenerationStore::open_exclusive(directory.path()),
        Err(RestoreError::DatasetRootInUse)
    ));
    child.kill().unwrap();
    child.wait().unwrap();
    DatasetGenerationStore::open_exclusive(directory.path()).unwrap();
}

#[test]
#[ignore = "child-process helper"]
fn root_lease_child_helper() {
    let Some(root) = std::env::var_os("ONEBRAIN_CHILD_ROOT") else {
        return;
    };
    let ready = std::env::var_os("ONEBRAIN_CHILD_READY").unwrap();
    let _store = DatasetGenerationStore::open_exclusive(Path::new(&root)).unwrap();
    std::fs::write(ready, b"ready").unwrap();
    std::thread::sleep(Duration::from_secs(30));
}

#[cfg(unix)]
#[test]
fn symlink_alias_is_rejected_before_lock_or_pointer_side_effects() {
    use std::os::unix::fs::symlink;
    let parent = tempdir().unwrap();
    let root = parent.path().join("root");
    DatasetGenerationStore::open_exclusive(&root).unwrap();
    let alias = parent.path().join("alias");
    symlink(&root, &alias).unwrap();
    assert!(matches!(
        DatasetGenerationStore::open_exclusive(&alias),
        Err(RestoreError::UnsafeRoot)
    ));
}
