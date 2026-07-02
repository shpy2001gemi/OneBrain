# OneBrain Token: Một Knowledge Utility Token với Account-Chain Ledger và Output-Based Minting

**Tác giả:** OneBrain Protocol Team

**Ngày:** Tháng 7 năm 2026

**Phiên bản:** 1.0

---

## Tóm tắt (Abstract)

Chúng tôi giới thiệu OneBrain Token (OBT), một utility token được thiết kế để khuyến khích việc đóng góp, encoding, verification và storage tri thức trong OneBrain Protocol (OBP) — một hệ thống quản lý tri thức phi tập trung. Khác với các token cryptocurrency truyền thống vốn có giá trị từ sự khan hiếm nhân tạo, OBT lấy giá trị từ *knowledge utility*, hoạt động như một đơn vị tính toán cho các hoạt động trí tuệ có ý nghĩa, tương tự như cách kilowatt-giờ đo lường năng lượng.

OBT giới thiệu một số đóng góp mới cho không gian token design. Đầu tiên, chúng tôi áp dụng một *Nano-inspired Account-Chain ledger*, trong đó mỗi người tham gia duy trì một append-only chain độc lập, đạt được các giao dịch peer-to-peer miễn phí với thời gian hoàn thành dưới một giây mà không cần một blockchain toàn cầu. Chúng tôi chứng minh một cách chính thức rằng các CRDT counter truyền thống (G-Counter, PN-Counter) không phù hợp cho việc theo dõi số dư, và trình bày cách kiến trúc Account-Chain giải quyết vấn đề overdraft trong khi vẫn bảo toàn các thuộc tính conflict-free cần thiết cho quá trình lan truyền dựa trên gossip protocol.

Thứ hai, chúng tôi đề xuất một hệ thống *output-based minting* được điều hành bởi công thức phát hành toàn cầu $E(\text{epoch}) = B \times A(\text{epoch}) \times Q(\text{epoch})$, liên kết trực tiếp việc phát hành token với hoạt động mạng lưới và chất lượng tri thức được đo lường bởi giao thức Proof of Meaningful Verification (PoMV). Bốn dòng phần thưởng riêng biệt — owner rewards (R1, 40%), encoding rewards (R2, 25%), verification rewards (R3, 15%), và storage rewards (R4, 20%) — đảm bảo rằng tất cả những người tham gia vào vòng đời tri thức đều được đền bù tương xứng với đóng góp của họ.

Thứ ba, chúng tôi giới thiệu cơ chế *trust-as-resource-proxy* thay thế phí giao dịch bằng quyền truy cập được kiểm soát bởi danh tiếng (reputation-gated access), công thức *5-factor content-aware storage reward* với các thử thách Proof-of-Storage, và hệ thống hình phạt lũy tiến 5 cấp độ (*5-tier graduated penalty system*) với cơ chế khuếch đại dựa trên mối tương quan lấy cảm hứng từ thiết kế slashing của Ethereum 2.0. Sự tách biệt giữa các token đã kiếm được (vĩnh viễn, G-Counter) và uy tín trust reputation (có thể bị slash) thể hiện một quan điểm triết học mới: "Chúng tôi không thu hồi tiền lương cũ; chúng tôi thu hồi giấy phép hành nghề y."

OBT được triển khai dưới dạng 10 Rust module bao gồm khoảng 243 KB mã nguồn với hơn 240 unit test, được tích hợp trong hệ sinh thái OBP rộng lớn hơn với tổng cộng 733 bài test trên các crate ku-core và ku-net. Chúng tôi cung cấp phân tích bảo mật toàn diện bao gồm năm attack vector, ba kịch bản phân tách mạng (network partition), và chứng minh rằng trong tất cả các trường hợp được mô hình hóa, chi phí gian lận vượt quá lợi ích của gian lận.

---

## Từ khóa (Keywords)

Knowledge token, utility token, Account-Chain ledger, block-lattice, output-based minting, Proof of Meaningful Verification (PoMV), storage reward, Proof of Storage, anti-gaming, trust-as-resource-proxy, CRDT, gossip protocol, penalty system, correlation slashing, EigenTrust, decentralized knowledge management

---

## Mục lục (Table of Contents)

1. [Giới thiệu](01_introduction.md)
2. [Nghiên cứu liên quan (Related Work)](02_related_work.md)
3. [Triết lý thiết kế Token (Token Design Philosophy)](03_token_design.md)
4. [Account-Chain Ledger](04_ledger.md)
5. [Output-Based Minting](05_minting.md)
6. [Phần thưởng lưu trữ nhận biết nội dung (Content-Aware Storage Rewards)](06_storage_reward.md)
7. [Chống gian lận và Đảm bảo chất lượng (Anti-Gaming and Quality Assurance)](07_anti_gaming.md)
8. [Hệ thống hình phạt lũy tiến (Graduated Penalty System)](08_penalty.md)
9. [Đánh giá (Evaluation)](09_evaluation.md)
10. [Kết luận (Conclusion)](10_conclusion.md)
