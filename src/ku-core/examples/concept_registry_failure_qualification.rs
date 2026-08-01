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
use std::time::{SystemTime, UNIX_EPOCH};

use ed25519_dalek::SigningKey;
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

fn main() {
    if let Err(error) = run() {
        eprintln!("concept-registry-failure-qualification: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let work_dir = required_path(&mut args, "WORK_DIR")?;
    let obr_path = required_path(&mut args, "OBR_PATH")?;
    let sbom_path = required_path(&mut args, "SPDX_SBOM_PATH")?;
    let sources_path = required_path(&mut args, "SOURCES_JSON_PATH")?;
    let private_key_path = required_path(&mut args, "PRIVATE_KEY_FILE")?;
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

    package(
        &obr_path,
        &sbom_path,
        &registry_root,
        STABLE_RELEASE,
        &sources,
        &signing_key,
    )?;
    package(
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
    });
    let qualified = exit_oracles
        .as_object()
        .expect("exit_oracles is an object")
        .values()
        .all(|value| value == &Value::Bool(true));

    let report = json!({
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
        },
        "exit_oracles": exit_oracles,
        "qualified": qualified,
        "full_registry_evidence_required": true,
        "production_qualified": false,
    });
    write_report_atomic(&output_path, &report)?;
    println!("{}", serde_json::to_string(&report)?);
    if !qualified {
        return Err("one or more failure qualification oracles failed".into());
    }
    Ok(())
}

fn package(
    obr_path: &Path,
    sbom_path: &Path,
    registry_root: &Path,
    release_id: &str,
    sources: &[ConceptRegistryReleaseSource],
    signing_key: &SigningKey,
) -> Result<(), ConceptRegistryReleaseError> {
    package_concept_registry_release(
        obr_path,
        sbom_path,
        registry_root,
        ConceptRegistryReleasePackageInput {
            release_id: release_id.to_owned(),
            sources: sources.to_vec(),
        },
        signing_key,
    )?;
    Ok(())
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
    "usage: concept_registry_failure_qualification WORK_DIR OBR_PATH SPDX_SBOM_PATH SOURCES_JSON_PATH PRIVATE_KEY_FILE OUTPUT_JSON"
}
