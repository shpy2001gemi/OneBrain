# 7. Implementation and Evaluation

## 7.1 Implementation Summary

### 7.1.1 Module Inventory

PoMV được triển khai trên **16 modules** thuộc hai Rust crates:

**Core Modules (ku-core):**

| # | Module | File | LOC | Structs/Enums | Constants | Tests |
|:-:|--------|------|----:|:------------:|:---------:|:-----:|
| 1 | Metabolism | [metabolism.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/metabolism.rs) | 385 | 2 | 8 | 16 |
| 2 | Metabolism Store | [metabolism_store.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/metabolism_store.rs) | 235 | 2 | 3 | 7 |
| 3 | Epistemic Engine | [epistemic_engine.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/epistemic_engine.rs) | 300 | 0 | 11 | 10 |
| 4 | Entropy | [entropy.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/entropy.rs) | 280 | 1 | 5 | 15 |
| 5 | Prediction | [prediction.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/prediction.rs) | 350 | 5 | 0 | 12 |
| 6 | Synaptic | [synaptic.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/synaptic.rs) | 382 | 4 | 9 | 14 |
| 7 | Immune | [immune.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/immune.rs) | 389 | 3 | 7 | 11 |
| 8 | Ecosystem | [ecosystem.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/ecosystem.rs) | 292 | 3 | 4 | 8 |
| 9 | PoMV Aggregator | [pomv.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/pomv.rs) | 256 | 4 | 1 | 9 |
| 10 | EigenTrust | [eigentrust.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/eigentrust.rs) | 272 | 3 | 5 | 9 |
| 11 | Spread Analysis | [spread_analysis.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/spread_analysis.rs) | 308 | 2 | 4 | 11 |
| 12 | Runtime | [pomv_runtime.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/pomv_runtime.rs) | 466 | 3 | 0 | 9 |
| 13 | KU Lifecycle | [ku_lifecycle.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/ku_lifecycle.rs) | 246 | 1 | 0 | 5 |
| 14 | Epigenetics | [epigenetics.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/epigenetics.rs) | 219 | 2 | 3 | 7 |
| 15 | OBT Integration | [obt_integration.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/obt_integration.rs) | 354 | 1 | 0 | 8 |
| | **Core Subtotal** | | **4,734** | **36** | **60** | **151** |

*Table 12: Các module cốt lõi của PoMV với các chỉ số kích thước, độ phức tạp và kiểm thử.*

**Network Module (ku-net):**

| # | Module | File | LOC | Structs/Enums | Tests |
|:-:|--------|------|----:|:------------:|:-----:|
| 16 | Metabolism Gossip | [metabolism_gossip.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/metabolism_gossip.rs) | 278 | 4 | 6 |
| | **Network Subtotal** | | **278** | **4** | **6** |

**Grand Total:** 5.012 LOC | 40 types | 60 constants | 157 tests

### 7.1.2 Type Safety

Tất cả 40 định nghĩa struct/enum đều áp dụng type safety tại thời điểm biên dịch (compile time):

- `MetabolismEvent` (7 variants) — ngăn chặn việc ghi nhận các loại sự kiện không xác định.
- `ResolutionMethod` (4 variants) — ràng buộc các chế độ giải quyết dự đoán (prediction resolution modes).
- `PredictionOutcome` (4 variants) — phân loại kết quả có giới hạn.
- `AntibodyType` (4 variants) — phân loại phát hiện miễn dịch có định kiểu.
- `BondReason` (3 variants) — theo dõi nguyên nhân liên kết synaptic.

### 7.1.3 Dependencies

| Dependency | Purpose | CRDT Used |
|-----------|---------|:---------:|
| G-Counter (ku-core/crdt.rs) | Các bộ đếm metabolism, tương tác | ✅ |
| LWW-Register (ku-core/crdt.rs) | Nhãn thời gian hoạt động cuối, giải quyết dự đoán | ✅ |
| ORSet (ku-core/crdt.rs) | Gossip antibody, registry dự đoán | ✅ |
| BLAKE3 (ku-core) | Hash mô hình, tính toán CID | — |
| Types (ku-core/types.rs) | EpistemicStatus, TrustSection, Gene | — |

**Zero external dependencies** cho PoMV — tất cả các thành phần được xây dựng trên cơ sở hạ tầng CRDT sẵn có trong `ku-core/crdt.rs`.

## 7.2 Test Coverage

### 7.2.1 Unit Tests by Module (157 tests)

**Metabolism (16 tests):**

