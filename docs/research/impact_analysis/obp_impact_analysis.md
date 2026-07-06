# OBP Impact Analysis — KU/OBT Changes

## Kết Luận

> **OBP cần cập nhật cả CODE, DOC lẫn PAPER.** Đặc biệt có 1 bug critical: `MessageType` enum thiếu 0xA0–0xA6 → wire layer sẽ **reject tất cả OBT messages**.

---

## 🔴 MUST FIX (5 items)

### Code (3 items)

| # | File | Vấn đề | Impact |
|---|------|--------|--------|
| 1 | `messages.rs` | `MessageType` enum **THIẾU** 0xA0–0xA6 → `from_u8()` returns `None` | 💥 **Wire layer reject ALL OBT messages** |
| 2 | `obt_transfer.rs` | `is_obt_message()` dùng `0xA0..=0xA5` (thiếu 0xA6), test assert `!is_obt_message(0xA6)` sai, không có `ObtForkWarrant` struct | ForkWarrant broadcast không hoạt động |
| 3 | `error.rs` | `NetError` thiếu `Obt(ObtError)` variant | Không thể báo lỗi OBT qua network |

### Doc (2 items)

| # | File | Vấn đề |
|---|------|--------|
| 4 | `OBP_SPEC.md` §5 | Message codes **HOÀN TOÀN SAI** — Spec nói 0xA1=MintProof, code nói 0xA1=TransferConfirm |
| 5 | Network paper Ch.3 | Message registry chỉ đến 0x95, thiếu 0xA0–0xA6 (74→81 types) |

---

## 🟡 SHOULD FIX (6 items)

| # | File | Vấn đề | Priority |
|---|------|--------|----------|
| 6 | `dht.rs` | Không có replica tracking (`actual_replicas`, `epochs_stored`) → R4 storage rewards chưa hoạt động | HIGH |
| 7 | `membership.rs` | `NodeTier` riêng biệt với `ku-core::obt_constants::NodeTier` — 2 enum trùng nhau | MEDIUM |
| 8 | `obt_transfer.rs` | Không gọi `check_transfer_eligibility()` → node bị jail vẫn transfer được | MEDIUM |
| 9 | `membership.rs` | Fitness scoring không tích hợp OBT penalty state | MEDIUM |
| 10 | — | Thiếu handlers cho MintProof validation, ForkWarrant propagation, EpochSummary | MEDIUM |
| 11 | — | Thiếu `obt_gossip.rs` cho gossip security integration | MEDIUM |

---

## 🟢 NICE TO HAVE (3 items)

| # | Vấn đề |
|---|--------|
| 12 | PubSub topics cho OBT events (mint, fork, epoch) |
| 13 | OBT integration tests trong `tests.rs` |
| 14 | Paper line counts/module counts update |

---

## Implementation Plan

### Phase 1: Critical Fixes (Code)

#### 1.1 `messages.rs` — Add OBT MessageType variants
```rust
// Add to MessageType enum:
ObtTransferRequest = 0xA0,
ObtTransferConfirm = 0xA1,
ObtBalanceQuery = 0xA2,
ObtBalanceResponse = 0xA3,
ObtMintBroadcast = 0xA4,
ObtStorageChallenge = 0xA5,
ObtForkWarrant = 0xA6,
```

#### 1.2 `obt_transfer.rs` — Fix 0xA6 and add ForkWarrant
- `is_obt_message()`: `0xA0..=0xA5` → `0xA0..=0xA6`
- `obt_message_name()`: add `0xA6 => "ForkWarrant"`
- Add `ObtForkWarrant` struct
- Fix test assertion

#### 1.3 `error.rs` — Add OBT error variants
```rust
#[derive(Debug)]
pub enum ObtError {
    TransferFailed(String),
    InsufficientBalance { required: u64, available: u64 },
    ForkDetected { offender: [u8; 32] },
    PenaltyActive(String),
    StorageChallengeTimeout,
    MintValidationFailed(String),
}
```

### Phase 2: Doc Fixes

#### 2.1 `OBP_SPEC.md` §5 — Fix message code table
Match actual code: 0xA0=TransferRequest, 0xA1=TransferConfirm, etc.

#### 2.2 Network paper Ch.3 — Add OBT message range
Add 0xA0–0xA6 to message registry table, update counts 74→81.

### Phase 3: Should Fix (Code)

Items #6-#11 — larger design work for DHT tracking, NodeTier unification, handler layer.

---

## Hiện trạng ku-net Code

```
ku-net/src/
├── lib.rs              ✅ Registers obt_transfer
├── constants.rs        ✅ 0xA0–0xA6 defined
├── obt_transfer.rs     ⚠️ Missing 0xA6 handler
├── messages.rs         🔴 Missing 0xA0–0xA6 in MessageType enum
├── error.rs            🔴 Missing OBT errors
├── identity.rs         ✅ OK (Ed25519)
├── membership.rs       ⚠️ Duplicate NodeTier
├── dht.rs              ⚠️ No replica tracking
├── transport.rs        ✅ OK (QUIC)
├── encoding_gossip.rs  ✅ OK
├── encoding_job.rs     ⚠️ No encoding_time_ms
├── metabolism_gossip.rs ✅ OK
├── discovery.rs        ✅ OK
├── pubsub.rs           ✅ OK
├── sync.rs             ✅ OK
├── stigmergy.rs        ✅ OK
├── vacuum.rs           ✅ OK
└── tests.rs            ⚠️ No OBT tests
```
