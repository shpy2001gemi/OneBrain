//! Host transfer for Base archives through the scoped management facade.
//!
//! The shared core ([`create_base_archive`], [`restore_base_archive`],
//! [`reconcile_restored_archive`]) runs the reserved/prepared/confirmed Base
//! archive operation over an already opened management session and streams
//! bytes through a host-provided [`ArchiveByteSink`] / [`ArchiveByteSource`].
//! The CLI `backup`/`restore` file functions below and the API
//! `/api/backup` / `/api/restore` byte projection both use it; each host only
//! owns how it opens/closes management and where the bytes live. Nothing here
//! reads or writes the dataset directly.
//! ARCH-13 slice 16: `tests/base_gate_integration.rs` round-trips the file
//! facade against a real installed Base runtime.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use onebrain_base_contract::{
    ArchiveCapabilityHandleV1, ArchiveChunkV1, ArchiveCredentialKindV1, ArchiveSinkBeginV1,
    ArchiveSinkHandleV1, ArchiveSinkReadV1, ArchiveSourceBeginV1, ArchiveSourceHandleV1,
    ArchiveSourcePushV1, BaseCapabilityRequirements, BaseCapabilitySet, BaseCommandV1,
    BaseConfirmRequestV1, BaseErrorCodeV1, BaseIdempotencyKey, BaseManagementRequestV1,
    BaseOperationId, BaseOperationKindV1, BasePrepareRequestV1, BaseRequestV1,
    BoundedSecretIngressV1, CreateArchiveCommandV1, ResourceBudgetV1, RestoreArchiveCommandV1,
    MAX_BASE_ARCHIVE_DATASET_BYTES,
};

use crate::archive_capabilities::DEFAULT_ARCHIVE_SPOOL_BYTES;
use crate::base_runtime::{
    BaseManagementResponseV1, BaseManagementScope, BaseManagementServices, BaseNegotiationRequest,
    BaseResponseV1, BaseServiceError, BaseServices,
};
use crate::node::OneBrainNode;

/// Largest archive the file transfer accepts in either direction.
pub const BASE_ARCHIVE_FILE_MAX_BYTES: u64 = DEFAULT_ARCHIVE_SPOOL_BYTES;
/// Chunk size used for archive sink reads and source pushes.
pub const BASE_ARCHIVE_CHUNK_BYTES: usize = 1_048_576;
const GRANT_LIFETIME: Duration = Duration::from_secs(300);

/// Destination for bytes read from a committed Base archive sink.
pub trait ArchiveByteSink {
    /// Append one contiguous chunk.
    fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), BaseServiceError>;
    /// Called after the last chunk and before the Base sink is committed.
    fn finish(&mut self) -> Result<(), BaseServiceError> {
        Ok(())
    }
}

/// Source of bytes pushed into a Base archive source.
pub trait ArchiveByteSource {
    /// Fill up to `buffer.len()` bytes; `0` means the end of the archive.
    fn read_chunk(&mut self, buffer: &mut [u8]) -> Result<usize, BaseServiceError>;
}

/// In-memory sink bounded to `max_bytes` (API byte projection).
pub struct BoundedVecSink {
    pub bytes: Vec<u8>,
    pub max_bytes: u64,
}

