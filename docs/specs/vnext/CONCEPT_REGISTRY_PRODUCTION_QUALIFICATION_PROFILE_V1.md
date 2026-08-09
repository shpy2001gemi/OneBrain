# OneBrain vNext — Concept Registry Production Qualification Profile v1

> **Program lane:** Base v1 WS-21 — Registry production kernel
>
> **Status:** Contract frozen; production measurements are not yet complete
>
> **Machine contract:** [`concept-registry-production-qualification-v1.json`](../../../src/test-vectors/vnext/concept-registry-production-qualification-v1.json)

## 1. Scope and non-claim

This profile freezes the only production-reference contract for the Base v1
Concept Registry. It does not turn the existing CI fixture, a portability run,
or a component prequalification into production evidence. A report **MUST NOT**
claim `registry_production_qualified=true` until every fresh release-mode report
passes the Task 20 aggregator against one exact Base candidate.

The `production_profile_blake3` receipt field is BLAKE3 over the complete
profile object serialized as UTF-8 JSON with sorted keys and no insignificant
whitespace. The profile does not embed that digest value and therefore avoids
self-reference.

The small-fixture mechanisms in the operations profile remain valuable
preflight evidence. They **MUST** continue to emit `base_candidate_bound=false`
and are never a substitute for the full-size runs defined here.

## 2. Exact release package and aggregate root

A qualifying release **MUST** contain exactly these five payload artifacts in
the closed set below, plus a separate `release.stamp.json`:

1. `OBR` — `concepts.obr`
2. `LABEL_INDEX` — `concepts.obr.labels.idx`
3. `CCID_INDEX` — `concepts.obr.ccids.idx`
4. `MANIFEST` — `concepts.obr.manifest.json`
5. `SPDX_SBOM` — `sbom.spdx.json`

The aggregate root **MUST** use BLAKE3 with the exact byte domain
`6f6e65627261696e3a636f6e636570742d72656769737472792d6172746966616374733a3100`
(`onebrain:concept-registry-artifacts:1` followed by NUL). Rows are ordered
bytewise by role and then relative path. Each UTF-8 string is framed by its
unsigned 64-bit big-endian byte length. Each row frames the role and path,
includes the exact unsigned 64-bit big-endian artifact length, and frames the
lowercase ASCII BLAKE3 hex digest.
The stamp **MUST NOT** be included in the root it signs. The root in the stamp,
every child receipt, and the aggregate report **MUST** match exactly.

The stamp signature **MUST** use Ed25519 over the existing release-stamp
message: the byte domain
`6f6e65627261696e3a636f6e636570742d72656769737472792d72656c656173652d7374616d703a3100`
followed by the BLAKE3 of the unsigned stamp serialized in frozen Rust struct
field order. The unsigned transform clones the full stamp and sets its
`signature` member to the empty string; that empty member remains present as
the final serialized field. Its signed fields include the release ID used by
activation, the five artifact rows/root, source rows/root, builder and dedup
versions, distribution policy, signer public key, and empty signature member.

`concepts.obr` is production-size only when its exact length is between
2,200,000,000 and 2,500,000,000 bytes inclusive. An otherwise valid release
outside this interval **MUST** fail the production gate.

## 3. Frozen resource profiles

All limits are inclusive. Ready time includes uncached signature, manifest,
and artifact verification before the probe is ready for lookup.

| Profile | Ready | Lookup p95 | Peak RSS | Additional proof |
|---|---:|---:|---:|---|
| Cold cache | 180 s | 250 ms | 512 MiB | Linux `POSIX_FADV_DONTNEED` requests or successful `vmtouch -e` eviction |
| Low RAM | 300 s | 500 ms | 256 MiB | Linux `RLIMIT_AS` fixed at 3 GiB |
| SSD | 120 s | 100 ms | 512 MiB | `findmnt` source/filesystem plus resolved sysfs block device with `rotational=0` |
| Rotational HDD | 300 s | 750 ms | 512 MiB | `findmnt` source/filesystem plus resolved sysfs block device with `rotational=1` |

The SSD/HDD class **MUST** come from captured OS evidence for the filesystem
that contains the candidate release. A free-form operator label, an unknown
device mapping, or missing evidence **MUST** fail closed.

## 4. Reference target and immutable environment identity

The production reference target is `x86_64-unknown-linux-gnu`. Cold-cache,
low-RAM, SSD, and HDD producers **MUST** receive the Rust toolchain digest,
runner-image digest, probe BLAKE3, probe signature, and probe signer fingerprint
from the verified signed release request. Those inputs **MUST NOT** be replaced
by command-line or environment overrides.

The target, toolchain digest, runner-image digest, and probe BLAKE3 **MUST** be
byte-identical across all reference reports. The probe signature **MUST** verify
under the artifact-signing policy named by the signed request. Windows and
macOS collectors remain portability/preflight coverage and **MUST NOT** claim
production-reference status.

This design deliberately binds the not-yet-built release probe and runner image
through the signed release request instead of placing invented binary digests
in this profile. Task 21 will build and sign the exact bytes consumed by every
reference host.

## 5. Registry signer trust policy

The owner-approved Ed25519 Registry signer has:

- public key: `bef8e2b9d8ae7a38b3753a7d756a39c20948f128a66ca71ed04799e7a5d5177c`
- fingerprint: `dcc09574ac53ec8b95585cad5e2e88cbdfbe44841ad46b3709f73c989b4316d4`
- trust-policy digest: `e0a2551a39823c3f2cb088defe60484c8a33ffe0f3aab9df9493b52557ab55fe`

The fingerprint is BLAKE3 derive-key v1 over the raw 32-byte public key with
context `onebrain:concept-registry:signer-fingerprint:1`. The policy digest is
BLAKE3 derive-key v1 over UTF-8 JSON with sorted keys and no insignificant
whitespace, using context `onebrain:concept-registry:trust-policy:1`. The exact
canonical policy object is embedded in the machine contract.

The allowlist permits only `registry-release-stamp` and
`registry-qualification-receipt` usages. `release.stamp.json` and every Registry
evidence receipt **MUST** verify against this policy and carry the frozen policy
digest. A cryptographically valid signature from any unlisted key **MUST** be
rejected.

Every qualification receipt **MUST** use the closed envelope
`onebrain/concept-registry-qualification-receipt/1`. Its exact fields are
`format`, `receipt_kind`, `usage`, `payload`, `signer_public_key`,
`signer_fingerprint`, `trust_policy_digest`, and `signature`; unknown fields
**MUST** be rejected. The unsigned transform sets `signature` to the empty
string, retains that field, and serializes the full envelope as UTF-8 JSON with
sorted keys and no insignificant whitespace.

The receipt signature message **MUST** be the byte domain
`6f6e65627261696e3a636f6e636570742d72656769737472792d7175616c696669636174696f6e2d726563656970743a3100`
followed by BLAKE3 of those canonical unsigned-envelope bytes. `usage` is
exactly `registry-qualification-receipt`; the closed kinds are resource,
failure, generation-swap, CCID-stability, signed-release-cycle, and production
aggregate receipts.

Every receipt payload **MUST** bind the common report identity list from the
machine contract plus its exact command, result, exit oracles, and limitations.
A signed `evidence_tier` is mandatory. It is exactly `prequalification` for
fixture/prequalification receipts, `nonproduction-test` for explicit ephemeral
Release helpers, and `production-reference` only for the fixed Linux production
path. Production aggregation rejects any other tier before signer availability
can become the only fence; `base_candidate_bound` remains a separate fact.
A prequalification payload additionally binds `closure_digest`, requires
`base_candidate_bound=false`, and forbids release-request/session/commit/tree
fields. A release payload instead binds the full release request/session,
candidate commit/tree, semantic and artifact-tuple digests, and requires
`base_candidate_bound=true`. Changing usage, kind, signer, policy digest,
payload, or the empty-signature transform **MUST** invalidate verification.

## 6. Closed qualification run context

Every producer **MUST** accept one closed `QualificationRunContextV1` variant:

- `Prequalification { closure_digest }` always emits
  `base_candidate_bound=false` and cannot produce a production aggregate.
- `Release { release_request_digest, qualification_session_id,
  candidate_commit, candidate_tree }` is accepted only after the signed request
  verifies and all four values match it exactly.

Missing context, mixed release-request/session values, or a producer override
**MUST** fail closed. Only `Release` may emit candidate-bound evidence. Every
fresh production report **MUST** bind the same request/session, commit/tree,
candidate semantic digest, artifact tuple digest, Registry root/generation,
profile and trust-policy digests, allowlisted signer, probe/executable hashes,
five candidate payload hashes, and stamp hash.

Candidate semantic evidence is canonical `BaseCompatibilityTuple` JSON whose
bytes are independently encoded with the frozen Base field encoder. The
semantic digest covers fields 1-14; the artifact tuple digest covers fields
1-16 and therefore adds only the independently measured target triple and
toolchain identity. Payload, stamp, executable, probe, and runner bindings are
separate mandatory measured prerequisites and are not new tuple fields.

The production request also pins BLAKE3 identities for fixed `/usr/bin/python3`
and `/usr/bin/gpg`. Rust independently verifies canonical request/policy bytes,
OpenPGP `VALIDSIG` fingerprint/algorithm/time, fixed tooling bytes, and every
Python-derived binding before accepting a closed production context.

`registry_production_qualified` is a Registry-only subgate. It **MUST NOT** be
interpreted as `BASE-GATE-V1`, and Base v1 **MUST NOT** accept carry-forward
Registry reports in place of a fresh exact-candidate run.

## 7. Required failure and release-cycle gates

Production evidence **MUST** include all of:

- truncated label and CCID indexes;
- disk shortage before staging/publication;
- process kills around update publication and activation-state append;
- live readers pinned to a complete old generation while new readers see only
  the complete new generation;
- rollback with active readers and exact-root verification after reopen;
- CCID stability over the exact previous and candidate builder inputs; and
- one complete signed release-cycle drill.

The signed release-cycle harness **MUST** package, verify, activate, query,
build a new signed generation, run CCID diff, activate the new generation,
rollback, and reactivate the new generation. `quarterly_update.py` is only an
operational dry run and **MUST NOT** be accepted as this signed release cycle.

## 8. Qualification state

Freezing this profile closes the contract-design part of Task 19 only. It does
not create the large artifacts or measurements required by Tasks 20–21. Until
those reports exist and the exact-candidate aggregate passes, the authoritative
state remains `production_qualified=false`.
