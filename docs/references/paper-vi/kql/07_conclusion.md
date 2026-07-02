# 7. Thảo luận, Hướng nghiên cứu Tương lai và Kết luận

## 7.1 Thảo luận

### 7.1.1 Các Phát hiện Chính

Thiết kế và hiện thực của KQL cho thấy một số phát hiện chính về các ngôn ngữ truy vấn cho các hệ thống tri thức phi tập trung:

**Phát hiện 1: Mệnh đề SCOPE là trừu tượng hóa quan trọng còn thiếu trong các ngôn ngữ truy vấn hiện có.** SQL, SPARQL, Cypher, và GQL đều giả định công cụ truy vấn có khả năng hiển thị toàn vẹn trên tập dữ liệu. Trong một mạng lưới phi tập trung, giả định này bị vi phạm về mặt cơ bản. Mệnh đề SCOPE giải quyết vấn đề này bằng cách biến độ rộng phân phối thành một tham số khai báo, rõ ràng — người dùng có thể chọn giữa tốc độ (LOCAL) và tính toàn vẹn (GLOBAL) mà không cần biết cấu trúc liên kết mạng. Phạm vi AUTO trừu tượng hóa sâu hơn lựa chọn này thông qua leo thang tăng dần, cung cấp các kết quả "đủ tốt" trong thời gian tối thiểu.

**Phát hiện 2: Xếp hạng nhận biết tin cậy tạo ra kết quả khác biệt cơ bản so với xếp hạng không điểm số.** Khi nhiều nguồn trả về kết quả cho cùng một truy vấn, xếp hạng truyền thống (theo độ liên quan hoặc độ mới) có thể ưu tiên tri thức có độ tin cậy thấp hoặc chưa được xác thực. Xếp hạng trust×scope của KQL đảm bảo rằng các kết quả có độ tin cậy cao và ở gần sẽ xuất hiện trước tiên. Trong một mạng lưới tri thức mà tính toàn vẹn nhận thức (epistemic integrity) là tối quan trọng — các sự thật khoa học, tri thức y tế, tiền lệ pháp lý — xếp hạng nhận biết tin cậy không phải là một tùy chọn mà là một yếu tố thiết yếu.

**Phát hiện 3: DEPRECATE vượt trội về mặt ngữ nghĩa so với DELETE đối với quản lý tri thức.** Trong các cơ sở dữ liệu truyền thống, DELETE xóa dữ liệu vĩnh viễn. Trong một mạng lưới tri thức, việc hủy bỏ tri thức phải bảo toàn nguồn gốc (provenance): *ai* đã hủy bỏ nó, *tại sao*, và *cái gì thay thế nó*. Lệnh DEPRECATE của KQL với các trường bắt buộc REASON và SIGNED BY đảm bảo rằng ngay cả tri thức đã bị hủy bỏ vẫn đóng góp vào lịch sử của đồ thị. Điều này phản ánh việc quản lý tri thức trong thế giới thực — mô hình địa tâm (geocentric model) đã bị hủy bỏ chứ không bị xóa bỏ, bởi vì việc hiểu *tại sao* nó bị thay thế chính là một loại tri thức.

**Phát hiện 4: Các truy vấn liên tục (WATCH) bắc cầu nối giữa mô hình kéo (pull) và đẩy (push).** Các ngôn ngữ truy vấn truyền thống hoạt động theo mô hình kéo (pull-based) — người dùng yêu cầu dữ liệu một cách rõ ràng. Trong một mạng lưới tri thức động, người dùng cũng cần các thông báo theo mô hình đẩy (push-based) khi tri thức liên quan xuất hiện. Các truy vấn WATCH cung cấp khả năng này mà không cần một cơ sở hạ tầng thông báo riêng biệt. Sự kết hợp giữa kéo (FIND) và đẩy (WATCH) trong một ngôn ngữ duy nhất giúp đơn giản hóa việc phát triển ứng dụng.

