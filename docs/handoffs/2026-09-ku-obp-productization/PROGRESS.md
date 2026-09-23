# Workstream progress

> This is the authoritative execution ledger for this folder.
> Allowed states: `Planned`, `In progress`, `Review`, `Merged`, `Blocked`, `Deferred`.

## Current checkpoint

- 2026-09-24 OBP-MIG-001 review publication: validated implementation
  `1d649ce` was committed and published on
  `origin/codex/obp-mig-001-retire-legacy-seed`. The retained branch is
  `Review`; no merge, default rollout, live host action or legacy data
  removal occurred. D-010 keeps merge as a separate owner direction.

- 2026-09-24 OBP-MIG-001 — `Review`: final local verification passed after
  the [migration map](outputs/OBP_MIG_001_IMPLEMENTATION.md) was completed.
  CLI feature-on 52+2 and base-only 43+2, plus one real-process default-start
  test in each mode; seed integration 1, node runtime 8,
  node library 253 (two threads), Desktop lifecycle 6 and no-default vNext
  CLI check passed. The aggregate vNext validator, focused Rust format,
  whitespace and link checks passed. A first parallel node-library run had
  one unrelated PoMV timeout; focused and bounded two-thread reruns passed.
  No default OBP opt-in, live host or durable data was modified. All
  changes remain local and uncommitted on the declared migration branch;
  owner review and D-010 Git closure are separate steps.

- 2026-09-24 OBP-MIG-001 — `In progress` on
  `codex/obp-mig-001-retire-legacy-seed` from published main `940fa61`.
  [Migration inventory, rollback map and tests](outputs/OBP_MIG_001_IMPLEMENTATION.md):
  legacy seed/mDNS/UPnP startup and REPL connect now require explicit
  `--legacy-seed-compat`; vNext opt-in uses node-owned vNext-only startup;
  standalone `onebrain-seed` refuses to bind without the flag. Old config,
  peer memory, daemon package and legacy listener compatibility remain. CLI
  feature-on 52+2, base-only 43+2, seed integration and node runtime 8 pass.
  Node library passes 253 with two threads after one unrelated PoMV timeout
  in a parallel run. No live host, default OBP activation or remote state
  changed. Final contract/format/packaging checks remain before Review.

- 2026-09-24 main publication: D-041 QA merge `d695e4a` and handoff closure
  commit `940fa619925c6b1903525141d2244eb6d4e19009` were published to
  `origin/main`; the retained QA branch remains published at `fb5b71b`.
  Task 18 is `Merged`; its consumer-NAT gate was skipped and remains
  unqualified. Task 19 is dependency-ready.

- 2026-09-24 D-041 Git closure: validated QA tip `fb5b71b` was published on
  `origin/codex/obp-qa-001-nat-canary` and merged locally without conflicts as
  `d695e4ae976c53fceffdc103580c9462997a5389`. Task 18 is `Merged`
  under D-039/D-040 functional acceptance, with D-041's consumer-NAT gate
  explicitly skipped (`consumer_nat_qualified=false`). The separate Linux P5
  qualification on `c453f3e` is unchanged. Main publication is recorded after
  it completes; task 19 becomes dependency-ready but has not started at this
  checkpoint.

- 2026-09-24 D-041: owner directed task 18 closure and progression to task 19,
  skipping consumer-NAT qualification because the existing VPS cannot prove
  ordinary consumer NAT or run the supported product host. D-039/D-040
  functional review is accepted; `consumer_nat_qualified=false` and all ten
  independent-network product scenarios remain unrun. This is an explicit
  skipped gate, not qualification. QA branch publication and merge are
  directed under D-010 but have not yet been recorded at this checkpoint.
  Linux P5 qualification on `c453f3e` remains valid and separate.

- 2026-09-24 D-040: owner accepted the OBP-QA-001 functional review under
  D-039. Task 18 remains `Review` with the tested local scope and unclaimed
  consumer-NAT/native-platform qualification stated in
  [functional acceptance v2](outputs/OBP_QA_001_FUNCTIONAL_ACCEPTANCE_V2.md).
  The QA branch and its local changes remain intact; no commit, merge, push,
  remote operation, default networking change, tuning or task 19 start.

- 2026-09-24 continuation: re-ran the renamed bidirectional two-relay node
  test and aggregate vNext validator; both passed. Updated the acceptance-v1
  test pointer and clarified historical `Blocked` text in task 18. Functional
  Review under D-039 and its qualification limits are unchanged. All edits
  remain local on `codex/obp-qa-001-nat-canary`; no remote action, Git
  publication, default networking change, tuning or task 19 start.

- 2026-09-23 16:09 UTC D-039 [functional acceptance v2](outputs/OBP_QA_001_FUNCTIONAL_ACCEPTANCE_V2.md):
  owner accepts correct code and functional tests without strict external
  evidence as the OBP-QA-001 review criterion. Extended the live two-relay
  integration test to verify a separate B→A domain intent, authenticated
  relay path, durable receiver state and sender checkpoint after the existing
  A→B failover journey. Fresh runs passed: node 253, Desktop 6, API 44/one
  ignored, CLI 51 + 2, Web 89 and vNext contracts. Task 18 advances from
  `Blocked` to `Review` for functional QA. The consumer-network scenarios
  remain unrun and carry no NAT/platform qualification claim. The separate
  Linux P5 exact-candidate qualification remains valid. No remote state,
  networking default, tuning or task 19 changed; work remains local on the
  retained QA branch.

- 2026-09-23 15:44 UTC [two-VPS product admission](outputs/OBP_QA_001_VPS_PRODUCT_ADMISSION_20260923.md):
  owner selected two of the approved VPS as application-node roles and asserted
  their networks are independent. Read-only pinned-SSH checks on a/c and relay
  host b passed. The P5 bundle has no product host, and Linux custom Desktop
  network startup is deliberately fenced by the missing native lifecycle
  adapter. No OBP-QA-V1-01..10 product scenario was executed or relabeled from
  P5. VPS topology does not prove ordinary consumer NAT. Task 18 remains
  `Blocked`; Linux P5 qualification remains valid and task 19 unstarted.

- 2026-09-23 15:26 UTC [Linux P5 production-reference qualification](outputs/OBP_QA_001_P5_PRODUCTION_20260923.md):
  exact `c453f3e` bundle installed on a/b/c; fresh signed Base/Registry/P5 V2
  authority, sequence-7 relay history and four cross-host probes verified.
  Signed relay-only ring, selected-relay shutdown/alternate recovery and all
  13 faults with before/during/after markers completed. Independent verifier
  reports `multi_host_qualified=true` with 332 signed child receipts and 387
  raw objects; public aggregate privacy scan is clean. Signed cleanup and
  finalization passed, and direct three-host checks found no active P5 session
  or TCP 443 listener. Old runs, staging and archived fixtures/cursors remain.
  Task 18 stays `Blocked` on separate two-consumer-network/two-relay product
  scenarios and native surface/lifecycle evidence; task 19 is unstarted.

