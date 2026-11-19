//! DEV-ONLY synthetic Concept Registry for a fresh-clone contributor demo (D-056).
//!
//! Builds a tiny signed and activated Registry from a fixed list of synthetic
//! concepts so the local `/ku` host can start without an externally distributed
//! release. The signer is generated from the operating system's randomness, held
//! in memory only and discarded after activation, so nobody (including this tool)
//! can sign another release under the written public key.
//!
//! Limits, by design:
//! * synthetic CCIDs use a dev-only derivation domain and are NOT compatible with
//!   any real Registry release; KUs saved against this Registry are demo data;
//! * the public key is self-issued and is NOT operator Registry trust;
//! * real hosts keep requiring an independently trusted, distributed release.

use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use ed25519_dalek::SigningKey;
use ku_core::concept_registry_manifest::{
    manifest_path, ConceptRegistryIndexManifest, ConceptRegistrySourceManifest,
};
use ku_core::indexed_concept_registry::{
    CCID_INDEX_MAGIC, LABEL_INDEX_MAGIC, REGISTRY_INDEX_VERSION,
};
use ku_core::{
    activate_concept_registry_release, package_concept_registry_release,
    resolve_active_concept_registry_release, ConceptRegistryManifest,
    ConceptRegistryReleasePackageInput, ConceptRegistryReleaseSource, IndexedConceptRegistry,
};
use rand::rngs::OsRng;

/// Marker embedded in builder/source/license fields of every dev-demo release.
const DEV_MARKER: &str = "onebrain-dev-demo-synthetic";
/// Domain separating dev-demo CCIDs from any real Registry derivation.
const DEV_CCID_DOMAIN: &[u8] = b"onebrain:dev-demo-registry-ccid:1\0";
const REQUIRED_SOURCES: [&str; 5] = ["chebi", "geonames", "ncbi", "wikidata", "wordnet"];

/// `(canonical name, OBR category byte, labels)`; labels must be lowercase and
/// unique across entries so every label resolves to exactly one concept.
/// Categories: 0 Entity, 1 Property, 3 Taxon, 4 Place, 7 Substance.
const DEV_CONCEPTS: &[(&str, u8, &[&str])] = &[
    ("water", 7, &["water", "nước"]),
    ("liquid", 1, &["liquid", "chất lỏng"]),
    ("solid", 1, &["solid", "chất rắn"]),
    ("gas", 1, &["gas", "chất khí"]),
    ("ice", 7, &["ice", "nước đá"]),
    ("sun", 0, &["sun", "mặt trời"]),
    ("earth", 4, &["earth", "trái đất"]),
    ("cat", 3, &["cat", "mèo"]),
    ("tree", 3, &["tree", "cây"]),
];

const USAGE: &str = "usage: concept_registry_dev_demo REGISTRY_ROOT PUBLIC_KEY_FILE\n\
    Creates a NEW DEV-ONLY synthetic signed Registry at REGISTRY_ROOT (must not exist)\n\
    and writes its self-issued public key to PUBLIC_KEY_FILE (must not exist).\n\
    Both parents must already exist. Not operator trust; not real CCIDs.";

fn main() {
    if let Err(error) = run(std::env::args_os().skip(1).map(PathBuf::from).collect()) {
        eprintln!("concept-registry-dev-demo: {error}");
        std::process::exit(1);
    }
}

fn run(args: Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    let [registry_root, public_key_file] = <[PathBuf; 2]>::try_from(args).map_err(|_| USAGE)?;
    let report = create_dev_demo_registry(&registry_root, &public_key_file)?;
    println!(
        "ku_dev_demo_registry_ready release_id={}",
        report.release_id
    );
    println!("public_key={}", report.public_key_hex);
    println!("concepts={}", report.labels.join(","));
    println!(
        "DEV-ONLY: synthetic concepts and self-issued signer (private key discarded). \
         Not operator Registry trust; CCIDs are not compatible with real releases."
    );
    Ok(())
}

