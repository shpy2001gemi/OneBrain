# Research Topic 6: IPFS / Decentralized Storage Integration for OneBrain

> **Date**: 2025-07-06  
> **Status**: Research Complete — Recommendation: **Option C (Hybrid Self-Contained + Optional Bridge)**  
> **Confidence**: High (based on codebase analysis + ecosystem research)

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Current OneBrain Storage Architecture](#2-current-onebrain-storage-architecture)
3. [Q1: IPFS Integration Analysis](#3-q1-ipfs-integration-analysis)
4. [Q2: CID Format — Raw BLAKE3 vs CIDv1/Multihash](#4-q2-cid-format--raw-blake3-vs-cidv1multihash)
5. [Q3: IPLD / DAG-CBOR Migration](#5-q3-ipld--dag-cbor-migration)
6. [Q4: Arweave — Permanent Archival](#6-q4-arweave--permanent-archival)
7. [Q5: Filecoin — Incentivized Storage](#7-q5-filecoin--incentivized-storage)
8. [Q6: Self-Contained vs Federated Architecture](#8-q6-self-contained-vs-federated-architecture)
9. [Recommended Architecture](#9-recommended-architecture)
10. [Implementation Roadmap](#10-implementation-roadmap)
11. [References](#11-references)

---

## 1. Executive Summary

OneBrain is a self-contained, bio-inspired decentralized knowledge network with its own custom S/Kademlia DHT, OBP wire protocol, BLAKE3-based content addressing (`[u8; 32]`), and redb-backed ACID storage. The question of integrating with external decentralized storage networks (IPFS, Arweave, Filecoin) involves fundamental architectural tradeoffs between **interoperability** and **autonomy**.

### Verdict

| Option | Recommendation | Rationale |
|--------|---------------|-----------|
| IPFS as primary storage | ❌ **Reject** | Replaces working architecture, adds libp2p dependency, kills bio-inspired autonomy |
| IPFS as secondary/backup | ⚠️ **Defer** | Useful for blobs only, but KUs are tiny (16–172B); overhead not justified yet |
| IPFS as archival layer | ✅ **Future option** | Cold storage bridge for "Established" KUs; implement when blob support arrives |
| CIDv1/Multihash wrapper | ✅ **Adopt** | 4-byte overhead, full interop, BLAKE3 supported (multihash code `0x1e`) |
| DAG-CBOR migration | ✅ **Adopt** | Already planned in Cargo.toml; `serde_ipld_dagcbor` is mature, zero-overhead for serde users |
| Arweave archival | ✅ **Future option** | "Established" + "Formally Proven" KUs → permanent write-once archive |
| Filecoin storage | ❌ **Reject** | 32GB sector minimum vs 172B KUs — 186 million× mismatch |
| Hybrid Bridge | ✅ **Recommend** | Self-contained core + optional `StorageBridge` trait for external backends |

---

## 2. Current OneBrain Storage Architecture

Understanding the existing system is critical before evaluating external integrations.

### 2.1 Content Addressing

```rust
// ku_runtime.rs — CID computation
pub struct KuRuntime {
    /// Content identity — BLAKE3 hash of `wire_bytes`.
    pub cid: [u8; 32],
    pub dna: CoreDna,          // Layer 1: 16-172 bytes
    pub epi: Epigenetics,      // Layer 2: stored separately
    pub wire_bytes: Vec<u8>,   // Canonical wire format
}

impl KuRuntime {
    pub fn new(dna: CoreDna, wire_bytes: Vec<u8>) -> Self {
        let cid = blake3::hash(&wire_bytes).into();
        // ...
    }
}
```

**Key properties:**
- **CID = `blake3::hash(wire_bytes)`** → raw 32-byte BLAKE3 digest
- **Immutable**: Changing Core DNA → new CID → new KU
- **No multicodec prefix**, no multihash envelope, no base encoding
- **Tiny payloads**: KUs are 16–172 bytes (Core DNA wire format)

### 2.2 Storage Stack

| Component | Technology | Purpose |
|-----------|-----------|---------|
| Primary store | `redb` v2 (Pure Rust, ACID) | CID → KU wire bytes |
| Index tables | `redb` | trust_score index, concept_id index |
| Serialization | `ciborium` (CBOR) | Wire format + epigenetics |
| Network | Custom OBP (TCP) | KuPush, KuPull, DHT ops |
| DHT | Custom S/Kademlia | Peer discovery, CID → peer mapping |
| Incentives | OBT (OneBrain Token) | Storage reward challenges (BLAKE3-based) |

### 2.3 Planned Migration

From `ku-core/Cargo.toml`:
```toml
ciborium = "0.2"     # CBOR serialization — will migrate to serde_ipld_dagcbor
```

This comment signals intent to adopt IPLD-compatible serialization, which is a critical design signal.

---

## 3. Q1: IPFS Integration Analysis

### 3.1 Option A: IPFS as Primary Storage — ❌ REJECT

**What this means**: Replace redb + custom DHT with IPFS (Kubo/Helia) as the underlying storage and routing layer.

**Why it doesn't fit OneBrain:**

| Concern | Impact |
|---------|--------|
| **Dependency on libp2p** | OneBrain uses custom S/Kademlia with bio-inspired extensions (pheromone routing, immune system, metabolism). libp2p's Kademlia is a generic implementation that cannot support these custom behaviors without extensive forking. |
| **CID format mismatch** | IPFS expects CIDv1 with multicodec + multihash; OneBrain uses raw `[u8; 32]`. Every KU operation would need wrapping/unwrapping. |
| **Overhead for tiny data** | IPFS is optimized for files (KB–GB range). KUs at 16–172 bytes would be dominated by IPFS protocol overhead (block headers, DAG nodes, bitswap messages). |
| **Storage reward conflict** | OneBrain's OBT system uses BLAKE3 challenges to verify node storage. IPFS provides/pinning has a completely different model. The two incentive systems would conflict. |
| **Loss of autonomy** | OneBrain's bio-inspired philosophy treats the network as a living organism. Depending on IPFS infrastructure (pinning services, public gateways) contradicts this design principle. |

**Verdict**: This would require rewriting the entire networking and storage layer while losing custom bio-inspired features that differentiate OneBrain.

### 3.2 Option B: IPFS as Secondary/Backup (Blobs Only) — ⚠️ DEFER

**What this means**: Keep redb + custom DHT for KUs, but use IPFS for large binary blobs (images, datasets, documents) that KUs reference via `CID_REF`.

**Analysis:**

- **Not currently needed**: All KUs are 16–172 bytes. There is no blob storage in the current system.
- **Future-relevant**: When OneBrain supports rich media attachments (papers, datasets, images), IPFS becomes a natural fit for content-addressed blob storage.
- **Implementation**: KU `CID_REF` instructions (opcode `0x12`) could reference IPFS CIDs for external blobs.

**Verdict**: Good idea in principle, but premature. Revisit when blob support is designed.

### 3.3 Option C: IPFS as Archival Layer (Cold Storage) — ✅ FUTURE OPTION

**What this means**: Periodically export "Established" or "Formally Proven" KUs to IPFS for redundant, archival-grade storage.

**Why this works:**

1. **Non-invasive**: No changes to the core protocol. An external bridge process reads from redb and publishes to IPFS.
2. **Semantic fit**: "Established" knowledge (epistemic status) is stable enough to warrant archival permanence.
3. **Redundancy**: IPFS's global network provides geographic redundancy beyond what OneBrain's early-stage network can offer.
4. **Verifiable**: Since both systems use content-addressing, integrity is preserved across the bridge.

**Cost estimate**: IPFS pinning services (Pinata, Filebase) charge ~$0.15/GB/month. At 172 bytes/KU, 1 million KUs = ~172 MB → negligible cost.

### 3.4 Option D: No IPFS — ✅ VALID (Current Default)

OneBrain's custom stack is well-designed and functional. IPFS integration is not a prerequisite for the system to work.

---

## 4. Q2: CID Format — Raw BLAKE3 vs CIDv1/Multihash

### 4.1 Current Format

```
OneBrain CID: [u8; 32] = blake3(wire_bytes)
              └── 32 bytes, no metadata, no self-description
```

### 4.2 CIDv1 Format with BLAKE3

BLAKE3 is fully supported in the multihash standard with code `0x1e`:

```
CIDv1 structure:
  <multibase>  <cid-version>  <multicodec>       <multihash>
  (if text)    0x01           0x55 (raw)         <hash-func><digest-size><digest>
                              or 0x71 (dag-cbor)  0x1e       0x20        [32 bytes]

Binary CIDv1 with BLAKE3:
  0x01  0x71  0x1e  0x20  [32 bytes BLAKE3 digest]
  └─┘   └─┘   └─┘   └─┘   └────────────────────┘
  ver  codec  algo  len=32      hash digest
  1B    1B    1B    1B          32 bytes
                              = 36 bytes total
```

### 4.3 Tradeoff Analysis

| Aspect | Raw BLAKE3 `[u8; 32]` | CIDv1 `[u8; 36]` |
|--------|----------------------|-------------------|
| **Size** | 32 bytes | 36 bytes (+12.5%) |
| **Self-describing** | ❌ No — assumes BLAKE3 | ✅ Yes — hash algo + content type embedded |
| **IPFS interop** | ❌ Not compatible | ✅ Native support (Kubo ≥ 0.11) |
| **Filecoin interop** | ❌ | ✅ |
| **Web3 ecosystem** | ❌ | ✅ |
| **Parsing overhead** | None | ~10ns (varint decode) |
| **Future-proofing** | ❌ Locked to BLAKE3 | ✅ Can migrate hash algo |
| **Internal simplicity** | ✅ Maximum | ⚠️ Slight increase |

### 4.4 Recommendation: Adopt CIDv1 with Internal Optimization

**Strategy**: Use CIDv1 as the canonical external format, but optimize internally.

```rust
/// Internal representation — always BLAKE3, skip prefix parsing
pub struct KuCid {
    /// Raw BLAKE3 digest — used for all internal operations (DHT, storage, OBT)
    digest: [u8; 32],
}

impl KuCid {
    /// Compute CID from wire bytes
    pub fn from_wire(wire_bytes: &[u8]) -> Self {
        Self { digest: blake3::hash(wire_bytes).into() }
    }

    /// Export as CIDv1 bytes for external interop (IPFS, Arweave)
    pub fn to_cidv1(&self) -> [u8; 36] {
        let mut cid = [0u8; 36];
        cid[0] = 0x01;       // CID version 1
        cid[1] = 0x55;       // multicodec: raw (or 0x71 for dag-cbor)
        cid[2] = 0x1e;       // multihash: blake3
        cid[3] = 0x20;       // digest size: 32
        cid[4..].copy_from_slice(&self.digest);
        cid
    }

    /// Parse from CIDv1 bytes
    pub fn from_cidv1(bytes: &[u8]) -> Result<Self, CidError> {
        if bytes.len() < 36 || bytes[0] != 0x01 || bytes[2] != 0x1e {
            return Err(CidError::InvalidFormat);
        }
        let mut digest = [0u8; 32];
        digest.copy_from_slice(&bytes[4..36]);
        Ok(Self { digest })
    }
}
```

**Rationale**: This gives OneBrain the best of both worlds — internal operations remain fast with raw 32-byte digests, while external-facing APIs and bridges produce standards-compliant CIDv1 identifiers. The 4-byte overhead is negligible relative to the interoperability benefits.

---

## 5. Q3: IPLD / DAG-CBOR Migration

### 5.1 What is IPLD?

InterPlanetary Linked Data (IPLD) is a data model for content-addressed data that defines:
- **Data Model**: Scalars, maps, lists, links (CIDs), bytes
- **Codecs**: DAG-CBOR (binary), DAG-JSON (text), DAG-PB (legacy IPFS)
- **Schemas**: Optional type definitions for IPLD structures

### 5.2 DAG-CBOR vs Plain CBOR

| Feature | `ciborium` (plain CBOR) | `serde_ipld_dagcbor` (DAG-CBOR) |
|---------|------------------------|--------------------------------|
| **CID links** | ❌ Not supported | ✅ Native `CID` type (CBOR tag 42) |
| **Map key ordering** | ❌ Unspecified | ✅ Deterministic (length-sorted) |
| **Canonical form** | ❌ Multiple valid encodings | ✅ Single canonical encoding |
| **IPFS compatibility** | ❌ | ✅ Direct |
| **serde integration** | ✅ | ✅ Drop-in compatible |
| **Crate maturity** | ✅ Mature (0.2) | ✅ Mature (0.6, actively maintained) |
| **Overhead vs CBOR** | Baseline | ~0% (same wire format, stricter rules) |

### 5.3 Migration Effort

Since `serde_ipld_dagcbor` uses the same serde `Serialize`/`Deserialize` traits as `ciborium`, migration is largely mechanical:

```rust
// Before (ciborium)
let bytes = ciborium::to_vec(&ku_data)?;
let decoded: KuData = ciborium::from_reader(&bytes[..])?;

// After (serde_ipld_dagcbor)
let bytes = serde_ipld_dagcbor::to_vec(&ku_data)?;
let decoded: KuData = serde_ipld_dagcbor::from_slice(&bytes)?;
```

**Key differences to handle:**
1. **CID fields**: `[u8; 32]` should become `ipld_core::cid::Cid` for full IPLD link support
2. **Deterministic encoding**: DAG-CBOR enforces canonical map key ordering — existing data may need re-encoding
3. **No floats**: DAG-CBOR prohibits IEEE 754 floats (NaN ambiguity). OneBrain uses `f64` for trust scores — these must be serialized as fixed-point or kept in epigenetics (separate store)

### 5.4 Recommendation: ✅ ADOPT (Phased)

**Phase 1** (Low effort): Replace `ciborium` with `serde_ipld_dagcbor` for new serialization. Keep existing wire format unchanged (Core DNA is custom binary, not CBOR).

**Phase 2** (Medium effort): Migrate epigenetics storage to DAG-CBOR. Handle float → fixed-point conversion for trust scores.

**Phase 3** (Future): Use IPLD links (`Cid` type) in KU references, enabling native Merkle DAG traversal across the knowledge graph.

### 5.5 Cargo.toml Change

```toml
# Replace:
ciborium = "0.2"     # CBOR serialization — will migrate to serde_ipld_dagcbor

# With:
serde_ipld_dagcbor = "0.6"  # IPLD DAG-CBOR — deterministic, CID-aware CBOR
```

---

## 6. Q4: Arweave — Permanent Archival

### 6.1 How Arweave Works

Arweave is a **write-once, pay-once** permanent storage protocol:
- **Endowment model**: ~95% of the storage fee goes into a reserve that funds miners for 200+ years
- **Cost (2025)**: ~$3,500/TB, meaning 172-byte KUs cost fractions of a cent each
- **Bundling**: Small objects can be aggregated into bundles for efficiency (e.g., using Irys/Bundlr)
- **Immutability**: Data cannot be deleted once written — aligns with "Established" KU semantics

### 6.2 Fit Assessment for OneBrain

| Criterion | Assessment |
|-----------|-----------|
| **Epistemic alignment** | ✅ Excellent — "Established" and "Formally Proven" KUs are meant to be permanent |
| **Cost for KUs** | ✅ Negligible — 1M KUs (~172 MB) ≈ $0.60 one-time |
| **Bundling for small objects** | ✅ Arweave bundling (Irys) aggregates small items into single transactions |
| **No deletion** | ⚠️ Acceptable for "Established" only — RAW/SELF KUs should NOT be archived |
| **External dependency** | ⚠️ Requires AR tokens and Arweave SDK |
| **Bio-inspired analogy** | ✅ Long-term memory consolidation (hippocampus → cortex) |

### 6.3 Archival Strategy

```
KU Lifecycle → Arweave Bridge:

  RAW → SELF → PART → FULL
   │      │      │      │
   ×      ×      ×      └── When EpistemicStatus reaches "Established"
                                  → Archive to Arweave (irreversible)
                                  → Store Arweave TX ID in epigenetics
                                  → KU can be garbage-collected locally
                                    (recoverable from Arweave)
```

### 6.4 Recommendation: ✅ FUTURE OPTION

Implement an `ArweaveBridge` when:
1. OneBrain has KUs reaching "Established" status in production
2. The network is large enough to benefit from permanent off-chain archival
3. Blob/attachment support creates genuine large-data storage needs

---

## 7. Q5: Filecoin — Incentivized Storage

### 7.1 Fundamental Mismatch

| Filecoin Constraint | OneBrain Reality | Mismatch |
|--------------------|--------------------|----------|
| **Minimum sector size**: 32 GiB | **KU size**: 16–172 bytes | 186,000,000× |
| **Sealing time**: 1.5–3 hours | **KU ingestion**: <1ms | Incompatible latency |
| **Deal duration**: 6–18 months | **KU lifecycle**: seconds to years | Over-specified |
| **Payment**: FIL tokens | **Payment**: OBT tokens | Dual-token overhead |
| **Proof-of-Spacetime** | **BLAKE3 storage challenges** | Redundant verification |

### 7.2 Could Filecoin Store KU Bundles?

Theoretically, millions of KUs could be bundled into a 32 GiB sector. But:
- Individual KU retrieval from a sealed sector requires unsealing → 1+ hour latency
- Filecoin is designed for "cold" archival, not active knowledge queries
- The OBT storage reward system already incentivizes local storage more efficiently
- Cost benefit is marginal: Filecoin pricing (~$0.001/GiB/month) doesn't improve on self-hosting for such tiny data

### 7.3 Recommendation: ❌ REJECT

Filecoin's architecture is fundamentally misaligned with OneBrain's data profile. The 32 GiB sector minimum, slow sealing, and retrieval latency make it unsuitable for KU-level operations. Arweave is a strictly better choice for permanent archival.

---

## 8. Q6: Self-Contained vs Federated Architecture

### 8.1 The Bootstrap Problem

OneBrain starts with zero nodes. IPFS has millions. The practical concern is real:

| Phase | OneBrain Nodes | Risk | Mitigation |
|-------|---------------|------|------------|
| **Launch** (0–100) | <100 | Data loss, no redundancy | Seed nodes, founder-operated infrastructure |
| **Growth** (100–10K) | 100–10K | Geographic concentration | OBT incentives for diverse hosting |
| **Scale** (10K+) | 10K+ | Self-sustaining | Network effects kick in |

### 8.2 Why Self-Containment Wins

OneBrain's bio-inspired design makes self-containment not just a philosophy but a **technical requirement**:

1. **Pheromone-based routing**: Query paths are reinforced by usage patterns. This requires tight coupling between routing and storage that IPFS cannot provide.

2. **Immune system (Trojan/Sybil detection)**: OneBrain's immune module scores peers and quarantines bad actors. This requires integration with the DHT layer that external networks don't expose.

3. **Metabolism (resource management)**: The metabolism system governs CPU/memory/bandwidth allocation per KU. External storage has no concept of metabolic cost.

4. **OBT incentives**: Storage rewards use BLAKE3-based challenges. These challenges assume the node holds the KU locally in redb. IPFS-pinned data cannot be challenged in the same way.

5. **Epistemic lifecycle**: KUs traverse RAW → SELF → PART → FULL → Established. This lifecycle is deeply integrated with local storage. External systems can only store snapshots, not participate in the lifecycle.

### 8.3 Where External Bridges Help

Despite self-containment, external bridges serve specific needs:

| Need | Bridge Target | When |
|------|--------------|------|
| **Permanence** | Arweave | KU reaches "Established" |
| **Blob storage** | IPFS | When rich media support is added |
| **Interop** | IPFS gateway | When external apps query OneBrain |
| **Backup** | S3/Arweave | Disaster recovery for seed nodes |

---

## 9. Recommended Architecture

### 9.1 Hybrid Self-Contained + Bridge Pattern

```
┌─────────────────────────────────────────────────────┐
│                  OneBrain Core                       │
│  ┌─────────┐  ┌──────────┐  ┌───────────────────┐  │
│  │ KuStore │  │ Custom   │  │  OBT Storage      │  │
│  │ (redb)  │◄─┤ S/Kadel. │  │  Reward System    │  │
│  │ BLAKE3  │  │ DHT      │  │  (BLAKE3 proofs)  │  │
│  └────┬────┘  └──────────┘  └───────────────────┘  │
│       │                                              │
│  ┌────▼──────────────────────────────────────────┐  │
│  │          StorageBridge Trait                    │  │
│  │  fn export(&self, ku: &KuRuntime) -> BridgeId │  │
│  │  fn import(&self, id: &BridgeId) -> KuRuntime │  │
│  │  fn exists(&self, cid: &KuCid) -> bool        │  │
│  └────┬──────────┬──────────┬────────────────────┘  │
│       │          │          │                        │
└───────┼──────────┼──────────┼────────────────────────┘
        │          │          │
   ┌────▼───┐ ┌───▼────┐ ┌──▼──────┐
   │  IPFS  │ │Arweave │ │   S3    │
   │ Bridge │ │ Bridge │ │ Bridge  │
   │(blobs) │ │(archiv)│ │(backup) │
   └────────┘ └────────┘ └─────────┘
```

### 9.2 StorageBridge Trait Design

```rust
/// Trait for external storage integration.
/// All bridges are OPTIONAL and non-blocking.
/// The core system operates independently.
pub trait StorageBridge: Send + Sync {
    /// Bridge name for logging/metrics
    fn name(&self) -> &str;

    /// Export a KU to external storage. Returns external ID.
    fn export(&self, ku: &KuRuntime) -> Result<BridgeId, BridgeError>;

    /// Import a KU from external storage by its external ID.
    fn import(&self, id: &BridgeId) -> Result<KuRuntime, BridgeError>;

    /// Check if a CID exists in external storage.
    fn exists(&self, cid: &KuCid) -> Result<bool, BridgeError>;

    /// Export CID in external format (e.g., CIDv1 for IPFS)
    fn external_cid(&self, cid: &KuCid) -> Vec<u8>;
}
```

### 9.3 Priority Actions

| Priority | Action | Effort | Impact |
|----------|--------|--------|--------|
| **P0** | Migrate `ciborium` → `serde_ipld_dagcbor` | 1 week | DAG-CBOR compliance, IPLD readiness |
| **P1** | Add `KuCid` wrapper with `to_cidv1()` / `from_cidv1()` | 2 days | CIDv1 interop without internal changes |
| **P2** | Define `StorageBridge` trait | 1 day | Future-proof architecture |
| **P3** | Implement `IpfsBridge` (blobs) | 2 weeks | When blob support is designed |
| **P4** | Implement `ArweaveBridge` (archival) | 2 weeks | When "Established" lifecycle is active |

---

## 10. Implementation Roadmap

### Phase 1: Foundation (No External Dependencies)

```
Timeline: Sprint N

1. Create `KuCid` wrapper struct
   - Internal: [u8; 32] raw BLAKE3
   - External: to_cidv1() / from_cidv1()
   - Replaces bare `[u8; 32]` across codebase

2. Migrate ciborium → serde_ipld_dagcbor
   - Swap dependency in Cargo.toml
   - Fix float serialization (trust scores → fixed-point)
   - Re-encode existing test data
   - Verify wire compatibility
```

### Phase 2: Bridge Infrastructure (No External Services)

```
Timeline: Sprint N+1

1. Define StorageBridge trait
2. Implement NullBridge (no-op, for testing)
3. Add bridge registry to KuStore
4. Add CLI commands: `onebrain export --bridge=<name>`
```

### Phase 3: External Integrations (When Needed)

```
Timeline: Future (driven by feature needs)

1. IpfsBridge — when blob/attachment support is designed
2. ArweaveBridge — when KUs reach "Established" in production
3. S3Bridge — for enterprise backup requirements
```

---

## 11. References

### Ecosystem

| Source | Key Finding |
|--------|------------|
| IPFS 2025 ecosystem | Moving to "post-gateway world" — direct verified retrieval via Helia, Service Worker gateways |
| CIDv1 + BLAKE3 | BLAKE3 multihash code `0x1e` supported since Kubo 0.11; fully compatible with CIDv1 |
| `serde_ipld_dagcbor` | v0.6, actively maintained, drop-in serde-compatible replacement for ciborium |
| Arweave 2025 pricing | ~$3,500/TB one-time; bundling services (Irys) optimize for small objects |
| Filecoin sectors | 32 GiB minimum → 186M× mismatch with KU sizes |
| Custom DHT vs libp2p | libp2p is "rarely the bottleneck" but custom DHTs are justified for novel routing (pheromone, immune) |
| Hybrid P2P patterns | Bridge Pattern decouples core from external storage; common in cross-chain architectures |

### OneBrain Codebase References

| File | Relevance |
|------|-----------|
| `ku-core/Cargo.toml` | `ciborium = "0.2" # will migrate to serde_ipld_dagcbor` — confirms planned migration |
| `ku-core/src/ku_runtime.rs` | `pub cid: [u8; 32]` — raw BLAKE3, no CIDv1 wrapping |
| `ku-core/src/core_dna.rs` | `CID_REF(32 bytes)` instruction — inter-KU references |
| `ku-core/src/obt_storage_reward.rs` | BLAKE3-based storage challenges — requires local redb access |
| `ku-kql/Cargo.toml` | `redb = { version = "2", optional = true }` — feature-gated persistence |

---

> [!TIP]
> **Bottom line**: OneBrain should remain self-contained at its core, adopt CIDv1/DAG-CBOR for standards compliance, and implement optional bridges only when specific features (blobs, archival) demand them. The bio-inspired architecture is a competitive advantage — don't dilute it with unnecessary external dependencies.
