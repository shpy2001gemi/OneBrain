# OneBrain Protocol: A Bio-Inspired 9-Layer P2P Network Stack for Decentralized Knowledge Sharing

**Authors:** OneBrain Project Contributors  
**Contact:** shpy2001@gmail.com  
**Date:** June 2026  
**Version:** 1.0

---

## Abstract

Contemporary peer-to-peer (P2P) protocols — including IPFS/libp2p [1], BitTorrent [2], and Ethereum's devp2p [3] — were architecturally conceived for the distribution of opaque data objects: files, blocks, and transactions. None provides native support for *semantic query routing*, *fitness-based node hierarchies*, or *conflict-free knowledge metadata synchronization*. As a result, deploying a decentralized knowledge-sharing network atop existing P2P infrastructure requires bridging fundamental architectural mismatches between file-centric data models and the structured, queryable, and trust-annotated nature of human knowledge.

This paper presents the **OneBrain Protocol (OBP)**, a purpose-built 9-layer P2P network stack designed for decentralized knowledge sharing. The protocol comprises: **Layer 0 (Identity)** — Ed25519 keypairs with BLAKE3-based cryptographic puzzles for Sybil resistance; **Layer 1 (Transport)** — native QUIC (RFC 9000) with 0-RTT resumption and multiplexed bidirectional streams; **Layer 2 (Membership)** — an extended SWIM protocol augmented with a novel 7-tier node hierarchy (Leaf through GlobalBackbone) with automated fitness-based promotion and demotion across 7 weighted dimensions; **Layer 3 (Discovery)** — a 6-layer offline-first bootstrap cascade spanning social exchange (QR/NFC/BLE), local mDNS, HTTP well-known endpoints, DHT bootstrap, DNS TXT records, and hardcoded seeds; **Layer 4 (DHT)** — S/Kademlia routing with 256 k-buckets, k=20, and β=3 disjoint lookup paths for Byzantine resistance; **Layer 5 (Stigmergy)** — a bio-inspired pheromone routing system that reinforces successful knowledge query paths and evaporates unsuccessful ones, inspired by ant colony optimization; **Layer 6 (Content)** — Vacuum probabilistic filters (BLAKE3-based Bloom filters) for constant-size content capability summaries; **Layer 7 (PubSub)** — topic-based publish/subscribe with 128-bit interest vectors; and **Layer 8 (Sync)** — delta-state CRDT synchronization with VectorClock-based bidirectional exchange, achieving 10–100× bandwidth reduction over full-state replication.

The protocol is implemented in **~8,000 lines of Rust** across 30 core modules with **159 unit and integration tests** and **12 wire format test vectors**. The message system supports **74 distinct message types** (including 6 Encoding Consensus messages) encoded with a compact 6-byte universal header. Design targets include operation on mobile devices at **<0.5% battery per day**, offline-first networking without internet dependency, and scaling to **100 billion nodes** through hierarchical DHT routing achieving ~7 hops (~240ms) versus ~37 hops (~1.85s) for flat Kademlia at equivalent scale.

**Keywords:** Peer-to-peer networks, decentralized knowledge sharing, QUIC transport, Kademlia DHT, SWIM membership protocol, stigmergy, ant colony optimization, pheromone routing, Bloom filters, CRDT synchronization, node hierarchy, offline-first networking, bio-inspired computing, mobile P2P
