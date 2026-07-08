//! Multi-modal input handlers.
//!
//! Phase 1: Text only. Phase 3: Voice, Image, PDF.

pub mod text;
pub use text::TextInput;

/// Unified input type.
#[derive(Debug, Clone)]
pub enum UserInput {
    /// Text message from the user.
    Text(String),
    // Future: Voice(Vec<u8>), Image(Vec<u8>), Document(PathBuf)
}

impl UserInput {
    /// Convert any input to text representation.
    pub fn to_text(&self) -> String {
        match self {
            UserInput::Text(s) => s.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_input_text() {
        let input = UserInput::Text("hello world".to_string());
        assert_eq!(input.to_text(), "hello world");
    }
}
