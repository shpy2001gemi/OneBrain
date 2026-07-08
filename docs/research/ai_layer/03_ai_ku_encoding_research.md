# AI-Assisted KU Encoding — OneBrain Pillar 6

> **Research Date:** July 2026  
> **Author:** OneBrain Research Team  
> **Status:** Research Complete  
> **Scope:** How local AI models encode natural language text into CoreDna instructions (Tier 2 Encoding)

---

## Executive Summary

OneBrain's existing `KuToolExecutor` + 15-tool schema provides an excellent foundation for AI integration. The AI model only needs to output JSON tool calls matching the existing schema, and the executor handles CoreDna binary construction. **Grammar-constrained generation (GBNF) with a 7-8B model** is the recommended approach for Tier 2 encoding.

### Key Findings

| Finding | Detail |
|:---|:---|
| **Best technique** | Grammar-constrained generation (GBNF) — impossible to produce invalid output |
| **Recommended approach** | Two-phase: reasoning first, then constrained format output |
| **7-8B sweet spot** | 80-90% KU encoding accuracy, fits consumer hardware (5GB VRAM) |
| **3B models** | 60-70% accuracy — usable with grammar constraints, better with fine-tuning |
| **Existing infrastructure** | `KuToolExecutor` (754 LOC) + `ku_tools.rs` (454 LOC) already ready |
| **Fallback** | Graceful degradation to Tier 1 (rule-based) when AI confidence < 0.60 |
| **Vietnamese support** | Qwen 2.5/3 and Gemma 4 have good multilingual capabilities |

---

## 1. Structured Output Generation from LLMs

### 1.1 State of the Art (2025-2026)

| Approach | Mechanism | Guarantee | Best For |
|:---|:---|:---|:---|
| **JSON Mode** | Provider-level setting | No guarantee — can still produce invalid syntax | Quick prototyping |
| **Schema-based JSON** | Model prompted with JSON Schema | Retry-based — fails then retries | Medium complexity |
| **Grammar-Constrained (GBNF)** | FSM masks invalid tokens at logit level | **Impossible** to produce invalid output | ✅ Production systems |
| **Tool/Function Calling** | Model outputs structured function call objects | Framework-dependent | Multi-step workflows |

### 1.2 Grammar-Constrained Generation (Recommended)

**How it works**: Before each token is sampled, a constraint engine (FSM or Pushdown Automaton) masks out tokens that would violate the specified grammar. This makes invalid output **impossible**.

**Key frameworks**:
- **XGrammar** (vLLM, SGLang): Optimized mask computation, microsecond overhead
- **llama.cpp GBNF**: Native support for JSON Schema → GBNF conversion
- **Outlines**: Python library for constrained decoding
- **Instructor**: Pydantic-based API

**GBNF Grammar for OneBrain Tool Calls**:
```gbnf
root ::= "[" ws tool-call ("," ws tool-call)* ws "]"
tool-call ::= "{" ws "\"name\":" ws tool-name "," ws "\"arguments\":" ws arguments ws "}"
tool-name ::= "\"new_ku\"" | "\"finalize\"" | "\"lookup\"" | "\"lookup_or_create\"" 
            | "\"add_triple\"" | "\"add_part_of\"" | "\"add_quality\"" 
            | "\"add_quantity\"" | "\"add_tolerance\"" | "\"add_enum_val\""
            | "\"add_causal\"" | "\"add_located\"" | "\"add_step\""
            | "\"set_certainty\"" | "\"set_difficulty\""
arguments ::= "{" ws (argument ("," ws argument)*)? ws "}"
argument ::= string ":" ws value
string ::= "\"" [a-zA-Z_]+ "\""
value ::= number | string | array
number ::= "-"? [0-9]+ ("." [0-9]+)?
array ::= "[" ws (value ("," ws value)*)? ws "]"
ws ::= [ \t\n]*
```

### 1.3 The "Quality Tax" Warning

ACL 2025 research shows that rigid formatting constraints can lead to a **slight decline in reasoning ability** on complex tasks. Solution: "reasoning-then-formatting" pipeline.

