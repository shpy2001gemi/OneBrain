# OBP-QA-001 — two-VPS product scenario admission

2026-09-23 15:44 UTC. The owner directed use of two of the three existing VPS
as application nodes and stated that the VPS networks are independent. For a
bounded staging attempt, host-a and host-c were selected as application-node
roles; host-b and host-c would supply the two existing relay roles. Host-c
would therefore carry both an application and a relay process with separate
durable roots. This selection does not change the accepted
[two-consumer-network acceptance v1](OBP_QA_001_ACCEPTANCE_V1.md): VPS network
independence does not demonstrate ordinary consumer NAT or no consumer inbound
port forwarding. The exact-candidate three-host [Linux P5 qualification](OBP_QA_001_P5_PRODUCTION_20260923.md)
remains valid and separate.

## Read-only remote admission

Using the pinned SSH transport already approved for these hosts, a fixed
read-only script ran on a/c and b. Script SHA-256:
`9ee66542aa33db3e4bd11213e9db73498a66bb207761e2a609b4772af85f7f3e`.
The private command/result collections are retained under ignored
`target/obp-qa-001/p5-step-20260923T154302509008Z-host-a`,
`p5-step-20260923T154301556158Z-host-c`, and
`p5-step-20260923T154351932659Z-host-b`. Their respective result SHA-256 values
are `4869de7bb0fd9a29aa1d94824e743bd759761fb5f9960ca58569871b4f0266a6`,
`3c8388a9c0d5135520b5838cdd39571145b91009447c412b961fb2d1b3c5a220`,
and `c58218eb39d45fa54be66b830adae20d46873ea3cb395ec259c61c05a5fa487a`.
All three commands exited zero. Host-a/c report KVM, host-b VMware; all are
Linux x86_64. The checked relay and P5 agent units remain inactive. No service,
listener, firewall, NAT, forwarding, installed generation or durable state was
changed by this inspection.

No Desktop or dedicated product QA host executable is available on the checked
PATH. More decisively, the installed source-free P5 bundle's closed binary
inventory contains the P5 agent/admin/signers, relay and preflights, but no
`OneBrainNode` product host, shared API/CLI/Web service or Desktop executable.
The stock Desktop provides no OBP custody, policy, source-intake or execution
grant ports. A custom Desktop network host on Linux is fenced at startup:
`src/onebrain-desktop/src/platform.rs` returns
`desktop_native_lifecycle_adapter_unavailable` and
`src/onebrain-desktop/src/lib.rs` fences the supervisor. This is the accepted
Desktop projection's deliberate platform gate, not a missing CLI switch.

## Scenario outcome and next gate

| Acceptance IDs | Outcome | Current blocker |
|---|---|---|
| OBP-QA-V1-01..10 | Not executed; no product acceptance claim | No supported, provisioned node-owned application assembly on the selected VPS; Linux native Desktop lifecycle unavailable; VPS topology does not prove consumer NAT. |

Launching only the P5 agent or relay probes would repeat the P5/reference lane,
not exercise the shared product service, bidirectional domain intent, native
WebView lifecycle or the ten scenario oracles. No relay was activated merely to
produce a nonqualifying probe. Task 18 remains `Blocked`, with
`product_acceptance_qualified=false`. To run the accepted scenarios, provide two
ordinary consumer application hosts on independent networks and a supported
trusted in-process host assembly with custody, policy, signed sources, control
grants and the node-owned service. Native Windows lifecycle/surface observations
need a Windows host; Linux custom Desktop startup cannot substitute. If the
owner instead wants a VPS-only acceptance claim, the acceptance procedure and
claim boundary need an explicit revision before it can be called complete.
Networking remains default-off, no tuning occurred, and OBP-MIG-001 is unstarted.
