# Trạng thái dự án OneBrain

> **Định hướng hiện hành — D-044 / D-050 / D-051:** ưu tiên hoàn thiện và mở rộng
> bản concept MVP mã nguồn mở chạy được trên kiến trúc dùng chung, không biến các
> chiến dịch qualification dài hạn thành điểm nghẽn cho các đóng góp có giới hạn.
> Xem [kế hoạch duy nhất](handoffs/2026-09-ku-obp-productization/MASTER_PLAN.md)
> về phạm vi/thứ tự và [PROGRESS.md](handoffs/2026-09-ku-obp-productization/PROGRESS.md)
> là sổ trạng thái task, nhánh và worktree chính thức duy nhất.

[English](PROJECT_STATUS.md)

> Snapshot: **2026-11-27 (Asia/Saigon)**
>
> Nguồn được audit: `main` sau khi tích hợp đồng bộ 7 đợt (`Waves 1–7`) phủ kín `Core + Runtime + Software Prod-Readiness` cho **9/10 trụ cột hoạt động (`P1`–`P8`, `P10`)** (`514,7k` tổng LOC / `461,0k` `src/` LOC, `25` crate Rust, `4.140+` bài kiểm thử tự động)
>
> Phạm vi: các luồng công việc của repository, củng cố kiến trúc (`14/15`), phân biệt rõ hoàn tất phần mềm 9 trụ cột với các cổng máy vật lý/operator/reviewer bên ngoài, bộ kiểm tra contract và bằng chứng CI.

Specification mô tả hành vi bắt buộc hoặc mục tiêu; cài đặt và bộ test chứng minh hệ thống vận hành đúng trong phần mềm, trong khi kiểm thử đa máy vật lý, đánh giá độc lập của con người và lễ ký khóa của operator là các cổng riêng. Xem [`specs/vnext/README.md`](specs/vnext/README.md) để tra cứu cột `Reachability` từng hợp đồng.

## Trạng thái tổng quan