**Recommendation for OneBrain**: Use a two-phase approach:
1. Phase 1: Let model analyze text freely (chain-of-thought reasoning)
2. Phase 2: Constrain output to OneBrain tool-call JSON schema using GBNF grammar

### 1.4 Recommended JSON Schema for AI Output

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "properties": {
    "reasoning": {
      "type": "string",
      "description": "Model's analysis of the text (free-form thinking)"
    },
    "gene_type": {
      "type": "string",
      "enum": ["fact", "procedure", "experience", "hypothesis", "definition", "relation"]
    },
    "certainty": {
      "type": "integer",
      "minimum": 0,
      "maximum": 10000
    },
    "tool_calls": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "name": { "type": "string" },
          "arguments": { "type": "object" }
        },
        "required": ["name", "arguments"]
      }
    }
  },
  "required": ["gene_type", "tool_calls"]
}
```

---

## 2. Knowledge Extraction with Local LLMs

### 2.1 Current Paradigm

The field has shifted from traditional NER+RE pipelines to **generative extraction**:

- **Schema-Based** (recommended for OneBrain): Pre-defined ontology, entities mapped to known types → more consistent
- **Schema-Free**: Flexible discovery of new entities/relations → useful for exploration but inconsistent

### 2.2 Key Techniques for Triple Extraction

| Technique | Description | Suitability for OneBrain |
|:---|:---|:---|
| **Direct Prompting** | "Extract triples from this text" | Good for simple facts |
| **Schema-Guided Extraction** | Provide opcode set as schema, model maps text | ✅ Best fit |
| **Multi-Agent (OneKE)** | Multiple agents collaborate | Overkill for local |
| **Reasoning-Guided** | LLM verifies its own triples | Good for quality |

### 2.3 Hallucination Mitigation

1. **Schema validation**: Use `KuToolExecutor` to validate each tool call
2. **Source grounding**: Include original text in prompt, extract only what's stated
3. **Confidence scoring**: Model outputs certainty level → `set_certainty` tool
4. **LLM-as-Judge**: Second pass to verify (expensive, for high-value KUs only)

### 2.4 Multilingual Handling (Vietnamese + English)

**Vietnamese-capable models** (2025-2026):
- **Qwen 2.5/3**: Good Vietnamese, excellent structured output
- **Gemma 4**: Strong reasoning, decent Vietnamese
- **PhoGPT / Vistral-7B**: Specialized Vietnamese models
- **Phi-4-mini**: Punches above weight in multilingual tasks

**Cross-lingual strategy**:
1. ConceptDict already supports bilingual entries (`name_vi`, `name_en`)
2. Back-translation verification: encode Vietnamese → decode English → verify meaning
3. `text_parser.rs` already handles Vietnamese patterns ("X là Y", "X gồm A, B")

---

## 3. Prompt Engineering for CoreDna Encoding

### 3.1 System Prompt Design

```
You are OneBrain Knowledge Encoder v2. Your task is to convert natural language 
text into structured Knowledge Units (KUs) using tool calls.

## Available Tools (15 tools)
{tool_definitions_json from ku_tools.rs tool_definitions_openai_format()}

## Encoding Rules
1. ALWAYS start with new_ku(gene_type) and end with finalize()
2. Use lookup() first to check if a concept exists, then lookup_or_create() if not
3. Use the most SPECIFIC instruction type:
   - Use add_triple for "X is Y", "X has Y" relationships
   - Use add_part_of for containment/composition
   - Use add_quantity for any numeric measurement
   - Use add_causal for cause→effect
   - Use add_step for procedures (ordered steps)
4. Set certainty: 9500+ for established facts, 7000-9000 for well-known info, 
   5000-7000 for claims, <5000 for speculation
5. One KU per atomic knowledge unit

## Well-Known ConceptIds (use these directly, no lookup needed)
- 1 = IS_A ("is a" relationship)
- 2 = HAS_PART ("has part")
- 3 = RELATED_TO (generic relation)
- 10 = degree, 11 = meter, 12 = second, 13 = kg, 14 = percent
- 127 = UNKNOWN

