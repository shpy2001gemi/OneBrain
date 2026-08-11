use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ed25519_dalek::SigningKey;
use ku_core::concept_registry_manifest::{
    manifest_path, ConceptRegistryIndexManifest, ConceptRegistryManifest,
    ConceptRegistrySourceManifest,
};
use ku_core::foundation::{
    ActorRootDelegation, ConceptCcid, DeviceId, DisclosureClass, FeedInception,
    KnowledgeEventEnvelope, LocalSourceTextRecordV1, NamespaceCommitment, ObjectReference,
    ReservedDomain, ResourceProfile, UseEvidencePayload, UseMode, USE_EVIDENCE_EVENT_TYPE,
};
use ku_core::indexed_concept_registry::{
    IndexedConceptRegistry, CCID_INDEX_MAGIC, LABEL_INDEX_MAGIC, REGISTRY_INDEX_VERSION,
};
use ku_core::{
    activate_concept_registry_release, package_concept_registry_release,
    ConceptRegistryReleasePackageInput, ConceptRegistryReleaseSource,
};
use onebrain_archive::{
    ArchiveEntryKind, ArchiveLimits, ArchiveOwner, PortableDataCompatibilityV1,
    PortableProfileVersion,
};
use onebrain_base_contract::{
    ArchiveCapabilityHandleV1, ArchiveChunkV1, ArchiveCredentialKindV1, ArchiveSinkBeginV1,
    ArchiveSinkHandleV1, ArchiveSinkReadV1, ArchiveSourceBeginV1, ArchiveSourcePushV1,
    BaseCommandV1, BaseConfirmRequestV1, BaseIdempotencyKey, BaseManagementRequestV1,
    BaseOperationKindV1, BasePrepareRequestV1, BaseRequestV1, BoundedSecretIngressV1,
    CreateArchiveCommandV1, ResourceBudgetV1, RestoreArchiveCommandV1,
};
use onebrain_node::archive::{
    ArchiveSnapshotRecord, BaseArchiveService, SnapshotVerifiedBackend, StagedArchiveBackendFactory,
};
use onebrain_node::identity_recovery::SignerRecoveryPolicy;
use onebrain_node::signer_ports::{
    ExpectedSignerIdentity, NodeTransportIdentity, SessionPublicKey,
};
use onebrain_node::{
    compiled_base_runtime_config, BaseHostAuthorizer, BaseIntegrationReceipt,
    BaseManagementResponseV1, BaseManagementScope, BaseResponseV1, ConceptRegistryMode,
    DatasetGenerationStore, DatasetPathResolver, NodeConfig, NodeError, OneBrainNode,
};
use zeroize::Zeroizing;

const MAX_ARCHIVE_BYTES: u64 = 16 * 1024 * 1024;

struct AllowIntegrationHost;

impl BaseHostAuthorizer for AllowIntegrationHost {
    fn authenticate(&self, _principal: [u8; 32], proof: &[u8]) -> bool {
        proof == b"task-25-integration-proof"
    }
}

type IntegrationRows = Arc<Mutex<BTreeMap<(u16, u16, Vec<u8>), ArchiveSnapshotRecord>>>;

#[derive(Clone)]
struct IntegrationBackend {
    rows: IntegrationRows,
}

impl IntegrationBackend {
    fn with_rows(rows: Vec<ArchiveSnapshotRecord>) -> Self {
        Self {
            rows: Arc::new(Mutex::new(
                rows.into_iter()
                    .map(|record| {
                        (
                            (record.owner.get(), record.namespace, record.key.clone()),
                            record,
                        )
                    })
                    .collect(),
            )),
        }
    }

    fn empty() -> Self {
        Self::with_rows(Vec::new())
    }
}

impl SnapshotVerifiedBackend for IntegrationBackend {
    fn owns(&self, owner: ArchiveOwner) -> bool {
        !owner.is_disposable_projection()
    }

    fn bounded_snapshot(&self) -> Result<Vec<ArchiveSnapshotRecord>, NodeError> {
        self.rows
            .lock()
            .map_err(|_| NodeError::Storage("integration backend lock failed".into()))
            .map(|rows| rows.values().cloned().collect())
    }

    fn restore_validated(&self, record: &ArchiveSnapshotRecord) -> Result<(), NodeError> {
        if !self.owns(record.owner) || record.namespace == 0 || record.bytes.is_empty() {
            return Err(NodeError::ArchiveCapability(
                "invalid integration archive record".into(),
            ));
        }
        self.rows
            .lock()
            .map_err(|_| NodeError::Storage("integration backend lock failed".into()))?
            .insert(
                (record.owner.get(), record.namespace, record.key.clone()),
                record.clone(),
            );
        Ok(())
    }
}

