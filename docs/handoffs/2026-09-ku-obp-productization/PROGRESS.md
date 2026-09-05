# Workstream progress

> This is the authoritative execution ledger for this folder.
> Allowed states: `Planned`, `In progress`, `Review`, `Merged`, `Blocked`, `Deferred`.

## Current checkpoint

- Current task: `KU-CON-001`
- Current branch: `codex/ku-con-001-product-contract`
- Starting main: `d8effb772b0cb7766e91b799dd598061a81a9df5` (clean, pushed and synchronized)
- Last accepted task: `KU-REV-002`, merge `b87226311d57278e488fd55cbbcbd16dfd54e200` on `origin/main`
- Default rollout change authorized: **no**
- Mobile work authorized by this package: **no**

## Task ledger

| Order | Task | State | Branch | Dependency | Merge commit/evidence |
|---:|---|---|---|---|---|
| 1 | `KU-REV-001` | Merged | `codex/ku-rev-001-canonical-audit` | — | `25d008d211f450d15ba1a63cacc0368298ed3e7a` on `origin/main`; [authority audit](outputs/KU_AUTHORITY_AUDIT.md), D-011–D-014. |
| 2 | `KU-REV-002` | Merged | `codex/ku-rev-002-runtime-map` | `KU-REV-001` | [Runtime gap map](outputs/KU_RUNTIME_GAP_MAP.md), including owner D-011–D-014; merge `b872263` on `origin/main`. |
| 3 | `KU-CON-001` | Review | `codex/ku-con-001-product-contract` | `KU-REV-002` | [Candidate contract](../../specs/vnext/KU_PRODUCT_WORKFLOW_PROFILE_V1.md); KU-PC-A/B/C proposed, not frozen; evidence below. |
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

### KU-REV-001 review evidence — 2026-09-05

- Starting main: `3704da6b68237f50998f73c02bc5a2c59d27def8`, clean and equal to
  fetched `origin/main` before branching.
- Deliverable: [KU_AUTHORITY_AUDIT.md](outputs/KU_AUTHORITY_AUDIT.md), including
  authority/ownership/lifecycle/storage, legacy contradictions, owner-resolved
  direction and outstanding specification work.
