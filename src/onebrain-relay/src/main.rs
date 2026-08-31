use std::path::Path;

use onebrain_relay::{
    activate_descriptor, export_candidate_descriptor, generate_identity, initialize_state, serve,
    verify_config,
};

fn main() {
    if let Err(error) = run(std::env::args().skip(1).collect()) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run(arguments: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    match arguments.as_slice() {
        [mode, output_flag, output] if mode == "generate-identity" && output_flag == "--output" => {
            let public = generate_identity(Path::new(output))?;
            println!("{}", encode_hex(&public));
        }
        [mode, config_flag, config] if mode == "initialize-state" && config_flag == "--config" => {
            initialize_state(Path::new(config))?;
        }
        [mode, config_flag, config] if mode == "verify-config" && config_flag == "--config" => {
            verify_config(Path::new(config))?;
            println!("verified");
        }
        [mode, config_flag, config, output_flag, output]
            if mode == "export-candidate-descriptor"
                && config_flag == "--config"
                && output_flag == "--output" =>
        {
            println!(
                "{}",
                encode_hex(&export_candidate_descriptor(
                    Path::new(config),
                    Path::new(output)
                )?)
            );
        }
        [mode, config_flag, config] if mode == "serve" && config_flag == "--config" => {
            serve(Path::new(config), false)?;
        }
        [mode, config_flag, config, preflight]
            if mode == "serve" && config_flag == "--config" && preflight == "--preflight-only" =>
        {
            serve(Path::new(config), true)?;
        }
        [mode, config_flag, config, probes_flag, probes]
            if mode == "activate-descriptor"
                && config_flag == "--config"
                && probes_flag == "--probe-set" =>
        {
            println!(
                "{}",
                encode_hex(&activate_descriptor(Path::new(config), Path::new(probes))?)
            );
        }
        _ => return Err("OBP_RELAY_CLI: invalid closed command".into()),
    }
    Ok(())
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
