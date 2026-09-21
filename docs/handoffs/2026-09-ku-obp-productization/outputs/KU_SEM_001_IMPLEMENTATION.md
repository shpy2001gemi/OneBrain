# KU-SEM-001 — Private semantic draft v2 implementation and evidence

Date: 2026-09-20. Owner approved the
[representation proposal](KU_SEM_001_REPRESENTATION_PROPOSAL.md).
Source workspace: `C:/Users/shpy2/Documents/OneBrain`, branch
`codex/ku-web-001-workflow`; includes substantial pre-existing uncommitted work.

Status: shared implementation added; **production activation has not passed its
semantic gate**. Draft extraction is not independent verification or canonical KU.

## Implemented surface

- [Shared encoder](../../../../src/ku-encoder/src/extraction/semantic_selection_v2.rs)
  and [closed schemas/prompts](../../../specs/vnext/ku-semantic-selection-v2/selection.schema.json)
  implement `ku-semantic-selection/2.0` → `ku-semantic-draft/2.0`.
- OR groups retain ordered source branches and explicit/reconstructed provenance.
  Host checks group/argument binding, branch order, exact source scope and borrowed
  material in another branch. It never turns a disjunction into two asserted facts.
- Ellipsis, term/group/claim reference and comparison are separate fields. Target
  IDs and UTF-8 spans are host derived and local to each window/revision. Reference
  dependency cycles and ambiguous targets require review. Inferred or unspecified
  comparison targets stay visibly distinct from explicit source content.
- Role/cue evidence uses separate spans. Reference targets and borrowed provenance
  do not expand semantic coverage or masquerade as local exact quotes. Missing
  choice/comparison structure and qualifier overlap produce review hints, not
  host-selected semantic relationships.
- Quantity grammar excludes output when there is no supported numeric token.
  Host also rejects unsupported full numeric syntax and substring reinterpretation.
  Common VI/EN number-word cues produce an unassessed issue without converting them
  to digits; they can have false positives and are not a complete language detector.
- Existing claim-bound repairs continue through common Rust. Structured patches
  are atomic and protect group/branch cardinality, source and provenance. Automatic
  scheduling remains limited to mechanically identifiable legacy fields; unresolved
  structured meaning is not repaired by regenerating the source.
- Source/call/token/time/checkpoint limits remain bounded. Additional 256-node
  admission is checked across retained job windows, before assembling a new graph.
- [Node](../../../../src/onebrain-node/src/ku_product/review_jobs.rs) uses the same
  encrypted durable job lifecycle. An explicit host-only `with_selection_v2()`
  gate controls new jobs; the local example exposes it through `KU_SELECTION=2`.
  The default remains v1 while semantic activation is not accepted.
- [Web renderer](../../../../src/onebrain-web/src/pages/KuDraftClaims.tsx) displays
  OR branches, reconstruction, proposed references and comparison uncertainty,
  including historical revisions. Unknown draft profiles are not interpreted as
  legacy fields. Source and job status remain readable. No save/share path is added.

The [v2 contract](../../../specs/vnext/KU_SEMANTIC_SELECTION_PROFILE_V2.md) explicitly
keeps nested alternatives, inner-term qualifier scope and unresolved cross-window
references out of complete extraction in the initial surface. Independent verifier,
canonical lowering, publication and mobile implementation remain separate work.

## Compatibility and build behavior

V1 records are not rewritten. Read/list/cancel do not require the old executor to
be available. Shared lifecycle edits change executable commitments: an old v1
commitment cannot resume under changed code. Tests retain original serialized
records and verify rejection without changing their budget or revision bytes.
There is no hard-coded old hash pretending to authorize the new implementation.

The running user host is PID 38668 at `http://127.0.0.1:4280/ku`. It was not stopped
or restarted. Building its occupied executable hit Windows file locking; a second
Cargo example target `ku_local_web_staging` now compiles the exact same source
under a separate executable name. The staging build passed and was not launched
against the user's dataset. The existing signed Registry, keys and jobs were
preserved. A read-only host check returned HTTP 200, Registry/encoder ready and
catalog `qwen3:8b`, with `model_qualified=false`.

## Mechanical checks

- Encoder extraction suite: 70 passed, one opt-in owned-worker test ignored.
  Includes 16 new v2 tests for rocket structure, inferred targets, numeric syntax,
  branch/source loss, quote scope, cyclic/cross-window reference, node budgets,
  unknown fields through closed schemas, cancellation and old commitments.
- API background job suite: 8 passed, including v2 restart/read/idempotency,
  cancel fencing, explicit resume and retained charged budget/revisions.
- Web: 22 tests passed, including visible inferred comparison and unsupported
  profile handling; production build passed.
- Post-run assessor: 4 tests passed. Mechanical state alone cannot make an
  incorrect interpretation pass; flattening and borrowed-predicate failures stay red.
- Generated schemas are checked byte-for-byte by the aggregate vNext validator.
  The reviewed encoder bundle was regenerated after implementation changes;
  aggregate validator and whitespace checks passed.

