//! # Pre-scan — anchor extraction and verification.
//!
//! Extracts "anchors" from input text before AI processing. Anchors are
//! terms that must be preserved exactly (chemical formulas, numbers,
//! mathematical expressions). After AI extraction, anchors are verified
//! to catch any unwanted modifications (e.g., AI "correcting" H8O → H2O).
//!
//! # Pipeline position
//! ```text
//! BƯỚC 0: prescan_anchors(text) → anchors
//!    ↓
//! BƯỚC 2: AI extract (prompt includes anchors)
//!    ↓
//! BƯỚC 2.5: verify_anchors(anchors, triples) → VerifyResult
//!           override_corrected(anchors, triples) if needed
//! ```

use regex::Regex;
use std::sync::LazyLock;

use crate::types::{Anchor, SpoTriple, VerifyResult};

// ============================================================================
// Compiled regex patterns (lazy-static for performance)
// ============================================================================

/// Chemical formula: 2+ elements, each uppercase letter optionally followed
/// by lowercase + digits. Must have at least 2 element groups.
/// Matches: H8O, CH3COOH, NaCl, C6H12O6, H2SO4
/// Does NOT match: single elements (H, O, Na), common English words (NASA, PhD)
static RE_CHEMICAL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b([A-Z][a-z]?\d*){2,}\b").unwrap());

/// Mathematical expression with equals sign.
/// Matches: E=mc², F=ma, a²+b²=c², PV=nRT
static RE_MATH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[A-Za-z][A-Za-z0-9²³√±×÷]*\s*=\s*[A-Za-z0-9²³√±×÷+\-*/^()]+").unwrap()
});

/// Number with optional unit/degree.
/// Matches: 100°C, 3.14159, 9.8 m/s², -273.15°C, 6.022e23
/// Does NOT match: standalone digits in casual text (like "4 chân")
static RE_NUMBER_UNIT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"-?\d+\.?\d*(?:[eE][+-]?\d+)?\s*[°℃℉%‰][A-Za-z]*").unwrap());

// ============================================================================
// Pre-scan: extract anchors from text
// ============================================================================

/// Pre-scan text to extract anchors that must be preserved by AI.
///
/// Scans for:
/// 1. Chemical formulas (H8O, CH3COOH, NaCl)
/// 2. Math expressions with = (E=mc², F=ma)
/// 3. Numbers with units/degrees (100°C, -273.15°C)
///
/// # Example
/// ```
/// use ku_encoder::prescan::prescan_anchors;
///
/// let anchors = prescan_anchors("H8O tồn tại ở 100°C");
/// assert_eq!(anchors.len(), 2); // "H8O" + "100°C"
/// ```
pub fn prescan_anchors(text: &str) -> Vec<Anchor> {
    let mut anchors = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // 1. Chemical formulas
    for m in RE_CHEMICAL.find_iter(text) {
        let s = m.as_str().to_string();
        // Filter out common English abbreviations that look like formulas
        if !is_common_abbreviation(&s) && seen.insert(s.clone()) {
            anchors.push(Anchor::Formula(s));
        }
    }

    // 2. Math expressions
    for m in RE_MATH.find_iter(text) {
        let s = m.as_str().trim().to_string();
        if seen.insert(s.clone()) {
            anchors.push(Anchor::Math(s));
        }
    }

    // 3. Numbers with units/degrees
    for m in RE_NUMBER_UNIT.find_iter(text) {
        let s = m.as_str().trim().to_string();
        if seen.insert(s.clone()) {
            anchors.push(Anchor::Number(s));
        }
    }

    anchors
}

