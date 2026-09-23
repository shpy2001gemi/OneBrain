# Decisions and claim boundary

## D-038 — Use two existing VPS for the product scenario attempt

On 2026-09-23, after being told the available three P5 machines are VPS rather
than ordinary consumer hosts, the owner directed: “hãy dùng 2 trong 3 vps .
3 vps đso đang ở mạng độc lập với nhau”. This selects the existing independent
VPS networks for a scoped product staging attempt without requesting new SSH
approval. It does not itself provide a node-owned product host assembly, Linux
native Desktop lifecycle support or measurements of ordinary consumer NAT.
The [remote admission result](outputs/OBP_QA_001_VPS_PRODUCT_ADMISSION_20260923.md)
records those gates. Keep P5 qualification distinct, the accepted scenario
oracles unchanged, default networking off and OBP-MIG-001 unstarted.

## D-037 — Approve descriptor-history correction; prioritize real happy case

On 2026-09-23 the owner accepted the concrete correction proposal and said
“đồng ý duyệt. Mục tiêu mau chóng chạy happy case.” This extends task 18 to the
shared verified-history admission path, probe/P5 input bindings and focused
regressions in [the additive profile](../../specs/vnext/P5_DESCRIPTOR_HISTORY_ADMISSION_V1.md).
Continue necessary local candidate preparation and previously authorized scoped
three-host execution toward the happy case. Preserve failed runs and durable
floors. No Git push/merge, default networking, tuning, mobile or task 19.
Happy-case evidence must remain distinct from full qualification. Existing
SSH/sudo, D-036 placement reuse and same-key policy approvals remain in force.

## D-036 — Retain the owner-accepted three-physical-host placement evidence

On 2026-09-23 the owner explicitly clarified:
“trước đso đã chốt và gửi kèm thông tin 3 vps chạy trên 3 máy vật lý khác nhau rồi,
bạn không cần phải lấy lại bằng chứng đó. Tôi đã quyết định nội dung này.”

The existing three VPS placement on three separate physical machines is an
owner-accepted fact for this continuation. Reuse the previously supplied placement
information and retained evidence; do not require another provider contact,
placement collection or owner confirmation. This supersedes handoff language
treating new physical-placement evidence as an unresolved external prerequisite.

Keep original evidence bytes, dates and provenance. This decision is not a new
provider-issued document, a new measurement or provider-document verification.
Retain `owner-telephone-verified-provider-document-pending`; bind the accepted
evidence through the canonical V2 authority path for the current candidate.
Do not weaken a verifier or relabel old probes as current evidence.

Host-b clock correction, exact installed-generation/public exports, fresh relay
successor descriptors and possession probes, complete signed V2 admission and
actual session/fault/cleanup evidence remain technical work. The separate
two-consumer-network product scenarios remain unexecuted. No qualification,
default rollout, tuning, Git publication or OBP-MIG-001 follows from this decision.

## D-035 — Accept relay renewal correction and create QA candidate

On 2026-09-23 the owner answered “tôi đồng ý duyệt” to the reviewed
OBP-QA-001 relay renewal correction and explicit request to create a new commit
candidate on `codex/obp-qa-001-nat-canary`, rebuild and prepare fresh V2 authority.
This authorizes committing the retained QA package and correction on that branch.
Do not ask again for that review, candidate commit, D-029, SSH execution or the
same-key September 23–30 policy renewal. Keep old staging and attempts intact.
No merge, remote Git publication, default rollout or task 19 is included.
Signed session/faults still require the complete fresh exact-candidate V2 gate.

## D-034 — Merge and publish OBP-DESK-001

After accepting Desktop review under D-033, the owner explicitly instructed
“merge và push nhé”. This authorizes committing the complete reviewed local
OBP-DESK-001 changes, merging the retained Desktop branch into main and publishing
both branches. Record the actual merge only after it exists. D-033's pending Git
closure condition is satisfied by this instruction.

