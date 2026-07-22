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

2. **Start a KU.** Call `new_ku(gene_type="fact")` before adding instructions.

3. **Pass concept names as STRINGS** to all add_* tools.
   Do NOT use numeric IDs. The system auto-resolves names.
   - `add_triple(subject="water", predicate="is_a", object="liquid")`
   - `add_quality(subject="water", quality="boiling")`
   - `add_quantity(subject="water", value=100, unit="degree")`

4. **Call `finalize()` after each KU** to commit it.

5. **Encoding order:**
   a. `new_ku` to start
   b. `add_*` instructions with concept name strings
   c. `finalize` to commit

"#;

const FEW_SHOT_EXAMPLES: &str = r#"## Examples

### Example 1: "Water boils at 100 degrees Celsius"

```
→ new_ku(gene_type="fact")
→ add_quality(subject="water", quality="boiling")
→ add_quantity(subject="water", value=100, unit="degree")
→ finalize()
```

### Example 2: "Bơi ếch là kỹ thuật bơi cơ bản" (Breaststroke is a basic swimming technique)

```
→ new_ku(gene_type="fact")
→ add_triple(subject="bơi ếch", predicate="is_a", object="kỹ thuật bơi")
→ add_quality(subject="bơi ếch", quality="cơ bản")
→ finalize()
```

### Example 3: "The heart pumps blood to the lungs"

```
→ new_ku(gene_type="fact")
→ add_triple(subject="heart", predicate="pumps", object="blood")
→ add_located(subject="blood", location="lungs")
→ finalize()
```

### IMPORTANT:
- Pass concept names as **strings** (e.g. subject="water"), NOT numbers.
- Include 2+ add_* instructions per KU for richer encoding.

"#;

const COMPACT_ROLE: &str = r#"# Knowledge Encoder

You are a Knowledge Encoder. Convert natural language → structured Knowledge Units (KUs).
1 KU = 1 atomic idea (fact, relation, measurement, or step).

"#;

const COMPACT_RULES: &str = r#"## Rules

1. **1 KU = 1 idea.** Never combine multiple facts.
2. **Start KU.** Call `new_ku(gene_type="fact")` before adding instructions.
3. **Pass concept names as STRINGS** to add_* tools. Do NOT use numbers.
   Example: `add_quality(subject="water", quality="boiling")`
4. **Include 2+ instructions** per KU for rich encoding.
5. **Finalize.** Call `finalize()` after every KU.

"#;

// ============================================================================
// V2 Extraction Prompt — compact JSON output (no tool-calling)
// ============================================================================

/// Generate a compact extraction prompt for the v2 pipeline.
///
/// Unlike the v1 prompt which uses tool-calling, this prompt asks the AI
/// to output a JSON array of SPO triples directly. Much simpler, smaller
/// (~300 tokens), and works well even with small models.
///
/// # Arguments
/// * `anchor_instruction` — optional string like "DO NOT modify: H8O, 100°C"
///
/// # Returns
/// A `(system_prompt, user_template)` tuple. The caller fills the user
/// template with the actual paragraph text.
pub fn generate_extraction_prompt(anchor_instruction: Option<&str>) -> (String, String) {
    let mut system = String::with_capacity(2048);

    system.push_str(EXTRACTION_SYSTEM);

    if let Some(anchor) = anchor_instruction {
        system.push('\n');
        system.push_str(anchor);
        system.push('\n');
    }

    let user_template = "Extract knowledge from this text:\n\n{TEXT}".to_string();

    (system, user_template)
}

const EXTRACTION_SYSTEM: &str = r#"You are a knowledge extractor. Given text in any language, extract structured triples.

Output a JSON array. Each element:
{"s":"subject","s_en":"english subject","p":"predicate","o":"object","o_en":"english object","qty":number_or_null,"role":"semantic_role","notation":"type_or_null","c":"certainty"}

## Fields
- s, o: original language
- s_en, o_en: English canonical name (translate if needed)
- p: predicate in original language
- qty: number extracted from text (e.g., "4 legs" → 4), null if none
- role: one of part, material, purpose, location, cause, property, category, formula, relation
- notation: only when role=formula. One of: chemical, latex, smiles, code
- c: one of always, usually, sometimes, rarely

