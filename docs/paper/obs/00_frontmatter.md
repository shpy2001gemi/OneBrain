# OneBrain Storage: A Bio-Inspired, Content-Addressed, Tiered Storage Architecture for Decentralized Knowledge Networks

---

**Authors:** OneBrain Project Contributors

**Date:** July 2026

**Version:** 1.0

**Pillar:** P8 — Storage Layer

---

## Abstract

Existing decentralised storage systems — IPFS, Filecoin, Swarm, Storj, Arweave — are optimised for large files (256 KB to 32 GiB) and provide no semantic awareness of the objects they store. They treat all data as opaque byte sequences, ignoring the trust relationships, epistemic lifecycle, and biological activity patterns that characterise knowledge in cognitive architectures. This paper presents **OneBrain Storage (OBS)**, the storage layer (Pillar 8 of 10) of the OneBrain decentralised knowledge network, designed specifically for ultra-small Knowledge Units (KUs) of 16–172 bytes that carry rich semantic metadata.

We introduce seven novel contributions: (1) **Metabolism-Aware Adaptive Replacement Cache (M-ARC)**, the first cache replacement policy to use bio-inspired metabolic activity signals for eviction decisions within the ARC framework; (2) a **Dual-Layer Consistency Model** that combines zero-overhead immutability for content-addressed Core DNA with five-CRDT eventual consistency for mutable Epigenetics metadata; (3) **Tier-Aware Replica Placement (4+2+1)**, a hybrid strategy that distributes $R = 7$ replicas across XOR-closest, infrastructure-anchored, and subnet-diverse nodes; (4) **Content-Addressed Schema Migration**, a "never migrate wire bytes" framework that enables forward-compatible evolution without invalidating existing CIDs; (5) a **Six-Table Composite Key Graph Index** providing O(1) prefix-scan for six distinct query patterns; (6) **Stigmergy-Driven Replication Repair**, where digital pheromone trails autonomously trigger re-replication of under-replicated KUs; and (7) a **Unified Storage Abstraction** spanning 16 redb tables across 4 modules with centralised schema versioning.

The reference implementation comprises 6,021 lines of pure Rust across 9 modules with 125 tests and zero C dependencies. Evaluation demonstrates >300,000× write headroom above current workload, sub-microsecond hot-path reads via M-ARC, and 3.7–6.3× wire compression over UTF-8 text. To the best of our knowledge, OBS is the first content-addressed storage system to combine semantic metadata, bio-inspired caching, CRDT consistency, knowledge graph indexing, and tier-aware replication in a single architecture.

---

## Keywords

Content-Addressed Storage, Knowledge Unit, Bio-Inspired Computing, Cache Replacement Policy, ARC, CRDT, Distributed Replication, Schema Migration, Knowledge Graph, Decentralized Storage, Peer-to-Peer, Rust, redb, BLAKE3, Metabolism, Stigmergy, Tiered Storage

---

## Table of Contents

- **§1 [Introduction](./01_introduction.md)** — Motivation, problem statement, design principles, and seven contributions
- **§2 [Related Work](./02_related_work.md)** — Content-addressed storage, embedded KV stores, cache policies, knowledge graph storage, replication strategies, and schema evolution
- **§3 [Architecture Overview](./03_architecture.md)** — Design goals, three-tier model, content-addressing, dual-layer consistency, module organisation, and 16-table inventory
- **§4 [Local Storage and Schema Migration](./04_local_storage.md)** — Backend selection, KuStorage, GraphStorage 6-table design, schema versioning framework, concept dictionary
- **§5 [Metabolism-Aware Caching](./05_caching.md)** — Standard cache failures, ARC foundation, M-ARC algorithm, ghost list adaptation, prefetch, batch bypass, memory footprint
- **§6 [Distributed Replication](./06_replication.md)** — R=7 analysis, 4+2+1 placement, DHT persistence, CRDT consistency, stigmergy repair, health classification
- **§7 [Media and Blob Storage](./07_blob_storage.md)** — Architecture separation, chunking, OB-CID format, erasure coding, blob economics, streaming, external bridges
- **§8 [Cross-Pillar Integration](./08_cross_pillar.md)** — P1 KU Core, P2 OBP Network, P4 PoMV, P5 OBT Token, P7 OBKG integration with zero-modification composition
- **§9 [Evaluation](./09_evaluation.md)** — Implementation metrics, performance analysis, storage efficiency, test coverage, comparative assessment, limitations
- **§10 [Conclusion and Future Work](./10_conclusion.md)** — Seven contributions, six findings, six future directions

---

## Notation

| Symbol | Meaning |
|--------|---------|
| CID | Content Identifier = BLAKE3(wire_bytes), 32 bytes |
| KU | Knowledge Unit — atomic unit of knowledge in OneBrain |
| $R$ | Replication factor ($R = 7$) |
| $K$ | Kademlia routing parameter ($K = 20$) |
| $r_m$ | Metabolic rate of a KU |
| $p$ | ARC balancing parameter between T1 and T2 |
| $\phi$ | Replication pheromone strength |
| $w_s, w_r, w_d$ | Size, rarity, and demand weights (storage reward) |
| $f_t, f_\tau$ | Duration and trust factors (storage reward) |
| OBS | OneBrain Storage (Pillar 8) |
| OBP | OneBrain Protocol (Pillar 2) |
| OBT | OneBrain Token (Pillar 5) |
| OBKG | OneBrain Knowledge Graph (Pillar 7) |
| PoMV | Proof-of-Metabolic-Value (Pillar 4) |
| M-ARC | Metabolism-Aware Adaptive Replacement Cache |
| SEC | Strong Eventual Consistency |
| CRDT | Conflict-Free Replicated Data Type |

---

## Acknowledgements

This work builds upon the OneBrain pillar stack: Knowledge Unit (P1), OneBrain Protocol (P2), Knowledge Query Language (P3), Proof-of-Metabolic-Value (P4), OneBrain Token (P5), Immune System (P6), and OneBrain Knowledge Graph (P7). The storage architecture draws inspiration from the biological principles of metabolism, stigmergy, and epigenetics that underpin the OneBrain design philosophy.
