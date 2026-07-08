//! Session state management.
//!
//! Tracks conversation mode, encoded KUs, and query counts
//! across a single user interaction session.

use serde::{Serialize, Deserialize};
use crate::intent::UserIntent;

/// Unique session identifier.
pub type SessionId = String;

/// Conversation mode — drives system prompt and behavior.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConversationMode {
    Encoding,
    Retrieval,
    GraphExplore,
    Synthesis,
    FreeChat,
}

/// Active session state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediatorSession {
    pub session_id: SessionId,
    pub started_at: u64,
    pub mode: ConversationMode,
    /// CID hex strings of KUs encoded in this session.
    pub encoded_kus: Vec<String>,
    pub queries_count: usize,
    /// Serialized last intent (for debugging/logging).
    pub last_intent: Option<String>,
}

impl MediatorSession {
    pub fn new() -> Self {
        Self {
            session_id: generate_session_id(),
            started_at: current_timestamp_ms(),
            mode: ConversationMode::FreeChat,
            encoded_kus: Vec::new(),
            queries_count: 0,
            last_intent: None,
        }
    }

    /// Update mode based on the classified intent.
    pub fn update_mode(&mut self, intent: &UserIntent) {
        self.mode = match intent {
            UserIntent::Encode { .. } => ConversationMode::Encoding,
            UserIntent::Retrieve { .. } => ConversationMode::Retrieval,
            UserIntent::Connect { .. } | UserIntent::GraphQuery { .. } => ConversationMode::GraphExplore,
            UserIntent::Synthesize { .. } => ConversationMode::Synthesis,
            UserIntent::FreeChat | UserIntent::Ambiguous => ConversationMode::FreeChat,
        };
        self.last_intent = Some(format!("{:?}", intent));
    }

    /// Record an encoded KU by its CID hex string.
    pub fn record_encoding(&mut self, cid_hex: String) {
        self.encoded_kus.push(cid_hex);
    }

    /// Record a query.
    pub fn record_query(&mut self) {
        self.queries_count += 1;
    }

    /// Session duration in seconds.
    pub fn duration_secs(&self) -> u64 {
        (current_timestamp_ms() - self.started_at) / 1000
    }
}

impl Default for MediatorSession {
    fn default() -> Self { Self::new() }
}

fn generate_session_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let ts = current_timestamp_ms();
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u64(ts);
    let random = hasher.finish();
    format!("session_{}_{:08x}", ts, random as u32)
}

fn current_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::{EncodeSource, EncodeTrigger};

    #[test]
    fn test_new_session_defaults() {
        let session = MediatorSession::new();
        assert!(session.session_id.starts_with("session_"));
        assert_eq!(session.mode, ConversationMode::FreeChat);
        assert!(session.encoded_kus.is_empty());
        assert_eq!(session.queries_count, 0);
    }

    #[test]
    fn test_update_mode_encode() {
        let mut session = MediatorSession::new();
        session.update_mode(&UserIntent::Encode {
            source: EncodeSource::TextInput,
            trigger: EncodeTrigger::Explicit,
        });
        assert_eq!(session.mode, ConversationMode::Encoding);
        assert!(session.last_intent.is_some());
    }

    #[test]
    fn test_update_mode_retrieve() {
        let mut session = MediatorSession::new();
        session.update_mode(&UserIntent::Retrieve { query: "test".into() });
        assert_eq!(session.mode, ConversationMode::Retrieval);
    }

    #[test]
    fn test_update_mode_graph() {
        let mut session = MediatorSession::new();
        session.update_mode(&UserIntent::GraphQuery { nl_query: "test".into() });
        assert_eq!(session.mode, ConversationMode::GraphExplore);
    }

    #[test]
    fn test_update_mode_connect() {
        let mut session = MediatorSession::new();
        session.update_mode(&UserIntent::Connect { source: "a".into(), target: Some("b".into()) });
        assert_eq!(session.mode, ConversationMode::GraphExplore);
    }

    #[test]
    fn test_update_mode_synthesize() {
        let mut session = MediatorSession::new();
        session.update_mode(&UserIntent::Synthesize { topic: "ai".into() });
        assert_eq!(session.mode, ConversationMode::Synthesis);
    }

    #[test]
    fn test_record_encoding() {
        let mut session = MediatorSession::new();
        session.record_encoding("abc123".to_string());
        session.record_encoding("def456".to_string());
        assert_eq!(session.encoded_kus.len(), 2);
    }

    #[test]
    fn test_record_query() {
        let mut session = MediatorSession::new();
        session.record_query();
        session.record_query();
        assert_eq!(session.queries_count, 2);
    }

    #[test]
    fn test_duration_secs() {
        let session = MediatorSession::new();
        // Duration should be >= 0 (just created)
        assert!(session.duration_secs() < 5);
    }
}
