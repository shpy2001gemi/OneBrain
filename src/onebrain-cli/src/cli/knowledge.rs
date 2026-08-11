//! Knowledge management commands: encode, search, list, detail, delete,
//! bulk_delete, kql, graph, deprecate, edit.

use onebrain_node::node::OneBrainNode;
use onebrain_node::types::*;
use tokio::io::{AsyncBufReadExt, BufReader};

use super::helpers::*;

pub(crate) async fn cmd_encode(
    node: &mut OneBrainNode,
    args: &str,
    _reader: &mut BufReader<tokio::io::Stdin>,
) {
    if args.is_empty() {
        eprintln!();
        eprintln!("  ✗ Usage: encode <text>");
        eprintln!("    Options: --draft       Save locally without broadcasting");
        eprintln!("             --attach <f>  Attach files (can repeat)");
        eprintln!("    Type 'help encode' for details.");
        eprintln!();
        return;
    }

    // Parse flags
    let is_draft = args.contains("--draft");
    let mut attachments: Vec<String> = Vec::new();
    let mut text_parts: Vec<&str> = Vec::new();

    let parts: Vec<&str> = args.split_whitespace().collect();
    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "--draft" => {
                i += 1;
            }
            "--attach" => {
                if i + 1 < parts.len() {
                    attachments.push(parts[i + 1].to_string());
                    i += 2;
                } else {
                    eprintln!("  ✗ --attach requires a filename");
                    return;
                }
            }
            other => {
                text_parts.push(other);
                i += 1;
            }
        }
    }

    let text = text_parts.join(" ");
    if text.is_empty() {
        eprintln!("  ✗ No text provided for encoding");
        return;
    }

    // Choose encode path
    let result = if is_draft {
        match node.save_draft(&text, None) {
            Ok(draft) => {
                println!();
                println!("  📝 Draft saved!");
                println!("  ID:      {}", draft.id);
                println!("  Title:   {}", draft.title);
                println!("  Created: {}", draft.created);
                println!();
                println!(
                    "  Use `onebrain draft publish {}` to encode and broadcast.",
                    draft.id
                );
                return;
            }
            Err(e) => {
                eprintln!("  ✗ Failed to save draft: {}", e);
                return;
            }
        }
    } else if !attachments.is_empty() {
        node.encode_with_attachments(&text, &attachments).await
    } else {
        node.encode_and_store(&text).await
    };

    match result {
        Ok(result) => {
            let cid_hex: String = result.cid.iter().map(|b| format!("{:02x}", b)).collect();
            let peer_count = node.peer_count();
            println!();
            println!("  ✓ Encoded and stored successfully");
            println!("  CID:          {}", cid_hex);
            if let Some(ref gt) = result.gene_type {
                println!("  Gene type:    {}", gt);
            }
            println!("  Confidence:   {:.0}%", result.confidence * 100.0);
            println!("  Wire size:    {} bytes", result.wire_size);
            println!("  Instructions: {}", result.instruction_count);
            if !attachments.is_empty() {
                println!("  Attachments:  {} file(s)", attachments.len());
            }
            if peer_count > 0 {
                println!("  📡 Broadcasting to {} peer(s)...", peer_count);
                println!("  🔍 Verification requested from {} peer(s)", peer_count);
            }
            println!();
        }
        Err(e) => {
            eprintln!();
            eprintln!("  ✗ {}", e);
            eprintln!();
        }
    }
}

pub(crate) async fn cmd_search(node: &mut OneBrainNode, args: &str) {
    if args.is_empty() {
        eprintln!();
        eprintln!("  ✗ Usage: search <query>");
        eprintln!("    Type 'help search' for details.");
        eprintln!();
        return;
    }

    let search_text = format!("find {}", args);
    match node.process_input(&search_text).await {
        Ok(text) => {
            println!();
            println!("  {}", text.replace('\n', "\n  "));
            println!();
        }
        Err(e) => {
            eprintln!("  ✗ Search failed: {}", e);
            println!();
        }
    }
}

