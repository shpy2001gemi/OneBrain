use std::fs;
use std::path::{Path, PathBuf};

use onebrain_base_contract::{
    BaseCompatibilityTuple, BasePrerelease, BaseReleaseVersion, CompatibilityDigestV1,
    ProfileVersion, SourceCommitId, SourceCommitIdentity, SourceCommitSha1, StorageSchemaVersion,
    TargetTriple, ToolchainDigest, ToolchainIdentity,
};
use serde_json::Value;

const TUPLE_FIELDS: [&str; 16] = [
    "base_version",
    "base_commit",
    "canonical_schema_digest",
    "domain_registry_digest",
    "resource_registry_digest",
    "storage_schema",
    "archive_profile",
    "migration_profile",
    "registry_profile",
    "registry_profile_digest",
    "wire_session",
    "product_api",
    "c_abi",
    "feature_set_digest",
    "target_triple",
    "toolchain",
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace source root")
        .to_path_buf()
}

fn fixed_hex<const N: usize>(value: &str) -> [u8; N] {
    assert_eq!(value.len(), N * 2);
    let mut output = [0; N];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).expect("hex");
    }
    output
}

fn digest(value: &Value) -> CompatibilityDigestV1 {
    CompatibilityDigestV1(fixed_hex(value.as_str().expect("digest")))
}

fn profile(value: &Value) -> ProfileVersion {
    ProfileVersion {
        major: value["major"].as_u64().expect("major") as u16,
        minor: value["minor"].as_u64().expect("minor") as u16,
    }
}

fn frozen_tuple(document: &Value) -> BaseCompatibilityTuple {
    let value = &document["baseline"];
    let release = &value["base_version"];
    BaseCompatibilityTuple {
        base_version: BaseReleaseVersion {
            major: release["major"].as_u64().expect("base major") as u16,
            minor: release["minor"].as_u64().expect("base minor") as u16,
            patch: release["patch"].as_u64().expect("base patch") as u16,
            prerelease: release["prerelease"].as_str().map(|value| {
                BasePrerelease::try_from_string(value.to_owned()).expect("prerelease")
            }),
        },
        base_commit: SourceCommitIdentity::Known(SourceCommitId::Sha1(SourceCommitSha1(
            fixed_hex(value["base_commit"]["hex"].as_str().expect("commit")),
        ))),
        canonical_schema_digest: digest(&value["canonical_schema_digest"]),
        domain_registry_digest: digest(&value["domain_registry_digest"]),
        resource_registry_digest: digest(&value["resource_registry_digest"]),
        storage_schema: StorageSchemaVersion(
            value["storage_schema"].as_u64().expect("storage") as u32
        ),
        archive_profile: profile(&value["archive_profile"]),
        migration_profile: profile(&value["migration_profile"]),
        registry_profile: profile(&value["registry_profile"]),
        registry_profile_digest: digest(&value["registry_profile_digest"]),
        wire_session: profile(&value["wire_session"]),
        product_api: profile(&value["product_api"]),
        c_abi: profile(&value["c_abi"]),
        feature_set_digest: digest(&value["feature_set_digest"]),
        target_triple: TargetTriple::try_from_string(
            value["target_triple"].as_str().expect("target").to_owned(),
        )
        .expect("target"),
        toolchain: ToolchainIdentity::Known(ToolchainDigest(fixed_hex(
            value["toolchain"]["hex"].as_str().expect("toolchain"),
        ))),
    }
}

#[test]
fn frozen_tuple_digest_and_all_product_projections_have_one_field_set() {
    let root = workspace_root();
    let document: Value = serde_json::from_slice(
        &fs::read(root.join("src/test-vectors/vnext/base-v1-compatibility-v1.json"))
            .expect("compatibility vector"),
    )
    .expect("valid vector");
    let tuple = frozen_tuple(&document);
    assert_eq!(
        tuple.candidate_semantic_digest().0,
        fixed_hex(
            document["golden_digests"]["candidate_semantic"]
                .as_str()
                .unwrap()
        )
    );
    assert_eq!(
        tuple.artifact_tuple_digest().0,
        fixed_hex(
            document["golden_digests"]["artifact_tuple"]
                .as_str()
                .unwrap()
        )
    );

    let projections = [
        ("Rust service", "src/onebrain-node/src/base_runtime.rs"),
        ("Axum API", "src/onebrain-api/src/handlers.rs"),
        ("CLI verbose version", "src/onebrain-cli/src/cli/data.rs"),
        ("C ABI", "src/onebrain-base-abi/src/lib.rs"),
        (
            "generated TypeScript",
            "src/onebrain-base-contract/generated/typescript/base_v1.ts",
        ),
        (
            "generated Dart",
            "src/onebrain-base-contract/generated/dart/base_v1.dart",
        ),
    ];
    for (name, relative) in projections {
        let source = fs::read_to_string(root.join(relative)).expect("projection source");
        for field in TUPLE_FIELDS {
            assert!(source.contains(field), "{name} omits tuple field {field}");
        }
    }

    for relative in [
        "src/onebrain-node/src/base_runtime.rs",
        "src/onebrain-api/src/handlers.rs",
        "src/onebrain-cli/src/cli/data.rs",
        "src/onebrain-base-abi/src/lib.rs",
        "src/onebrain-base-contract/generated/typescript/base_v1.ts",
        "src/onebrain-base-contract/generated/dart/base_v1.dart",
    ] {
        let source = fs::read_to_string(root.join(relative)).expect("status projection");
        assert!(source.contains("candidate_semantic_digest"), "{relative}");
        assert!(source.contains("artifact_tuple_digest"), "{relative}");
    }
}

#[test]
fn black_box_projection_gates_are_present_in_the_candidate_workflow() {
    let root = workspace_root();
    let workflow = fs::read_to_string(root.join(".github/workflows/vnext-foundation.yml"))
        .expect("foundation workflow");
    for command in [
        "-p onebrain-node --test base_gate_integration",
        "-p onebrain-base-contract --test cross_consumer_tuple",
        "-p onebrain-api --test base_contract",
        "-p onebrain-cli --test version",
        "-p onebrain-base-abi",
        "npm test --prefix src/onebrain-base-contract/conformance/typescript",
        "dart test",
    ] {
        assert!(workflow.contains(command), "workflow omits {command}");
    }
}
