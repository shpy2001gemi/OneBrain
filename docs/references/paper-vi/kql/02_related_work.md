# 2. Related Work

Mục này khảo sát các ngôn ngữ truy vấn và hệ thống xử lý truy vấn phân tán hiện có, chỉ ra những hạn chế của chúng đối với các ứng dụng đồ thị tri thức phi tập trung và định vị các đóng góp của KQL.

## 2.1 Ngôn ngữ Truy vấn Quan hệ

**SQL** [1] đã là ngôn ngữ truy vấn thống trị trong hơn bốn thập kỷ qua. Cấu trúc SELECT-FROM-WHERE khai báo, các hàm gom tụ (COUNT, SUM, AVG) và các mệnh đề sắp xếp của nó đã truyền cảm hứng trực tiếp cho thiết kế cú pháp của KQL. Tuy nhiên, SQL về cơ bản giả định:
- Một lược đồ quan hệ tập trung hiển thị đối với bộ tối ưu hóa truy vấn (query optimizer)
- Các giao dịch ACID với các đảm bảo về tính khả tuần tự (serializability guarantees)
- Một công cụ truy vấn (query engine) duy nhất có quyền truy cập dữ liệu hoàn chỉnh

Các hệ thống **Distributed SQL** (CockroachDB [2], Google Spanner [3], TiDB) mở rộng SQL sang các thiết lập phân tán nhưng dựa vào các giao thức đồng thuận (consensus protocols) (Paxos, Raft) để có tính nhất quán mạnh (strong consistency) — điều này không tương thích về mặt cơ bản với mô hình nhất quán cuối cùng (eventual consistency model) của một mạng lưới tri thức P2P phi tập trung không phân quyền (permissionless P2P knowledge network).

**KQL kế thừa** cú pháp khai báo của SQL (FIND ≈ SELECT, WHERE, ORDER BY, LIMIT) nhưng thay thế việc thực thi tập trung bằng định tuyến phân tán dựa trên phạm vi (scope-based distributed routing).

## 2.2 Ngôn ngữ Truy vấn Đồ thị

**Cypher** [4] (Neo4j) đã giới thiệu cú pháp mẫu đồ thị kiểu ASCII-art `(a)-[r:TYPE]->(b)` để duyệt đồ thị một cách trực quan. KQL kế thừa cú pháp mẫu này cho khớp nút (`(k:KU)`) và mẫu cạnh (`-[r:BondType]->`). Tuy nhiên, Cypher:
- Nhắm tới cơ sở dữ liệu đồ thị thuộc tính đơn máy chủ (single-server property graph database)
- Không có khái niệm về phân phạm vi thực thi phân tán (distributed execution scoping)
- Thiếu xếp hạng nhận biết tin cậy (trust-aware ranking) hoặc các loại đặc thù của tri thức
- Không hỗ trợ các truy vấn liên tục (standing queries) hay hủy bỏ tri thức (knowledge deprecation)

**SPARQL** [5] cung cấp khả năng khớp mẫu đồ thị mạnh mẽ trên các bộ ba RDF (RDF triples) với OPTIONAL, UNION, FILTER, và truy vấn liên hợp (federated query) (SERVICE). Các phần mở rộng liên hợp của SPARQL [6] cho phép thực hiện các truy vấn chéo điểm cuối (cross-endpoint queries), nhưng yêu cầu các URL điểm cuối rõ ràng — điều này không khả thi trong một mạng P2P nơi các điểm cuối ẩn danh, động và có số lượng lên tới hàng triệu.

**GQL** [7] (ISO/IEC 39075:2024), tiêu chuẩn ISO sắp tới cho các ngôn ngữ truy vấn đồ thị, thống nhất các khái niệm từ Cypher, PGQL và G-CORE. GQL giải quyết việc truy vấn đồ thị thuộc tính trong các cơ sở dữ liệu tập trung nhưng không xem xét thực thi phi tập trung (decentralized execution), siêu dữ liệu tin cậy (trust metadata), hay quản lý vòng đời tri thức (knowledge lifecycle management).

