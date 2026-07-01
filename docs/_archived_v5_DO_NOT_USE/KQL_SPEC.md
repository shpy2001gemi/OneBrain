# KQL — Knowledge Query Language Specification

> **Version**: 2.0 — KU v6 Core DNA  
> **Status**: Specification (v6 redesign in progress)  
> **Depends on**: KU_CORE_DNA_V6_SPEC.md, KU_ARCHITECTURE.md

## 1. Overview

KQL (Knowledge Query Language) is a declarative query language for the OneBrain decentralized knowledge network. It enables:

- **Local queries**: Search Knowledge Units (KUs) on the local node.
- **Distributed queries**: Route queries across the P2P network via 6-layer scope escalation.
- **Standing queries (WATCH)**: Register persistent queries that fire on matching events.
- **Knowledge discovery**: Proactive gap detection, bridge finding, and serendipitous discovery.

### 1.1 KU v6 Data Model

KQL operates on `KuRuntime` — a 3-layer composite struct that reflects the KU 3-layer architecture:

```
KuRuntime
├── cid: [u8; 32]              # BLAKE3 content identity (hash of wire_bytes)
├── dna: CoreDna                # Layer 1 — Core DNA (stored, compact binary)
│   ├── header.version          #   Format version (current = 1)
│   ├── header.gene_type        #   Gene type (0-10, e.g., 0=Fact, 3=Procedure)
│   ├── header.has_qualifiers   #   Whether instructions contain qualifiers
│   └── instructions: Vec<Instruction>  #   32 opcode types
│       ├── Triple { s, p, o }
│       ├── Quality { s, q }
│       ├── Quantity { s, value, unit }
│       ├── PartOf { part, whole }
│       ├── Step { ord, action, target }
│       ├── Certainty { level }
│       ├── ... (32 opcodes total)
│       └── End
├── epi: Option<Epigenetics>    # Layer 2 — Epigenetics (runtime, not persisted)
│   ├── trust: TrustSection     #   PoMV 6 scores + 13 other fields
│   ├── bonds: Vec<Bond>        #   33 directed bond types
│   ├── epistemic_status        #   11 levels (Rumor → Axiomatic)
│   ├── evidence_type           #   9 GRADE-aligned types
│   └── epigenetic: EpigeneticSection  #   Embeddings, decay, metadata
├── expr: Option<Expression>    # Layer 3 — Expression (generated on-demand)
│   ├── text: String            #   Natural language rendering
│   └── lang: String            #   Language code ("vi", "en", etc.)
└── wire_bytes: Vec<u8>         # Raw Core DNA binary (for storage/transport)
```

### 1.2 Queryable Fields

KQL fields are organized by layer:

| Field | Layer | Type | Source |
|-------|-------|------|--------|
| `k.gene_type` | Core DNA | String | `header.gene_type` → name |
| `k.concept_ids` | Core DNA | `Vec<u64>` | All ConceptIDs in instruction stream |
| `k.primary_concept` | Core DNA | `u64` | First subject ConceptID in first instruction |
| `k.certainty` | Core DNA | `u16` | From `Instruction::Certainty { level }` |
| `k.instruction_count` | Core DNA | `usize` | `instructions.len()` |
| `k.has_triple` | Core DNA | `bool` | Any `Instruction::Triple` exists |
| `k.has_step` | Core DNA | `bool` | Any `Instruction::Step` exists |
| `k.trust_score` | Epigenetics | `u16` | `epi.trust.trust_score` |
| `k.confidence` | Epigenetics | `u16` | `epi.trust.confidence` |
| `k.epistemic_status` | Epigenetics | String | `epi.epistemic_status` → name |
| `k.bond_count` | Epigenetics | `usize` | `epi.bonds.len()` |
| `k.metabolic_rate` | Epigenetics | `u16` | `epi.trust.metabolic_rate` (PoMV) |
| `k.text` | Expression | String | Lazy-generated from CoreDna + ConceptDict |
| `k.cid` | Runtime | `[u8;32]` | BLAKE3 content ID |
| `k.wire_size` | Runtime | `usize` | `wire_bytes.len()` |

> **Note**: Field extraction from Core DNA requires scanning the instruction stream (O(n) where n = instruction count, typically 3-8). This is fast due to cache locality on small Vec.

## 2. Syntax

### 2.1 FIND — Search for Knowledge Units

```
FIND (k:KU) [WHERE condition]
             [SCOPE scope]
             [ORDER BY field [ASC|DESC]]
             [LIMIT n]
             [RETURN expression]
```

