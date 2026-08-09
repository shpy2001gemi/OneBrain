//! Signed live-reader generation-swap and rollback qualification harness.

use std::error::Error;
#[cfg(unix)]
use std::fs::File;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signer, SigningKey};
use ku_core::{
    activate_concept_registry_release, parse_concept_registry_verifying_key,
    rollback_concept_registry_release,
};
use onebrain_node::concept_registry_runtime::ConceptRegistryGenerationManager;
use onebrain_node::{ConceptRegistryMode, NodeConfig};
use serde_json::{json, Map, Value};

const RECEIPT_DOMAIN: &[u8] = b"onebrain:concept-registry-qualification-receipt:1\0";
const FINGERPRINT_CONTEXT: &str = "onebrain:concept-registry:signer-fingerprint:1";
const TRUST_POLICY_CONTEXT: &str = "onebrain:concept-registry:trust-policy:1";

fn main() {
    if let Err(error) = run() {
        eprintln!("concept-registry-production-qualification: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let registry_root = required_path(&mut args, "REGISTRY_ROOT")?;
    let public_key_hex = required_string(&mut args, "RELEASE_PUBLIC_KEY")?;
    let old_release = required_string(&mut args, "OLD_RELEASE_ID")?;
    let new_release = required_string(&mut args, "NEW_RELEASE_ID")?;
    let query_label = required_string(&mut args, "QUERY_LABEL")?;
    let context_path = required_path(&mut args, "RUN_CONTEXT_JSON")?;
    let binding_path = required_path(&mut args, "BINDING_JSON")?;
    let policy_path = required_path(&mut args, "TRUST_POLICY_JSON")?;
    let private_key_path = required_path(&mut args, "PRIVATE_KEY_FILE")?;
    let output_path = required_path(&mut args, "OUTPUT_JSON")?;
    if args.next().is_some() {
        return Err(usage().into());
    }

    let release_key = parse_concept_registry_verifying_key(&public_key_hex)?;
    let signing_key = read_signing_key(&private_key_path)?;
    let policy: Value = serde_json::from_slice(&fs::read(policy_path)?)?;
    let context: Value = serde_json::from_slice(&fs::read(context_path)?)?;
    let mut binding: Map<String, Value> = serde_json::from_slice(&fs::read(binding_path)?)?;
    validate_receipt_signer(&policy, &signing_key, &binding)?;
    let expected_root = binding
        .get("release_aggregate_root")
        .and_then(Value::as_str)
        .ok_or("binding release_aggregate_root is missing")?
        .to_owned();
    let expected_generation = binding
        .get("registry_generation")
        .and_then(Value::as_u64)
        .ok_or("binding registry_generation is missing")?;

    let mut config = NodeConfig {
        concept_registry_mode: ConceptRegistryMode::Required,
        concept_registry_release_root: Some(registry_root.clone()),
        concept_registry_release_public_key: Some(public_key_hex),
        ..NodeConfig::default()
    };
    config.concept_registry_path = None;
    let manager = ConceptRegistryGenerationManager::open(config)?;
    let old_reader = manager.reader_lease();
    if old_reader.status().release_id.as_deref() != Some(old_release.as_str()) {
        return Err("initial reader is not pinned to the expected old release".into());
    }
    old_reader.resolve_checked(&query_label)?;

    activate_concept_registry_release(&registry_root, &new_release, &release_key)?;
    manager.refresh()?;
    let candidate_reader = manager.reader_lease();
    candidate_reader.resolve_checked(&query_label)?;
    let old_reader_pinned = old_reader.status().release_id.as_deref() == Some(old_release.as_str());
    let new_reader_complete = candidate_reader.status().release_id.as_deref()
        == Some(new_release.as_str())
        && candidate_reader.status().release_aggregate_root.as_deref()
            == Some(expected_root.as_str());

    rollback_concept_registry_release(&registry_root, &release_key)?;
    manager.refresh()?;
    let rollback_reader = manager.reader_lease();
    rollback_reader.resolve_checked(&query_label)?;
    let rollback_with_active_reader = candidate_reader.status().release_id.as_deref()
        == Some(new_release.as_str())
        && rollback_reader.status().release_id.as_deref() == Some(old_release.as_str());

    activate_concept_registry_release(&registry_root, &new_release, &release_key)?;
    manager.refresh()?;
    let final_reader = manager.reader_lease();
    final_reader.resolve_checked(&query_label)?;
    let exact_root_after_reopen = final_reader.status().release_aggregate_root.as_deref()
        == Some(expected_root.as_str())
        && final_reader.status().release_generation == Some(expected_generation);

    let result = old_reader_pinned
        && new_reader_complete
        && rollback_with_active_reader
        && exact_root_after_reopen;
    binding.insert(
        "command".to_owned(),
        json!([
            "concept_registry_production_qualification",
            "generation-swap"
        ]),
    );
    binding.insert("result".to_owned(), json!(result));
    binding.insert(
        "exit_oracles".to_owned(),
        json!({
            "old_reader_remained_pinned": old_reader_pinned,
            "new_reader_saw_complete_candidate": new_reader_complete,
            "rollback_preserved_active_candidate_reader": rollback_with_active_reader,
            "reactivation_reopened_exact_candidate_root": exact_root_after_reopen,
        }),
    );
    binding.insert(
        "limitations".to_owned(),
        json!(["Registry-only generation-swap evidence; never BASE-GATE-V1"]),
    );
    apply_run_context(&mut binding, &context)?;
    let receipt = sign_receipt(Value::Object(binding), &policy, &signing_key)?;
    write_json_atomic(&output_path, &receipt)?;
    println!("{}", serde_json::to_string(&receipt)?);
    if !result {
        return Err("one or more generation-swap oracles failed".into());
    }
    Ok(())
}

fn apply_run_context(
    payload: &mut Map<String, Value>,
    context: &Value,
) -> Result<(), Box<dyn Error>> {
    let object = context
        .as_object()
        .ok_or("QualificationRunContextV1 is not an object")?;
    if context.get("format").and_then(Value::as_str) != Some("onebrain/qualification-run-context/1")
    {
        return Err("QualificationRunContextV1 format is invalid".into());
    }
    match context.get("variant").and_then(Value::as_str) {
        Some("Prequalification") => {
            let expected: std::collections::BTreeSet<_> = ["format", "variant", "closure_digest"]
                .into_iter()
                .collect();
            if object
                .keys()
                .map(String::as_str)
                .collect::<std::collections::BTreeSet<_>>()
                != expected
            {
                return Err("Prequalification context fields are not closed".into());
            }
            payload.remove("candidate_semantic_digest");
            payload.remove("artifact_tuple_digest");
            payload.insert(
                "qualification_context_variant".to_owned(),
                json!("Prequalification"),
            );
            payload.insert(
                "closure_digest".to_owned(),
                context
                    .get("closure_digest")
                    .cloned()
                    .ok_or("closure_digest is missing")?,
            );
            payload.insert("base_candidate_bound".to_owned(), json!(false));
        }
        Some("Release") => {
            let fields = [
                "release_request_digest",
                "qualification_session_id",
                "candidate_commit",
                "candidate_tree",
            ];
            let expected: std::collections::BTreeSet<_> =
                ["format", "variant"].into_iter().chain(fields).collect();
            if object
                .keys()
                .map(String::as_str)
                .collect::<std::collections::BTreeSet<_>>()
                != expected
            {
                return Err("Release context fields are not closed".into());
            }
            validate_hex_text(
                context
                    .get("release_request_digest")
                    .and_then(Value::as_str)
                    .ok_or("release_request_digest is missing")?,
                &[64],
            )?;
            for field in ["candidate_commit", "candidate_tree"] {
                validate_hex_text(
                    context
                        .get(field)
                        .and_then(Value::as_str)
                        .ok_or("Release git identity is missing")?,
                    &[40, 64],
                )?;
            }
            if context
                .get("qualification_session_id")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            {
                return Err("qualification_session_id is missing".into());
            }
            if !payload.contains_key("candidate_semantic_digest")
                || !payload.contains_key("artifact_tuple_digest")
            {
                return Err("verified Release binding is incomplete".into());
            }
            payload.insert("qualification_context_variant".to_owned(), json!("Release"));
            for field in fields {
                payload.insert(
                    field.to_owned(),
                    context
                        .get(field)
                        .cloned()
                        .ok_or("Release field is missing")?,
                );
            }
            payload.insert("base_candidate_bound".to_owned(), json!(true));
        }
        _ => return Err("QualificationRunContextV1 variant is invalid".into()),
    }
    Ok(())
}

fn sign_receipt(payload: Value, policy: &Value, key: &SigningKey) -> Result<Value, Box<dyn Error>> {
    let public = hex(key.verifying_key().as_bytes());
    let fingerprint = signer_fingerprint(key.verifying_key().as_bytes());
    let policy_digest = derive_json(TRUST_POLICY_CONTEXT, policy)?;
    let mut receipt = json!({
        "format": "onebrain/concept-registry-qualification-receipt/1",
        "receipt_kind": "generation-swap",
        "usage": "registry-qualification-receipt",
        "payload": payload,
        "signer_public_key": public,
        "signer_fingerprint": fingerprint,
        "trust_policy_digest": policy_digest,
        "signature": "",
    });
    let digest = blake3::hash(&serde_json::to_vec(&receipt)?);
    let mut message = Vec::with_capacity(RECEIPT_DOMAIN.len() + 32);
    message.extend_from_slice(RECEIPT_DOMAIN);
    message.extend_from_slice(digest.as_bytes());
    receipt["signature"] = Value::String(hex(&key.sign(&message).to_bytes()));
    Ok(receipt)
}

fn validate_receipt_signer(
    policy: &Value,
    key: &SigningKey,
    binding: &Map<String, Value>,
) -> Result<(), Box<dyn Error>> {
    let public = hex(key.verifying_key().as_bytes());
    let fingerprint = signer_fingerprint(key.verifying_key().as_bytes());
    let allowed = policy
        .get("signers")
        .and_then(Value::as_array)
        .is_some_and(|signers| {
            signers.iter().any(|signer| {
                signer.get("public_key_hex").and_then(Value::as_str) == Some(public.as_str())
                    && signer.get("fingerprint_hex").and_then(Value::as_str)
                        == Some(fingerprint.as_str())
            })
        });
    if policy.get("algorithm").and_then(Value::as_str) != Some("Ed25519")
        || !policy
            .get("allowed_usages")
            .and_then(Value::as_array)
            .is_some_and(|values| {
                values
                    .iter()
                    .any(|value| value == "registry-qualification-receipt")
            })
        || !allowed
    {
        return Err("qualification receipt signer is not allowlisted".into());
    }
    let policy_digest = derive_json(TRUST_POLICY_CONTEXT, policy)?;
    if binding.get("trust_policy_digest").and_then(Value::as_str) != Some(policy_digest.as_str())
        || binding.get("signer_fingerprint").and_then(Value::as_str) != Some(fingerprint.as_str())
    {
        return Err("binding trust policy or signer fingerprint mismatch".into());
    }
    Ok(())
}

fn derive_json(context: &str, value: &Value) -> Result<String, Box<dyn Error>> {
    let mut hasher = blake3::Hasher::new_derive_key(context);
    hasher.update(&serde_json::to_vec(value)?);
    Ok(hasher.finalize().to_hex().to_string())
}

fn signer_fingerprint(public: &[u8; 32]) -> String {
    let mut hasher = blake3::Hasher::new_derive_key(FINGERPRINT_CONTEXT);
    hasher.update(public);
    hasher.finalize().to_hex().to_string()
}

fn read_signing_key(path: &Path) -> Result<SigningKey, Box<dyn Error>> {
    let value = fs::read_to_string(path)?;
    let bytes = decode_hex::<32>(value.trim())?;
    Ok(SigningKey::from_bytes(&bytes))
}

fn decode_hex<const N: usize>(value: &str) -> Result<[u8; N], Box<dyn Error>> {
    if value.len() != N * 2
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("hex value has the wrong lowercase shape".into());
    }
    let mut bytes = [0u8; N];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)?;
    }
    Ok(bytes)
}

fn validate_hex_text(value: &str, lengths: &[usize]) -> Result<(), Box<dyn Error>> {
    if !lengths.contains(&value.len())
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("context identity is not lowercase hexadecimal".into());
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn write_json_atomic(path: &Path, value: &Value) -> Result<(), Box<dyn Error>> {
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap().to_string_lossy(),
        std::process::id()
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    fs::rename(temporary, path)?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}

fn required_path(
    args: &mut impl Iterator<Item = String>,
    name: &str,
) -> Result<PathBuf, Box<dyn Error>> {
    Ok(PathBuf::from(required_string(args, name)?))
}

fn required_string(
    args: &mut impl Iterator<Item = String>,
    name: &str,
) -> Result<String, Box<dyn Error>> {
    args.next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("missing {name}\n{}", usage()).into())
}

fn usage() -> &'static str {
    "usage: concept_registry_production_qualification REGISTRY_ROOT RELEASE_PUBLIC_KEY OLD_RELEASE_ID NEW_RELEASE_ID QUERY_LABEL RUN_CONTEXT_JSON BINDING_JSON TRUST_POLICY_JSON PRIVATE_KEY_FILE OUTPUT_JSON"
}
