//! Fixed privileged P5 V2 admin boundary.
//!
//! This executable accepts one bounded canonical frame on stdin and exposes no
//! shell, executable, service, interface, address, or path argument.

use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(unix)]
use std::{ffi::CString, os::unix::ffi::OsStrExt};

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use onebrain_node::vnext_p5_disk_pressure::P5DiskPressureBackend;
use onebrain_node::vnext_p5_linux_admin::{P5LinuxAdminBackend, ProductionLinuxCommandRunner};
use onebrain_node::vnext_p5_multi_host_v2::P5FaultKindV2;
use onebrain_node::vnext_p5_recovery_ops_v2::{
    explicit_re_enable, obarv002_restore, prepare_obarv002_fixture, rollback, verify_inputs,
    verify_previous_generation, P5RecoveryInputsV2, P5RecoveryOperationV2,
};
use onebrain_node::vnext_p5_signer_provider::DurableSequenceCursor;
#[cfg(unix)]
use onebrain_node::vnext_p5_signer_provider::{
    P5SignerDomainV2, P5SigningProvider, UnixSocketP5SigningProvider,
};
use serde::{Deserialize, Serialize};

const MAX_ADMIN_FRAME: u64 = 4_194_304;
const SESSION_CONFIG: &str = "/run/onebrain/p5-v2/current-session.json";
#[cfg(unix)]
const SIGNER_GROUP: &str = "onebrain-p5-sign-client";
const ADMIN_CURSOR: &str = "/var/lib/onebrain/p5-v2/admin-command.cursor";
const CLEANUP_RECEIPT_DIGEST: &str = "/var/lib/onebrain/p5-v2/cleanup-receipt.digest";
const ADMIN_DOMAIN: &[u8] = b"onebrain/p5/signed-admin-frame/v2\0";
#[cfg(unix)]
const RECEIPT_SOCKET: &str = "/run/onebrain/p5-v2/receipt-signer.sock";
const ADMIN_RECEIPT_DOMAIN: &[u8] = b"onebrain/p5/admin-operation-receipt/v2";
const BOOTSTRAP_DOMAIN: &[u8] = b"onebrain/p5/bootstrap-admin-frame/v2\0";

#[derive(Deserialize)]
struct AdminSessionConfig {
    host_id: String,
    controller_application_public_key: String,
    receipt_signer_public_key: String,
    identity_signer_public_key: String,
    request_digest: String,
    inventory_blake3: String,
    evidence_authority: EvidenceAuthorityConfig,
    session_id: String,
    expires_at: u64,
    runner_data_root: String,
    activation_root: String,
    #[serde(default)]
    archive_input: Option<String>,
    #[serde(default)]
    archive_recovery_key: Option<String>,
    #[serde(default)]
    base_dataset_root: Option<String>,
    #[serde(default)]
    previous_generation: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct EvidenceAuthorityConfig {
    inventory_blake3: String,
    public_probe_set_blake3: String,
    topology_attestation_blake3: String,
    provider_evidence_blake3: String,
    provider_evidence_status: String,
    qualification_tier: String,
}

#[derive(Deserialize, Serialize)]
struct AdminFrame {
    format: u64,
    host_id: String,
    session_id: String,
    sequence: u64,
    issued_at: u64,
    expires_at: u64,
    action: String,
    #[serde(default)]
    fault: Option<String>,
    #[serde(default)]
    phase: Option<String>,
    #[serde(default)]
    parameters: serde_json::Value,
    signature: String,
}

#[derive(Serialize)]
struct UnsignedAdminFrame<'a> {
    action: &'a str,
    expires_at: u64,
    fault: &'a Option<String>,
    format: u64,
    host_id: &'a str,
    issued_at: u64,
    parameters: &'a serde_json::Value,
    phase: &'a Option<String>,
    sequence: u64,
    session_id: &'a str,
}

#[derive(Deserialize)]
struct BootstrapFrame {
    base_keyring_hex: String,
    base_policy_hex: String,
    bundle_manifest_hex: String,
    expires_at: u64,
    format: u64,
    host_id: String,
    inventory_hex: String,
    issued_at: u64,
    kind: String,
    operation_id: String,
    p5_approval_policy_hex: String,
    p5_request_hex: String,
    p5_signature_hex: String,
    release_request_hex: String,
    release_signature_hex: String,
    session_config: serde_json::Value,
    signature: String,
}

