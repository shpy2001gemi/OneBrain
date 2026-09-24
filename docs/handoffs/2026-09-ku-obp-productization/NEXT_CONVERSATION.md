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

Historical QA/MIG checkpoints remain in [PROGRESS](PROGRESS.md),
[DECISIONS](DECISIONS.md), their linked evidence files and Git history.
