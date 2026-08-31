//! UKRL Type definitions — v4/v5 legacy types + v6/v7 shared types (Trust, Bonds, Epigenetics, GeneType)
//!
//! v7 additions: Normative (type 11), Definition (type 12) gene types. 13 total.
//! v5 additions: Composite Gene (type 10), Bond order/required fields,
//! u32 PAYLOAD_LEN in wire header.
//!
//! All types derived from the v4 specification:
//! - ConceptId: varint-encoded u64 (4-tier resolution)
//! - RoleId: 14 semantic roles  
//! - GeneType: 13 gene variants (0-6 direct, 7=EXTENDED + ext byte)
//! - EpistemicStatus: 11-level epistemic classification
//! - EvidenceType: 9 evidence types (Cochrane/GRADE pyramid)
//! - RelationType: 34 edge types across 8 categories
//! - TrustSection: Trust & epistemic metadata (v4 spec §8)
//! - EpigeneticSection: Layer 4 metadata (v4 spec §6)
//! - Codon, Bond, Gene, KnowledgeUnit structs

use serde::{Deserialize, Serialize};

/// Serde helper: skip serializing u16 fields when value is 0 (PoK v2 backward compat)
fn is_zero_u16(v: &u16) -> bool {
    *v == 0
}

// ============================================================================
// Layer 1: Concept Codons
// ============================================================================

/// ConceptID — language-agnostic concept identifier.
///
/// Varint-encoded on wire (1-5 bytes):
/// - Tier 0 (1B): 0-127 → 128 universal primitives
/// - Tier 1 (2B): 128-16,511 → ~16K common concepts
/// - Tier 2 (3B): 16,512-2,113,663 → ~2M standard concepts
/// - Tier 3 (4-5B): 2,113,664+ → extended/community concepts
pub type ConceptId = u64;

/// RoleID — semantic role of a codon within a KU.
///
/// From v4 spec: 14 roles including v4-new COMPOUND_HEAD and COMPOUND_MOD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum RoleId {
    Agent = 0x01,
    Object = 0x02,
    Tool = 0x03,
    Location = 0x04,
    Time = 0x05,
    Cause = 0x06,
    Result = 0x07,
    Manner = 0x08,
    Condition = 0x09,
    Quantity = 0x0A,
    Quality = 0x0B,
    Purpose = 0x0C,
    /// ★ v4 NEW — head of a compound concept
    CompoundHead = 0x0D,
    /// ★ v4 NEW — modifier of a compound concept
    CompoundMod = 0x0E,
}

impl RoleId {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x01 => Some(Self::Agent),
            0x02 => Some(Self::Object),
            0x03 => Some(Self::Tool),
            0x04 => Some(Self::Location),
            0x05 => Some(Self::Time),
            0x06 => Some(Self::Cause),
            0x07 => Some(Self::Result),
            0x08 => Some(Self::Manner),
            0x09 => Some(Self::Condition),
            0x0A => Some(Self::Quantity),
            0x0B => Some(Self::Quality),
            0x0C => Some(Self::Purpose),
            0x0D => Some(Self::CompoundHead),
            0x0E => Some(Self::CompoundMod),
            _ => None,
        }
    }
}

/// Qualifier — key-value pair on a codon (e.g. unit=DEGREE).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Qualifier {
    pub key: String,
    pub value: QualifierValue,
}

/// Qualifier value — either a concept reference or a raw integer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum QualifierValue {
    Concept(ConceptId),
    Integer(i64),
    Text(String),
}

/// Codon — smallest semantic unit, completely language-agnostic.
///
/// ```cbor
/// {"c": varint, "r": u8, "q": {}?}
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Codon {
    /// ConceptID (varint on wire, 1-5 bytes)
    #[serde(rename = "c")]
    pub concept_id: ConceptId,
    /// Semantic role
    #[serde(rename = "r")]
    pub role: RoleId,
    /// Optional qualifiers
    #[serde(rename = "q", skip_serializing_if = "Vec::is_empty", default)]
    pub qualifiers: Vec<Qualifier>,
}

// ============================================================================
// Layer 2: Relation Bonds — 34 Edge Types
// ============================================================================

/// RelationType — 34 edge types across 8 categories.
///
/// Categories: A=Epistemic, B=Structural, C=Causal, D=Derivation,
///             E=Similarity, F=Temporal, G=Provenance, H=Experiential(★v4)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum RelationType {
    // Category A: Epistemic (0x01-0x06)
    Extends = 0x01,
    Supplements = 0x02,
    Refutes = 0x03,
    Corroborates = 0x04,
    Supersedes = 0x05,
    Qualifies = 0x06,

    // Category B: Structural (0x10-0x13)
    PartOf = 0x10,
    InstanceOf = 0x11,
    Specializes = 0x12,
    Generalizes = 0x13,

    // Category C: Causal (0x20-0x23)
    Causes = 0x20,
    Enables = 0x21,
    Prevents = 0x22,
    DependsOn = 0x23,

    // Category D: Derivation (0x30-0x33)
    ExampleOf = 0x30,
    AnalogyOf = 0x31,
    AppliesTo = 0x32,
    DerivedFrom = 0x33,

    // Category E: Similarity (0x40-0x43)
    Duplicates = 0x40,
    Translates = 0x41,
    Paraphrases = 0x42,
    Inspires = 0x43,

    // Category F: Temporal (0x50-0x51)
    Precedes = 0x50,
    Cooccurs = 0x51,

    // Category G: Provenance (0x60-0x62)
    Cites = 0x60,
    AuthoredBy = 0x61,
    ReviewedBy = 0x62,

    // Category H: Experiential — ★ v4 NEW (0x70-0x76)
    ReactionTo = 0x70,
    TestimonyAbout = 0x71,
    FormallyProves = 0x72,
    EvolvesInto = 0x73,
    VariantOf = 0x74,
    SensoryEvidenceFor = 0x75,
    CulturallyContextualizes = 0x76,
}

