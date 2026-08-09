use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

use ed25519_dalek::SigningKey;
use ku_core::foundation::{
    ActorRootDelegation, ConceptCcid, DeviceId, DisclosureClass, FeedInception,
    KnowledgeEventEnvelope, LocalSourceTextRecordV1, NamespaceCommitment, ObjectReference,
    ReservedDomain, ResourceProfile, UseEvidencePayload, UseMode, USE_EVIDENCE_EVENT_TYPE,
};
use onebrain_archive::{
    ArchiveCredentialKind, ArchiveEntryKind, ArchiveLimits, ArchiveOwner, ArchiveRestorePolicyV1,
    PortableDataCompatibilityV1, PortableProfileVersion, ProducerArtifactIdentityV1,
};
use onebrain_node::archive::{
    ArchiveSnapshotRecord, BaseArchiveService, SnapshotVerifiedBackend, StagedArchiveBackendFactory,
};
use onebrain_node::identity_recovery::SignerRecoveryPolicy;
use onebrain_node::signer_ports::{
    ExpectedSignerIdentity, NodeTransportIdentity, SessionPublicKey,
};
use onebrain_node::{
    ArchiveCapabilityRegistry, DatasetGenerationStore, DatasetPathResolver, NodeError,
};
use zeroize::Zeroizing;

const MAX_ARCHIVE_BYTES: u64 = 16 * 1024 * 1024;

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

fn restore_policy() -> ArchiveRestorePolicyV1 {
    let compatibility = compatibility();
    ArchiveRestorePolicyV1 {
        canonical_schema_digest: compatibility.canonical_schema_digest,
        domain_registry_digest: compatibility.domain_registry_digest,
        resource_registry_digest: compatibility.resource_registry_digest,
        storage_schema_version: compatibility.storage_schema_version,
        archive_profile: compatibility.archive_profile,
        migration_profile: compatibility.migration_profile,
        max_dataset_bytes: 16 * 1024 * 1024,
    }
}

fn signer_policy() -> Vec<u8> {
    let seed = [77; 32];
    let key = SigningKey::from_bytes(&seed);
    let public_key = *key.verifying_key().as_bytes();
    SignerRecoveryPolicy::ExportableSeedEnvelope {
        expected: ExpectedSignerIdentity::NodeTransport(NodeTransportIdentity {
            session_public_key: SessionPublicKey::from_bytes(public_key),
            principal_node_id: ku_net::vnext_session::principal_node_id(&public_key),
        }),
        sealed_seed: Zeroizing::new(seed.to_vec()),
    }
    .encode()
    .unwrap()
}