impl ArchiveByteSink for BoundedVecSink {
    fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), BaseServiceError> {
        let next = self.bytes.len().checked_add(bytes.len()).ok_or_else(|| {
            BaseServiceError::new(
                BaseErrorCodeV1::ResourceExhausted,
                "archive_response_size_overflow",
            )
        })?;
        if next as u64 > self.max_bytes {
            return Err(BaseServiceError::new(
                BaseErrorCodeV1::ResourceExhausted,
                "archive_response_exceeds_bound",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
}

/// In-memory source over an archive already held by the host.
pub struct SliceSource<'a> {
    pub bytes: &'a [u8],
}

impl ArchiveByteSource for SliceSource<'_> {
    fn read_chunk(&mut self, buffer: &mut [u8]) -> Result<usize, BaseServiceError> {
        let count = buffer.len().min(self.bytes.len());
        buffer[..count].copy_from_slice(&self.bytes[..count]);
        self.bytes = &self.bytes[count..];
        Ok(count)
    }
}

fn internal(reason: &'static str) -> BaseServiceError {
    BaseServiceError::new(BaseErrorCodeV1::InternalError, reason)
}

fn idempotency_key() -> Result<BaseIdempotencyKey, BaseServiceError> {
    let mut key = [0; 32];
    getrandom::fill(&mut key).map_err(|_| internal("archive_entropy_unavailable"))?;
    if key == [0; 32] {
        return Err(internal("archive_entropy_returned_zero"));
    }
    Ok(BaseIdempotencyKey(key))
}

fn budget() -> Result<ResourceBudgetV1, BaseServiceError> {
    ResourceBudgetV1::try_new(1, 1_048_576, 1_000_000)
        .map_err(|_| internal("compiled_archive_budget_is_invalid"))
}

async fn register_password(
    management: &BaseManagementServices,
    password: Vec<u8>,
) -> Result<onebrain_base_contract::ArchiveSecretHandleV1, BaseServiceError> {
    match management
        .invoke(BaseManagementRequestV1::ArchiveSecretRegister(
            BoundedSecretIngressV1::try_new(ArchiveCredentialKindV1::Password, password).map_err(
                |_| {
                    BaseServiceError::new(BaseErrorCodeV1::InvalidRequest, "invalid_archive_secret")
                },
            )?,
        ))
        .await?
    {
        BaseManagementResponseV1::ArchiveSecret(value) => Ok(value),
        _ => Err(internal("unexpected_archive_secret_response")),
    }
}

/// Create an encrypted Base archive over an open management session holding
/// `ArchiveSink` + `ArchiveSecret` scopes; returns the archive size. The sink
/// is committed only after `sink.finish()` succeeds. The caller closes
/// `management`.
pub async fn create_base_archive<S: ArchiveByteSink + Send>(
    services: &BaseServices,
    management: &BaseManagementServices,
    password: Vec<u8>,
    max_total_bytes: u64,
    sink: &mut S,
) -> Result<u64, BaseServiceError> {
    let reservation = match services
        .invoke(BaseRequestV1::ReserveOperation(
            BaseOperationKindV1::CreateArchive,
        ))
        .await?
    {
        BaseResponseV1::Reserved(value) => value,
        _ => return Err(internal("unexpected_archive_reserve_response")),
    };
    let handle = match management
        .invoke(BaseManagementRequestV1::ArchiveSinkBegin(
            ArchiveSinkBeginV1 {
                reservation_id: reservation,
                max_total_bytes,
            },
        ))
        .await?
    {
        BaseManagementResponseV1::ArchiveSink(value) => value,
        _ => return Err(internal("unexpected_archive_sink_response")),
    };
    let sink_id = *handle.as_bytes();
    let secret = register_password(management, password).await?;
    let prepared = match services
        .invoke(BaseRequestV1::Prepare(BasePrepareRequestV1 {
            reservation_id: reservation,
            command: BaseCommandV1::CreateArchive(CreateArchiveCommandV1 {
                sink: ArchiveSinkHandleV1::from_opaque_bytes(sink_id),
                secret,
                budget: budget()?,
            }),
        }))
        .await?
    {
        BaseResponseV1::Prepared(value) => value,
        _ => return Err(internal("unexpected_archive_prepare_response")),
    };
    let receipt = match services
        .invoke(BaseRequestV1::Confirm(BaseConfirmRequestV1 {
            operation_id: prepared.operation_id,
            idempotency_key: idempotency_key()?,
        }))
        .await?
    {
        BaseResponseV1::Receipt(value) => value,
        _ => return Err(internal("unexpected_archive_confirm_response")),
    };
    if let Some(error) = receipt.error {
        return Err(BaseServiceError::new(error, "base_archive_create_failed"));
    }
    if receipt.result.len() != 64 || receipt.result[..32] != sink_id {
        return Err(BaseServiceError::new(
            BaseErrorCodeV1::CorruptState,
            "archive_create_receipt_binding_mismatch",
        ));
    }

    let mut offset = 0u64;
    loop {
        let (chunk_offset, bytes, eof) = match management
            .invoke(BaseManagementRequestV1::ArchiveSinkRead(
                ArchiveSinkReadV1 {
                    handle: ArchiveSinkHandleV1::from_opaque_bytes(sink_id),
                    offset,
                    max_bytes: BASE_ARCHIVE_CHUNK_BYTES as u32,
                },
            ))
            .await?
        {
            BaseManagementResponseV1::ArchiveChunk { offset, bytes, eof } => (offset, bytes, eof),
            _ => return Err(internal("unexpected_archive_chunk_response")),
        };
        if chunk_offset != offset || (bytes.is_empty() && !eof) {
            return Err(BaseServiceError::new(
                BaseErrorCodeV1::CorruptState,
                "archive_sink_non_contiguous",
            ));
        }
        sink.write_chunk(&bytes)?;
        offset = offset.checked_add(bytes.len() as u64).ok_or_else(|| {
            BaseServiceError::new(
                BaseErrorCodeV1::ResourceExhausted,
                "archive_response_size_overflow",
            )
        })?;
        if eof {
            break;
        }
    }
    sink.finish()?;
    match management
        .invoke(BaseManagementRequestV1::ArchiveSinkCommit(
            ArchiveCapabilityHandleV1::from_opaque_bytes(sink_id),
        ))
        .await?
    {
        BaseManagementResponseV1::CapabilityClosed => Ok(offset),
        _ => Err(internal("unexpected_archive_commit_response")),
    }
}

/// Upload, seal, prepare and confirm a Base archive restore of exactly
/// `total_bytes` over an open management session holding `ArchiveSource` +
/// `ArchiveSecret` scopes. Returns the confirmed operation; the caller closes
/// `management` and then calls [`reconcile_restored_archive`] on refreshed
/// services (activation switches the Base generation).
pub async fn restore_base_archive<S: ArchiveByteSource + Send>(
    services: &BaseServices,
    management: &BaseManagementServices,
    password: Vec<u8>,
    total_bytes: u64,
    source: &mut S,
) -> Result<BaseOperationId, BaseServiceError> {
    let reservation = match services
        .invoke(BaseRequestV1::ReserveOperation(
            BaseOperationKindV1::RestoreArchive,
        ))
        .await?
    {
        BaseResponseV1::Reserved(value) => value,
        _ => return Err(internal("unexpected_restore_reserve_response")),
    };
    let handle = match management
        .invoke(BaseManagementRequestV1::ArchiveSourceBegin(
            ArchiveSourceBeginV1 {
                reservation_id: reservation,
                declared_total_bytes: total_bytes,
            },
        ))
        .await?
    {
        BaseManagementResponseV1::ArchiveSource(value) => value,
        _ => return Err(internal("unexpected_archive_source_response")),
    };
    let source_id = *handle.as_bytes();
    let mut offset = 0u64;
    let mut buffer = vec![0; BASE_ARCHIVE_CHUNK_BYTES];
    loop {
        let read = source.read_chunk(&mut buffer)?;
        if read == 0 {
            break;
        }
        match management
            .invoke(BaseManagementRequestV1::ArchiveSourcePush(
                ArchiveSourcePushV1 {
                    handle: ArchiveSourceHandleV1::from_opaque_bytes(source_id),
                    offset,
                    chunk: ArchiveChunkV1::try_from_bytes(buffer[..read].to_vec())
                        .map_err(|_| internal("bounded_archive_chunk_is_invalid"))?,
                },
            ))
            .await?
        {
            BaseManagementResponseV1::ArchiveCapability(_) => {}
            _ => return Err(internal("unexpected_archive_source_push_response")),
        }
        offset = offset.checked_add(read as u64).ok_or_else(|| {
            BaseServiceError::new(
                BaseErrorCodeV1::ResourceExhausted,
                "archive_upload_offset_overflow",
            )
        })?;
    }
    if offset != total_bytes {
        return Err(BaseServiceError::new(
            BaseErrorCodeV1::InvalidRequest,
            "archive_changed_during_upload",
        ));
    }
    match management
        .invoke(BaseManagementRequestV1::ArchiveSourceSeal(
            ArchiveCapabilityHandleV1::from_opaque_bytes(source_id),
        ))
        .await?
    {
        BaseManagementResponseV1::ArchiveSource(sealed) if sealed.as_bytes() == &source_id => {}
        _ => return Err(internal("unexpected_archive_seal_response")),
    }
    let secret = register_password(management, password).await?;
    let prepared = match services
        .invoke(BaseRequestV1::Prepare(BasePrepareRequestV1 {
            reservation_id: reservation,
            command: BaseCommandV1::RestoreArchive(RestoreArchiveCommandV1 {
                source: ArchiveSourceHandleV1::from_opaque_bytes(source_id),
                secret,
                budget: budget()?,
            }),
        }))
        .await?
    {
        BaseResponseV1::Prepared(value) => value,
        _ => return Err(internal("unexpected_restore_prepare_response")),
    };
    let receipt = match services
        .invoke(BaseRequestV1::Confirm(BaseConfirmRequestV1 {
            operation_id: prepared.operation_id,
            idempotency_key: idempotency_key()?,
        }))
        .await?
    {
        BaseResponseV1::Receipt(value) => value,
        _ => return Err(internal("unexpected_restore_confirm_response")),
    };
    if let Some(error) = receipt.error {
        return Err(BaseServiceError::new(error, "base_archive_restore_failed"));
    }
    Ok(prepared.operation_id)
}

/// Reconcile a confirmed restore on the activated (refreshed) Base services.
pub async fn reconcile_restored_archive(
    services: &BaseServices,
    operation_id: BaseOperationId,
) -> Result<(), BaseServiceError> {
    match services
        .invoke(BaseRequestV1::Reconcile(operation_id))
        .await?
    {
        BaseResponseV1::Reconciled(result)
            if result.receipt.error.is_none() && !result.receipt.reconcile_required =>
        {
            Ok(())
        }
        BaseResponseV1::Reconciled(_) => Err(BaseServiceError::new(
            BaseErrorCodeV1::UnknownOutcome,
            "base_restore_reconcile_required",
        )),
        _ => Err(internal("unexpected_restore_reconcile_response")),
    }
}

fn open_archive_management(
    node: &OneBrainNode,
    base_host_proof: &[u8],
    scopes: impl IntoIterator<Item = BaseManagementScope>,
) -> Result<(BaseServices, BaseManagementServices), String> {
    let services = node
        .base_services()
        .ok_or_else(|| "Base runtime is not installed".to_owned())?;
    let grant = node
        .issue_base_management_grant([0; 32], base_host_proof, scopes, GRANT_LIFETIME)
        .map_err(|error| error.to_string())?;
    let management = services
        .management(grant)
        .map_err(|error| error.to_string())?;
    Ok((services, management))
}

/// File sink: the file is created on first use and fsynced in `finish`, so it
/// is durable before the Base sink is committed. I/O detail is kept for the
/// host's error message.
struct FileSink {
    path: PathBuf,
    file: Option<File>,
    io_error: Option<std::io::Error>,
}

impl FileSink {
    fn file(&mut self) -> Result<&mut File, BaseServiceError> {
        if self.file.is_none() {
            match File::create(&self.path) {
                Ok(file) => self.file = Some(file),
                Err(error) => return Err(self.fail(error)),
            }
        }
        Ok(self.file.as_mut().expect("file was just created"))
    }
    fn fail(&mut self, error: std::io::Error) -> BaseServiceError {
        self.io_error = Some(error);
        BaseServiceError::new(
            BaseErrorCodeV1::DependencyUnavailable,
            "archive_file_io_failed",
        )
    }
}

impl ArchiveByteSink for FileSink {
    fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), BaseServiceError> {
        let result = self.file()?.write_all(bytes);
        result.map_err(|error| self.fail(error))
    }
    fn finish(&mut self) -> Result<(), BaseServiceError> {
        let result = self.file()?.sync_all();
        result.map_err(|error| self.fail(error))
    }
}

