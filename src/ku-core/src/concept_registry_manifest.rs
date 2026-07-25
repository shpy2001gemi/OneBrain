//! Versioned provenance manifest for compiled Concept Registry artifacts.

use std::collections::BTreeMap;
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const CONCEPT_REGISTRY_MANIFEST_VERSION: u32 = 1;
pub const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const REQUIRED_SOURCES: [&str; 5] = ["wikidata", "wordnet", "geonames", "ncbi", "chebi"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConceptRegistrySourceManifest {
    pub snapshot_id: String,
    pub source_uri: String,
    pub license: String,
    pub record_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConceptRegistryIndexManifest {
    pub schema_version: u32,
    pub record_size: u32,
    pub record_count: u64,
    pub blake3: String,
    pub file_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConceptRegistryManifest {
    pub manifest_version: u32,
    pub obr_schema_version: u32,
    pub builder_version: String,
    pub dedup_policy_version: String,
    pub built_at_utc: String,
    pub obr_blake3: String,
    pub entry_count: u64,
    pub label_count: u64,
    pub sources: BTreeMap<String, ConceptRegistrySourceManifest>,
    #[serde(default)]
    pub label_index: Option<ConceptRegistryIndexManifest>,
    #[serde(default)]
    pub ccid_index: Option<ConceptRegistryIndexManifest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ConceptRegistryVerificationStamp {
    obr_blake3: String,
    file_size: u64,
    modified_ns: u128,
    #[serde(default)]
    label_index: Option<ArtifactVerificationStamp>,
    #[serde(default)]
    ccid_index: Option<ArtifactVerificationStamp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ArtifactVerificationStamp {
    blake3: String,
    file_size: u64,
    modified_ns: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObrHeaderMetadata {
    pub schema_version: u32,
    pub entry_count: u64,
    pub label_count: u64,
}

#[derive(Debug)]
pub enum ConceptRegistryManifestError {
    Io(std::io::Error),
    TooLarge(u64),
    InvalidJson(serde_json::Error),
    UnsupportedManifestVersion(u32),
    UnsupportedObrVersion(u32),
    InvalidField(&'static str),
    MissingSource(String),
    CountMismatch {
        field: &'static str,
        manifest: u64,
        obr: u64,
    },
    ChecksumMismatch {
        expected: String,
        actual: String,
    },
    ArtifactChecksumMismatch {
        artifact: &'static str,
        expected: String,
        actual: String,
    },
}

impl fmt::Display for ConceptRegistryManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "manifest I/O error: {error}"),
            Self::TooLarge(bytes) => write!(
                formatter,
                "manifest exceeds the {} byte limit: {bytes} bytes",
                MAX_MANIFEST_BYTES
            ),
            Self::InvalidJson(error) => write!(formatter, "invalid manifest JSON: {error}"),
            Self::UnsupportedManifestVersion(version) => {
                write!(formatter, "unsupported manifest version: {version}")
            }
            Self::UnsupportedObrVersion(version) => {
                write!(
                    formatter,
                    "unsupported OBR schema version in manifest: {version}"
                )
            }
            Self::InvalidField(field) => write!(formatter, "invalid manifest field: {field}"),
            Self::MissingSource(source) => {
                write!(formatter, "manifest is missing required source: {source}")
            }
            Self::CountMismatch {
                field,
                manifest,
                obr,
            } => write!(
                formatter,
                "manifest {field} mismatch: manifest={manifest}, obr={obr}"
            ),
            Self::ChecksumMismatch { expected, actual } => write!(
                formatter,
                "registry checksum mismatch: expected={expected}, actual={actual}"
            ),
            Self::ArtifactChecksumMismatch {
                artifact,
                expected,
                actual,
            } => write!(
                formatter,
                "{artifact} checksum mismatch: expected={expected}, actual={actual}"
            ),
        }
    }
}

impl std::error::Error for ConceptRegistryManifestError {}

pub fn manifest_path(obr_path: &Path) -> PathBuf {
    let mut path = obr_path.as_os_str().to_os_string();
    path.push(".manifest.json");
    PathBuf::from(path)
}

pub fn verification_stamp_path(obr_path: &Path) -> PathBuf {
    let mut path = obr_path.as_os_str().to_os_string();
    path.push(".verification.json");
    PathBuf::from(path)
}

pub fn load_and_validate_manifest(
    obr_path: &Path,
    header: ObrHeaderMetadata,
) -> Result<ConceptRegistryManifest, ConceptRegistryManifestError> {
    let path = manifest_path(obr_path);
    let metadata = std::fs::metadata(&path).map_err(ConceptRegistryManifestError::Io)?;
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err(ConceptRegistryManifestError::TooLarge(metadata.len()));
    }
    let bytes = std::fs::read(path).map_err(ConceptRegistryManifestError::Io)?;
    let manifest: ConceptRegistryManifest =
        serde_json::from_slice(&bytes).map_err(ConceptRegistryManifestError::InvalidJson)?;

    validate_manifest_shape(&manifest, header)?;
    let obr_metadata = std::fs::metadata(obr_path).map_err(ConceptRegistryManifestError::Io)?;
    let modified_ns = modified_ns(&obr_metadata)?;
    if verification_stamp_matches(obr_path, &manifest, obr_metadata.len(), modified_ns) {
        return Ok(manifest);
    }

    let actual = blake3_file_hex(obr_path)?;
    if manifest.obr_blake3 != actual {
        return Err(ConceptRegistryManifestError::ChecksumMismatch {
            expected: manifest.obr_blake3.clone(),
            actual,
        });
    }
    validate_sidecar_checksum(
        obr_path,
        ".labels.idx",
        "label index",
        &manifest.label_index,
    )?;
    validate_sidecar_checksum(obr_path, ".ccids.idx", "CCID index", &manifest.ccid_index)?;
    let _ = write_verification_stamp(obr_path, &manifest, obr_metadata.len(), modified_ns);
    Ok(manifest)
}

fn verification_stamp_matches(
    obr_path: &Path,
    manifest: &ConceptRegistryManifest,
    file_size: u64,
    modified_ns: u128,
) -> bool {
    let path = verification_stamp_path(obr_path);
    let Ok(metadata) = std::fs::metadata(&path) else {
        return false;
    };
    if metadata.len() > MAX_MANIFEST_BYTES {
        return false;
    }
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let Ok(stamp) = serde_json::from_slice::<ConceptRegistryVerificationStamp>(&bytes) else {
        return false;
    };
    stamp.obr_blake3 == manifest.obr_blake3
        && stamp.file_size == file_size
        && stamp.modified_ns == modified_ns
        && artifact_stamp_matches(
            obr_path,
            ".labels.idx",
            &manifest.label_index,
            &stamp.label_index,
        )
        && artifact_stamp_matches(
            obr_path,
            ".ccids.idx",
            &manifest.ccid_index,
            &stamp.ccid_index,
        )
}

fn write_verification_stamp(
    obr_path: &Path,
    manifest: &ConceptRegistryManifest,
    file_size: u64,
    modified_ns: u128,
) -> Result<(), ConceptRegistryManifestError> {
    let stamp = ConceptRegistryVerificationStamp {
        obr_blake3: manifest.obr_blake3.clone(),
        file_size,
        modified_ns,
        label_index: artifact_stamp(obr_path, ".labels.idx", &manifest.label_index)?,
        ccid_index: artifact_stamp(obr_path, ".ccids.idx", &manifest.ccid_index)?,
    };
    let bytes =
        serde_json::to_vec_pretty(&stamp).map_err(ConceptRegistryManifestError::InvalidJson)?;
    std::fs::write(verification_stamp_path(obr_path), bytes)
        .map_err(ConceptRegistryManifestError::Io)
}

fn artifact_stamp_matches(
    obr_path: &Path,
    suffix: &str,
    expected: &Option<ConceptRegistryIndexManifest>,
    stamp: &Option<ArtifactVerificationStamp>,
) -> bool {
    match (expected, stamp) {
        (None, None) => true,
        (Some(expected), Some(stamp)) => {
            let path = append_suffix(obr_path, suffix);
            let Ok(metadata) = std::fs::metadata(path) else {
                return false;
            };
            let Ok(modified_ns) = modified_ns(&metadata) else {
                return false;
            };
            stamp.blake3 == expected.blake3
                && stamp.file_size == expected.file_size
                && stamp.file_size == metadata.len()
                && stamp.modified_ns == modified_ns
        }
        _ => false,
    }
}

fn artifact_stamp(
    obr_path: &Path,
    suffix: &str,
    expected: &Option<ConceptRegistryIndexManifest>,
) -> Result<Option<ArtifactVerificationStamp>, ConceptRegistryManifestError> {
    let Some(expected) = expected else {
        return Ok(None);
    };
    let metadata = std::fs::metadata(append_suffix(obr_path, suffix))
        .map_err(ConceptRegistryManifestError::Io)?;
    Ok(Some(ArtifactVerificationStamp {
        blake3: expected.blake3.clone(),
        file_size: metadata.len(),
        modified_ns: modified_ns(&metadata)?,
    }))
}

fn validate_sidecar_checksum(
    obr_path: &Path,
    suffix: &str,
    artifact: &'static str,
    expected: &Option<ConceptRegistryIndexManifest>,
) -> Result<(), ConceptRegistryManifestError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let actual = blake3_file_hex(&append_suffix(obr_path, suffix))?;
    if actual != expected.blake3 {
        return Err(ConceptRegistryManifestError::ArtifactChecksumMismatch {
            artifact,
            expected: expected.blake3.clone(),
            actual,
        });
    }
    Ok(())
}

fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn modified_ns(metadata: &std::fs::Metadata) -> Result<u128, ConceptRegistryManifestError> {
    metadata
        .modified()
        .map_err(ConceptRegistryManifestError::Io)?
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .map_err(|error| {
            ConceptRegistryManifestError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error,
            ))
        })
}

