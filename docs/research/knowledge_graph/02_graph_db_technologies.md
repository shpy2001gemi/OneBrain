# Graph Database Technologies — Survey for OBKG

> **Author**: OneBrain Research Team  
> **Date**: 2026-07-02  
> **Purpose**: Evaluate graph database technologies for OBKG's storage and query layer

---

## Executive Summary

This survey evaluates 12 graph database technologies to identify the best storage, indexing, and query patterns for OneBrain's Knowledge Graph (OBKG). OBKG currently uses **redb** (embedded Rust B+tree KV store) with 4 tables. The key limitation is that bonds are stored as `Vec<Bond>` inside each KU's Epigenetics JSON — NO dedicated edge index. This means finding "all KUs that point to KU-X" requires scanning ALL KUs.

**Primary Recommendation**: Build a native "Biological Graph Engine" on redb, borrowing the best ideas from each system studied. Add 6 new edge index tables for O(1) adjacency lookups. Adopt Datalog-inspired recursive queries from CozoDB. Follow TerminusDB's delta-layer concept for CRDT sync.

**Most Relevant Technologies**: SurrealDB (Rust-native embedded), CozoDB (Datalog + cozo-redb fork exists!), TerminusDB (git-for-data, most architecturally aligned), Oxigraph (Rust-native binary encoding).

---

## Technology Survey (12 Systems)

### 1. Neo4j + Cypher

| Aspect | Details |
|--------|---------|
| **Data Model** | Labeled property graph |
| **Query Language** | Cypher (declarative, pattern matching) |
| **Storage Engine** | Native graph storage with **index-free adjacency** — each node stores direct physical pointers to neighbors |
| **Index Structures** | B-tree/range indexes for entry-point lookup, then pointer-chasing for traversal (O(1) per hop) |
| **Scalability** | Infinigraph (horizontal, property sharding) |
| **Embeddability** | No (JVM-based server only) |
| **Rust** | No native Rust |
| **Key Innovation** | Index-free adjacency — O(1) traversal per hop. Native vector types (4096 dims) for AI/RAG |
| **Lesson for OBKG** | Cypher's pattern matching syntax inspired KQL's edge patterns. Index-free adjacency concept is excellent for local traversal but complex in distributed P2P. Consider hybrid approach: index-free for local hot path, index-based for P2P |

### 2. TigerGraph + GSQL

| Aspect | Details |
|--------|---------|
| **Data Model** | Property graph (vertices + edges as computation mesh) |
| **Query Language** | GSQL — Turing-complete, SQL-like, compiled to parallel code |
| **Storage Engine** | Native Parallel Graph (NPG). Custom C++ engine. Ingests 50-150 GB/hr/machine |
| **Scalability** | Massively parallel (BSP model). Horizontal sharding |
| **Embeddability** | No (enterprise cluster-oriented) |
| **Rust** | No |
| **Key Innovation** | Every vertex/edge is both storage and compute unit. Bulk Synchronous Parallel model. TigerVector for vector+graph fusion |
| **Lesson for OBKG** | BSP model interesting for P2P gossip-based graph analytics. GSQL's compiled query approach could inspire KQL query compilation |

### 3. Amazon Neptune

| Aspect | Details |
|--------|---------|
| **Data Model** | Dual — Property graph (Gremlin/openCypher) AND RDF (SPARQL) |
| **Query Language** | Gremlin, openCypher, SPARQL |
| **Storage Engine** | Purpose-built cloud storage. Decoupled compute/storage |
| **Scalability** | Auto-scaling to 128 TiB. 15 read replicas |
| **Embeddability** | No (AWS managed service only) |
| **Key Innovation** | Dual-model (property graph + RDF) in single managed service. 6x replication across 3 AZs |
| **Lesson for OBKG** | Dual-model concept valuable — OBKG could expose both property-graph-like API (bonds) and RDF-like API (Core DNA triples) from same storage |

### 4. Dgraph + GraphQL

| Aspect | Details |
|--------|---------|
| **Data Model** | Property graph with native GraphQL schema |
| **Query Language** | DQL (GraphQL-derived) + native GraphQL API |
| **Storage Engine** | **Badger** (Go) — LSM tree KV store (WiscKey design) |
| **Key Innovation** | "GraphQL-native". Predicate-based sharding (each predicate distributed independently) |
| **Lesson for OBKG** | Predicate-based sharding maps well to RelationType-based distribution. A node could specialize in certain relation categories |

