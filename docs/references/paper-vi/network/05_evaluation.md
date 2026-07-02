# 5. Implementation and Evaluation

## 5.1 Tổng quan Triển khai

OneBrain Protocol được triển khai bằng Rust, tận dụng các đảm bảo an toàn bộ nhớ (memory safety guarantees), các trừu tượng hóa chi phí bằng không (zero-cost abstractions) của ngôn ngữ, và async runtime tokio hoàn chỉnh cho các hoạt động mạng đồng thời. Bản triển khai trải dài trên hai nhóm chức năng: chồng giao thức cốt lõi (core protocol stack) và distributed query engine.

### 5.1.1 Các Module Giao thức Cốt lõi (Core Protocol Modules)

| Module | LOC | Lớp (Layer) | Mục đích (Purpose) |
|--------|----:|:-----:|---------|
| `identity.rs` | 245 | L0 | Ed25519 keypairs, BLAKE3 NodeId, crypto puzzle, DID |
| `transport.rs` | 457 | L1 | QUIC transport qua quinn, self-signed certs |
| `messages.rs` | 471 | — | 74 loại message, header 6-byte, các compression modes |
| `membership.rs` | 408 | L2 | Giao thức SWIM, 7-tier hierarchy, tính điểm fitness |
| `discovery.rs` | 309 | L3 | Cascade bootstrap 6 lớp (6-layer bootstrap cascade), PEX |
| `dht.rs` | 624 | L4 | S/Kademlia routing table, k-buckets, store/find |
| `stigmergy.rs` | 302 | L5 | Pheromone routing table, reinforce/evaporate |
| `vacuum.rs` | 314 | L6 | Bloom filter content routing |
| `pubsub.rs` | 269 | L7 | Topic subscriptions, interest vectors |
| `sync.rs` | 383 | L8 | Delta-state CRDT sync với VectorClock |
| `metabolism_gossip.rs` | 325 | — | PoMV gossip handler, CRDT merge |
| `error.rs` | 178 | — | Cấu trúc phân tầng lỗi 5 cấp (5-level error hierarchy) |
| `constants.rs` | 116 | — | Đăng ký hằng số hoàn chỉnh (Complete constant registry) |
| **Tổng cộng** | **4,377** | | |

*Table 11: Core protocol modules.*

### 5.1.2 Các Module Distributed Query Engine

| Module | LOC | Mục đích (Purpose) |
|--------|----:|---------|
| `index.rs` | 209 | ConceptIndex + VacuumFilter, DHT publishing |
| `router.rs` | 479 | Thang leo phạm vi 6 lớp (6-layer scope escalation) |
| `merger.rs` | 206 | Loại bỏ trùng lặp (Deduplication) + xếp hạng trust×scope |
| `watch.rs` | 478 | Standing queries + event filters |
| `cache.rs` | 253 | LRU query cache, các khóa BLAKE3 |
| `learning.rs` | 264 | Học tập củng cố pheromone (Pheromone reinforcement learning) |
| `gaps.rs` | 230 | Bộ phát hiện khoảng trống tri thức (Knowledge gap detector) |
| `bridges.rs` | 240 | Bộ tìm cầu nối liên miền Swanson ABC (Swanson ABC cross-domain bridge finder) |
| `serendipity.rs` | 230 | Engine của các ẩn số chưa biết (Unknown unknowns engine) |
| `encoding_gossip.rs` | 236 | Xử lý message Encoding consensus |
| `encoding_stigmergy.rs` | 227 | Cân bằng tải công việc Encoding (Encoding job load balancing) |
| `encoding_job.rs` | 198 | Quản lý vòng đời công việc Encoding (Encoding job lifecycle management) |
| **Tổng cộng** | **2,589** | |

*Table 12: Distributed query engine modules.*

**Tổng số dòng code kết hợp:** ~8,000 dòng code Rust trên 30 modules.

### 5.1.3 Các Dependency

