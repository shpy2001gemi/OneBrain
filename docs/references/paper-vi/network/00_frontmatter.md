# OneBrain Protocol: Một Bio-Inspired 9-Layer P2P Network Stack cho Chia sẻ Tri thức Phi tập trung

**Authors:** OneBrain Project Contributors  
**Contact:** shpy2001@gmail.com  
**Date:** June 2026  
**Version:** 1.0

---

## Abstract

Các giao thức peer-to-peer (P2P) đương đại — bao gồm IPFS/libp2p [1], BitTorrent [2], và devp2p của Ethereum [3] — được thiết kế về mặt kiến trúc cho việc phân phối các đối tượng dữ liệu mờ đục (opaque data objects): files, blocks, và transactions. Không có giao thức nào cung cấp hỗ trợ native cho *semantic query routing*, *fitness-based node hierarchies*, hoặc *conflict-free knowledge metadata synchronization*. Do đó, việc triển khai một mạng lưới chia sẻ tri thức phi tập trung trên nền tảng hạ tầng P2P hiện tại đòi hỏi phải giải quyết các bất tương thích kiến trúc cơ bản giữa các mô hình dữ liệu lấy file làm trung tâm (file-centric data models) và bản chất có cấu trúc, có thể truy vấn (queryable), và được chú giải độ tin cậy (trust-annotated) của tri thức nhân loại.

Tài liệu này trình bày **OneBrain Protocol (OBP)**, một P2P network stack 9 lớp được xây dựng chuyên biệt cho việc chia sẻ tri thức phi tập trung. Giao thức bao gồm: **Layer 0 (Identity)** — các cặp khóa Ed25519 với các cryptographic puzzles dựa trên BLAKE3 để chống tấn công Sybil; **Layer 1 (Transport)** — QUIC native (RFC 9000) với 0-RTT resumption và multiplexed bidirectional streams; **Layer 2 (Membership)** — một giao thức SWIM mở rộng được bổ sung bằng một node hierarchy 7 tầng mới (từ Leaf đến GlobalBackbone) với cơ chế promotion và demotion tự động dựa trên fitness trên 7 chiều có trọng số; **Layer 3 (Discovery)** — một cascade bootstrap offline-first 6 lớp trải dài từ trao đổi xã hội (social exchange) (QR/NFC/BLE), mDNS cục bộ, HTTP well-known endpoints, DHT bootstrap, DNS TXT records, đến các hardcoded seeds; **Layer 4 (DHT)** — S/Kademlia routing với 256 k-buckets, k=20, và β=3 đường lookup rời rạc để chống Byzantine; **Layer 5 (Stigmergy)** — một hệ thống pheromone routing lấy cảm hứng từ sinh học giúp củng cố các đường knowledge query thành công và làm bay hơi các đường không thành công, lấy cảm hứng từ tối ưu hóa đàn kiến (ant colony optimization); **Layer 6 (Content)** — các Vacuum probabilistic filters (Bloom filters dựa trên BLAKE3) cho các bản tóm tắt năng lực nội dung (content capability summaries) có kích thước không đổi; **Layer 7 (PubSub)** — topic-based publish/subscribe với 128-bit interest vectors; và **Layer 8 (Sync)** — delta-state CRDT synchronization với trao đổi hai chiều dựa trên VectorClock, đạt được mức giảm băng thông từ 10–100× so với full-state replication.

Giao thức được triển khai trong **~8,000 dòng code Rust** trên 30 core modules với **159 unit và integration tests** và **12 wire format test vectors**. Hệ thống tin nhắn hỗ trợ **74 loại message riêng biệt** (bao gồm 6 message Encoding Consensus) được mã hóa với một universal header 6-byte nhỏ gọn. Các mục tiêu thiết kế bao gồm khả năng hoạt động trên các thiết bị di động với mức tiêu hao pin **<0.5% mỗi ngày**, kết nối mạng offline-first không phụ thuộc vào internet, và mở rộng quy mô lên tới **100 tỷ nodes** thông qua hierarchical DHT routing giúp đạt được ~7 hops (~240ms) so với ~37 hops (~1.85s) của flat Kademlia ở cùng quy mô.

**Keywords:** Peer-to-peer networks, decentralized knowledge sharing, QUIC transport, Kademlia DHT, SWIM membership protocol, stigmergy, ant colony optimization, pheromone routing, Bloom filters, CRDT synchronization, node hierarchy, offline-first networking, bio-inspired computing, mobile P2P
