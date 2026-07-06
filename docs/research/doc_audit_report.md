# 📋 Đánh Giá Toàn Bộ Tài Liệu — 4 Pillar OneBrain

> **Phạm vi**: 13 tài liệu (trừ paper/), so sánh với code thực tế sau Phase A–D.
> **Nguyên tắc**: Không có data cũ, không có release trước → chỉ phục vụ v6. Mọi tham chiếu v5 đều SAI.

---

## Tổng Quan

| # | Document | Pillar | Status | Priority |
|---|----------|--------|--------|----------|
| 1 | [KU_CORE_DNA_V6_SPEC.md](file:///c:/Users/shpy2/Documents/OneBrain/docs/specs/KU_CORE_DNA_V6_SPEC.md) | P1 | 🟡 NEEDS-UPDATE | **P1** |
| 2 | [KU_ARCHITECTURE.md](file:///c:/Users/shpy2/Documents/OneBrain/docs/specs/KU_ARCHITECTURE.md) | P1 | 🔴 **CRITICAL** | **P0** |
| 3 | [KU_ENCODING_PIPELINE.md](file:///c:/Users/shpy2/Documents/OneBrain/docs/specs/KU_ENCODING_PIPELINE.md) | P1 | 🟢 UP-TO-DATE | P2 |
| 4 | [POK_DESIGN.md](file:///c:/Users/shpy2/Documents/OneBrain/docs/specs/POK_DESIGN.md) | P2 | 🟡 NEEDS-UPDATE | **P1** |
| 5 | [POK_V2_SPECIFICATION.md](file:///c:/Users/shpy2/Documents/OneBrain/docs/specs/POK_V2_SPECIFICATION.md) | P2 | 🟢 MOSTLY-OK | P2 |
| 6 | [KQL_SPEC.md](file:///c:/Users/shpy2/Documents/OneBrain/docs/specs/KQL_SPEC.md) | P3 | 🟢 UP-TO-DATE | P2 |
| 7 | [OBP_SPEC.md](file:///c:/Users/shpy2/Documents/OneBrain/docs/specs/OBP_SPEC.md) | P4 | 🟢 UP-TO-DATE | P2 |
| 8 | [README.md](file:///c:/Users/shpy2/Documents/OneBrain/docs/README.md) | Cross | 🟡 NEEDS-UPDATE | **P1** |
| 9 | [FEATURE_TREE.md](file:///c:/Users/shpy2/Documents/OneBrain/docs/features/FEATURE_TREE.md) | Cross | 🟡 NEEDS-UPDATE | **P1** |
| 10 | [FEATURE_DETAILS.md](file:///c:/Users/shpy2/Documents/OneBrain/docs/features/FEATURE_DETAILS.md) | Cross | 🟡 NEEDS-UPDATE | **P1** |
| 11 | [whitepaper.md](file:///c:/Users/shpy2/Documents/OneBrain/docs/whitepaper.md) | Cross | ⬜ STUB | P2 |
| 12 | [KU_vs_AI_Model_vi.md](file:///c:/Users/shpy2/Documents/OneBrain/docs/references/KU_vs_AI_Model_vi.md) | Ref | 🟡 NEEDS-UPDATE | **P1** |
| 13 | [PILLAR_REVIEW.md](file:///c:/Users/shpy2/Documents/OneBrain/docs/PILLAR_REVIEW.md) | Cross | 🟡 NEEDS-UPDATE | **P1** |

---

## 🔴 P0 — CRITICAL: Sửa Ngay

### KU_ARCHITECTURE.md §4 — Bảng Opcode SAI HOÀN TOÀN

> [!CAUTION]
> §4 "Instruction Set (32 Opcodes)" liệt kê các opcode **KHÔNG TỒN TẠI** trong code!
> Ví dụ: `0x14=DURATION`, `0x23=SPATIAL_REL`, `0x32=IMPORTANCE`, `0x33=CONTEXT`, `0x34=SOURCE`, `0x35=TIMESTAMP`, `0x40=AFFECT`, `0x41=SENSORY`, `0x50=ANALOGY`, `0x51=CONTRAST`, `0x52=EXAMPLE`, `0xF1=NOP`.
> **KHÔNG opcode nào trong số này tồn tại trong `Op` enum thực tế** (`core_dna.rs`).

**Hành động**: Thay toàn bộ bảng bằng `Op` enum từ `core_dna.rs` (0x00-0x1F).

**Thêm nội dung thiếu:**

| Nội dung | Mô tả |
|---------|-------|
| `KuRuntime` struct | Primary v6 composite — CHƯA ĐƯỢC NHẮC ĐẾN |
| `Epigenetics` struct | Layer 2 — chưa có |
| `Expression` struct | Layer 3 rendering — chưa có |
| `ConceptDict` v6 | `ConceptEntry` với multilingual, varint tiers — doc mô tả v5 HashMap |
| `PersistentConceptDict` | redb-backed — doc nói "planned SQLite" nhưng redb đã implement |
| `KuLifecycle` | Orchestrator mới — hoàn toàn chưa có |
| v6 module map | `lib.rs` có 27+ modules nhưng doc chỉ liệt kê ~10, thiếu 5+ v6 modules |

**Xóa/thu gọn:**
- §9 "Backward Compatibility" (`decode_any()`) — không có release cũ
- v5 module labels ("Legacy v4/v5")

---

## 🟡 P1 — Major Updates

### 1. KU_CORE_DNA_V6_SPEC.md

| Vấn đề | Section | Chi tiết |
|--------|---------|---------|
| v5 backward compat | §9 "Auto-detect v4/v5" | Toàn bộ section về detect v4/v5 CBOR — không có data cũ |
| v5 backward compat | §11 "Backward Compatibility — decode_any" | Mô tả bridge CBOR v4/v5 → KnowledgeUnit — không cần |
| v5 bridge | §12 "Bridge — KU ↔ CoreDna Conversion" | Convert từ v5 `KnowledgeUnit` (Gene enum, Codon) → CoreDna — v5 types |
| v5 type reference | §12 line 1026 | Reference `KnowledgeUnit` struct — v6 dùng `KuRuntime` |
| v5 recommendation | §12 line 1158 | "use CBOR v4/v5 encoder" — KHÔNG! |
| Thiếu KuRuntime | Overall | Primary v6 struct chưa được đề cập |
| Thiếu Expression | Overall | Chỉ nhắc sơ trong §2 — thiếu chi tiết rendering |
| Thiếu KuLifecycle | Overall | Chưa có |
| Thiếu PersistentConceptDict | Overall | Chưa có |

**Hành động**: Xóa §9, §11, §12. Thêm §6-8 cho KuRuntime, ConceptDict v6, KuLifecycle.

---

### 2. POK_DESIGN.md

| Vấn đề | Section | Chi tiết |
|--------|---------|---------|
| 90% nội dung là PoK v1 | Toàn bộ body | Vote-based model (Submit → Screen → Review → Calculate → Reward) — **đã bị thay bởi PoMV v2** |
| v5 "codons" | §2.2, §Phase D | "Quiz generation from KU codons" — v6 dùng Instructions |
| KRL references | §7.3 | "KRL maturity scale" — không tồn tại trong code |

**Hành động**: Thu gọn v1 thành 1 section lịch sử. Phần lớn nội dung đã thay bằng POK_V2_SPECIFICATION.

---

### 3. FEATURE_TREE.md + FEATURE_DETAILS.md

> [!WARNING]
> Hai file này vẫn mô tả **PoK v1 (voting)** và **v5 KU schema**. Hoàn toàn sai so với code hiện tại.

| Vấn đề | Chi tiết |
|--------|---------|
| PoK v1 Voting | §3.2 "Community Voting" — Upvote/Downvote, Weighted Voting. **PoMV v2 KHÔNG có voting** |
| PoK v1 Value Calc | §3.3 — Novelty/Accuracy/Utility/Depth. **PoMV v2 dùng 6 signals**: Metabolism, Prediction, Entropy, Survival, Synaptic, Niche |
| PoK v1 Flow | §3.5 — "submit → screen → review → calculate → reward". **v2**: observe → tick → epistemic transition |
| v5 KU Schema | §1.1.4 — fields: id, author, content, category, tags. **v6**: CoreDna (instructions, gene_type) + Epigenetics (trust, bonds) + Expression |
| Blockchain | §8.4.1 — "Blockchain Layer (smart contracts, token)". **OneBrain KHÔNG dùng blockchain** (POK_DESIGN §1.5) |
| Thiếu v6 features | Không có: 3-layer architecture, 32 opcodes, ConceptDict, Expression rendering, CREATE FROM TEXT, KuLifecycle |

**Hành động**: Viết lại phần PoK theo v2. Thêm v6 features. Xóa blockchain references.

---

### 4. README.md

| Vấn đề | Chi tiết |
|--------|---------|
| Thiếu OBP_SPEC | Specs table không liệt kê OBP_SPEC.md |
| Thiếu module cross-refs | Missing: `epigenetics.rs`, `ku_runtime.rs`, `concept_dict.rs`, `persistent_concept_dict.rs`, `ku_lifecycle.rs`, `pomv_runtime.rs` |
| Stale code refs | `encoder.rs / decoder.rs` linked to "Backward Compat" |

---

### 5. KU_vs_AI_Model_vi.md

| Vấn đề | Section | Chi tiết |
|--------|---------|---------|
| v5 "Codon" | §2.1 line 61 | `Codons: (Boeing_737_Wing, sweep_angle, 25.0°)` → v6: `Instructions` |
| v5 Gene enum | §4 line 239 | `Gene::Hypothesis { body: ... }` → v6: `CoreDna { header: { gene_type: 7 } }` |
| v5 KQL query | §2.3 line 110 | `k.codons CONTAINS concept_id` → v6: `k.concept_ids CONTAINS` |
| `prev_cid` | §2.2 line 89 | Không có trong v6 Core DNA |

---

### 6. PILLAR_REVIEW.md

| Vấn đề | Chi tiết |
|--------|---------|
| v5 `KnowledgeUnit` | P1 line 123 — primary type đã đổi thành `KuRuntime` |
| Thiếu 5 v6 modules | `epigenetics.rs`, `ku_runtime.rs`, `concept_dict.rs`, `persistent_concept_dict.rs`, `ku_lifecycle.rs` |
| Message count mismatch | P2 nói "56 types" nhưng OBP_SPEC nói "59 Types" |
| v5 backward compat emphasis | "decode_any() tự động phát hiện v4/v5" — không cần |
| v5 KQL CREATE example | P3 — `CREATE (ku:KU {body: "..."})` là v5 property-bag syntax |

---

## 🟢 P2 — Minor / OK

### KU_ENCODING_PIPELINE.md ✅
- Đúng nhất trong tất cả specs
- Minor: ConceptDict description hơi cũ, PersistentConceptDict chưa nhắc

### POK_V2_SPECIFICATION.md ✅
- Mostly accurate
- Minor: v5 code examples trong "KU v6 Compatibility" section (Before/After) — xóa "Before (v5)"
- Minor: `Gene` enum reference line 393

### KQL_SPEC.md ✅
- Up-to-date
- Minor: Status markers "⚠️ v6 refactor needed" — v6 refactor đã XONG
- Minor: `KnowledgeUnit` in §6.2 → đổi `KuRuntime`

### OBP_SPEC.md ✅
- Up-to-date (chưa implement nhiều nên chưa có conflict)
- Minor: v5 rename mentions không cần
- Minor: "56 vs 59 messages" inconsistency

### whitepaper.md — Placeholder
- Chỉ là stub "Coming Soon". Không sai, chỉ trống.

---

## 📌 Đề Xuất Hành Động (Theo Thứ Tự)

### Phase E1: Critical Fix (P0)

| # | Action | Document | Effort |
|---|--------|----------|--------|
| 1 | **Thay bảng opcode sai** bằng `Op` enum thực từ `core_dna.rs` | KU_ARCHITECTURE.md §4 | Nhỏ |
| 2 | **Thêm KuRuntime, Epigenetics, Expression** vào architecture | KU_ARCHITECTURE.md | Trung bình |
| 3 | **Cập nhật module map** (thêm 5+ v6 modules) | KU_ARCHITECTURE.md §7 | Nhỏ |

### Phase E2: Major Updates (P1)

| # | Action | Document | Effort |
|---|--------|----------|--------|
| 4 | **Xóa §9, §11, §12 backward compat** | KU_CORE_DNA_V6_SPEC.md | Nhỏ |
| 5 | **Thêm KuRuntime, ConceptDict v6, KuLifecycle** sections | KU_CORE_DNA_V6_SPEC.md | Trung bình |
| 6 | **Viết lại PoK → PoMV v2** | FEATURE_TREE.md, FEATURE_DETAILS.md | Lớn |
| 7 | **Xóa blockchain references** | FEATURE_TREE.md §8.4.1 | Nhỏ |
| 8 | **Thêm v6 features** (3-layer, CREATE FROM TEXT, Expression) | FEATURE_TREE.md, FEATURE_DETAILS.md | Trung bình |
| 9 | **Thu gọn PoK v1** thành 1 section lịch sử | POK_DESIGN.md | Nhỏ |
| 10 | **Cập nhật README** (OBP_SPEC, module cross-refs) | README.md | Nhỏ |
| 11 | **Đổi Codon/Gene → Instruction/CoreDna** | KU_vs_AI_Model_vi.md | Nhỏ |
| 12 | **Cập nhật source tree + fix message count** | PILLAR_REVIEW.md | Nhỏ |

### Phase E3: Minor Polish (P2)

| # | Action | Document |
|---|--------|----------|
| 13 | Update ConceptDict description | KU_ENCODING_PIPELINE.md |
| 14 | Remove v5 "Before" code examples | POK_V2_SPECIFICATION.md |
| 15 | Update status markers "v6 refactor done" | KQL_SPEC.md |
| 16 | Clean v5 rename mentions | OBP_SPEC.md |

---

## Open Questions

> [!IMPORTANT]
> 1. **FEATURE_TREE / FEATURE_DETAILS**: Viết lại hoàn toàn (sạch hơn) hay sửa từng section? Hai file này sai ~40% nội dung (PoK v1 + v5 schema + blockchain).

> [!IMPORTANT]
> 2. **POK_DESIGN.md**: 90% là PoK v1. Giữ lại làm tài liệu lịch sử hay xóa/archive? POK_V2_SPECIFICATION.md đã thay thế.

> [!TIP]
> 3. **Merge 2 ConceptDict**: `text_parser::ConceptDict` (simple HashMap) vs `concept_dict::ConceptDict` (v6 ConceptEntry). Nên merge thành 1 hay document rõ 2 implementations?
