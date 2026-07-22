//! Identity and multi-device commands: identity, recover, devices, sync, profile.

use onebrain_node::node::OneBrainNode;
use tokio::io::{AsyncBufReadExt, BufReader};

use super::helpers::*;

pub(crate) fn cmd_identity(node: &OneBrainNode) {
    match node.get_identity_info() {
        Ok(info) => {
            println!();
            println!("  ── Identity ──");
            println!("  NodeId:       {}", info.node_id);
            println!("  Display name: {}", info.name);
            println!("  Created:      {}", format_timestamp(info.created));
            println!();
            println!("  ── Device Group ──");
            println!("  Devices:      {}/{}", info.device_count, info.max_devices);
            println!();
            println!("  ── Trust ──");
            println!(
                "  Tier:         {} (score: {:.2})",
                info.tier, info.trust_score
            );
            // Simple progress bar for trust
            let progress = (info.trust_score * 16.0) as usize;
            let progress_bar = format!(
                "{}{}",
                "█".repeat(progress),
                "░".repeat(16_usize.saturating_sub(progress))
            );
            println!(
                "  Progress:     {} {:.0}%",
                progress_bar,
                info.trust_score * 100.0
            );
            println!();
            println!("  ── Statistics ──");
            println!("  KUs encoded:  {}", info.kus_encoded);
            println!("  KUs received: {}", info.kus_received);
            println!("  Queries:      {}", info.total_queries);
            println!();
        }
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
        }
    }
}

pub(crate) async fn cmd_recover(node: &mut OneBrainNode, reader: &mut BufReader<tokio::io::Stdin>) {
    println!();
    println!("  ⚠ This will REPLACE the current identity on this device.");

    // Show current identity if available
    if let Ok(info) = node.get_identity_info() {
        println!("    Current NodeId: {}...", short_cid(&info.node_id));
    }

    // Confirm
    println!();
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

    // Read the recovery phrase
    println!();
    eprint!("  Enter your 24-word recovery phrase:\n  > ");
    let mut phrase = String::new();
    if reader.read_line(&mut phrase).await.is_err() {
        eprintln!("  ✗ Failed to read input.");
        println!();
        return;
    }
    let phrase = phrase.trim();

    if phrase.is_empty() {
        eprintln!("  ✗ Recovery phrase cannot be empty.");
        println!();
        return;
    }

    println!();
    println!("  Verifying phrase...");

    let words: Vec<String> = phrase.split_whitespace().map(|s| s.to_string()).collect();

    match node.recover_identity(&words, "") {
        Ok(identity) => {
            println!("  ✓ Valid BIP39 phrase");
            println!("  Deriving keypair... ✓");
            println!();
            println!("  ✓ Identity recovered!");
            println!("  NodeId: {}", identity.node_id);
            println!();
        }
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
        }
    }
}

pub(crate) fn cmd_devices(node: &mut OneBrainNode) {
    let devices = node.list_devices();

    println!();
    println!("  ── Devices ({}) ──", devices.len());
    println!(
        "  {:<16}  {:<20}  {:<8}  {:<12}  {:<6}  {}",
        "Device ID", "Name", "Type", "Last Seen", "KUs", "Status"
    );
    println!("  {}", "─".repeat(80));

    for dev in &devices {
        let short_id = if dev.device_id.len() >= 12 {
            &dev.device_id[..12]
        } else {
            &dev.device_id
        };
        let last_seen = format_timestamp(dev.last_seen);
        let status_icon = match dev.sync_status.as_str() {
            "up-to-date" => "🟢",
            "syncing" | "behind" => "🟡",
            _ => "🔴",
        };
        println!(
            "  {:<16}  {:<20}  {:<8}  {:<12}  {:<6}  {} {}",
            short_id,
            dev.name,
            dev.device_type,
            last_seen,
            dev.ku_count,
            status_icon,
            dev.sync_status
        );
    }
    println!();
}

pub(crate) fn cmd_sync(node: &mut OneBrainNode, args: &str) {
    let subcmd = args.split_whitespace().next().unwrap_or("status");

    match subcmd {
        "status" | "" => {
            let info = node.sync_status();

            let status_icon = match info.status.as_str() {
                "up-to-date" => "🟢",
                "syncing" => "🟡",
                _ => "🔴",
            };

            println!();
            println!("  ── Sync Status ──");
            println!("  Status:      {} {}", status_icon, info.status);
            println!("  Pending:     {} items", info.pending_count);
            println!("  Last sync:   {}", format_timestamp(info.last_sync));
            println!("  Devices:     {}", info.devices.len());
            println!();
        }
        _ => {
            eprintln!();
            eprintln!("  ✗ Usage: sync status");
            eprintln!("    Show multi-device sync status.");
            eprintln!();
        }
    }
}

pub(crate) fn cmd_profile(node: &mut OneBrainNode, args: &str) {
    if args.is_empty() {
        // View profile
        match node.get_profile() {
            Ok(profile) => {
                println!();
                println!("  ── User Profile ──");
                println!("  Display name:     {}", profile.name);
                println!("  Language:         {}", profile.language);
                println!("  Response style:   {}", profile.style);
                println!();

                if !profile.expertise.is_empty() {
                    println!("  ── Expertise ──");
                    for (i, exp) in profile.expertise.iter().enumerate() {
                        println!(
                            "  {}. {:<16} ({} KUs, active {})",
                            i + 1,
                            exp.domain,
                            exp.ku_count,
                            format_timestamp(exp.last_active)
                        );
                    }
                    println!();
                }

                println!("  ── Statistics ──");
                println!("  Total KUs:     {}", profile.total_kus);
                println!("  Total queries: {}", profile.total_queries);
                println!(
                    "  Member since:  {}",
                    format_timestamp(profile.member_since)
                );
                println!();
            }
            Err(e) => {
                eprintln!("  ✗ {}", e);
                println!();
            }
        }
    } else if let Some(rest) = args.strip_prefix("set ") {
        // profile set <field> <value>
        let set_parts: Vec<&str> = rest.splitn(2, ' ').collect();
        if set_parts.len() < 2 {
            eprintln!();
            eprintln!("  ✗ Usage: profile set <field> <value>");
            eprintln!("    Fields: name, language, style");
            eprintln!();
            return;
        }
        let field = set_parts[0].trim();
        let value = set_parts[1].trim().trim_matches('"');

        match node.update_profile(field, value) {
            Ok(()) => {
                println!("  ✓ {} updated to \"{}\"", field, value);
                // Extra hint for style
                if field == "style" {
                    println!("    Options: concise, balanced, detailed, academic");
                }
                println!();
            }
            Err(e) => {
                eprintln!("  ✗ {}", e);
                println!();
            }
        }
    } else {
        eprintln!();
        eprintln!("  ✗ Unknown profile subcommand. Usage:");
        eprintln!("    profile               View profile");
        eprintln!("    profile set <f> <v>    Update field");
        eprintln!();
    }
}
