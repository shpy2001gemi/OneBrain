# OneBrain Outbound-First NAT Traversal and Federated Relay Design

> **Date:** 2026-08-14
> **Cross-platform amendment:** 2026-08-18
> **Status:** Owner-approved design; implementation plan at [`../plans/2026-08-14-onebrain-outbound-first-nat-traversal-linux-p5.md`](../plans/2026-08-14-onebrain-outbound-first-nat-traversal-linux-p5.md)
> **Scope:** Platform-independent OBP reachability, direct-path upgrade, permissionless community-relay fallback, platform adapters, and Linux/P5 reference qualification
> **Does not change:** NodeID authority, authenticated-session authority, content authority, feed authority, or local-first operation

## 1. Problem and objective

OneBrain nodes must be able to participate when the operator cannot configure
the surrounding network. Common cases include mobile networks, CGNAT, home
routers, restrictive enterprise networks, shared-IP VPS products, and virtual
machines behind an upstream firewall.

The product must therefore not require a user to:

- own a public IP;
- select or forward a public port;
- configure NAT, UPnP, a router, hypervisor, or provider firewall;
- know whether the current network permits inbound UDP; or
- understand STUN, TURN, ICE, QUIC routing, or relay topology.

This baseline applies equally to Linux, Windows, macOS, Android, iOS, and a
browser/WASM client. Ordinary nodes can attempt the baseline route entirely
through outbound connections when at least one admitted relay is reachable;
success still depends on the target's current reservation and lifecycle.
Direct reachability is an optimization, not an installation or participation
requirement.

Running a public relay is a separate operator role. A relay operator must make
at least one relay transport reachable, but that requirement is never pushed
onto ordinary node users. A laptop, phone, browser tab, home computer, or
shared-IP VPS remains a complete OneBrain node without opening an inbound
port.

The required route order is:

```text
direct/LAN
  -> public or server-reflexive direct
  -> coordinated UDP hole punch
  -> federated QUIC/UDP relay
  -> federated TLS/TCP 443 relay fallback
```

The selected carrier never becomes identity or authority. Every successful
route must terminate in the existing authenticated OBP session with the exact
expected NodeID.

The route order is capability-aware rather than OS-name-aware. A node skips a
route kind that its current platform grant or network cannot execute. For
example, a browser may begin at an outbound web-compatible relay carrier, and
an iOS node may omit LAN discovery or hole punching while backgrounded. That
is a truthful limitation, not a degraded identity or a different protocol.

## 2. Existing authority and compatibility

This design is subordinate to the frozen
`docs/specs/vnext/AUTHENTICATED_SESSION_PROFILE_V1.md` session contract and
`docs/specs/vnext/THREAT_MODEL_V1.md` carrier boundary.

The existing `Hello -> Welcome -> Finish` handshake remains the only session
authentication path. Its transport binding, nonces, full NodeIDs, public keys,
profile negotiation, and replay guard remain unchanged. A seed, rendezvous
server, discovery record, NAT observation, relay, IP address, port, DNS name,
or operator claim grants no session, content, actor, or feed authority.

The legacy `seed_client.rs` and best-effort `upnp.rs` are not accepted as the
production implementation of this design. Their existing centralized seed
assumptions, incomplete receive dispatch, and non-executable UPnP fallback must
not be promoted silently into the vNext path.

### 2.1 Portable-core boundary

OBP is split into five strict layers:

```text
product/UI
  -> platform lifecycle and secure-storage adapter
  -> portable OBP reachability manager and route state machine
  -> carrier capability interface
  -> native/web transport adapter
  -> direct peer or permissionless community relay
```

The portable core owns canonical objects, NodeID checks, signatures, replay
floors, reservations, route planning, authenticated-session binding, durable
intents/checkpoints, resource budgets, privacy projections, and typed failure
semantics. It must not call systemd, UFW, nftables, Network.framework,
URLSession, Android services, WinSock policy, browser APIs, or a platform key
store directly.

Platform adapters own sockets, DNS execution, lifecycle grants, background
time, secure-key handles, monotonic-clock observations, network-change events,
and durable file primitives. An adapter reports measured capabilities and
grants; it does not rewrite a protocol object or claim authority for an OS
observation.

The carrier interface is framed, bounded, cancellation-safe, and independent
of whether its implementation uses QUIC datagrams, a TLS byte stream,
WebTransport, or secure WebSocket framing. Every carrier yields a fresh
transport-binding digest to the unchanged authenticated-session handshake.