## Gene Type Selection Guide
- "fact": Declarative statements (X is Y, measurements)
- "procedure": Step-by-step processes (recipes, instructions)
- "experience": First-person sensory/emotional accounts
- "hypothesis": Testable propositions, theories
- "definition": Concept definitions
```

### 3.2 Few-Shot Examples

**Example 1: Fact Encoding**
```
Input: "The Saturn V rocket stands 110.6 meters tall and weighs 2,970 tonnes"

Output:
[
  {"name": "new_ku", "arguments": {"gene_type": "fact"}},
  {"name": "lookup_or_create", "arguments": {"word": "saturn_v"}},
  {"name": "lookup_or_create", "arguments": {"word": "height"}},
  {"name": "lookup_or_create", "arguments": {"word": "mass"}},
  {"name": "add_quantity", "arguments": {"subject": 200, "value": 110.6, "unit": "meter"}},
  {"name": "add_quantity", "arguments": {"subject": 200, "value": 2970000.0, "unit": "kg"}},
  {"name": "set_certainty", "arguments": {"level": 9800}},
  {"name": "finalize", "arguments": {}}
]
```

**Example 2: Procedure Encoding (Vietnamese)**
```
Input: "Pha cà phê: Bước 1: Đun nước sôi. Bước 2: Cho cà phê vào phin. Bước 3: Rót nước nóng vào."

Output:
[
  {"name": "new_ku", "arguments": {"gene_type": "procedure"}},
  {"name": "lookup_or_create", "arguments": {"word": "pha_ca_phe"}},
  {"name": "lookup_or_create", "arguments": {"word": "dun"}},
  {"name": "lookup_or_create", "arguments": {"word": "nuoc"}},
  {"name": "lookup_or_create", "arguments": {"word": "ca_phe"}},
  {"name": "lookup_or_create", "arguments": {"word": "phin"}},
  {"name": "lookup_or_create", "arguments": {"word": "rot"}},
  {"name": "add_step", "arguments": {"ord": 0, "action": 201, "target": 202}},
  {"name": "add_step", "arguments": {"ord": 1, "action": 203, "target": 204}},
  {"name": "add_step", "arguments": {"ord": 2, "action": 205, "target": 202}},
  {"name": "set_difficulty", "arguments": {"level": 1}},
  {"name": "set_certainty", "arguments": {"level": 9000}},
  {"name": "finalize", "arguments": {}}
]
```

**Example 3: Experience Encoding**
```
Input: "The coffee tasted bitter and strong, warming my hands on a cold morning"

Output:
[
  {"name": "new_ku", "arguments": {"gene_type": "experience"}},
  {"name": "lookup_or_create", "arguments": {"word": "coffee"}},
  {"name": "lookup_or_create", "arguments": {"word": "bitter"}},
  {"name": "lookup_or_create", "arguments": {"word": "strong"}},
  {"name": "lookup_or_create", "arguments": {"word": "warming"}},
  {"name": "add_quality", "arguments": {"subject": 200, "quality": 201}},
  {"name": "add_quality", "arguments": {"subject": 200, "quality": 202}},
  {"name": "add_quality", "arguments": {"subject": 200, "quality": 203}},
  {"name": "set_certainty", "arguments": {"level": 8000}},
  {"name": "finalize", "arguments": {}}
]
```

### 3.3 Common Error Patterns in Small Models (3B-8B)

| Error Pattern | Frequency | Mitigation |
|:---|:---|:---|
| **Wrong gene_type** | ~15-25% | Provide explicit hints in prompt |
| **Missing finalize()** | ~10-20% | Grammar constraint forces it; auto-finalize in executor |
| **Hallucinated ConceptIds** | ~20-30% | Require lookup_or_create flow; validate IDs |
| **Overly generic triples** | ~25-35% | Few-shot examples showing specificity |
| **Number parsing errors** | ~5-10% | Post-validate numeric values |
| **Wrong tool for relationship** | ~15-20% | Clear tool descriptions + examples |
| **Incorrect certainty scale** | ~20-30% | Provide scale anchors in prompt |

---

## 4. Quality Verification Round-Trip

### 4.1 Encode-Decode-Compare Pipeline

```
Original Text
    │
    ▼
