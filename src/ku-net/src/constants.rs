//! # OBP Protocol Constants
//!
//! Central registry of all protocol constants from SPEC A/B/C/D.
//! Avoids magic numbers scattered across modules.
//!
//! Re-exports constants from submodules where they are defined,
//! plus transport/routing constants used across multiple modules.

// ─── Re-exports from identity.rs (SPEC A §1) ──────────────────────────────
pub use crate::identity::{
    DEVICE_GROUP_MAX, OBP_ALPN, OBP_PORT, PUZZLE_C_LARGE, PUZZLE_C_MEDIUM, PUZZLE_C_SMALL,
};

// ─── Transport (SPEC A §3) ─────────────────────────────────────────────────

/// Maximum concurrent QUIC streams per connection.
pub const QUIC_MAX_STREAMS: u32 = 100;
/// QUIC idle timeout in seconds.
pub const QUIC_IDLE_TIMEOUT_S: u64 = 30;
/// QUIC keep-alive interval in seconds.
pub const QUIC_KEEP_ALIVE_S: u64 = 15;
/// Maximum QUIC datagram size (bytes, fits in single MTU).
pub const QUIC_MAX_DATAGRAM: usize = 1200;

// ─── Message Framing (SPEC D §1) ──────────────────────────────────────────

/// OBP message header size in bytes (type:1 + flags:1 + length:4).
pub const HEADER_SIZE: usize = 6;
/// Maximum payload size (capped at 16 MB for sanity, u32 allows more).
pub const MAX_PAYLOAD_SIZE: usize = 16 * 1024 * 1024;

// ─── SWIM Membership (SPEC B §1) ──────────────────────────────────────────

/// SWIM probe period (ms).
pub const T_PERIOD_MS: u64 = 1_000;
/// Number of indirect probes per failure suspicion.
pub const K_INDIRECT: usize = 3;
/// Maximum membership list size.
pub const MAX_MEMBERS: usize = 10_000;
/// Lifeguard LHA initial multiplier.
pub const LHA_INITIAL: u32 = 2;
/// Maximum incarnation increment for refutation.
pub const MAX_INCARNATION_JUMP: u32 = 100;

// ─── 7-Tier Fitness Model (SPEC B §2) ─────────────────────────────────────

/// Fitness weight: uptime (w₁).
pub const W_UPTIME: f64 = 0.20;
/// Fitness weight: battery (w₂).
pub const W_BATTERY: f64 = 0.15;
/// Fitness weight: bandwidth (w₃).
pub const W_BANDWIDTH: f64 = 0.20;
/// Fitness weight: available storage (w₄).
pub const W_STORAGE: f64 = 0.15;
/// Fitness weight: CPU headroom (w₅).
pub const W_CPU: f64 = 0.10;
/// Fitness weight: network quality — jitter, loss (w₆).
pub const W_NETWORK: f64 = 0.10;
/// Fitness weight: reputation score (w₇).
pub const W_REPUTATION: f64 = 0.10;

/// Tier thresholds (min fitness score for each tier).
pub const TIER_THRESHOLDS: [f64; 7] = [
    0.0,  // T0 Leaf
    0.30, // T1 Contributor
    0.50, // T2 Local SP
    0.65, // T3 District SP
    0.80, // T4 Country SP
    0.90, // T5 Region SP
    0.95, // T6 Global Backbone
];

/// Demotion hysteresis margin (node must drop below threshold - margin to demote).
pub const TIER_HYSTERESIS: f64 = 0.05;

// ─── DHT (SPEC B §5) ──────────────────────────────────────────────────────

/// Kademlia replication factor (k-bucket size).
pub const K_BUCKET_SIZE: usize = 20;
/// Kademlia lookup parallelism.
pub const ALPHA: usize = 3;
/// S/Kademlia disjoint lookup paths.
pub const BETA: usize = 3;
/// Number of k-buckets (256-bit NodeId space).
pub const NUM_BUCKETS: usize = 256;

// ─── Bootstrap Discovery (SPEC A §6) ──────────────────────────────────────

