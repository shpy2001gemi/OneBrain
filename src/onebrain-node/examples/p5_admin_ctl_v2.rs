//! Fixed privileged P5 V2 admin boundary.
//!
//! This executable accepts one bounded canonical frame on stdin and exposes no
//! shell, executable, service, interface, address, or path argument.

use std::io::{Read, Write};

const MAX_ADMIN_FRAME: u64 = 4_194_304;

fn main() {
    if std::env::args().len() != 1 {
        eprintln!("p5_admin_ctl_v2 accepts no arguments");
        std::process::exit(2);
    }
    if let Err(error) = run() {
        eprintln!("P5 V2 admin boundary failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    if !cfg!(target_os = "linux") {
        return Err("P5 V2 admin boundary requires Linux".into());
    }
    let mut bytes = Vec::new();
    std::io::stdin()
        .lock()
        .take(MAX_ADMIN_FRAME + 1)
        .read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_ADMIN_FRAME {
        return Err("admin frame outside fixed bound".into());
    }
    // Task 15 wires the verified native mutation backend. Until then this
    // boundary is deliberately fail-closed, but still supplies a source-free
    // executable for packaging and forced-command verification.
    let response = serde_json::json!({
        "accepted": false,
        "frame_blake3": bytes.iter().fold(blake3::Hasher::new(), |mut h, b| { h.update(&[*b]); h }).finalize().to_hex().to_string(),
        "format": "onebrain/p5/admin-response-stub/2",
        "reason": "P5_V2_ADMIN_BACKEND_NOT_ATTACHED"
    });
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(serde_json::to_string(&response)?.as_bytes())?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}