fn validate_manifest_shape(
    manifest: &ConceptRegistryManifest,
    header: ObrHeaderMetadata,
) -> Result<(), ConceptRegistryManifestError> {
    if manifest.manifest_version != CONCEPT_REGISTRY_MANIFEST_VERSION {
        return Err(ConceptRegistryManifestError::UnsupportedManifestVersion(
            manifest.manifest_version,
        ));
    }
    if manifest.obr_schema_version != header.schema_version {
        return Err(ConceptRegistryManifestError::UnsupportedObrVersion(
            manifest.obr_schema_version,
        ));
    }
    for (field, value) in [
        ("builder_version", manifest.builder_version.as_str()),
        (
            "dedup_policy_version",
            manifest.dedup_policy_version.as_str(),
        ),
        ("built_at_utc", manifest.built_at_utc.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(ConceptRegistryManifestError::InvalidField(field));
        }
    }
    if manifest.obr_blake3.len() != 64
        || !manifest
            .obr_blake3
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(ConceptRegistryManifestError::InvalidField("obr_blake3"));
    }
    if manifest.entry_count != header.entry_count {
        return Err(ConceptRegistryManifestError::CountMismatch {
            field: "entry_count",
            manifest: manifest.entry_count,
            obr: header.entry_count,
        });
    }
    if manifest.label_count != header.label_count {
        return Err(ConceptRegistryManifestError::CountMismatch {
            field: "label_count",
            manifest: manifest.label_count,
            obr: header.label_count,
        });
    }
    for source in REQUIRED_SOURCES {
        let metadata = manifest
            .sources
            .get(source)
            .ok_or_else(|| ConceptRegistryManifestError::MissingSource(source.to_string()))?;
        if metadata.snapshot_id.trim().is_empty()
            || metadata.source_uri.trim().is_empty()
            || metadata.license.trim().is_empty()
        {
            return Err(ConceptRegistryManifestError::InvalidField(
                "sources.* metadata",
            ));
        }
    }
    if manifest.label_index.is_some() != manifest.ccid_index.is_some() {
        return Err(ConceptRegistryManifestError::InvalidField(
            "sidecar index pair",
        ));
    }
    for index in [manifest.label_index.as_ref(), manifest.ccid_index.as_ref()]
        .into_iter()
        .flatten()
    {
        if index.schema_version == 0
            || index.record_size == 0
            || index.blake3.len() != 64
            || !index.blake3.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(ConceptRegistryManifestError::InvalidField(
                "sidecar index metadata",
            ));
        }
    }
    Ok(())
}

