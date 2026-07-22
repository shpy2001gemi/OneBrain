//! # Types for the v2 encoding pipeline.
//!
//! Defines the data structures that flow through the pipeline:
//! ```text
//! AI JSON → SpoTriple → AnalyzedTriple → ResolvedTriple → CoreDna
//! ```

use ku_core::ccid::Ccid;
use ku_core::core_dna::Op;
use serde::{Deserialize, Serialize};

// ============================================================================
// SpoTriple — raw AI output
// ============================================================================

/// A single Subject-Predicate-Object triple extracted by AI from natural text.
///
/// This is the direct deserialization target of the AI's JSON output.
/// All string fields preserve the original language + English canonical form.
///
/// # Example JSON
/// ```json
/// {
///   "s": "bàn làm việc", "s_en": "desk",
///   "p": "có",
///   "o": "chân", "o_en": "leg",
///   "qty": 4,
///   "role": "part",
///   "c": "usually"
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpoTriple {
    /// Subject — original language.
    pub s: String,
    /// Subject — English canonical name (for ConceptRegistry lookup).
    pub s_en: String,
    /// Predicate — original language verb/relation.
    pub p: String,
    /// Object — original language.
    pub o: String,
    /// Object — English canonical name (for ConceptRegistry lookup).
    pub o_en: String,
    /// Quantity (optional) — AI extracts numbers from text (e.g., "4 chân" → qty=4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qty: Option<f64>,
    /// Semantic role — a universal linguistic concept (not internal schema).
    /// One of: "part", "material", "purpose", "location", "cause",
    ///         "property", "category", "formula", "relation"
    pub role: String,
    /// Notation type (only when role="formula").
    /// One of: "latex", "chemical", "smiles", "code"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notation: Option<String>,
    /// Certainty level — one of: "always", "usually", "sometimes", "rarely".
    pub c: String,
}

// ============================================================================
// ExtractionResult — output of AI extraction step
// ============================================================================

/// Result of AI extraction for a single paragraph.
#[derive(Debug, Clone)]
pub struct ExtractionResult {
    /// Extracted SPO triples.
    pub triples: Vec<SpoTriple>,
    /// Original paragraph text (for debugging/retry).
    pub source_paragraph: String,
}

// ============================================================================
// Anchor — pre-scanned terms to protect from AI modification
// ============================================================================

/// A term pre-scanned from the input text that must be preserved exactly.
///
/// AI models may "auto-correct" novel terms (e.g., H8O → H2O) due to
/// training data bias. Anchors are extracted by regex before AI processing,
/// then verified after AI output to catch any modifications.
#[derive(Debug, Clone, PartialEq)]
pub enum Anchor {
    /// Chemical formula: H8O, CH3COOH, NaCl, C6H12O6
    Formula(String),
    /// Numeric value with optional unit: 100°C, 3.14159, 9.8 m/s²
    Number(String),
    /// Mathematical expression: E=mc², F=ma, a²+b²=c²
    Math(String),
}

impl Anchor {
    /// Get the anchor string value.
    pub fn as_str(&self) -> &str {
        match self {
            Anchor::Formula(s) | Anchor::Number(s) | Anchor::Math(s) => s,
        }
    }
}

impl std::fmt::Display for Anchor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Anchor::Formula(s) => write!(f, "Formula({})", s),
            Anchor::Number(s) => write!(f, "Number({})", s),
            Anchor::Math(s) => write!(f, "Math({})", s),
        }
    }
}

// ============================================================================
// VerifyResult — anchor verification after AI extraction
// ============================================================================

/// Result of verifying that pre-scanned anchors survived AI processing.
#[derive(Debug, Clone, PartialEq)]
pub enum VerifyResult {
    /// All anchors found intact in AI output.
    Ok,
    /// An anchor is completely missing from AI output.
    Missing(String),
    /// An anchor was modified by AI (e.g., H8O → H2O).
    Modified { expected: String, actual: String },
}

// ============================================================================
// AnalyzedTriple — after code maps role + certainty
// ============================================================================

/// A triple after deterministic analysis (role → opcode, certainty → u16).
///
/// Produced by the analyzer from raw `SpoTriple`.
#[derive(Debug, Clone)]
pub struct AnalyzedTriple {
    /// Original AI-extracted triple.
    pub raw: SpoTriple,
    /// Mapped opcode (e.g., "part" → Op::PartOf, "formula" → Op::Formula).
    pub op: Op,
    /// Mapped certainty value (e.g., "usually" → 8000).
    pub certainty: u16,
}

// ============================================================================
// ResolvedTriple — after concept name → CCID resolution
// ============================================================================

/// A triple with all concepts resolved to CCIDs.
///
/// Produced by the concept resolver from `AnalyzedTriple`.
#[derive(Debug, Clone)]
pub struct ResolvedTriple {
    /// The analyzed triple (contains raw + op + certainty).
    pub analyzed: AnalyzedTriple,
    /// Resolved CCID for the subject concept.
    pub subject_ccid: Ccid,
    /// Resolved CCID for the object concept.
    /// `None` when role="formula" (formula string stored separately).
    pub object_ccid: Option<Ccid>,
    /// Resolved CCID for the predicate concept.
    pub predicate_ccid: Ccid,
    /// Formula string (only when role="formula", preserved verbatim).
    pub formula_string: Option<String>,
}

// ============================================================================
// Notation type mapping
// ============================================================================