struct IntegrationRestoreFactory;

impl StagedArchiveBackendFactory for IntegrationRestoreFactory {
    fn open_for_staged_generation(
        &self,
        _resolver: &dyn DatasetPathResolver,
    ) -> Result<Vec<Arc<dyn SnapshotVerifiedBackend>>, NodeError> {
        Ok(vec![Arc::new(IntegrationBackend::empty())])
    }
}

fn row(
    kind: ArchiveEntryKind,
    owner: ArchiveOwner,
    key: &str,
    bytes: &[u8],
) -> ArchiveSnapshotRecord {
    ArchiveSnapshotRecord {
        kind,
        owner,
        namespace: 1,
        key: key.as_bytes().to_vec(),
        bytes: bytes.to_vec(),
        required: true,
    }
}

fn signer_policy() -> Vec<u8> {
    let seed = [0x77; 32];
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
    .expect("signer recovery policy")
}

fn canonical_rows() -> Vec<ArchiveSnapshotRecord> {
    let root_key = SigningKey::from_bytes(&[0x81; 32]);
    let feed_key = SigningKey::from_bytes(&[0x82; 32]);
    let device = DeviceId::from_bytes([0x84; 32]);
    let namespace = NamespaceCommitment::derive(b"base-gate", [0x85; 32]).unwrap();
    let mut feed = FeedInception::new(*feed_key.verifying_key().as_bytes(), namespace, 0, device);
    let feed_id = feed.feed_id().unwrap();
    let authority = ActorRootDelegation::new(
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
    feed.actor_delegation_ref = Some(ReservedDomain::AuthorityEvent.digest(&authority));
    let feed = feed.sign(&feed_key).unwrap().encode().unwrap();
    let author = ku_core::foundation::decode_feed_inception(&feed).unwrap();
    let (object, object_cid) = UseEvidencePayload {
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
    let (event, _) = event.sign(&author, &feed_key).unwrap().encode().unwrap();
    vec![
        row(
            ArchiveEntryKind::CanonicalObject,
            ArchiveOwner::CANONICAL,
            "canonical-object",
            &object,
        ),
        row(
            ArchiveEntryKind::CanonicalEvent,
            ArchiveOwner::CANONICAL,
            "canonical-event",
            &event,
        ),
        row(
            ArchiveEntryKind::FeedInception,
            ArchiveOwner::CANONICAL,
            "feed-inception",
            &feed,
        ),
        row(
            ArchiveEntryKind::AuthorityEvent,
            ArchiveOwner::CANONICAL,
            "authority-event",
            &authority,
        ),
    ]
}

fn integration_rows() -> Vec<ArchiveSnapshotRecord> {
    let source_subject = ObjectReference::new(1, [0x91; 32]);
    let (vault_record, _) = LocalSourceTextRecordV1::new(
        source_subject,
        "private source retained only through the Vault archive boundary".to_owned(),
    )
    .unwrap()
    .encode()
    .unwrap();
    let mut rows = canonical_rows();
    rows.extend([
        row(
            ArchiveEntryKind::AuthorityHighWater,
            ArchiveOwner::CANONICAL,
            "authority-high-water-v1",
            b"authority:1",
        ),
        row(
            ArchiveEntryKind::VaultRecord,
            ArchiveOwner::VAULT,
            "private-source",
            &vault_record,
        ),
        row(
            ArchiveEntryKind::OwnedBlob,
            ArchiveOwner::BLOB,
            "owned-blob",
            b"owned-blob-bytes",
        ),
        row(
            ArchiveEntryKind::PendingBlobUploadIntent,
            ArchiveOwner::PENDING_BLOB_INTENT,
            "pending-blob-owner",
            b"pending-owner-intent",
        ),
        row(
            ArchiveEntryKind::SourceCaptureIntent,
            ArchiveOwner::SOURCE_CAPTURE_INTENT,
            "source-capture",
            b"encrypted-source-capture-intent",
        ),
        row(
            ArchiveEntryKind::OutboxRecord,
            ArchiveOwner::OUTBOX,
            "pending-outbox",
            b"pending-outbox-record",
        ),
        row(
            ArchiveEntryKind::BaseOperationRecord,
            ArchiveOwner::BASE_OPERATIONS,
            "pending-operation",
            b"prepared-operation-record",
        ),
        row(
            ArchiveEntryKind::MigrationState,
            ArchiveOwner::MIGRATION,
            "migration-state-v1",
            b"migration:complete",
        ),
        row(
            ArchiveEntryKind::InterpretationConfig,
            ArchiveOwner::INTERPRETATION_CONFIG,
            "interpretation-config-v1",
            b"base-v1",
        ),
        row(
            ArchiveEntryKind::RegistryHighWater,
            ArchiveOwner::REGISTRY_METADATA,
            "registry-high-water-v1",
            b"registry:1",
        ),
        row(
            ArchiveEntryKind::SignerRecoveryPolicy,
            ArchiveOwner::IDENTITY,
            "node-transport-policy",
            &signer_policy(),
        ),
    ]);
    rows
}

fn write_tiny_obr(path: &std::path::Path) {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"OBR1");
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&1u64.to_le_bytes());
    bytes.extend_from_slice(&1u64.to_le_bytes());
    bytes.extend_from_slice(&[0; 8]);
    bytes.extend_from_slice(&[7; 16]);
    bytes.extend_from_slice(&283u32.to_le_bytes());
    bytes.extend_from_slice(&[0, 0]);
    bytes.extend_from_slice(&5u16.to_le_bytes());
    bytes.extend_from_slice(b"water");
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&5u16.to_le_bytes());
    bytes.extend_from_slice(b"water");
    fs::write(path, &bytes).unwrap();
    let sources = ["wikidata", "wordnet", "geonames", "ncbi", "chebi"]
        .into_iter()
        .map(|name| {
            (
                name.to_owned(),
                ConceptRegistrySourceManifest {
                    snapshot_id: format!("{name}-task25"),
                    source_uri: format!("https://example.test/{name}"),
                    license: "test-license".to_owned(),
                    record_count: 1,
                },
            )
        })
        .collect();
    let manifest = ConceptRegistryManifest {
        manifest_version: 1,
        obr_schema_version: 1,
        builder_version: "task25-builder".to_owned(),
        dedup_policy_version: "task25-dedup".to_owned(),
        built_at_utc: "2026-08-11T00:00:00Z".to_owned(),
        obr_blake3: blake3::hash(&bytes).to_hex().to_string(),
        entry_count: 1,
        label_count: 1,
        sources,
        label_index: None,
        ccid_index: None,
    };
    fs::write(
        manifest_path(path),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
}