**Phát hiện 5: Các công cụ khám phá chuyển đổi ngôn ngữ truy vấn từ công cụ truy xuất thành công cụ tạo lập tri thức.** Các ngôn ngữ truy vấn truyền thống thực hiện truy xuất dữ liệu hiện có. GapDetector, BridgeFinder và SerendipityEngine của KQL tạo ra *tri thức mới* — xác định tri thức còn thiếu, các kết nối xuyên miền và các ẩn số chưa biết (unknown unknowns). Điều này nâng tầm ngôn ngữ truy vấn từ một giao diện truy xuất thành một hệ thống khuếch đại tri thức (knowledge amplification system).

**Phát hiện 6: Học pheromone tạo ra một hệ thống truy vấn tự cải thiện.** Vòng lặp phản hồi giữa kết quả truy vấn và các pheromone định tuyến đồng nghĩa với việc mạng lưới ngày càng định tuyến truy vấn tốt hơn theo thời gian — mà không cần cấu hình rõ ràng. Các chủ đề phổ biến sẽ phát triển các dấu vết pheromone mạnh mẽ đến các nút chuyên gia, trong khi các truy vấn mới sẽ quay lại các phạm vi rộng hơn. Cách tiếp cận lấy cảm hứng sinh học này tránh được vấn đề khởi động lạnh (cold-start) của các bộ tối ưu hóa truy vấn dựa trên học máy.

### 7.1.2 Đánh đổi Thiết kế

**Cú pháp quen thuộc với SQL so với cú pháp mới lạ.** KQL cố tình mô phỏng cú pháp SQL (`FIND ≈ SELECT`, `WHERE`, `ORDER BY`, `LIMIT`) để giảm thiểu thời gian học. Tuy nhiên, sự quen thuộc với SQL này có thể tạo ra những kỳ vọng sai lệch — người dùng có thể mong đợi các tính năng SQL mà KQL không hỗ trợ (JOIN, GROUP BY, HAVING, truy vấn con). Ngữ pháp phiên bản v1.0 hiện tại được thiết kế đơn giản một cách có chủ đích; các phiên bản tương lai có thể thêm các tính năng này.

**Các giá trị có kiểu so với các giá trị dựa trên chuỗi.** AST của KQL bao gồm các kiểu dữ liệu đặc thù của miền (`EpistemicStatus`, `EvidenceType`) bên cạnh các kiểu dữ liệu chuẩn. Điều này cung cấp sự an toàn kiểu dữ liệu tại thời điểm phân tích cú pháp (parse time) nhưng liên kết đặc tả ngôn ngữ với mô hình dữ liệu KU. Một cách tiếp cận chung chung hơn (chỉ dùng các giá trị chuỗi với kiểm tra kiểu lúc chạy) sẽ linh hoạt hơn nhưng mất đi tính an toàn kiểu tại thời điểm biên dịch.

**Ưu tiên cục bộ so với ưu tiên phân tán.** Phạm vi `AUTO` của KQL bắt đầu bằng việc thực thi cục bộ và leo thang dần. Điều này là tối ưu cho các truy vấn có thể trả lời cục bộ nhưng làm tăng độ trễ cho các truy vấn đòi hỏi phạm vi toàn cầu (công cụ phải cạn kiệt các phạm vi thấp hơn trước). Một cách tiếp cận thay thế có "gợi ý" (hinted approach) — nơi parser ước tính phạm vi từ ngữ nghĩa truy vấn — có thể bỏ qua các kiểm tra cục bộ không cần thiết.

**redb so với SQLite.** Phần phụ trợ lưu trữ sử dụng redb (thuần Rust, ACID, nhúng) thay vì SQLite (C, được triển khai rộng rãi). redb cho phép biên dịch chéo sang WebAssembly và thiết bị di động mà không cần phụ thuộc vào bộ công cụ C, nhưng hy sinh bộ tối ưu hóa truy vấn trưởng thành của SQLite và các khả năng tìm kiếm toàn văn (full-text search). Đối với phiên bản hiện thực v1.0, sự đơn giản và thuần khiết của redb vượt trội hơn các tính năng của SQLite.

