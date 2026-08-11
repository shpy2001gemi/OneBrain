//! Data import/export, backup, and restore commands.

use onebrain_node::node::OneBrainNode;
use rand::rngs::OsRng;
use rand::RngCore;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use zeroize::Zeroize;

use onebrain_base_contract::{
    ArchiveCapabilityHandleV1, ArchiveCredentialKindV1, ArchiveSinkBeginV1, ArchiveSinkReadV1,
    ArchiveSourceBeginV1, ArchiveSourcePushV1, BaseCapabilitySet, BaseCommandV1,
    BaseConfirmRequestV1, BaseIdempotencyKey, BaseManagementRequestV1, BaseOperationKindV1,
    BaseOperationReservationId, BasePrepareRequestV1, BaseQualificationState, BaseRequestV1,
    BoundedSecretIngressV1, CreateArchiveCommandV1, ProfileVersion, ResourceBudgetV1,
    RestoreArchiveCommandV1, SourceCommitId, SourceCommitIdentity, ToolchainIdentity,
    MAX_BASE_ARCHIVE_DATASET_BYTES,
};
use onebrain_node::{
    BaseManagementResponseV1, BaseManagementScope, BaseNegotiationRequest, BaseResponseV1,
};

use super::helpers::*;

const BASE_ARCHIVE_PROJECTION_MAX_BYTES: u64 =
    onebrain_node::archive_capabilities::DEFAULT_ARCHIVE_SPOOL_BYTES;

/// Build the compiled tuple without creating a node or opening any storage.
pub(crate) fn compiled_base_status_json() -> serde_json::Value {
    let status = onebrain_node::compiled_base_runtime_config().version_status;
    serde_json::json!({
        "onebrain_version": env!("CARGO_PKG_VERSION"),
        "base_version": {
            "major": status.compatibility.base_version.major,
            "minor": status.compatibility.base_version.minor,
            "patch": status.compatibility.base_version.patch,
            "prerelease": status.compatibility.base_version.prerelease.as_ref().map(|value| value.as_str()),
        },
        "base_commit": source_commit_json(status.compatibility.base_commit),
        "canonical_schema_digest": encode_hex(&status.compatibility.canonical_schema_digest.0),
        "domain_registry_digest": encode_hex(&status.compatibility.domain_registry_digest.0),
        "resource_registry_digest": encode_hex(&status.compatibility.resource_registry_digest.0),
        "storage_schema": status.compatibility.storage_schema.0,
        "archive_profile": profile_json(status.compatibility.archive_profile),
        "migration_profile": profile_json(status.compatibility.migration_profile),
        "registry_profile": profile_json(status.compatibility.registry_profile),
        "registry_profile_digest": encode_hex(&status.compatibility.registry_profile_digest.0),
        "wire_session": profile_json(status.compatibility.wire_session),
        "product_api": profile_json(status.compatibility.product_api),
        "c_abi": profile_json(status.compatibility.c_abi),
        "feature_set_digest": encode_hex(&status.compatibility.feature_set_digest.0),
        "target_triple": status.compatibility.target_triple.as_str(),
        "toolchain": toolchain_json(status.compatibility.toolchain),
        "candidate_semantic_digest": encode_hex(&status.candidate_semantic_digest.0),
        "artifact_tuple_digest": encode_hex(&status.artifact_tuple_digest.0),
        "qualification": match status.qualification {
            BaseQualificationState::Unqualified => serde_json::json!({"state": "unqualified"}),
            BaseQualificationState::Qualified(evidence) => serde_json::json!({
                "state": "qualified",
                "candidate_commit": source_commit_id_json(evidence.candidate_commit),
                "candidate_semantic_digest": encode_hex(&evidence.candidate_semantic_digest.0),
                "evidence_blake3": encode_hex(&evidence.evidence_blake3.0),
            }),
        },
        "features": {
            "base_v1": cfg!(feature = "base-v1"),
            "legacy_read_compat": cfg!(feature = "legacy-read-compat"),
            "vnext_network_runtime_compiled": cfg!(feature = "vnext-network-runtime"),
            "distributed_requested": false,
            "distributed_active": false,
        }
    })
}