/// Maximum peers returned in a single PEX exchange.
pub const PEX_MAX_PEERS: usize = 32;
/// Bootstrap layer timeout (seconds).
pub const BOOTSTRAP_LAYER_TIMEOUT_S: u64 = 10;
/// Minimum peers needed to consider bootstrap successful.
pub const BOOTSTRAP_MIN_PEERS: usize = 3;

// ─── Content Routing (SPEC B §6-8) ────────────────────────────────────────

/// Pheromone decay rate (τ) per hour.
pub const PHEROMONE_DECAY: f32 = 0.95;
/// Maximum pheromone table entries per node.
pub const MAX_PHEROMONE_ENTRIES: usize = 10_000;
/// Vacuum filter bits per item (default).
pub const VACUUM_BITS_PER_ITEM: u8 = 10;
/// Vacuum filter target false-positive rate.
pub const VACUUM_TARGET_FPR: f64 = 0.001;

// ─── Security (SPEC C) ────────────────────────────────────────────────────

/// Maximum query depth for recursive traversal.
pub const MAX_QUERY_DEPTH: usize = 10;
/// Query timeout (seconds).
pub const QUERY_TIMEOUT_S: u64 = 30;
/// Maximum concurrent queries per node.
pub const MAX_CONCURRENT_QUERIES: usize = 50;

// ─── Encoding Consensus (SPEC §4.5) ──────────────────────────────────────

/// Maximum number of verifiers per encoding job (capped for efficiency).
pub const MAX_ENCODING_VERIFIERS: u8 = 3;
/// Encoding job TTL on DHT (7 days in seconds).
pub const ENCODING_JOB_TTL_S: u64 = 7 * 24 * 3600;
/// Maximum concurrent encoding jobs per verifier node.
pub const MAX_CONCURRENT_ENCODING_JOBS: usize = 3;
/// Consensus score threshold for PART → FULL transition.
pub const ENCODING_CONSENSUS_THRESHOLD: f32 = 0.70;
/// Anti-stampede cooldown between claim attempts (seconds).
pub const ENCODING_CLAIM_COOLDOWN_S: u64 = 60;
/// Base OBT reward per verification.
pub const ENCODING_REWARD_BASE_OBT: u64 = 5;
/// Encoding gossip interval — how often to broadcast job updates (seconds).
pub const ENCODING_GOSSIP_INTERVAL_S: u64 = 30;

/// Stigmergy weight: waiting time factor.
pub const ENCODING_PHEROMONE_ALPHA_WAIT: f32 = 0.4;
/// Stigmergy weight: remaining slots factor.
pub const ENCODING_PHEROMONE_BETA_SLOTS: f32 = 0.3;
/// Stigmergy weight: reward factor.
pub const ENCODING_PHEROMONE_GAMMA_REWARD: f32 = 0.3;
/// Stigmergy evaporation rate per hour.
pub const ENCODING_PHEROMONE_EVAPORATION: f32 = 0.1;

// ─── OBT Token Protocol (SPEC §5) ────────────────────────────────────────

/// OBT epoch duration in seconds (1 hour). Same as ku-core OBT_EPOCH_DURATION_S.
pub const OBT_EPOCH_DURATION_S: u64 = 3_600;

// Message type codes 0xA0–0xA6 for OBT network protocol.
// See docs/specs/obt/06_TRANSFER.md §6.1 for wire format details.

/// OBT Transfer Request: from, to, amount, nonce, signature.
pub const MSG_OBT_TRANSFER_REQUEST: u8 = 0xA0;
/// OBT Transfer Confirm: tx_id, witness_signature.
pub const MSG_OBT_TRANSFER_CONFIRM: u8 = 0xA1;
/// OBT Balance Query: node_id.
pub const MSG_OBT_BALANCE_QUERY: u8 = 0xA2;
/// OBT Balance Response: node_id, balance, head_hash, Merkle proof.
pub const MSG_OBT_BALANCE_RESPONSE: u8 = 0xA3;
/// OBT Mint Broadcast: signed MintProof to network.
pub const MSG_OBT_MINT_BROADCAST: u8 = 0xA4;
/// OBT Storage Challenge: ku_cid, challenge_type, params.
pub const MSG_OBT_STORAGE_CHALLENGE: u8 = 0xA5;
/// ForkWarrant broadcast — propagate fork evidence to DHT neighbors.
pub const MSG_OBT_FORK_WARRANT: u8 = 0xA6;

