# §9 Constants Registry

> OBT Specification v1.0 — Complete Constants Reference
>
> Cross-refs: [§7 Trust & Security](./07_TRUST_SECURITY.md) · [§8 Penalty](./08_PENALTY.md) · [Research Synthesis](../../research/obt/05_research_synthesis.md)
>
> Mọi hằng số trong OBT spec được tập trung tại đây. Mỗi constant có: tên, giá trị, đơn vị, rationale, và source section.

---

## 9.1 Epoch & Timing

> Quyết định Q6: Epoch = 1 giờ — xem [06_q4_q5_q6_decisions.md](../../research/obt/06_q4_q5_q6_decisions.md)

| Constant | Value | Unit | Rationale | Source |
|----------|-------|------|-----------|--------|
| `OBT_EPOCH_DURATION_S` | **3,600** | seconds | 1 hour — compatible with pheromone decay, sufficient PoMV data (3,600 SWIM probes, 120 gossip rounds), reward < 1hr from contribution | [Q6 Decision](../../research/obt/06_q4_q5_q6_decisions.md) |
| `EPOCHS_PER_DAY` | **24** | epochs | 3,600s × 24 = 86,400s = 1 day | Derived |
| `EPOCHS_PER_WEEK` | **168** | epochs | 24 × 7 | Derived |
| `CONFIRMATION_TIMEOUT_S` | **30** | seconds | Max wait for transfer confirmation. Matches PoS-KU challenge timeout. Sufficient for DHT lookup + gossip propagation | [D9](../../research/obt/05_research_synthesis.md) |
| `ENCODING_JOB_TTL_EPOCHS` | **168** | epochs | 7 days — encoding jobs expire after 1 week if unclaimed | [ENCODING_CONSENSUS_SPEC](../ENCODING_CONSENSUS_SPEC.md) |

```rust
pub const OBT_EPOCH_DURATION_S: u64 = 3_600;
pub const EPOCHS_PER_DAY: u64 = 24;
pub const EPOCHS_PER_WEEK: u64 = 168;
pub const CONFIRMATION_TIMEOUT_S: u64 = 30;
pub const ENCODING_JOB_TTL_EPOCHS: u64 = 168;
```

---

## 9.2 Emission & Rewards

> Quyết định D1 — xem [research synthesis §D1](../../research/obt/05_research_synthesis.md)

### Global Emission

| Constant | Value | Unit | Rationale | Source |
|----------|-------|------|-----------|--------|
| `BASE_EMISSION_PER_EPOCH` | **10,000** | OBT | Governance-adjustable base. At 1,000 nodes × avg PoMV 0.7 = 7,000 OBT/epoch actual | [D1](../../research/obt/05_research_synthesis.md) |
| `ACTIVITY_MULTIPLIER_TARGET` | **1,000** | nodes | A(epoch) = min(active_nodes / 1,000, 10.0). Network scales emission with adoption | [D1](../../research/obt/05_research_synthesis.md) |
| `ACTIVITY_MULTIPLIER_MAX` | **10.0** | × | Cap on activity multiplier. At 10,000+ nodes, emission saturates at B × 10 | [D1](../../research/obt/05_research_synthesis.md) |

### Encoding Rewards (R2/R3)

| Constant | Value | Unit | Rationale | Source |
|----------|-------|------|-----------|--------|
| `BASE_OBT_PER_KB` | **1** | OBT/KB | 1 OBT per KB of raw text encoded. Linear scaling with knowledge size | [encoding_reward.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/encoding_reward.rs) |
| `FIRST_ENCODER_BONUS` | **5** | OBT | Bonus for the first AI to encode a KU. Incentivizes being first | [encoding_reward.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/encoding_reward.rs) |
| `PRO_BONO_BONUS` | **10** | OBT | Bonus for AI helping a node without its own AI (pro bono encoding) | [encoding_reward.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/encoding_reward.rs) |
| `CORRECTOR_MULTIPLIER` | **3** | × | Finding encoding errors = 3× base reward. Incentivizes quality checking | [encoding_reward.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/encoding_reward.rs) |

