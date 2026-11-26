# Trạng thái dự án OneBrain

> **Định hướng hiện hành — D-044 / D-050 / D-051:** ưu tiên hoàn thiện và mở rộng
> bản concept MVP mã nguồn mở chạy được trên kiến trúc dùng chung, không biến các
> chiến dịch qualification dài hạn thành điểm nghẽn cho các đóng góp có giới hạn.
> Xem [kế hoạch duy nhất](handoffs/2026-09-ku-obp-productization/MASTER_PLAN.md)
> về phạm vi/thứ tự và [PROGRESS.md](handoffs/2026-09-ku-obp-productization/PROGRESS.md)
> là sổ trạng thái task, nhánh và worktree chính thức duy nhất.

[English](PROJECT_STATUS.md)

> Snapshot: **2026-11-26 (Asia/Saigon)**
>
> Nguồn được audit: `main` / `origin/main` tại `bc1934d48825687de82ea8304138576873497dc4`
> trước lần cập nhật tài liệu này
>
> Phạm vi: các luồng công việc của repository, đợt củng cố kiến trúc, ngoại lệ
> release, bộ kiểm tra contract và bằng chứng CI GitHub Actions đã xác minh.

Specification mô tả hành vi bắt buộc hoặc mục tiêu; bản thân specification không
phải bằng chứng rằng một đường production đã hoàn tất. Xem cột `Reachability`
trong [`specs/vnext/README.md`](specs/vnext/README.md) để phân biệt rõ
`default-build`, `feature-gated`, `library-only` và `spec-only`.

## Trạng thái tổng quan

