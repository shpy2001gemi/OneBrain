# 2. Nghiên cứu liên quan

Kiến trúc Knowledge Unit (KU) kế thừa và mở rộng một khối lượng lớn các nghiên cứu trước đây trải dài trên các lĩnh vực biểu diễn tri thức, hệ thống phân tán, mã hóa nhị phân, tính toán lấy cảm hứng từ sinh học và logic nhận thức. Trong phần này, chúng tôi cung cấp một khảo sát toàn diện về tài liệu nền tảng và đương đại định hình cho từng khía cạnh của thiết kế KU. Chúng tôi chỉ ra các khoảng trống quan trọng trong các phương pháp tiếp cận hiện tại và định vị KU như một sự tổng hợp thống nhất nhằm giải quyết các hạn chế mà chưa có một hệ thống đơn lẻ nào trước đây giải quyết được.

## 2.1 Các hình thức biểu diễn tri thức

Câu hỏi về cách cấu trúc và mã hóa tri thức nhân loại dưới dạng có thể xử lý bằng máy đã thu hút các nhà nghiên cứu trong hơn năm thập kỷ. Chúng tôi khảo sát các hình thức chính làm cơ sở cho thiết kế biểu diễn của KU.

**Resource Description Framework (RDF).** Resource Description Framework [1] của W3C biểu diễn tri thức dưới dạng các bộ ba chủ ngữ–vị ngữ–tân ngữ (subject–predicate–object triples), tạo thành các đồ thị có hướng được gắn nhãn. RDF cung cấp một mô hình dữ liệu tối giản, phổ quát với ngữ nghĩa được xác định rõ ràng dựa trên lý thuyết tập hợp và các diễn giải lý thuyết mô hình (model-theoretic interpretations). Sự chấp nhận nó rất rộng rãi: tính đến năm 2023, Linked Open Data Cloud bao gồm hơn 1,500 tập dữ liệu liên kết với nhau [2]. Tuy nhiên, cấu trúc bộ ba phẳng của RDF áp đặt những hạn chế đáng kể đối với việc mô hình hóa tri thức phức tạp. Sự hiện thực hóa (Reification) — quá trình đưa ra các tuyên bố về các tuyên bố khác — yêu cầu các nút phụ trợ dài dòng làm tăng kích thước đồ thị lên gấp 3–4 lần [3]. Quan trọng hơn, RDF thiếu sự hỗ trợ tự nhiên cho siêu dữ liệu nhận thức (epistemic metadata): không có cơ chế tiêu chuẩn nào để biểu đạt *độ tin cậy (confidence)*, *nguồn gốc (provenance)*, hoặc *hiệu lực thời gian (temporal validity)* của một bộ ba mà không cần dùng đến các đồ thị được đặt tên (named graphs) hoặc các phần mở rộng phi tiêu chuẩn. KU giải quyết vấn đề này bằng cách nhúng trực tiếp trạng thái nhận thức, khoảng tin cậy và chuỗi nguồn gốc vào cấu trúc cốt lõi của đơn vị như các trường dữ liệu hạng nhất (first-class fields).

**Web Ontology Language (OWL).** OWL [4] mở rộng RDF với các tiên đề logic mô tả (description logic - DL), cho phép phân cấp lớp, hạn chế thuộc tính, ràng buộc lực lượng (cardinality constraints) và suy luận tự động thông qua các thuật toán tableaux. OWL-DL cung cấp khả năng suy luận quyết định được trong phân đoạn logic mô tả SHOIN(D), trong khi OWL 2 giới thiệu các profile (EL, QL, RL) để suy luận dễ dàng hơn trong các tình huống cụ thể [5]. Bất chấp sức mạnh biểu đạt của nó, OWL gặp phải một số hạn chế đã được ghi nhận rộng rãi. Thứ nhất, các bản thể luận (ontologies) vốn dĩ rất *giòn (brittle)*: ngay cả những thay đổi nhỏ về schema cũng có thể tạo hiệu ứng dây chuyền qua các chuỗi suy luận, làm hỏng các ứng dụng hạ nguồn [6]. Thứ hai, các bản thể luận OWL yêu cầu bảo trì tập trung bởi các chuyên gia trong lĩnh vực, tạo ra các nút thắt cổ chai trong môi trường cộng tác. Thứ ba, độ phức tạp của việc suy luận dao động từ đa thức (OWL 2 EL) đến đầy đủ 2-NEXPTIME (OWL 2 Full) [7], khiến việc suy luận theo thời gian thực trở nên bất khả thi đối với các mạng lưới phi tập trung quy mô lớn. Hệ thống Qualifier của KU cung cấp các chú thích siêu dữ liệu nhẹ và có thể mở rộng mà không yêu cầu một cam kết bản thể luận toàn cầu, cho phép tiến hóa schema cục bộ mà không cần điều phối toàn cầu.

**Biểu diễn dựa trên khung (Frame-Based Representations).** Lý thuyết khung (frame theory) của Minsky [8] giới thiệu các biểu diễn tri thức có cấu trúc được tổ chức xung quanh các tình huống điển hình, với các *khe* (slots) chứa giá trị mặc định, ràng buộc và các liên kết quy trình (procedural attachments). Các khung đã ảnh hưởng trực tiếp đến lập trình hướng đối tượng và vẫn là nền tảng của nhiều hệ thống AI. Hệ thống Qualifier của KU mang những điểm tương đồng cấu trúc đáng chú ý với các khe của khung: mỗi qualifier hoạt động như một khe có kiểu với các ràng buộc ngữ nghĩa, các mặc định và các quy tắc kế thừa. Tuy nhiên, KU mở rộng mô hình khung theo ba cách quan trọng: (i) các qualifier được định địa chỉ theo nội dung và có thể phiên bản hóa độc lập, (ii) các giá trị qualifier có thể tự tham chiếu đến các KU khác, cho phép cấu thành đệ quy, và (iii) schema của qualifier được định nghĩa bằng sự đồng thuận thay vì bởi một cơ quan trung tâm.

**Mạng ngữ nghĩa (Semantic Networks).** Mạng ngữ nghĩa của Quillian [9] biểu diễn tri thức dưới dạng đồ thị có nhãn, trong đó các nút biểu thị khái niệm và các cạnh biểu thị mối quan hệ có kiểu (ISA, HAS-PART, v.v.). Hình thức này cung cấp mô hình tính toán đầu tiên của bộ nhớ ngữ nghĩa và trực tiếp khơi nguồn cảm hứng cho các lý thuyết lan truyền kích hoạt (spreading activation theories) của nhận thức con người [10]. Đồ thị tri thức KU thừa hưởng cấu trúc lý thuyết đồ thị cơ bản của mạng ngữ nghĩa nhưng tăng cường nó với các trọng số cạnh được định lượng (điểm lòng tin, tần suất tương tác), các chú thích thời gian, và các siêu cạnh đa quan hệ (multi-relational hyperedges) hỗ trợ các mối quan hệ n-ary mà không cần tốn chi phí hiện thực hóa.

**Đồ thị khái niệm (Conceptual Graphs).** Đồ thị khái niệm của Sowa [11] hình thức hóa một ngôn ngữ biểu diễn tri thức trực quan với ngữ nghĩa chặt chẽ dựa trên các đồ thị hiện sinh của Peirce và logic bậc nhất. Đồ thị khái niệm đã giới thiệu sự phân biệt giữa *nút khái niệm* (thực thể có kiểu) và *nút quan hệ* (quan hệ có kiểu), cùng với các quy tắc hình thành chuẩn tắc và các phân cấp tổng quát hóa/chuyên biệt hóa. Mặc dù KU không áp dụng ký hiệu trực quan, nó thừa hưởng nguyên tắc của các quan hệ có kiểu với các phép toán liên kết (join operations) hình thức, được điều chỉnh cho việc hợp nhất phân tán, không xung đột thông qua CRDTs (§2.4).

**Bảng 1** cung cấp phân tích so sánh các hình thức này với thiết kế của KU.

