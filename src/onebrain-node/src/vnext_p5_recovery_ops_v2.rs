//! Closed recovery operations used by the privileged P5 V2 boundary.
//!
//! Callers select a typed operation. They cannot supply a command, executable,
//! service, or path outside the already verified runner roots.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use onebrain_archive::{
    ArchiveCredentialKind, ArchiveEntryKind, ArchiveLimits, ArchiveOwner, ArchiveRestorePolicyV1,
    PortableProfileVersion,
};
use zeroize::Zeroizing;

use crate::archive::{
    ArchiveSnapshotRecord, BaseArchiveService, SnapshotVerifiedBackend,
    StagedArchiveBackendFactory, VNextStagedArchiveBackendFactory,
};
use crate::identity_recovery::SignerRecoveryPolicy;
use crate::signer_ports::{
    ExpectedSignerIdentity, NodeTransportIdentity, SessionPublicKey, SignerProviderId,
};
use crate::{
    compiled_base_runtime_config, ArchiveCapabilityRegistry, BaseStorageOwnerId,
    DatasetGenerationStore, DatasetPathResolver, NodeError,
};

use crate::vnext_runtime_rollout::{
    VNextRuntimeLane, VNextRuntimeLaneRequest, VNextRuntimeRollout,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum P5RecoveryOperationV2 {
    Obarv002Restore,
    Rollback,
    ExplicitReEnable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5RecoveryInputsV2 {
    pub request_digest: [u8; 32],
    pub session_id: [u8; 32],
    pub host_id: String,
    pub operation_id: [u8; 32],
    pub identity_public_key: [u8; 32],
    pub runner_data_root: PathBuf,
    pub activation_root: PathBuf,
    pub evidence_output: PathBuf,
    pub archive_input: Option<PathBuf>,
    pub archive_recovery_key: Option<PathBuf>,
    pub base_dataset_root: Option<PathBuf>,
    pub previous_generation: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedP5RecoveryInputsV2 {
    pub operation: P5RecoveryOperationV2,
    pub request_digest: [u8; 32],
    pub session_id: [u8; 32],
    pub host_id: String,
    pub operation_id: [u8; 32],
    pub identity_public_key: [u8; 32],
    pub runner_data_root: PathBuf,
    pub activation_root: PathBuf,
    pub evidence_output: PathBuf,
    pub archive_input: Option<PathBuf>,
    pub archive_recovery_key: Option<PathBuf>,
    pub base_dataset_root: Option<PathBuf>,
    pub previous_generation: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5RecoveryReceiptV2 {
    pub operation: P5RecoveryOperationV2,
    pub operation_id: [u8; 32],
    pub state_changed: bool,
    pub evidence_blake3: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5RecoveryFixtureReceiptV2 {
    pub archive_blake3: [u8; 32],
    pub archive_bytes: u64,
    pub dataset_generation: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum P5RecoveryErrorV2 {
    EmptyBinding,
    InvalidHost,
    RootMissing,
    RootNotDirectory,
    ActivationRootMissing,
    EvidenceExists,
    PathEscapesRoot,
    MissingArchive,
    MissingPreviousGeneration,
    UnexpectedInput,
    Io,
    Rollout,
    Activation,
    Archive,
}

const MAX_P5_RECOVERY_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const P5_ARCHIVE_CHUNK_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug)]
struct P5RecoveryArchiveBackendFactory {
    identity_public_key: [u8; 32],
}

impl StagedArchiveBackendFactory for P5RecoveryArchiveBackendFactory {
    fn open_for_staged_generation(
        &self,
        resolver: &dyn DatasetPathResolver,
    ) -> Result<Vec<Arc<dyn SnapshotVerifiedBackend>>, NodeError> {
        let mut backends = VNextStagedArchiveBackendFactory.open_for_staged_generation(resolver)?;
        backends.push(Arc::new(P5RequiredMetadataArchiveBackend::open(
            resolver,
            self.identity_public_key,
        )?));
        Ok(backends)
    }
}

/// The encrypted Base archive profile requires these metadata owners even for
/// a network-only P5 dataset. They are persisted inside the selected dataset
/// generation and are intentionally limited to one closed row per owner.
struct P5RequiredMetadataArchiveBackend {
    migration: PathBuf,
    interpretation: PathBuf,
    registry: PathBuf,
    identity_public_key: [u8; 32],
}

impl P5RequiredMetadataArchiveBackend {
    fn open(
        resolver: &dyn DatasetPathResolver,
        identity_public_key: [u8; 32],
    ) -> Result<Self, NodeError> {
        Ok(Self {
            migration: resolver
                .owner_path(BaseStorageOwnerId::MIGRATION)
                .map_err(|error| NodeError::Storage(error.to_string()))?
                .join("p5-migration-state-v2"),
            interpretation: resolver
                .owner_path(BaseStorageOwnerId::INTERPRETATION_CONFIG)
                .map_err(|error| NodeError::Storage(error.to_string()))?
                .join("p5-interpretation-config-v2"),
            registry: resolver
                .owner_path(BaseStorageOwnerId::REGISTRY_METADATA)
                .map_err(|error| NodeError::Storage(error.to_string()))?
                .join("p5-registry-high-water-v2"),
            identity_public_key,
        })
    }

    fn read_or_default(path: &Path, default: &[u8]) -> Result<Vec<u8>, NodeError> {
        match std::fs::read(path) {
            Ok(bytes) if !bytes.is_empty() && bytes.len() <= 4096 => Ok(bytes),
            Ok(_) => Err(NodeError::ArchiveCapability(
                "P5 archive metadata is empty or oversized".into(),
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(default.to_vec()),
            Err(error) => Err(NodeError::Storage(error.to_string())),
        }
    }

    fn persist_exact(path: &Path, bytes: &[u8]) -> Result<(), NodeError> {
        if bytes.is_empty() || bytes.len() > 4096 {
            return Err(NodeError::ArchiveCapability(
                "P5 archive metadata is empty or oversized".into(),
            ));
        }
        match std::fs::read(path) {
            Ok(existing) if existing == bytes => return Ok(()),
            Ok(_) => {
                return Err(NodeError::ArchiveCapability(
                    "P5 archive metadata restore conflicts".into(),
                ))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(NodeError::Storage(error.to_string())),
        }
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(path)
            .map_err(|error| NodeError::Storage(error.to_string()))?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| NodeError::Storage(error.to_string()))?;
        #[cfg(unix)]
        if let Some(parent) = path.parent() {
            std::fs::File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|error| NodeError::Storage(error.to_string()))?;
        }
        Ok(())
    }
}

impl SnapshotVerifiedBackend for P5RequiredMetadataArchiveBackend {
    fn owns(&self, owner: ArchiveOwner) -> bool {
        owner == ArchiveOwner::MIGRATION
            || owner == ArchiveOwner::INTERPRETATION_CONFIG
            || owner == ArchiveOwner::REGISTRY_METADATA
    }

    fn bounded_snapshot(&self) -> Result<Vec<ArchiveSnapshotRecord>, NodeError> {
        let signer_policy = SignerRecoveryPolicy::ReprovisionRequired {
            expected: ExpectedSignerIdentity::NodeTransport(NodeTransportIdentity {
                session_public_key: SessionPublicKey::from_bytes(self.identity_public_key),
                principal_node_id: ku_net::vnext_session::principal_node_id(
                    &self.identity_public_key,
                ),
            }),
            provider_id: SignerProviderId::new("p5-external-identity-signer-v2")
                .map_err(|error| NodeError::ArchiveCapability(error.to_string()))?,
        }
        .encode()
        .map_err(|error| NodeError::ArchiveCapability(error.to_string()))?;
        Ok(vec![
            ArchiveSnapshotRecord {
                kind: ArchiveEntryKind::MigrationState,
                owner: ArchiveOwner::MIGRATION,
                namespace: 1,
                key: b"p5-migration-state-v2".to_vec(),
                bytes: Self::read_or_default(&self.migration, b"p5-migration-complete-v2")?,
                required: true,
            },
            ArchiveSnapshotRecord {
                kind: ArchiveEntryKind::InterpretationConfig,
                owner: ArchiveOwner::INTERPRETATION_CONFIG,
                namespace: 1,
                key: b"p5-interpretation-config-v2".to_vec(),
                bytes: Self::read_or_default(&self.interpretation, b"p5-interpretation-v2")?,
                required: true,
            },
            ArchiveSnapshotRecord {
                kind: ArchiveEntryKind::RegistryHighWater,
                owner: ArchiveOwner::REGISTRY_METADATA,
                namespace: 1,
                key: b"p5-registry-high-water-v2".to_vec(),
                bytes: Self::read_or_default(&self.registry, b"p5-registry-zero-v2")?,
                required: true,
            },
            ArchiveSnapshotRecord {
                kind: ArchiveEntryKind::SignerRecoveryPolicy,
                owner: ArchiveOwner::IDENTITY,
                namespace: 1,
                key: b"p5-node-transport-reprovision-v2".to_vec(),
                bytes: signer_policy,
                required: true,
            },
        ])
    }

    fn restore_validated(&self, record: &ArchiveSnapshotRecord) -> Result<(), NodeError> {
        match (record.owner, record.kind, record.key.as_slice()) {
            (
                ArchiveOwner::MIGRATION,
                ArchiveEntryKind::MigrationState,
                b"p5-migration-state-v2",
            ) => Self::persist_exact(&self.migration, &record.bytes),
            (
                ArchiveOwner::INTERPRETATION_CONFIG,
                ArchiveEntryKind::InterpretationConfig,
                b"p5-interpretation-config-v2",
            ) => Self::persist_exact(&self.interpretation, &record.bytes),
            (
                ArchiveOwner::REGISTRY_METADATA,
                ArchiveEntryKind::RegistryHighWater,
                b"p5-registry-high-water-v2",
            ) => Self::persist_exact(&self.registry, &record.bytes),
            _ => Err(NodeError::ArchiveCapability(
                "P5 archive metadata row is outside the closed profile".into(),
            )),
        }
    }
}

pub fn verify_inputs(
    operation: P5RecoveryOperationV2,
    input: &P5RecoveryInputsV2,
) -> Result<VerifiedP5RecoveryInputsV2, P5RecoveryErrorV2> {
    if input.request_digest == [0; 32]
        || input.session_id == [0; 32]
        || input.operation_id == [0; 32]
        || input.identity_public_key == [0; 32]
    {
        return Err(P5RecoveryErrorV2::EmptyBinding);
    }
    if input.host_id.is_empty() || input.host_id.len() > 128 || !input.host_id.is_ascii() {
        return Err(P5RecoveryErrorV2::InvalidHost);
    }
    let root = input
        .runner_data_root
        .canonicalize()
        .map_err(|_| P5RecoveryErrorV2::RootMissing)?;
    if !root.is_dir() {
        return Err(P5RecoveryErrorV2::RootNotDirectory);
    }
    let activation_root = input
        .activation_root
        .canonicalize()
        .map_err(|_| P5RecoveryErrorV2::ActivationRootMissing)?;
    if !activation_root.is_dir() {
        return Err(P5RecoveryErrorV2::ActivationRootMissing);
    }
    if input.evidence_output.exists() {
        return Err(P5RecoveryErrorV2::EvidenceExists);
    }
    let evidence_parent = input
        .evidence_output
        .parent()
        .ok_or(P5RecoveryErrorV2::PathEscapesRoot)?;
    let evidence_parent = evidence_parent
        .canonicalize()
        .map_err(|_| P5RecoveryErrorV2::PathEscapesRoot)?;
    if !evidence_parent.starts_with(&root) {
        return Err(P5RecoveryErrorV2::PathEscapesRoot);
    }

    let archive_input = canonical_optional(&root, input.archive_input.as_deref())?;
    let archive_recovery_key = canonical_optional(&root, input.archive_recovery_key.as_deref())?;
    let base_dataset_root = canonical_optional(&root, input.base_dataset_root.as_deref())?;
    let previous_generation =
        canonical_optional(&activation_root, input.previous_generation.as_deref())?;
    match operation {
        P5RecoveryOperationV2::Obarv002Restore
            if archive_input.is_none()
                || archive_recovery_key.is_none()
                || base_dataset_root.is_none() =>
        {
            return Err(P5RecoveryErrorV2::MissingArchive)
        }
        P5RecoveryOperationV2::Rollback if previous_generation.is_none() => {
            return Err(P5RecoveryErrorV2::MissingPreviousGeneration)
        }
        P5RecoveryOperationV2::ExplicitReEnable
            if archive_input.is_some()
                || archive_recovery_key.is_some()
                || base_dataset_root.is_some()
                || previous_generation.is_some() =>
        {
            return Err(P5RecoveryErrorV2::UnexpectedInput)
        }
        P5RecoveryOperationV2::Rollback
            if archive_input.is_some()
                || archive_recovery_key.is_some()
                || base_dataset_root.is_some() =>
        {
            return Err(P5RecoveryErrorV2::UnexpectedInput)
        }
        _ => {}
    }
    Ok(VerifiedP5RecoveryInputsV2 {
        operation,
        request_digest: input.request_digest,
        session_id: input.session_id,
        host_id: input.host_id.clone(),
        operation_id: input.operation_id,
        identity_public_key: input.identity_public_key,
        runner_data_root: root,
        activation_root,
        evidence_output: input.evidence_output.clone(),
        archive_input,
        archive_recovery_key,
        base_dataset_root,
        previous_generation,
    })
}

fn canonical_optional(
    root: &Path,
    value: Option<&Path>,
) -> Result<Option<PathBuf>, P5RecoveryErrorV2> {
    value
        .map(|path| {
            let canonical = path
                .canonicalize()
                .map_err(|_| P5RecoveryErrorV2::PathEscapesRoot)?;
            if !canonical.starts_with(root) {
                return Err(P5RecoveryErrorV2::PathEscapesRoot);
            }
            Ok(canonical)
        })
        .transpose()
}

/// Creates the closed, source-free recovery fixture consumed by the real P5
/// archive-restore fault. The fixture is generated from an actual Base dataset
/// store and sealed as OBARV002; it is never synthesized by the controller.
/// Existing complete fixtures are verified and reused, while partial fixtures
/// fail closed.
pub fn prepare_obarv002_fixture(
    runner_data_root: &Path,
    archive_path: &Path,
    key_path: &Path,
    dataset_root: &Path,
    identity_public_key: [u8; 32],
) -> Result<P5RecoveryFixtureReceiptV2, P5RecoveryErrorV2> {
    if identity_public_key == [0; 32] {
        return Err(P5RecoveryErrorV2::EmptyBinding);
    }
    let root = runner_data_root
        .canonicalize()
        .map_err(|_| P5RecoveryErrorV2::RootMissing)?;
    if !root.is_dir() {
        return Err(P5RecoveryErrorV2::RootNotDirectory);
    }
    let fixture_root = root.join("recovery-input");
    create_private_directory(&fixture_root)?;
    create_private_directory(dataset_root)?;
    let fixture_root = fixture_root
        .canonicalize()
        .map_err(|_| P5RecoveryErrorV2::PathEscapesRoot)?;
    let dataset_root = dataset_root
        .canonicalize()
        .map_err(|_| P5RecoveryErrorV2::PathEscapesRoot)?;
    let archive_parent = archive_path
        .parent()
        .ok_or(P5RecoveryErrorV2::PathEscapesRoot)?
        .canonicalize()
        .map_err(|_| P5RecoveryErrorV2::PathEscapesRoot)?;
    let key_parent = key_path
        .parent()
        .ok_or(P5RecoveryErrorV2::PathEscapesRoot)?
        .canonicalize()
        .map_err(|_| P5RecoveryErrorV2::PathEscapesRoot)?;
    if !fixture_root.starts_with(&root)
        || !dataset_root.starts_with(&fixture_root)
        || archive_parent != fixture_root
        || key_parent != fixture_root
        || archive_path.file_name().and_then(|v| v.to_str()) != Some("base.obar")
        || key_path.file_name().and_then(|v| v.to_str()) != Some("base.key")
    {
        return Err(P5RecoveryErrorV2::PathEscapesRoot);
    }
    match (archive_path.exists(), key_path.exists()) {
        (true, true) => return inspect_fixture(archive_path, key_path, &dataset_root),
        (false, false) => {}
        _ => return Err(P5RecoveryErrorV2::Archive),
    }

    let mut key = [0u8; 32];
    getrandom::fill(&mut key).map_err(|_| P5RecoveryErrorV2::Archive)?;
    let generations = Arc::new(
        DatasetGenerationStore::open_exclusive(&dataset_root)
            .map_err(|_| fixture_archive_error("open-dataset"))?,
    );
    let dataset_generation = generations
        .current_resolver()
        .map_err(|_| fixture_archive_error("current-resolver"))?
        .current_generation()
        .0;
    let factory = Arc::new(P5RecoveryArchiveBackendFactory {
        identity_public_key,
    });
    let source_backends = factory
        .open_for_staged_generation(
            &generations
                .current_resolver()
                .map_err(|_| P5RecoveryErrorV2::Archive)?,
        )
        .map_err(|_| fixture_archive_error("open-backends"))?;
    let registry = ArchiveCapabilityRegistry::with_spool_limit(MAX_P5_RECOVERY_ARCHIVE_BYTES)
        .map_err(|_| fixture_archive_error("capability-registry"))?;
    let policy = compiled_archive_policy();
    let service = BaseArchiveService::new(
        registry.clone(),
        generations,
        source_backends,
        policy.portable_data_compatibility(),
        p5_archive_limits(),
        root.join("recovery-fixture-spool"),
        None,
        Arc::new(Mutex::new(())),
    )
    .map_err(|_| fixture_archive_error("archive-service"))?
    .with_restore_backend_factory(factory);
    let reservation = registry
        .reserve_operation()
        .map_err(|_| fixture_archive_error("reserve-operation"))?;
    let sink = registry
        .begin_sink(reservation, MAX_P5_RECOVERY_ARCHIVE_BYTES)
        .map_err(|_| fixture_archive_error("begin-sink"))?;
    let secret = registry
        .register_secret(
            reservation,
            ArchiveCredentialKind::RecoveryKey,
            Zeroizing::new(key.to_vec()),
        )
        .map_err(|_| fixture_archive_error("register-secret"))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| fixture_archive_error("runtime"))?;
    let created = runtime
        .block_on(service.create_archive(
            sink,
            secret,
            onebrain_archive::ProducerArtifactIdentityV1::Unknown,
        ))
        .map_err(|_error| {
            #[cfg(test)]
            eprintln!("P5 recovery fixture create failed: {_error:?}");
            P5RecoveryErrorV2::Archive
        })?;
    drop(service);
    let archive_temp = fixture_root.join(".base.obar.tmp");
    let key_temp = fixture_root.join(".base.key.tmp");
    if archive_temp.exists() || key_temp.exists() {
        return Err(P5RecoveryErrorV2::Archive);
    }
    let result = (|| {
        let mut archive = create_new_private_file(&archive_temp, 0o400)?;
        let mut offset = 0u64;
        loop {
            let chunk = registry
                .read_sink_chunk(
                    &created.readable_sink,
                    offset,
                    P5_ARCHIVE_CHUNK_BYTES as u32,
                )
                .map_err(|_| P5RecoveryErrorV2::Archive)?;
            archive
                .write_all(&chunk.bytes)
                .map_err(|_| P5RecoveryErrorV2::Archive)?;
            offset = offset
                .checked_add(chunk.bytes.len() as u64)
                .ok_or(P5RecoveryErrorV2::Archive)?;
            if chunk.eof {
                break;
            }
        }
        archive.sync_all().map_err(|_| P5RecoveryErrorV2::Archive)?;
        registry
            .commit_sink(created.readable_sink)
            .map_err(|_| P5RecoveryErrorV2::Archive)?;
        let mut key_file = create_new_private_file(&key_temp, 0o400)?;
        key_file
            .write_all(&key)
            .and_then(|_| key_file.sync_all())
            .map_err(|_| P5RecoveryErrorV2::Archive)?;
        std::fs::rename(&archive_temp, archive_path).map_err(|_| P5RecoveryErrorV2::Archive)?;
        std::fs::rename(&key_temp, key_path).map_err(|_| P5RecoveryErrorV2::Archive)?;
        #[cfg(unix)]
        std::fs::File::open(&fixture_root)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| P5RecoveryErrorV2::Archive)?;
        inspect_fixture_files(archive_path, key_path).map(|(archive_blake3, archive_bytes)| {
            P5RecoveryFixtureReceiptV2 {
                archive_blake3,
                archive_bytes,
                dataset_generation,
            }
        })
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&archive_temp);
        let _ = std::fs::remove_file(&key_temp);
        // Both final paths were proven absent before this publication attempt.
        // Remove only artifacts created by this failed attempt so a later
        // prepare-session can retry instead of being trapped by a partial pair.
        let _ = std::fs::remove_file(archive_path);
        let _ = std::fs::remove_file(key_path);
    }
    result
}

fn fixture_archive_error(stage: &str) -> P5RecoveryErrorV2 {
    #[cfg(test)]
    eprintln!("P5 recovery fixture failed at {stage}");
    let _ = stage;
    P5RecoveryErrorV2::Archive
}

fn inspect_fixture(
    archive_path: &Path,
    key_path: &Path,
    dataset_root: &Path,
) -> Result<P5RecoveryFixtureReceiptV2, P5RecoveryErrorV2> {
    let (archive_blake3, archive_bytes) = inspect_fixture_files(archive_path, key_path)?;
    let generation = DatasetGenerationStore::open_exclusive(dataset_root)
        .map_err(|_| P5RecoveryErrorV2::Archive)?
        .current_resolver()
        .map_err(|_| P5RecoveryErrorV2::Archive)?
        .current_generation()
        .0;
    Ok(P5RecoveryFixtureReceiptV2 {
        archive_blake3,
        archive_bytes,
        dataset_generation: generation,
    })
}

fn inspect_fixture_files(
    archive_path: &Path,
    key_path: &Path,
) -> Result<([u8; 32], u64), P5RecoveryErrorV2> {
    reject_symlink_or_non_file(archive_path)?;
    reject_symlink_or_non_file(key_path)?;
    let archive = std::fs::read(archive_path).map_err(|_| P5RecoveryErrorV2::Archive)?;
    let key = std::fs::read(key_path).map_err(|_| P5RecoveryErrorV2::Archive)?;
    if archive.len() < 8
        || archive.len() as u64 > MAX_P5_RECOVERY_ARCHIVE_BYTES
        || !archive.starts_with(b"OBARV002")
        || key.len() != 32
    {
        return Err(P5RecoveryErrorV2::Archive);
    }
    Ok((*blake3::hash(&archive).as_bytes(), archive.len() as u64))
}

fn create_private_directory(path: &Path) -> Result<(), P5RecoveryErrorV2> {
    if !path.exists() {
        std::fs::create_dir(path).map_err(|_| P5RecoveryErrorV2::Io)?;
    }
    let metadata = std::fs::symlink_metadata(path).map_err(|_| P5RecoveryErrorV2::Io)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(P5RecoveryErrorV2::PathEscapesRoot);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| P5RecoveryErrorV2::Io)?;
    }
    Ok(())
}

fn create_new_private_file(path: &Path, mode: u32) -> Result<std::fs::File, P5RecoveryErrorV2> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(mode);
    }
    #[cfg(not(unix))]
    let _ = mode;
    options.open(path).map_err(|_| P5RecoveryErrorV2::Io)
}

