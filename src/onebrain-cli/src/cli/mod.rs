//! Interactive REPL for the OneBrain node.
//!
//! Full command set: encode, search, list, detail, delete, kql, graph,
//! connect, status, peers, identity, recover, profile, model, wallet,
//! export, import, backup, restore, config, help, quit.

mod ai;
mod blob;
mod config;
mod data;
mod help;
pub mod helpers;
mod identity;
mod knowledge;
mod network;
mod social;
mod tags;
#[cfg(test)]
mod tests;
pub(crate) mod vnext;
mod wallet;
mod workflow;

use onebrain_node::error::NodeError;
use onebrain_node::network::NodeEvent;
use onebrain_node::node::OneBrainNode;

use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Mutex;

// ═══════════════════════════════════════════════════════════════════════════
// Event draining
// ═══════════════════════════════════════════════════════════════════════════

/// Drain pending network events and display notifications.
fn drain_and_display_events(node: &mut OneBrainNode) {
    let events = node.drain_events();
    for event in events {
        match event {
            NodeEvent::PeerConnected(info) => {
                eprintln!(
                    "  🔗 Peer connected: '{}' at {} ({} KUs)",
                    info.name, info.addr, info.ku_count
                );
            }
            NodeEvent::KuReceived { cid_hex, from, .. } => {
                eprintln!(
                    "  📥 Received KU from {}: [{}...]",
                    from,
                    &cid_hex[..std::cmp::min(16, cid_hex.len())]
                );
            }
            NodeEvent::VerifyResult {
                cid_hex,
                agreement_score,
                verified,
                from,
            } => {
                let icon = if verified { "✅" } else { "❌" };
                eprintln!(
                    "  {} Verification from {}: [{}...] agreement={:.0}%",
                    icon,
                    from,
                    &cid_hex[..std::cmp::min(16, cid_hex.len())],
                    agreement_score * 100.0
                );
            }
            NodeEvent::Notification(msg) => {
                eprintln!("{}", msg);
            }
            NodeEvent::EncodeProgress {
                step,
                total_steps,
                message,
            } => {
                eprintln!("  ⚙ [{}/{}] {}", step, total_steps, message);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Unified command dispatch
// ═══════════════════════════════════════════════════════════════════════════

/// Dispatch a single parsed command to the appropriate handler.
///
/// This is the **single source of truth** for the command→handler mapping,
/// eliminating the duplicated match arms that previously existed in both
/// `run_repl` and `run_repl_shared`.
async fn dispatch(
    node: &mut OneBrainNode,
    cmd: &str,
    args: &str,
    full_input: &str,
    reader: &mut BufReader<tokio::io::Stdin>,
) {
    match cmd {
        // Knowledge
        "encode" | "remember" => knowledge::cmd_encode(node, args, reader).await,
        "search" | "find" => knowledge::cmd_search(node, args).await,
        "list" => knowledge::cmd_list(node, args),
        "detail" => knowledge::cmd_detail(node, args),
        "delete" => {
            if args.contains("--gene") || args.contains("--before") || args.contains("--type") {
                knowledge::cmd_bulk_delete(node, args, reader).await;
            } else {
                knowledge::cmd_delete(node, args, reader).await;
            }
        }
        "kql" => knowledge::cmd_kql(node, args),
        "graph" => knowledge::cmd_graph(node, args),
        "deprecate" => knowledge::cmd_deprecate(node, args),
        "edit" => knowledge::cmd_edit(node, args, reader).await,

        // Network
        "connect" => network::cmd_connect(node, args).await,
        "status" => network::cmd_status(node).await,
        "peers" => network::cmd_peers(node),
        "workflow" | "vnext" => workflow::cmd_workflow(args),

        // Social
        "follow" => social::cmd_follow(node, args),
        "unfollow" => social::cmd_unfollow(node, args),
        "following" => social::cmd_following(node),
        "peer-info" => social::cmd_peer_info(node, args),
        "share" => social::cmd_share(node, args),

        // Identity
        "identity" => identity::cmd_identity(node),
        "recover" => identity::cmd_recover(node, reader).await,

        // Multi-Device
        "devices" => identity::cmd_devices(node),
        "sync" => identity::cmd_sync(node, args),

        // Profile & AI
        "profile" => identity::cmd_profile(node, args),
        "model" => ai::cmd_model(node, args).await,

        // Wallet
        "wallet" => wallet::cmd_wallet(node, args),

        // Blob storage
        "blob" => blob::cmd_blob(node, args, reader).await,

        // Tags & Pin
        "tag" => tags::cmd_tag(node, args),
        "pin" => tags::cmd_pin(node, args),
        "unpin" => tags::cmd_unpin(node, args),

        // Watch
        "watch" => tags::cmd_watch(node, args),

        // Data
        "export" => data::cmd_export(node, args),
        "import" => data::cmd_import(node, args).await,
        "backup" => data::cmd_backup(node, args, reader).await,
        "restore" => data::cmd_restore(node, args, reader).await,

        // Config
        "config" => config::cmd_config(node, args),

        // Free chat — send as-is to mediator
        _ => ai::cmd_chat(node, full_input).await,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// REPL loops
// ═══════════════════════════════════════════════════════════════════════════

/// Run the interactive REPL loop.
pub async fn run_repl(node: &mut OneBrainNode) -> Result<(), NodeError> {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    loop {
        // Check for pending network events before showing prompt
        drain_and_display_events(node);

        // Print prompt (using eprint to avoid buffering issues with stdout)
        eprint!("{}> ", node.node_name());

        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;

        // EOF
        if bytes_read == 0 {
            println!("\nGoodbye!");
            break;
        }

        let trimmed = line.trim();

        // Skip empty lines
        if trimmed.is_empty() {
            continue;
        }

        // Parse and dispatch commands
        let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
        let cmd = parts[0].to_lowercase();
        let args = parts.get(1).map(|s| s.trim()).unwrap_or("");

        match cmd.as_str() {
            "quit" | "exit" => {
                println!("Goodbye!");
                break;
            }
            "help" => help::cmd_help(args),
            _ => dispatch(node, &cmd, args, trimmed, &mut reader).await,
        }
    }

    Ok(())
}

/// Run the interactive REPL loop using a shared `Arc<Mutex<OneBrainNode>>`.
///
/// Unlike `run_repl`, this variant acquires the Mutex lock **only during
/// command execution** and releases it between commands.  This allows the
/// API server (which shares the same `Arc<Mutex>`) to serve web-dashboard
/// requests while the REPL is waiting for user input.
pub async fn run_repl_shared(shared_node: Arc<Mutex<OneBrainNode>>) -> Result<(), NodeError> {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    loop {
        // Briefly lock to drain events and get the node name for the prompt
        {
            let mut node = shared_node.lock().await;
            drain_and_display_events(&mut node);
            eprint!("{}> ", node.node_name());
        } // ← lock released here — API can serve requests while we wait for input

        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;

        if bytes_read == 0 {
            println!("\nGoodbye!");
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
        let cmd = parts[0].to_lowercase();
        let args_str = parts.get(1).map(|s| s.trim()).unwrap_or("").to_string();
        let full_input = trimmed.to_string();

        match cmd.as_str() {
            "quit" | "exit" => {
                println!("Goodbye!");
                break;
            }
            "help" => help::cmd_help(&args_str),
            _ => {
                // Lock for the duration of the command, then release
                let mut node = shared_node.lock().await;
                dispatch(&mut node, &cmd, &args_str, &full_input, &mut reader).await;
            }
        }
    }

    Ok(())
}
