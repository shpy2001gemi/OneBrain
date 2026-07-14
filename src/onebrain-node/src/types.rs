//! Shared view types for all UI interfaces.
//!
//! These structs are "view models" — lightweight representations
//! of internal data for display purposes. All interfaces use these
//! instead of accessing internal types directly.

use serde::{Serialize, Deserialize};

/// Summary of a KU for list views.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KuListItem {
    /// Hex-encoded CID.
    pub cid_hex: String,
    /// Gene type (Fact, Procedure, Experience, etc.).
    pub gene_type: String,
    /// First ~80 chars of content.
    pub preview: String,
    /// PoMV score (0.0-1.0).
    pub pomv: f64,
    /// Trust score (0.0-1.0).
    pub trust: f64,
    /// Creation timestamp (epoch seconds).
    pub created: u64,
    /// Wire size in bytes.
    pub wire_size: usize,
}

/// Detailed view of a single KU.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KuDetail {
    /// Hex-encoded CID.
    pub cid_hex: String,
    /// Gene type.
    pub gene_type: String,
    /// Full source text.
    pub content: String,
    /// Extracted codons/concepts.
    pub codons: Vec<CodonView>,
    /// Bonds (outgoing + incoming).
    pub bonds: Vec<BondView>,
    /// Trust score.
    pub trust: f64,
    /// PoMV composite score.
    pub pomv: f64,
    /// PoMV breakdown.
    pub pomv_breakdown: PomvBreakdown,
    /// Epistemic status.
    pub epistemic: String,
    /// Evidence type.
    pub evidence: String,
    /// Wire size in bytes.
    pub wire_size: usize,
    /// Instruction count.
    pub instruction_count: usize,
    /// Encoding confidence.
    pub confidence: f32,
    /// Creation timestamp.
    pub created: u64,
    /// Verification status.
    pub verification_status: String,
    /// Number of outgoing bonds.
    pub outgoing_bond_count: usize,
    /// Number of incoming bonds.
    pub incoming_bond_count: usize,
    /// Decoded instructions in human-readable form.
    pub decoded_instructions: Vec<InstructionView>,
}

/// Human-readable decoded instruction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstructionView {
    /// Instruction type (Triple, Quality, Quantity, Step, etc.).
    pub op: String,
    /// Human-readable description.
    pub description: String,
    /// Raw concept IDs involved.
    pub concept_ids: Vec<u64>,
}

/// A codon/concept extracted from a KU.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodonView {
    /// Concept name.
    pub name: String,
    /// Role (Domain, Agent, Content, Time, Result, etc.).
    pub role: String,
}

/// A bond (relationship) to/from a KU.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BondView {
    /// Direction: "OUT" or "IN".
    pub direction: String,
    /// Relation type (Extends, Cites, Refutes, PartOf, etc.).
    pub relation: String,
    /// CID of the other KU.
    pub other_cid: String,
    /// Preview text of the other KU (if available).
    pub other_preview: String,
    /// Bond weight (0.0-1.0).
    pub weight: f64,
}

/// PoMV score breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PomvBreakdown {
    pub metabolic: f64,
    pub prediction: f64,
    pub entropy: f64,
    pub survival: f64,
    pub centrality: f64,
    pub niche: f64,
}

impl Default for PomvBreakdown {
    fn default() -> Self {
        Self {
            metabolic: 0.0,
            prediction: 0.0,
            entropy: 0.0,
            survival: 0.0,
            centrality: 0.0,
            niche: 0.0,
        }
    }
}

/// Graph neighbor info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeighborInfo {
    /// CID of neighbor.
    pub cid_hex: String,
    /// Relation type.
    pub relation: String,
    /// Direction: "OUT" or "IN".
    pub direction: String,
    /// Preview text.
    pub preview: String,
    /// Bond weight.
    pub weight: f64,
    /// Gene type of the neighbor KU.
    pub gene_type: String,
    /// PoMV score of neighbor.
    pub pomv: f64,
    /// Whether the KU exists in local storage.
    pub is_local: bool,
    /// Children (for tree display at depth > 1).
    pub children: Vec<NeighborInfo>,
}

/// Identity information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityInfo {
    /// Node ID (hex).
    pub node_id: String,
    /// Display name.
    pub name: String,
    /// Creation timestamp.
    pub created: u64,
    /// Trust tier name.
    pub tier: String,
    /// Trust score (0.0-1.0).
    pub trust_score: f64,
    /// Number of devices in group.
    pub device_count: u32,
    /// Max devices allowed.
    pub max_devices: u32,
    /// KUs encoded count.
    pub kus_encoded: u64,
    /// KUs received count.
    pub kus_received: u64,
    /// Total queries.
    pub total_queries: u64,
}

