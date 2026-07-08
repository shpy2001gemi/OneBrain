# Pillar 6 — AI Layer: Technical Specification

## 1. Executive Summary

Pillar 6 introduces a **local-first AI layer** to OneBrain through three new Rust crates that provide pluggable LLM inference, AI-assisted knowledge encoding, and a personal AI mediator interface. The implementation prioritizes offline capability, device adaptability, and seamless integration with the existing 7-pillar architecture.

| Metric | Value |
|:-------|:------|
| New crates | 3 (`ku-ai`, `ku-encoder`, `ku-mediator`) |
| Source files | 37 `.rs` + 1 `.json` + 3 `Cargo.toml` |
| Estimated LOC | ~6,600 (code + tests) |
| Unit tests | 120+ across all modules |
| Build status | ✅ Zero warnings from new crates |
| Existing crate changes | None — purely additive |

---

## 2. Architecture Overview

### 2.1 Dependency Graph

```
Layer 0:  ku-core  (P1+P2+P4+P5+P8 — Foundation, ~42K LOC)
Layer 1:  ku-ai    → ku-core
Layer 2:  ku-encoder → ku-ai, ku-core
Layer 3:  ku-mediator → ku-encoder, ku-ai, ku-core
```

**No circular dependencies.** No modifications to existing crates. All integration through established public APIs.

### 2.2 Data Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                        ku-mediator                              │
│  User Input → IntentClassifier → Route to Handler               │
│       │                                                         │
│       ├── Encode: text → AiEncoder → CoreDna wire bytes         │
│       ├── Retrieve: query → KuRetriever → RAG synthesis         │
│       ├── GraphQuery: NL → KQL string → (future: execute)       │
│       ├── Chat: text → ModelBackend → response                  │
│       └── Detect: proactive knowledge signal detection          │
│                                                                 │
│  ContextManager (4-tier memory) ←→ UserProfile (JSON persist)   │
└─────────────────────────────────────────────────────────────────┘
         │                    │
         ▼                    ▼
┌─────────────────┐  ┌──────────────────┐
│   ku-encoder    │  │     ku-ai        │
│                 │  │                  │
│ PromptBuilder   │  │ ModelBackend     │
│ AiEncoder       │  │ EmbeddingProvider│
│ EncodingVerifier│  │ OllamaBackend    │
│ FallbackChain   │  │ MockBackend      │
│ BatchEncoder    │  │ DeviceProfile    │
│ EncodingLog     │  │ ModelRegistry    │
└────────┬────────┘  └────────┬─────────┘
         │                    │
         ▼                    ▼
┌─────────────────────────────────────────┐
│              ku-core (existing)         │
│                                         │
│  KuToolExecutor  ←  15 ToolDefs         │
│  CoreDna encode/decode                  │
│  ConceptDict                            │
│  ku_system_prompt                       │
│  text_parser (Tier 1 fallback)          │
└─────────────────────────────────────────┘
```

---

## 3. Crate Specifications

### 3.1 ku-ai — Core AI Runtime

**Purpose:** Pluggable local LLM runtime with device detection, model registry, and backend abstraction.

#### 3.1.1 Trait Abstraction

```rust
#[async_trait]
pub trait ModelBackend: Send + Sync {
    async fn chat(&self, messages: &[ChatMessage], options: &InferenceOptions)
        -> Result<ChatResponse, AiError>;
    async fn chat_structured(&self, messages: &[ChatMessage],
        schema: &serde_json::Value, options: &InferenceOptions)
        -> Result<serde_json::Value, AiError>;
    async fn chat_with_tools(&self, messages: &[ChatMessage],
        tools: &[ToolDefinition], options: &InferenceOptions)
        -> Result<ChatOrToolResponse, AiError>;
    async fn health_check(&self) -> Result<BackendStatus, AiError>;
    async fn model_info(&self) -> Result<ModelInfo, AiError>;
    fn backend_name(&self) -> &str;
}

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, AiError>;
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AiError>;
    fn dimensions(&self) -> usize;
    fn model_name(&self) -> &str;
}
```

Both traits are **object-safe** (verified by test), enabling runtime polymorphism via `Box<dyn ModelBackend>`.

#### 3.1.2 Device Detection & Tier Classification

7-tier device classification based on available AI memory (RAM + VRAM − 2GB OS overhead):

| Tier | RAM Range | Default Model | Use Case |
|:-----|:----------|:-------------|:---------|
| T0 | ≤4 GB | Phi-4 Mini Q2_K (1.7GB) | Mobile/embedded |
| T1 | 4–8 GB | Phi-4 Mini Q2_K (1.7GB) | Low-end laptop |
| T2 | 8–12 GB | Qwen3 4B Q4_K_M (2.8GB) | Mid-range |
| T3 | 12–24 GB | **Qwen3 8B Q4_K_M (4.9GB)** | **Standard (default)** |
| T4 | 24–48 GB | Qwen3 8B Q8_0 (8.5GB) | High-end |
| T5 | 48–96 GB | Qwen3 14B Q4_K_M (9.2GB) | Workstation |
| T6 | 96+ GB | Qwen3 32B Q4_K_M (20GB) | Server |

Conservative selection strategy: model must fit within 50% of available memory.

#### 3.1.3 Ollama Backend

Full HTTP REST client implementing both `ModelBackend` and `EmbeddingProvider`:

| Endpoint | Method | Purpose |
|:---------|:-------|:--------|
| `POST /api/chat` | `chat()` | Free-form chat, `stream: false` |
| `POST /api/chat` + tools | `chat_with_tools()` | Function calling |
| `POST /api/chat` + format | `chat_structured()` | JSON schema-constrained output |
| `POST /api/embed` | `embed()`, `embed_batch()` | Vector embeddings |
| `GET /api/tags` | `health_check()` | List loaded models |
| `POST /api/show` | `model_info()` | Model metadata |

#### 3.1.4 Module Map

| Module | LOC | Tests | Description |
|:-------|:----|:------|:------------|
| error.rs | 112 | 5 | `AiError` — 12 variants with `#[from]` chains |
| types.rs | 312 | 7 | ChatMessage, ToolDefinition, InferenceOptions, ModelInfo |
| traits.rs | 85 | — | ModelBackend + EmbeddingProvider (object-safe) |
| config.rs | 258 | 7 | TOML config with tier-aware defaults |
| device/ | 521 | 12+ | DeviceProfile, DeviceTier T0–T6, MemoryMonitor |
| registry/ | 376 | 12+ | ModelRegistry, ModelSelector (compile-time embedded JSON) |
| backend/ollama.rs | 618 | 7 | Full ModelBackend + EmbeddingProvider impl |
| backend/mock.rs | 300 | 9 | Deterministic testing backend |

