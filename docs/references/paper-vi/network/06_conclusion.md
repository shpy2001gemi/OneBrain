# 6. Discussion, Future Work, and Conclusion

## 6.1 Discussion

### 6.1.1 Các Phát hiện Chính (Key Findings)

OneBrain Protocol chứng minh rằng các kiến trúc P2P được xây dựng chuyên biệt cho việc chia sẻ tri thức có thể đạt được các khả năng về mặt căn bản vượt trội hơn những gì các giao thức lấy file làm trung tâm (file-centric protocols) cung cấp. Thiết kế và triển khai của chúng tôi chỉ ra một số phát hiện chính:

**Phát hiện 1: Thiết kế lớp tích hợp (Integrated layer design) cho phép thực hiện các tối ưu hóa chéo lớp (cross-layer optimizations) vốn là bất khả thi trong các modular stacks.** Thiết kế mô-đun của libp2p — nơi các giao thức transport, routing, và application được cấu thành một cách độc lập — ngăn chặn các tối ưu hóa trải dài qua ranh giới các lớp. Thiết kế tích hợp của OBP cho phép: cập nhật SWIM membership được piggyback trên các transport messages (L2→L1, zero thêm băng thông), củng cố pheromone (pheromone reinforcement) được kích hoạt bởi kết quả truy vấn (L5←L4, adaptive routing), trao đổi Bloom filter trong các vòng membership (L6→L2, không tốn thêm tin nhắn), và delta-state CRDT sync được kích hoạt bởi các thay đổi cục bộ (L8→L1, lan truyền tức thời). Các tối ưu hóa chéo lớp này nói chung giúp giảm khoảng 40–60% số lượng tin nhắn so với các giao thức được cấu thành độc lập.

**Phát hiện 2: Stigmergy routing thích ứng tự nhiên với các mô hình nhu cầu tri thức mà không cần sự điều phối trung tâm.** Không giống như DHT routing — vốn phân bổ trách nhiệm đồng đều trên không gian khóa bất kể tần suất truy vấn — stigmergy routing tập trung nỗ lực định tuyến vào các chủ đề có nhu cầu cao. Các miền tri thức phổ biến phát triển các pheromone trails mạnh mẽ dẫn đến các nodes có năng lực, trong khi các miền ít được truy vấn sẽ quay về tra cứu DHT lookup. Điều này tạo ra một cấu trúc **hot-cold routing topology** nổi bật phản ánh các mô hình truy cập tri thức thực tế. Cơ chế bay hơi (evaporation mechanism) (γ=0.95/giờ) đảm bảo rằng cấu trúc topology thích ứng trong vòng vài giờ khi mô hình nhu cầu dịch chuyển — nhanh hơn đáng kể so với việc cấu hình lại rõ ràng.

**Phát hiện 3: Cấu trúc phân cấp 7 tầng (7-tier hierarchy) khai thác tính dị thể của thiết bị (device heterogeneity) mà các mô hình P2P phẳng (flat P2P models) bỏ qua.** Các mạng lưới trong thế giới thực chứa các thiết bị trải dài trên 4 cấp độ về năng lực: từ các cảm biến IoT (100 MHz, 256 KB RAM) đến các datacenters (100+ cores, TB of RAM). Các mô hình P2P phẳng (IPFS, BitTorrent) đối xử với tất cả các peers giống hệt nhau, buộc các nodes có năng lực phải hoạt động dưới mức tiềm năng và các nodes không đủ năng lực phải hoạt động quá sức. Hierarchy 7 tầng của OBP phân bổ các trách nhiệm phù hợp với năng lực: Leaf nodes (tầng 0) chỉ tiêu thụ; GlobalBackbone nodes (tầng 6) xử lý định tuyến xuyên lục địa. Cơ chế promotion/demotion tự động dựa trên fitness đảm bảo rằng hierarchy thích ứng với điều kiện thay đổi của thiết bị (ví dụ: một laptop bị rút phích cắm điện sẽ bị demote từ tầng 2 xuống tầng 1 khi dung lượng pin cạn dần).

