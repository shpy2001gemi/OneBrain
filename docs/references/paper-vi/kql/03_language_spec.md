# 3. Language Specification

Mục này trình bày đặc tả ngôn ngữ KQL hoàn chỉnh: cú pháp, ngữ nghĩa, hệ thống kiểu dữ liệu và hành vi vận hành cho cả sáu loại truy vấn.

## 3.1 Cấu trúc Từ vựng

### 3.1.1 Từ khóa

Các từ khóa KQL **không phân biệt chữ hoa chữ thường**: `FIND`, `find`, và `Find` là tương đương nhau. Lựa chọn thiết kế này tối đa hóa khả năng tiếp cận cho người dùng chưa quen thuộc với các ngôn ngữ lập trình phân biệt chữ hoa chữ thường.

**Từ khóa dành riêng** (33):

| Phân loại (Category) | Từ khóa (Keywords) |
|----------|----------|
| Loại truy vấn | `FIND`, `CREATE`, `UPDATE`, `DEPRECATE`, `WATCH`, `EXPLAIN` |
| Mệnh đề | `WHERE`, `SCOPE`, `RETURN`, `ORDER`, `BY`, `LIMIT`, `SET`, `SIGNED`, `ON`, `NOTIFY`, `REASON`, `AS` |
| Toán tử | `AND`, `OR`, `NOT`, `EXISTS` |
| Phạm vi | `LOCAL`, `NEIGHBORS`, `CLUSTER`, `DHT`, `GLOBAL`, `AUTO` |
| Hàm gom tụ | `COUNT`, `SUM`, `AVG`, `MIN`, `MAX` |
| Thứ tự | `ASC`, `DESC` |
| Sự kiện | `CREATE`, `UPDATE`, `DEPRECATE`, `ANY` |
| Boolean | `TRUE`, `FALSE` |

### 3.1.2 Định danh (Identifiers)

Các định danh (identifiers) khớp với `[a-zA-Z0-9_]+`. Chúng đóng vai trò là bí danh nút (`k` trong `(k:KU)`), tên trường và khóa thuộc tính.

### 3.1.3 Hằng (Literals)

| Kiểu dữ liệu | Cú pháp | Ví dụ | Giá trị AST (AST Value) |
|------|--------|----------|-----------|
| Số nguyên (Integer) | `-?[0-9]+` | `8000`, `-100`, `0` | `Value::Integer(i64)` |
| Số thực (Float) | `-?[0-9]+\.[0-9]+` | `0.95`, `-3.14` | `Value::Float(f64)` |
| Chuỗi (String) | `"[^"]*"` | `"Fact"`, `"did:key:z6Mk..."` | `Value::Text(String)` |
| Logic (Boolean) | `true \| false` | `true`, `FALSE` | `Value::Bool(bool)` |

### 3.1.4 Đường dẫn Trường (Field Paths)

Các đường dẫn trường dấu chấm (dotted field paths) truy cập vào các trường KU lồng nhau: `k.trust_score`, `k.trust.confidence`, `k.epistemic_status`. Struct `FieldPath` chứa một `Vec<String>` đại diện cho các phân đoạn đường dẫn.

**Các trường có thể truy cập** (28 trường, được ánh xạ tới cấu trúc KuRuntime):

*Các Trường DNA Cốt lõi (Core DNA Fields):*

| Đường dẫn Trường (Field Path) | Nguồn KU (KU Source) | Kiểu dữ liệu (Type) |
|------------|-----------|------|
| `k.gene_type` | `ku.gene_type()` → name | Text |
| `k.primary_concept` | `ku.primary_concept()` | Integer (u64 → i64) |
| `k.certainty` | `ku.certainty()` | Integer (u16 → i64) |
| `k.difficulty` | `ku.difficulty()` | Integer (u16 → i64) |
| `k.instruction_count` | `ku.instruction_count()` | Integer |
| `k.has_triple` | `ku.has_triple()` | Bool |
| `k.has_step` | `ku.has_step()` | Bool |
| `k.wire_size` | `ku.wire_size()` | Integer |

