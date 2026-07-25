# OneBrain v6 — Feature Details

> **Chi tiết kỹ thuật cho từng feature trong [Feature Tree](FEATURE_TREE.md).**  
> Mỗi feature bao gồm: Description, Technical Details, Current Status, Code References.

---

## §1 Knowledge Units — Kiến trúc 3 lớp

### §1.1 Core DNA — Layer 1

#### Description
Core DNA là lớp lưu trữ chính của Knowledge Unit — một chuỗi instruction nhị phân siêu nhỏ, hoàn toàn không phụ thuộc ngôn ngữ tự nhiên. Mỗi instruction gồm opcode (5 bit) + modifier (3 bit) + operands (varint ConceptIDs hoặc numeric literals).

#### Technical Details

**Wire Format:**
```text
MAGIC(0x4B) | VER_META(1B) | INSTRUCTION_STREAM | END(0x1E) | CRC-16(2B)

VER_META byte layout:
  bit 7-5: version (3 bits, current = 1)
  bit 4-1: gene_type (4 bits, 0-15)
  bit 0:   has_qualifiers flag
```

**Core structs:**

```rust
// core_dna.rs
pub struct CoreDna {
    pub header: CoreDnaHeader,
    pub instructions: Vec<Instruction>,
}

pub struct CoreDnaHeader {
    pub version: u8,       // 0-7
    pub gene_type: u8,     // 0-15, maps to GeneType enum
    pub has_qualifiers: bool,
}
```

**Instruction enum** — 31 typed variants:

```rust
// core_dna.rs — Instruction enum (selected variants)
pub enum Instruction {
    Triple { s: ConceptId, p: ConceptId, o: ConceptId },
    Quality { s: ConceptId, q: ConceptId },
    Quantity { s: ConceptId, value: NumericValue, unit: ConceptId },
    Sequence { items: Vec<ConceptId> },
    PartOf { part: ConceptId, whole: ConceptId },
    Causal { cause: ConceptId, effect: ConceptId },
    Step { ord: u8, action: ConceptId, target: ConceptId },
    Certainty { level: u16 },     // 0-10000
    CidRef { cid: [u8; 32] },     // BLAKE3 reference
    Affect { v: i16, a: i16, d: i16 },  // VAD emotion
    CompositeHdr { composite_type: u8, completeness: u8, version: u32 },
    Member { order: u16, role: u8, required: bool, label: ConceptId, cid: [u8; 32] },
    End,
    // ... 31 total variants
}
```

**NumericValue** — inline numeric literals:

```rust
// core_dna.rs
pub enum NumericValue {
    U8(u8), U16(u16), I16(i16),
    U32(u32), I32(i32), F32(f32),
}
```

**ConceptId** — varint-encoded semantic identifiers:

```rust
// types.rs
pub type ConceptId = u64;
// Tier 0 (1B): 0-127     → universal primitives
// Tier 1 (2B): 128-16K   → common concepts
// Tier 2 (3B): 16K-2M    → domain knowledge
// Tier 3 (4B): 2M-268M   → specialized terms
```

**Encode/Decode API:**

```rust
// Encode CoreDna → binary wire bytes
let wire_bytes = encode_core_dna(&dna)?;

// Decode binary → CoreDna
let dna = decode_core_dna(&wire_bytes)?;

// Full round-trip via KuRuntime
let runtime = KuRuntime::from_dna(dna)?;
let cid: [u8; 32] = runtime.cid;
```

#### Current Status
✅ **Implemented** — Full encode/decode pipeline, CRC-16 integrity, all 31 opcodes.

#### Code References
- [core_dna.rs](../../src/ku-core/src/core_dna.rs) — `CoreDna`, `CoreDnaHeader`, `Instruction`, `Op`, `encode_core_dna()`, `decode_core_dna()`
- [varint.rs](../../src/ku-core/src/varint.rs) — `encode_varint()`, `decode_varint()`
- [types.rs](../../src/ku-core/src/types.rs) — `ConceptId`, `GeneType`

---

### §1.2 Epigenetics — Layer 2

#### Description
Lớp metadata runtime — chứa tất cả thông tin có thể thay đổi mà KHÔNG ảnh hưởng đến CID (content identity) của KU. Lưu riêng biệt khỏi Core DNA.

Tương tự sinh học: các biến đổi epigenetic (methylation, histone modification) điều chỉnh cách gene biểu hiện mà không thay đổi trình tự DNA.

#### Technical Details

```rust
// epigenetics.rs
pub struct Epigenetics {
    pub trust: TrustSection,              // PoMV 6 signals + trust scores
    pub bonds: Vec<Bond>,                  // 33 relation types, 8 categories
    pub epistemic_status: EpistemicStatus, // 11 levels: Rumor → Axiomatic
    pub evidence_type: EvidenceType,       // 9 GRADE-aligned types
    pub epigenetic: Option<EpigeneticSection>, // embeddings, temporal, versioning
}
```

**TrustSection** — Core quality/reputation data:

```rust
// types.rs
pub struct TrustSection {
    pub epistemic_status: EpistemicStatus,
    pub evidence_type: EvidenceType,
    pub verification_level: u8,    // 0=none, 1=self, 2=peer, 3=expert, 4=formal
    pub corroboration_count: u16,
    pub challenge_count: u16,
    pub error_susceptibility: u16, // 16 independent flags (bitfield)
    pub trust_score: u16,          // [0, 10000] → [0.0, 1.0]
    pub confidence: u16,           // [0, 10000]

    // PoMV 6 signals (u16 each, scaled to [0, 10000])
    pub metabolic_rate: u16,
    pub prediction_score: u16,
    pub entropy_at_creation: u16,
    pub survival_score: u16,
    pub synaptic_centrality: u16,
    pub niche_fitness: u16,
}
```

