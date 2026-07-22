# OneBrain vNext — Additive KU Workflow Surface v1

> **Task:** `RUN-002`
> **Status:** Complete
> **Code:** [`onebrain-node::vnext_workflow_surface`](../../../src/onebrain-node/src/vnext_workflow_surface.rs)
> **Surfaces:** REST `/api/vnext/workflow`, REST `/api/vnext/workflow/{stage}`, CLI `workflow [stage]`

## 1. Purpose and boundary

This additive surface gives API and CLI clients one shared description of the
six KU workflow stages: Assembly, Receptor, Discover, Proposal, Mapping and
Resolution. It is deliberately read-only. Calling it does not execute a query,
create a proposal, materialize a Mapping, adopt a Mapping, publish knowledge or
grant authority.

The surface exists so an operator or future UI can see the exact artifact
identity, scope, assumptions, violated constraints, unknown constraints,
limitations and next explicit action before crossing a durable boundary.

## 2. Stage contract

| Stage | Identity-bearing boundary | Display rule | Next explicit boundary |
|---|---|---|---|
| Assembly | lineage plus exact immutable revision | a new revision never rewrites its predecessor | select placement and inspect its Receptor Definition |
| Receptor | Definition CID plus exact placement | open relative to revision, placement, policy and frontier | create a bounded private Need or run local discovery |
| Discover | Need commitment plus selector, frontier and budget | partial within named bounds; zero results are not absence | evaluate typed constraints and emit a proposal |
| Proposal | ProposalID plus candidate Kernel CID and provenance | candidate only; unknown and violated constraints stay visible | explicitly materialize, retain or reject |
| Mapping | Kernel CID plus Envelope CID and destination | materialized relative to authorization and destination; not adopted | submit a separate signed adoption event |
| Resolution | revision, placement, policy and assessed frontier | `Satisfied relative to exact assembly revision, placement, policy and assessed frontier` | continue local use, derive, reopen or revise with a new event |

The `violated_constraints` field is present even when this contract-only view
has no evaluated candidate. An operational query fills it from exact typed
matching; absence in this view is displayed as “none declared in this contract
view”, not as proof that every constraint passed.

## 3. Authority and closure firewall

Every returned stage declares four side-effect flags. In this profile all are
false: materialization, adoption, authority grant and global closure. Proposal
and Mapping remain separate artifacts; durable materialization does not change
Resolution until an independently authorized adoption event is reduced.

Resolution wording is frontier-relative. There is no network-wide closed
state, no central completion oracle and no implication that an unreachable or
unsearched region contains no useful KU.

## 4. Evidence

Three node tests cover all six stages, exact relative-resolution wording,
unknown preservation and the proposal/materialization/adoption split. The CLI
has a stage-name coverage test, and the API and CLI compile against the same
serialized Rust type.