**Tốc độ bay hơi pheromone.** PheromoneLearner sử dụng cùng các tham số bay hơi như lớp mạng (γ=0,95/giờ). Điều này có thể không tối ưu cho việc định tuyến truy vấn, nơi mức độ phổ biến của chủ đề thay đổi nhanh hơn cấu trúc liên kết mạng. Tốc độ bay hơi thích ứng — nhanh hơn đối với các chủ đề đang thịnh hành, chậm hơn đối với tri thức nền tảng — là một phần mở rộng tự nhiên.

## 7.2 Các Hạn chế

**L1: Không có bộ tối ưu hóa truy vấn.** Bộ thực thi hiện tại sử dụng chiến lược quét toàn bộ đơn giản cho các truy vấn FIND. Một bộ tối ưu hóa truy vấn dựa trên chi phí (cost-based query optimizer) tận dụng thống kê chỉ mục, mô hình chi phí phạm vi và ước tính số lượng kết quả sẽ cải thiện đáng kể hiệu năng cho các truy vấn phức tạp.

**L2: Không hỗ trợ JOIN hoặc truy vấn con.** KQL v1.0 hỗ trợ các truy vấn mẫu đơn lẻ. Các truy vấn đa mẫu (JOIN), truy vấn con và biểu thức đường dẫn vẫn chưa được hiện thực hóa. Đây là các yếu tố cần thiết cho việc duyệt đồ thị tri thức phức tạp (ví dụ: "Tìm tất cả các KU mâu thuẫn với một sự thật có độ tin cậy cao").

**L3: Duyệt đồ thị hạn chế.** Mặc dù các mẫu cạnh đã được định nghĩa trong AST (`EdgePattern` với hướng và kiểu), bộ thực thi vẫn chưa hiện thực hóa việc duyệt đồ thị đa hop. Việc khớp mẫu đồ thị hoàn chỉnh với các đường dẫn có độ dài thay đổi (`-[r:TYPE*1..5]->`) được trì hoãn sang phiên bản v2.0.

**L4: Không thực thi hệ thống kiểu dữ liệu.** Parser chấp nhận bất kỳ đường dẫn trường nào (ví dụ: `k.nonexistent_field`) mà không có kiểm tra kiểu. Các đường dẫn trường không hợp lệ được xử lý tại thời điểm thực thi (trả về kết quả trống) thay vì tại thời điểm phân tích cú pháp. Một hệ thống kiểu dữ liệu xác thực các đường dẫn trường đối với lược đồ KU sẽ cải thiện trải nghiệm của nhà phát triển.

**L5: Kiểm thử truy vấn phân tán ở quy mô lớn.** Các bài kiểm thử áp lực xác thực lên tới 10K concept và 1K truy vấn liên tục. Việc triển khai trong thế giới thực sẽ liên quan đến hàng triệu concept và hàng ngàn truy vấn đồng thời. Cần có các kiểm thử mô phỏng phân tán.

**L6: Vô hiệu hóa bộ nhớ đệm trong mạng P2P.** Thông điệp `CacheInvalidate(0x68)` lan truyền việc vô hiệu hóa bộ nhớ đệm, nhưng trong một mạng lưới bị phân mảnh (partitioned network), các bản ghi cache cũ có thể tồn tại lâu hơn TTL. Các đảm bảo nhất quán bị giới hạn bởi mô hình nhất quán cuối cùng CRDT của mạng.

**L7: Tinh chỉnh Serendipity Engine.** Các tham số đường cong hình chuông điểm tối ưu (sweet-spot bell curve) cho việc tính điểm serendipity hiện tại đang được mã hóa cứng (hard-coded). Các tham số được cá nhân hóa — được hiệu chỉnh theo sở thích khám phá của từng người dùng — sẽ cải thiện chất lượng khuyến nghị.

## 7.3 Hướng Nghiên cứu Tương lai

### 7.3.1 Ngắn hạn (v1.1)

