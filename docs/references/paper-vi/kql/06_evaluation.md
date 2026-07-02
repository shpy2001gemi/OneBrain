# 6. Đánh giá (Evaluation)

## 6.1 Tóm tắt Hiện thực

### 6.1.1 Kiểm kê Mô-đun (Module Inventory)

**KQL Core (ku-kql crate):**

| Mô-đun (Module) | Tệp tin (File) | Số dòng code (LOC) | Mục đích (Purpose) |
|--------|------|----:|---------|
| AST | [ast.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/ast.rs) | 402 | Hơn 30 kiểu nút AST: Query, Pattern, Condition, Value, v.v. |
| Parser | [parser.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/parser.rs) | 1,456 | Bộ phân tích cú pháp nom-based recursive descent, 6 loại truy vấn |
| Executor | [executor.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/executor.rs) | 1,263 | Bộ thực thi cục bộ: FIND/CREATE/UPDATE/DEPRECATE/WATCH/EXPLAIN |
| Storage | [storage.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/storage.rs) | 447 | Kho lưu trữ bền vững ACID được hỗ trợ bởi redb, chỉ mục BLAKE3 CID |
| lib.rs | [lib.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/lib.rs) | 15 | Xuất mô-đun (Module exports) |
| **Tổng phụ** | | **3,583** | |

*Bảng 5: Các mô-đun cốt lõi của KQL.*

**Distributed Query Engine (ku-net/query):**

| Mô-đun (Module) | Tệp tin (File) | Số dòng code (LOC) | Mục đích (Purpose) |
|--------|------|----:|---------|
| ConceptIndex | [index.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/index.rs) | 178 | Khóa concept VacuumFilter + BLAKE3, xuất bản lên DHT |
| QueryMessages | [messages.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/messages.rs) | 208 | Định dạng dây (Wire format): QueryForward(0x50), QueryResponse(0x51), QueryCancel(0x52) |
| QueryRouter | [router.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/router.rs) | 417 | Công cụ leo thang phạm vi 6 lớp |
| ResultMerger | [merger.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/merger.rs) | 252 | Khử trùng lặp + xếp hạng trust×scope |
| WatchEngine | [watch.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/watch.rs) | 392 | Các truy vấn liên tục, bộ lọc sự kiện, lan truyền TTL |
| GapDetector | [gaps.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/discovery/gaps.rs) | 303 | Các concept mồ côi, độ tin cậy thấp, thiếu bằng chứng |
| BridgeFinder | [bridges.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/discovery/bridges.rs) | 198 | Phát hiện cầu nối xuyên miền Swanson ABC |
| SerendipityEngine | [serendipity.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/discovery/serendipity.rs) | 272 | Các ẩn số chưa biết qua tính điểm relevance×novelty |
| QueryCache | [cache.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/cache.rs) | 301 | LRU cache, chuẩn hóa KQL gán khóa BLAKE3, hết hạn TTL |
| PheromoneLearner | [learning.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/learning.rs) | 314 | Tăng cường lấy cảm hứng từ ACO cho định tuyến phạm vi |
| mod.rs (query) | [mod.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/mod.rs) | 15 | Tái xuất mô-đun |
| mod.rs (discovery) | [mod.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/discovery/mod.rs) | 9 | Tái xuất mô-đun khám phá |
| **Tổng phụ** | | **~2,859** | |

*Bảng 6: Các mô-đun công cụ truy vấn phân tán.*

**Tổng cộng:** ~3.583 dòng code (cốt lõi) + ~2.859 dòng code (phân tán) ≈ **~6,442 dòng code** trên 17 mô-đun.

### 6.1.2 Thư viện Phụ thuộc

| Thư viện phụ thuộc (Dependency) | Mục đích (Purpose) | Thuần Rust? (Pure Rust?) |
|-----------|---------|:----------:|
| `nom` 7.x | Parser combinator | ✅ |
| `ku-core` | Các kiểu KU và bộ mã hóa/giải mã (codec) | ✅ |
| `blake3` 1.x | Băm nội dung (CID, khóa bộ nhớ đệm) | ✅ |
| `serde` + `ciborium` | Tuần tự hóa CBOR | ✅ |
| `redb` 2.x | Kho lưu trữ bền vững ACID (bị giới hạn tính năng - feature-gated) | ✅ |

Tất cả các thư viện phụ thuộc đều là thuần Rust — cho phép biên dịch chéo sang các mục tiêu di động và WebAssembly mà không cần các yêu cầu về bộ công cụ C (C toolchain).

## 6.2 Độ bao phủ Kiểm thử

