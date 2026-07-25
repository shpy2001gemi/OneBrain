//! Explicit Concept Registry startup policy and display-safe runtime status.

use std::collections::BTreeMap;
use std::path::PathBuf;

use ku_core::concept_registry::{ConceptLookup, ConceptRegistry, ObrLoadError};
use ku_core::concept_registry_manifest::{
    load_and_validate_manifest, ConceptRegistryManifest, ConceptRegistryManifestError,
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
    let path = config.obr_path();
    if config.concept_registry_mode == ConceptRegistryMode::Disabled {
        return Ok(LoadedConceptRegistry {
            registry: None,
            status: ConceptRegistryStatus {
                mode: config.concept_registry_mode,
                state: ConceptRegistryRuntimeState::Disabled,
                path,
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
                failure_kind: None,
                error: None,
            },
        });
    }

    match load_registry_artifact(&path, config.concept_registry_cache_capacity) {
        Ok((registry, manifest, backend)) => {
            let source_snapshots = manifest
                .sources
                .iter()
                .map(|(name, source)| (name.clone(), source.snapshot_id.clone()))
                .collect();
            let status = ConceptRegistryStatus {
                mode: config.concept_registry_mode,
                state: ConceptRegistryRuntimeState::Loaded,
                path,
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
                path.display()
            )))
        }
        Err(error) => Ok(LoadedConceptRegistry {
            registry: None,
            status: ConceptRegistryStatus {
                mode: config.concept_registry_mode,
                state: ConceptRegistryRuntimeState::FallbackV1,
                path,
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
        }
    }
}

fn load_registry_artifact(
    path: &std::path::Path,
    cache_capacity: usize,
) -> Result<
    (
        Box<dyn ConceptLookup>,
        ConceptRegistryManifest,
        ConceptRegistryBackendKind,
    ),
    RegistryArtifactError,
> {
    let header = ConceptRegistry::inspect_obr(path).map_err(RegistryArtifactError::Obr)?;
    let manifest =
        load_and_validate_manifest(path, header).map_err(RegistryArtifactError::Manifest)?;
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
    use ku_core::concept_registry_manifest::{manifest_path, ConceptRegistrySourceManifest};

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
}