| Crate | Phiên bản (Version) | Mục đích (Purpose) |
|-------|---------|---------|
| `blake3` | latest | BLAKE3 hashing (NodeId, CID, Vacuum) |
| `ed25519-dalek` | latest | Chữ ký Ed25519 |
| `quinn` | latest | QUIC transport (feature-gated) |
| `rustls` | latest | TLS 1.3 cho QUIC |
| `rcgen` | latest | Tạo chứng chỉ self-signed certificate |
| `tokio` | latest | Async runtime |
| `serde` + `ciborium` | latest | Tuần tự hóa CBOR (CBOR serialization) |
| `ku-core` | local | Knowledge Unit codec |

## 5.2 Độ phủ Kiểm thử (Test Coverage)

Bản triển khai bao gồm **159 bài test** thuộc 12 danh mục, cộng với **12 test vectors định dạng wire (wire format test vectors)** để xác minh khả năng tương tác.

### 5.2.1 Các Bài Unit Test

| Danh mục (Category) | Số test (Tests) | Các kịch bản chính (Key Scenarios) |
|----------|:-----:|---------------|
| Identity | 12 | Tạo puzzle (difficulty 16), xác minh (valid/invalid nonce/pubkey), XOR distance, leading zeros, sign/verify, bounded timeout, device ID, định dạng DID |
| Messages | 7 | Header roundtrip, tất cả 74 types đều có IDs duy nhất, phạm vi type, IPv4/IPv6 address roundtrip, loại địa chỉ không hợp lệ, phân loại an toàn 0-RTT |
| Membership | 8 | Tính điểm fitness, các ngưỡng promotion tầng, demotion hysteresis, chuyển đổi máy trạng thái, phản bác nghi ngờ (refute suspicion), xử lý ping, rời khỏi mạng duyên dáng (graceful departure), định dạng wire |
| Discovery | 4 | Bootstrap state machine, all-layers-fail fallback, phân loại yêu cầu internet, lựa chọn PEX peer theo fitness |
| DHT | 12 | k-bucket insert/update/full/remove/stale-eviction, routing table bucket_index/insert/reject_self/find_closest, DhtNode store/get/find_value |
| Stigmergy | 7 | Reinforce new hop, củng cố (reinforce) làm tăng strength, thất bại làm giảm strength, lựa chọn best_next_hop, route_query loại trừ các nodes đã truy cập, evaporate làm giảm strength, evaporate loại bỏ các hops đã chết |
| Vacuum | 6 | Insert/contains, không có âm tính giả (no false negatives), FPR nằm trong giới hạn, encode/decode roundtrip, gộp hai filters, xác thực kích thước wire |
| PubSub | 6 | Subscribe/unsubscribe, interest vector khác không, phát hiện interest overlap, thêm/tìm subscribers, xóa node, không trùng lặp |
| Sync | 6 | Lưu trữ ticks clock cục bộ, sync request/response, sync tăng dần, sync hai chiều, idempotent merge, thông số peer |
| Metabolism | 6 | Xử lý gộp cập nhật, chu kỳ query/response, chuẩn bị cập nhật, cập nhật idempotent, các message types, max deltas bị giới hạn |
| **Total** | **74** | |

### 5.2.2 Các Bài Integration Test

| Bài test (Test) | Kịch bản (Scenario) | Các thuộc tính được xác minh (Validated Properties) |
|------|----------|---------------------|
| `e2e_3_nodes_ku_transfer` | Mạng 3 node, KU được gửi từ A→B→C | End-to-end delivery, routing |
| `e2e_bootstrap_to_connected` | Node tham gia qua bootstrap cascade | Discovery, membership join |
| `e2e_signed_frame_tamper_detection` | Sửa đổi signed frame, xác minh sự từ chối | Tính toàn vẹn, xác minh Ed25519 |
| `e2e_cid_deterministic` | Cùng một KU tạo ra cùng một CID trên tất cả các nodes | Tính xác định của BLAKE3 |
| `e2e_xor_routing_closest_node` | Truy vấn được route đến node gần nhất theo XOR | Tính chính xác của DHT routing |
| `e2e_membership_3_nodes_with_tiers` | 3 nodes với fitness khác nhau tham gia | Phân bổ tầng (tier assignment), promotion |
| `e2e_mixed_address_network` | Các nodes IPv4 và IPv6 tương tác hoạt động chéo | Tương thích dual-stack |
| `e2e_full_pipeline` | Đường ống pipeline hoàn chỉnh: tạo→push→truy vấn→xác minh | Tích hợp toàn bộ stack (Full stack integration) |

