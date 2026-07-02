# 7. Thảo luận, Hướng nghiên cứu tương lai và Kết luận

## 7.1 Thảo luận

### 7.1.1 Các phát hiện chính

Hệ thống Knowledge Unit (KU) giới thiệu một mô hình biểu diễn tri thức lấy cảm hứng từ sinh học mới lạ giải quyết các hạn chế cơ bản của các phương pháp hiện tại. Đánh giá của chúng tôi chỉ ra một số phát hiện chính:

**Phát hiện 1: Ẩn dụ sinh học mang lại năng suất về mặt cấu trúc, không đơn thuần mang tính trang trí.** Ánh xạ 3 lớp từ DNA-sang-KU (Core DNA → Epigenetics → Expression) tạo ra các lợi ích kiến trúc cụ thể. Lớp Core DNA mã hóa tri thức thành một luồng lệnh nhỏ gọn sử dụng 32 opcodes — tương tự như cách 4 nucleotide của DNA kết hợp thành các codon. Lớp Epigenetics cung cấp siêu dữ liệu runtime có tính thích ứng (sự tin cậy, liên kết, sự chuyển hóa) phát sinh từ các tương tác mạng — tương tự như cách các dấu vết biểu di truyền (epigenetic marks) điều hòa biểu hiện gen mà không làm thay đổi trình tự DNA. Lớp Expression tạo ra kết xuất ngôn ngữ tự nhiên theo yêu cầu — tương tự như biểu hiện kiểu hình. Sự phân tách 3 lớp này là nhận thức mấu chốt giải quyết vấn đề phình to dữ liệu của CBOR: bằng cách chỉ lưu trữ Core DNA (các câu lệnh ngữ nghĩa thiết yếu) và tính toán mọi thứ khác vào thời điểm chạy, chúng tôi đạt được kích thước đường truyền luôn **nhỏ hơn văn bản gốc**.

**Phát hiện 2: Mã hóa varint 5 tầng đạt được sự liên kết ngữ nghĩa với tần suất khái niệm.** Không giống như các varint của LEB128 và Protocol Buffer vốn độc lập với tần suất, varint của OneBrain gán độ rộng byte cho các tầng ID khái niệm dựa trên tần suất sử dụng kỳ vọng. Dưới các giả định phân phối Zipfian, điều này mang lại trung bình có trọng số là 1.89 bytes cho mỗi ID khái niệm — tiết kiệm 76.4% so với mã hóa `u64` độ rộng cố định. Việc xác định độ dài $O(1)$ từ tiền tố của byte đầu tiên là một lợi thế thực tế cho giải mã thông lượng cao.

**Phát hiện 3: Tích hợp CRDT cho phép xây dựng tri thức phi tập trung đáng tin cậy không cần giao thức đồng thuận.** Bằng cách ánh xạ mỗi trường KU khả biến tới một kiểu CRDT thích hợp (GCounter cho các số liệu đơn điệu, LWWRegister cho trạng thái nhận thức, ORSet cho các mã lĩnh vực), hệ thống đảm bảo Tính Nhất quán Sau cùng Mạnh mẽ mà không yêu cầu đồng thuận chịu lỗi Byzantine. Đây là một sự khác biệt cơ bản so với các hệ thống tri thức dựa trên blockchain (ví dụ: OriginTrail), vốn áp đặt chi phí gas trên mỗi thao tác và giới hạn về thông lượng.

**Phát hiện 4: Khung nhận thức nắm bắt mức độ trưởng thành của tri thức mịn hơn bất kỳ hệ thống hiện có nào.** Thang trạng thái nhận thức 11 cấp độ (Rumor → Axiomatic), kết hợp với 9 kiểu bằng chứng liên kết GRADE và trường bit nhạy cảm lỗi 16-bit, cung cấp từ vựng có cấu trúc để thể hiện sự không chắc chắn vốn thiếu vắng trong tất cả các hệ thống biểu diễn tri thức được khảo sát. Khung nhận thức này dựa trên quan sát — trạng thái nhận thức nâng cấp thông qua các tín hiệu đo lường được (trích dẫn, truy xuất, xác minh) thay vì bỏ phiếu chủ quan.

