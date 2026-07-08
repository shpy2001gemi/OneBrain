# 🧠 OneBrain — Tổng quan Trụ cột & Tiến độ

> **Review toàn diện dự án OneBrain — 10 trụ cột chính và mức độ hoàn thiện**
> Ngày review: 07/07/2026 | Codebase: ~35,000+ dòng Rust | 757+ tests (ku-core 565 + ku-net 192) | 60+ tài liệu nghiên cứu
> **Update lớn (07/2026)**: Pillar 6 AI Layer — `onebrain` CLI node hoàn chỉnh (13 source files), P2P networking với seed node, encoding pipeline, anti-Sybil 12-layer, cross-node verification. Seed node binary (`onebrain-seed`) cho VPS deployment.

---

## Tổng quan nhanh

OneBrain là **mạng chia sẻ tri thức phi tập trung** lấy cảm hứng từ blockchain, nơi con người đóng góp, xác thực và hấp thụ tri thức — giống cách AI chia sẻ tri thức qua mạng. Dự án được viết bằng **Rust**, sử dụng kiến trúc **3 tầng KU** (Core DNA / Epigenetics / Expression) với 10 trụ cột chính.

```mermaid
graph TD
    subgraph PILLARS["TIEN DO 10 TRU COT"]
        P1["P1: Knowledge Unit - 95%"]
        P2["P2: Network Protocol - 95%"]
        P3["P3: KQL Query - 95%"]
        P4["P4: Consensus PoK v2 - 95%"]
        P5["P5: OBT Token - 95%"]
        P6["P6: AI Layer - 75%"]
        P7["P7: Knowledge Graph - 40%"]
        P8["P8: Storage Layer - 60%"]
        P9["P9: BCI Protocol - 15%"]
        P10["P10: User Interface - 25%"]
    end

    style P1 fill:#16a34a,stroke:#15803d,color:#fff
    style P2 fill:#16a34a,stroke:#15803d,color:#fff
    style P3 fill:#16a34a,stroke:#15803d,color:#fff
    style P4 fill:#16a34a,stroke:#15803d,color:#fff
    style P5 fill:#22c55e,stroke:#16a34a,color:#fff
    style P6 fill:#22c55e,stroke:#16a34a,color:#fff
    style P7 fill:#eab308,stroke:#ca8a04,color:#333
    style P8 fill:#22c55e,stroke:#16a34a,color:#fff
    style P9 fill:#ef4444,stroke:#dc2626,color:#fff
    style P10 fill:#f97316,stroke:#ea580c,color:#fff
    style PILLARS fill:none,stroke:#666,stroke-width:2px
```

---

## Bảng tổng hợp tiến độ

| # | Trụ cột | Tài liệu | Nghiên cứu | Code | Tests | Tiến độ |
|---|---------|-----------|-------------|------|-------|---------|
| 1 | **Knowledge Unit** | ⬛⬛⬛⬛⬛ | ⬛⬛⬛⬛⬛ | ⬛⬛⬛⬛⬛ | 565 | 🟢 **v6 Core DNA** |
| 2 | **Network Protocol** | ⬛⬛⬛⬛⬛ | ⬛⬛⬛⬛⬛ | ⬛⬛⬛⬛⬛ | 192 | 🟢 **Hoàn thiện** |
| 3 | **KQL (Query Language)** | ⬛⬛⬛⬛⬛ | ⬛⬛⬛⬛⬛ | ⬛⬛⬛⬛⬛ | 58 | 🟢 **Hoàn thiện** |
| 4 | **Consensus (PoK v2)** | ⬛⬛⬛⬛⬛ | ⬛⬛⬛⬛⬛ | ⬛⬛⬛⬛⬛ | 157 | 🟢 **Hoàn thiện** |
| 5 | **OBT Token** | ⬛⬛⬛⬛⬛ | ⬛⬛⬛⬛⬛ | ⬛⬛⬛⬛⬛ | 264+ | 🟢 **~95% implemented** |
| 6 | **AI Layer** | ⬛⬛⬛⬛⬜ | ⬛⬛⬛⬛⬜ | ⬛⬛⬛⬛⬜ | — | 🟢 **3 crates, 17 files, P2P + AI** |
| 7 | **Knowledge Graph** | ⬛⬛⬛⬜⬜ | ⬛⬛⬛⬛⬜ | ⬛⬛⬛⬜⬜ | — | 🟡 **Đang phát triển** |
| 8 | **Storage Layer** | ⬛⬛⬜⬜⬜ | ⬛⬛⬜⬜⬜ | ⬛⬛⬛⬛⬜ | 6 | 🟢 **Đã có nền tảng** |
| 9 | **BCI Protocol** | ⬛⬛⬜⬜⬜ | ⬛⬛⬜⬜⬜ | ⬜⬜⬜⬜⬜ | — | 🔴 **Tầm nhìn xa** |
| 10 | **User Interface** | ⬛⬛⬜⬜⬜ | ⬛⬜⬜⬜⬜ | ⬛⬛⬜⬜⬜ | — | 🟠 **CLI + P2P demo** |

---

## Chi tiết từng trụ cột

---

### 🟢 Pillar 1: Knowledge Unit (KU) — Nền tảng dữ liệu

> **Trạng thái: ✅ Hoàn thiện Phase 2 | ~18,000+ dòng Rust | 541 tests | 40+ modules**
>
> **v6 Major Redesign (29/06/2026)**: Chuyển từ CBOR monolithic sang **Core DNA** — custom binary format ultra-compact.
> Kết quả: CBOR v5 (1053B, 3.3x LỚN hơn text) → **Core DNA v6 (88B, 3.7x NHỎ hơn text)** — cải thiện **12x**.

Knowledge Unit là **đơn vị cơ bản nhất** của OneBrain — tương đương "transaction" trong blockchain. Mỗi KU đại diện cho một mẩu tri thức.

#### Kiến trúc 3 tầng v6 (Core DNA / Epigenetics / Expression)

```mermaid
graph TD
    subgraph STORED["💾 Stored (persistent)"]
        DNA["Core DNA<br/>Custom binary<br/>32 opcodes + varint<br/>CRC-16"]
    end
    
    subgraph RUNTIME["⚡ Runtime only"]
        EPI["Epigenetics<br/>CBOR overlay<br/>Trust, Bonds, Metabolism"]
    end
    
    subgraph GENERATED["📝 Generated"]
        EXP["Expression<br/>UTF-8 text<br/>Natural language rendering"]
    end
    
    DNA -->|"decode + inflate"| EPI
    EPI -->|"render"| EXP
    EXP -->|"encode (3-tier)"| DNA
    
    style DNA fill:#16a34a,stroke:#15803d,color:#fff
    style EPI fill:#3b82f6,stroke:#2563eb,color:#fff
    style EXP fill:#6b7280,stroke:#4b5563,color:#fff
    style STORED fill:none,stroke:#16a34a,stroke-width:2px
    style RUNTIME fill:none,stroke:#3b82f6,stroke-width:2px
    style GENERATED fill:none,stroke:#6b7280,stroke-width:2px
```

#### Pipeline encoding 3 tầng

```mermaid
graph LR
    T["Text input"] --> T1["Tier 1<br/>Rule-based<br/>~60-70%"]
    T --> T2["Tier 2<br/>AI Local<br/>~90%+"]
    T2 --> TE["Tool Executor<br/>15 tools"]
    T1 --> CD["CoreDna<br/>binary"]
    TE --> CD
    CD --> P2P["Tier 3<br/>P2P Refine"]
    
    style T1 fill:#22c55e,color:#fff
    style T2 fill:#3b82f6,color:#fff
    style TE fill:#8b5cf6,color:#fff
    style CD fill:#16a34a,color:#fff
    style P2P fill:#f97316,color:#fff
```

#### Code đã triển khai