### 5.2.3 Các Wire Format Test Vector (TV-1 đến TV-12)

| TV | Đầu vào (Input) | Kết quả dự kiến (Expected Output) | Xác minh (Validates) |
|:--:|-------|-----------------|-----------|
| 1 | KU_PUSH, 264 bytes | `[0x01, 0x00, 0x00, 0x00, 0x01, 0x08]` | Mã hóa Header (6-byte) |
| 2 | SWIM_PING, empty | `[0x10, 0x00, 0x00, 0x00, 0x00, 0x00]` | Payload độ dài bằng không |
| 3 | Max payload (16 MB) | `[0x02, 0x00, 0x01, 0x00, 0x00, 0x00]` | Kích thước thực tế tối đa |
| 4 | IPv4 127.0.0.1:4242 | 7-byte encoding | IPv4 roundtrip |
| 5 | IPv4 192.168.1.100 | 7-byte encoding | Địa chỉ riêng tư (Private address) |
| 6 | IPv6 ::1 port 4242 | 19-byte encoding | IPv6 roundtrip |
| 7 | Known input | Fixed BLAKE3 output | Kết quả BLAKE3 cố định |
| 8 | Puzzle solution | Valid NodeId | Xác minh puzzle |
| 9 | KU wire format | MAGIC=0x4B, VER_META, CRC-16 | Sự đóng gói KU (Core DNA v6) |
| 10 | All message types | Unique, non-overlapping IDs | Bộ đăng ký type (Type registry) |
| 11 | All-layers header | Complete roundtrip | Tuần tự hóa (Serialization) |
| 12 | Ed25519 signature | 64 bytes | Kích thước chữ ký (Signature size) |

## 5.3 Phân tích Quy mô (Scale Analysis)

### 5.3.1 DHT Routing ở quy mô 100 tỷ Nodes

Flat Kademlia routing yêu cầu $\lceil\log_2 N\rceil$ hops. Với $N = 10^{11}$:

$$\text{hops}_{\text{flat}} = \lceil\log_2(10^{11})\rceil = 37 \text{ hops}$$

Với 50ms cho mỗi hop (mức RTT trung bình toàn cầu): $37 \times 50\text{ms} = 1.85\text{s}$ — quá chậm cho các truy cập tương tác.

**Hierarchical DHT** của OBP thông qua 7-tier node hierarchy giúp giảm thiểu điều này:

$$\text{hops}_{\text{hier}} \approx 2(\text{local}) + 3(\text{regional}) + 2(\text{backbone}) = 7 \text{ hops}$$

Với khoảng ~35ms trung bình cho mỗi hop (các RTT nội tầng ngắn hơn): $7 \times 35\text{ms} \approx 240\text{ms}$ — nằm trong mục tiêu 500ms.

### 5.3.2 Phân bổ Lưu trữ (Storage Distribution)

| Chỉ số (Metric) | Giá trị (Value) | Tính toán (Calculation) |
|--------|------:|-------------|
| Target KUs | 10 trillion | Tập tri thức toàn cầu |
| Average KU size | 400 bytes | Điển hình với trust metadata |
| Total storage | 4 PB | 10T × 400B |
| Storage per node | 40 bytes | 4 PB / 100B nodes |
| Hệ số nhân bản (Replication factor) | 20 | k=20 bản sao cho mỗi KU |
| Effective per node | 800 bytes | 40B × 20 |