Preserve the original workspace and all retained branches. Advance the handoff
to OBP-QA-001, Planned, after the merge; do not start QA during this closure.
The accepted implementation limits remain. No live-host change, networking
activation, model tuning, mobile, qualification or default rollout is authorized.

## D-033 — Accept OBP-DESK-001 review

The owner stated “tôi đồng ý duyệt review” after the local Desktop implementation,
verification results and explicit platform limits were presented. This accepts
the reviewed OBP-DESK-001 implementation and its registered Desktop projection.
No further approval of the same implementation or D-029 transport is required.

The accepted scope uses one shared node-owned service, local credential handoff,
Windows lifecycle fencing, explicit process restart and durable client recovery.
The stated native WebView, real sleep/network-switch, other-OS and NAT qualification
limits remain. API/node/CLI and the live host remain unchanged.

This message approves review; it does not explicitly request commit, publication,
merge, branch deletion or next-task implementation. Under D-010 and the current
handoff, retain the local changes and record owner acceptance while the task stays
Review until an actual authorized merge exists. OBP-QA-001 remains Planned.
No networking activation, model tuning, mobile or default rollout is authorized.

## D-032 — Accept OBP-WEB-001 review and close the task

The owner stated “tôi đã duyệt review” after the completed Web implementation
and evidence were presented, then directed continuation after usage reset.
This accepts OBP-WEB-001 and its closure through commit, merge and publication.
Preserve the reviewed implementation and retained task branches. Record the
actual merge in PROGRESS once it exists; hand off OBP-DESK-001 as Planned.
This closure does not implement Desktop, activate networking, change the live
host or models, qualify platforms, or enable default rollout. D-029 remains
satisfied without another transport approval.

> Authority: owner-approved workstream direction, 2026-09-05.
> This file controls planning and product wording; it does not supersede a
> frozen protocol contract or founder directive.

The owner clarification in D-011 through D-014 was recorded on 2026-09-05
after this package was created. It revises the requested product direction.
D-014 explicitly requests a change to the earlier benefit-only OBT issuance
direction. It is not a claim that the frozen contracts or production ledger
already implement that change; the exact conflicting references and required
contract work are recorded in the KU authority audit.

## D-001 — Resume KU now

KU review and local product work may start immediately. OBP is not an
architectural blocker for local KU creation, validation, persistence, search,
inspection or revision.

## D-002 — Stabilize rather than redesign OBP

Authenticated sessions, scoped inventory, deterministic reconciliation,
persisted journals, multi-carrier behavior, outbound-first routing and the
permissionless relay are treated as implemented foundation components.

Changes to their canonical wire, authority or privacy semantics require a
specific defect, incompatibility or approved contract revision. Product work
must consume these boundaries instead of inventing parallel networking rules.

## D-003 — Productize OBP in a separate lane

The remaining desktop product gap is orchestration:

- node-owned Reachability Manager lifecycle;
- trusted-local bootstrap source configuration;
- bounded discovery and refresh;
- outbound relay reservations and target advertisements;
- automatic route selection, failover and durable outbox delivery;
- product API plus CLI/Web/Desktop presentation;
- product-level two-node NAT and outage acceptance.

This work remains opt-in/default-off until its gates pass.

## D-004 — Seeder and relay terminology

Allowed wording:

> Anyone can self-host a compatible vNext relay/rendezvous service. Bootstrap
> sources are replaceable discovery hints, not identity or knowledge roots of
> trust.

Forbidden wording:

- “`onebrain-seed` is the production vNext seeder.”
- “A seed or relay cannot be malicious.”
- “Inclusion in a directory makes a relay trusted.”

`onebrain-seed` is a legacy TCP/JSON prototype. The supported vNext direction
is `onebrain-relay`, signed bootstrap manifests, signed relay/peer invitations,
authenticated PEX, rendezvous/DHT records and local cache.

## D-005 — Exact malicious-infrastructure claim

Allowed wording:

> A malicious seed, mirror or relay can deny service, return bounded junk,
> delay, drop, duplicate, reorder or censor traffic and observe available
> metadata. It cannot by itself impersonate an expected NodeID, alter accepted
> canonical content undetected, terminate the inner authenticated OBP session
> as the target, or acquire content/feed/policy/knowledge authority.

