//! Isolated truncated-index and disk-shortage qualification for one exact
//! Concept Registry input set.

#[cfg(not(feature = "concept-registry-failure-harness"))]
compile_error!(
    "concept_registry_failure_qualification requires --features concept-registry-failure-harness"
);

use std::env;
use std::error::Error;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signer, SigningKey};
use ku_core::{
    activate_concept_registry_release, concept_registry_release_capacity,
    package_concept_registry_release, package_concept_registry_release_with_capacity_for_drill,
    resolve_active_concept_registry_release, verify_concept_registry_release,
    ConceptRegistryReleaseError, ConceptRegistryReleasePackageInput, ConceptRegistryReleaseSource,
};
use serde_json::{json, Value};

const PROFILE: &str = "onebrain/concept-registry-failure-qualification/1";
const STABLE_RELEASE: &str = "qualification-stable";
const CANDIDATE_RELEASE: &str = "qualification-candidate";
const DISK_SHORTAGE_RELEASE: &str = "qualification-disk-shortage";
const RECEIPT_DOMAIN: &[u8] = b"onebrain:concept-registry-qualification-receipt:1\0";
const FINGERPRINT_CONTEXT: &str = "onebrain:concept-registry:signer-fingerprint:1";
const TRUST_POLICY_CONTEXT: &str = "onebrain:concept-registry:trust-policy:1";