/// User profile view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfileView {
    /// Display name.
    pub name: String,
    /// Preferred language.
    pub language: String,
    /// Response style.
    pub style: String,
    /// Top expertise areas.
    pub expertise: Vec<ExpertiseView>,
    /// Total KUs.
    pub total_kus: u64,
    /// Total queries.
    pub total_queries: u64,
    /// Member since (epoch seconds).
    pub member_since: u64,
}

/// An expertise area.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpertiseView {
    /// Domain name.
    pub domain: String,
    /// Number of KUs in this domain.
    pub ku_count: u64,
    /// Last active (epoch seconds).
    pub last_active: u64,
}

/// AI model info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model name (e.g., "qwen3:8b").
    pub name: String,
    /// Parameter count description (e.g., "8B params").
    pub params: String,
    /// Whether this is the currently active model.
    pub is_current: bool,
    /// Whether it's installed in Ollama.
    pub is_installed: bool,
}

/// AI health check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiHealthInfo {
    /// Whether Ollama is connected.
    pub connected: bool,
    /// Current model name.
    pub model: String,
    /// Ollama URL.
    pub ollama_url: String,
    /// Latency in milliseconds (0 if not connected).
    pub latency_ms: u64,
    /// Status message.
    pub status_message: String,
}

/// OBT wallet info (from local AccountState).
/// OBT uses Nano-style block-lattice — each node has its own chain.
/// Balance = head_block.balance (authoritative, local, instant).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletInfo {
    /// Current spendable balance (milliOBT).
    pub balance: u64,
    /// Number of blocks in local chain.
    pub chain_length: u64,
    /// Current trust tier.
    pub tier: String,
    /// Tier reward multiplier.
    pub multiplier: f64,
    /// Total earned (informational, from GCounter).
    pub total_earned: u64,
    /// Total spent (informational, from GCounter).
    pub total_spent: u64,
    /// Earnings by stream.
    pub streams: EarningsStreams,
    /// Rate limit info.
    pub rate_used: u32,
    /// Rate limit max.
    pub rate_max: u32,
}

/// Earnings breakdown by stream.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EarningsStreams {
    /// R1: Owner (PoMV-based) — 40%.
    pub owner: u64,
    /// R2: Encoder — 25%.
    pub encoder: u64,
    /// R3: Verifier — 15%.
    pub verifier: u64,
    /// R4: Storage — 20%.
    pub storage: u64,
}

/// A single wallet transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletTransaction {
    /// Block type: "Mint", "Send", "Receive", "Refund", "Open".
    pub block_type: String,
    /// Amount (milliOBT). Positive for credit, negative for debit.
    pub amount: i64,
    /// Detail string (e.g., "R1:Owner — KU a1b2c3...").
    pub detail: String,
    /// Timestamp (epoch seconds).
    pub timestamp: u64,
    /// Confirmation level: "Pending", "Tentative", "Confirmed", "Settled".
    pub confirmation: String,
}

/// Node configuration view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigView {
    pub name: String,
    pub port: u16,
    pub data_dir: String,
    pub ollama_url: String,
    pub model: String,
    pub seeds: Vec<String>,
    /// Derived paths.
    pub identity_path: String,
    pub storage_path: String,
    pub profile_path: String,
    pub peers_path: String,
}

/// Backup info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    /// Output file path.
    pub path: String,
    /// Backup size in bytes.
    pub size: u64,
    /// Number of KUs backed up.
    pub ku_count: usize,
    /// Timestamp.
    pub timestamp: u64,
}

/// Import result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    /// Number of KUs imported.
    pub imported: usize,
    /// Number of duplicates skipped.
    pub skipped: usize,
    /// Number of errors.
    pub errors: usize,
}


/// Blob storage stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobStatsView {
    /// Total blob count.
    pub count: usize,
    /// Total size in bytes.
    pub total_size: u64,
    /// Quota in bytes.
    pub quota: u64,
    /// Usage percentage.
    pub usage_pct: f64,
}

/// A followed node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowedNode {
    /// Node ID (hex).
    pub node_id: String,
    /// Display name.
    pub name: String,
    /// When the follow was created (epoch seconds).
    pub followed_since: u64,
}

/// Public profile of another node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerProfile {
    /// Node ID (hex).
    pub node_id: String,
    /// Display name.
    pub name: String,
    /// Trust score (0.0-1.0).
    pub trust_score: f64,
    /// Trust tier name.
    pub tier: String,
    /// Number of KUs encoded.
    pub ku_count: u64,
    /// Top expertise areas.
    pub expertise: Vec<String>,
    /// Member since (epoch seconds).
    pub member_since: u64,
}

/// Device info in identity group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Device ID (hex).
    pub device_id: String,
    /// Friendly device name.
    pub name: String,
    /// Device type: "Desktop", "Mobile", "CLI".
    pub device_type: String,
    /// Last seen (epoch seconds).
    pub last_seen: u64,
    /// KU count on this device.
    pub ku_count: u64,
    /// Sync status: "up-to-date", "behind", "offline".
    pub sync_status: String,
}

