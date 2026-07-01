# OBP — OneBrain Protocol Specification

> **Version**: 2.0 — KU v6 Core DNA Compatible  
> **Status**: Implementation Complete (~4,700 LOC, 162 tests)  
> **Depends on**: KU_CORE_DNA_V6_SPEC.md, KU_ARCHITECTURE.md

## 1. Overview

OBP (OneBrain Protocol) is a **9-layer P2P network stack** for decentralized KU sharing. It handles transport, membership, discovery, routing, content addressing, pub/sub, and CRDT sync — while being **agnostic** to the KU wire format.

### 1.1 Architecture — 9 Layers

```
Layer 8: Sync          — Delta-state CRDT sync with VectorClock
Layer 7: PubSub        — Topic subscriptions, 128-bit Interest Vectors
Layer 6: Content       — Vacuum Bloom Filters (BLAKE3-based)
Layer 5: Stigmergy     — Bio-inspired pheromone routing (novel)
Layer 4: DHT           — S/Kademlia, 256 buckets, k=20, α=3, β=3
Layer 3: Discovery     — 6-layer cascade: Social→Local→HTTP→DHT→DNS→Hardcoded
Layer 2: Membership    — SWIM protocol, 7-tier node hierarchy, fitness scoring
Layer 1: Transport     — QUIC (quinn crate), 0-RTT/1-RTT, ALPN="obp/1"
Layer 0: Identity      — Ed25519 + BLAKE3 puzzle NodeId, DID format
```

### 1.2 Key Parameters

| Parameter | Value | Notes |
|-----------|-------|-------|
| Port | 4242 | Default, configurable |
| ALPN | `obp/1` | QUIC application protocol |
| NodeId | 32 bytes | Ed25519 public key |
| DID format | `did:key:z6Mk...` | Multibase-encoded |
| Puzzle difficulty | 16 bits | PoW for node registration |
| Kademlia k | 20 | Bucket size |
| Kademlia α | 3 | Parallelism factor |
| SWIM fanout | 5 | Membership gossip |
| Max message payload | 65,535 bytes | u16 BE in header |

## 2. KU Transport — v6 Core DNA

### 2.1 Frame Format

OBP transports KU as opaque bytes inside its message frame:

```
┌─────────────────────────────────────────────────────────┐
│ OBP Header (4B)                                         │
│ ┌─────────┬──────┬──────────────┐                       │
│ │msg_type │flags │payload_length│                       │
│ │ (1B)    │ (1B) │ (2B BE)      │                       │
│ └─────────┴──────┴──────────────┘                       │
├─────────────────────────────────────────────────────────┤
│ Payload: Core DNA wire bytes                            │
│ ┌──────┬────────┬────────────────┬─────┬───────┐        │
│ │MAGIC │VER_META│ INSTRUCTIONS   │ END │CRC-16 │        │
│ │0x4B  │ (1B)   │ (variable)     │0xF0 │ (2B)  │        │
│ └──────┴────────┴────────────────┴─────┴───────┘        │
├─────────────────────────────────────────────────────────┤
│ Ed25519 Signature (64B, optional)                       │
└─────────────────────────────────────────────────────────┘
```

**Key point**: OBP does NOT parse the KU payload. It treats it as opaque `Vec<u8>`. This means OBP is **completely agnostic** to whether the payload is Core DNA v6, CBOR v5, or any future format.

### 2.2 Message Header

```
Offset  Size  Field
0       1     msg_type (MessageType discriminant, u8)
1       1     flags
2       2     payload_length (u16 BE, max 65535)
```

**Flags byte layout:**

| Bits | Field | Values |
|------|-------|--------|
| 0-1 | Compression | 0=None, 1=PackedBinary, 2=PackedZstd, 3=Delta |
| 2 | dict_id | 0=default, 1=custom dictionary |
| 3 | fragmented | More fragments follow |
| 4 | 0-RTT safe | Can be sent in 0-RTT QUIC |
| 5-7 | reserved | Must be 0 |

> **v6 change**: Compression mode 1 renamed from `PackedCbor` to `PackedBinary` to reflect format-agnostic payload.

## 3. Message Types — 59 Types

### 3.1 Type Registry

