# Storage Architecture Benchmark — redb vs Alternatives

> **Project**: OneBrain — Decentralized Knowledge Sharing Network  
> **Date**: 2026-07-06  
> **Status**: Research Complete  
> **Scope**: Evaluate redb as the storage engine for OneBrain, compare with alternatives, recommend strategy

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Current redb Usage](#2-current-redb-usage)
3. [redb Characteristics](#3-redb-characteristics)
4. [Alternatives Comparison](#4-alternatives-comparison)
5. [Benchmark Data](#5-benchmark-data)
6. [OneBrain Workload Analysis](#6-onebrain-workload-analysis)
7. [Compression Analysis](#7-compression-analysis)
8. [Async I/O Strategy](#8-async-io-strategy)
9. [Recommendation](#9-recommendation)
10. [Migration Strategy](#10-migration-strategy)
11. [References](#11-references)

---

## 1. Executive Summary

OneBrain currently uses **redb** (pure Rust embedded database, v2) for all persistent storage across 13 tables in 3 modules. After comprehensive evaluation against sled, RocksDB, SQLite, fjall, and LMDB, **redb remains the optimal choice** for OneBrain's current and near-term workload: read-heavy, low write rate, range scans on small values (16-172 bytes), single-writer model.

**Key findings:**
- redb's B+tree architecture is ideal for OneBrain's read-heavy, range-scan workload
- No compression benefit for values < 200 bytes regardless of engine choice
- Async wrappers unnecessary at current scale — blocking is acceptable for sub-100μs operations
- If future scale requires change, the abstraction layer makes backend swap a 2-3 day effort

---

## 2. Current redb Usage

### 2.1 Table Layout (13 tables across 3 modules, 2 redb files)

#### KuStorage (`ku-kql/src/storage.rs`) — 4 tables in main `.redb` file

| Table | Key | Value | Purpose |
|-------|-----|-------|---------|
| `kus` | CID (32B) | Core DNA wire bytes | Immutable Layer 1 content |
| `epigenetics` | CID (32B) | JSON serialized Epigenetics | Mutable Layer 2 metadata |
| `index_trust` | trust_score(u16 BE) + CID(32B) = 34B | empty | Range scan by trust |
| `index_concept` | concept_id(u64 BE) + CID(32B) = 40B | empty | Lookup by concept |

#### GraphStorage (`ku-kql/src/graph_storage.rs`) — 6 tables in `.graph.redb` file

| Table | Key | Value | Purpose |
|-------|-----|-------|---------|
| `edges_out` | src(32) + rel(1) + tgt(32) = 65B | BondMeta(9B) | Outgoing edges |
| `edges_in` | tgt(32) + rel(1) + src(32) = 65B | empty | Incoming edge index |
| `edges_type` | rel(1) + src(32) + tgt(32) = 65B | empty | Type-first lookup |
| `index_state` | state(1) + src(32) + rel(1) + tgt(32) = 66B | empty | State filter |
| `bond_weight` | weight(2) + src(32) + tgt(32) + rel(1) = 67B | empty | Weight-sorted scan |
| `edge_time` | ts(4) + src(32) + tgt(32) + rel(1) = 69B | empty | Temporal ordering |

#### PersistentConceptDict (`ku-core/src/persistent_concept_dict.rs`) — 3 tables

| Table | Key | Value | Purpose |
|-------|-----|-------|---------|
| `concepts` | name (str) | JSON ConceptEntry | Concept lookup |
| `ids` | id (u64) | name (str) | Reverse mapping |
| `meta` | "next_id" | u64 | ID sequence |

### 2.2 Key Observations

1. **No `spawn_blocking` usage** — all DB access is synchronous
2. **Feature-gated**: `persist` flag in ku-core, `storage` flag in ku-kql
3. **redb v2** used (`redb = { version = "2", optional = true }`)
4. All operations are single-writer serialized; read operations use `begin_read()` transactions
5. Graph storage writes to all 6 tables in a single write transaction (atomic)
6. All keys are composite byte arrays using big-endian encoding for sort ordering

---

## 3. redb Characteristics

### 3.1 Architecture

| Property | Detail |
|----------|--------|
| **Data structure** | Copy-on-Write B+tree with MVCC |
| **Concurrency** | Multi-reader / single-writer (within single process) |
| **Language** | Pure Rust — no C dependencies |
| **ACID** | Full ACID compliance |
| **Stability** | 1.0+ since 2023, stable file format |
| **Cross-compile** | Easy — no native dependencies |
| **Max key/value** | u32 length field (~4 GiB theoretical) |
| **API style** | Type-safe generic tables |

### 3.2 Strengths for OneBrain

- **Read performance**: B+tree provides excellent point lookups and range scans
- **MVCC readers**: Multiple concurrent readers don't block each other or the writer
- **Pure Rust**: Memory safety, easy cross-compilation, no C toolchain needed
- **Crash safety**: CoW semantics mean no WAL corruption scenarios
- **Type-safe API**: Compile-time key/value type checking reduces runtime errors
- **Small footprint**: Minimal dependencies, small binary size

### 3.3 Limitations

- **Single-process access**: Cannot share database file between processes
- **Write throughput**: Lower than LSM-tree engines for write-heavy workloads
- **No compression**: No built-in compression support
- **No multi-threaded writes**: Single writer lock means write serialization
- **Scale ceiling**: Not tested beyond hundreds of GB

---

## 4. Alternatives Comparison

### 4.1 Full Comparison Matrix

| Feature | redb | sled | RocksDB | SQLite | fjall | LMDB |
|---------|------|------|---------|--------|-------|------|
| **Architecture** | CoW B+tree | Log-structured | LSM-tree | B-tree | LSM-tree | CoW B+tree |
| **Language** | Pure Rust | Pure Rust | C++ (bindings) | C (bindings) | Pure Rust | C (bindings) |
| **Status 2025** | Stable 1.0+ | Stalled/Abandoned | Very Mature | Very Mature | Active v3.0 | Very Mature |
| **ACID** | Yes | Partial | Yes (tunable) | Yes | Yes | Yes |
| **Write throughput** | Good | Poor/risky | Excellent | Moderate | Very Good | Moderate |
| **Read throughput** | Excellent | Fair | Good | Good | Good | Excellent |
| **Range scans** | Excellent (B-tree) | Fair | Good | Excellent (SQL) | Good | Excellent |
| **Concurrent readers** | Yes (MVCC) | Yes | Yes | WAL mode | Yes | Yes (lock-free) |
| **Single writer** | Yes | Yes | Concurrent writers | Limited | Yes | Yes |
| **Compression** | No | No | LZ4/Snappy/Zstd | No | Configurable | No |
| **Disk efficiency** | Good | Fair | Good (with compression) | Good | Good | Can waste space (CoW) |
| **Crash safety** | Excellent | Questionable | Configurable | Excellent | Good | Excellent |
| **Rust ecosystem** | Native | Native | rust-rocksdb | rusqlite | Native | heed |
| **Cross-compile** | Easy | Easy | Hard (C++ toolchain) | Moderate | Easy | Moderate |
| **Max tested scale** | Hundreds of GB | Unknown | Multi-TB | Multi-TB | Hundreds of GB | TB+ |
| **API ergonomics** | Excellent (type-safe) | Good | Complex (many knobs) | SQL | Good | Good (heed) |

### 4.2 Per-Engine Assessment

#### sled — ❌ Not Recommended
- Development stalled/abandoned
- Known data corruption issues
- No longer suitable for new production work

#### RocksDB — ⚠️ Overkill
- Excellent write throughput via LSM-tree
- But: requires C++ toolchain, complex configuration (100+ knobs)
- Write optimization unnecessary for OneBrain's 1-10 KU/min write rate
- Best for: write-heavy workloads exceeding 1000 writes/sec

#### SQLite — ⚠️ Viable Alternative
- Excellent maturity, battle-tested at massive scale
- SQL interface adds overhead for simple key-value operations
- Could work but adds abstraction mismatch (SQL over byte keys)
- Best for: applications needing SQL queries or multi-process access

#### fjall — ✅ Best Alternative if Needed
- Pure Rust, actively maintained (v3.0 in 2026)
- LSM-tree with good write throughput
- Most natural migration path from redb
- Best for: when write rate exceeds redb's comfort zone (>1000/sec)

#### LMDB — ⚠️ Strong but Requires C
- Excellent read performance (memory-mapped)
- CoW B+tree like redb (similar architecture)
- Requires C bindings (via `heed` crate)
- Best for: read-heavy workloads needing multi-process access

---

## 5. Benchmark Data

### 5.1 Approximate Performance Ranges

For workloads with small keys (32-69B) and small values (9-172B):

| Engine | Random Reads/sec | Random Writes/sec | Range Scan/sec | Notes |
|--------|-----------------|-------------------|----------------|-------|
| **redb** | ~200K-500K | ~50K-150K | Excellent | Single-threaded, with fsync |
| **LMDB** | ~300K-800K | ~50K-200K | Excellent | Memory-mapped |
| **RocksDB** | ~100K-400K | ~200K-800K | Good | LSM advantage for writes |
| **fjall v3** | ~150K-400K | ~100K-500K | Good | Pure Rust LSM |
| **SQLite** | ~50K-200K | ~20K-80K | Good (SQL) | SQL parsing overhead |

> **Note**: These are approximate ranges from various community benchmarks. Actual performance depends on hardware, fsync settings, and concurrency. For authoritative results, use [rust-storage-bench](https://github.com/marvin-j97/rust-storage-bench).

### 5.2 OneBrain-Specific Projections

| Metric | Current Load | redb Capacity | Headroom |
|--------|-------------|---------------|----------|
| **Reads/sec** | ~10-100 (queries) | ~200K-500K | **2000-50000×** |
| **Writes/sec** | ~0.02-0.17 (1-10 KU/min) | ~50K-150K | **>300000×** |
| **Database size** | <10MB (early stage) | Hundreds of GB | **>10000×** |
| **Table count** | 13 | No hard limit | N/A |

Conclusion: redb has **orders of magnitude more capacity** than OneBrain currently needs.

---

## 6. OneBrain Workload Analysis

### 6.1 Workload Profile

| Dimension | Characteristic | Best Engine Type |
|-----------|---------------|------------------|
| **Read/write ratio** | Read-heavy (10-100 reads per write) | B-tree (redb, LMDB) |
| **Write rate** | Low (1-10 KU/min) | Any engine handles this |
| **Access pattern** | Point lookups + range scans | B-tree excels |
| **Value sizes** | Tiny (9-172 bytes) | All engines equal |
| **Key sizes** | Medium (32-69 bytes composite) | All engines equal |
| **Concurrency** | Single writer, multiple readers | redb, LMDB |
| **Transaction scope** | Multi-table atomic writes | redb native support |
| **Durability** | Required (storage rewards depend on epochs_stored) | ACID required |

### 6.2 Verdict: B-tree Sweet Spot

OneBrain's workload sits squarely in the **B-tree sweet spot**:
- Read-heavy → B-tree read amplification = 1 (LSM = 1-N levels)
- Range scans → B-tree stores data sorted on disk (LSM may span levels)
- Low write rate → B-tree write amplification is acceptable
- Small values → No compression benefit regardless of engine

---

## 7. Compression Analysis

### 7.1 Why Compression Doesn't Help

For OneBrain's value sizes (16-172 bytes):

| Factor | Analysis |
|--------|----------|
| **Compression header overhead** | LZ4: ~11 bytes header. For a 16-byte value, overhead = 69% of data size |
| **Minimum compressible size** | Rule of thumb: compression returns diminish below 100-200 bytes |
| **OneBrain wire bytes** | Already binary-encoded (CBOR) — minimal redundancy |
| **CPU cost** | Compress/decompress cycle costs more than reading 172 bytes from disk |
| **Block-level compression** | Could help if engine batches many KVs into a page, but redb doesn't support this |

### 7.2 Recommendation

**Do not add compression** at the storage engine level. If compression is ever needed:
- Apply at the **application level** by batching multiple KUs into larger blocks before storage
- Consider only if database size exceeds 10GB and disk I/O becomes a bottleneck

---

## 8. Async I/O Strategy

### 8.1 Options Evaluated

| Strategy | Complexity | Overhead | Best For |
|----------|-----------|----------|----------|
| **Accept blocking** | None | None | Sub-100μs operations on small values |
| **`spawn_blocking`** | Low | Thread pool overhead | Sporadic DB ops in async context |
| **Dedicated DB thread** | Medium | Channel overhead | High-frequency access patterns |

### 8.2 Recommendation

**Accept blocking** for now:
- redb operations on small values (9-172B) complete in ~10-50μs
- At OneBrain's write rate (1-10/min), blocking is negligible
- No async runtime pollution with `spawn_blocking` overhead

**Upgrade path**: When/if async becomes necessary:
1. Wrap `KuStorage`/`GraphStorage` methods with `tokio::task::spawn_blocking`
2. This is a purely additive change — no API redesign needed
3. Estimated effort: 1 day

---

## 9. Recommendation

### 9.1 Decision: Stay with redb

**redb is the right choice for OneBrain's current and near-term needs:**

1. ✅ **Perfect workload match**: Read-heavy + low write rate + range scans = B+tree sweet spot
2. ✅ **Pure Rust**: No build complexity, memory safety, easy cross-compilation
3. ✅ **Stable API**: v2 is mature, no migration risk
4. ✅ **Small values**: No compression benefit regardless of engine choice
5. ✅ **Single writer**: Natural fit for redb's concurrency model
6. ✅ **ACID**: Required for storage reward continuity (epochs_stored)
7. ✅ **Crash safety**: CoW semantics prevent corruption

### 9.2 When to Reconsider

| Trigger | Threshold | Recommended Alternative |
|---------|-----------|------------------------|
| Write rate exceeds comfort zone | >1000 KU/sec sustained | fjall (pure Rust LSM) |
| Database size grows very large | >100 GB | LMDB or RocksDB |
| Need multi-process access | Multiple processes need DB | SQLite WAL or LMDB |
| Need concurrent writes | Multiple writer threads | RocksDB |
| Async becomes critical | High-concurrency async paths | Add `spawn_blocking` wrapper |

---

## 10. Migration Strategy

### 10.1 Abstraction Layer Advantage

All redb access is behind well-defined abstractions:

| Abstraction | Module | Tables |
|-------------|--------|--------|
| `KuStorage` | `ku-kql/src/storage.rs` | 4 tables |
| `GraphStorage` | `ku-kql/src/graph_storage.rs` | 6 tables |
| `PersistentConceptDict` | `ku-core/src/persistent_concept_dict.rs` | 3 tables |

Swapping the backend requires **only changing the internal implementation** of these 3 structs. The composite key scheme is engine-agnostic (raw byte arrays).

### 10.2 Migration Effort Estimate

| Step | Effort |
|------|--------|
| Replace `redb` imports with new engine | 0.5 day |
| Adapt table creation / transaction API | 1 day |
| Update error types and mappings | 0.5 day |
| Test suite adaptation | 0.5 day |
| Performance validation | 0.5 day |
| **Total** | **2-3 days** |

### 10.3 Feature Flag Approach

The existing feature-gate pattern (`persist`, `storage`) makes it straightforward to add an alternative backend behind a new feature flag:

```toml
[features]
storage-redb = ["dep:redb"]      # Current default
storage-fjall = ["dep:fjall"]     # Future alternative
```

---

## 11. References

1. **redb** — [github.com/cberner/redb](https://github.com/cberner/redb) — Pure Rust embedded database
2. **fjall** — [github.com/fjall-rs/fjall](https://github.com/fjall-rs/fjall) — Pure Rust LSM-tree storage engine
3. **RocksDB** — [rocksdb.org](https://rocksdb.org) — High-performance LSM key-value store
4. **LMDB / heed** — [github.com/meilisearch/heed](https://github.com/meilisearch/heed) — Rust bindings for LMDB
5. **rust-storage-bench** — [github.com/marvin-j97/rust-storage-bench](https://github.com/marvin-j97/rust-storage-bench) — Community benchmark suite
6. **OneBrain codebase**: `ku-kql/src/storage.rs`, `ku-kql/src/graph_storage.rs`, `ku-core/src/persistent_concept_dict.rs`

---

> [!NOTE]
> This benchmark focuses on **engine selection**, not storage layer optimization. For tiering and caching strategy, see `03_hot_cold_tiering_research.md`.