### 2.2 Platform capability profiles

Profiles describe available execution mechanisms, not distinct OBP wire
protocols:

| Platform lane | Baseline outbound carrier | Optional optimization | Lifecycle rule |
|---|---|---|---|
| Linux server/desktop | QUIC/UDP; TLS/TCP 443 | inbound direct, LAN, hole punch | daemon or foreground is an adapter choice |
| Windows desktop | QUIC/UDP; TLS/TCP 443 | inbound direct, LAN, hole punch | no systemd/UFW assumption |
| macOS desktop | QUIC/UDP; TLS/TCP 443 | inbound direct, LAN, hole punch | app/service lifecycle is OS-owned |
| Android | outbound QUIC/UDP or TLS/TCP 443 | foreground LAN/direct/hole punch | finite user-visible/background grants; durable resume |
| iOS/iPadOS | outbound QUIC/UDP when granted; TLS/TCP 443 | foreground LAN/direct when permitted | no 24/7 listener or completion promise while suspended |
| Browser/WASM | WebTransport when available; `wss://` framed carrier fallback | browser-permitted peer transport | no raw UDP, filesystem, or permanent-tab assumption |

Raw carrier differences are private adapter details. Public route receipts use
the closed path classes and limitations defined by the shared profile. The
portable core must not infer that a platform is globally offline merely
because one adapter capability is absent.

## 3. Governance: permissionless but not implicitly trusted

Anyone may operate and announce a compatible relay. No OneBrain owner,
maintainer, committee, registry, vote, token, or provider approval is required
for a relay to exist or be discovered.

Permissionless operation is distinct from local selection:

- a relay may announce only its own self-certifying identity;
- a node admits an announcement only after schema, signature, sequence,
  expiry, resource-bound, and challenge-response checks;
- a node chooses routes under local policy and measured observations; and
- a relay operator may enforce its own capacity, abuse, and payment policy,
  but that policy grants no OneBrain authority.

Bundled bootstrap entries are initial discovery hints, not an approved relay
list. Community and self-hosted relays remain usable without inclusion in a
OneBrain release.

A user may self-host or share a relay without OneBrain approval. Other nodes
learn it from its signed descriptor through manual import, authenticated peer
exchange, rendezvous/DHT sources, or independently configured mirrors. A node
selects it only after the same proof-of-possession, freshness, address,
resource, and liveness checks used for every other relay.

## 4. Discovery and the bootstrap limit

Each relay publishes a short-lived signed descriptor through multiple
independent paths:

1. OneBrain DHT/rendezvous discovery;
2. authenticated peer exchange;
3. multiple bootstrap discovery nodes shipped as non-authoritative hints;
4. a signed bootstrap manifest retrievable from untrusted mirrors; and
5. explicit manual invitation by URL, file, or QR code.

Nodes maintain reservations with at least two, and normally three, relays
subject to resource policy and operator diversity. They refresh discovery
before the current set expires. Relay liveness is measured by bounded probes;
an advertisement alone is never evidence of reachability.

A completely new node with no initial address cannot discover a disconnected
network if every bootstrap, mirror, manual invitation, and nearby peer path is
unavailable. This is an explicit bootstrap limitation, not a condition that
may be hidden behind a global-availability claim.

No bootstrap path is a mandatory OneBrain-operated service. A release may
carry several replaceable community hints, but those hints are neither roots
of trust nor required infrastructure. A user can instead begin from a signed
relay/peer invitation, local file, QR code, or an authenticated existing peer.
If the current relay disappears, the node uses already reserved alternates and
refreshes from the remaining independent sources; it does not contact a fixed
OneBrain control plane.

## 5. Signed protocol objects

All objects use canonical closed schemas, domain-separated signature
preimages, monotonically increasing sequence values where applicable, and
short explicit validity intervals.

### 5.0 `BootstrapManifestV1`

```text
format
discovery_source_id
discovery_endpoints[]
protocol_versions[]
sequence
issued_at
expires_at
source_signature
```

A node may configure and merge manifests from multiple independent discovery
source keys. A bundled manifest signature proves only which source supplied
the initial hints and that the bytes were not modified. It does not approve,
authorize, rank, or grant trust to any relay discovered through those hints.
Anyone may distribute another manifest, and local policy decides which source
keys to admit. Individual relay identities are still validated independently.