**Phát hiện 5: Định dạng wire format Core DNA đạt kích thước nhỏ hơn văn bản ngôn ngữ tự nhiên.** Định dạng Core DNA đạt kích thước xấp xỉ **16 bytes** cho một KU kiểu Fact tối thiểu và **88 bytes** cho một mã hóa tri thức tiếng Việt nhiều câu lệnh điển hình ("bơi ếch") — **nhỏ hơn 3.7×** so với văn bản gốc. Core DNA đạt được tỷ lệ kích thước trên chức năng tốt nhất trong số tất cả các định dạng so sánh, thậm chí nhỏ hơn cả các bộ ba RDF/Turtle thô trong khi mang siêu dữ liệu kiểu gen, độ tin cậy và tính toàn vẹn.

**Phát hiện 6: Mã hóa được hỗ trợ bởi AI thông qua gọi hàm là thực tế và hiệu quả.** Quy trình mã hóa 3 tầng (dựa trên quy tắc → AI cục bộ → đồng thuận phân tán) cho phép mã hóa tri thức mà không phụ thuộc vào đám mây. Tier 2, sử dụng 15 công cụ gọi hàm JSON-schema, cho phép bất kỳ mô hình AI cục bộ nào (Gemma 4, Qwen, Phi-3, v.v.) tạo ra các mã hóa KU chất lượng cao bằng cách chỉ đơn giản gọi các công cụ — mà không cần hiểu định dạng nhị phân. Tier 3, Nghị thức Đồng thuận Mã hóa, cung cấp xác thực phân tán thông qua vòng đời 4 trạng thái (RAW → SELF → PART → FULL) với xác minh 2 pha (đồng thuận phân rã AI + vòng phản hồi công cụ) và chấm điểm đồng thuận có trọng số — đảm bảo độ trung thực của mã hóa mà không cần cơ quan quản lý trung tâm. Kiến trúc runtime có thể cắm nóng (Tùy chọn C) giúp hệ thống sẵn sàng cho các nâng cấp phần cứng và mô hình trong tương lai.

### 7.1.2 Các đánh đổi thiết kế

Một số quyết định thiết kế liên quan đến các đánh đổi xứng đáng được thảo luận:

**Khả năng biểu đạt so với độ phức tạp.** 11 kiểu gen và 33 kiểu liên kết cung cấp biểu diễn tri thức mịn, nhưng làm tăng độ dốc học tập cho các nhà phát triển và độ phức tạp của hệ thống kiểu. Chúng tôi giảm thiểu điều này thông qua: (a) trường kiểu gen 4-bit trong VER_META, mã hóa trực tiếp tất cả 11 kiểu với 5 mã dự phòng cho các phương thức tương lai; và (b) tập lệnh 32-opcode với các opcode dự phòng (0x55–0xEF) cho các câu lệnh tương lai — một dạng "khả năng mở rộng tiến hóa" lấy cảm hứng từ quá trình nhân đôi gen trong sinh học.

**Sự tăng trưởng trạng thái CRDT.** GCounter tăng tuyến tính với số lượng nút đóng góp. Trong một mạng lưới toàn cầu với hàng triệu nút, điều này có thể dẫn đến sự tăng trưởng trạng thái không giới hạn trên mỗi KU. Chúng tôi giải quyết vấn đề này thông qua: (1) hệ thống chuyển hóa PoMV, áp dụng suy giảm lũy thừa với chu kỳ bán rã 30 ngày, tự động cắt tỉa trạng thái không liên quan; và (2) hệ thống miễn dịch, phát hiện và cách ly các mẫu đóng góp bất thường (sự bùng phát thời gian, sự tập trung nguồn).

