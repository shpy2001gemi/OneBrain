# OneBrain Base v1 P5 Multi-Host and Exact-Candidate Soak Guide

This guide prepares Task 28 operators; it does not authorize a production run.
The machine contracts are
[`P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V2.md`](../specs/vnext/P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V2.md)
and
[`BASE_V1_EXACT_CANDIDATE_SOAK_PROFILE.md`](../specs/vnext/BASE_V1_EXACT_CANDIDATE_SOAK_PROFILE.md).
P5 V1 remains an observe-only compatibility contract and can never satisfy the
Task 28 multi-host gate.

## Admission checklist

1. Confirm Task 27 has produced the only eligible commit and clean tree.
2. Stage the detached signed Task 28 release request, its owner-approved policy
   and isolated GPG home on the qualification controller.
3. Confirm three distinct P5 hosts and three distinct soak runners match their
   signed inventories, host keys, durable roots, principals and runner roles.
4. Confirm the owner-local prebuilt Registry binding is newly signed for this
   request and that all five final `onebrain_data` files rehash exactly.
5. Confirm every P5 and soak private key is readable only from the external
   secret mount and no private-key bytes exist in the checkout or artifacts.
6. Use the manual `vnext-p5-production-canary.yml` workflow under the protected
   `base-v1-production-qualification` environment.

Do not pass a candidate SHA, tree, Registry root or session as a dispatch
input. The workflow derives these values from the verified request and fails
closed when checkout or measured bytes differ.

The controller first authenticates the signed request with the frozen approver
allowlist without executing candidate code.  It then checks out the resulting
full commit into a separate clean directory and reruns the verifier from those
exact signed-candidate bytes before publishing any workflow identity output.

## Required external secret mounts

- `ONEBRAIN_BASE_RELEASE_REQUEST`
- `ONEBRAIN_BASE_RELEASE_REQUEST_SIGNATURE`
- `ONEBRAIN_QUALIFICATION_APPROVER_POLICY`
- `ONEBRAIN_QUALIFICATION_GPG_HOME`
- `ONEBRAIN_TASK28_REGISTRY_PREBUILT_ROOT`
- `ONEBRAIN_TASK28_REGISTRY_PREBUILT_BINDING`
- `ONEBRAIN_CANDIDATE_SEMANTIC_EVIDENCE`
- `ONEBRAIN_BASE_SBOM_FILE`
- `ONEBRAIN_BASE_PROVENANCE_FILE`
- `ONEBRAIN_RUNNER_IMAGE_EVIDENCE_FILE`
- `ONEBRAIN_P5_V2_REQUEST_FILE`
- `ONEBRAIN_P5_V2_SIGNATURE_FILE`
- `ONEBRAIN_P5_V2_APPROVAL_POLICY`
- `ONEBRAIN_P5_V2_INVENTORY_FILE`
- `ONEBRAIN_P5_V2_RAW_EVIDENCE_ROOT`
- `ONEBRAIN_P5_V2_AGGREGATE_FILE`
- `ONEBRAIN_P5_V2_BUNDLE_ROOT`
- runner-specific `ONEBRAIN_SOAK_RUNNER_A_PRIVATE_KEY`,
  `ONEBRAIN_SOAK_RUNNER_B_PRIVATE_KEY` and
  `ONEBRAIN_SOAK_RUNNER_C_PRIVATE_KEY`
- `ONEBRAIN_SOAK_AGGREGATOR_PRIVATE_KEY`

The P5 controller runner and every soak runner must mount the same read-only
full native bundle, P5 request/policy/inventory/aggregate, protected raw P5
evidence root, and read-only prebuilt Registry root named above. The raw root is
verified on every soak host but is never uploaded as a public workflow
artifact. A path that exists only on the P5 controller is insufficient.

No VPS receives a checkpoint source archive or runs Wikidata/checkpoint
extraction, cold-cache, low-RAM, SSD, or HDD Registry qualification. The three
moderate VPS are reserved for real multi-host P5 networking, faults, recovery,
and soak. Each VPS only stores and rehashes the approximately 2.2 GB final
Registry output plus bounded P5/soak state.

The P5 V2 request must remain valid through prebuilt Registry verification, all three
uninterrupted 72-hour soak runs, and final aggregation. It must be nested within
the 168-hour Base request interval. If either request expires, preserve the
attempt and create a new signed request; never extend timestamps in place.

The current development key directory is external to Git at
`C:\Users\shpy2\.onebrain\soak-signing\base-v1`. Production runners should
mount only their own role key; the aggregator should mount only its key.

## Evidence layout

Keep immutable attempts under the release-request digest:

```text
target/base-v1/evidence/<release-request-digest>/
  verified-release-request.json
  registry/prebuilt-registry-binding.json
  registry/prebuilt-registry-verified.json
  p5/public/p5-request.json
  p5/public/p5-request.sig
  p5/public/p5-approval-policy.json
  p5/public/p5-inventory.json
  p5/public/p5-multi-host-aggregate.json
  p5/restricted-raw/                 # protected mount; never public artifact
  soak/raw/<runner-id>/
  soak/base-v1-exact-candidate-soak-aggregate.json
  carry-forward/legacy-m5-07-analysis.json
```

Registry, P5 and soak outputs remain separate. Retain raw signed receipts even after a
failure; never overwrite a previous attempt and never edit a report to satisfy
a duration or root gate.

## Production ordering

1. On the owner workstation only, form the logical package directly from the
   existing `onebrain_data` output; no second 2.2 GB local copy is required.
   Stop if the exact sum of `concepts.obr`, its label and CCID indexes, and its
   manifest is outside the frozen 2.2--2.5 GB interval. Do not count the
   verification receipt or runtime REDb bytes, and do not rerun checkpoint
   extraction.
2. Create and sign one Task 28 request v2 for the exact clean candidate, then
   create a new signed prebuilt Registry binding for that request.
3. Copy only the five bound Registry output files and public binding to the
   same protected read-only path on all three VPS.
4. Produce a fresh exact-request P5 V2 run on the three physical VPS hosts and
   retain its full native bundle plus protected raw evidence.
5. Dispatch `vnext-p5-production-canary.yml`. The workflow independently
   rehashes the prebuilt Registry, re-verifies P5 V2, and only then starts three
   fresh 72-hour soak lanes.
6. Treat any changed candidate byte, request expiry, missing mount, child
   failure, interrupted interval, or aggregate mismatch as a new attempt.

## Local-only prebuilt Registry binding

After the new candidate is committed and its Task 28 request is signed, run the
binding creator in WSL from that exact pristine checkout. `REGISTRY_ROOT` may
point directly at `onebrain_data`: the binding selects exactly the five named
final files and ignores runtime REDb files. It is not a checkpoint or
extraction directory, and the deployment copies only those five selected files.

```bash
python scripts/release/task28_prebuilt_registry.py prepare \
  --release-request "$REQUEST_FILE" --release-signature "$REQUEST_SIGNATURE" \
  --base-policy "$APPROVER_POLICY" --base-gpg-home "$QUALIFICATION_GPG_HOME" \
  --registry-root "$REGISTRY_ROOT" --candidate-root "$PWD" \
  --candidate-semantic-evidence "$CANDIDATE_SEMANTIC_EVIDENCE" \
  --signing-key "$REGISTRY_SIGNING_KEY" \
  --output "$ATTEMPT_ROOT/prebuilt-registry-binding.json"
```

The private Registry signing key remains on the owner workstation. Copy only
the five final Registry files and `prebuilt-registry-binding.json` to the three
VPS. The workflow has no Registry private-key secret and cannot rebuild or
resign Registry output.

## Dry-run

Run:

```powershell
python -m unittest scripts.ci.test_validate_base_v1_soak_profile.ExactCandidateSoakProfileTests.test_three_local_process_p5_dry_run_cannot_claim_multi_host -v
```

The three local child processes exercise bounded control/receipt round trips.
Success means the protocol passes while `multi_host_qualified=false`; it is not
physical-host evidence.

## Old evidence analysis

The committed M5-07 macOS ARM64 fixture lacks a signed Base release request,
full commit/tree, qualification session and runner identity. The Task 24 test
records it as `legacy-m5-07`, `base_v1_reusable=false` and
`fresh_soak_required=true`.

## Abort and incident rules

Abort when any signature, role, host, runner, commit/tree, artifact, Registry,
P5, duration, resource or monotonic interval check fails. Preserve the attempt,
fence distributed lanes if runtime state changed, and open a new signed request
for any retry that changes the candidate or evidence identity.

## Outbound-first P5 V2 deployment

The exact relay, forced-command SSH, signer isolation, systemd, namespace,
UFW/NAT, bootstrap/prepare, and two-phase cleanup procedure is frozen in
[`ONEBRAIN_OUTBOUND_FIRST_RELAY_GUIDE.md`](ONEBRAIN_OUTBOUND_FIRST_RELAY_GUIDE.md).
General nodes never require an inbound public port or operator-managed NAT.
Only volunteers choosing to expose a permissionless relay need a remotely
reachable UDP or TLS/TCP-443 transport. Relay operation conveys availability,
not trust, route authority, knowledge authority, or production qualification.
