# 2. Related Work

Phần này khảo sát tám lĩnh vực nghiên cứu trước đây làm cơ sở cho thiết kế của OneBrain Protocol, xác định các hạn chế của chúng đối với các ứng dụng chia sẻ tri thức, và định vị các đóng góp của OBP so với state of the art.

## 2.1 Distributed Hash Tables

Distributed Hash Tables (DHTs) cung cấp khả năng tìm kiếm key-value phi tập trung trong O(log N) hops cho N nodes. Bốn kiến trúc DHT nền tảng đã xuất hiện đồng thời trong giai đoạn 2001–2002:

**Chord** [1] tổ chức các nodes trên một circular identifier space với các finger tables cung cấp routing O(log N). Mỗi node duy trì log N entries trỏ tới các successors ở khoảng cách tăng dần theo cấp số nhân. Cấu trúc ring topology của Chord cho phép suy luận đơn giản về trách nhiệm nhưng yêu cầu các stabilization protocols để xử lý churn.

**Pastry** [2] sử dụng sơ đồ prefix-based routing trong đó các nodes chia sẻ các key prefixes dài dần. Pastry khai thác network proximity trong các quyết định routing của mình, đạt được độ trễ thấp hơn so với các topology-unaware DHTs. Tuy nhiên, routing state của nó tăng theo độ dài khóa (key length) và yêu cầu các phép đo lường mạng (network measurements) tốn kém.

**CAN** [3] phân chia một không gian tọa độ Descartes d chiều (d-dimensional Cartesian coordinate space) giữa các nodes. Mỗi node sở hữu một zone và duy trì neighbor state cho 2d neighbors. CAN đạt được routing O(d × N^{1/d}), mang lại sự đánh đổi về khả năng mở rộng (scalability) khác với các ring-based DHTs.

**Kademlia** [4] giới thiệu XOR distance metric $d(a,b) = a \oplus b$, có tính đối xứng, giúp cho routing vốn dĩ mang tính hai chiều (bidirectional). Các nodes duy trì 160 (hoặc 256) k-buckets, mỗi bucket chứa tối đa k entries cho các peers ở một phạm vi XOR distance cụ thể. Các cải tiến cốt lõi của Kademlia bao gồm: (1) iterative parallel lookups với α concurrent RPCs; (2) cơ chế tự tổ chức tự nhiên trong quá trình lookups (mọi tương tác đều cập nhật routing tables); (3) khả năng mở rộng thực tế đã được chứng minh trong Mainline DHT của BitTorrent (hàng triệu nodes).

**S/Kademlia** [5] mở rộng Kademlia với cryptographic node ID generation (để ngăn chặn việc tự do lựa chọn IDs) và β disjoint lookup paths nhằm tăng khả năng kháng Byzantine. Với β=3, hệ thống chịu đựng được lên tới 20% malicious nodes với tỷ lệ lookup thành công đạt 92%.

**Lựa chọn DHT của OneBrain.** OBP triển khai S/Kademlia với 256 k-buckets (khớp với không gian NodeId 256-bit), k=20, α=3 (lookup parallelism), và β=3 (disjoint paths). Kademlia được lựa chọn thay vì Chord/Pastry/CAN vì ba lý do: (1) tính đối xứng của XOR metric giúp giảm overhead bảo trì; (2) iterative lookups cho phép hoạt động song song, không đồng bộ (parallel, asynchronous operation); (3) việc triển khai thực tế đã được chứng minh ở quy mô lớn trong BitTorrent, Ethereum, và IPFS. Cryptographic puzzle dành cho việc tạo NodeId (§3.2) triển khai cơ chế bảo vệ danh tính (identity protection) của S/Kademlia.

| Đặc tính | Chord | Pastry | CAN | Kademlia | OneBrain |
|---------|-------|--------|-----|----------|----------|
| Topology | Ring | Prefix tree | Không gian d chiều (d-dim space) | XOR tree | XOR tree |
| Routing | O(log N) | O(log N) | O(dN^{1/d}) | O(log N) | O(log N) |
| Symmetry | Không | Không | Có | Có | Có |
| Proximity | Không | Có | Có | Không | Qua tier |
| Byzantine | Không | Không | Không | S/Kademlia | β=3 paths |
| Thực tế | Hạn chế | Hạn chế | Không | BitTorrent, IPFS | — |

