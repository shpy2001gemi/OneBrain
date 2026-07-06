# 🔍 Cross-Pillar Gap Analysis — KU v6 Architecture

> Reviewed by 4 parallel agents. Total: **34 issues** found across all pillars.

## Executive Summary

| Pillar | Status | Critical | Medium | Minor |
|--------|--------|:--------:|:------:|:-----:|
| **P1: KU Core** | ⚠️ Có lỗ hổng | 3 | 4 | 3 |
| **P2: OBP Network** | ✅ Gần hoàn chỉnh | 1 | 2 | 2 |
| **P3: KQL** | ⚠️ Có lỗ hổng | 3 | 6 | 3 |
| **P4: PoK** | ⚠️ Thiếu integration | 0 | 5 | 2 |
| **Tổng** | | **7** | **17** | **10** |

---

## 🔴 CRITICAL (7 issues — phải fix)

### C1. `storage.rs` (KQL) vẫn hoàn toàn v5
- **File**: [storage.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/storage.rs)
- **Vấn đề**: Dùng `KnowledgeUnit`, `encode_knowledge_unit()`, index by `ku.codons` — toàn bộ v5
- **Spec §7**: Nên là `CID → wire_bytes` (Core DNA binary) + SQLite cho Epigenetics
- **Fix**: Rewrite `KuStorage` cho v6 KuRuntime
- **Effort**: 2-3 giờ

### C2. Parser không support `CONTAINS` condition
- **File**: [parser.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/parser.rs)
- **Vấn đề**: AST có `Condition::Contains`, executor handle được, nhưng parser không parse được
- **Spec §2.1**: `WHERE k.concept_ids CONTAINS 301` là documented syntax
- **Fix**: Thêm `contains_condition` parser
- **Effort**: 30 phút

### C3. `gene_type` field trả về Integer, spec nói String
- **File**: [ku_runtime.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/ku_runtime.rs) L283
- **Vấn đề**: `extract_field("gene_type")` → `Integer(0)`, nhưng spec nói `WHERE k.gene_type = "Fact"` (String)
- **Impact**: Query `WHERE k.gene_type = "Fact"` **luôn fail** vì type mismatch
- **Fix**: Map gene_type number → name string trong extract_field
- **Effort**: 15 phút

### C4. `epistemic_status` field cũng trả về Integer thay vì String
- **File**: [ku_runtime.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/ku_runtime.rs) L300
- **Vấn đề**: Tương tự C3 — `WHERE k.epistemic_status = "Validated"` sẽ fail
- **Fix**: Map enum → name string
- **Effort**: 15 phút

### C5. lib.rs doc comment sai wire format
- **File**: [lib.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/lib.rs) L10
- **Vấn đề 1**: `VER_META(version:4|gene_type:4)` → thực tế là `version:3|gene_type:4|qualifier:1`
- **Vấn đề 2**: `END(0xF0)` → opcode là `0x1E`, wire byte mới là `0xF0`
- **Fix**: Sửa doc comment
- **Effort**: 5 phút

### C6. types.rs v4/v5 wire constants không có warning comment
- **File**: [types.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/types.rs) L1242-1247
- **Vấn đề**: `MAGIC: [u8; 2] = [0x4B, 0x44]` và `VERSION: u8 = 0x05` — v5 constants tồn tại song song với v6 `CORE_DNA_MAGIC = 0x4B`
- **Fix**: Thêm `// LEGACY v5 — do not use for v6` comment
- **Effort**: 5 phút

### C7. Tier 1 Structured CREATE chưa implement
- **File**: [parser.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/parser.rs), [executor.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/executor.rs)
- **Vấn đề**: Spec §2.2 define `CREATE (k:KU) FACT certainty=9000 { TRIPLE(s,p,o) }` syntax nhưng parser chỉ parse `{properties}` bag, executor chỉ tạo 2 instructions (Triple + Certainty)
- **Impact**: Không thể tạo KU với đa instructions qua KQL
- **Fix**: Extend AST, parser, executor
- **Effort**: 4-6 giờ

---

## 🟠 MEDIUM (17 issues)

### Pillar 1 (KU Core)

| # | Issue | File | Fix |
|---|-------|------|-----|
| M1 | `expression()` method quảng cáo trong doc nhưng chưa implement | [ku_runtime.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/ku_runtime.rs) L18 | Implement hoặc xoá doc |
| M2 | `Member.label` (ConceptId) không được extract trong `concept_ids()` | [ku_runtime.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/ku_runtime.rs) L183 | Thêm match arm |
| M3 | v6 functions không re-export từ lib.rs | [lib.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/lib.rs) | Thêm `pub use core_dna::{encode_core_dna, decode_core_dna, ...}` |
| M4 | ConceptDict không có persistence (save/load) | [concept_dict.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/concept_dict.rs) | Thêm JSON/SQLite export |

### Pillar 2 (OBP)

