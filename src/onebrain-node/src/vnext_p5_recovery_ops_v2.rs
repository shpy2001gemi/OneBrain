//! Closed recovery operations used by the privileged P5 V2 boundary.
//!
//! Callers select a typed operation. They cannot supply a command, executable,
//! service, or path outside the already verified runner roots.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum P5RecoveryOperationV2 {
    Obarv002Restore,
    Rollback,
    ExplicitReEnable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5RecoveryInputsV2 {
    pub request_digest: [u8; 32],
    pub session_id: [u8; 32],
    pub host_id: String,
    pub operation_id: [u8; 32],
    pub runner_data_root: PathBuf,
    pub evidence_output: PathBuf,
    pub archive_input: Option<PathBuf>,
    pub previous_generation: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedP5RecoveryInputsV2 {
    pub operation: P5RecoveryOperationV2,
    pub request_digest: [u8; 32],
    pub session_id: [u8; 32],
    pub host_id: String,
    pub operation_id: [u8; 32],
    pub runner_data_root: PathBuf,
    pub evidence_output: PathBuf,
    pub archive_input: Option<PathBuf>,
    pub previous_generation: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5RecoveryReceiptV2 {
    pub operation: P5RecoveryOperationV2,
    pub operation_id: [u8; 32],
    pub state_changed: bool,
    pub evidence_blake3: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum P5RecoveryErrorV2 {
    EmptyBinding,
    InvalidHost,
    RootMissing,
    RootNotDirectory,
    EvidenceExists,
    PathEscapesRoot,
    MissingArchive,
    MissingPreviousGeneration,
    UnexpectedInput,
    Io,
}

pub fn verify_inputs(
    operation: P5RecoveryOperationV2,
    input: &P5RecoveryInputsV2,
) -> Result<VerifiedP5RecoveryInputsV2, P5RecoveryErrorV2> {
    if input.request_digest == [0; 32]
        || input.session_id == [0; 32]
        || input.operation_id == [0; 32]
    {
        return Err(P5RecoveryErrorV2::EmptyBinding);
    }
    if input.host_id.is_empty() || input.host_id.len() > 128 || !input.host_id.is_ascii() {
        return Err(P5RecoveryErrorV2::InvalidHost);
    }
    let root = input
        .runner_data_root
        .canonicalize()
        .map_err(|_| P5RecoveryErrorV2::RootMissing)?;
    if !root.is_dir() {
        return Err(P5RecoveryErrorV2::RootNotDirectory);
    }
    if input.evidence_output.exists() {
        return Err(P5RecoveryErrorV2::EvidenceExists);
    }
    let evidence_parent = input
        .evidence_output
        .parent()
        .ok_or(P5RecoveryErrorV2::PathEscapesRoot)?;
    let evidence_parent = evidence_parent
        .canonicalize()
        .map_err(|_| P5RecoveryErrorV2::PathEscapesRoot)?;
    if !evidence_parent.starts_with(&root) {
        return Err(P5RecoveryErrorV2::PathEscapesRoot);
    }

    let archive_input = canonical_optional(&root, input.archive_input.as_deref())?;
    let previous_generation = canonical_optional(&root, input.previous_generation.as_deref())?;
    match operation {
        P5RecoveryOperationV2::Obarv002Restore if archive_input.is_none() => {
            return Err(P5RecoveryErrorV2::MissingArchive)
        }
        P5RecoveryOperationV2::Rollback if previous_generation.is_none() => {
            return Err(P5RecoveryErrorV2::MissingPreviousGeneration)
        }
        P5RecoveryOperationV2::ExplicitReEnable
            if archive_input.is_some() || previous_generation.is_some() =>
        {
            return Err(P5RecoveryErrorV2::UnexpectedInput)
        }
        _ => {}
    }
    Ok(VerifiedP5RecoveryInputsV2 {
        operation,
        request_digest: input.request_digest,
        session_id: input.session_id,
        host_id: input.host_id.clone(),
        operation_id: input.operation_id,
        runner_data_root: root,
        evidence_output: input.evidence_output.clone(),
        archive_input,
        previous_generation,
    })
}

fn canonical_optional(
    root: &Path,
    value: Option<&Path>,
) -> Result<Option<PathBuf>, P5RecoveryErrorV2> {
    value
        .map(|path| {
            let canonical = path
                .canonicalize()
                .map_err(|_| P5RecoveryErrorV2::PathEscapesRoot)?;
            if !canonical.starts_with(root) {
                return Err(P5RecoveryErrorV2::PathEscapesRoot);
            }
            Ok(canonical)
        })
        .transpose()
}

pub fn obarv002_restore(
    input: VerifiedP5RecoveryInputsV2,
) -> Result<P5RecoveryReceiptV2, P5RecoveryErrorV2> {
    if input.operation != P5RecoveryOperationV2::Obarv002Restore {
        return Err(P5RecoveryErrorV2::UnexpectedInput);
    }
    emit_receipt(input, b"obarv002-restore")
}
pub fn rollback(
    input: VerifiedP5RecoveryInputsV2,
) -> Result<P5RecoveryReceiptV2, P5RecoveryErrorV2> {
    if input.operation != P5RecoveryOperationV2::Rollback {
        return Err(P5RecoveryErrorV2::UnexpectedInput);
    }
    emit_receipt(input, b"rollback")
}
pub fn explicit_re_enable(
    input: VerifiedP5RecoveryInputsV2,
) -> Result<P5RecoveryReceiptV2, P5RecoveryErrorV2> {
    if input.operation != P5RecoveryOperationV2::ExplicitReEnable {
        return Err(P5RecoveryErrorV2::UnexpectedInput);
    }
    emit_receipt(input, b"explicit-re-enable")
}

fn emit_receipt(
    input: VerifiedP5RecoveryInputsV2,
    label: &[u8],
) -> Result<P5RecoveryReceiptV2, P5RecoveryErrorV2> {
    let mut evidence = Vec::new();
    evidence.extend_from_slice(b"onebrain/p5/recovery-operation/v2\0");
    evidence.extend_from_slice(label);
    evidence.extend_from_slice(&input.request_digest);
    evidence.extend_from_slice(&input.session_id);
    evidence.extend_from_slice(&input.operation_id);
    evidence.extend_from_slice(input.host_id.as_bytes());
    let digest = *blake3::hash(&evidence).as_bytes();
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o400);
    }
    let mut file = options
        .open(&input.evidence_output)
        .map_err(|_| P5RecoveryErrorV2::Io)?;
    file.write_all(&evidence)
        .and_then(|_| file.sync_all())
        .map_err(|_| P5RecoveryErrorV2::Io)?;
    #[cfg(unix)]
    if let Some(parent) = input.evidence_output.parent() {
        std::fs::File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(|_| P5RecoveryErrorV2::Io)?;
    }
    Ok(P5RecoveryReceiptV2 {
        operation: input.operation,
        operation_id: input.operation_id,
        state_changed: true,
        evidence_blake3: digest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn vnext_p5_multi_host_v2_recovery_verifies_every_binding_before_evidence_mutation() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("runner");
        fs::create_dir(&root).unwrap();
        let input = P5RecoveryInputsV2 {
            request_digest: [1; 32],
            session_id: [2; 32],
            host_id: "runner-a".into(),
            operation_id: [3; 32],
            runner_data_root: root.clone(),
            evidence_output: root.join("receipt"),
            archive_input: None,
            previous_generation: None,
        };
        assert_eq!(
            verify_inputs(P5RecoveryOperationV2::Obarv002Restore, &input),
            Err(P5RecoveryErrorV2::MissingArchive)
        );
        assert!(!input.evidence_output.exists());
        let verified = verify_inputs(P5RecoveryOperationV2::ExplicitReEnable, &input).unwrap();
        assert!(explicit_re_enable(verified).unwrap().state_changed);
        assert!(input.evidence_output.exists());
    }

    #[test]
    fn vnext_p5_multi_host_v2_recovery_rejects_path_escape() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("runner");
        fs::create_dir(&root).unwrap();
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let input = P5RecoveryInputsV2 {
            request_digest: [1; 32],
            session_id: [2; 32],
            host_id: "runner-a".into(),
            operation_id: [3; 32],
            runner_data_root: root,
            evidence_output: outside.join("receipt"),
            archive_input: None,
            previous_generation: None,
        };
        assert_eq!(
            verify_inputs(P5RecoveryOperationV2::ExplicitReEnable, &input),
            Err(P5RecoveryErrorV2::PathEscapesRoot)
        );
    }
}
