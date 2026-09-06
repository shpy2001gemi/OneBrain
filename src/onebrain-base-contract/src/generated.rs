// Generated from src/test-vectors/vnext/base-v1-runtime-interface-v1.json; DO NOT EDIT.
use crate::operation::{BoundedAscii, BoundedBytes, BoundedVec, SecretBytes};

pub const BASE_RUNTIME_PROFILE_MAJOR: u16 = 1;
pub const BASE_RUNTIME_PROFILE_MINOR: u16 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseCapabilitySet(pub(crate) BoundedVec<u16, 64>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BasePrerelease(pub(crate) BoundedAscii<32>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilitySetV1(pub(crate) BoundedVec<u16, 64>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompatibilityDigestV1(pub [u8; 32]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DatasetGenerationV1(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventCursorV1(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdempotencyKeyV1(pub [u8; 32]);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LimitationCodeV1(pub(crate) BoundedAscii<128>);

pub struct ManagementHandleV1(pub(crate) [u8; 32]);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationVectorIdV1(pub(crate) BoundedAscii<64>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpaqueContinuationV1(pub(crate) BoundedBytes<4096>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperationIdV1(pub [u8; 32]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperationReservationIdV1(pub [u8; 32]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessGenerationV1(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileMajorV1(pub u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileMinorV1(pub u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestIdV1(pub [u8; 32]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageSchemaVersion(pub u32);

pub struct SubscriptionHandleV1(pub(crate) [u8; 32]);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetTriple(pub(crate) BoundedAscii<96>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypedPayloadV1(pub(crate) BoundedBytes<1048576>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorRootPublicIdV1(pub [u8; 32]);

pub struct ArchiveCapabilityHandleV1(pub(crate) [u8; 32]);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveChunkV1(pub(crate) BoundedBytes<1048576>);

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveCredentialKindV1 {
    Password = 1,
    RecoveryKey = 2,
}

impl ArchiveCredentialKindV1 {
    pub const fn discriminator(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArchiveRestorePolicyV1 {
    pub canonical_schema_digest: CompatibilityDigestV1,
    pub domain_registry_digest: CompatibilityDigestV1,
    pub resource_registry_digest: CompatibilityDigestV1,
    pub storage_schema: StorageSchemaVersion,
    pub archive_profile: ProfileVersion,
    pub migration_profile: ProfileVersion,
    pub max_dataset_bytes: u64,
}

pub struct ArchiveSecretHandleV1(pub(crate) [u8; 32]);

pub struct ArchiveSinkBeginV1 {
    pub reservation_id: BaseOperationReservationId,
    pub max_total_bytes: u64,
}

pub struct ArchiveSinkHandleV1(pub(crate) [u8; 32]);

pub struct ArchiveSinkReadV1 {
    pub handle: ArchiveSinkHandleV1,
    pub offset: u64,
    pub max_bytes: u32,
}

pub struct ArchiveSourceBeginV1 {
    pub reservation_id: BaseOperationReservationId,
    pub declared_total_bytes: u64,
}

pub struct ArchiveSourceHandleV1(pub(crate) [u8; 32]);

pub struct ArchiveSourcePushV1 {
    pub handle: ArchiveSourceHandleV1,
    pub offset: u64,
    pub chunk: ArchiveChunkV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseCapabilityRequirements {
    pub supported: BaseCapabilitySet,
    pub required: BaseCapabilitySet,
}

pub enum BaseCommandV1 {
    ExistingLocalCommand(BaseLocalCommandV1),
    CreateArchive(CreateArchiveCommandV1),
    RestoreArchive(RestoreArchiveCommandV1),
}

impl BaseCommandV1 {
    pub const fn discriminator(&self) -> u16 {
        match self {
            Self::ExistingLocalCommand(..) => 1,
            Self::CreateArchive(..) => 2,
            Self::RestoreArchive(..) => 3,
        }
    }
}

#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaseCompatibilityError {
    BaseMajorMismatch = 1,
    BaseMinorBelowMinimum = 2,
    CanonicalSchemaMismatch = 3,
    DomainRegistryMismatch = 4,
    ResourceRegistryMismatch = 5,
    RegistryProfileMismatch = 6,
    RegistryProfileDigestMismatch = 7,
    WireSessionMajorMismatch = 8,
    WireSessionMinorBelowMinimum = 9,
    ProductApiMajorMismatch = 10,
    ProductApiMinorBelowMinimum = 11,
    CAbiMajorMismatch = 12,
    CAbiMinorBelowMinimum = 13,
    MigrationVectorRequired = 14,
    MissingRequiredCapability = 15,
    InvalidPolicy = 16,
}

impl BaseCompatibilityError {
    pub const fn discriminator(self) -> u16 {
        self as u16
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseCompatibilityPolicy {
    pub current: BaseCompatibilityTuple,
    pub minimum_additive: NegotiatedVersions,
    pub archive_restore: ArchiveRestorePolicyV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseCompatibilityTuple {
    pub base_version: BaseReleaseVersion,
    pub base_commit: SourceCommitIdentity,
    pub canonical_schema_digest: CompatibilityDigestV1,
    pub domain_registry_digest: CompatibilityDigestV1,
    pub resource_registry_digest: CompatibilityDigestV1,
    pub storage_schema: StorageSchemaVersion,
    pub archive_profile: ProfileVersion,
    pub migration_profile: ProfileVersion,
    pub registry_profile: ProfileVersion,
    pub registry_profile_digest: CompatibilityDigestV1,
    pub wire_session: ProfileVersion,
    pub product_api: ProfileVersion,
    pub c_abi: ProfileVersion,
    pub feature_set_digest: CompatibilityDigestV1,
    pub target_triple: TargetTriple,
    pub toolchain: ToolchainIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseCompatibleNegotiationV1 {
    pub versions: NegotiatedVersions,
    pub capabilities: BaseCapabilitySet,
}

pub struct BaseConfirmRequestV1 {
    pub operation_id: BaseOperationId,
    pub idempotency_key: BaseIdempotencyKey,
}

#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaseErrorCodeV1 {
    InvalidRequest = 1,
    NotFound = 2,
    Conflict = 3,
    Expired = 4,
    RateLimited = 5,
    CapabilityDisabled = 6,
    DependencyUnavailable = 7,
    IncompatibleProfile = 8,
    ResourceExhausted = 9,
    CorruptState = 10,
    ReprovisionRequired = 11,
    UnknownOutcome = 12,
    InternalError = 13,
}

impl BaseErrorCodeV1 {
    pub const fn discriminator(self) -> u16 {
        self as u16
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BaseIdempotencyKey(pub [u8; 32]);

pub struct BaseLocalCommandV1 {
    pub kind: u16,
    pub payload: TypedPayloadV1,
}

pub struct BaseManagementGrantV1(pub(crate) [u8; 32]);

pub enum BaseManagementRequestV1 {
    ArchiveSourceBegin(ArchiveSourceBeginV1),
    ArchiveSourcePush(ArchiveSourcePushV1),
    ArchiveSourceSeal(ArchiveCapabilityHandleV1),
    ArchiveSinkBegin(ArchiveSinkBeginV1),
    ArchiveSinkRead(ArchiveSinkReadV1),
    ArchiveSinkCommit(ArchiveCapabilityHandleV1),
    ArchiveSecretRegister(BoundedSecretIngressV1),
    ArchiveCapabilityAbort(ArchiveCapabilityHandleV1),
    ArchiveCapabilityDestroy(ArchiveCapabilityHandleV1),
    CompleteSignerReprovision(CompleteSignerReprovisionV1),
    Close,
}

impl BaseManagementRequestV1 {
    pub const fn discriminator(&self) -> u16 {
        match self {
            Self::ArchiveSourceBegin(..) => 102,
            Self::ArchiveSourcePush(..) => 103,
            Self::ArchiveSourceSeal(..) => 104,
            Self::ArchiveSinkBegin(..) => 105,
            Self::ArchiveSinkRead(..) => 106,
            Self::ArchiveSinkCommit(..) => 107,
            Self::ArchiveSecretRegister(..) => 108,
            Self::ArchiveCapabilityAbort(..) => 109,
            Self::ArchiveCapabilityDestroy(..) => 110,
            Self::CompleteSignerReprovision(..) => 111,
            Self::Close => 112,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseMigrationRequiredNegotiationV1 {
    pub from: BaseReleaseVersion,
    pub to: BaseReleaseVersion,
    pub vector: MigrationVectorBindingV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BaseNegotiationOutcome {
    Compatible(BaseCompatibleNegotiationV1),
    MigrationRequired(BaseMigrationRequiredNegotiationV1),
    Incompatible(BaseCompatibilityError),
}

impl BaseNegotiationOutcome {
    pub const fn discriminator(&self) -> u8 {
        match self {
            Self::Compatible(..) => 1,
            Self::MigrationRequired(..) => 2,
            Self::Incompatible(..) => 3,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseOpaqueContinuation(pub(crate) BoundedBytes<4096>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BaseOperationId(pub [u8; 32]);

#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaseOperationKindV1 {
    ExistingLocalCommand = 1,
    CreateArchive = 2,
    RestoreArchive = 3,
}

impl BaseOperationKindV1 {
    pub const fn discriminator(self) -> u16 {
        self as u16
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BaseOperationReservationId(pub [u8; 32]);

pub struct BasePollEventsRequestV1 {
    pub subscription_id: BaseSubscriptionId,
    pub after_cursor: u64,
    pub max_items: u32,
}

pub struct BasePrepareRequestV1 {
    pub reservation_id: BaseOperationReservationId,
    pub command: BaseCommandV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaseQualificationState {
    Unqualified,
    Qualified(BaseQualifiedEvidence),
}

impl BaseQualificationState {
    pub const fn discriminator(&self) -> u8 {
        match self {
            Self::Unqualified => 1,
            Self::Qualified(..) => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BaseQualifiedEvidence {
    pub candidate_commit: SourceCommitId,
    pub candidate_semantic_digest: CompatibilityDigestV1,
    pub evidence_blake3: CompatibilityDigestV1,
}

pub struct BaseQueryRequestV1 {
    pub payload: TypedPayloadV1,
    pub continuation: Option<BaseOpaqueContinuation>,
    pub budget: ResourceBudgetV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseReleaseVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
    pub prerelease: Option<BasePrerelease>,
}

pub enum BaseRequestV1 {
    Status,
    Query(BaseQueryRequestV1),
    ReserveOperation(BaseOperationKindV1),
    Prepare(BasePrepareRequestV1),
    Confirm(BaseConfirmRequestV1),
    Cancel(BaseOperationId),
    Reconcile(BaseOperationId),
    Subscribe(BaseSubscriptionRequestV1),
    PollEvents(BasePollEventsRequestV1),
    CloseSubscription(BaseSubscriptionId),
    Drain,
    Close,
}

impl BaseRequestV1 {
    pub const fn discriminator(&self) -> u16 {
        match self {
            Self::Status => 3,
            Self::Query(..) => 5,
            Self::ReserveOperation(..) => 6,
            Self::Prepare(..) => 7,
            Self::Confirm(..) => 8,
            Self::Cancel(..) => 9,
            Self::Reconcile(..) => 10,
            Self::Subscribe(..) => 11,
            Self::PollEvents(..) => 12,
            Self::CloseSubscription(..) => 13,
            Self::Drain => 14,
            Self::Close => 15,
        }
    }
}

pub struct BaseSubscriptionId(pub(crate) [u8; 32]);

pub struct BaseSubscriptionRequestV1 {
    pub topic: TopicKindV1,
    pub cursor: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseVersionStatus {
    pub compatibility: BaseCompatibilityTuple,
    pub candidate_semantic_digest: CompatibilityDigestV1,
    pub artifact_tuple_digest: CompatibilityDigestV1,
    pub qualification: BaseQualificationState,
}

pub struct BoundedSecretIngressV1 {
    pub kind: ArchiveCredentialKindV1,
    pub(crate) bytes: SecretBytes<1024>,
}

pub struct CompleteSignerReprovisionV1 {
    pub domain: SignerDomainV1,
    pub expected_public_id: SignerPublicIdV1,
    pub provision_handle: SignerProvisionHandleV1,
}

pub struct CreateArchiveCommandV1 {
    pub sink: ArchiveSinkHandleV1,
    pub secret: ArchiveSecretHandleV1,
    pub budget: ResourceBudgetV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeedAuthorPublicIdV1(pub [u8; 32]);

#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KuDtoKindV1 {
    KuPrepareV1 = 19457,
    KuPreparedV1 = 19458,
    KuOperationRefV1 = 19459,
    KuSaveV1 = 19460,
    KuReceiptV1 = 19461,
    KuGetV1 = 19462,
    KuViewV1 = 19463,
    KuListV1 = 19464,
    KuSearchV1 = 19465,
    KuPageV1 = 19466,
    KuReviseV1 = 19467,
    KuExportV1 = 19468,
    KuExportViewV1 = 19469,
    KuStatusV1 = 19470,
    KuFailureV1 = 19471,
    KuStatusRequestV1 = 19472,
    KuPreparedArtifactV1 = 19473,
    KuSummaryV1 = 19474,
}

impl KuDtoKindV1 {
    pub const fn discriminator(self) -> u16 {
        self as u16
    }
}

#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KuOperationKindV1 {
    Prepare = 19201,
    Preview = 19202,
    Save = 19203,
    Get = 19204,
    List = 19205,
    Search = 19206,
    Revise = 19207,
    Export = 19208,
    Status = 19209,
    Cancel = 19210,
    Reconcile = 19211,
}

impl KuOperationKindV1 {
    pub const fn discriminator(self) -> u16 {
        self as u16
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationVectorBindingV1 {
    pub vector_id: MigrationVectorIdV1,
    pub vector_blake3: CompatibilityDigestV1,
    pub trust_policy_digest: CompatibilityDigestV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NegotiatedVersions {
    pub base_minor: u16,
    pub wire_session_minor: u16,
    pub product_api_minor: u16,
    pub c_abi_minor: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeTransportPublicIdV1(pub [u8; 32]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileVersion {
    pub major: u16,
    pub minor: u16,
}

pub struct ResourceBudgetV1 {
    pub max_items: u32,
    pub max_bytes: u64,
    pub max_work_units: u64,
}

pub struct RestoreArchiveCommandV1 {
    pub source: ArchiveSourceHandleV1,
    pub secret: ArchiveSecretHandleV1,
    pub budget: ResourceBudgetV1,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignerDomainV1 {
    NodeTransport = 1,
    ActorRoot = 2,
    FeedAuthor = 3,
}

impl SignerDomainV1 {
    pub const fn discriminator(self) -> u8 {
        self as u8
    }
}

pub struct SignerProvisionHandleV1(pub(crate) [u8; 32]);

pub enum SignerPublicIdV1 {
    NodeTransport(NodeTransportPublicIdV1),
    ActorRoot(ActorRootPublicIdV1),
    FeedAuthor(FeedAuthorPublicIdV1),
}

impl SignerPublicIdV1 {
    pub const fn discriminator(&self) -> u8 {
        match self {
            Self::NodeTransport(..) => 1,
            Self::ActorRoot(..) => 2,
            Self::FeedAuthor(..) => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceCommitId {
    Sha1(SourceCommitSha1),
    Sha256(SourceCommitSha256),
}

impl SourceCommitId {
    pub const fn discriminator(&self) -> u8 {
        match self {
            Self::Sha1(..) => 1,
            Self::Sha256(..) => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceCommitIdentity {
    Known(SourceCommitId),
    Unknown,
}

impl SourceCommitIdentity {
    pub const fn discriminator(&self) -> u8 {
        match self {
            Self::Known(..) => 1,
            Self::Unknown => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceCommitSha1(pub [u8; 20]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceCommitSha256(pub [u8; 32]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolchainDigest(pub [u8; 32]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolchainIdentity {
    Known(ToolchainDigest),
    Unknown,
}

impl ToolchainIdentity {
    pub const fn discriminator(&self) -> u8 {
        match self {
            Self::Known(..) => 1,
            Self::Unknown => 2,
        }
    }
}

#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopicKindV1 {
    RuntimeStatus = 1,
    OperationReceipts = 2,
    QueryResults = 3,
    ArchiveProgress = 4,
    Compatibility = 5,
}

impl TopicKindV1 {
    pub const fn discriminator(self) -> u16 {
        self as u16
    }
}

pub const BASE_REQUEST_DISCRIMINATORS: &[(&str, u16)] = &[
    ("OpenRequestV1", 1),
    ("NegotiateRequestV1", 2),
    ("StatusRequestV1", 3),
    ("SnapshotRequestV1", 4),
    ("QueryRequestV1", 5),
    ("ReserveOperationRequestV1", 6),
    ("PrepareRequestV1", 7),
    ("ConfirmRequestV1", 8),
    ("CancelRequestV1", 9),
    ("ReconcileRequestV1", 10),
    ("SubscribeRequestV1", 11),
    ("PollEventsRequestV1", 12),
    ("CloseSubscriptionRequestV1", 13),
    ("DrainRequestV1", 14),
    ("CloseRequestV1", 15),
    ("ManagementOpenRequestV1", 101),
    ("ArchiveSourceBeginRequestV1", 102),
    ("ArchiveSourcePushChunkRequestV1", 103),
    ("ArchiveSourceSealRequestV1", 104),
    ("ArchiveSinkBeginRequestV1", 105),
    ("ArchiveSinkReadChunkRequestV1", 106),
    ("ArchiveSinkCommitRequestV1", 107),
    ("ArchiveSecretRegisterRequestV1", 108),
    ("ArchiveCapabilityAbortRequestV1", 109),
    ("ArchiveCapabilityDestroyRequestV1", 110),
    ("CompleteSignerReprovisionRequestV1", 111),
    ("ManagementCloseRequestV1", 112),
];
pub const BASE_RESPONSE_DISCRIMINATORS: &[(&str, u16)] = &[
    ("OpenResponseV1", 1),
    ("NegotiateResponseV1", 2),
    ("StatusResponseV1", 3),
    ("SnapshotResponseV1", 4),
    ("QueryResponseV1", 5),
    ("OperationReservationV1", 6),
    ("PreparedIntentV1", 7),
    ("OperationReceiptV1", 8),
    ("CancelResponseV1", 9),
    ("ReconcileResponseV1", 10),
    ("SubscriptionHandleResponseV1", 11),
    ("EventBatchV1", 12),
    ("CloseSubscriptionResponseV1", 13),
    ("DrainResponseV1", 14),
    ("CloseResponseV1", 15),
    ("ManagementHandleResponseV1", 101),
    ("ArchiveSourceHandleResponseV1", 102),
    ("ArchiveChunkAcceptedV1", 103),
    ("ArchiveSourceSealedV1", 104),
    ("ArchiveSinkHandleResponseV1", 105),
    ("ArchiveChunkV1", 106),
    ("ArchiveSinkCommittedV1", 107),
    ("ArchiveSecretHandleResponseV1", 108),
    ("ArchiveCapabilityAbortedV1", 109),
    ("ArchiveCapabilityDestroyedV1", 110),
    ("SignerReprovisionReceiptV1", 111),
    ("ManagementCloseResponseV1", 112),
];
pub const BASE_ERROR_DISCRIMINATORS: &[(&str, u16)] = &[
    ("InvalidRequest", 1),
    ("NotFound", 2),
    ("Conflict", 3),
    ("Expired", 4),
    ("RateLimited", 5),
    ("CapabilityDisabled", 6),
    ("DependencyUnavailable", 7),
    ("IncompatibleProfile", 8),
    ("ResourceExhausted", 9),
    ("CorruptState", 10),
    ("ReprovisionRequired", 11),
    ("UnknownOutcome", 12),
    ("InternalError", 13),
];
pub const BASE_COMMAND_DISCRIMINATORS: &[(&str, u16)] = &[
    ("ExistingLocalCommand", 1),
    ("CreateArchive", 2),
    ("RestoreArchive", 3),
];
pub const BASE_TOPIC_DISCRIMINATORS: &[(&str, u16)] = &[
    ("RuntimeStatus", 1),
    ("OperationReceipts", 2),
    ("QueryResults", 3),
    ("ArchiveProgress", 4),
    ("Compatibility", 5),
];
pub const BASE_OPERATION_DISCRIMINATORS: &[(&str, u16)] = &[
    ("open", 1),
    ("negotiate", 2),
    ("status", 3),
    ("snapshot", 4),
    ("query", 5),
    ("reserve_operation", 6),
    ("prepare", 7),
    ("confirm", 8),
    ("cancel", 9),
    ("reconcile", 10),
    ("subscribe", 11),
    ("poll_events", 12),
    ("close_subscription", 13),
    ("drain", 14),
    ("close", 15),
    ("management.open", 101),
    ("management.archive_source_begin", 102),
    ("management.archive_source_push_chunk", 103),
    ("management.archive_source_seal", 104),
    ("management.archive_sink_begin", 105),
    ("management.archive_sink_read_chunk", 106),
    ("management.archive_sink_commit", 107),
    ("management.archive_secret_register", 108),
    ("management.archive_capability_abort", 109),
    ("management.archive_capability_destroy", 110),
    ("management.complete_signer_reprovision", 111),
    ("management.close", 112),
];

pub mod ku {
    #![allow(non_camel_case_types)]
    use crate::ku_payload::{ensure, validate_base64, KuPayload, KuPayloadError};
    use serde::{Deserialize, Serialize};
    pub const MINIMUM_BASE_MINOR: u16 = 2;
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    #[serde(try_from = "String", into = "String")]
    pub struct ObjectCID(pub [u8; 32]);
    impl TryFrom<String> for ObjectCID {
        type Error = KuPayloadError;
        fn try_from(value: String) -> Result<Self, Self::Error> {
            Ok(Self(crate::ku_payload::decode_hex::<32>(&value)?))
        }
    }
    impl From<ObjectCID> for String {
        fn from(value: ObjectCID) -> Self {
            crate::ku_payload::hex(&value.0)
        }
    }
    impl std::fmt::Debug for ObjectCID {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("ObjectCID([private])")
        }
    }
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    #[serde(try_from = "String", into = "String")]
    pub struct SemanticContentCID(pub [u8; 32]);
    impl TryFrom<String> for SemanticContentCID {
        type Error = KuPayloadError;
        fn try_from(value: String) -> Result<Self, Self::Error> {
            Ok(Self(crate::ku_payload::decode_hex::<32>(&value)?))
        }
    }
    impl From<SemanticContentCID> for String {
        fn from(value: SemanticContentCID) -> Self {
            crate::ku_payload::hex(&value.0)
        }
    }
    impl std::fmt::Debug for SemanticContentCID {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("SemanticContentCID([private])")
        }
    }
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    #[serde(try_from = "String", into = "String")]
    pub struct SourceArtifactCID(pub [u8; 32]);
    impl TryFrom<String> for SourceArtifactCID {
        type Error = KuPayloadError;
        fn try_from(value: String) -> Result<Self, Self::Error> {
            Ok(Self(crate::ku_payload::decode_hex::<32>(&value)?))
        }
    }
    impl From<SourceArtifactCID> for String {
        fn from(value: SourceArtifactCID) -> Self {
            crate::ku_payload::hex(&value.0)
        }
    }
    impl std::fmt::Debug for SourceArtifactCID {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("SourceArtifactCID([private])")
        }
    }
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    #[serde(try_from = "String", into = "String")]
    pub struct ReleaseRoot(pub [u8; 32]);
    impl TryFrom<String> for ReleaseRoot {
        type Error = KuPayloadError;
        fn try_from(value: String) -> Result<Self, Self::Error> {
            Ok(Self(crate::ku_payload::decode_hex::<32>(&value)?))
        }
    }
    impl From<ReleaseRoot> for String {
        fn from(value: ReleaseRoot) -> Self {
            crate::ku_payload::hex(&value.0)
        }
    }
    impl std::fmt::Debug for ReleaseRoot {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("ReleaseRoot([private])")
        }
    }
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    #[serde(try_from = "String", into = "String")]
    pub struct ImplementationCommitment(pub [u8; 32]);
    impl TryFrom<String> for ImplementationCommitment {
        type Error = KuPayloadError;
        fn try_from(value: String) -> Result<Self, Self::Error> {
            Ok(Self(crate::ku_payload::decode_hex::<32>(&value)?))
        }
    }
    impl From<ImplementationCommitment> for String {
        fn from(value: ImplementationCommitment) -> Self {
            crate::ku_payload::hex(&value.0)
        }
    }
    impl std::fmt::Debug for ImplementationCommitment {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("ImplementationCommitment([private])")
        }
    }
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    #[serde(try_from = "String", into = "String")]
    pub struct RevisionFrontier(pub [u8; 32]);
    impl TryFrom<String> for RevisionFrontier {
        type Error = KuPayloadError;
        fn try_from(value: String) -> Result<Self, Self::Error> {
            Ok(Self(crate::ku_payload::decode_hex::<32>(&value)?))
        }
    }
    impl From<RevisionFrontier> for String {
        fn from(value: RevisionFrontier) -> Self {
            crate::ku_payload::hex(&value.0)
        }
    }
    impl std::fmt::Debug for RevisionFrontier {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("RevisionFrontier([private])")
        }
    }
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    #[serde(try_from = "String", into = "String")]
    pub struct OperationId(pub [u8; 32]);
    impl TryFrom<String> for OperationId {
        type Error = KuPayloadError;
        fn try_from(value: String) -> Result<Self, Self::Error> {
            Ok(Self(crate::ku_payload::decode_hex::<32>(&value)?))
        }
    }
    impl From<OperationId> for String {
        fn from(value: OperationId) -> Self {
            crate::ku_payload::hex(&value.0)
        }
    }
    impl std::fmt::Debug for OperationId {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("OperationId([private])")
        }
    }
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    #[serde(try_from = "String", into = "String")]
    pub struct IdempotencyKey(pub [u8; 32]);
    impl TryFrom<String> for IdempotencyKey {
        type Error = KuPayloadError;
        fn try_from(value: String) -> Result<Self, Self::Error> {
            Ok(Self(crate::ku_payload::decode_hex::<32>(&value)?))
        }
    }
    impl From<IdempotencyKey> for String {
        fn from(value: IdempotencyKey) -> Self {
            crate::ku_payload::hex(&value.0)
        }
    }
    impl std::fmt::Debug for IdempotencyKey {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("IdempotencyKey([private])")
        }
    }
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    #[serde(try_from = "String", into = "String")]
    pub struct RequestId(pub [u8; 32]);
    impl TryFrom<String> for RequestId {
        type Error = KuPayloadError;
        fn try_from(value: String) -> Result<Self, Self::Error> {
            Ok(Self(crate::ku_payload::decode_hex::<32>(&value)?))
        }
    }
    impl From<RequestId> for String {
        fn from(value: RequestId) -> Self {
            crate::ku_payload::hex(&value.0)
        }
    }
    impl std::fmt::Debug for RequestId {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("RequestId([private])")
        }
    }
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    #[serde(try_from = "String", into = "String")]
    pub struct PolicyCID(pub [u8; 32]);
    impl TryFrom<String> for PolicyCID {
        type Error = KuPayloadError;
        fn try_from(value: String) -> Result<Self, Self::Error> {
            Ok(Self(crate::ku_payload::decode_hex::<32>(&value)?))
        }
    }
    impl From<PolicyCID> for String {
        fn from(value: PolicyCID) -> Self {
            crate::ku_payload::hex(&value.0)
        }
    }
    impl std::fmt::Debug for PolicyCID {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("PolicyCID([private])")
        }
    }
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    #[serde(try_from = "String", into = "String")]
    pub struct CCID(pub [u8; 16]);
    impl TryFrom<String> for CCID {
        type Error = KuPayloadError;
        fn try_from(value: String) -> Result<Self, Self::Error> {
            Ok(Self(crate::ku_payload::decode_hex::<16>(&value)?))
        }
    }
    impl From<CCID> for String {
        fn from(value: CCID) -> Self {
            crate::ku_payload::hex(&value.0)
        }
    }
    impl std::fmt::Debug for CCID {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("CCID([private])")
        }
    }
    pub type Text = String;
    pub type Limitation = String;
    pub type Continuation = String;
    pub type U64 = u64;
    pub type PageLimit = u64;
    pub type CanonicalPreview = String;
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    pub enum Disclosure {
        #[serde(rename = "LOCAL_ONLY")]
        LOCALONLY,
        #[serde(rename = "NEGOTIATED_ENCRYPTED")]
        NEGOTIATEDENCRYPTED,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    pub enum Validity {
        #[serde(rename = "ready")]
        Ready,
        #[serde(rename = "needs_resolution")]
        NeedsResolution,
        #[serde(rename = "rejected")]
        Rejected,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    pub enum Coverage {
        #[serde(rename = "local_only")]
        LocalOnly,
        #[serde(rename = "partial")]
        Partial,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    pub enum Lifecycle {
        #[serde(rename = "disabled")]
        Disabled,
        #[serde(rename = "requested")]
        Requested,
        #[serde(rename = "active")]
        Active,
        #[serde(rename = "degraded")]
        Degraded,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    pub enum BaseState {
        #[serde(rename = "reserved")]
        Reserved,
        #[serde(rename = "prepared")]
        Prepared,
        #[serde(rename = "confirming")]
        Confirming,
        #[serde(rename = "committed")]
        Committed,
        #[serde(rename = "canceled")]
        Canceled,
        #[serde(rename = "failed")]
        Failed,
        #[serde(rename = "unknown_outcome")]
        UnknownOutcome,
    }
    pub type False = bool;
    pub type Boolean = bool;
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    pub enum BaseError {
        #[serde(rename = "InvalidRequest")]
        InvalidRequest,
        #[serde(rename = "NotFound")]
        NotFound,
        #[serde(rename = "Conflict")]
        Conflict,
        #[serde(rename = "Expired")]
        Expired,
        #[serde(rename = "RateLimited")]
        RateLimited,
        #[serde(rename = "CapabilityDisabled")]
        CapabilityDisabled,
        #[serde(rename = "DependencyUnavailable")]
        DependencyUnavailable,
        #[serde(rename = "IncompatibleProfile")]
        IncompatibleProfile,
        #[serde(rename = "ResourceExhausted")]
        ResourceExhausted,
        #[serde(rename = "CorruptState")]
        CorruptState,
        #[serde(rename = "ReprovisionRequired")]
        ReprovisionRequired,
        #[serde(rename = "UnknownOutcome")]
        UnknownOutcome,
        #[serde(rename = "InternalError")]
        InternalError,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    pub enum ExportMode {
        #[serde(rename = "canonical_public_exchange")]
        CanonicalPublicExchange,
        #[serde(rename = "encrypted_base_archive")]
        EncryptedBaseArchive,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    pub enum InputMode {
        #[serde(rename = "resolved_semantic_draft")]
        ResolvedSemanticDraft,
        #[serde(rename = "local_rule")]
        LocalRule,
        #[serde(rename = "local_ai")]
        LocalAi,
    }
    pub type ObjectIDs = Vec<ObjectCID>;
    pub type Sources = Vec<SourceArtifactCID>;
    pub type Limitations = Vec<Limitation>;
    pub type KuViews = Vec<KuSummaryV1>;
    pub type PreparedArtifacts = Vec<KuPreparedArtifactV1>;
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    pub enum ArtifactDisclosure {
        #[serde(rename = "PUBLIC")]
        PUBLIC,
        #[serde(rename = "ROUTE_MINIMAL")]
        ROUTEMINIMAL,
        #[serde(rename = "LOCAL_ONLY")]
        LOCALONLY,
        #[serde(rename = "NEGOTIATED_ENCRYPTED")]
        NEGOTIATEDENCRYPTED,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    pub enum ArtifactValidity {
        #[serde(rename = "accepted_known")]
        AcceptedKnown,
        #[serde(rename = "accepted_opaque")]
        AcceptedOpaque,
    }
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct KuPrepareV1 {
        pub operation_id: OperationId,
        pub idempotency_key: IdempotencyKey,
        pub input_mode: InputMode,
        pub source_refs: Sources,
        pub registry_release_root: ReleaseRoot,
        pub semantic_profile: Text,
        pub implementation_commitment: ImplementationCommitment,
        pub destination: Disclosure,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "crate::ku_payload::deserialize_present"
        )]
        pub draft_ref: Option<ObjectCID>,
    }
    impl KuPayload for KuPrepareV1 {
        const DTO_ID: u16 = 19457;
        fn validate(&self) -> Result<(), KuPayloadError> {
            ensure((&self.source_refs).len() <= 256)?;
            ensure((&self.semantic_profile).len() <= 4096)?;
            ensure(
                self.semantic_profile == "ku-semantic-content/1.0" && !self.source_refs.is_empty(),
            )?;
            ensure(
                (self.input_mode == InputMode::ResolvedSemanticDraft) == self.draft_ref.is_some(),
            )?;
            Ok(())
        }
    }
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct KuPreparedV1 {
        pub operation_id: OperationId,
        pub validity: Validity,
        pub object_cids: ObjectIDs,
        pub registry_release_root: ReleaseRoot,
        pub semantic_profile: Text,
        pub destination: Disclosure,
        pub limitations: Limitations,
        pub executable: False,
        pub artifacts: PreparedArtifacts,
    }
    impl KuPayload for KuPreparedV1 {
        const DTO_ID: u16 = 19458;
        fn validate(&self) -> Result<(), KuPayloadError> {
            ensure((&self.object_cids).len() <= 256)?;
            ensure((&self.semantic_profile).len() <= 4096)?;
            ensure((&self.limitations).len() <= 64)?;
            for item_0 in &self.limitations {
                ensure(item_0.len() <= 128)?;
            }
            ensure(!*(&self.executable))?;
            ensure((&self.artifacts).len() <= 256)?;
            for item_0 in &self.artifacts {
                item_0.validate()?;
            }
            ensure(self.semantic_profile == "ku-semantic-content/1.0")?;
            if self.validity == Validity::Ready {
                ensure(
                    !self.object_cids.is_empty() && self.object_cids.len() == self.artifacts.len(),
                )?;
                let mut ids = std::collections::BTreeSet::new();
                for (id, artifact) in self.object_cids.iter().zip(&self.artifacts) {
                    ensure(*id == artifact.object_cid && ids.insert(*id))?;
                }
            } else {
                ensure(self.artifacts.is_empty() && self.object_cids.is_empty())?;
            }
            Ok(())
        }
    }
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct KuOperationRefV1 {
        pub operation_id: OperationId,
    }
    impl KuPayload for KuOperationRefV1 {
        const DTO_ID: u16 = 19459;
        fn validate(&self) -> Result<(), KuPayloadError> {
            Ok(())
        }
    }
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct KuSaveV1 {
        pub operation_id: OperationId,
        pub idempotency_key: IdempotencyKey,
        pub object_cids: ObjectIDs,
    }
    impl KuPayload for KuSaveV1 {
        const DTO_ID: u16 = 19460;
        fn validate(&self) -> Result<(), KuPayloadError> {
            ensure((&self.object_cids).len() <= 256)?;
            ensure(!self.object_cids.is_empty())?;
            Ok(())
        }
    }
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct KuReceiptV1 {
        pub operation_id: OperationId,
        pub state: BaseState,
        pub object_cids: ObjectIDs,
        pub limitations: Limitations,
        pub published: False,
        pub authorizes_reward: False,
    }
    impl KuPayload for KuReceiptV1 {
        const DTO_ID: u16 = 19461;
        fn validate(&self) -> Result<(), KuPayloadError> {
            ensure((&self.object_cids).len() <= 256)?;
            ensure((&self.limitations).len() <= 64)?;
            for item_0 in &self.limitations {
                ensure(item_0.len() <= 128)?;
            }
            ensure(!*(&self.published))?;
            ensure(!*(&self.authorizes_reward))?;
            Ok(())
        }
    }
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct KuGetV1 {
        pub object_cid: ObjectCID,
    }
    impl KuPayload for KuGetV1 {
        const DTO_ID: u16 = 19462;
        fn validate(&self) -> Result<(), KuPayloadError> {
            Ok(())
        }
    }
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct KuViewV1 {
        pub object_cid: ObjectCID,
        pub disclosure_class: ArtifactDisclosure,
        pub artifact_validity: ArtifactValidity,
        pub coverage: Coverage,
        pub limitations: Limitations,
        pub executable: False,
        pub canonical_bytes: CanonicalPreview,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "crate::ku_payload::deserialize_present"
        )]
        pub semantic_content_cid: Option<SemanticContentCID>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "crate::ku_payload::deserialize_present"
        )]
        pub fidelity_policy_cid: Option<PolicyCID>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "crate::ku_payload::deserialize_present"
        )]
        pub fidelity_frontier: Option<RevisionFrontier>,
    }
    impl KuPayload for KuViewV1 {
        const DTO_ID: u16 = 19463;
        fn validate(&self) -> Result<(), KuPayloadError> {
            ensure((&self.limitations).len() <= 64)?;
            for item_0 in &self.limitations {
                ensure(item_0.len() <= 128)?;
            }
            ensure(!*(&self.executable))?;
            validate_base64(&self.canonical_bytes, 1048576)?;
            ensure(self.fidelity_policy_cid.is_some() == self.fidelity_frontier.is_some())?;
            ensure(
                self.artifact_validity != ArtifactValidity::AcceptedOpaque
                    || self.semantic_content_cid.is_none(),
            )?;
            Ok(())
        }
    }
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct KuListV1 {
        pub limit: PageLimit,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "crate::ku_payload::deserialize_present"
        )]
        pub continuation: Option<Continuation>,
    }
    impl KuPayload for KuListV1 {
        const DTO_ID: u16 = 19464;
        fn validate(&self) -> Result<(), KuPayloadError> {
            ensure(*(&self.limit) >= 1)?;
            ensure(*(&self.limit) <= 256)?;
            if let Some(value) = &self.continuation {
                ensure(value.len() <= 2048)?;
                crate::ku_payload::validate_continuation(value)?;
            }
            Ok(())
        }
    }
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct KuSearchV1 {
        pub query: Text,
        pub limit: PageLimit,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "crate::ku_payload::deserialize_present"
        )]
        pub continuation: Option<Continuation>,
    }
    impl KuPayload for KuSearchV1 {
        const DTO_ID: u16 = 19465;
        fn validate(&self) -> Result<(), KuPayloadError> {
            ensure((&self.query).len() <= 4096)?;
            ensure(*(&self.limit) >= 1)?;
            ensure(*(&self.limit) <= 256)?;
            if let Some(value) = &self.continuation {
                ensure(value.len() <= 2048)?;
                crate::ku_payload::validate_continuation(value)?;
            }
            Ok(())
        }
    }
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct KuPageV1 {
        pub items: KuViews,
        pub coverage: Coverage,
        pub snapshot_frontier: RevisionFrontier,
        pub limitations: Limitations,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "crate::ku_payload::deserialize_present"
        )]
        pub continuation: Option<Continuation>,
    }
    impl KuPayload for KuPageV1 {
        const DTO_ID: u16 = 19466;
        fn validate(&self) -> Result<(), KuPayloadError> {
            ensure((&self.items).len() <= 256)?;
            for item_0 in &self.items {
                item_0.validate()?;
            }
            ensure((&self.limitations).len() <= 64)?;
            for item_0 in &self.limitations {
                ensure(item_0.len() <= 128)?;
            }
            if let Some(value) = &self.continuation {
                ensure(value.len() <= 2048)?;
                crate::ku_payload::validate_continuation(value)?;
            }
            Ok(())
        }
    }
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct KuReviseV1 {
        pub preparation: KuPrepareV1,
        pub predecessor_object_cid: ObjectCID,
        pub expected_revision_frontier: RevisionFrontier,
    }
    impl KuPayload for KuReviseV1 {
        const DTO_ID: u16 = 19467;
        fn validate(&self) -> Result<(), KuPayloadError> {
            (&self.preparation).validate()?;
            Ok(())
        }
    }
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct KuExportV1 {
        pub object_cids: ObjectIDs,
        pub mode: ExportMode,
    }
    impl KuPayload for KuExportV1 {
        const DTO_ID: u16 = 19468;
        fn validate(&self) -> Result<(), KuPayloadError> {
            ensure((&self.object_cids).len() <= 256)?;
            ensure(!self.object_cids.is_empty())?;
            Ok(())
        }
    }
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct KuExportViewV1 {
        pub mode: ExportMode,
        pub object_cids: ObjectIDs,
        pub limitations: Limitations,
        pub requires_base_management: Boolean,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "crate::ku_payload::deserialize_present"
        )]
        pub public_records: Option<CanonicalPreview>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "crate::ku_payload::deserialize_present"
        )]
        pub archive_operation_id: Option<OperationId>,
    }
    impl KuPayload for KuExportViewV1 {
        const DTO_ID: u16 = 19469;
        fn validate(&self) -> Result<(), KuPayloadError> {
            ensure((&self.object_cids).len() <= 256)?;
            ensure((&self.limitations).len() <= 64)?;
            for item_0 in &self.limitations {
                ensure(item_0.len() <= 128)?;
            }
            if let Some(value) = &self.public_records {
                validate_base64(value, 1048576)?;
            }
            let archive = self.mode == ExportMode::EncryptedBaseArchive;
            ensure(
                self.requires_base_management == archive
                    && self.archive_operation_id.is_some() == archive
                    && self.public_records.is_some() != archive,
            )?;
            Ok(())
        }
    }
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct KuStatusV1 {
        pub lifecycle: Lifecycle,
        pub coverage: Coverage,
        pub limitations: Limitations,
        pub registry_ready: Boolean,
        pub local_encoder_ready: Boolean,
        pub remote_encoding_enabled: False,
        pub direct_issuance_enabled: False,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "crate::ku_payload::deserialize_present"
        )]
        pub receipt: Option<KuReceiptV1>,
    }
    impl KuPayload for KuStatusV1 {
        const DTO_ID: u16 = 19470;
        fn validate(&self) -> Result<(), KuPayloadError> {
            ensure((&self.limitations).len() <= 64)?;
            for item_0 in &self.limitations {
                ensure(item_0.len() <= 128)?;
            }
            ensure(!*(&self.remote_encoding_enabled))?;
            ensure(!*(&self.direct_issuance_enabled))?;
            if let Some(value) = &self.receipt {
                value.validate()?;
            }
            Ok(())
        }
    }
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct KuFailureV1 {
        pub code: BaseError,
        pub retryable: Boolean,
        pub reconcile_before_retry: Boolean,
        pub limitations: Limitations,
    }
    impl KuPayload for KuFailureV1 {
        const DTO_ID: u16 = 19471;
        fn validate(&self) -> Result<(), KuPayloadError> {
            ensure((&self.limitations).len() <= 64)?;
            for item_0 in &self.limitations {
                ensure(item_0.len() <= 128)?;
            }
            let (retryable, reconcile) = match self.code {
                BaseError::InvalidRequest => (false, false),
                BaseError::NotFound => (false, false),
                BaseError::Conflict => (false, false),
                BaseError::Expired => (false, false),
                BaseError::RateLimited => (true, true),
                BaseError::CapabilityDisabled => (false, false),
                BaseError::DependencyUnavailable => (true, true),
                BaseError::IncompatibleProfile => (false, false),
                BaseError::ResourceExhausted => (true, true),
                BaseError::CorruptState => (false, false),
                BaseError::ReprovisionRequired => (false, false),
                BaseError::UnknownOutcome => (true, true),
                BaseError::InternalError => (false, true),
            };
            ensure(self.retryable == retryable && self.reconcile_before_retry == reconcile)?;
            Ok(())
        }
    }
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct KuStatusRequestV1 {
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "crate::ku_payload::deserialize_present"
        )]
        pub operation_id: Option<OperationId>,
    }
    impl KuPayload for KuStatusRequestV1 {
        const DTO_ID: u16 = 19472;
        fn validate(&self) -> Result<(), KuPayloadError> {
            Ok(())
        }
    }
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct KuPreparedArtifactV1 {
        pub object_cid: ObjectCID,
        pub semantic_content_cid: SemanticContentCID,
        pub canonical_preview: CanonicalPreview,
    }
    impl KuPayload for KuPreparedArtifactV1 {
        const DTO_ID: u16 = 19473;
        fn validate(&self) -> Result<(), KuPayloadError> {
            validate_base64(&self.canonical_preview, 1048576)?;
            Ok(())
        }
    }
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct KuSummaryV1 {
        pub object_cid: ObjectCID,
        pub disclosure_class: ArtifactDisclosure,
        pub artifact_validity: ArtifactValidity,
        pub coverage: Coverage,
        pub limitations: Limitations,
        pub executable: False,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "crate::ku_payload::deserialize_present"
        )]
        pub semantic_content_cid: Option<SemanticContentCID>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "crate::ku_payload::deserialize_present"
        )]
        pub fidelity_policy_cid: Option<PolicyCID>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "crate::ku_payload::deserialize_present"
        )]
        pub fidelity_frontier: Option<RevisionFrontier>,
    }
    impl KuPayload for KuSummaryV1 {
        const DTO_ID: u16 = 19474;
        fn validate(&self) -> Result<(), KuPayloadError> {
            ensure((&self.limitations).len() <= 64)?;
            for item_0 in &self.limitations {
                ensure(item_0.len() <= 128)?;
            }
            ensure(!*(&self.executable))?;
            ensure(self.fidelity_policy_cid.is_some() == self.fidelity_frontier.is_some())?;
            ensure(
                self.artifact_validity != ArtifactValidity::AcceptedOpaque
                    || self.semantic_content_cid.is_none(),
            )?;
            Ok(())
        }
    }
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum KuRequestV1 {
        Prepare(KuPrepareV1),
        Preview(KuOperationRefV1),
        Save(KuSaveV1),
        Get(KuGetV1),
        List(KuListV1),
        Search(KuSearchV1),
        Revise(KuReviseV1),
        Export(KuExportV1),
        Status(KuStatusRequestV1),
        Cancel(KuOperationRefV1),
        Reconcile(KuOperationRefV1),
    }
    impl KuRequestV1 {
        pub fn discriminator(&self) -> u16 {
            match self {
                Self::Prepare(_) => 19201,
                Self::Preview(_) => 19202,
                Self::Save(_) => 19203,
                Self::Get(_) => 19204,
                Self::List(_) => 19205,
                Self::Search(_) => 19206,
                Self::Revise(_) => 19207,
                Self::Export(_) => 19208,
                Self::Status(_) => 19209,
                Self::Cancel(_) => 19210,
                Self::Reconcile(_) => 19211,
            }
        }
        pub fn validate(&self) -> Result<(), KuPayloadError> {
            match self {
                Self::Prepare(value) => value.validate(),
                Self::Preview(value) => value.validate(),
                Self::Save(value) => value.validate(),
                Self::Get(value) => value.validate(),
                Self::List(value) => value.validate(),
                Self::Search(value) => value.validate(),
                Self::Revise(value) => value.validate(),
                Self::Export(value) => value.validate(),
                Self::Status(value) => value.validate(),
                Self::Cancel(value) => value.validate(),
                Self::Reconcile(value) => value.validate(),
            }
        }
        pub fn payload_bytes(&self) -> Result<Vec<u8>, KuPayloadError> {
            match self {
                Self::Prepare(value) => value.encode(),
                Self::Preview(value) => value.encode(),
                Self::Save(value) => value.encode(),
                Self::Get(value) => value.encode(),
                Self::List(value) => value.encode(),
                Self::Search(value) => value.encode(),
                Self::Revise(value) => value.encode(),
                Self::Export(value) => value.encode(),
                Self::Status(value) => value.encode(),
                Self::Cancel(value) => value.encode(),
                Self::Reconcile(value) => value.encode(),
            }
        }
        pub const fn is_registered_kind(kind: u16) -> bool {
            matches!(
                kind,
                19201
                    | 19202
                    | 19203
                    | 19204
                    | 19205
                    | 19206
                    | 19207
                    | 19208
                    | 19209
                    | 19210
                    | 19211
            )
        }
        pub fn decode_for_base_minor(
            kind: u16,
            bytes: &[u8],
            minor: u16,
        ) -> Result<Self, KuPayloadError> {
            ensure(minor >= MINIMUM_BASE_MINOR)?;
            Self::decode(kind, bytes)
        }
        pub fn decode(kind: u16, bytes: &[u8]) -> Result<Self, KuPayloadError> {
            match kind {
                19201 => Ok(Self::Prepare(KuPrepareV1::decode(bytes)?)),
                19202 => Ok(Self::Preview(KuOperationRefV1::decode(bytes)?)),
                19203 => Ok(Self::Save(KuSaveV1::decode(bytes)?)),
                19204 => Ok(Self::Get(KuGetV1::decode(bytes)?)),
                19205 => Ok(Self::List(KuListV1::decode(bytes)?)),
                19206 => Ok(Self::Search(KuSearchV1::decode(bytes)?)),
                19207 => Ok(Self::Revise(KuReviseV1::decode(bytes)?)),
                19208 => Ok(Self::Export(KuExportV1::decode(bytes)?)),
                19209 => Ok(Self::Status(KuStatusRequestV1::decode(bytes)?)),
                19210 => Ok(Self::Cancel(KuOperationRefV1::decode(bytes)?)),
                19211 => Ok(Self::Reconcile(KuOperationRefV1::decode(bytes)?)),
                _ => Err(KuPayloadError),
            }
        }
    }
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum KuResponseV1 {
        Prepare(KuPreparedV1),
        Preview(KuPreparedV1),
        Save(KuReceiptV1),
        Get(KuViewV1),
        List(KuPageV1),
        Search(KuPageV1),
        Revise(KuPreparedV1),
        Export(KuExportViewV1),
        Status(KuStatusV1),
        Cancel(KuReceiptV1),
        Reconcile(KuReceiptV1),
    }
    impl KuResponseV1 {
        pub fn discriminator(&self) -> u16 {
            match self {
                Self::Prepare(_) => 19201,
                Self::Preview(_) => 19202,
                Self::Save(_) => 19203,
                Self::Get(_) => 19204,
                Self::List(_) => 19205,
                Self::Search(_) => 19206,
                Self::Revise(_) => 19207,
                Self::Export(_) => 19208,
                Self::Status(_) => 19209,
                Self::Cancel(_) => 19210,
                Self::Reconcile(_) => 19211,
            }
        }
        pub fn validate(&self) -> Result<(), KuPayloadError> {
            match self {
                Self::Prepare(value) => value.validate(),
                Self::Preview(value) => value.validate(),
                Self::Save(value) => value.validate(),
                Self::Get(value) => value.validate(),
                Self::List(value) => value.validate(),
                Self::Search(value) => value.validate(),
                Self::Revise(value) => value.validate(),
                Self::Export(value) => value.validate(),
                Self::Status(value) => value.validate(),
                Self::Cancel(value) => value.validate(),
                Self::Reconcile(value) => value.validate(),
            }
        }
        pub fn payload_bytes(&self) -> Result<Vec<u8>, KuPayloadError> {
            match self {
                Self::Prepare(value) => value.encode(),
                Self::Preview(value) => value.encode(),
                Self::Save(value) => value.encode(),
                Self::Get(value) => value.encode(),
                Self::List(value) => value.encode(),
                Self::Search(value) => value.encode(),
                Self::Revise(value) => value.encode(),
                Self::Export(value) => value.encode(),
                Self::Status(value) => value.encode(),
                Self::Cancel(value) => value.encode(),
                Self::Reconcile(value) => value.encode(),
            }
        }
    }
}
