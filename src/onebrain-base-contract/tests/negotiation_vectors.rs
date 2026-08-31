use onebrain_archive::{PortableDataCompatibilityV1, ProducerArtifactIdentityV1};
use onebrain_base_contract::{
    ArchiveRestorePolicyV1, BaseCapabilityRequirements, BaseCapabilitySet, BaseCompatibilityError,
    BaseCompatibilityPolicy, BaseCompatibilityTuple, BaseNegotiationOutcome, BasePrerelease,
    BaseQualificationState, BaseQualifiedEvidence, BaseReleaseVersion, CompatibilityDigestV1,
    MigrationVectorBindingV1, MigrationVectorIdV1, NegotiatedVersions, ProfileVersion,
    SourceCommitId, SourceCommitIdentity, SourceCommitSha1, SourceCommitSha256,
    StorageSchemaVersion, TargetTriple, ToolchainDigest, ToolchainIdentity, COMPILED_BASE_COMMIT,
    COMPILED_TARGET_TRIPLE, COMPILED_TOOLCHAIN, MAX_BASE_ARCHIVE_DATASET_BYTES,
};
use serde_json::Value;

fn vectors() -> Value {
    serde_json::from_str(include_str!(
        "../../test-vectors/vnext/base-v1-compatibility-v1.json"
    ))
    .expect("parse compatibility vectors")
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
    CompatibilityDigestV1(fixed_hex(value.as_str().expect("digest string")))
}

fn profile(value: &Value) -> ProfileVersion {
    ProfileVersion {
        major: value["major"].as_u64().expect("major") as u16,
        minor: value["minor"].as_u64().expect("minor") as u16,
    }
}