struct FileSource {
    file: File,
    io_error: Option<std::io::Error>,
}

impl ArchiveByteSource for FileSource {
    fn read_chunk(&mut self, buffer: &mut [u8]) -> Result<usize, BaseServiceError> {
        self.file.read(buffer).map_err(|error| {
            self.io_error = Some(error);
            BaseServiceError::new(
                BaseErrorCodeV1::DependencyUnavailable,
                "archive_file_io_failed",
            )
        })
    }
}

/// Create an encrypted Base archive at `output_path` and return its size.
///
/// The file is fsynced before the archive sink is committed. On error the
/// caller owns removing a partially written file.
pub async fn create_base_archive_file(
    node: &OneBrainNode,
    base_host_proof: &[u8],
    password: Vec<u8>,
    output_path: &Path,
) -> Result<u64, String> {
    let (services, management) = open_archive_management(
        node,
        base_host_proof,
        [
            BaseManagementScope::ArchiveSink,
            BaseManagementScope::ArchiveSecret,
        ],
    )?;
    let mut sink = FileSink {
        path: output_path.to_path_buf(),
        file: None,
        io_error: None,
    };
    let operation = create_base_archive(
        &services,
        &management,
        password,
        BASE_ARCHIVE_FILE_MAX_BYTES,
        &mut sink,
    )
    .await
    .map_err(|error| match sink.io_error.take() {
        Some(io) => io.to_string(),
        None => error.to_string(),
    });
    let close = management.close().await.map_err(|error| error.to_string());
    match (operation, close) {
        (Ok(size), Ok(_)) => Ok(size),
        (Err(error), _) | (Ok(_), Err(error)) => Err(error),
    }
}

