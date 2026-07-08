# Chapter 8: Cross-Pillar Integration

> *"No man is an island, entire of itself; every man is a piece of the continent, a part of the main."*
> — John Donne, *Devotions upon Emergent Occasions* (1624)

---

OneBrain Storage does not exist in isolation. It serves as the persistence and replication substrate for five preceding pillars: Knowledge Unit (P1), Network Protocol (P2), Consensus/PoMV (P4), Token Economics (P5), and Knowledge Graph (P7). This chapter details the integration interfaces between OBS and each pillar, demonstrating how the storage layer achieves cross-pillar functionality through **composition** — additive modules that adapt to existing pillar APIs without modifying foundation code.

---

## §8.1 Integration Philosophy

We adopt a strict design axiom first established during the OBKG (Pillar 7) integration and subsequently formalised as a project-wide principle [1]:

> **Axiom A3 (Composition over Modification).** Pillar $N$ adapts to Pillars $1$ through $N-1$. It introduces new modules, new message types, and new wire protocol extensions, but it never modifies existing source files of earlier pillars.

**Table 1.** Files modified by OBS in earlier pillars.

| Pillar | Source Files Modified | OBS Modules Added |
|--------|-----------------------|-------------------|
| P1 (KU Core) | 0 | `obs_schema.rs`, `obs_cache.rs` |
| P2 (OBP Network) | 0 | `dht_store.rs`, `replication.rs` |
| P3 (KQL) | 0 | — (uses existing `storage.rs`) |
| P4 (PoMV) | 0 | — (reads `MetabolismStore`) |
| P5 (OBT) | 0 | — (reads `obt_storage_reward.rs`) |
| P7 (OBKG) | 0 | — (uses existing `graph_storage.rs`) |

This zero-modification property is verified by the Git history: all OBS-related code resides in files prefixed with `obs_`, `dht_`, or `replication`, none of which existed prior to Pillar 8 development.

---

## §8.2 P1 Integration: Knowledge Unit Core

The integration with P1 (Knowledge Unit) is the most fundamental, as OBS exists primarily to persist KU objects.

### §8.2.1 CID Computation

Every storage operation begins with computing the content identifier from Core DNA wire bytes:

$$\text{CID} = \text{BLAKE3}(\text{core\_dna.encode()})$$

The `core_dna` module provides `encode() -> Vec<u8>` which produces the binary wire format: `MAGIC(1B) | VER_META(1B) | INSTRUCTIONS(var) | END(1B) | CRC-16(2B)`. The BLAKE3 hash of these bytes serves as the universal key across all OBS tables.

### §8.2.2 Two-Layer Persistence Model

OBS maps directly onto the KU's 3-layer architecture:

| KU Layer | OBS Treatment | Table | Consistency |
|----------|---------------|-------|-------------|
| Layer 1: Core DNA | Immutable write-once | `kus` | CID = hash (zero-cost) |
| Layer 2: Epigenetics | Mutable JSON overlay | `epigenetics` | CRDT (§6.4) |
| Layer 3: Expression | Not persisted | — | Regenerated on demand |

The Expression Layer (natural language rendering) is deliberately excluded from persistence — it is a deterministic function of Core DNA and the reader's locale, and can always be regenerated. This reduces storage requirements by approximately 60% compared to a system that persists rendered text.

### §8.2.3 Bond Extraction and Graph Indexing

When a KU is stored via `KuStorage::put()`, the operation automatically extracts bonds from the Epigenetics layer and indexes them in `GraphStorage`. This cross-module call is the primary bridge between P1 (KU data) and P7 (Knowledge Graph topology):

1. Decode `EpigeneticSection` from the JSON overlay.
2. For each bond in `epigenetics.bonds`, construct a `BondMeta` record.
3. Insert into all 6 `GraphStorage` tables within the same redb write transaction.

This ensures that every stored KU is immediately discoverable through graph traversal queries.

