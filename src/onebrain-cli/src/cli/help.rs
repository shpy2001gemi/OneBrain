//! Help text for all CLI commands.

/// Show general help or per-command help.
pub(crate) fn cmd_help(args: &str) {
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
        println!("  ║  workflow [stage]        Inspect vNext KU workflow            ║");
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
        println!("  ║  recover                 Show secure recovery guidance        ║");
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
        println!("  ║  wallet                  Show non-economic OBT simulation     ║");
        println!("  ║  wallet history          Show simulated activity history      ║");
        println!("  ║                                                               ║");
        println!("  ║  ── Data ──                                                   ║");
        println!("  ║  export --mode <mode>    Export canonical data or a view      ║");
        println!("  ║  import --mode <mode>    Import canonical data or text drafts ║");
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
                println!(
                    "    --type T    Filter: fact, procedure, experience, creative, hypothesis"
                );
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
                println!("  Patterns (13 gene types):");
                println!("    facts, procedures, experiences, creatives, hypotheses,");
                println!("    mediaexperiences, testimonies, formals, narratives,");
                println!("    sensories, composites, normatives, definitions");
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
            "workflow" | "vnext" => {
                println!();
                println!("  workflow [stage]");
                println!("  vnext [stage]  (alias)");
                println!();
                println!("  Inspect the additive vNext KU workflow contract.");
                println!("  Stages: assembly, receptor, discover, proposal, mapping, resolution");
                println!("  This view is read-only and claims no network-wide closure.");
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
                println!("  Legacy phrase recovery is disabled.");
                println!("  Import a verified encrypted Base recovery package instead.");
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
                println!("  wallet                   Show non-economic OBT simulation");
                println!("  wallet history [--limit N] Show simulated activity history");
                println!();
                println!("  OBT uses Nano-style block-lattice — each node has its own chain.");
                println!("  No AccountChain is connected; values are non-economic placeholders.");
                println!();
            }
            "export" => {
                println!();
                println!("  export --mode MODE [--output FILE]");
                println!();
                println!("  Modes: canonical-v1, json-view-v1, csv-view-v1.");
                println!("  JSON/CSV are non-restorable views.");
                println!();
                println!("  Examples:");
                println!("    export --mode canonical-v1 --output public.obx");
                println!("    export --mode json-view-v1 --output knowledge.json");
                println!();
            }
            "import" => {
                println!();
                println!("  import --mode MODE <file>");
                println!();
                println!("  Modes: canonical-v1 or text-drafts-v1.");
                println!("  JSON/CSV views are never importable.");
                println!();
                println!("  Example:");
                println!("    import --mode canonical-v1 public.obx");
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
                println!(
                    "  No detailed help for '{}'. Type 'help' for all commands.",
                    args
                );
                println!();
            }
        }
    }
}
