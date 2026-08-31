//! # Tier 1 Rule-Based Text Parser
//!
//! Converts Vietnamese and English natural-language text into [`CoreDna`]
//! instructions using **pure pattern matching** — no AI model required.
//!
//! ## Accuracy Target
//! ~60-70% (will be refined by the P2P network in later tiers).
//!
//! ## Supported Patterns
//! | Pattern (VI)           | Pattern (EN)              | Instruction       |
//! |------------------------|---------------------------|--------------------|
//! | "X là Y"               | "X is Y"                  | Triple(X, IS_A, Y)|
//! | "X gồm A, B"           | "X consists of A, B"      | PartOf(A, X), …   |
//! | "Bước N: action target" | "Step N: action target"   | Step(N, act, tgt)  |
//! | "= 35.2°"              | "= 35.2°"                 | Quantity(F32)      |
//! | "± 0.1"                | "± 0.1"                   | Tolerance          |
//! | bare numbers           | bare numbers              | Quantity           |

use std::collections::HashMap;

use crate::core_dna::{CoreDna, CoreDnaHeader, Instruction, NumericValue};
use crate::error::KuError;
use crate::types::ConceptId;

// ============================================================================
// Well-known ConceptIds (Tier-0 universal primitives)
// ============================================================================

/// Predicate: "is a" / "là" relationship
pub const IS_A: ConceptId = 1;
/// Predicate: "has part" / "gồm" relationship
pub const HAS_PART: ConceptId = 2;
/// Predicate: generic relation (fallback)
pub const RELATED_TO: ConceptId = 3;
/// Unit: degree (°)
pub const UNIT_DEGREE: ConceptId = 10;
/// Unit: meter (m)
pub const UNIT_METER: ConceptId = 11;
/// Unit: second (s)
pub const UNIT_SECOND: ConceptId = 12;
/// Unit: kilogram (kg)
pub const UNIT_KILOGRAM: ConceptId = 13;
/// Unit: percent (%)
pub const UNIT_PERCENT: ConceptId = 14;
/// Unit: centimeter (cm)
pub const UNIT_CENTIMETER: ConceptId = 15;
/// Unit: kilometer (km)
pub const UNIT_KILOMETER: ConceptId = 16;
/// Unit: millisecond (ms)
pub const UNIT_MILLISECOND: ConceptId = 17;
/// Unit: minute (min)
pub const UNIT_MINUTE: ConceptId = 18;
/// Unit: hour (h)
pub const UNIT_HOUR: ConceptId = 19;
/// Unit: dimensionless quantity
pub const UNIT_DIMENSIONLESS: ConceptId = 20;
/// Unknown/fallback concept
pub const UNKNOWN_CONCEPT: ConceptId = 127;

// ============================================================================
// ConceptDict — word → ConceptId mapping
// ============================================================================

/// A dictionary mapping word stems (lowercase) to ConceptIds.
///
/// This is the T1 "vocabulary" — a simple HashMap used to resolve
/// natural language tokens into language-agnostic concept identifiers.
///
/// **Note:** This is the lightweight, encoding-only variant used by
/// the text parser pipeline. For the full bidirectional dictionary with
/// multilingual aliases and tier-based ID allocation, see
/// [`concept_dict::ConceptDict`](crate::concept_dict::ConceptDict).
#[derive(Debug, Clone)]
pub struct ConceptDict {
    map: HashMap<String, ConceptId>,
    next_id: ConceptId,
}

impl ConceptDict {
    /// Create an empty dictionary.
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            next_id: 1000,
        }
    }

    /// Insert a word→concept mapping.
    pub fn insert(&mut self, word: &str, id: ConceptId) {
        self.map.insert(word.to_lowercase(), id);
    }

    /// Look up a word, returning its ConceptId or `UNKNOWN_CONCEPT`.
    pub fn lookup(&self, word: &str) -> ConceptId {
        let key = word.to_lowercase();
        self.map.get(&key).copied().unwrap_or(UNKNOWN_CONCEPT)
    }

    /// Look up a word; if missing, auto-assign a new ConceptId.
    pub fn lookup_or_create(&mut self, word: &str) -> ConceptId {
        let key = word.to_lowercase();
        if let Some(&id) = self.map.get(&key) {
            id
        } else {
            let id = self.next_id;
            self.next_id += 1;
            self.map.insert(key, id);
            id
        }
    }

    /// Number of entries in the dictionary.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether the dictionary is empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Iterate over all (word, ConceptId) entries.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &ConceptId)> {
        self.map.iter()
    }
}

