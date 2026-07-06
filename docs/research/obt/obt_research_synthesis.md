# 🔬 OBT Research Synthesis — Kết Quả 4 Nhóm Nghiên Cứu

> 30/06/2026 | Tổng hợp để Owner review trước khi viết OBT_SPEC.md

---

## Phát Hiện Quan Trọng Nhất

> [!CAUTION]
> **G-Counter KHÔNG dùng được cho OBT balance!** G-Counter chỉ tăng (increment-only) — không thể chi tiêu (spend). PNCounter cũng không được vì cho phép overdraft. Cần chuyển sang **Account-Chain model** (mỗi node có chain riêng, giống Nano).

> [!IMPORTANT]
> **Triết lý OBT nhất quán**: OBT (earned) = KHÔNG clawback (non-punitive). Trust (reputation) = CÓ THỂ slash (punitive khi gian lận). Hai hệ thống tách biệt — "Không lấy lại lương cũ, nhưng tước bằng hành nghề."

---

## D1: Minting Rate — Giải pháp đề xuất

### Global Emission Formula (per epoch)

$$E(\text{epoch}) = B \times A(\text{epoch}) \times Q(\text{epoch})$$

| Tham số | Ý nghĩa | Giá trị đề xuất |
|---------|---------|-----------------|
| $B$ | Base emission | 10,000 OBT/epoch |
| $A$ | Activity multiplier | $\min(\frac{\text{active\_nodes}}{1000}, 10.0)$ |
| $Q$ | Quality factor | $\frac{\sum \text{PoMV}(ku)}{|\text{KU\_set}|}$ (avg PoMV score) |

**Ví dụ:**

| Network size | Avg PoMV | Emission/epoch |
|-------------|---------|---------------|
| 100 nodes (early) | 0.5 | 500 OBT |
| 1,000 nodes | 0.7 | 7,000 OBT |
| 10,000 nodes | 0.9 | 90,000 OBT |

**Per-node cap**: `max_node_reward = E(epoch) / active_nodes × TrustMultiplier`

| NodeTier | Trust Multiplier |
|----------|-----------------|
| Leaf | 0.1 (10%) |
| Contributor | 0.5 (50%) |
| LocalSP+ | 1.0 (100%) |

> **Reconciles with "near-infinite supply"**: Không có hard cap tổng (like Bitcoin 21M). Nhưng có flow control per epoch. Giống sông — không giới hạn tổng nước, nhưng có lưu lượng. Tri thức vô hạn, tốc độ ghi nhận có giới hạn.

---

## D2: Storage Reward — Giải pháp đề xuất

### Tham khảo: Sia (Merkle proof đơn giản) + Arweave (random recall)

Filecoin quá nặng (32GB sectors, GPU sealing) — không phù hợp KU nhỏ (16-172 bytes wire).

### Formula

```
storage_reward(node, epoch) = Σ per stored KU:
    base_rate × size_w × rarity_w × demand_w × duration_f × trust_f
```

| Yếu tố | Formula | Ý nghĩa |
|---------|---------|---------|
| `base_rate` | 0.001 OBT/KU/epoch | Hằng số cơ bản |
| `size_w` | clamp(wire_bytes/1024, 0.1, 10.0) | KU lớn → reward cao hơn |
| `rarity_w` | clamp(20/actual_replicas, 0.5, 3.0) | KU hiếm (ít replicas) → bonus |
| `demand_w` | clamp(metabolism/median, 0.1, 5.0) | KU hot (nhiều người dùng) → bonus |
| `duration_f` | min(epochs_stored/100, 2.0) | Lưu lâu → reward tăng dần |
| `trust_f` | eigentrust_score (0-1) | Trust cao → reward cao |

### Proof-of-Storage Protocol (PoS-KU)

```
Mỗi epoch:
  1. Challenge seed = BLAKE3(epoch || node_id) → deterministic
  2. Chọn 5-10 KU ngẫu nhiên từ stored set
  3. 3 loại challenge:
     Type A: "Trả BLAKE3 hash của KU X" → chứng minh có full KU
     Type B: "Trả bytes[offset..offset+len]" → chứng minh có content thật
     Type C: "Trả GeneType + ConceptID đầu tiên" → chứng minh decode được
  4. Phải trả lời trong 30 giây (đủ cho disk read, quá nhanh cho network fetch)
  5. K=3 witnesses verify (DHT-selected)
```