fn baseline_tuple(document: &Value) -> BaseCompatibilityTuple {
    let value = &document["baseline"];
    let release = &value["base_version"];
    BaseCompatibilityTuple {
        base_version: BaseReleaseVersion {
            major: release["major"].as_u64().expect("base major") as u16,
            minor: release["minor"].as_u64().expect("base minor") as u16,
            patch: release["patch"].as_u64().expect("base patch") as u16,
            prerelease: release["prerelease"].as_str().map(|prerelease| {
                BasePrerelease::try_from_string(prerelease.to_owned()).expect("prerelease")
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

fn policy(document: &Value, current: BaseCompatibilityTuple) -> BaseCompatibilityPolicy {
    let minimum = &document["minimum_additive"];
    BaseCompatibilityPolicy {
        archive_restore: ArchiveRestorePolicyV1 {
            canonical_schema_digest: current.canonical_schema_digest,
            domain_registry_digest: current.domain_registry_digest,
            resource_registry_digest: current.resource_registry_digest,
            storage_schema: current.storage_schema,
            archive_profile: current.archive_profile,
            migration_profile: current.migration_profile,
            max_dataset_bytes: document["archive_restore"]["max_dataset_bytes"]
                .as_u64()
                .expect("archive limit"),
        },
        minimum_additive: NegotiatedVersions {
            base_minor: minimum["base_minor"].as_u64().expect("base floor") as u16,
            wire_session_minor: minimum["wire_session_minor"].as_u64().expect("wire floor") as u16,
            product_api_minor: minimum["product_api_minor"].as_u64().expect("API floor") as u16,
            c_abi_minor: minimum["c_abi_minor"].as_u64().expect("ABI floor") as u16,
        },
        current,
    }
}

fn capability_requirements(value: &Value) -> BaseCapabilityRequirements {
    let list = |name: &str| {
        value[name]
            .as_array()
            .expect("capability list")
            .iter()
            .map(|item| item.as_u64().expect("capability") as u16)
            .collect()
    };
    BaseCapabilityRequirements {
        supported: BaseCapabilitySet::try_from_discriminators(list("supported"))
            .expect("supported capabilities"),
        required: BaseCapabilitySet::try_from_discriminators(list("required"))
            .expect("required capabilities"),
    }
}

fn migration_vector(document: &Value) -> MigrationVectorBindingV1 {
    let value = &document["migration_vector"];
    MigrationVectorBindingV1 {
        vector_id: MigrationVectorIdV1::try_from_string(
            value["vector_id"].as_str().expect("vector ID").to_owned(),
        )
        .expect("vector ID"),
        vector_blake3: digest(&value["vector_blake3"]),
        trust_policy_digest: digest(&value["trust_policy_digest"]),
    }
}

fn mutate_case(
    identifier: &str,
    tuple: &mut BaseCompatibilityTuple,
    peer_capabilities: &mut BaseCapabilityRequirements,
) {
    match identifier {
        "exact" => {}
        "base-major" => tuple.base_version.major += 1,
        "base-minor" => tuple.base_version.minor -= 1,
        "base-minor-below-floor" => tuple.base_version.minor = 1,
        "base-patch" => tuple.base_version.patch += 1,
        "base-prerelease" => {
            tuple.base_version.prerelease =
                Some(BasePrerelease::try_from_string("rc.1".to_owned()).expect("prerelease"));
        }
        "commit-known" => {
            tuple.base_commit =
                SourceCommitIdentity::Known(SourceCommitId::Sha256(SourceCommitSha256([0x12; 32])));
        }
        "commit-unknown" => tuple.base_commit = SourceCommitIdentity::Unknown,
        "canonical-schema" => tuple.canonical_schema_digest = CompatibilityDigestV1([0x41; 32]),
        "domain-registry" => tuple.domain_registry_digest = CompatibilityDigestV1([0x42; 32]),
        "resource-registry" => tuple.resource_registry_digest = CompatibilityDigestV1([0x43; 32]),
        "storage-with-vector" | "storage-without-vector" => tuple.storage_schema.0 -= 1,
        "archive-profile" | "archive-profile-without-vector" => {
            tuple.archive_profile.minor += 1;
        }
        "migration-profile" | "migration-profile-without-vector" => {
            tuple.migration_profile.minor -= 1;
        }
        "registry-profile" => tuple.registry_profile.minor += 1,
        "registry-profile-digest" => {
            tuple.registry_profile_digest = CompatibilityDigestV1([0x44; 32]);
        }
        "wire-major" => tuple.wire_session.major += 1,
        "wire-minor" => tuple.wire_session.minor -= 1,
        "wire-minor-below-floor" => tuple.wire_session.minor = 2,
        "product-major" => tuple.product_api.major += 1,
        "product-minor" => tuple.product_api.minor += 1,
        "product-minor-below-floor" => tuple.product_api.minor = 0,
        "c-abi-major" => tuple.c_abi.major += 1,
        "c-abi-minor" => tuple.c_abi.minor -= 1,
        "c-abi-minor-below-floor" => tuple.c_abi.minor = 0,
        "optional-feature" => tuple.feature_set_digest = CompatibilityDigestV1([0x45; 32]),
        "required-feature" => {
            tuple.feature_set_digest = CompatibilityDigestV1([0x46; 32]);
            *peer_capabilities = BaseCapabilityRequirements {
                supported: BaseCapabilitySet::try_from_discriminators(vec![1, 2, 4, 9])
                    .expect("peer supported"),
                required: BaseCapabilitySet::try_from_discriminators(vec![9])
                    .expect("peer required"),
            };
        }
        "target" => {
            tuple.target_triple =
                TargetTriple::try_from_string("aarch64-apple-darwin".to_owned()).expect("target");
        }
        "toolchain-known" => {
            tuple.toolchain = ToolchainIdentity::Known(ToolchainDigest([0x47; 32]));
        }
        "toolchain-unknown" => tuple.toolchain = ToolchainIdentity::Unknown,
        "commit-toolchain-unknown" => {
            tuple.base_commit = SourceCommitIdentity::Unknown;
            tuple.toolchain = ToolchainIdentity::Unknown;
        }
        other => panic!("unknown vector case: {other}"),
    }
}

fn error_name(error: BaseCompatibilityError) -> &'static str {
    match error {
        BaseCompatibilityError::BaseMajorMismatch => "BaseMajorMismatch",
        BaseCompatibilityError::BaseMinorBelowMinimum => "BaseMinorBelowMinimum",
        BaseCompatibilityError::CanonicalSchemaMismatch => "CanonicalSchemaMismatch",
        BaseCompatibilityError::DomainRegistryMismatch => "DomainRegistryMismatch",
        BaseCompatibilityError::ResourceRegistryMismatch => "ResourceRegistryMismatch",
        BaseCompatibilityError::RegistryProfileMismatch => "RegistryProfileMismatch",
        BaseCompatibilityError::RegistryProfileDigestMismatch => "RegistryProfileDigestMismatch",
        BaseCompatibilityError::WireSessionMajorMismatch => "WireSessionMajorMismatch",
        BaseCompatibilityError::WireSessionMinorBelowMinimum => "WireSessionMinorBelowMinimum",
        BaseCompatibilityError::ProductApiMajorMismatch => "ProductApiMajorMismatch",
        BaseCompatibilityError::ProductApiMinorBelowMinimum => "ProductApiMinorBelowMinimum",
        BaseCompatibilityError::CAbiMajorMismatch => "CAbiMajorMismatch",
        BaseCompatibilityError::CAbiMinorBelowMinimum => "CAbiMinorBelowMinimum",
        BaseCompatibilityError::MigrationVectorRequired => "MigrationVectorRequired",
        BaseCompatibilityError::MissingRequiredCapability => "MissingRequiredCapability",
        BaseCompatibilityError::InvalidPolicy => "InvalidPolicy",
    }
}

fn hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn frozen_golden_digests_are_stable_and_domain_separated() {
    let document = vectors();
    let tuple = baseline_tuple(&document);
    assert_eq!(
        hex(tuple.candidate_semantic_digest().0),
        document["golden_digests"]["candidate_semantic"]
            .as_str()
            .expect("semantic golden")
    );
    assert_eq!(
        hex(tuple.artifact_tuple_digest().0),
        document["golden_digests"]["artifact_tuple"]
            .as_str()
            .expect("artifact golden")
    );
    assert_ne!(
        tuple.candidate_semantic_digest(),
        tuple.artifact_tuple_digest()
    );
}

#[test]
fn every_tuple_field_follows_the_frozen_decision_table() {
    let document = vectors();
    let baseline = baseline_tuple(&document);
    let baseline_semantic = baseline.candidate_semantic_digest();
    let baseline_artifact = baseline.artifact_tuple_digest();
    let local = capability_requirements(&document["capabilities"]["local"]);

    for case in document["cases"].as_array().expect("cases") {
        let identifier = case["id"].as_str().expect("case ID");
        let mut peer = baseline.clone();
        let mut peer_capabilities = capability_requirements(&document["capabilities"]["peer"]);
        mutate_case(identifier, &mut peer, &mut peer_capabilities);
        assert_eq!(
            peer.candidate_semantic_digest() != baseline_semantic,
            case["semantic_digest_changed"]
                .as_bool()
                .expect("semantic change"),
            "semantic digest classification: {identifier}"
        );
        assert_eq!(
            peer.artifact_tuple_digest() != baseline_artifact,
            case["artifact_digest_changed"]
                .as_bool()
                .expect("artifact change"),
            "artifact digest classification: {identifier}"
        );

        let expected = case["outcome"].as_str().expect("outcome");
        let migration = case["migration_vector"]
            .as_bool()
            .expect("migration flag")
            .then(|| migration_vector(&document));
        let outcome = policy(&document, baseline.clone()).negotiate(
            &peer,
            &local,
            &peer_capabilities,
            migration,
        );
        match (expected, outcome) {
            ("compatible", BaseNegotiationOutcome::Compatible(compatible)) => {
                let expected_intersection = document["capabilities"]["expected_intersection"]
                    .as_array()
                    .expect("intersection")
                    .iter()
                    .map(|value| value.as_u64().expect("capability") as u16)
                    .collect::<Vec<_>>();
                assert_eq!(
                    compatible.capabilities.as_discriminators(),
                    expected_intersection,
                    "capability intersection: {identifier}"
                );
                assert_eq!(
                    compatible.versions.base_minor,
                    baseline.base_version.minor.min(peer.base_version.minor)
                );
                assert_eq!(
                    compatible.versions.wire_session_minor,
                    baseline.wire_session.minor.min(peer.wire_session.minor)
                );
                assert_eq!(
                    compatible.versions.product_api_minor,
                    baseline.product_api.minor.min(peer.product_api.minor)
                );
                assert_eq!(
                    compatible.versions.c_abi_minor,
                    baseline.c_abi.minor.min(peer.c_abi.minor)
                );
            }
            ("migration_required", BaseNegotiationOutcome::MigrationRequired(required)) => {
                assert_eq!(required.from, peer.base_version);
                assert_eq!(required.to, baseline.base_version);
                assert_eq!(required.vector, migration_vector(&document));
            }
            (expected, BaseNegotiationOutcome::Incompatible(error))
                if expected.strip_prefix("incompatible:") == Some(error_name(error)) => {}
            (expected, actual) => panic!(
                "unexpected negotiation outcome for {identifier}: expected {expected}, discriminator {}",
                actual.discriminator()
            ),
        }

        let status = peer.clone().unqualified_status();
        let evidence = match peer.base_commit {
            SourceCommitIdentity::Known(candidate_commit) => BaseQualifiedEvidence {
                candidate_commit,
                candidate_semantic_digest: status.candidate_semantic_digest,
                evidence_blake3: CompatibilityDigestV1([0x55; 32]),
            },
            SourceCommitIdentity::Unknown => BaseQualifiedEvidence {
                candidate_commit: SourceCommitId::Sha1(SourceCommitSha1([0; 20])),
                candidate_semantic_digest: status.candidate_semantic_digest,
                evidence_blake3: CompatibilityDigestV1([0x55; 32]),
            },
        };
        let qualification =
            status.attach_verified_qualification(evidence, peer.artifact_tuple_digest());
        let eligible = case["qualification"].as_str().expect("qualification") == "eligible";
        assert_eq!(
            qualification.is_ok(),
            eligible,
            "qualification: {identifier}"
        );
        assert_eq!(
            matches!(
                peer.producer_artifact_identity(),
                ProducerArtifactIdentityV1::Known(_)
            ),
            eligible,
            "producer identity: {identifier}"
        );
    }
}

#[test]
fn archive_adapter_preserves_portable_fields_and_never_relaxes_limits() {
    let document = vectors();
    let tuple = baseline_tuple(&document);
    let mut policy = policy(&document, tuple.clone());
    let adapter = policy.to_archive_restore_policy().expect("archive adapter");
    assert_eq!(
        adapter.portable_data_compatibility(),
        PortableDataCompatibilityV1 {
            canonical_schema_digest: tuple.canonical_schema_digest.0,
            domain_registry_digest: tuple.domain_registry_digest.0,
            resource_registry_digest: tuple.resource_registry_digest.0,
            storage_schema_version: tuple.storage_schema.0,
            archive_profile: onebrain_archive::PortableProfileVersion {
                major: tuple.archive_profile.major,
                minor: tuple.archive_profile.minor,
            },
            migration_profile: onebrain_archive::PortableProfileVersion {
                major: tuple.migration_profile.major,
                minor: tuple.migration_profile.minor,
            },
        }
    );
    assert_eq!(adapter.max_dataset_bytes, MAX_BASE_ARCHIVE_DATASET_BYTES);

    policy.archive_restore.max_dataset_bytes = MAX_BASE_ARCHIVE_DATASET_BYTES + 1;
    assert!(policy.to_archive_restore_policy().is_err());
    policy.archive_restore.max_dataset_bytes = MAX_BASE_ARCHIVE_DATASET_BYTES;
    policy.archive_restore.storage_schema.0 += 1;
    assert!(policy.to_archive_restore_policy().is_err());
}

#[test]
fn qualification_is_external_and_cannot_change_tuple_digests() {
    let document = vectors();
    let tuple = baseline_tuple(&document);
    let semantic = tuple.candidate_semantic_digest();
    let artifact = tuple.artifact_tuple_digest();
    let SourceCommitIdentity::Known(candidate_commit) = tuple.base_commit else {
        panic!("known vector commit")
    };
    let status = tuple.unqualified_status();
    let qualified = status
        .attach_verified_qualification(
            BaseQualifiedEvidence {
                candidate_commit,
                candidate_semantic_digest: semantic,
                evidence_blake3: CompatibilityDigestV1([0x61; 32]),
            },
            artifact,
        )
        .expect("qualified status");
    assert!(matches!(
        qualified.qualification,
        BaseQualificationState::Qualified(_)
    ));
    assert_eq!(
        qualified.compatibility.candidate_semantic_digest(),
        semantic
    );
    assert_eq!(qualified.compatibility.artifact_tuple_digest(), artifact);
    assert!(qualified
        .clone()
        .attach_verified_qualification(
            BaseQualifiedEvidence {
                candidate_commit,
                candidate_semantic_digest: semantic,
                evidence_blake3: CompatibilityDigestV1([0x62; 32]),
            },
            artifact,
        )
        .is_err());
}

#[test]
fn capability_and_ascii_constructors_fail_closed() {
    assert!(BaseCapabilitySet::try_from_discriminators(vec![0]).is_err());
    assert!(BaseCapabilitySet::try_from_discriminators(vec![1, 1]).is_err());
    assert!(BaseCapabilitySet::try_from_discriminators((1..=65).collect()).is_err());
    assert!(TargetTriple::try_from_string(String::new()).is_err());
    assert!(TargetTriple::try_from_string("é".to_owned()).is_err());
    assert!(MigrationVectorIdV1::try_from_string(String::new()).is_err());
}

#[test]
fn invalid_policy_and_untrusted_migration_fail_with_typed_reasons() {
    let document = vectors();
    let tuple = baseline_tuple(&document);
    let local = capability_requirements(&document["capabilities"]["local"]);
    let peer_capabilities = capability_requirements(&document["capabilities"]["peer"]);

    let mut invalid = policy(&document, tuple.clone());
    invalid.minimum_additive.base_minor = tuple.base_version.minor + 1;
    assert!(matches!(
        invalid.negotiate(&tuple, &local, &peer_capabilities, None),
        BaseNegotiationOutcome::Incompatible(BaseCompatibilityError::InvalidPolicy)
    ));

    let mut migration_peer = tuple.clone();
    migration_peer.storage_schema.0 -= 1;
    let mut untrusted = migration_vector(&document);
    untrusted.trust_policy_digest = CompatibilityDigestV1([0; 32]);
    assert!(matches!(
        policy(&document, tuple).negotiate(
            &migration_peer,
            &local,
            &peer_capabilities,
            Some(untrusted)
        ),
        BaseNegotiationOutcome::Incompatible(BaseCompatibilityError::MigrationVectorRequired)
    ));
}

#[test]
fn compiled_build_identity_is_typed_even_for_development_builds() {
    assert!(!COMPILED_TARGET_TRIPLE.is_empty());
    assert!(COMPILED_TARGET_TRIPLE.is_ascii());
    assert!(matches!(
        COMPILED_BASE_COMMIT,
        SourceCommitIdentity::Known(_) | SourceCommitIdentity::Unknown
    ));
    assert!(matches!(
        COMPILED_TOOLCHAIN,
        ToolchainIdentity::Known(_) | ToolchainIdentity::Unknown
    ));
    if std::env::var_os("ONEBRAIN_EXPECT_UNKNOWN_BUILD_IDENTITY").is_some() {
        assert!(matches!(
            COMPILED_BASE_COMMIT,
            SourceCommitIdentity::Unknown
        ));
        assert!(matches!(COMPILED_TOOLCHAIN, ToolchainIdentity::Unknown));
    }
    if std::env::var_os("ONEBRAIN_EXPECT_KNOWN_BUILD_IDENTITY").is_some() {
        assert!(matches!(
            COMPILED_BASE_COMMIT,
            SourceCommitIdentity::Known(_)
        ));
        assert!(matches!(COMPILED_TOOLCHAIN, ToolchainIdentity::Known(_)));
    }
    if std::env::var_os("ONEBRAIN_EXPECT_UNKNOWN_COMMIT").is_some() {
        assert!(matches!(
            COMPILED_BASE_COMMIT,
            SourceCommitIdentity::Unknown
        ));
    }
}