**Bond** — 33 relation types across 8 categories:

```rust
// types.rs
pub struct Bond {
    pub target_cid: Vec<u8>,       // Target KU CID
    pub relation: RelationType,     // 33 types (A-H categories)
    pub weight: u16,                // [0, 10000]
    pub creator: Creator,           // Human | Ai | System | Hybrid
    pub created_at: u32,
    pub state: EdgeState,           // Active | Weakened | Deprecated
    pub decay: Option<DecayRate>,   // None | Slow | Med | Fast
    pub order: Option<u16>,         // Composite ordering
    pub required: Option<bool>,     // Structural integrity
    // ...
}
```

**RelationType** — 33 edge types:

| Category | Types | Hex Range |
|----------|-------|-----------|
| A: Epistemic | Extends, Supplements, Refutes, Corroborates, Supersedes, Qualifies | 0x01–0x06 |
| B: Structural | PartOf, InstanceOf, Specializes, Generalizes | 0x10–0x13 |
| C: Causal | Causes, Enables, Prevents, DependsOn | 0x20–0x23 |
| D: Derivation | ExampleOf, AnalogyOf, AppliesTo, DerivedFrom | 0x30–0x33 |
| E: Similarity | Duplicates, Translates, Paraphrases, Inspires | 0x40–0x43 |
| F: Temporal | Precedes, Cooccurs | 0x50–0x51 |
| G: Provenance | Cites, AuthoredBy, ReviewedBy | 0x60–0x62 |
| H: Experiential | ReactionTo, TestimonyAbout, FormallyProves, EvolvesInto, VariantOf, SensoryEvidenceFor, CulturallyContextualizes | 0x70–0x76 |

#### Current Status
✅ **Implemented** — Full Epigenetics struct with serialization, bond management, PoMV score computation.

#### Code References
- [epigenetics.rs](../../src/ku-core/src/epigenetics.rs) — `Epigenetics`, `Expression`
- [types.rs](../../src/ku-core/src/types.rs) — `TrustSection`, `Bond`, `RelationType`, `EpistemicStatus`, `EvidenceType`, `EpigeneticSection`

---

### §1.3 Expression — Layer 3

#### Description
Lớp render ngôn ngữ tự nhiên — tạo on-demand từ CoreDna + ConceptDict. Không lưu trữ, tái tạo khi cần. Cùng một CoreDna có thể render ra nhiều ngôn ngữ khác nhau.

Tương tự sinh học: gene expression tạo ra protein (phenotype) từ DNA.

#### Technical Details

```rust
// epigenetics.rs
pub struct Expression {
    pub text: String,                          // Rendered text
    pub lang: String,                          // "vi", "en", "ja", ...
    pub concept_names: Vec<(ConceptId, String)>, // Cached concept lookups
}
```

**Rendering flow:**
```text
CoreDna.instructions → ConceptDict.resolve(concept_id, lang) → Expression.text
```

**Example:**
```rust
// CoreDna with instructions:
//   Triple { s: 301, p: 1, o: 302 }  // "water" IS_A "liquid"
//   Quantity { s: 301, value: F32(100.0), unit: 10 }  // temp = 100°

// ConceptDict:
//   301 → {name_vi: "nước", name_en: "water"}
//   302 → {name_vi: "chất lỏng", name_en: "liquid"}

// Expression (vi): "Nước là chất lỏng. Nhiệt độ: 100°"
// Expression (en): "Water is liquid. Temperature: 100°"
```

#### Current Status
✅ **Implemented** — Expression struct and rendering via KuRuntime.

#### Code References
- [epigenetics.rs](../../src/ku-core/src/epigenetics.rs) — `Expression`
- [ku_runtime.rs](../../src/ku-core/src/ku_runtime.rs) — `KuRuntime::expression()`

---

### §1.4 KuRuntime — Unified 3-Layer Composite

#### Description
Struct thống nhất kết hợp cả 3 lớp vào 1 đối tượng queryable. Đây là struct chính được sử dụng bởi KQL, PoMV, và OBP network.

#### Technical Details

```rust
// ku_runtime.rs
pub struct KuRuntime {
    pub cid: [u8; 32],           // BLAKE3 hash of wire_bytes (immutable)
    pub dna: CoreDna,             // Layer 1: binary instructions
    pub epi: Epigenetics,         // Layer 2: runtime metadata
    pub expr: Option<Expression>, // Layer 3: generated text (lazy)
    pub wire_bytes: Vec<u8>,      // Canonical serialized form
}
```

**3-Layer Storage Model:**

| Layer | Field | Stored? | Format |
|-------|-------|---------|--------|
| 1. Core DNA | `dna` | ✅ Persistent | Custom binary (16–172B) |
| 2. Epigenetics | `epi` | ✅ Separate store | CBOR/SQLite |
| 3. Expression | `expr` | ❌ Generated | On-demand text |

**Factory methods:**
```rust
let rt = KuRuntime::from_wire(wire_bytes)?;  // Decode from binary
let rt = KuRuntime::from_dna(dna)?;          // Encode, then build
```

#### Current Status
✅ **Implemented** — Full lifecycle including encode/decode, CID computation, expression rendering.

#### Code References
- [ku_runtime.rs](../../src/ku-core/src/ku_runtime.rs) — `KuRuntime`, `ExtractedValue`
- [ku_lifecycle.rs](../../src/ku-core/src/ku_lifecycle.rs) — `KuLifecycle`

---

## §2 Knowledge Input — Pipeline 3 tầng

