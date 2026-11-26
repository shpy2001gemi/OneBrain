# KU-SEM-001 — contributor backlog

D-044 (2026-09-28): this is the existing detailed semantic defect list, linked
from the single MASTER_PLAN contributor queue. Baseline source is now in main
through `437dba0` / `3216f1d`; the old local-only description below is historical.
Remaining defects are not fixed and do not block the scoped concept demonstration.

Owner accepted the current foundation for continued product development on
2026-09-20 (D-024). These gaps are deferred, not fixed, and do not block unrelated
CLI/OBP framework work. This file is a local handoff; no public issues were filed.

| Item | Reproduction / gap | Completion evidence |
|---|---|---|
| SEM-F01 | Rocket source in task 24: omitted group reference, future scope, Qwen branch/target errors | Source-preserving output, correct qualifier roles, reference or explicit faithful unresolved state; separately assessed broader cases |
| SEM-F02 | Qwen v2 regression on simple OR, condition, water/car roles and units | Original development cases plus independent cases; no loosened assessor or answer leakage |
| SEM-F03 | Automatic structured repair is not routed; atomic guarded patch support exists. 2026-11-25 (D-060): `borrowed_outside_other_branch` / `borrowed_ambiguous` now route to a bounded `alternatives` repair (drop borrowed or another branch's exact token run); 2026-11-26 (D-061): `branch_scope_or_order` now routes to bounded `alternatives` reorder or local non-cue token-run `surface` narrowing when finite choices exist; 2026-11-26 (D-064): `incompatible_or_cue` now routes to a bounded whole-group omission (exact quote stays the bound argument); 2026-11-26 (D-065): comparison `property_binding` / `unexpected_target` now route to a bounded `comparisons` repair (rebind token-extended property to the predicate, omit an unspecified-kind target); 2026-11-26 (D-066): ellipsis `role_binding` now routes to a bounded `ellipses` repair (omit the flagged ellipsis or rebind its surface to a bound token-extended subject/argument); 2026-11-26 (D-067): comparison `target_absent_or_ambiguous` now joins the bounded `comparisons` repair (downgrade to `target_kind: unspecified` and omit the unresolved target); 2026-11-26 (D-068): ellipsis `self_reference` now joins the bounded `ellipses` repair (omit only, argument stays explicit); 2026-11-26 (D-069): comparison `explicit_target_scope` now joins the bounded `comparisons` repair (same downgrade to `target_kind: unspecified` and omit the out-of-scope target); 2026-11-26 (D-070): comparison `cue_ambiguous` now joins the bounded `comparisons` repair when finite local choices exist (narrow `cue` to a whitespace-bounded token run of the emitted `cue` resolving uniquely in the claim scope); 2026-11-26 (D-071): alternatives `cue_ambiguous` now routes to a bounded `alternatives` repair when finite local choices exist (narrow `group.cue` to a whitespace-bounded token run of the emitted cue resolving uniquely inside `group.quote` outside located branches); 2026-11-26 (D-072): references `cue_ambiguous` now routes to a bounded `references` repair when finite local choices exist (narrow `via.quote` to a whitespace-bounded token run of the emitted `via.quote` resolving uniquely in the claim scope outside the resolved target); 2026-11-26 (D-073): alternatives `argument_binding` now routes to a bounded `alternatives` repair when finite local choices exist (rebind `group.quote` to a whitespace-bounded token-extended bound `argument` of the claim preserving `cue`/branch/`borrowed` resolution and coverage); 2026-11-26 (D-074 slice 1): bound ellipsis `absent_or_ambiguous` now joins the bounded `ellipses` repair when the bound role resolves uniquely (omit only, role stays explicit); 2026-11-26 (D-074 slice 2): references `target_absent_ambiguous_or_self` now joins the bounded `references` repair when finite local choices exist (narrow `to.quote` to a whitespace-bounded token run of the emitted `to.quote` resolving uniquely outside `via` to one non-cyclic target of `target_kind` in another claim); other structured issues remain unrouted | Source-local finite choices, cardinality/provenance/coverage preservation, rejected proposals retained, bounded lifecycle tests |
| SEM-F04 | Worker startup intermittently failed on Qwen; six-case retry had none | Isolated reproducible worker lifecycle diagnosis; preserve raw failures and retry as distinct jobs |
| SEM-F05 | Nested alternatives, inner-term qualifier scope, cross-window reference incomplete | Versioned contract first; bounded representation and compatibility tests |
| SEM-F06 | Independent semantic/factual verification and canonical lowering absent | Separate approved contracts and explicit acceptance/save boundaries; no draft-to-KU shortcut |
| SEM-F07 | Model/device qualification and wider portability unproven | KU-ENC-003 controlled holdout process and resource evidence; known cases do not qualify a model |

Implementation locations: `src/ku-encoder/src/extraction/semantic_selection_v2.rs`,
its `tests.rs`, shared `review_draft.rs`, `src/onebrain-node/src/ku_product/`,
and `src/onebrain-web/src/pages/KuDraftClaims.tsx` in the original repository.
Public development sources and assessors are under `scripts/encoder/`.
[Implementation evidence](KU_SEM_001_IMPLEMENTATION.md) records report filenames,
commitments, checks and limitations. Private raw traces remain outside the repo
under `OneBrainLocal/development-reports`; do not publish them or secrets by default.

Contributors should preserve private draft versus canonical KU, exact source,
uncertainty, consent, job budgets and reproducible executor commitments. Changes
require focused tests and `python scripts/ci/validate_vnext_contracts.py`; regenerate
the reviewed bundle when bound code changes. Current live host stays on v1 until
its separate activation gate is satisfied. No more tuning is required to begin CLI.
