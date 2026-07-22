# OneBrain vNext — Inventory Scope Profile v1

> **Task:** `INV-001`  
> **Status:** Normative — frozen 2026-07-20  
> **Code:** [`foundation::inventory`](../../../src/ku-core/src/foundation/inventory.rs)  
> **Vector:** [`PublicKnowledgeExchangeFixture/1`](../../../src/test-vectors/vnext/inventory/public-knowledge-exchange-v1.json)

## 1. Selector identity

A Selector is a bounded, content-addressed description of what one operation is
allowed to inspect. SelectorCID uses canonical `control/1` bytes and the
domain-separated `selector/1` digest. Identity includes:

- purpose (`PUBLIC_KNOWLEDGE_EXCHANGE`, `EXACT_CID_FETCH`, `RECONCILIATION`);
- randomized namespace commitment;
- record kinds and generic object-kind IDs;
- permitted public storage classes;
- observed EventCID frontier;
- record/byte/work/depth budget;
- carrier profile and its size/direction/store-carry-forward properties.

All collection fields are canonical sets. Duplicate members are invalid and
input order cannot change SelectorCID. The schema has no field whose Rust type
can accept local `ConceptId`, raw KQL, full KnowledgeNeedIR, user identity or
private receptor context.

## 2. Storage firewall

Network inventory selectors admit only `PUBLIC` and, where a purpose explicitly
uses it, `ROUTE_MINIMAL`. `LOCAL_ONLY` and `NEGOTIATED_ENCRYPTED` are rejected
by selector validation rather than filtered after enumeration. Private Vault
records therefore cannot enter ordinary selector inventory by configuration
mistake.

An Object record selector must name at least one generic object kind. Mapping
Kernel is an independent record kind because its typed CID is not a generic
ObjectCID.

## 3. Budget and carrier profile

Budgets are finite, non-zero and capped by profile v1. A carrier describes only
delivery constraints: carrier kind, maximum frame/bundle size, duplex support
and store-carry-forward support. Carrier choice does not grant content,
epistemic or author authority.

`SelectorOffer` binds one SelectorCID to offered budget, supported carriers and
the source frontier. It is an availability/control statement, not proof that
the source has all matching records.

## 4. Coverage statement

Every response can carry a CoverageStatement naming the exact SelectorCID and
assessed frontier. Basis is one of:

- `EXACT_INVENTORY`;
- `PROBABILISTIC_SUMMARY(false_positive_ppm)`;
- `SAMPLED`.

Status is `PARTIAL` or `COMPLETE_WITHIN_SELECTOR`. Completion is valid only for
exact inventory with no continuation and no limitation. Probabilistic, sampled,
budget-exhausted, path-limited or frontier-incomplete results cannot serialize a
completion claim.

Even an exact zero-result statement means only “zero within Selector S at
frontier F.” `is_globally_complete()` is unconditionally false; OneBrain has no
global inventory-completeness state.

## 5. Frozen fixture and evidence

`PublicKnowledgeExchangeFixture/1` fixes the public object/event/MappingKernel
selector, namespace commitment, frontier, budgets and file-bundle carrier.
Its canonical bytes and SelectorCID are stored in the vector file and asserted
by Rust tests.

- Set permutation keeps identical bytes and SelectorCID.
- Both Private Vault disclosure classes are rejected.
- Probabilistic and limited exact statements cannot claim completion.
- Exact zero result remains selector/frontier-relative and never global.
