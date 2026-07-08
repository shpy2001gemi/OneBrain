# Chapter 6: Distributed Replication

> *"Nature is the greatest engineer of redundancy. Every cell carries the full blueprint; every organism is a distributed backup of its species' knowledge."*
> — Richard Dawkins, *The Selfish Gene* (1976)

---

In the preceding chapters we described the local storage substrate — how Knowledge Units are persisted on a single node (§4) and cached in memory for hot-path access (§5). A single-node store, however, provides no durability guarantee against hardware failure, network partition, or node churn. This chapter addresses the distributed dimension of **OneBrain Storage (OBS)**: how KUs are replicated across the peer-to-peer network to achieve fault tolerance, consistency, and availability without any centralised coordinator.

We present three principal contributions in this chapter: (1) a **tier-aware replica placement strategy** that distributes $R = 7$ replicas across a heterogeneous node hierarchy using a 4+2+1 allocation scheme (§6.2); (2) a **dual-layer consistency model** that combines zero-overhead immutability for Layer 1 (Core DNA) with five distinct CRDT types for Layer 2 (Epigenetics) metadata (§6.4); and (3) a **stigmergy-driven repair mechanism** inspired by ant colony pheromone trails that autonomously re-replicates under-replicated KUs (§6.5).

---

## §6.1 Replication Factor Analysis

The choice of replication factor $R$ is governed by a fundamental observation: **Knowledge Units are extraordinarily small**. A minimal Fact-type KU occupies approximately **16 bytes** of Core DNA wire format, while a complex multi-instruction KU with full metadata reaches approximately **172 bytes** [1]. This stands in stark contrast to the objects managed by conventional peer-to-peer storage systems — IPFS operates on 256 KB blocks [2], Filecoin seals 32 GiB sectors [3], and Swarm partitions data into 4 KB chunks [4].

The diminutive size of KUs fundamentally changes the replication calculus. At $R = 7$, the total network storage cost for a single KU is:

$$C_{\text{total}} = R \times \bar{s} = 7 \times 172 = 1{,}204 \text{ bytes}$$

where $\bar{s}$ represents the average KU wire size. This is less than 1.2 KB — a negligible cost that would be dwarfed by the metadata overhead of erasure coding schemes. For comparison, a Reed-Solomon RS(10,4) encoding of a 172-byte KU would require fragment headers, parity indices, and alignment padding that collectively exceed the data itself [5].

### §6.1.1 Why R = 7

We select $R = 7$ for four complementary reasons:

1. **Failure tolerance.** With $R = 7$ and a minimum healthy threshold of $R_{\min} = 4$, the system tolerates up to 3 simultaneous node failures before a KU becomes critically under-replicated. This exceeds the typical $f < n/3$ Byzantine tolerance threshold.

2. **Alignment with node hierarchy.** The OneBrain Protocol (OBP) defines a 7-tier node hierarchy — Leaf, Contributor, LocalSuperPeer, RegionalSuperPeer, CountrySuperPeer, ContinentalSuperPeer, and GlobalBackbone [6]. Each tier represents a distinct level of infrastructure commitment, and $R = 7$ enables at least one replica to be anchored at each tier level when the network is sufficiently diverse.

3. **Negligible marginal cost.** At 1,204 bytes per KU, even a node storing 1 million KUs with full replication consumes approximately 1.15 GB — well within the capacity of the cheapest storage tier.

4. **Quorum arithmetic.** A majority quorum of $\lceil R/2 \rceil + 1 = 4$ replicas ensures that any two quorums intersect, providing the foundation for consistent reads when needed.

### §6.1.2 Dual-K Architecture

We distinguish between the **routing parameter** $K = 20$ (the Kademlia k-bucket size used for DHT routing table maintenance [7]) and the **storage replication factor** $R = 7$. This separation is deliberate:

- $K = 20$ governs how many peers each node tracks per bucket for routing purposes, providing $O(\log N)$ lookup in a network of $N$ nodes.
- $R = 7$ governs how many copies of each KU are maintained for durability.

