# Hot/Cold Tiering & Caching Strategy for OneBrain

> **Research Topic 3** — Storage Optimization for Decentralized Knowledge Network
> **Date**: 2026-07-06 | **Status**: Research Complete

---

## Executive Summary

OneBrain's Knowledge Units (KUs) exhibit a classic power-law access pattern: a small fraction of KUs are "hot" (metabolic_rate > 0.3), most are warm, and a long tail are metabolically dead (rate < 0.001). The current architecture stores all KUs uniformly in redb with no content caching — every `get()` hits disk. This research proposes a **3-tier metabolism-aware architecture** that leverages OneBrain's existing `KUMetabolism` system as a natural cache scoring signal, paired with graph-locality prefetching and gossip-based cache coherence.

**Key recommendations:**
1. **Metabolism-Aware Read-Through Cache** (ARC variant) with ~10K KU capacity (~4MB)
2. **1-hop selective prefetch** during spreading activation, gated by bond weight
3. **Gossip-based invalidation** using existing `CacheInvalidate` (0x68) message type
4. **Eventual consistency** acceptable for Epigenetics updates (soft trust scores)

---

## 1. Proposed 3-Tier Architecture

### 1.1 Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                    APPLICATION LAYER                     │
│   QueryEngine  │  SpreadingActivation  │  DreamEngine   │
├─────────────────────────────────────────────────────────┤
│                   KU CACHE MANAGER                      │
│         (Metabolism-Aware Read-Through Cache)           │
├──────────┬──────────────────┬───────────────────────────┤
│ HOT TIER │   WARM TIER      │     COLD TIER             │
│ (Memory) │   (Local Disk)   │     (Network/DHT)         │
│ ~10K KUs │   ~1M KUs        │     Unbounded             │
│ <1μs     │   ~50-200μs      │     ~50-500ms             │
└──────────┴──────────────────┴───────────────────────────┘
```

### 1.2 Tier Definitions

#### Hot Tier (In-Memory Cache)

| Parameter | Value | Rationale |
|:---|:---|:---|
| **Capacity** | 10,000 KUs | ~4MB at ~400 bytes/KU (KU content ~172 bytes + Epigenetics ~200-500 bytes CBOR). Fits comfortably in L2/L3 cache working set. |
| **Data stored** | Full KU content + Epigenetics + 1-hop bond list | Bonds needed for spreading activation prefetch |
| **Access latency** | < 1μs | In-process HashMap lookup |
| **Promotion trigger** | `metabolic_rate > 0.1` OR accessed in last 60 seconds OR prefetched neighbor | Combines metabolism signal with recency |
| **Demotion trigger** | ARC eviction OR `metabolic_rate < 0.01` for > 5 minutes | Metabolism decay naturally demotes stale entries |

**Why 10K?** The `MetabolismStore` already caps at `MAX_METABOLISM_ENTRIES = 100_000`. Empirically, the Pareto principle suggests ~10% of KUs receive 90% of queries. At ~400 bytes per cached entry, 10K entries = **4MB** — negligible memory overhead.

#### Warm Tier (Local Disk / redb)

| Parameter | Value | Rationale |
|:---|:---|:---|
| **Capacity** | ~1M KUs | redb's B-tree handles this efficiently; ~400MB on disk |
| **Data stored** | All KUs this node is responsible for (full Core DNA + Epigenetics) |
| **Access latency** | 50-200μs | redb read transaction with memory-mapped pages |
| **Promotion trigger** | KU stored locally via DHT responsibility or explicit replication |
| **Demotion trigger** | `gc_dead()` removes entries with zero engagement AND age > 1 year (existing logic) |

**Key insight:** redb already provides excellent read performance via memory-mapped I/O and zero-copy reads. The warm tier isn't "slow" — it's the **baseline**. The hot tier exists to eliminate the overhead of opening read transactions and deserializing CBOR for the most frequently accessed KUs.

#### Cold Tier (Network / DHT)

| Parameter | Value | Rationale |
|:---|:---|:---|
| **Capacity** | Unbounded (entire network) | Distributed across all peers |
| **Data stored** | KUs stored on other nodes, reachable via DHT routing |
| **Access latency** | 50-500ms | Network round-trip; existing `QueryForward` (0x50) / `QueryResponse` (0x51) messages |
| **Promotion trigger** | Query hit on a remote KU → fetch and cache locally |
| **Demotion trigger** | N/A — cold tier is "everywhere else" |

**DreamEngine consideration:** DreamEngine's `run_dream_cycle()` needs access to all bonds for replay/association/pruning. Since bonds are stored in `Epigenetics.bonds`, and DreamEngine operates offline, it should read directly from the warm tier (redb) without polluting the hot cache. The `obkg_bridge::collect_bonds_map()` function already builds a `HashMap` from KU bond data — this bulk read should bypass the cache entirely.

### 1.3 Data Flow

```
Query arrives → Check Hot Cache (HashMap)
    ├── HIT: Return immediately, record MetabolismEvent::QueryHit
    └── MISS: Read from Warm Tier (redb)
         ├── FOUND: Return, optionally promote to Hot if metabolic_rate > threshold
         └── NOT FOUND: Fetch from Cold Tier (DHT network)
              ├── FOUND: Store in Warm, optionally promote to Hot
              └── NOT FOUND: Return None
