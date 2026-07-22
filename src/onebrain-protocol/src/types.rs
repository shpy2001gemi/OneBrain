//! Logical vNext protocol messages. Transport adapters consume these types;
//! they do not own their own wire enums.

use ku_core::foundation::{
    DisclosureClass, EventCid, FeedId, NamespaceCommitment, NodeId, ObjectCid, ObjectReference,
    SelectorCid,
};

pub const VNEXT_PROTOCOL_SCHEMA_ID: u64 = 0x4f42_5000_0000_0001;
pub const VNEXT_PROTOCOL_SCHEMA_MAJOR: u64 = 1;
pub const VNEXT_PROTOCOL_SCHEMA_MINOR: u64 = 0;
pub const MAX_VNEXT_PAYLOAD_BYTES: usize = 1_000_000;

pub mod wire_id {
    pub const OBJECT_MANIFEST: u64 = 1;
    pub const OBJECT_PAYLOAD: u64 = 2;
    pub const EVENT_MANIFEST: u64 = 3;
    pub const EVENT_PAYLOAD: u64 = 4;
    pub const SESSION_HELLO: u64 = 10;
    pub const SESSION_WELCOME: u64 = 11;
    pub const SESSION_FINISH: u64 = 12;
    pub const RECONCILE_HELLO: u64 = 20;
    pub const RECONCILE_SELECTOR_OFFER: u64 = 21;
    pub const RECONCILE_INVENTORY_SUMMARY: u64 = 22;
    pub const RECONCILE_DIFF: u64 = 23;
    pub const RECONCILE_MANIFEST: u64 = 24;
    pub const RECONCILE_RECEIPT: u64 = 25;
    pub const RECONCILE_PROGRESS: u64 = 26;
    pub const RECONCILE_ABORT: u64 = 27;
    pub const RECONCILE_RESUME: u64 = 28;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SessionProfile {
    pub family: u64,
    pub major: u64,
    pub minor: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionCapability([u8; 32]);

impl SessionCapability {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Capability-scoped reference to a separately validated feed proof. Merely
/// carrying this disclosure in a handshake grants no feed/content authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectiveFeedProof {
    pub feed: FeedId,
    pub namespace: NamespaceCommitment,
    pub capability: SessionCapability,
    pub proof: ObjectReference,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionHello {
    pub transport_binding: [u8; 32],
    pub initiator_nonce: [u8; 32],
    pub node: NodeId,
    pub node_public_key: [u8; 32],
    /// Ordered strongest/preferred first.
    pub profiles: Vec<SessionProfile>,
    pub capabilities: Vec<SessionCapability>,
    pub feed_proofs: Vec<SelectiveFeedProof>,
    pub signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionWelcome {
    pub transport_binding: [u8; 32],
    pub initiator_transcript: [u8; 32],
    pub responder_nonce: [u8; 32],
    pub node: NodeId,
    pub node_public_key: [u8; 32],
    pub selected_profile: SessionProfile,
    pub negotiated_capabilities: Vec<SessionCapability>,
    pub feed_proofs: Vec<SelectiveFeedProof>,
    pub signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionFinish {
    pub transcript: [u8; 32],
    pub initiator: NodeId,
    pub signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionHandshakeMessage {
    Hello(SessionHello),
    Welcome(SessionWelcome),
    Finish(SessionFinish),
}

impl SessionHandshakeMessage {
    pub const fn wire_id(&self) -> u64 {
        match self {
            Self::Hello(_) => wire_id::SESSION_HELLO,
            Self::Welcome(_) => wire_id::SESSION_WELCOME,
            Self::Finish(_) => wire_id::SESSION_FINISH,
        }
    }
}

/// The only correctness-required inventory summary in OBP-RP v1. Optional
/// accelerators must never change the result represented by this radix forest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum ReconciliationSummaryMethod {
    RadixForest256V1 = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum ReconciliationResumeMode {
    Disabled = 0,
    BoundTokenV1 = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReconciliationBudget {
    pub max_summary_nodes: u64,
    pub max_diff_ranges: u64,
    pub max_manifest_entries: u64,
    pub max_payload_bytes: u64,
}

/// Immutable scope agreed for one reconciliation exchange. The digest of this
/// complete structure is carried by every OBP-RP message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReconciliationContext {
    pub authenticated_transcript: [u8; 32],
    pub selector: SelectorCid,
    pub namespace: NamespaceCommitment,
    pub disclosure: DisclosureClass,
    pub summary_method: ReconciliationSummaryMethod,
    pub budget: ReconciliationBudget,
    pub resume_mode: ReconciliationResumeMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u64)]
pub enum InventoryLane {
    Object = 1,
    Event = 2,
    MappingKernel = 3,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InventorySummaryNode {
    pub lane: InventoryLane,
    pub prefix_bits: u64,
    pub prefix: Vec<u8>,
    pub digest: [u8; 32],
    pub leaf_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InventoryDiffRange {
    pub lane: InventoryLane,
    pub prefix_bits: u64,
    pub prefix: Vec<u8>,
    pub offered_digest: [u8; 32],
    pub observed_digest: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum ReconcileManifestKind {
    Object = 1,
    Event = 2,
    MappingKernel = 3,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReconcileManifestEntry {
    pub kind: ReconcileManifestKind,
    pub cid: [u8; 32],
    pub canonical_length: u64,
}

/// A receipt records protocol/storage handling only. It is never an authority,
/// truth, adoption, benefit, ranking or reward decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum ReconcileReceiptStatus {
    ValidatedStored = 1,
    AlreadyPresent = 2,
    RejectedInvalid = 3,
    DeferredBudget = 4,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReconcileReceiptEntry {
    pub kind: ReconcileManifestKind,
    pub cid: [u8; 32],
    pub status: ReconcileReceiptStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum ReconciliationPhase {
    Offered = 1,
    Summarizing = 2,
    Diffing = 3,
    Manifesting = 4,
    Receiving = 5,
    ManifestBatchComplete = 6,
    SelectorComplete = 7,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum ReconciliationAbortCode {
    UnsupportedProfile = 1,
    ScopeMismatch = 2,
    BudgetExhausted = 3,
    InvalidMessage = 4,
    LocalPolicy = 5,
}

/// Opaque continuation material plus inspectable scope binding. OBP-005 will
/// define persistence and MAC ownership; this protocol type cannot grant any
/// authority by itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReconciliationResumeToken {
    pub binding_digest: [u8; 32],
    pub checkpoint_digest: [u8; 32],
    pub next_sequence: u64,
    pub opaque: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReconciliationBody {
    Hello {
        nonce: [u8; 32],
        profile: SessionProfile,
        capability: SessionCapability,
    },
    SelectorOffer {
        inventory_root: [u8; 32],
        lanes: Vec<InventoryLane>,
        checkpoint_frontier: Option<[u8; 32]>,
    },
    InventorySummary {
        inventory_root: [u8; 32],
        leaf_count: u64,
        nodes: Vec<InventorySummaryNode>,
    },
    Diff {
        ranges: Vec<InventoryDiffRange>,
    },
    Manifest {
        entries: Vec<ReconcileManifestEntry>,
    },
    Receipt {
        entries: Vec<ReconcileReceiptEntry>,
    },
    Progress {
        phase: ReconciliationPhase,
        processed: u64,
        pending_upper_bound: Option<u64>,
        resume_token: Option<ReconciliationResumeToken>,
    },
    Abort {
        code: ReconciliationAbortCode,
        retryable: bool,
        progress_digest: [u8; 32],
    },
    Resume {
        token: ReconciliationResumeToken,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReconciliationMessage {
    pub context: ReconciliationContext,
    pub binding_digest: [u8; 32],
    pub sequence: u64,
    pub body: ReconciliationBody,
}

impl ReconciliationMessage {
    pub const fn wire_id(&self) -> u64 {
        match &self.body {
            ReconciliationBody::Hello { .. } => wire_id::RECONCILE_HELLO,
            ReconciliationBody::SelectorOffer { .. } => wire_id::RECONCILE_SELECTOR_OFFER,
            ReconciliationBody::InventorySummary { .. } => wire_id::RECONCILE_INVENTORY_SUMMARY,
            ReconciliationBody::Diff { .. } => wire_id::RECONCILE_DIFF,
            ReconciliationBody::Manifest { .. } => wire_id::RECONCILE_MANIFEST,
            ReconciliationBody::Receipt { .. } => wire_id::RECONCILE_RECEIPT,
            ReconciliationBody::Progress { .. } => wire_id::RECONCILE_PROGRESS,
            ReconciliationBody::Abort { .. } => wire_id::RECONCILE_ABORT,
            ReconciliationBody::Resume { .. } => wire_id::RECONCILE_RESUME,
        }
    }

    pub const fn grants_authority(&self) -> bool {
        false
    }

    pub const fn establishes_global_completion(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VNextMessage {
    ObjectManifest {
        selector: SelectorCid,
        object: ObjectCid,
        disclosure: DisclosureClass,
        canonical_length: u64,
    },
    ObjectPayload {
        selector: SelectorCid,
        object: ObjectCid,
        canonical_bytes: Vec<u8>,
    },
    EventManifest {
        selector: SelectorCid,
        event: EventCid,
        disclosure: DisclosureClass,
        canonical_length: u64,
    },
    EventPayload {
        selector: SelectorCid,
        event: EventCid,
        canonical_bytes: Vec<u8>,
    },
}

impl VNextMessage {
    pub const fn wire_id(&self) -> u64 {
        match self {
            Self::ObjectManifest { .. } => wire_id::OBJECT_MANIFEST,
            Self::ObjectPayload { .. } => wire_id::OBJECT_PAYLOAD,
            Self::EventManifest { .. } => wire_id::EVENT_MANIFEST,
            Self::EventPayload { .. } => wire_id::EVENT_PAYLOAD,
        }
    }
}
