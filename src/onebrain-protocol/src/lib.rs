//! OneBrain protocol contracts shared by carriers and runtimes.
//!
//! `types` and `codec` own vNext logical/wire identity. The TCP/JSON demo is
//! isolated under `legacy` and retained only for backward compatibility.

pub mod codec;
pub mod legacy;
pub mod legacy_adapter;
pub mod reachability_codec;
pub mod reachability_types;
pub mod reconciliation_codec;
pub mod session_codec;
pub mod types;

pub use codec::{decode_message, encode_message, VNextCodecError};
pub use legacy::{recv_message, send_message, PeerMessage, PeerSummary, SeedMessage};
pub use legacy_adapter::{
    LegacyAdapter, LegacyAdapterError, LegacyAdapterOffer, LegacyNormalizationProvenance,
    NormalizedLegacyEncoding, NormalizedLegacyQuery, LEGACY_ADAPTER_MAJOR, LEGACY_ENCODING_FULL,
    LEGACY_ENCODING_PART, LEGACY_SCOPE_GLOBAL,
};
pub use reachability_codec::{
    decode_reachability_object, encode_reachability_object, reachability_signing_bytes,
    ReachabilityCodecError,
};
pub use reachability_types::*;
pub use reconciliation_codec::{
    bind_reconciliation_message, decode_reconciliation_message, encode_reconciliation_message,
    make_peer_bound_resume_token, make_resume_token, reconciliation_binding_digest,
    reconciliation_capability, reconciliation_profile, reconciliation_resume_scope_digest,
    validate_reconciliation_context, ReconciliationCodecError,
};
pub use session_codec::{
    decode_session_message, encode_session_message, session_signing_bytes, SessionCodecError,
};
pub use types::{
    wire_id, InventoryDiffRange, InventoryLane, InventorySummaryNode, ReconcileManifestEntry,
    ReconcileManifestKind, ReconcileReceiptEntry, ReconcileReceiptStatus, ReconciliationAbortCode,
    ReconciliationBody, ReconciliationBudget, ReconciliationContext, ReconciliationMessage,
    ReconciliationPhase, ReconciliationResumeMode, ReconciliationResumeToken,
    ReconciliationSummaryMethod, SelectiveFeedProof, SessionCapability, SessionFinish,
    SessionHandshakeMessage, SessionHello, SessionProfile, SessionWelcome, VNextMessage,
};

pub const SEED_DOMAINS: &[&str] = &["n1.onebrain.live", "n2.onebrain.live"];
pub const SEED_PORT: u16 = 4242;
pub const DEFAULT_NODE_PORT: u16 = 4242;
pub const HEARTBEAT_INTERVAL_SECS: u64 = 60;
pub const PEER_TIMEOUT_SECS: u64 = 300;