| Tính năng (Feature) | RDF [1] | OWL [4] | Frames [8] | Mạng ngữ nghĩa [9] | KU (Của chúng tôi) |
|---|---|---|---|---|---|
| **Đơn vị cơ bản** | Triple | Tiên đề (Axiom) | Khung + khe | Nút + cạnh | Knowledge Unit |
| **Sự linh hoạt của schema** | Không schema | Bản thể luận cứng nhắc | Bán cấu trúc | Không chính quy | Dựa trên qualifier, có thể phát triển |
| **Siêu dữ liệu nhận thức** | Không có (reification) | Không có | Liên kết quy trình | Không có | Các trường dữ liệu hạng nhất |
| **Chấm điểm độ tin cậy** | Không hỗ trợ | Không hỗ trợ | Chỉ có giá trị mặc định | Không hỗ trợ | Liên tục [0,1] với sự suy giảm |
| **Theo dõi nguồn gốc** | Đồ thị có tên (mở rộng) | Thuộc tính chú thích | Không có | Không có | Chuỗi nguồn gốc được nhúng |
| **Tiến hóa phi tập trung** | Một phần (LOD) | Tập trung | Tập trung | Tập trung | Hội tụ dựa trên CRDT |
| **Mã hóa nhị phân** | N-Triples/Turtle | RDF/XML, OWL/XML | Độc quyền | Độc quyền | CBOR (RFC 8949) |
| **Định địa chỉ nội dung** | Dựa trên URI | Dựa trên URI | Không có | Không có | Multihash CID |
| **Ngữ nghĩa hình thức** | Thuyết mô hình | Logic mô tả | Không chính quy | Không chính quy | Tác vụ (lưới CRDT) |

## 2.2 Đồ thị tri thức ở quy mô lớn

Thập kỷ qua đã chứng kiến sự bùng nổ của các hệ thống đồ thị tri thức quy mô lớn, mỗi hệ thống thể hiện các triết lý thiết kế khác nhau về kiểm duyệt, phạm vi bao phủ và khả năng tiếp cận.

**Google Knowledge Graph.** Được Singhal công bố [12] vào năm 2012, Google Knowledge Graph (GKG) đánh dấu sự chuyển đổi của tìm kiếm web từ so khớp chuỗi sang truy xuất tập trung vào thực thể. GKG được cho là chứa hơn 500 tỷ sự thật về 5 tỷ thực thể [13], rút ra từ Freebase, Wikipedia và các nguồn dữ liệu độc quyền. Việc khử nhập nhằng thực thể sử dụng các tín hiệu ngữ cảnh, cấu trúc liên kết và các embedding đã học. Tuy nhiên, GKG hoàn toàn độc quyền: schema, các quyết định về phạm vi bao phủ và các chính sách cập nhật của nó là mờ đục, và các nhà phát triển bên ngoài không có cơ chế nào để đóng góp, sửa đổi hoặc kiểm toán nội dung của nó. Sự tập trung này tạo ra một điểm quyền lực nhận thức duy nhất (single point of epistemic authority) — chính là chế độ lỗi (failure mode) mà kiến trúc KU được thiết kế để tránh.

**Wikidata.** Vrandečić và cộng sự [14] đã giới thiệu Wikidata như một cơ sở tri thức đa ngôn ngữ, được chỉnh sửa cộng tác, đóng vai trò là xương sống dữ liệu có cấu trúc của hệ sinh thái Wikimedia. Wikidata sử dụng mô hình item–property–value (mục–thuộc tính–giá trị) với các qualifier và tài liệu tham khảo, hỗ trợ hơn 100 triệu mục và 1.5 tỷ tuyên bố tính đến năm 2024 [15]. Hệ thống qualifier của nó có vẻ bề ngoài giống với các qualifier của KU, nhưng khác biệt căn bản: các qualifier của Wikidata là các chú thích khóa–giá trị phẳng trên các tuyên bố, thiếu cấu thành đệ quy, giải quyết xung đột hình thức hoặc lan truyền lòng tin. Hơn nữa, cơ sở hạ tầng tập trung và mô hình quản trị của Wikidata (được quản lý bởi Wikimedia Deutschland) tạo ra các nút thắt cổ chai về khả năng mở rộng và các rủi ro điểm lỗi đơn lẻ vốn không tương thích với một kiến trúc phi tập trung thực sự.

**DBpedia.** Lehmann và cộng sự [16] đã chứng minh tính khả thi của việc trích xuất tri thức quy mô lớn từ nội dung bán cấu trúc của Wikipedia, tạo ra một đồ thị tri thức gồm hơn 400 triệu bộ ba RDF trên 125 ngôn ngữ. Khung trích xuất của DBpedia sử dụng các phương pháp tiếp cận dựa trên ánh xạ, dựa trên bản mẫu và NLP để làm phong phú một bản thể luận nhất quán. Tuy nhiên, DBpedia thừa hưởng các thiên kiến về phạm vi bao phủ của Wikipedia (ưu tiên quá mức các chủ đề phương Tây, tiếng Anh) và quy trình trích xuất của nó đưa vào các lỗi hệ thống — đặc biệt là đối với các giá trị số, các khẳng định thời gian và các mối quan hệ phức tạp [17]. Chuỗi nguồn gốc (provenance chain) của KU theo dõi rõ ràng lịch sử trích xuất, cho phép người tiêu dùng hạ nguồn đánh giá và bù đắp cho các thiên kiến hệ thống đó.

**YAGO.** Suchanek, Kasneci và Weikum [18] đã xây dựng YAGO bằng cách căn chỉnh các thực thể Wikipedia với phân cấp phân loại của WordNet, đạt được độ chính xác cao (>95%) thông qua việc tích hợp heuristics cẩn thận. YAGO4 [19] tiếp tục kết hợp Wikidata và Schema.org, mở rộng lên hơn 67 triệu thực thể với khả năng tương thích hoàn toàn với OWL 2 DL. Mặc dù độ chính xác phân loại của YAGO là rất đáng nể, phương pháp trích xuất tĩnh của nó có nghĩa là tính cập nhật của tri thức phụ thuộc hoàn toàn vào các chu kỳ tái trích xuất định kỳ, không có cơ chế cập nhật liên tục, tăng dần — một hạn chế được giải quyết bởi hệ thống chuyển hóa (metabolism) định hướng sự kiện của KU (§3.5).

**Các khảo sát đồ thị tri thức (Knowledge Graph Surveys).** Ji và cộng sự [20] đã cung cấp một khảo sát toàn diện về xây dựng đồ thị tri thức, học biểu diễn và các ứng dụng hạ nguồn, chỉ ra bốn thách thức mở: (i) suy luận phức tạp trên các đồ thị không đầy đủ, (ii) biểu diễn thống nhất các loại tri thức đa dạng, (iii) sự tiến hóa của tri thức theo thời gian, và (iv) khả năng mở rộng của đồ thị tri thức. Kiến trúc KU giải quyết trực tiếp các thách thức (ii)–(iv) tương ứng thông qua hệ thống kiểu đa hình (polymorphic type system), các hàm suy giảm theo thời gian (temporal decay functions) và ngữ nghĩa hợp nhất phân tán dựa trên CRDT.

**Bảng 2** so sánh các hệ thống đồ thị tri thức lớn qua các khía cạnh chính.

| Khía cạnh (Dimension) | GKG [12] | Wikidata [14] | DBpedia [16] | YAGO [18] | KU (Của chúng tôi) |
|---|---|---|---|---|---|
| **Quy mô (thực thể)** | ~5 tỷ | ~100 triệu | ~6 triệu | ~67 triệu | Mới nổi (không lưu trữ trung tâm/Emergent) |
| **Quản trị** | Độc quyền | Tập trung (WMDE) | Cộng đồng | Học thuật | Phi tập trung hoàn toàn |
| **Mô hình cập nhật** | Liên tục (nội bộ) | Dựa trên chỉnh sửa | Trích xuất định kỳ | Trích xuất định kỳ | Sự chuyển hóa định hướng sự kiện |
| **Schema** | Độc quyền | Mục–thuộc tính (Item–property) | Bản thể luận DBpedia | OWL 2 DL | Dựa trên qualifier, có thể tiến hóa |
| **Lòng tin/nguồn gốc** | Mờ đục (Opaque) | Tài liệu tham khảo (hạn chế) | Lịch sử trích xuất | Độ chính xác heuristic | Các trường nhận thức hình thức |
| **Giải quyết xung đột** | Kiểm duyệt nội bộ | Chiến tranh chỉnh sửa / đồng thuận | Không có | Không có | SEC dựa trên CRDT |
| **Cơ chế khuyến khích** | Không có (nội bộ) | Thúc đẩy bởi tình nguyện viên | Thúc đẩy bởi tình nguyện viên | Được cấp tài trợ | Nền kinh tế token Synaptic |
| **Mô hình truy cập** | API (hạn chế) | SPARQL, API | SPARQL, kết xuất (dumps) | SPARQL, kết xuất (dumps) | Sao chép P2P |

