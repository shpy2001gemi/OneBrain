# Báo cáo tổng quan OneBrain — 28/09/2026

> **Cập nhật hướng triển khai cùng ngày:** sau báo cáo này, owner đã quyết định
> D-044: ưu tiên MVP chạy được, hoãn qualification sâu và bỏ dependency ENC-003
> khỏi task QA/demo. [MASTER_PLAN](handoffs/2026-09-ku-obp-productization/MASTER_PLAN.md)
> và [PROGRESS](handoffs/2026-09-ku-obp-productization/PROGRESS.md) là nguồn hiện hành.
> Các con số/blocker/đường găng bên dưới là snapshot trước quyết định đó;
> thiếu reviewer/harness không còn là blocker của MVP thử nghiệm. Audit nhánh
> mới (gồm fetch/remote và worktrees) nằm trong PROGRESS, không trong snapshot này.

Ngày rà soát: 28/09/2026, Asia/Saigon. Repository tại `689efd5`, nhánh làm việc `codex/ku-enc-003-handoff`; có thay đổi local chưa commit từ công việc qualification. Báo cáo bao gồm cả tiến độ local này, nhưng không tính nó là đã merge.

**OneBrain đã có nền tảng kỹ thuật, runtime dùng chung và các bề mặt CLI/Web/Desktop. Đợt productization KU–OBP đã merge 20/24 đầu việc (83,3%). Tuy nhiên, sản phẩm chưa đạt nghiệm thu end-to-end: model chưa qualified, QA xuyên các bề mặt và hành trình KU→OBP còn mở. Mobile, kinh tế OBT và các giao diện mở rộng còn nhiều việc.**

## 1. Cách đọc phần trăm

Phần trăm dưới đây là số đầu việc được ghi nhận hoàn thành chia cho số đầu việc trong đúng kế hoạch đó; không phải phần trăm công sức, chất lượng hay xác suất phát hành thành công. Các task có độ lớn khác nhau.

| Phạm vi có mẫu số rõ | Đã hoàn thành | Tỷ lệ | Cách hiểu |
|---|---:|---:|---|
| Foundation vNext, gồm tùy chọn | 96/99 task được đánh dấu `[x]` | **97,0%** | Ba task còn lại đều optional |
| Foundation bắt buộc | 96/96 task không optional | **100%** | Phạm vi foundation; không đóng production rollout |
| Đợt KU–OBP hiện tại, gồm task Deferred | 20/24 task Merged | **83,3%** | Còn 1 Blocked, 2 Planned, 1 Deferred |
| Đợt KU–OBP nếu loại task Deferred | 20/23 | **87,0%** | Chỉ là cách nhìn phạm vi đang theo đuổi |
| Nhóm KU trong ledger, gồm encoder và semantic follow-up | 10/13 Merged | **76,9%** | Còn ENC-003, QA-001 và SEM-001 |
| Nhóm OBP trong ledger | 10/10 Merged | **100% theo tiêu chí task đã chấp thuận** | Consumer NAT được bỏ qua theo quyết định owner; chưa qualified |
| Tích hợp sản phẩm KU→OBP | 0/1 Merged | **0% nghiệm thu task tích hợp** | Các module bên dưới đã có, task end-to-end chưa bắt đầu |

**Chưa có mẫu số và trọng số chung đáng tin cậy để tính một phần trăm cho toàn bộ tầm nhìn OneBrain.** Không lấy 83,3% làm tỷ lệ toàn dự án, không cộng foundation với productization vì có phạm vi chồng lấn, và không lấy số màn hình/spec/test làm số tính năng production hoàn tất. Mobile và các milestone tương lai được báo cáo bằng trạng thái cụ thể thay vì gán tỷ lệ tùy ý.

Nguồn: [foundation plan](research/ONEBRAIN_FOUNDATION_IMPLEMENTATION_PLAN_V7_1.md), [execution ledger](handoffs/2026-09-ku-obp-productization/PROGRESS.md), [master plan](handoffs/2026-09-ku-obp-productization/MASTER_PLAN.md).

## 2. Chúng ta đã làm được gì, còn thiếu gì?