*Table 1: Comparison of DHT architectures.*

## 2.2 Membership Protocols

**SWIM** [6] (Scalable Weakly-consistent Infection-style Process Group Membership Protocol) tách biệt việc phát hiện lỗi (failure detection) khỏi việc phổ biến thông tin (information dissemination). Mỗi node định kỳ chọn một peer ngẫu nhiên để thực hiện probing trực tiếp; nếu probe thất bại, nó sẽ yêu cầu K lượt indirect probes thông qua các nodes khác trước khi đưa vào trạng thái nghi ngờ (suspicion). Các bản cập nhật membership được piggyback trên các protocol messages, đạt được O(1) message overhead cho mỗi member trong mỗi chu kỳ giao thức. SWIM cung cấp tính toàn vẹn completeness (tất cả các lỗi cuối cùng đều được phát hiện) và O(log N) infection time cho N nodes.

**HashiCorp Memberlist** [7] là một bản triển khai Go cấp độ production của SWIM với các mở rộng Lifeguard, được sử dụng trong Consul, Serf, và Nomad. Lifeguard bổ sung: (1) Local Health Awareness (LHA) — các nodes khi phát hiện sức khỏe của chính mình bị suy giảm sẽ tăng suspect timeouts để giảm false positives; (2) suspicion sub-protocol với cấu hình ngưỡng xác nhận (confirmation threshold).

**Akka Cluster** [8] triển khai một failure detector lấy cảm hứng từ SWIM dành cho các hệ thống phân tán dựa trên JVM với phi-accrual failure detection, cung cấp một suspicion level liên tục thay vì phân loại nhị phân alive/dead.

**Đóng góp của OneBrain.** OBP mở rộng SWIM với một **7-tier node hierarchy** — giao thức membership đầu tiên triển khai phân loại node nhận biết năng lực (capability-aware), phân tầng theo địa lý (geographically-stratified) với cơ chế promotion/demotion tự động dựa trên fitness. SWIM truyền thống đối xử với tất cả các nodes như các peers bình đẳng; hierarchy của OBP khai thác sự dị thể (heterogeneity) cơ bản của các thành viên tham gia mạng lưới tri thức (smartphones ≠ laptops ≠ servers ≠ datacenters). Điểm fitness tổng hợp 7 chiều có trọng số (uptime, battery, bandwidth, storage, CPU, network quality, reputation) và các bước chuyển tier (tier transitions) bao gồm cả độ trễ trễ (hysteresis) (0.05) để ngăn ngừa hiện tượng dao động (oscillation) giữa các tiers.

## 2.3 Bio-Inspired Network Routing

**Stigmergy** lần đầu tiên được mô tả bởi Grassé [9] trong nghiên cứu của ông về việc xây tổ của mối: côn trùng phối hợp thông qua các dấu vết môi trường (pheromones) thay vì giao tiếp trực tiếp. Heylighen [10] đã tổng quát hóa stigmergy như một cơ chế điều phối phổ quát có thể áp dụng cho tính toán, các hệ thống xã hội và trí tuệ nhân tạo.

**Ant Colony Optimization (ACO)** [11] hình thức hóa khái niệm stigmergy thành một khung tối ưu hóa metaheuristic (metaheuristic optimization framework). Các kiến nhân tạo di chuyển qua một đồ thị, để lại pheromone trên các cạnh tỷ lệ thuận với chất lượng giải pháp. Pheromone bay hơi (evaporate) theo thời gian, ngăn chặn sự hội tụ vào các giải pháp dưới mức tối ưu (suboptimal solutions). ACO đã được áp dụng thành công cho bài toán người bán hàng (traveling salesman problem), định tuyến phương tiện (vehicle routing) và lập lịch (scheduling).

**AntNet** [12] áp dụng ACO vào adaptive routing trong các mạng viễn thông. Forward ants khám phá mạng lưới trong khi backward ants củng cố (reinforce) các con đường thành công. AntNet đã chứng minh hiệu suất cạnh tranh với các thuật toán định tuyến ngắn nhất truyền thống (OSPF, RIP) đồng thời cung cấp khả năng thích ứng vượt trội đối với các điều kiện mạng thay đổi.