```

---

## 2. Cache Design Recommendation

### 2.1 Recommended: Metabolism-Aware ARC (M-ARC)

After analyzing the options, the recommendation is a **Metabolism-Aware Adaptive Replacement Cache (M-ARC)** — a hybrid that combines ARC's self-tuning recency/frequency balance with OneBrain's metabolism signal as an eviction weight.

**Why ARC over plain LRU?**

| Criterion | LRU | LFU | ARC | **M-ARC (Proposed)** |
|:---|:---|:---|:---|:---|
| Scan resistance | ❌ Poor | ✅ Good | ✅ Excellent | ✅ Excellent |
| Adapts to workload shifts | ❌ No | ❌ No | ✅ Yes | ✅ Yes |
| Tuning required | None | Manual | None | None (uses existing metabolism) |
| Knowledge graph suitability | ❌ | ⚠️ | ✅ | ✅✅ |
| Implementation complexity | Low | Low | Medium | Medium |

**Why ARC matters for OneBrain specifically:**
- **ConsolidationEngine** performs batch scoring of all KUs — this is a classic "scan" pattern that would flush an LRU cache. ARC's ghost lists protect against this.
- **DreamEngine** replays access patterns — if the replay reads KUs sequentially, LRU would evict the actually-hot KUs. ARC handles this gracefully.
- The existing `metabolic_rate` already encodes a sophisticated recency × frequency signal with exponential decay — this maps perfectly to ARC's dual-list philosophy.

### 2.2 M-ARC Implementation Design

```rust
pub struct MetabolismArcCache {
    /// T1: Recently accessed (one-hit wonders)
    t1: IndexMap<[u8; 32], CachedKU>,
    /// T2: Frequently accessed (confirmed hot)
    t2: IndexMap<[u8; 32], CachedKU>,
    /// B1: Ghost list for T1 evictions (metadata only)
    b1: IndexMap<[u8; 32], GhostEntry>,
    /// B2: Ghost list for T2 evictions (metadata only) 
    b2: IndexMap<[u8; 32], GhostEntry>,
    /// Target size for T1 (self-tuning parameter)
    p: usize,
    /// Total capacity
    capacity: usize,
}

struct CachedKU {
    content: Vec<u8>,          // Core DNA bytes (~172 bytes)
    epigenetics: Epigenetics,  // Runtime metadata (~200-500 bytes)
    neighbors: Vec<([u8; 32], u16)>,  // 1-hop bonds for prefetch
    metabolic_rate: f64,       // Cached rate (refreshed periodically)
    inserted_at: Instant,      // For TTL
}