| Mảng | Đã có và bằng chứng | Còn mở / giới hạn |
|---|---|---|
| Core tri thức và foundation vNext | Canonical identities/codec, feed, authority, private storage, reconciliation, fidelity, KQL/Mapping và các contract liên crate; 96 task foundation được đánh dấu hoàn tất | Remote cognition `RUN-003`, RIBLT `RIB-001/002` là tùy chọn chưa làm; foundation không đồng nghĩa triển khai vận hành mọi capability |
| Runtime, API và độ bền dữ liệu | Runtime node sở hữu dịch vụ; REST/private WS/CLI; resource budgets, lifecycle, crash recovery, compaction, rollback; hồ sơ P0–P3 và DR-M5 đã có bằng chứng chấp thuận | Lane vNext vẫn opt-in/default-off; cần qualification đúng candidate/platform khi phát hành |
| KU dùng cục bộ | Shared service cho prepare/preview/save/read/search/revision, private vault và journal; idempotency, exact confirmation, recovery qua restart; contract, service, API đã merge | Không được suy khả năng hiểu văn bản tùy ý từ việc lưu/read/recovery chạy đúng |
| Encoder và AI | Framework dùng chung, compiler/validator xác định, model proposal có giới hạn, token/budget/cancellation, Ollama adapter, tokenizer Qwen và cơ chế worker; KU-ENC-001/002 đã merge | `model_qualified=false`; bộ harness thực tế, họ model thứ hai, đo tài nguyên và grading độc lập chưa đủ; các lỗi giữ nghĩa trong semantic follow-up chưa được đóng |
| CLI, Web, Desktop | KU và networking được nối vào shared node/API. Desktop KU đã merge theo D-043; lifecycle Windows có kết quả 8/8 mặc định và 10/10 feature-on | KU-QA xuyên bề mặt chưa làm; native WebView, OS sleep thật, lifecycle macOS/Linux chưa qualified; Desktop lib test có giới hạn host `STATUS_ENTRYPOINT_NOT_FOUND` |
| OBP/network | Node-owned lifecycle, bootstrap/discovery/reservation, routing, durable outbox, relay failover, API/CLI/Web/Desktop; đã chuyển legacy seed sang chế độ compatibility rõ ràng | Networking vẫn default-off; 10 kịch bản consumer-network chưa chạy; browser/mobile và qualification đa nền tảng chưa hoàn tất |
| Linux P5 | Hồ sơ ngày 23/09 ghi nhận ba host, 13 fault drills, 10 exit oracles và independent verification đạt trên candidate `c453f3e` | Chỉ đúng candidate và phạm vi Linux đó; không tự chuyển qualification sang HEAD hiện tại, Windows/macOS hoặc mạng NAT người dùng |
| Base v1 và Concept Registry | Có tag `base-v1.0.0-owner-waiver.1`; hồ sơ trước ghi nhận CI ba OS, prebuilt Registry và soak 72 giờ. Checkpoint 27/09 ghi nhận native Registry verify/status pass | Chưa có strict tag `base-v1.0.0`; waiver không phải `BASE-GATE-V1 qualified=true`. Production-reference, strict gate và từng product/platform là các phạm vi khác nhau |
| Mobile Android/iOS | Flutter/native/Rust scaffolding, token/component/i18n, bảo vệ khóa và vault, private capture, restart recovery; signed Registry admission; Android Local Import ABI 13 có bằng chứng emulator | Chưa `ReadyOffline`; MOB-04 còn dở; MOB-05B peer/provider/iOS, MOB-05C Registry thật cỡ 2,2 GB, MOB-06 AI/tools, MOB-07 media/lifecycle, MOB-08 network, MOB-09 release và physical-device gates còn mở |
| KQL, OBKG, PoMV mở rộng | Đã có module nền, query/graph/evidence và các projection cục bộ; foundation có implementation/test | M6 active distributed KQL và luồng Use→Outcome→Benefit production chưa được đóng. Không suy một query hit thành lợi ích hoặc thưởng |
| OBT/wallet | Có legacy/prototype và ranh giới ngăn dữ liệu tương thích bị hiểu thành kinh tế thật | M7 reward policy, ledger/finality, wallet production và adversarial gates còn phía trước; chưa có nền kinh tế production |
| Extension, bot, glasses, BCI | Extension/bot/glasses có scaffold và mô tả định hướng; BCI ở nghiên cứu | Chưa có sản phẩm đã qualified; không nằm trên đường găng KU–OBP hiện tại |

Nguồn chi tiết: [KU service](handoffs/2026-09-ku-obp-productization/outputs/KU_RUN_001_IMPLEMENTATION.md), [encoder](handoffs/2026-09-ku-obp-productization/outputs/KU_ENC_002_IMPLEMENTATION.md), [Desktop](handoffs/2026-09-ku-obp-productization/outputs/KU_DESK_001_IMPLEMENTATION.md), [OBP acceptance](handoffs/2026-09-ku-obp-productization/outputs/OBP_QA_001_FUNCTIONAL_ACCEPTANCE_V2.md), [P5](handoffs/2026-09-ku-obp-productization/outputs/OBP_QA_001_P5_PRODUCTION_20260923.md), [migration](handoffs/2026-09-ku-obp-productization/outputs/OBP_MIG_001_IMPLEMENTATION.md), [mobile evidence hiện có](../src/onebrain-mobile/compliance/mobile_build_evidence_v1.json), [distributed-runtime plan](research/WIP_DISTRIBUTED_RUNTIME_IMPLEMENTATION_PLAN_V2.md), [snapshot Base](PROJECT_STATUS.vi.md).

