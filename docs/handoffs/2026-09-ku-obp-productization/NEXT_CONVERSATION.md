# Next conversation — KU-DESK-001 Git closure

## Current state

- `KU-DESK-001` is `Review` on the retained
  `codex/ku-desk-001-workflow` branch, based on clean, synchronized
  `main == origin/main` at `073e141`. Read the
  [implementation evidence](outputs/KU_DESK_001_IMPLEMENTATION.md),
  [task 08](tasks/08-KU-DESK-001.md) and [PROGRESS](PROGRESS.md).
  D-010 requires explicit owner direction before merge or branch deletion.
- The owner reviewed the branch and directed the shutdown reporting
  correction. The task branch now reports Base close failure in the WebView,
  releases the local listener even on that error, and keeps the node for retry.
  Default/feature lifecycle tests pass 8/8 and 10/10; feature Desktop build
  passes. See the updated evidence for the Windows Desktop lib-test executable
  limitation. This review direction did not itself specify Git merge/branch
  deletion.
- The accepted `/ku` Web workflow is packaged in Desktop over the same
  embedded node, local authenticated API and node-owned KU service. Windows
  Desktop build, default/feature lifecycle tests, Web build/tests, API tests
  and vNext contracts pass. Operator installation, live WebView and other OS
  lifecycle evidence are still open. `model_qualified=false`.
- `KU-ENC-003` remains `Blocked`, so `KU-QA-001` and `INT-KU-OBP-001` are
  not ready for acceptance.
- OBP-MIG-001 remains merged under D-042 with rollback and legacy data
  retained. OBP-QA-001 remains merged under functional acceptance, with
  `consumer_nat_qualified=false`; Linux three-host P5 is qualified only for
  exact candidate `c453f3e`. Preserve old staging/runs and default-off OBP
  networking. No tuning or mobile implementation is part of this handoff.

## Review route

1. Read root `AGENTS.md`, [MASTER_PLAN](MASTER_PLAN.md),
   [DECISIONS](DECISIONS.md), [PROGRESS](PROGRESS.md), task 08 and the
   implementation evidence. Inspect the retained branch against main.
2. Check the owner-reviewed shutdown correction and retained task branch.
3. Give an explicit D-010 merge/publication direction if the task should enter
   main. Keep the review branch retained. Do not advance KU-QA or INT-KU-OBP while
   KU-ENC-003 remains Blocked.

## Prompt to paste into a new conversation

```text
Đọc AGENTS.md và docs/handoffs/2026-09-ku-obp-productization/NEXT_CONVERSATION.md
trong working tree gốc. Tiếp tục KU-DESK-001 trên nhánh review
codex/ku-desk-001-workflow theo D-010. Kiểm tra working tree sạch, nhánh đã
push và đồng bộ origin/codex/ku-desk-001-workflow, bao gồm bản sửa 4ec7b1a;
kiểm tra main == origin/main trước khi có thao tác Git tiếp theo. Đọc task 08,
PROGRESS, DECISIONS, MASTER_PLAN và evidence
outputs/KU_DESK_001_IMPLEMENTATION.md.

Owner đã review và yêu cầu sửa nhánh lỗi Quit/Restart: bản sửa đã push trên
nhánh tác vụ. Kiểm tra Base close failure vẫn đóng local API listener, giữ
node để thử lại, báo lỗi shutdown riêng qua desktop-lifecycle, không để lỗi
KU trước đó che lỗi shutdown. Windows lifecycle tests đạt 8/8 mặc định và
10/10 với vnext-outbound-first; feature Desktop build đạt. Desktop lib test
binary không khởi chạy trên host này (STATUS_ENTRYPOINT_NOT_FOUND), nhưng
status test mới đạt trong lifecycle integration suite. Live WebView, real OS
sleep và macOS/Linux lifecycle vẫn chưa được qualified.

Giữ nhánh review. Chỉ merge/push main khi tôi chỉ dẫn rõ theo D-010; không
xóa nhánh nếu chưa có chỉ dẫn riêng. Nếu được chỉ dẫn merge, cập nhật task,
PROGRESS, evidence và handoff với commit merge/verification thực tế. Không
claim KU-QA hoặc INT-KU-OBP: KU-ENC-003 còn Blocked và model_qualified=false.

OBP-MIG-001 đã merged theo D-042; giữ rollback/legacy data. OBP-QA-001 đã
merged theo functional acceptance nhưng consumer_nat_qualified=false; Linux
ba host P5 chỉ qualified cho exact candidate c453f3e. Giữ staging/runs và
durable state cũ; không bật OBP networking mặc định, không tuning, không sửa
mobile hay thay đổi remote host trong công việc này.
```

Historical QA/MIG checkpoints remain in [PROGRESS](PROGRESS.md),
[DECISIONS](DECISIONS.md), their linked evidence files and Git history.
