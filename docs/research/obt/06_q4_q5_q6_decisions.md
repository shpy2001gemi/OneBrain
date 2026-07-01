# Quyết Định Q4, Q5, Q6 — Phân Tích & Kết Luận

> Phân tích kỹ thuật để đưa ra quyết định cho 3 câu hỏi Owner giao lại

---

## Q4: Account-Chain thay G-Counter cho balance?

### Vấn đề

OBT cần **chi tiêu được** (send/receive). G-Counter chỉ tăng (increment-only) → không thể trừ balance.

### 3 phương án

| # | Phương án | Ưu | Nhược |
|---|----------|-----|-------|
| A | **G-Counter** (giữ nguyên) | Đơn giản, đã có code | ❌ Không spend được — vô nghĩa cho tradeable token |
| B | **PNCounter** (tăng + giảm) | Đã có code trong crdt.rs | ❌ Cho phép overdraft — 2 node cùng trừ đồng thời = balance âm |
| C | **Account-Chain** (Nano model) | ✅ Spend an toàn, mỗi account 1 chain | Cần code mới (~400 LOC) |

### Phân tích chi tiết Phương án C (Account-Chain)

```
Mỗi node có chain riêng (giống Nano block-lattice):

Node Alice: [Open] → [Mint +100] → [Send -50 to Bob] → [Mint +20] → ...
Node Bob:   [Open] → [Receive +50 from Alice] → [Mint +30] → ...

Mỗi block ghi balance SAU operation → chỉ owner ký → không ai sửa được
Sequence number tăng dần → fork = phát hiện ngay
```

**Tại sao an toàn:**
- Chỉ owner ký block mới (Ed25519) → không ai tự ý trừ balance người khác
- Sequence tăng dần → double-spend = 2 blocks cùng sequence = fork = phát hiện
- Balance ghi rõ trong mỗi block → ai cũng verify được
- DHT neighbors validate mỗi block mới

**Tương thích với OBP:**
- Ed25519 ✅ (identity.rs)
- BLAKE3 hash ✅ (CID)
- DHT validation ✅ (dht.rs, k=20)
- VectorClock ✅ (crdt.rs)
- G-Counter vẫn dùng cho analytics (total_earned, total_spent, global supply)

### ✅ QUYẾT ĐỊNH: CHỌN ACCOUNT-CHAIN

**Lý do**: Đây là phương án duy nhất khả thi cho tradeable token. G-Counter và PNCounter đều có lỗ hổng toán học không thể khắc phục. Account-Chain đã được Nano chứng minh ở production scale (triệu users, 0 fee, <1s finality). Hoàn toàn tương thích với OBP primitives hiện có.

---

## Q5: Tombstone (ban vĩnh viễn) — có hay không?

### Vấn đề

Khi phát hiện fraud nghiêm trọng (ring leader, identity forgery, large-scale systematic attack) — có nên ban vĩnh viễn NodeID không?

### So sánh với các hệ thống lớn

| Hệ thống | Có permanent ban? | Cách thực hiện |
|----------|-------------------|---------------|
| Ethereum 2.0 | ✅ Forced exit — validator bị đuổi vĩnh viễn | Slashed + exit queue |
| Cosmos | ✅ Tombstone — validator không bao giờ rejoin với cùng key | Permanent jail |
| Helium | ✅ Permanent denylist — hotspot key blocked forever | Community governance |
| Ngân hàng | ✅ Blacklist — tài khoản bị đóng vĩnh viễn | KYC/AML regulations |
| Wikipedia | ✅ Indefinite block — editor bị cấm vĩnh viễn | Admin decision |

**Mọi hệ thống lớn đều có permanent ban.** Lý do: nếu không có, attacker biết rằng penalty tối đa chỉ là tạm thời → giảm chi phí tấn công.

### Phản biện: "Nhưng tạo key mới rất dễ?"

```
Đúng — Ed25519 key mới mất <1ms.
Nhưng trust mới = 0 (Leaf tier).

Chi phí THỰC SỰ khi bị Tombstone:
  → Mất key cũ (trust 0.8+, tier cao, months of work)
  → Key mới: trust = 0, tier = Leaf
  → S/Kademlia puzzle: BLAKE3 puzzle cost (crypto puzzle để tạo NodeId)
  → Mất hàng THÁNG để rebuild trust
  → Trong thời gian đó: earn cực thấp (Leaf = 10% reward)

Tổng chi phí: THÁNG công sức, không phải giây.
```

### Tombstone có cần cho tất cả fraud?

**KHÔNG.** Chỉ cho 2 loại fraud nghiêm trọng nhất:

| Loại | Tombstone? | Lý do |
|------|-----------|-------|
| Spam KU | ❌ Tier 2 (trust reduction) | Có thể do nhầm lẫn |
| Fake PoMV | ❌ Tier 2-3 (trust slash + jail) | Có thể do bug |
| Isolation attack (1 lần) | ❌ Tier 3 (jail) | Có thể learn from mistake |
| **Systematic collusion ring** | ✅ Tier 5 (Tombstone) | Chủ ý, có tổ chức, repeated |
| **Identity forgery** | ✅ Tier 5 (Tombstone) | Tấn công nền tảng trust |

