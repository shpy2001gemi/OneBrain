//! Node-owned retriever projection rebuilt exclusively from durable Vault sources.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use ku_core::foundation::VaultSourceSnapshotRecord;
use ku_mediator::retriever::{
    retriever_source_root, KuRetriever, RetrieverConfig, RetrieverError, RetrieverIndexEnvelope,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DerivedProjectionOpenState {
    Ready,
    Rebuilt,
    Degraded { reason: String },
}

pub struct RetrieverProjectionService {
    snapshot_path: PathBuf,
    retriever: Arc<RwLock<KuRetriever>>,
    source_root: [u8; 32],
}

impl RetrieverProjectionService {
    pub fn unavailable(
        owner_root: impl AsRef<Path>,
        accepted_vnext_root: [u8; 32],
        reason: impl Into<String>,
    ) -> (Self, DerivedProjectionOpenState) {
        let snapshot_path = owner_root.as_ref().join("retriever-index-v2.json");
        (
            Self {
                snapshot_path,
                retriever: Arc::new(RwLock::new(KuRetriever::default())),
                source_root: retriever_source_root(accepted_vnext_root, [0; 32]),
            },
            DerivedProjectionOpenState::Degraded {
                reason: reason.into(),
            },
        )
    }

    pub fn open_or_rebuild(
        owner_root: impl AsRef<Path>,
        accepted_vnext_root: [u8; 32],
        vault_source_root: [u8; 32],
        source_records: Vec<VaultSourceSnapshotRecord>,
    ) -> (Self, DerivedProjectionOpenState) {
        let owner_root = owner_root.as_ref();
        let snapshot_path = owner_root.join("retriever-index-v2.json");
        let source_root = retriever_source_root(accepted_vnext_root, vault_source_root);
        let expected =
            match KuRetriever::from_vault_records(source_records, RetrieverConfig::default()) {
                Ok(expected) => expected,
                Err(error) => {
                    return Self::unavailable(
                        owner_root,
                        accepted_vnext_root,
                        format!("VAULT_SOURCE_SNAPSHOT_INVALID: {error}"),
                    );
                }
            };
        let expected_envelope = expected.envelope(source_root);

        let (retriever, state) = match KuRetriever::load_for_source_root(
            &snapshot_path,
            source_root,
        )
        .and_then(|envelope| verify_exact(envelope, &expected_envelope))
        {
            Ok(loaded) => (
                KuRetriever::from_envelope(loaded, RetrieverConfig::default())
                    .expect("verified envelope remains constructible"),
                DerivedProjectionOpenState::Ready,
            ),
            Err(error) => {
                if snapshot_path.exists() {
                    let _ = quarantine_corrupt_snapshot(&snapshot_path);
                }
                match expected.save_atomic(&snapshot_path, source_root) {
                    Ok(()) => (expected, DerivedProjectionOpenState::Rebuilt),
                    Err(publish_error) => (
                        KuRetriever::default(),
                        DerivedProjectionOpenState::Degraded {
                            reason: format!(
                                "projection recovery after {error}; publication failed: {publish_error}"
                            ),
                        },
                    ),
                }
            }
        };

        (
            Self {
                snapshot_path,
                retriever: Arc::new(RwLock::new(retriever)),
                source_root,
            },
            state,
        )
    }

    pub fn retriever(&self) -> Arc<RwLock<KuRetriever>> {
        self.retriever.clone()
    }

    pub fn source_root(&self) -> [u8; 32] {
        self.source_root
    }

    pub fn snapshot_path(&self) -> &Path {
        &self.snapshot_path
    }

    pub fn verify_generation(
        owner_root: impl AsRef<Path>,
        accepted_vnext_root: [u8; 32],
        vault_source_root: [u8; 32],
        source_records: Vec<VaultSourceSnapshotRecord>,
    ) -> Result<(), RetrieverError> {
        let source_root = retriever_source_root(accepted_vnext_root, vault_source_root);
        let expected = KuRetriever::from_vault_records(source_records, RetrieverConfig::default())?;
        let expected_envelope = expected.envelope(source_root);
        let loaded = KuRetriever::load_for_source_root(
            &owner_root.as_ref().join("retriever-index-v2.json"),
            source_root,
        )?;
        verify_exact(loaded, &expected_envelope).map(|_| ())
    }
}

fn verify_exact(
    loaded: RetrieverIndexEnvelope,
    expected: &RetrieverIndexEnvelope,
) -> Result<RetrieverIndexEnvelope, RetrieverError> {
    if loaded != *expected {
        return Err(RetrieverError::EntriesRootMismatch);
    }
    Ok(loaded)
}

fn quarantine_corrupt_snapshot(path: &Path) -> std::io::Result<()> {
    let bytes = std::fs::read(path).unwrap_or_default();
    let digest = blake3::hash(&bytes);
    let suffix = digest
        .to_hex()
        .as_str()
        .get(..16)
        .unwrap_or("unreadable")
        .to_owned();
    let quarantine = path.with_extension(format!("corrupt-{suffix}"));
    std::fs::rename(path, quarantine)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ku_core::foundation::{
        LocalSourceTextRecordV1, ObjectReference, VaultSourceSnapshotRecord,
    };

    fn source(byte: u8, text: &str) -> VaultSourceSnapshotRecord {
        let record =
            LocalSourceTextRecordV1::new(ObjectReference::new(3, [byte; 32]), text.to_owned())
                .unwrap();
        let (_, source_record) = record.encode().unwrap();
        VaultSourceSnapshotRecord {
            subject: record.subject,
            source_record,
            source_digest: record.source_digest,
            source_text: record.source_text,
        }
    }

    #[test]
    fn truncated_snapshot_is_quarantined_and_rebuilt() {
        let directory = tempfile::tempdir().unwrap();
        let (first, state) = RetrieverProjectionService::open_or_rebuild(
            directory.path(),
            [1; 32],
            [2; 32],
            vec![source(7, "exact")],
        );
        assert_eq!(state, DerivedProjectionOpenState::Rebuilt);
        std::fs::write(first.snapshot_path(), b"{").unwrap();
        let (reopened, state) = RetrieverProjectionService::open_or_rebuild(
            directory.path(),
            [1; 32],
            [2; 32],
            vec![source(7, "exact")],
        );
        assert_eq!(state, DerivedProjectionOpenState::Rebuilt);
        assert_eq!(
            reopened.retriever().read().unwrap().subjects(),
            vec![ObjectReference::new(3, [7; 32])]
        );
    }

    #[test]
    fn unwritable_projection_owner_is_typed_degraded_not_startup_failure() {
        let directory = tempfile::tempdir().unwrap();
        let owner_file = directory.path().join("not-a-directory");
        std::fs::write(&owner_file, b"occupied").unwrap();
        let (service, state) = RetrieverProjectionService::open_or_rebuild(
            &owner_file,
            [1; 32],
            [2; 32],
            vec![source(3, "durable")],
        );
        assert!(matches!(state, DerivedProjectionOpenState::Degraded { .. }));
        assert_eq!(service.retriever().read().unwrap().index_size(), 0);
    }
}
