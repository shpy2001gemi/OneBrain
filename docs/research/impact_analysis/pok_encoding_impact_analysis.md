# Phân tích tác động Encoding Consensus → Pillar 4 POK

## Kết luận tổng thể

> [!TIP]
> **POK Code hoàn toàn độc lập** — PoMV scoring không phụ thuộc encoding_status. Docs đã ghi nhận rõ ràng. Chỉ cần **2 bổ sung nhỏ** về cross-reference.

---

## Kiến trúc — Tại sao POK không bị ảnh hưởng

Encoding Consensus và PoMV là **2 lifecycle song song và độc lập**, đã được thiết kế có chủ đích:

```
┌─────────────────────────────────────────────┐
│               KuRuntime                      │
│                                              │
│  ┌─── Encoding Lifecycle ──────────────────┐ │
│  │ RAW → SELF → PART → FULL               │ │
│  │ (cấu trúc binary có đúng không?)        │ │
│  └─────────────────────────────────────────┘ │
│                                              │
│  ┌─── PoMV Lifecycle ──────────────────────┐ │
│  │ Rumor → Observation → ... → Axiomatic   │ │
│  │ (tri thức có giá trị không?)            │ │
│  └─────────────────────────────────────────┘ │
│                                              │
│  Hai lifecycle KHÔNG gate nhau:              │
│  • KU có thể Corroborated + SELF            │
│  • KU có thể Raw + Axiomatic (lý thuyết)    │
└─────────────────────────────────────────────┘
```

### Bằng chứng code

| Module POK | Tham chiếu encoding? | Chi tiết |
|-----------|---------------------|----------|
| [pomv.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/pomv.rs) | ❌ Không | 6 signals → weighted score. Zero encoding refs |
| [pomv_runtime.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/pomv_runtime.rs) | ❌ Không | Per-KU state machine. Zero encoding refs |
| [metabolism.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/metabolism.rs) | ❌ Không | Usage tracking. Zero encoding refs |
| [prediction.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/prediction.rs) | ❌ Không | Prediction accuracy. Zero encoding refs |
| [immune.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/immune.rs) | ❌ Không | Anti-fragile survival. Zero encoding refs |
| [entropy.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/entropy.rs) | ❌ Không | Novelty decay. Zero encoding refs |
| [synaptic.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/synaptic.rs) | ❌ Không | Graph centrality. Zero encoding refs |
| [ecosystem.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/ecosystem.rs) | ❌ Không | Niche fitness. Zero encoding refs |
| [epigenetics.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/epigenetics.rs) | ❌ Không | TrustSection store. Zero encoding refs |
| [encoding_reward.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/encoding_reward.rs) | ✅ Riêng biệt | OBT rewards cho encoding. **Không chạm PoMV scoring** |

### Bằng chứng docs

| Doc | Đã đề cập encoding? | Status |
|-----|---------------------|--------|
| [POK_V2_SPECIFICATION.md §3.7](file:///c:/Users/shpy2/Documents/OneBrain/docs/specs/POK_V2_SPECIFICATION.md) | ✅ "parallel but independent" | ✅ OK |
| [POK_DESIGN.md](file:///c:/Users/shpy2/Documents/OneBrain/docs/specs/POK_DESIGN.md) | ✅ OBT = internal accounting, not crypto | ✅ OK |
| [KU_ARCHITECTURE.md §3.7](file:///c:/Users/shpy2/Documents/OneBrain/docs/specs/KU_ARCHITECTURE.md) | ✅ "encoding status không ảnh hưởng CID" | ✅ OK |
| [ENCODING_CONSENSUS_SPEC.md](file:///c:/Users/shpy2/Documents/OneBrain/docs/specs/ENCODING_CONSENSUS_SPEC.md) | ✅ "Contributor được thưởng bởi PoMV sau" | ✅ OK |

---

## Gaps nhỏ cần bổ sung

### Gap 1: POK_V2_SPECIFICATION.md — thiếu OBT reward reference (LOW)

§3.7 nói "parallel but independent" nhưng **chưa nhắc đến encoding_reward.rs** — là module kết nối giữa Encoding Consensus (Pillar 1) và POK (Pillar 4) về mặt OBT token.

**Thêm**: 1 paragraph trong §3.7 nói rõ:
- Contributor được thưởng qua PoMV (không qua encoding reward)
- Verifiers/Correctors/ProBono được thưởng OBT qua `encoding_reward.rs`
- OBT rewards là internal accounting, không phải PoMV score

### Gap 2: FEATURE_DETAILS.md — thiếu encoding_reward trong POK section (LOW)

Encoding reward section đã có nhưng nằm trong "Encoding Consensus" section (đúng). Tuy nhiên, POK section nên cross-reference tới nó vì OBT liên quan trực tiếp đến POK tokenomics.

---

## Đề xuất thay đổi

| # | Target | Loại | Nội dung | Priority |
|---|--------|------|----------|----------|
| 1 | `POK_V2_SPECIFICATION.md` §3.7 | **Doc** | Thêm OBT reward cross-reference (2-3 lines) | LOW |
| 2 | `FEATURE_DETAILS.md` | **Doc** | Thêm cross-ref trong POK section tới encoding_reward | LOW |

> [!IMPORTANT]
> **Không cần sửa code POK**. Thiết kế "parallel but independent" là chủ đích — PoMV đánh giá *giá trị tri thức*, encoding consensus đánh giá *độ chính xác encoding*. Hai concerns hoàn toàn tách biệt.