### 5.1 `RelayDescriptorV1`

```text
format
relay_node_id
relay_public_key
endpoints[]
supported_transports[]
protocol_versions[]
capacity_policy_digest
sequence
issued_at
expires_at
relay_signature
```

The NodeID must derive from the advertised public key. A proof-of-possession
challenge must succeed before the descriptor becomes dialable.

Every dialable native or web endpoint is named explicitly by the signed
descriptor and is proven separately. WebTransport and secure WebSocket
facades must present a certificate/SPKI and possession proof bound to the same
relay identity as its native QUIC/TLS endpoints; an HTTP origin, CDN, reverse
proxy, or WebPKI certificate alone never becomes relay identity. The route
receipt projects WebTransport into `relay-udp` and secure WebSocket into
`relay-tcp-443`, while private diagnostics retain the exact adapter transport.

### 5.2 `RelayReservationV1`

```text
format
relay_node_id
target_node_id
reservation_id
transport_scope
issued_at
expires_at
target_signature
relay_signature
```

Both signatures are required. A relay cannot claim that an unrelated target
is reachable through it, and a target cannot claim relay capacity that was not
reserved.

### 5.3 `ReachabilityAdvertisementV1`

```text
format
target_node_id
relay_reservations[]
optional_public_candidates[]
capability_ceiling
sequence
issued_at
expires_at
target_signature
```

Only the target signs target reachability. Route providers and discovery
systems carry the object without modifying it.

### 5.4 `RoutePlanV1`

```text
expected_peer
direct_candidates[]
relay_candidates[]
deadline
attempt_budget
resource_budget
privacy_policy_digest
```

`RoutePlanV1` is local execution state. It is not a public authority object.

### 5.5 `RouteReceiptV1`

```text
expected_peer
authenticated_peer
selected_path_kind
selected_carrier_identity
attempts[]
transport_binding_digest
session_id
started_at
authenticated_at
terminal_outcome
limitations[]
local_signature
```

The only successful outcome requires `expected_peer == authenticated_peer` and
a valid authenticated-session transcript. Relay success without peer
authentication is not connection success.

## 6. Runtime components

All components below are portable state machines or traits unless explicitly
identified as a platform adapter. The Linux P5 harness is one consumer of
these components, not their owner.

### 6.1 Reachability Manager

Collects local candidates, server-reflexive observations, explicitly provided
provider mappings, relay reservations, network-change events, and transport
availability. It invalidates candidates when interfaces or mapped addresses
change.

### 6.2 Relay Discovery

Consumes the discovery paths in Section 4, verifies signed descriptors, caps
candidate work, deduplicates identities, and supplies only bounded verified
candidates to the planner.

### 6.3 Reservation Manager

Creates outbound reservations, maintains bounded keepalives, rotates before
expiry, and preserves operator/network diversity. It never exposes raw private
keys or promotes a reservation to session authority.

Operator and network diversity are availability heuristics derived from local
observations and configured constraints. Self-claimed operator labels do not
create independence, trust, or authority evidence.

### 6.4 Connection Planner

Forms and prioritizes candidate pairs. It performs direct checks, coordinated
hole punching, relay selection, and transport fallback within fixed deadlines
and resource budgets.

### 6.5 Secure Session Adapter

Presents the chosen carrier's authenticated exporter or equivalent transcript
binding to the frozen OBP handshake. Carrier replacement must produce a new
binding and reauthenticate the expected peer before traffic resumes.

### 6.6 Route Journal

Stores bounded local observations: attempted candidates, failure reasons,
selected path, latency, timestamps, limitations, and authenticated outcomes.
It is rebuildable operational state, not a global reputation or truth system.

### 6.7 Platform Runtime Adapter

Translates an OS/browser execution grant into a bounded capability snapshot,
opens only outbound carriers permitted by that grant, reports network epochs,
and persists durable resume state through the platform storage boundary. It
cannot elevate a relay, address, push hint, background callback, or platform
credential into OBP authority.

The adapter must support process loss at every await point. Mobile suspension,
desktop sleep, browser-tab eviction, service restart, and network handover all
converge on the same portable recovery path: invalidate ephemeral carriers,
retain durable intent/checkpoint state, gather a new capability snapshot, and
reauthenticate before resuming.

### 6.8 Community Relay Service

