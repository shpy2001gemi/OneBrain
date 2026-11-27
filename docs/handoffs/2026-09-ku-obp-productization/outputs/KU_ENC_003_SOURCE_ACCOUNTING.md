# KU-ENC-003 source export and accounting — 2026-09-27

Task remains **Blocked**, `model_qualified=false`. Exact private source export
is now implemented and verified. Durable scheduling/accounting and an operational
reducer are implemented and tested on synthetic data. The complete native
inference, measurement and independent evaluation harness is still unfinished.

## Preserved workspace and authority

Original tree: `C:/Users/shpy2/Documents/OneBrain`, branch
`codex/ku-enc-003-handoff`, HEAD `689efd546e7c4f8e41c9f9f9ff3243b63c979908`.
All incoming local handoff files and the preparation auditor/tests were retained.
The auditor source still matches its prior private audit commitment. Local
main/origin-main remain `689efd5`; Desktop and qualification refs remain
`bf3edb1` and `4a8f29d`. Remote refs were not queried. No switch/reset, commit,
push, merge or branch deletion occurred.

Read the requested handoff/ledger/decisions/task/resume/recovery/offline reports
and the four historical files through `git show`. Reread applicable extraction
workflow/source/budget/qualification and machine profile/Attempt contracts,
AI-003 source custody, CAP-001/002, FID-001/002/003, KU-ENC-001/002 evidence and
research evaluation design. Inspected current source-port and Qwen tokenizer/
managed-worker seams. This is not a claim that all public corpus/schema files
or mobile boundary documents were reread in this continuation; the previous
full-read checkpoint remains historical. No runtime or canonical spec changed.

## Private exact-byte export

[Exporter](../../../../scripts/encoder/qualification_source_export.py) reads the
pinned workbook snapshot, checks bounded archive/sheet dimensions and text cells,
and exports only `Nguon.source_text`. Formula/nontext/missing-ID cells fail.
It does not evaluate formulas, inspect annotation rows or export labels.

Each source binds the original workbook digest, sheet and cell coordinates,
source-ID digest, language, exact UTF-8 byte length/hash and the separate old
NFC comparison hash. There is no trimming, Unicode normalization, newline
conversion or added BOM. The transformation records the OOXML decoder/library,
Python/Unicode versions and exporter bytes. “Original” means the decoded
workbook cell, not independently attested bytes from before workbook creation.

The verifier reopens the pinned workbook and checks **exact** cell bytes,
coordinates and metadata, not just NFC equality. Rehashed normalization or a
substituted coordinate fails. Outputs are exclusively created, fsynced and
complete only when the final `completion.json` exists. They remain mutable
filesystem artifacts protected by commitments, not an immutable run lock.

Final private export:

`C:/Users/shpy2/.codex/ku-enc-003-evidence/source-export-20260927-02`

- `sources/`: **200** exact UTF-8 files, **100 EN / 100 VI**.
- `source-manifest.json`, `transformation.json`, `completion.json`: private bindings.
- `custody-request.json`: unresolved custodian/permission/exposure/governance
  fields, no SourceArtifacts and **zero issued grants**. This request cannot
  satisfy native `Authorized` capture or principal/process/dataset access.

The first export `source-export-20260927-01` is preserved. Native ACL inspection
found Python's Windows mkdir had explicit Administrators/OWNER RIGHTS defaults.
The helper was corrected to replace the DACL and verify exactly owner/SYSTEM
before private writes. ACLs on the three directories created earlier in this
turn were restricted without changing source/evidence bytes. An initial
PowerShell Set-Acl helper failed to autoload its security module; the final
helper uses the Windows PowerShell .NET ACL API and passes a native regression.
The final export's **206 files/directories have zero unexpected access rules**.
This is preparation-directory access control, not reviewer independence or
worker/evaluator sandbox qualification.

Final exporter CLI exited **2**, meaning successful export with execution
blocked. The first wrapper invocation reported PowerShell exit 1 despite the
successful completion marker; its record is preserved. The final invocation
explicitly propagates `$LASTEXITCODE` and independent verification succeeds.

Final private assembly verification:

`C:/Users/shpy2/.codex/ku-enc-003-evidence/assembly-20260927-02/report.json`

It binds the eight preparation implementation/test files, analysis draft and
export completion. It reverified **13 input/contract pins and 18 bundle
artifacts**, with no workbook or intake/draft modification. The preceding
`assembly-20260927-01/report.json` remains preserved. Native Registry verify/
status from the previous checkpoint was **not rerun**; current opaque artifact
pin verification does not create fresh signature or source-scope authority.

## Durable metadata accounting and operational reduction

