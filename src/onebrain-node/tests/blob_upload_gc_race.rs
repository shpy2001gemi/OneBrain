use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ku_core::blob_store::{BlobCid, BlobType};
use ku_core::foundation::{
    schema_registry::OBJECT_KIND_SOURCE_ARTIFACT, BlobRetentionState, DisclosureClass, EventCid,
    KnowledgeObjectEnvelope, ObjectKind, ObjectReference, OwnedBlobReferenceV1, OwnedBlobRole,
    ReservedDomain, ResourceProfile, SchemaVersion,
};
use ku_kql::blob_storage::{BlobStorage, BlobStorageConfig, BlobStorageError};
use onebrain_node::archive::{OwnedBlobArchiveBackend, SnapshotVerifiedBackend};
use onebrain_node::{
    BaseStorageOwnerId, BlobAuthority, BlobAuthorityError, DatasetGenerationId,
    DatasetPathResolver, PendingBlobUploadId, PendingUploadIdSource,
    ValidatedBlobAuthoritySnapshot, ValidatedBlobReferenceSource,
};

struct TestResolver {
    root: PathBuf,
    generation: Mutex<DatasetGenerationId>,
}

impl DatasetPathResolver for TestResolver {
    fn current_generation(&self) -> DatasetGenerationId {
        *self.generation.lock().unwrap()
    }

    fn owner_path(&self, owner: BaseStorageOwnerId) -> Result<PathBuf, BlobStorageError> {
        if owner != BaseStorageOwnerId::PENDING_BLOB_INTENT {
            return Err(BlobStorageError::InvalidConfig);
        }
        let path = self.root.join("pending");
        std::fs::create_dir_all(&path)?;
        Ok(path)
    }
}

enum IdOutcome {
    Id(PendingBlobUploadId),
    EntropyFailure,
}

struct SequenceIds(Mutex<VecDeque<IdOutcome>>);

impl PendingUploadIdSource for SequenceIds {
    fn next_id(&self) -> Result<PendingBlobUploadId, BlobAuthorityError> {
        match self.0.lock().unwrap().pop_front().unwrap() {
            IdOutcome::Id(id) => Ok(id),
            IdOutcome::EntropyFailure => Err(BlobAuthorityError::EntropyUnavailable),
        }
    }
}

#[derive(Default)]
struct Records {
    objects: Mutex<Vec<Vec<u8>>>,
    events: Mutex<Vec<Vec<u8>>>,
}

impl ValidatedBlobReferenceSource for Records {
    fn snapshot(&self) -> Result<ValidatedBlobAuthoritySnapshot, BlobAuthorityError> {
        Ok(ValidatedBlobAuthoritySnapshot {
            objects: self.objects.lock().unwrap().clone(),
            events: self.events.lock().unwrap().clone(),
        })
    }
}

#[test]
fn pending_upload_prevents_gc_until_abort_then_becomes_collectable() {
    let temp = tempfile::tempdir().unwrap();
    let records = Arc::new(Records::default());
    let authority = authority(
        temp.path().join("authority"),
        records,
        vec![IdOutcome::Id(id(1))],
    );
    let data = b"pending owned bytes";
    let cid = BlobCid::from_content(BlobType::Raw, data);
    let owner = ObjectReference::new(0, [0x21; 32]);
    let pending = authority
        .prepare(owner, cid, BlobType::Raw, data.len() as u64)
        .unwrap();
    let storage =
        BlobStorage::open_with_config(&temp.path().join("blob.redb"), config(), authority.oracle())
            .unwrap();
    storage.store_bytes("pending", data, BlobType::Raw).unwrap();

    assert_eq!(storage.garbage_collect().unwrap().0, 0);
    assert!(authority.abort(pending.id).unwrap());
    assert_eq!(storage.garbage_collect().unwrap().0, 1);
}