impl RelationType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x01 => Some(Self::Extends),
            0x02 => Some(Self::Supplements),
            0x03 => Some(Self::Refutes),
            0x04 => Some(Self::Corroborates),
            0x05 => Some(Self::Supersedes),
            0x06 => Some(Self::Qualifies),
            0x10 => Some(Self::PartOf),
            0x11 => Some(Self::InstanceOf),
            0x12 => Some(Self::Specializes),
            0x13 => Some(Self::Generalizes),
            0x20 => Some(Self::Causes),
            0x21 => Some(Self::Enables),
            0x22 => Some(Self::Prevents),
            0x23 => Some(Self::DependsOn),
            0x30 => Some(Self::ExampleOf),
            0x31 => Some(Self::AnalogyOf),
            0x32 => Some(Self::AppliesTo),
            0x33 => Some(Self::DerivedFrom),
            0x40 => Some(Self::Duplicates),
            0x41 => Some(Self::Translates),
            0x42 => Some(Self::Paraphrases),
            0x43 => Some(Self::Inspires),
            0x50 => Some(Self::Precedes),
            0x51 => Some(Self::Cooccurs),
            0x60 => Some(Self::Cites),
            0x61 => Some(Self::AuthoredBy),
            0x62 => Some(Self::ReviewedBy),
            0x70 => Some(Self::ReactionTo),
            0x71 => Some(Self::TestimonyAbout),
            0x72 => Some(Self::FormallyProves),
            0x73 => Some(Self::EvolvesInto),
            0x74 => Some(Self::VariantOf),
            0x75 => Some(Self::SensoryEvidenceFor),
            0x76 => Some(Self::CulturallyContextualizes),
            _ => None,
        }
    }

    /// Returns the category letter (A-H) for this edge type.
    pub fn category(&self) -> char {
        match *self as u8 {
            0x01..=0x06 => 'A',
            0x10..=0x13 => 'B',
            0x20..=0x23 => 'C',
            0x30..=0x33 => 'D',
            0x40..=0x43 => 'E',
            0x50..=0x51 => 'F',
            0x60..=0x62 => 'G',
            0x70..=0x76 => 'H',
            _ => '?',
        }
    }

    /// ★ OBKG Fix: Match a string name (case-insensitive) to this RelationType.
    /// Used by KQL edge pattern matching instead of fragile Debug format comparison.
    pub fn matches_name(&self, name: &str) -> bool {
        Self::from_name(name) == Some(*self)
    }

    /// ★ OBKG Fix: Parse a relation type name (case-insensitive).
    pub fn from_name(name: &str) -> Option<Self> {
        // All 34 variants — case-insensitive match
        let lower = name.to_ascii_lowercase();
        match lower.as_str() {
            "extends" => Some(Self::Extends),
            "supplements" => Some(Self::Supplements),
            "refutes" => Some(Self::Refutes),
            "corroborates" => Some(Self::Corroborates),
            "supersedes" => Some(Self::Supersedes),
            "qualifies" => Some(Self::Qualifies),
            "partof" | "part_of" => Some(Self::PartOf),
            "instanceof" | "instance_of" => Some(Self::InstanceOf),
            "specializes" => Some(Self::Specializes),
            "generalizes" => Some(Self::Generalizes),
            "causes" => Some(Self::Causes),
            "enables" => Some(Self::Enables),
            "prevents" => Some(Self::Prevents),
            "dependson" | "depends_on" => Some(Self::DependsOn),
            "exampleof" | "example_of" => Some(Self::ExampleOf),
            "analogyof" | "analogy_of" => Some(Self::AnalogyOf),
            "appliesto" | "applies_to" => Some(Self::AppliesTo),
            "derivedfrom" | "derived_from" => Some(Self::DerivedFrom),
            "duplicates" => Some(Self::Duplicates),
            "translates" => Some(Self::Translates),
            "paraphrases" => Some(Self::Paraphrases),
            "inspires" => Some(Self::Inspires),
            "precedes" => Some(Self::Precedes),
            "cooccurs" | "co_occurs" => Some(Self::Cooccurs),
            "cites" => Some(Self::Cites),
            "authoredby" | "authored_by" => Some(Self::AuthoredBy),
            "reviewedby" | "reviewed_by" => Some(Self::ReviewedBy),
            "reactionto" | "reaction_to" => Some(Self::ReactionTo),
            "testimonyabout" | "testimony_about" => Some(Self::TestimonyAbout),
            "formallyproves" | "formally_proves" => Some(Self::FormallyProves),
            "evolvesinto" | "evolves_into" => Some(Self::EvolvesInto),
            "variantof" | "variant_of" => Some(Self::VariantOf),
            "sensoryevidencefor" | "sensory_evidence_for" => Some(Self::SensoryEvidenceFor),
            "culturallycontextualizes" | "culturally_contextualizes" => {
                Some(Self::CulturallyContextualizes)
            }
            _ => None,
        }
    }
}

impl std::fmt::Display for RelationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Edge State — lifecycle of a bond.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum EdgeState {
    #[default]
    Active = 0,
    Weakened = 1,
    Deprecated = 2,
}

