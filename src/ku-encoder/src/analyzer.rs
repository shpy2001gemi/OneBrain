//! # Analyzer — deterministic role/certainty mapping.
//!
//! Maps AI-extracted roles and certainty strings to CoreDna opcodes and
//! numeric values. This is pure code — no AI needed.
//!
//! # Pipeline position
//! ```text
//! BƯỚC 3: analyze(triples) → Vec<AnalyzedTriple>
//! ```

use ku_core::core_dna::Op;

use crate::types::{AnalyzedTriple, SpoTriple};

// ============================================================================
// Role → Op mapping
// ============================================================================

/// Map a semantic role string (from AI output) to a CoreDna opcode.
///
/// # Mapping table
/// | AI role    | CoreDna Op     |
/// |-----------|---------------|
/// | part      | PartOf        |
/// | material  | Triple        |
/// | purpose   | Triple        |
/// | location  | Located       |
/// | cause     | Causal        |
/// | property  | Quality       |
/// | category  | Triple        |
/// | formula   | Formula       |
/// | relation  | Triple        |
/// | step      | Step          |
/// | *unknown* | Triple        |
pub fn map_role(role: &str) -> Op {
    match role.to_lowercase().as_str() {
        "part" => Op::PartOf,
        "location" => Op::Located,
        "cause" => Op::Causal,
        "property" => Op::Quality,
        "formula" => Op::Formula,
        "step" => Op::Step,
        // These roles use the generic S-P-O Triple
        "material" | "purpose" | "category" | "relation" => Op::Triple,
        // Unknown roles fall back to Triple
        _ => Op::Triple,
    }
}

// ============================================================================
// Certainty → u16 mapping
// ============================================================================

/// Map a certainty string (from AI output) to a numeric value (0-10000).
///
/// # Mapping table
/// | AI certainty | Numeric value | Interpretation        |
/// |-------------|--------------|----------------------|
/// | always      | 10000        | Universal fact        |
/// | usually     | 8000         | Strong tendency       |
/// | sometimes   | 5000         | Occasional/conditional|
/// | rarely      | 2000         | Exceptional case      |
/// | *unknown*   | 9000         | Default (high)        |
pub fn map_certainty(c: &str) -> u16 {
    match c.to_lowercase().as_str() {
        "always" => 10000,
        "usually" => 8000,
        "sometimes" => 5000,
        "rarely" => 2000,
        _ => 9000, // Default: assume high certainty
    }
}

// ============================================================================
// Gene type detection
// ============================================================================

/// Gene type codes (maps to CoreDna header.gene_type).
pub const GENE_FACT: u8 = 0;
pub const GENE_PROCEDURE: u8 = 1;
pub const GENE_EXPERIENCE: u8 = 2;

/// Determine the gene type from a set of triples.
///
/// - If any triple has role="step" → procedure (gene_type=1)
/// - If any triple has role="emotion" or "feeling" → experience (gene_type=2)
/// - Otherwise → fact (gene_type=0)
pub fn determine_gene_type(triples: &[SpoTriple]) -> u8 {
    for t in triples {
        match t.role.to_lowercase().as_str() {
            "step" => return GENE_PROCEDURE,
            "emotion" | "feeling" | "experience" => return GENE_EXPERIENCE,
            _ => {}
        }
    }
    GENE_FACT
}

// ============================================================================
// Analyze: SpoTriple → AnalyzedTriple
// ============================================================================

