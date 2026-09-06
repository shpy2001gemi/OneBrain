# KU-ENC-002 implementation evidence

Date: 2026-09-06. Branch: `codex/ku-enc-002-shared-encoder`.
Starting main: `5faf50cd8b1319c63bc1eabd7b294a94d410821b`.
Authority: D-018/D-019 and the owner's instruction to merge KU-ENC-001 and
implement KU-ENC-002. Contract merge `22599d0` is on `origin/main`.

## Delivered behavior

The host owns source admission, Registry lookup, call scheduling, validation,
replay and preparation. A model receives bounded windows, opaque option keys and
the reviewed Candidate schema; it returns raw proposals. It receives no tool,
source custody, journal, Vault or service capability. Existing KU-RUN-001 owns
confirmation and accepted writes. No route into the legacy CoreDna encoder is
used by this implementation.

- [Shared native extraction module](../../../../src/ku-encoder/src/extraction/mod.rs):
  strict JSON rejects duplicate keys, floats, malformed UTF-8, truncation,
  oversized payloads and excessive nesting. The validator consumes the frozen
  schema directly. The native compiler checks exact UTF-8 spans, coverage,
  source-bound qualifiers, full CCIDs, exact rational units and closed statement
  references before producing existing SEM types. It matches all 48 reviewed
  corpus oracles; these fixtures do not prove that a model understood a source.
- [Workflow](../../../../src/ku-encoder/src/extraction/workflow.rs): one live call,
  durable reservation before dispatch, at most two calls per context, one shared
  work/token/deadline allowance and cancellation checks. Invalid JSON/schema may
  receive one repair call; invalid semantic candidates fail closed. Context or
  provider changes do not become exact replay. Complete recorded candidates can
  be revalidated in the same process without sampling again. Another process
  interrupts the attempt and leaves reconciliation to Base.
- Manifest assembly rejects duplicate attempts, mixed source/Registry/profile
  bindings, reversed/overlapping focus and coverage. It preserves chunk/frame
  order and rebases argument, condition, time, location and perspective statement
  references. Any unresolved chunk removes all output drafts. Both reviewed
  multi-chunk jobs run through the native workflow.
- [Node bridge](../../../../src/onebrain-node/src/ku_extraction.rs): host-installed
  source custody port, immutable typed SourceArtifact validation and the existing
  signed pinned indexed Registry lease. Exact selection requires a complete unique
  match. Ambiguity, unverified unit metadata and model suggestions never authorize
  a CCID. Implementation commitments bind native source, lockfile, bundle,
  provider manifest, planner and resource profile; byte-bound files use LF.
- [KU service](../../../../src/onebrain-node/src/ku_product.rs): async extraction
  releases the service mutation lock so Cancel reaches the running provider.
  Current custody is checked before reads, checkpoints, preparation and accepted
  writes. Remaining extraction work and monotonic deadline continue through
  preparation. Incomplete output produces `NeedsResolution`,
  `extraction_incomplete`, no artifact IDs and no saveable KU.
- Attempt/context/candidate/resolution evidence is encrypted in an additional
  table of the existing KU journal, using the existing Vault metadata purpose
  with a distinct binding. Each table is bounded to 1,024 operation records;
  both share the 64 MiB ciphertext cap. Reservations and monotonically increasing
  counters commit before inference. A staged bundle cannot supply preview or
  exact replay until extraction has recorded successful preparation.
- Real process exits at reservation, candidate, validated and staged-bundle
  boundaries recover without model calls or partial public/accepted artifacts.
  Reconciliation now handles a missing prepared bundle as no completed save,
  closing an interrupted preparation as failed. Existing save crash tests and
  exact prepared/saved replay continue to pass.
- [Ollama adapter](../../../../src/ku-encoder/src/extraction/provider.rs): a
  provider-neutral interface and mandatory host tokenizer port bind the full
  schema/prompt/examples/chat wrapper to token accounting. The bounded backend
  route preserves raw candidate JSON, supplies no tools, disables thinking,
  limits request/response bytes, bounds HTTP time and discards private error
  bodies. It accepts literal loopback HTTP addresses only, disables proxies and
  redirects, and makes no download or warm-up request.

## Offline rule and integration boundary