These checks establish bounded mechanics and compatibility behavior. They do not
establish model fidelity or factual accuracy.

## Development runs

Raw reports are private under `OneBrainLocal/development-reports`; they retain
prompts, schemas, raw replies, host issues, rejected proposals and commitment.
Only source text and structural instructions go to the model; post-run assessment
answers are separate. No locked VI/EN holdout was opened or used.

| Qwen3 8B native development iteration | New known cases | Mean seconds | Outcome |
|---|---:|---:|---|
| Initial v2 schema serialization | 0/8 meaning checks | 54.78 | Failed; schema property order was lost and model emitted badly scoped fields. |
| Preserved schema order and fuller generic examples | 2/8 meaning checks | 59.59 | Explicit comparison and conjunction passed; rocket/reference and other errors remained. |

Files: `20260920-semantic-v2-initial-qwen8b.jsonl` and
`20260920-semantic-v2-ordered-qwen8b.jsonl`. The second iteration's apparently
clear simple OR draft borrowed the predicate into an already complete branch;
its number-word draft did not expose unresolved numeric meaning. Both are counted
as failures. New host checks flag those defects; raw historical results are not
rewritten or reclassified as successes.

Final-candidate run: `20260920-semantic-v2-final-qwen8b.jsonl` completed 13 source-only
development cases (8 new + 5 existing regression cases), with **2/13** inspected
meaning checks passing: explicit comparison and negation/contrast. Six cases had
at least one `worker_startup_failed`, including failed repair calls with a retained
earlier draft. These transport failures are not treated as semantic evidence.
The run's commitment was
`3711d62cced8df4237c11e7bf10c62f9cd2ee45ee25384b0810a9cfad732da09`.
Separate retry report: `20260920-semantic-v2-retry-qwen8b.jsonl`; original jobs,
charged budgets and raw failed records are preserved. The read-only post-run
assessments for the completed original iterations and follow-up runs are in
`20260920-semantic-v2-assessment.json`, with report hashes and commitments.

The six-case Qwen retry completed with **1/6** meaning checks passing and **zero
transport errors**. The passing number-word case faithfully retained unresolved
numeric meaning. OR reconstruction, ambiguous reference, conditional scope and
the water/car regressions still failed. This isolates semantic failures from the
intermittent worker failures; the retry does not replace the original results.

The separate Gemma 4 12B development chat run completed **2/3** checks: simple OR
and explicit comparison passed; the rocket/reference case failed. Report:
`20260920-semantic-v2-gemma12b.jsonl`, commitment
`528f8c1d190e84529b56816b4b5ee32732a70dbf8d2cf6ab84db2f929b2f4892`.
This small known-case comparison is neither model qualification nor Web admission.

After that candidate, a negative test and host issue were added for a known
conjunction cue (`và`/`and`) being placed inside an OR group. This does not infer
an alternative or define arbitrary-language connective semantics. It changes the
executor commitment; earlier model reports remain evidence of their recorded
executor, not the final source. The retained AND failure also demonstrates that
schema success must not be interpreted as independent semantic verification.

The Gemma rocket result exposed a missing dispatch for the new predicate/qualifier
overlap issue. A subsequent bounded predicate repair now offers only the original
predicate and exact substrings obtained by trimming a selected edge qualifier at
a whitespace boundary. Decoder and host enforce the same choices; the qualifier
is retained, and no-op/out-of-choice/coverage-loss proposals are rejected. A new
test covers dispatch, wire choices, accepted repair and adversarial proposals.
This changes the executor commitment again; the above model runs precede this fix.

The final-source Gemma rocket replay used two calls and retained two revisions in
149.48 seconds. The predicate repair was accepted: `có thể sử dụng` became
`sử dụng`, while `có thể` remained in modality. The OR/ellipsis structure survived.
The case still fails meaning checks because the group reference is omitted and
the future marker is assigned to modality instead of time. It correctly remains
`needs_review`; the unspecified comparison target is permitted faithful abstention.
Report: `20260920-semantic-v2-overlap-gemma12b.jsonl`, commitment
`7c1e2c767aa2c053a0ee2b3cdfb7d658d82a915712bb701cd625ba4d145fdc7a`.
Final-source API tests (8), staging-host build and aggregate validator passed.

## Remaining activation gate

The Qwen rocket output still misselects a branch/expanded phrase, omits reference
and future scope, and calls an inferred target explicit. Gemma's final-source
replay repairs the predicate but retains the reference/future failures described
above. Host issues correctly retain `needs_review`; extraction is incomplete.
Automatic repair currently handles the existing bounded mechanical fields. The
next fidelity step is to route a specific invalid structured selection to a
host-bound repair with source-local choices, while preserving group cardinality,
scope and provenance. Do not solve it by hard-coding the rocket answer, dropping
meaning checks, switching the Web model silently or inserting inferred targets.
Do not activate v2 on the user host based on passing Rust/API/Web tests. Preserve
the portable common executor and failed evidence while improving model selection
or bounded local repair. No KU save, commit, push or merge was performed.