pub fn obarv002_restore(
    input: VerifiedP5RecoveryInputsV2,
) -> Result<P5RecoveryReceiptV2, P5RecoveryErrorV2> {
    if input.operation != P5RecoveryOperationV2::Obarv002Restore {
        return Err(P5RecoveryErrorV2::UnexpectedInput);
    }
    let archive_path = input
        .archive_input
        .as_deref()
        .ok_or(P5RecoveryErrorV2::MissingArchive)?;
    let key_path = input
        .archive_recovery_key
        .as_deref()
        .ok_or(P5RecoveryErrorV2::MissingArchive)?;
    let dataset_root = input
        .base_dataset_root
        .as_deref()
        .ok_or(P5RecoveryErrorV2::MissingArchive)?;
    reject_symlink_or_non_file(archive_path)?;
    reject_symlink_or_non_file(key_path)?;
    if !dataset_root.is_dir()
        || std::fs::symlink_metadata(dataset_root)
            .map_err(|_| P5RecoveryErrorV2::Archive)?
            .file_type()
            .is_symlink()
    {
        return Err(P5RecoveryErrorV2::Archive);
    }
    let archive_length = std::fs::metadata(archive_path)
        .map_err(|_| P5RecoveryErrorV2::Archive)?
        .len();
    if archive_length < 8 || archive_length > MAX_P5_RECOVERY_ARCHIVE_BYTES {
        return Err(P5RecoveryErrorV2::Archive);
    }
    let mut magic = [0u8; 8];
    std::fs::File::open(archive_path)
        .and_then(|mut file| file.read_exact(&mut magic))
        .map_err(|_| P5RecoveryErrorV2::Archive)?;
    if &magic != b"OBARV002" {
        return Err(P5RecoveryErrorV2::Archive);
    }
    let key: [u8; 32] = std::fs::read(key_path)
        .map_err(|_| P5RecoveryErrorV2::Archive)?
        .as_slice()
        .try_into()
        .map_err(|_| P5RecoveryErrorV2::Archive)?;

    let generations = Arc::new(
        DatasetGenerationStore::open_exclusive(dataset_root)
            .map_err(|_| P5RecoveryErrorV2::Archive)?,
    );
    let factory = Arc::new(P5RecoveryArchiveBackendFactory {
        identity_public_key: input.identity_public_key,
    });
    let current = generations
        .current_resolver()
        .map_err(|_| P5RecoveryErrorV2::Archive)?;
    let source_backends = factory
        .open_for_staged_generation(&current)
        .map_err(|_| P5RecoveryErrorV2::Archive)?;
    let compatibility = compiled_archive_policy();
    let capabilities = ArchiveCapabilityRegistry::with_spool_limit(MAX_P5_RECOVERY_ARCHIVE_BYTES)
        .map_err(|_| P5RecoveryErrorV2::Archive)?;
    let spool = input.runner_data_root.join("recovery-spool");
    std::fs::create_dir_all(&spool).map_err(|_| P5RecoveryErrorV2::Archive)?;
    let service = BaseArchiveService::new(
        capabilities.clone(),
        Arc::clone(&generations),
        source_backends,
        compatibility.portable_data_compatibility(),
        p5_archive_limits(),
        &spool,
        None,
        Arc::new(Mutex::new(())),
    )
    .map_err(|_| P5RecoveryErrorV2::Archive)?
    .with_restore_backend_factory(factory);
    let reservation = capabilities
        .reserve_operation()
        .map_err(|_| P5RecoveryErrorV2::Archive)?;
    let source = capabilities
        .begin_source(reservation, archive_length)
        .map_err(|_| P5RecoveryErrorV2::Archive)?;
    let mut archive = std::fs::File::open(archive_path).map_err(|_| P5RecoveryErrorV2::Archive)?;
    let mut offset = 0u64;
    let mut chunk = vec![0u8; P5_ARCHIVE_CHUNK_BYTES];
    loop {
        let read = archive
            .read(&mut chunk)
            .map_err(|_| P5RecoveryErrorV2::Archive)?;
        if read == 0 {
            break;
        }
        capabilities
            .push_source_chunk(&source, offset, &chunk[..read])
            .map_err(|_| P5RecoveryErrorV2::Archive)?;
        offset = offset
            .checked_add(read as u64)
            .ok_or(P5RecoveryErrorV2::Archive)?;
    }
    if offset != archive_length {
        return Err(P5RecoveryErrorV2::Archive);
    }
    let source = capabilities
        .seal_source(source)
        .map_err(|_| P5RecoveryErrorV2::Archive)?;
    let secret = capabilities
        .register_secret(
            reservation,
            ArchiveCredentialKind::RecoveryKey,
            Zeroizing::new(key.to_vec()),
        )
        .map_err(|_| P5RecoveryErrorV2::Archive)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| P5RecoveryErrorV2::Archive)?;
    let restored = runtime
        .block_on(service.restore_archive(source, secret, &compatibility))
        .map_err(|_error| {
            #[cfg(test)]
            eprintln!("P5 OBARV002 restore failed: {_error}");
            P5RecoveryErrorV2::Archive
        })?;
    let mut state = Vec::with_capacity(32 * 2 + 8);
    state.extend_from_slice(&restored.activation.old_generation_root);
    state.extend_from_slice(&restored.activation.new_generation_root);
    state.extend_from_slice(&restored.activation.generation_sequence.to_be_bytes());
    emit_receipt_with_state(input, b"obarv002-restore", &state)
}