/// Multi-device sync status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatusInfo {
    /// Overall status: "up-to-date", "syncing", "offline".
    pub status: String,
    /// Number of items pending sync.
    pub pending_count: usize,
    /// Last sync timestamp (epoch seconds).
    pub last_sync: u64,
    /// Per-device statuses.
    pub devices: Vec<DeviceInfo>,
}

/// Result of a bulk delete operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkDeleteResult {
    /// Number of KUs deleted.
    pub deleted: usize,
    /// Number of KUs skipped (e.g., not matching filter).
    pub skipped: usize,
}

/// Watch query info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchInfo {
    /// Watch ID.
    pub id: String,
    /// KQL query string.
    pub kql_query: String,
    /// Creation timestamp (epoch seconds).
    pub created_at: u64,
    /// Number of matches so far.
    pub match_count: u64,
}

// ── Tier C — Search History ────────────────────────────────────────────

/// A single entry in the search history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHistoryEntry {
    /// Unique ID.
    pub id: String,
    /// The query string.
    pub query: String,
    /// Number of results returned.
    pub result_count: usize,
    /// Timestamp (epoch seconds).
    pub timestamp: u64,
}

// ── Tier C — Notification Preferences ──────────────────────────────────

/// User notification preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPrefs {
    /// Enable encode completion notifications.
    pub encode_complete: bool,
    /// Enable peer connection notifications.
    pub peer_connected: bool,
    /// Enable sync completion notifications.
    pub sync_complete: bool,
    /// Enable watch match notifications.
    pub watch_match: bool,
    /// Enable error notifications.
    pub errors: bool,
}

impl Default for NotificationPrefs {
    fn default() -> Self {
        Self {
            encode_complete: true,
            peer_connected: true,
            sync_complete: true,
            watch_match: true,
            errors: true,
        }
    }
}

// ── Tier C — Saved Searches ────────────────────────────────────────────

/// A saved search query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSearch {
    /// Unique ID.
    pub id: String,
    /// User-facing name.
    pub name: String,
    /// The query string (text or KQL).
    pub query: String,
    /// Whether this is a KQL query.
    pub is_kql: bool,
    /// Creation timestamp.
    pub created_at: u64,
}

// ── Tier C — Collections ───────────────────────────────────────────────

/// A collection of KUs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    /// Unique ID.
    pub id: String,
    /// Collection name.
    pub name: String,
    /// Optional description.
    pub description: String,
    /// CID hex strings of KUs in this collection.
    pub ku_cids: Vec<String>,
    /// Creation timestamp.
    pub created_at: u64,
    /// Last updated timestamp.
    pub updated_at: u64,
}

// ── Tier C — KU Version Chain ──────────────────────────────────────────

/// An entry in a KU's version chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KuVersionEntry {
    /// CID of this version.
    pub cid_hex: String,
    /// Gene type.
    pub gene_type: String,
    /// Preview text.
    pub preview: String,
    /// Version number (1 = original).
    pub version: u32,
    /// Timestamp.
    pub created: u64,
}

// ── Tier C — Trending KUs ──────────────────────────────────────────────

/// A trending KU.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendingKu {
    /// The KU summary.
    pub ku: KuListItem,
    /// Trending score (higher = more trending).
    pub trend_score: f64,
    /// Reason for trending (e.g., "high_pomv", "recent_access", "most_bonds").
    pub reason: String,
}

// ── Tier C — Recommendations ───────────────────────────────────────────

/// A recommended KU.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendedKu {
    /// The KU summary.
    pub ku: KuListItem,
    /// Relevance score (0.0-1.0).
    pub relevance: f64,
    /// Reason for recommendation.
    pub reason: String,
}

// ── Tier C — Analytics ─────────────────────────────────────────────────

/// Analytics snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsSnapshot {
    /// Total KU count.
    pub total_kus: usize,
    /// KUs per gene type.
    pub kus_by_type: Vec<(String, usize)>,
    /// Average PoMV score.
    pub avg_pomv: f64,
    /// Average trust score.
    pub avg_trust: f64,
    /// Total wire size (bytes).
    pub total_wire_size: u64,
    /// Number of unique bonds.
    pub total_bonds: usize,
    /// KUs encoded in last 24h.
    pub kus_last_24h: usize,
    /// KUs encoded in last 7d.
    pub kus_last_7d: usize,
    /// Top gene type.
    pub top_gene_type: String,
}

// ── Tier C — Domain Taxonomy ───────────────────────────────────────────

/// A knowledge domain group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainInfo {
    /// Domain name (based on gene_type grouping).
    pub name: String,
    /// Number of KUs in this domain.
    pub ku_count: usize,
    /// Average PoMV in this domain.
    pub avg_pomv: f64,
    /// Example KU CIDs.
    pub example_cids: Vec<String>,
}
