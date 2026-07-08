//! Text output formatting.

/// Format responses for text output.
pub struct TextFormatter;

impl TextFormatter {
    /// Wrap text to a maximum width at word boundaries.
    pub fn wrap(text: &str, max_width: usize) -> String {
        if text.len() <= max_width {
            return text.to_string();
        }
        // Simple wrap at word boundaries
        let mut result = String::new();
        let mut line_len = 0;
        for word in text.split_whitespace() {
            if line_len + word.len() + 1 > max_width && line_len > 0 {
                result.push('\n');
                line_len = 0;
            }
            if line_len > 0 {
                result.push(' ');
                line_len += 1;
            }
            result.push_str(word);
            line_len += word.len();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wrap_short_text() {
        let result = TextFormatter::wrap("hello world", 80);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_wrap_long_text() {
        let result = TextFormatter::wrap("one two three four five six seven eight nine ten", 15);
        assert!(result.contains('\n'));
    }
}
