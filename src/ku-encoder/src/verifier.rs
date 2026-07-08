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

use ku_core::core_dna::decode_core_dna;
use crate::encoder::EncodingResult;

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
        Self { min_instructions: 2 }
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
        let dna = CoreDna::new(0, vec![
            Instruction::Quality { s: 100, q: 200 },
            Instruction::Certainty { level: 9500 },
        ]);
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
        let dna = CoreDna::new(0, vec![
            Instruction::Certainty { level: 9000 },
        ]);
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
        let dna = CoreDna::new(0, vec![
            Instruction::Certainty { level: 9000 },
        ]);
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
}
