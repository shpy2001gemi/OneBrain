//! Host file transfer for Base archives through the scoped management facade.
//!
//! The CLI `backup`/`restore` commands use these functions. Every archive byte
//! moves through a Base management grant (archive sink/source + secret
//! scopes), a reserved/prepared/confirmed Base operation and an explicit
//! management close; nothing here reads or writes the dataset directly.
//! ARCH-13 slice 16: `tests/base_gate_integration.rs` round-trips this facade
//! against a real installed Base runtime.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use onebrain_base_contract::{
    ArchiveCapabilityHandleV1, ArchiveChunkV1, ArchiveCredentialKindV1, ArchiveSinkBeginV1,
    ArchiveSinkHandleV1, ArchiveSinkReadV1, ArchiveSourceBeginV1, ArchiveSourceHandleV1,
    ArchiveSourcePushV1, BaseCapabilityRequirements, BaseCapabilitySet, BaseCommandV1,
    BaseConfirmRequestV1, BaseIdempotencyKey, BaseManagementRequestV1, BaseOperationKindV1,
    BaseOperationReservationId, BasePrepareRequestV1, BaseRequestV1, BoundedSecretIngressV1,
    CreateArchiveCommandV1, ResourceBudgetV1, RestoreArchiveCommandV1,
    MAX_BASE_ARCHIVE_DATASET_BYTES,
};

use crate::archive_capabilities::DEFAULT_ARCHIVE_SPOOL_BYTES;
use crate::base_runtime::{
    BaseManagementResponseV1, BaseManagementScope, BaseManagementServices, BaseNegotiationRequest,
    BaseResponseV1, BaseServices,
};
use crate::node::OneBrainNode;

/// Largest archive the file transfer accepts in either direction.
pub const BASE_ARCHIVE_FILE_MAX_BYTES: u64 = DEFAULT_ARCHIVE_SPOOL_BYTES;
const CHUNK_BYTES: usize = 1_048_576;
const GRANT_LIFETIME: Duration = Duration::from_secs(300);

fn idempotency_key() -> Result<BaseIdempotencyKey, String> {
    let mut key = [0; 32];
    getrandom::fill(&mut key).map_err(|error| error.to_string())?;
    Ok(BaseIdempotencyKey(key))
}