### §2.1 Tier 1: Rule-Based Parser

#### Description
Parser pattern-matching thuần túy chuyển đổi text tiếng Việt và tiếng Anh thành CoreDna instructions. Không cần AI model — hoạt động offline 100%.

#### Technical Details

**Well-known ConceptIds (Tier 0):**

| ID | Constant | Meaning |
|----|----------|---------|
| 1 | `IS_A` | "is a" / "là" |
| 2 | `HAS_PART` | "has part" / "gồm" |
| 3 | `RELATED_TO` | Generic relation |
| 10 | `UNIT_DEGREE` | ° (degree) |
| 11 | `UNIT_METER` | m (meter) |
| 14 | `UNIT_PERCENT` | % |
| 127 | `UNKNOWN_CONCEPT` | Fallback |

**Supported patterns:**

| Pattern (VI) | Pattern (EN) | Generated Instruction |
|---|---|---|
| `"X là Y"` | `"X is Y"` | `Triple(X, IS_A, Y)` |
| `"X gồm A, B"` | `"X consists of A, B"` | `PartOf(A, X), PartOf(B, X)` |
| `"Bước N: action target"` | `"Step N: action target"` | `Step(N, action, target)` |
| `"= 35.2°"` | `"= 35.2°"` | `Quantity(S, F32(35.2), UNIT_DEGREE)` |
| `"± 0.1"` | `"± 0.1"` | `Tolerance(S, value, delta)` |

**Accuracy target:** ~60–70% (refined by Tier 2 AI and Tier 3 P2P).

> [!NOTE]
> **Encoding Status**: Sau khi Tier 1 parse text thành CoreDna, KU được gán `encoding_status = SELF (0x01)` — đánh dấu đã hoàn tất local encoding. KU ở trạng thái SELF có thể được submit lên mạng P2P để refine qua Tier 3 Encoding Consensus.

#### Current Status
✅ **Implemented** — Vietnamese + English pattern matching.

#### Code References
- [text_parser.rs](../../src/ku-core/src/text_parser.rs) — pattern matching engine, well-known ConceptIds

---

### §2.2 Tier 2: AI-Assisted (Local Models)

#### Description
AI local model phân tách text thành CoreDna instructions thông qua tool-calling interface. Hỗ trợ Gemma4, Qwen, Phi-3 — chạy 100% trên máy local.

#### Technical Details

**KQL syntax:**
```
CREATE FROM TEXT "Nước sôi ở 100 độ C ở áp suất tiêu chuẩn"
WITH AI model="gemma4"
SIGNED BY "node_abc"
```

**AST type:**
```rust
// ast.rs
pub struct CreateFromTextQuery {
    pub text: String,         // Natural language input
    pub model: String,        // "gemma4", "qwen", "phi-3"
    pub gene_hint: Option<KqlGeneType>,  // Optional type hint
    pub signed_by: String,
}
```

**Tool-calling flow:**
```text
User text → ku_system_prompt (build prompt)
         → AI model (tool calls)
         → ku_tool_executor (execute tool calls)
         → CoreDna instructions
```

> [!NOTE]
> **Encoding Status**: Sau khi Tier 2 AI decompose text thành CoreDna, KU được gán `encoding_status = SELF (0x01)` — tương tự Tier 1. AI encoding cũng là local encoding, chưa qua P2P verification.

#### Current Status
✅ **Implemented** — Tool definitions, executor, system prompt generation.

#### Code References
- [ku_tools.rs](../../src/ku-core/src/ku_tools.rs) — Tool definitions
- [ku_tool_executor.rs](../../src/ku-core/src/ku_tool_executor.rs) — Execution engine
- [ku_system_prompt.rs](../../src/ku-core/src/ku_system_prompt.rs) — Prompt generation
- [ast.rs](../../src/ku-kql/src/ast.rs) — `CreateFromTextQuery`

---

### §2.3 Tier 3: Distributed Encoding Consensus

#### Description
Mạng ngang hàng refine CoreDna thông qua **encoding consensus** phân tán. Nhiều node cùng verify và cải thiện chất lượng encoding, đảm bảo CoreDna chính xác và nhất quán trên toàn mạng.

#### Technical Details

**EncodingStatus** — State machine vòng đời encoding:

```rust
// encoding_consensus.rs
#[repr(u8)]
pub enum EncodingStatus {
    Raw  = 0x00,  // Vừa tạo, chưa qua encoding
    Self_ = 0x01, // Đã encode local (Tier 1 hoặc Tier 2)
    Part = 0x02,  // Đang P2P verification, chưa đủ consensus
    Full = 0x03,  // Consensus đạt — IMMUTABLE, không thể sửa
}
```

> [!IMPORTANT]
> **FULL = Immutable**: Khi KU đạt `EncodingStatus::Full`, encoding được coi là bất biến. Mọi thay đổi yêu cầu tạo KU mới (new RAW) với CID mới. Đây là nguyên tắc cốt lõi đảm bảo tính toàn vẹn của mạng.

**EncodingSubmission** — Struct gửi encoding lên mạng:

```rust
// encoding_consensus.rs
pub struct EncodingSubmission {
    pub ku_cid: [u8; 32],              // CID của KU gốc
    pub proposed_dna: CoreDna,          // CoreDna đề xuất
    pub verifier_node_id: NodeId,       // Node thực hiện encoding
    pub phase_a_result: DecompositionResult, // Kết quả Phase A
    pub phase_b_result: RoundTripResult,     // Kết quả Phase B
    pub timestamp: u64,
    pub signature: [u8; 64],           // Ed25519 signature
}
```

**ConsensusConfig** — Cấu hình consensus:

```rust
// encoding_consensus.rs
pub struct ConsensusConfig {
    pub min_verifiers: u8,           // Tối thiểu verifiers cần thiết (default: 2)
    pub max_verifiers: u8,           // Tối đa verifiers per job (capped: 3)
    pub agreement_threshold: f32,    // Ngưỡng agreement để đạt FULL (default: 0.67)
    pub similarity_threshold: f32,   // Ngưỡng similarity giữa các submissions (default: 0.80)
    pub claim_ttl_secs: u64,         // TTL của ClaimToken (default: 300s)
    pub reward_obt: u64,             // OBT reward per verification
}
```

**2-Phase Verification:**

```text
┌─────────────────────────────────────────────────────────────────┐
│ Phase A: AI Decomposition Agreement                             │
│                                                                 │
│  Verifier chạy AI decompose text → CoreDna độc lập              │
│  So sánh kết quả với proposed_dna:                              │
│    - Instruction-level diff                                     │
│    - Concept mapping alignment                                  │
│    - Structural similarity score                                │
│  Output: agreement_score ∈ [0.0, 1.0]                          │
├─────────────────────────────────────────────────────────────────┤
│ Phase B: Tool Encoding Round-trip                               │
│                                                                 │
│  encode(CoreDna) → wire_bytes → decode(wire_bytes) → CoreDna'  │
│  Verify: CoreDna == CoreDna' (lossless round-trip)              │
│  Verify: CRC-16 integrity                                      │
│  Verify: All ConceptIds resolve in ConceptDict                  │
│  Output: pass/fail                                              │
└─────────────────────────────────────────────────────────────────┘
```

```rust
// encoding_verifier.rs
pub struct VerificationResult {
    pub phase_a: DecompositionResult,
    pub phase_b: RoundTripResult,
    pub overall_score: f32,
}

pub struct DecompositionResult {
    pub agreement_score: f32,         // [0.0, 1.0]
    pub instruction_overlap: f32,     // % instructions trùng khớp
    pub concept_alignment: f32,       // % concepts mapped đúng
}

pub struct RoundTripResult {
    pub passed: bool,                 // Encode → decode lossless?
    pub crc_valid: bool,              // CRC-16 integrity
    pub all_concepts_resolved: bool,  // Tất cả ConceptIds hợp lệ
}
```

**Stigmergy-Based Job Selection:**

Sử dụng pheromone routing từ OBP Layer 5 để phân phối encoding jobs:

```rust
// encoding_stigmergy.rs
pub struct EncodingPheromone {
    pub job_cid: [u8; 32],            // CID của encoding job
    pub domain_niche: NicheId,        // Domain expertise cần thiết
    pub concentration: f32,           // Pheromone concentration [0.0, 1.0]
    pub evaporation_rate: f32,        // Tốc độ bay hơi (default: 0.95/hour)
}
```

- Node chọn job dựa trên: **pheromone concentration × domain expertise match**
- Pheromone evaporation tự nhiên → tránh overload, cân bằng tải
- High-demand jobs có concentration cao → thu hút nhiều verifiers hơn
- Job đã đủ verifiers → pheromone giảm nhanh → tránh stampede

**DHT Job Board — EncodingJob:**

```rust
// encoding_job.rs
pub struct EncodingJob {
    pub ku_cid: [u8; 32],             // KU cần refine
    pub source_node: NodeId,          // Node đăng job
    pub current_status: EncodingStatus,
    pub reward_obt: u64,              // OBT reward
    pub created_at: u64,
    pub claims: Vec<ClaimToken>,      // Active claims (max 3)
    pub submissions: Vec<EncodingSubmission>,
}

pub struct ClaimToken {
    pub claimer: NodeId,              // Node claim job
    pub claimed_at: u64,
    pub ttl_secs: u64,                // Hết hạn → job quay lại pool
    pub nonce: [u8; 16],              // Anti-replay
}
```

- Jobs được publish lên DHT theo `ku_cid` key
- **ClaimToken** = anti-stampede mechanism: mỗi node claim trước khi verify
- TTL hết → claim tự hủy, job available cho node khác
- Capped tối đa **3 verifiers** per job (threshold)

**Consensus Weighted Scoring:**

```text
Consensus Score = 0.50 × agreement + 0.30 × detail + 0.20 × reputation

Trong đó:
  agreement  = Tỷ lệ verifiers đồng ý (Phase A agreement_score trung bình)
  detail     = Mức chi tiết CoreDna (instruction count, concept coverage)
  reputation = Uy tín verifier (lịch sử encoding accuracy, OBT stake)
```

```rust
// encoding_consensus.rs
pub struct ConsensusScore {
    pub agreement: f32,    // weight: 0.50
    pub detail: f32,       // weight: 0.30
    pub reputation: f32,   // weight: 0.20
    pub total: f32,        // Weighted sum
}

impl ConsensusScore {
    pub fn compute(agreement: f32, detail: f32, reputation: f32) -> Self {
        let total = 0.50 * agreement + 0.30 * detail + 0.20 * reputation;
        Self { agreement, detail, reputation, total }
    }
}
```

**OBT Rewards:**

```rust
// encoding_reward.rs
pub struct EncodingReward {
    pub verifier: NodeId,
    pub ku_cid: [u8; 32],
    pub obt_amount: u64,              // OBT tokens earned
    pub quality_multiplier: f32,      // Bonus cho high-quality encoding
    pub timestamp: u64,
}

pub struct RewardDistributor;
impl RewardDistributor {
    /// Phân phối OBT reward cho verifiers sau khi consensus đạt FULL
    pub fn distribute(
        job: &EncodingJob,
        scores: &[ConsensusScore],
        config: &ConsensusConfig,
    ) -> Vec<EncodingReward>;
}
```