**Gremlin** [8] (Apache TinkerPop) cung cấp một ngôn ngữ duyệt đồ thị mệnh lệnh (imperative graph traversal language). Mặc dù mạnh mẽ cho các thuật toán đồ thị phức tạp, bản chất mệnh lệnh của Gremlin khiến nó không phù hợp để làm giao diện truy vấn tri thức khai báo.

| Feature | SQL | SPARQL | Cypher | GQL | Gremlin | **KQL** |
|---------|-----|--------|--------|-----|---------|---------|
| Paradigm | Declarative | Declarative | Declarative | Declarative | Imperative | Declarative |
| Data model | Relational | RDF triples | Property graph | Property graph | Property graph | Knowledge Units |
| Graph patterns | Không | Có (BGP) | Có (ASCII-art) | Có | Có (traversal) | Có (Cypher-style) |
| Distributed | Federated SQL | SERVICE clause | Không | Không | Không | SCOPE clause (6 levels) |
| Trust-aware | Không | Không | Không | Không | Không | Có (trust_score, epistemic) |
| Standing queries | Không | Không | Không | Không | Không | WATCH + bộ lọc sự kiện |
| Deprecation | DELETE | DELETE | DELETE | DELETE | DROP | DEPRECATE + REASON |
| Query plan | EXPLAIN | Không | EXPLAIN | EXPLAIN | explain() | EXPLAIN |
| Aggregation | Full | Full | Full | Full | fold/unfold | COUNT/SUM/AVG/MIN/MAX |

*Bảng 1: So sánh các ngôn ngữ truy vấn qua các tính năng chính. KQL kết hợp một cách độc đáo các khả năng phân phạm vi phân tán, nhận biết độ tin cậy và quản lý vòng đời tri thức.*

## 2.3 Xử lý Truy vấn Phân tán

**Federated query processing** (Xử lý truy vấn liên hợp) [9] phân rã các truy vấn trên nhiều nguồn dữ liệu tự trị. Các kỹ thuật bao gồm phân rã truy vấn (query decomposition), định tuyến truy vấn con (subquery routing) và tích hợp kết quả (result integration). Các thách thức chính:
- **Source selection** (Lựa chọn nguồn): Nguồn nào có thể trả lời các truy vấn con nào?
- **Query optimization** (Tối ưu hóa truy vấn): Làm thế nào để giảm thiểu giao tiếp giữa các nguồn?
- **Result integration** (Tích hợp kết quả): Làm thế nào để hợp nhất các kết quả dị thể?

KQL giải quyết những vấn đề này thông qua leo thang phạm vi 6 lớp (6-layer scope escalation) (§4.2), đánh giá khả năng của nguồn dựa trên bộ lọc Vacuum Bloom (Vacuum Bloom filter-based source capability assessment) (§5.1), và xếp hạng kết quả theo độ tin cậy × khoảng cách (trust×proximity result ranking) (§5.2).

**CQL** (Continuous Query Language) [10] mở rộng SQL với các cửa sổ (windows) và toán tử luồng (streaming operators) để xử lý truy vấn liên tục trên các luồng dữ liệu. Các truy vấn WATCH của KQL phục vụ một mục đích tương tự — cung cấp các thông báo hướng sự kiện khi tri thức phù hợp xuất hiện — nhưng hoạt động trên một đồ thị tri thức P2P thay vì một luồng tập trung.

**Linked Data Fragments** [11] (TPF, brTPF) phân phối xử lý truy vấn giữa máy khách (client) và máy chủ (server) bằng cách cung cấp các khả năng tối thiểu phía máy chủ (ví dụ: tra cứu mẫu bộ ba - triple pattern lookup) và đẩy việc xử lý phức tạp cho máy khách. Triết lý này phù hợp với việc leo thang phạm vi của KQL, nơi thực thi cục bộ (local execution) xử lý các trường hợp "dễ" và các truy vấn mạng xử lý các trường hợp "khó".

## 2.4 Các Hệ thống Truy vấn Đồ thị Tri thức

**Wikidata Query Service** [12] cung cấp quyền truy cập SPARQL vào đồ thị tri thức tập trung với khoảng 100 tỷ bộ ba (triples). Mặc dù minh chứng cho giá trị của truy vấn tri thức có cấu trúc, kiến trúc tập trung của nó tạo ra các điểm lỗi đơn lẻ (single points of failure) và sự kiểm soát tập trung.

