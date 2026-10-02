# Danh sách task và trạng thái OneBrain

**Cập nhật: 02/10/2026 · Ưu tiên: MVP chạy được để kiểm chứng ý tưởng và mời cộng đồng.**

Đây là bảng xem nhanh ngay trong task index có sẵn, tổng hợp từ
[PROGRESS](../PROGRESS.md). PROGRESS vẫn là sổ trạng thái chính;
[MASTER_PLAN](../MASTER_PLAN.md) quản lý phạm vi và thứ tự. Khi task đổi trạng
thái, cập nhật PROGRESS trước rồi đồng bộ bảng này; không tạo thêm bản theo ngày.

## Tổng quan đợt KU–OBP / MVP

| Trạng thái | Số task |
|---|---:|
| ✅ Đã merge | 22 |
| ⬜ Chưa bắt đầu | 0 |
| ⏸ Hoãn sau MVP | 2 |
| 🔄 Đang làm / 👀 Chờ review / ⛔ Bị chặn | 0 |
| **Tổng** | **24** |

**22/24 task đã merge (91,7%)**; nếu chỉ tính 22 task không hoãn thì là **100%**.
Đây là tỷ lệ đếm task của đợt này, không phải tỷ lệ toàn bộ dự án hay khối lượng
còn lại. Task 09 và task 20 đã được owner chấp nhận theo D-046, merge `8d064c4`
và xác minh push trên origin/main; checklist A–D hoàn tất.
“Đã merge” nghĩa là hoàn tất phạm vi đã chấp thuận, không phải mọi platform/model
đã đủ điều kiện production. Chuẩn bị qualification local vẫn được giữ dù task hoãn.

## Danh sách đầy đủ 24 task

Bấm mã task để mở nội dung và checklist. Số thứ tự là ID lịch sử, không phải thứ
tự cần thực hiện từ đầu.

