# OneBrain Concept Registry Qualification Runner Guide v1

> **Scope:** Task 21 preparation and optional full-size component
> prequalification for the Base v1 Concept Registry.
>
> **Production contract:**
> [`CONCEPT_REGISTRY_PRODUCTION_QUALIFICATION_PROFILE_V1.md`](../specs/vnext/CONCEPT_REGISTRY_PRODUCTION_QUALIFICATION_PROFILE_V1.md)
>
> **Runner:**
> [`onebrain-registry-runner.sh`](../../scripts/runner/onebrain-registry-runner.sh)

## 1. Non-claim

This runner prepares and measures the 2.2 GB-class Registry component.
Task 21 prequalification is not `BASE-GATE-V1`, is not candidate-bound, and cannot
make `registry_production_qualified=true`. Only the Task 28 fresh release run
against its exact signed Base candidate may produce the Registry production
subgate.

The existing `vnext-foundation.yml` Registry job remains fixture-only. It is a
fast pull-request regression lane and is never an input fallback for this
runner.

Never commit measured reports. Raw reports, closure manifests, receipts, and
summaries stay in external immutable artifact storage. The repository contains
only the runner, workflow, guide, tests, and validator references.

## 2. Trust boundaries

The runner accepts only these operator choices:

- command: `preflight`, `closure`, `build`, `resource`, `kernel`, or
  `aggregate`;
- mode: `prequalification` or `release`; and
- resource profile: `cold-cache`, `low-ram`, `ssd`, or `hdd`.

It does not accept a candidate path, previous-release path, evidence path,
release-request digest, qualification-session ID, candidate commit, candidate
tree, Registry root, probe path, tool path, or closure digest. Those values are
fixed by the reviewed checkout, staged layout, or verified signed request.

The Registry Ed25519 private key is supplied only by the external
`ONEBRAIN_REGISTRY_PRIVATE_KEY_FILE` path. The runner resolves the path and
rejects it if it is inside the repository. The qualification-approver GPG home
is supplied by `ONEBRAIN_QUALIFICATION_GPG_HOME` and also remains external.
Neither private key is uploaded as evidence.

In `release` mode, fixed `/usr/bin/python3` invokes
`verify_base_release_request.py`, which in turn uses fixed `/usr/bin/gpg` and
the owner-approved qualification-approver policy. Producers derive the release
request digest, qualification-session ID, candidate commit, and candidate tree
from that verified result. Workflow dispatch inputs cannot override them.

## 3. Reference hosts and immutable labels

All production-reference hosts are Linux x86_64 and use target
`x86_64-unknown-linux-gnu`. Register separate self-hosted runners with these
code-owned label sets:

| Lane | Required labels |
|---|---|
| Cold cache | `self-hosted,linux,x64,onebrain-registry-image-v1,onebrain-registry-cold-cache` |
| Low RAM | `self-hosted,linux,x64,onebrain-registry-image-v1,onebrain-registry-low-ram` |
| SSD | `self-hosted,linux,x64,onebrain-registry-image-v1,onebrain-registry-ssd` |
| HDD | `self-hosted,linux,x64,onebrain-registry-image-v1,onebrain-registry-hdd` |
| Controller | `self-hosted,linux,x64,onebrain-registry-image-v1,onebrain-registry-controller` |

Do not expose a free-form runner-label workflow input. Changing a label means
reviewing and committing a workflow change. The host receipt must name the
same labels, target, and runner-image evidence digest.

The SSD and HDD labels are admission hints, not proof. The resource producer
captures `findmnt`, resolved sysfs block-device, and `rotational` evidence for
the filesystem that contains the candidate. Missing or ambiguous device
evidence fails closed. Low-RAM enforcement uses Linux `RLIMIT_AS`; cold-cache
uses `POSIX_FADV_DONTNEED` or verified `vmtouch` eviction.

## 4. Fixed staging layout

Stage external, immutable input bytes below the runner checkout without adding
them to Git:

```text
target/base-v1/registry/
├── previous/
│   ├── input.jsonl
│   ├── concepts.obr
│   ├── concepts.obr.labels.idx
│   ├── concepts.obr.ccids.idx
│   ├── concepts.obr.manifest.json
│   ├── sbom.spdx.json
│   ├── release.stamp.json
│   ├── state.json
│   └── sources.json
├── candidate/
│   ├── input.jsonl
│   ├── concepts.obr
│   ├── concepts.obr.labels.idx
│   ├── concepts.obr.ccids.idx
│   ├── concepts.obr.manifest.json
│   ├── sbom.spdx.json
│   ├── release.stamp.json
│   ├── state.json
│   └── sources.json
└── environment/
    ├── runner-image.json
    ├── rust-toolchain.json
    ├── registry_probe.sig
    ├── registry-trust-policy.json
    ├── release-public-key.hex
    ├── query-label.txt
    ├── candidate-semantic-evidence.json
    ├── append-only-idl-history-root.txt
    ├── host-environment-receipt.json
    ├── release-request.json       # release mode only
    └── release-request.json.asc   # release mode only
```

