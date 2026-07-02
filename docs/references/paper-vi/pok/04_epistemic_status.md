# 4. Epistemic Status State Machine

Phần này hình thức hóa hệ thống epistemic status — 9 observable transitions từ Rumor sang Formally Proven, hoàn toàn không cần bỏ phiếu.

## 4.1 Motivation

Các hệ thống tri thức truyền thống sử dụng các nhãn nhị phân: "đã xác thực" hoặc "chưa xác thực", "đúng" hoặc "sai". Điều này không đủ cho việc quản lý tri thức:

- Một hypothesis không phải là "đúng" hay "sai" — nó là một tuyên bố đang chờ đợi bằng chứng.
- Một observation không phải là "đã xác thực" — nó là một báo cáo ở ngôi thứ nhất đang chờ được chứng thực.
- Một consensus không phải là vĩnh viễn — nó có thể bị lật đổ bởi các bằng chứng mới.

PoMV sử dụng một thang đo epistemic status gồm 11 cấp độ lấy cảm hứng từ triết học (Justified True Belief [1]), luật pháp (standards of proof [2]), và khoa học (NASA Technology Readiness Levels [3]):

$$\text{Rumor} \rightarrow \text{Hearsay} \rightarrow \text{Testimony} \rightarrow \text{Observation} \rightarrow \text{Hypothesis} \rightarrow \text{Evidence}$$
$$\rightarrow \text{Corroborated} \rightarrow \text{Peer Reviewed} \rightarrow \text{Consensus} \rightarrow \text{Formally Proven} \rightarrow \text{Axiomatic}$$

## 4.2 The Key Innovation: Observable Thresholds

> **Mỗi quá trình transition đều được kích hoạt bởi các observable, CRDT-measurable thresholds. Không có con người nào tham gia bỏ phiếu về epistemic status.**

Đây là quyết định thiết kế quan trọng nhất trong PoMV. Mỗi transition đều có các điều kiện được định nghĩa chính xác mà bất kỳ node nào cũng có thể tự xác minh độc lập từ trạng thái local CRDT của mình:

```mermaid
graph LR
    R["RUMOR"] -->|"metabolic_rate > 0.001"| H["HEARSAY"]
    H -->|"retrieval_count ≥ 3"| T["TESTIMONY"]
    T -->|"citation_count ≥ 1"| O["OBSERVATION"]
    O -->|"citations ≥ 3 AND<br/>diversity ≥ 3"| HY["HYPOTHESIS"]
    HY -->|"node_diversity ≥ 5"| EV["EVIDENCE"]
    EV -->|"citations ≥ 5"| CO["CORROBORATED"]
    CO -->|"engagement ≥ 50"| PR["PEER_REVIEWED"]
    PR -->|"age ≥ 6 tháng AND<br/>rate ≥ 1.0"| CON["CONSENSUS"]
    CON -->|"age ≥ 1 năm AND<br/>engagement ≥ 200"| FP["FORMALLY_PROVEN"]
    
    style R fill:#ef4444,color:#fff
    style H fill:#f97316,color:#fff
    style T fill:#eab308,color:#000
    style O fill:#84cc16,color:#000
    style HY fill:#22c55e,color:#fff
    style EV fill:#14b8a6,color:#fff
    style CO fill:#06b6d4,color:#fff
    style PR fill:#3b82f6,color:#fff
    style CON fill:#6366f1,color:#fff
    style FP fill:#8b5cf6,color:#fff
```

*Figure 3: Epistemic status state machine với các điều kiện transition có thể quan sát được.*

## 4.3 Transition Specification

### Transition 1: RUMOR → HEARSAY

**Condition:** $\text{metabolic\_rate}(ku, t) > 0.001$

**Ý nghĩa:** Ai đó, ở một nơi nào đó, đã truy cập tri thức này. "Nhịp tim" (heartbeat) của KU đã bắt đầu.

**Observable metric:** G-Counter `query_hits` hoặc `retrieval_count` > 0 và metabolic rate nằm trên alive threshold.

**Lý do chọn ngưỡng này:** Ngưỡng sống (alive threshold - 0.001) được thiết lập thấp một cách có chủ đích — chỉ cần một lần truy cập thực tế là đủ. Mục tiêu chỉ đơn giản là phân biệt giữa "đã được nhìn thấy" và "chưa từng được nhìn thấy."

