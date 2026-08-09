//! Explicit Concept Registry startup policy and display-safe runtime status.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use ku_core::concept_registry::{ConceptLookup, ConceptRegistry, ObrLoadError};
use ku_core::concept_registry_manifest::{
    load_and_validate_manifest, load_and_validate_manifest_uncached, ConceptRegistryManifest,
    ConceptRegistryManifestError,
};
use ku_core::concept_registry_release::{
    parse_concept_registry_verifying_key, resolve_active_concept_registry_release,
    ConceptRegistryReleaseError,
};
use ku_core::indexed_concept_registry::{IndexedConceptRegistry, IndexedRegistryError};
use serde::{Deserialize, Serialize};

use crate::config::{ConceptRegistryMode, NodeConfig};
use crate::error::NodeError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConceptRegistryRuntimeState {
    Loaded,
    FallbackV1,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConceptRegistryFailureKind {
    Missing,
    Corrupt,
    Truncated,
    Unsupported,
    ResourceLimit,
    Manifest,
    Io,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConceptRegistryBackendKind {
    InMemory,
    IndexedOnDemand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConceptRegistryStatus {
    pub mode: ConceptRegistryMode,
    pub state: ConceptRegistryRuntimeState,
    pub path: PathBuf,
    pub encoder_version: u8,
    pub backend: Option<ConceptRegistryBackendKind>,
    pub cache_capacity: usize,
    pub obr_schema_version: Option<u32>,
    pub manifest_version: Option<u32>,
    pub concept_count: Option<u64>,
    pub label_count: Option<u64>,
    pub checksum_blake3: Option<String>,
    pub built_at_utc: Option<String>,
    pub builder_version: Option<String>,
    pub source_snapshots: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_generation: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_aggregate_root: Option<String>,
    pub failure_kind: Option<ConceptRegistryFailureKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub(crate) struct LoadedConceptRegistry {
    pub registry: Option<Box<dyn ConceptLookup>>,
    pub status: ConceptRegistryStatus,
}

pub(crate) fn initialize_concept_registry(
    config: &NodeConfig,
) -> Result<LoadedConceptRegistry, NodeError> {
    let configured_path = config
        .concept_registry_release_root
        .clone()
        .unwrap_or_else(|| config.obr_path());
    if config.concept_registry_mode == ConceptRegistryMode::Disabled {
        return Ok(LoadedConceptRegistry {
            registry: None,
            status: ConceptRegistryStatus {
                mode: config.concept_registry_mode,
                state: ConceptRegistryRuntimeState::Disabled,
                path: configured_path,
                encoder_version: 1,
                backend: None,
                cache_capacity: config.concept_registry_cache_capacity,
                obr_schema_version: None,
                manifest_version: None,
                concept_count: None,
                label_count: None,
                checksum_blake3: None,
                built_at_utc: None,
                builder_version: None,
                source_snapshots: BTreeMap::new(),
                release_id: None,
                release_generation: None,
                release_aggregate_root: None,
                failure_kind: None,
                error: None,
            },
        });
    }

    let selected = select_registry_artifact(config);
    let loaded = selected.and_then(|selected| {
        load_registry_artifact(
            &selected.path,
            config.concept_registry_cache_capacity,
            selected.release_id.is_none(),
        )
        .map(|(registry, manifest, backend)| (selected, registry, manifest, backend))
    });
    match loaded {
        Ok((selected, registry, manifest, backend)) => {
            let source_snapshots = manifest
                .sources
                .iter()
                .map(|(name, source)| (name.clone(), source.snapshot_id.clone()))
                .collect();
            let status = ConceptRegistryStatus {
                mode: config.concept_registry_mode,
                state: ConceptRegistryRuntimeState::Loaded,
                path: selected.path,
                encoder_version: 2,
                backend: Some(backend),
                cache_capacity: config.concept_registry_cache_capacity,
                obr_schema_version: Some(manifest.obr_schema_version),
                manifest_version: Some(manifest.manifest_version),
                concept_count: Some(manifest.entry_count),
                label_count: Some(manifest.label_count),
                checksum_blake3: Some(manifest.obr_blake3),
                built_at_utc: Some(manifest.built_at_utc),
                builder_version: Some(manifest.builder_version),
                source_snapshots,
                release_id: selected.release_id,
                release_generation: selected.release_generation,
                release_aggregate_root: selected.release_aggregate_root,
                failure_kind: None,
                error: None,
            };
            Ok(LoadedConceptRegistry {
                registry: Some(registry),
                status,
            })
        }
        Err(error) if config.concept_registry_mode == ConceptRegistryMode::Required => {
            Err(NodeError::Config(format!(
                "required Concept Registry failed at {}: {error}",
                configured_path.display()
            )))
        }
        Err(error) => Ok(LoadedConceptRegistry {
            registry: None,
            status: ConceptRegistryStatus {
                mode: config.concept_registry_mode,
                state: ConceptRegistryRuntimeState::FallbackV1,
                path: configured_path,
                encoder_version: 1,
                backend: None,
                cache_capacity: config.concept_registry_cache_capacity,
                obr_schema_version: None,
                manifest_version: None,
                concept_count: None,
                label_count: None,
                checksum_blake3: None,
                built_at_utc: None,
                builder_version: None,
                source_snapshots: BTreeMap::new(),
                release_id: None,
                release_generation: None,
                release_aggregate_root: None,
                failure_kind: Some(error.kind()),
                error: Some(error.to_string()),
            },
        }),
    }
}

#[derive(Debug)]
enum RegistryArtifactError {
    Obr(ObrLoadError),
    Manifest(ConceptRegistryManifestError),
    Index(IndexedRegistryError),
    ScalableIndexRequired(u64),
    Release(ConceptRegistryReleaseError),
}

impl RegistryArtifactError {
    fn kind(&self) -> ConceptRegistryFailureKind {
        match self {
            Self::Obr(ObrLoadError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                ConceptRegistryFailureKind::Missing
            }
            Self::Obr(ObrLoadError::Io(_)) => ConceptRegistryFailureKind::Io,
            Self::Obr(
                ObrLoadError::InvalidMagic
                | ObrLoadError::NoEntriesLoaded
                | ObrLoadError::EntryCountMismatch { .. },
            ) => ConceptRegistryFailureKind::Corrupt,
            Self::Obr(
                ObrLoadError::TruncatedHeader
                | ObrLoadError::TruncatedBody { .. }
                | ObrLoadError::TruncatedEntry(_),
            ) => ConceptRegistryFailureKind::Truncated,
            Self::Obr(ObrLoadError::UnsupportedVersion(_)) => {
                ConceptRegistryFailureKind::Unsupported
            }
            Self::Obr(ObrLoadError::ResourceLimit { .. }) => {
                ConceptRegistryFailureKind::ResourceLimit
            }
            Self::Manifest(_) => ConceptRegistryFailureKind::Manifest,
            Self::Index(IndexedRegistryError::Io(error))
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                ConceptRegistryFailureKind::Missing
            }
            Self::Index(IndexedRegistryError::Io(_) | IndexedRegistryError::Poisoned(_)) => {
                ConceptRegistryFailureKind::Io
            }
            Self::Index(IndexedRegistryError::UnsupportedVersion(_)) => {
                ConceptRegistryFailureKind::Unsupported
            }
            Self::Index(
                IndexedRegistryError::InvalidHeader(_)
                | IndexedRegistryError::ArtifactMismatch(_)
                | IndexedRegistryError::InvalidOffset(_)
                | IndexedRegistryError::InvalidUtf8,
            ) => ConceptRegistryFailureKind::Corrupt,
            Self::Index(IndexedRegistryError::TooManyMatches(_))
            | Self::ScalableIndexRequired(_) => ConceptRegistryFailureKind::ResourceLimit,
            Self::Release(ConceptRegistryReleaseError::Io(error))
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                ConceptRegistryFailureKind::Missing
            }
            Self::Release(ConceptRegistryReleaseError::TooLarge { .. }) => {
                ConceptRegistryFailureKind::ResourceLimit
            }
            Self::Release(_) => ConceptRegistryFailureKind::Corrupt,
        }
    }
}

impl std::fmt::Display for RegistryArtifactError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Obr(error) => error.fmt(formatter),
            Self::Manifest(error) => error.fmt(formatter),
            Self::Index(error) => error.fmt(formatter),
            Self::ScalableIndexRequired(bytes) => write!(
                formatter,
                "registry is {bytes} bytes and requires .labels.idx and .ccids.idx sidecars"
            ),
            Self::Release(error) => error.fmt(formatter),
        }
    }
}

