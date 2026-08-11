//! Signed, immutable Concept Registry release packages and append-only
//! activation state.
//!
//! A release packages the OBR, both bounded indexes, the provenance manifest,
//! an SPDX SBOM, and an Ed25519 verification stamp. Activation never mutates
//! an installed release. Instead it appends a generation record, so an
//! interrupted install or activation leaves the previously active registry
//! readable and rollback only appends another generation.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::concept_registry::{ConceptRegistry, ObrLoadError};
use crate::concept_registry_manifest::{
    load_and_validate_manifest_uncached, ConceptRegistryManifest, ConceptRegistryManifestError,
};

pub const CONCEPT_REGISTRY_RELEASE_PROFILE: &str = "onebrain/concept-registry-release/1";
pub const CONCEPT_REGISTRY_STATE_PROFILE: &str = "onebrain/concept-registry-release-state/1";
pub const RELEASE_STAMP_FILE: &str = "release.stamp.json";
pub const RELEASE_SBOM_FILE: &str = "sbom.spdx.json";
pub const RELEASE_OBR_FILE: &str = "concepts.obr";

const MAX_RELEASE_STAMP_BYTES: u64 = 4 * 1024 * 1024;
const MAX_STATE_BYTES: u64 = 64 * 1024;
const MAX_SBOM_BYTES: u64 = 64 * 1024 * 1024;
const RELEASE_STAGING_SAFETY_MARGIN_BYTES: u64 = 64 * 1024 * 1024;
const SIGNATURE_DOMAIN: &[u8] = b"onebrain:concept-registry-release-stamp:1\0";
const ARTIFACT_ROOT_DOMAIN: &[u8] = b"onebrain:concept-registry-artifacts:1\0";
const SOURCE_ROOT_DOMAIN: &[u8] = b"onebrain:concept-registry-sources:1\0";
const STATE_ROOT_DOMAIN: &[u8] = b"onebrain:concept-registry-state:1\0";
const REQUIRED_SOURCES: [&str; 5] = ["chebi", "geonames", "ncbi", "wikidata", "wordnet"];
const REQUIRED_ARTIFACTS: [(&str, &str); 5] = [
    ("OBR", "concepts.obr"),
    ("LABEL_INDEX", "concepts.obr.labels.idx"),
    ("CCID_INDEX", "concepts.obr.ccids.idx"),
    ("MANIFEST", "concepts.obr.manifest.json"),
    ("SPDX_SBOM", "sbom.spdx.json"),
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConceptRegistryReleaseArtifact {
    pub role: String,
    pub relative_path: String,
    pub length: u64,
    pub blake3: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConceptRegistryReleaseSource {
    pub name: String,
    pub snapshot_id: String,
    pub source_uri: String,
    pub license: String,
    pub snapshot_blake3: String,
    pub download_blake3: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConceptRegistryReleaseStamp {
    pub profile: String,
    pub release_id: String,
    pub builder_version: String,
    pub dedup_policy_version: String,
    pub artifacts: Vec<ConceptRegistryReleaseArtifact>,
    pub artifact_root: String,
    pub sources: Vec<ConceptRegistryReleaseSource>,
    pub source_root: String,
    pub distribution: String,
    pub signer_public_key: String,
    pub signature: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConceptRegistryReleasePackageInput {
    pub release_id: String,
    pub sources: Vec<ConceptRegistryReleaseSource>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConceptRegistryReleaseCapacity {
    pub source_bytes: u64,
    pub metadata_reserve_bytes: u64,
    pub safety_margin_bytes: u64,
    pub required_bytes: u64,
    pub available_bytes: u64,
    pub sufficient: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConceptRegistryReleaseState {
    pub profile: String,
    pub generation: u64,
    pub active_release: String,
    pub previous_release: Option<String>,
    pub state_root: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveConceptRegistryRelease {
    pub generation: u64,
    pub release_id: String,
    pub previous_release: Option<String>,
    pub release_dir: PathBuf,
    pub obr_path: PathBuf,
    pub stamp: ConceptRegistryReleaseStamp,
}

/// Build and install one immutable release without changing the active state.
pub fn package_concept_registry_release(
    obr_path: &Path,
    sbom_path: &Path,
    registry_root: &Path,
    input: ConceptRegistryReleasePackageInput,
    signing_key: &SigningKey,
) -> Result<ConceptRegistryReleaseStamp, ConceptRegistryReleaseError> {
    package_concept_registry_release_with_space_probe(
        obr_path,
        sbom_path,
        registry_root,
        input,
        signing_key,
        |path| fs2::available_space(path),
    )
}

/// Calculate the exact staging admission requirement against the filesystem
/// that will contain the immutable release directory.
pub fn concept_registry_release_capacity(
    obr_path: &Path,
    sbom_path: &Path,
    registry_root: &Path,
) -> Result<ConceptRegistryReleaseCapacity, ConceptRegistryReleaseError> {
    validate_regular_file(sbom_path, "SPDX SBOM")?;
    validate_spdx_sbom(sbom_path)?;
    let source_artifacts = release_source_artifacts(obr_path, sbom_path);
    for (_, source, target_name) in &source_artifacts {
        validate_regular_file(source, target_name)?;
    }
    let releases_dir = registry_root.join("releases");
    fs::create_dir_all(&releases_dir)?;
    release_capacity(&source_artifacts, fs2::available_space(&releases_dir)?)
}

/// Deterministic disk-shortage injection for the isolated qualification
/// executable. Production builds cannot override measured free space.
#[cfg(feature = "concept-registry-failure-harness")]
pub fn package_concept_registry_release_with_capacity_for_drill(
    obr_path: &Path,
    sbom_path: &Path,
    registry_root: &Path,
    input: ConceptRegistryReleasePackageInput,
    signing_key: &SigningKey,
    available_space_bytes: u64,
) -> Result<ConceptRegistryReleaseStamp, ConceptRegistryReleaseError> {
    package_concept_registry_release_with_space_probe(
        obr_path,
        sbom_path,
        registry_root,
        input,
        signing_key,
        |path| Ok(fs2::available_space(path)?.min(available_space_bytes)),
    )
}

fn package_concept_registry_release_with_space_probe<F>(
    obr_path: &Path,
    sbom_path: &Path,
    registry_root: &Path,
    input: ConceptRegistryReleasePackageInput,
    signing_key: &SigningKey,
    space_probe: F,
) -> Result<ConceptRegistryReleaseStamp, ConceptRegistryReleaseError>
where
    F: FnOnce(&Path) -> std::io::Result<u64>,
{
    validate_release_id(&input.release_id)?;
    validate_sources_shape(&input.sources)?;
    validate_regular_file(sbom_path, "SPDX SBOM")?;
    validate_spdx_sbom(sbom_path)?;

    let source_artifacts = release_source_artifacts(obr_path, sbom_path);
    for (_, source, target_name) in &source_artifacts {
        validate_regular_file(source, target_name)?;
    }

    let releases_dir = registry_root.join("releases");
    fs::create_dir_all(&releases_dir)?;
    let final_dir = releases_dir.join(&input.release_id);
    if final_dir.exists() {
        return Err(ConceptRegistryReleaseError::ReleaseExists(input.release_id));
    }
    let capacity = release_capacity(&source_artifacts, space_probe(&releases_dir)?)?;
    if !capacity.sufficient {
        return Err(ConceptRegistryReleaseError::InsufficientSpace {
            required: capacity.required_bytes,
            available: capacity.available_bytes,
        });
    }
    let staging_dir = releases_dir.join(format!(
        ".{}.staging-{}-{}",
        input.release_id,
        std::process::id(),
        unix_nanos()?
    ));
    fs::create_dir(&staging_dir)?;
    let mut staging = StagingDirectoryGuard::new(staging_dir);
    for (_, source, target_name) in &source_artifacts {
        copy_file_sync(source, &staging.path().join(target_name))?;
    }

    let artifacts = source_artifacts
        .iter()
        .map(|(role, _, target_name)| {
            artifact_metadata(role, target_name, &staging.path().join(target_name))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let manifest = validate_packaged_registry(staging.path())?;
    validate_sources_against_manifest(&input.sources, &manifest)?;

    let mut stamp = ConceptRegistryReleaseStamp {
        profile: CONCEPT_REGISTRY_RELEASE_PROFILE.to_owned(),
        release_id: input.release_id.clone(),
        builder_version: manifest.builder_version,
        dedup_policy_version: manifest.dedup_policy_version,
        artifact_root: artifact_root(&artifacts),
        artifacts,
        source_root: source_root(&input.sources),
        sources: input.sources,
        distribution: "MIRROR_OR_OFFLINE_ONLY_NO_OBP_GOSSIP".to_owned(),
        signer_public_key: encode_hex(signing_key.verifying_key().as_bytes()),
        signature: String::new(),
    };
    let message = release_signature_message(&stamp)?;
    stamp.signature = encode_hex(&signing_key.sign(&message).to_bytes());
    write_new_json_sync(&staging.path().join(RELEASE_STAMP_FILE), &stamp)?;

    verify_concept_registry_release_inner(
        staging.path(),
        &input.release_id,
        &signing_key.verifying_key(),
    )?;
    qualification_failpoint("release-publication-before")?;
    fs::rename(staging.path(), &final_dir)?;
    staging.disarm();
    qualification_failpoint("release-publication-during")?;
    sync_directory(&releases_dir)?;
    qualification_failpoint("release-publication-after")?;
    verify_concept_registry_release(&final_dir, &signing_key.verifying_key())
}

/// Verify the trusted signer, signature, exact file set, every artifact hash,
/// OBR manifest/index binding, SPDX shape, and source provenance binding.
pub fn verify_concept_registry_release(
    release_dir: &Path,
    expected_signer: &VerifyingKey,
) -> Result<ConceptRegistryReleaseStamp, ConceptRegistryReleaseError> {
    let directory_release_id = release_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ConceptRegistryReleaseError::InvalidField("release directory".to_owned()))?;
    verify_concept_registry_release_inner(release_dir, directory_release_id, expected_signer)
}

fn verify_concept_registry_release_inner(
    release_dir: &Path,
    expected_release_id: &str,
    expected_signer: &VerifyingKey,
) -> Result<ConceptRegistryReleaseStamp, ConceptRegistryReleaseError> {
    let stamp_path = release_dir.join(RELEASE_STAMP_FILE);
    validate_regular_file(&stamp_path, RELEASE_STAMP_FILE)?;
    let stamp: ConceptRegistryReleaseStamp =
        read_bounded_json(&stamp_path, MAX_RELEASE_STAMP_BYTES)?;
    validate_release_id(&stamp.release_id)?;
    if stamp.release_id != expected_release_id {
        return Err(ConceptRegistryReleaseError::ReleaseDirectoryMismatch {
            expected: expected_release_id.to_owned(),
            actual: stamp.release_id,
        });
    }
    if stamp.profile != CONCEPT_REGISTRY_RELEASE_PROFILE
        || stamp.distribution != "MIRROR_OR_OFFLINE_ONLY_NO_OBP_GOSSIP"
        || stamp.builder_version.trim().is_empty()
        || stamp.dedup_policy_version.trim().is_empty()
    {
        return Err(ConceptRegistryReleaseError::InvalidField(
            "release stamp identity".to_owned(),
        ));
    }
    let signer = decode_fixed_hex::<32>(&stamp.signer_public_key, "signer_public_key")?;
    if signer != expected_signer.to_bytes() {
        return Err(ConceptRegistryReleaseError::UntrustedSigner);
    }
    validate_artifact_shape(&stamp.artifacts)?;
    validate_sources_shape(&stamp.sources)?;
    if stamp.artifact_root != artifact_root(&stamp.artifacts)
        || stamp.source_root != source_root(&stamp.sources)
    {
        return Err(ConceptRegistryReleaseError::RootMismatch);
    }
    let signature = decode_fixed_hex::<64>(&stamp.signature, "signature")?;
    expected_signer
        .verify_strict(
            &release_signature_message(&stamp)?,
            &Signature::from_bytes(&signature),
        )
        .map_err(|_| ConceptRegistryReleaseError::InvalidSignature)?;

    let expected_files: BTreeSet<_> = stamp
        .artifacts
        .iter()
        .map(|artifact| artifact.relative_path.as_str())
        .chain(std::iter::once(RELEASE_STAMP_FILE))
        .collect();
    let actual_files = collect_release_files(release_dir)?;
    if actual_files != expected_files {
        return Err(ConceptRegistryReleaseError::UnexpectedFileSet);
    }
    for artifact in &stamp.artifacts {
        let relative = safe_relative_path(&artifact.relative_path)?;
        let path = release_dir.join(relative);
        validate_regular_file(&path, &artifact.relative_path)?;
        let metadata = fs::metadata(&path)?;
        let actual = blake3_file_hex(&path)?;
        if metadata.len() != artifact.length || actual != artifact.blake3 {
            return Err(ConceptRegistryReleaseError::ArtifactMismatch {
                artifact: artifact.relative_path.clone(),
                expected: artifact.blake3.clone(),
                actual,
            });
        }
    }
    validate_spdx_sbom(&release_dir.join(RELEASE_SBOM_FILE))?;
    let manifest = validate_packaged_registry(release_dir)?;
    if manifest.builder_version != stamp.builder_version
        || manifest.dedup_policy_version != stamp.dedup_policy_version
    {
        return Err(ConceptRegistryReleaseError::ManifestBinding);
    }
    validate_sources_against_manifest(&stamp.sources, &manifest)?;
    Ok(stamp)
}

/// Append a new active generation. Existing packages and state generations
/// remain untouched, allowing old/new coexistence and deterministic rollback.
pub fn activate_concept_registry_release(
    registry_root: &Path,
    release_id: &str,
    expected_signer: &VerifyingKey,
) -> Result<ConceptRegistryReleaseState, ConceptRegistryReleaseError> {
    validate_release_id(release_id)?;
    verify_concept_registry_release(
        &registry_root.join("releases").join(release_id),
        expected_signer,
    )?;
    let latest = latest_concept_registry_state(registry_root)?;
    if latest
        .as_ref()
        .is_some_and(|state| state.active_release == release_id)
    {
        return Ok(latest.expect("checked as Some"));
    }
    append_state(
        registry_root,
        release_id,
        latest.as_ref().map(|state| state.active_release.as_str()),
    )
}

pub fn rollback_concept_registry_release(
    registry_root: &Path,
    expected_signer: &VerifyingKey,
) -> Result<ConceptRegistryReleaseState, ConceptRegistryReleaseError> {
    let latest = latest_concept_registry_state(registry_root)?
        .ok_or(ConceptRegistryReleaseError::NoActiveRelease)?;
    let previous = latest
        .previous_release
        .as_deref()
        .ok_or(ConceptRegistryReleaseError::NoPreviousRelease)?;
    verify_concept_registry_release(
        &registry_root.join("releases").join(previous),
        expected_signer,
    )?;
    append_state(registry_root, previous, Some(&latest.active_release))
}

pub fn latest_concept_registry_state(
    registry_root: &Path,
) -> Result<Option<ConceptRegistryReleaseState>, ConceptRegistryReleaseError> {
    let state_dir = registry_root.join("state");
    if !state_dir.exists() {
        return Ok(None);
    }
    let mut candidates = Vec::new();
    for entry in fs::read_dir(state_dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_file() || file_type.is_symlink() {
            continue;
        }
        let name = entry.file_name();
        let Some(generation) = parse_state_generation(&name.to_string_lossy()) else {
            continue;
        };
        candidates.push((generation, entry.path()));
    }
    candidates.sort_by_key(|(generation, _)| *generation);
    for (generation, path) in candidates.into_iter().rev() {
        let Ok(state) = read_bounded_json::<ConceptRegistryReleaseState>(&path, MAX_STATE_BYTES)
        else {
            continue;
        };
        if state.generation == generation && validate_state(&state).is_ok() {
            return Ok(Some(state));
        }
    }
    Ok(None)
}

pub fn resolve_active_concept_registry_release(
    registry_root: &Path,
    expected_signer: &VerifyingKey,
) -> Result<ActiveConceptRegistryRelease, ConceptRegistryReleaseError> {
    let state = latest_concept_registry_state(registry_root)?
        .ok_or(ConceptRegistryReleaseError::NoActiveRelease)?;
    let release_dir = registry_root.join("releases").join(&state.active_release);
    let stamp = verify_concept_registry_release(&release_dir, expected_signer)?;
    Ok(ActiveConceptRegistryRelease {
        generation: state.generation,
        release_id: state.active_release,
        previous_release: state.previous_release,
        obr_path: release_dir.join(RELEASE_OBR_FILE),
        release_dir,
        stamp,
    })
}

/// Parse the pinned Ed25519 public key used to verify release packages.
///
/// The textual form is exactly 64 lowercase hexadecimal characters. Keeping
/// parsing here gives every runtime and operator tool the same strict rule.
pub fn parse_concept_registry_verifying_key(
    value: &str,
) -> Result<VerifyingKey, ConceptRegistryReleaseError> {
    let bytes = decode_fixed_hex::<32>(value.trim(), "concept_registry_release_public_key")?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| {
        ConceptRegistryReleaseError::InvalidField("concept_registry_release_public_key".to_owned())
    })
}

fn append_state(
    registry_root: &Path,
    active_release: &str,
    previous_release: Option<&str>,
) -> Result<ConceptRegistryReleaseState, ConceptRegistryReleaseError> {
    let state_dir = registry_root.join("state");
    fs::create_dir_all(&state_dir)?;
    let generation = highest_state_generation(&state_dir)?.saturating_add(1);
    let mut state = ConceptRegistryReleaseState {
        profile: CONCEPT_REGISTRY_STATE_PROFILE.to_owned(),
        generation,
        active_release: active_release.to_owned(),
        previous_release: previous_release.map(str::to_owned),
        state_root: String::new(),
    };
    validate_release_id(&state.active_release)?;
    if let Some(previous) = &state.previous_release {
        validate_release_id(previous)?;
    }
    state.state_root = state_root(&state)?;
    let path = state_dir.join(format!("state-{generation:020}.json"));
    write_state_json_sync(&path, &state)?;
    sync_directory(&state_dir)?;
    Ok(state)
}

fn highest_state_generation(state_dir: &Path) -> Result<u64, ConceptRegistryReleaseError> {
    let mut highest = 0;
    for entry in fs::read_dir(state_dir)? {
        let entry = entry?;
        if let Some(generation) = parse_state_generation(&entry.file_name().to_string_lossy()) {
            highest = highest.max(generation);
        }
    }
    Ok(highest)
}

fn parse_state_generation(name: &str) -> Option<u64> {
    let digits = name.strip_prefix("state-")?.strip_suffix(".json")?;
    if digits.len() != 20 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn validate_state(state: &ConceptRegistryReleaseState) -> Result<(), ConceptRegistryReleaseError> {
    if state.profile != CONCEPT_REGISTRY_STATE_PROFILE
        || state.generation == 0
        || state.state_root != state_root(state)?
    {
        return Err(ConceptRegistryReleaseError::InvalidState);
    }
    validate_release_id(&state.active_release)?;
    if let Some(previous) = &state.previous_release {
        validate_release_id(previous)?;
    }
    Ok(())
}

fn state_root(state: &ConceptRegistryReleaseState) -> Result<String, ConceptRegistryReleaseError> {
    #[derive(Serialize)]
    struct StateView<'a> {
        profile: &'a str,
        generation: u64,
        active_release: &'a str,
        previous_release: &'a Option<String>,
    }
    let bytes = serde_json::to_vec(&StateView {
        profile: &state.profile,
        generation: state.generation,
        active_release: &state.active_release,
        previous_release: &state.previous_release,
    })?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(STATE_ROOT_DOMAIN);
    hasher.update(&bytes);
    Ok(hasher.finalize().to_hex().to_string())
}

fn validate_packaged_registry(
    release_dir: &Path,
) -> Result<ConceptRegistryManifest, ConceptRegistryReleaseError> {
    let obr_path = release_dir.join(RELEASE_OBR_FILE);
    let header = ConceptRegistry::inspect_obr(&obr_path)?;
    Ok(load_and_validate_manifest_uncached(&obr_path, header)?)
}

fn validate_sources_against_manifest(
    sources: &[ConceptRegistryReleaseSource],
    manifest: &ConceptRegistryManifest,
) -> Result<(), ConceptRegistryReleaseError> {
    let by_name: BTreeMap<_, _> = sources
        .iter()
        .map(|source| (source.name.as_str(), source))
        .collect();
    if by_name.len() != manifest.sources.len() {
        return Err(ConceptRegistryReleaseError::ManifestBinding);
    }
    for (name, manifest_source) in &manifest.sources {
        let Some(source) = by_name.get(name.as_str()) else {
            return Err(ConceptRegistryReleaseError::ManifestBinding);
        };
        if source.snapshot_id != manifest_source.snapshot_id
            || source.source_uri != manifest_source.source_uri
            || source.license != manifest_source.license
        {
            return Err(ConceptRegistryReleaseError::ManifestBinding);
        }
    }
    Ok(())
}

fn validate_artifact_shape(
    artifacts: &[ConceptRegistryReleaseArtifact],
) -> Result<(), ConceptRegistryReleaseError> {
    if artifacts.len() != REQUIRED_ARTIFACTS.len() {
        return Err(ConceptRegistryReleaseError::UnexpectedFileSet);
    }
    let actual: BTreeSet<_> = artifacts
        .iter()
        .map(|artifact| (artifact.role.as_str(), artifact.relative_path.as_str()))
        .collect();
    let expected: BTreeSet<_> = REQUIRED_ARTIFACTS.into_iter().collect();
    if actual != expected {
        return Err(ConceptRegistryReleaseError::UnexpectedFileSet);
    }
    for artifact in artifacts {
        safe_relative_path(&artifact.relative_path)?;
        validate_lower_hex(&artifact.blake3, 64, "artifact.blake3")?;
        if artifact.length == 0 {
            return Err(ConceptRegistryReleaseError::InvalidField(
                "artifact.length".to_owned(),
            ));
        }
    }
    Ok(())
}

fn validate_sources_shape(
    sources: &[ConceptRegistryReleaseSource],
) -> Result<(), ConceptRegistryReleaseError> {
    let names: BTreeSet<_> = sources.iter().map(|source| source.name.as_str()).collect();
    if names != REQUIRED_SOURCES.into_iter().collect() {
        return Err(ConceptRegistryReleaseError::InvalidField(
            "release sources".to_owned(),
        ));
    }
    for source in sources {
        if source.snapshot_id.trim().is_empty()
            || source.source_uri.trim().is_empty()
            || source.license.trim().is_empty()
        {
            return Err(ConceptRegistryReleaseError::InvalidField(
                "source provenance".to_owned(),
            ));
        }
        validate_lower_hex(&source.snapshot_blake3, 64, "source.snapshot_blake3")?;
        validate_lower_hex(&source.download_blake3, 64, "source.download_blake3")?;
    }
    Ok(())
}

fn artifact_metadata(
    role: &str,
    relative_path: &str,
    path: &Path,
) -> Result<ConceptRegistryReleaseArtifact, ConceptRegistryReleaseError> {
    Ok(ConceptRegistryReleaseArtifact {
        role: role.to_owned(),
        relative_path: relative_path.to_owned(),
        length: fs::metadata(path)?.len(),
        blake3: blake3_file_hex(path)?,
    })
}

fn artifact_root(artifacts: &[ConceptRegistryReleaseArtifact]) -> String {
    let mut values = artifacts.to_vec();
    values.sort_by(|left, right| {
        (&left.role, &left.relative_path).cmp(&(&right.role, &right.relative_path))
    });
    let mut hasher = blake3::Hasher::new();
    hasher.update(ARTIFACT_ROOT_DOMAIN);
    for artifact in values {
        update_framed(&mut hasher, artifact.role.as_bytes());
        update_framed(&mut hasher, artifact.relative_path.as_bytes());
        hasher.update(&artifact.length.to_be_bytes());
        update_framed(&mut hasher, artifact.blake3.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn source_root(sources: &[ConceptRegistryReleaseSource]) -> String {
    let mut values = sources.to_vec();
    values.sort_by(|left, right| left.name.cmp(&right.name));
    let mut hasher = blake3::Hasher::new();
    hasher.update(SOURCE_ROOT_DOMAIN);
    for source in values {
        for value in [
            source.name,
            source.snapshot_id,
            source.source_uri,
            source.license,
            source.snapshot_blake3,
            source.download_blake3,
        ] {
            update_framed(&mut hasher, value.as_bytes());
        }
    }
    hasher.finalize().to_hex().to_string()
}

fn release_signature_message(
    stamp: &ConceptRegistryReleaseStamp,
) -> Result<Vec<u8>, ConceptRegistryReleaseError> {
    let mut unsigned = stamp.clone();
    unsigned.signature.clear();
    let bytes = serde_json::to_vec(&unsigned)?;
    let digest = blake3::hash(&bytes);
    let mut message = Vec::with_capacity(SIGNATURE_DOMAIN.len() + 32);
    message.extend_from_slice(SIGNATURE_DOMAIN);
    message.extend_from_slice(digest.as_bytes());
    Ok(message)
}

fn update_framed(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn validate_spdx_sbom(path: &Path) -> Result<(), ConceptRegistryReleaseError> {
    let value: serde_json::Value = read_bounded_json(path, MAX_SBOM_BYTES)?;
    let spdx = value
        .get("spdxVersion")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if !spdx.starts_with("SPDX-")
        || value
            .get("dataLicense")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .is_empty()
    {
        return Err(ConceptRegistryReleaseError::InvalidField(
            "SPDX SBOM".to_owned(),
        ));
    }
    Ok(())
}

fn collect_release_files(
    release_dir: &Path,
) -> Result<BTreeSet<&str>, ConceptRegistryReleaseError> {
    // The returned strings are matched against static names, so convert by
    // lookup instead of retaining directory-entry owned strings.
    let mut files = BTreeSet::new();
    for entry in fs::read_dir(release_dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() || !file_type.is_file() {
            return Err(ConceptRegistryReleaseError::UnsupportedEntry(entry.path()));
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let static_name = REQUIRED_ARTIFACTS
            .iter()
            .map(|(_, path)| *path)
            .chain(std::iter::once(RELEASE_STAMP_FILE))
            .find(|expected| *expected == name)
            .ok_or(ConceptRegistryReleaseError::UnexpectedFileSet)?;
        files.insert(static_name);
    }
    Ok(files)
}

fn validate_regular_file(path: &Path, label: &str) -> Result<(), ConceptRegistryReleaseError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ConceptRegistryReleaseError::UnsupportedEntry(
            PathBuf::from(label),
        ));
    }
    Ok(())
}

fn safe_relative_path(value: &str) -> Result<PathBuf, ConceptRegistryReleaseError> {
    let path = PathBuf::from(value);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ConceptRegistryReleaseError::InvalidField(
            "artifact.relative_path".to_owned(),
        ));
    }
    Ok(path)
}

fn validate_release_id(value: &str) -> Result<(), ConceptRegistryReleaseError> {
    if value.is_empty()
        || value.len() > 96
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(ConceptRegistryReleaseError::InvalidField(
            "release_id".to_owned(),
        ));
    }
    Ok(())
}

fn validate_lower_hex(
    value: &str,
    length: usize,
    field: &str,
) -> Result<(), ConceptRegistryReleaseError> {
    if value.len() != length
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(ConceptRegistryReleaseError::InvalidField(field.to_owned()));
    }
    Ok(())
}

type ReleaseSourceArtifacts = [(&'static str, PathBuf, &'static str); 5];

fn release_source_artifacts(obr_path: &Path, sbom_path: &Path) -> ReleaseSourceArtifacts {
    [
        ("OBR", obr_path.to_path_buf(), RELEASE_OBR_FILE),
        (
            "LABEL_INDEX",
            append_suffix(obr_path, ".labels.idx"),
            "concepts.obr.labels.idx",
        ),
        (
            "CCID_INDEX",
            append_suffix(obr_path, ".ccids.idx"),
            "concepts.obr.ccids.idx",
        ),
        (
            "MANIFEST",
            append_suffix(obr_path, ".manifest.json"),
            "concepts.obr.manifest.json",
        ),
        ("SPDX_SBOM", sbom_path.to_path_buf(), RELEASE_SBOM_FILE),
    ]
}

fn release_capacity(
    source_artifacts: &ReleaseSourceArtifacts,
    available_bytes: u64,
) -> Result<ConceptRegistryReleaseCapacity, ConceptRegistryReleaseError> {
    let source_bytes = source_artifacts
        .iter()
        .try_fold(0u64, |total, (_, path, _)| {
            total.checked_add(fs::metadata(path)?.len()).ok_or_else(|| {
                ConceptRegistryReleaseError::InvalidField("release capacity overflow".to_owned())
            })
        })?;
    let metadata_reserve_bytes = MAX_RELEASE_STAMP_BYTES;
    let required_bytes = source_bytes
        .checked_add(metadata_reserve_bytes)
        .and_then(|value| value.checked_add(RELEASE_STAGING_SAFETY_MARGIN_BYTES))
        .ok_or_else(|| {
            ConceptRegistryReleaseError::InvalidField("release capacity overflow".to_owned())
        })?;
    Ok(ConceptRegistryReleaseCapacity {
        source_bytes,
        metadata_reserve_bytes,
        safety_margin_bytes: RELEASE_STAGING_SAFETY_MARGIN_BYTES,
        required_bytes,
        available_bytes,
        sufficient: available_bytes >= required_bytes,
    })
}

struct StagingDirectoryGuard {
    path: PathBuf,
    armed: bool,
}

impl StagingDirectoryGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for StagingDirectoryGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn copy_file_sync(source: &Path, target: &Path) -> Result<(), ConceptRegistryReleaseError> {
    fs::copy(source, target)?;
    OpenOptions::new().write(true).open(target)?.sync_all()?;
    Ok(())
}

fn write_new_json_sync<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), ConceptRegistryReleaseError> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), ConceptRegistryReleaseError> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), ConceptRegistryReleaseError> {
    // Rust's standard library cannot open directories for fsync on Windows.
    // Every payload and state file is still individually flushed before its
    // unique-name publication, and no existing active file is overwritten.
    Ok(())
}

fn read_bounded_json<T: for<'de> Deserialize<'de>>(
    path: &Path,
    maximum: u64,
) -> Result<T, ConceptRegistryReleaseError> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(maximum.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        return Err(ConceptRegistryReleaseError::TooLarge {
            path: path.to_path_buf(),
            length: bytes.len() as u64,
        });
    }
    Ok(serde_json::from_slice(&bytes)?)
}

fn blake3_file_hex(path: &Path) -> Result<String, ConceptRegistryReleaseError> {
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn unix_nanos() -> Result<u128, ConceptRegistryReleaseError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .map_err(|error| {
            ConceptRegistryReleaseError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error,
            ))
        })
}