#[derive(Serialize)]
struct UnsignedBootstrapFrame<'a> {
    base_keyring_hex: &'a str,
    base_policy_hex: &'a str,
    bundle_manifest_hex: &'a str,
    expires_at: u64,
    format: u64,
    host_id: &'a str,
    inventory_hex: &'a str,
    issued_at: u64,
    kind: &'a str,
    operation_id: &'a str,
    p5_approval_policy_hex: &'a str,
    p5_request_hex: &'a str,
    p5_signature_hex: &'a str,
    release_request_hex: &'a str,
    release_signature_hex: &'a str,
    session_config: &'a serde_json::Value,
}

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
    if !Path::new(SESSION_CONFIG).exists() {
        return bootstrap_session(&bytes);
    }
    let config_bytes = fs::read(SESSION_CONFIG)?;
    let config: AdminSessionConfig = serde_json::from_slice(&config_bytes)?;
    validate_config(&config)?;
    let frame = validate_frame(&config, &bytes)?;
    let cursor =
        DurableSequenceCursor::open(ADMIN_CURSOR, *blake3::hash(&config_bytes).as_bytes())?;
    // Commit the signed operation ID/sequence before any host observation or
    // mutation. A repeated forced-command frame therefore rejects after a
    // helper/process restart.
    cursor.advance(frame.sequence)?;
    if frame.action == "finalize-session" {
        return finalize_session(&config, &frame);
    }
    let observation = execute_admin_action(&config, &frame, &bytes)?;
    let unsigned_response = serde_json::json!({
        "accepted": true,
        "action": frame.action,
        "evidence_authority": config.evidence_authority,
        "fault": frame.fault,
        "format": 2,
        "frame_blake3": blake3::hash(&bytes).to_hex().to_string(),
        "host_id": config.host_id,
        "inventory_blake3": config.inventory_blake3,
        "observation": observation,
        "request_digest": config.request_digest,
        "sequence": frame.sequence,
        "session_id": config.session_id,
    });
    let unsigned_bytes = serde_json::to_vec(&unsigned_response)?;
    let receipt_public_key = decode_hex::<32>(&config.receipt_signer_public_key)?;
    let signature = sign_admin_receipt(receipt_public_key, frame.sequence, &unsigned_bytes)?;
    let mut preimage = ADMIN_RECEIPT_DOMAIN.to_vec();
    preimage.extend_from_slice(&unsigned_bytes);
    VerifyingKey::from_bytes(&receipt_public_key)?
        .verify(&preimage, &Signature::from_bytes(&signature))?;
    let response = serde_json::json!({
        "receipt": unsigned_response,
        "signature": signature.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
        "signer_public_key": config.receipt_signer_public_key,
    });
    let response_bytes = serde_json::to_vec(&response)?;
    if frame.action == "cleanup-session" {
        write_private_create_new(
            Path::new(CLEANUP_RECEIPT_DIGEST),
            blake3::hash(&response_bytes).to_hex().as_bytes(),
        )?;
        fs::File::open(Path::new(CLEANUP_RECEIPT_DIGEST).parent().unwrap())?.sync_all()?;
    }
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&response_bytes)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

fn finalize_session(
    config: &AdminSessionConfig,
    frame: &AdminFrame,
) -> Result<(), Box<dyn std::error::Error>> {
    let object = frame
        .parameters
        .as_object()
        .ok_or("finalization parameters must be a closed object")?;
    if object.len() != 1 {
        return Err("finalization parameters are not closed".into());
    }
    let cleanup_receipt_blake3 = object
        .get("cleanup_receipt_blake3")
        .and_then(serde_json::Value::as_str)
        .ok_or("finalization lacks the verified cleanup receipt digest")?;
    decode_hex::<32>(cleanup_receipt_blake3)?;
    let persisted_cleanup = fs::read_to_string(CLEANUP_RECEIPT_DIGEST)?;
    if persisted_cleanup != cleanup_receipt_blake3 {
        return Err("finalization cleanup receipt was not produced by this host".into());
    }
    let observation = P5LinuxAdminBackend::new(ProductionLinuxCommandRunner).finalize_session()?;
    let path = Path::new(SESSION_CONFIG);
    let parent = path.parent().ok_or("session config has no parent")?;
    fs::remove_file(path)?;
    fs::remove_file(CLEANUP_RECEIPT_DIGEST)?;
    fs::File::open(parent)?.sync_all()?;
    let response = serde_json::json!({
        "cleanup_receipt_blake3": cleanup_receipt_blake3,
        "format": 2,
        "host_id": config.host_id,
        "operation": observation,
        "session_config_removed": !path.exists(),
        "signer_stopped": true,
    });
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&serde_json::to_vec(&response)?)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

