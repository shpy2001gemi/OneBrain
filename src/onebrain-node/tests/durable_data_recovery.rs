use std::sync::Arc;

use ku_core::foundation::{
    InMemoryVerifiedBackend, LocalSourceTextRecordV1, ObjectReference, PrivateVault, VaultKey,
    VaultSourceSnapshotPort,
};
use onebrain_node::{
    BootstrapDatasetPathResolver, DerivedProjectionOpenState, RetrieverProjectionService,
    SourceCaptureError, SourceCaptureRecoveryState, SourceCaptureTransactionStore,
};

#[test]
fn encrypted_source_intent_finishes_vault_binding_without_plaintext_journal() {
    let directory = tempfile::tempdir().unwrap();
    let resolver = Arc::new(BootstrapDatasetPathResolver::new(directory.path()).unwrap());
    let vault = PrivateVault::new(
        InMemoryVerifiedBackend::default(),
        VaultKey::from_bytes([0x41; 32]),
    );
    let transactions =
        SourceCaptureTransactionStore::new(resolver, directory.path().join("vault-staging"));
    let subject = ObjectReference::new(4, [0x11; 32]);
    let source = "bí mật chính xác 👩🏽‍💻";
    let intent = transactions
        .prepare(&vault, subject.clone(), [0x22; 32], source.to_owned())
        .unwrap();

    for entry in walk_files(directory.path()) {
        let bytes = std::fs::read(entry).unwrap();
        assert!(!bytes
            .windows(source.len())
            .any(|window| window == source.as_bytes()));
    }

    let outcomes = transactions
        .reconcile(&vault, |candidate, digest| {
            candidate == &subject && digest == &[0x22; 32]
        })
        .unwrap();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].0, intent);
    assert_eq!(
        outcomes[0].1,
        SourceCaptureRecoveryState::FinishVaultBinding
    );
    let snapshot = vault.source_snapshot().unwrap();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].subject, subject);
    assert_eq!(snapshot[0].source_text.as_str(), source);
}

#[test]
fn wrong_key_and_orphan_source_fail_closed() {
    let directory = tempfile::tempdir().unwrap();
    let resolver = Arc::new(BootstrapDatasetPathResolver::new(directory.path()).unwrap());
    let vault = PrivateVault::new(
        InMemoryVerifiedBackend::default(),
        VaultKey::from_bytes([0x51; 32]),
    );
    let transactions = SourceCaptureTransactionStore::new(
        resolver.clone(),
        directory.path().join("vault-staging"),
    );
    transactions
        .prepare(
            &vault,
            ObjectReference::new(1, [9; 32]),
            [7; 32],
            "never reconstruct me".into(),
        )
        .unwrap();

    let wrong_key_vault = PrivateVault::new(
        InMemoryVerifiedBackend::default(),
        VaultKey::from_bytes([0x52; 32]),
    );
    assert!(matches!(
        transactions.reconcile(&wrong_key_vault, |_, _| true),
        Err(SourceCaptureError::AuthenticationFailed)
    ));

    let outcomes = transactions.reconcile(&vault, |_, _| false).unwrap();
    assert_eq!(
        outcomes[0].1,
        SourceCaptureRecoveryState::QuarantineOrphanSource
    );
    assert!(vault.source_snapshot().unwrap().is_empty());
}

#[test]
fn ciphertext_tamper_exposes_incomplete_instead_of_inventing_source() {
    let directory = tempfile::tempdir().unwrap();
    let resolver = Arc::new(BootstrapDatasetPathResolver::new(directory.path()).unwrap());
    let vault = PrivateVault::new(
        InMemoryVerifiedBackend::default(),
        VaultKey::from_bytes([0x56; 32]),
    );
    let staging = directory.path().join("vault-staging");
    let transactions = SourceCaptureTransactionStore::new(resolver, &staging);
    transactions
        .prepare(
            &vault,
            ObjectReference::new(1, [0x44; 32]),
            [0x45; 32],
            "do not reconstruct".into(),
        )
        .unwrap();
    let stage_path = std::fs::read_dir(&staging)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut ciphertext = std::fs::read(&stage_path).unwrap();
    *ciphertext.last_mut().unwrap() ^= 1;
    std::fs::write(stage_path, ciphertext).unwrap();

    let outcomes = transactions.reconcile(&vault, |_, _| true).unwrap();
    assert_eq!(
        outcomes[0].1,
        SourceCaptureRecoveryState::SourceCaptureIncomplete
    );
    assert!(vault.source_snapshot().unwrap().is_empty());
}