*Table 13: Storage analysis at 100 billion node scale.*

### 5.3.3 Khả năng Mở rộng Membership (Membership Scalability)

Mỗi node chỉ theo dõi **local cluster** của nó (chứ không phải toàn bộ mạng):

- Quy mô cụm (Cluster size): ~5,000–10,000 members
- Bộ nhớ cho mỗi member: ~32 bytes (NodeId + address + metadata)
- Tổng trạng thái membership state: ~160–320 KB mỗi node
- Tốc độ SWIM probe: 1 probe/giây → băng thông không đáng kể

### 5.3.4 Sự Hội tụ Gossip (Gossip Convergence)

| Phạm vi (Scope) | Thời gian Hội tụ (Convergence Time) | Công thức (Formula) |
|-------|:----------------:|---------|
| Local cluster (10K nodes) | ~13 minutes | $\lceil\log_2(10^4)\rceil \times 60\text{s}$ |
| Regional (1M nodes) | ~30 minutes | Hierarchical gossip |
| Global (100B nodes) | 3–6 hours | Qua các backbone tiers |

*Table 14: Các ước tính hội tụ gossip. Metadata tri thức (không nhạy cảm về mặt thời gian) chấp nhận sự hội tụ kéo dài nhiều giờ; query routing sử dụng stigmergy để thích ứng tức thời.*

### 5.3.5 Khả năng Kháng Byzantine (Byzantine Tolerance)

Với β=3 disjoint paths của S/Kademlia và 20% nodes thù địch, xác suất có ít nhất một đường đi trung thực thành công là:

$$P_{\text{success}} = 1 - (0.2)^3 = 1 - 0.008 = 0.992 = 99.2\%$$

Đối với ước tính thận trọng hơn tính đến các con đường multi-hop có độ dài $h$:

$$P_{\text{success}} \approx 1 - (1 - 0.8^h)^{\beta}$$

Với $h=7$ hops và $\beta=3$: $P_{\text{success}} \approx 1 - (1-0.21)^3 = 1-0.49 = 51\%$ cho một lần thử đơn lẻ. Với việc thử lại qua các con đường khác nhau, tỷ lệ thành công thực tế đạt ~92%.

## 5.4 Phân tích Năng lượng (Energy Analysis)

### 5.4.1 Ngân sách Pin (Battery Budget)

Mục tiêu: <0.5% pin/ngày = <20 mAh cho một viên pin smartphone dung lượng 4,000 mAh.

| Hoạt động (Activity) | Tin nhắn/Ngày (Messages/Day) | Bytes/Tin nhắn (Bytes/Msg) | Tổng/Ngày (Total/Day) | Ước tính Năng lượng (Energy Est.) |
|----------|:------------:|:---------:|:---------:|:-----------:|
| SWIM probes | 86,400 | 8 | 691 KB | ~5 mAh |
| DHT lookups | ~100 | 200 | 20 KB | ~1 mAh |
| Query routing | ~50 | 500 | 25 KB | ~2 mAh |
| CRDT sync | ~144 | 530 | 76 KB | ~3 mAh |
| PubSub | ~200 | 100 | 20 KB | ~1 mAh |
| **Total** | | | **~832 KB** | **~12 mAh** |

*Table 15: Ước tính ngân sách năng lượng hàng ngày cho một Leaf node. Tổng ~12 mAh = 0.3% của viên pin 4,000 mAh.*

### 5.4.2 Tối ưu hóa Giao thức cho Năng lượng

| Tối ưu hóa (Optimization) | Cơ chế (Mechanism) | Tiết kiệm (Savings) |
|-------------|-----------|---------|
| QUIC 0-RTT | Bỏ qua handshake cho các peers lặp lại | ~30% ít RTTs hơn |
| SWIM piggybacking | Thực hiện gossip trên các probes hiện có | Không tốn thêm tin nhắn |
| Delta-state CRDT | Chỉ gửi các thay đổi | Giảm băng thông 10–100× |
| Bloom filters | Bản tóm tắt nội dung kích thước không đổi | so với trao đổi full index |
| Tier-based routing | Điện thoại không chuyển tiếp (forward) lưu lượng backbone | ~90% ít chuyển tiếp hơn |

