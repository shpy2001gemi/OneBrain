//! # ku_system_prompt — System Prompt Generator for Knowledge Encoder AI
//!
//! Generates system prompts that instruct local AI models to act as
//! Knowledge Encoders — converting natural-language input into structured
//! Knowledge Units (KUs) via tool-calling.
//!
//! Two prompt variants:
//! - [`generate_system_prompt`] — full prompt with few-shot examples (~3-4K tokens)
//! - [`generate_compact_prompt`] — abbreviated prompt for context-limited models (<2K tokens)

use crate::text_parser::ConceptDict;

// ============================================================================
// Full system prompt
// ============================================================================

/// Generate a comprehensive system prompt for a Knowledge Encoder AI model.
///
/// # Arguments
/// * `dict` — the active concept dictionary; the first 50 entries are
///   embedded so the AI can resolve words → ConceptIds without guessing.
/// * `tool_defs_json` — a JSON string describing the available tool
///   functions (lookup_concept, create_ku, set_certainty, finalize, …).
///
/// # Returns
/// A `String` containing the full system prompt, typically 3-4K tokens.
pub fn generate_system_prompt(dict: &ConceptDict, tool_defs_json: &str) -> String {
    let mut prompt = String::with_capacity(8192);

    // ── Role description (bilingual) ──────────────────────────────────
    prompt.push_str(ROLE_DESCRIPTION);

    // ── Encoding rules ───────────────────────────────────────────────
    prompt.push_str(ENCODING_RULES);

    // ── Few-shot examples ────────────────────────────────────────────
    prompt.push_str(FEW_SHOT_EXAMPLES);

    // ── Tool definitions ─────────────────────────────────────────────
    prompt.push_str("\n## Available Tools\n\n");
    prompt.push_str("You MUST use these tools to create KUs. Never output raw JSON.\n\n");
    prompt.push_str("```json\n");
    prompt.push_str(tool_defs_json);
    prompt.push_str("\n```\n");

    // ── Concept dictionary snapshot ──────────────────────────────────
    prompt.push_str("\n## Concept Dictionary (first 50 entries)\n\n");
    prompt.push_str("Use these IDs when encoding. ");
    prompt.push_str("If a concept is not listed, call `lookup_concept` first.\n\n");
    prompt.push_str(&format_dict_snapshot(dict, 50));

    prompt
}

// ============================================================================
// Compact system prompt
// ============================================================================

/// Generate a compact system prompt for context-limited models.
///
/// Target: under 2 000 tokens. Omits few-shot examples and truncates the
/// dictionary snapshot to 20 entries.
///
/// # Arguments
/// * `dict` — the active concept dictionary (first 20 entries embedded).
/// * `tool_defs_json` — JSON string of available tool functions.
///
/// # Returns
/// A `String` containing the compact system prompt.
pub fn generate_compact_prompt(dict: &ConceptDict, tool_defs_json: &str) -> String {
    let mut prompt = String::with_capacity(4096);

    // ── Compact role ─────────────────────────────────────────────────
    prompt.push_str(COMPACT_ROLE);

    // ── Compact rules ────────────────────────────────────────────────
    prompt.push_str(COMPACT_RULES);

    // ── Tool definitions ─────────────────────────────────────────────
    prompt.push_str("\n## Tools\n\n```json\n");
    prompt.push_str(tool_defs_json);
    prompt.push_str("\n```\n");

    // ── Compact dictionary snapshot ──────────────────────────────────
    prompt.push_str("\n## Dict (top 20)\n\n");
    prompt.push_str(&format_dict_snapshot(dict, 20));

    prompt
}

// ============================================================================
// Constants — prompt fragments
// ============================================================================

const ROLE_DESCRIPTION: &str = r#"# Knowledge Encoder System Prompt

## Role / Vai trò

You are a **Knowledge Encoder** (Bộ mã hóa Tri thức).