- **Bộ tối ưu hóa truy vấn dựa trên chi phí** sử dụng thống kê chỉ mục, mô hình chi phí phạm vi và ước tính số lượng kết quả để lựa chọn chiến lược thực thi tối ưu.
- **Tích hợp tìm kiếm toàn văn** thông qua tantivy (công cụ tìm kiếm toàn văn thuần Rust) cho các truy vấn ngôn ngữ tự nhiên đối với nội dung KU.
- **Kiểm tra kiểu dữ liệu tại thời điểm phân tích cú pháp** đối với lược đồ KU để phát hiện các đường dẫn trường không hợp lệ trước khi thực thi.
- **Thực thi truy vấn theo lô** cho nhiều truy vấn trong một yêu cầu duy nhất, với việc chia sẻ leo thang phạm vi để giảm thiểu chi phí mạng.

### 7.3.2 Trung hạn (v2.0)

- **Các truy vấn đa mẫu (JOIN)**: `FIND (a:KU)-[r:Contradicts]->(b:KU) WHERE a.trust > 8000` với duyệt đồ thị đầy đủ.
- **Các biểu thức đường dẫn**: Các đường dẫn có độ dài thay đổi `(a:KU)-[:PartOf*1..5]->(b:Concept)` cho duyệt đồ thị đa hop.
- **GROUP BY và HAVING**: Gom tụ kèm theo nhóm cho các truy vấn phân tích.
- **Truy vấn con (Subqueries)**: Các truy vấn lồng nhau cho việc lọc phức tạp.
- **Ngữ nghĩa UPSERT**: Các thao tác chèn-hoặc-cập-nhật nguyên tử.
- **Các truy vấn thời gian (Temporal queries)**: Các truy vấn du hành thời gian trên lịch sử phiên bản CRDT.

### 7.3.3 Dài hạn (v3.0)

- **Giao diện ngôn ngữ tự nhiên**: Dịch thuật bằng LLM từ câu hỏi ngôn ngữ tự nhiên sang truy vấn KQL, cho phép người dùng phi kỹ thuật truy vấn mạng lưới tri thức.
- **Liên hợp truy vấn với SPARQL**: Cầu nối hai chiều giữa KQL và các điểm cuối SPARQL để tương tác với hệ sinh thái Linked Data.
- **Ngữ nghĩa hình thức (Formal semantics)**: Ngữ nghĩa biểu thị (denotational semantics) cho KQL dưới dạng các thao tác tập hợp trên đồ thị tri thức KU, cho phép xác thực hình thức sự tương đương của truy vấn.
- **Duy trì khung nhìn tăng dần (Incremental view maintenance)**: Các khung nhìn hiện thực hóa trên các truy vấn KQL với duy trì phân tán dựa trên CRDT tự động.
- **Lựa chọn phạm vi nâng cao bằng ML**: Mạng thần kinh được huấn luyện trên nhật ký truy vấn lịch sử để dự đoán phạm vi tối ưu cho mỗi truy vấn, bổ sung cho thuật toán heuristic dựa trên pheromone.

## 7.4 Kết luận

Bài báo này đã trình bày **KQL (Knowledge Query Language)**, một ngôn ngữ truy vấn khai báo được thiết kế cho các đồ thị tri thức phi tập trung. Không giống như các ngôn ngữ truy vấn hiện có giả định các kho lưu trữ dữ liệu tập trung, KQL cung cấp các cấu trúc bản địa cho việc thực thi phân tán có phạm vi (scoped distributed execution), xếp hạng nhận biết tin cậy (trust-aware ranking), các truy vấn phản ứng liên tục (standing reactive queries) và quản lý vòng đời tri thức (knowledge lifecycle management).

Sáu đóng góp chính của chúng tôi là:

1. **Một ngôn ngữ truy vấn khai báo cho các đồ thị tri thức phi tập trung** với 6 loại truy vấn (`FIND`, `CREATE`, `UPDATE`, `DEPRECATE`, `WATCH`, `EXPLAIN`), khớp mẫu đồ thị với các nút có kiểu, lọc nhận biết tin cậy và 5 hàm gom tụ — cung cấp một giao diện quản lý tri thức hoàn chỉnh trong một ngôn ngữ duy nhất, nhất quán.

2. **Mệnh đề SCOPE để kiểm soát thực thi phân tán rõ ràng** trên 6 cấp độ leo thang (Local → Neighbors → Cluster → DHT → Semantic → Global), cho phép người dùng lựa chọn đánh đổi độ trễ lấy tính toàn vẹn một cách khai báo. Phạm vi AUTO cung cấp khả năng leo thang tăng dần mà không cần tinh chỉnh thủ công.