struct SelectedRegistryArtifact {
    path: PathBuf,
    release_id: Option<String>,
    release_generation: Option<u64>,
    release_aggregate_root: Option<String>,
}

fn select_registry_artifact(
    config: &NodeConfig,
) -> Result<SelectedRegistryArtifact, RegistryArtifactError> {
    match (
        &config.concept_registry_release_root,
        &config.concept_registry_release_public_key,
    ) {
        (None, None) => Ok(SelectedRegistryArtifact {
            path: config.obr_path(),
            release_id: None,
            release_generation: None,
            release_aggregate_root: None,
        }),
        (Some(_), None) | (None, Some(_)) => Err(RegistryArtifactError::Release(
            ConceptRegistryReleaseError::InvalidField(
                "release root and public key must be configured together".to_owned(),
            ),
        )),
        (Some(root), Some(public_key)) => {
            if config.concept_registry_path.is_some() {
                return Err(RegistryArtifactError::Release(
                    ConceptRegistryReleaseError::InvalidField(
                        "concept_registry_path conflicts with release root".to_owned(),
                    ),
                ));
            }
            let verifying_key = parse_concept_registry_verifying_key(public_key)
                .map_err(RegistryArtifactError::Release)?;
            let active = resolve_active_concept_registry_release(root, &verifying_key)
                .map_err(RegistryArtifactError::Release)?;
            Ok(SelectedRegistryArtifact {
                path: active.obr_path,
                release_id: Some(active.release_id),
                release_generation: Some(active.generation),
                release_aggregate_root: Some(active.stamp.artifact_root),
            })
        }
    }
}

