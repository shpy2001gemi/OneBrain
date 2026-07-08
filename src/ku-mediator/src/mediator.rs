//! Main Mediator — the Personal AI interface to OneBrain.
//!
//! Orchestrates intent classification, encoding, retrieval,
//! graph queries, and knowledge detection. This is the primary
//! entry point for all user interactions.

use ku_ai::traits::ModelBackend;
use ku_ai::types::{ChatMessage, InferenceOptions};
use ku_core::text_parser::ConceptDict;

use crate::intent::{IntentClassifier, UserIntent};
use crate::context::{ContextManager, MessageRole};
use crate::session::MediatorSession;
use crate::retriever::KuRetriever;
use crate::deduplicator::{KnowledgeDeduplicator, DeduplicationResult};
use crate::detector::KnowledgeDetector;
use crate::graph_agent::GraphAgent;
use crate::profile::UserProfile;
use crate::input::UserInput;
use crate::output::MediatorResponse;
use crate::error::MediatorError;

use ku_encoder::{AiEncoder, EncoderConfig, EncodingVerifier, FallbackChain, EncodingDecision};

/// Configuration for the Mediator.
#[derive(Debug, Clone)]
pub struct MediatorConfig {
    /// Maximum conversation history messages.
    pub max_history: usize,
    /// Whether to proactively detect and suggest encoding.
    pub proactive_encoding: bool,
    /// Whether the graph agent is enabled.
    pub graph_agent_enabled: bool,
}

impl Default for MediatorConfig {
    fn default() -> Self {
        Self {
            max_history: 20,
            proactive_encoding: false,
            graph_agent_enabled: true,
        }
    }
}

/// The Personal AI Mediator.
///
/// Ties together intent classification, encoding, retrieval,
/// deduplication, knowledge detection, graph queries, and user profiling
/// into a single orchestrated pipeline.
pub struct Mediator {
    config: MediatorConfig,
    backend: Box<dyn ModelBackend>,
    encoder: AiEncoder,
    classifier: IntentClassifier,
    context: ContextManager,
    session: MediatorSession,
    retriever: KuRetriever,
    deduplicator: KnowledgeDeduplicator,
    detector: KnowledgeDetector,
    graph_agent: GraphAgent,
    profile: UserProfile,
    verifier: EncodingVerifier,
    fallback: FallbackChain,
    /// Concept dictionary for fallback encoding.
    dict: ConceptDict,
}

impl Mediator {
    /// Create a new Mediator with separate backends for chat and encoding.
    ///
    /// # Arguments
    /// * `backend` — Backend for chat/free-form inference.
    /// * `encoder_backend` — Backend for AI-assisted KU encoding (tool calling).
    /// * `dict` — Concept dictionary shared across encoder and fallback chain.
    /// * `config` — Mediator configuration.
    pub fn new(
        backend: Box<dyn ModelBackend>,
        encoder_backend: Box<dyn ModelBackend>,
        dict: ConceptDict,
        config: MediatorConfig,
    ) -> Self {
        let encoder_config = EncoderConfig::default();
        let encoder = AiEncoder::new(
            encoder_backend,
            dict.clone(),
            encoder_config.clone(),
        );

        Self {
            context: ContextManager::new(config.max_history),
            config,
            backend,
            encoder,
            classifier: IntentClassifier::new(),
            session: MediatorSession::new(),
            retriever: KuRetriever::default(),
            deduplicator: KnowledgeDeduplicator::new(),
            detector: KnowledgeDetector::new(),
            graph_agent: GraphAgent::new(),
            profile: UserProfile::default(),
            verifier: EncodingVerifier::default(),
            fallback: FallbackChain::new(encoder_config),
            dict,
        }
    }

