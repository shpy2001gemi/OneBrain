# KU review and OBP productization handoff

> Status: **OBP-MIG-001 merged under D-042; consumer-NAT qualification skipped and unclaimed**
> Owner decision: 2026-09-05
> Baseline: `main` / `origin/main` at `409fca34db8faaf238b899a2481175d922113b99` before this handoff package
> Scope: review KU and expose it consistently through CLI, local Web and Desktop while productizing the already implemented OBP outbound-first core as a separate lane.

This folder is the compact starting point for future conversations. It records
the decision, the evidence boundary, the dependency order and one bounded file
per task so a new conversation does not need the history that produced it.

Latest: D-042 accepted and merged OBP-MIG-001 as `0604b55`. The
[migration map](outputs/OBP_MIG_001_IMPLEMENTATION.md) records the tested
compatibility gate, node-owned vNext-only CLI startup and retained rollback
path. Implementation `1d649ce` remains on the published task branch.
D-041 closed the D-039/D-040 [functional review](outputs/OBP_QA_001_FUNCTIONAL_ACCEPTANCE_V2.md)
with QA tip `fb5b71b`, merge `d695e4a`, and an explicit skipped consumer-NAT
qualification gate. D-039
uses working code and functional tests for task 18 review. A bidirectional
two-relay domain exchange test and focused node/Desktop/API/CLI/Web checks pass.
Consumer-NAT and native platform qualification remain unclaimed; task 19 is
dependency-ready. The earlier [two-VPS product admission](outputs/OBP_QA_001_VPS_PRODUCT_ADMISSION_20260923.md)
checked the owner's selected hosts remotely. No product scenario ran: the
source-free P5 bundle has no product assembly and Linux custom Desktop network
startup is fenced. VPS network independence does not establish consumer NAT.
The separate [three-host Linux P5 lane](outputs/OBP_QA_001_P5_PRODUCTION_20260923.md)
is qualified on exact candidate `c453f3e`.

Current QA record: [acceptance v1](outputs/OBP_QA_001_ACCEPTANCE_V1.md) separates
local preflight from independent-network product evidence and exact-candidate
P5 qualification. The retained QA branch remains published; task 19 is next.

Latest 13:11 UTC: [installation and successor admission blocker](outputs/OBP_QA_001_INSTALLED_SUCCESSOR_BLOCKER.md).
Candidate `2dc5581` is installed on all three hosts; four sequence-2 probes fail
`SequenceRollback`. Relay-b/c are stopped and fenced; preserve durable successor
floors. Host-b measured skew is corrected, NTP remains unsynchronized. No signed
session/fault or qualification. The review/staging-only checkpoints below are historical.

Latest continuation: [local relay renewal correction and evidence](outputs/OBP_QA_001_RELAY_RENEWAL_FIX.md)
is ready for review. A new immutable candidate and fresh V2 bindings are still
required before remote use; old staging/runs and all prior local changes remain.

D-035 supersedes that review-pending checkpoint: [candidate `2dc5581`](outputs/OBP_QA_001_CANDIDATE_2DC5581.md)
is committed/rebuilt and staged on all three hosts. Base/Registry bindings verify;
six native preflights pass. Current gates include fresh topology evidence and
host-b's measured ~7-hour clock error; no signed production session has run.

Previous closure (D-034): accepted Desktop implementation `410a6a0` merged as
`7e4fc14`. Main and the retained Desktop branch are published in this closure.
See [Desktop evidence](outputs/OBP_DESK_001_IMPLEMENTATION.md) and
[NEXT_CONVERSATION.md](NEXT_CONVERSATION.md). The live host remains unchanged.

Historical Web closure follows:
Latest closure (D-032): accepted OBP-WEB-001 implementation `cade635` merged
as `7f49eeb`. Main and retained Web branch are published. Read the
[Web evidence](outputs/OBP_WEB_001_IMPLEMENTATION.md) and updated
[NEXT_CONVERSATION.md](NEXT_CONVERSATION.md). The live host remains unchanged.

Previous closure (2026-09-22, D-031): owner approved the complete CLI work and
requested continuation. Implementation `e0dcc7d` merged as `04bcb30`; main and
the retained CLI branch are published. See [CLI evidence](outputs/OBP_CLI_001_IMPLEMENTATION.md).
At that closure the workspace remained on main. The current local Web branch
and review pointer are in [NEXT_CONVERSATION.md](NEXT_CONVERSATION.md).
The live host, models and default-off networking remain
unchanged; D-029 transport approval remains satisfied.

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

**Latest continuation handoff:** [Product framework / OBP contract review](NEXT_CONVERSATION.md). This supersedes the previous semantic-tuning continuation.

Read only these files first:

1. repository `AGENTS.md`;
2. this `README.md`;
3. [PROGRESS.md](PROGRESS.md);
4. the single task file named by `Current task` below.

Do not load the whole historical distributed-runtime or foundation plan unless
the selected task explicitly requires a section from it.

**Current task:** [`OBP-MIG-001`](tasks/19-OBP-MIG-001.md) — Review after D-041 task 18 merge. D-036 accepts reuse of the already supplied three-physical-host placement for the separate P5 record; local preflight is not consumer-NAT qualification.
The API dependency was satisfied by `7d37a30` under D-030; CLI is now merged
at `04bcb30` under D-031. Read the accepted API and private-WS contracts plus
current Web design rules before further edits. Current Web evidence supersedes
the earlier Planned checkpoint; Desktop is merged under D-034.

