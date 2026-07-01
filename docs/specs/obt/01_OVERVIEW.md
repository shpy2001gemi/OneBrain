# §1 Overview & Design Principles — OneBrain Token (OBT)

> **OBT_SPEC** · File 01 of 09 · Specification version: 1.0 · 30/06/2026

---

## 1.1 What is OBT? — Định Nghĩa

**OBT** (OneBrain Token) is the **native utility token** of the OneBrain Protocol (OBP).
OBT represents **knowledge value** — it is earned by nodes that contribute, encode,
verify, and store Knowledge Units (KUs) within the decentralized network.

### Core Identity

```
OBT ≠ Cryptocurrency            OBT = Knowledge Utility Token
OBT ≠ Speculation vehicle       OBT = Recognition of contribution
OBT ≠ Scarce commodity          OBT = Measure of knowledge work
```

| Property | Value |
|----------|-------|
| **Full name** | OneBrain Token |
| **Abbreviation** | OBT |
| **Ledger** | OBP-native Account-Chain (not blockchain) |
| **Supply model** | Near-infinite — flow-controlled, not hard-capped |
| **Value source** | Utility (PoMV usage), NOT scarcity |
| **Tradeability** | Yes — peer-to-peer transfer via Account-Chain |
| **Fees** | Zero — no gas, no transaction fees |
| **Consensus** | PoMV (Proof of Metabolic Value) — no PoW/PoS waste |
| **Smallest unit** | 1 OBT (integer, `u64`) |
| **Epoch** | 1 hour = 3,600 seconds (`OBT_EPOCH_DURATION_S`) |

### Analogy

> OBT is to knowledge what **kWh is to energy** — a unit of measure for work performed.
> Adding a new KU does not diminish the value of existing KUs,
> just as generating more electricity does not devalue the kWh already consumed.

---

## 1.2 Three Owner Principles — Ba Nguyên Tắc

The Owner of OneBrain established three non-negotiable principles (N1–N3)
that govern all OBT design decisions.

### N1: Tradeable + Secure + Fast + No Waste → OBP-native Ledger

> *"OBT phải trade được, bảo mật/minh bạch như blockchain, nhưng nhanh, không tốn phí tính toán vô ích."*

| Requirement | Solution |
|------------|----------|
| **Tradeable** | Account-Chain ledger: Send/Receive blocks (Nano-style) |
| **Secure** | Ed25519 signatures, BLAKE3 hashing, K/N threshold witnesses |
| **Fast** | Gossip propagation: 50–200 ms per hop, ~1–3 s full confirmation |
| **No waste** | No PoW mining, no GPU farms — minting is OUTPUT of consensus |

**Decision**: Build OBT ledger natively on OBP — no external blockchain required.
OBP already provides Ed25519, BLAKE3, DHT (k=20), VectorClock, delta-state CRDT,
and 6-byte wire header. See [§2 LEDGER](./02_LEDGER.md) for architecture.

### N2: Four Reward Streams

> *"Trả cho: KU owner (PoMV), encoder, verifier, storage provider — tỷ lệ với công việc."*

| Stream | Trigger | Formula Reference |
|--------|---------|-------------------|
| **R1 — KU Owner** | PoMV score evaluated each epoch | `reward = base_emission × PoMV(ku) / Σ PoMV(all)` |
| **R2 — Encoder** | AI encoding consensus completes | `base × multiplier + bonus` (role-based) |
| **R3 — Verifier** | AI verification of encoding succeeds | `base + (selected ? base/2 : 0)` |
| **R4 — Storage Provider** | Proof-of-Storage challenge passed | `base_rate × size_w × rarity_w × demand_w × duration_f × trust_f` |

R2 and R3 are implemented in `encoding_reward.rs` (209 LOC, 9 tests).
R1 formula exists in `pomv.rs`. R4 is specified in [§4 STORAGE_REWARD](./04_STORAGE_REWARD.md).

### N3: Near-Infinite Supply

> *"OBT thể hiện giá trị tri thức — không bị giới hạn, không bị ảnh hưởng bởi kinh tế/khu vực."*

| Traditional Crypto | OBT |
|--------------------|-----|
| Capped supply → halving → scarcity → value | Activity → minting → circulation → utility → value |
| 21M BTC hard cap | **No total cap** — flow-controlled per epoch |
| Pre-allocation (team, foundation, mining) | **No pre-allocation** — mint on-demand when value is created |
| Halving schedule | **No halving** — emission scales with network activity |
| Value = scarcity | **Value = knowledge utility** (PoMV network effect) |