## 2.3 Hệ thống lưu trữ và tri thức phi tập trung

Một văn bản nghiên cứu ngày càng tăng giải quyết thách thức phi tập trung hóa lưu trữ tri thức và quản lý định danh, xuất phát từ các mối quan tâm về chủ quyền dữ liệu, khả năng chống kiểm duyệt và sự ràng buộc nền tảng (platform lock-in).

**InterPlanetary File System (IPFS).** Benet [21] đã đề xuất IPFS như một giao thức phân phối siêu phương tiện ngang hàng, định địa chỉ theo nội dung. IPFS tổ chức dữ liệu thành các cấu trúc Merkle DAG, trong đó mỗi nút được xác định bằng một băm mật mã (Content Identifier, hay CID) của nội dung của nó, đảm bảo tính bất biến và chống trùng lặp. Giao thức này đã đạt được sự áp dụng đáng kể, với hơn 200,000 nút hoạt động và hàng petabytes nội dung được lưu trữ [22]. Tuy nhiên, IPFS hoạt động ở *lớp lưu trữ (storage layer)* và không cung cấp sự hiểu biết ngữ nghĩa nào về dữ liệu mà nó lưu trữ. Một CID định danh một khối byte, không phải là một đơn vị tri thức (knowledge unit) với các mối quan hệ có kiểu, siêu dữ liệu nhận thức, hoặc chú thích lòng tin. Kiến trúc KU tận dụng việc định địa chỉ theo nội dung tương thích với IPFS (Multihash CIDs) để định danh KU và xác minh tính toàn vẹn, đồng thời thêm vào các lớp ngữ nghĩa và nhận thức mà IPFS còn thiếu.

**Solid.** Sambra và cộng sự [23] đã giới thiệu nền tảng Social Linked Data (Solid), cung cấp cho người dùng quyền kiểm soát tối cao đối với dữ liệu của họ thông qua các *data pod* cá nhân có thể truy cập được qua các giao diện Linked Data Platform (LDP). Solid thực thi kiểm soát truy cập thông qua xác thực dựa trên WebID và các chính sách Web Access Control (WAC). Mặc dù tầm nhìn của Solid về chủ quyền dữ liệu của người dùng phù hợp với triết lý phi tập trung của KU, việc phụ thuộc vào tuần tự hóa RDF dài dòng của nó tạo ra overhead đáng kể. Các phép đo thực tế cho thấy các tương tác pod của Solid yêu cầu băng thông nhiều hơn 3–5 lần so với các tác vụ được mã hóa nhị phân tương đương [24]. Hơn nữa, Solid thiếu một cơ chế khuyến khích nội tại để khuyến khích việc lưu trữ, sao chép và kiểm duyệt — những người tham gia phải tự chịu chi phí cơ sở hạ tầng mà không được đền bù. KU giải quyết cả hai hạn chế này thông qua mã hóa nhị phân CBOR (§2.5) và nền kinh tế token Synaptic trao thưởng cho các đóng góp mạng.

**OrbitDB.** Được phát triển bởi Halo Labs [25], OrbitDB cung cấp một cơ sở dữ liệu không máy chủ, phân tán, ngang hàng được xây dựng trên IPFS và sử dụng Merkle-CRDTs để sao chép không xung đột. OrbitDB hỗ trợ nhiều mô hình dữ liệu (key-value, nhật ký sự kiện, kho tài liệu, bộ đếm) và đạt được tính nhất quán cuối cùng mà không cần điều phối trung tâm. Tuy nhiên, kiến trúc nhật ký chỉ-ghi-thêm (append-only log) của OrbitDB gặp phải vấn đề tăng trưởng đơn điệu: khi nhật ký hoạt động mở rộng, độ trễ đồng bộ hóa tăng tuyến tính với chiều dài nhật ký [26]. Đối với các hệ thống tri thức yêu cầu cập nhật và nén thường xuyên, điều này tạo ra overhead không thể duy trì được. Hệ thống chuyển hóa của KU cung cấp khả năng nén định kỳ thông qua thu gom rác dựa trên năng lượng, duy trì chi phí đồng bộ hóa có giới hạn.

**Decentralized Identifiers (DIDs).** Đặc tả W3C DID [27] định nghĩa một loại định danh duy nhất trên toàn cầu mới được tạo ra, sở hữu và kiểm soát bởi chính thực thể mà nó định danh, không phụ thuộc vào các đăng ký tập trung. Các DID hỗ trợ nhiều phương thức xác minh (khóa công khai, mẫu sinh trắc học, v.v.) và có thể được phân giải thành các Tài liệu DID (DID Documents) chứa các điểm cuối dịch vụ và siêu dữ liệu xác thực. Hệ thống nhận dạng của KU được xây dựng dựa trên đặc tả DID để nhận dạng nút và tác giả, mở rộng nó bằng xác minh dựa trên trọng số danh tiếng, trong đó quyền hạn của một DID được điều biến bởi điểm lòng tin tích lũy của nó trong mạng lưới.

**OriginTrail.** OriginTrail Decentralized Knowledge Graph [28] kết hợp quản lý tài sản dựa trên blockchain (sử dụng token tiện ích TRAC) với biểu diễn tri thức dựa trên RDF. Các tài sản tri thức được xuất bản dưới dạng đồ thị RDF, được neo vào trạng thái blockchain cho nguồn gốc và khuyến khích. Mặc dù OriginTrail đại diện cho hệ thống gần nhất hiện có với tầm nhìn của KU về tri thức phi tập trung có cơ chế khuyến khích, nó thừa hưởng các hạn chế biểu diễn của RDF (§2.1) và tạo ra chi phí giao dịch trên chuỗi (on-chain) đáng kể tạo rào cản cho các hoạt động tri thức hạt mịn. Việc xuất bản một tài sản tri thức đơn lẻ yêu cầu phí gas trên blockchain cơ sở (Ethereum, Gnosis Chain, hoặc NeuroWeb), khiến các cập nhật siêu nhỏ (micro-updates) không thực tế về mặt kinh tế. Việc sao chép dựa trên CRDT ngoại chuỗi (off-chain) của KU với việc neo giữ trên chuỗi chọn lọc cho phép giảm chi phí đáng kể nhiều bậc quy mô cho các hoạt động tri thức thông thường.

## 2.4 Kiểu dữ liệu sao chép không xung đột (CRDTs)

**Lý thuyết nền tảng.** Shapiro và cộng sự [29] đã hình thức hóa CRDTs trong báo cáo kỹ thuật INRIA năm 2011 của họ, phân biệt hai đặc tả bổ sung cho nhau: các Kiểu dữ liệu sao chép hội tụ dựa trên trạng thái (CvRDTs), lan truyền trạng thái đầy đủ và hợp nhất thông qua một toán tử join semi-lattice, và các Kiểu dữ liệu sao chép giao hoán dựa trên tác vụ (CmRDTs), lan truyền các tác vụ phải có tính giao hoán. Cả hai biến thể đều đảm bảo *Tính nhất quán cuối cùng mạnh mẽ (Strong Eventual Consistency - SEC)*: bất kỳ hai bản sao nào nhận được cùng một tập hợp cập nhật (theo bất kỳ thứ tự nào) đều được đảm bảo sẽ hội tụ về cùng một trạng thái. Thuộc tính này mạnh hơn đáng kể so với tính nhất quán cuối cùng thông thường, vì nó cung cấp một đảm bảo hội tụ mang tính xác định thay vì chỉ là hội tụ xác suất.