fn install_signed_registry(root: &std::path::Path) -> (SigningKey, [u8; 32]) {
    let source = root.join("registry-source");
    let registry = root.join("registry");
    fs::create_dir_all(&source).unwrap();
    let obr = source.join("registry.obr");
    write_tiny_obr(&obr);
    let checksum = *blake3::hash(&fs::read(&obr).unwrap()).as_bytes();
    let write_index = |path: &std::path::Path, magic: [u8; 4], key: [u8; 16]| {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&magic);
        bytes.extend_from_slice(&REGISTRY_INDEX_VERSION.to_le_bytes());
        bytes.extend_from_slice(&1u64.to_le_bytes());
        bytes.extend_from_slice(&checksum);
        bytes.extend_from_slice(&[0; 16]);
        bytes.extend_from_slice(&key);
        bytes.extend_from_slice(&32u64.to_le_bytes());
        fs::write(path, bytes).unwrap();
    };
    let label = IndexedConceptRegistry::label_index_path(&obr);
    let ccid = IndexedConceptRegistry::ccid_index_path(&obr);
    write_index(
        &label,
        LABEL_INDEX_MAGIC,
        blake3::hash(b"water").as_bytes()[..16].try_into().unwrap(),
    );
    write_index(&ccid, CCID_INDEX_MAGIC, [7; 16]);
    let mut manifest: ConceptRegistryManifest =
        serde_json::from_slice(&fs::read(manifest_path(&obr)).unwrap()).unwrap();
    let index_manifest = |path: &std::path::Path| ConceptRegistryIndexManifest {
        schema_version: 1,
        record_size: 24,
        record_count: 1,
        blake3: blake3::hash(&fs::read(path).unwrap()).to_hex().to_string(),
        file_size: fs::metadata(path).unwrap().len(),
    };
    manifest.label_index = Some(index_manifest(&label));
    manifest.ccid_index = Some(index_manifest(&ccid));
    fs::write(
        manifest_path(&obr),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let sbom = source.join("sbom.spdx.json");
    fs::write(
        &sbom,
        br#"{"spdxVersion":"SPDX-2.3","dataLicense":"CC0-1.0"}"#,
    )
    .unwrap();
    let release_source = |name: &str, marker: u8| ConceptRegistryReleaseSource {
        name: name.to_owned(),
        snapshot_id: format!("{name}-task25"),
        source_uri: format!("https://example.test/{name}"),
        license: "test-license".to_owned(),
        snapshot_blake3: blake3::hash(&[marker; 8]).to_hex().to_string(),
        download_blake3: blake3::hash(&[marker + 1; 8]).to_hex().to_string(),
    };
    let sources = ["chebi", "geonames", "ncbi", "wikidata", "wordnet"]
        .into_iter()
        .enumerate()
        .map(|(index, name)| release_source(name, index as u8 + 1))
        .collect();
    let key = SigningKey::from_bytes(&[0x42; 32]);
    let stamp = package_concept_registry_release(
        &obr,
        &sbom,
        &registry,
        ConceptRegistryReleasePackageInput {
            release_id: "registry-task25".to_owned(),
            sources,
        },
        &key,
    )
    .unwrap();
    activate_concept_registry_release(&registry, "registry-task25", &key.verifying_key()).unwrap();
    (key, decode_hex_32(&stamp.artifact_root))
}

