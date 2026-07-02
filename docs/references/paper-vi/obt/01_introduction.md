# 1. Introduction (Giới thiệu)

## 1.1 The Knowledge Incentive Problem (Vấn đề Khuyến khích Tri thức)

Việc tạo lập, xác thực (validation) và bảo tồn tri thức có cấu trúc đại diện cho những thách thức cơ bản trong các hệ thống phân tán. Mặc dù internet đã dân chủ hóa việc tiếp cận thông tin, nó đồng thời tạo ra các cấu trúc khuyến khích sai lệch: những người tạo nội dung được thưởng dựa trên lượt tương tác thay vì độ chính xác, các nhà cung cấp lưu trữ (storage providers) tối ưu hóa cho dung lượng thay vì giá trị, và việc xác thực (verification) — nhiệm vụ đòi hỏi trí tuệ cao nhất — lại bị đền bù thiếu hụt một cách có hệ thống.

Các hệ thống khuyến khích dựa trên token hiện tại, chủ yếu bắt nguồn từ các thiết kế cryptocurrency, không giải quyết được các yêu cầu đặc thù của một nền kinh tế tri thức vì ba lý do cơ bản sau:

1. **Scarcity-based valuation không phù hợp với tri thức.** Bitcoin và các token tương tự lấy giá trị từ giới hạn nguồn cung nhân tạo. Ngược lại, tri thức không có tính cạnh tranh tiêu dùng (non-rivalrous) và gia tăng giá trị thông qua việc nhân bản (*replication*), chứ không phải sự hạn chế. Một mô hình token dựa trên sự khan hiếm (scarcity) sẽ cản trở việc chia sẻ tri thức một cách tự nhiên.

2. **Phí giao dịch (transaction fees) tạo ra rào cản truy cập tri thức.** Mô hình gas của Ethereum, mặc dù hiệu quả cho các ứng dụng tài chính, áp đặt chi phí trên mỗi hoạt động khiến các tương tác tri thức chi tiết (fine-grained) — như truy xuất một thực tế đơn lẻ, xác thực một tuyên bố, hoặc cập nhật điểm số trust score — trở nên bất khả thi về mặt kinh tế ở quy mô lớn.

3. **Proof-of-Work và Proof-of-Stake không liên quan đến chất lượng tri thức.** Việc đào Bitcoin hoặc staking Ethereum chứng minh chi phí tính toán hoặc cam kết vốn, cả hai đều không liên quan đến chất lượng, tính mới hoặc tính hữu dụng (utility) của tri thức được tạo ra.

OneBrain Protocol (OBP) giải quyết những thách thức này thông qua một kiến trúc quản lý tri thức được thiết kế chuyên biệt gồm bốn trụ cột: Knowledge Units (KU) để biểu diễn có cấu trúc, Knowledge Query Language (KQL) để truy xuất ngữ nghĩa (semantic retrieval), Proof of Meaningful Verification (PoMV) để đảm bảo chất lượng, và OneBrain Token (OBT) để điều chỉnh các khuyến khích kinh tế.

Bài báo này tập trung vào OBT — trụ cột thứ tư — vốn phải giải quyết một vấn đề khác biệt căn bản so với các hệ thống token thông thường: *Làm thế nào để tạo ra một token khuyến khích việc sản xuất tri thức chất lượng cao, được xác thực tốt và được lưu trữ bền vững, mà không đưa vào các khuyến khích trục lợi (gaming incentives) vốn đang gây khó khăn cho các hệ thống hiện tại?*

## 1.2 Why Existing Token Models Fail for Knowledge (Tại sao các mô hình Token hiện tại thất bại đối với Tri thức)

Để hiểu rõ không gian thiết kế (design space), chúng tôi phân tích bảy hệ thống token nổi bật qua mười bốn khía cạnh liên quan đến kinh tế học tri thức:

| Mục đích chính (Primary purpose) | Bitcoin | Ethereum | Filecoin | Arweave | Nano | Helium | **OBT** |
|-----------|---------|----------|----------|---------|------|--------|---------|
| **Primary purpose** | Lưu trữ giá trị | Hợp đồng thông minh | Lưu trữ | Lưu trữ vĩnh viễn | Thanh toán | Phủ sóng IoT | Knowledge utility |
| **Supply model** | Hard cap (21M) | Lạm phát (burn) | Hard cap (2B) | Hard cap (66M) | Cố định (133M) | Chia đôi (Halving) | **Near-infinite, flow-controlled** |
| **Consensus** | PoW | PoS | PoRep+PoSt | SPoRA | ORV | PoC | **PoMV** |
| **Fees** | Động | Gas | Gas | AR/byte | **Zero** | DC burn | **Zero** |
| **Ledger** | Chuỗi UTXO | Tài khoản/trạng thái | Chuỗi Tipset | Weave | **Block-lattice** | Blockchain | **Account-Chain** |
| **Finality** | ~60 phút | ~12 phút | ~30 giây | ~2 phút | <1 giây | ~60 phút | **<1 giây (L1), ~30 giây (L3)** |
| **Content awareness** | Không | Phụ thuộc hợp đồng | Phân khu mờ đục (Opaque sectors) | Phân đoạn mờ đục (Opaque chunks) | Không | Bản đồ phủ sóng | **Semantic (FieldExtract)** |
| **Anti-spam** | Phí | Gas | Gas + tài sản thế chấp | Phí | Khay số dư (Balance buckets) | Stake | **Trust proxy** |
| **Identity** | Ẩn danh (Pseudonymous) | Ẩn danh (Pseudonymous) | Miner ID | Ẩn danh (Pseudonymous) | Tài khoản (Account) | Điểm phát sóng (Hotspot) | **EigenTrust reputation** |
| **Storage proofs** | N/A | N/A | WindowPoSt (GPU) | SPoRA (SSD) | N/A | N/A | **PoS-KU (CPU)** |
| **Penalty model** | Không | Slashing | Lỗi phân khu (Sector fault) | Không | Không | Denylist | **5-tier graduated** |
| **Appeals** | Không | Không | Không | Không | Không | Bỏ phiếu | **4-layer process** |
| **Knowledge quality** | N/A | N/A | N/A | N/A | N/A | N/A | **PoMV 6-signal** |
| **Token/Trust split** | N/A | Staking | Tài sản thế chấp | N/A | N/A | Staking | **Separate domains** |

**Bảng 1.** So sánh OBT với các hệ thống token hiện tại qua 14 khía cạnh.

Một số nhận xét rút ra từ sự so sánh này:

**Không có hệ thống hiện tại nào đo lường chất lượng tri thức.** Filecoin và Arweave xác minh rằng dữ liệu *tồn tại* nhưng không thể đánh giá liệu nội dung được lưu trữ có *giá trị*, *chính xác* hay *được cấu trúc tốt* hay không. Sự tích hợp của OBT với PoMV cho phép tính toán phần thưởng nhận biết nội dung (content-aware reward calculation).

**Chống spam dựa trên phí không tương thích với các hoạt động tri thức.** Một lần truy xuất tri thức đơn lẻ có thể liên quan đến hàng tá cập nhật trust, tính toán tín hiệu PoMV, và các thông điệp gossip. Với mức phí gas điển hình của Ethereum, các hoạt động chi tiết như vậy sẽ bị cấm đoán về mặt kinh tế. OBT sử dụng *trust* làm đại diện tài nguyên (resource proxy) — danh tiếng đã được thiết lập sẽ thay thế các khoản tiền gửi tài chính.

**Kiến trúc Block-lattice và Account-Chain phù hợp một cách độc đáo.** Nano đã chứng minh rằng các chuỗi trên mỗi tài khoản (per-account chains) có thể đạt được các giao dịch miễn phí và hoàn thành dưới một giây. OBT điều chỉnh kiến trúc này cho các ngữ cảnh tri thức, bổ sung vector clocks để sắp xếp thứ tự nhân quả (causal ordering) và chữ ký nhân chứng ngưỡng (threshold witness signatures) để bảo mật.

## 1.3 Design Principles and Axioms (Nguyên tắc Thiết kế và Tiên đề)

OBT được xây dựng trên bốn tiên đề nền tảng giúp phân biệt nó với cả cryptocurrency và các hệ thống phần thưởng truyền thống:

> **Tiên đề A1 (Tính vĩnh viễn của Token kiếm được):** OBT một khi đã kiếm được là vĩnh viễn. Hệ thống theo dõi `total_earned` bằng cách sử dụng một G-Counter tăng đơn điệu — một conflict-free replicated data type, theo thiết kế, chỉ có thể tăng lên. Điều này có nghĩa là không một cơ quan nào — không phải giao thức, không phải một cuộc bỏ phiếu quản trị, không phải một hình phạt — có thể tịch thu hồi tố các token đã kiếm được.

> **Tiên đề A2 (Tính khả biến của Trust):** Uy tín trust reputation là một miền tách biệt với số dư token và phải chịu cả sự suy giảm tự nhiên lẫn sự giảm trừ trừng phạt. Trong khi OBT là vĩnh viễn, *khả năng kiếm thêm OBT* lại bị kiểm soát bởi trust. Điều này tạo ra tính bất đối xứng mong muốn: những người tham gia trung thực tích lũy cả token và trust; các tác nhân gian lận giữ lại các khoản thu nhập trong quá khứ nhưng mất đi tiềm năng kiếm tiền trong tương lai.

> **Tiên đề A3 (Tri thức là miễn phí):** OBT không bao giờ tạo ra một bức tường phí (paywall) cho việc tiếp cận tri thức. Các Knowledge Units (KU) có thể được truy xuất tự do; OBT khuyến khích việc *tạo lập, xác thực (verification) và lưu trữ (storage)* — chứ không phải việc tiếp cận. Đây là một cam kết mang tính triết học: tri thức là một hàng hóa công cộng.

> **Tiên đề A4 (Giá trị từ sự Hữu dụng):** Giá trị của OBT bắt nguồn duy nhất từ sự hữu dụng của tri thức (knowledge utility). Không có cơ chế đầu cơ, không có lợi suất staking, không có tính kết hợp DeFi. OBT đo lường "lượng công việc tri thức đã được xác thực đã thực hiện", tương tự như kilowatt-giờ đo lường sản lượng năng lượng.

Các tiên đề này tương tác với nhau để tạo ra một hệ thống với các thuộc tính cụ thể:

| Thuộc tính (Property) | Cơ chế (Mechanism) | Tiên đề (Axiom) |
|----------|-----------|-------|
| Gian lận không thể chiếm đoạt thu nhập quá khứ | Tính đơn điệu của G-Counter | A1 |
| Gian lận làm giảm thu nhập tương lai | Trust slashing, giới hạn tỷ lệ (rate limits) | A2 |
| Truy cập vẫn miễn phí | Không yêu cầu giao dịch để đọc | A3 |
| Không có động lực Ponzi/đầu cơ | Không staking, không yield farming | A4 |
| Khả năng chống Sybil | Quyền truy cập kiểm soát bởi trust, EigenTrust | A2 |
| Điều tiết nguồn cung tự nhiên | Phát hành liên kết với hoạt động thực tế | A4 |

**Bảng 2.** Tương tác giữa các tiên đề OBT và các thuộc tính hệ thống.

## 1.4 Three Owner Principles (Ba Nguyên tắc từ Người sáng lập)

Ngoài các tiên đề, thiết kế hệ thống được hướng dẫn bởi ba yêu cầu cấp cao do kiến trúc sư của giao thức thiết lập:

**Nguyên tắc N1: Tradeable + Secure + Fast + No Waste (Có thể giao dịch + Bảo mật + Nhanh chóng + Không lãng phí).**
OBT phải có thể chuyển nhượng giữa các thành viên tham gia, được bảo mật bằng mã hóa hiện đại (chữ ký Ed25519, hàm băm BLAKE3), đạt được thời gian hoàn thành (finality) dưới một giây cho các hoạt động thông thường, và không gây lãng phí tính toán (không có proof-of-work, no GPU requirements).

**Nguyên tắc N2: Bốn Dòng Phần thưởng (Four Reward Streams).**
Vòng đời tri thức liên quan đến bốn vai trò riêng biệt — sáng tạo tri thức (creation - R1), encoding thành dạng có cấu trúc (R2), xác thực tính chính xác (verification - R3), và lưu trữ bền vững (storage - R4). Mỗi vai trò yêu cầu một dòng phần thưởng chuyên dụng với logic tính toán phần thưởng độc lập.