fn encode_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(DIGITS[(byte >> 4) as usize] as char);
        value.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    value
}

fn decode_fixed_hex<const N: usize>(
    value: &str,
    field: &str,
) -> Result<[u8; N], ConceptRegistryReleaseError> {
    validate_lower_hex(value, N * 2, field)?;
    let mut bytes = [0; N];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(chunk)
            .map_err(|_| ConceptRegistryReleaseError::InvalidField(field.to_owned()))?;
        bytes[index] = u8::from_str_radix(text, 16)
            .map_err(|_| ConceptRegistryReleaseError::InvalidField(field.to_owned()))?;
    }
    Ok(bytes)
}

#[derive(Debug)]
pub enum ConceptRegistryReleaseError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Obr(ObrLoadError),
    Manifest(ConceptRegistryManifestError),
    InvalidField(String),
    UnsupportedEntry(PathBuf),
    TooLarge {
        path: PathBuf,
        length: u64,
    },
    InsufficientSpace {
        required: u64,
        available: u64,
    },
    ReleaseExists(String),
    ReleaseDirectoryMismatch {
        expected: String,
        actual: String,
    },
    ArtifactMismatch {
        artifact: String,
        expected: String,
        actual: String,
    },
    UnexpectedFileSet,
    RootMismatch,
    ManifestBinding,
    UntrustedSigner,
    InvalidSignature,
    InvalidState,
    NoActiveRelease,
    NoPreviousRelease,
}