fn bootstrap_session(bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let root: serde_json::Value = serde_json::from_slice(bytes)?;
    require_exact_keys(
        &root,
        &[
            "base_keyring_hex",
            "base_policy_hex",
            "bundle_manifest_hex",
            "expires_at",
            "format",
            "host_id",
            "inventory_hex",
            "issued_at",
            "kind",
            "operation_id",
            "p5_approval_policy_hex",
            "p5_request_hex",
            "p5_signature_hex",
            "release_request_hex",
            "release_signature_hex",
            "session_config",
            "signature",
        ],
    )?;
    let frame: BootstrapFrame = serde_json::from_value(root)?;
    let now = unix_now()?;
    if frame.format != 2
        || frame.kind != "bootstrap"
        || frame.host_id.is_empty()
        || frame.host_id.len() > 128
        || decode_hex::<32>(&frame.operation_id).is_err()
        || frame.issued_at > now.saturating_add(30)
        || frame.expires_at.saturating_add(30) < now
        || frame.expires_at > now.saturating_add(300)
    {
        return Err("bootstrap authority/freshness mismatch".into());
    }
    let release_request = decode_bounded_hex(&frame.release_request_hex, 262_144)?;
    let release_signature = decode_bounded_hex(&frame.release_signature_hex, 16_384)?;
    let base_policy = decode_bounded_hex(&frame.base_policy_hex, 65_536)?;
    let base_keyring = decode_bounded_hex(&frame.base_keyring_hex, 131_072)?;
    let p5_request_bytes = decode_bounded_hex(&frame.p5_request_hex, 262_144)?;
    let p5_signature = decode_hex::<64>(&frame.p5_signature_hex)?;
    let p5_policy_bytes = decode_bounded_hex(&frame.p5_approval_policy_hex, 65_536)?;
    let inventory_bytes = decode_bounded_hex(&frame.inventory_hex, 262_144)?;
    let bundle_manifest = decode_bounded_hex(&frame.bundle_manifest_hex, 262_144)?;

    let p5_request = canonical_json_object(&p5_request_bytes, "P5 request")?;
    let p5_policy = canonical_json_object(&p5_policy_bytes, "P5 approval policy")?;
    let inventory = canonical_json_object(&inventory_bytes, "P5 inventory")?;
    let session_bytes = serde_json::to_vec(&frame.session_config)?;
    let config: AdminSessionConfig = serde_json::from_slice(&session_bytes)?;
    validate_config(&config)?;
    if config.host_id != frame.host_id || frame.expires_at > config.expires_at {
        return Err("bootstrap session host/validity mismatch".into());
    }

    let controller_hex = json_string(&inventory, "controller_application_public_key")?;
    if controller_hex != config.controller_application_public_key {
        return Err("bootstrap controller is not inventory-bound".into());
    }
    let controller_public = decode_hex::<32>(controller_hex)?;
    let unsigned = UnsignedBootstrapFrame {
        base_keyring_hex: &frame.base_keyring_hex,
        base_policy_hex: &frame.base_policy_hex,
        bundle_manifest_hex: &frame.bundle_manifest_hex,
        expires_at: frame.expires_at,
        format: frame.format,
        host_id: &frame.host_id,
        inventory_hex: &frame.inventory_hex,
        issued_at: frame.issued_at,
        kind: &frame.kind,
        operation_id: &frame.operation_id,
        p5_approval_policy_hex: &frame.p5_approval_policy_hex,
        p5_request_hex: &frame.p5_request_hex,
        p5_signature_hex: &frame.p5_signature_hex,
        release_request_hex: &frame.release_request_hex,
        release_signature_hex: &frame.release_signature_hex,
        session_config: &frame.session_config,
    };
    let mut bootstrap_preimage = BOOTSTRAP_DOMAIN.to_vec();
    bootstrap_preimage.extend_from_slice(&serde_json::to_vec(&unsigned)?);
    VerifyingKey::from_bytes(&controller_public)?.verify(
        &bootstrap_preimage,
        &Signature::from_bytes(&decode_hex::<64>(&frame.signature)?),
    )?;

    let policy_public = decode_hex::<32>(json_string(&p5_policy, "public_key")?)?;
    if json_u64(&p5_policy, "format")? != 2
        || json_string(&p5_policy, "role")? != "p5-run-approver"
        || json_string(&p5_policy, "signing_domain")? != "onebrain/p5/run-request/v2"
        || frame.issued_at < json_u64(&p5_policy, "valid_from")?
        || frame.expires_at > json_u64(&p5_policy, "valid_until")?
    {
        return Err("P5 approval policy does not authorize bootstrap".into());
    }
    let canonical_request = serde_json::to_vec(&p5_request)?;
    let mut request_preimage = b"onebrain/p5/run-request/v2\0".to_vec();
    request_preimage.extend_from_slice(blake3::hash(&canonical_request).as_bytes());
    VerifyingKey::from_bytes(&policy_public)?
        .verify(&request_preimage, &Signature::from_bytes(&p5_signature))?;
    if json_string(&p5_request, "inventory_blake3")?
        != blake3::hash(&inventory_bytes).to_hex().as_str()
        || json_string(&p5_request, "session_id")? != config.session_id
        || json_u64(&p5_request, "expires_at")? != config.expires_at
        || json_string(&p5_request, "qualification_tier")? != "production-reference"
    {
        return Err("P5 request/session/inventory binding mismatch".into());
    }

    validate_inventory_host(&inventory, &config)?;
    validate_config_digests(
        &frame.session_config,
        &release_request,
        &release_signature,
        &base_policy,
        &canonical_request,
        &p5_signature,
        &serde_json::to_vec(&p5_policy)?,
        &inventory_bytes,
        &bundle_manifest,
    )?;
    verify_installed_bundle(&frame.session_config, &bundle_manifest)?;
    verify_base_release_signature(
        &frame.operation_id,
        &release_request,
        &release_signature,
        &base_policy,
        &base_keyring,
    )?;
    install_session_config(&session_bytes)?;
    let response = serde_json::json!({
        "format": 2,
        "host_id": frame.host_id,
        "installed_config_blake3": blake3::hash(&session_bytes).to_hex().to_string(),
        "network_changed": false,
        "operation_id": frame.operation_id,
        "units_changed": false,
    });
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&serde_json::to_vec(&response)?)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

