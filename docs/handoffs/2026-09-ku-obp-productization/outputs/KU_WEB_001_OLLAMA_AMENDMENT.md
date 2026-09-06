# KU-WEB-001 — requested Ollama MVP amendment

Status: owner approved under D-023; implementation and development verification complete.
See [Ollama implementation and run instructions](KU_WEB_001_OLLAMA_IMPLEMENTATION.md).
The owner accepted this amendment after checkpoint `4a0a1bf`. Owner request, 2026-09-06: the Web MVP must
support Ollama, model selection including `qwen3:8b`, and text encoding through
the previously designed architecture. This requests actual inference, not a
manual editor presented as AI, and expands the current Web deliverable.

## Exact conflict requiring owner direction

[Local REST profile](../../../specs/vnext/KU_LOCAL_REST_PROFILE_V1.md), responses
and capability meaning, currently says:

> qualified real-model tuple; AI prepare/revise is rejected before dispatch.

and:

> Enabling real AI requires the separate measured qualification integration.

The [accepted encoder implementation boundary](KU_ENC_002_IMPLEMENTATION.md)
also requires KU-ENC-003 weight/tokenizer/chat-template bindings, measured memory
and worker cancellation before enabling an Ollama tuple. KU-ENC-003 is still
blocked on locked independent evaluation inputs; no tuple is qualified.

Enabling `local_ai` today under those unchanged statements would conflict with
the requested usable Ollama MVP. D-021 allowed manual/no-LLM MVP and did not
explicitly introduce an unqualified inference lane. Package README requires
recording canonical conflicts and requesting owner direction, so the existing
API rejection has not been removed. This is not a request to approve the already
accepted shared-encoder architecture again.

## Recommended bounded resolution

Authorize a **local experimental Ollama lane before full KU-ENC-003 quality
qualification**, exclusively for the opt-in MVP host, with these boundaries:

1. Keep `model_qualified: false`. Display “Experimental — model not qualified”
   separately from “Ollama connected”, “model installed” and operation state.
   Inference availability must not imply semantic accuracy or release readiness.
2. The host lists installed local Ollama models and admits configured models;
   `qwen3:8b` is the initial requested model. Pin the selected tag to its verified
   local artifact, tokenizer, template and backend identities per job. Switching
   model creates new work; it cannot reinterpret/replay an existing preparation.
   Do not download a model automatically or route source text to a cloud service.
3. Provide actual text intake: the user enters text and explicitly authorizes
   local encoding. The host preserves exact UTF-8 bytes and records real custody,
   capture and consent provenance before constructing a source reference. Reuse
   existing capture/governance owners; never fabricate governance commitments,
   ask the browser to invent a SourceArtifactCID, or reuse fixture custody.
4. Route the source through `SharedKuExtractionInputs` → `ExtractionWorkflow`
   → bounded Ollama provider → strict Candidate validation → pinned Registry
   resolution → native SEM compiler → KU preparation. No legacy tool-agent,
   CoreDna fallback, direct model-to-Vault path or browser-side compilation.
5. Show bounded operation progress, cancel and explicit unresolved/error states.
   Valid complete output becomes the existing exact preview. Save remains a
   separate explicit private action. Missing concepts, ambiguous choices,
   unsupported semantics and incomplete coverage cannot become saveable output.
6. Preserve actual token/context accounting, weight/tokenizer integrity,
   bounded concurrency/calls/deadline/memory admission, cancellation/late-output
   fences, encrypted journals and explicit reconciliation. The proposed exception
   is to waiting for full quality qualification, not to these technical controls.
   Any necessary change to a technical contract must be specified separately,
   rather than replacing exact counts with guessed token estimates.
7. Continue on `codex/ku-web-001-workflow`; specify the additive host/API fields
   before implementation. Update the REST activation policy and host-editor
   contract with the approved experimental exception. Preserve the frozen encoder
   schema/SEM meanings and default-off behavior of other hosts.

## Evidence required for the amended MVP

- Real local `qwen3:8b` inference on explicitly designated development text,
  through the same node/API path used by Web. Never use private VI/EN holdouts.
- Record verified model/backend/tokenizer/template pins and actual observable
  limits. Report unresolved output honestly; do not manufacture a successful KU.
- Tests for installed/missing model and Ollama outage, model binding, private
  source intake, strict output rejection, exact preview/save, cancellation,
  stale generation, lost responses and recovery without resampling.
- Web component/accessibility tests, production build, focused native suites,
  unchanged generated contracts, validator, run instructions and a pushed branch.
- No claim of qualified extraction accuracy, cross-model convergence, mobile
  support, production readiness, publication, reward or owner-approved merge.

## Read-only preflight

`ollama list` on this Windows host reported Ollama 0.33.3 and installed
`qwen3:8b`, displayed model ID `500a1f067a9f`, size 5.2 GB. This is availability
evidence only: the short tag ID is not a verified weight/tokenizer manifest.
Other installed models were listed; none was selected or invoked.

The native shared workflow and bounded Ollama adapter already exist. The current
adapter requires a host `ExtractionTokenizer`; product integration must supply
its real implementation and bind the actual chat wrapper. Merely removing the
API AI rejection and calling the legacy `/api/encode` would not satisfy this task.
No inference, model download, holdout read, runtime change or qualification run
was performed during this preflight.