No document or UI may promise absolute Sybil prevention, global traffic-analysis
resistance or guaranteed availability.

## D-006 — Bootstrap and seed independence

A new Internet node may initially use DNS, a public IP, signed bootstrap
manifest, manual invitation, file, URL or QR to locate a first source. DNS/IP
is location only; source/relay/peer identity is verified independently.

After admission of other sources, nodes may learn routes from authenticated
PEX, rendezvous/DHT records, cached descriptors, provider leases and signed
reachability advertisements. Loss of the first bootstrap source must not alter
identity or local usefulness.

A completely new node with no address, cache, invitation, nearby peer or
reachable bootstrap path cannot discover a disconnected network. Product copy
must state that limitation honestly.

## D-007 — NAT and bidirectional connection

Ordinary nodes must not require a public IP, port forwarding, UPnP, router/NAT
changes or inbound firewall configuration. Both peers may create outbound
reservations to permissionless relays and obtain a bidirectional authenticated
OBP carrier.

The public endpoint requirement belongs to relay operators. An offline or
suspended target without a live reservation cannot promise immediate delivery;
work remains a durable pending intent. The optional ciphertext mailbox remains
a separate pending gate.

## D-008 — Shared product surfaces

CLI, local Web and Desktop must project one node-owned KU/OBP service contract.
They may have different presentation, but may not implement different authority,
consent, lifecycle, completion or error semantics.

## D-009 — Release boundary

The repository has production-reference evidence under the Base owner waiver,
not an unrestricted claim that every product/platform lane is production-ready.
Strict Base qualification, default enablement, general operator rollout,
Windows/macOS outbound-first qualification, mobile and browser lanes remain
separate decisions.

## D-010 — Merge and branch policy for this workstream

Each task uses its declared `codex/` branch. Completion means the branch is
validated and pushed with an updated handoff ledger. Merge and local branch
deletion require explicit owner instruction. Accepted tasks are merged before
dependent tasks branch, unless `MASTER_PLAN.md` explicitly permits parallel
work.

## D-011 — Deterministic identity after semantic normalization

The owner explicitly selected: "Cùng ngữ nghĩa chuẩn hóa → cùng CID".
AI and rule-based extraction must converge to the same semantic content
identity when their normalized semantic representation is identical under the
same versioned canonical profile. This is not a promise that arbitrary models
interpret the same natural-language text identically.

Registry standardization alone is insufficient. The product contract must
name the semantic identity boundary, supported normalization rules, unresolved
concept behavior and cross-encoder conformance evidence. Source/model/node/run
provenance remains separate from semantic identity. Existing generic ObjectCID
bytes, which include disclosure, references and other envelope fields, must
not silently acquire a different meaning. Existing alternative interpretations
and original immutable bytes remain preserved.

## D-012 — Regular Registry releases and node-assisted distribution

The owner requires a standard Concept Registry updated regularly, obtainable
from publisher servers and other nodes. The target must retain signed immutable
releases, exact release verification, CCID stability, atomic activation and
offline usability with an already active valid release.

The large release currently has distribution policy
`MIRROR_OR_OFFLINE_ONLY_NO_OBP_GOSSIP`. Peer-assisted package/chunk transport,
release discovery, update cadence, delta compatibility and historical release
reproducibility need explicit contract coverage before implementation. A peer
or mirror supplies bytes; it does not acquire release-signing or KU authority.
No particular update interval or network transport is approved by this entry.

## D-013 — Delegated encode and independent verify work

The owner requires nodes without a local AI encoder to request encoding from
capable nodes, and capable nodes to opt into automatically discovering and
claiming eligible encode/verify work. A local rule-based route remains useful
within its supported domain.

The target consumes the capability Definition/Manifest/Offer/Permit/Execution
separation and blind fidelity attempts. Worker participation does not imply
unrestricted source access or publication. Job ownership, bounded claims,
expiry/reassignment, restart recovery and signed work evidence need a product
contract; legacy encoding gossip is not accepted as its vNext implementation.