| Luồng công việc | Trạng thái hiện tại | Phần còn mở |
|---|---|---|
| Concept MVP (`KU–OBP`, D-044 / D-045 / D-046) | **22/24 task Đã merge trên `main`; 2 Hoãn.** Luồng KU riêng tư local lưu/tìm/khởi động lại (`KU-QA-001`), chia sẻ Public KU tường minh giữa hai node (`INT-KU-OBP-001`), bootstrap một lệnh DEV-ONLY từ fresh clone (`D-056`), đồng bộ `/ku` trên CLI/Web/Desktop (`onebrain ku share`, `describe_ccids` `D-058`, `share_preview` `D-059`, prepared share bền vững qua restart `D-062`, tạo nháp riêng tư mới từ Public KU `D-057`, bind mạng LAN riêng DEV-ONLY opt-in `D-063`), hạ v2 `D-011` (`review_lowering_v2`) và CI hai node `CLI↔CLI` / `Web↔Web` đa hệ điều hành (`ku-dev-demo-scripts.yml`) đều đã merge và xanh. | Phân phối gói Concept Registry thật do operator ký cho contributor; kiểm tra thực tế hai máy vật lý qua LAN (`D-063`); đánh giá holdout đa họ model chính thức (`KU-ENC-003`, `model_qualified=false`); độ trung thực ngữ nghĩa rộng do người chấm (`KU-SEM-001`, Hoãn — đã merge đủ 18/18 lát sửa cơ học `SEM-F03` đến `D-075`). |
| Củng cố kiến trúc (`D-051`, `ARCH-01..15`) | **14/15 hạng mục Hoàn tất trên `main`** (`ARCH-01..13`, `ARCH-15`): vòng đời outer-auth của relay (`ARCH-01`), tắt mặc định TCP legacy không xác thực (`ARCH-02` / `D-052`), nén journal KU & bản ghi terminal Base (`ARCH-03` / `ARCH-15`), đóng băng KQL chuỗi chỉ-đọc (`ARCH-04`), cột Reachability cho spec (`ARCH-05`), một mô hình `/ku` phía người dùng & cờ `legacy` (`ARCH-06`), hạ claim đã review (`ARCH-07` + `D-011` v2), tìm kiếm KU trên `vnext_semantic_index` (`ARCH-08`), tách `onebrain-node` độc lập host với 0 bypass phân tầng (`ARCH-09`), **tách nhân đồng bộ `onebrain-core` (`0` `tokio`, `0` `std::net`) dùng chung cho cả `onebrain-node` và `onebrain-mobile-core` (`ARCH-10`)**, gate `experimental` cho KQL + tách crate `onebrain-blob-store` / `onebrain-legacy-ku-store` (`ARCH-11`), bản đồ recovery unit & bỏ theo dõi DB mẫu (`ARCH-12`), và thay kiểm tra chuỗi nguồn bằng Rust test gắn profile (`ARCH-13`). | Chỉ còn `ARCH-14` (hardening thực địa consumer-NAT trên cụm VPS mới) đang **Bị chặn** do phụ thuộc hạ tầng VPS vật lý. |
| Khép kín Phần mềm 9 Trụ cột (`P1`–`P8`, `P10` — `Core + Runtime + Software Prod`) | **Hoàn tất đồng bộ trong phần mềm qua 7 đợt phụ thuộc ngang (`nine_pillar_cross_runtime_suite` 6/6 PASS).** Gồm `P1` `ConceptLifecycleTable` + cô lập va chạm CCID `.obr` v2 + kéo gói `D-012` + tổng hợp Receptor/Affordance/Assembly/Mapping; `P2` đối soát nhanh `RIBLT-1` + `FrontierSummaryPage` + hộp thư E2EE bền vững `obp/mailbox/1` + truyền khối `obp/blob-chunks/1`; `P3` trình biên dịch tất định `FIND → KnowledgeNeedIr` (`/api/vnext/kql/compile`) + khám phá đa đường `M6A`; `P4` publisher 2 bước `M6B` `Derivation`/`Outcome`/`Benefit` + `DeferredEvidenceStaging` + gom cụm Sybil 4 chiều; `P5` crate cô lập `onebrain-obt` (toán nguyên `u128`, tường lửa `RewardEvidenceNoticeV2`, `ScopedNullifier`, mặc định Shadow Mode); `P6` `RUN-003` `obp/cognition/1` + hợp đồng thuê mù `D-013` + cầu nối PAM Vault 4 tầng; `P7` đồ thị 5 mặt phẳng `OBKG-002` (`/api/vnext/obkg/snapshot`) + `DreamConsolidatorV1` + KGE gắn CCID; `P8` đóng đủ `OBARV002` `0x0002ns2` & `0x000B..0x000F` (gồm sàn chống ký phân nhánh `highest_issued_seq`) + 4 pha khôi phục Redb + xoay vòng `TombstoneEpochAccumulatorV1` + dọn rác cục bộ `CHK-003..006`; `P10` `obp/personal-mesh/1` + trần quyền `obc_surface1` trên API/CLI/Web/Desktop/Mobile và `onebrain-extension`, `onebrain-bot`, `onebrain-glasses`. | Các lane mạng/nhận thức từ xa/sổ cái vẫn tắt mặc định (`VNextExtendedFeatureFlags`); OBT mặc định chạy `M7.1` Shadow Mode (`0` phát hành chi tiêu được); thử nghiệm đa máy vật lý, mạng sóng di động thật và kiểm toán đối kháng độc lập giữ cổng riêng. |
| Contract và foundation vNext (`ARCH-10P-001`) | **Hoàn tất đặc tả chuẩn hóa cho cả 10/10 trụ cột.** `validate_vnext_contracts.py` và `validate_ten_pillar_contracts.py` xác nhận đủ 10/10 hồ sơ đặc tả trụ cột (`P1`–`P10`), 10 tường lửa liên trụ cột (`FW-01`–`FW-10`), 15 cạnh thẩm quyền bất tuần hoàn, 99 task, 18 ADR, 37 + 12 negative assertion, 56 foundation vector thuộc 22 domain, 985 dòng quy phạm và 708+ liên kết nội bộ. | `P9` (`P9-BCI-SAFETY-001`) chủ đích giữ `spec-only (CI-checked)` với `0` LOC runtime cho tới khi có phần cứng thần kinh chuẩn hóa và chứng nhận an toàn sinh học độc lập. |
| Tích hợp sản phẩm P0–P3 và DR-M5 | **Implementation và bằng chứng CI đã ghi nhận hoàn tất.** Runtime ownership, REST/private WebSocket/CLI/Desktop-Web, resource admission, observability, crash, chaos/fuzz, compaction, rollback và bằng chứng soak M5-07 đã chấp nhận đều có trong `main`. | Các lane mạng/discovery/PoMV vNext vẫn opt-in/default-off; chưa claim `consumer_nat_qualified=true`. |
| Base v1 | **Đã phát hành với các ngoại lệ được chủ dự án chấp thuận và công khai** bằng signed tag `base-v1.0.0-owner-waiver.1` tại `1e0fb2321aee`. Trên `main` hiện tại, đã vá các lỗ hổng lockfile (`250fd0d`), phân loại P3 cho các advisory unmaintained (`2551eee`) và workflow `Base v1 three-OS` chạy xanh hoàn toàn (`45763665749`). | Không có strict tag `base-v1.0.0`; dự án tuyên bố rõ **không** claim `BASE-GATE-V1 qualified=true`. |
| Concept Registry và mạng P5 | **Hoàn tất đường ống đóng gói/kéo gói Registry v2 bằng phần mềm và bằng chứng tham chiếu P5.** | Trạng thái strict trong source giữ nguyên trung thực: vector Registry giữ `production_qualified=false` chờ gói ký thật của operator; P5 giữ `provider-document-pending`, `non-linux-platform-lanes-pending`, `mobile-carrier-mailbox-pending`. |
| Mobile (`MOB-01..09`) | **Hoàn tất phần mềm xuyên suốt `MOB-01..09` (`BootstrapOnly/Limited`).** Tái sử dụng `onebrain-core` (`ARCH-10`) và `onebrain-archive` (`OBARV002`); tổng hợp KU riêng tư (`MOB-04`), kích hoạt/rollback Registry (`MOB-05B/C`), vòng đời AI/media cục bộ (`MOB-06/07`), engine mạng khóa cổng (`MOB-08`) và trình gác cổng release (`MOB-09`) đều xanh (`49/49` Rust, `16/16` unit/widget, `14/14` golden, `validate_mobile_build_contracts.py` PASS với `108` dòng evidence, `190` ID, `0` sai lệch). | Đo kiểm trên điện thoại Android/iOS vật lý, nạp gói `.obr` 2,2 GB thật của operator và ký phát hành kho ứng dụng vẫn mở; chưa claim `ReadyOffline`. |