The relay is a standalone, self-hostable, non-authoritative service. It may be
run by a person, cooperative, institution, or commercial provider. It forwards
bounded opaque encrypted frames and signed control objects; it does not hold
the peer's NodeID signing key, decrypt application content, decide authorship,
or certify another relay.

Relay discovery has no global membership list. Removal from one directory
does not revoke a relay, and inclusion does not approve it. Local policy may
deny or prefer relays for cost, jurisdiction, capacity, or measured quality
without changing protocol authority.

### 6.9 Intermittent-node store-and-forward lane

A sleeping phone, suspended app, closed browser tab, or powered-off laptop
cannot maintain a live session. OBP records that state honestly and keeps the
sender's work as a durable outbound intent. It never converts the absence of a
live target reservation into a promise of immediate delivery.

An optional community relay may additionally expose a bounded SeedInbox-style
mailbox. The sender uploads only recipient-bound end-to-end ciphertext with an
expiry, size class, dedupe key, and opaque retrieval token. The recipient
polls outbound when its platform grants execution, verifies and decrypts
locally, and then resumes the normal authenticated OBP protocol. The relay
cannot author, approve, decrypt, execute, or mark application work complete.

Mailbox retention, quotas, deletion receipts, metadata exposure, and abuse
policy are independently gated. A live relay carrier works without this lane;
this architecture therefore supports foreground peer sessions now without
silently claiming that `MOB-GATE-CARRIER` is already complete.

## 7. Connection flow

1. On startup, the node performs local operation without waiting for network
   reachability.
2. When its current platform grant permits, it gathers only the candidates the
   adapter can truthfully observe and creates outbound reservations at diverse
   relays. No step asks the user or router to open a port.
3. It publishes a short-lived signed reachability advertisement.
4. An initiator resolves the target's signed advertisement and builds a local
   route plan.
5. The peers test direct candidates in a bounded order only when both platform
   capability snapshots permit that path.
6. If permitted, a selected relay coordinates simultaneous UDP hole punching.
7. If direct work is unsupported, denied, or unsuccessful, both peers use an
   existing outbound relay reservation without treating direct failure as an
   error requiring operator intervention.
8. Native nodes prefer QUIC/UDP relay transport and require TLS/TCP 443
   fallback when UDP is unavailable. Browser adapters use WebTransport when
   available and otherwise a secure WebSocket framed carrier. All carry the
   same opaque inner OBP session and authority rules.
9. The endpoints run the frozen authenticated-session handshake through the
   selected carrier.
10. No application traffic is accepted until the expected NodeID and complete
    transcript verify.
11. A relayed session may continue bounded direct checks and migrate to a
    direct path only after re-binding and reauthentication.
12. Network changes, mobile suspend/resume, or relay failure trigger a bounded
    replan; they do not change NodeID or session authority.
13. If the target has no live reservation, the sender persists a durable
    outbound intent. An enabled mailbox lane may carry only opaque expiring
    ciphertext; otherwise work waits locally until a later reachable epoch.

## 8. Privacy

Public discovery may contain only NodeID, relay references, minimal capability
ceilings, explicitly public candidates, sequence, and expiry.

The following are excluded from public DHT records and public receipts:

- LAN addresses and host candidates;
- Wi-Fi/mobile interface addresses;
- SSIDs, carrier identity, router identity, or private topology;
- candidate attempt history; and
- unrelated peer or session lists.

Private candidates are exchanged only inside a target-specific authenticated
signaling context. Relay operators may still observe endpoint IPs, timing, and
traffic volume. The design does not claim protection against a global traffic
observer.

## 9. Malicious relay and discovery defenses

A relay can drop, delay, reorder, duplicate, rate-limit, censor, or observe
traffic metadata. It cannot be allowed to:

- forge another relay or target NodeID;
- alter payload bytes without detection;
- terminate the inner OBP authentication as the target;
- create a successful route receipt for the target; or
- gain content, actor, feed, policy, or application authority.

Controls include self-certifying relay identity, dual-signed reservations,
target-signed advertisements, transcript-bound peer authentication, replay
guards, short expiry, sequence checks, route diversity, bounded candidate
sets, liveness probes, timeouts, circuit breakers, and no successful outcome
before a peer-authenticated receipt.

Discovery poisoning may add junk candidates but cannot impersonate a known
identity. Candidate count, bytes, parsing depth, signature work, probe
concurrency, retry count, and per-source contribution are all hard bounded.

