# KU-SEM-001 — contributor backlog

Owner accepted the current foundation for continued product development on
2026-09-20 (D-024). These gaps are deferred, not fixed, and do not block unrelated
CLI/OBP framework work. This file is a local handoff; no public issues were filed.

| Item | Reproduction / gap | Completion evidence |
|---|---|---|
| SEM-F01 | Rocket source in task 24: omitted group reference, future scope, Qwen branch/target errors | Source-preserving output, correct qualifier roles, reference or explicit faithful unresolved state; separately assessed broader cases |
| SEM-F02 | Qwen v2 regression on simple OR, condition, water/car roles and units | Original development cases plus independent cases; no loosened assessor or answer leakage |
| SEM-F03 | Automatic structured repair is not routed; atomic guarded patch support exists | Source-local finite choices, cardinality/provenance/coverage preservation, rejected proposals retained, bounded lifecycle tests |
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
