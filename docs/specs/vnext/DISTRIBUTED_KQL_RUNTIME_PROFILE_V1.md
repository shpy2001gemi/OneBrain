# OneBrain vNext — Distributed KQL Runtime Profile v1

> **Milestone:** M3 — read-only one-hop P2P KQL  
> **Status:** Implemented behind `vnext-network-runtime` — 2026-07-25  
> **Code:** [`onebrain-node::vnext_distributed_kql`](../../../src/onebrain-node/src/vnext_distributed_kql.rs)

## 1. Implemented boundary

The M3 runtime lets a local private `StandingNeed` react to a validated Public
`KnowledgeAffordance` received from an authenticated direct peer. It does not
send raw KQL, `KnowledgeNeedIR`, `QueryDefinitionCID`, `StandingNeedID`,
Receptor identity or private goal context to that peer.

M3 is an affordance-reconciliation flow, not remote query execution:

```text
peer B Public KnowledgeAffordance
    -> authenticated QUIC + OBP-RP
    -> canonical and typed validation on peer A
    -> durable peer/selector provenance
    -> bounded local ReunionFrontier join
    -> private, quarantined BindingProposal
```

Active route sketches, encrypted multipath replies, DHT discovery and remote
`WATCH` remain M6 work.

## 2. Admission and provenance

An object can enter the M3 join only when all of these conditions hold:

- OBP-RP delivered it within an authenticated session;
- its generic object envelope, disclosure, full CID and canonical bytes passed
  the Public Store boundary;
- object kind `KnowledgeAffordance/1` passed the complete typed decoder and
  canonical round trip;
- the runtime has a durable source observation for the exact
  `(record kind, CID, SelectorCID, authenticated NodeID)` tuple.

Typed-invalid affordances are quarantined at admission. A malformed affordance
left by an older binary is ignored as one isolated branch and cannot stop
unrelated local matches.

Source observations record transport provenance only. They do not prove
authorship, semantic truth, authority, benefit, reward or network completion.

## 3. Local and durable state

The runtime persists:

- Public validated object bytes and selector-scoped inventory;
- authenticated source observations by exact peer and selector;
- the exact `LOCAL_ONLY` `QueryDefinition` and `LocalNeedTarget` bundle in the
  caller-keyed [Private KQL Vault](PRIVATE_KQL_PERSISTENCE_PROFILE_V1.md);
- an idempotent match record keyed by
  `(StandingNeedID, BindingProposalID)` and bound to the exact affordance CID,
  selector and local source frontier.

At startup the runtime authenticates and validates every bounded vault record,
then automatically rehydrates only active targets. Paused targets remain
encrypted and inactive. Canceled or retired records are terminal tombstones
and cannot be resurrected by stale target replay. Wrong-key, tampered, legacy
plaintext or inconsistent state fails closed before any target becomes active.

## 4. Matching and result semantics

`process_one_hop_affordance_delta`:

1. selects local active targets for one exact selector;
2. scans only validated Public objects with authenticated provenance under that
   selector;
3. applies explicit affordance budgets and the bounded `ReunionFrontier`;
4. uses exact typed matching;
5. returns object/proposal references, authenticated responder scope, selector,
   assessed local frontier and replay/new status.

Every result reports `PARTIAL` coverage with `PATH_LIMITED`. Budget deferral
also reports `BUDGET_EXHAUSTED` and a continuation token. Zero results and an
offline peer remain partial; neither is converted into a global negative
answer.

The runtime exposes no materialize or adopt operation. Its proposal store is
non-executable, and report fields explicitly refuse automatic materialization,
automatic adoption and network-completion claims.

## 5. Executable evidence

The real two-runtime integration test
`two_real_peers_create_one_private_match_and_restart_does_not_duplicate_it`
demonstrates:

- two independent node identities and data directories over real QUIC/OBP-RP;
- one Public affordance producing exactly one private match;
- durable receipt and exact authenticated peer/selector provenance;
- bounded scans advancing past the first CID without pagination starvation;
- no raw KQL, private QueryDefinitionCID or StandingNeedID in the exact
  application payload, and no private semantic context in that payload;
- zero results remaining partial after the source peer goes offline;
- receiver and KQL-runtime restart automatically restoring the exact target,
  same need and match count without caller re-registration;
- replay rebuilding one quarantined proposal without duplicate durable state;
- no automatic materialization, adoption or network-completion claim.

Additional unit tests cover typed affordance round-trip validation,
typed-invalid quarantine, and restart-safe selector-scoped source provenance.
The P1.4 focused suite additionally covers encrypted-at-rest target recovery,
wrong keys, tamper, deterministic local-intent commitment, durable
pause/resume, and terminal cancel/retire tombstones.