## 3. Blocking ở đâu?

| Mức / blocker | Tình trạng thực tế | Chặn điều gì? | Bên cần xử lý / điều kiện gỡ |
|---|---|---|---|
| **P0 — Harness qualification chưa hoàn chỉnh** | Export và ledger/reducer mới chạy offline/synthetic; chưa nối đầy đủ native inference, nguồn được cấp quyền, tokenizer/worker họ thứ hai, collector tài nguyên/cancellation, thống kê và FID/grading độc lập | Hoàn tất `KU-ENC-003`, từ đó KU-QA và integration | Engineering: hoàn chỉnh harness theo contract; chứng minh measurement, accounting, recovery và grading trước khi run |
| **P0 — Custody/reviewer/run lock** | Đã tìm và xuất 100 VI + 100 EN; cả 200 trường reviewer còn trống; chưa cấp source grants, chưa khóa/acknowledge run package | Chưa được chạy qualification model calls, chưa kết luận chất lượng model | Custodian/reviewer thực tế: provenance, processing permission, exposure/grouping/near-duplicate review, labels/rubric/FID và acknowledgment đúng digest |
| **P1 — KU-QA chưa thực hiện** | CLI/Web/Desktop đã có implementation, nhưng task QA chung còn Planned | Kết luận cùng thao tác có cùng identity/state/error và recovery xuyên các surface | Engineering/QA sau dependency ENC-003: thực hiện task 09, gồm các ca lỗi/restart |
| **P1 — KU→OBP end-to-end chưa nghiệm thu** | `INT-KU-OBP-001` Planned; phụ thuộc KU-QA và OBP-QA | Cam kết người dùng đi từ KU local đến publish/reconcile/remote validate có scope đúng | Engineering/QA: task 20 sau khi đủ dependency, giữ các bước consent và privacy |
| **P1 — Consumer NAT và OS journey chưa qualified** | OBP task được chấp thuận chức năng với waiver; `consumer_nat_qualified=false`; VPS hiện có không thay được mạng người dùng độc lập | Claim automatic connectivity trên môi trường người dùng và quyết định rollout tương ứng | QA/owner: môi trường hai consumer networks, hai relay độc lập và chạy kịch bản thực tế khi mở lại gate; không chặn lại task đã được waiver đóng |
| **P1 — Mobile Registry/thiết bị/nền tảng** | Chưa đủ provider/iOS adapter, activation Registry thật, A/B recovery và physical-device evidence | `ReadyOffline`, network/mobile release | Mobile engineering và device QA khi lane được mở lại; đây là danh sách tồn đọng, không phải kế hoạch triển khai mới |
| **P2 — Strict release và phạm vi evidence** | Base đang ở owner-waiver; P5 còn provider-document/non-Linux/mobile limitations | Strict qualification hoặc rollout rộng hơn phạm vi đã chấp thuận | Owner/release: quyết định mục tiêu strict; bổ sung evidence đúng candidate và platform nếu theo đuổi |
| **P2 — Chất lượng semantic còn Deferred** | Ghi nhận lỗi reference/future scope, alternatives/qualifiers/units và independent verification trong follow-up | Activation/claim chất lượng của semantic v2; không chặn ngược các task framework đã chấp thuận | Engineering/research: xử lý theo contract/acceptance riêng; không lấy known development cases thay holdout |

P0/P1/P2 ở đây là thứ tự ưu tiên đề xuất cho báo cáo, không thay đổi trạng thái hoặc quyết định canonical.

Nguồn: [checkpoint source/accounting](handoffs/2026-09-ku-obp-productization/outputs/KU_ENC_003_SOURCE_ACCOUNTING.md), [continuation](handoffs/2026-09-ku-obp-productization/NEXT_CONVERSATION.md), [semantic backlog](handoffs/2026-09-ku-obp-productization/outputs/KU_SEM_001_CONTRIBUTOR_BACKLOG.md), [OBP acceptance và waiver](handoffs/2026-09-ku-obp-productization/outputs/OBP_QA_001_FUNCTIONAL_ACCEPTANCE_V2.md).

## 4. Đường găng và thứ tự đề xuất

```text
Hoàn chỉnh harness + bằng chứng custody/reviewer
  → khóa và acknowledge run package
  → chạy/đánh giá KU-ENC-003
  → KU-QA-001: cùng semantics trên CLI/Web/Desktop và qua restart
  → INT-KU-OBP-001: KU local → consent → network → remote validation
  → xem xét release/rollout trong đúng phạm vi đã qualified
```