### Storage Rewards (R4)

| Constant | Value | Unit | Rationale | Source |
|----------|-------|------|-----------|--------|
| `STORAGE_BASE_RATE` | **0.001** | OBT/KU/epoch | Base storage reward per KU per epoch. Low because storage is passive | [D2](../../research/obt/05_research_synthesis.md) |
| `STORAGE_SIZE_WEIGHT_MIN` | **0.1** | × | Minimum size weight (KU 16 bytes = 0.1×) | [D2](../../research/obt/05_research_synthesis.md) |
| `STORAGE_SIZE_WEIGHT_MAX` | **10.0** | × | Maximum size weight (KU > 10KB capped) | [D2](../../research/obt/05_research_synthesis.md) |
| `STORAGE_RARITY_WEIGHT_MIN` | **0.5** | × | Min rarity weight (over-replicated KU) | [D2](../../research/obt/05_research_synthesis.md) |
| `STORAGE_RARITY_WEIGHT_MAX` | **3.0** | × | Max rarity weight (under-replicated KU = bonus) | [D2](../../research/obt/05_research_synthesis.md) |
| `STORAGE_DEMAND_WEIGHT_MIN` | **0.1** | × | Min demand weight (unused KU = near-zero reward) | [D2](../../research/obt/05_research_synthesis.md) |
| `STORAGE_DEMAND_WEIGHT_MAX` | **5.0** | × | Max demand weight (hot KU = 5× bonus) | [D2](../../research/obt/05_research_synthesis.md) |
| `STORAGE_DURATION_FACTOR_MAX` | **2.0** | × | Max duration factor (100+ epochs = 2× bonus) | [D2](../../research/obt/05_research_synthesis.md) |
| `STORAGE_MAX_REWARD_PER_NODE_EPOCH` | **10** | OBT | Per-node per-epoch cap for storage. Prevents domination | [D2](../../research/obt/05_research_synthesis.md) |

### PoMV Rewards (R1)

| Constant | Value | Unit | Rationale | Source |
|----------|-------|------|-----------|--------|
| `K_TARGET` | **20** | replicas | DHT replication factor. 20 replicas per KU in DHT | [dht.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/dht.rs) |

### Trust Multiplier (Per-Node Reward Cap)

| NodeTier | Trust Multiplier | Effective max reward | Promotion Threshold | Source |
|----------|-----------------|---------------------|---------------------|--------|
| Leaf (T0) | **0.1** (10%) | E(epoch) / nodes × 0.1 | 0.00 | [D1](../../research/obt/05_research_synthesis.md) |
| Contributor (T1) | **0.5** (50%) | E(epoch) / nodes × 0.5 | 0.30 | [D1](../../research/obt/05_research_synthesis.md) |
| LocalSP (T2) | **1.0** (100%) | E(epoch) / nodes × 1.0 | 0.60 | [D1](../../research/obt/05_research_synthesis.md) |
| RegionalSP (T3) | **1.5** (150%) | E(epoch) / nodes × 1.5 | 0.75 | [D1](../../research/obt/05_research_synthesis.md) |
| CountrySP (T4) | **2.0** (200%) | E(epoch) / nodes × 2.0 | 0.85 | [D1](../../research/obt/05_research_synthesis.md) |
| ContinentalSP (T5) | **3.0** (300%) | E(epoch) / nodes × 3.0 | 0.92 | [D1](../../research/obt/05_research_synthesis.md) |
| GlobalBackbone (T6) | **5.0** (500%) | E(epoch) / nodes × 5.0 | 0.97 | [D1](../../research/obt/05_research_synthesis.md) |

```rust
pub const BASE_EMISSION_PER_EPOCH: u64 = 10_000;
pub const ACTIVITY_MULTIPLIER_TARGET: u64 = 1_000;
pub const ACTIVITY_MULTIPLIER_MAX: f64 = 10.0;
pub const BASE_OBT_PER_KB: u64 = 1;
pub const FIRST_ENCODER_BONUS: u64 = 5;
pub const PRO_BONO_BONUS: u64 = 10;
pub const CORRECTOR_MULTIPLIER: u64 = 3;
pub const STORAGE_BASE_RATE: f64 = 0.001;
pub const STORAGE_MAX_REWARD_PER_NODE_EPOCH: u64 = 10;
pub const K_TARGET: u32 = 20;
```