impl Default for ConceptDict {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// default_dict — demo dictionary (~100 concepts)
// ============================================================================

/// Returns a small demo dictionary with ~100 common concepts for both
/// Vietnamese and English, covering sports, science, and everyday terms.
pub fn default_dict() -> ConceptDict {
    let mut d = ConceptDict::new();

    // ---- Structural / meta predicates ----
    d.insert("is_a", IS_A);
    d.insert("has_part", HAS_PART);
    d.insert("related_to", RELATED_TO);

    // ---- Units ----
    d.insert("degree", UNIT_DEGREE);
    d.insert("°", UNIT_DEGREE);
    d.insert("meter", UNIT_METER);
    d.insert("m", UNIT_METER);
    d.insert("second", UNIT_SECOND);
    d.insert("s", UNIT_SECOND);
    d.insert("kilogram", UNIT_KILOGRAM);
    d.insert("kg", UNIT_KILOGRAM);
    d.insert("percent", UNIT_PERCENT);
    d.insert("%", UNIT_PERCENT);
    d.insert("cm", UNIT_CENTIMETER);
    d.insert("centimeter", UNIT_CENTIMETER);
    d.insert("km", UNIT_KILOMETER);
    d.insert("kilometer", UNIT_KILOMETER);
    d.insert("ms", UNIT_MILLISECOND);
    d.insert("millisecond", UNIT_MILLISECOND);
    d.insert("min", UNIT_MINUTE);
    d.insert("minute", UNIT_MINUTE);
    d.insert("h", UNIT_HOUR);
    d.insert("hour", UNIT_HOUR);

    // ---- Sports / Swimming (Vietnamese + English) ----
    let sports_words = [
        // Vietnamese
        "bơi",
        "ếch",
        "bơi ếch",
        "kỹ thuật",
        "tay",
        "chân",
        "đạp",
        "quạt",
        "hít",
        "thở",
        "thở ra",
        "nước",
        "mặt nước",
        "đầu",
        "cơ thể",
        "nhịp",
        "phối hợp",
        "lực",
        "tốc độ",
        "khoảng cách",
        "góc",
        "vai",
        "hông",
        "khuỷu tay",
        "lòng bàn tay",
        "ngón chân",
        "bước",
        "giai đoạn",
        "chuẩn bị",
        "thực hiện",
        "hoàn thành",
        "bắt đầu",
        "kết thúc",
        "lặp lại",
        "duỗi",
        "co",
        "gập",
        "xoay",
        "đẩy",
        "kéo",
        "trượt",
        "nổi",
        // English
        "swimming",
        "breaststroke",
        "technique",
        "arm",
        "leg",
        "kick",
        "stroke",
        "breathe",
        "inhale",
        "exhale",
        "water",
        "surface",
        "head",
        "body",
        "rhythm",
        "coordination",
        "force",
        "speed",
        "distance",
        "angle",
        "shoulder",
        "hip",
        "elbow",
        "palm",
        "toe",
        "step",
        "phase",
        "preparation",
        "execution",
        "completion",
        "start",
        "end",
        "repeat",
        "extend",
        "contract",
        "bend",
        "rotate",
        "push",
        "pull",
        "glide",
        "float",
        // General knowledge
        "temperature",
        "nhiệt độ",
        "pressure",
        "áp suất",
        "weight",
        "trọng lượng",
        "height",
        "chiều cao",
        "width",
        "chiều rộng",
        "length",
        "chiều dài",
        "time",
        "thời gian",
        "frequency",
        "tần số",
        "energy",
        "năng lượng",
        "power",
        "công suất",
        "velocity",
        "vận tốc",
        "acceleration",
        "gia tốc",
    ];
    for (id, w) in (200_u64..).zip(sports_words.iter()) {
        d.insert(w, id);
    }

    d
}

// ============================================================================
// Unit detection helper
// ============================================================================

/// Try to detect a unit suffix at the end of a token. Returns (number_str, unit_concept_id).
fn detect_unit(token: &str) -> Option<(&str, ConceptId)> {
    // Order matters — try longest suffixes first
    let suffixes: &[(&str, ConceptId)] = &[
        ("°c", UNIT_DEGREE),
        ("°f", UNIT_DEGREE),
        ("°", UNIT_DEGREE),
        ("cm", UNIT_CENTIMETER),
        ("km", UNIT_KILOMETER),
        ("kg", UNIT_KILOGRAM),
        ("ms", UNIT_MILLISECOND),
        ("min", UNIT_MINUTE),
        ("%", UNIT_PERCENT),
        ("m", UNIT_METER),
        ("s", UNIT_SECOND),
        ("h", UNIT_HOUR),
    ];

    let lower = token.to_lowercase();
    for &(suffix, unit_id) in suffixes {
        if lower.ends_with(suffix) {
            let num_part = &token[..token.len() - suffix.len()];
            // Check there's actually a number before the suffix
            if !num_part.is_empty()
                && num_part
                    .bytes()
                    .all(|b| b.is_ascii_digit() || b == b'.' || b == b'-')
            {
                return Some((num_part, unit_id));
            }
        }
    }
    None
}

/// Try to parse a string as a numeric value.
fn parse_numeric(s: &str) -> Option<NumericValue> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // Try integer first
    if let Ok(v) = s.parse::<i64>() {
        return Some(if (0..=255).contains(&v) {
            NumericValue::U8(v as u8)
        } else if (0..=65535).contains(&v) {
            NumericValue::U16(v as u16)
        } else if v >= i16::MIN as i64 && v < 0 {
            NumericValue::I16(v as i16)
        } else if v >= 0 && v <= u32::MAX as i64 {
            NumericValue::U32(v as u32)
        } else {
            NumericValue::I32(v as i32)
        });
    }