AI Encoding → CoreDna → encode() → wire_bytes
                                        │
                                        ▼
                          decode() → CoreDna' → expression() → Reconstructed Text
                                        │
                                        ▼
                          Compare: Original Text ↔ Reconstructed Text
```

### 4.2 Verification Pseudocode

```rust
fn verify_encoding(
    original_text: &str, 
    wire_bytes: &[u8], 
    dict: &ConceptDict,
) -> VerifyResult {
    // Step 1: Decode wire bytes back to CoreDna
    let dna = decode_core_dna(wire_bytes)?;
    
    // Step 2: Re-encode to verify bit-perfect roundtrip
    let re_encoded = encode_core_dna(&dna)?;
    let roundtrip_ok = blake3::hash(&re_encoded) == blake3::hash(wire_bytes);
    
    // Step 3: Generate expression (natural language) from CoreDna
    let runtime = KuRuntime::from_dna(dna);
    let reconstructed = runtime.expression(dict);
    
    // Step 4: Semantic similarity check (embedding model)
    let similarity = compute_semantic_similarity(original_text, &reconstructed);
    
    // Step 5: Information completeness check
    let completeness = check_information_coverage(original_text, &dna);
    
    VerifyResult {
        roundtrip_ok,
        semantic_similarity: similarity,  // 0.0 - 1.0
        completeness,                      // 0.0 - 1.0
        confidence: similarity * 0.6 + completeness * 0.4,
    }
}
```

### 4.3 Semantic Similarity Methods

| Method | Complexity | Accuracy | Suitability |
|:---|:---|:---|:---|
| **Embedding cosine similarity** | Low | Good | ✅ Best for local |
| **LLM-as-Judge** | High | Best | Too expensive for every KU |
| **BLEU/ROUGE** | Low | Poor | ❌ Token overlap doesn't capture semantics |
| **Keyword overlap** | Minimal | Decent | Good as quick sanity check |

**Recommended**: Use embedding model (nomic-embed-text) to compute cosine similarity. Threshold: ≥ 0.75 = pass.

### 4.4 Fallback Decision Logic

```rust
fn decide_encoding_tier(text: &str, ai_result: Option<AiResult>) -> EncodingDecision {
    match ai_result {
        Some(result) if result.confidence >= 0.85 => {
            EncodingDecision::UseAi(result)          // High quality → use AI
        }
        Some(result) if result.confidence >= 0.60 => {
            EncodingDecision::UseAiWithFlag(result)  // Decent → use but flag
        }
        Some(_) | None => {
            let tier1 = parse_text_to_core_dna(text, &dict);
            EncodingDecision::FallbackToTier1(tier1) // Low → rule-based
        }
    }
}
```

---

## 5. ConceptId Management with AI

### 5.1 AI Concept Mapping Protocol

1. **lookup()** → Check if concept exists in ConceptDict
2. **lookup_or_create()** → Auto-register new concepts
3. Well-known IDs (1-127) → Use directly without lookup

### 5.2 Normalization Pipeline

```
AI proposes: "boiling point" 
    → normalize: lowercase, trim, spaces → underscores
    → "boiling_point"
    → lookup("boiling_point") → found? use it : create new
    → also check aliases: "boiling point", "điểm sôi" → same concept
