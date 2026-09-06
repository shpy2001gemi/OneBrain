# KU review and OBP productization handoff

> Status: **`KU-ENC-002` merged / `KU-ENC-003` Blocked on qualification inputs**
> Owner decision: 2026-09-05
> Baseline: `main` / `origin/main` at `409fca34db8faaf238b899a2481175d922113b99` before this handoff package
> Scope: review KU and expose it consistently through CLI, local Web and Desktop while productizing the already implemented OBP outbound-first core as a separate lane.

This folder is the compact starting point for future conversations. It records
the decision, the evidence boundary, the dependency order and one bounded file
per task so a new conversation does not need the history that produced it.

## Owner-approved decision

1. Return to KU review and product development now.
2. Treat OBP protocol, reconciliation and outbound-first core as a stable
   implemented foundation. Do not redesign it unless a concrete defect or a
   canonical-document conflict is found.
3. Keep KU correct and useful locally without requiring OBP availability.
4. Productize OBP separately by connecting the existing Reachability Manager,
   discovery, reservation, route and relay components to the normal
   `OneBrainNode` lifecycle and shared product API.
5. CLI, local Web and Desktop must use the same node-owned services and the
   same semantic boundaries.
6. Keep all new network lanes opt-in/default-off until the product acceptance
   and applicable release gates pass.
7. Do not describe the legacy `onebrain-seed` TCP/JSON prototype as the secure
   vNext seeder. The vNext role is permissionless `onebrain-relay` plus signed
   rendezvous/bootstrap/discovery inputs.

See [DECISIONS.md](DECISIONS.md) for the exact allowed and forbidden claims.

## Start here in a new conversation

Read only these files first:

1. repository `AGENTS.md`;
2. this `README.md`;
3. [PROGRESS.md](PROGRESS.md);
4. the single task file named by `Current task` below.

Do not load the whole historical distributed-runtime or foundation plan unless
the selected task explicitly requires a section from it.

**Current task:** [`KU-ENC-003`](tasks/23-KU-ENC-003.md) — Blocked; no qualified model/profile
**Current branch:** `codex/ku-enc-003-model-qualification`
**Checkpoint:** [qualification evidence and missing gates](outputs/KU_ENC_003_QUALIFICATION.md)
**Owner preparation:** [mẫu nguồn và cách đánh giá](outputs/KU_ENC_003_DATA_GUIDE.vi.md)

