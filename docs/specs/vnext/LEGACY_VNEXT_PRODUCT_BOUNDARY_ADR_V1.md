# ADR — Legacy and vNext Product Boundaries v1

> Status: Accepted  
> Version: 1.0  
> Date: 2026-07-25  
> Scope: P0 capability truth before M4.5 product integration

## Decision

Legacy KQL, DHT, PoMV, and OBT remain compatibility subsystems. They are not
aliases, fallbacks, or implicit implementations of vNext distributed
capabilities.

All vNext product integration is additive, default-off, and exposed under
explicit `/api/vnext/...` contracts after the relevant exit gate. Existing
legacy endpoint meanings do not change silently.

## Frozen boundary matrix

| Surface/capability | Legacy meaning retained | vNext meaning | Forbidden bridge |
|---|---|---|---|
| `/api/kql`, local CLI query | Local parser and local KU scan | Private local Need plus bounded one-hop discovery under a future additive endpoint | Raw KQL, StandingNeed ID, private target, or private context leaving the origin |
| Legacy DHT/WATCH | Existing compatibility discovery/state | Authenticated, selector-scoped, bounded availability hints after M6A | Treating DHT/provider count as truth, authority, rank, reward, or global completeness |
| `pomv`, `pomv_breakdown`, legacy PoMV UI | `legacy_local_pomv_scalar_v1`, a local compatibility scalar | Policy/frontier-relative Metabolic Evidence View with lineage, conflicts, limitations, and no reward semantics | Relabeling the scalar as vNext evidence, Outcome, Benefit, authorization, or economic value |
| Wallet/balance/history | `simulated_non_economic`, derived from local KU count; staking fenced | No production wallet before the M7 ledger/finality exit gate | Presenting placeholder values as spendable, settled, transferable, minted, or authoritative OBT |
| Legacy OBT/reward modules | Library/prototype code and compatibility research evidence | Isolated reward firewall, Benefit-based policy, durable ledger and finality only after M7 | Minting from encode, store, query, retrieval, citation, UseEvidence, one Benefit event, or one peer observation |

## Product rules

1. `/api/kql` and `/api/watch` remain local legacy surfaces.
2. `pomv` and `pomv_breakdown` remain serialized for compatibility, but their
   DTOs include `pomv_profile = "legacy_local_pomv_scalar_v1"` and
   `pomv_is_economic = false`.
3. Wallet DTOs include
   `economic_status = "simulated_non_economic"` and explicit limitations.
4. Stake and unstake reject while the wallet is simulated.
5. UI and CLI surfaces must say `legacy`, `simulated`, or `non-economic`
   adjacent to these values; a distant documentation disclaimer is
   insufficient.
6. vNext Public UseEvidence and Metabolic Evidence View never mutate wallet or
   OBT state.
7. A compatibility adapter may preserve exact legacy bytes and provenance, but
   cannot invent vNext identity, authority, consent, coverage, or fidelity.

## Feature and rollback policy

- vNext network and product lanes default to disabled.
- Distributed KQL, Public UseEvidence publication, and distributed metabolic
  view require independent kill switches.
- Disabling a vNext lane must not disable local KQL, local storage, or offline
  knowledge use.
- Rollback preserves raw accepted records, journals, pending outbox entries,
  quarantine, and private Vault state.

## Acceptance checks

- Default anti-gaming policy uses production constants.
- Development admission policy cannot change cooldown, mint, consensus, or
  reward limits.
- API JSON exposes the legacy PoMV profile and non-economic wallet status.
- CLI and Web do not display a placeholder balance as real OBT.
- Stake/unstake fail closed before the M7 gate.
- Legacy endpoint compatibility tests remain green.

## Consequences

The product may temporarily retain legacy fields and pages, but capability
truth is explicit and machine-readable. M4.5 can add typed vNext services
without changing legacy semantics or coupling the knowledge plane to the
economic plane.