- Verifier hoàn tất verification → nhận **OBT tokens**
- Reward tỷ lệ thuận với `quality_multiplier` (dựa trên consensus score)
- Encoding chất lượng cao (agreement > 0.9) → bonus multiplier

#### Current Status
🔧 **Designed** — Kiến trúc hoàn chỉnh, đang triển khai. Phụ thuộc OBP Network (§7) cho DHT và gossip layer.

#### Code References
- [encoding_consensus.rs](../../src/ku-core/src/encoding_consensus.rs) — `EncodingStatus`, `EncodingSubmission`, `ConsensusConfig`, `ConsensusScore`
- [encoding_verifier.rs](../../src/ku-core/src/encoding_verifier.rs) — `VerificationResult`, `DecompositionResult`, `RoundTripResult`
- [encoding_reward.rs](../../src/ku-core/src/encoding_reward.rs) — `EncodingReward`, `RewardDistributor`
- [encoding_job.rs](../../src/ku-net/src/encoding_job.rs) — `EncodingJob`, `ClaimToken`
- [encoding_gossip.rs](../../src/ku-net/src/encoding_gossip.rs) — Job status gossip protocol
- [encoding_stigmergy.rs](../../src/ku-net/src/encoding_stigmergy.rs) — `EncodingPheromone`, pheromone-based selection

---

## §3 PoMV v2 — Proof of Metabolic Value

> **Capability boundary (P0, 2026-07-25):** các trường `pomv`,
> `pomv_breakdown` và aggregate `avg_pomv` hiện là
> `legacy_local_pomv_scalar_v1`. Chúng là metric compatibility cục bộ,
> non-economic; không phải vNext Metabolic Evidence View, UseEvidence,
> Outcome, Benefit, authority, reward hoặc tuyên bố toàn mạng. Query/retrieval
> counters bên dưới không tự tạo Public UseEvidence.

### §3.1 Signal 1: Metabolism

#### Description
Tín hiệu chính (weight: 0.35) — đo lường usage thực tế của KU thông qua CRDT GCounters. Mỗi sự kiện (query, retrieval, citation) được ghi nhận như counter tăng đơn điệu.

#### Technical Details

```rust
// metabolism.rs
pub struct KUMetabolism {
    // Consumption
    pub query_hits: GCounter,         // Lần xuất hiện trong query results
    pub retrieval_count: GCounter,    // Lần đọc đầy đủ
    pub dwell_time_ms: GCounter,      // Tổng thời gian đọc (ms)

    // Transformation
    pub citation_count: GCounter,     // Lần bị trích dẫn
    pub derivative_count: GCounter,   // Lần tạo cảm hứng cho KU mới
    pub refutation_count: GCounter,   // Phản bác (tín hiệu TÍCH CỰC!)
    pub corroboration_count: GCounter, // Xác nhận (optional)

    // Excretion
    pub downstream_usage: GCounter,   // Usage của downstream KUs

    // Temporal
    pub created_at: u64,
    pub last_activity: LWWRegister<u64>,
    pub unique_nodes: GCounter,
}
```

**Events:**
```rust
pub enum MetabolismEvent {
    QueryHit,
    Retrieval { dwell_ms: u64 },
    Citation,
    Derivative,
    DownstreamUsage,
    Corroboration,
    Refutation,    // Positive engagement!
}
```

**Metabolic rate formula:**
```text
rate = (α₁×query_velocity + α₂×retrieval_depth + α₃×citation_fresh +
        α₄×derivative_novelty + α₅×downstream_cascade) × decay(t)

decay(t) = 2^(-Δt / half_life)
```

**Default weights:** α₁=0.25, α₂=0.20, α₃=0.25, α₄=0.15, α₅=0.15  
**Default half-life:** 30 days

#### Current Status
✅ **Implemented** — Full CRDT metabolism tracking with exponential decay.

#### Code References
- [metabolism.rs](../../src/ku-core/src/metabolism.rs) — `KUMetabolism`, `MetabolismEvent`
- [metabolism_store.rs](../../src/ku-core/src/metabolism_store.rs) — `MetabolismStore`
- [crdt.rs](../../src/ku-core/src/crdt.rs) — `GCounter`, `LWWRegister`

---

### §3.2 Signal 2: Prediction Accuracy

#### Description
Tri thức có khả năng dự đoán đúng là tri thức có giá trị khách quan. Mỗi KU có thể đăng ký predictions và được giải quyết theo thời gian.

#### Technical Details

```rust
// prediction.rs
pub enum ResolutionMethod {
    TemporalConsistency,  // Fact: still true over time?
    UsageOutcome,         // Procedure: users report success?
    CrossReference,       // Hypothesis: new evidence confirms?
    NoResolution,         // Experience: pure metabolism
}

pub struct Prediction {
    pub predicate_hash: [u8; 32],      // BLAKE3
    pub deadline: u64,
    pub resolution_method: ResolutionMethod,
    pub registered_at: u64,
}

pub enum PredictionOutcome {
    Confirmed,
    Refuted,
    Partial { confidence: u16 },
    Inconclusive,
}
```

**Score:** `prediction_score = correct / total × confidence`

#### Current Status
✅ **Implemented** — `PredictionRegistry` with registration and resolution.

#### Code References
- [prediction.rs](../../src/ku-core/src/prediction.rs) — `PredictionRegistry`, `Prediction`, `ResolutionMethod`, `PredictionOutcome`

---

### §3.3 Signal 3: Entropy (Novelty)

#### Description
KU mới lạ (lấp lỗ trống kiến thức) nhận cold-start boost. Đo bằng cosine distance trên int8 embeddings.

