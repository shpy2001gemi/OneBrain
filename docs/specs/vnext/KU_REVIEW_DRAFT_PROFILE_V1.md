# Local semantic review drafts v1

Owner authorization: 2026-09-08 KU-WEB-001 follow-up to D-023. The owner approved
quoted semantic drafts, host-owned mechanics, bounded field repair, independent
review suggestions and durable asynchronous work, tested before Web activation.
This is an additive private draft surface. It does not change canonical SEM/KU
bytes, Registry authority, publication, rewards or model qualification.

The 2026-09-09 owner-approved [semantic selection extension](KU_SEMANTIC_SELECTION_PROFILE_V1.md)
adds a distinct `ku-semantic-selection/1.0` executor and `draft_extracted` state.
New selection jobs delegate sparse meaning choices to the model and assembly to
the shared host. They do not run the v1 mandatory self-review described below;
semantic/factual verification stays explicitly unassessed. Existing v1 records
retain their original interpretation. All clients consume the shared executor.

## Draft and authority

A draft retains the exact consented source and ordered claims. Each claim contains
an exact-quote subject, core predicate, ordered arguments, separately scoped
frequency, negation, condition, time, location, modality and approximation quotes,
quoted quantities and explicit links to other claims. Empty arrays mean no such
content was proposed, not a conclusion that the source has none. Difficult content
is retained in `unresolved`. The wire schemas and prompts live in
[`ku-review-draft-v1`](ku-review-draft-v1/).

The model supplies no cryptographic IDs, UTF-8 offsets, Registry IDs, truth scores
or actions. Host code assigns local claim/revision identities, anchors quotes in
the admitted source, retains repeated-quote ambiguity and parses exact numbers.
Unit spelling and counted entities stay separate; neither establishes a unit CCID.
All fields retain their producer and revision provenance. Proposed values cannot
authorize an external action, source access, compilation or persistence as KU.

Validation distinguishes closed-schema conformance, source grounding, role-level
source coverage, independent review and canonical lowering. Full evidence quotes
do not count as semantic role coverage. Uncovered source content is a review issue,
not silently discarded text. Coverage does not prove correct roles or meaning.

## Bounded tasks and repair

Short sources use one draft task followed by review. Longer sources use bounded
focus windows with explicit surrounding context and ordered source anchors; no
truncation or punctuation-only claim splitting. Cross-window references that
cannot be represented remain unresolved. Original required scope remains fixed.

Review tasks return typed field edits with claim index, field, previous quote and
replacement source quote. The host checks the base revision and exact old value,
known field, bounded index and source grounding before staging a new draft revision.
There are no arbitrary JSON paths or executable suggestions. A repair may change
only its declared fields. Every proposed revision is revalidated; a change that
introduces grounding/coverage/reference errors is rejected. Previous revisions
remain available. Removing an issue cannot remove its source from the required scope.
Missing-claim repairs use a separately bounded quoted-claim task; no invented facts.

A model review finding, empty findings or a successful patch never means verified
truth. The automatic loop is finite. Remaining uncertainty is returned with the
draft, rather than concealed by repeated generation. Different models may perform
draft/review tasks, but each production adapter must have admitted artifacts,
templates, tokenizer and resource accounting. No automatic download or hidden
fallback to another model. Development loopback probes are separate evidence.

## Durable background lifecycle

The node owns source consent, job identity, encrypted checkpoint storage and task
execution. Source and consent commit before queueing; call/token reservations and
the exact task/revision binding commit before inference. Each completed phase is
durable before the next call. Private content never appears in URLs, logs or WS.
The browser starts a job, receives its ID, then reads progress independently of
the inference connection. Closing the browser does not cancel the job or replay it.

States: `queued`, `extracting`, `reviewing`, `repairing`, `needs_review`,
`draft_ready`, `interrupted`, `canceled`, `failed`. `draft_ready` means that draft
mechanical checks pass with no outstanding findings from the bounded review; it
does not mean a saveable KU. A failed call leaves prior draft revisions readable.
Restart preserves completed work; an interrupted call has unknown model output
and its reserved budget remains charged. Explicit resume can schedule bounded
remaining work; polling never starts inference. Cancel fences late callbacks.
Source/model/scope changes require a new job. Idempotent start reuses the exact
job only when its full principal/dataset/input binding matches.