    // Try float
    if let Ok(v) = s.parse::<f32>() {
        return Some(NumericValue::F32(v));
    }

    None
}

// ============================================================================
// Core parser — line-level pattern matching
// ============================================================================

/// Main entry point: parse natural-language text into a `CoreDna` struct.
///
/// The parser applies rules line-by-line:
/// 1. "Bước N:" / "Step N:" → `Step` instruction
/// 2. "X là Y" / "X is Y" → `Triple(X, IS_A, Y)`
/// 3. "X gồm A, B, C" / "X consists of A, B, C" → `PartOf(A, X), PartOf(B, X), …`
/// 4. "= <number><unit>" → `Quantity`
/// 5. "± <number>" → `Tolerance`
/// 6. Bare numeric tokens → `Quantity`
pub fn parse_text_to_core_dna(text: &str, dict: &ConceptDict) -> Result<CoreDna, KuError> {
    if text.trim().is_empty() {
        return Err(KuError::InvalidData("Empty input text".into()));
    }

    let mut instructions: Vec<Instruction> = Vec::new();
    let mut mutable_dict = dict.clone();
    let lower_full = text.to_lowercase();

    // Determine gene type from content
    let gene_type = detect_gene_type(&lower_full);

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Try each pattern in priority order
        if let Some(instr) = try_parse_step(trimmed, &mut mutable_dict) {
            instructions.push(instr);
            continue;
        }

        if let Some(instrs) = try_parse_consists_of(trimmed, &mut mutable_dict) {
            instructions.extend(instrs);
            continue;
        }

        if let Some(instr) = try_parse_is_a(trimmed, &mut mutable_dict) {
            instructions.push(instr);
            continue;
        }

        // Scan for inline numerics (= value, ± delta, bare numbers)
        let inline = parse_inline_numerics(trimmed, &mut mutable_dict);
        if !inline.is_empty() {
            instructions.extend(inline);
            continue;
        }

        // Fallback: try to extract any recognizable tokens as a quality/label
        if let Some(instr) = try_parse_fallback(trimmed, &mut mutable_dict) {
            instructions.push(instr);
        }
    }

    if instructions.is_empty() {
        return Err(KuError::InvalidData(
            "No recognizable patterns found in input text".into(),
        ));
    }

    Ok(CoreDna {
        header: CoreDnaHeader {
            version: 2,
            gene_type,
            has_concept_table: false,
        },
        concept_table: Vec::new(),
        instructions,
    })
}

/// Detect gene type: Procedure if text mentions steps, otherwise Fact.
fn detect_gene_type(lower_text: &str) -> u8 {
    // Procedure(1) if text contains step-like keywords
    if lower_text.contains("bước")
        || lower_text.contains("step")
        || lower_text.contains("giai đoạn")
        || lower_text.contains("phase")
    {
        return 1; // Procedure
    }
    // Fact(0) for everything else (including numeric content)
    0
}

// ============================================================================
// Pattern: "Bước N:" / "Step N:" → Step instruction
// ============================================================================

fn try_parse_step(line: &str, dict: &mut ConceptDict) -> Option<Instruction> {
    let lower = line.to_lowercase();

    // Vietnamese: "Bước N:" or "Bước N."
    // English:    "Step N:" or "Step N."
    let (prefix, rest) = if lower.starts_with("bước ") || lower.starts_with("bước\u{a0}") {
        ("bước", &line[skip_prefix_bytes(line, "bước")?..])
    } else if lower.starts_with("step ") {
        ("step", &line[4..])
    } else {
        return None;
    };
    let _ = prefix; // suppress warning

    let rest = rest.trim_start();

    // Extract step number
    let (num_str, after_num) = split_first_token(rest);
    let num_str = num_str.trim_end_matches([':', '.', ')']);
    let ord: u8 = num_str.parse().unwrap_or(1);

    // The rest is "action target"
    let after_num = after_num.trim_start_matches([':', '.', ')', ' ']);

    let words: Vec<&str> = after_num.split_whitespace().collect();
    let (action_text, target_text) = if words.len() >= 2 {
        // First word = action, rest = target
        (words[0].to_string(), words[1..].join(" "))
    } else if words.len() == 1 {
        (words[0].to_string(), String::new())
    } else {
        return None;
    };

    let action = dict.lookup_or_create(&action_text);
    let target = if target_text.is_empty() {
        UNKNOWN_CONCEPT
    } else {
        dict.lookup_or_create(&target_text)
    };

    Some(Instruction::Step {
        ord,
        action,
        target,
    })
}

