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
#[cfg(feature = "legacy")]
use onebrain_node::seed_client::SeedClient;
use onebrain_node::{
    mdns_discovery, peer_memory::PeerMemory, upnp, ConceptRegistryMode, LegacyTcpListener,
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
    /// Bounded OBP operations through the authenticated local node-owned API.
    #[cfg(feature = "vnext-outbound-first")]
    Obp(cli::obp::ObpArgs),
    /// Private KU workflow through the authenticated local shared service.
    Ku(cli::ku::KuArgs),
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
        /// Explicitly enable the legacy TCP/JSON seed discovery path.
        #[arg(long, default_value_t = false)]
        legacy_seed_compat: bool,
        /// Legacy peer addresses; only valid in legacy seed compatibility mode.
        #[arg(long, value_delimiter = ',', requires = "legacy_seed_compat")]
        seeds: Vec<SocketAddr>,
        /// Unauthenticated legacy TCP/JSON listener in legacy seed compatibility
        /// mode: loopback (default), all-interfaces, or disabled. Without
        /// --legacy-seed-compat the listener is never bound.
        #[arg(long, requires = "legacy_seed_compat")]
        legacy_tcp_listener: Option<LegacyTcpListener>,

        /// Enable the REST/WebSocket API server for Web Dashboard
        #[arg(long, default_value_t = false)]
        api: bool,
        /// API server port (default: 4280)
        #[arg(long, default_value_t = 4280)]
        api_port: u16,
        /// API Bearer token for authentication (prefer --api-token-file to avoid shell/argv exposure)
        #[arg(
            long,
            default_value = DEFAULT_API_TOKEN,
            conflicts_with = "api_token_file"
        )]
        api_token: String,
        /// Private UTF-8 file containing the API Bearer token (32..1024 ASCII letters,
        /// digits, '-' or '_'). Conflicts with --api-token; never printed on stdout.
        #[arg(long, value_name = "PATH", conflicts_with = "api_token")]
        api_token_file: Option<PathBuf>,
        /// Path to built web dashboard (default: auto-detect)
        #[arg(long)]
        web_dir: Option<PathBuf>,
        /// Enable the local `/ku` workspace on the API host: the dataset's
        /// private 32-byte binary Vault key file. Requires --api and a signed
        /// Registry (--concept-registry-release-root and its public key).
        #[arg(long, requires = "api")]
        ku_vault_key_file: Option<PathBuf>,
        /// Operator-admitted canonical LOCAL_ONLY Text SourceArtifact for `/ku`
        /// manual drafts, as LABEL=PATH (repeatable, at most 64).
        #[arg(
            long = "ku-source",
            value_name = "LABEL=PATH",
            requires = "ku_vault_key_file",
            value_parser = parse_ku_source
        )]
        ku_sources: Vec<(String, PathBuf)>,
        /// Opt-in loopback OBP-RP listener for `/ku` D-045 Public KU share,
        /// browse and inspect, e.g. 127.0.0.1:0. Off by default (local/private
        /// only). Requires --ku-vault-key-file and a binary built with
        /// --features vnext-network-runtime; not combinable with vNext lanes.
        #[arg(long, value_name = "LOOPBACK_ADDR", requires = "ku_vault_key_file")]
        ku_network_bind: Option<SocketAddr>,
        /// Experimental local Ollama for `/ku` AI text intake and background
        /// drafts: path to the installed `ollama` executable. Off by default.
        /// Requires --ku-vault-key-file, --ku-ollama-models-dir and at least one
        /// --ku-ollama-model; models are admitted at startup, never downloaded.
        #[arg(
            long,
            value_name = "PATH",
            requires_all = ["ku_vault_key_file", "ku_ollama_models_dir", "ku_ollama_models"]
        )]
        ku_ollama_executable: Option<PathBuf>,
        /// Ollama models directory for --ku-ollama-executable.
        #[arg(long, value_name = "DIR", requires = "ku_ollama_executable")]
        ku_ollama_models_dir: Option<PathBuf>,
        /// Installed Ollama model tag offered on `/ku` (repeatable, 1..8), e.g. qwen3:8b.
        #[arg(
            long = "ku-ollama-model",
            value_name = "MODEL",
            requires = "ku_ollama_executable"
        )]
        ku_ollama_models: Vec<String>,
        /// Memory limit for the managed Ollama worker (default 12 GiB).
        #[arg(long, value_name = "BYTES", requires = "ku_ollama_executable")]
        ku_ollama_memory_limit_bytes: Option<u64>,
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

