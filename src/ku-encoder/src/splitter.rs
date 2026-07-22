//! # Splitter — paragraph-level text splitting.
//!
//! Splits long input text into paragraphs for individual AI processing.
//! This is a simple, deterministic step — no AI needed.
//!
//! # Design
//! - Split on double newlines (`\n\n`)
//! - Trim whitespace from each paragraph
//! - Drop empty paragraphs
//! - If no double newline found, return the whole text as one paragraph

/// Split text into paragraphs by double newlines.
///
/// Each paragraph is trimmed. Empty paragraphs are discarded.
///
/// # Examples
/// ```
/// use ku_encoder::splitter::split_paragraphs;
///
/// let text = "First paragraph.\n\nSecond paragraph.\n\nThird.";
/// let paragraphs = split_paragraphs(text);
/// assert_eq!(paragraphs.len(), 3);
/// assert_eq!(paragraphs[0], "First paragraph.");
/// assert_eq!(paragraphs[2], "Third.");
/// ```
pub fn split_paragraphs(text: &str) -> Vec<String> {
    // Normalize CRLF → LF for cross-platform support
    let normalized = text.replace("\r\n", "\n");
    normalized
        .split("\n\n")
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_paragraph() {
        let result = split_paragraphs("Bàn làm việc thường có 4 chân.");
        assert_eq!(result, vec!["Bàn làm việc thường có 4 chân."]);
    }

    #[test]
    fn test_three_paragraphs() {
        let text = "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.";
        let result = split_paragraphs(text);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "First paragraph.");
        assert_eq!(result[1], "Second paragraph.");
        assert_eq!(result[2], "Third paragraph.");
    }

    #[test]
    fn test_extra_whitespace_trimmed() {
        let text = "  First with spaces.  \n\n  Second with spaces.  ";
        let result = split_paragraphs(text);
        assert_eq!(result, vec!["First with spaces.", "Second with spaces."]);
    }

    #[test]
    fn test_multiple_blank_lines() {
        let text = "First.\n\n\n\nSecond.";
        let result = split_paragraphs(text);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "First.");
        assert_eq!(result[1], "Second.");
    }

    #[test]
    fn test_empty_input() {
        let result = split_paragraphs("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_only_whitespace() {
        let result = split_paragraphs("   \n\n   \n\n   ");
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_newline_no_split() {
        // Single newline should NOT split (only \n\n splits)
        let text = "Line 1.\nLine 2.\nLine 3.";
        let result = split_paragraphs(text);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "Line 1.\nLine 2.\nLine 3.");
    }

    #[test]
    fn test_unicode_content() {
        let text = "Nước sôi ở 100°C.\n\n물은 100°C에서 끓는다.";
        let result = split_paragraphs(text);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "Nước sôi ở 100°C.");
        assert_eq!(result[1], "물은 100°C에서 끓는다.");
    }

    #[test]
    fn test_windows_crlf() {
        let text = "First.\r\n\r\nSecond.";
        let result = split_paragraphs(text);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "First.");
        assert_eq!(result[1], "Second.");
    }
}