fn fixture_rows(include_canonical: bool) -> Vec<ArchiveSnapshotRecord> {
    let (vault_record, _) = LocalSourceTextRecordV1::new(
        ObjectReference::new(1, [9; 32]),
        "exact private source text".to_owned(),
    )
    .unwrap()
    .encode()
    .unwrap();
    let mut rows = vec![
        row(
            ArchiveEntryKind::AuthorityHighWater,
            ArchiveOwner::CANONICAL,
            "authority-high-water",
            b"authority:7",
            true,
        ),
        row(
            ArchiveEntryKind::MigrationState,
            ArchiveOwner::MIGRATION,
            "migration-state",
            b"migration:complete",
            true,
        ),
        row(
            ArchiveEntryKind::InterpretationConfig,
            ArchiveOwner::INTERPRETATION_CONFIG,
            "interpretation-config",
            b"config:v1",
            true,
        ),
        row(
            ArchiveEntryKind::RegistryHighWater,
            ArchiveOwner::REGISTRY_METADATA,
            "registry-high-water",
            b"registry:42",
            true,
        ),
        ArchiveSnapshotRecord {
            kind: ArchiveEntryKind::SignerRecoveryPolicy,
            owner: ArchiveOwner::IDENTITY,
            namespace: 1,
            key: b"node-transport-policy".to_vec(),
            bytes: signer_policy(),
            required: true,
        },
        row(
            ArchiveEntryKind::VaultRecord,
            ArchiveOwner::VAULT,
            "vault-record",
            &vault_record,
            true,
        ),
        row(
            ArchiveEntryKind::QuarantineRecord,
            ArchiveOwner::QUARANTINE,
            "quarantine-record",
            b"non-executable-evidence",
            false,
        ),
        row(
            ArchiveEntryKind::OwnedBlob,
            ArchiveOwner::BLOB,
            "owned-blob",
            b"owned-original-secret-marker",
            true,
        ),
        row(
            ArchiveEntryKind::IdentityEnvelope,
            ArchiveOwner::IDENTITY,
            "identity-envelope",
            b"identity-envelope-v1",
            true,
        ),
        row(
            ArchiveEntryKind::ReconciliationJournalRecord,
            ArchiveOwner::RECONCILIATION,
            "reconciliation-journal",
            b"pending-reconciliation",
            true,
        ),
        row(
            ArchiveEntryKind::InventoryRecord,
            ArchiveOwner::INVENTORY,
            "inventory-record",
            b"selector-root",
            true,
        ),
        row(
            ArchiveEntryKind::OutboxRecord,
            ArchiveOwner::OUTBOX,
            "outbox-record",
            b"pending-outbox",
            true,
        ),
        row(
            ArchiveEntryKind::ProvenanceRecord,
            ArchiveOwner::PROVENANCE,
            "provenance-record",
            b"source-observation",
            true,
        ),
        row(
            ArchiveEntryKind::PrivateNeedRecord,
            ArchiveOwner::PRIVATE_KQL,
            "private-need",
            b"encrypted-private-need-logical-row",
            true,
        ),
        row(
            ArchiveEntryKind::ReceivedUseRecord,
            ArchiveOwner::PRIVATE_POMV,
            "received-use",
            b"received-use-branch",
            true,
        ),
        row(
            ArchiveEntryKind::OperationalRecord,
            ArchiveOwner::OPERATIONAL,
            "operational-record",
            b"operational-state",
            true,
        ),
        row(
            ArchiveEntryKind::RolloutRecord,
            ArchiveOwner::ROLLOUT,
            "rollout-record",
            b"default-off-rollout",
            true,
        ),
        row(
            ArchiveEntryKind::BaseOperationRecord,
            ArchiveOwner::BASE_OPERATIONS,
            "base-operation",
            b"operation-receipt",
            true,
        ),
        row(
            ArchiveEntryKind::PendingBlobUploadIntent,
            ArchiveOwner::PENDING_BLOB_INTENT,
            "pending-blob",
            b"pending-blob-intent",
            true,
        ),
        row(
            ArchiveEntryKind::SourceCaptureIntent,
            ArchiveOwner::SOURCE_CAPTURE_INTENT,
            "source-capture",
            b"encrypted-source-capture-intent",
            true,
        ),
    ];
    if include_canonical {
        rows.extend(canonical_rows());
    }
    rows
}

fn canonical_rows() -> Vec<ArchiveSnapshotRecord> {
    let root_key = SigningKey::from_bytes(&[0x81; 32]);
    let feed_key = SigningKey::from_bytes(&[0x82; 32]);
    let device = DeviceId::from_bytes([0x84; 32]);
    let namespace = NamespaceCommitment::derive(b"archive-roundtrip", [0x85; 32]).unwrap();
    let mut feed = FeedInception::new(*feed_key.verifying_key().as_bytes(), namespace, 0, device);
    let feed_id = feed.feed_id().unwrap();
    let authority_bytes = ActorRootDelegation::new(
        *root_key.verifying_key().as_bytes(),
        feed_id,
        device,
        Some(namespace),
        0,
        0,
    )
    .unwrap()
    .sign(&root_key)
    .unwrap()
    .encode()
    .unwrap();
    feed.actor_delegation_ref = Some(ReservedDomain::AuthorityEvent.digest(&authority_bytes));
    let feed_bytes = feed.sign(&feed_key).unwrap().encode().unwrap();
    let author = ku_core::foundation::decode_feed_inception(&feed_bytes).unwrap();
    let (object_bytes, object_cid) = UseEvidencePayload {
        subjects: vec![ObjectReference::new(0, [0x64; 32])],
        mode: UseMode::Application,
        actor_class: ConceptCcid::from_bytes([0x65; 16]),
        task_context_commitment: [0x66; 32],
        causal_role: ConceptCcid::from_bytes([0x67; 16]),
        assembly: None,
        mapping: None,
        outcome_observation: None,
        use_policy: ObjectReference::new(0, [0x68; 32]),
        observed_frontier: [0x69; 32],
    }
    .to_knowledge_object(DisclosureClass::Public)
    .unwrap()
    .encode(ResourceProfile::ObjectV1)
    .unwrap();
    let mut event = KnowledgeEventEnvelope::new(
        USE_EVIDENCE_EVENT_TYPE,
        author.feed_id,
        0,
        DisclosureClass::Public,
        [0x70; 32],
    );
    event.payload_refs = vec![ObjectReference::new(0, object_cid.into_bytes())];
    let (event_bytes, _) = event.sign(&author, &feed_key).unwrap().encode().unwrap();
    vec![
        row(
            ArchiveEntryKind::CanonicalObject,
            ArchiveOwner::CANONICAL,
            "object-branch",
            &object_bytes,
            true,
        ),
        row(
            ArchiveEntryKind::CanonicalEvent,
            ArchiveOwner::CANONICAL,
            "event-branch",
            &event_bytes,
            true,
        ),
        row(
            ArchiveEntryKind::FeedInception,
            ArchiveOwner::CANONICAL,
            "feed-inception",
            &feed_bytes,
            true,
        ),
        row(
            ArchiveEntryKind::AuthorityEvent,
            ArchiveOwner::CANONICAL,
            "authority-event",
            &authority_bytes,
            true,
        ),
    ]
}