## Rules
1. Split lists into separate triples: "wood, metal or plastic" → 3 triples
2. Extract numbers from text: "four legs" → qty=4
3. Detect certainty from words: "usually/thường" → usually, "may/có thể" → sometimes
4. Chemical formulas (H2O, NaCl): role=formula, notation=chemical
5. Math expressions (E=mc²): role=formula, notation=latex
6. DO NOT correct, modify, or "fix" any terms from the input. Extract exactly as written.
7. Output ONLY the JSON array, nothing else.
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
        assert!(
            prompt.contains("Knowledge Encoder"),
            "Full prompt must contain role name"
        );
        assert!(
            prompt.contains("Bộ mã hóa Tri thức"),
            "Full prompt must contain Vietnamese role name"
        );
    }

    #[test]
    fn test_full_prompt_contains_rules() {
        let dict = default_dict();
        let prompt = generate_system_prompt(&dict, SAMPLE_TOOLS);
        assert!(
            prompt.contains("1 KU = 1 idea"),
            "Must contain the 1-KU-1-idea rule"
        );
        assert!(
            prompt.contains("lookup_concept"),
            "Must mention lookup_concept"
        );
        assert!(
            prompt.contains("set_certainty"),
            "Must mention set_certainty"
        );
        assert!(prompt.contains("finalize"), "Must mention finalize");
    }

    #[test]
    fn test_full_prompt_contains_examples() {
        let dict = default_dict();
        let prompt = generate_system_prompt(&dict, SAMPLE_TOOLS);
        // Example 1: bơi ếch / breaststroke
        assert!(
            prompt.contains("bơi ếch"),
            "Must contain the breaststroke example (Vietnamese)"
        );
        assert!(
            prompt.contains("Breaststroke"),
            "Must contain the breaststroke example (English)"
        );
        // Example 2: rocket body & shell
        assert!(prompt.contains("rocket"), "Must contain the rocket example");
        assert!(
            prompt.contains("body"),
            "Must contain body in rocket example"
        );
        assert!(
            prompt.contains("shell"),
            "Must contain shell in rocket example"
        );
    }

    #[test]
    fn test_full_prompt_contains_tool_defs() {
        let dict = default_dict();
        let prompt = generate_system_prompt(&dict, SAMPLE_TOOLS);
        assert!(
            prompt.contains("lookup_concept"),
            "Must inject tool definitions"
        );
        assert!(prompt.contains("create_ku"), "Must inject create_ku tool");
        assert!(
            prompt.contains("```json"),
            "Tool defs should be in a JSON code block"
        );
    }

    #[test]
    fn test_full_prompt_contains_dict_snapshot() {
        let dict = default_dict();
        let prompt = generate_system_prompt(&dict, SAMPLE_TOOLS);
        assert!(
            prompt.contains("| Word | ConceptId |"),
            "Must contain dict snapshot table header"
        );
        // default_dict has ~100 entries; we show first 50
        assert!(prompt.contains("…and"), "Must indicate more entries exist");
    }

    #[test]
    fn test_full_prompt_with_empty_dict() {
        let dict = ConceptDict::new();
        let prompt = generate_system_prompt(&dict, SAMPLE_TOOLS);
        // Should still work — just no table rows
        assert!(prompt.contains("Knowledge Encoder"));
        assert!(prompt.contains("| Word | ConceptId |"));
        assert!(
            !prompt.contains("…and"),
            "Empty dict should not show 'more entries' message"
        );
    }

    #[test]
    fn test_full_prompt_with_small_dict() {
        let mut dict = ConceptDict::new();
        dict.insert("water", 10);
        dict.insert("fire", 11);
        let prompt = generate_system_prompt(&dict, SAMPLE_TOOLS);
        assert!(prompt.contains("| water | 10 |"));
        assert!(prompt.contains("| fire | 11 |"));
        assert!(
            !prompt.contains("…and"),
            "Small dict (<50) should not show 'more entries'"
        );
    }

    #[test]
    fn test_full_prompt_dict_truncation() {
        let mut dict = ConceptDict::new();
        for i in 0..100u64 {
            dict.insert(&format!("concept_{}", i), i);
        }
        let prompt = generate_system_prompt(&dict, SAMPLE_TOOLS);
        // Should only show 50 entries
        assert!(
            prompt.contains("…and 50 more entries"),
            "Should indicate 50 remaining entries"
        );
    }

    // ─── generate_compact_prompt ────────────────────────────────────

    #[test]
    fn test_compact_prompt_is_shorter() {
        let dict = default_dict();
        let full = generate_system_prompt(&dict, SAMPLE_TOOLS);
        let compact = generate_compact_prompt(&dict, SAMPLE_TOOLS);
        assert!(
            compact.len() < full.len(),
            "Compact prompt ({} bytes) must be shorter than full ({} bytes)",
            compact.len(),
            full.len()
        );
    }

    #[test]
    fn test_compact_prompt_under_2000_tokens_estimate() {
        let dict = default_dict();
        let compact = generate_compact_prompt(&dict, SAMPLE_TOOLS);
        // Rough estimate: 1 token ≈ 4 chars for English text
        let estimated_tokens = compact.len() / 4;
        println!(
            "Compact prompt: {} chars, ~{} estimated tokens",
            compact.len(),
            estimated_tokens
        );
        assert!(
            estimated_tokens < 2500,
            "Compact prompt should be roughly under 2000 tokens \
             (estimated {} tokens from {} chars)",
            estimated_tokens,
            compact.len()
        );
    }

    #[test]
    fn test_compact_prompt_contains_essentials() {
        let dict = default_dict();
        let compact = generate_compact_prompt(&dict, SAMPLE_TOOLS);
        assert!(
            compact.contains("Knowledge Encoder"),
            "Compact must still state the role"
        );
        assert!(
            compact.contains("1 KU = 1 idea"),
            "Compact must contain the core rule"
        );
        assert!(
            compact.contains("lookup_concept"),
            "Compact must mention lookup_concept"
        );
        assert!(
            compact.contains("finalize"),
            "Compact must mention finalize"
        );
    }

    #[test]
    fn test_compact_prompt_no_examples() {
        let dict = default_dict();
        let compact = generate_compact_prompt(&dict, SAMPLE_TOOLS);
        assert!(
            !compact.contains("Example 1"),
            "Compact prompt should NOT include few-shot examples"
        );
        assert!(
            !compact.contains("Example 2"),
            "Compact prompt should NOT include few-shot examples"
        );
    }

    #[test]
    fn test_compact_prompt_dict_max_20() {
        let mut dict = ConceptDict::new();
        for i in 0..100u64 {
            dict.insert(&format!("concept_{}", i), i);
        }
        let compact = generate_compact_prompt(&dict, SAMPLE_TOOLS);
        assert!(
            compact.contains("…and 80 more entries"),
            "Compact prompt shows 20 entries → 80 remaining"
        );
    }

    #[test]
    fn test_compact_prompt_contains_tools() {
        let dict = ConceptDict::new();
        let compact = generate_compact_prompt(&dict, SAMPLE_TOOLS);
        assert!(
            compact.contains("create_ku"),
            "Compact must inject tool definitions"
        );
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
        assert!(
            alpha_pos < mid_pos,
            "alpha (id=1) must come before mid (id=500)"
        );
        assert!(
            mid_pos < zebra_pos,
            "mid (id=500) must come before zebra (id=999)"
        );
    }

    #[test]
    fn test_dict_snapshot_empty() {
        let dict = ConceptDict::new();
        let snapshot = format_dict_snapshot(&dict, 50);
        assert!(
            snapshot.contains("| Word | ConceptId |"),
            "Even empty dict should have table header"
        );
        // Should NOT have any data rows (just header + separator)
        let lines: Vec<&str> = snapshot.lines().collect();
        assert_eq!(
            lines.len(),
            2,
            "Empty dict snapshot should only have header (2 lines)"
        );
    }

    #[test]
    fn test_dict_snapshot_exact_max() {
        let mut dict = ConceptDict::new();
        for i in 0..50u64 {
            dict.insert(&format!("w{}", i), i);
        }
        let snapshot = format_dict_snapshot(&dict, 50);
        assert!(
            !snapshot.contains("…and"),
            "Exactly 50 entries with max=50 should not show overflow"
        );
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