3. **Các truy vấn phản ứng liên tục (WATCH)** với các thông báo hướng sự kiện (`CREATE`, `UPDATE`, `DEPRECATE`, `ANY`), lan truyền bộ lọc qua các siêu nút, và quản lý vòng đời dựa trên TTL — cơ chế truy vấn liên tục đầu tiên được tích hợp vào một ngôn ngữ truy vấn đồ thị.

4. **Một nom-based recursive descent parser** tạo ra một typed AST với hơn 30 loại nút, hỗ trợ các từ khóa không phân biệt chữ hoa chữ thường, các điều kiện boolean lồng nhau với AND/OR/NOT/EXISTS, 5 hàm gom tụ, các mẫu cạnh đồ thị, và 8 kiểu giá trị bao gồm EpistemicStatus và EvidenceType đặc thù của miền.

5. **Ba công cụ khám phá tri thức mới** — Knowledge Gap Detector (xác định các concept mồ côi, các KU có độ tin cậy thấp và các giả thuyết chưa được kiểm chứng), Swanson ABC Bridge Finder (tri thức công cộng chưa được phát hiện xuyên miền), và Serendipity Engine (gợi mở các ẩn số chưa biết qua việc tính điểm relevance×novelty) — nâng tầm ngôn ngữ truy vấn từ một giao diện truy xuất thành một hệ thống khuếch đại tri thức.

6. **Tăng cường định tuyến truy vấn dựa trên pheromone** đóng vòng lặp phản hồi giữa kết quả truy vấn và định tuyến mạng: các truy vấn thành công sẽ củng cố dấu vết pheromone (+0.1), các truy vấn thất bại sẽ phạt chúng (-0.2), và các dấu vết không sử dụng sẽ bay hơi (×0.95/giờ). Cách tiếp cận lấy cảm hứng sinh học này tạo ra một hệ thống truy vấn tự cải thiện, thích ứng với các mẫu nhu cầu tri thức mà không cần cấu hình rõ ràng.

Phần hiện thực trải rộng trên **~3.175 dòng code** (cốt lõi) + **~2.860 dòng code** (phân tán) trên 17 mô-đun Rust, được xác thực bằng **66+ bài kiểm thử** bao gồm 13 bài kiểm thử tích hợp và áp lực ở quy mô lên tới 10K concept. Tất cả các thư viện phụ thuộc đều là thuần Rust, cho phép biên dịch chéo sang di động và WebAssembly.

KQL chứng minh rằng các mạng lưới tri thức phi tập trung đòi hỏi các ngôn ngữ truy vấn được xây dựng chuyên biệt — chứ không phải các sự thích ứng của các ngôn ngữ truy vấn cơ sở dữ liệu tập trung. Bằng cách tích hợp kiểm soát phân phối, nhận biết độ tin cậy, giám sát phản ứng, quản lý vòng đời và khám phá tri thức vào một ngôn ngữ khai báo duy nhất, KQL cung cấp giao diện truy vấn mà các hệ thống tri thức phi tập trung cần.

---

## References

[1] ISO/IEC 9075:2023, "Information technology — Database languages — SQL," 2023.

[2] W3C, "SPARQL 1.1 Query Language," W3C Recommendation, Mar. 2013.

[3] N. Francis *et al.*, "Cypher: An Evolving Query Language for Property Graphs," in *Proc. ACM SIGMOD '18*, pp. 1433–1445, 2018.

[4] Facebook, "GraphQL: A Query Language for APIs," 2015.

[5] W3C, "XQuery 3.1: An XML Query Language," W3C Recommendation, Mar. 2017.

[6] S. Ceri, G. Gottlob, and L. Tanca, "What You Always Wanted to Know About Datalog," *IEEE TKDE*, vol. 1, no. 1, pp. 146–166, 1989.

[7] OneBrain Project, "Knowledge Unit: A Bio-Inspired Knowledge Representation for Decentralized Knowledge Networks," 2026 (companion paper).

