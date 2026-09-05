# OneBrain Project Status

[Tiếng Việt](PROJECT_STATUS.vi.md)

> Snapshot: **2026-09-05 (Asia/Saigon)**
>
> Audited source: `main` / `origin/main` at
> `409fca34db8faaf238b899a2481175d922113b99` before this documentation update
>
> Scope: repository state, local Git branches/worktrees, source-controlled
> qualification claims, current validators, and recent CI evidence.

This is the current progress entry point. Specifications describe required or
target behavior; they are not, by themselves, proof that a production path is
complete.

## Executive status

| Workstream | Current state | What remains open |
|---|---|---|
| vNext contracts and foundation | **Complete at repository contract/foundation scope.** The current validator reports 99 tasks, 18 ADRs, 37 negative assertions, and 55 foundation vectors across 21 domains. | Product defaults, operator rollout, and later milestones remain separate gates. |
| Product integration P0-P3 and DR-M5 | **Implementation and recorded CI evidence complete.** Runtime ownership, REST/private WebSocket/CLI/Desktop-Web surfaces, resource admission, observability, crash, chaos/fuzz, compaction, rollback, and accepted M5-07 soak evidence are in `main`. | vNext lanes remain opt-in/default-off; the legacy live path has not been fully retired. |
| Base v1 | **Released with owner-approved disclosed exceptions** as signed tag `base-v1.0.0-owner-waiver.1` at `1e0fb2321aee`. Three-OS qualification, prebuilt Registry verification, P5 verification, and a continuous three-runner 72-hour soak are recorded in the tag and CI. | The strict tag `base-v1.0.0` is absent and the release explicitly does **not** claim `BASE-GATE-V1 qualified=true`. The owner must either accept the waiver release as the terminal Base v1 milestone or authorize a new strict qualification closure. |
| Concept Registry and P5 production | **Production-reference evidence exists under the Base owner waiver.** | Source-controlled strict status remains open: Registry vectors retain `production_qualified=false`; P5 retains `provider-document-pending`, `non-linux-platform-lanes-pending`, and `mobile-carrier-mailbox-pending`; operator-approved product rollout is not claimed. |
| Mobile | **Partial BootstrapOnly/Limited implementation.** MOB-05A signed admission and the Android portion of MOB-05B through ABI 13 Local Import are implemented; contract validation passes. | MOB-05B peer/iOS/provider work, MOB-05C full 2.2 GB-class activation, MOB-04 private KU completion, MOB-06 AI/tools, MOB-07 media/lifecycle, physical-device gates, MOB-08 networking, and MOB-09 release remain open. `ReadyOffline` is not claimed. |
| M6 active distributed KQL and Outcome/Benefit | **Not opened as the next production milestone.** Existing M3/M4 bounded one-hop KQL and Public UseEvidence capabilities do not close M6. | Active multipath/provider discovery and end-to-end Use -> Outcome -> Benefit flows require their P5/Registry entry gates. |
| M7/OBT | **Prototype/legacy only; no production economy.** | Benefit-based reward policy, ledger/finality vNext, operational wallet, and adversarial production gates. |
| Extension, bot, glasses, and BCI | **Scaffold or research.** | Product implementation, qualification, and, for BCI, sufficient external safety evidence. |

## Base v1 release boundary

The annotated `base-v1.0.0-owner-waiver.1` tag records:

- candidate commit `1e0fb2321aeec04cb711f4259e2bc807e73a35dd`;
- successful three-OS run
  [33529983318](https://github.com/shpy2001gemi/OneBrain/actions/runs/33529983318);
- successful Task 28 run
  [33592716241](https://github.com/shpy2001gemi/OneBrain/actions/runs/33592716241);
- a prebuilt Registry root, P5 root, and uninterrupted 72-hour soak root;
- disclosed exceptions for late evidence assembly, corrected frozen test-target
  names, existing all-features Clippy findings, and dependency-policy triage.

The synchronized baseline is seven workflow/handoff commits ahead of that
candidate. CI at
`c65f1739fcd0` includes a successful
[three-OS candidate run](https://github.com/shpy2001gemi/OneBrain/actions/runs/33592237276)
and successful nightly parser fuzz. The tag contains a PGP signature, but this machine cannot
verify it because the signer's public key is not installed locally; therefore
this audit records the tag and its contents without claiming independent local
signature verification.

## Owner-approved implementation direction

On 2026-09-05, the owner decided to resume KU review/product development and
apply one contract across CLI, local Web, and Desktop. The outbound-first OBP
core remains a stable foundation, but productization runs as a separate lane;
the legacy `onebrain-seed` must not be presented as a secure vNext seeder and
the new lane remains default-off until its acceptance gate passes.

The compact entry point for a new conversation is
[`handoffs/2026-09-ku-obp-productization/README.md`](handoffs/2026-09-ku-obp-productization/README.md).
It contains the owner decisions, capability boundary, dependency graph,
progress ledger, prompts, and 20 independently bounded branch tasks. The
current task is `KU-REV-001`; no implementation branch has been created.

## Remaining implementation priorities

1. **Review KU and freeze its product contract.** Execute `KU-REV-001`,
   `KU-REV-002`, then `KU-CON-001` before changing public behavior.
2. **Put KU behind one shared local service.** After contract freeze, project
   the same semantics through local REST/private WS, CLI, local Web, and
   Desktop; KU must remain useful with zero peers.
3. **Productize OBP in a separate lane.** Connect existing lifecycle,
   bootstrap/discovery, reservation, route/outbox, and failover components to
   the normal node aggregate and shared product API; remain opt-in/default-off
   until real acceptance evidence passes.
4. **Integrate KU -> OBP only after both lanes pass QA.** Preserve
   `encode ≠ publish`, `proposal ≠ materialize`, `materialize ≠ adopt`, and
   never turn relay/delivery evidence into authority.
5. **Keep other lanes outside current scope.** The strict Base decision,
   mobile, M6 active multipath KQL, Outcome/Benefit, M7/OBT, and BCI retain
   separate gates; resumed mobile work must follow the mobile build contract.

The detailed mobile order remains in
[`WIP_MOBILE_APP_IMPLEMENTATION_PLAN_V1.md`](research/WIP_MOBILE_APP_IMPLEMENTATION_PLAN_V1.md).
The distributed-runtime history and gates remain in
[`WIP_DISTRIBUTED_RUNTIME_IMPLEMENTATION_PLAN_V2.md`](research/WIP_DISTRIBUTED_RUNTIME_IMPLEMENTATION_PLAN_V2.md).

## Local Git branch audit

The pre-cleanup audit after `git fetch --all --prune` found:

| Check | Result |
|---|---:|
| Local branch heads | 46 |
| Local branches contained in `origin/main` | 46 |
| Local branches with commits not contained in `origin/main` | **0** |
| `main` versus `origin/main` at audit time | 0 ahead / 0 behind |
| Registered worktrees | 12 |
| Existing worktrees | 10, all clean |
| Missing temporary worktrees with prunable metadata | 2 |

**No local branch was unfinished in the Git sense.** Every local branch tip was
already an ancestor of `origin/main`. The 45 non-`main` branches were historical
integration/release branches eligible for local retirement.

Cleanup completed on 2026-09-05:

| Current local state | Result |
|---|---:|
| Local branches | 1 (`main`) |
| Registered worktrees | 1 (the primary repository) |
| Stashes | 0 |
| Commits reachable only from local refs | 0 |
| Remote branches deleted | 0 |

Two apparent divergences are not unmerged work:

- `codex/dr-m5-operational-compaction` is one commit ahead of its namesake
  remote branch, but that commit is already contained in `origin/main`.
- `codex/task28-prebuilt-registry` tracks `origin/main` and is five commits
  behind it; its tip is the owner-waiver candidate, not a branch with missing
  commits.

Branch families already contained in `origin/main`:

| Family | Count | Contents |
|---|---:|---|
| Base v1 implementation | 8 | IDL, archive, authority, contract, freeze, P5, Registry, and storage branches |
| DR-M5 | 8 | Baseline, resource admission, observability, crash, chaos/fuzz, compaction, rollback, and soak branches |
| P1/P2/P3 integration | 14 | Five P1, five P2, and four P3 branches |
| P5/runners/fixes | 9 | P5 preflight, M5-07 acceptance/runners, and runner fixes |
| Task 28 | 4 | Preparation, external request root, prebuilt Registry, and request handoff |
| Documentation/history | 2 | English README and the older Mermaid/PDF conversion branch |

Cleanup pruned the two missing temporary bootstrap registrations, removed nine
clean auxiliary worktrees in dependency-safe order, and deleted all 45 merged
local branches. One already-deregistered Base candidate directory required
long-path cleanup; its empty directory was held by an orphan Ubuntu `wslhost`
session, so only that verified stale process pair was stopped before removal.
Remote branches and tags were not changed.

## Validation evidence for this snapshot

- `python scripts/ci/validate_vnext_contracts.py` — **PASS**.
- Focused OBP assessment on baseline `409fca3`: 378 `ku-net` persist/QUIC
  tests, 18 `onebrain-node` vNext runtime tests, 4 Anti-Gravity Reunion tests,
  31 `onebrain-relay` tests, and 184 `onebrain-node` outbound-first tests —
  **PASS**.
- `python scripts/ci/validate_mobile_build_contracts.py` — **PASS** with 98
  evidence rows, 123 mobile features, 112 screens, 62 components, 13 patterns,
  and no broken links or source-guard failures.
- `git diff --check` — **PASS** after documentation changes.
- `cargo fmt --all -- --check` — **PASS**.
- `cargo check --workspace --locked` — **PASS**.
- Latest audited nightly parser fuzz on `main`:
  [33951243848](https://github.com/shpy2001gemi/OneBrain/actions/runs/33951243848) —
  **success**.

This snapshot does not claim that every workspace test, physical-device test,
or strict production qualification gate was rerun locally.