| Module | File | Nội dung | Tests |
|--------|------|---------|-------|
| Types | [types.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/types.rs) | `KnowledgeUnit` struct, 11 `Gene` types, 33 `BondType`, `TrustSection`, `EpigeneticSection` | ✅ |
| **Core DNA v6** | [core_dna.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/core_dna.rs) | **32 opcodes, encode/decode, CRC-16, KU↔CoreDna bridge, auto-detect decoder** (~1800 dòng) | ✅ 13 |
| **Text Parser** | [text_parser.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/text_parser.rs) | **Tier 1 rule-based Vietnamese/English → CoreDna** (~1100 dòng) | ✅ 24 |
| **Tool Defs** | [ku_tools.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/ku_tools.rs) | **15 AI tool definitions (JSON Schema, OpenAI-compatible)** | ✅ 8 |
| **Tool Executor** | [ku_tool_executor.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/ku_tool_executor.rs) | **Stateful executor for AI function calling → CoreDna** | ✅ 5 |
| **System Prompt** | [ku_system_prompt.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/ku_system_prompt.rs) | **System prompt generator for local AI models** | ✅ 20 |
| Encoder (v5) | [encoder.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/encoder.rs) | CBOR encoder (backward compat, superseded by Core DNA) | ✅ |
| Decoder (v5) | [decoder.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/decoder.rs) | CBOR decoder (backward compat via `decode_any`) | ✅ |
| Varint | [varint.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/varint.rs) | 5-tier variable-length integer encoding | ✅ |
| CRDT | [crdt.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/crdt.rs) | `GCounter`, `PNCounter`, `LWWRegister`, `ORSet`, `VectorClock` | ✅ |

#### Kích thước thực đo

| Tri thức | Text UTF-8 | CBOR v5 (cũ) | **Core DNA v6** | Cải thiện |
|----------|-----------|-------------|----------------|-----------|
| "Bơi ếch" (3 KUs) | 323B | 1053B ❌ | **88B** ✅ | **12x nhỏ hơn CBOR** |
| Rocket (5 KUs) | 1078B | — | **172B** ✅ | **6.3x nhỏ hơn text** |
| Airplane wing | 131B | ~1500B | **118B** ✅ | **0.9x text** |
| Simple fact | 21B | ~226B | **16B** ✅ | **1.3x nhỏ hơn text** |

#### Điểm nổi bật v6
- **Core DNA LUÔN nhỏ hơn text** — đúng triết lý "DNA" gốc
- **32 opcodes**: Triple, PartOf, Quality, Quantity, Tolerance, Range, Step, Causal, EnumVal, Formula, Affect, Witness, Composite...
- **3-tier encoding**: Rule-based (offline) → AI Local (function calling) → P2P refine
- **15 AI tools**: `add_triple`, `add_part_of`, `add_quality`, `set_certainty`... — AI chỉ cần gọi tool, không cần biết binary
- **Pluggable AI runtime**: Gemma 4, Qwen, Phi-3, hoặc bất kỳ model nào hỗ trợ function calling
- **Backward compat**: `decode_any()` tự động phát hiện v4/v5 CBOR vs v6 Core DNA
- **11 Gene types**: Fact, Procedure, Experience, Creative, MediaExperience, Testimony, Formal, Hypothesis, Narrative, Sensory, Composite
- **11 Epistemic Status levels**: Rumor → Hearsay → Testimony → ... → FormallyProven → Axiomatic
- **CRDT primitives**: Sẵn sàng cho distributed sync

