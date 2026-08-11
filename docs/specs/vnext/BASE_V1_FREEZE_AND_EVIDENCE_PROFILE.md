# Base v1 freeze and evidence profile

## Status and authority

This profile closes the Task 27 source contract for `BASE-GATE-V1`. The Task 27
commit is the only eligible candidate for Task 28. Changing the commit, tree,
compatibility tuple, qualifier tooling, signer policy, or evidence schema makes
all later qualification evidence stale.

The source version is `1.0.0`, but its runtime status remains `Unqualified`.
Only the pure Task 28 qualifier may derive `qualified=true` after verifying all
evidence. A version string, workflow success flag, target report boolean, tag,
or caller-provided `qualified` value is not qualification evidence.

## Exact manifest boundary

The canonical manifest uses UTF-8 JSON with sorted keys, compact separators,
and no insignificant whitespace. It binds:

- release-request digest and random 256-bit qualification-session ID;
- exact Git commit, tree, object format, and candidate semantic digest;
- complete compatibility tuple and artifact digest for Linux, Windows, and
  macOS;
- schema, domain, resource, storage, archive, Registry, wire, product API, and
  C ABI identities;
- feature defaults and kill switches;
- three-OS job receipts and canonical/vector/blob/index/archive/recovery/
  transaction/projection evidence;
- fresh Registry, fresh three-host P5, and fresh uninterrupted 72-hour soak
  aggregates bound to the same request, session, commit, tree, semantic digest,
  and target artifact;
- dependency audit and triage, SBOM, provenance, migration, rollback, and
  changelog bytes;
- raw evidence hashes, child signer roles/fingerprints/trust-policy digests,
  limitations, child signatures, and a separate evidence-approver signature
  for every target receipt and every gate not already authenticated by its
  frozen Registry, P5, or soak signer.

The dedicated production `base-evidence-approver` policy is owner-approved. Its
Ed25519 public key is
`c40d8892b480f80b78cb1acddaa5a85c571ac5adfac71ff1ccebd6c3f6abce42`;
the derive-key fingerprint is
`a5f274124c48fdc9c9c50a504733ac731e67a7dfdbcfe59d83bf5ed0c8944009`,
and the canonical public-policy digest is
`01f5989e96ca840b2ddc53781bd57dad18bf52fb332046543bfb1dbd42fb0df8`.
The policy is valid from `2026-08-11T07:40:40Z` through
`2028-08-10T07:40:40Z`, only for gate- and target-receipt approval. It must not
be replaced with a sample, borrowed from another role, or used outside that
interval. Production qualification and the contract validator independently
derive both identities from the exact frozen public bytes.

Registry and soak carry-forward are forbidden for Base v1. Missing, duplicate,
unknown, mixed-session, mixed-candidate, cross-target, stale, invalidly signed,
or false evidence fails closed. The manifest's own detached signature is an
outer envelope and cannot participate in the bytes or digest it signs.

Gate and target results are derived only from closed canonical machine receipts.
Each receipt records the exact command vector and its digest, exit code, and
hashed stdout/stderr evidence. A zero exit code is never sufficient: every
gate/target uses its frozen check name, command vector, candidate-bound runner
identity/provenance, and nonempty assertion set whose raw evidence and
canonical output-oracle bytes are rehashed. Unknown, missing, empty, or
caller-invented checks fail closed. Signed Registry, P5, and soak roots are
derived from the verified signed receipt bytes, not copied from outer caller
bindings. Target receipts additionally bind the binary to a substantive SPDX
2.3 document and in-toto/SLSA v1 provenance, artifact tuple, and three-target
artifact map. Because the package sets `filesAnalyzed=true`, its closed
`packageVerificationCode` is the SHA-1 of the concatenated, lexically sorted
SHA-1 digests of all analyzed files; the sole release binary is not excluded
and carries both SHA-1 and SHA-256 checksums. Each target freezes a distinct
absolute HTTPS TypeURI as its SLSA `builder.id`, separate from the operational
`runner_identity`; the receipt still binds that runner to the exact candidate,
command, and invocation. Outer metadata contains only a receipt identity and
digest; it has no accepted result field. Production and nonproduction
manifests carry distinct qualification tiers.

## Release publication

