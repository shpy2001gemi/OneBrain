# Decentralized & P2P Graph Systems — Survey for OBKG

> **Author**: OneBrain Research Team  
> **Date**: 2026-07-02  
> **Purpose**: Survey decentralized graph systems for OBKG's distributed architecture

---

## Executive Summary

This survey analyzes 12 decentralized and P2P graph systems to identify patterns for OBKG's distributed knowledge graph layer. The most relevant systems are: **OrbitDB** (closest architecture — Merkle-CRDTs on IPFS), **Holochain** (closest philosophical match — agent-centric, bio-inspired, DHT), **Merkle-CRDTs** (core sync mechanism), **AT Protocol** (MST for O(1) consistency checks), and **GossipSub** (propagation protocol).

---

## System Analysis

### 1. The Graph (Web3) — ★★★

**Architecture:** Decentralized indexing protocol — the "Google of blockchains." Data organized into **subgraphs** — open APIs that define what blockchain data to extract and how to transform it. Indexers stake GRT to provide indexing services.

**Consensus:** Economic/token-based via GRT token. Deterministic indexing ensures all Indexers produce identical results.

**Key Innovation:** Subgraph-as-unit-of-work pattern — separates data definition from data serving. Economic marketplace for data availability.

**Lesson for OBKG:** The subgraph model maps directly to OBKG's concept of Knowledge Domains or topic-specific graph slices. OBKG could define "Knowledge Subgraphs" that specific nodes choose to index and serve. The Curator role maps to OBKG's pheromone-based quality signaling.

---

### 2. Ceramic Network + ComposeDB — ★★★★

**Architecture:** Decentralized data protocol — like a decentralized Kafka. Data organized into **Streams** — sequences of operations on JSON documents, each with a unique StreamID and cryptographic history. ComposeDB adds a graph database layer with local SQL indexing.

**Consensus:** No global consensus. Each stream controlled by its creator's DID. Eventual consistency via stream synchronization.

**Key Innovation:** Composable data models via shared GraphQL schemas. When different apps use the same ComposeDB model, data becomes automatically interoperable.

**Lesson for OBKG:** The stream model maps to KU update histories. OBKG could use DID-like ownership for KUs. ComposeDB's approach of adding a queryable index layer over content-addressed streams is directly applicable.

---

### 3. IPLD (InterPlanetary Linked Data) — ★★★★★

**Architecture:** The data model layer for the content-addressed web. Everything is a Merkle DAG — nodes identified by CIDs (content hashes), links are CID references. Self-describing CIDs encode hash algorithm + data format.

**Key Innovation:** Universal content-addressed linking. A CID in IPLD can reference data across any system. Merkle property means parent hash changes when any child changes — tamper-evident by construction.

**Lesson for OBKG:** IPLD's linking model is the blueprint for OBKG's KU graph. OBKG already uses BLAKE3 CIDs — adopt IPLD-style linking where KU bonds are CID references. This gives tamper-evident graph structure, efficient sync via root hash comparison, and cross-system interoperability.

---

### 4. Solid (Tim Berners-Lee) — ★★

**Architecture:** Decentralized web platform decoupling data from applications. Users store data in **Pods**. Built on W3C standards: RDF, OWL, Linked Data.

**Key Innovation:** User-controlled data sovereignty with standard Linked Data (RDF/OWL). Data portability — users can switch Pod providers.

**Lesson for OBKG:** Pod model validates OBKG's agent-centric data ownership. The federated SPARQL pattern informs cross-node query design.

---

### 5. AT Protocol (Bluesky) — ★★★★★

**Architecture:** Federated social networking protocol. User data lives in **Personal Data Servers (PDS)** — signed repos organized as **Merkle Search Trees (MSTs)**. MSTs are deterministic, balanced, sorted trees reducing to a single root CID. Big Graph Services/Relays aggregate updates into a network-wide "firehose."

