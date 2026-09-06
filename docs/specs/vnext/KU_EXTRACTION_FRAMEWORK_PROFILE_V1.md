# Shared KU extraction framework v1

> Task: KU-ENC-001 — contract ready for review; runtime integration remains KU-ENC-002.
> Direction: owner-approved D-018. No production provider or model qualification is claimed.

This contract narrows the work delegated to a local/personal model: propose
source-grounded concepts, statements and relationships in a closed schema. The
host owns custody, context selection, budgets, Registry resolution, validation,
SEM compilation and all KU service calls. Rules and AI use the same boundary.
The framework is shared across platforms; provider adapters only implement local
inference mechanics. It does not introduce another orchestrator or KU format.

## 1. Authority and scope

The [SEM profile](SEMANTIC_PRIMITIVES_V1.md),
[KU product workflow](KU_PRODUCT_WORKFLOW_PROFILE_V1.md),
[Base runtime interface](BASE_V1_RUNTIME_INTERFACE_PROFILE.md),
[Registry operations](CONCEPT_REGISTRY_OPERATIONS_PROFILE_V1.md) and
[canonical profile](CANONICAL_PROFILE_V1.md) remain authoritative for their
respective boundaries. This profile defines an intentionally smaller extraction
surface; it cannot reinterpret an accepted SEM construct. A conflict with those
contracts stops work and is recorded for the owner.

The [local Receptor encoder](LOCAL_RECEPTOR_ENCODER_PROFILE_V1.md) remains the
AI-001 Receptor path; [observation intake](AI_LOCAL_OBSERVATION_INTAKE_V1.md)
retains source custody. This text framework does not silently convert text to a
Receptor or treat a derived observation as raw input. Existing MOB-06 owns mobile
provider integration and MOB-07 owns mobile evidence. No mobile files, builds or
qualification evidence are changed by this task.

Context, ProviderInput, Candidate, Resolution, ProviderManifest and Attempt are private internal
DTOs, not KnowledgeObjects, published API DTOs, Base operations or new signature
domains. No Base IDL registration, object kind, canonical byte format or rollout
flag changes are needed. The merged `KuInputProvider` is only an integration
seam; it does not yet implement the extraction workflow specified here.

The following contract requirements apply equally to rules and model adapters:

- The provider MUST NOT select tools, write KU objects or authorize save/publication.
- The host MUST reject unsupported schema fields, fabricated bindings and altered source evidence.
- Unresolved coverage or concept selection MUST block whole-job artifact preparation.
- Retries and repairs MUST share durable job budgets and cancellation fences.
- Compilation MUST preserve the existing SEM/profile identity rules and authority boundaries.
- Structural conformance MUST NOT be reported as truth, blind fidelity or model qualification.

## 2. Versioned contract bundle

[profile.json](ku-encoder-v1/profile.json) fixes the supported surface, resource
ceilings, phase mapping and qualification thresholds. [schema.json](ku-encoder-v1/schema.json)
is the sole schema source. The context, input, candidate, resolution, attempt and provider
schema files are generated reachable-definition projections, not hand-maintained
copies. [Examples](ku-encoder-v1/examples.json) are generated from validated corpus
rows and rendered provider views. [English](ku-encoder-v1/prompt.en.txt) and
[Vietnamese](ku-encoder-v1/prompt.vi.txt) prompts share the same task and fields.

[bundle.manifest.json](ku-encoder-v1/bundle.manifest.json) hashes all contract
artifacts, generated schemas, prompts, corpus, spec and oracle tools. Its own
SHA-256 identifies a bundle; hashes use exact repository LF UTF-8 bytes.
`context_sha256` hashes sorted-key, compact, UTF-8 JSON with no floats, duplicate
keys or non-finite numbers. These are artifact bindings, never KU ObjectCids or
proofs of source/reviewer authority. A semantic contract change increments the
profile; editorial changes still change the bundle hash. Attempts pin exact bytes.

Adapters pin model artifact, tokenizer, backend build, bundle and optional grammar
hashes, decoding temperature and optional seed. Model name/tag alone is insufficient.
No claim that temperature zero or a seed guarantees equal outputs is permitted.

