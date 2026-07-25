# OneBrain vNext — Distributed Public PoMV Runtime Profile v1

> **Milestone:** `M4`  
> **Status:** Implemented behind `vnext-network-runtime` — 2026-07-25  
> **Code:** [`onebrain-node::vnext_distributed_pomv`](../../../src/onebrain-node/src/vnext_distributed_pomv.rs)  
> **Depends on:** `POMV-001`, `POMV-002`, `FEED-001`, `AUTH-001`, `OBP-001`

## 1. Demonstrated scope

This profile freezes the first distributed PoMV runtime slice: an explicitly
confirmed, Public `UseEvidence` record can be created by one node, reconciled
over authenticated QUIC/OBP-RP, validated by another node and projected into a
frontier- and policy-relative `MetabolicEvidenceView`.

The implementation is intentionally narrower than the complete PoMV vision:

- it accepts Public `UseEvidence` only;
- it does not create or infer Use from query hits, retrieval, presentation or
  exposure;
- it does not yet distribute `DerivationEvidence`, `OutcomeObservation` or
  `BenefitEvidence`;
- it does not claim truth, benefit, rank, global completeness or consensus;
- it cannot mint, transfer, price or authorize OBT; and
- it does not mutate wallet state.

## 2. Sender transaction

`PublicUseEvidencePublisher` requires a non-zero
`ExplicitUseConfirmation`. There is no implicit creation path.

One redb transaction commits all of the logical publication state:

1. the next per-Feed sequence and previous EventCID;
2. the canonical Public `UseEvidencePayload` and ObjectCID;
3. the signed `KnowledgeEvent`, EventCID and causal parent;
4. the non-zero idempotency key and confirmation commitment; and
5. a durable logical outbound publication record.

An exact retry returns the existing publication. Reusing the same
`(FeedId, idempotency_key)` for different content fails closed. A later event
on the same Feed receives the next sequence and exact causal parent.

`flush_pending` idempotently copies the FeedInception, payload object and event
into the existing durable network outbox. The logical publication is marked
exported only after those transfer intents have been stored. Delivery and
authenticated receipts remain the responsibility of the restart-safe OBP-RP
outbox.

The caller supplies the Feed signing key for the bounded publish operation.
The runtime does not retain that private key. A production OS/HSM/remote
Feed-signer adapter is not shipped by this milestone and is separate from the
existing injectable NodeID session signer.

## 3. Receiver admission and binding

The receiver applies the same validate-then-accept boundary as other vNext
records:

1. canonical envelope, CID, kind, schema and Public disclosure;
2. complete typed `UseEvidencePayload` decoding and canonical round trip;
3. exact FeedInception branch and Ed25519 event-signature verification;
4. durable payload and causal-dependency availability;
5. exact event-type, payload-reference and disclosure binding; and
6. authenticated source provenance under the requested SelectorCID.

A structurally valid generic object whose declared `UseEvidence` payload is
typed-invalid is quarantined with
`USE_EVIDENCE_TYPED_PAYLOAD_INVALID`. A locally created record with no
authenticated source observation does not become distributed evidence.
Source peer provenance describes transport paths only; it grants no
authorship, authority, truth, benefit or completion.

## 4. Authority, replay and conflict rules

Callers cannot assert `ExerciseAuthority::Authorized`. For every verifying
FeedInception branch, the runtime derives its decision against the exact
caller-supplied authority frontier:

| Feed authority decision | PoMV assessment |
| --- | --- |
| `AuthorizedRelative` | `Authorized` |
| `QuarantinedRevokedRelative` | `Unauthorized` |
| missing, stale or unresolved | `Unresolved` |

The immutable accepted-event store deduplicates the same EventCID regardless
of delivery path. A separate durable identity index binds
`(FeedId, UseEvidence event type, idempotency key)` to its complete sorted set
of EventCIDs.

- An exact replay remains one event.
- One EventCID observed from one, two or five authenticated NodeIDs remains one
  cumulative evidence item.
- Two different EventCIDs under the same identity are an explicit conflict.
- Every conflicting variant is excluded from the metabolic reducer; arrival
  order cannot choose a winner.
- Overflow of the bounded variant set also fails closed as a conflict.

## 5. Durable metabolic view

`DistributedPomvRuntime` rebuilds accepted observations from durable objects,
events, source provenance, Feed state and authority state. It then applies the
frozen `MetabolicEvidenceReducer` using the exact target, policy reference,
accepted evidence policies and authority frontier supplied by the caller.

The runtime persists a view head per `(target, policy)`:

- unchanged view roots retain the same revision;
- a changed evidence, policy/frontier projection or limitation creates the
  next linked revision;
- `previous_view_root` preserves lineage; and
- reopening the receiver and PoMV database reproduces the same view root and
  revision for the same durable state.

Every report sets these machine-checkable boundaries to `false`:

- `claims_truth`;
- `claims_benefit`;
- `changes_wallet_state`;
- `changes_obt_state`; and
- `claims_network_completion`.

## 6. Executable acceptance evidence

The feature-gated test suite proves:

- explicit confirmation and a non-zero idempotency key are mandatory;
- sender publication is atomic, idempotent, causal and restart-safe;
- typed-invalid UseEvidence is quarantined;
- one event delivered through one, two and five independently authenticated
  paths contributes exactly one cumulative EventCID;
- the same view root and revision survive sender and receiver restart;
- an un-delegated Feed remains unresolved;
- a self-revocation changes the exact frontier-relative assessment to
  unauthorized;
- a second EventCID with the same Feed/type/idempotency identity excludes all
  conflicting variants without double counting; and
- wallet and OBT state remain unchanged.

The acceptance path uses real loopback QUIC/TLS 1.3 listeners and the durable
OBP-RP outbox, not an in-memory transport simulation.

## 7. Deferred expansion

This profile does not authorize active remote KQL, DHT/global discovery,
Outcome/Benefit attribution, reward export or OBT. Those remain later
milestones with independent threat models and kill switches.