---

## §8.3 P2 Integration: Network Protocol

OBS extends the 9-layer OBP stack [2] with three storage-specific message types and hooks into existing protocol layers.

### §8.3.1 Storage Message Types

**Table 2.** OBS-specific OBP message types.

| Message | Code | Layer | Direction | Payload |
|---------|------|-------|-----------|---------|
| `STORE_RPC` | `0x24` | L6 (Content) | Sender → Replica | CID + wire_bytes + epi_json |
| `STORE_ACK` | `0x25` | L6 (Content) | Replica → Sender | CID + status_code |
| `REPLICATION_CHECK` | `0x26` | L6 (Content) | Any → Any | CID + replica_count |
| `CacheInvalidate` | `0x68` | L7 (PubSub) | Gossip | CID (invalidation) |

These messages follow the standard OBP wire format: 6-byte header (`MAGIC | VER | TYPE | FLAGS | LEN_HI | LEN_LO`) followed by the payload.

### §8.3.2 DHT Layer Integration

OBS persists DHT entries to redb via `DhtPersistence` (§6.3), ensuring that `STORE` and `FIND_VALUE` operations survive node restarts. The integration follows the existing DHT module's `DhtNode` API without modification — `DhtPersistence` wraps the in-memory `HashMap` entries with a redb-backed durable store.

### §8.3.3 Stigmergy Layer Integration

The stigmergy-driven repair mechanism (§6.5) leverages the existing pheromone routing infrastructure in `stigmergy.rs`. OBS introduces a **replication pheromone** channel alongside the existing query routing pheromone:

- **Query pheromone** (existing): Reinforced when a query route succeeds; evaporated on failure.
- **Replication pheromone** (OBS): Reinforced when a replica ACK is received; evaporated when a replica node fails.

Both channels share the same `reinforce()` / `evaporate()` / `best_hop()` API, demonstrating the composability of the bio-inspired design.

---

## §8.4 P4 Integration: Proof-of-Metabolic-Value (PoMV)

The PoMV consensus mechanism (Pillar 4) produces the **metabolic rate** signal — a continuous measure of a KU's biological activity computed from 7 metabolic events (access, share, cite, challenge, corroborate, embed, bookmark) [3]. OBS consumes this signal at three integration points.

### §8.4.1 MetabolismStore as Data Source

The `MetabolismStore` (283 LOC, 7 tests) maintains per-KU metabolism records in a `HashMap<[u8; 32], KUMetabolism>`. OBS reads metabolic rates from this store for:

1. **M-ARC eviction decisions** (§5.3): The cache evicts entries with the lowest `metabolic_rate`.
2. **Storage reward computation** (§8.5): The `demand_w` factor in the 5-factor reward formula.
3. **Replication repair priority** (§6.5): High-metabolism KUs are repaired first.

### §8.4.2 CRDT Merge for Network Consistency

`MetabolismStore::merge_remote()` accepts incoming metabolism deltas from network peers and applies CRDT merge semantics (GCounter-based). This ensures that metabolic rates converge across all nodes without coordination, providing OBS with a consistent view of KU activity regardless of which node processes a given event.

### §8.4.3 Garbage Collection

The MetabolismStore implements a biologically-inspired garbage collection policy:

$$\text{GC eligible} \iff r_m < 0.0001 \wedge \text{age} > 365\text{ days} \wedge \text{engagement} = 0$$

Only KUs that are metabolically dead (rate below $10^{-4}$), older than one year, and have zero total engagement are eligible for removal. This conservative policy prevents premature deletion of knowledge that may have long-term archival value.

---

## §8.5 P5 Integration: OBT Token Economics

The OBT token system (Pillar 5) provides economic incentives for storage participation. The integration centres on two mechanisms: the **5-factor storage reward formula** and the **Proof-of-Storage for Knowledge Units (PoS-KU)** challenge protocol.