### Transition 2: HEARSAY → TESTIMONY

**Condition:** $\text{retrieval\_count} \geq 3$

**Ý nghĩa:** Ít nhất 3 lần truy xuất riêng biệt cho thấy sự quan tâm bền vững. KU không phải là sự tò mò nhất thời mà là thứ mà mọi người quay lại để đọc.

**Observable metric:** G-Counter `retrieval_count` được cộng tổng giữa các nodes.

**Lý do chọn số 3:** Dưới 3, một người dùng quan tâm duy nhất có thể tạo ra tất cả các lượt truy xuất. Ở mức 3 trở lên, sự quan tâm độc lập có khả năng cao hơn (mặc dù không đảm bảo hoàn toàn — do đó đây là "Testimony", chứ không phải "Evidence").

### Transition 3: TESTIMONY → OBSERVATION

**Condition:** $\text{citation\_count} \geq 1$

**Ý nghĩa:** Một KU khác đã trích dẫn KU này. Tri thức đã đi vào mạng lưới trích dẫn (citation network) — nó đang được sử dụng làm khối xây dựng cho tri thức mới.

**Observable metric:** G-Counter `citation_count`.

**Lý do chọn số 1:** Một trích dẫn đi vào (inbound citation) duy nhất chứng minh rằng người tạo ra KU không phải là người duy nhất coi tri thức này là đáng tham chiếu. Sự chuyển dịch từ "ai đó đã đọc nó" sang "ai đó đã xây dựng dựa trên nó" là có ý nghĩa quan trọng về mặt chất.

### Transition 4: OBSERVATION → HYPOTHESIS

**Condition:** $\text{citation\_count} \geq 3$ AND $\text{node\_diversity} \geq 3$

**Ý nghĩa:** Nhiều trích dẫn từ các nguồn đa dạng. Tri thức đang được tham chiếu bởi nhiều tác nhân độc lập.

**Observable metrics:** G-Counter `citation_count` + G-Counter `unique_nodes`.

**Lý do cần yêu cầu diversity:** 3 trích dẫn từ một node duy nhất có thể là tự trích dẫn (self-citation). Yêu cầu diversity ≥ 3 đảm bảo rằng ít nhất 3 nodes khác nhau đã tương tác với KU này, làm cho việc tấn công giả mạo danh tính (Sybil gaming) trở nên tốn kém hơn.

### Transition 5: HYPOTHESIS → EVIDENCE

**Condition:** $\text{node\_diversity} \geq 5$

**Ý nghĩa:** Tri thức đã được truy cập bởi ít nhất 5 nodes riêng biệt trong mạng lưới. Việc mở rộng tiếp xúc này làm tăng khả năng tri thức đó đã được đánh giá một cách độc lập.

**Observable metric:** G-Counter `unique_nodes`.

**Lý do chọn số 5:** Khi diversity ≥ 5, chi phí cho Sybil gaming trở nên đáng kể — kẻ tấn công sẽ cần 5 nodes riêng biệt, mỗi node có bản sắc riêng, tài nguyên tính toán riêng và các mô hình sử dụng hợp lý.

### Transition 6: EVIDENCE → CORROBORATED

**Condition:** $\text{citation\_count} \geq 5$

**Ý nghĩa:** Bằng chứng trích dẫn mạnh mẽ. Năm KUs độc lập tham chiếu tri thức này làm nền tảng.

**Observable metric:** G-Counter `citation_count`.

**Tương đồng:** Trong xuất bản học thuật, một bài báo có 5 trích dẫn trở lên được coi là đã đóng góp được công nhận cho lĩnh vực đó.

### Transition 7: CORROBORATED → PEER REVIEWED

**Condition:** $\text{total\_engagement} \geq 50$

**Ý nghĩa:** Tương tác lớn — tổng của tất cả các bộ đếm sử dụng (truy vấn + truy xuất + trích dẫn + phái sinh + bác bỏ + corroborations + downstream) vượt quá 50.

**Observable metric:** Tổng của tất cả các G-Counters.

**Lý do chọn số 50:** Ngưỡng này đảm bảo sự tương tác cộng đồng rộng rãi, không chỉ là việc đọc thụ động. Với tổng số sự kiện tương tác từ 50 trở lên, tri thức đã được cộng đồng xem xét kỹ lưỡng.

### Transition 8: PEER REVIEWED → CONSENSUS

