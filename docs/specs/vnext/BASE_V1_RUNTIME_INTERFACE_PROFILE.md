# OneBrain Base v1 Runtime Interface Profile

> **Status:** Frozen
> **Profile ID:** `BASE_V1_RUNTIME_INTERFACE_V1`
> **Profile version:** `1.0`
> **Product API projection:** `VNEXT_PRODUCT_INTEGRATION_PROFILE_V1/1.1`
> **Machine IDL:** [`base-v1-runtime-interface-v1.json`](../../../src/test-vectors/vnext/base-v1-runtime-interface-v1.json)
> **Discriminator history:** [`base-v1-runtime-interface-history-v1.json`](../../../src/test-vectors/vnext/base-v1-runtime-interface-history-v1.json)

## 1. Scope and authority

This profile freezes the product-neutral semantic boundary used by Desktop,
Web, mobile, CLI, REST, Rust, TypeScript, Dart, and the later C ABI. The machine
IDL is the only declaration source; this document explains its security and
lifecycle meaning.

- A projection MUST NOT become an independent source of operation, type,
  ownership, error, or lifecycle semantics.
- A product adapter MUST NOT receive a raw path, runtime/store implementation,
  authority implementation, private key, borrowed reader, or borrowed writer.
- Every variable payload, continuation, string, collection, event batch, and
  archive chunk MUST retain the finite bound in the machine IDL.
- The management namespace MUST remain product-neutral and privileged within
  the same runtime lifecycle.
- An ordinary service handle MUST NOT mint, infer, or upgrade management
  authority.

The archive command semantics are those frozen by the
[Base v1 Storage Integrity Profile](BASE_V1_STORAGE_INTEGRITY_PROFILE.md) and
its machine archive vector. This interface transports bounded opaque capabilities; it does
not make filesystems or Rust readers/writers part of the public contract.

## 2. Closed operation surface

The operation IDs, request IDs, response IDs, names, and projection names are
closed discriminators. The live semantic surface is:

| Namespace | Operations |
|---|---|
| Session | `open`, `negotiate`, `status`, `snapshot`, `query` |
| Durable operation | `reserve_operation`, `prepare`, `confirm`, `cancel`, `reconcile` |
| Subscription | `subscribe`, `poll_events`, `close_subscription` |
| Runtime lifecycle | `drain`, `close` |
| Management lifecycle | `management.open`, `management.close` |
| Archive source | `management.archive_source_begin`, `management.archive_source_push_chunk`, `management.archive_source_seal` |
| Archive sink | `management.archive_sink_begin`, `management.archive_sink_read_chunk`, `management.archive_sink_commit` |
| Archive custody | `management.archive_secret_register`, `management.archive_capability_abort`, `management.archive_capability_destroy` |
| Recovery | `management.complete_signer_reprovision` |

- An implementation MUST reject an unknown operation, request, response,
  command, topic, or error discriminator.
- `CreateArchive` and `RestoreArchive` MUST remain explicit prepared-command
  discriminators; registration of a capability alone MUST NOT execute either.
- Request and response discriminator IDs MUST remain bound to the matching
  operation ID.
- Every public error MUST expose its closed discriminator, retryability, and
  reconciliation requirement without leaking internal error text.

## 3. Common envelope and finite resources

Every projection carries the same semantic fields: profile major/minor,
process and dataset generations, request/operation/idempotency IDs, lifecycle,
coverage, limitations, retryability, resource budget, typed payload
discriminator, and compatibility digest.

- A request MUST be rejected before dispatch when either generation fence is
  absent, stale, or belongs to another runtime instance.
- IDs and opaque handles MUST retain their typed role even when they have the
  same byte width.
- A Base continuation MUST remain opaque and context-bound and MUST NOT exceed
  4,096 bytes. The REST projection retains its narrower 2,048-character bound.
- A general payload MUST NOT exceed 1 MiB; an event payload MUST NOT exceed
  64 KiB; an archive chunk MUST NOT exceed 1 MiB.
- An event poll MUST NOT return more than 256 items, and a query response MUST
  NOT return more than 256 items.
- Resource admission MUST fail with a typed bounded error before allocating an
  unbounded buffer or accepting an unbounded stream.

## 4. Durable operation protocol

The required order is `reserve_operation` → management capability registration
when needed → `prepare` → `confirm` or `cancel` → `reconcile`.

- `reserve_operation` MUST durably allocate the operation ID and generation,
  kind, and principal fences before any archive capability is registered.
- `prepare` MUST consume a valid reservation and MUST durably record the exact
  typed command before returning `PreparedIntentV1`.
- `confirm` MUST require the exact prepared intent and idempotency key and MUST
  record intent before the next side effect.
- A caller that loses a response or receives `unknown_outcome` MUST call
  `reconcile` before retrying.
- A retryable error MUST NOT authorize blind replay; reconciliation remains
  mandatory when the operation may have crossed a durable boundary.
- `cancel` MUST be idempotent for an already canceled operation and MUST NOT
  roll back a committed authoritative effect.
- Terminal receipts MUST remain durable across service-handle reacquisition
  and dataset activation.

The closed states are `reserved`, `prepared`, `confirming`, `committed`,
`canceled`, `failed`, and `unknown_outcome`. Only the transitions listed in the
machine IDL are legal.

## 5. Management grants and archive capabilities

`management.open` consumes one host-authenticated, unforgeable
`BaseManagementGrantV1`. The grant binds principal, exact scopes, process and
dataset generations, expiry, and revocation epoch.

