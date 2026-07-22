//! Encoding fallback chain.
//!
//! When AI encoding fails or produces low confidence, this module
//! decides what to do: retry with different temperature, or fall back
//! to rule-based encoding (text_parser).
//!
//! # Decision flow
//! ```text
//! AI encoding result
//!     │
//!     ├─ confidence ≥ min_confidence  → Accept
//!     ├─ attempt < max_retries        → Retry (higher temperature)
//!     └─ otherwise                    → FallbackTier1 (rule-based)
//! ```

use crate::encoder::{EncoderConfig, EncodingResult};
use crate::error::EncoderError;
use ku_core::ku_tool_executor::EncodingStats;
use ku_core::text_parser::{parse_text_to_core_dna, ConceptDict};

/// Decision from the fallback chain.
#[derive(Debug, Clone)]
pub enum EncodingDecision {
    /// Accept the encoding result as-is.
    Accept(EncodingResult),
    /// Retry with modified parameters (higher temperature, next attempt).
    Retry {
        /// New temperature to use for the retry.
        temperature: f32,
        /// Attempt number (1-indexed).
        attempt: u32,
    },
    /// Fall back to Tier 1 rule-based encoding via `parse_text_to_core_dna`.
    FallbackTier1,
    /// Reject — cannot encode this text at all.
    Reject {
        /// Reason for rejection.
        reason: String,
    },
}

/// Fallback chain for encoding decisions.
///
/// Evaluates an `EncodingResult` and decides whether to accept it,
/// retry with different parameters, or fall back to rule-based encoding.
pub struct FallbackChain {
    /// Encoder configuration (thresholds and limits).
    config: EncoderConfig,
}

impl FallbackChain {
    /// Create a new fallback chain with the given encoder config.
    pub fn new(config: EncoderConfig) -> Self {
        Self { config }
    }

    /// Decide what to do with an encoding result.
    ///
    /// # Arguments
    /// * `result` — the encoding result to evaluate.
    /// * `attempt` — the current attempt number (0-indexed).
    pub fn decide(&self, result: &EncodingResult, attempt: u32) -> EncodingDecision {
        // If confidence is high enough, accept
        if result.confidence >= self.config.min_confidence && !result.wire_bytes.is_empty() {
            return EncodingDecision::Accept(result.clone());
        }

        // If we haven't exceeded retries, retry with different temperature
        if attempt < self.config.max_retries {
            let new_temp = self.config.temperature + 0.1 * (attempt as f32 + 1.0);
            return EncodingDecision::Retry {
                temperature: new_temp.min(0.8),
                attempt: attempt + 1,
            };
        }

        // Fall back to rule-based encoding
        EncodingDecision::FallbackTier1
    }

