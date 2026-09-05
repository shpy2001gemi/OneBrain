# Decisions and claim boundary

> Authority: owner-approved workstream direction, 2026-09-05.
> This file controls planning and product wording; it does not supersede a
> frozen protocol contract or founder directive.

## D-001 — Resume KU now

KU review and local product work may start immediately. OBP is not an
architectural blocker for local KU creation, validation, persistence, search,
inspection or revision.

## D-002 — Stabilize rather than redesign OBP

Authenticated sessions, scoped inventory, deterministic reconciliation,
persisted journals, multi-carrier behavior, outbound-first routing and the
permissionless relay are treated as implemented foundation components.

Changes to their canonical wire, authority or privacy semantics require a
specific defect, incompatibility or approved contract revision. Product work
must consume these boundaries instead of inventing parallel networking rules.

## D-003 — Productize OBP in a separate lane

The remaining desktop product gap is orchestration:

- node-owned Reachability Manager lifecycle;
- trusted-local bootstrap source configuration;
- bounded discovery and refresh;
- outbound relay reservations and target advertisements;
- automatic route selection, failover and durable outbox delivery;
- product API plus CLI/Web/Desktop presentation;
- product-level two-node NAT and outage acceptance.

This work remains opt-in/default-off until its gates pass.

## D-004 — Seeder and relay terminology

Allowed wording:

> Anyone can self-host a compatible vNext relay/rendezvous service. Bootstrap
> sources are replaceable discovery hints, not identity or knowledge roots of
> trust.

Forbidden wording:

- “`onebrain-seed` is the production vNext seeder.”
- “A seed or relay cannot be malicious.”
- “Inclusion in a directory makes a relay trusted.”

`onebrain-seed` is a legacy TCP/JSON prototype. The supported vNext direction
is `onebrain-relay`, signed bootstrap manifests, signed relay/peer invitations,
authenticated PEX, rendezvous/DHT records and local cache.

## D-005 — Exact malicious-infrastructure claim

Allowed wording:

> A malicious seed, mirror or relay can deny service, return bounded junk,
> delay, drop, duplicate, reorder or censor traffic and observe available
> metadata. It cannot by itself impersonate an expected NodeID, alter accepted
> canonical content undetected, terminate the inner authenticated OBP session
> as the target, or acquire content/feed/policy/knowledge authority.

No document or UI may promise absolute Sybil prevention, global traffic-analysis
resistance or guaranteed availability.

## D-006 — Bootstrap and seed independence

A new Internet node may initially use DNS, a public IP, signed bootstrap
manifest, manual invitation, file, URL or QR to locate a first source. DNS/IP
is location only; source/relay/peer identity is verified independently.

After admission of other sources, nodes may learn routes from authenticated
PEX, rendezvous/DHT records, cached descriptors, provider leases and signed
reachability advertisements. Loss of the first bootstrap source must not alter
identity or local usefulness.

A completely new node with no address, cache, invitation, nearby peer or
reachable bootstrap path cannot discover a disconnected network. Product copy
must state that limitation honestly.

## D-007 — NAT and bidirectional connection

Ordinary nodes must not require a public IP, port forwarding, UPnP, router/NAT
changes or inbound firewall configuration. Both peers may create outbound
reservations to permissionless relays and obtain a bidirectional authenticated
OBP carrier.

The public endpoint requirement belongs to relay operators. An offline or
suspended target without a live reservation cannot promise immediate delivery;
work remains a durable pending intent. The optional ciphertext mailbox remains
a separate pending gate.

## D-008 — Shared product surfaces

CLI, local Web and Desktop must project one node-owned KU/OBP service contract.
They may have different presentation, but may not implement different authority,
consent, lifecycle, completion or error semantics.

## D-009 — Release boundary

The repository has production-reference evidence under the Base owner waiver,
not an unrestricted claim that every product/platform lane is production-ready.
Strict Base qualification, default enablement, general operator rollout,
Windows/macOS outbound-first qualification, mobile and browser lanes remain
separate decisions.

## D-010 — Merge and branch policy for this workstream

Each task uses its declared `codex/` branch. Completion means the branch is
validated and pushed with an updated handoff ledger. Merge and local branch
deletion require explicit owner instruction. Accepted tasks are merged before
dependent tasks branch, unless `MASTER_PLAN.md` explicitly permits parallel
work.