fn decode_hex_32(value: &str) -> [u8; 32] {
    assert_eq!(value.len(), 64);
    let mut bytes = [0; 32];
    for (index, output) in bytes.iter_mut().enumerate() {
        *output = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).unwrap();
    }
    bytes
}

fn node_config(root: &std::path::Path, key: &SigningKey) -> NodeConfig {
    fs::create_dir_all(root.join("node")).unwrap();
    NodeConfig {
        data_dir: root.join("node"),
        concept_registry_mode: ConceptRegistryMode::Required,
        concept_registry_release_root: Some(root.join("registry")),
        concept_registry_release_public_key: Some(
            key.verifying_key()
                .as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        ),
        ..NodeConfig::default()
    }
}

fn install_base(node: &mut OneBrainNode, root: &std::path::Path) {
    let mut base = compiled_base_runtime_config();
    base.host_authorizer = Arc::new(AllowIntegrationHost);
    let tuple = base.compatibility_policy.current.clone();
    let portable = PortableDataCompatibilityV1 {
        canonical_schema_digest: tuple.canonical_schema_digest.0,
        domain_registry_digest: tuple.domain_registry_digest.0,
        resource_registry_digest: tuple.resource_registry_digest.0,
        storage_schema_version: tuple.storage_schema.0,
        archive_profile: PortableProfileVersion {
            major: tuple.archive_profile.major,
            minor: tuple.archive_profile.minor,
        },
        migration_profile: PortableProfileVersion {
            major: tuple.migration_profile.major,
            minor: tuple.migration_profile.minor,
        },
    };
    let spool = root.join("archive-spool");
    base.archive_factory = Some(Arc::new(
        move |capabilities, generations: Arc<DatasetGenerationStore>, _operation_store| {
            BaseArchiveService::new(
                capabilities,
                generations,
                vec![Arc::new(IntegrationBackend::with_rows(integration_rows()))],
                portable,
                ArchiveLimits::default(),
                &spool,
                None,
                Arc::new(Mutex::new(())),
            )
            .map(|service| {
                service.with_restore_backend_factory(Arc::new(IntegrationRestoreFactory))
            })
        },
    ));
    node.install_base_runtime(base).unwrap();
}

fn reservation(response: BaseResponseV1) -> onebrain_base_contract::BaseOperationReservationId {
    match response {
        BaseResponseV1::Reserved(value) => value,
        _ => panic!("expected reservation"),
    }
}

fn budget() -> ResourceBudgetV1 {
    ResourceBudgetV1::try_new(16, 1_048_576, 1_000_000).unwrap()
}

