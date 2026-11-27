# KU-ENC-003 analysis preparation

Status: **draft, not preregistered or reviewer-acknowledged**. This document
contains no holdout text, labels or model results. Bind its final bytes, the
implemented statistical reducer, reviewed feature/group assignments and rubric
into the finished run package before the reviewer acknowledges its digest.
The existing framework/profile thresholds remain unchanged.

## Units and denominators

- A source is the immutable complete exported cell, subsequently admitted as
  a governed SourceArtifact. Three repetitions are repeated observations of
  that source, not three independent sources. Group membership comes from
  independently checked provenance/translation/template/near-duplicate review.
- Freeze supported/unsupported/ambiguous categories and claimed features per
  language before outputs exist. At least 100 independent VI and 100 EN sources
  and ten per claimed feature are required, plus the additional adversarial
  sets. Workbook counts alone do not establish these conditions.
- Create the complete source × tuple × repeat matrix before dispatch. Every
  planned row survives rejection, invalidity, timeout, cancellation, OOM,
  process death and interruption. Pending rows prevent a completed-run claim.
  No automatic replacement sample or hidden resampling is permitted.
- Evaluate every language/feature stratum separately. A source can belong to
  multiple strata; overlapping counts are disclosed, never added as independent
  samples. The supported intersection for cross-model comparison is frozen
  from scope/labels, not selected from successful outputs.

## Independent grading and fidelity

The encoder sees only granted sources and admitted context. Labels, rubric,
acceptable alternatives and source-specific FID expectations belong to a
separate evaluator authority. An OS folder readable by the same user is not
evidence of administrative independence or a secure worker/evaluator boundary.

The operational reducer cannot assign `acceptable_complete`. A native
preparation/CID binding plus independently evidenced grading is required before
an output enters a quality numerator. `reported_complete` is only an operational
claim; a review draft, schema pass or matching model answer is insufficient.
Unreviewed/missing assessments remain explicit and prevent a passing result.

FID evidence must use the existing native FID-001/002/003 contracts: publisher
attempt, at least two external blind attempts, commit-before-reveal transcript,
and evidenced administrative-principal plus pipeline/model-lineage groups under
the applicable policy/frontier. Keep all signed mismatch/unresolved evidence
and validated alternatives. No majority winner, truth status or invented
reviewer identity is introduced by this plan.

## Proposed statistical method to finish and lock

1. Report integer numerators/denominators for each repetition and each
   language/feature stratum. Complete precision uses all claimed-complete
   outputs; recall uses all supported source/repeat rows. Failure and abstention
   contribute zero acceptable completions, without shrinking the denominator.
   Zero denominators produce unavailable results, never 100%.
2. Report pooled descriptive ratios and per-source repeat summaries. The
   effective independent sample count comes from reviewed provenance groups.
   For uncertainty, propose paired cluster bootstrap over those groups within
   each language, 10,000 draws, deterministic seed 20260927, percentile 95%
   intervals (2.5/97.5). Keep all repeats and all model outputs of a sampled
   group together. Specify exact PRNG and percentile implementation in code
   before locking. Fewer than two groups yields an unavailable interval.
   Confidence intervals are descriptive; do not silently replace the accepted
   point-threshold gates with a different confidence-bound gate.
3. Repeat semantic agreement: all three unordered repeat pairs per supported
   source. Cross-family semantic agreement: all nine repeat pairs per shared
   supported source for each predeclared tuple pair. A failed, absent,
   incomplete or ungraded member is not semantic agreement. Also report the
   successful-intersection diagnostic with its own denominator. Grade agreement
   against preregistered meaning/alternative relations; do not collapse every
   individually acceptable interpretation into one identical meaning or CID.
4. Report generic canonical CID and normalized-semantic CID agreement separately
   from semantic agreement, including provenance differences and unsuccessful
   rows. Same validated SEM canonical identity remains a separate 100% gate.
5. First-pass validity, repair usage, complete coverage, unsupported abstention,
   unresolved outcomes and meaning-changing errors each retain their own
   numerator/denominator and per-feature confusion counts. Do not infer
   unsupported correctness merely from a transport error or timeout.

The statistical bootstrap and independent quality/FID bridge are **not yet
implemented**. The current Python reducer only checks the full operational
matrix, aggregates charged budgets/outcomes and reports available timings.
It leaves quality, agreement, independence and qualification unevaluated.

## Resource and fault measurements

Use monotonic end-to-end time from admission through load, extraction, repair,
resolution and preparation, with stage times plus observed call/token/work
counts. Quantiles use nearest rank: sorted value at `ceil(p*n)`, one-based.
Report p50/p95 for completed results and all terminal outcomes separately,
alongside failures and recall. Unknown interruption time is missing, not zero.
The 30-second constrained/no-LLM and 120-second standard ceilings are unchanged.

Declare cache state and worker lifetime for every lane. The existing managed
provider starts a worker per call and uses `keep_alive=0`; repetition alone
cannot be labeled resident-model warm evidence. Unsupported warm lanes stay
unqualified. Fix the order/seeds and cold/warm definitions before lock.

Collect actual peak process-tree working set/RSS and private/commit memory,
host validation memory, configured OS limits, sampling interval and collector
limitations. Include KV/cache/runtime buffers in measured ownership; do not
add nominal weights to an RSS value and call the sum a measured peak. Record
energy only where the physical device exposes a meaningful measurement.

Cancellation/OOM/process-death tests must retain request, stop and late-callback
evidence for the owned worker and descendants. A terminal ledger row or receipt
digest is not proof of OS termination. Refuse the next dispatch while stop is
unconfirmed. The synthetic Python process-exit regression tests ledger durability
only; it does not establish in-flight model cancellation or OOM behavior.

## Final assembly order

Complete authenticated isolated custody, both exact tokenizer/worker adapters,
native workflow journal integration, collectors, evaluator/FID bridge and
statistical reducer first. Bind exact source/split/labels/rubric/evaluator,
candidate/build/Registry, model/tokenizer/template/quantization/backend/device,
profile/thresholds, run order/options and this analysis into one immutable
package. Then obtain real reviewer acknowledgement of that digest. Every
qualification call, including schema probes, warm-up and ablations, remains
blocked until that admission succeeds.