fn reject_symlink_or_non_file(path: &Path) -> Result<(), P5RecoveryErrorV2> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| P5RecoveryErrorV2::Archive)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(P5RecoveryErrorV2::Archive);
    }
    Ok(())
}

fn compiled_archive_policy() -> ArchiveRestorePolicyV1 {
    let policy = compiled_base_runtime_config()
        .compatibility_policy
        .archive_restore;
    ArchiveRestorePolicyV1 {
        canonical_schema_digest: policy.canonical_schema_digest.0,
        domain_registry_digest: policy.domain_registry_digest.0,
        resource_registry_digest: policy.resource_registry_digest.0,
        storage_schema_version: policy.storage_schema.0,
        archive_profile: PortableProfileVersion {
            major: policy.archive_profile.major,
            minor: policy.archive_profile.minor,
        },
        migration_profile: PortableProfileVersion {
            major: policy.migration_profile.major,
            minor: policy.migration_profile.minor,
        },
        max_dataset_bytes: MAX_P5_RECOVERY_ARCHIVE_BYTES,
    }
}

fn p5_archive_limits() -> ArchiveLimits {
    ArchiveLimits {
        max_entries: 65_536,
        max_manifest_bytes: 1024 * 1024,
        max_entry_bytes: 256 * 1024 * 1024,
        max_total_plaintext_bytes: MAX_P5_RECOVERY_ARCHIVE_BYTES,
        max_spool_bytes: MAX_P5_RECOVERY_ARCHIVE_BYTES,
    }
}
pub fn rollback(
    input: VerifiedP5RecoveryInputsV2,
) -> Result<P5RecoveryReceiptV2, P5RecoveryErrorV2> {
    if input.operation != P5RecoveryOperationV2::Rollback {
        return Err(P5RecoveryErrorV2::UnexpectedInput);
    }
    let previous = input
        .previous_generation
        .as_deref()
        .ok_or(P5RecoveryErrorV2::MissingPreviousGeneration)?;
    verify_signed_generation(previous)?;
    let rollout = open_rollout(&input.runner_data_root)?;
    let before = rollout.snapshot().map_err(|_| P5RecoveryErrorV2::Rollout)?;
    let after = rollout.rollback().map_err(|_| P5RecoveryErrorV2::Rollout)?;
    if after.lanes.iter().any(|lane| lane.enabled)
        || after
            .lanes
            .iter()
            .zip(before.lanes.iter())
            .any(|(next, prior)| next.generation < prior.generation)
    {
        return Err(P5RecoveryErrorV2::Rollout);
    }
    activate_generation(&input.activation_root, previous, &input.operation_id)?;
    emit_receipt_with_state(input, b"rollback", &rollout_state_bytes(&after))
}
pub fn explicit_re_enable(
    input: VerifiedP5RecoveryInputsV2,
) -> Result<P5RecoveryReceiptV2, P5RecoveryErrorV2> {
    if input.operation != P5RecoveryOperationV2::ExplicitReEnable {
        return Err(P5RecoveryErrorV2::UnexpectedInput);
    }
    verify_current_generation(&input.activation_root)?;
    let rollout = open_rollout(&input.runner_data_root)?;
    let before = rollout.snapshot().map_err(|_| P5RecoveryErrorV2::Rollout)?;
    for lane in VNextRuntimeLane::ALL {
        rollout
            .reenable(lane)
            .map_err(|_| P5RecoveryErrorV2::Rollout)?;
    }
    let after = rollout.snapshot().map_err(|_| P5RecoveryErrorV2::Rollout)?;
    if after.lanes.iter().any(|lane| !lane.enabled)
        || after
            .lanes
            .iter()
            .zip(before.lanes.iter())
            .any(|(next, prior)| !prior.enabled && next.generation <= prior.generation)
    {
        return Err(P5RecoveryErrorV2::Rollout);
    }
    emit_receipt_with_state(input, b"explicit-re-enable", &rollout_state_bytes(&after))
}

