# Experimental local Ollama host v1

Owner authority: D-023, KU-WEB-001. This profile adds an opt-in experimental
host composition; full quality qualification stays in KU-ENC-003. The API always
returns `model_qualified: false`. Other input providers reject experimental AI.

The existing private `/api/vnext/ku/editor` tagged request gains:

- `models` with `{}`: returns `models` (at most eight entries with `model`,
  `implementation_commitment`, `experimental: true`), `limitations` and
  `consent_text`. Entries are installed, verified and host-admitted Qwen3 models;
  a model name alone never grants a provider capability.
- `encode_text`: `{operation_id,idempotency_key,model,text,consent}`; consent
  must be `true`, text nonempty and at most 8192 UTF-8 bytes/characters. Returns
  the generated `KuPrepareV1`, pinned to source/model/Registry, `local_ai` and
  `LOCAL_ONLY`. This action captures/stages input only. Existing prepare/revise
  runs the shared extraction workflow; existing save remains explicit.

Unknown/duplicate/null fields, stale generations, wrong principal/reservation,
unsupported model, unavailable source custody and changed idempotent requests
fail before inference. Intake is bounded to 256 entries and 8 MiB per dataset.
It uses an additional encrypted table in the existing KU journal; no competing
Vault writer. The complete source and consent bundle is durable before the
template is returned, and restored into the input custody port on node reopen.

The local host policy permits only deliberate user-submitted text for local
experimental encoding and private retention, until operator removal of the
dataset. Policy, adapter description, per-operation consent receipt, capture
scope and authority assessment are actual serialized private host records,
bound to authenticated principal/operation, exact text, model and policy version.
They are encrypted alongside the source. `ObservationGovernance` references
these content-addressed host records using context-local opaque reference kind
0; their digests are private record keys, not canonical KnowledgeObject CIDs,
public grants or a new global policy schema. The host resolves them against the
same stored record bundle and assesses the configured policy; nonzero digests
alone never authorize capture. SourceArtifact canonical schema is unchanged.
Denied/unresolved consent creates no source. No signed ObservationEvent or
Receptor proposal is fabricated for this text-extraction path.

The provider reuses `ExtractionWorkflow` and `SharedKuExtractionInputs` with
the standard token/work limits and an owner-approved experimental 600,000 ms
aggregate deadline (2026-09-07 follow-up). This explicit host constructor policy
is bound into the implementation commitment; standard/constrained qualification
deadlines stay unchanged. Source planning, inference, repair, validation and
preparation use the same deadline policy, including already charged elapsed time.
The generic WorkBudget and Attempt remaining-time representation permit up to
600,000 ms so they can carry this policy. That representation ceiling does not
raise other profiles' execution budgets. Final preparation records actual elapsed
time from the returned workflow budget, never a hard-coded standard maximum.
It uses the reviewed system prompt, an additional experimental semantic analysis
guide, full Candidate schema
as Ollama's structured format, and one Candidate example from the reviewed bundle.
The prompt's SCHEMA block is a compact structural glossary derived mechanically
from all reviewed schema definitions, preserving fields, required/optional status,
unions, enums and constants. Numeric/string bounds remain enforced by the full
format schema and host validator. This reduces repetitive CPU prefill without
changing accepted Candidate semantics. Model output remains untrusted, and complete
coverage, exact spans, Registry resolution and native SEM compilation apply.
The provider also supplies at most 1024 host-computed word spans in a BYTE_SPANS
data block, derived from the exact admitted windows. These are position hints,
not proposed semantics or repaired model output. Every returned span still passes
the shared exact-quote validator.
Private record/source intake has no publication or reward authority.

For Qwen3, render the known two-message ChatML template with an empty thinking
block and send `/api/generate` with `raw:true` and the Candidate JSON schema.
This bypasses hidden Ollama chat-template expansion: tokenize the exact rendered
prompt using a tokenizer constructed from verified GGUF vocabulary/merges and
the Qwen2 pre-tokenizer used by Qwen3. No chars/4 estimate. Pin the GGUF SHA-256,
tokenizer metadata, rendering source, Ollama executable and CPU runner/DLL SHA-256, bundle and
runtime options. Reject unsupported GGUF architecture/tokenizer and missing
artifacts. Compare returned prompt/output token counts against admission and
reject truncation, tools, malformed JSON and late output.

