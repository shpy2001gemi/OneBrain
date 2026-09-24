# Next conversation — KU-DESK-001

## Current state

- `main` and `origin/main` contain D-042's OBP-MIG-001 merge `0604b55` and
  the handoff publication. The retained branch is
  `codex/obp-mig-001-retire-legacy-seed` at `c608069`. Task 19 is `Merged`.
- OBP-QA-001 is `Merged` under D-039/D-041 functional acceptance. The
  independent consumer-NAT qualification was explicitly skipped;
  `consumer_nat_qualified=false`. The separate Linux three-host P5
  production-reference qualification applies only to exact candidate
  `c453f3e`. Preserve all existing remote staging, runs and durable state.
- OBP remains opt-in. The normal product surfaces use the shared node-owned
  service. The legacy TCP/JSON seed path requires explicit compatibility
  selection; no automatic trust or data migration was made.
- `KU-DESK-001` is the next dependency-ready implementation task:
  `KU-WEB-001` is merged. `KU-ENC-003` remains `Blocked`, so `KU-QA-001`
  and `INT-KU-OBP-001` are not ready for acceptance.

## Read first

1. Root `AGENTS.md`, [PROGRESS](PROGRESS.md), [DECISIONS](DECISIONS.md),
   [MASTER_PLAN](MASTER_PLAN.md), and [KU-DESK-001](tasks/08-KU-DESK-001.md).
2. The accepted KU product/API/Desktop-Web profiles,
   [Web implementation](outputs/KU_WEB_001_IMPLEMENTATION.md), and the
   Desktop bootstrap, sidecar, tray, shutdown and event-bridge code/tests.
3. For OBP boundaries, read the
   [migration map](outputs/OBP_MIG_001_IMPLEMENTATION.md),
   [QA functional acceptance](outputs/OBP_QA_001_FUNCTIONAL_ACCEPTANCE_V2.md),
   and [P5 production-reference record](outputs/OBP_QA_001_P5_PRODUCTION_20260923.md).

## Prompt to paste into a new conversation

```text
Đọc AGENTS.md và docs/handoffs/2026-09-ku-obp-productization/NEXT_CONVERSATION.md
trong working tree gốc. Kiểm tra main sạch và đồng bộ origin/main, rồi làm
KU-DESK-001 trên nhánh codex/ku-desk-001-workflow theo D-010. Đọc task 08,
PROGRESS, DECISIONS, MASTER_PLAN, các KU product/API/Desktop-Web profiles,
Web implementation và Desktop lifecycle trước khi sửa.

Tích hợp Web KU đã chấp nhận vào Desktop với cùng embedded node/API và
node-owned service. Kiểm tra startup failure, local read, shutdown/restart,
durable work và ranh giới private keys/capabilities; chạy build và lifecycle
tests phù hợp. Ghi evidence và cập nhật ledger trên nhánh tác vụ.

OBP-MIG-001 đã merged theo D-042; giữ rollback/legacy data. OBP-QA-001 đã
merged theo functional acceptance nhưng consumer NAT chưa qualified
(consumer_nat_qualified=false); P5 Linux ba host chỉ qualified riêng trên
candidate c453f3e. Giữ staging/runs cũ, không bật OBP networking mặc định,
không tuning hay thay đổi mobile. KU-ENC-003 còn Blocked; chưa claim KU-QA
hoặc INT-KU-OBP. Giữ nhánh review; merge theo D-010 cần chỉ dẫn của owner.
```

Historical QA/MIG checkpoints remain in [PROGRESS](PROGRESS.md),
[DECISIONS](DECISIONS.md), their linked evidence files and Git history.
