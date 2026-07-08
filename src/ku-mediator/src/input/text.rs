//! Text input handler.

/// Process raw text input.
pub struct TextInput;

impl TextInput {
    /// Clean and normalize text input.
    pub fn process(raw: &str) -> String {
        raw.trim().to_string()
    }

    /// Check if input is too short to process.
    pub fn is_too_short(text: &str) -> bool {
        text.trim().len() < 3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_trims() {
        assert_eq!(TextInput::process("  hello  "), "hello");
    }

    #[test]
    fn test_is_too_short() {
        assert!(TextInput::is_too_short("hi"));
        assert!(TextInput::is_too_short("  "));
        assert!(!TextInput::is_too_short("hello world"));
    }
}
