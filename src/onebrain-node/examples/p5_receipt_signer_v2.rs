use onebrain_node::vnext_p5_signer_provider::{run_signer_service_cli, P5SignerServiceKindV2};

fn main() {
    if let Err(error) = run_signer_service_cli(P5SignerServiceKindV2::Receipt) {
        eprintln!("P5 receipt signer failed: {error}");
        std::process::exit(1);
    }
}