### ✅ QUYẾT ĐỊNH: CÓ TOMBSTONE

**Lý do**: 
1. Mọi hệ thống production đều cần → đã được thực tế chứng minh
2. Chỉ áp dụng cho 2 loại fraud nghiêm trọng nhất (có appeal process 4 tầng)
3. Chi phí tạo lại identity = tháng (trust rebuild), không phải trivial
4. Không có Tombstone = attacker biết risk tối đa chỉ là temporary → khuyến khích tấn công

**Bảo vệ false positive**: Tombstone Appeal cần >80% top-tier nodes đồng ý + cryptographic evidence.

---

## Q6: Epoch = bao lâu?

### Vấn đề

Epoch là đơn vị thời gian cho: PoMV tick, reward distribution, storage challenge, rate limiting.

### Phân tích các lựa chọn

| Epoch | Ưu | Nhược | Phù hợp? |
|-------|-----|-------|----------|
| **1 phút** | Reward rất nhanh | Overhead lớn (60 challenge/giờ), gossip chưa kịp converge | ❌ |
| **10 phút** | Nhanh, overhead hợp lý | Bitcoin-like, có thể chưa đủ data cho PoMV | 🟡 |
| **1 giờ** | Đủ data cho PoMV, overhead thấp | Chờ hơi lâu cho reward đầu tiên | ✅ |
| **6 giờ** | PoMV rất chính xác | Quá chậm, user experience kém | 🟡 |
| **1 ngày** | Ít overhead nhất | Quá chậm, contributor phải chờ 24h cho reward | ❌ |

### Tại sao 1 giờ là tối ưu?

**1. Phù hợp với timing hiện có trong OBP:**

```
SWIM probe:          1 giây  (1 epoch = 3,600 probes → đủ data cho trust)
Gossip interval:     30 giây (1 epoch = 120 gossip rounds → convergence tốt)
Pheromone decay:     per hour (ĐÃ ĐÚNG 1 giờ!)
Encoding job TTL:    7 ngày  (168 epochs → hợp lý)
Encoding claim CD:   60 giây (60 claims/epoch max → hợp lý)
```

**2. Đủ data cho PoMV chính xác:**

```
1 giờ = 
  → Hàng nghìn metabolism events (G-Counter increments)
  → Nhiều prediction resolutions
  → Entropy đã ổn định
  → Synaptic bonds đã cập nhật
  → Niche fitness đã tính
  
→ PoMV score sau 1 giờ: đáng tin cậy
```

**3. User experience hợp lý:**

```
Contributor đăng KU:
  → Encoding consensus: ~5-30 phút (3 AI verify)
  → R2/R3 reward (encoding): mint ngay khi consensus xong
  → R1 reward (PoMV): nhận sau 1 epoch đầu tiên (≤ 1 giờ)
  → R4 reward (storage): nhận sau 1 epoch

Contributor thấy OBT trong tài khoản: < 1 giờ
→ Đủ nhanh để motivate, đủ chậm để chính xác
```

**4. Storage challenge overhead hợp lý:**

```
1 epoch/giờ = 24 challenges/ngày per node
Mỗi challenge: 5-10 KU × 3 types = 15-30 proofs
Bandwidth: ~15-30 KB/giờ → negligible
```

**5. So sánh với hệ thống khác:**

| Hệ thống | Epoch/Block time | Network size |
|----------|-----------------|-------------|
| Bitcoin | 10 phút | Global |
| Ethereum | 12 giây (slot), 6.4 phút (epoch) | Global |
| Filecoin | 30 giây | ~3,000 miners |
| Helium | 1 phút (block), epoch varies | ~350,000 hotspots |
| **OBT** | **1 giờ** | Expected: 100-10,000 nodes initially |

OBT network nhỏ hơn BTC/ETH rất nhiều ban đầu. 1 giờ cho phép đủ data converge mà không tạo quá nhiều overhead.

### ✅ QUYẾT ĐỊNH: EPOCH = 1 GIỜ (3,600 giây)

**Lý do**: Cân bằng tối ưu giữa:
- Tốc độ reward (< 1 giờ từ contribution đến OBT)
- Chất lượng PoMV (3,600 SWIM probes, 120 gossip rounds)
- Overhead (24 storage challenges/ngày = negligible)
- Compatibility (pheromone decay ĐÃ per-hour)

**Constant mới cho constants.rs:**
```rust
/// OBT epoch duration in seconds (1 hour).
pub const OBT_EPOCH_DURATION_S: u64 = 3_600;
```

---

## Tổng kết 3 Quyết Định

| Câu hỏi | Quyết định | Lý do chính |
|---------|-----------|-------------|
| **Q4**: Balance structure | ✅ **Account-Chain** | G-Counter bất khả thi cho spending. Nano model đã proven at scale |
| **Q5**: Permanent ban | ✅ **Có Tombstone** | Mọi system production cần. Chỉ cho 2 loại fraud nặng nhất. Appeal 4 tầng |
| **Q6**: Epoch duration | ✅ **1 giờ** | Cân bằng speed/quality/overhead. Compatible với pheromone decay hiện có |

> [!IMPORTANT]
> 3 quyết định này hoàn tất tất cả D1-D10. Bước tiếp theo: viết **OBT_SPEC.md** hoàn chỉnh.
