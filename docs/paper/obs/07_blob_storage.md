# Chapter 7: Media and Blob Storage

> *"The medium is the message."*
> — Marshall McLuhan, *Understanding Media* (1964)

---

The preceding chapters addressed the storage of Knowledge Units — ultra-small semantic objects of 16–172 bytes. However, human knowledge frequently includes rich media: diagrams, photographs, audio recordings, video demonstrations, and datasets that can range from kilobytes to gigabytes. This chapter presents the **blob storage architecture** of OBS — a design that preserves the semantic purity of KUs while enabling efficient storage and retrieval of arbitrarily large binary objects.

---

## §7.1 Architecture: Separation of Concerns

We adopt a strict architectural separation: **KUs remain pure semantic units** (≤172 bytes of Core DNA wire format), while binary large objects (blobs) reside in a dedicated **Blob Store**. The connection between a KU and its associated media is established through the `MediaRef` opcode (`0x1B`), which encodes a typed reference to an external blob [1].

```
MediaRef instruction: [0x1B] [system:u8] [len:u8] [ob_cid:34B]  = 37 bytes total
```

The `system` byte identifies the storage backend:
- `0x01` — OneBrain Blob Store (native, described in this chapter)
- `0x02` — IPFS CIDv1 reference (external bridge, §7.7)
- `0x03` — Arweave transaction ID (archival bridge, §7.7)

This design preserves three invariants:

1. **KU size invariant.** No KU exceeds the 172-byte wire format ceiling, regardless of the size of associated media. The `MediaRef` instruction adds only 37 bytes to the KU (1 opcode + 1 system + 1 length + 34 OB-CID). A single KU may contain up to **10 `MediaRef` instructions** (`BLOB_MAX_PER_KU = 10`).

2. **CID stability.** The KU's CID is computed from its Core DNA wire bytes, which include only the blob reference — not the blob content. Modifying a blob does not alter the referring KU's CID.

3. **Semantic independence.** The KU carries the *meaning* of knowledge; the blob carries the *evidence*. A KU about "the structure of DNA" is semantically complete without the attached X-ray crystallography image, which serves as supporting evidence.

We rejected two alternative architectures:

- **Blob-as-KU:** Encoding blob content directly into a KU type would break the size invariant, overwhelm the DHT with multi-megabyte entries, and conflate storage economics (§8.5) for KUs and blobs.
- **External-only (IPFS):** Delegating all blob storage to IPFS would introduce an external dependency, create CID format mismatches, and preclude OneBrain-specific features (metabolism-aware caching, trust propagation, OBT blob rewards).

---

## §7.2 Chunking Strategy

Large blobs are partitioned into fixed-size **chunks** before storage and replication.

### §7.2.1 Phase 1: Fixed 256 KB Chunks

We adopt a fixed chunk size of 256 KB (262,144 bytes) for the initial implementation. This aligns with IPFS's default block size [2] and provides a good balance between chunk overhead and deduplication granularity:

| File Size | Chunks | Manifest Overhead |
|-----------|--------|-------------------|
| 1 MB | 4 | 136 B |
| 10 MB | 40 | 1,360 B |
| 100 MB | 400 | 13,600 B |
| 1 GB | 4,000 | 136,000 B |

Each chunk is content-addressed independently: $\text{ChunkCID} = \text{BLAKE3}(\text{chunk\_bytes})$. BLAKE3 hashes a 256 KB chunk in approximately **65 µs** on modern hardware [3], enabling real-time chunking at ingestion rates exceeding 3 GB/s.

### §7.2.2 Phase 2: Content-Defined Chunking (Future)

For workloads requiring cross-file deduplication (e.g., versioned datasets), we plan to adopt **FastCDC** (Fast Content-Defined Chunking) [4] as an optional chunking mode. FastCDC uses a gear-hash rolling function to produce variable-size chunks with expected size 256 KB, achieving deduplication rates of 30–60% on typical datasets while maintaining computational overhead under 2× compared to fixed chunking.

---

## §7.3 OB-CID: Extended Content Identifier Format

While KUs use raw `[u8; 32]` BLAKE3 digests as their content identifiers, blob storage requires a typed CID format that encodes the media type alongside the content hash.

We introduce **OB-CID** (OneBrain Content Identifier), now implemented as the `BlobCid` type in `ku-core/src/blob_store.rs`:

$$\text{OB-CID} = [\text{version}: \text{u8}] \parallel [\text{type}: \text{u8}] \parallel [\text{blake3}: 32\text{B}] = 34 \text{ bytes}$$

The `BlobCid` struct wraps a fixed `[u8; 34]` array. Construction is performed by `BlobCid::from_content(blob_type, data)`, which computes `BLAKE3(data)` and prepends the version and type bytes. Hex serialisation (`to_hex()` / `from_hex()`) is supported for JSON interoperability.

**Table 1.** OB-CID type byte values (`BlobType` enum).

| Type Code | Name | Description | Detection |
|-----------|------|-------------|----------|
| `0x00` | `Raw` | Unclassified binary data | Default fallback |
| `0x01` | `Image` | Image files (JPEG, PNG, WebP, GIF, BMP, SVG, ICO, TIFF) | Magic bytes `FF D8` (JPEG), `89 50 4E 47` (PNG), `RIFF...WEBP`, `GIF8` |
| `0x02` | `Video` | Video files (MP4, WebM, MKV, AVI, MOV, WMV, FLV) | Magic bytes `ftyp` at offset 4 |
| `0x03` | `Audio` | Audio files (MP3, OGG, FLAC, WAV, M4A, AAC, WMA) | Magic bytes `ID3`, `OggS`, `fLaC`, `RIFF...WAVE` |
| `0x04` | `Document` | Document files (PDF, DOCX, XLSX, PPTX, TXT, MD, CSV, JSON, XML, HTML, RTF) | Magic bytes `%PDF`, `PK\x03\x04` (ZIP/Office) |

**Type detection.** `BlobType::detect(extension, magic_bytes)` uses a two-stage strategy: magic byte signatures take priority (eliminating extension spoofing), falling back to file extension matching via `BlobType::from_extension()`. This dual-detection approach is implemented in approximately 50 lines of Rust with explicit match arms for each supported format.

### §7.3.1 CIDv1 Interoperability

For external interoperability with IPFS and other content-addressed systems, we provide bidirectional mapping to the CIDv1 format [5]:

$$\text{CIDv1} = [0\text{x}01] \parallel [0\text{x}55] \parallel [0\text{x}1\text{e}] \parallel [0\text{x}20] \parallel [\text{blake3}: 32\text{B}] = 36 \text{ bytes}$$

where `0x55` is the `raw` multicodec, `0x1e` is the BLAKE3 multihash code, and `0x20` indicates a 32-byte digest length. The `KuCid` wrapper provides `to_cidv1()` and `from_cidv1()` conversion methods for external API boundaries.

### §7.3.2 Local Blob Storage Implementation

The Blob Store is implemented as a separate redb database file (`.blob.redb`), following the same architectural pattern established by `GraphStorage` (§6). The implementation is split across two modules:

- **`ku-core/src/blob_store.rs`** — Core types: `BlobCid`, `BlobType`, `BlobMeta`, and system constants.
- **`ku-kql/src/blob_storage.rs`** — Persistence layer: `BlobStorage` struct with two redb tables.

**Table 1a.** Blob Store redb tables.

| Table | Key Format | Key Size | Value Format | Value Size | Purpose |
|-------|-----------|:--------:|-------------|:----------:|---------|
| `blob_meta` | OB-CID `[u8; 34]` | 34 B | JSON `BlobMeta` | variable | Blob metadata, type, size, chunk count, references |
| `blob_chunks` | `ob_cid(34B) ∥ index(4B BE)` | 38 B | Raw chunk bytes | ≤256 KB | Fixed-size data chunks |

The `BlobMeta` structure tracks all metadata for a stored blob:

```rust
pub struct BlobMeta {
    pub blob_cid_hex: String,       // 68-char hex of 34-byte OB-CID
    pub blob_type: String,          // "Image", "Video", "Audio", "Document", "Raw"
    pub size_bytes: u64,            // Original file size
    pub chunk_count: u32,           // Number of 256 KB chunks
    pub created_at: u64,            // Unix timestamp
    pub original_filename: Option<String>,
    pub referencing_kus: Vec<String>, // CID hex strings of KUs referencing this blob
    pub pinned: bool,               // If true, exempt from garbage collection
}
```

**Ingestion pipeline.** The `store_file(path)` method implements the complete ingestion flow:

1. Read file bytes and verify size ≤ `BLOB_MAX_SIZE` (100 MB)
2. Detect `BlobType` from extension and magic bytes
3. Compute `BlobCid::from_content(blob_type, &data)` — BLAKE3 hash with type prefix
4. Check for deduplication: if `blob_meta` already contains this OB-CID, return existing metadata (automatic content-addressed deduplication)
5. Chunk data into ≤256 KB segments and write to `blob_chunks` table
6. Write `BlobMeta` JSON to `blob_meta` table

**Reference tracking.** The `referencing_kus` field in `BlobMeta` maintains a list of KU CID hex strings that reference this blob via `MediaRef` instructions. Methods `add_ku_reference()` and `remove_ku_reference()` maintain this list. A blob with zero references and `pinned = false` is eligible for garbage collection.

**Garbage collection.** The `collect_garbage()` method identifies orphaned blobs — those with empty `referencing_kus` and `pinned = false` — and deletes both their metadata and chunk data. This pin-based GC model prevents premature deletion of blobs that are in transit or awaiting KU creation.

**Device-adaptive quotas.** Default storage quotas scale with device capabilities:

| Device Class | Default Quota | Rationale |
|-------------|:------------:|-----------|
| Server | 200 GB | Dedicated storage infrastructure |
| Desktop | 50 GB | Generous local disk |
| Laptop | 20 GB | Balanced portability |
| Mobile | 10 GB | Constrained storage |
| IoT | 2 GB | Minimal footprint |

**System constants.**

| Constant | Value | Description |
|----------|:-----:|-------------|
| `BLOB_CHUNK_SIZE` | 256 KB (262,144 B) | Fixed chunk size, IPFS-compatible |
| `BLOB_MAX_SIZE` | 100 MB (104,857,600 B) | Maximum single blob size |
| `BLOB_MAX_PER_KU` | 10 | Maximum MediaRef instructions per KU |
| `BLOB_REPLICATION_HOT` | 3 | Full replicas for hot-demand blobs |

**Schema versioning.** The Blob Store registers its own independent migration chain via `blob_store_registry()` with schema name `"blob_store"` at version 1. This follows the same `ensure_schema()` framework described in §4, enabling independent schema evolution without affecting KU or graph storage.

---

## §7.4 Blob Replication Strategy

Replicating blobs at $R = 7$ (as with KUs) would be prohibitively expensive for large files: a 100 MB video at $R = 7$ consumes 700 MB of network storage. We therefore adopt a **tier-based hybrid replication strategy** that combines full replication for hot blobs with erasure coding for warm and cold blobs.

> [!IMPORTANT]
> **Implementation status.** Phase 1 implements $R = 3$ full replication for hot blobs (`BLOB_REPLICATION_HOT = 3`). Erasure coding (RS(10,4)) is designed but deferred to Phase 2, pending integration of the `reed-solomon-erasure` crate.

### §7.4.1 Tier-Based Hybrid

**Table 2.** Blob replication strategy by demand tier.

| Demand Tier | Condition | Strategy | Storage Overhead | Status |
|-------------|-----------|----------|-----------------|--------|
| **HOT** | `demand_w > 3.0` | $R = 3$ full | 3.0× | **Implemented** |
| **WARM** | `0.5 ≤ demand_w ≤ 3.0` | RS(10,4) only | 1.4× | Phase 2 |
| **COLD** | `demand_w < 0.5` | RS(10,4) only | 1.4× | Phase 2 |
| **Manifest** | Always | $R = 7$ (same as KU) | 7.0× | **Implemented** |

### §7.4.2 Reed-Solomon Erasure Coding (Phase 2)

We design RS(10,4) erasure coding — 10 data fragments and 4 parity fragments — for warm and cold blobs. Given a 100 MB blob:

- **Data fragments:** 10 × 10 MB = 100 MB
- **Parity fragments:** 4 × 10 MB = 40 MB
- **Total storage:** 140 MB (1.4× overhead)
- **Compared to $R = 7$:** 700 MB (7.0× overhead)

The system can reconstruct the original blob from any 10 of the 14 fragments, tolerating up to 4 simultaneous fragment losses. RS encoding at modern SIMD speeds (Intel ISA-L library) processes approximately **2 GB/s**, encoding a 100 MB blob in approximately **50 ms** [6]. This capability requires the `reed-solomon-erasure` crate and is targeted for Phase 2 deployment.

Blob manifests are replicated at $R = 7$ (identical to KUs) because they are small (typically < 1 KB) and critical for blob retrieval — losing the manifest makes all chunks inaccessible.

---