| # | Task | Công việc | Trạng thái | Kết quả / phần còn lại |
|---:|---|---|---|---|
| 1 | [KU-REV-001](01-KU-REV-001.md) | Rà soát đặc tả KU và nguồn quy định chính | ✅ Đã merge | Đã chốt nền tảng để triển khai. |
| 2 | [KU-REV-002](02-KU-REV-002.md) | Đối chiếu mã nguồn, chức năng và phần thiếu | ✅ Đã merge | Đã có bản đồ hiện trạng KU. |
| 3 | [KU-CON-001](03-KU-CON-001.md) | Chốt quy trình sản phẩm KU dùng chung | ✅ Đã merge | Đã thống nhất hành vi giữa các giao diện. |
| 4 | [KU-RUN-001](04-KU-RUN-001.md) | Dịch vụ KU dùng chung trong node | ✅ Đã merge | Chuẩn bị, lưu, đọc, tìm kiếm và phục hồi. |
| 5 | [KU-API-001](05-KU-API-001.md) | API KU cục bộ | ✅ Đã merge | REST/WS dùng chung service. |
| 6 | [KU-CLI-001](06-KU-CLI-001.md) | Thao tác KU qua CLI | ✅ Đã merge | Đã tích hợp quy trình KU. |
| 7 | [KU-WEB-001](07-KU-WEB-001.md) | Thao tác KU qua Web | ✅ Đã merge | Có editor và luồng AI thử nghiệm; model chưa qualified. |
| 8 | [KU-DESK-001](08-KU-DESK-001.md) | Thao tác KU qua Desktop | ✅ Đã merge | Đã tích hợp; kiểm tra native/đa OS sâu để sau. |
| 9 | [KU-QA-001](09-KU-QA-001.md) | Demo KU local và kiểm tra vừa đủ | ✅ Đã merge | A–D PASS theo scope MVP: save/restart không model; 5 AI drafts với giới hạn thật. |
| 10 | [OBP-PROD-001](10-OBP-PROD-001.md) | Chốt quy trình networking OBP | ✅ Đã merge | Đã chốt contract tích hợp sản phẩm. |
| 11 | [OBP-PROD-002](11-OBP-PROD-002.md) | Node quản lý vòng đời networking | ✅ Đã merge | Một runtime chung quản lý start/stop. |
| 12 | [OBP-PROD-003](12-OBP-PROD-003.md) | Khởi tạo và tìm peer/relay | ✅ Đã merge | Đã nối bootstrap, discovery và reservation. |
| 13 | [OBP-PROD-004](13-OBP-PROD-004.md) | Định tuyến, hàng đợi gửi và relay dự phòng | ✅ Đã merge | Đã có outbox bền vững, failover và resume. |
| 14 | [OBP-API-001](14-OBP-API-001.md) | API networking | ✅ Đã merge | Đã cung cấp trạng thái và thao tác mạng. |
| 15 | [OBP-CLI-001](15-OBP-CLI-001.md) | Networking qua CLI | ✅ Đã merge | Đã tích hợp API networking. |
| 16 | [OBP-WEB-001](16-OBP-WEB-001.md) | Networking qua Web | ✅ Đã merge | Đã có giao diện trạng thái và điều khiển. |
| 17 | [OBP-DESK-001](17-OBP-DESK-001.md) | Networking qua Desktop | ✅ Đã merge | Dùng runtime node chung. |
| 18 | [OBP-QA-001](18-OBP-QA-001.md) | Kiểm tra chức năng OBP | ✅ Đã merge | Đã chấp thuận chức năng; consumer NAT chưa qualified. |
| 19 | [OBP-MIG-001](19-OBP-MIG-001.md) | Chuyển đường legacy seed sang chế độ tương thích | ✅ Đã merge | Đã merge; giữ dữ liệu cũ và rollback. |
| 20 | [INT-KU-OBP-001](20-INT-KU-OBP-001.md) | Demo KU qua hai node và hướng dẫn contributor | ✅ Đã merge | A–D PASS theo D-045: confirm riêng → Public CID/bytes khớp → restart/retry; nguồn private giữ local. |
| 21 | [KU-ENC-001](21-KU-ENC-001.md) | Khung encoder dùng chung | ✅ Đã merge | Đã chốt schema, workflow và compiler. |
| 22 | [KU-ENC-002](22-KU-ENC-002.md) | Triển khai encoder dùng chung | ✅ Đã merge | Đã có workflow/adapter; không đồng nghĩa chất lượng model đã đạt. |
| 23 | [KU-ENC-003](23-KU-ENC-003.md) | Đánh giá sâu model và tài nguyên | ⏸ Hoãn sau MVP | Giữ code chuẩn bị, dữ liệu và nhánh riêng; không chặn MVP. |
| 24 | [KU-SEM-001](24-KU-SEM-001.md) | Nâng chất lượng bản nháp ngữ nghĩa | ⏸ Hoãn sau MVP | Code nền đã vào main; các lỗi giữ nghĩa/activation còn để sau. |

## MVP đã chấp nhận và backlog kế tiếp

Các dòng dưới là substep của task 09 và 20, không phải task mới. Checklist nguồn
trong task 09 và task 20 đã đánh dấu A–D và được chấp nhận/merge theo D-046.
First-run onboarding tiếp theo là contributor backlog chưa bắt đầu, giữ trong task 20.

| Ưu tiên | Task cha / bước | Trạng thái | Kết quả cần thấy |
|---:|---|---|---|
| 1 | KU-QA-001 / A — kiểm tra setup hiện có | Hoàn tất local | Host build + source manual có consent thật; dataset/keys được giữ |
| 2 | KU-QA-001 / B — luồng KU local | Hoàn tất local | Web/API giữ cùng ID/bytes/receipt sau restart không model |
| 3 | KU-QA-001 / C — AI thử nghiệm nhỏ | Hoàn tất local | 5 câu: 3 extracted, 2 needs-review, 47–83 giây; không save |
| 4 | KU-QA-001 / D — kiểm tra và hướng dẫn | Hoàn tất local | Test/build/contracts PASS, hướng dẫn đã cập nhật; ghi baseline fmt/native limits |
| 5 | INT-KU-OBP-001 / A — nối hai node | Hoàn tất local | Hai runtime QUIC loopback riêng + host KU/API custody có sẵn; opt-in và sai NodeID được kiểm tra |
| 6 | INT-KU-OBP-001 / B — trao đổi KU | Hoàn tất local | Preview/confirm riêng → Public CID/bytes/semantic ID khớp; nguồn private không gửi; restart/retry PASS |
| 7 | INT-KU-OBP-001 / C — lối vào cho contributor | Hoàn tất scope operator demo | Hướng dẫn config/JSONL/sample, prerequisite Registry/keys thật, expected outcome/error và contribution nhỏ |
| 8 | INT-KU-OBP-001 / D — chốt demo và việc cộng đồng | Hoàn tất local | Task/ledger/overview ghi kết quả và giới hạn; share UI/host composition giữ trong backlog |

