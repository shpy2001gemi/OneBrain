# OneBrain vNext — Additive Migration Storage Profile v1

> **Task:** `MIG-001`  
> **Status:** Complete  
> **Code:** [`ku-core::foundation::migration`](../../../src/ku-core/src/foundation/migration.rs)

## 1. Boundary

Migration is local storage work, not a network consensus event. A node may run,
pause, retry or roll back a migration independently. No seed, relay, activation
epoch, OBT balance or globally complete inventory is required.

The implementation uses parallel namespaces for:

- exact read-only v1 source rows;
- derived vNext migration rows;
- non-executable migration quarantine;
- immutable per-row journal entries; and
- immutable batch manifests with a separate completion bit.

New vNext writes never mutate the v1 namespace. Removal of v1 tables is outside
this profile and requires backup, root/count reconciliation and a separate
operator action after the supported rollback window.

## 2. Atomic row rule

For every row, one backend transaction MUST preserve the exact source bytes and
commit exactly one of:

1. a derived vNext row plus `VNextDerived` journal outcome; or
2. a quarantine record plus `Quarantined` journal outcome.

The journal binds batch ID, typed legacy row key, source digest, disposition and
output digest. Replaying the exact entry is idempotent. Reusing a row key with
different raw bytes, normalized bytes or journal result is a conflict and MUST
NOT overwrite either copy.

Quarantine carries original bytes and a deterministic ID, but exposes
`is_executable() == false`. It has no projection or authority path.

## 3. Batch kill/restart

A batch manifest is the sorted set of `(typed row key, source digest)` values.
The batch ID cannot be reused for a different manifest or expected row count.
Completion is set only after the backend observes one journal entry for every
manifest row.

A process may stop after any row transaction. Restarting with the same batch
and input exact-replays completed rows and continues the remainder. There is no
partially accepted row.

## 4. Dual read and copy-on-read

Dual read prefers a vNext row only when the caller's current verifier accepts
it. Otherwise it returns the exact read-only v1 row. A quarantine result is
never returned as executable vNext data.

Copy-on-read runs the same one-row batch path when neither vNext nor quarantine
exists. It is therefore journaled and idempotent, not an untracked cache write.
Rollback code can continue reading raw v1 bytes independently of normalized
data or adapter enablement.

## 5. Identity firewall

`LegacyIdentityPrefix` stores the original `u64` and source-row digest. It has
no conversion into `NodeId`, `ActorId`, `DeviceId` or `FeedId`, and explicitly
reports that it is not a full-width identity. Hashing or padding a legacy
counter does not reconstruct the missing 192 bits and MUST NOT be presented as
the original principal.

## 6. Persistence and evidence

The in-memory and redb backends implement the same atomic contract. Tests cover:

- kill after one row followed by idempotent restart;
- exact replay and manifest conflict protection;
- verified-vNext preference and v1 fallback;
- idempotent copy-on-read;
- corrupt-row quarantine with raw rollback; and
- close/reopen persistence of batch journal, vNext bytes and raw v1 bytes.