struct LeasedConceptRegistryGeneration {
    registry: Arc<dyn ConceptLookup>,
    status: ConceptRegistryStatus,
}

/// An immutable snapshot of one fully verified active Registry generation.
/// Refreshing the manager never retargets an existing lease.
#[derive(Clone)]
pub struct ConceptRegistryReaderLease {
    generation: Arc<LeasedConceptRegistryGeneration>,
}

impl ConceptRegistryReaderLease {
    pub fn status(&self) -> &ConceptRegistryStatus {
        &self.generation.status
    }

    pub fn resolve_checked(
        &self,
        label: &str,
    ) -> Result<
        ku_core::concept_registry::ResolveResult,
        ku_core::concept_registry::ConceptLookupError,
    > {
        self.generation.registry.resolve_checked(label)
    }
}

/// Loads a complete signed generation before atomically swapping the Arc held
/// for newly acquired reader leases.
pub struct ConceptRegistryGenerationManager {
    config: NodeConfig,
    current: RwLock<Arc<LeasedConceptRegistryGeneration>>,
}

impl ConceptRegistryGenerationManager {
    pub fn open(config: NodeConfig) -> Result<Self, NodeError> {
        let current = Arc::new(load_leased_generation(&config)?);
        Ok(Self {
            config,
            current: RwLock::new(current),
        })
    }

    pub fn reader_lease(&self) -> ConceptRegistryReaderLease {
        let generation = self
            .current
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        ConceptRegistryReaderLease { generation }
    }

    pub fn refresh(&self) -> Result<ConceptRegistryStatus, NodeError> {
        let next = Arc::new(load_leased_generation(&self.config)?);
        Ok(self.install_loaded_generation(next))
    }

    fn install_loaded_generation(
        &self,
        next: Arc<LeasedConceptRegistryGeneration>,
    ) -> ConceptRegistryStatus {
        let mut current = self
            .current
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let current_generation = current.status.release_generation.unwrap_or(0);
        let next_generation = next.status.release_generation.unwrap_or(0);
        if next_generation > current_generation {
            *current = next;
        }
        current.status.clone()
    }
}