**Không chờ qualification model, reviewer độc lập, bộ bằng chứng hay ma trận
kiểm thử lớn để làm các bước này.** Model có thể trả bản nháp cần review;
luồng manual/resolved vẫn là đường lưu đáng tin cậy. Không gọi bản nháp chưa
được chấp nhận là KU đã lưu.

## Công việc ngoài đợt 24 task

Đây là trạng thái theo nhóm để nhìn toàn dự án; không cộng vào mẫu số 24 ở trên.
Chi tiết và backlog được giữ trong MASTER_PLAN và kế hoạch gốc tương ứng.

| Nhóm / ID có sẵn | Trạng thái tổng quan | Việc còn lại | Ưu tiên |
|---|---|---|---|
| Foundation vNext bắt buộc | Đã hoàn tất phạm vi foundation: 96 task | Giữ nền dùng chung; không cần làm lại | Nền đã có |
| RUN-003, RIB-001, RIB-002 | Chưa làm, tùy chọn | Remote cognition và tối ưu reconciliation | Sau MVP |
| P0–P3, DR-M5 | Đã có implementation và acceptance được ghi nhận | Qualification theo candidate/platform khi cần phát hành rộng | Tái dùng |
| Base v1 / Registry | Có release owner-waiver và công cụ Registry | Setup dễ hơn cho contributor; strict release là phần sau | Setup phục vụ MVP; strict để sau |
| Mobile MOB-00…09 | Triển khai một phần, BootstrapOnly/Limited | Registry thực tế, iOS/provider, AI/media/network và release; chưa ReadyOffline | Lane riêng sau MVP desktop/local |
| M6: distributed KQL, Outcome/Benefit | Có nền; chưa đóng milestone production | Tìm kiếm phân tán chủ động và chuỗi Use→Outcome→Benefit | Sau MVP |
| M7: OBT / wallet | Legacy/prototype | Chính sách reward, ledger/finality và wallet production | Sau MVP |
| Extension / bot / glasses | Scaffold | Adapter/sản phẩm thực tế dựa trên core chung | Cộng đồng / sau MVP |
| BCI | Nghiên cứu | Nghiên cứu và các điều kiện triển khai riêng | Dài hạn |

## Công việc chưa vào main / chưa push

Kiểm tra local và fresh `git fetch origin` ngày 02/10/2026 cho thấy:

| Nơi lưu | Trạng thái | Cách tính tiến độ |
|---|---|---|
| Main / origin-main | D-046 merge `8d064c4` + commit docs closure; đã push | Chứa 22 task Merged; model/NAT vẫn unqualified |
| Nhánh `codex/ku-enc-003-model-qualification` | 3 commit riêng: `89c5f33`, `6e4df3a`, `4a8f29d`; đã push, chưa merge | Công cụ preflight/tài liệu được giữ cho task 23, chưa phải qualification hoàn tất |
| Nhánh `codex/ku-enc-003-handoff` | Giữ tại `0b050a4`, đã trong main; root workspace hiện ở main | 17 dirty/untracked preparation/history/overview vẫn giữ local |
| Commit MVP chưa có trên origin | 0 sau push merge + docs closure | Không đồng nghĩa preparation local đã được publish |
| Worktree phụ `3bbf/OneBrain` | Sạch, detached `798eabf`, đã nằm trong main | Không thấy implementation riêng cần mang về |

Task 20 đã chạy trao đổi/restart thật trên Windows loopback theo D-045; task 09
được tái dùng đúng scope. Hai task đã Merged theo D-046 và push origin/main. Không có
blocker acceptance local; Registry/setup và share product UX tiếp tục ở backlog.
