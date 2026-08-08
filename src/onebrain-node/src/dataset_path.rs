//! Owner-scoped dataset paths used before the generation switch in Task 11.

use std::path::{Path, PathBuf};

use ku_kql::blob_storage::BlobStorageError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DatasetGenerationId(pub [u8; 32]);

impl DatasetGenerationId {
    pub const BOOTSTRAP: Self = Self([0; 32]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BaseStorageOwnerId(u16);

impl BaseStorageOwnerId {
    pub const CANONICAL: Self = Self(0x0001);
    pub const VAULT: Self = Self(0x0002);
    pub const QUARANTINE: Self = Self(0x0003);
    pub const BLOB: Self = Self(0x0004);
    pub const PENDING_BLOB_INTENT: Self = Self(0x0005);
    pub const SOURCE_CAPTURE_INTENT: Self = Self(0x0006);
    pub const RECONCILIATION: Self = Self(0x0007);
    pub const INVENTORY: Self = Self(0x0008);
    pub const OUTBOX: Self = Self(0x0009);
    pub const PROVENANCE: Self = Self(0x000A);
    pub const PRIVATE_KQL: Self = Self(0x000B);
    pub const PRIVATE_POMV: Self = Self(0x000C);
    pub const OPERATIONAL: Self = Self(0x000D);
    pub const ROLLOUT: Self = Self(0x000E);
    pub const OPTIONAL_NETWORK: Self = Self(0x000F);
    pub const MIGRATION: Self = Self(0x0010);
    pub const BASE_OPERATIONS: Self = Self(0x0011);
    pub const INTERPRETATION_CONFIG: Self = Self(0x0012);
    pub const IDENTITY: Self = Self(0x0013);
    pub const REGISTRY_METADATA: Self = Self(0x0014);
    pub const DERIVED_INDEX: Self = Self(0x0015);
    pub const RETRIEVER_PROJECTION: Self = Self(0x0016);

    pub fn new(value: u16) -> Result<Self, BlobStorageError> {
        if (0x0001..=0x0016).contains(&value) {
            Ok(Self(value))
        } else {
            Err(BlobStorageError::InvalidConfig)
        }
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Debug)]
pub struct ActiveDatasetPathResolver {
    root: PathBuf,
    generation: DatasetGenerationId,
}

impl ActiveDatasetPathResolver {
    pub fn new(
        dataset_root: impl AsRef<Path>,
        generation: DatasetGenerationId,
    ) -> Result<Self, BlobStorageError> {
        let root = dataset_root
            .as_ref()
            .join("datasets/generations")
            .join(hex(&generation.0));
        if !root.is_dir() || std::fs::symlink_metadata(&root)?.file_type().is_symlink() {
            return Err(BlobStorageError::InvalidConfig);
        }
        Ok(Self {
            root: root.canonicalize()?,
            generation,
        })
    }
}

impl DatasetPathResolver for ActiveDatasetPathResolver {
    fn current_generation(&self) -> DatasetGenerationId {
        self.generation
    }

    fn owner_path(&self, owner: BaseStorageOwnerId) -> Result<PathBuf, BlobStorageError> {
        let name = owner_name(owner).ok_or(BlobStorageError::InvalidConfig)?;
        let path = self.root.join("owners").join(name);
        std::fs::create_dir_all(&path)?;
        let canonical = path.canonicalize()?;
        if !canonical.starts_with(&self.root) {
            return Err(BlobStorageError::InvalidConfig);
        }
        Ok(canonical)
    }
}

pub trait DatasetPathResolver: Send + Sync {
    fn current_generation(&self) -> DatasetGenerationId;
    fn owner_path(&self, owner: BaseStorageOwnerId) -> Result<PathBuf, BlobStorageError>;
}

#[derive(Clone, Debug)]
pub struct BootstrapDatasetPathResolver {
    root: PathBuf,
}

impl BootstrapDatasetPathResolver {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, BlobStorageError> {
        let root = root.as_ref().to_path_buf();
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }
}

impl DatasetPathResolver for BootstrapDatasetPathResolver {
    fn current_generation(&self) -> DatasetGenerationId {
        DatasetGenerationId::BOOTSTRAP
    }

    fn owner_path(&self, owner: BaseStorageOwnerId) -> Result<PathBuf, BlobStorageError> {
        let name = owner_name(owner).ok_or(BlobStorageError::InvalidConfig)?;
        let path = self.root.join("owners").join(name);
        std::fs::create_dir_all(&path)?;
        Ok(path)
    }
}

fn owner_name(owner: BaseStorageOwnerId) -> Option<&'static str> {
    Some(match owner.get() {
        0x0001 => "canonical",
        0x0002 => "vault",
        0x0003 => "quarantine",
        0x0004 => "blob",
        0x0005 => "pending_blob_intent",
        0x0006 => "source_capture_intent",
        0x0007 => "reconciliation",
        0x0008 => "inventory",
        0x0009 => "outbox",
        0x000A => "provenance",
        0x000B => "private_kql",
        0x000C => "private_pomv",
        0x000D => "operational",
        0x000E => "rollout",
        0x000F => "optional_network",
        0x0010 => "migration",
        0x0011 => "base_operations",
        0x0012 => "interpretation_config",
        0x0013 => "identity",
        0x0014 => "registry_metadata",
        0x0015 => "derived_index",
        0x0016 => "retriever_projection",
        _ => return None,
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_resolver_rejects_reserved_owners_and_scopes_paths() {
        assert!(BaseStorageOwnerId::new(0).is_err());
        assert!(BaseStorageOwnerId::new(0x0017).is_err());
        let directory = tempfile::tempdir().unwrap();
        let resolver = BootstrapDatasetPathResolver::new(directory.path()).unwrap();
        let blob = resolver.owner_path(BaseStorageOwnerId::BLOB).unwrap();
        assert!(blob.ends_with("blob"));
        assert!(blob.starts_with(directory.path()));
        assert_eq!(
            resolver.current_generation(),
            DatasetGenerationId::BOOTSTRAP
        );
    }
}