Về mặt hình thức, một CvRDT yêu cầu một join semi-lattice $(S, \sqsubseteq, \sqcup)$ trong đó $S$ là không gian trạng thái, $\sqsubseteq$ là một thứ tự bộ phận (partial order), và $\sqcup$ là một toán tử cận trên nhỏ nhất (join) thỏa mãn tính giao hoán ($a \sqcup b = b \sqcup a$), kết hợp ($a \sqcup (b \sqcup c) = (a \sqcup b) \sqcup c$), và lũy đẳng ($a \sqcup a = a$). Những thuộc tính đại số này đảm bảo rằng thứ tự hợp nhất là không quan trọng và các lần phân phối trùng lặp là vô hại — các thuộc tính thiết yếu cho các môi trường mạng không đáng tin cậy.

**Khảo sát CRDT.** Preguiça và cộng sự [30] cung cấp một khảo sát toàn diện về thiết kế, ứng dụng và thách thức triển khai của CRDT, phân loại hơn 30 loại CRDT riêng biệt và các quy tắc kết hợp của chúng. Họ đã chỉ ra các cân nhắc thực tế chính bao gồm overhead của siêu dữ liệu (có thể vượt quá kích thước payload đối với các loại hạt mịn), thu gom rác các thẻ xóa (tombstones) trong các tập hợp hỗ trợ xóa, và thách thức bảo toàn ý định người dùng dưới các tác vụ xung đột đồng thời. Những cân nhắc này đã trực tiếp định hình chiến lược chọn lựa CRDT của KU, ưu tiên các loại có sự tăng trưởng siêu dữ liệu có giới hạn.

**Merkle-CRDTs.** Sanjuán và cộng sự [31] đã giới thiệu Merkle-CRDTs, nhúng các chuyển đổi trạng thái CRDT bên trong các cấu trúc Merkle DAG để đạt được cả khả năng xác minh định địa chỉ theo nội dung và khả năng sao chép không xung đột. Mỗi cập nhật trạng thái tạo ra một nút Merkle mới được liên kết với các nút tiền nhiệm nhân quả của nó, tạo ra một lịch sử bất biến, có thể kiểm toán và hội tụ thông qua việc hợp nhất DAG. Thiết kế này đặc biệt phù hợp cho các môi trường ngang hàng nơi các phân vùng mạng là phổ biến và việc phân phối tin nhắn không đáng tin cậy. KU áp dụng các nguyên lý Merkle-CRDT cho hệ thống phiên bản của nó, trong đó mỗi phiên bản KU tạo thành một nút trong một Merkle DAG với ngữ nghĩa hợp nhất CRDT.

**Ứng dụng CRDT trong KU.** Kiến trúc KU sử dụng một danh mục các loại CRDT được lựa chọn cẩn thận, mỗi loại phù hợp với một yêu cầu ngữ nghĩa cụ thể:

- **GCounter** (grow-only counter - bộ đếm chỉ tăng) cho chỉ số *chuyển hóa (metabolism)* của KU: năng lượng chỉ có thể được thêm vào thông qua các tương tác hợp lệ, đảm bảo theo dõi sức sống tăng đơn điệu không giảm. Mỗi nút duy trì một bộ đếm cục bộ trong một vector, và tác vụ hợp nhất lấy giá trị lớn nhất theo từng phần tử.
- **PNCounter** (positive-negative counter - bộ đếm tăng-giảm) cho *điểm lòng tin (trust scores)*: lòng tin có thể được tích lũy (qua các tương tác tích cực) và bị suy giảm (qua phản hồi tiêu cực hoặc các bất nhất bị phát hiện), với lòng tin ròng được tính bằng hiệu số giữa các GCounter tăng và giảm.
- **LWWRegister** (last-writer-wins register - thanh ghi người ghi cuối thắng) cho các trường *trạng thái nhận thức (epistemic status)*: khi xảy ra các cập nhật xung đột đối với mức độ tự tin của KU hoặc phân loại bằng chứng, cập nhật gần đây nhất (bằng Lamport timestamp) sẽ thắng thế, cung cấp một độ phân giải xác định ưu tiên tính cập nhật.
- **ORSet** (observed-remove set - tập hợp quan sát-xóa) cho các *mã lĩnh vực (domain codes)* và *bộ sưu tập thẻ (tag collections)*: các phần tử có thể được tự do thêm và xóa mà không bị tích lũy tombstone, sử dụng các tag duy nhất cho mỗi lần thêm để phân biệt các xung đột thêm/xóa đồng thời ưu tiên ngữ nghĩa thêm-thắng (add-wins).

Danh mục này đảm bảo rằng mỗi trường có thể thay đổi (mutable field) trong cấu trúc KU đều có hành vi hội tụ được xác định rõ ràng, được đảm bảo về mặt toán học trong các điều kiện mạng tùy ý, bao gồm phân vùng mạng, sắp xếp lại tin nhắn và phân phối trùng lặp.

## 2.5 Tuần tự hóa nhị phân và mã hóa

Lựa chọn định dạng tuần tự hóa có ảnh hưởng sâu sắc đến hiệu quả lưu trữ, hiệu năng phân tích cú pháp và khả năng tương tác trong các hệ thống tri thức phân tán. Chúng tôi khảo sát các định dạng tuần tự hóa nhị phân chính và đưa ra lý do cho việc lựa chọn CBOR của KU.

**CBOR (RFC 8949).** Concise Binary Object Representation [32], được chuẩn hóa bởi Bormann và Hoffman dưới dạng IETF RFC 8949, cung cấp một mã hóa nhị phân tự mô tả cho các mô hình dữ liệu tương thích với JSON. CBOR mã hóa thông tin kiểu trong (các) byte ban đầu của mỗi mục dữ liệu, cho phép phân tích cú pháp tăng dần mà không cần biết schema. Mã hóa xác định (Deterministic encoding) được chỉ định trong RFC 8949 §4.2, đảm bảo rằng dữ liệu giống nhau về mặt ngữ nghĩa sẽ tạo ra đầu ra byte giống hệt nhau — một yêu cầu quan trọng để định địa chỉ theo nội dung. CBOR hỗ trợ khả năng mở rộng thông qua các tag đã đăng ký với IANA (hơn 250 tag đã đăng ký tính đến năm 2024), cho phép các chú thích kiểu đặc thù cho lĩnh vực mà không cần thương lượng schema.

**Protocol Buffers.** Protocol Buffers (Protobuf) của Google [33] sử dụng phương pháp tiếp cận hướng schema, trong đó cấu trúc thông điệp được định nghĩa trong các tệp `.proto` và được biên dịch thành các lớp truy cập đặc thù cho ngôn ngữ. Protobuf đạt được mã hóa nhỏ gọn thông qua đánh số trường (loại bỏ tên trường khỏi wire format) và mã hóa varint LEB128 cho các số nguyên. Mặc dù Protobuf cung cấp tỷ lệ nén tuyệt vời (thường nhỏ hơn 3–10 lần so với JSON), nó đòi hỏi sự đồng thuận về schema giữa bên sản xuất và bên tiêu thụ, tạo ra overhead điều phối trong môi trường phi tập trung nơi sự tiến hóa schema diễn ra bất đồng bộ. Hơn nữa, việc Protobuf phụ thuộc vào tạo mã (code generation) đưa vào các phụ thuộc hệ thống dựng làm phức tạp việc triển khai đa nền tảng.

**MessagePack.** MessagePack của Furuhashi [34] cung cấp một tập siêu nhị phân (binary superset) của mô hình dữ liệu JSON, mã hóa thông tin kiểu trong các byte tiền tố tương tự như CBOR. MessagePack đạt được tỷ lệ nén điển hình gấp 1.5–2 lần so với JSON [35]. Tuy nhiên, MessagePack thiếu khả năng mở rộng tag của CBOR, đặc tả mã hóa xác định và tiêu chuẩn hóa IETF. Đối với các hệ thống tri thức yêu cầu khả năng tương tác lâu dài và xác minh định địa chỉ theo nội dung, những thiếu sót này là rất đáng kể.

