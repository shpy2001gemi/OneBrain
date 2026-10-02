//! Deliberate operator provisioning for the existing private manual host.
//! No Registry, model, Vault write, API admission or publication authority.
use ku_core::foundation::{
    ObjectReference, ObservationGovernance, ResourceProfile, SourceArtifact, SourceArtifactKind,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{io::Read, path::PathBuf};

const CONSENT: &str = "I own or am permitted to capture this text and permit manual local encoding and private retention of its source, governance records and KU until I remove them. No publication is permitted.";

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
    std::fs::File::open(&request.text_file)?
        .take(8193)
        .read_to_end(&mut raw)?;
    if raw.len() > 8192 || std::str::from_utf8(&raw)?.trim().is_empty() {
        return Err("source must be 1..8192 exact UTF-8 bytes of nonempty text".into());
    }
    let policy =
        json!({"profile":"ku-local-manual-source/1","consent":CONSENT,"destination":"LOCAL_ONLY"});
    let adapter = json!({"profile":"ku-local-manual-source-adapter/1","version":1,"media":"text/plain; charset=utf-8"});
    let scope = json!({"policy":hex(&digest(&policy)?),"source":hex(&digest(&json!(std::str::from_utf8(&raw)?))?),"use":"manual_local_encoding"});
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
    std::fs::create_dir(&request.output_dir)?;
    std::fs::write(request.output_dir.join("governance.json"), records)?;
    std::fs::write(request.output_dir.join("source.canonical"), bytes)?;
    println!(
        "Private manual source provisioned; explicitly configure host admission. No KU saved."
    );
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    onebrain_base_contract::ku_payload::hex(bytes)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: ku_manual_source <operator-request.json>")?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(65537)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err("operator request exceeds limit".into());
    }
    provision(serde_json::from_slice(&bytes)?)
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
        assert!(provision(request(true)).is_err());
        assert_eq!(
            std::fs::read(output_dir.join("source.canonical")).unwrap(),
            original
        );
    }
}