**Examples:**
```kql
-- Core DNA fields
FIND (k:KU) WHERE k.gene_type = "Fact" AND k.certainty > 8000 LIMIT 10
FIND (k:KU) WHERE k.concept_ids CONTAINS 301 SCOPE CLUSTER
FIND (k:KU) WHERE k.primary_concept = 600 ORDER BY k.trust_score DESC

-- Epigenetics fields
FIND (k:KU) WHERE k.trust_score > 5000 SCOPE CLUSTER LIMIT 10
FIND (k:KU) WHERE k.epistemic_status = "Validated"
FIND (k:KU) WHERE k.metabolic_rate > 3000

-- Aggregation
FIND (k:KU) RETURN COUNT(k.cid), AVG(k.trust_score)

-- Expression (triggers lazy text generation)
FIND (k:KU) WHERE k.concept_ids CONTAINS 301 RETURN k.text
```

### 2.2 CREATE — Create a Knowledge Unit

KQL supports **2-tier CREATE**:

#### Tier 1: Structured CREATE (offline, no AI needed)

Directly builds `CoreDna` from structured clauses. Requires ConceptDict (pre-downloaded SQLite) for concept name → ID resolution.

```kql
-- Simple fact
CREATE (k:KU) FACT certainty=9000 {
    TRIPLE(water, boils_at, 100_celsius)
    LOCATED(water, sea_level)
}

-- Procedure with steps
CREATE (k:KU) PROCEDURE {
    STEP(1, enter, water)
    STEP(2, kick, legs)
    STEP(3, pull, arms)
    PRECOND(know_swimming)
    EFFECT(move_forward)
}

-- Quantity with tolerance
CREATE (k:KU) FACT certainty=9500 {
    QUANTITY(wing_chord, 3.2, meters)
    TOLERANCE(wing_chord, 3.2, 0.05)
}
```

**Clause types** (map 1:1 to Core DNA opcodes):

| Clause | Opcode | Operands |
|--------|--------|----------|
| `TRIPLE(s, p, o)` | 0x00 | Subject, Predicate, Object |
| `QUALITY(s, q)` | 0x01 | Subject, Quality |
| `QUANTITY(s, val, unit)` | 0x02 | Subject, NumericValue, Unit |
| `PARTOF(part, whole)` | 0x04 | Part, Whole |
| `LOCATED(s, loc)` | 0x05 | Subject, Location |
| `TEMPORAL(s, time)` | 0x06 | Subject, Time |
| `CAUSAL(cause, effect)` | 0x07 | Cause, Effect |
| `STEP(ord, action, target)` | 0x13 | Order, Action, Target |
| `PRECOND(concept)` | 0x14 | Concept |
| `EFFECT(concept)` | 0x15 | Concept |
| `CERTAINTY(level)` | 0x10 | Level (0-10000) |
| `TOLERANCE(s, val, delta)` | 0x0D | Subject, Value, ±Delta |
| `RANGE(s, min, max)` | 0x0C | Subject, Min, Max |
| `CONSTRAINT(s, op, t)` | 0x0E | Subject, Operator, Target |

**ConceptDict resolution**: Text names (e.g., `water`) are resolved to ConceptIDs via the local SQLite-backed ConceptDict. Unknown concepts are auto-registered with the next available ID in the appropriate tier.

#### Tier 2: Natural Text CREATE (AI local)

Encodes natural language text into CoreDna via local AI model + 15 function-calling tools.

```kql
-- Vietnamese text
CREATE FROM TEXT "Nước sôi ở 100 độ C tại mực nước biển"
    WITH AI model="gemma4"

-- English text, multiple KUs
CREATE FROM TEXT "The rocket body is made of aluminum-lithium alloy 
    for weight optimization while maintaining structural strength."
    WITH AI model="gemma4"

-- Specify gene type hint
CREATE FROM TEXT "First, enter the water. Then kick your legs."
    WITH AI model="gemma4" gene_hint="Procedure"
```

**Requirements**: Local AI model installed (Gemma 4, Qwen, Phi-3, or any model supporting JSON function calling).

### 2.3 UPDATE — Modify Epigenetics Fields

Only Epigenetics layer fields can be updated (Core DNA is immutable after creation):

```kql
UPDATE (k:KU) SET k.trust_score = 9000, k.confidence = 8500
              WHERE k.primary_concept = 301
              SIGNED BY "did:ob:abc123"
```

**Updatable fields** (Epigenetics layer only):

| Field | Type | Notes |
|-------|------|-------|
| `k.trust_score` | u16 | Trust section |
| `k.confidence` | u16 | Trust section |
| `k.epistemic_status` | String | 11 levels |
| `k.evidence_type` | String | 9 types |
| `k.verification_level` | u16 | Trust section |