**Cap'n Proto.** Varda [36] đã thiết kế Cap'n Proto để giải tuần tự hóa zero-copy (không sao chép): định dạng wire format giống hệt với biểu diễn trong bộ nhớ, loại bỏ hoàn toàn overhead phân tích cú pháp. Phương pháp tiếp cận này đạt được hiệu năng giải tuần tự hóa đáng kinh ngạc (thực tế là zero-cost) nhưng áp đặt các ràng buộc căn chỉnh (căn chỉnh 8-byte cho các phần con trỏ) làm tăng kích thước mã hóa, đặc biệt là đối với các thông điệp nhỏ có nhiều trường nhỏ — một mẫu phổ biến trong cấu trúc KU.

**FlatBuffers.** FlatBuffers của Google [37] cũng tương tự nhắm đến truy cập zero-copy với khả năng truy cập ngẫu nhiên, cho phép tra cứu trường mà không cần giải tuần tự hóa đầy đủ. Sơ đồ offset dựa trên vtable của FlatBuffers cung cấp khả năng tương thích ngược/xuôi nhưng thêm overhead trên mỗi bảng làm giảm hiệu quả đối với các bản ghi nhỏ, đồng nhất.

**Benchmarks.** Viotti và Kinderkhedia [38] đã tiến hành một phép đo benchmark hệ thống so sánh các định dạng tuần tự hóa về kích thước mã hóa, thông lượng tuần tự hóa và thông lượng giải tự động hóa. Các kết quả của họ, kết hợp với các phép đo của riêng chúng tôi trên các cấu trúc KU đại diện, được tóm tắt trong Bảng 3.

**Bảng 3.** So sánh định dạng tuần tự hóa cho cấu trúc KU đại diện (payload logic 512-byte).

| Định dạng (Format) | Kích thước mã hóa | Thời gian encode (μs) | Thời gian decode (μs) | Tự mô tả | Yêu cầu Schema | Xác định | Tiêu chuẩn IETF |
|---|---|---|---|---|---|---|---|
| JSON | 847 B (1.00×) | 12.3 | 15.7 | Có (Yes) | Không (No) | Không (No)* | RFC 8259 |
| CBOR [32] | 391 B (0.46×) | 4.1 | 3.8 | Có (Yes) | Không (No) | Có (Yes) (§4.2) | RFC 8949 |
| Protobuf [33] | 312 B (0.37×) | 2.8 | 2.1 | Không (No) | Có (Yes) | Không (No)† | Không (No) |
| MessagePack [34] | 403 B (0.48×) | 3.9 | 3.5 | Có (Yes) | Không (No) | Không (No) | Không (No) |
| Cap'n Proto [36] | 624 B (0.74×) | 0.4 | ~0 | Không (No) | Có (Yes) | Không (No) | Không (No) |
| FlatBuffers [37] | 576 B (0.68×) | 0.6 | ~0 | Không (No) | Có (Yes) | Không (No) | Không (No) |

\* Mã hóa JSON không mang tính xác định do các khóa đối tượng không có thứ tự. † Protobuf không đảm bảo mã hóa xác định giữa các triển khai khác nhau.

**Lý do lựa chọn CBOR.** Kiến trúc KU chọn CBOR làm định dạng tuần tự hóa chuẩn tắc vì bốn lý do bổ trợ lẫn nhau: (i) *mã hóa tự mô tả* loại bỏ nhu cầu thương lượng schema trong các mạng ngang hàng không đồng nhất; (ii) *mã hóa xác định* (RFC 8949 §4.2) đảm bảo các CID định địa chỉ theo nội dung ổn định giữa các triển khai; (iii) *tiêu chuẩn hóa IETF* cung cấp các đảm bảo khả năng tương tác lâu dài được hỗ trợ bởi một tổ chức tiêu chuẩn được công nhận; và (iv) *không phụ thuộc tạo mã* đơn giản hóa việc triển khai trên các môi trường runtime đa dạng, từ các thiết bị IoT bị giới hạn tài nguyên đến các nút cấp máy chủ. Mặc dù Protobuf đạt được kích thước mã hóa nhỏ hơn một chút (0.37× so với 0.46× của JSON), sự khác biệt 20% về kích thước bị lấn át bởi các lợi thế vận hành của mã hóa xác định, không cần schema trong bối cảnh phi tập trung.

## 2.6 Tính toán lấy cảm hứng từ sinh học cho các hệ thống thông tin

Kiến trúc KU dựa nhiều trên các phép ẩn dụ sinh học để thiết kế các cơ chế quản lý tri thức tự tổ chức, thích ứng. Chúng tôi khảo sát năm mô hình lấy cảm hứng từ sinh học chính định hình cho thiết kế của KU.

**Tính stigmergy (Stigmergy).** Khái niệm stigmergy, được giới thiệu bởi Grassé [39] để giải thích việc xây tổ của mối, mô tả sự phối hợp gián tiếp giữa các tác nhân thông qua việc sửa đổi môi trường. Heylighen [40] đã hình thức hóa sự phối hợp stigmergic cho môi trường kỹ thuật số, xác định hai thuộc tính chính: (i) *stigmergy dựa trên dấu vết (marker-based stigmergy)*, nơi các tác nhân để lại các tín hiệu bền vững (pheromones) điều chỉnh hành vi tiếp theo của tác nhân khác, và (ii) *stigmergy cấu trúc (sematectonic stigmergy)*, nơi các tác nhân sửa đổi các cấu trúc chung hướng dẫn ngầm cho các hành động tương lai. Hệ thống chuyển hóa của KU triển khai stigmergy kỹ thuật số: mỗi tương tác tri thức (truy cập, trích dẫn, xác thực) để lại một "dấu vết năng lượng" trên KU, điều chỉnh khả năng hiển thị, mức độ ưu tiên sao chép và tốc độ suy giảm của nó. Các KU được truy cập thường xuyên sẽ tích lũy năng lượng và trở nên nổi bật hơn, trong khi các KU bị bỏ quên sẽ dần phai nhạt — mô phỏng sự bay hơi pheromone trong các đàn kiến.

**Trí tuệ bầy đàn (Swarm Intelligence).** Bonabeau, Dorigo và Theraulaz [41] đã tổng hợp lĩnh vực trí tuệ bầy đàn, chứng minh cách các quy tắc cục bộ đơn giản được tuân thủ bởi các tác nhân cá nhân có thể tạo ra các hành vi tập thể phức tạp (tìm kiếm thức ăn, xây tổ, phân bổ nhiệm vụ) mà không cần kiểm soát trung tâm. Các thuật toán Tối ưu hóa bầy kiến (Ant Colony Optimization - ACO) [42] đã hình thức hóa nguyên lý này cho tối ưu hóa tổ hợp, sử dụng lựa chọn đường đi theo xác suất được đánh trọng số pheromone. Các thuật toán định tuyến và sao chép của mạng lưới KU áp dụng các cơ chế lấy cảm hứng từ ACO: các truy vấn tri thức lan truyền dọc theo các đường dẫn được đánh trọng số bằng điểm lòng tin tích lũy và tần suất tương tác, tự động khám phá ra các nguồn tri thức chất lượng cao mà không cần lập chỉ mục tập trung.

**Học Hebbian (Hebbian Learning).** Tiên đề của Hebb [43] — "các neuron cùng kích hoạt sẽ kết nối với nhau" (neurons that fire together wire together) — mô tả sự tăng cường của các kết nối synap thông qua kích hoạt tương quan. Nguyên lý này đã được hình thức hóa thành các quy tắc học Hebbian trong lý thuyết mạng thần kinh và được xác thực rộng rãi trong khoa học thần kinh [44]. Đồ thị tri thức KU triển khai một mô phỏng tính toán tương đương: khi hai KU thường xuyên được đồng truy cập, đồng trích dẫn hoặc đồng xác thực trong một cửa sổ thời gian, trọng số cạnh giữa chúng sẽ tăng lên theo quy tắc cập nhật lấy cảm hứng từ Hebbian. Điều này tạo ra một cấu trúc ngữ nghĩa mới nổi, nơi các đơn vị tri thức liên kết mạnh mẽ sẽ phân cụm một cách tự nhiên, cho phép truy xuất liên tưởng mà không cần tổ chức phân loại rõ ràng.