## D-014 — Direct OBT issuance for accepted encode/verify work

The owner explicitly selected: "Phát hành OBT trực tiếp từ encode/verify".
The requested reward trigger is accepted encoding or verification work, without
waiting for a later BenefitEvent. Do not replace this requirement with a
fee/bounty or a benefit-contingent ContributionReceipt.

This changes the earlier benefit-only issuance direction in Research Baseline
§1.5, §3.1 and §3.5, and requires a versioned canonical economic amendment to
the current no-reward boundaries in FID-001 §6, FID-002 §6 and FID-003 §5.
The owner has resolved the direction; the issuance mechanism remains to be
specified. No additional confirmation of this same choice is required.

Acceptance evidence must feed a separate reward-authorization/ledger boundary;
a returned result or signature is not itself mint authorization. Define work
admission, accepted correct verification (including evidenced mismatch),
duplicate-work/replay handling, correlated-worker abuse, bounded issuance,
disputes and partition-safe settlement before enabling payouts. Exact amounts,
formula, supply policy and finality mechanism remain open, not inherited from
legacy raw-text-length or agreement-bonus rules.

OBT must still not become KU truth, fidelity, discovery or adoption authority.
Local KU operations remain usable when rewards are unavailable. This direction
does not enable a production wallet/network lane or authorize rewriting legacy
history. KU-REV-001 records the amendment and gaps; implementation requires
explicitly scoped specification and implementation tasks beyond that audit.

## D-015 — KU product contract accepted

After review of KU-CON-001 at `b5956e8e3d27598d118c0529ac416e54549b981e`,
the owner answered "đồng ý" on 2026-09-05. This accepts KU-PC-A, KU-PC-B and
KU-PC-C in [the KU product profile](../../specs/vnext/KU_PRODUCT_WORKFLOW_PROFILE_V1.md)
and the merge of the reviewed task.

- KU-PC-A: separate SemanticContentCID under `semantic-content/1`, with the
  exact finite normalization and private provenance separation in profile §2;
  existing ObjectCID and legacy bytes retain their meanings.
- KU-PC-B: the shared local workflow, 11 operations and 18 bounded DTOs,
  private save and explicit prepare/confirm/reconcile semantics.
- KU-PC-C: immutable predecessor/successor artifacts with a private local
  revision journal, preserving concurrent successors without replicated
  supersession authority.

Domain registration, golden equality/separation vectors, generated Base
payload registration and compatibility history remain required technical
gates before runtime dispatch. Acceptance does not allocate unspecified
numeric IDs, routes or WS events or enable an implementation/rollout lane.
D-012–D-014 retain their separate required distribution, work and economic
specification dependencies. Do not ask the owner to approve KU-PC-A/B/C again.

## D-016 — KU-RUN-001 registration scope accepted

On 2026-09-05 the owner answered "đồng ý" to the concrete registration scope
extension recorded in PROGRESS.md at `9fb94dd`. KU-RUN-001 now includes the
approved semantic domain and golden vectors, generated Base KU payload/DTO
registration, append-only history and compatibility revision, and corresponding
validator changes, followed by the original runtime implementation and gates.
No further approval of these prerequisite changes is needed. Existing task
exclusions and default-off network policy remain in effect.

The owner also reports owning the Internet domain `onebrain.live`. This is
infrastructure context for later separately scoped hosting/discovery work;
it does not change the cryptographic `semantic-content/1` domain or authorize
DNS, deployment, listener or rollout changes in KU-RUN-001.

## D-017 — Shared local encoder framework and KU-RUN acceptance

On 2026-09-06 the owner approved the completed KU-RUN-001 task and requested
research plus task coverage for a shared local/personal-AI encoding framework.
The owner described the earlier move from model-controlled tools to structured
extraction followed by workflow-controlled execution, with improved observed
speed. That observation is owner history, not a fresh measured benchmark.