The two parameters serve orthogonal concerns. Conflating them — as some Kademlia implementations do — forces a trade-off between routing efficiency and storage redundancy that is unnecessary for ultra-small objects.

---

## §6.2 Tier-Aware Replica Placement (4+2+1)

Standard Kademlia replication stores values at the $K$ nodes closest to the key in XOR metric space [7]. While this provides good load distribution, it is blind to the physical infrastructure characteristics of nodes. A naive XOR-closest placement might concentrate all replicas on residential laptops with intermittent connectivity, ignoring the availability of dedicated servers.

We introduce a **tier-aware placement strategy** that allocates $R = 7$ replicas across three categories:

$$R = R_{\text{xor}} + R_{\text{tier}} + R_{\text{div}} = 4 + 2 + 1$$

### §6.2.1 Placement Algorithm

Given a KU with content identifier $\text{CID}$ and a set of candidate nodes $\mathcal{N}$, the `select_targets()` algorithm proceeds as follows:

**Step 1 — XOR-Closest ($R_{\text{xor}} = 4$).** Compute the XOR distance between the CID and each candidate's node identifier:

$$d(n_i) = \text{CID} \oplus \text{BLAKE3}(\text{node\_id}(n_i))$$

Sort candidates by $d(n_i)$ in ascending order and select the closest 4 as `xor_closest`. This preserves the standard Kademlia property of deterministic key-to-node mapping.

**Step 2 — Tier-Anchored ($R_{\text{tier}} = 2$).** From the remaining candidates:
- Select the XOR-closest node with tier $\geq 2$ (RegionalSuperPeer or above) as `tier_anchored[0]`.
- Select the XOR-closest node with tier $\geq 3$ (CountrySuperPeer or above) as `tier_anchored[1]`.

These anchors ensure that at least two replicas reside on infrastructure-grade nodes with high uptime, dedicated bandwidth, and reliable storage.

**Step 3 — Diversity ($R_{\text{div}} = 1$).** From the remaining candidates, select the first node whose IP address falls in a different /24 subnet than all previously selected nodes. This provides partition tolerance against localised network failures.

**Step 4 — Shortfall Filling.** If any category is under-populated (e.g., no tier $\geq 3$ node is available), the deficit is filled from the XOR-closest overflow — the next-closest candidates not yet selected.

The algorithm is implemented in `replication.rs` (663 LOC) with the following data structure:

```rust
pub struct ReplicationTargets {
    pub xor_closest: Vec<u64>,    // 4 XOR-nearest nodes
    pub tier_anchored: Vec<u64>,  // 2 infrastructure anchors
    pub diversity: Vec<u64>,      // 1 subnet-diverse node
}
```

**Complexity.** The dominant cost is the initial sort: $O(|\mathcal{N}| \log |\mathcal{N}|)$. The subsequent linear scans for tier and diversity selection are $O(|\mathcal{N}|)$.

### §6.2.2 Biological Analogy

The 4+2+1 placement mirrors biological seed dispersal strategies. Wind-dispersed seeds (analogous to XOR-closest) land near the parent plant with high probability. Animal-dispersed seeds (tier-anchored) are deposited at reliable foraging sites. Water-dispersed seeds (diversity) travel to geographically distant habitats. The combination maximises species survival across diverse environmental disruptions [8].

---

## §6.3 DHT Persistence

The Distributed Hash Table serves as the discovery mechanism for the cold storage tier. When a node stores KU replicas, the DHT entries must survive node restarts to avoid costly re-replication after every reboot.

### §6.3.1 Storage Design

We implement DHT persistence in `dht_store.rs` (568 LOC, 13 tests) using two redb tables:

**Table 1: `dht_entries`.** Maps content identifiers to serialised DHT entry records:

| Key | Value | Serialisation |
|-----|-------|---------------|
| CID `[u8; 32]` | `DhtEntryRecord` | CBOR (ciborium) |