*Các Trường Epigenetics (Epigenetics Fields):*

| Đường dẫn Trường (Field Path) | Nguồn KU (KU Source) | Kiểu dữ liệu (Type) |
|------------|-----------|------|
| `k.trust_score` | `ku.epi.trust.trust_score` | Integer (u16 → i64) |
| `k.confidence` | `ku.epi.trust.confidence` | Integer (u16 → i64) |
| `k.verification_level` | `ku.epi.trust.verification_level` | Integer (u8 → i64) |
| `k.corroboration_count` | `ku.epi.trust.corroboration_count` | Integer (u16 → i64) |
| `k.challenge_count` | `ku.epi.trust.challenge_count` | Integer (u16 → i64) |
| `k.error_susceptibility` | `ku.epi.trust.error_susceptibility` | Integer (u16 → i64) |
| `k.bond_count` | `ku.bond_count()` | Integer |
| `k.epistemic_status` | `ku.epi.epistemic_status` | Text (11 values) |
| `k.evidence_type` | `ku.epi.evidence_type` | Integer (u8 → i64) |

*Các Trường Tín hiệu PoMV (PoMV Signal Fields):*

| Đường dẫn Trường (Field Path) | Nguồn KU (KU Source) | Kiểu dữ liệu (Type) |
|------------|-----------|------|
| `k.metabolic_rate` | `ku.epi.trust.metabolic_rate` | Integer (u16 → i64) |
| `k.prediction_score` | `ku.epi.trust.prediction_score` | Integer (u16 → i64) |
| `k.entropy_at_creation` | `ku.epi.trust.entropy_at_creation` | Integer (u16 → i64) |
| `k.survival_score` | `ku.epi.trust.survival_score` | Integer (u16 → i64) |
| `k.synaptic_centrality` | `ku.epi.trust.synaptic_centrality` | Integer (u16 → i64) |
| `k.niche_fitness` | `ku.epi.trust.niche_fitness` | Integer (u16 → i64) |

*Các Trường Biểu đạt (Expression Fields):*

| Đường dẫn Trường (Field Path) | Nguồn KU (KU Source) | Kiểu dữ liệu (Type) |
|------------|-----------|------|
| `k.text` | `ku.expr.text` | Text (Option) |

*Các Trường Hệ thống (System Fields):*

| Đường dẫn Trường (Field Path) | Nguồn KU (KU Source) | Kiểu dữ liệu (Type) |
|------------|-----------|------|
| `k.epi` | always present (v6) | Bool |
| `k.expression` | `ku.expr.is_some()` | Bool |
| `k.encoding_status` | `ku.encoding_status` | Text (Raw/Self/Part/Full) |
| `k.cid` | `ku.cid` | Text (hex) |

## 3.2 Các Loại Truy vấn

### 3.2.1 FIND — Truy vấn Đọc

```
FIND <pattern>
  [WHERE <condition>]
  [SCOPE <scope>]
  [RETURN <return_exprs>]
  [ORDER BY <order_exprs>]
  [LIMIT <n>]
```

**Ngữ nghĩa:** Khớp tất cả các KU thỏa mãn mẫu và các điều kiện, gom tụ tùy chọn, sắp xếp và giới hạn kết quả. Đây là thao tác đọc chính.

**Ví dụ:**

```sql
-- Đơn giản: tìm tất cả KU
FIND (k:KU)

-- Có bộ lọc: tri thức độ tin cậy cao
FIND (k:KU) WHERE k.trust_score > 8000 SCOPE CLUSTER LIMIT 10

-- Gom tụ: đếm và tính trung bình độ tin cậy
FIND (k:KU) RETURN COUNT(k.id), AVG(k.trust_score)

-- Phức tạp: các điều kiện kết hợp kèm sắp xếp
FIND (k:KU) WHERE k.trust_score > 5000 AND k.certainty >= 9000
  ORDER BY k.trust_score DESC LIMIT 20

-- Khớp thuộc tính
FIND (k:KU {gene_type: "Fact", certainty: 9500})

-- Kiểm tra sự tồn tại
FIND (k:KU) WHERE EXISTS k.trust

-- Các truy vấn concept
FIND (c:Concept)
```