### Anti-gaming lưu trữ (5 lớp)

| Lớp | Cơ chế |
|-----|--------|
| 1 | `size_weight` floor — KU 16 bytes chỉ earn 1/10 vs KU 1KB |
| 2 | Challenge scales with KU count — store 10,000 KU vẫn bị challenge |
| 3 | Max 10 OBT/node/epoch cap — không ai dominate |
| 4 | KU phải FULL encoding status — spam KU không earn |
| 5 | `demand_weight` — KU không ai dùng → reward ≈ 0 |

---

## D3: Anti-gaming — Giải pháp đề xuất

### Rate Limits (Trust-gated, inspired by Nano buckets + IOTA DRR)

| NodeTier | Max KU/hr | Max Encode/hr | Cooldown |
|----------|----------|--------------|---------|
| Leaf | 1 | 2 | 60 min |
| Contributor | 5 | 5 | 12 min |
| LocalSP+ | 10 | 10 | 6 min |

### KU Quality Gates (4 tầng)

| Gate | Rule | Mục đích |
|------|------|---------|
| Min size | ≥ 256 bytes raw (~50 words), ≥ 2 genes | Chặn KU rỗng |
| Content validation | Encoding Consensus 3+ AI verify | Chặn nội dung rác |
| PoMV threshold | PoMV ≥ 0.01 sau 7 ngày, ≥ 0.05 sau 30 ngày | Chặn KU không ai dùng |
| Encoding complexity | Min 100ms encoding time, ≥ 1 bond | Chặn auto-generated spam |

### 4 Gaming Pattern Detection

| Pattern | Detection | Response |
|---------|----------|---------|
| **Isolation Attack** | ≥3 nodes cùng offline/online trong 30s | Elevated scrutiny, 2× witnesses required |
| **Burst Spam** | >2× tier rate, KU size near minimum | Warn → throttle → trust slash |
| **Circular Transfer** | A→B→C→A trong 1 epoch, same subnet | PoMV discounted by isolation factor |
| **Long Con** | High trust but low KU quality divergence >0.3 | Alert + audit |

---

## D4: Transfer Wire Format — Account-Chain Model

### Phát hiện quan trọng: CẦN ACCOUNT-CHAIN, KHÔNG DÙNG CRDT CHO BALANCE

```
G-Counter:   chỉ tăng → không spend được
PNCounter:   cho phép overdraft → không an toàn
Account-Chain (Nano model): ✅ MỖI NODE CÓ CHAIN RIÊNG
```

### Proposed Data Structure

```rust
/// Mỗi entry trong chain riêng của 1 account
pub struct TransferBlock {
    pub previous: [u8; 32],    // hash block trước (hoặc [0;32] cho genesis)
    pub account: [u8; 32],     // Ed25519 public key
    pub sequence: u64,         // tăng dần, không lặp
    pub balance: u64,          // BALANCE SAU operation này
    pub operation: TransferOp, // Mint | Send | Receive
    pub clock: VectorClock,    // causal ordering
    pub timestamp: u64,
    pub signature: [u8; 64],   // Ed25519 signature
    pub block_hash: [u8; 32],  // BLAKE3 hash of this block
}

pub enum TransferOp {
    Open,                                // Genesis block
    Mint { source: MintSource, amount: u64 },  // OBT từ reward
    Send { receiver: [u8; 32], amount: u64 },  // Gửi OBT
    Receive { send_block_hash: [u8; 32], amount: u64 }, // Nhận OBT
}
```

### Transfer Flow (2-phase, Nano-style)

```
Alice (balance=100) gửi 50 cho Bob:

ALICE:                              BOB:
1. Tạo Send block:                 4. Thấy pending Send (DHT/gossip)
   balance = 100 - 50 = 50         5. Tạo Receive block:
   op = Send{Bob, 50}                 balance = old + 50
2. Ký Ed25519                         op = Receive{send_hash, 50}
3. Broadcast → DHT                 6. Ký Ed25519
   → Neighbors verify:             7. Broadcast → DHT
     signature OK?                    → Neighbors verify:
     sequence = prev+1?               Send block exists?
     balance ≥ 0?                     Not already received?
```

