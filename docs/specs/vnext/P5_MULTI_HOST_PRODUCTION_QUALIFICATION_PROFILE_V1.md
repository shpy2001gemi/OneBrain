# OneBrain vNext — P5 Multi-Host Production Qualification Profile v1

> **Work package:** Base v1 Task 22 / `WS-22`
>
> **Status:** Frozen — signer identities approved by owner on 2026-08-10
>
> **Machine contract:** [`p5-multi-host-production-qualification-v1.json`](../../../src/test-vectors/vnext/p5-multi-host-production-qualification-v1.json)
>
> **Implementation owner:** Task 23 (`vnext-production-canary-harness`)

## 1. Qualification boundary

Production P5 qualification MUST use three distinct physical
`x86_64-unknown-linux-gnu` hosts, three independent durable roots and three
restart-stable principals.

All hosts MUST run the byte-identical signed release agent, pinned toolchain
and pinned runner image named by the verified signed Base release request.

The A→B→C→A data ring MUST use authenticated real QUIC; SSH stdio is a bounded,
control-only channel and MUST NOT carry application data.

Single-host and three-process runs remain preflight and MUST emit
`multi_host_qualified=false`; cross-platform portability is a separate Base v1
gate and MUST NOT substitute for this Linux production topology.

## 2. Candidate and inventory binding

The orchestrator MUST verify the canonical Task 28 release request v2 and its
owner-approved qualification signature. Candidate commit/tree come from that
request. Semantic digest, Linux artifact-tuple digest, target and toolchain are
read from the exact compiled agent. The controller independently checks every
file record in the exact native-bundle manifest before executing bundle code.
The `p5-orchestrator` key signs both the agent bytes and the native-bundle
manifest digest under the separate `p5-release-agent-signature` usage and
domain; hashing an opaque signature file is not authentication.

P5 also measures the exact existing Registry candidate files
`concepts.obr`, `concepts.obr.ccids.idx`, `concepts.obr.labels.idx`,
`concepts.obr.manifest.json` and `concepts.obr.verification.json`. It hashes the
actual bytes, checks size/digest agreement with manifest and verification, and
derives a domain-separated Registry candidate root. The P5 subgate does not
require every future Registry resource profile. This deliberately does **not**
set `registry_production_qualified=true` and does **not** satisfy
`BASE-GATE-V1`.

Every host MUST use the identical candidate/environment binding, and neither a
producer nor an operator-supplied command may override a derived identity.

The orchestrator-signed inventory MUST pin each physical host ID, runner
identity, observed and expected SSH host key, machine fingerprint, complete
host-evidence and placement-evidence SHA-256, receipt role/key, durable-root
locator and expected principal, plus the exact limitations and Registry
candidate measurement.

Duplicate physical hosts, runner identities, durable roots, principals, SSH
host keys or receipt keys MUST fail before the first remote command.

Topology admission may be based on provider-signed placement, a bare-metal
lease/inventory receipt, or an owner-signed out-of-band verification of the
provider's placement statement. In the owner-attested case the signed
inventory MUST bind the exact complete host-evidence and placement-evidence
SHA-256 for all three hosts; an unsigned telephone note by itself is not a
machine receipt. This accepts the owner's signed evidence judgment without
claiming that it came from a provider API.

## 3. Receipt trust policy

Receipts MUST use Ed25519 with the public keys, role bindings, fingerprint
derivation context and trust-policy digest frozen in the machine contract.

The allowed roles are exactly `p5-host:host-a`, `p5-host:host-b`,
`p5-host:host-c` and `p5-orchestrator`; a valid signature from an unlisted key,
a signature used under the wrong role, or cross-host key reuse MUST fail.

Each signed child receipt MUST bind role, physical host, signed release-request
digest, qualification-session ID, candidate commit/tree, semantic digest,
Linux artifact tuple, release-agent binary/signature, Registry root, profile
and trust-policy digests, runner/SSH identity, monotonic command sequence,
fault, before/after roots, bounded resource observation, result and
limitations.

Every child receipt and the aggregate MUST carry the frozen limitations:
the receipt is evidence rather than authority, aggregate qualification is
orchestrator-owned, Registry candidate bytes are bound without full Registry
profile qualification, Registry resource profiles remain pending, Registry
production qualification is not claimed, and `BASE-GATE-V1` is not claimed.

The aggregate root MUST cover canonical ordered child-receipt bytes only; the
aggregate report and detached orchestrator signature MUST remain outside the
root they attest.

The orchestrator MUST reject a missing, wrong or mixed release request,
qualification session, candidate, artifact, agent, Registry, profile or
trust-policy binding instead of trusting an input qualification boolean.

## 4. Control and fault authority

Every SSH command MUST use bounded canonical JSON, a monotonic sequence and a
signed agent receipt; replayed, stale, oversized or unsigned control input
MUST fail closed.

The application fault proxy MUST be default-off and may change delivery
conditions only; it MUST NOT validate data, create knowledge, grant authority,
or fabricate truth, completion, reward or wallet effects.

A production run MUST execute the complete ordered matrix: partition, drop,
reorder, duplicate, restart, authenticated address change, seed outage, signer
outage, disk pressure, slow peer, Base `OBARV002` archive restore, rollback and
explicit generation-advancing re-enable.

Every fault MUST record signed before/after canonical, journal, outbox and
operational roots from every affected host.

## 5. Archive, rollback and network-off behavior

Production recovery MUST use the Base `OBARV002` archive service, restore into
a new dataset generation, verify complete parity/health and atomically switch
only after verification succeeds.

The legacy `onebrain/p5-offline-backup/1` profile remains byte-for-byte
unchanged and preflight-only; it MUST NOT qualify production recovery and MUST
NOT be renamed or silently superseded.

Rollback MUST fence every distributed lane durably, preserve canonical state
and require explicit generation-advancing re-enable before real QUIC resumes.

Local private KQL MUST remain usable while all network and distributed lanes
are disabled.

## 6. Resource and exit oracles

Each physical host MUST stay within the frozen RSS, durable-growth, task,
session, control-message and duration bounds in the machine contract.

The run MUST prove durable reunion/idempotency, per-host principal
preservation, canonical root preservation or exact advancement, reconciled
journal/outbox/operational roots and zero active sessions after quiescence.

The run MUST prove zero truth, authority, network-completion, reward and wallet
amplification under every fault and recovery path.

Missing faults, before/after roots, resource observations, signed receipts or
any failed exit oracle MUST force `multi_host_qualified=false`.

## 7. Qualification state

This document freezes the contract and owner-approved public signer identities;
it is not measured multi-host evidence and does not set
`multi_host_qualified=true`.

Even after a measured P5 run sets the P5-only `multi_host_qualified` field,
`registry_production_qualified` and `base_gate_v1_qualified` remain false until
their separate future gates are complete.