fn row(
    kind: ArchiveEntryKind,
    owner: ArchiveOwner,
    key: &str,
    bytes: &[u8],
    required: bool,
) -> ArchiveSnapshotRecord {
    ArchiveSnapshotRecord {
        kind,
        owner,
        namespace: 1,
        key: key.as_bytes().to_vec(),
        bytes: bytes.to_vec(),
        required,
    }
}

#[derive(Default)]
struct MemoryBackend {
    rows: Mutex<BTreeMap<(u16, Vec<u8>), ArchiveSnapshotRecord>>,
}

impl MemoryBackend {
    fn with_rows(rows: Vec<ArchiveSnapshotRecord>) -> Self {
        Self {
            rows: Mutex::new(
                rows.into_iter()
                    .map(|row| ((row.owner.get(), row.key.clone()), row))
                    .collect(),
            ),
        }
    }
}

impl SnapshotVerifiedBackend for MemoryBackend {
    fn owns(&self, _owner: ArchiveOwner) -> bool {
        true
    }

    fn bounded_snapshot(&self) -> Result<Vec<ArchiveSnapshotRecord>, NodeError> {
        Ok(self.rows.lock().unwrap().values().cloned().collect())
    }

    fn restore_validated(&self, record: &ArchiveSnapshotRecord) -> Result<(), NodeError> {
        let key = (record.owner.get(), record.key.clone());
        let mut rows = self.rows.lock().unwrap();
        match rows.get(&key) {
            Some(existing) if existing == record => Ok(()),
            Some(_) => Err(NodeError::ArchiveCapability(
                "logical restore conflict".into(),
            )),
            None => {
                rows.insert(key, record.clone());
                Ok(())
            }
        }
    }
}

struct Fixture {
    _root: tempfile::TempDir,
    registry: ArchiveCapabilityRegistry,
    service: BaseArchiveService,
    quiesce: Arc<Mutex<()>>,
}

struct MemoryRestoreFactory {
    backends: Vec<Arc<dyn SnapshotVerifiedBackend>>,
}

impl StagedArchiveBackendFactory for MemoryRestoreFactory {
    fn open_for_staged_generation(
        &self,
        _resolver: &dyn DatasetPathResolver,
    ) -> Result<Vec<Arc<dyn SnapshotVerifiedBackend>>, NodeError> {
        Ok(self.backends.clone())
    }
}

fn make_fixture(rows: Vec<ArchiveSnapshotRecord>) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let registry = ArchiveCapabilityRegistry::with_spool_limit(64 * 1024 * 1024).unwrap();
    let generations = Arc::new(
        DatasetGenerationStore::open_exclusive(&root.path().join("base-dataset")).unwrap(),
    );
    let quiesce = Arc::new(Mutex::new(()));
    let backend: Arc<dyn SnapshotVerifiedBackend> = Arc::new(MemoryBackend::with_rows(rows));
    let service = BaseArchiveService::new(
        registry.clone(),
        generations,
        vec![backend.clone()],
        compatibility(),
        ArchiveLimits {
            max_entries: 256,
            max_manifest_bytes: 1024 * 1024,
            max_entry_bytes: 16 * 1024 * 1024,
            max_total_plaintext_bytes: 16 * 1024 * 1024,
            max_spool_bytes: 32 * 1024 * 1024,
        },
        root.path().join("secure-spool"),
        None,
        quiesce.clone(),
    )
    .unwrap()
    .with_restore_backend_factory(Arc::new(MemoryRestoreFactory {
        backends: vec![backend],
    }));
    Fixture {
        _root: root,
        registry,
        service,
        quiesce,
    }
}