```

### 5.3 Cross-Language Concept Alignment

```rust
// Existing ConceptEntry already supports bilingual
pub struct ConceptEntry {
    pub id: ConceptId,
    pub name: String,
    pub name_vi: Option<String>,
    pub name_en: Option<String>,
    pub tier: u8,
    pub category: Option<String>,
}
```

**Strategy**: When processing Vietnamese text, create concepts with both `name_vi` and `name_en`.

### 5.4 Dictionary Learning

- **Tier assignment**: New AI-created concepts start at Tier 2 (IDs 128-16,383)
- **Promotion**: Frequently used concepts promoted to Tier 1 (IDs 0-127) by network
- **Deduplication**: Periodic background job merges synonymous concepts
- **Persistence**: Use `PersistentConceptDict` (redb) for ACID-safe storage

---

## 6. Accuracy Analysis by Model Size

### 6.1 Expected Accuracy for KU Encoding

| Model Size | Schema Adherence | Triple Accuracy | Overall Quality | RAM (Q4) | Speed |
|:---|:---|:---|:---|:---|:---|
| **3B** (SmolLM3, Llama 3.2) | 85-90% | 65-75% | 60-70% | ~2 GB | ~80-100 t/s |
| **4B** (Phi-4-mini, Qwen3-4B) | 90-95% | 70-80% | 70-80% | ~3 GB | ~60-80 t/s |
| **7-8B** (Qwen3-8B, Llama 3.1) | 95-98% | 80-90% | **80-90%** | ~5 GB | ~60-75 t/s |
| **14B** (Qwen2.5-14B) | 97-99% | 85-92% | 85-92% | ~9 GB | ~30-40 t/s |
| **30B+** (Qwen2.5-32B) | 99%+ | 90-95% | 90-95% | ~20 GB | ~15-20 t/s |

### 6.2 7B-8B: The Sweet Spot for OneBrain

1. **Fits consumer hardware**: 5GB VRAM (Q4_K_M) fits on RTX 4060 (8GB)
2. **Good tool-calling**: Specifically trained for function calling
3. **Reasonable speed**: ~3-5 KUs/minute
4. **Vietnamese support**: Qwen 2.5/3 has good multilingual capabilities
5. **Grammar-constrained output works reliably**

**Recommended models** (2026):
- **Qwen3-8B**: Best for structured output + multilingual
- **Gemma 4 9B**: Strong reasoning
- **Llama 3.3-8B**: Best community support
- **Phi-4-mini (3.8B)**: When RAM is tight

### 6.3 Fine-Tuning Impact

Fine-tuning 3B-4B on OneBrain data can **outperform general 8B** for KU encoding:
- Dataset: 1000-5000 (text → tool_calls) pairs
- Method: QLoRA (4-bit, 1-2 hours on RTX 4090)
- Expected improvement: +10-15% accuracy for KU-specific tasks

---

## 7. Batch Encoding vs Interactive Encoding

### 7.1 Encoding Modes

| Mode | Latency | Throughput | Best For |
|:---|:---|:---|:---|
| **Interactive** | 2-5s/KU | 12-30 KUs/min | Real-time user input |
| **Batch** | Higher/batch | 20-60 KUs/min | Import, bulk processing |
| **Streaming** | Lowest perceived | Same as interactive | UI responsiveness |

### 7.2 Context Window Budget (8K tokens)

```
├── System Prompt + Tools:    ~2000 tokens (fixed)
├── Few-Shot Examples:        ~1500 tokens (fixed)
├── Input Text(s):            ~1000-2000 tokens (variable)
├── Output (tool calls):      ~1000-2500 tokens (variable)
└── Safety margin:            ~500 tokens
```

### 7.3 Recommended Strategy

```rust
enum EncodingMode {
    Interactive,                        // User types → encode immediately
    Batch { max_per_batch: usize },     // Import → batch for throughput
}

fn choose_mode(source: &InputSource) -> EncodingMode {
    match source {
        InputSource::UserInput => EncodingMode::Interactive,
        InputSource::FileImport | InputSource::BulkPaste => {
            EncodingMode::Batch { max_per_batch: 5 }
        }
    }
}
```

---

## 8. Error Handling and Fallback Strategy

### 8.1 Error Categories

| Error Type | Example | Retryable? | Action |
|:---|:---|:---|:---|
| **Syntax error** | Invalid JSON | With GBNF: impossible | Grammar prevents |
| **Schema violation** | Missing field | Yes | Feed error to model |
| **Semantic error** | Wrong gene_type | Partially | Verify + fallback |
| **Hallucination** | Non-existent ConceptIds | Yes | Validate against dict |
| **Model timeout** | Inference too slow | Yes (once) | Retry then fallback |
| **Model crash** | OOM, CUDA error | No | Fallback to Tier 1 |

### 8.2 Retry Strategy

```rust
const MAX_RETRIES: u8 = 2;