| Range | Layer | Count | Description |
|-------|-------|-------|-------------|
| 0x01–0x0F | L0/L1: Core Transport | 13 | KuPush, KuPull, Gossip, Ping/Pong, Relay |
| 0x10–0x1C | L2: Membership (SWIM) | 13 | SwimPing/Ack/PingReq/Nack, SuperPeer ops |
| 0x20–0x26 | L3: DHT (Kademlia) | 7 | FindNode, FindValue, Store, HierLookup |
| 0x30–0x38 | L4: Content Routing | 9 | VacuumFilter, Pheromone, Topic PubSub, NDN |
| 0x40–0x52 | L5: Query/Trust/WATCH | 9 | WatchNotify, TrustGossip, Query Forward/Response |
| 0x60–0x68 | Cross-layer: Sync | 6 | CRDT Sync (Init/Delta/Ack/Complete), Mesh, Cache |
| 0x80–0x89 | Security | 8 | PoW, Backpressure, ProofOfStorage, Metabolism gossip |

### 3.2 KU-Carrying Messages

These messages carry KU bytes as payload:

| Type | ID | Payload |
|------|----|---------|
| `KuPush` | 0x01 | Single KU — Core DNA wire bytes |
| `KuPull` | 0x02 | Request by CID, response contains Core DNA bytes |
| `QueryResponse` | 0x51 | `results_payload: Vec<u8>` — concatenated Core DNA KUs |
| `WatchNotify` | 0x40 | `ku_payload: Vec<u8>` — single KU Core DNA bytes |
| `TopicPublish` | 0x35 | KU bytes for topic distribution |
| `TopicDeliver` | 0x36 | KU bytes delivered to subscriber |

> **v6 change**: Field names updated: `results_cbor` → `results_payload`, `ku_cbor` → `ku_payload`.

### 3.3 Metabolism Gossip Messages (PoK v2)

| Type | ID | Description |
|------|----|-------------|
| `MetabolismUpdate` | 0x86 | Push CRDT delta for KU metabolism counters |
| `MetabolismQuery` | 0x87 | Request metabolism data by CID(s) |
| `MetabolismResponse` | 0x89 | Response with `KUMetabolism` data |

Metabolism gossip carries `KUMetabolism` structs (8 G-Counters per KU), serialized via serde. **These are independent of KU wire format** — they track usage metadata, not knowledge content.

## 4. Layer Details

### 4.1 L0: Identity

- **Key pair**: Ed25519 (32-byte public, 64-byte secret)
- **NodeId**: `BLAKE3(Ed25519_pubkey)` — 32 bytes
- **DID format**: `did:key:z6Mk{multibase_encode(pubkey)}`
- **PoW puzzle**: 16-bit difficulty to prevent Sybil attacks

### 4.2 L1: Transport (QUIC)

- **Implementation**: `quinn` crate (real async QUIC)
- **TLS**: Self-signed certificates with Ed25519
- **ALPN**: `b"obp/1"`
- **Features**: 0-RTT reconnection, multiplexed streams, built-in congestion control
- **Port**: 4242 (configurable)

### 4.3 L2: Membership (SWIM)

- **Protocol**: SWIM (Scalable Weakly-consistent Infection-style Membership)
- **Messages**: Ping → Ack / PingReq → Ack / Nack
- **7-tier hierarchy**: Leaf → Branch → Relay → Hub → SuperPeer → RegionalBackbone → GlobalBackbone
- **Fitness scoring**: 7 weights (uptime, bandwidth, storage, CPU, reliability, diversity, trust)
- **Super-peer management**: Promotion, demotion, handoff, overload redirect

### 4.4 L3: Discovery — 6-Layer Cascade

| Priority | Method | Protocol | Scope |
|----------|--------|----------|-------|
| 1 | Social | Pre-configured peers | Bootstrap |
| 2 | Local | mDNS `_obp._udp.local` | LAN |
| 3 | HTTP | `/.well-known/obp-peers` | Domain |
| 4 | DHT | Kademlia iterative | Global |
| 5 | DNS | TXT records | Global |
| 6 | Hardcoded | Fallback seed nodes | Emergency |

### 4.5 L4: DHT (S/Kademlia)