| Test | Validates |
|------|-----------|
| `test_new_metabolism_starts_at_zero` | Fresh KU có các bộ đếm bằng 0 |
| `test_query_hit_increases_rate` | QueryHit → metabolic_rate > 0 |
| `test_retrieval_with_dwell_time` | Theo dõi Retrieval + dwell time |
| `test_citation_increases_rate` | Citation làm tăng metabolic rate |
| `test_refutation_is_positive` | **Quan trọng: refutation làm tăng chứ không làm giảm** |
| `test_metabolic_rate_decays_over_time` | Suy giảm lũy thừa (Exponential decay) với chu kỳ bán rã (half-life) |
| `test_node_diversity_tracking` | Độ chính xác của bộ đếm node duy nhất |
| `test_merge_two_metabolisms` | CRDT merge tạo ra trạng thái chính xác |
| `test_merge_idempotent` | Gộp cùng một dữ liệu hai lần = không đổi |
| `test_is_alive_with_no_activity` | Hoạt động bằng 0 → không sống (not alive) |
| `test_is_alive_with_activity` | Hoạt động tích cực → sống (alive) |
| `test_rate_to_u16_normalization` | Độ chính xác của chuẩn hóa Sigmoid |
| `test_avg_dwell_no_retrievals` | Trường hợp biên: xử lý phép chia cho 0 |
| `test_total_engagement_counts_all` | Tổng của cả 7 bộ đếm |
| `test_merge_keeps_earliest_created_at` | Phép gộp giữ nguyên nhãn thời gian tạo lập |
| `test_zero_metabolism_after_very_long_time` | Decay tiệm cận về 0 |

**Epistemic Engine (10 tests):**

| Test | Validates |
|------|-----------|
| `test_rumor_stays_without_activity` | Không hoạt động → giữ nguyên RUMOR |
| `test_rumor_to_hearsay` | metabolic_rate > 0.001 → HEARSAY |
| `test_hearsay_to_testimony` | retrieval_count ≥ 3 → TESTIMONY |
| `test_testimony_to_observation` | citation_count ≥ 1 → OBSERVATION |
| `test_observation_to_hypothesis` | citations ≥ 3 + diversity ≥ 3 → HYPOTHESIS |
| `test_hypothesis_to_evidence` | node_diversity ≥ 5 → EVIDENCE |
| `test_evidence_to_corroborated` | citations ≥ 5 → CORROBORATED |
| `test_formally_proven_is_terminal` | Không có transition từ FORMALLY_PROVEN |
| `test_evaluate_max_status_jumps` | Nhảy nhiều bước khi đạt ngưỡng |
| `test_consensus_requires_time` | Cổng thời gian: 6 tháng + tỷ lệ cao |

**Immune Engine (11 tests):**

| Test | Validates |
|------|-----------|
| `test_healthy_spread_no_antibodies` | Lan truyền tự nhiên → không có phát hiện |
| `test_temporal_burst_detected` | 50+/giờ → antibody kích hoạt |
| `test_source_concentration_detected` | >80% từ nguồn đơn lẻ → bị phát hiện |
| `test_low_engagement_detected` | Sao chép nhiều, sử dụng ít → bị phát hiện |
| `test_diversity_deficit_detected` | Ít nguồn, nhiều lượt sao chép → bị phát hiện |
| `test_bot_spread_multiple_signals` | Hành vi bot kích hoạt nhiều antibodies |
| `test_quarantine_requires_multiple_types` | Một antibody đơn lẻ là không đủ để quarantine |
| `test_survival_score_anti_fragile` | 10 cuộc tấn công → score = 1.0 |
| `test_dead_ku_no_survival_bonus` | KU đã chết nhận 0 survival |
| `test_survival_to_u16` | Độ chính xác chuẩn hóa u16 |
| `test_too_few_replications_no_flags` | Dưới ngưỡng → không có dương tính giả |

**Spread Analysis (11 tests):**

| Test | Validates |
|------|-----------|
| `test_organic_high_score` | Lan truyền tự nhiên → organicity cao |
| `test_bot_low_score` | Lan truyền bot → organicity thấp |
| `test_organic_beats_bot` | Organic luôn đạt điểm cao hơn bot |
| `test_temporal_regular_intervals_bot` | Thời gian đều đặn → phát hiện bot |
| `test_temporal_varied_intervals_organic` | Thời gian đa dạng → phân loại tự nhiên |
| `test_source_diversity_high` | Nhiều nguồn duy nhất → điểm cao |
| `test_source_diversity_low` | Ít nguồn → điểm thấp |
| `test_engagement_bot_dwell` | Dwell <1s → tương tác bot |
| `test_engagement_real_user` | Dwell >5s → tương tác thực |
| `test_organicity_multiplier` | Độ chính xác của công thức multiplier |
| `test_empty_metrics_neutral` | Trường hợp biên: đầu vào rỗng → trung tính |

