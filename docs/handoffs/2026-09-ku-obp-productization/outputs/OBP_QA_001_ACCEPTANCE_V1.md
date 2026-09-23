# OBP-QA-001 acceptance scenarios v1

Historical strict-evidence procedure. Owner decision D-039 now uses
[functional acceptance v2](OBP_QA_001_FUNCTIONAL_ACCEPTANCE_V2.md) for task 18
review. The scenario gaps and claim limitations below remain factual; this
procedure is retained for any later consumer-network qualification run. D-041
explicitly skips this qualification gate for task 18 closure because the
available VPS cannot establish consumer NAT; no scenario pass is inferred.

2026-09-23. This is a versioned test procedure for task 18, not a new wire,
host-provisioning, authority or qualification profile. D-029 remains accepted.
Use the existing node-owned service for API, CLI, local Web and Desktop.

Execution update (2026-09-23): the separate exact-candidate Linux P5 V2
three-host gate [qualified](OBP_QA_001_P5_PRODUCTION_20260923.md) on `c453f3e`.
The ten two-consumer-network product scenarios below remain unrun. The original
local-preflight and future-run instructions retain their historical scope.
The owner's later two-VPS selection received a
[read-only remote admission audit](OBP_QA_001_VPS_PRODUCT_ADMISSION_20260923.md);
no product scenario was executed or accepted from that VPS topology.

## Evidence lanes

Continuation D-036 (2026-09-23): the owner explicitly retains the previously
supplied and accepted placement of the existing three VPS on three separate
physical hosts. Reuse those placement inputs for the current P5 continuation;
no replacement provider confirmation is required. This supersedes the blanket
historical-attestation reuse prohibition below for these accepted placement
inputs only. Preserve their original dates/provenance and explicit
owner-telephone/provider-document-pending status. Current candidate bindings,
relay probes and all execution evidence must still satisfy canonical V2 gates.

| Lane | Required evidence | Meaning |
|---|---|---|
| Local preflight | Temporary stores, deterministic custody, loopback carriers, adapter fixtures and offline validators | Regression evidence only. One workstation cannot prove independent hosts/networks, real NAT, relay operators or platform qualification. |
| Two-node product acceptance | Two consumer application hosts on independent networks; two independently configured vNext relays; exact candidate/build/platform and before/during/after observations for the scenarios below | Task 18 product evidence. Container/process labels and distinct NodeIDs do not prove independence. No inbound port forwarding, UPnP or consumer firewall changes. |
| Linux P5 production reference | Exact signed candidate/request/inventory, three physical Linux hosts, independent roots/principals, relay-only A→B→C→A ring, typed provider/topology evidence, signed receipts, all 13 faults and ten exit oracles | Existing V2 controller/verifier owns qualification. Two consumer nodes are not a substitute for this three-host gate. Historical evidence cannot qualify this Desktop candidate. |

P5 rules remain in [V2 qualification](../../../specs/vnext/P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V2.md),
[V2 preflight](../../../specs/vnext/P5_OUTBOUND_FIRST_PREFLIGHT_PROFILE_V2.md),
[preserved V1](../../../specs/vnext/P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V1.md),
and the [operator guide](../../../operations/ONEBRAIN_BASE_V1_P5_MULTI_HOST_GUIDE.md).
The accepted P5 provider status is
`owner-telephone-verified-provider-document-pending`, bound to that signed
evidence set. Do not copy its attestation or waiver to a new run. A free-text
telephone note is insufficient; provider-document-verified requires the three
typed documents. Missing evidence stays missing.

Native Windows/macOS lifecycle, Linux reference, browser networking and mobile
are separate lanes. Local Web is an API client, not a browser peer carrier.
No claim of mailbox wake, guaranteed availability, global discovery, strict
Base qualification or default rollout follows from any scenario here.

## Common setup and collection

For a future authorized isolated-host run, bind the candidate commit/tree,
artifact/toolchain/features, OS/architecture, run interval and scenario version.
Keep independently verified host/network/relay configuration evidence in the
restricted collection. Record the expected consumer and relay identities and
independent durable roots; labels, counts and screenshots alone are insufficient.
Preserve original receipts and failed attempts without editing their outcomes.