#[cfg(feature = "legacy")]
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
        #[cfg(feature = "vnext-outbound-first")]
        Commands::Obp(args) => exit_on_client_error(cli::obp::execute(args).await),
        Commands::Ku(args) => exit_on_client_error(cli::ku::execute(args).await),
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
            legacy_seed_compat,
            seeds,
            legacy_tcp_listener,
            api,
            api_port,
            api_token,
            api_token_file,
            web_dir,
            ku_vault_key_file,
            ku_sources,
            ku_network_bind,
            ku_ollama_executable,
            ku_ollama_models_dir,
            ku_ollama_models,
            ku_ollama_memory_limit_bytes,
            vnext,
        } => {
            let vnext_config = match vnext.feature_config() {
                Ok(config) => config,
                Err(error) => {
                    eprintln!("Invalid vNext configuration: {error}");
                    std::process::exit(2);
                }
            };
            // Validate the opt-in `/ku` network before anything (including vNext
            // dependency preparation) touches data_dir, as `ku_local_web` does.
            #[cfg(feature = "base-v1")]
            let ku_network = match ku_network_bind {
                Some(bind_addr) => match validate_start_ku_network(bind_addr, vnext.requested()) {
                    Ok(network) => Some(network),
                    Err(error) => {
                        eprintln!("{error}");
                        std::process::exit(2);
                    }
                },
                None => None,
            };
            #[cfg(feature = "base-v1")]
            let ku_ollama = match start_ku_ollama(
                ku_ollama_executable,
                ku_ollama_models_dir,
                ku_ollama_models,
                ku_ollama_memory_limit_bytes,
            ) {
                Ok(ollama) => ollama,
                Err(error) => {
                    eprintln!("{error}");
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
            // Read and validate --api-token-file before anything touches data_dir,
            // Registry or Vault key; never print token bytes or private paths.
            let (api_token, api_token_from_file) =
                match resolve_start_api_token(api_token, api_token_file.as_deref()) {
                    Ok(resolved) => resolved,
                    Err(error) => {
                        eprintln!("{error}");
                        std::process::exit(1);
                    }
                };
            let api_token_is_default = !api_token_from_file && api_token == DEFAULT_API_TOKEN;
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
                legacy_tcp_listener: if legacy_seed_compat {
                    legacy_tcp_listener.unwrap_or(LegacyTcpListener::Loopback)
                } else {
                    LegacyTcpListener::Disabled
                },
            };
            // Read the Vault key and verify the Registry before touching data_dir,
            // like the `ku_local_web` host; private paths/bytes are never printed.
            #[cfg(feature = "base-v1")]
            let ku_runtime = match ku_vault_key_file.as_deref() {
                Some(vault_key_file) => {
                    match prepare_start_ku_runtime(&config, vault_key_file, &ku_sources, ku_ollama)
                    {
                        Ok(runtime) => Some(runtime),
                        Err(error) => {
                            eprintln!("{error}");
                            std::process::exit(1);
                        }
                    }
                }
                None => None,
            };
            #[cfg(not(feature = "base-v1"))]
            if ku_vault_key_file.is_some()
                || !ku_sources.is_empty()
                || ku_network_bind.is_some()
                || ku_ollama_executable.is_some()
            {
                eprintln!("--ku-vault-key-file requires a binary built with the base-v1 feature");
                std::process::exit(2);
            }
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
            println!(
                "  Legacy seed compatibility: {}",
                if legacy_seed_compat {
                    "enabled"
                } else {
                    "disabled"
                }
            );
            if legacy_seed_compat {
                println!("  Legacy seeds: {:?}", config.seeds);
            }
            println!("  Legacy TCP listener: {}", config.legacy_tcp_listener);
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
            let (ku_enabled, ku_issue) = (
                ku_runtime.is_some(),
                ku_runtime.as_ref().and_then(|(_, issue)| *issue),
            );
            #[cfg(feature = "base-v1")]
            {
                let mut base = onebrain_api::base_runtime_config_for_api_token(&api_token);
                base.ku = ku_runtime.map(|(ku, _)| ku);
                if let Err(error) = node.install_base_runtime(base) {
                    eprintln!("Failed to install the Base v1 runtime: {error}");
                    std::process::exit(1);
                }
            }
            // Kept alive for the whole REPL/API session; dropping it stops OBP-RP.
            #[cfg(all(feature = "base-v1", feature = "vnext-network-runtime"))]
            let ku_network_runtime = match ku_network.as_ref() {
                Some(network) => {
                    match onebrain_api::ku_host::start_host_loopback_network_runtime(
                        &config.data_dir,
                        network,
                    )
                    .await
                    {
                        Ok(runtime) => Some(runtime),
                        Err(_) => {
                            eprintln!("{KU_NETWORK_STARTUP_FAILED}");
                            std::process::exit(1);
                        }
                    }
                }
                None => None,
            };
            #[cfg(all(feature = "base-v1", not(feature = "vnext-network-runtime")))]
            let _ = ku_network;
            #[cfg(feature = "vnext-network-runtime")]
            if let Some(dependencies) = vnext_dependencies {
                if let Err(error) = node.set_vnext_product_dependencies(dependencies) {
                    eprintln!("Failed to configure vNext product runtime: {error}");
                    std::process::exit(1);
                }
            }
            println!("  \u{2713} Node initialized successfully");

            let started_kind = if config.legacy_tcp_listener == LegacyTcpListener::Disabled {
                "vNext"
            } else {
                "Legacy TCP"
            };
            #[cfg(feature = "vnext-network-runtime")]
            let network_start = if !node.network_listener_requested() {
                None
            } else if vnext.requested() && !legacy_seed_compat {
                Some(
                    node.start_vnext_network_only()
                        .await
                        .map(|addr| ("vNext", addr)),
                )
            } else {
                Some(node.start_network().await.map(|addr| (started_kind, addr)))
            };
            #[cfg(not(feature = "vnext-network-runtime"))]
            let network_start = if node.network_listener_requested() {
                Some(node.start_network().await.map(|addr| (started_kind, addr)))
            } else {
                None
            };
            match network_start {
                None if ku_network_bind.is_some() => println!(
                    "  Node networking: off (no vNext lane requested; legacy TCP listener disabled); /ku opt-in loopback OBP-RP starts below"
                ),
                None => println!(
                    "  Networking: off (no vNext lane requested; legacy TCP listener disabled)"
                ),
                Some(Ok((kind, addr))) => println!("  \u{2713} {kind} listener started on {addr}"),
                Some(Err(e)) => eprintln!(
                    "  \u{26a0} Network start failed: {} (continuing without networking)",
                    e
                ),
            }

            if legacy_seed_compat {
                for seed_addr in &seed_addrs {
                    if let Err(e) = node.connect_to_seed(*seed_addr).await {
                        eprintln!(
                            "  \u{26a0} Failed to connect to legacy peer {}: {}",
                            seed_addr, e
                        );
                    }
                }

                let mdns = mdns_discovery::try_mdns_discovery(&config.name, config.port).await;
                println!("  {}", mdns.message);
                for peer_addr in &mdns.discovered_peers {
                    println!("    LAN peer: {}", peer_addr);
                }

                let upnp_result = upnp::try_upnp_map(config.port).await;
                println!("  {}", upnp_result.message);

                #[cfg(not(feature = "legacy"))]
                {
                    println!("  ⚠ Legacy central seed client is disabled in the default build (build with `--features legacy` to enable SeedClient).");
                }

                #[cfg(feature = "legacy")]
                {
                    let peer_id = generate_peer_id();
                    let mut seed =
                        SeedClient::new(peer_id.clone(), config.name.clone(), config.port);
                    match seed.connect().await {
                        Ok(_) => {
                            if let Some(stream) = seed.stream() {
                                SeedClient::run_background(
                                    stream,
                                    peer_id.clone(),
                                    node.event_tx.clone(),
                                )
                                .await;
                            }
                            match seed.get_peers().await {
                                Ok(peers) => {
                                    println!(
                                        "  \u{2713} Found {} legacy peer(s) online",
                                        peers.len()
                                    );
                                    for p in &peers {
                                        let short_id = if p.peer_id.len() >= 8 {
                                            &p.peer_id[..8]
                                        } else {
                                            &p.peer_id
                                        };
                                        println!("    - {} ({})", p.name, short_id);
                                    }
                                }
                                Err(e) => {
                                    println!("  \u{26a0} Could not get legacy peer list: {}", e)
                                }
                            }
                        }
                        Err(e) => println!("  \u{26a0} Legacy seed connection failed: {}", e),
                    }
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
                println!(
                    "{}",
                    api_token_banner(api_token_from_file, api_token_is_default)
                );
                #[cfg(feature = "base-v1")]
                if ku_enabled {
                    #[cfg(feature = "vnext-network-runtime")]
                    let ku_network_status = ku_network_runtime.as_ref().map(|r| r.status());
                    #[cfg(not(feature = "vnext-network-runtime"))]
                    let ku_network_status: Option<()> = None;
                    if ku_network_status.is_none() {
                        println!(
                            "  ✓ Local KU: http://127.0.0.1:{}/ku — local/private only; AI unqualified; no publication",
                            api_port
                        );
                    }
                    #[cfg(feature = "vnext-network-runtime")]
                    if let Some(status) = ku_network_status {
                        println!(
                            "  ✓ Local KU: http://127.0.0.1:{}/ku — explicit D-045 Public share over opt-in loopback; AI unqualified",
                            api_port
                        );
                        println!(
                            "  ✓ Opt-in loopback OBP-RP: node_id={} listen_addr={} — explicit D-045 share only; no reward/NAT qualification",
                            onebrain_base_contract::ku_payload::hex(&status.principal),
                            status.listen_addr
                        );
                    }
                    if let Some(reason) = ku_issue {
                        eprintln!("  ⚠ {}", ku_source_warning(reason));
                    }
                } else {
                    println!("  Local KU (/ku): unavailable (start with --ku-vault-key-file and a signed Registry to enable)");
                }
                println!();
                println!("Type 'help' for available commands.");
                println!();

                // REPL uses shared node — lock per-command, not permanently
                if let Err(e) =
                    cli::run_repl_shared(shared_node.clone(), &api_token, legacy_seed_compat).await
                {
                    eprintln!("REPL error: {}", e);
                }
            } else {
                println!();
                println!("Type 'help' for available commands.");
                println!("Tip: Use --api to enable Web Dashboard");
                println!();

                if let Err(e) = cli::run_repl(&mut node, &api_token, legacy_seed_compat).await {
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

const DEFAULT_API_TOKEN: &str = "onebrain-dev-token";

const API_TOKEN_UNREADABLE: &str = "api_token_unreadable: cannot read --api-token-file. Point it \
    at an existing locally readable UTF-8 token file and keep the token outside Git and command \
    arguments.";

const API_TOKEN_TOO_LARGE: &str = "api_token_too_large: --api-token-file exceeds 1024 bytes \
    including surrounding whitespace. Supply a plain UTF-8 token file, not JSON, a key file or a \
    canonical source file.";

const API_TOKEN_INVALID: &str = "api_token_invalid: --api-token-file must contain a UTF-8 token \
    of 32..1024 ASCII letters, digits, '-' or '_' after trimming surrounding whitespace, without \
    quotes or JSON.";

/// Resolve the `onebrain start` API token before touching `data_dir` or custody.
fn resolve_start_api_token(
    api_token: String,
    api_token_file: Option<&std::path::Path>,
) -> Result<(String, bool), &'static str> {
    match api_token_file {
        Some(path) => Ok((load_api_token_file(path)?, true)),
        None => Ok((api_token, false)),
    }
}

/// Read and validate `--api-token-file` with the same 32..=1024 ASCII rule as
/// `ku_local_web`, without reflecting private file paths or bytes in errors.
fn load_api_token_file(path: &std::path::Path) -> Result<String, &'static str> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| API_TOKEN_UNREADABLE)?
        .take(1025)
        .read_to_end(&mut bytes)
        .map_err(|_| API_TOKEN_UNREADABLE)?;
    if bytes.len() > 1024 {
        return Err(API_TOKEN_TOO_LARGE);
    }
    let token = String::from_utf8(bytes)
        .map_err(|_| API_TOKEN_INVALID)?
        .trim()
        .to_owned();
    if token.len() < 32
        || !token
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(API_TOKEN_INVALID);
    }
    Ok(token)
}

/// Startup auth banner for `onebrain start --api`; takes only flags so raw
/// token bytes and private file paths can never be printed to stdout.
fn api_token_banner(from_file: bool, is_default: bool) -> &'static str {
    if from_file {
        "  ✓ Auth: Bearer token loaded from --api-token-file (not printed)"
    } else if is_default {
        "  ✓ Auth: default dev Bearer token active (not printed; pass --api-token-file for private custody)"
    } else {
        "  ✓ Auth: Bearer token set via --api-token (not printed; prefer --api-token-file)"
    }
}

/// Parse one `--ku-source LABEL=PATH`; the label is split at the first `=`.
fn parse_ku_source(value: &str) -> Result<(String, PathBuf), String> {
    match value.split_once('=') {
        Some((label, path)) if !label.is_empty() && !path.is_empty() => {
            Ok((label.to_owned(), PathBuf::from(path)))
        }
        _ => Err("expected LABEL=PATH with a non-empty label and path".into()),
    }
}

#[cfg(feature = "base-v1")]
const KU_REGISTRY_REQUIRED: &str = "ku_registry_unavailable: --ku-vault-key-file needs a signed \
    Registry. Pass --concept-registry-release-root (the root containing releases/ and activation \
    state) and --concept-registry-release-public-key (its independently trusted key); an unsigned \
    concepts.obr is insufficient. Retain the existing data_dir and Vault key when retrying.";

#[cfg(feature = "base-v1")]
const KU_REGISTRY_UNAVAILABLE: &str = "ku_registry_unavailable: cannot open the signed Registry \
    for the local KU workspace. Check --concept-registry-release-root and \
    --concept-registry-release-public-key; retain the existing data_dir and Vault key when \
    retrying.";

/// Prepare the shared node-owned KU runtime for `onebrain start --api`, using
/// the same `ku_host` helper as `ku_local_web` and Desktop. Fatal errors are
/// Vault-key or Registry setup failures; source failures keep saved reads.
#[cfg(feature = "base-v1")]
fn prepare_start_ku_runtime(
    config: &NodeConfig,
    vault_key_file: &std::path::Path,
    sources: &[(String, PathBuf)],
    ollama: Option<onebrain_api::ku_host::Ollama>,
) -> Result<
    (
        onebrain_node::ku_product::KuRuntimeConfig,
        Option<&'static str>,
    ),
    &'static str,
> {
    use onebrain_api::ku_host::{prepare_ku_runtime, KuHostInputsConfig, Source};
    let (Some(registry_root), Some(registry_public_key)) = (
        config.concept_registry_release_root.clone(),
        config.concept_registry_release_public_key.clone(),
    ) else {
        return Err(KU_REGISTRY_REQUIRED);
    };
    let inputs = KuHostInputsConfig {
        registry_root,
        registry_public_key,
        vault_key_file: vault_key_file.to_path_buf(),
        sources: sources
            .iter()
            .map(|(label, canonical_file)| Source {
                label: label.clone(),
                canonical_file: canonical_file.clone(),
            })
            .collect(),
        ollama,
    };
    let (ku, issue) = prepare_ku_runtime(config, &inputs).map_err(|reason| match reason {
        "ku_host_input_unavailable" => {
            "ku_vault_key_unreadable: cannot read --ku-vault-key-file. Point it at the \
            dataset's existing private Vault key; do not generate a replacement key or change \
            data_dir."
        }
        "ku_host_input_exceeds_limit" | "ku_vault_key_invalid" => {
            "ku_vault_key_invalid: --ku-vault-key-file must contain exactly 32 binary bytes \
            (not hex, Base64 or text). Restore the dataset's original key; do not pad, \
            truncate or replace it."
        }
        other => other,
    })?;
    if issue == Some("ku_registry_unavailable") {
        return Err(KU_REGISTRY_UNAVAILABLE);
    }
    Ok((ku, issue))
}

#[cfg(feature = "base-v1")]
const KU_OLLAMA_MODEL_LIMIT: &str = "ku_model_limit: pass 1..8 --ku-ollama-model tags with \
    --ku-ollama-executable; omit all --ku-ollama-* flags for a /ku host without AI intake.";

/// Default memory limit for the managed `/ku` Ollama worker (12 GiB), matching
/// the documented `ku_local_web` host example.
#[cfg(feature = "base-v1")]
const KU_OLLAMA_DEFAULT_MEMORY_LIMIT: u64 = 12 * 1024 * 1024 * 1024;

/// Map the opt-in `--ku-ollama-*` flags onto the shared `ku_host::Ollama`
/// config before any Registry, Vault key or dataset access.
#[cfg(feature = "base-v1")]
fn start_ku_ollama(
    executable: Option<PathBuf>,
    models_dir: Option<PathBuf>,
    models: Vec<String>,
    memory_limit_bytes: Option<u64>,
) -> Result<Option<onebrain_api::ku_host::Ollama>, &'static str> {
    let (Some(executable), Some(models_dir)) = (executable, models_dir) else {
        return Ok(None);
    };
    if models.is_empty() || models.len() > 8 || models.iter().any(|m| m.trim().is_empty()) {
        return Err(KU_OLLAMA_MODEL_LIMIT);
    }
    Ok(Some(onebrain_api::ku_host::Ollama {
        executable,
        models_dir,
        models,
        memory_limit_bytes: memory_limit_bytes.unwrap_or(KU_OLLAMA_DEFAULT_MEMORY_LIMIT),
    }))
}