> [!TIP]
> Tài liệu chi tiết:
> - [KU_CORE_DNA_V6_SPEC.md](file:///c:/Users/shpy2/Documents/OneBrain/docs/specs/KU_CORE_DNA_V6_SPEC.md) — Wire format specification
> - [KU_ENCODING_PIPELINE.md](file:///c:/Users/shpy2/Documents/OneBrain/docs/specs/KU_ENCODING_PIPELINE.md) — Encoding pipeline (3-tier)
> - [KU_ARCHITECTURE.md](file:///c:/Users/shpy2/Documents/OneBrain/docs/specs/KU_ARCHITECTURE.md) — Overall architecture

---

### 🟢 Pillar 2: Network Protocol — Hạ tầng P2P

> **Trạng thái: ✅ Hoàn thiện | ~9,500+ dòng Rust | 192 tests | Trụ cột lớn nhất**
>
> **Update (07/2026)**: Thêm OBT gossip, transfer validation, fork warrant modules. 192 tests (tăng từ 162).

Network Protocol cho phép các node OneBrain kết nối phi tập trung, chia sẻ KU, và đồng bộ tri thức — không cần server trung tâm.

#### Stack 9 tầng giao thức (đã triển khai)

```mermaid
graph LR
    L0["L0: Identity"] --> L1["L1: Transport"]
    L1 --> L2["L2: Membership"]
    L2 --> L3["L3: Discovery"]
    L3 --> L4["L4: DHT"]
    L4 --> L5["L5: Stigmergy"]
    L5 --> L6["L6: Content"]
    L6 --> L7["L7: PubSub"]
    L7 --> L8["L8: Sync"]

    style L0 fill:#16a34a,color:#fff
    style L1 fill:#16a34a,color:#fff
    style L2 fill:#16a34a,color:#fff
    style L3 fill:#16a34a,color:#fff
    style L4 fill:#16a34a,color:#fff
    style L5 fill:#16a34a,color:#fff
    style L6 fill:#16a34a,color:#fff
    style L7 fill:#16a34a,color:#fff
    style L8 fill:#16a34a,color:#fff
```

#### Code đã triển khai

| Module | File | Nội dung | Tests |
|--------|------|---------|-------|
| Identity | [identity.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/identity.rs) | `NodeId` (BLAKE3), `KeyPair` (Ed25519), crypto puzzle, DID format | ✅ |
| Transport | [transport.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/transport.rs) | **Real QUIC** transport (not mock), self-signed certs, bi-directional streams | ✅ |
| Messages | [messages.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/messages.rs) | **56 message types** across 8 layers, 4-byte header, compression modes | ✅ |
| Membership | [membership.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/membership.rs) | **SWIM protocol**, 7 node tiers, fitness scoring (7 weights), promotion/demotion | ✅ |
| Discovery | [discovery.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/discovery.rs) | **6-layer cascade**: Social → Local → HTTP → DHT → DNS → Hardcoded | ✅ |
| DHT | [dht.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/dht.rs) | **Kademlia** routing table (256 buckets, k=20), store/find/closest | ✅ |
| Stigmergy | [stigmergy.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/stigmergy.rs) | **Bio-inspired** pheromone routing: reinforce/evaporate/best_hop | ✅ |
| Vacuum | [vacuum.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/vacuum.rs) | Bloom filter for content routing (BLAKE3-based) | ✅ |
| PubSub | [pubsub.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/pubsub.rs) | Topic subscription, interest vectors (128-bit Bloom) | ✅ |
| Sync | [sync.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/sync.rs) | **Delta-state CRDT sync** with VectorClock, bidirectional exchange | ✅ |
| Constants | [constants.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/constants.rs) | Full constant registry (QUIC, SWIM, DHT, fitness weights) | ✅ |
| OBT Transfer | [obt_transfer.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/obt_transfer.rs) | OBT transfer validation, eligibility | ✅ |
| OBT Gossip | [obt_gossip.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/obt_gossip.rs) | Fork warrant validation, mint broadcast | ✅ |
| DHT Replica | [dht.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/dht.rs) (extension) | ReplicaTracker for OBT storage rewards | ✅ |

#### Đặc biệt: Bio-inspired Design
- **Stigmergy** (lấy cảm hứng từ kiến): Query routing dựa trên "pheromone trails" — routes thành công được reinforced, routes thất bại bị penalty
- **7-tier node hierarchy**: Leaf → Contributor → LocalSP → RegionalSP → CountrySP → ContinentalSP → GlobalBackbone
- **Fitness scoring**: 7 components (uptime, battery, bandwidth, storage, latency, availability, reliability) → auto-promote/demote

#### Integration Tests

| File | Nội dung |
|------|---------|
| [integration.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/tests/integration.rs) | 6 end-to-end tests: 3-node transfer, bootstrap, tamper detection, CID, XOR routing, full pipeline |
| [test_vectors.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/tests/test_vectors.rs) | 12 wire format test vectors: MessageHeader, IPv4/IPv6, BLAKE3, CRC, Ed25519 |

> [!IMPORTANT]
> Đây là trụ cột **hoàn thiện nhất** và **độc đáo nhất** — kết hợp QUIC + Kademlia + SWIM + Stigmergy trong một protocol thống nhất. Rất ít dự án open-source nào đạt được mức hoàn thiện này.

---

### 🟢 Pillar 3: Knowledge Query Language (KQL)

> **Trạng thái: ✅ Hoàn thiện | ~5,300 dòng Rust | 58 tests | Spec: [KQL_SPEC.md](file:///c:/Users/shpy2/Documents/OneBrain/docs/specs/KQL_SPEC.md)**

KQL là **ngôn ngữ truy vấn riêng** cho OneBrain — tương tự SQL cho database, nhưng chuyên biệt cho Knowledge Graph phi tập trung. Đã triển khai **7 phases (A→G)** bao gồm parser, executor, distributed query, discovery engine, optimization.

#### Kiến trúc KQL (đã triển khai đầy đủ)

```mermaid
graph TD
    KQL["KQL String"] --> Parser
    Parser --> AST["AST 6 query types"]
    AST --> Cache{"Query Cache LRU"}
    Cache -->|Hit| Results
    Cache -->|Miss| Executor

    subgraph Local["Local Execution"]
        Executor --> LocalExec["Local Executor"]
        LocalExec --> WatchEngine["Watch Engine"]
    end

    subgraph Network["Distributed Query"]
        Executor --> Router["6-Layer Router"]
        Router --> L0["L0 Local"]
        Router --> L1["L1 Neighbors"]
        Router --> L2["L2 Cluster"]
        Router --> L3["L3 DHT"]
        Router --> L4["L4 Semantic"]
        Router --> L5["L5 Global"]
    end

    subgraph Discovery["Discovery Engine"]
        GapDetector --> WatchEngine
        BridgeFinder --> Router
        Serendipity --> Router
    end

    subgraph Merge["Result Processing"]
        L1 --> Merger
        L2 --> Merger
        L3 --> Merger
        Merger --> Results
        Merger --> Learner["Pheromone Learner"]
        Learner --> Router
    end

    style KQL fill:#6c63ff,color:#fff
    style Cache fill:#f59e0b,color:#fff
    style Router fill:#3b82f6,color:#fff
    style Merger fill:#22c55e,color:#fff
    style Results fill:#16a34a,color:#fff
    style GapDetector fill:#ef4444,color:#fff
    style BridgeFinder fill:#ef4444,color:#fff
    style Serendipity fill:#ef4444,color:#fff
    style Learner fill:#8b5cf6,color:#fff
```

#### Code đã triển khai — KQL Engine (ku-kql)

| Module | File | Nội dung | Tests |
|--------|------|---------|-------|
| AST | [ast.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/ast.rs) | 6 query types: Find, Create, Update, Deprecate, Watch, Explain | ✅ |
| Parser | [parser.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/parser.rs) | **nom-based parser** (~800L): FIND, CREATE, UPDATE, DEPRECATE, WATCH, EXPLAIN | 28 tests |
| Executor | [executor.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/executor.rs) | Aggregation (COUNT/SUM/AVG/MIN/MAX), CREATE, UPDATE, DEPRECATE, WATCH, EXPLAIN | 23 tests |
| Storage | [storage.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/storage.rs) | **redb-backed** persistent storage (BLAKE3 CID, ACID) | 6 tests |

#### Code đã triển khai — Distributed Query (ku-net/query)

| Module | File | Nội dung | Tests |
|--------|------|---------|-------|
| ConceptIndex | [index.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/index.rs) | VacuumFilter + BLAKE3 concept keys, DHT publishing | 7 tests |
| Wire Messages | [messages.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/messages.rs) | QueryForward (0x50), QueryResponse (0x51), QueryCancel (0x52) | 5 tests |
| QueryRouter | [router.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/router.rs) | **6-layer scope escalation** (Local→Neighbors→Cluster→DHT→Semantic→Global) | 6 tests |
| ResultMerger | [merger.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/merger.rs) | Dedup via BLAKE3, trust×scope ranking, multi-source aggregation | 7 tests |
| WatchEngine | [watch.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/watch.rs) | Standing queries, event filter (Create/Update/Deprecate/Any), TTL propagation | 9 tests |
| GapDetector | [gaps.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/discovery/gaps.rs) | Orphan concepts, low-confidence, missing evidence, untested hypotheses | 6 tests |
| BridgeFinder | [bridges.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/discovery/bridges.rs) | **Swanson ABC model** — cross-domain undiscovered public knowledge | 3 tests |
| SerendipityEngine | [serendipity.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/discovery/serendipity.rs) | Interest profiles, relevance×novelty scoring, exploration queries | 6 tests |
| QueryCache | [cache.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/cache.rs) | LRU cache, BLAKE3-keyed normalized KQL, TTL expiration, hit rate stats | 9 tests |
| PheromoneLearner | [learning.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/learning.rs) | Ant colony-inspired reinforcement learning for scope routing | 8 tests |
| Integration | [query_integration.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/tests/query_integration.rs) | 7 E2E pipeline + 6 stress tests (10K concepts, 1000 watches, 500 KUs) | 13 tests |

#### Ví dụ KQL

```sql
-- Tìm knowledge với scope và limit
FIND (ku:KU) WHERE ku.trust_score > 8000 SCOPE cluster LIMIT 10

-- Tìm với aggregation
FIND (ku:KU) RETURN COUNT(ku), AVG(ku.trust_score)

-- Tạo KU mới
CREATE (ku:KU {body: "Water boils at 100°C"}) SIGNED BY "author_id"

-- Cập nhật tri thức
UPDATE (ku:KU) SET ku.trust_score = 9000 WHERE ku.concept_id = 42 SIGNED BY "did:ob:abc"

-- Deprecate tri thức lỗi thời
DEPRECATE (ku:KU) WHERE ku.concept_id = 42 REASON "Superseded" SIGNED BY "did:ob:abc"

-- Standing query — tự động thông báo khi có knowledge mới
WATCH FIND (ku:KU) WHERE ku.trust_score > 7000 ON CREATE NOTIFY "callback://agent"

-- Giải thích query plan
EXPLAIN FIND (ku:KU) WHERE ku.confidence > 50 SCOPE DHT
```

#### Scopes (6 levels)

| Layer | Scope | TTL | Strategy |
|-------|-------|-----|----------|
| L0 | `LOCAL` | 0 | Execute on self |
| L1 | `NEIGHBORS` | 1 | 1-hop SWIM peers (fanout=5) |
| L2 | `CLUSTER` | 3 | Super-peer routing |
| L3 | `DHT` | 8 | Kademlia concept key lookup |
| L4 | `SEMANTIC` | 5 | Stigmergy pheromone trails |
| L5 | `GLOBAL` | 12 | Random walk + TTL flooding |

#### Discovery Engine — "Tìm tri thức bạn không biết là mình cần"

| Component | Thuật toán | Mô tả |
|-----------|-----------|-------|
| **GapDetector** | Orphan + Cluster analysis | Phát hiện lỗ hổng tri thức, đề xuất query để fill |
| **BridgeFinder** | Swanson ABC model | Tìm cầu nối xuyên lĩnh vực (undiscovered public knowledge) |
| **SerendipityEngine** | Relevance × Novelty | Scoring = relevance × novelty, sweet-spot bell curve |
| **WatchEngine** | Event-driven + TTL propagation | Standing queries tự fire khi có KU mới match |

> [!TIP]
> KQL đã hoàn thiện Phase 1 với 7 sub-phases (A→G). Spec đầy đủ tại [KQL_SPEC.md](file:///c:/Users/shpy2/Documents/OneBrain/docs/specs/KQL_SPEC.md). Remaining 5% là async I/O integration khi có tokio runtime.

---

### 🟢 Pillar 4: Consensus — Proof of Knowledge v2 (PoMV)

> **Trạng thái: ✅ Hoàn thiện | 16 modules | ~5,012 dòng Rust | 157 tests | Spec: [POK_V2_SPECIFICATION.md](file:///c:/Users/shpy2/Documents/OneBrain/docs/specs/POK_V2_SPECIFICATION.md)**
>
> **Update (07/2026)**: Thêm obt_integration, ku_lifecycle, epigenetics, spread_analysis. PoMV scores feed trực tiếp vào OBT minting formula.

PoK v2 là cơ chế đồng thuận **Proof-of-Metabolic-Value (PoMV)** — tri thức tự chứng minh giá trị qua 6 tín hiệu **quan sát được**, hoàn toàn **observation-based**, không cần voting.

> [!IMPORTANT]
> **PoK v2 (06/2026)**: Redesign hoàn toàn từ vote-based sang **observation-based**. Đã implement đầy đủ 12 modules + runtime + gossip protocol. Founder approved: "Knowledge value = usage, NOT correctness". **KHÔNG CẦN VOTING, KHÔNG THU HỒI OBT.**

#### Kiến trúc 6 tín hiệu PoMV (đã triển khai đầy đủ)

```mermaid
graph LR
    M["Metabolism 35%"] --> POMV["PoMV Score"]
    P["Prediction 15%"] --> POMV
    E["Entropy 10%"] --> POMV
    S["Survival 10%"] --> POMV
    SY["Synaptic 15%"] --> POMV
    N["Niche 15%"] --> POMV
    POMV --> R["OBToken Reward"]

    style M fill:#ef4444,color:#fff
    style P fill:#f97316,color:#fff
    style E fill:#eab308,color:#fff
    style S fill:#22c55e,color:#fff
    style SY fill:#3b82f6,color:#fff
    style N fill:#8b5cf6,color:#fff
    style POMV fill:#16a34a,color:#fff
    style R fill:#f59e0b,color:#fff
```

#### Code đã triển khai — PoK v2 Engine (ku-core)

| Module | File | Nội dung | Tests |
|--------|------|---------|-------|
| Metabolism | [metabolism.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/metabolism.rs) | GCounter-based usage tracking: query, retrieval, citation, derivative, refutation, dwell time | 15 |
| Metabolism Store | [metabolism_store.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/metabolism_store.rs) | Per-KU CRDT storage, merge_remote, GC dead KUs, top_active | 7 |
| Epistemic Engine | [epistemic_engine.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/epistemic_engine.rs) | 9 observable status transitions (Rumor→...→Consensus→Formally Proven) | 10 |
| Entropy | [entropy.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/entropy.rs) | Cosine distance on int8 embeddings, LSH bridge detection, SimHash near-duplicate, 7-day decay | 16 |
| Prediction | [prediction.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/prediction.rs) | 4 resolution methods: TemporalConsistency, UsageOutcome, CrossReference, NoResolution (Experience) | 12 |
| Synaptic | [synaptic.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/synaptic.rs) | Hebbian bonds (co-retrieval/co-citation), pheromone evaporation, PageRank centrality | 13 |
| Immune | [immune.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/immune.rs) | 4 antibody types (content-agnostic): TemporalBurst, SourceConcentration, LowEngagement, DiversityDeficit | 11 |
| Ecosystem | [ecosystem.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/ecosystem.rs) | Ecological niche fitness: density, bridging, metabolic share | 8 |
| PoMV Aggregator | [pomv.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/pomv.rs) | Weighted aggregation of 6 signals, configurable weights, reward calculation | 9 |
| EigenTrust | [eigentrust.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/eigentrust.rs) | Node-level reputation: local trust + power iteration, quarantine penalty, diversity bonus | 8 |
| Spread Analysis | [spread_analysis.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/spread_analysis.rs) | Organic vs bot spread: temporal CV, source diversity, geographic, engagement authenticity | 12 |
| PoMV Runtime | [pomv_runtime.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/pomv_runtime.rs) | Orchestrator: register → record → tick → compute 6 signals → update TrustSection → GC | 9 |

#### Code đã triển khai — Gossip Protocol (ku-net)

| Module | File | Nội dung | Tests |
|--------|------|---------|-------|
| Metabolism Gossip | [metabolism_gossip.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/metabolism_gossip.rs) | Wire types (0x86/0x87/0x89), handle_update/query/response, prepare_update, CRDT-safe merge | 6 |

#### Nguyên tắc thiết kế (Founder decisions, 06/2026)

| Nguyên tắc | Quyết định | Impact |
|---|---|---|
| **No voting** | Tất cả signals từ GCounters (usage) | Anti-Sybil: bots tốn cost mà không tạo usage thật |
| **No clawback** | GCounters chỉ tăng, không giảm | Công bằng: không ai mất token đã earn |
| **No censorship** | Immune system nhìn PATTERN, không nhìn CONTENT | Freedom: content-agnostic spread analysis |
| **Decentralized** | Mỗi node tự evaluate, CRDT merge | Scalable: no central authority |
| **Experience respected** | NoResolution mode cho Experience KU | Inclusive: trải nghiệm không đúng/sai |
| **Anti-fragile** | Survive attack → stronger | Robust: KU sống sót qua attack được bonus |

> [!TIP]
> PoK v2 là trụ cột **phức tạp nhất** — 16 modules, 157 tests, 6 tín hiệu, phát minh mới hoàn toàn. Remaining 5% là fine-tuning weights khi có dữ liệu thực từ production.

---

### 🟢 Pillar 5: Token Economics (OBT) — Knowledge Utility Token

> **Trạng thái: ✅ ~95% implemented | 11 modules | ~260 KB Rust | 264+ tests**
>
> **Major update (06-07/2026)**: OBT redesigned from scratch. NOT a cryptocurrency — it's a Knowledge Utility Token ("kWh for knowledge").
> Account-Chain ledger (Nano-inspired), output-based minting, 4 reward streams, 5-tier penalty system.
> **Update 01/07**: Closed 3 gaps — DHT replica wiring ✅, Ed25519 real verification ✅, GovernanceConfig ✅.

OBT là **utility token** của OneBrain — phần thưởng cho việc đóng góp, mã hóa, xác thực và lưu trữ tri thức.

#### Core Design Decisions (đã giải quyết)

| # | Quyết định | Giải pháp |
|---|------------|----------|
| Q1 | Global emission cap? | ✅ E(epoch) = B × A × Q |
| Q2 | Trust-gated rewards? | ✅ 7-tier NodeTier (Leaf 0.10× → GlobalBackbone 2.00×) |
| Q3 | Fraud punishment? | ✅ 5-tier graduated penalties |
| Q4 | Balance structure? | ✅ Account-Chain (Nano-style, NOT CRDTs) |
| Q5 | Permanent ban? | ✅ Tombstone (Tier 5) |
| Q6 | Epoch duration? | ✅ 1 hour (3,600s) |

#### Code đã triển khai — OBT Engine (ku-core)

| Module | File | Nội dung | Tests |
|--------|------|---------|-------|
| Constants | [obt_constants.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/obt_constants.rs) | 96 governance constants, NodeTier enum, 7-tier hierarchy | 25+ |
| Ledger | [obt_ledger.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/obt_ledger.rs) | TransferBlock, AccountState, **real Ed25519 verification**, fork detection | 43+ |
| Minting | [obt_minting.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/obt_minting.rs) | Emission formula E=B×A×Q, 4 reward streams (R1-R4), MintProof | 30+ |
| Storage Reward | [obt_storage_reward.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/obt_storage_reward.rs) | 5-factor formula, PoS-KU challenges, strike system | 25+ |
| Penalty | [obt_penalty.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/obt_penalty.rs) | 5-tier penalties, correlation multiplier, appeal framework | 30+ |
| Anti-Gaming | [obt_anti_gaming.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/obt_anti_gaming.rs) | Rate limiting, 4 quality gates, 4 pattern detectors | 35+ |
| Gossip Security | [obt_gossip_security.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/obt_gossip_security.rs) | GossipGapDetector, ConnectivityProof, EpochSummary | 17+ |
| Fork Pipeline | [obt_fork_pipeline.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/obt_fork_pipeline.rs) | ForkWarrant lifecycle: Detected → Verified → Penalized | 12+ |
| Epoch | [obt_epoch.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/obt_epoch.rs) | EpochAccumulator, settlement, epoch boundaries | 17+ |
| Integration | [obt_integration.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/obt_integration.rs) | KU↔OBT bridge, **ReplicaSnapshot**, **compute_epoch_storage_rewards()** | 11+ |
| **Governance** | [obt_governance.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/obt_governance.rs) | **GovernanceConfig** (15 params), validate(), runtime-configurable | **18** |

#### Key Formulas (đã implement)

- **Emission**: E(epoch) = B × A(epoch) × Q(epoch), B=10,000 OBT/epoch
- **Trust decay**: trust(t) = trust₀ × e^(-0.01×t), half-life ≈ 69.3 hours
- **Storage reward**: base × size_w × rarity_w × demand_w × duration_f × trust_f
- **Correlation penalty**: multiplier = 1 + log₂(simultaneous_nodes)

#### Documentation

| Doc | Files | Nội dung |
|-----|-------|---------|
| Specs | 9 files (docs/specs/obt/) | Complete specification: Ledger, Minting, Storage, Anti-Gaming, Transfer, Security, Penalty, Constants |
| Research | 6 files (docs/research/obt/) | CRDT ledger analysis, penalty research, anti-gaming research, synthesis |
| Paper | 11 chapters (docs/paper/obt/) | Full academic paper: ~3,000 lines, 45 tables, 13 figures |
| Design | OBT_DESIGN.md, OBT_CURRENT_STATE.md | Architecture decisions, security analysis |

#### Đã hoàn thành (01/07/2026)
- ~~🔲 DHT replica tracking full wiring~~ ✅ `ReplicaSnapshot` + `compute_epoch_storage_rewards()` bridge
- ~~🔲 Ed25519 full key management integration~~ ✅ Real Ed25519 verify in `validate_signature()`, `create_signed_block()`
- ~~🔲 Governance parameter runtime adjustment~~ ✅ `GovernanceConfig` struct (15 params, validation, serde)

#### Remaining ~5%
- 🔲 Cross-shard transfers (future architectural feature — requires multi-shard design)

---

### 🟢 Pillar 6: AI Layer — Local AI + P2P Network Node

> **Trạng thái: ✅ ~75% implemented | 3 crates, 17 source files | Spec: [PILLAR6_AI_TECHNICAL_SPEC.md](file:///c:/Users/shpy2/Documents/OneBrain/docs/specs/PILLAR6_AI_TECHNICAL_SPEC.md)**
>
> **Major update (07/07/2026)**: Triển khai hoàn chỉnh `onebrain` CLI node — encoding pipeline với local Ollama AI, persistent storage (redb), anti-Sybil 12-layer, TCP P2P networking với cross-node verification. Thêm `onebrain-seed` (lightweight relay cho VPS) và `onebrain-protocol` (shared message types).

#### Quyết định kiến trúc (đã giải quyết)

| # | Quyết định | Giải pháp |
|---|------------|----------|
| Q1 | Cloud vs Local | ✅ **100% Local** — Ollama, không cloud API |
| Q2 | Model management | ✅ **Curated + Custom** — default `qwen3:8b` + `nomic-embed-text` |
| Q3 | Runtime | ✅ **OllamaBackend** trait-based abstraction |
| Q4 | 1 model hay nhiều? | ✅ **1 chat model + 1 embedding model** per node |
| Q5 | Anti-bot | ✅ **12-layer defense** (rate limit + quality gates + encoding consensus) |
| Q6 | NAT traversal | ✅ **Seed node relay** (n1.onebrain.live, n2.onebrain.live) |

#### 3 Crates đã triển khai

**`onebrain` — Full client node (13 files)**

| Module | File | Nội dung |
|--------|------|---------|
| Entry | [main.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/onebrain/src/main.rs) | Clap CLI (`start` subcommand), P2P auto-discovery startup |
| Node | [node.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/onebrain/src/node.rs) | `OneBrainNode` — ties mediator, storage, network, anti-gaming |
| CLI | [cli.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/onebrain/src/cli.rs) | Interactive REPL (encode, search, peers, connect, status) |
| Config | [config.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/onebrain/src/config.rs) | `NodeConfig` (port, data-dir, ollama-url, model, seeds) |
| Error | [error.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/onebrain/src/error.rs) | `NodeError` enum unifying all subsystem errors |
| Network | [network.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/onebrain/src/network.rs) | TCP transport, `NetMessage`, `NodeEvent` |
| Peers | [peer_manager.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/onebrain/src/peer_manager.rs) | Peer tracking with deduplication |
| Anti-Sybil | [anti_gaming_guard.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/onebrain/src/anti_gaming_guard.rs) | Rate limiting (1 KU/hr Leaf) + Gate 1 quality check |
| Verifier | [verifier_service.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/onebrain/src/verifier_service.rs) | Cross-node verification via `core_dna_agreement()` |
| Seed Client | [seed_client.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/onebrain/src/seed_client.rs) | Connect to seed, register, get peers, relay |
| Peer Memory | [peer_memory.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/onebrain/src/peer_memory.rs) | Remember peers across restarts (JSON) |
| UPnP | [upnp.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/onebrain/src/upnp.rs) | UPnP stub (ready for `igd-next`) |
| mDNS | [mdns_discovery.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/onebrain/src/mdns_discovery.rs) | mDNS stub (ready for `mdns-sd`) |

**`onebrain-seed` — Lightweight relay for VPS (4 files)**

| Module | File | Nội dung |
|--------|------|---------|
| Entry | [main.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/onebrain-seed/src/main.rs) | Clap CLI (--port, --max-peers, --name) |
| Registry | [registry.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/onebrain-seed/src/registry.rs) | Peer tracking, heartbeat, stale cleanup |
| Relay | [relay.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/onebrain-seed/src/relay.rs) | Message forwarding between NAT'd peers |
| Server | [server.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/onebrain-seed/src/server.rs) | TCP accept loop, message routing, stats |

**`onebrain-protocol` — Shared P2P protocol (1 file)**

| Module | File | Nội dung |
|--------|------|---------|
| Protocol | [lib.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/onebrain-protocol/src/lib.rs) | `PeerMessage`, `SeedMessage`, `PeerSummary`, wire format, hardcoded seeds |

#### Encoding Pipeline (hoàn chỉnh)

```
User text → Mediator (intent routing)
    → AiEncoder (Ollama qwen3:8b) → CoreDna wire_bytes
    → AntiGamingGuard (rate limit + Gate 1)
    → KuStorage.put() (redb, ACID)
    → KuRetriever.index() (keyword search)
    → broadcast_ku() (TCP push to peers)
    → request_verification() (3-node consensus)
```

#### Anti-Sybil 12-Layer (6 layers wired)

| Layer | Mechanism | Status |
|:------|:----------|:-------|
| 1 | BLAKE3 crypto puzzle (NodeID) | ✅ Implemented in ku-net |
| 2 | Rate limiting (1 KU/hr Leaf) | ✅ **Enforced in AntiGamingGuard** |
| 3 | Quality Gate 1 (min 256B + 2 instr) | ✅ **Enforced in AntiGamingGuard** |
| 4 | Encoding consensus (3 verifiers) | ✅ **Wired via VerifyRequest/Response** |
| 5 | Semantic agreement (≥0.6) | ✅ **Used in verifier_service** |
| 6 | Duplicate detection (BLAKE3 CID) | ✅ **Enforced in KuStorage.put()** |
| 7-12 | Gaming detection, PoMV, SWIM, caps | Implemented in ku-core/ku-net |

#### Còn thiếu (~25%)
- 🔲 UPnP auto port forward (crate integration)
- 🔲 mDNS LAN discovery (crate integration)
- 🔲 QUIC transport thay TCP (production)
- 🔲 UDP hole punching (NAT traversal không cần relay)
- 🔲 Full DHT-based peer discovery

---

### 🟡 Pillar 7: Knowledge Graph

> **Trạng thái: Schema đã thiết kế, CRDT/Bond types đã code | Tiến độ ~40%**

Knowledge Graph kết nối tất cả KU — phát hiện tri thức liên quan, tìm lỗ hổng tri thức, và kết nối xuyên lĩnh vực.

#### Đã có

| Hạng mục | Chi tiết |
|----------|---------|
| **33 Bond types** | Đã code trong [types.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/types.rs) (Causal, Spatial, Temporal, Analogical...) |
| **26 edge types** | Thiết kế trong research (7 categories) |
| **Graph DB survey** | [knowledge_graph_research.md](file:///c:/Users/shpy2/Documents/OneBrain/.analysis/research/knowledge_graph_research.md) — Neo4j, TigerGraph, ArangoDB, DGraph |
| **Schema design** | Node types + Edge types + query patterns |
| **Serendipity Engine** | ✅ Đã code trong ku-net/query/discovery/serendipity.rs |
| **Gap Detector** | ✅ Đã code trong ku-net/query/discovery/gaps.rs |
| **Bridge Finder** | ✅ Đã code trong ku-net/query/discovery/bridges.rs (Swanson ABC) |

#### Chưa triển khai
- 🔲 Graph database integration (Neo4j hoặc custom)
- 🔲 Graph traversal algorithms (BFS/DFS trên distributed graph)
- ~~🔲 Gap detection~~ ✅ Đã triển khai
- ~~🔲 Cross-domain connection engine~~ ✅ Đã triển khai (BridgeFinder)
- ~~🔲 Serendipity Engine~~ ✅ Đã triển khai

---

### 🟢 Pillar 8: Storage Layer

> **Trạng thái: Có nền tảng | redb-backed persistence đã code**

#### Đã triển khai

| Module | File | Nội dung |
|--------|------|---------|
| **KuStorage** | [storage.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/storage.rs) | redb-backed ACID storage, content-addressed (BLAKE3 CID), 3 tables, 6 tests |

#### Đặc điểm
- ✅ **Content-addressed**: CID = BLAKE3 hash → deterministic, idempotent
- ✅ **ACID**: Transaction-based qua redb
- ✅ **3 indexes**: `kus` (CID→KU), `index_trust` (trust_score), `index_concept` (concept_id)
- ✅ **6 tests passing**

#### Còn thiếu
- 🔲 IPFS/decentralized storage integration
- 🔲 Data replication across nodes
- 🔲 Caching layer
- 🔲 Media storage pipeline

---

### 🔴 Pillar 9: BCI Protocol

> **Trạng thái: Tầm nhìn dài hạn (Phase 5) | Tiến độ ~15%**

#### Đã nghiên cứu
- ✅ Landscape BCI (Neuralink, Synchron, OpenBCI)
- ✅ Neural signal types & encoding
- ✅ Privacy considerations
- ✅ Timeline estimates: **2030-2035**

> [!NOTE]
> BCI là tầm nhìn Phase 5, không cần triển khai ngay. Nghiên cứu hiện tại đủ để giữ hướng phát triển.

---

### 🟠 Pillar 10: User Interface

> **Trạng thái: CLI node hoàn chỉnh + Demo cũ | Tiến độ ~25%**
>
> **Update (07/07/2026)**: `onebrain` CLI node thay thế `ku-demo` cũ — interactive REPL với encoding, search, peer management, P2P networking. Có thể chạy multi-node demo thực tế.

#### Đã có
- ✅ **[onebrain CLI](file:///c:/Users/shpy2/Documents/OneBrain/src/onebrain/)** — Full interactive node (13 files)
  - REPL: `encode`, `search`, `peers`, `connect`, `status`, `help`, `quit`
  - P2P: auto-connect to seed, broadcast KU, cross-node verify
  - Storage: persistent redb, keyword search index
  - Anti-Sybil: rate limiting + quality gates
- ✅ **[onebrain-seed](file:///c:/Users/shpy2/Documents/OneBrain/src/onebrain-seed/)** — VPS relay node (4 files)
- ✅ [ku-demo](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-demo/src/main.rs) — 10-step CLI demo cũ (402 dòng)
- ✅ [runtime.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-demo/src/runtime.rs) — `OBPNode` unified runtime (204 dòng)
- ✅ [testbed.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-demo/src/testbed.rs) — 3-node testbed (322 dòng)
- ✅ [README.md](file:///c:/Users/shpy2/Documents/OneBrain/src/onebrain/README.md) — User guide (Vietnamese)

#### Chưa có
- 🔲 Web App (dashboard, KU browser)
- 🔲 Knowledge Graph visualization
- 🔲 Mobile App (Android/iOS)
- 🔲 Desktop App (Tauri/Electron)

---

## Cấu trúc Source Code

```
src/                                    # ~35,000+ dòng Rust | 757+ tests | 10 crates
├── Cargo.toml                          # Workspace root (10 members)
│
├── ku-core/                            # 🟢 Pillar 1+4+5 — ~18,000+ LOC | 541 tests
│   └── src/
│       ├── lib.rs                      # Module exports (40+ modules)
│       ├── types.rs                    # KU struct, Genes, Bonds, Trust+PoMV, Epigenetic (1023L)
│       ├── encoder.rs                  # Wire format encoder (229L)
│       ├── decoder.rs                  # Wire format decoder + CRC (149L)
│       ├── varint.rs                   # Variable-length integer (241L)
│       ├── crdt.rs                     # GCounter, PNCounter, LWW, ORSet, VectorClock (484L)
│       ├── error.rs                    # Error types (33L)
│       │
│       │  ── PoK v2: Proof-of-Metabolic-Value ──
│       ├── metabolism.rs               # ★ GCounter usage tracking (385L, 15 tests)
│       ├── metabolism_store.rs         # ★ Per-KU CRDT storage + GC (235L, 7 tests)
│       ├── epistemic_engine.rs         # ★ Observable status transitions (300L, 10 tests)
│       ├── entropy.rs                  # ★ Novelty + bridge + SimHash (280L, 16 tests)
│       ├── prediction.rs              # ★ 4 resolution methods (350L, 12 tests)
│       ├── synaptic.rs                # ★ Hebbian bonds + PageRank (382L, 13 tests)
│       ├── immune.rs                  # ★ Content-agnostic antibodies (389L, 11 tests)
│       ├── ecosystem.rs               # ★ Ecological niche fitness (292L, 8 tests)
│       ├── pomv.rs                    # ★ 6-signal weighted aggregator (256L, 9 tests)
│       ├── eigentrust.rs              # ★ Node reputation scoring (272L, 8 tests)
│       ├── spread_analysis.rs         # ★ Organic spread detection (308L, 12 tests)
│       ├── pomv_runtime.rs            # ★ Orchestrator: tick + update (310L, 9 tests)
│       │
│       │  ── OBT: OneBrain Token ──
│       ├── obt_constants.rs          # 96 constants, NodeTier (723L)
│       ├── obt_ledger.rs             # Account-Chain, TransferBlock (55KB)
│       ├── obt_minting.rs            # Emission formula, 4 streams (613L)
│       ├── obt_storage_reward.rs     # 5-factor formula, PoS-KU (27KB)
│       ├── obt_penalty.rs            # 5-tier penalties (29KB)
│       ├── obt_anti_gaming.rs        # Rate limits, quality gates (17KB)
│       ├── obt_gossip_security.rs    # Gap detection, connectivity (15KB)
│       ├── obt_fork_pipeline.rs      # Fork warrants (17KB)
│       ├── obt_epoch.rs              # Epoch settlement (16KB)
│       ├── obt_integration.rs        # KU↔OBT bridge + ReplicaSnapshot (14KB)
│       ├── obt_governance.rs         # ★ GovernanceConfig (15 params, 18 tests)
│       │
│       ├── tests.rs                    # Integration tests (~1185L)
│       ├── benchmark.rs               # Performance benchmarks (~1003L)
│       └── demo.rs                    # Demo scenarios (~1002L)
│
├── ku-net/                             # 🟢 Pillar 2+3 — ~9,500+ LOC | 192 tests
│   ├── src/
│   │   ├── lib.rs                      # 12 modules, 9 protocol layers
│   │   ├── constants.rs               # QUIC, SWIM, DHT, fitness constants (116L)
│   │   ├── error.rs                   # 5 error enums (178L)
│   │   ├── identity.rs               # BLAKE3 NodeId, Ed25519, crypto puzzle (245L)
│   │   ├── messages.rs               # 59 message types, 4B header (447L)
│   │   ├── membership.rs             # SWIM protocol, 7 tiers, fitness (408L)
│   │   ├── discovery.rs              # 6-layer bootstrap cascade (309L)
│   │   ├── dht.rs                    # Kademlia, 256 k-buckets (624L)
│   │   ├── stigmergy.rs             # Pheromone routing (302L)
│   │   ├── vacuum.rs                 # Bloom filter (314L)
│   │   ├── pubsub.rs                 # Topic management (269L)
│   │   ├── sync.rs                   # Delta-state CRDT sync (383L)
│   │   ├── transport.rs              # Real QUIC transport (457L)
│   │   ├── metabolism_gossip.rs      # ★ PoK v2 gossip handler (325L, 6 tests)
│   │   ├── tests.rs                  # 25+ unit tests (571L)
│   │   └── query/                    # 🟢 KQL Distributed Query Engine (~3,300 LOC)
│   │       ├── mod.rs                # Module registry
│   │       ├── index.rs              # ConceptIndex + VacuumFilter (209L)
│   │       ├── messages.rs           # Wire format 0x50-0x52, 0x40-0x41 (235L)
│   │       ├── router.rs             # 6-layer scope escalation (479L)
│   │       ├── merger.rs             # Dedup + trust×scope ranking (206L)
│   │       ├── watch.rs              # Standing queries + event filter (478L)
│   │       ├── cache.rs              # LRU query cache, BLAKE3 keys (253L)
│   │       ├── learning.rs           # Pheromone reinforcement learning (264L)
│   │       └── discovery/
│   │           ├── mod.rs             # Discovery submodules
│   │           ├── gaps.rs            # Knowledge gap detector (230L)
│   │           ├── bridges.rs         # Swanson ABC cross-domain finder (240L)
│   │           └── serendipity.rs     # Unknown unknowns engine (230L)
│   └── tests/
│       ├── integration.rs             # 8 E2E tests (406L)
│       ├── test_vectors.rs            # 12 wire format vectors (265L)
│       └── query_integration.rs       # 13 KQL pipeline + stress tests (330L)
│
├── ku-kql/                             # 🟢 Pillar 3 — ~2,300 LOC | 51 tests
│   └── src/
│       ├── lib.rs                      # 4 modules
│       ├── ast.rs                     # 6 query types, patterns, conditions (306L)
│       ├── parser.rs                  # nom-based parser, 6 query types (~800L)
│       ├── executor.rs               # Full executor with aggregation (~700L)
│       └── storage.rs                 # redb persistence (351L)
│
├── ku-ai/                              # 🟢 Pillar 6 — AI runtime
│   └── src/
│       ├── ollama.rs                   # OllamaBackend: REST API client
│       └── model_backend.rs           # ModelBackend trait abstraction
│
├── ku-encoder/                         # 🟢 Pillar 6 — AI encoding pipeline
│   └── src/
│       └── encoder.rs                  # AiEncoder: text → CoreDna via AI
│
├── ku-mediator/                        # 🟢 Pillar 6 — Personal AI orchestrator
│   └── src/
│       ├── mediator.rs                 # Intent routing, session management
│       └── retriever.rs               # Keyword search index
│
├── onebrain-protocol/                  # 🟢 Pillar 6 — Shared P2P protocol
│   └── src/
│       └── lib.rs                      # PeerMessage, SeedMessage, wire format
│
├── onebrain-seed/                      # 🟢 Pillar 6 — VPS seed node binary
│   └── src/
│       ├── main.rs                    # CLI entry (--port, --max-peers)
│       ├── registry.rs               # Peer tracking + heartbeat + cleanup
│       ├── relay.rs                   # Message forwarding
│       └── server.rs                  # TCP accept loop + routing
│
├── onebrain/                           # 🟢 Pillar 6+10 — Full client node binary
│   └── src/
│       ├── main.rs                    # Clap CLI + P2P startup sequence
│       ├── node.rs                    # OneBrainNode (storage + mediator + network)
│       ├── cli.rs                     # Interactive REPL
│       ├── config.rs                  # NodeConfig
│       ├── error.rs                   # NodeError
│       ├── network.rs                 # TCP transport + messages
│       ├── peer_manager.rs            # Peer tracking
│       ├── anti_gaming_guard.rs       # Rate limiting + quality gates
│       ├── verifier_service.rs        # Cross-node verification
│       ├── seed_client.rs             # Seed node client
│       ├── peer_memory.rs             # Remember peers (JSON)
│       ├── upnp.rs                    # UPnP stub
│       └── mdns_discovery.rs          # mDNS stub
│
└── ku-demo/                            # Demo (legacy) — ~930 LOC | 17 tests
    └── src/
        ├── main.rs                    # 10-step E2E demo (402L)
        ├── runtime.rs                 # OBPNode unified runtime (204L)
        └── testbed.rs                 # 3-node testbed (322L)
```

---

## Tài liệu & Nghiên cứu (60+ files, ~2.5 MB)

### 6 Rounds nghiên cứu

| Round | Files | Size | Chủ đề |
|-------|-------|------|--------|
| **R1** Foundation | 3 | 87KB | Semantic representation, distributed storage, distributed graphs |
| **R2** Deep Research | 4 | 78KB | Bio-inspired protocols, collective intelligence, scale analysis, Knowledge DNA |
| **R3** Technical Design | 4 | 50KB | Storage design, graph schema, security (NO blockchain → CID + Ed25519), scale modeling |
| **R4** Deep Dive | 4 | 49KB | New gene/edge types, bio-inspired trust, optimizations, serendipity engine |
| **R5** Query & Registry | 11 | 254KB | Rust selection, registry governance, distributed registry, polysemy, KQL design |
| **R6** Network Protocol | 14 | 687KB | Transport, topology, distribution, scale analysis, 4 formal specs (**SPEC A-D**), OBP description |

### Formal Specifications

| Spec | Size | Nội dung |
|------|------|---------|
| **UKRL v4** | 111KB | 10 gene types, 33 edge types, Trust layer, 11 epistemic levels, KRL maturity scale |
| **Concept Registry v1** | 151KB | Complete specification |
| **KQL v1** | 69KB | Knowledge Query Language specification |
| **SPEC A** | 84KB | Identity + Transport |
| **SPEC B** | 81KB | Overlay + Routing |
| **SPEC C** | 72KB | Query + Security |
| **SPEC D** | 57KB | Message Catalog (56 types) |
| **OBT Specs** | 9 files | Ledger, Minting, Storage, Anti-Gaming, Transfer, Security, Penalty, Constants |
| **OBT Paper** | 11 chapters | Full academic paper: ~3,000 lines, 45 tables, 13 figures |
| **Papers** | 5 pillars | KU, Network, KQL, PoK, OBT — full academic papers |

### Phân tích & Review

| File | Size | Nội dung |
|------|------|---------|
| [analysis_results.md](file:///c:/Users/shpy2/Documents/OneBrain/.analysis/analysis_results.md) | 14KB | 8 nhóm vấn đề kỹ thuật, top 10 priorities |
| [detailed_technical_analysis.md](file:///c:/Users/shpy2/Documents/OneBrain/.analysis/detailed_technical_analysis.md) | 46KB | 15 nhóm, 67 vấn đề cụ thể, P0-P3 priority |
| [review_v1.0](file:///c:/Users/shpy2/Documents/OneBrain/.analysis/reviews) | 14KB | Code review 28 files, 73 tests, 17/36 issues fixed |

---

## Đánh giá tổng thể

### ✅ Điểm mạnh xuất sắc

| # | Điểm mạnh | Chi tiết |
|---|-----------|---------|
| 1 | **Protocol hoàn chỉnh** | 9-layer network stack với QUIC + Kademlia + SWIM + Stigmergy — rất ít project nào đạt được |
| 2 | **Bio-inspired design** | Pheromone routing, fitness-based tiers, epigenetic decay — paradigm shift so với traditional P2P |
| 3 | **Wire efficiency** | **Core DNA v6**: ~16-88B per KU (16.5× smaller than CBOR v5), 32 opcodes, varint encoding, CRC-16 validation |
| 4 | **CRDT foundation** | GCounter, PNCounter, LWW, ORSet, VectorClock — sẵn sàng cho distributed consensus |
| 5 | **KQL innovation** | Ngôn ngữ truy vấn riêng cho knowledge — unique trong lĩnh vực |
| 6 | **PoK v2 (PoMV)** | Proof-of-Metabolic-Value — phát minh mới, observation-based, no voting, 6 tín hiệu |
| 7 | **Test coverage** | 733 tests covering encoding, networking, wire format, query, discovery, consensus, OBT token, integration |
| 8 | **Nghiên cứu sâu** | 1.55MB research across 45+ documents, 6 rounds, formal specifications |
| 9 | **Scalability design** | 100B+ node capacity, mobile-first energy-conscious, serverless |

### ⚠️ Gaps cần giải quyết

| # | Gap | Impact | Priority |
|---|-----|--------|----------|
| ~~1~~ | ~~**PoK Engine chưa code**~~ | ✅ **ĐÃ HOÀN THÀNH** — PoK v2 (PoMV), 16 modules, 157 tests | ✅ Done |
| ~~2~~ | ~~**OBT Token chưa tồn tại**~~ | ✅ **~95% HOÀN THÀNH** — 11 modules, 264+ tests | 🟢 ~95% |
| ~~3~~ | ~~**Whitepaper chưa viết**~~ | ✅ **5 papers đã viết** | ✅ Done |
| ~~4~~ | ~~**OBT remaining ~20%**~~ | ✅ DHT wiring + Ed25519 + GovernanceConfig done | 🟢 ~95% |
| ~~5~~ | ~~**AI Layer = 0 code**~~ | ✅ **~75% HOÀN THÀNH** — 3 crates, 17 files, full pipeline + P2P | 🟢 ~75% |
| ~~6~~ | ~~**UI chưa có**~~ | ✅ **CLI node hoàn chỉnh** — REPL + P2P + storage | 🟢 ~25% |
| 1 | **Graph database chưa tích hợp** | Bonds/edges exist in code nhưng chưa queryable | 🟠 High |
| 2 | **Web/Mobile App chưa có** | CLI chỉ dùng được cho dev/demo | 🟡 Medium |
| 3 | **UPnP + mDNS chưa active** | Stub only, cần crate integration | 🟡 Medium |
| 4 | **Team = 1 người** | Phân tích ước tính cần **14-22 người** | 🔴 Critical |

---

## Tiến độ so với Roadmap

```mermaid
gantt
    title OneBrain Progress
    dateFormat  YYYY-MM
    axisFormat  %Y

    section Phase 1 Foundation
    KU Schema + Wire Format        :done, p1a, 2025-06, 2025-08
    Network Protocol 9 layers      :done, p1b, 2025-06, 2026-06
    KQL Full Implementation         :done, p1c, 2025-08, 2026-06
    Research 6 rounds              :done, p1d, 2025-07, 2026-06
    CRDT Primitives                :done, p1e, 2026-01, 2026-06
    Storage redb                   :done, p1f, 2026-03, 2026-06
    PoK v2 PoMV Engine             :done, p1g, 2026-06, 2026-06
    Papers (5 pillars)             :done, p1h, 2026-06, 2026-07

    section Phase 2 Alpha
    OBT Token Engine               :done, p2a, 2026-06, 2026-07
    OBT gaps (DHT, Ed25519, Gov)   :done, p2a2, 2026-07, 2026-07
    AI Layer + P2P Node            :done, p2c, 2026-07, 2026-07
    Seed Node + Protocol           :done, p2c2, 2026-07, 2026-07
    Knowledge Graph integration    :p2b, 2026-08, 2027-03
    Web App prototype              :p2d, 2027-01, 2027-06
```

> [!IMPORTANT]
> **Dự án đã hoàn thành Phase 1 + PoK v2 + OBT ~95% + AI Layer ~75%!** Sáu trụ cột nền tảng (KU 95% + Network 95% + KQL 95% + PoK v2 95% + OBT 95% + **AI 75%**) đều ở trạng thái hoạt động. Codebase: 35,000+ LOC, **757+ tests**, 10 crates. `onebrain` CLI node chạy được multi-node demo thực tế với local AI + P2P networking.

---

## Bước tiếp theo (đề xuất)

| # | Bước | Mô tả | Status |
|---|------|-------|--------|
| ~~🥇~~ | ~~**AI Layer + P2P Node**~~ | ✅ 3 crates (onebrain, onebrain-seed, onebrain-protocol), 17 files | **~75% DONE** |
| ~~🥈~~ | ~~**OBT Token**~~ | ✅ 11 modules, 264+ tests, Ed25519, GovernanceConfig | **~95% DONE** |
| ~~🥉~~ | ~~**PoK Engine**~~ | ✅ PoK v2 (PoMV), 16 modules, 157 tests | **DONE** |
| 1️⃣ | **Deploy seed + test remote** | Deploy onebrain-seed lên n1/n2.onebrain.live, test multi-node xa | VPS sẵn sàng |
| 2️⃣ | **UPnP + mDNS activation** | Kích hoạt crate `igd-next` + `mdns-sd` cho auto port/LAN discovery | Stub đã có |
| 3️⃣ | **Knowledge Graph DB** | Tích hợp graph database để 33 bond types traversal/query | Bond types đã code |
| 4️⃣ | **Web Dashboard** | Visualize Knowledge Graph, browse KUs, demo contribution flow | Cần để attract community |
| 5️⃣ | **Mobile App** | Android/iOS client node | Cần cho mass adoption |