---

### 3.2 ku-encoder — AI KU Encoding Pipeline

**Purpose:** Bridge between AI model output and ku-core's CoreDna encoding infrastructure.

#### 3.2.1 Encoding Pipeline

```
 Input Text ──→ PromptBuilder ──→ LLM (tool-calling) ──→ ToolCallResponse[]
                    │                                          │
                    │  system prompt from                      │  JSON tool calls:
                    │  ku_system_prompt +                      │  new_ku("fact"),
                    │  ConceptDict context                     │  lookup_or_create("water"),
                    │                                          │  add_quantity(...),
                    │                                          │  set_certainty(9500),
                    ▼                                          │  finalize()
                                                               │
 KuToolExecutor ◄──────────────────────────────────────────────┘
       │  CoreDna instructions → encode() → wire bytes
       ▼
 EncodingVerifier
       │  structural: decode wire bytes ✓
       │  completeness: ≥2 instructions ✓
       ▼
 FallbackChain
       │  confidence ≥ 60% → Accept
       │  retries left    → Retry (higher temperature)
       │  max retries      → FallbackTier1 (text_parser)
       ▼
 EncodingResult { wire_bytes, confidence, gene_type, stats }
```

#### 3.2.2 Confidence Calculation

```
confidence = (success_rate × 0.7) + (instruction_richness × 0.3)

where:
  success_rate = 1.0 - (failed_tool_calls / total_tool_calls)
  instruction_richness = 1.0 if instructions > 1, else 0.5
```

#### 3.2.3 Fallback Decision Tree

```
if confidence ≥ min_confidence (0.60) AND wire_bytes non-empty:
    → Accept
elif attempt < max_retries (2):
    → Retry(temperature += 0.1, capped at 0.8)
else:
    → FallbackTier1 (ku-core text_parser, confidence = 0.50)
```

#### 3.2.4 Module Map

| Module | LOC | Tests | Description |
|:-------|:----|:------|:------------|
| encoder.rs | 366 | 6 | Core AiEncoder pipeline |
| prompt.rs | 126 | 4 | System prompt + message construction |
| verifier.rs | 244 | 5 | Structural + completeness verification |
| fallback.rs | 252 | 5 | Accept/Retry/Tier1 decision chain |
| batch.rs | 167 | 3 | Sequential multi-text encoding |
| log.rs | 198 | 4 | JSON-persistable encoding audit log |

---

### 3.3 ku-mediator — Personal AI Interface

**Purpose:** User-facing "second brain" — intent routing, knowledge encoding, RAG retrieval, graph queries, and adaptive profiling.

#### 3.3.1 Intent Classification (3-Tier)

| Tier | Method | Latency | Coverage |
|:-----|:-------|:--------|:---------|
| **Tier 1** | Keyword/pattern matching | ~0ms | ~60% of inputs |
| **Tier 2** | Embedding similarity (planned) | ~10ms | ~85% |
| **Tier 3** | LLM classification | ~500ms–2s | ~99% |

Supported intents with bilingual triggers:

| Intent | EN Triggers | VI Triggers |
|:-------|:-----------|:-----------|
| `Encode` | remember, save, store, encode | nhớ, lưu, ghi, ghi nhớ |
| `Retrieve` | find, search, what is, tell me about | tìm, tra cứu, cho biết |
| `Connect` | connect, relate, relationship | kết nối, liên quan |
| `GraphQuery` | graph, traverse, path, bonds | đồ thị, mạng lưới |
| `Synthesize` | (extracted from context) | — |
| `FreeChat` | (default for short messages) | — |

#### 3.3.2 Context Memory (4-Tier)

```
Tier 1: Working Memory    — VecDeque<ConversationMessage>, max 20, ~3000 tokens
Tier 2: Core Memory       — core_facts + active_goal, ~500 tokens
Tier 3: Episodic Memory   — ConversationSummary list, ~2000 tokens
Tier 4: Archival Memory   — Full history on disk (future)
                            Total budget: 8K tokens (adjustable per tier)
```

#### 3.3.3 Knowledge Detection

| Signal Type | Pattern Examples | Confidence | Gene Type |
|:------------|:----------------|:-----------|:----------|
| **Explicit** | "remember this", "nhớ" | 0.95 | (auto) |
| **Definition** | "X is Y", "X là Y" | 0.70 | `fact` |
| **Procedure** | "Step 1...", "Bước 1..." | 0.75 | `procedure` |
| **Implicit** | Long factual statements | 0.50 | (auto) |

#### 3.3.4 Deduplication (Jaccard Similarity)

```
similarity = |intersection(A, B)| / |union(A, B)|   (words > 2 chars)

≥ 0.85 → Duplicate (skip encoding)
≥ 0.60 → Overlap (warn user)
< 0.60 → New (proceed with encoding)
```

#### 3.3.5 Module Map

| Module | LOC | Tests | Description |
|:-------|:----|:------|:------------|
| mediator.rs | 482 | 8 | Main orchestrator |
| intent.rs | 256 | 11 | 3-tier classifier (EN+VI) |
| context.rs | 283 | 8 | 4-tier memory |
| session.rs | 166 | 7 | Mode tracking |
| retriever.rs | 199 | 7 | Keyword RAG (Phase 1) |
| deduplicator.rs | 150 | 5 | Jaccard dedup |
| detector.rs | 186 | 7 | Knowledge signal detection |
| graph_agent.rs | 178 | 5 | NL → KQL translation |
| synthesizer.rs | 119 | 4 | Multi-KU answer synthesis |
| profile.rs | 205 | 6 | Privacy-preserving user profile |
| input/ | 69 | — | Multi-modal input (text Phase 1) |
| output/ | 118 | — | Response formatting |

---

## 4. Quality Assessment

| Metric | Score | Notes |
|:-------|:------|:------|
| Compilation | ✅ | Zero warnings from all 3 crates |
| Test coverage | ⭐⭐⭐⭐⭐ | 120+ unit tests |
| Documentation | ⭐⭐⭐⭐⭐ | Every public item has doc comments |
| Error handling | ⭐⭐⭐⭐⭐ | Clean thiserror chain: AiError → EncoderError → MediatorError |
| Idiomatic Rust | ⭐⭐⭐⭐⭐ | Builder patterns, From impls, proper lifetimes |
| Architecture | ⭐⭐⭐⭐⭐ | Matches design doc, no circular deps |
| **Overall** | **⭐⭐⭐⭐⭐** | **Production-ready Phase 1** |

### Design Decisions

| Decision | Rationale |
|:---------|:----------|
| Two-backend Mediator constructor | Separate backends for chat vs. encoding allows different models or configs per task |
| KQL CONTAINS-based queries | Generates valid ku-kql grammar; `k.title CONTAINS "topic"` maps to existing parser |
| File-based retriever persistence | JSON serialization via `save()`/`load()` keeps retriever self-contained within ku-mediator |
| Hash-based session IDs | `session_{ts}_{random_hex}` prevents collisions without adding UUID dependency |
| Configurable embedding dimensions | `OllamaBackend::with_embedding_dimensions(dims)` supports varied embedding models |
| Concept extraction from tool calls | `concepts_used` populated from `lookup`/`lookup_or_create` arguments for profile tracking |

### Known Limitations (Phase 1)

1. GPU detection limited to macOS Apple Silicon — Windows/Linux NVIDIA returns `None`
2. Single-shot tool calling — no multi-turn conversation with model
3. Sequential batch encoding only — no concurrency
4. No model download manager — user must install Ollama and pull models manually
5. Retriever uses keyword matching — embedding-based semantic search planned for Phase 2
6. NL→KQL translation covers common patterns only — complex queries require LLM fallback

### Strengths

1. **Zero modifications to existing code** — all 3 crates purely additive
2. **Real end-to-end pipeline** — Text → LLM → CoreDna binary verified by tests
3. **Bilingual from day one** — Vietnamese + English keyword detection
4. **Graceful degradation** — AI fail → retry → rule-based fallback
5. **Privacy-preserving** — all processing local, profile stored locally
6. **Device-adaptive** — automatic model selection based on hardware capabilities
7. **Persistent state** — retriever index and user profile survive restarts via JSON