### 5. JanusGraph

| Aspect | Details |
|--------|---------|
| **Data Model** | Property graph (TinkerPop-compliant) |
| **Query Language** | Gremlin |
| **Storage Engine** | **Modular** — pluggable backends (Cassandra, HBase, BerkeleyDB). Adjacency-list-in-a-row model |
| **Key Innovation** | Adjacency-list-per-vertex model on top of wide-column stores. Backend-agnostic graph abstraction |
| **Lesson for OBKG** | **JanusGraph's adjacency-list-per-vertex model is closest to OBKG's current bond storage** (Vec<Bond> per KU). Shows how to build graph semantics on KV storage |

### 6. ArangoDB + AQL

| Aspect | Details |
|--------|---------|
| **Data Model** | Multi-model — document + graph + key-value |
| **Query Language** | AQL (declarative, unified across models) |
| **Storage Engine** | **RocksDB** (LSM tree). Document-level locking |
| **Key Innovation** | True multi-model — single AQL query spans documents, graphs, and KV lookups. **Edges ARE documents** (with properties) |
| **Lesson for OBKG** | **Edges-as-documents pattern directly parallels OBKG's bonds-as-rich-structs**. AQL's unified multi-model approach validates KQL's aspiration to query both content and relationships |

### 7. SurrealDB (Rust) ★★★★★

| Aspect | Details |
|--------|---------|
| **Data Model** | Multi-model — document + graph + relational + time-series + KV + geospatial |
| **Query Language** | SurrealQL (SQL-like with RELATE for graph edges, `->purchased->product` traversal) |
| **Storage Engine** | Pluggable — RocksDB, in-memory, distributed backends |
| **Scalability** | Single-node to distributed cluster |
| **Embeddability** | **Yes** — full in-process embedded mode. WASM support |
| **Rust** | **Rust-native** |
| **Key Innovation** | Rust multi-model that scales from embedded to distributed. RELATE statement creates graph edges. Row-level permissions |
| **Lesson for OBKG** | **Most directly relevant**. Proves Rust embedded multi-model is viable. SurrealQL's RELATE could inspire KQL bond creation syntax. Embedded-to-distributed scaling model matches OBKG's local-first → P2P sync |

### 8. Oxigraph (Rust) ★★★★

| Aspect | Details |
|--------|---------|
| **Data Model** | RDF (triples/quads) |
| **Query Language** | SPARQL 1.1 |
| **Storage Engine** | **RocksDB** backend. Binary encoding of RDF terms (type byte + 32-byte hash) |
| **Embeddability** | **Yes** — pure Rust library. Python/JS (WASM) bindings |
| **Rust** | **Rust-native** (modular crates: oxrdf, oxrdfio, spargebra, spareval) |
| **Key Innovation** | Compact binary encoding. Modular Rust crate design — can reuse individual parsing/evaluation crates independently |
| **Lesson for OBKG** | Oxigraph's **binary encoding scheme** (type byte + hash) is directly applicable. Modular crate design validates OBKG's ku-core/ku-kql/ku-net separation. RDF triple model parallels `Instruction::Triple{s,p,o}`. Multi-index strategy applicable to redb |

### 9. IndraDB (Rust) ★★★

| Aspect | Details |
|--------|---------|
| **Data Model** | Directed typed property graph (inspired by Facebook TAO) |
| **Storage Engine** | Pluggable — **RocksDB**, **Sled**, PostgreSQL |
| **Embeddability** | **Yes** — embeddable Rust library OR gRPC server |
| **Rust** | **Rust-native** |
| **Key Innovation** | Pluggable storage trait. Facebook TAO-inspired simplicity |
| **Lesson for OBKG** | IndraDB's **pluggable storage trait** directly maps to what OBKG needs — define a `GraphStorage` trait over redb. TAO's directed typed graph model almost identical to OBKG's Bond model |

### 10. CozoDB (Rust) ★★★★★

