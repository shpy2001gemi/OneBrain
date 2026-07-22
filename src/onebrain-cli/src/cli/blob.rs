//! Blob / media attachment management commands.

use onebrain_node::node::OneBrainNode;
use tokio::io::{AsyncBufReadExt, BufReader};

use super::helpers::*;

pub(crate) async fn cmd_blob(
    node: &mut OneBrainNode,
    args: &str,
    reader: &mut BufReader<tokio::io::Stdin>,
) {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let subcmd = parts.first().map(|s| s.trim()).unwrap_or("");
    let rest = parts.get(1).map(|s| s.trim()).unwrap_or("");

    match subcmd {
        "" | "help" => {
            println!();
            println!("  ── Blob Commands ──");
            println!("  blob list              List stored blobs");
            println!("  blob store <file>      Store a file as blob");
            println!("  blob detail <cid>      View blob metadata");
            println!("  blob export <cid> [out] Export blob to file");
            println!("  blob delete <cid>      Delete a blob");
            println!("  blob pin <cid>         Pin blob (prevent GC)");
            println!("  blob unpin <cid>       Unpin blob (allow GC)");
            println!("  blob stats             Storage statistics");
            println!("  blob gc                Garbage collect orphans");
            println!();
        }

        "list" | "ls" => match node.list_blobs() {
            Ok(blobs) => {
                println!();
                if blobs.is_empty() {
                    println!("  No blobs stored.");
                } else {
                    println!("  ── Stored Blobs ({}) ──", blobs.len());
                    println!();
                    println!(
                        "  {:<12} {:<20} {:<10} {:<10} {:<5}",
                        "CID", "Name", "Type", "Size", "Refs"
                    );
                    println!("  {}", "─".repeat(60));
                    for blob in &blobs {
                        let type_name =
                            ku_core::blob_store::BlobType::from_u8(blob.blob_type).name();
                        let size_str = format_size(blob.total_size);
                        let pin = if blob.pinned { " \u{1f4cc}" } else { "" };
                        println!(
                            "  {:<12} {:<20} {:<10} {:<10} {}{}",
                            &blob.blob_cid_hex[..12.min(blob.blob_cid_hex.len())],
                            truncate_str(&blob.original_name, 18),
                            type_name,
                            size_str,
                            blob.referencing_kus.len(),
                            pin,
                        );
                    }
                }
                println!();
            }
            Err(e) => {
                eprintln!("  \u{2717} {}", e);
                println!();
            }
        },

        "store" | "add" => {
            if rest.is_empty() {
                eprintln!("  \u{2717} Usage: blob store <file_path>");
                println!();
                return;
            }
            let file_path = std::path::Path::new(rest);
            if !file_path.exists() {
                eprintln!("  \u{2717} File not found: {}", rest);
                println!();
                return;
            }
            match node.store_blob(file_path) {
                Ok(meta) => {
                    let type_name = ku_core::blob_store::BlobType::from_u8(meta.blob_type).name();
                    println!();
                    println!("  \u{2713} Blob stored successfully");
                    println!(
                        "  CID:    {}",
                        &meta.blob_cid_hex[..16.min(meta.blob_cid_hex.len())]
                    );
                    println!("  Name:   {}", meta.original_name);
                    println!("  Type:   {}", type_name);
                    println!("  Size:   {}", format_size(meta.total_size));
                    println!("  Chunks: {}", meta.chunk_count);
                    println!("  MIME:   {}", meta.mime_type);
                    println!();
                }
                Err(e) => {
                    eprintln!("  \u{2717} {}", e);
                    println!();
                }
            }
        }

        "detail" | "info" => {
            if rest.is_empty() {
                eprintln!("  \u{2717} Usage: blob detail <blob_cid>");
                println!();
                return;
            }
            match node.get_blob_meta(rest) {
                Ok(meta) => {
                    let type_name = ku_core::blob_store::BlobType::from_u8(meta.blob_type).name();
                    println!();
                    println!("  ── Blob Detail ──");
                    println!("  CID:        {}", meta.blob_cid_hex);
                    println!("  Name:       {}", meta.original_name);
                    println!("  Type:       {}", type_name);
                    println!("  MIME:       {}", meta.mime_type);
                    println!(
                        "  Size:       {} ({} bytes)",
                        format_size(meta.total_size),
                        meta.total_size
                    );
                    println!(
                        "  Chunks:     {} \u{00d7} {}KB",
                        meta.chunk_count,
                        meta.chunk_size / 1024
                    );
                    println!("  BLAKE3:     {}", meta.blake3_hex);
                    println!("  Created:    {}", meta.created_at);
                    println!(
                        "  Pinned:     {}",
                        if meta.pinned { "Yes \u{1f4cc}" } else { "No" }
                    );
                    println!("  References: {} KU(s)", meta.referencing_kus.len());
                    for ku_cid in &meta.referencing_kus {
                        println!("    \u{2192} {}", &ku_cid[..16.min(ku_cid.len())]);
                    }
                    println!();
                }
                Err(e) => {
                    eprintln!("  \u{2717} {}", e);
                    println!();
                }
            }
        }

        "export" | "get" => {
            let export_parts: Vec<&str> = rest.splitn(2, ' ').collect();
            let cid = export_parts.first().map(|s| s.trim()).unwrap_or("");
            if cid.is_empty() {
                eprintln!("  \u{2717} Usage: blob export <blob_cid> [output_path]");
                println!();
                return;
            }
            // Get meta first for default filename
            let output = if let Some(out) = export_parts.get(1) {
                std::path::PathBuf::from(out.trim())
            } else {
                // Use original name
                match node.get_blob_meta(cid) {
                    Ok(meta) => std::path::PathBuf::from(&meta.original_name),
                    Err(_) => std::path::PathBuf::from("blob_export.bin"),
                }
            };
            match node.export_blob(cid, &output) {
                Ok(size) => {
                    println!(
                        "  \u{2713} Exported {} to {}",
                        format_size(size),
                        output.display()
                    );
                    println!();
                }
                Err(e) => {
                    eprintln!("  \u{2717} {}", e);
                    println!();
                }
            }
        }

        "delete" | "rm" => {
            if rest.is_empty() {
                eprintln!("  \u{2717} Usage: blob delete <blob_cid>");
                println!();
                return;
            }
            // Confirm
            eprint!("  Delete blob {}...? (y/N): ", &rest[..12.min(rest.len())]);
            let mut confirm = String::new();
            if reader.read_line(&mut confirm).await.is_err() {
                return;
            }
            if confirm.trim().to_lowercase() != "y" {
                println!("  Cancelled.");
                println!();
                return;
            }
            match node.delete_blob_file(rest) {
                Ok(true) => {
                    println!("  \u{2713} Blob deleted.");
                    println!();
                }
                Ok(false) => {
                    eprintln!("  \u{2717} Blob not found.");
                    println!();
                }
                Err(e) => {
                    eprintln!("  \u{2717} {}", e);
                    println!();
                }
            }
        }

        "stats" => match node.blob_stats() {
            Ok((count, total_size)) => {
                println!();
                println!("  ── Blob Storage Stats ──");
                println!("  Blobs:  {}", count);
                println!("  Size:   {}", format_size(total_size));
                println!();
            }
            Err(e) => {
                eprintln!("  \u{2717} {}", e);
                println!();
            }
        },

        "gc" => {
            println!("  Scanning for orphaned blobs...");
            match node.blob_gc() {
                Ok((deleted, freed)) => {
                    if deleted == 0 {
                        println!("  \u{2713} No orphaned blobs found.");
                    } else {
                        println!(
                            "  \u{2713} Deleted {} orphaned blob(s), freed {}",
                            deleted,
                            format_size(freed)
                        );
                    }
                    println!();
                }
                Err(e) => {
                    eprintln!("  \u{2717} {}", e);
                    println!();
                }
            }
        }

        "pin" => {
            if rest.is_empty() {
                eprintln!("  \u{2717} Usage: blob pin <cid>");
                println!();
                return;
            }
            match node.pin_blob(rest) {
                Ok(true) => {
                    println!();
                    println!("  📌 Blob pinned: {}", rest);
                    println!("  This blob will not be removed by garbage collection.");
                    println!();
                }
                Ok(false) => {
                    eprintln!("  \u{2717} Blob not found.");
                    println!();
                }
                Err(e) => {
                    eprintln!("  \u{2717} {}", e);
                    println!();
                }
            }
        }

        "unpin" => {
            if rest.is_empty() {
                eprintln!("  \u{2717} Usage: blob unpin <cid>");
                println!();
                return;
            }
            match node.unpin_blob(rest) {
                Ok(true) => {
                    println!();
                    println!("  ✓ Blob unpinned: {}", rest);
                    println!();
                }
                Ok(false) => {
                    eprintln!("  \u{2717} Blob not found.");
                    println!();
                }
                Err(e) => {
                    eprintln!("  \u{2717} {}", e);
                    println!();
                }
            }
        }

        _ => {
            eprintln!("  \u{2717} Unknown blob subcommand: {}", subcmd);
            eprintln!("    Type 'blob help' for available commands.");
            println!();
        }
    }
}
