//! Closed bridge to the owner-approved Base release-request verifier.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

/// Invoke only the candidate-owned verifier with the fixed Linux interpreter.
pub fn verify_base_release_request(
    request: &Path,
    signature: &Path,
    approver_policy: &Path,
    gpg_home: &Path,
) -> Result<Value, String> {
    if !cfg!(target_os = "linux") {
        return Err("production release-request verification requires Linux".to_owned());
    }
    let python = Path::new("/usr/bin/python3");
    if !python.is_file() {
        return Err("fixed production Python interpreter is unavailable".to_owned());
    }
    let verifier = candidate_verifier_path();
    if !verifier.is_file() {
        return Err("candidate-owned release-request verifier is unavailable".to_owned());
    }
    let (request_value, request_digest) = authenticate_production_request(
        request,
        signature,
        approver_policy,
        gpg_home,
        python,
        &verifier,
    )?;
    let result = Command::new(python)
        .arg(&verifier)
        .arg("--request")
        .arg(request)
        .arg("--signature")
        .arg(signature)
        .arg("--policy")
        .arg(approver_policy)
        .arg("--gpg-home")
        .arg(gpg_home)
        .output()
        .map_err(|error| format!("fixed release-request verifier failed to start: {error}"))?;
    if !result.status.success() {
        return Err(format!(
            "signed release request verification failed: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    let verified: Value = serde_json::from_slice(&result.stdout)
        .map_err(|error| format!("release-request verifier output is invalid: {error}"))?;
    validate_python_context(&request_value, &request_digest, &verified, true)?;
    Ok(verified)
}

/// Test-only bridge for exercising the signed boundary on non-Linux hosts.
/// The Python verifier is required to return a context that cannot claim production.
pub fn verify_base_release_request_for_test_nonproduction(
    python: &Path,
    gpg: &Path,
    request: &Path,
    signature: &Path,
    approver_policy: &Path,
    gpg_home: &Path,
) -> Result<Value, String> {
    let verifier = candidate_verifier_path();
    let result = Command::new(python)
        .arg(&verifier)
        .arg("--request")
        .arg(request)
        .arg("--signature")
        .arg(signature)
        .arg("--policy")
        .arg(approver_policy)
        .arg("--gpg-home")
        .arg(gpg_home)
        .arg("--test-nonproduction-gpg")
        .arg(gpg)
        .output()
        .map_err(|error| format!("test-only release-request verifier failed to start: {error}"))?;
    if !result.status.success() {
        return Err(format!(
            "signed release request verification failed: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    let verified: Value = serde_json::from_slice(&result.stdout)
        .map_err(|error| format!("release-request verifier output is invalid: {error}"))?;
    let (request_value, request_digest) = read_canonical_request(request)?;
    let mut expected_fields: BTreeSet<&str> = [
        "format",
        "usage",
        "qualification_session_id",
        "candidate",
        "qualification_approver_fingerprint",
        "trust_policy_digest",
        "required_targets",
        "production_profile_blake3",
        "production_vector_blake3",
        "append_only_idl_history_root",
        "created_utc",
        "expires_utc",
        "evidence_root_uri",
        "candidate_tooling_blake3",
    ]
    .into_iter()
    .collect();
    if request_value.get("format").and_then(Value::as_str)
        == Some("onebrain/base-v1-release-request/1")
    {
        expected_fields.extend(["registry_candidate", "reference_environment"]);
    }
    let actual_fields: BTreeSet<&str> = request_value
        .as_object()
        .ok_or("authenticated release request is not an object")?
        .keys()
        .map(String::as_str)
        .collect();
    if actual_fields != expected_fields {
        return Err("authenticated release request fields are not closed".to_owned());
    }
    validate_python_context(&request_value, &request_digest, &verified, false)?;
    Ok(verified)
}

fn candidate_verifier_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("scripts/release/verify_base_release_request.py")
}

fn read_canonical_request(path: &Path) -> Result<(Value, String), String> {
    let bytes =
        fs::read(path).map_err(|error| format!("release request could not be read: {error}"))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("release request JSON is invalid: {error}"))?;
    let canonical = serde_json::to_vec(&value)
        .map_err(|error| format!("release request could not be canonicalized: {error}"))?;
    if canonical != bytes {
        return Err("release request bytes are not canonical".to_owned());
    }
    Ok((value, blake3::hash(&bytes).to_hex().to_string()))
}

const APPROVER_FINGERPRINT: &str = "A9BFDC59364354F954ABD26947FCF15DD9C32781";
const APPROVER_POLICY_DIGEST: &str =
    "0710845f71ca7aca7ce89a0377a31ce293c6dd99778c0ff3decf9e04745528be";
const APPROVER_POLICY_CONTEXT: &str = "onebrain:base-v1:qualification-approver-policy:1";
const APPROVER_PACKET_BLAKE3: &str =
    "228d43ea4f3cc0b7548124682e353544ae9549e5458456016242cfa738a5575e";
const FROZEN_POLICY_JSON: &str = "{\"algorithm\":\"OpenPGP-Ed25519\",\"allowed_usages\":[\"base-release-request\"],\"format\":\"onebrain/base-v1-qualification-approver-policy/1\",\"role\":\"qualification-approver\",\"signers\":[{\"created_utc\":\"2026-08-27T04:49:51Z\",\"expires_utc\":\"2028-08-26T04:49:51Z\",\"fingerprint\":\"A9BFDC59364354F954ABD26947FCF15DD9C32781\",\"key_id\":\"47FCF15DD9C32781\",\"public_key_packet_blake3\":\"228d43ea4f3cc0b7548124682e353544ae9549e5458456016242cfa738a5575e\"}],\"valid_unlisted_signature\":\"reject\",\"verification\":{\"fingerprint_source\":\"gpg-status-fd-VALIDSIG-full-primary-fingerprint\",\"trust_model\":\"explicit-allowlist\"}}";

fn file_blake3(path: &Path) -> Result<String, String> {
    fs::read(path)
        .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
        .map_err(|error| format!("qualification tooling could not be measured: {error}"))
}

fn authenticate_production_request(
    request: &Path,
    signature: &Path,
    policy: &Path,
    gpg_home: &Path,
    python: &Path,
    verifier: &Path,
) -> Result<(Value, String), String> {
    let gpg = Path::new("/usr/bin/gpg");
    if !gpg.is_file() {
        return Err("fixed production GPG executable is unavailable".to_owned());
    }
    let policy_bytes =
        fs::read(policy).map_err(|error| format!("approver policy could not be read: {error}"))?;
    let policy_value: Value = serde_json::from_slice(&policy_bytes)
        .map_err(|error| format!("approver policy JSON is invalid: {error}"))?;
    let frozen_policy = if policy_value.get("format").and_then(Value::as_str)
        == Some("onebrain/base-v1-release-signers/1")
    {
        let rows = policy_value
            .get("policies")
            .and_then(Value::as_array)
            .ok_or("approver signer vector policies are missing")?;
        let matches: Vec<&Value> = rows
            .iter()
            .filter_map(|row| row.get("policy"))
            .filter(|candidate| {
                candidate.get("role").and_then(Value::as_str) == Some("qualification-approver")
            })
            .collect();
        if matches.len() != 1 {
            return Err("approver signer vector role binding is not closed".to_owned());
        }
        matches[0].clone()
    } else {
        policy_value.clone()
    };
    let frozen_policy_bytes = serde_json::to_vec(&frozen_policy)
        .map_err(|error| format!("approver policy could not be canonicalized: {error}"))?;
    if frozen_policy_bytes != FROZEN_POLICY_JSON.as_bytes()
        || blake3::derive_key(APPROVER_POLICY_CONTEXT, &frozen_policy_bytes).as_slice()
            != hex32(APPROVER_POLICY_DIGEST)?.as_slice()
    {
        return Err("production qualification approver policy is not frozen".to_owned());
    }
    let (request_value, request_digest) = read_canonical_request(request)?;
    let tooling = request_value
        .get("candidate_tooling_blake3")
        .and_then(Value::as_object)
        .ok_or("authenticated request tooling is missing")?;
    if let Some(environment) = request_value
        .get("reference_environment")
        .and_then(Value::as_object)
    {
        for (field, actual) in [
            ("python_executable_blake3", file_blake3(python)?),
            ("gpg_executable_blake3", file_blake3(gpg)?),
        ] {
            if environment.get(field).and_then(Value::as_str) != Some(actual.as_str()) {
                return Err(format!("signed {field} tooling digest mismatch"));
            }
        }
    }
    for (field, path) in [("verifier", verifier), ("signer_policy", policy)] {
        let actual = file_blake3(path)?;
        if tooling.get(field).and_then(Value::as_str) != Some(actual.as_str()) {
            return Err(format!("signed {field} tooling digest mismatch"));
        }
    }
    if request_value
        .get("qualification_approver_fingerprint")
        .and_then(Value::as_str)
        != Some(APPROVER_FINGERPRINT)
        || request_value
            .get("trust_policy_digest")
            .and_then(Value::as_str)
            != Some(APPROVER_POLICY_DIGEST)
    {
        return Err("authenticated request approver identity is not frozen".to_owned());
    }
    let verified = Command::new(gpg)
        .arg("--homedir")
        .arg(gpg_home)
        .arg("--batch")
        .arg("--no-tty")
        .arg("--status-fd")
        .arg("1")
        .arg("--verify")
        .arg(signature)
        .arg(request)
        .output()
        .map_err(|error| format!("fixed GPG verifier failed to start: {error}"))?;
    let status = String::from_utf8_lossy(&verified.stdout);
    let valid: Vec<Vec<&str>> = status
        .lines()
        .filter(|line| line.starts_with("[GNUPG:] VALIDSIG "))
        .map(|line| line.split_whitespace().collect())
        .collect();
    if !verified.status.success() || valid.len() != 1 {
        return Err("detached signature verification failed locally".to_owned());
    }
    let tokens = &valid[0];
    if tokens.len() < 12
        || tokens[8] != "22"
        || tokens.last().copied() != Some(APPROVER_FINGERPRINT)
    {
        return Err("local VALIDSIG fingerprint or Ed25519 algorithm mismatch".to_owned());
    }
    let signature_epoch: i64 = tokens
        .get(4)
        .ok_or("local VALIDSIG creation time is missing")?
        .parse()
        .map_err(|_| "local VALIDSIG creation time is invalid")?;
    let created = parse_utc(
        request_value
            .get("created_utc")
            .and_then(Value::as_str)
            .ok_or("request created_utc is missing")?,
    )?;
    let expires = parse_utc(
        request_value
            .get("expires_utc")
            .and_then(Value::as_str)
            .ok_or("request expires_utc is missing")?,
    )?;
    let now = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "system clock precedes Unix epoch")?
            .as_secs(),
    )
    .map_err(|_| "system clock is out of range")?;
    let signer_created = parse_utc("2026-08-27T04:49:51Z")?;
    let signer_expires = parse_utc("2028-08-26T04:49:51Z")?;
    if created >= expires
        || created < signer_created
        || expires > signer_expires
        || now < created
        || now >= expires
    {
        return Err(
            "locally authenticated request validity is outside approved policy/current time"
                .to_owned(),
        );
    }
    if signature_epoch < created || signature_epoch >= expires {
        return Err("local VALIDSIG creation time is outside request validity".to_owned());
    }
    let exported = Command::new(gpg)
        .arg("--homedir")
        .arg(gpg_home)
        .arg("--batch")
        .arg("--export")
        .arg(APPROVER_FINGERPRINT)
        .output()
        .map_err(|error| format!("allowlisted public key export failed: {error}"))?;
    if !exported.status.success()
        || blake3::hash(&exported.stdout).to_hex().as_str() != APPROVER_PACKET_BLAKE3
    {
        return Err("allowlisted public key packet BLAKE3 mismatch".to_owned());
    }
    Ok((request_value, request_digest))
}

fn hex32(value: &str) -> Result<[u8; 32], String> {
    if value.len() != 64 {
        return Err("digest length is invalid".to_owned());
    }
    let mut output = [0_u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| "digest hexadecimal is invalid".to_owned())?;
    }
    Ok(output)
}

fn parse_utc(value: &str) -> Result<i64, String> {
    if value.len() != 20
        || &value[4..5] != "-"
        || &value[7..8] != "-"
        || &value[10..11] != "T"
        || &value[13..14] != ":"
        || &value[16..17] != ":"
        || &value[19..] != "Z"
    {
        return Err("request UTC instant is invalid".to_owned());
    }
    let number = |range: std::ops::Range<usize>| {
        value[range]
            .parse::<i64>()
            .map_err(|_| "request UTC instant is invalid".to_owned())
    };
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return Err("request UTC instant is out of range".to_owned());
    }
    let adjusted_year = year - i64::from(month <= 2);
    let era = adjusted_year.div_euclid(400);
    let yoe = adjusted_year - era * 400;
    let shifted_month = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * shifted_month + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Ok(days * 86400 + hour * 3600 + minute * 60 + second)
}

