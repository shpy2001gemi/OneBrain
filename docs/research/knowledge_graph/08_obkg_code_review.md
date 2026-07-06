# OBKG Code Review — Báo Cáo Toàn Diện

> **Ngày**: 2 tháng 7, 2026  
> **Scope**: 9 new files + 6 modified files, 855 tests  
> **Reviewer**: AI Code Review Agent (đọc toàn bộ source code)

---

## Tổng Quan

> [!IMPORTANT]
> **Đánh giá tổng thể: ★★★★☆ (4/5) — Chất lượng cao, kiến trúc mạnh**
>
> Core domain types, decay engine, embeddings, dream mode, FedR protocol, và qualifiers đều **xuất sắc**. Các vấn đề chính tập trung ở **executor.rs** (graph traversal) và **graph_storage.rs** (transactional safety).

---

## Per-File Quality Scores

| File | Phase | Quality | Issues |
|------|-------|---------|--------|
| [graph_types.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/graph_types.rs) | 1 | ★★★★★ | None |
| [graph_events.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/graph_events.rs) | 1 | ★★★★☆ | 1 LOW |
| [graph_decay.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/graph_decay.rs) | 1 | ★★★★★ | None |
| [graph_storage.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/graph_storage.rs) | 1 | ★★★★☆ | 1 HIGH, 1 MEDIUM |
| [storage.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/storage.rs) | 1 | ★★★★★ | None |
| [graph_embeddings.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/graph_embeddings.rs) | 2 | ★★★★★ | None |
| [graph_bio.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/graph_bio.rs) | 2 | ★★★★☆ | 1 LOW |
| [immune.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/immune.rs) | 2 | ★★★★☆ | None |
| [ast.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/ast.rs) | 3 | ★★★★★ | None |
| [parser.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/parser.rs) | 3 | ★★★★☆ | 1 MEDIUM |
| [executor.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/executor.rs) | 3 | ★★★☆☆ | 2 HIGH, 2 MEDIUM, 1 LOW |
| [graph_dream.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/graph_dream.rs) | 4 | ★★★★★ | 1 LOW |
| [graph_fedr.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/graph_fedr.rs) | 4 | ★★★★★ | None |
| [graph_qualifiers.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/graph_qualifiers.rs) | 4 | ★★★★★ | None |

---

## 🔴 HIGH Priority — Cần Fix

### H1. `executor.rs` — Edge type matching dùng Debug format

```rust
// Hiện tại (fragile):
format!("{:?}", bond.relation) == *t  // "Extends" == "Extends"
```

**Vấn đề**: So sánh bằng `Debug` representation — có thể break nếu Rust thay đổi format, và không hỗ trợ case-insensitive matching.

**Fix**: Thêm method `RelationType::from_str()` hoặc `matches_name()`:
```rust
// Đề xuất:
bond.relation.matches_name(t)  // impl trong types.rs
```

---

### H2. `executor.rs` — FIND HISTORY được parse nhưng không dispatch

**Vấn đề**: Parser parse `FIND HISTORY (k:KU)` thành `FindQuery { history: true }`, nhưng `exec_find()` **không check** `find.history` → `exec_history_find()` **không bao giờ được gọi** (marked `#[allow(dead_code)]`).

**Fix**: Thêm dispatch trong `exec_find()`:
```rust
if find.history {
    results = self.exec_history_find(&find)?;
} else if !find.pattern.edges.is_empty() {
    results = self.exec_graph_find(&find)?;
} else {
    // existing linear scan
}
```

---

### H3. `graph_storage.rs` — Non-atomic multi-transaction insert

**Vấn đề**: `insert_bond()` dùng **3 transactions riêng biệt** (read → remove → insert). Nếu crash giữa chừng → database inconsistent (secondary indexes mất mà primary còn).

**Fix**: Gộp thành 1 write transaction duy nhất.

---

## 🟡 MEDIUM Priority — Nên Fix

### M1. `executor.rs` — O(N×M) linear scan trong graph traversal

**Vấn đề**: `exec_graph_find()` loop qua `self.kus` cho mỗi CID (O(N) per lookup). Với 10K KUs × 100 edges = 1M operations.

**Fix**: Build `HashMap<[u8;32], &KuRuntime>` trước khi traverse.

---

### M2. `executor.rs` — `exec_create_from_text()` returns empty rows

**Vấn đề**: Returns `QueryResult` với `rows: vec![]` nhưng `affected_count: 1`. Inconsistent với `exec_create()` (trả về KU trong rows).

---

### M3. `parser.rs` — `quoted_string()` fails on empty strings

**Vấn đề**: Dùng `take_while1` (1+ chars) → `""` parse fail.

**Fix**: Đổi sang `take_while` (0+ chars).

---

### M4. `graph_storage.rs` — Index key collisions

**Vấn đề**: `bond_weight` và `edge_time` keys không chứa `RelationType`. Hai bonds cùng (src, tgt) nhưng khác relation + cùng weight/time → key collision → overwrite.

**Fix**: Thêm relation byte vào composite key.

---

## 🟢 LOW Priority — Nice to Fix

| # | File | Issue |
|---|------|-------|
| L1 | `graph_events.rs` | `replay_at_time()` giả định events theo thứ tự thời gian — không enforce trên `append()` |
| L2 | `graph_dream.rs` | `now_secs as u32` truncation — Y2106 (consistent với BondMeta design) |
| L3 | `graph_bio.rs` | STDP `delta_ms.abs()` có thể panic trên `i64::MIN` trong debug builds |
| L4 | `executor.rs` | Silent failures trên unknown/mismatched SET assignments |
| L5 | `ku-core/lib.rs` | OBKG types chưa re-export ở crate root → phải dùng full path |

---

## Positives ✅

### Kiến trúc
- **3-layer separation** (Core DNA → Epigenetics → Graph) rõ ràng, không circular deps
- **Feature gating** đúng (`#[cfg(feature = "storage")]` cho redb)
- **Pure functions** trong ku-core (không phụ thuộc storage) — dễ test

### Code Quality
- **Comprehensive tests**: 855 tests, 0 failures, coverage tốt
- **No unsafe code** — pure safe Rust
- **Proper error handling**: Custom error types, no unwrap() ngoài tests
- **Good documentation**: Module-level docs, struct docs, function docs

### Design Decisions
- **BondMeta 9 bytes** — compact, memory-efficient
- **int8 RotatE** — 64x compression so với float32, vẫn hoạt động tốt
- **FedR privacy** — chỉ share relation deltas, entity embeddings stay local
- **Dream Mode** — elegant bio-inspired offline restructuring
- **Bond Qualifiers** — Wikidata-style extensibility

---

## Đề Xuất Hành Động

> [!WARNING]
> **Recommend fix H1 + H2 + H3 trước khi merge/deploy.** Các issues MEDIUM có thể fix sau nhưng nên track.

Bạn muốn tôi **fix 3 issues HIGH** ngay không?