| Luồng công việc | Trạng thái hiện tại | Phần còn mở |
|---|---|---|
| Concept MVP (`KU–OBP`, D-044 / D-045 / D-046) | **22/24 task Đã merge trên `main`; 2 Hoãn.** Luồng KU riêng tư local lưu/tìm/khởi động lại (`KU-QA-001`), chia sẻ Public KU tường minh giữa hai node (`INT-KU-OBP-001`), bootstrap một lệnh DEV-ONLY từ fresh clone (`D-056`), đồng bộ `/ku` trên CLI/Web/Desktop (`onebrain ku share`, `describe_ccids` `D-058`, `share_preview` `D-059`, prepared share bền vững qua restart `D-062`, tạo nháp riêng tư mới từ Public KU `D-057`, bind mạng LAN riêng DEV-ONLY opt-in `D-063`) và CI hai node `CLI↔CLI` / `Web↔Web` đa hệ điều hành (`ku-dev-demo-scripts.yml`) đều đã merge và xanh. | Phân phối gói Concept Registry thật do operator ký cho contributor; kiểm tra thực tế hai máy vật lý qua LAN (`D-063`); đánh giá holdout đa họ model chính thức (`KU-ENC-003`, `model_qualified=false`); độ trung thực ngữ nghĩa rộng và kích hoạt v2 trên host người dùng (`KU-SEM-001`, Hoãn — đã merge 8 lát sửa có giới hạn `SEM-F03` đến `D-069`). |
| Củng cố kiến trúc (`D-051`, `ARCH-01..15`) | **13/15 hạng mục Hoàn tất trên `main`** (`ARCH-01..09`, `ARCH-11..13`, `ARCH-15`): vòng đời outer-auth của relay (`ARCH-01`), tắt mặc định TCP legacy không xác thực (`ARCH-02` / `D-052`), nén journal KU & bản ghi terminal Base (`ARCH-03` / `ARCH-15`), đóng băng KQL chuỗi chỉ-đọc (`ARCH-04`), cột Reachability cho spec (`ARCH-05`), một mô hình `/ku` phía người dùng & cờ `legacy` (`ARCH-06`), hạ claim đơn giản đã review (`ARCH-07`), tìm kiếm KU trên `vnext_semantic_index` (`ARCH-08`), tách `onebrain-node` độc lập host với 0 bypass phân tầng (`ARCH-09`), gate `experimental` cho KQL + tách crate `onebrain-blob-store` / `onebrain-legacy-ku-store` (`ARCH-11`), bản đồ recovery unit & bỏ theo dõi DB mẫu (`ARCH-12`), và thay kiểm tra chuỗi nguồn bằng Rust test gắn profile kèm gộp archive byte facade (`ARCH-13`). | `ARCH-10` (runtime dùng chung cho mobile) đang **Tạm hoãn** (`D-054`) sau lát 1 (`MOB-03` dùng chung archive `OBARV002`, `0b6d8d0`) và bản đồ cắt 6 cạnh; `ARCH-14` (hardening thực địa consumer-NAT / VPS) đang **Bị chặn** chờ đăng ký cụm VPS mới. |
| Contract và foundation vNext | **Hoàn tất đặc tả chuẩn hóa cho cả 10/10 trụ cột (`ARCH-10P-001`) ở phạm vi contract/foundation.** `validate_vnext_contracts.py` và `validate_ten_pillar_contracts.py` xác nhận đủ 10/10 hồ sơ đặc tả trụ cột (`P1`–`P10`), 10 tường lửa liên trụ cột (`FW-01`–`FW-10`), 15 cạnh thẩm quyền bất tuần hoàn, 99 task, 18 ADR, 37 + 12 negative assertion, 56 foundation vector thuộc 22 domain, 985 dòng quy phạm và 708 liên kết nội bộ. | Triển khai runtime cho các lane mở rộng (`RUN-003`, `RIB-001/002`, `M6A/M6B`, `M7` OBT, `P9` BCI, `P10` Personal Mesh/adapters), product default và operator rollout là những gate riêng (`spec-only`). |
| Tích hợp sản phẩm P0–P3 và DR-M5 | **Implementation và bằng chứng CI đã ghi nhận hoàn tất.** Runtime ownership, REST/private WebSocket/CLI/Desktop-Web, resource admission, observability, crash, chaos/fuzz, compaction, rollback và bằng chứng soak M5-07 đã chấp nhận đều có trong `main`. | Các lane mạng/discovery/PoMV vNext vẫn opt-in/default-off; chưa claim `consumer_nat_qualified=true`. |
| Base v1 | **Đã phát hành với các ngoại lệ được chủ dự án chấp thuận và công khai** bằng signed tag `base-v1.0.0-owner-waiver.1` tại `1e0fb2321aee`. Trên `main` hiện tại, đã vá các lỗ hổng lockfile (`250fd0d`), phân loại P3 cho các advisory unmaintained (`2551eee`) và workflow `Base v1 three-OS` chạy xanh hoàn toàn (`45763665749`). | Không có strict tag `base-v1.0.0`; dự án tuyên bố rõ **không** claim `BASE-GATE-V1 qualified=true`. |
| Concept Registry và P5 production | **Đã có bằng chứng production-reference trong phạm vi Base owner waiver** (kèm bằng chứng Linux ba host P5 trên đúng candidate `c453f3e` ngày 23/09/2026). | Trạng thái strict trong source vẫn mở: vector Registry giữ `production_qualified=false`; P5 giữ `provider-document-pending`, `non-linux-platform-lanes-pending`, `mobile-carrier-mailbox-pending`; chưa claim operator rollout. |
| Mobile | **Bản triển khai `BootstrapOnly/Limited` một phần.** Đã có `MOB-01..04` nền tảng/capture, `MOB-03` dùng chung `onebrain-archive` (`OBARV002`, `D-053`), `MOB-05A` signed admission và phần Android `MOB-05B` đến ABI 13 Local Import; `validate_mobile_build_contracts.py` xanh (98 dòng evidence, 91 ID). | `MOB-05B` peer/iOS/provider, `MOB-05C` kích hoạt bộ Registry 2.2 GB, hoàn tất private KU `MOB-04`, `MOB-06` AI/tools, `MOB-07` media/lifecycle, physical-device gate, `MOB-08` networking và `MOB-09` release còn mở. Chưa claim `ReadyOffline`. |
| M6 active distributed KQL và Outcome/Benefit | **Đã khép kín đặc tả vNext (`P3-KQL-COMPILER-M6A-001`, `P4-POMV-CONVERGENCE-001`); runtime chưa mở như milestone production tiếp theo.** KQL one-hop có giới hạn và Public UseEvidence của M3/M4 chưa đóng M6. | Cài đặt runtime cho compiler `FIND → NeedIR`, multipath/provider discovery (`M6A`) và hội tụ `Use -> Outcome -> Benefit` (`M6B`) cùng các entry gate P5/Registry. |
| M7 / OBT | **Đã khép kín đặc tả kinh tế vNext (`P5-OBT-ECONOMY-VNEXT-001`); runtime mới ở mức prototype/legacy.** Các module wallet/OBT legacy được gắn nhãn/cô lập như projection cục bộ phi kinh tế. | Cài đặt runtime `onebrain-obt` (luồng kép `D-014` + `M7`, toán nguyên `u128`, cửa sổ challenge/audit), wallet vận hành và adversarial production gate. |
| Extension, bot, glasses và BCI | **Đã khép kín hợp đồng an toàn & adapter vNext (`P9-BCI-SAFETY-001`, `P10-SURFACES-SYNC-001`); runtime đang ở mức scaffold hoặc research.** | Cài đặt sản phẩm, qualification và, riêng với BCI (`SomaticImmediate/1`), kiểm định phần cứng/an toàn sinh học độc lập. |

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

## Bằng chứng validation của snapshot (2026-11-26)

- `python scripts/ci/validate_vnext_contracts.py` & `python scripts/ci/validate_ten_pillar_contracts.py` — **PASS** (10/10 trụ cột, 10 tường lửa, 15 cạnh thẩm quyền bất tuần hoàn, 99 task, 18 ADR,
  37 + 12 negative assertion, 56 foundation vector/22 domain, 985 dòng quy phạm,
  708 liên kết nội bộ).
- `python scripts/ci/validate_mobile_build_contracts.py` — **PASS** (11 tài liệu
  thẩm quyền, 98 dòng evidence, 91 ID, 123 tính năng mobile, 112 screen, 62
  component, 13 pattern, 0 liên kết lỗi, 0 vi phạm source-guard).
- `python scripts/ci/check_node_layering.py` — **PASS** (`onebrain-node` độc lập
  host; 0 bypass phân tầng ARCH-09).
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