## 5.5 So sánh với IPFS/libp2p

| Đặc tính (Feature) | IPFS/libp2p | OneBrain OBP |
|---------|-------------|--------------|
| **Primary purpose** (Mục đích chính) | File storage & retrieval | Knowledge sharing & discovery |
| **Transport** | TCP, QUIC, WebTransport, WebSocket | QUIC (native, sole transport) |
| **DHT** | Kademlia (amino DHT, 256-bit) | S/Kademlia (k=20, α=3, β=3) |
| **Membership** | Random walks, bootstrap list | SWIM + 7-tier fitness hierarchy |
| **Content routing** | DHT + Bitswap (want/have) | DHT + Stigmergy + Bloom filters |
| **Semantic routing** | None (hash-only) | Pheromone-based topic routing |
| **Sync mechanism** | Bitswap block exchange | Delta-state CRDT |
| **Reputation** | None built-in | EigenTrust + PoMV |
| **Data unit** | Content-addressed blocks | Typed Knowledge Units |
| **Bio-inspired** | Không | Có (stigmergy, fitness, pheromones) |
| **Protocol layers** | ~5 (modular, composable) | 9 (integrated, cross-optimized) |
| **Wire format** | Protobuf / CBOR (varies) | 6B header + Core DNA + CRC-16 |
| **Message types** | ~20 | 74 |
| **Offline-first** | Không (yêu cầu các bootstrap nodes) | Có (BLE/WiFi mesh, layers 0-1) |
| **Node hierarchy** | Flat (tất cả các peers bình đẳng) | 7 tiers (tự động promote/demote) |
| **Mobile target** | Không có mục tiêu cụ thể | <0.5% pin/ngày |
| **Scale target** | Hàng triệu (Millions) | 100 tỷ (100 billion) |
| **Active nodes** | ~50K DHT nodes [1] | — (chưa được triển khai) |

*Table 16: Comprehensive comparison of IPFS/libp2p and OneBrain Protocol.*

## 5.6 Các Mức Độ Tuân Thủ (Conformance Levels)

Giao thức định nghĩa 4 conformance levels (mức độ tuân thủ) cho phép triển khai tăng dần:

| Mức độ (Level) | Tên (Name) | Fitness | Các yêu cầu chính (Key Requirements) |
|:-----:|------|:-------:|-----------------|
| 0 | Leaf (Mobile/IoT) | — | Identity (L0), KU Codec, passive SWIM, Interest Vector |
| 1 | Contributor | 0.30–0.49 | Full SWIM, passive DHT, Vacuum Filter, KU Storage, PEX |
| 2 | Supernode | 0.50–0.79 | Active DHT, Stigmergy, PubSub, QUIC transport, Query Forwarding, 6-layer Bootstrap |
| 3 | Backbone | 0.80+ | S/Kademlia (disjoint paths), Cluster Aggregate, Trust Engine, KQL Engine, CRDT Sync, Multi-stream (100+) |

*Table 17: Các mức độ tuân thủ (conformance levels). Mức độ 0 có thể được triển khai trên các thiết bị IoT bị hạn chế; Mức độ 3 yêu cầu các tài nguyên cấp độ datacenter.*

Cách tiếp cận phân cấp này đảm bảo rằng ngay cả các thiết bị bị hạn chế về tài nguyên (smartwatches, cảm biến IoT) cũng có thể tham gia vào mạng lưới tri thức ở Mức độ 0, đồng thời dành các tính năng giao thức đầy đủ cho các nodes có năng lực.

---

## References

[1] D. J. Trautwein *et al.*, "Design and Evaluation of IPFS: A Storage Layer for the Decentralized Web," in *Proc. ACM SIGCOMM '22*, 2022.
