# Local KU manual editor v1

KU-WEB-001 additive host integration for the [local REST profile](KU_LOCAL_REST_PROFILE_V1.md).
No Base discriminator, canonical schema, authority or rollout changes.

D-023 adds opt-in `models` and `encode_text` actions through
[the experimental Ollama profile](KU_EXPERIMENTAL_OLLAMA_PROFILE_V1.md).
The manual actions and their custody requirements below retain their meaning.

The opt-in host installs `ManualKuInputs` with one principal, a verified signed
Registry and a bounded set of canonical private Text SourceArtifacts explicitly
admitted by the local operator. Admission is a host custody decision; decoding a
source or possessing its CID does not grant access. The operator supplies original
source objects and a stable Vault key through local files, never through the Web.
No synthetic source governance, test Registry, fallback CCID or model is installed.

`POST /api/vnext/ku/editor` accepts `{session,budget,request}` under the existing
Bearer, generation, JSON, 1 MiB, no-store and private error policies. `request` is
a closed tagged union using `action` and `payload`:

| action | payload | result payload |
|---|---|---|
| catalog | `{}` | `{sources:[{source_ref,label}], limitations}` |
| resolve | `{label}` | `{candidates:[{ccid}], limitations}` |
| draft | `{operation_id,idempotency_key,source_ref,predicate_label,selected_ccid?,argument_text}` | generated `KuPrepareV1` |

Catalog returns only the authenticated principal's host-admitted sources (at most
64, labels at most 128 UTF-8 bytes). Resolve accepts at most 256 UTF-8 bytes and
returns at most 64 exact candidates from the pinned release, without selecting
one. Draft accepts one source and at most 4096 bytes of nonempty NFC literal text.
This finite editor creates one manually asserted predicate with one text argument
and a private whole-source span; it does not infer natural-language meaning,
negation, units or truth. Users explicitly select a returned CCID. Missing selection
creates a preparation with `needs_resolution`, no invented predicate or saveable
artifact. A selected CCID must match the exact Registry lookup again at prepare.

Host generates the draft reference and supplies source, implementation and release
commitments. Draft admission is volatile, bounded to 256 drafts / 4 MiB encoded
requests per process. Exact reuse of an operation and request returns its original
template; changed reuse conflicts. Admission does not prepare or save. The node
checks reservation ownership again on prepare. Prepare/revise and explicit save
use the existing eleven-operation service, with `LOCAL_ONLY` destination.

Prepared bytes and saved bundles use existing encrypted durable journals. After
restart, refresh generations and reconcile the original operation; never replay
draft admission/extraction automatically. Unprepared drafts expire with the
process. Host sources must remain admitted for preparation/save/recovery requiring
custody. The host's read-only admitted source catalog is frozen until restart;
remove admission by stopping the host and changing its configuration.

The Web keeps private draft, results and pending IDs in memory only, sends no KU
payload to debug logs, WS, URLs or browser persistence, and makes no automatic
mutation retries. On an ambiguous response it retains the operation ID and gates
further mutations on explicit reconciliation. Refreshing the page loses this
in-memory state; the user can enter the original operation ID to recover it.
Local reads and manual editing do not depend on network/AI readiness. Save is
private, not publication, Use, adoption, fidelity acceptance or reward issuance.

The bounded `ku_local_web` host example is an explicit launch, with operator
provided Registry trust key, admitted sources and Vault key. It is not default
CLI/Desktop lifecycle integration or a self-provisioning production installation.
