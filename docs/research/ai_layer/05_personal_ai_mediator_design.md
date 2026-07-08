# Personal AI Mediator — OneBrain Pillar 6

> **Research Date:** July 2026  
> **Author:** OneBrain Research Team  
> **Status:** Research Complete  
> **Scope:** Architecture design for the Personal AI Mediator — the primary human↔OneBrain interface  
> **Existing Infrastructure:** `KuToolExecutor` (754 LOC) + `ku_tools.rs` (454 LOC) + `ku_system_prompt.rs` (521 LOC) + KQL parser (ku-kql crate) + OBKG (13 modules, 280 tests) + Graph Embeddings (RotatE 64-dim)

---

## Executive Summary

The Personal AI Mediator is the "second brain" interface between humans and OneBrain. It handles intent classification, knowledge encoding from conversations, RAG-based retrieval from the local KU store, knowledge graph interaction via natural language → KQL translation, and adaptive user profiling — all running locally.

### Key Findings

| Finding | Detail |
|:---|:---|
| **Architecture** | 4-tier memory (Working → Core → Episodic → Archival) |
| **Intent classification** | 3-tier: keyword → embedding → LLM (Qwen3-8B: 92-95% accuracy) |
| **RAG pipeline** | Hybrid: embedding search + KQL structured query + graph traversal |
| **Knowledge detection** | Proactive/reactive/auto modes for conversation→KU encoding |
| **Rust RAG framework** | `rig` crate for orchestration + `ort`/`candle` for embeddings |
| **User profiling** | Local-only, privacy-preserving, adaptive behavior |
| **Multi-modal** | Text always, Voice (T1+), Image/OCR (T3+), PDF (T2+) |

---

## §1 Personal AI / Second Brain Architecture

### 1.1 Industry Landscape (2025-2026)

| Platform | Approach | Privacy | Local? |
|:---|:---|:---|:---|
| **Apple Intelligence** | On-device NPU + Private Cloud Compute | High | Hybrid |
| **Windows Recall** | Local screenshot + OCR + semantic search | Medium | Yes |
| **Obsidian + Local LLM** | Markdown vault + Ollama/llama.cpp | **Very High** | **Yes** |
| **Khoj** | Self-hosted personal AI | **Very High** | **Yes** |
| **Mem.ai / Notion AI** | Cloud-based AI | Low | No |

### 1.2 OneBrain Alignment

| Industry Pattern | OneBrain Equivalent |
|:---|:---|
| Markdown vault | CoreDna + Expression layer |
| Vector embeddings | RotatE graph embeddings (64-dim int8) |
| Knowledge graph | OBKG (13 modules, 280 tests) |
| AI agent | KuToolExecutor (15 tools) |
| Query language | KQL (SQL-like, nom parser) |
| Local inference | Ollama/llama.cpp via pluggable runtime |

---

## §2 Conversational Context Management

### 2.1 4-Tier Memory Architecture

```
┌─────────────────────────────────────────────────────┐
│  Tier 1: Working Context (~4K-8K tokens)             │
│  System prompt + current conversation + tool results │
├─────────────────────────────────────────────────────┤
│  Tier 2: Core Memory (~1K-2K tokens)                 │
│  User profile + active goals + recent concepts       │
├─────────────────────────────────────────────────────┤
│  Tier 3: Episodic Memory (Searchable)                │
│  Conversation summaries + interaction events         │
│  → Retrieved via semantic search                     │
├─────────────────────────────────────────────────────┤
│  Tier 4: Archival Memory (Long-term)                 │
│  Full logs + all KUs + OBKG graph state              │
│  → Storage: redb / SQLite                            │
└─────────────────────────────────────────────────────┘
```

### 2.2 Context Strategies Comparison

| Strategy | Token Cost | Retention | Latency | Best For |
|:---|:---|:---|:---|:---|
| **Sliding Window** | Fixed N turns | Recent only | Zero | Simple chat |
| **Recursive Summarization** | ~200-500 tokens | Good (lossy) | ~1-2s | Long conversations |
| **Structured State Object** | ~200-500 tokens | Excellent | ~0.5s | Goal-tracking |
| **RAG Injection** | Variable (top-K) | Excellent | ~0.5-1s | Knowledge queries |

