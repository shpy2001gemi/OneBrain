# OneBrain vNext — P5 Operations Preflight Profile v1

> **Work packages:** `P5-02` through `P5-06`
>
> **Status:** Frozen single-host operational preflight
>
> **Machine contract:** [`p5-operations-preflight-v1.json`](../../../src/test-vectors/vnext/p5-operations-preflight-v1.json)
>
> **Executable:** [`onebrain-node::vnext_p5_operations`](../../../src/onebrain-node/src/vnext_p5_operations.rs)

## 1. Qualification boundary

The combined report MUST keep `consumes_pre_release_72h_evidence=false`,
`multi_host_canary_qualified=false`, and
`production_canary_qualified=false`.

Passing this profile MUST NOT replace the pinned 72-hour artifact, multi-host
canary, or explicit production rollout approval.

## 2. P5-02 fault drills

An unavailable session signer MUST fail before creating a durable runtime file.

Storage above the hard watermark MUST reject the payload with the finite
`REJECTED_STORAGE` reason and leave zero durable feed branches.

A healthy authenticated peer MUST durably progress within 5,000 milliseconds
while a slow authenticated peer keeps a separate session open.

All fault drills MUST quiesce to zero active sessions without wallet, OBT,
authority, truth, Benefit, or network-completion amplification.

## 3. P5-03 offline backup and restore

Backup MUST run after clean runtime shutdown and copy only regular files through
a sorted relative-path manifest with exact length and BLAKE3 for every file.

The archive aggregate root MUST be domain-separated, and every copied payload
file must be flushed before the manifest is accepted.

Restore MUST verify the complete archive before creating its target, reject
unsafe relative paths or symlinks, and copy only into a new empty location.

Restore MUST preserve the principal, raw feed branch, journal bytes, pending
outbox, quarantine/provenance evidence, and exact operational root.

A one-byte-corrupt archive MUST fail before the restore target is created.

## 4. P5-04 rollback and P5-05 rollout

Atomic rollback MUST fence all four runtime lanes while preserving every
durable file and semantic oracle covered by the backup drill.

Restart with the same enabled configuration MUST NOT revive a durably rolled
back lane.

Each lane MUST require explicit generation-advancing re-enable before real QUIC
reconnect or distributed side effects resume.

All twelve public vNext feature flags MUST remain false in default
configuration, including after a prior opt-in store is reopened by defaults.

Local private KQL canonicalization MUST round-trip while every network runtime
lane is effectively disabled.

## 5. P5-06 operator dashboard

The dashboard MUST expose startup/degraded state, signer and registry health,
lane generations, route/session counts, journal/outbox/quarantine state,
storage pressure, and finite incident/action codes.

The dashboard MUST NOT serialize NodeID, selector, private Need, free-form peer
labels, wallet/OBT mutation, or network-completion claims.
