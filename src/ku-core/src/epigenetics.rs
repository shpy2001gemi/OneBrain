//! Epigenetics Layer — Runtime metadata (Layer 2 of KU 3-layer architecture).
//!
//! This layer contains ALL runtime metadata that is NOT persisted in Core DNA:
//! - Trust scores (PoMV 6 signals)
//! - Relation bonds (33 types)
//! - Epistemic status transitions
//! - Temporal/embedding metadata
//!
//! # Biological Analogy
//! Epigenetic marks (methylation, histone modifications) regulate gene expression
//! without altering the DNA sequence. Similarly, the Epigenetics layer modifies
//! how a KU is perceived (trust, relevance, connections) without changing its
//! Core DNA content.
//!
//! # Persistence
//! Epigenetics data is stored separately from Core DNA (e.g., in SQLite),
//! serialized as CBOR via serde. It is NOT included in the Core DNA wire bytes
//! and therefore does NOT affect the CID (content identity).

use crate::types::{
    Bond, ConceptId, Creator, DecayRate, EdgeState, EpigeneticSection,
    EpistemicStatus, EvidenceType, RelationType, TrustSection,
};
use serde::{Deserialize, Serialize};

// ============================================================================
// Epigenetics — Runtime metadata composite
// ============================================================================

/// Layer 2: Epigenetics — runtime metadata overlay.
///
/// Contains all mutable, non-content data associated with a KU:
/// - Trust section with PoMV 6-signal scores (includes epistemic_status, evidence_type)
/// - Relation bonds to other KUs
/// - Optional embedding/temporal metadata
///
/// Epistemic status and evidence type live in `trust` (TrustSection) — the single
/// source of truth for all quality/reputation data.
///
/// This struct is serialized separately from Core DNA (e.g., to SQLite as CBOR).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Epigenetics {
    /// Trust & PoMV scores — the core quality/reputation data.
    /// Contains: trust_score, confidence, 6 PoMV signals, verification_level,
    /// epistemic_status, evidence_type, etc.
    #[serde(rename = "tr")]
    pub trust: TrustSection,

    /// Directed relation bonds to other KUs (33 types across 8 categories).
    /// Bonds are runtime-only — they are discovered through network interaction,
    /// not encoded in Core DNA.
    #[serde(rename = "bn", skip_serializing_if = "Vec::is_empty", default)]
    pub bonds: Vec<Bond>,

    /// Rich metadata: embeddings, temporal validity, versioning, categories.
    /// Optional because new KUs may not yet have embedding/temporal data.
    #[serde(rename = "ep", skip_serializing_if = "Option::is_none", default)]
    pub epigenetic: Option<EpigeneticSection>,
}

impl Default for Epigenetics {
    fn default() -> Self {
        Self {
            trust: TrustSection::default(),
            bonds: Vec::new(),
            epigenetic: None,
        }
    }
}

impl Epigenetics {
    /// Create with initial trust score and confidence.
    pub fn with_trust(trust_score: u16, confidence: u16) -> Self {
        let mut epi = Self::default();
        epi.trust.trust_score = trust_score;
        epi.trust.confidence = confidence;
        epi
    }

    /// Create with specific epistemic status.
    pub fn with_status(status: EpistemicStatus) -> Self {
        let mut epi = Self::default();
        epi.trust.epistemic_status = status;
        epi
    }

    /// Add a bond to another KU.
    pub fn add_bond(
        &mut self,
        target_cid: Vec<u8>,
        relation: RelationType,
        weight: u16,
    ) {
        self.bonds.push(Bond {
            target_cid,
            relation,
            weight,
            creator: Creator::System,
            created_at: 0, // TODO: use real timestamp
            evidence: Vec::new(),
            state: EdgeState::Active,
            initial_weight: Some(weight),
            decay: Some(DecayRate::None),
            last_reinforced: None,
            reinforce_count: None,
            bidirectional: None,
            context: Vec::new(),
            order: None,
            required: None,
        });
    }