fn open_rollout(root: &Path) -> Result<VNextRuntimeRollout, P5RecoveryErrorV2> {
    VNextRuntimeRollout::open(
        &root.join("rollout"),
        VNextRuntimeLaneRequest::all_enabled(),
        VNextRuntimeLaneRequest {
            network: false,
            distributed_kql: false,
            public_use_evidence_publish: false,
            distributed_pomv_view: false,
        },
    )
    .map_err(|_| P5RecoveryErrorV2::Rollout)
}

fn rollout_state_bytes(
    snapshot: &crate::vnext_runtime_rollout::VNextRuntimeRolloutSnapshot,
) -> Vec<u8> {
    let mut bytes = b"onebrain/p5/runtime-rollout-observation/v2\0".to_vec();
    for lane in snapshot.lanes {
        bytes.push(lane.lane as u8);
        bytes.extend_from_slice(&lane.generation.to_be_bytes());
        bytes.push(u8::from(lane.requested));
        bytes.push(u8::from(lane.enabled));
    }
    bytes
}

fn verify_signed_generation(path: &Path) -> Result<(), P5RecoveryErrorV2> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(P5RecoveryErrorV2::Activation)?;
    if name.len() != 64
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || !path.join("scripts/verify.sh").is_file()
        || !path.join("metadata/bundle.manifest.json").is_file()
    {
        return Err(P5RecoveryErrorV2::Activation);
    }
    Ok(())
}

