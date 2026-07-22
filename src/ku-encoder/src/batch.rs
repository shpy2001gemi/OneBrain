//! Batch encoding — process multiple texts sequentially.
//!
//! Wraps `AiEncoder` to encode a list of text inputs one at a time,
//! collecting results and tracking success/failure counts.
//!
//! # Usage
//! ```rust,ignore
//! let batch = BatchEncoder::new(&encoder);
//! let result = batch.encode_all(&["text1", "text2"]).await;
//! println!("{}/{} succeeded", result.succeeded, result.total);
//! ```

use crate::encoder::{AiEncoder, EncodingResult};
use crate::error::EncoderError;

/// Result of batch encoding multiple texts.
#[derive(Debug)]
pub struct BatchResult {
    /// Individual results for each text (in input order).
    pub results: Vec<Result<EncodingResult, EncoderError>>,
    /// Total number of texts processed.
    pub total: usize,
    /// Number of successful encodings.
    pub succeeded: usize,
    /// Number of failed encodings.
    pub failed: usize,
}

/// Batch encoder that processes multiple texts sequentially.
///
/// Each text is encoded independently using the provided `AiEncoder`.
/// Results are collected in order with per-item success/failure tracking.
pub struct BatchEncoder<'a> {
    /// Reference to the AI encoder used for each text.
    encoder: &'a AiEncoder,
}

impl<'a> BatchEncoder<'a> {
    /// Create a new batch encoder wrapping the given AI encoder.
    pub fn new(encoder: &'a AiEncoder) -> Self {
        Self { encoder }
    }

    /// Encode multiple texts sequentially.
    ///
    /// Each text is encoded independently. A failure in one text does not
    /// affect the encoding of subsequent texts.
    pub async fn encode_all(&self, texts: &[&str]) -> BatchResult {
        let mut results = Vec::with_capacity(texts.len());
        let mut succeeded = 0;
        let mut failed = 0;

        for text in texts {
            match self.encoder.encode(text).await {
                Ok(result) => {
                    succeeded += 1;
                    results.push(Ok(result));
                }
                Err(e) => {
                    failed += 1;
                    results.push(Err(e));
                }
            }
        }

        BatchResult {
            total: texts.len(),
            results,
            succeeded,
            failed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoder::EncoderConfig;
    use ku_ai::backend::MockBackend;
    use ku_ai::types::ToolCallResponse;
    use ku_core::text_parser::default_dict;

    /// Helper: create tool calls that produce a valid KU.
    fn make_fact_tool_calls() -> Vec<ToolCallResponse> {
        vec![
            ToolCallResponse {
                id: "c1".into(),
                name: "new_ku".into(),
                arguments: serde_json::json!({"gene_type": "fact"}),
            },
            ToolCallResponse {
                id: "c2".into(),
                name: "add_quality".into(),
                arguments: serde_json::json!({"subject": 100, "quality": 200}),
            },
            ToolCallResponse {
                id: "c3".into(),
                name: "set_certainty".into(),
                arguments: serde_json::json!({"level": 9000}),
            },
            ToolCallResponse {
                id: "c4".into(),
                name: "finalize".into(),
                arguments: serde_json::json!({}),
            },
        ]
    }

    #[tokio::test]
    async fn test_batch_encode_all_succeed() {
        let mock = MockBackend::new().with_tool_response(make_fact_tool_calls());
        let encoder = AiEncoder::new(Box::new(mock), default_dict(), EncoderConfig::default());

        let batch = BatchEncoder::new(&encoder);
        let result = batch.encode_all(&["text one", "text two"]).await;

        assert_eq!(result.total, 2);
        // MockBackend wraps around, so both should succeed with same tool calls
        assert_eq!(result.succeeded, 2);
        assert_eq!(result.failed, 0);
    }

    #[tokio::test]
    async fn test_batch_encode_with_failures() {
        // First call returns tool calls, second returns chat (no tools → error)
        let mock = MockBackend::new()
            .with_tool_response(make_fact_tool_calls())
            .with_chat_response("I cannot encode this");
        let encoder = AiEncoder::new(Box::new(mock), default_dict(), EncoderConfig::default());

        let batch = BatchEncoder::new(&encoder);
        let result = batch.encode_all(&["good text", "bad text"]).await;

        assert_eq!(result.total, 2);
        assert_eq!(result.succeeded, 1);
        assert_eq!(result.failed, 1);
        assert!(result.results[0].is_ok());
        assert!(result.results[1].is_err());
    }

    #[tokio::test]
    async fn test_batch_encode_empty() {
        let mock = MockBackend::new();
        let encoder = AiEncoder::new(Box::new(mock), default_dict(), EncoderConfig::default());

        let batch = BatchEncoder::new(&encoder);
        let result = batch.encode_all(&[]).await;

        assert_eq!(result.total, 0);
        assert_eq!(result.succeeded, 0);
        assert_eq!(result.failed, 0);
        assert!(result.results.is_empty());
    }
}