impl fmt::Display for ConceptRegistryReleaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "registry release I/O error: {error}"),
            Self::Json(error) => write!(formatter, "registry release JSON error: {error}"),
            Self::Obr(error) => write!(formatter, "registry release OBR error: {error}"),
            Self::Manifest(error) => write!(formatter, "registry release manifest error: {error}"),
            Self::InvalidField(field) => write!(formatter, "invalid registry release field: {field}"),
            Self::UnsupportedEntry(path) => write!(formatter, "unsupported registry release entry: {}", path.display()),
            Self::TooLarge { path, length } => write!(formatter, "registry release metadata is too large: {} ({length} bytes)", path.display()),
            Self::InsufficientSpace { required, available } => write!(formatter, "insufficient registry release staging space: required={required}, available={available}"),
            Self::ReleaseExists(release) => write!(formatter, "registry release already exists: {release}"),
            Self::ReleaseDirectoryMismatch { expected, actual } => write!(formatter, "registry release directory mismatch: expected={expected}, stamp={actual}"),
            Self::ArtifactMismatch { artifact, expected, actual } => write!(formatter, "registry release artifact mismatch for {artifact}: expected={expected}, actual={actual}"),
            Self::UnexpectedFileSet => formatter.write_str("registry release file set is not exact"),
            Self::RootMismatch => formatter.write_str("registry release aggregate root mismatch"),
            Self::ManifestBinding => formatter.write_str("registry release provenance does not match the OBR manifest"),
            Self::UntrustedSigner => formatter.write_str("registry release signer is not trusted"),
            Self::InvalidSignature => formatter.write_str("registry release signature is invalid"),
            Self::InvalidState => formatter.write_str("registry release activation state is corrupt"),
            Self::NoActiveRelease => formatter.write_str("no active registry release exists"),
            Self::NoPreviousRelease => formatter.write_str("no previous registry release exists for rollback"),
        }
    }
}

