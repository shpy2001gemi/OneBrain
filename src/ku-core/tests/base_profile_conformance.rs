use std::fs;
use std::path::PathBuf;

use ku_core::foundation::schema_registry::SCHEMAS_V1;
use ku_core::foundation::{
    base_v1_profile_digest, base_v1_profile_registry, ReservedDomain, ResourceProfile,
};
use serde::Deserialize;

const SCHEMA_REGISTRY_DOMAIN: &[u8] = b"onebrain:base-v1:schema-registry:1\0";
const DOMAIN_REGISTRY_DOMAIN: &[u8] = b"onebrain:base-v1:domain-registry:1\0";
const RESOURCE_REGISTRY_DOMAIN: &[u8] = b"onebrain:base-v1:resource-registry:1\0";
const STORAGE_OWNER_REGISTRY_DOMAIN: &[u8] = b"onebrain:base-v1:storage-owner-registry:1\0";
const BASE_PROFILE_DOMAIN: &[u8] = b"onebrain:base-v1:profile:1\0";

#[derive(Deserialize)]
struct StorageContract {
    format: String,
    storage_owner_table: StorageOwnerTable,
}

#[derive(Deserialize)]
struct StorageOwnerTable {
    encoding: String,
    owners: Vec<StorageOwnerRow>,
}

#[derive(Deserialize)]
struct StorageOwnerRow {
    name: String,
    code_u16: u16,
    base_storage_owner_bytes: String,
    archive_owner_bytes: String,
}

#[derive(Deserialize)]
struct DerivedProjectionContract {
    format: String,
    object_mappings: Vec<ProjectionMapping>,
    event_mappings: Vec<ProjectionMapping>,
}

#[derive(Deserialize)]
struct ProjectionMapping {
    id: u64,
    kind: String,
}

#[test]
fn machine_contracts_match_the_typed_base_profile_registry() {
    let storage: StorageContract = load_vector("base-v1-storage-integrity-v1.json");
    let projection: DerivedProjectionContract = load_vector("base-v1-derived-projection-v1.json");
    assert_eq!(storage.format, "onebrain/base-v1-storage-integrity/1");
    assert_eq!(projection.format, "onebrain/base-v1-derived-projection/1");

    let registry = base_v1_profile_registry();
    assert_eq!(registry.profile_major, 1);
    assert_eq!(
        registry.canonical_schema_digest,
        expected_schema_digest(&projection)
    );
    assert_eq!(registry.domain_registry_digest, expected_domain_digest());
    assert_eq!(
        registry.resource_registry_digest,
        expected_resource_digest()
    );
    assert_eq!(
        registry.storage_owner_registry_digest,
        expected_storage_owner_digest(&storage)
    );
    assert_eq!(base_v1_profile_digest(), expected_profile_digest(&registry));
}

fn load_vector<T: for<'de> Deserialize<'de>>(name: &str) -> T {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../test-vectors/vnext")
        .join(name);
    let bytes = fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("decode {}: {error}", path.display()))
}

fn expected_schema_digest(profile: &DerivedProjectionContract) -> [u8; 32] {
    let mut rows = Vec::new();
    rows.extend(
        SCHEMAS_V1
            .iter()
            .map(|entry| schema_row(0, entry.id, entry.name)),
    );
    rows.extend(
        profile
            .object_mappings
            .iter()
            .map(|entry| schema_row(1, entry.id, &entry.kind)),
    );
    rows.extend(
        profile
            .event_mappings
            .iter()
            .map(|entry| schema_row(2, entry.id, &entry.kind)),
    );
    digest_rows(SCHEMA_REGISTRY_DOMAIN, rows)
}

fn expected_domain_digest() -> [u8; 32] {
    let rows = ReservedDomain::ALL.into_iter().map(|domain| {
        let mut row = Vec::new();
        push_string(&mut row, domain.name());
        row.extend_from_slice(&domain.version().to_be_bytes());
        row
    });
    digest_rows(DOMAIN_REGISTRY_DOMAIN, rows)
}

fn expected_resource_digest() -> [u8; 32] {
    let rows = [
        ResourceProfile::ControlV1,
        ResourceProfile::ObjectV1,
        ResourceProfile::ManifestV1,
    ]
    .into_iter()
    .map(|profile| {
        let limits = profile.limits();
        let mut row = Vec::new();
        push_string(&mut row, profile.name());
        for value in [
            limits.max_bytes,
            limits.max_depth,
            limits.max_map_entries,
            limits.max_array_items,
            limits.max_total_nodes,
            limits.max_scalar_bytes,
        ] {
            row.extend_from_slice(&(value as u64).to_be_bytes());
        }
        row
    });
    digest_rows(RESOURCE_REGISTRY_DOMAIN, rows)
}

fn expected_storage_owner_digest(profile: &StorageContract) -> [u8; 32] {
    assert_eq!(profile.storage_owner_table.encoding, "big-endian-u16");
    let rows = profile.storage_owner_table.owners.iter().map(|owner| {
        let expected = format!("{:04x}", owner.code_u16);
        assert_eq!(owner.base_storage_owner_bytes, expected);
        assert_eq!(owner.archive_owner_bytes, expected);
        let mut row = Vec::new();
        row.extend_from_slice(&owner.code_u16.to_be_bytes());
        push_string(&mut row, &owner.name);
        row
    });
    digest_rows(STORAGE_OWNER_REGISTRY_DOMAIN, rows)
}

fn expected_profile_digest(registry: &ku_core::foundation::BaseProfileRegistry) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(BASE_PROFILE_DOMAIN);
    hasher.update(&registry.profile_major.to_be_bytes());
    hasher.update(&registry.canonical_schema_digest);
    hasher.update(&registry.domain_registry_digest);
    hasher.update(&registry.resource_registry_digest);
    hasher.update(&registry.storage_owner_registry_digest);
    *hasher.finalize().as_bytes()
}

fn schema_row(class: u8, id: u64, name: &str) -> Vec<u8> {
    let mut row = vec![class];
    row.extend_from_slice(&id.to_be_bytes());
    push_string(&mut row, name);
    row
}

fn push_string(output: &mut Vec<u8>, value: &str) {
    let length = u16::try_from(value.len()).expect("registry name length");
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(value.as_bytes());
}

fn digest_rows(domain: &[u8], rows: impl IntoIterator<Item = Vec<u8>>) -> [u8; 32] {
    let mut rows: Vec<Vec<u8>> = rows.into_iter().collect();
    rows.sort();
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(&(rows.len() as u32).to_be_bytes());
    for row in rows {
        hasher.update(&(row.len() as u32).to_be_bytes());
        hasher.update(&row);
    }
    *hasher.finalize().as_bytes()
}
