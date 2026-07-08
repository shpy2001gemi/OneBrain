//! 3-tier intent classification for the Personal AI Mediator.
//!
//! Tier 1: Keyword/pattern matching (~0ms)
//! Tier 2: Embedding similarity (~10ms) [placeholder for now]
//! Tier 3: LLM-based classification (~500ms-2s)

use serde::{Serialize, Deserialize};
use ku_ai::types::{ChatMessage, InferenceOptions};

/// User intent categories.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UserIntent {
    /// User wants to store knowledge.
    Encode { source: EncodeSource, trigger: EncodeTrigger },
    /// User wants to find/retrieve knowledge.
    Retrieve { query: String },
    /// User wants to explore connections between concepts.
    Connect { source: String, target: Option<String> },
    /// User wants AI to synthesize/explain knowledge.
    Synthesize { topic: String },
    /// User wants to query the knowledge graph.
    GraphQuery { nl_query: String },
    /// General conversation (no knowledge operation).
    FreeChat,
    /// Could not classify.
    Ambiguous,
}

/// Source of knowledge to encode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EncodeSource {
    Conversation,
    TextInput,
    Document,
}

/// What triggered the encoding request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EncodeTrigger {
    /// User explicitly asked to encode.
    Explicit,
    /// System proactively detected knowledge.
    Proactive,
}

/// Intent classifier with 3-tier approach.
pub struct IntentClassifier {
    // Keyword patterns for Tier 1
    encode_keywords: Vec<&'static str>,
    retrieve_keywords: Vec<&'static str>,
    connect_keywords: Vec<&'static str>,
    graph_keywords: Vec<&'static str>,
}

impl IntentClassifier {
    pub fn new() -> Self {
        Self {
            encode_keywords: vec![
                "remember", "save", "store", "encode", "note", "record",
                "nhớ", "lưu", "ghi", "ghi nhớ", // Vietnamese
            ],
            retrieve_keywords: vec![
                "what do i know", "find", "search", "look up", "retrieve",
                "what is", "tell me about", "explain",
                "tìm", "tra cứu", "cho biết", // Vietnamese
            ],
            connect_keywords: vec![
                "connect", "relate", "link", "how does", "relationship",
                "kết nối", "liên quan", // Vietnamese
            ],
            graph_keywords: vec![
                "graph", "network", "traverse", "path", "bonds",
                "đồ thị", "mạng lưới", // Vietnamese
            ],
        }
    }

    /// Classify user intent using 3-tier approach.
    /// Currently implements Tier 1 (keyword) + Tier 3 (LLM) fallback.
    pub fn classify(&self, input: &str) -> UserIntent {
        // Tier 1: Keyword matching
        let lower = input.to_lowercase();

        // Check encode patterns
        if self.encode_keywords.iter().any(|k| lower.contains(k)) {
            return UserIntent::Encode {
                source: EncodeSource::TextInput,
                trigger: EncodeTrigger::Explicit,
            };
        }

        // Check retrieve patterns
        if self.retrieve_keywords.iter().any(|k| lower.contains(k)) {
            return UserIntent::Retrieve { query: input.to_string() };
        }

        // Check connect patterns
        if self.connect_keywords.iter().any(|k| lower.contains(k)) {
            return UserIntent::Connect {
                source: input.to_string(),
                target: None,
            };
        }

        // Check graph patterns
        if self.graph_keywords.iter().any(|k| lower.contains(k)) {
            return UserIntent::GraphQuery { nl_query: input.to_string() };
        }

        // Tier 2: Embedding similarity (placeholder)
        // TODO: Phase 3 - compare input embedding to intent cluster centroids

        // Default: FreeChat for short, Ambiguous for long
        if input.len() < 20 {
            UserIntent::FreeChat
        } else {
            // Long messages likely contain knowledge
            UserIntent::Ambiguous
        }
    }