#[cfg(feature = "base-v1")]
const KU_NETWORK_CONFIG_INVALID: &str = "ku_network_config_invalid: --ku-network-bind must be a \
    loopback socket address such as 127.0.0.1:0; omit it for local/private-only /ku. Retain the \
    existing data_dir and Vault key when retrying.";

#[cfg(feature = "base-v1")]
const KU_NETWORK_CONFLICTS_VNEXT: &str = "ku_network_config_invalid: --ku-network-bind cannot be \
    combined with --vnext-kql, --vnext-public-use or --vnext-pomv-view in this MVP; start /ku \
    sharing without vNext product lanes.";

#[cfg(all(feature = "base-v1", not(feature = "vnext-network-runtime")))]
const KU_NETWORK_RUNTIME_UNAVAILABLE: &str = "ku_network_runtime_unavailable: --ku-network-bind \
    needs a onebrain binary built with --features vnext-network-runtime. Omit it for \
    local/private-only /ku; retain the existing data_dir and Vault key when retrying.";

#[cfg(all(feature = "base-v1", feature = "vnext-network-runtime"))]
const KU_NETWORK_STARTUP_FAILED: &str = "ku_network_startup_failed: cannot start the opt-in \
    loopback VNextNetworkRuntime for /ku. Check that --ku-network-bind is an available local \
    address and that no other process uses this data_dir; retain the existing data_dir and Vault \
    key when retrying.";

