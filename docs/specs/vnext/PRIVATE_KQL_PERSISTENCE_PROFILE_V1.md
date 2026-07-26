# OneBrain vNext — Private KQL Persistence Profile v1

> **Work package:** DR-P1.4
> **Status:** Frozen and implemented — 2026-07-26
> **Code:** [`ku-kql::vnext_private_need`](../../../src/ku-kql/src/vnext_private_need.rs) and [`onebrain-node::vnext_distributed_kql`](../../../src/onebrain-node/src/vnext_distributed_kql.rs)

## 1. Boundary

A durable local KQL need is one `PrivateNeedBundle` containing the canonical
private `QueryDefinition` and the exact typed `LocalNeedTarget`. The encrypted
bundle is the source of truth for runtime rehydration. A separate plaintext
`StandingNeed` database MUST NOT be used by the distributed KQL runtime.

The vault key is caller-owned. `DistributedKqlRuntime::open` MUST require a
`LocalNeedVaultKey`; it MUST NOT create, persist, export, log, derive from the
data path, or silently replace that key. A wrong key or authenticated-record
tamper MUST fail closed before any target becomes active.

The pre-P1.4 `vnext_standing_needs.redb` layout cannot reconstruct the exact
typed target. Its presence therefore MUST fail with an explicit migration
error; it MUST NOT be treated as an empty encrypted vault.

## 2. Deterministic local-intent adapter

`adapt_local_intent` accepts either parsed local KQL text or non-empty private
user-intent bytes plus a typed template. It MUST:

1. reject invalid raw KQL;
2. alpha-normalize the semantic goal and local context;
3. reduce the source bytes to a domain-separated BLAKE3 commitment;
4. include only that commitment in the `LOCAL_ONLY` semantic context;
5. derive the private `QueryDefinitionCID`, `StandingNeed`, and exact
   `LocalNeedTarget` deterministically; and
6. never include the raw source bytes in the returned bundle.

The template binds the receptor definition, selector, policies, frontier,
provenance, evidence, matcher metrics, expiry, and semantic fields. The
adapter MUST reject a receptor CID mismatch, a query-definition mismatch,
duplicate evidence references, a non-local definition, or a zero expiry.

## 3. Encrypted record and index

The Redb table `vnext_private_need_vault_v1` stores:

- a keyed BLAKE3 commitment of `StandingNeedID` as the table key;
- an XChaCha20-Poly1305 authenticated ciphertext as the value;
- a versioned, canonical local record inside the ciphertext.

The record binds its identifier, generation, lifecycle, disclosure class and,
for active or paused records, the exact canonical bundle. Nonces are
deterministically derived with a separate subkey over AAD and the plaintext
hash. The AAD binds the opaque table key and vault profile version.

Raw KQL, user-intent bytes, canonical bundle bytes, `StandingNeedID`,
`QueryDefinitionCID`, receptor identity and private semantic context MUST NOT
appear as plaintext table keys or values. The vault has a one-million-record
ceiling and an 8 MiB canonical plaintext ceiling per record.

## 4. Lifecycle

| Operation | Prior state | Durable result | Startup behavior |
|---|---|---|---|
| register | absent | `ACTIVE`, generation 0 or supplied generation | rehydrate exact target |
| pause | `ACTIVE` | `PAUSED`, generation + 1, encrypted bundle retained | do not schedule |
| resume | `PAUSED` | `ACTIVE`, generation + 1 | rehydrate/schedule |
| cancel | `ACTIVE` or `PAUSED` | `CANCELED` tombstone, generation + 1 | never rehydrate |
| retire | `ACTIVE` or `PAUSED` | `RETIRED` tombstone, generation + 1 | never rehydrate |

Pause is reversible. Cancel and retire are terminal. A terminal transition
MUST replace the current table value with a tombstone that contains no bundle.
An exact terminal retry is idempotent; a stale bundle MUST NOT resurrect a
terminal identifier. Generation mismatch, invalid transition, integer
overflow, missing record, or identity mismatch MUST fail closed.

At startup, the runtime MUST authenticate and validate every bounded record
before activating any target. Only `ACTIVE` bundles enter the matcher.
`PAUSED` bundles remain retrievable but inactive. Tombstones never expose a
`StandingNeed` or target.

## 5. Transaction and public boundaries

`TX-KQL-000` is the atomic Redb replacement of one encrypted private-need
record. The in-memory active-target map changes only after this commit.
Restart is the recovery mechanism if the process stops between durable commit
and in-memory projection.

No private-need record is a Public Store record, reconciliation inventory
member, route payload, WebSocket event, metric label, log field, or telemetry
attribute. The M3 network path continues to carry only validated Public
`KnowledgeAffordance` bytes. Wire evidence MUST show the absence of raw KQL,
private `QueryDefinitionCID`, `StandingNeedID`, and private semantic context.

Cancel/retire do not delete already accepted Public objects or historical
local match evidence. They only prevent future scheduling and target
rehydration. Proposal materialization/adoption remains a separate explicit
authority boundary.

## 6. Executable evidence

The focused tests prove:

- deterministic adapter output and strict private codec round trip;
- different local intent changes the private definition identity;
- no raw source, canonical bundle, or plaintext `StandingNeedID` on disk;
- exact target recovery after close/reopen without caller re-registration;
- wrong-key and ciphertext-tamper rejection;
- durable pause/resume and terminal cancel/retire tombstones;
- stale target replay cannot resurrect a tombstone;
- M3 real-QUIC replay keeps one durable match after automatic rehydration; and
- exact outbound application bytes omit raw KQL, private definition identity,
  standing-need identity, and private context.
