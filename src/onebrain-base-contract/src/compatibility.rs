use std::fmt;

use onebrain_archive::{
    ArchiveRestorePolicyV1 as ArchiveRestorePolicyAdapterV1, PortableProfileVersion,
    ProducerArtifactIdentityV1,
};
use thiserror::Error;

use crate::{
    BaseCapabilitySet, BaseCompatibilityPolicy, BaseCompatibilityTuple, BasePrerelease,
    BaseQualificationState, BaseQualifiedEvidence, BaseVersionStatus, CompatibilityDigestV1,
    MigrationVectorIdV1, SourceCommitId, SourceCommitIdentity, TargetTriple, ToolchainIdentity,
};
use crate::{BaseContractError, BoundedAscii, BoundedVec};

/// Development source version after the additive KU-RUN-001 registration.
/// Qualification remains a separate runtime state and is never implied by
/// this version value.
pub const BASE_V1_RELEASE_VERSION: crate::BaseReleaseVersion = crate::BaseReleaseVersion {
    major: 1,
    minor: 2,
    patch: 0,
    prerelease: None,
};

pub const MAX_BASE_ARCHIVE_DATASET_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const CANDIDATE_DIGEST_DOMAIN: &str = "onebrain:base:candidate-semantic:1\0";
const ARTIFACT_DIGEST_DOMAIN: &str = "onebrain:base:artifact-tuple:1\0";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BaseCompatibilityBuildError {
    #[error(transparent)]
    Contract(#[from] BaseContractError),
    #[error("{0} cannot be empty")]
    EmptyToken(&'static str),
    #[error("capability discriminator 0 is reserved")]
    ReservedCapability,
    #[error("duplicate capability discriminator: {0}")]
    DuplicateCapability(u16),
    #[error("archive restore policy does not preserve the compatibility tuple")]
    ArchiveTupleMismatch,
    #[error("archive dataset limit is zero or exceeds the frozen Base v1 limit")]
    ArchiveLimitRelaxed,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BaseQualificationError {
    #[error("qualification has already been attached")]
    AlreadyQualified,
    #[error("unknown source commit or toolchain cannot be qualified")]
    UnknownBuildIdentity,
    #[error("qualified evidence commit does not match the compatibility tuple")]
    CommitMismatch,
    #[error("qualified evidence semantic digest does not match the compatibility tuple")]
    SemanticDigestMismatch,
    #[error("qualified manifest artifact digest does not match the compatibility tuple")]
    ArtifactDigestMismatch,
    #[error("qualified evidence digest cannot be all zeroes")]
    EmptyEvidenceDigest,
}

impl<T: fmt::Debug, const MAX: usize> fmt::Debug for BoundedVec<T, MAX> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BoundedVec")
            .field("items", &self.as_slice())
            .field("maximum", &MAX)
            .finish()
    }
}

impl BasePrerelease {
    pub fn try_from_string(value: String) -> Result<Self, BaseCompatibilityBuildError> {
        if value.is_empty() {
            return Err(BaseCompatibilityBuildError::EmptyToken("BasePrerelease"));
        }
        Ok(Self(BoundedAscii::<32>::try_from_string(
            "BasePrerelease",
            value,
        )?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl TargetTriple {
    pub fn try_from_string(value: String) -> Result<Self, BaseCompatibilityBuildError> {
        if value.is_empty() {
            return Err(BaseCompatibilityBuildError::EmptyToken("TargetTriple"));
        }
        Ok(Self(BoundedAscii::<96>::try_from_string(
            "TargetTriple",
            value,
        )?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl MigrationVectorIdV1 {
    pub fn try_from_string(value: String) -> Result<Self, BaseCompatibilityBuildError> {
        if value.is_empty() {
            return Err(BaseCompatibilityBuildError::EmptyToken(
                "MigrationVectorIdV1",
            ));
        }
        Ok(Self(BoundedAscii::<64>::try_from_string(
            "MigrationVectorIdV1",
            value,
        )?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl BaseCapabilitySet {
    pub fn try_from_discriminators(
        mut values: Vec<u16>,
    ) -> Result<Self, BaseCompatibilityBuildError> {
        values.sort_unstable();
        if values.first() == Some(&0) {
            return Err(BaseCompatibilityBuildError::ReservedCapability);
        }
        if let Some(duplicate) = values.windows(2).find_map(|pair| {
            if pair[0] == pair[1] {
                Some(pair[0])
            } else {
                None
            }
        }) {
            return Err(BaseCompatibilityBuildError::DuplicateCapability(duplicate));
        }
        Ok(Self(BoundedVec::<u16, 64>::try_from_vec(
            "BaseCapabilitySet",
            values,
        )?))
    }

    pub fn as_discriminators(&self) -> &[u16] {
        self.0.as_slice()
    }
}

impl BaseCompatibilityTuple {
    pub fn candidate_semantic_digest(&self) -> CompatibilityDigestV1 {
        CompatibilityDigestV1(blake3::derive_key(
            CANDIDATE_DIGEST_DOMAIN,
            &self.canonical_bytes(false),
        ))
    }

    pub fn artifact_tuple_digest(&self) -> CompatibilityDigestV1 {
        CompatibilityDigestV1(blake3::derive_key(
            ARTIFACT_DIGEST_DOMAIN,
            &self.canonical_bytes(true),
        ))
    }

    pub fn producer_artifact_identity(&self) -> ProducerArtifactIdentityV1 {
        if matches!(self.base_commit, SourceCommitIdentity::Known(_))
            && matches!(self.toolchain, ToolchainIdentity::Known(_))
        {
            ProducerArtifactIdentityV1::Known(self.artifact_tuple_digest().0)
        } else {
            ProducerArtifactIdentityV1::Unknown
        }
    }

    pub fn unqualified_status(self) -> BaseVersionStatus {
        let candidate_semantic_digest = self.candidate_semantic_digest();
        let artifact_tuple_digest = self.artifact_tuple_digest();
        BaseVersionStatus {
            compatibility: self,
            candidate_semantic_digest,
            artifact_tuple_digest,
            qualification: BaseQualificationState::Unqualified,
        }
    }

    fn canonical_bytes(&self, include_artifact_fields: bool) -> Vec<u8> {
        let mut encoder = CanonicalEncoder::default();
        encoder.field(1, &encode_release(&self.base_version));
        encoder.field(2, &encode_commit_identity(self.base_commit));
        encoder.field(3, &self.canonical_schema_digest.0);
        encoder.field(4, &self.domain_registry_digest.0);
        encoder.field(5, &self.resource_registry_digest.0);
        encoder.field(6, &self.storage_schema.0.to_le_bytes());
        encoder.field(7, &encode_profile(self.archive_profile));
        encoder.field(8, &encode_profile(self.migration_profile));
        encoder.field(9, &encode_profile(self.registry_profile));
        encoder.field(10, &self.registry_profile_digest.0);
        encoder.field(11, &encode_profile(self.wire_session));
        encoder.field(12, &encode_profile(self.product_api));
        encoder.field(13, &encode_profile(self.c_abi));
        encoder.field(14, &self.feature_set_digest.0);
        if include_artifact_fields {
            encoder.field(15, self.target_triple.as_str().as_bytes());
            encoder.field(16, &encode_toolchain_identity(self.toolchain));
        }
        encoder.finish()
    }
}

impl BaseVersionStatus {
    pub fn attach_verified_qualification(
        mut self,
        evidence: BaseQualifiedEvidence,
        manifest_artifact_tuple_digest: CompatibilityDigestV1,
    ) -> Result<Self, BaseQualificationError> {
        if matches!(self.qualification, BaseQualificationState::Qualified(_)) {
            return Err(BaseQualificationError::AlreadyQualified);
        }
        let SourceCommitIdentity::Known(expected_commit) = self.compatibility.base_commit else {
            return Err(BaseQualificationError::UnknownBuildIdentity);
        };
        if !matches!(self.compatibility.toolchain, ToolchainIdentity::Known(_)) {
            return Err(BaseQualificationError::UnknownBuildIdentity);
        }
        if evidence.candidate_commit != expected_commit {
            return Err(BaseQualificationError::CommitMismatch);
        }
        if evidence.candidate_semantic_digest != self.candidate_semantic_digest {
            return Err(BaseQualificationError::SemanticDigestMismatch);
        }
        if manifest_artifact_tuple_digest != self.artifact_tuple_digest {
            return Err(BaseQualificationError::ArtifactDigestMismatch);
        }
        if evidence.evidence_blake3.0 == [0; 32] {
            return Err(BaseQualificationError::EmptyEvidenceDigest);
        }
        self.qualification = BaseQualificationState::Qualified(evidence);
        Ok(self)
    }
}

impl BaseCompatibilityPolicy {
    pub fn to_archive_restore_policy(
        &self,
    ) -> Result<ArchiveRestorePolicyAdapterV1, BaseCompatibilityBuildError> {
        let archive = self.archive_restore;
        if archive.canonical_schema_digest != self.current.canonical_schema_digest
            || archive.domain_registry_digest != self.current.domain_registry_digest
            || archive.resource_registry_digest != self.current.resource_registry_digest
            || archive.storage_schema != self.current.storage_schema
            || archive.archive_profile != self.current.archive_profile
            || archive.migration_profile != self.current.migration_profile
        {
            return Err(BaseCompatibilityBuildError::ArchiveTupleMismatch);
        }
        if archive.max_dataset_bytes == 0
            || archive.max_dataset_bytes > MAX_BASE_ARCHIVE_DATASET_BYTES
        {
            return Err(BaseCompatibilityBuildError::ArchiveLimitRelaxed);
        }
        Ok(ArchiveRestorePolicyAdapterV1 {
            canonical_schema_digest: archive.canonical_schema_digest.0,
            domain_registry_digest: archive.domain_registry_digest.0,
            resource_registry_digest: archive.resource_registry_digest.0,
            storage_schema_version: archive.storage_schema.0,
            archive_profile: PortableProfileVersion {
                major: archive.archive_profile.major,
                minor: archive.archive_profile.minor,
            },
            migration_profile: PortableProfileVersion {
                major: archive.migration_profile.major,
                minor: archive.migration_profile.minor,
            },
            max_dataset_bytes: archive.max_dataset_bytes,
        })
    }
}

#[derive(Default)]
struct CanonicalEncoder {
    bytes: Vec<u8>,
}

impl CanonicalEncoder {
    fn field(&mut self, identifier: u16, value: &[u8]) {
        self.bytes.extend_from_slice(&identifier.to_le_bytes());
        self.bytes.extend_from_slice(
            &u32::try_from(value.len())
                .expect("bounded compatibility field length")
                .to_le_bytes(),
        );
        self.bytes.extend_from_slice(value);
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

fn encode_release(version: &crate::BaseReleaseVersion) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(6 + 1 + 4 + 32);
    bytes.extend_from_slice(&version.major.to_le_bytes());
    bytes.extend_from_slice(&version.minor.to_le_bytes());
    bytes.extend_from_slice(&version.patch.to_le_bytes());
    match &version.prerelease {
        Some(prerelease) => {
            bytes.push(1);
            bytes.extend_from_slice(
                &u32::try_from(prerelease.as_str().len())
                    .expect("bounded prerelease length")
                    .to_le_bytes(),
            );
            bytes.extend_from_slice(prerelease.as_str().as_bytes());
        }
        None => bytes.push(0),
    }
    bytes
}

fn encode_profile(version: crate::ProfileVersion) -> [u8; 4] {
    let mut bytes = [0; 4];
    bytes[..2].copy_from_slice(&version.major.to_le_bytes());
    bytes[2..].copy_from_slice(&version.minor.to_le_bytes());
    bytes
}

fn encode_commit_identity(identity: SourceCommitIdentity) -> Vec<u8> {
    match identity {
        SourceCommitIdentity::Known(SourceCommitId::Sha1(commit)) => {
            let mut bytes = Vec::with_capacity(2 + 4 + 20);
            bytes.extend_from_slice(&[1, 1]);
            bytes.extend_from_slice(&20_u32.to_le_bytes());
            bytes.extend_from_slice(&commit.0);
            bytes
        }
        SourceCommitIdentity::Known(SourceCommitId::Sha256(commit)) => {
            let mut bytes = Vec::with_capacity(2 + 4 + 32);
            bytes.extend_from_slice(&[1, 2]);
            bytes.extend_from_slice(&32_u32.to_le_bytes());
            bytes.extend_from_slice(&commit.0);
            bytes
        }
        SourceCommitIdentity::Unknown => vec![2],
    }
}

fn encode_toolchain_identity(identity: ToolchainIdentity) -> Vec<u8> {
    match identity {
        ToolchainIdentity::Known(digest) => {
            let mut bytes = Vec::with_capacity(1 + 4 + 32);
            bytes.push(1);
            bytes.extend_from_slice(&32_u32.to_le_bytes());
            bytes.extend_from_slice(&digest.0);
            bytes
        }
        ToolchainIdentity::Unknown => vec![2],
    }
}
