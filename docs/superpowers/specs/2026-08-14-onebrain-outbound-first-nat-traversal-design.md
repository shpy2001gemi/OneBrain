# OneBrain Outbound-First NAT Traversal and Federated Relay Design

> **Date:** 2026-08-14
> **Status:** Owner-approved design; implementation planning pending
> **Scope:** Shared OBP reachability, direct-path upgrade, federated relay fallback, and P5 real-network qualification
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

## 7. Connection flow

1. On startup, the node performs local operation without waiting for network
   reachability.
2. When network policy permits, it gathers local and server-reflexive
   candidates and creates outbound reservations at diverse relays.
3. It publishes a short-lived signed reachability advertisement.
4. An initiator resolves the target's signed advertisement and builds a local
   route plan.
5. The peers test direct candidates in a bounded order.
6. If needed, a selected relay coordinates simultaneous UDP hole punching.
7. If no direct pair succeeds, both peers use an existing outbound relay
   reservation.
8. QUIC/UDP relay transport is preferred; TLS/TCP 443 framed-datagram relay is
   the required fallback when UDP is unavailable.
9. The endpoints run the frozen authenticated-session handshake through the
   selected carrier.
10. No application traffic is accepted until the expected NodeID and complete
    transcript verify.
11. A relayed session may continue bounded direct checks and migrate to a
    direct path only after re-binding and reauthentication.
12. Network changes, mobile suspend/resume, or relay failure trigger a bounded
    replan; they do not change NodeID or session authority.

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

### 12.2 Three-host P5 acceptance

The current physical topology is an intentional real-world case:

- runner-a uses a provider-assigned external UDP port mapped to an internal
  bind port;
- runner-b has a directly assigned public IP but upstream inbound UDP is not
  observable at the guest interface;
- runner-c is independently hosted and networked.

P5 must use the production Reachability Manager and complete an authenticated
three-node QUIC ring over a mixed direct/relay plan. It must then disable the
selected relay and demonstrate authenticated failover to a reserved alternate
from a durable checkpoint.

Every receipt records the exact path kind: `direct`, `hole-punched`,
`relay-udp`, or `relay-tcp-443`. Local simulation, `socat`, WireGuard, a
handcrafted receipt, or an observe-only fault command cannot satisfy the
multi-host qualification gate.

## 13. Implementation decomposition

Implementation is sequenced so no incomplete layer can claim production
reachability:

1. canonical signed objects and validators;
2. bounded discovery and bootstrap inputs;
3. outbound reservation and relay data plane;
4. candidate gathering, connectivity checks, and planner;
5. secure-session carrier adapter and path migration;
6. P5 signed command/receipt integration and real three-host qualification;
7. low-resource/mobile lifecycle integration under the separate mobile build
   contract; and
8. operational relay packaging, abuse controls, monitoring, and runbooks.

The first implementation milestone may prove Linux/P5 behavior, but shared
interfaces and wire objects must not encode a Linux, VPS, public-IP, or
operator-configured-port assumption.

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