**Mint events occur ONLY when real work is performed:**
- ✅ KU is used by the network (R1: PoMV reward)
- ✅ AI encoding completes successfully (R2: Encoding reward)
- ✅ AI verification succeeds (R3: Verifier reward)
- ✅ Node proves it stores KUs (R4: Storage reward)

**No minting for:**
- ❌ Wasteful computation (no PoW)
- ❌ Pre-allocation for team/foundation
- ❌ Airdrops or speculative distribution

---

## 1.3 Key Decisions Made — Các Quyết Định Đã Đưa Ra

Six design questions (Q1–Q6) were resolved through parallel research
across four research groups and Owner review. Full analysis in
[`docs/research/obt/`](../../research/obt/).

| # | Question | Decision | Rationale |
|---|----------|----------|-----------|
| **Q1** | Global emission cap per epoch? | **YES** — flow-controlled | `E(epoch) = B × A × Q` — scales with activity, bounded per epoch. No hard cap on total supply. Like a river: infinite water, controlled flow rate. |
| **Q2** | Trust-gated rewards? | **YES** — graduated by tier | Leaf = 10%, Contributor = 50%, LocalSP+ = 100% of max per-node reward. Prevents Sybil farming. |
| **Q3** | Fraud penalty? | **YES** — graduated 5-tier | Tier 0 (natural decay) → Tier 5 (tombstone). Trust is a PN-Counter — can decrease on fraud. |
| **Q4** | Balance data structure? | **Account-Chain** (Nano-style) | G-Counter cannot spend (increment-only). PN-Counter allows overdraft. Account-Chain: each node owns a chain of blocks, proven at production scale by Nano. |
| **Q5** | Permanent ban (Tombstone)? | **YES** — for worst fraud | Only for systematic collusion ring leaders and identity forgery. Appeal requires >80% top-tier node consensus. |
| **Q6** | Epoch duration? | **1 hour** (3,600 seconds) | Compatible with pheromone decay (already per-hour). User receives reward < 1 hour from contribution. 3,600 SWIM probes and 120 gossip rounds per epoch = reliable PoMV scores. |

> [!IMPORTANT]
> Q4 is the most significant architectural change — G-Counter (previously used for
> balance tracking per OBP_SPEC §6) is replaced by Account-Chain for OBT balances.
> G-Counters remain for analytics (`total_earned`, `total_spent`, global supply counter).
> See [research analysis](../../research/obt/06_q4_q5_q6_decisions.md) for full justification.

---

## 1.4 Spec File Map — Cấu Trúc Tài Liệu

The OBT specification is split into nine files. Each file covers one domain deeply.

| File | Title | Scope |
|------|-------|-------|
| **[01_OVERVIEW.md](./01_OVERVIEW.md)** | Overview & Design Principles | **This file** — identity, principles, decisions, philosophy |
| **[02_LEDGER.md](./02_LEDGER.md)** | Account-Chain Architecture | TransferBlock, TransferOp, fork detection, Merkle state root, hybrid storage (local redb + DHT k=20) |
| **[03_MINTING.md](./03_MINTING.md)** | Minting Model & 4 Reward Streams | Global emission formula `E = B × A × Q`, per-node caps, trust gating, R1–R4 detailed formulas, MintProof struct |
| **[04_STORAGE_REWARD.md](./04_STORAGE_REWARD.md)** | Storage Reward & Proof-of-Storage | R4 5-factor formula, PoS-KU challenge protocol (Type A/B/C), 30-second deadline, 5-layer anti-gaming |
| **[05_ANTI_GAMING.md](./05_ANTI_GAMING.md)** | Rate Limits, Quality Gates, Pattern Detection | Trust-gated rate limits, 4 KU quality gates, 4 gaming pattern detectors (isolation, burst, circular, long con) |
| **[06_TRANSFER.md](./06_TRANSFER.md)** | Wire Format, Message Types, Transfer Flow | 6 new message types (0xA0–0xA5), 2-phase send/receive, double-spend prevention, confirmation levels |
| **[07_TRUST_SECURITY.md](./07_TRUST_SECURITY.md)** | Trust Decay, Gossip Gap, Connectivity Proof | Exponential decay `e^(-0.01×t)`, 1-hour grace period, ≥3 gossip receipts, elevated scrutiny triggers |
| **[08_PENALTY.md](./08_PENALTY.md)** | 5-Tier Penalty, Correlation, Appeal Process | Tier 0–5 graduated penalties, correlation multiplier `1 + log₂(N)`, 4-layer appeal, tombstone criteria |
| **[09_CONSTANTS.md](./09_CONSTANTS.md)** | All Constants Registry | Every constant with value, type, rationale, and source file reference |

---

## 1.5 Philosophical Foundation — Triết Lý Nền Tảng

OBT design rests on a fundamental separation between **rewards** (OBT tokens)
and **reputation** (Trust scores):