**Nhị phân tùy chỉnh so với CBOR so với Protobuf.** Chúng tôi chọn một tập lệnh nhị phân tùy chỉnh (Core DNA) thay vì CBOR và Protocol Buffers cho lớp mã hóa lâu dài. Mặc dù CBOR cung cấp mã hóa tự mô tả và tiêu chuẩn hóa IETF, nó tạo ra overhead đáng kể cho các mẫu câu lệnh có kiểu, nhỏ gọn mà Core DNA sử dụng. Định dạng tùy chỉnh đạt được kích thước dây **nhỏ hơn 3.7×** so với văn bản ngôn ngữ tự nhiên ban đầu — một kết quả không thể đạt được với các định dạng tuần tự hóa đa dụng. Sự đánh đổi là mất đi tính chất tự mô tả của CBOR, nhưng điều này được giảm bớt nhờ định dạng opcode có cấu trúc và từ vựng dùng chung ConceptDict. CBOR được giữ lại cho lớp Epigenetics (chỉ ở thời điểm chạy, không lưu trữ lâu dài).

**Định địa chỉ theo nội dung so với tính khả biến.** BLAKE3 CID cung cấp nhận dạng nội dung bất biến, nhưng siêu dữ liệu KU (điểm tin cậy, số lượng sử dụng) có tính khả biến thông qua các CRDT. Chúng tôi giải quyết điều này bằng cách chỉ tính toán CID trên các byte đường truyền của Core DNA (mã hóa tri thức bất biến, lâu dài), trong khi siêu dữ liệu tiến hóa độc lập trong lớp Epigenetics.

### 7.1.3 Đánh giá tính mới

Theo hiểu biết của chúng tôi, hệ thống Knowledge Unit là hệ thống đầu tiên kết hợp tất cả các yếu tố sau vào một khung nhất quán duy nhất:

1. **Biểu diễn tri thức lấy cảm hứng từ sinh học** với ẩn dụ DNA 3 lớp nhất quán (Core DNA / Epigenetics / Expression) xuyên suốt từ thiết kế đến triển khai.
2. **Tập lệnh nhị phân tùy chỉnh với 32 opcodes** đạt kích thước đường truyền luôn nhỏ hơn văn bản ngôn ngữ tự nhiên (cải tiến gấp 16.5 lần so với định dạng CBOR trước đây).
3. **Mã hóa độ dài biến đổi phân tầng ngữ nghĩa** trong đó độ rộng byte tương quan với tần suất khái niệm.
4. **Siêu dữ liệu tích hợp CRDT** cho phép nhất quán hoàn toàn phi tập trung mà không cần các giao thức đồng thuận.
5. **Nâng cấp nhận thức dựa trên quan sát** qua 11 cấp độ với các tiêu chí chuyển đổi đo lường được.
6. **Quy trình mã hóa 3 tầng** từ phân tích cú pháp văn bản dựa trên quy tắc đến gọi hàm AI và Đồng thuận Mã hóa phân tán với xác minh 2 pha cùng phần thưởng token OBT.
7. **Hệ thống miễn dịch độc lập với nội dung** phát hiện hành vi thao túng thông qua các mẫu hành vi, không phải qua kiểm duyệt nội dung.

Không có hệ thống nào trước đây được xác định trong phần tổng quan tài liệu của chúng tôi (§2) kết hợp quá hai trong số tám khả năng này.

## 7.2 Các hạn chế

Hệ thống KU hiện tại có một số hạn chế cần được ghi nhận:

**L1: Chưa có dữ liệu triển khai thực tế.** Tất cả các chỉ số hiệu năng đều được rút ra từ các kiểm thử tổng hợp và kiểm thử đơn vị. Hệ thống chưa được triển khai trong một mạng lưới sản xuất thực tế, và các phân phối ID khái niệm, kích thước KU, và tần suất merge CRDT trong thế giới thực có thể khác so với các giả định của chúng tôi.

**L2: Rủi ro tập trung hóa của sổ đăng ký khái niệm (concept registry).** Mặc dù các ID khái niệm là phổ quát về mặt số học, việc ánh xạ từ các thuật ngữ ngôn ngữ tự nhiên sang ID khái niệm yêu cầu một sổ đăng ký dùng chung. Thiết kế hiện tại bao gồm một phạm vi ID khái niệm tạm thời (`0xF0000000+`) cho các khái niệm chưa đăng ký, nhưng việc quản trị không gian tên khái niệm chuẩn tắc vẫn là một vấn đề mở.