fn main() {
    if let Err(error) = run() {
        eprintln!("concept-registry-failure-qualification: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    if env::args().nth(1).as_deref() == Some("--kill-worker") {
        return run_kill_worker();
    }
    let mut args = env::args().skip(1);
    let work_dir = required_path(&mut args, "WORK_DIR")?;
    let obr_path = required_path(&mut args, "OBR_PATH")?;
    let sbom_path = required_path(&mut args, "SPDX_SBOM_PATH")?;
    let sources_path = required_path(&mut args, "SOURCES_JSON_PATH")?;
    let private_key_path = required_path(&mut args, "PRIVATE_KEY_FILE")?;
    let context_path = required_path(&mut args, "RUN_CONTEXT_JSON")?;
    let binding_path = required_path(&mut args, "RELEASE_BINDING_JSON")?;
    let policy_path = required_path(&mut args, "TRUST_POLICY_JSON")?;
    let output_path = required_path(&mut args, "OUTPUT_JSON")?;
    no_more_args(args)?;

    fs::create_dir_all(&work_dir)?;
    let temporary = tempfile::Builder::new()
        .prefix("onebrain-registry-failure-")
        .tempdir_in(&work_dir)?;
    let registry_root = temporary.path().join("registry");
    let sources: Vec<ConceptRegistryReleaseSource> =
        serde_json::from_slice(&fs::read(&sources_path)?)?;
    let signing_key = read_signing_key(&private_key_path)?;
    let public_key = signing_key.verifying_key();
    let context: Value = serde_json::from_slice(&fs::read(&context_path)?)?;
    let binding: Value = serde_json::from_slice(&fs::read(&binding_path)?)?;
    let policy: Value = serde_json::from_slice(&fs::read(&policy_path)?)?;
    validate_receipt_signer(&policy, &signing_key, &binding)?;

    package(
        &obr_path,
        &sbom_path,
        &registry_root,
        STABLE_RELEASE,
        &sources,
        &signing_key,
    )?;
    let candidate_stamp = package(
        &obr_path,
        &sbom_path,
        &registry_root,
        CANDIDATE_RELEASE,
        &sources,
        &signing_key,
    )?;
    let stable_state =
        activate_concept_registry_release(&registry_root, STABLE_RELEASE, &public_key)?;

    let label_drill = truncated_index_drill(
        &registry_root,
        &obr_path,
        "concepts.obr.labels.idx",
        ".labels.idx",
        &public_key,
    )?;
    let ccid_drill = truncated_index_drill(
        &registry_root,
        &obr_path,
        "concepts.obr.ccids.idx",
        ".ccids.idx",
        &public_key,
    )?;

    let measured_capacity =
        concept_registry_release_capacity(&obr_path, &sbom_path, &registry_root)?;
    let disk_error = package_concept_registry_release_with_capacity_for_drill(
        &obr_path,
        &sbom_path,
        &registry_root,
        ConceptRegistryReleasePackageInput {
            release_id: DISK_SHORTAGE_RELEASE.to_owned(),
            sources: sources.clone(),
        },
        &signing_key,
        0,
    )
    .expect_err("zero simulated capacity must reject release publication");
    let (required_bytes, available_bytes, disk_rejected) = match &disk_error {
        ConceptRegistryReleaseError::InsufficientSpace {
            required,
            available,
        } => (*required, *available, true),
        _ => (0, 0, false),
    };
    let releases_dir = registry_root.join("releases");
    let disk_final_absent = !releases_dir.join(DISK_SHORTAGE_RELEASE).exists();
    let disk_staging_absent = fs::read_dir(&releases_dir)?.all(|entry| {
        entry
            .ok()
            .and_then(|value| value.file_name().into_string().ok())
            .is_none_or(|name| !name.starts_with(&format!(".{DISK_SHORTAGE_RELEASE}.staging-")))
    });
    let active_after_disk = resolve_active_concept_registry_release(&registry_root, &public_key)?;
    let disk_active_preserved = active_after_disk.release_id == STABLE_RELEASE
        && active_after_disk.generation == stable_state.generation;
    let process_kills = process_kill_drills(
        &work_dir,
        &obr_path,
        &sbom_path,
        &sources_path,
        &private_key_path,
        &sources,
        &signing_key,
    )?;
    let process_kills_qualified = process_kills.iter().all(|drill| {
        drill.get("old_or_new_complete") == Some(&Value::Bool(true))
            && drill.get("child_was_killed") == Some(&Value::Bool(true))
    });

    let label_qualified = drill_qualified(&label_drill);
    let ccid_qualified = drill_qualified(&ccid_drill);
    let exit_oracles = json!({
        "truncated_label_index_rejected": label_qualified,
        "truncated_ccid_index_rejected": ccid_qualified,
        "disk_shortage_rejected_before_publication": disk_rejected
            && available_bytes == 0
            && required_bytes > 0,
        "disk_shortage_left_no_final_release": disk_final_absent,
        "disk_shortage_left_no_staging_directory": disk_staging_absent,
        "active_release_survived_every_failure": disk_active_preserved,
        "all_process_kills_reopened_complete_state": process_kills_qualified,
    });
    let qualified = exit_oracles
        .as_object()
        .expect("exit_oracles is an object")
        .values()
        .all(|value| value == &Value::Bool(true));

    let executable_blake3 = blake3_file_hex(&std::env::current_exe()?)?;
    if binding.get("executable_blake3").and_then(Value::as_str) != Some(executable_blake3.as_str())
    {
        return Err("release binding executable_blake3 does not match this executable".into());
    }
    let mut payload = json!({
        "profile": PROFILE,
        "generated_at_unix_seconds": SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        "input": {
            "obr": artifact_evidence(&obr_path)?,
            "label_index": artifact_evidence(&append_suffix(&obr_path, ".labels.idx"))?,
            "ccid_index": artifact_evidence(&append_suffix(&obr_path, ".ccids.idx"))?,
            "manifest": artifact_evidence(&append_suffix(&obr_path, ".manifest.json"))?,
            "sbom": artifact_evidence(&sbom_path)?,
            "sources": artifact_evidence(&sources_path)?,
        },
        "measured_capacity": measured_capacity,
        "drills": {
            "truncated_label_index": label_drill,
            "truncated_ccid_index": ccid_drill,
            "disk_shortage": {
                "simulated_available_bytes": 0,
                "required_bytes": required_bytes,
                "reported_available_bytes": available_bytes,
                "error": disk_error.to_string(),
                "publication_rejected": disk_rejected,
                "final_release_absent": disk_final_absent,
                "staging_directory_absent": disk_staging_absent,
                "active_release_preserved": disk_active_preserved,
            },
            "process_kills": process_kills,
        },
        "exit_oracles": exit_oracles,
        "release_aggregate_root": candidate_stamp.artifact_root,
        "registry_generation": stable_state.generation,
        "production_profile_blake3": binding.get("production_profile_blake3").cloned().ok_or("release binding production_profile_blake3 is missing")?,
        "trust_policy_digest": derive_json(TRUST_POLICY_CONTEXT, &policy)?,
        "signer_fingerprint": signer_fingerprint(signing_key.verifying_key().as_bytes()),
        "probe_blake3": binding.get("probe_blake3").cloned().ok_or("release binding probe_blake3 is missing")?,
        "executable_blake3": executable_blake3,
        "candidate_payload_artifacts_blake3": candidate_stamp.artifacts.iter().map(|artifact| {
            (format!("{}:{}", artifact.role, artifact.relative_path), Value::String(artifact.blake3.clone()))
        }).collect::<serde_json::Map<String, Value>>(),
        "release_stamp_blake3": blake3_file_hex(&registry_root.join("releases").join(CANDIDATE_RELEASE).join("release.stamp.json"))?,
        "command": ["concept_registry_failure_qualification", "truncated-index", "disk-shortage"],
        "result": qualified,
        "production_qualified": false,
        "limitations": ["Registry-only failure evidence; never BASE-GATE-V1"],
    });
    apply_run_context(&mut payload, &context, &binding)?;
    let receipt = sign_receipt(payload, &policy, &signing_key)?;
    write_report_atomic(&output_path, &receipt)?;
    println!("{}", serde_json::to_string(&receipt)?);
    if !qualified {
        return Err("one or more failure qualification oracles failed".into());
    }
    Ok(())
}

fn run_kill_worker() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(env::var("ONEBRAIN_REGISTRY_KILL_ROOT")?);
    let registry = root.join("registry");
    let key = read_signing_key(Path::new(&env::var("ONEBRAIN_REGISTRY_KILL_PRIVATE_KEY")?))?;
    match env::var("ONEBRAIN_REGISTRY_KILL_OPERATION")?.as_str() {
        "package" => {
            let sources: Vec<ConceptRegistryReleaseSource> =
                serde_json::from_slice(&fs::read(env::var("ONEBRAIN_REGISTRY_KILL_SOURCES")?)?)?;
            package(
                Path::new(&env::var("ONEBRAIN_REGISTRY_KILL_OBR")?),
                Path::new(&env::var("ONEBRAIN_REGISTRY_KILL_SBOM")?),
                &registry,
                CANDIDATE_RELEASE,
                &sources,
                &key,
            )?;
        }
        "activate" => {
            activate_concept_registry_release(&registry, CANDIDATE_RELEASE, &key.verifying_key())?;
        }
        _ => return Err("unknown process-kill worker operation".into()),
    }
    Ok(())
}

