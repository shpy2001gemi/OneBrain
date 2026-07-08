//! Response formatting.

pub mod text;
pub use text::TextFormatter;

/// Mediator response returned from processing user input.
#[derive(Debug, Clone)]
pub struct MediatorResponse {
    /// The text response to show the user.
    pub text: String,
    /// Detected intent (for debugging/logging).
    pub intent_detected: Option<String>,
    /// CID hex strings of KUs encoded during this interaction.
    pub kus_encoded: Vec<String>,
    /// Number of KUs retrieved.
    pub kus_retrieved: usize,
    /// Follow-up suggestions for the user.
    pub suggestions: Vec<String>,
}

impl MediatorResponse {
    /// Create a simple chat response.
    pub fn chat(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            intent_detected: None,
            kus_encoded: Vec::new(),
            kus_retrieved: 0,
            suggestions: Vec::new(),
        }
    }

    /// Attach the detected intent.
    pub fn with_intent(mut self, intent: impl Into<String>) -> Self {
        self.intent_detected = Some(intent.into());
        self
    }

    /// Add a follow-up suggestion.
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestions.push(suggestion.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_response() {
        let r = MediatorResponse::chat("hello");
        assert_eq!(r.text, "hello");
        assert!(r.intent_detected.is_none());
        assert!(r.kus_encoded.is_empty());
    }

    #[test]
    fn test_with_intent() {
        let r = MediatorResponse::chat("hi").with_intent("FreeChat");
        assert_eq!(r.intent_detected.as_deref(), Some("FreeChat"));
    }

    #[test]
    fn test_with_suggestion() {
        let r = MediatorResponse::chat("result")
            .with_suggestion("Try asking about connections");
        assert_eq!(r.suggestions.len(), 1);
    }
}