/// Decay rate classification for edge weight decay.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum DecayRate {
    #[default]
    None = 0, // Never decays
    Slow = 1, // Half-life 1 year
    Med = 2,  // Half-life 3 months
    Fast = 3, // Half-life 1 week
}

/// Creator enum — who created this edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Creator {
    Human = 0,
    Ai = 1,
    System = 2,
    Hybrid = 3,
}

/// Bond — a directed edge to another KU.
///
/// ```cbor
/// {"t": CID, "r": u8, "w": float16, "cr": u8, "ts": u32, "state": u8, ...}
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bond {
    /// Target KU CID (SHA-256 based, 36 bytes)
    #[serde(rename = "t", with = "serde_bytes")]
    pub target_cid: Vec<u8>,
    /// Relation type
    #[serde(rename = "r")]
    pub relation: RelationType,
    /// Weight [0, 10000] stored as u16, represents [0.0, 1.0] scaled by 10000
    #[serde(rename = "w")]
    pub weight: u16,
    /// Creator
    #[serde(rename = "cr")]
    pub creator: Creator,
    /// Created-at timestamp (unix seconds, u32)
    #[serde(rename = "ts")]
    pub created_at: u32,
    /// Evidence CIDs (optional)
    #[serde(rename = "ev", skip_serializing_if = "Vec::is_empty", default)]
    pub evidence: Vec<Vec<u8>>,
    /// Edge lifecycle state — ★ v4
    #[serde(rename = "s", default)]
    pub state: EdgeState,
    /// Initial weight (for decay computation)
    #[serde(rename = "w0", skip_serializing_if = "Option::is_none", default)]
    pub initial_weight: Option<u16>,
    /// Decay rate class
    #[serde(rename = "dk", skip_serializing_if = "Option::is_none", default)]
    pub decay: Option<DecayRate>,
    /// Last reinforcement timestamp
    #[serde(rename = "lr", skip_serializing_if = "Option::is_none", default)]
    pub last_reinforced: Option<u32>,
    /// Number of reinforcements
    #[serde(rename = "rc", skip_serializing_if = "Option::is_none", default)]
    pub reinforce_count: Option<u8>,
    /// Bidirectional flag
    #[serde(rename = "bi", skip_serializing_if = "Option::is_none", default)]
    pub bidirectional: Option<bool>,
    /// Context concept IDs
    #[serde(rename = "ctx", skip_serializing_if = "Vec::is_empty", default)]
    pub context: Vec<ConceptId>,

    /// ★ v5: Ordering hint within a Composite KU (0-based position)
    #[serde(rename = "od", skip_serializing_if = "Option::is_none", default)]
    pub order: Option<u16>,

    /// ★ v5: Is this bond required for structural integrity?
    #[serde(rename = "rq", skip_serializing_if = "Option::is_none", default)]
    pub required: Option<bool>,
}

// ============================================================================
// Layer 3: Content Genes — 11 Gene Types
// ============================================================================

/// GeneType — 13 types from v4/v5/v7 spec.
///
/// Wire encoding: bits 5-7 of FLAGS byte for types 0-6,
/// type 7 = EXTENDED → read gene_type_ext from first payload byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum GeneType {
    Fact = 0,
    Procedure = 1,
    Experience = 2,
    Creative = 3,
    MediaExperience = 4, // ★ v4 NEW
    Testimony = 5,       // ★ v4 NEW
    Formal = 6,          // ★ v4 NEW
    Hypothesis = 7,      // ★ v4 NEW (EXTENDED 0x00)
    Narrative = 8,       // ★ v4 NEW (EXTENDED 0x01)
    Sensory = 9,         // ★ v4 NEW (EXTENDED 0x02)
    Composite = 10,      // ★ v5 NEW (EXTENDED 0x03)
    Normative = 11,      // ★ v7 NEW (EXTENDED 0x04)
    Definition = 12,     // ★ v7 NEW (EXTENDED 0x05)
}

impl GeneType {
    /// Returns (base_type for FLAGS bits 5-7, optional ext byte).
    pub fn wire_encoding(&self) -> (u8, Option<u8>) {
        match self {
            Self::Fact => (0, None),
            Self::Procedure => (1, None),
            Self::Experience => (2, None),
            Self::Creative => (3, None),
            Self::MediaExperience => (4, None),
            Self::Testimony => (5, None),
            Self::Formal => (6, None),
            Self::Hypothesis => (7, Some(0x00)),
            Self::Narrative => (7, Some(0x01)),
            Self::Sensory => (7, Some(0x02)),
            Self::Composite => (7, Some(0x03)),
            Self::Normative => (7, Some(0x04)),
            Self::Definition => (7, Some(0x05)),
        }
    }

    pub fn from_wire(base: u8, ext: Option<u8>) -> Option<Self> {
        match (base, ext) {
            (0, _) => Some(Self::Fact),
            (1, _) => Some(Self::Procedure),
            (2, _) => Some(Self::Experience),
            (3, _) => Some(Self::Creative),
            (4, _) => Some(Self::MediaExperience),
            (5, _) => Some(Self::Testimony),
            (6, _) => Some(Self::Formal),
            (7, Some(0x00)) => Some(Self::Hypothesis),
            (7, Some(0x01)) => Some(Self::Narrative),
            (7, Some(0x02)) => Some(Self::Sensory),
            (7, Some(0x03)) => Some(Self::Composite),
            (7, Some(0x04)) => Some(Self::Normative),
            (7, Some(0x05)) => Some(Self::Definition),
            _ => None,
        }
    }
}