#### Technical Details

```rust
// entropy.rs
pub struct EntropyCalculator;  // Stateless

impl EntropyCalculator {
    // Novelty: [0.0, 1.0] — average cosine distance from K neighbors
    pub fn novelty_score(new_embedding: &[u8], neighbors: &[&[u8]]) -> f32;

    // Bridge: connects disparate clusters
    pub fn bridge_score(lsh_buckets: &[u8], neighbor_buckets: &[&[u8]]) -> f32;
}
```

**Constants:**
- Decay period: 7 days (`ENTROPY_DECAY_PERIOD_SECS`)
- Novelty weight: 0.6 / Bridge weight: 0.4
- Min embedding length: 8 bytes

#### Current Status
✅ **Implemented** — Stateless calculator, no internal state.

#### Code References
- [entropy.rs](../../src/ku-core/src/entropy.rs) — `EntropyCalculator`

---

### §3.4 Signal 4: Survival (Anti-fragile Immune)

#### Description
Hệ miễn dịch phát hiện spam/manipulation dựa trên PATTERN lan truyền, không phải nội dung. KU sống sót qua tấn công trở nên MẠNH HƠN.

#### Technical Details

```rust
// immune.rs
pub enum AntibodyType {
    TemporalBurst,        // Quá nhiều bản sao quá nhanh (bot-like)
    SourceConcentration,  // >80% từ cùng 1 cluster nguồn
    LowEngagement,        // Sao chép nhiều nhưng ít sử dụng
    DiversityDeficit,     // Chỉ lan trong các node tương tự
}
```

**Thresholds:**
| Detection | Threshold |
|-----------|-----------|
| Temporal burst | > 50 replications/hour |
| Source concentration | > 80% from same cluster |
| Engagement ratio | < 5% usage/replication |
| Diversity deficit | < 10% unique sources |

**Anti-fragile mechanism:**
```text
survival_score += SURVIVAL_BONUS (0.1) per confirmed attack survived
max(survival_score) = 1.0
```

**Privacy:** antibodies gossip `pattern_hash` only — NEVER NodeID/PII.

#### Current Status
✅ **Implemented** — `ImmuneEngine` with 4 antibody types.

#### Code References
- [immune.rs](../../src/ku-core/src/immune.rs) — `ImmuneEngine`, `AntibodyType`
- [spread_analysis.rs](../../src/ku-core/src/spread_analysis.rs) — Pattern analysis

---

### §3.5 Signal 5: Synaptic Centrality

#### Description
Hebbian learning: "Neurons that fire together, wire together." KU được co-retrieve hoặc co-cite sẽ tăng bond strength. Centrality tính bằng PageRank-like algorithm.

#### Technical Details

```rust
// synaptic.rs
pub enum BondReason {
    CoRetrieval,  // Cùng session
    CoCitation,   // Cùng được trích dẫn bởi KU_C
}

// Constants
pub const INITIAL_CO_RETRIEVAL_STRENGTH: f32 = 0.1;
pub const INITIAL_CO_CITATION_STRENGTH: f32 = 0.15;
pub const REINFORCE_INCREMENT: f32 = 0.05;
pub const MAX_BOND_STRENGTH: f32 = 1.0;
pub const EVAPORATION_RATE: f32 = 0.95;  // per day
pub const CENTRALITY_DAMPING: f32 = 0.85;  // PageRank damping
pub const MAX_BONDS_PER_KU: usize = 100;
```

#### Current Status
✅ **Implemented** — `SynapticMap`, `CentralityCalculator`.

#### Code References
- [synaptic.rs](../../src/ku-core/src/synaptic.rs) — `SynapticMap`, `CentralityCalculator`, `BondReason`

---

### §3.6 Signal 6: Niche Fitness

#### Description
Đo mức đóng góp giá trị của KU trong hệ sinh thái domain. Lấy cảm hứng từ sinh thái học: đa dạng hệ sinh thái = khỏe mạnh hơn.

#### Technical Details

```rust
// ecosystem.rs
pub type NicheId = u64;

pub struct NicheStats {
    pub population: usize,           // Số KU trong niche
    pub total_metabolic_rate: f64,   // Tổng metabolism
    pub avg_metabolic_rate: f64,     // Trung bình
    pub source_diversity: usize,     // Nguồn đa dạng
}

pub struct KUNicheProfile {
    pub niches: Vec<NicheId>,        // Niche chính của KU
    pub metabolic_rate: f64,
    pub uniqueness: f32,
    pub cross_niche_count: usize,
}
```

**Fitness formula:**
```text
fitness = w₁×density_score + w₂×uniqueness + w₃×bridge + w₄×metabolic_share
Weights: 0.25, 0.30, 0.20, 0.25
```

#### Current Status
✅ **Implemented** — `EcosystemAnalyzer` with 4-component scoring.

#### Code References
- [ecosystem.rs](../../src/ku-core/src/ecosystem.rs) — `EcosystemAnalyzer`, `NicheStats`, `KUNicheProfile`

---

### §3.7 PoMV Aggregator

#### Description
Kết hợp 6 tín hiệu thành 1 score duy nhất với trọng số có thể cấu hình.

#### Technical Details