The qualification approver signs the external immutable release request. The
Base release signer signs a temporary manifest signature, verifies it, then
publishes a create-new content-addressed release envelope and checksummed ready
pointer. The annotated tag object is written unreferenced and verified before a
single compare-and-swap creates `refs/tags/base-v1.0.0`. No failure path deletes
or overwrites an existing ref. An existing ref is idempotent only when every
request, session, commit, manifest, signature, and tag-object binding matches.
The request validity is exactly 168 hours, covering the uninterrupted 72-hour
soak plus execution and evidence-finalization margin. Both manifest and tag
signature `VALIDSIG` timestamps are checked against the intersection of the
request and approved release-signer validity intervals. Before signing, the
production publisher resolves only the checksummed immutable manifest-ready
pointer, reruns the frozen qualifier, byte-compares its output with the supplied
immutable manifest, and performs the persisted candidate finalizer immediately
before any signer callback. Manifest generations contain exactly
`manifest.json` and `manifest.blake3`. The detached manifest signature is the
only file in `release-envelopes/<manifest-digest>/<signature-digest>/`; files,
generation directories, and their containing directories are fsynced before an
atomic create-new ready pointer. Exact retries reconstruct and compare the full
unsigned tag bytes, including tag name, commit/tree, request, session, manifest,
and envelope bindings, before verifying the signature.

## Compatibility, migration, and rollback

Base `1.0.x` permits correctness/security fixes without a semantic or wire
break. Base `1.x.0` permits additive optional capability changes with old-client
behavior preserved. Any canonical, authority, storage, archive, wire, API, or
ABI incompatibility reopens the program as Base v2 with a new design,
migration, rollback, vectors, and product requalification.

Legacy KU storage remains read-only migration evidence and cannot become a
Base write authority. Legacy runtime selection is explicit and default-off;
silent fallback is forbidden. Restore always materializes a staged generation,
verifies canonical/blob/feed/authority and derived-projection parity, then
atomically activates. Rollback restores the prior verified generation and
rebinds projections without reinterpreting canonical bytes.

## Completion boundary

Task 27 freezes source contracts only. Task 28 must run the exact signed request
in a pristine read-only candidate worktree, collect all production evidence,
derive the manifest, sign it, and atomically publish the verified tag. Until
then no Base source, product, or workflow may claim `BASE-GATE-V1` complete.
Candidate preparation has an explicit post-run finalization transition that
rechecks HEAD/tree, tracked hashes, index/filesystem equality, status, diff,
and external output locations after qualification returns.

## Normative rules

- The Task 28 run MUST use the exact Task 27 commit and tree.
- The release request MUST be canonical, immutable, content-addressed, and signed by the allowlisted qualification approver.
- Candidate source MUST remain read-only while every build, cache, temporary file, and evidence output is redirected outside it.
- The qualifier MUST derive `qualified`; it MUST NOT accept that value from an input or child report.
- Every gate/target check MUST match its frozen name, command, runner identity, and substantive nonempty output oracle; `exit_code=0` alone MUST NOT qualify.
- Every raw evidence digest MUST be recomputed from the supplied bytes.
- Linux, Windows, and macOS target receipts MUST bind their own tuple and artifact bytes without cross-target substitution, carry the exact analyzed-file SPDX package verification code, and use the target-frozen absolute SLSA builder TypeURI rather than the operational runner label.
- Registry, P5, and soak evidence MUST be fresh and MUST bind one request, session, candidate, semantic digest, and artifact map.
- Registry and soak evidence MUST NOT use carry-forward for Base v1.
- Every child signature MUST use its frozen signer role, full fingerprint, public key, and trust-policy digest.
- Every target receipt and every non-child-signed gate receipt MUST carry a valid Ed25519 `base-evidence-approver` signature bound to its exact receipt digest; production qualification MUST reconstruct the owner-approved canonical policy, derive its fingerprint and trust-policy digest, and reject a candidate request outside the approved signer interval.
- The publisher MUST finalize the exact persisted candidate receipt after requalification and before signing; any post-run tracked, untracked, ignored, generated, tooling, or filesystem mutation MUST leave the signer uncalled.
- Base MUST be the packaged default; legacy and network transport MUST remain explicit and default-off.
- The manifest signature MUST remain outside the canonical manifest bytes and digest.
- The final tag ref MUST be created only by a single compare-and-swap after its object, manifest envelope, and ready pointer verify exactly.
