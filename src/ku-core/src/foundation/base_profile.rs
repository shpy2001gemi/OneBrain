//! Frozen machine-readable registry digests for the OneBrain Base v1 profile.
//!
//! Production code derives every digest from typed Rust constants. JSON
//! vectors are consumed only by integration tests that independently verify
//! the same canonical row encoding.

use super::canonical::ResourceProfile;
use super::content_id::ReservedDomain;
use super::schema_registry::{EVENT_TYPES_V1, OBJECT_KINDS_V1, SCHEMAS_V1};

const SCHEMA_REGISTRY_DOMAIN: &[u8] = b"onebrain:base-v1:schema-registry:1\0";
const DOMAIN_REGISTRY_DOMAIN: &[u8] = b"onebrain:base-v1:domain-registry:1\0";
const RESOURCE_REGISTRY_DOMAIN: &[u8] = b"onebrain:base-v1:resource-registry:1\0";
const STORAGE_OWNER_REGISTRY_DOMAIN: &[u8] = b"onebrain:base-v1:storage-owner-registry:1\0";
const BASE_PROFILE_DOMAIN: &[u8] = b"onebrain:base-v1:profile:1\0";

pub const BASE_PROFILE_MAJOR: u16 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BaseProfileRegistry {
    pub profile_major: u16,
    pub canonical_schema_digest: [u8; 32],
    pub domain_registry_digest: [u8; 32],
    pub resource_registry_digest: [u8; 32],
    pub storage_owner_registry_digest: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageOwnerRegistryEntry {
    pub id: u16,
    pub name: &'static str,
}

pub const STORAGE_OWNERS_V1: &[StorageOwnerRegistryEntry] = &[
    owner(0x0001, "canonical"),
    owner(0x0002, "vault"),
    owner(0x0003, "quarantine"),
    owner(0x0004, "blob"),
    owner(0x0005, "pending_blob_intent"),
    owner(0x0006, "source_capture_intent"),
    owner(0x0007, "reconciliation"),
    owner(0x0008, "inventory"),
    owner(0x0009, "outbox"),
    owner(0x000A, "provenance"),
    owner(0x000B, "private_kql"),
    owner(0x000C, "private_pomv"),
    owner(0x000D, "operational"),
    owner(0x000E, "rollout"),
    owner(0x000F, "optional_network"),
    owner(0x0010, "migration"),
    owner(0x0011, "base_operations"),
    owner(0x0012, "interpretation_config"),
    owner(0x0013, "identity"),
    owner(0x0014, "registry_metadata"),
    owner(0x0015, "derived_index"),
    owner(0x0016, "retriever_projection"),
];

const fn owner(id: u16, name: &'static str) -> StorageOwnerRegistryEntry {
    StorageOwnerRegistryEntry { id, name }
}

pub fn base_v1_profile_registry() -> BaseProfileRegistry {
    BaseProfileRegistry {
        profile_major: BASE_PROFILE_MAJOR,
        canonical_schema_digest: schema_registry_digest(),
        domain_registry_digest: domain_registry_digest(),
        resource_registry_digest: resource_registry_digest(),
        storage_owner_registry_digest: storage_owner_registry_digest(),
    }
}

pub fn base_v1_profile_digest() -> [u8; 32] {
    let registry = base_v1_profile_registry();
    let mut hasher = blake3::Hasher::new();
    hasher.update(BASE_PROFILE_DOMAIN);
    hasher.update(&registry.profile_major.to_be_bytes());
    hasher.update(&registry.canonical_schema_digest);
    hasher.update(&registry.domain_registry_digest);
    hasher.update(&registry.resource_registry_digest);
    hasher.update(&registry.storage_owner_registry_digest);
    *hasher.finalize().as_bytes()
}

fn schema_registry_digest() -> [u8; 32] {
    let mut rows = Vec::new();
    rows.extend(
        SCHEMAS_V1
            .iter()
            .map(|entry| schema_row(0, entry.id, entry.name)),
    );
    rows.extend(
        OBJECT_KINDS_V1
            .iter()
            .map(|entry| schema_row(1, entry.id, entry.name)),
    );
    rows.extend(
        EVENT_TYPES_V1
            .iter()
            .map(|entry| schema_row(2, entry.id, entry.name)),
    );
    digest_rows(SCHEMA_REGISTRY_DOMAIN, rows)
}

fn domain_registry_digest() -> [u8; 32] {
    let rows = ReservedDomain::ALL.into_iter().map(|domain| {
        let mut row = Vec::new();
        push_string(&mut row, domain.name());
        row.extend_from_slice(&domain.version().to_be_bytes());
        row
    });
    digest_rows(DOMAIN_REGISTRY_DOMAIN, rows)
}

fn resource_registry_digest() -> [u8; 32] {
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

fn storage_owner_registry_digest() -> [u8; 32] {
    let rows = STORAGE_OWNERS_V1.iter().map(|entry| {
        let mut row = Vec::new();
        row.extend_from_slice(&entry.id.to_be_bytes());
        push_string(&mut row, entry.name);
        row
    });
    digest_rows(STORAGE_OWNER_REGISTRY_DOMAIN, rows)
}

fn schema_row(class: u8, id: u64, name: &str) -> Vec<u8> {
    let mut row = vec![class];
    row.extend_from_slice(&id.to_be_bytes());
    push_string(&mut row, name);
    row
}

fn push_string(output: &mut Vec<u8>, value: &str) {
    let length = u16::try_from(value.len()).expect("Base registry name exceeds u16");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_owner_ids_are_contiguous_and_nonzero() {
        assert_eq!(STORAGE_OWNERS_V1.len(), 22);
        for (offset, entry) in STORAGE_OWNERS_V1.iter().enumerate() {
            assert_eq!(entry.id, u16::try_from(offset + 1).unwrap());
        }
    }

    #[test]
    fn component_change_changes_the_profile_digest() {
        let registry = base_v1_profile_registry();
        let digest = base_v1_profile_digest();
        let mut hasher = blake3::Hasher::new();
        hasher.update(BASE_PROFILE_DOMAIN);
        hasher.update(&registry.profile_major.to_be_bytes());
        hasher.update(&registry.canonical_schema_digest);
        hasher.update(&registry.domain_registry_digest);
        hasher.update(&registry.resource_registry_digest);
        let mut changed_owner_digest = registry.storage_owner_registry_digest;
        changed_owner_digest[0] ^= 1;
        hasher.update(&changed_owner_digest);
        assert_ne!(digest, *hasher.finalize().as_bytes());
    }
}
