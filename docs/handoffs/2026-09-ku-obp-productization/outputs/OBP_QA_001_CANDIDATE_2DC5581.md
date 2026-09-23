# OBP-QA-001 — approved candidate rebuild and staging

2026-09-23. D-035 accepts the reviewed relay correction and local candidate commit.
Owner subsequently requested autonomous completion and reported no known location
for newer provider/topology evidence. That directs independent investigation; it
does not assert physical placement or permit fabricating an attestation.

## Immutable candidate and verified preparation

- Commit `2dc5581e74387953b21051c930e52a8044ce0503`, tree
  `f08092d800615181da4ccc50db997606edb6f03b`, on retained branch
  `codex/obp-qa-001-nat-canary`. Includes the approved correction and retained QA
  package. No push/merge or branch removal.
- Clean isolated local source: `target/obp-qa-001/p5-upload-source-2dc5581`.
  Clean external WSL checkout/controller root:
  `/home/shpy2/.onebrain/obp-qa-2dc5581-20260923/candidate`.
- All 12 native binaries rebuilt offline/locked in the pinned Rust 1.98.0 Linux
  image, networking disabled. Compiled candidate/tree/toolchain/profile/vector
  bindings are explicit. Release build and default CLI build exited 0. Existing
  compiler warnings remain; no tuning or dependency changes.
- Fresh signed Base request verified by the canonical exact-candidate tool:
  `release-requests/5c3bd10410e0533f19e6e1dc5ba726b024bd6cf160bfd4434847a6cf3a3055e8/release-request.json`
  under the external controller root. The existing qualification-approver key was
  reused. The approved same-key P5 policy interval remains September 23–30.
- New default CLI `--version --verbose` semantic/artifact tuples independently
  rehashed. Canonical tools remeasured existing Registry files and signed/verified
  `prebuilt-registry-binding.json` and `prebuilt-registry-verified.json` under the
  new external root. No Registry extraction, model work or key rotation.
- Operational bundle reuses only measured historical unit/verifier templates;
  native executable bytes come from this candidate. Canonical manifest binding
  and operational `verify.sh --root` both passed on the extracted Linux bundle.

Archive: `target/obp-qa-001/p5-upload-03/onebrain-p5-2dc5581e7438-linux-x86_64.tar.gz`.
SHA-256 `7184eb936c1fc5c623c58547a566eac87e65dc46b27c449da0c1bcfb61e90f1c`.
Manifest SHA-256/generation:
`b8c1c0da86aff3ce315c03172de445377c44021a21a29923d03726f19b8e0e4a`.
Manifest BLAKE3:
`c8b0456fa5843c2a8edc267639a6328f19aff37b0c688db3e304d6e5011c9446`.

The actual release relay binary passed a new isolated fixture reproduction:
first export, deliberately failed successor publication, successful recovery,
byte-identical equal-sequence republication, and `NotActivated` before fresh
probes. The intentional failed attempt is retained. This is local binary
regression evidence, not remote possession or qualification.

## Three-host staging and fresh observations

All three pinned SSH hosts accepted the identical bundle in new directory
`~/onebrain-p5-staging/obp-qa-2dc5581-20260923-03`. Archive checksums, operational
bundle verification and role-specific read-only readiness passed on each host.
Only the newly extracted bundle root mode was set to 0755. Existing generations,
current selectors, unit files, identities and relay database files were unchanged.

Both native preflight binaries returned exit 0 on each host in new `preflight-01`
directories under that new staging root: **six single-host loopback preflights**.
All qualification flags remain false. They do not prove a cross-host ring, NAT,
real relay failover, platform parity, thirteen faults or ten exit oracles.

Fresh guest evidence was independently collected on all three hosts using the
existing bounded host-evidence collector and separate create-new topology staging
directories. Machine-id hashes and boot IDs differ; these are guest observations,
not proof of separate physical hypervisors. No placement receipt was supplied to
the collector, so `placement` stays null. Restricted output is retained under
`target/obp-qa-001/p5-topology-03`.

Searched relevant local `.onebrain`/release archives, WSL controller roots and
P5-owned remote evidence locations. Remote b/c placement receipts were found and
read: both are still owner-telephone records dated **2026-08-13**, using
`owner-phone-attested:` identifiers. Their embedded `receipt_verified=true` is
not fresh provider-signature verification. They were preserved as observations,
not copied into a new inventory as current evidence. Host-a's privileged receipt
path could not be read with `sudo -n` (password required); its pinned SSH and
unprivileged guest measurements succeeded. No credential was requested or saved.

## Newly measured admission blocker: host-b clock

Fresh paired controller/remote measurements show host-b ahead by
**25,387.14–25,389.21 seconds** (about 7 h 3 min). Host-a and host-c were within
the measured SSH round-trip interval of the controller. Host-b reports
`Timezone=Etc/UTC`, `NTP=no`, `NTPSynchronized=no`, with no timesync1 service.
This is an epoch-clock discrepancy, not display timezone conversion.

P5 controller bootstrap uses a 30-second skew budget and a 300-second remote
future limit. Signed host frames also check issued/expiry against host time.
Do not shift signed request timestamps to disguise the discrepancy or increase
the canonical validity windows. Host clocks were not changed: stepping a live
host backward would affect its existing time-sensitive services and cannot be
treated as isolated staging. Remediation must account for those services and
remeasure synchronization before descriptor renewal and signed admission.

## Current gate and retained evidence

The former no-candidate/no-renewal-binary blocker is resolved for the approved
candidate. The installed relays are still the historical generation with expired
descriptors. Do not replace their database or rerun identity/state initialization.
The corrected immutable bundle is available for the documented state-preserving
maintenance procedure once admission prerequisites can be completed.

Still required: current admissible provider/topology evidence, host clock
alignment, candidate-bound installed/forced-command exports, fresh same-key
descriptor chains and actual endpoint possession from both other hosts, then the
complete canonical V2 inventory/request/signature/verify/bootstrap/session/fault/
cleanup path. No P5 inventory or request was fabricated; no signed session,
production fault, relay restart or clock mutation occurred in this continuation.

Restricted artifacts:

- `target/obp-qa-001/p5-upload-build-03/`: build command/environment/log/result,
  compiled public bindings and measured CLI status.
- `target/obp-qa-001/p5-remote-03/`: all host upload/readiness/preflight results,
  canonical bundle verification, Registry verification and release-binary renewal
  reproduction (including its intentional failure).
- `target/obp-qa-001/p5-topology-03/`: fresh guest records, clock measurements,
  discovered historical placement receipts and denied-read result.
- External WSL controller root holds the pristine checkout, new signed Base
  request, approved policy copy, Registry bindings and isolated fixture key.
  No private key enters Git, bundle or public evidence.

The two-consumer independent-network/two-relay product lane remains separately
unexecuted. Task 18 remains **Blocked**, OBP-MIG-001 unstarted, networking
default-off, D-024 tuning Deferred. All older source checkouts, bundles, staging
directories, keys and evidence runs are retained. Do not ask again for D-035,
D-029, SSH authority, policy renewal or artifact/key locations already recorded.