/// Validate the opt-in `/ku` loopback network before any Registry, Vault key
/// or dataset access, with the same `KuHostNetworkConfig` rule as `ku_local_web`.
#[cfg(feature = "base-v1")]
fn validate_start_ku_network(
    bind_addr: SocketAddr,
    vnext_requested: bool,
) -> Result<onebrain_api::ku_host::KuHostNetworkConfig, &'static str> {
    let network = onebrain_api::ku_host::KuHostNetworkConfig {
        network_opt_in: true,
        bind_addr,
    };
    network.validate().map_err(|_| KU_NETWORK_CONFIG_INVALID)?;
    // The vNext product runtime owns the same network stores for data_dir.
    if vnext_requested {
        return Err(KU_NETWORK_CONFLICTS_VNEXT);
    }
    #[cfg(not(feature = "vnext-network-runtime"))]
    return Err(KU_NETWORK_RUNTIME_UNAVAILABLE);
    #[cfg(feature = "vnext-network-runtime")]
    Ok(network)
}

/// Non-fatal `/ku` source/model issues: saved KU reads stay available.
#[cfg(feature = "base-v1")]
fn ku_source_warning(reason: &'static str) -> String {
    if reason == "ku_experimental_model_unavailable" {
        return format!(
            "{reason}: a --ku-ollama-model could not be admitted (check --ku-ollama-executable, \
            --ku-ollama-models-dir and that the model is already installed; models are never \
            downloaded). AI intake offers only admitted models; manual editing and saved KU \
            reads still work."
        );
    }
    let guidance = match reason {
        "ku_host_input_unavailable" => "cannot read a --ku-source file",
        "ku_host_input_exceeds_limit" => "a --ku-source file exceeds 65536 bytes",
        "ku_source_admission_failed" => {
            "a --ku-source file is not a canonical LOCAL_ONLY Text SourceArtifact, a label is \
            not 1..128 UTF-8 bytes, or source IDs repeat"
        }
        "ku_host_source_limit" => "more than 64 --ku-source entries",
        _ => "the KU input setup is incomplete",
    };
    format!(
        "{reason}: {guidance}. Source editing is unavailable for this launch; saved KU reads \
        still work. Retry with the same data_dir and original Vault key."
    )
}