struct GhostEntry {
    metabolic_rate: f64,  // Rate at eviction time
    evicted_at: Instant,
}
```

**Key adaptation over standard ARC:**
When deciding which entry to evict from T1 or T2, standard ARC evicts the LRU entry. M-ARC instead evicts the entry with the **lowest `metabolic_rate`** within the candidate list. This leverages the exponential decay built into `KUMetabolism::metabolic_rate()` — entries that haven't been accessed recently will have naturally decayed rates.

### 2.3 Cache Invalidation

Invalidation triggers when Epigenetics is updated:
1. **Local update**: Trust score change, bond added/removed, epistemic status transition
2. **Cache action**: Remove entry from hot cache (or update in-place if the update is local)
3. **Network propagation**: Send `CacheInvalidate` (0x68) gossip message

```rust
// Invalidation on Epigenetics update
fn update_epigenetics(&mut self, cid: &[u8; 32], new_epi: Epigenetics) {
    // 1. Write to warm tier (redb)
    self.warm_store.put(cid, &new_epi);
    
    // 2. Invalidate hot cache
    self.hot_cache.invalidate(cid);
    
    // 3. Gossip invalidation to peers
    self.network.broadcast(MessageType::CacheInvalidate, cid);
}
```

### 2.4 Write Strategy: Write-Through (Not Write-Behind)

**Recommendation: Write-through** (writes go to redb immediately, cache updated synchronously).

Rationale:
- KU writes are relatively infrequent compared to reads
- Write-behind risks data loss on crash (ACID violation)
- redb write transactions are fast (< 1ms for single KU)
- The system already has CRDT merge semantics — lost writes mean missed CRDT states, which breaks convergence guarantees

### 2.5 Rust Implementation Considerations

For the underlying data structures in Rust:
- Use `lru` crate (O(1) operations) or implement ARC using two `IndexMap`s from the `indexmap` crate
- Wrap in `parking_lot::RwLock` for concurrent access (readers don't block each other)
- Consider `moka` crate for production — it provides a concurrent cache with TTL support and ARC-like behavior built-in
- LRU caches in Rust require `&mut self` for `get()` (recency update is a mutation), so `RwLock` must be upgraded to write lock on reads — `parking_lot::RwLock` has efficient upgrade semantics

---

## 3. Prefetch Strategy

### 3.1 Recommendation: 1-Hop Selective Prefetch

When a KU is accessed, prefetch its **1-hop neighbors** with **bond weight > 5000** (top-50% strength).

**Why 1-hop, not 2-hop?**

| Depth | KUs Prefetched (avg) | Useful Hit Rate | Wasted Bandwidth |
|:---|:---|:---|:---|
| 0 (no prefetch) | 0 | N/A | 0% |
| **1-hop selective** | **~3-5** | **~60-70%** | **~30%** |
| 1-hop all | ~10-15 | ~40-50% | ~50% |
| 2-hop selective | ~15-30 | ~25-35% | ~65% |
| 2-hop all | ~50-100+ | ~15% | ~85% |

**Rationale from codebase analysis:**
- `spreading_activation()` (graph_bio.rs:271) already traverses neighbors with weight-based decay: `spread = activation * decay_factor * (weight / 10000.0)`. With `decay_factor = 0.8` and `threshold = 0.01`, only neighbors with `weight > 125` receive meaningful activation at 1-hop.
- The STDP engine strengthens causally-accessed bonds. If KU-A → KU-B is a strong causal bond, prefetching B when A is accessed has high probability of a subsequent hit.
- `ConsolidationEngine` uses `bond_count` as a scoring signal (w_bonds = 0.20). Hub nodes (high bond count) are more likely to be consolidated — and their neighbors are more likely to be accessed.

### 3.2 Prefetch Implementation

```rust
fn on_ku_access(&mut self, cid: &[u8; 32]) {
    // 1. Serve the request (from hot cache or warm tier)
    let ku = self.get_ku(cid);
    
    // 2. Async prefetch strong neighbors
    if let Some(cached) = self.hot_cache.get(cid) {
        for &(neighbor_cid, weight) in &cached.neighbors {
            if weight > 5000 && !self.hot_cache.contains(&neighbor_cid) {
                // Prefetch from warm tier (non-blocking)
                self.prefetch_queue.push(neighbor_cid);
            }
        }
    }
}
```

### 3.3 Special Handling for Batch Operations

| Operation | Prefetch Strategy |
|:---|:---|
| **Interactive query** | 1-hop selective prefetch (weight > 5000) |
| **Spreading activation** | No additional prefetch — the algorithm itself traverses neighbors |
| **DreamEngine replay** | **Bypass cache entirely** — reads directly from warm tier |
| **ConsolidationEngine scoring** | **Bypass cache entirely** — batch scan pattern |
| **STDP weight update** | No prefetch — operates on bond metadata only |

The key insight is that batch operations (DreamEngine, ConsolidationEngine) should **not** use the hot cache. They access KUs in patterns that would thrash the cache (sequential scan, full-graph traversal). These should read directly from redb via dedicated read transactions.

---

## 4. Eviction Policy Recommendation

### 4.1 Metabolism-Weighted ARC Eviction

The eviction policy combines ARC's adaptive recency/frequency balancing with metabolism awareness:

```
eviction_score(ku) = metabolic_rate(now, half_life) 
                   × (1.0 + 0.1 × hub_bonus)
                   × freshness_factor