fn blake3_file_hex(path: &Path) -> Result<String, ConceptRegistryManifestError> {
    let mut file = std::fs::File::open(path).map_err(ConceptRegistryManifestError::Io)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(ConceptRegistryManifestError::Io)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(name: &str) -> ConceptRegistrySourceManifest {
        ConceptRegistrySourceManifest {
            snapshot_id: format!("{name}-snapshot"),
            source_uri: format!("https://example.test/{name}"),
            license: "test-license".to_string(),
            record_count: 1,
        }
    }

    fn manifest(obr: &[u8]) -> ConceptRegistryManifest {
        ConceptRegistryManifest {
            manifest_version: 1,
            obr_schema_version: 1,
            builder_version: "test-builder".to_string(),
            dedup_policy_version: "test-dedup".to_string(),
            built_at_utc: "2026-07-23T00:00:00Z".to_string(),
            obr_blake3: blake3::hash(obr).to_hex().to_string(),
            entry_count: 1,
            label_count: 1,
            sources: REQUIRED_SOURCES
                .into_iter()
                .map(|name| (name.to_string(), source(name)))
                .collect(),
            label_index: None,
            ccid_index: None,
        }
    }

    fn index_manifest(bytes: &[u8]) -> ConceptRegistryIndexManifest {
        ConceptRegistryIndexManifest {
            schema_version: 1,
            record_size: 24,
            record_count: 1,
            blake3: blake3::hash(bytes).to_hex().to_string(),
            file_size: bytes.len() as u64,
        }
    }

    #[test]
    fn validates_checksum_counts_and_required_provenance() {
        let directory = std::env::temp_dir().join(format!(
            "onebrain-manifest-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let obr_path = directory.join("concepts.obr");
        let obr = b"tiny-registry";
        std::fs::write(&obr_path, obr).unwrap();
        std::fs::write(
            manifest_path(&obr_path),
            serde_json::to_vec_pretty(&manifest(obr)).unwrap(),
        )
        .unwrap();

        let loaded = load_and_validate_manifest(
            &obr_path,
            ObrHeaderMetadata {
                schema_version: 1,
                entry_count: 1,
                label_count: 1,
            },
        )
        .unwrap();
        assert_eq!(loaded.sources.len(), 5);
        assert!(verification_stamp_path(&obr_path).is_file());

        std::fs::write(&obr_path, b"tampered").unwrap();
        assert!(matches!(
            load_and_validate_manifest(
                &obr_path,
                ObrHeaderMetadata {
                    schema_version: 1,
                    entry_count: 1,
                    label_count: 1,
                }
            ),
            Err(ConceptRegistryManifestError::ChecksumMismatch { .. })
        ));

        std::fs::write(&obr_path, obr).unwrap();
        let label_index = b"label-index";
        let ccid_index = b"ccid-index";
        std::fs::write(append_suffix(&obr_path, ".labels.idx"), label_index).unwrap();
        std::fs::write(append_suffix(&obr_path, ".ccids.idx"), ccid_index).unwrap();
        let mut with_indexes = manifest(obr);
        with_indexes.label_index = Some(index_manifest(label_index));
        with_indexes.ccid_index = Some(index_manifest(ccid_index));
        std::fs::write(
            manifest_path(&obr_path),
            serde_json::to_vec_pretty(&with_indexes).unwrap(),
        )
        .unwrap();
        let _ = std::fs::remove_file(verification_stamp_path(&obr_path));
        load_and_validate_manifest(
            &obr_path,
            ObrHeaderMetadata {
                schema_version: 1,
                entry_count: 1,
                label_count: 1,
            },
        )
        .unwrap();

        std::fs::write(append_suffix(&obr_path, ".labels.idx"), b"label-Index").unwrap();
        let _ = std::fs::remove_file(verification_stamp_path(&obr_path));
        assert!(matches!(
            load_and_validate_manifest(
                &obr_path,
                ObrHeaderMetadata {
                    schema_version: 1,
                    entry_count: 1,
                    label_count: 1,
                }
            ),
            Err(ConceptRegistryManifestError::ArtifactChecksumMismatch {
                artifact: "label index",
                ..
            })
        ));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
