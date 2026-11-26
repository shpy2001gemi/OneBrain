# OneBrain Project Status

> **Active direction — D-044 / D-050 / D-051:** ship and refine the runnable
> open-source concept MVP on top of the shared architecture without turning
> deferred qualification campaigns into blockers for bounded improvements.
> Use the [single delivery plan](handoffs/2026-09-ku-obp-productization/MASTER_PLAN.md)
> for scope/order and [PROGRESS.md](handoffs/2026-09-ku-obp-productization/PROGRESS.md)
> as the sole authoritative task and Git/worktree ledger.

[Tiếng Việt](PROJECT_STATUS.vi.md)

> Snapshot: **2026-11-26 (Asia/Saigon)**
>
> Audited source: `main` / `origin/main` at `bc1934d48825687de82ea8304138576873497dc4`
> before this documentation refresh
>
> Scope: repository workstreams, architecture remediation, release caveats,
> contract validators, and verified GitHub Actions CI evidence.

Specifications describe required or target behavior; they are not, by themselves,
proof that a production path is complete. See the `Reachability` column in
[`specs/vnext/README.md`](specs/vnext/README.md) to distinguish `default-build`,
`feature-gated`, `library-only`, and `spec-only` contracts.

## Executive status

| Workstream | Current state | What remains open |
|---|---|---|
| Concept MVP (`KU–OBP`, D-044 / D-045 / D-046) | **22 of 24 tasks Merged on `main`; 2 Deferred.** Local private KU authoring/search/restart (`KU-QA-001`), two-node explicit Public KU sharing (`INT-KU-OBP-001`), one-command DEV-ONLY fresh-clone bootstrap (`D-056`), CLI/Web/Desktop `/ku` projections (`onebrain ku share`, `describe_ccids` `D-058`, `share_preview` `D-059`, restart-durable prepared shares `D-062`, private draft prefill `D-057`, opt-in DEV-ONLY private-LAN bind `D-063`), and multi-OS `CLI↔CLI` / `Web↔Web` CI (`ku-dev-demo-scripts.yml`) are merged and green. | Real operator-signed Concept Registry release distribution for contributors; two-physical-machine private-LAN check (`D-063`); formal multi-model holdout qualification (`KU-ENC-003`, `model_qualified=false`); broader semantic fidelity and v2 host activation (`KU-SEM-001`, Deferred — 8 narrow `SEM-F03` bounded repairs merged through `D-069`). |
| Architecture remediation (`D-051`, `ARCH-01..15`) | **13 of 15 items Done on `main`** (`ARCH-01..09`, `ARCH-11..13`, `ARCH-15`): relay outer-auth lifecycle (`ARCH-01`), default-disabled legacy TCP listener (`ARCH-02` / `D-052`), KU journal & Base terminal compaction (`ARCH-03` / `ARCH-15`), read-only legacy KQL freeze (`ARCH-04`), spec Reachability audit (`ARCH-05`), single user-facing `/ku` model & `legacy` gate (`ARCH-06`), reviewed simple claim lowering (`ARCH-07`), `vnext_semantic_index` KU search (`ARCH-08`), host-agnostic `onebrain-node` split with 0 layering bypasses (`ARCH-09`), `experimental` KQL gate + extracted `onebrain-blob-store` / `onebrain-legacy-ku-store` crates (`ARCH-11`), recovery-unit map & untracked sample DBs (`ARCH-12`), and profile-bound Rust tests replacing source-needle checks plus shared Base archive byte facade (`ARCH-13`). | `ARCH-10` (mobile shared runtime) is **Deferred** (`D-054`) after slice 1 (`MOB-03` shared `OBARV002` archive, `0b6d8d0`) and the 6-edge boundary map; `ARCH-14` (consumer-NAT / VPS field hardening) is **Blocked** pending new VPS registration. |
| vNext contracts and foundation | **Complete at repository contract/foundation scope.** `validate_vnext_contracts.py` verifies 99 tasks, 18 ADRs, 37 negative assertions, 56 foundation vectors across 22 domains, 853 normative lines, and 530 local links. | Optional foundation tasks (`RUN-003`, `RIB-001/002`), product defaults, and operator rollout remain separate gates. |
| Product integration P0–P3 and DR-M5 | **Implementation and recorded CI evidence complete.** Runtime ownership, REST/private WebSocket/CLI/Desktop-Web surfaces, resource admission, observability, crash, chaos/fuzz, compaction, rollback, and accepted M5-07 soak evidence are in `main`. | vNext network/discovery/PoMV lanes remain opt-in/default-off; consumer-NAT qualification (`consumer_nat_qualified=false`) remains unclaimed. |
| Base v1 | **Released with owner-approved disclosed exceptions** as signed tag `base-v1.0.0-owner-waiver.1` at `1e0fb2321aee`. On current `main`, lockfile vulnerabilities were patched (`250fd0d`), remaining unmaintained-only advisories triaged (`2551eee`), and the `Base v1 three-OS` qualification candidate workflow is green (`45763665749`). | The strict tag `base-v1.0.0` is absent and the project explicitly does **not** claim `BASE-GATE-V1 qualified=true`. |
| Concept Registry and P5 production | **Production-reference evidence exists under the Base owner waiver** (plus exact-candidate Linux three-host P5 reference `c453f3e` on 2026-09-23). | Source-controlled strict status remains open: Registry vectors retain `production_qualified=false`; P5 retains `provider-document-pending`, `non-linux-platform-lanes-pending`, and `mobile-carrier-mailbox-pending`; operator rollout is not claimed. |
| Mobile | **Partial `BootstrapOnly/Limited` implementation.** `MOB-01..04` foundation/capture, `MOB-03` shared `onebrain-archive` (`OBARV002`, `D-053`), `MOB-05A` signed admission, and Android `MOB-05B` through ABI 13 Local Import are implemented; `validate_mobile_build_contracts.py` passes (98 evidence rows, 91 affected IDs). | `MOB-05B` peer/iOS/provider work, `MOB-05C` full 2.2 GB-class activation, `MOB-04` private KU completion, `MOB-06` AI/tools, `MOB-07` media/lifecycle, physical-device gates, `MOB-08` networking, and `MOB-09` release remain open. `ReadyOffline` is not claimed. |
| M6 active distributed KQL and Outcome/Benefit | **Not opened as the next production milestone.** Existing M3/M4 bounded one-hop KQL and Public UseEvidence capabilities do not close M6. | Active multipath/provider discovery and end-to-end Use -> Outcome -> Benefit flows require their P5/Registry entry gates. |
| M7 / OBT | **Prototype/legacy only; no production economy.** Legacy wallet/OBT modules are gated/labeled as local non-economic projections. | Benefit-based reward policy, ledger/finality vNext, operational wallet, and adversarial production gates. |
| Extension, bot, glasses, and BCI | **Scaffold or research.** | Product implementation, qualification, and, for BCI, sufficient external safety evidence. |