### Double-spend Prevention

```
Alice cố gửi 80 cho Bob VÀ 80 cho Charlie (chỉ có 100):

→ Cả 2 Send block có sequence=5 cho cùng account
→ DHT neighbors phát hiện FORK (2 blocks cùng sequence)
→ Chấp nhận block THẤY TRƯỚC, reject block sau
→ Tiebreak: lower block_hash wins (deterministic)
→ Nếu cố tình → "warrant" (bằng chứng gian lận cryptographic)
```

### New Message Types (6 types, range 0xA0-0xA5)

| Code | Message | Payload |
|------|---------|---------|
| 0xA0 | ObtTransferRequest | from, to, amount, nonce, signature |
| 0xA1 | ObtTransferConfirm | tx_id, witness_signature |
| 0xA2 | ObtBalanceQuery | node_id |
| 0xA3 | ObtBalanceResponse | node_id, balance, head_hash, proof |
| 0xA4 | ObtMintBroadcast | mint_proof |
| 0xA5 | ObtStorageChallenge | ku_cid, challenge_type, params |

### CRDTs vẫn dùng cho (analytics + support)

| CRDT | Dùng cho |
|------|---------|
| G-Counter | `total_earned`, `total_spent` (analytics, chỉ tăng) |
| G-Counter | Global supply counter (tổng OBT đã mint) |
| ORSet | Pending/unreceived Send blocks |
| VectorClock | Causal ordering giữa accounts |
| LWWRegister | Account metadata |

---

## D5: Balance Storage

**Hybrid: Local + DHT**

| Layer | Lưu gì | Mục đích |
|-------|--------|---------|
| **Local (redb)** | Full account chain | Nhanh, luôn available |
| **DHT (k=20)** | AccountState (balance, head_hash, sequence) | Verify từ xa |
| **Merkle** | State root = hash(all AccountStates) | Global state summary |

---

## D6: Trust Decay Formula

$$\text{trust}(t) = \text{trust}_0 \times e^{-\lambda \times t_{\text{offline\_hours}}}$$

| λ | Half-life | Sau 1 ngày | Sau 1 tuần |
|---|-----------|-----------|-----------|
| 0.01 | ~69 giờ (~3 ngày) | -21% | -81% |

**Recovery**: `min(interaction_rate × 0.01, 0.05/hour)` — phục hồi CHẬM HƠN decay.

**Grace period**: < 1 giờ offline = no decay (cho phép restart, maintenance).

---

## D7-D8: Gossip Gap + Connectivity Proof

- **D7**: ≥3 nodes cùng offline trong 30s window → flag suspicious. Audit tất cả mint proofs.
- **D8**: Mint proof kèm ≥3 gossip receipts từ nodes NGOÀI witness set, timestamp < 60s.

---

## D9: Dispute Resolution

- Transfer conflict → block có nhiều witnesses thắng
- Timeout 30s cho confirmation → retry hoặc cancel
- Fork detection: 2 blocks cùng sequence → warrant → trust slash

---

## D10: Penalty System — 5 Tầng (Graduated)

### Nguyên tắc: "PoMV non-punitive cho NORMAL behavior. FRAUD bị phạt."

```
┌─────────────────────────────┐  ┌─────────────────────────────┐
│     PoMV (REWARDS)          │  │   FRAUD DEFENSE (PENALTIES) │
│                             │  │                             │
│  OBT = G-Counter            │  │  Trust = PN-Counter          │
│  Chỉ tăng, không clawback  │  │  Có thể giảm khi fraud     │
│  Earned = permanent         │  │  Earned = losable           │
│                             │  │                             │
│  Target: KNOWLEDGE (KUs)    │  │  Target: NODES (actors)     │
│  "Tri thức không bị phạt"   │  │  "Kẻ gian bị phạt"         │
└─────────────────────────────┘  └─────────────────────────────┘
```

### 5 Tầng Phạt

