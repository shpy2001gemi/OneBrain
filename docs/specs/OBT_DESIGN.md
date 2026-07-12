# 💰 OBT Token — Phân Tích 3 Nguyên Tắc Thiết Kế

> Owner directives + technical analysis | 30/06/2026
>
> *v7 note (2026-07-11): BlobStore introduces `StorageReward` for media hosting. ConceptRegistry hosting does not need separate incentive (bundled with node participation).*

---

## Nguyên tắc từ Owner

| # | Nguyên tắc | Tóm tắt |
|---|-----------|---------|
| **N1** | Tradeable + Bảo mật + Nhanh | OBT trade được, bảo mật/minh bạch như blockchain, nhưng nhanh, không tốn phí tính toán vô ích |
| **N2** | 4 luồng reward | Trả cho: KU owner (PoMV), encoder, verifier, storage provider — tỷ lệ với công việc |
| **N3** | Supply gần vô hạn | OBT = giá trị tri thức, không giới hạn, không bị ảnh hưởng bởi kinh tế/khu vực |

---

## Phân Tích Nguyên Tắc 1: Tradeable + Bảo mật + Nhanh + Không lãng phí

### Yêu cầu kỹ thuật

| Yêu cầu | Giải thích |
|----------|-----------|
| **Tradeable** | Node A có thể chuyển OBT cho Node B |
| **Bảo mật** | Không ai giả mạo balance, không double-spend |
| **Minh bạch** | Mọi giao dịch kiểm chứng được |
| **Nhanh** | Không cần đợi 10 phút như Bitcoin |
| **Không lãng phí** | Không PoW (đào coin vô ích), không cần GPU farm |

### So sánh các hướng kỹ thuật

| Hướng | Tốc độ | Bảo mật | Lãng phí? | Phù hợp? |
|-------|--------|---------|-----------|----------|
| PoW Blockchain (Bitcoin) | ❌ Chậm | ✅ Cao | ❌ Rất lãng phí | ❌ |
| PoS Blockchain (Ethereum) | 🟡 Trung bình | ✅ Cao | 🟡 Ít lãng phí | 🟡 |
| DAG (IOTA/Nano) | ✅ Nhanh | ✅ Cao | ✅ Không lãng phí | 🟡 Phức tạp |
| **OBP-native Ledger** | ✅ Nhanh | ✅ Cao | ✅ Không lãng phí | ✅ **Tối ưu** |

### Đề xuất: OBP-native Ledger

OneBrain đã có sẵn **tất cả primitives cần thiết** để xây token ledger mà không cần blockchain bên ngoài:

```
Đã có trong OBP:
├── Ed25519 signatures     → Xác thực giao dịch (identity.rs)
├── BLAKE3 hashing         → Transaction IDs, Merkle proofs
├── CRDT (G-Counter)       → Balance tracking conflict-free
├── VectorClock            → Causal ordering (ai trước ai sau)
├── DHT (Kademlia)         → Lưu trữ phân tán balance
├── Delta-state Sync       → Đồng bộ balance across nodes
└── 6-byte message header  → Wire format cho OBT transactions
```

**Tại sao OBP-native tốt hơn blockchain?**

| So sánh | Blockchain | OBP-native Ledger |
|---------|-----------|-------------------|
| Consensus | PoW/PoS (tốn tài nguyên) | **PoMV** — đã có sẵn, 0 waste |
| Finality | Blocks → chờ confirmations | **CRDT merge** → instant convergence |
| Double-spend | Mining + longest chain | **G-Counter** — chỉ tăng, không giả được |
| Storage | Mọi node lưu toàn bộ chain | **DHT** — phân tán, mỗi node lưu một phần |
| Speed | 10s → 10min per block | **Gossip** — ms-level propagation |
| Trust | Trustless (phải verify tất cả) | **EigenTrust** — node uy tín confirm nhanh hơn |

> [!IMPORTANT]
> **Kết luận N1**: Không cần blockchain bên ngoài. Xây OBT ledger trực tiếp trên OBP protocol, dùng G-Counter CRDT cho balance (increment-only = anti-fraud), Ed25519 cho signatures, DHT cho storage, gossip cho propagation. Nhanh, an toàn, không lãng phí.