**Hệ miễn dịch nhân tạo (Artificial Immune Systems - AIS).** De Castro và Timmis [45] đã hình thức hóa các hệ miễn dịch nhân tạo như các khung tính toán lấy cảm hứng từ khả năng miễn dịch thích ứng của động vật có xương sống, kết hợp các cơ chế như lựa chọn dòng (clonal selection), lựa chọn âm tính (negative selection) và lý thuyết mạng miễn dịch. Forrest và cộng sự [46] trước đó đã chứng minh việc ứng dụng các thuật toán lựa chọn âm tính để phát hiện bất thường, trong đó các bộ phát hiện tự tham chiếu sẽ nhận diện các mẫu không thuộc về bản thân (non-self). Kiến trúc KU sử dụng các cơ chế lấy cảm hứng từ AIS cho tính toàn vẹn của tri thức: các KU đi vào trải qua một xác thực "lựa chọn âm tính" đối chiếu với các mẫu đã biết là xấu (mâu thuẫn với tri thức xác thực có độ tự tin cao, nguồn gốc từ các nguồn bị đưa vào danh sách đen, biến dạng cấu trúc), và hệ thống duy trì một "bộ nhớ" tiến hóa của các mẫu tri thức đã được xác thực giúp tăng tốc xác thực trong tương lai. Cách tiếp cận lấy cảm hứng từ miễn dịch này cung cấp khả năng phòng thủ phân tán, thích ứng chống lại thông tin sai lệch mà không yêu cầu kiểm duyệt nội dung tập trung.

**Lý thuyết ổ sinh thái (Ecological Niche Theory).** Hutchinson [47] đã hình thức hóa ổ sinh thái như một siêu thể tích n-chiều trong không gian môi trường nơi một loài có thể tồn tại. Ổ sinh thái cơ bản (được định nghĩa bởi khả năng chịu đựng sinh lý) và ổ sinh thái thực tế (bị hạn chế bởi sự cạnh tranh và ăn thịt) cùng nhau quyết định sự phân bố và cùng tồn tại của loài. Kiến trúc KU áp dụng lý thuyết ổ sinh thái vào việc tổ chức tri thức: mỗi KU chiếm giữ một vị trí trong một "không gian tri thức" đa chiều được định nghĩa bởi các mã lĩnh vực, phạm vi thời gian, mức độ tự tin và lịch sử tương tác của nó. Các KU chiếm giữ các ổ sinh thái tương tự nhau sẽ cạnh tranh giành sự chú ý và các tài nguyên sao chép, với các KU chất lượng cao hơn (lòng tin cao hơn, gần đây hơn, nguồn gốc tốt hơn) sẽ thay thế các giải pháp thay thế chất lượng thấp hơn thông qua loại trừ cạnh tranh — mô phỏng nguyên lý loại trừ cạnh tranh của Gause [48] trong sinh thái học.

**Tổng hợp (Synthesis).** Theo hiểu biết tốt nhất của chúng tôi, chưa có hệ thống thông tin hiện tại nào kết hợp cả năm cơ chế lấy cảm hứng từ sinh học này trong một khung quản lý tri thức thống nhất. Các cơ chế riêng lẻ đã được áp dụng riêng rẽ — stigmergy trong lọc cộng tác [49], trí tuệ bầy đàn trong tìm kiếm phân tán [50], học Hebbian trong hệ thống gợi ý [51], AIS trong bảo mật mạng [52], và lý thuyết ổ sinh thái trong phân bổ tài nguyên [53] — nhưng sự tích hợp hiệp đồng của chúng vẫn chưa được khám phá. Kiến trúc KU đại diện cho nỗ lực đầu tiên để cấu thành các cơ chế này thành một hệ thống nhất quán, tương trợ lẫn nhau, nơi các dấu vết năng lượng stigmergic thúc đẩy sự tăng cường liên kết Hebbian, các cơ chế miễn dịch duy trì tính toàn vẹn tri thức, định tuyến lấy cảm hứng từ bầy đàn khám phá các nguồn chất lượng cao, và cạnh tranh dựa trên ổ sinh thái đảm bảo sức khỏe của hệ sinh thái tri thức.

## 2.7 Logic nhận thức và lòng tin trong hệ thống phân tán

Các hệ thống tri thức hoạt động trong môi trường phi tập trung, đối kháng yêu cầu các khung hình thức để suy luận về niềm tin, bằng chứng và lòng tin. Chúng tôi khảo sát các đóng góp chính định hình cho kiến trúc nhận thức của KU.

**Khung AGM (AGM Framework).** Alchourrón, Gärdenfors và Makinson [54] đã thiết lập lý thuyết nền tảng về hiệu chỉnh niềm tin (belief revision), định nghĩa ba tác vụ trên các tập hợp niềm tin: *mở rộng (expansion)* (thêm một niềm tin mới), *co hẹp (contraction)* (loại bỏ một niềm tin) và *hiệu chỉnh (revision)* (thêm một niềm tin có khả năng mâu thuẫn trong khi vẫn duy trì tính nhất quán). Các tiên đề AGM (khép kín, thành công, bao hàm, trống rỗng, nhất quán, mở rộng và các tiên đề bổ sung cho co hẹp) cung cấp các ràng buộc hợp lý về sự thay đổi niềm tin. Việc quản lý trạng thái nhận thức của KU triển khai một mô phỏng tính toán tương đương của hiệu chỉnh AGM: khi một KU mới mâu thuẫn với một KU có độ tự tin cao hiện có, hệ thống sẽ áp dụng một toán tử hiệu chỉnh được đánh trọng số lòng tin có tính đến quyền hạn nhận thức tương đối của các nguồn, sức mạnh của bằng chứng hỗ trợ và tính cập nhật thời gian của các khẳng định mâu thuẫn.

**EigenTrust.** Kamvar, Schlosser và Garcia-Molina [55] đã đề xuất EigenTrust để tính toán các giá trị danh tiếng toàn cầu trong các mạng ngang hàng thông qua việc tổng hợp lòng tin lặp đi lặp lại, tương tự như tính toán quyền hạn PageRank cho các trang web. Mỗi peer đánh giá các đối tác giao dịch của mình, và điểm lòng tin toàn cầu nổi lên như là eigenvector chính của ma trận lòng tin chuẩn hóa. Mặc dù EigenTrust cung cấp các thuộc tính toán học thanh lịch (đảm bảo hội tụ, kháng Sybil thông qua các peer được tin cậy trước), nó giả định một chiều lòng tin duy nhất, đồng nhất. KU mở rộng phương pháp tổng hợp lặp của EigenTrust sang lòng tin đa chiều, nơi quyền hạn của nguồn thay đổi theo lĩnh vực, độ mới và loại bằng chứng — một nhà hóa học có thể được tin cậy cao cho các khẳng định về thuộc tính phân tử nhưng không được tin cậy cho các tuyên bố lịch sử.

**Hiệu chỉnh niềm tin nhạy cảm lòng tin (Trust-Sensitive Belief Revision).** Booth và Hunter [56] đã tích hợp các cân nhắc về lòng tin vào khung hiệu chỉnh niềm tin AGM, định nghĩa các toán tử hiệu chỉnh nhạy cảm lòng tin, trong đó mức độ bám rễ của một niềm tin được điều biến bởi độ tin cậy của nguồn cung cấp nó. Hình thức hóa của họ chứng minh rằng hiệu chỉnh AGM cổ điển có thể được xem như một trường hợp đặc biệt của hiệu chỉnh nhạy cảm lòng tin khi mọi nguồn đều được tin cậy như nhau. KU vận hành khung lý thuyết này thông qua các điểm lòng tin được hỗ trợ bởi CRDT: mỗi KU mang một bộ tích lũy lòng tin dựa trên PNCounter (§2.4) trực tiếp điều biến mức độ ưu tiên hiệu chỉnh khi xảy ra xung đột, cung cấp một triển khai phân tán, thực tế cho sự thay đổi niềm tin nhạy cảm lòng tin.