fn budget() -> Result<ResourceBudgetV1, String> {
    ResourceBudgetV1::try_new(1, 1_048_576, 1_000_000).map_err(|error| error.to_string())
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
    let operation = async {
        let reservation = match services
            .invoke(BaseRequestV1::ReserveOperation(
                BaseOperationKindV1::CreateArchive,
            ))
            .await
            .map_err(|error| error.to_string())?
        {
            BaseResponseV1::Reserved(value) => value,
            _ => return Err("unexpected reserve response".to_owned()),
        };
        let sink = match management
            .invoke(BaseManagementRequestV1::ArchiveSinkBegin(
                ArchiveSinkBeginV1 {
                    reservation_id: reservation,
                    max_total_bytes: BASE_ARCHIVE_FILE_MAX_BYTES,
                },
            ))
            .await
            .map_err(|error| error.to_string())?
        {
            BaseManagementResponseV1::ArchiveSink(value) => value,
            _ => return Err("unexpected archive sink response".to_owned()),
        };
        let secret = match management
            .invoke(BaseManagementRequestV1::ArchiveSecretRegister(
                BoundedSecretIngressV1::try_new(ArchiveCredentialKindV1::Password, password)
                    .map_err(|error| error.to_string())?,
            ))
            .await
            .map_err(|error| error.to_string())?
        {
            BaseManagementResponseV1::ArchiveSecret(value) => value,
            _ => return Err("unexpected archive secret response".to_owned()),
        };
        let prepared = match services
            .invoke(BaseRequestV1::Prepare(BasePrepareRequestV1 {
                reservation_id: reservation,
                command: BaseCommandV1::CreateArchive(CreateArchiveCommandV1 {
                    sink: ArchiveSinkHandleV1::from_opaque_bytes(*sink.as_bytes()),
                    secret,
                    budget: budget()?,
                }),
            }))
            .await
            .map_err(|error| error.to_string())?
        {
            BaseResponseV1::Prepared(value) => value,
            _ => return Err("unexpected prepare response".to_owned()),
        };
        let receipt = match services
            .invoke(BaseRequestV1::Confirm(BaseConfirmRequestV1 {
                operation_id: prepared.operation_id,
                idempotency_key: idempotency_key()?,
            }))
            .await
            .map_err(|error| error.to_string())?
        {
            BaseResponseV1::Receipt(receipt) if receipt.error.is_none() => receipt,
            BaseResponseV1::Receipt(_) => return Err("archive creation was not committed".into()),
            _ => return Err("unexpected confirm response".to_owned()),
        };
        if receipt.result.len() != 64 || receipt.result[..32] != sink.as_bytes()[..] {
            return Err("archive creation receipt does not bind the readable sink".into());
        }

        let mut file = File::create(output_path).map_err(|error| error.to_string())?;
        let mut offset = 0u64;
        loop {
            let (chunk_offset, bytes, eof) = match management
                .invoke(BaseManagementRequestV1::ArchiveSinkRead(
                    ArchiveSinkReadV1 {
                        handle: ArchiveSinkHandleV1::from_opaque_bytes(*sink.as_bytes()),
                        offset,
                        max_bytes: CHUNK_BYTES as u32,
                    },
                ))
                .await
                .map_err(|error| error.to_string())?
            {
                BaseManagementResponseV1::ArchiveChunk { offset, bytes, eof } => {
                    (offset, bytes, eof)
                }
                _ => return Err("unexpected archive chunk response".to_owned()),
            };
            if chunk_offset != offset {
                return Err("archive sink returned a non-contiguous offset".into());
            }
            file.write_all(&bytes).map_err(|error| error.to_string())?;
            offset = offset
                .checked_add(bytes.len() as u64)
                .ok_or_else(|| "archive byte count overflow".to_owned())?;
            if eof {
                break;
            }
        }
        file.sync_all().map_err(|error| error.to_string())?;
        management
            .invoke(BaseManagementRequestV1::ArchiveSinkCommit(
                ArchiveCapabilityHandleV1::from_opaque_bytes(*sink.as_bytes()),
            ))
            .await
            .map_err(|error| error.to_string())?;
        Ok(offset)
    }
    .await;
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
        let reservation = match services
            .invoke(BaseRequestV1::ReserveOperation(
                BaseOperationKindV1::RestoreArchive,
            ))
            .await
            .map_err(|error| error.to_string())?
        {
            BaseResponseV1::Reserved(value) => value,
            _ => return Err("unexpected reserve response".to_owned()),
        };
        let source = match management
            .invoke(BaseManagementRequestV1::ArchiveSourceBegin(
                ArchiveSourceBeginV1 {
                    reservation_id: reservation,
                    declared_total_bytes: total,
                },
            ))
            .await
            .map_err(|error| error.to_string())?
        {
            BaseManagementResponseV1::ArchiveSource(value) => value,
            _ => return Err("unexpected archive source response".to_owned()),
        };
        let mut file = File::open(input_path).map_err(|error| error.to_string())?;
        let mut offset = 0u64;
        let mut buffer = vec![0; CHUNK_BYTES];
        loop {
            let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            management
                .invoke(BaseManagementRequestV1::ArchiveSourcePush(
                    ArchiveSourcePushV1 {
                        handle: ArchiveSourceHandleV1::from_opaque_bytes(*source.as_bytes()),
                        offset,
                        chunk: ArchiveChunkV1::try_from_bytes(buffer[..read].to_vec())
                            .map_err(|error| error.to_string())?,
                    },
                ))
                .await
                .map_err(|error| error.to_string())?;
            offset = offset
                .checked_add(read as u64)
                .ok_or_else(|| "archive byte count overflow".to_owned())?;
        }
        if offset != total {
            return Err("archive changed while it was being uploaded".into());
        }
        management
            .invoke(BaseManagementRequestV1::ArchiveSourceSeal(
                ArchiveCapabilityHandleV1::from_opaque_bytes(*source.as_bytes()),
            ))
            .await
            .map_err(|error| error.to_string())?;
        let secret = match management
            .invoke(BaseManagementRequestV1::ArchiveSecretRegister(
                BoundedSecretIngressV1::try_new(ArchiveCredentialKindV1::Password, password)
                    .map_err(|error| error.to_string())?,
            ))
            .await
            .map_err(|error| error.to_string())?
        {
            BaseManagementResponseV1::ArchiveSecret(value) => value,
            _ => return Err("unexpected archive secret response".to_owned()),
        };
        let prepared = match services
            .invoke(BaseRequestV1::Prepare(BasePrepareRequestV1 {
                reservation_id: BaseOperationReservationId(reservation.0),
                command: BaseCommandV1::RestoreArchive(RestoreArchiveCommandV1 {
                    source: ArchiveSourceHandleV1::from_opaque_bytes(*source.as_bytes()),
                    secret,
                    budget: budget()?,
                }),
            }))
            .await
            .map_err(|error| error.to_string())?
        {
            BaseResponseV1::Prepared(value) => value,
            _ => return Err("unexpected prepare response".to_owned()),
        };
        match services
            .invoke(BaseRequestV1::Confirm(BaseConfirmRequestV1 {
                operation_id: prepared.operation_id,
                idempotency_key: idempotency_key()?,
            }))
            .await
            .map_err(|error| error.to_string())?
        {
            BaseResponseV1::Receipt(receipt) if receipt.error.is_none() => {}
            BaseResponseV1::Receipt(_) => return Err("archive restore was not committed".into()),
            _ => return Err("unexpected confirm response".to_owned()),
        }
        let refreshed = node
            .base_services()
            .ok_or_else(|| "Base runtime disappeared after activation".to_owned())?;
        match refreshed
            .invoke(BaseRequestV1::Reconcile(prepared.operation_id))
            .await
            .map_err(|error| error.to_string())?
        {
            BaseResponseV1::Reconciled(result)
                if result.receipt.error.is_none() && !result.receipt.reconcile_required =>
            {
                Ok(())
            }
            BaseResponseV1::Reconciled(_) => {
                Err("Base restore still requires reconciliation".to_owned())
            }
            _ => Err("unexpected reconcile response".to_owned()),
        }
    }
    .await;
    // A successful activation invalidates the old management generation and
    // revokes it as part of the switch. Before activation, close explicitly.
    let close = management.close().await.map_err(|error| error.to_string());
    match operation {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = close;
            Err(error)
        }
    }
}
