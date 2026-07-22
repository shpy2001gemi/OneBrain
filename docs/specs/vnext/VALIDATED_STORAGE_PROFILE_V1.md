# OneBrain vNext — Validated Storage Profile v1

> **Tasks:** `OBS-001`, `OBS-002`  
> **Status:** Normative — frozen 2026-07-20  
> **Code:** [`foundation::storage`](../../../src/ku-core/src/foundation/storage.rs), [`foundation::vault`](../../../src/ku-core/src/foundation/vault.rs)

## 1. Single acceptance boundary

Immutable vNext object/event bytes enter the accepted namespace only through
`ValidatedStore::put_verified_object` or `put_verified_event`. The boundary
performs, in order:

1. bounded canonical decode and root schema/resource validation;
2. object-kind disposition or event-type disposition;
3. feed/author/signature verification for events;
4. recomputation and constant typed comparison of `ObjectCid`/`EventCid`;
5. one atomic backend operation that either stores exact accepted bytes,
   reports the exact idempotent duplicate, or preserves a collision attempt in
   Quarantine without replacing the accepted record.

No graph, KQL index, profile, tool or semantic reducer is called from this
transaction. Those are rebuildable downstream projections over the accepted
namespace.

## 2. Accepted namespace

The accepted key is `(record_kind, full_256_bit_cid)`. The value is exactly the
validated original canonical bytes. Re-encoding is never the source of record.

| Existing key | Incoming bytes | Outcome |
|---|---|---|
| absent | fully validated | `Stored` |
| present | byte-identical | `AlreadyPresent` |
| present | different | accepted value unchanged; incoming attempt quarantined |
| any | validation/CID/signature failure | no accepted write; incoming attempt quarantined |

Although a same-CID/different-bytes cryptographic collision is expected to be
infeasible, the storage invariant is enforced independently of that assumption.

## 3. Quarantine namespace

A `QuarantineRecord` contains a local deterministic quarantine ID, record kind,
claimed CID, stable reason code and original received bytes. It always reports
`is_executable() == false`. Quarantine has no API that returns a semantic object
and must never be scanned by OBKG/KQL/tool/profile projectors.

Quarantine records are evidence for inspection, repair, protocol diagnostics or
later revalidation. They are not a claim that a knowledge proposition is false.

## 4. Storage-class firewall and Private Vault

The Public Store accepts only `PUBLIC` and `ROUTE_MINIMAL` records. A fully
decoded `LOCAL_ONLY` or `NEGOTIATED_ENCRYPTED` record returns
`STORE_CLASS_MISMATCH` before any public accepted write. Raw input provenance
must select Public Store or Private Vault before decoding; private/local intake
must never be offered to the public-invalid-input path.

`PrivateVault` applies the same canonical/schema/CID/signature validation and
the same atomic backend abstraction, but accepts only `LOCAL_ONLY` and
`NEGOTIATED_ENCRYPTED`. It encrypts both accepted values and private quarantine
payloads before invoking the backend.

Vault encryption is XChaCha20-Poly1305 AEAD with a 256-bit caller-supplied key.
Associated data binds profile version, accepted/quarantine purpose, record kind
and full CID. The 192-bit nonce is a keyed derivation over that associated data
and the plaintext digest: an exact idempotent retry repeats the same sealed
value, while different plaintext under the same claimed CID receives a distinct
nonce. The key must originate from a CSPRNG/OS key store; the API does not invent
a password-derived default, clone it, print it or log it. Caller key bytes and
the internal nonce key are zeroized on drop.

Opening a vault value authenticates/decrypts it and recomputes its typed
domain-separated CID before returning plaintext. Wrong key, modified nonce,
modified associated metadata or changed ciphertext returns `VAULT_CRYPTO`;
post-open CID drift returns `VAULT_CID_MISMATCH`.

## 5. Atomic backend contract

`InMemoryVerifiedBackend` supplies deterministic conformance behavior.
`RedbVerifiedBackend` (feature `persist`) maps accepted and quarantine records to
separate tables and commits each outcome in one ACID write transaction.

If a process stops before `commit`, redb rolls back the transaction. Tests insert
an accepted value into an uncommitted transaction, drop it, reopen the database,
and verify that neither a partial record nor a false accepted state remains.

## 6. Acceptance evidence

- Valid object bytes round-trip exactly and duplicate insertion is idempotent.
- Malformed canonical bytes never enter the accepted namespace.
- A changed payload under the same claimed CID cannot replace accepted bytes and
  is available only as non-executable quarantine evidence.
- A tampered event signature cannot enter the accepted event namespace.
- Redb commits collision quarantine atomically while retaining the original.
- Dropping a write transaction before commit remains empty after database reopen.
- Private disclosure cannot enter Public Store.
- Private accepted/quarantine plaintext is absent from raw backend values.
- Exact private retry is idempotent; different plaintext cannot reuse the same
  nonce under one claimed CID.
- Wrong vault key and changed ciphertext are rejected before plaintext return.
