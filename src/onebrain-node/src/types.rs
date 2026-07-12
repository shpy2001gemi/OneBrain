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