async fn create_archive(node: &OneBrainNode) -> (Vec<u8>, [u8; 32]) {
    let services = node.base_services().unwrap();
    let reservation = reservation(
        services
            .invoke(BaseRequestV1::ReserveOperation(
                BaseOperationKindV1::CreateArchive,
            ))
            .await
            .unwrap(),
    );
    let grant = node
        .issue_base_management_grant(
            [0; 32],
            b"task-25-integration-proof",
            [
                BaseManagementScope::ArchiveSink,
                BaseManagementScope::ArchiveSecret,
            ],
            Duration::from_secs(60),
        )
        .unwrap();
    let management = services.management(grant).unwrap();
    let sink = match management
        .invoke(BaseManagementRequestV1::ArchiveSinkBegin(
            ArchiveSinkBeginV1 {
                reservation_id: reservation,
                max_total_bytes: MAX_ARCHIVE_BYTES,
            },
        ))
        .await
        .unwrap()
    {
        BaseManagementResponseV1::ArchiveSink(value) => value,
        _ => panic!("expected archive sink"),
    };
    let secret = match management
        .invoke(BaseManagementRequestV1::ArchiveSecretRegister(
            BoundedSecretIngressV1::try_new(
                ArchiveCredentialKindV1::Password,
                b"task25-password".to_vec(),
            )
            .unwrap(),
        ))
        .await
        .unwrap()
    {
        BaseManagementResponseV1::ArchiveSecret(value) => value,
        _ => panic!("expected archive secret"),
    };
    let operation = match services
        .invoke(BaseRequestV1::Prepare(BasePrepareRequestV1 {
            reservation_id: reservation,
            command: BaseCommandV1::CreateArchive(CreateArchiveCommandV1 {
                sink,
                secret,
                budget: budget(),
            }),
        }))
        .await
        .unwrap()
    {
        BaseResponseV1::Prepared(value) => value.operation_id,
        _ => panic!("expected prepared create"),
    };
    let receipt = match services
        .invoke(BaseRequestV1::Confirm(BaseConfirmRequestV1 {
            operation_id: operation,
            idempotency_key: BaseIdempotencyKey([0xA1; 32]),
        }))
        .await
        .unwrap()
    {
        BaseResponseV1::Receipt(value) => value,
        _ => panic!("expected create receipt"),
    };
    assert_eq!(receipt.result.len(), 64);
    let readable = ArchiveSinkHandleV1::from_opaque_bytes(receipt.result[..32].try_into().unwrap());
    let manifest_root = receipt.result[32..].try_into().unwrap();
    let mut archive = Vec::new();
    loop {
        let response = management
            .invoke(BaseManagementRequestV1::ArchiveSinkRead(
                ArchiveSinkReadV1 {
                    handle: ArchiveSinkHandleV1::from_opaque_bytes(*readable.as_bytes()),
                    offset: archive.len() as u64,
                    max_bytes: 64 * 1024,
                },
            ))
            .await
            .unwrap();
        let (bytes, eof) = match response {
            BaseManagementResponseV1::ArchiveChunk { bytes, eof, .. } => (bytes, eof),
            _ => panic!("expected archive chunk"),
        };
        archive.extend_from_slice(&bytes);
        if eof {
            break;
        }
    }
    management
        .invoke(BaseManagementRequestV1::ArchiveSinkCommit(
            ArchiveCapabilityHandleV1::from_opaque_bytes(*readable.as_bytes()),
        ))
        .await
        .unwrap();
    management.close().await.unwrap();
    (archive, manifest_root)
}

