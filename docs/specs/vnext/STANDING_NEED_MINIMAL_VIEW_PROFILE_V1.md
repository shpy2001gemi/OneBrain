# OneBrain vNext — Standing Need and Minimal View Profile v1

> **Task:** `KQL-005`  
> **Status:** Normative local-state contract — frozen 2026-07-20  
> **Code:** [`ku-kql::vnext_standing_need`](../../../src/ku-kql/src/vnext_standing_need.rs)

## 1. Standalone StandingNeed

A StandingNeed is durable local watch state over one private QueryDefinition and
one Selector. It references a ReceptorDefinition directly and does not require
an Assembly, placement or resolution event. Its canonical identity includes:

- full receptor-definition reference;
- private QueryDefinitionCID;
- SelectorCID;
- watch-policy reference;
- native or legacy-watch-import origin.

Generation, active/paused/retired state and observed frontier are mutable local
record state outside the stable StandingNeedID. Disclosure is unconditionally
`LOCAL_ONLY`; there is no public encoder or outbound inventory class.

## 2. Durable generation store

`StandingNeedStore` validates and stores exact canonical bytes through a backend
generation contract:

| Existing state | Incoming | Outcome |
|---|---|---|
| absent | generation `g` | stored |
| same generation, same bytes | exact retry | replay |
| same generation, different bytes | conflict, no overwrite |
| higher generation | update |
| lower generation | stale, no overwrite |

The deterministic memory backend provides unit conformance. The redb backend
commits each generation transactionally and is tested by closing/reopening the
database and loading the same canonical StandingNeed.

Legacy watch import creates a new local v1 record with explicit
`LEGACY_WATCH_IMPORT` origin. It never invents a public need or publishes legacy
watch bytes.

## 3. Minimal ReceptorView and MappingView

`MinimalKnowledgeViews` is a disposable projection over:

- canonical StandingNeeds;
- canonical `ResolutionView` outputs already produced by the KU Resolution
  reducer;
- durable Mapping view records.

It does not consume raw resolution events and therefore cannot become a second
resolution reducer. The snapshot records its own reducer version and the exact
`RESOLUTION_REDUCER_VERSION` it accepts. Receptor postings expose standing need
IDs and scoped resolution states; Mapping postings expose durable envelope refs
and adopted Assembly targets.

## 4. Rebuild semantics

Source and projection roots are deterministic over full-width identities.
Deleting the views loses no StandingNeed, Mapping or event bytes. Rebuilding
from the same canonical sources after restart/partition produces the same roots
and ordered postings.

No view field grants authority, establishes truth or silently materializes a
proposal. Adoption counts are derived from Resolution branches only.

## 5. Acceptance evidence

- A standalone need round-trips without any Assembly identity.
- Memory store preserves exact reload and rejects stale/same-generation
  conflicts.
- Redb close/reopen returns the same need.
- Legacy import is always local-only.
- Rebuilding minimal views returns identical roots, one standing need, one
  scoped resolution and one adopted Mapping target using the canonical
  Resolution reducer version.
