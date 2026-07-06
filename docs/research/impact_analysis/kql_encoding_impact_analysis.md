# Phân tích tác động Encoding Consensus → Pillar 3 KQL

## Kết luận tổng thể

> [!TIP]
> **KQL Code đã tương thích 100%** — không cần sửa code. Chỉ cần bổ sung tests và ví dụ trong spec.

---

## Kiến trúc KQL — Tại sao không cần sửa code

KQL sử dụng kiến trúc **field-driven extraction**, mọi field query đều đi qua 1 pipeline duy nhất:

```
KQL Parser → FieldPath("k.encoding_status") → field_path_to_name() → "encoding_status"
    → KuRuntime::extract_field("encoding_status") → ExtractedValue::Text("Full")
        → compare_values(Text vs Text) → Eq/NotEq
```

Vì `KuRuntime::extract_field()` đã xử lý `"encoding_status"` (trả `ExtractedValue::Text`), và KQL executor match `(Text, Text)` cho `Eq`/`NotEq` operators, query sau **đã hoạt động ngay**:

```kql
FIND (k:KU) WHERE k.encoding_status = "Full" RETURN k.gene_type, k.encoding_status
```

---

## Phân tích chi tiết từng file

### Code (ku-kql crate)

| File | LOC | Tác động | Status |
|------|-----|----------|--------|
| [parser.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/parser.rs) | 53K | `FieldPath` parser chấp nhận mọi dotted identifier. Không hardcode field names | ✅ OK |
| [executor.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/executor.rs) | 43K | `evaluate_condition()` delegate sang `extract_field()`. `compare_values()` match `(Text, Text)` cho `Eq`/`NotEq` | ✅ OK |
| [ast.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/ast.rs) | 14K | `FieldPath`, `Condition::Comparison` — generic, không hardcode | ✅ OK |
| [storage.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/storage.rs) | 15K | In-memory store. Lưu `KuRuntime` instances — đã chứa `encoding_status` | ✅ OK |
| [lib.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/lib.rs) | 450B | Module re-exports | ✅ OK |

### Dependency chain (đã connected)

```
ku-kql → ku-core::KuRuntime::extract_field("encoding_status")
                              ↓
                  ExtractedValue::Text(self.encoding_status.name())
                              ↓
                  "Raw" | "Self" | "Part" | "Full"
```

### Docs (ku-kql spec)

| Doc | Tác động | Status |
|-----|----------|--------|
| [KQL_SPEC.md §1.5](file:///c:/Users/shpy2/Documents/OneBrain/docs/specs/KQL_SPEC.md) | `encoding_status` đã có trong field table (line 162) | ✅ OK |
| [KQL_SPEC.md §2.2](file:///c:/Users/shpy2/Documents/OneBrain/docs/specs/KQL_SPEC.md) | CREATE note về `encoding_status = SELF` | ✅ OK |
| [KQL_SPEC.md §2.3](file:///c:/Users/shpy2/Documents/OneBrain/docs/specs/KQL_SPEC.md) | CREATE FROM TEXT note tương tự | ✅ OK |
| KQL_SPEC.md §2.1 FIND | ❌ Chưa có ví dụ query sử dụng `encoding_status` | 🟡 GAP |

---

## Gaps cần bổ sung

### Gap 1: Test coverage (MEDIUM)

**Hiện tại**: 0 tests cho `encoding_status` trong `ku-kql`.

**Cần thêm**: 3-4 tests kiểm chứng:
1. `FIND WHERE k.encoding_status = "Full"` → chỉ trả KU có status Full
2. `FIND WHERE k.encoding_status != "Raw"` → trả tất cả trừ Raw
3. `RETURN k.encoding_status` → kết quả chứa đúng status text
4. `ORDER BY k.encoding_status` → sort alphabetical (Full < Part < Raw < Self)

### Gap 2: KQL_SPEC.md ví dụ (LOW)

**Thiếu**: Ví dụ thực tế dùng `encoding_status` trong FIND.

**Thêm**: 1 paragraph trong §2.1 FIND section:

```kql
-- Lọc chỉ KU đã verified hoàn toàn
FIND (k:KU) WHERE k.encoding_status = "Full" RETURN k.gene_type, k.trust_score

-- Tìm KU đang chờ verification
FIND (k:KU) WHERE k.encoding_status = "Part" RETURN k.cid, k.encoding_status

-- Loại trừ KU chưa encode (raw text)
FIND (k:KU) WHERE k.encoding_status != "Raw" AND k.gene_type = "Definition" 
  RETURN k.text LIMIT 10
```

---

## Đề xuất thay đổi

| # | Target | Loại | Nội dung | Priority |
|---|--------|------|----------|----------|
| 1 | `ku-kql` tests | **Code** | Thêm 4 encoding_status tests (parser + executor) | MEDIUM |
| 2 | `KQL_SPEC.md` §2.1 | **Doc** | Thêm ví dụ FIND + encoding_status | LOW |

> [!NOTE]
> **Không cần sửa code logic** — kiến trúc field-driven của KQL tự động hỗ trợ mọi field mới trong `extract_field()`. Đây là lợi thế thiết kế lớn.