---

## Phân Tích Nguyên Tắc 2: 4 Luồng Reward

### Mô hình reward hoàn chỉnh

```
                    ┌─────────────────────────────────┐
                    │      OBT MINTING ENGINE          │
                    │  (mint khi có giá trị được tạo)  │
                    └──────────┬──────────────────────┘
                               │
              ┌────────────────┼────────────────┐
              ▼                ▼                ▼                ▼
     ┌────────────┐   ┌────────────┐   ┌────────────┐   ┌────────────┐
     │ R1: OWNER  │   │ R2: ENCODE │   │ R3: VERIFY │   │ R4: STORE  │
     │            │   │            │   │            │   │            │
     │ KU được    │   │ AI encode  │   │ AI verify  │   │ Node lưu   │
     │ mạng sử    │   │ text→DNA   │   │ chất lượng │   │ KU trên    │
     │ dụng       │   │            │   │ encoding   │   │ disk/DHT   │
     │            │   │            │   │            │   │            │
     │ Tỷ lệ:    │   │ Tỷ lệ:    │   │ Tỷ lệ:    │   │ Tỷ lệ:    │
     │ PoMV score │   │ KU size    │   │ KU size    │   │ KU size ×  │
     │ × epoch    │   │ × role     │   │ × role     │   │ thời gian  │
     └────────────┘   └────────────┘   └────────────┘   └────────────┘
```

### Chi tiết từng luồng

#### R1: KU Owner Reward (ĐÃ THIẾT KẾ, ĐÃ CODE)

| Item | Giá trị |
|------|---------|
| **Trigger** | Mỗi epoch (định kỳ), tính PoMV score |
| **Formula** | `reward = pomv_score × max_reward_per_epoch` |
| **Đặc điểm** | Continuous — KU sống lâu, được dùng nhiều → earn nhiều |
| **Code** | [pomv.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/pomv.rs) `to_reward()` |
| **Paper** | PoK Paper §6.2 |
| **Status** | ✅ Formula + code có, ❌ `max_reward_per_epoch` chưa define |

#### R2: Encoder Reward (ĐÃ THIẾT KẾ, ĐÃ CODE)

| Item | Giá trị |
|------|---------|
| **Trigger** | Khi encoding hoàn tất (RAW→SELF, SELF→PART, PART→FULL) |
| **Formula** | `base × multiplier + bonus` (1 OBT/KB, multiplier by role) |
| **Code** | [encoding_reward.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/encoding_reward.rs) |
| **Status** | ✅ Hoàn chỉnh (209 LOC, 9 tests) |

#### R3: Verifier Reward (ĐÃ THIẾT KẾ, ĐÃ CODE)

| Item | Giá trị |
|------|---------|
| **Trigger** | Khi verify encoding thành công |
| **Formula** | `base + (selected ? base/2 : 0)` |
| **Code** | [encoding_reward.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/encoding_reward.rs) — role `Verifier` |
| **Status** | ✅ Hoàn chỉnh (cùng file R2) |

#### R4: Storage Provider Reward (CHƯA THIẾT KẾ)

| Item | Giá trị |
|------|---------|
| **Trigger** | Định kỳ, dựa trên dung lượng KU đang lưu trữ |
| **Formula** | **Cần thiết kế** — đề xuất: `OBT = Σ(KU_size × duration × replication_factor)` |
| **Tham chiếu** | Tương tự Filecoin storage rewards nhưng đơn giản hơn |
| **Chống gian lận** | Proof-of-Storage: node phải chứng minh đang giữ KU (random challenge) |
| **Status** | ❌ Chưa có code lẫn thiết kế |

> [!NOTE]
> **R2 và R3 đã merge trong `encoding_reward.rs`** — cùng file, cùng flow, khác role. R1 đã có formula. **Chỉ R4 (Storage) là hoàn toàn mới.**

#### OBKG Graph Contribution (via obkg_rewards.rs)

`GraphContributionScore` enriches PoMV-based rewards (R1) with 4 graph dimensions:
- **Bond Richness** (0.35) — active bond count + weight quality
- **Dream Contribution** (0.25) — bonds reinforced + associations discovered
- **FedR Participation** (0.20) — federated training epochs contributed
- **Graph Health** (0.20) — active/total bond ratio