- 2026-09-23 14:34 UTC [signed V2 happy case completed](outputs/OBP_QA_001_SIGNED_HAPPY_SUCCESS_20260923.md):
  immutable `5b8dc3b` installed on a/b/c; fresh Base/Registry/P5 authority
  verified, sequence-4 history and four cross-host probes passed. Signed
  reservations, relay-only A→B→C→A ring and three marker exchanges completed,
  followed by cleanup/finalization. Direct a/b/c checks found no session or
  network state, active P5 agents/signers or TCP 443 listener; normal relays
  remain fenced. Earlier generations, failures and cursor archives remain.
  P5 13-fault/ten-oracle qualification and the two independent consumer-network
  product lane remain outstanding. Task 18 Blocked, task 19 unstarted.

- 2026-09-23 14:05 UTC [signed V2 happy-case attempt](outputs/OBP_QA_001_SIGNED_HAPPY_ATTEMPT_20260923.md):
  immutable `85adf65` installed on a/b/c, exact Base/Registry/bundle/P5
  authority verified, four sequence-3 cross-host probes passed. Signed
  bootstrap/prepare/reachability/matrix passed, but reservations failed closed
  at host-b. Signed cleanup and finalization succeeded on all three hosts;
  relay listeners are stopped and the prior cursor files preserved in typed
  maintenance archives. A durable relay-sequence and stable node-cursor
  correction is in progress locally; no full happy case, fault qualification,
  independent consumer-network evidence or OBP-MIG-001 start exists.

- 2026-09-23 13:11 UTC [installed candidate and successor blocker](outputs/OBP_QA_001_INSTALLED_SUCCESSOR_BLOCKER.md):
  all three immutable installations/public exports match `2dc5581`; identities
  preserved. D-036 placement wrappers retained with historical signature binding.
  Host-b's future timesyncd timestamp corrected; bounded skew now near zero, NTP
  still unsynchronized. Both relays durably renewed to sequence 2. Four actual
  restricted-SSH probes fail `SequenceRollback` because cold admission has no
  predecessor-history input. Relay-b/c and candidate-only listeners are stopped;
  new units/configs are safely fenced by `NotActivated`, state/backups retained.
  47 evidence file commitments and identity preservation verified. Versioned
  history-admission correction proposed for review; no new runtime edit/commit,
  complete V2 request, signed session/fault or qualification. Task 18 Blocked,
  task 19 unstarted. Earlier no-installation/clock-unmodified entries are historical.

- 2026-09-23 D-036: owner explicitly reaffirmed the already supplied/accepted
  placement of the three VPS on three separate physical machines. New placement
  collection is no longer a prerequisite; reuse retained evidence with original
  dates/provenance and provider-document-pending status. Earlier external topology
  blocker entries are superseded. Remaining work: clock maintenance, installed
  candidate/public exports, fresh relay descriptors/probes and complete V2
  admission/execution. Task 18 is still incomplete; task 19 remains unstarted.

- 2026-09-23 08:39 UTC [admission continuation](outputs/OBP_QA_001_ADMISSION_RECHECK.md):
  new read-only SSH collection confirms host-b remains about 25,388 seconds ahead,
  NTP off and relay running. Host-a search remains permission-limited; b/c still
  expose August placement receipts. All three collected file hashes verify.
  Added canonical V2 input completion order; current physical placement evidence
  remains an external dependency. No host mutation or signed session/fault.
  Task 18 remains Blocked; task 19 unstarted. Existing approvals remain accepted.

- 2026-09-23 08:35 UTC [retained authority recheck](outputs/OBP_QA_001_AUTHORITY_RECHECK.md):
  all five local checks pass for clean candidate `2dc5581`, signed Base request,
  rehashed Registry binding, canonical native bundle and approved policy identity/
  interval. Existing WSL dependency environment used after a retained system-Python
  import failure. No SSH, signing, host mutation or session/fault operation.
  Current topology, host-b clock and complete V2 admission remain unresolved;
  task 18 remains Blocked and task 19 unstarted. This is not qualification.

- 2026-09-23 08:30 UTC admission recheck: [fresh read-only SSH diagnostics](outputs/OBP_QA_001_ADMISSION_RECHECK.md)
  confirm host-b remains ~25,388 seconds ahead. Timesyncd is installed but
  disabled; VMware time sync is disabled. Relay remains running; P5 agent/signers
  are inactive. Host-a/c report NTP synchronized. Bounded host search found no
  new provider receipt; host-a search is partial/permission-limited. Initial SSH
  timeouts and successful bounded retries are retained separately. No host clock,
  service, runtime, signed session or fault mutation. Task 18 remains Blocked.

- 2026-09-23 D-035 execution: candidate `2dc5581e74387953b21051c930e52a8044ce0503`
  (tree `f08092d800615181da4ccc50db997606edb6f03b`) committed locally on the
  retained QA branch. [Rebuild/staging evidence](outputs/OBP_QA_001_CANDIDATE_2DC5581.md):
  fresh Base/Registry bindings verified; rebuilt bundle passed canonical checks
  and all three remote staging verifiers; six native loopback preflights passed.
  Fresh guest evidence does not prove physical independence. Found remote b/c
  placement receipts are still August 13 telephone attestations. New measured
  blocker: host-b clock ahead ~25,388 seconds, NTP off. No installed service,
  identity/state, clock, signed session or fault changed. Task 18 remains Blocked;
  next work is admissible topology/clock prerequisites and complete fresh V2
  admission. User requested autonomous investigation; no repeat approval request.

- 2026-09-23 D-035: owner approved the reviewed relay renewal correction and
  creation of a new local immutable QA candidate, followed by rebuild and fresh
  exact-candidate authority preparation. The reviewed source hashes still match
  both final verification reports. Keep the retained QA branch and all old runs;
  no push/merge or task 19. Candidate/build outcomes follow only after execution.

- 2026-09-23 QA continuation: [relay renewal correction](outputs/OBP_QA_001_RELAY_RENEWAL_FIX.md)
  is locally implemented for review on the retained QA branch. Canonical same-key
  successor, atomic candidate/activation fencing, exact-byte publication recovery
  and persisted-descriptor startup are covered. Windows and local Linux: 40 relay
  tests each; node integration/matrix 32; Python 86 passed/one skipped; vNext,
  format and whitespace pass. All source/log commitments verify. No commit,
  push, merge, remote replacement or signed P5 session. Next: review/new immutable
  candidate, then fresh exact-candidate V2 authority and independent-network
  evidence. OBP-QA-001 remains `Blocked`; OBP-MIG-001 is unstarted.

- 2026-09-23 approved P5 follow-up: owner accepted all proposals; same-key policy
  renewal applied. Fresh `fc65f08` Base request and remeasured Registry binding
  signed/verified; operational bundle 02 verified on all three hosts. The current
  blocker is [relay descriptor renewal](outputs/OBP_QA_001_P5_RENEWAL_BLOCKER.md):
  existing descriptors expired, while exact-candidate CLI only supports first
  export and cannot preserve the descriptor chain on renewal. Binary repro fails
  closed; no state reset/service replacement or production fault performed.
  OBP-QA-001 remains `Blocked`. Earlier approval-pending entries are historical.