### 3.2.2 CREATE — Khởi tạo Tri thức

```
CREATE <pattern>
  [SIGNED BY <signer>]
```

**Ngữ nghĩa:** Tạo một Knowledge Unit mới với các thuộc tính được chỉ định. Mệnh đề `SIGNED BY` xác định tác giả (định dạng DID). Loại gen mặc định là Fact; các loại được hỗ trợ bao gồm Fact, Procedure, và Narrative.

```sql
-- Tạo một fact
CREATE (k:KU {body: "Water boils at 100°C"}) SIGNED BY "did:key:z6Mk..."

-- Tạo một procedure
CREATE (k:KU {gene_type: "Procedure"}) SIGNED BY "author_id"
```

**Thực thi:** Bộ thực thi (executor) xây dựng một `KnowledgeUnit` từ các thuộc tính, gán siêu dữ liệu tin cậy mặc định (epistemic_status = Observation, evidence_type = Anecdotal, trust_score = 1000, confidence = 5000) và chèn vào kho lưu trữ cục bộ.

### 3.2.3 UPDATE — Sửa đổi Tri thức

```
UPDATE <pattern>
  SET <assignments>
  [WHERE <condition>]
  SIGNED BY <signer>
```

**Ngữ nghĩa:** Sửa đổi các trường của các KU hiện có khớp với điều kiện. Mệnh đề `SIGNED BY` là **bắt buộc** — tất cả các sửa đổi phải có thể quy kết trách nhiệm.

```sql
-- Cập nhật điểm tin cậy cho một concept cụ thể
UPDATE (k:KU) SET k.trust_score = 9000
  WHERE k.concept_id = 42 SIGNED BY "did:ob:abc"

-- Cập nhật nhiều trường
UPDATE (k:KU) SET k.trust_score = 8500, k.confidence = 9000
  WHERE k.trust_score < 5000 SIGNED BY "did:ob:reviewer"
```

**Thực thi:** Bộ thực thi lặp qua tất cả các KU, đánh giá điều kiện WHERE và áp dụng các phép gán cho các KU khớp. Trả về `affected_count`.

### 3.2.4 DEPRECATE — Hủy bỏ Tri thức

```
DEPRECATE <pattern>
  [WHERE <condition>]
  REASON <reason_string>
  SIGNED BY <signer>
```

**Ngữ nghĩa:** Đánh dấu các KU là deprecated (hủy bỏ) — không bị xóa hoàn toàn, mà được gắn cờ là không còn có thẩm quyền. Cả `REASON` và `SIGNED BY` đều là **bắt buộc**, đảm bảo tính nguồn gốc của việc hủy bỏ.

```sql
-- Hủy bỏ tri thức đã bị thay thế
DEPRECATE (k:KU) WHERE k.concept_id = 42
  REASON "Superseded by newer research" SIGNED BY "did:ob:abc"
```

**Thực thi:** Bộ thực thi đặt `trust_score = 0`, `verification_level = 0`, và `epistemic_status = Rumor` cho các KU khớp. KU vẫn nằm trong kho lưu trữ cùng với siêu dữ liệu hủy bỏ của nó — cho phép phân tích lịch sử và khả năng hoàn tác hủy bỏ (undeprecation) trong tương lai.

**Cơ sở thiết kế:** Sử dụng DEPRECATE thay vì DELETE vì nguồn gốc tri thức rất quan trọng. Một KU đã bị hủy bỏ vẫn đóng góp vào lịch sử của đồ thị tri thức và có thể được tham chiếu trong các liên kết "superseded by" (bị thay thế bởi).

### 3.2.5 WATCH — Truy vấn Phản ứng Liên tục

```
WATCH FIND <pattern>
  [WHERE <condition>]
  [ON <event>]
  [NOTIFY <endpoint>]
```

**Ngữ nghĩa:** Đăng ký một truy vấn liên tục kích hoạt thông báo khi các KU phù hợp xuất hiện. Điều này hoàn toàn khác biệt với truy vấn FIND tại một thời điểm nhất định — WATCH mang tính phản ứng, hướng sự kiện và liên tục.

