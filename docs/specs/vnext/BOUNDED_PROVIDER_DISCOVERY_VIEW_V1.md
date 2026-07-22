# Bounded Provider Discovery View v1

> **Task:** `DHT-002`  
> **Status:** Complete  
> **Depends on:** `DHT-001`, `OBP-001`

## 1. Purpose

`ProviderDiscoveryView` is a local, rebuildable and deliberately sampled route
view over the canonical `ProviderLeaseMap`. It combines provider observations
from direct exchange, PEX and DHT cache without turning any transport path into
knowledge, identity or availability authority.

The view remains useful inside a disconnected island and after reunion. It is
never a source of record and never claims to enumerate every provider in a
network, component or selector scope.

## 2. Exact provider identity

The view consumes only DHT-001 leases that already passed signature,
principal, key-state-frontier, generation, retirement and local-age checks.
Entries retain the exact LeaseCID and `ProviderTuple`; two principals under the
same index cannot overwrite one another. Same-generation conflicts are still
distinct leases and remain available to the bounded selection policy.

Direct, peer-exchange and cache observations are merged by exact LeaseCID.
Their source labels record local path provenance only. Re-observation can add a
source label but cannot renew the DHT-001 first-seen lease age or amplify its
authority.

## 3. Hard bounds and deterministic sampling

Every instance has positive, explicit bounds for:

- observed LeaseCIDs;
- active leases scanned per lookup;
- entries returned per page; and
- entries per provider principal per page.

When the observation bound is full, the implementation deterministically
retains the lexicographically smallest LeaseCIDs. This is stable and bounded,
but it is not an unbiased network sample; `ObservationEvicted` exposes that
limitation. A hot index cannot force an unbounded scan or response.

The diversity cap prevents one principal, including multiple retained
same-generation conflicts, from occupying an entire page. Candidates skipped
by that cap are part of the sampled projection and are not evidence of absence
or invalidity.

## 4. Pagination and coverage

Lookup orders eligible candidates by exact LeaseCID. It emits a continuation
cursor only when another eligible candidate is known to remain inside the
current bounded scan. If the scan bound itself is reached, the response reports
`ScanBoundReached`; it does not fabricate a cursor beyond observed state.

Every response has `coverage.sampled = true`, includes local limitations, and
returns `is_globally_complete() = false`. An empty page therefore means only
that this local bounded view has no currently usable route hint.

## 5. Local liveness probes

A probe is keyed by LeaseCID and held only in local runtime state. It records
`Reachable`, `Unreachable` or `Unknown` with a caller-local monotonic tick and a
positive TTL. A fresh `Unreachable` result suppresses that route for the local
lookup; after its TTL it becomes `Unknown` and can be retried. Clock rollback is
also `Unknown`.

Probe state does not modify, retire or renew a signed lease. `Reachable` does
not prove content possession, correctness, utility or future reachability, and
`Unreachable` does not prove global absence.

## 6. Boundaries

The materialized provider view does not:

- establish a central registry, global membership or global completeness;
- judge a KU true, false, correct, incorrect or valuable;
- delete a KU, lease conflict or retirement history;
- convert path count, geography or node tier into authority;
- grant publication, execution, adoption or disclosure permission;
- create benefit, reward or OBT; or
- introduce a Core DNA Gene or execution opcode.

## 7. Executable evidence

Five tests prove:

- direct, PEX and cache paths merge by LeaseCID while multiple providers remain;
- real page overflow emits a cursor and subsequent pages do not duplicate it;
- fresh local unreachability suppresses a route, then expires to `Unknown`;
- observation, scan and page bounds constrain a hot index and disclose sampled
  coverage; and
- the per-principal diversity cap applies without arrival-order overwrite.
