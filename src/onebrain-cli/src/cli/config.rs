//! Node configuration commands.

use onebrain_node::node::OneBrainNode;

pub(crate) fn cmd_config(node: &mut OneBrainNode, args: &str) {
    let subcmd = args.trim();

    if subcmd.is_empty() {
        // View config
        let config_view = node.get_config_view();
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
                println!(
                    "  ✓ {} updated to \"{}\" (takes effect next restart)",
                    key, value
                );
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
