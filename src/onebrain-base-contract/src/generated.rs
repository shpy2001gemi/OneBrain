// Generated from src/test-vectors/vnext/base-v1-runtime-interface-v1.json; DO NOT EDIT.
use crate::operation::{BoundedAscii, BoundedBytes, BoundedVec, SecretBytes};

pub const BASE_RUNTIME_PROFILE_MAJOR: u16 = 1;
pub const BASE_RUNTIME_PROFILE_MINOR: u16 = 0;

#[derive(Clone, PartialEq, Eq)]
pub struct CapabilitySetV1(pub(crate) BoundedVec<u16, 64>);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CompatibilityDigestV1(pub [u8; 32]);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DatasetGenerationV1(pub u64);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct EventCursorV1(pub u64);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct IdempotencyKeyV1(pub [u8; 32]);

#[derive(Clone, PartialEq, Eq)]
pub struct LimitationCodeV1(pub(crate) BoundedAscii<128>);

pub struct ManagementHandleV1(pub(crate) [u8; 32]);

#[derive(Clone, PartialEq, Eq)]
pub struct OpaqueContinuationV1(pub(crate) BoundedBytes<4096>);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct OperationIdV1(pub [u8; 32]);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct OperationReservationIdV1(pub [u8; 32]);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ProcessGenerationV1(pub u64);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ProfileMajorV1(pub u16);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ProfileMinorV1(pub u16);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RequestIdV1(pub [u8; 32]);

pub struct SubscriptionHandleV1(pub(crate) [u8; 32]);

#[derive(Clone, PartialEq, Eq)]
pub struct TypedPayloadV1(pub(crate) BoundedBytes<1048576>);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ActorRootPublicIdV1(pub [u8; 32]);

pub struct ArchiveCapabilityHandleV1(pub(crate) [u8; 32]);

#[derive(Clone, PartialEq, Eq)]
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

#[derive(Clone, Copy, PartialEq, Eq)]
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

#[derive(Clone, PartialEq, Eq)]
pub struct BaseOpaqueContinuation(pub(crate) BoundedBytes<4096>);

#[derive(Clone, Copy, PartialEq, Eq)]
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

#[derive(Clone, Copy, PartialEq, Eq)]
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

pub struct BaseQueryRequestV1 {
    pub payload: TypedPayloadV1,
    pub continuation: Option<BaseOpaqueContinuation>,
    pub budget: ResourceBudgetV1,
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct FeedAuthorPublicIdV1(pub [u8; 32]);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct NodeTransportPublicIdV1(pub [u8; 32]);

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