AI proposes concepts, relations and grounded structured fields. Workflow owns
tool selection/calls, Registry lookup, validation, compilation and persistence.
The framework must support different models and resource-limited platforms
without loosening semantic or authority rules. Exact normalized-semantic
identity remains D-011; equal raw text across different models is a convergence
target to measure, not a newly approved canonical equivalence rule.

The [research and overlap map](outputs/KU_ENCODER_FRAMEWORK_RESEARCH.md)
identifies a shared framework gap beyond KU-RUN-001 and beyond completed
AI-001/AI-003/FID components. Add KU-ENC-001/002/003 for contract, shared runtime
and qualification; reuse MOB-06 for mobile providers and physical-device
evidence. Supplement API and cross-surface tasks rather than duplicating their
semantics. Research candidate schemas, budgets and thresholds still need the
normal contract task; this decision does not freeze unspecified fields.

KU-RUN-001 acceptance is recorded against implementation `b608a82` and handoff
tip `0d50550`. The owner did not explicitly request merge or branch deletion in
this message, so both remain pending. This follow-up changes research/backlog
documentation only, not the accepted runtime or mobile implementation.

## D-018 — Encoder framework direction and task sequence accepted

After the research/backlog checkpoint `e513552`, the owner answered
"tôi đồng ý". This accepts the proposed shared framework direction and the
KU-ENC-001 → KU-ENC-002 → KU-ENC-003 sequence, including reuse of MOB-06 and
the updated API/QA dependencies. Do not request approval of that direction
again. KU-ENC-001 now has an accepted design brief; its precise schema,
compiler coverage and measurable qualification thresholds remain its outputs.

The accepted boundary is grounded model proposals with workflow-owned tools,
Registry resolution, validation and canonical compilation. Exact identity is
guaranteed for equal validated semantics under the same profile; cross-model
raw-text convergence is measured. Limited hardware does not relax these rules.

This acceptance does not explicitly instruct a merge, branch deletion,
deployment or mobile implementation. The prepared KU-RUN branch remains
available for the explicit merge step required by the handoff instructions.

## D-019 — KU-ENC-001 contract accepted after owner review

The owner stated "tôi đã review và đồng ý" after the KU-ENC-001 handoff at
`e4c1bb6`, accepting reviewed contract commit `a6f0a00`. This accepts the
[shared extraction framework](../../specs/vnext/KU_EXTRACTION_FRAMEWORK_PROFILE_V1.md),
its six closed DTO schemas, vi/en prompts, supported/unsupported semantic
surface, source/Registry bindings, compile rules, resource/lifecycle policy,
private evidence contract, corpus and predeclared qualification gates.

Do not request approval of this contract or the D-018 direction again.
The accepted bundle is preserved byte-for-byte; its creation-time
`contract-review` metadata is superseded for approval status by this decision.
Production integration remains KU-ENC-002 and measured model/resource
qualification remains KU-ENC-003, with existing mobile ownership unchanged.

This message accepts the reviewed task. It does not explicitly instruct a
merge or branch deletion under D-010. Keep the task in Review with owner
acceptance recorded until its merge exists on main; the earlier explicit
merge instruction applied to KU-RUN-001.

## D-020 — KU-ENC-002 accepted and closed

After reviewing implementation `16408c6` and handoff tip `a687600`, the owner
stated: "tôi đẫ review và đồng ý. hãy hoàn thành task này, task kế tiếp tôi sẽ
mở conversation mới để thực hiện ."

This accepts KU-ENC-002 and directs completion of the current task, including
its merge and handoff closure. Fresh focused tests and contract gates passed;
merge `dc04b71b48b27588800b682ef1e71d4506945db1` is on `origin/main`.
The accepted [implementation evidence](outputs/KU_ENC_002_IMPLEMENTATION.md)
retains its explicit source-planner, unit/review-authority and qualification
limits. Acceptance does not qualify a real model/device or enable default rollout.

KU-ENC-003 is the next planned task. The owner will open its new conversation;
do not start qualification, create that branch or dispatch another task during
this closure. No branch deletion was requested.