fn profile_json(value: ProfileVersion) -> serde_json::Value {
    serde_json::json!({"major": value.major, "minor": value.minor})
}

fn source_commit_json(value: SourceCommitIdentity) -> serde_json::Value {
    match value {
        SourceCommitIdentity::Known(value) => source_commit_id_json(value),
        SourceCommitIdentity::Unknown => serde_json::json!({"kind": "unknown"}),
    }
}

fn source_commit_id_json(value: SourceCommitId) -> serde_json::Value {
    match value {
        SourceCommitId::Sha1(value) => {
            serde_json::json!({"kind": "sha1", "digest": encode_hex(&value.0)})
        }
        SourceCommitId::Sha256(value) => {
            serde_json::json!({"kind": "sha256", "digest": encode_hex(&value.0)})
        }
    }
}

fn toolchain_json(value: ToolchainIdentity) -> serde_json::Value {
    match value {
        ToolchainIdentity::Known(value) => {
            serde_json::json!({"kind": "known", "digest": encode_hex(&value.0)})
        }
        ToolchainIdentity::Unknown => serde_json::json!({"kind": "unknown"}),
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn cmd_export(node: &OneBrainNode, args: &str) {
    // Parse: --mode MODE --output FILE. Mode is intentionally mandatory.
    let mut mode: Option<String> = None;
    let mut output: Option<String> = None;

    let parts: Vec<&str> = args.split_whitespace().collect();
    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "--mode" if i + 1 < parts.len() => {
                mode = Some(parts[i + 1].to_string());
                i += 2;
            }
            "--output" if i + 1 < parts.len() => {
                output = Some(parts[i + 1].to_string());
                i += 2;
            }
            _ => {
                // Treat bare arg as output filename
                if output.is_none() {
                    output = Some(parts[i].to_string());
                }
                i += 1;
            }
        }
    }

    let Some(mode) = mode else {
        eprintln!("  Usage: export --mode <canonical-v1|json-view-v1|csv-view-v1> [--output FILE]");
        return;
    };
    let extension = match mode.as_str() {
        "canonical-v1" => "obx",
        "json-view-v1" => "json",
        "csv-view-v1" => "csv",
        _ => {
            eprintln!("  Unsupported export mode: {mode}");
            return;
        }
    };

    println!();
    let ku_count = node.ku_count().unwrap_or(0);
    println!("  Exporting {} KUs...", ku_count);

    let out_path = output.unwrap_or_else(|| format!("onebrain_export.{extension}"));
    match node.export_data(&mode, Path::new(&out_path)) {
        Ok(count) => {
            println!("  ✓ Exported {} KUs to {}", count, out_path);
            println!();
        }
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
        }
    }
}

pub(crate) async fn cmd_import(node: &mut OneBrainNode, args: &str) {
    let parts = args.split_whitespace().collect::<Vec<_>>();
    let mut mode = None;
    let mut file_path = None;
    let mut index = 0;
    while index < parts.len() {
        match parts[index] {
            "--mode" if index + 1 < parts.len() => {
                mode = Some(parts[index + 1]);
                index += 2;
            }
            value if file_path.is_none() => {
                file_path = Some(value);
                index += 1;
            }
            _ => index += 1,
        }
    }
    let (Some(mode), Some(file_path)) = (mode, file_path) else {
        eprintln!();
        eprintln!("  ✗ Usage: import --mode <canonical-v1|text-drafts-v1> <file>");
        eprintln!();
        return;
    };

    if !Path::new(file_path).exists() {
        eprintln!();
        eprintln!("  ✗ File not found: {}", file_path);
        eprintln!();
        return;
    }

    println!();
    println!("  Reading file...");

    let result = match mode {
        "canonical-v1" => node.import_canonical_exchange(Path::new(file_path)),
        "text-drafts-v1" => node.import_text_drafts(Path::new(file_path)).await,
        "json-view-v1" | "csv-view-v1" => {
            eprintln!("  JSON/CSV views are not importable");
            return;
        }
        _ => {
            eprintln!("  Unsupported import mode: {mode}");
            return;
        }
    };
    match result {
        Ok(result) => {
            println!(
                "  ✓ Imported {} KUs ({} skipped as duplicates, {} errors)",
                result.imported, result.skipped, result.errors
            );
            println!();
        }
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
        }
    }
}