[8] O. Görlitz and S. Staab, "SPLENDID: SPARQL Endpoint Federation Exploiting VOID Descriptions," in *Proc. COLD '11*, 2011.

[9] ISO/IEC 39075:2024, "Information technology — Database languages — GQL," 2024.

[10] M. A. Rodriguez, "The Gremlin Graph Traversal Machine and Language," in *Proc. DBPL '15*, 2015.

[11] D. Kossmann, "The State of the Art in Distributed Query Processing," *ACM Computing Surveys*, vol. 32, no. 4, pp. 422–469, 2000.

[12] A. Arasu, S. Babu, and J. Widom, "The CQL Continuous Query Language," *VLDB Journal*, vol. 15, no. 2, pp. 121–142, 2006.

[13] R. Verborgh *et al.*, "Triple Pattern Fragments," *Journal of Web Semantics*, vol. 37, pp. 184–206, 2016.

[14] D. Vrandečić and M. Krötzsch, "Wikidata: A Free Collaborative Knowledgebase," *CACM*, vol. 57, no. 10, pp. 78–85, 2014.

[15] A. Singhal, "Introducing the Knowledge Graph," Google Blog, May 2012.

[16] G. Hutton, "Higher-Order Functions for Parsing," *JFP*, vol. 2, no. 3, pp. 323–343, 1992.

[17] D. Leijen and E. Meijer, "Parsec: Direct Style Monadic Parser Combinators," UU-CS-2001-27, 2001.

[18] G. Couprie, "nom: A Byte-Oriented, Streaming, Zero-Copy Parser Combinators Library in Rust," in *Proc. IEEE SecDev '15*, 2015.

[19] T. Parr, "ANTLR," 2023. [Online]. Available: https://www.antlr.org/

[20] A. Gupta and I. S. Mumick, "Maintenance of Materialized Views," *IEEE DE Bulletin*, vol. 18, no. 2, pp. 3–18, 1995.

[21] Q. Luo *et al.*, "Form-Based Proxy Caching for Database-Backed Web Sites," in *Proc. VLDB '01*, pp. 191–200, 2001.

[22] R. Taft *et al.*, "CockroachDB: The Resilient Geo-Distributed SQL Database," in *Proc. ACM SIGMOD '20*, 2020.

[23] J. C. Corbett *et al.*, "Spanner: Google's Globally-Distributed Database," in *Proc. OSDI '12*, 2012.

[24] D. R. Swanson, "Fish Oil, Raynaud's Syndrome, and Undiscovered Public Knowledge," *Perspectives in Biology and Medicine*, vol. 30, no. 1, pp. 7–18, 1986.

[25] C. Olson, "redb: An embedded key-value store written in pure Rust," 2023.

[26] M. Shapiro *et al.*, "A Comprehensive Study of Convergent and Commutative Replicated Data Types," INRIA RR-7506, 2011.

[27] P. Maymounkov and D. Mazières, "Kademlia: A Peer-to-Peer Information System Based on the XOR Metric," in *Proc. IPTPS '02*, pp. 53–65, 2002.

[28] A. Das, I. Gupta, and A. Motivala, "SWIM: Scalable Weakly-consistent Infection-style Process Group Membership Protocol," in *Proc. IEEE/IFIP DSN '02*, 2002.

[29] M. Dorigo and T. Stützle, *Ant Colony Optimization*. MIT Press, 2004.

[30] B. H. Bloom, "Space/Time Trade-offs in Hash Coding with Allowable Errors," *CACM*, vol. 13, no. 7, pp. 422–426, 1970.

[31] P.-P. Grassé, "La reconstruction du nid et les coordinations interindividuelles chez Bellicositermes natalensis," *Insectes Sociaux*, vol. 6, pp. 41–80, 1959.

[32] OneBrain Project, "OneBrain Protocol: A Bio-Inspired 9-Layer P2P Network Stack for Decentralized Knowledge Sharing," 2026 (companion paper).

---

*Kết thúc Bài báo — KQL: Một Ngôn ngữ Truy vấn Khai báo cho các Đồ thị Tri thức Phi tập trung*
