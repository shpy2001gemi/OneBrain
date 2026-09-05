# OneBrain Project Status

[Tiếng Việt](PROJECT_STATUS.vi.md)

> Snapshot: **2026-09-05 (Asia/Saigon)**  
> Audited source: `main` / `origin/main` at
> `c65f1739fcd0ac6b7a9518ed44c0ee6f81df41f1`  
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

`main` is five workflow/handoff commits ahead of that candidate. Current CI at
`c65f1739fcd0` includes a successful
[three-OS candidate run](https://github.com/shpy2001gemi/OneBrain/actions/runs/33592237276)
and successful nightly parser fuzz. The tag contains a PGP signature, but this machine cannot
verify it because the signer's public key is not installed locally; therefore
this audit records the tag and its contents without claiming independent local
signature verification.

## Remaining implementation priorities

1. **Resolve the Base v1 strict-release decision.** Preserve the disclosed
   waiver boundary, or run the approved strict process before creating
   `base-v1.0.0`; do not silently reinterpret the waiver as strict
   qualification.
2. **Finish the mobile offline critical path.** Complete MOB-05B/MOB-05C,
   connect deterministic canonical KU encode/preview/private Save, finish local
   Library/search/KQL/export/backup, close media storage/recovery, then execute
   physical-device and store-release gates.
3. **Close production-network entry gates.** Resolve Registry/P5 limitations,
   explicit operator approval, provider evidence, and mobile carrier mailbox
   before enabling normal peer networking or seeding by default.
4. **Open M6 only after those gates.** Implement active distributed KQL and the
   end-to-end Outcome/Benefit evidence flow without expanding authority.
5. **Keep M7/OBT and BCI outside current completion claims.** Both remain future
   milestones with additional policy, safety, and qualification requirements.

The detailed mobile order remains in
[`WIP_MOBILE_APP_IMPLEMENTATION_PLAN_V1.md`](research/WIP_MOBILE_APP_IMPLEMENTATION_PLAN_V1.md).
The distributed-runtime history and gates remain in
[`WIP_DISTRIBUTED_RUNTIME_IMPLEMENTATION_PLAN_V2.md`](research/WIP_DISTRIBUTED_RUNTIME_IMPLEMENTATION_PLAN_V2.md).

## Local Git branch audit

After `git fetch --all --prune`:

| Check | Result |
|---|---:|
| Local branch heads | 46 |
| Local branches contained in `origin/main` | 46 |
| Local branches with commits not contained in `origin/main` | **0** |
| `main` versus `origin/main` at audit time | 0 ahead / 0 behind |
| Registered worktrees | 12 |
| Existing worktrees | 10, all clean |
| Missing temporary worktrees with prunable metadata | 2 |

**No local branch is unfinished in the Git sense.** Every local branch tip is
already an ancestor of `origin/main`. The 45 non-`main` branches are historical
integration/release branches and may be retired after their attached worktrees
are deliberately removed or detached.

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

Worktree cleanup was **not** performed during this audit. Two missing temporary
bootstrap registrations are safe candidates for a later `git worktree prune`;
the other worktrees are real directories and should be retired intentionally.

## Validation evidence for this snapshot

- `python scripts/ci/validate_vnext_contracts.py` — **PASS**.
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