Use trusted in-process host provisioning already accepted by the API/Desktop
contracts: custody, policy, execution/control grants, typed discovery inputs,
and scoped management capabilities. Stock Desktop supplies none and remains
unavailable; this procedure adds no provisioning endpoint or second runtime.
Obtain explicit test-host execution authority before enabling its network lane.
The initial task invocation authorized no live-host activation or remote faults.
The later owner instruction authorizes scoped SSH upload/execution on the three
existing P5 hosts; see [execution record](OBP_QA_001_P5_UPLOAD.md). Canonical signed
P5 admission remains required before production faults.

Capture private observations before, during and after each fault: process/dataset
fences, durable generation, source and reservation state, authenticated expected
peer, exact path kind, outbox state/counters, checkpoint and bounded resource
observations. A route receipt or successful command is not delivery. Application
acceptance requires the domain owner's durable acknowledgement and checkpoint.
Traffic must originate from consumers; never fix a failed case by opening their
inbound ports. No raw-payload OBP endpoint is introduced: use an already authorized
domain operation with synthetic public test data and its original intent.

Compare CLI/API/local Web/Desktop at the same quiescent service snapshot,
including generation and limitations; do not compare unrelated process datasets.
WS is an aggregate hint. After gaps, clear stale detail and read REST; never
replay a command. Keep tokens, source data, endpoints, interfaces, raw receipts
and original pending-command recovery private. Public reports use bounded typed
observations and digest references. P5 raw evidence uses its existing encrypted
archive and request-bound recipient, not a new artifact format.

## Scenarios and local coverage

All independent-network outcomes are **not run** in this collection. Local
suite IDs below refer to the fixed command inventory in
`scripts/runner/obp_product_preflight.py`; they are supporting coverage, not
automatic scenario passes.

