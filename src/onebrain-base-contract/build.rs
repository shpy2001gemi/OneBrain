use std::{env, fs, path::PathBuf};

const GENERATED_HEADER: &str =
    "// Generated from src/test-vectors/vnext/base-v1-runtime-interface-v1.json; DO NOT EDIT.";

fn main() {
    println!("cargo:rerun-if-changed=../test-vectors/vnext/base-v1-runtime-interface-v1.json");
    println!(
        "cargo:rerun-if-changed=../test-vectors/vnext/base-v1-runtime-interface-history-v1.json"
    );
    println!("cargo:rerun-if-changed=../../scripts/base/generate_contract.py");

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let generated = manifest_dir.join("src/generated.rs");
    let source = fs::read_to_string(&generated).unwrap_or_else(|error| {
        panic!(
            "missing generated Base contract at {}: {error}; run scripts/base/generate_contract.py",
            generated.display()
        )
    });
    if source.lines().next() != Some(GENERATED_HEADER) {
        panic!(
            "generated Base contract provenance drift at {}; run the generator",
            generated.display()
        );
    }
}
