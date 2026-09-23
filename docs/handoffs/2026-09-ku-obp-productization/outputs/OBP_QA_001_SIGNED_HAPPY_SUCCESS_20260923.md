# OBP-QA-001 — signed P5 happy case completed

2026-09-23 14:34 UTC. Original working tree, branch
`codex/obp-qa-001-nat-canary`. The exact immutable runtime candidate is
`5b8dc3bc9e1922d99ef4839f4ce0211a526bf177`, tree
`73be69bbf2f25d161a37329a8f10dbacebc2b46e`, installed generation
`0efb45d54ea1e2aba0de865689faf0e3f9e1d8c0f60a493543ac42e1a74c4c3a`
on host-a/b/c. The operational bundle manifest BLAKE3 is
`db59ca996fe6d9ff2ee336239120fca302836cc93f665b93ceb169066eb28659`.
The first new package without inherited operational templates is retained as
attempt `p5-upload-20`; only verified `p5-upload-20b` was installed. Earlier
staging, generations, relay state, cursor archives and failed sessions remain.

The relay restart test and full relay suite passed (22 unit, 8 descriptor
lifecycle, 11 data-plane); the Linux release node example tests (11) and
vNext validator passed before the immutable build. A one-time preserved-floor
migration seeded two stable reservation cursors per host at sequence 7 from
the highest retained legacy cursor. The old cursor bytes and their hashes are
kept in each host's `obp-qa-global-reservation-seed-20260923-19` archive.

The newly installed relays advanced their durable descriptor floor from
sequence 3 to 4, retaining original sequences 1–3 and state snapshots under
`renewal-20260923-21`. Four live restricted-SSH probes (a→b, c→b, a→c,
b→c) passed with the signed contiguous sequence-1→4 history. The new signed
Base request, Registry binding, bundle, three public host exports, accepted
D-036 placement wrappers, four probes, V2 inventory and approval policy all
verified before execution. Inventory SHA-256:
`2130f1c4cf73bc92eaaa8335540fc5ddd60291a481719109aca2d468bff280ec`;
P5 request SHA-256:
`980c1aef7c35f0c78014fb3250b1993c12ad3960bbf71fb15141213f5634cb6b`.
The accepted provider status remains
`owner-telephone-verified-provider-document-pending` with its original dates.

Signed session `bb6d48ec20217113313a3a78d6c9245c66207e9e3e366c742bef61936457af86`
first stopped at the expected prior-session cursor binding before network
creation. Ten session-bound cursors per host were archived byte-for-byte, with
manifest hashes, in `obp-qa-cursor-rollover-20260923-23`; the stable global
reservation cursors were untouched. The same signed bootstrap responses were
reverified against the new host configuration digests, then the canonical
controller resumed. Signed prepare, reachability, two-relay diagnostics,
reservations, relay-only A→B→C→A ring, three delivered/three received markers,
cleanup and finalization completed. Each ring hop reports `accepted=true` and
`RelayTcp443`. Production-preflight SHA-256:
`f2385add0384cc9e38dfb1bb3f1a53fd2f003331895d67a56b0947e6adf0c3b3`.
The raw controller evidence remains private at external
`/home/shpy2/.onebrain/obp-qa-5b8dc3b-20260923/p5-happy-resume-24/`;
the initial bootstrap attempt is separately retained in `p5-happy-case-23/`.

Both temporary sequence-4 relay listeners were stopped. Direct checks on
a/b/c found no session config, network namespace or network-session state,
no active P5 agent/signer unit, and no TCP 443 listener. Normal relay units
remain fenced. This is a successful signed **happy case only**: it does not
execute the 13 P5 production faults/ten exit oracles, prove the separate
two-consumer/two-network product scenarios, or qualify any platform. Host-b
NTP synchronization remains unconfirmed. Networking defaults and D-024 tuning
were not changed; OBP-MIG-001 remains unstarted.