**Sự kiện (Events):**

| Sự kiện (Event) | Kích hoạt khi... |
|-------|-------------|
| `CREATE` | Một KU mới khớp với bộ lọc được khởi tạo |
| `UPDATE` | Một KU hiện có khớp với bộ lọc bị sửa đổi |
| `DEPRECATE` | Một KU phù hợp bị hủy bỏ (deprecated) |
| `ANY` | Bất kỳ sự kiện nào ở trên (mặc định) |

```sql
-- Theo dõi tri thức mới có độ tin cậy cao
WATCH FIND (k:KU) WHERE k.trust_score > 7000
  ON CREATE NOTIFY "callback://agent"

-- Theo dõi bất kỳ thay đổi nào đối với một concept
WATCH FIND (k:KU) WHERE k.concept_id = 42
  ON ANY NOTIFY "ws://localhost:8080/updates"

-- Theo dõi sự kiện cụ thể
WATCH FIND (c:Concept) ON UPDATE
```

**Thực thi:** Bộ thực thi lưu trữ đăng ký WATCH (trả về một `WatchId`). Trên mỗi thao tác `insert()` hoặc `update()` tiếp theo, bộ thực thi đánh giá tất cả các watch đã đăng ký đối với các KU bị ảnh hưởng và kích hoạt thông báo cho các kết quả khớp.

### 3.2.6 EXPLAIN — Nội soi Kế hoạch Truy vấn

```
EXPLAIN <any_query>
```

**Ngữ nghĩa:** Thay vì thực thi truy vấn, trả về kế hoạch thực thi của nó — phạm vi (scope), chiến lược (strategy), kết quả ước tính và các chỉ mục được sử dụng. Điều này rất cần thiết cho việc tối ưu hóa truy vấn và gỡ lỗi trong môi trường phân tán.

```sql
-- Nội soi một truy vấn tìm kiếm cục bộ
EXPLAIN FIND (k:KU) WHERE k.confidence > 50 SCOPE DHT

-- Nội soi một đăng ký watch
EXPLAIN WATCH FIND (k:KU) WHERE k.trust_score > 8000 ON CREATE
```

**Đầu ra kế hoạch truy vấn (Query plan output):**

| Trường (Field) | Mô tả (Description) | Ví dụ (Example) |
|-------|-------------|---------|
| `scope` | Phạm vi thực thi | `Dht` |
| `strategy` | Chiến lược thực thi | `kademlia_lookup` |
| `estimated_results` | Số lượng kết quả khớp ước tính | `1,247` |
| `indexes_used` | Các chỉ mục được tham chiếu | `["trust_score_index", "concept_id_index"]` |

**Ánh xạ chiến lược (Strategy mapping):**

| Phạm vi (Scope) | Chiến lược (Strategy) |
|-------|----------|
| `LOCAL` | `local_scan` |
| `NEIGHBORS` | `neighbor_broadcast` |
| `CLUSTER` | `super_peer_route` |
| `DHT` | `kademlia_lookup` |
| `GLOBAL` | `global_flood` |
| `AUTO` | `auto_escalation` |

## 3.3 Hệ thống Phạm vi (Scope System)

Mệnh đề SCOPE là tính năng đặc trưng nhất của KQL — một cơ chế hạng nhất để kiểm soát việc phân phối truy vấn:

| Phạm vi (Scope) | Cấp độ (Level) | TTL | Chiến lược (Strategy) | Độ trễ (Latency) | Tính toàn vẹn (Completeness) |
|-------|:-----:|:---:|----------|:-------:|:------------:|
| `LOCAL` | 0 | 0 | Chỉ thực thi trên chính nó | <1ms | Thấp nhất |
| `NEIGHBORS` | 1 | 1 | Nút peer SWIM 1-hop (fanout=5) | ~50ms | Thấp |
| `CLUSTER` | 2 | 3 | Định tuyến qua các siêu nút (super-peers) | ~100ms | Trung bình |
| `DHT` | 3 | 8 | Tra cứu khóa concept Kademlia | ~200ms | Cao |
| `SEMANTIC` | 4 | 5 | Dấu vết pheromone stigmergy | ~150ms | Cao* |
| `GLOBAL` | 5 | 12 | Đi bộ ngẫu nhiên + lan truyền TTL | ~500ms+ | Cao nhất |
| `AUTO` | — | — | Leo thang tăng dần L0→L5 | Thay đổi | Thích ứng |