**Di Caro và Dorigo** [12] đã chỉ ra rằng stigmergetic routing đạt được độ trễ trung bình thấp hơn 5–15% so với OSPF dưới các mô hình lưu lượng động (dynamic traffic patterns), với tốc độ hội tụ nhanh hơn đáng kể sau các thay đổi về topology.

**Cải tiến của OneBrain.** OBP áp dụng stigmergy vào **knowledge query routing** — một ứng dụng mới lạ chưa từng xuất hiện trong các tài liệu mạng ACO trước đây. Không giống như AntNet (định tuyến các gói tin giữa các cặp source-destination đã biết), OBP định tuyến các truy vấn ngữ nghĩa (semantic queries) đến các nodes có chuyên môn (expertise) *chưa biết*. Các pheromone trails không mã hóa "đường nào dẫn tới Node X" mà mã hóa "đường nào đã trả lời thành công các câu hỏi về Topic Y." Đây là một bài toán routing khác biệt căn bản: đích đến không phải là một địa chỉ mà là một năng lực (capability). Các động lực reinforcement/evaporation (§3.7) đảm bảo rằng mạng lưới tự tối ưu hóa cấu trúc routing topology của nó để phù hợp với các mô hình nhu cầu tri thức liên tục tiến hóa.

## 2.4 Transport Protocols for P2P

**QUIC** (RFC 9000) [13] là một UDP-based multiplexed transport protocol với mã hóa TLS 1.3 tích hợp sẵn. Langley và các cộng sự [14] đã báo cáo việc Google triển khai QUIC trên 75% lưu lượng Chrome, chứng minh: (1) thiết lập kết nối 0-RTT cho các kết nối lặp lại; (2) loại bỏ hiện tượng nghẽn đầu hàng (head-of-line blocking) thông qua việc stream multiplexing độc lập; (3) connection migration khi thay đổi mạng (ví dụ: từ WiFi sang di động); (4) mã hóa tích hợp sẵn loại bỏ overhead của TLS handshake.

Đối với các ứng dụng P2P, QUIC cung cấp các lợi thế cụ thể so với TCP: (1) NAT traversal đơn giản hơn với UDP; (2) các multiplexed streams cho phép các request/response đồng thời mà không cần tạo kết nối mới; (3) 0-RTT giảm đáng kể độ trễ cho các peers thường xuyên giao tiếp; (4) connection migration xử lý việc các mobile nodes thay đổi mạng lưới liên tục.

**Cách sử dụng của OneBrain.** OBP sử dụng QUIC như **sole transport** duy nhất (thông qua crate Rust quinn), biến nó trở thành một trong số ít hệ thống tri thức P2P được xây dựng native trên QUIC thay vì lắp ghép thêm vào các kiến trúc dựa trên TCP. Giao thức phân biệt rõ các message an toàn với 0-RTT-safe (idempotent: SWIM PING, FIND_NODE, BLOOM_FILTER) khỏi các message yêu cầu 1-RTT-required (non-idempotent: KU_PUSH, QUERY).

## 2.5 Content-Addressed P2P Systems

**IPFS** [15] triển khai một content-addressed storage layer kết hợp một Kademlia DHT, một giao thức trao đổi dữ liệu Bitswap, và một cấu trúc dữ liệu Merkle DAG. Trautwein và các cộng sự [16] đã thực hiện nghiên cứu đo lường quy mô lớn đầu tiên về IPFS, tìm thấy ~50K hoạt động DHT nodes nhưng có sự tập trung hóa (centralization) đáng kể hướng về các nhà vận hành gateway (Cloudflare, Pinata).

**libp2p** [17] là chồng giao thức mạng mô-đun (modular networking stack) được trích xuất từ IPFS, cung cấp các composable protocols cho transport, discovery, routing, và multiplexing. libp2p hỗ trợ các transport TCP, QUIC, WebTransport, và WebSocket, với Kademlia và GossipSub là các giao thức chính.