/// EpistemicStatus — 11-level epistemic classification (★ v4 Trust layer).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum EpistemicStatus {
    #[default]
    Rumor = 0x00,
    Hearsay = 0x01,
    Testimony = 0x02,
    Observation = 0x03,
    Hypothesis = 0x04,
    Evidence = 0x05,
    Corroborated = 0x06,
    PeerReviewed = 0x07,
    Consensus = 0x08,
    FormallyProven = 0x09,
    Axiomatic = 0x0A,
}

/// EvidenceType — 9 types aligned with Cochrane/GRADE pyramid.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum EvidenceType {
    #[default]
    None = 0x00,
    Anecdotal = 0x01,
    CaseStudy = 0x02,
    Observational = 0x03,
    Correlational = 0x04,
    Experimental = 0x05,
    MetaAnalysis = 0x06,
    FormalProof = 0x07,
    Computational = 0x08,
}

impl EpistemicStatus {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(Self::Rumor),
            0x01 => Some(Self::Hearsay),
            0x02 => Some(Self::Testimony),
            0x03 => Some(Self::Observation),
            0x04 => Some(Self::Hypothesis),
            0x05 => Some(Self::Evidence),
            0x06 => Some(Self::Corroborated),
            0x07 => Some(Self::PeerReviewed),
            0x08 => Some(Self::Consensus),
            0x09 => Some(Self::FormallyProven),
            0x0A => Some(Self::Axiomatic),
            _ => None,
        }
    }
}

impl EvidenceType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(Self::None),
            0x01 => Some(Self::Anecdotal),
            0x02 => Some(Self::CaseStudy),
            0x03 => Some(Self::Observational),
            0x04 => Some(Self::Correlational),
            0x05 => Some(Self::Experimental),
            0x06 => Some(Self::MetaAnalysis),
            0x07 => Some(Self::FormalProof),
            0x08 => Some(Self::Computational),
            _ => None,
        }
    }
}

// ============================================================================
// Trust & Epistemic Layer (v4 spec §8)
// ============================================================================

/// Trust & Epistemic metadata section (v4 spec §8)
///
/// Comprehensive trust framework replacing the single `certainty: float16` from v3.
/// Size: ~19 bytes mandatory, ~60-100 bytes with optional fields.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TrustSection {
    /// Epistemic status level (0x00-0x0A)
    #[serde(rename = "es")]
    pub epistemic_status: EpistemicStatus,

    /// Evidence type (0x00-0x08)
    #[serde(rename = "et")]
    pub evidence_type: EvidenceType,

    /// Verification level: 0=none, 1=self, 2=peer, 3=expert, 4=formal
    #[serde(rename = "vl")]
    pub verification_level: u8,

    /// Number of independent corroborations
    #[serde(rename = "cc")]
    pub corroboration_count: u16,

    /// Number of challenges/refutations received
    #[serde(rename = "ch")]
    pub challenge_count: u16,

    /// Error susceptibility flags (bitfield, 16 independent flags)
    /// bit 0: EYEWITNESS_MEMORY, bit 1: SINGLE_SOURCE, bit 2: NO_INSTRUMENT,
    /// bit 3: EMOTIONAL_STATE, bit 4: SELF_REPORTED, bit 5: SELECTION_BIAS,
    /// bit 6: CONFIRMATION_BIAS, bit 7: TEMPORAL_DISTANCE, bit 8: CULTURAL_SPECIFIC,
    /// bit 9: TRANSLATION_LOSS, bit 10: CORRELATION_NOT_CAUSE, bit 11: SMALL_SAMPLE,
    /// bit 12: UNFALSIFIABLE, bit 13: CONFLICT_OF_INTEREST, bit 14: AI_GENERATED,
    /// bit 15: SUPERSEDED_METHOD
    #[serde(rename = "er")]
    pub error_susceptibility: u16,

    /// Computed trust score [0, 10000] (represents 0.0-1.0)
    #[serde(rename = "ts")]
    pub trust_score: u16,

    /// Confidence in the trust score [0, 10000]
    #[serde(rename = "cf")]
    pub confidence: u16,

    /// Domain expertise codes (ConceptIDs of relevant domains)
    #[serde(rename = "dc", default, skip_serializing_if = "Vec::is_empty")]
    pub domain_codes: Vec<u64>,

    /// CIDs of verification KUs (optional)
    #[serde(rename = "vk", default, skip_serializing_if = "Vec::is_empty")]
    pub verifications: Vec<Vec<u8>>,

    /// CIDs of challenge KUs (optional)
    #[serde(rename = "ck", default, skip_serializing_if = "Vec::is_empty")]
    pub challenges: Vec<Vec<u8>>,

    // === PoK v2: Proof-of-Metabolic-Value (PoMV) signals ===
    /// Current metabolic rate [0, 10000] — measures usage/activity
    #[serde(rename = "mr", default, skip_serializing_if = "is_zero_u16")]
    pub metabolic_rate: u16,

    /// Prediction accuracy score [0, 10000]
    #[serde(rename = "ps", default, skip_serializing_if = "is_zero_u16")]
    pub prediction_score: u16,

    /// Novelty/entropy at creation time [0, 10000]
    #[serde(rename = "en", default, skip_serializing_if = "is_zero_u16")]
    pub entropy_at_creation: u16,

    /// Battle-hardened survival bonus [0, 10000]
    #[serde(rename = "sv", default, skip_serializing_if = "is_zero_u16")]
    pub survival_score: u16,

    /// Synaptic centrality — network position value [0, 10000]
    #[serde(rename = "sc", default, skip_serializing_if = "is_zero_u16")]
    pub synaptic_centrality: u16,

    /// Ecological niche fitness [0, 10000]
    #[serde(rename = "nf", default, skip_serializing_if = "is_zero_u16")]
    pub niche_fitness: u16,
}

