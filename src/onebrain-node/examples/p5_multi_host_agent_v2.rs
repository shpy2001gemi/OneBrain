//! Long-lived P5 V2 agent process. Private keys are intentionally absent.

use std::fs;
#[cfg(unix)]
use std::io::{Read, Write};

use onebrain_base_contract::{SourceCommitId, SourceCommitIdentity, ToolchainIdentity};
use onebrain_node::compiled_base_runtime_config;

const SESSION_CONFIG: &str = "/run/onebrain/p5-v2/current-session.json";
const IDENTITY_SOCKET: &str = "/run/onebrain/p5-v2/identity-signer.sock";
const RECEIPT_SOCKET: &str = "/run/onebrain/p5-v2/receipt-signer.sock";
#[cfg(unix)]
const MAX_FRAME: usize = 65_536;

fn main() {
    if let Err(error) = run() {
        eprintln!("P5 V2 agent failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--print-compiled-binding"] {
        return print_compiled_binding();
    }
    if args
        != [
            "--control-socket-fd",
            "3",
            "--identity-signer-socket",
            IDENTITY_SOCKET,
            "--receipt-signer-socket",
            RECEIPT_SOCKET,
            "--session-config",
            SESSION_CONFIG,
            "--bind",
            "0.0.0.0:41010",
        ]
    {
        return Err("expected fixed P5 V2 socket/config arguments".into());
    }
    if !cfg!(target_os = "linux") {
        return Err("P5 V2 service requires Linux".into());
    }
    let metadata = fs::symlink_metadata(SESSION_CONFIG)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("invalid session config".into());
    }
    serve_fd3()
}

#[cfg(unix)]
fn serve_fd3() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::fd::FromRawFd;
    use std::os::unix::net::UnixListener;
    let listener = unsafe { UnixListener::from_raw_fd(3) };
    loop {
        let (mut stream, _) = listener.accept()?;
        let mut length = [0u8; 4];
        stream.read_exact(&mut length)?;
        let length = u32::from_be_bytes(length) as usize;
        if length == 0 || length > MAX_FRAME {
            continue;
        }
        let mut frame = vec![0; length];
        stream.read_exact(&mut frame)?;
        // Task 14 supplies orchestration. Task 13's service boundary returns a
        // closed rejection until a fully verified command executor is wired.
        let response = b"P5_V2_COMMAND_EXECUTOR_NOT_ATTACHED";
        stream.write_all(&(response.len() as u32).to_be_bytes())?;
        stream.write_all(response)?;
        stream.flush()?;
    }
}

#[cfg(not(unix))]
fn serve_fd3() -> Result<(), Box<dyn std::error::Error>> {
    Err("fd 3 listener requires Unix".into())
}

fn print_compiled_binding() -> Result<(), Box<dyn std::error::Error>> {
    let compiled = compiled_base_runtime_config();
    let tuple = &compiled.compatibility_policy.current;
    let commit = match tuple.base_commit {
        SourceCommitIdentity::Known(SourceCommitId::Sha1(value)) => hex(&value.0),
        _ => return Err("known SHA-1 candidate commit required".into()),
    };
    let toolchain = match tuple.toolchain {
        ToolchainIdentity::Known(value) => hex(&value.0),
        ToolchainIdentity::Unknown => return Err("known toolchain required".into()),
    };
    let value = serde_json::json!({
        "agent_binary_identity": hex(blake3::hash(env!("CARGO_PKG_NAME").as_bytes()).as_bytes()),
        "candidate_commit": commit,
        "candidate_tree": option_env!("ONEBRAIN_SOURCE_TREE").unwrap_or("bound-by-bundle-provenance"),
        "format": "onebrain/p5-compiled-binding/2",
        "profile_digest": option_env!("ONEBRAIN_P5_PROFILE_BLAKE3").unwrap_or("bound-by-session-config"),
        "toolchain_digest": toolchain,
        "vector_digest": option_env!("ONEBRAIN_P5_VECTOR_BLAKE3").unwrap_or("bound-by-session-config")
    });
    println!("{}", serde_json::to_string(&value)?);
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