- A management grant MUST be single-use and MUST fail closed after expiry,
  revocation, generation change, or scope mismatch.
- Each source, sink, or secret capability MUST bind its management handle,
  principal, reserved operation ID, process generation, dataset generation,
  and capability kind.
- A capability MUST NOT be inferred from caller-provided bytes or another
  operation's opaque handle.
- Source streaming MUST follow begin → bounded push → seal → consume or
  abort/destroy.
- Sink streaming MUST follow begin → bounded read → commit or abort/destroy.
- Secret registration MUST use zeroizing, non-exportable custody and MUST NOT
  return secret bytes through any projection.
- Commit, abort, destroy, terminal reuse, cross-operation reuse, and stale
  generation behavior MUST be explicit and typed.
- `management.close` MUST revoke every remaining capability owned by that
  handle.
- Signer recovery MUST complete only through
  `management.complete_signer_reprovision`; archive restore alone MUST NOT
  synthesize a non-exportable signer.

## 6. Subscriptions, cursors, and backpressure

The topic vocabulary is exactly `RuntimeStatus`, `OperationReceipts`,
`QueryResults`, `ArchiveProgress`, and `Compatibility` for profile v1.

- A subscription handle MUST be owned by the service session and bind the
  principal, process generation, dataset generation, and exact topic.
- `poll_events` MUST accept an after-cursor and finite item bound and MUST
  return a strictly non-regressing next cursor.
- A retention gap MUST return a typed resync-required result with the earliest
  available cursor; it MUST NOT silently skip events.
- A slow consumer MUST receive bounded buffering followed by a typed disconnect
  and resync requirement; the runtime MUST NOT grow an unbounded queue.
- `close_subscription` MUST be explicit and idempotent, and polling a closed
  handle MUST return the frozen typed error.

## 7. Drain and close

- `drain` MUST reject new reservations and preparations while preserving
  status, polling, cancel, and reconcile paths required to reach a known state.
- `close` MUST require the runtime to enter drain, close subscriptions, abort
  open capabilities, and preserve durable receipts.
- Runtime close MUST be explicit and idempotent; dropping a language wrapper
  MAY invoke it but MUST NOT change its receipt or revocation semantics.
- Management close and service close MUST remain separate authority checks.
- A closed service or management handle MUST reject every later operation.

## 8. Generated projections

Rust, TypeScript, Dart, and C ABI declarations are projections of the exact
machine IDL. Task 15 owns the first three generators; Task 18 owns the C ABI.

- Generated projections MUST preserve every numeric discriminator, field name,
  finite bound, optionality, ownership rule, lifecycle transition, and error
  property.
- Handwritten public declarations MUST NOT replace or shadow a generated
  discriminator.
- Every C input, output, and error struct MUST begin with `u32 struct_size` so
  callers and implementations can reject incompatible layouts.
- Projection-specific convenience behavior MUST remain outside generated data
  declarations and MUST NOT create new authority.
- Generator check mode MUST compare generated bytes without modifying checked-in
  files and MUST fail on drift.

The exact method-name mapping for all four targets is part of the machine IDL;
language conventions do not authorize an adapter to invent aliases.

## 9. Append-only discriminator history

The history vector records every numeric/name pair as an active entry or a
later tombstone. Its root is a SHA-256 domain-separated chain over canonical
JSON entries in sequence order.

- Existing history entries MUST never be reordered, rewritten, or deleted.
- A numeric ID or name MUST never be reused for another meaning, including
  after tombstoning.
- Removing or retyping a discriminator, changing optional to required,
  widening a bound, or changing ownership MUST require a new profile major.
- Additive same-major changes MUST append history records and increment the
  profile minor.
- CI MUST compare the live history with the immutable Task 14 baseline loaded
  using `git show`; comparing only two mutable working-tree files is invalid.

The protected baseline ref is `refs/heads/base-v1-idl-baseline`. The immutable
CI receipt format is `onebrain/base-v1-idl-baseline-receipt/1` and contains the
exact ref, commit SHA-1, tree SHA-1, machine-IDL SHA-256, and history-chain
root. Before generation, CI MUST verify that the ref equals the receipt commit,
the commit is an ancestor of the candidate, both `git show` payloads match the
receipt digests, and the baseline history is an exact prefix. A missing, moved,
non-ancestor, or digest-mismatched baseline MUST stop generation.

The baseline ref is not a qualification or release tag. Task 27 separately
binds the history-chain root into the signed release request.

## 10. Product projection and acceptance evidence

Product API profile minor `1.1` adds only
`POST /api/vnext/base/negotiate`. It carries bounded capabilities and a
compatibility tuple/digest and MUST NOT expose management grants or handles.
Every pre-existing vNext endpoint retains its method, path, visibility, DTO
meaning, and semantic firewall.

Acceptance evidence is:

- [`test_validate_base_v1_runtime_interface.py`](../../../scripts/ci/test_validate_base_v1_runtime_interface.py), which mutates each authority, bound, operation, history, subscription, archive, lifecycle, and projection rule;
- [`validate_vnext_contracts.py`](../../../scripts/ci/validate_vnext_contracts.py), which validates the focused machine profile as part of the global vNext gate;
- the machine IDL and append-only history linked at the top of this profile.

Task 14 freezes declarations only. It does not claim that Task 15 projections,
Task 17 facade implementations, Task 18 ABI, or release qualification already
exist.
