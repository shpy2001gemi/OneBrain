//! Encoding verification — ensures AI-generated CoreDna is valid.
//!
//! Three verification levels:
//! 1. **Structural**: Can the wire bytes be decoded back to valid CoreDna?
//! 2. **Completeness**: Does the KU have minimum required instructions?
//! 3. **Non-empty**: Were any wire bytes produced at all?
//!
//! # Usage
//! ```rust,ignore
//! use ku_encoder::verifier::EncodingVerifier;
//!
//! let verifier = EncodingVerifier::new();
//! let result = verifier.verify(&encoding_result);
//! if result.passed {
//!     // Accept the encoding
//! }
//! ```

use crate::encoder::EncodingResult;
use ku_core::core_dna::decode_core_dna;

/// Result of verifying an encoding.
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// Whether all verification checks passed.
    pub passed: bool,
    /// Whether all wire bytes decode to valid CoreDna.
    pub structural_ok: bool,
    /// Whether all KUs have the minimum required instructions.
    pub completeness_ok: bool,
    /// The minimum instruction count required.
    pub min_instructions: usize,
    /// Total actual instructions across all KUs.
    pub actual_instructions: usize,
    /// List of issues found during verification.
    pub issues: Vec<String>,
}

/// Verifies that AI-generated CoreDna encodings are structurally valid
/// and meet minimum completeness criteria.
pub struct EncodingVerifier {
    /// Minimum number of instructions per KU (default: 2).
    min_instructions: usize,
}

impl EncodingVerifier {
    /// Create a new verifier with default settings (min 2 instructions per KU).
    pub fn new() -> Self {
        Self {
            min_instructions: 2,
        }
    }

    /// Set the minimum required instructions per KU.
    pub fn with_min_instructions(mut self, min: usize) -> Self {
        self.min_instructions = min;
        self
    }

    /// Verify an encoding result.
    ///
    /// Checks:
    /// 1. Wire bytes are non-empty.
    /// 2. Each KU decodes to valid CoreDna (structural check).
    /// 3. Each KU has at least `min_instructions` instructions (completeness).
    pub fn verify(&self, result: &EncodingResult) -> VerificationResult {
        let mut issues = Vec::new();
        let mut structural_ok = true;
        let mut completeness_ok = true;
        let mut total_instructions = 0usize;

        // Check each wire bytes set
        for (i, bytes) in result.wire_bytes.iter().enumerate() {
            // Structural: can we decode?
            match decode_core_dna(bytes) {
                Ok(dna) => {
                    total_instructions += dna.instructions.len();
                    // Completeness: enough instructions?
                    if dna.instructions.len() < self.min_instructions {
                        completeness_ok = false;
                        issues.push(format!(
                            "KU {} has only {} instructions (min: {})",
                            i,
                            dna.instructions.len(),
                            self.min_instructions
                        ));
                    }
                }
                Err(e) => {
                    structural_ok = false;
                    issues.push(format!("KU {} decode failed: {:?}", i, e));
                }
            }
        }

        if result.wire_bytes.is_empty() {
            structural_ok = false;
            issues.push("No wire bytes produced".into());
        }

        VerificationResult {
            passed: structural_ok && completeness_ok,
            structural_ok,
            completeness_ok,
            min_instructions: self.min_instructions,
            actual_instructions: total_instructions,
            issues,
        }
    }
    /// Verify an encoding result from the v2 pipeline.
    ///
    /// In addition to standard verification, checks:
    /// - Concept table consistency: every concept ID referenced in instructions
    ///   must have a corresponding entry in the concept table.
    /// - No orphan concept table entries (entries not referenced by any instruction).
    pub fn validate_v2(&self, result: &EncodingResult) -> VerificationResult {
        let mut vr = self.verify(result);

        // Additional v2 checks: concept table consistency
        for (i, bytes) in result.wire_bytes.iter().enumerate() {
            if let Ok(dna) = decode_core_dna(bytes) {
                // Collect all concept IDs from concept table
                let table_ids: std::collections::HashSet<u64> =
                    dna.concept_table.iter().map(|e| e.local_id).collect();

                // Collect all concept IDs referenced in instructions
                let referenced_ids = collect_referenced_ids(&dna.instructions);

                // Check: every referenced ID in Tier 2+ should be in concept table
                for id in &referenced_ids {
                    if *id >= 16512 && !table_ids.contains(id) {
                        vr.passed = false;
                        vr.issues.push(format!(
                            "KU {}: concept ID {} referenced but not in concept table",
                            i, id
                        ));
                    }
                }

                // Check: orphan entries (in table but not referenced)
                for entry in &dna.concept_table {
                    if !referenced_ids.contains(&entry.local_id) {
                        vr.issues.push(format!(
                            "KU {}: concept table entry {} not referenced by any instruction (orphan)",
                            i, entry.local_id
                        ));
                        // Orphans are warnings, not failures
                    }
                }
            }
        }

        vr
    }
}

