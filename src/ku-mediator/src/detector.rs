//! Knowledge signal detection.
//!
//! Detects when user messages contain encodable knowledge
//! using pattern matching for explicit commands, definitions,
//! and procedural statements.

use serde::{Deserialize, Serialize};

/// A detected knowledge signal from conversation text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSignal {
    /// Type of signal detected.
    pub signal_type: SignalType,
    /// Confidence in the detection (0.0-1.0).
    pub confidence: f32,
    /// Suggested gene type for encoding.
    pub suggested_gene_type: Option<String>,
    /// The text to potentially encode.
    pub extract: String,
}

/// The type of knowledge signal detected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SignalType {
    /// User explicitly says "remember this", "save", etc.
    Explicit,
    /// User shares a fact without asking to save.
    Implicit,
    /// "X is Y" pattern.
    Definition,
    /// "To do X, first..." pattern.
    Procedure,
}

/// Detects knowledge signals from conversation messages.
pub struct KnowledgeDetector {
    explicit_patterns: Vec<&'static str>,
    definition_patterns: Vec<&'static str>,
    procedure_patterns: Vec<&'static str>,
    min_length: usize,
}

impl KnowledgeDetector {
    pub fn new() -> Self {
        Self {
            explicit_patterns: vec![
                "remember",
                "save this",
                "note that",
                "encode",
                "nhớ",
                "lưu lại",
                "ghi nhớ",
            ],
            definition_patterns: vec![
                " is ",
                " are ",
                " means ",
                " refers to ",
                " defines ",
                " là ",
                " nghĩa là ",
            ],
            procedure_patterns: vec![
                "step 1",
                "first,",
                "to do this",
                "how to",
                "bước 1",
                "đầu tiên",
            ],
            min_length: 20,
        }
    }

    /// Detect knowledge signals in a message.
    ///
    /// Returns `None` if the message is too short or no signal is detected.
    pub fn detect(&self, message: &str) -> Option<KnowledgeSignal> {
        if message.len() < self.min_length {
            return None;
        }

        let lower = message.to_lowercase();

        // Check explicit patterns (highest priority)
        if self.explicit_patterns.iter().any(|p| lower.contains(p)) {
            return Some(KnowledgeSignal {
                signal_type: SignalType::Explicit,
                confidence: 0.95,
                suggested_gene_type: None,
                extract: message.to_string(),
            });
        }

        // Check definition patterns
        if self.definition_patterns.iter().any(|p| lower.contains(p)) {
            return Some(KnowledgeSignal {
                signal_type: SignalType::Definition,
                confidence: 0.70,
                suggested_gene_type: Some("fact".to_string()),
                extract: message.to_string(),
            });
        }

        // Check procedure patterns
        if self.procedure_patterns.iter().any(|p| lower.contains(p)) {
            return Some(KnowledgeSignal {
                signal_type: SignalType::Procedure,
                confidence: 0.75,
                suggested_gene_type: Some("procedure".to_string()),
                extract: message.to_string(),
            });
        }

        None
    }
}

impl Default for KnowledgeDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detector() -> KnowledgeDetector {
        KnowledgeDetector::new()
    }

    #[test]
    fn test_explicit_signal() {
        let d = detector();
        let signal = d.detect("Remember that water boils at 100 degrees");
        assert!(signal.is_some());
        let signal = signal.unwrap();
        assert_eq!(signal.signal_type, SignalType::Explicit);
        assert!(signal.confidence > 0.9);
    }

    #[test]
    fn test_definition_signal() {
        let d = detector();
        let signal = d.detect("Photosynthesis is the process by which plants make food");
        assert!(signal.is_some());
        let signal = signal.unwrap();
        assert_eq!(signal.signal_type, SignalType::Definition);
        assert_eq!(signal.suggested_gene_type.as_deref(), Some("fact"));
    }

    #[test]
    fn test_procedure_signal() {
        let d = detector();
        let signal = d.detect("To do this, first you need to install Rust");
        assert!(signal.is_some());
        let signal = signal.unwrap();
        assert_eq!(signal.signal_type, SignalType::Procedure);
        assert_eq!(signal.suggested_gene_type.as_deref(), Some("procedure"));
    }

    #[test]
    fn test_too_short() {
        let d = detector();
        let signal = d.detect("Hello world");
        assert!(signal.is_none());
    }

    #[test]
    fn test_no_signal() {
        let d = detector();
        let signal = d.detect("I was thinking about going for a walk today in the park");
        assert!(signal.is_none());
    }

    #[test]
    fn test_vietnamese_explicit() {
        let d = detector();
        let signal = d.detect("Ghi nhớ rằng nước sôi ở nhiệt độ 100 độ");
        assert!(signal.is_some());
        assert_eq!(signal.unwrap().signal_type, SignalType::Explicit);
    }

    #[test]
    fn test_vietnamese_definition() {
        let d = detector();
        let signal = d.detect("Quang hợp là quá trình thực vật tạo thức ăn");
        assert!(signal.is_some());
        assert_eq!(signal.unwrap().signal_type, SignalType::Definition);
    }

    #[test]
    fn test_vietnamese_procedure() {
        let d = detector();
        let signal = d.detect("Bước 1: cài đặt Rust trên máy tính");
        assert!(signal.is_some());
        assert_eq!(signal.unwrap().signal_type, SignalType::Procedure);
    }
}
