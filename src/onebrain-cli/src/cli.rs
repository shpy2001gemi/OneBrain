//! Interactive REPL for the OneBrain node.
//!
//! Full command set: encode, search, list, detail, delete, kql, graph,
//! connect, status, peers, identity, recover, profile, model, wallet,
//! export, import, backup, restore, config, help, quit.

use onebrain_node::error::NodeError;
use onebrain_node::network::NodeEvent;
use onebrain_node::node::OneBrainNode;
use onebrain_node::types::*;

use tokio::io::{AsyncBufReadExt, BufReader};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

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
            "help" => cmd_help(args),

            // Knowledge
            "encode" | "remember" => cmd_encode(node, args, &mut reader).await,
            "search" | "find" => cmd_search(node, args).await,
            "list" => cmd_list(node, args),
            "detail" => cmd_detail(node, args),
            "delete" => {
                if args.contains("--gene") || args.contains("--before") || args.contains("--type") {
                    cmd_bulk_delete(node, args, &mut reader).await;
                } else {
                    cmd_delete(node, args, &mut reader).await;
                }
            }
            "kql" => cmd_kql(node, args),
            "graph" => cmd_graph(node, args),
            "deprecate" => cmd_deprecate(node, args),
            "edit" => cmd_edit(node, args, &mut reader).await,

            // Network
            "connect" => cmd_connect(node, args).await,
            "status" => cmd_status(node).await,
            "peers" => cmd_peers(node),

            // Social
            "follow" => cmd_follow(node, args),
            "unfollow" => cmd_unfollow(node, args),
            "following" => cmd_following(node),
            "peer-info" => cmd_peer_info(node, args),
            "share" => cmd_share(node, args),

            // Identity
            "identity" => cmd_identity(node),
            "recover" => cmd_recover(node, &mut reader).await,

            // Multi-Device
            "devices" => cmd_devices(node),
            "sync" => cmd_sync(node, args),

            // Profile & AI
            "profile" => cmd_profile(node, args),
            "model" => cmd_model(node, args).await,

            // Wallet
            "wallet" => cmd_wallet(node, args),

            // Blob storage
            "blob" => cmd_blob(node, args, &mut reader).await,

            // Tags & Pin
            "tag" => cmd_tag(node, args),
            "pin" => cmd_pin(node, args),
            "unpin" => cmd_unpin(node, args),

            // Watch
            "watch" => cmd_watch(node, args),

            // Data
            "export" => cmd_export(node, args),
            "import" => cmd_import(node, args).await,
            "backup" => cmd_backup(node, args, &mut reader).await,
            "restore" => cmd_restore(node, args, &mut reader).await,

            // Config
            "config" => cmd_config(node, args),

            // Free chat — send as-is to mediator
            _ => cmd_chat(node, trimmed).await,
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

        match cmd.as_str() {
            "quit" | "exit" => {
                println!("Goodbye!");
                break;
            }
            "help" => cmd_help(&args_str),
            _ => {
                // Lock for the duration of the command, then release
                let mut node = shared_node.lock().await;
                match cmd.as_str() {
                    "encode" | "remember" => cmd_encode(&mut node, &args_str, &mut reader).await,
                    "search" | "find" => cmd_search(&mut node, &args_str).await,
                    "list" => cmd_list(&node, &args_str),
                    "detail" => cmd_detail(&node, &args_str),
                    "delete" => {
                        if args_str.contains("--gene") || args_str.contains("--before") || args_str.contains("--type") {
                            cmd_bulk_delete(&node, &args_str, &mut reader).await;
                        } else {
                            cmd_delete(&mut node, &args_str, &mut reader).await;
                        }
                    }
                    "kql" => cmd_kql(&node, &args_str),
                    "graph" => cmd_graph(&node, &args_str),
                    "deprecate" => cmd_deprecate(&node, &args_str),
                    "edit" => cmd_edit(&mut node, &args_str, &mut reader).await,
                    "connect" => cmd_connect(&mut node, &args_str).await,
                    "status" => cmd_status(&node).await,
                    "peers" => cmd_peers(&node),
                    "follow" => cmd_follow(&node, &args_str),
                    "unfollow" => cmd_unfollow(&node, &args_str),
                    "following" => cmd_following(&node),
                    "peer-info" => cmd_peer_info(&node, &args_str),
                    "share" => cmd_share(&node, &args_str),
                    "identity" => cmd_identity(&node),
                    "recover" => cmd_recover(&mut node, &mut reader).await,
                    "devices" => cmd_devices(&node),
                    "sync" => cmd_sync(&node, &args_str),
                    "profile" => cmd_profile(&mut node, &args_str),
                    "model" => cmd_model(&mut node, &args_str).await,
                    "wallet" => cmd_wallet(&node, &args_str),
                    "blob" => cmd_blob(&mut node, &args_str, &mut reader).await,
                    "tag" => cmd_tag(&node, &args_str),
                    "pin" => cmd_pin(&node, &args_str),
                    "unpin" => cmd_unpin(&node, &args_str),
                    "watch" => cmd_watch(&node, &args_str),
                    "export" => cmd_export(&node, &args_str),
                    "import" => cmd_import(&mut node, &args_str).await,
                    "backup" => cmd_backup(&mut node, &args_str, &mut reader).await,
                    "restore" => cmd_restore(&mut node, &args_str, &mut reader).await,
                    "config" => cmd_config(&mut node, &args_str),
                    _ => cmd_chat(&mut node, trimmed).await,
                }
            }
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// Helper functions
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
            NodeEvent::KuReceived {
                cid_hex, from, ..
            } => {
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
            NodeEvent::EncodeProgress { step, total_steps, message } => {
                eprintln!("  ⚙ [{}/{}] {}", step, total_steps, message);
            }
        }
    }
}

/// Format an epoch timestamp as a human-readable relative time string.
fn format_timestamp(epoch_secs: u64) -> String {
    if epoch_secs == 0 {
        return "--".to_string();
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let diff = now.saturating_sub(epoch_secs);
    if diff < 60 {
        format!("{}s ago", diff)
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86400)
    }
}

/// Format milliOBT into a human-readable OBT string.
fn format_obt(milliobt: u64) -> String {
    format!("{},{:03}.{:03} OBT", milliobt / 1_000_000, (milliobt / 1000) % 1000, milliobt % 1000)
}

/// Format milliOBT into a shorter form (no leading group).
fn format_obt_short(milliobt: u64) -> String {
    let whole = milliobt / 1000;
    let frac = milliobt % 1000;
    format!("{}.{:03} OBT", whole, frac)
}

/// Format a signed milliOBT amount (for transaction history).
fn format_obt_signed(milliobt: i64) -> String {
    let sign = if milliobt >= 0 { "+" } else { "-" };
    let abs = milliobt.unsigned_abs();
    let whole = abs / 1000;
    let frac = abs % 1000;
    format!("{}{}.{:03} OBT", sign, whole, frac)
}

