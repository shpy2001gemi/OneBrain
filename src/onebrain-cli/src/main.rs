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

use onebrain_api::ApiServer;
use onebrain_node::{
    mdns_discovery, peer_memory::PeerMemory, seed_client::SeedClient, upnp, ConceptRegistryMode,
    NodeConfig, OneBrainNode,
};

#[derive(Parser)]
#[command(name = "onebrain")]
#[command(about = "OneBrain — Decentralized Knowledge Network")]
#[command(version)]
struct CliArgs {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
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
    },
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

    match args.command {
        Commands::Start {
            name,
            port,
            data_dir,
            ollama_url,
            model,
            concept_registry,
            concept_registry_mode,
            concept_registry_cache_capacity,
            seeds,
            api,
            api_port,
            api_token,
            web_dir,
        } => {
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
                vnext: Default::default(),
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
            println!("  Registry: {}", config.obr_path().display());
            println!("  Policy:   {}", config.concept_registry_mode);
            println!("  Seeds:    {:?}", config.seeds);
            if api {
                println!("  API:      http://127.0.0.1:{}", api_port);
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
                if let Err(e) = cli::run_repl_shared(shared_node.clone()).await {
                    eprintln!("REPL error: {}", e);
                }
            } else {
                println!();
                println!("Type 'help' for available commands.");
                println!("Tip: Use --api to enable Web Dashboard");
                println!();

                if let Err(e) = cli::run_repl(&mut node).await {
                    eprintln!("REPL error: {}", e);
                }
            }
        }
    }
}