| ID | Procedure and pass oracle | Existing local coverage and gap |
|---|---|---|
| OBP-QA-V1-01 | Start with both opt-ins false. Read all surfaces; missing bindings are unavailable. Supply test host dependencies explicitly; preserve one owner, identity and durable generation. No reads/WS/startup may enable a lane. | `node`, `api-off`, `cli`, `desktop`, `desktop-off`, `web`. Actual cross-surface native WebView run pending. |
| OBP-QA-V1-02 | On each consumer, admit a DNS bootstrap, then repeat with direct public IP plus expected identity. Verify signed descriptors, expiry/replay, endpoint possession and exact expected peer; no bare address authority. Reserve both independent relays before traffic. | `node` discovery tests exercise DNS/IP through fixture transport. `relay`, `core`, `routes` exercise crypto and loopback carriers. Real DNS/provider/consumer NAT evidence pending. |
| OBP-QA-V1-03 | After admitting other approved sources, remove only the initial bootstrap in the isolated test boundary. Refresh from learned sources; preserve identity, replay floors and valid records. A fresh empty node with all sources unavailable reports bounded absence, never global offline. | `dns_and_ip_bootstrap_keep_learned_sources_after_seed_loss` uses a fixture outage; no remote source has been disabled. |
| OBP-QA-V1-04 | A initiates a domain intent to B, then B initiates a separate intent to A. Both originate outbound connections, authenticate the expected NodeID and accept exact canonical content once. Record each direction and durable acknowledgement separately. | Local routing tests cover sender→receiver traffic. Full bidirectional product journey across two independent consumer networks remains missing. |
| OBP-QA-V1-05 | Preserve both peers' selected/alternate reservations before fault. Disable selected relay, observe typed RelayUnavailable, and recover via a different pre-reserved relay. Require fresh outer binding and inner session with the same expected peer, exact acknowledged checkpoint and bounded next sequence; no duplicate semantic acceptance. | `bidirectional_durable_delivery_moves_to_pre_reserved_alternate_tls_relay` runs two loopback relay services and closes the selected sender carrier. It does not stop an independent relay process or prove full P5 fault evidence. |
| OBP-QA-V1-06 | Restart each consumer with pending intent and with an unknown command response. Keep exact bytes/identity/counters/checkpoint and original key/context. Fresh status + metadata reconciliation only; no blind replay, no restored live session. Dataset replacement must reject old context. | `node`, `api`, `cli`, `desktop`, `web`: durable restart, journal crash boundaries, recovery fixtures. Physical process/OS/power and multi-host convergence pending. |
| OBP-QA-V1-07 | Try wrong relay key, wrong target NodeID, stale/expired/replayed descriptor, substituted transcript and malformed/oversized source data. Reject before peer authority/content acceptance. Malicious delay/drop/duplicate/reorder may reduce availability, never create trust or completion. | `core`, `relay`, `routes`, `node` negative tests. The matrix's attack catalog alone is not an attack execution. Independent-host malicious-relay observations pending. |
| OBP-QA-V1-08 | Inject synthetic private canaries; inspect actual public discovery bytes and aggregate notifications. No private payload, LAN/topology, credentials, IDs, attempts or raw errors may leak through the relevant public/aggregate channel. Authenticated REST IDs remain private, as contracted. No authority/reward/wallet amplification. | Core/API/Web redaction and rejection fixtures; public-artifact matrix contains synthetic strings only. Full run artifact/packet privacy scan pending. |
| OBP-QA-V1-09 | In an isolated authorized boundary block UDP while permitting outbound TLS/TCP 443; require relay-tcp-443 and exact peer auth. Remove all relays: pending/path_limited within frozen budgets, no delivery claim. Preserve local usefulness. | Loopback TLS listeners use ephemeral ports, not a real restrictive TCP-443 network. NAT/capability matrix models path selection; real firewall/provider evidence pending. |
| OBP-QA-V1-10 | Compare quiescent CLI/Web/Desktop/API state after bootstrap loss, failover, kill and restart. Disconnect WS; restore only reads. Kill must persist; explicit re-enable advances generation and does not grant execution or enable unrelated lanes. Windows suspend/interface event must fence execution; explicit Restart reconstructs dependencies. | Separate CLI actual HTTP, Desktop shared-service integration and Web fixtures pass independently; no simultaneous native WebView parity, real sleep or interface switch is claimed. |

## Reproduction and integrity

From repository root:

```powershell
python scripts/runner/obp_product_preflight.py --list
python scripts/runner/obp_product_preflight.py --run target/obp-qa-001/NEW-ATTEMPT
python scripts/runner/obp_product_preflight.py --verify target/obp-qa-001/NEW-ATTEMPT/report.json
```

The collector refuses to reuse an output directory, records exact commands,
exit codes and SHA-256 log hashes, preserves partial/failing results and runs
only the fixed local suites. Cargo is offline/locked. It never dispatches the
P5 production controller, SSH, host activation or privileged fault commands.
All qualification flags are always false. Verification checks collection
integrity and derived local pass status; it is not a signature verifier,
test-coverage proof, privacy certification or production evidence authority.
Logs are local diagnostics and must be reviewed before any public upload.

The base Git commit/tree identify the starting runtime, not an immutable signed
release candidate. Working-tree status records local QA additions. No production
carry-forward from this dirty collection is permitted. A later qualifying run
must bind its own exact approved candidate and use existing evidence verifiers.

## Outstanding evidence required to finish task 18

Provide an authorized isolated two-consumer/two-relay environment, verifiable
host/network independence and an explicit supported host assembly. Execute all
ten scenarios and retain their scoped observations. The exact-candidate Linux
P5 V2 three-host request/inventory/provider/receipt/fault/resource/privacy/
cleanup gate is now verified in the linked qualification report above. Its
three-host topology does not replace the missing two-consumer-network product
evidence. No remote credentials or private keys should be placed in this handoff.

Until these gates are met, keep task 18 incomplete and task 19 unstarted.
Networking remains default-off. No owner decision already accepted is reopened.