The `DhtEntryRecord` contains:
```rust
pub struct DhtEntryRecord {
    pub value: Vec<u8>,          // Stored KU wire bytes
    pub stored_at: u64,          // Unix timestamp
    pub ttl_secs: Option<u64>,   // Optional time-to-live
}
```

**Table 2: `replica_meta`.** Tracks replication metadata for storage reward computation (§8.5):

| Key | Value | Serialisation |
|-----|-------|---------------|
| CID `[u8; 32]` | `StoredKuMetaRecord` | CBOR |

```rust
pub struct StoredKuMetaRecord {
    pub actual_replicas: u32,     // Known replica count
    pub first_stored_epoch: u64,  // First storage epoch
    pub epochs_stored: u64,       // Duration of storage
}
```

### §6.3.2 TTL Expiration

Expired entries are removed via a two-pass algorithm:

1. **Read pass:** Iterate all entries in a read transaction, collecting CIDs where `stored_at + ttl_secs < now`.
2. **Write pass:** Delete collected CIDs in a single write transaction.

The two-pass approach avoids holding a write transaction during the potentially long read scan, minimising lock contention. Complexity is $O(n)$ where $n$ is the total entry count.

### §6.3.3 Batch Persistence

At epoch boundaries (every `OBT_EPOCH_DURATION_S = 3,600` seconds), the node flushes accumulated DHT entries to disk via `persist_batch()`. This batches multiple small writes into a single redb write transaction, amortising the transaction overhead. Schema versioning is enforced via `obs_schema::dht_store_registry()`.

### §6.3.4 What is NOT Persisted

Following the principle of **selective persistence**, we deliberately exclude:

- **Routing table:** Reconstructed from seed nodes via `FIND_NODE` in approximately 30 seconds [6]. Persisting stale routing entries would be counterproductive.
- **Pheromone table:** Ephemeral by design — pheromone trails should evaporate naturally (§6.5). Persisting them would violate the bio-inspired decay model.
- **Epigenetics cache:** Recovered lazily via CRDT gossip, converging in 2–3 rounds (30–60 seconds). The cost of gossip-based recovery is negligible compared to the complexity of persisting CRDT state.

---

## §6.4 CRDT-Based Consistency Model

The dual-layer architecture of the Knowledge Unit (§3.4) enables a remarkably simple consistency model. Layer 1 (Core DNA) requires no consistency protocol whatsoever; Layer 2 (Epigenetics) achieves eventual consistency through Conflict-Free Replicated Data Types [9].

### §6.4.1 Layer 1: Zero-Cost Consistency

Core DNA wire bytes are **immutable** and **content-addressed**. The CID is the BLAKE3 hash of the wire bytes, which means:

$$\text{CID}(k) = \text{BLAKE3}(\text{wire\_bytes}(k))$$

Any replica holding the same wire bytes is, by definition, identical to every other replica. There is no consistency problem to solve — no conflicts, no merges, no coordination. The integrity of each replica is verified on read by recomputing the hash and comparing it to the CID key.

This zero-cost property is a direct consequence of the **Immutable Wire + Mutable Overlay** design principle (§1.3, A4). It eliminates the need for consensus protocols, quorum reads, or version vectors for the primary data — a significant advantage over systems where stored content can be modified in place.

### §6.4.2 Layer 2: Five-CRDT Eventual Consistency

Epigenetic metadata — trust scores, bond states, epistemic status, domain labels, and metabolic counters — evolves over time as the knowledge is consumed, verified, and challenged by network participants. This mutable layer uses five distinct CRDT types, each selected for the semantic requirements of the metadata it governs:

**Table 1.** CRDT type assignments for Layer 2 metadata.