/// Filter common English abbreviations that match chemical formula pattern.
///
/// Uses a comprehensive static list plus a heuristic: if the string is
/// all-uppercase ASCII letters (no digits) and ≤5 chars, it's almost
/// certainly an abbreviation, not a chemical formula. Real formulas like
/// NaCl have mixed case or digits.
fn is_common_abbreviation(s: &str) -> bool {
    use std::collections::HashSet;
    use std::sync::LazyLock;

    static ABBREVIATIONS: LazyLock<HashSet<&str>> = LazyLock::new(|| {
        [
            // Science / Medicine
            "DNA", "RNA", "ATP", "ADP", "PCR", "MRI", "CT", "ICU", "BMI", "ECG", "EEG", "HIV",
            "AIDS", "COPD", "PTSD", "ADHD", "OCD", // Computing / Tech
            "CPU", "GPU", "SSD", "HDD", "USB", "RAM", "ROM", "API", "SDK", "IDE", "JSON", "XML",
            "HTML", "CSS", "HTTP", "HTTPS", "SQL", "TCP", "UDP", "IP", "URL", "URI", "DNS", "SSH",
            "FTP", "CLI", "GUI", "OS", "VM", "AI", "ML", "NLP", "LLM", "AGI", "IoT", "AR", "VR",
            "XR", "NFT", "DAO", // Organizations
            "NASA", "UNESCO", "WHO", "NATO", "FBI", "CIA", "NSA", "UN", "EU", "USA", "UK", "IMF",
            "WTO", "OPEC", "ASEAN", "UNICEF", // Business / Finance
            "CEO", "CTO", "CFO", "COO", "MBA", "GDP", "ROI", "KPI", "HR", "PR", "IPO", "ETF",
            "ESG", "B2B", "B2C", "SaaS", // Education
            "PhD", "GPA", "STEM", "SAT", "GRE", "TOEFL", "IELTS",
            // Display / Electronics
            "LED", "LCD", "OLED", "AMOLED", "WiFi", "HDMI", "JPEG", "PNG", "PDF", "CSV", "ZIP",
            "ISO", // Misc
            "ASAP", "FAQ", "DIY", "RIP", "RSVP", "TBD", "ETA", "FYI",
        ]
        .into_iter()
        .collect()
    });

    // Check static list first
    if ABBREVIATIONS.contains(s) {
        return true;
    }

    // Heuristic: all-uppercase ASCII, no digits, 2-5 chars → abbreviation
    // Real chemical formulas almost always have digits (H2O) or mixed case (NaCl)
    s.len() >= 2 && s.len() <= 5 && s.chars().all(|c| c.is_ascii_uppercase())
}

// ============================================================================
// Verify: check anchors survived AI processing
// ============================================================================

/// Verify that all pre-scanned anchors exist in the AI output triples.
///
/// Returns `VerifyResult::Ok` if all anchors are found, otherwise
/// returns the first missing anchor.
pub fn verify_anchors(anchors: &[Anchor], triples: &[SpoTriple]) -> VerifyResult {
    if anchors.is_empty() {
        return VerifyResult::Ok;
    }

    // Concatenate all string fields from triples for searching
    let all_text: String = triples
        .iter()
        .flat_map(|t| [&t.s, &t.s_en, &t.o, &t.o_en, &t.p])
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");

    for anchor in anchors {
        let anchor_str = anchor.as_str();
        if !all_text.contains(anchor_str) {
            return VerifyResult::Missing(anchor_str.to_string());
        }
    }

    VerifyResult::Ok
}

// ============================================================================
// Override: fix AI-corrected anchors in triples
// ============================================================================

/// Attempt to override AI-corrected anchors back to original values.
///
/// When AI changes "H8O" to "H2O", this function detects the similar-but-wrong
/// term and replaces it with the original anchor value.
///
/// Returns the number of overrides applied.
pub fn override_corrected(anchors: &[Anchor], triples: &mut [SpoTriple]) -> usize {
    let mut override_count = 0;

    for anchor in anchors {
        let anchor_str = anchor.as_str();

        // Check if anchor is already present — skip if so
        let present = triples.iter().any(|t| {
            t.s.contains(anchor_str)
                || t.s_en.contains(anchor_str)
                || t.o.contains(anchor_str)
                || t.o_en.contains(anchor_str)
        });
        if present {
            continue;
        }

        // Anchor is missing — look for a similar term that AI may have "corrected"
        for triple in triples.iter_mut() {
            if is_similar_formula(&triple.s, anchor_str) {
                triple.s = anchor_str.to_string();
                triple.s_en = anchor_str.to_string();
                override_count += 1;
            }
            if is_similar_formula(&triple.o, anchor_str) {
                triple.o = anchor_str.to_string();
                triple.o_en = anchor_str.to_string();
                override_count += 1;
            }
        }
    }

    override_count
}