## 10. Failure semantics

The route state machine is:

```text
Discovering
  -> DirectChecking
  -> HolePunching
  -> RelayConnecting
  -> PeerAuthenticating
  -> Connected
```

Required typed failures include:

- `NoBootstrapReachable`
- `CandidateExpired`
- `DirectTimeout`
- `HolePunchFailed`
- `RelayDenied`
- `RelayUnavailable`
- `PeerIdentityMismatch`
- `NetworkChanged`
- `BudgetExceeded`

Direct failure is not terminal while an admitted relay route remains. Relay
failure stops new writes on the old carrier, selects a reserved alternate,
reauthenticates the peer, and resumes only from an acknowledged durable
sequence/checkpoint. Duplicate and replayed application work remain rejected.

When no path succeeds within budget, the result is `PathLimited` with exact
attempt scope and limitations. It must not claim that the peer is offline,
absent, unreachable globally, or permanently unavailable.

## 11. Resource policy

All nodes, including low-end and mobile nodes, use bounded defaults:

- finite discovery records and bytes per source;
- finite concurrent connectivity checks;
- finite relay reservations;
- paced keepalive and probe intervals;
- fixed attempt/deadline budgets;
- OS-aware background and battery policy; and
- explicit bandwidth and storage ceilings.

Profiles have a conservative shared ceiling and may only reduce work for a
device class or active OS grant. A phone or browser is not required to match a
server's reservation count, probe concurrency, keepalive cadence, or uptime.
Low-resource nodes remain interoperable because peers advertise capability
ceilings and route around unavailable work rather than assuming identical
hardware.

Relay operators may publish capacity and rate policies. The client treats
these as service policy, not trust. V1 does not require a payment or token
system.

## 12. Verification matrix

### 12.1 Deterministic and namespace tests

- full-cone, restricted, port-restricted, symmetric NAT, and CGNAT;
- public-IP node with upstream inbound UDP filtering;
- UDP blocked with TLS/TCP 443 available;
- malformed, duplicated, oversized, expired, replayed, and unknown-field
  objects;
- wrong relay key, wrong target key, wrong NodeID, route substitution, and
  transcript substitution;
- bounded descriptor Sybil flood;
- relay drop, delay, duplicate, reorder, and mid-session shutdown;
- all current relays down while another discovery path remains;
- all bootstrap paths unavailable with an honest explicit limitation;
- interface change, Wi-Fi/4G transition, suspend/resume, and address churn;
- privacy scan proving private candidates do not enter public records; and
- direct-to-relay and relay-to-direct migration with reauthentication.

The portable conformance suite must run the same canonical vectors and route
state-machine tests on Linux, Windows, macOS, Android, iOS, and browser/WASM
build lanes. Platform-specific integration evidence additionally proves:

- Windows and macOS work without Linux service/firewall commands;
- Android/iOS process loss and network handover resume from durable intent;
- denial of local-network/background permission leaves the relay path usable;
- a browser with no raw UDP or inbound listener completes an authenticated
  session over its web-compatible relay carrier; and
- a sleeping/mobile-evicted target yields durable pending intent, while an
  enabled mailbox relay proves ciphertext-only bounded poll/resume and a
  disabled mailbox makes no delivery claim; and
- every lane rejects carrier/relay substitution before peer authentication.

### 12.2 Three-host P5 acceptance

The current physical topology is an intentional real-world case:

- runner-a is outbound-only behind a provider mapping whose UDP return path is
  not usable as a stable public carrier;
- runner-b has a directly assigned public IP but upstream inbound UDP is not
  observable at the guest interface;
- runner-c is independently hosted and networked.

P5 must use the production Reachability Manager and complete an authenticated
three-node ring over outbound-created, independently authenticated relay
carriers. It must not require any ordinary node to expose or configure an
inbound public port. It must then disable the selected relay and demonstrate
authenticated failover to a reserved alternate from a durable checkpoint.
Direct and hole-punched carriers remain capability-gated optimizations for
platforms and networks that can prove them; their absence does not disqualify
this outbound-first reference topology.

Every receipt records the exact path kind: `direct`, `hole-punched`,
`relay-udp`, or `relay-tcp-443`. Local simulation, `socat`, WireGuard, a
handcrafted receipt, or an observe-only fault command cannot satisfy the
multi-host qualification gate.