## D-021 — Prioritize an early open-source concept / MVP

The owner stated that the project needs a concept/MVP soon so contributors can
see it and participate; the initial author should not attempt to perfect the
whole system alone. This is a delivery-priority direction, not acceptance of
unmeasured encoder quality or a change to canonical semantics.

Use a small, runnable local KU journey as the first product milestone. The
existing dependency graph already permits KU-API-001 after KU-RUN-001 and
KU-ENC-002, followed by KU-WEB-001. KU-ENC-003 is a qualification dependency of
KU-QA-001, not an entry dependency of API/Web implementation. Keep qualification
work from unnecessarily blocking that product path. The initial demonstration
can expose supported no-LLM/manual draft work and honest unresolved states;
arbitrary-text AI readiness requires the existing qualification evidence.

Suggested demonstration: create a local draft, preview and validate it, save
explicitly, then search and inspect the saved KU. Provide runnable instructions,
known limits and bounded contribution opportunities. Defer additional product
surfaces and broad capability claims from this first demonstration, without
marking their tasks complete or changing their eventual acceptance criteria.

The owner also reports two newly collected workbooks, each with 100 sources,
and supplied only their column headers. Both have matching machine field names
in `Nguon` and `Nhan_truoc_encode`; explanatory text differs by language. The
new workbook contents, independence, reviewer status and locked hashes have
not been inspected. Keep them separate from the development examples and
annotation exercises already reviewed in this task.

This checkpoint records the priority and available product path. It does not
start another implementation task, close KU-ENC-003, enable a model, merge a
branch or deploy a product.

Owner follow-up: the owner answered "tôi đồng ý" to starting KU-API-001, then
KU-WEB-001. KU-API-001 is authorized now; dependent Web work follows the accepted
API merge. The qualification branch is retained separately; no merge, deployment
or default AI enablement is authorized by this direction.

## D-022 — KU-API-001 accepted and merged; Web handoff next

The owner reviewed implementation `29c34d1` and handoff tip `423b7b8`, then
stated: "tôi đã review và đồng ý. hãy thực hiện. sau đó cập nhật tài liệu hand
off để tôi làm task kế ở conversation mới".

This accepts KU-API-001 and authorizes its merge and handoff closure. Fresh
API tests, generated Base projections, format and global contract checks pass.
Merge `3eba370df1df91627595e0acbf7645d94ea75276` is pushed to `origin/main`.
The accepted implementation retains its documented host-intake and real-model
qualification limits; no default rollout or model qualification is implied.

KU-WEB-001 is next under D-021. Leave it Planned for the owner's new
conversation, using branch `codex/ku-web-001-workflow` from updated main. Do
not begin Web implementation during this closure. Preserve the separate
KU-ENC-003 branch and private holdouts. No branch deletion was requested.

## D-023 — Experimental local Ollama MVP approved

The owner answered “tôi đồng ý bổ sung” to the concrete Ollama amendment at
`4a0a1bf`. KU-WEB-001 now includes text intake, installed-model selection
(initially `qwen3:8b`) and actual shared-workflow inference before full KU-ENC-003
quality qualification. Retain `model_qualified: false`, opt-in host activation,
technical resource/tokenizer/custody/validation/recovery controls and explicit
private save. This supersedes the blanket pre-qualification REST inference ban
only for this experimental host lane. It does not qualify a tuple or approve
mobile, default rollout, publication, merge, branch deletion or holdout use.
No further approval of this same exception is needed.

### D-023 follow-up — semantic guidance, longer experimental deadline and async direction

On 2026-09-07 the owner approved preserving detailed semantic analysis guidance
and examples for multiple related propositions, then requested a higher timeout.
The local experimental host uses a bounded 10-minute aggregate deadline, with
standard token/work/memory controls and unchanged qualification thresholds.
The owner also described the eventual product direction: consented KU encoding
and sharing run asynchronously so the user need not wait on the page. This is a
follow-up design requirement, not evidence that a durable background queue or
publication consent is implemented. Encoding consent and authorized publication
must remain separately enforced. The current local host retains explicit save
and no publication. No new task, merge, default rollout or holdout use is implied.