**Phân cấp bằng chứng (Evidence Hierarchies).** Kim tự tháp bằng chứng y học, được hình thức hóa thông qua phương pháp luận đánh giá hệ thống của Cochrane Collaboration [57] và khung GRADE (Grading of Recommendations, Assessment, Development, and Evaluation) [58], thiết lập một hệ phân cấp về sức mạnh của bằng chứng từ ý kiến chuyên gia (thấp nhất) qua báo cáo ca bệnh, nghiên cứu thuần tập, thử nghiệm lâm sàng ngẫu nhiên có đối chứng (RCTs), đến các đánh giá hệ thống và phân tích gộp (cao nhất). Mặc dù có nguồn gốc đặc thù trong ngành, phương pháp phân cấp để phân loại bằng chứng này cung cấp một bản mẫu có thể tổng quát hóa cho việc đánh giá chất lượng tri thức. Trường `evidence_level` của KU triển khai một phân cấp bằng chứng tổng quát có thể áp dụng trên nhiều lĩnh vực, nơi mỗi cấp độ tương ứng với một phương pháp xác minh riêng biệt: khẳng định chưa kiểm chứng, quan sát đồng đẳng, xác minh thuật toán, đồng thuận đa nguồn và chứng minh hình thức.

**Tổng hợp (Synthesis).** Kiến trúc nhận thức của KU đại diện cho hệ thống đầu tiên, theo hiểu biết của chúng tôi, kết hợp ngữ nghĩa hiệu chỉnh niềm tin hình thức (tương thích AGM) với triển khai phân tán thực tế được hỗ trợ bởi CRDT và truyền bá lòng tin đa chiều. Các hệ thống hiện tại hoặc triển khai logic nhận thức hình thức (không có hiện thực hóa phân tán) hoặc lòng tin phân tán (không có cơ sở nhận thức hình thức), chứ không phải cả hai. KU thu hẹp khoảng cách này bằng cách nhúng các toán tử hiệu chỉnh tương thích AGM vào bên trong các hàm hợp nhất CRDT, đảm bảo rằng việc hiệu chỉnh niềm tin vừa hợp lý về mặt hình thức vừa hội tụ thực tế trên các phân vùng mạng.

## 2.8 Tóm tắt và Định vị

Bảng 4 trình bày một so sánh toàn diện về kiến trúc KU với các hệ thống chính được khảo sát trong phần này, được tổ chức theo các khả năng chính cần thiết cho một hệ thống quản lý tri thức phi tập trung, lấy cảm hứng từ sinh học.

**Bảng 4.** So sánh tính năng toàn diện giữa các hệ thống được khảo sát và KU.

| Tính năng (Feature) | RDF/OWL [1][4] | IPFS [21] | Wikidata [14] | OriginTrail [28] | OneBrain KU |
|---|---|---|---|---|---|
| **Các kiểu tri thức cấu trúc** | Triples/axioms | Byte thô | Mục + thuộc tính | Bộ ba RDF | KU có kiểu với qualifiers |
| **Siêu dữ liệu nhận thức** | Không có / reification | Không có | Tài liệu tham khảo (hạn chế) | Không có | Hạng nhất (độ tự tin, cấp bằng chứng) |
| **Lan truyền lòng tin** | Không có | Không có | Chỉ lịch sử chỉnh sửa | Neo giữ vào Blockchain | Lòng tin CRDT đa chiều |
| **Lưu trữ phi tập trung** | Một phần (LOD) | Đầy đủ (định địa chỉ nội dung) | Tập trung | Blockchain + các nút | P2P với định địa chỉ nội dung |
| **Giải quyết xung đột** | Không có / thủ công | Không có (bất biến) | Chiến tranh chỉnh sửa | Tính chung thẩm của Blockchain | SEC dựa trên CRDT |
| **Cơ chế khuyến khích** | Không có | Filecoin (riêng biệt) | Tình nguyện viên | Token TRAC | Nền kinh tế token Synaptic |
| **Cơ chế lấy cảm hứng sinh học** | Không có | Không có | Không có | Không có | 5 cơ chế tích hợp |
| **Mã hóa nhị phân** | Turtle/RDF-XML | Protobuf (libp2p) | JSON/RDF | JSON-LD / RDF | CBOR (RFC 8949) |
| **Định địa chỉ nội dung** | Không gian tên URI | Multihash CID | Wikidata Q-IDs | Assertion CID | Multihash CID |
| **Hội tụ CRDT** | Không có | Không có | Không có | Không có | GCounter, PNCounter, LWW, ORSet |
| **Hiệu chỉnh niềm tin** | Không có | Không có | Không có | Không có | Các toán tử tương thích AGM |
| **Tiến hóa schema** | Phiên bản hóa bản thể luận | N/A | Đề xuất thuộc tính | Căn chỉnh Schema.org | Tiến hóa qualifier phi tập trung |
| **Động lực học thời gian** | Đồ thị có tên (mở rộng) | Ảnh chụp bất biến | Dấu thời gian chỉnh sửa | Dấu thời gian khối | Suy giảm năng lượng + chuyển hóa |

Như Bảng 4 tiết lộ, chưa có hệ thống hiện tại nào giải quyết đồng thời quá ba trong số mười hai khía cạnh tính năng được xác định. RDF/OWL cung cấp khả năng biểu đạt nhưng thiếu tính phi tập trung, lòng tin và các động lực học lấy cảm hứng từ sinh học. IPFS cung cấp lưu trữ phi tập trung nhưng không cung cấp lớp ngữ nghĩa. Wikidata đạt quy mô ấn tượng nhưng vẫn tập trung và thiếu giải quyết xung đột hình thức. OriginTrail kết hợp phi tập trung hóa với các cơ chế khuyến khích nhưng thừa hưởng các hạn chế của RDF và đưa vào các rào cản chi phí trên chuỗi.

Kiến trúc KU được định vị một cách độc đáo ở giao lộ của các khả năng này, cung cấp: (i) một hình thức biểu diễn phong phú hơn các bộ ba RDF nhưng nhẹ hơn các bản thể luận OWL; (ii) lưu trữ phi tập trung, định địa chỉ theo nội dung với giải quyết xung đột dựa trên CRDT; (iii) siêu dữ liệu nhận thức hình thức với hiệu chỉnh niềm tin nhạy cảm lòng tin; (iv) một cơ chế khuyến khích tích hợp phù hợp với chất lượng tri thức; (v) năm cơ chế lấy cảm hứng từ sinh học hiệp đồng để tự tổ chức; và (vi) mã hóa nhị phân hiệu quả thông qua một định dạng tiêu chuẩn IETF. Sự kết hợp này cấu thành một đóng góp mới cho tài liệu quản lý tri thức, giải quyết một khoảng trống mà chưa có một hệ thống đơn lẻ nào trước đây lấp đầy được.

---

## Tài liệu tham khảo

[1] W3C, "RDF 1.1 Concepts and Abstract Syntax," W3C Recommendation, Feb. 2014.

[2] M. Schmachtenberg, C. Bizer, and H. Paulheim, "Adoption of the Linked Data Best Practices in Different Topical Domains," in *Proc. ISWC*, 2014, pp. 245–260.

[3] O. Hartig, "Foundations of RDF* and SPARQL*," in *Proc. AMW*, 2017.

[4] W3C, "OWL 2 Web Ontology Language Document Overview," W3C Recommendation, Dec. 2012.

[5] B. Motik, B. C. Grau, I. Horrocks, Z. Wu, A. Fokoue, and C. Lutz, "OWL 2 Web Ontology Language Profiles," W3C Recommendation, 2012.

[6] M. C. Suárez-Figueroa, A. Gómez-Pérez, and M. Fernández-López, "The NeOn Methodology for Ontology Engineering," in *Ontology Engineering in a Networked World*, Springer, 2012, pp. 9–34.

[7] F. Baader, D. Calvanese, D. McGuinness, D. Nardi, and P. Patel-Schneider, *The Description Logic Handbook*, 2nd ed. Cambridge Univ. Press, 2007.

[8] M. Minsky, "A Framework for Representing Knowledge," MIT AI Lab Memo 306, 1974.

[9] M. R. Quillian, "Semantic Memory," in *Semantic Information Processing*, M. Minsky, Ed. MIT Press, 1968, pp. 227–270.

[10] A. M. Collins and E. F. Loftus, "A Spreading-Activation Theory of Semantic Processing," *Psychol. Rev.*, vol. 82, no. 6, pp. 407–428, 1975.

