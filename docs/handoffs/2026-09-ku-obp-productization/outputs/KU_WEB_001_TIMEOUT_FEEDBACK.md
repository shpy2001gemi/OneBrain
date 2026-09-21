# Local Ollama timeout feedback — 2026-09-07

Owner reported the Web response `rate_limited / ResourceExhausted / deadline`
and requested more detailed results and an explanation of AI's role.

The existing standard extraction profile gives the complete workflow 120,000 ms.
The node maps the `deadline` reason to `ResourceExhausted`; the registered REST
projection maps that Base code to HTTP 429 / `rate_limited`. The reported reason
therefore identifies a workflow timeout, not an account quota. This response
does not identify which internal stage consumed the budget or prove RAM exhaustion.
The admitted Ollama worker is CPU-only, starts per call, and handles the reviewed
instructions, Candidate schema and source; an optional repair shares the budget.

The Web now presents a plain-language failure explanation, preserves raw error
details, shows observed intake/preparation milestones and browser elapsed time,
and offers reconciliation beside the error. Reconciliation reads the existing
operation and never silently resamples or saves. A collapsible explanation covers
source custody, AI extraction, host validation/Registry resolution, and explicit
private save. Prepared previews also summarize artifact count and saveability.
These displays do not claim token streaming, internal-stage telemetry, factual
truth verification, decoded semantic content or improved inference speed.

Validation: Web suite 12 passing tests (including timeout vs generic resource
failure, blocked retry before reconciliation, no repeated inference/save, and
keyboard/accessibility checks); production build passes; lint passes with four
existing warnings outside the KU changes. The built Web is served by the existing
local host. No backend restart, policy/deadline change, new inference or save was
performed for this feedback update. The owner retains the current page/operation
until they reconcile and reload.

The owner-authorized local setup is outside Git at
`C:\Users\shpy2\Documents\OneBrainLocal`. Its README documents launch, token,
data and signed local Registry provenance limits. No private keys, source text,
tokens, prompts or runtime receipts are included here. AI remains unqualified.
