# ADR — vNext Product Integration Contract v1

> **Status:** Accepted
> **Work package:** `DR-P1.1`
> **Date:** 2026-07-26
> **Decision owner:** Product/API boundary with security review

## Context

M2–M4 already prove bounded real-QUIC reconciliation, one-hop distributed KQL,
Public UseEvidence publication, and metabolic evidence views. Those runtimes
must not be surfaced by silently reinterpreting legacy KQL, PoMV, watch, or
wallet contracts.

Product clients also need stable representation for typed CIDs, continuation,
partial coverage, lifecycle state, quarantined proposals, consent preparation,
and fail-closed errors before P2/P3 implementation begins.

## Decision

Adopt
[`VNEXT_PRODUCT_INTEGRATION_PROFILE_V1`](VNEXT_PRODUCT_INTEGRATION_PROFILE_V1.md)
as the only product integration contract for M4.5:

1. New capability endpoints live exclusively below `/api/vnext/...`.
2. Required DTO fields and endpoint inventory are frozen in a machine-readable
   fixture.
3. REST CIDs use exact lowercase 64-character hex while their field names
   preserve the typed-ID role.
4. Continuations are versioned, opaque, context-bound base64url values.
5. Lifecycle, coverage, limitations, work state, error retryability, policy,
   frontier, and revision remain distinct.
6. Clients cannot submit authority decisions, authority frontiers, policy
   implementations, or signer secrets.
7. Proposals remain quarantined/non-executable and metabolic views remain
   non-truth, non-benefit, non-reward, and non-global.
8. Legacy endpoint and field meanings remain unchanged.

## Alternatives rejected

- Reusing `/api/kql` for one-hop discovery: rejected because it changes a
  local legacy contract and risks disclosing raw KQL/private targets.
- Serializing `[u8; 32]` as JSON integer arrays: rejected because it loses a
  compact stable product representation and encourages untyped digest mixing.
- Offset/page-number pagination: rejected because distributed/restart state is
  context-bound and cannot promise a stable global order.
- Letting API callers provide `Authorized`, a frontier, or policy code:
  rejected because request data would become self-granted authority.
- Returning a scalar PoMV score as the vNext view: rejected because it erases
  policy/frontier/revision/conflict/limitation evidence and invites reward or
  truth interpretation.

## Consequences

P2/P3 implementations must conform to the frozen fixture or introduce an
explicit profile revision. The profile adds no runtime side effect today.
Machine validation fails on namespace escape, legacy meaning drift, secret
field exposure, authority injection, executable proposals, or economic/truth
claims from metabolic evidence.