/// Recompute the frozen Base semantic/artifact tuple digests from canonical evidence.
pub fn verify_base_tuple_evidence(
    path: &Path,
    candidate_commit: &str,
    target_triple: &str,
    toolchain_digest: &str,
    expected_semantic: &str,
    expected_artifact: &str,
) -> Result<(), String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("semantic tuple evidence could not be read: {error}"))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("semantic tuple evidence is invalid: {error}"))?;
    if serde_json::to_vec(&value).map_err(|error| error.to_string())? != bytes {
        return Err("semantic tuple evidence is not canonical JSON".to_owned());
    }
    if value.pointer("/base_commit/hex").and_then(Value::as_str) != Some(candidate_commit) {
        return Err("semantic tuple commit differs from signed request".to_owned());
    }
    if value.get("target_triple").and_then(Value::as_str) != Some(target_triple)
        || value.pointer("/toolchain/kind").and_then(Value::as_str) != Some("known")
        || value.pointer("/toolchain/hex").and_then(Value::as_str) != Some(toolchain_digest)
    {
        return Err("artifact tuple target/toolchain differs from measured evidence".to_owned());
    }
    let semantic = compatibility_tuple_bytes(&value, false)?;
    let artifact = compatibility_tuple_bytes(&value, true)?;
    if blake3::derive_key("onebrain:base:candidate-semantic:1\0", &semantic)
        != hex32(expected_semantic)?
    {
        return Err("measured candidate semantic digest differs from signed request".to_owned());
    }
    if blake3::derive_key("onebrain:base:artifact-tuple:1\0", &artifact)
        != hex32(expected_artifact)?
    {
        return Err("derived artifact tuple differs from signed request".to_owned());
    }
    Ok(())
}