## §7.5 Blob Economics (Phase 2)

> [!NOTE]
> **Implementation status.** The blob economics model is fully designed but deferred to Phase 2. Phase 1 provides the storage infrastructure without OBT token integration. Upload deposit, bandwidth rewards, and the logarithmic size weight formula will be implemented alongside the OBT token launch.

Blob storage participates in the OBT token economy through a dedicated reward pool and pricing mechanism.

### §7.5.1 Separate Reward Pool

20% of the R4 storage reward budget (§8.5) is allocated to blob storage rewards. This separation prevents large blobs from crowding out KU storage incentives.

### §7.5.2 Logarithmic Size Weight

Blob storage rewards use a logarithmic size weight to prevent linear reward scaling (which would incentivise storing maximally large blobs):

$$w_{\text{size}} = \ln(1 + S_{\text{MB}}) \quad \text{clamped to } [0.1, 7.0]$$

**Table 3.** Blob size weight examples.

| Blob Size | $w_{\text{size}}$ |
|-----------|-------------------|
| 1 MB | 0.69 |
| 10 MB | 2.40 |
| 100 MB | 4.62 |
| 1 GB | 6.93 |

The logarithmic curve ensures diminishing marginal returns: storing a 1 GB blob earns only 10× the reward of a 1 MB blob, not 1,000×.

### §7.5.3 Upload Deposit (Phase 2)

To prevent spam uploads, each blob upload requires a deposit of **0.01 OBT per MB**. This deposit is non-refundable and serves as a Sybil resistance mechanism — uploading 1 GB of random data costs 10 OBT, making spam economically unviable.

### §7.5.4 Bandwidth Rewards (Phase 2)

Nodes serving blob chunks earn a bandwidth reward of **0.0001 OBT per chunk served**, verified by signed `TransferReceipt` messages exchanged between the requester and the server. This incentivises availability and responsiveness, not just storage.

---

## §7.6 Streaming Protocol

Blob retrieval supports both full download and range-based streaming through the **BlobFetch** protocol.

### §7.6.1 Wire Format

Four OBP message types support blob operations (message codes `0x30`–`0x33`):

| Message | Code | Purpose | Status |
|---------|------|---------|--------|
| `MSG_BLOB_STORE` | `0x30` | Store blob data to a peer | **Implemented** |
| `MSG_BLOB_STORE_ACK` | `0x31` | Acknowledge blob storage | **Implemented** |
| `MSG_BLOB_FETCH` | `0x32` | Request blob chunks by OB-CID | **Implemented** |
| `MSG_BLOB_FETCH_RESPONSE` | `0x33` | Deliver requested blob chunks | **Implemented** |

### §7.6.2 Range Requests

Range requests map byte offsets to chunk indices:

$$\text{chunk\_index} = \lfloor \text{byte\_offset} / \text{chunk\_size} \rfloor$$

A request for bytes 500,000–750,000 in a 256 KB-chunked blob translates to chunks 1–2 (zero-indexed). The response includes only the requested chunks, enabling efficient partial retrieval for media seeking.

### §7.6.3 Adaptive Prefetch Window

The streaming protocol maintains an adaptive prefetch window of **4–16 chunks** ahead of the current playback position. The window expands when sequential access is detected and contracts when random access patterns emerge, borrowing from TCP congestion window dynamics.

A priority field distinguishes streaming requests (high priority, low latency) from background download requests (low priority, bandwidth-efficient).

---

## §7.7 External Storage Integration (Phase 2)

> [!NOTE]
> **Implementation status.** External storage bridges are designed but deferred to Phase 2. Phase 1 provides the native OneBrain Blob Store as the sole storage backend (`system = 0x01` in `MediaRef`). The `StorageBridge` trait and IPFS/Arweave integrations require additional crate dependencies (`ipfs-api`, `arweave-rs`) and are targeted for later releases.

While OBS operates as a self-contained storage system, optional bridges to external decentralised storage networks provide additional capabilities.

### §7.7.1 StorageBridge Trait (Phase 2)

We define a uniform integration interface:

```rust
pub trait StorageBridge: Send + Sync {
    fn export(&self, ku: &KuRuntime) -> Result<BridgeId, BridgeError>;
    fn import(&self, id: &BridgeId) -> Result<KuRuntime, BridgeError>;
    fn exists(&self, cid: &KuCid) -> Result<bool, BridgeError>;
    fn external_cid(&self, cid: &KuCid) -> Vec<u8>;
}
```

