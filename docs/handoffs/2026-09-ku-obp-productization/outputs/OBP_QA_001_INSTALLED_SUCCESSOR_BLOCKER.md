# OBP-QA-001 — installed candidate and successor admission blocker

2026-09-23, continuation through 13:11 UTC. Work remains in the original tree on
`codex/obp-qa-001-nat-canary`, candidate `2dc5581e74387953b21051c930e52a8044ce0503`.
This supersedes the earlier no-installation checkpoint. No new commit/push/merge.
Task 18 remains Blocked; task 19 remains unstarted.

## Completed maintenance and exact host bindings

All three hosts now select installed immutable generation
`b8c1c0da86aff3ce315c03172de445377c44021a21a29923d03726f19b8e0e4a`.
The canonical operational verifier passed before and after installation; compiled
agent commit/tree match `2dc5581` / `f08092d800615181da4ccc50db997606edb6f03b`.
Six P5 unit templates and three restricted forced-command paths on each host
bind that generation. P5 agent/signer services and sockets remain inactive;
no signed bootstrap, prepare-session, fault, or qualification session ran.

Existing identity/receipt keys and NodeIDs match the archived public exports.
All three host identities, receipt keys and SSH host keys remain distinct.
Agent access to either signer key is denied. No identity generation, database
reset, source extraction or model work occurred. Previous generations, selectors'
prior targets, configuration backups, staging and all old attempts are retained.
Host-c's measured previous selector is `e7a8f50...`, not the older archived
`84258411...`; the new public export records the actual previous generation.

Host-a initially rejected `sudo -n`; the owner supplied an administrative
credential and explicitly directed its use for this test server. Sudo then
worked. No credential is stored in the scripts, retained evidence or handoff.
Do not reopen SSH/sudo approval or ask for the credential again.

## Clock maintenance: measured correction, NTP still pending

The first attempt enabled existing timesyncd, but received no NTP packets and
did not synchronize. Its failed result is retained. RTC matched the controller,
so a second attempt stopped relay-b, copied the database, set system time from
RTC and restarted the existing service. Database/descriptor hashes were unchanged.
However, after NTP restart, timesyncd restored its recorded future timestamp.
The journal explicitly reports a backward jump restored from recorded time.

At 13:06 UTC, maintenance stopped relay-b and timesyncd, preserved both database
and `/var/lib/systemd/timesync/clock`, corrected system time from RTC, updated
that clock-file timestamp and restarted NTP and relay. Subsequent checks through
13:11 UTC stay near controller time. No signed-frame time or skew budget changed.

Final controller/remote intervals, rounded outwards:

| Host | Remote minus controller | NTP state |
|---|---|---|
| a | -0.38 to +4.21 seconds | enabled, synchronized |
| b | -0.65 to +1.21 seconds | enabled, **not synchronized** |
| c | -0.40 to +1.21 seconds | enabled, synchronized |

Host-b UDP/123 replies were not observed; the cause is unproven. Accurate bounded
measurements do not establish NTP synchronization or long-term clock stability.
Remeasure before future admission; preserve this limitation.

## D-036 placement reuse

The accepted topology and three typed provider entries were copied byte-for-byte
to external controller directory
`/home/shpy2/.onebrain/obp-qa-2dc5581-20260923/accepted-placement-06/`.
The historical P5 signature verifies, its inventory binding verifies, and each
copied object equals the corresponding signed inventory object. Original August
13 dates and `owner-telephone-verified-provider-document-pending` are preserved.
`provenance.json` records original paths and SHA-256/BLAKE3 commitments.

An earlier local assertion tried to recompute `source_attestation_blake3` from
re-serialized embedded JSON and failed before copying anything. The empty
`accepted-placement-05/` attempt remains. That nested digest is not asserted to
be a canonical embedded-object digest; original raw source bytes were not
reconstructed or changed. The successful check authenticates the exact archived
wrapper through the historical signed inventory. Current complete V2 binding
still requires fresh probes and a new request. No new placement claim was made.

## Actual successor/probe failure

Both relays exported a same-key sequence-2 successor with retained predecessor
digest, using the corrected installed binary and the existing database. Each
database was copied while its service was stopped. Identity, endpoints and
capacity configuration were preserved. Separate candidate-only listeners ran
on the real relay hosts. Same-sequence republication produced byte-identical
successors after stopping those listeners.

| Relay | Successor BLAKE3 | Remote probe sources | Result |
|---|---|---|---|
| b | `f5d7f9de1578c8dcddc64ab9b426832a599498b3166e5f43b693fca812093279` | a, c | both `SequenceRollback` |
| c | `ea65bd75189435f81b2c19cc3fe0eab737eb8f5f53bc145d6fcba8c4a30db6c0` | a, b | both `SequenceRollback` |

