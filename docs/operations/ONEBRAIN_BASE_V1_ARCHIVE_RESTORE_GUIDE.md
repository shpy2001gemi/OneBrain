# OneBrain Base v1 Archive and Restore Guide

This runbook covers the portable Base dataset contract introduced by the
`OBARV002` archive profile. It does not make a development build production
qualified, and it does not make rebuildable projections authoritative.

## What the archive contains

The encrypted stream contains a deterministic `DatasetManifestV1` followed by
logical payloads in sorted entry-ID order. Entries use the closed Task 2 owner
table and bounded logical keys; local filesystem paths and raw database paths
are forbidden.

Included data comprises canonical objects and events, feed inception and
authority branches, authority and Registry high-water metadata, Vault records
as canonical plaintext inside the encrypted stream, Quarantine evidence,
owned blobs, identity envelopes, reconciliation/inventory/outbox/provenance
records, private KQL/POMV records, operational and rollout state, Base operation
records, pending blob/source-capture intents, migration state, interpretation
configuration, and signer recovery policy.

Derived graph, search, index and retriever generations are excluded and rebuilt
after verified restore. Registry/model payload bytes are excluded; their signed
identity and high-water metadata remain in the manifest.

## Create an archive

1. Stop new Base mutations and wait for admitted operations to reach a durable
   terminal or reconcilable state.
2. Acquire the source-owned `SnapshotLease`. Record its dataset, blob and
   retention generations, canonical root, every high-water root, and opaque
   source binding.
3. Enumerate logical entries through the bounded snapshot ports. Vault rows are
   decrypted only within this capture boundary; raw Vault ciphertext is never
   exported as portable data.
4. Read every entry, checking exact length and BLAKE3. Revalidate the lease
   before each read and after the final read. Any same-length mutation, root
   drift, high-water drift, or generation drift aborts the attempt.
5. Build and validate the sorted manifest and aggregate root. Do not publish an
   archive if capture or lease validation failed.
6. Feed the deterministic dataset plaintext into `seal_archive` with a bounded
   password or recovery-key credential. Persist the completed `OBARV002` bytes
   atomically, then release snapshot and retention holds.
7. Store the archive and its independently recorded expected digest in separate
   failure domains.

## Restore an archive

1. Copy the archive into a staging area that is not the current dataset
   generation. Never restore in place.
2. Inspect only the bounded header/profile. Supply the recovery credential and
   call full `OBARV002` verification before any logical restore sink receives
   plaintext.
3. Compare the portable-data compatibility tuple: canonical schema digest,
   domain and resource Registry digests, storage schema version, archive
   profile, and migration profile. A target/toolchain or producer artifact
   difference is provenance and does not by itself block portable restore.
4. Materialize verified logical rows into a new dataset generation through the
   normal validation boundaries. Re-encrypt Vault records under the target
   Vault key. Reconcile pending upload/source intents and migration journals.
5. Recompute canonical, object, blob, feed and high-water roots. Rebuild all
   excluded projections from validated canonical rows and check parity/health.
6. Atomically switch the active generation only after all required rows, roots,
   reconciliation, projection rebuilds, and health checks pass. Keep the old
   generation available for bounded rollback until the retention policy allows
   cleanup.
7. Reopen the runtime and verify the active generation, compatibility tuple,
   roots, projection bindings, and degraded-capability report.

## Non-exportable signer limitation

Hardware-backed or operating-system-backed private signing keys may be
non-exportable. The archive contains `SignerRecoveryPolicy`, not those private
keys. On a different machine, restore therefore reports
`ReprovisionRequired` for the affected network/feed capability while local,
network-off Base remains usable. An operator must provision a new authorized
signer and complete the later recovery workflow; the runtime must not silently
generate a replacement identity, copy an unrelated key, or fail all startup.

## Development and qualification status

`ProducerArtifactIdentityV1::Unknown` is the truthful value before the release
artifact tuple is available. Such an archive can be restored when its portable
data tuple is compatible, but it cannot support a qualified-release claim.
Task 16 supplies the one-way adapter to `Known(digest)`; changing producer
identity does not rewrite portable data compatibility.

## Verification commands

From the repository root:

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-archive --test dataset_roundtrip
python scripts/ci/validate_vnext_contracts.py
python scripts/ci/validate_mobile_build_contracts.py
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-mobile-core
```

The Node-owned snapshot/restore adapter and generation activation workflow are
implemented by later tasks. This task deliberately keeps `ku-core` ports
substrate-neutral and free of a dependency on `onebrain-archive`.