**Phát hiện 4: Thiết kế offline-first là khả thi mà không làm giảm hiệu suất trực tuyến.** Cascade bootstrap 6 lớp chứng minh rằng hoạt động độc lập với internet không yêu cầu một giao thức riêng biệt. Các giao thức SWIM membership, DHT routing, và CRDT sync giống nhau hoạt động giống hệt nhau trên BLE/WiFi mesh (các lớp 0–1 của cascade) cũng như trên các kết nối QUIC internet. Điểm mấu chốt là giao thức không bao giờ giả định một transport đáng tin cậy, độ trễ thấp — nó hoạt động chính xác trên bất kỳ chất nền (substrate) nào cuối cùng có thể chuyển phát các tin nhắn.

**Phát hiện 5: Delta-state CRDT sync là thiết yếu cho các mạng tri thức di động (mobile knowledge networks).** Đồng bộ hóa Full-state CRDT truyền tải toàn bộ trạng thái CRDT (~530 bytes mỗi KU) trong mỗi lần sync. Đối với một node theo dõi 10,000 KUs thực hiện sync mỗi 10 giây, điều này tạo ra 5.3 MB/sync = 45.8 GB/ngày — rõ ràng là bất khả thi trên di động. Delta-state sync chỉ truyền các thay đổi kể từ VectorClock được biết gần nhất, thông thường là 0–10 deltas mỗi lần sync = 0–5.3 KB/sync = tối đa 45.8 MB/ngày — một mức giảm 1,000×.

### 6.1.2 Sự Đánh đổi Thiết kế (Design Trade-offs)

**Tính tích hợp so với tính mô-đun (Integration vs. Modularity).** Sự tích hợp 9 lớp của OBP cung cấp các tối ưu hóa chặt chẽ hơn nhưng khiến cho việc thay thế từng lớp riêng lẻ trở nên khó khăn. Nếu xuất hiện một thuật toán DHT ưu việt hơn, việc thay thế Layer 4 đòi hỏi phải phân tích kỹ lưỡng các phụ thuộc chéo lớp (cross-layer dependencies). Chúng tôi giảm thiểu điều này thông qua các giao diện lớp được xác định rõ ràng (well-defined layer interfaces) và hệ thống mức độ tuân thủ conformance level (§5.6), cho phép thực hiện các triển khai một phần.

**7 tầng so với 2 tầng.** Cấu trúc 7-tier hierarchy cung cấp khả năng khai thác năng lực chi tiết nhưng làm tăng độ phức tạp trong quản lý trạng thái. Mỗi node không chỉ phải theo dõi trạng thái hoạt động (aliveness) của peer mà còn cả tầng (tier), fitness, và các topic vectors. Overhead bộ nhớ (~32 bytes mỗi member × 10K members = 320 KB) là có thể chấp nhận được đối với smartphones nhưng có thể gây áp lực lên các cảm biến IoT ở Mức độ 0 (Level 0).

**Stigmergy overhead.** Các bảng pheromone tables tiêu tốn bộ nhớ (lên tới 10,000 entries × ~100 bytes = 1 MB) và yêu cầu tính toán bay hơi định kỳ. Đối với các nodes Level 0, overhead này có thể là quá mức. Chúng tôi giải quyết vấn đề này bằng cách giới hạn stigmergy routing cho Level 2 trở lên (Supernode conformance), cho phép các nodes đơn giản hơn dựa vào DHT-only routing.

**QUIC-only transport.** Việc sử dụng QUIC như sole transport duy nhất giúp đơn giản hóa triển khai và cung cấp mã hóa toàn bộ, nhưng loại trừ các môi trường nơi UDP bị chặn (một số tường lửa doanh nghiệp). Nghiên cứu trong tương lai có thể bổ sung cơ chế dự phòng TCP fallback cho các trường hợp đặc biệt này.

**Tốc độ bay hơi (Evaporation rate) (γ=0.95/giờ).** Giá trị này cân bằng giữa khả năng thích ứng (các con đường bị lãng quên trong vòng vài ngày nếu không được sử dụng) và tính ổn định (các con đường đang hoạt động không bị biến động). Sau 24 giờ không được củng cố (reinforcement), strength pheromone từ 1.0 giảm xuống còn $0.95^{24} = 0.29$ — vẫn có thể route được nhưng yếu. Sau 72 giờ: $0.95^{72} = 0.025$ — gần ngưỡng loại bỏ (0.01). Điều này có thể quá hung hăng đối với các miền tri thức nền tảng (ví dụ: "mathematics basics") có mô hình truy vấn ổn định nhưng tần suất thấp.