fn compatibility_tuple_bytes(value: &Value, artifact: bool) -> Result<Vec<u8>, String> {
    validate_compatibility_tuple_schema(value)?;
    let u16le = |v: &Value, name: &str| -> Result<[u8; 2], String> {
        let number = v.as_u64().ok_or_else(|| format!("{name} is invalid"))?;
        Ok(u16::try_from(number)
            .map_err(|_| format!("{name} is out of range"))?
            .to_le_bytes())
    };
    let profile = |name: &str| -> Result<Vec<u8>, String> {
        let raw = value
            .get(name)
            .ok_or_else(|| format!("{name} is missing"))?;
        Ok([
            u16le(&raw["major"], name)?.as_slice(),
            u16le(&raw["minor"], name)?.as_slice(),
        ]
        .concat())
    };
    let digest = |name: &str| -> Result<Vec<u8>, String> {
        hex_bytes(
            value
                .get(name)
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{name} is missing"))?,
            32,
        )
    };
    let version = &value["base_version"];
    let mut release = Vec::new();
    for name in ["major", "minor", "patch"] {
        release.extend(u16le(&version[name], name)?);
    }
    match version.get("prerelease") {
        Some(Value::Null) => release.push(0),
        Some(Value::String(text)) if !text.is_empty() && text.is_ascii() => {
            release.push(1);
            release.extend(
                u32::try_from(text.len())
                    .map_err(|_| "prerelease too long")?
                    .to_le_bytes(),
            );
            release.extend(text.as_bytes());
        }
        _ => return Err("base_version.prerelease is invalid".to_owned()),
    }
    let commit_kind = value
        .pointer("/base_commit/kind")
        .and_then(Value::as_str)
        .ok_or("base_commit.kind is missing")?;
    let (discriminator, commit_size) = if commit_kind == "sha1" {
        (1_u8, 20)
    } else if commit_kind == "sha256" {
        (2, 32)
    } else {
        return Err("base_commit.kind is invalid".to_owned());
    };
    let commit_digest = hex_bytes(
        value
            .pointer("/base_commit/hex")
            .and_then(Value::as_str)
            .ok_or("base_commit.hex is missing")?,
        commit_size,
    )?;
    let mut commit = vec![1, discriminator];
    commit.extend((commit_size as u32).to_le_bytes());
    commit.extend(commit_digest);
    let storage = u32::try_from(
        value["storage_schema"]
            .as_u64()
            .ok_or("storage_schema is invalid")?,
    )
    .map_err(|_| "storage_schema out of range")?
    .to_le_bytes()
    .to_vec();
    let target = value["target_triple"]
        .as_str()
        .ok_or("target_triple is invalid")?
        .as_bytes()
        .to_vec();
    let tool_digest = hex_bytes(
        value
            .pointer("/toolchain/hex")
            .and_then(Value::as_str)
            .ok_or("toolchain.hex is missing")?,
        32,
    )?;
    let mut toolchain = vec![1];
    toolchain.extend(32_u32.to_le_bytes());
    toolchain.extend(tool_digest);
    let fields = vec![
        release,
        commit,
        digest("canonical_schema_digest")?,
        digest("domain_registry_digest")?,
        digest("resource_registry_digest")?,
        storage,
        profile("archive_profile")?,
        profile("migration_profile")?,
        profile("registry_profile")?,
        digest("registry_profile_digest")?,
        profile("wire_session")?,
        profile("product_api")?,
        profile("c_abi")?,
        digest("feature_set_digest")?,
        target,
        toolchain,
    ];
    let mut output = Vec::new();
    for (index, raw) in fields
        .into_iter()
        .take(if artifact { 16 } else { 14 })
        .enumerate()
    {
        output.extend(u16::try_from(index + 1).unwrap().to_le_bytes());
        output.extend(
            u32::try_from(raw.len())
                .map_err(|_| "compatibility field too long")?
                .to_le_bytes(),
        );
        output.extend(raw);
    }
    Ok(output)
}

