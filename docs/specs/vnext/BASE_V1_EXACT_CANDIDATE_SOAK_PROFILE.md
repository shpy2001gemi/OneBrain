# OneBrain Base v1 — Exact-Candidate Soak Profile v1

> **Work package:** Base v1 Task 24 / `WS-22`
>
> **Status:** Frozen — signer identities approved by owner on 2026-08-11
>
> **Machine contract:** [`base-v1-exact-candidate-soak-v1.json`](../../../src/test-vectors/vnext/base-v1-exact-candidate-soak-v1.json)
>
> **Production execution owner:** Task 28, after Task 27 emits the only eligible candidate SHA

## 1. Fresh-candidate boundary

Base v1 qualification MUST run a fresh, uninterrupted 259,200-second soak on
the exact Task 27 commit and tree named by one verified signed Task 28 release
request. Task 25 is only an integration checkpoint and MUST NOT be treated as
the final candidate.

The old M5-07 72-hour report and any synthetically unchanged transitive closure
MUST NOT qualify Base v1. The carry-forward analyzer MAY classify unchanged
evidence as analytically reusable, but the Base v1 decision remains
`fresh_soak_required=true` and `production_qualified=false`.

## 2. Immutable release identity

The production workflow MUST derive the release-request digest, qualification
session, candidate commit/tree, semantic digest, frozen Linux artifact digest
and final candidate-bound Registry root from the verified signed request.

The P5 aggregate root, exact release executable, SBOM, provenance and runner
image digests MUST be derived from the same attempt and frozen before the first
soak interval. Dispatch inputs MUST NOT override any derived identity.

Every physical runner MUST check out the exact commit, verify the exact tree
and use a byte-identical release executable. A short commit, filename-only
identity, missing runner identity or mixed release request/session MUST fail
before qualifying work begins.

## 3. Signed interval and fault receipts

Every child receipt MUST bind its `soak-runner:<runner-id>` role, runner ID and
identity, release request/session, commit/tree, semantic and target artifact
digests, Registry/P5 roots, executable/SBOM/provenance digests, runner image,
trust-policy digest, monotonic interval, exact command, result and limitations.

Receipt signatures MUST use Ed25519 and the exact role-bound public keys and
fingerprints in the machine contract. A valid signature from an unlisted key,
wrong role, cross-runner key or changed trust policy MUST fail closed.

Raw child receipts MUST be retained even when a runner or aggregate fails. The
aggregator MUST recompute every signature and binding instead of trusting a
child or caller qualification boolean.

## 4. Aggregate root and duration

The aggregate root MUST cover only canonical child receipt bytes ordered by
runner ID, monotonic start, interval sequence and receipt kind. The aggregate
report and its detached `soak-aggregator` signature MUST remain outside that
root, preventing self-reference.

Each of the three physical runners MUST provide an uninterrupted monotonic
interval of at least 259,200 seconds, the complete frozen fault-cycle evidence
and identical release bindings. A gap, overlap, replay, missing fault, resource
failure or non-passing result MUST force `soak_qualified=false`.

## 5. Relationship to P5 and release state

The exact-candidate workflow MUST retain separate P5 multi-host and 72-hour
soak reports. P5 success MUST NOT substitute for elapsed soak time, and a soak
success MUST NOT substitute for the real three-host P5 fault matrix.

Task 24 MAY run three local processes to verify control and receipt plumbing,
but such evidence MUST be tagged `nonproduction-test` and MUST emit both
`multi_host_qualified=false` and `production_qualified=false`.

Production execution MUST wait for Task 27 and MUST occur only in Task 28. This
profile, workflow preparation, signer generation and dry-run are not measured
production evidence.

## 6. Signer approval boundary

The owner approved the exact public signer fingerprints, role bindings and
trust-policy digest in the machine vector on 2026-08-11. Private keys remain
outside the repository. Production use MUST still wait for Task 27 and the
fresh Task 28 signed release request; signer approval alone is not evidence.