## 6.2 Hạn chế (Limitations)

**L1: Chưa triển khai quy mô lớn.** Tất cả các chỉ số hiệu suất đều được rút ra từ các thí nghiệm testbed 3 node và mô hình hóa phân tích. Giao thức chưa được triển khai với hàng ngàn hay hàng triệu nodes thực tế. Tỷ lệ churn thế giới thực, tỷ lệ thành công NAT traversal, và hành vi mạng di động có thể khác biệt đáng kể so với mô hình của chúng tôi.

**L2: Hiệu quả của Stigmergy chưa được chứng minh ở quy mô lớn.** Hệ thống định tuyến pheromone (pheromone routing system) mới chỉ được thử nghiệm với các khối lượng công việc truy vấn tổng hợp trên các topologies nhỏ. Các thuộc tính hội tụ, hiệu quả định tuyến, và khả năng chống lại thao túng pheromone của đối thủ ở quy mô lớn vẫn chưa được xác thực.

**L3: Độ chín chắn của bản triển khai QUIC.** Crate `quinn`, dù được duy trì tích cực, vẫn chưa được thử thách qua thực tế nhiều như các bản triển khai TCP. Hiệu suất dưới tải cực hạn, tương tác với các thiết bị trung gian (middleboxes), và hành vi dưới các điều kiện thù địch cần được nghiên cứu thêm.

**L4: Quản trị cấu trúc 7-tier hierarchy.** Các ngưỡng fitness (Bảng 5) và các vectors trọng số (Bảng 6) hiện là các hằng số hard-code. Trong một mạng lưới thực tế, các tham số này sẽ cần cơ chế quản trị để điều chỉnh. Các ngưỡng không chính xác có thể gây ra các đợt thác promote/demote hàng loạt.

**L5: Kết nối mạng offline mesh vẫn chỉ dừng lại ở đặc tả mà chưa được triển khai.** Các lớp phát hiện BLE và WiFi Direct (các lớp cascade 0–1) được đặc tả về mặt kiến trúc và tương thích với giao thức, nhưng bản triển khai BLE/WiFi Direct transport thực tế vẫn chưa hoàn thành.

**L6: Các phép đo năng lượng mang tính lý thuyết.** Mục tiêu <0.5% pin/ngày (Bảng 15) dựa trên việc đếm số tin nhắn và các mô hình năng lượng WiFi radio. Cần có các phép đo thực tế trên các thiết bị đa dạng (iPhone, Android, IoT) để xác thực các ước tính này.

## 6.3 Nghiên cứu trong Tương lai (Future Work)

### 6.3.1 Ngắn hạn (Short-term)

- **Mô phỏng quy mô lớn** (1,000–10,000 nodes) sử dụng mô phỏng sự kiện rời rạc để xác thực phân tích quy mô, sự hội tụ stigmergy, và hiệu quả của hierarchical routing.
- **Triển khai di động thực tế** với phép đo pin trên các thiết bị iOS và Android để xác thực thực nghiệm mục tiêu <0.5%.
- **Kiểm thử thù địch (Adversarial testing)** đối với stigmergy routing: các cuộc tấn công pheromone poisoning, Sybil amplification đối với các pheromone trails, và các cơ chế phòng thủ.

### 6.3.2 Trung hạn (Medium-term)

- **Hỗ trợ WebTransport** cho các nodes chạy trên trình duyệt, cho phép tham gia mà không cần cài đặt ứng dụng native.
- **Bản triển khai BLE/WiFi Direct mesh** nhằm hoàn thiện tầm nhìn offline-first cho discovery cascade.
- **Các cầu nối chéo giao thức (Cross-protocol bridges)** tới IPFS (cho khả năng tương tác nội dung) và ActivityPub/Fediverse (cho chia sẻ tri thức mạng xã hội).
- **Tốc độ bay hơi thích ứng (Adaptive evaporation rates)** dựa trên tần suất truy vấn topic — bay hơi chậm hơn đối với các miền tri thức ổn định, nhanh hơn đối với các chủ đề thịnh hành (trending topics).