struct DevDemoReport {
    release_id: String,
    public_key_hex: String,
    labels: Vec<String>,
}

fn create_dev_demo_registry(
    registry_root: &Path,
    public_key_file: &Path,
) -> Result<DevDemoReport, Box<dyn Error>> {
    // Refuse before writing anything: never extend or replace existing custody.
    if registry_root.exists() || fs::symlink_metadata(registry_root).is_ok() {
        return Err(
            "ku_dev_demo_registry_exists: REGISTRY_ROOT already exists. Choose a new \
            directory; an existing Registry (real or dev) is never modified."
                .into(),
        );
    }
    if public_key_file.exists() || fs::symlink_metadata(public_key_file).is_ok() {
        return Err(
            "ku_dev_demo_public_key_exists: PUBLIC_KEY_FILE already exists. Choose a \
            new file name; existing trust files are never overwritten."
                .into(),
        );
    }
    for (path, field) in [
        (registry_root, "REGISTRY_ROOT"),
        (public_key_file, "PUBLIC_KEY_FILE"),
    ] {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        if !parent.is_dir() {
            return Err(format!(
                "ku_dev_demo_parent_missing: the parent directory of {field} must already \
                 exist; this command does not create parents."
            )
            .into());
        }
    }

    let signing_key = SigningKey::generate(&mut OsRng);
    let public_key_hex = encode_hex(signing_key.verifying_key().as_bytes());
    let release_id = format!("dev-demo-synthetic-{}", &public_key_hex[..12]);

    fs::create_dir(registry_root)?;
    // Build inputs under the new root, then remove them once the immutable
    // release has been packaged; only releases/ and state/ remain.
    let build_dir = registry_root.join("dev-demo-build");
    fs::create_dir(&build_dir)?;
    let obr_path = build_dir.join("concepts.obr");
    write_dev_obr(&obr_path)?;
    let sbom = build_dir.join("sbom.spdx.json");
    fs::write(
        &sbom,
        format!(
            r#"{{"spdxVersion":"SPDX-2.3","dataLicense":"CC0-1.0","name":"{DEV_MARKER}","comment":"DEV-ONLY synthetic Concept Registry; not a distributed release"}}"#
        ),
    )?;
    let sources = REQUIRED_SOURCES
        .iter()
        .map(|name| ConceptRegistryReleaseSource {
            name: (*name).to_owned(),
            snapshot_id: format!("{DEV_MARKER}-{name}"),
            source_uri: format!("urn:{DEV_MARKER}:{name}"),
            license: "DEV-ONLY-SYNTHETIC-NOT-A-REAL-SOURCE".to_owned(),
            snapshot_blake3: blake3::hash(format!("{DEV_MARKER}:snapshot:{name}").as_bytes())
                .to_hex()
                .to_string(),
            download_blake3: blake3::hash(format!("{DEV_MARKER}:download:{name}").as_bytes())
                .to_hex()
                .to_string(),
        })
        .collect();
    package_concept_registry_release(
        &obr_path,
        &sbom,
        registry_root,
        ConceptRegistryReleasePackageInput {
            release_id: release_id.clone(),
            sources,
        },
        &signing_key,
    )?;
    activate_concept_registry_release(registry_root, &release_id, &signing_key.verifying_key())?;
    drop(signing_key);
    fs::remove_dir_all(&build_dir)?;

    // Re-verify the activated package exactly as a host would before writing
    // the public key, so a written key always names a usable release.
    let verifying = ku_core::parse_concept_registry_verifying_key(&public_key_hex)?;
    let active = resolve_active_concept_registry_release(registry_root, &verifying)?;
    if active.release_id != release_id {
        return Err("ku_dev_demo_registry_unverified: activated release mismatch".into());
    }
    write_new_text(public_key_file, &format!("{public_key_hex}\n"))?;

    Ok(DevDemoReport {
        release_id,
        public_key_hex,
        labels: DEV_CONCEPTS
            .iter()
            .flat_map(|(_, _, labels)| labels.iter().map(|label| (*label).to_owned()))
            .collect(),
    })
}

