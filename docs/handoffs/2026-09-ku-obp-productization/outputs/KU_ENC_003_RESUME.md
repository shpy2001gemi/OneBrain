# KU-ENC-003 continuation map

Read-only reconstruction on 2026-09-27 from main `689efd5` and retained
qualification tip `4a8f29dde9803355f6251d6f266f40e3aa43c958`.
This is a handoff map, not new implementation or model qualification evidence.
Task status is maintained in [PROGRESS](../PROGRESS.md).

Latest [offline preparation](KU_ENC_003_OFFLINE_PREPARATION.md) adds the local
preparation auditor/tests, completes required reading and verifies the recovered
Registry through native code. Inputs still lack reviewer evidence and final
source/run bindings. Keep this result and code; do not repeat workbook discovery
or interpret successful integrity checks as execution admission. The older
missing-location/Registry-unverified statements below retain their original
checkpoint scope and are superseded by recovery and this result.

Latest [autonomous recovery](KU_ENC_003_INPUT_RECOVERY.md) resolves the earlier
missing workbook paths and creates a private intake plus unlocked run draft.
Use its actual paths and remaining engineering/reviewer work when resuming.

Follow-up on the same date: [input and harness preparation](KU_ENC_003_HARNESS_PREPARATION.md)
records the actual read-only comparison with current code, public conformance
checks and a concrete metadata intake checklist. The old preflight's missing
tokenizer/worker integration is historical: mechanisms now exist for Qwen on
Windows, while qualification measurements and a complete execution/grading
harness still do not. No private corpus/reviewer/run-lock references were
provided during this continuation; keep `Blocked` and `model_qualified=false`.

## Verified repository checkpoint

- The original working tree was clean on main before this documentation update.
  HEAD, main, origin/main and the remote main advertised by `git ls-remote`
  were `689efd546e7c4f8e41c9f9f9ff3243b63c979908`.
- D-043 merge `6449446` is an ancestor of origin/main. The retained remote
  Desktop branch is `bf3edb1ef247a74fc9fc8ed6b85c6516e213861b`.
- Both local and remote `codex/ku-enc-003-model-qualification` are at
  `4a8f29dde9803355f6251d6f266f40e3aa43c958`.
- This handoff update uses `codex/ku-enc-003-handoff` in the original working
  tree, based on `689efd5`. Preserve its local changes when resuming. It does
  not move the retained qualification branch or authorize merging it.

## Recover the existing work without switching branches

The following files exist at the retained qualification commit and are absent
from main. Read them with `git show`; do not replace current handoff files with
the old branch's README, DECISIONS or PROGRESS.

```text
git show 4a8f29dde9803355f6251d6f266f40e3aa43c958:docs/handoffs/2026-09-ku-obp-productization/outputs/KU_ENC_003_QUALIFICATION.md
git show 4a8f29dde9803355f6251d6f266f40e3aa43c958:docs/handoffs/2026-09-ku-obp-productization/outputs/KU_ENC_003_DATA_GUIDE.vi.md
git show 4a8f29dde9803355f6251d6f266f40e3aa43c958:scripts/encoder/qualification_preflight.py
git show 4a8f29dde9803355f6251d6f266f40e3aa43c958:scripts/encoder/test_qualification_preflight.py
```

The September 6 report records zero inference calls, zero holdout runs and
zero qualified tuples for that qualification checkpoint. Later experimental
Web/model development results remain development evidence. The historical
report's instruction not to advance to KU-API-001 is superseded by D-021 and
the subsequent merges; its missing qualification inputs remain relevant.

The private historical inventory path is
`C:/Users/shpy2/.codex/ku-enc-003-evidence/preflight-20260906.json`.
Its existence was checked on September 27; its contents and hashes were not
revalidated. Preserve it. Artifact bindings and Ollama version from September 6
must be measured again for a future candidate, not reused as current evidence.

## Inputs required before qualification execution

The accepted [framework section 7](../../../specs/vnext/KU_EXTRACTION_FRAMEWORK_PROFILE_V1.md#7-conformance-and-model-qualification)
and [machine thresholds](../../../specs/vnext/ku-encoder-v1/profile.json)
remain the gate. The task's [full required read set](../tasks/23-KU-ENC-003.md)
must be completed before implementation or execution.

| Input or work | What the next conversation must establish |
|---|---|
| Private corpus location and custody | Owner-provided local paths and permission for the new VI/EN workbooks; keep source text and labels outside public handoff files and development prompts. Their contents have not been inspected here. |
| Independence and labels | Source grouping/split, reviewer identity and provenance, acceptable interpretations, unsupported cases and the applicable blind-fidelity evidence. AI-assisted guide examples and earlier development workbooks are not blind evidence. |
| Locked run package | Commitments for sources, split, labels/rubric, evaluators, thresholds, statistical analysis, exact candidate, Registry, model/tokenizer/backend and device/resource tuple, fixed before inference. |
| Candidate compatibility | Compare the old preflight code with current shared extraction/node/provider interfaces. Retain both histories; do not merge old status documents over current decisions or assume September 6 bindings still apply. |
| Execution harness | Authenticated source/Registry admission, exact tokenizer and worker build, bounded worker lifecycle, actual cancellation/OOM/process-death controls and measured resource accounting. The old runner only inventories artifacts. |
| Measurement | Two model families, no-LLM baseline, repeated vi/en holdout, supported/adversarial cases, constrained-memory lane, complete-source denominators and unchanged acceptance thresholds. |

The next useful action is to resolve the private corpus/evaluator/locked-run
inputs and prepare the current-candidate harness within task 23. Do not start
model runs with missing entry gates. Thresholds, scope and semantic tuning are
not changed by the owner's instruction to continue and update the handoff.
If required inputs remain unavailable, report exactly which input is missing
and keep the task Blocked; do not substitute a downstream acceptance claim.

## Boundaries carried forward

KU-DESK-001 is merged under D-043. Its Windows lifecycle evidence is 8/8 default
and 10/10 feature-enabled, with feature build and vNext validator passing on
the merged tree. These checks were not rerun during this documentation update.
The Desktop library test executable has the recorded
`STATUS_ENTRYPOINT_NOT_FOUND` host limit; live WebView, real OS sleep and
macOS/Linux lifecycle remain unqualified.

`model_qualified=false`; KU-QA-001 and INT-KU-OBP-001 have no acceptance claim.
OBP-MIG-001 remains merged under D-042, retaining rollback and legacy data.
OBP-QA-001 remains functionally accepted with `consumer_nat_qualified=false`.
The Linux three-host P5 result applies only to candidate `c453f3e`.
Preserve old staging/runs and durable state. Networking stays default-off;
no tuning, mobile implementation or remote host action is included.

Git merge and branch deletion still require explicit owner direction under
D-010. The retained Desktop and qualification branches remain available.