**Google Knowledge Graph** [13] hỗ trợ tìm kiếm thông qua một đồ thị tri thức khổng lồ độc quyền với các giao diện truy vấn nội bộ. Bản chất độc quyền và tập trung ngăn cản việc sử dụng và kiểm tra từ bên ngoài.

**Amazon Neptune**, **Microsoft Azure Cosmos DB** (giao diện lập trình Gremlin API), và **ArangoDB** (AQL) cung cấp các dịch vụ truy vấn đồ thị được lưu trữ trên đám mây (cloud-hosted) với các ngôn ngữ truy vấn độc quyền hoặc tiêu chuẩn. Tất cả đều giả định một triển khai tập trung trên đám mây.

**RDF4J**, **Apache Jena**, và **Stardog** cung cấp các điểm cuối SPARQL cho dữ liệu RDF. Mặc dù đã trưởng thành và tuân thủ tiêu chuẩn, chúng nhắm mục tiêu triển khai trên một trang web duy nhất (single-site) hoặc liên hợp (explicit endpoint).

**Sự khác biệt của KQL**: Không có hệ thống truy vấn đồ thị tri thức hiện tại nào cung cấp một ngôn ngữ truy vấn được thiết kế cho các mạng P2P không phân quyền (permissionless P2P networks) với các nút ẩn danh, tính nhất quán cuối cùng, dữ liệu được gán nhãn độ tin cậy và cấu trúc tri thức lấy cảm hứng sinh học.

## 2.5 Các Parser Combinator

