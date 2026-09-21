# KU-WEB-001: schema diagnostics and repair feedback

Owner authorized implementation after repeated experimental Ollama `oneof`
failures and cancellation of the old reservation.

The host now derives bounded field paths from the Candidate schema when valid
JSON fails schema validation. Diagnostics identify missing fields, incompatible
Term kinds and other schema constraints. The existing single repair call receives
these details together with the original error code. Validation, Registry
resolution, token limits and the experimental 600-second deadline remain enforced.
This is a diagnostic improvement, not relaxed KU acceptance or additional retries.

At most eight messages of 240 characters are retained in the encrypted extraction
checkpoint and returned through authenticated API failure limitations. Paths use
schema field names and array indices; unknown model keys, values and source quotes
are not reflected. Older checkpoints deserialize with empty diagnostics. The Web
shows field issues directly and explains schema failures without implying that
Ollama disconnected. Reconciliation continues to preserve the original error.

Verification:

- Extraction tests: 21 pass, one real-worker test explicitly ignored. A malformed
  quantity identifies missing fields without reflecting private values; a repair
  provider receives the detailed path and returns an accepted candidate.
- KU API tests: 12 pass, one real-model test explicitly ignored. Repeated invalid
  output exposes diagnostics, consumes only the existing two calls, and cannot
  save. Cancellation remains reachable.
- Node KU tests: 22 pass, including process-crash and restart recovery.
- Web: 14 tests and production build pass, including visible schema diagnostics.
- Rust format and diff whitespace checks pass.
- Global vNext contract validation passes. The rebuilt local host is running;
  authenticated startup checks return Web HTTP 200, Registry ready, local encoder
  ready and admitted `qwen3:8b`. Model qualification remains false.

The prior Vietnamese failure cannot be diagnosed to an exact field retrospectively:
its invalid candidate was not retained with these new diagnostics. No new real
Vietnamese inference or model-quality qualification is claimed by these tests.
New attempts use this feedback path; previous outcomes do not gain details.

## Follow-up: concept_label

The owner subsequently reported `concept_label`. Its compiler guard requires
`concept.label == concept.evidence.quote`, after checking the evidence span. This
is a source-grounding mismatch; it does not establish missing Registry coverage.
Previously this check occurred after the structural repair loop had closed.

A bounded preflight now detects this mismatch before candidate recording and
feeds schema-owned field paths plus exact-copy instructions into the same repair
allowance. It never changes the candidate itself. Final compiler source/span and
Registry checks remain mandatory. Private diagnostics also admit the fixed
`grounding:` prefix. Web feedback explains the code even for old responses without
field details. Other semantic compiler failures are not automatically made
repairable by this targeted change.

Extraction tests pass (22, one owned-worker test ignored), covering a successful
label repair and repeated invalid labels with no recorded candidate. Web tests
pass (15) and production build succeeds. Real Vietnamese inference has not been
rerun as part of this follow-up; its actual mismatched strings are not inferred
from the screenshot.

KU API tests pass (13, one real-model test ignored), including encrypted checkpoint
to private API propagation of concept-label diagnostics without source values.
Node KU tests pass (22), including cancellation and process recovery. Global vNext
contract validation, formatting and diff whitespace checks pass.

## Follow-up: missing Span.start and inference schema order

Owner screenshots showed missing `start` in concept evidence, argument evidence
and statement evidence; an argument also failed its enclosing `oneof`. The source
schema requires all three Span fields. Host serialization through ordinary
`serde_json::Value` sorted properties to `end,quote,start` on the inference wire.

Local Ollama 0.33.3 / qwen3:8b probes used the same synthetic Water prompt and
schema bounds. With `start,end,quote`, the model returned all fields correctly
even when instructed to omit start (9.32 seconds). With `end,quote,start`, it
returned malformed/truncated output (5.38 seconds); reducing quote maxLength to
64 made the JSON complete but left corrupted quote text (3.81 seconds). With
`end,start,quote`, all fields were correct (1.69 seconds). These are controlled
development probes, not semantic qualification or proof of the exact cause of
the historical screenshot. Probe service was unloaded and stopped afterward.

The generate request now uses raw JSON only for its reviewed `format` schema,
preserving property order. Canonical hashing and validation still use sorted
Values. No bounds or required fields change and no missing field is filled in by
the host. The regression test checks retained wire order and semantic equality.

Validation: 23 extraction tests pass (one owned-worker test ignored), including
the wire-order regression and unchanged corpus oracles. Global vNext, formatting
and whitespace checks pass. The real qwen3:8b development roundtrip passes with
the new wire format: `Copper is conductive.`, inference/validation 146.761 seconds,
HTTP 200, ready preview, private save and reopen/read in an isolated signed test
Registry. Total 157.33 seconds. This does not qualify Vietnamese/model quality.

The owner's Vietnamese development sentence was then exercised against the real
local Registry, preview only: `Nước sôi ở khoảng 100oC ở gần mặt nước biển`.
After 223.02 seconds, schema validation advanced but compilation failed with
`duplicate_id`. No KU was saved or shared; the test's own reservation was canceled.
This is explicitly not a successful Vietnamese encode. Repeated concept/statement
keys and coverage units are now checked before recording the candidate, with
bounded collection-index diagnostics supplied to the existing repair call.
The Web explains duplicate identifiers as model output errors. No automatic
deduplication, source rewriting, extra calls or relaxed validation is introduced.

After this preflight addition: 24 extraction tests pass, including successful
repair and exhausted repair for duplicate keys; API invalid-output/cancel and
node extraction crash tests pass. Web tests pass (16) and production build passes.
The rebuilt local host uses the owner's unchanged signed Registry/configuration.

Second real Vietnamese preview after duplicate-key preflight: 310.86 seconds,
failure `unsupported_number`, no prepared preview. Its own reservation was
canceled; neither Vietnamese probe saved or published a KU. This demonstrates
that the full Vietnamese workflow remains unsuccessful. The error comes from
numeric compilation; the exact offending model string was not captured in the
test report, so no specific quote/value is inferred. Further numeric-grounding
diagnostics and a complete end-to-end correction remain outstanding. Increasing
timeouts or the successful English development fixture does not resolve this.