**Consensus:** No global consensus. DID-based identity decouples identity from hosting. Cryptographic signing ensures authenticity.

**Key Innovation:** MST as repo structure — enables O(1) consistency checks via root hash, efficient delta sync by comparing tree branches, and cryptographic proofs of record inclusion. Speech/reach separation allows modular scaling.

**Lesson for OBKG:** **EXTREMELY relevant**. OBKG could organize each node's local Knowledge Graph as an MST, enabling: (1) O(1) consistency checks via root hash, (2) efficient delta sync between nodes, (3) cryptographic proofs that a KU exists in a node's graph. The Relay/firehose pattern maps to OBKG's gossip-based propagation. Lexicon schemas map to OBKG's 33 RelationTypes.

---

### 6. Gun.js — ★★★★

**Architecture:** Decentralized, offline-first graph database. Multi-master mesh networking — every peer is both client and relay. WebRTC for browser-to-browser.

**Consensus:** Uses **HAM algorithm** — a proprietary CRDT-like mechanism. Lexical ordering + logical timestamps determine "latest" state. Strong Eventual Consistency.

**Key Innovation:** Zero-config P2P graph database that works in browsers. HAM algorithm for automatic conflict resolution without coordination.

**Lesson for OBKG:** Validates that a P2P graph database with CRDTs is viable at scale (used by Internet Archive). HAM is similar to LWWRegister. Key lesson: keep conflict resolution simple and deterministic.

---

### 7. OrbitDB — ★★★★★

**Architecture:** Serverless P2P database on IPFS + libp2p. Every entry is an IPFS object with a CID. Uses **Merkle-CRDTs** — operations form a DAG (like Git commits), each entry pointing to predecessors. Five store types: Log, Feed, KeyValue, Document, Counter.

**Consensus:** None — relies on Merkle-CRDTs for eventual consistency. Deterministic merge of concurrent operations.

**Key Innovation:** Merkle-CRDTs — combining content-addressed DAGs with CRDT semantics. Provides both verifiability (content addressing) and conflict-free replication (CRDTs).

**Lesson for OBKG:** **Closest existing system to what OBKG needs**. The Merkle-CRDT approach — using BLAKE3-CID DAGs with CRDT merge semantics — is the natural foundation for OBKG's edge/bond sync. Head-exchange sync protocol directly applicable.

---

### 8. Holochain — ★★★★★

**Architecture:** Agent-centric distributed computing. Each agent has a **source chain** — personal, append-only, cryptographically signed ledger. Public data published to **DHT**. App logic defined in **DNA** — shared validation rules. No global ledger.

**Consensus:** NO global consensus. Distributed data integrity via local validation + peer validation. Warrants issued for invalid data. "Immune system" approach.

**Key Innovation:** Agent-centric model eliminates need for global consensus entirely. Immune system metaphor — peer validators detect and exclude bad actors. Scales linearly.

**Lesson for OBKG:** **Closest philosophical match to OneBrain.** Both are agent-centric, DHT-based, bio-inspired (immune system), no global consensus. Key adoptions: (1) DNA validation rules → per-domain validation for KU bonds, (2) Warrant system → bad-actor detection, (3) Source chain → signed append-only KU history.

---

### 9. ActivityPub/Mastodon — ★★★

**Architecture:** Federated social graph protocol (W3C standard). Every entity is an **Actor** with Inbox (receive) and Outbox (send) endpoints. JSON-LD for semantic linked data.

**Key Innovation:** Simple, standards-based federation via Inbox/Outbox pattern. Actor model provides clean abstraction.

**Lesson for OBKG:** Inbox/Outbox pattern elegant for KU propagation. Each node could have a KU Inbox (receiving bond updates) and Outbox (publishing bond updates).

---

### 10. Yjs / Automerge (CRDT Libraries) — ★★★★

