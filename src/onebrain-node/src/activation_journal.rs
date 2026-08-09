use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const POINTER_PROFILE: &str = "onebrain/base-dataset-pointer/1";
const JOURNAL_PROFILE: &str = "onebrain/base-activation-journal/1";
const RECEIPT_PROFILE: &str = "onebrain/base-generation-receipt/1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivationPhase {
    Prepared,
    PointerPublished,
    Complete,
    RolledBack,
    UnknownOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivationOperationContext {
    pub principal_digest: [u8; 32],
    pub process_generation: [u8; 32],
    pub migration_vector_id: Option<String>,
    pub migration_vector_blake3: Option<[u8; 32]>,
    pub migration_trust_policy_digest: Option<[u8; 32]>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetGenerationReceipt {
    pub operation_id: [u8; 32],
    pub idempotency_key: [u8; 32],
    pub old_generation_root: [u8; 32],
    pub new_generation_root: [u8; 32],
    pub generation_sequence: u64,
    pub phase: ActivationPhase,
    #[serde(default)]
    pub operation_context: Option<ActivationOperationContext>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ActivationJournalRecord {
    pub revision: u64,
    pub pointer_sequence: u64,
    pub operation_id: [u8; 32],
    pub idempotency_key: [u8; 32],
    pub old_generation_root: [u8; 32],
    pub new_generation_root: [u8; 32],
    pub phase: ActivationPhase,
    #[serde(default)]
    pub operation_context: Option<ActivationOperationContext>,
    pub receipt: Option<DatasetGenerationReceipt>,
}

#[derive(Serialize, Deserialize)]
struct PointerPayload {
    profile: String,
    sequence: u64,
    generation_root: [u8; 32],
}

#[derive(Serialize, Deserialize)]
struct PointerEnvelope {
    payload: PointerPayload,
    checksum: [u8; 32],
}

#[derive(Serialize, Deserialize)]
struct JournalPayload {
    profile: String,
    record: ActivationJournalRecord,
}

#[derive(Serialize, Deserialize)]
struct JournalEnvelope {
    payload: JournalPayload,
    checksum: [u8; 32],
}

#[derive(Serialize, Deserialize)]
struct ReceiptPayload {
    profile: String,
    receipt: DatasetGenerationReceipt,
}

#[derive(Serialize, Deserialize)]
struct ReceiptEnvelope {
    payload: ReceiptPayload,
    checksum: [u8; 32],
}

pub(crate) fn read_current_pointer(control: &Path) -> Result<Option<(u64, [u8; 32])>, String> {
    let mut valid = Vec::new();
    for name in ["current.a.json", "current.b.json"] {
        let path = control.join(name);
        if !path.exists() {
            continue;
        }
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let envelope: PointerEnvelope = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if envelope.payload.profile != POINTER_PROFILE
            || checksum(POINTER_PROFILE, &envelope.payload)? != envelope.checksum
        {
            continue;
        }
        valid.push((envelope.payload.sequence, envelope.payload.generation_root));
    }
    valid.sort_by_key(|value| value.0);
    Ok(valid.pop())
}

pub(crate) fn write_current_pointer(
    control: &Path,
    sequence: u64,
    generation_root: [u8; 32],
) -> Result<(), String> {
    let payload = PointerPayload {
        profile: POINTER_PROFILE.to_string(),
        sequence,
        generation_root,
    };
    let envelope = PointerEnvelope {
        checksum: checksum(POINTER_PROFILE, &payload)?,
        payload,
    };
    write_slot(
        &control.join(if sequence % 2 == 0 {
            "current.a.json"
        } else {
            "current.b.json"
        }),
        &envelope,
    )
}

pub(crate) fn read_latest_journal(
    control: &Path,
) -> Result<Option<ActivationJournalRecord>, String> {
    let mut valid = Vec::new();
    for name in ["activation.a.json", "activation.b.json"] {
        let path = control.join(name);
        if !path.exists() {
            continue;
        }
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let envelope: JournalEnvelope = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if envelope.payload.profile != JOURNAL_PROFILE
            || checksum(JOURNAL_PROFILE, &envelope.payload)? != envelope.checksum
        {
            continue;
        }
        valid.push(envelope.payload.record);
    }
    valid.sort_by_key(|record| record.revision);
    Ok(valid.pop())
}

pub(crate) fn write_journal(
    control: &Path,
    record: &ActivationJournalRecord,
) -> Result<(), String> {
    let payload = JournalPayload {
        profile: JOURNAL_PROFILE.to_string(),
        record: record.clone(),
    };
    let envelope = JournalEnvelope {
        checksum: checksum(JOURNAL_PROFILE, &payload)?,
        payload,
    };
    write_slot(
        &control.join(if record.revision % 2 == 0 {
            "activation.a.json"
        } else {
            "activation.b.json"
        }),
        &envelope,
    )
}

pub(crate) fn write_receipt(
    control: &Path,
    receipt: &DatasetGenerationReceipt,
) -> Result<(), String> {
    let directory = control.join("receipts");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let path = receipt_path(&directory, receipt.operation_id);
    let payload = ReceiptPayload {
        profile: RECEIPT_PROFILE.to_string(),
        receipt: receipt.clone(),
    };
    let envelope = ReceiptEnvelope {
        checksum: checksum(RECEIPT_PROFILE, &payload)?,
        payload,
    };
    if path.exists() {
        let existing = read_receipt(control, receipt.operation_id)?
            .ok_or_else(|| "CORRUPT_EXISTING_RECEIPT".to_string())?;
        return if existing == *receipt {
            write_idempotency_receipt(control, &envelope, receipt)
        } else {
            Err("RESTORE_OPERATION_CONFLICT".into())
        };
    }
    write_new(&path, &envelope)?;
    write_idempotency_receipt(control, &envelope, receipt)
}

pub(crate) fn read_idempotency_receipt(
    control: &Path,
    idempotency_key: [u8; 32],
) -> Result<Option<DatasetGenerationReceipt>, String> {
    let path = control
        .join("receipts/by-idempotency")
        .join(format!("{}.json", hex(&idempotency_key)));
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let envelope: ReceiptEnvelope =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if envelope.payload.profile != RECEIPT_PROFILE
        || checksum(RECEIPT_PROFILE, &envelope.payload)? != envelope.checksum
        || envelope.payload.receipt.idempotency_key != idempotency_key
    {
        return Err("CORRUPT_IDEMPOTENCY_RECEIPT".into());
    }
    Ok(Some(envelope.payload.receipt))
}

pub(crate) fn read_receipt(
    control: &Path,
    operation_id: [u8; 32],
) -> Result<Option<DatasetGenerationReceipt>, String> {
    let path = receipt_path(&control.join("receipts"), operation_id);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let envelope: ReceiptEnvelope =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if envelope.payload.profile != RECEIPT_PROFILE
        || checksum(RECEIPT_PROFILE, &envelope.payload)? != envelope.checksum
        || envelope.payload.receipt.operation_id != operation_id
    {
        return Err("CORRUPT_RECEIPT".into());
    }
    Ok(Some(envelope.payload.receipt))
}

fn receipt_path(directory: &Path, operation_id: [u8; 32]) -> PathBuf {
    directory.join(format!("{}.json", hex(&operation_id)))
}

fn write_idempotency_receipt(
    control: &Path,
    envelope: &ReceiptEnvelope,
    receipt: &DatasetGenerationReceipt,
) -> Result<(), String> {
    let directory = control.join("receipts/by-idempotency");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let path = directory.join(format!("{}.json", hex(&receipt.idempotency_key)));
    if path.exists() {
        let existing = read_idempotency_receipt(control, receipt.idempotency_key)?
            .ok_or_else(|| "CORRUPT_IDEMPOTENCY_RECEIPT".to_string())?;
        return if existing == *receipt {
            Ok(())
        } else {
            Err("RESTORE_IDEMPOTENCY_CONFLICT".into())
        };
    }
    write_new(&path, envelope)
}

fn checksum<T: Serialize>(domain: &str, value: &T) -> Result<[u8; 32], String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    let mut hasher = blake3::Hasher::new_derive_key(domain);
    hasher.update(&bytes);
    Ok(*hasher.finalize().as_bytes())
}

fn write_slot(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    let mut output = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    output
        .write_all(&bytes)
        .map_err(|error| error.to_string())?;
    output.sync_all().map_err(|error| error.to_string())
}

fn write_new(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    output
        .write_all(&bytes)
        .map_err(|error| error.to_string())?;
    output.sync_all().map_err(|error| error.to_string())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