Linux/P5 is the first real-network reference lane. It qualifies the shared
wire objects, relay service, failure semantics, and reference native adapter;
it does not qualify Windows, macOS, Android, iOS, or browser lifecycle
behavior by implication.

### 12.3 Platform qualification and honest claims

One successful platform lane cannot promote another. Release evidence records
separate status for:

- portable protocol/core conformance;
- native desktop adapters: Linux, Windows, and macOS;
- mobile adapters: Android and iOS/iPadOS;
- browser/WASM adapter; and
- self-hosted relay service targets.

An unexecuted lane is `not-qualified` or `pending`, never silently inherited
from Linux. This separation lets the shared framework ship and improve
incrementally without claiming that every optional platform profile has
already completed every soak or physical-device test.

## 13. Implementation decomposition

Implementation is sequenced so no incomplete layer can claim production
reachability:

1. canonical signed objects and validators;
2. bounded discovery and bootstrap inputs;
3. outbound reservation and relay data plane;
4. candidate gathering, connectivity checks, and planner;
5. secure-session carrier adapter and path migration;
6. portable carrier capability interface plus native and web adapter
   conformance harnesses;
7. Linux P5 signed command/receipt integration and real three-host reference
   qualification;
8. Windows and macOS native adapter qualification;
9. Android/iOS low-resource lifecycle integration under the separate mobile
   build contract; and
10. browser/WASM adapter plus operational relay packaging, abuse controls,
    monitoring, and runbooks.

The first implementation milestone may prove Linux/P5 behavior, but shared
interfaces and wire objects must not encode a Linux, VPS, public-IP, or
operator-configured-port assumption.

For the mobile authority map, this design affects `MOB-00`, `MOB-FND-004`,
`MOB-NET-001`, `MOB-NET-004`, `MOB-NET-007`, `MOB-NET-009`,
`MOB-NET-010`, `MOB-SYS-003`, `MOB-SYS-004`, `MOB-SYS-008`, and
`MOB-GATE-CARRIER`. It does not define or modify any `MOB-SCR-*`,
`OBM-CMP-*`, or `OBM-PAT-*` UI contract. Mobile implementation remains gated
by its own required read set and evidence contract; this amendment does not by
itself close `MOB-GATE-CARRIER` or `MOB-GATE-NETWORKED-BETA`.

## 14. Standards alignment

The candidate-gathering and connectivity-check model follows the architecture
of ICE in [RFC 8445](https://www.rfc-editor.org/info/rfc8445/). Relay fallback
and authenticated allocations follow the relevant threat and operational
model of TURN in [RFC 8656](https://www.rfc-editor.org/rfc/rfc8656.html), while
OneBrain retains its own end-to-end authenticated session above the carrier.
The relay-plus-direct-upgrade structure also aligns conceptually with the
[libp2p relay and direct connection upgrade specifications](https://github.com/libp2p/specs).

These references guide interoperability and threat analysis. They do not
replace the frozen OneBrain NodeID, session, privacy, resource, or receipt
contracts.

## 15. Source-free Linux relay packaging and operations

The native Linux bundle carries both legacy P5/soak executables and the eight
outbound-first executables (`onebrain-relay`, `relay_preflight_probe`, and six
P5 V2 node/admin/signer executables). Every executable is built under the same
pinned Linux/amd64 toolchain and recorded independently by ELF/content/provenance
evidence. The two public compiled-binding probes must emit closed candidate and
toolchain JSON in both the builder and a source-free runtime container.

The bundle also carries only public relay configs and reviewed immutable-path
systemd units. Relay, node-identity, and receipt-signing private keys remain
external and owned by distinct locked service users. The node agent never reads
those keys. Three forced-command SSH principals separate pre-request relay
probing, unprivileged agent control, and the no-argument privileged admin
boundary; none offers an interactive shell or caller-selected command.

Ordinary nodes remain outbound-only on every platform and do not configure NAT
or public ports. The Linux three-host qualification uses a private
session-scoped veth override solely to avoid hairpinning into a co-resident
relay, while preserving the admitted relay NodeID/SPKI and never publishing the
private endpoint. Namespace/NAT/UFW changes are exact, reversible,
session-owned, management-safe, and qualification evidence rather than shared
portable-core semantics. Other platforms implement the same outbound/direct/
hole-punch/relay contract through their platform capability adapter and earn
separate evidence.