**L3: Sự phụ thuộc vào vector nhúng (embedding).** Vector nhúng 512-byte và vector nhúng nhị phân 128-byte của phần Epigenetic giả định tính sẵn có của một mô hình embedding cụ thể. Việc kiểm soát phiên bản mô hình (trường `embed_version`) giảm thiểu điều này, nhưng khả năng tương thích embedding giữa các mô hình khác nhau không được đảm bảo.

**L4: Xác thực hình thức hạn chế.** Mặc dù các triển khai CRDT vượt qua 267 kiểm thử, chúng chưa được đưa vào xác thực hình thức (ví dụ: sử dụng TLA+ hoặc Coq). Các chứng minh hội tụ trong §5.3 là các phác thảo không chính thức dựa trên các thuộc tính join semi-lattice được thiết lập bởi Shapiro và các cộng sự [14].

**L5: Triển khai đơn ngôn ngữ.** Triển khai Rust hiện tại là triển khai tham chiếu duy nhất. Khả năng tương thích với các ngôn ngữ khác phụ thuộc vào đặc tả wire format, vốn chưa được triển khai và xác thực độc lập.

**L6: Sự phân tán của ConceptDict.** `ConceptDict` ánh xạ các thuật ngữ ngôn ngữ tự nhiên sang các ConceptID dạng số và hiện đang được lưu trữ trong bộ nhớ. Sự phân tán toàn cầu và đồng thuận trên các ánh xạ khái niệm giữa các nút vẫn là một vấn đề mở — việc di chuyển theo kế hoạch sang SQLite cung cấp khả năng lưu trữ cục bộ lâu dài, nhưng việc quản trị sổ đăng ký khái niệm toàn mạng lưới vẫn chưa được thiết kế.

## 7.3 Hướng nghiên cứu tương lai

### 7.3.1 Ngắn hạn (Pha 2)

- **Tích hợp cơ sở dữ liệu đồ thị.** 33 kiểu liên kết hiện đang mã hóa các mối quan hệ liên KU trong wire format, nhưng cần một cơ sở dữ liệu đồ thị chuyên dụng (ví dụ: Neo4j, công cụ đồ thị Rust tùy chỉnh) để duyệt đồ thị hiệu quả, phát hiện khoảng trống tri thức, và tìm kiếm các cầu nối liên miền. Mô hình Swanson ABC [40] cho tri thức công cộng chưa được khám phá đã được tạo nguyên mẫu trong KQL Discovery Engine.

- **Bộ kết xuất Lớp Expression.** Bộ kết xuất từ Core DNA → văn bản ngôn ngữ tự nhiên cần được triển khai — hiện tại, văn bản được tạo ad-hoc từ tra cứu ConceptDict. Một bộ kết xuất Lớp Expression hình thức sẽ hỗ trợ nhiều ngôn ngữ đầu ra và phong cách định dạng khác nhau.

- **Lưu trữ lâu dài ConceptDict bằng SQLite.** Di chuyển ConceptDict lưu trong bộ nhớ sang SQLite cung cấp khả năng lưu trữ khái niệm lâu dài, có thể truy vấn được. Đây là điều kiện tiên quyết cho các luồng công việc mã hóa AI nhiều phiên (multi-session).

- **Tích hợp token OBT.** Điểm Proof-of-Metabolic-Value (PoMV) được tính toán bởi công cụ đồng thuận 12 mô-đun phải được kết nối với một cơ chế đúc và phân phối token thực tế. Mô hình kinh tế (60% khai thác tri thức, 15% quỹ phát triển, 15% cộng đồng, 10% đội ngũ) đã được thiết kế nhưng chưa triển khai.

### 7.3.2 Trung hạn (Pha 3–4)

- **Phân giải khái niệm đa ngôn ngữ.** Sơ đồ ID khái niệm hiện tại hỗ trợ mã hóa độc lập ngôn ngữ, nhưng phân giải đầu vào ngôn ngữ tự nhiên sang ID khái niệm trên hơn 100 ngôn ngữ yêu cầu tích hợp với các mô hình NLP đa ngôn ngữ và một sổ đăng ký khái niệm phân tán có quản trị.