```
┌─────────────────────────────────┐  ┌─────────────────────────────────┐
│       OBT (REWARDS)             │  │     TRUST (REPUTATION)          │
│                                 │  │                                 │
│  Structure: G-Counter           │  │  Structure: PN-Counter          │
│  Direction: increment-only      │  │  Direction: can decrease        │
│  Earned:    permanent            │  │  Earned:    losable on fraud    │
│  Target:    Knowledge (KUs)     │  │  Target:    Nodes (actors)      │
│                                 │  │                                 │
│  "Past salary is never revoked" │  │  "Medical license is revocable" │
└─────────────────────────────────┘  └─────────────────────────────────┘
```

### Four Axioms

| # | Axiom | Implication |
|---|-------|-------------|
| **A1** | **OBT earned is permanent** | G-Counter for `total_earned` — no clawback. A node that contributed genuine knowledge retains that record forever. Fraud is punished via Trust, not by revoking past OBT. |
| **A2** | **Trust is losable** | PN-Counter for trust scores — fraud causes trust decrease. Future earning capacity is reduced (trust-gated rewards), but past earnings remain. |
| **A3** | **Knowledge is free** | OBT rewards contribution, it does NOT gate access. No paywall. No "premium knowledge." Every KU is accessible to every node. |
| **A4** | **Value = utility, not scarcity** | OBT has no fixed supply cap. Value emerges from network effect: more valuable KUs → more users → more demand for OBT as recognition of contribution. |

> *"We don't take back past salary. We revoke your medical license."*
> — Design metaphor for OBT vs Trust separation

### Resolving Historical Contradictions

The Owner's three principles resolved six contradictions from earlier design documents:

| Contradiction | Resolution |
|---------------|------------|
| "Cryptocurrency" vs "Internal credits" | OBT is a **tradeable utility token** (N1), value = knowledge utility, not speculation |
| "Premium knowledge" vs "Knowledge is free" | **Knowledge is free** (N3). OBT = recognition/reward, not paywall |
| "Reviewers earn OBT" | **Verifiers earn OBT** (N2). In PoMV, verifier = encoding verifier, not human reviewer |
| 60% mining pre-allocation | **No pre-allocation** (N3). Mint on-demand when real activity occurs |
| Halving schedule needed? | **No halving** (N3). Supply is near-infinite, emission scales with activity |
| Token velocity problem | **Not applicable** — OBT does not need to "hold value" in the traditional crypto sense |

---

## 1.6 Comparison with Other Systems — So Sánh

| Dimension | **OBT** | **Bitcoin** | **Ethereum** | **Filecoin** | **Nano** |
|-----------|---------|-------------|--------------|-------------|----------|
| **Purpose** | Knowledge utility | Digital gold | Smart contract gas | Storage incentive | Digital cash |
| **Supply** | Near-infinite (flow-controlled) | 21M hard cap | ~120M + inflation | 2B hard cap | 133M fixed |
| **Consensus** | PoMV (observation) | PoW (mining) | PoS (staking) | PoSt + PoRep | ORV (voting) |
| **Fees** | **Zero** | Variable (sats/vB) | Gas (gwei) | Gas (attoFIL) | **Zero** |
| **Finality** | 1–3 s (Level 2) | ~60 min (6 blocks) | ~15 min (2 epochs) | ~30 s (tipset) | < 1 s |
| **Ledger** | Account-Chain | UTXO chain | Account-based | Tipset chain | Block-lattice |
| **Minting trigger** | Real work (encode/verify/store/use) | Hash puzzle | Block proposal | Storage proof | Genesis (pre-minted) |
| **Penalties** | 5-tier graduated + Tombstone | None (miners exit) | Slashing (validators) | Slashing (miners) | None |
| **Waste** | **Zero** | Massive (electricity) | Low (staking) | Moderate (sealing) | **Zero** |
| **Pre-allocation** | **None** | None | Presale 60M | 70% mining/30% team | Developer fund |
| **Value model** | Usage/utility | Scarcity | Utility + scarcity | Storage demand | Velocity |
| **Identity** | Ed25519 + EigenTrust tiers | Pseudonymous | Pseudonymous | Miner actors | Ed25519 accounts |
| **Anti-Sybil** | 7-tier hierarchy + trust decay | Hash difficulty | Stake requirement | Collateral | Balance-weighted voting |

> [!NOTE]
> OBT's Account-Chain model is architecturally closest to **Nano's block-lattice**
> (each account has its own chain, 2-phase send/receive, zero fees). The key difference:
> OBT has **continuous minting** triggered by network activity, whereas Nano's supply
> was fully pre-minted at genesis. OBT also adds trust-gated rewards and a penalty
> system — features absent in Nano.