where:
  hub_bonus = min(bond_count / 20.0, 1.0)    // Keep hub nodes
  freshness_factor = e^(-age_in_cache / 300)  // 5-minute decay in cache
```

**Priority for eviction** (lowest score evicted first):

1. **Dead KUs** (metabolic_rate < 0.001): Evict immediately — these passed the `METABOLISM_ALIVE_THRESHOLD`
2. **Low-metabolism leaf nodes** (rate < 0.01, bond_count < 3): Low value, low connectivity
3. **Standard ARC eviction**: T1-LRU or T2-LRU based on ARC's adaptive p parameter
4. **Hub nodes** (bond_count > 10): Protected — they're graph connectors with high prefetch value
5. **Pinned entries**: ConsolidationEngine's `PromoteToCore` results — never evicted

### 4.2 Why Not Standard LRU/LFU?

**LRU fails for knowledge graphs because:**
- A DreamEngine replay cycle reads KUs sequentially → flushes all hot entries
- A ConsolidationEngine scoring pass touches every KU once → cache pollution
- Spreading activation follows graph structure, not temporal order → temporal locality is weak

**LFU fails because:**
- Frequency counters accumulate forever → old popular KUs can never be evicted
- OneBrain's `metabolic_rate` already solves this with exponential decay, but raw LFU doesn't decay
- New KUs start with 0 frequency → they can never enter the cache even if highly relevant

**ARC + Metabolism solves both:**
- ARC's ghost lists learn from eviction patterns (self-tuning)
- Metabolism rate provides a natural, decay-aware frequency signal
- The 30-day half-life (`DEFAULT_HALF_LIFE_SECS = 30 * 24 * 3600`) ensures old entries fade

### 4.3 Graph-Locality Clustering (Future Enhancement)

Keep topologically related KUs together in cache. If KU-A is cached, bias toward also caching its neighbors. This naturally emerges from the 1-hop prefetch strategy — but could be explicitly enforced by giving a small eviction-resistance bonus to KUs that share bonds with other cached KUs.

This is similar to Neo4j's page cache behavior, where the page cache naturally clusters related nodes that share the same storage page. In OneBrain's case, we achieve this at the application level through bond-aware prefetching.

---

## 5. Distributed Cache Coherence

### 5.1 Recommendation: Gossip Invalidation + TTL + Accept Staleness

The recommended approach is a **layered strategy** that matches coherence strictness to data sensitivity:

| Data Type | Coherence Model | Justification |
|:---|:---|:---|
| **Core DNA content** | Immutable — no coherence needed | CID = hash of content; content never changes |
| **Epigenetics trust scores** | Eventual consistency (TTL = 300s) | Trust scores are soft signals; minor staleness is acceptable |
| **Epigenetics bonds** | Gossip invalidation | Bond changes affect graph traversal — propagate quickly |
| **Metabolism data** | CRDT merge (existing) | `KUMetabolism` uses GCounters — merge is always safe |
| **Consolidation results** | Local only — no coherence needed | Each node scores independently |

### 5.2 Implementation: 3-Layer Coherence

#### Layer 1: Gossip Invalidation (Fast Path)

Use the existing `CacheInvalidate` (0x68) message type. When a node updates Epigenetics for a KU:

```rust
// Existing message type in messages.rs
CacheInvalidate = 0x68