`NoLlmProvider` supports the explicit source form
`@ku1 ("subject") [predicate] ("object")`. The complete required unit must match;
the predicate still needs unique verified Registry resolution. Subject/object
values are exact text literals. Arbitrary prose becomes unsupported coverage,
with zero model calls. The existing resolved-SEM draft provider remains available.

Install `SharedKuExtractionInputs` through the existing `KuRuntimeConfig.inputs`
host configuration. Supply a `KuExtractionSources` implementation that enforces
real principal/grant/revocation policy, a verified Registry generation and one
admitted `ExtractionWorkflow`. Product handles cannot install these ports.
Neither a source ID nor a ProviderManifest is an authorization grant.

The first concrete node planner admits **one whole text source per job**, up to
8,192 Unicode characters within SourceArtifact/work/byte bounds. It does not
guess paragraph boundaries, omit regions or combine different sources. Larger
or multiple-source requests fail explicitly. The shared workflow supports the
contract's bounded multi-chunk manifest when a trusted planner supplies it.
Registry mention discovery considers one-to-four-token spans plus the explicit
bracketed predicate; an unavailable option cannot be invented by the model.

The current indexed Registry exposes CCID/category/labels, without authenticated
affine unit metadata or principal-bound review receipts. The node bridge abstains
on units and ambiguous selections. Native mapping tests exercise authenticated
unit/review *fixture preconditions*; no live unit-catalog or review-UI integration
is claimed. An implementation of those host authorities must preserve the frozen
contract before expanding the admitted surface.

## Identity and evidence limits

The compiler preserves complete private SEM source evidence. The existing
`ku-semantic-content/1.0` product normalizer then removes private source spans
from semantic identity and preserves the private original separately. Tests show
different source artifacts change private canonical bytes while identical
normalized semantic content keeps its semantic CID. This does not change generic
SEM identity, Base IDL, object profiles or canonical encoders.

No real model, physical constrained host or mobile lane is qualified here.
Inference fixtures use synthetic manifests/token counts; HTTP tests use a local
mock server. A manifest memory reservation is an admission declaration, not
measured peak RAM or an OS-enforced allocation limit. Tests establish bounded
payload/work/call admission and cancellation/deadline outcomes, not real model
OOM behavior, weight integrity or semantic quality.

Before enabling an Ollama tuple, KU-ENC-003 must bind the configured model tag
to verified weights, supply the exact tokenizer/chat template, measure complete
peak memory and establish backend cancellation/worker termination. Dropping the
HTTP future discards late output; it does not by itself prove that an external
Ollama worker stopped. If the worker cannot be stopped, that tuple needs a managed
isolated worker and remains unqualified until then. No default model, automatic
download, public rollout, mobile change or quality/convergence claim is included.

## Verification

Commands run locally on Windows; Cargo commands use `src` as working directory.
Counts below are overlapping suites, not independent samples.

| Command | Result / evidence |
|---|---|
| `cargo test --locked -q -p ku-encoder --lib` | 153 pass; 16 extraction tests, all 48 mapping cases and two multi-chunk jobs. |
| `cargo test --locked -q -p onebrain-node --lib` | 123 pass; includes 19 KU tests, cancellation, encrypted storage, exact replay, four extraction process-kill phases and existing save crash matrix. |
| `cargo test --locked -q -p ku-ai --lib` | 109 pass; includes three bounded HTTP/endpoint tests using local fixtures. |
| `cargo test --locked -q -p ku-core --lib foundation::semantic::tests` | Seven existing SEM tests pass. |
| `cargo check --locked --workspace` | Pass on the local Windows default-feature workspace. |
| `cargo clippy --locked -p ku-encoder -p ku-ai -p onebrain-node --lib --tests -- --cap-lints warn` | Pass with warnings under the existing CI warning policy; not a zero-warning claim. |
| `cargo fmt --all -- --check` | Pass. |
| `python -m scripts.encoder.generate_bundle --check` | Eight generated artifacts unchanged. |
| `python -m unittest scripts.encoder.test_contract scripts.ci.test_validate_ku_product_contract scripts.ci.test_validate_vnext_product_profile` | 62 pass. |
| `python scripts/ci/validate_vnext_contracts.py` | Pass, including encoder, KU, Base, Registry and normative/link checks. |
| `git diff --check` | Pass. |

The existing foundation workflow now runs the native extraction, bounded provider
and node KU suites explicitly. Local results do not substitute for execution of
the remote CI OS lanes or KU-ENC-003 qualification.