[Attempt ledger](../../../../scripts/encoder/qualification_attempt_ledger.py)
creates a full source × tuple × three-repeat matrix in private SQLite before
accounting begins. Exclusive creation, `BEGIN IMMEDIATE`, `synchronous=FULL`,
plan/profile/source bindings and matrix checks prevent accidental overwrites,
omitted rows and budget renewal through ordinary API use. Reservations commit
before return. Context/aggregate call/token/work ceilings come from the current
machine profile; no-LLM cannot reserve model calls and the experimental 600s
profile is rejected.

Failure/cancellation retains reservations. Restart marks active rows interrupted
with unknown elapsed time and rejects late callbacks/resampling. A late success
becomes timeout. A second active row and a second outstanding call are rejected.
The next row remains fenced until a separate worker-stop evidence reference is
recorded; that digest alone is explicitly not verification of OS termination.

This is an **outer metadata ledger**, not the native encrypted Attempt/KU
journal. It has no provider or permission interface. It has not been wired into
native source admission, per-stage accounting or a worker collector. SQLite
hash metadata is private but is not encrypted/authenticated native attempt
storage. No real holdout attempt matrix was created with placeholder tuples.

[Operational reducer](../../../../scripts/encoder/qualification_analysis.py)
rejects missing/duplicate/changed matrix rows, counts failures and pending rows,
aggregates charged budgets and separates reported-complete versus all-terminal
nearest-rank p50/p95 timings. Three repeats still have one source row, whose
independence remains unverified. It cannot infer acceptable complete KU,
precision/recall, semantic/CID agreement or qualification from encoder status.

[Analysis draft](KU_ENC_003_ANALYSIS_DRAFT.md) specifies denominators, pairing,
clustered uncertainty proposal, independent grading/FID and measurement boundaries.
It is **not** a preregistration or acknowledged lock. The statistical bootstrap
and native independent grading/FID bridge remain unimplemented.

## Fresh verification

From the repository root, bundled Python:

```text
python -X utf8 -m unittest scripts.encoder.test_qualification_source_export scripts.encoder.test_qualification_attempt_ledger scripts.encoder.test_qualification_analysis scripts.encoder.test_qualification_package_audit scripts.encoder.test_contract
python -X utf8 -m scripts.encoder.generate_bundle --check
python -X utf8 scripts/ci/validate_vnext_contracts.py
```

- **85 tests pass**: 21 source-export (including real Windows DACL), 22 ledger,
  eight operational reducer, 16 retained preparation auditor, 18 contract.
- Ledger regression includes an actual synthetic Python child exiting 73 after
  committed reservation; reopening retains charges and records interruption.
  This is not a model-worker fault or resource measurement.
- Eight generated artifacts unchanged; aggregate vNext validator passes,
  including 48 encoder cases/two jobs and 529 specification links.
- Exact export re-verification: 200 sources; final private ACL check passes.
- `git diff --quiet HEAD -- src docs/specs` and `git diff --check` pass.
  Application runtime and canonical contracts remain unchanged.
- Final handoff check: **228 local links across 11 Markdown files** resolve;
  changed/untracked Markdown and Python files have no trailing whitespace.

## Concrete next work

1. Keep the final export and old intake/draft. Do not repeat workbook discovery
   or replace their NFC commitments; bind the separate exact export into the
   future finished package. Custody needs actual processing permission,
   exposure/provenance/grouping/near-duplicate review and governance assessment
   before isolated native SourceArtifact/grant installation.
2. Complete the isolated native host using `SharedKuExtractionInputs` and the
   fixed qualification workflow constructor. Wire this outer ledger to native
   encrypted checkpoints and actual collector evidence. Preserve whole-source,
   unit/ambiguity and draft-versus-canonical limitations in the denominator.
3. Finish the second-family exact tokenizer/worker integration. Current
   `ManagedOllamaProvider` still admits only Qwen3; its GGUF tokenizer is Qwen
   BPE-specific. Do not substitute a Gemma tag or the development chat adapter.
   No current model/worker/build tuple was inventoried or admitted this turn.
4. Implement and bind actual process-tree/host memory, stage/total time,
   cancellation/OOM/process-death collectors, declared cold/warm lanes and
   independent evaluator/FID/statistical reducer. No real measurements may be
   obtained through pre-lock probes or warm-up calls.
5. Obtain actual reviewer identities, competence/independence evidence,
   approved labels/rubric/alternatives and FID plan. After engineering/build/
   tuples/analysis are complete, lock everything and obtain acknowledgement
   of that exact digest before any qualification model call.

No tuning, live-host mutation, model call, mobile or remote-host action occurred.
KU-QA-001/INT-KU-OBP-001 stay Planned; model/platform/NAT limits remain. Preserve
OBP default-off, rollback/legacy data and staging/runs/durable state. Linux P5
remains qualified only for `c453f3e`; D-010 still governs later merge/deletion.
