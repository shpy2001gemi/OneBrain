//! Tag, pin, unpin, and watch (standing query) commands.

use onebrain_node::node::OneBrainNode;

use super::helpers::*;

pub(crate) fn cmd_tag(node: &mut OneBrainNode, args: &str) {
    let parts: Vec<&str> = args.splitn(3, ' ').collect();

    match parts.first().copied() {
        Some("add") => {
            if parts.len() < 3 {
                eprintln!("  ✗ Usage: tag add <cid> <tag>");
                println!();
                return;
            }
            let cid = parts[1];
            let tag = parts[2];
            match node.add_tag(cid, tag) {
                Ok(()) => {
                    println!();
                    println!(
                        "  ✓ Tag '{}' added to KU {}",
                        tag,
                        &cid[..std::cmp::min(16, cid.len())]
                    );
                    println!();
                }
                Err(e) => {
                    eprintln!("  ✗ {}", e);
                    println!();
                }
            }
        }
        Some("remove") | Some("rm") => {
            if parts.len() < 3 {
                eprintln!("  ✗ Usage: tag remove <cid> <tag>");
                println!();
                return;
            }
            let cid = parts[1];
            let tag = parts[2];
            match node.remove_tag(cid, tag) {
                Ok(()) => {
                    println!();
                    println!(
                        "  ✓ Tag '{}' removed from KU {}",
                        tag,
                        &cid[..std::cmp::min(16, cid.len())]
                    );
                    println!();
                }
                Err(e) => {
                    eprintln!("  ✗ {}", e);
                    println!();
                }
            }
        }
        Some("list") | Some("ls") => {
            let tags = node.list_all_tags();
            println!();
            if tags.is_empty() {
                println!("  No tags found.");
            } else {
                println!("  ── Tags ({}) ──", tags.len());
                for tag in &tags {
                    println!("  • {}", tag);
                }
            }
            println!();
        }
        _ => {
            eprintln!();
            eprintln!("  ✗ Usage:");
            eprintln!("    tag add <cid> <tag>      Add tag to KU");
            eprintln!("    tag remove <cid> <tag>   Remove tag from KU");
            eprintln!("    tag list                 List all tags");
            eprintln!();
        }
    }
}

pub(crate) fn cmd_pin(node: &mut OneBrainNode, args: &str) {
    if args.is_empty() {
        // Show pinned KUs
        let pinned = node.pinned_kus();
        println!();
        if pinned.is_empty() {
            println!("  No pinned KUs.");
            println!("  Use 'pin <cid>' to pin a KU for quick access.");
        } else {
            println!("  ── Pinned KUs ({}) ──", pinned.len());
            for ku in &pinned {
                let short_cid = &ku.cid_hex[..std::cmp::min(8, ku.cid_hex.len())];
                println!(
                    "  📌 [{}] ({}) {}",
                    short_cid,
                    ku.gene_type,
                    truncate_str(&ku.preview, 50)
                );
            }
        }
        println!();
        return;
    }

    match node.pin_ku(args.trim()) {
        Ok(true) => {
            println!();
            println!("  📌 KU pinned: {}", args.trim());
            println!();
        }
        Ok(false) => {
            eprintln!("  ✗ KU not found: {}", args.trim());
            println!();
        }
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
        }
    }
}

pub(crate) fn cmd_unpin(node: &mut OneBrainNode, args: &str) {
    if args.is_empty() {
        eprintln!("  ✗ Usage: unpin <cid>");
        println!();
        return;
    }

    match node.unpin_ku(args.trim()) {
        Ok(true) => {
            println!();
            println!("  ✓ KU unpinned: {}", args.trim());
            println!();
        }
        Ok(false) => {
            eprintln!("  ✗ KU not found: {}", args.trim());
            println!();
        }
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
        }
    }
}

pub(crate) fn cmd_watch(node: &mut OneBrainNode, args: &str) {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();

    match parts.first().copied() {
        Some("create") | Some("add") => {
            let kql = parts.get(1).copied().unwrap_or("").trim();
            if kql.is_empty() {
                eprintln!("  ✗ Usage: watch create <kql_query>");
                eprintln!("    Example: watch create FIND facts WHERE trust > 0.8");
                println!();
                return;
            }
            match node.create_watch(kql) {
                Ok(id) => {
                    println!();
                    println!("  ✓ Watch created: {}", id);
                    println!("  Query: {}", kql);
                    println!("  You will be notified when new matching KUs arrive.");
                    println!();
                }
                Err(e) => {
                    eprintln!("  ✗ {}", e);
                    println!();
                }
            }
        }
        Some("list") | Some("ls") => {
            let watches = node.list_watches();
            println!();
            if watches.is_empty() {
                println!("  No active watches.");
                println!("  Use 'watch create <kql>' to create a standing query.");
            } else {
                println!("  ── Active Watches ({}) ──", watches.len());
                for w in &watches {
                    println!("  [{}] {} (matches: {})", w.id, w.kql_query, w.match_count);
                }
            }
            println!();
        }
        Some("delete") | Some("rm") => {
            let id = parts.get(1).copied().unwrap_or("").trim();
            if id.is_empty() {
                eprintln!("  ✗ Usage: watch delete <watch_id>");
                println!();
                return;
            }
            match node.delete_watch(id) {
                Ok(true) => {
                    println!();
                    println!("  ✓ Watch deleted: {}", id);
                    println!();
                }
                Ok(false) => {
                    eprintln!("  ✗ Watch not found: {}", id);
                    println!();
                }
                Err(e) => {
                    eprintln!("  ✗ {}", e);
                    println!();
                }
            }
        }
        _ => {
            eprintln!();
            eprintln!("  ✗ Usage:");
            eprintln!("    watch create <kql>   Create standing query");
            eprintln!("    watch list           List active watches");
            eprintln!("    watch delete <id>    Delete a watch");
            eprintln!();
        }
    }
}
