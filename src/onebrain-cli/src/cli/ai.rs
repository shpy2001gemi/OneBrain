//! AI model management and chat commands.

use onebrain_node::node::OneBrainNode;
use onebrain_node::types::ModelInfo;

pub(crate) async fn cmd_model(node: &mut OneBrainNode, args: &str) {
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

                let installed: Vec<&ModelInfo> = models.iter().filter(|m| m.is_installed).collect();
                let not_installed: Vec<&ModelInfo> =
                    models.iter().filter(|m| !m.is_installed).collect();

                if !installed.is_empty() {
                    println!();
                    println!("  Available (installed in Ollama):");
                    for m in &installed {
                        let current = if m.is_current { "  [current]" } else { "" };
                        let star = if m.is_current { "★" } else { " " };
                        println!("    {} {:<20} {:<12}{}", star, m.name, m.params, current);
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
                    println!("  Latency:   {}ms ({})", health.latency_ms, latency_quality);
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

pub(crate) async fn cmd_chat(node: &mut OneBrainNode, input: &str) {
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
