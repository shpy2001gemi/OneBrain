# Workstream progress

> This is the authoritative execution ledger for this folder.
> Allowed states: `Planned`, `In progress`, `Review`, `Merged`, `Blocked`, `Deferred`.

## Current checkpoint

- Current task: `KU-REV-001`
- Current branch: none; create `codex/ku-rev-001-canonical-audit`
- Last accepted task: handoff package creation
- Default rollout change authorized: **no**
- Mobile work authorized by this package: **no**

## Task ledger

| Order | Task | State | Branch | Dependency | Merge commit/evidence |
|---:|---|---|---|---|---|
| 1 | `KU-REV-001` | Planned | `codex/ku-rev-001-canonical-audit` | — | — |
| 2 | `KU-REV-002` | Planned | `codex/ku-rev-002-runtime-map` | `KU-REV-001` | — |
| 3 | `KU-CON-001` | Planned | `codex/ku-con-001-product-contract` | `KU-REV-002` | — |
| 4 | `KU-RUN-001` | Planned | `codex/ku-run-001-shared-service` | `KU-CON-001` | — |
| 5 | `KU-API-001` | Planned | `codex/ku-api-001-local-api` | `KU-RUN-001` | — |
| 6 | `KU-CLI-001` | Planned | `codex/ku-cli-001-workflow` | `KU-API-001` | — |
| 7 | `KU-WEB-001` | Planned | `codex/ku-web-001-workflow` | `KU-API-001` | — |
| 8 | `KU-DESK-001` | Planned | `codex/ku-desk-001-workflow` | `KU-WEB-001` | — |
| 9 | `KU-QA-001` | Planned | `codex/ku-qa-001-cross-surface` | `KU-CLI-001`, `KU-DESK-001` | — |
| 10 | `OBP-PROD-001` | Planned | `codex/obp-prod-001-product-contract` | `KU-CON-001` | — |
| 11 | `OBP-PROD-002` | Planned | `codex/obp-prod-002-node-lifecycle` | `OBP-PROD-001` | — |
| 12 | `OBP-PROD-003` | Planned | `codex/obp-prod-003-discovery` | `OBP-PROD-002` | — |
| 13 | `OBP-PROD-004` | Planned | `codex/obp-prod-004-routing` | `OBP-PROD-003` | — |
| 14 | `OBP-API-001` | Planned | `codex/obp-api-001-network-api` | `OBP-PROD-004` | — |
| 15 | `OBP-CLI-001` | Planned | `codex/obp-cli-001-networking` | `OBP-API-001` | — |
| 16 | `OBP-WEB-001` | Planned | `codex/obp-web-001-networking` | `OBP-API-001` | — |
| 17 | `OBP-DESK-001` | Planned | `codex/obp-desk-001-networking` | `OBP-WEB-001` | — |
| 18 | `OBP-QA-001` | Planned | `codex/obp-qa-001-nat-canary` | `OBP-CLI-001`, `OBP-DESK-001` | — |
| 19 | `OBP-MIG-001` | Planned | `codex/obp-mig-001-retire-legacy-seed` | `OBP-QA-001` | — |
| 20 | `INT-KU-OBP-001` | Planned | `codex/int-ku-obp-001-product-journey` | `KU-QA-001`, `OBP-QA-001` | — |

## Per-task update protocol

When a task begins:

1. set its state to `In progress`;
2. record the exact branch and starting `main` commit;
3. keep `Current checkpoint` synchronized.

When implementation is ready:

1. set state to `Review`;
2. record test commands and branch tip;
3. push the branch;
4. do not mark `Merged` until the merge exists on `origin/main`.

After owner-approved merge:

1. set state to `Merged` with the merge/main commit;
2. advance the current task to the earliest dependency-ready item;
3. update the pointer in `README.md`;
4. verify clean synchronized `main`, then remove the local task branch only if
   the owner requested cleanup.

## Blocker protocol

Record the exact canonical conflict, missing authority, failing gate or external
dependency here. Do not replace `Blocked` with an inferred product behavior.
