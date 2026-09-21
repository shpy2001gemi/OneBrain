# Headless encoding and portable proposal experiment — 2026-09-08

Owner requested autonomous tests of two development sentences and improvements to
the prompt/task architecture across personal models, rather than selection of a
single best model. These cases are public development inputs, not blind holdouts.

## Implemented test loop

- [Development cases](../../../../scripts/encoder/development_cases.json) keep
  source text separate from human review expectations. Expectations are never
  sent to the model.
- [extraction_probe](../../../../src/ku-encoder/examples/extraction_probe.rs)
  runs the actual managed provider/shared extraction workflow, one case at a
  time, without Web, Vault or Registry authority. It records exact provider pins,
  input-token reservations, repair errors, raw responses and checkpoints in an
  explicitly selected private development report. Passing here is **not** a
  Registry-resolved or verified KU.
- [probe_ku_host](../../../../scripts/encoder/probe_ku_host.py) exercises the real
  local private API, including source consent/intake/prepare, then cancels only
  its own test reservations. It never saves or publishes.
- [probe_review_proposal](../../../../scripts/encoder/probe_review_proposal.py)
  tests a smaller semantic proposal task using a
  [portable prompt](../../../../scripts/encoder/review_proposal_prompt.txt) and
  [closed proposal schema](../../../../scripts/encoder/review_proposal.schema.json).
  Model/template are explicit parameters. No download or automatic model fallback.

All reports contain deliberate source/model development data; keep them in the
owner's private directory. They are not runtime logs, public APIs or qualification
artifacts. The lightweight proposal is **not integrated into Web or the KU service**.

## Architectural boundary

The model proposes ordered predicates/arguments, qualifiers, quoted numbers/units
or counted entities, explicit statement relations and unresolved content. It does
not count UTF-8 offsets, generate cryptographic IDs, choose Registry IDs, perform
unit arithmetic or claim verification. Host code anchors exact quotes and keeps
all possible positions when a quote is repeated; it never silently picks a sense.
No exact source match, incomplete coverage or invalid reference means repair or
review, not an inferred correction.

The review proposal can retain `frequency: thường` and a counted entity `bánh`.
The restricted current KU Candidate cannot faithfully lower every such construct.
This is a representation gap, independent of whether an 8b model is clever enough.
A later admitted draft/reviewer/lowering profile must specify frequency, counts,
scope and provenance before these drafts can produce canonical KU. It must retain
the source and proposal revisions through verification. Another AI can challenge
or repair the proposal; it does not grant publication, truth or Registry authority.

Mechanical schema/quote coverage is separate from semantic fidelity. For example,
copying the whole sentence as a predicate can pass structure while failing useful
extraction. The prototype always labels its result as requiring independent
semantic review, even if all mechanical checks pass.

## Production-path improvements within the existing contract

The shared workflow now checks source spans, numeric source lexemes and candidate
compilation with empty bindings before closing its existing one-repair allowance.
The authoritative Registry-bound compile still runs later. Numeric substring
hints cover attached units without rewriting the source. No extra inference call,
silent deduplication, weakened acceptance or new canonical representation is added.
The guide asks for explicit unsupported coverage when frequency/typicality cannot
be represented, rather than flattening it into an unconditional claim.

The active host still uses the Candidate prompt assembled in
[managed_ollama.rs](../../../../src/ku-encoder/src/extraction/managed_ollama.rs),
including the pinned
[semantic guide](../../../../src/ku-encoder/src/extraction/ollama_semantic_guide.txt),
host context and span hints. The portable review prompt above is a separate
experiment, not a replacement secretly enabled in the Web host. Qwen3 runs use
the raw template with an empty closed thinking block; increasing the experimental
deadline to 600 seconds does not fix an invalid or incomplete extraction.

## Baseline observations

Managed Qwen3 8b, CPU, two calls each, synthetic empty Registry context:

| Source | Seconds | Structural result | Human inspection of proposal |
|---|---:|---|---|
| Nước thường sôi ở 100oC ở gần mặt nước biển | 317.6 | Failed `concept_label` | Invalid `source` predicate label; self-reference; not an acceptable extraction. |
| Xe oto cá nhân thường có 4 bánh | 286.2 | Candidate checks passed, unresolved Registry | Entire sentence used as predicate and argument; not a useful semantic decomposition. |

These wall-clock development observations are not a latency benchmark: compilation
occurred during part of the baseline, Registry contexts differ from the real host,
and the later proposal task has less work than a complete KU preparation. Do not
report a portable speedup or model-quality qualification from two cases.

## Updated real-host API result

The rebuilt local host was exercised directly with the owner's signed Registry
and the exact two requested sources, using installed `qwen3:8b`. Both runs used
the updated production-path preflight/guide and the 600-second experimental limit.

| Source | Seconds, intake + prepare | Actual API result | Test reservation |
|---|---:|---|---|
| Nước thường sôi ở 100oC ở gần mặt nước biển | 379.26 | Failed `duplicate_id`: `concepts[4].key` duplicated `concepts[0].key`; repair had not produced an acceptable candidate. | Canceled successfully |
| Xe oto cá nhân thường có 4 bánh | 451.29 | Failed `oneof`: arguments combined incompatible Term fields, including missing concept references and unknown fields. | Canceled successfully |

Neither request produced a validated preview. Neither saved nor published a KU.
These were model-output failures within the deadline, not evidence of account
quota exhaustion or an Ollama disconnection. Detailed failures arrived through
the existing private API. The Web's current Candidate path can still encounter
these failures; successful unit tests and a longer timeout do not make these
source sentences succeed. Raw API results and cleanup receipts are retained in
private `20260908-updated-host.jsonl`.

## Review-proposal results and failures retained