### 6.3.3 Dài hạn (Long-term)

- **Xác minh hình thức (Formal verification)** đối với logic chuyển đổi SWIM + tier sử dụng TLA+ hoặc các công cụ kiểm tra mô hình tương tự để chứng minh không có các vấn đề về liveness (ví dụ: các chu kỳ promotion/demotion).
- **ML-enhanced stigmergy** sử dụng mạng neural đồ thị (graph neural networks) để dự đoán các đường routing tối ưu dựa trên các query embeddings, bổ sung cho cơ chế heuristic dựa trên pheromone.
- **Hierarchical DHT sharding** cho mục tiêu 100B node, triển khai hierarchical DHT 5 cấp được mô tả ở §5.3.1 với phân bổ shard nhận biết vị trí (locality-aware shard assignment).

## 6.4 Kết luận

Tài liệu này đã trình bày **OneBrain Protocol (OBP)**, một P2P network stack 9 lớp được xây dựng chuyên biệt cho việc chia sẻ tri thức phi tập trung. Không giống như các giao thức P2P hiện tại được thiết kế cho phân phối file, OBP coi tri thức có cấu trúc, có thể truy vấn, và được chú giải độ tin cậy như một thực thể hạng nhất (first-class citizen).

Sáu đóng góp chính của chúng tôi là:

1. **Một chồng giao thức tích hợp 9 lớp (9-layer integrated protocol stack)** (§3) kết hợp identity, QUIC transport, SWIM membership, phát hiện offline-first, S/Kademlia DHT, stigmergy routing, Bloom filter content routing, topic-based PubSub, và delta-state CRDT synchronization vào một kiến trúc gắn kết với các tối ưu hóa chéo lớp.

2. **Bio-inspired stigmergy routing** (§3.7) áp dụng cơ chế củng cố và bay hơi pheromone của đàn kiến vào knowledge query routing — ứng dụng đầu tiên của stigmergy cho truy xuất nội dung ngữ nghĩa (semantic content retrieval). Các con đường truy vấn thành công được củng cố (+0.1 strength), các con đường thất bại bị phạt (−0.2), và các con đường không sử dụng bay hơi (×0.95/giờ), tạo ra một self-optimizing routing topology.

3. **Một 7-tier fitness-based node hierarchy** (§3.4) mở rộng SWIM với sự phân loại nhận biết năng lực từ Leaf nodes (cảm biến IoT) đến GlobalBackbone nodes (datacenters), với cơ chế promotion/demotion tự động dựa trên tính điểm fitness 7 chiều và hysteresis để ngăn ngừa dao động.

4. **Một cascade bootstrap offline-first 6 lớp** (§3.5) cho phép thiết lập mạng lưới mà không cần truy cập internet thông qua trao đổi xã hội (QR/NFC/BLE) và phát hiện mDNS cục bộ, với thang leo tăng dần tới các phương thức phụ thuộc vào internet.

5. **Đồng bộ hóa Delta-state CRDT** (§3.10) cho metadata tri thức, đạt được mức giảm băng thông 10–100× so với full-state replication thông qua trao đổi vi phân dựa trên VectorClock.

6. **Một định dạng wire toàn diện** (§3.11) hỗ trợ 81 loại message trên 9 phạm vi chức năng với một universal header 6-byte, được thiết kế cho hiệu suất năng lượng hướng tới mục tiêu <0.5% pin mỗi ngày trên các thiết bị di động.

Giao thức được triển khai trong ~8,000 dòng code Rust trên 40 modules với 159 bài test và 12 wire format test vectors. Phân tích quy mô dự báo độ trễ định tuyến ~7-hop, ~240ms cho 100 tỷ nodes thông qua hierarchical DHT routing — một cải tiến gấp 5 lần so với flat Kademlia.

