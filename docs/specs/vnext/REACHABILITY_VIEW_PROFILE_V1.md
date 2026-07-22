# OneBrain vNext — Reachability View Profile v1

> **Task:** `OBP-001`  
> **Status:** Executable local derived-view contract — frozen 2026-07-20  
> **Code:** [`ku-net::vnext_reachability`](../../../src/ku-net/src/vnext_reachability.rs)

## 1. Local observation, not network identity

`ReachabilityView` is rebuilt from locally authenticated session observations.
It contains full peer NodeIDs/session digest, SelectorCID inventory roots and
event frontiers, per-peer budgets, carrier paths, observation intervals,
limitations and optional rendezvous hints.

There is no IslandID, component ID, IslandEpoch, leader, membership authority or
claim that the node knows the complete connected component. The view exposes
`has_global_component_knowledge() == false`.

## 2. Derived display mode

The display mode is computed locally:

- `Standalone`: no authenticated peer observation;
- `ComponentReachable`: at least one authenticated peer and bidirectional path;
- `PathLimited`: authenticated peer observations exist, but only one-way/delayed
  paths are currently observed.

These modes are not protocol entities or authority states. Observation order
does not change the peer digest.

## 3. Offline-first behavior

Every view, including `Standalone` and `PathLimited`, returns
`can_encode_query_and_use_locally() == true`. Reachability may affect which
remote work is attempted; it never gates the local KU/KQL/Assembly loop.

Selector observations contain only selector/root/frontier/budget boundaries,
not full private KnowledgeNeedIR or StandingNeed state.

## 4. Carriers and limitations

Each path binds a local opaque path commitment, negotiated CarrierProfile and
observation interval. One-way, store-carry-forward, unknown frontier, bounded
budget, absent peer and absent seed are explicit limitations rather than false
completion or disconnection authority.

Budgets remain per peer; the view does not combine them into invented global
capacity.

## 5. Seed semantics

`SeedRendezvousHint` is an untrusted address commitment plus local observation
tick. It grants no authority. Adding/removing a seed hint does not change peer
digest, authenticated sessions or derived reachability mode. Seed outage is a
limitation only.

## 6. Executable evidence

Tests cover standalone usefulness, authenticated LAN reachability, explicit
one-way/store-carry limitations, observation-order stability, seed outage/hint
authority invariance and selector-only privacy boundary.

