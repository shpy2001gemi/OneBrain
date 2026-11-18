//! Data import/export, backup, and restore commands.

use onebrain_node::base_archive_file::{create_base_archive_file, restore_base_archive_file};
use onebrain_node::node::OneBrainNode;
use std::path::Path;
use tokio::io::{AsyncBufReadExt, BufReader};
use zeroize::Zeroize;

use onebrain_base_contract::{
    BaseQualificationState, ProfileVersion, SourceCommitId, SourceCommitIdentity, ToolchainIdentity,
};

use super::helpers::*;

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
    match create_base_archive_file(
        node,
        base_host_proof.as_bytes(),
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

    match restore_base_archive_file(
        node,
        base_host_proof.as_bytes(),
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