---

## 9.3 Rate Limits

> Quyết định D3 — xem [anti-gaming research](../../research/obt/04_anti_gaming_research.md)

### KU Creation Rate Limits

| Constant | Leaf (T0) | Contributor (T1) | LocalSP+ (T2+) | Unit | Source |
|----------|-----------|-------------------|-----------------|------|--------|
| `MAX_KU_PER_HOUR` | **1** | **5** | **10** | KU/hour | [D3](../../research/obt/04_anti_gaming_research.md) |
| `MAX_ENCODINGS_PER_HOUR` | **2** | **5** | **10** | encodings/hour | [D3](../../research/obt/04_anti_gaming_research.md) |
| `ENCODING_CLAIM_COOLDOWN` | **60** | **12** | **6** | minutes | [D3](../../research/obt/04_anti_gaming_research.md) |

### Transfer Rate Limits

| Constant | Value | Unit | Rationale | Source |
|----------|-------|------|-----------|--------|
| `MAX_TRANSFERS_PER_EPOCH` | **100** | transfers | Anti-wash-trading. 100 transfers/hour sufficient for legitimate use | [D4](../../research/obt/05_research_synthesis.md) |
| `MIN_TRANSFER_AMOUNT` | **0.001** | OBT | Floor to prevent dust attacks (millions of micro-transfers) | [D4](../../research/obt/05_research_synthesis.md) |

### PoS-KU Challenge Limits

| Constant | Value | Unit | Rationale | Source |
|----------|-------|------|-----------|--------|
| `POS_KU_CHALLENGES_PER_EPOCH` | **5–10** | challenges | Random selection per node per epoch. Bandwidth ~15-30 KB/hr | [D2](../../research/obt/05_research_synthesis.md) |
| `POS_KU_RESPONSE_TIMEOUT_S` | **30** | seconds | Fast enough for disk read, too fast for network fetch from elsewhere | [D2](../../research/obt/05_research_synthesis.md) |
| `POS_KU_WITNESSES` | **3** | witnesses | K=3 DHT-selected witnesses verify challenge responses | [D2](../../research/obt/05_research_synthesis.md) |

```rust
// KU creation rate limits by tier
pub const MAX_KU_PER_HOUR_LEAF: u32 = 1;
pub const MAX_KU_PER_HOUR_CONTRIBUTOR: u32 = 5;
pub const MAX_KU_PER_HOUR_LOCALSP: u32 = 10;

pub const MAX_ENCODINGS_PER_HOUR_LEAF: u32 = 2;
pub const MAX_ENCODINGS_PER_HOUR_CONTRIBUTOR: u32 = 5;
pub const MAX_ENCODINGS_PER_HOUR_LOCALSP: u32 = 10;

pub const CLAIM_COOLDOWN_LEAF_MIN: u32 = 60;
pub const CLAIM_COOLDOWN_CONTRIBUTOR_MIN: u32 = 12;
pub const CLAIM_COOLDOWN_LOCALSP_MIN: u32 = 6;

// Transfer limits
pub const MAX_TRANSFERS_PER_EPOCH: u32 = 100;
pub const MIN_TRANSFER_AMOUNT: u64 = 1; // 0.001 OBT in milliunits

// PoS-KU
pub const POS_KU_RESPONSE_TIMEOUT_S: u64 = 30;
pub const POS_KU_WITNESSES: u32 = 3;
```

---

## 9.4 Quality Gates

> Quyết định D3 — xem [anti-gaming research](../../research/obt/04_anti_gaming_research.md)