#[cfg(test)]
mod vnext_command_tests {
    use super::*;

    #[test]
    fn legacy_seed_compatibility_requires_explicit_start_opt_in() {
        let default = CliArgs::try_parse_from(["onebrain", "start"]).unwrap();
        assert!(matches!(
            default.command,
            Some(Commands::Start {
                legacy_seed_compat: false,
                seeds,
                ..
            }) if seeds.is_empty()
        ));
        assert!(
            CliArgs::try_parse_from(["onebrain", "start", "--seeds", "127.0.0.1:4242"]).is_err()
        );
        let rollback = CliArgs::try_parse_from([
            "onebrain",
            "start",
            "--legacy-seed-compat",
            "--seeds",
            "127.0.0.1:4242",
        ])
        .unwrap();
        assert!(matches!(
            rollback.command,
            Some(Commands::Start {
                legacy_seed_compat: true,
                seeds,
                ..
            }) if seeds == vec!["127.0.0.1:4242".parse().unwrap()]
        ));
    }

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

    #[test]
    fn api_token_file_conflicts_with_api_token_validates_bounds_and_redacts_banner() {
        let default = CliArgs::try_parse_from(["onebrain", "start", "--api"]).unwrap();
        let Some(Commands::Start {
            api_token,
            api_token_file,
            ..
        }) = default.command
        else {
            panic!()
        };
        assert_eq!(api_token, DEFAULT_API_TOKEN);
        assert!(api_token_file.is_none());

        let with_file = CliArgs::try_parse_from([
            "onebrain",
            "start",
            "--api",
            "--api-token-file",
            "token.txt",
        ])
        .unwrap();
        assert!(matches!(
            with_file.command,
            Some(Commands::Start {
                api_token_file: Some(ref path),
                ..
            }) if path == &PathBuf::from("token.txt")
        ));

        assert!(CliArgs::try_parse_from([
            "onebrain",
            "start",
            "--api",
            "--api-token",
            "custom-token",
            "--api-token-file",
            "token.txt",
        ])
        .is_err());

        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("private-missing-canary.txt");
        let err = resolve_start_api_token(DEFAULT_API_TOKEN.into(), Some(&missing)).unwrap_err();
        assert_eq!(err, API_TOKEN_UNREADABLE);
        assert!(!err.contains("canary"));

        let token_path = root.path().join("private-token-canary.txt");
        for (bytes, expected) in [
            (b"short-private-canary".to_vec(), API_TOKEN_INVALID),
            (vec![0xff; 32], API_TOKEN_INVALID),
            (
                format!("\"{}\"", "private-canary-".repeat(3)).into_bytes(),
                API_TOKEN_INVALID,
            ),
            (vec![b'a'; 1025], API_TOKEN_TOO_LARGE),
        ] {
            std::fs::write(&token_path, &bytes).unwrap();
            let err =
                resolve_start_api_token(DEFAULT_API_TOKEN.into(), Some(&token_path)).unwrap_err();
            assert_eq!(err, expected);
            assert!(!err.contains("canary"));
            assert!(!err.contains(&root.path().display().to_string()));
            assert_eq!(std::fs::read(&token_path).unwrap(), bytes);
        }

        let valid = "synthetic-private-token-canary-1234567890";
        std::fs::write(&token_path, format!(" \r\n{valid}\t ")).unwrap();
        let (loaded, from_file) =
            resolve_start_api_token(DEFAULT_API_TOKEN.into(), Some(&token_path)).unwrap();
        assert!(from_file);
        assert_eq!(loaded, valid);

        std::fs::write(&token_path, vec![b'a'; 1024]).unwrap();
        assert_eq!(load_api_token_file(&token_path).unwrap().len(), 1024);

        let (inline, from_file) = resolve_start_api_token("inline-secret".into(), None).unwrap();
        assert!(!from_file);
        assert_eq!(inline, "inline-secret");

        for (from_file, is_default) in [(true, false), (false, true), (false, false)] {
            let banner = api_token_banner(from_file, is_default);
            assert!(!banner.contains(DEFAULT_API_TOKEN));
            assert!(!banner.contains(valid));
            assert!(!banner.contains("inline-secret"));
        }
        assert!(api_token_banner(true, false).contains("--api-token-file"));
        assert!(api_token_banner(false, true).contains("default dev Bearer token"));
        assert!(api_token_banner(false, false).contains("--api-token"));
    }
}