The offline schema checker implements only the explicitly listed closed subset;
unknown keywords and nonlocal references fail. A backend capability list is a
claim requiring KU-ENC-003 probes. Every schema keyword affecting acceptance must
either be enforced by the backend or be explicitly recorded as host-only. Grammar
conversion omissions cannot be described as full schema support. `bounded_json`
is permitted with the same strict parser, semantic checks, output cap and retry
budget. It cannot enable tools. Generated grammar bytes and their compiler version
belong to a qualified adapter manifest; this task ships no fabricated grammar.

## 3. Host workflow and provider contract

1. Authenticate principal and source grant; bind immutable source object, dataset
   and process generation. Preserve exact original UTF-8 source bytes privately.
2. Reserve aggregate job resources before reading large bodies or loading a model.
   Verify and pin a complete signed Registry generation under configured trust.
3. Build an ordered manifest of independently meaningful focus chunks and required
   coverage units. Attach only necessary admitted context windows and Registry
   lookup candidates. Freeze and hash context before a provider sees it.
4. Run a registered bounded rule extractor or one provider extraction call. The
   provider returns a full Candidate, never tool calls, CCIDs, source corrections,
   `ready`, confidence-as-truth or save/publication commands.
5. Parse strictly, validate schema, spans, coverage, references and resource use.
   At most one additional call per context may repair or propose disambiguation;
   those activities share that slot. Its output is a full replacement for the
   same frozen context. Host errors are bounded codes and JSON paths, not logs.
6. Resolve concept mentions with verified Registry candidates. Ambiguity remains
   unresolved unless the trusted host obtains explicit review evidence. Validate
   the complete candidate and bindings; compile to existing SEM and KU preparation.
7. Show exact complete preview through the KU service. User correction creates a
   new preparation. Only the existing explicit Save/confirm path can persist the
   private KU. Publication/minting remain separate existing authorization paths.

The provider port accepts a frozen provider view, exact schema/prompt bundle,
call kind (`extract` or `repair_or_disambiguate`), reserved token/output limits,
deadline and cancellation fence. It returns bounded raw UTF-8 candidate bytes,
actual token usage when available and a typed outcome. It has no Registry write,
Vault write, KU-save, network-discovery or tool-selection capability. Inference
must run outside the aggregate service lock. Adapter errors must not expose raw
source/model output through logs or public APIs.

`provider_view` in the [oracle](../../../scripts/encoder/contract.py) serializes
only admitted window text, absolute byte positions, required units, opaque keys
and labels. It omits the full host `source_text`, CCIDs, unit arithmetic and
private authority metadata, validated against the separate ProviderInput schema.
The prompt, schema, example, context and bounded error
list are separate data blocks. Their entire rendered/tokenized length counts
towards admission. Source instructions remain data even if they imitate a system
message or demand tool use. No long-form reasoning output is required or stored.

Rules mode declares its supported source forms and abstains outside them. It is
not a regex fallback that marks arbitrary prose as complete. No-LLM never calls a
model, downloads one or silently switches to cloud inference. Unsupported local
capability produces an explicit unresolved/dependency outcome.

## 4. Source, scope and chunks

All spans are nonempty absolute UTF-8 byte intervals `[start,end)` over the exact
immutable source. Both endpoints must be codepoint boundaries and the quote must
equal that exact byte slice. Source is at most 786,432 bytes; each serialized DTO
is at most 1 MiB. A source reference is the admitted SourceArtifact ObjectCid;
the host restores its typed kind/major reference during SEM compilation. Corpus
hex IDs are explicitly synthetic fixtures, not validated source objects.

The host Context retains full source solely for custody/validation. A provider
receives at most 16 admitted focus/context windows. Each required unit lies in a
focus window. The host splitter must preserve every in-scope source region:
nonsemantic separators may be excluded with a private segmentation receipt;
uncertain content becomes a required unit. Required units are not selected by the
model. A long unit that cannot fit with necessary context is unresolved, never
truncated or discarded to fit a paragraph-success percentage.

For each required unit, Candidate coverage is exactly one of `represented`,
`unresolved`, `unsupported`, with referenced statement keys and a closed reason.
All represented units have at least one statement and reason `none`. Any other
status blocks the entire job's KU preparation. Partial output may be retained
privately for repair/review but carries no final artifact identities. An explicit
user request to encode a smaller scope creates a new source/scope-bound job;
the host must not silently redefine completion.