The Windows MVP runs an isolated Ollama server per call, on a literal loopback
port with proxy/redirect disabled. Assign the suspended child to a Windows Job
Object before resume, using kill-on-close and an explicit memory ceiling. Its
descendants are owned by that job. Cancellation, timeout, successful completion
and provider drop close/terminate only this job. Do not kill an unrelated user's
Ollama process. Non-Windows support is unavailable until a worker implementation
exists. Limit one live inference across the host's admitted models. Model files
are opened under read-only sharing while the host lives to prevent mutation on
Windows; no model download. CPU-only worker operation bounds native process RAM;
admission reserves 4 GiB for bounded host tokenizers/parsers and gives the remaining
configured reservation to the Windows job. Startup has a 10-second ceiling inside
the 600-second experimental job deadline. Reservations are not measured quality or cross-device qualification.

The semantic guide explains proposition boundaries, predicate/argument roles,
qualifier scope and source-grounded references using simple, conditional and
multi-proposition examples. It cannot extend the Candidate schema: an unstated
numeric tolerance, unresolved pronoun or unrepresentable relation must remain
unresolved/unsupported. Original evidence and complete coverage remain mandatory.
The guide bytes are pinned with the provider source. Thinking remains disabled
through the existing empty closed thinking block in the raw ChatML template.

Schema repair diagnostics are bounded to eight messages of at most 240 characters
each, with schema-owned field paths, array positions, expected types and fixed
validation reasons. They never reflect candidate values or unknown property names.
An invalid Candidate remains rejected by the original validator. A charged,
deadline/cancellation-aware diagnostic walk supplies details for the existing
single repair call, whose complete rendered prompt is token-counted again.
Diagnostics are retained in the encrypted extraction checkpoint (default-empty
for older checkpoints) and projected through the existing private KU failure
limitations. The Web shows them next to the failure without requiring expansion.
No raw candidate is exposed, and no extra inference calls or automatic save are
authorized by diagnostics. Earlier failures cannot acquire retroactive detail.

Source-grounding preflight also checks that each concept label equals its evidence
quote before recording the candidate and closing the existing repair allowance.
`concept_label` details use `grounding:` plus schema-owned paths and fixed repair
instructions to preserve original spelling, case and accents. The same diagnostic
bounds and private checkpoint/API projection apply. Host compilation still checks
exact source bytes, spans and label equality; it never normalizes model output to
make it pass. This preflight does not claim that a Registry binding exists.

The experimental generate request retains the reviewed Candidate schema's JSON
property order using a raw JSON value for `format`. Local Ollama 0.33.3 probes
showed order-sensitive decoding for Span objects. This wire compatibility measure
does not alter schema meaning, bounds, required fields, canonical sorted hashing,
or host validation. It is not a guarantee of model conformance or semantic quality.

Candidate preflight also rejects repeated concept/statement keys and repeated
coverage units before closing the same bounded repair allowance. Diagnostics use
collection positions, not model-generated identifiers. Distinct items require
distinct keys and updated references; coverage retains one entry per required
unit. The host does not deduplicate or merge model content automatically.

Owner follow-up (2026-09-08): development tests run headlessly with deliberately
supplied cases and inspect provider proposals without requiring a Web rebuild.
The shared workflow now preflights source spans, number lexemes and candidate
compilation using empty bindings before closing the existing repair allowance.
No verified Registry resolution is invented by that preflight, and the final
authority-bound compile remains mandatory. Numeric source hints contain exact
substring offsets (including the number in an attached-unit token), without
asserting units or meanings. Frequency/typicality must not be silently lowered
to an unconditional claim in this restricted extraction profile.

The opt-in `extraction_probe` development example uses synthetic source/Registry
context and no storage authority. Its caller-selected private report intentionally
retains source, per-call proposals, metrics and repair errors. This explicit test
artifact is separate from runtime logs/APIs; it must not process secrets or blind
holdouts. A separate semantic-review proposal experiment under `scripts/encoder`
tests a portable, quote-based task with host-computed anchors and future independent
verification. That experiment is not an admitted KU input format or a Web feature.

The host can restore encrypted text custody with zero admitted models. Missing,
changed or removed model artifacts disable new AI work while existing private
KU remains readable. Re-enabling a model requires host restart and re-verification.

The UI selects model before intake, displays experimental state and source
consent, then calls the existing reservation/intake/prepare/preview/save flow.
Poll authenticated status for pending operations; cancel must remain accessible
while inference is running. Display/reconnect cannot resample or save. All
source, prompts, candidates, IDs and receipts remain out of URLs/logs/WS.

Official transport/template references: [Ollama raw generation](https://docs.ollama.com/api/generate)
and [Qwen3 tokenizer template](https://huggingface.co/Qwen/Qwen3-8B/blob/21073ac5a57f8ac6b159dae129728af51ac707e8/tokenizer_config.json).