**Nguyên tắc N3: Nguồn cung gần như vô hạn (Near-Infinite Supply).**
Khác với giới hạn 21 triệu của Bitcoin hay 2 tỷ của Filecoin, OBT không có giới hạn nguồn cung cứng (hard supply cap). Các token mới được đúc (minted) khi công việc thực tế được thực hiện, và tốc độ dòng chảy được kiểm soát bởi công thức phát hành (emission formula). Hình ảnh tương tự là một *dòng sông, chứ không phải một hồ nước* — không có giới hạn tổng lượng nước, nhưng tốc độ dòng chảy được kiểm soát bởi con đập.

## 1.5 Contributions (Các Đóng góp)

Bài báo này thực hiện các đóng góp sau:

1. **Account-Chain Ledger cho Knowledge Tokens (§4).** Chúng tôi điều chỉnh kiến trúc block-lattice của Nano cho kinh tế học tri thức, chứng minh một cách chính thức rằng các CRDT counter truyền thống (G-Counter, PN-Counter, Bounded Counter) không phù hợp cho việc theo dõi số dư, và trình bày cách Account-Chain giải quyết vấn đề overdraft trong khi vẫn bảo toàn sự lan truyền gossip conflict-free. Chúng tôi mở rộng thiết kế của Nano với vector clocks để sắp xếp thứ tự nhân quả, chữ ký nhân chứng ngưỡng (threshold witness signatures) và lệnh truy quét rẽ nhánh (fork warrants).

2. **Output-Based Minting với Kiểm soát Phát hành Toàn cầu (§5).** Chúng tôi trình bày một hệ thống minting nơi việc phát hành token là *đầu ra* (output) của sự đồng thuận tri thức, không bao giờ là đầu vào. Công thức phát hành $E = B \times A \times Q$ liên kết sự tăng trưởng nguồn cung với hoạt động mạng lưới và chất lượng tri thức, làm giảm lạm phát một cách tự nhiên từ 100% (Năm 1) xuống còn 13.5% (Năm 10) mà không cần các sự kiện halving nhân tạo.

3. **Phân bổ Phần thưởng Bốn Dòng (Four-Stream Reward Allocation) (§5).** Chúng tôi phân tách vòng đời tri thức thành bốn hoạt động đủ điều kiện nhận phần thưởng — owner rewards thông qua điểm số PoMV (40%), encoding rewards theo vai trò (25%), verification rewards (15%), và content-aware storage rewards (20%) — mỗi hoạt động đều có tính toán độc lập và được kiểm soát bởi trust.

4. **Content-Aware Storage Rewards (§6).** Chúng tôi đề xuất công thức phần thưởng lưu trữ 5 yếu tố tích hợp dung lượng nội dung, độ hiếm của nhân bản (replication rarity), nhu cầu ngữ nghĩa (PoMV metabolism), thời lượng lưu trữ, và trust của nhà cung cấp. Chúng tôi giới thiệu PoS-KU (Proof of Storage cho Knowledge Units), một giao thức thử thách với ba loại thử thách bao gồm *FieldExtract* — kiểm tra mức độ hiểu biết ngữ nghĩa của nội dung được lưu trữ, khác với các bằng chứng phân khu mờ đục (opaque sector proofs) của Filecoin.

5. **Trust-as-Resource-Proxy (§7).** Chúng tôi chứng minh rằng danh tiếng, được tính toán thông qua thuật toán EigenTrust và được ánh xạ vào một hệ phân cấp 7 tầng, có thể thay thế hiệu quả phí giao dịch để làm cơ chế chống spam (anti-spam). Các giới hạn tỷ lệ (rate limits), cổng chất lượng (quality gates), và mức trần phần thưởng (reward caps) đều được tham số hóa theo tầng trust thay vì các khoản tiền gửi tài chính.

