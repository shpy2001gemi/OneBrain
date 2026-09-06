# Workstream progress

> This is the authoritative execution ledger for this folder.
> Allowed states: `Planned`, `In progress`, `Review`, `Merged`, `Blocked`, `Deferred`.

## Current checkpoint

- Current task: `KU-ENC-003`
- Current branch: `main`; no next-task branch created
- Planned task branch: `codex/ku-enc-003-model-qualification`
- Current state: `Planned`; owner will start this task in a new conversation
- Prerequisite evidence: [KU-ENC-002 implementation and limits](outputs/KU_ENC_002_IMPLEMENTATION.md)
- Last accepted task: `KU-ENC-002`, implementation `16408c6`, reviewed tip `a687600`, D-020
- Last merged task: `KU-ENC-002`, merge `dc04b71b48b27588800b682ef1e71d4506945db1` on `origin/main`
- Next action: new conversation executes `tasks/23-KU-ENC-003.md`; no qualification work started here
- Encoder framework direction and sequence: owner accepted at `e513552` under D-018; no further direction approval needed
- Default rollout change authorized: **no**
- Mobile work authorized by this package: **no**

## Task ledger

| Order | Task | State | Branch | Dependency | Merge commit/evidence |
|---:|---|---|---|---|---|
| 1 | `KU-REV-001` | Merged | `codex/ku-rev-001-canonical-audit` | — | `25d008d211f450d15ba1a63cacc0368298ed3e7a` on `origin/main`; [authority audit](outputs/KU_AUTHORITY_AUDIT.md), D-011–D-014. |
| 2 | `KU-REV-002` | Merged | `codex/ku-rev-002-runtime-map` | `KU-REV-001` | [Runtime gap map](outputs/KU_RUNTIME_GAP_MAP.md), including owner D-011–D-014; merge `b872263` on `origin/main`. |
| 3 | `KU-CON-001` | Merged | `codex/ku-con-001-product-contract` | `KU-REV-002` | [Approved contract](../../specs/vnext/KU_PRODUCT_WORKFLOW_PROFILE_V1.md); KU-PC-A/B/C accepted under D-015; merge `2cbc8f2` on `origin/main`. |
| 4 | `KU-RUN-001` | Merged | `codex/ku-run-001-shared-service` | `KU-CON-001` | Owner-authorized merge `d141701` on `origin/main`; [implementation evidence](outputs/KU_RUN_001_IMPLEMENTATION.md). |
| 5 | `KU-API-001` | Planned | `codex/ku-api-001-local-api` | `KU-RUN-001`, `KU-ENC-002` | — |
| 6 | `KU-CLI-001` | Planned | `codex/ku-cli-001-workflow` | `KU-API-001` | — |
| 7 | `KU-WEB-001` | Planned | `codex/ku-web-001-workflow` | `KU-API-001` | — |
| 8 | `KU-DESK-001` | Planned | `codex/ku-desk-001-workflow` | `KU-WEB-001` | — |
| 9 | `KU-QA-001` | Planned | `codex/ku-qa-001-cross-surface` | `KU-CLI-001`, `KU-DESK-001`, `KU-ENC-003` | — |
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
| 21 | `KU-ENC-001` | Merged | `codex/ku-enc-001-framework-contract` | `KU-RUN-001` | Owner-authorized merge `22599d0` on `origin/main`; [contract evidence](outputs/KU_ENC_001_CONTRACT.md), D-019. |
| 22 | `KU-ENC-002` | Merged | `codex/ku-enc-002-shared-encoder` | `KU-ENC-001`, `KU-RUN-001` | Owner accepted under D-020; merge `dc04b71` on `origin/main`; [implementation and verification](outputs/KU_ENC_002_IMPLEMENTATION.md). |
| 23 | `KU-ENC-003` | Planned | `codex/ku-enc-003-model-qualification` | `KU-ENC-002` | Real-model/resource evidence; reuse MOB-06 mobile ownership. |

## Per-task update protocol

### KU-ENC-002 review evidence — 2026-09-06

Closure: the owner reviewed, accepted and requested completion of KU-ENC-002,
reserving the next task for a new conversation (D-020). Clean reviewed tip
`a687600` matched its fetched remote. Fresh extraction (16), node KU (19),
bounded HTTP (3), format, generated-bundle, global vNext and diff checks passed.
Merge `dc04b71b48b27588800b682ef1e71d4506945db1` was pushed to `origin/main`.
KU-ENC-003 remains Planned; its branch and conversation were not created.
Task branches are retained; no cleanup was requested.