async fn create_archive(fixture: &Fixture, password: &[u8]) -> (Vec<u8>, [u8; 32]) {
    let reservation = fixture.registry.reserve_operation().unwrap();
    let sink = fixture
        .registry
        .begin_sink(reservation, MAX_ARCHIVE_BYTES)
        .unwrap();
    let secret = fixture
        .registry
        .register_secret(
            reservation,
            ArchiveCredentialKind::Password,
            Zeroizing::new(password.to_vec()),
        )
        .unwrap();
    let receipt = fixture
        .service
        .create_archive(sink, secret, ProducerArtifactIdentityV1::Unknown)
        .await
        .unwrap();
    let mut bytes = Vec::new();
    let mut offset = 0;
    loop {
        let chunk = fixture
            .registry
            .read_sink_chunk(&receipt.readable_sink, offset, 64 * 1024)
            .unwrap();
        offset += chunk.bytes.len() as u64;
        bytes.extend_from_slice(&chunk.bytes);
        if chunk.eof {
            break;
        }
    }
    fixture.registry.commit_sink(receipt.readable_sink).unwrap();
    (bytes, receipt.manifest_root)
}

async fn restore_archive(
    fixture: &Fixture,
    bytes: &[u8],
    password: &[u8],
) -> Result<onebrain_node::archive::DatasetRestoreReceipt, NodeError> {
    let reservation = fixture.registry.reserve_operation()?;
    let source = fixture
        .registry
        .begin_source(reservation, bytes.len() as u64)?;
    for (index, chunk) in bytes.chunks(17 * 1024).enumerate() {
        fixture
            .registry
            .push_source_chunk(&source, (index * 17 * 1024) as u64, chunk)?;
    }
    let source = fixture.registry.seal_source(source)?;
    let secret = fixture.registry.register_secret(
        reservation,
        ArchiveCredentialKind::Password,
        Zeroizing::new(password.to_vec()),
    )?;
    fixture
        .service
        .restore_archive(source, secret, &restore_policy())
        .await
}

#[tokio::test]
async fn encrypted_logical_fixture_roundtrips_and_rebuilds_excluded_projection() {
    let _serial = environment_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let fixture = make_fixture(fixture_rows(true));
    let (archive, manifest_root) = create_archive(&fixture, b"archive-password").await;
    assert!(archive.starts_with(b"OBARV002"));
    assert!(!archive
        .windows(b"owned-original-secret-marker".len())
        .any(|window| window == b"owned-original-secret-marker"));
    let receipt = restore_archive(&fixture, &archive, b"archive-password")
        .await
        .unwrap();
    assert_eq!(receipt.activation.new_generation_root, manifest_root);
    assert!(receipt.identity.reprovision_required.as_slice().is_empty());
    assert_eq!(receipt.identity.restored.as_slice().len(), 1);
}

#[tokio::test]
async fn wrong_password_modified_byte_and_plaintext_never_activate() {
    let _serial = environment_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let fixture = make_fixture(fixture_rows(false));
    let (archive, _) = create_archive(&fixture, b"correct-password").await;
    assert!(restore_archive(&fixture, &archive, b"wrong-password")
        .await
        .is_err());

    let fixture = make_fixture(fixture_rows(false));
    let mut modified = archive;
    let index = modified.len() / 2;
    modified[index] ^= 1;
    assert!(restore_archive(&fixture, &modified, b"correct-password")
        .await
        .is_err());
}