fn validate_inventory_host(
    inventory: &serde_json::Value,
    config: &AdminSessionConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let hosts = inventory
        .get("hosts")
        .and_then(serde_json::Value::as_array)
        .ok_or("inventory hosts are missing")?;
    let matches = hosts
        .iter()
        .filter(|row| {
            row.get("host_id")
                .or_else(|| row.get("physical_host_id"))
                .and_then(serde_json::Value::as_str)
                == Some(config.host_id.as_str())
        })
        .collect::<Vec<_>>();
    if matches.len() != 1
        || matches[0]
            .get("identity_public_key")
            .and_then(serde_json::Value::as_str)
            != Some(config.identity_signer_public_key.as_str())
        || matches[0]
            .get("receipt_public_key")
            .and_then(serde_json::Value::as_str)
            != Some(config.receipt_signer_public_key.as_str())
    {
        return Err("session signer identity is not inventory-bound".into());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_config_digests(
    config: &serde_json::Value,
    release_request: &[u8],
    release_signature: &[u8],
    base_policy: &[u8],
    p5_request: &[u8],
    p5_signature: &[u8],
    p5_policy: &[u8],
    inventory: &[u8],
    bundle_manifest: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    for (field, bytes) in [
        ("release_request_blake3", release_request),
        ("release_signature_blake3", release_signature),
        ("base_release_policy_blake3", base_policy),
        ("p5_request_blake3", p5_request),
        ("p5_signature_blake3", p5_signature),
        ("p5_approval_policy_blake3", p5_policy),
        ("inventory_blake3", inventory),
        ("bundle_manifest_blake3", bundle_manifest),
    ] {
        if json_string(config, field)? != blake3::hash(bytes).to_hex().as_str() {
            return Err(format!("session config {field} mismatch").into());
        }
    }
    Ok(())
}

fn verify_installed_bundle(
    config: &serde_json::Value,
    embedded_manifest: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let executable = fs::canonicalize("/proc/self/exe")?;
    let generation = executable
        .parent()
        .and_then(Path::parent)
        .ok_or("admin executable is not inside an immutable generation")?;
    let manifest_path = generation.join("metadata/bundle.manifest.json");
    let installed = fs::read(&manifest_path)?;
    if installed != embedded_manifest {
        return Err("embedded bundle manifest differs from installed generation".into());
    }
    let manifest: serde_json::Value = serde_json::from_slice(&installed)?;
    let candidate = manifest
        .get("candidate")
        .ok_or("bundle candidate is missing")?;
    if json_string(config, "candidate_commit")? != json_string(candidate, "id")?
        || json_string(config, "candidate_tree")? != json_string(candidate, "version")?
    {
        return Err("session candidate differs from installed generation".into());
    }
    Ok(())
}

fn verify_base_release_signature(
    operation_id: &str,
    request: &[u8],
    signature: &[u8],
    policy_bytes: &[u8],
    keyring: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let policy: serde_json::Value = serde_json::from_slice(policy_bytes)?;
    let fingerprint = base_policy_fingerprint(&policy)?;
    let root = PathBuf::from(format!("/run/onebrain/p5-v2/bootstrap-{operation_id}"));
    fs::create_dir_all(&root)?;
    set_mode(&root, 0o700)?;
    let request_path = root.join("release-request.json");
    let signature_path = root.join("release-request.sig");
    let keyring_path = root.join("release-keyring.gpg");
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        write_private_create_new(&request_path, request)?;
        write_private_create_new(&signature_path, signature)?;
        write_private_create_new(&keyring_path, keyring)?;
        let output = Command::new("/usr/bin/gpgv")
            .env_clear()
            .env("LC_ALL", "C")
            .args(["--status-fd", "1", "--keyring"])
            .arg(&keyring_path)
            .arg(&signature_path)
            .arg(&request_path)
            .output()?;
        if !output.status.success() {
            return Err("base release OpenPGP verification failed".into());
        }
        let status = String::from_utf8(output.stdout)?;
        let valid = status.lines().any(|line| {
            let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
            fields.first() == Some(&"[GNUPG:]")
                && fields.get(1) == Some(&"VALIDSIG")
                && (fields.get(2) == Some(&fingerprint.as_str())
                    || fields.last() == Some(&fingerprint.as_str()))
        });
        if !valid {
            return Err("base release signer is outside the explicit policy".into());
        }
        Ok(())
    })();
    let _ = fs::remove_file(&request_path);
    let _ = fs::remove_file(&signature_path);
    let _ = fs::remove_file(&keyring_path);
    let _ = fs::remove_dir(&root);
    result
}