Every staged input must be a regular file, not a symlink. The sum of the exact
candidate data bytes in `concepts.obr`, `concepts.obr.labels.idx`,
`concepts.obr.ccids.idx`, and `concepts.obr.manifest.json` must be between
2,200,000,000 and 2,500,000,000 bytes inclusive. The SBOM, release stamp,
verification receipt, and runtime REDb files do not count. There is no
small-fixture fallback.

`registry-trust-policy.json` is the canonical `policy` object frozen by the
Task 19 machine contract. The external Registry private key must resolve to
the one allowlisted public key and fingerprint.

## 5. Registry closure

Run:

```bash
bash scripts/runner/onebrain-registry-runner.sh build --mode prequalification
bash scripts/runner/onebrain-registry-runner.sh closure --mode prequalification
```

The runner computes `registry_closure_digest` with domain
`onebrain:concept-registry-closure:1\0`. The canonical closure contains sorted
logical-path rows with exact length and BLAKE3 for:

- Registry source, requirements, profile, vector, labels, validator, runner,
  lockfile, and append-only IDL history;
- exact old/new builder input bytes;
- all five old/new payload artifacts and both release stamps;
- old/new state bytes, release roots, generations, and release IDs;
- sources metadata, signer policy, signer identity, and trust-policy digest;
- target/toolchain/runner-image evidence;
- exact signed probe and probe-signature bytes; and
- signed-request bytes in release mode.

The closure is recomputed independently on every host. All component reports
must carry the same digest and release root. A caller-provided closure digest
is rejected. The digest supports Task 21 comparison only; Task 28 must freshly
rerun every release producer against the exact final candidate.

Outputs:

```text
target/base-v1/evidence/prequalification/registry/registry-closure.json
target/base-v1/evidence/prequalification/registry/registry-closure.blake3
```

## 6. Manual workflow

Run the GitHub Actions workflow **Concept Registry production qualification**
manually. It has no pull-request, push, schedule, or reusable-workflow trigger.
Choose only `prequalification` during Task 21. The four resource jobs run on
separate immutable host labels, and the controller runs the fixed failure,
CCID, and live-reader/process-kill lanes.

The workflow retains raw reports for 90 days even when a job fails. Download
and move the complete artifacts to the owner-approved immutable evidence
store. Never retain only the summary; the raw receipt bytes are authoritative.

`release` exists for the later exact-candidate run. It requires the signed Base
release request and owner-approved GPG keyring. The current workflow never
takes request/session/commit/tree as dispatch parameters.

## 7. Local prequalification commands

Export external secret paths without printing their contents:

```bash
export ONEBRAIN_REGISTRY_PRIVATE_KEY_FILE=/secure/onebrain/registry-production.key
export ONEBRAIN_QUALIFICATION_GPG_HOME=/secure/onebrain/qualification-approver-gnupg
```

Then run each physical host's admitted profile:

```bash
bash scripts/runner/onebrain-registry-runner.sh preflight --mode prequalification
bash scripts/runner/onebrain-registry-runner.sh closure --mode prequalification
bash scripts/runner/onebrain-registry-runner.sh resource --mode prequalification --profile ssd
```

The controller runs:

```bash
bash scripts/runner/onebrain-registry-runner.sh kernel --mode prequalification
bash scripts/runner/onebrain-registry-runner.sh aggregate --mode prequalification
```

The exact CCID diff uses previous and candidate `input.jsonl`, OBR, and
manifest bytes. The failure harness covers truncated indexes, disk shortage,
and publication/activation process kills. The runtime filter exercises pinned
live readers, generation swap, rollback, and reopen behavior.

## 8. Reading the Task 21 summary

`component-summary.json` is a non-production derived view. It lists the BLAKE3
of every raw report and requires:

- one `registry_closure_digest`;
- one candidate release aggregate root;
- four passing resource profiles;
- passing failure qualification;
- passing exact CCID stability; and
- non-empty live-reader/process-kill output.

Its invariant fields are:

```json
{
  "base_candidate_bound": false,
  "registry_production_qualified": false
}
```

If either field differs, reject the artifact. `component_qualified=true` means
only that the Task 21 component set passed under one closure. It does not
qualify Base v1 and cannot be carried forward into Task 28.

## 9. Failure handling

- Missing staged file: restore the exact immutable artifact; do not substitute
  a fixture.
- Closure mismatch between hosts: stop, compare `registry-closure.json` rows,
  and replace the drifting host/image/input. Do not average results.
- Wrong storage class: fix the physical mount or runner assignment. Editing a
  label cannot replace sysfs evidence.
- Signature or signer mismatch: stop and obtain the owner-approved bytes. Do
  not create a temporary production signer.
- Disk shortage: preserve the raw failure report, provision a new clean work
  volume, and rerun the whole profile.
- Interrupted run: preserve the attempt artifact and start a new workflow run;
  never overwrite prior evidence.
- Production aggregate refusal during Task 21: expected unless Task 28's exact
  signed-candidate prerequisites are present. Do not weaken the gate.

## 10. Evidence handoff

Record workflow run URL, attempt, runner receipt digest, closure digest,
release root, generation, target triple, probe digest, toolchain digest,
runner-image digest, and every raw-report digest. Retain the signed request and
signature for release runs. Store these outside Git with immutable retention.

Task 21 commits only source and contract files. Task 28 consumes fresh external
evidence; it does not consume a self-referential aggregate committed beside
the source it claims to bind.