**Các hạn chế chính đối với việc chia sẻ tri thức:**
- Không có semantic routing: các truy vấn phải chỉ định chính xác các content hashes
- Không có reputation tích hợp sẵn: tất cả các nodes được đối xử bình đẳng bất kể hành vi
- Không có các kiểu dữ liệu chuyên biệt cho tri thức (knowledge-specific data types): tất cả nội dung là các opaque blocks
- Cấu trúc flat node topology phẳng: không có hierarchy nhận biết năng lực (capability-aware)
- Phụ thuộc vào internet: yêu cầu các bootstrap nodes cho việc phát hiện ban đầu (initial discovery)
- Xu hướng tập trung hóa (centralization trend): các nhà vận hành gateway lớn tập trung hóa quyền truy cập [16]

## 2.6 Gossip Protocols và Epidemic Dissemination

**Demers và các cộng sự** [18] đã giới thiệu các thuật toán dịch bệnh (epidemic algorithms) cho việc duy trì cơ sở dữ liệu nhân bản (replicated database maintenance), chứng minh rằng rumor-spreading đạt được O(log N) dissemination time với O(N log N) tổng số tin nhắn. Ba biến thể — direct mail, anti-entropy, và rumor mongering — cung cấp các sự đánh đổi khác nhau giữa tính nhất quán (consistency) và băng thông (bandwidth).

**Kermarrec và van Steen** [19] khảo sát các gossip protocols trong các hệ thống phân tán, xác định các thuộc tính chính: các cam kết xác suất (probabilistic guarantees), khả năng mở rộng (scalability), tính đơn giản và khả năng chống chịu lỗi (robustness to failures).

**Cách sử dụng của OneBrain.** OBP sử dụng gossip để lan truyền dữ liệu metabolism (wire types 0x86/0x87/0x89), trong đó các số liệu thống kê sử dụng dựa trên GCounter được piggyback trên các message của giao thức SWIM. Điều này giúp đạt được mức zero additional message overhead cho gossip — một tối ưu hóa năng lượng quan trọng đối với các mobile nodes.

## 2.7 Super-Peer và Hierarchical Overlays

**Yang và Garcia-Molina** [20] phân tích thiết kế mạng super-peer cho việc chia sẻ file P2P, chỉ ra rằng cấu trúc 2-tier topology (ordinary peers + super-peers) giúp giảm chi phí tìm kiếm bằng cách tập trung trách nhiệm routing trên các nodes có kết nối tốt.

**Montresor** [21] đề xuất các giao thức để xây dựng các cấu trúc super-peer overlay topologies mạnh mẽ với cơ chế tự động bầu chọn super-peer (automatic super-peer election) dựa trên năng lực của node.

Các mô hình super-peer truyền thống sử dụng một static 2-tier hierarchy: ordinary peers kết nối với super-peers, các nodes chịu trách nhiệm xử lý routing liên peer. Phân loại nhị phân này thất bại trong việc khai thác toàn bộ phổ năng lực thiết bị trong một mạng lưới tri thức.

**Đóng góp của OneBrain.** OBP giới thiệu một **7-tier hierarchy** (Leaf → Contributor → LocalSP → RegionalSP → CountrySP → ContinentalSP → GlobalBackbone) với tính thân thuộc địa lý (geographic affinity) và cơ chế promotion/demotion tự động dựa trên fitness. Đây là một mở rộng đáng kể vượt trội hơn các mô hình super-peer 2 tầng: (1) 7 tiers giúp khai thác năng lực một cách chi tiết (granular capability exploitation); (2) phân tầng địa lý (geographic stratification) giảm thiểu việc routing xuyên lục địa; (3) tính điểm fitness giúp tự động hóa quá trình chuyển đổi tầng (tier transitions) mà không cần cấu hình thủ công; (4) cơ chế hysteresis ngăn ngừa hiện tượng dao động (oscillation) giữa các tiers.

## 2.8 CRDTs trong các Hệ thống P2P

**Shapiro và các cộng sự** [22] đã hình thức hóa các Conflict-free Replicated Data Types (CRDTs), chứng minh rằng các state-based CRDTs (CvRDTs) trên các join semi-lattices đảm bảo tính Nhất quán Cuối cùng Mạnh mẽ (Strong Eventual Consistency - SEC) mà không cần điều phối. Năm kiểu dữ liệu cơ bản — GCounter, PNCounter, LWWRegister, ORSet, và VectorClock — cung cấp các khối xây dựng (building blocks) cho các cấu trúc dữ liệu phân tán.