// ============================================================================
// Layer 4: Epigenetic Metadata (v4 spec §6)
// ============================================================================

/// Layer 4: Epigenetic metadata (v4 spec §6)
///
/// Contains semantic embeddings (int8[512] + binary[1024]), temporal validity,
/// knowledge maturity (KRL), cultural context, and rendering hints.
/// Size: variable, ~640-900 bytes typical.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EpigeneticSection {
    /// Semantic embedding (int8[512] stored as bytes)
    #[serde(
        rename = "em",
        with = "serde_bytes",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub embedding: Vec<u8>, // 512 bytes

    /// Binary embedding for fast screening (binary[1024] = 128 bytes)
    #[serde(
        rename = "eb",
        with = "serde_bytes",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub embedding_binary: Vec<u8>, // 128 bytes

    /// Embedding model version
    #[serde(rename = "ev", default, skip_serializing_if = "Option::is_none")]
    pub embed_version: Option<u16>,

    /// Valid-from timestamp (epoch seconds)
    #[serde(rename = "vf", default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<u64>,

    /// Valid-until timestamp (epoch seconds)
    #[serde(rename = "vu", default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<u64>,

    /// When this KU was recorded (bitemporal)
    #[serde(rename = "ra", default, skip_serializing_if = "Option::is_none")]
    pub recorded_at: Option<u64>,

    /// Temporal precision enum (v4 spec §6)
    /// 0=EXACT, 1=SECOND, 2=MINUTE, 3=HOUR, 4=DAY, 5=WEEK,
    /// 6=MONTH, 7=YEAR, 8=DECADE, 9=CENTURY, 10=MILLENNIUM
    #[serde(rename = "tp", default, skip_serializing_if = "Option::is_none")]
    pub temporal_precision: Option<u8>,

    /// Temporal uncertainty in seconds (±)
    #[serde(rename = "tu", default, skip_serializing_if = "Option::is_none")]
    pub temporal_uncertainty: Option<u32>,

    /// Half-life in seconds (knowledge decay)
    #[serde(rename = "hl", default, skip_serializing_if = "Option::is_none")]
    pub half_life: Option<u32>,

    /// Knowledge Readiness Level (0-9, NASA TRL-inspired)
    #[serde(rename = "kl", default, skip_serializing_if = "Option::is_none")]
    pub krl: Option<u8>,

    /// Language of content (ISO 639-1 numeric code)
    #[serde(rename = "lg", default, skip_serializing_if = "Option::is_none")]
    pub language: Option<u8>,

    /// Rendering template: 0=NARRATIVE, 1=STEP_BY_STEP, ...
    #[serde(rename = "rt", default, skip_serializing_if = "Option::is_none")]
    pub template: Option<u8>,

    /// Difficulty: 0=BEGINNER → 4=EXPERT
    #[serde(rename = "df", default, skip_serializing_if = "Option::is_none")]
    pub difficulty: Option<u8>,

    /// Category ConceptIDs for discovery
    #[serde(rename = "ct", default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<ConceptId>,

    /// Tag ConceptIDs for discovery
    #[serde(rename = "tg", default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<ConceptId>,

    /// SimHash (128-bit) for approximate duplicate detection
    #[serde(
        rename = "sh",
        with = "serde_bytes",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub simhash: Vec<u8>, // 16 bytes

    /// LSH bucket IDs for locality-sensitive hashing
    #[serde(
        rename = "lb",
        with = "serde_bytes",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub lsh_buckets: Vec<u8>, // 16 bytes

    /// Schema version
    #[serde(rename = "sv", default, skip_serializing_if = "Option::is_none")]
    pub schema_ver: Option<u16>,

    /// Content version (semantic)
    #[serde(rename = "cv", default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,

    /// Previous version CID
    #[serde(rename = "pv", default, skip_serializing_if = "Option::is_none")]
    pub prev_cid: Option<Vec<u8>>,

    /// Superseded-by CID
    #[serde(rename = "sb", default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<Vec<u8>>,
}

/// SPO Triple — Subject-Predicate-Object for FactGene/HypothesisGene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Triple {
    #[serde(rename = "s")]
    pub subject: ConceptId,
    #[serde(rename = "p")]
    pub predicate: ConceptId,
    #[serde(rename = "o")]
    pub object: ConceptId,
}

/// VAD Affect model — Valence, Arousal, Dominance.
/// Values stored as i16 scaled: [-10000, +10000] for valence, [0, 10000] for arousal/dominance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Affect {
    /// Valence: -10000 to +10000 (represents -1.0 to +1.0)
    pub v: i16,
    /// Arousal: 0 to 10000 (represents 0.0 to 1.0)
    pub a: i16,
    /// Dominance: 0 to 10000 (represents 0.0 to 1.0)
    pub d: i16,
}

/// Procedure step — used by ProcedureGene and CreativeGene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcedureStep {
    /// Step order
    pub ord: u16,
    /// Action ConceptID
    pub act: ConceptId,
    /// Preconditions
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub pre: Vec<Codon>,
    /// Target ConceptID
    pub tgt: ConceptId,
    /// Tool ConceptIDs
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tools: Vec<ConceptId>,
    /// Effects
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub eff: Vec<Codon>,
    /// Warnings
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warn: Vec<Codon>,
}