| # | Issue | File | Fix |
|---|-------|------|-----|
| M5 | `KnowledgeUnit` dùng trong 50+ chỗ ở ku-net (merger, watch, discovery) | ku-net/query/*.rs | Migrate sang KuRuntime |
| M6 | Message count sai: code nói "56", spec nói "59", thực tế **68** | [messages.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/messages.rs) L3 | Đếm lại, update cả code + spec |

### Pillar 3 (KQL)

| # | Issue | File | Fix |
|---|-------|------|-----|
| M7 | `k.cid` không trong extract_field | [ku_runtime.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/ku_runtime.rs) | Thêm "cid" match arm |
| M8 | UPDATE không support `epistemic_status`, `evidence_type` | [executor.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/executor.rs) L474 | Thêm string→enum mapping |
| M9 | `Scope::Semantic` thiếu trong AST/parser | [ast.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/ast.rs) L242 | Thêm variant |
| M10 | CREATE hardcode `"Creative" => 3` (trùng Procedure) | [executor.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/executor.rs) L234 | Fix gene type mapping |
| M11 | Tier 2 `CREATE FROM TEXT "..." WITH AI` chưa implement | parser.rs, ast.rs | Extend parser + AST |

### Pillar 4 (PoK)

| # | Issue | File | Fix |
|---|-------|------|-----|
| M12 | **KuRuntime ↔ PomvRuntime bridge missing** | [pomv_runtime.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/pomv_runtime.rs) | Tạo bridge function |
| M13 | `Epigenetics` có 2 bản `epistemic_status` — risk desync | [epigenetics.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/epigenetics.rs) L55 vs trust.epistemic_status | Xoá duplicate hoặc auto-sync |
| M14 | `epistemic_engine` transitions khác spec (dùng citations thay prediction_score) | [epistemic_engine.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/epistemic_engine.rs) | Align with spec |
| M15 | `spread_analysis` + `eigentrust` chưa wired vào PomvRuntime | pomv_runtime.rs | Wire in |
| M16 | `gene_type → ResolutionMethod` mapping thiếu trong prediction | [prediction.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/prediction.rs) | Add auto-mapping |
| M17 | `ciborium` dependency trong ku-net Cargo.toml có thể không cần | [Cargo.toml](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/Cargo.toml) L21 | Test remove |

---

## 🟡 MINOR (10 issues)

| # | Issue | File |
|---|-------|------|
| m1 | `messages.rs:22` doc nói `packed_cbor` thay vì `packed_binary` | ku-net messages.rs |
| m2 | lib.rs layer numbering khác spec | ku-net lib.rs |
| m3 | types.rs header vẫn nói "UKRL v4/v5" | ku-core types.rs |
| m4 | `add_bond()` hardcode `created_at: 0` (TODO) | ku-core epigenetics.rs |
| m5 | crdt.rs có `#[allow(dead_code)]` | ku-core crdt.rs |
| m6 | Cargo.toml TODO ciborium → DAG-CBOR | ku-core Cargo.toml |
| m7 | EXPLAIN hardcode index names | ku-kql executor.rs |
| m8 | Parser clause ordering cố định | ku-kql parser.rs |
| m9 | `k.difficulty` trong code nhưng không trong spec | ku_runtime.rs |
| m10 | POK spec §v6 mô tả tương lai, không phải hiện tại | POK_V2_SPECIFICATION.md |

---

## ✅ ĐÃ HOÀN CHỈNH

| Component | Status |
|-----------|--------|
| OBP wire format agnostic (opaque `Vec<u8>`) | ✅ |
| `PackedCbor` → `PackedBinary` rename toàn bộ | ✅ |
| `results_cbor` / `ku_cbor` → `results_payload` / `ku_payload` | ✅ |
| 32 opcodes Core DNA consistent | ✅ |
| 6 PoMV signals đúng trong TrustSection | ✅ |
| KuRuntime encode/decode roundtrip | ✅ |
| ConceptDict varint tiers đúng | ✅ |
| Executor v6 trên KuRuntime (14 tests) | ✅ |
| Epigenetics Layer 2 + Expression Layer 3 | ✅ |

---

## 📋 Recommended Action Plan

### Phase A — Quick Fixes (< 1 giờ, high impact)
1. **C3+C4**: Fix `gene_type` + `epistemic_status` → return String in extract_field
2. **C5+C6**: Fix doc comments (lib.rs wire format, types.rs legacy warning)
3. **C2**: Add CONTAINS parser
4. **M2**: Fix Member.label extraction
5. **M7**: Add `k.cid` to extract_field
6. **m1-m4**: Fix stale comments

### Phase B — Medium Effort (2-4 giờ)
7. **M13**: Fix epistemic_status duplication in Epigenetics
8. **M12**: Create KuRuntime ↔ PomvRuntime bridge
9. **M3**: Re-export v6 functions from lib.rs
10. **M8+M9+M10**: Fix executor UPDATE fields, Scope::Semantic, gene_type mapping

### Phase C — Large Effort (4-8 giờ)
11. **C1**: Rewrite storage.rs for v6
12. **C7**: Implement Tier 1 structured CREATE
13. **M5**: Migrate ku-net query modules từ KnowledgeUnit → KuRuntime

### Phase D — Future (khi cần)
14. **M11**: Tier 2 CREATE FROM TEXT (cần AI runtime)
15. **M1**: Implement expression() rendering
16. **M4**: ConceptDict persistence (SQLite)
17. **M14-M16**: Align epistemic transitions + wire PoK modules