**Almeida, Shoker, và Baquero** [23] đã giới thiệu **delta-state CRDTs**, loại chỉ truyền đi các thay đổi trạng thái (deltas) kể từ điểm đồng bộ hóa gần nhất. Delta-state CRDTs đạt được hiệu quả băng thông của các operation-based CRDTs trong khi vẫn giữ nguyên tính mạnh mẽ của các state-based CRDTs.

**Automerge** [24] và **Yjs** [25] là các thư viện CRDT thương mại cho việc chỉnh sửa tài liệu cộng tác, chứng minh tính khả thi của CRDTs đối với đồng bộ hóa thời gian thực (real-time synchronization).

**Đóng góp của OneBrain.** OBP áp dụng delta-state CRDT synchronization vào **knowledge metadata và trust scores** — một domain ứng dụng mới lạ. Trong khi Automerge/Yjs nhắm tới việc chỉnh sửa tài liệu, OBP đồng bộ hóa các GCounters (usage counts), LWWRegisters (epistemic status), ORSets (domain codes), và VectorClocks (causal ordering) trên một P2P knowledge network. Lớp sync layer (§3.10) đạt được mức giảm băng thông từ 10–100× so với full-state replication, điều thiết yếu cho các mobile nodes bị hạn chế băng thông.

## 2.9 Tóm tắt và Định vị

Bảng 2 trình bày so sánh toàn diện của OneBrain Protocol với các hệ thống hiện tại có liên quan nhất.

| Đặc tính | IPFS/libp2p | BitTorrent | Ethereum devp2p | OneBrain OBP |
|---------|-------------|------------|-----------------|--------------|
| **Purpose** | File storage | File sharing | Tx/block propagation | Knowledge sharing |
| **Protocol layers** | ~5 (modular) | 3 | 4 | 9 (tích hợp/integrated) |
| **Transport** | TCP, QUIC, WebTransport | TCP, uTP | TCP, devp2p | QUIC (native) |
| **DHT** | Kademlia (amino) | Mainline DHT | Kademlia (discv5) | S/Kademlia (k=20, β=3) |
| **Membership** | Random walks | — | — | SWIM + 7-tier hierarchy |
| **Content routing** | DHT + Bitswap | Tracker + DHT | — | DHT + Stigmergy + Bloom |
| **Bio-inspired** | Không | Không | Không | Có (stigmergy, fitness) |
| **Node hierarchy** | Flat | Flat | Flat | 7 tiers (tự động promote) |
| **Sync mechanism** | Bitswap (want/have) | Piece requests | Block sync | Delta-state CRDT |
| **Reputation** | Không tích hợp sẵn | Không | Không | EigenTrust + PoMV |
| **Offline-first** | Không (yêu cầu bootstrap) | Không | Không | Có (BLE/WiFi mesh) |
| **Mobile target** | Không | Không | Không | <0.5% pin/ngày |
| **Data unit** | Content blocks | File pieces | Transactions/blocks | Knowledge Units |
| **Message types** | ~20 | ~10 | ~15 | 74 |
| **Scale target** | Hàng triệu (Millions) | Hàng triệu (Millions) | ~8K full nodes | 100 tỷ (100 billion) |
| **Wire format** | Protobuf/CBOR | Bencode | RLP | 6B header + Core DNA + CRC-16 |

*Table 2: Comprehensive comparison of P2P protocol architectures.*

Kết quả so sánh cho thấy không có giao thức P2P hiện tại nào kết hợp được các yếu tố semantic routing, capability-aware hierarchy, thiết kế offline-first, đồng bộ hóa tri thức dựa trên CRDT, và tối ưu hóa định tuyến lấy cảm hứng từ sinh học (bio-inspired routing optimization). Giao thức OneBrain Protocol giải quyết khoảng trống này bằng kiến trúc 9 lớp được xây dựng chuyên biệt, coi tri thức như một thực thể hạng nhất (first-class citizen) thay vì một opaque blob mờ đục.

---

## References

[1] I. Stoica *et al.*, "Chord: A Scalable Peer-to-Peer Lookup Service for Internet Applications," in *Proc. ACM SIGCOMM '01*, pp. 149–160, 2001.

[2] A. Rowstron and P. Druschel, "Pastry: Scalable, Decentralized Object Location and Routing for Large-Scale Peer-to-Peer Systems," in *Proc. IFIP/ACM Middleware '01*, LNCS 2218, pp. 329–350, 2001.

