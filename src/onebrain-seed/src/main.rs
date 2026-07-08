use clap::Parser;
use std::net::SocketAddr;

mod registry;
mod relay;
mod server;

#[derive(Parser)]
#[command(name = "onebrain-seed")]
#[command(about = "OneBrain Seed Node — P2P relay and peer discovery")]
struct Cli {
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
