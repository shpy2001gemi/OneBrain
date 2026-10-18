# OneBrain concept MVP — master plan

Owner direction: **2026-09-28, D-044** in [DECISIONS](DECISIONS.md).
This is the single active delivery plan. [PROGRESS](PROGRESS.md) owns status,
branch/worktree inventory and the next action; task files own acceptance.
The September implementation history remains in PROGRESS and existing outputs.

## Product question and minimum useful demonstration

Can a person turn a small piece of knowledge into an inspectable local object,
keep and find it after restart, and explicitly exchange that object with another
node while retaining identity, provenance and control over private content?

The MVP answers this with a working narrow path, not a claim that arbitrary
language extraction, global knowledge convergence or token economics is solved.
Use the existing Rust/node services and local Web as the primary demonstration
surface on the available Windows host. CLI is the practical inspection/second-node
surface; Desktop reuses the Web workflow. No additional UI framework is needed.

The concept demonstration has three visible outcomes:

1. A supported manual/resolved draft can be previewed, explicitly saved, searched
   and reopened after restart with the same identity. A few ordinary public or
   developer-owned text examples also exercise the existing experimental model
   path; keep successful proposals and visible unresolved/needs-review outcomes.
2. Two real node instances explicitly exchange an accepted KU; the receiver can
   inspect its identity and provenance. Two processes on one host are sufficient
   for the first concept demo. This does not establish consumer-NAT independence.
3. A new contributor can find the prerequisites, build/run commands, sample
   journey, architectural seams and a bounded improvement to make. Document setup
   that still needs operator input; do not advertise a one-command install yet.

## Remaining MVP sequence

```mermaid
flowchart LR
    A["Existing shared KU + CLI/Web/Desktop"] --> B["KU-QA-001: local demo and focused smoke"]
    B --> C["INT-KU-OBP-001: two-node journey"]
    D["Existing OBP runtime and functional QA"] --> C
    C --> E["Same integration task: runnable contributor entry"]
    Q["KU-ENC-003: deferred qualification"]
    S["KU-SEM-001: deferred fidelity improvements"]
```

| Parent task | Substeps kept in that task | MVP completion |
|---|---|---|
| [KU-QA-001](tasks/09-KU-QA-001.md) | A: reconcile available implementation/setup; B: local save/search/restart; C: small experimental AI check and shared-surface smoke | Local concept can be demonstrated and actual limits are recorded; strict ENC-003 is not a dependency |
| [INT-KU-OBP-001](tasks/20-INT-KU-OBP-001.md) | A: explicit two-node exchange; B: receiver read/identity and one retry or restart; C: contributor build/run instructions and backlog links | Repeatable concept journey plus a usable open-source entry point, without platform qualification claims |

These are existing tasks 09 and 20, not a new milestone/task hierarchy. Planning
this scope does not mark either task implemented. Each next step fixes the first
concrete obstacle to the demo, rather than expanding an evaluation framework.
Dependent local work may use a reviewed available implementation before merge;
record the actual base in PROGRESS. Publication/merge state remains separate.

## MVP checks and stopping rule

- Run the primary happy path once after relevant changes; inspect the result.
- Use focused existing tests for the changed module. Add a small regression only
  for a real bug or a meaningful data/consent boundary; avoid implementation-mirror
  tests and automatic full-workspace reruns for documentation or small UI edits.
- Check saved data after one restart and that save/share require their explicit
  action. Show a useful failure when AI/network is unavailable or input unresolved.
- For experimental AI, inspect about five short VI/EN developer examples with one
  available admitted model. Record what worked, what failed and rough observed
  elapsed time. No accuracy threshold, blind benchmark, reviewer signature, second
  model, p95 campaign or statistical report is required to accept this demo.
- Reuse CLI/Desktop component results; a known unrelated platform test-host failure
  is a documented follow-up, not a reason to hold the Web/CLI concept indefinitely.
- Record command/result and limitations briefly in PROGRESS or the existing parent
  task output. Stop expanding checks when the scoped journey works and no concrete
  data-loss/privacy/false-success defect remains in it.

Model drafts remain proposals unless the existing canonical workflow accepts them.
Use the working manual/resolved path when lowering is unsupported; that limitation
must be apparent to the tester. Do not call manual corrections automatic encoding.

## Shared framework contributors should extend

| Seam | Existing owner/location | Rule for an MVP contribution |
|---|---|---|
| Canonical knowledge, identity and storage | `src/ku-core`, `src/onebrain-base-contract` | Reuse registered types/codec; keep durable identity stable |
| Extraction and model adapters | `src/ku-encoder/src/extraction` | Models return proposals; reuse the shared workflow and bounded provider interface |
| KU jobs, save/recovery and orchestration | `src/onebrain-node` | Keep one service owner; avoid frontend-specific knowledge semantics |
| Local API and user experience | `src/onebrain-api`, `src/onebrain-cli`, `src/onebrain-web`, `src/onebrain-desktop` | Project the same service; improve setup and visible errors in place |
| Peer exchange | `src/ku-net`, `src/onebrain-relay`, node networking | Reuse authenticated transport/reconciliation and explicit publication |
| Qualification tools | `scripts/encoder/qualification_*` and retained ENC-003 branch | Preserve for later; do not turn them into the MVP's prerequisite |