**Key Insights:**
- **Yjs:** YATA algorithm. Extreme performance for collaborative editing. Garbage collection for memory efficiency.
- **Automerge:** RGA + LWW. Full Git-like DAG history. Rust core with WASM bindings.
- Both support graphs via Map/Array primitives.

**Lesson for OBKG:** (1) Automerge's full DAG history perfect for KU version tracking. (2) Yjs's garbage collection needed for resource-constrained nodes. (3) Both validate graph structures CAN be built from CRDT primitives.

---

### 11. GossipSub (libp2p) — ★★★★★

**Architecture:** Hybrid publish/subscribe protocol combining mesh links (eager-push, low latency) and gossip links (lazy-pull, safety net). IHAVE/IWANT protocol for efficient exchange.

**Key Innovation:** Hybrid eager-push/lazy-pull achieves both low latency and high reliability. Peer scoring provides Sybil resistance. Used by Ethereum and Filecoin.

**Lesson for OBKG:** Directly applicable to OBKG's pheromone-based routing: (1) Hot bonds via eager-push mesh, (2) Cold bonds via lazy-pull gossip, (3) Peer scoring maps to node reputation. IHAVE/IWANT perfect for delta-state CRDT sync.

---

### 12. Merkle-CRDTs — ★★★★★

**Two main variants:**
1. **Merkle-DAG CRDTs (Op-based):** Operations embedded in Merkle DAG. Content addressing via CIDs. Key paper: Sanjuán et al.
2. **Merkle Search Tree CRDTs (State-based):** MST encodes CRDT state in balanced Merkle tree. O(1) consistency checks. Key paper: Auvolat & Taïani.

**Key Innovation:** Combining content-addressed verifiability with CRDT convergence guarantees. Enables efficient sync (compare root hashes → exchange diffs) with mathematical consistency guarantees.

**Lesson for OBKG:** **This is OBKG's core pattern.** BLAKE3-CID + CRDTs = Merkle-CRDTs. Use Op-based for causal bond tracking, State-based MST for efficient anti-entropy.

---

## Comparison Matrix

| System | Architecture | Consensus | Conflict Resolution | OBKG Relevance |
|--------|-------------|-----------|-------------------|----------------|
| The Graph | Subgraph marketplace | Economic (GRT) | Deterministic | ★★★ |
| Ceramic/ComposeDB | Stream + graph DB | Single-writer DID | No conflicts | ★★★★ |
| IPLD | Merkle DAG data model | None (data layer) | Immutable (new CIDs) | ★★★★★ |
| Solid | Pod-based linked data | Pod owner | Single owner | ★★ |
| AT Protocol | MST repos + relays | Single-writer | No conflicts | ★★★★★ |
| Gun.js | P2P graph database | HAM algorithm | LWW + lexical | ★★★★ |
| OrbitDB | Merkle-CRDT on IPFS | None (CRDT) | Merkle-CRDT merge | ★★★★★ |
| Holochain | Agent-centric DHT | Peer validation | Validation rules | ★★★★★ |
| ActivityPub | Federated actors | Server authority | Actor ownership | ★★★ |
| Yjs/Automerge | CRDT libraries | CRDT convergence | CRDT algorithms | ★★★★ |
| GossipSub | Pub/sub mesh+gossip | N/A (messaging) | N/A | ★★★★★ |
| Merkle-CRDTs | DAG + CRDT hybrid | CRDT convergence | Mathematical | ★★★★★ |

---

## Key Patterns to Adopt for OBKG

1. **Merkle-CRDT Bonds** (OrbitDB) — Every KU bond is a Merkle-CRDT operation — BLAKE3 CID + CRDT metadata. Sync via root hash comparison + delta exchange.

2. **MST-Based Node State** (AT Protocol) — Each node's local KG organized as a Merkle Search Tree. O(1) consistency checks, efficient delta sync, cryptographic inclusion proofs.

3. **Knowledge Subgraphs** (The Graph) — Domain-specific graph views with schema + mapping rules. Nodes choose which subgraphs to index/serve.