fn process_kill_drills(
    work_dir: &Path,
    obr_path: &Path,
    sbom_path: &Path,
    sources_path: &Path,
    private_key_path: &Path,
    sources: &[ConceptRegistryReleaseSource],
    signing_key: &SigningKey,
) -> Result<Vec<Value>, Box<dyn Error>> {
    let phases = [
        ("release-publication-before", "package"),
        ("release-publication-during", "package"),
        ("release-publication-after", "package"),
        ("state-append-before", "activate"),
        ("state-append-during", "activate"),
        ("state-append-after", "activate"),
    ];
    let mut receipts = Vec::with_capacity(phases.len());
    for (phase, operation) in phases {
        let temporary = tempfile::Builder::new()
            .prefix("onebrain-registry-kill-")
            .tempdir_in(work_dir)?;
        let registry = temporary.path().join("registry");
        package(
            obr_path,
            sbom_path,
            &registry,
            STABLE_RELEASE,
            sources,
            signing_key,
        )?;
        activate_concept_registry_release(&registry, STABLE_RELEASE, &signing_key.verifying_key())?;
        if operation == "activate" {
            package(
                obr_path,
                sbom_path,
                &registry,
                CANDIDATE_RELEASE,
                sources,
                signing_key,
            )?;
        }
        let marker = temporary.path().join(format!("{phase}.marker"));
        let mut child = Command::new(env::current_exe()?)
            .arg("--kill-worker")
            .env("ONEBRAIN_REGISTRY_KILL_ROOT", temporary.path())
            .env("ONEBRAIN_REGISTRY_KILL_PHASE", phase)
            .env("ONEBRAIN_REGISTRY_KILL_MARKER", &marker)
            .env("ONEBRAIN_REGISTRY_KILL_OPERATION", operation)
            .env("ONEBRAIN_REGISTRY_KILL_OBR", obr_path)
            .env("ONEBRAIN_REGISTRY_KILL_SBOM", sbom_path)
            .env("ONEBRAIN_REGISTRY_KILL_SOURCES", sources_path)
            .env("ONEBRAIN_REGISTRY_KILL_PRIVATE_KEY", private_key_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let deadline = Instant::now() + Duration::from_secs(15);
        while !marker.exists() {
            if let Some(status) = child.try_wait()? {
                return Err(format!("process-kill worker exited before {phase}: {status}").into());
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                return Err(format!("timeout waiting for process-kill marker: {phase}").into());
            }
            thread::sleep(Duration::from_millis(10));
        }
        child.kill()?;
        let status = child.wait()?;

        let active =
            resolve_active_concept_registry_release(&registry, &signing_key.verifying_key())?;
        let active_is_old_or_new = matches!(
            active.release_id.as_str(),
            STABLE_RELEASE | CANDIDATE_RELEASE
        );
        let active_complete =
            verify_concept_registry_release(&active.release_dir, &signing_key.verifying_key())
                .is_ok();
        let all_published_complete = fs::read_dir(registry.join("releases"))?
            .filter_map(Result::ok)
            .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
            .all(|entry| {
                verify_concept_registry_release(&entry.path(), &signing_key.verifying_key()).is_ok()
            });
        receipts.push(json!({
            "phase": phase,
            "operation": operation,
            "child_was_killed": !status.success(),
            "active_release": active.release_id,
            "active_generation": active.generation,
            "active_state_root": ku_core::latest_concept_registry_state(&registry)?.ok_or("active state disappeared")?.state_root,
            "old_or_new_complete": active_is_old_or_new && active_complete && all_published_complete,
        }));
    }
    Ok(receipts)
}

fn package(
    obr_path: &Path,
    sbom_path: &Path,
    registry_root: &Path,
    release_id: &str,
    sources: &[ConceptRegistryReleaseSource],
    signing_key: &SigningKey,
) -> Result<ku_core::ConceptRegistryReleaseStamp, ConceptRegistryReleaseError> {
    package_concept_registry_release(
        obr_path,
        sbom_path,
        registry_root,
        ConceptRegistryReleasePackageInput {
            release_id: release_id.to_owned(),
            sources: sources.to_vec(),
        },
        signing_key,
    )
}

fn apply_run_context(
    payload: &mut Value,
    context: &Value,
    binding: &Value,
) -> Result<(), Box<dyn Error>> {
    let object = context
        .as_object()
        .ok_or("QualificationRunContextV1 is not an object")?;
    if context.get("format").and_then(Value::as_str) != Some("onebrain/qualification-run-context/1")
    {
        return Err("QualificationRunContextV1 format is invalid".into());
    }
    let target = payload.as_object_mut().expect("payload is an object");
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
            let closure = context
                .get("closure_digest")
                .and_then(Value::as_str)
                .ok_or("Prequalification closure_digest is missing")?;
            validate_lower_hex(closure, 64)?;
            target.insert(
                "qualification_context_variant".to_owned(),
                json!("Prequalification"),
            );
            target.insert("closure_digest".to_owned(), json!(closure));
            target.insert("base_candidate_bound".to_owned(), json!(false));
        }
        Some("Release") => {
            let expected: std::collections::BTreeSet<_> = [
                "format",
                "variant",
                "release_request_digest",
                "qualification_session_id",
                "candidate_commit",
                "candidate_tree",
            ]
            .into_iter()
            .collect();
            if object
                .keys()
                .map(String::as_str)
                .collect::<std::collections::BTreeSet<_>>()
                != expected
            {
                return Err("Release context fields are not closed".into());
            }
            validate_lower_hex(
                context
                    .get("release_request_digest")
                    .and_then(Value::as_str)
                    .ok_or("release_request_digest is missing")?,
                64,
            )?;
            for field in ["candidate_commit", "candidate_tree"] {
                validate_lower_hex_lengths(
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
            target.insert("qualification_context_variant".to_owned(), json!("Release"));
            for field in [
                "release_request_digest",
                "qualification_session_id",
                "candidate_commit",
                "candidate_tree",
            ] {
                target.insert(
                    field.to_owned(),
                    context
                        .get(field)
                        .cloned()
                        .ok_or("Release context field is missing")?,
                );
            }
            for field in ["candidate_semantic_digest", "artifact_tuple_digest"] {
                target.insert(
                    field.to_owned(),
                    binding
                        .get(field)
                        .cloned()
                        .ok_or("verified release binding field is missing")?,
                );
            }
            let generation = binding
                .get("registry_generation")
                .and_then(Value::as_u64)
                .ok_or("verified release registry_generation is missing")?;
            target.insert("registry_generation".to_owned(), json!(generation));
            target.insert("base_candidate_bound".to_owned(), json!(true));
        }
        _ => return Err("QualificationRunContextV1 variant is invalid".into()),
    }
    Ok(())
}

fn sign_receipt(payload: Value, policy: &Value, key: &SigningKey) -> Result<Value, Box<dyn Error>> {
    let mut receipt = json!({
        "format": "onebrain/concept-registry-qualification-receipt/1",
        "receipt_kind": "failure-qualification",
        "usage": "registry-qualification-receipt",
        "payload": payload,
        "signer_public_key": encode_hex(key.verifying_key().as_bytes()),
        "signer_fingerprint": signer_fingerprint(key.verifying_key().as_bytes()),
        "trust_policy_digest": derive_json(TRUST_POLICY_CONTEXT, policy)?,
        "signature": "",
    });
    let digest = blake3::hash(&serde_json::to_vec(&receipt)?);
    let mut message = Vec::with_capacity(RECEIPT_DOMAIN.len() + 32);
    message.extend_from_slice(RECEIPT_DOMAIN);
    message.extend_from_slice(digest.as_bytes());
    receipt["signature"] = Value::String(encode_hex(&key.sign(&message).to_bytes()));
    Ok(receipt)
}

fn validate_receipt_signer(
    policy: &Value,
    key: &SigningKey,
    binding: &Value,
) -> Result<(), Box<dyn Error>> {
    let public = encode_hex(key.verifying_key().as_bytes());
    let fingerprint = signer_fingerprint(key.verifying_key().as_bytes());
    let allowed = policy
        .get("signers")
        .and_then(Value::as_array)
        .is_some_and(|values| {
            values.iter().any(|value| {
                value.get("public_key_hex").and_then(Value::as_str) == Some(public.as_str())
                    && value.get("fingerprint_hex").and_then(Value::as_str)
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
    if binding.get("signer_fingerprint").is_some()
        && binding.get("signer_fingerprint").and_then(Value::as_str) != Some(fingerprint.as_str())
    {
        return Err("release binding signer_fingerprint mismatch".into());
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

fn blake3_file_hex(path: &Path) -> Result<String, Box<dyn Error>> {
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn validate_lower_hex(value: &str, length: usize) -> Result<(), Box<dyn Error>> {
    if value.len() != length
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("context digest is not lowercase hexadecimal".into());
    }
    Ok(())
}

fn validate_lower_hex_lengths(value: &str, lengths: &[usize]) -> Result<(), Box<dyn Error>> {
    if !lengths.contains(&value.len())
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("context identity is not lowercase hexadecimal".into());
    }
    Ok(())
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn truncated_index_drill(
    registry_root: &Path,
    obr_path: &Path,
    packaged_name: &str,
    source_suffix: &str,
    public_key: &ed25519_dalek::VerifyingKey,
) -> Result<Value, Box<dyn Error>> {
    let release_dir = registry_root.join("releases").join(CANDIDATE_RELEASE);
    let target = release_dir.join(packaged_name);
    let source = append_suffix(obr_path, source_suffix);
    let original_length = fs::metadata(&target)?.len();
    let truncated_length = original_length / 2;
    let file = OpenOptions::new().write(true).open(&target)?;
    file.set_len(truncated_length)?;
    file.sync_all()?;

    let verify_error = verify_concept_registry_release(&release_dir, public_key)
        .expect_err("truncated index must fail release verification");
    let verify_rejected = matches!(
        &verify_error,
        ConceptRegistryReleaseError::ArtifactMismatch { .. }
    );
    let activation_error =
        activate_concept_registry_release(registry_root, CANDIDATE_RELEASE, public_key)
            .expect_err("truncated index must fail before activation");
    let activation_rejected = matches!(
        &activation_error,
        ConceptRegistryReleaseError::ArtifactMismatch { .. }
    );
    let active = resolve_active_concept_registry_release(registry_root, public_key)?;
    let active_preserved = active.release_id == STABLE_RELEASE && active.generation == 1;

    fs::copy(source, &target)?;
    OpenOptions::new().write(true).open(&target)?.sync_all()?;
    verify_concept_registry_release(&release_dir, public_key)?;
    Ok(json!({
        "artifact": packaged_name,
        "original_length": original_length,
        "truncated_length": truncated_length,
        "verification_rejected": verify_rejected,
        "activation_rejected": activation_rejected,
        "active_release_preserved": active_preserved,
        "verification_error": verify_error.to_string(),
        "activation_error": activation_error.to_string(),
    }))
}

fn drill_qualified(value: &Value) -> bool {
    [
        "verification_rejected",
        "activation_rejected",
        "active_release_preserved",
    ]
    .into_iter()
    .all(|field| value.get(field) == Some(&Value::Bool(true)))
}

fn artifact_evidence(path: &Path) -> Result<Value, Box<dyn Error>> {
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(json!({
        "bytes": fs::metadata(path)?.len(),
        "blake3": hasher.finalize().to_hex().to_string(),
    }))
}

fn write_report_atomic(path: &Path, report: &Value) -> Result<(), Box<dyn Error>> {
    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let temporary = parent.join(format!(
        ".{}.{}-{}.tmp",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("report"),
        std::process::id(),
        nonce
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    serde_json::to_writer_pretty(&mut file, report)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    fs::rename(&temporary, path)?;
    sync_directory(parent)?;
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), Box<dyn Error>> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), Box<dyn Error>> {
    Ok(())
}

fn required_path(
    args: &mut impl Iterator<Item = String>,
    name: &str,
) -> Result<PathBuf, Box<dyn Error>> {
    args.next()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing {name}\n{}", usage()).into())
}

fn no_more_args(mut args: impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    if let Some(extra) = args.next() {
        return Err(format!("unexpected argument: {extra}\n{}", usage()).into());
    }
    Ok(())
}

fn read_signing_key(path: &Path) -> Result<SigningKey, Box<dyn Error>> {
    let value = fs::read_to_string(path)?;
    let value = value.trim();
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("private signing key must be exactly 64 lowercase hex digits".into());
    }
    let mut bytes = [0u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)?;
    }
    Ok(SigningKey::from_bytes(&bytes))
}

fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn usage() -> &'static str {
    "usage: concept_registry_failure_qualification WORK_DIR OBR_PATH SPDX_SBOM_PATH SOURCES_JSON_PATH PRIVATE_KEY_FILE RUN_CONTEXT_JSON RELEASE_BINDING_JSON TRUST_POLICY_JSON OUTPUT_JSON"
}
