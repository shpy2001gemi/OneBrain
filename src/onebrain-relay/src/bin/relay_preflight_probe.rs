use std::io::Read;

const MAX_REQUEST_BYTES: u64 = 65_536;

fn main() {
    if std::env::args_os().len() != 1 {
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