fn base_policy_fingerprint(
    value: &serde_json::Value,
) -> Result<String, Box<dyn std::error::Error>> {
    let policy = if value.get("format").and_then(serde_json::Value::as_str)
        == Some("onebrain/base-v1-release-signers/1")
    {
        let rows = value
            .get("policies")
            .and_then(serde_json::Value::as_array)
            .ok_or("base signer vector has no policies")?;
        let matches = rows
            .iter()
            .filter_map(|row| row.get("policy"))
            .filter(|row| {
                row.get("role").and_then(serde_json::Value::as_str)
                    == Some("qualification-approver")
            })
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err("base signer vector qualification policy is ambiguous".into());
        }
        matches[0]
    } else if value.get("policy").is_some() {
        value.get("policy").unwrap()
    } else {
        value
    };
    if json_string(policy, "format")? != "onebrain/base-v1-qualification-approver-policy/1"
        || json_string(policy, "algorithm")? != "OpenPGP-Ed25519"
        || json_string(policy, "role")? != "qualification-approver"
    {
        return Err("base release policy is not the qualification policy".into());
    }
    let signers = policy
        .get("signers")
        .and_then(serde_json::Value::as_array)
        .ok_or("base release policy has no signers")?;
    if signers.len() != 1 {
        return Err("base release policy signer is ambiguous".into());
    }
    let fingerprint = json_string(&signers[0], "fingerprint")?.to_owned();
    if fingerprint.len() != 40
        || !fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'A'..=b'F').contains(&byte))
    {
        return Err("base release fingerprint is invalid".into());
    }
    Ok(fingerprint)
}

fn install_session_config(bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(SESSION_CONFIG);
    #[cfg(unix)]
    let signer_gid = Some(lookup_group_gid(SIGNER_GROUP)?);
    #[cfg(not(unix))]
    let signer_gid = None;
    install_session_config_at(path, bytes, signer_gid)
}

fn install_session_config_at(
    path: &Path,
    bytes: &[u8],
    signer_gid: Option<u32>,
) -> Result<(), Box<dyn std::error::Error>> {
    let parent = path.parent().ok_or("session config has no parent")?;
    fs::create_dir_all(parent)?;
    // The admin boundary owns and writes the session, while the two dedicated
    // unprivileged signer services must read it to bind their durable cursors.
    // Grant only the closed signer-client group traversal/read access.
    if let Some(gid) = signer_gid {
        set_group_id(parent, gid)?;
    }
    set_mode(parent, 0o750)?;
    write_private_create_new(path, bytes)?;
    if let Some(gid) = signer_gid {
        set_group_id(path, gid)?;
    }
    set_mode(path, 0o640)?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

fn write_private_create_new(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn set_mode(_path: &Path, mode: u32) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(_path, fs::Permissions::from_mode(mode))?;
    }
    let _ = mode;
    Ok(())
}

#[cfg(unix)]
fn lookup_group_gid(group: &str) -> Result<u32, Box<dyn std::error::Error>> {
    let group = CString::new(group)?;
    let entry = unsafe { libc::getgrnam(group.as_ptr()) };
    if entry.is_null() {
        return Err("P5 signer client group is unavailable".into());
    }
    Ok(unsafe { (*entry).gr_gid })
}

#[cfg(unix)]
fn set_group_id(path: &Path, gid: u32) -> Result<(), Box<dyn std::error::Error>> {
    let encoded_path = CString::new(path.as_os_str().as_bytes())?;
    let result = unsafe { libc::chown(encoded_path.as_ptr(), !0, gid) };
    if result != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_group_id(_path: &Path, _gid: u32) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

fn canonical_json_object(
    bytes: &[u8],
    label: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    if !value.is_object() || serde_json::to_vec(&value)? != bytes {
        return Err(format!("{label} is not canonical JSON").into());
    }
    Ok(value)
}

fn require_exact_keys(
    value: &serde_json::Value,
    keys: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let object = value.as_object().ok_or("frame must be an object")?;
    if object.len() != keys.len() || keys.iter().any(|key| !object.contains_key(*key)) {
        return Err("frame schema is not closed".into());
    }
    Ok(())
}

fn json_string<'a>(
    value: &'a serde_json::Value,
    field: &str,
) -> Result<&'a str, Box<dyn std::error::Error>> {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("{field} is missing or not a string").into())
}

fn json_u64(value: &serde_json::Value, field: &str) -> Result<u64, Box<dyn std::error::Error>> {
    value
        .get(field)
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| format!("{field} is missing or not unsigned").into())
}

fn decode_bounded_hex(value: &str, maximum: usize) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if value.is_empty()
        || value.len() > maximum.saturating_mul(2)
        || value.len() % 2 != 0
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("bounded canonical hex is invalid".into());
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).map_err(Into::into))
        .collect()
}

#[cfg(unix)]
fn sign_admin_receipt(
    receipt_public_key: [u8; 32],
    sequence: u64,
    unsigned_bytes: &[u8],
) -> Result<[u8; 64], Box<dyn std::error::Error>> {
    let signer = UnixSocketP5SigningProvider::new(
        RECEIPT_SOCKET,
        receipt_public_key,
        std::time::Duration::from_secs(3),
    )?;
    Ok(signer.sign(
        P5SignerDomainV2::AdminOperation,
        sequence,
        unsigned_bytes,
        None,
    )?)
}

#[cfg(not(unix))]
fn sign_admin_receipt(
    _receipt_public_key: [u8; 32],
    _sequence: u64,
    _unsigned_bytes: &[u8],
) -> Result<[u8; 64], Box<dyn std::error::Error>> {
    Err("P5 admin receipt signing requires Unix".into())
}

