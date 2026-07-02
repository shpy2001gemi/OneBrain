# 2. Related Work

Phần này khảo sát các phương pháp hiện tại đối với việc xác thực tri thức (knowledge validation), tính toán độ tin cậy (trust computation), và các consensus mechanisms — chỉ ra các hạn chế cụ thể tạo động lực cho thiết kế dựa trên quan sát (observation-based design) của PoMV.

## 2.1 Knowledge Quality Assessment Systems

### 2.1.1 Academic Peer Review

Academic peer review [1] vẫn là tiêu chuẩn vàng để xác thực tri thức (knowledge validation). Các thế mạnh của nó — đánh giá của chuyên gia lĩnh vực, phê bình có cấu trúc, kiểm soát lối vào chống lại ngụy khoa học — đã phục vụ tốt cho khoa học. Tuy nhiên:

- **Reviewer burnout**: ~76% công việc bình duyệt là tình nguyện không lương [2]. 20% số người bình duyệt thực hiện 69% số lượt bình duyệt.
- **Publication bias**: Các tạp chí ưu tiên một cách hệ thống các kết quả tích cực [3]. Các kết quả tiêu cực — vốn có giá trị tương đương đối với tri thức — ít được xuất bản hơn.
- **Replication crisis**: 60% nghiên cứu tâm lý học [4] và 50% nghiên cứu ung thư tiền lâm sàng [5] thất bại khi sao chép.
- **Gatekeeping**: Các ý tưởng thách thức hệ hình (paradigm-challenging ideas) bị từ chối một cách hệ thống [6].
- **Speed**: Thời gian trung bình từ khi nộp đến khi xuất bản: 12–18 tháng.

**Prediction signal** của PoMV giải quyết vấn đề sao chép bằng cách đo lường liên tục xem các dự đoán được mã hóa tri thức (knowledge-encoded predictions) có được duy trì theo thời gian hay không. **Entropy signal** của PoMV trao thưởng cho tính mới (novelty) — các ý tưởng thách thức hệ hình sẽ nhận được phần thưởng entropy (entropy bonuses) cao.

### 2.1.2 Wikipedia's Consensus Model

Wikipedia [7] sử dụng mô hình đồng thuận cộng đồng: các biên tập viên thảo luận và đồng ý về nội dung bài viết. Thế mạnh của nó bao gồm sự tham gia mở và các yêu cầu về khả năng kiểm chứng. Hạn chế:

- **Edit wars**: Các bài viết nhạy cảm về chính trị (ví dụ: Israel-Palestine) gặp phải xung đột kéo dài [8].
- **First-mover advantage**: Các bài viết đã được thiết lập rất khó thay đổi bất kể có bằng chứng mới.
- **Systemic bias**: 87% biên tập viên Wikipedia tiếng Anh là nam giới [9]; phạm vi bao phủ bị thiên lệch về các chủ đề phương Tây, nói tiếng Anh.
- **No temporal decay**: Thông tin lỗi thời vẫn tồn tại trừ khi được chủ động cập nhật.

PoMV giải quyết first-mover advantage thông qua **Metabolism signal** — tri thức phải duy trì việc sử dụng liên tục để giữ lại giá trị. Tri thức lỗi thời sẽ tự động mất đi hoạt động metabolism khi người dùng chuyển sang các giải pháp thay thế tốt hơn.

### 2.1.3 Stack Overflow and Q&A Platforms

Stack Overflow [10] sử dụng reputation-weighted voting nơi danh tiếng của người dùng ảnh hưởng đến khả năng hiển thị của nội dung. Các anti-patterns được xác định:

| Anti-Pattern | Description | PoMV Solution |
|-------------|-------------|---------------|
| **Halo effect** | Câu trả lời của người dùng có danh tiếng cao được giả định là đúng | Metabolism được tính trên từng KU, không phải trên từng người dùng |
| **No temporal decay** | Các câu trả lời cũ kỹ từ 10 năm trước chiếm ưu thế | Suy giảm theo chu kỳ bán rã lũy thừa (Exponential half-life decay) |
| **Speed over quality** | Câu trả lời đầu tiên nhận được nhiều lượt bình chọn nhất | Entropy thưởng cho các đóng góp mới (novel contributions) bất kể thời điểm |
| **Global reputation** | Một điểm số duy nhất cho tất cả các lĩnh vực | Danh tiếng theo từng lĩnh vực của EigenTrust (EigenTrust per-domain reputation) |