[3] S. Ratnasamy *et al.*, "A Scalable Content-Addressable Network," in *Proc. ACM SIGCOMM '01*, pp. 161–172, 2001.

[4] P. Maymounkov and D. Mazières, "Kademlia: A Peer-to-Peer Information System Based on the XOR Metric," in *Proc. IPTPS '02*, LNCS 2429, pp. 53–65, 2002.

[5] I. Baumgart and S. Mies, "S/Kademlia: A Practicable Approach Towards Secure Key-Based Routing," in *Proc. IEEE ICPADS '07*, pp. 1–8, 2007.

[6] A. Das, I. Gupta, and A. Motivala, "SWIM: Scalable Weakly-consistent Infection-style Process Group Membership Protocol," in *Proc. IEEE/IFIP DSN '02*, pp. 303–312, 2002.

[7] HashiCorp, "Memberlist: Golang package for gossip based membership and failure detection," 2023. [Online]. Available: https://github.com/hashicorp/memberlist

[8] Lightbend, "Akka Cluster Specification," 2023. [Online]. Available: https://doc.akka.io/docs/akka/current/typed/cluster.html

[9] P.-P. Grassé, "La reconstruction du nid et les coordinations interindividuelles chez Bellicositermes natalensis et Cubitermes sp.," *Insectes Sociaux*, vol. 6, pp. 41–80, 1959.

[10] F. Heylighen, "Stigmergy as a Universal Coordination Mechanism: Components, Varieties and Applications," *Human Ecology Special Issue*, 2016.

[11] M. Dorigo and T. Stützle, *Ant Colony Optimization*. MIT Press, 2004.

[12] G. Di Caro and M. Dorigo, "AntNet: Distributed Stigmergetic Control for Communications Networks," *Journal of Artificial Intelligence Research*, vol. 9, pp. 317–365, 1998.

[13] J. Iyengar and M. Thomson, "QUIC: A UDP-Based Multiplexed and Secure Transport," *IETF RFC 9000*, May 2021.

[14] A. Langley *et al.*, "The QUIC Transport Protocol: Design and Internet-Scale Deployment," in *Proc. ACM SIGCOMM '17*, pp. 183–196, 2017.

[15] J. Benet, "IPFS — Content Addressed, Versioned, P2P File System," *arXiv preprint arXiv:1407.3561*, 2014.

[16] D. J. Trautwein *et al.*, "Design and Evaluation of IPFS: A Storage Layer for the Decentralized Web," in *Proc. ACM SIGCOMM '22*, 2022.

[17] Protocol Labs, "libp2p: A Modular Network Stack," 2023. [Online]. Available: https://libp2p.io/

[18] A. Demers *et al.*, "Epidemic Algorithms for Replicated Database Maintenance," in *Proc. ACM PODC '87*, pp. 1–12, 1987.

[19] A.-M. Kermarrec and M. van Steen, "Gossiping in Distributed Systems," *ACM SIGOPS Operating Systems Review*, vol. 41, no. 5, pp. 2–7, 2007.

[20] B. Yang and H. Garcia-Molina, "Designing a Super-Peer Network," in *Proc. IEEE ICDE '03*, pp. 49–60, 2003.

[21] A. Montresor, "A Robust Protocol for Building Superpeer Overlay Topologies," in *Proc. IEEE P2P '04*, pp. 202–209, 2004.

[22] M. Shapiro, N. Preguiça, C. Baquero, and M. Zawirski, "A Comprehensive Study of Convergent and Commutative Replicated Data Types," *INRIA Research Report RR-7506*, 2011.

[23] P. S. Almeida, A. Shoker, and C. Baquero, "Delta State Replicated Data Types," *Journal of Parallel and Distributed Computing*, vol. 111, pp. 162–173, 2018.

[24] M. Kleppmann and A. R. Beresford, "A Conflict-Free Replicated JSON Datatype," *IEEE Transactions on Parallel and Distributed Systems*, vol. 28, no. 10, pp. 2733–2746, 2017.

[25] K. Jahns, "Yjs: A Framework for Near Real-Time P2P Shared Editing on Arbitrary Data Types," in *Proc. ECSCW '19*, 2019.