- **Personal AI SDK.** Một SDK cho phép các trợ lý AI cá nhân (Personal AI) tạo, truy vấn và tiêu thụ các KU thông qua mạng lưới OneBrain. Điều này bao gồm thu thập tri thức tự động từ hoạt động người dùng (Giai đoạn 2 trong tiến trình phát triển chia sẻ tri thức) và phân phối tri thức được cá nhân hóa.

- **Xác thực hình thức.** Áp dụng kiểm thử dựa trên thuộc tính (property-based testing - QuickCheck/proptest) và có thể là mô hình hóa TLA+ để xác thực các đảm bảo hội tụ CRDT và tính an toàn của việc phân tích cú pháp wire format cho tất cả các trường hợp biên.

### 7.3.3 Dài hạn (Pha 5)

- **Giao thức giao diện não-máy tính (BCI).** Wire format của KU được thiết kế hướng tới khả năng tương thích BCI: mã hóa nhị phân, biểu diễn nhỏ gọn, và khả năng truyền phát thời gian thực định vị nó như một mục tiêu mã hóa thần kinh tiềm năng. Kiểu gen Sensory (§3.5) đã hỗ trợ mã hóa tri thức đặc thù phương thức có thể giao tiếp với các luồng dữ liệu BCI.

- **Mã hóa tri thức trải nghiệm.** Kiểu gen Experience và Sensory đặt nền móng cho việc mã hóa không chỉ tri thức sự thật mà còn cả các trải nghiệm chủ quan — bao gồm dữ liệu cảm giác, trạng thái cảm xúc (qua mô hình cảm xúc VAD), và bối cảnh không gian-thời gian. Mã hóa trải nghiệm đầy đủ sẽ yêu cầu các tiến bộ trong biểu diễn dữ liệu thần kinh và các mô hình cảm xúc chuẩn hóa.

- **Bản đồ Tri thức Toàn cầu.** Một trực quan hóa có thể điều hướng của toàn bộ tri thức nhân loại, được tổ chức dưới dạng Đồ thị Tri thức với 33 kiểu liên kết, có thể duyệt bởi bất kỳ trợ lý Personal AI nào trên mạng lưới. Đây là tầm nhìn tối thượng của OneBrain: một biểu diễn sống động của tri thức tập thể của nhân loại.

## 7.4 Kết luận

Tài liệu này đã trình bày về **Knowledge Unit (KU)**, một mô hình biểu diễn tri thức lấy cảm hứng từ sinh học được thiết kế như cấu trúc dữ liệu nền tảng cho các mạng lưới tri thức phi tập trung. Dựa trên các nguyên tắc kiến trúc của di truyền học phân tử, chúng tôi đã giới thiệu **kiến trúc ba lớp** — Core DNA (luồng lệnh nhị phân nhỏ gọn), Epigenetics (siêu dữ liệu và độ tin cậy thời điểm chạy), và Expression (kết xuất ngôn ngữ tự nhiên) — mã hóa tri thức nhân loại dưới một định dạng đồng thời nhỏ gọn, khả năng biểu đạt cao, đáng tin cậy và phi tập trung.

Tám đóng góp chính của chúng tôi là:

1. **Một mô hình biểu diễn tri thức ba lớp lấy cảm hứng từ sinh học** (§3) ánh xạ các khái niệm sinh học (trình tự DNA, dấu vết biểu di truyền, kiểu hình) tới các lớp mã hóa tri thức (nhị phân Core DNA, siêu dữ liệu runtime, kết xuất văn bản), với 11 kiểu gen và 33 kiểu liên kết (bond types) trải dài trên 8 danh mục ngữ nghĩa.

2. **Một tập lệnh nhị phân tùy chỉnh với 32 opcodes** (§4) đạt kích thước đường truyền luôn **nhỏ hơn văn bản ngôn ngữ tự nhiên** — xấp xỉ **16 bytes** cho một sự thật tối thiểu, **88 bytes** cho một mã hóa tri thức tiếng Việt điển hình, và **172 bytes** cho một mô tả hệ thống tên lửa 5-KU toàn diện (so với 1,078 bytes của văn bản gốc).