### 2.3 Recommended: Hybrid Approach

```rust
/// Context budget for Qwen3-8B (8K context window)
struct ContextBudget {
    system_prompt: usize,    // ~2000 tokens (KU tools + rules)
    core_memory: usize,      // ~500 tokens (user profile + state)
    rag_results: usize,      // ~2000 tokens (retrieved KU context)
    conversation: usize,     // ~3000 tokens (sliding window)
    response_budget: usize,  // ~500 tokens (generation headroom)
}
```

### 2.4 Session Management

```rust
struct MediatorSession {
    session_id: SessionId,
    started_at: u64,
    mode: ConversationMode,
    history: VecDeque<Message>,
    summaries: Vec<ConversationSummary>,
    state: SessionState,
    encoded_kus: Vec<[u8; 32]>, // CIDs encoded this session
}

enum ConversationMode {
    Encoding,       // Storing knowledge
    Retrieval,      // Asking questions
    GraphExplore,   // Exploring connections
    Synthesis,      // Combining knowledge
    FreeChat,       // General conversation
}

struct SessionState {
    current_goal: Option<String>,
    established_facts: Vec<String>,
    active_concepts: Vec<ConceptId>,
    pending_encoding: Option<PendingKU>,
}
```

---

## §3 RAG for OneBrain

### 3.1 RAG Pipeline

```
User Query
    │
    ├──────────────────┐
    ▼                  ▼
 Embed Query       Extract Concepts
 (nomic-embed)     (pattern match)
    │                  │
    ▼                  ▼
 Vector Search     KQL Query
 (OBKG embeds)     (structured)
    │                  │
    └────────┬─────────┘
             ▼
         Merge & Re-rank
             │
             ▼
         Express (CoreDna → text)
             │
             ▼
         Inject into LLM context
             │
             ▼
         Generate answer with citations
```

### 3.2 Hybrid Retrieval

```rust
struct KuRetriever {
    embedding_index: EmbeddingIndex,  // RotatE graph embeddings
    kql_executor: LocalExecutor,      // ku-kql crate
    reranker: Option<CrossEncoder>,   // Optional for T3+
}

impl KuRetriever {
    async fn retrieve(&self, query: &str, top_k: usize) -> Vec<RetrievedKU> {
        // Path 1: Semantic search via embeddings
        let query_embedding = self.embed(query);
        let semantic_results = self.embedding_index.search(&query_embedding, top_k * 2);

        // Path 2: Structured KQL search
        let concepts = self.extract_concepts(query);
        let kql = format!(
            "FIND (k:KU) WHERE k.has_concept({}) ORDER BY k.trust_score DESC LIMIT {}",
            concepts.join(", "), top_k
        );
        let structured_results = self.kql_executor.execute(&kql);

        // Path 3: Graph traversal (2-hop related KUs)
        let graph_results = self.traverse_related(&semantic_results, 2);

        // Merge, deduplicate, re-rank
        self.merge_and_rerank(semantic_results, structured_results, graph_results, top_k)
    }
}
```

### 3.3 Chunking Strategy

KUs are already atomic — no traditional document chunking needed:

| Level | Unit | Use Case |
|:---|:---|:---|
| **Atomic** | Single KU | Direct retrieval |
| **Cluster** | KU + 1-hop bonds | Context enrichment |
| **Topic** | KU subgraph | Comprehensive answer |

### 3.4 Rust RAG Frameworks

| Framework | Language | OneBrain Fit |
|:---|:---|:---|
| **Rig** | **Rust** | ✅ Best fit — same language |
| **Candle** | **Rust** | ✅ Best for local embeddings |
| **ort** | Rust bindings | ✅ ONNX Runtime for GPU acceleration |

---

## §4 User Intent Classification

### 4.1 Intent Taxonomy

