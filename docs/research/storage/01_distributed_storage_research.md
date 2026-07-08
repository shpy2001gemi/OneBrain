# Distributed KU Storage — Replication Strategy Research

> **Project**: OneBrain — Decentralized Knowledge Sharing Network  
> **Date**: 2026-07-06  
> **Status**: Research / Pre-Implementation  
> **Scope**: Replication factor, consistency model, DHT persistence, proactive repair, placement strategy, anti-hoarding

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Current State Analysis](#2-current-state-analysis)
3. [Q1: Replication Factor](#3-q1-replication-factor-k)
4. [Q2: Consistency Model](#4-q2-consistency-model)
5. [Q3: DHT Persistence](#5-q3-dht-persistence)
6. [Q4: Proactive Replication](#6-q4-proactive-replication)
7. [Q5: Replica Placement Strategy](#7-q5-replica-placement-strategy)
8. [Q6: Anti-Hoarding / Anti-Freeloading](#8-q6-anti-hoarding--anti-freeloading)
9. [Comparative Analysis](#9-comparative-analysis)
10. [Recommended Architecture](#10-recommended-architecture)
11. [Implementation Roadmap](#11-implementation-roadmap)
12. [References](#12-references)

---

## 1. Executive Summary

OneBrain stores Knowledge Units (KUs) — small binary objects (16–172 bytes) identified by BLAKE3 CIDs — on a modified S/Kademlia DHT. While the DHT routing layer (K=20, 256 buckets) and ReplicaTracker are implemented, **no actual network STORE replication** to K closest nodes exists yet. This document researches and recommends a complete replication strategy that leverages OneBrain's unique bio-inspired architecture (stigmergy, CRDTs, metabolism, 7-tier fitness model) to provide durable, self-healing, incentive-aligned storage.

**Key recommendation**: A **dual-K architecture** separating routing (K=20) from storage replication (R=7), using **CRDT-based eventual consistency** for mutable Layer 2 data, **hybrid persistence** (redb for DHT entries, lazy recovery for routing table), **stigmergy-driven proactive repair**, and **tier-aware replica placement**.

---

## 2. Current State Analysis

### 2.1 What Exists

| Component | Location | Status |
|-----------|----------|--------|
| S/Kademlia DHT | `ku-net/src/dht.rs` | ✅ Routing table, k-buckets, FIND_NODE/FIND_VALUE, local store |
| ReplicaTracker | `ku-net/src/dht.rs:498` | ✅ Tracks `actual_replicas`, `epochs_stored` per CID |
| CRDT Suite | `ku-core/src/crdt.rs` | ✅ GCounter, PNCounter, LWWRegister, ORSet, VectorClock |
| MetabolismStore | `ku-core/src/metabolism_store.rs` | ✅ CRDT merge, delta sync, GC |
| Stigmergy | `ku-net/src/stigmergy.rs` | ✅ PheromoneTable with reinforce/evaporate |
| Encoding Stigmergy | `ku-net/src/encoding_stigmergy.rs` | ✅ JobPheromone for load balancing |
| OBT Storage Reward | `ku-core/src/obt_storage_reward.rs` | ✅ 5-factor formula + PoS-KU challenges |
| redb Persistence | `ku-core/src/persistent_concept_dict.rs`, `ku-kql/src/storage.rs` | ✅ ConceptDict + KU storage |
| 7-Tier Fitness | `ku-net/src/constants.rs:46-72` | ✅ T0 Leaf → T6 Global Backbone |

### 2.2 What's Missing

- **STORE RPC**: No network-level STORE to K closest nodes
- **DHT persistence**: Routing table and DHT entries lost on restart
- **Replication monitoring**: No periodic check for under-replicated CIDs
- **Placement policy**: No tier-aware or region-aware placement logic
- **Serve/store tracking**: No metrics for anti-freeloading enforcement

---

## 3. Q1: Replication Factor (K)

### 3.1 Industry Comparison

| System | Routing K | Storage Replication | Object Size | Notes |
|--------|-----------|-------------------|-------------|-------|
| **Kademlia (original)** | 20 | 20 | Any | K serves double duty |
| **IPFS (libp2p)** | 20 | 20 providers | Blocks (256KB) | Provider records, not data copies |
| **Filecoin** | — | Min 3, typically 5-10 | Sectors (32GB) | Proof-of-Replication required |
| **BitTorrent Mainline** | 8 | 8 | Token (announce) | Only stores metadata, not content |
| **Cassandra** | — | 3 (default RF) | Rows (variable) | Tunable per keyspace |
| **Ethereum Swarm** | — | Neighborhood size (~4-16) | Chunks (4KB) | Proximity-based neighborhoods |
| **Ceramic Network** | — | ~3-5 | Streams (variable) | Event log replication |

### 3.2 Analysis for OneBrain

**Arguments for high R (7-10):**
- KU size is tiny (16-172 bytes) → storage cost of 7 replicas ≈ 1.2KB max, negligible
- Network bandwidth for replicating 7 copies of 172 bytes ≈ 1.2KB per STORE, trivial
- Higher R means faster retrieval (more nodes can serve) and better availability
- OneBrain is a knowledge network — data loss is unacceptable (no "cold tier")

**Arguments against very high R (>10):**
- Metadata overhead per replica: ReplicaTracker entries, PoS-KU challenges scale linearly
- Network chatter for consistency (Layer 2 epigenetics updates) scales with R
- Storage reward rarity_w decreases with more replicas (designed: fewer replicas → higher reward)

**Arguments against low R (<5):**
- Mobile nodes (T0 Leaf, T1 Contributor) go offline frequently → high churn
- Small network sizes in early stages → need more redundancy relative to total nodes
- PoMV validation requires data availability for verification queries

### 3.3 Recommendation: R = 7 (Storage Replication Factor)

**Separate routing K (=20) from storage replication factor R (=7).**

```rust
// Proposed new constant in ku-net/src/constants.rs
/// Storage replication factor (number of nodes to STORE each KU on).
/// Distinct from K_BUCKET_SIZE which governs routing table size.
pub const STORAGE_REPLICATION_FACTOR: usize = 7;

/// Minimum replicas before triggering repair.
pub const MIN_HEALTHY_REPLICAS: usize = 4;

/// Target replicas after repair (may overshoot briefly).
pub const REPAIR_TARGET_REPLICAS: usize = 7;
```

**Rationale**: R=7 provides:
- Survival of up to 3 simultaneous node failures (majority quorum = 4)
- Fits within the 7-tier model (at least 1 replica per relevant tier is feasible)
- Storage cost: 7 × 172 bytes = 1,204 bytes per KU across the network — negligible
- Compatible with existing `rarity_w` formula in `obt_storage_reward.rs` which has `TARGET_REPLICAS = 7` implied by the rarity curve

---

## 4. Q2: Consistency Model

### 4.1 The Two-Layer Challenge

OneBrain KUs have a unique dual-layer structure:

| Layer | Content | Mutability | Consistency Need |
|-------|---------|------------|-----------------|
| **Layer 1 (Core DNA)** | Encoded knowledge, BLAKE3 CID, wire bytes | **Immutable** | None — any copy is authoritative |
| **Layer 2 (Epigenetics)** | Trust scores, metabolism, bonds, epistemic status | **Mutable** | Eventual consistency required |

### 4.2 Options Evaluated

#### Option A: Quorum Reads/Writes (R+W>N)
- **Pros**: Strong consistency, well-understood (Dynamo/Cassandra model)
- **Cons**: Overkill for tiny metadata. R=4, W=4 on N=7 means 4 network round-trips for every trust score update. Latency-sensitive in mobile/heterogeneous network. Violates bio-inspired philosophy (centralized coordination).
- **Verdict**: ❌ Rejected

#### Option B: Primary-Copy with Gossip
- **Pros**: Simple write path — one "owner" node per CID. Consistent reads from owner.
- **Cons**: Single point of failure. Who is "owner"? Closest node by XOR? What if it goes offline? Re-election adds complexity. Doesn't match OneBrain's decentralized philosophy.
- **Verdict**: ❌ Rejected

#### Option C: Last-Writer-Wins with VectorClock
- **Pros**: Simple. VectorClock already implemented in `crdt.rs`. Fast.
- **Cons**: Loses concurrent updates. If Node A and Node B both update trust simultaneously, one is silently dropped. For trust scores this is unacceptable — both corroborations should count.
- **Verdict**: ⚠️ Partially suitable (for `epistemic_status` LWWRegister only)

#### Option D: Eventual Consistency with CRDT Merge (Recommended)
- **Pros**: 
  - **Already implemented**: GCounter (corroboration), PNCounter (trust), LWWRegister (epistemic status), ORSet (domains), VectorClock (causal ordering)
  - **Strong Eventual Consistency (SEC)**: All replicas converge deterministically regardless of update order
  - **No coordination needed**: Merge is commutative, associative, idempotent — perfect for decentralized network
  - **Bio-inspired fit**: Like biological systems, state converges through repeated local interactions
  - **MetabolismStore already uses this**: `merge_remote()` + delta sync = proven pattern
- **Cons**: 
  - Metadata growth (VectorClock grows with number of contributing nodes)
  - Temporary divergence is visible (but harmless for trust/metabolism use cases)
- **Verdict**: ✅ **Recommended**

### 4.3 Recommendation: CRDT-First Eventual Consistency

```
┌─────────────────────────────────────────────────────┐
│              KU Consistency Model                    │
├─────────────────┬───────────────────────────────────┤
│ Layer 1 (DNA)   │ Immutable: verify BLAKE3(bytes)   │
│                 │ == CID. Any copy is valid.         │
├─────────────────┼───────────────────────────────────┤
│ Layer 2 fields: │                                   │
│  trust_score    │ PNCounter → merge via per-node max│
│  corroboration  │ GCounter → merge via per-node max │
│  challenge_ct   │ GCounter → merge via per-node max │
│  metabolism     │ KUMetabolism → merge via GCounter  │
│  epistemic_st   │ LWWRegister → last timestamp wins │
│  domain_codes   │ ORSet → union with tombstones     │
│  bonds          │ ORSet → union with tombstones     │
├─────────────────┼───────────────────────────────────┤
│ Sync mechanism  │ Delta-state CRDT via gossip       │
│                 │ (existing 0x60-0x63 messages)      │
│                 │ + MetabolismGossip (0x85-0x86)     │
├─────────────────┼───────────────────────────────────┤
│ Conflict detect │ VectorClock.is_concurrent()       │
│ Convergence     │ Guaranteed by CRDT math properties │
└─────────────────┴───────────────────────────────────┘
```

This approach mirrors **Ceramic Network's** event streaming model where individual data streams (≈ KU epigenetics) converge independently without global consensus, while maintaining verifiable provenance through cryptographic signatures.

---

## 5. Q3: DHT Persistence

### 5.1 Industry Approaches

| System | Routing Table | Data Storage | Recovery Strategy |
|--------|--------------|-------------|-------------------|
| **libp2p** | Not persisted | Not persisted | Bootstrap from seed nodes |
| **BitTorrent** | Persisted (`dht.dat`) | N/A (data is in files) | Load from file on startup |
| **Ethereum Swarm** | Persisted | Persisted (chunks on disk) | Pull-sync from neighbors |
| **OneBrain (current)** | Not persisted | Not persisted | Lost on restart |

### 5.2 Analysis

**Routing Table Persistence:**
- **Pros**: Faster startup, no bootstrap delay, preserves learned network topology
- **Cons**: Stale entries after long downtime, complexity of serializing `Instant` timestamps, libp2p deliberately avoids this
- **Risk**: Loading stale k-buckets can poison routing with dead nodes

**DHT Data Persistence:**
- **Pros**: Node restarts don't lose stored KUs, critical for storage rewards (epochs_stored counter)
- **Cons**: Additional I/O overhead, redb file size growth
- **Necessity**: **High** — without this, a node restart resets `epochs_stored` and loses storage reward history

**Lazy Recovery via CRDT Sync:**
- **Pros**: Already works for MetabolismStore. No persistence code needed. Self-healing.
- **Cons**: Recovery time depends on gossip cycle. May miss data if all replicas are down.
- **Sufficiency**: Works for Layer 2 epigenetics. Does NOT work for Layer 1 DNA bytes.

### 5.3 Recommendation: Hybrid Persistence

```
┌────────────────────────┬──────────────────────────────────┐
│ Component              │ Strategy                          │
├────────────────────────┼──────────────────────────────────┤
│ Routing Table          │ LAZY RECOVERY                    │
│                        │ - Persist only seed node list     │
│                        │ - Bootstrap from seeds on restart │
│                        │ - Rebuild via FIND_NODE probes    │
│                        │ - Full table recovered in ~30s    │
├────────────────────────┼──────────────────────────────────┤
│ DHT Entries (KU data)  │ PERSIST TO REDB                  │
│                        │ - New table: DHT_ENTRIES          │
│                        │   Key: [u8; 32] (CID)            │
│                        │   Value: wire_bytes + TTL + meta  │
│                        │ - Flush on STORE, batch on epoch  │
│                        │ - Load on startup                 │
├────────────────────────┼──────────────────────────────────┤
│ ReplicaTracker state   │ PERSIST TO REDB                  │
│                        │ - epochs_stored must survive      │
│                        │   restarts for reward continuity  │
│                        │ - New table: REPLICA_META         │
├────────────────────────┼──────────────────────────────────┤
│ Layer 2 Epigenetics    │ CRDT LAZY RECOVERY               │
│                        │ - Recover via MetabolismGossip    │
│                        │ - Recover via CrdtSync (0x60-63)  │
│                        │ - Converges within 2-3 gossip     │
│                        │   rounds (~30-60s)                │
├────────────────────────┼──────────────────────────────────┤
│ Pheromone Table        │ DO NOT PERSIST                   │
│                        │ - Ephemeral by design             │
│                        │ - Rebuilds naturally via queries   │
│                        │ - Stale pheromones are harmful     │
└────────────────────────┴──────────────────────────────────┘
```

**Implementation sketch** (extends existing redb pattern from `persistent_concept_dict.rs`):

```rust
// New redb tables for DHT persistence
const DHT_ENTRIES: TableDefinition<&[u8; 32], &[u8]> = 
    TableDefinition::new("dht_entries");
const REPLICA_META: TableDefinition<&[u8; 32], &[u8]> = 
    TableDefinition::new("replica_meta");
```

---

## 6. Q4: Proactive Replication

### 6.1 Approaches Compared

| Approach | Detection | Trigger | Latency | Cost |
|----------|-----------|---------|---------|------|
| **Passive** | On next lookup | Client request | High (until someone asks) | Zero when idle |
| **Active (periodic)** | Epoch sweep | Timer (every epoch = 1hr) | Medium (max 1 epoch) | Constant bandwidth |
| **Stigmergy-based** | Pheromone decay | Continuous, organic | Low (adaptive) | Proportional to need |

### 6.2 Recommendation: Stigmergy-Driven Repair (Bio-Inspired)

OneBrain already has a rich stigmergy framework (`PheromoneTable`, `JobPheromone`). Extending this to replication repair is a natural fit:

```
┌──────────────────────────────────────────────────┐
│         Stigmergy-Driven Replication Repair       │
├──────────────────────────────────────────────────┤
│                                                  │
│  1. Each stored CID has a "replication pheromone"│
│     strength = f(actual_replicas, metabolism)     │
│                                                  │
│  2. Pheromone EVAPORATES when replicas go offline│
│     - SWIM failure detection triggers evaporate  │
│     - Pheromone drops below threshold            │
│                                                  │
│  3. Nodes "forage" for weak pheromones           │
│     - Periodic scan (every epoch, 1 hour)        │
│     - Find CIDs with pheromone < MIN_THRESHOLD   │
│     - These are "cold" / under-replicated CIDs   │
│                                                  │
│  4. Repair as "ant task"                         │
│     - Node volunteers to re-replicate            │
│     - Fetches KU from existing replica           │
│     - STOREs to new K-closest nodes              │
│     - Reinforces pheromone on success            │
│                                                  │
│  5. High-metabolism CIDs get priority            │
│     - More accessed = stronger pheromone base    │
│     - Natural priority: popular data repaired    │
│       faster than dormant data                   │
│                                                  │
└──────────────────────────────────────────────────┘
```

**Fallback**: Passive repair on FIND_VALUE (when a lookup discovers `actual_replicas < MIN_HEALTHY_REPLICAS`, trigger background re-replication). This catches CIDs that escaped the epoch sweep.

**Integration with existing systems:**
- SWIM membership detects node failures → updates ReplicaTracker counters
- MetabolismStore provides demand signal → prioritizes high-value KUs
- OBT storage reward's `rarity_w` naturally incentivizes nodes to store under-replicated KUs (higher reward)

---

## 7. Q5: Replica Placement Strategy

### 7.1 Options

| Strategy | Pros | Cons | Fit for OneBrain |
|----------|------|------|-----------------|
| **XOR-closest (Kademlia default)** | Simple, O(log n) lookup | No fault domain diversity | Partial — good for routing, not resilience |
| **Random placement** | Good distribution | Slow lookup, no locality | ❌ Breaks Kademlia invariant |
| **Region-aware** | Survives regional outages | Needs geo-IP, complex | ⚠️ Future phase |
| **Tier-aware** | Leverages OneBrain's 7 tiers | Novel, needs design | ✅ Recommended |

### 7.2 Recommendation: Tier-Aware Placement with XOR Base

```
Replica Placement Rule (R=7):
┌─────────────────────────────────────────────────────┐
│ 1. Primary replicas (4 of 7):                       │
│    → K-closest nodes by XOR distance (Kademlia)     │
│    → Standard, fast lookup                          │
│                                                     │
│ 2. Tier-anchored replicas (2 of 7):                 │
│    → At least 1 on a SuperPeer (T2+ node)           │
│    → At least 1 on a different SuperPeer (T3+)      │
│    → Ensures availability even during leaf churn     │
│                                                     │
│ 3. Diversity replica (1 of 7):                      │
│    → Random node NOT in the same /24 subnet         │
│    → Protects against network partition              │
│                                                     │
│ Selection priority within each group:                │
│    1. Fitness score (higher tier preferred)           │
│    2. Uptime history (from SWIM)                     │
│    3. RTT (lower latency preferred)                  │
│    4. Available storage capacity                     │
└─────────────────────────────────────────────────────┘
```

**Tier distribution model:**

| Tier | Role | Min Replicas | Rationale |
|------|------|-------------|-----------|
| T0 Leaf | Mobile/IoT | 0 | Too unreliable for guaranteed storage |
| T1 Contributor | Home PC | 1-2 | Good capacity but variable uptime |
| T2 Local SP | Server | 1 (required) | Reliable, anchor replica |
| T3+ District/Country/Region/Global | Infrastructure | 1 (required) | Backbone reliability guarantee |
| Any | Diversity | 1 | Random selection for partition tolerance |

---

## 8. Q6: Anti-Hoarding / Anti-Freeloading

### 8.1 Existing Mechanisms

OneBrain already has strong anti-gaming foundations:

1. **PoS-KU Challenges** (`obt_storage_reward.rs`): Three challenge types verify actual possession:
   - `FullHash`: BLAKE3 of entire wire bytes
   - `ByteRange`: Random byte range extraction
   - `FieldExtract`: Field extraction + Merkle proof

2. **5-Factor Storage Reward**: The `rarity_w` factor already penalizes hoarding (storing over-replicated KUs yields diminishing returns)

3. **EigenTrust `trust_f`**: Low-trust nodes earn less per stored KU

### 8.2 Additional Mechanisms Needed

```
┌───────────────────────────────────────────────────────────┐
│              Anti-Hoarding / Anti-Freeloading              │
├───────────────────────┬───────────────────────────────────┤
│ Problem               │ Solution                          │
├───────────────────────┼───────────────────────────────────┤
│ Store but never serve │ Track serve_count per CID per     │
│                       │ epoch. Reward = base × serve_ratio│
│                       │ where serve_ratio =               │
│                       │   serves / expected_serves        │
│                       │ Expected = queries_received / R   │
├───────────────────────┼───────────────────────────────────┤
│ Query but never store │ SWAP-like credit system:          │
│                       │ - Each GET costs 1 credit         │
│                       │ - Each STORE earns 1 credit       │
│                       │ - Nodes with credit < -100 are    │
│                       │   rate-limited (not blocked)      │
│                       │ - Credits decay hourly (forgive)  │
├───────────────────────┼───────────────────────────────────┤
│ Sybil storage farms   │ Already handled:                  │
│                       │ - PoW identity (puzzle_c)         │
│                       │ - PoS-KU challenges prove real    │
│                       │   possession, not just CID claim  │
│                       │ - EigenTrust penalizes colluding  │
│                       │   low-quality nodes                │
├───────────────────────┼───────────────────────────────────┤
│ Strategic KU hoarding │ rarity_w naturally handles this:  │
│ (store only rare KUs) │ as replicas increase, reward drops│
│                       │ Organic market equilibrium.        │
└───────────────────────┴───────────────────────────────────┘
```

---

## 9. Comparative Analysis

### 9.1 OneBrain vs. Comparable Systems

| Feature | IPFS | Filecoin | Swarm | Ceramic | **OneBrain (Proposed)** |
|---------|------|----------|-------|---------|------------------------|
| **Object size** | 256KB blocks | 32GB sectors | 4KB chunks | Variable streams | **16-172 bytes** |
| **Replication** | Provider records | Proof-of-Rep | Neighborhood sync | Stream replication | **R=7, tier-aware** |
| **Consistency** | Eventual | Blockchain | Eventual | Eventual (streams) | **CRDT (SEC)** |
| **Persistence** | IPFS datastore | Sector storage | LevelDB | IPFS backend | **redb + CRDT recovery** |
| **Incentive** | None (Bitswap) | FIL token | BZZ token | None | **OBT 5-factor reward** |
| **Repair** | Re-provide (passive) | Sector recovery | Pull-sync | Re-pin | **Stigmergy-driven** |
| **Bio-inspired** | No | No | No | No | **Yes (pheromone, metabolism)** |
| **Identity** | PeerId | Miner ID | Overlay address | DID | **PoW NodeId + tiers** |

### 9.2 Key Insight: OneBrain's Unique Position

OneBrain's KU objects are **orders of magnitude smaller** than any comparable system. This fundamentally changes the trade-offs:

- **Erasure coding**: Inappropriate. EC metadata (fragment IDs, parity checksums) would exceed the data itself. Pure replication wins for objects < 1KB.
- **Proof-of-Replication** (Filecoin-style): Overkill. Sealing 172 bytes into a proof is computationally wasteful. PoS-KU byte-range challenges are more appropriate.
- **Content-addressed deduplication**: Automatic via BLAKE3 CID. Two identical KUs have the same CID — no duplicates stored.
- **Bandwidth**: Replicating 7 copies of 172 bytes = 1.2KB total. Even on mobile networks, this is negligible.

---

## 10. Recommended Architecture

### 10.1 Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                    OneBrain Storage Architecture                 │
│                                                                 │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────────────┐  │
│  │  Layer 1     │    │  Layer 2     │    │   Metadata          │  │
│  │  (DNA bytes) │    │  (Epigenetics)│    │   (Replica State)   │  │
│  │              │    │              │    │                     │  │
│  │  Immutable   │    │  CRDTs       │    │  ReplicaTracker     │  │
│  │  BLAKE3 CID  │    │  GCounter    │    │  PheromoneTable     │  │
│  │              │    │  PNCounter   │    │  MetabolismStore    │  │
│  │              │    │  LWWRegister │    │                     │  │
│  │              │    │  ORSet       │    │                     │  │
│  └──────┬───────┘    └──────┬───────┘    └──────────┬──────────┘  │
│         │                   │                       │             │
│         ▼                   ▼                       ▼             │
│  ┌──────────────────────────────────────────────────────────────┐ │
│  │                    Replication Manager                        │ │
│  │                                                              │ │
│  │  ┌────────────┐  ┌────────────┐  ┌─────────────────────┐    │ │
│  │  │ STORE RPC  │  │ CRDT Sync  │  │ Stigmergy Repair    │    │ │
│  │  │ (R=7 nodes)│  │ (delta-    │  │ (pheromone-driven   │    │ │
│  │  │            │  │  state)    │  │  re-replication)    │    │ │
│  │  └────────────┘  └────────────┘  └─────────────────────┘    │ │
│  └──────────────────────────────────────────────────────────────┘ │
│         │                   │                       │             │
│         ▼                   ▼                       ▼             │
│  ┌──────────────────────────────────────────────────────────────┐ │
│  │                    Persistence Layer                          │ │
│  │                                                              │ │
│  │  ┌──────────────┐  ┌──────────────┐  ┌───────────────────┐  │ │
│  │  │ redb:        │  │ In-Memory:   │  │ Lazy Recovery:    │  │ │
│  │  │ DHT_ENTRIES  │  │ RoutingTable │  │ CRDT gossip sync  │  │ │
│  │  │ REPLICA_META │  │ PheromoneTab │  │ Bootstrap nodes   │  │ │
│  │  └──────────────┘  └──────────────┘  └───────────────────┘  │ │
│  └──────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

### 10.2 STORE RPC Flow

```
Creator Node                    R=7 Target Nodes
     │                               │
     │ 1. FIND_NODE(CID)             │
     │──────────────────────────────►│
     │                               │
     │ 2. Select 7 targets:          │
     │    4× XOR-closest             │
     │    2× Tier-anchored (T2+,T3+) │
     │    1× Diversity (diff subnet) │
     │                               │
     │ 3. STORE(CID, wire_bytes)     │
     │──────────────────────────────►│ Node stores locally
     │                               │ Updates ReplicaTracker
     │ 4. STORE_ACK                  │ Persists to redb
     │◄──────────────────────────────│
     │                               │
     │ 5. If ACKs < 4:              │
     │    Retry with next-closest    │
     │    Log under-replication      │
     │                               │
```

### 10.3 Summary of Decisions

| Question | Decision | Rationale |
|----------|----------|-----------|
| Q1: Replication Factor | **R=7** (separate from routing K=20) | Tiny objects, cheap replication, 7-tier alignment |
| Q2: Consistency | **CRDT eventual (SEC)** | Already implemented, bio-inspired, no coordination |
| Q3: DHT Persistence | **Hybrid**: redb for data, lazy for routing | Critical data persisted, ephemeral state rebuilt |
| Q4: Proactive Repair | **Stigmergy-driven** + passive fallback | Natural extension of existing pheromone system |
| Q5: Placement | **Tier-aware**: 4 XOR + 2 tier-anchored + 1 diversity | Leverages 7-tier model for resilience |
| Q6: Anti-hoarding | **Serve ratio tracking** + credit system | Complements existing PoS-KU and rarity_w |

---

## 11. Implementation Roadmap

### Phase 1: Foundation (Sprint 1-2)
- [ ] Add `STORAGE_REPLICATION_FACTOR` constant (R=7)
- [ ] Implement STORE RPC message type (0x24) and handler
- [ ] Basic STORE to R closest nodes on KU creation
- [ ] Persist DHT entries to redb (extend existing `persist` feature)
- [ ] Persist ReplicaTracker to redb

### Phase 2: Consistency (Sprint 3-4)
- [ ] Wire CRDT delta sync for Layer 2 epigenetics across replicas
- [ ] Implement STORE_ACK with receipt
- [ ] Add replica count tracking via FIND_VALUE responses
- [ ] Integration test: 3-node cluster with CRDT convergence

### Phase 3: Repair & Placement (Sprint 5-6)
- [ ] Implement replication pheromone (extends PheromoneTable)
- [ ] Epoch-based repair sweep (find under-replicated CIDs)
- [ ] Tier-aware placement logic in STORE target selection
- [ ] SWIM failure → ReplicaTracker update pipeline

### Phase 4: Anti-Gaming (Sprint 7-8)
- [ ] Serve count tracking per CID per epoch
- [ ] Credit system for GET/STORE balance
- [ ] Rate limiting for negative-credit nodes
- [ ] Integration with OBT storage reward (serve_ratio factor)

---

## 12. References

### Papers
1. **Maymounkov, P. & Mazières, D. (2002)**. "Kademlia: A Peer-to-peer Information System Based on the XOR Metric." IPTPS. — Foundation of DHT design.
2. **Stutzbach, D. & Rejaie, R. (2006)**. "Improving Lookup Performance Over a Widely-Deployed DHT." INFOCOM. — Empirical analysis of K parameter in Kad network.
3. **Hassanzadeh-Nazarabadi, Y. et al. (2019)**. "Decentralized Utility- and Locality-Aware Replication for Heterogeneous DHT-Based P2P Cloud Storage Systems." — Tier-aware replication research.
4. **Cortes-Goicoechea, M. et al. (2024)**. "Scalability limitations of Kademlia DHTs when enabling Data Availability Sampling in Ethereum." — Modern DHT scalability analysis.
5. **Shapiro, M. et al. (2011)**. "Conflict-free Replicated Data Types." SSS. — CRDT formal foundations.

### Projects
6. **IPFS / libp2p**: Content-addressed P2P storage. Provider records model. No native DHT persistence.
7. **Filecoin**: Proof-of-Replication, min 3 replicas, sector-based storage.
8. **Ethereum Swarm (DISC)**: Neighborhood-based chunk replication, push-sync/pull-sync, postage stamps.
9. **Ceramic Network**: Event streaming, eventual consistency, DID-based ownership, IPFS backend.

### Bio-Inspired
10. **Ant Colony Optimization for Data Replication** (Sogang University): Stigmergy-based adaptive replication using digital pheromones.
11. **Stigmergic Coordination in Distributed Systems** (ULB Brussels): Formal model of pheromone-based coordination without central control.

---

> [!NOTE]
> This document focuses on the **design** of the replication strategy. Implementation details for each phase will be tracked in separate feature specs under `docs/specs/storage/`.