### §7.7.2 IPFS Bridge (Phase 2)

An `IpfsBridge` implementation would export KUs and blobs to IPFS via the Kubo HTTP API, translating OB-CIDs to CIDv1 format. Primary use case: interoperability with the broader IPFS ecosystem for content discovery and redundant distribution.

**Key decision: IPFS as primary storage was rejected.** Replacing the custom redb + DHT architecture with IPFS would require rewriting the entire networking and storage layer, and libp2p cannot support bio-inspired extensions (pheromone routing, immune system, metabolism-aware caching) [7].

### §7.7.3 Arweave Bridge (Phase 2)

An `ArweaveBridge` would export KUs that have reached the **Established** or **Formally Proven** epistemic status to Arweave's permanent storage layer. Cost analysis:

$$\text{Cost}_{1\text{M KUs}} = 172\text{B} \times 10^6 = 172\text{ MB} \approx \$0.60 \text{ (one-time)}$$

The write-once, pay-once model aligns with the semantics of established knowledge — once formally proven, knowledge should be permanently accessible. The biological analogy is long-term memory consolidation: knowledge transitions from hippocampal (volatile, OBS) to cortical (permanent, Arweave) storage [8].

### §7.7.4 Filecoin: Rejected

Filecoin was evaluated and rejected for OneBrain integration. The minimum 32 GiB sector size represents a **186,000,000×** mismatch with the 172-byte KU. Sealing time of 1.5–3 hours per sector and retrieval time exceeding 1 hour from sealed sectors make Filecoin fundamentally incompatible with OneBrain's real-time knowledge access patterns [9].

---

## §7.8 Summary

This chapter presented the blob storage subsystem of OBS, addressing the fundamental challenge of coupling semantic knowledge units with rich media content. The Phase 1 implementation — now operational — provides the complete local storage pipeline: `MediaRef` opcode references link KUs to blobs (§7.1), fixed 256 KB chunking partitions blobs for storage and replication (§7.2), the 34-byte OB-CID typed identifier format with five media types enables content-addressed deduplication (§7.3), and the `BlobStorage` module persists metadata and chunks in a dedicated `.blob.redb` database with pin-based garbage collection and device-adaptive quotas (§7.3.2). The reference implementation comprises `blob_store.rs` (core types) in `ku-core` and `blob_storage.rs` (persistence) in `ku-kql`, with four OBP message codes (`0x30`–`0x33`) for network-level blob exchange (§7.6).

Phase 2 targets include RS(10,4) erasure coding for storage-efficient replication (§7.4), OBT token economics for blob storage incentives (§7.5), content-defined chunking via FastCDC (§7.2.2), and external storage bridges to IPFS and Arweave (§7.7).

The next chapter examines how OBS integrates with the five preceding OneBrain pillars — Knowledge Unit, Network Protocol, Consensus, Token Economics, and Knowledge Graph.

---

## References

[1] OneBrain Project Contributors, "Knowledge Unit: A Bio-Inspired Knowledge Representation with Core DNA Encoding," *OneBrain Technical Report*, 2026.

[2] J. Benet, "IPFS — Content Addressed, Versioned, P2P File System," *arXiv preprint arXiv:1407.3561*, 2014.

[3] J. O'Connor, J.-P. Aumasson, S. Neves, and Z. Wilcox-O'Hearn, "BLAKE3: One Function, Fast Everywhere," *BLAKE3 Specification*, 2020.

[4] W. Xia, H. Jiang, D. Feng, F. Douglis, P. Shilane, Y. Hua, M. Fu, Y. Zhang, and Y. Zhou, "A Comprehensive Study of the Past, Present, and Future of Data Deduplication," *Proc. IEEE*, vol. 104, no. 9, pp. 1681–1710, 2016.

[5] IPFS, "CID (Content Identifier) Specification," https://github.com/multiformats/cid, 2023.

[6] Intel, "Intel Intelligent Storage Acceleration Library (ISA-L)," https://github.com/intel/isa-l, 2023.

[7] IPFS, "libp2p Specifications," https://github.com/libp2p/specs, 2023.

[8] L. R. Squire, "Memory and Brain Systems: 1969–2009," *Journal of Neuroscience*, vol. 29, no. 41, pp. 12711–12716, 2009.

[9] Protocol Labs, "Filecoin: A Decentralized Storage Network," *Filecoin Whitepaper*, 2017.