    /// Classify using LLM (Tier 3) — for when keyword matching is ambiguous.
    pub async fn classify_with_llm(
        &self,
        input: &str,
        backend: &dyn ku_ai::traits::ModelBackend,
    ) -> Result<UserIntent, crate::error::MediatorError> {
        let messages = vec![
            ChatMessage::system(
                "You are an intent classifier. Classify the user's message into one of: \
                 ENCODE (storing knowledge), RETRIEVE (asking questions), CONNECT (exploring relations), \
                 SYNTHESIZE (combining knowledge), GRAPH_QUERY (graph traversal), FREE_CHAT (general chat). \
                 Respond with ONLY the intent name, nothing else."
            ),
            ChatMessage::user(input),
        ];

        let options = InferenceOptions { temperature: 0.0, ..Default::default() };
        let response = backend.chat(&messages, &options).await
            .map_err(crate::error::MediatorError::Ai)?;

        // Parse response
        let intent_str = response.content.trim().to_uppercase();
        Ok(match intent_str.as_str() {
            s if s.contains("ENCODE") => UserIntent::Encode {
                source: EncodeSource::TextInput,
                trigger: EncodeTrigger::Proactive,
            },
            s if s.contains("RETRIEVE") => UserIntent::Retrieve { query: input.to_string() },
            s if s.contains("CONNECT") => UserIntent::Connect { source: input.to_string(), target: None },
            s if s.contains("SYNTHESIZE") => UserIntent::Synthesize { topic: input.to_string() },
            s if s.contains("GRAPH") => UserIntent::GraphQuery { nl_query: input.to_string() },
            _ => UserIntent::FreeChat,
        })
    }
}

impl Default for IntentClassifier {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classifier() -> IntentClassifier {
        IntentClassifier::new()
    }

    #[test]
    fn test_encode_intent_english() {
        let c = classifier();
        let intent = c.classify("Remember that water boils at 100°C");
        assert!(matches!(intent, UserIntent::Encode { trigger: EncodeTrigger::Explicit, .. }));
    }

    #[test]
    fn test_encode_intent_vietnamese() {
        let c = classifier();
        let intent = c.classify("Ghi nhớ rằng nước sôi ở 100 độ C");
        assert!(matches!(intent, UserIntent::Encode { .. }));
    }

    #[test]
    fn test_retrieve_intent() {
        let c = classifier();
        let intent = c.classify("What do I know about quantum physics?");
        assert!(matches!(intent, UserIntent::Retrieve { .. }));
    }

    #[test]
    fn test_retrieve_intent_explain() {
        let c = classifier();
        let intent = c.classify("Explain the theory of relativity");
        assert!(matches!(intent, UserIntent::Retrieve { .. }));
    }

    #[test]
    fn test_connect_intent() {
        let c = classifier();
        let intent = c.classify("How does photosynthesis relate to respiration?");
        assert!(matches!(intent, UserIntent::Connect { .. }));
    }

    #[test]
    fn test_graph_intent() {
        let c = classifier();
        let intent = c.classify("Show me the graph of my knowledge network");
        assert!(matches!(intent, UserIntent::GraphQuery { .. }));
    }

    #[test]
    fn test_free_chat_short() {
        let c = classifier();
        let intent = c.classify("Hello!");
        assert!(matches!(intent, UserIntent::FreeChat));
    }

    #[test]
    fn test_ambiguous_long() {
        let c = classifier();
        let intent = c.classify("I was thinking about the interesting dynamics of complex systems in nature");
        assert!(matches!(intent, UserIntent::Ambiguous));
    }

    #[test]
    fn test_vietnamese_retrieve() {
        let c = classifier();
        let intent = c.classify("Tìm kiến thức về vật lý lượng tử");
        assert!(matches!(intent, UserIntent::Retrieve { .. }));
    }

    #[test]
    fn test_vietnamese_connect() {
        let c = classifier();
        let intent = c.classify("Kết nối giữa quang hợp và hô hấp");
        assert!(matches!(intent, UserIntent::Connect { .. }));
    }

    #[tokio::test]
    async fn test_classify_with_llm_encode() {
        let c = classifier();
        let mock = ku_ai::MockBackend::new().with_chat_response("ENCODE");
        let result = c.classify_with_llm("save this fact", &mock).await.unwrap();
        assert!(matches!(result, UserIntent::Encode { .. }));
    }

    #[tokio::test]
    async fn test_classify_with_llm_free_chat() {
        let c = classifier();
        let mock = ku_ai::MockBackend::new().with_chat_response("FREE_CHAT");
        let result = c.classify_with_llm("hello there", &mock).await.unwrap();
        assert!(matches!(result, UserIntent::FreeChat));
    }
}
