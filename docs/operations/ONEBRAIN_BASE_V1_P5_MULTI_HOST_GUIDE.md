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
4. Confirm Registry qualification is fresh and candidate-bound.
5. Confirm every P5 and soak private key is readable only from the external
   secret mount and no private-key bytes exist in the checkout or artifacts.
6. Use the manual `vnext-p5-production-canary.yml` workflow under the protected
   `base-v1-production-qualification` environment.

Do not pass a candidate SHA, tree, Registry root or session as a dispatch
input. The workflow derives these values from the verified request and fails
closed when checkout or measured bytes differ.

## Required external secret mounts

- `ONEBRAIN_BASE_RELEASE_REQUEST`
- `ONEBRAIN_BASE_RELEASE_REQUEST_SIGNATURE`
- `ONEBRAIN_QUALIFICATION_APPROVER_POLICY`
- `ONEBRAIN_QUALIFICATION_GPG_HOME`
- `ONEBRAIN_TASK28_REGISTRY_STAGE_ROOT`
- `ONEBRAIN_REGISTRY_PROCESSED_ROOT`
- `ONEBRAIN_REGISTRY_CHECKPOINT_ROOT`
- `ONEBRAIN_REGISTRY_CANONICAL_INPUT`
- `ONEBRAIN_REGISTRY_PRIVATE_KEY_FILE`
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
evidence root, and Registry candidate stage named above. The raw root is
verified on every soak host but is never uploaded as a public workflow
artifact. A path that exists only on the P5 controller is insufficient.

The P5 V2 request must remain valid through Registry qualification, all three
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
  registry/task28-registry-binding.json
  registry/production-aggregate.json
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

1. Build the external full-size Registry candidate and stop if the exact sum
   of `concepts.obr`, its label and CCID indexes, and its manifest is outside
   the frozen 2.2--2.5 GB interval. Do not count SBOM, stamp, verification, or
   runtime REDb bytes.
2. Create and sign one Task 28 request v2 for the exact clean candidate.
3. Produce a fresh exact-request P5 V2 run on the three physical VPS hosts and
   retain its full native bundle plus protected raw evidence.
4. Dispatch `vnext-p5-production-canary.yml`. The workflow independently
   remeasures Registry evidence, re-verifies P5 V2, and only then starts three
   fresh 72-hour soak lanes.
5. Treat any changed candidate byte, request expiry, missing mount, child
   failure, interrupted interval, or aggregate mismatch as a new attempt.

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
