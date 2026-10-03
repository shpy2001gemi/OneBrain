//! Deliberate operator provisioning for the existing private manual host.
//! No Registry, model, Vault write, API admission or publication authority.
use ku_core::foundation::{
    ObjectReference, ObservationGovernance, ResourceProfile, SourceArtifact, SourceArtifactKind,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{io::Read, path::PathBuf};

const CONSENT: &str = "I own or am permitted to capture this text and permit manual local encoding and private retention of its source, governance records and KU until I remove them. No publication is permitted.";
const TEXT_UNREADABLE: &str = "ku_manual_text_unreadable: cannot read text_file. Set it to an existing locally readable plain UTF-8 text file; relative paths resolve against the launch working directory, not the request JSON's directory. Keep private source text outside Git. No output created.";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    operator: String,
    text_file: PathBuf,
    output_dir: PathBuf,
    consent_local_private: bool,
}

fn digest(value: &Value) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    Ok(onebrain_base_contract::ku_payload::decode_hex(
        &ku_encoder::extraction::artifact_sha256(value)?,
    )?)
}

fn provision(request: Request) -> Result<(), Box<dyn std::error::Error>> {
    if !request.consent_local_private
        || request.operator.trim().is_empty()
        || request.operator.len() > 128
    {
        return Err("explicit local/private consent and operator identity required".into());
    }
    let mut raw = Vec::new();
    std::fs::File::open(&request.text_file)
        .map_err(|_| TEXT_UNREADABLE)?
        .take(8193)
        .read_to_end(&mut raw)
        .map_err(|_| TEXT_UNREADABLE)?;
    if raw.len() > 8192 {
        return Err("ku_manual_text_too_large: text_file exceeds 8192 bytes. Explicitly select a shorter permitted source; this command does not truncate or rewrite text. No output created.".into());
    }
    let text = std::str::from_utf8(&raw).map_err(|_| {
        "ku_manual_text_invalid: text_file must contain plain UTF-8 text, not UTF-16 or a canonical source object. Use a separate UTF-8 copy if conversion is needed; the command preserves exact accepted bytes. No output created."
    })?;
    if text.trim().is_empty() {
        return Err("ku_manual_text_empty: text_file must contain nonempty text within 8192 UTF-8 bytes; empty or whitespace-only text is rejected. No output created.".into());
    }
    let policy =
        json!({"profile":"ku-local-manual-source/1","consent":CONSENT,"destination":"LOCAL_ONLY"});
    let adapter = json!({"profile":"ku-local-manual-source-adapter/1","version":1,"media":"text/plain; charset=utf-8"});
    let scope = json!({"policy":hex(&digest(&policy)?),"source":hex(&digest(&json!(text))?),"use":"manual_local_encoding"});
    let receipt = json!({"operator":request.operator,"consent":true,"policy":hex(&digest(&policy)?),"scope":hex(&digest(&scope)?)});
    let assessment = json!({"policy":hex(&digest(&policy)?),"receipt":hex(&digest(&receipt)?),"scope":hex(&digest(&scope)?),"decision":"permitted_by_operator"});
    let frontier =
        json!({"policy":hex(&digest(&policy)?),"revocation":"operator_removes_host_admission"});
    let retention = json!({"policy":hex(&digest(&policy)?),"duration":"until_operator_removal"});
    let source = SourceArtifact {
        source_kind: SourceArtifactKind::Text,
        raw_bytes: raw,
        media_type_commitment: digest(&json!("text/plain; charset=utf-8"))?,
        capture_adapter: ObjectReference::new(0, digest(&adapter)?),
        capture_sequence: 0,
        governance: ObservationGovernance {
            consent_policy: ObjectReference::new(0, digest(&policy)?),
            consent_receipt: ObjectReference::new(0, digest(&receipt)?),
            revocation_policy: ObjectReference::new(0, digest(&frontier)?),
            retention_policy: ObjectReference::new(0, digest(&retention)?),
            capture_scope_commitment: digest(&scope)?,
            authorization_assessment_commitment: digest(&assessment)?,
            assessed_frontier: digest(&frontier)?,
        },
    };
    let (bytes, _) = source
        .to_private_object()
        .map_err(|_| "invalid private source governance")?
        .encode(ResourceProfile::ObjectV1)?;
    let records = serde_json::to_vec_pretty(
        &json!({"policy":policy,"adapter":adapter,"receipt":receipt,"scope":scope,"assessment":assessment,"frontier":frontier,"retention":retention}),
    )?;
    // The caller chooses an existing private parent; never replace prior custody.
    std::fs::create_dir(&request.output_dir).map_err(|error| match error.kind() {
        std::io::ErrorKind::AlreadyExists => {
            "ku_manual_output_exists: output_dir already exists. Existing custody is never overwritten. Retain and inspect its governance.json and source.canonical; admit an intended source only from a complete successful provisioning, or choose a new directory name under an existing private parent outside Git. Do not delete prior custody to retry."
        }
        _ => {
            "ku_manual_output_unavailable: cannot create output_dir. Choose a new directory under an existing private parent outside Git and check local write permissions; relative paths resolve against the launch working directory. The command does not create parent directories or replace existing custody."
        }
    })?;
    write_custody(&request.output_dir, &records, &bytes)?;
    println!(
        "Private manual source provisioned; explicitly configure host admission. No KU saved."
    );
    Ok(())
}

