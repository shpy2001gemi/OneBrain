# OneBrain vNext — Shared KU product workflow v1

> Task: `KU-CON-001` — **owner approved**; additive registration implemented by `KU-RUN-001` under D-016.
> Profile: `KU_PRODUCT_WORKFLOW_PROFILE_V1`, version `1.0`.
> Machine inventory: [ku-product-workflow-v1.json](../../../src/test-vectors/vnext/ku-product-workflow-v1.json).
> Baseline: `d8effb772b0cb7766e91b799dd598061a81a9df5`.

## 1. Authority and approval boundary

This profile specifies a local KU service for node, REST, CLI, Web and
Desktop. The owner accepted KU-PC-A/B/C on 2026-09-05 in
[D-015](../../handoffs/2026-09-ku-obp-productization/DECISIONS.md#d-015--ku-product-contract-accepted).
Requirements below are the approved contract; they do not claim current implementation. The
[authority audit](../../handoffs/2026-09-ku-obp-productization/outputs/KU_AUTHORITY_AUDIT.md),
[runtime map](../../handoffs/2026-09-ku-obp-productization/outputs/KU_RUNTIME_GAP_MAP.md)
and [D-011–D-014](../../handoffs/2026-09-ku-obp-productization/DECISIONS.md#d-011--deterministic-identity-after-semantic-normalization)
are the traceability baseline. No existing canonical bytes or contracts are
reinterpreted by accepting this additive profile.

| Accepted item | Approved design | Remaining implementation condition |
|---|---|---|
| KU-PC-A | Add a typed `SemanticContentCID` under digest domain `semantic-content/1`, for the finite normalization in §2. | Append-only domain registration and golden separation/equality vectors before production hashing. No numeric domain/schema ID is allocated here. |
| KU-PC-B | Add the eleven local operations and typed DTOs in the machine inventory, with private save and explicit preparation/confirmation. | Generated Base payload registration/compatibility revision before dispatch; no local-command IDs, new REST paths, CLI spellings or WS events are assigned here. |
| KU-PC-C | Add a private durable revision journal/projection that relates exact predecessor/successor objects without changing either object. | Local management metadata, not an Assembly revision event or replicated author claim. Replicated supersession requires a separate event contract. |

The registration/vector gates are now bound by the machine inventory's
`registration` record, Base profile 1.2 and the independent semantic golden
corpus. The original D-015 approval snapshot remains historical evidence.
`BaseLocalCommandV1.kind` is a closed
registration boundary, not permission to invent numbers. A future API adapter
uses `/api/vnext/...` and the existing envelopes; it does not reinterpret
`/api/encode`, `/api/kus`, `/api/kql` or the read-only six-stage workflow.

Change-control traceability:

| Contract clause | Existing owner / affected contract | Required migration or downgrade |
|---|---|---|
| KU-PC-A, §2 | FND-001 ownership; FND-003 canonical/domain profile; KU-001 SEM; OBJ-001/002 object identity; D-011 | New typed comparison digest and domain vectors; preserve every original ObjectCID/legacy byte family. Unsupported profile has no semantic comparison identity. |
| KU-PC-B, §§3–6 | [product integration ADR](VNEXT_PRODUCT_INTEGRATION_ADR_V1.md), DR-P1.1/P3 projections; Base Task 14 IDL and history; D-008 | Additive negotiated payload registration and generated projections; old hosts reject unknown operations and keep legacy reads explicit. Existing routes and numeric discriminators retain meaning. |
| KU-PC-C, §5 | FND-001 ownership, MIG-001 journal discipline; D-011 | Private local journal only, no invented replicated event; preserved predecessor bytes and concurrent successors. |
| §7 | D-012 Registry, D-013 CAP/FID, D-014 baseline/FID economic amendment | Separate scoped specification work before new distribution/jobs/settlement. No activation, legacy reward fallback or benefit-only substitution. |

## 2. KU identity and supported semantic normalization

A product KU has an exact stored artifact and a separately typed semantic
comparison identity. `object_cid` always names the full existing generic
object envelope, verified under `object/1`. `legacy_wire_cid` remains a
different byte family. Neither is renamed or cast to `semantic_content_cid`.
References, retrieval, authority, import and storage dedup use the exact
artifact CID; semantic comparison is not an alias that can bypass disclosure.
Implementations MUST NOT use SemanticContentCID as an ObjectCID or disclosure grant.

For a newly resolved draft under profile `ku-semantic-content/1.0`:

1. Retain the original source bytes without normalization in private staging
   or an already consented SourceArtifact. Both AI and rules produce the same
   typed `SemanticFrameSet` input; neither writes an accepted KU directly.
2. Resolve every predicate, concept, type and unit to full 16-byte CCIDs using
   one pinned, completely verified signed Registry generation for the run.
   Ambiguity requires a selected CCID with resolution provenance; missing,
   unsupported or conflicting concepts are explicit validation issues. No
   first-candidate rule, local numeric ID or hash-of-label fallback is allowed.
3. Separate source-span qualifiers into a private provenance binding keyed by
   normalized statement position. Preserve their exact source references,
   offsets and mapping from pre-normalized IDs. This is a transformation of a
   new draft, never an in-place rewrite of accepted SemanticFrameSet bytes.
4. Preserve every other semantic qualifier: negation, modality, condition,
   time, location, perspective, tolerance, argument/statement order and typed
   constraints. Source-unit identity and exact affine transform remain.
   Model confidence is run evidence, not semantic certainty; an explicit
   source claim of uncertainty stays a semantic modality.
5. Apply SEM v1 alpha normalization and reduced checked exact rationals. Text
   must already be NFC; non-NFC input is rejected with an issue, while raw
   source remains byte-exact. No paraphrase, case folding, synonym merge,
   commutative reordering or conversion to a preferred unit is inferred.
6. Encode the resulting SEM `1.0` root in `onebrain/canonical/1`. After KU-PC-A
   registration, compute BLAKE3-256 over
   `UTF8("onebrain:vnext:semantic-content:1") || 0x00 || canonical_semantic_root`.

The domain version binds this exact normalization profile. A change to these
rules requires a new identity-domain version, not a silent profile minor.
Equal normalized roots under that profile yield equal semantic CID across
AI/rules/nodes. Equal raw text, merely equivalent quantities in different
source units, different ordering or different profile versions are outside
that guarantee. Registry release changes alone do not change the identity if
all resolved CCIDs and normalized semantics remain identical.

Source/model/node/run/Registry release, signatures, disclosure, reward and
current assessment remain outside this semantic preimage. Each stored
semantic-kernel object still hashes its complete envelope; distinct envelopes
can share a semantic comparison identity while retaining different ObjectCIDs.
Private semantic fingerprints are private identifiers: no public inventory,
WS, logs or telemetry may expose them. A content hash is not a hiding commitment.
Both encoder routes MUST preserve the normalization and provenance boundaries above.

The private preparation/journal binds exact source(s), normalized statement
mapping, compiler/implementation commitment, canonical/profile version,
signed release root, builder/dedup commitments and validation results. Save
commits this provenance binding with its exact object set. Publication later
needs its own reviewed provenance/disclosure contract, never automatic copying
of this private journal into a generic envelope.

## 3. Service ownership and finite operations

OneBrainNode owns the local service through the existing
[Base runtime interface](BASE_V1_RUNTIME_INTERFACE_PROFILE.md). Local KU work
does not require an active network aggregate, listener, reward service or model
when its inputs are already supported locally. It shares existing store owners;
it does not open competing writers or return raw store/signer/path handles.
Callers select allow-listed policies and opaque source references; the service
resolves authority and Vault custody itself. Client fields such as
`authorized`, private keys or a supplied authority frontier are rejected.

DTOs below are operation payloads. They inherit the generated Base envelope's
request ID, session binding, process/dataset generations, operation/idempotency
identities, compatibility and budget; omitting them from a payload does not
remove the enclosing checks. The service derives the principal from the
authenticated session, and typed Base errors retain retry/reconcile fields.
The machine inventory pins `base_envelope_fields` to the current IDL.
`draft_ref` is present only for an already resolved private draft admitted
through bounded intake; rule/AI modes consume exact source references. Neither
reference is a path, unverified producer output or a source-access grant.
Adapters MUST preserve generated Base generation, principal and resource fences.

| Logical operation | Boundary | Result and permitted effect |
|---|---|---|
| `prepare` | reserve → prepare | Bound local encoding/validation, encrypted source/provenance staging and durable exact prepared command; no accepted KU. |
| `preview` | query exact prepared operation | Same canonical object preview and issues, no re-encode, save or other effect. |
| `save` | confirm exact preparation | Commit private canonical objects, provenance binding and durable idempotency result; no publication or Use. |
| `get` | query exact ObjectCID | Authorized exact accepted local artifact; quarantine/unknown schema is non-executable. |
| `list` | bounded snapshot query | Stable full-CID ordering within explicit authorized local store set. |
| `search` | bounded local derived query | Results with index version/frontier/limitations; no peers, StandingNeed or Mapping. |
| `revise` | reserve → prepare | Prepare a new private object with exact predecessor and expected local revision frontier; save remains separate. |
| `export` | bounded public read or Base CreateArchive | Public canonical exchange of already-public records, or separately authorized encrypted private archive. |
| `status` | query operation/service | Capability readiness and durable operation state; no inferred creation, delivery or reward. |
| `cancel` | Base cancel | Cancel eligible reserved/prepared work; release staging under retention rules, never undo committed effects. |
| `reconcile` | Base reconcile | Read/repair the recorded operation to a known outcome; never replay model extraction blindly. |

Prepared results include all outputs, not only the first triple. The
`object_cids`/`artifacts` lists correspond one-to-one: each artifact has its own
exact ObjectCID, SemanticContentCID and canonical preview. List/search return
bounded summaries; exact get returns original accepted bytes. A fixture preview
in the machine inventory tests DTO shape only, not whether its sample bytes
hash to the sample CID. Runtime validation must recompute both typed identities.
Bounds apply
to the whole request/response and aggregate object set, including encoded
transport overhead. This version admits at most 1 MiB product payload, 256
page/output items and 1,000,000 work units, narrowed by existing SEM/object
limits. Resource admission happens before allocation/model dispatch. Larger
sources require a separately admitted source/artifact or archive mechanism;
a raw filesystem path or unbounded attachment is not a service argument.

## 4. Lifecycle, privacy and restart

Base operation states remain `reserved`, `prepared`, `confirming`, `committed`,
`canceled`, `failed`, `unknown_outcome`; legal transitions are copied exactly
from the Base IDL. Validation disposition (`ready`, `needs_resolution`,
`rejected`) is a separate product projection, never a new Base state or truth
judgment. Preparation with issues cannot confirm; corrections require a new
preparation. Prepared means durable intent/staging, not accepted knowledge.

Save accepts only `LOCAL_ONLY` or `NEGOTIATED_ENCRYPTED` destinations. The
default is `LOCAL_ONLY`; no create/save flag selects PUBLIC or publication.
Save MUST NOT publish, create UseEvidence, materialize/adopt a Mapping or authorize reward.
Even a private output derived from public source stays private until a separate
disclosure action. Vault unavailable means dependency failure, never plaintext
fallback. Private validation failures stay encrypted/private quarantine.

The journal binds principal, operation/idempotency IDs, process/dataset
generations, exact command and output bytes, destination, Registry/profile,
source/provenance set and optional predecessor/frontier. Exact replay returns
the same result. Same key with changed content, destination, principal or
predecessor is conflict. Repeating a save under another operation may reuse
byte-identical objects but does not fabricate an independent encoding attempt.

Before any durable write, validate the whole bundle and recheck current
authority, generation, source access and destination constraints. An accepted
object set plus provenance/revision/idempotency metadata needs an atomic
encrypted boundary or a durable intent/recovery protocol that suppresses
product visibility until all required records are durable. Two independent
store writes are not an atomic save. `committed` means that complete finite
boundary survived; a lost reply requires reconcile before retry.
Implementations MUST reconcile unknown outcomes before replaying durable work.

After restart, reacquire the authenticated service under current generations,
then reconcile the original operation ID through its owner. Old handles remain
invalid. Restore prepared bytes from encrypted staging, not a fresh model run
or latest Registry. A missing/corrupt pinned release, payload or source reports
a typed failure/unknown outcome according to the journal, never reconstructed
success. An already committed operation retains its receipt even if the model
or old Registry is no longer available. Rebuildable indexes may lag: exact get
reads accepted state, while list/search reports the assessed snapshot/frontier.

## 5. Reads, revisions and export

Every view retains typed identity, storage/disclosure, artifact validity,
fidelity policy/frontier when present, local lifecycle, coverage and limitations.
Missing fidelity is unassessed, not invalid knowledge. Empty search means no
match in the named authorized snapshot and budget, never network-wide absence.
Private not-found and inaccessible results do not disclose another principal's
holdings. Semantic CID lookup, if added later, returns only authorized artifact
references and cannot choose one envelope globally.

Continuations reuse opaque `obc1` with the REST ceiling of 2,048 characters.
They bind principal, dataset generation, authorized store set, query/filter,
sort/index version, snapshot frontier and last full CID. Changed context is
conflict; evicted snapshot is expired and requires a new query. No offset over
a moving full scan is presented as a stable snapshot. Byte/work budgets can
end a page early, with partial coverage and continuation when resumable.

Revision creates a new artifact plus a local revision relation containing exact
predecessor/successor ObjectCIDs and the scoped expected journal frontier.
Concurrent successors are retained as branches; none silently wins. Stale
expected frontier fails before commit. A byte-identical successor is a no-op
with the same artifact, not a self-cycle. This local relation does not rewrite
Assembly manifests, append an invented signed event or declare supersession
to other nodes. Existing immutable Assembly revision/adoption contracts remain
separate. Local retention is not semantic/global deletion.

Public export uses the existing canonical exchange codec: only previously
accepted PUBLIC records, exact original bytes and valid dependency closure.
Missing/private dependencies produce a bounded explicit error/limitation,
never plaintext substitution. Private export uses Base `CreateArchive` with
host-authorized sink/secret capabilities and the existing encrypted archive
profile. Ordinary service handles cannot manufacture those capabilities.
`KuExportV1` selects the mode and records; it is not an archive grant. The
private result refers to the separately reserved Base archive operation, whose
typed management flow supplies its sink/secret. Public bytes use the bounded
public-record result field. Private plaintext never uses that field.
JSON/CSV remain labeled views and cannot be reimported as canonical evidence.
Legacy reads/export retain original wire bytes and labels; no dual-write or
silent migration is introduced. A profile-unsupported node keeps exact old
artifacts readable and reports unsupported new operations.

## 6. Surface, error and notification projection

The machine inventory owns approved DTO fields and types; it is an inventory,
not an alternate canonical serializer or generated Base IDL. REST/CLI/Web/
Desktop adapters project the same service result. Exact Base errors retain
their discriminator, retryability and reconcile requirement. The outer REST
mapping reuses the existing eight error codes and keeps the typed Base error
in local `KuFailureV1`; unknown outcomes never collapse into an
ordinary retryable HTTP 503 without `reconcile_before_retry`.

No new WS topic/event is proposed. Existing private WS remains bounded hints
with its unchanged vocabulary and privacy suppression. Clients poll/reconcile
through authenticated local operations, including after notification gaps.
The later API task may propose a separate WS extension but cannot smuggle KU
identifiers/payloads into existing runtime/view events. Display/reconnect/
refresh cannot save, materialize, adopt, create UseEvidence or authorize reward.

## 7. Required linked work for D-012–D-014

These are mandatory specification dependencies, not a completed network market
or permission to implement it in this task. Amounts, cadence and settlement
rules are not fabricated here; the machine inventory records each dependency
and the operations it blocks.

| Dependency | Required output before implementation | Local behavior while unavailable |
|---|---|---|
| Registry distribution (D-012) | Signed publisher/peer discovery and bounded chunk transfer, cadence, resume, rollback, trust-key policy and release reproducibility. Preserve the current no-large-OBP-gossip restriction. | Pin one already valid local release; update status does not silently switch an in-flight encode. |
| Delegated work (D-013) | Capability/Manifest/Offer/Permit/Execution integration; opt-in automatic eligible claims, durable leases/attempts, cancellation/reassignment and source consent. | Supported local rule/AI route works; missing encoder reports unavailable. No implicit source upload. |
| Independent fidelity (D-013) | Commit-before-reveal across durable worker attempts; exact source/concept checks, preserved alternatives and evidenced correlation groups. | Saved KU remains locally useful with explicit unassessed fidelity. |
| Direct issuance (D-014) | Versioned economic amendment to baseline and FID no-mint clauses; separate work acceptance, reward authorization and ledger; bounded admission/supply, correct mismatch rewards, replay/correlation abuse, disputes and partition-safe settlement. | No simulated value is called issued OBT. Local validity/save/search/use do not depend on reward processing. |

D-014's trigger is accepted encoding or verification work **without a later
BenefitEvent**. This proposal does not replace it with bounty or benefit-based
vesting. Local save/display by itself is not accepted-work evidence or mint
authorization; a later explicit admitted work workflow may consume the saved
artifact and produce the required acceptance separately.

## 8. Validation and implementation acceptance

The contract validator checks operation/DTO cross-references, bounded types,
Base lifecycle/errors, approval status, private save, identity/provenance
separation, continuation context and linked dependencies. Mutation tests
exercise violations and valid/invalid DTO fixtures. They establish contract
consistency only, not Rust runtime or semantic hashing conformance.

KU-RUN-001 and later QA need golden canonical bytes/domain separation,
cross-encoder/process equality for alpha renaming and source-provenance
variation, and distinct output for negation/order/unit/profile changes.
They also need ambiguity rejection, multi-output atomic save, wrong-key/private
quarantine, crash at each write boundary, exact replay/conflicting reuse,
stale generation and source revocation, concurrent revision, snapshot paging,
bounded export and cross-surface error parity. Network Registry/job/reward
cases belong to their separately scoped dependencies in §7.

Canonical references: [ownership](FIELD_OWNERSHIP_MATRIX_V1.md),
[vocabulary](NORMATIVE_VOCABULARY_V1.md), [objects](IDENTITY_OBJECT_PROFILE_V1.md),
[canonical bytes](CANONICAL_PROFILE_V1.md), [SEM](SEMANTIC_PRIMITIVES_V1.md),
[storage](VALIDATED_STORAGE_PROFILE_V1.md), [Mapping](MAPPING_MATERIALIZATION_PROFILE_V1.md),
[Base authority](BASE_V1_AUTHORITY_AND_RECOVERY_PROFILE.md),
[migration](ADDITIVE_MIGRATION_STORAGE_PROFILE_V1.md),
[Registry](CONCEPT_REGISTRY_OPERATIONS_PROFILE_V1.md),
[product](VNEXT_PRODUCT_INTEGRATION_PROFILE_V1.md), [REST](VNEXT_REST_API_PROFILE_V1.md),
[WS](VNEXT_PRIVATE_WEBSOCKET_PROFILE_V1.md), [CLI](VNEXT_CLI_PROFILE_V1.md),
[Desktop/Web](VNEXT_DESKTOP_WEB_UX_PROFILE_V1.md),
[workflow](ADDITIVE_KU_WORKFLOW_SURFACE_V1.md).