/// Confirmation timeout for OBT transfers (seconds).
pub const OBT_CONFIRMATION_TIMEOUT_S: u64 = 30;
/// Maximum retries for transfer confirmation.
pub const OBT_TRANSFER_MAX_RETRIES: u8 = 3;
/// Minimum witnesses required for MintProof.
pub const OBT_MIN_WITNESSES: u32 = 3;
/// Maximum witnesses for MintProof.
pub const OBT_MAX_WITNESSES: u32 = 7;
/// Unreceived Send blocks expire after 7 days (seconds).
pub const OBT_UNRECEIVED_SEND_EXPIRY_S: u64 = 7 * 24 * 3_600;
/// PoS-KU challenge response timeout (seconds).
pub const OBT_POS_KU_TIMEOUT_S: u64 = 30;

// ─── OBKG Graph Gossip (SPEC §OBKG) ──────────────────────────────────────

// Message type codes 0xB0–0xB3 for OBKG graph gossip protocol.

/// FedR Delta Push: peer_id, epoch, deltas, signature.
pub const MSG_FEDR_DELTA_PUSH: u8 = 0xB0;
/// FedR Delta Pull: requester_id, min_epoch.
pub const MSG_FEDR_DELTA_PULL: u8 = 0xB1;
/// Graph Stats: bond counts, KU count, FedR epoch, last dream time.
pub const MSG_GRAPH_STATS: u8 = 0xB2;
/// Dream Report: reinforcement/pruning results from dream consolidation.
pub const MSG_DREAM_REPORT: u8 = 0xB3;

// ─── Replication (Phase 4 — DHT Storage Replication) ─────────────────────

/// Storage replication factor (distinct from routing K=20).
/// Each KU is replicated to R=7 nodes for durability.
pub const STORAGE_REPLICATION_FACTOR: usize = 7;
/// Minimum healthy replicas before triggering repair.
pub const MIN_HEALTHY_REPLICAS: usize = 4;
/// Target replicas after repair (same as STORAGE_REPLICATION_FACTOR).
pub const REPAIR_TARGET_REPLICAS: usize = 7;

// Message type codes 0x24–0x26 for replication protocol.

/// STORE RPC: request a peer to store a KU value.
pub const MSG_STORE_RPC: u8 = 0x24;
/// STORE ACK: acknowledgment of successful STORE.
pub const MSG_STORE_ACK: u8 = 0x25;
/// Replication health check: query replica count for a CID.
pub const MSG_REPLICATION_CHECK: u8 = 0x26;

// ── Blob Storage ───────────────────────────────────────────────────────────

/// Blob chunk size: 256KB (IPFS-compatible fixed chunks).
pub const BLOB_CHUNK_SIZE: usize = 256 * 1024;

/// Maximum single blob size: 100MB.
pub const BLOB_MAX_SIZE: u64 = 100 * 1024 * 1024;

/// Blob replication factor for hot blobs.
pub const BLOB_REPLICATION_HOT: usize = 3;

/// OB-CID version byte.
pub const BLOB_CID_VERSION: u8 = 0x01;

/// MediaRef system byte for OBS Blob Store.
pub const BLOB_MEDIAREF_SYSTEM: u8 = 0x01;

/// Message code: Blob STORE request.
pub const MSG_BLOB_STORE_RPC: u8 = 0x30;

/// Message code: Blob STORE acknowledgment.
pub const MSG_BLOB_STORE_ACK: u8 = 0x31;

/// Message code: Blob chunk request.
pub const MSG_BLOB_CHUNK_REQ: u8 = 0x32;

/// Message code: Blob chunk response.
pub const MSG_BLOB_CHUNK_RES: u8 = 0x33;
