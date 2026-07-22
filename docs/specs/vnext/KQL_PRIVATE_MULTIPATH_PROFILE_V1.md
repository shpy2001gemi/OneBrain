# KQL Private Multipath Query Profile v1

> **Task:** `KQL-012`  
> **Status:** Complete  
> **Depends on:** `SEC-002`, `SEC-003`, `KQL-005`, `KQL-011`

## 1. Purpose

This profile lets one local query use up to three independently compiled route
packets, merge partial replies by canonical identity and notify an active local
StandingNeed without publishing the need or its notification state. It reduces
dependence on any one carrier/path while preserving partition-first operation.

Multipath is optional. A node with no network path continues to use local KQL
results and StandingNeeds normally.

## 2. Route packet contract

Every branch is an existing `RouteNeedSketchV1` produced under `SEC-002` and
must contain exactly one allowlisted coarse token. Across one local plan, every
branch has a distinct:

- local path commitment, which is never serialized;
- sketch ID;
- one-time reply capability;
- replay nonce; and
- salted disclosure commitment.

The plan accepts one to three branches. Full NeedIR, StandingNeedID, ReceptorID,
AssemblyID, user identity, NodeID and a plaintext cross-packet correlation key
are absent from the packet schema.

This is schema unlinkability, not a promise of transport unlinkability. Timing,
packet size, carrier metadata or an adversarial relay can still correlate
traffic. Padding, carrier selection and timing policy remain transport-layer
responsibilities.

## 3. Progressive reply boundary

A reply enters the coordinator only as an `OpenedMultipathReply`. Its capsule
receipt can be constructed by the public API only from an `OpenedDisclosure`
already authenticated, permit-checked, replay-checked and decrypted through
the `SEC-003` capsule inbox. The coordinator has no raw plaintext-reply bypass.

Each reply capability is one-time:

- exact replay of the same opened reply is idempotent;
- conflicting reuse of a consumed capability is rejected; and
- a capability not present in the local plan is rejected.

This boundary proves successful capsule opening, not that a remote result is
true, useful, adopted or authorized for execution.

## 4. Canonical local union

ObjectCID, MappingKernelCID and EventCID occupy separate typed identity domains.
The coordinator unions results by that identity, independent of reply order or
path. The same CID returned through several paths remains one result; source or
path count does not affect rank. Conflicting declared metadata for one ObjectCID
is rejected transactionally.

The union view retains only local path observations and scoped partial coverage.
A dropped, timed-out, unavailable or suspected-eclipsed path does not invalidate
results already held locally. A later valid reply may still extend the view.
The view never claims global completion, component membership or execution
authority.

## 5. Encrypted StandingNeed mailbox

Only an active, valid local StandingNeed may produce a notification. Notification
plaintext binds:

- StandingNeedID;
- the canonical typed match set;
- the local match-rule commitment; and
- the first QueryView revision that exposed it.

The exactly-once notification ID deliberately excludes QueryView revision. The
same StandingNeed/match/rule tuple observed again in a child revision therefore
does not notify twice.

Mailbox payloads are encrypted at rest with XChaCha20-Poly1305. Notification ID
is authenticated as associated data; nonce reuse is rejected and the key is
zeroized on drop. A failed decryption does not consume the delivery. Snapshot
state retains sealed entries, used nonces and delivered tombstones so restart
preserves pending and exactly-once behavior.

The mailbox and snapshots are `LOCAL_ONLY`. They expose no publication method.
Storage backends must persist the whole snapshot atomically and keep the key
outside that snapshot.

## 6. Partition, eclipse and reunion behavior

Each connected island may query its own reachable frontier and derive a
different partial union. Carrier loss or suspected eclipse is recorded as a
local observation, never as proof that no matching knowledge exists. Delayed
or reunion-carried replies extend the same local result set by canonical CID.

Several paths delivering the same object preserve availability but never create
semantic weight. Multipath is therefore a resilience mechanism, not consensus,
truth voting or provider-count reputation.

## 7. Boundaries

The profile does not:

- disclose full NeedIR or stable human/node identifiers;
- promise resistance to timing or carrier-level correlation;
- infer falsehood or irrelevance from an absent/dropped reply;
- convert retrieval or notification into Use, benefit, PoMV or OBT;
- materialize, adopt or execute a returned proposal;
- let source multiplicity affect relevance or eligibility;
- require a server, globally reachable path or global completion marker; or
- introduce a Core DNA Gene or execution opcode.

## 8. Executable evidence

Six tests prove:

- three route packets have distinct schema entropy and omit private IDs;
- reply reordering produces the same canonical union;
- drop/suspected eclipse keeps coverage partial while local results stay usable;
- exact replay and the same CID across paths do not boost the union;
- encrypted mailbox state survives restart, a wrong key does not consume a
  delivery and one notification is delivered exactly once; and
- the same match in a later QueryView revision does not notify again.
