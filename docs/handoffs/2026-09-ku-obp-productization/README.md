# KU review and OBP productization handoff

> Status: **`KU-CON-001` approved and merged / next: `KU-RUN-001`**
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

**Current task:** [`KU-RUN-001`](tasks/04-KU-RUN-001.md)
**Suggested branch:** `codex/ku-run-001-shared-service`

Audit output: [KU_AUTHORITY_AUDIT.md](outputs/KU_AUTHORITY_AUDIT.md).
Runtime review: [KU_RUNTIME_GAP_MAP.md](outputs/KU_RUNTIME_GAP_MAP.md).
`KU-REV-002` was merged as `b872263` after the owner requested the next task.
The [KU-CON-001 contract](../../specs/vnext/KU_PRODUCT_WORKFLOW_PROFILE_V1.md)
is approved for KU-PC-A/B/C under [D-015](DECISIONS.md#d-015--ku-product-contract-accepted).
Domain/payload registration and golden vectors remain technical gates before
runtime dispatch. Merge `2cbc8f2` is on `origin/main`; KU-RUN-001 is the next
implementation task and has not started. Fresh evidence is in [PROGRESS.md](PROGRESS.md).
The owner-approved audit merge is `25d008d` on `origin/main`. The owner has
clarified normalized-semantic CID convergence, regularly updated Registry
distribution from publishers/peers, delegated encode/verify work and direct
OBT issuance for accepted work; see [D-011–D-014](DECISIONS.md#d-011--deterministic-identity-after-semantic-normalization).
The economic choice changes the earlier benefit-only direction and needs a
versioned specification amendment before implementation. This audit does not
enable minting or change canonical bytes, application code or rollout state.

Copy/paste prompt:

```text
Read AGENTS.md and docs/handoffs/2026-09-ku-obp-productization/README.md,
then execute only KU-RUN-001 from tasks/04-KU-RUN-001.md. Follow its exact
read set, scope, branch and acceptance criteria. Update PROGRESS.md and the
Current task pointer in README.md before handing off. Do not merge or delete
the task branch without my explicit instruction.
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
| [`tasks/README.md`](tasks/README.md) | Index of all 20 independently executable tasks |
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