### 6.2.1 Các Bài Kiểm thử Parser (34 bài kiểm thử)

| Bài kiểm thử (Test) | Nội dung xác thực (Validates) |
|------|-----------|
| `test_parse_simple_find` | Phân tích cú pháp cơ bản FIND (k:KU) |
| `test_parse_find_with_where` | Điều kiện WHERE với phép so sánh |
| `test_parse_find_with_scope_and_limit` | Các mệnh đề SCOPE và LIMIT |
| `test_parse_find_with_and_condition` | Cấu thành boolean AND |
| `test_parse_find_with_return` | Mệnh đề RETURN với các đường dẫn trường |
| `test_parse_find_with_order` | Mệnh đề ORDER BY DESC |
| `test_parse_find_with_properties` | Bản đồ thuộc tính {key: value} |
| `test_parse_create` | CREATE với các thuộc tính và SIGNED BY |
| `test_parse_explain` | EXPLAIN bao bọc FIND |
| `test_parse_aggregate` | COUNT/SUM/AVG/MIN/MAX với bí danh AS |
| `test_parse_error_invalid` | Từ chối đầu vào không phải KQL (ví dụ: SQL) |
| `test_parse_concept_label` | Nhãn nút Concept |
| `test_parse_exists_condition` | Kiểm tra sự tồn tại của trường EXISTS |
| `test_parse_negative_number` | Các giá trị số nguyên âm |
| `test_parse_float_value` | Các giá trị so sánh số thực |
| `test_parse_all_scopes` | Tất cả 6 từ khóa scope |
| `test_parse_watch_simple` | WATCH FIND cơ bản |
| `test_parse_watch_full` | WATCH với ON CREATE NOTIFY |
| `test_parse_watch_on_update` | Sự kiện WATCH ON UPDATE |
| `test_parse_watch_on_deprecate` | Sự kiện WATCH ON DEPRECATE |
| `test_parse_update` | UPDATE SET WHERE SIGNED BY |
| `test_parse_deprecate` | DEPRECATE REASON SIGNED BY |
| `test_parse_or_condition` | Toán tử logic OR |
| `test_parse_multiple_assignments` | SET a=1, b=2 |
| `test_parse_no_alias` | Nút không có bí danh (:KU) |
| `test_parse_case_insensitive` | Chữ hoa chữ thường hỗn hợp FiNd |
| `test_parse_trailing_input_rejected` | Từ chối dữ liệu rác ở cuối |
| `test_parse_multiple_aggregations` | Nhiều phép gom tụ trong RETURN |

### 6.2.2 Các Bài Kiểm thử Executor (24 bài kiểm thử)

| Bài kiểm thử (Test) | Nội dung xác thực (Validates) |
|------|-----------|
| `test_find_all` | FIND không có WHERE trả về tất cả |
| `test_find_where_gt` | Lọc theo phép so sánh lớn hơn |
| `test_find_where_and` | Đánh giá điều kiện AND |
| `test_find_with_limit` | Cắt bớt theo LIMIT sau khi đếm tổng số |
| `test_find_order_by_desc` | Sắp xếp giảm dần theo trường |
| `test_find_exists_trust` | Điều kiện EXISTS trên trường tùy chọn |
| `test_find_scope_local` | Phạm vi được đặt chính xác thành Local |
| `test_empty_result` | Kết quả trống khi không có kết quả khớp |
| `test_aggregation_count` | Hàm COUNT |
| `test_aggregation_avg` | Hàm AVG (kết quả số thực) |
| `test_aggregation_sum` | Hàm SUM |
| `test_aggregation_min_max` | Các hàm MIN và MAX |
| `test_create_execution` | CREATE chèn KU với gene mặc định |
| `test_create_procedure` | CREATE với gene_type="Procedure" |
| `test_update_basic` | UPDATE sửa đổi tất cả các KU |
| `test_update_with_where` | UPDATE chỉ sửa đổi các KU khớp |
| `test_deprecate_basic` | DEPRECATE đặt độ tin cậy về 0 |
| `test_deprecate_with_where` | DEPRECATE với bộ lọc WHERE |
| `test_watch_register` | WATCH trả về WatchId |
| `test_watch_check_match` | check_watches so khớp chính xác |
| `test_unwatch` | unwatch loại bỏ đăng ký |
| `test_explain_find` | EXPLAIN trả về QueryPlan |
| `test_explain_auto_scope` | EXPLAIN với phạm vi AUTO |

### 6.2.3 Các Bài Kiểm thử Storage (6 bài kiểm thử)