- Shared native strict validator/compiler, 48 oracle cases, two complete/partial
  multi-chunk jobs, bounded provider and durable node integration implemented.
- Native suites: 153 encoder, 123 node, 109 AI and seven existing SEM tests pass.
  KU coverage includes four real extraction process-kill phases plus the existing
  save crash matrix. Inference/token manifests are explicitly fixtures.
- Workspace check, format, generated bundle, 62 Python tests and global vNext
  validator pass. Clippy uses the existing warning policy, not zero-warning gates.
- [Evidence](outputs/KU_ENC_002_IMPLEMENTATION.md) records the first planner's
  single whole-source scope, unavailable unit/review authority and unqualified
  real-model/device behavior. KU-ENC-003 carries those qualification inputs.
- Review-stage scope included no new Base IDL/canonical semantics, default
  rollout, mobile implementation or model download. The subsequent merge is
  recorded above.

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
- Reviewed content checkpoint: `8b6aa9d`, pushed successfully to
  `origin/codex/ku-con-001-product-contract`. The final tracked tip additionally
  contains the ledger-only commit recording this checkpoint. This is not a
  merge commit or owner acceptance of KU-PC-A/B/C.

### KU-CON-001 owner acceptance — 2026-09-05

- Owner answered "đồng ý" after the completed review at `b5956e8`.
  [D-015](DECISIONS.md#d-015--ku-product-contract-accepted) records acceptance
  of KU-PC-A/B/C and merge of the reviewed task. The preceding review section
  is historical evidence of the candidate before this acceptance.
- Profile and machine inventory now report owner approval with registration
  pending. Existing byte formats and numeric domain/payload inventories remain
  unchanged; no runtime or rollout was enabled.
- Added a mutation test binding acceptance to D-015, the reviewed commit and
  all three approved items. Approval cannot erase technical dispatch gates.
- Approved contract tip `9f67022` was clean, pushed and synchronized before
  merge. Merged and pushed `2cbc8f263961d5a6368ef2c7bdc5a77f209d5b21`
  to main. Current task advances to KU-RUN-001, which has not started.
  Preserve the registration/vector gates before dispatch; no branch deletion
  or runtime/default-rollout change was requested or performed.
- Re-ran the same six-module unittest command from the review evidence:
  PASS, 102 tests (34 KU contract tests and 68 existing contract tests).
- `python scripts/ci/validate_vnext_contracts.py` — PASS, including 480
  specification links and unchanged canonical inventories.
- `git diff --check` — PASS.

### KU-RUN-001 registration preflight — 2026-09-05

- Started from `91cc715b547b71941f6d66fea2093fc2326eb481`, clean and equal
  to freshly fetched `origin/main`; created the exact task branch
  `codex/ku-run-001-shared-service`. State moved through `In progress` during
  preflight to `Blocked`. Current task remains KU-RUN-001.
- Read the start/resume prompts, repository instructions, handoff
  README/decisions/progress, task, authority audit, runtime gap map and approved
  KU profile. Inspected the implicated Base interface/ownership/storage/canonical
  profiles, inventories, validator and local adapter dispatch boundary.
  This is prerequisite evidence, not the completed runtime required-read or
  implementation acceptance gate.

#### Missing gate and scope boundary

1. [KU profile §1](../../specs/vnext/KU_PRODUCT_WORKFLOW_PROFILE_V1.md#1-authority-and-approval-boundary)
   requires domain registration and golden equality/separation vectors before
   production hashing, and generated Base payload registration/compatibility
   revision before dispatch. D-015 accepts KU-PC-A/B/C but expressly retains
   those gates; their approval is not being reopened.
2. The [KU inventory](../../../src/test-vectors/vnext/ku-product-workflow-v1.json)
   still has `owner_approved_pending_registration`,
   `implementation_enabled: false`, an empty `base_local_command_ids`,
   `domain_registry_allocated: false`, and null operation wire IDs. The
   [KU validator](../../../scripts/ci/validate_ku_product_contract.py)
   explicitly requires these values. Its successful result proves contract
   consistency in that pending state, not readiness to dispatch.
3. [Canonical §6.2](../../specs/vnext/CANONICAL_PROFILE_V1.md#62-reserved-v1-domains)
   calls addition of a domain a contract change. The current canonical
   inventory has no `semantic-content` entry. The
   [Base IDL](../../../src/test-vectors/vnext/base-v1-runtime-interface-v1.json)
   has no generated KU operation DTOs. Its generic `BaseLocalCommandV1.kind`
   is not an allocated KU discriminator. [Base §§8–9](../../specs/vnext/BASE_V1_RUNTIME_INTERFACE_PROFILE.md#8-generated-projections)
   require generated projections and append-only history with a profile-minor
   increment for additive registration.
4. The selected task scopes runtime service implementation. The handoff
   [working rules](README.md#working-rules) state: "Any wider change requires
   an explicit scope revision." The task does not explicitly assign the
   outstanding canonical/IDL registration changes. Treating implementation
   as permission to change those frozen inventories would silently expand
   that scope. This is a missing prerequisite/scope assignment, not a new
   conflict with the owner's approved semantic design.
5. [The Base adapter](../../../src/onebrain-node/src/base_runtime.rs) currently
   defaults to `UnavailableBaseLocalOperationAdapter`; local confirmation
   passes a `BaseLocalCommandV1` to that adapter. Installing a hand-written KU
   dispatcher with invented IDs would bypass the gate and would not satisfy
   the approved service contract.

#### Concrete proposed scope extension, pending owner direction

Extend KU-RUN-001 with a prerequisite registration phase, then continue its
existing runtime objective on the same branch:

1. Register the already approved `semantic-content/1` domain in the canonical
   contract/inventory and typed core domain declarations; add golden canonical
   byte/hash equality and separation vectors for the finite approved
   normalization. Preserve existing domains, IDs and original artifact bytes.
2. Register the approved eleven operation payload mappings and eighteen DTOs
   in the Base machine IDL; append discriminator history, advance the additive
   profile/compatibility declarations and regenerate affected projections.
   Add old-host rejection and generation/history drift checks. Do not allocate
   REST routes, CLI commands or WS events in this phase.
3. Update the KU inventory/validator to recognize registered state only when
   those exact registrations and vector gates pass. Then implement the
   node-owned service, encrypted atomic/recoverable save and all existing
   KU-RUN-001 acceptance cases. Keep API/UI, OBP orchestration, mobile
   implementation and D-012–D-014 distribution/work/reward changes excluded.

Owner direction is requested only for this scope extension. No request to
approve KU-PC-A/B/C again, merge, delete branches or enable default rollout.

#### Fresh validation and evidence limit

- `python scripts/ci/validate_ku_product_contract.py` — PASS: 11 operations,
  18 DTOs, 11 fixtures; tool explicitly reports registration pending.
- `python -m unittest scripts.ci.test_validate_ku_product_contract
  scripts.ci.test_validate_base_v1_runtime_interface` — PASS: 74 tests.
- `python scripts/ci/validate_vnext_contracts.py` — PASS: existing contract
  inventories, including 21 foundation domains and 27 Base runtime operations.
- `git diff --check` — PASS. Local file-link check for README/PROGRESS — PASS.
- Preflight content checkpoint `8464770` was pushed to
  `origin/codex/ku-run-001-shared-service`; the branch also includes the
  ledger-only follow-up recording this checkpoint. No merge or branch deletion.
- Source, canonical contracts, generated declarations, tests and rollout state
  are unchanged. Only README/PROGRESS handoff records changed. Runtime tests,
  workspace check and Rust format are not claimed; KU-RUN-001 remains
  incomplete and cannot be marked Review on this evidence.

### KU-RUN-001 review evidence — 2026-09-06

- Implementation checkpoint `b608a82` is pushed to
  `origin/codex/ku-run-001-shared-service`. The branch tip also contains this
  ledger-only follow-up recording that checkpoint; neither commit is a merge.
- D-016 supersedes the historical preflight blocker above. The owner approved
  its registration scope and reported `onebrain.live`; no DNS/deployment work
  was needed or performed.
- Delivered registered semantic identity/goldens, Base 1.2 payload/DTO/history
  generation, authenticated node-owned KU service, encrypted recoverable
  private save, snapshot index/revisions and typed lifecycle/recovery fences.
  Full evidence and integration limits:
  [KU_RUN_001_IMPLEMENTATION.md](outputs/KU_RUN_001_IMPLEMENTATION.md).
- Validation: node library **117 PASS**, core foundation **196 PASS**, semantic
  golden suite **2 PASS** plus child run, Base contract **21 PASS**, Base facade/
  exchange/recovery/capability/index integration **22 PASS**. Six real process
  kills verify partial-save visibility and exact recovery without model replay.
- Whole-workspace `cargo check --workspace --locked -q` and
  `cargo fmt --all -- --check` pass. Generated `--check`, global vNext contract
  validator and **85 Python tests** pass. Existing TypeScript conformance and
  both Dart conformance tests pass outside the mobile subtree.
- Host input/Registry/public-read ports are explicit. Test encoders are
  controlled fixtures; no live AI, automatic Registry synchronization, remote
  work, minting or product UI qualification is claimed. Private export is a
  Base management reservation; portable KU metadata/archive round-trip remains
  unqualified. Network and default rollout are unchanged.
- Current task remains KU-RUN-001 for owner review. Do not merge/delete the
  branch or start KU-API-001 without the corresponding instruction.

### Encoder framework research and task amendment — 2026-09-06

- Owner accepted KU-RUN-001 and requested the D-017 framework research and
  backlog amendment. Runtime remains unchanged and unmerged.
- [Research output](outputs/KU_ENCODER_FRAMEWORK_RESEARCH.md) records source
  findings from legacy tool and v2 extraction paths, unsafe semantic defaults,
  available structured adapter, overlap with AI-001/AI-003/FID/MOB-06 and the
  shared framework gap. Official Ollama/llama.cpp/Anthropic references were
  checked; no live model or latency/accuracy benchmark was run.
- Added KU-ENC-001/002/003; supplemented API, QA and integration acceptance,
  dependency graph and indexes. Research includes a candidate workflow,
  prompt guidance, data responsibilities, resource policy and evaluation plan.
  New machine schema and production implementation remain future task outputs.
- This documentation-only amendment stays on the accepted task's handoff
  branch. It grants no mobile implementation, runtime rollout or merge action.
- Validation passed: 200 local links, 23 task files with an acyclic dependency
  graph and the new API/QA gates, global vNext contract validator and
  `git diff --check`. All changes are handoff Markdown; no runtime test rerun
  or live-model/mobile performance claim is needed for this documentation edit.

### KU-RUN-001 merge and encoder handoff — 2026-09-06

- Owner explicitly authorized merging KU-RUN-001 into main and starting
  KU-ENC-001. Fresh generated/global contract checks, 13 KU runtime tests,
  workspace check and format passed on the clean synchronized branch.
- Merged tip `13e03f3` with merge `d1417018a236798a910ceb625fbe5fd0b10dc406`
  and pushed `origin/main`. No branch deletion or rollout change.
- README/progress pointer advances to KU-ENC-001; the accepted direction is
  D-018. No further merge or framework-direction approval is required here.

### KU-ENC-001 contract review — 2026-09-06

- Started from synchronized main `5d8fba077076597da163d5f17b8e290f28eb12c9`.
- Reviewed contract commit: `a6f0a00`. All 18 manifest artifact hashes were
  compared against exact Git index blobs to verify LF portability before commit.
- [Framework contract](../../specs/vnext/KU_EXTRACTION_FRAMEWORK_PROFILE_V1.md)
  and [review evidence](outputs/KU_ENC_001_CONTRACT.md) define one host-controlled
  extraction path, six DTO schemas, vi/en prompts, eight generated artifacts,
  48 cases/two jobs, explicit unsupported semantics and qualification gates.
- Validation: 18 encoder tests, 44 KU/product regression tests, seven existing
  Rust SEM tests, generated bundle
  and Base checks, global vNext validator, independent Draft202012 comparison
  and diff/link/dependency checks. Commands and evidence boundaries are recorded
  in the review output.
- No production inference/compiler, real model/hardware qualification, mobile
  implementation/evidence, accepted bytes, IDL registrations or rollout changes.
- Task remains Review on its own branch; no merge/deletion or KU-ENC-002 start
  is included in this contract handoff.

### KU-ENC-001 owner acceptance

Owner acceptance update: the owner reviewed and accepted KU-ENC-001 at handoff
`e4c1bb6`; D-019 records the concrete accepted contract. This update changes only
handoff metadata, preserving the validated bundle. No repeat contract approval
is needed. Merge remains a separate explicit instruction under D-010; do not
start the dependent KU-ENC-002 branch before that merge.
Generated bundle integrity, global vNext validation and diff checks pass for
this handoff-only update.

### Update protocol

Merge update: owner explicitly authorized merging KU-ENC-001 and starting
KU-ENC-002. Clean synchronized tip `7a360a7` passed fresh generated-bundle,
62 Python tests, global vNext and diff checks. Merge
`22599d036f903c5b5be2cb3f445ab6904e92896c` was pushed to main. No branch deletion
or rollout change was requested or performed.

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