#[cfg(all(test, feature = "base-v1"))]
#[path = "../../onebrain-api/src/ku_api/tests/registry_fixture.rs"]
#[allow(dead_code)]
mod registry_fixture;

#[cfg(all(test, feature = "base-v1"))]
mod start_ku_tests {
    use super::*;

    fn start_args(argv: &[&str]) -> Result<CliArgs, clap::Error> {
        let mut full = vec!["onebrain", "start"];
        full.extend_from_slice(argv);
        CliArgs::try_parse_from(full)
    }

    #[test]
    fn ku_flags_require_api_and_vault_key_and_parse_label_path() {
        assert!(start_args(&["--ku-vault-key-file", "vault.key"]).is_err());
        assert!(start_args(&["--api", "--ku-source", "Label=source.canonical"]).is_err());
        let parsed = start_args(&[
            "--api",
            "--ku-vault-key-file",
            "vault.key",
            "--ku-source",
            "My label=a=b.canonical",
        ])
        .unwrap();
        assert!(matches!(
            parsed.command,
            Some(Commands::Start { ku_vault_key_file: Some(_), ref ku_sources, .. })
                if ku_sources == &vec![("My label".to_owned(), PathBuf::from("a=b.canonical"))]
        ));
        for bad in ["no-separator", "=path", "label="] {
            assert!(parse_ku_source(bad).is_err(), "{bad}");
        }
        let default = start_args(&["--api"]).unwrap();
        assert!(matches!(
            default.command,
            Some(Commands::Start { ku_vault_key_file: None, ref ku_sources, .. }) if ku_sources.is_empty()
        ));
    }