**Delivery priority:** [D-021](DECISIONS.md#d-021--prioritize-an-early-open-source-concept--mvp)
records the owner's request for an early open-source concept/MVP. The existing
KU-API-001 → KU-WEB-001 path can deliver a small local draft/preview/validate/
save/search journey without waiting for all model qualification. Model-ready
claims still require KU-ENC-003. The owner reports new VI/EN workbooks with
100 sources each; only matching column headers have been reviewed, not the
holdout contents, independence or gold labels.

KU-RUN-001 implements the D-016 registration and node-owned local service.
The owner authorized merge `d141701`, now on `origin/main`. See the
[implementation evidence and limits](outputs/KU_RUN_001_IMPLEMENTATION.md)
and [PROGRESS.md](PROGRESS.md). The owner authorized starting KU-ENC-001,
followed by the shared encoder runtime; KU-API-001 has not started.

The new [encoder framework research](outputs/KU_ENCODER_FRAMEWORK_RESEARCH.md)
maps the old tool-driven/v2 paths, existing AI/mobile tasks and the shared gap.
KU-ENC-001/002/003 cover contract, workflow implementation and model/resource
qualification. Model proposals carry no execution or persistence authority.

KU-ENC-001 now provides the [shared framework contract](../../specs/vnext/KU_EXTRACTION_FRAMEWORK_PROFILE_V1.md)
and [review evidence](outputs/KU_ENC_001_CONTRACT.md): closed schemas, vi/en
prompts, 48 corpus cases, bounded resource/lifecycle rules and qualification gates.
The owner accepted the reviewed contract under [D-019](DECISIONS.md#d-019--ku-enc-001-contract-accepted-after-owner-review).
The owner-authorized merge `22599d0` is on `origin/main`. The owner authorized
starting KU-ENC-002 to implement the shared encoder against this contract.

KU-ENC-002 provides the native compiler/workflow and node integration, accepted
under [D-020](DECISIONS.md#d-020--ku-enc-002-accepted-and-closed) and merged into
`origin/main` at `dc04b71`. KU-ENC-003 has started on its task branch; the
locked holdout/evaluator evidence remains missing, and only artifact preflight
has run. Newly prepared workbooks are owner-reported, not yet validated inputs.
Read the [implementation evidence and limits](outputs/KU_ENC_002_IMPLEMENTATION.md)
for test results, offline behavior, current source/Registry boundaries and the
remaining real-model qualification work. Model/tool/storage authority remains
separate, and default rollout has not changed.

Audit output: [KU_AUTHORITY_AUDIT.md](outputs/KU_AUTHORITY_AUDIT.md).
Runtime review: [KU_RUNTIME_GAP_MAP.md](outputs/KU_RUNTIME_GAP_MAP.md).
`KU-REV-002` was merged as `b872263` after the owner requested the next task.
The [KU-CON-001 contract](../../specs/vnext/KU_PRODUCT_WORKFLOW_PROFILE_V1.md)
is approved for KU-PC-A/B/C under [D-015](DECISIONS.md#d-015--ku-product-contract-accepted).
Domain/payload registration and golden gates now pass on the KU-RUN-001 task
branch. Merge `2cbc8f2` is on `origin/main`; runtime review evidence is in
[PROGRESS.md](PROGRESS.md).
The owner-approved audit merge is `25d008d` on `origin/main`. The owner has
clarified normalized-semantic CID convergence, regularly updated Registry
distribution from publishers/peers, delegated encode/verify work and direct
OBT issuance for accepted work; see [D-011–D-014](DECISIONS.md#d-011--deterministic-identity-after-semantic-normalization).
The economic choice changes the earlier benefit-only direction and needs a
versioned specification amendment before implementation. This audit does not
enable minting or change canonical bytes, application code or rollout state.

Copy/paste prompt:

```text
Read AGENTS.md, this README, PROGRESS.md and tasks/23-KU-ENC-003.md.
Resume the existing codex/ku-enc-003-model-qualification branch; do not recreate
it. Read outputs/KU_ENC_003_QUALIFICATION.md and the task's required read set.
Use outputs/KU_ENC_003_DATA_GUIDE.vi.md to obtain the missing holdout/evaluator
inputs. Preserve the preflight work and do not treat guide examples or the
public corpus as blind evidence. Lock the full qualification run before any
model execution. Update the handoff and push; do not merge or delete the branch.
```

More prompts are in [NEW_CONVERSATION_PROMPTS.md](NEW_CONVERSATION_PROMPTS.md).

## Package contents

| File | Purpose |
|---|---|
| [DECISIONS.md](DECISIONS.md) | Owner decisions, architectural boundaries and claim language |
| [CAPABILITY_STATUS.md](CAPABILITY_STATUS.md) | Compact OBP capability/status map used by product planning |
| [MASTER_PLAN.md](MASTER_PLAN.md) | Two-lane dependency graph, sequencing and shared exit gates |
| [PROGRESS.md](PROGRESS.md) | Authoritative status/branch/commit ledger for this workstream |
| [NEW_CONVERSATION_PROMPTS.md](NEW_CONVERSATION_PROMPTS.md) | Short prompts for starting, reviewing or merging one task |
| [`tasks/README.md`](tasks/README.md) | Index of all 23 independently executable tasks |
| [`outputs/README.md`](outputs/README.md) | Naming and placement rules for task audit/evidence outputs |

## Working rules

- One task, one branch, one primary objective.
- Branch from an up-to-date clean `main`; use the exact `codex/` branch in the
  task file.
- A task may edit its declared deliverables and the handoff index/progress
  files. Any wider change requires an explicit scope revision.
- Specification tasks precede implementation when a public field, endpoint,
  status, command or behavior is new.
- Existing vNext authority, consent, privacy and negative assertions are not
  weakened for product convenience.
- Complete the task's focused validation before requesting review.
- Push the task branch. Do not merge, delete the branch or change the default
  rollout state without explicit owner instruction.
- After an accepted merge, update `PROGRESS.md`, advance `Current task`, and
  keep `main` equal to `origin/main` with no leftover worktree or stash.
- Mobile implementation is outside this package. Any later mobile task must
  follow the separate mobile build contract and subtree `AGENTS.md`.

## Canonical references

This folder is a planning and handoff layer, not new protocol authority. The
following remain authoritative:

- [`ONEBRAIN_RESEARCH_BASELINE_V7_1.md`](../../research/ONEBRAIN_RESEARCH_BASELINE_V7_1.md)
- [`ONEBRAIN_FOUNDATION_IMPLEMENTATION_PLAN_V7_1.md`](../../research/ONEBRAIN_FOUNDATION_IMPLEMENTATION_PLAN_V7_1.md)
- [vNext contract index](../../specs/vnext/README.md)
- [Outbound-first design](../../superpowers/specs/2026-08-14-onebrain-outbound-first-nat-traversal-design.md)
- [Current project status](../../PROJECT_STATUS.vi.md)

If these sources conflict, stop and record the exact conflict. This package
must not silently choose a new semantic rule.