The opt-in authenticated `/api/vnext/ku/editor` transport adds `review_start`
with the same consented TextIntake fields as `encode_text`, `review_list` with `{}`
(at most 16 recent job summaries, no inference), and `review_get`,
`review_resume`, `review_cancel` with `{operation_id}`. Responses contain a private
`review_job` with operation, source, model, revision/checkpoint state and limitations.
All use existing session/budget fences and no-store responses; polling uses POST
so identifiers stay out of URLs. Start requires the owner's reserved operation.
Resume may also use its `unknown_outcome` receipt after a process restart, but
only for an existing interrupted draft with the same principal, dataset, source,
model and profile. It neither replays canonical preparation nor changes that
receipt; the draft checkpoint independently records all spent work. A canceled
operation or canceled draft cannot resume.
Get/cancel remain recovery reads/actions when new work is unavailable. Start commits
consent and a queued checkpoint before returning. Repeating it is idempotent, not
permission to replay a failed or interrupted call. Resume is explicit and keeps
spent counters. This surface never calls prepare/save or alters a canonical receipt.
At most 16 live jobs wait/run, 256 stored drafts and 16 MiB encrypted draft metadata
per dataset. Each job checkpoint is bounded to 1 MiB; limits retain prior revisions.
Initial scheduling allows at most five calls per focus window: drafting, a separate
number analysis when ASCII numeric source syntax is present, review, one bounded
missing-claim supplement when requested, and review/field repair within remaining
calls. Number refinement cannot drop an existing number. Supplements append only
new evidence, rebase local references and retain all existing claims unchanged.
Unsolved findings remain `needs_review`. This preserves the same total job bounds.
Number analysis is skipped when every numeric occurrence is already proposed
with a unit or counted entity. This is a completeness check for scheduling, not
a semantic verdict. Simultaneously assigning a physical unit and a counted entity
to one simple quantity remains ambiguous in this profile and requires review.
Inert edits (including repeated exact old-to-same-value proposals and empty array
edits) do not create revisions. Nonempty old values must still match the declared
field; inconsistent or coverage-losing changes remain rejected atomically.

On short windows (at most 48 lexical tokens and 16 KiB of candidate quote strings),
the host may restrict quote fields to exact contiguous source phrases in the
structured decoder. Numeric substrings and unit letter substrings are included.
Review `before` values are restricted to existing draft field values; known claim
indices are bounded to the current draft. Longer windows retain the original
closed schema and host grounding checks. This cannot establish semantic roles;
it only excludes fabricated quote strings. Schema property order is preserved.

Initial source size stays at 8192 UTF-8 bytes. Job bounds: 64 claims, 16 windows,
32 model calls, 2048 output tokens and 8192 total context tokens per call,
600 seconds per call and 1800 seconds charged execution per job. Only one model
call runs at a time on the admitted host; waiting in the durable queue does not
spend inference time. Limits exhausted retain the draft as needing review. Resume
does not reset counters. Changing these limits changes the profile commitment.

## Promotion and activation

Canonical preparation remains a separate operation through existing source and
Registry validation. A lowering plan must enumerate every draft construct and
its existing supported SEM target; any unrepresentable frequency, count, scope,
reference or uncertainty blocks complete KU preparation. Do not turn `usually`
into an unconditional rule or a counted entity into an invented SI unit. This
profile provides no new canonical representation for those constructs.

Web activation follows headless real-model development tests: the two owner
sentences plus negation, condition and multiple-claim variants must retain their
meaning in inspected outputs. Expected interpretations stay outside model prompts.
Tests include omitted qualifiers/claims, fabricated quotes, invalid field edits,
stale revisions, restart, cancellation and partial progress. Record unsuccessful
models/cases as well as successes. Development success is not blind qualification.
The old experimental Candidate path remains available until the draft path passes
these gates. No save, share or publication is triggered by producing a draft.