```rust
// pomv.rs
pub struct PomvSignals {
    pub metabolism: f32,     // [0.0, 1.0]
    pub prediction: f32,
    pub entropy: f32,
    pub survival: f32,
    pub synaptic: f32,
    pub niche_fitness: f32,
}

pub struct PomvWeights {
    pub metabolism: f32,     // default: 0.35
    pub prediction: f32,     // default: 0.15
    pub entropy: f32,        // default: 0.10
    pub survival: f32,       // default: 0.10
    pub synaptic: f32,       // default: 0.15
    pub niche_fitness: f32,  // default: 0.15
}

pub struct PomvScore {
    pub total: f32,                      // Final [0.0, 1.0]
    pub contributions: PomvContributions, // Per-signal breakdown
    pub weights: PomvWeights,
}

// Stateless calculator
pub struct PomvCalculator;
impl PomvCalculator {
    pub fn compute(signals: &PomvSignals, weights: &PomvWeights) -> PomvScore;
}
```

#### Current Status
✅ **Implemented** — Stateless, configurable weights.

#### Code References
- [pomv.rs](../../src/ku-core/src/pomv.rs) — `PomvCalculator`, `PomvSignals`, `PomvScore`, `PomvWeights`, `PomvContributions`

---

### §3.8 PoMV Runtime & KU Lifecycle

#### Description
Orchestrator chạy "tick" trên mỗi node: accept events → compute signals → update TrustSection → run epistemic transitions → run immune analysis.

#### Technical Details

```rust
// pomv_runtime.rs
pub struct PomvConfig {
    pub weights: PomvWeights,
    pub half_life_secs: u64,
    pub entropy_decay_secs: u64,
    pub node_id: u64,
}

pub struct KUPomvState {
    pub predictions: PredictionRegistry,
    pub synaptic: SynapticMap,
    pub entropy_at_creation: f32,
    pub bridge_at_creation: f32,
    pub created_at: u64,
    pub attacks_survived: u32,
    pub niches: Vec<NicheId>,
    pub epistemic_status: EpistemicStatus,
}

// ku_lifecycle.rs
pub struct KuLifecycle {
    pub kus: HashMap<[u8; 32], KuRuntime>,
    pub pomv: PomvRuntime,
}
```

#### Current Status
✅ **Implemented** — Full lifecycle: ingest → events → tick → GC.

#### Code References
- [pomv_runtime.rs](../../src/ku-core/src/pomv_runtime.rs) — `PomvRuntime`, `PomvConfig`, `KUPomvState`
- [ku_lifecycle.rs](../../src/ku-core/src/ku_lifecycle.rs) — `KuLifecycle`

> [!NOTE]
> **Encoding Consensus OBT Rewards** are handled separately in [encoding_reward.rs](../../src/ku-core/src/encoding_reward.rs) — compensating AI compute for encoding verification. Contributors (knowledge providers) are rewarded through PoMV lifecycle, not encoding rewards. The two systems are parallel and independent.

---

## §4 Epistemic Ladder — 11 bậc nhận thức

#### Description
Thang 11 bậc nhận thức dựa trên tín hiệu observable, không cần human judgment. Mỗi transition có threshold rõ ràng từ CRDT counters.

#### Technical Details

```rust
// types.rs
pub enum EpistemicStatus {
    Rumor          = 0x00,
    Hearsay        = 0x01,
    Testimony      = 0x02,
    Observation    = 0x03,
    Hypothesis     = 0x04,
    Evidence       = 0x05,
    Corroborated   = 0x06,
    PeerReviewed   = 0x07,
    Consensus      = 0x08,
    FormallyProven = 0x09,
    Axiomatic      = 0x0A,
}

// 9 evidence types (Cochrane/GRADE aligned)
pub enum EvidenceType {
    None, Anecdotal, CaseStudy, Observational,
    Correlational, Experimental, MetaAnalysis,
    FormalProof, Computational,
}
```

**Transition function:**
```rust
// epistemic_engine.rs
pub fn evaluate_transition(
    current: EpistemicStatus,
    metabolism: &KUMetabolism,
    now: u64,
    half_life_secs: u64,
) -> Option<EpistemicStatus>
```

**Properties:** Monotonic, Observable, Local, Deterministic.

#### Current Status
✅ **Implemented** — Full engine with 10 transition thresholds.

#### Code References
- [epistemic_engine.rs](../../src/ku-core/src/epistemic_engine.rs) — `evaluate_transition()`
- [types.rs](../../src/ku-core/src/types.rs) — `EpistemicStatus`, `EvidenceType`

---

## §5 KQL — Knowledge Query Language

#### Description
Ngôn ngữ truy vấn cho tri thức — 6 commands covering full CRUD + reactive + introspection.

#### Technical Details

**Top-level AST:**
```rust
// ast.rs
pub enum Query {
    Find(FindQuery),
    Create(CreateQuery),
    CreateFromText(CreateFromTextQuery),
    Update(UpdateQuery),
    Deprecate(DeprecateQuery),
    Watch(WatchQuery),
    Explain(Box<Query>),
}
```

**FIND query:**
```rust
pub struct FindQuery {
    pub pattern: Pattern,                // Graph pattern (nodes + edges)
    pub where_clause: Option<Condition>, // Filter
    pub scope: Scope,                    // Local | Neighbors | ... | Global
    pub return_clause: Option<Vec<ReturnExpr>>,
    pub limit: Option<u32>,
    pub order_by: Option<Vec<OrderExpr>>,
}
```

**CREATE (Tier 1 structured):**
```rust
pub struct CreateQuery {
    pub pattern: Pattern,
    pub properties: Vec<Property>,
    pub gene_type: Option<KqlGeneType>,
    pub certainty: Option<u16>,
    pub instructions: Vec<CreateClause>,
    pub signed_by: String,
}

pub enum CreateClause {
    Triple { s: String, p: String, o: String },
    Quality { s: String, q: String },
    Quantity { s: String, value: f64, unit: String },
    PartOf { part: String, whole: String },
    Causal { cause: String, effect: String },
    Step { ord: u8, action: String, target: String },
    Certainty { level: u16 },
    // ...
}
```