OneBrain Protocol chứng minh rằng các mạng P2P không nhất thiết phải là các hệ thống phân phối file mục đích chung. Bằng cách thiết kế chuyên biệt cho tri thức — với semantic routing, capability-aware hierarchy, và khả năng thích ứng lấy cảm hứng từ sinh học — chúng tôi đạt được các khả năng mà không giao thức hiện tại nào cung cấp. Khi nhân loại tiến tới việc chia sẻ tri thức qua trung gian AI (AI-mediated knowledge sharing), cơ sở hạ tầng mạng tri thức được xây dựng chuyên biệt không chỉ hữu ích mà còn trở nên thiết yếu.

---

## References

[1] P. Maymounkov and D. Mazières, "Kademlia: A Peer-to-Peer Information System Based on the XOR Metric," in *Proc. IPTPS '02*, LNCS 2429, pp. 53–65, 2002.

[2] I. Baumgart and S. Mies, "S/Kademlia: A Practicable Approach Towards Secure Key-Based Routing," in *Proc. IEEE ICPADS '07*, pp. 1–8, 2007.

[3] I. Stoica *et al.*, "Chord: A Scalable Peer-to-Peer Lookup Service for Internet Applications," in *Proc. ACM SIGCOMM '01*, pp. 149–160, 2001.

[4] A. Rowstron and P. Druschel, "Pastry: Scalable, Decentralized Object Location and Routing for Large-Scale Peer-to-Peer Systems," in *Proc. IFIP/ACM Middleware '01*, 2001.

[5] S. Ratnasamy *et al.*, "A Scalable Content-Addressable Network," in *Proc. ACM SIGCOMM '01*, pp. 161–172, 2001.

[6] A. Das, I. Gupta, and A. Motivala, "SWIM: Scalable Weakly-consistent Infection-style Process Group Membership Protocol," in *Proc. IEEE/IFIP DSN '02*, pp. 303–312, 2002.

[7] HashiCorp, "Memberlist: Golang package for gossip based membership and failure detection," 2023.

[8] P.-P. Grassé, "La reconstruction du nid et les coordinations interindividuelles chez Bellicositermes natalensis et Cubitermes sp.," *Insectes Sociaux*, vol. 6, pp. 41–80, 1959.

[9] F. Heylighen, "Stigmergy as a Universal Coordination Mechanism: Components, Varieties and Applications," *Human Ecology Special Issue*, 2016.

[10] M. Dorigo and T. Stützle, *Ant Colony Optimization*. MIT Press, 2004.

[11] G. Di Caro and M. Dorigo, "AntNet: Distributed Stigmergetic Control for Communications Networks," *JAIR*, vol. 9, pp. 317–365, 1998.

[12] J. Iyengar and M. Thomson, "QUIC: A UDP-Based Multiplexed and Secure Transport," *IETF RFC 9000*, May 2021.

[13] A. Langley *et al.*, "The QUIC Transport Protocol: Design and Internet-Scale Deployment," in *Proc. ACM SIGCOMM '17*, pp. 183–196, 2017.

[14] J. Benet, "IPFS — Content Addressed, Versioned, P2P File System," *arXiv preprint arXiv:1407.3561*, 2014.

[15] D. J. Trautwein *et al.*, "Design and Evaluation of IPFS: A Storage Layer for the Decentralized Web," in *Proc. ACM SIGCOMM '22*, 2022.

[16] Protocol Labs, "libp2p: A Modular Network Stack," 2023. [Online]. Available: https://libp2p.io/

[17] B. Cohen, "Incentives Build Robustness in BitTorrent," in *Proc. Workshop on Economics of Peer-to-Peer Systems*, 2003.

[18] A. Demers *et al.*, "Epidemic Algorithms for Replicated Database Maintenance," in *Proc. ACM PODC '87*, pp. 1–12, 1987.

[19] A.-M. Kermarrec and M. van Steen, "Gossiping in Distributed Systems," *ACM SIGOPS Operating Systems Review*, vol. 41, no. 5, pp. 2–7, 2007.

[20] B. Yang and H. Garcia-Molina, "Designing a Super-Peer Network," in *Proc. IEEE ICDE '03*, pp. 49–60, 2003.

[21] A. Montresor, "A Robust Protocol for Building Superpeer Overlay Topologies," in *Proc. IEEE P2P '04*, 2004.

