# OBP-QA-001 — admission recheck, 2026-09-23

**Superseding owner decision D-036:** the owner confirms that the three VPS on
three separate physical machines were already documented and accepted. Reuse
that retained placement evidence without reacquisition. The external-placement
blocker and fresh-placement requirements below describe the earlier checkpoint
and no longer apply. Preserve original evidence dates and provider-document-pending
status; canonical V2 binding and fresh operational evidence remain required.
Host-b clock, installed-generation exports, renewed descriptors/probes and actual
signed session/fault/cleanup execution are still outstanding.

## Continuation at 08:39 UTC

Reused the inspected read-only collector, without changing its commands:
`python target/obp-qa-001/audit_admission_readonly_02.py`.
New create-new collection: `target/obp-qa-001/p5-admission-audit-20260923T083908Z/`.
Summary SHA-256:
`fdbd54eae2da2ea459dbf251234b23609d695e7d85b8584e5b3b6ae3610ae5b8`.
All three host-file hashes were verified against that summary.

| Host | Remote minus controller clock interval | Command result |
|---|---|---|
| a | +0.09 to +4.90 seconds | Exit 1: partial observation; restricted search remains incomplete |
| b | +25,386.78 to +25,389.90 seconds | Exit 0; NTP off, timesyncd inactive, relay running |
| c | +0.27 to +2.90 seconds | Exit 0; NTP synchronized, relay running |

Intervals are conservatively rounded outward. The bounded readable search did
not find a current placement receipt; b/c still list August 13 receipts. This
does not establish absence outside the searched paths. No host mutation occurred.

### Concrete V2 input completion order

The canonical controller's `prepare_inventory` requires the following inputs.
Its ability to assemble JSON is not evidence that the physical facts are true,
nor a substitute for complete verification and the signed session gate.

| Input | Required next evidence/action |
|---|---|
| `provider-evidence-root/host-a.json`, `host-b.json`, `host-c.json` | Exactly one current typed entry per host, grounded in actual placement evidence. Existing guest machine/boot IDs cannot prove distinct physical hosts. Retain provider-document-pending unless all three typed provider documents verify. |
| `topology-attestation` | Current owner-signed attestation grounded in that evidence; possession of the signing key and approval to execute SSH do not establish placement facts. Do not redate the August telephone records. |
| Host clocks | Resolve host-b's host-wide time discrepancy around active services, preserve state, and remeasure actual synchronization before short-lived descriptor/probe creation. Do not compensate in signed timestamps or skew limits. |
| `host-public-root/host-{a,b,c}.json` | Fresh installed immutable-generation, forced-command and public-role exports for `2dc5581`; staged bundle checks are insufficient. Preserve existing identities and durable roots. |
| `relay-evidence-root` | At least two current descriptor-key-bound public probe sets; contiguous same-key successor descriptors and actual endpoint possession from both other hosts for each endpoint. Create these only when the remaining preparation is ready because descriptor validity is bounded. |
| Bundle, Registry, controller public identities | Reuse the measured candidate artifacts and existing distinct role identities; reverify exact bytes and request validity at admission time. The 08:35 local audit is supporting evidence, not a fresh complete inventory. |
| Inventory and request | Assemble the canonical inventory, prepare the exact Base-bound P5 request, sign with the approved existing policy, and verify the complete authority set before bootstrap and signed prepare-session. |

Current external dependency is factual placement evidence, not another D-029,
SSH, candidate or policy-renewal approval. No evidence source discovered so far
can supply a truthful fresh topology attestation. Repeating local suites cannot
resolve that dependency. The separate product lane still requires two independent
consumer networks, two relays and supported shared node-owned host assembly.

Task 18 remains Blocked. No signed session/fault, host clock/service change,
commit/push/merge, default networking change, tuning or task 19 work occurred.
The earlier observations below and all staging/attempts are retained.

Candidate remains `2dc5581e74387953b21051c930e52a8044ce0503`, tree
`f08092d800615181da4ccc50db997606edb6f03b`, in the original working tree on
`codex/obp-qa-001-nat-canary`. Existing dirty handoff changes were preserved.
This continuation collected read-only SSH diagnostics; it did not run a signed
P5 session or any fault. D-029, D-035, SSH execution and policy renewal remain
accepted and were not reopened.

## Fresh observations

The bounded collection at approximately 08:30 UTC retained the following:

| Host | Remote clock minus controller clock, conservative interval | Time service | Existing relay |
|---|---|---|---|
| a | +0.75 to +6.59 seconds | NTP synchronized; timesyncd active | Failed, unchanged |
| b | +25,386.92 to +25,390.59 seconds | NTP off; timesyncd inactive | Running |
| c | +0.04 to +3.59 seconds | NTP synchronized; timesyncd active | Running |

Intervals include the entire SSH command round trip and one second of timestamp
quantization. They are diagnostics, not signed clock or qualification receipts.
Host-a's overall command exited 1 at the restricted evidence-file search; its
preceding clock/service output is retained as partial evidence, not a full pass.
Hosts b/c exited 0. No new placement evidence appeared in the bounded readable
`/var/lib/onebrain` and `/etc/onebrain` search: b/c still expose the previously
recorded August 13 placement receipts. Host-a's privileged evidence remains unread.
This is not an exhaustive provider-account search or proof that no other evidence
exists.

Additional host-b diagnosis found `systemd-timesyncd.service` installed but
**disabled**, with no active NTP override in the displayed configuration.
VMware Tools reports time synchronization **Disabled**. The earlier missing
`dbus-org.freedesktop.timesync1` alias did not prove the package was absent.
Chrony/ntp/ntpsec service units were not found. P5 agent and both signer services
are inactive. The relay, cron and other normal OS services remain active, so a
backward clock step is a host-wide maintenance action, not isolated staging.

## Retained attempts and integrity

Restricted local artifacts under `target/obp-qa-001/`:

- `p5-admission-audit-20260923T082857Z/`: first attempt; all three client calls
  timed out at 45 seconds. No successful host observation is claimed. This first
  collector did not retain partial timeout stdout; the limitation is explicit.
- `p5-clock-retry-20260923T083015Z.json`: minimal host-b date retry succeeded.
- `p5-admission-audit-20260923T083036Z/`: bounded command results and summary;
  each host JSON SHA-256 was independently checked against the summary.
- `p5-clock-diagnosis-20260923T083110Z.json`: host-b service/configuration and
  VMware time-sync diagnosis, exit 0.
- `audit_admission_readonly.py` and `audit_admission_readonly_02.py`: retained
  diagnostic collectors. The second bounds individual remote inspection commands
  and preserves timeout output. Neither is a production qualification tool.

Summary SHA-256:
`cf5ad4200146e655cb0db46855948d86e9d50003b8e0a5c00faa3bbb8f61cf47`.
Host-b diagnosis SHA-256:
`7402806bbe8650c850269d947a8c66bb3a9eb7378b3601fbef9be02095394c35`.

The first collector used additional SSH client options; the successful retry
used the recorded working client configuration with bounded inspection commands.
The cause of the first timeout was not established. Do not label it a remote
outage. No remote configuration, service restart, clock adjustment, installation,
descriptor export, probe activation or fault command was issued.

## Concrete continuation order

1. Obtain current admissible placement/topology evidence for the three hosts.
   Preserve `owner-telephone-verified-provider-document-pending` where applicable;
   do not derive provider verification from guest IDs, IP addresses or historical
   telephone receipts. Bind the new evidence through the canonical V2 path.
2. Plan host-b maintenance around its active relay and other clock-sensitive
   services. Snapshot current state and preserve keys, databases, descriptors and
   old evidence. Coordinate host clock correction and the existing state-preserving
   relay renewal procedure. The installed timesyncd service is a remediation
   option; installation of another time daemon is not established as necessary.
   Verify actual synchronization and remeasure all three clocks before issuing
   fresh short-lived descriptors or signed admission frames. Never compensate
   with future-dated frames or wider validity/skew limits.
3. Complete exact-candidate installed generation/forced-command/public exports,
   same-key successor chains and real endpoint possession from both other hosts.
   Use fresh probe evidence for each endpoint; retain every failed attempt.
4. Form and verify the complete V2 inventory/request/signature before canonical
   bootstrap, signed prepare-session, session/faults and two-phase cleanup.

These prerequisites remain unresolved. Independently, the product lane still
needs two consumer networks, two relays, supported shared node-owned host assembly
and all ten acceptance scenarios. P5's three physical hosts are a separate gate.
Task 18 remains **Blocked**, all qualification claims remain false, task 19 remains
unstarted, networking remains default-off and D-024 tuning remains Deferred.
No runtime source, mobile implementation, Git commit/push/merge, retained staging
or old run was changed.
