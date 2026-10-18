# Local KU manual editor v1

KU-WEB-001 additive host integration for the [local REST profile](KU_LOCAL_REST_PROFILE_V1.md).
No Base discriminator, canonical schema, authority or rollout changes.

D-023 adds opt-in `models` and `encode_text` actions through
[the experimental Ollama profile](KU_EXPERIMENTAL_OLLAMA_PROFILE_V1.md).
The owner-approved [review-draft profile](KU_REVIEW_DRAFT_PROFILE_V1.md) adds
private `review_start/list/get/resume/cancel` background work through the same
authenticated envelope. Draft readiness does not authorize canonical KU saving.
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
| share_prepare | `{object_cid,expected_node_id,address,selector,namespace}` | `{event:"share_prepared",prepared_id,intent_id,public_object_cid,semantic_content_cid,canonical_base64,semantic,expected_node_id,address,selector,namespace,expires_at,consequence,enqueued:false,payload_sent:false,support}` |
| share_confirm | `{prepared_id,confirm_public}` | `{event:"share_confirmed",intent_id,public_object_cid,outbox,delivery,authorizes_reward:false}` |
| share_intent | `{intent_id}` | `{event:"intent",intent_id,public_object_cid,state,terminal_sequence,claims_network_completion:false}` |
| share_inspect | `{object_cid,selector}` | `{event:"public_object",object_cid,canonical_base64,semantic,authenticated_source_peers,provenance,fidelity:"unassessed",authorizes_reward:false}` |

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

For a deliberate developer-owned manual demonstration, the opt-in
`ku_manual_source` provisioning example accepts an operator JSON request with
`operator`, `text_file`, `output_dir` and `consent_local_private: true`. It reads
at most 8192 exact UTF-8 bytes, creates a new output directory and writes one
private canonical Text SourceArtifact plus its actual local policy, consent
receipt, scope, assessment, retention, frontier and adapter records. Context-local
opaque reference kind 0 binds their SHA-256 commitments, as in the existing
text-intake producer; these records are neither canonical public policy objects
nor publication grants. Consent covers manual local encoding and private retention
until operator removal, with no model call. Missing/false consent fails before
creating output. Existing directories are never overwritten.

The operator must own or be permitted to capture the text, retain the complete
output under private filesystem custody outside Git, and explicitly add
`source.canonical` to the host's `sources` configuration. Provisioning alone does
not admit a source to a principal or save a KU. The manual host remains the custody
owner; removing admission requires stopping it and changing its source list.

The [shared semantic selection extension](KU_SEMANTIC_SELECTION_PROFILE_V1.md)
uses the same private review-job transport and node lifecycle. New jobs expose
`draft_extracted` when host assembly succeeds, with semantic/factual verification
`unassessed`; clients must not translate that into the older self-reviewed
`draft_ready`. Model wire choices and assembly rules live in the common encoder,
not in Web/CLI/Desktop or platform bridges. Previously recorded drafts remain
readable with their original profile and state.

Under D-045 (`INT-KU-OBP-001`), the same authenticated `POST /api/vnext/ku/editor`
envelope projects explicit Public KU sharing (`share_prepare`, `share_confirm`,
`share_intent`, `share_inspect`) without adding unregistered HTTP routes or enabling
networking by default. `share_prepare` constructs a new `DisclosureClass::Public`
manual kernel (one resolved predicate + one text literal, same `SemanticContentCID`,
new `public_object_cid`) in a bounded in-memory coordinator (max 16 prepared items,
900-second expiry) without returning the private `ObjectCID`, private source bytes,
or source spans, and without enqueueing or sending any payload. `share_confirm`
requires `confirm_public: true`, rechecks current caller access to the saved
private KU, requires an active opt-in `VNextNetworkRuntime` attached to the host's
canonical store (failing closed with `capability_disabled` /
`ku_public_share_network_unavailable` when local-only), consumes the single-use
`prepared_id`, and enqueues the durable outbound transfer intent while reporting
delivery as pending (`authorizes_reward: false`). `share_intent` inspects the
sender's durable outbox state (`claims_network_completion: false`), and
`share_inspect` inspects an accepted Public KU and its authenticated transport/selector
provenance (`fidelity: "unassessed"`, `authorizes_reward: false`) on the receiver;
accepted Public KUs are also readable through registered `POST /api/vnext/ku/operations`
`get` and `export` (`mode: "canonical_public_exchange"`). The `/ku` Web client
(`createKuClient`) and workspace (`KuWorkflowPage`) project these four actions
from the currently inspected `LOCAL_ONLY` `accepted_known` artifact with a
separate preview-then-confirm gesture (`confirm_public: true`), clear stale
`shareConfirmed` (`Confirmed public share intent`) when inspecting an artifact
in `Saved local knowledge` or when editing any share preparation input
(`sharePeer`, `shareAddress`, `shareSelector`, `shareNamespace`), prefill
`Accepted Public ObjectCID` (`shareInspectCid`) and `Accepted selector CID`
(`shareInspectSelector`) while clearing stale `shareInspectView` when
`share_confirm` succeeds, explain and prefill `Accepted Public ObjectCID`
(`shareInspectCid`) while clearing stale `shareInspectView` when the inspected
artifact is already `PUBLIC`, preserve the
in-memory preview when `share_confirm` returns `503 capability_disabled`
(`ku_public_share_network_unavailable`), `explicit_public_confirmation_required`
or `invalid_clock`, clear stale or invalidated in-memory
preview state with re-prepare guidance when `share_confirm` returns
`prepared_share_missing`, `public_ku_consent_expired`,
`saved_access_changed` or `outbox_unavailable_reprepare_same_intent`, clear
stale `shareIntentView` and `shareInspectView` cards when lookup inputs change
or a subsequent inspect call fails, explain bounded D-045 refusals,
validation/system checks, and storage/outbox errors (`unsupported_public_ku`,
`prepared_limit`, `saved_ku_not_accessible`, `outbox_unavailable`,
`public_store_unavailable`, `provenance_unavailable`,
`public_object_not_found`, `intent_not_found`,
`explicit_public_confirmation_required`, `invalid_saved_ku`,
`invalid_public_ku`, `invalid_public_ku_intent`, `invalid_peer`,
`invalid_peer_address`, `invalid_selector`, `invalid_namespace`,
`invalid_public_identity`, `invalid_intent_identity`,
`consent_randomness_unavailable`, `invalid_clock`), keep private `ObjectCID` and
source bytes out of the share preview/inspect UI, and distinguish durable outbox
intent creation from peer delivery. In `ku_local_web`, `network` is omitted by
default so normal launches start no network listener; when the operator
explicitly configures `"network": { "network_opt_in": true, "bind_addr": "127.0.0.1:0" }`
and runs `ku_local_web` with `--features vnext-network-runtime`,
`start_host_loopback_network_runtime` validates the loopback opt-in before
touching Registry or dataset state, starts a loopback `VNextNetworkRuntime`
after node/Base initialization, and binds it via `attach_live_public_reader` so
the `/ku` Web share confirmation and peer inspection actions run end-to-end.