Statements retain source order and ordered arguments. Every statement belongs to
a required unit or is reachable as a supporting statement reference/condition.
An independent assertion from a context-only window is an orphan and is rejected.
Cycles are unsupported in this extraction surface. Whole-statement conditions
reference statement keys, not a compressed concept or isolated noun. Predicate,
argument and qualifier evidence must lie inside the statement's admitted evidence
spans. Quotation validates location, not whether a predicate or scope is correct.
Double negation, quantifiers, implicit subjects, pronouns without enough context,
uncertain attachment and unsupported number formats must remain unresolved when
the supported representation cannot preserve their meaning.

Jobs contain at most 16 ordered, nonoverlapping focus chunks and 256 output
statements. A chunk is compiled only as part of the complete admitted job. The
assembler preserves manifest and frame order, renumbers all local references and
rejects duplicate attempts, overlapping coverage and mixed source/Registry/profile
bindings. It never uses label equality to merge statements or remove duplicates.
Cross-chunk semantic dependencies require host rechunking into a bounded context
with explicit support statements before inference; unavailable context blocks the
job. Rechunking changes context and starts a new attempt under the same remaining
job budget. No replay under a different context is allowed.

## 5. Registry resolution and compiler rules

Only the host can create Context options from a verified signed Registry release.
Each option binds a mention, lookup label and full 16-byte CCID; unit metadata
comes from that same pinned generation. Lookup completeness is an external
precondition: truncated/top-k/partial indexes cannot establish unique identity.
More candidates than the admitted cap means unresolved, not a shorter list that
can now appear unique. Failure to verify a required Registry is fail-closed.

Optional bounded Registry descriptions may help a model distinguish senses;
they come from the pinned release and cannot replace source evidence. A concept's
optional `option_proposal` names an admitted opaque key, never a CCID. The host
may record it as `model_proposal` but cannot treat it as reviewed resolution.

Candidate concepts use exact source labels and local keys. Bindings must reference
the same mention and context; fabricated/missing/extra keys are rejected or remain
unresolved. `exact_label` is allowed only for one complete verified matching
candidate. No `matches[0]`, hash-of-label identity or guessed unit exists here.
`model_proposal` does not authorize selection. `host_review` requires a separate
principal-bound review record covering candidate/context/option hashes. A JSON
field or digest supplied by the model cannot satisfy that precondition. Alias and
translation lookup may return verified options but cannot change source labels.

| Candidate construct | Existing SEM target and compile rule |
|---|---|
| Concept/predicate | Full verified `ConceptCcid`; local keys and labels are excluded from semantic identity. |
| Ordered statements/arguments | `StatementFrame`; alpha-renumber by frame order from zero, rewrite every reference; never sort semantic frames to force agreement. |
| Text | `Literal::Text`; exact source quote, already NFC. Reject non-NFC literal rather than rewriting the source. |
| Boolean | `Literal::Boolean`; v1 lexical forms `true`, `false`, `đúng`, `sai`, case-sensitive. Other forms need a later explicit profile. |
| Exact quantity | Parse source decimal or fraction into reduced `(i64,u64)` ratio using integer arithmetic; no float. Preserve source unit CCID, seven SI exponents and exact affine scale/offset. |
| Statement reference | `TermRef::Statement`; resolve in this closed frame set and alpha-rewrite. |
| Negation/modality | Preserve explicit values; true negation and non-asserted modality require source evidence. Asserted is a speech mode, never inferred certainty. |
| Condition | `StatementQualifiers::condition` references the complete proposition; evidence anchors attachment. |
| Time/location/perspective | Corresponding typed qualifier term; no guessed timezone, entity or point of view. |
| Tolerance | `QuantityLiteral` in tolerance qualifier; source must explicitly express tolerance, with the same exact unit rules. |
| Source evidence | Ordered `SourceSpan` references to the admitted typed source object and byte intervals; quotes stay private outside SEM. |

ASCII number syntax is `-?(0|[1-9][0-9]*)(\.[0-9]+|/[1-9][0-9]*)?`, at most
64 characters. Integers are included; exponent, grouping/comma/local decimal,
implicit units and approximate precision are unsupported. Parse/reduce before
range checking. Unit ratios must already be reduced; scale must be positive.
Checked affine conversion must fit the same ratio bounds or fail explicitly.
Source unit and source value remain in SEM; coherent conversion is a validation
operation, not permission to substitute a value or alter the source.