fn exact_object<'a>(
    value: &'a Value,
    fields: &[&str],
    name: &str,
) -> Result<&'a serde_json::Map<String, Value>, String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{name} must be an object"))?;
    let expected: BTreeSet<&str> = fields.iter().copied().collect();
    let actual: BTreeSet<&str> = object.keys().map(String::as_str).collect();
    if actual != expected {
        return Err(format!("{name} fields are not closed"));
    }
    Ok(object)
}

fn validate_compatibility_tuple_schema(value: &Value) -> Result<(), String> {
    const TOP_LEVEL: [&str; 16] = [
        "base_version",
        "base_commit",
        "canonical_schema_digest",
        "domain_registry_digest",
        "resource_registry_digest",
        "storage_schema",
        "archive_profile",
        "migration_profile",
        "registry_profile",
        "registry_profile_digest",
        "wire_session",
        "product_api",
        "c_abi",
        "feature_set_digest",
        "target_triple",
        "toolchain",
    ];
    let tuple = exact_object(value, &TOP_LEVEL, "Base compatibility tuple")?;
    let unsigned = |value: &Value, maximum: u64, name: &str| -> Result<(), String> {
        match value.as_u64() {
            Some(number) if number <= maximum => Ok(()),
            _ => Err(format!("{name} must be an unsigned integer in range")),
        }
    };

    let version = exact_object(
        &tuple["base_version"],
        &["major", "minor", "patch", "prerelease"],
        "base_version",
    )?;
    for field in ["major", "minor", "patch"] {
        unsigned(
            &version[field],
            u16::MAX.into(),
            &format!("base_version.{field}"),
        )?;
    }
    match &version["prerelease"] {
        Value::Null => {}
        Value::String(text) if !text.is_empty() && text.is_ascii() && text.len() <= 32 => {}
        _ => return Err("base_version.prerelease is invalid".to_owned()),
    }

    let commit = exact_object(&tuple["base_commit"], &["kind", "hex"], "base_commit")?;
    let (kind, size) = match commit["kind"].as_str() {
        Some("sha1") => ("sha1", 20),
        Some("sha256") => ("sha256", 32),
        _ => return Err("base_commit.kind is invalid".to_owned()),
    };
    let commit_hex = commit["hex"]
        .as_str()
        .ok_or("base_commit.hex must be lowercase hexadecimal")?;
    hex_bytes(commit_hex, size)
        .map_err(|_| format!("base_commit {kind} hexadecimal is invalid"))?;

    for field in [
        "canonical_schema_digest",
        "domain_registry_digest",
        "resource_registry_digest",
        "registry_profile_digest",
        "feature_set_digest",
    ] {
        let text = tuple[field]
            .as_str()
            .ok_or_else(|| format!("{field} must be lowercase hexadecimal"))?;
        hex_bytes(text, 32).map_err(|_| format!("{field} must be lowercase hexadecimal"))?;
    }
    unsigned(&tuple["storage_schema"], u32::MAX.into(), "storage_schema")?;

    for field in [
        "archive_profile",
        "migration_profile",
        "registry_profile",
        "wire_session",
        "product_api",
        "c_abi",
    ] {
        let profile = exact_object(&tuple[field], &["major", "minor"], field)?;
        for component in ["major", "minor"] {
            unsigned(
                &profile[component],
                u16::MAX.into(),
                &format!("{field}.{component}"),
            )?;
        }
    }

    let target = tuple["target_triple"]
        .as_str()
        .ok_or("target_triple must be a string")?;
    if target.is_empty() || !target.is_ascii() || target.len() > 96 {
        return Err("target_triple is invalid".to_owned());
    }
    let toolchain = exact_object(&tuple["toolchain"], &["kind", "hex"], "toolchain")?;
    if toolchain["kind"].as_str() != Some("known") {
        return Err("qualification artifact toolchain must be known".to_owned());
    }
    let toolchain_hex = toolchain["hex"]
        .as_str()
        .ok_or("toolchain.hex must be lowercase hexadecimal")?;
    hex_bytes(toolchain_hex, 32)
        .map_err(|_| "toolchain.hex must be lowercase hexadecimal".to_owned())?;
    Ok(())
}