    #[tokio::test]
    async fn ku_runtime_setup_fails_closed_and_keeps_saved_reads_for_source_issues() {
        let root = tempfile::tempdir().unwrap();
        drop(registry_fixture::registry(root.path()));
        let public_key = onebrain_base_contract::ku_payload::hex(
            ed25519_dalek::SigningKey::from_bytes(&[0x42; 32])
                .verifying_key()
                .as_bytes(),
        );
        let key_path = root.path().join("vault.key");
        std::fs::write(&key_path, [0x51; 32]).unwrap();
        let data_dir = root.path().join("dataset");
        let config = |release_root: Option<PathBuf>, key: Option<String>| NodeConfig {
            data_dir: data_dir.clone(),
            concept_registry_release_root: release_root,
            concept_registry_release_public_key: key,
            ..Default::default()
        };
        let signed = config(Some(root.path().join("registry")), Some(public_key.clone()));

        // No signed Registry, unreadable or wrong-size key: fatal, data_dir untouched.
        let error = prepare_start_ku_runtime(&config(None, None), &key_path, &[], None)
            .err()
            .unwrap();
        assert_eq!(error, KU_REGISTRY_REQUIRED);
        let missing = root.path().join("private-missing-canary.key");
        let error = prepare_start_ku_runtime(&signed, &missing, &[], None)
            .err()
            .unwrap();
        assert!(error.starts_with("ku_vault_key_unreadable:"));
        assert!(!error.contains("canary"));
        let short = root.path().join("short.key");
        std::fs::write(&short, [1; 31]).unwrap();
        let error = prepare_start_ku_runtime(&signed, &short, &[], None)
            .err()
            .unwrap();
        assert!(error.starts_with("ku_vault_key_invalid:"));
        let wrong_key = config(Some(root.path().join("registry")), Some("00".repeat(32)));
        let error = prepare_start_ku_runtime(&wrong_key, &key_path, &[], None)
            .err()
            .unwrap();
        assert_eq!(error, KU_REGISTRY_UNAVAILABLE);
        assert!(!data_dir.exists());

        // Signed Registry + key: usable runtime; a source issue is a warning only.
        let (_, issue) = prepare_start_ku_runtime(&signed, &key_path, &[], None).unwrap();
        assert!(issue.is_none());
        let (_, issue) = prepare_start_ku_runtime(
            &signed,
            &key_path,
            &[(
                "label".into(),
                root.path().join("private-source-canary.canonical"),
            )],
            None,
        )
        .unwrap();
        assert_eq!(issue, Some("ku_host_input_unavailable"));
        let warning = ku_source_warning(issue.unwrap());
        assert!(warning.starts_with("ku_host_input_unavailable:"));
        assert!(warning.contains("saved KU reads"));
        assert!(!warning.contains("canary"));

        // Opt-in Ollama with an unavailable executable is a model warning only:
        // the runtime still starts (manual editing and saved reads keep working).
        let ollama = start_ku_ollama(
            Some(root.path().join("missing-ollama-canary.exe")),
            Some(root.path().join("models")),
            vec!["qwen3:8b".into()],
            None,
        )
        .unwrap()
        .unwrap();
        assert_eq!(ollama.memory_limit_bytes, KU_OLLAMA_DEFAULT_MEMORY_LIMIT);
        let (_, issue) = prepare_start_ku_runtime(&signed, &key_path, &[], Some(ollama)).unwrap();
        assert_eq!(issue, Some("ku_experimental_model_unavailable"));
        let warning = ku_source_warning(issue.unwrap());
        assert!(warning.contains("manual editing and saved KU reads still work"));
        assert!(!warning.contains("canary"));
        assert_eq!(std::fs::read(&key_path).unwrap(), [0x51; 32]);
    }