fn dev_ccid(canonical: &str) -> [u8; 16] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(DEV_CCID_DOMAIN);
    hasher.update(canonical.as_bytes());
    hasher.finalize().as_bytes()[..16].try_into().unwrap()
}

/// Write `concepts.obr`, both fixed indexes and the manifest next to it.
fn write_dev_obr(obr_path: &Path) -> Result<(), Box<dyn Error>> {
    let label_total: usize = DEV_CONCEPTS.iter().map(|entry| entry.2.len()).sum();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"OBR1");
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&(DEV_CONCEPTS.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&(label_total as u64).to_le_bytes());
    bytes.extend_from_slice(&[0; 8]);
    let mut label_records = Vec::new();
    let mut ccid_records = Vec::new();
    for (canonical, category, labels) in DEV_CONCEPTS {
        let offset = bytes.len() as u64;
        let ccid = dev_ccid(canonical);
        bytes.extend_from_slice(&ccid);
        // ext_id 0 and source 0: no external identifier is claimed.
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&[0, *category]);
        bytes.extend_from_slice(&u16::try_from(canonical.len())?.to_le_bytes());
        bytes.extend_from_slice(canonical.as_bytes());
        bytes.extend_from_slice(&u16::try_from(labels.len())?.to_le_bytes());
        for label in labels.iter() {
            bytes.extend_from_slice(&u16::try_from(label.len())?.to_le_bytes());
            bytes.extend_from_slice(label.as_bytes());
            let key: [u8; 16] = blake3::hash(label.to_lowercase().as_bytes()).as_bytes()[..16]
                .try_into()
                .unwrap();
            label_records.push((key, offset));
        }
        ccid_records.push((ccid, offset));
    }
    fs::write(obr_path, &bytes)?;
    let checksum = *blake3::hash(&bytes).as_bytes();
    let write_index = |path: &Path,
                       magic: [u8; 4],
                       mut records: Vec<([u8; 16], u64)>|
     -> std::io::Result<ConceptRegistryIndexManifest> {
        records.sort();
        let mut index = Vec::new();
        index.extend_from_slice(&magic);
        index.extend_from_slice(&REGISTRY_INDEX_VERSION.to_le_bytes());
        index.extend_from_slice(&(records.len() as u64).to_le_bytes());
        index.extend_from_slice(&checksum);
        index.extend_from_slice(&[0; 16]);
        for (key, offset) in &records {
            index.extend_from_slice(key);
            index.extend_from_slice(&offset.to_le_bytes());
        }
        fs::write(path, &index)?;
        Ok(ConceptRegistryIndexManifest {
            schema_version: 1,
            record_size: 24,
            record_count: records.len() as u64,
            blake3: blake3::hash(&index).to_hex().to_string(),
            file_size: index.len() as u64,
        })
    };
    let label_index = write_index(
        &IndexedConceptRegistry::label_index_path(obr_path),
        LABEL_INDEX_MAGIC,
        label_records,
    )?;
    let ccid_index = write_index(
        &IndexedConceptRegistry::ccid_index_path(obr_path),
        CCID_INDEX_MAGIC,
        ccid_records,
    )?;
    let manifest = ConceptRegistryManifest {
        manifest_version: 1,
        obr_schema_version: 1,
        builder_version: format!("{DEV_MARKER}/1"),
        dedup_policy_version: format!("{DEV_MARKER}/none"),
        built_at_utc: "2026-11-19T00:00:00Z".into(),
        obr_blake3: blake3::hash(&bytes).to_hex().to_string(),
        entry_count: DEV_CONCEPTS.len() as u64,
        label_count: label_total as u64,
        sources: REQUIRED_SOURCES
            .iter()
            .map(|name| {
                (
                    (*name).to_owned(),
                    ConceptRegistrySourceManifest {
                        snapshot_id: format!("{DEV_MARKER}-{name}"),
                        source_uri: format!("urn:{DEV_MARKER}:{name}"),
                        license: "DEV-ONLY-SYNTHETIC-NOT-A-REAL-SOURCE".into(),
                        record_count: 1,
                    },
                )
            })
            .collect(),
        label_index: Some(label_index),
        ccid_index: Some(ccid_index),
    };
    fs::write(manifest_path(obr_path), serde_json::to_vec(&manifest)?)?;
    Ok(())
}