> **Core DNA is immutable**: To change the knowledge content, create a new KU and deprecate the old one. This mirrors DNA biology — mutations create new sequences, not modify existing ones.

### 2.4 DEPRECATE — Mark Knowledge as Deprecated

```kql
DEPRECATE (k:KU) WHERE k.primary_concept = 301
                  REASON "Superseded by newer research"
                  SIGNED BY "did:ob:abc123"
```

Sets `epistemic_status = Rumor`, `trust_score = 0`, `verification_level = 0` in Epigenetics layer.

### 2.5 WATCH — Standing Queries

```kql
WATCH FIND (k:KU) WHERE k.trust_score > 7000
      ON CREATE
      NOTIFY "callback://my-agent"
```

**Events:** `CREATE` | `UPDATE` | `DEPRECATE` | `ANY`

### 2.6 EXPLAIN — Query Plan Inspection

```kql
EXPLAIN FIND (k:KU) WHERE k.trust_score > 5000 SCOPE DHT
```

Returns: estimated scope, strategy, indexes used, instruction scan cost.

## 3. Conditions

| Operator | Example | Description |
|----------|---------|-------------|
| `>` | `k.trust_score > 5000` | Greater than |
| `<` | `k.trust_score < 3000` | Less than |
| `=` | `k.gene_type = "Fact"` | Equality |
| `>=`, `<=` | `k.confidence >= 7000` | Comparison |
| `!=` | `k.gene_type != "Hypothesis"` | Not equal |
| `CONTAINS` | `k.concept_ids CONTAINS 301` | ConceptID in instruction stream |
| `EXISTS` | `k.epi EXISTS` | Layer/field existence |
| `AND` | `condition1 AND condition2` | Logical AND |
| `OR` | `condition1 OR condition2` | Logical OR |

## 4. Scope Levels

Queries are routed through 6 escalation layers:

| Layer | Scope | TTL | Strategy |
|-------|-------|-----|----------|
| L0 | `LOCAL` | 0 | Execute on self |
| L1 | `NEIGHBORS` | 1 | 1-hop SWIM peers (fanout=5) |
| L2 | `CLUSTER` | 3 | Super-peer routing |
| L3 | `DHT` | 8 | Kademlia concept key lookup |
| L4 | `SEMANTIC` | 5 | Stigmergy pheromone trails |
| L5 | `GLOBAL` | 12 | Random walk + TTL flooding |

**Auto-escalation:** If a scope returns insufficient results, the router automatically escalates to `next_scope()`.

## 5. Aggregation Functions

| Function | Example | Description |
|----------|---------|-------------|
| `COUNT(field)` | `COUNT(k.cid)` | Count results |
| `SUM(field)` | `SUM(k.trust_score)` | Sum numeric field |
| `AVG(field)` | `AVG(k.trust_score)` | Average |
| `MIN(field)` | `MIN(k.confidence)` | Minimum |
| `MAX(field)` | `MAX(k.trust_score)` | Maximum |

## 6. Wire Format

### 6.1 QueryForwardMsg (0x50)

```
{
    query_id:      [u8; 16],       // BLAKE3(kql) XOR origin
    origin:        NodeId,         // 32-byte originator
    kql:           String,         // Raw KQL string
    scope:         u8,             // 0=Local..5=Global
    ttl:           u8,             // Hop counter
    max_results:   u32,            // Result limit
    concept_hints: Vec<u64>,       // DHT routing hints (ConceptIDs)
    visited:       Vec<NodeId>,    // Loop prevention
}
```

### 6.2 QueryResponseMsg (0x51)
```
{
    query_id:        [u8; 16],
    responder:       NodeId,
    results_payload: Vec<u8>,      // Core DNA wire bytes (concatenated KUs)
    result_count:    u32,
    scope:           u8,
}
```

> **v6 change**: `results_cbor` → `results_payload`. Payload contains Core DNA wire format bytes, not CBOR-encoded `KnowledgeUnit` structs.

### 6.3 QueryCancelMsg (0x52)
```
{
    query_id: [u8; 16],
    reason:   u8,   // 0=Timeout, 1=Enough, 2=UserCancel
}
```

## 7. Storage Architecture

### 7.1 Primary Storage — Core DNA bytes → redb

```
Table: "kus"
Key:   CID ([u8; 32])             → BLAKE3 hash of Core DNA wire bytes
Value: wire_bytes (Vec<u8>)       → Raw Core DNA binary (16-172B typical)
```

### 7.2 Epigenetics Storage — SQLite