#[tokio::test]
async fn restore_without_a_staged_target_factory_fails_before_activation() {
    let _serial = environment_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let source = make_fixture(fixture_rows(false));
    let (archive, _) = create_archive(&source, b"correct-password").await;

    let root = tempfile::tempdir().unwrap();
    let registry = ArchiveCapabilityRegistry::new().unwrap();
    let generations = Arc::new(
        DatasetGenerationStore::open_exclusive(&root.path().join("base-dataset")).unwrap(),
    );
    let service = BaseArchiveService::new(
        registry.clone(),
        generations,
        vec![Arc::new(MemoryBackend::with_rows(fixture_rows(false)))],
        compatibility(),
        ArchiveLimits::default(),
        root.path().join("secure-spool"),
        None,
        Arc::new(Mutex::new(())),
    )
    .unwrap();
    let reservation = registry.reserve_operation().unwrap();
    let source = registry
        .begin_source(reservation, archive.len() as u64)
        .unwrap();
    registry.push_source_chunk(&source, 0, &archive).unwrap();
    let source = registry.seal_source(source).unwrap();
    let secret = registry
        .register_secret(
            reservation,
            ArchiveCredentialKind::Password,
            Zeroizing::new(b"correct-password".to_vec()),
        )
        .unwrap();
    assert!(matches!(
        service
            .restore_archive(source, secret, &restore_policy())
            .await,
        Err(NodeError::ArchiveCapability(message))
            if message.contains("target backend factory")
    ));
}

#[tokio::test]
async fn capability_type_state_bounds_disconnect_and_cross_operation_fail_closed() {
    let _serial = environment_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let fixture = make_fixture(fixture_rows(false));
    let first = fixture.registry.reserve_operation().unwrap();
    let second = fixture.registry.reserve_operation().unwrap();
    let source = fixture.registry.begin_source(first, 4).unwrap();
    assert!(fixture
        .registry
        .push_source_chunk(&source, 1, b"ab")
        .is_err());
    fixture
        .registry
        .push_source_chunk(&source, 0, b"ab")
        .unwrap();
    assert!(fixture.registry.seal_source(source).is_err());
    assert_eq!(fixture.registry.active_capability_count().unwrap(), 0);

    let first = fixture.registry.reserve_operation().unwrap();
    let sink = fixture.registry.begin_sink(first, 1024).unwrap();
    let sink_id = sink.id();
    let secret = fixture
        .registry
        .register_secret(
            second,
            ArchiveCredentialKind::Password,
            Zeroizing::new(b"password".to_vec()),
        )
        .unwrap();
    assert!(fixture
        .service
        .create_archive(sink, secret, ProducerArtifactIdentityV1::Unknown)
        .await
        .is_err());
    assert!(fixture.registry.destroy(sink_id).is_err());

    let first = fixture.registry.reserve_operation().unwrap();
    assert!(fixture
        .registry
        .register_secret(
            first,
            ArchiveCredentialKind::RecoveryKey,
            Zeroizing::new(vec![7; 31]),
        )
        .is_err());
    let abandoned = fixture.registry.begin_sink(first, 1024).unwrap();
    let abandoned_id = abandoned.id();
    drop(abandoned);
    assert!(fixture.registry.destroy(abandoned_id).is_err());

    let first = fixture.registry.reserve_operation().unwrap();
    let overflow = fixture.registry.begin_source(first, 4).unwrap();
    assert!(fixture
        .registry
        .push_source_chunk(&overflow, u64::MAX, b"x")
        .is_err());
    let overflow_id = overflow.id();
    fixture.registry.abort(overflow_id).unwrap();
    assert!(fixture.registry.abort(overflow_id).is_err());
    drop(overflow);

    let first = fixture.registry.reserve_operation().unwrap();
    assert!(fixture
        .registry
        .begin_sink(first, 65 * 1024 * 1024)
        .is_err());
    assert!(fixture
        .registry
        .register_secret(
            first,
            ArchiveCredentialKind::Password,
            Zeroizing::new(vec![9; 1025]),
        )
        .is_err());

    let doomed_secret = fixture
        .registry
        .register_secret(
            first,
            ArchiveCredentialKind::Password,
            Zeroizing::new(b"destroy-me".to_vec()),
        )
        .unwrap();
    let doomed_id = doomed_secret.id();
    fixture.registry.destroy(doomed_id).unwrap();
    assert!(fixture.registry.destroy(doomed_id).is_err());
    assert!(fixture.registry.abort(doomed_id).is_err());
    drop(doomed_secret);
    assert_eq!(fixture.registry.active_capability_count().unwrap(), 0);

    for _ in 0..64 {
        let reservation = fixture.registry.reserve_operation().unwrap();
        let capability = fixture
            .registry
            .register_secret(
                reservation,
                ArchiveCredentialKind::Password,
                Zeroizing::new(b"one-shot".to_vec()),
            )
            .unwrap();
        fixture.registry.destroy(capability.id()).unwrap();
        drop(capability);
    }
}

