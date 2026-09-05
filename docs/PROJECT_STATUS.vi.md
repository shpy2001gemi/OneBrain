# Trạng thái dự án OneBrain

[English](PROJECT_STATUS.md)

> Snapshot: **2026-09-05 (Asia/Saigon)**
>
> Nguồn được audit: `main` / `origin/main` tại
> `409fca34db8faaf238b899a2481175d922113b99` trước cập nhật tài liệu này
>
> Phạm vi: trạng thái repository, nhánh/worktree Git local, các tuyên bố
> qualification trong source, validator hiện tại và bằng chứng CI gần nhất.

Đây là điểm vào cho tiến độ hiện tại. Specification mô tả hành vi bắt buộc hoặc
mục tiêu; bản thân specification không phải bằng chứng rằng một đường production
đã hoàn tất.

## Trạng thái tổng quan

| Luồng công việc | Trạng thái hiện tại | Phần còn mở |
|---|---|---|
| Contract và foundation vNext | **Hoàn tất ở phạm vi contract/foundation của repository.** Validator hiện báo 99 task, 18 ADR, 37 negative assertion và 55 foundation vector thuộc 21 domain. | Product default, operator rollout và các milestone sau là những gate riêng. |
| Tích hợp sản phẩm P0-P3 và DR-M5 | **Implementation và bằng chứng CI đã ghi nhận hoàn tất.** Runtime ownership, REST/private WebSocket/CLI/Desktop-Web, resource admission, observability, crash, chaos/fuzz, compaction, rollback và bằng chứng soak M5-07 đã được chấp nhận đều có trong `main`. | Các lane vNext vẫn opt-in/default-off; đường live legacy chưa được loại bỏ hoàn toàn. |
| Base v1 | **Đã phát hành với các ngoại lệ được chủ dự án chấp thuận và công khai** bằng signed tag `base-v1.0.0-owner-waiver.1` tại `1e0fb2321aee`. Tag và CI ghi nhận qualification ba hệ điều hành, kiểm tra prebuilt Registry, P5 và soak 72 giờ liên tục trên ba runner. | Không có strict tag `base-v1.0.0`; release tuyên bố rõ là **không** claim `BASE-GATE-V1 qualified=true`. Chủ dự án cần chọn coi waiver release là mốc Base v1 cuối cùng hoặc cho phép một lượt strict qualification mới. |
| Concept Registry và P5 production | **Đã có bằng chứng production-reference trong phạm vi Base owner waiver.** | Trạng thái strict trong source vẫn mở: vector Registry còn `production_qualified=false`; P5 còn `provider-document-pending`, `non-linux-platform-lanes-pending`, `mobile-carrier-mailbox-pending`; chưa claim operator-approved product rollout. |
| Mobile | **Bản triển khai BootstrapOnly/Limited còn một phần.** MOB-05A signed admission và phần Android của MOB-05B đến ABI 13 Local Import đã triển khai; contract validator xanh. | MOB-05B peer/iOS/provider, MOB-05C activation đầy đủ bộ Registry 2.2 GB, hoàn tất private KU trong MOB-04, MOB-06 AI/tools, MOB-07 media/lifecycle, physical-device gate, MOB-08 networking và MOB-09 release. Chưa claim `ReadyOffline`. |
| M6 active distributed KQL và Outcome/Benefit | **Chưa mở như milestone production tiếp theo.** Các capability M3/M4 gồm KQL one-hop có giới hạn và Public UseEvidence chưa đủ để đóng M6. | Active multipath/provider discovery và luồng end-to-end Use -> Outcome -> Benefit cần các entry gate P5/Registry. |
| M7/OBT | **Mới ở mức prototype/legacy; chưa có nền kinh tế production.** | Benefit-based reward policy, ledger/finality vNext, wallet vận hành và adversarial production gate. |
| Extension, bot, glasses và BCI | **Scaffold hoặc research.** | Product implementation, qualification và, riêng với BCI, đủ bằng chứng an toàn từ bên ngoài. |

