# OBP-QA-001 — Linux P5 production-reference qualification

2026-09-23 15:26 UTC. The three-host P5 V2 lane **qualified** on branch
`codex/obp-qa-001-nat-canary`. This is the Linux production reference for the
exact candidate below. It does not complete the separate two-consumer-network
product acceptance or qualify Windows, macOS, browser or mobile. Task 18 remains
`Blocked`; OBP-MIG-001 has not started. Networking remains default-off; no
tuning or rollout was performed.

## Exact candidate and admission

- Immutable candidate `c453f3e636092384606a70b06fce3335e66b00c9`, tree
  `846d413cb5bc89130b57cf29f04098187001abd2`. The Python controller and
  independent verifier on this candidate bind public child-receipt digests to
  restricted raw receipts and reject embedded endpoint-bearing receipts.
- The same offline, locked, source-free Linux x86_64 bundle was staged,
  canonically verified and installed on host-a/b/c. Generation SHA-256:
  `b9af4b7b38da62fd7dab226c6317334661ba89c209ca2440f94f4a02d17308cc`;
  bundle manifest BLAKE3:
  `0f29fb3a7ffc51b3165bfb8d52a5144a3b94f6c2cb4bc45906b94adccea84c02`.
  Prior staging, generations, durable relay state and identities were retained.
- Fresh signed Base release request, Registry binding, three public host exports,
  accepted D-036 placement inputs, two sequence-7 relay descriptor chains,
  four live cross-host probes (a→b, c→b, a→c, b→c), inventory, approval policy
  and P5 request verified before the signed session. Request digest:
  `f19183f4369ae857c3535fc604b6ffbca72e807dc630421965fbe10f1cad8262`.
  Original placement provenance remains
  `owner-telephone-verified-provider-document-pending`.

## Signed execution and independent verification

Session `ee1933fc07361513db39f1861f55f2ccc3e4deb2486d280a6100e07f67aa8d82`
ran the relay-only A→B→C→A ring. The selected relay was stopped; recovery used
the other pre-reserved relay, a fresh authenticated session and the exact
checkpoint. All 13 faults ran with before/during/after observations and 13
verified recovery markers: partition, drop, reorder, duplicate, restart,
address change, seed outage, signer outage, disk pressure, slow peer, Base
OBARV002 archive restore, rollback and explicit re-enable. The ten exit oracles
passed: request authority, three-host inventory, authenticated relay-only ring,
complete faults, selected-relay failure, authenticated alternate relay, exact
checkpoint resume, resource bounds, privacy and aggregate signature.

The exact-candidate independent `verify-p5` result is
`multi_host_qualified=true`, with 332 signed child receipts and 387 raw objects
verified. The signed [public aggregate](OBP_QA_001_P5_PRODUCTION_20260923/p5-multi-host-aggregate.json)
has BLAKE3 `ec989da4c23dffef4c7a12fb23587ab51ecdf8f4c9f1b77851031f913913bd7d`;
the restricted raw manifest has BLAKE3
`0cbe2b75542263d86f175581245bed95d7c6b053f8dde8f4e23d44a04cf354b1`.
The [independent verifier receipt](OBP_QA_001_P5_PRODUCTION_20260923/p5-verification.json)
records the qualification and limitations. A separate check confirmed all 13
fault phases/markers, digest-only aggregate and deterministic raw archive bytes
after decrypting the request-bound HPKE envelope.

The [public privacy scan](OBP_QA_001_P5_PRODUCTION_20260923/p5-public-privacy-scan.json)
found zero restricted keys, known host addresses, endpoint, secret, path or
interface findings in the aggregate and verifier receipt. The controller now
enforces a public aggregate privacy gate before signing. Public artifact
SHA-256: aggregate
`08b93b1a9d9989b9d6bc9d0ac10c45d7a8bdda67b00ed565b58b62e2f5c9806a`;
verifier receipt
`15a4dd6f7dd507460f1ebfe2522319f5d33b1f7fb8993463386d6a29283b9a8e`.
The raw receipts, exact host/network observations and HPKE envelope stay in the
restricted controller collection at
`/home/shpy2/.onebrain/obp-qa-c453f3e-20260923/p5-fault-resume-49/p5/`.

## Retained attempts and cleanup

An earlier `5b8dc3b` sequence-5 run failed at archive restore using stale
August recovery fixtures. The original fixtures and hashes were archived on
all hosts under `obp-qa-recovery-fixture-20260923-37`; the failed signed
session was cleaned up and finalized. A following sequence-6 run on `5b8dc3b`
completed all faults, but its public aggregate embedded raw receipts containing
endpoints. That artifact is retained as restricted evidence, not the public
qualified report. The privacy correction required the new exact candidate
above. Previous runs were not relabeled.

The final session first encountered an expected prior-session cursor binding.
Nine old session cursors per host were archived byte-for-byte under
`obp-qa-cursor-rollover-20260923-49`; global reservation cursors were not reset.
Verified bootstrap configuration hashes were resumed. After the full fault
matrix, signed cleanup and finalization completed. Temporary sequence-7 relay
listeners were stopped. Direct checks on a/b/c found no P5 session config,
network namespace or network-session state, active P5 agent/signer or TCP 443
listener; normal relay units remain fenced. All old staging, generations,
cursor/fixture archives and failed runs remain.

## Remaining boundary

The verifier reports `provider-document-pending`,
`non-linux-platform-lanes-pending` and `mobile-carrier-mailbox-pending`. Task 18
still needs its own two independent consumer networks, two independent relays
and the ten product scenarios, including native surface/lifecycle evidence.
P5's three-host gate is separate from and does not substitute for that work.
