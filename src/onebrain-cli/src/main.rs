//! OneBrain CLI — Interactive terminal interface.
//!
//! Uses onebrain-node for core runtime, provides REPL.
//! Optionally spawns the REST/WebSocket API server on the same node
//! so that the Web Dashboard can connect.

use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

mod cli;

use cli::vnext::{NeedArgs, PomvArgs, VNextArgs, VNextStartArgs};
use onebrain_api::ApiServer;
use onebrain_node::{
    mdns_discovery, peer_memory::PeerMemory, seed_client::SeedClient, upnp, ConceptRegistryMode,
    NodeConfig, OneBrainNode,
};

#[cfg(all(feature = "legacy-read-compat", not(feature = "base-v1")))]
compile_error!("legacy-read-compat requires base-v1");

#[derive(Parser)]
#[command(name = "onebrain")]
#[command(about = "OneBrain — Decentralized Knowledge Network")]
#[command(disable_version_flag = true)]
struct CliArgs {
    /// Print the OneBrain version without opening a node.
    #[arg(long)]
    version: bool,
    /// Include the complete Base tuple and digests with --version.
    #[arg(long, requires = "version")]
    verbose: bool,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Inspect the offline-first Base runtime contract.
    Base(BaseArgs),
    /// Start an OneBrain node
    Start {
        #[arg(long, default_value = "OneBrain")]
        name: String,
        #[arg(long, default_value_t = 4242)]
        port: u16,
        #[arg(long, default_value = "./onebrain_data")]
        data_dir: PathBuf,
        #[arg(long, default_value = "http://localhost:11434")]
        ollama_url: String,
        #[arg(long, default_value = "qwen3:8b")]
        model: String,
        /// Path to a compiled Concept Registry (.obr).
        #[arg(long)]
        concept_registry: Option<PathBuf>,
        /// Registry policy: required, optional, or disabled.
        #[arg(long, default_value = "optional")]
        concept_registry_mode: ConceptRegistryMode,
        /// Maximum resolved labels retained by the bounded registry cache.
        #[arg(long, default_value_t = 4096)]
        concept_registry_cache_capacity: usize,
        /// Root containing signed immutable registry releases and activation state.
        #[arg(
            long,
            requires = "concept_registry_release_public_key",
            conflicts_with = "concept_registry"
        )]
        concept_registry_release_root: Option<PathBuf>,
        /// Pinned Ed25519 release signer public key (64 lowercase hex digits).
        #[arg(long, requires = "concept_registry_release_root")]
        concept_registry_release_public_key: Option<String>,
        #[arg(long, value_delimiter = ',')]
        seeds: Vec<SocketAddr>,

        /// Enable the REST/WebSocket API server for Web Dashboard
        #[arg(long, default_value_t = false)]
        api: bool,
        /// API server port (default: 4280)
        #[arg(long, default_value_t = 4280)]
        api_port: u16,
        /// API Bearer token for authentication
        #[arg(long, default_value = "onebrain-dev-token")]
        api_token: String,
        /// Path to built web dashboard (default: auto-detect)
        #[arg(long)]
        web_dir: Option<PathBuf>,
        #[command(flatten)]
        vnext: VNextStartArgs,
    },
    /// Manage private one-hop standing Needs through the authenticated local API.
    Need(NeedArgs),
    /// Prepare, explicitly confirm, and inspect vNext PoMV evidence.
    Pomv(PomvArgs),
    /// Inspect the vNext product runtime.
    Vnext(VNextArgs),
}

#[derive(clap::Args)]
struct BaseArgs {
    #[command(subcommand)]
    command: BaseCommand,
}

#[derive(Subcommand)]
enum BaseCommand {
    /// Print the compiled compatibility tuple without starting a node.
    Status,
}