*Bảng 3: Các cấp độ phạm vi KQL. SEMANTIC (L4) có tính toàn vẹn thay đổi — cao đối với các chủ đề phổ biến, thấp đối với các truy vấn mới.*

**Phạm vi `AUTO`** (mặc định): Công cụ truy vấn bắt đầu từ LOCAL và leo thang dần cho đến khi tìm thấy đủ kết quả hoặc tất cả các phạm vi đã cạn kiệt. Đây là phạm vi được khuyên dùng cho hầu hết các truy vấn — người dùng nhận được phản hồi nhanh nhất có thể mà không cần điều chỉnh phân phối thủ công.

## 3.4 Điều kiện (Mệnh đề WHERE)

Các điều kiện KQL tạo thành một cây biểu thức boolean:

```
Condition ::= Comparison | And | Or | Not | Exists | Contains

Comparison: field op value
  field:  dotted path (e.g., k.trust_score)
  op:     = | != | > | >= | < | <=
  value:  integer | float | string | boolean

And:      condition AND condition
Or:       condition OR condition  
Not:      NOT condition
Exists:   EXISTS field
Contains: field CONTAINS value
```

**Độ ưu tiên của toán tử** (từ cao đến thấp): NOT > AND > OR. Các điều kiện kết hợp bên phải (associate right): `A AND B AND C` được phân tích cú pháp dưới dạng `A AND (B AND C)`.

**Đánh giá**: Hàm `evaluate_condition(ku, condition)` đánh giá đệ quy cây điều kiện đối với một KU, trích xuất các giá trị trường thông qua `extract_field_value(ku, field_path)` và so sánh với toán tử được chỉ định.

## 3.5 Các Hàm Gom tụ

KQL hỗ trợ 5 hàm gom tụ, được tính toán trong quá trình thực thi FIND:

| Hàm (Function) | Cú pháp (Syntax) | Đầu vào (Input) | Đầu ra (Output) | Mô tả (Description) |
|----------|--------|-------|--------|-------------|
| `COUNT` | `COUNT(k.field)` | Any | Integer | Đếm các giá trị phi null |
| `SUM` | `SUM(k.field)` | Numeric | Integer/Float | Tổng của tất cả các giá trị |
| `AVG` | `AVG(k.field)` | Numeric | Float | Trung bình cộng |
| `MIN` | `MIN(k.field)` | Numeric | Integer/Float | Giá trị nhỏ nhất |
| `MAX` | `MAX(k.field)` | Numeric | Integer/Float | Giá trị lớn nhất |

Các hàm gom tụ được tính toán trên **tập kết quả đã lọc** (sau WHERE, trước LIMIT):

$$\text{AVG}(f) = \frac{1}{N} \sum_{i=1}^{N} f(ku_i) \quad \text{where } ku_i \in \text{filtered\_results}$$

```sql
-- Nhiều phép gom tụ trong một truy vấn
FIND (k:KU) WHERE k.trust_score > 1000
  RETURN COUNT(k.id) AS total,
         AVG(k.trust_score) AS avg_trust,
         MIN(k.certainty) AS min_cert,
         MAX(k.certainty) AS max_cert
```

## 3.6 Khớp Mẫu Đồ thị

KQL hỗ trợ khớp mẫu đồ thị với các nút có kiểu (typed nodes) và các cạnh có hướng (directed edges):

### 3.6.1 Mẫu Nút

```
NodePattern ::= '(' [alias ':'] label ['{' properties '}'] ')'
```

**Nhãn (Labels):** `KU` (Knowledge Unit) | `Concept`

