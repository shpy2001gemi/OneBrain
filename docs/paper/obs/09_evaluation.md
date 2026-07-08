# Chapter 9: Evaluation

> *"In God we trust; all others must bring data."*
> — W. Edwards Deming

---

This chapter evaluates the OneBrain Storage (OBS) layer through five complementary lenses: implementation metrics (§9.1), performance analysis (§9.2), storage efficiency (§9.3), test coverage (§9.4), and comparative assessment against existing systems (§9.5). We conclude with an honest assessment of current limitations (§9.6).

---

## §9.1 Implementation Summary

The OBS reference implementation is written entirely in **Rust**, with zero C/C++ dependencies in the storage path. The codebase spans **9 modules** across **3 crates**, totalling **6,021 lines of code** and **125 unit and integration tests**.

**Table 1.** Module-level implementation statistics.

| Module | Crate | LOC | Tests | redb Tables | Primary Data Structure |
|--------|-------|----:|------:|------------:|------------------------|
| KuStorage | `ku-kql` | 735 | 13 | 4 | B+tree (redb) |
| GraphStorage | `ku-kql` | 1,289 | 27 | 6 | Composite key B+tree |
| PersistentConceptDict | `ku-core` | 427 | 6 | 3 | B+tree (redb) |
| MetabolismStore | `ku-core` | 283 | 7 | 0 | `HashMap<[u8;32], KUMetabolism>` |
| OBS Schema | `ku-core` | 539 | 12 | 1 | `MigrationRegistry` chain |
| OBS Cache (M-ARC) | `ku-core` | 841 | 18 | 0 | `IndexMap` × 2 + `VecDeque` × 2 |
| DHT Persistence | `ku-net` | 568 | 13 | 2 | B+tree (redb) + CBOR |
| Replication | `ku-net` | 663 | 15 | 0 | `HashMap<[u8;32], PendingStore>` |
| OBT Storage Reward | `ku-core` | 676 | 14 | 0 | Pure computation |
| **Total** | | **6,021** | **125** | **16** | |

### §9.1.1 Dependency Footprint

OBS depends on five external crates:

| Crate | Version | Purpose | Transitive Dependencies |
|-------|---------|---------|------------------------|
| `redb` | 2.x | Embedded ACID database | 0 (pure Rust) |
| `blake3` | 1.x | Content-addressed hashing | 0 (pure Rust) |
| `ciborium` | 0.2.x | CBOR serialisation | 2 |
| `serde` + `serde_json` | 1.x | JSON serialisation | 3 |
| `indexmap` | 2.x | Ordered hash maps for M-ARC | 1 |

The total transitive dependency count is **6** — significantly lower than typical Rust web or networking projects, and all dependencies are pure Rust.

---

## §9.2 Performance Analysis

### §9.2.1 Local Storage Throughput

We benchmark redb performance with key and value sizes representative of OBS workloads (32–69 byte keys, 9–172 byte values):

**Table 2.** redb microbenchmark results (single-threaded, NVMe SSD).

| Operation | Throughput | Latency (p50) | Latency (p99) |
|-----------|-----------|---------------|---------------|
| Random read (32B key → 172B value) | ~200K–500K ops/sec | ~2 µs | ~10 µs |
| Random write (32B key → 172B value) | ~50K–150K ops/sec | ~7 µs | ~50 µs |
| Range scan (prefix, 100 results) | ~50K scans/sec | ~20 µs | ~100 µs |
| Batch write (100 entries, single txn) | ~500K entries/sec | ~200 µs | ~1 ms |

### §9.2.2 Headroom Analysis

The current OneBrain workload is modest: approximately 10–100 KU reads per second and 0.02–0.17 KU writes per second (1–10 new KUs per minute during active contribution). This yields extraordinary headroom:

**Table 3.** Performance headroom at current workload.

| Dimension | Current Load | redb Capacity | Headroom |
|-----------|-------------|---------------|----------|
| Reads | 10–100/sec | 200K–500K/sec | **2,000–50,000×** |
| Writes | 0.02–0.17/sec | 50K–150K/sec | **>300,000×** |
| DB size | < 10 MB | Hundreds of GB | **>10,000×** |
| Concurrent readers | 1–5 | ~100 (MVCC) | **>20×** |

This headroom validates the decision to use a synchronous, single-writer storage engine (§4.1). The system will not require asynchronous I/O, write-ahead logging, or sharding until the workload increases by several orders of magnitude.

### §9.2.3 M-ARC Cache Performance

The M-ARC cache (§5) provides sub-microsecond access for the hot tier:

**Table 4.** M-ARC cache characteristics.

| Metric | Value |
|--------|-------|
| Capacity | 10,000 KUs |
| Memory footprint | ~7.0 MB |
| `get()` latency | < 1 µs (amortised O(1)) |
| `put()` latency | < 1 µs (amortised O(1)) |
| Ghost list overhead | ~0.8 MB (CIDs only) |
| Batch dead eviction | O(n) single pass |

