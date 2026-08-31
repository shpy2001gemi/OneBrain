# OneBrain Base v1 changelog

## 1.0.0 qualification candidate — 2026-08-11

This source release freezes the product-neutral Base v1 contract and remains
`Unqualified` until Task 28 produces a valid external evidence manifest and
atomically publishes the signed `base-v1.0.0` tag.

- vNext object/event/feed data is the sole Base write authority; legacy data is
  read-only migration evidence.
- Encrypted OBARV002 archive/restore, staged dataset generations, portable-data
  compatibility, signer possession/reprovision, blob integrity and derived
  projection rebuilds are part of the frozen recovery boundary.
- The durable reserve/prepare/confirm/reconcile facade, bounded subscriptions,
  management authority, archive capability handles, stable C ABI, TypeScript
  and Dart projections, and independent version negotiation are frozen.
- Network transport is authenticated, optional, default-off, and governed by
  an explicit kill switch. Local/network-off operation remains available when
  network or non-exportable signers are unavailable.
- Production qualification requires exact Linux, Windows, and macOS artifacts;
  fresh full-size Registry, three-host P5, and uninterrupted 72-hour soak
  evidence; security audits/triage; SBOM/provenance; migration/rollback; and
  signed child evidence under owner-approved policies.
- Release requests and manifests are content-addressed and create-new. The
  annotated tag is written unreferenced, verified, then published by one
  compare-and-swap from an absent ref.

### Compatibility promise

`1.0.x` is limited to correctness or security fixes without a semantic break.
`1.x.0` may add optional capabilities only when old-client behavior remains
valid. Incompatible canonical, authority, storage/archive, wire/API/ABI, or
ownership changes reopen the program as Base v2 with new design, migration,
rollback, vectors, and product requalification.

### Known boundary

The Task 27 commit is a qualification candidate, not a production-qualified
release. No source boolean, workflow result, version string, or tag name can
substitute for the external Task 28 evidence manifest and its verified outer
signature.
