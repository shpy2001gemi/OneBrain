# Danh sách task và trạng thái OneBrain

**Cập nhật: 18/10/2026 · Ưu tiên: MVP chạy được để kiểm chứng ý tưởng và mời cộng đồng.**

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
| 20 | [INT-KU-OBP-001](20-INT-KU-OBP-001.md) | Demo KU qua hai node và hướng dẫn contributor | ✅ Đã merge | A–D PASS theo D-045; năm first-run slices đã merge/push theo D-047; ba diagnostics manual provisioning đã merge/push theo D-048 (`403bbbb`). Helper tạo secrets cho dataset mới đã merge/push theo D-049 (`ea92e8a`); commit/state trong PROGRESS; Registry CLI public-key diagnostics đã merge/push qua `2b14b23` theo D-050; received-package verify guidance đã commit/push `e77202c`; composed config UTF-8 BOM + guide PowerShell đã commit/push `832425b`; normal-host public reader + shared D-045 share seam đã commit/push `2eb30d4`; registered editor (`POST /api/vnext/ku/editor`) D-045 share actions đã commit/push `bef0d28`; `/ku` Web share UI & transport client đã commit/push `25b89c5`; `ku_local_web` opt-in loopback network config đã commit/push `5dfc6bd`; `VNextProductRuntime` (`OneBrainNode` / Desktop) binding với `CanonicalPublicReader` và `Network` lane fence đã commit/push `fbcda30`; contributor entry refresh (`CONTRIBUTING.md` + D-045 section) đã commit/push `ef9b459`; UX lỗi expired-preparation / acquisition còn mở. |
| 21 | [KU-ENC-001](21-KU-ENC-001.md) | Khung encoder dùng chung | ✅ Đã merge | Đã chốt schema, workflow và compiler. |
| 22 | [KU-ENC-002](22-KU-ENC-002.md) | Triển khai encoder dùng chung | ✅ Đã merge | Đã có workflow/adapter; không đồng nghĩa chất lượng model đã đạt. |
| 23 | [KU-ENC-003](23-KU-ENC-003.md) | Đánh giá sâu model và tài nguyên | ⏸ Hoãn sau MVP | Giữ code chuẩn bị, dữ liệu và nhánh riêng; không chặn MVP. |
| 24 | [KU-SEM-001](24-KU-SEM-001.md) | Nâng chất lượng bản nháp ngữ nghĩa | ⏸ Hoãn sau MVP | Code nền đã vào main; các lỗi giữ nghĩa/activation còn để sau. |

## MVP đã chấp nhận và backlog kế tiếp