| Metadata | CRDT Type | Merge Semantics | Rationale |
|----------|-----------|-----------------|-----------|
| Trust score | `PNCounter` | Sum of increments − decrements | Concurrent corroborations and challenges must both be counted |
| Corroboration count | `GCounter` | Max per node | Grow-only; corroborations cannot be retracted |
| Challenge count | `GCounter` | Max per node | Grow-only; challenges cannot be retracted |
| Epistemic status | `LWWRegister` | Latest timestamp wins | Status transitions are ordered; concurrent transitions resolve by wall clock |
| Domain labels | `ORSet` | Union with unique tags | Domains can be added and removed concurrently |
| Active bonds | `ORSet` | Union with unique tags | Bond creation and deletion must coexist without lost-update |
| Causal ordering | `VectorClock` | Component-wise max | Establishes happens-before relation across nodes |

All five CRDT types satisfy the mathematical properties of a join-semilattice:

$$\text{merge}(a, b) = \text{merge}(b, a) \quad \text{(commutativity)}$$
$$\text{merge}(a, \text{merge}(b, c)) = \text{merge}(\text{merge}(a, b), c) \quad \text{(associativity)}$$
$$\text{merge}(a, a) = a \quad \text{(idempotence)}$$

These properties guarantee **Strong Eventual Consistency (SEC)** [9]: any two replicas that have received the same set of updates — regardless of order — will converge to the same state.

### §6.4.3 Convergence Analysis

Epigenetic updates propagate via the existing `metabolism_gossip` protocol [6]. Each gossip round propagates updates to $\text{fan-out} = 3$ peers. In a network of $N$ nodes, full propagation requires approximately $\log_3(N)$ rounds. At a gossip interval of 10 seconds:

$$T_{\text{converge}} \approx \log_3(N) \times 10\text{s}$$

For $N = 10{,}000$ nodes: $T_{\text{converge}} \approx \lceil \log_3(10{,}000) \rceil \times 10 \approx 9 \times 10 = 90\text{s}$.

In practice, with overlapping gossip waves and the small size of CRDT deltas (typically < 100 bytes per KU), convergence occurs in **2–3 rounds (30–60 seconds)** for the relevant neighbourhood of a KU.

### §6.4.4 Rejected Alternatives

We considered and rejected three alternative consistency models:

1. **Quorum reads/writes ($R + W > N$):** Overkill for metadata updates where eventual consistency suffices. Requiring quorum for every trust score increment would create unnecessary latency and coordination overhead, violating the bio-inspired philosophy of autonomous, uncoordinated agents.

2. **Primary-copy replication:** Designating a primary replica introduces a single point of failure and requires leader election — antithetical to the fully decentralised design.

3. **Last-writer-wins only:** While LWW is appropriate for epistemic status (a single ordered value), applying it to trust scores would lose concurrent updates — two nodes simultaneously corroborating a KU should both be counted, not one overwritten.

---

## §6.5 Stigmergy-Driven Replication Repair

In biological systems, ant colonies maintain trail networks through **stigmergy** — indirect coordination via environmental signals (pheromone deposits) rather than direct communication [10]. We adapt this principle to replication repair: each CID in the network carries a virtual **replication pheromone** whose strength reflects the health of its replica set.

### §6.5.1 Pheromone Model

The replication pheromone strength $\phi$ for a given CID is defined as:

$$\phi(\text{CID}) = \phi_0 \times e^{-\lambda t} \times \frac{R_{\text{actual}}}{R_{\text{target}}}$$

where:
- $\phi_0$ is the initial pheromone deposit at replication time
- $\lambda$ is the natural decay rate (evaporation)
- $t$ is the time since last reinforcement
- $R_{\text{actual}}$ is the current known replica count
- $R_{\text{target}} = 7$ is the target replication factor

When $R_{\text{actual}} < R_{\text{target}}$, the pheromone weakens below the baseline, signalling to **foraging nodes** that re-replication is needed.

### §6.5.2 Failure Detection and Evaporation

The SWIM protocol [11] — already deployed in OBP for membership management — detects node failures through periodic ping/ping-req cycles. When a node is marked as failed:

1. All CIDs for which the failed node was a replica have their pheromone strength reduced proportionally.
2. The reduction is propagated via metabolism gossip (§6.4.3).
3. Nodes with available storage capacity **forage** for weak pheromones — CIDs whose $\phi$ falls below a repair threshold $\phi_{\text{repair}}$.

### §6.5.3 Repair Prioritisation

High-metabolism KUs — those being actively accessed, cited, and challenged — receive priority repair. The repair priority score combines pheromone weakness with metabolic demand:

$$P_{\text{repair}}(\text{CID}) = \frac{1}{\phi(\text{CID})} \times r_m(\text{CID})$$

where $r_m$ is the metabolic rate. This ensures that a heavily-used KU with only 2 remaining replicas is repaired before a dormant KU with 5 replicas.

### §6.5.4 Passive Repair Fallback

In addition to active stigmergy-driven repair, a passive repair mechanism triggers when a `FIND_VALUE` lookup for a CID discovers fewer replicas than expected. The querying node initiates a supplementary `STORE_RPC` to additional nodes to restore the target count. This serves as a last-resort repair for CIDs that have not been reached by active foraging.

---

## §6.6 Health Classification and ACK Tracking

Replication operations are asynchronous — a `STORE_RPC` message is sent to each target node, and the sender tracks acknowledgements.

### §6.6.1 Replication Status

The `ReplicationManager` (663 LOC, 15 tests) classifies the health of each CID's replica set:

**Table 2.** Replication health classification.

| Status | Condition | Action |
|--------|-----------|--------|
| `Healthy` | $R_{\text{acked}} \geq 7$ | No action needed |
| `Degraded` | $4 \leq R_{\text{acked}} < 7$ | Schedule background repair |
| `Critical` | $R_{\text{acked}} < 4$ | Immediate priority repair |
| `Unknown` | No ACK data available | Query DHT for replica count |

### §6.6.2 ACK Tracking Protocol

Each replication initiation creates a `PendingStore` record:

```rust
pub struct PendingStore {
    pub cid: [u8; 32],
    pub target_nodes: Vec<u64>,
    pub acked_nodes: Vec<u64>,
    pub initiated_at: u64,
}
```

ACK handling is **idempotent** — duplicate acknowledgements from the same node are silently ignored. Timed-out stores (past a configurable deadline) are flagged for retry via `timed_out_stores()`. Fully-acknowledged stores are cleaned up by `cleanup_completed()`.

### §6.6.3 Wire Protocol

Three OBP message types support the replication protocol:

| Message | Code | Direction | Payload |
|---------|------|-----------|---------|
| `STORE_RPC` | `0x24` | Sender → Replica | CID + wire_bytes + epigenetics |
| `STORE_ACK` | `0x25` | Replica → Sender | CID + status |
| `REPLICATION_CHECK` | `0x26` | Any → Any | CID + replica_count query |

---

## §6.7 Anti-Hoarding and Anti-Freeloading

A decentralised storage system must defend against two adversarial behaviours: **hoarding** (nodes that store data but refuse to serve it) and **freeloading** (nodes that request data but refuse to store it).

### §6.7.1 Serve Ratio Tracking

Each node's storage reward is modulated by its serve ratio — the proportion of served requests relative to expected requests:

$$\text{reward}_{\text{adjusted}} = \text{reward}_{\text{base}} \times \frac{\text{serves}}{\text{expected\_serves}}$$

A node that stores KUs but never responds to `FIND_VALUE` queries receives a diminished reward, approaching zero for persistent non-responsiveness.

### §6.7.2 SWAP-Like Credit System

Inspired by Swarm's SWAP protocol [4], we implement a bilateral credit system:

- Each `STORE` operation earns 1 credit with the storing node.
- Each `GET` operation costs 1 credit with the serving node.
- Nodes with credit balance below −100 are rate-limited on GET requests.

This creates a natural incentive for balanced participation without requiring global coordination.

### §6.7.3 Proof-of-Storage for Knowledge Units (PoS-KU)