pub(crate) fn cmd_list(node: &OneBrainNode, args: &str) {
    // Parse args: --page N --limit N --type T --sort S
    let mut page: usize = 1;
    let mut limit: usize = 15;
    let mut type_filter: Option<String> = None;
    let mut sort_by = "created".to_string();

    let parts: Vec<&str> = args.split_whitespace().collect();
    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "--page" if i + 1 < parts.len() => {
                page = parts[i + 1].parse().unwrap_or(1);
                i += 2;
            }
            "--limit" if i + 1 < parts.len() => {
                limit = parts[i + 1].parse().unwrap_or(15);
                i += 2;
            }
            "--type" if i + 1 < parts.len() => {
                type_filter = Some(parts[i + 1].to_string());
                i += 2;
            }
            "--sort" if i + 1 < parts.len() => {
                sort_by = parts[i + 1].to_string();
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
    }

    match node.list_kus(page, limit, type_filter.as_deref(), &sort_by) {
        Ok((items, total)) => {
            let total_pages = if total == 0 { 1 } else { total.div_ceil(limit) };
            println!();
            if items.is_empty() {
                println!("  No KUs found.");
            } else {
                let type_str = type_filter
                    .as_ref()
                    .map(|t| format!(" (type: {})", t))
                    .unwrap_or_default();
                println!(
                    "  ── Knowledge Units ({} total{}, page {}/{}) ──",
                    total, type_str, page, total_pages
                );
                println!(
                    "  {:>3}  {:<8} {:<5} {:<5} {:<10} {:<10} Preview",
                    "#", "Gene", "PoMV", "Trust", "Created", "CID"
                );
                for (idx_offset, item) in items.iter().enumerate() {
                    let idx = (page - 1) * limit + idx_offset + 1;
                    let created = format_timestamp(item.created);
                    let cid_short = short_cid(&item.cid_hex);
                    println!(
                        "  {:>3}. [{:<6}] {:.2}  {:.2}  {:<10} {}...  {}",
                        idx,
                        item.gene_type,
                        item.pomv,
                        item.trust,
                        created,
                        cid_short,
                        item.preview
                    );
                }
            }
            if total_pages > 1 && page < total_pages {
                println!();
                println!(
                    "  Page {}/{}. Use 'list --page {}' for next.",
                    page,
                    total_pages,
                    page + 1
                );
            }
            println!();
        }
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
        }
    }
}

pub(crate) fn cmd_detail(node: &OneBrainNode, args: &str) {
    let cid = args.trim();
    if cid.is_empty() {
        eprintln!();
        eprintln!("  ✗ Usage: detail <cid>");
        eprintln!("    Type 'help detail' for details.");
        eprintln!();
        return;
    }

    match node.get_ku(cid) {
        Ok(detail) => {
            println!();
            println!("  ══════════════════════════════════════════");
            println!("  KU Detail — {}", detail.cid_hex);
            println!("  ══════════════════════════════════════════");
            println!();
            println!("  Gene type:    {}", detail.gene_type);
            println!("  Created:      {}", format_timestamp(detail.created));
            println!("  Wire size:    {} bytes", detail.wire_size);
            println!("  Instructions: {}", detail.instruction_count);
            println!("  Confidence:   {:.0}%", detail.confidence * 100.0);
            println!();
            println!("  ── Trust & Legacy Local PoMV ──");
            println!("  Epistemic:    {}", detail.epistemic);
            println!("  Evidence:     {}", detail.evidence);
            println!("  Trust score:  {:.2}", detail.trust);
            println!("  PoMV scalar:  {:.2}", detail.pomv);
            println!("  Profile:      {}", detail.pomv_profile);
            println!("  Economic:     no (not vNext evidence, Outcome, Benefit, or reward)");
            let bd = &detail.pomv_breakdown;
            println!("    ├─ Metabolic:     {:.2}", bd.metabolic);
            println!("    ├─ Prediction:    {:.2}", bd.prediction);
            println!("    ├─ Entropy:       {:.2}", bd.entropy);
            println!("    ├─ Survival:      {:.2}", bd.survival);
            println!("    ├─ Centrality:    {:.2}", bd.centrality);
            println!("    └─ Niche:         {:.2}", bd.niche);
            println!();
            println!("  Verification: {}", detail.verification_status);
            println!();

            // Codons
            if !detail.codons.is_empty() {
                println!("  ── Codons (Concepts) ──");
                let codon_strs: Vec<String> = detail
                    .codons
                    .iter()
                    .map(|c| format!("[{}] ({})", c.name, c.role))
                    .collect();
                // Print codons in rows of ~3
                for chunk in codon_strs.chunks(3) {
                    println!("  {}", chunk.join("  "));
                }
                println!();
            }

            // Content
            println!("  ── Content ──");
            for line in detail.content.lines() {
                println!("  {}", line);
            }
            println!();

            // Bonds
            if !detail.bonds.is_empty() {
                println!(
                    "  ── Bonds ({} outgoing, {} incoming) ──",
                    detail.outgoing_bond_count, detail.incoming_bond_count
                );
                for bond in &detail.bonds {
                    let arrow = if bond.direction == "OUT" {
                        "→"
                    } else {
                        "←"
                    };
                    let dir_label = if bond.direction == "OUT" {
                        "OUT →"
                    } else {
                        "IN  ←"
                    };
                    let other_short = short_cid(&bond.other_cid);
                    println!(
                        "  {} [{}] {} {}  CID: {}...  w: {:.2}",
                        dir_label,
                        bond.relation,
                        arrow,
                        bond.other_preview,
                        other_short,
                        bond.weight
                    );
                }
                println!();
            }
        }
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
        }
    }
}

