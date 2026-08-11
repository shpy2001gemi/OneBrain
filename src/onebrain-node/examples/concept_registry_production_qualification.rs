//! Signed live-reader generation-swap and rollback qualification harness.

use std::error::Error;
#[cfg(unix)]
use std::fs::File;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use ku_core::{
    activate_concept_registry_release, parse_concept_registry_verifying_key,
    qualification_request::{
        verify_base_release_request, verify_base_release_request_for_test_nonproduction,
        verify_base_tuple_evidence,
    },
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
    let raw_invocation: Vec<String> = std::env::args().collect();
    let mut args = raw_invocation.iter().skip(1).cloned();
    let mode = required_string(&mut args, "MODE")?;
    if mode != "--release" && mode != "--test-release-nonproduction" {
        return Err("explicit --release mode is required".into());
    }
    let registry_root = required_path(&mut args, "REGISTRY_ROOT")?;
    let public_key_hex = required_string(&mut args, "RELEASE_PUBLIC_KEY")?;
    let old_release = required_string(&mut args, "OLD_RELEASE_ID")?;
    let new_release = required_string(&mut args, "NEW_RELEASE_ID")?;
    let query_label = required_string(&mut args, "QUERY_LABEL")?;
    let release_request = required_path(&mut args, "RELEASE_REQUEST")?;
    let release_request_signature = required_path(&mut args, "RELEASE_REQUEST_SIGNATURE")?;
    let approver_policy = required_path(&mut args, "QUALIFICATION_APPROVER_POLICY")?;
    let gpg_home = required_path(&mut args, "GPG_HOME")?;
    let (verified, git_executable, production) = if mode == "--release" {
        (
            verify_base_release_request(
                &release_request,
                &release_request_signature,
                &approver_policy,
                &gpg_home,
            )?,
            PathBuf::from("/usr/bin/git"),
            true,
        )
    } else {
        let python = required_path(&mut args, "TEST_PYTHON")?;
        let gpg = required_path(&mut args, "TEST_GPG")?;
        let git = required_path(&mut args, "TEST_GIT")?;
        (
            verify_base_release_request_for_test_nonproduction(
                &python,
                &gpg,
                &release_request,
                &release_request_signature,
                &approver_policy,
                &gpg_home,
            )?,
            git,
            false,
        )
    };
    let candidate_root = required_path(&mut args, "CANDIDATE_ROOT")?;
    let semantic_evidence = required_path(&mut args, "CANDIDATE_SEMANTIC_EVIDENCE")?;
    let production_profile = required_path(&mut args, "PRODUCTION_PROFILE")?;
    let production_vector = required_path(&mut args, "PRODUCTION_VECTOR")?;
    let idl_history = required_path(&mut args, "APPEND_ONLY_IDL_HISTORY")?;
    let probe_executable = required_path(&mut args, "PROBE_EXECUTABLE")?;
    let probe_signature = required_path(&mut args, "PROBE_SIGNATURE")?;
    let rust_toolchain_evidence = required_path(&mut args, "RUST_TOOLCHAIN_EVIDENCE")?;
    let runner_image_evidence = required_path(&mut args, "RUNNER_IMAGE_EVIDENCE")?;
    let target_triple = required_string(&mut args, "TARGET_TRIPLE")?;
    let policy_path = required_path(&mut args, "TRUST_POLICY_JSON")?;
    let private_key_path = required_path(&mut args, "PRIVATE_KEY_FILE")?;
    let output_path = required_path(&mut args, "OUTPUT_JSON")?;
    if args.next().is_some() {
        return Err(usage().into());
    }

    let release_key = parse_concept_registry_verifying_key(&public_key_hex)?;
    let signing_key = read_signing_key(&private_key_path)?;
    let policy: Value = serde_json::from_slice(&fs::read(policy_path)?)?;
    let context = verified
        .get("run_context")
        .cloned()
        .ok_or("verified request omitted run_context")?;
    let mut binding: Map<String, Value> = verified
        .get("bindings")
        .and_then(Value::as_object)
        .cloned()
        .ok_or("verified request omitted bindings")?;
    validate_candidate_measurements(
        &registry_root,
        &new_release,
        &binding,
        &probe_executable,
        &probe_signature,
        &rust_toolchain_evidence,
        &runner_image_evidence,
        &target_triple,
        &candidate_root,
        &semantic_evidence,
        &production_profile,
        &production_vector,
        &idl_history,
        &git_executable,
        production,
    )?;
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
    let manager = ConceptRegistryGenerationManager::open(config.clone())?;
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
    drop(manager);
    let reopened_manager = ConceptRegistryGenerationManager::open(config)?;
    let final_reader = reopened_manager.reader_lease();
    final_reader.resolve_checked(&query_label)?;
    let exact_root_after_reopen = final_reader.status().release_aggregate_root.as_deref()
        == Some(expected_root.as_str())
        && final_reader.status().release_generation == Some(expected_generation);

    let result = old_reader_pinned
        && new_reader_complete
        && rollback_with_active_reader
        && exact_root_after_reopen;
    let command = sanitized_invocation(&raw_invocation, &mode)?;
    let command_blake3 = blake3::hash(&serde_json::to_vec(&command)?)
        .to_hex()
        .to_string();
    binding.insert("command".to_owned(), json!(command));
    binding.insert("command_blake3".to_owned(), json!(command_blake3));
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

fn sanitized_invocation(raw: &[String], mode: &str) -> Result<Vec<String>, Box<dyn Error>> {
    raw.iter()
        .enumerate()
        .map(|(index, value)| {
            if index == 10 {
                return Ok("<gpg-home-redacted>".to_owned());
            }
            let private_index = if mode == "--release" { 22 } else { 25 };
            if index == private_index {
                return Ok("<external-private-key-redacted>".to_owned());
            }
            if mode == "--test-release-nonproduction" && (11..=13).contains(&index) {
                return Ok(format!("<nonproduction-test-tool-{index}>"));
            }
            let path = Path::new(value);
            if path.is_file() {
                let name = path
                    .file_name()
                    .and_then(|part| part.to_str())
                    .unwrap_or("input");
                return Ok(format!("{name}@blake3:{}", blake3_file(path)?));
            }
            Ok(value.clone())
        })
        .collect()
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
            payload.insert("evidence_tier".to_owned(), json!("prequalification"));
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

fn validate_candidate_measurements(
    registry_root: &Path,
    release_id: &str,
    binding: &Map<String, Value>,
    probe: &Path,
    probe_signature: &Path,
    toolchain: &Path,
    runner_image: &Path,
    target_triple: &str,
    candidate_root: &Path,
    semantic_evidence: &Path,
    production_profile: &Path,
    production_vector: &Path,
    idl_history: &Path,
    git_executable: &Path,
    production: bool,
) -> Result<(), Box<dyn Error>> {
    if production && (!cfg!(target_os = "linux") || git_executable != Path::new("/usr/bin/git")) {
        return Err("production candidate measurement requires fixed Linux Git".into());
    }
    let git = |revision: &str| -> Result<String, Box<dyn Error>> {
        let output = std::process::Command::new(git_executable)
            .arg("-C")
            .arg(candidate_root)
            .arg("rev-parse")
            .arg(revision)
            .output()?;
        if !output.status.success() {
            return Err(format!("candidate Git measurement failed: {revision}").into());
        }
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    };
    if binding.get("candidate_commit").and_then(Value::as_str) != Some(git("HEAD")?.as_str())
        || binding.get("candidate_tree").and_then(Value::as_str)
            != Some(git("HEAD^{tree}")?.as_str())
    {
        return Err("candidate Git evidence differs from signed request".into());
    }
    let canonical_digest = |path: &Path| -> Result<String, Box<dyn Error>> {
        let value: Value = serde_json::from_slice(&fs::read(path)?)?;
        Ok(blake3::hash(&serde_json::to_vec(&value)?)
            .to_hex()
            .to_string())
    };
    if binding
        .get("production_profile_blake3")
        .and_then(Value::as_str)
        != Some(canonical_digest(production_profile)?.as_str())
        || binding
            .get("production_vector_blake3")
            .and_then(Value::as_str)
            != Some(canonical_digest(production_vector)?.as_str())
        || binding
            .get("append_only_idl_history_root")
            .and_then(Value::as_str)
            != Some(fs::read_to_string(idl_history)?.trim())
        || binding
            .get("required_targets")
            .and_then(|value| value.get(target_triple))
            .and_then(Value::as_str)
            != binding.get("artifact_tuple_digest").and_then(Value::as_str)
    {
        return Err("candidate profile/vector/history/target differs from signed request".into());
    }
    let measured_toolchain_digest = blake3_file(toolchain)?;
    verify_base_tuple_evidence(
        semantic_evidence,
        binding
            .get("candidate_commit")
            .and_then(Value::as_str)
            .ok_or("candidate commit is missing")?,
        target_triple,
        &measured_toolchain_digest,
        binding
            .get("candidate_semantic_digest")
            .and_then(Value::as_str)
            .ok_or("semantic digest is missing")?,
        binding
            .get("artifact_tuple_digest")
            .and_then(Value::as_str)
            .ok_or("artifact tuple digest is missing")?,
    )?;
    let stamp_path = registry_root
        .join("releases")
        .join(release_id)
        .join("release.stamp.json");
    if binding.get("release_stamp_blake3").and_then(Value::as_str)
        != Some(blake3_file(&stamp_path)?.as_str())
    {
        return Err("measured release stamp differs from signed request".into());
    }
    let stamp: Value = serde_json::from_slice(&fs::read(&stamp_path)?)?;
    let mut artifacts = Map::new();
    for artifact in stamp
        .get("artifacts")
        .and_then(Value::as_array)
        .ok_or("release stamp artifacts are missing")?
    {
        let role = artifact
            .get("role")
            .and_then(Value::as_str)
            .ok_or("stamp role missing")?;
        let path = artifact
            .get("relative_path")
            .and_then(Value::as_str)
            .ok_or("stamp path missing")?;
        let digest = artifact
            .get("blake3")
            .cloned()
            .ok_or("stamp digest missing")?;
        artifacts.insert(format!("{role}:{path}"), digest);
    }
    if binding.get("candidate_payload_artifacts_blake3") != Some(&Value::Object(artifacts)) {
        return Err("five measured release payloads differ from signed request".into());
    }
    let probe_digest = blake3_file(probe)?;
    if binding.get("probe_blake3").and_then(Value::as_str) != Some(probe_digest.as_str())
        || binding.get("executable_blake3").and_then(Value::as_str) != Some(probe_digest.as_str())
        || binding.get("probe_signature").and_then(Value::as_str)
            != Some(blake3_file(probe_signature)?.as_str())
        || binding.get("rust_toolchain_digest").and_then(Value::as_str)
            != Some(blake3_file(toolchain)?.as_str())
        || binding.get("runner_image_digest").and_then(Value::as_str)
            != Some(blake3_file(runner_image)?.as_str())
        || binding.get("target_triple").and_then(Value::as_str) != Some(target_triple)
    {
        return Err("measured probe or reference environment differs from signed request".into());
    }
    let public = decode_hex::<32>(
        binding
            .get("probe_signer_public_key")
            .and_then(Value::as_str)
            .ok_or("probe signer key missing")?,
    )?;
    if signer_fingerprint(&public)
        != binding
            .get("probe_signer_fingerprint")
            .and_then(Value::as_str)
            .ok_or("probe signer fingerprint missing")?
    {
        return Err("probe signer fingerprint differs from public key".into());
    }
    let signature = decode_hex::<64>(fs::read_to_string(probe_signature)?.trim())?;
    let mut message = b"onebrain:concept-registry-probe:1\0".to_vec();
    message.extend_from_slice(blake3::hash(&fs::read(probe)?).as_bytes());
    VerifyingKey::from_bytes(&public)?.verify(&message, &Signature::from_bytes(&signature))?;
    Ok(())
}

fn blake3_file(path: &Path) -> Result<String, Box<dyn Error>> {
    Ok(blake3::hash(&fs::read(path)?).to_hex().to_string())
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
    "usage: concept_registry_production_qualification (--release | --test-release-nonproduction) REGISTRY_ROOT RELEASE_PUBLIC_KEY OLD_RELEASE_ID NEW_RELEASE_ID QUERY_LABEL RELEASE_REQUEST RELEASE_REQUEST_SIGNATURE QUALIFICATION_APPROVER_POLICY GPG_HOME [TEST_PYTHON TEST_GPG TEST_GIT] CANDIDATE_ROOT CANDIDATE_SEMANTIC_EVIDENCE PRODUCTION_PROFILE PRODUCTION_VECTOR APPEND_ONLY_IDL_HISTORY PROBE_EXECUTABLE PROBE_SIGNATURE RUST_TOOLCHAIN_EVIDENCE RUNNER_IMAGE_EVIDENCE TARGET_TRIPLE TRUST_POLICY_JSON PRIVATE_KEY_FILE OUTPUT_JSON"
}
