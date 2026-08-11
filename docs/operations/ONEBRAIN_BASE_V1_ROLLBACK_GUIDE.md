# OneBrain Base v1 rollback guide

## When rollback is allowed

Rollback is an explicit recovery operation after a failed activation,
post-activation health failure, signer reprovision failure, or incompatible
product finding. It may select only a previously verified dataset generation
whose portable compatibility tuple is accepted by the running Base. It never
reinterprets canonical bytes and never enables legacy writes.

## Procedure

1. Disable network admission with the Base kill switch and drain new product
   operations. Preserve the non-switched activation and idempotency journal.
2. Acquire the lifetime-held exclusive dataset/control-plane lease. Inspect
   the current and previous generation manifests, archive identities, roots,
   operation bindings, and release/Registry generation IDs.
3. Verify the previous generation in full: canonical/object/blob/feed/
   authority roots, pending-intent reconciliation, archive authentication,
   signer possession or typed reprovision status, and resource bounds.
4. Rebuild derived index and retriever projections against the previous
   canonical source root. Do not reuse projections bound to the failed
   generation.
5. Write a rollback activation intent, atomically compare-and-swap the current
   generation pointer, reopen all Base services, and bind them to the restored
   generation and rebuilt projections.
6. Reconcile interrupted operations from a newly acquired service handle.
   Record the old/current generation IDs, before/after roots, Registry release
   root, request/session, exact Base tuple, reason, and operator identity.
7. Re-enable network admission only after local health, archive round-trip,
   projection parity, and product capability checks pass.

If pointer publication is uncertain, report `UnknownOutcome` and reconcile;
do not retry with a new operation ID. If the prior generation cannot satisfy
the portable compatibility gate, stop and restore from an authenticated
OBARV002 archive into another staged generation. Silent fallback to legacy or
to a different Registry release is forbidden.