```rust
enum UserIntent {
    Encode { source: EncodeSource, trigger: EncodeTrigger },
    Retrieve { query_type: QueryType },
    Connect { source_concept: String, target_concept: Option<String> },
    Synthesize { topic: String, depth: SynthesisDepth },
    GraphQuery { nl_query: String },
    GraphExplore { start_ku: Option<[u8; 32]> },
    ManageProfile { action: ProfileAction },
    FreeChat { topic: Option<String> },
    Ambiguous { candidates: Vec<UserIntent> },
}
```

### 4.2 Three-Tier Classification

```
User Input
    │
    ▼
┌─────────────────────────┐
│ Tier 1: Keyword/Pattern  │  ← ~0ms, regex
│ "remember this" → Encode │
│ "what do I know" → Retrieve
└────────┬────────────────┘
         │ (no match)
         ▼
┌─────────────────────────┐
│ Tier 2: Embedding Router │  ← ~10ms, cosine similarity
│ Compare to intent        │
│ cluster centroids        │
└────────┬────────────────┘
         │ (confidence < 0.75)
         ▼
┌─────────────────────────┐
│ Tier 3: LLM Classification│ ← ~500ms-2s
│ Full model reasoning      │
└─────────────────────────┘
```

### 4.3 Model Performance for Intent Classification

| Model | Accuracy | Tool Calling | RAM |
|:---|:---|:---|:---|
| **Qwen3-8B** | **92-95%** | **Excellent** | ~5GB |
| **Gemma 4 12B** | 93-96% | Excellent | ~8GB |
| **Phi-4-mini 3.8B** | 80-85% | Decent | ~3GB |

---

## §5 Knowledge Graph Interaction via AI

### 5.1 Natural Language → KQL Translation

| User Pattern | KQL Template |
|:---|:---|
| "What do I know about X?" | `FIND (k:KU) WHERE k.has_concept(X) LIMIT 20` |
| "How does X relate to Y?" | `FIND (a:KU)-[r]->(b:KU) WHERE a.concept=X AND b.concept=Y` |
| "Most trusted about X?" | `FIND (k:KU) WHERE k.has_concept(X) ORDER BY k.trust_score DESC` |
| "Recent knowledge" | `FIND (k:KU) ORDER BY k.created_at DESC LIMIT 20` |
| "Needs verification?" | `FIND (k:KU) WHERE k.epistemic_status = 'Rumor'` |

### 5.2 Agentic Graph Interaction

```rust
struct GraphAgent {
    llm: LocalLLM,
    kql_executor: LocalExecutor,
    max_iterations: usize, // 3
}

impl GraphAgent {
    async fn answer(&self, question: &str) -> Answer {
        let mut context = GraphContext::new(question);
        for _ in 0..self.max_iterations {
            let kql = self.llm.generate_kql(question, &context);
            match parse_query(&kql) {
                Ok(ast) => {
                    let results = self.kql_executor.execute(ast);
                    context.add_results(results);
                    if context.is_sufficient() {
                        return self.llm.synthesize_answer(question, &context);
                    }
                }
                Err(e) => context.add_error(e), // Self-correction
            }
        }
        self.llm.synthesize_answer(question, &context) // Best-effort
    }
}
```

---

## §6 User Profiling and Personalization

### 6.1 Local User Profile

```rust
struct UserProfile {
    display_name: String,
    preferred_language: Language,
    expertise_areas: Vec<ExpertiseArea>,
    response_style: ResponseStyle,     // Concise/Balanced/Detailed/Academic
    detail_level: DetailLevel,         // Summary/Standard/Deep
    proactive_encoding: bool,          // AI suggests encoding?
    auto_connect: bool,                // Auto-discover KU connections?
    most_queried_concepts: Vec<(ConceptId, u32)>,
    total_kus_encoded: u64,
    device_tier: DeviceTier,
}

impl UserProfile {
    fn to_context_block(&self) -> String {
        format!(
            "## User Context\nLanguage: {}\nExpertise: {}\nStyle: {:?}\nKUs: {}",
            self.preferred_language,
            self.top_expertise_summary(),
            self.response_style,
            self.total_kus_encoded,
        )
    }
}
```