The following Web/encoder checkpoints are historical context; they do not override the current task or the new handoff.
**Historical Web branch:** `codex/ku-web-001-workflow`; manual and experimental Ollama Web implementation ready for review.
**Owner follow-up:** actual Ollama (`qwen3:8b`) text encoding requested; see the
[activation-contract amendment](outputs/KU_WEB_001_OLLAMA_AMENDMENT.md). The owner approved this exception under D-023. The extension is implemented; see
[Ollama run instructions and real-model evidence](outputs/KU_WEB_001_OLLAMA_IMPLEMENTATION.md).
**Latest development evidence (2026-09-08):** [headless tests and portable proposal experiments](outputs/KU_ENCODER_MODEL_PORTABILITY_EXPERIMENT.md). The owner's two Vietnamese sources still fail the current Candidate path; the separate lightweight proposal experiment is not integrated into Web or canonical KU.
**Owner-approved follow-up, ready for review:** [durable review-draft implementation and tests](outputs/KU_REVIEW_DRAFT_IMPLEMENTATION.md). Background jobs, retained semantic drafts and checked repairs are active in the local Web. Qwen3 8b passed five inspected development meanings and both original sources through the actual Web API; 1.7b still needs review on all five. Recent jobs retain both original results for inspection. A draft is not a saveable KU; broad model portability remains unproven.
**Gemma 4 comparison (2026-09-09):** [actual native draft tests](outputs/KU_GEMMA4_DEVELOPMENT_PROBE.md) with the same prompts: E4B passed 3/5 inspected meanings, 12B passed 5/5; mean CPU times 66.57/179.78 seconds. Thinking was disabled in all 20 calls. This is development evidence; Gemma is not yet admitted by the Web adapter.
**Latest implementation checkpoint (2026-09-20):** shared selection host activated locally; Qwen3 8b passed 5/5 public development meanings through the real Web API (52.32 s mean, admission below 0.1 s); old jobs remain readable. E4B passed 4/5 after constrained repairs; 12B passed 5/5 (78.58 s mean). All three use the same executor. Independent semantic verification is not yet integrated; Gemma remains development-only. See the [current implementation report](outputs/KU_SEMANTIC_SELECTION_IMPLEMENTATION.md).
**Latest owner-approved architecture (2026-09-09):** [shared semantic selection](../../specs/vnext/KU_SEMANTIC_SELECTION_PROFILE_V1.md) assigns sparse meaning choices to the LLM and full-draft assembly, numbers, anchoring and targeted repair scope to common Rust code. New drafts are explicitly unassessed; independent verification remains a separate integration gate. Read [implementation and current model evidence](outputs/KU_SEMANTIC_SELECTION_IMPLEMENTATION.md) before continuing. These follow-up edits are local and uncommitted, not part of the earlier pushed milestone.
**Historical Web review branch:** `codex/ku-web-001-workflow`
**MVP direction:** [D-021](DECISIONS.md#d-021--prioritize-an-early-open-source-concept--mvp).
The separate `codex/ku-enc-003-model-qualification` branch remains blocked on
locked qualification inputs; it is preserved at `4a8f29d` and has no model runs.

KU-API-001 now projects the eleven registered operations through three private
REST routes. Read its [implementation evidence and integration limits](outputs/KU_API_001_IMPLEMENTATION.md).
The owner accepted it under [D-022](DECISIONS.md#d-022--ku-api-001-accepted-and-merged-web-handoff-next);
merge `3eba370` is on `origin/main`. The Web manual MVP is implemented on its separate task branch; see
[run instructions and implementation limits](outputs/KU_WEB_001_IMPLEMENTATION.md).
The experimental host captures user text with explicit consent; signed Registry
and secret provisioning remain operator responsibilities. Real `qwen3:8b`
development inference, private save and restart/read passed; AI remains unqualified.

KU-RUN-001 implements the D-016 registration and node-owned local service.
The owner authorized merge `d141701`, now on `origin/main`. See the
[implementation evidence and limits](outputs/KU_RUN_001_IMPLEMENTATION.md)
and [PROGRESS.md](PROGRESS.md). The owner authorized starting KU-ENC-001,
followed by the shared encoder runtime. The owner now authorized KU-API-001
as the next local MVP step, followed by KU-WEB-001 after API acceptance/merge.

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
`origin/main` at `dc04b71`. Qualification continues on its separate task branch.
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
OBT issuance for accepted work; see [D-011â€“D-014](DECISIONS.md#d-011--deterministic-identity-after-semantic-normalization).
The economic choice changes the earlier benefit-only direction and needs a
versioned specification amendment before implementation. This audit does not
enable minting or change canonical bytes, application code or rollout state.

Copy/paste prompt: use the Vietnamese prompt in [NEXT_CONVERSATION.md](NEXT_CONVERSATION.md).

More prompts are in [NEW_CONVERSATION_PROMPTS.md](NEW_CONVERSATION_PROMPTS.md).

## Package contents

| File | Purpose |
|---|---|
| [DECISIONS.md](DECISIONS.md) | Owner decisions, architectural boundaries and claim language |
| [CAPABILITY_STATUS.md](CAPABILITY_STATUS.md) | Compact OBP capability/status map used by product planning |
| [MASTER_PLAN.md](MASTER_PLAN.md) | Two-lane dependency graph, sequencing and shared exit gates |
| [PROGRESS.md](PROGRESS.md) | Authoritative status/branch/commit ledger for this workstream |
| [NEW_CONVERSATION_PROMPTS.md](NEW_CONVERSATION_PROMPTS.md) | Short prompts for starting, reviewing or merging one task |
| [`tasks/README.md`](tasks/README.md) | Index of all 24 tasks |
| [`outputs/README.md`](outputs/README.md) | Naming and placement rules for task audit/evidence outputs |

## Working rules

For the current continuation, D-024 and NEXT_CONVERSATION.md override the clean-main
start and automatic push instructions below. Preserve the dirty baseline and do not
publish Git changes without owner authorization.

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