#[test]
fn archive_requires_every_canonical_owned_blob_and_restores_by_logical_cid() {
    let temp = tempfile::tempdir().unwrap();
    let records = Arc::new(Records::default());
    let source_authority = authority(
        temp.path().join("source-authority"),
        records.clone(),
        vec![],
    );
    let data = b"archive-owned-blob";
    let cid = BlobCid::from_content(BlobType::Document, data);
    let reference = OwnedBlobReferenceV1::new(
        ObjectReference::new(0, [0x27; 32]),
        cid,
        OwnedBlobRole::Attachment,
        BlobRetentionState::Live,
        None,
    )
    .unwrap();
    records
        .objects
        .lock()
        .unwrap()
        .push(reference_object(&reference));
    let source_storage = Arc::new(
        BlobStorage::open_with_config(
            &temp.path().join("source.redb"),
            config(),
            source_authority.oracle(),
        )
        .unwrap(),
    );
    let source =
        OwnedBlobArchiveBackend::new(source_storage.clone(), source_authority.canonical_oracle());

    assert!(
        source.bounded_snapshot().is_err(),
        "missing blob must block"
    );
    source_storage
        .store_bytes("owned", data, BlobType::Document)
        .unwrap();
    let rows = source.bounded_snapshot().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].key, cid.0);

    let target_records = Arc::new(Records::default());
    target_records
        .objects
        .lock()
        .unwrap()
        .push(reference_object(&reference));
    let target_authority = authority(temp.path().join("target-authority"), target_records, vec![]);
    let target_storage = Arc::new(
        BlobStorage::open_with_config(
            &temp.path().join("target.redb"),
            config(),
            target_authority.oracle(),
        )
        .unwrap(),
    );
    let target =
        OwnedBlobArchiveBackend::new(target_storage.clone(), target_authority.canonical_oracle());
    target.restore_validated(&rows[0]).unwrap();
    target.reconcile_after_restore().unwrap();
    assert_eq!(target_storage.read_full_blob(&cid).unwrap(), data);
}

#[test]
fn canonical_owner_confirmation_is_exact_and_legacy_or_forged_bytes_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let records = Arc::new(Records::default());
    let authority = authority(
        temp.path().join("authority"),
        records.clone(),
        vec![IdOutcome::Id(id(2))],
    );
    let data = b"confirmed";
    let cid = BlobCid::from_content(BlobType::Document, data);
    let owner = ObjectReference::new(0, [0x31; 32]);
    let pending = authority
        .prepare(owner.clone(), cid, BlobType::Document, data.len() as u64)
        .unwrap();
    assert!(matches!(
        authority.confirm_canonical_owner(pending.id),
        Err(BlobAuthorityError::BindingMismatch)
    ));

    let wrong = OwnedBlobReferenceV1::new(
        ObjectReference::new(0, [0x32; 32]),
        cid,
        OwnedBlobRole::Attachment,
        BlobRetentionState::Live,
        None,
    )
    .unwrap();
    records
        .objects
        .lock()
        .unwrap()
        .push(reference_object(&wrong));
    assert!(matches!(
        authority.confirm_canonical_owner(pending.id),
        Err(BlobAuthorityError::BindingMismatch)
    ));

    records.objects.lock().unwrap().clear();
    let terminal_event_bytes = b"accepted validated terminal event".to_vec();
    let terminal_event = EventCid::compute(ReservedDomain::Event, &terminal_event_bytes).unwrap();
    records.events.lock().unwrap().push(terminal_event_bytes);
    let correct = OwnedBlobReferenceV1::new(
        owner,
        cid,
        OwnedBlobRole::Attachment,
        BlobRetentionState::TerminalRetain,
        Some(terminal_event),
    )
    .unwrap();
    records
        .objects
        .lock()
        .unwrap()
        .push(reference_object(&correct));
    authority.confirm_canonical_owner(pending.id).unwrap();
    assert!(authority.pending().list().unwrap().is_empty());

    records
        .objects
        .lock()
        .unwrap()
        .push(b"legacy-forged".to_vec());
    let storage =
        BlobStorage::open_with_config(&temp.path().join("blob.redb"), config(), authority.oracle())
            .unwrap();
    storage
        .store_bytes("confirmed", data, BlobType::Document)
        .unwrap();
    assert!(matches!(
        storage.garbage_collect(),
        Err(BlobStorageError::ReferenceParityUnknown)
    ));
}

#[test]
fn entropy_failure_and_deterministic_collision_leave_original_intent_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let records = Arc::new(Records::default());
    let collision = id(7);
    let authority = authority(
        temp.path().join("authority"),
        records,
        vec![
            IdOutcome::EntropyFailure,
            IdOutcome::Id(collision),
            IdOutcome::Id(collision),
        ],
    );
    let cid = BlobCid::from_content(BlobType::Raw, b"id");
    let owner = ObjectReference::new(0, [0x51; 32]);
    assert!(matches!(
        authority.prepare(owner.clone(), cid, BlobType::Raw, 2),
        Err(BlobAuthorityError::EntropyUnavailable)
    ));
    assert!(authority.pending().list().unwrap().is_empty());
    authority
        .prepare(owner.clone(), cid, BlobType::Raw, 2)
        .unwrap();
    assert!(matches!(
        authority.prepare(owner, cid, BlobType::Raw, 2),
        Err(BlobAuthorityError::IdCollision(value)) if value == collision
    ));
    assert_eq!(authority.pending().list().unwrap().len(), 1);
}