| Aspect | Details |
|--------|---------|
| **Data Model** | Relational-graph-vector hybrid |
| **Query Language** | **CozoScript (Datalog dialect)** — logic programming, recursive queries, built-in graph algorithms |
| **Storage Engine** | Pluggable — RocksDB, SQLite, in-memory, TiKV. **cozo-redb fork exists!** |
| **Scalability** | Single-node (RocksDB/SQLite) or distributed (TiKV) |
| **Embeddability** | **Yes** — SQLite-like embedded design. Rust/Python/Node/Java/C/WASM bindings |
| **Rust** | **Rust-native** |
| **Key Innovation** | **Datalog for graph queries** — recursive path queries as first-class primitives. Time-travel queries. Built-in graph algorithms (PageRank, shortest path, community detection). **cozo-redb fork exists!** |
| **Lesson for OBKG** | **Strongest candidate for inspiration**. Datalog's recursive query capability solves KQL's multi-hop traversal needs. The cozo-redb fork proves graph queries CAN run on redb. Time-travel maps to CRDT versioning. Graph algorithms as built-ins (PageRank = trust propagation, community detection = cluster discovery) directly serve OBKG |

### 11. Apache AGE (PostgreSQL)

| Aspect | Details |
|--------|---------|
| **Data Model** | Property graph (stored as PostgreSQL tables) |
| **Query Language** | openCypher (integrated with SQL) |
| **Key Innovation** | Graph queries on top of relational storage with zero data movement. Hybrid SQL + openCypher |
| **Lesson for OBKG** | **Proves graph semantics work as a layer on top of tabular/KV storage** — exactly OBKG's architecture. AGE's approach of compiling Cypher into execution plans informs how KQL could be compiled into redb range queries |

### 12. TerminusDB ★★★★★

| Aspect | Details |
|--------|---------|
| **Data Model** | Document-oriented graph (JSON documents in schema-enforced graph) |
| **Query Language** | WOQL (Datalog variant), GraphQL, REST APIs |
| **Storage Engine** | **terminus-store** (Rust!) — immutable, append-only delta layers. Succinct data structures |
| **Embeddability** | Partial (Prolog core + Rust storage) |
| **Rust** | **Storage layer (terminus-store) is Rust** |
| **Key Innovation** | **Git-for-data** — branch, merge, diff, time-travel on graph data. Delta encoding (only store changes). Lock-free concurrency (immutable committed layers). ACID per commit |
| **Lesson for OBKG** | **MOST ARCHITECTURALLY ALIGNED with OneBrain**. Delta-encoded, immutable layers parallel OBKG's content-addressed immutable Core DNA + mutable Epigenetics. Branch/merge/diff maps to CRDT sync. Delta rollups = CRDT compaction. Git-like clone/sync maps to P2P replication |

---

## Storage Format Deep-Dive: Modeling Graphs on redb

### Current Tables (keep)

```
1. TABLE_KUS:           CID(32B) → CoreDNA wire bytes
2. TABLE_EPI:           CID(32B) → Epigenetics JSON (bonds + trust)
3. TABLE_INDEX_TRUST:   trust_score(2B)+CID(32B) → ∅
4. TABLE_INDEX_CONCEPT: concept_id(8B)+CID(32B) → ∅
```

### Proposed New Graph Tables (+6)

```
5. TABLE_EDGES_OUT:   source_cid(32B)+relation(1B)+target_cid(32B) → BondMeta
   // Forward adjacency: "what does KU-X point to?"
   // Range scan on source_cid prefix = all outgoing edges

6. TABLE_EDGES_IN:    target_cid(32B)+relation(1B)+source_cid(32B) → ∅
   // Reverse adjacency: "what points to KU-X?"
   // Range scan on target_cid prefix = all incoming edges

7. TABLE_EDGES_TYPE:  relation(1B)+source_cid(32B)+target_cid(32B) → ∅
   // Type index: "all CAUSES edges in the system"
   // Range scan on relation prefix

8. TABLE_INDEX_STATE: state(1B)+cid(32B) → ∅
   // Edge state: find all Active/Weakened/Deprecated edges

9. TABLE_BOND_WEIGHT: weight(2B BE)+source_cid(32B)+target_cid(32B) → ∅
   // Weight index: "strongest bonds in the system"

10. TABLE_EDGE_TIME:  created_at(4B BE)+source_cid(32B)+target_cid(32B) → ∅
    // Temporal: "bonds created in time range"
```