fn write_new_text(path: &Path, text: &str) -> std::io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ku_core::concept_registry_manifest::load_and_validate_manifest_uncached;
    use ku_core::{ConceptRegistry, ResolveResult};

    #[test]
    fn dev_demo_registry_activates_verifies_and_resolves_every_label() {
        let root = tempfile::tempdir().unwrap();
        let registry = root.path().join("registry");
        let key_file = root.path().join("registry-public-key.txt");
        let report = create_dev_demo_registry(&registry, &key_file).unwrap();
        assert!(report.release_id.starts_with("dev-demo-synthetic-"));

        // Only the signed release and activation state remain under the root.
        let mut entries: Vec<_> = fs::read_dir(&registry)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        entries.sort();
        assert_eq!(entries, ["releases", "state"]);

        let key_text = fs::read_to_string(&key_file).unwrap();
        assert_eq!(key_text.trim(), report.public_key_hex);
        let verifying = ku_core::parse_concept_registry_verifying_key(key_text.trim()).unwrap();
        let active = resolve_active_concept_registry_release(&registry, &verifying).unwrap();
        assert_eq!(active.release_id, report.release_id);
        assert!(active.stamp.builder_version.contains(DEV_MARKER));

        let header = ConceptRegistry::inspect_obr(&active.obr_path).unwrap();
        let manifest = load_and_validate_manifest_uncached(&active.obr_path, header).unwrap();
        let indexed = IndexedConceptRegistry::open(&active.obr_path, &manifest, 16).unwrap();
        for (canonical, _, labels) in DEV_CONCEPTS {
            for label in labels.iter() {
                match indexed.resolve_checked(label).unwrap() {
                    ResolveResult::Found(concept) => assert_eq!(concept.ccid, dev_ccid(canonical)),
                    other => panic!("label {label} did not resolve uniquely: {other:?}"),
                }
            }
        }
        assert!(matches!(
            indexed.resolve_checked("not-a-dev-concept").unwrap(),
            ResolveResult::NotFound
        ));

        // A different self-issued signer must not verify this release.
        let other = SigningKey::generate(&mut OsRng).verifying_key();
        assert!(resolve_active_concept_registry_release(&registry, &other).is_err());
    }

    #[test]
    fn dev_demo_registry_refuses_existing_targets_without_mutation() {
        let root = tempfile::tempdir().unwrap();
        let registry = root.path().join("registry");
        let key_file = root.path().join("registry-public-key.txt");
        fs::create_dir(&registry).unwrap();
        fs::write(registry.join("keep.txt"), b"existing").unwrap();
        let error = create_dev_demo_registry(&registry, &key_file)
            .err()
            .unwrap()
            .to_string();
        assert!(error.starts_with("ku_dev_demo_registry_exists:"));
        assert_eq!(fs::read(registry.join("keep.txt")).unwrap(), b"existing");
        assert!(!key_file.exists());

        let fresh = root.path().join("fresh-registry");
        fs::write(&key_file, b"existing-key").unwrap();
        let error = create_dev_demo_registry(&fresh, &key_file)
            .err()
            .unwrap()
            .to_string();
        assert!(error.starts_with("ku_dev_demo_public_key_exists:"));
        assert_eq!(fs::read(&key_file).unwrap(), b"existing-key");
        assert!(!fresh.exists());

        let orphan = root.path().join("missing-parent").join("registry");
        let error = create_dev_demo_registry(&orphan, &root.path().join("k2.txt"))
            .err()
            .unwrap()
            .to_string();
        assert!(error.starts_with("ku_dev_demo_parent_missing:"));
        assert!(!root.path().join("missing-parent").exists());
    }
}