async fn restore_archive(node: &OneBrainNode, archive: &[u8]) -> [u8; 32] {
    let services = node.base_services().unwrap();
    let reservation = reservation(
        services
            .invoke(BaseRequestV1::ReserveOperation(
                BaseOperationKindV1::RestoreArchive,
            ))
            .await
            .unwrap(),
    );
    let grant = node
        .issue_base_management_grant(
            [0; 32],
            b"task-25-integration-proof",
            [
                BaseManagementScope::ArchiveSource,
                BaseManagementScope::ArchiveSecret,
            ],
            Duration::from_secs(60),
        )
        .unwrap();
    let management = services.management(grant).unwrap();
    let source = match management
        .invoke(BaseManagementRequestV1::ArchiveSourceBegin(
            ArchiveSourceBeginV1 {
                reservation_id: reservation,
                declared_total_bytes: archive.len() as u64,
            },
        ))
        .await
        .unwrap()
    {
        BaseManagementResponseV1::ArchiveSource(value) => value,
        _ => panic!("expected archive source"),
    };
    for (index, chunk) in archive.chunks(256 * 1024).enumerate() {
        management
            .invoke(BaseManagementRequestV1::ArchiveSourcePush(
                ArchiveSourcePushV1 {
                    handle: onebrain_base_contract::ArchiveSourceHandleV1::from_opaque_bytes(
                        *source.as_bytes(),
                    ),
                    offset: (index * 256 * 1024) as u64,
                    chunk: ArchiveChunkV1::try_from_bytes(chunk.to_vec()).unwrap(),
                },
            ))
            .await
            .unwrap();
    }
    let sealed = match management
        .invoke(BaseManagementRequestV1::ArchiveSourceSeal(
            ArchiveCapabilityHandleV1::from_opaque_bytes(*source.as_bytes()),
        ))
        .await
        .unwrap()
    {
        BaseManagementResponseV1::ArchiveSource(value) => value,
        _ => panic!("expected sealed archive source"),
    };
    let secret = match management
        .invoke(BaseManagementRequestV1::ArchiveSecretRegister(
            BoundedSecretIngressV1::try_new(
                ArchiveCredentialKindV1::Password,
                b"task25-password".to_vec(),
            )
            .unwrap(),
        ))
        .await
        .unwrap()
    {
        BaseManagementResponseV1::ArchiveSecret(value) => value,
        _ => panic!("expected archive secret"),
    };
    let operation = match services
        .invoke(BaseRequestV1::Prepare(BasePrepareRequestV1 {
            reservation_id: reservation,
            command: BaseCommandV1::RestoreArchive(RestoreArchiveCommandV1 {
                source: sealed,
                secret,
                budget: budget(),
            }),
        }))
        .await
        .unwrap()
    {
        BaseResponseV1::Prepared(value) => value.operation_id,
        _ => panic!("expected prepared restore"),
    };
    let receipt = match services
        .invoke(BaseRequestV1::Confirm(BaseConfirmRequestV1 {
            operation_id: operation,
            idempotency_key: BaseIdempotencyKey([0xB2; 32]),
        }))
        .await
        .unwrap()
    {
        BaseResponseV1::Receipt(value) => value,
        _ => panic!("expected restore receipt"),
    };
    assert_eq!(receipt.result.len(), 105);
    receipt.result[64..96].try_into().unwrap()
}

#[tokio::test]
async fn task25_integrates_archive_restart_registry_tuple_and_default_off_lanes() {
    let temp = tempfile::tempdir().unwrap();
    let (registry_key, expected_registry_root) = install_signed_registry(temp.path());
    let config = node_config(temp.path(), &registry_key);
    let mut node = OneBrainNode::new(config.clone()).await.unwrap();
    install_base(&mut node, temp.path());

    let expected_kinds = integration_rows()
        .into_iter()
        .map(|record| record.kind)
        .collect::<BTreeSet<_>>();
    for kind in [
        ArchiveEntryKind::CanonicalObject,
        ArchiveEntryKind::CanonicalEvent,
        ArchiveEntryKind::VaultRecord,
        ArchiveEntryKind::OwnedBlob,
        ArchiveEntryKind::OutboxRecord,
        ArchiveEntryKind::BaseOperationRecord,
    ] {
        assert!(expected_kinds.contains(&kind), "fixture omits {kind:?}");
    }

    let (archive, root_before_restart) = create_archive(&node).await;
    assert!(archive.starts_with(b"OBARV002"));
    let archive_restore_root = restore_archive(&node, &archive).await;
    assert_eq!(archive_restore_root, root_before_restart);
    let projection_binding = config
        .base_dataset_root()
        .join("datasets/generations")
        .join(
            archive_restore_root
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        )
        .join("projection-binding.json");
    assert!(
        projection_binding.is_file(),
        "restore did not rebuild projections"
    );

    drop(node);
    let mut reopened = OneBrainNode::new(config).await.unwrap();
    install_base(&mut reopened, temp.path());
    let (_, root_after_restart) = create_archive(&reopened).await;
    assert!(reopened
        .base_integration_receipt([0; 32], root_after_restart, archive_restore_root,)
        .is_err());
    let receipt: BaseIntegrationReceipt = reopened
        .base_integration_receipt(
            root_before_restart,
            root_after_restart,
            archive_restore_root,
        )
        .unwrap();
    let compiled = compiled_base_runtime_config().version_status;
    assert_eq!(
        receipt.candidate_semantic_digest,
        compiled.candidate_semantic_digest.0
    );
    assert_eq!(
        receipt.artifact_tuple_digest,
        compiled.artifact_tuple_digest.0
    );
    assert_eq!(receipt.registry_release_root, expected_registry_root);
    assert_eq!(
        receipt.canonical_root_before_restart,
        receipt.canonical_root_after_restart
    );
    assert_eq!(
        receipt.canonical_root_after_restart,
        receipt.archive_restore_root
    );
    assert_eq!(receipt.default_active_network_lanes, 0);
    assert!(!receipt.legacy_write_enabled);
}