    /// Execute Tier 1 (rule-based) encoding.
    ///
    /// Uses ku-core's `parse_text_to_core_dna` to produce a CoreDna from
    /// natural language text without any AI model. This is deterministic
    /// but less sophisticated than AI-assisted encoding.
    pub fn encode_tier1(
        &self,
        text: &str,
        dict: &ConceptDict,
    ) -> Result<EncodingResult, EncoderError> {
        match parse_text_to_core_dna(text, dict) {
            Ok(dna) => {
                let wire_bytes = dna
                    .encode()
                    .map_err(|e| EncoderError::CoreDnaError(format!("{:?}", e)))?;
                Ok(EncodingResult {
                    wire_bytes: vec![wire_bytes],
                    gene_type: Some("fact".to_string()), // Tier 1 always produces facts
                    concepts_used: Vec::new(),
                    confidence: 0.50, // Lower confidence for rule-based
                    stats: EncodingStats::default(),
                    source_text: text.to_string(),
                })
            }
            Err(e) => Err(EncoderError::CoreDnaError(format!("{:?}", e))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ku_core::text_parser::default_dict;

    #[test]
    fn test_decide_accept_high_confidence() {
        let config = EncoderConfig {
            min_confidence: 0.6,
            max_retries: 2,
            temperature: 0.1,
        };
        let chain = FallbackChain::new(config);

        let result = EncodingResult {
            wire_bytes: vec![vec![0x4B, 0x20, 0xF0, 0x00, 0x00]],
            gene_type: Some("fact".into()),
            concepts_used: Vec::new(),
            confidence: 0.85,
            stats: EncodingStats::default(),
            source_text: "test".into(),
        };

        let decision = chain.decide(&result, 0);
        assert!(
            matches!(decision, EncodingDecision::Accept(_)),
            "High confidence should be accepted"
        );
    }

    #[test]
    fn test_decide_retry_low_confidence() {
        let config = EncoderConfig {
            min_confidence: 0.6,
            max_retries: 2,
            temperature: 0.1,
        };
        let chain = FallbackChain::new(config);

        let result = EncodingResult {
            wire_bytes: vec![vec![0x4B]],
            gene_type: None,
            concepts_used: Vec::new(),
            confidence: 0.3, // Below threshold
            stats: EncodingStats::default(),
            source_text: "test".into(),
        };

        let decision = chain.decide(&result, 0);
        match decision {
            EncodingDecision::Retry {
                temperature,
                attempt,
            } => {
                assert_eq!(attempt, 1);
                assert!(temperature > 0.1, "Retry should use higher temperature");
            }
            other => panic!("Expected Retry, got {:?}", other),
        }
    }

    #[test]
    fn test_decide_fallback_after_max_retries() {
        let config = EncoderConfig {
            min_confidence: 0.6,
            max_retries: 2,
            temperature: 0.1,
        };
        let chain = FallbackChain::new(config);

        let result = EncodingResult {
            wire_bytes: vec![],
            gene_type: None,
            concepts_used: Vec::new(),
            confidence: 0.2,
            stats: EncodingStats::default(),
            source_text: "test".into(),
        };

        // attempt = 2 (equals max_retries), should fall back
        let decision = chain.decide(&result, 2);
        assert!(
            matches!(decision, EncodingDecision::FallbackTier1),
            "Should fall back to Tier 1 after max retries"
        );
    }

    #[test]
    fn test_encode_tier1_simple_text() {
        let config = EncoderConfig::default();
        let chain = FallbackChain::new(config);
        let dict = default_dict();

        // Use a simple text that the rule-based parser can handle
        let result = chain.encode_tier1("Water is liquid", &dict);
        assert!(result.is_ok(), "Tier 1 should succeed for simple text");

        let enc = result.unwrap();
        assert!(!enc.wire_bytes.is_empty());
        assert_eq!(enc.gene_type.as_deref(), Some("fact"));
        assert!((enc.confidence - 0.50).abs() < f32::EPSILON);
    }

    #[test]
    fn test_encode_tier1_empty_text_fails() {
        let config = EncoderConfig::default();
        let chain = FallbackChain::new(config);
        let dict = default_dict();

        let result = chain.encode_tier1("", &dict);
        assert!(result.is_err(), "Empty text should fail");
    }

    #[test]
    fn test_retry_temperature_capped_at_08() {
        let config = EncoderConfig {
            min_confidence: 0.6,
            max_retries: 10,
            temperature: 0.5,
        };
        let chain = FallbackChain::new(config);

        let result = EncodingResult {
            wire_bytes: vec![vec![1]],
            gene_type: None,
            concepts_used: Vec::new(),
            confidence: 0.3,
            stats: EncodingStats::default(),
            source_text: "test".into(),
        };

        // At attempt 5, temp = 0.5 + 0.1 * 6 = 1.1, should be capped at 0.8
        let decision = chain.decide(&result, 5);
        match decision {
            EncodingDecision::Retry { temperature, .. } => {
                assert!(
                    temperature <= 0.8,
                    "Temperature should be capped at 0.8, got {}",
                    temperature
                );
            }
            other => panic!("Expected Retry, got {:?}", other),
        }
    }
}