fn validate_config(config: &AdminSessionConfig) -> Result<(), Box<dyn std::error::Error>> {
    if config.host_id.is_empty()
        || config.host_id.len() > 128
        || decode_hex::<32>(&config.controller_application_public_key).is_err()
        || decode_hex::<32>(&config.receipt_signer_public_key).is_err()
        || decode_hex::<32>(&config.identity_signer_public_key).is_err()
        || decode_hex::<32>(&config.request_digest).is_err()
        || decode_hex::<32>(&config.inventory_blake3).is_err()
        || config.inventory_blake3 != config.evidence_authority.inventory_blake3
        || decode_hex::<32>(&config.evidence_authority.inventory_blake3).is_err()
        || decode_hex::<32>(&config.evidence_authority.public_probe_set_blake3).is_err()
        || decode_hex::<32>(&config.evidence_authority.topology_attestation_blake3).is_err()
        || decode_hex::<32>(&config.evidence_authority.provider_evidence_blake3).is_err()
        || config
            .evidence_authority
            .provider_evidence_status
            .is_empty()
        || config.evidence_authority.qualification_tier != "production-reference"
        || decode_hex::<32>(&config.session_id).is_err()
        || config.expires_at < unix_now()?
        || config.runner_data_root != format!("/var/lib/onebrain/p5-v2/{}", config.host_id)
        || config.activation_root != "/opt/onebrain/base-v1"
    {
        return Err("invalid P5 admin session authority".into());
    }
    Ok(())
}

fn validate_frame(
    config: &AdminSessionConfig,
    bytes: &[u8],
) -> Result<AdminFrame, Box<dyn std::error::Error>> {
    let frame: AdminFrame = serde_json::from_slice(bytes)?;
    let now = unix_now()?;
    if frame.format != 2
        || frame.host_id != config.host_id
        || frame.session_id != config.session_id
        || frame.sequence == 0
        || frame.issued_at > now.saturating_add(30)
        || frame.expires_at.saturating_add(30) < now
        || frame.expires_at > config.expires_at
    {
        return Err("admin authority/freshness mismatch".into());
    }
    let unsigned = UnsignedAdminFrame {
        action: &frame.action,
        expires_at: frame.expires_at,
        fault: &frame.fault,
        format: frame.format,
        host_id: &frame.host_id,
        issued_at: frame.issued_at,
        parameters: &frame.parameters,
        phase: &frame.phase,
        sequence: frame.sequence,
        session_id: &frame.session_id,
    };
    let mut preimage = ADMIN_DOMAIN.to_vec();
    preimage.extend_from_slice(&serde_json::to_vec(&unsigned)?);
    VerifyingKey::from_bytes(&decode_hex::<32>(
        &config.controller_application_public_key,
    )?)?
    .verify(
        &preimage,
        &Signature::from_bytes(&decode_hex::<64>(&frame.signature)?),
    )?;
    validate_action_phase(&frame)?;
    Ok(frame)
}