    #[test]
    fn ku_ollama_flags_are_opt_in_and_bounded() {
        let default = start_args(&["--api", "--ku-vault-key-file", "vault.key"]).unwrap();
        assert!(matches!(
            default.command,
            Some(Commands::Start { ku_ollama_executable: None, ref ku_ollama_models, .. })
                if ku_ollama_models.is_empty()
        ));
        // Executable needs the Vault key, a models dir and at least one model.
        assert!(start_args(&[
            "--api",
            "--ku-ollama-executable",
            "ollama.exe",
            "--ku-ollama-models-dir",
            "models",
            "--ku-ollama-model",
            "qwen3:8b"
        ])
        .is_err());
        for missing in [
            &["--ku-ollama-model", "qwen3:8b"][..],
            &["--ku-ollama-models-dir", "models"][..],
        ] {
            let mut args = vec!["--api", "--ku-vault-key-file", "vault.key"];
            args.extend_from_slice(&["--ku-ollama-executable", "ollama.exe"]);
            args.extend_from_slice(missing);
            assert!(start_args(&args).is_err(), "{missing:?}");
        }
        assert!(start_args(&[
            "--api",
            "--ku-vault-key-file",
            "v",
            "--ku-ollama-model",
            "m"
        ])
        .is_err());
        let parsed = start_args(&[
            "--api",
            "--ku-vault-key-file",
            "vault.key",
            "--ku-ollama-executable",
            "ollama.exe",
            "--ku-ollama-models-dir",
            "models",
            "--ku-ollama-model",
            "qwen3:8b",
            "--ku-ollama-model",
            "qwen3:4b",
            "--ku-ollama-memory-limit-bytes",
            "1024",
        ])
        .unwrap();
        let Some(Commands::Start {
            ku_ollama_executable,
            ku_ollama_models_dir,
            ku_ollama_models,
            ku_ollama_memory_limit_bytes,
            ..
        }) = parsed.command
        else {
            panic!()
        };
        let ollama = start_ku_ollama(
            ku_ollama_executable,
            ku_ollama_models_dir,
            ku_ollama_models,
            ku_ollama_memory_limit_bytes,
        )
        .unwrap()
        .unwrap();
        assert_eq!(ollama.executable, PathBuf::from("ollama.exe"));
        assert_eq!(ollama.models_dir, PathBuf::from("models"));
        assert_eq!(ollama.models, vec!["qwen3:8b", "qwen3:4b"]);
        assert_eq!(ollama.memory_limit_bytes, 1024);
        assert!(start_ku_ollama(None, None, vec![], None).unwrap().is_none());
        let nine = (0..9).map(|i| format!("m{i}")).collect();
        assert_eq!(
            start_ku_ollama(Some("o".into()), Some("d".into()), nine, None).err(),
            Some(KU_OLLAMA_MODEL_LIMIT)
        );
        assert_eq!(
            start_ku_ollama(Some("o".into()), Some("d".into()), vec![" ".into()], None).err(),
            Some(KU_OLLAMA_MODEL_LIMIT)
        );
    }

    #[test]
    fn ku_network_bind_is_opt_in_loopback_and_requires_vault_key() {
        // Off by default; requires the KU Vault key (and so --api).
        let default = start_args(&["--api", "--ku-vault-key-file", "vault.key"]).unwrap();
        assert!(matches!(
            default.command,
            Some(Commands::Start {
                ku_network_bind: None,
                ..
            })
        ));
        assert!(start_args(&["--api", "--ku-network-bind", "127.0.0.1:0"]).is_err());
        assert!(start_args(&[
            "--api",
            "--ku-vault-key-file",
            "vault.key",
            "--ku-network-bind",
            "not-an-address"
        ])
        .is_err());
        let parsed = start_args(&[
            "--api",
            "--ku-vault-key-file",
            "vault.key",
            "--ku-network-bind",
            "127.0.0.1:0",
        ])
        .unwrap();
        assert!(matches!(
            parsed.command,
            Some(Commands::Start { ku_network_bind: Some(addr), .. })
                if addr == "127.0.0.1:0".parse::<SocketAddr>().unwrap()
        ));

        // Non-loopback or vNext-lane combinations fail closed before any I/O.
        for addr in ["0.0.0.0:0", "192.168.1.10:4500", "[::]:0"] {
            assert_eq!(
                validate_start_ku_network(addr.parse().unwrap(), false).err(),
                Some(KU_NETWORK_CONFIG_INVALID),
                "{addr}"
            );
        }
        assert_eq!(
            validate_start_ku_network("127.0.0.1:0".parse().unwrap(), true).err(),
            Some(KU_NETWORK_CONFLICTS_VNEXT)
        );
        let loopback = validate_start_ku_network("[::1]:0".parse().unwrap(), false);
        #[cfg(feature = "vnext-network-runtime")]
        assert!(loopback.unwrap().network_opt_in);
        #[cfg(not(feature = "vnext-network-runtime"))]
        assert_eq!(loopback.err(), Some(KU_NETWORK_RUNTIME_UNAVAILABLE));
    }
}
