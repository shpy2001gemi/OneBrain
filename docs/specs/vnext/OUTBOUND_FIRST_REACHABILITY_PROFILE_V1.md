# OneBrain Outbound-First Reachability Profile v1

> **Profile ID:** `OUTBOUND_FIRST_REACHABILITY_PROFILE_V1`
> **Machine vector:** [`../../../src/test-vectors/vnext/outbound-first-reachability-v1.json`](../../../src/test-vectors/vnext/outbound-first-reachability-v1.json)
> **Authority:** [Outbound-First NAT Traversal and Federated Relay Design](../../superpowers/specs/2026-08-14-onebrain-outbound-first-nat-traversal-design.md)

## 1. Scope

This profile freezes the portable OBP reachability boundary shared by Linux,
Windows, macOS, Android, iOS/iPadOS and browser/WASM adapters. Linux P5 is a
reference qualification lane, not protocol authority for another platform.

An ordinary node MUST be able to attempt its baseline route using only
outbound connections. Direct listen, LAN discovery and hole punching are
optional optimizations. A missing capability MUST remove only that attempt; it
MUST NOT change NodeID, session authority or canonical object meaning.

No OneBrain-operated relay, directory, DNS name, bootstrap manifest or
rendezvous service is mandatory. A user-operated relay requires no owner
approval, but every relay and endpoint MUST pass the same bounded identity,
signature, freshness, proof-of-possession and liveness checks.

## 2. Authority boundary

The existing authenticated-session profile remains the only peer-session
authority. A selected carrier, relay, address, port, NAT observation,
platform callback or discovery record MUST NOT authenticate a peer.

Every carrier replacement MUST create a fresh transport binding and repeat the
expected-NodeID handshake before application traffic resumes. A successful
route receipt requires `expected_peer == authenticated_peer`.

Relays carry opaque encrypted frames and signed control objects. They MUST NOT
decrypt application content, sign as a target, create authorship, decide
truth, grant policy, or mark application work complete.

## 3. Canonical objects and IDs

The closed reachability schema IDs are:

| ID | Object |
|---:|---|
| 40 | `BootstrapManifestV1` |
| 41 | `RelayDescriptorV1` |
| 42 | `RelayReservationV1` |
| 43 | `ReachabilityAdvertisementV1` |
| 44 | local `RoutePlanV1` |
| 45 | local `RouteReceiptV1` |

Relay-control IDs are `50..55` and `61..63`; connectivity-signaling IDs are
`56..60`, exactly as frozen in the machine vector. These IDs are protocol-local
and MUST NOT alter frozen authenticated-session wire IDs.

Known objects use canonical closed CBOR, depth 12, no duplicate/unknown fields
and byte-for-byte re-encoding. Parsers MUST enforce the object byte ceiling
before unbounded allocation and reject one byte or one element over a limit.

The NodeID in every signed relay/target object MUST derive from the claimed
Ed25519 public key. Each advertised relay endpoint requires a distinct live
proof bound to descriptor digest, endpoint index, transport, challenge and
fresh carrier connection/exporter digest.

## 4. Admission and discovery

Signed objects require explicit issue/expiry values, short validity and a
durable replay/sequence floor. Descriptor successors MUST name the exact prior
canonical digest and advance by one; gaps, forks, rollback and replay reject.

Public IP literals and every bounded DNS result MUST be global-unicast.
Resolution is repeated immediately before dialing and MUST equal the admitted
canonical address set. Loopback, private, link-local, CGNAT, multicast,
documentation and other special-use targets reject before network work.

Discovery inputs are bounded rendezvous/DHT records, authenticated peer
exchange, replaceable signed bootstrap manifests and manual signed
invitations. A source stores bytes but grants no relay authority. If every
source is unavailable, the result is `NoBootstrapReachable`, not a global
offline claim.

## 5. Outbound route state machine

The closed path kinds are:

```text
direct
hole-punched
relay-udp
relay-tcp-443
```

`direct_class={direct,hole-punched}` and
`relay_class={relay-udp,relay-tcp-443}`. WebTransport projects to
`relay-udp`; secure WebSocket framing projects to `relay-tcp-443`. Exact web
adapter transport remains private diagnostics and MUST NOT create a new
authority class.

The planner tests admitted direct work only when the current capability grant
allows it. Direct failure is nonterminal while an admitted relay remains.
Relay failure stops new writes, selects an already reserved alternate,
reauthenticates and resumes only from the exact acknowledged checkpoint.

`PathLimited` records the finite attempted scope. It MUST NOT claim that a
peer is globally offline, absent or permanently unreachable.

## 6. Platform capability boundary

The local nonserialized capabilities are:

```text
outbound-datagram
outbound-stream-443
webtransport
websocket-tls
direct-listen
lan-discovery
hole-punch
durable-resume
```

A snapshot binds monotonic observation time, network epoch, execution-grant
deadline, byte/work budgets and cancellation. It carries no OS name and MUST
NOT synthesize a candidate, reservation, receipt, peer identity or protocol
authority.

Native, stream-only, web-only, foreground-mobile, suspended-mobile and
expired/revoked-grant fixtures use the same canonical vectors. Process loss,
sleep, tab eviction and network handover invalidate ephemeral carriers while
preserving only durable intent/checkpoint state.

## 7. Privacy and intermittent nodes

Public candidates are limited to server-reflexive and explicit
provider-mapped endpoints. LAN/host candidates, interface names, SSIDs,
carrier/router identity and attempt history MUST NOT enter public discovery or
public receipts.

When a target has no live reservation, the sender persists a durable outbound
intent. An optional mailbox may store only recipient-bound end-to-end
ciphertext with bounded expiry, size and dedupe metadata. Mailbox acceptance
MUST NOT mean delivery, custody, execution or application completion.

## 8. Frozen resources and mutations

All numeric ceilings, signature domains, object fields, failure names and
mutation classes are exact in the machine vector. Implementations MUST reject
unknown/missing fields, noncanonical bytes, wrong domains, over-limit input,
invalid expiry/sequence, public private-candidate leakage, central-service or
owner-approval requirements, carrier-authority substitution, path-class
substitution and checkpoint mismatch.

The machine validator and mutation suite are the Task 1 evidence. Runtime,
namespace, mobile physical-device, browser and real multi-host evidence remain
separate gates.
