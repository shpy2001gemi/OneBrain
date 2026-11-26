# OneBrain Project Status

> **Active direction — D-044 / D-050 / D-051:** ship and refine the runnable
> open-source concept MVP on top of the shared architecture without turning
> deferred qualification campaigns into blockers for bounded improvements.
> Use the [single delivery plan](handoffs/2026-09-ku-obp-productization/MASTER_PLAN.md)
> for scope/order and [PROGRESS.md](handoffs/2026-09-ku-obp-productization/PROGRESS.md)
> as the sole authoritative task and Git/worktree ledger.

[Tiếng Việt](PROJECT_STATUS.vi.md)

> Snapshot: **2026-11-27 (Asia/Saigon)**
>
> Audited source: `main` after full 9-pillar (`P1`–`P8`, `P10`) Core + Runtime + Software Prod-Readiness integration across 7 dependency waves (`514.7k` total LOC / `461.0k` `src/` LOC, `25` Rust crates, `4,140+` automated tests)
>
> Scope: repository workstreams, architecture remediation, 9-pillar software closure vs. physical/operator qualification gates, contract validators, and CI evidence.

Specifications describe required or target behavior; software implementation and tests verify repository execution, while physical multi-machine, human-reviewer, and operator-custody ceremonies remain separate gates. See [`specs/vnext/README.md`](specs/vnext/README.md) for per-contract `Reachability`.

## Executive status

| Workstream | Current state | What remains open |
|---|---|---|
| Concept MVP (`KU–OBP`, D-044 / D-045 / D-046) | **22 of 24 tasks Merged on `main`; 2 Deferred.** Local private KU authoring/search/restart (`KU-QA-001`), two-node explicit Public KU sharing (`INT-KU-OBP-001`), one-command DEV-ONLY fresh-clone bootstrap (`D-056`), CLI/Web/Desktop `/ku` projections (`onebrain ku share`, `describe_ccids` `D-058`, `share_preview` `D-059`, restart-durable prepared shares `D-062`, private draft prefill `D-057`, opt-in DEV-ONLY private-LAN bind `D-063`), `D-011` v2 lowering (`review_lowering_v2`), and multi-OS `CLI↔CLI` / `Web↔Web` CI (`ku-dev-demo-scripts.yml`) are merged and green. | Real operator-signed Concept Registry release distribution for contributors; two-physical-machine private-LAN check (`D-063`); formal multi-model holdout qualification (`KU-ENC-003`, `model_qualified=false`); broader human-reviewed semantic fidelity (`KU-SEM-001`, Deferred — all 18 mechanical `SEM-F03` repairs merged through `D-075`). |
| Architecture remediation (`D-051`, `ARCH-01..15`) | **14 of 15 items Done on `main`** (`ARCH-01..13`, `ARCH-15`): relay outer-auth lifecycle (`ARCH-01`), default-disabled legacy TCP listener (`ARCH-02` / `D-052`), KU journal & Base terminal compaction (`ARCH-03` / `ARCH-15`), read-only legacy KQL freeze (`ARCH-04`), spec Reachability audit (`ARCH-05`), single user-facing `/ku` model & `legacy` gate (`ARCH-06`), reviewed claim lowering (`ARCH-07` + `D-011` v2), `vnext_semantic_index` KU search (`ARCH-08`), host-agnostic `onebrain-node` split with 0 layering bypasses (`ARCH-09`), **mobile-safe synchronous `onebrain-core` consumed by both `onebrain-node` and `onebrain-mobile-core` with 0 `tokio` / 0 `std::net` (`ARCH-10`)**, `experimental` KQL gate + extracted `onebrain-blob-store` / `onebrain-legacy-ku-store` crates (`ARCH-11`), recovery-unit map & untracked sample DBs (`ARCH-12`), and profile-bound Rust tests replacing source-needle checks (`ARCH-13`). | Only `ARCH-14` (consumer-NAT / live VPS field hardening) remains **Blocked** pending new physical VPS registration. |
| 9-Pillar Core + Runtime + Software Prod-Readiness (`P1`–`P8`, `P10`) | **Complete in software across 7 horizontal dependency waves (`nine_pillar_cross_runtime_suite` 6/6 PASS).** Includes `P1` `ConceptLifecycleTable` + `.obr` v2 collision quarantine + `D-012` chunk pull + Receptor/Affordance/Assembly/Mapping composition; `P2` `RIBLT-1` fast set-sketch + `FrontierSummaryPage` + durable E2EE `obp/mailbox/1` + `obp/blob-chunks/1`; `P3` deterministic `FIND → KnowledgeNeedIr` compiler (`/api/vnext/kql/compile`) + `M6A` active multipath discovery; `P4` `M6B` `Derivation`/`Outcome`/`Benefit` consent publishers + `DeferredEvidenceStaging` + 4D Sybil grouping; `P5` isolated `onebrain-obt` (`u128`, `RewardEvidenceNoticeV2` firewall, `ScopedNullifier`, Shadow Mode default); `P6` `RUN-003` `obp/cognition/1` + `D-013` blind verification leases + 4-tier `PrivateVault` PAM; `P7` 5-plane `OBKG-002` (`/api/vnext/obkg/snapshot`) + `DreamConsolidatorV1` + CCID-grounded KGE; `P8` `OBARV002` `0x0002ns2` & `0x000B..0x000F` (`highest_issued_seq` anti-equivocation floor) + 4-phase Redb recovery + `TombstoneEpochAccumulatorV1` rollover + `CHK-003..006` local GC; `P10` `obp/personal-mesh/1` + `obc_surface1` capability ceilings across API/CLI/Web/Desktop/Mobile and `onebrain-extension`, `onebrain-bot`, `onebrain-glasses`. | Optional network/cognition/ledger lanes remain default-off (`VNextExtendedFeatureFlags`); OBT defaults to `M7.1` Shadow Mode (`0` spendable minting); physical multi-machine, live carrier NAT, and external adversarial audits remain separate field gates. |
| vNext contracts and foundation (`ARCH-10P-001`) | **Complete across all 10 pillars.** `validate_vnext_contracts.py` and `validate_ten_pillar_contracts.py` verify 10/10 pillar specifications (`P1`–`P10`), 10 cross-pillar firewalls (`FW-01`–`FW-10`), 15 acyclic authority edges, 99 foundation tasks, 18 ADRs, 37 + 12 negative assertions, 56 foundation vectors across 22 domains, 985 normative lines, and 708+ local links. | `P9` (`P9-BCI-SAFETY-001`) intentionally remains `spec-only (CI-checked)` at `0` runtime LOC until standardized neural hardware and independent external biosafety certification exist. |
| Product integration P0–P3 and DR-M5 | **Implementation and recorded CI evidence complete.** Runtime ownership, REST/private WebSocket/CLI/Desktop-Web surfaces, resource admission, observability, crash, chaos/fuzz, compaction, rollback, and accepted M5-07 soak evidence are in `main`. | vNext network/discovery/PoMV lanes remain opt-in/default-off; consumer-NAT qualification (`consumer_nat_qualified=false`) remains unclaimed. |
| Base v1 | **Released with owner-approved disclosed exceptions** as signed tag `base-v1.0.0-owner-waiver.1` at `1e0fb2321aee`. On current `main`, lockfile vulnerabilities were patched (`250fd0d`), remaining unmaintained-only advisories triaged (`2551eee`), and the `Base v1 three-OS` qualification candidate workflow is green (`45763665749`). | The strict tag `base-v1.0.0` is absent and the project explicitly does **not** claim `BASE-GATE-V1 qualified=true`. |
| Concept Registry and P5 network | **Software v2 Registry release/pull pipeline and P5 reference evidence complete.** | Source-controlled strict status remains truthful: Registry vectors retain `production_qualified=false` until an operator-signed real release bundle is distributed; P5 vectors retain `provider-document-pending`, `non-linux-platform-lanes-pending`, and `mobile-carrier-mailbox-pending`. |
| Mobile (`MOB-01..09`) | **Software-complete across `MOB-01..09` (`BootstrapOnly/Limited`).** Consumes shared `onebrain-core` (`ARCH-10`) and `onebrain-archive` (`OBARV002`); private KU composition (`MOB-04`), Registry activation/rollback (`MOB-05B/C`), local AI & media lifecycle (`MOB-06/07`), gated transport engine (`MOB-08`), and native release guards (`MOB-09`) pass (`49/49` Rust, `16/16` unit/widget, `14/14` golden, `validate_mobile_build_contracts.py` PASS with `108` evidence rows, `190` affected IDs, `0` deviations). | Physical Android/iOS hardware benchmarks, operator-signed 2.2 GB `.obr` bundle on device, and store signing ceremonies remain open; `ReadyOffline` is not claimed. |

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

