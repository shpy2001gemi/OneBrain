use clap::Parser;
use std::net::SocketAddr;

mod registry;
mod relay;
mod server;

#[derive(Parser)]
#[command(name = "onebrain-seed")]
#[command(about = "Legacy TCP/JSON seed compatibility daemon; not a vNext relay")]
struct Cli {
    /// Explicitly run the legacy compatibility daemon.
    #[arg(long)]
    legacy_seed_compat: bool,
    /// Port to listen on
    #[arg(long, default_value_t = 4242)]
    port: u16,

    /// Maximum peers to track
    #[arg(long, default_value_t = 10000)]
    max_peers: usize,

    /// Seed node display name
    #[arg(long, default_value = "OneBrain Seed")]
    name: String,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if !cli.legacy_seed_compat {
        eprintln!(
            "onebrain-seed is legacy compatibility only; pass --legacy-seed-compat explicitly"
        );
        std::process::exit(2);
    }

    println!("╔══════════════════════════════════════╗");
    println!("║    OneBrain Seed Node Starting...     ║");
    println!("╚══════════════════════════════════════╝");
    println!();
    println!("  Name:       {}", cli.name);
    println!("  Port:       {}", cli.port);
    println!("  Max Peers:  {}", cli.max_peers);
    println!();

    let bind_addr: SocketAddr = ([0, 0, 0, 0], cli.port).into();

    match server::run_seed_server(bind_addr, &cli.name, cli.max_peers).await {
        Ok(_) => println!("Seed node shut down gracefully."),
        Err(e) => eprintln!("Seed node error: {}", e),
    }
}
