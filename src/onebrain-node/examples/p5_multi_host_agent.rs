//! Production P5 host-agent executable.
//!
//! The executable is deliberately default-off and Linux-only at runtime. It
//! reads bounded signed JSON over SSH stdio while all application delivery is
//! performed by the authenticated QUIC runtime. Signing keys are explicit
//! host-owned files outside the repository.

use std::fs;
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ed25519_dalek::SigningKey;
use onebrain_base_contract::{SourceCommitId, SourceCommitIdentity, ToolchainIdentity};
use onebrain_node::{
    compiled_base_runtime_config, BaseRuntime, DatasetGenerationStore, OperationalCompactionPolicy,
    OperationalCompactionStore, P5DirectoryRootObserver, P5HostAgentConfig,
    P5LinuxProcessResourceObserver, P5MultiHostAgent, VNextNetworkPolicy, VNextNetworkRuntime,
    P5_MAX_CONTROL_MESSAGE_BYTES,
};
use zeroize::{Zeroize, Zeroizing};

struct Arguments {
    config: PathBuf,
    data_root: PathBuf,
    bind: SocketAddr,
    receipt_signing_key: PathBuf,
    network_signing_key: PathBuf,
}

enum Mode {
    PrintCompiledBinding,
    Run(Arguments),
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("P5 multi-host agent failed: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    if !cfg!(target_os = "linux") {
        return Err("production P5 host agent requires Linux".into());
    }
    let arguments = match arguments()? {
        Mode::PrintCompiledBinding => {
            print_compiled_binding()?;
            return Ok(());
        }
        Mode::Run(arguments) => arguments,
    };
    let config: P5HostAgentConfig = serde_json::from_slice(&fs::read(&arguments.config)?)?;
    fs::create_dir_all(&arguments.data_root)?;

    let receipt_key = read_signing_key(&arguments.receipt_signing_key)?;
    let network_key = Arc::new(read_signing_key(&arguments.network_signing_key)?);
    let network_root = arguments.data_root.join("network");
    let network = Arc::new(
        VNextNetworkRuntime::start_with_signer(
            &network_root,
            arguments.bind,
            VNextNetworkPolicy::default(),
            network_key,
        )
        .await?,
    );

    let operational_path = network_root.join("vnext_operational_compaction.redb");
    let _operational = OperationalCompactionStore::open(
        &operational_path,
        OperationalCompactionPolicy::default(),
    )?;
    let generations = Arc::new(DatasetGenerationStore::open_exclusive(
        &arguments.data_root.join("base"),
    )?);
    let mut base_config = compiled_base_runtime_config();
    base_config.network_enabled = true;
    let mut base_runtime = BaseRuntime::open(generations, base_config)?;
    let base_services = base_runtime.services()?;

    let roots = Arc::new(P5DirectoryRootObserver::new(
        network_root.join("vnext_verified.redb"),
        network_root.join("vnext_reconciliation.redb"),
        network_root.join("vnext_outbox.redb"),
        operational_path,
    ));
    let resources = Arc::new(P5LinuxProcessResourceObserver::new(
        arguments.data_root.clone(),
    )?);
    let agent = P5MultiHostAgent::production(
        config,
        arguments.data_root.join("p5-control-journal.json"),
        receipt_key,
        base_services,
        None,
        Arc::clone(&network),
        roots,
        resources,
    )?;

    let mut bounded = std::io::stdin()
        .lock()
        .take((P5_MAX_CONTROL_MESSAGE_BYTES + 1) as u64);
    let mut input = Vec::new();
    bounded.read_to_end(&mut input)?;
    if input.len() > P5_MAX_CONTROL_MESSAGE_BYTES {
        return Err("signed control batch exceeds the fixed byte bound".into());
    }
    let commands: Vec<serde_json::Value> = serde_json::from_slice(&input)?;
    let mut receipts = Vec::with_capacity(commands.len());
    for command in commands {
        let bytes = canonical_json(command)?;
        receipts.push(agent.execute_json(&bytes).await?);
    }
    let output = canonical_json(serde_json::to_value(receipts)?)?;
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&output)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;

    drop(agent);
    if let Ok(mut network) = Arc::try_unwrap(network) {
        network.shutdown().await;
    }
    base_runtime.close().await?;
    Ok(())
}

