# OneBrain Base v1 migration guide

## Supported source and authority boundary

Base v1 accepts legacy KU stores only as read-only migration input. Before the
migration begins, stop every legacy writer and retain an immutable source
snapshot. The vNext object/event/feed stores are the only destination write
authority; graph, retriever, KQL, and search state are derived projections and
are never copied as authority.

## Procedure

1. Verify the signed Base release request and exact candidate compatibility
   tuple. Refuse an unknown commit/toolchain or an incompatible schema,
   storage, archive, wire, API, or ABI identity.
2. Inventory the complete legacy source and record its raw BLAKE3, record
   counts, high-water marks, and limitations. Do not mutate the source.
3. Materialize canonical vNext objects, authority events, feeds, owned-blob
   references, pending intents, and operation-control state into a new dataset
   generation. A record without an exact deterministic mapping is a typed
   migration failure, never a silent omission.
4. Verify content IDs, chunk digests, authority branches, feed continuity,
   object/reference parity, and canonical roots. Rebuild every derived
   projection from canonical bytes and compare its source-root binding.
5. Run reopen, crash-window, archive/restore, signer-possession, network-off,
   and resource-bound checks against the staged generation.
6. Acquire the exclusive dataset/control-plane lease, write the activation
   journal, and atomically switch the generation pointer. Reconcile the
   operation receipt from a newly opened Base service handle.
7. Keep the legacy snapshot read-only until the rollback window closes. Never
   route a Base write back into `KuStorage`.

Migration succeeds only when the generated receipt binds the same release
request, qualification session, candidate commit/tree, candidate semantic
digest, portable-data tuple, source root, destination root, and signed
migration-vector ID/digest/trust policy. A restart resumes or reconciles the
same operation ID; it does not begin an untracked second migration.

An additive Base `1.x` migration may preserve the old client behavior. Any
canonical, authority, storage/archive, wire/API/ABI-major, or ownership break
requires a Base v2 design and new migration vectors before implementation.