### 7.2.2 Integration Properties Verified

| Thuộc tính (Property) | Bằng chứng kiểm thử (Test Evidence) |
|----------|-------------|
| **CRDT convergence** | `test_merge_idempotent`, `test_merge_two_metabolisms`, `test_merge_registries`, `test_merge_synaptic_maps` |
| **Monotonic status** | `test_formally_proven_is_terminal`, `test_evaluate_max_status_jumps` |
| **Non-punitive** | `test_refutation_is_positive`, ngữ nghĩa chỉ tăng (increment-only) của G-Counter |
| **Content-agnostic** | Tất cả các kiểm thử miễn dịch sử dụng `SpreadObservation` (dữ liệu hành vi), không dùng nội dung |
| **Antifragile** | `test_survival_score_anti_fragile` — 10 tấn công → phần thưởng tối đa |
| **Ngăn ngừa dương tính giả** | `test_quarantine_requires_multiple_types`, `test_too_few_replications_no_flags` |
| **Full lifecycle** | `test_full_lifecycle` — tạo lập → sự kiện → tick → tính điểm |

## 7.3 Comparison: PoMV vs PoK v1

PoMV là bản tái thiết kế hoàn chỉnh của PoK v1 ban đầu (vốn sử dụng kiến trúc dựa trên bỏ phiếu):

| Khía cạnh (Dimension) | PoK v1 (Dựa trên bình chọn) | PoMV v2 (Dựa trên quan sát) |
|-----------|---------------------|---------------------------|
| **Ai quyết định giá trị** | Người bỏ phiếu cộng đồng | Không có ai — việc sử dụng mang tính khách quan |
| **Tri thức chủ quan** | Không thể đánh giá ("Hoàng hôn có đẹp không?") | Hỗ trợ đầy đủ (metabolism = việc sử dụng) |
| **Cấu trúc (Architecture)** | 5 lớp (Identity → Screening → Evaluation → Trust → Evolution) | 6 tín hiệu + hệ thống miễn dịch |
| **Chống thao túng** | 7 cơ chế (quadratic voting, commit-reveal, câu đố PoU, staking, phát hiện thông đồng, EigenTrust, sàng lọc AI) | 4 loại antibodies + phân tích lan truyền + EigenTrust + bộ nhớ miễn dịch |
| **Mô hình phần thưởng** | Staking bất đối xứng (tỷ lệ rủi ro 1:3) + clawback | Chia sẻ tỷ lệ PoMV tuyến tính, **không có clawback** |
| **Epistemic transitions** | Quorum dựa trên bình chọn | Các ngưỡng CRDT có thể quan sát được |
| **Độ phức tạp** | 11+ lớp phòng thủ | 6 tín hiệu + 4 loại antibodies |
| **Phân tập trung (Decentralization)** | Cần quorum (một phần) | Mỗi node đánh giá độc lập (toàn phần) |
| **Cân chỉnh triết học** | "Tri thức này có đúng không?" | "Tri thức này có được sử dụng không?" |
| **Độ phức tạp mã nguồn** | Chưa được triển khai | 5.012 LOC, 157 tests, đã triển khai đầy đủ |

*Table 13: So sánh PoK v1 vs PoMV v2.*

Sự chuyển dịch cơ bản: PoK v1 đã cố gắng trả lời một câu hỏi không thể trả lời được ("Tri thức này có đúng không?"). PoMV v2 trả lời một câu hỏi có thể quan sát được bằng thực nghiệm ("Tri thức này có được sử dụng không?").

## 7.4 Threat Analysis

### 7.4.1 Attack Scenarios and Defenses

