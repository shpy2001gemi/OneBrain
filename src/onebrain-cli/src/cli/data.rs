//! Data import/export, backup, and restore commands.

use onebrain_node::node::OneBrainNode;
use std::path::Path;
use tokio::io::{AsyncBufReadExt, BufReader};

use super::helpers::*;

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
    let password = password.trim();

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
    let confirm = confirm.trim();

    if password != confirm {
        eprintln!("  ✗ Passwords do not match.");
        println!();
        return;
    }

    println!();
    println!("  Backing up:");

    let backup_path = format!("onebrain_backup_{}.obk", chrono_timestamp());
    match node.create_backup(Path::new(&backup_path), password) {
        Ok(info) => {
            println!("    ✓ identity.json (encrypted)");
            println!("    ✓ ku.redb ({} KUs)", info.ku_count);
            println!("    ✓ user_profile.json");
            println!("    ✓ known_peers.json");
            println!("    ✓ retriever_index.json");
            println!();

            let size_display = if info.size > 1_048_576 {
                format!("{:.1} MB", info.size as f64 / 1_048_576.0)
            } else {
                format!("{:.1} KB", info.size as f64 / 1024.0)
            };
            println!("  ✓ Backup saved: {} ({})", info.path, size_display);
            println!("    ⚠ Keep this file safe. It contains your private key (encrypted).");
            println!();
        }
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
        }
    }
}

pub(crate) async fn cmd_restore(
    node: &mut OneBrainNode,
    args: &str,
    reader: &mut BufReader<tokio::io::Stdin>,
) {
    let file_path = args.trim();
    if file_path.is_empty() {
        eprintln!();
        eprintln!("  ✗ Usage: restore <file>");
        eprintln!("    Example: restore onebrain_backup_20260707.obk");
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
    let password = password.trim();

    println!();
    println!("  Restoring:");

    match node.restore_backup(Path::new(file_path), password) {
        Ok(()) => {
            println!("    ✓ identity.json");
            println!("    ✓ user_profile.json");
            println!("    ✓ known_peers.json");
            println!();
            println!("  ✓ Restore complete! Restart the node to apply.");
            println!();
        }
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
        }
    }
}
