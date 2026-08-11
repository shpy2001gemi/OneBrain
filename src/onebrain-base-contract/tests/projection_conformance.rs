use std::fs;
use std::path::PathBuf;

use serde_json::Value;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn single_corpus_matches_the_machine_idl_discriminators() {
    let corpus: Value = serde_json::from_slice(
        &fs::read(root().join("conformance/corpus.json")).expect("conformance corpus"),
    )
    .expect("valid corpus");
    let idl: Value = serde_json::from_slice(
        &fs::read(root().join("../test-vectors/vnext/base-v1-runtime-interface-v1.json"))
            .expect("machine IDL"),
    )
    .expect("valid IDL");
    let request_ids = idl["requests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_u64().unwrap())
        .collect::<Vec<_>>();
    for group in ["ordinary", "management"] {
        for item in corpus[group].as_array().unwrap() {
            assert!(request_ids.contains(&item["id"].as_u64().unwrap()));
        }
    }
    assert_eq!(corpus["errors"], idl["errors"]);
    assert_eq!(corpus["lifecycle"], idl["runtime_lifecycle"]["states"]);
}

#[test]
fn every_generated_language_contains_each_corpus_variant() {
    let corpus: Value = serde_json::from_slice(
        &fs::read(root().join("conformance/corpus.json")).expect("conformance corpus"),
    )
    .expect("valid corpus");
    let typescript = fs::read_to_string(root().join("generated/typescript/base_v1.ts")).unwrap();
    let dart = fs::read_to_string(root().join("generated/dart/base_v1.dart")).unwrap();
    for group in ["ordinary", "management"] {
        for item in corpus[group].as_array().unwrap() {
            let name = item["name"].as_str().unwrap();
            let id = item["id"].as_u64().unwrap().to_string();
            assert!(
                typescript.contains(&format!("kind: {id}")),
                "TS missing {name}"
            );
            assert!(
                dart.contains(&format!("super({id})")),
                "Dart missing {name}"
            );
        }
    }
}

#[test]
fn dart_harness_is_a_pre_gate_artifact_outside_mobile() {
    let path = root().join("conformance/dart/test/base_v1_test.dart");
    let normalized = path.to_string_lossy().replace('\\', "/");
    assert!(!normalized.contains("onebrain-mobile"));
    let source = fs::read_to_string(path).unwrap();
    assert!(!source.contains("package:flutter"));
    assert!(source.contains("generated/dart/base_v1.dart"));
}