The machine feature matrix lists variables/quantifiers, Receptor slots, typed
constraints, bytes literals, object-reference terms and cyclic references as
unsupported. They remain valid in the wider SEM profile; this extractor emits
empty constraints only for statements that do not require them. It cannot flatten
unsupported semantics into triples. A feature expansion needs a profile change,
mapping rules and fixtures before implementation. No Core DNA lowering is implied.

Exact identity means the same **complete normalized SEM, including source spans,
unit metadata, statement order, and the same canonical/object profile** yields
the same bytes and ObjectCid through the existing canonical encoder. The same raw
text can have different valid interpretations or source artifacts; exact CID
equality is not promised. Candidate local-key changes alone must not change SEM.

## 6. Budgets, interruption and private evidence

Machine ceilings are in profile.json: one simultaneous inference, at most two
calls per context including all retries and disambiguation, at most 32 calls per
job. Constrained jobs allow 8 calls, 4,096 input/2,048 output tokens per call and
30 seconds total. Standard jobs allow 32 calls, 8,192 input/2,048 output tokens
per call and 120 seconds total. Aggregate input/output token ceilings include
failed calls, transport retries, adapter warm-up requests and repairs; they are
not renewed when a provider or context changes. No-LLM permits zero calls.

Before dispatch, atomically reserve the maximum call input/output tokens and
increment attempt/job counters in private durable state. Charge work for source
bytes scanned, Registry candidates visited, validation nodes and emitted semantic
nodes, one unit per item/byte visited, bounded by the existing 1,000,000 work cap;
this is deliberately conservative, not a speed estimate. All repeated scans count.
Model token accounting remains a separate cap. Do not refund spent call/work/token
reservations on error/cancel. Release only live memory/concurrency leases.

Actual tokenizer counts plus reserved output must fit both backend context and
profile limits, including schema, examples, context, errors and adapter wrappers.
An adapter whose tokenizer/accounting is unavailable cannot qualify. Estimate and
reserve model weights, KV cache, grammar buffers, temporary generation buffers and
incremental Registry/host memory under the platform's admitted memory envelope.
ProviderManifest carries the complete peak reservation; qualification must measure
it. A small profile is a ceiling, not a claim that any named model fits weak/mobile
hardware. Memory admission failure leaves the job unresolved; no silent semantic
simplification, model download or unapproved off-device fallback.

Deadlines cover admission, load, extraction, repair, resolution and preparation,
using a monotonic clock. Reserve before dispatch and reject callbacks after the
deadline or cancellation fence. Cancel propagates to the adapter; if it cannot
stop, isolate/terminate its worker and discard late output. No service call can
be authorized by a late callback. Strict parsing bounds bytes/nesting before
allocation; streaming output is aborted at the byte/token cap, never repaired by
silently trimming to a valid prefix. Rules and validation need equivalent work,
memory and deadline checks even though they spend no model tokens.

The encrypted Attempt record binds principal, operation, dataset/process
generations, exact source/context, Registry root, bundle, provider manifest,
candidate/resolution digests, remaining deadline and monotonic counters. Its
`operation_id` is an opaque private binding digest to the existing operation, not
a change to Base's public operation ID encoding. Provider call detail and any
review/segmentation receipts are bounded private attachments covered by those
digests. No raw text, prompts, candidates or evidence digests enter public metrics.
This record is not the FID EncodingAttempt object and proves no independent fidelity.

`admitted`, `extracting`, `candidate_recorded`, `resolving`, `validated` map to
Base `reserved`; `prepared` maps to existing `prepared`. Extraction never owns
`confirming` or `committed`; the KU service owns consent/save/reconciliation.
`failed`/`canceled` retain existing terminal meanings. `interrupted` maps to an
honest `unknown` pending reconciliation, not a new public Base enum. The private
phase map cannot overwrite the authoritative service state.

Persist reservation before inference; persist a candidate and its digest before
marking `candidate_recorded`; persist validated resolution before `validated`;
record `prepared` only after exact service preparation has succeeded. A crash in
either side of these boundaries never invents a success. Recovery first reads
the authoritative KU journal. Process-bound capabilities are not revived: a new
process marks old inference interrupted, rejects its callbacks and requires fresh
authorized preparation where Base requires it. Revalidate custody, Registry and
dataset bindings before any continuation. A source revocation cancels further use.