### D-023 follow-up — autonomous development tests and model portability

On 2026-09-08 the owner requested agent-run development tests for “Nước thường
sôi ở 100oC ở gần mặt nước biển” and “Xe oto cá nhân thường có 4 bánh”, without
repeated Web builds/manual owner testing. The owner prioritizes portable prompts,
instructions and task architecture for personal models of varying speed/ability,
with later AI-assisted verification of approximate proposals. This authorizes
bounded local development experiments and shared workflow fixes, not labeling
approximate/unverified proposals as canonical or Registry-resolved KU. Preserve
source, uncertainty and scope through later verification. The review-proposal
experiment is documented separately and does not silently expand the accepted
Candidate/SEM mapping, model admission, publication or qualification surface.

### D-023 follow-up — staged review drafts approved

On 2026-09-08 the owner approved the proposed implementation of semantic drafts,
host-owned identifiers/anchors/numeric mechanics, checked field-level repairs and
durable background jobs, with agent-run development tests before Web activation.
The additive [review-draft profile](../../specs/vnext/KU_REVIEW_DRAFT_PROFILE_V1.md)
records the scope and lifecycle. Drafts and reviewer suggestions remain private
unaccepted producer output. This approval does not collapse draft readiness into
canonical KU readiness or enable automatic publication/reward authority.

### D-023 follow-up — shared semantic selection and host mechanics

On 2026-09-09 the owner approved reducing model work to semantic selections,
host-owned JSON/defaults/anchors/numeric parsing, targeted repairs and separate
verification, explicitly requiring the common architecture to govern every
platform. The [shared selection contract](../../specs/vnext/KU_SEMANTIC_SELECTION_PROFILE_V1.md)
records the boundary and unassessed draft state. This authorizes the common
implementation and development tests; it does not qualify models, implement
mobile, authorize publication or change canonical KU/SEM semantics.

### D-023 follow-up — KU-SEM-001 private representation v2

On 2026-09-20 the owner approved the
[representation proposal](outputs/KU_SEM_001_REPRESENTATION_PROPOSAL.md) and
implementation: alternatives, ellipsis/reference, comparison and explicit versus
inferred provenance in a separately versioned private draft. Shared Rust semantics,
durable node lifecycle and unassessed status remain mandatory. V1 records remain
readable; unavailable old commitments cannot resume under changed code. This does
not approve canonical changes or bypass inspected headless evidence before host
activation. The active [v2 contract](../../specs/vnext/KU_SEMANTIC_SELECTION_PROFILE_V2.md)
records the bounded initial surface and unresolved cases.

## D-027 — Merge the retained baseline and continue routing

On 2026-09-21 the owner instructed “vây hãy merge rồi làm tiếp” in response
to the task-004 merge prerequisite. This authorizes committing the complete
retained KU/Web/CLI and OBP-001..003 working-tree baseline and merging it into
local main, then continuing OBP-PROD-004 on its task branch in the original
workspace. The previous no-commit/no-merge restriction is superseded for this
baseline. No remote push, live-host/network activation, model tuning or model
qualification is implied. Task 004 remains bounded by its existing contract;
public API work remains a subsequent task.

## D-025 — Accept OBP composition and authorize local lifecycle implementation

On 2026-09-20 the owner accepted the concrete OBP-PROD-001 proposal and replied
“Chấp nhận, triển khai tại local chưa merge” to the explicit contract-review and
task-002 merge-prerequisite question. OBP-PROD-002 may therefore be implemented
and tested on the complete original dirty tree before merge. This supersedes
its `001 merged` prerequisite for this local continuation. No commit, push,
merge, production activation or network opt-in was authorized. Frozen wire
contracts and the deferred discovery/routing/API task boundaries still apply.

## D-026 — Authorize local discovery orchestration before lifecycle merge

