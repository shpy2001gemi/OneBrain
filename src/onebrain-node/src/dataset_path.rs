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
    pub const BLOB: Self = Self(0x0004);
    pub const PENDING_BLOB_INTENT: Self = Self(0x0005);

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