| Constant | Value | Unit | Rationale | Source |
|----------|-------|------|-----------|--------|
| `MIN_KU_RAW_SIZE` | **256** | bytes | ~50 words minimum. KU must contain meaningful content | [D3](../../research/obt/04_anti_gaming_research.md) |
| `MIN_GENE_COUNT` | **2** | genes | At least 2 Knowledge DNA genes. Ensures structural complexity | [D3](../../research/obt/04_anti_gaming_research.md) |
| `MIN_ENCODING_VERIFY_COUNT` | **3** | verifiers | Encoding Consensus needs 3+ independent AI verifiers | [ENCODING_CONSENSUS_SPEC](../ENCODING_CONSENSUS_SPEC.md) |
| `MIN_POMV_7D` | **0.01** | score | PoMV ≥ 0.01 after 7 days. KU must show some metabolic activity | [D3](../../research/obt/04_anti_gaming_research.md) |
| `MIN_POMV_30D` | **0.05** | score | PoMV ≥ 0.05 after 30 days. Long-term viability check | [D3](../../research/obt/04_anti_gaming_research.md) |
| `MIN_ENCODING_TIME_MS` | **100** | milliseconds | Minimum encoding processing time. Prevents pre-computed spam | [D3](../../research/obt/04_anti_gaming_research.md) |
| `MIN_BOND_COUNT` | **1** | bonds | At least 1 synaptic bond (inter-KU connection) | [D3](../../research/obt/04_anti_gaming_research.md) |
| `KU_ENCODING_STATUS_REQUIRED` | **FULL** | status | Only FULL-encoded KUs earn storage rewards | [ENCODING_CONSENSUS_SPEC](../ENCODING_CONSENSUS_SPEC.md) |

```rust
pub const MIN_KU_RAW_SIZE: u32 = 256;
pub const MIN_GENE_COUNT: u32 = 2;
pub const MIN_ENCODING_VERIFY_COUNT: u32 = 3;
pub const MIN_POMV_7D: f32 = 0.01;
pub const MIN_POMV_30D: f32 = 0.05;
pub const MIN_ENCODING_TIME_MS: u64 = 100;
pub const MIN_BOND_COUNT: u32 = 1;
```

---

## 9.5 Trust & Security

> Xem [§7 Trust & Security](./07_TRUST_SECURITY.md) cho derivation và formulas

### Trust Decay (D6)

