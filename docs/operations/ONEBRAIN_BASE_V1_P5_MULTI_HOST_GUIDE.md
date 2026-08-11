# OneBrain Base v1 P5 Multi-Host and Exact-Candidate Soak Guide

This guide prepares Task 28 operators; it does not authorize a production run.
The machine contracts are
[`P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V1.md`](../specs/vnext/P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V1.md)
and
[`BASE_V1_EXACT_CANDIDATE_SOAK_PROFILE.md`](../specs/vnext/BASE_V1_EXACT_CANDIDATE_SOAK_PROFILE.md).

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
- `ONEBRAIN_P5_INVENTORY`
- `ONEBRAIN_P5_AGENT_SIGNATURE`
- `ONEBRAIN_P5_ORCHESTRATOR_PRIVATE_KEY`
- runner-specific `ONEBRAIN_SOAK_RUNNER_A_PRIVATE_KEY`,
  `ONEBRAIN_SOAK_RUNNER_B_PRIVATE_KEY` and
  `ONEBRAIN_SOAK_RUNNER_C_PRIVATE_KEY`
- `ONEBRAIN_SOAK_AGGREGATOR_PRIVATE_KEY`

The current development key directory is external to Git at
`C:\Users\shpy2\.onebrain\soak-signing\base-v1`. Production runners should
mount only their own role key; the aggregator should mount only its key.

## Evidence layout

Keep immutable attempts under the release-request digest:

```text
target/base-v1/evidence/<release-request-digest>/
  verified-release-request.json
  p5/raw/
  p5/p5-multi-host-aggregate.json
  soak/raw/<runner-id>/
  soak/base-v1-exact-candidate-soak-aggregate.json
  carry-forward/legacy-m5-07-analysis.json
```

P5 and soak outputs remain separate. Retain raw signed receipts even after a
failure; never overwrite a previous attempt and never edit a report to satisfy
a duration or root gate.

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