| Bài kiểm thử (Test) | Nội dung xác thực (Validates) |
|------|-----------|
| `test_open_create_db` | Khởi tạo cơ sở dữ liệu |
| `test_put_and_get` | Chu trình chèn + truy xuất |
| `test_has` | Kiểm tra sự tồn tại (hiện diện/vắng mặt) |
| `test_delete` | Xóa + xác minh việc loại bỏ |
| `test_count_and_get_all` | Đếm + quét toàn bộ |
| `test_deterministic_cid` | Cùng nội dung → cùng CID (idempotent) |

### 6.2.4 Các Bài Kiểm thử Truy vấn Phân tán (Tổng cộng 66+ bài trên toàn bộ các mô-đun)

| Mô-đun (Module) | Số bài kiểm thử (Tests) | Các kịch bản chính (Key Scenarios) |
|--------|:-----:|---------------|
| ConceptIndex | 7 | Chèn, tra cứu, tích hợp VacuumFilter, xuất bản lên DHT |
| QueryMessages | 5 | Chu trình định dạng dây, mã hóa tiêu đề, mã hóa phạm vi |
| QueryRouter | 6 | Leo thang phạm vi, kiểm soát fanout, giảm TTL |
| ResultMerger | 7 | Khử trùng lặp, xếp hạng độ tin cậy, độ gần phạm vi, gom tụ đa nguồn |
| WatchEngine | 9 | Đăng ký, khớp, hủy đăng ký, hết hạn TTL, lọc sự kiện |
| GapDetector | 6 | Phát hiện mồ côi, gắn cờ độ tin cậy thấp, tạo gợi ý |
| BridgeFinder | 3 | Phát hiện cầu nối xuyên miền, tính điểm, mô hình Swanson ABC |
| SerendipityEngine | 6 | Khớp hồ sơ sở thích, tính điểm tính mới lạ, phát hiện điểm tối ưu |
| QueryCache | 9 | Chèn, trúng, trượt, thu hồi LRU, hết hạn TTL, thống kê, chuẩn hóa |
| PheromoneLearner | 8 | Tăng cường, phạt, ưu tiên phạm vi, phân rã, tín hiệu tương tác |

### 6.2.5 Các Bài Kiểm thử Tích hợp và Áp lực (13 bài kiểm thử)

