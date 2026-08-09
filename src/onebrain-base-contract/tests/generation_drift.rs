use std::{collections::BTreeMap, env, fs, path::PathBuf, process::Command};

use onebrain_base_contract::{
    ArchiveCredentialKindV1, BaseErrorCodeV1, BASE_COMMAND_DISCRIMINATORS,
    BASE_ERROR_DISCRIMINATORS, BASE_OPERATION_DISCRIMINATORS, BASE_REQUEST_DISCRIMINATORS,
    BASE_RESPONSE_DISCRIMINATORS, BASE_TOPIC_DISCRIMINATORS,
};

const GENERATED_HEADER: &str =
    "// Generated from src/test-vectors/vnext/base-v1-runtime-interface-v1.json; DO NOT EDIT.";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn idl() -> serde_json::Value {
    let path = workspace_root().join("src/test-vectors/vnext/base-v1-runtime-interface-v1.json");
    serde_json::from_slice(&fs::read(path).expect("read Base IDL")).expect("parse Base IDL")
}

fn expected_inventory(source: &str) -> BTreeMap<String, u16> {
    idl()[source]
        .as_array()
        .expect("IDL discriminator rows")
        .iter()
        .map(|row| {
            (
                row["name"].as_str().expect("name").to_owned(),
                u16::try_from(row["id"].as_u64().expect("id")).expect("u16 ID"),
            )
        })
        .collect()
}

fn actual_inventory(rows: &[(&str, u16)]) -> BTreeMap<String, u16> {
    rows.iter()
        .map(|(name, identifier)| ((*name).to_owned(), *identifier))
        .collect()
}

#[test]
fn checked_in_projections_match_the_verified_task14_baseline() {
    let root = workspace_root();
    let receipt = env::var_os("BASE_V1_IDL_BASELINE_RECEIPT")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join(".git/base-v1-idl-baseline-receipt.json"));
    let output = Command::new(env::var_os("PYTHON").unwrap_or_else(|| "python".into()))
        .arg(root.join("scripts/base/generate_contract.py"))
        .arg("--check")
        .arg("--baseline-receipt")
        .arg(receipt)
        .current_dir(&root)
        .output()
        .expect("run Base contract generator");
    assert!(
        output.status.success(),
        "generator check failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn every_projection_has_the_frozen_generated_header() {
    let root = workspace_root();
    for path in [
        "src/onebrain-base-contract/src/generated.rs",
        "src/onebrain-base-contract/generated/typescript/base_v1.ts",
        "src/onebrain-base-contract/generated/dart/base_v1.dart",
    ] {
        let first = fs::read_to_string(root.join(path))
            .expect("read projection")
            .lines()
            .next()
            .expect("generated first line")
            .to_owned();
        assert_eq!(first, GENERATED_HEADER, "provenance drift in {path}");
    }
}

#[test]
fn generated_discriminator_serialization_matches_idl_golden_rows() {
    for (source, actual) in [
        ("requests", BASE_REQUEST_DISCRIMINATORS),
        ("responses", BASE_RESPONSE_DISCRIMINATORS),
        ("errors", BASE_ERROR_DISCRIMINATORS),
        ("command_kinds", BASE_COMMAND_DISCRIMINATORS),
        ("topic_kinds", BASE_TOPIC_DISCRIMINATORS),
        ("operations", BASE_OPERATION_DISCRIMINATORS),
    ] {
        assert_eq!(actual_inventory(actual), expected_inventory(source));
        for identifier in actual.iter().map(|(_, identifier)| *identifier) {
            assert_eq!(
                identifier.to_le_bytes(),
                [identifier as u8, (identifier >> 8) as u8]
            );
        }
    }
    assert_eq!(BaseErrorCodeV1::InvalidRequest.discriminator(), 1);
    assert_eq!(BaseErrorCodeV1::InternalError.discriminator(), 13);
    assert_eq!(ArchiveCredentialKindV1::Password.discriminator(), 1);
    assert_eq!(ArchiveCredentialKindV1::RecoveryKey.discriminator(), 2);
}