pub fn verify_previous_generation(
    activation_root: &Path,
    previous_generation: &Path,
) -> Result<PathBuf, P5RecoveryErrorV2> {
    let root = activation_root
        .canonicalize()
        .map_err(|_| P5RecoveryErrorV2::ActivationRootMissing)?;
    let previous = previous_generation
        .canonicalize()
        .map_err(|_| P5RecoveryErrorV2::Activation)?;
    if !previous.starts_with(&root) || previous == verify_current_generation(&root)? {
        return Err(P5RecoveryErrorV2::Activation);
    }
    verify_signed_generation(&previous)?;
    Ok(previous)
}

fn verify_current_generation(activation_root: &Path) -> Result<PathBuf, P5RecoveryErrorV2> {
    let current = activation_root.join("current");
    let target = std::fs::read_link(&current).map_err(|_| P5RecoveryErrorV2::Activation)?;
    let target = if target.is_absolute() {
        target
    } else {
        activation_root.join(target)
    };
    let target = target
        .canonicalize()
        .map_err(|_| P5RecoveryErrorV2::Activation)?;
    if !target.starts_with(activation_root) {
        return Err(P5RecoveryErrorV2::Activation);
    }
    verify_signed_generation(&target)?;
    Ok(target)
}

#[cfg(unix)]
fn activate_generation(
    activation_root: &Path,
    generation: &Path,
    operation_id: &[u8; 32],
) -> Result<(), P5RecoveryErrorV2> {
    use std::os::unix::fs::symlink;
    let current = activation_root.join("current");
    let suffix = operation_id
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let next = activation_root.join(format!(".p5-current-{suffix}.next"));
    if next.exists() {
        return Err(P5RecoveryErrorV2::Activation);
    }
    symlink(generation, &next).map_err(|_| P5RecoveryErrorV2::Activation)?;
    if let Err(error) = std::fs::rename(&next, &current) {
        let _ = std::fs::remove_file(&next);
        let _ = error;
        return Err(P5RecoveryErrorV2::Activation);
    }
    std::fs::File::open(activation_root)
        .and_then(|file| file.sync_all())
        .map_err(|_| P5RecoveryErrorV2::Activation)?;
    let selected = verify_current_generation(activation_root)?;
    if selected != generation {
        return Err(P5RecoveryErrorV2::Activation);
    }
    Ok(())
}