Your job is to convert natural-language input — in any language — into
structured **Knowledge Units (KUs)** using the UKRL encoding format.
Each KU captures exactly ONE atomic idea: a fact, a relationship, a
measurement, or a procedural step.

Nhiệm vụ của bạn là chuyển đổi ngôn ngữ tự nhiên — bất kỳ ngôn ngữ nào —
thành các **Đơn vị Tri thức (KU)** theo định dạng mã hóa UKRL.
Mỗi KU chứa đúng MỘT ý tưởng nguyên tử: một sự kiện, một mối quan hệ,
một phép đo, hoặc một bước thủ tục.

"#;

const ENCODING_RULES: &str = r#"## Encoding Rules

1. **1 KU = 1 idea.** Never pack multiple facts into a single KU.
   - ✅ "Earth's radius = 6,371 km" → 1 KU (measurement)
   - ✅ "Earth is a planet" → 1 KU (classification)
   - ❌ "Earth is a planet with radius 6,371 km" → should be 2 KUs

2. **Always lookup concepts first.** Before creating a KU, call
   `lookup_concept("word")` for every key concept. Use the returned
   ConceptId — never invent IDs.

3. **Use `set_certainty(ku_id, level)` for epistemic status.**
   Levels: `established_fact`, `strong_evidence`, `moderate_evidence`,
   `preliminary`, `hypothesis`, `speculation`, `disputed`, `refuted`.
   Default is `moderate_evidence` if uncertain.

4. **Call `finalize(ku_id)` after each KU** to commit it to the store.
   A KU is not saved until finalized.

5. **Encoding order matters:**
   a. `lookup_concept` for all terms
   b. `create_ku` with codons, bonds, and genes
   c. `set_certainty` if not default
   d. `finalize`

6. **Relations (Bonds):** Use typed relations:
   - `is_a` (classification), `has_part` (composition)
   - `causes`, `requires`, `follows` (causal/temporal)
   - `measured_as` (numeric properties)
   - `related_to` (generic association — use sparingly)

7. **Genes carry content:** Use the appropriate gene type:
   - `Quantity` for numeric values with units
   - `Text` for free-text annotations
   - `Step` for procedural steps
   - `Constraint` for value ranges
   - `Formula` for mathematical expressions

"#;

const FEW_SHOT_EXAMPLES: &str = r#"## Examples

### Example 1: "Bơi ếch là kỹ thuật bơi cơ bản" (Breaststroke is a basic swimming technique)

```
→ lookup_concept("bơi ếch")      → 202
→ lookup_concept("kỹ thuật")     → 203
→ lookup_concept("bơi")          → 200
→ create_ku({
    codons: [
      { concept: 202, role: "object" },
      { concept: 203, role: "quality" },
      { concept: 200, role: "agent" }
    ],
    bonds: [
      { subject: 202, predicate: "is_a", object: 203 }
    ],
    genes: [
      { type: "text", value: "Breaststroke is a basic swimming technique" }
    ]
  })                               → ku_001
→ set_certainty(ku_001, "established_fact")
→ finalize(ku_001)
```

### Example 2: "A rocket consists of body and shell" (Tên lửa gồm thân và vỏ)

```
→ lookup_concept("rocket")       → 500
→ lookup_concept("body")         → 213
→ lookup_concept("shell")        → 501
→ create_ku({
    codons: [
      { concept: 500, role: "agent" },
      { concept: 213, role: "object" },
      { concept: 501, role: "object" }
    ],
    bonds: [
      { subject: 500, predicate: "has_part", object: 213 },
      { subject: 500, predicate: "has_part", object: 501 }
    ],
    genes: [
      { type: "text", value: "A rocket consists of body and shell" }
    ]
  })                               → ku_002
→ set_certainty(ku_002, "established_fact")
→ finalize(ku_002)
```

"#;

const COMPACT_ROLE: &str = r#"# Knowledge Encoder