## 2.2 Decentralized Trust and Reputation

### 2.2.1 EigenTrust

EigenTrust [11] tính toán các giá trị tin cậy toàn cục thông qua power iteration của một local trust matrix. Mỗi node $i$ gán một local trust $s_{ij}$ cho node $j$ dựa trên sự hài lòng về giao dịch. Global trust vector $\vec{t}$ hội tụ thông qua:

$$\vec{t}^{(k+1)} = C^T \cdot \vec{t}^{(k)}$$

trong đó $C$ is the normalized local trust matrix. Thế mạnh của EigenTrust — đảm bảo tính hội tụ, khả năng chống lại sự thao túng chiến lược — giúp nó phù hợp với danh tiếng cấp độ node (node-level reputation). PoMV áp dụng EigenTrust cho danh tiếng của node (§5.5) đồng thời mở rộng nó với per-domain trust và các điểm thưởng đa dạng (diversity bonuses).

### 2.2.2 SybilGuard and SybilRank

SybilGuard [12] và SybilRank [13] sử dụng cấu trúc đồ thị xã hội để phát hiện các Sybil nodes. Chi tiết: các mạng xã hội thực tế có các "attack edges" nhỏ kết nối các vùng Sybil với các vùng trung thực, cho phép phát hiện thông qua các bước đi ngẫu nhiên (random walks). **Spread Analysis** (§5.3) của PoMV áp dụng phân tích cấu trúc tương tự cho việc lan truyền tri thức — disinformation lan truyền qua các cấu trúc có thể phân biệt được.

### 2.2.3 Nostr Web of Trust

Nostr [14] triển khai một mạng lưới tin cậy phi tập trung (decentralized web of trust) nơi người dùng ký vào các danh sách "follow" và "mute". Độ tin cậy được tính toán cục bộ: mỗi người dùng nhìn nhận mạng lưới từ góc nhìn của chính họ. Mô hình này — không có cơ quan toàn cục, tính toán thuần túy cục bộ — đã trực tiếp truyền cảm hứng cho thiết kế của PoMV nơi mỗi node tính toán giá trị tri thức một cách độc lập.

## 2.3 Consensus Mechanisms

### 2.3.1 Proof-of-Work and Proof-of-Stake

Proof-of-Work (Bitcoin [15]) và Proof-of-Stake (Ethereum [16]) giải quyết double-spend problem cho các giao dịch tài chính. Tuy nhiên, chúng giải quyết một vấn đề cơ bản khác với việc định giá tri thức (knowledge valuation):
- PoW xác thực *nỗ lực tính toán (computational effort)*, không phải *chất lượng tri thức (knowledge quality)*.
- PoS xác thực *cam kết vốn (capital commitment)*, không phải *chất lượng tri thức (knowledge quality)*.
- Cả hai đều giả định tính hợp lệ nhị phân (giao dịch hợp lệ/không hợp lệ), chứ không phải giá trị đa chiều.

PoMV không sử dụng blockchain — việc xác thực tri thức không yêu cầu thứ tự toàn cục (global ordering) hoặc tính đồng thuận tuyệt đối (finality). Thay vào đó, nó sử dụng CRDTs [17] cho tính nhất quán cuối cùng (eventual consistency), điều này là đủ cho việc tính toán giá trị tri thức.

### 2.3.2 Proof-of-Useful-Work

Các đề xuất gần đây về Proof-of-Useful-Work [18] chuyển hướng nỗ lực tính toán sang các tác vụ hữu ích (protein folding, nhân ma trận). Mặc dù gần gũi hơn với việc định giá tri thức, các hệ thống này vẫn đo lường *nỗ lực tính toán (computational effort)* thay vì *giá trị tri thức (knowledge value)*.

### 2.3.3 Prediction Markets