impl std::error::Error for ConceptRegistryReleaseError {}

impl From<std::io::Error> for ConceptRegistryReleaseError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for ConceptRegistryReleaseError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<ObrLoadError> for ConceptRegistryReleaseError {
    fn from(error: ObrLoadError) -> Self {
        Self::Obr(error)
    }
}

impl From<ConceptRegistryManifestError> for ConceptRegistryReleaseError {
    fn from(error: ConceptRegistryManifestError) -> Self {
        Self::Manifest(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::concept_registry_manifest::{
        ConceptRegistryIndexManifest, ConceptRegistrySourceManifest,
    };

    fn source(name: &str, marker: u8) -> ConceptRegistryReleaseSource {
        ConceptRegistryReleaseSource {
            name: name.to_owned(),
            snapshot_id: format!("{name}-snapshot-v1"),
            source_uri: format!("https://example.test/{name}"),
            license: "test-license".to_owned(),
            snapshot_blake3: encode_hex(&[marker; 32]),
            download_blake3: encode_hex(&[marker.wrapping_add(1); 32]),
        }
    }

    fn sources() -> Vec<ConceptRegistryReleaseSource> {
        REQUIRED_SOURCES
            .iter()
            .enumerate()
            .map(|(index, name)| source(name, index as u8 + 1))
            .collect()
    }

    fn write_fixture(root: &Path, marker: u8) -> (PathBuf, PathBuf) {
        fs::create_dir_all(root).unwrap();
        let obr_path = root.join("source.obr");
        let ccid = blake3::hash(&[marker; 8]).as_bytes()[..16].to_vec();
        let mut obr = Vec::new();
        obr.extend_from_slice(b"OBR1");
        obr.extend_from_slice(&1u32.to_le_bytes());
        obr.extend_from_slice(&1u64.to_le_bytes());
        obr.extend_from_slice(&1u64.to_le_bytes());
        obr.extend_from_slice(&[0; 8]);
        obr.extend_from_slice(&ccid);
        obr.extend_from_slice(&283u32.to_le_bytes());
        obr.push(0);
        obr.push(7);
        obr.extend_from_slice(&5u16.to_le_bytes());
        obr.extend_from_slice(b"water");
        obr.extend_from_slice(&1u16.to_le_bytes());
        obr.extend_from_slice(&5u16.to_le_bytes());
        obr.extend_from_slice(b"water");
        fs::write(&obr_path, &obr).unwrap();

        let label_index = [marker; 64];
        let ccid_index = [marker.wrapping_add(1); 64];
        fs::write(append_suffix(&obr_path, ".labels.idx"), label_index).unwrap();
        fs::write(append_suffix(&obr_path, ".ccids.idx"), ccid_index).unwrap();
        let index = |bytes: &[u8]| ConceptRegistryIndexManifest {
            schema_version: 1,
            record_size: 24,
            record_count: 1,
            blake3: blake3::hash(bytes).to_hex().to_string(),
            file_size: bytes.len() as u64,
        };
        let release_sources = sources();
        let manifest = ConceptRegistryManifest {
            manifest_version: 1,
            obr_schema_version: 1,
            builder_version: "fixture-builder/1".to_owned(),
            dedup_policy_version: "fixture-dedup/1".to_owned(),
            built_at_utc: "2026-08-01T00:00:00Z".to_owned(),
            obr_blake3: blake3::hash(&obr).to_hex().to_string(),
            entry_count: 1,
            label_count: 1,
            sources: release_sources
                .iter()
                .map(|source| {
                    (
                        source.name.clone(),
                        ConceptRegistrySourceManifest {
                            snapshot_id: source.snapshot_id.clone(),
                            source_uri: source.source_uri.clone(),
                            license: source.license.clone(),
                            record_count: 1,
                        },
                    )
                })
                .collect(),
            label_index: Some(index(&label_index)),
            ccid_index: Some(index(&ccid_index)),
        };
        fs::write(
            append_suffix(&obr_path, ".manifest.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let sbom_path = root.join("sbom.spdx.json");
        fs::write(
            &sbom_path,
            br#"{"spdxVersion":"SPDX-2.3","dataLicense":"CC0-1.0","packages":[]}"#,
        )
        .unwrap();
        (obr_path, sbom_path)
    }

    fn package(
        fixture: &Path,
        registry: &Path,
        release_id: &str,
        marker: u8,
        key: &SigningKey,
    ) -> ConceptRegistryReleaseStamp {
        let (obr, sbom) = write_fixture(fixture, marker);
        package_concept_registry_release(
            &obr,
            &sbom,
            registry,
            ConceptRegistryReleasePackageInput {
                release_id: release_id.to_owned(),
                sources: sources(),
            },
            key,
        )
        .unwrap()
    }

    #[test]
    fn signed_package_detects_wrong_key_signature_and_artifact_corruption() {
        let directory = tempfile::tempdir().unwrap();
        let registry = directory.path().join("registry");
        let key = SigningKey::from_bytes(&[7; 32]);
        let stamp = package(
            &directory.path().join("fixture"),
            &registry,
            "registry-2026q3",
            3,
            &key,
        );
        assert_eq!(stamp.artifacts.len(), 5);
        assert!(verify_concept_registry_release(
            &registry.join("releases/registry-2026q3"),
            &key.verifying_key()
        )
        .is_ok());
        let wrong = SigningKey::from_bytes(&[8; 32]);
        assert!(matches!(
            verify_concept_registry_release(
                &registry.join("releases/registry-2026q3"),
                &wrong.verifying_key()
            ),
            Err(ConceptRegistryReleaseError::UntrustedSigner)
        ));
        let artifact = registry.join("releases/registry-2026q3/concepts.obr.labels.idx");
        let mut bytes = fs::read(&artifact).unwrap();
        bytes[0] ^= 1;
        fs::write(&artifact, bytes).unwrap();
        assert!(matches!(
            verify_concept_registry_release(
                &registry.join("releases/registry-2026q3"),
                &key.verifying_key()
            ),
            Err(ConceptRegistryReleaseError::ArtifactMismatch { .. })
        ));
    }

    #[test]
    fn activation_keeps_old_and_new_and_rollback_appends_generation() {
        let directory = tempfile::tempdir().unwrap();
        let registry = directory.path().join("registry");
        let key = SigningKey::from_bytes(&[9; 32]);
        package(
            &directory.path().join("fixture-v1"),
            &registry,
            "registry-v1",
            1,
            &key,
        );
        package(
            &directory.path().join("fixture-v2"),
            &registry,
            "registry-v2",
            2,
            &key,
        );
        let first =
            activate_concept_registry_release(&registry, "registry-v1", &key.verifying_key())
                .unwrap();
        assert_eq!(first.generation, 1);
        assert!(first.previous_release.is_none());
        let second =
            activate_concept_registry_release(&registry, "registry-v2", &key.verifying_key())
                .unwrap();
        assert_eq!(second.generation, 2);
        assert_eq!(second.previous_release.as_deref(), Some("registry-v1"));
        assert!(registry.join("releases/registry-v1").is_dir());
        assert!(registry.join("releases/registry-v2").is_dir());
        let rolled_back =
            rollback_concept_registry_release(&registry, &key.verifying_key()).unwrap();
        assert_eq!(rolled_back.generation, 3);
        assert_eq!(rolled_back.active_release, "registry-v1");
        assert_eq!(rolled_back.previous_release.as_deref(), Some("registry-v2"));
        let active =
            resolve_active_concept_registry_release(&registry, &key.verifying_key()).unwrap();
        assert_eq!(active.release_id, "registry-v1");
        assert_eq!(active.generation, 3);
    }

    #[test]
    fn interrupted_staging_and_corrupt_state_leave_previous_active() {
        let directory = tempfile::tempdir().unwrap();
        let registry = directory.path().join("registry");
        let key = SigningKey::from_bytes(&[10; 32]);
        package(
            &directory.path().join("fixture"),
            &registry,
            "registry-stable",
            1,
            &key,
        );
        activate_concept_registry_release(&registry, "registry-stable", &key.verifying_key())
            .unwrap();
        fs::create_dir_all(registry.join("releases/.registry-next.staging-dead")).unwrap();
        fs::write(
            registry.join("state/state-00000000000000000002.json"),
            b"{truncated",
        )
        .unwrap();
        let active =
            resolve_active_concept_registry_release(&registry, &key.verifying_key()).unwrap();
        assert_eq!(active.release_id, "registry-stable");
        assert_eq!(active.generation, 1);
    }

    #[test]
    fn release_stamp_identity_is_bound_to_the_release_directory() {
        let directory = tempfile::tempdir().unwrap();
        let registry = directory.path().join("registry");
        let key = SigningKey::from_bytes(&[12; 32]);
        package(
            &directory.path().join("fixture"),
            &registry,
            "registry-original",
            1,
            &key,
        );
        fs::rename(
            registry.join("releases/registry-original"),
            registry.join("releases/registry-renamed"),
        )
        .unwrap();
        assert!(matches!(
            verify_concept_registry_release(
                &registry.join("releases/registry-renamed"),
                &key.verifying_key()
            ),
            Err(ConceptRegistryReleaseError::ReleaseDirectoryMismatch { .. })
        ));
    }

    #[test]
    fn missing_stamp_and_unsafe_release_id_fail_explicitly() {
        let directory = tempfile::tempdir().unwrap();
        let registry = directory.path().join("registry");
        let key = SigningKey::from_bytes(&[11; 32]);
        package(
            &directory.path().join("fixture"),
            &registry,
            "registry-safe",
            1,
            &key,
        );
        fs::remove_file(registry.join("releases/registry-safe/release.stamp.json")).unwrap();
        assert!(matches!(
            verify_concept_registry_release(
                &registry.join("releases/registry-safe"),
                &key.verifying_key()
            ),
            Err(ConceptRegistryReleaseError::Io(error))
                if error.kind() == std::io::ErrorKind::NotFound
        ));
        assert!(matches!(
            activate_concept_registry_release(&registry, "../escape", &key.verifying_key()),
            Err(ConceptRegistryReleaseError::InvalidField(_))
        ));
    }

    #[test]
    fn truncated_indexes_fail_before_activation_and_preserve_active_release() {
        let directory = tempfile::tempdir().unwrap();
        let registry = directory.path().join("registry");
        let key = SigningKey::from_bytes(&[13; 32]);
        package(
            &directory.path().join("fixture-stable"),
            &registry,
            "registry-stable",
            1,
            &key,
        );
        package(
            &directory.path().join("fixture-candidate"),
            &registry,
            "registry-candidate",
            2,
            &key,
        );
        activate_concept_registry_release(&registry, "registry-stable", &key.verifying_key())
            .unwrap();

        for name in ["concepts.obr.labels.idx", "concepts.obr.ccids.idx"] {
            let path = registry.join("releases/registry-candidate").join(name);
            let original = fs::read(&path).unwrap();
            fs::write(&path, &original[..original.len() / 2]).unwrap();
            assert!(matches!(
                verify_concept_registry_release(
                    &registry.join("releases/registry-candidate"),
                    &key.verifying_key()
                ),
                Err(ConceptRegistryReleaseError::ArtifactMismatch { .. })
            ));
            assert!(matches!(
                activate_concept_registry_release(
                    &registry,
                    "registry-candidate",
                    &key.verifying_key()
                ),
                Err(ConceptRegistryReleaseError::ArtifactMismatch { .. })
            ));
            let active =
                resolve_active_concept_registry_release(&registry, &key.verifying_key()).unwrap();
            assert_eq!(active.release_id, "registry-stable");
            assert_eq!(active.generation, 1);
            fs::write(path, original).unwrap();
        }
    }

    #[test]
    fn disk_shortage_fails_before_staging_or_state_side_effects() {
        let directory = tempfile::tempdir().unwrap();
        let registry = directory.path().join("registry");
        let key = SigningKey::from_bytes(&[14; 32]);
        let (obr, sbom) = write_fixture(&directory.path().join("fixture"), 1);
        let error = package_concept_registry_release_with_space_probe(
            &obr,
            &sbom,
            &registry,
            ConceptRegistryReleasePackageInput {
                release_id: "registry-no-space".to_owned(),
                sources: sources(),
            },
            &key,
            |_| Ok(0),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ConceptRegistryReleaseError::InsufficientSpace {
                required,
                available: 0
            } if required > RELEASE_STAGING_SAFETY_MARGIN_BYTES
        ));
        let releases = registry.join("releases");
        assert!(releases.is_dir());
        assert_eq!(fs::read_dir(releases).unwrap().count(), 0);
        assert!(latest_concept_registry_state(&registry).unwrap().is_none());
    }

    #[test]
    fn process_kills_around_release_publication_leave_only_complete_releases() {
        for phase in [
            "release-publication-before",
            "release-publication-during",
            "release-publication-after",
        ] {
            run_process_kill_case(phase, false);
        }
    }

    #[test]
    fn process_kills_around_activation_append_reopen_old_or_new_exact_state() {
        for phase in [
            "state-append-before",
            "state-append-during",
            "state-append-after",
        ] {
            run_process_kill_case(phase, true);
        }
    }

    #[test]
    fn concept_registry_release_process_kill_worker() {
        let Ok(root) = std::env::var("ONEBRAIN_REGISTRY_KILL_ROOT") else {
            return;
        };
        let root = PathBuf::from(root);
        let registry = root.join("registry");
        let key = SigningKey::from_bytes(&[88; 32]);
        match std::env::var("ONEBRAIN_REGISTRY_KILL_OPERATION")
            .unwrap()
            .as_str()
        {
            "package" => {
                package(
                    &root.join("candidate-fixture"),
                    &registry,
                    "candidate",
                    2,
                    &key,
                );
            }
            "activate" => {
                activate_concept_registry_release(&registry, "candidate", &key.verifying_key())
                    .unwrap();
            }
            other => panic!("unknown kill operation: {other}"),
        }
    }

    fn run_process_kill_case(phase: &str, activation: bool) {
        use std::process::{Command, Stdio};
        use std::thread;
        use std::time::{Duration, Instant};

        let directory = tempfile::tempdir().unwrap();
        let registry = directory.path().join("registry");
        let key = SigningKey::from_bytes(&[88; 32]);
        package(
            &directory.path().join("stable-fixture"),
            &registry,
            "stable",
            1,
            &key,
        );
        activate_concept_registry_release(&registry, "stable", &key.verifying_key()).unwrap();
        if activation {
            package(
                &directory.path().join("candidate-fixture"),
                &registry,
                "candidate",
                2,
                &key,
            );
        }
        let marker = directory.path().join(format!("{phase}.marker"));
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "concept_registry_release::tests::concept_registry_release_process_kill_worker",
                "--nocapture",
            ])
            .env("ONEBRAIN_REGISTRY_KILL_ROOT", directory.path())
            .env("ONEBRAIN_REGISTRY_KILL_PHASE", phase)
            .env("ONEBRAIN_REGISTRY_KILL_MARKER", &marker)
            .env(
                "ONEBRAIN_REGISTRY_KILL_OPERATION",
                if activation { "activate" } else { "package" },
            )
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !marker.exists() {
            if child.try_wait().unwrap().is_some() {
                panic!("kill worker exited before {phase} marker");
            }
            assert!(
                Instant::now() < deadline,
                "timeout waiting for {phase} marker"
            );
            thread::sleep(Duration::from_millis(10));
        }
        child.kill().unwrap();
        child.wait().unwrap();

        let active =
            resolve_active_concept_registry_release(&registry, &key.verifying_key()).unwrap();
        assert!(matches!(active.release_id.as_str(), "stable" | "candidate"));
        verify_concept_registry_release(&active.release_dir, &key.verifying_key()).unwrap();
        let latest = latest_concept_registry_state(&registry).unwrap().unwrap();
        assert_eq!(latest.active_release, active.release_id);
        assert_eq!(latest.generation, active.generation);
        for entry in fs::read_dir(registry.join("releases")).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            verify_concept_registry_release(&entry.path(), &key.verifying_key()).unwrap();
        }
    }
}