## Validation evidence for this snapshot (2026-11-27)

- `python scripts/ci/validate_vnext_contracts.py` & `python scripts/ci/validate_ten_pillar_contracts.py` — **PASS** (10/10 pillars, 10 firewalls, 15 acyclic authority edges, 99 tasks, 18 ADRs,
  37 + 12 negative assertions, 56 foundation vectors/22 domains, 985 normative lines,
  708+ local links).
- `python scripts/ci/validate_mobile_build_contracts.py` — **PASS** (11
  authorities, `108` evidence rows, `190` affected IDs, 123 mobile features, 112
  screens, 62 components, 13 patterns, 0 broken links, 0 source-guard failures, `verify_native_release_guards.py` PASS).
- `python scripts/ci/check_node_layering.py` — **PASS** (`onebrain-node`
  host-agnostic; 0 ARCH-09 bypasses; `onebrain-core` `tokio`-free and `std::net`-free).
- Cross-pillar software verification suites:
  - `onebrain-node --test nine_pillar_cross_runtime_suite` (`6/6`), `onebrain-obt --test p5_obt_economy_suite` (`11/11`), `onebrain-api --test p10_surfaces_api`, `onebrain-web` (`163/163` vitest, `0` `oxlint` warnings), `onebrain-extension` (`9/9`), `onebrain-bot` (`4/4`), `onebrain-glasses` (`7/7`), `onebrain-mobile-core`/`bridge` (`49/49`) + Flutter (`16/16` unit/widget, `14/14` golden) — **PASS**.
- Recent GitHub Actions runs on `main`:
  - `Base v1 three-OS qualification candidate` on `bc1934d`:
    [45763665749](https://github.com/shpy2001gemi/OneBrain/actions/runs/45763665749) — **success** (and `45759117567` on `fc75ff2` — **success**);
  - `vNext foundation contract` on `570761e` (D-069):
    [45760738065](https://github.com/shpy2001gemi/OneBrain/actions/runs/45760738065) — **success** (6/6 jobs);
  - `KU dev demo scripts` (Linux, macOS, Windows two-node `CLI↔CLI` and `Web↔Web`) on `56e5101` (t22):
    [45753417707](https://github.com/shpy2001gemi/OneBrain/actions/runs/45753417707) — **success**.

This snapshot does not claim that every workspace test, physical-device test,
model holdout evaluation, or strict production qualification gate was rerun locally.
