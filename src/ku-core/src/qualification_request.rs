//! Closed bridge to the owner-approved Base release-request verifier.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

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
    if verified.get("format").and_then(Value::as_str)
        != Some("onebrain/verified-qualification-context/1")
        || verified.get("production").and_then(Value::as_bool) != Some(true)
    {
        return Err("fixed verifier did not return a production closed context".to_owned());
    }
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
    if verified.get("format").and_then(Value::as_str)
        != Some("onebrain/verified-qualification-context/1")
        || verified.get("production").and_then(Value::as_bool) != Some(false)
    {
        return Err("test-only verifier returned a production context".to_owned());
    }
    Ok(verified)
}

fn candidate_verifier_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("scripts/release/verify_base_release_request.py")
}