/// Verify, stage, activate and reconcile the Base archive at `input_path`.
pub async fn restore_base_archive_file(
    node: &OneBrainNode,
    base_host_proof: &[u8],
    password: Vec<u8>,
    input_path: &Path,
) -> Result<(), String> {
    let total = input_path
        .metadata()
        .map_err(|error| error.to_string())?
        .len();
    if total == 0 || total > MAX_BASE_ARCHIVE_DATASET_BYTES.min(BASE_ARCHIVE_FILE_MAX_BYTES) {
        return Err("archive size is outside the Base restore bound".into());
    }
    let (services, management) = open_archive_management(
        node,
        base_host_proof,
        [
            BaseManagementScope::ArchiveSource,
            BaseManagementScope::ArchiveSecret,
        ],
    )?;
    let operation = async {
        let status = services.snapshot().map_err(|error| error.to_string())?;
        let empty = || BaseCapabilitySet::try_from_discriminators(Vec::new());
        services
            .negotiate(BaseNegotiationRequest {
                peer: status.version.compatibility,
                peer_capabilities: BaseCapabilityRequirements {
                    supported: empty().map_err(|error| error.to_string())?,
                    required: empty().map_err(|error| error.to_string())?,
                },
                verified_migration: None,
            })
            .map_err(|error| error.to_string())?;
        let mut source = FileSource {
            file: File::open(input_path).map_err(|error| error.to_string())?,
            io_error: None,
        };
        let operation_id =
            restore_base_archive(&services, &management, password, total, &mut source)
                .await
                .map_err(|error| match source.io_error.take() {
                    Some(io) => io.to_string(),
                    None => error.to_string(),
                })?;
        let refreshed = node
            .base_services()
            .ok_or_else(|| "Base runtime disappeared after activation".to_owned())?;
        reconcile_restored_archive(&refreshed, operation_id)
            .await
            .map_err(|error| error.to_string())
    }
    .await;
    // A successful activation invalidates the old management generation and
    // revokes it as part of the switch. Before activation, close explicitly.
    let _ = management.close().await;
    operation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_vec_sink_enforces_max_bytes_and_preserves_chunks() {
        let mut sink = BoundedVecSink {
            bytes: Vec::new(),
            max_bytes: 5,
        };
        sink.write_chunk(b"ab").unwrap();
        sink.write_chunk(b"cde").unwrap();
        sink.finish().unwrap();
        assert_eq!(sink.bytes, b"abcde");

        let err = sink.write_chunk(b"f").unwrap_err();
        assert_eq!(err.code, BaseErrorCodeV1::ResourceExhausted);
        assert_eq!(err.reason, "archive_response_exceeds_bound");
        assert_eq!(sink.bytes, b"abcde");
    }

    #[test]
    fn slice_source_streams_all_chunks_until_eof() {
        let mut source = SliceSource { bytes: b"abcdefg" };
        let mut buf = [0u8; 3];
        assert_eq!(source.read_chunk(&mut buf).unwrap(), 3);
        assert_eq!(&buf, b"abc");
        assert_eq!(source.read_chunk(&mut buf).unwrap(), 3);
        assert_eq!(&buf, b"def");
        assert_eq!(source.read_chunk(&mut buf).unwrap(), 1);
        assert_eq!(&buf[..1], b"g");
        assert_eq!(source.read_chunk(&mut buf).unwrap(), 0);
    }
}