## Ranh giới release Base v1

Annotated tag `base-v1.0.0-owner-waiver.1` ghi nhận:

- candidate commit `1e0fb2321aeec04cb711f4259e2bc807e73a35dd`;
- run ba hệ điều hành
  [33529983318](https://github.com/shpy2001gemi/OneBrain/actions/runs/33529983318)
  thành công;
- Task 28 run
  [33592716241](https://github.com/shpy2001gemi/OneBrain/actions/runs/33592716241)
  thành công;
- prebuilt Registry root, P5 root và soak root 72 giờ liên tục;
- các ngoại lệ công khai gồm evidence assembly muộn, sửa tên frozen test target,
  các phát hiện Clippy all-features đã tồn tại và dependency-policy triage.

Baseline đã đồng bộ đi trước candidate này bảy commit về workflow/handoff. CI
tại
`c65f1739fcd0` có
[run candidate ba hệ điều hành](https://github.com/shpy2001gemi/OneBrain/actions/runs/33592237276)
và nightly parser fuzz thành công. Tag có chữ ký PGP, nhưng máy này chưa thể verify vì chưa cài public key
của signer; do đó audit chỉ ghi nhận tag và nội dung tag, không tuyên bố đã tự
verify chữ ký trên máy local.

## Hướng triển khai đã được owner chấp thuận

Ngày 2026-09-05, owner quyết định quay lại review/phát triển KU và áp dụng cùng
một contract cho CLI, local Web và Desktop. Phần core outbound-first của OBP
được giữ làm nền ổn định, nhưng productization chạy thành lane riêng; không
được xem legacy `onebrain-seed` là secure vNext seeder và không bật mặc định
trước acceptance gate.

Điểm bắt đầu gọn cho conversation mới là
[`handoffs/2026-09-ku-obp-productization/README.md`](handoffs/2026-09-ku-obp-productization/README.md).
Gói này chứa quyết định, capability boundary, dependency graph, progress ledger,
prompt và 20 task/nhánh độc lập. Task hiện tại là `KU-REV-001`; chưa tạo nhánh
implementation.

## Ưu tiên triển khai còn lại

1. **Review và khóa product contract KU.** Thực hiện `KU-REV-001`,
   `KU-REV-002`, rồi `KU-CON-001` trước khi thay đổi public behavior.
2. **Đưa KU vào một shared local service.** Sau contract, nối cùng semantics
   qua local REST/private WS, CLI, local Web và Desktop; KU phải vẫn hữu dụng
   khi không có peer.
3. **Productize OBP ở lane riêng.** Nối lifecycle, bootstrap/discovery,
   reservation, route/outbox/failover đã có vào normal node aggregate và cùng
   product API; giữ opt-in/default-off đến khi acceptance thực tế đạt.
4. **Chỉ tích hợp KU -> OBP sau khi hai lane đều qua QA.** Luồng phải giữ rõ
   `encode ≠ publish`, `proposal ≠ materialize`, `materialize ≠ adopt` và
   không biến relay/delivery thành authority.
5. **Giữ các lane khác ngoài scope hiện tại.** Strict Base decision, mobile,
   M6 active multipath KQL, Outcome/Benefit, M7/OBT và BCI vẫn là gate/milestone
   riêng; mobile khi mở lại phải theo mobile build contract.

Thứ tự mobile chi tiết nằm trong
[`WIP_MOBILE_APP_IMPLEMENTATION_PLAN_V1.md`](research/WIP_MOBILE_APP_IMPLEMENTATION_PLAN_V1.md).
Lịch sử và gate distributed runtime nằm trong
[`WIP_DISTRIBUTED_RUNTIME_IMPLEMENTATION_PLAN_V2.md`](research/WIP_DISTRIBUTED_RUNTIME_IMPLEMENTATION_PLAN_V2.md).

## Audit nhánh Git local

Audit trước khi dọn, sau `git fetch --all --prune`, cho thấy:

| Kiểm tra | Kết quả |
|---|---:|
| Số đầu nhánh local | 46 |
| Nhánh local đã nằm trong `origin/main` | 46 |
| Nhánh local có commit chưa nằm trong `origin/main` | **0** |
| `main` so với `origin/main` tại thời điểm audit | 0 ahead / 0 behind |
| Worktree đã đăng ký | 12 |
| Worktree còn tồn tại | 10, tất cả sạch |
| Worktree tạm đã mất thư mục và có metadata prunable | 2 |

**Không có nhánh local nào chưa hoàn thành theo nghĩa Git.** Tip của mọi nhánh
local đều là ancestor của `origin/main`. Cả 45 nhánh khác `main` là nhánh lịch
sử tích hợp/release và đủ điều kiện để dọn khỏi local.

Kết quả dọn dẹp ngày 2026-09-05:

| Trạng thái local hiện tại | Kết quả |
|---|---:|
| Nhánh local | 1 (`main`) |
| Worktree đã đăng ký | 1 (repository chính) |
| Stash | 0 |
| Commit chỉ còn reachable từ local ref | 0 |
| Remote branch bị xóa | 0 |

Hai chênh lệch dễ gây hiểu nhầm không phải code chưa merge:

- `codex/dr-m5-operational-compaction` đi trước remote branch cùng tên một
  commit, nhưng commit đó đã nằm trong `origin/main`.
- `codex/task28-prebuilt-registry` đang track `origin/main` và chậm năm commit;
  tip của nó là owner-waiver candidate, không phải nhánh có commit bị bỏ sót.

Các nhóm nhánh đã nằm trong `origin/main`:

| Nhóm | Số lượng | Nội dung |
|---|---:|---|
| Implementation Base v1 | 8 | IDL, archive, authority, contract, freeze, P5, Registry và storage |
| DR-M5 | 8 | Baseline, resource admission, observability, crash, chaos/fuzz, compaction, rollback và soak |
| Tích hợp P1/P2/P3 | 14 | Năm nhánh P1, năm nhánh P2 và bốn nhánh P3 |
| P5/runner/fix | 9 | P5 preflight, M5-07 acceptance/runner và các runner fix |
| Task 28 | 4 | Preparation, external request root, prebuilt Registry và request handoff |
| Documentation/history | 2 | English README và nhánh chuyển Mermaid/PDF cũ |

Quá trình dọn đã prune hai đăng ký bootstrap tạm bị mất, gỡ chín worktree phụ
sạch theo đúng thứ tự phụ thuộc và xóa cả 45 nhánh local đã merge. Một thư mục
Base candidate đã mất đăng ký Git cần xử lý long-path; thư mục trống bị giữ bởi
một phiên Ubuntu `wslhost` mồ côi nên chỉ cặp process cũ đã được xác minh đó bị
dừng trước khi xóa. Remote branch và tag không bị thay đổi.

## Bằng chứng validation của snapshot

- `python scripts/ci/validate_vnext_contracts.py` — **PASS**.
- Focused OBP assessment trên baseline `409fca3`: `ku-net` persist/QUIC 378
  test, `onebrain-node` vNext runtime 18 test, Anti-Gravity Reunion 4 test,
  `onebrain-relay` 31 test và `onebrain-node` outbound-first 184 test — **PASS**.
- `python scripts/ci/validate_mobile_build_contracts.py` — **PASS** với 98 dòng
  evidence, 123 tính năng mobile, 112 screen, 62 component, 13 pattern và không
  có broken link hoặc source-guard failure.
- `git diff --check` — **PASS** sau khi cập nhật tài liệu.
- `cargo fmt --all -- --check` — **PASS**.
- `cargo check --workspace --locked` — **PASS**.
- Nightly parser fuzz gần nhất đã audit trên `main`:
  [33951243848](https://github.com/shpy2001gemi/OneBrain/actions/runs/33951243848) —
  **success**.

Snapshot này không tuyên bố đã chạy lại toàn bộ workspace test, physical-device
test hoặc mọi strict production qualification gate trên máy local.