**Parser combinators** [14] kết hợp các hàm phân tích cú pháp nhỏ thành các parser phức tạp. Cách tiếp cận này có nguồn gốc từ lập trình hàm (Haskell's Parsec [15]) và đã được áp dụng trong các ngôn ngữ hệ thống:

- **nom** [16] (Rust): Parser combinators không sao chép dữ liệu (zero-copy), có khả năng xử lý dạng luồng (streaming-capable). KQL sử dụng nom cho parser của nó, đạt khoảng 1.310 dòng code cho ngữ pháp hoàn chỉnh.
- **pest** (Rust): Parser generator dựa trên PEG. Cung cấp định nghĩa ngữ pháp đơn giản hơn nhưng kém linh hoạt hơn khi chạy (runtime flexibility).
- **LALRPOP** (Rust): LR parser generator. Phù hợp hơn cho các ngữ pháp phức tạp nhưng tạo ra mã nguồn kém dễ đọc hơn.
- **ANTLR** [17] (Java/đa ngôn ngữ): LL(*) parser generator được sử dụng rộng rãi trong hiện thực ngôn ngữ.

KQL chọn **nom** vì ba lý do: (1) phân tích cú pháp zero-copy giảm thiểu việc cấp phát bộ nhớ; (2) cấu thành combinator cho phép mở rộng ngữ pháp tăng dần; (3) tích hợp Rust bản địa mà không cần tạo mã nguồn tại thời điểm xây dựng (build-time code generation).

## 2.6 Lưu Bộ nhớ đệm Truy vấn trong các Hệ thống Phân tán

**Materialized views** (Khung nhìn hiện thực hóa) [18] tính toán trước các kết quả truy vấn để truy cập nhanh. Trong một thiết lập phân tán, việc duy trì tính nhất quán của khung nhìn là một thách thức.

**Query result caching** (Lưu bộ nhớ đệm kết quả truy vấn) [19] lưu trữ các kết quả truy vấn gần đây được gắn khóa theo các chuỗi truy vấn được chuẩn hóa. Các chiến lược vô hiệu hóa bộ nhớ đệm (cache invalidation) bao gồm hết hạn dựa trên TTL, vô hiệu hóa hướng sự kiện và các phương pháp tiếp cận lai.

Bộ nhớ đệm truy vấn (query cache) của KQL (§5.3) sử dụng các chuỗi truy vấn được chuẩn hóa đã được băm BLAKE3 làm khóa, thu hồi LRU với dung lượng có thể cấu hình, và hết hạn dựa trên TTL. Thông điệp mạng `CacheInvalidate(0x68)` cho phép duy trì tính mạch lạc bộ nhớ đệm phân tán (distributed cache coherence).

## 2.7 Tóm tắt và Định vị

Bảng 2 tóm tắt định vị của KQL so với các hệ thống hiện có:

| System | Decentralized | Trust-Aware | Standing Queries | Lifecycle Mgmt | Knowledge Types |
|--------|:------------:|:-----------:|:----------------:|:--------------:|:---------------:|
| SQL | Không | Không | Không | Không | Không |
| SPARQL | Federated | Không | Không | Không | RDF types |
| Cypher | Không | Không | Không | Không | Không |
| GQL | Không | Không | Không | Không | Không |
| Gremlin | Không | Không | Không | Không | Không |
| Wikidata SPARQL | Không | Không | Không | Không | Wikidata types |
| CQL (streams) | Không | Không | Windows | Không | Không |
| **KQL** | **Có (6 scopes)** | **Có** | **Có (WATCH)** | **Có (DEPRECATE)** | **Có (10 genes)** |

*Bảng 2: Định vị của KQL so với các ngôn ngữ và hệ thống truy vấn hiện có.*

---

## References

[1] ISO/IEC 9075:2023, "Information technology — Database languages — SQL," 2023.

[2] R. Taft *et al.*, "CockroachDB: The Resilient Geo-Distributed SQL Database," in *Proc. ACM SIGMOD '20*, pp. 1493–1509, 2020.

[3] J. C. Corbett *et al.*, "Spanner: Google's Globally-Distributed Database," in *Proc. OSDI '12*, pp. 251–264, 2012.

[4] N. Francis *et al.*, "Cypher: An Evolving Query Language for Property Graphs," in *Proc. ACM SIGMOD '18*, pp. 1433–1445, 2018.

[5] W3C, "SPARQL 1.1 Query Language," W3C Recommendation, Mar. 2013.

[6] O. Görlitz and S. Staab, "SPLENDID: SPARQL Endpoint Federation Exploiting VOID Descriptions," in *Proc. COLD '11*, 2011.

[7] ISO/IEC 39075:2024, "Information technology — Database languages — GQL," 2024.

[8] M. A. Rodriguez, "The Gremlin Graph Traversal Machine and Language," in *Proc. DBPL '15*, pp. 1–10, 2015.

[9] D. Kossmann, "The State of the Art in Distributed Query Processing," *ACM Computing Surveys*, vol. 32, no. 4, pp. 422–469, 2000.

[10] A. Arasu, S. Babu, and J. Widom, "The CQL Continuous Query Language: Semantic Foundations and Query Execution," *VLDB Journal*, vol. 15, no. 2, pp. 121–142, 2006.

[11] R. Verborgh *et al.*, "Triple Pattern Fragments: A Low-Cost Knowledge Graph Interface for the Web," *Journal of Web Semantics*, vol. 37, pp. 184–206, 2016.

[12] D. Vrandečić and M. Krötzsch, "Wikidata: A Free Collaborative Knowledgebase," *Communications of the ACM*, vol. 57, no. 10, pp. 78–85, 2014.

[13] A. Singhal, "Introducing the Knowledge Graph: Things, Not Strings," Google Blog, May 2012.

[14] G. Hutton, "Higher-Order Functions for Parsing," *Journal of Functional Programming*, vol. 2, no. 3, pp. 323–343, 1992.

[15] D. Leijen and E. Meijer, "Parsec: Direct Style Monadic Parser Combinators for the Real World," *Technical Report UU-CS-2001-27*, Utrecht University, 2001.

[16] G. Couprie, "nom: A Byte-Oriented, Streaming, Zero-Copy Parser Combinators Library in Rust," in *Proc. IEEE SecDev '15*, pp. 1–6, 2015.

[17] T. Parr, "ANTLR (ANother Tool for Language Recognition)," 2023. [Online]. Available: https://www.antlr.org/

[18] A. Gupta and I. S. Mumick, "Maintenance of Materialized Views: Problems, Techniques, and Applications," *IEEE Data Engineering Bulletin*, vol. 18, no. 2, pp. 3–18, 1995.

[19] Q. Luo, J. F. Naughton *et al.*, "Form-Based Proxy Caching for Database-Backed Web Sites," in *Proc. VLDB '01*, pp. 191–200, 2001.