- 2026-09-23 P5 follow-up: owner authorized reuse of the existing three hosts,
  SSH upload and remote execution. [Upload/execution evidence](outputs/OBP_QA_001_P5_UPLOAD.md):
  exact `fc65f08` source-free bundle built and canonical manifest verified;
  pinned SSH, upload and artifact checks passed on all three hosts. Both native
  single-host preflights passed on each host (six runs), qualification false.
  Installed candidates differ; no service/generation/identity was replaced.
  [V2 authority recovered](outputs/OBP_QA_001_RECOVERED_P5_AUTHORITY.md) from the
  sibling release archive; old signatures/keys verify. The policy expired on
  September 17; a same-key September 23–30 renewal proposal awaits approval.
  OBP-QA-001 remains `Blocked`; no production fault or Git publication occurred.

- 2026-09-23 OBP-QA-001 — `Blocked` on
  `codex/obp-qa-001-nat-canary`, original clean main `fc65f08` verified equal
  to remote main. All retained branches preserved. Versioned
  [acceptance scenarios](outputs/OBP_QA_001_ACCEPTANCE_V1.md) and fixed local
  preflight collector added. [Local evidence](outputs/OBP_QA_001_PREFLIGHT.md):
  all 13 groups pass, with one Linux-only Python skip and one ignored model test;
  22 supplemental P5 tests and report integrity verification pass. Independent
  consumer networks, relay-host faults and exact-candidate P5 evidence are not
  supplied or executed. No qualification, live-host activation or Git publication.
  The D-034 Planned pointer below is historical.

- D-034 closure: accepted Desktop implementation `410a6a0` merged without
  conflicts as `7e4fc14e9ba4082e425d474ef9f7da26129106b0`. Fresh checks passed:
  six Desktop integration tests, 89 Web tests, 35 Python tests and vNext validator.
  Main and the retained Desktop branch are published in this closure.
  Current task: OBP-QA-001, Planned. No QA execution or live-host activation.
  Earlier local/uncommitted Desktop Review entries below are historical.

- D-033: owner accepted OBP-DESK-001 review (“tôi đồng ý duyệt review”).
  Implementation and evidence are unchanged. State remains Review with owner
  acceptance recorded, pending explicit Git closure direction. Original working
  tree and all local changes retained; no commit/push/merge or next task started.

- 2026-09-22 OBP-DESK-001 — `Review`, local implementation on
  `codex/obp-desk-001-networking` from clean main `636c113` in the original tree.
  One shared node/API supervisor, local credential handoff, Windows lifecycle
  fencing, scoped tray status and durable client recovery are implemented.
  [Evidence and limits](outputs/OBP_DESK_001_IMPLEMENTATION.md): six feature-enabled
  and four feature-off integration tests, 89 Web tests, two receipt tests,
  Web build/lint, 35 Python tests and vNext validator pass. Windows Tauri debug
  asset build passes. API/node/CLI and live host remain unchanged. Work remains
  uncommitted; no push/merge, networking activation or platform qualification.
  Earlier Desktop Planned/main pointers below are historical.

- D-032 closure: owner accepted Web review. Implementation `cade635` merged
  without conflicts as `7f49eeb`. Fresh checks passed: 82 Web tests, production
  build, 25 Python tests and aggregate vNext validator. API/node/CLI unchanged.
  Main and retained Web branch are published in this closure. Current task:
  OBP-DESK-001, Planned. No Desktop implementation or live-host activation.
  Earlier local/uncommitted Web entries below are historical.

- 2026-09-22 OBP-WEB-001 — `Review`, local implementation on
  `codex/obp-web-001-networking` from clean main `2d4d5f4` in the original tree.
  All 13 accepted operations plus metadata reconcile use the shared local API;
  private aggregate WS hints only trigger reads. Exact interactions are in the
  [Web projection](../../specs/vnext/OBP_LOCAL_WEB_PROJECTION_V1.md).
  [Evidence](outputs/OBP_WEB_001_IMPLEMENTATION.md): 82 Web tests, two receipt
  tests, production build, lint (eight existing unrelated warnings), 25 Python
  tests and vNext validator pass. Isolated Chromium at 1280/390px passes layout
  and axe including contrast. API/node/CLI and the live host remain unchanged.
  Work is local/uncommitted; no push, merge, networking activation or model work.
  The earlier Web Planned pointers are historical; next action is Web review.

- 2026-09-22 D-031: owner approved the complete CLI implementation and requested
  continuation. Implementation `e0dcc7d` merged without conflicts as
  `04bcb30e42fa2594d0bf11e146fa0e44df0dcf82`. Fresh checks passed: CLI 51 unit +
  2 integration, Python 25 and aggregate vNext validation. API/node content
  remains unchanged. Main and the retained CLI branch are published in this
  closure. Current pointer: OBP-WEB-001, Planned; no Web implementation started.
  Earlier local/uncommitted CLI Review entries below are historical.

- 2026-09-22 OBP-CLI-001 — `Review`, local implementation on
  `codex/obp-cli-001-networking` from clean `b66780f` in the original workspace.
  All 13 accepted operations and metadata reconcile use authenticated local REST;
  exact syntax, confirmations and host-input limits are registered in the
  [CLI projection](../../specs/vnext/OBP_LOCAL_CLI_PROJECTION_V1.md).
  [Implementation evidence](outputs/OBP_CLI_001_IMPLEMENTATION.md): feature CLI
  51 unit + 2 integration, default CLI 42 + 2, no-default check, 25 Python contract
  tests, vNext validation and format/whitespace checks pass. API/node code remains
  unchanged. All work is retained locally, uncommitted; no push/merge, live-host,
  networking activation, model, mobile or rollout changes.

- 2026-09-22 D-030: owner instructed "merge và push tất cả". Reviewed API tip
  `c4aafb9` (implementation `de04a57`) merged without conflicts as
  `7d37a3066a561a38867352b3fab2e08387940ba6` and published on `origin/main`.
  All retained baseline work remains in ancestry. Verification immediately before
  merge passed: node 252 + 22, API 44 + 8 (one opt-in ignored), feature-off 16,
  ku-net 315, Python 19 and vNext validator. Implementation was unchanged.
  Current pointer advances to OBP-CLI-001, Planned; OBP-WEB-001 is also dependency-ready.
  No next-task implementation, host activation, model tuning or rollout occurred.
  Earlier Review entries below are historical checkpoints superseded by D-030.