pub(crate) async fn cmd_delete(
    node: &mut OneBrainNode,
    args: &str,
    reader: &mut BufReader<tokio::io::Stdin>,
) {
    let cid = args.trim();
    if cid.is_empty() {
        eprintln!();
        eprintln!("  ✗ Usage: delete <cid>");
        eprintln!();
        return;
    }

    // Show what will be deleted
    match node.get_ku(cid) {
        Ok(detail) => {
            println!();
            println!(
                "  ⚠ This will delete KU [{}...] from LOCAL storage.",
                short_cid(&detail.cid_hex)
            );
            println!(
                "    Gene: {} | \"{}\"",
                detail.gene_type,
                if detail.content.chars().count() > 60 {
                    format!("{}...", detail.content.chars().take(57).collect::<String>())
                } else {
                    detail.content.clone()
                }
            );
            println!("    Other nodes may still have copies.");
        }
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
            return;
        }
    };

    // Ask for confirmation
    eprint!("  Confirm delete? (y/N): ");
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

    match node.delete_ku(cid) {
        Ok(_deleted) => {
            println!("  ✓ Deleted from local storage.");
            println!();
        }
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
        }
    }
}

pub(crate) fn cmd_kql(node: &OneBrainNode, args: &str) {
    if args.is_empty() {
        eprintln!();
        eprintln!("  ✗ Usage: kql <query>");
        eprintln!("    Type 'help kql' for syntax and examples.");
        eprintln!();
        return;
    }

    match node.execute_kql(args) {
        Ok(items) => {
            println!();
            if items.is_empty() {
                println!("  ── KQL Results (0 matches) ──");
                println!("  No matching KUs found.");
            } else {
                println!("  ── KQL Results ({} matches) ──", items.len());
                for (i, item) in items.iter().enumerate() {
                    let cid_short = short_cid(&item.cid_hex);
                    println!(
                        "  {}. [{}] {}  trust: {:.2}  pomv: {:.2}  CID: {}...",
                        i + 1,
                        item.gene_type,
                        item.preview,
                        item.trust,
                        item.pomv,
                        cid_short
                    );
                }
            }
            println!();
        }
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
        }
    }
}

pub(crate) fn cmd_graph(node: &OneBrainNode, args: &str) {
    // Parse: <cid> [--depth N]
    let parts: Vec<&str> = args.split_whitespace().collect();
    if parts.is_empty() {
        eprintln!();
        eprintln!("  ✗ Usage: graph <cid> [--depth N]");
        eprintln!("    Type 'help graph' for details.");
        eprintln!();
        return;
    }

    let cid = parts[0];
    let mut depth: usize = 1;
    let mut i = 1;
    while i < parts.len() {
        if parts[i] == "--depth" && i + 1 < parts.len() {
            depth = parts[i + 1].parse().unwrap_or(1).min(3);
            i += 2;
        } else {
            i += 1;
        }
    }

    match node.get_neighbors(cid, depth as u32) {
        Ok(neighbors) => {
            println!();
            println!(
                "  ── Knowledge Graph: {}... (depth={}) ──",
                short_cid(cid),
                depth
            );
            println!();

            // Print root
            println!("  ● [{}] (root)", short_cid(cid),);

            // Print neighbors as tree
            let neighbor_count = neighbors.len();
            if neighbor_count == 0 {
                println!("    (no bonds found)");
            }
            for (idx, neighbor) in neighbors.iter().enumerate() {
                let is_last = idx == neighbor_count - 1;
                print_graph_neighbor(neighbor, "", is_last);
            }

            println!();
            println!("  ● = in local storage  ○ = CID only (not synced)");
            println!(
                "  Nodes: {}  |  Edges: {}  |  Max depth: {}",
                1 + count_graph_items(&neighbors),
                count_graph_items(&neighbors),
                depth
            );
            println!();
        }
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
        }
    }
}

/// Recursively print a graph neighbor with tree indentation.
fn print_graph_neighbor(neighbor: &NeighborInfo, prefix: &str, is_last: bool) {
    let connector = if is_last { "└──" } else { "├──" };
    let arrow = if neighbor.direction == "OUT" {
        "→"
    } else {
        "←"
    };
    let marker = if neighbor.is_local { "●" } else { "○" };

    println!(
        "  {}{} {} [{}] {} {} [{}] {} ({}, PoMV: {:.2})",
        prefix,
        connector,
        arrow,
        neighbor.relation,
        arrow,
        marker,
        short_cid(&neighbor.cid_hex),
        neighbor.preview,
        neighbor.gene_type,
        neighbor.pomv
    );

    // Print children
    let child_prefix = format!("{}{}   ", prefix, if is_last { " " } else { "│" });
    let child_count = neighbor.children.len();
    for (idx, child) in neighbor.children.iter().enumerate() {
        let child_is_last = idx == child_count - 1;
        print_graph_neighbor(child, &child_prefix, child_is_last);
    }
}