#[test]
fn truncated_retriever_snapshot_rebuilds_from_vault_with_same_subject_set() {
    let directory = tempfile::tempdir().unwrap();
    let vault = PrivateVault::new(
        InMemoryVerifiedBackend::default(),
        VaultKey::from_bytes([0x61; 32]),
    );
    let subject = ObjectReference::new(8, [0x33; 32]);
    let record = LocalSourceTextRecordV1::new(subject.clone(), "nguồn bền vững".into()).unwrap();
    let (source_cid, _) = vault.put_source_text(&record).unwrap();
    let (_, recomputed_cid) = record.encode().unwrap();
    assert_eq!(source_cid, recomputed_cid);

    let records = vault.source_snapshot().unwrap();
    let vault_root = vault.vault_source_root().unwrap();
    let projection_root = directory.path().join("projection");
    let (first, state) = RetrieverProjectionService::open_or_rebuild(
        &projection_root,
        [0x71; 32],
        vault_root,
        records.clone(),
    );
    assert_eq!(state, DerivedProjectionOpenState::Rebuilt);
    std::fs::write(first.snapshot_path(), b"{\"truncated\":").unwrap();

    let (reopened, state) = RetrieverProjectionService::open_or_rebuild(
        &projection_root,
        [0x71; 32],
        vault_root,
        records,
    );
    assert_eq!(state, DerivedProjectionOpenState::Rebuilt);
    assert_eq!(
        reopened.retriever().read().unwrap().subjects(),
        vec![subject]
    );
}

fn walk_files(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                pending.push(entry.path());
            } else {
                files.push(entry.path());
            }
        }
    }
    files
}

#[cfg(feature = "vnext-crash-harness")]
const CHILD_ENV: &str = "ONEBRAIN_SOURCE_CAPTURE_CHILD";
#[cfg(feature = "vnext-crash-harness")]
const ROOT_ENV: &str = "ONEBRAIN_SOURCE_CAPTURE_ROOT";
#[cfg(feature = "vnext-crash-harness")]
const CHILD_TEST: &str = "source_capture_transaction_worker";

#[cfg(feature = "vnext-crash-harness")]
#[test]
fn source_capture_transaction_worker() {
    if std::env::var_os(CHILD_ENV).is_none() {
        return;
    }
    let root = std::path::PathBuf::from(std::env::var_os(ROOT_ENV).unwrap());
    let resolver = Arc::new(BootstrapDatasetPathResolver::new(&root).unwrap());
    let vault = PrivateVault::new(
        InMemoryVerifiedBackend::default(),
        VaultKey::from_bytes([0x77; 32]),
    );
    SourceCaptureTransactionStore::new(resolver, root.join("vault-staging"))
        .prepare(
            &vault,
            ObjectReference::new(2, [0x78; 32]),
            [0x79; 32],
            "child source plaintext".into(),
        )
        .unwrap();
}

#[cfg(feature = "vnext-crash-harness")]
#[test]
fn source_capture_child_process_kill_matrix_reconciles_every_phase() {
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    for phase in ku_core::foundation::dr_m5_failpoint::FAILPOINT_PHASES {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("dataset");
        let marker = temporary.path().join("marker.json");
        let token = format!("source-capture-{phase}-{}", std::process::id());
        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg(CHILD_TEST)
            .arg("--nocapture")
            .env(CHILD_ENV, "1")
            .env(ROOT_ENV, &root)
            .env(ku_core::foundation::dr_m5_failpoint::ENABLE_ENV, "1")
            .env(
                ku_core::foundation::dr_m5_failpoint::FAILPOINT_ENV,
                format!("TX-SOURCE-001:{phase}"),
            )
            .env(ku_core::foundation::dr_m5_failpoint::MARKER_ENV, &marker)
            .env(ku_core::foundation::dr_m5_failpoint::TOKEN_ENV, &token)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !marker.exists() && Instant::now() < deadline {
            assert!(child.try_wait().unwrap().is_none());
            thread::sleep(Duration::from_millis(10));
        }
        assert!(marker.exists(), "phase {phase} did not arm");
        child.kill().unwrap();
        assert!(!child.wait().unwrap().success());

        let resolver = Arc::new(BootstrapDatasetPathResolver::new(&root).unwrap());
        let vault = PrivateVault::new(
            InMemoryVerifiedBackend::default(),
            VaultKey::from_bytes([0x77; 32]),
        );
        SourceCaptureTransactionStore::new(resolver, root.join("vault-staging"))
            .reconcile(&vault, |_, _| false)
            .unwrap();
        let leftovers = walk_files(&root)
            .into_iter()
            .filter(|path| {
                matches!(
                    path.extension().and_then(|value| value.to_str()),
                    Some("json" | "preparing" | "stage")
                )
            })
            .collect::<Vec<_>>();
        assert!(leftovers.is_empty(), "phase {phase}: {leftovers:?}");
    }
}
