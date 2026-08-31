//! # Builder — build CoreDna from resolved triples.
//!
//! Takes resolved triples (with CCIDs) and constructs CoreDna units
//! with proper instruction streams, concept tables, and gene type headers.
//!
//! This is pure code — no AI needed. Directly builds CoreDna structs
//! and calls `encode_core_dna()` to produce wire bytes.
//!
//! # Pipeline position
//! ```text
//! BƯỚC 5: build(resolved_triples) → Vec<CoreDna> → Vec<Vec<u8>>
//! ```

use std::collections::HashMap;

use ku_core::ccid::Ccid;
use ku_core::core_dna::{
    encode_core_dna, ConceptTableEntry, CoreDna, CoreDnaHeader, Instruction, NumericValue, Op,
};
use ku_core::types::ConceptId;

use crate::analyzer::determine_gene_type;
use crate::error::EncoderError;
use crate::types::{NotationType, ResolvedTriple};

// ============================================================================
// Concept table allocator
// ============================================================================

/// Allocates local ConceptIds for CCIDs.
///
/// CoreDna instructions use local u64 IDs. The concept table maps them
/// to 16-byte CCIDs. This allocator assigns sequential IDs starting from 16512
/// (Tier 2+, as Tier 0-1 are reserved for built-in concepts).
struct ConceptAllocator {
    /// CCID → local ConceptId mapping.
    map: HashMap<Ccid, ConceptId>,
    /// Next available local ID.
    next_id: ConceptId,
}

impl ConceptAllocator {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
            // Start at 16512 (Tier 2 concepts, above reserved range)
            next_id: 16512,
        }
    }

    /// Get or allocate a local ConceptId for a CCID.
    fn get_or_alloc(&mut self, ccid: Ccid) -> ConceptId {
        *self.map.entry(ccid).or_insert_with(|| {
            let id = self.next_id;
            self.next_id += 1;
            id
        })
    }

    /// Build the concept table entries.
    fn build_table(&self) -> Vec<ConceptTableEntry> {
        let mut entries: Vec<ConceptTableEntry> = self
            .map
            .iter()
            .map(|(&ccid, &local_id)| ConceptTableEntry { local_id, ccid })
            .collect();
        // Sort by local_id for deterministic binary output
        entries.sort_by_key(|e| e.local_id);
        entries
    }
}

// ============================================================================
// KuBuilder
// ============================================================================

/// Builds CoreDna units from resolved triples.
///
/// Strategy: **each resolved triple produces exactly 1 KU** (1 KU = 1 atomic idea).
/// The concept table for each KU contains only the concepts referenced by that triple.
pub struct KuBuilder;

impl KuBuilder {
    /// Build CoreDna units from resolved triples.
    ///
    /// Returns a list of CoreDna units (**one per triple**) plus their wire bytes.
    ///
    /// # Returns
    /// `Vec<(CoreDna, Vec<u8>)>` — each entry is a CoreDna + its encoded wire bytes.
    pub fn build(triples: Vec<ResolvedTriple>) -> Result<Vec<(CoreDna, Vec<u8>)>, EncoderError> {
        if triples.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();

        for rt in &triples {
            // Each triple gets its own allocator → its own concept_table
            let mut allocator = ConceptAllocator::new();
            let mut instructions: Vec<Instruction> = Vec::new();

            // Determine gene type per triple
            let gene_type = determine_gene_type(std::slice::from_ref(&rt.analyzed.raw));

            // Build instructions for this single triple
            build_instructions_for_triple(rt, &mut allocator, &mut instructions);

            // Add certainty instruction
            instructions.push(Instruction::Certainty {
                level: rt.analyzed.certainty,
            });

            // Build concept table (only concepts used by THIS triple)
            let concept_table = allocator.build_table();

            let dna = CoreDna {
                header: CoreDnaHeader {
                    version: 2, // CoreDna v7
                    gene_type,
                    has_concept_table: !concept_table.is_empty(),
                },
                concept_table,
                instructions,
            };

            // Encode to wire bytes
            let wire_bytes =
                encode_core_dna(&dna).map_err(|e| EncoderError::CoreDnaError(e.to_string()))?;

            results.push((dna, wire_bytes));
        }

        Ok(results)
    }
}

// ============================================================================
// Instruction builder
// ============================================================================