Prediction markets [19, 20] tổng hợp thông tin thông qua các khuyến khích tài chính. Người tham gia đặt cược vào các kết quả; giá cả thị trường phản ánh ước lượng xác suất tập thể. Thế mạnh: độ chính xác đã được chứng minh đối với các sự kiện nhị phân (bầu cử, thể thao). Hạn chế:

- Yêu cầu **explicit bet placement** — hầu hết tri thức không có các thị trường cá cược rõ ràng.
- Yêu cầu **thanh khoản thị trường (market liquidity)** — các chủ đề ngách có thị trường rất mỏng.
- Yêu cầu **tiêu chí giải quyết (resolution criteria)** — câu hỏi "Hoàng hôn này có đẹp không?" không có giải pháp khách quan.
- Dễ bị **thao túng thị trường (market manipulation)** bởi các tác nhân có nguồn vốn lớn.

**Prediction signal** của PoMV nắm bắt lợi ích chính xác của prediction markets mà không yêu cầu đặt cược rõ ràng — mỗi Fact KU ngầm dự đoán rằng "điều này sẽ vẫn đúng vào ngày mai," và mỗi Procedure KU ngầm dự đoán rằng "thực hiện theo các bước này sẽ thành công."

## 2.4 Content Moderation and Anti-Disinformation

### 2.4.1 Centralized Moderation

Việc kiểm duyệt nền tảng (Facebook, YouTube, Twitter) sử dụng sự kết hợp giữa phát hiện tự động và đánh giá của con người. Hạn chế cơ bản: các kiểm duyệt viên tập trung áp đặt phán quyết của họ về những gì cấu thành "misinformation", tạo ra rủi ro kiểm duyệt và thiên lệch văn hóa.

### 2.4.2 Community Notes (Birdwatch)

Community Notes [21] của Twitter đã giới thiệu **bridging-based consensus**: một ghi chú chỉ được hiển thị khi những người thường bất đồng ý kiến lại đồng ý rằng nó hữu ích. Thuật toán sử dụng phân tích nhân tử ma trận (matrix factorization) để xác định các ghi chú "bridging" — những ghi chú được đánh giá tích cực bởi các nhóm đa dạng. Điều này đạt được độ chính xác ~97% đối với thông tin sai lệch về COVID-19.

PoMV áp dụng nguyên lý bắc cầu này: tri thức được trích dẫn bởi các nguồn đa dạng (**Synaptic signal**) nhận được sự tin cậy cao hơn so với tri thức chỉ được trích dẫn bởi các nguồn tương tự.

### 2.4.3 Content-Agnostic Analysis

Nghiên cứu cho thấy misinformation có thể được phát hiện thông qua **mô hình lan truyền (propagation patterns)** mà không cần kiểm tra nội dung [22, 23]:

- Misinformation lan truyền nhanh hơn và xa hơn so với sự thật [24].
- Lan truyền do bot điều khiển (bot-driven propagation) cho thấy tính quy luật về thời gian (khoảng thời gian cố định) trong khi lan truyền tự nhiên cho thấy thời gian bất thường.
- Misinformation có xu hướng lan truyền qua các node có cấu trúc tương tự nhau, trong khi sự thật lan truyền qua các cộng đồng đa dạng.

Module **Spread Analysis** của PoMV (§5.3) triển khai việc phát hiện content-agnostic — phân tích *cách thức* tri thức lan truyền, chứ không bao giờ phân tích nội dung *những gì* nó viết. Điều này tránh hoàn toàn vấn đề kiểm duyệt.

## 2.5 Bio-Inspired Computing in Trust Systems

### 2.5.1 Immune System Models

Các hệ thống miễn dịch nhân tạo (artificial immune systems) [25] mô hình hóa bảo mật tính toán dựa trên các phản ứng miễn dịch sinh học. Các khái niệm chính được áp dụng trong PoMV:

- **Antibodies**: Phát hiện dựa trên chữ ký (signature-based detection) của các mô hình tấn công đã biết (PoMV: AntibodyRule).
- **Immune memory**: Phản hồi nhanh hơn đối với các mối đe dọa đã gặp trước đó (PoMV: các antibodies được lưu trữ trong VacuumFilter).
- **Cytokine signaling**: Lan truyền cảnh báo qua mạng lưới (PoMV: CRDT gossip).
- **Self/non-self discrimination**: Phân biệt hành vi bình thường và bất thường (PoMV: phân tích lan truyền content-agnostic).