4. **GossipSub for Bond Propagation** — Hybrid mesh+gossip for bond updates. IHAVE/IWANT for delta-state CRDT sync.

5. **Agent-Centric Validation** (Holochain) — Per-domain DNA validation rules. Peer validation by DHT neighbors. Warrant system for bad-actor exclusion.

6. **Composable Schemas** (Ceramic) — Shared graph models for interoperability. OBKG's 33 RelationTypes as shared schemas.

7. **Inbox/Outbox Bond Delivery** (ActivityPub) — Each node has Bond Inbox (receive) and Bond Outbox (publish).

8. **Streaming Partitioning + Label Propagation** — Stream-assign new KUs, periodically refine using pheromone affinity.

---

## Proposed Architecture for OBKG's Distributed Graph Layer

```
┌─────────────────────────────────────────────────────────┐
│                   APPLICATION LAYER                      │
│  Knowledge Subgraph Definitions (The Graph pattern)     │
│  GraphQL/SPARQL Query Interface                         │
├─────────────────────────────────────────────────────────┤
│                   QUERY LAYER                            │
│  Local MST Index (AT Protocol pattern)                  │
│  ComposeDB-style SQL Index over CID-addressed KUs       │
│  Cross-node query routing via DHT                       │
│  Federated query planning                               │
├─────────────────────────────────────────────────────────┤
│                   SYNC LAYER                             │
│  Merkle-CRDT Bond DAG (OrbitDB pattern)                 │
│  Delta-state sync via root hash comparison              │
│  GossipSub mesh+gossip propagation                      │
│  IHAVE/IWANT protocol for efficient exchange            │
│  Inbox/Outbox per node (ActivityPub pattern)            │
├─────────────────────────────────────────────────────────┤
│                   VALIDATION LAYER                       │
│  Agent-centric source chains (Holochain pattern)        │
│  Per-domain DNA validation rules                        │
│  Peer validation by DHT neighbors                       │
│  Warrant system for misbehavior detection               │
│  Immune system response                                 │
├─────────────────────────────────────────────────────────┤
│                   STORAGE LAYER                          │
│  BLAKE3-CID content addressing (IPLD pattern)           │
│  KU bonds as CID-linked Merkle DAG                      │
│  Local graph store per node                             │
│  MST for efficient state representation                 │
│  CRDT state: GCounter, PNCounter, LWWRegister, ORSet   │
├─────────────────────────────────────────────────────────┤
│                   NETWORK LAYER                          │
│  OneBrain 9-layer protocol                              │
│  QUIC transport, Kademlia DHT, SWIM membership          │
│  Stigmergy routing with pheromone-informed GossipSub    │
│  7-tier node hierarchy (Leaf → GlobalBackbone)          │
└─────────────────────────────────────────────────────────┘
```

---

## Special Focus: Graph Partitioning in P2P

| Approach | Quality | Speed | Global Knowledge | OBKG Fit |
|----------|---------|-------|-----------------|----------|
| **METIS** (offline multilevel) | Highest | Slow | Required | ❌ |
| **Label Propagation** (iterative) | Good | Fast | Local only | ✅ Best fit |
| **Streaming** (FENNEL, AKIN) | Moderate | Real-time | None | ✅ For ingestion |

**Recommendation**: Streaming partitioning for new KU placement + periodic label propagation refinement. Leverage pheromone trails as partition affinity signals.

## Special Focus: Gossip-Based Graph Sync

- **Rumor Mongering**: Rapid initial propagation. Exponential spread.
- **Anti-Entropy**: Background reconciliation comparing hash trees. Ensures convergence.
- **Conflict-free via CRDTs**: Version vectors eliminate manual conflict resolution.
- **Logarithmic scaling**: Communication overhead O(log N).

---

> **Last updated**: 2026-07-02  
> **Status**: Survey complete — ready for implementation planning
