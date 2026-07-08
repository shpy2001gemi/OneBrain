//! Knowledge synthesis — combines multiple KUs into a coherent answer.
//!
//! Uses the AI backend to synthesize retrieved knowledge into natural language,
//! or provides a simple formatted listing when no LLM is available.

use ku_ai::traits::ModelBackend;
use ku_ai::types::{ChatMessage, InferenceOptions};
use crate::retriever::RetrievedKU;

/// Synthesizes answers from retrieved KUs.
pub struct Synthesizer;

impl Synthesizer {
    /// Synthesize an answer from retrieved KUs using an AI backend.
    pub async fn synthesize(
        query: &str,
        retrieved_kus: &[RetrievedKU],
        backend: &dyn ModelBackend,
    ) -> Result<String, crate::error::MediatorError> {
        if retrieved_kus.is_empty() {
            return Ok("I don't have any relevant knowledge about this topic yet.".to_string());
        }

        // Build context from retrieved KUs
        let mut context = String::from("Based on the following knowledge units:\n\n");
        for (i, ku) in retrieved_kus.iter().enumerate() {
            context.push_str(&format!("{}. {} (relevance: {:.0}%)\n",
                i + 1, ku.expression, ku.score * 100.0));
        }

        let messages = vec![
            ChatMessage::system(
                "You are a knowledge synthesis assistant. \
                 Given retrieved knowledge units, answer the user's question \
                 by combining the relevant information. \
                 Cite the KU numbers in your answer. \
                 If the knowledge is insufficient, say so."
            ),
            ChatMessage::user(format!("{}\n\nQuestion: {}", context, query)),
        ];

        let options = InferenceOptions { temperature: 0.3, ..Default::default() };
        let response = backend.chat(&messages, &options).await
            .map_err(|e| crate::error::MediatorError::RetrievalError(e.to_string()))?;

        Ok(response.content)
    }

    /// Format a simple response when no LLM is available.
    pub fn format_results_simple(query: &str, retrieved_kus: &[RetrievedKU]) -> String {
        if retrieved_kus.is_empty() {
            return format!("No knowledge found for: {}", query);
        }

        let mut response = format!("Found {} relevant knowledge unit(s):\n\n", retrieved_kus.len());
        for (i, ku) in retrieved_kus.iter().enumerate() {
            response.push_str(&format!("{}. {} (score: {:.0}%)\n",
                i + 1, ku.expression, ku.score * 100.0));
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retriever::RetrievalSource;

    fn sample_kus() -> Vec<RetrievedKU> {
        vec![
            RetrievedKU {
                cid: "cid1".into(),
                expression: "Water boils at 100°C".into(),
                score: 0.9,
                source: RetrievalSource::Keyword,
            },
            RetrievedKU {
                cid: "cid2".into(),
                expression: "Water freezes at 0°C".into(),
                score: 0.7,
                source: RetrievalSource::Keyword,
            },
        ]
    }

    #[test]
    fn test_format_simple_with_results() {
        let kus = sample_kus();
        let result = Synthesizer::format_results_simple("water temperature", &kus);
        assert!(result.contains("Found 2 relevant"));
        assert!(result.contains("Water boils"));
        assert!(result.contains("Water freezes"));
        assert!(result.contains("90%"));
    }

    #[test]
    fn test_format_simple_no_results() {
        let result = Synthesizer::format_results_simple("quantum physics", &[]);
        assert!(result.contains("No knowledge found"));
        assert!(result.contains("quantum physics"));
    }

    #[tokio::test]
    async fn test_synthesize_empty_kus() {
        let mock = ku_ai::MockBackend::new();
        let result = Synthesizer::synthesize("test", &[], &mock).await.unwrap();
        assert!(result.contains("don't have any relevant knowledge"));
    }

    #[tokio::test]
    async fn test_synthesize_with_kus() {
        let kus = sample_kus();
        let mock = ku_ai::MockBackend::new()
            .with_chat_response("Water has various temperature states. It boils at 100°C (1) and freezes at 0°C (2).");
        let result = Synthesizer::synthesize("water temperature", &kus, &mock).await.unwrap();
        assert!(result.contains("Water"));
    }
}