/// Notation format byte for Formula instruction encoding.
///
/// Maps to `Instruction::Formula { format: u8, data: Vec<u8> }` in core_dna.rs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NotationType {
    /// Generic/unspecified notation.
    Generic = 0,
    /// Chemical formula/equation: H₈O, 2H₂ + O₂ → 2H₂O
    Chemical = 1,
    /// LaTeX mathematical notation: E = mc^2, \int_0^1 f(x)dx
    Latex = 2,
    /// SMILES molecular notation: CC(=O)O
    Smiles = 3,
    /// Code/algorithm snippet.
    Code = 4,
}

impl NotationType {
    /// Parse notation type from AI output string.
    pub fn from_str_opt(s: Option<&str>) -> Self {
        match s {
            Some("chemical") => Self::Chemical,
            Some("latex") => Self::Latex,
            Some("smiles") => Self::Smiles,
            Some("code") => Self::Code,
            _ => Self::Generic,
        }
    }

    /// Convert to u8 for binary encoding.
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_spo_triple_full() {
        let json = r#"{
            "s": "bàn làm việc", "s_en": "desk",
            "p": "có",
            "o": "chân", "o_en": "leg",
            "qty": 4,
            "role": "part",
            "c": "usually"
        }"#;
        let triple: SpoTriple = serde_json::from_str(json).unwrap();
        assert_eq!(triple.s, "bàn làm việc");
        assert_eq!(triple.s_en, "desk");
        assert_eq!(triple.o, "chân");
        assert_eq!(triple.o_en, "leg");
        assert_eq!(triple.qty, Some(4.0));
        assert_eq!(triple.role, "part");
        assert_eq!(triple.c, "usually");
        assert_eq!(triple.notation, None);
    }

    #[test]
    fn test_deserialize_spo_triple_no_qty() {
        let json = r#"{
            "s": "bàn", "s_en": "desk",
            "p": "làm bằng",
            "o": "gỗ", "o_en": "wood",
            "role": "material",
            "c": "usually"
        }"#;
        let triple: SpoTriple = serde_json::from_str(json).unwrap();
        assert_eq!(triple.qty, None);
        assert_eq!(triple.role, "material");
    }

    #[test]
    fn test_deserialize_spo_triple_formula() {
        let json = r#"{
            "s": "H8O", "s_en": "H8O",
            "p": "expressed as",
            "o": "H₈O", "o_en": "H₈O",
            "role": "formula",
            "notation": "chemical",
            "c": "always"
        }"#;
        let triple: SpoTriple = serde_json::from_str(json).unwrap();
        assert_eq!(triple.role, "formula");
        assert_eq!(triple.notation, Some("chemical".to_string()));
    }

    #[test]
    fn test_deserialize_array_of_triples() {
        let json = r#"[
            { "s": "nước", "s_en": "water", "p": "sôi ở", "o": "nhiệt độ", "o_en": "temperature", "qty": 100, "role": "property", "c": "always" },
            { "s": "nước", "s_en": "water", "p": "gồm", "o": "hydro", "o_en": "hydrogen", "qty": 2, "role": "part", "c": "always" }
        ]"#;
        let triples: Vec<SpoTriple> = serde_json::from_str(json).unwrap();
        assert_eq!(triples.len(), 2);
        assert_eq!(triples[0].qty, Some(100.0));
        assert_eq!(triples[1].qty, Some(2.0));
    }

    #[test]
    fn test_anchor_as_str() {
        let a = Anchor::Formula("H8O".to_string());
        assert_eq!(a.as_str(), "H8O");

        let b = Anchor::Number("100°C".to_string());
        assert_eq!(b.as_str(), "100°C");

        let c = Anchor::Math("E=mc²".to_string());
        assert_eq!(c.as_str(), "E=mc²");
    }

    #[test]
    fn test_verify_result_variants() {
        let ok = VerifyResult::Ok;
        assert_eq!(ok, VerifyResult::Ok);

        let missing = VerifyResult::Missing("H8O".to_string());
        assert_eq!(missing, VerifyResult::Missing("H8O".to_string()));

        let modified = VerifyResult::Modified {
            expected: "H8O".to_string(),
            actual: "H2O".to_string(),
        };
        assert!(matches!(modified, VerifyResult::Modified { .. }));
    }

    #[test]
    fn test_notation_type_from_str() {
        assert_eq!(
            NotationType::from_str_opt(Some("chemical")),
            NotationType::Chemical
        );
        assert_eq!(
            NotationType::from_str_opt(Some("latex")),
            NotationType::Latex
        );
        assert_eq!(
            NotationType::from_str_opt(Some("smiles")),
            NotationType::Smiles
        );
        assert_eq!(NotationType::from_str_opt(Some("code")), NotationType::Code);
        assert_eq!(NotationType::from_str_opt(None), NotationType::Generic);
        assert_eq!(
            NotationType::from_str_opt(Some("unknown")),
            NotationType::Generic
        );
    }

    #[test]
    fn test_notation_type_as_u8() {
        assert_eq!(NotationType::Generic.as_u8(), 0);
        assert_eq!(NotationType::Chemical.as_u8(), 1);
        assert_eq!(NotationType::Latex.as_u8(), 2);
        assert_eq!(NotationType::Smiles.as_u8(), 3);
        assert_eq!(NotationType::Code.as_u8(), 4);
    }
}
