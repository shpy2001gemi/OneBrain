# KU-ENC-003 offline preparation — 2026-09-27

Task remains **Blocked**, `model_qualified=false`. This continuation adds an
executable preparation audit and fresh native Registry verification. It does
not complete the inference, measurement or independent grading harness.

## Preserved checkpoint and reading

Work stayed in the original tree on `codex/ku-enc-003-handoff`, HEAD `689efd5`.
All eight incoming modified/untracked handoff files remain. Local main and
origin/main still resolve to `689efd5`; retained Desktop and qualification
branches remain at `bf3edb1` and `4a8f29d`. No fresh remote-ref check, switch,
reset, commit, push, merge or branch deletion occurred.

Read AGENTS, NEXT_CONVERSATION, README, PROGRESS, DECISIONS, MASTER_PLAN, task 23
and the resume/recovery/preparation maps. Read the four historical reports and
runner/test files with the resume map's `git show` commands; none was restored.
Completed task 23's required reading before adding the preparation utility:

- KU-ENC-001 contract evidence, framework and machine profile, full schema and
  six projections, both prompts, examples, manifest, all 48 public corpus rows
  and both jobs (repeated corpus structures also inspected as lossless deltas);
- KU-ENC-002 implementation evidence and framework research/evaluation design;
- CAP-001 capability ownership, CAP-002 local manifest/conformance, FID-001,
  FID-002 and FID-003 authority profiles;
- mobile plan sections 6.4–6.7 and MOB-06/MOB-07 for provider, supply-chain and
  physical-device benchmark boundaries only. No mobile implementation/evidence
  was changed or qualified.

## Executable offline preparation audit

[qualification_package_audit.py](../../../../scripts/encoder/qualification_package_audit.py)
reads the existing private draft/intake format and streams pinned files through
SHA-256. Workbook files are treated as opaque bytes; no sheet values, source
text or expected meanings are printed or sent to a model. It checks:

- exact sizes/digests of recovered inputs and all current bundle artifacts;
- source/label commitment counts, matching unique IDs and language totals;
- declared development overlap, split and missing reviewer metadata;
- local allowlisted paths, bounded strict JSON and private exclusive-create
  output, with redacted CLI errors;
- explicit refusal of a draft claiming `locked=true` or qualification, and
  refusal to treat populated reviewer/grant fields as verified authority.

The command has no network, provider, worker or model-dispatch interface and
always reports `execution_admitted=false`. Exit **2** means a successful audit
with execution blocked; exit 1 means an invalid input/output operation. A
successful integrity check cannot authorize inference. This is a preparation
component, not implementation of all twelve scenarios in the earlier map.

Private results (outside Git; earlier files preserved):

- `C:/Users/shpy2/.codex/ku-enc-003-evidence/preparation-audit-20260927-01.json`
  is the initial audit; `preparation-audit-20260927-02.json` additionally binds
  the final auditor source SHA-256.
- The final audit reverified **13 private/contract pins and 18 bundle artifacts**.
  Declared source counts remain **100 VI / 100 EN**, with **200 missing reviewer
  fields**. Three repeats imply 600 planned source/repeat rows per tuple before
  additional adversarial sets; no attempt rows or model calls were created.
- Intake's `source_text_sha256` hashes NFC-normalized text for duplicate checks.
  It is **not an exact original UTF-8 source-export commitment**. The workbook
  byte hashes are exact, but an immutable exact-text export/transformation record
  is still needed before source admission. No workbook or old intake was changed.

Reproduce from repository root, using a new output filename each time:

```powershell
python -X utf8 -m scripts.encoder.qualification_package_audit `
  --draft C:/Users/shpy2/.codex/ku-enc-003-evidence/intake-20260927T004431004208Z/run-package.draft.json `
  --input-root C:/Users/shpy2/.codex/ku-enc-003-evidence `
  --input-root 'C:/Users/shpy2/Documents/OneBrain KU data' `
  --input-root C:/Users/shpy2/.codex/outputs/01a075c0-0712-7bd1-9946-2470f31c85f5 `
  --input-root C:/Users/shpy2/Documents/OneBrainLocal `
  --output C:/Users/shpy2/.codex/ku-enc-003-evidence/preparation-audit-NEW.json
```

## Native Registry result

Built the existing `concept_registry_release` example with
`cargo build --offline --locked -q -p ku-core --example concept_registry_release`
from `src`. Ran its read-only `verify` and `status` commands against recovered
release `owner-local-20260907-v1` using the public key configured in host.json.
Both exited **0**. The native verifier checks the configured signer, Ed25519
signature, artifact/source roots, complete file set, lengths and hashes, SBOM,
packaged Registry/indexes and source/manifest bindings.

Private stdout/stderr, binary/source/lockfile commitments and before/after pins:
`C:/Users/shpy2/.codex/ku-enc-003-evidence/registry-readonly-20260927T022714699406Z/report.json`.
All eight checked host/Registry files were unchanged. Only the configured public
key was copied into the new evidence folder; no key/token secret file was read.
No activation, rollback, host-service request or state mutation occurred.

This resolves fresh signature/integrity verification under the existing local
trust configuration. It does not supply source grants, qualification-scope
approval, authenticated unit metadata or reviewer authority. The earlier draft
audit's Registry blocker is historical to that audit and deliberately not
rewritten; bind this separate verification evidence into the future package.

## Verification and next work

- `python -X utf8 -m unittest scripts.encoder.test_qualification_package_audit scripts.encoder.test_contract`:
  **34 pass** (16 new synthetic preparation tests + 18 public contract tests).
  Tests include same-size tampering, stale intake, source/label mismatch,
  duplicate IDs, development split/overlap, forged lock/reviewer flags, path
  escape, JSON bounds, private-output preservation and redacted CLI errors.
- Generated bundle check: **8 artifacts unchanged**. Aggregate vNext validator:
  **PASS**, including 48 encoder cases, two jobs and 529 specification links.
- Native Registry example build and both read-only commands: **PASS**.
- Handoff checks: **214 local links resolve**; `git diff --check` and a separate
  whitespace check including untracked files pass. `git diff --quiet HEAD --
  src docs/specs` passes; application runtime and canonical specs are unchanged.
- No model inference, holdout semantic grading, worker fault drill or resource
  measurement ran. No application runtime or canonical specification was edited.

Concrete remaining external inputs are custodian/reviewer evidence, not workbook
paths: processing permission and exposure history; independently checked source
provenance/grouping/near-duplicates; reviewer identity, competence and evidenced
administrative-principal/pipeline lineage; reviewed labels, rubric, acceptable
alternatives and applicable FID plan. Keep their contents in private evaluator
storage. A reviewer must later acknowledge the exact **finished** package digest;
this report and a filled spreadsheet do not manufacture that acknowledgement.

Agent-owned next work: exact source export and isolated custody/grants; two-family
tokenizer/worker integration; durable attempt/failure accounting; actual memory,
latency and cancellation collection; separate evaluator/reducer integration and
preregistered source-level analysis. Then bind candidate/build/Registry/tuples,
corpus/split/labels/thresholds and reviewer evidence into one immutable run lock.
No schema probe, warm-up, ablation or other qualification model call precedes it.

KU-QA-001 and INT-KU-OBP-001 remain Planned with unmet dependencies. Keep model,
platform and NAT limits, OBP default-off, rollback/legacy data and staging/runs/
durable state. Linux P5 qualifies only `c453f3e`. No tuning, mobile, remote-host
operation or Git publication is included. D-010 governs any later merge/deletion.