fn write_custody(
    output_dir: &std::path::Path,
    records: &[u8],
    bytes: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    // Fixed diagnostics omit OS detail and paths. A failed write may leave partial
    // bytes; retain the directory and never treat file presence as success.
    std::fs::write(output_dir.join("governance.json"), records).map_err(|_| {
        "ku_manual_governance_write_failed: cannot finish writing governance.json after output_dir creation. The directory may be incomplete; source.canonical was not written by this attempt. Retain the entire directory under private custody and inspect it locally. Do not admit this output to a host. Correct local filesystem availability/write permissions and explicitly retry with a new output_dir under an existing private parent outside Git. Do not delete prior custody to retry. Writes are not transactional; no KU saved."
    })?;
    std::fs::write(output_dir.join("source.canonical"), bytes).map_err(|_| {
        "ku_manual_source_write_failed: cannot finish writing source.canonical after governance.json was written. The directory may be incomplete, even if both files exist. Retain the entire directory under private custody and inspect it locally. Do not admit this output to a host. Correct local filesystem availability/write permissions and explicitly retry with a new output_dir under an existing private parent outside Git. Do not delete prior custody to retry. Writes are not transactional; no KU saved."
    })?;
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    onebrain_base_contract::ku_payload::hex(bytes)
}

fn load_request(path: &std::path::Path) -> Result<Request, Box<dyn std::error::Error>> {
    let unreadable = "ku_manual_request_unreadable: cannot read the operator request JSON. \
        Check the command's request path and local read permissions; relative paths resolve \
        against the launch working directory. Keep the request and private source outside Git. \
        No output created.";
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| unreadable)?
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| unreadable)?;
    if bytes.len() > 65536 {
        return Err(
            "ku_manual_request_too_large: operator request JSON exceeds 65536 bytes. \
            Keep source text in the referenced text_file, not inline in the request. \
            No output created."
                .into(),
        );
    }
    // serde's detailed error can echo operator values and unknown field names.
    // Include only fixed guidance and its numeric position, as in load_config.
    serde_json::from_slice(&bytes).map_err(|error| {
        let guidance = match error.classify() {
            serde_json::error::Category::Data => {
                "use an object with required fields operator (string), text_file (path string), \
                output_dir (path string), consent_local_private (boolean). Unknown or duplicate \
                fields are rejected; provisioning requires explicit true consent"
            }
            _ => "use complete UTF-8 JSON; escape Windows backslashes or use forward slashes",
        };
        format!(
            "ku_manual_request_invalid: {guidance} (line {}, column {}). Follow 'Provision a \
            developer-owned manual source' in docs/handoffs/2026-09-ku-obp-productization/outputs/\
            KU_WEB_001_IMPLEMENTATION.md. Retain existing custody when correcting the request. \
            No output created.",
            error.line(),
            error.column()
        )
        .into()
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: ku_manual_source <operator-request.json>")?;
    provision(load_request(std::path::Path::new(&path))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ku_core::foundation::{
        decode_knowledge_object, DisclosureClass, KnownObjectKind, SOURCE_ARTIFACT_KIND,
    };

    #[test]
    fn consent_gate_and_existing_custody_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let text_file = dir.path().join("text.txt");
        let output_dir = dir.path().join("custody");
        std::fs::write(&text_file, "Nước là chất lỏng.").unwrap();
        let request = |consent| Request {
            operator: "developer".into(),
            text_file: text_file.clone(),
            output_dir: output_dir.clone(),
            consent_local_private: consent,
        };
        assert!(provision(request(false)).is_err());
        assert!(!output_dir.exists());
        provision(request(true)).unwrap();
        let original = std::fs::read(output_dir.join("source.canonical")).unwrap();
        let original_governance = std::fs::read(output_dir.join("governance.json")).unwrap();
        let object = decode_knowledge_object(
            &original,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(SOURCE_ARTIFACT_KIND, 1)],
            &[],
        )
        .unwrap();
        assert_eq!(object.disclosure(), DisclosureClass::LocalOnly);
        let source = SourceArtifact::from_validated(&object).unwrap();
        assert_eq!(source.raw_bytes, "Nước là chất lỏng.".as_bytes());
        let records: Value =
            serde_json::from_slice(&std::fs::read(output_dir.join("governance.json")).unwrap())
                .unwrap();
        assert_eq!(
            source.governance.consent_receipt.cid,
            digest(&records["receipt"]).unwrap()
        );
        std::fs::write(&text_file, "changed").unwrap();
        let error = provision(request(true)).unwrap_err().to_string();
        assert!(error.starts_with("ku_manual_output_exists:"));
        assert!(error.contains("Do not delete prior custody"));
        assert_eq!(
            std::fs::read(output_dir.join("source.canonical")).unwrap(),
            original
        );
        assert_eq!(
            std::fs::read(output_dir.join("governance.json")).unwrap(),
            original_governance
        );
    }

    #[test]
    fn source_failures_are_safe_and_retry_preserves_exact_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let text_file = dir.path().join("PRIVATE_PATH_SENTINEL.txt");
        let output_dir = dir.path().join("custody");
        let request = || Request {
            operator: "PRIVATE_OPERATOR_SENTINEL".into(),
            text_file: text_file.clone(),
            output_dir: output_dir.clone(),
            consent_local_private: true,
        };
        let error = provision(request()).unwrap_err().to_string();
        assert!(error.starts_with("ku_manual_text_unreadable:"));
        assert!(!error.contains("PRIVATE_PATH_SENTINEL"));
        assert!(!error.contains("PRIVATE_OPERATOR_SENTINEL"));
        assert!(!output_dir.exists());
        for (bytes, code) in [
            (vec![0xff, 0xfe], "ku_manual_text_invalid:"),
            (vec![], "ku_manual_text_empty:"),
            (b" \r\n\t".to_vec(), "ku_manual_text_empty:"),
            (vec![b'x'; 8193], "ku_manual_text_too_large:"),
        ] {
            std::fs::write(&text_file, &bytes).unwrap();
            assert!(provision(request())
                .unwrap_err()
                .to_string()
                .starts_with(code));
            assert_eq!(std::fs::read(&text_file).unwrap(), bytes);
            assert!(!output_dir.exists());
        }
        // Exactly 8192 bytes, including non-ASCII text and surrounding whitespace.
        let mut text = format!("\r\n{}", "Nước".repeat(1000));
        text.push_str(&"x".repeat(8190 - text.len()));
        text.push_str("  ");
        assert_eq!(text.len(), 8192);
        std::fs::write(&text_file, &text).unwrap();
        provision(request()).unwrap();
        let object = decode_knowledge_object(
            &std::fs::read(output_dir.join("source.canonical")).unwrap(),
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(SOURCE_ARTIFACT_KIND, 1)],
            &[],
        )
        .unwrap();
        assert_eq!(object.disclosure(), DisclosureClass::LocalOnly);
        assert_eq!(
            SourceArtifact::from_validated(&object).unwrap().raw_bytes,
            text.as_bytes()
        );
        assert_eq!(std::fs::read(&text_file).unwrap(), text.as_bytes());
    }

    #[test]
    fn missing_output_parent_does_not_create_custody_or_change_source() {
        let dir = tempfile::tempdir().unwrap();
        let text_file = dir.path().join("text.txt");
        let parent = dir.path().join("private-parent");
        let output_dir = parent.join("custody");
        std::fs::write(&text_file, "permitted text").unwrap();
        let request = || Request {
            operator: "developer".into(),
            text_file: text_file.clone(),
            output_dir: output_dir.clone(),
            consent_local_private: true,
        };
        assert!(provision(request())
            .unwrap_err()
            .to_string()
            .starts_with("ku_manual_output_unavailable:"));
        assert!(!parent.exists());
        assert_eq!(std::fs::read(&text_file).unwrap(), b"permitted text");
        std::fs::create_dir(&parent).unwrap();
        provision(request()).unwrap();
        assert!(output_dir.join("source.canonical").is_file());
        assert!(output_dir.join("governance.json").is_file());
    }

    #[test]
    fn later_write_failure_retains_partial_custody_and_requires_new_destination() {
        for blocked_file in ["governance.json", "source.canonical"] {
            let dir = tempfile::tempdir().unwrap();
            let text_file = dir.path().join("PRIVATE_TEXT_PATH_SENTINEL.txt");
            let output_dir = dir.path().join("PRIVATE_OUTPUT_PATH_SENTINEL");
            let raw = b"PRIVATE_SOURCE_SENTINEL\r\n";
            std::fs::write(&text_file, raw).unwrap();
            // Synthetic obstruction after create_dir: both calls exercise real FS writes.
            std::fs::create_dir(&output_dir).unwrap();
            let obstruction = output_dir.join(blocked_file);
            std::fs::create_dir(&obstruction).unwrap();
            std::fs::write(obstruction.join("keep.txt"), b"retained custody").unwrap();
            let records = b"synthetic governance";
            let error = write_custody(&output_dir, records, b"synthetic canonical")
                .unwrap_err()
                .to_string();
            let code = if blocked_file == "governance.json" {
                "ku_manual_governance_write_failed:"
            } else {
                "ku_manual_source_write_failed:"
            };
            assert!(error.starts_with(code));
            assert!(!error.contains("PRIVATE_"));
            assert!(!error.contains("synthetic governance"));
            assert!(!error.contains("synthetic canonical"));
            assert!(error.contains("Do not admit this output"));
            assert!(error.contains("new output_dir"));
            assert!(output_dir.is_dir());
            assert_eq!(
                std::fs::read(obstruction.join("keep.txt")).unwrap(),
                b"retained custody"
            );
            if blocked_file == "governance.json" {
                assert!(!output_dir.join("source.canonical").exists());
            } else {
                assert_eq!(
                    std::fs::read(output_dir.join("governance.json")).unwrap(),
                    records
                );
            }
            let request = |output_dir| Request {
                operator: "PRIVATE_OPERATOR_SENTINEL".into(),
                text_file: text_file.clone(),
                output_dir,
                consent_local_private: true,
            };
            assert!(provision(request(output_dir.clone()))
                .unwrap_err()
                .to_string()
                .starts_with("ku_manual_output_exists:"));
            assert_eq!(
                std::fs::read(obstruction.join("keep.txt")).unwrap(),
                b"retained custody"
            );
            if blocked_file == "source.canonical" {
                assert_eq!(
                    std::fs::read(output_dir.join("governance.json")).unwrap(),
                    records
                );
            }
            let corrected = dir.path().join("new-custody");
            provision(request(corrected.clone())).unwrap();
            let object = decode_knowledge_object(
                &std::fs::read(corrected.join("source.canonical")).unwrap(),
                ResourceProfile::ObjectV1,
                &[KnownObjectKind::new(SOURCE_ARTIFACT_KIND, 1)],
                &[],
            )
            .unwrap();
            assert_eq!(object.disclosure(), DisclosureClass::LocalOnly);
            assert_eq!(
                SourceArtifact::from_validated(&object).unwrap().raw_bytes,
                raw
            );
            assert_eq!(std::fs::read(&text_file).unwrap(), raw);
            assert_eq!(
                std::fs::read(obstruction.join("keep.txt")).unwrap(),
                b"retained custody"
            );
        }
    }

    #[test]
    fn request_errors_do_not_echo_private_values_or_create_output() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("PRIVATE_REQUEST_PATH_SENTINEL.json");
        let text_file = dir.path().join("PRIVATE_TEXT_PATH_SENTINEL.txt");
        let output_dir = dir.path().join("custody");
        std::fs::write(&text_file, "PRIVATE_SOURCE_SENTINEL").unwrap();
        let valid = json!({
            "operator": "PRIVATE_OPERATOR_SENTINEL",
            "text_file": text_file,
            "output_dir": output_dir,
            "consent_local_private": true,
        });
        let mut wrong_type = valid.clone();
        wrong_type["consent_local_private"] = json!("PRIVATE_CONSENT_SENTINEL");
        let mut unknown = valid.clone();
        unknown["PRIVATE_FIELD_SENTINEL"] = json!("PRIVATE_VALUE_SENTINEL");
        let mut missing = valid;
        missing
            .as_object_mut()
            .unwrap()
            .remove("consent_local_private");
        for bytes in [
            serde_json::to_vec(&wrong_type).unwrap(),
            serde_json::to_vec(&unknown).unwrap(),
            serde_json::to_vec(&missing).unwrap(),
            b"{\"operator\":\"PRIVATE_OPERATOR_SENTINEL\",\"operator\":\"PRIVATE_DUPLICATE_SENTINEL\"}".to_vec(),
            b"{\"operator\":\"PRIVATE_OPERATOR_SENTINEL\"".to_vec(),
            vec![0xff],
        ] {
            std::fs::write(&path, &bytes).unwrap();
            let error = load_request(&path).err().unwrap().to_string();
            assert!(error.starts_with("ku_manual_request_invalid:"));
            assert!(error.contains("line ") && error.contains("column "));
            assert!(!error.contains("PRIVATE_"));
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            assert_eq!(std::fs::read(&text_file).unwrap(), b"PRIVATE_SOURCE_SENTINEL");
            assert!(!output_dir.exists());
        }
    }

    #[test]
    fn request_loading_retains_size_limit_and_explicit_consent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("PRIVATE_REQUEST_PATH_SENTINEL.json");
        let output_dir = dir.path().join("custody");
        let error = load_request(&path).err().unwrap().to_string();
        assert!(error.starts_with("ku_manual_request_unreadable:"));
        assert!(!error.contains("PRIVATE_"));
        assert!(load_request(dir.path())
            .err()
            .unwrap()
            .to_string()
            .starts_with("ku_manual_request_unreadable:"));
        let mut bytes = serde_json::to_vec(&json!({
            "operator": "developer",
            "text_file": dir.path().join("missing-text.txt"),
            "output_dir": output_dir,
            "consent_local_private": false,
        }))
        .unwrap();
        bytes.resize(65536, b' ');
        std::fs::write(&path, &bytes).unwrap();
        let request = load_request(&path).unwrap();
        assert!(!request.consent_local_private);
        assert_eq!(request.output_dir, output_dir);
        // A valid bounded request still needs consent before source access/output.
        assert!(provision(request)
            .unwrap_err()
            .to_string()
            .contains("explicit local/private consent"));
        assert!(!output_dir.exists());
        bytes.push(b' ');
        std::fs::write(&path, &bytes).unwrap();
        assert!(load_request(&path)
            .err()
            .unwrap()
            .to_string()
            .starts_with("ku_manual_request_too_large:"));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert!(!output_dir.exists());
    }
}