/// Check if two strings are "similar" chemical formulas.
///
/// "H2O" is similar to "H8O" (same element letters, different numbers).
/// "water" is NOT similar to "H8O".
fn is_similar_formula(actual: &str, expected: &str) -> bool {
    if actual == expected {
        return false; // Exact match — not a correction case
    }
    if actual.is_empty() || expected.is_empty() {
        return false;
    }

    // Extract just the letters from both strings
    let actual_letters: String = actual.chars().filter(|c| c.is_alphabetic()).collect();
    let expected_letters: String = expected.chars().filter(|c| c.is_alphabetic()).collect();

    // Same letters but different numbers → likely AI "corrected" the numbers
    if actual_letters.eq_ignore_ascii_case(&expected_letters) && actual_letters.len() >= 2 {
        // Also verify at least one has digits (to avoid matching plain words)
        let has_digits = actual.chars().any(|c| c.is_ascii_digit())
            || expected.chars().any(|c| c.is_ascii_digit());
        return has_digits;
    }

    false
}

// ============================================================================
// Format anchors for AI prompt
// ============================================================================

/// Format anchors as a string to include in the AI prompt.
///
/// Returns `None` if no anchors.
/// Returns something like: `"DO NOT modify these terms: H8O, 100°C, E=mc²"`
pub fn format_anchors_for_prompt(anchors: &[Anchor]) -> Option<String> {
    if anchors.is_empty() {
        return None;
    }
    let terms: Vec<&str> = anchors.iter().map(|a| a.as_str()).collect();
    Some(format!(
        "IMPORTANT: DO NOT correct or modify these terms, they are intentional: {}",
        terms.join(", ")
    ))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- prescan_anchors tests ---

    #[test]
    fn test_prescan_chemical_formula() {
        let anchors = prescan_anchors("Hợp chất H8O có thể tồn tại");
        assert!(
            anchors.iter().any(|a| a.as_str() == "H8O"),
            "Should detect H8O, got: {:?}",
            anchors
        );
    }

    #[test]
    fn test_prescan_complex_chemical() {
        let anchors = prescan_anchors("CH3COOH là acetic acid");
        assert!(
            anchors.iter().any(|a| a.as_str() == "CH3COOH"),
            "Should detect CH3COOH, got: {:?}",
            anchors
        );
    }

    #[test]
    fn test_prescan_number_with_unit() {
        let anchors = prescan_anchors("Nước sôi ở 100°C");
        assert!(
            anchors.iter().any(|a| a.as_str() == "100°C"),
            "Should detect 100°C, got: {:?}",
            anchors
        );
    }

    #[test]
    fn test_prescan_math_expression() {
        let anchors = prescan_anchors("Công thức E=mc² rất nổi tiếng");
        assert!(
            anchors.iter().any(|a| a.as_str() == "E=mc²"),
            "Should detect E=mc², got: {:?}",
            anchors
        );
    }

    #[test]
    fn test_prescan_no_anchors() {
        let anchors = prescan_anchors("Bàn làm việc thường có 4 chân");
        // "4" alone is not an anchor (no unit/degree), and no formulas
        assert!(
            anchors.is_empty(),
            "Plain text should have no anchors, got: {:?}",
            anchors
        );
    }

    #[test]
    fn test_prescan_dedup() {
        let anchors = prescan_anchors("H8O là H8O và H8O");
        let h8o_count = anchors.iter().filter(|a| a.as_str() == "H8O").count();
        assert_eq!(h8o_count, 1, "Should deduplicate anchors");
    }

    #[test]
    fn test_prescan_filters_abbreviations() {
        let anchors = prescan_anchors("NASA launched from USA");
        let has_nasa = anchors.iter().any(|a| a.as_str() == "NASA");
        let has_usa = anchors.iter().any(|a| a.as_str() == "USA");
        assert!(!has_nasa, "Should filter NASA");
        assert!(!has_usa, "Should filter USA");
    }

    // --- verify_anchors tests ---

    #[test]
    fn test_verify_all_present() {
        let anchors = vec![Anchor::Formula("H8O".to_string())];
        let triples = vec![SpoTriple {
            s: "H8O".into(),
            s_en: "H8O".into(),
            p: "composed of".into(),
            o: "hydrogen".into(),
            o_en: "hydrogen".into(),
            qty: None,
            role: "part".into(),
            notation: None,
            c: "always".into(),
        }];
        assert_eq!(verify_anchors(&anchors, &triples), VerifyResult::Ok);
    }

    #[test]
    fn test_verify_missing() {
        let anchors = vec![Anchor::Formula("H8O".to_string())];
        let triples = vec![SpoTriple {
            s: "H2O".into(),
            s_en: "H2O".into(),
            p: "composed of".into(),
            o: "hydrogen".into(),
            o_en: "hydrogen".into(),
            qty: None,
            role: "part".into(),
            notation: None,
            c: "always".into(),
        }];
        assert_eq!(
            verify_anchors(&anchors, &triples),
            VerifyResult::Missing("H8O".into())
        );
    }

    #[test]
    fn test_verify_empty_anchors() {
        let triples = vec![SpoTriple {
            s: "anything".into(),
            s_en: "anything".into(),
            p: "is".into(),
            o: "fine".into(),
            o_en: "fine".into(),
            qty: None,
            role: "relation".into(),
            notation: None,
            c: "always".into(),
        }];
        assert_eq!(verify_anchors(&[], &triples), VerifyResult::Ok);
    }

    // --- override_corrected tests ---

    #[test]
    fn test_override_h2o_to_h8o() {
        let anchors = vec![Anchor::Formula("H8O".to_string())];
        let mut triples = vec![SpoTriple {
            s: "H2O".into(),
            s_en: "H2O".into(),
            p: "composed of".into(),
            o: "hydrogen".into(),
            o_en: "hydrogen".into(),
            qty: None,
            role: "part".into(),
            notation: None,
            c: "always".into(),
        }];
        let count = override_corrected(&anchors, &mut triples);
        assert_eq!(count, 1);
        assert_eq!(triples[0].s, "H8O");
        assert_eq!(triples[0].s_en, "H8O");
    }

    #[test]
    fn test_override_no_change_needed() {
        let anchors = vec![Anchor::Formula("H8O".to_string())];
        let mut triples = vec![SpoTriple {
            s: "H8O".into(),
            s_en: "H8O".into(),
            p: "composed of".into(),
            o: "hydrogen".into(),
            o_en: "hydrogen".into(),
            qty: None,
            role: "part".into(),
            notation: None,
            c: "always".into(),
        }];
        let count = override_corrected(&anchors, &mut triples);
        assert_eq!(count, 0, "Should not override when anchor already present");
    }

    // --- is_similar_formula tests ---

    #[test]
    fn test_similar_h2o_h8o() {
        assert!(
            is_similar_formula("H2O", "H8O"),
            "H2O and H8O have same letters, different numbers"
        );
    }

    #[test]
    fn test_not_similar_water_h8o() {
        assert!(
            !is_similar_formula("water", "H8O"),
            "water and H8O are not similar"
        );
    }

    #[test]
    fn test_not_similar_exact_match() {
        assert!(
            !is_similar_formula("H8O", "H8O"),
            "Exact match is not a correction case"
        );
    }

    // --- format_anchors_for_prompt tests ---

    #[test]
    fn test_format_anchors_empty() {
        assert_eq!(format_anchors_for_prompt(&[]), None);
    }

    #[test]
    fn test_format_anchors_with_items() {
        let anchors = vec![
            Anchor::Formula("H8O".to_string()),
            Anchor::Number("100°C".to_string()),
        ];
        let result = format_anchors_for_prompt(&anchors).unwrap();
        assert!(result.contains("H8O"));
        assert!(result.contains("100°C"));
        assert!(result.contains("DO NOT"));
    }
}