/// Generate a simple horizontal bar chart string.
fn bar_chart(value: u64, max: u64, width: usize) -> String {
    let filled = if max > 0 {
        (value as f64 / max as f64 * width as f64) as usize
    } else {
        0
    };
    let empty = width.saturating_sub(filled);
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

/// Truncate CID hex to a short display form.
fn short_cid(cid_hex: &str) -> &str {
    if cid_hex.len() >= 8 {
        &cid_hex[..8]
    } else {
        cid_hex
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// cmd_help — Show help text (general or per-command)
// ═══════════════════════════════════════════════════════════════════════════

fn cmd_help(args: &str) {
    if args.is_empty() {
        println!();
        println!("  ╔═══════════════════════════════════════════════════════════════╗");
        println!("  ║                    OneBrain Commands                         ║");
        println!("  ╠═══════════════════════════════════════════════════════════════╣");
        println!("  ║                                                               ║");
        println!("  ║  ── Knowledge ──                                              ║");
        println!("  ║  encode <text>           Encode knowledge into KU             ║");
        println!("  ║  encode --draft <text>   Save as draft (no broadcast)         ║");
        println!("  ║  encode --attach <f> ... Encode with file attachments         ║");
        println!("  ║  search <query>          Search your knowledge base           ║");
        println!("  ║  list [--type T]         Browse all KUs                       ║");
        println!("  ║  detail <cid>            View KU details                      ║");
        println!("  ║  delete <cid>            Delete KU from local storage         ║");
        println!("  ║  delete --gene T         Bulk delete by gene type             ║");
        println!("  ║  deprecate <cid>         Mark KU as obsolete                  ║");
        println!("  ║  edit <cid>              Create new version of KU             ║");
        println!("  ║  kql <query>             Execute KQL query                    ║");
        println!("  ║  graph <cid>             View knowledge graph (text tree)     ║");
        println!("  ║                                                               ║");
        println!("  ║  ── Tags & Pins ──                                            ║");
        println!("  ║  tag add <cid> <tag>     Add tag to KU                        ║");
        println!("  ║  tag remove <cid> <tag>  Remove tag from KU                   ║");
        println!("  ║  tag list                List all tags                         ║");
        println!("  ║  pin [<cid>]             Pin KU / show pinned                 ║");
        println!("  ║  unpin <cid>             Unpin KU                             ║");
        println!("  ║                                                               ║");
        println!("  ║  ── Network ──                                                ║");
        println!("  ║  connect <ip:port>       Connect to peer                      ║");
        println!("  ║  peers                   Show connected peers                 ║");
        println!("  ║  status                  Show node status                     ║");
        println!("  ║                                                               ║");
        println!("  ║  ── Social ──                                                 ║");
        println!("  ║  follow <node_id>        Follow a node                        ║");
        println!("  ║  unfollow <node_id>      Unfollow a node                      ║");
        println!("  ║  following               List followed nodes                  ║");
        println!("  ║  peer-info <node_id>     View node profile                    ║");
        println!("  ║  share <cid>             Share KU via link                    ║");
        println!("  ║                                                               ║");
        println!("  ║  ── Identity & Profile ──                                     ║");
        println!("  ║  identity                Show identity info                   ║");
        println!("  ║  recover                 Recover from BIP39 phrase            ║");
        println!("  ║  profile                 View/edit profile                    ║");
        println!("  ║                                                               ║");
        println!("  ║  ── Multi-Device ──                                           ║");
        println!("  ║  devices                 List devices in group                ║");
        println!("  ║  sync status             Show sync status                     ║");
        println!("  ║                                                               ║");
        println!("  ║  ── AI ──                                                     ║");
        println!("  ║  model list              Show available AI models             ║");
        println!("  ║  model switch <name>     Switch AI model                      ║");
        println!("  ║  model test              Test AI connection                   ║");
        println!("  ║                                                               ║");
        println!("  ║  ── Wallet ──                                                 ║");
        println!("  ║  wallet                  Show OBT balance                     ║");
        println!("  ║  wallet history          Transaction history                  ║");
        println!("  ║                                                               ║");
        println!("  ║  ── Data ──                                                   ║");
        println!("  ║  export [--format json]  Export KUs to file                   ║");
        println!("  ║  import <file>           Import file into knowledge base      ║");
        println!("  ║  backup                  Full encrypted backup                ║");
        println!("  ║  restore <file>          Restore from backup                  ║");
        println!("  ║                                                               ║");
        println!("  ║  ── Blob ──                                                   ║");
        println!("  ║  blob list               List stored blobs                    ║");
        println!("  ║  blob store <file>       Store a file as blob                 ║");
        println!("  ║  blob detail <cid>       View blob detail                     ║");
        println!("  ║  blob export <cid>       Export blob to file                  ║");
        println!("  ║  blob delete <cid>       Delete a blob                        ║");
        println!("  ║  blob pin <cid>          Pin blob (prevent GC)                ║");
        println!("  ║  blob unpin <cid>        Unpin blob                           ║");
        println!("  ║  blob stats              Storage stats                        ║");
        println!("  ║  blob gc                 Garbage collect                      ║");
        println!("  ║                                                               ║");
        println!("  ║  ── Watch ──                                                  ║");
        println!("  ║  watch create <kql>      Create standing query                ║");
        println!("  ║  watch list              List active watches                  ║");
        println!("  ║  watch delete <id>       Delete a watch                       ║");
        println!("  ║                                                               ║");
        println!("  ║  ── Config ──                                                 ║");
        println!("  ║  config                  Show configuration                   ║");
        println!("  ║  config set <key> <val>  Update configuration                 ║");
        println!("  ║                                                               ║");
        println!("  ║  ── System ──                                                 ║");
        println!("  ║  help [command]          Show help (or help for command)      ║");
        println!("  ║  quit / exit             Exit the node                        ║");
        println!("  ║                                                               ║");
        println!("  ║  Any other text → chat with AI (Mediator)                    ║");
        println!("  ╚═══════════════════════════════════════════════════════════════╝");
        println!();
    } else {
        // Per-command help
        match args.trim() {
            "encode" | "remember" => {
                println!();
                println!("  encode <text>");
                println!("  remember <text>  (alias)");
                println!();
                println!("  Encode text into a Knowledge Unit (KU) and publish to the network.");
                println!();
                println!("  Pipeline: Text → AI analysis → Gene extraction → Bond creation");
                println!("            → CID calculation → Store → Broadcast → Verify");
                println!();
                println!("  Rate limits:");
                println!("    Leaf:        1 KU/hour");
                println!("    Contributor: 5 KU/hour");
                println!("    LocalSP+:   10 KU/hour");
                println!();
                println!("  Quality requirements:");
                println!("    Min text length: 256 bytes");
                println!("    Min genes:       2");
                println!("    Min bonds:       1");
                println!();
                println!("  Examples:");
                println!("    encode Einstein developed special relativity in 1905");
                println!("    encode Cách nấu phở: Bước 1: Ninh xương bò 8 tiếng...");
                println!("    remember The mitochondria is the powerhouse of the cell");
                println!();
            }
            "search" | "find" => {
                println!();
                println!("  search <query>");
                println!("  find <query>  (alias)");
                println!();
                println!("  Search KUs using semantic search (AI) + keyword matching.");
                println!("  Results are ranked by relevance score.");
                println!();
                println!("  Examples:");
                println!("    search thuyết tương đối");
                println!("    find how to cook pho");
                println!();
            }
            "list" => {
                println!();
                println!("  list [--page N] [--limit N] [--type TYPE] [--sort FIELD]");
                println!();
                println!("  Browse all KUs in local storage.");
                println!();
                println!("  Options:");
                println!("    --page N    Page number (default: 1)");
                println!("    --limit N   Items per page (default: 15)");
                println!("    --type T    Filter: fact, procedure, experience, creative, hypothesis");
                println!("    --sort F    Sort by: created (default), pomv, trust");
                println!();
                println!("  Examples:");
                println!("    list");
                println!("    list --type fact --sort pomv");
                println!("    list --page 2 --limit 20");
                println!();
            }
            "detail" => {
                println!();
                println!("  detail <cid>");
                println!();
                println!("  View full details of a Knowledge Unit by CID.");
                println!("  Shows gene type, codons, bonds, PoMV breakdown, and content.");
                println!();
                println!("  Example:");
                println!("    detail a1b2c3d4");
                println!();
            }
            "delete" => {
                println!();
                println!("  delete <cid>");
                println!();
                println!("  Delete a KU from local storage.");
                println!("  Requires confirmation. Other nodes may still have copies.");
                println!();
                println!("  Example:");
                println!("    delete a1b2c3d4");
                println!();
            }
            "kql" => {
                println!();
                println!("  kql <query>");
                println!();
                println!("  Execute a KQL (Knowledge Query Language) query.");
                println!();
                println!("  Syntax:");
                println!("    FIND <pattern> WHERE <condition> [ORDER BY <field>] [LIMIT N]");
                println!();
                println!("  Patterns: facts, procedures, experiences, creatives, hypotheses");
                println!();
                println!("  Examples:");
                println!("    kql FIND facts WHERE trust > 0.8");
                println!("    kql FIND procedures WHERE codons CONTAINS \"nấu\"");
                println!("    kql FIND facts WHERE trust > 0.5 ORDER BY pomv DESC LIMIT 10");
                println!();
            }
            "graph" => {
                println!();
                println!("  graph <cid> [--depth N]");
                println!();
                println!("  View knowledge graph neighbors as a text tree.");
                println!();
                println!("  Options:");
                println!("    --depth N   Traversal depth (default: 1, max: 3)");
                println!();
                println!("  Legend:");
                println!("    ● = exists in local storage");
                println!("    ○ = CID only (not synced)");
                println!();
                println!("  Example:");
                println!("    graph a1b2c3 --depth 2");
                println!();
            }
            "connect" => {
                println!();
                println!("  connect <ip:port>");
                println!();
                println!("  Connect to a peer node by address.");
                println!();
                println!("  Example:");
                println!("    connect 127.0.0.1:4243");
                println!("    connect 192.168.1.5:4242");
                println!();
            }
            "status" => {
                println!();
                println!("  status");
                println!();
                println!("  Show comprehensive node status including:");
                println!("  identity, storage, network, AI, and wallet info.");
                println!();
            }
            "peers" => {
                println!();
                println!("  peers");
                println!();
                println!("  Show currently connected peers and remembered peers");
                println!("  from previous sessions.");
                println!();
            }
            "identity" => {
                println!();
                println!("  identity");
                println!();
                println!("  Show identity info: NodeId, trust tier, device group,");
                println!("  and usage statistics.");
                println!();
            }
            "recover" => {
                println!();
                println!("  recover");
                println!();
                println!("  Recover identity from a 24-word BIP39 recovery phrase.");
                println!("  ⚠ This will REPLACE the current identity on this device.");
                println!();
            }
            "profile" => {
                println!();
                println!("  profile                  View current profile");
                println!("  profile set <field> <val> Update a profile field");
                println!();
                println!("  Fields: name, language, style");
                println!("  Styles: concise, balanced, detailed, academic");
                println!();
                println!("  Examples:");
                println!("    profile");
                println!("    profile set name \"Phúc Nguyễn\"");
                println!("    profile set language en");
                println!("    profile set style detailed");
                println!();
            }
            "model" => {
                println!();
                println!("  model list               List available AI models");
                println!("  model switch <name>       Switch to a different model");
                println!("  model test                Test AI connection health");
                println!();
                println!("  Examples:");
                println!("    model list");
                println!("    model switch qwen2.5:7b");
                println!("    model test");
                println!();
            }
            "wallet" => {
                println!();
                println!("  wallet                   Show OBT balance & earnings");
                println!("  wallet history [--limit N] Show transaction history");
                println!();
                println!("  OBT uses Nano-style block-lattice — each node has its own chain.");
                println!("  Balance is read from local AccountState (instant, no network).");
                println!();
            }
            "export" => {
                println!();
                println!("  export [--format FORMAT] [--output FILE]");
                println!();
                println!("  Export KUs to a file.");
                println!("  Formats: json (default), csv");
                println!();
                println!("  Examples:");
                println!("    export");
                println!("    export --format json --output my_knowledge.json");
                println!();
            }
            "import" => {
                println!();
                println!("  import <file>");
                println!();
                println!("  Import KUs from a JSON file.");
                println!("  Duplicates are automatically skipped.");
                println!();
                println!("  Example:");
                println!("    import knowledge_backup.json");
                println!();
            }
            "backup" => {
                println!();
                println!("  backup");
                println!();
                println!("  Create a full encrypted backup of all node data:");
                println!("  identity, KUs, profile, peers, and retriever index.");
                println!("  You will be prompted for a password.");
                println!();
            }
            "restore" => {
                println!();
                println!("  restore <file>");
                println!();
                println!("  Restore from an encrypted backup (.obk) file.");
                println!("  ⚠ This will REPLACE all local data.");
                println!();
                println!("  Example:");
                println!("    restore onebrain_backup_20260707.obk");
                println!();
            }
            "config" => {
                println!();
                println!("  config                   Show current configuration");
                println!("  config set <key> <val>    Update a config value");
                println!();
                println!("  Keys: name, port, ollama_url, model");
                println!("  Changes take effect on next restart.");
                println!();
                println!("  Examples:");
                println!("    config");
                println!("    config set name \"New Name\"");
                println!("    config set ollama_url http://192.168.1.100:11434");
                println!();
            }
            "blob" => {
                println!();
                println!("  blob list              List stored blobs");
                println!("  blob store <file>      Store a file as blob");
                println!("  blob detail <cid>      View blob metadata");
                println!("  blob export <cid> [out] Export blob to file");
                println!("  blob delete <cid>      Delete a blob");
                println!("  blob pin <cid>         Pin blob (prevent GC)");
                println!("  blob unpin <cid>       Unpin blob (allow GC)");
                println!("  blob stats             Storage statistics");
                println!("  blob gc                Garbage collect orphaned blobs");
                println!();
                println!("  Blobs are stored in .blob.redb, separate from KU storage.");
                println!("  Files are chunked at 256KB, deduplicated via BLAKE3.");
                println!("  Max file size: 100MB. Max blobs per KU: 10.");
                println!();
                println!("  Examples:");
                println!("    blob store photo.jpg");
                println!("    blob list");
                println!("    blob export 0101a3b4c5d6 output.jpg");
                println!("    blob pin 0101a3b4c5d6");
                println!("    blob gc");
                println!();
            }
            "deprecate" => {
                println!();
                println!("  deprecate <cid>");
                println!();
                println!("  Mark a KU as deprecated (obsolete) without deleting it.");
                println!("  Unlike 'delete', the KU remains in storage but is marked");
                println!("  as obsolete. Other nodes may still have copies.");
                println!();
                println!("  Example:");
                println!("    deprecate a1b2c3d4");
                println!();
            }
            "edit" => {
                println!();
                println!("  edit <cid>");
                println!();
                println!("  Create a new version of an existing KU.");
                println!("  Shows the current content and prompts for new content.");
                println!("  The new KU will have a prev_cid bond to the original.");
                println!();
                println!("  Example:");
                println!("    edit a1b2c3d4");
                println!();
            }
            "follow" | "unfollow" | "following" => {
                println!();
                println!("  follow <node_id>     Follow a node");
                println!("  unfollow <node_id>   Unfollow a node");
                println!("  following            List followed nodes");
                println!();
                println!("  Follow a node to receive its new KUs in your feed.");
                println!();
            }
            "peer-info" => {
                println!();
                println!("  peer-info <node_id>");
                println!();
                println!("  View the public profile of another node, including");
                println!("  trust score, tier, KU count, and expertise areas.");
                println!("  The node must be in your connected peers.");
                println!();
            }
            "share" => {
                println!();
                println!("  share <cid>");
                println!();
                println!("  Generate a shareable link for a KU.");
                println!("  Creates an onebrain:// URI that others can use.");
                println!();
                println!("  Example:");
                println!("    share a1b2c3d4");
                println!();
            }
            "devices" => {
                println!();
                println!("  devices");
                println!();
                println!("  List all devices in your identity group.");
                println!("  Shows device name, type, last seen, KU count, sync status.");
                println!();
            }
            "sync" => {
                println!();
                println!("  sync status");
                println!();
                println!("  Show multi-device sync status including:");
                println!("  - Overall sync state");
                println!("  - Pending items count");
                println!("  - Per-device sync status");
                println!();
            }
            "tag" => {
                println!();
                println!("  tag add <cid> <tag>      Add tag to KU");
                println!("  tag remove <cid> <tag>   Remove tag from KU");
                println!("  tag list                 List all tags");
                println!();
                println!("  Examples:");
                println!("    tag add a1b2c3d4 important");
                println!("    tag remove a1b2c3d4 draft");
                println!("    tag list");
                println!();
            }
            "pin" | "unpin" => {
                println!();
                println!("  pin [<cid>]    Pin KU for quick access / Show pinned");
                println!("  unpin <cid>    Unpin KU");
                println!();
                println!("  Pin important KUs for quick access.");
                println!("  Run 'pin' without arguments to see all pinned KUs.");
                println!();
            }
            "watch" => {
                println!();
                println!("  watch create <kql>   Create a standing query");
                println!("  watch list           List active watches");
                println!("  watch delete <id>    Delete a watch");
                println!();
                println!("  Standing queries notify you when new KUs match.");
                println!();
                println!("  Example:");
                println!("    watch create FIND facts WHERE trust > 0.8");
                println!();
            }
            _ => {
                println!();
                println!("  No detailed help for '{}'. Type 'help' for all commands.", args);
                println!();
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// cmd_encode — Encode text into a KU
// ═══════════════════════════════════════════════════════════════════════════

async fn cmd_encode(node: &mut OneBrainNode, args: &str, _reader: &mut BufReader<tokio::io::Stdin>) {
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
            "--draft" => { i += 1; }
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
        node.encode_draft(&text).await
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
            if is_draft {
                println!("  ✓ Draft saved (⚠ STUB: still broadcasts until DraftStore is implemented)");
            } else {
                println!("  ✓ Encoded and stored successfully");
            }
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
            if !is_draft && peer_count > 0 {
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

// ═══════════════════════════════════════════════════════════════════════════
// cmd_search — Search knowledge base
// ═══════════════════════════════════════════════════════════════════════════

async fn cmd_search(node: &mut OneBrainNode, args: &str) {
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

// ═══════════════════════════════════════════════════════════════════════════
// cmd_list — Browse all KUs
// ═══════════════════════════════════════════════════════════════════════════

fn cmd_list(node: &OneBrainNode, args: &str) {
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
            let total_pages = if total == 0 { 1 } else { (total + limit - 1) / limit };
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
                    "  {:>3}  {:<8} {:<5} {:<5} {:<10} {:<10} {}",
                    "#", "Gene", "PoMV", "Trust", "Created", "CID", "Preview"
                );
                for (idx_offset, item) in items.iter().enumerate() {
                    let idx = (page - 1) * limit + idx_offset + 1;
                    let created = format_timestamp(item.created);
                    let cid_short = short_cid(&item.cid_hex);
                    println!(
                        "  {:>3}. [{:<6}] {:.2}  {:.2}  {:<10} {}...  {}",
                        idx, item.gene_type, item.pomv, item.trust, created, cid_short, item.preview
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

// ═══════════════════════════════════════════════════════════════════════════
// cmd_detail — View KU details
// ═══════════════════════════════════════════════════════════════════════════

fn cmd_detail(node: &OneBrainNode, args: &str) {
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
            println!("  ── Trust & PoMV ──");
            println!("  Epistemic:    {}", detail.epistemic);
            println!("  Evidence:     {}", detail.evidence);
            println!("  Trust score:  {:.2}", detail.trust);
            println!("  PoMV rate:    {:.2}", detail.pomv);
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

// ═══════════════════════════════════════════════════════════════════════════
// cmd_delete — Delete KU from local storage (with confirmation)
// ═══════════════════════════════════════════════════════════════════════════

async fn cmd_delete(
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
    let preview = match node.get_ku(cid) {
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
            true
        }
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
            return;
        }
    };

    if !preview {
        return;
    }

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

// ═══════════════════════════════════════════════════════════════════════════
// cmd_kql — Execute KQL query
// ═══════════════════════════════════════════════════════════════════════════

fn cmd_kql(node: &OneBrainNode, args: &str) {
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

// ═══════════════════════════════════════════════════════════════════════════
// cmd_graph — View knowledge graph neighbors
// ═══════════════════════════════════════════════════════════════════════════

fn cmd_graph(node: &OneBrainNode, args: &str) {
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
            println!(
                "  ● [{}] (root)",
                short_cid(cid),
            );

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
            println!(
                "  ● = in local storage  ○ = CID only (not synced)"
            );
            println!(
                "  Nodes: {}  |  Edges: {}  |  Max depth: {}",
                1 + count_nodes(&neighbors),
                count_edges(&neighbors),
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
    let arrow = if neighbor.direction == "OUT" { "→" } else { "←" };
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

/// Count total nodes in a neighbor tree (recursively).
fn count_nodes(neighbors: &[NeighborInfo]) -> usize {
    let mut count = neighbors.len();
    for n in neighbors {
        count += count_nodes(&n.children);
    }
    count
}

/// Count total edges in a neighbor tree (recursively).
fn count_edges(neighbors: &[NeighborInfo]) -> usize {
    let mut count = neighbors.len();
    for n in neighbors {
        count += count_edges(&n.children);
    }
    count
}

// ═══════════════════════════════════════════════════════════════════════════
// cmd_connect — Connect to a peer
// Takes &T (immutable) — &mut T callers coerce automatically
// ═══════════════════════════════════════════════════════════════════════════

async fn cmd_connect(node: &OneBrainNode, args: &str) {
    let addr_str = args.trim();
    if addr_str.is_empty() {
        eprintln!();
        eprintln!("  ✗ Usage: connect <ip:port>");
        eprintln!("    Example: connect 127.0.0.1:4243");
        eprintln!();
        return;
    }

    match addr_str.parse::<std::net::SocketAddr>() {
        Ok(addr) => match node.connect_to_seed(addr).await {
            Ok(()) => println!("  ✓ Connected to {}", addr),
            Err(e) => eprintln!("  ✗ Connection failed: {}", e),
        },
        Err(e) => {
            eprintln!("  ✗ Invalid address '{}': {}", addr_str, e);
            eprintln!("  Usage: connect 127.0.0.1:4243");
        }
    }
    println!();
}

// ═══════════════════════════════════════════════════════════════════════════
// cmd_status — Show comprehensive node status
// ═══════════════════════════════════════════════════════════════════════════

async fn cmd_status(node: &OneBrainNode) {
    let config = node.config();
    let ku_count = node.ku_count().unwrap_or(0);
    let peer_count = node.peer_count();
    let listener = node
        .listener_addr()
        .map(|a| format!("{}", a))
        .unwrap_or_else(|| "not started".to_string());

    println!();
    println!("  ── Node Status ──");
    println!("  Name:       {}", config.name);
    println!("  Port:       {}", config.port);
    println!("  Listen:     {}", listener);
    println!("  Data:       {}", config.data_dir.display());

    // Identity info (if available)
    if let Ok(identity) = node.get_identity_info() {
        let id_short = short_cid(&identity.node_id);
        println!(
            "  NodeId:     {}... ({}, trust: {:.2})",
            id_short, identity.tier, identity.trust_score
        );
    }

    println!();
    println!("  ── Storage ──");
    println!("  KUs:        {} stored", ku_count);

    println!();
    println!("  ── Network ──");
    println!("  Peers:      {} connected", peer_count);
    let memory = onebrain_node::peer_memory::PeerMemory::load(&config.peer_memory_path());
    if memory.peer_count() > 0 {
        println!(
            "  Remembered: {} peer(s) from previous sessions",
            memory.peer_count()
        );
    }

    println!();
    println!("  ── AI ──");
    println!("  Ollama:     {}", config.ollama_url);
    println!("  Model:      {}", config.model);

    // AI health check (if available)
    if let Ok(health) = node.test_ai_connection().await {
        let status_icon = if health.connected { "✓" } else { "✗" };
        println!(
            "  Status:     {} {} ({}ms)",
            status_icon, health.status_message, health.latency_ms
        );
    }

    // Wallet info (if available)
    if let Ok(wallet) = node.get_balance() {
        println!();
        println!("  ── Wallet ──");
        println!("  Balance:    {}", format_obt_short(wallet.balance));
        println!(
            "  Rate:       {}/{} KU used this hour",
            wallet.rate_used, wallet.rate_max
        );
    }

    println!();
}

// ═══════════════════════════════════════════════════════════════════════════
// cmd_peers — Show connected peers
// ═══════════════════════════════════════════════════════════════════════════

fn cmd_peers(node: &OneBrainNode) {
    let peers = node.peer_list_snapshot();
    let config = node.config();
    println!();
    if peers.is_empty() {
        println!("  No peers connected.");
        println!("  Use 'connect <ip:port>' to connect to a peer.");
    } else {
        println!("  ── Connected Peers ({}) ──", peers.len());
        for (i, peer) in peers.iter().enumerate() {
            println!(
                "  {}. {} @ {} ({} KUs)",
                i + 1,
                peer.name,
                peer.addr,
                peer.ku_count
            );
        }
    }

    // Show remembered peers from previous sessions
    let memory = onebrain_node::peer_memory::PeerMemory::load(&config.peer_memory_path());
    if memory.peer_count() > 0 {
        println!();
        println!("  ── Remembered Peers ({}) ──", memory.peer_count());
        for rp in &memory.known_peers {
            let elapsed = rp
                .last_seen
                .elapsed()
                .map(|d| {
                    let secs = d.as_secs();
                    if secs < 60 {
                        format!("{}s ago", secs)
                    } else if secs < 3600 {
                        format!("{}m ago", secs / 60)
                    } else if secs < 86400 {
                        format!("{}h ago", secs / 3600)
                    } else {
                        format!("{}d ago", secs / 86400)
                    }
                })
                .unwrap_or_else(|_| "unknown".to_string());
            let short_id = if rp.peer_id.len() >= 8 {
                &rp.peer_id[..8]
            } else {
                &rp.peer_id
            };
            println!("  - {} ({}) last seen: {}", rp.name, short_id, elapsed);
        }
    }
    println!();
}

// ═══════════════════════════════════════════════════════════════════════════
// cmd_identity — Show identity info
// ═══════════════════════════════════════════════════════════════════════════

fn cmd_identity(node: &OneBrainNode) {
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
            println!("  Progress:     {} {:.0}%", progress_bar, info.trust_score * 100.0);
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

// ═══════════════════════════════════════════════════════════════════════════
// cmd_recover — Recover identity from BIP39 phrase
// ═══════════════════════════════════════════════════════════════════════════

async fn cmd_recover(node: &mut OneBrainNode, reader: &mut BufReader<tokio::io::Stdin>) {
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

// ═══════════════════════════════════════════════════════════════════════════
// cmd_profile — View/edit user profile
// ═══════════════════════════════════════════════════════════════════════════

fn cmd_profile(node: &mut OneBrainNode, args: &str) {
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
                println!("  Member since:  {}", format_timestamp(profile.member_since));
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

// ═══════════════════════════════════════════════════════════════════════════
// cmd_model — Manage AI models
// ═══════════════════════════════════════════════════════════════════════════

async fn cmd_model(node: &mut OneBrainNode, args: &str) {
    let subcmd = args.trim();

    if subcmd.is_empty() {
        eprintln!();
        eprintln!("  ✗ Usage: model <list|switch|test>");
        eprintln!("    Type 'help model' for details.");
        eprintln!();
        return;
    }

    if subcmd == "list" {
        match node.list_ai_models() {
            Ok(models) => {
                println!();
                println!("  ── AI Models ──");

                let installed: Vec<&ModelInfo> =
                    models.iter().filter(|m| m.is_installed).collect();
                let not_installed: Vec<&ModelInfo> =
                    models.iter().filter(|m| !m.is_installed).collect();

                if !installed.is_empty() {
                    println!();
                    println!("  Available (installed in Ollama):");
                    for m in &installed {
                        let current = if m.is_current { "  [current]" } else { "" };
                        let star = if m.is_current { "★" } else { " " };
                        println!(
                            "    {} {:<20} {:<12}{}",
                            star, m.name, m.params, current
                        );
                    }
                }

                if !not_installed.is_empty() {
                    println!();
                    println!("  Recommended (not yet installed):");
                    for m in &not_installed {
                        println!("      {:<20} {}", m.name, m.params);
                    }
                }

                println!();
                println!("  To install: ollama pull <model_name>");
                println!("  To switch:  model switch <model_name>");
                println!();
            }
            Err(e) => {
                eprintln!("  ✗ {}", e);
                println!();
            }
        }
    } else if let Some(model_name) = subcmd.strip_prefix("switch ") {
        let model_name = model_name.trim();
        if model_name.is_empty() {
            eprintln!("  ✗ Usage: model switch <model_name>");
            println!();
            return;
        }

        println!("  Checking model availability...");
        match node.switch_model(model_name) {
            Ok(()) => {
                println!("  ✓ Now using {}", model_name);
                println!();
            }
            Err(e) => {
                eprintln!("  ✗ {}", e);
                println!();
            }
        }
    } else if subcmd == "test" {
        println!();
        println!("  ── AI Health Check ──");
        match node.test_ai_connection().await {
            Ok(health) => {
                let status_icon = if health.connected { "✓" } else { "✗" };
                println!(
                    "  Ollama:    {} Connected ({})",
                    status_icon, health.ollama_url
                );
                println!("  Model:     {}", health.model);
                if health.connected {
                    let latency_quality = if health.latency_ms < 500 {
                        "good"
                    } else if health.latency_ms < 2000 {
                        "moderate"
                    } else {
                        "slow"
                    };
                    println!(
                        "  Latency:   {}ms ({})",
                        health.latency_ms, latency_quality
                    );
                }
                if !health.status_message.is_empty() {
                    println!("  Status:    {}", health.status_message);
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
        eprintln!(
            "  ✗ Unknown model subcommand '{}'. Use: list, switch, test",
            subcmd
        );
        eprintln!();
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// cmd_wallet — Show OBT balance and transaction history
// ═══════════════════════════════════════════════════════════════════════════

fn cmd_wallet(node: &OneBrainNode, args: &str) {
    let subcmd = args.trim();

    if subcmd.is_empty() {
        // Show balance
        match node.get_balance() {
            Ok(wallet) => {
                println!();
                println!("  ── OBT Wallet ──");
                println!("  Balance:     {}", format_obt_short(wallet.balance));
                println!("  Chain:       {} blocks", wallet.chain_length);
                println!();
                println!("  ── Tier ──");
                println!(
                    "  Current:     {} (multiplier: {:.2}x)",
                    wallet.tier, wallet.multiplier
                );
                println!();
                println!("  ── Earnings Summary ──");
                println!("  Total earned: {}", format_obt_short(wallet.total_earned));
                println!("  Total spent:  {}", format_obt_short(wallet.total_spent));
                println!();

                let max_stream = [
                    wallet.streams.owner,
                    wallet.streams.encoder,
                    wallet.streams.verifier,
                    wallet.streams.storage,
                ]
                .into_iter()
                .max()
                .unwrap_or(1);

                println!("  By stream:");
                println!(
                    "    R1 Owner (40%):    {:<16} {}",
                    format_obt_short(wallet.streams.owner),
                    bar_chart(wallet.streams.owner, max_stream, 16)
                );
                println!(
                    "    R2 Encoder (25%):  {:<16} {}",
                    format_obt_short(wallet.streams.encoder),
                    bar_chart(wallet.streams.encoder, max_stream, 16)
                );
                println!(
                    "    R3 Verifier (15%): {:<16} {}",
                    format_obt_short(wallet.streams.verifier),
                    bar_chart(wallet.streams.verifier, max_stream, 16)
                );
                println!(
                    "    R4 Storage (20%):  {:<16} {}",
                    format_obt_short(wallet.streams.storage),
                    bar_chart(wallet.streams.storage, max_stream, 16)
                );
                println!();
                println!("  ── Rate Limits ──");
                println!(
                    "  KU/hour:     {} ({} tier)",
                    wallet.rate_max, wallet.tier
                );
                println!(
                    "  Used:        {}/{} this hour",
                    wallet.rate_used, wallet.rate_max
                );
                println!();
            }
            Err(e) => {
                eprintln!("  ✗ {}", e);
                println!();
            }
        }
    } else if subcmd.starts_with("history") {
        // Parse optional --limit N
        let mut limit: usize = 10;
        let parts: Vec<&str> = subcmd.split_whitespace().collect();
        let mut i = 1;
        while i < parts.len() {
            if parts[i] == "--limit" && i + 1 < parts.len() {
                limit = parts[i + 1].parse().unwrap_or(10);
                i += 2;
            } else {
                i += 1;
            }
        }

        match node.get_wallet_history(limit) {
            Ok(transactions) => {
                println!();
                if transactions.is_empty() {
                    println!("  ── Transaction History ──");
                    println!("  No transactions yet.");
                } else {
                    println!(
                        "  ── Transaction History (latest {}) ──",
                        transactions.len()
                    );
                    println!(
                        "  {:>3}  {:<8} {:>14}  {:<12}  {}",
                        "#", "Type", "Amount", "When", "Detail"
                    );
                    for (i, tx) in transactions.iter().enumerate() {
                        println!(
                            "  {:>3}. {:<8} {:>14}  {:<12}  {}",
                            i + 1,
                            tx.block_type,
                            format_obt_signed(tx.amount),
                            format_timestamp(tx.timestamp),
                            tx.detail
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
    } else {
        eprintln!();
        eprintln!("  ✗ Unknown wallet subcommand '{}'. Use: wallet, wallet history", subcmd);
        eprintln!();
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// cmd_export — Export KUs to file
// ═══════════════════════════════════════════════════════════════════════════

fn cmd_export(node: &OneBrainNode, args: &str) {
    // Parse: --format FORMAT --output FILE
    let mut format = "json".to_string();
    let mut output: Option<String> = None;

    let parts: Vec<&str> = args.split_whitespace().collect();
    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "--format" if i + 1 < parts.len() => {
                format = parts[i + 1].to_string();
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

    println!();
    let ku_count = node.ku_count().unwrap_or(0);
    println!("  Exporting {} KUs...", ku_count);

    let out_path = output.unwrap_or_else(|| format!("onebrain_export.{}", format));
    match node.export_kus(&format, std::path::Path::new(&out_path)) {
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

// ═══════════════════════════════════════════════════════════════════════════
// cmd_import — Import KUs from file
// ═══════════════════════════════════════════════════════════════════════════

async fn cmd_import(node: &mut OneBrainNode, args: &str) {
    let file_path = args.trim();
    if file_path.is_empty() {
        eprintln!();
        eprintln!("  ✗ Usage: import <file>");
        eprintln!("    Example: import knowledge_backup.json");
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
    println!("  Reading file...");

    match node.import_file(std::path::Path::new(file_path)).await {
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

// ═══════════════════════════════════════════════════════════════════════════
// cmd_backup — Full encrypted backup
// ═══════════════════════════════════════════════════════════════════════════

async fn cmd_backup(
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
    match node.create_backup(std::path::Path::new(&backup_path), password) {
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

// ═══════════════════════════════════════════════════════════════════════════
// cmd_restore — Restore from encrypted backup
// ═══════════════════════════════════════════════════════════════════════════

async fn cmd_restore(
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

    match node.restore_backup(std::path::Path::new(file_path), password) {
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

// ═══════════════════════════════════════════════════════════════════════════
// cmd_config — View/set configuration
// ═══════════════════════════════════════════════════════════════════════════

fn cmd_config(node: &mut OneBrainNode, args: &str) {
    let subcmd = args.trim();

    if subcmd.is_empty() {
        // View config
        match Ok::<_, NodeError>(node.get_config_view()) {
            Ok(config_view) => {
                println!();
                println!("  ── Node Configuration ──");
                println!("  name:       {}", config_view.name);
                println!("  port:       {}", config_view.port);
                println!("  data_dir:   {}", config_view.data_dir);
                println!("  ollama_url: {}", config_view.ollama_url);
                println!("  model:      {}", config_view.model);
                if config_view.seeds.is_empty() {
                    println!("  seeds:      []");
                } else {
                    println!("  seeds:      [{}]", config_view.seeds.join(", "));
                }
                println!();
                println!("  ── Derived Paths ──");
                println!("  identity:   {}", config_view.identity_path);
                println!("  storage:    {}", config_view.storage_path);
                println!("  profile:    {}", config_view.profile_path);
                println!("  peers:      {}", config_view.peers_path);
                println!();
            }
            Err(e) => {
                eprintln!("  ✗ {}", e);
                println!();
            }
        }
    } else if let Some(rest) = subcmd.strip_prefix("set ") {
        // config set <key> <value>
        let set_parts: Vec<&str> = rest.splitn(2, ' ').collect();
        if set_parts.len() < 2 {
            eprintln!();
            eprintln!("  ✗ Usage: config set <key> <value>");
            eprintln!("    Keys: name, port, ollama_url, model");
            eprintln!();
            return;
        }
        let key = set_parts[0].trim();
        let value = set_parts[1].trim().trim_matches('"');

        match node.update_config(key, value) {
            Ok(()) => {
                println!("  ✓ {} updated to \"{}\" (takes effect next restart)", key, value);
                println!();
            }
            Err(e) => {
                eprintln!("  ✗ {}", e);
                println!();
            }
        }
    } else {
        eprintln!();
        eprintln!("  ✗ Unknown config subcommand. Usage:");
        eprintln!("    config               View config");
        eprintln!("    config set <k> <v>    Update config");
        eprintln!();
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// cmd_chat — Free-form chat with AI Mediator
// ═══════════════════════════════════════════════════════════════════════════

async fn cmd_chat(node: &mut OneBrainNode, input: &str) {
    match node.process_input(input).await {
        Ok(text) => {
            println!();
            println!("  {}", text.replace('\n', "\n  "));
            println!();
        }
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
        }
    }
}

/// Generate a timestamp string for backup filenames.
fn chrono_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}", now)
}

// ═══════════════════════════════════════════════════════════════════════════
// cmd_blob — Blob/media attachment management
// ═══════════════════════════════════════════════════════════════════════════

async fn cmd_blob(
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

        "list" | "ls" => {
            match node.list_blobs() {
                Ok(blobs) => {
                    println!();
                    if blobs.is_empty() {
                        println!("  No blobs stored.");
                    } else {
                        println!("  ── Stored Blobs ({}) ──", blobs.len());
                        println!();
                        println!("  {:<12} {:<20} {:<10} {:<10} {:<5}",
                            "CID", "Name", "Type", "Size", "Refs");
                        println!("  {}", "─".repeat(60));
                        for blob in &blobs {
                            let type_name = ku_core::blob_store::BlobType::from_u8(blob.blob_type).name();
                            let size_str = format_size(blob.total_size);
                            let pin = if blob.pinned { " \u{1f4cc}" } else { "" };
                            println!("  {:<12} {:<20} {:<10} {:<10} {}{}",
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
            }
        }

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
                    println!("  CID:    {}", &meta.blob_cid_hex[..16.min(meta.blob_cid_hex.len())]);
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
                    println!("  Size:       {} ({} bytes)", format_size(meta.total_size), meta.total_size);
                    println!("  Chunks:     {} \u{00d7} {}KB", meta.chunk_count, meta.chunk_size / 1024);
                    println!("  BLAKE3:     {}", meta.blake3_hex);
                    println!("  Created:    {}", meta.created_at);
                    println!("  Pinned:     {}", if meta.pinned { "Yes \u{1f4cc}" } else { "No" });
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
                    println!("  \u{2713} Exported {} to {}", format_size(size), output.display());
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

        "stats" => {
            match node.blob_stats() {
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
            }
        }

        "gc" => {
            println!("  Scanning for orphaned blobs...");
            match node.blob_gc() {
                Ok((deleted, freed)) => {
                    if deleted == 0 {
                        println!("  \u{2713} No orphaned blobs found.");
                    } else {
                        println!("  \u{2713} Deleted {} orphaned blob(s), freed {}", deleted, format_size(freed));
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

// ═══════════════════════════════════════════════════════════════════════════
// cmd_deprecate — Mark KU as obsolete
// ═══════════════════════════════════════════════════════════════════════════

fn cmd_deprecate(node: &OneBrainNode, args: &str) {
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

// ═══════════════════════════════════════════════════════════════════════════
// cmd_edit — Create new version of existing KU
// ═══════════════════════════════════════════════════════════════════════════

async fn cmd_edit(node: &mut OneBrainNode, args: &str, reader: &mut BufReader<tokio::io::Stdin>) {
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

// ═══════════════════════════════════════════════════════════════════════════
// cmd_follow / cmd_unfollow / cmd_following — Social features
// ═══════════════════════════════════════════════════════════════════════════

fn cmd_follow(node: &OneBrainNode, args: &str) {
    if args.is_empty() {
        eprintln!();
        eprintln!("  ✗ Usage: follow <node_id>");
        eprintln!("    Follow a node to receive its new KUs in your feed.");
        eprintln!();
        return;
    }

    match node.follow_node(args.trim()) {
        Ok(()) => {
            println!();
            println!("  ✓ Now following node: {}", args.trim());
            println!();
        }
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
        }
    }
}

fn cmd_unfollow(node: &OneBrainNode, args: &str) {
    if args.is_empty() {
        eprintln!();
        eprintln!("  ✗ Usage: unfollow <node_id>");
        eprintln!();
        return;
    }

    match node.unfollow_node(args.trim()) {
        Ok(()) => {
            println!();
            println!("  ✓ Unfollowed node: {}", args.trim());
            println!();
        }
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
        }
    }
}

fn cmd_following(node: &OneBrainNode) {
    let list = node.following_list();

    println!();
    if list.is_empty() {
        println!("  No nodes followed yet.");
        println!("  Use 'follow <node_id>' to follow a node.");
    } else {
        println!("  ── Following ({} nodes) ──", list.len());
        println!("  {:<32}  {:<20}  {}", "Node ID", "Name", "Since");
        println!("  {}", "─".repeat(70));
        for f in &list {
            let since = format_timestamp(f.followed_since);
            let short_id = if f.node_id.len() >= 16 { &f.node_id[..16] } else { &f.node_id };
            println!("  {:<32}  {:<20}  {}", short_id, f.name, since);
        }
    }
    println!();
}

// ═══════════════════════════════════════════════════════════════════════════
// cmd_peer_info — View another node's profile
// ═══════════════════════════════════════════════════════════════════════════

fn cmd_peer_info(node: &OneBrainNode, args: &str) {
    if args.is_empty() {
        eprintln!();
        eprintln!("  ✗ Usage: peer-info <node_id>");
        eprintln!("    View public profile of another node.");
        eprintln!();
        return;
    }

    match node.get_peer_profile(args.trim()) {
        Some(profile) => {
            println!();
            println!("  ╔═══════════════════════════════════════╗");
            println!("  ║         Node Profile                  ║");
            println!("  ╚═══════════════════════════════════════╝");
            println!("  Node ID:    {}", profile.node_id);
            println!("  Name:       {}", profile.name);
            println!("  Trust:      {:.2}", profile.trust_score);
            println!("  Tier:       {}", profile.tier);
            println!("  KUs:        {}", profile.ku_count);
            if !profile.expertise.is_empty() {
                println!("  Expertise:  {}", profile.expertise.join(", "));
            }
            if profile.member_since > 0 {
                println!("  Member:     {}", format_timestamp(profile.member_since));
            }
            println!();
        }
        None => {
            eprintln!();
            eprintln!("  ✗ Node not found: {}", args.trim());
            eprintln!("    The node must be in your connected peers.");
            eprintln!();
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// cmd_share — Share KU via link
// ═══════════════════════════════════════════════════════════════════════════

fn cmd_share(node: &OneBrainNode, args: &str) {
    if args.is_empty() {
        eprintln!();
        eprintln!("  ✗ Usage: share <cid>");
        eprintln!("    Generate a shareable link for a KU.");
        eprintln!();
        return;
    }

    let cid_hex = args.trim();

    // Verify KU exists
    match node.get_ku(cid_hex) {
        Ok(detail) => {
            println!();
            println!("  ── Share KU ──");
            println!("  CID:       {}", detail.cid_hex);
            println!("  Gene type: {}", detail.gene_type);
            println!("  Preview:   {}", truncate_str(&detail.content, 60));
            println!();
            println!("  📋 Shareable link:");
            println!("  onebrain://ku/{}", detail.cid_hex);
            println!();
            println!("  Recipients can use: detail {}", &detail.cid_hex[..std::cmp::min(16, detail.cid_hex.len())]);
            println!();
        }
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// cmd_devices — List devices in identity group
// ═══════════════════════════════════════════════════════════════════════════

fn cmd_devices(node: &OneBrainNode) {
    let devices = node.list_devices();

    println!();
    println!("  ── Devices ({}) ──", devices.len());
    println!("  {:<16}  {:<20}  {:<8}  {:<12}  {:<6}  {}",
        "Device ID", "Name", "Type", "Last Seen", "KUs", "Status");
    println!("  {}", "─".repeat(80));

    for dev in &devices {
        let short_id = if dev.device_id.len() >= 12 { &dev.device_id[..12] } else { &dev.device_id };
        let last_seen = format_timestamp(dev.last_seen);
        let status_icon = match dev.sync_status.as_str() {
            "up-to-date" => "🟢",
            "syncing" | "behind" => "🟡",
            _ => "🔴",
        };
        println!("  {:<16}  {:<20}  {:<8}  {:<12}  {:<6}  {} {}",
            short_id, dev.name, dev.device_type, last_seen,
            dev.ku_count, status_icon, dev.sync_status);
    }
    println!();
}

// ═══════════════════════════════════════════════════════════════════════════
// cmd_sync — Multi-device sync status
// ═══════════════════════════════════════════════════════════════════════════

fn cmd_sync(node: &OneBrainNode, args: &str) {
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

// ═══════════════════════════════════════════════════════════════════════════
// cmd_tag — Tag management
// ═══════════════════════════════════════════════════════════════════════════

fn cmd_tag(node: &OneBrainNode, args: &str) {
    let parts: Vec<&str> = args.splitn(3, ' ').collect();

    match parts.first().map(|s| *s) {
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
                    println!("  ✓ Tag '{}' added to KU {}", tag, &cid[..std::cmp::min(16, cid.len())]);
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
                    println!("  ✓ Tag '{}' removed from KU {}", tag, &cid[..std::cmp::min(16, cid.len())]);
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

// ═══════════════════════════════════════════════════════════════════════════
// cmd_pin / cmd_unpin — Pin/unpin KU for quick access
// ═══════════════════════════════════════════════════════════════════════════

fn cmd_pin(node: &OneBrainNode, args: &str) {
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
                println!("  📌 [{}] ({}) {}", short_cid, ku.gene_type, truncate_str(&ku.preview, 50));
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

fn cmd_unpin(node: &OneBrainNode, args: &str) {
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

// ═══════════════════════════════════════════════════════════════════════════
// cmd_watch — Standing queries (WATCH)
// ═══════════════════════════════════════════════════════════════════════════

fn cmd_watch(node: &OneBrainNode, args: &str) {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();

    match parts.first().map(|s| *s) {
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

// ═══════════════════════════════════════════════════════════════════════════
// cmd_bulk_delete — Bulk delete KUs by filter
// ═══════════════════════════════════════════════════════════════════════════

async fn cmd_bulk_delete(
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
                } else { i += 1; }
            }
            "--before" => {
                if i + 1 < parts.len() {
                    before = parts[i + 1].parse().ok();
                    i += 2;
                } else { i += 1; }
            }
            _ => { i += 1; }
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
    if let Some(ref g) = gene_filter { println!("    Gene type: {}", g); }
    if let Some(b) = before { println!("    Before:    {}", format_timestamp(b)); }
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


/// Format bytes as human-readable size.
fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Truncate a string for display (UTF-8 safe).
fn truncate_str(s: &str, max: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}