- 2026-09-22 OBP-API-001 continuation verification at `bcca300`: original tree
  initially clean; remote API/main tips verified directly. Fresh offline/locked
  reruns passed: node 252 unit + 22 integration; API 44 unit/one ignored + eight
  Base integration; feature-off 16; ku-net 315; Python 19 and vNext validator.
  [Updated evidence](outputs/OBP_API_001_IMPLEMENTATION.md#continuation-verification--2026-09-22).
  Corrected the stale next-action pointer. Implementation remains unchanged in
  Review; API merge requires owner direction, CLI/Web remain Planned.

- 2026-09-22 OBP-API-001 — `Review`, implementation under accepted D-029 transport.
  Shared node façade, durable command/reconciliation journal, host grants/input
  references and six REST/WS routes are implemented in the original workspace.
  [Implementation evidence and limits](outputs/OBP_API_001_IMPLEMENTATION.md).
  Verification passed: node 252 unit + 22 integration; API 44 unit/one ignored
  + eight Base integration; API feature-off 16; ku-net 315; Python 19 and vNext
  validator. Implementation `de04a57` is published on
  `origin/codex/obp-api-001-network-api` under D-010. No merge is authorized by
  D-029; dependent tasks remain Planned.
  The transport-review entry below is historical; approval is already satisfied.

- 2026-09-21 OBP-API-001 — `Review` of the transport prerequisite only.
  [Concrete proposal and evidence](outputs/OBP_API_001_TRANSPORT_REVIEW.md):
  six routes, 13 operations, management/input/reconciliation boundaries and a
  separately scoped private WS projection. 19 contract tests and aggregate
  vNext validation passed; 35 transport fixtures. No handlers implemented.
  Original tree retained on `codex/obp-api-001-network-api`, baseline `836af91`;
  these changes are local and uncommitted. Notification transport review remains
  required by the accepted OBP profile before this complete extension is implemented.

- 2026-09-21 D-028 — `OBP-PROD-004` Merged and pushed to `origin/main`.
  Implementation `8f7d327`; merge `2c39117183f167ef4d333757b050d6306043e068`.
  Atomic push published main and `codex/obp-prod-004-routing`, including the
  complete retained D-027 baseline at `3216f1d`. Original workspace preserved.
  Fresh verification passed: 242 node unit + 22 integration, 315 ku-net,
  six OBP contract tests, aggregate vNext validator and whitespace checks.
  [Evidence and limitations](outputs/OBP_PROD_004_IMPLEMENTATION.md).
  OBP-API-001 is now dependency-ready and remains Planned. No model tuning,
  live-host changes, networking activation or default rollout occurred.

- 2026-09-21 D-027: owner authorized merge and continuation. Complete retained
  baseline committed as `437dba0`, merged into local main as
  `3216f1ddf0cd72098924ccdf9fb0ef6b30186b78`. No remote push. Original workspace
  retained; task 004 now In progress on `codex/obp-prod-004-routing`.
  Fresh verification: node 234 unit + 22 integration; CLI 42 unit + 2 integration;
  encoder extraction 70 passed/1 opt-in ignored; KU API 21 passed/1 opt-in ignored;
  Web 22 tests and build; six OBP contract tests, aggregate validator and diff check.
  Task 003 merge prerequisite is satisfied. Historical unmerged checkpoints below
  describe their original observation dates, not the current Git state.

- 2026-09-21 OBP-PROD-003 â€” `Review`: discovery, signed cache recovery,
  standing reservations/renewal and separately opted-in advertisements are
  implemented locally under D-026. 234 node unit + 22 integration tests pass,
  including real loopback TLS renewal/keepalive/closure. Core tests, feature
  checks and validators pass. [Evidence and limits](outputs/OBP_PROD_003_IMPLEMENTATION.md).
  Original dirty tree retained on `codex/obp-prod-003-discovery`; no merge,
  publication, model tuning or live networking activation. Task 004's merge
  prerequisite is not waived by D-026.

- 2026-09-20 D-026: owner explicitly authorized OBP-PROD-003 local work before
  task 002 merge. Implementation is in progress on `codex/obp-prod-003-discovery`
  in the original dirty tree. Fresh task-002 verification: 219 unit + 20
  integration tests, six contract tests and aggregate vNext validation pass.
  No Git publication, model tuning or live network activation is authorized.

- 2026-09-20 D-025: owner accepted OBP-PROD-001 and explicitly authorized
  OBP-PROD-002 local implementation before merge. Work remains on the original
  dirty tree under `codex/obp-prod-002-node-lifecycle`; no Git publication or
  live-host activation. Task 002 is locally implemented/tested: 219 unit and
  20 integration tests passed, feature checks and contract validators passed.
  [Lifecycle evidence](outputs/OBP_PROD_002_IMPLEMENTATION.md).

- 2026-09-20 OBP-PROD-001 â€” `Review`: [orchestration proposal](outputs/OBP_PROD_001_CONTRACT.md)
  defines 13 logical operations, 18 DTOs and 26 fixtures. Six mutation tests and
  aggregate vNext validation pass. Original dirty tree, including completed CLI,
  retained on `codex/obp-prod-001-product-contract`; no commits/push/merge.
  Historical review gate superseded by D-025: contract accepted, local task 002 authorized before merge.

- 2026-09-20 KU-CLI-001 â€” `Review`: all eleven shared KU operations plus
  reservation implemented in `onebrain ku`. 42 CLI unit and 2 integration tests
  pass, including the isolated real API/node workflow; default/no-default checks,
  format and vNext contracts pass. [Evidence](outputs/KU_CLI_001_IMPLEMENTATION.md).
  Original dirty baseline retained on `codex/ku-cli-001-workflow`; uncommitted,
  no model tuning, live-host change, push or merge. Next lane: OBP-PROD-001 contract.

- 2026-09-20 D-024: owner accepts the current semantic foundation for this stage.
  Further fidelity work is Deferred to the [contributor backlog](outputs/KU_SEM_001_CONTRIBUTOR_BACKLOG.md).
  Next: KU-CLI-001, then the dependency-ready OBP framework lane. No merge or v2
  host activation is claimed. [New-conversation handoff](NEXT_CONVERSATION.md).

- 2026-09-20 shared semantic selection continuation â€” `Review`: common Rust host assembly, sparse LLM choices and mechanically constrained repair are implemented and activated locally. Qwen3 8b passed 5/5 through the actual Web API (52.32 s mean; admission <0.1 s); Gemma E4B passed 4/5 (43.19 s), 12B passed 5/5 (78.58 s) via the development adapter. Fifteen selection tests, five API tests, twenty Web tests, build and contract validation passed. Three legacy jobs remain readable; new drafts are explicitly unassessed and no KU save/share occurred. Ollama 0.34.2; earlier baseline 0.33.3. See [selection implementation and limits](outputs/KU_SEMANTIC_SELECTION_IMPLEMENTATION.md).

- 2026-09-09 shared semantic selection â€” historical checkpoint (superseded by September 20 `Review`): owner approved sparse LLM choices and host mechanics consistently across platforms. Common architecture, Rust assembler/targeted repair and unassessed draft state are implemented; 45 encoder, five API and 20 Web tests passed. Real-model comparisons and local host activation are pending. See [selection implementation](outputs/KU_SEMANTIC_SELECTION_IMPLEMENTATION.md).
- 2026-09-09 owner-requested Gemma 4 comparison â€” `Review`: development-only chat transport ran the same native DraftJob/prompt/schema flow on five public cases per model. E4B passed 3/5 meanings (66.57 s mean); 12B passed 5/5 (179.78 s mean). E4B missed a preposition and linked the wrong contrast endpoints despite mechanical readiness. All 20 calls had thinking disabled. No Web provider admission or canonical KU behavior changed; read-only live-host check passed. See [Gemma evidence and limits](outputs/KU_GEMMA4_DEVELOPMENT_PROBE.md).
- 2026-09-08 staged-draft follow-up â€” `Review`: shared draft workflow, encrypted background jobs and Web interface are implemented and activated on the local owner host. Qwen3 8b preserved all five inspected development meanings; 1.7b retained review issues on all five. Actual Web API originals passed (water 170.95 s, car 126.06 s; start replies 0.085/0.071 s), without resampling on reads/repeated start or any KU save/share. See [implementation and limitations](outputs/KU_REVIEW_DRAFT_IMPLEMENTATION.md). Complete canonical lowering and broad model portability remain unfinished; draft readiness does not claim those outcomes.
- 2026-09-08 owner follow-up: [headless tests and portable proposal experiments](outputs/KU_ENCODER_MODEL_PORTABILITY_EXPERIMENT.md); two requested sources tested locally with Qwen3 8b/1.7b and a separate 3b reviewer. Semantic defects remain explicitly reported; prototype proposals are not canonical KU or integrated Web behavior. Shared preflight now brings span/number/structural compilation errors into the bounded repair allowance.
- Local follow-up: [schema diagnostics and repair feedback](outputs/KU_WEB_001_SCHEMA_DIAGNOSTICS.md) implemented and tested after owner authorization; these follow-up edits remain uncommitted.
- Real Vietnamese follow-up remains unsuccessful on the current Web/host Candidate path: the two exact owner sources failed `duplicate_id` (379.26s, water) and `oneof` (451.29s, car) in the 2026-09-08 rebuilt-host API tests. Both test reservations were canceled; neither yielded a validated preview or saved/shared KU. Number/span preflight now reaches the existing repair allowance; earlier `unsupported_number` evidence remains historical. See the [model-portability report](outputs/KU_ENCODER_MODEL_PORTABILITY_EXPERIMENT.md) for separate proposal results and remaining semantic representation gaps.
- Current task: `OBP-QA-001` — `Blocked` (local preflight passed; independent-network evidence missing).
- Current branch: `codex/obp-qa-001-nat-canary` in the original workspace; prior branches retained.
- Completed review branch: `codex/obp-prod-004-routing`, implementation `8f7d327`.
- Retained baseline: `437dba0`, merged at `3216f1d` under D-027; now published.
- Last merge: `7e4fc14` (OBP-DESK-001, D-034).
- Next action: resolve current topology/clock admission prerequisites, then complete
  candidate-bound host/relay evidence and V2 authority for `2dc5581`; collect the
  separate independent-consumer-network evidence. Review/commit/rebuild is complete.
  Keep task 18 current, networking default-off and task 19 unstarted.
- Encoder framework direction and sequence: owner accepted at `e513552` under D-018; no further direction approval needed
- Default rollout change authorized: **no**
- Mobile work authorized by this package: **no**

## Task ledger

| Order | Task | State | Branch | Dependency | Merge commit/evidence |
|---:|---|---|---|---|---|
| 1 | `KU-REV-001` | Merged | `codex/ku-rev-001-canonical-audit` | â€” | `25d008d211f450d15ba1a63cacc0368298ed3e7a` on `origin/main`; [authority audit](outputs/KU_AUTHORITY_AUDIT.md), D-011â€“D-014. |
| 2 | `KU-REV-002` | Merged | `codex/ku-rev-002-runtime-map` | `KU-REV-001` | [Runtime gap map](outputs/KU_RUNTIME_GAP_MAP.md), including owner D-011â€“D-014; merge `b872263` on `origin/main`. |
| 3 | `KU-CON-001` | Merged | `codex/ku-con-001-product-contract` | `KU-REV-002` | [Approved contract](../../specs/vnext/KU_PRODUCT_WORKFLOW_PROFILE_V1.md); KU-PC-A/B/C accepted under D-015; merge `2cbc8f2` on `origin/main`. |
| 4 | `KU-RUN-001` | Merged | `codex/ku-run-001-shared-service` | `KU-CON-001` | Owner-authorized merge `d141701` on `origin/main`; [implementation evidence](outputs/KU_RUN_001_IMPLEMENTATION.md). |
| 5 | `KU-API-001` | Merged | `codex/ku-api-001-local-api` | `KU-RUN-001`, `KU-ENC-002` | D-022; merge `3eba370` on `origin/main`; [API implementation and verification](outputs/KU_API_001_IMPLEMENTATION.md). |
| 6 | `KU-CLI-001` | Merged | `codex/ku-cli-001-workflow` | `KU-API-001` | [Local implementation and tests](outputs/KU_CLI_001_IMPLEMENTATION.md); local merge `3216f1d`, D-027. |
| 7 | `KU-WEB-001` | Merged | `codex/ku-web-001-workflow` | `KU-API-001` | [Manual implementation](outputs/KU_WEB_001_IMPLEMENTATION.md), [Ollama integration and run instructions](outputs/KU_WEB_001_OLLAMA_IMPLEMENTATION.md); D-023; local merge `3216f1d`, D-027. |
| 8 | `KU-DESK-001` | Planned | `codex/ku-desk-001-workflow` | `KU-WEB-001` | â€” |
| 9 | `KU-QA-001` | Planned | `codex/ku-qa-001-cross-surface` | `KU-CLI-001`, `KU-DESK-001`, `KU-ENC-003` | â€” |
| 10 | `OBP-PROD-001` | Merged | `codex/obp-prod-001-product-contract` | `KU-CON-001` | [Contract proposal and tests](outputs/OBP_PROD_001_CONTRACT.md); accepted under D-025; local merge `3216f1d`, D-027. |
| 11 | `OBP-PROD-002` | Merged | `codex/obp-prod-002-node-lifecycle` | `OBP-PROD-001` accepted, D-025 local waiver | [Lifecycle evidence](outputs/OBP_PROD_002_IMPLEMENTATION.md); 219 unit + 20 integration tests; no activation; local merge `3216f1d`, D-027. |
| 12 | `OBP-PROD-003` | Merged | `codex/obp-prod-003-discovery` | `OBP-PROD-002` local evidence, D-026 | [Discovery evidence](outputs/OBP_PROD_003_IMPLEMENTATION.md); 234 node unit + 22 integration tests; local merge `3216f1d`, D-027. |
| 13 | `OBP-PROD-004` | Merged | `codex/obp-prod-004-routing` | `OBP-PROD-003` | [Routing evidence](outputs/OBP_PROD_004_IMPLEMENTATION.md); merge `2c39117` on `origin/main`, D-028. |
| 14 | `OBP-API-001` | Merged | `codex/obp-api-001-network-api` | `OBP-PROD-004` merged `2c39117` | [Implementation evidence](outputs/OBP_API_001_IMPLEMENTATION.md); merge `7d37a30` on `origin/main`, D-030. |
| 15 | `OBP-CLI-001` | Merged | `codex/obp-cli-001-networking` | `OBP-API-001` | [Evidence](outputs/OBP_CLI_001_IMPLEMENTATION.md); implementation `e0dcc7d`, merge `04bcb30`, D-031. |
| 16 | `OBP-WEB-001` | Merged | `codex/obp-web-001-networking` | `OBP-API-001` | [Evidence](outputs/OBP_WEB_001_IMPLEMENTATION.md); implementation `cade635`, merge `7f49eeb`, D-032. |
| 17 | `OBP-DESK-001` | Merged | `codex/obp-desk-001-networking` | `OBP-WEB-001` | [Evidence](outputs/OBP_DESK_001_IMPLEMENTATION.md); implementation `410a6a0`, merge `7e4fc14`, D-034. |
| 18 | `OBP-QA-001` | Merged | `codex/obp-qa-001-nat-canary` | `OBP-CLI-001`, `OBP-DESK-001` | D-041 merge `d695e4a`; [functional acceptance v2](outputs/OBP_QA_001_FUNCTIONAL_ACCEPTANCE_V2.md); consumer NAT skipped/unqualified; Linux P5 `c453f3e` qualified separately. |
| 19 | `OBP-MIG-001` | Review | `codex/obp-mig-001-retire-legacy-seed` | `OBP-QA-001` merged `d695e4a` | Published implementation `1d649ce`; [migration map and tests](outputs/OBP_MIG_001_IMPLEMENTATION.md). |
| 20 | `INT-KU-OBP-001` | Planned | `codex/int-ku-obp-001-product-journey` | `KU-QA-001`, `OBP-QA-001` | â€” |
| 21 | `KU-ENC-001` | Merged | `codex/ku-enc-001-framework-contract` | `KU-RUN-001` | Owner-authorized merge `22599d0` on `origin/main`; [contract evidence](outputs/KU_ENC_001_CONTRACT.md), D-019. |
| 22 | `KU-ENC-002` | Merged | `codex/ku-enc-002-shared-encoder` | `KU-ENC-001`, `KU-RUN-001` | Owner accepted under D-020; merge `dc04b71` on `origin/main`; [implementation and verification](outputs/KU_ENC_002_IMPLEMENTATION.md). |
| 23 | `KU-ENC-003` | Blocked | `codex/ku-enc-003-model-qualification` | `KU-ENC-002` | Separate branch retained at `4a8f29d`; artifact preflight only, no model runs/qualified tuples. Owner reports new VI/EN workbooks; contents and reviewer/locked-run evidence unverified. |
| 24 | `KU-SEM-001` | Deferred | `codex/ku-web-001-workflow` (local) | Shared selection follow-up | D-024 accepts implemented foundation; remaining quality work deferred. [Evidence](outputs/KU_SEM_001_IMPLEMENTATION.md), [backlog](outputs/KU_SEM_001_CONTRIBUTOR_BACKLOG.md). Unmerged; v2 host activation not accepted. |

## Per-task update protocol

### KU-WEB-001 reserved reconciliation feedback â€” 2026-09-07

Owner reported that checking an error's recorded outcome cleared the error and
only displayed `reserved`. Web now retains the original error during reconcile,
explains each recorded state, and places explicit reservation cancellation beside
the reserved outcome. Reserved is not interpreted as running inference or a
successful preparation. Thirteen Web tests pass, including reserved recovery,
editor unlocking after cancellation, and no automatic inference/save replay.
This UI correction does not resolve the separate model `oneof` schema failure.

### KU-WEB-001 semantic guidance and timeout â€” 2026-09-07

Owner approved semantic-analysis guidance and requested a higher experimental
timeout. The local Ollama policy now allows 10 minutes, retains standard token
limits, and preserves elapsed-time accounting through preparation. The provider
adds a pinned semantic guide while retaining the reviewed system prompt.
See [implementation and async follow-up scope](outputs/KU_WEB_001_SEMANTIC_TIMEOUT.md).
The owner-described async encoding/sharing direction remains a follow-up design
requirement; the current host still awaits preparation and does not publish.
Validation passed: 20 shared extraction tests, 12 KU API tests, 22 node KU tests,
18 Python contract tests, 12 Web tests, build and vNext validator. Real qwen3:8b
development preview/save/restart-read passed with 139.519s inference/validation
using an isolated test Registry. Local owner host rebuilt/restarted and checked
ready. Changes remain in the current review working tree; no merge or push.

### KU-WEB-001 timeout feedback â€” 2026-09-07

Owner local testing reported `ResourceExhausted / deadline`. Web feedback now
explains the 120-second workflow timeout, displays browser-observed progress and
elapsed time, and offers reconciliation without replay. Twelve Web tests and
the production build pass; backend limits and model quality status are unchanged.
See [timeout feedback evidence](outputs/KU_WEB_001_TIMEOUT_FEEDBACK.md).
This follow-up remains in the current task working tree for review.

### KU-WEB-001 experimental Ollama review evidence â€” 2026-09-06

Owner D-023 approved the addition. Implementation `5e540e5` adds governed text
intake, installed Qwen3 selection, the shared extraction workflow and an isolated
Windows Ollama worker with exact tokenizer/artifact and resource controls.
The actual `qwen3:8b` development probe passed intake â†’ ready preview â†’ explicit
private save â†’ restart/read, with inference/validation taking 109.037 seconds.
The test Registry is synthetic and confined to temporary integration tests.

Verification: 27 API library + 8 integration, 22 node KU, 18 encoder extraction,
10 Web component/transport tests and the real owned-worker shutdown probe pass.
Web build/lint, host example, feature-disabled API, generated Base projections,
vNext contracts and format/whitespace checks pass. Existing unrelated warnings
remain. See [run instructions, pins and precise evidence limits](outputs/KU_WEB_001_OLLAMA_IMPLEMENTATION.md).
AI remains unqualified; no holdout, mobile, rollout or merge work was performed.

### KU-ENC-002 review evidence â€” 2026-09-06

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

### KU-REV-001 review evidence â€” 2026-09-05

- Starting main: `3704da6b68237f50998f73c02bc5a2c59d27def8`, clean and equal to
  fetched `origin/main` before branching.
- Deliverable: [KU_AUTHORITY_AUDIT.md](outputs/KU_AUTHORITY_AUDIT.md), including
  authority/ownership/lifecycle/storage, legacy contradictions, owner-resolved
  direction and outstanding specification work.
- Owner clarification: [D-011â€“D-014](DECISIONS.md#d-011--deterministic-identity-after-semantic-normalization).
  Direct encode/verify OBT issuance is the requested new direction; the earlier
  benefit-only restriction is not to be silently retained in future design.
  Reward amounts, admission, replay prevention and settlement remain unspecified.
- `python scripts/ci/validate_vnext_contracts.py` â€” PASS (99 tasks, 18 ADRs,
  37 negative assertions; existing canonical contracts unchanged).
- `git diff --check` â€” PASS.
- Local file-link check â€” PASS (47 links across the four handoff deliverables).
- No source/application/mobile changes. No runtime tests or production
  encode/verify/Registry-sync/reward implementation claims.
- Owner explicitly authorized merge and KU-REV-002 in this conversation.
  Revalidated the clean, pushed audit tip, merged and pushed `25d008d` to main;
  README pointer now advances to `KU-REV-002`. No branch deletion requested.
- Audited content checkpoint: `c89f848`, pushed successfully to
  `origin/codex/ku-rev-001-canonical-audit`. The final branch tip also contains
  the ledger-only follow-up recording this checkpoint; resolve the tracked
  branch for that tip rather than treating this content hash as a merge commit.

### KU-REV-002 review evidence â€” 2026-09-05

- Starting main: `80119e1311b1e95171e5613e0335ad3ef69fa2a4`, clean and equal
  to `origin/main`. This includes the authorized KU-REV-001 merge `25d008d`
  and its handoff pointer update.
- Deliverable: [KU_RUNTIME_GAP_MAP.md](outputs/KU_RUNTIME_GAP_MAP.md).
  Maps semantic identity, Registry, canonical/public/private storage, Base
  operations, Mapping/adoption, existing interfaces, migration and delegated
  work/reward gaps. D-011â€“D-014 remain required future direction.
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

- `python scripts/ci/validate_vnext_contracts.py` â€” PASS (99 tasks, 18 ADRs,
  37 negative assertions; existing canonical contracts unchanged).
- `git diff --check` â€” PASS. Local file-link check â€” PASS (104 links across
  the four changed handoff files).
- Existing compiler dead-code/unused-import warnings were observed; no fixes made within
  this read-only audit. No claim of whole-workspace, live multi-node
  qualification, full-size Registry qualification or D-011â€“D-014 completion.
- After this review was reported, the owner requested "lÃ m task káº¿ tiáº¿p".
  Treated this as authorization for the prerequisite merge and KU-CON-001;
  rechecked the clean/pushed branch and contract/diff gates, then merged and
  pushed `b872263` to main. No branch deletion requested.
- Audited content checkpoint: `496d340`, pushed successfully to
  `origin/codex/ku-rev-002-runtime-map`. The branch tip additionally contains
  the ledger-only commit recording this checkpoint; resolve that tracked
  branch for its final tip. This content checkpoint is not a merge commit.

### KU-CON-001 review evidence â€” 2026-09-05

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
- `python scripts/ci/validate_ku_product_contract.py` â€” PASS:
  11 operations, 18 DTOs, 11 valid/invalid DTO fixtures. Fixture byte/hash values
  are shape examples, not golden canonical/hash conformance evidence.
- `python -m unittest scripts.ci.test_validate_ku_product_contract
  scripts.ci.test_validate_vnext_product_profile
  scripts.ci.test_validate_vnext_ws_profile
  scripts.ci.test_validate_vnext_cli_profile
  scripts.ci.test_validate_vnext_desktop_web_ux_profile
  scripts.ci.test_validate_base_v1_runtime_interface` â€” PASS: 101 tests,
  including 33 new KU tests with mutation subcases and 68 existing contract tests.
- `python scripts/ci/validate_vnext_contracts.py` â€” PASS: the candidate plus
  existing 99 tasks, 18 ADRs, 37 negative assertions, 841 normative lines and
  479 specification links. Existing canonical profile/domain/IDL inventories
  and generated runtime declarations unchanged.
- `git diff --check` â€” PASS. Only candidate specification/inventory,
  contract validators/tests and handoff/index/coverage files changed. No
  application runtime, screen, mobile, networking or rollout implementation.
- Local file-link check â€” PASS: 53 links across the candidate profile and
  changed handoff README/PROGRESS files, in addition to the global link gate.
- Current pointer remains KU-CON-001 pending owner review and merge. This is
  completion of the owner-reviewable deliverables, not approval/freeze of new
  public behavior and not implementation of KU-RUN-001.
- Reviewed content checkpoint: `8b6aa9d`, pushed successfully to
  `origin/codex/ku-con-001-product-contract`. The final tracked tip additionally
  contains the ledger-only commit recording this checkpoint. This is not a
  merge commit or owner acceptance of KU-PC-A/B/C.

### KU-CON-001 owner acceptance â€” 2026-09-05

- Owner answered "Ä‘á»“ng Ã½" after the completed review at `b5956e8`.
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
- `python scripts/ci/validate_vnext_contracts.py` â€” PASS, including 480
  specification links and unchanged canonical inventories.
- `git diff --check` â€” PASS.

### KU-RUN-001 registration preflight â€” 2026-09-05

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

1. [KU profile Â§1](../../specs/vnext/KU_PRODUCT_WORKFLOW_PROFILE_V1.md#1-authority-and-approval-boundary)
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
3. [Canonical Â§6.2](../../specs/vnext/CANONICAL_PROFILE_V1.md#62-reserved-v1-domains)
   calls addition of a domain a contract change. The current canonical
   inventory has no `semantic-content` entry. The
   [Base IDL](../../../src/test-vectors/vnext/base-v1-runtime-interface-v1.json)
   has no generated KU operation DTOs. Its generic `BaseLocalCommandV1.kind`
   is not an allocated KU discriminator. [Base Â§Â§8â€“9](../../specs/vnext/BASE_V1_RUNTIME_INTERFACE_PROFILE.md#8-generated-projections)
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
   implementation and D-012â€“D-014 distribution/work/reward changes excluded.

Owner direction is requested only for this scope extension. No request to
approve KU-PC-A/B/C again, merge, delete branches or enable default rollout.

#### Fresh validation and evidence limit

- `python scripts/ci/validate_ku_product_contract.py` â€” PASS: 11 operations,
  18 DTOs, 11 fixtures; tool explicitly reports registration pending.
- `python -m unittest scripts.ci.test_validate_ku_product_contract
  scripts.ci.test_validate_base_v1_runtime_interface` â€” PASS: 74 tests.
- `python scripts/ci/validate_vnext_contracts.py` â€” PASS: existing contract
  inventories, including 21 foundation domains and 27 Base runtime operations.
- `git diff --check` â€” PASS. Local file-link check for README/PROGRESS â€” PASS.
- Preflight content checkpoint `8464770` was pushed to
  `origin/codex/ku-run-001-shared-service`; the branch also includes the
  ledger-only follow-up recording this checkpoint. No merge or branch deletion.
- Source, canonical contracts, generated declarations, tests and rollout state
  are unchanged. Only README/PROGRESS handoff records changed. Runtime tests,
  workspace check and Rust format are not claimed; KU-RUN-001 remains
  incomplete and cannot be marked Review on this evidence.

### KU-RUN-001 review evidence â€” 2026-09-06

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

### Encoder framework research and task amendment â€” 2026-09-06

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

### KU-RUN-001 merge and encoder handoff â€” 2026-09-06

- Owner explicitly authorized merging KU-RUN-001 into main and starting
  KU-ENC-001. Fresh generated/global contract checks, 13 KU runtime tests,
  workspace check and format passed on the clean synchronized branch.
- Merged tip `13e03f3` with merge `d1417018a236798a910ceb625fbe5fd0b10dc406`
  and pushed `origin/main`. No branch deletion or rollout change.
- README/progress pointer advances to KU-ENC-001; the accepted direction is
  D-018. No further merge or framework-direction approval is required here.

### KU-ENC-001 contract review â€” 2026-09-06

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

### KU-API-001 review evidence â€” 2026-09-06

- Implementation checkpoint `29c34d1` was pushed successfully. The following
  ledger-only commit records that content checkpoint; neither is a merge.
- D-021 owner follow-up authorized the early local API/Web MVP path. API work
  starts from `ca1d4c22f61864a5bde53edf5c399e6ffdbc1943` on its own branch;
  the qualification branch remains preserved separately at `4a8f29d`.
- Added the versioned local KU REST projection and three authenticated routes
  for all eleven registered operations. No new Base/DTO IDs, canonical bytes,
  extraction implementation, Registry authority or WS vocabulary.
- [Implementation evidence](outputs/KU_API_001_IMPLEMENTATION.md) records
  API behavior, host integration needs, bounded/private envelopes and the
  explicit unqualified-model status. Host intake and Web UI remain the next
  product integration step; a complete runnable Web demo is not claimed here.
- Default API suite: 22 library plus 8 integration tests pass. Opt-in network
  library suite: 24 pass. Feature-disabled check, workspace format, generated
  Base projections, global vNext contracts and diff checks pass.
- New tests exercise exact save/read/replay, pagination and revisions,
  cancellation without the node mutex, auth/generation/privacy rejection,
  unavailable dependencies, export boundaries, unresolved and unqualified AI
  requests, response overflow and all Base error policies.
- Owner's new holdout files were not opened; no inference, download, rollout,
  merge or branch deletion. Current pointer remains KU-API-001 for review;
  KU-WEB-001 follows the accepted API merge.

### KU-API-001 merge and Web handoff â€” 2026-09-06

- Owner reviewed and accepted tip `423b7b8`, authorizing merge and handoff
  closure under D-022. The task branch was clean, pushed and synchronized.
- Fresh `cargo test --locked -q -p onebrain-api`: 22 library and 8 integration
  tests pass. Generated Base check, global vNext contracts, workspace format
  and diff checks also pass. No runtime changes were made during closure.
- Merged and pushed `3eba370df1df91627595e0acbf7645d94ea75276` to main.
  API task is Merged; current pointer advances to KU-WEB-001, still Planned.
- The next conversation should build the small local Web journey under D-021,
  accounting for API host-intake/Registry dependencies and unqualified AI.
  It must not invent client-side authority or treat fixture evidence as a
  usable production source pipeline.
- No Web branch or implementation started, holdout opened, model run, rollout
  enabled or branch deleted during closure. KU-ENC-003 remains separate.

### KU-WEB-001 Ollama follow-up â€” 2026-09-06

- Owner requests actual Ollama inference, model selection (`qwen3:8b`) and text
  encoding through the previously designed shared framework. This expands the
  manual MVP; it does not accept the existing manual-only deliverable as final.
- [Concrete amendment and exact conflict](outputs/KU_WEB_001_OLLAMA_AMENDMENT.md)
  proposes a local experimental lane with `model_qualified: false`, preserving
  technical admission/custody/validation/recovery controls. The existing REST
  contract explicitly rejects AI before full qualification; owner direction is
  required before changing that activation rule. No request to reapprove the
  shared architecture or to merge/delete the branch is being made.
- Installed `qwen3:8b` was confirmed by `ollama list`; no inference/model download,
  holdout read or runtime change. Qualification branch and corpus stay separate.
- The reviewed manual implementation remains at `ac5ce80` plus handoff `f316e5a`.
  This follow-up records the requested change; it does not claim AI completion.

### KU-WEB-001 implementation review â€” 2026-09-06

- Implementation checkpoint `ac5ce80` was pushed successfully. The branch tip
  additionally contains this ledger-only record; neither commit is a merge.
- Started from clean `main` at `798eabfee8acaa8cb473aa99fa52ccfd6f124f7e`,
  equal to fetched `origin/main`; exact task branch `codex/ku-web-001-workflow`.
- Implemented the local `/ku` manual create/preview/validate/save/search/inspect/
  revise journey, plus the scoped opt-in host editor and loopback launch example.
  [Evidence and run instructions](outputs/KU_WEB_001_IMPLEMENTATION.md) specify
  real host prerequisites and the finite predicate/text editor's limitations.
- Added the versioned editor transport contract before implementation. Native
  source custody, signed Registry selection, generated KU payloads, canonical
  validation, encrypted persistence and Base generation fences retain authority.
  No new Base IDs, canonical bytes, network lane, model or default rollout.
- API default suite: 24 library + 8 integration tests pass; node KU suite: 19
  pass; opt-in network-feature API library suite: 26 pass. Host example and
  feature-disabled API compile. Web: 8 component/transport
  tests, 2 existing receipt tests, automated accessibility, production build and
  lint pass under the existing non-KU warning policy. Generated Base, global
  vNext and format/diff checks pass. Full details and evidence limits are linked.
- Operator Registry/source/Vault provisioning is required. No production dataset
  or full physical-browser/assistive-technology run is claimed; source/Registry
  fixtures appear only in tests. New VI/EN holdouts and KU-ENC-003 remain untouched.
- Ready for owner review. Keep this task current and preserve its branch;
  no merge, deletion, deployment or next-task start is included.

### Update protocol

D-027 continuation exception: current baseline merge is local only and explicitly
identified as such; task-004 review does not authorize remote publication. The
older push/clean-main checklist below must not discard the retained workspace
or trigger an unsolicited push.

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

### 2026-09-23 — OBP-QA-001

Local preflight is complete; product acceptance is not. The current task has no
verified two-consumer independent-network/two-relay environment or scoped test
host assembly/execution authority. No independent-host fault or native GUI/OS
journey was run. P5 production-reference also lacks this candidate's signed
request/inventory, three physical hosts, provider/topology evidence, receipts
and complete fault/oracle collection. Historical Linux qualification cannot
promote this candidate/platform. See [scenarios](outputs/OBP_QA_001_ACCEPTANCE_V1.md)
and [measured local results](outputs/OBP_QA_001_PREFLIGHT.md). Do not enable live
networking to fill this gap; task 19 remains Planned. All changes are local,
uncommitted and retained on the QA branch.

### 2026-09-20 â€” KU-SEM-001 (historical gate; D-024 defers further work)

Owner approved the separate draft/selection v2 representation and implementation.
Shared encoder, node dispatch and Web rendering have been added in the original
working tree; [implementation evidence](outputs/KU_SEM_001_IMPLEMENTATION.md)
records checks and failed development iterations. The activation gate remains
unaccepted because the real-model rocket/reference outputs still lose meaning.
No canonical conflict was silently resolved, no independent verifier is claimed,
and no commit/push/merge or live-host restart was performed. Continue from the
original dirty baseline, preserving failed raw reports and v1 records.

Continuation: predicate/qualifier overlap now dispatches a finite-choice repair.
The real Gemma replay accepted that repair and retained both revisions, but the
rocket case still fails reference/future meaning checks. Encoder checks are now
70 passed/one ignored, API 8 passed; final staging build and validator passed.
The completed Qwen transport retry (1/6) and Gemma comparison (2/3) remain known
development evidence only. See the linked report for commitments and limitations.

Record the exact canonical conflict, missing authority, failing gate or external
dependency here. Do not replace `Blocked` with an inferred product behavior.