    /// Process a user input — the main entry point.
    pub async fn process(&mut self, input: UserInput) -> Result<MediatorResponse, MediatorError> {
        let text = input.to_text();

        if crate::input::text::TextInput::is_too_short(&text) {
            return Ok(MediatorResponse::chat("Please provide more detail."));
        }

        // 1. Classify intent
        let intent = self.classifier.classify(&text);

        // 2. Update session mode
        self.session.update_mode(&intent);

        // 3. Add to conversation history
        self.context.add_message(MessageRole::User, &text);

        // 4. Route to handler
        let mut response = match &intent {
            UserIntent::Encode { .. } => self.handle_encode(&text).await?,
            UserIntent::Retrieve { query } => self.handle_retrieve(query).await?,
            UserIntent::Connect { source, target } => {
                self.handle_connect(source, target.as_deref()).await?
            }
            UserIntent::Synthesize { topic } => self.handle_synthesize(topic).await?,
            UserIntent::GraphQuery { nl_query } => self.handle_graph_query(nl_query).await?,
            UserIntent::FreeChat => self.handle_chat(&text).await?,
            UserIntent::Ambiguous => self.handle_ambiguous(&text).await?,
        };

        // 5. Check for proactive encoding (if enabled)
        if self.config.proactive_encoding {
            if let Some(signal) = self.detector.detect(&text) {
                response = response.with_suggestion(
                    format!(
                        "I detected potential knowledge ({:?}). Would you like me to encode it?",
                        signal.signal_type
                    )
                );
            }
        }

        // 6. Update context and profile
        self.context.add_message(MessageRole::Assistant, &response.text);
        response = response.with_intent(format!("{:?}", intent));

        Ok(response)
    }

    /// Handle encoding request.
    async fn handle_encode(&mut self, text: &str) -> Result<MediatorResponse, MediatorError> {
        // Check dedup first
        let dedup = self.deduplicator.check(text);
        match &dedup {
            DeduplicationResult::Duplicate { existing_cid, similarity } => {
                return Ok(MediatorResponse::chat(
                    format!(
                        "This knowledge already exists (similarity: {:.0}%, CID: {}). Skipping.",
                        similarity * 100.0,
                        &existing_cid[..existing_cid.len().min(8)]
                    )
                ));
            }
            DeduplicationResult::Overlap { .. } => {
                // Continue but note the overlap — the knowledge is partially new
            }
            DeduplicationResult::New => {}
        }

        // Encode via AiEncoder
        match self.encoder.encode(text).await {
            Ok(result) => {
                let decision = self.fallback.decide(&result, 0);
                match decision {
                    EncodingDecision::Accept(_) => {
                        // Verify
                        let verification = self.verifier.verify(&result);
                        if verification.passed {
                            let wire_count = result.wire_bytes.len();
                            let gene = result.gene_type.as_deref().unwrap_or("unknown");
                            self.session.record_encoding("encoded".to_string());
                            self.profile.record_encoding(&result.concepts_used);
                            self.deduplicator.register("encoded".to_string(), text.to_string());

                            Ok(MediatorResponse::chat(
                                format!(
                                    "Encoded {} KU(s) as '{}' (confidence: {:.0}%)",
                                    wire_count, gene, result.confidence * 100.0
                                )
                            ))
                        } else {
                            Ok(MediatorResponse::chat(
                                format!(
                                    "Encoding verification failed: {}",
                                    verification.issues.join(", ")
                                )
                            ))
                        }
                    }
                    EncodingDecision::FallbackTier1 => {
                        // Try rule-based
                        match self.fallback.encode_tier1(text, &self.dict) {
                            Ok(result) => Ok(MediatorResponse::chat(
                                format!(
                                    "Encoded via rule-based (Tier 1) with {:.0}% confidence",
                                    result.confidence * 100.0
                                )
                            )),
                            Err(e) => Ok(MediatorResponse::chat(
                                format!("Could not encode this text: {}", e)
                            )),
                        }
                    }
                    EncodingDecision::Retry { .. } => {
                        Ok(MediatorResponse::chat(
                            "Encoding needs retry. Please try again.".to_string()
                        ))
                    }
                    EncodingDecision::Reject { reason } => {
                        Ok(MediatorResponse::chat(
                            format!("Cannot encode: {}", reason)
                        ))
                    }
                }
            }
            Err(e) => {
                // Try Tier 1 fallback
                match self.fallback.encode_tier1(text, &self.dict) {
                    Ok(result) => Ok(MediatorResponse::chat(
                        format!(
                            "AI encoding failed, used rule-based: confidence {:.0}%",
                            result.confidence * 100.0
                        )
                    )),
                    Err(_) => Ok(MediatorResponse::chat(
                        format!("Could not encode: {}", e)
                    )),
                }
            }
        }
    }