/// Analyze a vector of raw SPO triples into AnalyzedTriples.
///
/// Each triple gets:
/// - `op`: the CoreDna opcode (determined by `map_role`)
/// - `certainty`: the numeric certainty (determined by `map_certainty`)
pub fn analyze(triples: Vec<SpoTriple>) -> Vec<AnalyzedTriple> {
    triples
        .into_iter()
        .map(|raw| {
            let op = map_role(&raw.role);
            let certainty = map_certainty(&raw.c);
            AnalyzedTriple { raw, op, certainty }
        })
        .collect()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- map_role tests ---

    #[test]
    fn test_map_role_part() {
        assert_eq!(map_role("part"), Op::PartOf);
    }

    #[test]
    fn test_map_role_location() {
        assert_eq!(map_role("location"), Op::Located);
    }

    #[test]
    fn test_map_role_cause() {
        assert_eq!(map_role("cause"), Op::Causal);
    }

    #[test]
    fn test_map_role_property() {
        assert_eq!(map_role("property"), Op::Quality);
    }

    #[test]
    fn test_map_role_formula() {
        assert_eq!(map_role("formula"), Op::Formula);
    }

    #[test]
    fn test_map_role_step() {
        assert_eq!(map_role("step"), Op::Step);
    }

    #[test]
    fn test_map_role_material_is_triple() {
        assert_eq!(map_role("material"), Op::Triple);
    }

    #[test]
    fn test_map_role_purpose_is_triple() {
        assert_eq!(map_role("purpose"), Op::Triple);
    }

    #[test]
    fn test_map_role_category_is_triple() {
        assert_eq!(map_role("category"), Op::Triple);
    }

    #[test]
    fn test_map_role_unknown_fallback() {
        assert_eq!(map_role("some_unknown_role"), Op::Triple);
        assert_eq!(map_role(""), Op::Triple);
    }

    #[test]
    fn test_map_role_case_insensitive() {
        assert_eq!(map_role("Part"), Op::PartOf);
        assert_eq!(map_role("LOCATION"), Op::Located);
        assert_eq!(map_role("Formula"), Op::Formula);
    }

    // --- map_certainty tests ---

    #[test]
    fn test_map_certainty_always() {
        assert_eq!(map_certainty("always"), 10000);
    }

    #[test]
    fn test_map_certainty_usually() {
        assert_eq!(map_certainty("usually"), 8000);
    }

    #[test]
    fn test_map_certainty_sometimes() {
        assert_eq!(map_certainty("sometimes"), 5000);
    }

    #[test]
    fn test_map_certainty_rarely() {
        assert_eq!(map_certainty("rarely"), 2000);
    }

    #[test]
    fn test_map_certainty_unknown_default() {
        assert_eq!(map_certainty("xyz"), 9000);
        assert_eq!(map_certainty(""), 9000);
    }

    #[test]
    fn test_map_certainty_case_insensitive() {
        assert_eq!(map_certainty("Always"), 10000);
        assert_eq!(map_certainty("USUALLY"), 8000);
    }

    // --- determine_gene_type tests ---

    #[test]
    fn test_gene_type_fact() {
        let triples = vec![SpoTriple {
            s: "a".into(),
            s_en: "a".into(),
            p: "is".into(),
            o: "b".into(),
            o_en: "b".into(),
            qty: None,
            role: "part".into(),
            notation: None,
            c: "always".into(),
        }];
        assert_eq!(determine_gene_type(&triples), GENE_FACT);
    }

    #[test]
    fn test_gene_type_procedure() {
        let triples = vec![SpoTriple {
            s: "bước 1".into(),
            s_en: "step 1".into(),
            p: "do".into(),
            o: "action".into(),
            o_en: "action".into(),
            qty: None,
            role: "step".into(),
            notation: None,
            c: "always".into(),
        }];
        assert_eq!(determine_gene_type(&triples), GENE_PROCEDURE);
    }

    #[test]
    fn test_gene_type_experience() {
        let triples = vec![SpoTriple {
            s: "tôi".into(),
            s_en: "I".into(),
            p: "feel".into(),
            o: "vui".into(),
            o_en: "happy".into(),
            qty: None,
            role: "emotion".into(),
            notation: None,
            c: "sometimes".into(),
        }];
        assert_eq!(determine_gene_type(&triples), GENE_EXPERIENCE);
    }

    // --- analyze tests ---

    #[test]
    fn test_analyze_maps_correctly() {
        let triples = vec![
            SpoTriple {
                s: "bàn".into(),
                s_en: "desk".into(),
                p: "có".into(),
                o: "chân".into(),
                o_en: "leg".into(),
                qty: Some(4.0),
                role: "part".into(),
                notation: None,
                c: "usually".into(),
            },
            SpoTriple {
                s: "bàn".into(),
                s_en: "desk".into(),
                p: "ở".into(),
                o: "phòng".into(),
                o_en: "room".into(),
                qty: None,
                role: "location".into(),
                notation: None,
                c: "sometimes".into(),
            },
        ];
        let analyzed = analyze(triples);
        assert_eq!(analyzed.len(), 2);
        assert_eq!(analyzed[0].op, Op::PartOf);
        assert_eq!(analyzed[0].certainty, 8000);
        assert_eq!(analyzed[1].op, Op::Located);
        assert_eq!(analyzed[1].certainty, 5000);
    }

    #[test]
    fn test_analyze_formula_triple() {
        let triples = vec![SpoTriple {
            s: "H8O".into(),
            s_en: "H8O".into(),
            p: "is".into(),
            o: "H₈O".into(),
            o_en: "H₈O".into(),
            qty: None,
            role: "formula".into(),
            notation: Some("chemical".into()),
            c: "always".into(),
        }];
        let analyzed = analyze(triples);
        assert_eq!(analyzed[0].op, Op::Formula);
        assert_eq!(analyzed[0].certainty, 10000);
    }

    #[test]
    fn test_analyze_empty() {
        let analyzed = analyze(vec![]);
        assert!(analyzed.is_empty());
    }
}