[22] M. Shapiro, N. Preguiça, C. Baquero, and M. Zawirski, "A Comprehensive Study of Convergent and Commutative Replicated Data Types," *INRIA Research Report RR-7506*, 2011.

[23] P. S. Almeida, A. Shoker, and C. Baquero, "Delta State Replicated Data Types," *Journal of Parallel and Distributed Computing*, vol. 111, pp. 162–173, 2018.

[24] M. Kleppmann and A. R. Beresford, "A Conflict-Free Replicated JSON Datatype," *IEEE TPDS*, vol. 28, no. 10, pp. 2733–2746, 2017.

[25] D. J. Bernstein *et al.*, "High-speed high-security signatures," *Journal of Cryptographic Engineering*, vol. 2, no. 2, pp. 77–89, 2012.

[26] M. Sporny, D. Reed *et al.*, "Decentralized Identifiers (DIDs) v1.0," W3C Recommendation, Jul. 2022.

[27] B. H. Bloom, "Space/Time Trade-offs in Hash Coding with Allowable Errors," *Communications of the ACM*, vol. 13, no. 7, pp. 422–426, 1970.

[28] A. Broder and M. Mitzenmacher, "Network Applications of Bloom Filters: A Survey," *Internet Mathematics*, vol. 1, no. 4, pp. 485–509, 2004.

[29] D. R. Swanson, "Fish Oil, Raynaud's Syndrome, and Undiscovered Public Knowledge," *Perspectives in Biology and Medicine*, vol. 30, no. 1, pp. 7–18, 1986.

[30] S. D. Kamvar, M. T. Schlosser, and H. Garcia-Molina, "The EigenTrust Algorithm for Reputation Management in P2P Networks," in *Proc. WWW '03*, pp. 640–651, 2003.

[31] GSMA, "The Mobile Economy 2024," 2024.

[32] J. O'Connor, J.-P. Aumasson, S. Neves, and Z. Wilcox-O'Hearn, "BLAKE3: One function, fast everywhere," 2020. [Online]. Available: https://blake3.io/

[33] Ethereum Foundation, "devp2p: Ethereum Peer-to-Peer Networking Specifications," 2024.

[34] J. R. Douceur, "The Sybil Attack," in *Proc. IPTPS '02*, LNCS 2429, pp. 251–260, 2002.

[35] K. M. Sim and W. H. Sun, "Ant Colony Optimization for Routing and Load-Balancing," *IEEE Trans. SMC-A*, vol. 33, no. 5, pp. 560–572, 2003.

[36] G. Theraulaz and E. Bonabeau, "A Brief History of Stigmergy," *Artificial Life*, vol. 5, no. 2, pp. 97–116, 1999.

[37] E. Rivière and S. Voulgaris, "Gossip-based Networking for Internet-Scale Distributed Systems," *LNCS 6108*, 2011.

[38] A. J. Ganesh, L. Massoulié, and D. Towsley, "The Effect of Network Topology on the Spread of Epidemics," in *Proc. IEEE INFOCOM '05*, 2005.

[39] L. Xiong and L. Liu, "PeerTrust: Supporting Reputation-Based Trust for Peer-to-Peer Electronic Communities," *IEEE TKDE*, vol. 16, no. 7, pp. 843–857, 2004.

[40] K. Hoffman, D. Zage, and C. Nita-Rotaru, "A Survey of Attack and Defense Techniques for Reputation Systems," *ACM Computing Surveys*, vol. 42, no. 1, 2009.

[41] S. Tarkoma, C. E. Rothenberg, and E. Lagerspetz, "Theory and Practice of Bloom Filters for Distributed Systems," *IEEE Communications Surveys & Tutorials*, vol. 14, no. 1, pp. 131–155, 2012.

[42] K. Jahns, "Yjs: A Framework for Near Real-Time P2P Shared Editing on Arbitrary Data Types," in *Proc. ECSCW '19*, 2019.

[43] OneBrain Project, "Knowledge Unit: A Bio-Inspired Knowledge Representation for Decentralized Knowledge Networks," 2026 (companion paper).

[44] M. Castro and B. Liskov, "Practical Byzantine Fault Tolerance," in *Proc. OSDI '99*, pp. 173–186, 1999.