fn execute_admin_action(
    config: &AdminSessionConfig,
    frame: &AdminFrame,
    frame_bytes: &[u8],
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let root_free = fs2::available_space(Path::new("/"))?;
    let namespace_present = Path::new("/run/netns/onebrain-p5-v2").exists();
    let host_observation = serde_json::json!({
        "namespace_present": namespace_present,
        "root_filesystem_free_bytes": root_free,
        "session_config_blake3": blake3::hash(&fs::read(SESSION_CONFIG)?).to_hex().to_string(),
    });
    match frame.action.as_str() {
        "observe" => {
            let operation = P5LinuxAdminBackend::new(ProductionLinuxCommandRunner).observe()?;
            Ok(serde_json::json!({"host": host_observation, "operation": operation}))
        }
        "prepare-session" => {
            if !frame.parameters.is_object() {
                return Err("lifecycle parameters must be a closed object".into());
            }
            let previous_generation = verify_previous_generation(
                Path::new(&config.activation_root),
                Path::new(
                    config
                        .previous_generation
                        .as_deref()
                        .ok_or("missing previous signed generation")?,
                ),
            )
            .map_err(|error| format!("previous generation verification failed: {error:?}"))?;
            let fixture = prepare_obarv002_fixture(
                Path::new(&config.runner_data_root),
                Path::new(
                    config
                        .archive_input
                        .as_deref()
                        .ok_or("missing archive fixture path")?,
                ),
                Path::new(
                    config
                        .archive_recovery_key
                        .as_deref()
                        .ok_or("missing archive key path")?,
                ),
                Path::new(
                    config
                        .base_dataset_root
                        .as_deref()
                        .ok_or("missing archive dataset path")?,
                ),
                decode_hex::<32>(&config.identity_signer_public_key)?,
            )
            .map_err(|error| format!("recovery fixture preparation failed: {error:?}"))?;
            let network = P5LinuxAdminBackend::new(ProductionLinuxCommandRunner);
            let operation = network.prepare_session()?;
            let disk = match P5DiskPressureBackend::new(ProductionLinuxCommandRunner).prepare() {
                Ok(value) => value,
                Err(error) => {
                    let rollback = network.cleanup_session();
                    return Err(format!(
                        "disk-pressure preallocation failed: {error}; network rollback={rollback:?}"
                    )
                    .into());
                }
            };
            Ok(serde_json::json!({
                "disk_pressure": disk,
                "host": host_observation,
                "operation": operation,
                "recovery_fixture": {
                    "archive_blake3": hex(&fixture.archive_blake3),
                        "archive_bytes": fixture.archive_bytes,
                        "dataset_generation": hex(&fixture.dataset_generation),
                        "previous_generation": previous_generation,
                }
            }))
        }
        "cleanup-session" => {
            if !frame.parameters.is_object() {
                return Err("lifecycle parameters must be a closed object".into());
            }
            let disk = P5DiskPressureBackend::new(ProductionLinuxCommandRunner).cleanup()?;
            let operation =
                P5LinuxAdminBackend::new(ProductionLinuxCommandRunner).cleanup_session()?;
            Ok(
                serde_json::json!({"disk_pressure": disk, "host": host_observation, "operation": operation}),
            )
        }
        "apply" => {
            let fault = parse_fault(frame.fault.as_deref().ok_or("missing typed fault")?)?;
            if fault == P5FaultKindV2::DiskPressure {
                let operation = P5DiskPressureBackend::new(ProductionLinuxCommandRunner).apply()?;
                return Ok(serde_json::json!({"host": host_observation, "operation": operation}));
            }
            if matches!(
                fault,
                P5FaultKindV2::BaseObarv002ArchiveRestore
                    | P5FaultKindV2::Rollback
                    | P5FaultKindV2::ExplicitReEnable
            ) {
                return execute_recovery_action(config, frame, frame_bytes, fault);
            }
            let endpoints = peer_endpoints(&frame.parameters)?;
            let operation =
                P5LinuxAdminBackend::new(ProductionLinuxCommandRunner).apply(fault, &endpoints)?;
            Ok(serde_json::json!({"host": host_observation, "operation": operation}))
        }
        "clear" => {
            let fault = parse_fault(frame.fault.as_deref().ok_or("missing typed fault")?)?;
            let backend = P5LinuxAdminBackend::new(ProductionLinuxCommandRunner);
            let operation = if fault == P5FaultKindV2::DiskPressure {
                P5DiskPressureBackend::new(ProductionLinuxCommandRunner).clear()?
            } else if matches!(
                fault,
                P5FaultKindV2::BaseObarv002ArchiveRestore
                    | P5FaultKindV2::Rollback
                    | P5FaultKindV2::ExplicitReEnable
            ) {
                backend.observe()?
            } else {
                backend.clear(fault)?
            };
            Ok(serde_json::json!({"host": host_observation, "operation": operation}))
        }
        _ => Err("admin action is outside the P5 V2 allowlist".into()),
    }
}

fn execute_recovery_action(
    config: &AdminSessionConfig,
    frame: &AdminFrame,
    frame_bytes: &[u8],
    fault: P5FaultKindV2,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    if frame
        .parameters
        .as_object()
        .is_none_or(|value| !value.is_empty())
    {
        return Err("recovery action requires the frozen empty parameters object".into());
    }
    let operation = match fault {
        P5FaultKindV2::BaseObarv002ArchiveRestore => P5RecoveryOperationV2::Obarv002Restore,
        P5FaultKindV2::Rollback => P5RecoveryOperationV2::Rollback,
        P5FaultKindV2::ExplicitReEnable => P5RecoveryOperationV2::ExplicitReEnable,
        _ => return Err("fault is not a typed recovery operation".into()),
    };
    let operation_id = *blake3::hash(frame_bytes).as_bytes();
    let runner_root = PathBuf::from(&config.runner_data_root);
    let evidence_root = runner_root.join("recovery-evidence");
    fs::create_dir_all(&evidence_root)?;
    let evidence_output = evidence_root.join(format!("{}.receipt", hex(&operation_id)));
    let (archive_input, archive_recovery_key, base_dataset_root, previous_generation) =
        match operation {
            P5RecoveryOperationV2::Obarv002Restore => (
                config.archive_input.as_ref().map(PathBuf::from),
                config.archive_recovery_key.as_ref().map(PathBuf::from),
                config.base_dataset_root.as_ref().map(PathBuf::from),
                None,
            ),
            P5RecoveryOperationV2::Rollback => (
                None,
                None,
                None,
                config.previous_generation.as_ref().map(PathBuf::from),
            ),
            P5RecoveryOperationV2::ExplicitReEnable => (None, None, None, None),
        };
    let input = P5RecoveryInputsV2 {
        request_digest: decode_hex::<32>(&config.request_digest)?,
        session_id: decode_hex::<32>(&config.session_id)?,
        host_id: config.host_id.clone(),
        operation_id,
        identity_public_key: decode_hex::<32>(&config.identity_signer_public_key)?,
        runner_data_root: runner_root,
        activation_root: PathBuf::from(&config.activation_root),
        evidence_output,
        archive_input,
        archive_recovery_key,
        base_dataset_root,
        previous_generation,
    };
    let verified = verify_inputs(operation, &input)
        .map_err(|error| format!("recovery input verification failed: {error:?}"))?;
    let backend = P5LinuxAdminBackend::new(ProductionLinuxCommandRunner);
    let quiesced = backend.quiesce_agent_for_recovery()?;
    let result = match operation {
        P5RecoveryOperationV2::Obarv002Restore => obarv002_restore(verified),
        P5RecoveryOperationV2::Rollback => rollback(verified),
        P5RecoveryOperationV2::ExplicitReEnable => explicit_re_enable(verified),
    };
    let resumed = backend.resume_agent_after_recovery();
    let receipt = result.map_err(|error| format!("typed recovery failed: {error:?}"))?;
    let resumed = resumed?;
    Ok(serde_json::json!({
        "operation": {
            "evidence_blake3": hex(&receipt.evidence_blake3),
            "operation": match operation {
                P5RecoveryOperationV2::Obarv002Restore => "obarv002-restore",
                P5RecoveryOperationV2::Rollback => "rollback",
                P5RecoveryOperationV2::ExplicitReEnable => "explicit-re-enable",
            },
            "operation_id": hex(&receipt.operation_id),
            "state_changed": receipt.state_changed,
        },
        "quiesce": quiesced,
        "resume": resumed,
    }))
}

