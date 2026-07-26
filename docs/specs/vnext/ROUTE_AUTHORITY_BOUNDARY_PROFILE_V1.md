# OneBrain vNext — Route and Authority Boundary Profile v1

> **Work package:** DR-P1.5
>
> **Status:** Frozen and implemented — 2026-07-26
> **Code:** [`onebrain-node::vnext_route_authority`](../../../src/onebrain-node/src/vnext_route_authority.rs), [`onebrain-node::vnext_network_runtime`](../../../src/onebrain-node/src/vnext_network_runtime.rs), and [`onebrain-node::vnext_distributed_pomv`](../../../src/onebrain-node/src/vnext_distributed_pomv.rs)

## 1. Boundary

This profile closes three authority-confusion paths in the first distributed
runtime slice: unauthenticated address injection, caller-selected policy
implementation, and caller-selected historical authority frontier. It does
not create a global route oracle, global authority oracle, or global latest
state.

The route directory MUST mutate only after the signed session handshake and
the local replay guard both accept the exact transcript.

The stored peer identity MUST be derived from the authenticated session role;
an address or request parameter cannot assert a `NodeID`.

The directory MUST preserve a bounded `NodeID ↔ SocketAddr` bijection and
replace both sides atomically under its local write lock.

An inbound source address MUST NOT downgrade a previously authenticated
outbound responder route because the inbound source port may not be dialable.

## 2. Authenticated route use

An outbound Public Use publication MUST resolve its exact prepared recipient
through the authenticated route directory when creating network intents.

If no authenticated outbound responder route exists, export MUST fail closed
and leave the durable publication pending.

`last_known_addr` is removed from the confirmation capability and new
publication records. Schema-v2 records are read with their caller-supplied
address discarded and are requeued for resolution through a fresh
authenticated route.

## 3. Local policy registry

The local policy registry MUST be immutable after runtime construction and
bounded to 64 non-zero typed versions.

Every registered policy MUST pass canonical `MetabolicViewPolicy` validation
before the registry becomes available.

A materialization request MUST name only a `LocalPolicyVersion`; it cannot
supply policy fields, code, callbacks, or trait implementations.

An unregistered policy version MUST fail closed without materializing or
advancing a view lineage.

## 4. Authority-frontier resolver

The resolver MUST inspect only authority records already accepted by the
validated local store.

It MUST derive terminal authority tips from canonical delegation and
revocation dependencies rather than accepting a frontier from the caller.

A single terminal tip that yields an authorized or revoked decision MUST be
used as that feed's local frontier.

Missing authority state MUST remain unresolved and MUST NOT be promoted to
`Authorized`.

Multiple relevant incomparable tips MUST fail closed as ambiguous; the runtime
must not choose the most favorable branch.

The metabolic view frontier is a domain-separated digest of the sorted
per-feed local resolutions. It is a reproducible local-state commitment, not a
claim of global freshness or completion.

## 5. Product/API surface

Product and API callers MUST NOT provide `ExerciseAuthority::Authorized`, an
authority frontier, a policy implementation, or an outbound socket address in
the Public Use confirmation request.

Callers may select an allow-listed policy version and an exact recipient
`NodeID`. Those identifiers select locally controlled state; they do not
create authority.

The route writer methods remain crate-private and require an
`AuthenticatedSession` with the exact local initiator or responder role.
Public route access is read-only.

## 6. Executable evidence

Focused tests prove:

- role-mismatched observations cannot mutate the empty route directory;
- authenticated route replacement preserves the two-way index;
- real QUIC handshakes populate both peers only after authentication;
- invalid, duplicate and unknown policy versions fail closed;
- only one relevant local terminal authority frontier resolves;
- missing and ambiguous local authority state cannot become authorized;
- publication export without an authenticated outbound route remains pending;
- the PoMV caller cannot inject a policy object or authority frontier; and
- restart and revocation continue to reproduce the correct local view.