### BondMeta (Compact Binary Encoding)

Value for TABLE_EDGES_OUT: `weight(2B) + creator(1B) + state(1B) + decay(1B) + timestamp(4B)` = **9 bytes**, plus optional evidence CIDs.

### Key Design Decisions

1. **Composite keys with prefix scanning** — redb supports range queries on B+tree
2. **Dual edge tables (OUT + IN)** — Write each bond to both for O(1) lookups in both directions
3. **Bond data stays in Epigenetics** — Vec<Bond> remains source of truth; edge tables are secondary indexes
4. **Type index** enables "find all Causes edges" without scanning all KUs

---

## Index-Free Adjacency vs Index-Based for P2P

For OBKG's P2P architecture, **index-based is better** because:

| Factor | Index-Free | Index-Based |
|--------|-----------|-------------|
| Distributed graph | Pointers can't cross network | Indexes can reference remote CIDs |
| Dynamic topology | Pointer maintenance expensive | Index entries easy to update |
| CRDT sync | Pointer structures hard to merge | Index entries merge cleanly |
| Partial replication | Subset invalidates pointers | DHT-based index for remote edges |
| Update flexibility | Complex pointer rewiring | Simple index entry changes |

**Recommendation**: Hybrid — **local index-based adjacency** (redb edge tables) for fast local traversal + **DHT-based routing** for cross-peer graph traversal.

---

## Query Language Comparison

| Feature | KQL | Cypher | GSQL | AQL | SurrealQL | CozoScript |
|---------|-----|--------|------|-----|-----------|------------|
| Pattern matching | ✅ | ✅ MATCH | ✅ | ✅ | ✅ ->rel-> | ✅ rules |
| Recursive paths | ❌ Not yet | ✅ *1..5 | ✅ ACCUM | ✅ depth | ✅ | ✅ native |
| Graph algorithms | ❌ | ✅ built-in | ✅ MPP | ✅ | ❌ | ✅ PageRank |
| Aggregation | ✅ COUNT/SUM/AVG | ✅ | ✅ | ✅ | ✅ | ✅ |
| Distributed scope | ✅ SCOPE clause | ❌ | ✅ | ❌ | ❌ | ❌ |
| CRDT-aware | ✅ implicit | ❌ | ❌ | ❌ | ❌ | ❌ |
| Time-travel | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ |
| Reactive/watch | ✅ WATCH | ❌ | ❌ | ❌ | ✅ LIVE | ❌ |

**KQL's unique advantages**: SCOPE clause (P2P-aware), CRDT-integrated, WATCH for reactive queries.
**KQL needs from others**: Recursive path traversal (Cypher), built-in graph algorithms (Cozo), time-travel (Cozo/TerminusDB).

---

## Recommended Approach: "Biological Graph Engine" on redb

1. **Add 6 edge index tables** to redb — dual OUT/IN adjacency indexes for O(1) lookups
2. **Adopt Datalog-inspired recursive queries** from CozoDB for KQL's FIND — enable `FIND (k:KU)-[*1..5:CAUSES]->(m:KU)`
3. **Implement TerminusDB's delta-layer concept** for CRDT sync — each sync is a delta of added/removed bonds
4. **Follow SurrealDB's embedded-to-distributed model** — same KQL queries work local and P2P
5. **Study Oxigraph's multi-index encoding** — binary encoding with type-prefixed keys maps to redb B+tree
6. **Keep bonds as Vec<Bond> in Epigenetics** (source of truth) + secondary indexes for traversal

**Why NOT use an existing graph DB directly**:
- Neo4j/TigerGraph/Neptune: Too heavy, no embedded mode
- SurrealDB: Good candidate but OBKG's content-addressed model doesn't fit its document model
- CozoDB: Best candidate for embedding, but OBKG needs P2P-native features no existing DB provides
- **Bottom line**: Build the graph layer in Rust on redb, borrowing the best ideas

---

> **Last updated**: 2026-07-02  
> **Status**: Survey complete — ready for implementation planning