pub(crate) async fn cmd_backup(
    node: &OneBrainNode,
    base_host_proof: &str,
    _args: &str,
    reader: &mut BufReader<tokio::io::Stdin>,
) {
    println!();
    println!("  Creating encrypted backup...");

    // Read password
    eprint!("  Enter password: ");
    let mut password = String::new();
    if reader.read_line(&mut password).await.is_err() {
        eprintln!("  ✗ Failed to read password.");
        println!();
        return;
    }
    let trimmed = password.trim_end().len();
    password.truncate(trimmed);

    if password.is_empty() {
        eprintln!("  ✗ Password cannot be empty.");
        println!();
        return;
    }

    // Confirm password
    eprint!("  Confirm password: ");
    let mut confirm = String::new();
    if reader.read_line(&mut confirm).await.is_err() {
        eprintln!("  ✗ Failed to read confirmation.");
        println!();
        return;
    }
    let trimmed = confirm.trim_end().len();
    confirm.truncate(trimmed);

    if password != confirm {
        password.zeroize();
        confirm.zeroize();
        eprintln!("  ✗ Passwords do not match.");
        println!();
        return;
    }
    confirm.zeroize();

    println!();
    println!("  Capturing the authenticated Base dataset...");

    let archive_path = format!("onebrain_archive_{}.obar", chrono_timestamp());
    match create_base_archive(
        node,
        base_host_proof,
        password.into_bytes(),
        Path::new(&archive_path),
    )
    .await
    {
        Ok(size) => {
            let size_display = if size > 1_048_576 {
                format!("{:.1} MB", size as f64 / 1_048_576.0)
            } else {
                format!("{:.1} KB", size as f64 / 1024.0)
            };
            println!("  ✓ Base archive saved: {archive_path} ({size_display})");
            println!("    ⚠ Keep both the archive and its credential safe.");
            println!();
        }
        Err(error) => {
            let _ = std::fs::remove_file(&archive_path);
            eprintln!("  ✗ Base archive failed: {error}");
            println!();
        }
    }
}

pub(crate) async fn cmd_restore(
    node: &mut OneBrainNode,
    base_host_proof: &str,
    args: &str,
    reader: &mut BufReader<tokio::io::Stdin>,
) {
    let file_path = args.trim();
    if file_path.is_empty() {
        eprintln!();
        eprintln!("  ✗ Usage: restore <file>");
        eprintln!("    Example: restore onebrain_archive_20260809.obar");
        eprintln!();
        return;
    }

    if !Path::new(file_path).exists() {
        eprintln!();
        eprintln!("  ✗ File not found: {}", file_path);
        eprintln!();
        return;
    }

    println!();
    println!("  ⚠ This will REPLACE all local data.");

    // Confirm
    eprint!("  Continue? (y/N): ");
    let mut confirm = String::new();
    if reader.read_line(&mut confirm).await.is_err() {
        eprintln!("  ✗ Failed to read input.");
        println!();
        return;
    }
    if confirm.trim().to_lowercase() != "y" {
        println!("  Cancelled.");
        println!();
        return;
    }

    // Read password
    eprint!("  Enter backup password: ");
    let mut password = String::new();
    if reader.read_line(&mut password).await.is_err() {
        eprintln!("  ✗ Failed to read password.");
        println!();
        return;
    }
    let trimmed = password.trim_end().len();
    password.truncate(trimmed);
    if password.is_empty() {
        eprintln!("  ✗ Password cannot be empty.");
        return;
    }

    println!();
    println!("  Verifying and staging the Base archive...");

    match restore_base_archive(
        node,
        base_host_proof,
        password.into_bytes(),
        Path::new(file_path),
    )
    .await
    {
        Ok(()) => {
            println!("  ✓ Restore verified, activated, and reconciled.");
            println!();
        }
        Err(error) => {
            eprintln!("  ✗ Base restore failed: {error}");
            println!();
        }
    }
}

async fn open_archive_management(
    node: &OneBrainNode,
    base_host_proof: &str,
    scopes: impl IntoIterator<Item = BaseManagementScope>,
) -> Result<
    (
        onebrain_node::BaseServices,
        onebrain_node::BaseManagementServices,
    ),
    String,