| Constant | Value | Unit | Rationale | Source |
|----------|-------|------|-----------|--------|
| `TRUST_DECAY_LAMBDA` | **0.01** | 1/hour | Half-life = ln(2)/0.01 ≈ 69.3h ≈ 3 days. Balanced between tolerating maintenance and detecting abandonment | [§7.1](./07_TRUST_SECURITY.md#71-trust-decay-formula-d6) |
| `TRUST_GRACE_PERIOD_HOURS` | **1.0** | hours | < 1 hour offline = no decay. Allows reboot/upgrade without penalty | [§7.1](./07_TRUST_SECURITY.md#71-trust-decay-formula-d6) |
| `TRUST_RECOVERY_MAX_PER_HOUR` | **0.05** | trust/hour | Max recovery rate. 0→1.0 takes 20h active. Recovery intentionally SLOWER than decay | [§7.1](./07_TRUST_SECURITY.md#71-trust-decay-formula-d6) |
| `TRUST_RECOVERY_INTERACTION_FACTOR` | **0.01** | trust/interaction | Raw recovery = interactions × 0.01, capped at 0.05/hr | [§7.1](./07_TRUST_SECURITY.md#71-trust-decay-formula-d6) |

### Gossip Gap Detection (D7)

| Constant | Value | Unit | Rationale | Source |
|----------|-------|------|-----------|--------|
| `GOSSIP_GAP_WINDOW_S` | **30** | seconds | Window to detect simultaneous offline events | [§7.2](./07_TRUST_SECURITY.md#72-gossip-gap-detection-d7) |
| `GOSSIP_GAP_ELEVATED_THRESHOLD` | **3** | nodes | ≥3 nodes offline in window = ELEVATED_SCRUTINY | [§7.2](./07_TRUST_SECURITY.md#72-gossip-gap-detection-d7) |
| `GOSSIP_GAP_RED_FLAG_THRESHOLD` | **5** | nodes | ≥5 nodes offline in window = RED_FLAG → manual review | [§7.2](./07_TRUST_SECURITY.md#72-gossip-gap-detection-d7) |
| `GOSSIP_GAP_WITNESS_MULTIPLIER` | **2** | × | Under ELEVATED_SCRUTINY, require 2× the normal witness count | [§7.2](./07_TRUST_SECURITY.md#72-gossip-gap-detection-d7) |
| `GOSSIP_GAP_SCRUTINY_MULTIPLIER` | **10** | × | ELEVATED_SCRUTINY duration = gap_duration × 10, max 24h | [§7.2](./07_TRUST_SECURITY.md#72-gossip-gap-detection-d7) |
| `GOSSIP_GAP_SCRUTINY_MAX_HOURS` | **24** | hours | Maximum ELEVATED_SCRUTINY duration | [§7.2](./07_TRUST_SECURITY.md#72-gossip-gap-detection-d7) |

### Connectivity Proof (D8)

| Constant | Value | Unit | Rationale | Source |
|----------|-------|------|-----------|--------|
| `CONNECTIVITY_PROOF_COUNT` | **3** | receipts | Minimum external gossip receipts required in MintProof | [§7.3](./07_TRUST_SECURITY.md#73-connectivity-proof-d8) |
| `CONNECTIVITY_PROOF_TTL_S` | **60** | seconds | Receipts must be < 60s old. Prevents using cached/stale receipts | [§7.3](./07_TRUST_SECURITY.md#73-connectivity-proof-d8) |

### Witness Selection

| Constant | Value | Unit | Rationale | Source |
|----------|-------|------|-----------|--------|
| `MIN_WITNESSES` | **3** | nodes | Minimum witnesses for any MintProof | [§7.4](./07_TRUST_SECURITY.md#74-five-anti-manipulation-mechanisms) |
| `MAX_WITNESSES` | **7** | nodes | Maximum witnesses. K = min(max(3, active_nodes/100), 7) | [§7.4](./07_TRUST_SECURITY.md#74-five-anti-manipulation-mechanisms) |

```rust
// Trust Decay
pub const TRUST_DECAY_LAMBDA: f64 = 0.01;
pub const TRUST_GRACE_PERIOD_HOURS: f64 = 1.0;
pub const TRUST_RECOVERY_MAX_PER_HOUR: f64 = 0.05;
pub const TRUST_RECOVERY_INTERACTION_FACTOR: f64 = 0.01;

// Gossip Gap
pub const GOSSIP_GAP_WINDOW_S: u64 = 30;
pub const GOSSIP_GAP_ELEVATED_THRESHOLD: u32 = 3;
pub const GOSSIP_GAP_RED_FLAG_THRESHOLD: u32 = 5;
pub const GOSSIP_GAP_WITNESS_MULTIPLIER: u32 = 2;
pub const GOSSIP_GAP_SCRUTINY_MULTIPLIER: u32 = 10;
pub const GOSSIP_GAP_SCRUTINY_MAX_HOURS: u32 = 24;

// Connectivity
pub const CONNECTIVITY_PROOF_COUNT: u32 = 3;
pub const CONNECTIVITY_PROOF_TTL_S: u64 = 60;

// Witnesses
pub const MIN_WITNESSES: u32 = 3;
pub const MAX_WITNESSES: u32 = 7;
```

---

## 9.6 Penalty System

> Xem [§8 Penalty](./08_PENALTY.md) cho full specification

### Tier Thresholds

| Constant | Value | Unit | Rationale | Source |
|----------|-------|------|-----------|--------|
| `TIER1_EXPIRY_DAYS` | **90** | days | Warning (yellow card) auto-expires after 90 days | [§8.2](./08_PENALTY.md#82-five-penalty-tiers-graduated-system) |
| `TIER1_TO_TIER2_COUNT` | **3** | warnings | 3 active Tier 1 warnings → automatic Tier 2 escalation | [§8.2](./08_PENALTY.md#82-five-penalty-tiers-graduated-system) |
| `TIER2_SEVERITY_FACTOR` | **0.3** | × | trust_new = trust × (1 - severity × 0.3). Max 30% loss per slash | [§8.2](./08_PENALTY.md#82-five-penalty-tiers-graduated-system) |
| `TIER2_TO_TIER3_COUNT` | **3** | offenses | 3 Tier 2 offenses → escalate to Tier 3 | [§8.2](./08_PENALTY.md#82-five-penalty-tiers-graduated-system) |
| `TIER3_SLASH_FACTOR` | **0.2** | × | trust_new = trust × 0.2 (80% slash) | [§8.2](./08_PENALTY.md#82-five-penalty-tiers-graduated-system) |
| `TIER3_JAIL_MIN_DAYS` | **7** | days | Minimum jail duration | [§8.2](./08_PENALTY.md#82-five-penalty-tiers-graduated-system) |
| `TIER3_JAIL_MAX_DAYS` | **30** | days | Maximum jail duration | [§8.2](./08_PENALTY.md#82-five-penalty-tiers-graduated-system) |
| `TIER3_TO_TIER4_COUNT` | **2** | offenses/year | 2 Tier 3 within 1 year → escalate to Tier 4 | [§8.2](./08_PENALTY.md#82-five-penalty-tiers-graduated-system) |
| `TIER4_TRUST_FLOOR` | **0.001** | — | Near-zero trust, but not permanently banned | [§8.2](./08_PENALTY.md#82-five-penalty-tiers-graduated-system) |
| `TIER4_BAN_DAYS` | **180** | days | 6-month ban, restart as Leaf after | [§8.2](./08_PENALTY.md#82-five-penalty-tiers-graduated-system) |
| `TIER5_TRUST` | **0.0** | — | Permanent zero trust | [§8.2](./08_PENALTY.md#82-five-penalty-tiers-graduated-system) |

### Correlation Penalty

| Constant | Value | Unit | Rationale | Source |
|----------|-------|------|-----------|--------|
| `CORRELATION_PENALTY_BASE` | **1.0** | — | multiplier = 1 + log₂(n). Base for isolated incident | [§8.3](./08_PENALTY.md#83-correlation-penalty-ethereum-inspired) |

### Appeal Windows

| Constant | Value | Unit | Rationale | Source |
|----------|-------|------|-----------|--------|
| `DISPUTE_WINDOW_HOURS` | **48** | hours | L2: Time for accused to submit counter-evidence before penalty executes | [§8.5](./08_PENALTY.md#85-appeal-process-4-layers) |
| `RETROSPECTIVE_WINDOW_DAYS` | **30** | days | L3: Time to file retrospective appeal after penalty execution | [§8.5](./08_PENALTY.md#85-appeal-process-4-layers) |
| `APPEAL_TRUST_SCAR` | **0.30** | fraction | Successful appeal restores trust × 0.7 (30% permanent scar) | [§8.5](./08_PENALTY.md#85-appeal-process-4-layers) |
| `TOMBSTONE_APPEAL_THRESHOLD` | **0.80** | fraction | L4: >80% of top-tier nodes must agree to review Tombstone | [§8.5](./08_PENALTY.md#85-appeal-process-4-layers) |
| `AUTO_PROTECTION_MIN_ANTIBODIES` | **2** | count | L1: Pre-penalty check needs ≥2 antibody types | [§8.5](./08_PENALTY.md#85-appeal-process-4-layers) |
| `AUTO_PROTECTION_MIN_CONFIDENCE` | **0.70** | fraction | L1: Combined antibody confidence must exceed 0.7 | [§8.5](./08_PENALTY.md#85-appeal-process-4-layers) |

```rust
// Tier thresholds
pub const TIER1_EXPIRY_DAYS: u32 = 90;
pub const TIER1_TO_TIER2_COUNT: u32 = 3;
pub const TIER2_SEVERITY_FACTOR: f64 = 0.3;
pub const TIER2_TO_TIER3_COUNT: u32 = 3;
pub const TIER3_SLASH_FACTOR: f64 = 0.2;
pub const TIER3_JAIL_MIN_DAYS: u32 = 7;
pub const TIER3_JAIL_MAX_DAYS: u32 = 30;
pub const TIER3_TO_TIER4_COUNT: u32 = 2;
pub const TIER4_TRUST_FLOOR: f64 = 0.001;
pub const TIER4_BAN_DAYS: u32 = 180;
pub const TIER5_TRUST: f64 = 0.0;

// Correlation
pub const CORRELATION_PENALTY_BASE: f64 = 1.0;

// Appeals
pub const DISPUTE_WINDOW_HOURS: u32 = 48;
pub const RETROSPECTIVE_WINDOW_DAYS: u32 = 30;
pub const APPEAL_TRUST_SCAR: f64 = 0.30;
pub const TOMBSTONE_APPEAL_THRESHOLD: f64 = 0.80;
pub const AUTO_PROTECTION_MIN_ANTIBODIES: u32 = 2;
pub const AUTO_PROTECTION_MIN_CONFIDENCE: f64 = 0.70;
```

---

## 9.7 Transfer & Wire Protocol

> Quyết định D4 — xem [research synthesis §D4](../../research/obt/05_research_synthesis.md)

### Message Type Codes

| Constant | Value | Description | Source |
|----------|-------|-------------|--------|
| `MSG_OBT_TRANSFER_REQUEST` | **0xA0** | Send OBT: from, to, amount, nonce, signature | [D4](../../research/obt/05_research_synthesis.md) |
| `MSG_OBT_TRANSFER_CONFIRM` | **0xA1** | Witness confirmation: tx_id, witness_signature | [D4](../../research/obt/05_research_synthesis.md) |
| `MSG_OBT_BALANCE_QUERY` | **0xA2** | Query balance of a node_id | [D4](../../research/obt/05_research_synthesis.md) |
| `MSG_OBT_BALANCE_RESPONSE` | **0xA3** | Response: node_id, balance, head_hash, Merkle proof | [D4](../../research/obt/05_research_synthesis.md) |
| `MSG_OBT_MINT_BROADCAST` | **0xA4** | Broadcast signed MintProof to network | [D4](../../research/obt/05_research_synthesis.md) |
| `MSG_OBT_STORAGE_CHALLENGE` | **0xA5** | PoS-KU challenge: ku_cid, challenge_type, params | [D4](../../research/obt/05_research_synthesis.md) |

### Account-Chain Constants

| Constant | Value | Unit | Rationale | Source |
|----------|-------|------|-----------|--------|
| `GENESIS_BLOCK_PREVIOUS` | **[0u8; 32]** | bytes | Genesis block has all-zero previous hash | [D4](../../research/obt/05_research_synthesis.md) |
| `DHT_ACCOUNT_STATE_K` | **20** | replicas | Account state replicated to K=20 DHT nodes (same as KU replication) | [D5](../../research/obt/05_research_synthesis.md) |
| `FORK_TIEBREAK` | **lower block_hash** | — | Deterministic: when fork detected, block with lower BLAKE3 hash wins | [D4](../../research/obt/05_research_synthesis.md) |

```rust
// Message types
pub const MSG_OBT_TRANSFER_REQUEST: u8 = 0xA0;
pub const MSG_OBT_TRANSFER_CONFIRM: u8 = 0xA1;
pub const MSG_OBT_BALANCE_QUERY: u8 = 0xA2;
pub const MSG_OBT_BALANCE_RESPONSE: u8 = 0xA3;
pub const MSG_OBT_MINT_BROADCAST: u8 = 0xA4;
pub const MSG_OBT_STORAGE_CHALLENGE: u8 = 0xA5;

// Account-Chain
pub const GENESIS_BLOCK_PREVIOUS: [u8; 32] = [0u8; 32];
pub const DHT_ACCOUNT_STATE_K: u32 = 20;
```

---

## 9.8 Gaming Pattern Detection

| Constant | Value | Unit | Rationale | Source |
|----------|-------|------|-----------|--------|
| `ISOLATION_PATTERN_WINDOW_S` | **30** | seconds | Same as GOSSIP_GAP_WINDOW_S. ≥3 nodes offline/online within 30s | [D3](../../research/obt/04_anti_gaming_research.md) |
| `BURST_SPAM_RATE_MULTIPLIER` | **2.0** | × | If rate > 2× tier limit → burst spam detected | [D3](../../research/obt/04_anti_gaming_research.md) |
| `BURST_SPAM_SIMILARITY_THRESHOLD` | **0.8** | score | KU content similarity > 0.8 between burst submissions = spam | [D3](../../research/obt/04_anti_gaming_research.md) |
| `CIRCULAR_TRANSFER_WINDOW_EPOCHS` | **1** | epoch | A→B→C→A within 1 epoch = wash trading suspected | [D3](../../research/obt/04_anti_gaming_research.md) |
| `LONG_CON_DIVERGENCE_THRESHOLD` | **0.3** | score | High trust but KU quality divergence > 0.3 = trust farming suspected | [D3](../../research/obt/04_anti_gaming_research.md) |

```rust
pub const ISOLATION_PATTERN_WINDOW_S: u64 = 30;
pub const BURST_SPAM_RATE_MULTIPLIER: f64 = 2.0;
pub const BURST_SPAM_SIMILARITY_THRESHOLD: f64 = 0.8;
pub const CIRCULAR_TRANSFER_WINDOW_EPOCHS: u64 = 1;
pub const LONG_CON_DIVERGENCE_THRESHOLD: f64 = 0.3;
```

---

## 9.9 Confirmation Levels

| Constant | Value | Description | Source |
|----------|-------|-------------|--------|
| `LEVEL_PENDING` | **0** | Just created, no confirmations | [OBT_DESIGN §Confirmation](../OBT_DESIGN.md) |
| `LEVEL_TENTATIVE` | **1** | 1–2 witnesses confirmed | [OBT_DESIGN §Confirmation](../OBT_DESIGN.md) |
| `LEVEL_CONFIRMED` | **2** | K witnesses confirmed (K=3–7) | [OBT_DESIGN §Confirmation](../OBT_DESIGN.md) |
| `LEVEL_SETTLED` | **3** | Widely propagated, practically irreversible | [OBT_DESIGN §Confirmation](../OBT_DESIGN.md) |
| `MIN_LEVEL_FOR_MINT` | **2** | Mint requires CONFIRMED+ | [OBT_DESIGN §Confirmation](../OBT_DESIGN.md) |
| `MIN_LEVEL_FOR_TRANSFER` | **2** | Transfer requires CONFIRMED+ | [OBT_DESIGN §Confirmation](../OBT_DESIGN.md) |

```rust
pub const LEVEL_PENDING: u8 = 0;
pub const LEVEL_TENTATIVE: u8 = 1;
pub const LEVEL_CONFIRMED: u8 = 2;
pub const LEVEL_SETTLED: u8 = 3;
pub const MIN_LEVEL_FOR_MINT: u8 = LEVEL_CONFIRMED;
pub const MIN_LEVEL_FOR_TRANSFER: u8 = LEVEL_CONFIRMED;
```

---

## 9.10 Full Constant Count

| Category | Count | Sections |
|----------|-------|----------|
| Epoch & Timing | 5 | §9.1 |
| Emission & Rewards | 16 | §9.2 |
| Rate Limits | 14 | §9.3 |
| Quality Gates | 8 | §9.4 |
| Trust & Security | 16 | §9.5 |
| Penalty System | 17 | §9.6 |
| Transfer & Wire | 9 | §9.7 |
| Gaming Detection | 5 | §9.8 |
| Confirmation Levels | 6 | §9.9 |
| **Total** | **96** | — |

> [!TIP]
> All constants are designed for **governance adjustability**. Initial values are based on research ([§research/obt/](../../research/obt/)) and can be tuned as the network grows. The only constants that SHOULD NOT change are cryptographic ones (key sizes, hash algorithms) and wire protocol codes (message types must remain stable for backward compatibility).