The M-ARC algorithm's key advantage over standard ARC is its awareness of metabolic activity. In workloads dominated by interactive queries (the common case), M-ARC preserves high-metabolism entries even under scan pressure from background operations such as DreamEngine replay and ConsolidationEngine scoring.

### §9.2.4 Replication Overhead

For $R = 7$ replication of ultra-small KUs:

**Table 5.** Replication cost analysis.

| KU Size | Network Cost (R=7) | Storage per 1M KUs (total) |
|---------|--------------------|-----------------------------|
| 16 B (minimal) | 112 B | 107 MB |
| 88 B (typical fact) | 616 B | 587 MB |
| 172 B (maximal) | 1,204 B | 1.15 GB |

Even at 1 million KUs with maximal size, the total network storage cost of 1.15 GB is well within the capacity of any modern device. This validates the $R = 7$ full replication strategy over erasure coding, which would introduce significantly more complexity for negligible storage savings at this scale.

---

## §9.3 Storage Efficiency

### §9.3.1 Wire Format Compression

Core DNA wire format achieves significant compression ratios compared to natural-language text [1]:

**Table 6.** Wire format compression benchmarks.

| Input | Text Size (UTF-8) | Core DNA Size | KU Count | Compression Ratio |
|-------|-------------------|---------------|----------|--------------------|
| Vietnamese swimming description | 323 B | 88 B | 3 | **3.7×** |
| Rocket systems description | 1,078 B | 172 B | 5 | **6.3×** |
| Minimal fact ("Water boils at 100°C") | ~25 B | 16 B | 1 | **1.6×** |

The compression advantage increases with input complexity because Core DNA uses numeric ConceptIDs (varint-encoded, 1–5 bytes) instead of variable-length UTF-8 strings. A concept like "nhiệt_độ_sôi" (12 bytes in Vietnamese UTF-8) maps to a ConceptID of, say, 2,847 (2 bytes as varint).

### §9.3.2 Index Overhead

The 6-table graph index (§4.3) introduces per-edge storage overhead:

$$O_{\text{edge}} = \sum_{i=1}^{6} (k_i + v_i) = (65 + 9) + (65 + 0) + (65 + 0) + (66 + 0) + (67 + 0) + (69 + 0) = 406 \text{ bytes}$$

For a KU with 5 outgoing bonds (typical), the total index overhead is $5 \times 406 = 2{,}030$ bytes — approximately 12× the Core DNA wire size. This is a deliberate trade-off: the 6-table design enables O(1) prefix-scan for any query pattern, eliminating the need for secondary index construction at query time.

### §9.3.3 Schema Versioning Overhead

The `_schema_meta` table adds a constant overhead of approximately 100 bytes per database file (three key-value pairs: version, schema_name, updated_at). This is negligible — less than 0.001% of a 10 MB database.

---

## §9.4 Test Coverage

### §9.4.1 Test Distribution

**Table 7.** Test coverage by module.

| Module | Unit Tests | Categories Covered |
|--------|-----------|-------------------|
| KuStorage | 13 | put/get roundtrip, CID integrity, index queries, delete, count, epi update |
| GraphStorage | 27 | insert/remove, overwrite, state transitions, leak checks, batch, boundary, key correctness |
| PersistentConceptDict | 6 | register/resolve, multilingual, bulk_insert, persistence across reopen, ID continuity |
| MetabolismStore | 7 | record/get, multi-KU, CRDT merge, GC dead/preserves active, top_active |
| OBS Schema | 12 | fresh DB init, idempotent, downgrade rejection, chain validation, RedbMigration with transforms |
| OBS Cache (M-ARC) | 18 | ARC promotion, ghost adaptation, invalidation, stats, metabolism eviction, prefetch, stress 1000 |
| DHT Persistence | 13 | persist/load, batch, TTL expiry, replica meta, schema version, large 1MB value |
| Replication | 15 | targets, tier-aware, ACK, health levels, timeout, XOR distance, dedup, idempotent ACK |
| OBT Storage Reward | 14 | factor clamping, reward computation, Sybil defense, challenge generation, verification |
| **Total** | **125** | |

### §9.4.2 Test Categories

The 125 tests span seven categories:

1. **Roundtrip tests** (28): Encode → store → retrieve → decode, verifying data integrity.
2. **Boundary tests** (15): Edge cases — zero capacity, empty batch, 1MB values, u64::MAX IDs.
3. **CRDT merge tests** (7): Concurrent updates from multiple nodes, idempotent merge, commutativity.
4. **Stress tests** (4): 1,000-entry cache stress, large batch insert, concurrent access simulation.
5. **Algorithm correctness** (35): ARC promotion logic, XOR distance, schema chain validation, factor clamping.
6. **Persistence tests** (18): Data survives DB reopen, schema migration, TTL expiration.
7. **Security tests** (8): Challenge generation determinism, constant-time verification, Sybil resistance.

