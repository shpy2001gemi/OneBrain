# Proof-of-Metabolic-Value: An Observation-Based Consensus Mechanism for Decentralized Knowledge Networks

**Authors:** OneBrain Project Contributors  
**Contact:** shpy2001@gmail.com  
**Date:** June 2026  
**Version:** 2.0

---

## Abstract

Các consensus mechanisms hiện tại cho các hệ thống tri thức phụ thuộc vào **sự đánh giá của con người (human judgment)**: peer review yêu cầu sự đánh giá chuyên gia, prediction markets đòi hỏi các lượt đặt cược rõ ràng, và reputation-weighted voting yêu cầu các thành viên cộng đồng đánh giá chất lượng nội dung. Tất cả các cơ chế này đều kế thừa một câu hỏi nền tảng không thể trả lời một cách khách quan — *"Tri thức này có đúng không?"* — tạo ra các điểm nghẽn về khả năng mở rộng (scalability), khả năng dễ bị tổn thương trước sự thao túng có tổ chức, và các mâu thuẫn triết học khi áp dụng cho tri thức mang tính chủ quan, văn hóa hoặc trải nghiệm.

Bài báo này trình bày **Proof-of-Metabolic-Value (PoMV)**, một consensus mechanism thay thế việc bỏ phiếu bằng **sự quan sát (observation)**. Được truyền cảm hứng từ quá trình chuyển hóa sinh học (biological metabolism) — nơi các tế bào duy trì chức năng sẽ tồn tại và các tế bào ngừng hoạt động sẽ trải qua quá trình tự hủy (apoptosis) — PoMV xác định giá trị tri thức thông qua 6 tín hiệu có thể quan sát được mà không một tác nhân đơn lẻ nào có thể ngụy tạo ở quy mô lớn: (1) **Metabolism** — việc sử dụng thực tế được theo dõi qua G-Counter CRDTs (lượt truy vấn, lượt truy xuất, trích dẫn, dwell time, các phái sinh); (2) **Prediction** — độ chính xác thực nghiệm của các kiến thức được mã hóa dự đoán (knowledge-encoded predictions) được giải quyết qua 4 phương pháp; (3) **Entropy** — tính mới tại thời điểm tạo lập được đo lường qua cosine distance trên int8 embeddings với exponential decay 7 ngày; (4) **Survival** — khả năng phục hồi chống lại các adversarial attacks được theo dõi bởi một hệ thống miễn dịch lấy cảm hứng từ sinh học (bio-inspired immune system) với các antibodies không phụ thuộc vào nội dung (content-agnostic antibodies); (5) **Synaptic** — độ trung tâm mạng lưới (network centrality) thông qua Hebbian co-retrieval bonds và PageRank scoring; và (6) **Niche** — sự phù hợp sinh thái (ecological fitness) đo lường độ khan hiếm và carrying capacity trong hệ sinh thái tri thức.

Cơ chế này **hoàn toàn phi tập trung (fully decentralized)** (mỗi node tính toán độc lập, CRDT merge đảm bảo tính hội tụ), **content-agnostic** (không có node nào đánh giá tính chính xác của nội dung), và **không trừng phạt (non-punitive)** (G-Counters chỉ tăng dần — các phần thưởng trong quá khứ là vĩnh viễn). Các epistemic status transitions (Rumor → Formally Proven) diễn ra thông qua 9 ngưỡng có thể quan sát được mà không cần bỏ phiếu. Một hệ thống bộ nhớ miễn dịch đối kháng (adversarial immune memory system) tạo ra **tính phản dễ vỡ (antifragility)** — các cuộc tấn công giúp mạng lưới mạnh mẽ hơn.

Bản triển khai bao gồm **16 modules** (~5.012 LOC Rust) với **157 tests**, cùng với một gossip protocol để lan truyền metabolism dựa trên CRDT. EigenTrust-based node reputation, phân tích sự lan truyền content-agnostic (content-agnostic spread analysis), và carrying capacity sinh thái cùng nhau phòng thủ chống lại Sybil attacks, spam, và disinformation — mà không cần kiểm duyệt nội dung.

PoMV thể hiện quan điểm triết học rằng *tri thức không đúng hay sai — nó chỉ được thay thế bằng tri thức tốt hơn*, chuyển dịch quan điểm này thành một cơ chế được đặc tả hình thức, có thể triển khai và có thể kiểm thử.

**Keywords:** Consensus mechanism, knowledge valuation, proof of knowledge, decentralized trust, CRDT, metabolic value, observation-based consensus, Sybil resistance, antifragile systems, epistemic status, bio-inspired computing, knowledge graph, G-Counter, EigenTrust, knowledge discovery