#[cfg(not(unix))]
fn activate_generation(
    _activation_root: &Path,
    _generation: &Path,
    _operation_id: &[u8; 32],
) -> Result<(), P5RecoveryErrorV2> {
    Err(P5RecoveryErrorV2::Activation)
}

#[cfg(test)]
fn emit_receipt(
    input: VerifiedP5RecoveryInputsV2,
    label: &[u8],
) -> Result<P5RecoveryReceiptV2, P5RecoveryErrorV2> {
    emit_receipt_with_state(input, label, &[])
}

fn emit_receipt_with_state(
    input: VerifiedP5RecoveryInputsV2,
    label: &[u8],
    state: &[u8],
) -> Result<P5RecoveryReceiptV2, P5RecoveryErrorV2> {
    let mut evidence = Vec::new();
    evidence.extend_from_slice(b"onebrain/p5/recovery-operation/v2\0");
    evidence.extend_from_slice(label);
    evidence.extend_from_slice(&input.request_digest);
    evidence.extend_from_slice(&input.session_id);
    evidence.extend_from_slice(&input.operation_id);
    evidence.extend_from_slice(input.host_id.as_bytes());
    evidence.extend_from_slice(&(state.len() as u64).to_be_bytes());
    evidence.extend_from_slice(state);
    let digest = *blake3::hash(&evidence).as_bytes();
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o400);
    }
    let mut file = options
        .open(&input.evidence_output)
        .map_err(|_| P5RecoveryErrorV2::Io)?;
    file.write_all(&evidence)
        .and_then(|_| file.sync_all())
        .map_err(|_| P5RecoveryErrorV2::Io)?;
    #[cfg(unix)]
    if let Some(parent) = input.evidence_output.parent() {
        std::fs::File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(|_| P5RecoveryErrorV2::Io)?;
    }
    Ok(P5RecoveryReceiptV2 {
        operation: input.operation,
        operation_id: input.operation_id,
        state_changed: true,
        evidence_blake3: digest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DatasetPathResolver;
    use std::fs;

    #[test]
    fn vnext_p5_multi_host_v2_recovery_verifies_every_binding_before_evidence_mutation() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("runner");
        fs::create_dir(&root).unwrap();
        let activation = temp.path().join("activation");
        fs::create_dir(&activation).unwrap();
        let input = P5RecoveryInputsV2 {
            request_digest: [1; 32],
            session_id: [2; 32],
            host_id: "runner-a".into(),
            operation_id: [3; 32],
            identity_public_key: [7; 32],
            runner_data_root: root.clone(),
            activation_root: activation,
            evidence_output: root.join("receipt"),
            archive_input: None,
            archive_recovery_key: None,
            base_dataset_root: None,
            previous_generation: None,
        };
        assert_eq!(
            verify_inputs(P5RecoveryOperationV2::Obarv002Restore, &input),
            Err(P5RecoveryErrorV2::MissingArchive)
        );
        assert!(!input.evidence_output.exists());
        let verified = verify_inputs(P5RecoveryOperationV2::ExplicitReEnable, &input).unwrap();
        assert!(
            emit_receipt(verified, b"verified-input-smoke")
                .unwrap()
                .state_changed
        );
        assert!(input.evidence_output.exists());
    }

    #[test]
    fn vnext_p5_multi_host_v2_recovery_rejects_path_escape() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("runner");
        fs::create_dir(&root).unwrap();
        let activation = temp.path().join("activation");
        fs::create_dir(&activation).unwrap();
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let input = P5RecoveryInputsV2 {
            request_digest: [1; 32],
            session_id: [2; 32],
            host_id: "runner-a".into(),
            operation_id: [3; 32],
            identity_public_key: [7; 32],
            runner_data_root: root,
            activation_root: activation,
            evidence_output: outside.join("receipt"),
            archive_input: None,
            archive_recovery_key: None,
            base_dataset_root: None,
            previous_generation: None,
        };
        assert_eq!(
            verify_inputs(P5RecoveryOperationV2::ExplicitReEnable, &input),
            Err(P5RecoveryErrorV2::PathEscapesRoot)
        );
    }

    #[test]
    fn vnext_p5_multi_host_v2_obarv002_restore_activates_a_verified_generation() {
        let temp = tempfile::tempdir().unwrap();
        let runner = temp.path().join("runner");
        let activation = temp.path().join("activation");
        let fixture_root = runner.join("recovery-input");
        let target_dataset = fixture_root.join("base-dataset");
        fs::create_dir_all(&runner).unwrap();
        fs::create_dir_all(&activation).unwrap();
        let archive_path = fixture_root.join("base.obar");
        let key_path = fixture_root.join("base.key");
        let prepared =
            prepare_obarv002_fixture(&runner, &archive_path, &key_path, &target_dataset, [7; 32])
                .unwrap();
        assert!(prepared.archive_bytes > 8);
        assert_eq!(
            prepared,
            prepare_obarv002_fixture(&runner, &archive_path, &key_path, &target_dataset, [7; 32],)
                .unwrap()
        );

        let input = P5RecoveryInputsV2 {
            request_digest: [1; 32],
            session_id: [2; 32],
            host_id: "runner-a".into(),
            operation_id: [3; 32],
            identity_public_key: [7; 32],
            runner_data_root: runner.clone(),
            activation_root: activation,
            evidence_output: runner.join("restore-receipt"),
            archive_input: Some(archive_path),
            archive_recovery_key: Some(key_path),
            base_dataset_root: Some(target_dataset.clone()),
            previous_generation: None,
        };
        let receipt = obarv002_restore(
            verify_inputs(P5RecoveryOperationV2::Obarv002Restore, &input).unwrap(),
        )
        .unwrap();
        assert!(receipt.state_changed);
        assert!(input.evidence_output.is_file());
        let restored = DatasetGenerationStore::open_exclusive(&target_dataset).unwrap();
        assert_ne!(
            restored.current_resolver().unwrap().current_generation(),
            crate::DatasetGenerationId::BOOTSTRAP
        );
    }

    #[cfg(unix)]
    #[test]
    fn vnext_p5_multi_host_v2_rollback_switches_signed_generation_and_reenable_advances_fence() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let runner = temp.path().join("runner");
        let activation = temp.path().join("activation");
        fs::create_dir(&runner).unwrap();
        fs::create_dir(&activation).unwrap();
        let initial = activation.join("11".repeat(32));
        let previous = activation.join("22".repeat(32));
        for generation in [&initial, &previous] {
            fs::create_dir_all(generation.join("scripts")).unwrap();
            fs::create_dir_all(generation.join("metadata")).unwrap();
            fs::write(generation.join("scripts/verify.sh"), b"#!/bin/sh\n").unwrap();
            fs::write(generation.join("metadata/bundle.manifest.json"), b"{}\n").unwrap();
        }
        symlink(&initial, activation.join("current")).unwrap();

        let rollback_input = P5RecoveryInputsV2 {
            request_digest: [1; 32],
            session_id: [2; 32],
            host_id: "runner-a".into(),
            operation_id: [3; 32],
            identity_public_key: [7; 32],
            runner_data_root: runner.clone(),
            activation_root: activation.clone(),
            evidence_output: runner.join("rollback-receipt"),
            archive_input: None,
            archive_recovery_key: None,
            base_dataset_root: None,
            previous_generation: Some(previous.clone()),
        };
        let receipt =
            rollback(verify_inputs(P5RecoveryOperationV2::Rollback, &rollback_input).unwrap())
                .unwrap();
        assert!(receipt.state_changed);
        assert_eq!(verify_current_generation(&activation).unwrap(), previous);

        let reenable_input = P5RecoveryInputsV2 {
            request_digest: [1; 32],
            session_id: [2; 32],
            host_id: "runner-a".into(),
            operation_id: [4; 32],
            identity_public_key: [7; 32],
            runner_data_root: runner.clone(),
            activation_root: activation,
            evidence_output: runner.join("reenable-receipt"),
            archive_input: None,
            archive_recovery_key: None,
            base_dataset_root: None,
            previous_generation: None,
        };
        explicit_re_enable(
            verify_inputs(P5RecoveryOperationV2::ExplicitReEnable, &reenable_input).unwrap(),
        )
        .unwrap();
        let rollout = open_rollout(&runner).unwrap().snapshot().unwrap();
        assert!(rollout.lanes.iter().all(|lane| lane.enabled));
        assert!(rollout.lanes.iter().all(|lane| lane.generation >= 3));
    }
}