## Ranh giới release Base v1 (hồ sơ waiver lịch sử + trạng thái CI hiện tại)

Annotated tag `base-v1.0.0-owner-waiver.1` (audit ngày 2026-09-05) ghi nhận:

- candidate commit `1e0fb2321aeec04cb711f4259e2bc807e73a35dd`;
- run ba hệ điều hành
  [33529983318](https://github.com/shpy2001gemi/OneBrain/actions/runs/33529983318) thành công;
- Task 28 run
  [33592716241](https://github.com/shpy2001gemi/OneBrain/actions/runs/33592716241) thành công;
- prebuilt Registry root, P5 root và soak root 72 giờ liên tục;
- các ngoại lệ công khai gồm evidence assembly muộn, sửa tên frozen test target,
  các phát hiện Clippy all-features đã tồn tại và dependency-policy triage.

Tag có chữ ký PGP; audit local chỉ ghi nhận tag và nội dung mà không tuyên bố tự
xác minh khóa PGP khi chưa cài public key của signer. Ngày 25/11/2026 (`ci2`/`ci3`:
`0eaf719`, `f147232`, `250fd0d`, `2551eee`), toàn bộ cảnh báo Clippy `-D warnings`
trên stable `1.100`, xử lý mã lỗi PowerShell CI, 17 lỗ hổng lockfile + 2 cảnh báo
unsound và phân loại P3 được owner duyệt cho 5 advisory unmaintained đã được xử lý,
giúp workflow `Base v1 three-OS qualification candidate` chạy xanh toàn trình trên
`main` (gồm cả job closure), nhưng không tạo claim phát hành strict `base-v1.0.0` mới.

## Định hướng triển khai và backlog hiện hành

Theo các quyết định của owner **D-044** (28/09/2026), **D-050** (04/10/2026) và
**D-051** (18/10/2026):

1. **Một kế hoạch và một sổ tiến độ duy nhất.** [`MASTER_PLAN.md`](handoffs/2026-09-ku-obp-productization/MASTER_PLAN.md)
   quản lý phạm vi, lộ trình củng cố kiến trúc và hàng đợi cho contributor;
   [`PROGRESS.md`](handoffs/2026-09-ku-obp-productization/PROGRESS.md) giữ khối
   checkpoint hiện hành, trạng thái task và kiểm kê nhánh/worktree được bảo toàn.
2. **Đóng góp theo từng lát nhỏ, kiểm chứng vừa đủ.** Khi cả 22 task KU–OBP không
   hoãn và 13/15 hạng mục `ARCH-*` đã merge, công việc tiếp theo chọn từng hạng mục
   nhỏ từ backlog trong `MASTER_PLAN.md` — như các lát sửa ngữ nghĩa có giới hạn của
   [`KU-SEM-001`](handoffs/2026-09-ku-obp-productization/outputs/KU_SEM_001_CONTRIBUTOR_BACKLOG.md),
   thử nghiệm hai máy vật lý trên LAN riêng (`D-063`), click-test Desktop `/ku`
   (`D-057`) hoặc cải thiện onboarding cho contributor — đồng thời giữ nghiêm
   `encode ≠ publish`, `proposal ≠ materialize`, `materialize ≠ adopt` và ranh giới
   kho riêng tư.
3. **Các lane có gate riêng vẫn tách biệt.** Qualification chính thức cho encoder
   (`KU-ENC-003`), strict Base/Registry release, hardening NAT/VPS thực địa
   (`ARCH-14`), mobile (`src/onebrain-mobile/AGENTS.md` + mobile build contract;
   `ARCH-10` đang tạm hoãn), M6 distributed KQL/Outcome/Benefit, M7/OBT và BCI giữ
   nguyên điều kiện mở gate riêng.

Thứ tự triển khai mobile chi tiết nằm trong
[`WIP_MOBILE_APP_IMPLEMENTATION_PLAN_V1.md`](research/WIP_MOBILE_APP_IMPLEMENTATION_PLAN_V1.md).
Lịch sử và các gate distributed runtime nằm trong
[`WIP_DISTRIBUTED_RUNTIME_IMPLEMENTATION_PLAN_V2.md`](research/WIP_DISTRIBUTED_RUNTIME_IMPLEMENTATION_PLAN_V2.md).

## Tóm tắt nhánh Git và công việc đang bảo toàn

- Đợt dọn dẹp trước productization ngày **05/09/2026** (`c65f1739fcd0`) đã xác
  minh toàn bộ 46 nhánh local lịch sử (Base/DR-M5/P1–P3) đều nằm trong
  `origin/main` và gỡ các nhánh đã merge khỏi local mà không xóa remote branch hay tag.
- Đợt productization KU–OBP và chuẩn bị qualification từ tháng 9 đến tháng 11/2026
  đã giữ lại các nhánh tham chiếu theo quyết định owner, một worktree phụ sạch
  (`.codex/worktrees/3bbf/OneBrain` tại `798eabf`, đã nằm trong `main`) và bộ
  script/báo cáo chuẩn bị `KU-ENC-003` tại local (`4a8f29d` + các file untracked
  `scripts/encoder/qualification_*`). Bảng kiểm kê đầy đủ và chính thức duy nhất
  nằm trong [`PROGRESS.md`](handoffs/2026-09-ku-obp-productization/PROGRESS.md) —
  tuyệt đối không tự ý xóa hoặc ghi đè các phần việc đang bảo toàn này.

## Bằng chứng validation của snapshot (2026-11-27)

- `python scripts/ci/validate_vnext_contracts.py` & `python scripts/ci/validate_ten_pillar_contracts.py` — **PASS** (10/10 trụ cột, 10 tường lửa, 15 cạnh thẩm quyền bất tuần hoàn, 99 task, 18 ADR,
  37 + 12 negative assertion, 56 foundation vector/22 domain, 985 dòng quy phạm,
  708+ liên kết nội bộ).
- `python scripts/ci/validate_mobile_build_contracts.py` — **PASS** (11 tài liệu
  thẩm quyền, `108` dòng evidence, `190` ID, 123 tính năng mobile, 112 screen, 62
  component, 13 pattern, 0 liên kết lỗi, 0 vi phạm source-guard, `verify_native_release_guards.py` PASS).
- `python scripts/ci/check_node_layering.py` — **PASS** (`onebrain-node` độc lập
  host; 0 bypass phân tầng ARCH-09; `onebrain-core` sạch `tokio` và `std::net`).
- Bộ kiểm thử tích hợp phần mềm xuyên 9 trụ cột:
  - `onebrain-node --test nine_pillar_cross_runtime_suite` (`6/6`), `onebrain-obt --test p5_obt_economy_suite` (`11/11`), `onebrain-api --test p10_surfaces_api`, `onebrain-web` (`163/163` vitest, `0` cảnh báo `oxlint`), `onebrain-extension` (`9/9`), `onebrain-bot` (`4/4`), `onebrain-glasses` (`7/7`), `onebrain-mobile-core`/`bridge` (`49/49`) + Flutter (`16/16` unit/widget, `14/14` golden) — **PASS**.
- Các lượt chạy GitHub Actions gần nhất trên `main`:
  - `Base v1 three-OS qualification candidate` tại `bc1934d`:
    [45763665749](https://github.com/shpy2001gemi/OneBrain/actions/runs/45763665749) — **success** (và `45759117567` tại `fc75ff2` — **success**);
  - `vNext foundation contract` tại `570761e` (D-069):
    [45760738065](https://github.com/shpy2001gemi/OneBrain/actions/runs/45760738065) — **success** (6/6 job);
  - `KU dev demo scripts` (Linux, macOS, Windows hai node `CLI↔CLI` và `Web↔Web`) tại `56e5101` (t22):
    [45753417707](https://github.com/shpy2001gemi/OneBrain/actions/runs/45753417707) — **success**.

Snapshot này không tuyên bố đã chạy lại toàn bộ workspace test, physical-device
test, đánh giá holdout của model hay mọi strict production qualification gate trên
máy local.