fn write_state_json_sync(
    path: &Path,
    value: &ConceptRegistryReleaseState,
) -> Result<(), ConceptRegistryReleaseError> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    qualification_failpoint("state-append-before")?;
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    let midpoint = bytes.len() / 2;
    file.write_all(&bytes[..midpoint])?;
    file.sync_all()?;
    qualification_failpoint("state-append-during")?;
    file.write_all(&bytes[midpoint..])?;
    file.sync_all()?;
    qualification_failpoint("state-append-after")?;
    Ok(())
}

#[cfg(any(test, feature = "concept-registry-failure-harness"))]
fn qualification_failpoint(phase: &str) -> Result<(), ConceptRegistryReleaseError> {
    if std::env::var("ONEBRAIN_REGISTRY_KILL_PHASE")
        .ok()
        .as_deref()
        != Some(phase)
    {
        return Ok(());
    }
    let marker = std::env::var_os("ONEBRAIN_REGISTRY_KILL_MARKER").ok_or_else(|| {
        ConceptRegistryReleaseError::InvalidField("qualification kill marker is missing".to_owned())
    })?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(PathBuf::from(marker))?;
    file.write_all(phase.as_bytes())?;
    file.sync_all()?;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

#[cfg(not(any(test, feature = "concept-registry-failure-harness")))]
fn qualification_failpoint(_phase: &str) -> Result<(), ConceptRegistryReleaseError> {
    Ok(())
}
