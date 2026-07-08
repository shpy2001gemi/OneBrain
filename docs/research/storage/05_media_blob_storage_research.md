# Research Topic 5: Media/Blob Storage Design for OneBrain

> **Status**: Research Draft  
> **Date**: 2026-07-06  
> **Author**: OneBrain Research  
> **Scope**: Architecture, chunking, CID format, replication, economics, streaming, metadata  
> **Cross-references**: [04_STORAGE_REWARD.md](../../specs/obt/04_STORAGE_REWARD.md), [FEATURE_TREE.md](../../features/FEATURE_TREE.md)

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Q1: Blob Architecture — Separate Store vs Extended KU](#2-q1-blob-architecture)
3. [Q2: Chunking Strategy](#3-q2-chunking-strategy)
4. [Q3: Blob CID & Merkle DAG](#4-q3-blob-cid--merkle-dag)
5. [Q4: Blob Replication](#5-q4-blob-replication)
6. [Q5: Storage Economics for Blobs](#6-q5-storage-economics-for-blobs)
7. [Q6: Streaming & Range Requests](#7-q6-streaming--range-requests)
8. [Q7: Content Type & Metadata](#8-q7-content-type--metadata)
9. [Comparative Analysis](#9-comparative-analysis)
10. [Recommendation Summary](#10-recommendation-summary)

---

## 1. Executive Summary

OneBrain's Knowledge Units (KUs) are tiny structured data objects (16–172 bytes) encoded in the Core DNA v6 binary format. They are semantic, typed, and content-addressed via `BLAKE3(wire_bytes)`. However, real-world knowledge frequently includes images, PDFs, videos, audio, and other binary media that cannot fit within the KU envelope. **There is currently zero blob storage infrastructure in OneBrain.**

This research analyzes seven design dimensions for adding media/blob storage to OneBrain, examining tradeoffs across IPFS, Filecoin, Swarm, Storj, Sia, and Arweave. The central recommendation is a **hybrid layered architecture**: KUs remain pure semantic units that _reference_ blobs via CID links; blobs live in a dedicated Blob Store with its own chunking, replication, and economics — all unified under the OneBrain content-addressing scheme using an extended CID format based on BLAKE3.

---

## 2. Q1: Blob Architecture

### Problem Statement

KUs are capped at ~172 bytes (Core DNA v6). A single photograph is typically 2–10 MB. A video lecture can be 500 MB–2 GB. The KU format cannot and should not expand to accommodate blobs directly — doing so would destroy the compact, semantic, machine-queryable nature that makes KUs unique.

### Options Analysis

#### Option A: KU References Blob via CID Link

```
KU (Core DNA, ≤172B)
├── gene: Fact
├── qualifiers: [Triple, Quantity, ...]
├── MediaRef(0x1B): system=0x01, cid=[32B BLAKE3]  ← points to blob
└── end
         │
         ▼
Blob Store (separate subsystem)
├── BlobManifest (root CID → chunk list)
└── Chunks: [chunk_0, chunk_1, ..., chunk_n]
```

**Pros:**
- KU format unchanged — zero migration cost
- Clean separation of concerns (semantic vs. binary)
- `MediaRef` opcode (0x1B) already exists in Core DNA v6 spec
- Blobs can be garbage-collected independently
- KU replication (R=7) and blob replication can use different strategies

**Cons:**
- Two subsystems to maintain
- Dangling references if blob is GC'd but KU still references it

#### Option B: Blob as Special KU Type (Gene::Media)

Gene type 4 (`MediaExperience`) already exists but encodes _reactions to media_, not media data itself. A new Gene type (e.g., `Gene::BlobCarrier`) would embed binary data directly.

**Pros:**
- Single data model
- Reuses existing KU replication

**Cons:**
- **Breaks KU size invariant** — wire format assumes ≤172B
- DHT routing optimized for tiny KUs; 100MB blobs would overwhelm it
- Every KU handler must now handle arbitrary-size data
- Encoding pipeline (3-tier: NL → concept → binary) makes no sense for raw bytes
- Would require massive refactoring of `ku-core` and `ku-net`

#### Option C: External Storage (IPFS/Filecoin)

KU stores an IPFS CIDv1 string in the `MediaRef` field.

**Pros:**
- Zero storage infrastructure to build
- Leverage IPFS's existing 200K+ node network

**Cons:**
- **External dependency** — OneBrain's availability depends on IPFS network health
- IPFS has no persistence guarantee without pinning services (data disappears when no node hosts it)
- CID format mismatch: OneBrain uses raw BLAKE3 [32B]; IPFS uses CIDv1 with multihash (typically SHA-256)
- No control over chunking, replication, or economics
- OBT storage incentives cannot extend to IPFS nodes
- Latency and availability unpredictable

### ★ Recommendation: Option A — KU References Blob via CID Link

This is the clear winner. It preserves OneBrain's architectural purity while enabling media storage. The existing `MediaRef` opcode (0x1B) provides the hook:

```
MediaRef layout (existing in Core DNA v6):
┌──────────┬─────────┬──────────────────────┐
│ 0x1B     │ system  │ id_bytes             │
│ (opcode) │ (u8)    │ (varint len + data)  │
└──────────┴─────────┴──────────────────────┘

Proposed system values:
  0x00 = External URL (existing)
  0x01 = OneBrain Blob CID (NEW) → id = [32B BLAKE3 root hash]
  0x02 = IPFS CIDv1 (future interop)
```

**Key design decision**: The `system=0x01` value for OneBrain-native blob references fits within the existing wire format. No Core DNA changes needed.

---

## 3. Q2: Chunking Strategy

### Why Chunk?

P2P transfer of large files requires chunking for:
- **Parallel download** from multiple peers (swarming)
- **Resumability** — interrupted transfers restart from the last complete chunk
- **Deduplication** — identical chunks across different files stored once
- **Verification** — each chunk independently verifiable via hash

### Options Comparison

| Strategy | Chunk Size | Dedup Quality | CPU Cost | Complexity |
|----------|-----------|---------------|----------|------------|
| Fixed 4KB | 4 KB | Poor | Negligible | Trivial |
| Fixed 64KB | 64 KB | Poor | Negligible | Low |
| Fixed 256KB | 256 KB (IPFS default) | Poor | Negligible | Low |
| Rabin CDC | ~64KB avg (tunable) | Excellent | Moderate | High |
| Buzhash CDC | ~64KB avg | Excellent | Low–Moderate | Moderate |
| FastCDC | ~64KB avg | Excellent | Low | Moderate |

### Content-Defined Chunking (CDC) Deep Dive

CDC uses a rolling hash (Rabin fingerprint or Buzhash) over a sliding window. A chunk boundary is declared when the hash meets a condition (e.g., last N bits = 0). This makes boundaries dependent on **content**, not offset.

**Critical advantage**: If 10 bytes are inserted at the beginning of a file, CDC only changes 1–2 chunk boundaries. Fixed-size chunking shifts _every_ boundary, invalidating all chunk hashes.

**Rabin fingerprinting**: Uses polynomial arithmetic over GF(2). The hash of window `w[i..i+W]` is computed incrementally from `w[i-1..i+W-1]` in O(1). Boundary condition: `hash mod 2^B == 0` where B controls average chunk size (`avg_size ≈ 2^B`).

**FastCDC** (2016, Wen Xia et al.): A gear-hash based CDC that is 3–10× faster than Rabin while achieving comparable dedup ratios. Uses three mask levels (minimum, normal, maximum) to bound chunk sizes, eliminating pathological tiny/huge chunks.

### OneBrain-Specific Considerations

| Factor | Implication |
|--------|------------|
| Mobile nodes (Tier 5–7) | Prefer larger chunks (fewer DHT lookups, less overhead) |
| Bandwidth-constrained | Larger chunks = fewer round-trips = faster completion |
| DHT overhead | Each chunk CID occupies DHT routing space; fewer chunks = less DHT pressure |
| Dedup value | OneBrain stores _knowledge_, not file archives; dedup across blobs is moderate |
| Verification | BLAKE3 is extremely fast (3–4 GB/s on modern CPUs); chunk size doesn't bottleneck hashing |

### ★ Recommendation: Fixed 256KB chunks, CDC optional for v2

**Phase 1 (v0.1):** Fixed 256KB chunks.
- Simple, predictable, low implementation cost
- 256KB aligns with IPFS default — interop-friendly
- At 256KB: a 100MB file = 400 chunks, a 1GB file = 4,000 chunks
- Each chunk CID = 32B BLAKE3 hash → manifest for 1GB file ≈ 128KB
- BLAKE3 hashes 256KB in ~65µs — negligible overhead

**Phase 2 (v0.3+):** Optional FastCDC mode for files where dedup matters (e.g., document versioning, incremental backups). Use `BlobManifest.chunking_strategy` field to indicate which algorithm was used.

```rust
enum ChunkingStrategy {
    Fixed256K = 0,       // Default, 256KB fixed-size
    FastCDC64K = 1,      // FastCDC with 64KB average (future)
}
```

---

## 4. Q3: Blob CID & Merkle DAG

### Current OneBrain CID Format

```
CID = BLAKE3(wire_bytes)  →  [u8; 32]  (raw 32 bytes)
```

This is a **bare hash** — no version, no codec indicator, no hash algorithm prefix. It works perfectly for KUs because all KUs use the same hash (BLAKE3) and the same format (Core DNA wire bytes).

### IPFS CIDv1 Format

```
CIDv1 = <version:varint> <codec:varint> <multihash>
multihash = <hash-fn:varint> <digest-size:varint> <digest:bytes>

Example: CIDv1 with SHA-256:
  0x01           version = 1
  0x55           codec = raw
  0x12 0x20      sha2-256, 32 bytes
  <32 bytes>     digest
  ─────────────
  Total: 36 bytes (vs 32 for raw hash)
```

### Should OneBrain Adopt CIDv1?

| Dimension | Raw BLAKE3 [32B] | CIDv1 [36B+] |
|-----------|-------------------|---------------|
| Size overhead | 0 bytes | +4B minimum (version + codec + hash-fn + length) |
| Self-describing | ❌ No — must know it's BLAKE3 | ✅ Yes — hash algorithm embedded |
| Future-proof | ❌ Locked to BLAKE3 | ✅ Can switch hash algorithms |
| IPFS interop | ❌ Incompatible | ✅ Native interop |
| Simplicity | ✅ Trivial | ⚠️ Requires multiformat libraries |
| KU impact | Zero | +4B per CidRef → tighter KU budget |

### ★ Recommendation: OneBrain-Native CID (OB-CID) with Optional CIDv1 Bridge

**For blobs**: Use a minimal self-describing format:

```
OB-CID (Blob) = [version:u8] [type:u8] [blake3_hash:32B]
  version = 0x01
  type    = 0x00 (KU) | 0x01 (BlobManifest) | 0x02 (BlobChunk)
  Total   = 34 bytes
```

**For KUs**: Keep raw `[u8; 32]` — zero migration cost, zero overhead. The `CidRef` opcode (0x12) implicitly means "BLAKE3 hash of a KU."

**For interop**: Provide a bijective mapping function:

```rust
fn to_cidv1(ob_cid: &ObCid) -> CidV1 {
    CidV1 {
        version: 1,
        codec: match ob_cid.cid_type {
            KU => 0x55,          // raw
            BlobManifest => 0x71, // dag-cbor
            BlobChunk => 0x55,    // raw
        },
        multihash: Multihash::new(BLAKE3_CODE, &ob_cid.hash),
    }
}
```

### Merkle DAG Structure

For a single blob:

```
BlobManifest (root)
├── metadata: { size, mime_type, chunk_count, chunking_strategy }
├── chunks: [
│     { index: 0, cid: BLAKE3(chunk_0), size: 262144 },
│     { index: 1, cid: BLAKE3(chunk_1), size: 262144 },
│     ...
│     { index: n, cid: BLAKE3(chunk_n), size: remainder }
│   ]
└── root_hash: BLAKE3(concat(chunk_cids))
```

**Binary tree vs flat list**: IPFS uses a Merkle DAG (tree of nodes) for files, where each node has up to 174 links. For OneBrain's use case (relatively simple file storage, not a filesystem), a **flat chunk list** in the manifest is sufficient for v1:

- Simpler implementation
- O(1) random access: `chunk_index = byte_offset / chunk_size`
- No recursive DAG traversal needed
- Manifest size for 1GB file: ~400 entries × 36B ≈ 14.4KB

A tree structure becomes beneficial only when files exceed ~10GB (>40,000 chunks) or when partial-tree verification is needed. This can be added in v2.

---

## 5. Q4: Blob Replication

### The Scale Problem

KU replication at R=7 is cheap: 7 × 172B = 1.2KB total network storage per KU. But blob replication at R=7 is expensive: 7 × 100MB = 700MB total network storage per blob. For video: 7 × 1GB = 7GB.

### Replication Strategy Comparison

| Strategy | Storage Overhead | Availability | Recovery Speed | Complexity |
|----------|-----------------|-------------|---------------|------------|
| Full replication R=7 | 7× | Excellent | Instant | Low |
| Full replication R=3 | 3× | Good | Instant | Low |
| Reed-Solomon (10,4) | 1.4× | Good | Moderate | High |
| Reed-Solomon (10,6) | 1.6× | Very Good | Moderate | High |
| Hybrid (R=2 + RS) | ~2.4× | Very Good | Mixed | Medium |

### Erasure Coding Deep Dive

**Reed-Solomon (k, m)**: Original file split into k data shards. m parity shards computed. Any k of (k+m) shards reconstruct the original.

Example — RS(10, 4) for a 100MB file:
- 10 data chunks × 10MB each = 100MB original
- 4 parity chunks × 10MB each = 40MB parity
- Total storage: 140MB (1.4× overhead vs 700MB for R=7)
- Tolerates any 4 simultaneous node failures

**Practical considerations:**
- Encoding speed: RS encoding at ~2 GB/s with SIMD (Intel ISA-L); 100MB file encoded in ~50ms
- Decoding speed: Similar to encoding
- Minimum shard count: Needs at least k nodes online. If k=10, need 10 nodes — fine for Tier 1–3 but problematic for sparse Tier 5–7 neighborhoods

### What Peers Do

| System | Strategy | Details |
|--------|---------|---------|
| **IPFS** | On-demand (Bitswap) | No proactive replication; data lives as long as at least one node pins it |
| **Filecoin** | Full replication | Sealed sectors; miners prove storage via PoRep/PoSt |
| **Swarm** | Erasure coding + stamps | RS coding with configurable protection levels (Medium/Strong/Insane/Paranoid) |
| **Storj** | RS(29, 80) | 80 total pieces, need any 29 to reconstruct; 2.7× overhead |
| **Sia** | RS(10, 30) | 30 total pieces, need any 10; 3× overhead |

### ★ Recommendation: Tier-Based Hybrid Replication

```
Blob Replication Strategy (by blob popularity / tier):

HOT blobs (demand_w > 3.0):
  → Full replication R=3 across Tier 1–3 nodes
  → Additionally: RS(10, 4) across Tier 3–5 for durability
  → Total overhead: ~3× + 0.4× ≈ 3.4×

WARM blobs (demand_w 0.5–3.0):
  → RS(10, 4) across Tier 2–5 nodes
  → Total overhead: 1.4×

COLD blobs (demand_w < 0.5):
  → RS(10, 4) across Tier 3–5 nodes
  → Promoted to WARM on access
  → Total overhead: 1.4×

Manifest (tiny, critical):
  → Full replication R=7 (same as KUs)
```

**Key insight**: The `demand_w` factor already exists in the storage reward formula. Extend it to control replication strategy for blobs.

---

## 6. Q5: Storage Economics for Blobs

### Current Storage Reward (KUs only)

From [04_STORAGE_REWARD.md](../../specs/obt/04_STORAGE_REWARD.md):

```
storage_reward = STORAGE_BASE_RATE × size_w × rarity_w × demand_w × duration_f × trust_f

size_w = clamp(wire_bytes / 1024, 0.1, 10.0)  // KU range: 0.015–0.168 KB → always 0.1
```

The `size_w` factor is designed for KUs (max 172B). At `wire_bytes/1024`, even the largest KU has `size_w = 0.168`. **This formula completely breaks for blobs** — a 100MB blob would have `size_w = 100,000`, far exceeding the max clamp of 10.0.

### Proposed Blob Storage Economics

#### 6.1 Separate Blob Reward Pool

```
E(epoch) × STREAM_WEIGHTS[3]  →  R4 budget
                                    ├── 80% → KU storage rewards (existing)
                                    └── 20% → Blob storage rewards (NEW)
```

This prevents blobs from cannibalizing KU storage rewards.

#### 6.2 Blob Size Weight (Logarithmic)

```rust
/// Blob size weight uses logarithmic scaling to prevent
/// massive blobs from dominating the reward pool.
fn blob_size_weight(blob_bytes: u64) -> f64 {
    let mb = blob_bytes as f64 / (1024.0 * 1024.0);
    let w = (1.0 + mb).ln();  // ln(1 + MB)
    w.clamp(BLOB_SIZE_WEIGHT_MIN, BLOB_SIZE_WEIGHT_MAX)
}

const BLOB_SIZE_WEIGHT_MIN: f64 = 0.1;   // ~100KB blob
const BLOB_SIZE_WEIGHT_MAX: f64 = 7.0;   // ~1096MB blob (ln(1+1096) ≈ 7.0)
```

| Blob Size | `ln(1 + MB)` | Clamped |
|-----------|-------------|---------|
| 100 KB | 0.0001 | 0.1 (min) |
| 1 MB | 0.69 | 0.69 |
| 10 MB | 2.40 | 2.40 |
| 100 MB | 4.62 | 4.62 |
| 1 GB | 6.93 | 6.93 |
| 2 GB | 7.60 | 7.0 (max) |

#### 6.3 Blob Size Caps

| Resource | Cap | Rationale |
|----------|-----|-----------|
| Max single blob | 2 GB | Larger files should be split by application |
| Max blob storage per node (Tier 1) | 500 GB | Server-class infrastructure |
| Max blob storage per node (Tier 3) | 50 GB | Desktop-class |
| Max blob storage per node (Tier 5) | 5 GB | Mobile device |
| Max blob storage per node (Tier 7) | 0 | Ephemeral nodes don't store blobs |

#### 6.4 Bandwidth Reward for Blob Serving

KUs are so small that bandwidth is negligible. Blobs are different — serving a 100MB video costs real bandwidth.

```rust
/// Bandwidth reward per chunk served.
/// Paid per chunk transfer verified by the receiver.
fn bandwidth_reward(chunks_served: u64, trust_f: f64) -> f64 {
    BANDWIDTH_BASE_RATE * chunks_served as f64 * trust_f
}

const BANDWIDTH_BASE_RATE: f64 = 0.0001; // OBT per chunk served (256KB)
```

**Verification**: The receiver signs a `TransferReceipt`:

```rust
struct TransferReceipt {
    chunk_cid: [u8; 32],
    server_node_id: NodeId,
    client_node_id: NodeId,
    timestamp: u64,
    client_signature: Ed25519Signature,
}
```

This prevents bandwidth reward gaming — you can't claim rewards without a signed receipt from the actual requester.

#### 6.5 Storage Deposit (Anti-Spam)

To prevent blob spam (uploading terabytes of junk to earn storage rewards), uploaders must burn a small OBT deposit:

```
upload_cost = BLOB_UPLOAD_RATE × ceil(blob_bytes / (1024 * 1024))

BLOB_UPLOAD_RATE = 0.01 OBT/MB
```

A 100MB upload costs 1 OBT. This creates an economic barrier against spam while remaining affordable for legitimate use. The burned OBT is redistributed to the storage reward pool.

---

## 7. Q6: Streaming & Range Requests

### Requirements

| Use Case | Access Pattern | Requirement |
|----------|---------------|-------------|
| Video playback | Sequential + seek | Ordered chunk delivery, random seek |
| Audio streaming | Sequential | Ordered, low latency |
| PDF viewing | Random access | Arbitrary page access |
| Image display | Full download | All chunks, any order |

### Protocol Design

#### 7.1 Chunk-Level Range Mapping

With fixed 256KB chunks, mapping a byte range to chunks is trivial:

```rust
fn byte_range_to_chunks(start: u64, end: u64, chunk_size: u64) -> Range<u64> {
    let first_chunk = start / chunk_size;
    let last_chunk = end / chunk_size;
    first_chunk..(last_chunk + 1)
}

// Example: bytes 1,000,000–2,000,000 of a file with 256KB chunks:
// first_chunk = 1000000 / 262144 = 3
// last_chunk  = 2000000 / 262144 = 7
// → Request chunks 3, 4, 5, 6, 7
```

#### 7.2 BlobFetch Protocol (OBP Layer Extension)

Extend the OneBrain Protocol with new message types:

```rust
/// Request a range of chunks from a blob
struct BlobRangeRequest {
    msg_type: 0x80,           // New OBP message type
    blob_cid: [u8; 32],       // Root blob CID
    chunk_start: u32,         // First chunk index
    chunk_count: u16,         // Number of chunks requested
    priority: u8,             // 0=background, 1=normal, 2=streaming
}

/// Response: single chunk delivery
struct BlobChunkResponse {
    msg_type: 0x81,
    blob_cid: [u8; 32],
    chunk_index: u32,
    chunk_cid: [u8; 32],      // For verification
    data: Vec<u8>,            // Chunk bytes (max 256KB)
}

/// Request blob manifest
struct BlobManifestRequest {
    msg_type: 0x82,
    blob_cid: [u8; 32],
}
```

#### 7.3 Streaming Flow

```
Client                          DHT                         Storage Nodes
  │                              │                              │
  ├─ BlobManifestRequest ───────►│                              │
  │◄─ BlobManifest (chunk list)──┤                              │
  │                              │                              │
  │  [Parse manifest, determine needed chunks]                  │
  │                              │                              │
  ├─ BlobRangeRequest(0..4) ────►│── FindProviders(chunk_0) ──►│
  │                              │◄── Provider list ───────────┤
  │◄── BlobChunkResponse(0) ─────┼──────────────────────────────┤
  │◄── BlobChunkResponse(1) ─────┼──────────────────────────────┤
  │  [Begin playback at chunk 0]  │                              │
  │◄── BlobChunkResponse(2) ─────┼──────────────────────────────┤
  │                              │                              │
  │  [User seeks to 50%]         │                              │
  │                              │                              │
  ├─ BlobRangeRequest(200..204)─►│                              │
  │◄── BlobChunkResponse(200) ───┼──────────────────────────────┤
  │  [Resume playback]           │                              │
```

#### 7.4 Adaptive Prefetching

For streaming, the client prefetches chunks ahead of the playback position:

```rust
struct StreamState {
    current_chunk: u32,
    buffer_chunks: u32,      // Currently buffered ahead
    prefetch_window: u32,    // Adaptive: 4–16 chunks (1–4 MB)
    bandwidth_estimate: f64, // bytes/sec
}

impl StreamState {
    fn should_prefetch(&self) -> bool {
        self.buffer_chunks < self.prefetch_window / 2
    }
    
    fn adapt_window(&mut self, delivery_time_ms: u64) {
        // Increase window if network is fast, decrease if slow
        let rate = 262144.0 / (delivery_time_ms as f64 / 1000.0);
        if rate > self.bandwidth_estimate * 1.5 {
            self.prefetch_window = (self.prefetch_window + 2).min(16);
        } else if rate < self.bandwidth_estimate * 0.5 {
            self.prefetch_window = (self.prefetch_window - 2).max(4);
        }
        self.bandwidth_estimate = rate;
    }
}
```

#### 7.5 Comparison with Existing Protocols

| Feature | IPFS Bitswap | BitTorrent | OneBrain BlobFetch |
|---------|-------------|-----------|-------------------|
| Chunk discovery | DHT + local want-list | Tracker + DHT | DHT + pheromone routing |
| Range requests | ❌ (full DAG traversal) | ❌ (piece-based) | ✅ Native |
| Streaming priority | ❌ | ❌ | ✅ Priority field |
| Parallel download | ✅ Multi-peer | ✅ Swarm | ✅ Multi-peer |
| Verification | Per-block CID | Per-piece hash | Per-chunk BLAKE3 |

---

## 8. Q7: Content Type & Metadata

### What Metadata to Store

| Field | Type | Example | Size |
|-------|------|---------|------|
| MIME type | u16 (enum) | `image/jpeg = 0x0001` | 2B |
| Total size | u64 | 104857600 (100MB) | 8B |
| Width | u16 | 1920 | 2B (images/video) |
| Height | u16 | 1080 | 2B (images/video) |
| Duration | u32 | 7200 (seconds) | 4B (audio/video) |
| Bitrate | u32 | 128000 (bps) | 4B (audio/video) |
| Thumbnail CID | [u8; 32] | BLAKE3 hash | 32B (optional) |
| Codec | u16 (enum) | `H.264 = 0x0001` | 2B (audio/video) |

### Where to Store Metadata

Three options:

| Location | Pros | Cons |
|----------|------|------|
| **BlobManifest header** | Co-located with chunk info; single fetch | Duplicated if multiple KUs reference same blob |
| **Referring KU's Epigenetics** | Keeps KU layer semantically rich | Epigenetics is runtime-only, not content-addressed |
| **Dedicated metadata KU** | Content-addressed; reusable | Extra indirection; one more KU per blob |

### ★ Recommendation: BlobManifest Header (Primary) + KU Epigenetics (Cache)

```rust
/// BlobManifest — the "inode" of a blob.
/// Content-addressed: CID = BLAKE3(serialize(BlobManifest))
struct BlobManifest {
    version: u8,                    // = 1
    chunking_strategy: u8,         // 0 = Fixed256K
    mime_type: u16,                // Enumerated MIME type
    total_size: u64,               // Total blob size in bytes
    chunk_count: u32,              // Number of chunks
    
    // Media-specific (optional, encoded as a tagged union)
    media_meta: Option<MediaMeta>,
    
    // Chunk table
    chunks: Vec<ChunkEntry>,
}

enum MediaMeta {
    Image { width: u16, height: u16 },
    Video { width: u16, height: u16, duration_s: u32, codec: u16, bitrate: u32 },
    Audio { duration_s: u32, codec: u16, bitrate: u32, channels: u8 },
    Document { page_count: u16 },
}

struct ChunkEntry {
    cid: [u8; 32],    // BLAKE3 hash of chunk data
    size: u32,         // Actual chunk size (last chunk may be smaller)
}
```

### Thumbnail Generation

**Client-side**, for multiple reasons:
- No server in a P2P network — "server-side" is undefined
- Client has the decoded media data during upload
- Thumbnail itself is a small blob (typically 5–20KB JPEG), stored as a separate blob with its own CID
- The thumbnail CID is stored in `BlobManifest.media_meta` or in the KU's `MediaRef`

```
Upload Flow:
1. Client selects file (e.g., photo.jpg, 5MB)
2. Client generates thumbnail (200×200, 10KB)
3. Upload thumbnail → chunk (1 chunk) → BlobManifest_thumb → CID_thumb
4. Upload photo → chunks → BlobManifest_photo → CID_photo
5. Create KU with MediaRef(system=0x01, id=CID_photo)
6. Store CID_thumb in BlobManifest_photo.media_meta.thumbnail_cid
```

---

## 9. Comparative Analysis

### Full System Comparison

| Dimension | IPFS | Filecoin | Swarm | Storj | **OneBrain (Proposed)** |
|-----------|------|---------|-------|-------|------------------------|
| **Chunking** | 256KB fixed (default) | Provider-defined sectors | 4KB fixed | Varies (RS shards) | **256KB fixed (v1)** |
| **CID format** | CIDv1 (multihash) | CIDv1 | Swarm hash | Custom | **OB-CID (34B)** |
| **Replication** | On-demand (Bitswap) | Full replicas (sealed) | RS + stamps | RS(29,80) | **Tier-based hybrid** |
| **Persistence** | ❌ Voluntary pinning | ✅ Contract-based | ✅ Stamp-funded | ✅ Contract-based | **✅ PoS-KU extended** |
| **Streaming** | ❌ (gateway only) | ❌ (retrieval market) | ⚠️ Limited | ❌ | **✅ BlobFetch protocol** |
| **Economics** | None (external) | FIL token | BZZ stamps | STORJ token | **OBT (extended)** |
| **Semantic layer** | ❌ Opaque bytes | ❌ Opaque bytes | ❌ Opaque bytes | ❌ Opaque bytes | **✅ KU-linked** |
| **Max file size** | Unlimited | 32 GB sector | 4 GB | 1 TB | **2 GB (v1)** |

### OneBrain's Unique Advantage

Every other system treats stored data as **opaque bytes**. OneBrain's blob storage is semantically linked to Knowledge Units — a blob isn't just "some file," it's evidence for a scientific fact (Gene::Fact), a step illustration in a procedure (Gene::Procedure), or a recording of an experience (Gene::Experience). This semantic linkage enables:

- **Knowledge-aware caching**: Blobs referenced by high-trust, high-demand KUs are prioritized for replication
- **Semantic search over media**: KU metadata makes blobs discoverable via KQL queries
- **Trust propagation**: A blob's trustworthiness is inherited from the KUs that reference it
- **Garbage collection**: Blobs with no KU references can be safely pruned

---

## 10. Recommendation Summary

| Question | Recommendation | Rationale |
|----------|---------------|-----------|
| **Q1: Architecture** | KU → CID link (Option A) | Preserves KU compactness; `MediaRef` opcode already exists |
| **Q2: Chunking** | Fixed 256KB (v1), FastCDC optional (v2) | Simple, interop-friendly, efficient for OneBrain's use case |
| **Q3: CID format** | OB-CID [34B] for blobs; raw BLAKE3 [32B] for KUs | Self-describing without full CIDv1 complexity; bidirectional bridge |
| **Q4: Replication** | Tier-based hybrid: R=3 hot, RS(10,4) warm/cold | Balances availability vs storage cost |
| **Q5: Economics** | Separate blob reward pool (20%); log-scale size_w; upload deposit | Prevents blob spam; rewards proportional to actual cost |
| **Q6: Streaming** | BlobFetch protocol with range requests + adaptive prefetch | Native streaming support; no gateway dependency |
| **Q7: Metadata** | BlobManifest header; client-side thumbnails | Co-located with chunk data; no server needed |

### Implementation Priority

```
Phase 1 (v0.1) — Foundation:
  ├── BlobManifest struct + serialization (CBOR)
  ├── Fixed 256KB chunking + BLAKE3 per chunk
  ├── BlobStore (redb-backed, separate DB file)
  ├── MediaRef system=0x01 integration
  └── Basic upload/download (single peer)

Phase 2 (v0.2) — P2P:
  ├── BlobFetch protocol messages (0x80–0x82)
  ├── DHT provider records for blob chunks
  ├── Multi-peer parallel download
  └── Blob replication (R=3 for hot, RS for cold)

Phase 3 (v0.3) — Economics & Streaming:
  ├── Blob storage reward integration with OBT
  ├── Upload deposit mechanism
  ├── Bandwidth rewards with TransferReceipts
  ├── Streaming with adaptive prefetch
  └── Thumbnail generation pipeline

Phase 4 (v0.4) — Advanced:
  ├── FastCDC optional chunking
  ├── CIDv1 interop bridge (IPFS gateway)
  ├── Erasure coding (RS) implementation
  └── Tier-based replication policy engine
```

---

## References

1. Benet, J. (2014). "IPFS - Content Addressed, Versioned, P2P File System." arXiv:1407.3561
2. Protocol Labs. "CID (Content Identifier) Specification." https://github.com/multiformats/cid
3. Protocol Labs. "Multihash Specification." https://github.com/multiformats/multihash
4. Xia, W. et al. (2016). "FastCDC: A Fast and Efficient Content-Defined Chunking Approach for Data Deduplication." USENIX ATC '16
5. Trón, V. (2020). "Swarm: The Book of Swarm." Ethereum Foundation
6. Wilkinson, S. et al. (2014). "Storj: A Peer-to-Peer Cloud Storage Network." Storj Labs
7. Vorick, D. & Champine, L. (2014). "Sia: Simple Decentralized Storage." Nebulous Inc.
8. Protocol Labs. "Filecoin: A Decentralized Storage Network." https://filecoin.io/filecoin.pdf
9. Rabin, M.O. (1981). "Fingerprinting by Random Polynomials." Center for Research in Computing Technology, Harvard University
10. Reed, I.S. & Solomon, G. (1960). "Polynomial Codes over Certain Finite Fields." SIAM Journal on Applied Mathematics
11. O'Connor, J. et al. (2021). "BLAKE3: One Function, Fast Everywhere." https://github.com/BLAKE3-team/BLAKE3-specs