3. **Một mã hóa số nguyên độ dài biến đổi phân tầng ngữ nghĩa** (§4.5) gán độ rộng byte dựa trên tần suất khái niệm, đạt mức kỳ vọng 1.89 bytes cho mỗi ID khái niệm (tiết kiệm 76.4% so với mã hóa độ rộng cố định) với khả năng xác định độ dài $O(1)$ từ byte đầu tiên.

4. **Tích hợp năm kiểu CRDT** (§5) — GCounter, PNCounter, LWWRegister, ORSet, và VectorClock — cho phép siêu dữ liệu tri thức nhất quán sau cùng, phi tập trung hoàn toàn mà không yêu cầu các giao thức đồng thuận hay cơ quan trung tâm.

5. **Một khung nhận thức độc lập nội dung** (§3.6) với 11 cấp độ trưởng thành của tri thức, 9 kiểu bằng chứng liên kết GRADE, và trường bit nhạy cảm lỗi 16-bit, cung cấp từ vựng có cấu trúc để thể hiện sự không chắc chắn trong môi trường phi tập trung.

6. **Một quy trình mã hóa 3 tầng** (§4.9) từ phân tích cú pháp văn bản dựa trên quy tắc (ngoại tuyến, độ chính xác ~60–70%) qua gọi hàm AI cục bộ (15 công cụ, runtime có thể cắm nóng) đến Đồng thuận Mã hóa phân tán — một vòng đời 4 trạng thái (RAW → SELF → PART → FULL) với xác minh 2 pha (đồng thuận phân rã AI + vòng phản hồi mã hóa công cụ), chấm điểm đồng thuận có trọng số ($S_{\text{consensus}} = 0.50 \cdot S_{\text{agreement}} + 0.30 \cdot S_{\text{detail}} + 0.20 \cdot S_{\text{reputation}}$), và phần thưởng token OBT khi tham gia xác minh.

7. **Một triển khai mã nguồn mở toàn diện** (§6) bao gồm ~10,000+ dòng mã Rust trên 27 mô-đun với 267 bài kiểm thử, bao phủ các chu trình khứ hồi mã hóa/giải mã Core DNA, các mẫu phân tích cú pháp văn bản, các luồng công việc của bộ thực thi công cụ AI, và xác thực hợp nhất CRDT toàn diện.

Hệ thống Knowledge Unit định vị mình tại điểm giao thoa của biểu diễn tri thức, hệ thống phân tán, và tính toán lấy cảm hứng từ sinh học — ba lĩnh vực vốn đã tiến hóa độc lập trong lịch sử. Bằng cách kết hợp các hiểu biết sâu sắc từ cả ba lĩnh vực, chúng tôi trình bày một cách tiếp cận mới đối với thách thức cơ bản của quản lý tri thức phi tập trung: làm thế nào để mã hóa, chia sẻ, và tiến hóa tri thức nhân loại trên hàng triệu nút không đồng nhất mà không cần điều phối trung tâm.

Khi các hệ thống AI ngày càng đóng vai trò trung gian trong việc tiếp nhận và chia sẻ tri thức của con người, nhu cầu về một biểu diễn tri thức phi tập trung, đáng tin cậy và được chuẩn hóa trở nên cấp thiết hơn bao giờ hết. Knowledge Unit — đủ nhỏ gọn cho truyền tải di động (nhỏ hơn văn bản mà nó mã hóa), đủ khả năng biểu đạt cho toàn bộ phổ nhận thức của con người (11 kiểu gen, 32 opcodes), và đủ mạnh mẽ cho hoạt động phi tập trung (5 kiểu CRDT, tính toàn vẹn CRC-16) — là đóng góp của chúng tôi cho mục tiêu này.

> *"Không tri thức nào bị lãng phí. Không ý tưởng nào bị lãng quên. Không bộ não nào phải chiến đấu đơn độc."*
> — Tuyên ngôn OneBrain

---

## References

[1] S. Ji, S. Pan, E. Cambria, P. Marttinen, and P. S. Yu, "A Survey on Knowledge Graphs: Representation, Acquisition, and Applications," *IEEE Transactions on Neural Networks and Learning Systems*, vol. 33, no. 2, pp. 494–514, 2022.