/// Canonical text — compressed original text with language code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalText {
    /// ISO 639-1 numeric language code
    pub lang: u8,
    /// Text content (would be Zstd compressed in production)
    #[serde(with = "serde_bytes")]
    pub text: Vec<u8>,
}

/// Perspective data for ExperienceGene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Perspective {
    /// Expertise level: 0=novice → 4=expert
    pub expertise: u8,
    /// 0=OBJECTIVE, 1=SUBJECTIVE, 2=INTERSUBJECTIVE, 3=CONTESTED
    #[serde(rename = "type")]
    pub perspective_type: u8,
}

// ============================================================================
// Composite Gene Support Types (★ v5 NEW)
// ============================================================================

/// Structural roles for CompositeEntry members.
///
/// Defines the hierarchical role of a member within a Composite KU.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum StructuralRole {
    /// Top-level division (e.g., a chapter in a book)
    Chapter = 0,
    /// Mid-level division (e.g., a section in a chapter)
    Section = 1,
    /// Fine-grained division (e.g., a subsection)
    Subsection = 2,
    /// Atomic knowledge unit (leaf node)
    #[default]
    Detail = 3,
    /// Supplementary material (e.g., appendix)
    Appendix = 4,
    /// External reference or bibliography entry
    Reference = 5,
    /// Index or table of contents
    Index = 6,
    /// Definitions and terminology
    Glossary = 7,
}

impl StructuralRole {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Chapter),
            1 => Some(Self::Section),
            2 => Some(Self::Subsection),
            3 => Some(Self::Detail),
            4 => Some(Self::Appendix),
            5 => Some(Self::Reference),
            6 => Some(Self::Index),
            7 => Some(Self::Glossary),
            _ => None,
        }
    }
}

/// Composite type hint — categorizes the purpose of a Composite KU.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum CompositeType {
    #[default]
    Document = 0,
    Chapter = 1,
    Section = 2,
    Collection = 3,
    Dataset = 4,
    Specification = 5,
    Protocol = 6,
    Custom = 7,
}

impl CompositeType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Document),
            1 => Some(Self::Chapter),
            2 => Some(Self::Section),
            3 => Some(Self::Collection),
            4 => Some(Self::Dataset),
            5 => Some(Self::Specification),
            6 => Some(Self::Protocol),
            7 => Some(Self::Custom),
            _ => None,
        }
    }
}

/// Completeness status for a Composite KU.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Completeness {
    #[default]
    Draft = 0,
    Partial = 1,
    Complete = 2,
    Verified = 3,
    Certified = 4,
}

impl Completeness {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Draft),
            1 => Some(Self::Partial),
            2 => Some(Self::Complete),
            3 => Some(Self::Verified),
            4 => Some(Self::Certified),
            _ => None,
        }
    }
}

/// A member entry within a Composite Gene.
///
/// Each entry references a child KU (by CID) with ordering,
/// role classification, and requirement metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositeEntry {
    /// Content ID of the member KU (BLAKE3, 32 bytes)
    #[serde(rename = "c", with = "serde_bytes")]
    pub cid: Vec<u8>,

    /// Position order within parent (0-based, ascending)
    #[serde(rename = "o")]
    pub order: u16,

    /// Structural role of this member
    #[serde(rename = "r")]
    pub role: StructuralRole,

    /// Is this member required for cluster completeness?
    #[serde(rename = "q")]
    pub required: bool,

    /// Human-readable label (ConceptId)
    #[serde(rename = "l")]
    pub label: ConceptId,

    /// Expected gene type of member (for validation)
    /// None = any type accepted
    #[serde(rename = "g", skip_serializing_if = "Option::is_none", default)]
    pub expected_gene_type: Option<u8>,
}

/// Cross-constraint between members of a Composite Gene.
///
/// Uses KQL-Lite conditions serialized as CBOR. These constraints
/// express relationships like "IF wing.sweep_angle > 20 THEN
/// airfoil.type = SUPERCRITICAL".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositeConstraint {
    /// Human-readable constraint name
    #[serde(rename = "n")]
    pub name: String,

    /// Source member CID (the "IF" side)
    #[serde(rename = "s", with = "serde_bytes")]
    pub source_cid: Vec<u8>,

    /// Target member CID (the "THEN" side)
    #[serde(rename = "t", with = "serde_bytes")]
    pub target_cid: Vec<u8>,

    /// KQL-Lite condition (serialized as CBOR bytes)
    /// Uses the Condition AST from ku-kql
    #[serde(rename = "c", with = "serde_bytes")]
    pub condition: Vec<u8>,

    /// Severity: 0=INFO, 1=WARNING, 2=ERROR, 3=CRITICAL
    #[serde(rename = "v")]
    pub severity: u8,
}

// ============================================================================
// Gene — 11 variants
// ============================================================================

fn default_max_depth() -> u8 {
    255
}

