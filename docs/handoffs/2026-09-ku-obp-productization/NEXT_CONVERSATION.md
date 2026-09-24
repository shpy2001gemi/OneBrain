# Next conversation — KU-DESK-001 merged checkpoint

## Current state

- Under D-043, the owner approved review tip `bf3edb1` and explicitly
  directed the D-010 merge and main publication. Merge
  `6449446c3e538b8c97dbed2fdfb15c138e68a0c8` is verified on
  `origin/main`. `KU-DESK-001` is `Merged`; the review branch
  `codex/ku-desk-001-workflow` remains published at `bf3edb1` and was not
  deleted. Read [task 08](tasks/08-KU-DESK-001.md),
  [implementation evidence](outputs/KU_DESK_001_IMPLEMENTATION.md),
  [PROGRESS](PROGRESS.md) and [DECISIONS](DECISIONS.md).
- The packaged `/ku` Web workflow uses the same embedded node, authenticated
  local API and node-owned KU service. The owner-reviewed Quit/Restart fix
  releases the API listener after a Base close error, retains the node for
  retry and reports the shutdown failure separately through
  `desktop-lifecycle`, above an earlier KU dependency error. Post-merge
  Windows lifecycle tests pass 8/8 default and 10/10 with
  `vnext-outbound-first`; the feature Desktop build and vNext contract
  validator pass. The Desktop lib test executable limitation remains in the
  evidence. Live WebView, real OS sleep and macOS/Linux lifecycle are not
  qualified.
- `KU-ENC-003` remains `Blocked` and `model_qualified=false`.
  `KU-QA-001` and `INT-KU-OBP-001` have no acceptance claim.
- `OBP-MIG-001` remains merged under D-042 with rollback and legacy data
  retained. `OBP-QA-001` remains merged under functional acceptance with
  `consumer_nat_qualified=false`; Linux three-host P5 qualifies only exact
  candidate `c453f3e`. Preserve old staging/runs and durable state, and keep
  OBP networking default-off. No tuning, mobile work or remote host action
  is part of this checkpoint.

## Continuation route

1. Read root `AGENTS.md`, [MASTER_PLAN](MASTER_PLAN.md),
   [DECISIONS](DECISIONS.md), [PROGRESS](PROGRESS.md) and the relevant task
   and evidence before starting new work. Confirm clean `main == origin/main`
   and the retained branch when Git closure matters.
2. Do not repeat KU-DESK-001 implementation or merge. Keep its branch unless
   the owner separately directs deletion under D-010.
3. Do not claim KU-QA-001 or INT-KU-OBP-001 while KU-ENC-003 remains
   Blocked. Keep all platform, model, NAT and default-rollout limits explicit.

## Prompt to paste into a new conversation

```text
Đọc AGENTS.md và docs/handoffs/2026-09-ku-obp-productization/NEXT_CONVERSATION.md
trong working tree gốc. KU-DESK-001 đã được owner duyệt theo D-043 và merge
vào origin/main bằng 6449446 từ review tip bf3edb1. Kiểm tra working tree
sạch, main == origin/main và nhánh codex/ku-desk-001-workflow còn được giữ
trên origin. Đọc task 08, PROGRESS, DECISIONS, MASTER_PLAN và evidence
outputs/KU_DESK_001_IMPLEMENTATION.md trước khi chọn công việc tiếp theo.

Đừng lặp lại merge KU-DESK-001 hay xóa nhánh khi chưa có chỉ dẫn riêng theo
D-010. Windows lifecycle mặc định/feature đạt 8/8 và 10/10, feature Desktop
build và vNext contract validator đạt trên merged tree. Desktop lib test
binary có giới hạn STATUS_ENTRYPOINT_NOT_FOUND trên host này; live WebView,
real OS sleep và macOS/Linux lifecycle chưa qualified.

KU-ENC-003 còn Blocked, model_qualified=false; không claim KU-QA-001 hoặc
INT-KU-OBP-001. OBP-MIG-001 đã merged theo D-042 và giữ rollback/legacy
data. OBP-QA-001 đã merged theo functional acceptance nhưng
consumer_nat_qualified=false; Linux ba host P5 chỉ qualified exact candidate
c453f3e. Giữ staging/runs và durable state cũ, OBP networking default-off;
không tuning, sửa mobile hay thay đổi remote host từ checkpoint này.
```

Historical QA/MIG checkpoints remain in [PROGRESS](PROGRESS.md),
[DECISIONS](DECISIONS.md), their linked evidence files and Git history.