6. **Nguyên tắc phân tách OBT/Trust (§8).** Chúng tôi giới thiệu sự phân biệt mang tính triết học giữa token kiếm được (vĩnh viễn, được theo dõi bởi G-Counter) và uy tín trust reputation (khả biến, chịu sự suy giảm và slashing). Chúng tôi chính thức hóa điều này dưới dạng nguyên tắc "tiền lương so với giấy phép hành nghề y": khoản đền bù trong quá khứ không bị thu hồi hồi tố, nhưng giấy phép hành nghề (và việc kiếm đền bù trong tương lai) có thể bị đình chỉ hoặc thu hồi.

7. **Correlation Penalty với Kháng nghị Bốn Lớp (§8).** Lấy cảm hứng từ hình phạt tương quan (correlation penalty) của Ethereum 2.0 đối với validator slashing, chúng tôi áp dụng công thức $m = 1 + \log_2(n)$ cho gian lận tri thức, trong đó $n$ là số lượng các nút bị phạt đồng thời. Điều này làm cho các cuộc tấn công phối hợp trở nên tốn kém hơn theo cấp số siêu tuyến tính (super-linearly). Chúng tôi bổ sung điều này bằng quy trình kháng nghị bốn lớp kết hợp tính năng tự động bảo vệ (auto-protection), các cửa sổ tranh chấp (dispute windows), đánh giá hồi tố, và kháng nghị Tombstone cuối cùng.

## 1.6 Paper Organization (Bố cục Bài báo)

Bố cục của bài báo này được tổ chức như sau:

- **§2 (Related Work)** khảo sát các hệ thống token hiện tại, các cơ chế khuyến khích lưu trữ, các sổ cái dựa trên DAG (DAG-based ledgers), và các nỗ lực xây dựng nền kinh tế tri thức, qua đó xác định những khoảng trống thúc đẩy thiết kế của OBT.

- **§3 (Token Design Philosophy)** trình bày bản sắc của OBT như một knowledge utility token, mô hình nguồn cung "Dòng sông, không phải Hồ nước" ("River, Not Lake"), hệ thống độ chính xác (milliOBT), và sáu quyết định thiết kế quan trọng (Q1–Q6).

- **§4 (Account-Chain Ledger)** chi tiết hóa kiến trúc ledger, phân tích chính thức lý do tại sao các CRDT thất bại trong việc theo dõi số dư, đặc tả cấu trúc TransferBlock, các quy tắc xác thực khối (block validation rules), phát hiện và giải quyết rẽ nhánh (fork detection and resolution), và lưu trữ ba lớp (three-layer storage).

- **§5 (Output-Based Minting)** định nghĩa công thức phát hành toàn cầu (global emission formula), bốn dòng phần thưởng với đặc tả toán học, mức trần phần thưởng cho mỗi nút với hệ số nhân trust, cấu trúc và xác thực MintProof, và phân tích lạm phát.

- **§6 (Content-Aware Storage Rewards)** đặc tả công thức phần thưởng lưu trữ 5 yếu tố, giao thức thử thách PoS-KU với ba loại thử thách, trục xuất dựa trên số lần vi phạm (strike-based eviction), và phân tích so sánh với Filecoin, Arweave và Sia.

- **§7 (Anti-Gaming and Quality Assurance)** mô tả trust-as-resource-proxy, giới hạn tỷ lệ phân tầng (tiered rate limiting), bốn cổng chất lượng tuần tự, và bốn bộ phát hiện mô hình trục lợi (gaming pattern detectors) với phân tích tín hiệu có trọng số.

- **§8 (Graduated Penalty System)** trình bày năm tầng hình phạt với các công thức trust, sự suy giảm trust tự nhiên, khuếch đại hình phạt tương quan, tám loại gian lận, và quy trình kháng nghị bốn lớp.

- **§9 (Evaluation)** cung cấp các chỉ số triển khai thực tế, kiến trúc module, độ bao phủ kiểm thử (test coverage), mô hình hóa mối đe dọa bảo mật bao gồm năm attack vector và ba kịch bản phân tách mạng, và các đặc tính hiệu năng.

- **§10 (Conclusion)** tóm tắt các đóng góp, thảo luận về các hạn chế, xác định các hướng nghiên cứu tương lai, và phản ánh những tác động rộng lớn hơn đối với thiết kế token tri thức.