### 6.2 Privacy-Preserving Personalization

| Technique | OneBrain Application |
|:---|:---|
| On-device Learning | All profile data in local redb |
| Federated Concepts | Share concept popularity, not personal data |
| Differential Privacy | Add noise to shared statistics |
| Contextual Adaptation | System prompt includes expertise level |

---

## §7 Multi-Modal Knowledge Input

### 7.1 Input Pipeline

```
Text → direct                           (all tiers)
Voice → whisper.cpp → text              (T1+, 75MB-1.5GB)
Image → Qwen-VL / PaddleOCR → text     (T3+, 2-8GB)
PDF → Docling / Marker → text           (T2+, 500MB-2GB)
Web clip → readability + html2text      (all tiers, ~0 overhead)
    │
    ▼
Unified Text → Intent Classification → KU Encoding Pipeline
```

### 7.2 Availability by Device Tier

| Tier | Text | Voice | OCR/Image | PDF |
|:---|:---|:---|:---|:---|
| T0 (4GB) | ✅ | ❌ | ❌ | ✅ basic |
| T1 (8GB) | ✅ | ✅ tiny | ❌ | ✅ |
| T2 (16GB) | ✅ | ✅ base | ⚠️ basic | ✅ |
| T3+ (32GB+) | ✅ | ✅ large | ✅ VLM | ✅ |

---

## §8 Conversation-to-Knowledge Pipeline

### 8.1 Knowledge Extraction Flow

```
Conversation
    │
    ▼
Knowledge Signal Detection
    │ Explicit: "remember this", "save this"
    │ Implicit: "X is Y", "I learned that..."
    ▼
Candidate KU Extraction (LLM + KuToolExecutor)
    │
    ▼
Deduplication Check (embed → similarity search)
    │ > 0.85 → Skip (already known)
    │ 0.6-0.85 → Suggest update/merge
    │ < 0.6 → New knowledge
    ▼
Encoding Decision
    │ Proactive: AI asks "Shall I encode?"
    │ Reactive: User says "remember this"
    │ Auto: Encode silently, notify
    ▼
KU Encoding (KuToolExecutor → CoreDna)
```

### 8.2 Encoding Modes

| Mode | Behavior | User Type |
|:---|:---|:---|
| **Reactive** | User explicitly requests encoding | New users (default) |
| **Proactive** | AI detects and suggests | After user enables |
| **Auto** | AI silently encodes, notifies | Power users |

### 8.3 Correction Handling

CoreDna is **immutable** — corrections create a new KU with a `SUPERSEDES` bond:

```rust
async fn handle_correction(&mut self, original_cid: [u8; 32], correction: &str) {
    // 1. Create new KU with corrected knowledge
    let new_ku = self.encode_correction(correction)?;
    // 2. Deprecate old KU (lower trust, set Deprecated)
    // 3. Create SUPERSEDES bond in OBKG
    self.obkg.create_bond(new_ku.cid, original_cid, RelationType::Supersedes, BondWeight::Strong)?;
}
```

---

## §9 Overall Architecture

