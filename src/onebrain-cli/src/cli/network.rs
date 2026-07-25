//! Network commands: connect, status, peers.

use onebrain_node::node::OneBrainNode;

use super::helpers::*;

pub(crate) async fn cmd_connect(node: &OneBrainNode, args: &str) {
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

pub(crate) async fn cmd_status(node: &OneBrainNode) {
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

    let registry = node.concept_registry_status();
    println!();
    println!("  Concept Registry");
    println!("  Policy:     {}", registry.mode);
    println!("  State:      {:?}", registry.state);
    println!("  Path:       {}", registry.path.display());
    println!("  Encoder:    v{}", registry.encoder_version);
    println!("  Backend:    {:?}", registry.backend);
    println!("  Cache cap:  {}", registry.cache_capacity);
    if let Some(schema_version) = registry.obr_schema_version {
        println!("  OBR schema: v{}", schema_version);
    }
    if let Some(manifest_version) = registry.manifest_version {
        println!("  Manifest:   v{}", manifest_version);
    }
    if let Some(concept_count) = registry.concept_count {
        println!("  Concepts:   {}", concept_count);
    }
    if let Some(label_count) = registry.label_count {
        println!("  Labels:     {}", label_count);
    }
    if let Some(error) = &registry.error {
        println!("  Failure:    {:?}", registry.failure_kind);
        println!("  Error:      {}", error);
    }
    if let Some(checksum) = &registry.checksum_blake3 {
        println!("  BLAKE3:     {}", checksum);
    }
    for (source, snapshot) in &registry.source_snapshots {
        println!("  Source:     {} = {}", source, snapshot);
    }

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

    let vnext = node.vnext_status();
    println!();
    println!("  ── vNext scoped status ──");
    println!("  Usability:  {:?}", vnext.usability);
    println!(
        "  Reach:      {:?} ({} observed peer(s))",
        vnext.reachability.scope, vnext.reachability.observed_peer_count
    );
    println!(
        "  Coverage:   {:?}; frontier: {}",
        vnext.coverage.status,
        if vnext.coverage.assessed_frontier.is_some() {
            "assessed"
        } else {
            "not available"
        }
    );
    println!("  Fidelity:   {:?}", vnext.fidelity.status);
    println!(
        "  OBP-RP:     {:?}{}",
        vnext.network_runtime.lifecycle,
        vnext
            .network_runtime
            .listen_addr
            .as_deref()
            .map(|addr| format!(" on {addr}"))
            .unwrap_or_default()
    );
    println!(
        "  Sessions:   {} authenticated, {} active; records accepted/deferred/rejected={}/{}/{}",
        vnext.network_runtime.authenticated_sessions,
        vnext.network_runtime.active_sessions,
        vnext.network_runtime.accepted_records,
        vnext.network_runtime.deferred_records,
        vnext.network_runtime.rejected_records
    );
    if !vnext.coverage.limitations.is_empty() {
        println!("  Limits:     {}", vnext.coverage.limitations.join(", "));
    }
    for warning in &vnext.legacy.warnings {
        println!("  Legacy:     ⚠ {}", warning);
    }
    println!(
        "  Consent:    publish={:?}, need-disclosure={:?}, remote-cognition={:?}",
        vnext.consent.knowledge_publish,
        vnext.consent.public_need_disclosure,
        vnext.consent.remote_cognition
    );

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

pub(crate) fn cmd_peers(node: &OneBrainNode) {
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
                .map(|d| format_elapsed(d))
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