fn hex_bytes(value: &str, size: usize) -> Result<Vec<u8>, String> {
    if value.len() != size * 2
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("hexadecimal field length is invalid".to_owned());
    }
    (0..size)
        .map(|index| {
            u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
                .map_err(|_| "hexadecimal field is invalid".to_owned())
        })
        .collect()
}

fn validate_python_context(
    request: &Value,
    request_digest: &str,
    verified: &Value,
    production: bool,
) -> Result<(), String> {
    let candidate = request
        .get("candidate")
        .ok_or("authenticated request candidate is missing")?;
    if request.get("format").and_then(Value::as_str) == Some("onebrain/base-v1-release-request/2") {
        let tier = if production {
            "production-reference"
        } else {
            "nonproduction-test"
        };
        let expected_context = json!({
            "format": "onebrain/qualification-run-context/2",
            "variant": "Release",
            "release_request_digest": request_digest,
            "qualification_session_id": request.get("qualification_session_id"),
            "candidate_commit": candidate.get("commit"),
            "candidate_tree": candidate.get("tree"),
        });
        let expected_bindings = json!({
            "evidence_tier": tier,
            "release_request_digest": request_digest,
            "qualification_session_id": request.get("qualification_session_id"),
            "candidate_commit": candidate.get("commit"),
            "candidate_tree": candidate.get("tree"),
            "candidate_object_format": candidate.get("object_format"),
            "required_targets": request.get("required_targets"),
            "production_profile_blake3": request.get("production_profile_blake3"),
            "production_vector_blake3": request.get("production_vector_blake3"),
            "append_only_idl_history_root": request.get("append_only_idl_history_root"),
            "evidence_root_uri": request.get("evidence_root_uri"),
            "qualification_approver_trust_policy_digest": request.get("trust_policy_digest"),
            "qualification_approver_fingerprint": request.get("qualification_approver_fingerprint"),
        });
        let exact = verified.get("format").and_then(Value::as_str)
            == Some("onebrain/verified-qualification-context/2")
            && verified.get("production").and_then(Value::as_bool) == Some(production)
            && verified.get("request_digest").and_then(Value::as_str) == Some(request_digest)
            && verified.get("signer_fingerprint")
                == request.get("qualification_approver_fingerprint")
            && verified.get("trust_policy_digest") == request.get("trust_policy_digest")
            && verified.get("run_context") == Some(&expected_context)
            && verified.get("bindings") == Some(&expected_bindings)
            && verified.get("tooling_blake3") == request.get("candidate_tooling_blake3")
            && verified.as_object().is_some_and(|object| object.len() == 8);
        if !exact {
            return Err("Python verifier did not return the exact closed v2 context".to_owned());
        }
        return Ok(());
    }
    let registry = request
        .get("registry_candidate")
        .ok_or("authenticated request Registry binding is missing")?;
    let environment = request
        .get("reference_environment")
        .ok_or("authenticated request environment is missing")?;
    let tier = if production {
        "production-reference"
    } else {
        "nonproduction-test"
    };
    let expected_context = json!({
        "format": "onebrain/qualification-run-context/1",
        "variant": "Release",
        "release_request_digest": request_digest,
        "qualification_session_id": request.get("qualification_session_id"),
        "candidate_commit": candidate.get("commit"),
        "candidate_tree": candidate.get("tree"),
    });
    let expected_bindings = json!({
        "evidence_tier": tier,
        "release_request_digest": request_digest,
        "qualification_session_id": request.get("qualification_session_id"),
        "candidate_commit": candidate.get("commit"),
        "candidate_tree": candidate.get("tree"),
        "candidate_semantic_digest": registry.get("candidate_semantic_digest"),
        "artifact_tuple_digest": registry.get("artifact_tuple_digest"),
        "release_aggregate_root": registry.get("release_aggregate_root"),
        "registry_generation": registry.get("registry_generation"),
        "production_profile_blake3": request.get("production_profile_blake3"),
        "production_vector_blake3": request.get("production_vector_blake3"),
        "append_only_idl_history_root": request.get("append_only_idl_history_root"),
        "required_targets": request.get("required_targets"),
        "candidate_payload_artifacts_blake3": registry.get("payload_artifacts_blake3"),
        "release_stamp_blake3": registry.get("release_stamp_blake3"),
        "probe_blake3": environment.get("probe_blake3"),
        "probe_signature": environment.get("probe_signature"),
        "probe_signer_fingerprint": environment.get("probe_signer_fingerprint"),
        "probe_signer_public_key": environment.get("probe_signer_public_key"),
        "executable_blake3": environment.get("executable_blake3"),
        "rust_toolchain_digest": environment.get("rust_toolchain_digest"),
        "runner_image_digest": environment.get("runner_image_digest"),
        "target_triple": environment.get("target_triple"),
        "python_executable_blake3": environment.get("python_executable_blake3"),
        "gpg_executable_blake3": environment.get("gpg_executable_blake3"),
        "trust_policy_digest": registry.get("registry_trust_policy_digest"),
        "signer_fingerprint": registry.get("registry_signer_fingerprint"),
        "ccid_inputs_blake3": registry.get("ccid_inputs_blake3"),
    });
    let exact = verified.get("format").and_then(Value::as_str)
        == Some("onebrain/verified-qualification-context/1")
        && verified.get("production").and_then(Value::as_bool) == Some(production)
        && verified.get("request_digest").and_then(Value::as_str) == Some(request_digest)
        && verified.get("signer_fingerprint") == request.get("qualification_approver_fingerprint")
        && verified.get("trust_policy_digest") == request.get("trust_policy_digest")
        && verified.get("run_context") == Some(&expected_context)
        && verified.get("bindings") == Some(&expected_bindings)
        && verified.get("tooling_blake3") == request.get("candidate_tooling_blake3")
        && verified.as_object().is_some_and(|object| object.len() == 8);
    if !exact {
        return Err("Python verifier did not return the exact closed verified context".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_base_tuple() -> Value {
        json!({
            "base_version": {"major": 1, "minor": 0, "patch": 0, "prerelease": null},
            "base_commit": {"kind": "sha1", "hex": "11".repeat(20)},
            "canonical_schema_digest": "21".repeat(32),
            "domain_registry_digest": "22".repeat(32),
            "resource_registry_digest": "23".repeat(32),
            "storage_schema": 1,
            "archive_profile": {"major": 1, "minor": 0},
            "migration_profile": {"major": 1, "minor": 0},
            "registry_profile": {"major": 1, "minor": 0},
            "registry_profile_digest": "24".repeat(32),
            "wire_session": {"major": 1, "minor": 0},
            "product_api": {"major": 1, "minor": 0},
            "c_abi": {"major": 1, "minor": 0},
            "feature_set_digest": "25".repeat(32),
            "target_triple": "x86_64-unknown-linux-gnu",
            "toolchain": {"kind": "known", "hex": "26".repeat(32)}
        })
    }

    #[test]
    fn fake_python_stdout_without_authenticated_bindings_is_rejected() {
        let request = json!({
            "qualification_session_id": "42".repeat(32),
            "candidate": {"commit": "11".repeat(20), "tree": "22".repeat(20)},
            "trust_policy_digest": "33".repeat(32),
            "candidate_tooling_blake3": {
                "qualifier": "41".repeat(32), "request": "42".repeat(32),
                "clean_worktree": "43".repeat(32), "release_wrapper": "44".repeat(32),
                "verifier": "45".repeat(32), "signer_policy": "46".repeat(32)
            },
            "registry_candidate": {
                "candidate_semantic_digest": "51".repeat(32),
                "artifact_tuple_digest": "52".repeat(32),
                "release_aggregate_root": "53".repeat(32),
                "registry_generation": 7
            },
            "reference_environment": {"target_triple": "x86_64-unknown-linux-gnu"}
        });
        let fake = json!({
            "format": "onebrain/verified-qualification-context/1",
            "production": true
        });
        let error = validate_python_context(&request, "aa", &fake, true).unwrap_err();
        assert!(error.contains("closed verified context"), "{error}");
    }

    #[test]
    fn base_tuple_schema_rejects_unknown_missing_and_wrong_type_fields() {
        let mut extra_top_level = valid_base_tuple();
        extra_top_level["unexpected"] = json!(true);

        let mut extra_nested = valid_base_tuple();
        extra_nested["toolchain"]["unexpected"] = json!(true);

        let mut missing = valid_base_tuple();
        missing
            .as_object_mut()
            .expect("tuple object")
            .remove("feature_set_digest");

        let mut wrong_type = valid_base_tuple();
        wrong_type["storage_schema"] = json!("1");

        let accepted: Vec<&str> = [
            ("extra top-level field", extra_top_level),
            ("extra nested field", extra_nested),
            ("missing field", missing),
            ("wrong-type field", wrong_type),
        ]
        .into_iter()
        .filter_map(|(name, value)| {
            compatibility_tuple_bytes(&value, true)
                .is_ok()
                .then_some(name)
        })
        .collect();

        assert!(
            accepted.is_empty(),
            "accepted invalid Base tuple schemas: {accepted:?}"
        );
    }

    #[test]
    fn base_tuple_evidence_rejects_duplicate_and_noncanonical_json() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("base-tuple.json");
        let canonical = serde_json::to_string(&valid_base_tuple()).unwrap();
        let duplicate = format!(
            "{},\"feature_set_digest\":\"{}\"}}",
            canonical.strip_suffix('}').unwrap(),
            "25".repeat(32)
        );
        for (name, bytes) in [
            ("duplicate", duplicate.into_bytes()),
            ("noncanonical", format!("{canonical}\n").into_bytes()),
        ] {
            fs::write(&path, bytes).unwrap();
            let error = verify_base_tuple_evidence(
                &path,
                &"11".repeat(20),
                "x86_64-unknown-linux-gnu",
                &"26".repeat(32),
                &"00".repeat(32),
                &"00".repeat(32),
            )
            .unwrap_err();
            assert!(
                error.contains("not canonical JSON"),
                "{name} tuple should fail canonical parsing: {error}"
            );
        }
    }
}