/// Extract all concept IDs referenced in a set of instructions.
fn collect_referenced_ids(
    instructions: &[ku_core::core_dna::Instruction],
) -> std::collections::HashSet<u64> {
    use ku_core::core_dna::Instruction;
    let mut ids = std::collections::HashSet::new();

    for instr in instructions {
        match instr {
            Instruction::Triple { s, p, o } => {
                ids.insert(*s);
                ids.insert(*p);
                ids.insert(*o);
            }
            Instruction::Quality { s, q } => {
                ids.insert(*s);
                ids.insert(*q);
            }
            Instruction::Quantity { s, unit, .. } => {
                ids.insert(*s);
                ids.insert(*unit);
            }
            Instruction::Sequence { items } => {
                for id in items {
                    ids.insert(*id);
                }
            }
            Instruction::PartOf { part, whole } => {
                ids.insert(*part);
                ids.insert(*whole);
            }
            Instruction::Located { s, location } => {
                ids.insert(*s);
                ids.insert(*location);
            }
            Instruction::Temporal { s, time } => {
                ids.insert(*s);
                ids.insert(*time);
            }
            Instruction::Causal { cause, effect } => {
                ids.insert(*cause);
                ids.insert(*effect);
            }
            Instruction::Simulates { s, model } => {
                ids.insert(*s);
                ids.insert(*model);
            }
            Instruction::Condition { cond, result } => {
                ids.insert(*cond);
                ids.insert(*result);
            }
            Instruction::Agent { actor, action } => {
                ids.insert(*actor);
                ids.insert(*action);
            }
            Instruction::Tool { action, instrument } => {
                ids.insert(*action);
                ids.insert(*instrument);
            }
            Instruction::Range { s, .. } => {
                ids.insert(*s);
            }
            Instruction::Tolerance { s, .. } => {
                ids.insert(*s);
            }
            Instruction::Constraint { source, target, .. } => {
                ids.insert(*source);
                ids.insert(*target);
            }
            Instruction::EnumVal { s, values } => {
                ids.insert(*s);
                for v in values {
                    ids.insert(*v);
                }
            }
            Instruction::Step { action, target, .. } => {
                ids.insert(*action);
                ids.insert(*target);
            }
            Instruction::Precond { concept } => {
                ids.insert(*concept);
            }
            Instruction::Effect { concept } => {
                ids.insert(*concept);
            }
            // These don't reference concept IDs
            Instruction::Certainty { .. }
            | Instruction::Formula { .. }
            | Instruction::Difficulty { .. }
            | Instruction::CidRef { .. }
            | Instruction::Affect { .. } => {}
            // Catch-all for any future instruction variants
            #[allow(unreachable_patterns)]
            _ => {}
        }
    }

    ids
}