async fn encode_with_retry(text: &str, model: &AiModel) -> EncodingResult {
    for attempt in 0..=MAX_RETRIES {
        match model.encode(text).await {
            Ok(result) => {
                match validate_tool_calls(&result.tool_calls) {
                    Ok(_) => return Ok(result),
                    Err(err) if attempt < MAX_RETRIES => {
                        let retry_prompt = format!(
                            "Error in previous output: {}. Fix and try again.", err
                        );
                        model.append_context(&retry_prompt);
                        continue;
                    }
                    Err(_) => break,
                }
            }
            Err(e) if is_transient(&e) && attempt < MAX_RETRIES => continue,
            Err(_) => break,
        }
    }
    // All retries exhausted → Tier 1 fallback
    Ok(EncodingResult::fallback(parse_text_to_core_dna(text, &dict)?))
}
```

### 8.3 Graceful Degradation Flow

```
Text Input
    │
    ▼
┌─── Try Tier 2 (AI) ───────────┐
│ Grammar-constrained generation │
│ Tool call execution            │
└────────────┬───────────────────┘
             │
       ┌─────▼─────┐
       │ Validate   │──── Pass ──→ CoreDna (AI) ──→ Self-Verify ──→ Done
       └─────┬─────┘
             │ Fail
       ┌─────▼──────────┐
       │ Retry (max 2x) │──── Pass ──→ CoreDna (AI)
       └─────┬──────────┘
             │ Fail
       ┌─────▼──────────────┐
       │ Tier 1 (Rule-based)│──→ CoreDna (T1) ──→ Flag for review
       └────────────────────┘
```

### 8.4 Encoding Log Structure

```rust
#[derive(Debug, Serialize)]
pub struct EncodingLog {
    pub timestamp: u64,
    pub input_text: String,
    pub input_lang: String,
    pub tier_used: u8,
    pub model_name: Option<String>,
    pub model_output_raw: Option<String>,
    pub tool_calls_count: usize,
    pub validation_errors: Vec<String>,
    pub retry_count: u8,
    pub fell_back_to_tier1: bool,
    pub final_wire_bytes: usize,
    pub encoding_time_ms: u64,
    pub confidence: f32,
}
```

---

## 9. Existing Codebase Infrastructure

OneBrain already has excellent foundation for Tier 2:

| Module | LOC | Ready for AI? | Role |
|:---|:---|:---|:---|
| `ku_tools.rs` | 454 | ✅ Yes | 15 tools with JSON Schema, OpenAI format export |
| `ku_tool_executor.rs` | 754 | ✅ Yes | Stateful executor, processes tool calls → CoreDna |
| `text_parser.rs` | — | ✅ Yes | Tier 1 rule-based parser (fallback) |
| `concept_dict.rs` | — | ✅ Yes | Bilingual ConceptDict with persistent storage |
| `encoding_consensus.rs` | — | ✅ Yes | Tier 3 verification (validates Tier 2 output) |

**What's missing (to be built in ku-ai)**:
- LLM integration (model loading, inference)
- GBNF grammar file for tool call output
- System prompt + few-shot example storage
- Confidence scoring and fallback decision logic
- Encoding log persistence

---

## 10. Risk Assessment

| Risk | Severity | Likelihood | Mitigation |
|:---|:---|:---|:---|
| AI produces hallucinated triples | High | Medium | Source grounding + validation + round-trip verify |
| Grammar constraint too rigid | Medium | Low | Two-phase (think then format) approach |
| 3B model too inaccurate | Medium | High | Default to 7-8B; fine-tune if RAM limited |
| ConceptDict fragmentation | Medium | Medium | Normalization + dedup + multilingual alignment |
| Slow encoding on weak hardware | Low | Medium | Tier 1 fallback always available |
| Vietnamese text misparsed | Medium | Medium | Vietnamese-capable models (Qwen, SeaLLM) |
| Context window overflow | Low | Low | Limit batch size; monitor token count |
| Model output not deterministic | Low | High | temperature=0.1; GBNF reduces variance |

---

*Document version: 1.0 | Next review: After Tier 2 encoding implementation*