/// Helper: skip Unicode prefix and return byte offset after the prefix word.
fn skip_prefix_bytes(line: &str, prefix: &str) -> Option<usize> {
    // Find first char-boundary after the prefix
    let lower = line.to_lowercase();
    if lower.starts_with(prefix) {
        // Count bytes in original string that correspond to prefix characters
        let prefix_chars = prefix.chars().count();
        let byte_offset: usize = line.chars().take(prefix_chars).map(|c| c.len_utf8()).sum();
        Some(byte_offset)
    } else {
        None
    }
}

// ============================================================================
// Pattern: "X là Y" / "X is Y" → Triple(X, IS_A, Y)
// ============================================================================

fn try_parse_is_a(line: &str, dict: &mut ConceptDict) -> Option<Instruction> {
    let lower = line.to_lowercase();

    // Try Vietnamese "là"
    if let Some(pos) = find_word_boundary(&lower, " là ") {
        let subject_text = line[..pos].trim();
        let object_text = line[pos + find_word_boundary_len(&lower, " là ")..].trim();
        if !subject_text.is_empty() && !object_text.is_empty() {
            let s = dict.lookup_or_create(subject_text);
            let o = dict.lookup_or_create(object_text);
            return Some(Instruction::Triple { s, p: IS_A, o });
        }
    }

    // Try English "is a", "is an", "is the", or bare "is"
    for pattern in &[" is a ", " is an ", " is the ", " is "] {
        if let Some(pos) = find_word_boundary(&lower, pattern) {
            let subject_text = line[..pos].trim();
            let pat_byte_len = pattern.len();
            let object_text = line[pos + pat_byte_len..].trim();
            // Remove trailing period
            let object_text = object_text.trim_end_matches('.');
            if !subject_text.is_empty() && !object_text.is_empty() {
                let s = dict.lookup_or_create(subject_text);
                let o = dict.lookup_or_create(object_text);
                return Some(Instruction::Triple { s, p: IS_A, o });
            }
        }
    }

    None
}

fn find_word_boundary(haystack: &str, needle: &str) -> Option<usize> {
    haystack.find(needle)
}

fn find_word_boundary_len(haystack: &str, needle: &str) -> usize {
    let _ = haystack;
    needle.len()
}

// ============================================================================
// Pattern: "X gồm A, B, C" / "X consists of A, B, C" → PartOf
// ============================================================================

fn try_parse_consists_of(line: &str, dict: &mut ConceptDict) -> Option<Vec<Instruction>> {
    let lower = line.to_lowercase();

    // Vietnamese: "X gồm A, B, C" or "X bao gồm A, B, C"
    let (whole_text, parts_text) = if let Some(pos) = find_word_boundary(&lower, " gồm ") {
        let w = line[..pos].trim();
        let p = line[pos + " gồm ".len()..].trim();
        (w, p)
    } else if let Some(pos) = find_word_boundary(&lower, " bao gồm ") {
        let w = line[..pos].trim();
        let p = line[pos + " bao gồm ".len()..].trim();
        (w, p)
    }
    // English: "X consists of A, B, C" / "X includes A, B, C"
    else if let Some(pos) = find_word_boundary(&lower, " consists of ") {
        let w = line[..pos].trim();
        let p = line[pos + " consists of ".len()..].trim();
        (w, p)
    } else if let Some(pos) = find_word_boundary(&lower, " includes ") {
        let w = line[..pos].trim();
        let p = line[pos + " includes ".len()..].trim();
        (w, p)
    } else if let Some(pos) = find_word_boundary(&lower, " contains ") {
        let w = line[..pos].trim();
        let p = line[pos + " contains ".len()..].trim();
        (w, p)
    } else {
        return None;
    };

    if whole_text.is_empty() || parts_text.is_empty() {
        return None;
    }

    let whole_id = dict.lookup_or_create(whole_text);

    // Split parts by comma, "và", "and"
    let parts_text = parts_text.trim_end_matches('.');
    let parts: Vec<&str> = parts_text
        .split([',', ';'])
        .flat_map(|s| {
            // Further split by "và" / "and"
            let s = s.trim();
            let lower_s = s.to_lowercase();
            if lower_s.contains(" và ") {
                s.splitn(2, |_c: char| {
                    false // placeholder — handled below
                })
                .collect::<Vec<_>>()
            } else {
                vec![s]
            }
        })
        .collect();

    // Re-split handling "và" and "and"
    let mut final_parts: Vec<String> = Vec::new();
    for part in &parts {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        let lower_p = p.to_lowercase();
        if lower_p.contains(" và ") {
            for sub in p.split(" và ").chain(p.split(" Và ")) {
                let sub = sub.trim();
                if !sub.is_empty() && !final_parts.iter().any(|x| x.eq_ignore_ascii_case(sub)) {
                    final_parts.push(sub.to_string());
                }
            }
        } else if lower_p.contains(" and ") {
            for sub in p.split(" and ").chain(p.split(" And ")) {
                let sub = sub.trim();
                if !sub.is_empty() && !final_parts.iter().any(|x| x.eq_ignore_ascii_case(sub)) {
                    final_parts.push(sub.to_string());
                }
            }
        } else {
            final_parts.push(p.to_string());
        }
    }

    if final_parts.is_empty() {
        return None;
    }

    let mut instrs = Vec::new();
    for part_name in &final_parts {
        let part_id = dict.lookup_or_create(part_name);
        instrs.push(Instruction::PartOf {
            part: part_id,
            whole: whole_id,
        });
    }

    Some(instrs)
}