fn validate_action_phase(frame: &AdminFrame) -> Result<(), Box<dyn std::error::Error>> {
    let valid = match frame.action.as_str() {
        "prepare-session" | "cleanup-session" | "finalize-session" => {
            frame.fault.is_none() && frame.phase.is_none()
        }
        "observe" => frame.fault.is_some() && frame.phase.as_deref() == Some("before"),
        "apply" => frame.fault.is_some() && frame.phase.as_deref() == Some("during"),
        "clear" => frame.fault.is_some() && frame.phase.as_deref() == Some("after"),
        _ => false,
    };
    if !valid {
        return Err("admin action/fault/phase combination is invalid".into());
    }
    Ok(())
}

fn parse_fault(value: &str) -> Result<P5FaultKindV2, Box<dyn std::error::Error>> {
    Ok(match value {
        "partition" => P5FaultKindV2::Partition,
        "drop" => P5FaultKindV2::Drop,
        "reorder" => P5FaultKindV2::Reorder,
        "duplicate" => P5FaultKindV2::Duplicate,
        "restart" => P5FaultKindV2::Restart,
        "address-change" => P5FaultKindV2::AddressChange,
        "seed-outage" => P5FaultKindV2::SeedOutage,
        "signer-outage" => P5FaultKindV2::SignerOutage,
        "disk-pressure" => P5FaultKindV2::DiskPressure,
        "slow-peer" => P5FaultKindV2::SlowPeer,
        "base-obarv002-archive-restore" => P5FaultKindV2::BaseObarv002ArchiveRestore,
        "rollback" => P5FaultKindV2::Rollback,
        "explicit-re-enable" => P5FaultKindV2::ExplicitReEnable,
        "selected-relay-shutdown" => P5FaultKindV2::SelectedRelayShutdown,
        _ => return Err("fault is outside the P5 V2 allowlist".into()),
    })
}

fn peer_endpoints(
    parameters: &serde_json::Value,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let object = parameters
        .as_object()
        .ok_or("fault parameters must be a closed object")?;
    if object.keys().any(|key| key != "peer_endpoints") {
        return Err("unknown fault parameter".into());
    }
    let Some(values) = object.get("peer_endpoints") else {
        return Ok(Vec::new());
    };
    let values = values.as_array().ok_or("peer_endpoints must be an array")?;
    if values.len() > 8 {
        return Err("peer endpoint set exceeds the closed bound".into());
    }
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty() && value.len() <= 128)
                .map(str::to_owned)
                .ok_or_else(|| "invalid peer endpoint".into())
        })
        .collect()
}

fn unix_now() -> Result<u64, Box<dyn std::error::Error>> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

fn decode_hex<const N: usize>(value: &str) -> Result<[u8; N], Box<dyn std::error::Error>> {
    if value.len() != N * 2 {
        return Err("invalid lowercase hex length".into());
    }
    let mut output = [0u8; N];
    for (index, slot) in output.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)?;
    }
    if output
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
        != value
    {
        return Err("noncanonical hex".into());
    }
    Ok(output)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    #[test]
    fn session_config_is_readable_only_by_root_and_the_signer_group() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("p5-v2/current-session.json");
        let gid = unsafe { libc::getegid() };

        install_session_config_at(&path, br#"{"format":2}"#, Some(gid)).unwrap();

        let parent = path.parent().unwrap().metadata().unwrap();
        let session = path.metadata().unwrap();
        assert_eq!(parent.permissions().mode() & 0o777, 0o750);
        assert_eq!(session.permissions().mode() & 0o777, 0o640);
        assert_eq!(parent.gid(), gid);
        assert_eq!(session.gid(), gid);
        assert_eq!(fs::read(path).unwrap(), br#"{"format":2}"#);
    }
}
