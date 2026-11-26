//! OneBrain protocol contracts shared by carriers and runtimes.
//!
//! `types` and `codec` own vNext logical/wire identity. The TCP/JSON demo is
//! isolated under `legacy` and retained only for backward compatibility.

pub mod chunk_transfer_codec;
pub mod codec;
pub mod legacy;
pub mod legacy_adapter;
pub mod mailbox_codec;
pub mod pillar_wire;
pub mod reachability_codec;
pub mod reachability_signaling;
pub mod reachability_signaling_codec;
pub mod reachability_types;
pub mod reconciliation_codec;
pub mod relay_codec;
pub mod relay_wire;
pub mod session_codec;
pub mod types;

pub use chunk_transfer_codec::*;
pub use codec::{decode_message, encode_message, VNextCodecError};
pub use legacy::{recv_message, send_message, PeerMessage, PeerSummary, SeedMessage};
pub use legacy_adapter::{
    LegacyAdapter, LegacyAdapterError, LegacyAdapterOffer, LegacyNormalizationProvenance,
    NormalizedLegacyEncoding, NormalizedLegacyQuery, LEGACY_ADAPTER_MAJOR, LEGACY_ENCODING_FULL,
    LEGACY_ENCODING_PART, LEGACY_SCOPE_GLOBAL,
};
pub use mailbox_codec::*;
pub use pillar_wire::*;
pub use reachability_codec::{
    decode_reachability_object, encode_reachability_object, reachability_signing_bytes,
    reachability_signing_parts, ReachabilityCodecError,
};
pub use reachability_signaling::*;
pub use reachability_signaling_codec::{
    connectivity_signing_bytes, connectivity_signing_parts, decode_connectivity_signaling,
    encode_connectivity_signaling, ConnectivitySignalingCodecError,
};
pub use reachability_types::*;
pub use reconciliation_codec::{
    bind_reconciliation_message, decode_reconciliation_message, encode_reconciliation_message,
    make_frontier_cursor, make_peer_bound_resume_token, make_resume_token,
    reconciliation_binding_digest, reconciliation_capability, reconciliation_frontier_capability,
    reconciliation_profile, reconciliation_resume_scope_digest, reconciliation_riblt_capability,
    validate_reconciliation_context, verify_frontier_cursor, ReconciliationCodecError,
};
pub use relay_codec::*;
pub use relay_wire::*;
pub use session_codec::{
    blob_chunks_capability, decode_session_message, encode_session_message, mailbox_capability,
    reconcile_frontier_capability, reconcile_riblt_capability, registry_chunks_capability,
    session_capability_from_name, session_negotiates_capability, session_signing_bytes,
    SessionCodecError,
};
pub use types::{
    wire_id, FrontierContinuationCursorV1, FrontierSummaryEntryV1, InventoryDiffRange,
    InventoryLane, InventorySummaryNode, ReconcileManifestEntry, ReconcileManifestKind,
    ReconcileReceiptEntry, ReconcileReceiptStatus, ReconciliationAbortCode, ReconciliationBody,
    ReconciliationBudget, ReconciliationContext, ReconciliationMessage, ReconciliationPhase,
    ReconciliationResumeMode, ReconciliationResumeToken, ReconciliationSummaryMethod,
    RibltCodedSymbolV1, SelectiveFeedProof, SessionCapability, SessionFinish,
    SessionHandshakeMessage, SessionHello, SessionProfile, SessionWelcome, VNextMessage,
    CAPABILITY_BLOB_CHUNKS_V1, CAPABILITY_MAILBOX_V1, CAPABILITY_RECONCILE_FRONTIER_V1,
    CAPABILITY_RECONCILE_RIBLT_V1, CAPABILITY_RECONCILE_V1, CAPABILITY_REGISTRY_CHUNKS_V1,
    MAX_FRONTIER_ENTRIES_PER_PAGE, MAX_RIBLT_CODED_SYMBOLS_PER_RANGE, MAX_RIBLT_RANGE_ITEMS,
};

pub const SEED_DOMAINS: &[&str] = &["n1.onebrain.live", "n2.onebrain.live"];
pub const SEED_PORT: u16 = 4242;
pub const DEFAULT_NODE_PORT: u16 = 4242;
pub const HEARTBEAT_INTERVAL_SECS: u64 = 60;
pub const PEER_TIMEOUT_SECS: u64 = 300;