| # | Loại tấn công | Cơ chế | Phòng thủ | Chi phí tấn công | Kết quả |
|:-:|--------|-----------|---------|:--------------:|---------|
| 1 | **Query bombing** | Bot gửi 10K truy vấn cho KU mục tiêu | Temporal burst antibody + source concentration | Thấp | Bị quarantine; organicity ≈ 0 |
| 2 | **Citation ring** | 5 Sybil nodes trích dẫn chéo lẫn nhau | Diversity deficit + EigenTrust độ tin cậy thấp | Trung bình (5 S/Kademlia puzzles) | PoMV thấp; độ tin cậy node thấp |
| 3 | **Dwell time inflation** | Bot báo cáo thời gian đọc (dwell time) dài | Engagement authenticity (kiểm tra action_ratio) | Thấp | Điểm tương tác (engagement score) thấp |
| 4 | **Entropy gaming** | Gửi nội dung kỳ dị để lấy điểm thưởng entropy | Suy giảm 7 ngày → metabolism bằng 0 → chết tự nhiên | Thấp | Điểm tăng tạm thời, không có phần thưởng lâu dài |
| 5 | **Flash mob** | 100 nodes phối hợp chiến dịch trong 1 giờ | Kích hoạt cả 4 loại antibodies; quarantine | Rất cao (100 puzzles) | Bị quarantine trong vòng 1 epoch |
| 6 | **Slow manipulation** | 10 nodes, có vẻ tự nhiên, bền vững | Vượt qua phân tích lan truyền | Cực kỳ cao | Thành công — nhưng chi phí ≈ chuyển giao giá trị thực |
| 7 | **Eclipse attack** | Cô lập một node để kiểm soát tầm nhìn của nó | SWIM protocol (không phải phạm vi của PoMV) | Tấn công cấp mạng | Được xử lý bởi tầng vận chuyển (transport layer) |

### 7.4.2 Luận điểm "Slow Manipulation"

Tấn công số 6 (slow, organic-looking manipulation bởi các node thật với sự tương tác bền vững) là cuộc tấn công duy nhất mà PoMV không thể tự động phát hiện. Điều này là **do thiết kế**:

> Nếu 10 nodes thực sự dành nhiều tháng để tạo nội dung thật, tạo trích dẫn thật, duy trì thời gian đọc thật và duy trì các nguồn đa dạng — họ đã **thực sự chuyển giao giá trị** cho mạng lưới. "Cuộc tấn công" này không thể phân biệt được với đóng góp thực sự bởi vì nó CHÍNH LÀ đóng góp thực sự.

Chi phí để duy trì hành vi thao túng có vẻ tự nhiên ở quy mô lớn vượt quá phần thưởng nhận được từ PoMV, làm cho nó trở nên bất hợp lý về mặt kinh tế. Điều này tương tự như lập luận "tấn công 51%" trong Bitcoin — khả thi về mặt lý thuyết nhưng bất hợp lý về mặt kinh tế vì chi phí tấn công vượt quá lợi ích mang lại.

## 7.5 Performance Characteristics

### 7.5.1 Per-Module Computational Cost

| Module | Operation (Thao tác) | Complexity (Độ phức tạp) | Memory (Bộ nhớ) |
|--------|-----------|:----------:|:------:|
| Metabolism | `record_event()` | O(1) | O(N_nodes) trên mỗi KU |
| Metabolism | `metabolic_rate()` | O(1) | — |
| Metabolism Store | `tick()` tất cả KUs | O(N_KUs) | O(N_KUs) |
| Epistemic Engine | `evaluate_max_status()` | O(1) | — |
| Entropy | `cosine_distance()` | O(D) với D là chiều embedding | — |
| Entropy | `novelty_score()` | O(K × D) với K là số lân cận | — |
| Prediction | `prediction_score()` | O(P × R) với P là dự đoán, R là resolution | O(P + R) |
| Synaptic | `reinforce()` | O(1) khấu hao | O(B) trên mỗi KU, B≤100 |
| Synaptic | centrality (PageRank) | O(I × N × B) với I=10 vòng lặp | O(N) |
| Immune | `analyze()` | O(1) | O(1) |
| Spread Analysis | `analyze()` | O(T) với T là nhãn thời gian | O(T) |
| EigenTrust | `compute_global()` | O(I × N²) với I=10, N là số node | O(N²) |
| PoMV Aggregator | `compute()` | O(1) | O(1) |
| Runtime | `tick()` | O(N_KUs × (PageRank + EigenTrust)) | O(N_KUs + N²) |

*Table 14: Độ phức tạp tính toán của các thao tác PoMV.*

### 7.5.2 Expected Performance at Scale

| Quy mô (Scale) | Số lượng KU | Số lượng Node | Thời gian tick() | Bộ nhớ (Memory) |
|-------|:--------:|:----------:|:-----------:|:------:|
| Nhỏ (Small) | 1,000 | 100 | <10ms | ~10MB |
| Trung bình (Medium) | 100,000 | 10,000 | ~1s | ~1GB |
| Lớn (Large) | 10,000,000 | 1,000,000 | ~100s (batch) | ~100GB |

Ở quy mô lớn, hàm `tick()` sẽ được thực hiện theo đợt (batched) và song song hóa. Việc tính toán độ trung tâm PageRank và vòng lặp EigenTrust là các điểm nghẽn — cả hai đều có thể được xấp xỉ để đạt khả năng mở rộng (Monte Carlo sampling cho PageRank, EigenTrust chỉ tính cục bộ).

---