1. **Ưu tiên gỡ ENC-003.** Không cần tìm lại đường dẫn workbook. Phần engineering có thể tiếp tục trong lúc custodian/reviewer hoàn tất đầu vào bên ngoài; model qualification phải chờ đủ điều kiện khóa run.
2. **Đóng QA và integration trước khi tuyên bố MVP end-to-end hoàn thành.** Số task còn ít nhưng đây là các gate có thể phát hiện lỗi cần sửa lại implementation.
3. **Giữ riêng nhánh nghiệm thu môi trường thực.** Consumer NAT, native GUI/OS lifecycle, platform qualification và strict Base phải có quyết định/phạm vi rõ; kết quả unit test không thay được chúng.
4. **Sau đó ưu tiên mobile hoặc M6 theo mục tiêu sản phẩm.** Mobile cần một đợt hoàn thiện riêng; M6/M7 và các giao diện phụ không nên được gộp vào phần trăm KU–OBP.

Chưa đủ dữ liệu để đưa ngày hoàn thành đáng tin cậy: thời gian reviewer/custody và kết quả model qualification chưa biết. Không dùng phần trăm task để suy số ngày còn lại.

## 5. Chất lượng, kiểm tra mới và giới hạn audit

Đã chạy trực tiếp trong lượt lập báo cáo:

| Kiểm tra | Kết quả |
|---|---|
| `python scripts/ci/validate_vnext_contracts.py` | **PASS** — 99 task trong contract graph, 18 ADR, 37 negative assertions, 56 foundation vectors/22 domains, 48 encoder cases/2 jobs, 529 local links và các contract khác |
| `python scripts/ci/validate_mobile_build_contracts.py` | **PASS** — 98 evidence rows; 123 feature IDs, 112 screen mappings, 62 components, 13 patterns; không broken link/source guard failure |
| Năm module unittest encoder: source export, attempt ledger, analysis, package audit, contract | **85/85 PASS**, khoảng 14,4 giây; gồm test synthetic và Windows ACL |
| `git diff --check` trước thêm báo cáo | **PASS**; có cảnh báo chuyển LF/CRLF của các file handoff có sẵn |
| Đếm task từ file kế hoạch/ledger | Foundation 96 checked + 3 optional unchecked; KU–OBP 20 Merged + 1 Blocked + 2 Planned + 1 Deferred |

Validator xanh xác nhận các kiểm tra contract/evidence structure của nó; không xác nhận mọi feature production hoạt động. 85 test xanh không phải 85 model cases qualified.

Các kết quả Rust, Web, Desktop, Linux P5, Registry và CI/soak được trích từ báo cáo checkpoint tương ứng; lượt này không chạy lại toàn workspace build/test, CI từ xa, inference holdout, thiết bị thật, native GUI hoặc remote host. Không đọc/publish nội dung holdout hay labels để làm báo cáo.

Local `HEAD`, `main` và ref cache `origin/main` cùng tại `689efd5`; không fetch/query remote mới, nên không khẳng định đây là remote tip mới nhất. Các thay đổi đầu vào được giữ nguyên; báo cáo không commit/push/merge, sửa code hay cập nhật mobile implementation evidence.

## 6. Những điểm tài liệu dễ gây hiểu sai

- `PROJECT_STATUS.vi.md` là snapshot **05/09**, còn gọi KU-REV-001 là task hiện tại. Dùng ledger **27/09** cho tiến độ KU–OBP mới hơn.
- Foundation có **99 task được validator nhận diện**, trong đó **96 hoàn thành và 3 optional chưa làm**. Không viết “99 task hoàn thành” chỉ từ output validator.
- Một số báo cáo OBP/P5 lịch sử ghi task Blocked hoặc migration chưa bắt đầu. Ledger và các quyết định D-041/D-042 sau đó đóng task với phạm vi waiver rõ ràng; không xóa giới hạn qualification cũ.
- Mobile evidence header vẫn ghi `MOB-01`, README còn mô tả ABI 7, nhưng evidence rows/notes ghi Android Local Import ABI 13 và phần MOB-05B. Dùng các dòng evidence cụ thể để mô tả hiện trạng, không lấy header làm thước đo duy nhất. Không tự sửa specification/evidence trong lượt báo cáo này.
- Task `Merged` chỉ phản ánh acceptance trong phạm vi đã duyệt; `model_qualified=false`, `consumer_nat_qualified=false` và networking default-off vẫn là các giới hạn hiện hành.

**Mốc báo cáo nên theo dõi tiếp:** số task Merged; số model/resource tuples qualified; KU-QA và integration acceptance; qualification theo candidate/platform; trạng thái ReadyOffline của mobile. Những chỉ số này cho thấy khả năng sử dụng thực tế rõ hơn một phần trăm tổng hợp chưa có mẫu số.