// ============================================================================
// Pattern: inline numerics ("= 35.2°", "± 0.1", bare numbers)
// ============================================================================

fn parse_inline_numerics(line: &str, dict: &mut ConceptDict) -> Vec<Instruction> {
    let mut instrs = Vec::new();
    let tokens: Vec<&str> = line.split_whitespace().collect();

    let mut i = 0;
    while i < tokens.len() {
        let tok = tokens[i];

        // "= <number><unit>" pattern
        if tok == "=" && i + 1 < tokens.len() {
            let next = tokens[i + 1];
            if let Some(instr) = parse_quantity_token(next, dict) {
                instrs.push(instr);
                i += 2;
                continue;
            }
        }

        // "± <number>" pattern (tolerance)
        if (tok == "±" || tok == "+-") && i + 1 < tokens.len() {
            let next = tokens[i + 1];
            // Clean unit suffix from delta
            let (num_str, _unit) = if let Some((n, u)) = detect_unit(next) {
                (n, u)
            } else {
                (next, UNIT_DIMENSIONLESS)
            };

            if let Some(delta) = parse_numeric(num_str) {
                // Create a tolerance instruction; subject is a placeholder
                instrs.push(Instruction::Tolerance {
                    s: UNKNOWN_CONCEPT,
                    value: NumericValue::F32(0.0), // placeholder — the preceding Quantity should be the value
                    delta,
                });
                i += 2;
                continue;
            }
        }

        // Token with unit suffix: "35.2°", "100m", "5kg"
        if let Some(instr) = parse_quantity_token(tok, dict) {
            instrs.push(instr);
            i += 1;
            continue;
        }

        // Bare number (only if it starts with a digit or minus)
        if tok.starts_with(|c: char| c.is_ascii_digit() || c == '-') {
            if let Some(val) = parse_numeric(tok) {
                instrs.push(Instruction::Quantity {
                    s: UNKNOWN_CONCEPT,
                    value: val,
                    unit: UNIT_DIMENSIONLESS,
                });
                i += 1;
                continue;
            }
        }

        i += 1;
    }

    instrs
}

/// Parse a single token as a Quantity (number + optional unit).
fn parse_quantity_token(token: &str, _dict: &mut ConceptDict) -> Option<Instruction> {
    if let Some((num_str, unit_id)) = detect_unit(token) {
        if let Some(val) = parse_numeric(num_str) {
            return Some(Instruction::Quantity {
                s: UNKNOWN_CONCEPT,
                value: val,
                unit: unit_id,
            });
        }
    }
    None
}

// ============================================================================
// Fallback: extract meaningful tokens as a Quality
// ============================================================================

fn try_parse_fallback(line: &str, dict: &mut ConceptDict) -> Option<Instruction> {
    // Try to find at least two known words → Quality(subject, quality)
    let words: Vec<&str> = line
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty() && w.len() > 1)
        .collect();

    if words.len() >= 2 {
        let s = dict.lookup_or_create(words[0]);
        let q = dict.lookup_or_create(words[1]);
        Some(Instruction::Quality { s, q })
    } else if words.len() == 1 {
        let s = dict.lookup_or_create(words[0]);
        Some(Instruction::Quality {
            s,
            q: UNKNOWN_CONCEPT,
        })
    } else {
        None
    }
}

