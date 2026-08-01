# OneBrain vNext — Concept Registry Operations Profile v1

> **Plan lane:** Section 11 — Concept Registry operations
>
> **Status:** Signed release, atomic activation, bounded CCID stability gate, and cold-cache/low-RAM qualification harness implemented; full-size resource and recurring qualification evidence remains open
>
> **Machine contract:** [`concept-registry-operations-v1.json`](../../../src/test-vectors/vnext/concept-registry-operations-v1.json)

## 1. Security boundary

One release is a unique directory containing exactly the OBR, label index,
CCID index, manifest, SPDX SBOM, and `release.stamp.json`. The stamp binds the
five payload artifacts, their aggregate BLAKE3 root, source snapshot and
download hashes, licenses, builder version, dedup policy, distribution policy,
and pinned Ed25519 signer.

Package publication copies and flushes files into a uniquely named staging
directory, verifies the complete package without trusting the legacy unsigned
verification cache, and atomically renames it. An existing release identifier
is never overwritten.

## 2. Activation and rollback

Activation state is append-only at
`state/state-<20-digit-generation>.json`. Each state record has its own
domain-separated root and names the active and previous immutable releases.

An interrupted staging directory is not eligible for activation. A malformed
or truncated newest state record is ignored in favor of the latest valid
generation. Rollback verifies the previous package and appends it as a new
generation, leaving both old and new release directories intact.

## 3. Runtime policy

The node accepts `concept_registry_release_root` together with a pinned
`concept_registry_release_public_key`. Supplying only one field, combining a
release root with a direct OBR path, using an untrusted signer, or encountering
any corrupt artifact is an explicit registry failure.

`required` mode stops node initialization before subsystem side effects and
does not select encoder v1. `optional` mode can expose the failure and select
the existing v1 fallback. A signed release is verified uncached before the
indexed backend opens it, so runtime startup does not add a mutable cache file
to the immutable release directory.

## 4. Operator commands

Run the portable operator CLI from `src/`:

```text
cargo run --locked -p ku-core --example concept_registry_release -- keygen PRIVATE_KEY_FILE PUBLIC_KEY_FILE
cargo run --locked -p ku-core --example concept_registry_release -- package REGISTRY_ROOT RELEASE_ID OBR_PATH SPDX_SBOM_PATH SOURCES_JSON_PATH PRIVATE_KEY_FILE
cargo run --locked -p ku-core --example concept_registry_release -- verify RELEASE_DIR PUBLIC_KEY_FILE
cargo run --locked -p ku-core --example concept_registry_release -- activate REGISTRY_ROOT RELEASE_ID PUBLIC_KEY_FILE
cargo run --locked -p ku-core --example concept_registry_release -- status REGISTRY_ROOT PUBLIC_KEY_FILE
cargo run --locked -p ku-core --example concept_registry_release -- rollback REGISTRY_ROOT PUBLIC_KEY_FILE
```

The source JSON is an array with one record for each of `chebi`, `geonames`,
`ncbi`, `wikidata`, and `wordnet`. Every record contains `name`, `snapshot_id`,
`source_uri`, `license`, `snapshot_blake3`, and `download_blake3`.

The private key file is created with create-new semantics and mode `0600` on
Unix. It is never printed by the CLI. The public key is safe to place in node
configuration.

## 5. CCID stability gate

Compare two exact builder inputs against the CCIDs actually stored in their
corresponding OBR artifacts:

```text
python scripts/concept_registry/ccid_stability_diff.py --old-input OLD_INPUT.jsonl --old-obr OLD.concepts.obr --old-manifest OLD.concepts.obr.manifest.json --new-input NEW_INPUT.jsonl --new-obr NEW.concepts.obr --new-manifest NEW.concepts.obr.manifest.json --work-dir WORK_DIRECTORY --output ccid-stability-report.json
```

The gate validates both manifests and streams both OBR files. It reconstructs
the stable numeric or string source identity from each exact builder input and
checks that it matches the adjacent OBR entry before persisting the identity and
actual OBR CCID into a temporary SQLite database. The disk-backed join keeps
memory bounded at production registry size.

Qualification requires at least one stable identity shared by both releases,
no changed CCID for a shared identity, and no CCID collision within either
release. The deterministic JSON report also records old-only/new-only counts,
bounded failure samples, exact input/OBR/manifest hashes, builder/dedup versions,
and source snapshot identities. Invalid, truncated, mismatched, duplicate, or
trailing-byte input fails before a report can qualify.

## 6. Cold-cache and low-RAM qualification harness

Build the frozen Rust probe first:

```text
cargo build --release --locked --manifest-path src/Cargo.toml -p ku-core --example registry_probe
```

Then run one of the fixed production budget profiles. The examples use the
Unix probe path; append `.exe` on Windows:

```text
python scripts/concept_registry/resource_qualification.py --profile cold-cache --budget-profile cold-cache-production-v1 --probe src/target/release/examples/registry_probe --obr REGISTRY/concepts.obr --labels-file scripts/concept_registry/qualification-labels-v1.txt --cache-strategy auto --output EVIDENCE/cold-cache.json
python scripts/concept_registry/resource_qualification.py --profile low-ram --budget-profile low-ram-production-v1 --probe src/target/release/examples/registry_probe --obr REGISTRY/concepts.obr --labels-file scripts/concept_registry/qualification-labels-v1.txt --output EVIDENCE/low-ram.json
```

Every run uses a fresh process, uncached artifact verification, application
lookup cache capacity zero, and an external bounded labels file so selecting
queries cannot warm the OBR. Cold-cache preparation either submits targeted
`POSIX_FADV_DONTNEED` requests on Linux or invokes `vmtouch -e`. Low-RAM
qualification applies Linux `RLIMIT_AS`; unsupported enforcement fails rather
than silently degrading to observation-only mode.

The harness samples peak RSS using `/proc/<pid>/status`, macOS `ps`, or Windows
PSAPI and atomically writes artifact hashes, host identity, frozen budgets,
probe output, latency/RSS metrics, and every exit oracle. At least one positive
and one negative lookup must be observed. Arbitrary operator thresholds are not
accepted by the qualifying CLI.

CI runs both profiles against a small fixture to lock the mechanism and failure
oracles. That CI result is not a substitute for reports against the full-size
registry on the declared production host; `full_registry_evidence_required`
therefore remains true.

## 7. Distribution boundary and remaining gates

The signed distribution value is
`MIRROR_OR_OFFLINE_ONLY_NO_OBP_GOSSIP`. The large registry artifact is outside
the OBP gossip lane; distribution uses a mirror, offline media, or a separately
specified content-addressed chunk transport.

This foundation does not claim full Section 11 qualification. The remaining
work packages produce cold-cache and constrained resource profiles,
truncated-index and disk-shortage drills, and the quarterly
build/update/rollback dry-run. Production canary remains blocked until those
gates and the external canary evidence are complete.