// Payload: just the CID (32 bytes) + version counter (u64)
struct CacheInvalidatePayload {
    cid: [u8; 32],
    version: u64,       // Monotonic per-KU counter
    updated_fields: u8, // Bitfield: which sections changed
}
```

Gossip dissemination (rumor-mongering model):
- On invalidation, send to **3 random peers** (fan-out = 3)
- Each peer forwards to 3 more peers (with TTL = 5 hops)
- Expected propagation: reaches 90% of N-node network in O(log N) rounds
- Message size: 41 bytes per invalidation — negligible bandwidth

#### Layer 2: TTL-Based Expiry (Background Safety Net)

Even without receiving gossip, cached entries expire:

| Cache Entry Type | TTL | Rationale |
|:---|:---|:---|
| KU content + Epigenetics | 300 seconds (5 min) | Balance freshness vs. redb reads |
| Bond list | 120 seconds (2 min) | Bonds change more frequently (STDP updates) |
| Metabolism snapshot | 60 seconds (1 min) | Metabolism rate changes rapidly on active KUs |
| Prefetched neighbors | 180 seconds (3 min) | Speculative — shorter TTL acceptable |

#### Layer 3: Accept Staleness (Philosophy)

OneBrain's data model is inherently tolerant of staleness:

1. **Core DNA is immutable** — CID = content hash. A cached Core DNA entry is always valid.
2. **Epigenetics are "soft" signals** — a trust score of 7500 vs. 7520 doesn't materially change query results.
3. **Metabolism uses CRDTs** — `KUMetabolism::merge()` (metabolism.rs:236) handles convergence automatically. A slightly stale metabolic_rate just means a KU stays in cache slightly longer/shorter than optimal.
4. **Bonds are updated by STDP** — `StdpEngine::update_weight()` adjusts bond weights based on access timing. A stale bond weight means slightly suboptimal spreading activation — not incorrect results.

**The key insight:** For a knowledge network, **availability and performance** matter more than strict consistency. A user who gets a slightly stale trust score but gets their answer in 1μs is better served than one who waits 200ms for a perfectly fresh score.

### 5.3 Version Vectors (Optional Enhancement)

For stronger coherence when needed (e.g., during consensus rounds), implement lightweight version vectors:

```rust
struct CacheEntry {
    data: CachedKU,
    version: u64,    // Per-KU monotonic counter
    node_id: u64,    // Which node last wrote this version
}