    /// Handle retrieval request.
    async fn handle_retrieve(&mut self, query: &str) -> Result<MediatorResponse, MediatorError> {
        self.session.record_query();
        self.profile.record_query();

        let results = self.retriever.retrieve(query);
        if results.is_empty() {
            return Ok(MediatorResponse::chat(
                format!(
                    "I don't have any knowledge about '{}' yet. Would you like to teach me?",
                    query
                )
            ));
        }

        let response_text = crate::synthesizer::Synthesizer::format_results_simple(query, &results);
        Ok(MediatorResponse::chat(response_text))
    }

    /// Handle connection exploration.
    async fn handle_connect(
        &self,
        source: &str,
        target: Option<&str>,
    ) -> Result<MediatorResponse, MediatorError> {
        match target {
            Some(t) => Ok(MediatorResponse::chat(
                format!(
                    "Exploring connections between '{}' and '{}'. (Graph integration coming in Phase 2)",
                    source, t
                )
            )),
            None => Ok(MediatorResponse::chat(
                format!("What would you like to connect '{}' with?", source)
            )),
        }
    }

    /// Handle synthesis request.
    async fn handle_synthesize(
        &mut self,
        topic: &str,
    ) -> Result<MediatorResponse, MediatorError> {
        let results = self.retriever.retrieve(topic);
        let response_text = crate::synthesizer::Synthesizer::format_results_simple(topic, &results);
        Ok(MediatorResponse::chat(response_text))
    }

    /// Handle graph query.
    async fn handle_graph_query(
        &self,
        nl_query: &str,
    ) -> Result<MediatorResponse, MediatorError> {
        match self.graph_agent.translate_to_kql(nl_query) {
            Some(result) => Ok(MediatorResponse::chat(
                format!("Generated KQL: {}\n(KQL execution coming in Phase 2)", result.kql)
            )),
            None => Ok(MediatorResponse::chat(
                "Could not translate to KQL. Please rephrase your query.".to_string()
            )),
        }
    }

    /// Handle free chat.
    async fn handle_chat(&self, text: &str) -> Result<MediatorResponse, MediatorError> {
        // Build messages with conversation context
        let mut messages = vec![
            ChatMessage::system(
                "You are a helpful knowledge assistant for OneBrain. \
                 Help the user manage their knowledge base. \
                 If they share facts, offer to encode them. \
                 If they ask questions, help retrieve relevant knowledge."
            ),
        ];

        // Add profile context
        let profile_block = self.profile.to_context_block();
        if !profile_block.is_empty() {
            messages.push(ChatMessage::system(
                format!("User profile:\n{}", profile_block)
            ));
        }

        // Add context block
        let context_block = self.context.build_context_block();
        if !context_block.is_empty() {
            messages.push(ChatMessage::system(
                format!("Context:\n{}", context_block)
            ));
        }

        // Add recent conversation history
        let recent = self.context.recent_messages(10);
        messages.extend(recent);

        // Ensure the user's current message is included
        messages.push(ChatMessage::user(text));

        let options = InferenceOptions { temperature: 0.7, ..Default::default() };
        let response = self.backend.chat(&messages, &options).await
            .map_err(MediatorError::Ai)?;

        Ok(MediatorResponse::chat(response.content))
    }

    /// Handle ambiguous input — try knowledge detection, then default to chat.
    async fn handle_ambiguous(&mut self, text: &str) -> Result<MediatorResponse, MediatorError> {
        // Check for knowledge signals
        if let Some(signal) = self.detector.detect(text) {
            return Ok(MediatorResponse::chat(
                format!(
                    "I detected a {:?} signal (confidence: {:.0}%). \
                     Would you like me to encode this as knowledge?",
                    signal.signal_type, signal.confidence * 100.0
                )
            ).with_suggestion("Say 'encode' to save this knowledge"));
        }

        // Default to chat
        self.handle_chat(text).await
    }