[2] A. Singhal, "Introducing the Knowledge Graph: Things, Not Strings," *Google Official Blog*, May 2012.

[3] J. Lehmann *et al.*, "DBpedia — A Large-scale, Multilingual Knowledge Base Extracted from Wikipedia," *Semantic Web Journal*, vol. 6, no. 2, pp. 167–195, 2015.

[4] D. Vrandečić and M. Krötzsch, "Wikidata: A Free Collaborative Knowledgebase," *Communications of the ACM*, vol. 57, no. 10, pp. 78–85, 2014.

[5] R. Cyganiak, D. Wood, and M. Lanthaler, "RDF 1.1 Concepts and Abstract Syntax," W3C Recommendation, Feb. 2014.

[6] W3C OWL Working Group, "OWL 2 Web Ontology Language Document Overview (Second Edition)," W3C Recommendation, Dec. 2012.

[7] M. Minsky, "A Framework for Representing Knowledge," *MIT AI Laboratory Memo 306*, Jun. 1974.

[8] M. R. Quillian, "Semantic Memory," Ph.D. dissertation, Carnegie Mellon University, 1968.

[9] J. F. Sowa, *Conceptual Structures: Information Processing in Mind and Machine*. Reading, MA: Addison-Wesley, 1984.

[10] F. M. Suchanek, G. Kasneci, and G. Weikum, "YAGO: A Core of Semantic Knowledge," in *Proc. 16th International Conference on World Wide Web (WWW '07)*, pp. 697–706, 2007.

[11] J. Benet, "IPFS — Content Addressed, Versioned, P2P File System," *arXiv preprint arXiv:1407.3561*, 2014.

[12] D. J. Trautwein *et al.*, "Design and Evaluation of IPFS: A Storage Layer for the Decentralized Web," in *Proc. ACM SIGCOMM '22*, 2022.

[13] A. V. Sambra *et al.*, "Solid: A Platform for Decentralized Social Applications Based on Linked Data," *MIT CSAIL & Qatar Computing Research Institute*, 2016.

[14] M. Shapiro, N. Preguiça, C. Baquero, and M. Zawirski, "A Comprehensive Study of Convergent and Commutative Replicated Data Types," *INRIA Research Report RR-7506*, 2011.

[15] M. Shapiro, N. Preguiça, C. Baquero, and M. Zawirski, "Conflict-free Replicated Data Types," in *Proc. 13th International Symposium on Stabilization, Safety, and Security of Distributed Systems (SSS '11)*, LNCS 6976, pp. 386–400, 2011.

[16] N. Preguiça, C. Baquero, and M. Shapiro, "Conflict-free Replicated Data Types (CRDTs)," *arXiv preprint arXiv:1805.06358*, 2018.

[17] H. Sanjuán, S. Poyhtari, P. Dias, and J. Bullón, "Merkle-CRDTs: Merkle-DAGs meet CRDTs," *arXiv preprint arXiv:2004.00107*, 2020.

[18] M. Sporny, D. Reed *et al.*, "Decentralized Identifiers (DIDs) v1.0," W3C Recommendation, Jul. 2022.

[19] C. Bormann and P. Hoffman, "Concise Binary Object Representation (CBOR)," *IETF RFC 8949 (STD 94)*, Dec. 2020.

[20] Google Inc., "Protocol Buffers: Developer Guide," 2008. [Online]. Available: https://protobuf.dev/

[21] S. Furuhashi, "MessagePack: It's like JSON but fast and small," 2008. [Online]. Available: https://msgpack.org/

[22] K. Varda, "Cap'n Proto: Introduction," 2013. [Online]. Available: https://capnproto.org/

[23] Google Inc., "FlatBuffers: An Efficient Cross Platform Serialization Library," 2014.

[24] J. C. Viotti and M. Kinderkhedia, "A Benchmark of JSON-compatible Binary Serialization Specifications," *arXiv preprint arXiv:2201.03051*, 2022.

[25] P.-P. Grassé, "La reconstruction du nid et les coordinations interindividuelles chez Bellicositermes natalensis et Cubitermes sp.," *Insectes Sociaux*, vol. 6, pp. 41–80, 1959.

[26] F. Heylighen, "Stigmergy as a Universal Coordination Mechanism: Components, Varieties and Applications," *Human Ecology Special Issue*, 2016.

[27] E. Bonabeau, M. Dorigo, and G. Theraulaz, *Swarm Intelligence: From Natural to Artificial Systems*. Oxford University Press, 1999.

[28] D. O. Hebb, *The Organization of Behavior: A Neuropsychological Theory*. Wiley, 1949.

[29] L. N. de Castro and J. Timmis, *Artificial Immune Systems: A New Computational Intelligence Approach*. Springer, 2002.

[30] S. Forrest, A. S. Perelson, L. Allen, and R. Cherukuri, "Self-Nonself Discrimination in a Computer," in *Proc. 1994 IEEE Symposium on Security and Privacy*, pp. 202–212, 1994.

[31] G. E. Hutchinson, "Concluding Remarks," *Cold Spring Harbor Symposia on Quantitative Biology*, vol. 22, pp. 415–427, 1957.

[32] C. E. Alchourrón, P. Gärdenfors, and D. Makinson, "On the Logic of Theory Change: Partial Meet Contraction and Revision Functions," *Journal of Symbolic Logic*, vol. 50, no. 2, pp. 510–530, 1985.

[33] S. D. Kamvar, M. T. Schlosser, and H. Garcia-Molina, "The EigenTrust Algorithm for Reputation Management in Peer-to-Peer Networks," in *Proc. 12th International Conference on World Wide Web (WWW '03)*, pp. 640–651, 2003.

[34] R. Booth and A. Hunter, "Trust-Sensitive Belief Revision," *Journal of Artificial Intelligence Research*, vol. 63, pp. 523–580, 2018.

[35] A. Jøsang, R. Ismail, and C. Boyd, "A Survey of Trust and Reputation Systems for Online Service Provision," *Decision Support Systems*, vol. 43, no. 2, pp. 618–644, 2007.

[36] H. A. J. van Ditmarsch, W. van der Hoek, and B. Kooi, *Dynamic Epistemic Logic*. Cambridge University Press, 2007.

[37] P. Matzinger, "Tolerance, Danger, and the Extended Family," *Annual Review of Immunology*, vol. 12, pp. 991–1045, 1994.

[38] `crc32fast` crate, "Fast, SIMD-accelerated CRC32 (IEEE) checksum computation," 2023. [Online]. Available: https://crates.io/crates/crc32fast

[39] J. O'Connor, J.-P. Aumasson, S. Neves, and Z. Wilcox-O'Hearn, "BLAKE3: One function, fast everywhere," 2020. [Online]. Available: https://blake3.io/

[40] D. R. Swanson, "Fish Oil, Raynaud's Syndrome, and Undiscovered Public Knowledge," *Perspectives in Biology and Medicine*, vol. 30, no. 1, pp. 7–18, 1986.

[41] J. Devlin, M.-W. Chang, K. Lee, and K. Toutanova, "BERT: Pre-training of Deep Bidirectional Transformers for Language Understanding," in *Proc. NAACL-HLT*, pp. 4171–4186, 2019.

[42] R. Zhou and K. Hwang, "PowerTrust: A Robust and Scalable Reputation System for Trusted Peer-to-Peer Computing," *IEEE Transactions on Parallel and Distributed Systems*, vol. 18, no. 4, pp. 460–473, 2007.

[43] Cochrane Collaboration, "Cochrane Handbook for Systematic Reviews of Interventions," version 6.4, 2023.

[44] GRADE Working Group, "Grading quality of evidence and strength of recommendations," *BMJ*, vol. 328, pp. 1490, 2004.

[45] OriginTrail, "Decentralized Knowledge Graph White Paper," Trace Labs, 2023. [Online]. Available: https://origintrail.io/

---

*End of Paper — Knowledge Unit: A Bio-Inspired Knowledge Representation for Decentralized Knowledge Networks*
