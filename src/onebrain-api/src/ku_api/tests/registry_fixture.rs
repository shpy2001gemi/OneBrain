//! Test-only signed, indexed one-concept Registry; no production dataset is opened.
use ed25519_dalek::SigningKey;
use ku_core::concept_registry_manifest::{
    manifest_path, ConceptRegistryIndexManifest, ConceptRegistrySourceManifest,
};
use ku_core::indexed_concept_registry::{
    CCID_INDEX_MAGIC, LABEL_INDEX_MAGIC, REGISTRY_INDEX_VERSION,
};
use ku_core::{
    activate_concept_registry_release, package_concept_registry_release, ConceptRegistryManifest,
    ConceptRegistryReleasePackageInput, ConceptRegistryReleaseSource, IndexedConceptRegistry,
};
use onebrain_node::concept_registry_runtime::ConceptRegistryGenerationManager;
use onebrain_node::{ConceptRegistryMode, NodeConfig};
use std::{fs, path::Path, sync::Arc};

pub(super) fn registry(root: &Path) -> Arc<ConceptRegistryGenerationManager> {
    registry_for_label(root, "water")
}
pub(super) fn registry_for_label(
    root: &Path,
    label: &str,
) -> Arc<ConceptRegistryGenerationManager> {
    registry_for_entries(root, &[([7; 16], 0, label, &[label])])
}
/// `(ccid, category, canonical_name, labels)`; category uses the OBR byte
/// (0 Entity, 1 Property, 2 Unit, ...).
pub(super) type Entry<'a> = ([u8; 16], u8, &'a str, &'a [&'a str]);
pub(super) fn registry_for_entries(
    root: &Path,
    entries: &[Entry<'_>],
) -> Arc<ConceptRegistryGenerationManager> {
    let source = root.join("source");
    fs::create_dir_all(&source).unwrap();
    let obr_path = source.join("registry.obr");
    let label_total: usize = entries.iter().map(|e| e.3.len()).sum();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"OBR1");
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&(entries.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&(label_total as u64).to_le_bytes());
    bytes.extend_from_slice(&[0; 8]);
    let mut label_records = Vec::new();
    let mut ccid_records = Vec::new();
    for (ccid, category, canonical, labels) in entries {
        let offset = bytes.len() as u64;
        bytes.extend_from_slice(ccid);
        bytes.extend_from_slice(&283u32.to_le_bytes());
        bytes.extend_from_slice(&[0, *category]);
        bytes.extend_from_slice(&(canonical.len() as u16).to_le_bytes());
        bytes.extend_from_slice(canonical.as_bytes());
        bytes.extend_from_slice(&(labels.len() as u16).to_le_bytes());
        for label in labels.iter() {
            bytes.extend_from_slice(&(label.len() as u16).to_le_bytes());
            bytes.extend_from_slice(label.as_bytes());
            let key: [u8; 16] = blake3::hash(label.to_lowercase().as_bytes()).as_bytes()[..16]
                .try_into()
                .unwrap();
            label_records.push((key, offset));
        }
        ccid_records.push((*ccid, offset));
    }
    fs::write(&obr_path, &bytes).unwrap();
    let checksum = *blake3::hash(&bytes).as_bytes();
    let index = |path: &Path, magic: [u8; 4], mut records: Vec<([u8; 16], u64)>| {
        records.sort();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&magic);
        bytes.extend_from_slice(&REGISTRY_INDEX_VERSION.to_le_bytes());
        bytes.extend_from_slice(&(records.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&checksum);
        bytes.extend_from_slice(&[0; 16]);
        for (key, offset) in &records {
            bytes.extend_from_slice(key);
            bytes.extend_from_slice(&offset.to_le_bytes());
        }
        fs::write(path, &bytes).unwrap();
        ConceptRegistryIndexManifest {
            schema_version: 1,
            record_size: 24,
            record_count: records.len() as u64,
            blake3: blake3::hash(&bytes).to_hex().to_string(),
            file_size: bytes.len() as u64,
        }
    };
    let label_index = index(
        &IndexedConceptRegistry::label_index_path(&obr_path),
        LABEL_INDEX_MAGIC,
        label_records,
    );
    let ccid_index = index(
        &IndexedConceptRegistry::ccid_index_path(&obr_path),
        CCID_INDEX_MAGIC,
        ccid_records,
    );
    let names = ["chebi", "geonames", "ncbi", "wikidata", "wordnet"];
    let manifest = ConceptRegistryManifest {
        manifest_version: 1,
        obr_schema_version: 1,
        builder_version: "test-builder".into(),
        dedup_policy_version: "test-dedup".into(),
        built_at_utc: "2026-07-23T00:00:00Z".into(),
        obr_blake3: blake3::hash(&bytes).to_hex().to_string(),
        entry_count: entries.len() as u64,
        label_count: label_total as u64,
        sources: names
            .iter()
            .map(|name| {
                (
                    name.to_string(),
                    ConceptRegistrySourceManifest {
                        snapshot_id: format!("{name}-test-snapshot"),
                        source_uri: format!("https://example.test/{name}"),
                        license: "test-license".into(),
                        record_count: 1,
                    },
                )
            })
            .collect(),
        label_index: Some(label_index),
        ccid_index: Some(ccid_index),
    };
    fs::write(
        manifest_path(&obr_path),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let sbom = source.join("sbom.spdx.json");
    fs::write(
        &sbom,
        br#"{"spdxVersion":"SPDX-2.3","dataLicense":"CC0-1.0"}"#,
    )
    .unwrap();
    let signing = SigningKey::from_bytes(&[0x42; 32]);
    let sources = names
        .iter()
        .enumerate()
        .map(|(i, name)| ConceptRegistryReleaseSource {
            name: name.to_string(),
            snapshot_id: format!("{name}-test-snapshot"),
            source_uri: format!("https://example.test/{name}"),
            license: "test-license".into(),
            snapshot_blake3: blake3::hash(&[i as u8 + 1; 8]).to_hex().to_string(),
            download_blake3: blake3::hash(&[i as u8 + 2; 8]).to_hex().to_string(),
        })
        .collect();
    package_concept_registry_release(
        &obr_path,
        &sbom,
        &root.join("registry"),
        ConceptRegistryReleasePackageInput {
            release_id: "registry-v1".into(),
            sources,
        },
        &signing,
    )
    .unwrap();
    activate_concept_registry_release(
        &root.join("registry"),
        "registry-v1",
        &signing.verifying_key(),
    )
    .unwrap();
    Arc::new(
        ConceptRegistryGenerationManager::open(NodeConfig {
            data_dir: root.into(),
            concept_registry_mode: ConceptRegistryMode::Required,
            concept_registry_release_root: Some(root.join("registry")),
            concept_registry_release_public_key: Some(onebrain_base_contract::ku_payload::hex(
                signing.verifying_key().as_bytes(),
            )),
            ..Default::default()
        })
        .unwrap(),
    )
}