This does NOT create a new reward stream — it provides an additional quality signal for existing rewards.

> [!NOTE]
> `obkg_rewards.rs` acts as a **cross-pillar bridge module** between OBKG (Knowledge Graph) and OBT (Token Economics). It lives in `ku-core` alongside other reward modules.

---

## Phân Tích Nguyên Tắc 3: Supply Gần Vô Hạn

### Ý nghĩa triết học

> *"OBT thể hiện giá trị tri thức — không bị giới hạn, không bị ảnh hưởng bởi kinh tế/khu vực"*

Đây là nguyên tắc **rất khác** so với cryptocurrency truyền thống:

| | Bitcoin/ETH | OBT (Owner's vision) |
|-|-------------|---------------------|
| Supply | Giới hạn (21M BTC) | **Gần vô hạn** |
| Giá trị đến từ | Scarcity (khan hiếm) | **Utility (tri thức)** |
| Analogy | Vàng | **Năng lượng / Calories** |
| Inflation concern | Có (phải halving) | **Không** — tri thức không lạm phát |
| Regional variance | Có (1 BTC ≠ cùng giá trị khắp nơi) | **Không** — 1 OBT = 1 đơn vị tri thức |

### Tại sao supply vô hạn hợp lý cho tri thức?

1. **Tri thức là vô hạn** — Nhân loại sẽ không bao giờ "hết" tri thức để đóng góp
2. **Không có lạm phát tri thức** — Thêm 1 KU mới không làm giảm giá trị KU cũ (khác với in tiền)
3. **OBT = đơn vị đo, không phải tài sản** — Giống kWh (kilowatt-hour) đo năng lượng, OBT đo giá trị tri thức
4. **Giải quyết "Premium Knowledge" paradox** — Nếu OBT vô hạn, không cần paywall

### Ảnh hưởng tới thiết kế

| Hạng mục | Thiết kế cũ (README) | Thiết kế mới (Owner) |
|----------|---------------------|---------------------|
| Total supply | Capped (implied) | **Uncapped** |
| Halving | Bitcoin-style halving | **Không cần** — mint tỷ lệ với hoạt động |
| Distribution 60/15/15/10 | Pre-allocation | **Bỏ** — mint on-demand khi có giá trị |
| "Trade on exchanges" | Speculation-driven | **Utility-driven** — trade nhưng value = knowledge utility |
| Premium knowledge | Paywall | **Bỏ paywall** — knowledge is free, OBT = recognition |
| Scarcity | Drives value | **Irrelevant** — value = how much knowledge you contributed |

### Mô hình minting mới

```
Traditional crypto:     Capped supply → Halving → Scarcity → Value
OBT model:              Activity → Minting → Circulation → Utility → Value

Mint khi:
  ✅ KU được mạng sử dụng (R1: PoMV reward)
  ✅ AI encode thành công (R2: Encoding reward)
  ✅ AI verify thành công (R3: Verifier reward)
  ✅ Node lưu trữ KU (R4: Storage reward)

Không mint khi:
  ❌ "Mining" vô nghĩa (no PoW)
  ❌ Pre-allocation cho team/foundation
  ❌ Airdrop/speculation
```

> [!IMPORTANT]
> **Supply vô hạn + minting on-demand = cần cơ chế ổn định giá trị.**
> OBT value không đến từ scarcity mà từ **network effect**: càng nhiều KU có giá trị → càng nhiều người dùng → càng nhiều demand cho OBT.

---

## Giải Quyết Mâu Thuẫn

3 nguyên tắc của Owner giải quyết hầu hết mâu thuẫn đã phát hiện:

| Mâu thuẫn cũ | Giải quyết |
|-------------|-----------|
| "Crypto" vs "Internal credits" | ✅ **OBT là tradeable token** (N1), nhưng value = knowledge utility, không phải speculation |
| "Premium" vs "Free" | ✅ **Knowledge is free** (N3 — supply vô hạn). OBT = recognition/reward, không phải paywall |
| "Reviewers earn OBT" | ✅ **Verifiers earn OBT** (N2) — trong PoMV, verifier = encoding verifier, không phải human reviewer |
| 60% mining allocation | ✅ **Bỏ pre-allocation** (N3) — mint on-demand khi có hoạt động thực |
| Halving needed? | ✅ **Không cần halving** (N3) — supply vô hạn, mint tỷ lệ với activity |
| Token velocity problem | ✅ **Không áp dụng** — OBT không cần "hold value" theo nghĩa crypto truyền thống |

---

## Những Quyết Định Kỹ Thuật Cần Đưa Ra Tiếp

### Đã rõ ràng (từ 3 nguyên tắc)

- [x] OBT tradeable — ✅
- [x] Supply uncapped — ✅ 
- [x] 4 reward streams — ✅
- [x] No wasteful computation — ✅
- [x] Knowledge is free (no paywall) — ✅
- [x] OBP-native (không cần blockchain ngoài) — ✅

### Cần thiết kế tiếp

| # | Quyết định | Câu hỏi |
|---|-----------|---------|
| D1 | **Minting rate** | Bao nhiêu OBT/epoch cho mỗi luồng? R1 (owner) vs R2 (encoder) vs R3 (verifier) vs R4 (storage) — tỷ lệ nào? |
| D2 | **Storage reward formula** | R4 chưa thiết kế. `OBT = size × duration × factor` — factor bao nhiêu? |
| D3 | **Anti-gaming** | Supply vô hạn → có thể spam KU nhỏ để farm OBT. Cần threshold tối thiểu? |
| D4 | **Transfer protocol** | Wire format cho OBT transfer message (mới, cần thêm vào 74 message types) |
| D5 | **Balance storage** | Lưu balance ở đâu? Local ledger? DHT? Cả hai? |
| D6 | **Transaction history** | Giữ bao lâu? Merkle tree để verify? |
| D7 | **Dispute resolution** | Node A nói đã transfer, Node B nói chưa nhận — giải quyết thế nào? |

---

## Phân Tích Bảo Mật OBP-native Ledger (Deep Dive)

> Kết quả thảo luận Owner ↔ Agent, 30/06/2026

### Tại sao decentralized hoàn toàn?

OBP-native Ledger **không có center nào**:

| Thành phần | Cách phân tán | Code đã có |
|-----------|---------------|-----------|
| **Balance** | G-Counter CRDT — mỗi node giữ counter riêng, merge tự động | [crdt.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/crdt.rs) |
| **Transaction history** | DHT — chia nhỏ, mỗi nhóm node lưu 1 phần (k=20 replicas) | [dht.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/dht.rs) |
| **Minting** | Output của consensus — không ai "in tiền" | [encoding_consensus.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/encoding_consensus.rs) |
| **Verification** | Mọi node đều verify được (formula deterministic) | [encoding_reward.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/encoding_reward.rs) |
| **Trust** | Mỗi node tính trust riêng (EigenTrust) | [eigentrust.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/eigentrust.rs) |

Không có "ngân hàng trung ương", admin, hay node đặc biệt kiểm soát supply.

---

### Chống 5 loại thao túng

#### 1. Double-spend (tiêu 1 OBT 2 lần)

**Cơ chế**: VectorClock + causal ordering

- VectorClock phát hiện 2 transactions từ cùng 1 counter state
- Network chỉ chấp nhận transaction ĐẦU TIÊN theo causal order
- Transaction thứ 2 bị reject vì insufficient balance
- Code: [sync.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/sync.rs)

#### 2. Balance forgery (tự sửa balance)

**Cơ chế**: Ed25519 signatures + multi-witness

- Balance = Σ(tất cả mint events + transfer events) có chữ ký của NHIỀU nodes
- Node tự khai "tôi có 1 triệu OBT" → vô nghĩa vì không có mint events có chữ ký network
- Mỗi thay đổi balance cần **bằng chứng có chữ ký** (threshold K/N witnesses)

#### 3. Sybil attack (tạo 1000 node giả để farm)

**Cơ chế**: EigenTrust + 7-tier Node Hierarchy (ĐÃ CÓ)

- Node mới = Leaf, trust score ≈ 0 → reward rất thấp
- Phải hoạt động thật sự, lâu dài mới lên tier cao
- Fitness scoring: uptime, bandwidth, storage, latency, availability, reliability
- 1000 node giả trust = 0 → không earn gì đáng kể
- Code: [eigentrust.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/eigentrust.rs) + [membership.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/membership.rs)

#### 4. Replay attack (phát lại transaction cũ)

**Cơ chế**: Transaction nonce + VectorClock

- Mỗi transaction có: sender_id + nonce (tăng dần) + vector_clock + signature
- Phát lại transaction cũ → nonce trùng → bị reject

#### 5. Collusion (nhiều node thông đồng)

**Cơ chế**: Threshold signing + BLAKE3-deterministic witness selection

- Mint event cần K chữ ký từ witnesses
- Witnesses chọn qua DHT dựa trên CID (BLAKE3 hash) — deterministic nhưng không kiểm soát được
- Attacker muốn chọn witnesses = phải thay đổi content = KU khác = CID khác
- Phải kiểm soát > 50% mạng mới collusion được → rất đắt

---

### Deterministic Minting — OBT chỉ sinh ra từ hoạt động thật

> **Nguyên tắc cốt lõi: OBT minting là OUTPUT của consensus, không phải INPUT.**

Không ai "yêu cầu" mint OBT. OBT tự sinh ra khi consensus hoàn tất:

```
Luồng R2 (Encoding):
  1. KU raw text xuất hiện trên mạng
  2. Encoding Consensus chạy (3+ AI verify)
  3. Consensus đạt FULL status
  4. → OBT reward được TÍNH (deterministic formula)
  5. → Tất cả witness nodes ký xác nhận
  6. → Mint event broadcast qua gossip
  7. Mọi node verify: cùng input → cùng OBT amount
```

**Tại sao không ai nhét OBT giả được:**

| Bước tấn công | Tại sao thất bại |
|---------------|-----------------|
| Tự mint OBT không qua consensus | Mint event cần K signatures từ witnesses ngẫu nhiên |
| Tạo KU giả để trigger mint | KU phải qua Encoding Consensus (3+ AI verify nội dung thật) |
| Tạo KU copy để farm | BLAKE3 CID = content hash → KU trùng = cùng CID → chỉ mint 1 lần |
| Fake encoding consensus | Witnesses chọn ngẫu nhiên qua DHT, phải verify round-trip |
| Fake PoMV usage | Metabolism dùng G-Counter — nhiều node độc lập ghi nhận |
| Fake storage proof | Random challenge: network hỏi "byte thứ N của KU X là gì?" |

**Mỗi OBT khi sinh ra đều có bằng chứng gốc:**

```rust
struct MintProof {
    activity: MintActivity,        // Encode | Verify | PoMV | Storage
    ku_cid: [u8; 32],             // BLAKE3 hash của KU liên quan
    obt_amount: u64,              // Deterministic — ai cũng tính ra cùng số
    formula_inputs: FormulaInputs, // raw_size, role, pomv_score...
    witnesses: Vec<WitnessSignature>, // K/N threshold
    vector_clock: VectorClock,    // Causal ordering, không replay được
    timestamp: u64,
}
```

---

### Trust System Deep Dive

#### Trust lưu ở đâu?

**Phân tán — mỗi node giữ bảng trust riêng:**

```
Node Y lưu:  trust_table = { A: 0.82, B: 0.45, C: 0.91, X: ??? }
Node Z lưu:  trust_table = { A: 0.75, D: 0.88, X: 0.60 }

→ Không có "bảng trust toàn cầu" duy nhất.
→ Mỗi node có góc nhìn riêng — giống thế giới thật.
```

#### Node lạ gặp nhau lần đầu → trust bao nhiêu?

**Direct trust = 0.** Nhưng hỏi bạn bè (Transitive Trust / EigenTrust):

```
Y gặp X lần đầu:
  → Y hỏi A (trust 0.82): "Biết X không?" → A: "trust(X) = 0.70"
  → Y hỏi C (trust 0.91): "Biết X không?" → C: "trust(X) = 0.65"
  
  trust_Y(X) = Σ trust_Y(friend) × trust_friend(X) / Σ trust_Y(friend)
             = (0.82 × 0.70 + 0.91 × 0.65) / (0.82 + 0.91)
             ≈ 0.67
```

#### Trust KHÔNG thể tự khai

```
Node X có thể nói: "Trust tôi = 0.99!"
→ VÔ NGHĨA vì:
  → Y KHÔNG hỏi X "trust mày bao nhiêu?"
  → Y hỏi BẠN BÈ CỦA Y về X
  → X không kiểm soát câu trả lời của bạn bè Y
```

#### Không ai biết Node X?

```
→ trust(X) = 0
→ X chỉ hoạt động ở Leaf tier (thấp nhất)
→ Dần dần tương tác → trust tăng
→ Mất tuần/tháng để lên trust cao — KHÔNG CÓ ĐƯỜNG TẮT
```

---

### Phân Tích Tấn Công Partition (Cô Lập Mạng)

#### Sự thật toán học

> **Không hệ phân tán nào phân biệt được** "mạng bị cắt cáp biển" vs "10 node cố tình rút mạng" — từ bên trong chúng HOÀN TOÀN GIỐNG NHAU.

Bitcoin cũng không giải được. Bitcoin chọn: cứ đào ở cả 2 phía, nối lại → chain dài thắng.

OneBrain chọn: **làm cho gian lận KHÔNG CÓ LỢI** thay vì cố phân biệt thật/giả.

---

#### Kịch bản A: 10 node bị cô lập tự nhiên (đứt cáp)

```
→ Trong partition, vẫn encode/verify KU THẬT
→ OBT earned = TENTATIVE
→ Khi nối lại → mạng lớn kiểm tra:
   ✅ KU nội dung thật? ✅ Encoding đúng? ✅ Số lượng hợp lý?
→ Nếu OK → confirm OBT → SETTLED
→ Kết quả: Không mất OBT, chỉ delay confirmation.
```

---

#### Kịch bản B: Long Con Attack (node trust cao rồi cô lập)

```
Giai đoạn 1 (3 tháng): 10 nodes xây trust cao (0.8+)
Giai đoạn 2: Cô lập, tạo KU giả, encode/verify lẫn nhau
Giai đoạn 3: Kết nối lại, trình OBT
```

**Phòng thủ 3 tầng:**

**Tầng 1: Trust Decay** — node offline → trust giảm dần:
```
Ngày 0:  trust = 0.8
Ngày 1:  trust = 0.78 (mạng thật không thấy node)
Ngày 7:  trust = 0.5
Ngày 30: trust = 0.1
Ngày 60: trust ≈ 0
```

**Tầng 2: Trust do người khác tính** — không tự khai:
```
Node Z kiểm tra mint proof:
  → Z hỏi bạn bè: "Biết Node A không?"
  → Bạn bè: "A biến mất 2 tháng, trust = 0.05"
  → 10 witnesses × 0.05 = 0.5 < threshold 0.6 → ❌ REJECT
```

**Tầng 3: Connectivity Proof** — chứng minh "đang ở mạng thật":
```
MintProof phải kèm:
  recent_gossip: bằng chứng nhận gossip từ node NGOÀI nhóm witness
  network_sample: K node ngẫu nhiên KHÁC witnesses xác nhận
→ Mạng cô lập không có gossip từ bên ngoài → invalid
```

---

#### Kịch bản C: Quick Isolation Attack (cô lập vài phút)

```
10 node trust cao → ngắt 5 phút → spam KU giả → nối lại
Trust chưa kịp decay trong 5 phút!
```

**Phòng thủ 4 cơ chế:**

**Cơ chế 1: Rate Limiting**
```
MAX_ENCODINGS_PER_HOUR = 10 (per node)
5 phút = 1/12 giờ → tối đa ~1 encoding/node
10 nodes × 5 phút → tối đa 10 KU
Claim 100 KU trong 5 phút → vi phạm rate limit → ❌ REJECT TẤT CẢ
```

**Cơ chế 2: Gossip Gap Detection**
```
Mạng thật thấy:
  t=0:00  A1-A10 online, gossip bình thường
  t=0:01  A1-A10 ĐỒNG LOẠT mất tín hiệu
  t=0:06  A1-A10 ĐỒNG LOẠT quay lại + trình mint proofs
  → 🚩 RED FLAG: 10 nodes cùng offline/online ĐỒNG THỜI
  → Trigger: ELEVATED SCRUTINY
```

**Cơ chế 3: KU Content Validation (quan trọng nhất)**
```
KU giả → PoMV score = 0 (không ai dùng) → R1 owner reward = 0 mãi mãi
KU trùng CID → 0 OBT (duplicate)
KU vô nghĩa → encoding verifier re-check fail
→ Chỉ earn được R2/R3 encoding reward (tối đa ~10 OBT/KU × rate limit)
```

**Cơ chế 4: Economic Disincentive**
```
Chi phí tấn công:
  → 3 tháng xây trust × 10 nodes = 30 node-months

Lợi ích nếu thành công:
  → ~100 OBT encoding rewards (rate-limited)
  → R1 PoMV = 0 (KU giả không ai dùng)

Rủi ro nếu bị phát hiện:
  → Trust SLASH: 0.8 → 0.1 (mất 90% trust)
  → Tất cả mint proofs trong gap bị VOID
  → Mất hàng THÁNG trust đã xây

→ CHI PHÍ >> LỢI ÍCH → KHÔNG ĐÁNG ĐỂ TẤN CÔNG
```

---

### Confirmation Levels (xử lý mạng chậm)

```
Level 0: PENDING    — Vừa tạo, chưa ai confirm
Level 1: TENTATIVE  — 1-2 witnesses xác nhận
Level 2: CONFIRMED  — K witnesses xác nhận (K=3-5)
Level 3: SETTLED    — Lan rộng, không thể đảo ngược

Quy tắc:
  MINT:     Cần Level 2+ mới ghi nhận
  TRANSFER: Cần Level 2+ mới gửi được
  RECEIVE:  Level 1 thấy được (nhưng chưa tiêu được)
```

**Tốc độ confirm:**

| Hệ thống | Thời gian 1 confirm | Full confirm |
|----------|--------------------|----|
| Bitcoin | ~10 phút | ~1 giờ (6 blocks) |
| Ethereum | ~12 giây | ~2.5 phút |
| **OBP gossip** | **50-200ms** | **1-3 giây (Level 2), 10-30 giây (Level 3)** |

**Partition + reconnect:**
- CRDT đảm bảo **eventual consistency** — dù merge lúc nào, kết quả cuối cùng luôn đúng
- Double-spend trong partition: dùng log-based ledger với VectorClock → conflict detected → cái có nhiều witnesses thắng

---

### Đánh Giá Trung Thực: Có Hoàn Hảo Không?

> **Không.** Không hệ thống nào hoàn hảo 100%.

| Hệ thống | Hoàn hảo? | Gian lận tối đa | Chi phí tấn công |
|----------|----------|-----------------|-----------------|
| Bitcoin | Không (51% attack) | Hàng tỷ USD | Hàng tỷ USD điện + hardware |
| Ethereum | Không (33% stake) | Hàng tỷ USD | Hàng tỷ USD staked ETH |
| Ngân hàng | Không (insider fraud) | Tùy quy mô | Compliance + audit |
| **OBT** | **Không** | **~10 OBT/node × rate limit** | **Tháng xây trust, giây mất trust** |

**Triết lý bảo mật OneBrain:**
- Không cần ngăn 100% gian lận (bất khả thi)
- Chỉ cần: **chi phí gian lận > lợi ích gian lận**
- Gian lận 5 phút kiếm ~100 OBT nhưng mất 3 tháng trust → không ai làm
- Consistent với PoMV: **non-punitive nhưng giá trị phải đến từ hoạt động thật** — OBT giả mint được nhưng giá trị dài hạn = 0 (PoMV = 0, metabolism = 0, ecosystem sẽ để nó chết tự nhiên)

---

## Tồn Đọng Cần Giải Quyết

### Đã giải quyết qua thảo luận

- [x] Decentralization: OBP-native, không center
- [x] Anti-manipulation: 5 cơ chế (double-spend, forgery, sybil, replay, collusion)
- [x] Legitimate minting: Deterministic output của consensus
- [x] Trust storage: Phân tán, mỗi node tính riêng (EigenTrust)
- [x] Partition attacks: Rate limit + gossip gap + content validation + economic disincentive
- [x] Network speed: 4 confirmation levels, CRDT eventual consistency

### Cần thiết kế trong OBT Spec

| # | Hạng mục | Câu hỏi cụ thể | Priority |
|---|---------|----------------|---------|
| D1 | **Minting rate** | Bao nhiêu OBT/epoch cho R1/R2/R3/R4? Tỷ lệ giữa các luồng? | 🔴 Critical |
| D2 | **Storage reward** | R4 formula: `size × duration × factor` — factor = ? Proof-of-Storage protocol? | 🔴 Critical |
| D3 | **Anti-gaming thresholds** | Rate limit cụ thể? Min KU size để earn? Spam detection rules? | 🔴 Critical |
| D4 | **Transfer wire format** | Thêm message types cho OBT transfer (hiện có 74, cần thêm ~4-6) | 🟠 High |
| D5 | **Balance storage** | Local ledger + DHT replicas? Merkle tree structure? | 🟠 High |
| D6 | **Trust decay formula** | Decay rate khi offline? Linear hay exponential? Threshold phục hồi? | 🟠 High |
| D7 | **Gossip gap detection** | Ngưỡng "đồng loạt offline" là bao nhiêu nodes? Thời gian window? | 🟡 Medium |
| D8 | **Connectivity proof** | Bao nhiêu gossip receipts cần kèm mint proof? TTL? | 🟡 Medium |
| D9 | **Dispute resolution** | Transfer conflict: witness count thắng? Timeout rules? | 🟡 Medium |
| D10 | **Retrospective audit** | Audit trigger conditions? Slash penalty formula? Appeal process? | 🟡 Medium |

> [!TIP]
> **Bước tiếp theo đề xuất**: Thiết kế **OBT_SPEC.md** (tương tự ENCODING_CONSENSUS_SPEC) — cover D1-D10 ở trên, dựa trên 3 nguyên tắc của Owner + kết quả thảo luận bảo mật. Sau đó implement.


---

## UPDATE 30/06/2026: D1-D10 DA GIAI QUYET

> Ket qua nghien cuu song song 4 nhom + Owner review + Agent decisions

### Bang tong hop D1-D10

| # | Hang muc | Giai phap | Tham khao | Status |
|---|---------|----------|-----------|--------|
| D1 | Minting rate | E = B x A x Q, trust-gated per-node | Helium, IOTA mana | Done |
| D2 | Storage reward | 5-factor formula + PoS-KU challenge | Sia + Arweave | Done |
| D3 | Anti-gaming | Rate limits + 4 quality gates + 4 pattern detectors | Nano, IOTA, Helium | Done |
| D4 | Transfer format | Account-Chain (Nano) + 6 message types (0xA0-0xA5) | Nano block-lattice | Done |
| D5 | Balance storage | Hybrid local + DHT, Merkle state root | Nano + Holochain | Done |
| D6 | Trust decay | Exponential e^(-0.01 x t), recovery cham hon decay | EigenLayer | Done |
| D7 | Gossip gap | 3+ nodes cung offline 30s -> elevated scrutiny | Custom | Done |
| D8 | Connectivity proof | 3+ gossip receipts outside witnesses, <60s | Custom | Done |
| D9 | Dispute resolution | First-seen wins + warrant + 30s timeout | Holochain | Done |
| D10 | Penalty system | 5 tang graduated + correlation + appeal 4 layers | ETH + Cosmos + EigenLayer | Done |

### Q4: Account-Chain thay G-Counter - CHON

G-Counter KHONG dung duoc cho OBT balance (chi tang, khong spend). Account-Chain (Nano model) la phuong an duy nhat kha thi.

### Q5: Tombstone (ban vinh vien) - CO

Chi cho 2 loai fraud nang nhat: systematic collusion ring leader + identity forgery. Appeal: >80% top-tier nodes.

### Q6: Epoch = 1 gio (3,600s) - CHON

Compatible voi pheromone decay (da per-hour). User nhan reward < 1 gio. OBT_EPOCH_DURATION_S = 3,600.

### Tai lieu nghien cuu tham khao

Xem chi tiet tai: docs/research/obt/
- 01_storage_reward_research.md
- 02_penalty_slashing_research.md
- 03_crdt_ledger_research.md
- 04_anti_gaming_research.md
- 05_research_synthesis.md
- 06_q4_q5_q6_decisions.md
