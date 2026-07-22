# OneBrain vNext — Scoped Runtime Status Profile v1

> **Task:** `RUN-004`  
> **Status:** Complete  
> **Code:** [`onebrain-node::vnext_status`](../../../src/onebrain-node/src/vnext_status.rs)  
> **Surfaces:** REST `/api/status`, CLI `status`, Web `NetworkPage`

## 1. Display boundary

`VNextStatusSnapshot` is a read-only display projection. Reading or rendering
it cannot grant authority, record consent, publish a Need, establish fidelity,
complete a selector, adopt a Mapping or change a Receptor resolution.

Status separates five questions that legacy dashboards often collapsed:

1. can this local node still work;
2. which peers or paths were actually observed;
3. what coverage and frontier were assessed;
4. what encoding-fidelity evidence exists; and
5. what legacy and consent limitations apply.

## 2. Offline-first reporting

With zero peers, the node reports `USABLE_OFFLINE`, reachability
`LOCAL_NODE`, coverage `LOCAL_ONLY` and an unavailable assessed frontier. Local
KU work remains usable; this does not imply network absence or completeness.

With observed peers, the status becomes `USABLE_WITH_OBSERVED_PEERS`, scope
`OBSERVED_PEER_SET` and coverage `PARTIAL`. Peer count never upgrades coverage
to a network-wide completion statement.

## 3. Fidelity, legacy and consent

Until a frontier-scoped fidelity assessment is supplied, fidelity is
`UNASSESSED`; it never establishes proposition truth. The serializer emits no
legacy `FULL`, `GLOBAL` or `CLOSED` status value.

Raw v1 readability and isolated legacy-adapter activity are explicit. Legacy
claims are labeled advisory and warnings explain their downgrade.

Absent consent is never inferred. Continuous observation is `NOT_CONFIGURED`;
publishing requires an explicit action; public Need disclosure and remote
cognition are `NOT_GRANTED`. A later consent subsystem must provide a named,
auditable scope before these values can change.

## 4. Surface parity and evidence

The REST response embeds the shared Rust snapshot rather than reconstructing
labels in the handler. CLI and Web consume the same fields, including
reachability, coverage, frontier, fidelity, limitations, legacy warnings and
consent.

Four Rust tests cover standalone usability, observed-peer partial scope,
fail-closed consent and forbidden legacy-finality values. The API/CLI crates
compile against the shared type and the production TypeScript/Vite build
validates the Web contract.