/// Gene — the content payload of a Knowledge Unit (Layer 3).
///
/// 11 variants matching GeneType enum, each with type-specific fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Gene {
    /// Type 0: Established factual knowledge (S-P-O triples).
    #[serde(rename = "fact")]
    Fact {
        triples: Vec<Triple>,
        /// Certainty: 0-10000 (represents 0.0-1.0)
        certainty: u16,
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        evidence: Vec<Vec<u8>>,
    },

    /// Type 1: Step-by-step procedures.
    #[serde(rename = "procedure")]
    Procedure {
        steps: Vec<ProcedureStep>,
        #[serde(skip_serializing_if = "Option::is_none")]
        total_time: Option<u32>,
        /// Difficulty: 0-4
        difficulty: u8,
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        tools_req: Vec<ConceptId>,
    },

    /// Type 2: Personal/sensory experiences with VAD affect.
    #[serde(rename = "experience")]
    Experience {
        scene: Vec<Codon>,
        affect: Affect,
        #[serde(skip_serializing_if = "Option::is_none")]
        canonical: Option<CanonicalText>,
        #[serde(skip_serializing_if = "Option::is_none")]
        perspective: Option<Perspective>,
    },

    /// Type 3: Creative knowledge (recipes, compositions, designs).
    #[serde(rename = "creative")]
    Creative {
        steps: Vec<ProcedureStep>,
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        cultural_context: Vec<ConceptId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        origin_story: Option<CanonicalText>,
    },

    /// Type 4: Media experience reactions (★ v4 NEW).
    #[serde(rename = "media_experience")]
    MediaExperience {
        /// External media reference system (0=WIKIDATA, 1=IMDB, etc.)
        id_sys: u8,
        /// External media ID
        #[serde(with = "serde_bytes")]
        ext_id: Vec<u8>,
        /// Media type (0=FILM, 1=SERIES, 2=BOOK, etc.)
        media_type: u8,
        /// Rating 0-100
        rating: u8,
        affect: Affect,
        /// Spoiler level: 0=NONE, 1=MILD, 2=MAJOR, 3=FULL_PLOT
        spoiler_level: u8,
    },

    /// Type 5: Testimony / witness reports (★ v4 NEW).
    #[serde(rename = "testimony")]
    Testimony {
        triples: Vec<Triple>,
        /// 0=SIGHTING, 1=EVENT, 2=PHENOMENON, ...
        claim_type: u8,
        /// 0=MUNDANE → 3=UNPRECEDENTED
        extraordinary: u8,
        /// Number of independent witnesses
        witness_count: u16,
        /// 0=FIRSTHAND, 1=SECONDHAND, 2=THIRDHAND, 3=HEARSAY
        proximity: u8,
        /// 0=UNVERIFIED → 4=INCONCLUSIVE
        verification_status: u8,
    },

    /// Type 6: Formal/mathematical knowledge (★ v4 NEW).
    #[serde(rename = "formal")]
    Formal {
        /// 0=MATH, 1=PHYSICS, 2=CHEMISTRY, etc.
        domain: u8,
        /// Notation format (0=LATEX, 1=MATHML, ...)
        notation_format: u8,
        /// Notation source (compressed)
        #[serde(with = "serde_bytes")]
        notation_source: Vec<u8>,
        /// Statement type (0=DEFINITION, 1=AXIOM, 2=THEOREM, etc.)
        statement_type: u8,
        /// Verification status (0=UNVERIFIED → 3=FORMALLY_PROVED)
        verification_status: u8,
    },

    /// Type 7: Hypotheses / draft knowledge (★ v4 NEW, EXTENDED 0x00).
    #[serde(rename = "hypothesis")]
    Hypothesis {
        /// Base gene type when mature (0=FACT, 1=PROC, etc.)
        base_type: u8,
        body_codons: Vec<Codon>,
        /// Maturity: 0=INTUITION → 7=REPLICATED
        maturity_level: u8,
        /// Confidence: 0-10000 (represents 0.0-1.0)
        confidence: u16,
        /// Completeness: 0-10000
        completeness: u16,
        /// Is this hypothesis falsifiable?
        falsifiable: bool,
    },

    /// Type 8: Narratives, myths, folktales (★ v4 NEW, EXTENDED 0x01).
    #[serde(rename = "narrative")]
    Narrative {
        /// 0=FOLKTALE, 1=MYTH, 2=LEGEND, ...
        narrative_type: u8,
        /// Cultural origin concept IDs
        origin_culture: Vec<ConceptId>,
        /// Era: 0=PREHISTORIC..6=TIMELESS
        era: u8,
        /// Function: 0=ENTERTAINMENT, 1=MORAL_TEACHING, ...
        function: u8,
        /// Is it sacred?
        sacred: bool,
        /// Moral of the story (codons)
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        moral: Vec<Codon>,
        #[serde(skip_serializing_if = "Option::is_none")]
        canonical: Option<CanonicalText>,
    },

    /// Type 9: Pure sensory data (★ v4 NEW, EXTENDED 0x02).
    #[serde(rename = "sensory")]
    Sensory {
        /// 0=VISUAL, 1=AUDITORY, 2=OLFACTORY, ...
        modality: u8,
        /// Property being observed (ConceptID)
        property: ConceptId,
        /// Feature of interest (ConceptID)
        feature: ConceptId,
        /// Observation codons
        result_codons: Vec<Codon>,
        /// Sensor type: 0=HUMAN_EYE, 1=HUMAN_EAR, 2=CAMERA, ...
        sensor_type: u8,
        /// Quality: 0=RAW, 1=PROCESSED, 2=VERIFIED, 3=CALIBRATED
        quality: u8,
    },

    /// Type 10: Composite — groups multiple KUs into a structured cluster (★ v5 NEW, EXTENDED 0x03).
    ///
    /// Bio-metaphor: A "chromosome" organizing multiple "genes" (KUs) into
    /// a coherent functional unit with regulatory constraints.
    ///
    /// CYCLES ARE IMPOSSIBLE: CID = BLAKE3(content) creates a Merkle DAG.
    /// To create a cycle, you'd need to know a parent's CID before computing
    /// it — but the child's CID is part of the parent's content. Hash paradox. ∎
    #[serde(rename = "composite")]
    Composite {
        /// Ordered list of member KU entries.
        /// Each member CID can point to ANY KU type — including another Composite.
        members: Vec<CompositeEntry>,
        /// Cross-constraints between members (KQL-Lite conditions)
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        constraints: Vec<CompositeConstraint>,
        /// Cluster version — incremented when any member changes
        cluster_version: u32,
        /// Soft advisory maximum depth for nested composites (default: 255)
        /// NOT enforced at protocol level.
        #[serde(default = "default_max_depth")]
        max_depth: u8,
        /// Composite type hint
        composite_type: CompositeType,
        /// Cluster schema identifier
        #[serde(skip_serializing_if = "Option::is_none", default)]
        schema: Option<ConceptId>,
        /// Completeness status
        completeness: Completeness,
        /// Summary codons (language-agnostic abstract)
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        summary_codons: Vec<Codon>,
    },
}