pub(crate) fn cmd_deprecate(node: &mut OneBrainNode, args: &str) {
    if args.is_empty() {
        eprintln!();
        eprintln!("  ✗ Usage: deprecate <cid>");
        eprintln!("    Mark a KU as obsolete (keeps it in storage, unlike delete).");
        eprintln!();
        return;
    }

    match node.deprecate_ku(args.trim()) {
        Ok(true) => {
            println!();
            println!("  ✓ KU marked as deprecated (obsolete)");
            println!("  CID: {}", args.trim());
            println!("  Note: KU is still in storage but marked as obsolete.");
            println!("        Other nodes may still have copies.");
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

pub(crate) async fn cmd_edit(
    node: &mut OneBrainNode,
    args: &str,
    reader: &mut BufReader<tokio::io::Stdin>,
) {
    if args.is_empty() {
        eprintln!();
        eprintln!("  ✗ Usage: edit <cid>");
        eprintln!("    Create a new version of an existing KU.");
        eprintln!();
        return;
    }

    let cid_hex = args.trim();

    // Get existing KU content
    let detail = match node.get_ku(cid_hex) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
            return;
        }
    };

    println!();
    println!("  ── Current KU Content ──");
    println!("  Gene type: {}", detail.gene_type);
    println!("  Content:");
    println!("  {}", detail.content.replace('\n', "\n  "));
    println!();
    println!("  Enter new content (or press Enter to cancel):");
    eprint!("  > ");

    let mut new_content = String::new();
    let bytes_read = reader.read_line(&mut new_content).await.unwrap_or(0);

    if bytes_read == 0 || new_content.trim().is_empty() {
        println!("  Cancelled.");
        println!();
        return;
    }

    let new_text = new_content.trim();

    // Encode as new KU (TODO: set prev_cid to original)
    match node.encode_and_store(new_text).await {
        Ok(result) => {
            let new_cid_hex: String = result.cid.iter().map(|b| format!("{:02x}", b)).collect();
            println!();
            println!("  ✓ New version created");
            println!("  New CID:      {}", new_cid_hex);
            println!("  Previous CID: {}", cid_hex);
            if let Some(ref gt) = result.gene_type {
                println!("  Gene type:    {}", gt);
            }
            println!("  Confidence:   {:.0}%", result.confidence * 100.0);
            println!("  Note: prev_cid link is STUB — bond to original not yet created.");
            println!();
        }
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
        }
    }
}

pub(crate) async fn cmd_bulk_delete(
    node: &OneBrainNode,
    args: &str,
    reader: &mut BufReader<tokio::io::Stdin>,
) {
    // Parse args: --gene TYPE --before TIMESTAMP
    let mut gene_filter: Option<String> = None;
    let mut before: Option<u64> = None;

    let parts: Vec<&str> = args.split_whitespace().collect();
    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "--gene" | "--type" => {
                if i + 1 < parts.len() {
                    gene_filter = Some(parts[i + 1].to_string());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--before" if i + 1 < parts.len() => {
                before = parts[i + 1].parse().ok();
                i += 2;
            }
            "--before" => {
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    if gene_filter.is_none() && before.is_none() {
        eprintln!();
        eprintln!("  ✗ Usage: delete --gene <type> [--before <timestamp>]");
        eprintln!("    Bulk delete KUs matching filter.");
        eprintln!("    Example: delete --gene hypothesis --before 1720000000");
        eprintln!();
        return;
    }

    // Confirmation
    println!();
    println!("  ⚠ Bulk delete with filters:");
    if let Some(ref g) = gene_filter {
        println!("    Gene type: {}", g);
    }
    if let Some(b) = before {
        println!("    Before:    {}", format_timestamp(b));
    }
    eprint!("  Are you sure? [y/N] ");

    let mut confirm = String::new();
    let _ = reader.read_line(&mut confirm).await;
    if confirm.trim().to_lowercase() != "y" {
        println!("  Cancelled.");
        println!();
        return;
    }

    match node.bulk_delete(gene_filter.as_deref(), before) {
        Ok(result) => {
            println!();
            println!("  ✓ Bulk delete complete");
            println!("  Deleted: {}", result.deleted);
            println!("  Skipped: {}", result.skipped);
            println!();
        }
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
        }
    }
}
