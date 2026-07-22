# Disclosure Policy and Sanitizer v1

> **Task:** `SEC-001`  
> **Status:** Complete  
> **Depends on:** `FND-008`, `KQL-001`, `KU-002`

## 1. Purpose

This profile keeps a private need private by default and defines the only typed
projections that may cross a local-node boundary. It is a data-minimization
contract, not a promise of anonymity, truth, network reachability or global
privacy.

`ku-kql::vnext_disclosure` owns four disclosure modes:

- `LOCAL_ONLY`: the default; no network projection;
- `ROUTE_MINIMAL`: one allowlisted coarse routing token;
- `NEGOTIATED_ENCRYPTED`: an authorized mode whose capsule protocol is owned by
  `SEC-003`; and
- `PUBLIC_PROBLEM`: a new sanitized public object, distinct from the private
  source and private NeedIR.

Every non-local mode must be enabled by local policy and accompanied by an
unexpired explicit or standing consent bound to the exact policy, mode, purpose
and scope commitment. Absence, expiry or mismatch fails closed.

## 2. Local taint boundary

The sanitizer inventories the following taints before projection:

- raw text and private source references;
- stable Receptor, Assembly, Need, User and Node identifiers;
- exact literals/ranges and exact graph conjunctions;
- rare concepts, location/time, hypotheses, acceptance tests and private
  context.

The inventory and commitment to its private input are local audit state. They
are never serialized into a route sketch or referenced by a public problem
object. A commitment to private material is not treated as permission to reveal
that material.

## 3. Route-Minimal sanitizer

The route projection contains only a typed `CoarseRouteToken` and an estimated
support value consumed by the existing disclosure compiler. It contains no raw
KQL, exact conjunction, stable identity, private reference or exact literal.

The v1 heuristic requires locally estimated support of at least `64`. An exact
candidate below this threshold is replaced by the nearest deterministic,
allowlisted ontology ancestor that reaches the threshold. If no such ancestor
exists, transmission is suppressed. Ties are resolved by ontology distance,
token class and allowlisted code so replay is deterministic.

The support estimate is a local heuristic. It is not k-anonymity, cannot defeat
Sybil observations, and does not guarantee unlinkability in a small or isolated
partition. Packet shaping, reply-key separation and replay protection are
specified by `SEC-002`.

## 4. Public-Problem sanitizer

`sanitized-public-problem` is generic object kind `18`. Its canonical payload
contains only:

- a supported coarse problem Concept CCID;
- supported coarse role Concept CCIDs;
- bucketed constraint classes;
- the disclosure-policy reference and consent commitment; and
- public limitation Concept CCIDs.

A rare concept must be replaced by the nearest supported ontology ancestor or
the operation fails closed. The public object never contains or references the
private source, private NeedIR, raw text, stable identity, exact acceptance test
or local taint audit. It is a new immutable public formulation, not a redacted
view with a reversible pointer to the private object.

## 5. Boundaries

Successful sanitization does not:

- prove anonymity or eliminate traffic-analysis risk;
- authorize a later encrypted disclosure beyond its purpose/TTL permit;
- publish, adopt or materialize knowledge automatically;
- classify a KU as true, false or wrong;
- create benefit, reward or OBT; or
- introduce a Core DNA Gene or execution opcode.

Unknown support or unavailable generalization fails closed for non-local
projection; it is not interpreted as a false query or a wrong KU.

## 6. Executable evidence

Tests prove:

- `LOCAL_ONLY` is the default and every non-local mode needs scoped consent;
- rare routing values are generalized to support `>=64` or suppressed;
- route-network bytes contain none of the supplied raw text, stable IDs,
  private references or exact literals; and
- the public object contains no private reference, raw text or rare exact CCID.