fn load_leased_generation(
    config: &NodeConfig,
) -> Result<LeasedConceptRegistryGeneration, NodeError> {
    let loaded = initialize_concept_registry(config)?;
    let registry = loaded.registry.ok_or_else(|| {
        NodeError::Config(
            "Concept Registry generation manager requires a loaded Registry".to_owned(),
        )
    })?;
    Ok(LeasedConceptRegistryGeneration {
        registry: Arc::from(registry),
        status: loaded.status,
    })
}

fn load_registry_artifact(
    path: &std::path::Path,
    cache_capacity: usize,
    allow_verification_cache: bool,
) -> Result<
    (
        Box<dyn ConceptLookup>,
        ConceptRegistryManifest,
        ConceptRegistryBackendKind,
    ),
    RegistryArtifactError,
> {
    let header = ConceptRegistry::inspect_obr(path).map_err(RegistryArtifactError::Obr)?;
    let manifest = if allow_verification_cache {
        load_and_validate_manifest(path, header)
    } else {
        load_and_validate_manifest_uncached(path, header)
    }
    .map_err(RegistryArtifactError::Manifest)?;
    if IndexedConceptRegistry::indexes_exist(path) {
        let registry = IndexedConceptRegistry::open(path, &manifest, cache_capacity)
            .map_err(RegistryArtifactError::Index)?;
        return Ok((
            Box::new(registry),
            manifest,
            ConceptRegistryBackendKind::IndexedOnDemand,
        ));
    }
    let file_size = std::fs::metadata(path)
        .map_err(|error| RegistryArtifactError::Obr(ObrLoadError::Io(error)))?
        .len();
    if file_size > 256 * 1024 * 1024 {
        return Err(RegistryArtifactError::ScalableIndexRequired(file_size));
    }
    let registry = ConceptRegistry::load_obr(path).map_err(RegistryArtifactError::Obr)?;
    Ok((
        Box::new(registry),
        manifest,
        ConceptRegistryBackendKind::InMemory,
    ))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use ed25519_dalek::SigningKey;
    use ku_core::concept_registry_manifest::{
        manifest_path, ConceptRegistryIndexManifest, ConceptRegistrySourceManifest,
    };
    use ku_core::indexed_concept_registry::{
        CCID_INDEX_MAGIC, LABEL_INDEX_MAGIC, REGISTRY_INDEX_VERSION,
    };
    use ku_core::{
        activate_concept_registry_release, package_concept_registry_release,
        rollback_concept_registry_release, ConceptRegistryReleasePackageInput,
        ConceptRegistryReleaseSource,
    };

    fn config(directory: &std::path::Path, mode: ConceptRegistryMode) -> NodeConfig {
        NodeConfig {
            data_dir: directory.to_path_buf(),
            concept_registry_mode: mode,
            ..NodeConfig::default()
        }
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
        bytes.push(0);
        bytes.push(0);
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
                    name.to_string(),
                    ConceptRegistrySourceManifest {
                        snapshot_id: format!("{name}-test-snapshot"),
                        source_uri: format!("https://example.test/{name}"),
                        license: "test-license".to_string(),
                        record_count: 1,
                    },
                )
            })
            .collect();
        let manifest = ConceptRegistryManifest {
            manifest_version: 1,
            obr_schema_version: 1,
            builder_version: "test-builder".to_string(),
            dedup_policy_version: "test-dedup".to_string(),
            built_at_utc: "2026-07-23T00:00:00Z".to_string(),
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

    fn write_index(path: &std::path::Path, magic: [u8; 4], checksum: [u8; 32], key: [u8; 16]) {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&magic);
        bytes.extend_from_slice(&REGISTRY_INDEX_VERSION.to_le_bytes());
        bytes.extend_from_slice(&1u64.to_le_bytes());
        bytes.extend_from_slice(&checksum);
        bytes.extend_from_slice(&[0u8; 16]);
        bytes.extend_from_slice(&key);
        bytes.extend_from_slice(&32u64.to_le_bytes());
        fs::write(path, bytes).unwrap();
    }

    fn release_source(name: &str, marker: u8) -> ConceptRegistryReleaseSource {
        let hash = |byte| blake3::hash(&[byte; 8]).to_hex().to_string();
        ConceptRegistryReleaseSource {
            name: name.to_owned(),
            snapshot_id: format!("{name}-test-snapshot"),
            source_uri: format!("https://example.test/{name}"),
            license: "test-license".to_owned(),
            snapshot_blake3: hash(marker),
            download_blake3: hash(marker.wrapping_add(1)),
        }
    }

    fn install_signed_release(root: &std::path::Path) -> SigningKey {
        let source_dir = root.join("source");
        let registry_root = root.join("registry");
        fs::create_dir_all(&source_dir).unwrap();
        let obr_path = source_dir.join("registry.obr");
        write_tiny_obr(&obr_path);
        let obr = fs::read(&obr_path).unwrap();
        let checksum = *blake3::hash(&obr).as_bytes();
        let label_key: [u8; 16] = blake3::hash(b"water").as_bytes()[..16].try_into().unwrap();
        let ccid: [u8; 16] = [7; 16];
        let label_path = IndexedConceptRegistry::label_index_path(&obr_path);
        let ccid_path = IndexedConceptRegistry::ccid_index_path(&obr_path);
        write_index(&label_path, LABEL_INDEX_MAGIC, checksum, label_key);
        write_index(&ccid_path, CCID_INDEX_MAGIC, checksum, ccid);

        let mut manifest: ConceptRegistryManifest =
            serde_json::from_slice(&fs::read(manifest_path(&obr_path)).unwrap()).unwrap();
        let index_manifest = |path: &std::path::Path| ConceptRegistryIndexManifest {
            schema_version: 1,
            record_size: 24,
            record_count: 1,
            blake3: blake3::hash(&fs::read(path).unwrap()).to_hex().to_string(),
            file_size: fs::metadata(path).unwrap().len(),
        };
        manifest.label_index = Some(index_manifest(&label_path));
        manifest.ccid_index = Some(index_manifest(&ccid_path));
        fs::write(
            manifest_path(&obr_path),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let sbom_path = source_dir.join("sbom.spdx.json");
        fs::write(
            &sbom_path,
            br#"{"spdxVersion":"SPDX-2.3","dataLicense":"CC0-1.0"}"#,
        )
        .unwrap();
        let sources = ["chebi", "geonames", "ncbi", "wikidata", "wordnet"]
            .into_iter()
            .enumerate()
            .map(|(index, name)| release_source(name, index as u8 + 1))
            .collect();
        let signing_key = SigningKey::from_bytes(&[0x42; 32]);
        package_concept_registry_release(
            &obr_path,
            &sbom_path,
            &registry_root,
            ConceptRegistryReleasePackageInput {
                release_id: "registry-v1".to_owned(),
                sources,
            },
            &signing_key,
        )
        .unwrap();
        activate_concept_registry_release(
            &registry_root,
            "registry-v1",
            &signing_key.verifying_key(),
        )
        .unwrap();
        signing_key
    }

    #[test]
    fn disabled_mode_does_not_touch_the_registry_path() {
        let directory = tempfile::tempdir().unwrap();
        let loaded =
            initialize_concept_registry(&config(directory.path(), ConceptRegistryMode::Disabled))
                .unwrap();
        assert!(loaded.registry.is_none());
        assert_eq!(loaded.status.state, ConceptRegistryRuntimeState::Disabled);
        assert_eq!(loaded.status.encoder_version, 1);
        assert!(loaded.status.error.is_none());
    }

    #[test]
    fn optional_mode_exposes_the_real_load_error_and_falls_back() {
        let directory = tempfile::tempdir().unwrap();
        let loaded =
            initialize_concept_registry(&config(directory.path(), ConceptRegistryMode::Optional))
                .unwrap();
        assert!(loaded.registry.is_none());
        assert_eq!(loaded.status.state, ConceptRegistryRuntimeState::FallbackV1);
        assert!(loaded.status.error.unwrap().contains("OBR I/O error"));
    }

    #[test]
    fn required_mode_fails_before_node_subsystems_are_initialized() {
        let directory = tempfile::tempdir().unwrap();
        let error =
            initialize_concept_registry(&config(directory.path(), ConceptRegistryMode::Required))
                .err()
                .expect("required registry must fail");
        assert!(error
            .to_string()
            .contains("required Concept Registry failed"));
        assert!(error.to_string().contains("concepts.obr"));
    }

    #[test]
    fn optional_mode_distinguishes_corruption_from_missing_file() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("concepts.obr"), [0; 32]).unwrap();
        let loaded =
            initialize_concept_registry(&config(directory.path(), ConceptRegistryMode::Optional))
                .unwrap();
        assert!(loaded.status.error.unwrap().contains("Invalid OBR magic"));
    }

    #[test]
    fn loaded_registry_reports_encoder_and_counts() {
        let directory = tempfile::tempdir().unwrap();
        write_tiny_obr(&directory.path().join("concepts.obr"));
        let loaded =
            initialize_concept_registry(&config(directory.path(), ConceptRegistryMode::Required))
                .unwrap();
        assert!(loaded.registry.is_some());
        assert_eq!(loaded.status.state, ConceptRegistryRuntimeState::Loaded);
        assert_eq!(loaded.status.encoder_version, 2);
        assert_eq!(loaded.status.manifest_version, Some(1));
        assert_eq!(loaded.status.concept_count, Some(1));
        assert_eq!(loaded.status.label_count, Some(1));
        assert!(loaded.status.error.is_none());
    }

    #[test]
    fn valid_obr_without_manifest_is_explicitly_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("concepts.obr");
        write_tiny_obr(&path);
        fs::remove_file(manifest_path(&path)).unwrap();

        let loaded =
            initialize_concept_registry(&config(directory.path(), ConceptRegistryMode::Optional))
                .unwrap();
        assert_eq!(loaded.status.state, ConceptRegistryRuntimeState::FallbackV1);
        assert_eq!(
            loaded.status.failure_kind,
            Some(ConceptRegistryFailureKind::Manifest)
        );
        assert!(loaded.status.error.unwrap().contains("manifest I/O error"));
    }

    #[test]
    fn checksum_mismatch_is_never_accepted() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("concepts.obr");
        write_tiny_obr(&path);
        let mut bytes = fs::read(&path).unwrap();
        *bytes.last_mut().unwrap() ^= 0x01;
        fs::write(&path, bytes).unwrap();

        let loaded =
            initialize_concept_registry(&config(directory.path(), ConceptRegistryMode::Optional))
                .unwrap();
        assert_eq!(
            loaded.status.failure_kind,
            Some(ConceptRegistryFailureKind::Manifest)
        );
        assert!(loaded.status.error.unwrap().contains("checksum mismatch"));
    }

    #[test]
    fn corrupt_sidecar_is_classified_as_corrupt_not_resource_pressure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("concepts.obr");
        write_tiny_obr(&path);
        fs::write(IndexedConceptRegistry::label_index_path(&path), [0; 64]).unwrap();
        fs::write(IndexedConceptRegistry::ccid_index_path(&path), [0; 64]).unwrap();

        let loaded =
            initialize_concept_registry(&config(directory.path(), ConceptRegistryMode::Optional))
                .unwrap();
        assert_eq!(
            loaded.status.failure_kind,
            Some(ConceptRegistryFailureKind::Corrupt)
        );
        assert_eq!(loaded.status.state, ConceptRegistryRuntimeState::FallbackV1);
    }

    #[test]
    fn required_mode_loads_only_the_verified_active_release_without_cache_mutation() {
        let directory = tempfile::tempdir().unwrap();
        let signing_key = install_signed_release(directory.path());
        let mut config = config(directory.path(), ConceptRegistryMode::Required);
        config.concept_registry_release_root = Some(directory.path().join("registry"));
        config.concept_registry_release_public_key = Some(
            signing_key
                .verifying_key()
                .as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        );

        let loaded = initialize_concept_registry(&config).unwrap();
        assert!(loaded.registry.is_some());
        assert_eq!(loaded.status.state, ConceptRegistryRuntimeState::Loaded);
        assert_eq!(loaded.status.release_id.as_deref(), Some("registry-v1"));
        assert_eq!(loaded.status.release_generation, Some(1));
        assert_eq!(
            loaded.status.backend,
            Some(ConceptRegistryBackendKind::IndexedOnDemand)
        );
        assert!(!loaded
            .status
            .path
            .with_extension("obr.verification.json")
            .exists());
    }

    #[test]
    fn required_release_mode_never_falls_back_when_activation_is_missing() {
        let directory = tempfile::tempdir().unwrap();
        let mut config = config(directory.path(), ConceptRegistryMode::Required);
        config.concept_registry_release_root = Some(directory.path().join("registry"));
        config.concept_registry_release_public_key = Some(
            SigningKey::from_bytes(&[0x24; 32])
                .verifying_key()
                .as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        );
        let error = initialize_concept_registry(&config).err().unwrap();
        assert!(error.to_string().contains("no active registry release"));
    }

    #[test]
    fn old_reader_is_pinned_while_new_reader_sees_only_complete_new_generation() {
        let directory = tempfile::tempdir().unwrap();
        let signing_key = install_signed_release(directory.path());
        let registry_root = directory.path().join("registry");
        let mut release_config = config(directory.path(), ConceptRegistryMode::Required);
        release_config.concept_registry_release_root = Some(registry_root.clone());
        release_config.concept_registry_release_public_key = Some(encode_key(&signing_key));
        let manager = ConceptRegistryGenerationManager::open(release_config).unwrap();
        let old_reader = manager.reader_lease();
        assert_eq!(
            old_reader.status().release_id.as_deref(),
            Some("registry-v1")
        );

        install_additional_release(directory.path(), "registry-v2", 0x52, &signing_key);
        activate_concept_registry_release(
            &registry_root,
            "registry-v2",
            &signing_key.verifying_key(),
        )
        .unwrap();
        manager.refresh().unwrap();
        let new_reader = manager.reader_lease();
        assert_eq!(
            old_reader.status().release_id.as_deref(),
            Some("registry-v1")
        );
        assert_eq!(old_reader.status().release_generation, Some(1));
        assert_eq!(
            new_reader.status().release_id.as_deref(),
            Some("registry-v2")
        );
        assert_eq!(new_reader.status().release_generation, Some(2));
        assert_ne!(
            old_reader.status().release_aggregate_root,
            new_reader.status().release_aggregate_root
        );
    }

    #[test]
    fn rollback_with_active_reader_and_reopen_preserve_exact_roots() {
        let directory = tempfile::tempdir().unwrap();
        let signing_key = install_signed_release(directory.path());
        let registry_root = directory.path().join("registry");
        install_additional_release(directory.path(), "registry-v2", 0x62, &signing_key);
        activate_concept_registry_release(
            &registry_root,
            "registry-v2",
            &signing_key.verifying_key(),
        )
        .unwrap();
        let mut release_config = config(directory.path(), ConceptRegistryMode::Required);
        release_config.concept_registry_release_root = Some(registry_root.clone());
        release_config.concept_registry_release_public_key = Some(encode_key(&signing_key));
        let manager = ConceptRegistryGenerationManager::open(release_config.clone()).unwrap();
        let candidate_reader = manager.reader_lease();
        let candidate_root = candidate_reader
            .status()
            .release_aggregate_root
            .clone()
            .unwrap();

        rollback_concept_registry_release(&registry_root, &signing_key.verifying_key()).unwrap();
        manager.refresh().unwrap();
        let rollback_reader = manager.reader_lease();
        assert_eq!(
            candidate_reader.status().release_id.as_deref(),
            Some("registry-v2")
        );
        assert_eq!(
            rollback_reader.status().release_id.as_deref(),
            Some("registry-v1")
        );
        assert_ne!(
            rollback_reader.status().release_aggregate_root.as_deref(),
            Some(candidate_root.as_str())
        );

        drop(manager);
        let reopened = ConceptRegistryGenerationManager::open(release_config).unwrap();
        assert_eq!(
            reopened.reader_lease().status().release_aggregate_root,
            rollback_reader.status().release_aggregate_root
        );
    }

    #[test]
    fn overlapping_refresh_install_cannot_replace_newer_generation_with_stale_load() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let directory = tempfile::tempdir().unwrap();
        let signing_key = install_signed_release(directory.path());
        let registry_root = directory.path().join("registry");
        let mut release_config = config(directory.path(), ConceptRegistryMode::Required);
        release_config.concept_registry_release_root = Some(registry_root.clone());
        release_config.concept_registry_release_public_key = Some(encode_key(&signing_key));
        let stale = Arc::new(load_leased_generation(&release_config).unwrap());

        install_additional_release(directory.path(), "registry-v2", 0x72, &signing_key);
        activate_concept_registry_release(
            &registry_root,
            "registry-v2",
            &signing_key.verifying_key(),
        )
        .unwrap();
        let newest = Arc::new(load_leased_generation(&release_config).unwrap());
        let manager = Arc::new(ConceptRegistryGenerationManager::open(release_config).unwrap());
        let barrier = Arc::new(Barrier::new(3));

        let stale_thread = {
            let manager = Arc::clone(&manager);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                manager.install_loaded_generation(stale)
            })
        };
        let newest_thread = {
            let manager = Arc::clone(&manager);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                manager.install_loaded_generation(newest)
            })
        };
        barrier.wait();
        stale_thread.join().unwrap();
        newest_thread.join().unwrap();

        assert_eq!(
            manager.reader_lease().status().release_id.as_deref(),
            Some("registry-v2")
        );
        assert_eq!(manager.reader_lease().status().release_generation, Some(2));
    }

    fn encode_key(key: &SigningKey) -> String {
        key.verifying_key()
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn install_additional_release(
        root: &std::path::Path,
        release_id: &str,
        marker: u8,
        signing_key: &SigningKey,
    ) {
        let source_dir = root.join(format!("source-{release_id}"));
        fs::create_dir_all(&source_dir).unwrap();
        let obr_path = source_dir.join("registry.obr");
        write_tiny_obr(&obr_path);
        let mut obr = fs::read(&obr_path).unwrap();
        obr[32] = marker;
        fs::write(&obr_path, &obr).unwrap();
        let mut manifest: ConceptRegistryManifest =
            serde_json::from_slice(&fs::read(manifest_path(&obr_path)).unwrap()).unwrap();
        manifest.obr_blake3 = blake3::hash(&obr).to_hex().to_string();
        fs::write(
            manifest_path(&obr_path),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let checksum = *blake3::hash(&obr).as_bytes();
        let label_key: [u8; 16] = blake3::hash(b"water").as_bytes()[..16].try_into().unwrap();
        write_index(
            &IndexedConceptRegistry::label_index_path(&obr_path),
            LABEL_INDEX_MAGIC,
            checksum,
            label_key,
        );
        write_index(
            &IndexedConceptRegistry::ccid_index_path(&obr_path),
            CCID_INDEX_MAGIC,
            checksum,
            [7; 16],
        );
        let mut manifest: ConceptRegistryManifest =
            serde_json::from_slice(&fs::read(manifest_path(&obr_path)).unwrap()).unwrap();
        let index_manifest = |path: &std::path::Path| ConceptRegistryIndexManifest {
            schema_version: 1,
            record_size: 24,
            record_count: 1,
            blake3: blake3::hash(&fs::read(path).unwrap()).to_hex().to_string(),
            file_size: fs::metadata(path).unwrap().len(),
        };
        manifest.label_index = Some(index_manifest(&IndexedConceptRegistry::label_index_path(
            &obr_path,
        )));
        manifest.ccid_index = Some(index_manifest(&IndexedConceptRegistry::ccid_index_path(
            &obr_path,
        )));
        fs::write(
            manifest_path(&obr_path),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let sbom = source_dir.join("sbom.spdx.json");
        fs::write(
            &sbom,
            br#"{"spdxVersion":"SPDX-2.3","dataLicense":"CC0-1.0"}"#,
        )
        .unwrap();
        let sources = ["chebi", "geonames", "ncbi", "wikidata", "wordnet"]
            .into_iter()
            .enumerate()
            .map(|(index, name)| release_source(name, index as u8 + 1))
            .collect();
        package_concept_registry_release(
            &obr_path,
            &sbom,
            &root.join("registry"),
            ConceptRegistryReleasePackageInput {
                release_id: release_id.to_owned(),
                sources,
            },
            signing_key,
        )
        .unwrap();
    }
}