- Owner clarification: [D-011–D-014](DECISIONS.md#d-011--deterministic-identity-after-semantic-normalization).
  Direct encode/verify OBT issuance is the requested new direction; the earlier
  benefit-only restriction is not to be silently retained in future design.
  Reward amounts, admission, replay prevention and settlement remain unspecified.
- `python scripts/ci/validate_vnext_contracts.py` — PASS (99 tasks, 18 ADRs,
  37 negative assertions; existing canonical contracts unchanged).
- `git diff --check` — PASS.
- Local file-link check — PASS (47 links across the four handoff deliverables).
- No source/application/mobile changes. No runtime tests or production
  encode/verify/Registry-sync/reward implementation claims.
- Owner explicitly authorized merge and KU-REV-002 in this conversation.
  Revalidated the clean, pushed audit tip, merged and pushed `25d008d` to main;
  README pointer now advances to `KU-REV-002`. No branch deletion requested.
- Audited content checkpoint: `c89f848`, pushed successfully to
  `origin/codex/ku-rev-001-canonical-audit`. The final branch tip also contains
  the ledger-only follow-up recording this checkpoint; resolve the tracked
  branch for that tip rather than treating this content hash as a merge commit.

### KU-REV-002 review evidence — 2026-09-05

- Starting main: `80119e1311b1e95171e5613e0335ad3ef69fa2a4`, clean and equal
  to `origin/main`. This includes the authorized KU-REV-001 merge `25d008d`
  and its handoff pointer update.
- Deliverable: [KU_RUNTIME_GAP_MAP.md](outputs/KU_RUNTIME_GAP_MAP.md).
  Maps semantic identity, Registry, canonical/public/private storage, Base
  operations, Mapping/adoption, existing interfaces, migration and delegated
  work/reward gaps. D-011–D-014 remain required future direction.
- Changed only handoff documentation. No source fixes, public contract
  changes, rollout changes, minting, mobile implementation or migrations.
- Fresh focused results (Cargo run from `src/`, all with `--locked`):

| Command | Result | Evidence boundary |
|---|---|---|
| `cargo test --locked -p onebrain-node --lib vnext_local_runtime` | PASS: 1 | Local slice fixture; persistent Need reopen, in-memory Mapping after reopen. |
| `cargo test --locked -q -p onebrain-node --lib` | PASS: 104 | Default-feature node components; includes the preceding local test, Registry, migration, workflow and reward firewall. |
| `cargo test --locked -q -p onebrain-node --test base_runtime_facade --test canonical_exchange --test durable_data_recovery --test p0_capability_truth` | PASS: 9 + 5 + 4 + 1 | Base uses test adapters; source recovery uses in-memory Vault plus disk staging. |
| `cargo test --locked -q -p onebrain-node --test vnext_index_parity` | PASS: 3 | Canonical index parity and legacy mutation fence. |
| `cargo test --locked -q -p onebrain-api --test base_contract` | PASS: 8 | Base HTTP contract; test local adapter does not implement KU operations. |
| `cargo test --locked -q -p onebrain-api --features vnext-network-runtime --lib vnext_api` | PASS: 7 | Feature-enabled Need and explicit Public Use HTTP fixtures; 10 other tests filtered out. |
| `cargo test --locked -q -p onebrain-node --features vnext-network-runtime --lib vnext_distributed_kql` | PASS: 3 | Two local test peers, private match/restart dedup, lifecycle tombstones and legacy plaintext rejection; 167 other tests filtered out. |
| `cargo test --locked -q -p ku-encoder --lib` | PASS: 137 | Controlled encoder/resolver/builder tests; no live Ollama or shared T1/AI canonical identity conformance. |
| `cargo test --locked -q -p ku-ai --lib vnext_` | PASS: 21 | Executor/fidelity component tests; no durable distributed worker settlement. |
| `npm run test:vnext` (from `src/onebrain-web`) | PASS: 2 | Receipt tests only; no full browser or Desktop GUI run. |

- `python scripts/ci/validate_vnext_contracts.py` — PASS (99 tasks, 18 ADRs,
  37 negative assertions; existing canonical contracts unchanged).
- `git diff --check` — PASS. Local file-link check — PASS (104 links across
  the four changed handoff files).
- Existing compiler dead-code/unused-import warnings were observed; no fixes made within
  this read-only audit. No claim of whole-workspace, live multi-node
  qualification, full-size Registry qualification or D-011–D-014 completion.
- After this review was reported, the owner requested "làm task kế tiếp".
  Treated this as authorization for the prerequisite merge and KU-CON-001;
  rechecked the clean/pushed branch and contract/diff gates, then merged and
  pushed `b872263` to main. No branch deletion requested.
- Audited content checkpoint: `496d340`, pushed successfully to
  `origin/codex/ku-rev-002-runtime-map`. The branch tip additionally contains
  the ledger-only commit recording this checkpoint; resolve that tracked
  branch for its final tip. This content checkpoint is not a merge commit.

### KU-CON-001 review evidence — 2026-09-05

- Starting main: `d8effb772b0cb7766e91b799dd598061a81a9df5`, clean, pushed
  and synchronized after KU-REV-002 merge `b872263` and handoff update.
- Deliverables: [owner-review profile](../../specs/vnext/KU_PRODUCT_WORKFLOW_PROFILE_V1.md),
  [machine inventory](../../../src/test-vectors/vnext/ku-product-workflow-v1.json),
  [focused validator](../../../scripts/ci/validate_ku_product_contract.py),
  [mutation tests](../../../scripts/ci/test_validate_ku_product_contract.py).
  The global vNext validator includes the candidate check; normative coverage
  and specification index mark this as contract evidence only.
- Proposed review items are KU-PC-A (separate finite normalized semantic
  identity/domain), KU-PC-B (11 local operations and 18 typed bounded DTOs),
  KU-PC-C (private local revision journal). New domains/command IDs/routes are
  not allocated or activated. Owner acceptance plus explicit registration,
  compatibility and golden-vector gates precede runtime dispatch.
- D-012 publisher/peer Registry distribution, D-013 durable delegated work and
  blind verification, and D-014 direct work-based issuance remain mandatory
  linked specification/implementation dependencies. D-014 does not wait for
  BenefitEvent and does not fall back to bounty or simulated legacy balances.
- `python scripts/ci/validate_ku_product_contract.py` — PASS:
  11 operations, 18 DTOs, 11 valid/invalid DTO fixtures. Fixture byte/hash values
  are shape examples, not golden canonical/hash conformance evidence.
- `python -m unittest scripts.ci.test_validate_ku_product_contract
  scripts.ci.test_validate_vnext_product_profile
  scripts.ci.test_validate_vnext_ws_profile
  scripts.ci.test_validate_vnext_cli_profile
  scripts.ci.test_validate_vnext_desktop_web_ux_profile
  scripts.ci.test_validate_base_v1_runtime_interface` — PASS: 101 tests,
  including 33 new KU tests with mutation subcases and 68 existing contract tests.
- `python scripts/ci/validate_vnext_contracts.py` — PASS: the candidate plus
  existing 99 tasks, 18 ADRs, 37 negative assertions, 841 normative lines and
  479 specification links. Existing canonical profile/domain/IDL inventories
  and generated runtime declarations unchanged.
- `git diff --check` — PASS. Only candidate specification/inventory,
  contract validators/tests and handoff/index/coverage files changed. No
  application runtime, screen, mobile, networking or rollout implementation.
- Local file-link check — PASS: 53 links across the candidate profile and
  changed handoff README/PROGRESS files, in addition to the global link gate.
- Current pointer remains KU-CON-001 pending owner review and merge. This is
  completion of the owner-reviewable deliverables, not approval/freeze of new
  public behavior and not implementation of KU-RUN-001.

### Protocol

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