## Base v1 release boundary (historical waiver record + current CI state)

The annotated `base-v1.0.0-owner-waiver.1` tag (audited 2026-09-05) records:

- candidate commit `1e0fb2321aeec04cb711f4259e2bc807e73a35dd`;
- successful three-OS run
  [33529983318](https://github.com/shpy2001gemi/OneBrain/actions/runs/33529983318);
- successful Task 28 run
  [33592716241](https://github.com/shpy2001gemi/OneBrain/actions/runs/33592716241);
- a prebuilt Registry root, P5 root, and uninterrupted 72-hour soak root;
- disclosed exceptions for late evidence assembly, corrected frozen test-target
  names, existing all-features Clippy findings, and dependency-policy triage.

The tag contains a PGP signature; local audits record the tag and its contents
without claiming local PGP key verification where the signer's public key is not
installed. On 2026-11-25 (`ci2`/`ci3`: commits `0eaf719`, `f147232`, `250fd0d`,
`2551eee`), all workspace Clippy `-D warnings` findings on stable `1.100`,
PowerShell CI error propagation, 17 lockfile vulnerabilities + 2 unsound warnings,
and owner-approved P3 triage for 5 unmaintained-only advisories were resolved so
the `Base v1 three-OS qualification candidate` workflow passes end-to-end on
`main` (including the closure job), without claiming a new strict `base-v1.0.0`
release tag.

## Current implementation direction and active backlog

Under owner decisions **D-044** (2026-09-28), **D-050** (2026-10-04), and **D-051**
(2026-10-18):

1. **Single plan and ledger.** [`MASTER_PLAN.md`](handoffs/2026-09-ku-obp-productization/MASTER_PLAN.md)
   governs scope, architecture remediation, and the contributor backlog;
   [`PROGRESS.md`](handoffs/2026-09-ku-obp-productization/PROGRESS.md) owns live
   checkpoint state, task status, and retained branch/worktree inventory.
2. **Bounded contributor increments.** With all 22 non-deferred KU–OBP tasks and
   13/15 `ARCH-*` items merged, ongoing work picks small, testable slices from
   the `MASTER_PLAN.md` backlog—such as narrow host-bounded semantic repairs in
   [`KU-SEM-001`](handoffs/2026-09-ku-obp-productization/outputs/KU_SEM_001_CONTRIBUTOR_BACKLOG.md),
   two-machine private-LAN verification (`D-063`), Desktop `/ku` click parity
   (`D-057`), or contributor setup improvements—while preserving `encode ≠ publish`,
   `proposal ≠ materialize`, `materialize ≠ adopt`, and private Vault boundaries.
3. **Separate gated lanes.** Formal encoder qualification (`KU-ENC-003`), strict
   Base/Registry release closure, consumer-NAT/VPS field hardening (`ARCH-14`),
   mobile (`src/onebrain-mobile/AGENTS.md` + mobile build contract; `ARCH-10`
   deferred), M6 distributed KQL/Outcome/Benefit, M7/OBT, and BCI retain their
   own entry gates.

Detailed mobile sequencing remains in
[`WIP_MOBILE_APP_IMPLEMENTATION_PLAN_V1.md`](research/WIP_MOBILE_APP_IMPLEMENTATION_PLAN_V1.md).
Distributed-runtime history and gates remain in
[`WIP_DISTRIBUTED_RUNTIME_IMPLEMENTATION_PLAN_V2.md`](research/WIP_DISTRIBUTED_RUNTIME_IMPLEMENTATION_PLAN_V2.md).

## Git and retained-work summary

- Earlier pre-productization cleanup on **2026-09-05** (`c65f1739fcd0`) verified
  that all 46 historical Base/DR-M5/P1–P3 local branches were ancestors of
  `origin/main` and retired them locally without deleting remote refs or tags.
- Subsequent September–November 2026 KU–OBP productization and qualification
  preparation created retained local/remote task branches, a clean secondary
  worktree (`.codex/worktrees/3bbf/OneBrain` at `798eabf`, already in `main`),
  and preserved local `KU-ENC-003` preparation scripts/reports (`4a8f29d` +
  untracked `scripts/encoder/qualification_*`). The complete, authoritative
  inventory of retained refs and uncommitted preparation is maintained in
  [`PROGRESS.md`](handoffs/2026-09-ku-obp-productization/PROGRESS.md) — do not
  delete or overwrite retained work.

## Validation evidence for this snapshot (2026-11-26)

- `python scripts/ci/validate_vnext_contracts.py` — **PASS** (99 tasks, 18 ADRs,
  37 negative assertions, 56 foundation vectors/22 domains, 853 normative lines,
  530 local links).
- `python scripts/ci/validate_mobile_build_contracts.py` — **PASS** (11
  authorities, 98 evidence rows, 91 affected IDs, 123 mobile features, 112
  screens, 62 components, 13 patterns, 0 broken links, 0 source-guard failures).
- `python scripts/ci/check_node_layering.py` — **PASS** (`onebrain-node`
  host-agnostic; 0 ARCH-09 bypasses).
- Recent GitHub Actions runs on `main`:
  - `Base v1 three-OS qualification candidate` on `bc1934d`:
    [45763665749](https://github.com/shpy2001gemi/OneBrain/actions/runs/45763665749) — **success** (and `45759117567` on `fc75ff2` — **success**);
  - `vNext foundation contract` on `570761e` (D-069):
    [45760738065](https://github.com/shpy2001gemi/OneBrain/actions/runs/45760738065) — **success** (6/6 jobs);
  - `KU dev demo scripts` (Linux, macOS, Windows two-node `CLI↔CLI` and `Web↔Web`) on `56e5101` (t22):
    [45753417707](https://github.com/shpy2001gemi/OneBrain/actions/runs/45753417707) — **success**.

This snapshot does not claim that every workspace test, physical-device test,
model holdout evaluation, or strict production qualification gate was rerun locally.
