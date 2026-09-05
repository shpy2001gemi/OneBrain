# OBP-PROD-003 — Bootstrap, discovery and reservation orchestration

> State: Planned
> Branch: `codex/obp-prod-003-discovery`
> Depends on: `OBP-PROD-002` merged

## Objective

Connect trusted-local bootstrap configuration to the existing signed
rendezvous, peer exchange, discovery cache, DHT and relay-reservation
components so a normal node can learn additional paths and cease depending on
its initial seed source.

## Required read set

- `../README.md`, `../DECISIONS.md`, `../CAPABILITY_STATUS.md`, `../PROGRESS.md`
- the accepted `OBP-PROD-001` contract and `OBP-PROD-002` lifecycle evidence
- `../../../specs/vnext/OUTBOUND_FIRST_REACHABILITY_PROFILE_V1.md`
- `../../../specs/vnext/ROUTE_AUTHORITY_BOUNDARY_PROFILE_V1.md`
- outbound-first design sections for bootstrap, rendezvous, PEX and cache
- current `ku-net` discovery, signed-manifest, reservation and advertisement
  implementation files

## Deliverable

- DNS/IP relay endpoints, signed bootstrap manifests and manual invitations as
  explicit trusted-local inputs.
- Validation, deduplication, expiry and refresh for discovery sources.
- Reservation establishment/renewal and signed advertisement publication.
- Learned-source persistence and source-health status.
- Tests proving discovery continues from other learned/approved sources after
  the initial seed source disappears.

## Acceptance

- Raw socket address or DNS ownership grants no NodeID/content authority.
- Peer identity is accepted only after the required cryptographic handshake.
- Expired, invalid or policy-disallowed advertisements are not used.
- Source loss is reported without erasing valid learned state.
- A new node can bootstrap from DNS or IP and then discover another source.
- Applicable validators and focused tests pass.

## Excluded

Automatic outbox routing, product API/UI, global discovery claims and mobile.