[11] J. F. Sowa, *Conceptual Structures: Information Processing in Mind and Machine*. Addison-Wesley, 1984.

[12] A. Singhal, "Introducing the Knowledge Graph: Things, Not Strings," Google Official Blog, May 2012.

[13] N. Noy, Y. Gao, A. Jain, A. Naber, A. Patterson, and J. Taylor, "Industry-Scale Knowledge Graphs: Lessons and Challenges," *Commun. ACM*, vol. 62, no. 8, pp. 36–43, 2019.

[14] D. Vrandečić and M. Krötzsch, "Wikidata: A Free Collaborative Knowledgebase," *Commun. ACM*, vol. 57, no. 10, pp. 78–85, 2014.

[15] Wikidata, "Wikidata Statistics," https://www.wikidata.org/wiki/Special:Statistics, Accessed 2024.

[16] J. Lehmann et al., "DBpedia—A Large-Scale, Multilingual Knowledge Base Extracted from Wikipedia," *Semantic Web J.*, vol. 6, no. 2, pp. 167–195, 2015.

[17] P. N. Mendes, M. Jakob, and C. Bizer, "DBpedia: A Multilingual Cross-Domain Knowledge Base," in *Proc. LREC*, 2012.

[18] F. M. Suchanek, G. Kasneci, and G. Weikum, "YAGO: A Core of Semantic Knowledge," in *Proc. WWW*, 2007, pp. 697–706.

[19] T. P. Tanon, G. Weikum, and F. M. Suchanek, "YAGO 4: A Reason-able Knowledge Base," in *Proc. ESWC*, 2020, pp. 583–596.

[20] S. Ji, S. Pan, E. Cambria, P. Marttinen, and P. S. Yu, "A Survey on Knowledge Graphs: Representation, Acquisition, and Applications," *IEEE Trans. Neural Netw. Learn. Syst.*, vol. 33, no. 2, pp. 494–514, 2022.

[21] J. Benet, "IPFS—Content Addressed, Versioned, P2P File System," arXiv:1407.3561, 2014.

[22] Protocol Labs, "IPFS Ecosystem Report," 2023.

[23] A. V. Sambra et al., "Solid: A Platform for Decentralized Social Applications Based on Linked Data," MIT CSAIL Tech. Rep., 2016.

[24] R. Verborgh, "Re-decentralizing the Web, for Good This Time," in *Linking the World's Information: Essays on Tim Berners-Lee's Invention of the World Wide Web*, ACM, 2023.

[25] Halo Labs, "OrbitDB: Peer-to-Peer Databases for the Decentralized Web," https://orbitdb.org, 2015.

[26] M. Kleppmann and A. R. Beresford, "A Conflict-Free Replicated JSON Datatype," *IEEE Trans. Parallel Distrib. Syst.*, vol. 28, no. 10, pp. 2733–2746, 2017.

[27] W3C, "Decentralized Identifiers (DIDs) v1.0," W3C Recommendation, Jul. 2022.

[28] OriginTrail, "OriginTrail: Decentralized Knowledge Graph," White Paper, 2022.

[29] M. Shapiro, N. Preguiça, C. Baquero, and M. Zawirski, "Conflict-Free Replicated Data Types," INRIA Tech. Rep. RR-7687, 2011.

[30] N. Preguiça, C. Baquero, and M. Shapiro, "Conflict-Free Replicated Data Types (CRDTs)," in *Encyclopedia of Big Data Technologies*, Springer, 2018.

[31] H. Sanjuán, S. Poyhtari, P. Teixeira, and I. Psaras, "Merkle-CRDTs: Merkle-DAGs Meet CRDTs," arXiv:2004.00107, 2020.

[32] C. Bormann and P. Hoffman, "Concise Binary Object Representation (CBOR)," IETF RFC 8949, Dec. 2020.

[33] Google, "Protocol Buffers: Language Guide," https://protobuf.dev, 2008.

[34] S. Furuhashi, "MessagePack: It's Like JSON but Fast and Small," https://msgpack.org, 2008.

[35] S. Furuhashi, "MessagePack Specification," https://github.com/msgpack/msgpack/blob/master/spec.md, 2013.

[36] K. Varda, "Cap'n Proto: Introduction," https://capnproto.org, 2013.

[37] Google, "FlatBuffers: An Efficient Cross-Platform Serialization Library," https://flatbuffers.dev, 2014.

[38] P. Viotti and M. Kinderkhedia, "A Study of Serialization Formats for Data-Intensive Applications," in *Proc. EDBT/ICDT Workshops*, 2022.

[39] P.-P. Grassé, "La Reconstruction du Nid et les Coordinations Interindividuelles chez *Bellicositermes natalensis* et *Cubitermes* sp.," *Insectes Sociaux*, vol. 6, no. 1, pp. 41–80, 1959.

[40] F. Heylighen, "Stigmergy as a Universal Coordination Mechanism I: Definition and Components," *Cogn. Syst. Res.*, vol. 38, pp. 4–13, 2016.

[41] E. Bonabeau, M. Dorigo, and G. Theraulaz, *Swarm Intelligence: From Natural to Artificial Systems*. Oxford Univ. Press, 1999.

[42] M. Dorigo and T. Stützle, *Ant Colony Optimization*. MIT Press, 2004.

[43] D. O. Hebb, *The Organization of Behavior: A Neuropsychological Theory*. Wiley, 1949.

[44] S. Löwel and W. Singer, "Selection of Intrinsic Horizontal Connections in the Visual Cortex by Correlated Neuronal Activity," *Science*, vol. 255, no. 5041, pp. 209–212, 1992.

[45] L. N. de Castro and J. Timmis, *Artificial Immune Systems: A New Computational Intelligence Approach*. Springer, 2002.

[46] S. Forrest, A. S. Perelson, L. Allen, and R. Cherukuri, "Self-Nonself Discrimination in a Computer," in *Proc. IEEE S&P*, 1994, pp. 202–212.

[47] G. E. Hutchinson, "Concluding Remarks," *Cold Spring Harbor Symp. Quant. Biol.*, vol. 22, pp. 415–427, 1957.

[48] G. F. Gause, *The Struggle for Existence*. Williams & Wilkins, 1934.

[49] G. Di Marzo Serugendo, M.-P. Gleizes, and A. Karageorgos, "Self-Organization in Multi-Agent Systems," *Knowl. Eng. Rev.*, vol. 20, no. 2, pp. 165–189, 2005.

[50] A. Abraham, C. Grosan, and V. Ramos, Eds., *Swarm Intelligence in Data Mining*. Springer, 2006.

[51] X. He, L. Liao, H. Zhang, L. Nie, X. Hu, and T.-S. Chua, "Neural Collaborative Filtering," in *Proc. WWW*, 2017, pp. 173–182.

[52] U. Aickelin and S. Cayzer, "The Danger Theory and Its Application to Artificial Immune Systems," in *Proc. ICARIS*, 2002, pp. 141–148.

[53] M. Luck, P. McBurney, O. Shehory, and S. Willmott, *Agent Technology: Computing as Interaction*. AgentLink III, 2005.

[54] C. E. Alchourrón, P. Gärdenfors, and D. Makinson, "On the Logic of Theory Change: Partial Meet Contraction and Revision Functions," *J. Symb. Log.*, vol. 50, no. 2, pp. 510–530, 1985.

[55] S. D. Kamvar, M. T. Schlosser, and H. Garcia-Molina, "The EigenTrust Algorithm for Reputation Management in P2P Networks," in *Proc. WWW*, 2003, pp. 640–651.

[56] R. Booth and A. Hunter, "Trust as a Precondition for Belief Revision: A Synthesis," *J. Artif. Intell. Res.*, vol. 61, pp. 699–748, 2018.

[57] J. P. T. Higgins and S. Green, Eds., *Cochrane Handbook for Systematic Reviews of Interventions*, Version 5.1.0. The Cochrane Collaboration, 2011.

[58] G. H. Guyatt et al., "GRADE: An Emerging Consensus on Rating Quality of Evidence and Strength of Recommendations," *BMJ*, vol. 336, pp. 924–926, 2008.
