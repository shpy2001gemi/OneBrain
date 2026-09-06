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
    let source = root.join("source");
    fs::create_dir_all(&source).unwrap();
    let obr_path = source.join("registry.obr");
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
    fs::write(&obr_path, &bytes).unwrap();
    let checksum = *blake3::hash(&bytes).as_bytes();
    let index = |path: &Path, magic: [u8; 4], key: [u8; 16]| {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&magic);
        bytes.extend_from_slice(&REGISTRY_INDEX_VERSION.to_le_bytes());
        bytes.extend_from_slice(&1u64.to_le_bytes());
        bytes.extend_from_slice(&checksum);
        bytes.extend_from_slice(&[0; 16]);
        bytes.extend_from_slice(&key);
        bytes.extend_from_slice(&32u64.to_le_bytes());
        fs::write(path, &bytes).unwrap();
        ConceptRegistryIndexManifest {
            schema_version: 1,
            record_size: 24,
            record_count: 1,
            blake3: blake3::hash(&bytes).to_hex().to_string(),
            file_size: bytes.len() as u64,
        }
    };
    let label_key = blake3::hash(b"water").as_bytes()[..16].try_into().unwrap();
    let label_index = index(
        &IndexedConceptRegistry::label_index_path(&obr_path),
        LABEL_INDEX_MAGIC,
        label_key,
    );
    let ccid_index = index(
        &IndexedConceptRegistry::ccid_index_path(&obr_path),
        CCID_INDEX_MAGIC,
        [7; 16],
    );
    let names = ["chebi", "geonames", "ncbi", "wikidata", "wordnet"];
    let manifest = ConceptRegistryManifest {
        manifest_version: 1,
        obr_schema_version: 1,
        builder_version: "test-builder".into(),
        dedup_policy_version: "test-dedup".into(),
        built_at_utc: "2026-07-23T00:00:00Z".into(),
        obr_blake3: blake3::hash(&bytes).to_hex().to_string(),
        entry_count: 1,
        label_count: 1,
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