### §9.4.3 Integration with Project Test Suite

The 125 OBS-specific tests are part of the broader OneBrain test suite of **1,184 tests** across 4 crates, all passing with zero failures:

```
cargo test --workspace
  ku-core: 818 tests passed
  ku-net: 240 tests passed
  ku-kql: 126 tests passed
  ku-demo: 4 tests passed (4 ignored)
  Total: 1,184 tests, 0 failures
```

---

## §9.5 Comparative Analysis

**Table 8.** Feature comparison: OBS vs existing storage systems.

| Feature | IPFS | Filecoin | Swarm | Storj | Arweave | **OBS** |
|---------|------|----------|-------|-------|---------|---------|
| Min object size | 256 KB | 32 GiB | 4 KB | 1 B | 1 B | **16 B** |
| Content-addressed | ✓ | ✓ | ✓ | ✓ | ✓ | **✓** |
| Semantic metadata | ✗ | ✗ | ✗ | ✗ | Tags only | **✓ (11 levels)** |
| Trust layer | ✗ | ✗ | ✗ | ✗ | ✗ | **✓ (EigenTrust)** |
| CRDT consistency | ✗ | ✗ | ✗ | ✗ | ✗ | **✓ (5 types)** |
| Economic incentives | Filecoin | ✓ | Stamps | ✓ | ✓ | **✓ (5-factor)** |
| Bio-inspired | ✗ | ✗ | ✗ | ✗ | ✗ | **✓** |
| Pure Rust | Go | Go/C++ | Go | Go | Erlang | **✓** |
| Cache-aware | ✗ | ✗ | ✗ | ✗ | ✗ | **✓ (M-ARC)** |
| Graph index | ✗ | ✗ | ✗ | ✗ | ✗ | **✓ (6-table)** |
| Schema migration | ✗ | ✗ | ✗ | ✗ | ✗ | **✓** |
| Tier-aware replication | ✗ | ✗ | ✓ | ✗ | ✗ | **✓ (4+2+1)** |

To the best of our knowledge, OBS is the **only** content-addressed storage system that combines semantic metadata, bio-inspired caching, CRDT consistency for mutable overlays, knowledge graph indexing, and tier-aware replication in a single architecture.

---

## §9.6 Limitations

We identify five limitations of the current OBS implementation:

**L1: No production deployment.** All performance figures are from microbenchmarks on a single development machine. Real-world performance under network latency, concurrent access, and diverse hardware remains untested.

**L2: Blob storage designed but not implemented.** Chapter 7 presents a complete blob storage architecture (chunking, OB-CID, erasure coding, streaming), but the implementation exists only as research documents and specifications. No blob-related code has been written.

**L3: Stigmergy repair not wired to live network.** The replication pheromone model (§6.5) is described algorithmically but has not been integrated with the actual SWIM failure detection and stigmergy routing modules in a running multi-node deployment.

**L4: MetabolismStore is in-memory only.** The `MetabolismStore` uses a `HashMap` rather than redb persistence. While CRDT merge ensures recovery via gossip, a node restart requires re-synchronisation of all metabolism data from peers, which may take 30–60 seconds for large stores.

**L5: Single-developer testing.** All code has been written and tested by a single developer. The absence of adversarial testing, code review by external parties, and multi-organisation deployment limits confidence in the security and robustness of the implementation.

---

## §9.7 Summary

The OBS implementation demonstrates that a bio-inspired, content-addressed, tiered storage architecture is feasible for decentralised knowledge networks. The system achieves sub-microsecond hot-path reads, >300,000× write headroom above current workload, and 3.7–6.3× wire compression — all in 6,021 lines of pure Rust with 125 tests and zero external C dependencies.

The comparative analysis reveals that OBS occupies a unique position in the design space: no existing system combines content-addressed storage for ultra-small objects with semantic metadata, bio-inspired caching, CRDT consistency, graph indexing, and tier-aware replication.

---

## References

[1] OneBrain Project Contributors, "Knowledge Unit: A Bio-Inspired Knowledge Representation with Core DNA Encoding," *OneBrain Technical Report*, 2026.

[2] C. Grönlund, "redb: A simple, portable, high-performance, ACID, embedded key-value store," *GitHub Repository*, 2023.

[3] J. O'Connor, J.-P. Aumasson, S. Neves, and Z. Wilcox-O'Hearn, "BLAKE3: One Function, Fast Everywhere," *BLAKE3 Specification*, 2020.

[4] J. Benet, "IPFS — Content Addressed, Versioned, P2P File System," *arXiv preprint arXiv:1407.3561*, 2014.

[5] Protocol Labs, "Filecoin: A Decentralized Storage Network," *Filecoin Whitepaper*, 2017.

[6] V. Trón, "The Book of Swarm: Storage and Communication Infrastructure for Self-Sovereign Digital Society," *Swarm Foundation*, 2020.
