# Research Topic 4: Schema Migration & Versioning for OneBrain Storage

> **Date:** 2026-07-06  
> **Status:** Research Complete  
> **Scope:** ku-core (Core DNA, Epigenetics, ConceptDict), ku-kql (KuStorage, GraphStorage)

---

## Executive Summary

OneBrain currently has **zero migration infrastructure**. All four storage modules — KuStorage (Core DNA wire bytes), Epigenetics (JSON in redb), GraphStorage (fixed 9-byte BondMeta), and PersistentConceptDict (JSON ConceptEntry) — lack schema version tracking, migration code, and backward compatibility guarantees. This is a ticking time bomb: the first format change will silently corrupt or orphan existing data.

This document analyzes industry-standard migration strategies, evaluates them against OneBrain's content-addressed architecture, and proposes a concrete plan that respects the fundamental constraint that **CID = BLAKE3(wire_bytes)** makes wire format migration inherently destructive to references.

---

## 1. Current State Audit

### 1.1 Core DNA v6 Wire Format

From [`core_dna.rs`](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/core_dna.rs#L1-L29):
```text
MAGIC(0x4B) | VER_META(1B) | INSTRUCTION_STREAM | END(0x1E) | CRC-16(2B)
```
- `CORE_DNA_MAGIC = 0x4B` (single byte 'K')
- `CORE_DNA_VERSION = 1` (3 bits in VER_META byte, bits 7-5)
- Version is encoded **inside** the wire bytes → included in BLAKE3 CID hash
- No migration path: decoder rejects unknown versions with `UnsupportedVersion` error

### 1.2 KuStorage (redb)

From [`storage.rs`](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/storage.rs#L17-L43):
- 4 tables: `kus`, `epigenetics`, `index_trust`, `index_concept`
- No `_metadata` or `_schema_version` table
- No version check on `open()`
- `kus` table stores raw wire bytes keyed by 32-byte BLAKE3 CID
- `epigenetics` table stores JSON strings keyed by CID

### 1.3 GraphStorage (redb)

From [`graph_storage.rs`](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/graph_storage.rs#L1-L43):
- 6 index tables: `edges_out`, `edges_in`, `edges_type`, `index_state`, `bond_weight`, `edge_time`
- `edges_out` values are fixed 9-byte `BondMeta` — **no version byte, no extensibility**
- All other tables store empty values (index-only)
- No schema version tracking

### 1.4 BondMeta (9-byte fixed format)

From [`graph_types.rs`](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/graph_types.rs#L20-L69):
```text
[weight:2][creator:1][state:1][decay:1][timestamp:4] = 9 bytes
```
- Hard-coded `from_bytes(&[u8; 9])` with match arms that default unknown values
- No version prefix — adding fields would break all existing data

### 1.5 PersistentConceptDict (JSON in redb)

From [`persistent_concept_dict.rs`](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/persistent_concept_dict.rs#L1-L22):
- 3 tables: `concepts` (name→JSON), `ids` (u64→name), `meta` (next_id counter)
- ConceptEntry serialized as JSON — flexible but no schema version field
- No `#[serde(default)]` for future-proofing (adding a field breaks deserialization of old data)

---

## 2. Schema Versioning Strategy (Q1)

### 2.1 How Other Embedded Databases Handle This

| Database | Mechanism | Notes |
|----------|-----------|-------|
| **SQLite** | `PRAGMA user_version` — integer stored in file header | Set/read atomically; application checks on open, applies sequential migration scripts in a transaction |
| **RocksDB** | Column families + application-level metadata keys | No built-in versioning; common pattern is a `__meta__` column family with `schema_version` key |
| **LMDB** | Named databases (sub-databases) for metadata | Similar to RocksDB — `_meta` sub-DB with version integer |
| **redb** | **Nothing built-in** | redb is intentionally minimal; migration is 100% application responsibility |

### 2.2 Recommended Strategy for OneBrain

**Add a `_schema_meta` table to every redb database file:**

```rust
const TABLE_SCHEMA_META: TableDefinition<&str, &[u8]> = TableDefinition::new("_schema_meta");

// Keys stored in _schema_meta:
// "schema_version" → u32 (little-endian)
// "created_at"     → u64 (unix timestamp)
// "migrated_at"    → u64 (last migration timestamp)
// "app_version"    → utf-8 string (e.g., "0.7.0")
```

**Startup migration runner pattern:**

```rust
fn open_with_migration(path: &Path) -> Result<Database, StorageError> {
    let db = Database::create(path)?;
    let current_version = read_schema_version(&db)?; // 0 if table missing
    
    for migration in MIGRATIONS.iter().filter(|m| m.from >= current_version) {
        let txn = db.begin_write()?;
        (migration.apply)(&txn)?;
        write_schema_version(&txn, migration.to)?;
        txn.commit()?;
    }
    Ok(db)
}
```

**Key design decisions:**
- **One version per redb file**, not per table — simpler, matches SQLite PRAGMA model
- **Sequential, numbered migrations** — `v0→v1`, `v1→v2`, never skip
- **Migrations run in write transactions** — atomic, crash-safe (redb is ACID)
- **Forward-only** — no downgrade support (too complex for embedded DB)

### 2.3 Version Assignment

| Storage Module | Current (implicit) | First Explicit Version |
|---|---|---|
| KuStorage (`kus.redb`) | v0 | v1 (adds `_schema_meta` table) |
| GraphStorage (`kus.graph.redb`) | v0 | v1 (adds `_schema_meta` table) |
| PersistentConceptDict (`.redb`) | v0 | v1 (adds `_schema_meta` table) |

---

## 3. Core DNA Wire Format Migration (Q2)

### 3.1 The CID Dilemma

This is **the** fundamental constraint:

```
CID = BLAKE3(wire_bytes)
```

If you change the wire format (new opcodes, different encoding), the wire bytes change, and therefore the CID changes. This breaks:
- All KU-to-KU bond references (stored as `[u8; 32]` CIDs)
- All graph edges (keyed by source/target CID pairs)
- All external references (peers, indexes, caches)
- Content verification (CID is a cryptographic commitment)

**This is identical to the IPFS problem:** changing the UnixFS encoding or chunking parameters produces different CIDs for the same semantic content. IPFS solved this with standardized construction profiles (IPIP-0499) to ensure CID stability.

### 3.2 Options Analysis

| Strategy | Mechanism | CID Impact | Complexity | Recommendation |
|----------|-----------|------------|------------|----------------|
| **A. Never Migrate Wire Bytes** | Keep v6 bytes forever; new features only in Epigenetics | ✅ CID stable | Low | **✅ RECOMMENDED** |
| **B. Lazy Re-encode** | Decode v6 on read, re-encode as v7 on write | ❌ CID changes on write | Medium | ❌ Breaks references |
| **C. Batch Migration** | Rewrite all KUs with new CIDs, update all references | ❌ CID changes | Very High | ❌ Impractical at scale |
| **D. Dual-Format** | Store both v6 and v7 wire bytes, index by both CIDs | ⚠️ Both CIDs valid | High | ⚠️ Possible but complex |
| **E. CID-Envelope** | CID hashes only a stable subset (e.g., normalized semantic content) | ⚠️ Requires redesign | Very High | ❌ Breaks current model |

### 3.3 Recommended Approach: Immutable Wire Bytes + Semantic Versioning

**Strategy A — Never Migrate Wire Bytes** is the only viable approach for a content-addressed system:

1. **Core DNA wire bytes are immutable once stored** — just like Git blob objects
2. **The version byte in VER_META enables the decoder to select the correct parser:**
   ```rust
   match version {
       1 => decode_v6(bytes),  // Current
       2 => decode_v7(bytes),  // Future
       _ => Err(UnsupportedVersion(version)),
   }
   ```
3. **New features go into Epigenetics (mutable layer) or new opcodes for new KUs:**
   - Existing KUs keep v6 encoding forever
   - New KUs can use v7 encoding with new opcodes
   - Decoder supports all versions simultaneously (Protocol Buffers style)
4. **This follows the Protobuf golden rules:**
   - Never change the meaning of existing opcodes (= field numbers)
   - New opcodes get new numbers
   - Readers ignore unknown opcodes (forward compatibility)
   - Old readers skip new opcodes gracefully

### 3.4 Multi-Version Decoder Design

```rust
pub fn decode_core_dna(bytes: &[u8]) -> Result<CoreDna, KuError> {
    let magic = bytes[0];
    if magic != CORE_DNA_MAGIC {
        return Err(KuError::InvalidMagic);
    }
    
    let ver_meta = bytes[1];
    let version = (ver_meta >> 5) & 0x07;  // 3-bit version
    
    match version {
        1 => decode_v1(bytes),    // Core DNA v6 format
        2 => decode_v2(bytes),    // Future v7 format  
        _ => Err(KuError::UnsupportedVersion(version)),
    }
}
```

**Critical invariant:** Once a KU's wire bytes are stored and its CID computed, those bytes are **never modified**. The CID is a permanent address, like a Git commit hash.

---

## 4. Epigenetics JSON Migration (Q3)

### 4.1 Current Vulnerability

The `Epigenetics` struct uses `serde_json` for serialization. It already has some defensive annotations:

```rust
#[serde(rename = "bn", skip_serializing_if = "Vec::is_empty", default)]
pub bonds: Vec<Bond>,

#[serde(rename = "ep", skip_serializing_if = "Option::is_none", default)]
pub epigenetic: Option<EpigeneticSection>,
```

However, **not all fields have `#[serde(default)]`**. If a new required field is added (e.g., OBKG's `QualifiedBond`), old JSON lacking that field will fail to deserialize.

### 4.2 Strategy: Serde Defaults + Envelope Versioning

**Tier 1 — Immediate (Zero-Cost):**
Add `#[serde(default)]` to ALL fields in `Epigenetics` and its nested structs:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Epigenetics {
    #[serde(rename = "tr", default)]
    pub trust: TrustSection,
    
    #[serde(rename = "bn", skip_serializing_if = "Vec::is_empty", default)]
    pub bonds: Vec<Bond>,
    
    #[serde(rename = "es", default)]
    pub epistemic_status: EpistemicStatus,
    
    // ... all fields get #[serde(default)]
}
```

**Tier 2 — Version Envelope:**
Wrap JSON with a version number for breaking changes:

```rust
#[derive(Serialize, Deserialize)]
struct EpigeneticsEnvelope {
    #[serde(rename = "v", default = "default_v1")]
    version: u8,
    
    #[serde(flatten)]
    data: serde_json::Value,  // Parse version-specifically
}

fn deserialize_epigenetics(json: &str) -> Result<Epigenetics, Error> {
    let envelope: EpigeneticsEnvelope = serde_json::from_str(json)?;
    match envelope.version {
        1 => serde_json::from_value(envelope.data),
        2 => {
            let v2: EpigeneticsV2 = serde_json::from_value(envelope.data)?;
            Ok(v2.into())  // Convert to current version
        }
        _ => Err(Error::UnsupportedVersion),
    }
}
```

**Tier 3 — Consider CBOR Migration (Future):**
- CBOR is more compact than JSON (~30-40% smaller)
- CBOR supports schema evolution natively via integer keys
- Migration path: detect format on read (JSON starts with `{`, CBOR with CBOR tag)
- **Not urgent** — JSON with `#[serde(default)]` covers 95% of evolution needs

### 4.3 ConceptEntry JSON Evolution

The same `#[serde(default)]` pattern applies to `ConceptEntry`:

```rust
#[derive(Serialize, Deserialize)]
pub struct ConceptEntry {
    pub id: ConceptId,
    pub name: String,
    #[serde(default)]
    pub name_vi: Option<String>,
    #[serde(default)]
    pub name_en: Option<String>,
    #[serde(default = "default_tier")]
    pub tier: u8,
    #[serde(default)]
    pub category: Option<String>,
    // Future fields automatically safe:
    #[serde(default)]
    pub aliases: Vec<String>,  // New field — old JSON missing it gets Vec::new()
}
```

---

## 5. Graph Storage Migration (Q4)

### 5.1 The BondMeta Size Problem

`BondMeta` is a fixed 9-byte struct:
```text
[weight:2][creator:1][state:1][decay:1][timestamp:4] = 9 bytes
```

If OBKG Phase 2 needs more metadata per edge (e.g., `QualifiedBond` with context CID, confidence score, provenance chain), the 9-byte format is insufficient.

### 5.2 Migration Options

| Strategy | Mechanism | Pros | Cons |
|----------|-----------|------|------|
| **A. Version-Prefixed Variable Length** | First byte = version, then version-specific payload | Simple, extensible | Must handle both formats in hot path |
| **B. Auxiliary Metadata Table** | Keep 9-byte BondMeta, add `edge_meta_ext` table for overflow | Zero breaking change | Two lookups per edge, more complex writes |
| **C. CBOR-Encoded Values** | Replace fixed bytes with CBOR in `edges_out` value | Self-describing, infinitely extensible | Larger, slower to parse |
| **D. Tagged Length-Prefix** | `[version:1][len:2][payload:len]` | Clear framing | Variable-length values harder to scan |

### 5.3 Recommended: Strategy A (Version-Prefixed) + Strategy B (Auxiliary Table)

**Phase 1 — Add version prefix to BondMeta:**

```rust
impl BondMeta {
    const V1_SIZE: usize = 9;
    const V2_SIZE: usize = 13;  // +4 bytes for future fields
    
    pub fn to_bytes_versioned(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(1 + Self::V1_SIZE);
        buf.push(1u8);  // Version prefix
        buf.extend_from_slice(&self.to_bytes());
        buf
    }
    
    pub fn from_bytes_versioned(bytes: &[u8]) -> Self {
        if bytes.len() == 9 {
            // Legacy: no version prefix (v0)
            let mut buf = [0u8; 9];
            buf.copy_from_slice(bytes);
            Self::from_bytes(&buf)
        } else if bytes[0] == 1 {
            // v1: same as v0 but with prefix
            let mut buf = [0u8; 9];
            buf.copy_from_slice(&bytes[1..10]);
            Self::from_bytes(&buf)
        } else if bytes[0] == 2 {
            // v2: extended format
            Self::from_bytes_v2(&bytes[1..])
        } else {
            panic!("Unknown BondMeta version: {}", bytes[0]);
        }
    }
}
```

**Phase 2 — Auxiliary table for rich metadata:**

```rust
// New table for extended bond metadata (CBOR-encoded)
const TABLE_EDGE_META_EXT: TableDefinition<&[u8], &[u8]> = 
    TableDefinition::new("edge_meta_ext");
// Key: same 65-byte composite key as edges_out
// Value: CBOR-encoded QualifiedBondMeta
```

**Migration runner for GraphStorage v0→v1:**

```rust
fn migrate_graph_v0_to_v1(txn: &WriteTransaction) -> Result<(), StorageError> {
    // 1. Create _schema_meta table, set version = 1
    // 2. Create edge_meta_ext table
    // 3. No need to rewrite edges_out — reader handles both formats
    // (Lazy migration: v0 data stays as 9 bytes, new writes use 10+ bytes)
}
```

---

## 6. Rolling Upgrade in Distributed Network (Q5)

### 6.1 The Heterogeneous Network Problem

In a decentralized knowledge network, nodes upgrade independently. During the transition period:

```
Node A (v7) ←→ Node B (v6) ←→ Node C (v7)
```

**Scenarios that must work:**
1. v7 node receives v6 KU from peer → **Must decode successfully**
2. v6 node receives v7 KU from peer → **Must either decode or gracefully skip**
3. v7 node sends KU to v6 peer → **Must send in a format v6 understands**

### 6.2 Protocol Buffers Backward Compatibility Lessons

Protobuf's golden rules map directly to OneBrain:

| Protobuf Rule | OneBrain Equivalent |
|---|---|
| Never change field numbers | Never change opcode values |
| Never reuse field numbers | Never reuse opcode slots |
| Mark removed fields as `reserved` | Mark deprecated opcodes in spec |
| Unknown fields are preserved (round-trip) | Unknown opcodes are skipped on decode |
| All fields effectively optional | Missing instructions default to empty |

### 6.3 Recommended: N-1 Compatibility Window

**Policy:** Every release MUST be compatible with the previous release (N-1). This means:

1. **Encoding:** Nodes always send the **lowest common version** that the receiver understands
2. **Decoding:** Nodes always accept versions `[current, current-1]` at minimum
3. **Grace period:** Announce format changes 2 releases before removing old format support

**Implementation pattern:**

```rust
/// Negotiate wire format version during peer handshake
struct PeerCapabilities {
    /// Maximum Core DNA version this peer can decode
    max_core_dna_version: u8,
    /// Maximum Epigenetics envelope version
    max_epi_version: u8,
    /// Supported BondMeta versions
    bond_meta_versions: Vec<u8>,
}

/// When sending a KU to a peer, respect their capabilities
fn send_ku_to_peer(ku: &KuRuntime, peer: &PeerCapabilities) -> Vec<u8> {
    if peer.max_core_dna_version >= 2 {
        ku.encode_v7()  // New format
    } else {
        ku.encode_v6()  // Legacy format for old peers
    }
}
```

### 6.4 Version Negotiation Protocol

```text
HANDSHAKE:
  1. Node A → Node B: HELLO { versions: [core_dna: 1..2, epi: 1..2] }
  2. Node B → Node A: HELLO_ACK { agreed: [core_dna: 1, epi: 1] }
  3. Both nodes use the agreed (minimum common) versions
```

**Critical rule:** The wire format for **existing KUs** never changes (CID stability). Version negotiation only applies to:
- Epigenetics envelope format
- BondMeta encoding in graph sync
- Protocol-level framing (not content)

### 6.5 Upgrade Rollout Phases

| Phase | Duration | Action |
|-------|----------|--------|
| **1. Announce** | Release N | Add v2 decoder (read-only), keep v1 writer |
| **2. Enable** | Release N+1 | Default to v2 writer, fall back to v1 for old peers |
| **3. Deprecate** | Release N+2 | Log warnings for v1 reads, encourage upgrade |
| **4. Remove** | Release N+3 | Drop v1 decoder (breaking change, major version bump) |

---

## 7. Concrete Implementation Plan

### Phase 1: Foundation (No Breaking Changes)

| Task | Module | Effort | Risk |
|------|--------|--------|------|
| Add `_schema_meta` table to KuStorage | `ku-kql/storage.rs` | 2h | None — additive |
| Add `_schema_meta` table to GraphStorage | `ku-kql/graph_storage.rs` | 2h | None — additive |
| Add `_schema_meta` table to PersistentConceptDict | `ku-core/persistent_concept_dict.rs` | 2h | None — additive |
| Add `#[serde(default)]` to all Epigenetics fields | `ku-core/epigenetics.rs` | 1h | None — backward compatible |
| Add `#[serde(default)]` to ConceptEntry fields | `ku-core/concept_dict.rs` | 30m | None — backward compatible |
| Create migration runner trait + infrastructure | New `ku-core/migration.rs` | 4h | None — new code |

### Phase 2: Wire Format Hardening

| Task | Module | Effort | Risk |
|------|--------|--------|------|
| Multi-version Core DNA decoder | `ku-core/core_dna.rs` | 4h | Medium — must preserve exact v1 behavior |
| Version-prefixed BondMeta | `ku-core/graph_types.rs` | 3h | Medium — must handle legacy 9-byte format |
| Epigenetics version envelope | `ku-core/epigenetics.rs` | 3h | Low — JSON is forgiving |
| Unknown opcode skip in decoder | `ku-core/core_dna.rs` | 2h | Medium — must not corrupt state |

### Phase 3: Network Compatibility

| Task | Module | Effort | Risk |
|------|--------|--------|------|
| Peer capability handshake | `ku-net/` | 6h | High — protocol change |
| Version negotiation | `ku-net/` | 4h | High — must handle edge cases |
| Graceful unknown version handling | All decoders | 3h | Medium |

---

## 8. Key Recommendations

> [!IMPORTANT]
> **Recommendation 1: Never migrate Core DNA wire bytes.** CID = BLAKE3(wire_bytes) makes wire format migration fundamentally destructive. Treat stored wire bytes as immutable, like Git blob objects.

> [!IMPORTANT]
> **Recommendation 2: Add `_schema_meta` table to all redb databases immediately.** This is zero-risk, zero-breaking-change, and unblocks all future migrations.

> [!TIP]
> **Recommendation 3: Add `#[serde(default)]` to every field in every serialized struct today.** This is the single highest-leverage change — it makes all JSON schemas forward-compatible with zero effort.

> [!WARNING]
> **Recommendation 4: BondMeta must become variable-length before OBKG Phase 2.** The current 9-byte fixed format cannot accommodate QualifiedBond metadata. Add a version prefix byte now, while the data set is small.

> [!CAUTION]
> **Recommendation 5: Establish N-1 compatibility policy before the network has real peers.** Once peers exist with different versions, breaking changes become exponentially more expensive.

---

## 9. References

### Industry Patterns
- **SQLite PRAGMA user_version** — canonical embedded DB versioning pattern
- **Protocol Buffers backward compatibility** — golden rules for wire format evolution (never change field numbers, never reuse, mark reserved)
- **IPFS CID stability** — IPIP-0499 standardized construction profiles for deterministic CIDs
- **Kubernetes rolling updates** — N-1 compatibility, readiness probes, canary releases

### Rust Ecosystem
- **redb** — no built-in migration; application-level metadata table is the standard pattern
- **native_db** (crate) — higher-level wrapper on redb with schema migration via Rust type coercion
- **clove1db** (crate) — redb wrapper with built-in schema migrations and versioned backups
- **serde `#[serde(default)]`** — zero-cost forward compatibility for JSON/CBOR schemas

### OneBrain-Specific
- Core DNA v6 version is 3 bits in VER_META byte (bits 7-5), allowing versions 0-7
- BondMeta 9-byte format has no extensibility — `from_bytes(&[u8; 9])` is hard-coded
- Epigenetics uses `serde_json` with partial `#[serde(default)]` coverage
- GraphStorage composite keys encode CIDs directly — key format changes require full table rebuild

---

## Appendix A: Migration Runner Trait (Draft)

```rust
/// A single schema migration step.
pub struct Migration {
    /// Version this migration upgrades FROM.
    pub from: u32,
    /// Version this migration upgrades TO.
    pub to: u32,
    /// Human-readable description.
    pub description: &'static str,
    /// The migration function — receives a write transaction.
    pub apply: fn(&redb::WriteTransaction) -> Result<(), StorageError>,
}

/// Registry of all migrations for a storage module.
pub struct MigrationRegistry {
    pub target_version: u32,
    pub migrations: Vec<Migration>,
}

impl MigrationRegistry {
    /// Run all pending migrations on database open.
    pub fn run(&self, db: &Database) -> Result<(), StorageError> {
        let current = self.read_version(db)?;
        if current >= self.target_version {
            return Ok(());
        }
        
        for m in &self.migrations {
            if m.from >= current && m.to <= self.target_version {
                let txn = db.begin_write()?;
                (m.apply)(&txn)?;
                self.write_version(&txn, m.to)?;
                txn.commit()?;
            }
        }
        Ok(())
    }
}
```

## Appendix B: Decision Matrix

| Aspect | Wire Format (Core DNA) | Epigenetics (JSON) | BondMeta (Binary) | ConceptEntry (JSON) |
|--------|----------------------|-------------------|-------------------|-------------------|
| **CID-sensitive?** | ✅ YES — CID = hash of bytes | ❌ No — separate table | ❌ No — graph index | ❌ No — concept index |
| **Can migrate in-place?** | ❌ Never | ✅ Yes — JSON is flexible | ⚠️ With version prefix | ✅ Yes — JSON is flexible |
| **Best strategy** | Multi-version decoder | `#[serde(default)]` + envelope | Version-prefixed variable length | `#[serde(default)]` |
| **Urgency** | Low (v6 works) | Medium (missing defaults) | High (9-byte limit) | Low (JSON flexible) |
| **Risk of inaction** | Silent reject of new KUs | Deserialization failures on new fields | Cannot add OBKG metadata | Deserialization failures on new fields |