impl Gene {
    /// Returns the GeneType for this gene variant.
    pub fn gene_type(&self) -> GeneType {
        match self {
            Gene::Fact { .. } => GeneType::Fact,
            Gene::Procedure { .. } => GeneType::Procedure,
            Gene::Experience { .. } => GeneType::Experience,
            Gene::Creative { .. } => GeneType::Creative,
            Gene::MediaExperience { .. } => GeneType::MediaExperience,
            Gene::Testimony { .. } => GeneType::Testimony,
            Gene::Formal { .. } => GeneType::Formal,
            Gene::Hypothesis { .. } => GeneType::Hypothesis,
            Gene::Narrative { .. } => GeneType::Narrative,
            Gene::Sensory { .. } => GeneType::Sensory,
            Gene::Composite { .. } => GeneType::Composite,
        }
    }
}

// ============================================================================
// KnowledgeUnit — the complete unit (Layers 1-4 + Trust)
// ============================================================================

/// Wire format header flags.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HeaderFlags {
    pub has_ecc: bool,
    pub has_bbs: bool,
    pub has_merkle: bool,
    pub has_media: bool,
    pub is_encrypted: bool,
}

impl HeaderFlags {
    /// Pack flags + gene_type into a single u8.
    pub fn to_byte(&self, gene_type: GeneType) -> u8 {
        let (base, _) = gene_type.wire_encoding();
        let mut flags: u8 = 0;
        if self.has_ecc {
            flags |= 0x01;
        }
        if self.has_bbs {
            flags |= 0x02;
        }
        if self.has_merkle {
            flags |= 0x04;
        }
        if self.has_media {
            flags |= 0x08;
        }
        if self.is_encrypted {
            flags |= 0x10;
        }
        flags |= (base & 0x07) << 5;
        flags
    }

    /// Unpack from a flags byte.
    pub fn from_byte(byte: u8) -> (Self, u8) {
        let flags = Self {
            has_ecc: byte & 0x01 != 0,
            has_bbs: byte & 0x02 != 0,
            has_merkle: byte & 0x04 != 0,
            has_media: byte & 0x08 != 0,
            is_encrypted: byte & 0x10 != 0,
        };
        let gene_base = (byte >> 5) & 0x07;
        (flags, gene_base)
    }
}

/// KnowledgeUnit — complete KU with Layers 1-4 + Trust.
///
/// Wire format:
/// ```text
/// v4: MAGIC(0x4B44) | VERSION(0x04) | FLAGS(u8) | PAYLOAD_LEN(u16) | PAYLOAD | CRC32
/// v5: MAGIC(0x4B44) | VERSION(0x05) | FLAGS(u8) | PAYLOAD_LEN(u32) | PAYLOAD | CRC32
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeUnit {
    /// Layer 1: Concept Codons
    pub codons: Vec<Codon>,
    /// Layer 2: Relation Bonds
    pub bonds: Vec<Bond>,
    /// Layer 3: Content Gene
    pub gene: Gene,

    /// Header flags
    #[serde(default)]
    pub flags: HeaderFlags,

    /// Epistemic status (★ v4 Trust layer — kept for backward compat)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub epistemic_status: Option<EpistemicStatus>,
    /// Evidence type (kept for backward compat)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub evidence_type: Option<EvidenceType>,

    /// ★ v4: Full Trust & Epistemic section (spec §8)
    #[serde(rename = "tr", skip_serializing_if = "Option::is_none", default)]
    pub trust: Option<TrustSection>,

    /// ★ v4: Layer 4 Epigenetic metadata (spec §6)
    #[serde(rename = "ep", skip_serializing_if = "Option::is_none", default)]
    pub epigenetic: Option<EpigeneticSection>,
}

// ============================================================================
// Wire format constants
// ============================================================================

// ─── LEGACY v4/v5 wire constants ─────────────────────────────────────
// These are for the v4/v5 CBOR wire format ONLY.
// For v6 Core DNA format, use `core_dna::CORE_DNA_MAGIC` and `core_dna::CORE_DNA_VERSION`.

/// Magic bytes: "KD" = Knowledge DNA
pub const MAGIC: [u8; 2] = [0x4B, 0x44];
/// Version: v5 (Composite Gene + Bond Enhancement + u32 PAYLOAD_LEN)
pub const VERSION: u8 = 0x05;
/// Version: v4 (for backward compatibility decoding)
pub const VERSION_V4: u8 = 0x04;
