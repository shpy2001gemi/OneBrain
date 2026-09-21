# Semantic guidance and experimental timeout — 2026-09-07

Owner approved retaining detailed semantic instructions and examples, requested
a longer timeout, and described future consented encoding/sharing as asynchronous.

## Implemented change

The experimental Ollama host explicitly selects a 600,000 ms aggregate deadline.
The normal 120,000 ms standard and 30,000 ms constrained qualification budgets
stay unchanged. The selected policy is bound into the implementation commitment.
Planning and extraction use the same deadline; prior elapsed time remains charged.
The Attempt schema's representable remaining-time ceiling and generic WorkBudget
capacity now accommodate 600,000 ms. Final preparation records actual elapsed time
from the workflow budget, avoiding a hard-coded 120-second calculation/refund.
The generated bundle and validator bind this explicit experimental exception.

The existing reviewed system prompt is retained. The provider appends a pinned
[semantic guide](../../../../src/ku-encoder/src/extraction/ollama_semantic_guide.txt)
covering propositions, ordered arguments, shared qualifier scope, references,
quantities, coverage, ambiguity and unsupported semantics. Examples distinguish
simple assertions, conjunction, conditions, negation and approximate quantities.
The raw ChatML template still closes an empty thinking block. Full Candidate
schema, exact span validation, Registry resolution and host compilation remain.

This does not claim that arbitrary long documents or all natural-language meaning
are supported. The local editor still admits at most 8192 UTF-8 bytes, and each
call retains the existing input/output token caps. A phrase such as “near” does
not license an invented numeric tolerance. Unrepresentable meaning must abstain.

## Async product direction retained for follow-up

The current prepare request is awaited by the Web; it is not a durable background
queue. A later async transport must return an operation/job reference promptly,
retain authorized work independently of the page, and expose authenticated
status/recovery/cancellation. Scheduling needs bounded concurrency/backpressure,
durable ownership, process-restart semantics and no blind inference replay.

Encode, private-save and share permissions must be explicitly represented and
checked at their respective effects. Sharing checks the current authorization
and validated artifact; text-intake consent alone is not publication permission.
The current host has no publication port and this update does not enable one.
Long-source planning must preserve cross-sentence context and coverage, report
unresolved regions, and avoid silently publishing partial results as complete.

## Verification and local activation

- Shared extraction: 20 tests pass, including completion after 120 seconds of
  precharged elapsed time, remaining 600-second deadline exhaustion, policy
  commitment separation, cancellation and replay fencing.
- KU API: 12 tests pass; node KU: 22 tests pass, including private save/reopen,
  concurrent cancel and process-crash recovery.
- Python encoder contract suite: 18 tests pass; global vNext contract validator
  and regenerated bundle checks pass.
- Web: 12 tests and production build pass. Lint retains four existing warnings
  outside KU. Rust formatting and diff whitespace checks pass.
- Real `qwen3:8b` development roundtrip passes with the new semantic guide:
  inference/validation 139.519 seconds, HTTP 200 and ready preview, explicit
  private save, then restart/read. Total test duration 151.34 seconds; model
  verification 8.881 seconds. The sentence is `Copper is conductive.` and the
  signed Registry is test-only, isolated from the owner's real Registry/data.
  This demonstrates execution beyond the former 120-second ceiling, not quality
  qualification or Vietnamese/long-document correctness.
- New provider backend SHA-256:
  `2bea1b629d395be9b6012aaff7cd6f56fcee266c04066f7d8b9d1388facdd48f`;
  bundle SHA-256:
  `4869f112acf16feeafe252e4f985e08c73c47b0a7b293d4868db7428d945307d`.

The local host was rebuilt and restarted against the owner's existing config,
keys, data and signed Registry. Authenticated checks confirm Web HTTP 200,
Registry ready, encoder ready and admitted qwen3:8b. Old pending operations need
reconciliation against the new process generation; no user operation was
automatically replayed or saved. The updated review prompt is outside Git at
`C:\Users\shpy2\Documents\OneBrainLocal\prompt-review\semantic-guidance-prompt-template.txt`.
It contains placeholders for attempt-specific context, not a captured user request.