/// Build CoreDna instructions for a single resolved triple.
fn build_instructions_for_triple(
    rt: &ResolvedTriple,
    alloc: &mut ConceptAllocator,
    instructions: &mut Vec<Instruction>,
) {
    let s_id = alloc.get_or_alloc(rt.subject_ccid);
    let p_id = alloc.get_or_alloc(rt.predicate_ccid);

    match rt.analyzed.op {
        Op::Formula => {
            // Formula: store raw string as bytes, not concept ID
            if let Some(ref formula) = rt.formula_string {
                let notation = NotationType::from_str_opt(rt.analyzed.raw.notation.as_deref());
                instructions.push(Instruction::Formula {
                    format: notation.as_u8(),
                    data: formula.as_bytes().to_vec(),
                });
            }
        }
        Op::PartOf => {
            if let Some(o_ccid) = rt.object_ccid {
                let o_id = alloc.get_or_alloc(o_ccid);
                instructions.push(Instruction::PartOf {
                    part: o_id,
                    whole: s_id,
                });
            }
            // If there's a quantity, add Quantity instruction
            if let Some(qty) = rt.analyzed.raw.qty {
                let numeric = f64_to_numeric(qty);
                // Use subject + quantity + object as unit
                if let Some(o_ccid) = rt.object_ccid {
                    let o_id = alloc.get_or_alloc(o_ccid);
                    instructions.push(Instruction::Quantity {
                        s: s_id,
                        value: numeric,
                        unit: o_id,
                    });
                }
            }
        }
        Op::Located => {
            if let Some(o_ccid) = rt.object_ccid {
                let o_id = alloc.get_or_alloc(o_ccid);
                instructions.push(Instruction::Located {
                    s: s_id,
                    location: o_id,
                });
            }
        }
        Op::Causal => {
            if let Some(o_ccid) = rt.object_ccid {
                let o_id = alloc.get_or_alloc(o_ccid);
                instructions.push(Instruction::Causal {
                    cause: s_id,
                    effect: o_id,
                });
            }
        }
        Op::Quality => {
            if let Some(o_ccid) = rt.object_ccid {
                let o_id = alloc.get_or_alloc(o_ccid);
                instructions.push(Instruction::Quality { s: s_id, q: o_id });
            }
        }
        Op::Step => {
            if let Some(o_ccid) = rt.object_ccid {
                let o_id = alloc.get_or_alloc(o_ccid);
                // Use qty as step order if available, otherwise default to 1
                let ord = rt.analyzed.raw.qty.map(|q| q as u8).unwrap_or(1);
                instructions.push(Instruction::Step {
                    ord,
                    action: p_id,
                    target: o_id,
                });
            }
        }
        // Triple (generic S-P-O) — covers material, purpose, category, relation
        _ => {
            if let Some(o_ccid) = rt.object_ccid {
                let o_id = alloc.get_or_alloc(o_ccid);
                instructions.push(Instruction::Triple {
                    s: s_id,
                    p: p_id,
                    o: o_id,
                });
                // Add quantity if present
                if let Some(qty) = rt.analyzed.raw.qty {
                    let numeric = f64_to_numeric(qty);
                    instructions.push(Instruction::Quantity {
                        s: s_id,
                        value: numeric,
                        unit: o_id,
                    });
                }
            }
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Convert f64 to the most compact NumericValue representation.
fn f64_to_numeric(v: f64) -> NumericValue {
    // Try integer representations first (smaller encoding)
    if v.fract() == 0.0 && v.is_finite() {
        let i = v as i64;
        if (0..=255).contains(&i) {
            return NumericValue::U8(i as u8);
        }
        if (0..=65535).contains(&i) {
            return NumericValue::U16(i as u16);
        }
        if i >= i16::MIN as i64 && i <= i16::MAX as i64 {
            return NumericValue::I16(i as i16);
        }
        if i >= 0 && i <= u32::MAX as i64 {
            return NumericValue::U32(i as u32);
        }
        if i >= i32::MIN as i64 && i <= i32::MAX as i64 {
            return NumericValue::I32(i as i32);
        }
    }

    // Try f32 if no precision loss
    let as_f32 = v as f32;
    if (as_f32 as f64 - v).abs() < f64::EPSILON {
        return NumericValue::F32(as_f32);
    }

    NumericValue::F64(v)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::GENE_FACT;
    use crate::types::{AnalyzedTriple, SpoTriple};
    use ku_core::ccid::ccid;
    use ku_core::core_dna::{decode_core_dna, Op};

    fn make_part_triple(certainty: u16) -> ResolvedTriple {
        ResolvedTriple {
            analyzed: AnalyzedTriple {
                raw: SpoTriple {
                    s: "bàn".into(),
                    s_en: "desk".into(),
                    p: "có".into(),
                    o: "chân".into(),
                    o_en: "leg".into(),
                    qty: Some(4.0),
                    role: "part".into(),
                    notation: None,
                    c: if certainty == 8000 {
                        "usually"
                    } else {
                        "always"
                    }
                    .into(),
                },
                op: Op::PartOf,
                certainty,
            },
            subject_ccid: ccid(b"wd:Q7432"),      // desk
            object_ccid: Some(ccid(b"wd:Q1075")), // leg
            predicate_ccid: ccid("ob:có".as_bytes()),
            formula_string: None,
        }
    }

    fn make_formula_triple() -> ResolvedTriple {
        ResolvedTriple {
            analyzed: AnalyzedTriple {
                raw: SpoTriple {
                    s: "H8O".into(),
                    s_en: "H8O".into(),
                    p: "expressed as".into(),
                    o: "H₈O".into(),
                    o_en: "H₈O".into(),
                    qty: None,
                    role: "formula".into(),
                    notation: Some("chemical".into()),
                    c: "always".into(),
                },
                op: Op::Formula,
                certainty: 10000,
            },
            subject_ccid: ccid(b"ob:h8o"),
            object_ccid: None,
            predicate_ccid: ccid(b"ob:expressed as"),
            formula_string: Some("H₈O".into()),
        }
    }

    fn make_location_triple() -> ResolvedTriple {
        ResolvedTriple {
            analyzed: AnalyzedTriple {
                raw: SpoTriple {
                    s: "bàn".into(),
                    s_en: "desk".into(),
                    p: "ở".into(),
                    o: "phòng".into(),
                    o_en: "room".into(),
                    qty: None,
                    role: "location".into(),
                    notation: None,
                    c: "usually".into(),
                },
                op: Op::Located,
                certainty: 8000,
            },
            subject_ccid: ccid(b"wd:Q7432"),
            object_ccid: Some(ccid(b"ob:room")),
            predicate_ccid: ccid("ob:ở".as_bytes()),
            formula_string: None,
        }
    }

    #[test]
    fn test_build_single_part_triple() {
        let triples = vec![make_part_triple(8000)];
        let results = KuBuilder::build(triples).unwrap();
        assert_eq!(results.len(), 1, "Should produce 1 KU");

        let (dna, wire_bytes) = &results[0];
        assert!(!wire_bytes.is_empty(), "Wire bytes should not be empty");
        assert_eq!(dna.header.gene_type, GENE_FACT);
        assert!(dna.header.has_concept_table);

        // Should have PartOf + Quantity + Certainty instructions
        let has_part_of = dna
            .instructions
            .iter()
            .any(|i| matches!(i, Instruction::PartOf { .. }));
        let has_quantity = dna
            .instructions
            .iter()
            .any(|i| matches!(i, Instruction::Quantity { .. }));
        let has_certainty = dna
            .instructions
            .iter()
            .any(|i| matches!(i, Instruction::Certainty { level: 8000 }));
        assert!(has_part_of, "Should have PartOf instruction");
        assert!(has_quantity, "Should have Quantity instruction (qty=4)");
        assert!(has_certainty, "Should have Certainty instruction");
    }

    #[test]
    fn test_build_formula_triple() {
        let triples = vec![make_formula_triple()];
        let results = KuBuilder::build(triples).unwrap();
        assert_eq!(results.len(), 1);

        let (dna, _) = &results[0];
        let has_formula = dna.instructions.iter().any(|i| {
            matches!(i, Instruction::Formula { format: 1, .. }) // 1 = chemical
        });
        assert!(
            has_formula,
            "Should have Formula instruction with chemical notation"
        );
    }

    #[test]
    fn test_build_groups_by_certainty() {
        // v2: even different certainty → 1 KU per triple
        let triples = vec![
            make_part_triple(8000),  // usually
            make_part_triple(10000), // always
        ];
        let results = KuBuilder::build(triples).unwrap();
        assert_eq!(
            results.len(),
            2,
            "Different certainty levels → 2 KUs (1 per triple)"
        );
    }

    #[test]
    fn test_build_same_certainty_separate_kus() {
        // v2: same certainty → STILL 1 KU per triple (1 KU = 1 idea)
        let triples = vec![
            make_part_triple(8000),
            make_location_triple(), // also certainty=8000
        ];
        let results = KuBuilder::build(triples).unwrap();
        assert_eq!(
            results.len(),
            2,
            "Same certainty → still 2 KUs (1 per triple)"
        );
    }

    #[test]
    fn test_build_empty_triples() {
        let results = KuBuilder::build(vec![]).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_build_round_trip() {
        let triples = vec![make_part_triple(8000)];
        let results = KuBuilder::build(triples).unwrap();
        let (_, wire_bytes) = &results[0];

        // Decode the wire bytes back to CoreDna
        let decoded = decode_core_dna(wire_bytes);
        assert!(
            decoded.is_ok(),
            "Wire bytes should decode without error: {:?}",
            decoded.err()
        );
    }

    // --- f64_to_numeric tests ---

    #[test]
    fn test_f64_to_numeric_u8() {
        assert!(matches!(f64_to_numeric(4.0), NumericValue::U8(4)));
        assert!(matches!(f64_to_numeric(0.0), NumericValue::U8(0)));
        assert!(matches!(f64_to_numeric(255.0), NumericValue::U8(255)));
    }

    #[test]
    fn test_f64_to_numeric_u16() {
        assert!(matches!(f64_to_numeric(256.0), NumericValue::U16(256)));
        assert!(matches!(f64_to_numeric(65535.0), NumericValue::U16(65535)));
    }

    #[test]
    fn test_f64_to_numeric_f64_for_fractions() {
        match f64_to_numeric(std::f64::consts::PI) {
            NumericValue::F32(_) | NumericValue::F64(_) => {} // either is fine
            other => panic!("Expected float, got {:?}", other),
        }
    }
}