Final shared prompt (explicit subject/core predicate, exact-source qualifiers,
separate number/unit/counted entity, empty-array guidance and an unrelated syntax
example), same cases and CPU-only execution:

| Model | Water sentence | Car sentence | Manual review of final output |
|---|---|---|---|
| qwen3:8b | 117.31s / 2 calls | 59.74s / 1 call | Water retained subject, frequency, number/unit and location text, but put location in arguments rather than a location qualifier. Car decomposed subject, `có`, `4 bánh`, frequency `thường`, value `4`, counted entity `bánh` as intended. Both remain unverified proposals. |
| qwen3:1.7b | 14.66s / 1 call | 13.23s / 1 call | Water mislabeled location as frequency and omitted `thường`; car left `thường` inside predicate with no frequency qualifier. Mechanical checks passing did not establish semantic success. |

The final prompt cost 1,226/1,221 input tokens for the two sources versus the
baseline's 2,144/2,103 first-call tokens. Outputs were about 114–150 tokens in
these proposal runs. The proposal omits canonical compilation/Registry work;
latency differences are **not** equivalent-task speedup claims. Earlier prompt
versions are retained in private reports, including 45–53 second 8b outputs with
missing subjects or misplaced roles. Prompt edits were assessed on public
development cases and do not constitute untouched test evaluation.

Actual final 8b proposal for the car source (no manual correction):

```json
{
  "statements": [{
    "subject": "Xe oto cá nhân",
    "predicate": "có",
    "arguments": ["4 bánh"],
    "qualifiers": [{"kind": "frequency", "quote": "thường"}],
    "numbers": [{"value_quote": "4", "unit_quote": "", "counted_entity_quote": "bánh"}],
    "relations": [],
    "evidence": "Xe oto cá nhân thường có 4 bánh"
  }],
  "unresolved": []
}
```

This is a source interpretation for later review, not an assertion that all
personal cars have four wheels, a Registry binding or a saved KU.

Additional composition source: `Cảm biến ghi 12kg. Đèn không sáng, nhưng quạt chạy.`

- 1.7b: 16.50 seconds, one call. Omitted the fan proposition, misplaced numeric
  text as frequency and left negation inside predicate. Not semantically valid.
- 8b: 180.91 seconds, two calls. Three propositions, lamp negation and contrast
  relation were retained, but source `quạt` became `Quạt`. Exact grounding rejected
  it; model added unresolved entries instead of correcting the quote. No KU claim.

The [different-model review probe](../../../../scripts/encoder/probe_review_verifier.py)
used installed `qwen2.5:3b` on final 1.7b proposals, taking 11.32/10.06 seconds.
It flagged modifier omissions, but free-text suggestions introduced unsupported
Chinese phrasing and extra vehicle-type claims. Nothing was applied. This is
evidence that a second model is not an automatic authority or guaranteed repair.
Reviewer findings need exact source anchors, proposed field-level revisions,
mechanical revalidation and an explicit decision before promotion. These are
development observations, not blind independent fidelity evidence.

Private raw reports are in `C:\Users\shpy2\Documents\OneBrainLocal\development-reports`:
`20260908-baseline.jsonl`, `20260908-review-final-qwen8b.jsonl`,
`20260908-review-final-qwen1_7b.jsonl`, `20260908-composition-qwen8b.jsonl`,
`20260908-composition-qwen1_7b.jsonl` and `20260908-verifier-qwen2_5-3b.jsonl`.
They pin model digests and actual request/response bytes; all selected models were
already installed. The proposal/verification scripts are bounded development
tools, not the production worker's full admission/cancel/security implementation.

## Resulting direction

Keep an explicit sequence: source custody → quoted semantic proposal → host
anchoring/number parsing/reference checks → independent review suggestions →
versioned resolution and semantic lowering → canonical KU preparation. Run jobs
asynchronously later under the already recorded consent/lifecycle requirements.
Persist a reviewable draft without pretending every model can emit canonical
Candidate/SEM in one pass. Preserve omissions and uncertain roles for the reviewer.
Do not silently lower unsupported frequency/count semantics or increase arbitrary
retries to hide a weak extraction. These two small models' results identify task
design and validation needs; they do not select a universally best model.

Shared runtime tests: 26 extraction tests pass (one owned-worker test ignored),
including number-error repair before candidate recording, span checks, corpus
oracles and cancellation. Relevant API invalid-output/cancel and node extraction
crash tests pass. Six Python proposal checks cover duplicate/deep/nonfinite JSON,
exact UTF-8 matches, unknown fields, fabricated quotes, reference validity,
coverage and the distinction between mechanical and semantic validation.
The vNext contract validator, eight generated bundle artifacts, Python compilation,
Rust formatting and Git whitespace checks also pass. The real host example was
rebuilt and restarted for the API tests above; no Web rebuild was needed for
these shared-runtime and development-tool changes.

## Reproduce without Web interaction

From `src`, build `cargo build --locked --config
'profile.dev.package.sha2.opt-level=3' -p ku-encoder --example extraction_probe`.
Set `KU_OLLAMA_EXE`, `KU_OLLAMA_MODELS`, optionally `KU_OLLAMA_MODEL`, and run the
example with `--development-inputs CASES.json NEW_PRIVATE_REPORT.jsonl`.

From repository root, use `python -m scripts.encoder.probe_ku_host --help` for
the real local API runner. The proposal prototype needs Python `jsonschema` and
an operator-started loopback Ollama: `python -m
scripts.encoder.probe_review_proposal --help`. Choose `qwen3-raw` only for that
known template; `ollama-chat` delegates template selection to the installed model.
Only registered production adapters with pinned tokenizers/resource accounting
can eventually adopt the task; this prototype does not admit arbitrary models.