impl Default for EncodingVerifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ku_core::core_dna::{CoreDna, Instruction};
    use ku_core::ku_tool_executor::EncodingStats;

    /// Helper to create a valid encoding result with actual CoreDna bytes.
    fn make_valid_result() -> EncodingResult {
        // Create a CoreDna with quality + certainty instructions
        let dna = CoreDna::new(
            0,
            vec![
                Instruction::Quality { s: 100, q: 200 },
                Instruction::Certainty { level: 9500 },
            ],
        );
        let wire = dna.encode().expect("encode should succeed");

        EncodingResult {
            wire_bytes: vec![wire],
            gene_type: Some("fact".into()),
            concepts_used: Vec::new(),
            confidence: 0.85,
            stats: EncodingStats {
                total_kus: 1,
                total_instructions: 2,
                total_wire_bytes: 10,
                tool_calls_processed: 4,
                tool_calls_failed: 0,
                concepts_created: 2,
                concepts_looked_up: 2,
            },
            source_text: "test".into(),
        }
    }

    #[test]
    fn test_verify_valid_encoding() {
        let verifier = EncodingVerifier::new();
        let result = make_valid_result();
        let vr = verifier.verify(&result);

        assert!(vr.passed, "Valid encoding should pass: {:?}", vr.issues);
        assert!(vr.structural_ok);
        assert!(vr.completeness_ok);
        assert!(vr.issues.is_empty());
        assert_eq!(vr.actual_instructions, 2);
    }

    #[test]
    fn test_verify_empty_wire_bytes() {
        let verifier = EncodingVerifier::new();
        let result = EncodingResult {
            wire_bytes: Vec::new(),
            gene_type: None,
            concepts_used: Vec::new(),
            confidence: 0.0,
            stats: EncodingStats::default(),
            source_text: "test".into(),
        };

        let vr = verifier.verify(&result);
        assert!(!vr.passed);
        assert!(!vr.structural_ok);
        assert!(vr.issues.iter().any(|i| i.contains("No wire bytes")));
    }

    #[test]
    fn test_verify_invalid_bytes() {
        let verifier = EncodingVerifier::new();
        let result = EncodingResult {
            wire_bytes: vec![vec![0xFF, 0x00, 0x01]], // garbage bytes
            gene_type: None,
            concepts_used: Vec::new(),
            confidence: 0.0,
            stats: EncodingStats::default(),
            source_text: "test".into(),
        };

        let vr = verifier.verify(&result);
        assert!(!vr.passed);
        assert!(!vr.structural_ok);
        assert!(vr.issues.iter().any(|i| i.contains("decode failed")));
    }

    #[test]
    fn test_verify_insufficient_instructions() {
        // Create a KU with only 1 instruction (below min of 2)
        let dna = CoreDna::new(0, vec![Instruction::Certainty { level: 9000 }]);
        let wire = dna.encode().expect("encode should succeed");

        let verifier = EncodingVerifier::new(); // min_instructions = 2
        let result = EncodingResult {
            wire_bytes: vec![wire],
            gene_type: Some("fact".into()),
            concepts_used: Vec::new(),
            confidence: 0.5,
            stats: EncodingStats::default(),
            source_text: "test".into(),
        };

        let vr = verifier.verify(&result);
        assert!(!vr.passed);
        assert!(vr.structural_ok); // decode works
        assert!(!vr.completeness_ok); // not enough instructions
    }

    #[test]
    fn test_verifier_custom_min_instructions() {
        // With min=1, even a single instruction should pass
        let dna = CoreDna::new(0, vec![Instruction::Certainty { level: 9000 }]);
        let wire = dna.encode().expect("encode should succeed");

        let verifier = EncodingVerifier::new().with_min_instructions(1);
        let result = EncodingResult {
            wire_bytes: vec![wire],
            gene_type: None,
            concepts_used: Vec::new(),
            confidence: 0.5,
            stats: EncodingStats::default(),
            source_text: "test".into(),
        };

        let vr = verifier.verify(&result);
        assert!(vr.passed, "Should pass with min_instructions=1");
    }

    // --- validate_v2 tests ---

    #[test]
    fn test_validate_v2_valid() {
        let verifier = EncodingVerifier::new();
        let result = make_valid_result();
        let vr = verifier.validate_v2(&result);
        assert!(vr.passed, "Valid v2 encoding should pass: {:?}", vr.issues);
    }

    #[test]
    fn test_validate_v2_empty_fails() {
        let verifier = EncodingVerifier::new();
        let result = EncodingResult {
            wire_bytes: Vec::new(),
            gene_type: None,
            concepts_used: Vec::new(),
            confidence: 0.0,
            stats: EncodingStats::default(),
            source_text: "test".into(),
        };
        let vr = verifier.validate_v2(&result);
        assert!(!vr.passed);
    }

    #[test]
    fn test_validate_v2_with_concept_table() {
        use ku_core::core_dna::ConceptTableEntry;

        // Build a CoreDna with concept table + matching instructions
        let dna = CoreDna {
            header: ku_core::core_dna::CoreDnaHeader {
                version: 2,
                gene_type: 0,
                has_concept_table: true,
            },
            concept_table: vec![
                ConceptTableEntry {
                    local_id: 16512,
                    ccid: [1u8; 16],
                },
                ConceptTableEntry {
                    local_id: 16513,
                    ccid: [2u8; 16],
                },
            ],
            instructions: vec![
                Instruction::Quality { s: 16512, q: 16513 },
                Instruction::Certainty { level: 8000 },
            ],
        };
        let wire = dna.encode().expect("encode should succeed");

        let verifier = EncodingVerifier::new();
        let result = EncodingResult {
            wire_bytes: vec![wire],
            gene_type: Some("fact".into()),
            concepts_used: Vec::new(),
            confidence: 0.85,
            stats: EncodingStats::default(),
            source_text: "test".into(),
        };

        let vr = verifier.validate_v2(&result);
        assert!(
            vr.passed,
            "Consistent concept table should pass: {:?}",
            vr.issues
        );
    }
}