**Condition:** $\text{age} \geq 15{,}552{,}000\ \text{s}$ (6 tháng) AND $\text{metabolic\_rate} \geq 1.0$

**Ý nghĩa:** Tri thức đã duy trì metabolism cao trong ít nhất 6 tháng. Đây là "bài kiểm tra thời gian" — không chỉ là sự phổ biến nhất thời mà là giá trị **bền vững** (sustained).

**Observable metrics:** Nhãn thời gian tạo lập (creation timestamp) + metabolic rate hiện tại.

**Lý do chọn 6 tháng:** Nội dung lan truyền nhanh (viral) có thể tạo ra tương tác ngắn hạn cao nhưng thiếu giá trị lâu dài. Yêu cầu 6 tháng lọc ra các kiến thức mang lại giá trị bền vững — như một phát hiện khoa học tiếp tục được trích dẫn nhiều tháng sau khi xuất bản.

### Transition 9: CONSENSUS → FORMALLY PROVEN

**Condition:** $\text{age} \geq 31{,}536{,}000\ \text{s}$ (1 năm) AND $\text{total\_engagement} \geq 200$

**Ý nghĩa:** Một năm đầy đủ tương tác cao bền vững. Đây là trạng thái non-axiomatic cao nhất có thể đạt được.

**Observable metrics:** Nhãn thời gian tạo lập + tổng số lượng tương tác (total engagement).

**Lý do chọn các ngưỡng này:** Sau 1 năm và hơn 200 sự kiện tương tác, tri thức đã được sử dụng, trích dẫn và tham chiếu liên tục trong một khoảng thời gian dài. Điều này tương ứng với tri thức đã trở thành nền tảng trong lĩnh vực của nó.

### Terminal States

**FORMALLY PROVEN** và **AXIOMATIC** là các trạng thái cuối cùng (terminal states) — không có quá trình transition nào xảy ra thêm. AXIOMATIC được dành riêng cho các chân lý toán học và logic (ví dụ: $1 + 1 = 2$) được đặt tại thời điểm tạo lập, chứ không đạt được thông qua metabolism.

## 4.4 Formal Properties

### 4.4.1 Monotonicity

Hệ thống trạng thái (status) chỉ có thể **tăng lên** — một khi một KU đạt đến một status, nó không thể bị hạ cấp trở lại status thấp hơn chỉ thông qua state machine này.

**Sơ lược chứng minh (Proof sketch):** Mỗi G-Counter là tăng đơn điệu không giảm (chỉ tăng - increment-only). Mỗi điều kiện ngưỡng là một cận dưới của một giá trị tăng đơn điệu không giảm. Do đó, một khi điều kiện được thỏa mãn, nó sẽ vẫn được thỏa mãn đối với tất cả các trạng thái trong tương lai. ∎

### 4.4.2 Determinism

Với cùng một trạng thái CRDT, bất kỳ hai nodes nào cũng sẽ tính toán cùng một epistemic status cho một KU.

**Sơ lược chứng minh (Proof sketch):** Hàm `evaluate_max_status` đi lên các nấc thang trạng thái từ cấp độ hiện tại, kiểm tra lần lượt từng ngưỡng. Tất cả các ngưỡng đều là các hàm xác định (deterministic functions) của các giá trị CRDT. CRDT merge có tính hội tụ — cuối cùng tất cả các nodes sẽ có cùng giá trị bộ đếm. Do đó, epistemic status cuối cùng sẽ hội tụ giữa tất cả các nodes. ∎

### 4.4.3 Convergence

Epistemic status cuối cùng sẽ nhất quán trên toàn mạng lưới. Do ngữ nghĩa của CRDT merge, tất cả các nodes sẽ hội tụ về cùng một status cho mỗi KU.

**Lưu ý (Caveat):** Trong thời gian xảy ra phân tách mạng (network partitions), các nodes khác nhau có thể tạm thời gán các status khác nhau. Điều này có thể chấp nhận được vì:
1. Trạng thái (status) chỉ ảnh hưởng đến việc tính điểm PoMV cục bộ, không ảnh hưởng đến trạng thái toàn cục.
2. Khi phân tách mạng được khắc phục, CRDT merge sẽ đối chiếu các bộ đếm và trạng thái sẽ hội tụ.

## 4.5 Addressing the "But Is It True?" Objection