Bộ kiểm thử tích hợp ([query_integration.rs](file:///c:/Users/shpy2/Documents/OneBrain/tests/query_integration.rs), 330 dòng code) xác thực hành vi của toàn bộ đường ống đầu-cuối (end-to-end pipeline):

| Bài kiểm thử (Test) | Kịch bản (Scenario) | Quy mô (Scale) |
|------|----------|-------|
| `test_pipeline_find_local` | KQL → parse → thực thi → kết quả | 10 KUs |
| `test_pipeline_create_and_find` | CREATE sau đó FIND | 5 KUs |
| `test_pipeline_update_verify` | UPDATE sau đó xác minh các thay đổi | 20 KUs |
| `test_pipeline_deprecate_verify` | DEPRECATE sau đó xác minh đặt về 0 | 10 KUs |
| `test_pipeline_watch_trigger` | WATCH → chèn → check_watches | 50 KUs |
| `test_pipeline_explain_accuracy` | EXPLAIN khớp với thực thi thực tế | 100 KUs |
| `test_pipeline_aggregation_accuracy` | Gom tụ khớp với tính toán thủ công | 1K KUs |
| `stress_10k_concepts` | ConceptIndex với 10.000 concept | 10K |
| `stress_1000_watches` | WatchEngine với 1.000 truy vấn liên tục | 1K watches |
| `stress_500_kus_insert_query` | Chèn 500 KU, chạy 100 truy vấn | 500 KUs |
| `stress_cache_eviction` | Điền đầy bộ nhớ đệm vượt dung lượng, xác minh LRU | 2K queries |
| `stress_bridge_finder` | BridgeFinder trên 20 miền (domains) | 500 KUs |
| `stress_concurrent_queries` | 50 truy vấn đồng thời | 50 song song |

## 6.3 So sánh với các Ngôn ngữ Truy vấn Hiện tại

| Tính năng (Feature) | SQL | SPARQL | Cypher | **KQL** |
|---------|-----|--------|--------|---------|
| **LOC** | Không áp dụng (spec) | Không áp dụng (spec) | Không áp dụng (spec) | ~3,583 (core) + ~2,859 (distributed) |
| **Mô hình dữ liệu** | Quan hệ | RDF triples | Property graph | Knowledge Units |
| **Bộ phân tích cú pháp (Parser)** | Yacc/Bison | Custom | ANTLR | nom (Rust) |
| **Các loại truy vấn** | SELECT/INSERT/UPDATE/DELETE | SELECT/CONSTRUCT/ASK/DESCRIBE | MATCH/CREATE/MERGE/DELETE | FIND/CREATE/UPDATE/DEPRECATE/WATCH/EXPLAIN |
| **Phân phối** | Federated SQL | SERVICE | Không | SCOPE (6 cấp độ) |
| **Các truy vấn liên tục** | Triggers (giới hạn) | Không | Không | WATCH (hạng nhất) |
| **Nhận biết độ tin cậy** | Không | Không | Không | Có (trust_score, epistemic) |
| **Hủy bỏ (Deprecation)** | DELETE (vĩnh viễn) | DELETE | DELETE | DEPRECATE (đảo ngược được, có nguồn gốc) |
| **Khám phá** | Không | Không | Không | 3 công cụ (Gap/Bridge/Serendipity) |
| **Học máy (Learning)** | Gợi ý của bộ tối ưu hóa truy vấn | Không | Bộ định cấu hình truy vấn | Tăng cường bằng pheromone |
| **Bộ nhớ đệm (Cache)** | Buffer pool | Không tích hợp sẵn | Page cache | LRU gán khóa BLAKE3 |

*Bảng 7: So sánh toàn diện KQL với các ngôn ngữ truy vấn hiện có.*

## 6.4 Các Đặc tính Hiệu năng

### 6.4.1 Hiệu năng Parser

Parser dựa trên nom hoạt động trong **thời gian tuyến tính** O(n) với n là độ dài của chuỗi truy vấn. Thời gian phân tích cú pháp truy vấn điển hình:

| Độ phức tạp của truy vấn | Độ dài | Thời gian phân tích cú pháp kỳ vọng |
|-----------------|:------:|:-------------------:|
| FIND đơn giản | ~20 ký tự | <10 μs |
| FIND + WHERE + SCOPE + LIMIT | ~80 ký tự | <30 μs |
| Các điều kiện AND/OR phức tạp | ~200 ký tự | <80 μs |
| WATCH + WHERE + ON + NOTIFY | ~150 ký tự | <50 μs |

Hiệu năng của parser được giới hạn bởi thiết kế zero-copy của nom — không có việc cấp phát chuỗi trong quá trình phân tách mã báo hiệu (tokenization).

### 6.4.2 Hiệu năng Executor

Các thao tác của bộ thực thi cục bộ (local executor) mở rộng theo kích thước của kho lưu trữ KU:

| Thao tác (Operation) | Độ phức tạp (Complexity) | Ghi chú (Notes) |
|-----------|:----------:|-------|
| FIND (không chỉ mục) | O(N) | Quét toàn bộ kèm đánh giá điều kiện |
| FIND (kèm LIMIT) | O(N) | Quét toàn bộ, cắt bớt kết quả |
| CREATE | O(1) | Thêm vào kho lưu trữ |
| UPDATE | O(N) | Quét toàn bộ + đột biến |
| DEPRECATE | O(N) | Quét toàn bộ + đột biến |
| Đăng ký WATCH | O(1) | Thêm vào danh sách watch |
| check_watches | O(W × C) | W=số lượng watch, C=độ phức tạp của điều kiện |

Đối với các kho lưu trữ lớn (>100K KU), việc tra cứu dựa trên chỉ mục (thông qua các bảng `index_trust` và `index_concept` của redb) giảm độ phức tạp của FIND xuống O(log N) cho các trường được đánh chỉ mục.

### 6.4.3 Độ trễ Truy vấn Phân tán

Độ trễ đầu-cuối cho các truy vấn phân tán:

| Phạm vi (Scope) | Phân tích cục bộ | RTT mạng | Thực thi từ xa | Hợp nhất | Tổng cộng |
|-------|:----------:|:-----------:|:-----------:|:-----:|:-----:|
| LOCAL | <0.1ms | 0 | <1ms | 0 | ~1ms |
| NEIGHBORS | <0.1ms | ~50ms | <1ms | <1ms | ~52ms |
| CLUSTER | <0.1ms | ~100ms | <1ms | <2ms | ~103ms |
| DHT | <0.1ms | ~200ms | <5ms | <5ms | ~210ms |
| SEMANTIC | <0.1ms | ~150ms | <5ms | <5ms | ~160ms |
| GLOBAL | <0.1ms | ~500ms | <10ms | <10ms | ~520ms |

*Bảng 8: Độ trễ truy vấn phân tán kỳ vọng theo cấp độ phạm vi.*

---