| Tầng | Tên | Trigger | Trust Formula | Duration |
|------|-----|---------|--------------|---------|
| **0** | Natural Decay | Low quality, offline, inactive | Trust tự decay (không active punishment) | Continuous |
| **1** | ⚠️ Warning | 1 antibody type detected, pattern lạ | Không giảm trust, chỉ flag | Expires 90 ngày |
| **2** | 🟡 Trust Reduction | ≥2 antibodies + conf>0.7, hoặc 3+ warnings | `trust × (1 - severity × 0.3)` (9-30% loss) | Permanent (phải earn lại) |
| **3** | 🔴 Jail | Collusion ≥3 nodes, isolation attack, 3+ Tier 2 | `trust × 0.2` (80% slash) + exclusion | 7-30 ngày |
| **4** | ⛔ Trust Zero | Proven fraud with economic gain, 2+ Tier 3/year | `trust = 0.001` + extended ban | 180 ngày, restart as Leaf |
| **5** | ☠️ Tombstone | Large-scale fraud, ring leader, identity forgery | `trust = 0` + permanent ban NodeID | **PERMANENT** |

### Correlation Penalty (Ethereum-inspired)

```
correlation_multiplier = 1 + log₂(simultaneous_nodes_penalized)

1 node:  ×1.0    4 nodes: ×3.0    16 nodes: ×5.0
```

→ Collusion 4 nodes = mỗi node nhận penalty ×3 → dễ bị đẩy lên Tier 4-5.

### Appeal Process (EigenLayer-inspired)

| Layer | Cơ chế | Áp dụng cho |
|-------|--------|------------|
| **Auto** | Quarantine cần ≥2 antibodies + conf>0.7 (giảm false positive) | Tier 2+ |
| **Dispute Window** | 48h trước khi slash, node trình counter-evidence | Tier 3+ |
| **Retrospective** | Appeal trong 30 ngày, K random high-trust nodes re-evaluate | Tier 3-4 |
| **Tombstone Appeal** | >80% top-tier nodes đồng ý + cryptographic evidence | Tier 5 only |

Nếu appeal thành công: `restored_trust = pre_penalty × 0.7` (30% permanent scar — chống lạm dụng appeal).

---

## Tổng Hợp D1-D10 Solutions

| D# | Vấn đề | Giải pháp | Tham khảo |
|----|--------|----------|-----------|
| **D1** | Minting rate | E = B × A × Q, trust-gated | Helium, IOTA mana |
| **D2** | Storage reward | 5-factor formula + PoS-KU challenge protocol | Sia + Arweave |
| **D3** | Anti-gaming | Rate limits + 4 quality gates + 4 pattern detectors | Nano buckets, IOTA DRR |
| **D4** | Transfer format | Account-Chain (Nano model) + 6 new message types | Nano block-lattice |
| **D5** | Balance storage | Hybrid local+DHT, Merkle state root | Nano + Holochain |
| **D6** | Trust decay | Exponential: e^(-0.01×t), recovery chậm hơn decay | EigenLayer |
| **D7** | Gossip gap | ≥3 nodes cùng offline 30s → elevated scrutiny | Custom |
| **D8** | Connectivity proof | ≥3 gossip receipts từ outside witnesses, <60s | Custom |
| **D9** | Dispute resolution | First-seen wins + warrant system + 30s timeout | Holochain |
| **D10** | Penalty system | 5 tầng graduated + correlation penalty + appeal 4 layers | Ethereum + Cosmos + EigenLayer |

---

## Open Questions cho Owner

> [!IMPORTANT]
> **Q4**: Account-Chain model (mỗi node có chain riêng) thay G-Counter cho balance — bạn thấy hướng này OK không? Đây là thay đổi architectural lớn nhất.

> [!IMPORTANT]
> **Q5**: Tombstone (ban vĩnh viễn) — bạn có đồng ý mức phạt cao nhất này? Node bị tombstone phải tạo identity mới hoàn toàn, bắt đầu lại từ đầu.

> [!IMPORTANT]
> **Q6**: Base emission B = 10,000 OBT/epoch — con số này có hợp lý không? 1 epoch = bao lâu trong thiết kế của bạn? (ví dụ: 1 giờ? 1 ngày?)