fn generate_peer_id() -> String {
    use std::time::SystemTime;
    let t = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let hash = blake3::hash(format!("{}_{}", t.as_nanos(), std::process::id()).as_bytes());
    hash.as_bytes()[..16]
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

#[tokio::main]
async fn main() {
    let args = CliArgs::parse();

    if args.version {
        if args.verbose {
            println!(
                "{}",
                serde_json::to_string_pretty(&cli::data::compiled_base_status_json()).unwrap()
            );
        } else {
            println!("onebrain {}", env!("CARGO_PKG_VERSION"));
        }
        return;
    }

    let Some(command) = args.command else {
        eprintln!("A command is required. Use --help for available commands.");
        std::process::exit(2);
    };

    match command {
        Commands::Base(BaseArgs {
            command: BaseCommand::Status,
        }) => println!(
            "{}",
            serde_json::to_string_pretty(&cli::data::compiled_base_status_json()).unwrap()
        ),
        Commands::Need(args) => exit_on_client_error(cli::vnext::execute_need(args).await),
        Commands::Pomv(args) => exit_on_client_error(cli::vnext::execute_pomv(args).await),
        Commands::Vnext(args) => exit_on_client_error(cli::vnext::execute_vnext(args).await),
        Commands::Start {
            name,
            port,
            data_dir,
            ollama_url,
            model,
            concept_registry,
            concept_registry_mode,
            concept_registry_cache_capacity,
            concept_registry_release_root,
            concept_registry_release_public_key,
            seeds,
            api,
            api_port,
            api_token,
            web_dir,
            vnext,
        } => {
            let vnext_config = match vnext.feature_config() {
                Ok(config) => config,
                Err(error) => {
                    eprintln!("Invalid vNext configuration: {error}");
                    std::process::exit(2);
                }
            };
            #[cfg(not(feature = "vnext-network-runtime"))]
            if vnext.requested() {
                eprintln!(
                    "vNext product lanes require a binary built with --features vnext-network-runtime"
                );
                std::process::exit(2);
            }
            #[cfg(feature = "vnext-network-runtime")]
            let vnext_dependencies = if vnext.requested() {
                match cli::vnext::prepare_runtime_dependencies(&data_dir) {
                    Ok(dependencies) => Some(dependencies),
                    Err(error) => {
                        eprintln!("Failed to prepare vNext runtime dependencies: {error}");
                        std::process::exit(1);
                    }
                }
            } else {
                None
            };
            #[cfg(feature = "vnext-network-runtime")]
            let vnext_feed_publisher = match cli::vnext::prepare_feed_publisher(&vnext, &data_dir) {
                Ok(publisher) => publisher,
                Err(error) => {
                    eprintln!("Failed to prepare selected Feed signer: {error}");
                    std::process::exit(1);
                }
            };
            let config = NodeConfig {
                name,
                port,
                data_dir,
                ollama_url,
                model,
                seeds,
                concept_registry_path: concept_registry,
                concept_registry_mode,
                concept_registry_cache_capacity,
                concept_registry_release_root,
                concept_registry_release_public_key,
                vnext: vnext_config,
            };
            std::fs::create_dir_all(&config.data_dir).expect("Failed to create data directory");

            println!("\u{2554}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2557}");
            println!("\u{2551}       OneBrain Node Starting...      \u{2551}");
            println!("\u{255a}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d}");
            println!();
            println!("  Name:     {}", config.name);
            println!("  Port:     {}", config.port);
            println!("  Data:     {}", config.data_dir.display());
            println!("  Ollama:   {}", config.ollama_url);
            println!("  Model:    {}", config.model);
            if let Some(root) = &config.concept_registry_release_root {
                println!("  Registry releases: {}", root.display());
            } else {
                println!("  Registry: {}", config.obr_path().display());
            }
            println!("  Policy:   {}", config.concept_registry_mode);
            println!("  Seeds:    {:?}", config.seeds);
            if api {
                println!("  API:      http://127.0.0.1:{}", api_port);
            }
            println!(
                "  vNext:    {}",
                if vnext.requested() {
                    "requested"
                } else {
                    "disabled"
                }
            );
            println!("  Feed signer provider: {}", vnext.describe_signer());
            if vnext.vnext_feed_signer_provider == cli::vnext::FeedSignerProvider::DevelopmentFile {
                eprintln!(
                    "  ⚠ DEVELOPMENT ONLY: Feed events use an exportable local file key; this is not production custody."
                );
            }
            println!();

            let seed_addrs = config.seeds.clone();

            let mut node = match OneBrainNode::new(config.clone()).await {
                Ok(n) => n,
                Err(e) => {
                    eprintln!("Failed to start node: {}", e);
                    std::process::exit(1);
                }
            };
            #[cfg(feature = "base-v1")]
            if let Err(error) = node
                .install_base_runtime(onebrain_api::base_runtime_config_for_api_token(&api_token))
            {
                eprintln!("Failed to install the Base v1 runtime: {error}");
                std::process::exit(1);
            }
            #[cfg(feature = "vnext-network-runtime")]
            if let Some(dependencies) = vnext_dependencies {
                if let Err(error) = node.set_vnext_product_dependencies(dependencies) {
                    eprintln!("Failed to configure vNext product runtime: {error}");
                    std::process::exit(1);
                }
            }
            println!("  \u{2713} Node initialized successfully");

            match node.start_network().await {
                Ok(addr) => println!("  \u{2713} TCP listener started on {}", addr),
                Err(e) => eprintln!(
                    "  \u{26a0} Network start failed: {} (continuing without networking)",
                    e
                ),
            }

            for seed_addr in &seed_addrs {
                if let Err(e) = node.connect_to_seed(*seed_addr).await {
                    eprintln!("  \u{26a0} Failed to connect to seed {}: {}", seed_addr, e);
                }
            }

            let mdns = mdns_discovery::try_mdns_discovery(&config.name, config.port).await;
            println!("  {}", mdns.message);
            for peer_addr in &mdns.discovered_peers {
                println!("    LAN peer: {}", peer_addr);
            }

            let upnp_result = upnp::try_upnp_map(config.port).await;
            println!("  {}", upnp_result.message);

            let peer_id = generate_peer_id();
            let mut seed = SeedClient::new(peer_id.clone(), config.name.clone(), config.port);
            match seed.connect().await {
                Ok(_) => {
                    if let Some(stream) = seed.stream() {
                        SeedClient::run_background(stream, peer_id.clone(), node.event_tx.clone())
                            .await;
                    }
                    match seed.get_peers().await {
                        Ok(peers) => {
                            println!("  \u{2713} Found {} peer(s) online", peers.len());
                            for p in &peers {
                                let short_id = if p.peer_id.len() >= 8 {
                                    &p.peer_id[..8]
                                } else {
                                    &p.peer_id
                                };
                                println!("    - {} ({})", p.name, short_id);
                            }
                        }
                        Err(e) => println!("  \u{26a0} Could not get peer list: {}", e),
                    }
                }
                Err(e) => {
                    println!("  \u{26a0} Seed connection failed: {}", e);
                    println!("  \u{26a0} Running in offline/LAN-only mode");
                }
            }

            let memory = PeerMemory::load(&config.peer_memory_path());
            if memory.peer_count() > 0 {
                println!(
                    "  \u{2713} Remembered {} peer(s) from last session",
                    memory.peer_count()
                );
            }

            // ── API Server (optional) ──────────────────────────────────
            if api {
                let shared_node = Arc::new(tokio::sync::Mutex::new(node));

                // Auto-detect web dashboard directory
                let resolved_web_dir = web_dir.or_else(|| {
                    // Try relative paths from executable location
                    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
                    let candidates = [
                        exe_dir.join("web"),                     // installed: bin/../web
                        exe_dir.join("../web"),                  // installed: prefix/bin/../web
                        exe_dir.join("../../onebrain-web/dist"), // dev: target/debug/../../onebrain-web/dist
                        PathBuf::from("./onebrain-web/dist"),    // dev: cwd
                        PathBuf::from("../onebrain-web/dist"),   // dev: from src/
                    ];
                    candidates
                        .into_iter()
                        .find(|p| p.join("index.html").exists())
                });

                let mut api_server =
                    ApiServer::with_shared_node(shared_node.clone(), api_token.clone(), api_port);
                #[cfg(feature = "vnext-network-runtime")]
                if let Some(publisher) = vnext_feed_publisher {
                    api_server = api_server.with_vnext_feed_publisher(publisher);
                }
                if let Some(ref dir) = resolved_web_dir {
                    api_server = api_server.with_web_dir(dir.clone());
                    println!("  ✓ Web Dashboard: http://127.0.0.1:{}", api_port);
                    println!("    Serving from: {}", dir.display());
                } else {
                    println!("  ⚠ Web Dashboard not found (use --web-dir or run `npm run build` in onebrain-web)");
                    println!("    API-only mode: http://127.0.0.1:{}", api_port);
                }
                tokio::spawn(async move {
                    if let Err(e) = api_server.start().await {
                        eprintln!("  ✗ API server error: {}", e);
                    }
                });
                println!("  ✓ API server started on http://127.0.0.1:{}", api_port);
                println!("  ✓ Token: {}", api_token);
                println!();
                println!("Type 'help' for available commands.");
                println!();

                // REPL uses shared node — lock per-command, not permanently
                if let Err(e) = cli::run_repl_shared(shared_node.clone(), &api_token).await {
                    eprintln!("REPL error: {}", e);
                }
            } else {
                println!();
                println!("Type 'help' for available commands.");
                println!("Tip: Use --api to enable Web Dashboard");
                println!();

                if let Err(e) = cli::run_repl(&mut node, &api_token).await {
                    eprintln!("REPL error: {}", e);
                }
            }
        }
    }
}

fn exit_on_client_error(result: Result<(), String>) {
    if let Err(error) = result {
        eprintln!("vNext CLI error: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod vnext_command_tests {
    use super::*;

    #[test]
    fn p3_cli_command_inventory_is_parseable() {
        const ID: &str = "1111111111111111111111111111111111111111111111111111111111111111";
        for command in [
            vec![
                "onebrain",
                "need",
                "prepare",
                "--query",
                "FIND capability",
                "--idempotency-key",
                "need-1",
                "--api-token",
                "test",
            ],
            vec![
                "onebrain",
                "need",
                "activate",
                "--intent",
                ID,
                "--idempotency-key",
                "need-1",
                "--api-token",
                "test",
            ],
            vec!["onebrain", "need", "list", "--api-token", "test"],
            vec![
                "onebrain",
                "need",
                "scan",
                "--need",
                ID,
                "--idempotency-key",
                "scan-1",
                "--api-token",
                "test",
            ],
            vec![
                "onebrain",
                "need",
                "matches",
                "--need",
                ID,
                "--api-token",
                "test",
            ],
            vec![
                "onebrain",
                "need",
                "retire",
                "--need",
                ID,
                "--api-token",
                "test",
            ],
            vec![
                "onebrain",
                "pomv",
                "use",
                "prepare",
                "--target",
                ID,
                "--recipient",
                ID,
                "--selector",
                ID,
                "--namespace",
                "public-use",
                "--idempotency-key",
                "public-use-1",
                "--expires-at",
                "4102444800",
                "--public-permanent",
                "--api-token",
                "test",
            ],
            vec![
                "onebrain",
                "pomv",
                "use",
                "confirm",
                "--intent",
                ID,
                "--api-token",
                "test",
            ],
            vec![
                "onebrain",
                "pomv",
                "use",
                "status",
                "--publication",
                ID,
                "--api-token",
                "test",
            ],
            vec![
                "onebrain",
                "pomv",
                "view",
                "--target",
                ID,
                "--api-token",
                "test",
            ],
            vec!["onebrain", "vnext", "status", "--api-token", "test"],
        ] {
            assert!(
                CliArgs::try_parse_from(command).is_ok(),
                "command should parse"
            );
        }
    }

    #[test]
    fn public_use_has_no_yes_bypass_and_prepare_requires_acknowledgement() {
        const ID: &str = "2121212121212121212121212121212121212121212121212121212121212121";
        let confirm_with_yes = CliArgs::try_parse_from([
            "onebrain",
            "pomv",
            "use",
            "confirm",
            "--intent",
            ID,
            "--yes",
            "--api-token",
            "test",
        ]);
        assert!(confirm_with_yes.is_err());

        let prepare_without_ack = CliArgs::try_parse_from([
            "onebrain",
            "pomv",
            "use",
            "prepare",
            "--target",
            ID,
            "--recipient",
            ID,
            "--selector",
            ID,
            "--namespace",
            "public-use",
            "--idempotency-key",
            "public-use-1",
            "--expires-at",
            "4102444800",
            "--api-token",
            "test",
        ]);
        assert!(prepare_without_ack.is_err());
    }
}