fn arguments() -> Result<Mode, Box<dyn std::error::Error>> {
    let raw = std::env::args().skip(1).collect::<Vec<_>>();
    if raw == ["--print-compiled-binding"] {
        return Ok(Mode::PrintCompiledBinding);
    }
    let mut config = None;
    let mut data_root = None;
    let mut bind = None;
    let mut receipt_signing_key = None;
    let mut network_signing_key = None;
    let mut values = raw.into_iter();
    while let Some(argument) = values.next() {
        let value = values
            .next()
            .ok_or_else(|| format!("{argument} requires a value"))?;
        match argument.as_str() {
            "--config" => config = Some(PathBuf::from(value)),
            "--data-root" => data_root = Some(PathBuf::from(value)),
            "--bind" => bind = Some(value.parse::<SocketAddr>()?),
            "--receipt-signing-key" => receipt_signing_key = Some(PathBuf::from(value)),
            "--network-signing-key" => network_signing_key = Some(PathBuf::from(value)),
            _ => return Err(format!("unknown argument: {argument}").into()),
        }
    }
    Ok(Mode::Run(Arguments {
        config: config.ok_or("--config is required")?,
        data_root: data_root.ok_or("--data-root is required")?,
        bind: bind.ok_or("--bind is required")?,
        receipt_signing_key: receipt_signing_key.ok_or("--receipt-signing-key is required")?,
        network_signing_key: network_signing_key.ok_or("--network-signing-key is required")?,
    }))
}

fn print_compiled_binding() -> Result<(), Box<dyn std::error::Error>> {
    let compiled = compiled_base_runtime_config();
    let tuple = &compiled.compatibility_policy.current;
    let commit = match tuple.base_commit {
        SourceCommitIdentity::Known(SourceCommitId::Sha1(value)) => hex(&value.0),
        SourceCommitIdentity::Known(SourceCommitId::Sha256(_)) => {
            return Err("P5 production candidate requires a SHA-1 Git commit".into())
        }
        SourceCommitIdentity::Unknown => {
            return Err("P5 production candidate commit is unknown".into())
        }
    };
    let toolchain = match tuple.toolchain {
        ToolchainIdentity::Known(value) => hex(&value.0),
        ToolchainIdentity::Unknown => {
            return Err("P5 production toolchain identity is unknown".into())
        }
    };
    let value = serde_json::json!({
        "candidate_commit": commit,
        "candidate_semantic_digest": hex(&compiled.version_status.candidate_semantic_digest.0),
        "format": "onebrain/p5-compiled-binding/1",
        "linux_artifact_tuple_digest": hex(&compiled.version_status.artifact_tuple_digest.0),
        "target_triple": tuple.target_triple.as_str(),
        "toolchain_digest": toolchain,
    });
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&canonical_json(value)?)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn read_signing_key(path: &Path) -> Result<SigningKey, Box<dyn std::error::Error>> {
    let bytes = Zeroizing::new(fs::read(path)?);
    let decoded = if bytes.len() == 32 {
        Zeroizing::new(bytes.to_vec())
    } else {
        let text = std::str::from_utf8(&bytes)?.trim();
        if text.len() != 64
            || !text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("signing-key file is neither raw nor canonical lowercase hex".into());
        }
        Zeroizing::new(
            (0..text.len())
                .step_by(2)
                .map(|index| u8::from_str_radix(&text[index..index + 2], 16))
                .collect::<Result<Vec<_>, _>>()?,
        )
    };
    if decoded.len() != 32 {
        return Err("signing-key file must contain a 32-byte Ed25519 seed".into());
    }
    let mut key = [0_u8; 32];
    key.copy_from_slice(&decoded);
    let signing = SigningKey::from_bytes(&key);
    key.zeroize();
    Ok(signing)
}

fn canonical_json(value: serde_json::Value) -> Result<Vec<u8>, serde_json::Error> {
    fn sort(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.into_iter().map(sort).collect())
            }
            serde_json::Value::Object(values) => {
                let ordered = values
                    .into_iter()
                    .map(|(key, value)| (key, sort(value)))
                    .collect::<std::collections::BTreeMap<_, _>>();
                serde_json::to_value(ordered).expect("BTreeMap JSON serialization cannot fail")
            }
            scalar => scalar,
        }
    }
    serde_json::to_vec(&sort(value))
}