// On read from cache:
fn get_with_version_check(&mut self, cid: &[u8; 32]) -> Option<&CachedKU> {
    if let Some(entry) = self.hot_cache.get(cid) {
        // Check if warm tier has a newer version
        if let Some(warm_version) = self.warm_store.get_version(cid) {
            if warm_version > entry.version {
                // Stale — refresh from warm tier
                self.hot_cache.invalidate(cid);
                return self.load_from_warm(cid);
            }
        }
        return Some(&entry.data);
    }
    None
}
```

This is optional because the TTL + gossip combination should handle 99%+ of cases.

---

## 6. Implementation Roadmap

### Phase 1: Read-Through Cache (Immediate Value)
- Implement `MetabolismArcCache` with 10K capacity
- Wire into KU `get()` path
- Add cache hit/miss metrics
- **Estimated effort**: 2-3 days
- **Impact**: Eliminate redb reads for hot KUs → 100-200x faster for repeated queries

### Phase 2: Prefetch + Bypass (Graph Optimization)
- Add 1-hop selective prefetch on access
- Add cache bypass for DreamEngine and ConsolidationEngine
- **Estimated effort**: 1-2 days
- **Impact**: Reduce spreading activation latency by ~40% (neighbors pre-loaded)

### Phase 3: Gossip Invalidation (Distributed Coherence)
- Implement `CacheInvalidatePayload` encoding/decoding
- Wire into existing gossip infrastructure
- Add TTL expiry to cache entries
- **Estimated effort**: 2-3 days
- **Impact**: Correct cache behavior in multi-node deployment

### Phase 4: Eviction Tuning (Optimization)
- Integrate `metabolic_rate` into eviction scoring
- Add hub-node protection (bond count bonus)
- Add ConsolidationEngine's `PromoteToCore` pinning
- **Estimated effort**: 1-2 days
- **Impact**: ~10-20% improvement in cache hit rate vs. standard ARC

---

## 7. Rust Crate Recommendations

| Purpose | Crate | Notes |
|:---|:---|:---|
| ARC cache | `arc-cache` or custom | `arc-cache` on crates.io; or implement over `indexmap` |
| Concurrent cache | `moka` | Production-ready, TinyLFU-based, concurrent, TTL support |
| LRU fallback | `lru` | Simple, proven, O(1) — good for Phase 1 prototype |
| RwLock | `parking_lot` | Faster than std, supports lock upgrading |
| Timer/TTL | `tokio::time` | For async TTL expiry if using tokio runtime |
| Metrics | `metrics` crate | For cache hit/miss counters |

**Recommendation for Phase 1**: Start with `moka` — it provides concurrent, TTL-aware caching with TinyLFU (frequency + recency), which is conceptually similar to M-ARC but battle-tested. Integrate `metabolic_rate` as a custom weigher.

---

## 8. Memory Budget Analysis

```
Hot Tier Memory Budget:
  10,000 KUs × 400 bytes/KU           =  4.0 MB  (content + epigenetics)
  10,000 KUs × 5 neighbors × 34 bytes =  1.7 MB  (bond adjacency lists)
  Ghost lists (B1 + B2): 20,000 × 40  =  0.8 MB  (CID + metadata only)
  Index overhead (HashMap)              =  0.5 MB
  ─────────────────────────────────────────────────
  Total Hot Tier                        ≈  7.0 MB

QueryCache (existing):
  Current LRU with TTL                 ≈  varies

MetabolismStore (existing):
  100K entries × ~500 bytes            ≈ 50.0 MB (HashMap<[u8;32], KUMetabolism>)
  ─────────────────────────────────────────────────
  Total Memory (including Hot Tier)    ≈ 57.0 MB
```

This is well within reasonable bounds for a desktop/server application. The hot cache adds only ~7MB — less than 15% of the existing MetabolismStore memory.

---

## References

1. Megiddo & Modha, "ARC: A Self-Tuning, Low Overhead Replacement Cache" (USENIX FAST 2003) — Foundation for ARC algorithm
2. Neo4j Page Cache Architecture — Graph database caching patterns (node/relationship locality)
3. TigerGraph Memory Management — In-memory graph processing with memory pressure thresholds
4. Gossip Protocols for Distributed Systems — Rumor-mongering and anti-entropy patterns
5. TinyLFU (Ben Manes, 2015) — Frequency sketch with aging mechanism (used in Caffeine/Moka)
6. redb Architecture — MVCC, zero-copy reads, memory-mapped B-tree storage
7. OneBrain codebase: `metabolism.rs`, `metabolism_store.rs`, `graph_bio.rs`, `graph_dream.rs`, `epigenetics.rs`, `messages.rs`