The first probe attempt used the operator SSH identity and was rejected by the
restricted accounts. The retry used the existing distinct controller SSH key,
whose derived public key matches the installed authorized lines. All four
authenticated forced-command probes then reached the binary and exited 1 with
`OBP_RELAY_PREFLIGHT: OBP_REACHABILITY_ADMISSION: SequenceRollback`.
These are failed real-host attempts, not endpoint-possession evidence.

Source diagnosis:

- `src/onebrain-relay/src/bin/relay_preflight_probe.rs`: closed `ProbeRequestV1`
  carries only one descriptor; every invocation creates an empty
  `InMemoryReachabilityReplayStore`, then calls `register_prepared_descriptor`.
- `src/ku-net/src/vnext_reachability_crypto.rs`: an empty sequence store admits
  only sequence 1 without a predecessor; successors require the existing exact
  predecessor floor. The observed rejection is the intended fail-closed check.
- `src/onebrain-node/examples/p5_multi_host_agent_v2.rs`: P5 discovery and its
  isolated relay diagnostic also start with empty stores; commands currently
  carry current `relay_descriptors` only. Fixing only the probe would leave the
  P5 agent's fresh-discovery path unresolved (source finding, not executed P5).
- `OUTBOUND_FIRST_REACHABILITY_PROFILE_V1.md` section 4 requires short-lived
  signed objects, durable floors and exact contiguous predecessors. Replaying
  the now-expired sequence-1 descriptor as current admission is not a repair.

There is no admitted history/floor input in the current closed probe request.
No verifier bypass, timestamp shift, sequence reset or fabricated probe is used.

## Retained remote state — read before resuming

**Relay-b and relay-c are stopped and deliberately fenced.** Candidate-only
listeners are stopped as well. Their normal units now point at the new immutable
binary; `/etc/onebrain/relay-p5.json` matches the sequence-2 maintenance config.
Direct normal-start checks returned `NotActivated` on both. No activation was
forged. Their existing enabled/disabled settings were not changed.

The original `candidate-descriptor.cbor` remains historical sequence 1.
The authoritative durable candidate is sequence 2. Its exact bytes, predecessor,
pre-renewal database and old/new configs are retained under
`/var/lib/onebrain/relay-p5/renewal-20260923-09/` on each relay. Do not restore the
old database to erase the floor, restart an old binary, overwrite old descriptor
files, or issue another successor merely to hide this failed probe. The temporary
sequence-2 descriptor may expire before continuation; later renewal must advance
from the actual durable floor and preserve the complete chain.

Host installation backups are under
`/var/lib/onebrain/maintenance/obp-qa-install-20260923-05/`.
Host-b clock maintenance backups use the same parent with `obp-qa-clock-*` names.

## Concrete correction proposal for review

Add a versioned, bounded authenticated descriptor-history admission path shared
by probe and P5 node discovery. Preserve existing V1 inputs and wire descriptor
bytes. A successor input must carry its exact signed contiguous predecessor
chain, starting at an authenticated sequence-1 anchor or an already durable
verified floor. Validate canonical bytes, signatures, NodeID/key, sequence links,
same configuration and budgets before committing any floor. Expired predecessors
may supply history only: no dialing, reservation or live authority from them.
The terminal descriptor still requires current-time freshness, DNS/global-address
checks and fresh possession for every endpoint. Persist/recover floors without
clearing them or creating another planner/outbox/service.

Specify the closed input schemas and aggregate byte/count limits in the owner
specification before implementation; bind history into the P5 inventory/request
and the corresponding receipts. Cover cold start at sequence 2+, expired history,
gaps/forks/wrong keys, malformed/oversized chains, equal-sequence recovery and
partial-failure atomicity. Then review a new immutable candidate, rebuild all
bindings, install and repeat fresh four-way probes before V2 session/faults.
This proposal is not implemented or treated as accepted authority by this report.
D-035 approved candidate `2dc5581`; it is not silently extended to a new commit.

## Integrity and limits

Private local collections under `target/obp-qa-001/`:

- `p5-step-*/result.json`: bounded commands/results, including failures.
- `p5-maintenance-package-05/`, `p5-maintenance-upload-05/`: inspected scripts,
  checksums and all three uploads; no private key or password.
- `p5-installed-exports-05/` and `p5-installed-exports-11/`: installed public
  bindings, SSH clock measurements and identity-preservation checks.
- `p5-probes-09/` and `p5-probes-10/`: failed SSH and authenticated probe attempts.
- `p5-maintenance-integrity-11.json`: 47 file commitments independently rehashed;
  three distinct unchanged host/identity/receipt bindings verified; four actual
  successor probe failures checked. SHA-256:
  `75c12047fd8e7861bf3d95e6d16a02544f523f17b22ea51645ab8ed6148e7f84`.

No complete current V2 inventory/request exists; no signed session/fault/cleanup
execution or qualification is claimed. Product acceptance still separately lacks
two independent consumer networks and all ten scenarios. Shared node ownership,
networking default-off and Deferred tuning remain unchanged. Mobile, legacy
removal and OBP-MIG-001 are untouched.