// ============================================================================
// Utility: split first whitespace-delimited token
// ============================================================================

fn split_first_token(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    match s.find(char::is_whitespace) {
        Some(pos) => (&s[..pos], &s[pos..]),
        None => (s, ""),
    }
}

// ============================================================================
// Post-processing: link Tolerance to preceding Quantity

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_dna::Instruction;

    fn has_triple(instrs: &[Instruction], pred: ConceptId) -> bool {
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::Triple { p, .. } if *p == pred))
    }

    fn has_step(instrs: &[Instruction]) -> bool {
        instrs.iter().any(|i| matches!(i, Instruction::Step { .. }))
    }

    fn has_part_of(instrs: &[Instruction]) -> bool {
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::PartOf { .. }))
    }

    fn has_quantity(instrs: &[Instruction]) -> bool {
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::Quantity { .. }))
    }

    fn has_tolerance(instrs: &[Instruction]) -> bool {
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::Tolerance { .. }))
    }

    fn count_of<F>(instrs: &[Instruction], pred: F) -> usize
    where
        F: Fn(&Instruction) -> bool,
    {
        instrs.iter().filter(|i| pred(i)).count()
    }

    // ---- Basic Vietnamese patterns ----

    #[test]
    fn test_vietnamese_is_a() {
        let dict = default_dict();
        let text = "Bơi ếch là một kỹ thuật bơi lội";
        let dna = parse_text_to_core_dna(text, &dict).unwrap();
        println!("Vietnamese IS_A: {:?}", dna.instructions);
        assert!(has_triple(&dna.instructions, IS_A));
    }

    #[test]
    fn test_vietnamese_consists_of() {
        let dict = default_dict();
        let text = "Kỹ thuật bơi ếch gồm động tác tay, động tác chân, và nhịp thở";
        let dna = parse_text_to_core_dna(text, &dict).unwrap();
        println!("Vietnamese CONSISTS_OF: {:?}", dna.instructions);
        assert!(has_part_of(&dna.instructions));
        // Should have at least 3 PartOf instructions
        let part_count = count_of(&dna.instructions, |i| {
            matches!(i, Instruction::PartOf { .. })
        });
        println!("  PartOf count: {}", part_count);
        assert!(
            part_count >= 3,
            "Expected at least 3 PartOf, got {}",
            part_count
        );
    }

    #[test]
    fn test_vietnamese_step() {
        let dict = default_dict();
        let text = "Bước 1: Duỗi tay về phía trước\nBước 2: Đạp chân ra ngoài\nBước 3: Hít thở";
        let dna = parse_text_to_core_dna(text, &dict).unwrap();
        println!("Vietnamese STEP: {:?}", dna.instructions);
        assert!(has_step(&dna.instructions));
        let step_count = count_of(&dna.instructions, |i| matches!(i, Instruction::Step { .. }));
        assert_eq!(step_count, 3);
        // Verify step ordering
        let steps: Vec<u8> = dna
            .instructions
            .iter()
            .filter_map(|i| {
                if let Instruction::Step { ord, .. } = i {
                    Some(*ord)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(steps, vec![1, 2, 3]);
    }

    // ---- Basic English patterns ----

    #[test]
    fn test_english_is_a() {
        let dict = default_dict();
        let text = "Breaststroke is a swimming technique";
        let dna = parse_text_to_core_dna(text, &dict).unwrap();
        println!("English IS_A: {:?}", dna.instructions);
        assert!(has_triple(&dna.instructions, IS_A));
    }

    #[test]
    fn test_english_consists_of() {
        let dict = default_dict();
        let text = "Breaststroke consists of arm movement, leg kick, and breathing";
        let dna = parse_text_to_core_dna(text, &dict).unwrap();
        println!("English CONSISTS_OF: {:?}", dna.instructions);
        assert!(has_part_of(&dna.instructions));
        let part_count = count_of(&dna.instructions, |i| {
            matches!(i, Instruction::PartOf { .. })
        });
        assert!(
            part_count >= 3,
            "Expected at least 3 PartOf, got {}",
            part_count
        );
    }

    #[test]
    fn test_english_step() {
        let dict = default_dict();
        let text = "Step 1: Extend arms forward\nStep 2: Kick legs outward\nStep 3: Breathe in";
        let dna = parse_text_to_core_dna(text, &dict).unwrap();
        println!("English STEP: {:?}", dna.instructions);
        assert!(has_step(&dna.instructions));
        let step_count = count_of(&dna.instructions, |i| matches!(i, Instruction::Step { .. }));
        assert_eq!(step_count, 3);
    }

    // ---- Numeric patterns ----

    #[test]
    fn test_quantity_with_unit() {
        let dict = default_dict();
        let text = "Nhiệt độ nước = 35.2°";
        let dna = parse_text_to_core_dna(text, &dict).unwrap();
        println!("Quantity with unit: {:?}", dna.instructions);
        assert!(has_quantity(&dna.instructions));
        // Check the value is F32(35.2)
        let qty = dna
            .instructions
            .iter()
            .find(|i| matches!(i, Instruction::Quantity { .. }));
        if let Some(Instruction::Quantity { value, unit, .. }) = qty {
            assert_eq!(*unit, UNIT_DEGREE);
            let v = value.as_f64();
            assert!((v - 35.2).abs() < 0.01, "Expected ~35.2, got {}", v);
        } else {
            panic!("Expected Quantity instruction");
        }
    }

    #[test]
    fn test_tolerance() {
        let dict = default_dict();
        let text = "Góc = 45° ± 5°";
        let dna = parse_text_to_core_dna(text, &dict).unwrap();
        println!("Tolerance: {:?}", dna.instructions);
        assert!(has_quantity(&dna.instructions));
        assert!(has_tolerance(&dna.instructions));
    }

    #[test]
    fn test_bare_number() {
        let dict = default_dict();
        let text = "Lặp lại 10 lần";
        let dna = parse_text_to_core_dna(text, &dict).unwrap();
        println!("Bare number: {:?}", dna.instructions);
        // Should detect 10 as a quantity
        let has_num = dna.instructions.iter().any(|i| {
            if let Instruction::Quantity { value, .. } = i {
                value.as_f64() == 10.0
            } else {
                false
            }
        });
        assert!(has_num, "Expected to detect number 10");
    }

    // ---- Gene type detection ----

    #[test]
    fn test_gene_type_procedure() {
        let dict = default_dict();
        let text = "Bước 1: Chuẩn bị";
        let dna = parse_text_to_core_dna(text, &dict).unwrap();
        assert_eq!(dna.header.gene_type, 1, "Expected Procedure gene type");
    }

    #[test]
    fn test_gene_type_fact() {
        let dict = default_dict();
        let text = "Bơi ếch là một kỹ thuật";
        let dna = parse_text_to_core_dna(text, &dict).unwrap();
        assert_eq!(dna.header.gene_type, 0, "Expected Fact gene type");
    }

    // ---- Error handling ----

    #[test]
    fn test_empty_input() {
        let dict = default_dict();
        assert!(parse_text_to_core_dna("", &dict).is_err());
        assert!(parse_text_to_core_dna("   ", &dict).is_err());
    }

    // ---- Default dict ----

    #[test]
    fn test_default_dict_size() {
        let dict = default_dict();
        println!("Default dict size: {} entries", dict.len());
        assert!(
            dict.len() >= 80,
            "Expected at least 80 entries, got {}",
            dict.len()
        );
    }

    #[test]
    fn test_dict_lookup() {
        let dict = default_dict();
        assert_ne!(dict.lookup("bơi"), UNKNOWN_CONCEPT);
        assert_ne!(dict.lookup("swimming"), UNKNOWN_CONCEPT);
        assert_ne!(dict.lookup("°"), UNKNOWN_CONCEPT);
        assert_eq!(dict.lookup("nonexistent_word_xyz"), UNKNOWN_CONCEPT);
    }

    // ---- Integration: bơi ếch full text ----

    #[test]
    fn test_boi_ech_full_text() {
        let dict = default_dict();
        let text = r#"Bơi ếch là một kỹ thuật bơi cơ bản
Kỹ thuật bơi ếch gồm động tác tay, động tác chân, và nhịp thở
Bước 1: Duỗi hai tay về phía trước
Bước 2: Quạt tay ra hai bên
Bước 3: Đạp chân kiểu ếch
Bước 4: Hít thở khi đầu nhô lên mặt nước
Nhiệt độ nước lý tưởng = 28°
Góc gập khuỷu tay khoảng 90°"#;

        let dna = parse_text_to_core_dna(text, &dict).unwrap();
        println!("\n=== Bơi Ếch Full Parse ===");
        println!(
            "Header: version={}, gene_type={}, has_concept_table={}",
            dna.header.version, dna.header.gene_type, dna.header.has_concept_table
        );
        for (i, instr) in dna.instructions.iter().enumerate() {
            println!("  [{}] {:?}", i, instr);
        }
        println!("Total instructions: {}", dna.instructions.len());

        // Should be Procedure type (has "bước")
        assert_eq!(dna.header.gene_type, 1, "Expected Procedure");

        // Should have IS_A triple
        assert!(has_triple(&dna.instructions, IS_A), "Missing IS_A triple");

        // Should have PartOf from "gồm"
        assert!(has_part_of(&dna.instructions), "Missing PartOf");

        // Should have 4 steps
        let step_count = count_of(&dna.instructions, |i| matches!(i, Instruction::Step { .. }));
        assert_eq!(step_count, 4, "Expected 4 steps, got {}", step_count);

        // Should have Quantity from "28°" and "90°"
        assert!(has_quantity(&dna.instructions), "Missing Quantity");

        // At least 8 total instructions
        assert!(
            dna.instructions.len() >= 8,
            "Expected at least 8 instructions, got {}",
            dna.instructions.len()
        );
    }

    // ---- English integration test ----

    #[test]
    fn test_english_full_text() {
        let dict = default_dict();
        let text = r#"Breaststroke is a swimming technique
Breaststroke consists of arm stroke, leg kick, and breathing
Step 1: Extend both arms forward
Step 2: Sweep arms outward
Step 3: Frog kick with legs
Step 4: Inhale when head rises above water
Ideal water temperature = 28°"#;

        let dna = parse_text_to_core_dna(text, &dict).unwrap();
        println!("\n=== English Full Parse ===");
        for (i, instr) in dna.instructions.iter().enumerate() {
            println!("  [{}] {:?}", i, instr);
        }
        println!("Total instructions: {}", dna.instructions.len());

        assert_eq!(dna.header.gene_type, 1);
        assert!(has_triple(&dna.instructions, IS_A));
        assert!(has_part_of(&dna.instructions));
        let step_count = count_of(&dna.instructions, |i| matches!(i, Instruction::Step { .. }));
        assert_eq!(step_count, 4);
        assert!(has_quantity(&dna.instructions));
    }

    // ---- Unit detection ----

    #[test]
    fn test_detect_units() {
        assert!(detect_unit("35.2°").is_some());
        assert!(detect_unit("100m").is_some());
        assert!(detect_unit("5kg").is_some());
        assert!(detect_unit("50%").is_some());
        assert!(detect_unit("hello").is_none());

        let (num, unit) = detect_unit("35.2°").unwrap();
        assert_eq!(num, "35.2");
        assert_eq!(unit, UNIT_DEGREE);
    }

    // ---- Numeric parsing ----

    #[test]
    fn test_parse_numeric_values() {
        assert_eq!(parse_numeric("42"), Some(NumericValue::U8(42)));
        assert_eq!(parse_numeric("300"), Some(NumericValue::U16(300)));
        assert_eq!(parse_numeric("-5"), Some(NumericValue::I16(-5)));
        assert!(matches!(parse_numeric("3.14"), Some(NumericValue::F32(_))));
        assert_eq!(parse_numeric(""), None);
        assert_eq!(parse_numeric("abc"), None);
    }

    // ---- ConceptDict tests ----

    #[test]
    fn test_dict_lookup_or_create() {
        let mut dict = ConceptDict::new();
        let id1 = dict.lookup_or_create("hello");
        let id2 = dict.lookup_or_create("hello");
        assert_eq!(id1, id2, "Same word should return same ID");

        let id3 = dict.lookup_or_create("world");
        assert_ne!(id1, id3, "Different words should get different IDs");
    }

    #[test]
    fn test_dict_case_insensitive() {
        let mut dict = ConceptDict::new();
        dict.insert("Swimming", 500);
        assert_eq!(dict.lookup("swimming"), 500);
        assert_eq!(dict.lookup("SWIMMING"), 500);
        assert_eq!(dict.lookup("Swimming"), 500);
    }

    // ---- Edge cases ----

    #[test]
    fn test_mixed_language() {
        let dict = default_dict();
        let text = "Bơi ếch là breaststroke\nStep 1: Duỗi tay";
        let dna = parse_text_to_core_dna(text, &dict).unwrap();
        println!("Mixed language: {:?}", dna.instructions);
        assert!(has_triple(&dna.instructions, IS_A));
        assert!(has_step(&dna.instructions));
    }

    #[test]
    fn test_multiple_numbers_in_line() {
        let dict = default_dict();
        let text = "Khoảng cách 50m tốc độ 2.5km";
        let dna = parse_text_to_core_dna(text, &dict).unwrap();
        println!("Multiple numbers: {:?}", dna.instructions);
        let qty_count = count_of(&dna.instructions, |i| {
            matches!(i, Instruction::Quantity { .. })
        });
        assert!(
            qty_count >= 2,
            "Expected at least 2 quantities, got {}",
            qty_count
        );
    }

    #[test]
    fn test_includes_pattern() {
        let dict = default_dict();
        let text = "The technique includes breathing, kicking, and pulling";
        let dna = parse_text_to_core_dna(text, &dict).unwrap();
        println!("Includes: {:?}", dna.instructions);
        assert!(has_part_of(&dna.instructions));
    }
}