```sql
-- Nút KU có bí danh
(k:KU)

-- KU với bộ lọc thuộc tính
(k:KU {gene_type: "Fact", certainty: 9500})

-- Nút Concept
(c:Concept)
```

### 3.6.2 Mẫu Cạnh

```
EdgePattern ::= '-[' [alias ':'] edge_type ']->' | '<-[' ... ']-' | '-[' ... ']-'
```

**Hướng (Directions):**
- `->` (Hướng ra - Outgoing)
- `<-` (Hướng vào - Incoming)
- `-` (Không hướng - Undirected)

```sql
-- Tìm các KU được kết nối bởi một liên kết Causes
FIND (a:KU)-[r:Causes]->(b:KU) WHERE a.trust_score > 8000

-- Tìm các mối quan hệ concept
FIND (c1:Concept)-[r:PartOf]->(c2:Concept)
```

Các kiểu cạnh tương ứng với các kiểu liên kết (Bond types) của KU — tất cả 33 kiểu liên kết đều có thể truy vấn được, bao gồm `PartOf`, `Causes`, `Enables`, `Contradicts`, `AnalogyOf`, `Inspires`, và `EvolvesInto`.

## 3.7 Ngữ pháp Hình thức (EBNF)

```ebnf
query           = explain_query | watch_query | update_query
                | deprecate_query | find_query | create_query ;

find_query      = "FIND" pattern [where_clause] [scope_clause]
                  [return_clause] [order_clause] [limit_clause] ;

create_query    = "CREATE" pattern [signed_clause] ;

update_query    = "UPDATE" pattern "SET" assignments
                  [where_clause] signed_clause ;

deprecate_query = "DEPRECATE" pattern [where_clause]
                  reason_clause signed_clause ;

watch_query     = "WATCH" find_query [on_clause] [notify_clause] ;

explain_query   = "EXPLAIN" (find_query | create_query | update_query
                  | deprecate_query | watch_query) ;

pattern         = "(" [identifier ":"] node_label [property_map] ")" ;
node_label      = "KU" | "Concept" ;
property_map    = "{" property ("," property)* "}" ;
property        = identifier ":" value ;

where_clause    = "WHERE" condition ;
condition       = simple_cond [("AND" | "OR") condition] ;
simple_cond     = "EXISTS" field_path
                | field_path comp_op value ;
comp_op         = "=" | "!=" | ">" | ">=" | "<" | "<=" ;

scope_clause    = "SCOPE" scope ;
scope           = "LOCAL" | "NEIGHBORS" | "CLUSTER" | "DHT"
                | "GLOBAL" | "AUTO" ;

return_clause   = "RETURN" return_expr ("," return_expr)* ;
return_expr     = aggregate_expr | field_path | identifier ;
aggregate_expr  = agg_func "(" field_path ")" ["AS" identifier] ;
agg_func        = "COUNT" | "SUM" | "AVG" | "MIN" | "MAX" ;

order_clause    = "ORDER" "BY" order_expr ("," order_expr)* ;
order_expr      = field_path ["ASC" | "DESC"] ;

limit_clause    = "LIMIT" integer ;
signed_clause   = "SIGNED" "BY" (quoted_string | identifier) ;
reason_clause   = "REASON" quoted_string ;
on_clause       = "ON" watch_event ;
watch_event     = "CREATE" | "UPDATE" | "DEPRECATE" | "ANY" ;
notify_clause   = "NOTIFY" quoted_string ;

assignments     = assignment ("," assignment)* ;
assignment      = field_path "=" value ;
field_path      = identifier ("." identifier)* ;
value           = quoted_string | "true" | "false" | number ;
number          = ["-"] digit+ ["." digit+] ;
quoted_string   = '"' [^"]* '"' ;
identifier      = [a-zA-Z0-9_]+ ;
```

*Hình 2: Ngữ pháp KQL hoàn chỉnh trong Dạng Backus-Naur Mở rộng (EBNF).*

---

## References

[1] ISO/IEC 9075:2023, "Information technology — Database languages — SQL," 2023.