The OBT token system (§8.5) requires nodes to periodically prove that they are actually storing the KUs they claim to hold. Three challenge types are employed:

1. **FullHash:** Recompute BLAKE3(wire_bytes) and return the digest. Cost: ~65 µs for 172 bytes.
2. **ByteRange:** Return bytes at a specified offset and length. Cost: O(1) memory access.
3. **FieldExtract:** Decode a specific opcode field from the Core DNA. Cost: O(n) where n is instruction count.

Challenge generation is deterministic via BLAKE3-seeded random selection, with approximately 10% of stored KUs challenged per epoch. The distribution is approximately 20% FullHash, 50% ByteRange, 30% FieldExtract. Challenge verification uses constant-time byte comparison to prevent timing side-channel attacks [12].

---

## §6.8 Summary

This chapter presented the distributed replication layer of OBS, addressing the transition from single-node persistence to network-wide durability. The tier-aware 4+2+1 placement strategy (§6.2) leverages the node hierarchy to anchor replicas at infrastructure-grade nodes while maintaining Kademlia's load distribution properties. The dual-layer consistency model (§6.4) eliminates coordination overhead for immutable Core DNA while employing five distinct CRDT types for mutable Epigenetics metadata. The stigmergy-driven repair mechanism (§6.5) provides autonomous, decentralised self-healing through pheromone-based coordination.

In the next chapter, we extend the storage layer to handle media and blob objects — a fundamentally different workload from the ultra-small KUs addressed here.

---

## References

[1] OneBrain Project Contributors, "Knowledge Unit: A Bio-Inspired Knowledge Representation with Core DNA Encoding for Decentralized Knowledge Networks," *OneBrain Technical Report*, 2026.

[2] J. Benet, "IPFS — Content Addressed, Versioned, P2P File System," *arXiv preprint arXiv:1407.3561*, 2014.

[3] Protocol Labs, "Filecoin: A Decentralized Storage Network," *Filecoin Whitepaper*, 2017.

[4] V. Trón, "The Book of Swarm: Storage and Communication Infrastructure for Self-Sovereign Digital Society," *Swarm Foundation*, 2020.

[5] I. S. Reed and G. Solomon, "Polynomial Codes over Certain Finite Fields," *SIAM Journal on Applied Mathematics*, vol. 8, no. 2, pp. 300–304, 1960.

[6] OneBrain Project Contributors, "OneBrain Protocol: A Bio-Inspired Peer-to-Peer Network for Decentralized Knowledge Sharing," *OneBrain Technical Report*, 2026.

[7] P. Maymounkov and D. Mazières, "Kademlia: A Peer-to-Peer Information System Based on the XOR Metric," in *Proc. 1st International Workshop on Peer-to-Peer Systems (IPTPS)*, 2002, pp. 53–65.

[8] S. Levin, "The Problem of Pattern and Scale in Ecology," *Ecology*, vol. 73, no. 6, pp. 1943–1967, 1992.

[9] M. Shapiro, N. Preguiça, C. Baquero, and M. Zawirski, "Conflict-Free Replicated Data Types," in *Proc. 13th International Symposium on Stabilization, Safety, and Security of Distributed Systems (SSS)*, 2011, pp. 386–400.

[10] P.-P. Grassé, "La reconstruction du nid et les coordinations interindividuelles chez *Bellicositermes natalensis* et *Cubitermes* sp. La théorie de la stigmergie," *Insectes Sociaux*, vol. 6, no. 1, pp. 41–80, 1959.

[11] A. Das Gupta, I. Stoica, R. Morris, and H. Balakrishnan, "SWIM: Scalable Weakly-Consistent Infection-Style Process Group Membership Protocol," in *Proc. IEEE/IFIP International Conference on Dependable Systems and Networks (DSN)*, 2002, pp. 303–312.

[12] D. J. Bernstein, "Constant-time comparison functions," *cr.yp.to*, 2010.
