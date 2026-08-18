use std::io::Read;

const MAX_REQUEST_BYTES: u64 = 65_536;

fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments == ["--print-compiled-binding"] {
        let value = serde_json::json!({
            "candidate_commit": option_env!("ONEBRAIN_BASE_COMMIT").unwrap_or("unbound"),
            "candidate_tree": option_env!("ONEBRAIN_SOURCE_TREE").unwrap_or("bound-by-bundle-provenance"),
            "format": "onebrain/relay-preflight-compiled-binding/1",
            "toolchain_digest": option_env!("ONEBRAIN_TOOLCHAIN_DIGEST").unwrap_or("unbound")
        });
        println!(
            "{}",
            serde_json::to_string(&value).expect("closed JSON is serializable")
        );
        return;
    }
    if !arguments.is_empty() {
        eprintln!("OBP_RELAY_PREFLIGHT: argv forbidden");
        std::process::exit(2);
    }
    let mut request = Vec::new();
    if std::io::stdin()
        .take(MAX_REQUEST_BYTES + 1)
        .read_to_end(&mut request)
        .is_err()
        || request.is_empty()
        || request.len() as u64 > MAX_REQUEST_BYTES
    {
        eprintln!("OBP_RELAY_PREFLIGHT: invalid bounded request");
        std::process::exit(2);
    }
    // The controller-signed P5 request/transcript schema and real
    // possession-only carrier are installed by the P5 and Task 8 layers.
    // Until both are present, the one-shot binary is deliberately fail-closed.
    eprintln!("OBP_RELAY_PREFLIGHT: transport adapter unavailable");
    std::process::exit(3);
}