On 2026-09-20 the owner answered “Cho phép 003 local, chưa merge” after
reviewing the task-003 `OBP-PROD-002 merged` prerequisite. OBP-PROD-003 may
be implemented and tested on the complete original dirty tree before task 002
is merged. This exception is limited to task 003. Preserve all uncommitted
work, opt-in networking, frozen wire/authority semantics and the live host.
No commit, push, merge, model tuning or live networking activation is authorized.
Automatic routing and public API remain later tasks with their own prerequisites.

## D-024 — Accept the semantic foundation and advance product framework work

On 2026-09-20 the owner stated that this part is sufficient for now, prioritized
quickly building the framework for other functions, and asked for a new-conversation
handoff. Remaining defects/upgrades will be shared with open-source contributors.
This accepts the implemented KU-SEM-001 foundation for this development stage and
defers further fidelity tuning; it does not assert the failed cases are fixed.

Advance the continuation pointer to KU-CLI-001, the next dependency-ready planned
KU surface. Follow the [new-conversation handoff](NEXT_CONVERSATION.md); retain
[semantic gaps](outputs/KU_SEM_001_CONTRIBUTOR_BACKLOG.md) for later contributors.
Do not repeatedly request approval to move on or make semantic perfection a gate
for unrelated framework work. Preserve the complete original dirty working tree;
the older clean-main start rule does not apply to this continuation baseline.
No merge, push, publication, host activation, qualification or canonical change
is implied. KU-SEM-001 remains an accepted local implementation with deferred
quality work, not a merged task or a qualified model.


## D-028 — Merge routing and publish the retained baseline

On 2026-09-21 the owner explicitly instructed “Merge 004 và push tất cả luôn.”
This authorizes committing all retained task-004 changes in the original
workspace, merging OBP-PROD-004 into local main, and pushing main and the routing
branch to origin, including the complete D-027 baseline through task 003.
The original baseline at 3216f1d must remain an ancestor. This supersedes the
previous no-push restriction for this publication. It does not authorize model
tuning, live-host changes, networking activation or default rollout. OBP-API-001
becomes dependency-ready after the actual task-004 merge; no API implementation
is part of this merge/publication action.


## D-029 — Accept OBP local API transport

On 2026-09-21 the owner replied “chấp nhận đề xuất” to the concrete
OBP_LOCAL_API_PROFILE_V1 proposal and its REST/WS transport review request.
This accepts the six routes, unchanged 13 operation mappings, host capabilities,
input-reference boundary, durable command/reconciliation contract and separately
scoped aggregate WS projection. Implement OBP-API-001 on its existing local branch
and original retained workspace. No further approval of this transport is needed.
The accepted scope does not enable networking, change the live host or models,
authorize default rollout, mobile implementation or merge this task.


## D-030 — Merge and publish OBP-API-001

On 2026-09-22, after continuation verification at `c4aafb9`, the owner instructed
"merge và push tất cả". This authorizes merging the complete reviewed API branch
and publishing main plus all retained task changes. Implementation `de04a57`
merged without conflicts as `7d37a3066a561a38867352b3fab2e08387940ba6`.
The API branch is retained; no branch deletion was requested.

OBP-API-001 is Merged. Advance the handoff to OBP-CLI-001, Planned; CLI/Web API
merge prerequisites are satisfied. This closure does not start either task,
activate networking, change the live host/models, implement mobile or enable rollout.

## D-031 — Accept and close OBP-CLI-001

On 2026-09-22, after the local CLI implementation and evidence were presented,
the owner stated "tôi duyệt tất cả. hãy tiếp tục". This accepts the complete
reviewed OBP-CLI-001 work and directs its closure through commit, merge and
publication under the workstream policy. Preserve the original workspace and
retained branches. Record the actual merge in PROGRESS after it exists.

The accepted CLI scope is the 13 existing API operations plus metadata reconcile,
with host-issued input references/capabilities and exact confirmation/recovery
boundaries. No new raw intake, protocol operation, live-host activation, model
tuning, networking opt-in, mobile implementation or default rollout is included.
The next handoff pointer is OBP-WEB-001, Planned; this closure implements no Web UI.