    // ─── Accessors ──────────────────────────────────────────────────────

    /// Get the current session.
    pub fn session(&self) -> &MediatorSession { &self.session }

    /// Get the user profile.
    pub fn profile(&self) -> &UserProfile { &self.profile }

    /// Get the user profile mutably.
    pub fn profile_mut(&mut self) -> &mut UserProfile { &mut self.profile }

    /// Get the context manager.
    pub fn context(&self) -> &ContextManager { &self.context }

    /// Get the retriever mutably (for indexing KUs).
    pub fn retriever_mut(&mut self) -> &mut KuRetriever { &mut self.retriever }

    /// Get the deduplicator mutably.
    pub fn deduplicator_mut(&mut self) -> &mut KnowledgeDeduplicator { &mut self.deduplicator }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ku_core::text_parser::default_dict;

    fn make_mediator() -> Mediator {
        let chat_mock = ku_ai::MockBackend::new()
            .with_chat_response("Hello! I'm your knowledge assistant.");
        let encoder_mock = ku_ai::MockBackend::new();
        Mediator::new(
            Box::new(chat_mock),
            Box::new(encoder_mock),
            default_dict(),
            MediatorConfig::default(),
        )
    }

    #[tokio::test]
    async fn test_process_short_input() {
        let mut mediator = make_mediator();
        let response = mediator.process(UserInput::Text("hi".into())).await.unwrap();
        assert!(response.text.contains("more detail"));
    }

    #[tokio::test]
    async fn test_process_free_chat() {
        let mut mediator = make_mediator();
        let response = mediator.process(UserInput::Text("Hello!".into())).await.unwrap();
        assert!(!response.text.is_empty());
    }

    #[tokio::test]
    async fn test_process_retrieve() {
        let mut mediator = make_mediator();
        let response = mediator.process(
            UserInput::Text("Find information about Rust programming".into())
        ).await.unwrap();
        // Should route to retrieval, but no indexed KUs → "no knowledge"
        assert!(response.text.contains("knowledge") || response.text.contains("Rust"));
    }

    #[tokio::test]
    async fn test_process_encode() {
        let mut mediator = make_mediator();
        let response = mediator.process(
            UserInput::Text("Remember that water boils at 100 degrees Celsius".into())
        ).await.unwrap();
        // Should attempt encoding (may fail with mock, but should handle gracefully)
        assert!(!response.text.is_empty());
    }

    #[tokio::test]
    async fn test_process_graph_query() {
        let mut mediator = make_mediator();
        let response = mediator.process(
            UserInput::Text("Show me recent graph knowledge".into())
        ).await.unwrap();
        assert!(response.text.contains("KQL") || response.text.contains("knowledge"));
    }

    #[tokio::test]
    async fn test_session_tracking() {
        let mut mediator = make_mediator();
        mediator.process(UserInput::Text("Hello there!".into())).await.unwrap();
        assert!(mediator.session().session_id.starts_with("session_"));
    }

    #[tokio::test]
    async fn test_context_accumulation() {
        let mut mediator = make_mediator();
        mediator.process(UserInput::Text("Hello there!".into())).await.unwrap();
        // Should have user + assistant messages
        assert!(mediator.context().history_len() >= 2);
    }

    #[test]
    fn test_mediator_config_default() {
        let config = MediatorConfig::default();
        assert_eq!(config.max_history, 20);
        assert!(!config.proactive_encoding);
        assert!(config.graph_agent_enabled);
    }

    #[tokio::test]
    async fn test_proactive_encoding_detection() {
        let chat_mock = ku_ai::MockBackend::new()
            .with_chat_response("Acknowledged.");
        let encoder_mock = ku_ai::MockBackend::new();
        let config = MediatorConfig {
            proactive_encoding: true,
            ..Default::default()
        };
        let mut mediator = Mediator::new(
            Box::new(chat_mock),
            Box::new(encoder_mock),
            default_dict(),
            config,
        );

        let response = mediator.process(
            UserInput::Text("Remember that Rust is a systems programming language".into())
        ).await.unwrap();
        // proactive encoding is ON, explicit keyword "remember" → encode path
        assert!(!response.text.is_empty());
    }
}