### 2.5.2 Stigmergy and Ant Colony Optimization

Stigmergy [26] — phối hợp gián tiếp thông qua sửa đổi môi trường — truyền cảm hứng cho **Synaptic signal** của PoMV: các mô hình co-retrieval tạo ra các "đường mòn pheromone" (pheromone trails) giữa các knowledge units, cho phép các lộ trình học tập tự xuất hiện mà không cần thiết kế rõ ràng.

### 2.5.3 Ecological Models

Carrying capacity sinh thái [27] giới hạn mật độ quần thể trong một niche. PoMV áp dụng điều này cho tri thức: bài báo thứ 1.001 về "cách đun sôi nước" có giá trị cận biên gần như bằng không, trong khi bài báo đầu tiên về một chủ đề mới có giá trị tối đa. **Niche signal** triển khai điều này thông qua tính điểm phụ thuộc mật độ (density-dependent scoring).

## 2.6 CRDT-Based Distributed Systems

Conflict-free Replicated Data Types [17, 28] cho phép đạt được tính nhất quán cuối cùng (eventual consistency) mà không cần điều phối. PoMV phụ thuộc sâu sắc vào các CRDTs:

| CRDT Type | PoMV Usage | Property |
|-----------|-----------|----------|
| **G-Counter** | Các bộ đếm metabolism (truy vấn, truy xuất, trích dẫn, dwell time) | Tăng đơn điệu (Monotonically increasing) → không có clawback |
| **PN-Counter** | Theo dõi điểm tin cậy (trust score) | Tăng + Giảm (Increment + decrement) |
| **LWW-Register** | Giải quyết dự đoán, mức độ xác thực | Ghi đè theo thời gian (Last-writer-wins) với nhãn thời gian (timestamps) |
| **OR-Set** | Hồ sơ xác thực/thách thức, các antibodies | Phép hợp ưu tiên thêm (Add-wins union) |
| **VectorClock** | Đồng bộ trạng thái delta, thứ tự nhân quả | Nhất quán nhân quả (Causal consistency) |

*Table 3: Các loại CRDT được sử dụng trong PoMV và thuộc tính của chúng.*

Việc lựa chọn G-Counter cho metabolism là có chủ đích: **G-Counters chỉ có thể tăng**. Điều này đảm bảo rằng các phần thưởng trong quá khứ không bao giờ bị thu hồi — một nguyên lý thiết kế cơ bản giúp loại bỏ tranh cãi về việc thu hồi phần thưởng (clawback).

## 2.7 Summary: What Exists vs. What PoMV Provides

| Capability | Prior Art | PoMV |
|-----------|-----------|------|
| Knowledge valuation | Đánh giá của con người (bình chọn, bình duyệt) | Sử dụng có thể quan sát (metabolism) |
| Subjective knowledge | Không thể xử lý | Chỉ dựa trên metabolism (không cần tính đúng đắn) |
| Temporal dynamics | Điểm số tĩnh (Stack Overflow) | Suy giảm chu kỳ bán rã (Half-life decay) + quỹ đạo metabolism |
| Anti-manipulation | Kiểm duyệt nội dung (rủi ro kiểm duyệt) | Phân tích lan truyền content-agnostic |
| Antifragility | Không có (tấn công = thiệt hại) | Immune memory (tấn công = mạnh hơn) |
| Decentralized trust | EigenTrust (toàn cục) | EigenTrust + per-domain + tính toán cục bộ |
| Reward fairness | Thu hồi (clawback - gây tranh cãi) | G-Counter (phần thưởng vĩnh viễn) |
| Novelty incentive | Lợi thế người đi trước (first-mover advantage) | Điểm thưởng entropy khi tạo lập |

*Table 4: Vị thế của PoMV so với các giải pháp trước đây trên 8 khía cạnh.*

---

## References

[1] H. Zuckerman and R. K. Merton, "Patterns of Evaluation in Science," *Minerva*, vol. 9, no. 1, pp. 66–100, 1971.

