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
the standard limits. It uses the reviewed system prompt, full Candidate schema
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
the 120-second job deadline. Reservations are not measured quality or cross-device qualification.

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