- **Routing**: 256-bit key space, 256 buckets
- **Parameters**: k=20 (bucket size), α=3 (parallelism), β=3 (disjoint paths)
- **Lookup**: Iterative `FindNode` / `FindValue` with XOR distance
- **Storage**: CID → peer list (which peers have a given KU)

### 4.6 L5: Stigmergy (Novel)

Bio-inspired pheromone routing for semantic query optimization:
- **Deposit**: Successful queries deposit pheromone on (concept, peer) pairs
- **Evaporation**: Exponential decay over time
- **Reinforcement**: Repeated successes strengthen trails
- **Exploration**: Random exploration factor prevents local optima

### 4.7 L6: Content (Vacuum Bloom Filters)

- **Type**: Counting Bloom Filter with BLAKE3 hash functions
- **Purpose**: Efficiently advertise which CIDs a node holds
- **Exchange**: Periodic `VacuumFilter` / `VacuumExchange` messages
- **False positive rate**: Configurable (default ~1%)

### 4.8 L7: PubSub

- **Topics**: CID-based topic addressing
- **Interest Vectors**: 128-bit binary vectors for topic similarity
- **Messages**: Subscribe, Unsubscribe, Publish, Deliver
- **Delivery**: Best-effort via SWIM gossip overlay

### 4.9 L8: Sync (CRDT)

Delta-state CRDT synchronization:
- **Protocol**: Init → Delta → Ack → Complete (4-phase)
- **VectorClock**: Causal ordering for delta detection
- **Merge**: Lattice-based merge (join semi-lattice) for all CRDT types
- **Types synced**: TrustSection (LWWRegister), Metabolism (GCounter), Bonds (ORSet)

## 5. KU v6 Impact Assessment

### 5.1 What Changes

| Component | Change | Risk |
|-----------|--------|------|
| `results_cbor` field name | → `results_payload` | 🟢 Rename only |
| `ku_cbor` field name | → `ku_payload` | 🟢 Rename only |
| `PackedCbor` compression | → `PackedBinary` | 🟢 Rename only |
| Paper §3.12 wire diagram | Update to v6 Core DNA | 📄 Documentation only |
| Bandwidth estimates | 264B → 16-172B per KU | ✅ Positive — 16× reduction |

### 5.2 What Does NOT Change

| Component | Reason |
|-----------|--------|
| All 9 layers | No dependency on KU wire format |
| Identity (Ed25519 + NodeId) | Independent of KU |
| Transport (QUIC) | Carries opaque bytes |
| Membership (SWIM) | Node-level, not KU-level |
| DHT (Kademlia) | Uses CID `[u8;32]` — format-agnostic |
| Stigmergy (pheromone) | Uses ConceptIDs — still u64 |
| Vacuum Bloom Filters | Uses CID — format-agnostic |
| PubSub | Topic-based, not format-dependent |
| CRDT Sync | Syncs metadata, not KU content |
| Metabolism Gossip | Independent `KUMetabolism` struct |
| 59 message types | Only 2 field renames needed |

> **Conclusion**: OBP is architecturally isolated from KU wire format changes. The 9-layer design correctly treats KU as opaque payload.

## 6. Source Code

| Module | File | LOC | Tests |
|--------|------|-----|-------|
| Identity | `ku-net/src/identity.rs` | ~200 | 12 |
| Transport | `ku-net/src/transport.rs` | ~350 | 15 |
| Messages | `ku-net/src/messages.rs` | ~450 | 18 |
| Membership | `ku-net/src/membership.rs` | ~500 | 20 |
| Discovery | `ku-net/src/discovery.rs` | ~300 | 12 |
| DHT | `ku-net/src/dht.rs` | ~400 | 16 |
| Stigmergy | `ku-net/src/stigmergy.rs` | ~350 | 14 |
| Vacuum | `ku-net/src/vacuum.rs` | ~250 | 10 |
| PubSub | `ku-net/src/pubsub.rs` | ~300 | 12 |
| Sync | `ku-net/src/sync.rs` | ~350 | 14 |
| Metabolism Gossip | `ku-net/src/metabolism_gossip.rs` | ~325 | 6 |
| Distributed Query | `ku-net/src/query/` | ~3,300 | 74 |
| Constants | `ku-net/src/constants.rs` | ~100 | — |
| **Total** | | **~4,700** | **162** |