Same-process resume may replay recorded candidate bytes through deterministic
validation with unchanged context/Registry/bundle/provider pins and remaining
budgets. It may not resample while calling that an exact replay. Any changed
binding, source correction or re-extraction creates a linked new attempt, retaining
the job's charged work and elapsed/deadline accounting. After restart, unknown
elapsed time is treated as exhausted for inference; it does not reset a deadline.

## 7. Conformance and model qualification

The [corpus](ku-encoder-v1/corpus.json) contains explicit expected logical SEM
projections and rejection/abstention oracles. Synthetic trusted host inputs do not
prove signed Registry validation, source custody, semantic truth or production
canonical CIDs. Structural-mapping-only examples exercise field mapping, not
linguistic extraction accuracy. The executable oracle is a specification test
tool; it has no persistence, inference, publication or production compiler role.

KU-ENC-002 must implement native compilation against these oracles and the
existing SEM canonical tests, add real source/Registry/permission validation,
durable reserve/crash/cancel/restart drills and prove that unresolved whole jobs
cannot reach artifact preview/save. The old tool-agent path, default certainty,
source autocorrection, local-label hashing and first-match fallback cannot be
reused as semantic policy. Do not retrofit only the prompt over those behaviors.

KU-ENC-003 must lock a blind holdout before any model run and record exact corpus,
model, tokenizer, backend, quantization/build, hardware/OS, resource profile,
bundle/grammar and Registry versions. Minimum per qualified model/profile: 100
independent vi and 100 en sources, three repetitions each, at least ten sources
per claimed feature, plus unsupported, ambiguous, injection and multi-chunk sets.
The conformance corpus is public developer material and cannot be the blind set.
Use the existing [blind fidelity workflow](BLIND_ENCODING_FIDELITY_WORKFLOW_V1.md)
and [evidence profile](ENCODING_FIDELITY_EVIDENCE_PROFILE_V1.md); a model grading
its own output does not establish independence.

Predeclared gates apply per language and supported feature, without averaging a
weak feature away:

- 100% structural/adversarial conformance and same-SEM canonical identity; zero
  unauthorized tools/writes, fabricated identities, hidden source changes, budget
  overruns, successful late callbacks or partial jobs reported as complete.
- Complete-source precision >=98%: independently judged acceptable complete
  outputs / all outputs claimed complete. Complete-source recall >=90%: acceptable
  complete outputs / all supported holdout sources. Abstention counts against
  recall; invalid/incomplete output is not removed from the denominator.
- Unsupported abstention >=99%; repeat semantic agreement >=95%; cross-model
  semantic agreement >=90% on the shared supported intersection. Agreement uses
  independently preregistered acceptable interpretations, qualifiers, role/order,
  exact numbers/units and coverage; do not force all valid alternatives to one CID.
- Report raw exact CID agreement separately from semantic agreement, including
  provenance differences, with per-feature confusion counts, sample counts and
  confidence intervals. Also report first-pass validity, repair usage, unresolved
  rate, complete coverage, unsupported detection and meaning-changing errors.
- Measure end-to-end cold and warm latency, p50/p95, peak RSS/working set, tokens,
  source bytes, work, call counts, cancellation latency and energy where measurable.
  p95 complete-result latency must fit 30s constrained / 120s standard, while the
  recall gate still passes. Timeouts count as failed completion. Zero-call rules
  obey their 30s ceiling and claim only declared supported forms.

These are qualification targets, not measured results. Failure limits the declared
supported surface or rejects the model/profile; it cannot weaken authority rules.
Changes to thresholds after seeing a run require a versioned rationale and a new
untouched holdout. Mobile claims additionally require existing MOB-06/MOB-07 real
device gates; desktop results do not qualify mobile adapters.

## 8. Reproduce

From repository root:

```text
python -m scripts.encoder.generate_bundle --check
python scripts/ci/validate_ku_encoder_contract.py
python -m unittest scripts.encoder.test_contract
python scripts/ci/validate_vnext_contracts.py
```

Regenerate with `python -m scripts.encoder.generate_bundle` after intentional
source changes, then review generated diffs. The contract task ends at review;
production inference and real model/device claims remain subsequent work.