    /// Get the PoMV composite score (weighted average of 6 signals).
    pub fn pomv_score(&self) -> f64 {
        let t = &self.trust;
        let scores = [
            (t.metabolic_rate as f64, 0.35),
            (t.prediction_score as f64, 0.15),
            (t.entropy_at_creation as f64, 0.10),
            (t.survival_score as f64, 0.10),
            (t.synaptic_centrality as f64, 0.15),
            (t.niche_fitness as f64, 0.15),
        ];
        scores.iter().map(|(s, w)| s * w).sum::<f64>() / 10000.0
    }
}

// ============================================================================
// Expression Layer — Generated on-demand (Layer 3)
// ============================================================================

/// Layer 3: Expression — natural language rendering of Core DNA.
///
/// Generated on-demand from CoreDna instructions + ConceptDict.
/// Not persisted — regenerated when needed.
///
/// # Biological Analogy
/// Gene expression produces proteins (phenotype) from DNA. Similarly,
/// the Expression layer produces human-readable text from Core DNA opcodes.
#[derive(Debug, Clone, PartialEq)]
pub struct Expression {
    /// Rendered natural language text.
    pub text: String,

    /// Language code (ISO 639-1: "vi", "en", "ja", etc.).
    pub lang: String,

    /// Cached concept name lookups used during rendering.
    /// Maps ConceptId → human-readable name in the target language.
    pub concept_names: Vec<(ConceptId, String)>,
}

impl Expression {
    /// Create a new expression with rendered text.
    pub fn new(text: String, lang: String) -> Self {
        Self {
            text,
            lang,
            concept_names: Vec::new(),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_epigenetics_default() {
        let epi = Epigenetics::default();
        assert_eq!(epi.trust.trust_score, 0);
        assert_eq!(epi.trust.epistemic_status, EpistemicStatus::Rumor);
        assert!(epi.bonds.is_empty());
        assert!(epi.epigenetic.is_none());
    }

    #[test]
    fn test_epigenetics_with_trust() {
        let epi = Epigenetics::with_trust(7000, 8000);
        assert_eq!(epi.trust.trust_score, 7000);
        assert_eq!(epi.trust.confidence, 8000);
    }

    #[test]
    fn test_epigenetics_with_status() {
        let epi = Epigenetics::with_status(EpistemicStatus::Observation);
        assert_eq!(epi.trust.epistemic_status, EpistemicStatus::Observation);
    }

    #[test]
    fn test_add_bond() {
        let mut epi = Epigenetics::default();
        epi.add_bond(vec![0u8; 32], RelationType::Extends, 8000);
        assert_eq!(epi.bonds.len(), 1);
        assert_eq!(epi.bonds[0].relation, RelationType::Extends);
        assert_eq!(epi.bonds[0].weight, 8000);
    }

    #[test]
    fn test_pomv_score() {
        let mut epi = Epigenetics::default();
        epi.trust.metabolic_rate = 5000;
        epi.trust.prediction_score = 5000;
        epi.trust.entropy_at_creation = 5000;
        epi.trust.survival_score = 5000;
        epi.trust.synaptic_centrality = 5000;
        epi.trust.niche_fitness = 5000;
        let score = epi.pomv_score();
        assert!((score - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_expression_new() {
        let expr = Expression::new("Nước sôi ở 100 độ C".into(), "vi".into());
        assert_eq!(expr.lang, "vi");
        assert!(expr.concept_names.is_empty());
    }

    #[test]
    fn test_epigenetics_serialization() {
        let epi = Epigenetics::with_trust(5000, 6000);
        let json = serde_json::to_string(&epi).unwrap();
        let decoded: Epigenetics = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.trust.trust_score, 5000);
        assert_eq!(decoded.trust.confidence, 6000);
    }

    #[test]
    fn test_epigenetics_forward_compatible_deserialization() {
        // Simulate older JSON missing some fields — serde(default) should handle it
        let minimal_json = r#"{"tr":{"es":"Rumor","et":"None","vl":0,"cc":0,"ch":0,"er":0,"ts":5000,"cf":6000}}"#;
        let decoded: Epigenetics = serde_json::from_str(minimal_json).unwrap();
        assert_eq!(decoded.trust.trust_score, 5000);
        assert!(decoded.bonds.is_empty());
        assert_eq!(decoded.trust.epistemic_status, EpistemicStatus::Rumor);
        assert_eq!(decoded.trust.evidence_type, EvidenceType::default());
        assert!(decoded.epigenetic.is_none());
    }
}