[2] M. Kovanis *et al.*, "The Global Burden of Journal Peer Review in the Biomedical Literature," *PLoS ONE*, vol. 11, no. 11, 2016.

[3] A. Franco, N. Malhotra, and G. Simonovits, "Publication Bias in the Social Sciences," *Science*, vol. 345, no. 6203, pp. 1502–1505, 2014.

[4] Open Science Collaboration, "Estimating the Reproducibility of Psychological Science," *Science*, vol. 349, no. 6251, 2015.

[5] C. G. Begley and L. M. Ellis, "Raise Standards for Preclinical Cancer Research," *Nature*, vol. 483, pp. 531–533, 2012.

[6] J. P. A. Ioannidis, "Why Most Published Research Findings Are False," *PLoS Medicine*, vol. 2, no. 8, 2005.

[7] A. Kittur *et al.*, "He Says, She Says: Conflict and Coordination in Wikipedia," in *Proc. CHI '07*, 2007.

[8] R. S. Geiger and D. Ribes, "The Work of Sustaining Order in Wikipedia," in *Proc. CSCW '10*, 2010.

[9] B. Collier and J. Bear, "Conflict, Criticism, or Confidence: An Empirical Examination of the Gender Gap in Wikipedia," in *Proc. CSCW '12*, 2012.

[10] L. Mamykina *et al.*, "Design Lessons from the Fastest Q&A Site in the West," in *Proc. CHI '11*, 2011.

[11] S. D. Kamvar, M. T. Schlosser, and H. Garcia-Molina, "The EigenTrust Algorithm for Reputation Management in P2P Networks," in *Proc. WWW '03*, pp. 640–651, 2003.

[12] H. Yu *et al.*, "SybilGuard: Defending Against Sybil Attacks via Social Networks," *IEEE/ACM ToN*, vol. 16, no. 3, pp. 576–589, 2008.

[13] Q. Cao *et al.*, "Aiding the Detection of Fake Accounts in Large Scale Social Online Services," in *Proc. NSDI '12*, pp. 197–210, 2012.

[14] Nostr Protocol, "Notes and Other Stuff Transmitted by Relays," 2023.

[15] S. Nakamoto, "Bitcoin: A Peer-to-Peer Electronic Cash System," 2008.

[16] V. Buterin, "Ethereum: A Next-Generation Smart Contract and Decentralized Application Platform," 2014.

[17] M. Shapiro *et al.*, "A Comprehensive Study of Convergent and Commutative Replicated Data Types," INRIA RR-7506, 2011.

[18] A. Ball *et al.*, "Proofs of Useful Work," *IACR Cryptology ePrint Archive*, 2021.

[19] J. Wolfers and E. Zitzewitz, "Prediction Markets," *JEP*, vol. 18, no. 2, pp. 107–126, 2004.

[20] R. Hanson, "Shall We Vote on Values, But Bet on Beliefs?," *Journal of Political Philosophy*, vol. 21, no. 2, pp. 151–178, 2013.

[21] Twitter/X, "Community Notes: Bridging-Based Ranking," 2023.

[22] S. Vosoughi, D. Roy, and S. Aral, "The Spread of True and False News Online," *Science*, vol. 359, no. 6380, pp. 1146–1151, 2018.

[23] K. Sharma *et al.*, "Combating Fake News: A Survey on Identification and Mitigation Techniques," *ACM TIST*, vol. 10, no. 3, 2019.

[24] S. Vosoughi, D. Roy, and S. Aral, "The Spread of True and False News Online," *Science*, 2018.

[25] D. Dasgupta, "Artificial Immune Systems and Their Applications," Springer, 1999.

[26] P.-P. Grassé, "La reconstruction du nid et les coordinations interindividuelles chez Bellicositermes natalensis," *Insectes Sociaux*, vol. 6, pp. 41–80, 1959.

[27] E. P. Odum, *Fundamentals of Ecology*, 3rd ed. Saunders, 1971.

[28] P. S. Almeida, A. Shoker, and C. Baquero, "Delta State Replicated Data Types," *JPDC*, vol. 111, pp. 162–173, 2018.