**Conditions:**
```rust
pub enum Condition {
    Comparison { field: FieldPath, op: CompOp, value: Value },
    And(Box<Condition>, Box<Condition>),
    Or(Box<Condition>, Box<Condition>),
    Not(Box<Condition>),
    Exists(FieldPath),
    Contains { field: FieldPath, value: Value },
}
```

**Query examples:**
```
-- Tìm KU Fact với trust cao
FIND (k:KU) WHERE k.dna.header.gene_type = 0
  AND k.epi.trust.trust_score > 8000
SCOPE LOCAL LIMIT 10

-- Tìm KU chứa concept 301
FIND (k:KU) WHERE k.concept_ids CONTAINS 301

-- Deprecate KU cũ
DEPRECATE (k:KU) WHERE k.cid = "abc..."
REASON "Superseded by newer data"
SIGNED BY "node_xyz"

-- Watch realtime
WATCH FIND (k:KU) WHERE k.dna.header.gene_type = 1
ON CREATE NOTIFY "ws://localhost:8080"
```

#### Current Status
✅ **Implemented** — Parser (nom-based), executor, redb storage.

#### Code References
- [ast.rs](../../src/ku-kql/src/ast.rs) — `Query`, `FindQuery`, `CreateQuery`, `CreateFromTextQuery`, `Condition`, `Pattern`
- [parser.rs](../../src/ku-kql/src/parser.rs) — nom-based parser
- [executor.rs](../../src/ku-kql/src/executor.rs) — Local execution
- [storage.rs](../../src/ku-kql/src/storage.rs) — redb persistence

---

## §6 ConceptDict — Từ điển Concept

#### Description
Bảng ánh xạ 2 chiều: tên concept ↔ ConceptID. Đa ngôn ngữ (vi, en), case-insensitive, < 1μs per lookup.

#### Technical Details

```rust
// concept_dict.rs
pub struct ConceptEntry {
    pub id: ConceptId,
    pub name: String,                // Canonical name
    pub name_vi: Option<String>,     // Vietnamese
    pub name_en: Option<String>,     // English
    pub tier: u8,                    // 0-4 (varint byte width)
    pub category: Option<String>,
}

pub struct ConceptDict {
    by_id: HashMap<ConceptId, ConceptEntry>,
    by_name: HashMap<String, ConceptId>,  // Case-insensitive
    next_id: ConceptId,                    // Auto-increment from 128+
}
```

**Persistence (redb feature):**
- Module: `persistent_concept_dict.rs`
- Feature flag: `persist` → `dep:redb`
- Pure Rust, ACID, no C compiler needed

#### Current Status
✅ **Implemented** — In-memory + optional redb persistence.

#### Code References
- [concept_dict.rs](../../src/ku-core/src/concept_dict.rs) — `ConceptDict`, `ConceptEntry`
- [persistent_concept_dict.rs](../../src/ku-core/src/persistent_concept_dict.rs) — redb backend

---

## §7 OBP Network Protocol

#### Description
OneBrain Protocol — mạng ngang hàng 9 lớp cho phát tán, đồng bộ, và truy vấn KU. No central servers, mobile-first, scale target 100 BILLION+ nodes.

#### Technical Details

| Layer | Module | Structs/Types |
|-------|--------|--------------|
| 0 | `identity.rs` | NodeID, DID, Ed25519 keys |
| 1 | `messages.rs` | Message framing, wire format catalog |
| 2 | `membership.rs` | SWIM protocol, node fitness |
| 3 | `discovery.rs` | 6-layer peer bootstrap |
| 4 | `dht.rs` | S/Kademlia routing table |
| 5 | `stigmergy.rs` | Pheromone tables, ant colony routing |
| 6 | `vacuum.rs` | Probabilistic bloom/cuckoo filters |
| 7 | `pubsub.rs` | Topic-based subscribe/publish |
| 8 | `transport.rs` | QUIC transport (feature-gated) |
| — | `sync.rs` | State synchronization |
| — | `metabolism_gossip.rs` | PoMV metabolism data gossip |
| — | `query/` | Distributed query execution |

**Design principles:**
- NO central servers — network self-sustains
- Internet is OPTIMIZATION, not REQUIREMENT
- Scale: 100B+ nodes
- Mobile-first: <0.5% battery/day

#### Current Status
🔧 **In Progress** — Core modules implemented, integration testing ongoing.

#### Code References
- [ku-net/src/](../../src/ku-net/src/) — All network modules

---

## §8 AI Integration — Local Models Only

#### Description
Interface tool-calling cho AI models local. Hoàn toàn offline — không phụ thuộc cloud.

#### Technical Details

**Tool-calling pipeline:**
```text
1. ku_system_prompt.rs → Generate prompt with CoreDna schema
2. AI model (Gemma4/Qwen/Phi-3) → Return tool calls (JSON)
3. ku_tool_executor.rs → Parse tool calls → CoreDna instructions
4. KuRuntime::from_dna() → Complete KU
```

**Supported operations:**
- Create KU from tool calls
- Create CoreDna instructions (Triple, Quality, Quantity, etc.)
- Resolve concept names via ConceptDict

#### Current Status
✅ **Implemented** — Tool definitions, executor, system prompt.

#### Code References
- [ku_tools.rs](../../src/ku-core/src/ku_tools.rs) — Tool definitions
- [ku_tool_executor.rs](../../src/ku-core/src/ku_tool_executor.rs) — Executor
- [ku_system_prompt.rs](../../src/ku-core/src/ku_system_prompt.rs) — Prompt builder