### 9.1 Component Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Personal AI Mediator (ku-mediator)                │
│                                                                      │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐ ┌────────────┐ │
│  │ Input Layer   │ │ Intent       │ │ Context      │ │ Output     │ │
│  │ Text/Voice/   │ │ Router       │ │ Manager      │ │ Formatter  │ │
│  │ Image/Doc/Web │ │ 3-tier class.│ │ 4-tier memory│ │ NL + graph │ │
│  └──────┬───────┘ └──────┬───────┘ └──────┬───────┘ └─────┬──────┘ │
│         └────────────────┴────────────────┴───────────────┘         │
│                              │                                       │
│  ┌───────────────────────────┴────────────────────────────────────┐  │
│  │                    Mediator Core Engine                         │  │
│  │  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐              │  │
│  │  │ KU Encoder  │ │ KU Retriever│ │ Graph Agent  │              │  │
│  │  │ (Executor)  │ │ (RAG)       │ │ (NL→KQL)    │              │  │
│  │  ├─────────────┤ ├─────────────┤ ├─────────────┤              │  │
│  │  │ Knowledge   │ │ Dedup       │ │ Synthesis   │              │  │
│  │  │ Detector    │ │ Engine      │ │ Engine      │              │  │
│  │  └─────────────┘ └─────────────┘ └─────────────┘              │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                      │
├──────────────────────────────────────────────────────────────────────┤
│  Infrastructure: LLM | Embedding | KQL Engine | OBKG | redb stores  │
└──────────────────────────────────────────────────────────────────────┘
```

### 9.2 Crate Structure

```
ku-mediator/
├── src/
│   ├── lib.rs
│   ├── mediator.rs         // Main Mediator struct
│   ├── intent.rs           // Intent classification (3-tier)
│   ├── context.rs          // Context management (4-tier memory)
│   ├── session.rs          // Session lifecycle
│   ├── retriever.rs        // RAG pipeline (hybrid)
│   ├── encoder.rs          // Conversation→KU bridge
│   ├── deduplicator.rs     // Knowledge deduplication
│   ├── detector.rs         // Knowledge signal detection
│   ├── graph_agent.rs      // NL→KQL→answer
│   ├── synthesizer.rs      // Knowledge synthesis
│   ├── profile.rs          // User profile
│   ├── input/              // Multi-modal input handlers
│   └── output/             // Response formatting
```

### 9.3 Main Interface

```rust
pub struct Mediator {
    config: MediatorConfig,
    llm: Box<dyn LLMProvider>,
    embedder: Box<dyn EmbeddingProvider>,
    tool_executor: KuToolExecutor,      // Existing
    kql_executor: LocalExecutor,         // Existing
    retriever: KuRetriever,              // New
    deduplicator: KnowledgeDeduplicator, // New
    session: MediatorSession,
    profile: UserProfile,
    router: MediatorRouter,
    graph_agent: GraphAgent,
    detector: KnowledgeDetector,
}

impl Mediator {
    pub async fn process(&mut self, input: UserInput) -> MediatorResponse {
        let text = self.input_to_text(input).await?;
        let intent = self.router.classify(&text);
        self.session.add_message(Role::User, &text);

        let response = match intent {
            UserIntent::Encode { .. } => self.handle_encode(&text).await,
            UserIntent::Retrieve { .. } => self.handle_retrieve(&text).await,
            UserIntent::Connect { .. } => self.handle_connect(&text).await,
            UserIntent::GraphQuery { .. } => self.handle_graph_query(&text).await,
            UserIntent::FreeChat { .. } => self.handle_chat(&text).await,
            _ => self.handle_general(&text).await,
        };

        if self.profile.proactive_encoding {
            if let Some(signal) = self.detector.detect(&text) {
                response.add_suggestion(EncodingSuggestion::new(signal));
            }
        }

        self.session.add_message(Role::Assistant, &response.text);
        self.profile.update_from_interaction(&intent);
        response
    }
}
```

---

## §10 Risk Assessment

| Risk | Severity | Mitigation |
|:---|:---|:---|
| LLM hallucination in KQL generation | High | Validation loops + constrained generation |
| Context window overflow (8K tokens) | Medium | Recursive summarization + aggressive pruning |
| Deduplication false positives | Medium | Human-in-the-loop confirmation |
| Intent misclassification | Low | 3-tier classification; fallback to FreeChat |
| Multi-modal latency | Medium | Device-tier adaptive: disable heavy modalities on T0-T1 |
| Profile data corruption | High | redb ACID transactions; periodic backups |

### Resource by Device Tier

| Tier | Can Run Mediator? | Limitations |
|:---|:---|:---|
| T0 (4GB) | ⚠️ Minimal | Text-only, no embedding, Tier 1 encoding only |
| T1 (8GB) | ✅ Basic | LLM OR embedding (not both), basic RAG |
| T2 (16GB) | ✅ Full | All features, moderate context |
| T3+ (32GB+) | ✅ Premium | Full multimodal, large context, fast inference |

---

*Document version: 1.0 | Next review: After ku-mediator implementation*