#[tokio::test]
async fn quiesce_failure_and_every_archive_failpoint_publish_no_sink() {
    let _serial = environment_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let fixture = make_fixture(fixture_rows(false));
    let reservation = fixture.registry.reserve_operation().unwrap();
    let sink = fixture
        .registry
        .begin_sink(reservation, MAX_ARCHIVE_BYTES)
        .unwrap();
    let secret = fixture
        .registry
        .register_secret(
            reservation,
            ArchiveCredentialKind::Password,
            Zeroizing::new(b"password".to_vec()),
        )
        .unwrap();
    let held = fixture.quiesce.lock().unwrap();
    assert!(fixture
        .service
        .create_archive(sink, secret, ProducerArtifactIdentityV1::Unknown)
        .await
        .is_err());
    drop(held);
    assert_eq!(fixture.registry.active_capability_count().unwrap(), 0);

    for phase in [
        "before_begin_write",
        "after_begin_write_before_mutation",
        "after_mutation_before_commit",
        "after_commit_before_next_side_effect",
        "after_next_side_effect_before_ack",
    ] {
        let reservation = fixture.registry.reserve_operation().unwrap();
        let sink = fixture
            .registry
            .begin_sink(reservation, MAX_ARCHIVE_BYTES)
            .unwrap();
        let secret = fixture
            .registry
            .register_secret(
                reservation,
                ArchiveCredentialKind::Password,
                Zeroizing::new(b"password".to_vec()),
            )
            .unwrap();
        let failpoint = ArchiveFailpointGuard::set(phase);
        assert!(fixture
            .service
            .create_archive(sink, secret, ProducerArtifactIdentityV1::Unknown)
            .await
            .is_err());
        drop(failpoint);
        assert_eq!(fixture.registry.active_capability_count().unwrap(), 0);
    }
}

#[tokio::test]
async fn stale_process_registry_and_unsafe_logical_keys_are_rejected() {
    let _serial = environment_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let fixture = make_fixture(fixture_rows(false));
    let foreign = ArchiveCapabilityRegistry::new().unwrap();
    let reservation = foreign.reserve_operation().unwrap();
    let source = foreign.begin_source(reservation, 8).unwrap();
    foreign.push_source_chunk(&source, 0, b"OBARV002").unwrap();
    let source = foreign.seal_source(source).unwrap();
    let local_reservation = fixture.registry.reserve_operation().unwrap();
    let secret = fixture
        .registry
        .register_secret(
            local_reservation,
            ArchiveCredentialKind::Password,
            Zeroizing::new(b"password".to_vec()),
        )
        .unwrap();
    assert!(fixture
        .service
        .restore_archive(source, secret, &restore_policy())
        .await
        .is_err());

    let unsafe_fixture = make_fixture(vec![ArchiveSnapshotRecord {
        kind: ArchiveEntryKind::MigrationState,
        owner: ArchiveOwner::MIGRATION,
        namespace: 1,
        key: b"../raw.redb".to_vec(),
        bytes: b"plaintext".to_vec(),
        required: true,
    }]);
    let reservation = unsafe_fixture.registry.reserve_operation().unwrap();
    let sink = unsafe_fixture
        .registry
        .begin_sink(reservation, MAX_ARCHIVE_BYTES)
        .unwrap();
    let secret = unsafe_fixture
        .registry
        .register_secret(
            reservation,
            ArchiveCredentialKind::Password,
            Zeroizing::new(b"password".to_vec()),
        )
        .unwrap();
    assert!(unsafe_fixture
        .service
        .create_archive(sink, secret, ProducerArtifactIdentityV1::Unknown)
        .await
        .is_err());
}

fn environment_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct ArchiveFailpointGuard;

impl ArchiveFailpointGuard {
    fn set(phase: &str) -> Self {
        std::env::set_var("ONEBRAIN_ARCHIVE_FAILPOINT", phase);
        Self
    }
}

impl Drop for ArchiveFailpointGuard {
    fn drop(&mut self) {
        std::env::remove_var("ONEBRAIN_ARCHIVE_FAILPOINT");
    }
}