```sql
CREATE TABLE epigenetics (
    cid              BLOB PRIMARY KEY,  -- 32 bytes
    trust_score      INTEGER,
    confidence       INTEGER,
    epistemic_status INTEGER,
    evidence_type    INTEGER,
    metabolic_rate   INTEGER,
    -- ... other TrustSection fields
    bonds_cbor       BLOB,              -- CBOR-encoded Vec<Bond>
    updated_at       INTEGER            -- Unix timestamp
);
```

### 7.3 ConceptDict — SQLite (pre-downloaded)

```sql
CREATE TABLE concepts (
    concept_id  INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,           -- Default/canonical name
    name_vi     TEXT,                    -- Vietnamese
    name_en     TEXT,                    -- English
    tier        INTEGER NOT NULL,        -- 0-4 (varint tier)
    category    TEXT                     -- Domain category
);

-- Indexes for fast lookup
CREATE INDEX idx_name ON concepts(name);
CREATE INDEX idx_name_vi ON concepts(name_vi);
CREATE INDEX idx_name_en ON concepts(name_en);
```

**Distribution**: ConceptDict SQLite file (~50MB for ~16K core concepts) is downloaded once during node initialization. New concepts are registered locally and synced via OBP gossip protocol.

## 8. Discovery Engine

### 8.1 Gap Detection

Identifies missing knowledge:
- **Orphan concepts**: Referenced in instructions but never defined as subjects
- **Low-confidence regions**: Clusters of low-trust KUs (via Epigenetics scan)
- **Missing evidence**: Trust without corroboration
- **Untested hypotheses**: No challenges or corroboration

Each gap generates a suggested KQL query to fill it.

### 8.2 Cross-Domain Bridges (Swanson ABC Model)

```
Domain A ←→ Bridge Concept B ←→ Domain C
(strong)     (shared concept)     (weak/unknown)
```

Discovers undiscovered public knowledge by finding shared ConceptIDs connecting well-known domains to unexplored domains.

### 8.3 Serendipity Engine

Scores candidates by: `serendipity = relevance × novelty`

- **Relevance**: How related is this KU to user's interest profile?
- **Novelty**: How new/unexpected is this? (Bell curve — partial novelty > total novelty)

## 9. Optimization

### 9.1 Query Cache

LRU cache with BLAKE3-keyed normalized KQL strings.
- Case-insensitive, whitespace-collapsed matching
- Configurable TTL and capacity
- Hit rate statistics

### 9.2 Pheromone Learning

Ant colony-inspired reinforcement learning for query routing:
- **Success** → increase pheromone on that scope
- **Failure** → decrease pheromone
- **Evaporation** → decay toward neutral over time
- **Bounds** → [0.05, 0.95] prevent route starvation

### 9.3 ConceptID Index

Core DNA instruction scanning is O(n) per KU. To accelerate FIND queries with `concept_ids CONTAINS`, nodes maintain a **ConceptID index**:

```
HashMap<ConceptId, Vec<CID>>
```

This index is built incrementally as KUs are stored. A `CONTAINS` query becomes O(1) lookup instead of scanning all KUs.

## 10. Implementation Status

| Component | File | Tests | Status |
|-----------|------|-------|--------|
| Parser | `ku-kql/src/parser.rs` | 28 | ✅ (v6 grammar update needed) |
| AST | `ku-kql/src/ast.rs` | — | ✅ (v6 nodes needed) |
| Executor | `ku-kql/src/executor.rs` | 23 | ⚠️ (v6 KuRuntime refactor needed) |
| Storage | `ku-kql/src/storage.rs` | — | ⚠️ (v6 Core DNA + SQLite needed) |
| ConceptIndex | `ku-net/src/query/index.rs` | 7 | ✅ |
| Wire Messages | `ku-net/src/query/messages.rs` | 5 | ⚠️ (field rename needed) |
| QueryRouter | `ku-net/src/query/router.rs` | 6 | ✅ |
| ResultMerger | `ku-net/src/query/merger.rs` | 7 | ✅ |
| WatchEngine | `ku-net/src/query/watch.rs` | 9 | ✅ |
| GapDetector | `ku-net/src/query/discovery/gaps.rs` | 6 | ✅ |
| BridgeFinder | `ku-net/src/query/discovery/bridges.rs` | 3 | ✅ |
| SerendipityEngine | `ku-net/src/query/discovery/serendipity.rs` | 6 | ✅ |
| QueryCache | `ku-net/src/query/cache.rs` | 9 | ✅ |
| PheromoneLearner | `ku-net/src/query/learning.rs` | 8 | ✅ |
| Integration Tests | `ku-net/tests/query_integration.rs` | 13 | ✅ |