> {
    let services = node
        .base_services()
        .ok_or_else(|| "Base runtime is not installed".to_owned())?;
    let grant = node
        .issue_base_management_grant(
            [0; 32],
            base_host_proof.as_bytes(),
            scopes,
            Duration::from_secs(300),
        )
        .map_err(|error| error.to_string())?;
    let management = services
        .management(grant)
        .map_err(|error| error.to_string())?;
    Ok((services, management))
}

async fn create_base_archive(
    node: &OneBrainNode,
    base_host_proof: &str,
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
    )
    .await?;
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
                    max_total_bytes: BASE_ARCHIVE_PROJECTION_MAX_BYTES,
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
                    sink: onebrain_base_contract::ArchiveSinkHandleV1::from_opaque_bytes(
                        *sink.as_bytes(),
                    ),
                    secret,
                    budget: ResourceBudgetV1::try_new(1, 1_048_576, 1_000_000)
                        .map_err(|error| error.to_string())?,
                }),
            }))
            .await
            .map_err(|error| error.to_string())?
        {
            BaseResponseV1::Prepared(value) => value,
            _ => return Err("unexpected prepare response".to_owned()),
        };
        let mut idempotency = [0; 32];
        OsRng.fill_bytes(&mut idempotency);
        let receipt = match services
            .invoke(BaseRequestV1::Confirm(BaseConfirmRequestV1 {
                operation_id: prepared.operation_id,
                idempotency_key: BaseIdempotencyKey(idempotency),
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
                        handle: onebrain_base_contract::ArchiveSinkHandleV1::from_opaque_bytes(
                            *sink.as_bytes(),
                        ),
                        offset,
                        max_bytes: 1_048_576,
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

async fn restore_base_archive(
    node: &mut OneBrainNode,
    base_host_proof: &str,
    password: Vec<u8>,
    input_path: &Path,
) -> Result<(), String> {
    let total = input_path
        .metadata()
        .map_err(|error| error.to_string())?
        .len();
    if total == 0 || total > MAX_BASE_ARCHIVE_DATASET_BYTES.min(BASE_ARCHIVE_PROJECTION_MAX_BYTES) {
        return Err("archive size is outside the Base restore bound".into());
    }
    let (services, management) = open_archive_management(
        node,
        base_host_proof,
        [
            BaseManagementScope::ArchiveSource,
            BaseManagementScope::ArchiveSecret,
        ],
    )
    .await?;
    let operation = async {
        let status = services.snapshot().map_err(|error| error.to_string())?;
        let empty = || BaseCapabilitySet::try_from_discriminators(Vec::new()).unwrap();
        services
            .negotiate(BaseNegotiationRequest {
                peer: status.version.compatibility,
                peer_capabilities: onebrain_base_contract::BaseCapabilityRequirements {
                    supported: empty(),
                    required: empty(),
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
        let mut buffer = vec![0; 1_048_576];
        loop {
            let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            management
                .invoke(BaseManagementRequestV1::ArchiveSourcePush(
                    ArchiveSourcePushV1 {
                        handle: onebrain_base_contract::ArchiveSourceHandleV1::from_opaque_bytes(
                            *source.as_bytes(),
                        ),
                        offset,
                        chunk: onebrain_base_contract::ArchiveChunkV1::try_from_bytes(
                            buffer[..read].to_vec(),
                        )
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
                    source: onebrain_base_contract::ArchiveSourceHandleV1::from_opaque_bytes(
                        *source.as_bytes(),
                    ),
                    secret,
                    budget: ResourceBudgetV1::try_new(1, 1_048_576, 1_000_000)
                        .map_err(|error| error.to_string())?,
                }),
            }))
            .await
            .map_err(|error| error.to_string())?
        {
            BaseResponseV1::Prepared(value) => value,
            _ => return Err("unexpected prepare response".to_owned()),
        };
        let mut idempotency = [0; 32];
        OsRng.fill_bytes(&mut idempotency);
        match services
            .invoke(BaseRequestV1::Confirm(BaseConfirmRequestV1 {
                operation_id: prepared.operation_id,
                idempotency_key: BaseIdempotencyKey(idempotency),
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
