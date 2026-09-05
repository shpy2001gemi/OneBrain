# OBP capability and status map

This is the compact product-planning view. It intentionally separates code and
test evidence from user-facing orchestration and release qualification.

Legend: `Implemented` = code and focused evidence exist; `Core only` = reusable
implementation exists but the normal product lifecycle/surfaces do not yet own
it; `Designed` = contract/architecture exists but its named gate is open;
`Legacy` = not an acceptable vNext implementation.

## Bootstrap, discovery and relay

| Capability | Status | Product meaning |
|---|---|---|
| Build and run `onebrain-relay` | Implemented | Permissionless self-hosted UDP/QUIC or TLS/TCP-443 relay binary |
| Durable relay identity/state | Implemented | Ed25519 identity, replay floors and bounded durable state |
| Signed relay descriptor and endpoint proof | Implemented | NodeID/key binding, expiry, sequence and live proof-of-possession |
| Relay reservations | Implemented | Target/relay binding, bounded capacity and replay defense |
| Rendezvous record storage | Implemented | Signed byte-preserving discovery records with bounds |
| Signed bootstrap manifests | Implemented | Replaceable source hints; mirrors grant no trust |
| Bootstrap using DNS or public IP | Implemented core | DNS/IP locates a source; identity is verified separately |
| Manual relay/peer invitation | Implemented core | Canonical URL/file/QR-compatible envelopes |
| Authenticated PEX discovery | Implemented core | Only a live authenticated opposite peer can be a PEX source |
| Discovery from rendezvous/DHT/cache | Implemented core | Bounded merge without global membership/completeness |
| Automatic discovery/reservation refresh in normal product | Core only | Must be wired into node-owned lifecycle |
| Legacy `onebrain-seed` | Legacy | TCP/JSON prototype; do not claim as production vNext seed |

## Connectivity and NAT

| Capability | Status | Product meaning |
|---|---|---|
| Ordinary node works without public IP/port/NAT setup | Implemented reference | Proven by outbound relay reference lane; product orchestration remains opt-in |
| Bidirectional carrier through outbound relay reservations | Implemented | Both nodes connect outbound; relay does not become peer identity |
| Direct/LAN route | Implemented core | Optional optimization, capability-dependent |
| Coordinated UDP hole punch | Implemented core | Optional; not the required baseline |
| QUIC/UDP relay | Implemented | Real encrypted/authenticated outer carrier |
| TLS/TCP-443 relay fallback | Implemented | Works where UDP is unavailable |
| Multiple pre-reserved relays | Implemented | Availability diversity under local bounds |
| Alternate-relay failover | Implemented reference | Fresh carrier/session and exact checkpoint resume |
| Network change and route journal | Implemented | Durable bounded operational state; route does not change identity |
| Sleeping/offline target durable intent | Implemented | Pending work survives; no false delivery claim |
| Ciphertext mailbox/wake lane | Designed | Mobile carrier/mailbox gate remains open |
| WebTransport/secure WebSocket carriers | Designed | Browser/WASM adapter and qualification pending |

## Identity, security and privacy

| Capability | Status | Product meaning |
|---|---|---|
| Self-certifying full NodeID | Implemented | Derived from the advertised Ed25519 public key |
| `Hello → Welcome → Finish` session | Implemented | Transcript, nonces, identities and capabilities signed |
| Transport/channel binding | Implemented | Carrier replacement forces fresh authentication |
| Expected-peer enforcement | Implemented | Address, DNS, relay or caller cannot assert NodeID |
| Downgrade/capability-stripping defense | Implemented | Strongest common negotiated profile is verified |
| Replay/sequence/expiry checks | Implemented | Durable and bounded where required |
| DNS/private-address/rebinding defense | Implemented core | Public discovery admits only revalidated global-unicast targets |
| Resource/Sybil work bounds | Implemented | Candidate, bytes, signatures, probes, sessions and retries are capped |
| Public discovery privacy | Implemented | No LAN address, SSID, carrier/router identity or attempt history |
| Relay content authority | Prohibited | Relay contributes availability only |
| Relay metadata visibility | Explicit limitation | No protection claim against relay/global traffic observation |

## OBP reconciliation

| Capability | Status | Product meaning |
|---|---|---|
| Selector/namespace/disclosure/budget binding | Implemented | Every exchange is explicitly scoped |
| Private Vault inventory firewall | Implemented | Ordinary inventory cannot include private storage classes |
| Hybrid five-lane inventory forest | Implemented | Deterministic full-CID roots and restart recovery |
| Deterministic radix diff | Implemented | Stable divergent ranges without arrival-order meaning |
| Manifest before payload | Implemented | Undeclared bytes cannot reach the sink |
| Validate then accept | Implemented | CID/schema/signature/policy validation precedes durable acceptance |
| Deferred/quarantine/conflict isolation | Implemented | One bad or missing-dependency branch does not poison others |
| Persisted journal and cross-session resume | Implemented | Crash/reconnect-safe idempotent continuation |
| Multi-bridge deduplication | Implemented | One, two or five paths have one semantic result |
| Memory/file/delayed/QUIC parity | Implemented | Carrier choice grants no semantic authority |
| Partition autonomy and reunion | Implemented | Local work continues; later fair delivery converges within scope |
| Global completion/truth/benefit/reward | Prohibited | Receipts and carrier observations cannot create these claims |

## Product and platform boundary

| Capability | Status | Product meaning |
|---|---|---|
| Node-owned authenticated QUIC/reconciliation runtime | Implemented, default-off | Available behind feature/config gates |
| Bounded one-hop distributed KQL | Implemented, default-off | Private Need stays local; public delta may produce quarantined proposal |
| Public UseEvidence prepare/confirm | Implemented, default-off | Explicit consent; no truth/Benefit/reward inference |
| vNext REST/private WS/CLI/Desktop-Web KQL/PoMV surfaces | Implemented | Existing product integration lane |
| Reachability Manager in normal `OneBrainNode` lifecycle | Core only | Main productization gap |
| Relay/bootstrap management API and UI | Planned | Tasks in this package |
| Automatic seed-independent desktop experience | Planned | Requires product orchestration and acceptance |
| Linux three-host production-reference | Evidence recorded | Owner-waiver scope and explicit limitations |
| Windows/macOS outbound-first product qualification | Designed/pending | Must not inherit Linux qualification |
| Mobile/browser networking | Designed/pending | Separate build and physical lifecycle gates |
| Strict Base/default production rollout | Pending decision/gates | Not implied by this workstream |

## Evidence snapshot used for this handoff

On the clean synchronized repository snapshot before this package:

- `python scripts/ci/validate_vnext_contracts.py` — pass;
- `ku-net` with `persist,quic` — 378 tests passed;
- `onebrain-relay` — 31 tests passed;
- `onebrain-node --features vnext-outbound-first --lib` — 184 tests passed;
- focused reachability/outbound-first/live-relay integration suites — 27 tests passed;
- Anti-Gravity reunion integration — 4 tests passed.

This evidence supports bounded implementation/conformance claims. It does not
replace strict release, physical platform or default-rollout gates.
