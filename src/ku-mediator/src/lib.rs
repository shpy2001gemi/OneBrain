//! # ku-mediator — OneBrain Personal AI Mediator
//!
//! The user-facing "second brain" interface that orchestrates intent routing,
//! context management, knowledge encoding/retrieval, graph queries, and
//! proactive knowledge detection.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │                   Mediator                          │
//! │  ┌─────────────┐  ┌───────────┐  ┌──────────────┐  │
//! │  │   Intent     │  │  Context  │  │   Session    │  │
//! │  │  Classifier  │  │  Manager  │  │   State      │  │
//! │  └──────┬───────┘  └─────┬─────┘  └──────┬───────┘  │
//! │         │                │               │          │
//! │  ┌──────▼───────┐  ┌─────▼─────┐  ┌──────▼───────┐  │
//! │  │  Retriever   │  │ Encoder   │  │ Graph Agent  │  │
//! │  │  (Hybrid)    │  │ (AI+Rule) │  │ (NL→KQL)    │  │
//! │  └──────────────┘  └───────────┘  └──────────────┘  │
//! │  ┌──────────────┐  ┌───────────┐  ┌──────────────┐  │
//! │  │ Deduplicator │  │ Detector  │  │ Synthesizer  │  │
//! │  └──────────────┘  └───────────┘  └──────────────┘  │
//! │  ┌──────────────┐                                   │
//! │  │   Profile     │                                   │
//! │  └──────────────┘                                   │
//! └─────────────────────────────────────────────────────┘
//!       │              │              │
//!       ▼              ▼              ▼
//!    ku-ai          ku-encoder      ku-core
//! ```
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use ku_mediator::mediator::{Mediator, MediatorConfig};
//! use ku_mediator::input::UserInput;
//! use ku_ai::backend::mock::MockBackend;
//! use ku_core::text_parser::default_dict;
//!
//! # async fn example() {
//! let backend = MockBackend::new().with_chat_response("Hello!");
//! let mut mediator = Mediator::new(
//!     Box::new(backend),
//!     default_dict(),
//!     MediatorConfig::default(),
//! );
//!
//! let response = mediator.process(UserInput::Text("Hello!".into())).await.unwrap();
//! println!("{}", response.text);
//! # }
//! ```

pub mod error;
pub mod intent;
pub mod context;
pub mod session;
pub mod retriever;
pub mod deduplicator;
pub mod detector;
pub mod graph_agent;
pub mod synthesizer;
pub mod profile;
pub mod input;
pub mod output;
pub mod mediator;

// ─── Re-exports ─────────────────────────────────────────────────────────

pub use error::MediatorError;
pub use intent::{UserIntent, EncodeSource, EncodeTrigger, IntentClassifier};
pub use context::{ContextManager, ContextBudget, MessageRole};
pub use session::{MediatorSession, ConversationMode, SessionId};
pub use retriever::{KuRetriever, RetrievedKU, RetrievalSource, RetrieverConfig};
pub use deduplicator::{KnowledgeDeduplicator, DeduplicationResult};
pub use detector::{KnowledgeDetector, KnowledgeSignal, SignalType};
pub use graph_agent::{GraphAgent, KqlResult, KqlSource};
pub use synthesizer::Synthesizer;
pub use profile::{UserProfile, ResponseStyle, ExpertiseArea};
pub use input::UserInput;
pub use output::MediatorResponse;
pub use mediator::{Mediator, MediatorConfig};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_public_api_accessible() {
        // Verify key types are accessible through the public API
        let _classifier = IntentClassifier::new();
        let _context = ContextManager::new(10);
        let _session = MediatorSession::new();
        let _retriever = KuRetriever::default();
        let _dedup = KnowledgeDeduplicator::new();
        let _detector = KnowledgeDetector::new();
        let _agent = GraphAgent::new();
        let _profile = UserProfile::new("test");
        let _config = MediatorConfig::default();
    }

    #[test]
    fn test_intent_re_export() {
        let intent = UserIntent::FreeChat;
        assert!(matches!(intent, UserIntent::FreeChat));
    }

    #[test]
    fn test_response_re_export() {
        let response = MediatorResponse::chat("hello");
        assert_eq!(response.text, "hello");
    }

    #[test]
    fn test_input_re_export() {
        let input = UserInput::Text("test".into());
        assert_eq!(input.to_text(), "test");
    }

    #[test]
    fn test_budget_re_export() {
        let budget = ContextBudget::default_8k();
        assert_eq!(budget.total(), 8000);
    }
}
