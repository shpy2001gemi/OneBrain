# OBP-PROD-001 contract proposal and evidence

2026-09-20 — Review. Continued after the completed local KU-CLI-001 checks under
D-024, without asking again to change tasks. The original working tree and every
prior uncommitted change were carried into `codex/obp-prod-001-product-contract`.
No commit exists separating these local tasks; HEAD remains
`467e2ad3ada305597b2d1808c03da6190d0643cb`.

The [owner-reviewable profile](../../../specs/vnext/OBP_PRODUCT_ORCHESTRATION_PROFILE_V1.md)
specifies composition of the existing Reachability Manager, discovery,
reservation, authenticated route directory, durable outbox and checkpoints under
one node-owned aggregate. The [machine inventory](../../../../src/test-vectors/vnext/obp-product-orchestration-v1.json)
defines 13 logical operations, 18 bounded DTOs and 26 positive/negative fixtures.

The proposal includes trusted-local bootstrap/DNS/IP/manual admission, source
refresh/expiry, reservation and advertisement states, capability-aware routing,
expected-peer authentication, durable acknowledgement, explicit retry/failover,
redaction, default-off configuration and existing network generation kill/re-enable.
It preserves all frozen object IDs/domains, path kinds and resource ceilings.
Seven normalized UTF-8/LF authority-input SHA-256 pins detect unintended drift;
these are checker pins, not new canonical identity or signature domains.

Concrete reuse points inspected:

- `ConfiguredBootstrapSource`, sealed dial tokens and HTTPS bootstrap client;
- `RelayDiscovery`, live PEX lease and signed manual invitations;
- `ReachabilityManager`, `RelayReservationManager` and shared QUIC transport;
- `AuthenticatedRouteDirectory` and expected-peer runtime connections;
- `OutboundTransferIntent`, existing four outbox states and atomic
  `apply_receipt_and_checkpoint`;
- aggregate worker ceiling, lifecycle ordering and durable rollout generations.

No networking runtime, API handler, CLI network command or mobile file was changed.
No model experiment, live relay call, host activation, commit, push or merge occurred.

## Checks

| Command | Result |
|---|---|
| `python -m unittest scripts.ci.test_validate_obp_product_contract -v` | Six tests pass, including mutation rejection and all adversarial fixtures. |
| `python scripts/ci/validate_obp_product_contract.py` | 13 operations / 18 DTOs / 26 fixtures pass; owner review explicitly remains required. |
| `python scripts/ci/validate_vnext_contracts.py` | Pass, now including the OBP proposal checker. |
| `git diff --check` | Pass. |

An initial checker run detected inconsistent Windows default text encoding in
newly generated authority pins. Generation was corrected to explicit UTF-8/LF;
none of the pinned authority documents changed.

## Review boundary

This is a proposal, not an accepted freeze or runtime evidence. Review should
cover the local configuration/advertisement separation, 13 logical operations,
redacted status DTOs, bounded refresh/backoff and reuse of the existing network
generation. REST route/failure binding and host intake transport remain explicit
OBP-API-001 prerequisites; no endpoint is allocated by this proposal.

NEXT_CONVERSATION.md requires: “New public contracts still require their
documented review before implementation.” OBP-PROD-002 also depends on
OBP-PROD-001 merged. Those are the reasons runtime work has not started. The
owner already approved moving to this task; that direction needs no repeat
approval. Contract acceptance and any Git publication are separate decisions.

## Owner acceptance — 2026-09-20

D-025 supersedes the historical review boundary above. The owner accepted the
concrete proposal and explicitly authorized OBP-PROD-002 local implementation
before merge. Machine metadata and review-gate mutations now require that
accepted decision. This is not Git publication or production activation.