---

## 1.7 Reference Documents — Tài Liệu Tham Chiếu

### Research Files (`docs/research/obt/`)

| File | Topic |
|------|-------|
| [`01_storage_reward_research.md`](../../research/obt/01_storage_reward_research.md) | Storage reward models (Filecoin, Sia, Arweave comparison) |
| [`02_penalty_slashing_research.md`](../../research/obt/02_penalty_slashing_research.md) | Penalty/slashing models (Ethereum, Cosmos, EigenLayer comparison) |
| [`03_crdt_ledger_research.md`](../../research/obt/03_crdt_ledger_research.md) | CRDT vs Account-Chain analysis (G-Counter limitations) |
| [`04_anti_gaming_research.md`](../../research/obt/04_anti_gaming_research.md) | Anti-gaming mechanisms (Nano buckets, IOTA DRR, Helium) |
| [`05_research_synthesis.md`](../../research/obt/05_research_synthesis.md) | D1–D10 solutions synthesis with formulas and data structures |
| [`06_q4_q5_q6_decisions.md`](../../research/obt/06_q4_q5_q6_decisions.md) | Q4 (Account-Chain), Q5 (Tombstone), Q6 (Epoch=1h) decisions |

### Design & Specification Files

| File | Relevance to OBT |
|------|-------------------|
| [`docs/specs/OBT_DESIGN.md`](../OBT_DESIGN.md) | Owner's 3 principles analysis, security deep dive, Q&A history |
| [`docs/specs/OBT_CURRENT_STATE.md`](../OBT_CURRENT_STATE.md) | Pre-spec audit: what existed, what was missing, contradictions |
| [`docs/specs/OBP_SPEC.md`](../OBP_SPEC.md) | OBP protocol spec — OBT ledger builds on OBP transport, DHT, gossip |
| [`docs/specs/ENCODING_CONSENSUS_SPEC.md`](../ENCODING_CONSENSUS_SPEC.md) | Encoding consensus protocol — triggers R2/R3 reward minting |

### Paper References

| File | Relevance to OBT |
|------|-------------------|
| [`docs/paper/pok/06_runtime_rewards.md`](../../paper/pok/06_runtime_rewards.md) | PoMV reward model — R1 (KU Owner) formula: `OBT = base_emission × PoMV(ku) / Σ PoMV(all)` |

### Source Code

| File | Component |
|------|-----------|
| `src/ku-core/src/pomv.rs` | PoMV scoring engine + `to_reward()` function |
| `src/ku-core/src/encoding_reward.rs` | R2/R3 encoding reward calculation (209 LOC, 9 tests) |
| `src/ku-core/src/crdt.rs` | G-Counter, PN-Counter, VectorClock (used for analytics) |
| `src/ku-core/src/eigentrust.rs` | EigenTrust — trust scoring for reward gating |
| `src/ku-net/src/dht.rs` | DHT (k=20) — stores AccountState replicas |
| `src/ku-net/src/constants.rs` | Protocol constants including `ENCODING_REWARD_BASE_OBT` |

---

## 1.8 Notation & Conventions — Ký Hiệu

Throughout all OBT spec files, the following conventions apply:

| Symbol | Meaning |
|--------|---------|
| `E(epoch)` | Global emission for one epoch (OBT minted network-wide) |
| `B` | Base emission constant (10,000 OBT/epoch) |
| `A(epoch)` | Activity multiplier = `min(active_nodes / 1000, 10.0)` |
| `Q(epoch)` | Quality factor = average PoMV score across all KUs |
| `K` | Witness threshold (typically 3–5) |
| `k` | DHT replication factor (20) |
| `λ` | Trust decay rate (0.01 per offline hour) |
| `[u8; 32]` | 32-byte array (BLAKE3 hash or Ed25519 public key) |
| `[u8; 64]` | 64-byte array (Ed25519 signature) |
| `u64` | Unsigned 64-bit integer |
| Tier 0–5 | Penalty tiers (0 = natural decay, 5 = tombstone) |
| Level 0–3 | Confirmation levels (0 = pending, 3 = settled) |

### Rust Pseudocode

Data structures throughout this spec use **Rust-style pseudocode**:

```rust
/// Example — all OBT spec structs follow this pattern
pub struct ExampleStruct {
    pub field_name: Type,   // inline comment explaining purpose
    pub hash: [u8; 32],     // BLAKE3 hash
    pub signature: [u8; 64], // Ed25519 signature
}
```

These are **specification-level definitions**, not compilable code.
Implementation may differ in field ordering, serialization, or naming.

---

> **Next**: [§2 LEDGER — Account-Chain Architecture](./02_LEDGER.md)