Public contracts still govern changed behavior. Read the relevant contract when
changing that interface, not every research/evidence document for every substep.

## Architecture remediation backlog (D-051)

Source: [architecture review 2026-10-18](outputs/ARCHITECTURE_REVIEW_2026-10-18.md)
and owner direction [D-051](DECISIONS.md#d-051--architecture-review-priorities-shared-mobile-runtime-and-kql-direction).
Work top-down. **ARCH-01 and ARCH-02 precede further task 20 D-045 Web follow-ups**;
the paused Web step in PROGRESS resumes after them. Each item is one bounded change
with a focused regression; use existing parent tasks/ledger, not a new plan per item.

| ID | Pri. | Item | Evidence / scope | Done when |
|---|---|---|---|---|
| ARCH-01 | **Done** `1d85082` (2026-10-18) | Relay outer-auth lifecycle: remove `pending` challenge on every failure/drop path, prune `pending`/`consumed` by expiry, GC durable `ConsumedNonce` | `src/onebrain-relay/src/reservation_store.rs` `OuterClientAuthenticator`; drop guard in `production_service.rs`; expiring nonce rows in `state.rs` (older 32-byte rows are retained, not guessed at) | 5 regressions + existing suite: `cargo test -p onebrain-relay` 27 + 8 + 11 PASS; loopback/unit only, no live relay run |
| ARCH-02 | P0 — next | Fence the legacy TCP/JSON listener: no bind unless explicit legacy-compat opt-in, loopback by default, connection cap, smaller pre-auth allocation, gate `VerifyRequest`/`KuPush`; label `/api/peers/connect` legacy | `src/onebrain-node/src/node.rs` L454-L510 binds `0.0.0.0` on every `start_network()`; listener loop/`KuPush`/`VerifyRequest` near L3839-L3978 | Default CLI/Desktop/vNext start opens no unauthenticated port; legacy path still works when opted in; task 20 two-node happy path rechecked |
| ARCH-03 | P1 | KU journal capacity: compaction/GC after commit and cancel; journal-full becomes non-retryable `storage_full`, not `429 rate_limited` | `ku_product.rs` L39-L41, L1064-L1095; `ku_api.rs` L179-L183 | Capacity test past 1,024 operations; saved data preserved across restart |
| ARCH-04 | P1 | Freeze legacy textual KQL honestly: reject CREATE/UPDATE/DEPRECATE/WATCH (never persisted), AND/OR precedence + parentheses, depth/work bounds, EXPLAIN reports `local_scan`, fix CLI help and Web autocomplete | `node.rs` L1753-L1778; `ku-kql/src/parser.rs` L851-L878, L705-L716; `executor.rs` L885-L920; `cli/help.rs`; `KqlEditor.tsx` | Parser precedence/fuzz tests; documented examples parse and run |
| ARCH-05 | P1 | Spec status honesty: add default-build / feature-gated / library-only / spec-only column to `docs/specs/vnext/README.md`; refresh drifted PILLAR_REVIEW, STUB_TRACKING, KQL_SPEC, Cargo.toml comments | Review §6 | vNext validator passes; no “Complete” row implies user reachability without saying so |
| ARCH-06 | P2 | One user-facing KU model: route chat mediator encode and legacy Encode/Explorer pages to `/api/vnext/ku` or label them legacy; move Core DNA/AiEncoder/`obt_*`/DHT-SWIM/`seed_client` behind a `legacy` feature | `ku-mediator/src/mediator.rs`; `ku-core/src/lib.rs`; `ku-encoder/src/lib.rs`; `ku-net/src/lib.rs` | Default build/UI creates only vNext KUs or visibly labels legacy |
| ARCH-07 | P2 | Lower simple AI review drafts (concept–predicate–concept, quantity) to canonical drafts through `compiler.rs` with CCID resolution | Links KU-SEM-001; `semantic_selection_v2.rs`, `compiler.rs` | One VI and one EN example saved as canonical KU after explicit review; unresolved stays unsaveable |
| ARCH-08 | P2 | KU structured search on `vnext_semantic_index` (D-051 KQL choice): add a KU source, persist/rebuild with the store, query by concept/predicate/unit/dimension plus folded labels | Replaces `text.contains` at `ku_product.rs` L1846-L1850 | Concept-only KU findable; index rebuild deterministic after restart |
| ARCH-09 | P2 | Split `onebrain-node` into desktop-agnostic core (KU/Base/archive/OBP services), host (HTTP/Ollama/listeners), legacy and harness (m5/m6/scale/canary); split `ku_product.rs` and move KU dispatch out of `base_runtime.rs`; enforce API/CLI depend only on the node façade | Review §2, §6.3 | `cargo tree` layering check in CI; prerequisite for ARCH-10 |
| ARCH-10 | P3 | Mobile shared runtime (D-051): `onebrain-mobile-core` consumes the ARCH-09 node core instead of its own draft/archive implementations | Mobile lane only; requires mobile AGENTS/build-contract process and MOB/spec updates first | Mobile validator green; no mobile-only fork of canonical behavior |
| ARCH-11 | P3 | KQL surface reduction: unconsumed vNext modules and `graph_storage` behind `experimental` feature; label `/api/vnext/kql/needs` hash-derived until a bounded FIND→NeedIR compiler exists; move `storage`/`blob_storage` out of `ku-kql` with ARCH-09 | `vnext_api.rs` L1089-L1158; review §5 | Default build excludes experimental modules; their tests still run under the feature |
| ARCH-12 | P3 | Storage recovery units: document or consolidate the ~30 redb stores; stop committing `src/onebrain_data/*.redb` | Review §2, §6.4 | Written recovery-unit map; sample DBs removed from Git |
| ARCH-13 | P3 | Governance trim: split `validate_vnext_contracts.py` per profile; replace source-snippet needle checks with Rust tests | Review §6.5 | Validator still green; refactors no longer blocked by string matches |
| ARCH-14 | P3 | OBP field hardening: consumer-NAT smoke (two home/hotspot networks + VPS relay), DPAPI/keystore signer via `SessionIdentitySigner`, IPv6 /64 admission buckets, bound authenticated relay connections | Review §4.3 | Results recorded with scope; no qualification claim beyond evidence |

## After-MVP and contributor backlog

This is the central queue. Detailed existing specs/defect records stay at their
owners; the entries below link to them rather than spawning new plans.

| Area / existing owner | Bounded contribution | Completion example / dependency |
|---|---|---|
| Task 20.C — first-run experience | Make the existing local host easier to provision, document prerequisites and useful errors | A fresh contributor can follow the declared setup and demo; no hidden maintainer-only files |
| Task 20 follow-up — share projection | Project the D-045 shared share module through a registered product API/Web action and normal node host composition | Preserve exact preview/confirmation, private IDs/source suppression and pending/delivery distinction; current concept uses the isolated helper |
| KU-SEM-001 | Pick one reference/alternative/qualifier defect from the [semantic backlog](outputs/KU_SEM_001_CONTRIBUTOR_BACKLOG.md) | One reproducible case, shared fix and focused regression; broader fidelity stays deferred |
| KU-QA-001 follow-up | CLI/Desktop parity and native lifecycle portability | Reproduce one actual difference or host issue and fix it; no full platform lab required to contribute |
| KU-ENC-003 | Finish one missing harness/provider/measurement/evaluator seam | See [task 23](tasks/23-KU-ENC-003.md); formal qualified claims still require its full original conditions |
| OBP-QA-001 follow-up | Consumer NAT, native OS lifecycle, non-Linux paths | Reproduce on available real environment; record scope; do not reopen completed functional acceptance |
| Base / Registry | Simplify contributor setup and later strict release closure | Preserve existing trust/data; [historical status](../../PROJECT_STATUS.md) and runtime plan own production gates |
| Mobile | Existing autonomous mobile backlog | Separate later lane under mobile AGENTS/build contract; no mobile implementation is authorized by this plan |
| M6 — distributed KQL / Outcome / Benefit | Implement a bounded slice when that lane is selected | [Distributed runtime plan](../../research/WIP_DISTRIBUTED_RUNTIME_IMPLEMENTATION_PLAN_V2.md), sections 12–13; outside this MVP |
| M7 — economics and wallet | Reconcile reward-policy decisions, then versioned implementation | Same runtime plan section 14 plus D-013/D-014; no production token promise in MVP |
| Foundation optional | RUN-003 remote cognition, RIB-001/002 fast-path reconciliation | [Foundation plan](../../research/ONEBRAIN_FOUNDATION_IMPLEMENTATION_PLAN_V7_1.md); do not make optional work a new MVP prerequisite |
| Extension / bot / glasses / BCI | Later adapters/research after the working shared core demo | Existing scaffold/research; outside current MVP |

## Document and branch discipline

- README and NEXT_CONVERSATION link to this plan, PROGRESS and the current task.
  They do not contain a parallel chronological history or task-state table.
- PROGRESS is the only execution ledger. Its inventory distinguishes main,
  unmerged committed work, unpublished commits, dirty files and other worktrees.
- Subtasks use headings/checklists in the existing parent task. Update the same
  output file if a substantive report is useful; routine runs need only a ledger
  line. Keep old evidence read-only unless correcting it explicitly.
- Before creating a branch/worktree, inspect the relevant existing refs and dirty
  work. Reuse suitable work; do not branch from main and silently omit useful code.
  A subtask branch must name its parent task/base and feed back into this ledger.
- Do not copy older PROGRESS/DECISIONS from a retained branch over the active
  ledger. Integrate useful code selectively when appropriate; document the actual
  Git result. No automatic branch deletion, reset or cleanup of other work.
- Keep current limitations accessible to contributors without requiring them to
  recreate a historical audit trail. [CONTRIBUTING](../../../CONTRIBUTING.md)
  explains the entry path and proportional checks.