Các dòng dưới là substep của task 09 và 20, không phải task mới. Checklist nguồn
trong task 09 và task 20 đã đánh dấu A–D và được chấp nhận/merge theo D-046.
First-run onboarding giữ trong task 20: bản sửa Registry/config/token/Vault key/source và hướng dẫn catalog rỗng đã kiểm tra và
đã merge `cbb7d58` và push origin/main theo D-047, tách khỏi ENC-003;
không đổi tổng 22 task đã merge / 2 hoãn.
Ba follow-up diagnostics request JSON, nguồn/output và lỗi ghi sau `create_dir`
của `ku_manual_source` đã merge/push theo D-048 qua `403bbbb`, từ tip `4d01775`.
Retry lỗi ghi giữ custody dở dang và chọn thư mục mới; writes vẫn non-transactional.
Review tích hợp ba slices đã hoàn tất; phạm vi commit và kiểm tra bảo toàn được ghi
trong [task 20](20-INT-KU-OBP-001.md#manual-provisioning-integration-review--2026-10-03).
Đã tích hợp theo owner authorization D-048; không đổi tổng số task đã merge.

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
| 9 | INT-KU-OBP-001 / first-run — Registry/config/token/Vault key/source/catalog rỗng | Đã merge/push theo D-047 | `cbb7d58` khớp tip reviewed; post-merge vNext/whitespace PASS; reuse Rust 5/5, Web 16/16, builds + private bytes/receipt/restart; ENC-003 giữ ngoài commit, provisioning/composition rộng hơn còn mở |
| 10 | INT-KU-OBP-001 / first-run — manual provisioning request + nguồn/output + later writes | Đã merge/push theo D-048 | `403bbbb` khớp tip `4d01775`; post-merge vNext/whitespace PASS; 16 hashes + 108 dòng history giữ nguyên. Reuse Rust 6/6, build + 3 smoke calls, 9 source/output và 14 request calls trước. Lỗi ghi giữ output dở dang/retry thư mục mới; consent/exact bytes/custody cũ giữ nguyên; writes vẫn non-transactional |
| 11 | INT-KU-OBP-001 / first-run — secrets cho dataset mới | Đã merge/push theo D-049 | `ea92e8a` khớp tip reviewed `f3780c7`; post-merge vNext/whitespace/retained-work PASS. Helper Python sinh Vault key/token bằng OS randomness; từ chối dataset/custody cũ, không tạo dataset hay in secrets. 4/4 regression + 3 CLI/1 host call, build/vNext PASS; Windows ACL do operator quản lý, pair writes non-transactional; Registry/composition còn mở |
| 12 | INT-KU-OBP-001 / first-run — input public key của Registry CLI | Đã merge/push theo D-050 | `2b14b23` khớp reviewed tip `6bc146c`, gồm implementation/guide `645c6bc`; post-merge vNext/whitespace/exact tree PASS. Reuse Rust 2/2 + signer/artifact 1/1, build/4 CLI refusals; 16 hashes + 108 prep additions giữ ngoài commits. D-050 cho phép merge/push thường trực; tổng 22 Merged / 2 Deferred không đổi |
| 13 | INT-KU-OBP-001 / first-run — nhận và verify signed package | Đã commit/push theo D-050 | `e77202c`: hướng dẫn I/O/exact-file-set của read-only verify + layout sáu file, trust độc lập. Rust 3/3 + signer/artifact 1/1, build/12 CLI calls/vNext PASS; bytes/directories/generation fixture giữ nguyên. Không endpoint hay Registry bundle; normal host composition còn mở |
| 14 | INT-KU-OBP-001 / first-run — ghép config host bằng PowerShell | Đã commit/push theo D-050 | `832425b`: chấp nhận một UTF-8 BOM trong 64 KiB, giữ schema; guide ghép trusted public-key file + custody paths, NoClobber. Example 6/6, saved-read restart 1/1, build/vNext/rustfmt + PS5.1/two isolated hosts PASS; config/Registry/secrets giữ nguyên. Normal host sharing còn backlog |
| 15 | INT-KU-OBP-001 / follow-up — normal-host public reader + shared D-045 share seam | Đã commit/push theo D-050 | `2eb30d4`: nối `CanonicalPublicReader::for_data_dir` vào `ku_host` (`get` + `CanonicalPublicExchange` `export` cho Public KU đã nhận, giữ `semantic_content_cid`) và dùng chung `PublicKuShareCoordinator` + `prepare_host_public_ku_share`/`confirm_host_public_ku_share` giữa normal host và `ku_peer_demo`. `ku_public_share` 4/4, `ku_product` 1/1, `ku_host` 1/1 + 2/2 feature, `ku_local_web` 6/6, `ku_peer_demo` check, KU/vNext/rustfmt PASS. Không thêm HTTP route chưa đăng ký; API/Web share UI còn backlog |
| 16 | INT-KU-OBP-001 / follow-up — registered editor (`POST /api/vnext/ku/editor`) D-045 share projection | Đã commit/push theo D-050 | `bef0d28`: chiếu `share_prepare`, `share_confirm`, `share_intent`, `share_inspect` lên `POST /api/vnext/ku/editor` (`KuServices::editor`), giữ closed schema, `recovery` read-only cho `share_inspect`/`share_intent`, và `503 capability_disabled` (`ku_public_share_network_unavailable`) khi chưa gắn opt-in network runtime. `onebrain-api` 39/39 + 42/42 feature (`ku_host` 3/3), `ku_public_share` 4/4, `ku_product` 19/19, `ku_local_web` 6/6, `ku_peer_demo` check, KU/vNext/rustfmt PASS. Web/CLI share UI còn backlog |
| 17 | INT-KU-OBP-001 / follow-up — `/ku` Web Public KU share & peer inspection UI | Đã commit/push theo D-050 | `25b89c5`: nối `sharePrepare`, `shareConfirm`, `shareIntent`, `shareInspect` trong `src/onebrain-web/src/api/ku.ts` và `KuWorkflow.tsx` (`kuFeedback.ts`), giữ preview → confirm tách biệt (`confirm_public: true`), bảo toàn `prepared_id` khi `503 capability_disabled` (`ku_public_share_network_unavailable`), ẩn private CID/source trong share panels và phân biệt pending intent với delivery. `npm run test:ku` 92/92 (`kuWorkflow` 17/17 + `axe`, `kuTransport` 2/2), `npm run build`, `npm run lint`, KU/vNext PASS |
| 18 | INT-KU-OBP-001 / follow-up — `ku_local_web` opt-in loopback network config | Đã commit/push theo D-050 | `5dfc6bd`: thêm `KuHostNetworkConfig { network_opt_in, bind_addr }` (mặc định `None` khi không khai báo) và `start_host_loopback_network_runtime` trong `ku_host.rs` / `examples/ku_local_web.rs`, kiểm tra `network_opt_in == true` + loopback trước khi chạm Registry/dataset. `ku_local_web` 7/7 + 8/8 feature, `ku_host` 3/3, KU/vNext/rustfmt PASS |
| 19 | INT-KU-OBP-001 / follow-up — `VNextProductRuntime` (`OneBrainNode` / Desktop) `CanonicalPublicReader` binding & `Network` lane fence | Đã commit/push theo D-050 | `fbcda30`: đăng ký `Arc<VNextNetworkRuntime>` trong `VNextProductRuntime::start_with_paths` cho cả `paths.network.canonical` và `paths.operational`, kiểm tra `is_network_lane_active()` (`VNextRuntimeLane::Network`) cho `share_confirm`/`share_intent`/`share_inspect`, và đánh thức outbox khi bật lại `Network`. `onebrain-desktop --test lifecycle` 8/8 + 9/9 `vnext-network-runtime` + 11/11 `vnext-outbound-first`, `ku_product` 19/19, `ku_host` 3/3, Base/vNext/rustfmt PASS |
| 20 | INT-KU-OBP-001 / follow-up — cập nhật `CONTRIBUTING.md` và hướng dẫn D-045 trong task 20 | Đã commit/push theo D-050 | `ef9b459`: cập nhật `CONTRIBUTING.md` và `### Run the local-to-peer manual concept (D-045)` để hiển thị cả hai cách chạy D-045 (trực tiếp qua `/ku` Web + `ku_local_web` opt-in loopback `"network"` hoặc `OneBrainNode`/`onebrain-desktop` `VNextProductRuntime`, và qua helper `ku_peer_demo`). vNext validator, local links và whitespace PASS |

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

Kiểm tra local/worktree và `git ls-remote --heads origin` trực tiếp ngày
18/10/2026 07:55 (Asia/Saigon); fresh fetch main đầu lượt, scoped verify trực tiếp sau push:

| Nơi lưu | Trạng thái | Cách tính tiến độ |
|---|---|---|
| Main / origin-main | D-050 Registry merge `2b14b23`, received-package `e77202c`, composed-config `832425b`, normal-host share seam `2eb30d4`, registered editor share projection `bef0d28`, `/ku` Web share UI `25b89c5`, `ku_local_web` opt-in loopback network config `5dfc6bd`, `VNextProductRuntime` `CanonicalPublicReader` binding `fbcda30`, contributor entry refresh `ef9b459` + docs closure tại HEAD; push/verify trong cùng lượt | Chứa 22 task Merged; model/NAT vẫn unqualified |
| Nhánh `codex/ku-enc-003-model-qualification` | 3 commit riêng: `89c5f33`, `6e4df3a`, `4a8f29d`; đã push, chưa merge | Công cụ preflight/tài liệu được giữ cho task 23, chưa phải qualification hoàn tất |
| Nhánh `codex/ku-enc-003-handoff` | Giữ tại `0b050a4`, đã trong main | Giữ nhánh MVP; preparation vẫn local |
| Root / `main`; Registry branch `6bc146c`, secrets `f3780c7`, diagnostics `4d01775` giữ local | Gói scoped tích hợp qua `2b14b23`/`ea92e8a`/`403bbbb` và push; source branches giữ nguyên | Task23 + 15 untracked + 108 dòng preparation vẫn giữ riêng |
| Commit MVP/manual/secrets/Registry follow-ups chưa có trên origin | 0 sau D-050 merge + closure push | Preparation local vẫn ngoài integration commits |
| Worktree phụ `3bbf/OneBrain` | Sạch, detached `798eabf`, đã nằm trong main | Không thấy implementation riêng cần mang về |

Task 20 đã chạy trao đổi/restart thật trên Windows loopback theo D-045; task 09
được tái dùng đúng scope. Hai task đã Merged theo D-046 và push origin/main. Không có
blocker acceptance local; Registry/setup và UX lỗi expired-preparation tiếp tục ở backlog.