#[test]
fn wrong_cid_type_and_generation_reopen_are_reconciled_without_wall_clock_expiry() {
    let temp = tempfile::tempdir().unwrap();
    let resolver = Arc::new(TestResolver {
        root: temp.path().join("authority"),
        generation: Mutex::new(DatasetGenerationId([0x61; 32])),
    });
    let authority = BlobAuthority::new(
        resolver.clone(),
        Arc::new(SequenceIds(Mutex::new(VecDeque::from(vec![
            IdOutcome::Id(id(8)),
        ])))),
        Arc::new(Records::default()),
    );
    let cid = BlobCid::from_content(BlobType::Raw, b"generation");
    assert!(matches!(
        authority.prepare(
            ObjectReference::new(0, [0x62; 32]),
            cid,
            BlobType::Document,
            10,
        ),
        Err(BlobAuthorityError::BindingMismatch)
    ));
    authority
        .prepare(ObjectReference::new(0, [0x62; 32]), cid, BlobType::Raw, 10)
        .unwrap();
    *resolver.generation.lock().unwrap() = DatasetGenerationId([0x63; 32]);
    assert_eq!(authority.pending().reconcile_generation().unwrap(), 1);
    assert!(authority.pending().list().unwrap().is_empty());
}

#[cfg(feature = "vnext-crash-harness")]
const CHILD_ENV: &str = "ONEBRAIN_BLOB_UPLOAD_CHILD";
#[cfg(feature = "vnext-crash-harness")]
const ROOT_ENV: &str = "ONEBRAIN_BLOB_UPLOAD_ROOT";
#[cfg(feature = "vnext-crash-harness")]
const CHILD_TEST: &str = "blob_upload_transaction_worker";

#[cfg(feature = "vnext-crash-harness")]
#[test]
fn blob_upload_transaction_worker() {
    if std::env::var_os(CHILD_ENV).is_none() {
        return;
    }
    let root = PathBuf::from(std::env::var_os(ROOT_ENV).unwrap());
    let authority = authority(
        root,
        Arc::new(Records::default()),
        vec![IdOutcome::Id(id(9))],
    );
    let cid = BlobCid::from_content(BlobType::Raw, b"worker");
    authority
        .prepare(ObjectReference::new(0, [0x71; 32]), cid, BlobType::Raw, 6)
        .unwrap();
}

#[cfg(feature = "vnext-crash-harness")]
#[test]
fn child_process_kill_matrix_reopens_to_exact_pre_or_post_intent() {
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    for phase in ku_core::foundation::dr_m5_failpoint::FAILPOINT_PHASES {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("authority");
        let marker = temp.path().join("marker.json");
        let token = format!("blob-upload-{phase}-{}", std::process::id());
        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg(CHILD_TEST)
            .arg("--nocapture")
            .env(CHILD_ENV, "1")
            .env(ROOT_ENV, &root)
            .env(ku_core::foundation::dr_m5_failpoint::ENABLE_ENV, "1")
            .env(
                ku_core::foundation::dr_m5_failpoint::FAILPOINT_ENV,
                format!("TX-BLOB-UPLOAD-001:{phase}"),
            )
            .env(ku_core::foundation::dr_m5_failpoint::MARKER_ENV, &marker)
            .env(ku_core::foundation::dr_m5_failpoint::TOKEN_ENV, &token)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !marker.exists() && Instant::now() < deadline {
            assert!(child.try_wait().unwrap().is_none());
            thread::sleep(Duration::from_millis(10));
        }
        assert!(marker.exists(), "phase {phase} did not arm");
        child.kill().unwrap();
        assert!(!child.wait().unwrap().success());

        let reopened = authority(
            root,
            Arc::new(Records::default()),
            vec![IdOutcome::Id(id(10))],
        );
        let count = reopened.pending().list().unwrap().len();
        let expected = usize::from(matches!(
            phase,
            "after_commit_before_next_side_effect" | "after_next_side_effect_before_ack"
        ));
        assert_eq!(count, expected, "phase {phase}");
    }
}

fn authority(root: PathBuf, records: Arc<Records>, ids: Vec<IdOutcome>) -> BlobAuthority {
    BlobAuthority::new(
        Arc::new(TestResolver {
            root,
            generation: Mutex::new(DatasetGenerationId::BOOTSTRAP),
        }),
        Arc::new(SequenceIds(Mutex::new(VecDeque::from(ids)))),
        records,
    )
}

fn config() -> BlobStorageConfig {
    BlobStorageConfig {
        total_quota_bytes: 1024 * 1024,
        free_space_reserve_bytes: 1,
    }
}

const fn id(marker: u8) -> PendingBlobUploadId {
    PendingBlobUploadId::from_bytes([marker; 32])
}

fn reference_object(reference: &OwnedBlobReferenceV1) -> Vec<u8> {
    KnowledgeObjectEnvelope::new(
        ObjectKind(OBJECT_KIND_SOURCE_ARTIFACT),
        SchemaVersion::new(1, 0),
        DisclosureClass::Public,
        reference.to_value(),
    )
    .encode(ResourceProfile::ObjectV1)
    .unwrap()
    .0
}