You are a Knowledge Encoder. Convert natural language → structured Knowledge Units (KUs).
1 KU = 1 atomic idea (fact, relation, measurement, or step).

"#;

const COMPACT_RULES: &str = r#"## Rules

1. **1 KU = 1 idea.** Never combine multiple facts.
2. **Lookup first.** `lookup_concept("word")` before any `create_ku`.
3. **Set certainty.** `set_certainty(id, level)` — levels: established_fact, strong_evidence, moderate_evidence, preliminary, hypothesis, speculation, disputed, refuted.
4. **Finalize.** `finalize(id)` after every KU.
5. **Bonds:** is_a, has_part, causes, requires, follows, measured_as, related_to.
6. **Genes:** Quantity (numbers+units), Text, Step, Constraint, Formula.

"#;

// ============================================================================
// Helpers
// ============================================================================

/// Format the first `max_entries` of a ConceptDict as a markdown table.
fn format_dict_snapshot(dict: &ConceptDict, max_entries: usize) -> String {
    let mut out = String::from("| Word | ConceptId |\n|------|----------|\n");

    // Collect, sort by ConceptId for stable output, then take first N
    let mut entries: Vec<(&String, &u64)> = dict.iter().collect();
    entries.sort_by_key(|&(_, id)| *id);
    entries.truncate(max_entries);

    for (word, id) in &entries {
        out.push_str(&format!("| {} | {} |\n", word, id));
    }

    if dict.len() > max_entries {
        out.push_str(&format!(
            "\n_…and {} more entries. Use `lookup_concept` for unlisted words._\n",
            dict.len() - max_entries
        ));
    }

    out
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_parser::default_dict;

    const SAMPLE_TOOLS: &str = r#"[
  {
    "name": "lookup_concept",
    "description": "Look up a word in the concept dictionary",
    "parameters": { "word": "string" }
  },
  {
    "name": "create_ku",
    "description": "Create a new Knowledge Unit",
    "parameters": { "codons": "array", "bonds": "array", "genes": "array" }
  },
  {
    "name": "set_certainty",
    "description": "Set epistemic certainty level",
    "parameters": { "ku_id": "string", "level": "string" }
  },
  {
    "name": "finalize",
    "description": "Finalize and commit a KU",
    "parameters": { "ku_id": "string" }
  }
]"#;

    // ─── generate_system_prompt ──────────────────────────────────────

    #[test]
    fn test_full_prompt_contains_role() {
        let dict = default_dict();
        let prompt = generate_system_prompt(&dict, SAMPLE_TOOLS);
        assert!(prompt.contains("Knowledge Encoder"),
            "Full prompt must contain role name");
        assert!(prompt.contains("Bộ mã hóa Tri thức"),
            "Full prompt must contain Vietnamese role name");
    }

    #[test]
    fn test_full_prompt_contains_rules() {
        let dict = default_dict();
        let prompt = generate_system_prompt(&dict, SAMPLE_TOOLS);
        assert!(prompt.contains("1 KU = 1 idea"),
            "Must contain the 1-KU-1-idea rule");
        assert!(prompt.contains("lookup_concept"),
            "Must mention lookup_concept");
        assert!(prompt.contains("set_certainty"),
            "Must mention set_certainty");
        assert!(prompt.contains("finalize"),
            "Must mention finalize");
    }

    #[test]
    fn test_full_prompt_contains_examples() {
        let dict = default_dict();
        let prompt = generate_system_prompt(&dict, SAMPLE_TOOLS);
        // Example 1: bơi ếch / breaststroke
        assert!(prompt.contains("bơi ếch"),
            "Must contain the breaststroke example (Vietnamese)");
        assert!(prompt.contains("Breaststroke"),
            "Must contain the breaststroke example (English)");
        // Example 2: rocket body & shell
        assert!(prompt.contains("rocket"),
            "Must contain the rocket example");
        assert!(prompt.contains("body"),
            "Must contain body in rocket example");
        assert!(prompt.contains("shell"),
            "Must contain shell in rocket example");
    }

    #[test]
    fn test_full_prompt_contains_tool_defs() {
        let dict = default_dict();
        let prompt = generate_system_prompt(&dict, SAMPLE_TOOLS);
        assert!(prompt.contains("lookup_concept"),
            "Must inject tool definitions");
        assert!(prompt.contains("create_ku"),
            "Must inject create_ku tool");
        assert!(prompt.contains("```json"),
            "Tool defs should be in a JSON code block");
    }

    #[test]
    fn test_full_prompt_contains_dict_snapshot() {
        let dict = default_dict();
        let prompt = generate_system_prompt(&dict, SAMPLE_TOOLS);
        assert!(prompt.contains("| Word | ConceptId |"),
            "Must contain dict snapshot table header");
        // default_dict has ~100 entries; we show first 50
        assert!(prompt.contains("…and"),
            "Must indicate more entries exist");
    }

    #[test]
    fn test_full_prompt_with_empty_dict() {
        let dict = ConceptDict::new();
        let prompt = generate_system_prompt(&dict, SAMPLE_TOOLS);
        // Should still work — just no table rows
        assert!(prompt.contains("Knowledge Encoder"));
        assert!(prompt.contains("| Word | ConceptId |"));
        assert!(!prompt.contains("…and"),
            "Empty dict should not show 'more entries' message");
    }

    #[test]
    fn test_full_prompt_with_small_dict() {
        let mut dict = ConceptDict::new();
        dict.insert("water", 10);
        dict.insert("fire", 11);
        let prompt = generate_system_prompt(&dict, SAMPLE_TOOLS);
        assert!(prompt.contains("| water | 10 |"));
        assert!(prompt.contains("| fire | 11 |"));
        assert!(!prompt.contains("…and"),
            "Small dict (<50) should not show 'more entries'");
    }

    #[test]
    fn test_full_prompt_dict_truncation() {
        let mut dict = ConceptDict::new();
        for i in 0..100u64 {
            dict.insert(&format!("concept_{}", i), i);
        }
        let prompt = generate_system_prompt(&dict, SAMPLE_TOOLS);
        // Should only show 50 entries
        assert!(prompt.contains("…and 50 more entries"),
            "Should indicate 50 remaining entries");
    }

    // ─── generate_compact_prompt ────────────────────────────────────

    #[test]
    fn test_compact_prompt_is_shorter() {
        let dict = default_dict();
        let full = generate_system_prompt(&dict, SAMPLE_TOOLS);
        let compact = generate_compact_prompt(&dict, SAMPLE_TOOLS);
        assert!(compact.len() < full.len(),
            "Compact prompt ({} bytes) must be shorter than full ({} bytes)",
            compact.len(), full.len());
    }

    #[test]
    fn test_compact_prompt_under_2000_tokens_estimate() {
        let dict = default_dict();
        let compact = generate_compact_prompt(&dict, SAMPLE_TOOLS);
        // Rough estimate: 1 token ≈ 4 chars for English text
        let estimated_tokens = compact.len() / 4;
        println!("Compact prompt: {} chars, ~{} estimated tokens",
            compact.len(), estimated_tokens);
        assert!(estimated_tokens < 2500,
            "Compact prompt should be roughly under 2000 tokens \
             (estimated {} tokens from {} chars)",
            estimated_tokens, compact.len());
    }

    #[test]
    fn test_compact_prompt_contains_essentials() {
        let dict = default_dict();
        let compact = generate_compact_prompt(&dict, SAMPLE_TOOLS);
        assert!(compact.contains("Knowledge Encoder"),
            "Compact must still state the role");
        assert!(compact.contains("1 KU = 1 idea"),
            "Compact must contain the core rule");
        assert!(compact.contains("lookup_concept"),
            "Compact must mention lookup_concept");
        assert!(compact.contains("finalize"),
            "Compact must mention finalize");
    }

    #[test]
    fn test_compact_prompt_no_examples() {
        let dict = default_dict();
        let compact = generate_compact_prompt(&dict, SAMPLE_TOOLS);
        assert!(!compact.contains("Example 1"),
            "Compact prompt should NOT include few-shot examples");
        assert!(!compact.contains("Example 2"),
            "Compact prompt should NOT include few-shot examples");
    }

    #[test]
    fn test_compact_prompt_dict_max_20() {
        let mut dict = ConceptDict::new();
        for i in 0..100u64 {
            dict.insert(&format!("concept_{}", i), i);
        }
        let compact = generate_compact_prompt(&dict, SAMPLE_TOOLS);
        assert!(compact.contains("…and 80 more entries"),
            "Compact prompt shows 20 entries → 80 remaining");
    }

    #[test]
    fn test_compact_prompt_contains_tools() {
        let dict = ConceptDict::new();
        let compact = generate_compact_prompt(&dict, SAMPLE_TOOLS);
        assert!(compact.contains("create_ku"),
            "Compact must inject tool definitions");
    }

    // ─── format_dict_snapshot ───────────────────────────────────────

    #[test]
    fn test_dict_snapshot_sorted_by_id() {
        let mut dict = ConceptDict::new();
        dict.insert("zebra", 999);
        dict.insert("alpha", 1);
        dict.insert("mid", 500);
        let snapshot = format_dict_snapshot(&dict, 50);

        let alpha_pos = snapshot.find("| alpha |").unwrap();
        let mid_pos = snapshot.find("| mid |").unwrap();
        let zebra_pos = snapshot.find("| zebra |").unwrap();
        assert!(alpha_pos < mid_pos, "alpha (id=1) must come before mid (id=500)");
        assert!(mid_pos < zebra_pos, "mid (id=500) must come before zebra (id=999)");
    }

    #[test]
    fn test_dict_snapshot_empty() {
        let dict = ConceptDict::new();
        let snapshot = format_dict_snapshot(&dict, 50);
        assert!(snapshot.contains("| Word | ConceptId |"),
            "Even empty dict should have table header");
        // Should NOT have any data rows (just header + separator)
        let lines: Vec<&str> = snapshot.lines().collect();
        assert_eq!(lines.len(), 2,
            "Empty dict snapshot should only have header (2 lines)");
    }

    #[test]
    fn test_dict_snapshot_exact_max() {
        let mut dict = ConceptDict::new();
        for i in 0..50u64 {
            dict.insert(&format!("w{}", i), i);
        }
        let snapshot = format_dict_snapshot(&dict, 50);
        assert!(!snapshot.contains("…and"),
            "Exactly 50 entries with max=50 should not show overflow");
    }

    // ─── Integration: prompt is valid UTF-8 and non-empty ──────────

    #[test]
    fn test_prompts_are_nonempty() {
        let dict = default_dict();
        let full = generate_system_prompt(&dict, "[]");
        let compact = generate_compact_prompt(&dict, "[]");
        assert!(!full.is_empty(), "Full prompt must not be empty");
        assert!(!compact.is_empty(), "Compact prompt must not be empty");
    }

    #[test]
    fn test_full_prompt_printout() {
        let dict = default_dict();
        let prompt = generate_system_prompt(&dict, SAMPLE_TOOLS);
        println!("=== FULL SYSTEM PROMPT ({} chars) ===", prompt.len());
        println!("{}", prompt);
        println!("=== END ===");
    }

    #[test]
    fn test_compact_prompt_printout() {
        let dict = default_dict();
        let compact = generate_compact_prompt(&dict, SAMPLE_TOOLS);
        println!("=== COMPACT SYSTEM PROMPT ({} chars) ===", compact.len());
        println!("{}", compact);
        println!("=== END ===");
    }
}