Một phản đối tự nhiên: "Epistemic status đo lường *sự phổ biến* (popularity), chứ không phải *sự thật* (truth). Thông tin sai lệch (misinformation) có thể đạt được trạng thái CONSENSUS."

Phản hồi của PoMV bao gồm nhiều lớp:

1. **Về mặt triết học:** PoMV từ chối một cách rõ ràng tuyên bố rằng bất kỳ hệ thống nào cũng có thể xác định sự thật tuyệt đối. Ngay cả khoa học được bình duyệt (peer-reviewed science) cũng có tỷ lệ thất bại sao chép lên tới 60% [4]. Những gì PoMV đo lường là **tiện ích bền vững (sustained utility)** — liệu tri thức có tiếp tục hữu ích cho mọi người hay không.

2. **Prediction Signal:** Thông tin sai lệch đưa ra các dự đoán sai sẽ có điểm Prediction thấp khi các dự đoán đó bị bác bỏ. Điều này không ngăn cản trạng thái CONSENSUS nhưng làm giảm điểm PoMV tổng thể.

3. **Chọn lọc tự nhiên (Natural Selection):** Tri thức tốt hơn về cùng một chủ đề sẽ tự động thu hút metabolism từ tri thức kém hơn. Sức chứa (Niche signal) giới hạn số lượng KUs về cùng một chủ đề có thể phát triển mạnh — cuối cùng, KU hoạt động metabolism tích cực nhất (hữu ích nhất) sẽ chiếm ưu thế.

4. **Hệ thống miễn dịch (Immune System):** Các chiến dịch có tổ chức nhằm thổi phồng metabolism một cách nhân tạo sẽ bị phát hiện bởi Phân tích lan truyền content-agnostic (content-agnostic Spread Analysis) và Immune Engine (§5).

5. **Câu trả lời nền tảng:** Nếu một phần tri thức được sử dụng bởi hàng nghìn người trong nhiều năm, được trích dẫn bởi hàng trăm KUs khác và sống sót qua các thách thức đối kháng (adversarial challenges) — nó đã *chứng minh được giá trị* bất kể một nhà triết học có gọi nó là "đúng" hay không. Cơ học của Newton là "sai" (đã bị thay thế bởi Einstein), nhưng nó vẫn vô cùng giá trị và được sử dụng rộng rãi hơn 300 năm sau.

## 4.6 Comparison with Traditional Epistemic Systems

| Khía cạnh | Academic Peer Review | Wikipedia | PoMV Epistemic Status |
|---------|---------------------|-----------|----------------------|
| **Cấp độ (Levels)** | Nhị phân (xuất bản/chưa) | Nhị phân (được xác thực/chưa) | 11 cấp độ dần dần |
| **Cơ chế transition** | Quyết định của biên tập viên | Đồng thuận của biên tập viên | Các ngưỡng CRDT có thể quan sát được |
| **Khả năng đảo ngược** | Thu hồi (hiếm, bị kỳ thị) | Chỉnh sửa bất kỳ | Monotonic (không bị hạ cấp) |
| **Yếu tố thời gian** | Không có (tĩnh sau khi xuất bản) | Không có (tĩnh sau khi xác thực) | Các cổng thời gian 6 tháng và 1 năm |
| **Tri thức chủ quan** | Không áp dụng | Xóa bỏ do "Không đáng chú ý" | Hỗ trợ toàn bộ vòng đời |
| **Decentralization** | Tập trung (các biên tập viên tạp chí) | Bán tập trung (các quản trị viên) | Phi tập trung hoàn toàn (CRDT) |
| **Scalability** | Bị nghẽn bởi số lượng người bình duyệt | Bị nghẽn bởi số lượng biên tập viên | Không giới hạn (tự động hóa) |

*Table 8: So sánh các thuộc tính epistemic status giữa các hệ thống tri thức.*

---

## References

[1] E. L. Gettier, "Is Justified True Belief Knowledge?," *Analysis*, vol. 23, no. 6, pp. 121–123, 1963.

[2] J. O. Newman, "Quantifying the Standard of Proof Beyond a Reasonable Doubt," *Law, Probability and Risk*, vol. 5, no. 3–4, pp. 171–186, 2006.

[3] J. C. Mankins, "Technology Readiness Levels," NASA White Paper, 1995.

[4] Open Science Collaboration, "Estimating the Reproducibility of Psychological Science," *Science*, vol. 349, no. 6251, 2015.