### §8.5.1 Five-Factor Storage Reward (R4 Stream)

Storage rewards are computed per-KU per-epoch using a multiplicative 5-factor formula [4]:

$$R_4(\text{KU}) = B_{\text{rate}} \times w_s \times w_r \times w_d \times f_t \times f_{\tau}$$

**Table 3.** Storage reward factor definitions.

| Factor | Symbol | Range | Formula | Rationale |
|--------|--------|-------|---------|-----------|
| Base rate | $B_{\text{rate}}$ | Fixed | Protocol constant | Baseline reward per KU-epoch |
| Size weight | $w_s$ | [0.1, 10.0] | $\propto$ KB size | Larger KUs cost more to store |
| Rarity weight | $w_r$ | [0.5, 3.0] | $K_{\text{target}} / R_{\text{actual}}$ | Under-replicated KUs earn more |
| Demand weight | $w_d$ | [0.1, 5.0] | $r_m / \tilde{r}_m$ | High-metabolism KUs earn more |
| Duration factor | $f_t$ | [0.0, 2.0] | Linear ramp over maturity period | Long-term storage earns more |
| Trust factor | $f_{\tau}$ | [0.0, 1.0] | EigenTrust score | Sybil resistance |

The **rarity weight** $w_r$ creates a natural incentive to store under-replicated KUs — nodes earn up to 3× the base reward for maintaining replicas of rare knowledge. Conversely, over-replicated KUs (where $R_{\text{actual}} > K_{\text{target}}$) yield diminished rewards, discouraging redundant storage.

The **trust factor** $f_{\tau}$ uses the EigenTrust [5] reputation score to prevent Sybil attacks — a node that creates thousands of fake identities to claim storage rewards receives near-zero trust from the reputation algorithm.

### §8.5.2 Proof-of-Storage for Knowledge Units (PoS-KU)

To verify that nodes actually store the KUs they claim, the protocol issues periodic challenges. Challenge generation is deterministic via BLAKE3-seeded random selection:

$$\text{challenged} \iff \text{BLAKE3}(\text{epoch} \parallel \text{CID} \parallel \text{node\_id}) \mod 10 = 0$$

This selects approximately **10%** of stored KUs for challenge per epoch, with a per-KU maximum of 5 challenges per epoch.

**Table 4.** PoS-KU challenge types.

| Type | Distribution | Challenge | Expected Response |
|------|-------------|-----------|-------------------|
| `FullHash` | ~20% | Recompute BLAKE3 digest | Full 32-byte digest |
| `ByteRange` | ~50% | Return bytes at offset+length | Exact byte sequence |
| `FieldExtract` | ~30% | Decode specific opcode field | Field value |

Challenge verification uses **constant-time byte comparison** (XOR-based) to prevent timing side-channel attacks that could allow a node to infer challenge answers without actually storing the data [6].

### §8.5.3 Node-Level Aggregation

Per-node storage rewards are the sum of per-KU rewards across all KUs the node stores:

$$R_{4,\text{node}} = \sum_{i=1}^{n} R_4(\text{KU}_i)$$

The `compute_node_storage_reward()` function (in `obt_storage_reward.rs`, 676 LOC, 14 tests) iterates over all `StoredKuInfo` records and aggregates rewards, applying the trust factor $f_{\tau}$ once at the node level.

---

## §8.6 P7 Integration: Knowledge Graph (OBKG)

The OBKG (Pillar 7) is the most tightly coupled pillar, as graph operations are directly backed by OBS storage.

### §8.6.1 GraphStorage as OBKG Substrate

The 6-table composite key design in `graph_storage.rs` (§4.3) serves as the physical storage layer for all OBKG operations:

| OBKG Operation | GraphStorage API | Query Pattern |
|----------------|------------------|---------------|
| Outgoing edges | `outgoing_bonds(src)` | Prefix scan on `edges_out` |
| Incoming edges | `incoming_bonds(tgt)` | Prefix scan on `edges_in` |
| Edges by type | `outgoing_by_type(src, rel)` | Prefix scan on `edges_type` |
| Bond state filter | Scan `index_state` | Prefix scan by state byte |
| Weight ordering | Scan `bond_weight` | Ordered iteration |
| Temporal queries | `bonds_in_time_range()` | Range scan on `edge_time` |

### §8.6.2 Schema Versioning for OBKG

The `graph_storage_registry()` in `obs_schema.rs` registers a v1 schema for the 6-table graph index. Future OBKG features (e.g., variable-length BondMeta for Phase 2 qualifiers) will be handled through schema migrations that add new tables or extend existing value formats without breaking the v1 key layout.

### §8.6.3 OBS Cache and Graph Traversal

The M-ARC cache (§5) integrates with OBKG's `spreading_activation()` algorithm through the **1-hop selective prefetch** strategy. When a KU is accessed during graph traversal, the cache proactively loads its neighbours (bond weight > 5,000), reducing subsequent traversal latency from 50–200 µs (redb) to < 1 µs (cache hit).

The Dream Mode and Consolidation Engine — OBKG's offline graph restructuring processes — bypass the M-ARC cache entirely (§5.6), reading directly from the warm tier to avoid cache thrashing during batch operations.

---

## §8.7 Integration Metrics

**Table 5.** Cross-pillar integration summary.

| Metric | Value |
|--------|-------|
| Pillars integrated | 5 of 7 (P1, P2, P4, P5, P7) |
| Files modified in P1–P7 | **0** |
| OBS modules added to P1 crate | 2 (`obs_schema.rs`, `obs_cache.rs`) |
| OBS modules added to P2 crate | 2 (`dht_store.rs`, `replication.rs`) |
| OBP message types added | 4 (`0x24`, `0x25`, `0x26`, `0x68`) |
| CRDT types consumed | 5 (PNCounter, GCounter, LWW, ORSet, VClock) |
| Economic signals consumed | 3 (metabolic_rate, eigentrust_score, rarity) |

The zero-modification integration validates the **Composition over Modification** axiom: OBS achieves full cross-pillar functionality through additive modules, read-only adapters, and message-type extensions, without altering a single line of code in Pillars 1–7.

---

## §8.8 Summary

This chapter demonstrated that OBS integrates with five OneBrain pillars through a composition-based architecture that requires zero modifications to existing code. The integration spans CID computation from Core DNA (P1), storage-specific message types for OBP (P2), metabolic rate consumption from PoMV (P4), a 5-factor storage reward formula with PoS-KU challenges for OBT (P5), and 6-table graph index support for OBKG (P7).

The next chapter evaluates the complete OBS implementation through quantitative benchmarks, test coverage analysis, and comparative assessment against existing systems.

---

## References

[1] OneBrain Project Contributors, "OBKG: A Bio-Inspired Knowledge Graph with Dream Mode and Federated Embeddings," *OneBrain Technical Report*, 2026.

[2] OneBrain Project Contributors, "OneBrain Protocol: A Bio-Inspired Peer-to-Peer Network for Decentralized Knowledge Sharing," *OneBrain Technical Report*, 2026.

[3] OneBrain Project Contributors, "Proof-of-Metabolic-Value: A Content-Agnostic Consensus Mechanism for Knowledge Networks," *OneBrain Technical Report*, 2026.

[4] OneBrain Project Contributors, "OBT: A Utility Token for Decentralized Knowledge Economics," *OneBrain Technical Report*, 2026.

[5] S. D. Kamvar, M. T. Schlosser, and H. Garcia-Molina, "The EigenTrust Algorithm for Reputation Management in P2P Networks," in *Proc. 12th International Conference on World Wide Web (WWW)*, 2003, pp. 640–651.

[6] D. J. Bernstein, "Constant-time comparison functions," *cr.yp.to*, 2010.
